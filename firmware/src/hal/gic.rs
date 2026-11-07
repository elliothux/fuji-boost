//! GIC-400 at CBAR+0x1000 (distributor) / CBAR+0x2000 (CPU interface).
//! The firmware never unmasks CPSR.I: WFI still wakes when the GIC holds an
//! eligible interrupt, and the core acknowledges it by polling IAR.
use crate::mmio::{read32, write32, write8};

const CBAR: usize = 0xffff_0000;
const GICD: usize = CBAR + 0x1000;
const GICC: usize = CBAR + 0x2000;

const PRIORITY: u8 = 0xa0;
const PRIORITY_MASK: u32 = 0xf0;
const SPURIOUS: u32 = 1023;

pub fn init(lines: &[u32]) {
    write32(GICD, 0);
    for &line in lines {
        let line = line as usize;
        write8(GICD + 0x400 + line, PRIORITY);
        write8(GICD + 0x800 + line, 1);
        // Peripheral phase pulses are edges: a level line would drop the
        // pending bit again when the pulse falls.
        let config = GICD + 0xc00 + line / 16 * 4;
        write32(config, read32(config) | 2 << (line % 16 * 2));
        write32(GICD + 0x100 + line / 32 * 4, 1 << (line % 32));
    }
    write32(GICD, 1);
    write32(GICC + 4, PRIORITY_MASK);
    write32(GICC + 8, 0);
    write32(GICC, 1);
}

#[inline(always)]
fn wfi() {
    unsafe { core::arch::asm!("wfi", options(nomem, nostack, preserves_flags)) }
}

/// Acknowledge and complete one pending interrupt, if any.
pub fn take() -> Option<u32> {
    let iar = read32(GICC + 0xc);
    let line = iar & 0x3ff;
    if line == SPURIOUS {
        return None;
    }
    write32(GICC + 0x10, iar);
    Some(line)
}

/// Sleep until `line` fires; other enabled lines are acknowledged and dropped.
pub fn wait(line: u32) {
    loop {
        while let Some(taken) = take() {
            if taken == line {
                return;
            }
        }
        wfi();
    }
}

/// Drop a stale latched edge on `line` so the next `wait` sees a fresh one.
pub fn discard(line: u32) -> bool {
    let mut seen = false;
    while let Some(taken) = take() {
        seen |= taken == line;
    }
    seen
}
