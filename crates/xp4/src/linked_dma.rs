//! Exact original slices for an opt-in synthetic linked-DMA experiment.
//! This identity guard does not establish hardware data, clock, reload or IRQ behavior.
pub const GUARD_SHA256: &str = "ae30d3acfe9998d5c9351b99b775509aaf08b972b2c9749484ba0dfb7fc04911";
pub const GUARD_BYTES: usize = 908;
const RANGES: [(usize, usize); 7] = [
    (0x11a9e24, 0x11a9eb0),
    (0x11aa058, 0x11aa098),
    (0x11adb34, 0x11adb8c),
    (0x11adcd8, 0x11add58),
    (0x11ade80, 0x11ae03c),
    (0x1582df0, 0x1582dfc),
    (0x1730c00, 0x1730c20),
];

pub fn prepare(ram: &[u8]) -> Result<String, String> {
    let mut guard = Vec::with_capacity(GUARD_BYTES);
    for (start, end) in RANGES {
        guard.extend_from_slice(ram.get(start..end).ok_or("Missing linked DMA source")?);
    }
    if fujimod_firmware::sha256(&guard) != GUARD_SHA256 {
        return Err("Linked DMA source differs".into());
    }
    Ok(GUARD_SHA256.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_ram() -> Vec<u8> {
        let bytes = include_bytes!("../../../profiles/fixtures/xt4-212-linked-dma.bin");
        assert_eq!(bytes.len(), GUARD_BYTES);
        assert_eq!(fujimod_firmware::sha256(bytes), GUARD_SHA256);
        let mut ram = vec![0; RANGES.iter().map(|&(_, end)| end).max().unwrap()];
        let mut offset = 0;
        for (start, end) in RANGES {
            ram[start..end].copy_from_slice(&bytes[offset..offset + end - start]);
            offset += end - start;
        }
        assert_eq!(offset, GUARD_BYTES);
        ram
    }

    #[test]
    fn fixture_matches_the_original_absolute_ram_slices() {
        assert_eq!(prepare(&fixture_ram()).unwrap(), GUARD_SHA256);
    }

    #[test]
    fn guard_rejects_changed_or_missing_source_at_every_range_boundary() {
        let mut ram = fixture_ram();
        for (start, end) in RANGES {
            for address in [start, end - 1] {
                ram[address] ^= 1;
                assert_eq!(prepare(&ram).unwrap_err(), "Linked DMA source differs");
                ram[address] ^= 1;
            }
            assert_eq!(
                prepare(&ram[..end - 1]).unwrap_err(),
                "Missing linked DMA source"
            );
        }
        assert!(prepare(&[]).is_err());
    }
}
