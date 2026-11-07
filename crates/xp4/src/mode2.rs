use fujimod_emulator_xp4_board::BankedMode2Config;

pub const GUARD_SHA256: &str = "abc8e932fabd2593564cc864705e65bd82e166b6e3059aebf13694bdb9179829";
pub const TABLES_SHA256: &str = "0aabf94dd0b6bd0c527b590059c543d3982da51e19b786945a74341782eb63c2";
const RANGES: [(usize, usize); 10] = [
    (0x11c33a8, 0x11c34b8),
    (0x11c21ac, 0x11c22b0),
    (0x13d81bc, 0x13d81cc),
    (0x1589490, 0x15894b0),
    (0x15894b0, 0x15896e8),
    (0x15896e8, 0x1589920),
    (0x1589920, 0x1589b58),
    (0x1589b58, 0x1589d90),
    (0x5a9f0, 0x5aa08),
    (0x5aa38, 0x5aa50),
];

pub fn prepare(ram: &[u8]) -> Result<BankedMode2Config, String> {
    let mut guard = Vec::with_capacity(2900);
    for (start, end) in RANGES {
        guard.extend_from_slice(ram.get(start..end).ok_or("Missing mode2 source")?);
    }
    if fujimod_firmware::sha256(&guard) != GUARD_SHA256 {
        return Err("Mode2 source differs".into());
    }
    let mut scripts = std::array::from_fn(|_| Vec::with_capacity(71));
    let mut source = Vec::with_capacity(2272);
    for (unit, script) in scripts.iter_mut().enumerate() {
        let at = 0x1589490 + unit * 8;
        let index = ram.get(at..at + 8).ok_or("Missing mode2 index")?;
        let count = u32::from_le_bytes(index[..4].try_into().unwrap());
        let pointer = u32::from_le_bytes(index[4..].try_into().unwrap()) as usize;
        if count != 71 || pointer != 0x15894b0 + unit * 568 {
            return Err("Mode2 index differs".into());
        }
        let bytes = ram
            .get(pointer..pointer + 568)
            .ok_or("Missing mode2 table")?;
        source.extend_from_slice(bytes);
        script.extend(bytes.as_chunks::<8>().0.iter().map(|pair| {
            (
                u32::from_le_bytes(pair[..4].try_into().unwrap()),
                u32::from_le_bytes(pair[4..].try_into().unwrap()),
            )
        }));
    }
    if fujimod_firmware::sha256(&source) != TABLES_SHA256 {
        return Err("Mode2 ordered tables differ".into());
    }
    Ok(BankedMode2Config {
        scripts,
        source_sha256: TABLES_SHA256.into(),
    })
}

pub const POST_CONTROL_GUARD_SHA256: &str =
    "718b3a6254f9c2bea003b8c6f69b55c123e50ed6a1429f74095d673973069cb5";
pub const POST_CONTROL_GUARD_BYTES: usize = 1408;

pub fn prepare_post_control(ram: &[u8]) -> Result<String, String> {
    let source = ram
        .get(0x11c1b38..0x11c20b8)
        .ok_or("Missing post-control mode2 manager source")?;
    if fujimod_firmware::sha256(source) != POST_CONTROL_GUARD_SHA256 {
        return Err("Post-control mode2 manager source differs".into());
    }
    Ok(POST_CONTROL_GUARD_SHA256.into())
}

pub const SELECTOR1_GUARD_SHA256: &str =
    "a699dd402b94dff296ab64d354c57a7d55be0cda365f36f3ddfbdb300f86c6d3";
pub const SELECTOR1_GUARD_BYTES: usize = 508;
const SELECTOR1_RANGES: [(usize, usize); 7] = [
    (0x11c34b8, 0x11c355c),
    (0x11c20d8, 0x11c21ac),
    (0x158ca90, 0x158cac4),
    (0x158cd1c, 0x158cd2c),
    (0x13d81bc, 0x13d81cc),
    (0x5a9f0, 0x5aa08),
    (0x5aa38, 0x5aa50),
];

pub fn prepare_selector1(ram: &[u8]) -> Result<String, String> {
    let mut guard = Vec::with_capacity(SELECTOR1_GUARD_BYTES);
    for (start, end) in SELECTOR1_RANGES {
        guard.extend_from_slice(ram.get(start..end).ok_or("Missing selector1 source")?);
    }
    if fujimod_firmware::sha256(&guard) != SELECTOR1_GUARD_SHA256 {
        return Err("Selector1 source differs".into());
    }
    Ok(SELECTOR1_GUARD_SHA256.into())
}

pub const REQUEST2_GUARD_SHA256: &str =
    "e1515b5e62c4353d5ec4974a3ceafe3501abde056b78fa3f74251efe25254ad3";
pub const REQUEST2_GUARD_BYTES: usize = 276;
const REQUEST2_RANGES: [(usize, usize); 5] = [
    (0x11bfb18, 0x11bfb58),
    (0x11bfdfc, 0x11bfe90),
    (0x15874dc, 0x15874ec),
    (0x5a9f0, 0x5aa08),
    (0x5aa38, 0x5aa50),
];

pub fn prepare_ff702280_request(ram: &[u8]) -> Result<String, String> {
    let mut guard = Vec::with_capacity(REQUEST2_GUARD_BYTES);
    for (start, end) in REQUEST2_RANGES {
        guard.extend_from_slice(ram.get(start..end).ok_or("Missing FF702280 source")?);
    }
    if fujimod_firmware::sha256(&guard) != REQUEST2_GUARD_SHA256 {
        return Err("FF702280 source differs".into());
    }
    Ok(REQUEST2_GUARD_SHA256.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_or_changed_source_is_rejected() {
        assert!(prepare(&[]).is_err());
        assert!(prepare_ff702280_request(&[]).is_err());
        assert!(prepare_ff702280_request(&vec![0; 0x15874ec]).is_err());
        assert!(prepare_selector1(&[]).is_err());
        assert!(prepare_selector1(&vec![0; 0x158cd2c]).is_err());
        assert!(prepare_post_control(&[]).is_err());
        assert!(prepare_post_control(&vec![0; 0x11c20b8]).is_err());
        assert!(prepare(&vec![0; 0x1589d90]).is_err());
    }
}
