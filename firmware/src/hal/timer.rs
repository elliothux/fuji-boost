//! Interval timer (profile `timer_base` 0xfff3c300, 0x50-byte window)
//! channel 7 as a free-running down counter at the board tick
//! rate (one tick per instruction). Its IRQ stays disabled at the GIC, so it
//! never wakes WFI.
use crate::mmio::{read32, write32};

const BASE: usize = 0xfff3_c300;
const CHANNEL: usize = 7;
pub const TICKS_PER_SECOND: u64 = 54_000_000;

static mut LAST: u32 = 0;
static mut HIGH: u64 = 0;

pub fn init() {
    write32(BASE + 0x10 + CHANNEL * 8, u32::MAX);
    write32(BASE + 0x14 + CHANNEL * 8, u32::MAX);
    write32(BASE, 1 << (16 + CHANNEL) | 1 << CHANNEL);
    unsafe {
        LAST = 0;
        HIGH = 0;
    }
}

/// Ticks since `init`, extended past the 32-bit counter wrap. Must be called
/// at least once per wrap (about 79 s); the frame loop does.
pub fn now() -> u64 {
    let elapsed = !read32(BASE + 0x10 + CHANNEL * 8);
    unsafe {
        if elapsed < LAST {
            HIGH += 1 << 32;
        }
        LAST = elapsed;
        HIGH | u64::from(elapsed)
    }
}
