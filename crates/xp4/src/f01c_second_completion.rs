pub const GUARD_SHA256: &str = "8f1f0f23a60fc0257a8f6d67d019f26c5431205e77e064f9442e25227cdedde5";
pub const GUARD_BYTES: usize = 440;
const RANGES: [(usize, usize); 4] = [
    (0x1256d70, 0x1256db4),
    (0x11a9a18, 0x11a9b5c),
    (0x5a9f0, 0x5aa08),
    (0x5aa38, 0x5aa50),
];

pub fn prepare(ram: &[u8]) -> Result<String, String> {
    let mut guard = Vec::with_capacity(GUARD_BYTES);
    for (start, end) in RANGES {
        guard.extend_from_slice(
            ram.get(start..end)
                .ok_or("Missing second F01C consumer source")?,
        );
    }
    if fujimod_firmware::sha256(&guard) != GUARD_SHA256 {
        return Err("Second F01C consumer source differs".into());
    }
    Ok(GUARD_SHA256.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guard_binds_both_consumers_and_each_read_write_helper() {
        let bytes = include_bytes!("../../../profiles/fixtures/xt4-212-f01c-second-completion.bin");
        assert_eq!(bytes.len(), GUARD_BYTES);
        assert_eq!(fujimod_firmware::sha256(bytes), GUARD_SHA256);
        let mut ram = vec![0; RANGES.iter().map(|&(_, end)| end).max().unwrap()];
        let mut offset = 0;
        for (start, end) in RANGES {
            ram[start..end].copy_from_slice(&bytes[offset..offset + end - start]);
            offset += end - start;
        }
        assert_eq!(prepare(&ram).unwrap(), GUARD_SHA256);
        for (start, _) in RANGES {
            ram[start] ^= 1;
            assert!(prepare(&ram).is_err());
            ram[start] ^= 1;
        }
        assert!(prepare(&[]).is_err());
        assert!(prepare(&ram[..RANGES[0].1 - 1]).is_err());
    }

    #[test]
    #[ignore = "requires the official X-T4 2.12 DAT"]
    fn actual_firmware_binds_second_command_and_preserves_bootstrap() {
        use crate::Xp4Session;
        let firmware = std::fs::read(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../FWUP0024.DAT"),
        )
        .unwrap();
        let baseline = Xp4Session::discover_fff7801c_completion(&firmware)
            .unwrap()
            .state();
        assert!(baseline.get("f01cSecondCompletionFixture").is_none());
        assert_eq!(baseline["audioResetFixture"]["singleRequest"], true);
        let candidate = Xp4Session::discover_f01c_second_completion(&firmware)
            .unwrap()
            .state();
        assert_eq!(candidate["cpu"], baseline["cpu"]);
        assert_eq!(candidate["registers"], baseline["registers"]);
        assert_eq!(candidate["outputs"], baseline["outputs"]);
        assert_eq!(
            candidate["faCompletionFixture"],
            baseline["faCompletionFixture"]
        );
        assert_eq!(
            candidate["fff7801cCompletionFixture"],
            baseline["fff7801cCompletionFixture"]
        );
        assert_eq!(candidate["audioResetFixture"]["singleRequest"], false);
        let fixture = &candidate["f01cSecondCompletionFixture"];
        assert_eq!(fixture["sourceGuardSha256"], GUARD_SHA256);
        assert_eq!(fixture["sourceGuardBytes"], GUARD_BYTES);
        assert!(fixture["maximumCommands"].is_null());
        assert_eq!(fixture["synthetic"], true);
    }
}
