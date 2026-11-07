//! Rear touch panel: F_I2C unit 2 at 0xfff3f000, panel at 7-bit address
//! 0x55, INT on GIC153, raw coordinates 900x600; layout and report format
//! from the stock 2.12 driver (docs/research/input/2026-11-07.md).
//!
//! The frame loop acknowledges and drops every enabled GIC line, so INT is
//! configured as an edge but left disabled: the distributor still latches it,
//! and `poll` reads the pending bit once per frame. Bytes are polled on
//! BCR.INT, which the controller sets whether or not BCR.INTE is on.
use crate::mmio::{read32, write32};

const BASE: usize = 0xfff3_f000;
const BSR: usize = BASE;
const BCR: usize = BASE + 0x04;
const CCR: usize = BASE + 0x08;
const DAR: usize = BASE + 0x10;
const CSR: usize = BASE + 0x14;
const FSR: usize = BASE + 0x18;

const BSR_BB: u32 = 0x80;
const BSR_LRB: u32 = 0x10;
const BCR_SCC: u32 = 0x20;
const BCR_MSS: u32 = 0x10;
const BCR_ACK: u32 = 0x08;
const BCR_INT: u32 = 0x01;
/// Enable, standard mode, the stock clock divider.
const CCR_STANDARD: u32 = 0x20 | 12;

const ADDRESS: u32 = 0x55;
const REPORT: u8 = 0x10;
const RECORDS: u8 = 0x11;

const GICD: usize = 0xffff_1000;
const IRQ: usize = 153;

/// Far longer than one 400 kHz byte at any core speed.
const BYTE_POLLS: u32 = 100_000;

pub const WIDTH: u16 = 900;
pub const HEIGHT: u16 = 600;
/// Finger ids 1 and 2.
pub const FINGERS: usize = 2;

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Point {
    pub x: u16,
    pub y: u16,
}

pub fn init() {
    write32(FSR, 3);
    write32(CSR, 8);
    write32(CCR, CCR_STANDARD);
    write32(BCR, 0);
    let config = GICD + 0xc00 + IRQ / 16 * 4;
    write32(config, read32(config) | 2 << (IRQ % 16 * 2));
}

/// Take the latched INT edge, if any.
fn take_interrupt() -> bool {
    let bit = 1 << (IRQ % 32);
    let pending = read32(GICD + 0x200 + IRQ / 32 * 4) & bit != 0;
    if pending {
        write32(GICD + 0x280 + IRQ / 32 * 4, bit);
    }
    pending
}

struct Nack;

fn byte(control: u32) -> Result<u32, Nack> {
    write32(BCR, control);
    for _ in 0..BYTE_POLLS {
        if read32(BCR) & BCR_INT != 0 {
            return Ok(read32(BSR));
        }
    }
    Err(Nack)
}

fn stop() {
    write32(BCR, 0);
}

fn read_register(register: u8, out: &mut [u8]) -> Result<(), Nack> {
    for _ in 0..BYTE_POLLS {
        if read32(BSR) & BSR_BB == 0 {
            break;
        }
    }
    let result = (|| {
        write32(DAR, ADDRESS << 1);
        if byte(BCR_MSS)? & BSR_LRB != 0 {
            return Err(Nack);
        }
        write32(DAR, u32::from(register));
        if byte(BCR_MSS)? & BSR_LRB != 0 {
            return Err(Nack);
        }
        write32(DAR, ADDRESS << 1 | 1);
        if byte(BCR_MSS | BCR_SCC)? & BSR_LRB != 0 {
            return Err(Nack);
        }
        let last = out.len() - 1;
        for (index, slot) in out.iter_mut().enumerate() {
            byte(BCR_MSS | if index < last { BCR_ACK } else { 0 })?;
            *slot = read32(DAR) as u8;
        }
        Ok(())
    })();
    stop();
    result
}

/// Apply a pending panel report to `fingers` (indexed by finger id - 1).
/// Returns false when the panel did not answer.
pub fn poll(fingers: &mut [Option<Point>; FINGERS]) -> bool {
    if !take_interrupt() {
        return true;
    }
    let mut report = [0u8; 2];
    let mut records = [0u8; 14];
    let read = read_register(REPORT, &mut report)
        .and_then(|()| read_register(RECORDS, &mut records[..7]))
        .and_then(|()| read_register(RECORDS, &mut records[7..]));
    if read.is_err() || report[1] & 0x18 != 0 {
        return false;
    }
    for record in records
        .chunks_exact(7)
        .take(usize::from(report[0]).min(FINGERS))
    {
        let id = usize::from(record[0] & 0x0f);
        if !(1..=FINGERS).contains(&id) {
            continue;
        }
        fingers[id - 1] = (record[0] & 0x80 != 0).then(|| Point {
            x: u16::from_le_bytes([record[1], record[2]]).min(WIDTH - 1),
            y: u16::from_le_bytes([record[3], record[4]]).min(HEIGHT - 1),
        });
    }
    true
}
