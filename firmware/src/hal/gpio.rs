//! Masked GPIO banks at 0xfff41000; register 5 of each 0x40-byte port reads
//! the input levels. The six navigation keys are active low.
use crate::mmio::read32;

const BASE: usize = 0xfff4_1000;

pub const BTN_UP: u32 = 0x0010;
pub const BTN_RIGHT: u32 = 0x0020;
pub const BTN_DOWN: u32 = 0x0040;
pub const BTN_LEFT: u32 = 0x0080;
pub const BTN_CIRCLE: u32 = 0x2000;
pub const BTN_CROSS: u32 = 0x4000;

const KEYS: [(usize, u32, u32); 6] = [
    (7, 11, BTN_UP),
    (7, 10, BTN_DOWN),
    (7, 9, BTN_LEFT),
    (7, 8, BTN_RIGHT),
    (7, 12, BTN_CIRCLE),
    (9, 5, BTN_CROSS),
];

fn levels(port: usize) -> u32 {
    read32(BASE + port * 0x40 + 0x14)
}

/// PocketJS button mask (contracts/generated/pocket_spec.h bit values).
pub fn buttons() -> u32 {
    let port7 = levels(7);
    let port9 = levels(9);
    KEYS.iter().fold(0, |mask, &(port, pin, bit)| {
        let level = if port == 7 { port7 } else { port9 };
        if level & (1 << pin) == 0 {
            mask | bit
        } else {
            mask
        }
    })
}
