import { existsSync } from 'node:fs'
import { mkdir } from 'node:fs/promises'
import { resolve, join } from 'node:path'

export const FUJIMOD_COMMIT = '1de936e9b1ca738b04ad5132c727288ddd5eb2ae'
// Custom firmware sources (firmware/build.rs): PocketJS and the QuickJS
// revision its own Symbian toolchain pins.
export const POCKETJS_COMMIT = 'e27bff3e96df3c7b3c8263a34f2362b964df04fb'
export const QUICKJS_RS_COMMIT = 'ba5bdd0dc013518768e76cd9e05cd30ed53dd35b'
const root = resolve(import.meta.dir, '..')

async function run(args: string[], cwd = root, capture = false) {
  console.log('+', args.join(' '))
  const child = Bun.spawn(args, { cwd, stdout: capture ? 'pipe' : 'inherit', stderr: 'inherit' })
  const output = capture ? await new Response(child.stdout as ReadableStream).text() : ''
  if (await child.exited !== 0) throw new Error(`${args[0]} failed`)
  return output.trim()
}

export async function applyFujiModPatch() {
  const source = join(root, 'references/fujimod')
  async function check(patch: string, reverse: boolean) {
    const child = Bun.spawn(['git', 'apply', ...(reverse ? ['--reverse'] : []), '--check', patch],
      { cwd: source, stdout: 'ignore', stderr: 'ignore' })
    return await child.exited === 0
  }
  for (const name of ['fujimod-devtools.patch']) {
    const patch = join(root, 'patches', name)
    if (await check(patch, true)) continue
    if (!await check(patch, false)) throw new Error(`FujiMod patch conflicts with the checkout; preserve local changes and inspect patches/${name}.`)
    await run(['git', 'apply', patch], source)
  }
}

export async function setup() {
  for (const tool of ['git', 'rustup', 'cargo', 'cmake', 'ninja', 'node']) {
    if (!Bun.which(tool)) throw new Error(`Install ${tool} first; see README.md.`)
  }
  const source = join(root, 'references/fujimod')
  await mkdir(join(root, 'references'), { recursive: true })
  if (!await Bun.file(join(source, 'Cargo.toml')).exists()) {
    await run(['git', 'clone', '--no-checkout', 'https://github.com/justjake/fujimod.git', source])
    await run(['git', 'checkout', '--detach', FUJIMOD_COMMIT], source)
  }
  const actual = await run(['git', 'rev-parse', 'HEAD'], source, true)
  if (actual !== FUJIMOD_COMMIT) throw new Error(`Expected FujiMod ${FUJIMOD_COMMIT}, found ${actual}. Preserve the checkout and select the pinned source before building.`)
  await run(['git', 'submodule', 'update', '--init', 'crates/emulator-cpu-unicorn/vendor/unicorn'], source)
  await applyFujiModPatch()
  const pocketjs = await pinned('pocketjs', 'https://github.com/pocket-nexus/pocketjs.git', POCKETJS_COMMIT)
  if (!existsSync(join(pocketjs, 'node_modules'))) await run(['bun', 'install', '--frozen-lockfile'], pocketjs)
  await pinned('quickjs-rs', 'https://github.com/pocket-nexus/quickjs-rs.git', QUICKJS_RS_COMMIT)
  console.log('Setup complete. Run bun run dev.')
}

async function pinned(name: string, url: string, commit: string) {
  const source = join(root, 'references', name)
  if (!await Bun.file(join(source, '.git/HEAD')).exists()) {
    await run(['git', 'clone', '--filter=blob:none', '--no-checkout', url, source])
    await run(['git', 'checkout', '--detach', commit], source)
  }
  const actual = await run(['git', 'rev-parse', 'HEAD'], source, true)
  if (actual !== commit) throw new Error(`Expected ${name} ${commit}, found ${actual}. Preserve the checkout and select the pinned source before building the firmware.`)
  return source
}

if (import.meta.main) await setup()
