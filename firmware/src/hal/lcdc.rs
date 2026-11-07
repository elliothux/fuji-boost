//! LCDC1 at 0xfffa9000: one straight-alpha BGRA picture plane scaled to the
//! panel. The emulator's pixel clock is 2 ticks/pixel (27 MHz at 54 MHz), so
//! 1000 x 900 total pixels give a 1.8M-tick (30 Hz) frame.
use crate::layout::{PANEL_HEIGHT, PANEL_WIDTH};
use crate::mmio::write32;

const BASE: usize = 0xfffa_9000;
pub const IRQ_ACTIVE_END: u32 = 117;
/// Raised right after the frame is captured at the end of vertical blank.
pub const IRQ_CAPTURED: u32 = 118;

const H_BLANK: u32 = 100;
const V_BLANK: u32 = 300;
const TICKS_PER_PIXEL: u64 = 2;
pub const FRAME_TICKS: u64 =
    (PANEL_WIDTH + H_BLANK) as u64 * (PANEL_HEIGHT + V_BLANK) as u64 * TICKS_PER_PIXEL;
const _: () = assert!(FRAME_TICKS == crate::protocol::MAILBOX_FRAME_TICKS);

fn reg(offset: usize, value: u32) {
    write32(BASE + offset, value);
}

/// Program timing and the picture plane, then enable output.
pub fn init(address: u32, width: u32, height: u32) {
    reg(0x10, PANEL_WIDTH + H_BLANK);
    reg(0x14, PANEL_WIDTH | H_BLANK << 16);
    reg(0x18, 0);
    reg(0x1c, PANEL_HEIGHT + V_BLANK);
    reg(0x20, PANEL_HEIGHT | V_BLANK << 16);
    reg(0x24, 0);
    set_plane(address, width, height);
    reg(0x8, 0x11000);
    reg(0x4, 1);
}

/// Q12 sampling step: source pixels advanced per panel pixel.
fn step(source: u32, panel: u32) -> u32 {
    (source << 12) / panel
}

/// Only call right after `IRQ_CAPTURED`: the primary and alternate
/// addresses must match whenever the next capture samples them.
pub fn set_plane(address: u32, width: u32, height: u32) {
    reg(0x40, 0);
    reg(0x44, address);
    reg(0x48, 0);
    reg(0x4c, address);
    reg(0x50, 0x10000 | width);
    reg(0x54, width | height << 16);
    reg(0x58, 0);
    reg(0x5c, PANEL_WIDTH | PANEL_HEIGHT << 16);
    reg(
        0x60,
        step(width, PANEL_WIDTH) | step(height, PANEL_HEIGHT) << 16,
    );
    reg(0x64, 0);
    reg(0x68, 0);
}
