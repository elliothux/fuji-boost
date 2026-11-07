pub const GUARD_SHA256: &str = "8e1374266bb0e91b757470f7576e3919b5db9af016e9aa85ab57b12dadce5797";
pub const GUARD_BYTES: usize = 1500;
const RANGES: [(usize, usize); 14] = [
    (0x11aa128, 0x11aa178),
    (0x11aa178, 0x11aa2b4),
    (0x11aa2b4, 0x11aa348),
    (0x11aa348, 0x11aa3b4),
    (0x11ab03c, 0x11ab074),
    (0x11ab16c, 0x11ab278),
    (0x11ab278, 0x11ab35c),
    (0x11ab35c, 0x11ab38c),
    (0x11ab3d4, 0x11ab414),
    (0x11ab414, 0x11ab468),
    (0x11ab468, 0x11ab48c),
    (0x5a9f0, 0x5aa08),
    (0x5aa38, 0x5aa50),
    (0x1583dfc, 0x1583e0c),
];

pub fn prepare(ram: &[u8]) -> Result<String, String> {
    super::fa::prepare(ram)?;
    let mut guard = Vec::with_capacity(GUARD_BYTES);
    for (start, end) in RANGES {
        guard.extend_from_slice(ram.get(start..end).ok_or("Missing FA completion source")?);
    }
    if fujimod_firmware::sha256(&guard) != GUARD_SHA256 {
        return Err("FA completion source differs".into());
    }
    Ok(GUARD_SHA256.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_original_guard_is_required_in_every_range() {
        let guard = include_bytes!("../../../profiles/fixtures/xt4-212-fa-completion.bin");
        assert_eq!(guard.len(), GUARD_BYTES);
        assert_eq!(fujimod_firmware::sha256(guard), GUARD_SHA256);
        let mut ram = vec![0; RANGES.iter().map(|&(_, end)| end).max().unwrap()];
        let mut offset = 0;
        for (start, end) in RANGES {
            ram[start..end].copy_from_slice(&guard[offset..offset + end - start]);
            offset += end - start;
        }
        assert_eq!(prepare(&ram).unwrap(), GUARD_SHA256);
        for (start, _) in RANGES {
            ram[start] ^= 1;
            assert!(prepare(&ram).is_err());
            ram[start] ^= 1;
        }
        assert!(prepare(&[]).is_err());
        assert!(prepare(&ram[..RANGES.last().unwrap().0]).is_err());
    }
}
