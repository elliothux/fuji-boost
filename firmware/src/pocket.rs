//! PocketJS host: QuickJS + pocket_runtime.c + ui-cabi software raster.
//! The app bundle is the build-time default embedded in the image, or a
//! bundle the host staged in the app slot (hot reload).
use crate::frame::App;
use crate::hal::scene;
use crate::hal::timer;
use crate::hal::touch::{self, Point, FINGERS};
use crate::layout::*;
use crate::mailbox::{self, *};
use core::ffi::{c_char, CStr};

#[repr(C)]
#[derive(Clone, Copy)]
struct Contact {
    id: i32,
    x: i32,
    y: i32,
    hit: i32,
}

#[repr(C)]
struct ContactsInput {
    buttons: u32,
    contact_count: u32,
    contacts: [Contact; 8],
    cancelled_count: u32,
    cancelled: [i32; 8],
}

extern "C" {
    fn pocket_runtime_boot(
        java_script: *const u8,
        java_script_length: usize,
        pack: *const u8,
        pack_length: usize,
        width: i32,
        height: i32,
    ) -> i32;
    fn pocket_runtime_frame_contacts(input: *const ContactsInput, tick_count: u32) -> i32;
    fn pocket_runtime_render() -> *const u8;
    fn pocket_runtime_width() -> u32;
    fn pocket_runtime_height() -> u32;
    fn pocket_runtime_stride() -> u32;
    fn pocket_runtime_damage_bounds(bounds: *mut i32) -> i32;
    fn pocket_runtime_damage_pixels() -> u32;
    fn pocket_runtime_error() -> *const c_char;
    fn pocket_runtime_hit_test_bounds(x: f32, y: f32) -> i32;
    fn pocket_runtime_action_name() -> *const c_char;
    fn pocket_runtime_action_value() -> i32;
    fn pocket_runtime_action_sequence() -> u32;
    fn fuji_heap_publish();
    fn malloc(size: usize) -> *mut u8;
    fn free(pointer: *mut u8);
}

#[repr(C, align(16))]
struct Aligned<T: ?Sized>(T);

/// JS_Eval reads one byte past the source: it must be NUL-terminated.
#[repr(C)]
struct Terminated<const N: usize>([u8; N], u8);

const DEFAULT_JS_LENGTH: usize = include_bytes!(env!("FUJI_APP_JS")).len();
static DEFAULT_JS: Terminated<DEFAULT_JS_LENGTH> =
    Terminated(*include_bytes!(env!("FUJI_APP_JS")), 0);
static DEFAULT_PAK: &Aligned<[u8]> = &Aligned(*include_bytes!(env!("FUJI_APP_PAK")));

/// The core simulates at 60 Hz; the panel presents at 30 Hz.
const TICKS_PER_FRAME: u32 = 2;
const ERROR_COLOR: u32 = 0xff5a_1010;

struct Bundle {
    js: *const u8,
    js_length: usize,
    pak: *const u8,
    pak_length: usize,
    /// Heap copy of a slot bundle; the embedded default is static.
    owned: bool,
    generation: u32,
}

impl Bundle {
    fn embedded() -> Self {
        Self {
            js: DEFAULT_JS.0.as_ptr(),
            js_length: DEFAULT_JS_LENGTH,
            pak: DEFAULT_PAK.0.as_ptr(),
            pak_length: DEFAULT_PAK.0.len(),
            owned: false,
            generation: 0,
        }
    }

    /// Copy a valid slot bundle out of the slot, so the host can stage the
    /// next one while this one still runs.
    fn from_slot() -> Option<Self> {
        let word = |index: usize| unsafe { *(APP_SLOT_BASE as *const u32).add(index) };
        if word(H_MAGIC) != APP_MAGIC || word(H_VERSION) != APP_VERSION {
            return None;
        }
        let js_offset = word(H_JAVA_SCRIPT_OFFSET) as usize;
        let js_length = word(H_JAVA_SCRIPT_LENGTH) as usize;
        let (pak_offset, pak_length) = (word(H_PACK_OFFSET) as usize, word(H_PACK_LENGTH) as usize);
        if js_length == 0
            || js_offset < APP_HEADER_BYTES
            || js_offset + js_length > APP_SLOT_SIZE
            || pak_offset + pak_length > APP_SLOT_SIZE
            || pak_offset % APP_PACK_ALIGN != 0
        {
            crate::log!("app slot: bad layout");
            return None;
        }
        let js = unsafe {
            core::slice::from_raw_parts((APP_SLOT_BASE + js_offset) as *const u8, js_length)
        };
        let pak = unsafe {
            core::slice::from_raw_parts((APP_SLOT_BASE + pak_offset) as *const u8, pak_length)
        };
        if fnv1a(&[js, pak]) != word(H_CHECKSUM) {
            crate::log!("app slot: checksum mismatch");
            return None;
        }
        // NUL-terminated: JS_Eval reads one byte past the source.
        let copy = |bytes: &[u8]| unsafe {
            let target = malloc(bytes.len() + 1);
            if !target.is_null() {
                core::ptr::copy_nonoverlapping(bytes.as_ptr(), target, bytes.len());
                *target.add(bytes.len()) = 0;
            }
            target
        };
        let (js_copy, pak_copy) = (copy(js), copy(pak));
        if js_copy.is_null() || pak_copy.is_null() {
            crate::log!("app slot: out of memory");
            return None;
        }
        Some(Self {
            js: js_copy,
            js_length,
            pak: pak_copy,
            pak_length,
            owned: true,
            generation: word(H_GENERATION),
        })
    }

    fn release(self) {
        if self.owned {
            unsafe {
                free(self.js as *mut u8);
                free(self.pak as *mut u8);
            }
        }
    }
}

fn fnv1a(parts: &[&[u8]]) -> u32 {
    let mut hash = 0x811c_9dc5u32;
    for part in parts {
        for &byte in *part {
            hash = (hash ^ u32::from(byte)).wrapping_mul(0x0100_0193);
        }
    }
    hash
}

fn runtime_error() -> &'static str {
    let error = unsafe { pocket_runtime_error() };
    if error.is_null() {
        return "unknown";
    }
    unsafe { CStr::from_ptr(error) }
        .to_str()
        .unwrap_or("non-UTF-8 error")
}

pub struct Pocket {
    bundle: Option<Bundle>,
    running: bool,
    framebuffer: *const u8,
    /// Union of damage not yet copied to the present buffer: x0, y0, x1, y1.
    damage: Option<[i32; 4]>,
    error_screen: bool,
    /// Guest contact per panel finger, assigned at its down edge.
    contacts: [Option<Contact>; FINGERS],
    next_contact: i32,
    scene_generation: u32,
    /// Host posted a new scene; refresh holes without copying chroma into present.
    scene_dirty: bool,
}

impl Pocket {
    pub fn boot() -> Self {
        let mut pocket = Self {
            bundle: None,
            running: false,
            framebuffer: core::ptr::null(),
            damage: None,
            error_screen: false,
            contacts: [const { None }; FINGERS],
            next_contact: 0,
            scene_generation: 0,
            scene_dirty: false,
        };
        let bundle = Bundle::from_slot().unwrap_or_else(Bundle::embedded);
        pocket.start(bundle);
        pocket
    }

    fn start(&mut self, bundle: Bundle) {
        mailbox::set(W_APP_SOURCE, u32::from(bundle.owned));
        mailbox::set(W_APP_GENERATION, bundle.generation);
        let start = timer::now();
        let ok = unsafe {
            pocket_runtime_boot(
                bundle.js,
                bundle.js_length,
                bundle.pak,
                bundle.pak_length,
                RENDER_WIDTH as i32,
                RENDER_HEIGHT as i32,
            )
        };
        mailbox::set(W_BOOT_WORK, (timer::now() - start) as u32);
        unsafe { fuji_heap_publish() };
        // The previous bundle stayed borrowed until boot shut its runtime down.
        if let Some(previous) = self.bundle.replace(bundle) {
            previous.release();
        }
        self.framebuffer = core::ptr::null();
        self.damage = None;
        // Re-apply the host scene on the first present after (re)boot.
        self.scene_generation = 0;
        self.scene_dirty = false;
        // A finger still down across a reload is a new press to the new app.
        self.contacts = [None; FINGERS];
        if ok == 0 {
            self.fail("boot");
            return;
        }
        crate::log!(
            "pocket: booted generation {} in {} ticks",
            mailbox::get(W_APP_GENERATION),
            mailbox::get(W_BOOT_WORK)
        );
        self.running = true;
        self.error_screen = false;
        if mailbox::get(W_STATUS) == STATUS_APP_ERROR {
            mailbox::set(W_STATUS, STATUS_RUNNING);
        }
    }

    fn fail(&mut self, stage: &str) {
        crate::log!("pocket: {stage} failed: {}", runtime_error());
        self.running = false;
        self.framebuffer = core::ptr::null();
        self.damage = None;
        self.error_screen = true;
        mailbox::set(W_STATUS, STATUS_APP_ERROR);
    }

    fn publish_action(&self) {
        let sequence = unsafe { pocket_runtime_action_sequence() };
        if sequence == mailbox::get(W_ACTION_SEQUENCE) {
            return;
        }
        mailbox::set(W_ACTION_VALUE, unsafe { pocket_runtime_action_value() }
            as u32);
        let name = unsafe { pocket_runtime_action_name() };
        let bytes = if name.is_null() {
            &[][..]
        } else {
            unsafe { CStr::from_ptr(name) }.to_bytes()
        };
        let mut words = [0u32; ACTION_NAME_WORDS];
        for (index, &byte) in bytes.iter().take(ACTION_NAME_WORDS * 4 - 1).enumerate() {
            words[index / 4] |= u32::from(byte) << (index % 4 * 8);
        }
        for (index, word) in words.iter().enumerate() {
            mailbox::set(W_ACTION_NAME + index, *word);
        }
        mailbox::set(W_ACTION_SEQUENCE, sequence);
    }
}

impl Pocket {
    /// Track panel fingers as guest contacts in logical pixels, in down
    /// order; the bounds hit is resolved once at each down edge.
    fn sample_contacts(&mut self, fingers: &[Option<Point>; FINGERS], input: &mut ContactsInput) {
        for (contact, finger) in self.contacts.iter_mut().zip(fingers) {
            let Some(point) = finger else {
                *contact = None;
                continue;
            };
            let x = i32::from(point.x) * RENDER_WIDTH as i32 / i32::from(touch::WIDTH);
            let y = i32::from(point.y) * RENDER_HEIGHT as i32 / i32::from(touch::HEIGHT);
            let tracked = contact.get_or_insert_with(|| {
                let id = self.next_contact;
                self.next_contact = (self.next_contact + 1) & 255;
                Contact {
                    id,
                    x,
                    y,
                    hit: unsafe { pocket_runtime_hit_test_bounds(x as f32, y as f32) },
                }
            });
            tracked.x = x;
            tracked.y = y;
        }
        let mut active: [Option<Contact>; FINGERS] = self.contacts;
        // Ids increase with each press (mod 256), so they give down order.
        if let [Some(a), Some(b)] = active {
            if (b.id - a.id) & 255 > 128 {
                active = [Some(b), Some(a)];
            }
        }
        for contact in active.into_iter().flatten() {
            input.contacts[input.contact_count as usize] = contact;
            input.contact_count += 1;
        }
    }
}

impl App for Pocket {
    fn frame(&mut self, buttons: u32, fingers: &[Option<Point>; FINGERS]) -> bool {
        if !self.running {
            return self.error_screen;
        }
        let mut input = ContactsInput {
            buttons,
            contact_count: 0,
            contacts: [Contact {
                id: 0,
                x: 0,
                y: 0,
                hit: 0,
            }; 8],
            cancelled_count: 0,
            cancelled: [0; 8],
        };
        self.sample_contacts(fingers, &mut input);
        let start = timer::now();
        if unsafe { pocket_runtime_frame_contacts(&input, TICKS_PER_FRAME) } == 0 {
            self.fail("frame");
            return true;
        }
        let rendered = timer::now();
        let framebuffer = unsafe { pocket_runtime_render() };
        let end = timer::now();
        mailbox::set(W_JS_WORK_LAST, (rendered - start) as u32);
        mailbox::set(W_RENDER_WORK_LAST, (end - rendered) as u32);
        let (width, height, stride) = unsafe {
            (
                pocket_runtime_width(),
                pocket_runtime_height(),
                pocket_runtime_stride(),
            )
        };
        if framebuffer.is_null()
            || width != RENDER_WIDTH
            || height != RENDER_HEIGHT
            || stride != width * 4
        {
            crate::log!("pocket: framebuffer {width}x{height} stride {stride}");
            self.fail("render");
            return true;
        }
        self.framebuffer = framebuffer;
        let mut bounds = [0i32; 4];
        if unsafe { pocket_runtime_damage_bounds(bounds.as_mut_ptr()) } != 0 {
            self.damage = Some(match self.damage {
                Some(d) => [
                    d[0].min(bounds[0]),
                    d[1].min(bounds[1]),
                    d[2].max(bounds[2]),
                    d[3].max(bounds[3]),
                ],
                None => bounds,
            });
        }
        mailbox::set(W_DAMAGE_PIXELS, unsafe { pocket_runtime_damage_pixels() });
        self.publish_action();
        unsafe { fuji_heap_publish() };
        if scene::poll(&mut self.scene_generation) {
            self.scene_dirty = true;
        }
        self.damage.is_some() || self.scene_dirty
    }

    fn present(&mut self) {
        let width = RENDER_WIDTH as usize;
        let height = RENDER_HEIGHT as usize;
        let present =
            unsafe { core::slice::from_raw_parts_mut(PRESENT_BASE as *mut u32, width * height) };
        if self.error_screen {
            present.fill(ERROR_COLOR);
            self.error_screen = false;
            return;
        }
        if self.framebuffer.is_null() {
            self.damage = None;
            self.scene_dirty = false;
            return;
        }
        let source =
            unsafe { core::slice::from_raw_parts(self.framebuffer as *const u32, width * height) };
        if let Some([x0, y0, x1, y1]) = self.damage.take() {
            scene::compose(
                present,
                source,
                x0.clamp(0, RENDER_WIDTH as i32) as usize,
                y0.clamp(0, RENDER_HEIGHT as i32) as usize,
                x1.clamp(0, RENDER_WIDTH as i32) as usize,
                y1.clamp(0, RENDER_HEIGHT as i32) as usize,
            );
        }
        if self.scene_dirty {
            self.scene_dirty = false;
            scene::refresh(present, source);
        }
    }

    fn reload(&mut self) {
        match Bundle::from_slot() {
            Some(bundle) => self.start(bundle),
            None => crate::log!("pocket: reload ignored, app slot invalid"),
        }
    }
}
