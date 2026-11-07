pub const GUARD_SHA256: &str = "7a5c3711d1b005ca6a84b283cf24d9c89948a1c55272c0f05b5560c963569746";
pub const GUARD_BYTES: usize = 184;
const RANGES: [(usize, usize); 6] = [
    (0x11ab3d4, 0x11ab414),
    (0x11aa320, 0x11aa338),
    (0x11ab468, 0x11ab48c),
    (0x11ab308, 0x11ab314),
    (0x5a9f0, 0x5aa08),
    (0x5aa38, 0x5aa50),
];

pub fn prepare(ram: &[u8]) -> Result<String, String> {
    let mut guard = Vec::with_capacity(GUARD_BYTES);
    for (start, end) in RANGES {
        guard.extend_from_slice(ram.get(start..end).ok_or("Missing FA ready source")?);
    }
    if fujimod_firmware::sha256(&guard) != GUARD_SHA256 {
        return Err("FA ready source differs".into());
    }
    Ok(GUARD_SHA256.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_or_changed_source_is_rejected() {
        assert!(prepare(&[]).is_err());
        assert!(prepare(&vec![0; 0x11ab48c]).is_err());
    }
}
