use fuji_boost_xp4::{
    Xp4Session, checkpoint_file,
    input::{BUTTONS, press_button, rotate_dial},
};
use fujimod_firmware::sha256;
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    time::Instant,
};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}
fn fixture() -> Value {
    serde_json::from_str(include_str!("fixtures/xt4-rec.json")).unwrap()
}
fn digest(name: &str) -> String {
    fixture()[name].as_str().unwrap().into()
}
fn firmware() -> Vec<u8> {
    let bytes = fs::read(root().join("FWUP0024.DAT")).unwrap();
    assert_eq!(sha256(&bytes), digest("firmwareSha256"));
    bytes
}
fn evidence(name: &str, value: &Value) {
    let dir = root().join(".cache/xt4-tests");
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join(format!("{name}.json")),
        serde_json::to_vec_pretty(value).unwrap(),
    )
    .unwrap();
}
fn healthy(state: &Value) {
    for v in [
        &state["cpu"]["failure"],
        &state["cpu"]["abort_failure"],
        &state["error"],
        &state["guestHalt"],
    ] {
        assert!(v.is_null(), "Guest stopped: {v}");
    }
}
fn number(state: &Value, field: &str) -> u64 {
    let value = state["cpu"][field].as_u64().unwrap();
    assert!(
        value <= 9_007_199_254_740_991,
        "Counter exceeds JSON integer precision"
    );
    value
}
fn progress(before: u64, state: &Value, budget: u64) {
    healthy(state);
    let instructions = number(state, "instructions");
    assert!(
        instructions >= before && instructions - before <= budget,
        "Guest exceeded budget"
    );
    assert!(
        instructions <= 2_500_000_000,
        "REC instruction limit reached"
    );
}
fn advance(cpu: &Xp4Session) -> Value {
    let before = number(&cpu.state(), "instructions");
    let state = cpu.execute(1_000_000).unwrap();
    healthy(&state);
    assert!(
        number(&state, "instructions") > before
            && number(&state, "instructions") - before <= 1_000_000
    );
    state
}
fn frame(cpu: &Xp4Session) -> String {
    sha256(&cpu.frame_rgba(1).unwrap())
}
fn matches(cpu: &Xp4Session, hash: &str) -> bool {
    let state = cpu.state();
    let output = state["outputs"]
        .as_array()
        .unwrap()
        .iter()
        .find(|o| o["output"] == 0)
        .or_else(|| state["outputs"].as_array().unwrap().first());
    output.is_some_and(|o| {
        let bytes = cpu
            .frame_rgba(o["output"].as_u64().unwrap() as u32)
            .unwrap();
        assert_eq!(
            bytes.len(),
            o["width"].as_u64().unwrap() as usize * o["height"].as_u64().unwrap() as usize * 4
        );
        o["output"] == 1 && o["width"] == 900 && o["height"] == 600 && sha256(&bytes) == hash
    })
}
fn input(button: &str) -> u32 {
    match button {
        "up" => 0x9f,
        "down" => 0xa0,
        "ok" => 0xf,
        _ => panic!("Unknown fixture button"),
    }
}
fn rec(source: &[u8], measured: bool) -> Xp4Session {
    let cpu = if measured {
        Xp4Session::discover_linked_dma_input_measured(source)
    } else {
        Xp4Session::discover_linked_dma_input(source)
    }
    .unwrap();
    rec_from(cpu, measured)
}
fn rec_from(cpu: Xp4Session, measured: bool) -> Xp4Session {
    for id in BUTTONS {
        cpu.set_virtual_button(id, false).unwrap();
    }
    let f = fixture();
    let mut state = cpu.state();
    let mut batches = 0;
    progress(number(&state, "instructions"), &state, 0);
    let mut run = |state: &mut Value| {
        assert!(batches < 2500 && number(state, "instructions") + 1_000_000 <= 2_500_000_000);
        let before = number(state, "instructions");
        batches += 1;
        *state = cpu.execute(1_000_000).unwrap();
        progress(before, state, 1_000_000);
    };
    for (i, step) in f["wizard"].as_array().unwrap().iter().enumerate() {
        let mut extra = 0;
        while number(&state, "instructions") < step["instructions"].as_u64().unwrap()
            || !matches(&cpu, step["sha256"].as_str().unwrap())
        {
            if number(&state, "instructions") >= step["instructions"].as_u64().unwrap() {
                assert!(extra < 32, "Wizard frame {} missing", i + 1);
                extra += 1;
            }
            run(&mut state);
        }
        eprintln!("wizard {}: {}", i + 1, number(&state, "instructions"));
        let before = number(&state, "instructions");
        assert!(before + 97_000_000 <= 2_500_000_000);
        let id = input(step["button"].as_str().unwrap());
        state = press_button(&cpu, id).unwrap();
        progress(before, &state, 97_000_000);
        for id in BUTTONS {
            assert_eq!(state["virtualButtons"]["pressed"][id.to_string()], false);
        }
        let bytes = state["virtualButtons"]["samples"][id.to_string()]["bytes"]
            .as_array()
            .unwrap();
        assert_eq!(bytes[1], 0);
        assert_eq!(bytes[2], 0);
    }
    let mut prior: Vec<String> = [
        "graySha256",
        "blankLiveViewSha256",
        "startupRecSha256",
        "startupLiveViewSha256",
        "startupExposureSha256",
        "preLiveRecSha256",
    ]
    .iter()
    .map(|k| digest(k))
    .collect();
    prior.extend(
        f["wizard"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| s["sha256"].as_str().unwrap().to_string()),
    );
    let target = digest(if measured {
        "measuredRecSha256"
    } else {
        "recSha256"
    });
    for batch in 0..=1600 {
        if let Ok(bytes) = cpu.frame_rgba(1) {
            let hash = sha256(&bytes);
            if if measured {
                matches(&cpu, &target)
            } else {
                !prior.contains(&hash)
            } {
                if !matches(&cpu, &target) {
                    capture(&cpu, "unexpected-rec");
                }
                assert!(matches(&cpu, &target), "Unexpected REC candidate {hash}");
                if measured
                    || number(&state, "instructions")
                        >= f["settledRecInstructions"].as_u64().unwrap()
                {
                    evidence(
                        if measured { "rec-measured" } else { "rec" },
                        &json!({"passed":true,"instructions":number(&state,"instructions"),"ticks":number(&state,"virtual_ticks"),"batches":batches,"gestures":9,"sha256":hash,"syntheticInputs":true,"normalRECProven":false}),
                    );
                    return cpu;
                }
            }
        }
        assert!(batch < 1600, "REC image missing");
        run(&mut state);
    }
    unreachable!()
}
fn restored() -> Xp4Session {
    let cpu = Xp4Session::discover_linked_dma_input(&firmware()).unwrap();
    checkpoint_file::restore(
        &cpu,
        &fs::read(root().join(".cache/xt4-rec.checkpoint")).unwrap(),
    )
    .unwrap();
    cpu
}
fn capture(cpu: &Xp4Session, name: &str) -> Value {
    let state = cpu.state();
    healthy(&state);
    assert!(
        state["outputs"]
            .as_array()
            .unwrap()
            .iter()
            .any(|o| o["output"] == 1 && o["width"] == 900 && o["height"] == 600)
    );
    let bytes = cpu.frame_rgba(1).unwrap();
    assert_eq!(bytes.len(), 900 * 600 * 4);
    fs::create_dir_all(root().join(".cache/xt4-tests")).unwrap();
    fs::write(root().join(format!(".cache/xt4-tests/{name}.rgba")), &bytes).unwrap();
    evidence(&format!("{name}-state"), &state);
    evidence(&format!("{name}-devices"), &cpu.diagnostics().unwrap());
    json!({"name":name,"instructions":number(&state,"instructions"),"ticks":number(&state,"virtual_ticks"),"sha256":sha256(&bytes)})
}
fn wait_menu(cpu: &Xp4Session, accepts: impl Fn(&[u8]) -> bool) -> Vec<u8> {
    for batch in 0..=400 {
        let bytes = cpu.frame_rgba(1).unwrap();
        if bytes.len() == 900 * 600 * 4 && accepts(&bytes) {
            return bytes;
        }
        assert!(batch < 400, "MENU missing within 400M instructions");
        let before = number(&cpu.state(), "instructions");
        let state = cpu.execute(1_000_000).unwrap();
        progress(before, &state, 1_000_000);
    }
    unreachable!()
}
// PNG chunks use the compression and CRC implementation already used by checkpoints.
fn png(path: &Path, rgba: &[u8]) {
    use flate2::{Compression, Crc, write::ZlibEncoder};
    use std::io::Write;
    assert_eq!(rgba.len(), 900 * 600 * 4);
    let mut bytes = b"\x89PNG\r\n\x1a\n".to_vec();
    let mut chunk = |kind: &[u8; 4], data: &[u8]| {
        bytes.extend_from_slice(&(data.len() as u32).to_be_bytes());
        bytes.extend_from_slice(kind);
        bytes.extend_from_slice(data);
        let mut crc = Crc::new();
        crc.update(kind);
        crc.update(data);
        bytes.extend_from_slice(&crc.sum().to_be_bytes());
    };
    let mut header = Vec::from(900u32.to_be_bytes());
    header.extend_from_slice(&600u32.to_be_bytes());
    header.extend_from_slice(&[8, 6, 0, 0, 0]);
    chunk(b"IHDR", &header);
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::fast());
    for row in rgba.chunks_exact(900 * 4) {
        encoder.write_all(&[0]).unwrap();
        encoder.write_all(row).unwrap();
    }
    chunk(b"IDAT", &encoder.finish().unwrap());
    chunk(b"IEND", &[]);
    fs::write(path, bytes).unwrap();
}
#[test]
#[ignore = "REC checkpoint, every MENU tab down to MY and back"]
fn menu_tabs() {
    let cpu = restored();
    let dir = root().join(".cache/mode5/menu-tabs");
    fs::create_dir_all(&dir).unwrap();
    let [up, down, left, right, ok, back] = BUTTONS;
    let tabs = ["movie", "iq", "af-mf", "audio", "tc", "setup", "my"];
    let menu_hash = digest("menuSha256");
    let steps = [
        (ok, "menu".to_owned(), None),
        (left, "tabs-movie".to_owned(), Some(0)),
    ]
    .into_iter()
    .chain(tabs.iter().enumerate().skip(1).flat_map(|(n, tab)| {
        let mut steps = vec![(down, format!("down-{tab}"), Some(n))];
        if *tab == "setup" {
            // The stock checkpoint has empty MY; add Recording Mode through the guest UI.
            steps.extend([
                (right, "setup-rows".to_owned(), None),
                (ok, "setup-item".to_owned(), None),
                (down, "setup-user-1".to_owned(), None),
                (down, "setup-user-2".to_owned(), None),
                (down, "setup-user-3".to_owned(), None),
                (down, "setup-user-4".to_owned(), None),
                (down, "setup-user-5".to_owned(), None),
                (down, "setup-user-6".to_owned(), None),
                (ok, "setup-my-config".to_owned(), None),
                (ok, "setup-my-add".to_owned(), None),
                (ok, "setup-my-select".to_owned(), None),
                (ok, "setup-my-confirm".to_owned(), None),
                (back, "setup-back".to_owned(), None),
                (back, "setup-user-back".to_owned(), None),
                (back, "setup-rows-back".to_owned(), None),
                (left, "setup-tabs".to_owned(), Some(n)),
            ]);
        }
        steps
    }))
    .chain(
        tabs.iter()
            .enumerate()
            .take(6)
            .rev()
            .map(|(n, tab)| (up, format!("up-{tab}"), Some(n))),
    );
    let mut results = Vec::new();
    for (index, (button, label, tab)) in steps.enumerate() {
        let before = cpu.frame_rgba(1).unwrap();
        let start = cpu.run_state().cpu.instructions;
        healthy(&press_button(&cpu, button).unwrap());
        let mut last = before.clone();
        let mut stable = 0;
        let mut settled = false;
        for _ in 0..400 {
            let state = cpu.execute_batch(1_000_000).unwrap();
            if state.stopped() {
                break;
            }
            let next = match cpu.frame_rgba(1) {
                Ok(next) => next,
                Err(error) if error == "No active guest frame for this output" => {
                    stable = 0;
                    continue;
                }
                Err(error) => panic!("{error}"),
            };
            // A transient live-view frame can stay unchanged for 30M instructions.
            let selected = tab.map_or_else(
                || {
                    if label.starts_with("setup-") {
                        next != before
                    } else {
                        sha256(&next) == menu_hash
                    }
                },
                |tab| {
                    let at = ((108 + 68 * tab) * 900 + 105) * 4;
                    next[at..at + 4] == [220, 220, 220, 255]
                        // Setup uses blue and MY purple; shooting tabs use red.
                        && next[at + 12..at + 15] != [0, 0, 0]
                },
            );
            if next == last && next != before && selected {
                stable += 1;
            } else {
                stable = 0;
                last = next;
            }
            // Leaving Setup renders the movie UI before its handoff accepts another key.
            let settle = if label == "down-my" || label == "up-tc" {
                250
            } else {
                30
            };
            if stable == settle {
                settled = true;
                break;
            }
        }
        let suffix = if settled { "" } else { "-FAILED" };
        png(&dir.join(format!("{index:02}-{label}{suffix}.png")), &last);
        results.push(json!({"label":label,"button":button,"changed":last!=before,
            "settled":settled,"instructions":cpu.run_state().cpu.instructions-start,
            "sha256":sha256(&last)}));
        fs::write(
            dir.join("steps.json"),
            serde_json::to_vec_pretty(&results).unwrap(),
        )
        .unwrap();
        if !settled {
            fs::write(dir.join("failure-state.json"), cpu.state().to_string()).unwrap();
            fs::write(
                dir.join("failure-devices.json"),
                cpu.diagnostics().unwrap().to_string(),
            )
            .unwrap();
        }
        healthy(&cpu.state());
        assert!(
            settled,
            "{label}: LCD did not change and settle within 400M instructions after the key ACK"
        );
        eprintln!("{label}: {}", results.last().unwrap());
    }
    assert_eq!(results.len(), 30);
    cpu.restore_checkpoint(&cpu.checkpoint().unwrap()).unwrap();
    fs::write(
        dir.join("devices.json"),
        cpu.diagnostics().unwrap().to_string(),
    )
    .unwrap();
}
// RIGHT only opens settings; OK is reserved for MENU and the explicit F-Log case.
struct SetupCrawl {
    cpu: Xp4Session,
    dir: PathBuf,
    steps: Vec<Value>,
    route: Vec<(u32, String, String)>,
    live_header: String,
}
impl SetupCrawl {
    fn new(name: &str) -> Self {
        let dir = root().join(".cache/setup-crawl").join(name);
        fs::create_dir_all(&dir).unwrap();
        let cpu = restored();
        let live_header = sha256(&cpu.frame_rgba(1).unwrap()[..900 * 48 * 4]);
        Self {
            cpu,
            live_header,
            dir,
            steps: Vec::new(),
            route: Vec::new(),
        }
    }
    fn failure(&self) {
        fs::write(
            self.dir.join("failure-state.json"),
            self.cpu.state().to_string(),
        )
        .unwrap();
        fs::write(
            self.dir.join("failure-devices.json"),
            self.cpu.diagnostics().unwrap().to_string(),
        )
        .unwrap();
        fs::write(
            self.dir.join("failure.checkpoint"),
            checkpoint_file::save(&self.cpu, Default::default()).unwrap(),
        )
        .unwrap();
        for address in [
            0x40000, 0x50000, 0x60000, 0x70000, 0x1100000, 0x1180000, 0x1270000, 0x1f30000,
        ] {
            fs::write(
                self.dir.join(format!("memory-{address:x}.bin")),
                self.cpu.read_virtual_memory(0, address, 65536).unwrap(),
            )
            .unwrap();
        }
    }
    fn key(&mut self, key: u32, label: &str, expected: Option<&str>, gap: u32) -> bool {
        let expected = if expected.is_none() && label == "connection-00-child-00-open" {
            Some("pairing-page")
        } else {
            expected
        };
        let before = self.cpu.frame_rgba(1).unwrap();
        let live_before = if expected == Some("live-view") {
            self.cpu.diagnostics().unwrap()["devices"]["xp4-movdc1"]["frames"]
                .as_u64()
                .unwrap()
        } else {
            0
        };
        let start = self.cpu.run_state().cpu.instructions;
        let ack = press_button(&self.cpu, key).unwrap();
        let mut last = before.clone();
        let mut stable = 0;
        let mut answered = false;
        let mut retried = false;
        if ack["error"].is_null() && !self.cpu.run_state().stopped() {
            for batch in 0..=800 {
                let state = self.cpu.execute_batch(1_000_000).unwrap();
                if state.stopped() {
                    break;
                }
                if expected.is_some() && batch < gap {
                    continue;
                }
                let Ok(next) = self.cpu.frame_rgba(1) else {
                    stable = 0;
                    continue;
                };
                if next == last {
                    stable += 1;
                } else {
                    stable = 0;
                    last = next;
                }
                answered = if expected == Some("pairing-page") {
                    // Pairing animates; require its new heading and rendered black/white QR panel.
                    let mut black = 0;
                    let mut white = 0;
                    for y in (330..540).step_by(6) {
                        for x in (640..850).step_by(6) {
                            let value = last[(y * 900 + x) * 4];
                            black += usize::from(value < 8);
                            white += usize::from(value >= 192);
                        }
                    }
                    batch >= 40
                        && batch >= gap
                        && last[..900 * 76 * 4] != before[..900 * 76 * 4]
                        && black > 50
                        && white > 50
                } else if expected == Some("live-view") {
                    if batch >= gap && sha256(&last[..900 * 48 * 4]) == self.live_header {
                        let d = self.cpu.diagnostics().unwrap();
                        d["devices"]["xp4-lcdc1"]["regs"][2].as_u64().unwrap() & 0x10 != 0
                            && d["devices"]["xp4-movdc1"]["frames"].as_u64().unwrap() > live_before
                    } else {
                        false
                    }
                } else {
                    expected.map_or(
                        last != before
                            // Large-indicator dialogs pause 35M between layout passes.
                            && stable >= 90
                            && batch >= 40
                            && (16..45).any(|y| (140..850).any(|x| last[(y * 900 + x) * 4] > 32)),
                        |hash| sha256(&last) == hash && batch >= gap,
                    )
                };
                if answered {
                    break;
                }
                if batch == 399 {
                    // A sampled press can be ignored during the guest's mode handoff.
                    // Repeat only an unchanged navigation key; the expected selector must then appear.
                    if expected.is_some() && last == before {
                        let retry = press_button(&self.cpu, key).unwrap();
                        if !retry["error"].is_null() || self.cpu.run_state().stopped() {
                            break;
                        }
                        retried = true;
                    } else {
                        answered = expected.is_none() && last == before;
                        break;
                    }
                }
            }
        }
        let file = format!("{:04}-{label}.png", self.steps.len());
        png(&self.dir.join(&file), &last);
        let changed = last != before;
        let hash = sha256(&last);
        let state = self.cpu.run_state();
        self.steps.push(json!({"label":label,"key":key,"png":file,"changed":changed,"answered":answered,
            "start":start,"ackEnd":ack["cpu"]["instructions"],"end":state.cpu.instructions,"ticks":state.cpu.virtual_ticks,"waitingAtAck":ack["cpu"]["waiting"],"gapM":gap,"retriedIgnoredKey":retried,
            "expected":expected,"ack":ack["virtualButtons"]["samples"][key.to_string()],"error":ack["error"],"sha256":hash}));
        fs::write(
            self.dir.join("steps.json"),
            serde_json::to_vec_pretty(&self.steps).unwrap(),
        )
        .unwrap();
        if !answered || state.stopped() || !ack["error"].is_null() {
            self.failure();
        }
        healthy(&ack);
        assert!(
            !state.stopped(),
            "{label}: guest stopped; see {}",
            self.dir.display()
        );
        assert!(
            answered,
            "{label}: LCD/selector did not answer within 400M after ACK (800M with one ignored-key retry)"
        );
        self.route.push((
            key,
            label.to_owned(),
            if matches!(expected, Some("live-view" | "pairing-page")) {
                expected.unwrap().into()
            } else {
                hash
            },
        ));
        eprintln!(
            "{} {} {label}: changed={changed} retry={retried}",
            self.dir.file_name().unwrap().to_string_lossy(),
            self.steps.len()
        );
        changed
    }
    fn nav(&mut self, key: u32, label: &str) {
        let changed = self.key(key, label, None, 0);
        if !changed {
            self.failure();
        }
        assert!(changed, "{label}: navigation ignored");
    }
    fn back_to(&mut self, hash: &str, label: &str) {
        assert!(self.key(BUTTONS[5], label, Some(hash), 40));
    }
    fn title(&self, page: bool) -> String {
        let b = self.cpu.frame_rgba(1).unwrap();
        sha256(
            &(10..65)
                .flat_map(|y| {
                    b[(y * 900 + 76) * 4..(y * 900 + if page { 899 } else { 640 }) * 4]
                        .iter()
                        .copied()
                })
                .collect::<Vec<_>>(),
        )
    }
    fn submenu(&mut self, label: &str, readonly: bool, skip_execute: bool) -> Value {
        let mut seen = Vec::new();
        let mut pages = std::collections::BTreeSet::new();
        for row in 0..64 {
            let hash = frame(&self.cpu);
            if seen.contains(&hash) {
                break;
            }
            assert!(row < 63, "{label}: submenu did not end/wrap");
            seen.push(hash.clone());
            pages.insert(self.title(true));
            if !readonly
                && !(skip_execute && row == 0)
                && self.key(BUTTONS[3], &format!("{label}-child-{row:02}-open"), None, 0)
            {
                if label == "connection-00" && row == 0 {
                    // Pairing Cancel returns to REC and resets the MENU stack.
                    self.key(
                        BUTTONS[5],
                        &format!("{label}-child-{row:02}-back"),
                        Some("live-view"),
                        40,
                    );
                    self.pairing_return(&hash);
                } else {
                    self.back_to(&hash, &format!("{label}-child-{row:02}-back"));
                }
            }
            if !self.key(BUTTONS[1], &format!("{label}-child-{row:02}-down"), None, 0) {
                break;
            }
        }
        json!({"row":label,"rows":seen.len(),"pages":pages.len(),"readonlyCheckboxes":readonly,"executeNotConfirmed":skip_execute})
    }
    fn pairing_return(&mut self, parent: &str) {
        let targets = self.route.clone();
        for label in ["menu", "tabs", "iq", "af", "audio", "tc", "setup"] {
            let (key, _, hash) = targets.iter().find(|(_, name, _)| name == label).unwrap();
            self.key(*key, &format!("pair-return-{label}"), Some(hash), 40);
        }
        let category = &targets
            .iter()
            .find(|(_, name, _)| name == "save-leave")
            .unwrap()
            .2;
        self.key(
            BUTTONS[3],
            "pair-return-connection-selector",
            Some(category),
            40,
        );
        let list = &targets
            .iter()
            .find(|(_, name, _)| name == "connection-enter")
            .unwrap()
            .2;
        self.key(BUTTONS[3], "pair-return-connection", Some(list), 40);
        self.key(BUTTONS[3], "pair-return-bluetooth", Some(parent), 40);
    }
    fn walk(&mut self) {
        let [_, down, left, right, ok, back] = BUTTONS;
        for (key, label) in [
            (ok, "menu"),
            (left, "tabs"),
            (down, "iq"),
            (down, "af"),
            (down, "audio"),
            (down, "tc"),
            (down, "setup"),
            (right, "setup-list"),
        ] {
            if label == "menu" {
                self.key(key, label, Some(&digest("menuSha256")), 40);
            } else {
                self.nav(key, label);
            }
        }
        let mut coverage = Vec::new();
        for (category, rows) in [
            ("user", 11),
            ("sound", 7),
            ("screen", 24),
            ("buttons", 18),
            ("power", 4),
            ("save", 10),
            ("connection", 8),
        ] {
            self.nav(right, &format!("{category}-enter"));
            let title = self.title(false);
            let mut pages = std::collections::BTreeSet::new();
            let mut submenus = Vec::new();
            for row in 0..rows {
                assert_eq!(
                    self.title(false),
                    title,
                    "{category}-{row}: crossed category early"
                );
                pages.insert(self.title(true));
                let hash = frame(&self.cpu);
                let label = format!("{category}-{row:02}");
                self.nav(right, &format!("{label}-open"));
                // Action pages stop before confirmation. Checkbox lists are inspected without toggling.
                let nested = matches!(
                    (category, row),
                    ("user", 5 | 6 | 7)
                        | ("screen", 16 | 19)
                        | ("buttons", 3 | 4 | 6 | 7 | 8 | 11 | 16 | 17)
                        | ("save", 2 | 8)
                        | ("connection", 0 | 1 | 2)
                );
                if nested {
                    submenus.push(self.submenu(
                        &label,
                        category == "screen" && row == 16,
                        category == "user" && row == 7,
                    ));
                }
                self.back_to(&hash, &format!("{label}-back"));
                self.nav(down, &format!("{label}-down"));
            }
            assert_ne!(
                self.title(false),
                title,
                "{category}: unvisited rows remain"
            );
            coverage.push(
                json!({"category":category,"rows":rows,"pages":pages.len(),"submenus":submenus}),
            );
            fs::write(
                self.dir.join("coverage.json"),
                serde_json::to_vec_pretty(&coverage).unwrap(),
            )
            .unwrap();
            self.nav(back, &format!("{category}-leave"));
        }
    }
    fn live_view(&mut self) {
        assert!(self.key(BUTTONS[5], "exit-setup", Some("live-view"), 40));
        for _ in 0..300 {
            assert!(!self.cpu.execute_batch(1_000_000).unwrap().stopped());
        }
        let before = self.cpu.diagnostics().unwrap()["devices"]["xp4-movdc1"]["frames"]
            .as_u64()
            .unwrap();
        for _ in 0..100 {
            assert!(!self.cpu.execute_batch(1_000_000).unwrap().stopped());
        }
        let after = self.cpu.diagnostics().unwrap();
        assert!(
            after["devices"]["xp4-movdc1"]["frames"].as_u64().unwrap() > before,
            "Live view DMA did not resume"
        );
        png(
            &self.dir.join("live-view.png"),
            &self.cpu.frame_rgba(1).unwrap(),
        );
        healthy(&self.cpu.state());
    }
}
#[test]
#[ignore = "REC checkpoint, all Setup rows/submenus, six timings, F-Log exit"]
fn setup_crawl() {
    let _ = fs::remove_file(root().join(".cache/setup-crawl/passed.json"));
    // Preserve queued browser inputs, then finish the prior BACK before probing MENU.
    for (name, trace, early, ready, menu) in [
        (
            "queued-350",
            include_str!("fixtures/xt4-menu-queued-350.jsonl"),
            "bab50967b5eeccd8b710d0d0f98dd43107374f68bf7e5503b035c3d7972588f3",
            "bab50967b5eeccd8b710d0d0f98dd43107374f68bf7e5503b035c3d7972588f3",
            "763d26b11dd4d2801d136e44f4ebeacb6b59656f7a0d4887c2f8c047a4b0438a",
        ),
        (
            "queued-400",
            include_str!("fixtures/xt4-menu-queued-400.jsonl"),
            "7d81938c6c5c799d9f90668463e759e782d7d7b71ec730e1f8b8032617f9f3ce",
            "704d24b852f463e33ae799d07f654646d76ccd6bad0c205ecddc41c4dd5589fc",
            "7d81938c6c5c799d9f90668463e759e782d7d7b71ec730e1f8b8032617f9f3ce",
        ),
    ] {
        let mut c = SetupCrawl::new(name);
        for line in trace.lines() {
            let command: Value = serde_json::from_str(line).unwrap();
            match command["type"].as_str().unwrap() {
                "load" => assert_eq!(command["sha256"], digest("firmwareSha256")),
                "checkpoint" => assert_eq!(
                    command["sha256"],
                    sha256(&fs::read(root().join(".cache/xt4-rec.checkpoint")).unwrap())
                ),
                "execute" => {
                    assert_eq!(command["mode"], "reprogram");
                    assert!(
                        !c.cpu
                            .execute_batch(command["budget"].as_u64().unwrap().try_into().unwrap())
                            .unwrap()
                            .stopped()
                    );
                }
                "button" => {
                    let index = ["up", "down", "left", "right", "ok", "back"]
                        .iter()
                        .position(|button| command["button"] == *button)
                        .unwrap();
                    healthy(&press_button(&c.cpu, BUTTONS[index]).unwrap());
                }
                _ => panic!("Unexpected queued-menu trace command"),
            }
        }
        assert_eq!(frame(&c.cpu), early);
        for _ in 0..400 {
            assert!(!c.cpu.execute_batch(1_000_000).unwrap().stopped());
        }
        assert_eq!(frame(&c.cpu), ready);
        png(
            &c.dir.join("before-menu.png"),
            &c.cpu.frame_rgba(1).unwrap(),
        );
        assert!(c.key(BUTTONS[4], "menu-after-queued-back", Some(menu), 40));
        assert_eq!(c.steps.last().unwrap()["retriedIgnoredKey"], false);
    }
    // Browser failure: this phase reaches TC while the audio producer is wrapping.
    let mut deferred = SetupCrawl::new("tc-deferred-prefix");
    while deferred.cpu.run_state().cpu.instructions < 1_689_520_606 {
        let remaining = 1_689_520_606 - deferred.cpu.run_state().cpu.instructions;
        assert!(
            !deferred
                .cpu
                .execute_batch(remaining.min(1_000_000) as u32)
                .unwrap()
                .stopped()
        );
    }
    for (key, target, label) in [
        (0xf, 1_930_270_606, "delayed-menu"),
        (0xa1, 2_224_270_606, "delayed-tabs"),
        (0xa0, 2_511_520_606, "delayed-iq"),
        (0xa0, 2_794_020_606, "delayed-af"),
        (0xa0, 3_068_020_606, "delayed-audio"),
    ] {
        if label == "delayed-menu" {
            deferred.key(key, label, Some(&digest("menuSha256")), 40);
        } else {
            deferred.nav(key, label);
        }
        while deferred.cpu.run_state().cpu.instructions < target {
            let remaining = target - deferred.cpu.run_state().cpu.instructions;
            assert!(
                !deferred
                    .cpu
                    .execute_batch(remaining.min(1_000_000) as u32)
                    .unwrap()
                    .stopped()
            );
        }
    }
    let prefix = checkpoint_file::save(&deferred.cpu, Default::default()).unwrap();
    for gap in [275u64, 600, 1200] {
        let mut c = SetupCrawl::new(&format!("tc-deferred-{gap}"));
        checkpoint_file::restore(&c.cpu, &prefix).unwrap();
        c.nav(0xa0, "delayed-tc");
        let target = c.steps.last().unwrap()["ackEnd"].as_u64().unwrap() + gap * 1_000_000;
        while c.cpu.run_state().cpu.instructions < target {
            let remaining = target - c.cpu.run_state().cpu.instructions;
            assert!(
                !c.cpu
                    .execute_batch(remaining.min(1_000_000) as u32)
                    .unwrap()
                    .stopped()
            );
        }
        c.key(
            0xa0,
            "setup-after-deferred-exit",
            Some("399d717d590c460379960c6b3a87ea355a31ff64da6661207b9edd86ce0cca8f"),
            40,
        );
        c.live_view();
    }
    // Exact user sequence with short gaps; acknowledge every electrical press,
    // then use BACK as the second-key responsiveness check if the UI filtered keys.
    let mut c = SetupCrawl::new("short-user-path");
    for (i, key) in [0xf, 0xa1, 0xa0, 0xa0, 0xa0, 0xa0, 0xa0]
        .into_iter()
        .enumerate()
    {
        let before = frame(&c.cpu);
        let start = c.cpu.run_state().cpu.instructions;
        let ack = press_button(&c.cpu, key).unwrap();
        healthy(&ack);
        assert!(!c.cpu.execute_batch(250_000).unwrap().stopped());
        let bytes = c.cpu.frame_rgba(1).unwrap();
        let file = format!("{i:04}-short-key.png");
        png(&c.dir.join(&file), &bytes);
        let state = c.cpu.run_state();
        c.steps.push(json!({"key":key,"png":file,"start":start,"end":state.cpu.instructions,
            "ticks":state.cpu.virtual_ticks,"waitingAtAck":ack["cpu"]["waiting"],"gapInstructions":250_000,
            "changed":sha256(&bytes)!=before,"sha256":sha256(&bytes),"ack":ack["virtualButtons"]["samples"][key.to_string()]}));
    }
    for _ in 0..400 {
        assert!(!c.cpu.execute_batch(1_000_000).unwrap().stopped());
    }
    png(&c.dir.join("user-path.png"), &c.cpu.frame_rgba(1).unwrap());
    assert!(c.key(BUTTONS[5], "user-path-back", Some("live-view"), 40));
    let mut movie = SetupCrawl::new("movie-flog");
    // F-Log exit draws the capture overlay (4K29.97/remaining time).
    movie.live_header = "5eea4a1ca5499f072083e7d0372cd63c172fb60e3c1359994f536782acf32049".into();
    for (i, key) in [
        0xf, 0xa0, 0xa0, 0xa0, 0xa0, 0xa0, 0xa2, 0xa0, 0xa0, 0xf, 0x11, 0x11,
    ]
    .into_iter()
    .enumerate()
    {
        if i == 11 {
            assert!(movie.key(key, &format!("movie-{i:02}"), Some("live-view"), 40));
        } else {
            movie.nav(key, &format!("movie-{i:02}"));
        }
    }
    for _ in 0..300 {
        assert!(!movie.cpu.execute_batch(1_000_000).unwrap().stopped());
    }
    let d = movie.cpu.diagnostics().unwrap();
    assert_eq!(d["devices"]["xp4-movdc0"]["regs"][2], 0x103);
    assert!(d["devices"]["xp4-movdc0"]["frames"].as_u64().unwrap() > 0);
    assert_ne!(
        d["devices"]["xp4-lcdc1"]["regs"][2].as_u64().unwrap() & 0x10,
        0
    );
    png(
        &movie.dir.join("live-view.png"),
        &movie.cpu.frame_rgba(1).unwrap(),
    );
    // Run the complete walk only after the targeted exit cases pass.
    let mut baseline = SetupCrawl::new("settled");
    baseline.walk();
    let route = baseline.route.clone();
    baseline.live_view();
    std::thread::scope(|threads| {
        for seed in [1u64, 7, 42, 20261106, 0x585434] {
            let route = &route;
            threads.spawn(move || {
                let mut c = SetupCrawl::new(&format!("seed-{seed}"));
                let mut random = seed;
                for (key, label, hash) in route {
                    random ^= random << 13;
                    random ^= random >> 7;
                    random ^= random << 17;
                    c.key(*key, label, Some(hash), (random % 201) as u32);
                }
                c.live_view();
            });
        }
    });
    // Retain audio-producer continuity long enough to catch a premature RX chain end.
    let mut held = SetupCrawl::new("tc-hold");
    for (key, label, hash) in route.iter().take(6) {
        held.key(*key, label, Some(hash), 100);
    }
    for _ in 0..1200 {
        assert!(!held.cpu.execute_batch(1_000_000).unwrap().stopped());
    }
    held.key(route[6].0, "setup-after-tc-hold", Some(&route[6].2), 40);
    held.live_view();
    fs::write(root().join(".cache/setup-crawl/passed.json"),json!({"passed":true,"seeds":[1,7,42,20261106,0x585434],"routeSteps":route.len(),"topLevelRows":82}).to_string()).unwrap();
}

#[test]
#[ignore = "official DAT, cold boot"]
fn rec_ui() {
    rec(&firmware(), false);
}
#[test]
#[ignore = "official DAT, measured cold boot and keys"]
fn rec_measured() {
    let cpu = rec(&firmware(), true);
    let before = capture(&cpu, "measured-stills");
    let mut keys = Vec::new();
    let mut previous = before.clone();
    for button in ["ok", "down"] {
        let state = press_button(&cpu, input(button)).unwrap();
        let immediate = capture(&cpu, &format!("measured-{button}"));
        let mut batches = 0;
        while batches < 400
            && frame(&cpu)
                != digest(if button == "ok" {
                    "menuSha256"
                } else {
                    "menuDownSha256"
                })
        {
            advance(&cpu);
            batches += 1;
        }
        let settled = capture(&cpu, &format!("measured-{button}-settled"));
        keys.push(json!({"button":button,"error":state["error"],"immediateChanged":immediate["sha256"]!=previous["sha256"],"changed":settled["sha256"]!=previous["sha256"],"immediate":immediate,"settled":settled,"settleInstructions":batches*1_000_000,"samples":state["virtualButtons"]["samples"]}));
        previous = settled;
    }
    evidence(
        "measured-keys",
        &json!({"before":before,"keys":keys,"maximumSettleInstructionsPerKey":400_000_000}),
    );
}
fn frames(cpu: &Xp4Session, state: &Value) -> String {
    state["outputs"]
        .as_array()
        .unwrap()
        .iter()
        .map(|o| {
            let id = o["output"].as_u64().unwrap() as u32;
            format!("{id}:{}", sha256(&cpu.frame_rgba(id).unwrap()))
        })
        .collect::<Vec<_>>()
        .join(",")
}
fn continuation(cpu: &Xp4Session) -> Vec<String> {
    (0..20)
        .map(|_| {
            let state = cpu.execute(1_000_000).unwrap();
            healthy(&state);
            format!(
                "{}/{}/{}",
                number(&state, "instructions"),
                sha256(state.to_string().as_bytes()),
                frames(cpu, &state)
            )
        })
        .collect()
}
fn verify_checkpoint(expected: &Value) {
    let start = Instant::now();
    let cpu = restored();
    let state = cpu.state();
    assert_eq!(sha256(state.to_string().as_bytes()), expected["state"]);
    assert_eq!(frames(&cpu, &state), expected["frame"]);
    assert!(
        expected["frame"]
            .as_str()
            .unwrap()
            .contains(&format!("1:{}", digest("recSha256")))
    );
    assert_eq!(json!(continuation(&cpu)), expected["continuation"]);
    evidence(
        "checkpoint-load",
        &json!({"passed":true,"loadAndContinuationMs":start.elapsed().as_millis(),"continuationBatches":20}),
    );
}
#[test]
#[ignore = "official DAT, generate and restore REC file"]
fn checkpoint_full() {
    let cpu = rec(&firmware(), false);
    let state = cpu.state();
    let start = Instant::now();
    let file=checkpoint_file::save(&cpu,json!({"factory":"linked_dma_input","firmwareSha256":digest("firmwareSha256"),"instructions":number(&state,"instructions"),"frame":digest("recSha256")}).as_object().unwrap().clone()).unwrap();
    fs::create_dir_all(root().join(".cache")).unwrap();
    fs::write(root().join(".cache/xt4-rec.checkpoint"), &file).unwrap();
    let expected = json!({"state":sha256(state.to_string().as_bytes()),"frame":frames(&cpu,&state),"continuation":continuation(&cpu)});
    fs::write(
        root().join(".cache/xt4-rec.continuation.json"),
        expected.to_string(),
    )
    .unwrap();
    evidence(
        "checkpoint-save",
        &json!({"passed":true,"fileBytes":file.len(),"saveAndContinuationMs":start.elapsed().as_millis()}),
    );
    drop(cpu);
    verify_checkpoint(&expected);
}
#[test]
#[ignore = "existing REC checkpoint"]
fn checkpoint_load() {
    verify_checkpoint(
        &serde_json::from_slice(
            &fs::read(root().join(".cache/xt4-rec.continuation.json")).unwrap(),
        )
        .unwrap(),
    );
}
#[test]
#[ignore = "REC checkpoint, live view and AE"]
fn scene() {
    let cpu = restored();
    let initial = cpu.state();
    assert!(matches(&cpu, &digest("recSha256")));
    let gray = cpu.frame_rgba(1).unwrap();
    cpu.set_scene_gray().unwrap();
    for (w, h, n) in [(0, 1, 0), (1, 8193, 0), (2, 2, 11)] {
        assert!(cpu.set_scene_rgb(w, h, &vec![0; n]).is_err());
    }
    cpu.set_scene_analytic().unwrap();
    assert_eq!(cpu.state(), initial, "Host source leaked into guest state");
    let mut previous = (initial["outputs"][0]["sequence"].clone(), sha256(&gray));
    let mut analytic = Vec::new();
    for _ in 0..20 {
        let state = advance(&cpu);
        let output = state["outputs"]
            .as_array()
            .unwrap()
            .iter()
            .find(|o| o["output"] == 1)
            .unwrap();
        assert_eq!(output["width"], 900);
        assert_eq!(output["height"], 600);
        if output["sequence"] == previous.0 {
            continue;
        }
        let hash = frame(&cpu);
        if previous.1 != digest("recSha256") && hash != digest("recSha256") && hash != previous.1 {
            analytic = vec![previous.1, hash];
            break;
        }
        previous = (output["sequence"].clone(), hash);
    }
    assert_eq!(
        analytic.len(),
        2,
        "Two changing analytic frames missing within 20 batches"
    );
    let bytes = cpu.frame_rgba(1).unwrap();
    assert!(
        (47..553).any(|y| (0..900).any(|x| {
            let i = (y * 900 + x) * 4;
            bytes[i] > 200
                && bytes[i + 1] < 60
                && bytes[i + 2] < 60
                && gray[i] == gray[i + 1]
                && gray[i] == gray[i + 2]
        })),
        "Missing saturated red replacing gray"
    );
    let mut green = vec![0; 64 * 48 * 3];
    for i in (1..green.len()).step_by(3) {
        green[i] = 255;
    }
    cpu.set_scene_rgb(64, 48, &green).unwrap();
    let mut centre = Vec::new();
    for _ in 0..20 {
        advance(&cpu);
        let bytes = cpu.frame_rgba(1).unwrap();
        centre = bytes[(300 * 900 + 450) * 4..(300 * 900 + 450) * 4 + 3].to_vec();
        if centre[1] > 200
            && i32::from(centre[1]) > i32::from(centre[0]) + 40
            && i32::from(centre[1]) > i32::from(centre[2]) + 40
        {
            break;
        }
    }
    assert!(
        centre[1] > 200
            && i32::from(centre[1]) > i32::from(centre[0]) + 40
            && i32::from(centre[1]) > i32::from(centre[2]) + 40,
        "RGB centre did not turn green"
    );
    evidence(
        "scene",
        &json!({"passed":true,"grayHash":sha256(&gray),"analyticFrames":analytic,"centre":centre}),
    );
    drop(cpu);
    let mut results = Vec::new();
    for (name, value) in [("black", 0), ("white", 255)] {
        let cpu = restored();
        let before = cpu.diagnostics().unwrap();
        cpu.set_scene_rgb(64, 48, &vec![value; 64 * 48 * 3])
            .unwrap();
        let mut history = Vec::new();
        for batch in 1..=60 {
            let state = advance(&cpu);
            let d = cpu.diagnostics().unwrap();
            let stats = &d["devices"]["imaging-statistics-sram"];
            let bytes: Vec<u8> = stats["bytes"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_u64().unwrap() as u8)
                .collect();
            history.push(json!({"batch":batch,"instructions":number(&state,"instructions"),"sensor":d["devices"]["imaging-sensor"],"statisticsFrames":stats["frames"],"statisticsHash":sha256(&bytes)}));
        }
        let d = cpu.diagnostics().unwrap();
        let stats = &d["devices"]["imaging-statistics-sram"];
        assert!(
            stats["frames"].as_u64().unwrap()
                > before["devices"]["imaging-statistics-sram"]["frames"]
                    .as_u64()
                    .unwrap()
        );
        let bytes: Vec<u8> = stats["bytes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_u64().unwrap() as u8)
            .collect();
        fs::write(
            root().join(format!(".cache/xt4-tests/ae-{name}-statistics.bin")),
            &bytes,
        )
        .unwrap();
        capture(&cpu, &format!("ae-{name}"));
        let result = json!({"name":name,"before":{"sensor":before["devices"]["imaging-sensor"],"statisticsFrames":before["devices"]["imaging-statistics-sram"]["frames"]},"sensor":d["devices"]["imaging-sensor"],"statisticsHash":sha256(&bytes),"history":history});
        evidence(&format!("ae-{name}"), &result);
        results.push(result);
    }
    assert_ne!(
        results[0]["statisticsHash"], results[1]["statisticsHash"],
        "Black/white statistics did not differ"
    );
    let dark = results[0]["sensor"]["sensor"]["registers"]
        .as_object()
        .unwrap();
    let bright = &results[1]["sensor"]["sensor"]["registers"];
    let changed: Vec<_> = dark
        .iter()
        .filter(|(k, v)| *v != &bright[*k])
        .map(|(k, v)| json!({"register":k,"dark":v,"bright":bright[k]}))
        .collect();
    evidence("ae-comparison", &json!({"batches":60,"changed":changed}));
    assert!(
        !changed.is_empty(),
        "Firmware sensor programming did not react within 60 batches"
    );
}
#[test]
#[ignore = "REC checkpoint, front command dial"]
fn dials() {
    let cpu = restored();
    let before = capture(&cpu, "front-menu-rec");
    assert_eq!(before["sha256"], digest("recSha256"));
    healthy(&press_button(&cpu, 15).unwrap());
    wait_menu(&cpu, |b| sha256(b) == digest("menuSha256"));
    let menu = capture(&cpu, "front-menu-before");
    let mut detents = Vec::new();
    for i in 1..=4 {
        healthy(&rotate_dial(&cpu, "front_command_dial", 1).unwrap());
        detents.push(capture(&cpu, &format!("front-menu-cw-{i}")));
    }
    for _ in 0..400 {
        if frame(&cpu) != menu["sha256"] {
            break;
        }
        advance(&cpu);
    }
    let settled = capture(&cpu, "front-menu-settled");
    let changed = detents.iter().any(|f| f["sha256"] != menu["sha256"])
        || settled["sha256"] != menu["sha256"];
    evidence(
        "front-menu-result",
        &json!({"passed":changed,"before":before,"menu":menu,"detents":detents,"settled":settled}),
    );
    assert!(changed, "Front dial did not change LCD");
}
#[test]
#[ignore = "120 guest seconds"]
fn endurance() {
    let cpu = restored();
    let initial = cpu.run_state();
    let mut state = cpu.run_state();
    let started = Instant::now();
    let mut batches = 0;
    while state.cpu.virtual_ticks - initial.cpu.virtual_ticks < 120 * 54_000_000 {
        let before = state;
        state = cpu.execute_batch(1_000_000).unwrap();
        assert!(
            !state.stopped(),
            "{}",
            serde_json::to_string(&state).unwrap()
        );
        assert!(
            state.cpu.instructions > before.cpu.instructions
                && state.cpu.instructions - before.cpu.instructions <= 1_000_000
        );
        assert!(state.cpu.virtual_ticks > before.cpu.virtual_ticks);
        for counter in [state.cpu.instructions, state.cpu.virtual_ticks] {
            assert!(
                counter <= 9_007_199_254_740_991,
                "Counter exceeds JSON integer precision"
            );
        }
        batches += 1;
        if batches % 1000 == 0 {
            eprintln!(
                "endurance {batches}: {:.1} guest s / {:.1} wall s",
                (state.cpu.virtual_ticks - initial.cpu.virtual_ticks) as f64 / 54_000_000.,
                started.elapsed().as_secs_f64()
            );
        }
    }
    evidence(
        "endurance",
        &json!({"passed":true,"batches":batches,"guestSeconds":(state.cpu.virtual_ticks-initial.cpu.virtual_ticks) as f64/54_000_000.,"instructions":state.cpu.instructions-initial.cpu.instructions,"wallSeconds":started.elapsed().as_secs_f64()}),
    );
}
#[test]
#[ignore = "modified DAT cold boot"]
fn modded() {
    let stock = firmware();
    let inverted = |s: &str| {
        s.encode_utf16()
            .flat_map(u16::to_le_bytes)
            .map(|b| !b)
            .collect::<Vec<_>>()
    };
    let old = inverted("视频设置");
    let replacement = inverted("视频设定");
    let offsets: Vec<_> = stock
        .windows(old.len())
        .enumerate()
        .filter_map(|(i, b)| (b == old).then_some(i))
        .collect();
    assert_eq!(offsets.len(), 1);
    let at = offsets[0];
    assert!(at > 0);
    let mut modified = stock.clone();
    modified[at..at + old.len()].copy_from_slice(&replacement);
    let sum = |b: &[u8]| b.iter().map(|b| u32::from(!b)).sum::<u32>();
    let check = u32::from_le_bytes(stock[0x20c..0x210].try_into().unwrap())
        .wrapping_add(sum(&old))
        .wrapping_sub(sum(&replacement));
    modified[0x20c..0x210].copy_from_slice(&check.to_le_bytes());
    assert_eq!(modified.len(), stock.len());
    assert_eq!(
        modified.iter().zip(&stock).filter(|(a, b)| a != b).count(),
        3
    );
    let dir = root().join(".cache/modded-firmware");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("FWUP0024-MENU.DAT"), &modified).unwrap();
    let mut damaged = modified.clone();
    damaged[at] ^= 1;
    let err = Xp4Session::discover_linked_dma_input(&damaged)
        .err()
        .unwrap();
    assert!(err.contains("DAT checksum differs at 0x20c"));
    let cpu = Xp4Session::discover_linked_dma_input(&modified).unwrap();
    let err = checkpoint_file::restore(
        &cpu,
        &fs::read(root().join(".cache/xt4-rec.checkpoint")).unwrap(),
    )
    .unwrap_err();
    assert!(
        err.contains("Checkpoint belongs to")
            && err.contains(&sha256(&stock))
            && err.contains(&sha256(&modified))
    );
    drop(cpu);
    let cpu = restored();
    healthy(&press_button(&cpu, 15).unwrap());
    let stock_menu = wait_menu(&cpu, |b| sha256(b) == digest("menuSha256"));
    capture(&cpu, "stock-menu");
    drop(cpu);
    let cpu = rec(&modified, false);
    capture(&cpu, "modded-rec");
    healthy(&press_button(&cpu, 15).unwrap());
    let changed_pixels = |b: &[u8]| -> Option<usize> {
        let mut n = 0;
        for (pixel, (a, b)) in b
            .chunks_exact(4)
            .zip(stock_menu.chunks_exact(4))
            .enumerate()
        {
            if a == b {
                continue;
            }
            let (x, y) = (pixel % 900, pixel / 900);
            if !(300..350).contains(&x) || !(15..60).contains(&y) {
                return None;
            }
            n += 1;
        }
        (n > 0).then_some(n)
    };
    let bytes = wait_menu(&cpu, |b| changed_pixels(b).is_some());
    capture(&cpu, "modded-menu");
    let result = json!({"passed":true,"firmwareSha256":sha256(&modified),"stringDatOffset":at,"checkpointError":err,"rec":{"atRec":{"menuSha256":sha256(&bytes),"changedPixels":changed_pixels(&bytes).unwrap(),"changesConfinedToTitleGlyph":true}}});
    evidence("modded-result", &result);
    fs::write(dir.join("result.json"), result.to_string()).unwrap();
}

fn touch_panel(cpu: &Xp4Session) -> Value {
    let devices = cpu.diagnostics().unwrap()["devices"].clone();
    let state = devices["xp4-touch-i2c"]["state"].clone();
    assert!(!state.is_null(), "touch panel missing: {devices}");
    let registers = state["panel"]["registers"].as_array().unwrap();
    json!({
        "power": [registers[1], registers[2]],
        "probe": registers[0xf6],
        "heartbeat": registers[0x12],
        "reported": state["panel"]["reported"],
    })
}

fn touch_task(cpu: &Xp4Session) -> Value {
    let state = cpu.read_physical_memory(0x1792c28, 0x4c).unwrap();
    let half = |at: usize| u16::from_le_bytes([state[at], state[at + 1]]);
    let word = |at: usize| u32::from_le_bytes(state[at..at + 4].try_into().unwrap());
    let slot = |at: usize| {
        json!({
            "id": state[at] & 0xf,
            "down": state[at] & 0x80 != 0,
            "x": half(at + 2),
            "y": half(at + 4),
            "stage": state[at + 6],
        })
    };
    json!({
        "selfCheck": state[0x20],
        "presses": word(0x24),
        "slots": [slot(0x30), slot(0x3c)],
    })
}

fn unit2_fallback(state: &Value) -> usize {
    state["frontier"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| (0xfff3f000..0xfff3f020).contains(&e["address"].as_u64().unwrap()))
        .count()
}

#[test]
#[ignore = "official DAT, measured cold boot with the touch panel model"]
fn stock_touch() {
    let cpu = Xp4Session::discover_linked_dma_input_touch(&firmware()).unwrap();
    for id in BUTTONS {
        cpu.set_virtual_button(id, false).unwrap();
    }
    let mut powered_at = None;
    let mut fallback = 0;
    for _ in 0..800 {
        let state = cpu.execute(1_000_000).unwrap();
        healthy(&state);
        fallback = unit2_fallback(&state);
        if touch_panel(&cpu)["power"] == json!([1, 1]) {
            powered_at = Some(number(&state, "instructions"));
            break;
        }
    }
    let powered_at = powered_at.expect("stock driver never powered the panel");
    assert_eq!(fallback, 0, "unit 2 still falling through to discovery");
    cpu.set_touch([Some(fuji_boost_xp4::Contact { x: 450, y: 300 }), None])
        .unwrap();
    for _ in 0..20 {
        advance(&cpu);
    }
    let held = touch_task(&cpu);
    assert_eq!(
        held["slots"][0],
        json!({"id": 1, "down": true, "x": 450, "y": 300, "stage": 1})
    );
    cpu.set_touch([None, None]).unwrap();
    for _ in 0..20 {
        advance(&cpu);
    }
    let released = touch_task(&cpu);
    assert_eq!(released["slots"][0]["down"], false);
    assert_eq!(released["slots"][0]["stage"], 3);
    let result = json!({
        "poweredAt": powered_at,
        "panel": touch_panel(&cpu),
        "held": held,
        "released": released,
    });
    evidence("stock-touch", &result);
    eprintln!("{result}");
}
