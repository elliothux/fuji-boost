use fuji_boost_xp4::Xp4Session;

fn main() -> Result<(), String> {
    let source = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../../FWUP0024.DAT"))
        .map_err(|e| e.to_string())?;
    let session = Xp4Session::load(&source)?;
    let state = session.execute(100_000)?;
    assert!(state["cpu"]["instructions"].as_u64().unwrap() > 0);
    assert_eq!(state["bootVerified"], false);
    println!("{}", serde_json::to_string_pretty(&state).unwrap());
    Ok(())
}
