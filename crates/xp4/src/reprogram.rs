use fujimod_emulator_xp4_board::BankedReprogramConfig;

pub const GUARD_SHA256: &str = "33b931b8a079339887814faf62cd647855d7d36f13af8d1a8d5e9a3ac8a8df8f";
pub const CAPTURE_SHA256: &str = "fd03b5ccd50d1d6821f52b4be1475af4ec48bef2f0667d27108601ad0a1666c0";
const CAPTURE: &[u8; 232] =
    include_bytes!("../../../profiles/fixtures/xt4-212-reprogram-cache.bin");
const RANGES: [(usize, usize); 17] = [
    (0x11c2dc8, 0x11c3204),
    (0x11c2c98, 0x11c2dc8),
    (0x11c1b38, 0x11c1bcc),
    (0x11c1d60, 0x11c1dc8),
    (0x11c1ed4, 0x11c1f30),
    (0x11c1f88, 0x11c1fac),
    (0x1587eb0, 0x1589424),
    (0x1589438, 0x1589490),
    (0x158cba0, 0x158cbdc),
    (0x158cb00, 0x158cb14),
    (0x158cc3c, 0x158cc64),
    (0x158cc74, 0x158cc88),
    (0x158cac4, 0x158cacc),
    (0x158cc9c, 0x158cd2c),
    (0x1587c24, 0x1587cb8),
    (0x5a9f0, 0x5aa08),
    (0x5aa38, 0x5aa50),
];

pub fn prepare(ram: &[u8]) -> Result<BankedReprogramConfig, String> {
    let mut guard = Vec::with_capacity(7836);
    for (start, end) in RANGES {
        guard.extend_from_slice(ram.get(start..end).ok_or("Missing reprogram source")?);
    }
    if fujimod_firmware::sha256(&guard) != GUARD_SHA256 {
        return Err("Reprogram source differs".into());
    }
    if ram.get(0x1587c2c..0x1587cb4) != Some(&CAPTURE[96..]) {
        return Err("Captured reprogram profile differs from original selected profile".into());
    }
    decode_inputs(CAPTURE)
}

fn decode_inputs(bytes: &[u8]) -> Result<BankedReprogramConfig, String> {
    if bytes.len() != 232 || fujimod_firmware::sha256(bytes) != CAPTURE_SHA256 {
        return Err("Reprogram captured inputs differ".into());
    }
    let profile = bytes[96..]
        .as_chunks::<8>()
        .0
        .iter()
        .map(|pair| {
            (
                u32::from_le_bytes(pair[..4].try_into().unwrap()),
                u32::from_le_bytes(pair[4..].try_into().unwrap()),
            )
        })
        .collect();
    let mut cached = [None; 4];
    for (unit, slot) in cached.iter_mut().enumerate().skip(1) {
        *slot = Some(std::array::from_fn(|i| {
            let offset = unit * 24 + i * 4;
            u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap())
        }));
    }
    // Slot0 has no qualified save; slots1..3 match their modeled primary getter words.
    let input = [14u32.to_le_bytes().as_slice(), &bytes[96..], &bytes[24..96]].concat();
    Ok(BankedReprogramConfig {
        profile,
        cached,
        input_sha256: fujimod_firmware::sha256(&input),
    })
}

pub const POST_CONTROL_GUARD_SHA256: &str =
    "43d2d56dc11bb4be8baabb3cb5336c89ba69512967bc37e27c68ed88242774a6";
pub const POST_CONTROL_GUARD_BYTES: usize = 1428;
const POST_CONTROL_RANGES: [(usize, usize); 21] = [
    (0x11c3204, 0x11c33a8),
    (0x11c22b0, 0x11c253c),
    (0x11c1f58, 0x11c1f64),
    (0x5a9f0, 0x5aa08),
    (0x5aa38, 0x5aa50),
    (0x158cc64, 0x158cc74),
    (0x158cac4, 0x158cad8),
    (0x158cc88, 0x158cc9c),
    (0x1589424, 0x1589438),
    (0x158cbfc, 0x158cc10),
    (0x158cbdc, 0x158cbfc),
    (0x158cd0c, 0x158cd2c),
    (0x15880d0, 0x15880d8),
    (0x1588720, 0x1588728),
    (0x1588740, 0x1588748),
    (0x1588758, 0x1588760),
    (0x15887d8, 0x1588800),
    (0x1588b20, 0x1588b28),
    (0x1588b40, 0x1588b48),
    (0x1588b58, 0x1588b60),
    (0x1588bd8, 0x1588c00),
];

pub fn prepare_post_control(ram: &[u8]) -> Result<String, String> {
    let mut guard = Vec::with_capacity(POST_CONTROL_GUARD_BYTES);
    for (start, end) in POST_CONTROL_RANGES {
        guard.extend_from_slice(ram.get(start..end).ok_or("Missing post-control source")?);
    }
    if fujimod_firmware::sha256(&guard) != POST_CONTROL_GUARD_SHA256 {
        return Err("Post-control source differs".into());
    }
    Ok(POST_CONTROL_GUARD_SHA256.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn captured_inputs_bind_profile_slot_mask_and_little_endian_words() {
        let input = decode_inputs(CAPTURE).unwrap();
        assert_eq!(input.profile.len(), 17);
        assert_eq!(input.cached[0], None);
        for unit in 1..4 {
            assert_eq!(
                input.cached[unit],
                Some([0x03000000, 0x300, 0x30099, 0x03000300, 0x03000300, 0x300])
            );
        }
        assert_eq!(
            input.input_sha256,
            "b13413bae5b1ab161f5de3e87f76f3169846e0989c98abbf7cfd292f080450a2"
        );
        assert!(decode_inputs(&CAPTURE[..231]).is_err());
        let mut changed = *CAPTURE;
        changed[24] ^= 1;
        assert!(decode_inputs(&changed).is_err());
        assert!(prepare(&[]).is_err());
        assert!(prepare_post_control(&[]).is_err());
        assert_eq!(
            POST_CONTROL_RANGES
                .iter()
                .map(|(a, b)| b - a)
                .sum::<usize>(),
            POST_CONTROL_GUARD_BYTES
        );
    }
}
