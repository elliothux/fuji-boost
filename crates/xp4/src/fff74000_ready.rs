//! Original consumer/helper identity only; hardware ready cause and timing are unknown.
pub const GUARD_SHA256: &str = "aa46e8b568344cbcbd455e10f7834f4a89cefa41e10140f46a0cefe2957394b8";
pub const GUARD_BYTES: usize = 100;
const RANGES: [(usize, usize); 3] = [
    (0x1186f64, 0x1186f98),
    (0x5a9f0, 0x5aa08),
    (0x5aa38, 0x5aa50),
];

pub fn prepare(ram: &[u8]) -> Result<String, String> {
    let mut guard = Vec::with_capacity(GUARD_BYTES);
    for (start, end) in RANGES {
        guard.extend_from_slice(
            ram.get(start..end)
                .ok_or("Missing FFF74000 consumer source")?,
        );
    }
    if fujimod_firmware::sha256(&guard) != GUARD_SHA256 {
        return Err("FFF74000 consumer source differs".into());
    }
    Ok(GUARD_SHA256.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guard_binds_the_bit_zero_consumer_and_both_word_helpers() {
        let bytes = include_bytes!("../../../profiles/fixtures/xt4-212-fff74000-ready.bin");
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
}
