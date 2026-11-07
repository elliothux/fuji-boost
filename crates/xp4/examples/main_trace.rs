use fuji_boost_xp4::Xp4Session;

fn main() -> Result<(), String> {
    let source = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../../FWUP0024.DAT"))
        .map_err(|e| e.to_string())?;
    let session = match std::env::args().nth(1).as_deref() {
        None => Xp4Session::discover_ordered(&source)?,
        Some("--supported-nor") => Xp4Session::discover_supported_nor(&source)?,
        Some("--preloaded") => Xp4Session::discover_preloaded(&source)?,
        Some("--inactive-countdown") => Xp4Session::discover_inactive_countdown(&source)?,
        Some("--startup-input") => Xp4Session::discover_startup_input(&source)?,
        Some("--startup-cause") => Xp4Session::discover_startup_cause(&source)?,
        Some("--invalid-clock") => Xp4Session::discover_invalid_clock(&source)?,
        Some("--calibration-fixture") => Xp4Session::discover_power2_bit8_echo(&source)?,
        Some("--gpio9-high") => Xp4Session::discover_gpio9_high(&source)?,
        Some("--power3-raw768") => Xp4Session::discover_power3_raw768(&source)?,
        _ => {
            return Err(
                "Expected no argument, --supported-nor, --inactive-countdown, --startup-input, --startup-cause, --invalid-clock, --calibration-fixture, --gpio9-high, --power3-raw768 or --preloaded".into(),
            );
        }
    };
    let state = session.observe_main_initialization()?;
    assert_eq!(state["mainInitTrace"]["dropped"], 0);
    assert_eq!(state["bootVerified"], false);
    println!("{}", serde_json::to_string_pretty(&state).unwrap());
    Ok(())
}
