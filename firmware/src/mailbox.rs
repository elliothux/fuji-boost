//! Guest status words the host reads from RAM; word indices and statuses are
//! the `mailbox` section of `protocol.json`. Per-frame work samples go to the
//! ring at `WORK_RING_WORD`, indexed by `W_WORK_SAMPLES % WORK_RING_LEN`.
use crate::mmio::{read32, write32};
pub use crate::protocol::*;

#[inline]
pub fn get(word: usize) -> u32 {
    read32(MAILBOX_BASE + word * 4)
}

#[inline]
pub fn set(word: usize, value: u32) {
    write32(MAILBOX_BASE + word * 4, value)
}

#[inline]
pub fn increment(word: usize) {
    set(word, get(word).wrapping_add(1))
}

pub fn init() {
    for word in 0..WORK_RING_WORD + WORK_RING_LEN {
        set(word, 0);
    }
    set(W_MAGIC, MAILBOX_MAGIC);
    set(W_VERSION, MAILBOX_VERSION);
    set(W_STATUS, STATUS_BOOTING);
}

pub fn record_work(ticks: u64) {
    let ticks = ticks.min(u32::MAX as u64) as u32;
    let index = get(W_WORK_SAMPLES);
    set(WORK_RING_WORD + index as usize % WORK_RING_LEN, ticks);
    set(W_WORK_SAMPLES, index.wrapping_add(1));
    set(W_WORK_LAST, ticks);
    if ticks > get(W_WORK_MAX) {
        set(W_WORK_MAX, ticks);
    }
}
