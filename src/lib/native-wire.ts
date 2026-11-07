// One binary message: u32-LE JSON length, UTF-8 JSON, then byte fields in JSON order.
const limit = 256 * 1024 * 1024
export function encodeNative(value: unknown): ArrayBuffer {
  const chunks: Uint8Array[] = []
  const json = new TextEncoder().encode(JSON.stringify(value, (_key, item: unknown) => {
    if (item instanceof ArrayBuffer || item instanceof Uint8Array) {
      const bytes = item instanceof Uint8Array ? item : new Uint8Array(item)
      chunks.push(bytes)
      return { $bytes: bytes.byteLength }
    }
    return item
  }))
  const size = 4 + json.length + chunks.reduce((sum, bytes) => sum + bytes.byteLength, 0)
  if (size > limit) throw new Error('Native message exceeds 256 MiB.')
  const result = new Uint8Array(size)
  new DataView(result.buffer).setUint32(0, json.length, true)
  result.set(json, 4)
  let offset = 4 + json.length
  for (const bytes of chunks) { result.set(bytes, offset); offset += bytes.byteLength }
  return result.buffer
}

export function decodeNative<T>(buffer: ArrayBuffer): T {
  if (buffer.byteLength < 4 || buffer.byteLength > limit) throw new Error('Invalid native message length.')
  const end = 4 + new DataView(buffer).getUint32(0, true)
  if (end > buffer.byteLength) throw new Error('Truncated native JSON.')
  let offset = end
  const value: T = JSON.parse(new TextDecoder('utf-8', { fatal: true }).decode(new Uint8Array(buffer, 4, end - 4)), (key, item: unknown) => {
    if (item && typeof item === 'object' && '$bytes' in item) {
      const size = item.$bytes
      if (Object.keys(item).length !== 1 || typeof size !== 'number' || !Number.isSafeInteger(size) || size < 0 || size > buffer.byteLength - offset) throw new Error('Invalid native byte field.')
      const bytes = buffer.slice(offset, offset + size)
      offset += size
      return key === 'bytes' ? new Uint8Array(bytes) : bytes
    }
    return item
  })
  if (offset !== buffer.byteLength) throw new Error('Unexpected native message bytes.')
  return value
}
