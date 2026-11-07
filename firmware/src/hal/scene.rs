//! Host RGB viewfinder. The host writes `protocol.json` `scene` (RGB888 after
//! a 16-byte header). Pixels painted with `CHROMA` in the UI framebuffer are
//! replaced by the current frame when composing into the present buffer, so
//! apps decide the preview hole in layout and the key colour never reaches
//! the scanned-out plane (which would flash between host frames).
use crate::layout::{RENDER_HEIGHT, RENDER_WIDTH};
use crate::protocol::*;

/// RGB #010008 as the LCDC BGRA word (B, G, R, A).
pub const CHROMA: u32 = u32::from_le_bytes([0x08, 0x00, 0x01, 0xff]);

fn word(index: usize) -> u32 {
    unsafe { *(SCENE_BASE as *const u32).add(index) }
}

fn pixels() -> Option<(u32, u32, &'static [u8])> {
    if word(S_MAGIC) != SCENE_MAGIC {
        return None;
    }
    let (width, height) = (word(S_WIDTH), word(S_HEIGHT));
    if width == 0
        || height == 0
        || width > SCENE_MAX_WIDTH
        || height > SCENE_MAX_HEIGHT
        || word(S_GENERATION) == 0
    {
        return None;
    }
    let bytes = (width as usize)
        .checked_mul(height as usize)?
        .checked_mul(3)?;
    Some((width, height, unsafe {
        core::slice::from_raw_parts((SCENE_BASE + SCENE_HEADER_BYTES) as *const u8, bytes)
    }))
}

fn sample(rgb: &[u8], sw: usize, sh: usize, x: usize, y: usize, dw: usize, dh: usize) -> u32 {
    let sx = x * sw / dw;
    let sy = y * sh / dh;
    let p = (sy * sw + sx) * 3;
    u32::from_le_bytes([rgb[p + 2], rgb[p + 1], rgb[p], 0xff])
}

/// True when the host has posted a new frame since the last call.
pub fn poll(seen: &mut u32) -> bool {
    if word(S_MAGIC) != SCENE_MAGIC {
        *seen = 0;
        return false;
    }
    let generation = word(S_GENERATION);
    if generation == 0 || generation == *seen {
        return false;
    }
    *seen = generation;
    true
}

/// Copy a damage rectangle from the UI framebuffer into the present buffer.
/// Chroma pixels become the live scene (when one is posted) so the key colour
/// is never scanned out.
pub fn compose(present: &mut [u32], source: &[u32], x0: usize, y0: usize, x1: usize, y1: usize) {
    let scene = pixels();
    let (dw, dh) = (RENDER_WIDTH as usize, RENDER_HEIGHT as usize);
    for y in y0..y1.min(dh) {
        for x in x0..x1.min(dw) {
            let i = y * dw + x;
            let px = source[i];
            present[i] = if px == CHROMA {
                match scene {
                    Some((sw, sh, rgb)) => sample(rgb, sw as usize, sh as usize, x, y, dw, dh),
                    None => px,
                }
            } else {
                px
            };
        }
    }
}

/// Rewrite every chroma hole from `source` into `present` using the live scene.
/// Used when the host posts a new frame but the UI did not damage those pixels
/// (they already hold a previous scene sample, not chroma).
pub fn refresh(present: &mut [u32], source: &[u32]) {
    let Some((sw, sh, rgb)) = pixels() else {
        return;
    };
    let (dw, dh) = (RENDER_WIDTH as usize, RENDER_HEIGHT as usize);
    let (sw, sh) = (sw as usize, sh as usize);
    for y in 0..dh {
        for x in 0..dw {
            let i = y * dw + x;
            if source[i] == CHROMA {
                present[i] = sample(rgb, sw, sh, x, y, dw, dh);
            }
        }
    }
}
