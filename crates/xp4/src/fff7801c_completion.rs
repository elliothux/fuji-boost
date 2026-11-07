pub const GUARD_SHA256: &str = "e85945b0654c960ee8dd6c711ab183b74c6182a7788e68764ef516aabfa5cc19";
pub const GUARD_BYTES: usize = 204;
const RANGES: [(usize, usize); 3] = [
    (0x125a228, 0x125a2c4),
    (0x5a9f0, 0x5aa08),
    (0x5aa38, 0x5aa50),
];

pub fn prepare(ram: &[u8]) -> Result<String, String> {
    let mut guard = Vec::with_capacity(GUARD_BYTES);
    for (start, end) in RANGES {
        guard.extend_from_slice(
            ram.get(start..end)
                .ok_or("Missing FFF7801C consumer source")?,
        );
    }
    if fujimod_firmware::sha256(&guard) != GUARD_SHA256 {
        return Err("FFF7801C consumer source differs".into());
    }
    Ok(GUARD_SHA256.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_guard_rejects_changed_consumer_and_both_helpers() {
        let bytes = include_bytes!("../../../profiles/fixtures/xt4-212-fff7801c-completion.bin");
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
    fn actual_firmware_qualifies_the_opt_in_without_changing_bootstrap() {
        use crate::Xp4Session;
        let firmware = std::fs::read(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../FWUP0024.DAT"),
        )
        .unwrap();
        let baseline = Xp4Session::discover_fa_completion(&firmware)
            .unwrap()
            .state();
        assert!(baseline.get("fff7801cCompletionFixture").is_none());
        let candidate = Xp4Session::discover_fff7801c_completion(&firmware)
            .unwrap()
            .state();
        assert_eq!(candidate["cpu"], baseline["cpu"]);
        assert_eq!(candidate["registers"], baseline["registers"]);
        assert_eq!(candidate["outputs"], baseline["outputs"]);
        let fixture = &candidate["fff7801cCompletionFixture"];
        assert_eq!(fixture["sourceGuardSha256"], GUARD_SHA256);
        assert_eq!(fixture["sourceGuardBytes"], GUARD_BYTES);
        assert_eq!(fixture["latencyTicks"], 8);
        assert_eq!(fixture["synthetic"], true);
    }
}
