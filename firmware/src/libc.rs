//! Rust services behind the C mini libc (firmware/libc).
use crate::hal::timer;
use crate::mailbox;
use core::ffi::{c_char, CStr};

#[no_mangle]
extern "C" fn fuji_ticks() -> u64 {
    timer::now()
}

#[no_mangle]
extern "C" fn fuji_mailbox_set(word: u32, value: u32) {
    mailbox::set(word as usize, value);
}

/// `abort`, `exit`, failed asserts, and ui-cabi's panic/alloc-error handlers.
#[no_mangle]
extern "C" fn fuji_abort(reason: *const c_char) -> ! {
    let reason = if reason.is_null() {
        "abort"
    } else {
        unsafe { CStr::from_ptr(reason) }
            .to_str()
            .unwrap_or("abort")
    };
    crate::log!("abort: {reason}");
    crate::fault(9, 0, 0)
}
