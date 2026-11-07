use fuji_boost_xp4::Xp4Session;

fn main() -> Result<(), String> {
    let source = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../../FWUP0024.DAT"))
        .map_err(|e| e.to_string())?;
    let session = Xp4Session::discover_training(&source)?;
    let state = session.observe_loader_barrier()?;
    assert!(state["startupMemoryTrace"]["violation"].is_null());
    println!("{}", serde_json::to_string_pretty(&state).unwrap());
    Ok(())
}
