pub const GUARD_SHA256: &str = "17c49febd52cd0b40b00661eb4a7b85156049fe6f4b78379ef0cae4a60368a13";
pub const GUARD_BYTES: usize = 120;
const RANGES: [(usize, usize); 3] = [
    (0x118938c, 0x11893d4),
    (0x5a9f0, 0x5aa08),
    (0x5aa38, 0x5aa50),
];

pub fn prepare(ram: &[u8]) -> Result<String, String> {
    let mut guard = Vec::with_capacity(GUARD_BYTES);
    for (start, end) in RANGES {
        guard.extend_from_slice(
            ram.get(start..end)
                .ok_or("Missing FFF50000 consumer source")?,
        );
    }
    if fujimod_firmware::sha256(&guard) != GUARD_SHA256 {
        return Err("FFF50000 consumer source differs".into());
    }
    Ok(GUARD_SHA256.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guard_binds_the_bit_zero_consumer_and_both_word_helpers() {
        let bytes = include_bytes!("../../../profiles/fixtures/xt4-212-fff50000-ready.bin");
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
    fn actual_firmware_ready_entry_preserves_the_inherited_scenario() {
        use crate::Xp4Session;
        let firmware = std::fs::read(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../FWUP0024.DAT"),
        )
        .unwrap();
        let baseline = Xp4Session::discover_fff7801c_second_completion(&firmware)
            .unwrap()
            .state();
        assert!(baseline.get("fff50000ReadyFixture").is_none());
        let mut candidate = Xp4Session::discover_fff50000_ready(&firmware)
            .unwrap()
            .state();
        let fixture = candidate
            .as_object_mut()
            .unwrap()
            .remove("fff50000ReadyFixture")
            .unwrap();
        assert_eq!(fixture["address"], 0xfff50000u32);
        assert_eq!(fixture["sourceGuardSha256"], GUARD_SHA256);
        assert_eq!(fixture["sourceGuardBytes"], GUARD_BYTES);
        assert_eq!(fixture["pendingWord"], 0);
        assert_eq!(fixture["readyWord"], 1);
        assert_eq!(fixture["latencyTicks"], 8);
        let assumptions = candidate["assumptions"].as_array_mut().unwrap();
        let added = assumptions.pop().unwrap();
        assert!(
            added
                .as_str()
                .unwrap()
                .starts_with("Synthetic FFF50000 ready experiment:")
        );
        assert_eq!(candidate, baseline);
    }
}
