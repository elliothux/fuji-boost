use fuji_boost_core::FirmwareSession;

#[test]
#[ignore = "requires the user-provided X-T4 2.12 DAT; run bun run test:xt4"]
fn xt4_real_firmware_decode_identity_and_entrypoints() {
    let path = std::env::var_os("FUJI_XT4_FIRMWARE")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../FWUP0024.DAT")
        });
    let bytes = std::fs::read(path).expect("Provide FWUP0024.DAT or FUJI_XT4_FIRMWARE");
    let session = FirmwareSession::inspect(&bytes).expect("decode real X-T4 update");
    let report = session.report();
    assert_eq!(report.profile.as_ref().unwrap().id, "xt4-212");
    assert_eq!(report.segments.len(), 4);
    assert_eq!(report.decoded_size, 33_543_940);
    assert_eq!(report.segments[0].base, Some(0x01021000));
    assert_eq!(report.segments[0].entry, Some(0x012910c4));
    assert_eq!(
        report.segments[0].sha256,
        "0b5b0fb0ee5fe6b109dae9964b5cddfe18edab46b4a0c195f9da194448906eeb"
    );
    assert_eq!(
        report.segments[1].sha256,
        "e1ef8b30903d3078fde103cdd134fca7bfc4337d67bcaf55c3080ffd5fd93b13"
    );
    assert_eq!(
        report.segments[2].sha256,
        "6c8d7b8b0a37df8c2b636c504c07a80318243b313d5bf435e2c39ef3f56e9f80"
    );
    assert_eq!(
        report.segments[3].sha256,
        "b1a6146fc89394e7c47a2bd5b0e92c07de085567da3aa8d6198bc1a32de9eec6"
    );
    assert!(session.read_image(0, 0, 64).is_ok());
    assert!(session.read_image(0, usize::MAX, 64).is_err());
    assert!(session.read_image(4, 0, 1).is_err());
    println!("{}", serde_json::to_string(session.report()).unwrap());
}
