//! Custom firmware (firmware/) on the emulated X-T4 board. The emulator only
//! runs a RAM image; the image's debug protocol (status words, log ring, app
//! slot) is read from its manifest, `firmware/dist/fuji-boost.json`. Images
//! come from `bun run firmware:build`; metrics land in `.cache/custom-firmware/`.
use fuji_boost_xp4::{
    Contact, Execution, checkpoint_file,
    ram_image::{Image, ImageSession},
};
use fujimod_firmware::sha256;
use serde_json::{Map, Value, json};
use std::{
    collections::BTreeMap,
    fs,
    ops::{Deref, Index},
    path::{Path, PathBuf},
    sync::OnceLock,
    time::Instant,
};

const PANEL_WIDTH: usize = 900;
const PANEL_HEIGHT: usize = 600;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn dist() -> PathBuf {
    root().join("firmware/dist")
}

fn read(path: &Path) -> Vec<u8> {
    fs::read(path).unwrap_or_else(|e| {
        panic!(
            "{}: {e}; run `bun run firmware:build` first",
            path.display()
        )
    })
}

fn manifest() -> &'static Value {
    static MANIFEST: OnceLock<Value> = OnceLock::new();
    MANIFEST.get_or_init(|| serde_json::from_slice(&read(&dist().join("fuji-boost.json"))).unwrap())
}

fn protocol() -> &'static Value {
    &manifest()["protocol"]
}

/// Manifest numbers are JSON numbers or "0x…" strings.
fn number(value: &Value) -> u64 {
    match value {
        Value::String(text) => u64::from_str_radix(text.trim_start_matches("0x"), 16).unwrap(),
        _ => value
            .as_u64()
            .unwrap_or_else(|| panic!("not a number: {value}")),
    }
}

fn frame_ticks() -> u64 {
    number(&protocol()["mailbox"]["frameTicks"])
}

fn names(value: &Value) -> Vec<&str> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect()
}

fn word_address(name: &str) -> u32 {
    let mailbox = &protocol()["mailbox"];
    let index = names(&mailbox["words"])
        .iter()
        .position(|word| *word == name)
        .unwrap_or_else(|| panic!("no mailbox word {name}"));
    number(&mailbox["address"]) as u32 + 4 * index as u32
}

fn words(bytes: &[u8]) -> Vec<u32> {
    bytes
        .as_chunks::<4>()
        .0
        .iter()
        .map(|&w| u32::from_le_bytes(w))
        .collect()
}

#[derive(Clone, Debug, PartialEq)]
struct Mailbox {
    words: BTreeMap<String, u32>,
    action_name: String,
}

impl Index<&str> for Mailbox {
    type Output = u32;
    fn index(&self, name: &str) -> &u32 {
        self.words
            .get(name)
            .unwrap_or_else(|| panic!("no mailbox word {name}"))
    }
}

impl Mailbox {
    fn status(&self) -> &'static str {
        protocol()["mailbox"]["status"][self["status"] as usize]
            .as_str()
            .unwrap_or("unknown")
    }

    fn action_value(&self) -> i32 {
        self["actionValue"] as i32
    }

    fn json(&self) -> Value {
        let signed = names(&protocol()["mailbox"]["signed"]);
        let mut map: Map<String, Value> = self
            .words
            .iter()
            .map(|(name, &value)| {
                let value = if signed.contains(&name.as_str()) {
                    json!(value as i32)
                } else {
                    json!(value)
                };
                (name.clone(), value)
            })
            .collect();
        map.insert("status".into(), json!(self.status()));
        map.insert("actionName".into(), json!(self.action_name));
        Value::Object(map)
    }
}

/// An image session plus the host side of the image's debug protocol.
struct Guest(ImageSession);

impl Deref for Guest {
    type Target = ImageSession;
    fn deref(&self) -> &ImageSession {
        &self.0
    }
}

impl Guest {
    fn word(&self, name: &str) -> u32 {
        words(&self.read_physical_memory(word_address(name), 4).unwrap())[0]
    }

    fn set_word(&self, name: &str, value: u32) {
        self.write_physical_memory(word_address(name), &value.to_le_bytes())
            .unwrap();
    }

    fn mailbox(&self) -> Mailbox {
        let layout = &protocol()["mailbox"];
        let names = names(&layout["words"]);
        let base = number(&layout["address"]) as u32;
        let values = words(
            &self
                .read_physical_memory(base, 4 * names.len() as u32)
                .unwrap(),
        );
        let string = &layout["strings"]["actionName"];
        let name = self
            .read_physical_memory(
                base + 4 * number(&string["word"]) as u32,
                4 * number(&string["words"]) as u32,
            )
            .unwrap();
        let end = name.iter().position(|&b| b == 0).unwrap_or(name.len());
        Mailbox {
            words: names
                .iter()
                .map(|name| name.to_string())
                .zip(values)
                .collect(),
            action_name: String::from_utf8_lossy(&name[..end]).into_owned(),
        }
    }

    /// The firmware reports its own faults through the status word.
    fn guest_stopped(&self) -> bool {
        names(&protocol()["mailbox"]["stopped"]).contains(&self.mailbox().status())
    }

    fn stopped(&self, state: &Execution) -> bool {
        state.stopped() || self.guest_stopped()
    }

    fn stopped_reason(&self, state: &Execution) -> String {
        let log = self.log();
        let tail = &log[log.len().saturating_sub(2000)..];
        format!(
            "{}\n{:?}\n{tail}",
            serde_json::to_string(state).unwrap(),
            self.mailbox()
        )
    }

    /// The log ring's retained text.
    fn log(&self) -> String {
        let layout = &protocol()["log"];
        let base = number(&layout["address"]) as u32;
        let data = number(&layout["dataOffset"]) as u32;
        let capacity = number(&layout["size"]) as u32 - data;
        let head = words(
            &self
                .read_physical_memory(base + 4 * number(&layout["headWord"]) as u32, 4)
                .unwrap(),
        )[0];
        let ring = self.read_physical_memory(base + data, capacity).unwrap();
        let kept = head.min(capacity);
        let bytes: Vec<u8> = (head - kept..head)
            .map(|at| ring[(at % capacity) as usize])
            .collect();
        String::from_utf8_lossy(&bytes).into_owned()
    }

    /// Per-frame work samples (ticks), oldest first.
    fn work_samples(&self) -> Vec<u32> {
        let layout = &protocol()["mailbox"]["workRing"];
        let (word, length) = (
            number(&layout["word"]) as u32,
            number(&layout["length"]) as u32,
        );
        let base = number(&protocol()["mailbox"]["address"]) as u32;
        let ring = words(
            &self
                .read_physical_memory(base + 4 * word, 4 * length)
                .unwrap(),
        );
        let total = self.word("workSamples");
        (total - total.min(length)..total)
            .map(|at| ring[(at % length) as usize])
            .collect()
    }

    /// Write an app into the slot and request a reload; returns its generation.
    fn load_app(&self, java_script: &[u8], pack: &[u8]) -> u32 {
        let layout = &protocol()["appSlot"];
        assert_eq!(layout["checksum"], "fnv1a32");
        let header_bytes = number(&layout["headerBytes"]) as usize;
        let align = number(&layout["packAlign"]) as usize;
        let pack_offset = (header_bytes + java_script.len()).next_multiple_of(align);
        let generation = self.word("appGeneration") + 1;
        let checksum = java_script
            .iter()
            .chain(pack)
            .fold(0x811c_9dc5u32, |hash, &byte| {
                (hash ^ u32::from(byte)).wrapping_mul(0x0100_0193)
            });
        let magic = layout["magic"].as_str().unwrap().as_bytes();
        let fields = BTreeMap::from([
            ("magic", u32::from_le_bytes(magic.try_into().unwrap())),
            ("version", number(&layout["version"]) as u32),
            ("javaScriptOffset", header_bytes as u32),
            ("javaScriptLength", java_script.len() as u32),
            ("packOffset", pack_offset as u32),
            ("packLength", pack.len() as u32),
            ("checksum", checksum),
            ("generation", generation),
        ]);
        let mut slot = vec![0; pack_offset + pack.len()];
        assert!(slot.len() as u64 <= number(&layout["size"]));
        for (index, name) in names(&layout["header"]).into_iter().enumerate() {
            slot[4 * index..][..4].copy_from_slice(&fields[name].to_le_bytes());
        }
        slot[header_bytes..][..java_script.len()].copy_from_slice(java_script);
        slot[pack_offset..].copy_from_slice(pack);
        self.write_physical_memory(number(&layout["address"]) as u32, &slot)
            .unwrap();
        self.set_word(layout["reloadWord"].as_str().unwrap(), 1);
        generation
    }
}

fn session(name: &str) -> Guest {
    let image = &manifest()["images"][name];
    let bytes = read(&dist().join(image["file"].as_str().unwrap()));
    Guest(
        ImageSession::new(&Image {
            bytes: &bytes,
            load_address: number(&image["loadAddress"]) as u32,
            entry: number(&image["entry"]) as u32,
        })
        .unwrap(),
    )
}

fn evidence(name: &str, value: &Value) {
    let dir = root().join(".cache/custom-firmware");
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join(format!("{name}.json")),
        serde_json::to_vec_pretty(value).unwrap(),
    )
    .unwrap();
}

fn save_png(name: &str, rgba: &[u8]) -> PathBuf {
    fn crc32(bytes: &[u8]) -> u32 {
        let mut crc = !0u32;
        for &byte in bytes {
            crc ^= u32::from(byte);
            for _ in 0..8 {
                crc = if crc & 1 != 0 {
                    0xedb8_8320 ^ (crc >> 1)
                } else {
                    crc >> 1
                };
            }
        }
        !crc
    }
    fn chunk(png: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
        png.extend_from_slice(&(data.len() as u32).to_be_bytes());
        let start = png.len();
        png.extend_from_slice(kind);
        png.extend_from_slice(data);
        let crc = crc32(&png[start..]);
        png.extend_from_slice(&crc.to_be_bytes());
    }
    let mut raw = Vec::with_capacity(rgba.len() + PANEL_HEIGHT);
    for row in rgba.as_chunks::<{ PANEL_WIDTH * 4 }>().0 {
        raw.push(0);
        raw.extend_from_slice(row);
    }
    let mut encoder = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::fast());
    std::io::Write::write_all(&mut encoder, &raw).unwrap();
    let mut header = Vec::new();
    header.extend_from_slice(&(PANEL_WIDTH as u32).to_be_bytes());
    header.extend_from_slice(&(PANEL_HEIGHT as u32).to_be_bytes());
    header.extend_from_slice(&[8, 6, 0, 0, 0]);
    let mut png = b"\x89PNG\r\n\x1a\n".to_vec();
    chunk(&mut png, b"IHDR", &header);
    chunk(&mut png, b"IDAT", &encoder.finish().unwrap());
    chunk(&mut png, b"IEND", &[]);
    let dir = root().join(".cache/custom-firmware");
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join(format!("{name}.png"));
    fs::write(&path, png).unwrap();
    path
}

/// Run until `count` more LCDC captures; returns instructions executed.
fn vsyncs(cpu: &Guest, count: u32) -> u64 {
    // Generous: even a frame-dropping app finishes a capture within 40M.
    let limit = u64::from(count) * 40_000_000;
    let start = cpu.run_state().cpu.instructions;
    let target = cpu.word("vsyncs") + count;
    loop {
        let executed = cpu.run_state().cpu.instructions - start;
        let seen = cpu.word("vsyncs");
        if seen >= target {
            return executed;
        }
        assert!(
            executed < limit,
            "{count} vsyncs did not arrive within {limit} instructions: {:?}",
            cpu.mailbox()
        );
        // Short slices keep the stop close to the capture.
        let ticks = (u64::from(target - seen) * frame_ticks()).min(frame_ticks() / 8);
        let state = cpu.run_for(ticks, limit - executed).unwrap();
        assert!(!cpu.stopped(&state), "{}", cpu.stopped_reason(&state));
    }
}

fn panel(cpu: &Guest) -> Vec<u8> {
    let rgba = cpu.panel_rgba().unwrap();
    assert_eq!(rgba.len(), PANEL_WIDTH * PANEL_HEIGHT * 4);
    rgba
}

fn pixel(rgba: &[u8], x: usize, y: usize) -> [u8; 4] {
    rgba[(y * PANEL_WIDTH + x) * 4..][..4].try_into().unwrap()
}

fn healthy(cpu: &Guest) {
    let state = cpu.run_state();
    assert!(!cpu.stopped(&state), "{}", cpu.stopped_reason(&state));
    assert_eq!(cpu.mailbox().status(), "running");
}

/// Run whole batches until `done` holds; returns instructions executed.
fn until(cpu: &Guest, limit: u64, done: impl Fn(&Mailbox) -> bool) -> u64 {
    let start = cpu.run_state().cpu.instructions;
    loop {
        let executed = cpu.run_state().cpu.instructions - start;
        if done(&cpu.mailbox()) {
            return executed;
        }
        assert!(
            executed < limit,
            "condition not met within {limit} instructions: {:?}",
            cpu.mailbox()
        );
        let state = cpu.execute_batch(1_000_000).unwrap();
        assert!(!cpu.stopped(&state), "{}", cpu.stopped_reason(&state));
    }
}

fn percentile(samples: &[u32], fraction: f64) -> u32 {
    let mut sorted = samples.to_vec();
    sorted.sort_unstable();
    sorted[((sorted.len() - 1) as f64 * fraction).round() as usize]
}

// ---- M1: HAL demo (image "hello") ---------------------------------------

const HELLO: &str = "hello";
const HELLO_W: i32 = 450;
const HELLO_H: i32 = 300;
const HELLO_SIZE: i32 = 40;

/// The demo's picture, recomputed on the host and scaled 2x like LCDC1.
fn hello_reference(square: (i32, i32), color: [u8; 4]) -> Vec<u8> {
    let mut rgba = vec![0; PANEL_WIDTH * PANEL_HEIGHT * 4];
    for py in 0..PANEL_HEIGHT {
        for px in 0..PANEL_WIDTH {
            let (x, y) = ((px / 2) as i32, (py / 2) as i32);
            let value = if (square.0..square.0 + HELLO_SIZE).contains(&x)
                && (square.1..square.1 + HELLO_SIZE).contains(&y)
            {
                color
            } else if x < 4 || y < 4 || x >= HELLO_W - 4 || y >= HELLO_H - 4 {
                [255; 4]
            } else {
                [
                    (x * 255 / (HELLO_W - 1)) as u8,
                    (y * 255 / (HELLO_H - 1)) as u8,
                    128,
                    255,
                ]
            };
            rgba[(py * PANEL_WIDTH + px) * 4..][..4].copy_from_slice(&value);
        }
    }
    rgba
}

#[test]
#[ignore = "needs firmware/dist (bun run firmware:build)"]
fn hello_first_frame() {
    let cpu = session(HELLO);
    let started = Instant::now();
    let instructions = vsyncs(&cpu, 3);
    healthy(&cpu);
    let rgba = panel(&cpu);
    let center = ((HELLO_W - HELLO_SIZE) / 2, (HELLO_H - HELLO_SIZE) / 2);
    let reference = hello_reference(center, [255; 4]);
    assert!(
        rgba == reference,
        "first frame differs from the host reference"
    );
    let mailbox = cpu.mailbox();
    assert!(mailbox["presents"] >= 1);
    evidence(
        "hello-first-frame",
        &json!({
            "instructions": instructions,
            "wallMs": started.elapsed().as_millis(),
            "frameSha256": sha256(&rgba),
            "mailbox": mailbox.json(),
            "log": cpu.log(),
        }),
    );
}

#[test]
#[ignore = "needs firmware/dist (bun run firmware:build)"]
fn hello_buttons() {
    let cpu = session(HELLO);
    vsyncs(&cpu, 3);
    let center = ((HELLO_W - HELLO_SIZE) / 2, (HELLO_H - HELLO_SIZE) / 2);
    cpu.set_button("right", true).unwrap();
    vsyncs(&cpu, 5);
    cpu.set_button("right", false).unwrap();
    cpu.set_button("ok", true).unwrap();
    vsyncs(&cpu, 2);
    cpu.set_button("ok", false).unwrap();
    vsyncs(&cpu, 3);
    healthy(&cpu);
    let mailbox = cpu.mailbox();
    // Each held frame moves 6 px; sampling may see 4..=6 held frames.
    let rgba = panel(&cpu);
    let moved = (4..=6)
        .map(|frames| (center.0 + 6 * frames, center.1))
        .find(|&square| rgba == hello_reference(square, [255, 64, 64, 255]));
    assert!(
        moved.is_some(),
        "square did not move right and turn red: {mailbox:?}"
    );
    assert_eq!(pixel(&rgba, 0, 0), [255; 4]);
}

#[test]
#[ignore = "needs firmware/dist (bun run firmware:build)"]
fn hello_endurance_and_checkpoint() {
    let cpu = session(HELLO);
    let started = Instant::now();
    // 120 guest seconds at 30 Hz.
    let instructions = vsyncs(&cpu, 3600);
    healthy(&cpu);
    let mailbox = cpu.mailbox();
    let ticks = cpu.run_state().cpu.virtual_ticks;
    assert!(ticks >= 3600 * frame_ticks(), "only {ticks} guest ticks");
    assert_eq!(mailbox["overruns"], 0);
    let samples = cpu.work_samples();

    let file = checkpoint_file::save(&*cpu, Default::default()).unwrap();
    let resumed = session(HELLO);
    checkpoint_file::restore(&*resumed, &file).unwrap();
    for cpu in [&cpu, &resumed] {
        cpu.set_button("down", true).unwrap();
        vsyncs(cpu, 4);
        cpu.set_button("down", false).unwrap();
        vsyncs(cpu, 2);
    }
    assert_eq!(panel(&cpu), panel(&resumed), "checkpoint resume diverged");
    assert_eq!(cpu.mailbox(), resumed.mailbox());
    evidence(
        "hello-endurance",
        &json!({
            "guestSeconds": ticks as f64 / 54e6,
            "instructions": instructions,
            "wallMs": started.elapsed().as_millis(),
            "workP95": percentile(&samples, 0.95),
            "workMax": mailbox["workMax"],
            "checkpointBytes": file.len(),
            "mailbox": mailbox.json(),
        }),
    );
}

// ---- M2/M3: PocketJS image (image "pocket", counter app) ----------------

const POCKET: &str = "pocket";

/// Boot until the first app frame reached the panel.
fn pocket_booted() -> (Guest, u64) {
    let cpu = session(POCKET);
    let instructions = until(&cpu, 4_000_000_000, |m| m["presents"] >= 1);
    // One more capture puts the first presented frame on the panel.
    let instructions = instructions + vsyncs(&cpu, 1);
    healthy(&cpu);
    (cpu, instructions)
}

#[test]
#[ignore = "needs firmware/dist (bun run firmware:build)"]
fn pocket_first_frame() {
    let started = Instant::now();
    let (cpu, instructions) = pocket_booted();
    let wall = started.elapsed();
    let mailbox = cpu.mailbox();
    let rgba = panel(&cpu);
    let png = save_png("pocket-first-frame", &rgba);
    eprintln!("{mailbox:#?}\n{}\n{}", cpu.log(), png.display());
    assert_eq!(mailbox["appSource"], 0, "booted the embedded app");
    // bg-slate-900 (#0f172a) fills the corners.
    assert_eq!(pixel(&rgba, 2, 2), [0x0f, 0x17, 0x2a, 255]);
    // Golden picture of the embedded counter app; FUJI_UPDATE_GOLDEN=1 after
    // an intended app/PocketJS/firmware change rewrites it.
    let fixtures = root().join("crates/xp4/tests/fixtures");
    let golden = fixtures.join("custom-first-frame.json");
    let digest = sha256(&rgba);
    if std::env::var_os("FUJI_UPDATE_GOLDEN").is_some() {
        fs::copy(&png, fixtures.join("custom-first-frame.png")).unwrap();
        fs::write(
            &golden,
            serde_json::to_vec_pretty(&json!({ "frameSha256": digest })).unwrap(),
        )
        .unwrap();
    }
    let expected: Value = serde_json::from_slice(&fs::read(&golden).unwrap()).unwrap();
    assert_eq!(
        expected["frameSha256"],
        digest,
        "first frame differs from {}",
        golden.display()
    );
    evidence(
        "pocket-first-frame",
        &json!({
            "instructions": instructions,
            "guestMs": cpu.run_state().cpu.virtual_ticks as f64 / 54e3,
            "bootWorkTicks": mailbox["bootWork"],
            "wallMs": wall.as_millis(),
            "frameSha256": sha256(&rgba),
            "mailbox": mailbox.json(),
            "log": cpu.log(),
        }),
    );
}

fn app_bundle(app: &str) -> (Vec<u8>, Vec<u8>) {
    let apps = dist().join(manifest()["apps"]["index"].as_str().unwrap());
    let index: Value = serde_json::from_slice(&read(&apps)).unwrap();
    let file = |key: &str| read(&apps.with_file_name(index["apps"][app][key].as_str().unwrap()));
    (file("javaScript"), file("pack"))
}

/// Hold a button across two captures, then release it for two.
fn press(cpu: &Guest, button: &str) {
    cpu.set_button(button, true).unwrap();
    vsyncs(cpu, 2);
    cpu.set_button(button, false).unwrap();
    vsyncs(cpu, 2);
}

fn load(cpu: &Guest, app: &str) -> u32 {
    let (js, pak) = app_bundle(app);
    let generation = cpu.load_app(&js, &pak);
    until(cpu, 4_000_000_000, |m| {
        m["appGeneration"] == generation && m["reloadRequest"] == 0
    });
    until(cpu, 200_000_000, |m| m["presents"] >= 1);
    let presents = cpu.word("presents");
    until(cpu, 200_000_000, |m| m["presents"] > presents);
    vsyncs(cpu, 1);
    healthy(cpu);
    generation
}

#[test]
#[ignore = "needs firmware/dist (bun run firmware:build)"]
fn pocket_buttons() {
    let (cpu, _) = pocket_booted();
    let before = panel(&cpu);
    press(&cpu, "ok");
    let m = cpu.mailbox();
    assert_eq!(
        (m.action_name.as_str(), m.action_value()),
        ("count", 1),
        "{m:?}"
    );
    press(&cpu, "ok");
    assert_eq!(cpu.mailbox().action_value(), 2);
    let counted = panel(&cpu);
    assert_ne!(before, counted, "the count did not repaint");
    save_png("pocket-counter-2", &counted);
    // RIGHT moves focus to Reset; OK resets.
    press(&cpu, "right");
    press(&cpu, "ok");
    let m = cpu.mailbox();
    assert_eq!(
        (
            m.action_name.as_str(),
            m.action_value(),
            m["actionSequence"]
        ),
        ("count", 0, 3),
        "{m:?}"
    );
    healthy(&cpu);
}

/// Panel-pixel centre of the region filled with `color`.
fn centre(rgba: &[u8], color: [u8; 3]) -> Contact {
    let (mut x0, mut y0, mut x1, mut y1) = (usize::MAX, usize::MAX, 0, 0);
    for (index, px) in rgba.as_chunks::<4>().0.iter().enumerate() {
        if px[..3] == color {
            let (x, y) = (index % PANEL_WIDTH, index / PANEL_WIDTH);
            (x0, y0, x1, y1) = (x0.min(x), y0.min(y), x1.max(x), y1.max(y));
        }
    }
    assert!(x0 <= x1, "no {color:02x?} region on the panel");
    Contact {
        x: ((x0 + x1) / 2) as u16,
        y: ((y0 + y1) / 2) as u16,
    }
}

fn tap(cpu: &Guest, at: Contact, held_vsyncs: u32) {
    cpu.set_touch([Some(at), None]).unwrap();
    if held_vsyncs > 0 {
        vsyncs(cpu, held_vsyncs);
    }
    cpu.set_touch([None, None]).unwrap();
    vsyncs(cpu, 3);
}

#[test]
#[ignore = "needs firmware/dist (bun run firmware:build)"]
fn pocket_touch() {
    let (cpu, _) = pocket_booted();
    let before = panel(&cpu);
    // +1 starts focused (bg-blue-500); Reset is bg-slate-700.
    let plus = centre(&before, [0x3b, 0x82, 0xf6]);
    let reset = centre(&before, [0x33, 0x41, 0x55]);
    tap(&cpu, plus, 2);
    let m = cpu.mailbox();
    assert_eq!(
        (m.action_name.as_str(), m.action_value()),
        ("count", 1),
        "{m:?}\n{}",
        cpu.log()
    );
    // A press shorter than a frame is still one down and one up report.
    tap(&cpu, plus, 0);
    assert_eq!(cpu.mailbox().action_value(), 2);
    save_png("pocket-touch-2", &panel(&cpu));
    tap(&cpu, reset, 2);
    let m = cpu.mailbox();
    assert_eq!((m.action_value(), m["actionSequence"]), (0, 3), "{m:?}");
    // A drag off the button before release is not a press.
    cpu.set_touch([Some(plus), None]).unwrap();
    vsyncs(&cpu, 2);
    cpu.set_touch([Some(Contact { x: 10, y: 10 }), None])
        .unwrap();
    vsyncs(&cpu, 2);
    cpu.set_touch([None, None]).unwrap();
    vsyncs(&cpu, 3);
    assert_eq!(cpu.mailbox()["actionSequence"], 3);
    assert!(
        !cpu.log().contains("touch panel did not answer"),
        "{}",
        cpu.log()
    );
    healthy(&cpu);
}

#[test]
#[ignore = "needs firmware/dist (bun run firmware:build)"]
fn pocket_scene() {
    let (cpu, _) = pocket_booted();
    let before = panel(&cpu);
    let hole = centre(&before, [0x01, 0x00, 0x08]);
    let plus = centre(&before, [0x3b, 0x82, 0xf6]);
    let scene = &protocol()["scene"];
    let (width, height) = (
        number(&scene["maxWidth"]) as u32,
        number(&scene["maxHeight"]) as u32,
    );
    let mut rgb = vec![0u8; (width * height * 3) as usize];
    for pixel in rgb.chunks_exact_mut(3) {
        pixel.copy_from_slice(&[0xd0, 0x20, 0x20]);
    }
    cpu.set_scene_rgb(number(&scene["address"]) as u32, width, height, &rgb, 1)
        .unwrap();
    vsyncs(&cpu, 4);
    let after = panel(&cpu);
    save_png("pocket-scene", &after);
    assert_eq!(
        pixel(&after, hole.x as usize, hole.y as usize),
        [0xd0, 0x20, 0x20, 255],
        "viewfinder did not show the host scene"
    );
    assert_eq!(
        &pixel(&after, plus.x as usize, plus.y as usize)[..3],
        &[0x3b, 0x82, 0xf6],
        "scene punch overwrote the UI"
    );
    // A second host frame must replace the hole without leaving chroma
    // (the old path recopied #010008 into the scanout buffer first).
    for pixel in rgb.chunks_exact_mut(3) {
        pixel.copy_from_slice(&[0x20, 0xd0, 0x40]);
    }
    cpu.set_scene_rgb(number(&scene["address"]) as u32, width, height, &rgb, 2)
        .unwrap();
    vsyncs(&cpu, 4);
    let next = panel(&cpu);
    assert_eq!(
        pixel(&next, hole.x as usize, hole.y as usize),
        [0x20, 0xd0, 0x40, 255],
        "viewfinder did not refresh on a new generation"
    );
    assert_ne!(
        &pixel(&next, hole.x as usize, hole.y as usize)[..3],
        &[0x01, 0x00, 0x08],
        "chroma key leaked into the scanout buffer"
    );
    healthy(&cpu);
}

#[test]
#[ignore = "needs firmware/dist (bun run firmware:build)"]
fn pocket_hot_reload() {
    let (cpu, _) = pocket_booted();
    let counter = panel(&cpu);
    let started = Instant::now();
    let generation = load(&cpu, "dashboard");
    let reload_wall = started.elapsed();
    let m = cpu.mailbox();
    assert_eq!((m["appGeneration"], m["appSource"]), (generation, 1));
    let dashboard = panel(&cpu);
    assert_ne!(counter, dashboard);
    save_png("pocket-dashboard", &dashboard);
    press(&cpu, "down");
    press(&cpu, "ok");
    let m = cpu.mailbox();
    assert_eq!(
        (m.action_name.as_str(), m.action_value()),
        ("recipe", 1),
        "{m:?}"
    );
    save_png("pocket-dashboard-selected", &panel(&cpu));
    let reload_work = m["bootWork"];
    // Back to the counter from the slot: a fresh runtime, count starts at 0.
    let generation = load(&cpu, "counter");
    assert_eq!(cpu.word("appGeneration"), generation);
    press(&cpu, "ok");
    let m = cpu.mailbox();
    assert_eq!(
        (m.action_name.as_str(), m.action_value()),
        ("count", 1),
        "{m:?}"
    );
    evidence(
        "pocket-hot-reload",
        &json!({
            "dashboardBootWorkTicks": reload_work,
            "dashboardBootGuestMs": f64::from(reload_work) / 54e3,
            "reloadWallMs": reload_wall.as_millis(),
            "heapPeak": m["heapPeak"],
            "heapUsedAfterTwoReloads": m["heapUsed"],
            "mailbox": m.json(),
            "log": cpu.log(),
        }),
    );
}

/// 120 guest seconds of the animated dashboard with scripted input: frame
/// budget, heap stability, host throughput, and checkpoint determinism.
#[test]
#[ignore = "needs firmware/dist (bun run firmware:build)"]
fn pocket_endurance_and_checkpoint() {
    let (cpu, _) = pocket_booted();
    // The counter's idle frames: the static-UI cost.
    vsyncs(&cpu, 60);
    let idle = cpu.work_samples();
    let idle = idle[idle.len() - 30..].to_vec();
    load(&cpu, "dashboard");
    let heap_after_load = cpu.word("heapUsed");
    let script = [
        "down", "down", "ok", "up", "ok", "down", "down", "down", "ok",
    ];
    let started = Instant::now();
    let before = cpu.mailbox();
    let start_vsyncs = before["vsyncs"];
    let start_ticks = cpu.run_state().cpu.virtual_ticks;
    let mut samples = Vec::new();
    let mut heap_max = 0;
    for second in 0..120 {
        if second % 10 == 0 {
            press(&cpu, script[second / 10 % script.len()]);
            vsyncs(&cpu, 26);
        } else {
            vsyncs(&cpu, 30);
        }
        let m = cpu.mailbox();
        heap_max = heap_max.max(m["heapUsed"]);
        let ring = cpu.work_samples();
        samples.extend_from_slice(&ring[ring.len() - 30.min(ring.len())..]);
    }
    let wall = started.elapsed();
    healthy(&cpu);
    let m = cpu.mailbox();
    let frames = m["vsyncs"] - start_vsyncs;
    let guest_seconds = (cpu.run_state().cpu.virtual_ticks - start_ticks) as f64 / 54e6;
    assert!(guest_seconds >= 119.0, "{guest_seconds}");
    let budget = frame_ticks() as u32;
    let p95 = percentile(&samples, 0.95);
    assert!(
        p95 < budget / 2,
        "interactive p95 {p95} ticks exceeds half the frame"
    );
    let overruns = m["overruns"] - before["overruns"];
    let missed = m["missedVsyncs"] - before["missedVsyncs"];
    // Steady frames fit the budget; a recipe selection (OK) may take one
    // extra frame (one repeated picture), never a lost capture.
    let selections = (0..120)
        .filter(|s| s % 10 == 0 && script[s / 10 % script.len()] == "ok")
        .count();
    let over: Vec<u32> = samples.iter().copied().filter(|&w| w > budget).collect();
    assert!(
        over.len() <= selections && over.iter().all(|&w| w < 2 * budget) && missed == 0,
        "overruns {over:?} for {selections} selections, missed {missed}\n{}",
        cpu.log()
    );
    save_png("pocket-dashboard-endurance", &panel(&cpu));

    let file = checkpoint_file::save(&*cpu, Default::default()).unwrap();
    let resumed = session(POCKET);
    checkpoint_file::restore(&*resumed, &file).unwrap();
    for cpu in [&cpu, &resumed] {
        press(cpu, "up");
        press(cpu, "ok");
        vsyncs(cpu, 30);
    }
    assert_eq!(panel(&cpu), panel(&resumed), "checkpoint resume diverged");
    assert_eq!(cpu.mailbox(), resumed.mailbox());
    evidence(
        "pocket-endurance",
        &json!({
            "guestSeconds": guest_seconds,
            "frames": frames,
            "wallMs": wall.as_millis(),
            "hostFps": f64::from(frames) / wall.as_secs_f64(),
            "realtimeFactor": guest_seconds / wall.as_secs_f64(),
            "frameBudgetTicks": budget,
            "idle": { "p50": percentile(&idle, 0.5), "p95": percentile(&idle, 0.95), "max": idle.iter().max() },
            "interactive": {
                "samples": samples.len(),
                "p50": percentile(&samples, 0.5),
                "p95": p95,
                "p99": percentile(&samples, 0.99),
                "max": samples.iter().max(),
                "p95BudgetShare": f64::from(p95) / f64::from(budget),
            },
            "overruns": overruns,
            "overrunTicks": over,
            "selections": selections,
            "missedVsyncs": missed,
            "heap": { "afterLoad": heap_after_load, "maxDuringRun": heap_max, "end": m["heapUsed"], "peak": m["heapPeak"], "blocks": m["heapBlocks"] },
            "checkpointBytes": file.len(),
            "mailbox": m.json(),
        }),
    );
}

#[test]
#[ignore = "needs firmware/dist (bun run firmware:build)"]
fn pocket_deterministic() {
    let run = || {
        let (cpu, instructions) = pocket_booted();
        press(&cpu, "ok");
        press(&cpu, "ok");
        (instructions, sha256(&panel(&cpu)), cpu.mailbox())
    };
    let (first, second) = (run(), run());
    assert_eq!(first, second);
    evidence(
        "pocket-deterministic",
        &json!({ "bootInstructions": first.0, "frameSha256": first.1 }),
    );
}

/// Sample core 0's PC every `step` instructions for `frames` captures.
fn sample_pcs(cpu: &Guest, frames: u32, step: u32) -> Vec<u32> {
    let target = cpu.word("vsyncs") + frames;
    let mut pcs = Vec::new();
    while cpu.word("vsyncs") < target {
        let state = cpu.execute_batch(step).unwrap();
        assert!(!cpu.stopped(&state), "{}", cpu.stopped_reason(&state));
        pcs.push(cpu.pc(0).unwrap());
    }
    pcs
}

/// PC samples for `firmware/profile.ts`: steady dashboard frames and a
/// recipe selection (the heaviest input frame).
#[test]
#[ignore = "diagnostic"]
fn pocket_profile() {
    let (cpu, _) = pocket_booted();
    load(&cpu, "dashboard");
    vsyncs(&cpu, 30);
    let steady = sample_pcs(&cpu, 60, 2_000);
    press(&cpu, "down");
    cpu.set_button("ok", true).unwrap();
    let select = sample_pcs(&cpu, 4, 2_000);
    cpu.set_button("ok", false).unwrap();
    let dir = root().join(".cache/custom-firmware");
    for (name, pcs) in [("steady", steady), ("select", select)] {
        let text: String = pcs.iter().map(|pc| format!("{pc:#x}\n")).collect();
        fs::write(dir.join(format!("profile-{name}.pcs")), text).unwrap();
    }
}
