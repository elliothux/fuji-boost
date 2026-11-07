//! Firmware-private physical memory map. The image base is in `link.ld`; the
//! host-visible regions (log, mailbox, app slot, scene) are in `protocol.json`.

pub const HEAP_BASE: usize = 0x1000_0000;
pub const HEAP_SIZE: usize = 0x0400_0000;
pub const PRESENT_BASE: usize = 0x1400_0000;

/// Logical render surface; LCDC1 scales it 2x to the 900x600 panel.
pub const RENDER_WIDTH: u32 = 450;
pub const RENDER_HEIGHT: u32 = 300;
pub const PANEL_WIDTH: u32 = 900;
pub const PANEL_HEIGHT: u32 = 600;
