use fuji_boost_xp4::{Xp4Session, input::BUTTONS};
use fujimod_firmware::sha256;
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
};
fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}
fn firmware() -> Vec<u8> {
    let bytes = fs::read(root().join("FWUP0024.DAT")).unwrap();
    let f: Value = serde_json::from_str(include_str!("fixtures/xt4-rec.json")).unwrap();
    assert_eq!(sha256(&bytes), f["firmwareSha256"]);
    bytes
}
fn save(name: &str, value: &Value) {
    let dir = root().join(".cache/xt4-tests");
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join(format!("{name}.json")),
        serde_json::to_vec_pretty(value).unwrap(),
    )
    .unwrap();
}
fn healthy(s: &Value) {
    for v in [&s["cpu"]["failure"], &s["error"], &s["guestHalt"]] {
        assert!(v.is_null(), "{v}");
    }
}
fn startup(state: &Value, calibration: bool) {
    for key in [
        "inactiveCountdown",
        "startupGpio0Low",
        "latchedStartupCause",
        "virtualClock",
        "passiveDiscovery",
    ] {
        assert_eq!(state[key], true, "Startup declaration: {key}");
    }
    assert_eq!(state["flashIdentityBytes"], 1);
    assert_eq!(state["invalidClock"], false);
    assert_eq!(!state["calibrationFixture"].is_null(), calibration);
    assert_eq!(state["mainInitTrace"]["dropped"], 0);
    assert!(
        !state["mainInitTrace"]["records"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}
#[test]
#[ignore = "80M passive startup"]
fn long() {
    let cpu = Xp4Session::discover_preloaded(&firmware()).unwrap();
    let state = cpu.observe_main_initialization().unwrap();
    healthy(&state);
    startup(&state, false);
    assert_eq!(state["cpu"]["instructions"], 80_020_606u64);
    assert_eq!(state["passiveDiscovery"], true);
    assert_eq!(state["virtualClock"], true);
    assert_eq!(state["bootVerified"], false);
    assert_eq!(state["outputs"], json!([]));
    assert_eq!(state["interventions"], json!([]));
    let control = Xp4Session::discover_preloaded(&firmware()).unwrap();
    let mut baseline = control.state();
    for _ in 0..80 {
        baseline = control.execute(1_000_000).unwrap();
        healthy(&baseline);
    }
    baseline["diagnostics"] = control.diagnostics().unwrap();
    for key in [
        "cpu",
        "registers",
        "assumptions",
        "frontier",
        "outputs",
        "interventions",
    ] {
        assert_eq!(state[key], baseline[key], "Startup differs: {key}");
    }
    let diagnostics = cpu.diagnostics().unwrap();
    let peer = &diagnostics["devices"]["xp4-spi1"];
    assert_eq!(peer, &baseline["diagnostics"]["devices"]["xp4-spi1"]);
    assert_eq!(peer["input"]["rtc"]["committed"], true);
    assert_eq!(peer["input"]["rtc"]["counter"]["running"], true);
    assert!(
        state["frontier"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r["address"] == 0xff614670u64
                && r["write"] == false
                && r["kind"] == "unsupported"
                && r["value"] == 0x924
                && r["count"].as_u64().unwrap() > 1024)
    );
    let mut report = state;
    report["diagnostics"] = diagnostics;
    save("long", &report);
}
#[test]
#[ignore = "80M calibration trace, independent trace verifier"]
fn calibration() {
    let cpu = Xp4Session::discover_power2_bit8_echo(&firmware()).unwrap();
    let mut state = cpu.observe_main_initialization().unwrap();
    healthy(&state);
    startup(&state, true);
    assert_eq!(state["mainInitTrace"]["dropped"], 0);
    assert!(
        !state["mainInitTrace"]["records"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    state["diagnostics"] = cpu.diagnostics().unwrap();
    let control = Xp4Session::discover_power2_bit8_echo(&firmware()).unwrap();
    let mut baseline = control.observe_main_initialization().unwrap();
    baseline["diagnostics"] = control.diagnostics().unwrap();
    assert_eq!(
        state, baseline,
        "Independent native calibration traces differ"
    );
    save("calibration", &state);
    verify_calibration(&state);
}

// Preserve the original profile/audio verifier's assertions, including its exact tick fixture.
fn verify_calibration(s: &Value) {
    let expected: Value = serde_json::from_slice(
        &fs::read(root().join("crates/xp4/tests/fixtures/xt4-calibration-trace.json")).unwrap(),
    )
    .unwrap();
    let records = s["mainInitTrace"]["records"].as_array().unwrap();
    let at = |pc: u64| records.iter().filter(|r| r["pc"] == pc).collect::<Vec<_>>();
    let spi = &s["diagnostics"]["devices"]["xp4-spi1"]["input"];
    for key in [
        "power2Bit8Echo",
        "class3ReadInputs",
        "power2InitEcho",
        "power64ReadInputs",
        "controlRequestFixture",
        "power67Input",
        "power17Input",
        "rtcControlEcho",
        "powerInitReadInputs",
        "rtcStagingReadback",
        "power14Input",
        "datStorage",
        "calibrationFixture",
        "audioResetFixture",
    ] {
        assert_eq!(s[key], expected[key], "Calibration fixture differs: {key}");
    }
    assert_eq!(spi["word2_bit8_acks"], json!([]));
    assert_eq!(spi["word2_bit8_total_acks"], 0);
    assert_eq!(spi["word2_one_acks"], json!([]));
    for pc in [0x11d6504, 0x11d5b34, 0x11bfd44] {
        assert!(at(pc).is_empty(), "Unexpected trace point {pc:#x}");
    }
    assert!(!at(0x11d5b24).iter().any(|r| r["r0"] == 2));
    for r in at(0x11d539c) {
        assert!(
            ![47, 64, 65, 67, 17, 37, 38, 69, 70, 71, 29, 30].contains(&r["r0"].as_u64().unwrap())
        );
    }
    for register in [64, 65, 67, 17, 37, 38, 69, 70, 71, 14] {
        assert_eq!(spi["inputs"][register.to_string()], 0);
    }
    assert_eq!(spi["rtc"]["config"]["retained_control_echo"], true);
    assert_eq!(spi["rtc"]["config"]["staging_readback"], true);
    for text in [
        "Synthetic read-only power register67=0",
        "Synthetic read-only power register17=0",
        "Synthetic RTC staging readback",
        "all16 bits are deliberately selected zero",
        "Five raw DAT-backed ranges",
    ] {
        assert!(
            s["assumptions"]
                .as_array()
                .unwrap()
                .iter()
                .any(|a| a.as_str().unwrap().contains(text))
        );
    }
    assert_eq!(
        s["diagnostics"]["devices"]["xp4-control-requests"]["words"]["4294631552"],
        json!({"ticks":8,"value":0,"next":null})
    );
    assert_eq!(
        s["diagnostics"]["audioTaskState"],
        json!({"address":0x1af1840,"value":1})
    );
    assert_eq!(
        s["diagnostics"]["settingsData"]["directory"],
        expected["diagnostics"]["settingsData"]["directory"]
    );
    assert_eq!(
        s["diagnostics"]["settingsData"]["row0Column0"]["magic"],
        0xaa55
    );
    assert_eq!(s["cpu"]["instructions"], 80_020_606u64);
    healthy(s);
    assert_eq!(s["outputs"], json!([]));
    assert_eq!(s["interventions"], json!([]));
    assert_eq!(s["bootVerified"], false);
    assert_eq!(s["passiveDiscovery"], true);
    assert_eq!(s["virtualClock"], true);
    let model = &s["diagnostics"]["devices"]["xp4-banked-calibration-fixture"];
    assert_eq!(model["generation"], 22);
    assert_eq!(
        json!(
            model["units"]
                .as_array()
                .unwrap()
                .iter()
                .map(|u| u["stage"].clone())
                .collect::<Vec<_>>()
        ),
        json!(["Mode5Done", "Removed", "Removed", "Removed"])
    );
    assert!(
        model["units"]
            .as_array()
            .unwrap()
            .iter()
            .all(|u| u["pending"].is_null())
    );
    let pairs = |pc, a: &str, b: &str| {
        at(pc)
            .iter()
            .map(|r| json!([r[a], r[b]]))
            .collect::<Vec<_>>()
    };
    for pc in [
        0x11c19f0, 0x11c1a18, 0x11c1a28, 0x11c1a4c, 0x11c1ce4, 0x11c1fac,
    ] {
        assert_eq!(
            json!(pairs(pc, "r5", "r0")),
            json!([[1, 0], [2, 0], [3, 0]])
        );
    }
    assert_eq!(
        json!(pairs(0x11c33a8, "r0", "r1")),
        json!([[0, 5], [1, 5], [2, 5], [3, 5]])
    );
    assert_eq!(
        json!(pairs(0x11c1b20, "r4", "r0")),
        json!([[0, 0], [1, 0], [2, 0], [3, 0]])
    );
    assert_eq!(
        json!(
            at(0x1280c14)
                .iter()
                .filter(|r| r["lr"] == 0x11c1ce4)
                .map(|r| json!([r["r0"], r["r1"], r["r2"]]))
                .collect::<Vec<_>>()
        ),
        json!([
            [0x40000000u64, 0x40000000u64, 0],
            [0x80000000u64, 0x40000000u64, 0],
            [0xc0000000u64, 0x30000000u64, 0]
        ])
    );
    assert_eq!(
        json!(pairs(0x11c2c98, "r0", "r1")),
        json!(
            (1..=3)
                .map(|u| json!([u, 0x178ff10 + 0x24 + u * 24]))
                .collect::<Vec<_>>()
        )
    );
    let profile = at(0x129119c);
    assert_eq!(profile.len(), 1);
    let profile = profile[0];
    assert_eq!(profile["r0"], 0);
    let m = &profile["manager"];
    assert_eq!(
        json!([m["profile"], m["command"], m["activeMask"], m["demands"]]),
        json!([1, 5, 1, [1, 1, 1, 1, 1]])
    );
    for u in 1..=3 {
        assert_eq!(m["saved"][u]["unit"], u);
        let primary = &m["saved"][u]["primaryWords"];
        assert_eq!(primary.as_object().unwrap().len(), 6);
        for key in ["4660", "4664", "4480", "4484", "4488", "448c"] {
            assert_eq!(
                primary[key],
                model["units"][u]["primary"][u32::from_str_radix(key, 16).unwrap().to_string()]
            );
        }
        let paired = &model["units"][u]["paired"];
        assert_eq!(
            json!([
                paired[0x7114u32.to_string()],
                paired[0x7c40u32.to_string()],
                paired[0x7c14u32.to_string()],
                paired[0x7080u32.to_string()]
            ]),
            json!([0, 0x80000000u64, 0x80000000u64, 0x100])
        );
        assert_eq!(paired[0x7090u32.to_string()].as_u64().unwrap() & 1, 1);
    }
    let mut callbacks = at(0x126d450)
        .iter()
        .map(|r| {
            (
                r["r6"].as_u64().unwrap(),
                r["core"].as_u64().unwrap(),
                r["r0"].as_u64().unwrap(),
            )
        })
        .collect::<Vec<_>>();
    callbacks.sort();
    assert_eq!(callbacks, vec![(0x1291170, 1, 0), (0x1291184, 3, 0)]);
    let named = |name: &str| {
        s["diagnostics"]["records"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|r| r["name"] == name)
            .map(|r| {
                let fields = r["fields"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|pair| {
                        (
                            pair[0].as_str().unwrap().to_string(),
                            pair[1]
                                .as_object()
                                .unwrap()
                                .values()
                                .next()
                                .unwrap()
                                .clone(),
                        )
                    })
                    .collect::<serde_json::Map<_, _>>();
                Value::Object(fields)
            })
            .collect::<Vec<_>>()
    };
    let events = named("XP4 banked calibration fixture completed");
    let actual = events
        .iter()
        .map(|r| json!([r["unit"], r["phase"]]))
        .collect::<Vec<_>>();
    let mut phases = Vec::new();
    for u in 1..=3 {
        for phase in [
            "paired-kick",
            "paired-command3",
            "paired-command10",
            "controller",
            "finalization",
        ] {
            phases.push(json!([u, phase]));
        }
    }
    for u in 0..4 {
        phases.push(json!([u, "mode5"]));
    }
    for u in 1..=3 {
        phases.push(json!([u, "removal"]));
    }
    assert_eq!(actual, phases);
    let acks = named("XP4 control request acknowledged")
        .into_iter()
        .filter(|r| r["address"] == 0xff70c080u64)
        .collect::<Vec<_>>();
    assert_eq!(acks.len(), 3);
    for (i, ack) in acks.iter().enumerate() {
        assert_eq!(ack["request"], 2);
        let tick = ack["tick"].as_u64().unwrap();
        assert!(
            events[19 + i]["tick"].as_u64().unwrap() < tick
                && tick < profile["tick"].as_u64().unwrap()
        );
        if i < 2 {
            assert!(tick < events[20 + i]["tick"].as_u64().unwrap());
        }
    }
    let device = &s["diagnostics"]["devices"]["xp4-audio-reset-fixture"];
    let request = &device["stage"]["Done"];
    assert_eq!(device["ticks"], 8);
    assert_eq!(request["command"], 1);
    assert_eq!(request["token"], 1);
    let started = request["started"].as_u64().unwrap();
    let due = request["due"].as_u64().unwrap();
    assert_eq!(due, started + 8);
    assert_eq!(
        named("XP4 audio reset fixture scheduled"),
        vec![json!({"address":0xfff4f01cu64,"command":1,"tick":started,"due":due})]
    );
    assert_eq!(
        named("XP4 audio reset fixture completed"),
        vec![json!({"address":0xfff4f01cu64,"command":1,"tick":due})]
    );
    let entry = at(0x1256d70);
    let returned = at(0x125948c);
    assert_eq!(entry.len(), 1);
    assert_eq!(returned.len(), 1);
    assert_eq!(entry[0]["core"], 3);
    assert_eq!(entry[0]["lr"], 0x125948c);
    let returned_tick = returned[0]["tick"].as_u64().unwrap();
    assert!(entry[0]["tick"].as_u64().unwrap() < started && due < returned_tick);
    assert_eq!(returned[0]["r0"], 0);
    let config = at(0x12595b8);
    assert_eq!(config.len(), 1);
    assert_eq!(config[0]["r0"], 0);
    assert!(config[0]["tick"].as_u64().unwrap() > returned_tick);
    assert!(at(0x12595d4).is_empty() && at(0x125a358).is_empty());
    assert!(
        !s["frontier"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r["address"] == 0xfff4f01cu64 && r["kind"] == "unsupported")
    );
    assert_eq!(
        s["cpu"]["virtual_ticks"], 112_862_380u64,
        "Original calibration timing assertion"
    );
}

fn sample(cpu: &Xp4Session, state: &Value, batch: u32) -> Value {
    let frames = state["outputs"]
        .as_array()
        .unwrap()
        .iter()
        .map(|o| {
            format!(
                "{}:{}:{}",
                o["output"],
                o["sequence"],
                sha256(
                    &cpu.frame_rgba(o["output"].as_u64().unwrap() as u32)
                        .unwrap()
                )
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    json!({"batch":batch,"instructions":state["cpu"]["instructions"],"ticks":state["cpu"]["virtual_ticks"],"perCore":state["cpu"]["per_core"],"waiting":state["cpu"]["waiting"],"frames":frames,"frontier":state["frontier"].as_array().unwrap().len(),"milestones":state["cpu"]["history"]["milestones"].as_array().unwrap().len(),"halt":state["guestHalt"],"failure":state["cpu"]["failure"].as_str().or(state["error"].as_str())})
}
fn timeline(measured: bool) -> Value {
    let source = firmware();
    let cpu = if measured {
        Xp4Session::discover_linked_dma_input_measured(&source)
    } else {
        Xp4Session::discover_linked_dma_input(&source)
    }
    .unwrap();
    for id in BUTTONS {
        cpu.set_virtual_button(id, false).unwrap();
    }
    let f: Value = serde_json::from_str(include_str!("fixtures/xt4-rec.json")).unwrap();
    let mut known = std::collections::BTreeMap::new();
    known.insert(
        f["graySha256"].as_str().unwrap().to_string(),
        "gray".to_string(),
    );
    for (i, s) in f["wizard"].as_array().unwrap().iter().enumerate() {
        known.insert(
            s["sha256"].as_str().unwrap().into(),
            format!("wizard{}", i + 1),
        );
    }
    let mut state = cpu.state();
    let mut samples = vec![sample(&cpu, &state, 0)];
    let mut first = serde_json::Map::new();
    let mut stopped = "budget 800".to_string();
    for batch in 1..=800 {
        state = cpu.execute(1_000_000).unwrap();
        let s = sample(&cpu, &state, batch);
        for frame in s["frames"]
            .as_str()
            .unwrap()
            .split(',')
            .filter(|s| !s.is_empty())
        {
            let parts: Vec<_> = frame.split(':').collect();
            let name = format!(
                "{}:{}",
                parts[0],
                known
                    .get(parts[2])
                    .map(String::as_str)
                    .unwrap_or(&parts[2][..12])
            );
            first.entry(name).or_insert(s["instructions"].clone());
        }
        samples.push(s.clone());
        if !s["halt"].is_null() || !s["failure"].is_null() {
            stopped = format!("halt/failure: {s}");
            break;
        }
        if s["instructions"] == samples[samples.len() - 2]["instructions"] {
            stopped = "no progress".into();
            break;
        }
    }
    let frontier = state["frontier"].as_array().unwrap();
    json!({"factory":if measured {"linked_dma_input_measured"}else{"linked_dma_input"},"samples":samples,"firstFrames":first,"frontierTail":&frontier[frontier.len().saturating_sub(24)..],"milestones":state["cpu"]["history"]["milestones"],"stopped":stopped})
}
#[test]
#[ignore = "two 800M input timelines"]
fn compare_measured_inputs() {
    let base = timeline(false);
    save("inputs-catalog", &base);
    let measured = timeline(true);
    save("inputs-measured", &measured);
    let a = base["samples"].as_array().unwrap();
    let b = measured["samples"].as_array().unwrap();
    let first = a.iter().enumerate().position(|(i, s)| b.get(i) != Some(s));
    let at = |i: Option<usize>| {
        i.map(|i|json!({"catalog":a.get(i),"measured":b.get(i),"before":i.checked_sub(1).and_then(|i|a.get(i))}))
    };
    let only: Vec<_> = measured["frontierTail"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|m| {
            !base["frontierTail"]
                .as_array()
                .unwrap()
                .iter()
                .any(|b| b["address"] == m["address"] && b["pc"] == m["pc"])
        })
        .collect();
    save(
        "compare-measured-inputs",
        &json!({"batches":800,"identical":first.is_none()&&a.len()==b.len(),"firstDivergentBatch":first,"atDivergence":at(first),"catalog":base,"measured":measured,"measuredOnlyFrontier":only}),
    );
}
