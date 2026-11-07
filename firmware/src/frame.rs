//! Host frame loop shared by every app: wait for the LCDC capture edge,
//! publish the previous frame's pixels, sample buttons and the touch panel,
//! run one app frame.
use crate::hal::touch::{self, Point, FINGERS};
use crate::hal::{gic, gpio, lcdc, timer};
use crate::mailbox::{self, *};

pub trait App {
    /// Advance one display frame with the fingers on the panel (by finger id
    /// - 1, raw panel pixels). Returns true when `present` has new pixels.
    fn frame(&mut self, buttons: u32, fingers: &[Option<Point>; FINGERS]) -> bool;
    /// Copy finished pixels into the scanned-out buffer.
    fn present(&mut self);
    /// Host-requested app reload (mailbox `W_RELOAD`).
    fn reload(&mut self) {}
}

/// Publishing must finish well before the next capture samples the buffer.
const PRESENT_WINDOW: u64 = lcdc::FRAME_TICKS * 3 / 4;
const EDGE_SLACK: u64 = lcdc::FRAME_TICKS / 16;

/// Frame phase reference. Re-taken after an app (re)boot, whose blocking
/// gap is boot work rather than missed frames.
struct Anchor {
    origin: u64,
    vsyncs: u32,
    missed: u32,
}

fn anchor() -> Anchor {
    // An edge latched during a long app boot is stale: the phase origin must
    // be a capture observed as it happens.
    gic::discard(lcdc::IRQ_CAPTURED);
    wait_capture();
    Anchor {
        // Wake latency differs by a few ticks between waits; anchoring the
        // phase a little before the edge keeps a prompt wake from wrapping
        // to "late".
        origin: timer::now() - EDGE_SLACK,
        vsyncs: mailbox::get(W_VSYNCS),
        missed: mailbox::get(W_MISSED_VSYNCS),
    }
}

pub fn run(app: &mut impl App) -> ! {
    let mut anchor = anchor();
    let mut pending = false;
    let mut first_present = true;
    let mut fingers = [None; FINGERS];
    let mut panel_answered = true;
    if mailbox::get(W_STATUS) == STATUS_BOOTING {
        mailbox::set(W_STATUS, STATUS_RUNNING);
    }
    loop {
        let start = timer::now();
        let since = start - anchor.origin;
        let elapsed = mailbox::get(W_VSYNCS).wrapping_sub(anchor.vsyncs);
        mailbox::set(
            W_MISSED_VSYNCS,
            anchor.missed + ((since / lcdc::FRAME_TICKS) as u32).wrapping_sub(elapsed),
        );
        if since % lcdc::FRAME_TICKS > PRESENT_WINDOW {
            // The latched edge is stale; take the next capture instead.
            gic::discard(lcdc::IRQ_CAPTURED);
            wait_capture();
            continue;
        }
        if pending {
            app.present();
            pending = false;
            mailbox::increment(W_PRESENTS);
            if first_present {
                first_present = false;
                let at = timer::now();
                mailbox::set(W_FIRST_PRESENT_LO, at as u32);
                mailbox::set(W_FIRST_PRESENT_HI, (at >> 32) as u32);
            }
        }
        mailbox::set(W_PRESENT_WORK_LAST, (timer::now() - start) as u32);
        if mailbox::get(W_RELOAD) != 0 {
            app.reload();
            mailbox::set(W_RELOAD, 0);
            pending = false;
            anchor = self::anchor();
            continue;
        }
        let buttons = gpio::buttons();
        mailbox::set(W_BUTTONS, buttons);
        let answered = touch::poll(&mut fingers);
        if panel_answered && !answered {
            crate::log!("touch panel did not answer");
        }
        panel_answered = answered;
        pending |= app.frame(buttons, &fingers);
        mailbox::increment(W_FRAMES);
        let work = timer::now() - start;
        mailbox::record_work(work);
        if work > lcdc::FRAME_TICKS {
            mailbox::increment(W_OVERRUNS);
            crate::log!(
                "overrun: frame {} work {work} js {} render {}",
                mailbox::get(W_FRAMES),
                mailbox::get(W_JS_WORK_LAST),
                mailbox::get(W_RENDER_WORK_LAST)
            );
        }
        wait_capture();
    }
}

fn wait_capture() {
    gic::wait(lcdc::IRQ_CAPTURED);
    mailbox::increment(W_VSYNCS);
}
