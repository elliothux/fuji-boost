// bun firmware/profile.ts [name...]
//
// Symbolizes the PC samples `pocket_profile` (crates/xp4/tests/custom.rs)
// writes to .cache/custom-firmware/profile-<name>.pcs against the PocketJS
// image's ELF, and prints the hottest functions and the share per component.
import { readFileSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const repository = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const llvm = process.env.FUJI_LLVM_BIN ?? "/opt/homebrew/opt/llvm@19/bin";
const elf = join(repository, "target/firmware/pocket/armv7a-none-eabihf/release/fuji-boost");
const cache = join(repository, ".cache/custom-firmware");

interface Symbol {
  readonly address: number;
  readonly name: string;
}

function symbols(): Symbol[] {
  const result = Bun.spawnSync([join(llvm, "llvm-nm"), "-n", "-C", "--defined-only", elf]);
  if (result.exitCode !== 0) throw new Error(result.stderr.toString());
  return result.stdout
    .toString()
    .split("\n")
    .map((line) => line.match(/^([0-9a-f]+) [tTwW] (.+)$/))
    .filter((match): match is RegExpMatchArray => match !== null)
    .map((match) => ({ address: Number.parseInt(match[1], 16), name: match[2] }));
}

function locate(table: Symbol[], pc: number): string {
  let low = 0;
  let high = table.length - 1;
  while (low < high) {
    const middle = (low + high + 1) >> 1;
    if (table[middle].address <= pc) low = middle;
    else high = middle - 1;
  }
  return table[low]?.address <= pc ? table[low].name : "?";
}

function component(name: string): string {
  if (/^(JS_|js_|JS[A-Z]|__JS|free_|gc_|mark_|lre_|lru_|unicode_|cr_|dbuf_|delete_|skip_|ident_|next_token|resolve_|optimize_|code_|emit_|push_|set_|add_|find_|get_|string_|atom_|shape_|async_|build_|perform_|instantiate_|js)/.test(name)) return "quickjs";
  if (/^(pocket_runtime|host_|native_)/.test(name)) return "pocket_runtime.c";
  if (/^(malloc|free|realloc|calloc|mem|str|fuji_heap|block_|large_)/.test(name)) return "libc";
  if (/pocketjs_core|pocketjs_symbian_core|draw::|raster|layout|taffy/.test(name)) return "ui-cabi (Rust)";
  if (/fuji_boost|fuji_/.test(name)) return "firmware";
  if (/compiler_builtins|__aeabi|libm/.test(name)) return "compiler-builtins";
  if (/^(core::|alloc::|<)/.test(name)) return "rust core/alloc";
  return "other";
}

function report(name: string, table: Symbol[]): string {
  const pcs = readFileSync(join(cache, `profile-${name}.pcs`), "utf8")
    .split("\n")
    .filter(Boolean)
    .map((line) => Number.parseInt(line, 16));
  const functions = new Map<string, number>();
  const components = new Map<string, number>();
  for (const pc of pcs) {
    const symbol = locate(table, pc);
    functions.set(symbol, (functions.get(symbol) ?? 0) + 1);
    const part = component(symbol);
    components.set(part, (components.get(part) ?? 0) + 1);
  }
  const share = (count: number) => `${((100 * count) / pcs.length).toFixed(1).padStart(5)}%`;
  const lines = [`## ${name}: ${pcs.length} samples`, "", "components:"];
  for (const [part, count] of [...components].sort((a, b) => b[1] - a[1])) {
    lines.push(`  ${share(count)}  ${part}`);
  }
  lines.push("", "top functions:");
  for (const [symbol, count] of [...functions].sort((a, b) => b[1] - a[1]).slice(0, 25)) {
    lines.push(`  ${share(count)}  ${symbol.length > 140 ? `${symbol.slice(0, 137)}...` : symbol}`);
  }
  return lines.join("\n");
}

const table = symbols();
const names = process.argv.slice(2);
const text = (names.length ? names : ["steady", "select"]).map((name) => report(name, table)).join("\n\n");
writeFileSync(join(cache, "profile.txt"), `${text}\n`);
console.log(text);
