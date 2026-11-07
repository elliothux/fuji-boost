// bun firmware/build.ts [--app=<name>] [--bundle-only] [--watch] [--no-nan-boxing]
//
// 1. Resolve every firmware/apps/*/pocket.json against the fuji-xt4 profile.
// 2. Bundle each with the vendored PocketJS (references/pocketjs/tools/build.ts)
//    into firmware/dist/apps/<app>/<output>.{js,pak}; --app (default
//    counter) is the one embedded in the image, the rest are app-slot loads.
// 3. Unless --bundle-only: build the HAL self-test image and the PocketJS
//    image (cargo, pinned nightly + build-std), objcopy them into
//    firmware/dist/, and write firmware/dist/fuji-boost.json: per-image load
//    address and entry (from the ELF) plus protocol.json, which is all a host
//    needs to run the images and read their debug state.
// firmware/dist/apps/index.json lists the bundles with hashes; the camera page
// polls it and hot-loads a changed bundle into the app slot.
// --watch (implies --bundle-only) rebundles an app whenever its files change.
import { createHash } from "node:crypto";
import { existsSync, mkdirSync, readdirSync, readFileSync, watch, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import {
  FUJI_XT4_HOST_ABI,
  FUJI_XT4_TARGET_ID,
  resolveFujiBuildPlan,
} from "./app/profile.ts";

const repository = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const pocketjs = join(repository, "references/pocketjs");
const llvm = process.env.FUJI_LLVM_BIN ?? "/opt/homebrew/opt/llvm@19/bin";
const out = join(repository, "firmware/dist");

const args = process.argv.slice(2);
const flag = (name: string) => args.includes(`--${name}`);
const option = (name: string) =>
  args.find((arg) => arg.startsWith(`--${name}=`))?.slice(name.length + 3);

function run(command: string[], env: Record<string, string> = {}, cwd = repository): void {
  const result = Bun.spawnSync(command, {
    cwd,
    env: { ...process.env, ...env },
    stdout: "inherit",
    stderr: "inherit",
  });
  if (result.exitCode !== 0) {
    throw new Error(`${command.join(" ")} exited with ${result.exitCode}`);
  }
}

function sha256(path: string): string {
  return createHash("sha256").update(readFileSync(path)).digest("hex");
}

export interface AppBundle {
  readonly app: string;
  readonly javaScript: string;
  readonly pack: string;
  readonly planHash: string;
}

export function bundleApp(app: string): AppBundle {
  const manifestPath = join(repository, "firmware/apps", app, "pocket.json");
  if (!existsSync(manifestPath)) throw new Error(`No firmware app at ${manifestPath}`);
  const plan = resolveFujiBuildPlan(JSON.parse(readFileSync(manifestPath, "utf8")));
  const planPath = join(repository, ".cache/custom-firmware/plans", `${app}.plan.json`);
  const outdir = join(out, "apps", app);
  mkdirSync(dirname(planPath), { recursive: true });
  mkdirSync(outdir, { recursive: true });
  writeFileSync(planPath, `${JSON.stringify(plan, null, 2)}\n`);
  run(
    [
      process.execPath,
      join(pocketjs, "tools/build.ts"),
      `--plan=${planPath}`,
      `--project-root=${repository}`,
      `--outdir=${outdir}`,
    ],
    { POCKET_NEXUS_CACHE_DIR: join(repository, ".cache/pocket-nexus") },
  );
  const bundle = {
    app,
    javaScript: join(outdir, `${plan.app.output}.js`),
    pack: join(outdir, `${plan.app.output}.pak`),
    planHash: plan.planHash,
  };
  for (const path of [bundle.javaScript, bundle.pack]) {
    if (!existsSync(path)) throw new Error(`PocketJS build did not produce ${path}`);
  }
  return bundle;
}

function writeIndex(bundles: Record<string, AppBundle>, embedded: string): void {
  const apps = join(out, "apps");
  const relative = (path: string) => path.slice(apps.length + 1);
  const index = {
    embedded,
    apps: Object.fromEntries(
      Object.values(bundles).map((b) => [
        b.app,
        {
          javaScript: relative(b.javaScript),
          pack: relative(b.pack),
          sha256: createHash("sha256").update(readFileSync(b.javaScript)).update(readFileSync(b.pack)).digest("hex"),
        },
      ]),
    ),
  };
  writeFileSync(join(apps, "index.json"), `${JSON.stringify(index, null, 2)}\n`);
}

function cargo(features: string[], targetDir: string, env: Record<string, string>): string {
  run(
    ["cargo", "build", "--release", ...features],
    { CARGO_TARGET_DIR: targetDir, FUJI_LLVM_BIN: llvm, ...env },
    join(repository, "firmware"),
  );
  return join(targetDir, "armv7a-none-eabihf/release/fuji-boost");
}

interface Image {
  readonly file: string;
  readonly label: string;
  readonly loadAddress: string;
  readonly entry: string;
  readonly bytes: number;
  readonly sha256: string;
}

function hex(value: number): string {
  return `0x${value.toString(16).padStart(8, "0")}`;
}

function image(elf: string, file: string, label: string): Image {
  const path = join(out, file);
  mkdirSync(dirname(path), { recursive: true });
  run([join(llvm, "llvm-objcopy"), "-O", "binary", elf, path]);
  const headers = Bun.spawnSync([join(llvm, "llvm-readelf"), "--file-header", "--program-headers", elf]);
  const text = headers.stdout.toString();
  const entry = text.match(/Entry point address:\s+(0x[0-9a-f]+)/)?.[1];
  // objcopy -O binary starts at the lowest loaded physical address.
  const loads = [...text.matchAll(/^\s*LOAD\s+\S+\s+\S+\s+(0x[0-9a-f]+)\s+(0x[0-9a-f]+)/gm)];
  if (headers.exitCode !== 0 || !entry || !loads.length) throw new Error(`llvm-readelf could not read ${elf}`);
  const loadAddress = Math.min(...loads.filter((load) => Number(load[2]) > 0).map((load) => Number(load[1])));
  const bytes = readFileSync(path).length;
  return { file, label, loadAddress: hex(loadAddress), entry: hex(Number(entry)), bytes, sha256: sha256(path) };
}

if (import.meta.main) {
  const app = option("app") ?? "counter";
  const apps = readdirSync(join(repository, "firmware/apps")).filter((name) =>
    existsSync(join(repository, "firmware/apps", name, "pocket.json")),
  );
  if (!apps.includes(app)) throw new Error(`Unknown firmware app ${app}; have ${apps.join(", ")}`);
  const bundles = Object.fromEntries(apps.map((name) => [name, bundleApp(name)]));
  const bundle = bundles[app];
  writeIndex(bundles, app);
  console.log(`fuji-boost: bundled ${apps.join(", ")}; embedding ${app}`);
  if (flag("watch")) {
    let timer: ReturnType<typeof setTimeout> | undefined;
    const dirty = new Set<string>();
    watch(join(repository, "firmware/apps"), { recursive: true }, (_event, file) => {
      const name = file?.split(/[\\/]/)[0];
      if (!name || !apps.includes(name)) return;
      dirty.add(name);
      clearTimeout(timer);
      timer = setTimeout(() => {
        for (const changed of dirty) {
          const started = performance.now();
          try {
            bundles[changed] = bundleApp(changed);
            writeIndex(bundles, app);
            console.log(`fuji-boost: rebundled ${changed} in ${Math.round(performance.now() - started)} ms`);
          } catch (error) {
            console.error(`fuji-boost: ${changed}: ${error instanceof Error ? error.message : error}`);
          }
        }
        dirty.clear();
      }, 150);
    });
    console.log("fuji-boost: watching firmware/apps");
  } else if (!flag("bundle-only")) {
    const hello = image(
      cargo(["--no-default-features"], join(repository, "target/firmware/hello"), {}),
      "fuji-boost-hello.bin",
      "HAL 自检",
    );
    const env: Record<string, string> = {
      FUJI_APP_JS: bundle.javaScript,
      FUJI_APP_PAK: bundle.pack,
      POCKETJS_TARGET_ID: FUJI_XT4_TARGET_ID,
      POCKETJS_HOST_ABI: String(FUJI_XT4_HOST_ABI),
    };
    if (flag("no-nan-boxing")) env.FUJI_QUICKJS_NO_NAN_BOXING = "1";
    const pocket = image(cargo([], join(repository, "target/firmware/pocket"), env), "fuji-boost.bin", "PocketJS");
    const manifest = {
      schema: 1,
      name: "fuji-boost",
      images: { pocket, hello },
      protocol: JSON.parse(readFileSync(join(repository, "firmware/protocol.json"), "utf8")),
      apps: { index: "apps/index.json", image: "pocket", embedded: app },
      build: {
        target: FUJI_XT4_TARGET_ID,
        hostAbi: FUJI_XT4_HOST_ABI,
        planHash: bundle.planHash,
        javaScriptSha256: sha256(bundle.javaScript),
        packSha256: sha256(bundle.pack),
        nanBoxing: !flag("no-nan-boxing"),
      },
    };
    writeFileSync(join(out, "fuji-boost.json"), `${JSON.stringify(manifest, null, 2)}\n`);
    console.log(`fuji-boost: ${join(out, pocket.file)} (${pocket.bytes} bytes), ${join(out, hello.file)} (${hello.bytes} bytes)`);
  }
}
