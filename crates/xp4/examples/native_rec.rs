//! Measurement spike: native host JIT, unchanged checkpoint/guest and button ACKs.
//! Run from the repository root; checkpoint inflation is outside the timed region.
use fuji_boost_xp4::{Execution, Xp4Session};
use fujimod_firmware::sha256;
use serde_json::{Value, json};
use std::{
    fs,
    time::{Duration, Instant},
};

const BUTTONS: [u32; 6] = [0x9f, 0xa0, 0xa1, 0xa2, 0x0f, 0x11];
const REC: &str = "f2757c7a94e3315d43674be0b90b911b490010c7bca4be5dbb2eac7daac83372";
const MENU: &str = "0beb243fa5c23b761471cbc5c23d1c0966db417cbd5025f825f7f46e204a17a0";

fn restore(cpu: &Xp4Session) {
    fuji_boost_xp4::checkpoint_file::restore(cpu, &fs::read(".cache/xt4-rec.checkpoint").unwrap())
        .unwrap();
    assert_eq!(sha256(&cpu.frame_rgba(1).unwrap()), REC);
}

fn run(cpu: &Xp4Session, budget: u32) -> Execution {
    let state = cpu.execute_batch(budget).unwrap();
    assert!(
        !state.stopped(),
        "{}",
        serde_json::to_string(&state).unwrap()
    );
    state
}

fn capture(cpu: &Xp4Session) -> Value {
    let mut state = cpu.state();
    state["error"] = Value::Null;
    json!({"instructions":state["cpu"]["instructions"], "ticks":state["cpu"]["virtual_ticks"],
        "stateSha256":sha256(state.to_string().as_bytes()),
        "frames":state["outputs"].as_array().unwrap().iter().map(|o| {
            let id = o["output"].as_u64().unwrap() as u32;
            json!({"output":id,"sha256":sha256(&cpu.frame_rgba(id).unwrap())})
        }).collect::<Vec<_>>()})
}

fn scan(cpu: &Xp4Session, pressed: bool) {
    for _ in 0..128 {
        run(cpu, 250_000);
        let bytes = cpu.virtual_button_sample(15).unwrap();
        assert_eq!(bytes.len(), 8);
        assert!(bytes[1] <= 1 && bytes[2] <= 1);
        if bytes[1] == u8::from(pressed) && bytes[2] == u8::from(pressed) {
            return;
        }
    }
    panic!("button ACK timeout");
}

fn main() {
    let mode = std::env::args().nth(1).unwrap_or_else(|| "rec".into());
    assert!(["cold", "rec", "menu", "profile"].contains(&mode.as_str()));
    let firmware = fs::read("FWUP0024.DAT").unwrap();
    assert_eq!(
        sha256(&firmware),
        "853d273b5603d3a2b93493f0ebd6a762b0b4b6bdc70317777775ae5f8ed93bc5"
    );
    let cpu = Xp4Session::discover_linked_dma_input(&firmware).unwrap();
    for input in BUTTONS {
        cpu.set_virtual_button(input, false).unwrap();
    }
    if mode != "cold" {
        restore(&cpu);
    }
    let before = cpu.run_state();
    let mut state = cpu.run_state();
    let mut elapsed = Duration::ZERO;
    if mode == "menu" {
        // WARM=n runs n batches first, so the press measures a warmed translation cache.
        let warm: u32 = std::env::var("WARM").map_or(0, |v| v.parse().unwrap());
        for _ in 0..warm {
            run(&cpu, 1_000_000);
        }
        let start = Instant::now();
        for input in BUTTONS {
            cpu.set_virtual_button(input, false).unwrap();
        }
        scan(&cpu, false);
        cpu.set_virtual_button(15, true).unwrap();
        scan(&cpu, true);
        cpu.set_virtual_button(15, false).unwrap();
        scan(&cpu, false);
        state = run(&cpu, 1_000_000);
        for batch in 0..=400 {
            if sha256(&cpu.frame_rgba(1).unwrap()) == MENU {
                break;
            }
            assert!(batch < 400, "MENU timeout");
            state = run(&cpu, 1_000_000);
        }
        elapsed = start.elapsed();
    } else {
        let batches = match mode.as_str() {
            "cold" => 300,
            "profile" => 1000,
            _ => 100,
        };
        eprintln!("ready: {} {mode}", std::process::id());
        fs::write(".cache/native-spike/ready", std::process::id().to_string()).unwrap();
        for _ in 0..batches {
            let start = Instant::now();
            state = run(&cpu, 1_000_000);
            elapsed += start.elapsed();
        }
    }
    let instructions = state.cpu.instructions - before.cpu.instructions;
    let ticks = state.cpu.virtual_ticks - before.cpu.virtual_ticks;
    println!(
        "{}",
        json!({"mode":mode,"wallMs":elapsed.as_secs_f64()*1000.,"instructions":instructions,"ticks":ticks,
        "mips":instructions as f64 / elapsed.as_secs_f64() / 1e6,
        "guestSecondsPerWallSecond":ticks as f64 / 54_000_000. / elapsed.as_secs_f64(),"final":capture(&cpu)})
    );
}
