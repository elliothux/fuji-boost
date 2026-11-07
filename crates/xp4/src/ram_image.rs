//! Raw RAM images on the stock X-T4 board profile. No DAT, recipe or
//! cold-loader edits are involved: the image is copied to RAM and every core
//! starts at its entry. The session only knows the hardware; what the image
//! keeps in RAM (logs, status words, app slots) is the image's own business.
use super::{BOARD_PROFILE, Contact, Execution, TOUCH_PANEL, VIRTUAL_BUTTONS};
use fujimod_emulator_cpu::arm;
use fujimod_emulator_xp4_board::{Definition, Session};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// LCDC1 publishes the rear panel on this scanout output.
pub const PANEL_OUTPUT: u32 = 1;
/// Page 0 is unmapped on the real camera.
const LOWEST_LOAD: u32 = 0x1000;
/// Names for `VIRTUAL_BUTTONS`, in the same order.
const NAVIGATION: [&str; 6] = ["up", "down", "left", "right", "ok", "back"];

pub struct Image<'a> {
    pub bytes: &'a [u8],
    pub load_address: u32,
    pub entry: u32,
}

pub struct ImageSession {
    board: Session,
    identity: String,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Saved {
    schema: u32,
    identity: String,
    #[serde(with = "super::checkpoint::serde_bytes_vec")]
    board: Vec<u8>,
}

/// Every measured key is active low; an image starts with all keys released.
fn released_keys() -> Vec<(usize, usize)> {
    let measured: Value =
        serde_json::from_str(include_str!("../../../profiles/measured/xt4-controls.json")).unwrap();
    let mut pins: Vec<(usize, usize)> = VIRTUAL_BUTTONS
        .iter()
        .map(|&(_, port, pin, _)| (port, pin))
        .collect();
    for lines in measured["buttons"].as_object().unwrap().values() {
        for line in lines.as_array().unwrap() {
            pins.push((
                line[0].as_u64().unwrap() as usize,
                line[1].as_u64().unwrap() as usize,
            ));
        }
    }
    pins
}

impl ImageSession {
    pub fn new(image: &Image<'_>) -> Result<Self, String> {
        let mut definition: Definition =
            serde_json::from_str(BOARD_PROFILE).map_err(|e| e.to_string())?;
        let end = u64::from(image.load_address) + image.bytes.len() as u64;
        if image.bytes.is_empty()
            || image.load_address < LOWEST_LOAD
            || end > definition.ram_size as u64
            || !(u64::from(image.entry) >= u64::from(image.load_address)
                && u64::from(image.entry) < end)
            || image.entry & 3 != 0
        {
            return Err(format!(
                "Image must sit in RAM above {LOWEST_LOAD:#x} with an ARM entry inside it"
            ));
        }
        let mut levels = definition.gpio.unwrap_or([0; 16]);
        for (port, pin) in released_keys() {
            levels[port] |= 1 << pin;
        }
        definition.gpio = Some(levels);
        definition.touch_i2c = Some(TOUCH_PANEL);
        let board = Session::new(definition, None)?;
        board.write(image.load_address as usize, image.bytes)?;
        for core in 0..4 {
            board.write_register(core, arm::PC, u64::from(image.entry))?;
        }
        let identity = format!(
            "image/{}/{:#x}/{:#x}/{}",
            fujimod_firmware::sha256(image.bytes),
            image.load_address,
            image.entry,
            fujimod_firmware::sha256(BOARD_PROFILE.as_bytes()),
        );
        Ok(Self { board, identity })
    }

    pub fn identity(&self) -> &str {
        &self.identity
    }

    pub fn execute_batch(&self, budget: u32) -> Result<Execution, String> {
        if budget == 0 || budget > 1_000_000 {
            return Err("Instruction budget must be 1..1,000,000".into());
        }
        let result = self.board.run_board(u64::from(budget), 1024);
        let mut state = self.run_state();
        state.error = result.err();
        Ok(state)
    }

    pub fn run_state(&self) -> Execution {
        Execution {
            cpu: self.board.progress(),
            quiescent: self.board.quiescent(),
            guest_halt: None,
            outputs: self.board.scanout_metadata(),
            error: None,
        }
    }

    /// Advance guest time by `ticks` or execute `limit` instructions,
    /// whichever comes first. Busy cores never run past the target; when
    /// every core waits, guest time skips to the next device or timer event,
    /// so a stop may overshoot `ticks` by one idle gap (with nothing run after
    /// it). A stopped or quiescent guest returns early.
    pub fn run_for(&self, ticks: u64, limit: u64) -> Result<Execution, String> {
        let start = self.board.progress();
        let target = start.virtual_ticks.saturating_add(ticks);
        let mut next = 0;
        let mut quiescent = false;
        let mut error = None;
        loop {
            let now = self.board.state(false);
            let executed = now.instructions - start.instructions;
            if now.virtual_ticks >= target
                || executed >= limit
                || now.failure.is_some()
                || now.abort_failure.is_some()
            {
                break;
            }
            let runnable: Vec<usize> = (0..now.waiting.len())
                .filter(|&core| !now.waiting[core])
                .collect();
            let Some(&first) = runnable.first() else {
                if !self.board.idle() {
                    quiescent = true;
                    break;
                }
                continue;
            };
            // Same round robin as the board loop; a lone core runs long slices.
            let core = runnable
                .iter()
                .copied()
                .find(|&core| core >= next)
                .unwrap_or(first);
            next = (core + 1) % now.waiting.len();
            let quantum = if runnable.len() == 1 { 1_000_000 } else { 1024 };
            let budget = quantum
                .min(target - now.virtual_ticks)
                .min(limit - executed);
            if let Err(e) = self.board.run(core, budget as usize) {
                error = Some(e);
                break;
            }
        }
        let mut state = self.run_state();
        state.quiescent |= quiescent;
        state.error = error;
        Ok(state)
    }

    pub fn frame_rgba(&self, output: u32) -> Result<Vec<u8>, String> {
        self.board
            .scanout_frames()
            .into_iter()
            .find(|frame| frame.output == output as usize)
            .map(|frame| frame.rgba)
            .ok_or_else(|| "No active guest frame for this output".into())
    }

    pub fn panel_rgba(&self) -> Result<Vec<u8>, String> {
        self.frame_rgba(PANEL_OUTPUT)
    }

    pub fn read_physical_memory(&self, address: u32, length: u32) -> Result<Vec<u8>, String> {
        let mut bytes = vec![0; length as usize];
        self.board.read(address as usize, &mut bytes)?;
        Ok(bytes)
    }

    pub fn write_physical_memory(&self, address: u32, bytes: &[u8]) -> Result<(), String> {
        if u64::from(address) + bytes.len() as u64 > self.board.ram_bytes() as u64 {
            return Err("Physical write leaves RAM".into());
        }
        self.board.write(address as usize, bytes)
    }

    pub fn pc(&self, core: usize) -> Result<u32, String> {
        Ok(self.board.read_register(core, arm::PC)? as u32)
    }

    pub fn set_gpio(&self, port: usize, pin: usize, high: bool) -> Result<(), String> {
        self.board.set_gpio_input(port, pin, high)
    }

    pub fn set_adc(&self, channel: usize, value: u16) -> Result<(), String> {
        self.board.set_adc_input(channel, value)
    }

    /// Hold or release a navigation key (active-low GPIO line).
    pub fn set_button(&self, name: &str, pressed: bool) -> Result<(), String> {
        let index = NAVIGATION
            .iter()
            .position(|button| *button == name)
            .ok_or_else(|| format!("Unknown navigation key {name}"))?;
        let (_, port, pin, _) = VIRTUAL_BUTTONS[index];
        self.set_gpio(port, pin, !pressed)
    }

    /// Fingers on the rear panel, in its 900×600 raw coordinate space.
    pub fn set_touch(&self, contacts: [Option<Contact>; 2]) -> Result<(), String> {
        self.board.set_touch(contacts)
    }

    /// Host RGB888 viewfinder into a firmware-declared scene buffer.
    /// `address` is the header; pixels follow 16 bytes later. Generation 0
    /// means no frame; each call posts a new one.
    pub fn set_scene_rgb(
        &self,
        address: u32,
        width: u32,
        height: u32,
        rgb: &[u8],
        generation: u32,
    ) -> Result<(), String> {
        if width == 0
            || height == 0
            || width > 2048
            || height > 2048
            || generation == 0
            || rgb.len() as u64 != u64::from(width) * u64::from(height) * 3
        {
            return Err(
                "Scene RGB requires 1..=2048 size, width*height*3 bytes, generation>0".into(),
            );
        }
        let header = 16u32;
        let bytes = header
            .checked_add(rgb.len() as u32)
            .ok_or("Scene buffer overflow")?;
        if u64::from(address) + u64::from(bytes) > self.board.ram_bytes() as u64 {
            return Err("Scene buffer leaves RAM".into());
        }
        // Pixels first, generation last, so a concurrent reader never sees a
        // new generation with a half-written RGB plane.
        let mut header = [0u8; 16];
        header[0..4].copy_from_slice(b"FBSC");
        header[8..12].copy_from_slice(&width.to_le_bytes());
        header[12..16].copy_from_slice(&height.to_le_bytes());
        self.board.write(address as usize + 16, rgb)?;
        self.board.write(address as usize, &header)?;
        header[4..8].copy_from_slice(&generation.to_le_bytes());
        self.board.write(address as usize, &header)
    }

    pub fn state(&self) -> Value {
        let execution = self.run_state();
        json!({
            "mode": "image",
            "identity": self.identity,
            "cpu": execution.cpu,
            "quiescent": execution.quiescent,
            "outputs": execution.outputs,
        })
    }

    pub fn checkpoint(&self) -> Result<Vec<u8>, String> {
        let saved = Saved {
            schema: 1,
            identity: self.identity.clone(),
            board: self.board.checkpoint()?,
        };
        let mut bytes = Vec::new();
        ciborium::into_writer(&saved, &mut bytes).map_err(|e| format!("Checkpoint encode: {e}"))?;
        Ok(bytes)
    }

    pub fn restore_checkpoint(&self, bytes: &[u8]) -> Result<(), String> {
        let mut input = bytes;
        let saved: Saved =
            ciborium::from_reader(&mut input).map_err(|e| format!("Checkpoint decode: {e}"))?;
        if !input.is_empty() || saved.schema != 1 {
            return Err("Unknown image checkpoint format".into());
        }
        if saved.identity != self.identity {
            return Err(format!(
                "Checkpoint belongs to {}, not {}",
                saved.identity, self.identity
            ));
        }
        self.board.restore_checkpoint(&saved.board)
    }

    pub fn checkpoint_regions(&self) -> Vec<(u32, u32)> {
        self.board
            .checkpoint_regions()
            .into_iter()
            .map(|(base, size)| (base as u32, size as u32))
            .collect()
    }

    pub fn checkpoint_memory(&self, address: u32, length: u32) -> Result<Vec<u8>, String> {
        let mut bytes = vec![0; length as usize];
        self.board.checkpoint_read(u64::from(address), &mut bytes)?;
        if bytes.iter().all(|&b| b == 0) {
            bytes.clear();
        }
        Ok(bytes)
    }

    pub fn restore_memory(&self, address: u32, length: u32, bytes: &[u8]) -> Result<(), String> {
        if bytes.is_empty() {
            self.board
                .checkpoint_fill(u64::from(address), length as usize, 0)
        } else if bytes.len() == length as usize {
            self.board.checkpoint_write(u64::from(address), bytes)
        } else {
            Err("Checkpoint memory chunk length differs".into())
        }
    }
}
