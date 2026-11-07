//! Replays a frozen host-call ledger; it does not reproduce or replace button debounce.
use fuji_boost_xp4::Xp4Session;
use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::fs::{self, File, OpenOptions};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::time::Instant;

const DAT_SHA: &str = "853d273b5603d3a2b93493f0ebd6a762b0b4b6bdc70317777775ae5f8ed93bc5";
const LEDGER_SHA: &str = "edd4fa39df3a015594b8bd9f83857342232b645ec91ce33287b2000f9b077ea7";
const LEDGER: &str = ".temp/research-archive/xt4-board/settings-origin/power2-bit8-echo/gpio9-high/power3-raw768/continued/interactive/sampled/navigation-replay/run-06-first-header-jit/schedule.jsonl";
const IDS: [u32; 6] = [0x9f, 0xa0, 0xa1, 0xa2, 0x0f, 0x11];

#[derive(Clone, Debug)]
enum Call {
    Input { id: u32, pressed: bool },
    Run { budget: u32 },
    Checkpoint { label: String },
}
#[derive(Clone, Debug)]
struct Row {
    phase: String,
    call: Call,
}
fn io<T>(result: std::io::Result<T>) -> Result<T, String> {
    result.map_err(|error| error.to_string())
}
fn hash(bytes: &[u8]) -> String {
    fujimod_firmware::sha256(bytes)
}
fn number(value: &Value, key: &str) -> Result<u64, String> {
    value[key]
        .as_u64()
        .ok_or_else(|| format!("Missing unsigned integer {key}"))
}
fn text<'a>(value: &'a Value, key: &str) -> Result<&'a str, String> {
    value[key]
        .as_str()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| format!("Missing string {key}"))
}
fn time(value: &Value) -> Result<(u64, u64), String> {
    Ok((number(value, "instructions")?, number(value, "ticks")?))
}
fn keys(value: &Value, expected: &[&str]) -> Result<(), String> {
    let actual: BTreeSet<_> = value
        .as_object()
        .ok_or("Ledger row must be an object")?
        .keys()
        .map(String::as_str)
        .collect();
    if actual != expected.iter().copied().collect() {
        return Err(format!("Unexpected ledger fields: {actual:?}"));
    }
    Ok(())
}
fn parse(source: &str) -> Result<Vec<Row>, String> {
    let mut rows = vec![];
    let (mut runs, mut inputs, mut checkpoints, mut requested) = (0_u64, 0_u64, 0_u64, 0_u64);
    let mut clock = (20606, 20606);
    let mut labels = BTreeSet::new();
    for (line, raw) in source.lines().enumerate() {
        let value: Value =
            serde_json::from_str(raw).map_err(|e| format!("Ledger line{}: {e}", line + 1))?;
        let phase = text(&value, "phase")?.to_owned();
        let call = match text(&value, "kind")? {
            "input" => {
                keys(
                    &value,
                    &[
                        "kind",
                        "index",
                        "phase",
                        "id",
                        "pressed",
                        "instructions",
                        "ticks",
                        "gpioBefore",
                        "gpioAfter",
                        "cpuAndRegistersUnchanged",
                    ],
                )?;
                inputs += 1;
                if number(&value, "index")? != inputs || time(&value)? != clock {
                    return Err("Input sequence/recorded clock mismatch".into());
                }
                let id = u32::try_from(number(&value, "id")?).map_err(|_| "Input ID overflow")?;
                let pressed = value["pressed"]
                    .as_bool()
                    .ok_or("pressed must be a boolean")?;
                if !IDS.contains(&id)
                    || (inputs <= 6 && (id != IDS[inputs as usize - 1] || pressed))
                {
                    return Err("Unknown input or changed initial release order".into());
                }
                if value["cpuAndRegistersUnchanged"] != true {
                    return Err("Ledger input changed CPU state".into());
                }
                for field in ["gpioBefore", "gpioAfter"] {
                    let words = value[field].as_array().ok_or("Missing GPIO array")?;
                    if words.len() != 16
                        || words.iter().any(|v| v.as_u64().is_none_or(|n| n > 65535))
                    {
                        return Err("Invalid GPIO metadata".into());
                    }
                }
                Call::Input { id, pressed }
            }
            "run" => {
                keys(
                    &value,
                    &["kind", "call", "phase", "requested", "before", "after"],
                )?;
                runs += 1;
                let budget =
                    u32::try_from(number(&value, "requested")?).map_err(|_| "Budget overflow")?;
                if number(&value, "call")? != runs
                    || !(1..=1_000_000).contains(&budget)
                    || time(&value["before"])? != clock
                {
                    return Err("Run sequence/budget/recorded clock mismatch".into());
                }
                let after = time(&value["after"])?;
                if after.0 < clock.0 || after.1 < clock.1 || after.0 - clock.0 > u64::from(budget) {
                    return Err("Recorded run exceeds budget or reverses time".into());
                }
                clock = after;
                requested += u64::from(budget);
                Call::Run { budget }
            }
            "checkpoint" => {
                keys(
                    &value,
                    &[
                        "kind",
                        "label",
                        "phase",
                        "instructions",
                        "ticks",
                        "failure",
                        "guestHalt",
                        "outputs",
                        "samples",
                        "fullSha256",
                    ],
                )?;
                checkpoints += 1;
                let label = text(&value, "label")?.to_owned();
                if !label
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-')
                    || !labels.insert(label.clone())
                    || time(&value)? != clock
                {
                    return Err("Invalid checkpoint label/recorded clock".into());
                }
                Call::Checkpoint { label }
            }
            unknown => return Err(format!("Unknown ledger kind {unknown}")),
        };
        rows.push(Row { phase, call });
    }
    if (runs, inputs, checkpoints, requested) != (1035, 78, 20, 908250000) {
        return Err("Frozen ledger call totals differ".into());
    }
    Ok(rows)
}
fn arguments(args: &[String]) -> Result<Option<(bool, PathBuf)>, String> {
    match args {
        [flag] if flag == "--self-check" => Ok(None),
        [out] if !out.is_empty() && !out.starts_with('-') => Ok(Some((false, PathBuf::from(out)))),
        [flag, out] if flag == "--owner-ack" && !out.is_empty() && !out.starts_with('-') => {
            Ok(Some((true, PathBuf::from(out))))
        }
        _ => Err(
            "Usage: replay_spi2 [--owner-ack] NEW_NATIVE_OUTPUT_DIRECTORY | --self-check".into(),
        ),
    }
}
fn self_check(source: &str) -> Result<(), String> {
    let rows = parse(source)?;
    assert_eq!(rows.len(), 1133);
    for (needle, replacement) in [
        ("\"kind\":\"input\"", "\"kind\":\"unknown\""),
        ("\"index\":1,", "\"index\":2,"),
        ("\"id\":159,", "\"id\":99,"),
        ("\"pressed\":false", "\"pressed\":\"false\""),
        ("\"call\":1,", "\"call\":2,"),
        ("\"requested\":1000000", "\"requested\":0"),
        ("\"requested\":1000000", "\"requested\":1000001"),
        ("\"kind\":\"input\"", "\"extra\":0,\"kind\":\"input\""),
    ] {
        assert!(source.contains(needle));
        assert!(parse(&source.replacen(needle, replacement, 1)).is_err());
    }
    assert!(parse(&source.lines().skip(1).collect::<Vec<_>>().join("\n")).is_err());
    let args =
        |values: &[&str]| arguments(&values.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>());
    assert_eq!(args(&["out"])?, Some((false, PathBuf::from("out"))));
    assert_eq!(
        args(&["--owner-ack", "out"])?,
        Some((true, PathBuf::from("out")))
    );
    assert_eq!(args(&["--self-check"])?, None);
    for invalid in [
        vec![],
        vec![""],
        vec!["--unknown"],
        vec!["--owner-ack"],
        vec!["--owner-ack", "--self-check"],
        vec!["--self-check", "out"],
        vec!["out", "--owner-ack"],
        vec!["--owner-ack", "out", "extra"],
        vec!["out", "extra"],
    ] {
        assert!(args(&invalid).is_err());
    }
    Ok(())
}
fn write_new(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let mut file = io(OpenOptions::new().write(true).create_new(true).open(path))?;
    io(file.write_all(bytes))
}
fn write_json(path: &Path, value: &Value) -> Result<(), String> {
    write_new(path, &serde_json::to_vec(value).map_err(|e| e.to_string())?)
}
fn clock(state: &Value) -> Result<(u64, u64), String> {
    Ok((
        number(&state["cpu"], "instructions")?,
        number(&state["cpu"], "virtual_ticks")?,
    ))
}
fn stopped(state: &Value) -> bool {
    !state["cpu"]["failure"].is_null() || !state["error"].is_null() || !state["guestHalt"].is_null()
}
fn micros(start: Instant) -> u64 {
    u64::try_from(start.elapsed().as_micros()).unwrap()
}
fn snapshot(
    session: &Xp4Session,
    out: &Path,
    stem: &str,
    phase: &str,
    started: Instant,
    last: Option<&Value>,
) -> Result<Value, String> {
    let state_raw = session.state().to_string();
    let mut full: Value = serde_json::from_str(&state_raw).map_err(|e| e.to_string())?;
    let diag = session.diagnostics()?;
    let diag_raw = serde_json::to_vec(&diag).map_err(|e| e.to_string())?;
    write_new(
        &out.join(format!("{stem}.state.json")),
        state_raw.as_bytes(),
    )?;
    write_new(&out.join(format!("{stem}.diagnostics.json")), &diag_raw)?;
    let mut frames = vec![];
    for frame in full["outputs"].as_array().ok_or("State outputs missing")? {
        let output =
            u32::try_from(number(frame, "output")?).map_err(|_| "Output index overflow")?;
        let rgba = session
            .frame_rgba(output)
            .map_err(|_| "Previously active frame became unavailable")?;
        let bytes = number(frame, "width")?
            .checked_mul(number(frame, "height")?)
            .and_then(|n| n.checked_mul(4))
            .ok_or("Frame size overflow")?;
        if u64::try_from(rgba.len()).map_err(|_| "Frame size overflow")? != bytes {
            return Err("Active RGBA length differs from dimensions".into());
        }
        let file = format!("{stem}.output{output}.rgba");
        write_new(&out.join(&file), &rgba)?;
        frames.push(json!({"file":file,"output":output,"sequence":frame["sequence"],"width":frame["width"],"height":frame["height"],"bytes":bytes,"sha256":hash(&rgba)}));
    }
    full["diagnostics"] = diag;
    let full_raw = serde_json::to_vec(&full).map_err(|e| e.to_string())?;
    write_new(&out.join(format!("{stem}.full.json")), &full_raw)?;
    if let Some(last) = last {
        write_json(&out.join(format!("{stem}.last-execute.json")), last)?;
    }
    Ok(
        json!({"stem":stem,"phase":phase,"elapsedMicros":micros(started),"instructions":full["cpu"]["instructions"],"ticks":full["cpu"]["virtual_ticks"],"failure":full["cpu"]["failure"],"guestHalt":full["guestHalt"],"stateSHA256":hash(state_raw.as_bytes()),"diagnosticsSHA256":hash(&diag_raw),"fullSHA256":hash(&full_raw),"frames":frames}),
    )
}
fn main() -> Result<(), String> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let ledger_raw = io(fs::read(root.join(LEDGER)))?;
    let dat = io(fs::read(root.join("FWUP0024.DAT")))?;
    if hash(&ledger_raw) != LEDGER_SHA || hash(&dat) != DAT_SHA {
        return Err("Exact frozen ledger / X-T4 2.12 DAT hash mismatch".into());
    }
    let ledger = std::str::from_utf8(&ledger_raw).map_err(|e| e.to_string())?;
    self_check(ledger)?;
    let rows = parse(ledger)?;
    let args: Vec<_> = std::env::args().skip(1).collect();
    let Some((owner_ack, out)) = arguments(&args)? else {
        println!(
            "Ledger/DAT/hash/parser/arguments self-check PASS: 1035 runs, 78 setters, 20 host checkpoints"
        );
        return Ok(());
    };
    let factory = if owner_ack {
        "Xp4Session::discover_spi2_owner_ack"
    } else {
        "Xp4Session::discover_spi2_second_header"
    };
    io(fs::create_dir(&out))?;
    let started = Instant::now();
    let binary = io(fs::read(io(std::env::current_exe())?))?;
    write_new(&out.join("source-schedule.jsonl"), &ledger_raw)?;
    write_json(
        &out.join("producer.json"),
        &json!({"backend":"native Rust/Unicorn","factory":factory,"datSHA256":DAT_SHA,"ledgerSHA256":LEDGER_SHA,"compiledRootSHA256":hash(include_bytes!("../src/lib.rs")),"compiledSpi2SHA256":hash(include_bytes!("../src/spi2.rs")),"compiledExampleSHA256":hash(include_bytes!("replay_spi2.rs")),"binarySHA256":hash(&binary),"hostOS":std::env::consts::OS,"hostArchitecture":std::env::consts::ARCH,"cameraAccessed":false,"browserAccessed":false,"reimplementsDebounce":false,"allExpectedRuns":1035,"allExpectedSetters":78,"expectedRequestedInstructions":908250000}),
    )?;
    let prepared = if owner_ack {
        Xp4Session::discover_spi2_owner_ack(&dat)
    } else {
        Xp4Session::discover_spi2_second_header(&dat)
    };
    let session = match prepared {
        Ok(session) => session,
        Err(error) => {
            write_json(
                &out.join("result.json"),
                &json!({"outcome":"prepare-error","error":error,"elapsedMicros":micros(started)}),
            )?;
            return Err(error);
        }
    };
    let mut journal = BufWriter::new(io(File::create(out.join("actual-calls.jsonl")))?);
    let mut stages = vec![snapshot(
        &session, &out, "initial", "initial", started, None,
    )?];
    let mut state = session.state();
    let (mut runs, mut inputs, mut checkpoints, mut requested, mut execute_micros) =
        (0_u64, 0_u64, 0_u64, 0_u64, 0_u64);
    let mut last_execute = None;
    let mut seen_frames = BTreeSet::new();
    let mut applied_rows = 0;
    let mut api_error = None;
    for (index, row) in rows.iter().enumerate() {
        if stopped(&state) {
            break;
        }
        let before = clock(&state)?;
        let operation = match &row.call {
            Call::Input { id, pressed } => {
                match session.set_virtual_button(*id, *pressed) {
                    Ok(()) => {
                        let after = session.state();
                        if after["cpu"] != state["cpu"] || after["registers"] != state["registers"]
                        {
                            return Err(
                                "Input setter unexpectedly changed CPU/register state".into()
                            );
                        }
                        state = after;
                    }
                    Err(error) => {
                        api_error = Some(error);
                    }
                }
                inputs += 1;
                json!({"kind":"input","index":inputs,"id":id,"pressed":pressed})
            }
            Call::Run { budget } => {
                let execution_start = Instant::now();
                let result = session.execute(*budget);
                execute_micros += micros(execution_start);
                runs += 1;
                requested += u64::from(*budget);
                match result {
                    Ok(value) => {
                        state = value.clone();
                        last_execute = Some(value);
                    }
                    Err(error) => {
                        state = session.state();
                        api_error = Some(error);
                    }
                }
                let after = clock(&state)?;
                if after.0 < before.0
                    || after.1 < before.1
                    || after.0 - before.0 > u64::from(*budget)
                {
                    return Err("Native execution violated monotonic instruction budget".into());
                }
                let mut new_frame = false;
                for output in state["outputs"].as_array().ok_or("State outputs missing")? {
                    let key = (
                        number(output, "output")?,
                        number(output, "sequence")?,
                        number(output, "width")?,
                        number(output, "height")?,
                    );
                    new_frame |= seen_frames.insert(key);
                }
                if new_frame {
                    stages.push(snapshot(
                        &session,
                        &out,
                        &format!("frame-run-{runs:04}"),
                        &row.phase,
                        started,
                        last_execute.as_ref(),
                    )?);
                }
                if runs % 100 == 0 {
                    eprintln!(
                        "native run{runs}/1035 instructions={} ticks={} elapsed={:.3}s",
                        after.0,
                        after.1,
                        started.elapsed().as_secs_f64()
                    );
                }
                json!({"kind":"run","call":runs,"requested":budget})
            }
            Call::Checkpoint { label } => {
                checkpoints += 1;
                stages.push(snapshot(
                    &session,
                    &out,
                    &format!("checkpoint-{checkpoints:02}-{label}"),
                    &row.phase,
                    started,
                    last_execute.as_ref(),
                )?);
                io(journal.flush())?;
                json!({"kind":"checkpoint","label":label})
            }
        };
        let event = json!({"ledgerLine":index+1,"phase":row.phase,"operation":operation,"beforeInstructions":before.0,"beforeTicks":before.1,"instructions":state["cpu"]["instructions"],"ticks":state["cpu"]["virtual_ticks"],"failure":state["cpu"]["failure"],"error":state["error"],"guestHalt":state["guestHalt"],"apiError":api_error,"elapsedMicros":micros(started)});
        io(writeln!(journal, "{event}"))?;
        applied_rows = index + 1;
        if api_error.is_some() {
            break;
        }
    }
    stages.push(snapshot(
        &session,
        &out,
        "terminal",
        "terminal",
        started,
        last_execute.as_ref(),
    )?);
    io(journal.flush())?;
    let outcome = if api_error.is_some() {
        "api-error"
    } else if stopped(&state) {
        "concrete-stop"
    } else {
        "ledger-exhausted"
    };
    let result = json!({"outcome":outcome,"apiError":api_error,"runCalls":runs,"inputCalls":inputs,"hostCheckpoints":checkpoints,"appliedLedgerRows":applied_rows,"allStimuliApplied":runs==1035&&inputs==78,"requestedInstructions":requested,"instructions":state["cpu"]["instructions"],"ticks":state["cpu"]["virtual_ticks"],"failure":state["cpu"]["failure"],"lastExecutionError":state["error"],"guestHalt":state["guestHalt"],"elapsedMicros":micros(started),"executeMicros":execute_micros,"stages":stages,"extraGuestRuns":0,"extraInputSetters":0});
    write_json(&out.join("result.json"), &result)?;
    println!(
        "{}",
        json!({"outcome":outcome,"runCalls":runs,"inputCalls":inputs,"instructions":state["cpu"]["instructions"],"failure":state["cpu"]["failure"],"elapsedMicros":micros(started),"outputDirectory":out})
    );
    Ok(())
}
