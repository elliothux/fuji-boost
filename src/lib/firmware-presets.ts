// Firmware presets for the camera page. The emulator only runs the official
// DAT or a raw RAM image; everything a custom image wants the host to know
// (where to load it, its debug protocol, its apps) comes from its manifest.

/** Manifests to offer as presets; each is produced by its firmware's build. */
export const MANIFESTS = ['/local/firmware/fuji-boost.json']

type Numeric = number | string

export interface ImageEntry { file: string; label: string; loadAddress: Numeric; entry: Numeric; bytes: number; sha256: string }

export interface DebugProtocol {
  schema: number
  log?: { address: Numeric; size: Numeric; magic: string; headWord: number; dataOffset: Numeric }
  mailbox?: {
    address: Numeric; magic: string; words: string[]; signed?: string[]
    strings?: Record<string, { word: number; words: number }>
    status?: string[]; stopped?: string[]
  }
  appSlot?: {
    address: Numeric; size: Numeric; magic: string; version: number; headerBytes: number; packAlign: number
    header: string[]; checksum: 'fnv1a32'; reloadWord: string
  }
  scene?: {
    address: Numeric; maxWidth: number; maxHeight: number; format: string
    headerBytes: number; header: string[]; magic: string; chroma?: string
  }
}

export interface FirmwareManifest {
  schema: number
  name: string
  images: Record<string, ImageEntry>
  protocol?: DebugProtocol
  apps?: { index: string; image: string; embedded?: string }
}

export interface AppIndex { embedded?: string; apps: Record<string, { javaScript: string; pack: string; sha256: string }> }

export type Preset =
  | { kind: 'stock'; id: 'stock'; label: string }
  | { kind: 'image'; id: string; label: string; base: string; manifest: FirmwareManifest; image: ImageEntry; key: string }

export const STOCK: Preset = { kind: 'stock', id: 'stock', label: '原厂 2.12' }

/** Manifest numbers are JSON numbers or "0x…" strings. */
export function numeric(value: Numeric): number {
  const parsed = typeof value === 'number' ? value : Number(value)
  if (!Number.isSafeInteger(parsed) || parsed < 0) throw new Error(`Invalid manifest number ${value}`)
  return parsed
}

export function presetsFrom(url: string, manifest: FirmwareManifest): Preset[] {
  if (manifest.schema !== 1) throw new Error(`${url}: unsupported manifest schema ${manifest.schema}`)
  const base = url.slice(0, url.lastIndexOf('/') + 1)
  return Object.entries(manifest.images).map(([key, image]) => ({
    kind: 'image', id: `${manifest.name}/${key}`, label: `${manifest.name} · ${image.label}`, base, manifest, image, key,
  }))
}

type Read = (address: number, length: number) => Promise<Uint8Array>
const word = (bytes: Uint8Array, index: number) => new DataView(bytes.buffer, bytes.byteOffset).getUint32(index * 4, true)
const ascii = (text: string) => new TextEncoder().encode(text)
const magicWord = (text: string) => new DataView(ascii(text).buffer).getUint32(0, true)

export interface Mailbox { valid: boolean; words: Record<string, number>; strings: Record<string, string>; status: string; stopped: boolean }

export async function readMailbox(layout: NonNullable<DebugProtocol['mailbox']>, read: Read): Promise<Mailbox> {
  const strings = Object.entries(layout.strings ?? {})
  const count = Math.max(layout.words.length, ...strings.map(([, s]) => s.word + s.words))
  const bytes = await read(numeric(layout.address), count * 4)
  const signed = new Set(layout.signed ?? [])
  const words = Object.fromEntries(layout.words.map((name, index) => {
    const value = word(bytes, index)
    return [name, signed.has(name) ? value | 0 : value]
  }))
  const text = Object.fromEntries(strings.map(([name, { word: at, words: length }]) => {
    const raw = bytes.subarray(at * 4, (at + length) * 4)
    const end = raw.indexOf(0)
    return [name, new TextDecoder().decode(end < 0 ? raw : raw.subarray(0, end))]
  }))
  const status = layout.status?.[words.status] ?? String(words.status ?? '—')
  return { valid: word(bytes, 0) === magicWord(layout.magic), words, strings: text, status, stopped: (layout.stopped ?? []).includes(status) }
}

/** Log ring bytes written since `from` (a running byte total), oldest first. */
export async function readLog(layout: NonNullable<DebugProtocol['log']>, read: Read, from: number): Promise<{ total: number; text: string }> {
  const base = numeric(layout.address), data = numeric(layout.dataOffset)
  const capacity = numeric(layout.size) - data
  const header = await read(base, (layout.headWord + 1) * 4)
  if (word(header, 0) !== magicWord(layout.magic)) return { total: from, text: '' }
  const total = word(header, layout.headWord)
  let start = total < from ? 0 : from
  start = Math.max(start, total - capacity)
  const parts: Uint8Array[] = []
  for (let at = start; at < total;) {
    const offset = at % capacity, length = Math.min(total - at, capacity - offset)
    parts.push(await read(base + data + offset, length)); at += length
  }
  const bytes = new Uint8Array(parts.reduce((sum, part) => sum + part.length, 0))
  parts.reduce((offset, part) => { bytes.set(part, offset); return offset + part.length }, 0)
  return { total, text: new TextDecoder().decode(bytes) }
}

export function fnv1a32(...parts: Uint8Array[]): number {
  let hash = 0x811c9dc5
  for (const part of parts) for (const byte of part) hash = Math.imul(hash ^ byte, 0x01000193) >>> 0
  return hash
}

/** The app slot image (header, JavaScript, aligned pack) for one bundle. */
export function buildAppSlot(layout: NonNullable<DebugProtocol['appSlot']>, javaScript: Uint8Array, pack: Uint8Array, generation: number): Uint8Array {
  if (layout.checksum !== 'fnv1a32') throw new Error(`Unsupported app checksum ${layout.checksum}`)
  const packOffset = Math.ceil((layout.headerBytes + javaScript.length) / layout.packAlign) * layout.packAlign
  const slot = new Uint8Array(packOffset + pack.length)
  if (slot.length > numeric(layout.size)) throw new Error('App bundle exceeds the app slot.')
  const fields: Record<string, number> = {
    magic: magicWord(layout.magic), version: layout.version, javaScriptOffset: layout.headerBytes, javaScriptLength: javaScript.length,
    packOffset, packLength: pack.length, checksum: fnv1a32(javaScript, pack), generation,
  }
  const view = new DataView(slot.buffer)
  layout.header.forEach((name, index) => {
    if (!(name in fields)) throw new Error(`Unknown app slot header field ${name}`)
    view.setUint32(index * 4, fields[name], true)
  })
  slot.set(javaScript, layout.headerBytes); slot.set(pack, packOffset)
  return slot
}

export function wordAddress(layout: NonNullable<DebugProtocol['mailbox']>, name: string): number {
  const index = layout.words.indexOf(name)
  if (index < 0) throw new Error(`Mailbox has no word ${name}`)
  return numeric(layout.address) + index * 4
}
