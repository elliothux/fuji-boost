#![no_std]
#![no_main]
// Each image uses a subset of the protocol and layout constants.
#![allow(dead_code)]

mod frame;
mod hal;
mod layout;
mod log;
mod mailbox;
mod mmio;
mod protocol;

// ui-cabi supplies the global allocator (over C malloc) and the panic and
// alloc-error handlers (C abort) besides the retained UI core.
#[cfg(feature = "pocket")]
extern crate pocketjs_symbian_core;

#[cfg(not(feature = "pocket"))]
mod hello;
#[cfg(feature = "pocket")]
mod libc;
#[cfg(feature = "pocket")]
mod pocket;

use hal::{gic, lcdc, timer, touch};
use layout::{PRESENT_BASE, RENDER_HEIGHT, RENDER_WIDTH};
use mailbox::*;

core::arch::global_asm!(include_str!("start.S"), options(raw));

#[no_mangle]
extern "C" fn fuji_main() -> ! {
    log::init();
    mailbox::init();
    timer::init();
    unsafe {
        core::ptr::write_bytes(
            PRESENT_BASE as *mut u32,
            0,
            (RENDER_WIDTH * RENDER_HEIGHT) as usize,
        );
    }
    mailbox::set(W_RENDER_SIZE, RENDER_WIDTH | RENDER_HEIGHT << 16);
    mailbox::set(W_PRESENT_ADDRESS, PRESENT_BASE as u32);
    gic::init(&[lcdc::IRQ_CAPTURED]);
    lcdc::init(PRESENT_BASE as u32, RENDER_WIDTH, RENDER_HEIGHT);
    touch::init();
    crate::log!("fuji-boost {} up", env!("CARGO_PKG_VERSION"));

    #[cfg(not(feature = "pocket"))]
    frame::run(&mut hello::Hello::new());
    #[cfg(feature = "pocket")]
    frame::run(&mut pocket::Pocket::boot());
}

pub fn fault(kind: u32, pc: u32, psr: u32) -> ! {
    mailbox::set(W_FAULT_KIND, kind);
    mailbox::set(W_FAULT_PC, pc);
    mailbox::set(W_FAULT_PSR, psr);
    mailbox::set(W_STATUS, STATUS_FAULT);
    crate::log!("fault kind={kind} pc={pc:#010x} psr={psr:#010x}");
    loop {
        unsafe { core::arch::asm!("wfi") }
    }
}

#[no_mangle]
extern "C" fn fuji_exception(kind: u32, lr: u32, psr: u32) -> ! {
    fault(kind, lr, psr)
}

#[cfg(not(feature = "pocket"))]
#[panic_handler]
fn panic(info: &core::panic::PanicInfo<'_>) -> ! {
    crate::log!("panic: {info}");
    fault(8, 0, 0)
}
