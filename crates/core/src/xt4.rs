//! X-T4 type-6 update identity and container checksums, independent of stock hashes.
pub fn validate(source: &[u8]) -> Result<(), String> {
    let header = fujimod_firmware::outer_header(source).map_err(|e| e.to_string())?;
    let codes =
        b"0005600100056002000560040005600900057001000570020005700400057005000570060005700700057008";
    if header.kind != 6
        || header.version_major != 2
        || header.version_minor != 0x12
        || !source[4..0x204]
            .as_chunks::<2>()
            .0
            .iter()
            .zip(codes.iter().copied().chain(std::iter::repeat(b'0')))
            .all(|(pair, code)| *pair == [b'3', code])
    {
        return Err("Expected an X-T4 2.12-family type-6 DAT".into());
    }
    let word = |at| u32::from_le_bytes(source[at..at + 4].try_into().unwrap());
    // Type 6 uses one's-complement byte sums: four inverted payloads and two raw metadata blocks.
    for (field, offset, size, inverted) in [
        (0x20c, 0, word(0x248), true),
        (0x214, word(0x254), word(0x258), true),
        (0x21c, word(0x220), word(0x224), true),
        (0x228, word(0x22c), word(0x230), true),
        (0x264, word(0x25c), word(0x260), false),
        (0x270, word(0x268), word(0x26c), false),
    ] {
        if size == 0 {
            return Err(format!("Empty DAT checksum range at {field:#x}"));
        }
        let bytes = fujimod_firmware::extent(source, 0x274 + u64::from(offset), u64::from(size))
            .map_err(|e| e.to_string())?;
        let sum = bytes.iter().fold(0u32, |sum, &byte| {
            sum.wrapping_add(u32::from(if inverted { !byte } else { byte }))
        });
        if word(field) != !sum {
            return Err(format!("DAT checksum differs at {field:#x}"));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checks_identity_extents_and_all_six_checksums() {
        let mut dat = vec![0; 0x280];
        let put = |dat: &mut [u8], at: usize, value: u32| {
            dat[at..at + 4].copy_from_slice(&value.to_le_bytes());
        };
        put(&mut dat, 0, 6);
        let codes = b"0005600100056002000560040005600900057001000570020005700400057005000570060005700700057008";
        for (pair, code) in dat[4..0x204]
            .as_chunks_mut::<2>()
            .0
            .iter_mut()
            .zip(codes.iter().copied().chain(std::iter::repeat(b'0')))
        {
            pair.copy_from_slice(&[b'3', code]);
        }
        put(&mut dat, 0x204, 2);
        put(&mut dat, 0x208, 0x12);
        for size in [0x248, 0x258, 0x224, 0x230, 0x260, 0x26c] {
            put(&mut dat, size, 1);
        }
        for field in [0x20c, 0x214, 0x21c, 0x228] {
            put(&mut dat, field, !255);
        }
        for field in [0x264, 0x270] {
            put(&mut dat, field, u32::MAX);
        }
        validate(&dat).unwrap();
        for at in [
            4, 0x204, 0x208, 0x20c, 0x214, 0x21c, 0x228, 0x264, 0x270, 0x274,
        ] {
            let mut changed = dat.clone();
            changed[at] ^= 1;
            assert!(validate(&changed).is_err(), "accepted changed byte {at:#x}");
        }
        put(&mut dat, 0x254, u32::MAX);
        assert!(validate(&dat).is_err());
        assert!(validate(&dat[..0x270]).is_err());
    }
}
