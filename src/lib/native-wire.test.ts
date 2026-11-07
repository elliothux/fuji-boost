import { expect, test } from 'bun:test'
import { decodeNative, encodeNative } from './native-wire'

test('native wire preserves buffers, byte views, nesting and order', () => {
  const value = { id: 7, buffer: new Uint8Array([0, 255]).buffer, nested: { bytes: new Uint8Array([9, 1, 2, 9]).subarray(1, 3) } }
  const wire = encodeNative(value)
  expect(new DataView(wire).getUint32(0, true)).toBe(new TextEncoder().encode('{"id":7,"buffer":{"$bytes":2},"nested":{"bytes":{"$bytes":2}}}').length)
  expect(decodeNative(wire)).toEqual(value)
  for (let end = 0; end < wire.byteLength; end++) expect(() => decodeNative(wire.slice(0, end))).toThrow()
  const extra = new Uint8Array(wire.byteLength + 1); extra.set(new Uint8Array(wire))
  expect(() => decodeNative(extra.buffer)).toThrow()
  for (const size of [-1, 0.5, '2', Number.MAX_SAFE_INTEGER]) expect(() => decodeNative(encodeNative({ buffer: { $bytes: size } }))).toThrow()
})
