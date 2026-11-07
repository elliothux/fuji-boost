import { describe, expect, test } from 'bun:test'
import { existsSync, mkdirSync, mkdtempSync, readFileSync, writeFileSync } from 'node:fs'
import path from 'node:path'
import { captureLateRam } from '../../scripts/virtual-memory-snapshot'

function fixture() {
  const root = path.resolve('.temp/late-ram-helper-tests')
  mkdirSync(root, { recursive: true })
  const directory = mkdtempSync(path.join(root, 'case-'))
  const sealed = path.join(directory, 'sealed.json')
  writeFileSync(sealed, '{"sealed":true}\n')
  return { output: path.join(directory, 'memory'), sealed }
}
const state = () => '{"cpu":{"instructions":4,"virtual_ticks":8}}'
const diagnostics = () => '{"devices":{}}'

describe('paused virtual backing snapshot', () => {
  test('deduplicates content, preserves unavailable ranges and uses each probe core', () => {
    const f = fixture(), reads: number[][] = []
    const source = { state_json: state, diagnostics_json: diagnostics,
      read_virtual_memory(core: number, address: number, length: number) {
        reads.push([core, address, length])
        if (core === 0 && address === 131072 && length > 4) throw new Error('unreadable tail')
        return new Uint8Array(length).fill(core === 3 ? 0xa5 : 0)
      } }
    const result = captureLateRam(source, f.output, [f.sealed], 131088, [
      { label: 'other-core', core: 3, address: 32, length: 4 },
      { label: 'readable-prefix', core: 0, address: 131072, length: 4 },
    ])
    expect(reads.slice(0, 3)).toEqual([[0, 0, 65536], [0, 65536, 65536], [0, 131072, 16]])
    expect(result.chunks[0]).toEqual({ ...result.chunks[1], address: 0 })
    expect(result.chunks[2]).toEqual({ address: 131072, length: 16, status: 'unavailable', error: 'unreadable tail' })
    const p = result.probes[0]
    expect(p?.core).toBe(3)
    if (!p || p.status !== 'readable') throw new Error('Missing probe')
    expect([...readFileSync(path.join(f.output, p.file))]).toEqual([0xa5, 0xa5, 0xa5, 0xa5])
    expect(result.uniqueObjects).toBe(3)
    expect(result.readableBytes).toBe(131072)
    expect(result.unavailableChunks).toBe(1)
    expect(JSON.parse(readFileSync(path.join(f.output, 'read-guard.json'), 'utf8')).passed).toBe(true)
    expect(() => captureLateRam(source, f.output, [f.sealed], 1, [])).toThrow()
  })

  test('does not publish a manifest if a read changes serialized state or sealed data', () => {
    for (const kind of ['state', 'file'] as const) {
      const f = fixture(); let changed = false
      const source = { state_json: () => changed ? '{"changed":true}' : state(), diagnostics_json: diagnostics,
        read_virtual_memory(_core: number, _address: number, length: number) {
          if (kind === 'state') changed = true
          else writeFileSync(f.sealed, '{"tampered":true}')
          return new Uint8Array(length)
        } }
      expect(() => captureLateRam(source, f.output, [f.sealed], 8, [])).toThrow('Snapshot reads changed')
      expect(existsSync(path.join(f.output, 'manifest.json'))).toBe(false)
      expect(JSON.parse(readFileSync(path.join(f.output, 'read-guard.json'), 'utf8')).passed).toBe(false)
    }
  })

  test('rejects invalid requests and treats a WebAssembly trap as fatal', () => {
    const f = fixture(); let calls = 0
    const source = { state_json: state, diagnostics_json: diagnostics,
      read_virtual_memory() { calls++; throw new WebAssembly.RuntimeError('trap') } }
    for (const size of [0, -1, NaN, 0x1_0000_0001]) expect(() => captureLateRam(source, f.output, [f.sealed], size, [])).toThrow()
    expect(() => captureLateRam(source, f.output, [f.sealed], 8, [{ label: 'invalid-core', core: 4, address: 0, length: 1 }])).toThrow()
    expect(calls).toBe(0)
    expect(() => captureLateRam(source, f.output, [f.sealed], 8, [])).toThrow('trap')
    expect(existsSync(path.join(f.output, 'manifest.json'))).toBe(false)
  })
})
