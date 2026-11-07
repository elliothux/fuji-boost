import assert from 'node:assert/strict'
import { createHash } from 'node:crypto'
import { existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs'
import path from 'node:path'

interface MemorySource {
  read_virtual_memory(core: number, address: number, length: number): Uint8Array
  state_json(): string
  diagnostics_json(): string
}
export interface MemoryProbe { label: string; core: number; address: number; length: number }
type Chunk = { address: number; length: number } & (
  { status: 'readable'; sha256: string; file: string } |
  { status: 'unavailable'; error: string }
)
const sha = (data: string | Uint8Array) => createHash('sha256').update(data).digest('hex')

export function captureLateRam(source: MemorySource, directory: string,
  protectedJsonPaths: readonly string[], ramSize: number, probes: readonly MemoryProbe[]) {
  assert(Number.isSafeInteger(ramSize) && ramSize > 0 && ramSize <= 0x1_0000_0000)
  const relative = path.relative(process.cwd(), path.resolve(directory))
  assert(relative && !relative.startsWith('..') && !path.isAbsolute(relative), 'Snapshot must be inside the workspace')
  assert.equal(new Set(protectedJsonPaths).size, protectedJsonPaths.length)
  assert.equal(new Set(probes.map(p => p.label)).size, probes.length)
  for (const p of probes) {
    assert(Number.isSafeInteger(p.core) && p.core >= 0 && p.core < 4)
    assert(p.label && Number.isSafeInteger(p.address) && p.address >= 0)
    assert(Number.isSafeInteger(p.length) && p.length >= 1 && p.length <= 65536 && p.address + p.length <= ramSize)
  }
  mkdirSync(directory)
  mkdirSync(path.join(directory, 'objects'))
  const protectedHashes = () => protectedJsonPaths.map(file => ({ file, sha256: sha(readFileSync(file)) }))
  const protectedBefore = protectedHashes()
  const beforeState = source.state_json(), beforeDiagnostics = source.diagnostics_json()
  assert.equal(source.state_json(), beforeState, 'Diagnostics must preserve state')
  const chunks: Chunk[] = [], fields: (MemoryProbe & Chunk)[] = []
  const objects = new Set<string>()
  let storedBytes = 0
  function read(core: number, address: number, length: number): Chunk {
    let bytes: Uint8Array
    try { bytes = source.read_virtual_memory(core, address, length) }
    catch (error) {
      if (error instanceof WebAssembly.RuntimeError) throw error
      return { address, length, status: 'unavailable', error: error instanceof Error ? error.message : String(error) }
    }
    assert(bytes instanceof Uint8Array && bytes.length === length, 'Memory reader returned a wrong extent')
    const hash = sha(bytes), file = `objects/${hash}.bin`, target = path.join(directory, file)
    if (!objects.has(hash)) {
      if (existsSync(target)) assert.equal(sha(readFileSync(target)), hash)
      else writeFileSync(target, bytes, { flag: 'wx' })
      objects.add(hash); storedBytes += bytes.length
    }
    return { address, length, status: 'readable', sha256: hash, file }
  }
  try {
    for (let address = 0; address < ramSize; address += 65536) chunks.push(read(0, address, Math.min(65536, ramSize - address)))
    for (const p of probes) fields.push({ ...p, ...read(p.core, p.address, p.length) })
  } finally {
    const afterState = source.state_json(), afterDiagnostics = source.diagnostics_json(), protectedAfter = protectedHashes()
    for (const [name, raw] of Object.entries({ 'before-state': beforeState, 'before-diagnostics': beforeDiagnostics,
      'after-state': afterState, 'after-diagnostics': afterDiagnostics })) writeFileSync(path.join(directory, name + '.json'), raw, { flag: 'wx' })
    const passed = beforeState === afterState && beforeDiagnostics === afterDiagnostics &&
      JSON.stringify(protectedBefore) === JSON.stringify(protectedAfter)
    writeFileSync(path.join(directory, 'read-guard.json'), JSON.stringify({ passed,
      scope: 'Public serialized state, diagnostics and sealed files; not all VM internals or the soft TLB',
      beforeStateSha256: sha(beforeState), afterStateSha256: sha(afterState),
      beforeDiagnosticsSha256: sha(beforeDiagnostics), afterDiagnosticsSha256: sha(afterDiagnostics),
      protectedBefore, protectedAfter }, null, 2) + '\n', { flag: 'wx' })
    assert(passed, 'Snapshot reads changed serialized state, diagnostics or protected files')
  }
  const manifest = { kind: 'EMULATED-core0-virtual-backing-view', core: 0, start: 0, end: ramSize,
    chunkBytes: 65536, chunks, probes: fields, uniqueObjects: objects.size, storedBytes,
    readableBytes: chunks.reduce((n, c) => n + (c.status === 'readable' ? c.length : 0), 0),
    unavailableChunks: chunks.filter(c => c.status === 'unavailable').length,
    physicalCameraAccess: false, restorableCheckpoint: false, softTlbMayChange: true,
    apiTranslationStepBytes: 4096, perByteArchitecturalPermissionProof: false,
    meaning: 'A core0 current MMU/permission/cache view over the declared low RAM span, plus explicitly named per-core probes. A failed extent is entirely unavailable, not zero. Equal content does not establish physical aliasing. Guards cover public serialized state and files; translation may change the soft TLB. No guest execution follows this capture.' }
  writeFileSync(path.join(directory, 'manifest.json'), JSON.stringify(manifest, null, 2) + '\n', { flag: 'wx' })
  return manifest
}
