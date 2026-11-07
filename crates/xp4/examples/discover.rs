use fuji_boost_xp4::Xp4Session;

fn main() -> Result<(), String> {
    let source = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../../FWUP0024.DAT"))
        .map_err(|e| e.to_string())?;
    let mut args = std::env::args().skip(1);
    let session = match args.next().as_deref() {
        None => Xp4Session::discover(&source)?,
        Some("--power-ready") => Xp4Session::discover_ready(&source)?,
        Some("--power-commands") => Xp4Session::discover_commands(&source)?,
        Some("--empty-retained") => Xp4Session::discover_empty_retained(&source)?,
        Some("--erased-flash") => Xp4Session::discover_erased_flash(&source)?,
        Some("--training") => Xp4Session::discover_training(&source)?,
        Some("--ordered-startup") => Xp4Session::discover_ordered(&source)?,
        Some("--supported-nor") => Xp4Session::discover_supported_nor(&source)?,
        Some("--preloaded") => Xp4Session::discover_preloaded(&source)?,
        Some("--inactive-countdown") => Xp4Session::discover_inactive_countdown(&source)?,
        Some("--startup-input") => Xp4Session::discover_startup_input(&source)?,
        Some("--startup-cause") => Xp4Session::discover_startup_cause(&source)?,
        Some("--invalid-clock") => Xp4Session::discover_invalid_clock(&source)?,
        Some("--calibration-fixture") => Xp4Session::discover_power2_bit8_echo(&source)?,
        Some(_) => {
            return Err(
                "Expected no argument, --power-ready, --power-commands, --empty-retained, --erased-flash, --training, --ordered-startup, --supported-nor, --inactive-countdown, --startup-input, --startup-cause, --invalid-clock, --calibration-fixture or --preloaded".into(),
            );
        }
    };
    let batches: u32 = args
        .next()
        .map(|s| s.parse())
        .transpose()
        .map_err(|_| "Batch count must be an integer in 1..=512")?
        .unwrap_or(3);
    if !(1..=512).contains(&batches) || args.next().is_some() {
        return Err("Expected an optional batch count in 1..=512".into());
    }
    let mut progress = session.run_state();
    for batch in 0..batches {
        progress = session.execute_batch(1_000_000)?;
        if batches > 3 {
            eprintln!(
                "batch {}/{}: instructions={}, failure={}",
                batch + 1,
                batches,
                progress.cpu.instructions,
                serde_json::to_string(&progress.cpu.failure).unwrap()
            );
        }
        if progress.stopped() || progress.quiescent {
            break;
        }
    }
    let mut state = session.state();
    state["error"] = serde_json::json!(progress.error);
    assert_eq!(state["discovery"], true);
    assert_eq!(state["bootVerified"], false);
    assert!(!state["frontier"].as_array().unwrap().is_empty());
    state["diagnostics"] = session.diagnostics()?;
    state["instructionBudget"] = serde_json::json!(u64::from(batches) * 1_000_000);
    println!("{}", serde_json::to_string_pretty(&state).unwrap());
    Ok(())
}
