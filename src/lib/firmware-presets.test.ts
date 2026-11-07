import { expect, test } from 'bun:test'
import protocol from '../../firmware/protocol.json'
import { buildAppSlot, fnv1a32, numeric, presetsFrom, readLog, readMailbox, wordAddress, type DebugProtocol, type FirmwareManifest } from './firmware-presets'

const layout = protocol as DebugProtocol
const memory = new Map<number, number>()
const read = async (address: number, length: number) => Uint8Array.from({ length }, (_, i) => memory.get(address + i) ?? 0)
const write = (address: number, bytes: Uint8Array) => bytes.forEach((byte, i) => memory.set(address + i, byte))
const u32 = (value: number) => new Uint8Array(new Uint32Array([value]).buffer)

test('reads manifest numbers and names presets by manifest and image', () => {
  expect(numeric('0x00100000')).toBe(0x100000)
  expect(numeric(42)).toBe(42)
  expect(() => numeric('nope')).toThrow()
  const manifest = { schema: 1, name: 'demo', images: { a: { file: 'a.bin', label: 'A', loadAddress: '0x1000', entry: '0x1000', bytes: 4, sha256: '' } } } satisfies FirmwareManifest
  const [preset] = presetsFrom('/local/firmware/demo.json', manifest)
  expect(preset).toMatchObject({ id: 'demo/a', label: 'demo · A', base: '/local/firmware/' })
  expect(() => presetsFrom('/x.json', { ...manifest, schema: 2 })).toThrow()
})

test('decodes mailbox words, signed words, strings and status names', async () => {
  const mailbox = layout.mailbox!
  write(numeric(mailbox.address), new TextEncoder().encode(mailbox.magic))
  write(wordAddress(mailbox, 'status'), u32(4))
  write(wordAddress(mailbox, 'actionValue'), u32(-3 >>> 0))
  write(numeric(mailbox.address) + mailbox.strings!.actionName.word * 4, new TextEncoder().encode('count\0junk'))
  const decoded = await readMailbox(mailbox, read)
  expect(decoded).toMatchObject({ valid: true, status: 'fault', stopped: true, strings: { actionName: 'count' } })
  expect(decoded.words.actionValue).toBe(-3)
})

test('reads only new log bytes, across the ring wrap', async () => {
  const log = layout.log!
  const base = numeric(log.address), data = numeric(log.dataOffset), capacity = numeric(log.size) - data
  write(base, new TextEncoder().encode(log.magic))
  const total = capacity + 6
  write(base + log.headWord * 4, u32(total))
  write(base + data + capacity - 4, new TextEncoder().encode('abcd'))
  write(base + data, new TextEncoder().encode('efghij'))
  expect(await readLog(log, read, total - 10)).toEqual({ total, text: 'abcdefghij' })
  expect(await readLog(log, read, total)).toEqual({ total, text: '' })
})

test('places the scene buffer between mailbox and app slot', () => {
  const scene = layout.scene!
  const mailbox = numeric(layout.mailbox!.address)
  const slot = numeric(layout.appSlot!.address)
  const end = numeric(scene.address) + scene.headerBytes + scene.maxWidth * scene.maxHeight * 3
  expect(numeric(scene.address)).toBeGreaterThan(mailbox)
  expect(end).toBeLessThanOrEqual(slot)
  expect(scene).toMatchObject({ magic: 'FBSC', format: 'rgb888', chroma: '#010008' })
})

test('builds the app slot the firmware parses', () => {
  const slot = layout.appSlot!
  expect(fnv1a32(new TextEncoder().encode('a'))).toBe(0xe40c292c)
  const js = new TextEncoder().encode('app()'), pack = Uint8Array.of(1, 2, 3)
  const bytes = buildAppSlot(slot, js, pack, 7)
  const header = new DataView(bytes.buffer)
  const field = (name: string) => header.getUint32(slot.header.indexOf(name) * 4, true)
  expect(new TextDecoder().decode(bytes.subarray(0, 4))).toBe(slot.magic)
  expect([field('javaScriptOffset'), field('javaScriptLength'), field('packLength'), field('generation')]).toEqual([slot.headerBytes, 5, 3, 7])
  expect(field('packOffset') % slot.packAlign).toBe(0)
  expect(field('checksum')).toBe(fnv1a32(js, pack))
  expect([...bytes.subarray(field('packOffset'))]).toEqual([1, 2, 3])
})
