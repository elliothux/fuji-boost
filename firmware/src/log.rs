//! RAM log ring. The emulator's UART sink only counts bytes, so text goes
//! here: word 0 magic, word `LOG_HEAD_WORD` total bytes ever written, bytes
//! from `LOG_DATA_OFFSET` wrap within the rest of the ring.
use crate::mmio::{read32, write32, write8};
use crate::protocol::{LOG_BASE, LOG_DATA_OFFSET, LOG_HEAD_WORD, LOG_MAGIC, LOG_SIZE};
use core::fmt;

const HEAD: usize = LOG_BASE + LOG_HEAD_WORD * 4;
const DATA: usize = LOG_BASE + LOG_DATA_OFFSET;
const CAPACITY: usize = LOG_SIZE - LOG_DATA_OFFSET;

pub fn init() {
    write32(LOG_BASE, LOG_MAGIC);
    write32(HEAD, 0);
}

pub fn write_bytes(bytes: &[u8]) {
    let mut head = read32(HEAD);
    for &byte in bytes {
        write8(DATA + head as usize % CAPACITY, byte);
        head = head.wrapping_add(1);
    }
    write32(HEAD, head);
}

pub struct Writer;

impl fmt::Write for Writer {
    fn write_str(&mut self, text: &str) -> fmt::Result {
        write_bytes(text.as_bytes());
        Ok(())
    }
}

#[macro_export]
macro_rules! log {
    ($($arg:tt)*) => {{
        let _ = core::fmt::Write::write_fmt(&mut $crate::log::Writer, format_args!($($arg)*));
        $crate::log::write_bytes(b"\n");
    }};
}

#[no_mangle]
pub extern "C" fn fuji_log_write(bytes: *const u8, length: usize) {
    if !bytes.is_null() {
        write_bytes(unsafe { core::slice::from_raw_parts(bytes, length) });
    }
}
