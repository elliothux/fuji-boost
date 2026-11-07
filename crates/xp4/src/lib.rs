#![recursion_limit = "256"]

mod checkpoint;
pub mod checkpoint_file;
mod f01c_second_completion;
mod fa;
mod fa_completion;
mod fff50000_ready;
mod fff74000_ready;
mod fff7801c_completion;
pub mod input;
mod linked_dma;
mod mode2;
pub mod ram_image;
mod rec_display;
mod rec_input;
mod reprogram;
mod spi2;
#[path = "../../core/src/xt4.rs"]
mod xt4;

use fujimod_emulator_cpu::arm;
use fujimod_emulator_xp4_board::{BankedFixtureConfig, Definition, Session, Source, touch_i2c};
use fujimod_recovery::{Recipe, image};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::{
    cell::{Cell, OnceCell, RefCell},
    rc::Rc,
};
pub use touch_i2c::Contact;

#[derive(serde::Serialize)]
pub struct Execution {
    pub cpu: fujimod_emulator_xp4_board::observation::Progress,
    pub quiescent: bool,
    #[serde(rename = "guestHalt")]
    pub guest_halt: Option<&'static str>,
    pub outputs: Vec<fujimod_emulator_xp4_board::observation::Output>,
    pub error: Option<String>,
}

impl Execution {
    pub fn stopped(&self) -> bool {
        self.cpu.failure.is_some()
            || self.cpu.abort_failure.is_some()
            || self.error.is_some()
            || self.guest_halt.is_some()
    }
}

const BOARD_PROFILE: &str = include_str!("../../../profiles/boards/xt4-212.generated.json");
const ERASED_FLASH_PROFILE: &str =
    include_str!("../../../profiles/boards/xt4-erased-flash.generated.json");
const MEASURED_PROFILE: &str = include_str!("../../../profiles/measured/xt4.json");
// shortcut: synthetic outcomes/timing; replace with qualified captures for physical calibration fidelity.
const CALIBRATION_RESULTS: [[u16; 4]; 4] = [[0x300; 4]; 4];
const CALIBRATION_TICKS: u64 = 8;
// 54 MHz / 48 kHz; capture uses two 32-bit words per stereo 24-bit frame.
const AUDIO_FRAME_TICKS: u64 = 1_125;
// shortcut: aggregate instruction timing is synthetic; replace with measured XP4 CPU/peripheral timing.
const LINKED_DMA_INSTRUCTIONS_PER_TICK: (u64, u64) = (8, 1);
const POWER64_READ_REGISTERS: [u8; 2] = [64, 65];
const POWER_INIT_READ_REGISTERS: [u8; 5] = [37, 38, 69, 70, 71];
const POWER3_RAW_INPUT: u16 = 768;
const VIRTUAL_BUTTON_SOURCE_SHA256: &str =
    "1afccdd94e2379633b100d71b438a86153a892a866ef5079b6678b9df0b3ef9a";
const SPI2_CLIENT_SOURCE_SHA256: &str =
    "ab0e3061c813efe7cef0a903c1c65e9d241e1f2f209b91d9e10aeeaf37e3a815";
const VIRTUAL_BUTTONS: [(u32, usize, usize, u32); 6] = [
    (0x9f, 7, 11, 0x172c2d4),
    (0xa0, 7, 10, 0x172c274),
    (0xa1, 7, 9, 0x172c294),
    (0xa2, 7, 8, 0x172c2b4),
    (0x0f, 7, 12, 0x172c2f4),
    (0x11, 9, 5, 0x172c314),
];
/// Rear touch panel as the stock 2.12 driver uses it (I2C unit 2, address
/// 0x55, INT on GIC153; docs/research/input/2026-11-07.md). Bytes at
/// 400 kHz, reports at 100 Hz while a finger is down.
const TOUCH_PANEL: touch_i2c::Config = touch_i2c::Config {
    base: 0xfff3f000,
    irq: 67,
    touch_irq: 153,
    byte_ticks: 1_215,
    report_ticks: 540_000,
    width: 900,
    height: 600,
};
const UI_MODE_OBSERVATION_POINTS: [u64; 12] = [
    0x1131da4, 0x11255e8, 0x1131e38, 0x1131e60, 0x1131e70, 0x1131e80, 0x1131e8c, 0x1131f30,
    0x1141b04, 0x1141b34, 0x1e703fc, 0x1fb21dc,
];
const PROPERTY9_OBSERVATION_POINTS: [u64; 4] = [0x1124278, 0x11242b0, 0x11242b4, 0x11242ec];
const INPUT11_OBSERVATION_POINTS: [u64; 5] =
    [0x12036a4, 0x12036a8, 0x1167e40, 0x12b0a10, 0x12b0920];
const SETUP_REC_OBSERVATION_POINTS: [u64; 3] = [0x1131e48, 0x1131f40, 0x1131f44];
const POWEROFF_OBSERVATION_POINTS: [u64; 2] = [0x2016854, 0x2016ea0];
const POWEROFF_EVENT_OBSERVATION_POINTS: [u64; 2] = [0x1e28480, 0x1e28494];
const A00B_CLASS_OBSERVATION_POINTS: [u64; 3] = [0x1fa4cdc, 0x1131c04, 0x1131c4c];
const A00B_COPY_SOURCE_PC: u64 = 0x12818e4;
const BATTERY_STATUS_OBSERVATION_POINTS: [u64; 2] = [0x12944ac, 0x11de6e8];
const BATTERY_CACHE_OBSERVATION_POINTS: [u64; 2] = [0x11d68ac, 0x11d68dc];

fn read_virtual_memory(
    board: &Session,
    core: u32,
    address: u32,
    length: u32,
) -> Result<Vec<u8>, String> {
    if core as usize >= board.instructions().len()
        || !(1..=65536).contains(&length)
        || address.checked_add(length - 1).is_none()
    {
        return Err(
            "Virtual memory inspection requires a valid core and1..=65536 bytes within ARM32"
                .into(),
        );
    }
    // The board translates into declared memory backing and never accesses MMIO here.
    board.read_virtual(core as usize, address, length as usize)
}

fn read_physical_memory(board: &Session, address: u32, length: u32) -> Result<Vec<u8>, String> {
    if !(1..=65536).contains(&length) || address.checked_add(length - 1).is_none() {
        return Err("Physical memory inspection requires1..=65536 bytes within ARM32".into());
    }
    let mut bytes = vec![0; length as usize];
    board.read(address as usize, &mut bytes)?;
    Ok(bytes)
}

const KERNEL_WRITE_WINDOWS: [(u32, u32, &str); 2] = [
    (
        0x49408,
        28,
        "b4e39437ffb62c176a12513e1c0c9acefd556a1d4560644a6001d0faf8cd03f1",
    ),
    (
        0x58760,
        160,
        "4007c649e468c4a61fe8f08b156eb44a91188d385a0552e696f50744f51f952a",
    ),
];

fn kernel_write_readback(board: &Session) -> Result<Vec<Value>, String> {
    KERNEL_WRITE_WINDOWS.iter().map(|&(address, length, original)| {
        let bytes = read_physical_memory(board, address, length)?;
        let sha256 = fujimod_firmware::sha256(&bytes);
        Ok(json!({"physical":address,"bytes":length,"matchesOriginal":sha256==original,"sha256":sha256}))
    }).collect()
}

fn observe_spi2_client_ram(board: &Session) -> Value {
    let sample = (|| -> Result<Value, String> {
        let tx = board.read_virtual(0, 0x177b480, 4)?;
        let rx = board.read_virtual(0, 0x177bcc0, 4)?;
        let state = board.read_virtual(0, 0x1ab65a4, 1)?;
        let handle = board.read_virtual(0, 0x1ab20bc, 4)?;
        Ok(json!({"txBytes":tx,"rxBytes":rx,"clientState":state[0],
            "serialHandle":u32::from_le_bytes(handle.try_into().map_err(|_|"SPI2 handle width")?)}))
    })();
    json!({"core":0,"txAddress":0x177b480,"rxAddress":0x177bcc0,
        "stateAddress":0x1ab65a4,"handleAddress":0x1ab20bc,"maximumBytesRead":13,
        "sourceSha256":SPI2_CLIENT_SOURCE_SHA256,
        "meaning":"current virtual RAM only; not original call operands or a FIFO exchange receipt",
        "sample":sample.unwrap_or_else(|error|json!({"unavailable":error}))})
}

// register, synthetic input, API return PC, post-store PC, mirror address.
const CLASS3_READS: [(u8, u16, u32, u32, u32); 13] = [
    (47, 2000, 0x11d55e0, 0x11d5604, 0x1790f22),
    (48, 1000, 0x11d5610, 0x11d5634, 0x1790f24),
    (49, 0, 0x11d5640, 0x11d5668, 0x1790f26),
    (50, 2982, 0x11d5674, 0x11d56bc, 0x1790f28),
    (72, 0, 0x11d56c8, 0x11d56dc, 0x1790f3c),
    (111, 2000, 0x11d56e8, 0x11d570c, 0x1790f52),
    (112, 1000, 0x11d5718, 0x11d573c, 0x1790f54),
    (113, 0, 0x11d5748, 0x11d5770, 0x1790f56),
    (114, 2982, 0x11d577c, 0x11d57c4, 0x1790f58),
    (118, 2000, 0x11d57d0, 0x11d57f4, 0x1790f5a),
    (119, 1000, 0x11d5800, 0x11d5824, 0x1790f5c),
    (120, 0, 0x11d5830, 0x11d5858, 0x1790f5e),
    (121, 2982, 0x11d5864, 0x11d58ac, 0x1790f60),
];

// Explicit diagnostic stages; the virtual clock replaces the read-only clock input.
#[derive(Clone, Copy, Debug, PartialEq, PartialOrd)]
enum Probe {
    Strict,
    Discovery,
    Ready,
    Commands,
    EmptyRetained,
    ErasedFlash,
    Training,
    OrderedStartup,
    SupportedNor,
    InactiveCountdown,
    StartupInputLow,
    LatchedStartupCause,
    InvalidClock,
    VirtualClock,
    IdealCalibration,
    AudioReset,
    Power14Clear,
    DatStorage,
    Power67Clear,
    Power17Clear,
    RtcStagingReadback,
    PowerInitReads,
    RtcControlEcho,
    PackedResources,
    CatalogNor,
    FffaeRequest,
    Power64Inputs,
    Power2Echo,
    Class3Inputs,
    Power2Bit8Echo,
    Gpio9High,
    Power3Raw768,
    Spi2FirstHeader,
    Spi2SecondHeader,
    Spi2OwnerAck,
    CalibrationMode2,
    CalibrationReprogram,
    CalibrationPostControl,
    CalibrationPostMode2,
    CalibrationSelector1,
    Ff702280Request,
    FaReady,
    FaCompletion,
    Fff7801cCompletion,
    F01cSecondCompletion,
    Fff7801cSecondCompletion,
    Fff50000Ready,
    Fff50000SecondReady,
    Fff74000Ready,
    LinkedDmaInput,
}

pub struct Xp4Session {
    board: Rc<Session>,
    edits: Value,
    state_template: OnceCell<Value>,
    discovery: bool,
    power_status_override: Option<u16>,
    power_command_model: bool,
    empty_retained: bool,
    erased_flash: bool,
    flash_training_fixture: bool,
    bootstrap_instructions: u64,
    flash_jedec: Option<u32>,
    flash_identity_bytes: Option<u8>,
    inactive_countdown: bool,
    startup_gpio0_low: bool,
    latched_startup_cause: bool,
    invalid_clock: bool,
    virtual_clock: bool,
    passive_discovery: bool,
    calibration_source_sha256: Option<String>,
    calibration_mode5_sha256: Option<String>,
    calibration_mode2_sha256: Option<String>,
    calibration_reprogram_sha256: Option<String>,
    calibration_post_control_sha256: Option<String>,
    calibration_post_mode2_sha256: Option<String>,
    calibration_selector1_sha256: Option<String>,
    calibration_removal_sha256: Option<String>,
    audio_reset_source_sha256: Option<String>,
    power14_source_sha256: Option<String>,
    power67_source_sha256: Option<String>,
    power17_source_sha256: Option<String>,
    rtc_staging_source_sha256: Option<String>,
    power_init_source_sha256: Option<String>,
    rtc_control_source_sha256: Option<String>,
    dat_storage_inputs: Vec<Value>,
    packed_resource_input: Option<Value>,
    catalog_nor_inputs: Vec<Value>,
    control_request_source_sha256: Option<String>,
    ff702280_source_sha256: Option<String>,
    fa_ready_source_sha256: Option<String>,
    fa_completion_source_sha256: Option<String>,
    fff7801c_completion_source_sha256: Option<String>,
    f01c_second_completion_source_sha256: Option<String>,
    fff7801c_second_completion: bool,
    fff50000_ready_source_sha256: Option<String>,
    fff50000_second_ready: bool,
    fff74000_ready_source_sha256: Option<String>,
    linked_dma_source_sha256: Option<String>,
    power64_source_sha256: Option<String>,
    power2_echo_source_sha256: Option<String>,
    class3_source_sha256: Option<String>,
    power2_bit8_source_sha256: Option<String>,
    gpio9_source_sha256: Option<String>,
    power3_source_sha256: Option<String>,
    spi2_trace: Option<Rc<RefCell<spi2::Trace>>>,
    rec_input_trace: Option<Rc<RefCell<rec_input::Trace>>>,
    rec_state_memory_watch_started: Cell<bool>,
    spi2_second_header: bool,
    spi2_owner_ack: bool,
    virtual_buttons: RefCell<BTreeMap<u32, bool>>,
    measured_inputs: bool,
    checkpoint_identity: String,
}

/// Real-camera ff80 app80 input-register levels (`jig_baseline`).
#[derive(serde::Deserialize)]
struct MeasuredInputs {
    gpio: [u16; 16],
    adc: [u16; 12],
}
fn measured_inputs() -> Result<MeasuredInputs, String> {
    #[derive(serde::Deserialize)]
    struct Profile {
        jig_baseline: MeasuredInputs,
    }
    serde_json::from_str::<Profile>(MEASURED_PROFILE)
        .map(|p| p.jig_baseline)
        .map_err(|e| format!("Measured X-T4 profile: {e}"))
}

impl Xp4Session {
    pub fn load(source: &[u8]) -> Result<Self, String> {
        Self::prepare(source, Probe::Strict)
    }

    /// Research only: unknown accesses enter the upstream discovery ledger.
    pub fn discover(source: &[u8]) -> Result<Self, String> {
        Self::prepare(source, Probe::Discovery)
    }

    /// Declared diagnostic input for the loader's register-0 bit-15 poll.
    pub fn discover_ready(source: &[u8]) -> Result<Self, String> {
        Self::prepare(source, Probe::Ready)
    }

    /// Zero-status diagnostic scenario, with source-observed initialization commands.
    pub fn discover_commands(source: &[u8]) -> Result<Self, String> {
        Self::prepare(source, Probe::Commands)
    }

    /// Session-local words; zeroes fail the loader's retained-data marker check.
    pub fn discover_empty_retained(source: &[u8]) -> Result<Self, String> {
        Self::prepare(source, Probe::EmptyRetained)
    }

    /// Blank NOR with catalog identity/geometry; no installed-firmware layout.
    pub fn discover_erased_flash(source: &[u8]) -> Result<Self, String> {
        Self::prepare(source, Probe::ErasedFlash)
    }

    /// Ideal digital handshake plus the loader's known calibration pattern.
    pub fn discover_training(source: &[u8]) -> Result<Self, String> {
        Self::prepare(source, Probe::Training)
    }

    /// Preloaded harness order; this does not reconstruct the camera's Boot ROM.
    pub fn discover_ordered(source: &[u8]) -> Result<Self, String> {
        Self::prepare(source, Probe::OrderedStartup)
    }

    /// A synthetic manufacturer-byte input; no complete physical part ID is asserted.
    pub fn discover_supported_nor(source: &[u8]) -> Result<Self, String> {
        Self::prepare(source, Probe::SupportedNor)
    }

    pub fn discover_inactive_countdown(source: &[u8]) -> Result<Self, String> {
        Self::prepare(source, Probe::InactiveCountdown)
    }

    pub fn discover_startup_input(source: &[u8]) -> Result<Self, String> {
        Self::prepare(source, Probe::StartupInputLow)
    }

    pub fn discover_startup_cause(source: &[u8]) -> Result<Self, String> {
        Self::prepare(source, Probe::LatchedStartupCause)
    }

    pub fn discover_invalid_clock(source: &[u8]) -> Result<Self, String> {
        Self::prepare(source, Probe::InvalidClock)
    }

    pub fn discover_preloaded(source: &[u8]) -> Result<Self, String> {
        Self::prepare(source, Probe::VirtualClock)
    }

    pub fn discover_ideal_calibration(source: &[u8]) -> Result<Self, String> {
        Self::prepare(source, Probe::IdealCalibration)
    }

    pub fn discover_audio_reset(source: &[u8]) -> Result<Self, String> {
        Self::prepare(source, Probe::AudioReset)
    }

    pub fn discover_power14_clear(source: &[u8]) -> Result<Self, String> {
        Self::prepare(source, Probe::Power14Clear)
    }

    pub fn discover_dat_storage(source: &[u8]) -> Result<Self, String> {
        Self::prepare(source, Probe::DatStorage)
    }

    pub fn discover_power67_clear(source: &[u8]) -> Result<Self, String> {
        Self::prepare(source, Probe::Power67Clear)
    }

    pub fn discover_power17_clear(source: &[u8]) -> Result<Self, String> {
        Self::prepare(source, Probe::Power17Clear)
    }

    pub fn discover_rtc_staging(source: &[u8]) -> Result<Self, String> {
        Self::prepare(source, Probe::RtcStagingReadback)
    }

    pub fn discover_power_init_reads(source: &[u8]) -> Result<Self, String> {
        Self::prepare(source, Probe::PowerInitReads)
    }

    pub fn discover_rtc_control_echo(source: &[u8]) -> Result<Self, String> {
        Self::prepare(source, Probe::RtcControlEcho)
    }

    pub fn discover_packed_resources(source: &[u8]) -> Result<Self, String> {
        Self::prepare(source, Probe::PackedResources)
    }

    pub fn discover_catalog_nor(source: &[u8]) -> Result<Self, String> {
        Self::prepare(source, Probe::CatalogNor)
    }

    pub fn discover_fffae_request(source: &[u8]) -> Result<Self, String> {
        Self::prepare(source, Probe::FffaeRequest)
    }

    pub fn discover_power64_inputs(source: &[u8]) -> Result<Self, String> {
        Self::prepare(source, Probe::Power64Inputs)
    }

    pub fn discover_power2_echo(source: &[u8]) -> Result<Self, String> {
        Self::prepare(source, Probe::Power2Echo)
    }

    pub fn discover_class3_inputs(source: &[u8]) -> Result<Self, String> {
        Self::prepare(source, Probe::Class3Inputs)
    }

    pub fn discover_power2_bit8_echo(source: &[u8]) -> Result<Self, String> {
        Self::prepare(source, Probe::Power2Bit8Echo)
    }

    pub fn discover_gpio9_high(source: &[u8]) -> Result<Self, String> {
        Self::prepare(source, Probe::Gpio9High)
    }

    pub fn discover_power3_raw768(source: &[u8]) -> Result<Self, String> {
        Self::prepare(source, Probe::Power3Raw768)
    }

    pub fn discover_spi2_first_header(source: &[u8]) -> Result<Self, String> {
        Self::prepare(source, Probe::Spi2FirstHeader)
    }

    pub fn discover_spi2_second_header(source: &[u8]) -> Result<Self, String> {
        Self::prepare(source, Probe::Spi2SecondHeader)
    }

    pub fn discover_spi2_owner_ack(source: &[u8]) -> Result<Self, String> {
        Self::prepare(source, Probe::Spi2OwnerAck)
    }

    pub fn discover_calibration_mode2(source: &[u8]) -> Result<Self, String> {
        Self::prepare(source, Probe::CalibrationMode2)
    }

    pub fn discover_calibration_reprogram(source: &[u8]) -> Result<Self, String> {
        Self::prepare(source, Probe::CalibrationReprogram)
    }

    pub fn discover_calibration_post_control(source: &[u8]) -> Result<Self, String> {
        Self::prepare(source, Probe::CalibrationPostControl)
    }

    pub fn discover_calibration_post_mode2(source: &[u8]) -> Result<Self, String> {
        Self::prepare(source, Probe::CalibrationPostMode2)
    }

    pub fn discover_calibration_selector1(source: &[u8]) -> Result<Self, String> {
        Self::prepare(source, Probe::CalibrationSelector1)
    }

    pub fn discover_ff702280_request(source: &[u8]) -> Result<Self, String> {
        Self::prepare(source, Probe::Ff702280Request)
    }

    pub fn discover_fa_ready(source: &[u8]) -> Result<Self, String> {
        Self::prepare(source, Probe::FaReady)
    }

    pub fn discover_fa_completion(source: &[u8]) -> Result<Self, String> {
        Self::prepare(source, Probe::FaCompletion)
    }

    pub fn discover_fff7801c_completion(source: &[u8]) -> Result<Self, String> {
        Self::prepare(source, Probe::Fff7801cCompletion)
    }

    pub fn discover_f01c_second_completion(source: &[u8]) -> Result<Self, String> {
        Self::prepare(source, Probe::F01cSecondCompletion)
    }

    pub fn discover_fff7801c_second_completion(source: &[u8]) -> Result<Self, String> {
        Self::prepare(source, Probe::Fff7801cSecondCompletion)
    }

    pub fn discover_fff50000_ready(source: &[u8]) -> Result<Self, String> {
        Self::prepare(source, Probe::Fff50000Ready)
    }

    pub fn discover_fff50000_second_ready(source: &[u8]) -> Result<Self, String> {
        Self::prepare(source, Probe::Fff50000SecondReady)
    }

    pub fn discover_fff74000_ready(source: &[u8]) -> Result<Self, String> {
        Self::prepare(source, Probe::Fff74000Ready)
    }

    pub fn discover_linked_dma_input(source: &[u8]) -> Result<Self, String> {
        Self::prepare(source, Probe::LinkedDmaInput)
    }

    /// Linked DMA input with every initial GPIO/ADC level replaced by the
    /// measured real-camera baseline instead of the catalog/synthetic values.
    pub fn discover_linked_dma_input_measured(source: &[u8]) -> Result<Self, String> {
        Self::prepare_inputs(source, Probe::LinkedDmaInput, true, false)
    }

    /// Measured linked DMA input plus the rear touch panel on F_I2C unit 2.
    /// Opt-in so the default device graph, and checkpoints saved from it, stay
    /// unchanged.
    pub fn discover_linked_dma_input_touch(source: &[u8]) -> Result<Self, String> {
        Self::prepare_inputs(source, Probe::LinkedDmaInput, true, true)
    }

    fn prepare(source: &[u8], probe: Probe) -> Result<Self, String> {
        Self::prepare_inputs(source, probe, false, false)
    }

    fn prepare_inputs(
        source: &[u8],
        probe: Probe,
        measured: bool,
        touch: bool,
    ) -> Result<Self, String> {
        let spi2_second_header = probe >= Probe::Spi2SecondHeader;
        let spi2_owner_ack = probe >= Probe::Spi2OwnerAck;
        let discovery = probe >= Probe::Discovery;
        let power_status_override = (probe >= Probe::Ready).then_some(0x8000);
        let power_command_model = probe >= Probe::Commands;
        let empty_retained = probe >= Probe::EmptyRetained;
        let erased_flash = probe >= Probe::ErasedFlash;
        let flash_training_fixture = probe >= Probe::Training;
        let inactive_countdown = probe >= Probe::InactiveCountdown;
        let startup_gpio0_low = probe >= Probe::StartupInputLow;
        let latched_startup_cause = probe >= Probe::LatchedStartupCause;
        let invalid_clock = probe == Probe::InvalidClock;
        let virtual_clock = probe >= Probe::VirtualClock;
        let passive_discovery = probe >= Probe::VirtualClock;
        let calibration_fixture = probe >= Probe::IdealCalibration;
        let audio_reset = probe >= Probe::AudioReset;
        let power14_clear = probe >= Probe::Power14Clear;
        let mut recipe = Recipe::bundled("xt4-212")
            .map_err(|e| e.to_string())?
            .ok_or("Missing X-T4 recipe")?;
        xt4::validate(source)?;
        // The recipe API requires an input digest; container checks above replace stock identity.
        // shortcut: code/layout guards remain, replace them only with the corresponding hardware models.
        recipe.source_sha256 = fujimod_firmware::sha256(source);
        // This uses the upstream preloaded cold-loader edits, not a ROM reset.
        let plan = recipe.cold_plan(source).map_err(|e| e.to_string())?;
        let inverted = fujimod_firmware::invert(source);
        let ram = image::transform(&inverted, &plan).map_err(|e| e.to_string())?;
        if probe >= Probe::OrderedStartup {
            let expected: Vec<_> = [
                0xe30b306cu32,
                0xe3403000,
                0xe300222b,
                0xe5832000,
                0xe5832004,
                0xe5832008,
                0xe583200c,
                0xeb00031d,
            ]
            .into_iter()
            .flat_map(u32::to_le_bytes)
            .collect();
            if ram.get(0x4b0..0x4b0 + expected.len()) != Some(expected.as_slice()) {
                return Err(
                    "Loader barrier initialization differs from the verified X-T4 sequence".into(),
                );
            }
        }
        let profile = if erased_flash {
            ERASED_FLASH_PROFILE
        } else {
            BOARD_PROFILE
        };
        let mut definition: Definition =
            serde_json::from_str(profile).map_err(|e| e.to_string())?;
        let power3_source_sha256 = if probe >= Probe::Power3Raw768 {
            let mut original = Vec::new();
            for (start, end) in [
                (0x11d604c, 0x11d6070),
                (0x11d6134, 0x11d614c),
                (0x11d3aa0, 0x11d3bc4),
                (0x11d5c48, 0x11d5cb4),
                (0x11d62b8, 0x11d62ec),
                (0x11d6ef8, 0x11d6fa4),
                (0x1263bf4, 0x1263c1c),
                (0x172bad2, 0x172bad8),
            ] {
                original.extend_from_slice(
                    ram.get(start..end)
                        .ok_or("Missing raw read3 consumer source")?,
                );
            }
            let digest = fujimod_firmware::sha256(&original);
            if digest != "a4ca28e30407396154be867c8f71462997fab220a7845c9af7f46e153f271594" {
                return Err("Raw read3 consumer source differs".into());
            }
            let mut buttons = Vec::new();
            for (start, end) in [
                (0x172c2d4, 0x172c2f4),
                (0x156c89e, 0x156c8a4),
                (0x172c274, 0x172c294),
                (0x156c8a4, 0x156c8aa),
                (0x172c2f4, 0x172c314),
                (0x156c886, 0x156c88c),
                (0x172c314, 0x172c334),
                (0x156c88c, 0x156c892),
                (0x120369c, 0x12036bc),
                (0x11a2a24, 0x11a2a54),
                (0x11a3024, 0x11a3060),
                (0x11a2d10, 0x11a2d44),
                (0x11a2d50, 0x11a2d68),
                (0x11a2db4, 0x11a2dc8),
                (0x172c294, 0x172c2b4),
                (0x156c8aa, 0x156c8b0),
                (0x172c2b4, 0x172c2d4),
                (0x156c8b0, 0x156c8b6),
                (0x1202c28, 0x1202ca0),
                (0x1202dcc, 0x1202de4),
                (0x1204450, 0x1204484),
            ] {
                buttons
                    .extend_from_slice(ram.get(start..end).ok_or("Missing virtual button source")?);
            }
            if fujimod_firmware::sha256(&buttons) != VIRTUAL_BUTTON_SOURCE_SHA256 {
                return Err("Virtual button scanner source differs".into());
            }
            let mut client = Vec::new();
            for (start, end) in [
                (0x11fba28, 0x11fbaf8),
                (0x11fae74, 0x11fae90),
                (0x11faee8, 0x11faef4),
            ] {
                client.extend_from_slice(ram.get(start..end).ok_or("Missing SPI2 client source")?);
            }
            if fujimod_firmware::sha256(&client) != SPI2_CLIENT_SOURCE_SHA256 {
                return Err("SPI2 client observation source differs".into());
            }
            let inputs = &mut definition
                .checksum_power_spi
                .as_mut()
                .ok_or("Missing checksum peer")?
                .5;
            if inputs.get(&3) != Some(&512) {
                return Err("Raw read3 input differs from the catalog baseline".into());
            }
            inputs.insert(3, POWER3_RAW_INPUT);
            Some(digest)
        } else {
            None
        };
        let gpio9_source_sha256 = if probe >= Probe::Gpio9High {
            let mut cache_source = Vec::new();
            for (start, end) in [(0x11d6504, 0x11d656c), (0x11d68a0, 0x11d68e4)] {
                cache_source.extend_from_slice(
                    ram.get(start..end)
                        .ok_or("Missing battery cache observation source")?,
                );
            }
            if fujimod_firmware::sha256(&cache_source)
                != "5b15aa7b9753e9c899146420a65aff5aeaae7fb760ac0a47db6d4cd74747d38a"
            {
                return Err("Battery cache observation source differs".into());
            }
            let mut battery_source = Vec::new();
            for (start, end) in [
                (0x12944ac, 0x12944bc),
                (0x11df12c, 0x11df164),
                (0x11df270, 0x11df278),
                (0x11de6dc, 0x11de6ec),
                (0x11d4dd8, 0x11d4df8),
            ] {
                battery_source.extend_from_slice(
                    ram.get(start..end)
                        .ok_or("Missing battery status observation source")?,
                );
            }
            if fujimod_firmware::sha256(&battery_source)
                != "449c9577085f1599a5d8dc74e857d121c45e1036fbecf72469ca496030f93231"
            {
                return Err("Battery status observation source differs".into());
            }
            let mut copy_source = Vec::new();
            for (start, end) in [(0x1281848, 0x1281850), (0x12818c8, 0x12818f0)] {
                copy_source.extend_from_slice(
                    ram.get(start..end)
                        .ok_or("Missing A00B copy observation source")?,
                );
            }
            if fujimod_firmware::sha256(&copy_source)
                != "dee947ecd85ab5524571f129c64e144d214b363f410a4f8f7a1074d0939edd49"
            {
                return Err("A00B copy observation source differs".into());
            }
            let mut class_source = Vec::new();
            for (start, end) in [
                (0x1fa4cb0, 0x1fa4ce8),
                (0x1e5e3c0, 0x1e5e3c4),
                (0x1e5e454, 0x1e5e458),
                (0x1e5e4ec, 0x1e5e4f0),
                (0x1e5e584, 0x1e5e588),
                (0x1e5e610, 0x1e5e614),
                (0x1131c04, 0x1131c90),
            ] {
                class_source.extend_from_slice(
                    ram.get(start..end)
                        .ok_or("Missing A00B class observation source")?,
                );
            }
            if fujimod_firmware::sha256(&class_source)
                != "056148570b5ef92d3fea028414beef85a9554ed50c79bd42cd9e54bb238f24ce"
            {
                return Err("A00B class observation source differs".into());
            }
            let mut event_source = Vec::new();
            for (start, end) in [
                (0x1e28108, 0x1e28114),
                (0x1e281e0, 0x1e28200),
                (0x1e28480, 0x1e2849c),
                (0x1e27da0, 0x1e27e04),
            ] {
                event_source.extend_from_slice(
                    ram.get(start..end)
                        .ok_or("Missing poweroff event observation source")?,
                );
            }
            if fujimod_firmware::sha256(&event_source)
                != "b14428f3cd2b8d6f53ecdff097b0336ce56c2724906f11be1a7cbeac7eb36a09"
            {
                return Err("Poweroff event observation source differs".into());
            }
            let mut poweroff_source = Vec::new();
            for (start, end) in [
                (0x2016790, 0x2016854),
                (0x2016854, 0x20168dc),
                (0x2016e94, 0x2016ea4),
                (0x2016980, 0x20169d0),
                (0x201702c, 0x2017038),
                (0x2017038, 0x2017040),
            ] {
                poweroff_source.extend_from_slice(
                    ram.get(start..end)
                        .ok_or("Missing poweroff observation source")?,
                );
            }
            if fujimod_firmware::sha256(&poweroff_source)
                != "b27008406c6dae359a19cdf576ac2ef7ffad2147379a37837f7f18eeda68f51a"
            {
                return Err("Poweroff observation source differs".into());
            }
            let mut setup_source = Vec::new();
            for (start, end) in [
                (0x1131e40, 0x1131e58),
                (0x1131f38, 0x1131f58),
                (0x10210b8, 0x10210c8),
                (0x111efd4, 0x111f000),
                (0x115f5e0, 0x115f5f8),
                (0x110c82c, 0x110c848),
                (0x110d77c, 0x110d780),
                (0x111ccac, 0x111ccd0),
            ] {
                setup_source.extend_from_slice(
                    ram.get(start..end)
                        .ok_or("Missing setup observation source")?,
                );
            }
            if fujimod_firmware::sha256(&setup_source)
                != "ee5bde21b93a38ae65d23de39a148fb1ffe25737ec3a57f64f48a240a15209cf"
            {
                return Err("Setup observation source differs".into());
            }
            let mut source = Vec::new();
            for (start, end) in [
                (0x11a2a24, 0x11a2a54),
                (0x11a3024, 0x11a3060),
                (0x11a2d10, 0x11a2d44),
                (0x11a2d50, 0x11a2d68),
                (0x11a2db4, 0x11a2dc8),
                (0x172c314, 0x172c334),
            ] {
                source.extend_from_slice(ram.get(start..end).ok_or("Missing GPIO9 input source")?);
            }
            let digest = fujimod_firmware::sha256(&source);
            if digest != "c3ac08a179715972e114ec9131ed49fde253dfb627cebd64988d7906f4994f84" {
                return Err("GPIO9 input selector/consumer source differs".into());
            }
            Some(digest)
        } else {
            None
        };
        let power2_bit8_source_sha256 = if probe >= Probe::Power2Bit8Echo {
            let mut input_source = Vec::new();
            for (start, end) in [
                (0x1203630, 0x12036bc),
                (0x1167e40, 0x1167e78),
                (0x1167c4c, 0x1167e40),
                (0x12b0a10, 0x12b0a80),
                (0x12b08e8, 0x12b093c),
                (0x1133a78, 0x1133ab4),
                (0x12b18ec, 0x12b191c),
                (0x1203990, 0x1203a18),
                (0x156c820, 0x156cca6),
                (0x172c314, 0x172c334),
            ] {
                input_source.extend_from_slice(
                    ram.get(start..end)
                        .ok_or("Missing input11 observation source")?,
                );
            }
            if fujimod_firmware::sha256(&input_source)
                != "81a414b29c6c26f9d24134693d982f49ad00267daf49fae68c4fc687a118c1f5"
            {
                return Err("Input11 observation source differs".into());
            }
            let mut property_source = Vec::new();
            for (start, end) in [
                (0x1124278, 0x11242f0),
                (0x1133a78, 0x1133ab4),
                (0x1133b54, 0x1133bd4),
                (0x1133c3c, 0x1133c94),
                (0x20187e8, 0x2018830),
                (0x2018a38, 0x2018a50),
                (0x2018af4, 0x2018b0c),
                (0x2018b48, 0x2018b60),
                (0x2018b7c, 0x2018b94),
                (0x2018e44, 0x2018e5c),
            ] {
                property_source.extend_from_slice(
                    ram.get(start..end)
                        .ok_or("Missing property9 producer source")?,
                );
            }
            if fujimod_firmware::sha256(&property_source)
                != "22d1aa9d4e7d2bc71b12cfb0df69fc05ca3c1c00ac08d0800769c345e71b2f8a"
            {
                return Err("Property9 producer source differs".into());
            }
            let mut ui_source = Vec::new();
            for (start, end) in [
                (0x11255e8, 0x11258ec),
                (0x1131da4, 0x1131f88),
                (0x1141b04, 0x1141b38),
                (0x1141e38, 0x1141e3c),
                (0x1e703fc, 0x1e7040c),
                (0x1fb21dc, 0x1fb21ec),
            ] {
                ui_source.extend_from_slice(
                    ram.get(start..end)
                        .ok_or("Missing UI mode observation source")?,
                );
            }
            if fujimod_firmware::sha256(&ui_source)
                != "db3363ed4bedeea54a542038eb6333def488bdd107e786da02af354bd3db366b"
            {
                return Err("UI mode observation source differs".into());
            }
            let mut original = Vec::new();
            for (start, end) in [
                (0x11d3968, 0x11d39ac),
                (0x11d3bc4, 0x11d3c20),
                (0x11d4388, 0x11d4480),
                (0x11d4684, 0x11d4770),
                (0x11d4818, 0x11d482c),
                (0x11d4860, 0x11d48bc),
                (0x11d52fc, 0x11d54a0),
                (0x11d5aa8, 0x11d5b34),
                (0x11d64f0, 0x11d6508),
                (0x12638d0, 0x1263918),
                (0x1263954, 0x1263974),
                (0x172bacc, 0x172bad2),
            ] {
                original.extend_from_slice(ram.get(start..end).ok_or("Missing word2 bit8 source")?);
            }
            let digest = fujimod_firmware::sha256(&original);
            if digest != "033001d74a4f9d2012678871e1ea6d82f986a638909bfe81fa9650ae789e15e2"
                || definition
                    .checksum_power_spi
                    .as_ref()
                    .and_then(|peer| peer.5.get(&2))
                    != Some(&0)
            {
                return Err("Word2 bit8 writer/counter/read policy source differs".into());
            }
            Some(digest)
        } else {
            None
        };
        let class3_source_sha256 = if probe >= Probe::Class3Inputs {
            let mut original = Vec::new();
            for (start, end) in [
                (0x11d55c0, 0x11d58b4),
                (0x11d4024, 0x11d4324),
                (0x172bbda, 0x172bbf2),
                (0x172bc70, 0x172bc76),
                (0x172bd5a, 0x172bd72),
                (0x172bd84, 0x172bd9c),
                (0x12638d0, 0x1263918),
                (0x1263954, 0x1263974),
                (0x1263be4, 0x1263bf4),
                (0x1263c64, 0x1263c78),
                (0x11d621c, 0x11d6240),
                (0x11d656c, 0x11d659c),
                (0x11d6ac4, 0x11d6b50),
                (0x11d6b94, 0x11d6bdc),
                (0x11d539c, 0x11d54a0),
            ] {
                original.extend_from_slice(
                    ram.get(start..end)
                        .ok_or("Missing class3 consumer source")?,
                );
            }
            let digest = fujimod_firmware::sha256(&original);
            if digest != "5e2e43ac7cb1d93f3e59e6f7c2a42137a38c529a23b70a68883061378c82cbfa" {
                return Err("Class3 refresh/cache/numeric source differs".into());
            }
            let inputs = &mut definition
                .checksum_power_spi
                .as_mut()
                .ok_or("Missing checksum peer")?
                .5;
            for (register, value, _, _, _) in CLASS3_READS {
                if inputs.insert(register, value).is_some() {
                    return Err(format!("Class3 experiment would replace input{register}"));
                }
            }
            Some(digest)
        } else {
            None
        };
        let power2_echo_source_sha256 = if probe >= Probe::Power2Echo {
            let sequence = ram
                .get(0x11d61ac..0x11d61f4)
                .ok_or("Missing word2 initialization sequence")?;
            let policy = ram
                .get(0x172bacc..0x172bad2)
                .ok_or("Missing word2 read policy")?;
            let digest = fujimod_firmware::sha256(&[sequence, policy].concat());
            if digest != "2bfe86f4f133d58df84583e26f5bec480c9215393b8dce52ee81b0bfc951e0c0"
                || definition
                    .checksum_power_spi
                    .as_ref()
                    .and_then(|peer| peer.5.get(&2))
                    != Some(&0)
            {
                return Err("Word2 initialization source or independent read input differs".into());
            }
            Some(digest)
        } else {
            None
        };
        let power64_source_sha256 = if probe >= Probe::Power64Inputs {
            let refresh = ram
                .get(0x11d5544..0x11d55c0)
                .ok_or("Missing power refresh consumer")?;
            let policies = ram
                .get(0x172bc40..0x172bc4c)
                .ok_or("Missing power64/65 cache policies")?;
            let digest = fujimod_firmware::sha256(&[refresh, policies].concat());
            if digest != "e844a7f162021a36fcff1b610184b277340e6befc97826e1d85fad653499172f" {
                return Err(
                    "Power64/65 consumers or cache policies differ from verified source".into(),
                );
            }
            let inputs = &mut definition
                .checksum_power_spi
                .as_mut()
                .ok_or("Missing checksum peer")?
                .5;
            for register in POWER64_READ_REGISTERS {
                if inputs.insert(register, 0).is_some() {
                    return Err(format!("Power64 experiment would replace input{register}"));
                }
            }
            Some(digest)
        } else {
            None
        };
        let control_request_source_sha256 = if probe >= Probe::FffaeRequest {
            let block = ram
                .get(0x11bfb18..0x11bfb58)
                .ok_or("Missing shared command helper")?;
            let digest = fujimod_firmware::sha256(block);
            if digest != "779a3a7f5235fcdc7d59b0f9d47439c3ec860cdbbbf6f1f9e757916a7804ed2c"
                || ram.get(0x158767c..0x1587680) != Some(0xfffae000u32.to_le_bytes().as_slice())
                || definition
                    .control_requests
                    .iter()
                    .any(|&(address, _)| address == 0xfffae080)
            {
                return Err("FFFAE shared command contract differs or already has a model".into());
            }
            // shortcut: selected virtual latency, replace with a qualified capture for physical timing.
            definition
                .control_requests
                .push((0xfffae080, CALIBRATION_TICKS));
            Some(digest)
        } else {
            None
        };
        let mut calibration_source_sha256 = None;
        let mut calibration_mode5_sha256 = None;
        let mut calibration_mode2_sha256 = None;
        let mut calibration_reprogram_sha256 = None;
        let mut calibration_post_control_sha256 = None;
        let mut calibration_post_mode2_sha256 = None;
        let mut calibration_selector1_sha256 = None;
        let mut calibration_removal_sha256 = None;
        let mut audio_reset_source_sha256 = None;
        let rtc_control_source_sha256 = if probe >= Probe::RtcControlEcho {
            let block = ram
                .get(0x11d5b34..0x11d5b74)
                .ok_or("Missing RTC control bit2 setter")?;
            let digest = fujimod_firmware::sha256(block);
            if digest != "4d674eec2ab5f01fb29c58d9fc2d541acbf83f12c8af7501b4bd9ad8618dccbb" {
                return Err("RTC control bit2 setter differs from verified source".into());
            }
            Some(digest)
        } else {
            None
        };
        let power_init_source_sha256 = if probe >= Probe::PowerInitReads {
            let block = ram
                .get(0x11d5e14..0x11d5ed4)
                .ok_or("Missing power initialization read consumers")?;
            let digest = fujimod_firmware::sha256(block);
            if digest != "42b8db9f39200247216c9f4e7dfecf7f844af46cbcb74f18bf9d23029b32d1fe" {
                return Err(
                    "Power initialization read consumers differ from verified source".into(),
                );
            }
            let inputs = &mut definition
                .checksum_power_spi
                .as_mut()
                .ok_or("Missing checksum peer")?
                .5;
            for register in POWER_INIT_READ_REGISTERS {
                if inputs.insert(register, 0).is_some() {
                    return Err(format!(
                        "Power initialization experiment would replace input{register}"
                    ));
                }
            }
            Some(digest)
        } else {
            None
        };
        let rtc_staging_source_sha256 = if probe >= Probe::RtcStagingReadback {
            let block = ram
                .get(0x11d5d54..0x11d5d94)
                .ok_or("Missing RTC staging read consumers")?;
            let digest = fujimod_firmware::sha256(block);
            if digest != "843cb2238a66124d13777b86c723e4ef9f48727f20f467a6c445984104e83946" {
                return Err("RTC staging read consumers differ from the verified source".into());
            }
            Some(digest)
        } else {
            None
        };
        let power17_source_sha256 = if probe >= Probe::Power17Clear {
            let block = ram
                .get(0x11d5d34..0x11d5d54)
                .ok_or("Missing power17 input consumer")?;
            let digest = fujimod_firmware::sha256(block);
            if digest != "bf3f9525acfc87d518bbdc3b469c97228d674ea695b7dc750633e5f05f7afc00" {
                return Err("Power17 input consumer differs from the verified source".into());
            }
            // shortcut: independent synthetic read input; qualify physical status before hardware fidelity.
            let inputs = &mut definition
                .checksum_power_spi
                .as_mut()
                .ok_or("Missing checksum peer")?
                .5;
            if inputs.insert(17, 0).is_some() {
                return Err("Power17 experiment would replace an existing input".into());
            }
            Some(digest)
        } else {
            None
        };
        let power67_source_sha256 = if probe >= Probe::Power67Clear {
            let block = ram
                .get(0x11d5cdc..0x11d5d34)
                .ok_or("Missing power67 input consumer")?;
            let digest = fujimod_firmware::sha256(block);
            if digest != "8713bcb32349290d145742ae1f14e4088cb14f16d5f1a8cd80c744ff5308c4a6" {
                return Err("Power67 input consumer differs from the verified source".into());
            }
            // shortcut: explicitly synthetic read-only input; qualify physical values/transitions before hardware fidelity.
            let inputs = &mut definition
                .checksum_power_spi
                .as_mut()
                .ok_or("Missing checksum peer")?
                .5;
            if inputs.insert(67, 0).is_some() {
                return Err("Power67 experiment would replace an existing input".into());
            }
            Some(digest)
        } else {
            None
        };
        let power14_source_sha256 = if power14_clear {
            let block = ram
                .get(0x12031c8..0x120327c)
                .ok_or("Missing power14 input consumer")?;
            let digest = fujimod_firmware::sha256(block);
            if digest != "b52c0eab16f1fa85f1b7b6ad1d391035b99df2c1d3e73e4de06208e4b8d26ef6" {
                return Err("Power14 input consumer differs from the verified source".into());
            }
            let inputs = &mut definition
                .checksum_power_spi
                .as_mut()
                .ok_or("Missing checksum peer")?
                .5;
            if inputs.insert(14, 0).is_some() {
                return Err("Power14 experiment would replace an existing input".into());
            }
            Some(digest)
        } else {
            None
        };
        if audio_reset {
            let block = ram
                .get(0x1256d70..0x1256db4)
                .ok_or("Missing audio consumer block")?;
            let digest = fujimod_firmware::sha256(block);
            if digest != "b981334e14a28d1b2d4034d00ea94e09434bb1376c6f8049607435c34fde6e57" {
                return Err("Audio consumer differs from the verified source".into());
            }
            audio_reset_source_sha256 = Some(digest);
            definition.audio_reset_fixture_ticks = Some(CALIBRATION_TICKS);
        }
        if calibration_fixture {
            let count = ram
                .get(0x1589420..0x1589424)
                .ok_or("Missing calibration table count")?;
            if u32::from_le_bytes(count.try_into().unwrap()) != 686 {
                return Err("Calibration table differs from the verified source".into());
            }
            let mut source_bytes = Vec::new();
            let mut table = |address: usize, count: usize| -> Result<Vec<(u32, u32)>, String> {
                let bytes = ram
                    .get(address..address + count * 8)
                    .ok_or("Calibration script outside RAM image")?;
                source_bytes.extend_from_slice(bytes);
                Ok(bytes
                    .as_chunks::<8>()
                    .0
                    .iter()
                    .map(|pair| {
                        (
                            u32::from_le_bytes(pair[..4].try_into().unwrap()),
                            u32::from_le_bytes(pair[4..].try_into().unwrap()),
                        )
                    })
                    .collect())
            };
            let initial_script = table(0x1587eb0, 686)?;
            let preparation = table(0x158cba0, 7)?;
            let profile = table(0x158cb14, 17)?;
            let source_sha256 = fujimod_firmware::sha256(&source_bytes);
            let mut mode5_bytes = Vec::with_capacity(2272);
            let mut mode5_scripts = std::array::from_fn(|_| Vec::new());
            for (unit, script) in mode5_scripts.iter_mut().enumerate() {
                let at = 0x158b890 + unit * 8;
                let index = ram.get(at..at + 8).ok_or("Missing mode5 table index")?;
                let count = u32::from_le_bytes(index[..4].try_into().unwrap());
                let pointer = u32::from_le_bytes(index[4..].try_into().unwrap()) as usize;
                if count != 71 || pointer != 0x158b8b0 + unit * 568 {
                    return Err("Mode5 table differs from the verified source".into());
                }
                let bytes = ram
                    .get(pointer..pointer + 568)
                    .ok_or("Mode5 script outside RAM image")?;
                mode5_bytes.extend_from_slice(bytes);
                *script = bytes
                    .as_chunks::<8>()
                    .0
                    .iter()
                    .map(|pair| {
                        (
                            u32::from_le_bytes(pair[..4].try_into().unwrap()),
                            u32::from_le_bytes(pair[4..].try_into().unwrap()),
                        )
                    })
                    .collect();
            }
            let mode5_sha256 = fujimod_firmware::sha256(&mode5_bytes);
            let removal_bytes = ram
                .get(0x158cc24..0x158cc3c)
                .ok_or("Missing removal script")?;
            let removal_script = removal_bytes
                .as_chunks::<8>()
                .0
                .iter()
                .map(|pair| {
                    (
                        u32::from_le_bytes(pair[..4].try_into().unwrap()),
                        u32::from_le_bytes(pair[4..].try_into().unwrap()),
                    )
                })
                .collect();
            let poll_bytes = ram
                .get(0x158cc10..0x158cc24)
                .ok_or("Missing removal poll contract")?;
            let removal_poll = std::array::from_fn(|i| {
                u32::from_le_bytes(poll_bytes[i * 4..i * 4 + 4].try_into().unwrap())
            });
            let removal_sha256 = fujimod_firmware::sha256(&[removal_bytes, poll_bytes].concat());
            calibration_source_sha256 = Some(source_sha256.clone());
            calibration_mode5_sha256 = Some(mode5_sha256.clone());
            calibration_removal_sha256 = Some(removal_sha256.clone());
            if definition
                .control_requests
                .iter()
                .any(|&(address, _)| address == 0xff70c080)
            {
                return Err("Profile1 request already has a board model".into());
            }
            definition
                .control_requests
                .push((0xff70c080, CALIBRATION_TICKS));
            let mode2 = if probe >= Probe::CalibrationMode2 {
                let config = mode2::prepare(&ram)?;
                calibration_mode2_sha256 = Some(config.source_sha256.clone());
                Some(config)
            } else {
                None
            };
            let reprogram = if probe >= Probe::CalibrationReprogram {
                let config = reprogram::prepare(&ram)?;
                calibration_reprogram_sha256 = Some(config.input_sha256.clone());
                Some(config)
            } else {
                None
            };
            let post_control_source_sha256 = if probe >= Probe::CalibrationPostControl {
                let hash = reprogram::prepare_post_control(&ram)?;
                calibration_post_control_sha256 = Some(hash.clone());
                Some(hash)
            } else {
                None
            };
            let post_mode2_source_sha256 = if probe >= Probe::CalibrationPostMode2 {
                let hash = mode2::prepare_post_control(&ram)?;
                calibration_post_mode2_sha256 = Some(hash.clone());
                Some(hash)
            } else {
                None
            };
            let selector1_source_sha256 = if probe >= Probe::CalibrationSelector1 {
                let hash = mode2::prepare_selector1(&ram)?;
                calibration_selector1_sha256 = Some(hash.clone());
                Some(hash)
            } else {
                None
            };
            definition.banked_calibration_fixture = Some(BankedFixtureConfig {
                initial_script,
                preparation,
                profile,
                results: CALIBRATION_RESULTS,
                latency_ticks: CALIBRATION_TICKS,
                source_sha256,
                mode5_scripts,
                mode5_sha256,
                mode2,
                removal_script,
                removal_poll,
                removal_sha256,
                unit0_preinitialized: true,
                reprogram,
                post_control_source_sha256,
                post_mode2_source_sha256,
                selector1_source_sha256,
            });
        }
        let ff702280_source_sha256 = if probe >= Probe::Ff702280Request {
            let hash = mode2::prepare_ff702280_request(&ram)?;
            if definition
                .control_requests
                .iter()
                .any(|&(address, _)| address == 0xff702280)
            {
                return Err("FF702280 request already has a board model".into());
            }
            definition
                .control_requests
                .push((0xff702280, CALIBRATION_TICKS));
            Some(hash)
        } else {
            None
        };
        let rec_input_trace = if matches!(
            probe,
            Probe::FaReady
                | Probe::FaCompletion
                | Probe::Fff7801cCompletion
                | Probe::F01cSecondCompletion
                | Probe::Fff7801cSecondCompletion
                | Probe::Fff50000Ready
                | Probe::Fff50000SecondReady
                | Probe::Fff74000Ready
                | Probe::LinkedDmaInput
        ) {
            rec_input::qualify(&ram)?;
            rec_display::qualify(&ram)?;
            Some(Rc::new(RefCell::new(rec_input::Trace::default())))
        } else {
            None
        };
        let fa_ready_source_sha256 = if probe == Probe::FaReady {
            let hash = fa::prepare(&ram)?;
            if definition.fa_ready_fixture.is_some() {
                return Err("FA ready fixture already has a board model".into());
            }
            definition.fa_ready_fixture = Some((CALIBRATION_TICKS, hash.clone()));
            Some(hash)
        } else {
            None
        };
        let fa_completion_source_sha256 = if probe >= Probe::FaCompletion {
            let hash = fa_completion::prepare(&ram)?;
            if definition.fa_ready_fixture.is_some() || definition.fa_completion_fixture.is_some() {
                return Err("FA fixture already has a board model".into());
            }
            definition.fa_completion_fixture =
                Some((CALIBRATION_TICKS, CALIBRATION_TICKS, hash.clone()));
            Some(hash)
        } else {
            None
        };
        let fff7801c_completion_source_sha256 = if probe >= Probe::Fff7801cCompletion {
            let hash = fff7801c_completion::prepare(&ram)?;
            if definition.fff7801c_completion_fixture_ticks.is_some() {
                return Err("FFF7801C completion fixture already has a board model".into());
            }
            definition.fff7801c_completion_fixture_ticks = Some(CALIBRATION_TICKS);
            Some(hash)
        } else {
            None
        };
        let f01c_second_completion_source_sha256 = if matches!(
            probe,
            Probe::F01cSecondCompletion
                | Probe::Fff7801cSecondCompletion
                | Probe::Fff50000Ready
                | Probe::Fff50000SecondReady
                | Probe::Fff74000Ready
                | Probe::LinkedDmaInput
        ) {
            let hash = f01c_second_completion::prepare(&ram)?;
            if definition.audio_reset_second_command
                || definition.audio_reset_fixture_ticks != Some(CALIBRATION_TICKS)
            {
                return Err(
                    "Second F01C command requires the unchanged first-command fixture".into(),
                );
            }
            definition.audio_reset_second_command = true;
            Some(hash)
        } else {
            None
        };
        let fff7801c_second_completion = matches!(
            probe,
            Probe::Fff7801cSecondCompletion
                | Probe::Fff50000Ready
                | Probe::Fff50000SecondReady
                | Probe::Fff74000Ready
                | Probe::LinkedDmaInput
        );
        if fff7801c_second_completion {
            if f01c_second_completion_source_sha256.is_none()
                || definition.fff7801c_completion_fixture_ticks != Some(CALIBRATION_TICKS)
                || definition.fff7801c_second_command
            {
                return Err("Second FFF7801C command requires both qualified consumers".into());
            }
            definition.fff7801c_second_command = true;
        }
        let fff50000_ready_source_sha256 = if matches!(
            probe,
            Probe::Fff50000Ready
                | Probe::Fff50000SecondReady
                | Probe::Fff74000Ready
                | Probe::LinkedDmaInput
        ) {
            let hash = fff50000_ready::prepare(&ram)?;
            if definition.fff50000_ready_fixture_ticks.is_some() || !fff7801c_second_completion {
                return Err("FFF50000 ready requires the qualified inherited scenario".into());
            }
            definition.fff50000_ready_fixture_ticks = Some(CALIBRATION_TICKS);
            Some(hash)
        } else {
            None
        };
        let fff50000_second_ready = matches!(
            probe,
            Probe::Fff50000SecondReady | Probe::Fff74000Ready | Probe::LinkedDmaInput
        );
        if fff50000_second_ready {
            if definition.fff50000_second_request
                || definition.fff50000_ready_fixture_ticks != Some(CALIBRATION_TICKS)
            {
                return Err(
                    "Second FFF50000 request requires the qualified first-request fixture".into(),
                );
            }
            definition.fff50000_second_request = true;
        }
        let fff74000_ready_source_sha256 =
            if matches!(probe, Probe::Fff74000Ready | Probe::LinkedDmaInput) {
                let hash = fff74000_ready::prepare(&ram)?;
                if definition.fff74000_ready_fixture_ticks.is_some() || !fff50000_second_ready {
                    return Err("FFF74000 ready requires the qualified inherited scenario".into());
                }
                definition.fff74000_ready_fixture_ticks = Some(CALIBRATION_TICKS);
                Some(hash)
            } else {
                None
            };
        let linked_dma_source_sha256 = if probe == Probe::LinkedDmaInput {
            definition.instructions_per_tick = LINKED_DMA_INSTRUCTIONS_PER_TICK;
            let hash = linked_dma::prepare(&ram)?;
            if fff74000_ready_source_sha256.is_none()
                || definition.linked_dma_input_fixture.is_some()
            {
                return Err("Linked DMA input requires the qualified FFF74000 scenario".into());
            }
            // The RX model supplies a bounded startup FIFO, then the original steady clock.
            definition.linked_dma_input_fixture = Some(((AUDIO_FRAME_TICKS, 2), None));
            // Firmware125f70c: 720 stereo samples use5760 bytes, two4-byte words/sample.
            definition.linked_dma_output_fixture_ticks = Some((AUDIO_FRAME_TICKS, 2));
            Some(hash)
        } else {
            None
        };
        if probe >= Probe::LinkedDmaInput {
            definition.audio_handshake_ticks = Some(CALIBRATION_TICKS);
            definition.display_clock = Some((0xfffab000, 2700));
            definition.sd_hosts = vec![
                (0xfffc4000, Some(230), 0x2000000),
                (0xfffc8000, Some(237), 0x2000000),
            ];
            // Unit A of the FF70x080 request series: UILIB 011BFB40 and the battery task
            // post repeated requests here at REC.
            if definition
                .control_requests
                .iter()
                .any(|&(address, _)| address == 0xff70a080)
            {
                return Err("FF70A080 request already has a board model".into());
            }
            definition
                .control_requests
                .push((0xff70a080, CALIBRATION_TICKS));
        }
        definition.discovery = discovery;
        definition.discovery_passive = passive_discovery;
        definition.storage_training_handshake = flash_training_fixture;
        if startup_gpio0_low {
            let inputs = definition
                .gpio
                .as_mut()
                .ok_or("Missing startup GPIO inputs")?;
            if inputs[0] != 3 {
                return Err(
                    "Startup GPIO0 input differs from the verified catalog baseline".into(),
                );
            }
            inputs[0] = 2;
        }
        if gpio9_source_sha256.is_some() {
            let inputs = definition.gpio.as_mut().ok_or("Missing GPIO9 inputs")?;
            if inputs[9] != 0 {
                return Err("GPIO9 input differs from the catalog baseline".into());
            }
            inputs[9] = 1 << 5;
        }
        if measured {
            let inputs = measured_inputs()?;
            definition.gpio = Some(inputs.gpio);
            definition.adc.as_mut().ok_or("Missing ADC inputs")?.0 = inputs.adc;
        }
        if erased_flash {
            definition.storage_identity_bytes = Some(1);
        }
        if probe >= Probe::SupportedNor {
            definition.storage_jedec = Some(0x2c);
        }
        let flash_jedec = definition.storage_jedec;
        let flash_identity_bytes = definition.storage_identity_bytes;
        definition.checksum_power_initialization_commands = power_command_model;
        definition.checksum_power_inactive_countdown = inactive_countdown;
        definition.checksum_power_latched_startup_cause = latched_startup_cause;
        definition.checksum_power_word2_one_echo = probe >= Probe::Power2Echo;
        definition.checksum_power_word2_bit8_echo = probe >= Probe::Power2Bit8Echo;
        definition.checksum_power_word2_one_count_only = power3_source_sha256.is_some();
        if invalid_clock {
            let inputs = &mut definition
                .checksum_power_spi
                .as_mut()
                .ok_or("Missing checksum peer")?
                .5;
            inputs.insert(39, 0);
            inputs.remove(&35);
        }
        if virtual_clock {
            let peer = definition
                .checksum_power_spi
                .as_mut()
                .ok_or("Missing checksum peer")?;
            peer.5.remove(&35);
            peer.7 = Some(
                serde_json::from_value(json!({
                    "control":39, "read_low":35, "read_high":36,
                    "stage_low":29, "stage_high":30, "commit_value":0x202,
                    "staging_readback":rtc_staging_source_sha256.is_some(),
                    "retained_control_echo":rtc_control_source_sha256.is_some(),
                    "valid_mask":2, "retained_valid_mask":1,
                    "strobe":{"mask":0x200,"set_through_ticks":0,"clear_from_ticks":1},
                    "initial_control":0, "initial_seconds":0, "ticks_per_second":54_000_000
                }))
                .map_err(|e| e.to_string())?,
            );
        }
        if latched_startup_cause {
            definition
                .checksum_power_spi
                .as_mut()
                .ok_or("Missing checksum peer")?
                .5
                .insert(16, 0x10);
        }
        if inactive_countdown {
            definition
                .checksum_power_spi
                .as_mut()
                .ok_or("Missing checksum peer")?
                .5
                .insert(18, 0);
        }
        if probe >= Probe::OrderedStartup {
            definition.labels.extend([0x1291730, 0x1291818]);
        }
        if empty_retained {
            let peer = definition
                .checksum_power_spi
                .as_mut()
                .ok_or("Missing checksum power peer")?;
            peer.8 = vec![31, 32, 33, 34];
            for register in &peer.8 {
                peer.5.insert(*register, 0);
            }
        }
        if let Some(status) = power_status_override {
            definition
                .checksum_power_spi
                .as_mut()
                .ok_or("Missing checksum power peer")?
                .5
                .insert(0, status);
        }
        for region in &plan.regions {
            if let Some(header) = ram
                .get(region.address..region.address.saturating_add(64))
                .and_then(fujimod_firmware::inner_header)
                && header.entry < ram.len() as u64
            {
                definition.labels.push(header.entry as u32);
            }
        }
        let mut dat_storage_inputs = Vec::new();
        let mut packed_resource_input = None;
        let mut catalog_nor_inputs = Vec::new();
        let storage_image = if flash_training_fixture {
            let pattern = ram
                .get(0xa080..0xa100)
                .ok_or("Missing loader training pattern")?;
            if fujimod_firmware::sha256(pattern)
                != "10fa7918107458a1b72e18ef91b9bfc27b6eb76e3b562aa6566c5561dcf4929a"
            {
                return Err("Loader training pattern differs from the verified fixture".into());
            }
            let mut nor = vec![0xff; 0x2a100];
            nor[0x2a080..].copy_from_slice(pattern);
            if probe >= Probe::DatStorage {
                let preload = recipe
                    .cold_start
                    .as_ref()
                    .and_then(|cold| cold.preload.as_ref())
                    .ok_or("Missing DAT read map")?;
                let header = fujimod_firmware::outer_header(source).map_err(|e| e.to_string())?;
                if header.kind != 6 || header.payload_offset != 0x27c || preload.ranges.len() != 5 {
                    return Err("DAT read map layout differs".into());
                }
                let mut spans = Vec::<(usize, usize)>::new();
                for mapping in &preload.ranges {
                    if !matches!(mapping.source.compression, image::Compression::Raw)
                        || mapping.source.initialized_prefix.is_some()
                        || mapping.source.size != mapping.source.decoded_size
                        || mapping.offset.checked_add(0x274) != Some(mapping.source.dat_offset)
                    {
                        return Err("DAT read map is not a verified raw flash range".into());
                    }
                    let bytes = fujimod_firmware::extent(
                        &inverted,
                        mapping.source.dat_offset as u64,
                        mapping.source.size as u64,
                    )
                    .map_err(|e| e.to_string())?;
                    let end = mapping
                        .offset
                        .checked_add(bytes.len())
                        .ok_or("DAT flash range overflow")?;
                    if mapping.offset < 0x2a100
                        || end > 0x8000000
                        || spans
                            .iter()
                            .any(|&(lo, hi)| mapping.offset < hi && lo < end)
                    {
                        return Err("Overlapping or out-of-range DAT flash input".into());
                    }
                    nor.resize(nor.len().max(end), 0xff);
                    nor[mapping.offset..end].copy_from_slice(bytes);
                    spans.push((mapping.offset, end));
                    dat_storage_inputs.push(json!({"name":mapping.name,"flashOffset":mapping.offset,
                        "datOffset":mapping.source.dat_offset,"bytes":bytes.len(),"sha256":fujimod_firmware::sha256(bytes)}));
                }
                if probe >= Probe::PackedResources {
                    let region = plan
                        .regions
                        .get(3)
                        .ok_or("Missing packed resources1 source")?;
                    let input = &region.source;
                    if region.name != "resources1"
                        || region.address != 0x1e0000
                        || input.dat_offset != 0x1a00274
                        || input.size != 0x340000
                        || !matches!(input.compression, image::Compression::Method4)
                        || input.decoded_size != 0x625d0c
                        || input.initialized_prefix.is_some()
                        || input.sha256
                            != "b1a6146fc89394e7c47a2bd5b0e92c07de085567da3aa8d6198bc1a32de9eec6"
                    {
                        return Err(
                            "Packed resources1 source differs from verified DAT input".into()
                        );
                    }
                    // Source::read returns decoded RAM; NOR must retain the original packed bytes.
                    let end = input
                        .dat_offset
                        .checked_add(input.size)
                        .ok_or("Packed DAT range overflow")?;
                    let bytes = inverted
                        .get(input.dat_offset..end)
                        .ok_or("Packed DAT range exceeds file")?;
                    let digest = fujimod_firmware::sha256(bytes);
                    if digest != "24d0b8bb906acca351aa3c1fcf480cc0938a09aa35aadba88238b1c49e45dffd"
                    {
                        return Err("Packed resources1 encoded bytes differ".into());
                    }
                    let offset = input.dat_offset - 0x274;
                    let end = offset
                        .checked_add(bytes.len())
                        .ok_or("Packed flash range overflow")?;
                    if offset < 0x2a100
                        || end > 0x8000000
                        || spans.iter().any(|&(lo, hi)| offset < hi && lo < end)
                    {
                        return Err("Overlapping or out-of-range packed flash input".into());
                    }
                    nor.resize(nor.len().max(end), 0xff);
                    nor[offset..end].copy_from_slice(bytes);
                    packed_resource_input = Some(json!({"name":region.name,"flashOffset":offset,
                        "datOffset":input.dat_offset,"bytes":bytes.len(),"encoding":"method4",
                        "sha256":digest,"decodedBytes":input.decoded_size,"decodedSha256":input.sha256}));
                }
            }
            if probe >= Probe::CatalogNor {
                for (index, name, offset, size, expected_hash) in [
                    (
                        0,
                        "loader+sync",
                        0x20000usize,
                        0x40000usize,
                        "324cc497a3eec9e0a5a738aa2211a0510d36a49a3d61ce16e5d60a104a3a0a4e",
                    ),
                    (
                        1,
                        "section table",
                        0x140000,
                        0x400,
                        "41ca21d9445075090441de329567b9fa459f9be4ccf54b7e51ad0637884d8d3e",
                    ),
                    (
                        2,
                        "kernel0",
                        0x60000,
                        0x20000,
                        "bd28cbebeece84e9afbe2757cd964380d56be6cfb5a410268ca34739d1332420",
                    ),
                    (
                        4,
                        "resources2",
                        0x1da0000,
                        0xc0000,
                        "ed1edf5c0afaaa5e76d93d3679684aa58d29a80f96a22403c040050e042e01f0",
                    ),
                    (
                        5,
                        "module3",
                        0x160000,
                        0x40000,
                        "e2e87ae6ccd62d56115b91c4be6a499e416f82c29f751bab2d8f68be5d754686",
                    ),
                    (
                        6,
                        "module4",
                        0x1e0000,
                        0x40000,
                        "f627f0340cff47bc5af738017053371e16cb5374b1e621c13959d650bb189696",
                    ),
                    (
                        7,
                        "section5",
                        0x260000,
                        0x360000,
                        "0f36255aa8aaa6e1fdff9bd414846cde23d6232863747f42a6aa306d5fdbbf8b",
                    ),
                    (
                        8,
                        "section6",
                        0x5c0000,
                        0x520000,
                        "29d828759f581f11e410e6a8bf769c8b2c08ef8830d771ef9510a0a8017a3e37",
                    ),
                    (
                        9,
                        "overlays",
                        0xae0000,
                        0x480000,
                        "396ed27e6769d3354211268102b6cb8832eda15bb838cf50ed6079a21d337308",
                    ),
                ] {
                    let region = plan
                        .regions
                        .get(index)
                        .ok_or("Missing catalog NOR region")?;
                    let input = &region.source;
                    if region.name != name
                        || input.dat_offset != offset + 0x274
                        || input.size > size
                        || input.initialized_prefix.is_some()
                    {
                        return Err("Catalog NOR region differs from verified source".into());
                    }
                    let end = offset
                        .checked_add(size)
                        .ok_or("Catalog NOR range overflow")?;
                    let dat_end = input
                        .dat_offset
                        .checked_add(size)
                        .ok_or("Catalog DAT range overflow")?;
                    let bytes = inverted
                        .get(input.dat_offset..dat_end)
                        .ok_or("Catalog DAT range exceeds file")?;
                    let digest = fujimod_firmware::sha256(bytes);
                    if digest != expected_hash
                        || end > 0x8000000
                        || offset < 0x20000
                        || dat_storage_inputs.iter().any(|range| {
                            let lo = range["flashOffset"].as_u64().unwrap() as usize;
                            let hi = lo + range["bytes"].as_u64().unwrap() as usize;
                            offset < hi && lo < end
                        })
                        || (offset < 0x1d40000 && 0x1a00000 < end)
                        || catalog_nor_inputs.iter().any(|range: &Value| {
                            let lo = range["flashOffset"].as_u64().unwrap() as usize;
                            offset < lo + range["bytes"].as_u64().unwrap() as usize && lo < end
                        })
                    {
                        return Err("Invalid, overlapping or changed catalog NOR input".into());
                    }
                    if offset < 0x2a100
                        && end > 0x2a080
                        && (index != 0
                            || bytes.get(0x2a080 - offset..0x2a100 - offset)
                                != nor.get(0x2a080..0x2a100))
                    {
                        return Err("Catalog NOR bytes differ from the training pattern".into());
                    }
                    nor.resize(nor.len().max(end), 0xff);
                    nor[offset..end].copy_from_slice(bytes);
                    catalog_nor_inputs.push(json!({"name":name,"flashOffset":offset,
                        "datOffset":input.dat_offset,"bytes":size,"sha256":digest,
                        "sourceBytes":input.size,"compression":input.compression,
                        "decodedBytes":input.decoded_size,"decodedSha256":input.sha256}));
                }
            }
            Some(nor)
        } else {
            None
        };
        if probe >= Probe::Spi2FirstHeader {
            spi2::qualify(&ram, spi2_second_header, spi2_owner_ack)?;
            let mut expected = vec![0, 8, 32, 208];
            let mut response = vec![0; 4];
            if spi2_second_header {
                expected.extend_from_slice(&[8, 16, 128, 34]);
                response.extend_from_slice(&[8, 0, 160, 218]);
            }
            if spi2_owner_ack {
                expected.extend_from_slice(&[0, 0, 0, 0, 8, 0, 160, 218]);
                response.extend_from_slice(&[0, 0, 32, 200, 8, 0, 128, 18]);
            }
            definition.spi_dma_exchange = Some(fujimod_emulator_xp4_board::SpiDmaExchange {
                base: 0xfff34000,
                receive_request: 4,
                transmit_request: 5,
                beat_ticks: 1,
                expected,
                // shortcut: only the declared header bytes; later traffic needs separate qualification.
                response,
                bis_version: None,
            });
            if probe == Probe::LinkedDmaInput {
                let exchange = definition.spi_dma_exchange.as_mut().unwrap();
                exchange.expected.clear();
                exchange.response.clear();
                // Property bytes in the qualified X-T4 scenario, not a real BIS version.
                exchange.bis_version = Some([0xff; 2]);
            }
        }
        if touch {
            definition.touch_i2c = Some(TOUCH_PANEL);
        }
        let board = Rc::new(Session::new(definition, storage_image)?);
        board.write(0, &ram)?;
        for core in 0..4 {
            board.write_register(core, arm::PC, plan.entry as u64)?;
        }
        let mut bootstrap_instructions = 0;
        if probe >= Probe::OrderedStartup {
            while board.read_register(0, arm::PC)? != 0x4cc {
                if bootstrap_instructions >= 100_000 {
                    return Err(
                        "Primary did not reach the verified barrier initialization boundary".into(),
                    );
                }
                board.run(0, 1)?;
                bootstrap_instructions += 1;
            }
            bootstrap_instructions = board.instructions()[0];
        }
        let spi2_trace = if probe >= Probe::Spi2FirstHeader {
            Some(spi2::install_trace(
                &board,
                spi2_second_header,
                spi2_owner_ack,
                rec_input_trace.clone(),
            )?)
        } else {
            None
        };
        Ok(Self {
            board,
            edits: serde_json::to_value(&plan.edits).map_err(|e| e.to_string())?,
            state_template: OnceCell::new(),
            discovery,
            power_status_override,
            power_command_model,
            empty_retained,
            erased_flash,
            flash_training_fixture,
            bootstrap_instructions,
            flash_jedec,
            flash_identity_bytes,
            inactive_countdown,
            startup_gpio0_low,
            latched_startup_cause,
            invalid_clock,
            virtual_clock,
            passive_discovery,
            calibration_source_sha256,
            calibration_mode5_sha256,
            calibration_mode2_sha256,
            calibration_reprogram_sha256,
            calibration_post_control_sha256,
            calibration_post_mode2_sha256,
            calibration_selector1_sha256,
            calibration_removal_sha256,
            audio_reset_source_sha256,
            power14_source_sha256,
            power67_source_sha256,
            power17_source_sha256,
            rtc_staging_source_sha256,
            power_init_source_sha256,
            rtc_control_source_sha256,
            dat_storage_inputs,
            packed_resource_input,
            catalog_nor_inputs,
            control_request_source_sha256,
            ff702280_source_sha256,
            fa_ready_source_sha256,
            fa_completion_source_sha256,
            fff7801c_completion_source_sha256,
            f01c_second_completion_source_sha256,
            fff7801c_second_completion,
            fff50000_ready_source_sha256,
            fff50000_second_ready,
            fff74000_ready_source_sha256,
            linked_dma_source_sha256,
            power64_source_sha256,
            power2_echo_source_sha256,
            class3_source_sha256,
            power2_bit8_source_sha256,
            gpio9_source_sha256,
            power3_source_sha256,
            spi2_trace,
            rec_input_trace,
            rec_state_memory_watch_started: Cell::new(false),
            spi2_second_header,
            spi2_owner_ack,
            virtual_buttons: RefCell::new(BTreeMap::new()),
            measured_inputs: measured,
            checkpoint_identity: checkpoint::identity(probe, source, measured)
                + if touch { "/touch-panel" } else { "" },
        })
    }

    pub fn enable_rec_input_trace(&self) -> Result<Value, String> {
        let trace = self
            .rec_input_trace
            .as_ref()
            .ok_or("REC input trace requires fa_ready")?;
        spi2::enable_rec_input_trace(
            &self.board,
            trace,
            self.spi2_second_header,
            self.spi2_owner_ack,
        )
    }

    pub fn enable_rec_display_trace(&self) -> Result<Value, String> {
        let trace = self
            .rec_input_trace
            .as_ref()
            .ok_or("REC display trace requires fa_ready")?;
        spi2::enable_rec_display_trace(
            &self.board,
            trace,
            self.spi2_second_header,
            self.spi2_owner_ack,
        )
    }

    fn rec_state_memory_trace(&self) -> Result<Value, String> {
        let watch =
            serde_json::to_value(self.board.memory_watch_state()).map_err(|e| e.to_string())?;
        Ok(
            json!({"softwareFieldAddress":0x1b7c9c8,"sourceSha256":rec_input::PRODUCER_SOURCE_SHA256,
            "limit":4096,"startInstructions":self.bootstrap_instructions,"addressMappingVerified":false,
            "dataAddressContract":"backend physical data callback address; Fetch callbacks translate the virtual PC; original guest data VA is not recorded",
            "writeContract":"pre-store operand and before bytes; not a committed-store receipt",
            "coverageContract":"CPU memory hooks only; DMA and host bulk writes are outside this history",
            "watch":watch}),
        )
    }

    pub fn begin_rec_state_watch(&self) -> Result<Value, String> {
        if self.rec_state_memory_watch_started.get() {
            return Err("REC state memory watch already started".into());
        }
        if self.fa_ready_source_sha256.is_none() && self.fa_completion_source_sha256.is_none() {
            return Err("REC state memory watch requires fa_ready".into());
        }
        if self.bootstrap_instructions == 0
            || self.board.instructions().iter().sum::<u64>() != self.bootstrap_instructions
        {
            return Err("REC state memory watch requires a fresh post-bootstrap session before its first run".into());
        }
        self.board
            .memory_watch_begin(vec![(0x1b7c9c8, 0x1b7c9c9)], 4096)?;
        self.rec_state_memory_watch_started.set(true);
        self.rec_state_memory_trace()
    }

    pub fn begin_kernel_write_watch(&self) -> Result<Value, String> {
        self.check_kernel_observation()?;
        self.board.ram_write_watch_begin(
            KERNEL_WRITE_WINDOWS
                .iter()
                .map(|&(address, length, _)| {
                    (u64::from(address), u64::from(address) + u64::from(length))
                })
                .collect(),
            64,
        )?;
        self.kernel_write_watch()
    }

    fn check_kernel_observation(&self) -> Result<(), String> {
        if self.fa_ready_source_sha256.is_none() && self.fa_completion_source_sha256.is_none() {
            return Err("Kernel observation requires the X-T4 FA scenario".into());
        }
        if self.board.state(false).failure.is_some() {
            return Err("Kernel observation requires a healthy paused board".into());
        }
        let baseline = kernel_write_readback(&self.board)?;
        if baseline.iter().any(|row| row["matchesOriginal"] != true) {
            return Err("Kernel observation requires original2.12 physical code windows".into());
        }
        Ok(())
    }

    pub fn begin_kernel_irq_trace(&self) -> Result<Value, String> {
        self.check_kernel_observation()?;
        self.board
            .irq_trace_begin(0x49410, 0x49420, vec![0x403bc, 0x40414])?;
        self.kernel_irq_trace()
    }

    pub fn kernel_irq_trace(&self) -> Result<Value, String> {
        self.board
            .irq_trace_snapshot()
            .ok_or_else(|| "Kernel IRQ trace has not started".to_owned())
    }

    pub fn kernel_write_watch(&self) -> Result<Value, String> {
        let watch = self
            .board
            .ram_write_watch_snapshot()
            .ok_or("Kernel write watch has not started")?;
        Ok(
            json!({"watch":watch,"readback":kernel_write_readback(&self.board)?,
            "meaning":"Shared declared physical backing. CPU records are pre-store operands, not committed stores; host copies have a direct post-copy sample. Readback hashes describe this pause, not individual CPU stores. Context reads may affect soft TLB. Host/device rows do not invent a CPU owner. No physical-camera access."}),
        )
    }

    pub fn set_touch(&self, contacts: [Option<Contact>; 2]) -> Result<(), String> {
        self.board.set_touch(contacts)
    }

    pub fn set_virtual_button(&self, input: u32, pressed: bool) -> Result<(), String> {
        if self.power3_source_sha256.is_none() {
            return Err("Virtual buttons require the explicit raw768 session".into());
        }
        let &(_, port, pin, _) = VIRTUAL_BUTTONS
            .iter()
            .find(|(id, _, _, _)| *id == input)
            .ok_or("Unknown virtual button input")?;
        self.board.set_gpio_input(port, pin, !pressed)?;
        self.virtual_buttons.borrow_mut().insert(input, pressed);
        Ok(())
    }

    pub fn virtual_button_sample(&self, input: u32) -> Result<Vec<u8>, String> {
        let &(_, _, _, row) = VIRTUAL_BUTTONS
            .iter()
            .find(|(id, _, _, _)| *id == input && self.virtual_buttons.borrow().contains_key(id))
            .ok_or("Virtual button input has not been configured")?;
        self.board.read_virtual(0, row + 0x18, 8)
    }

    pub fn set_scene_gray(&self) -> Result<(), String> {
        self.board.set_scene_source(Source::Gray)
    }

    pub fn set_scene_analytic(&self) -> Result<(), String> {
        self.board.set_scene_source(Source::Analytic)
    }

    pub fn set_scene_rgb(&self, width: u32, height: u32, rgb: &[u8]) -> Result<(), String> {
        if !(1..=8192).contains(&width)
            || !(1..=8192).contains(&height)
            || rgb.len() as u64 != u64::from(width) * u64::from(height) * 3
        {
            return Err("Scene RGB requires 1..=8192 dimensions and width*height*3 bytes".into());
        }
        self.board.set_scene_source(Source::Rgb {
            width,
            height,
            rgb: rgb.into(),
        })
    }

    // Scenario declarations are immutable for the lifetime of this session.
    fn static_state(&self) -> Value {
        let mut assumptions = vec![];
        if self.linked_dma_source_sha256.is_some() {
            assumptions.push("BIS scalar peer: scenario version property bytes ff ff; GPIO0.4 low/IRQ171 then high/IRQ155; GPIO12.6 startup pulse resets transport and ACK sequence; sequence-matched ACKs. Commands41/42/282 return synthetic zero status/axis/scalar samples accepted by their decoded consumers; observed128/131/135/168/169/177/178 setters and255 barriers consume transport status only. Configuration handoff and unknown requests explicitly stop; physical IBIS health and long-term REC remain unqualified.");
        } else if self.spi2_owner_ack {
            assumptions.push("Four-header SPI2 owner experiment: the two-header sequence continues with synthetic000020c8 for TX00000000, then08008012 for predictedTX0800a0da. This assumes the original history copy/field131 and sequence0 remain valid at their consumers; actual entries/returns must be observed. A callback pointer, sequence increment or transport return0 is not owner message delivery: original post status and owner consumption must be captured separately. Any seventeenth byte or mismatch stops. Beat latency remains1guest tick; physical peer/timing, later commands and normal REC remain unqualified.");
        } else if self.spi2_second_header {
            assumptions.push("Two-header SPI2 experiment: TX000820d0/08108022 receives synthetic00000000/0800a0da. The prior emulated context has second-consumer fieldH=083 and savedH0=0; actual original consumer gates and return must still be observed. SavedH0=0 bypasses the protocol-owner callback and sequence advance even if the second action returns0. DMA beat latency is selected as1guest tick. A ninth byte or mismatch stops; physical peer/timing, command completion and normal REC remain unqualified.");
        } else if self.spi2_trace.is_some() {
            assumptions.push("Single SPI2 first-header experiment: TX000820d0 receives four synthetic zero bytes. The emulated baseline has both old+4c and current+50 fieldH=0; the original consumer must still run and be observed. This is not a physical peer capture. DMA beat latency is selected as1guest tick; electrical timing is unverified. Any fifth byte or mismatched request stops; no later protocol, sensor, shooting or recovery success is implied.");
        }
        if self.power_command_model {
            if self.latched_startup_cause {
                assumptions.push("Preexisting synthetic startup cause: status16=0x0010 before initialization. Observed checksum initialization commands are recorded and acknowledged; subsequent status16 reads stop as unqualified, with no invented clear/keep law or power transitions.");
            } else {
                assumptions.push("Zero fixed status16; observed initialization commands acknowledged without enabled/pending transitions; power-transition requests stop execution.");
            }
        }
        if self.empty_retained {
            assumptions.push("Registers31..34 are session-local 16-bit storage initialized to zero: no valid initial retained-data marker or physical power-cycle retention. Optional RTC control behavior is declared separately.");
        }
        if self.erased_flash {
            if self.flash_jedec == Some(0x2c) {
                if self.dat_storage_inputs.is_empty() {
                    assumptions.push("Synthetic one-byte NOR manufacturer2c with matching source-table geometry; upper identity bytes are unprovided and wider RDID requests stop. No measured physical part identity, installed firmware or settings contents.");
                } else {
                    assumptions.push("Synthetic one-byte NOR manufacturer2c with source-table geometry; upper identity bytes remain unprovided. Five raw DAT-backed ranges are mapped at their verified flash offsets, including object8. Other unspecified bytes are erasedFF; this is not a complete physical flash or per-unit calibration dump.");
                }
            } else {
                assumptions.push("Erased NOR base with catalog manufacturer0, geometry, IRQ and functional timing; only one identity byte is supplied, no installed firmware or settings contents.");
            }
        }
        if self.flash_training_fixture {
            if self.dat_storage_inputs.is_empty() {
                assumptions.push("Ideal training handshake with 8-tick completions and B4=0 clearing BC status; only the verified loader calibration pattern maps to NOR offset0x2a080. All taps see identical data; no measured analog window or complete flash layout.");
            } else {
                assumptions.push("Ideal training handshake with8-tick completions and B4=0 clearingBC; the verified loader pattern maps to NOR0x2a080 and additional DAT-backed ranges are listed separately. All taps still see identical pattern data; physical timing and a complete flash layout remain unqualified.");
            }
        }
        if self.bootstrap_instructions != 0 {
            assumptions.push("Preloaded harness runs only core0 until its original PC0x4cc, after the four barrier-initialization stores; secondary cores then start at the original entry. Physical Boot ROM release ordering is unknown.");
        }
        if self.inactive_countdown {
            assumptions.push("Countdown18 is inactive with zero status: only the observed0100 initialization is acknowledged and recorded. Arming0101, timer values, expiry and IRQ semantics remain unsupported; this is separate from the RTC.");
        }
        if self.measured_inputs {
            assumptions.push("Measured inputs: all16 initial GPIO input registers and12 ADC channels are the real-camera ff80 app80 jig baseline (both cards, no lens, dials as left), replacing the catalog, startup GPIO0 and GPIO9 values. Jig-mode levels include pins the jig firmware drove as outputs; the model's input register still ignores guest output state.");
        }
        if self.startup_gpio0_low && !self.measured_inputs {
            assumptions.push("Initial external GPIO bank0 bit0 is low (input3 to2, preserving bit1); original loader code captures this input for the main startup condition. Physical switch identity and polarity are unknown; no handoff RAM or guest return is patched.");
        }
        if self.invalid_clock {
            assumptions.push("Read-only invalid-clock input: control39=0. No seconds value, staging/commit, strobe or record-valid law is supplied; clock writes and seconds reads remain unsupported.");
        }
        if self.virtual_clock {
            assumptions.push("Synthetic functional RTC: initially invalid/stopped, source-derived registers39/35/36/29/30 and firmware epoch2000. Guest stages both halves; load starts a54,000,000-guest-tick/second counter. Virtual strobe is set at commit and cleared from the next tick; physical oscillator/strobe/persistence are unmeasured. Record commit before valid time and alarms remain unsupported.");
        }
        if self.passive_discovery {
            assumptions.push("Passive discovery records unsupported accesses and their shadow values; automatic poll-bit modifications are disabled. An unmodeled handshake can wait indefinitely rather than being forced complete.");
        }
        if self.calibration_source_sha256.is_some() {
            if self.calibration_selector1_sha256.is_some() {
                assumptions.push("Ideal calibration with selector continuation: inherited cold/mode5/removal results768, latency8 and unit0 preinitialization remain synthetic. The original71-word mode2 after Mode5Done clears the whole5384 word after8ticks. This Mode2Done may then accept the configured selector1 sequence, then alternate the original selector0/selector1 three-write scripts, retaining its cold-origin map and command history; unknown writes still reject. All inherited RTC/power/gate models remain synthetic. Physical calibration, timing and normal REC remain unqualified.");
            } else if self.calibration_mode2_sha256.is_some() {
                assumptions.push("Ideal calibration plus single mode2 experiment: inherited cold/mode5/removal results768 and latency8 are synthetic, unit0 is preinitialized. After Mode5Done, each unit may instead execute its exact source-derived71-word mode2 table once; other units may still use the original removal path. Completion clears the whole5384 word after8guest ticks, matching the original fullword-zero poll predicate, not measured hardware timing. Mode2Done rejects further writes. All inherited RTC/power/gate models remain synthetic. Physical calibration, command side effects, reentry and normal REC remain unqualified.");
            } else {
                assumptions.push("Ideal-calibration experiment only: four independent11-bit result fields per unit are selected as768, not recovered measurements. Original FF-mode686+7+17 source writes drive8-guest-tick synthetic completion events; nominal words stay guest-written. Unit0 is explicitly preinitialized for the RAM-preload harness. One cold initialization, one complete71-word mode5 script and one ordered removal per unit are supported. Mode5 clears the full5384 word after8ticks. The source removal script and7080=100 schedule an8-tick event setting7090 bit0; this is a synthetic completion, not measured readiness. FF70C080 uses the existing posted-request model with8-tick completion; external gate/RMW writes remain passive shadows without physical routing or power effects. Restore/reset/reentry and other modes remain unsupported. This does not qualify physical DDR calibration or timing.");
            }
        }
        if self.calibration_selector1_sha256.is_some() {
            assumptions.push("UNMEASURED FUNCTIONAL ASSUMPTION: firmware 0x11c33a8 mode5 reentry after completed selector0 (cold or post-control origin) accepts only the existing exact71-write table, repeatedly. Reuse cold mode5 latency8 guest ticks, timed full-word5384=0 completion and its retained-register/result convention; no hardware timing or side effects are established. Jig mode has no UI: FF505384 read0 at idle, while reading FF505000 hung the bus. Replace this assumption with timestamped stock UI reentry writes/status, latency, prerequisites, IRQ and retained-bank captures.");
            assumptions.push("Selector continuation extension: inherit EMULATED captured reprogram/profile data, original730-write reprogram,8-write post-control and71-write post-mode2 lifecycles. Four prior8tick post events set paired7c18 masked40000001 to1, clear7090 bit0, set primary2110 bit0 and40dc/42dc masks1300 to300, then7364 bit0 to1; these associations remain synthetic; the original18 sample fields remain640. Canonical Mode2Done or PostMode2Done accepts paired5038=03204020,5438=83,5384=1; completed selectors may alternate the source-guarded selector0 (5038=01004100,5438=81,5384=1) and selector1 scripts. A single8tick synthetic event clears the whole5384 word; no other result or readiness is changed. Explicit origin follows prefix, pending and done, retaining all prior maps and event counts. Each reentry preserves the mode2 origin and prior command count across checkpoints; every other write is rejected. The source and actual run13 qualify this bounded selector1 path, not a physical hardware function, timing or normal REC.");
        } else if self.calibration_post_mode2_sha256.is_some() {
            assumptions.push("Post-control mode2 continuation: inherit the captured EMULATED reprogram inputs,730-write plan and8-write/four-event post-control fixture. Only canonical PostControlDone may accept its original71-word mode2 table once, retaining every prior primary/paired register except the actual script targets. The last5384=1 command schedules one8tick synthetic fullword-zero completion and enters PostMode2Done, which rejects later writes. The four inherited8tick post events set paired7c18 masked40000001 to1, clear7090 bit0, set primary2110 bit0 and40dc/42dc readiness masks, then set7364 bit0; those associations remain synthetic. Original18 sample fields remain640. Manager source and observed run12 dispatcher qualify this specific lifecycle, not arbitrary reentry. Unit0 is not a captured reprogram target. Physical calibration, timing, external gates and normal REC remain unproven.");
        } else if self.calibration_post_control_sha256.is_some() {
            assumptions.push("Captured reprogram plus post-control extension: inherit run10 EMULATED slots1..3/profile17 and the original730-write reprogram plan. Then accept only the original8 post-control writes once per ReprogramDone unit. Four synthetic8tick events set paired7c18 masked40000001 to1, clear7090 bit0, set primary2110 bit0 and40dc/42dc readiness masks, then set7364 bit0. These are explicit virtual completion laws, not recovered physical causality or timing. Initial-script18 sample fields remain640; no new calibration result is fabricated. Retain every other register; final acknowledgment follows the original bounded sequence. ExternalFFF49C00 remains passive/unmodeled. Completion rejects further writes; normal REC remains unproven.");
        } else if self.calibration_reprogram_sha256.is_some() {
            assumptions.push("Single reprogram extension: slots1..3/profile17 are the actual run10 EMULATED cache bytes, each slot matches its modeled primary getter; slot0 is excluded. Their physical calibration validity is unproven. Only canonical Removed may replay the original730 banked writes with six cached RMWs based on this replay's prior writes; unwritten map entries are retained. Three independent8tick completions affect only7044 bit0,7090 lowbyte and three primary PLL masks. Paired7c18 is not invented as1, and the externalFFF49C00 gate remains separately owned. Completion rejects further writes; this is not a physical reset/retention/timing law or normal REC proof.");
        }
        if self.f01c_second_completion_source_sha256.is_some() {
            assumptions.push("Synthetic second FFF4F01C command experiment: inherit the original first fullword-zero consumer and permit repeated write32(FFF4F01C,1) after each Done state. Each accepted command has an independent8guest-tick deadline and a checked monotonically increasing token/ordinal; stale tokens cannot complete a later request. Pending same-value writes do not create a command or postpone its deadline. Reads never complete requests; initial reads, other routed values/widths are rejected, while cross-window accesses retain the existing fallback. The second consumer only tests bit0; selecting wholeword0 is an explicit additional assumption. Host RAM/DMA, PC, task flags and IRQs are untouched. F008/F00C/F010/F014 and other-unit effects remain unmodeled/passive; FFF4F01C and FF70F01C are distinct addresses, with no physical alias inferred. Hardware owner, electrical reset/clock law, physical timing and normal REC remain unqualified.");
        } else if self.audio_reset_source_sha256.is_some() {
            assumptions.push("Single audio-task reset experiment: only the observed write32(FFF4F01C,1) is accepted. Status is busy1 until an8-guest-tick synthetic completion makes the fullword0; no PC/poll-based completion, electrical reset or clock/IRQ routing is inferred. Initial reads, other values/widths and completed reentry remain unsupported. Physical completion and timing are unmeasured.");
        }
        if self.power14_source_sha256.is_some() {
            assumptions.push("Synthetic read-only power register14=0: all16 bits are deliberately selected zero, including input callback12031c8 bit5. This is not a measured value or inferred from the invalid cache/stale SPI response. The callback also uses a prior snapshot bit12 and GPIO8.4; those remain guest-derived. Physical input identity, other bits and transitions are unqualified.");
        }
        if self.power67_source_sha256.is_some() && self.power17_source_sha256.is_none() {
            assumptions.push("Synthetic read-only power register67=0: the entire16-bit word is deliberately selected zero. Original initializer11d5b74 caches this word at1790f32; software later changes its low2 bits and bit15. Neither the physical meaning nor a hardware reset value is known. Zero is not inferred from the invalid cache or stale SPI bytes. Register67 writes and register17 reads remain unsupported; no electrical transition or acknowledgement law is supplied.");
        }
        if self.power17_source_sha256.is_some() {
            assumptions.push("Synthetic read-only power register67=0: the entire16-bit word is deliberately selected zero, not a measured reset value. Original initializer caches it at1790f32; software later changes low2 bits and bit15. Register67 writes and electrical transitions remain unsupported.");
            assumptions.push("Synthetic read-only power register17=0: the entire16-bit word is selected independently of command17 writes. Original initializer stores the successful read at1790ee8. The read input never returns or changes the last7f00/3700 command; writes retain the existing bounded command-only contract. No read-clear, acknowledgement, hardware reset value or physical power effect is inferred.");
        }
        if self.rtc_staging_source_sha256.is_some() {
            assumptions.push("Synthetic RTC staging readback: registers29/30 expose low/high halves of the last guest-staged u32, including across commit. Reads do not advance, clear or mutate latches or the running counter; the counter's seconds registers35/36 remain separate. This is an explicit virtual latch law, not physical readback evidence or a forced ready/completion bit. Other RTC operations remain bounded by the existing model.");
        }
        if self.power_init_source_sha256.is_some() {
            assumptions.push("Synthetic independent fixed reads37/38/69/70/71=0: every complete16-bit word is deliberately selected zero, not inferred from cache initialization or18=0100. These read inputs do not implement countdown, read-clear, physical status, elapsed-time behavior or register writes. The existing18=0100 command leaves these fixed reads unchanged;18=0101 and37/38 writes remain unsupported. Source consumers store full words; physical meaning and values remain unqualified.");
        }
        if self.rtc_control_source_sha256.is_some() {
            assumptions.push("Synthetic unchanged RTC control39 echo: a valid fullword3 request is acknowledged only when3 is already the committed current control, all declared retained words exist and no fresh writes are pending. Echo changes no RTC/data/time/IRQ state. Partial fresh data still fails; a full fresh set still takes the existing commit path. Other bit changes, strobe loads and physical write-clear effects are not admitted by this echo rule.");
        }
        if self.packed_resource_input.is_some() {
            assumptions.push("DAT-backed packed resources1: the original XOR-FF encoded0x340000-byte slot maps to NOR0x1a00000, including original padding. It is separate from decoded RAM preload; guest parsing and decompression remain subject to unmodeled devices. This supplies file bytes, not a physical flash dump or decoder completion.");
        }
        if !self.catalog_nor_inputs.is_empty() {
            assumptions.push("Additional catalog NOR inputs: nine verified DAT slots provide loader/table/raw modules and the remaining original compressed sections, including source padding. The loader's training-pattern overlap is byte-identical. Only listed ranges are supplied; gaps remain erasedFF, not a complete physical flash or per-unit dump. These bytes do not change device timing or force completion.");
        }
        if self.control_request_source_sha256.is_some() {
            assumptions.push("Synthetic FFFAE080 posted-request transport: only this32-bit word is modeled, initially idle0; commands0/2/3/5/6 reuse Requests with selected8-guest-tick completion. Reads do not trigger ACK. The same source helper serves FF70 and FFFAE, but physical module identity/timing are unknown. No parameter, clock, reset, IRQ or RAM effect is added. Independent FFFAE0F0 status remains unmodeled/passive; an observed zero there is not physical readiness evidence.");
        }
        if self.power64_source_sha256.is_some() {
            assumptions.push("Synthetic independent power64/65 reads: both complete16-bit words are selected0, not inferred from cache/policy initialization, another register or a measured reset. Register64 controls guest gates/bits;65 software cache/event consumption remains original guest code. Fixed inputs never read-clear or generate IRQs;64/65 writes and physical transitions remain unsupported. This does not establish button identity or physical inactivity.");
        }
        if self.power2_bit8_source_sha256.is_some() {
            assumptions.push("Synthetic word2 checksum acknowledgment: complete0001 and explicitly selected0101 writes with a valid request checksum are echoed using separate opt-ins. Read2 stays the independent declared0 input, with no last-write readback or stored register. No guest mirror write or word2-specific IRQ/power/reset/clock/electrical effect is supplied; ordinary SPI completion remains active. Original guest RMW/counter branches and waits run unchanged. Other words remain unsupported. The0101 history retains its first256 ticks and counts later receipts without stopping transport; the original0001 history still has its separate256 limit. This is a transport experiment, not measured camera behavior.");
        } else if self.power2_echo_source_sha256.is_some() {
            assumptions.push("Synthetic word2 checksum acknowledgment: only complete0001 writes with a valid request checksum are echoed. Read2 stays the independent declared0 input; no last-write readback, stored register, guest mirror write, IRQ, power/reset/clock or electrical effect is supplied. Original guest RMW and scheduler waits run unchanged. Other words remain unsupported; receipt history is bounded to256.");
        }
        if self.class3_source_sha256.is_some() {
            assumptions.push("Synthetic class3 power inputs: full u16 values47/111/118=2000,48/112/119=1000,50/114/121=2982,49/72/113/120=0 are independent fixed read replies. These are not camera measurements, physical defaults or backup-derived calibration. The conditional numeric candidate gives ratio50 and derived25 under the observed settings; later settings/gates and high-byte classification remain original guest code. No guest cache/validity/PC, event/IRQ, write semantics or hardware read-clear effect is supplied.");
        }
        if self.gpio9_source_sha256.is_some() {
            assumptions.push("Synthetic cold GPIO9.5 high: external bank9 changes from0 to0020, preserving every other declared input bit. Original selector00090005 reads this input; descriptor input11 compares against0. Cold inactive sampling may omit the initial callback rather than emit a release. No property9, pending record, setting, UI mode, PC or return value is patched. Physical line identity and camera electrical state are unmeasured; original guest routing determines the result.");
        }
        if self.power3_source_sha256.is_some() {
            assumptions.push("Synthetic external power read3 changes only its fixed u16 reply512 to768 (low10 raw768, upper6 bits0). This also feeds the original ring/average and other read3 consumers, not solely the selected-client comparison. All settings, guest caches/flags, word2 writes, independent read2 and timing remain original or previously declared. No physical voltage, healthy battery state or exhaustive consumer coverage is established.");
            assumptions.push("The explicit raw768 experiment retains the first256 valid0001 checksum-echo receipt ticks plus a checked u64 total; trace capacity no longer interrupts those otherwise admitted complete frames. This recorder policy adds no power, stored-register, read2, IRQ or guest-state side effect.");
        }
        if !self.virtual_buttons.borrow().is_empty() {
            assumptions.push("Virtual navigation inputs are changed through six source-qualified active-low GPIO selectors. Only their declared bits change; original guest scanning, debounce, mapping and UI decide the result. These are synthetic logical inputs, not verified physical wiring; cold GPIO metadata describes the initial values.");
        }
        if self.ff702280_source_sha256.is_some() {
            assumptions.push("Synthetic FF702280 posted-request transport: add only this4-byte word, initially0, to the existing Requests model with8guest-tick wholeword-zero completion and no IRQ. Original write2 and low3bits-zero poll are qualified; for written2 wholeword0 satisfies that predicate. The reused model also admits0/2/3/5/6, zero cancellation and resubmission; identical busy writes do not postpone completion. Those broader commands are inherited virtual behavior, not qualified hardware laws. Non32-bit accesses stay unmodeled. FF702240=0x132 remains a passive shadow with no modeled routing effect. Diagnostic save/restore is not typed eventqueue restoration. Physical function, timing, software-lock ownership and normal REC remain unproven.");
        }
        if self.fa_ready_source_sha256.is_some() {
            assumptions.push("Synthetic single FA ready experiment: own only the4-byte wordsFA000000 andFA00001C. Initial status0 is selected; accept one observed status-write0, then arm40 schedules one8guest-tick event setting only bit16. Reads do not complete it. The original read|1 path predicts10001; after that single write the modeled word remains10001, including bit0, with no automatic completion, IRQ, clock, RAM/DMA or other FA register effect. Duplicate arms, unsupported routed widths/values and reentry are rejected. Cross-window accesses retain the board's existing fallback. Other FA configuration/copy words stay passive; arm40 is a helper's literal return, not the24-byte copy length. Ordering and the bit16 poll are original-source qualified; physical causality, timing, module identity, bit0 clearing and normal REC remain unproven.");
        }
        if self.fa_completion_source_sha256.is_some() {
            assumptions.push("Synthetic FA completion experiment: own only exact4-byte FA000000/FA00001C/FFF4AA10 words. Selected initial status/control0 and two independent8guest-tick deadlines; ready sets bit16, guest ACK sets bit0, internal completion asserts IRQ145. A bit3-set AA10 word write preserving its other shadow bits clears only FA bit0 and deasserts that line; the trailing bit3-clear write closes the cycle. This is a write strobe, not an edge rule. Repetition requires a new status-write0; reads never complete requests. AA10 lowbits0..3 are synthetic shadow storage; other values/accesses are rejected. Original handler, callback and task event APIs execute unmodified; no host RAM/DMA, task flags, owner, callback-slot or R7 writes. FF70F01C is not owned here and its physical alias is unknown. Physical ACK causality, timing, data processing, errors, cancellation and concurrent users are unqualified; normal REC is not inferred.");
        }
        if self.fff7801c_second_completion {
            assumptions.push("Synthetic second FFF7801C command experiment: inherit the qualified first1C and repeatable FFF4F01C scenario. Each completed request permits another exact word write1 atFFF7801C; its independent checked token and8guest-tick deadline change pending1 to wholeword0. The first consumer requires wholeword0; the second unit1 consumer tests bit0 only. Pending repeats are rejected, stale tokens cannot complete later requests, and reads never drive completion. No host PC/register/task/RAM/DMA/IRQ effects; other module registers retain existing fallback. Both original consumer guards are retained, not a complete dependency closure. Hardware owner, physical timing, configuration effects and normal REC remain unproven.");
        } else if self.fff7801c_completion_source_sha256.is_some() {
            assumptions.push("Synthetic single FFF7801C command experiment: inherit FA completion and own only this exact4-byte word. The source-qualified first enable writes1 and polls the whole word until0. One selected8guest-tick event changes pending1 to done0; reads never complete it. Initial reads, other routed values/widths and second writes are rejected; cross-window accesses retain the existing board fallback. No host RAM/DMA, task mask, register, PC or IRQ effects; EXR2 VectorGpu is not wired here. The pre-enable control dispatch still executes through the original firmware and existing models. Hardware owner, physical reset/clock law, timing, repetition and normal REC are unqualified.");
        }
        if self.fff50000_second_ready {
            assumptions.push("Synthetic second FFF50000 ready experiment: after each Ready state, permit another exact word write0 atFFF50000. An independent checked token and8guest-tick deadline change pending wholeword0 to Ready wholeword1; stale tokens cannot complete later requests. Pending repeats are rejected, reads never advance, and initial reads remain unsupported. The original72-byte consumer and word helpers are unchanged; their120-byte guard is not a complete upstream closure. The higher-level reason for reentry, object identity, hardware owner and physical ready/timing law are unknown. No host PC/register/task/RAM/DMA/IRQ effects or SD Host/Clocks reuse; normal REC is unproven.");
        } else if self.fff50000_ready_source_sha256.is_some() {
            assumptions.push("Synthetic FFF50000 ready experiment: own only this exact4-byte word. Accept one source-qualified write0, read pending wholeword0 until a selected8guest-tick event changes it to wholeword1. Only bit0 being set is required by the original consumer; the other zero bits and delay are deliberate virtual choices. Reads never advance completion; initial reads, repeat commands and other routed values/widths are rejected, while cross-window accesses retain existing fallback. No host PC/register/task/RAM/DMA/IRQ effects or SD Host/Clocks reuse. The72-byte consumer and two word helpers are qualified, not an upstream dependency closure. Hardware owner, physical ready cause/timing and normal REC remain unknown.");
        }
        if self.fff74000_ready_source_sha256.is_some() {
            assumptions.push("Synthetic FFF74000 ready experiment: inherit the repeatable FFF50000 scenario and own only exact4-byte FFF74000. Accept source-qualified write0 after each ready state with a fresh checked token; a checked8guest-tick event changes pending wholeword0 to ready wholeword1. The original52-byte consumer only requires bit0 set; other zero bits and latency are deliberate virtual choices. Reads never advance completion. Initial reads, pending repeats and other routed values/widths reject; cross-window accesses retain existing fallback. No host PC/register/task/RAM/DMA/IRQ effects or inferred clock/reset routing. The100-byte consumer/helper guard is not a complete upstream closure. Hardware owner, physical ready law/timing and normal REC remain unknown.");
        }
        if self.linked_dma_source_sha256.is_some() {
            assumptions.push("Synthetic aggregate A7/peripheral clock ratio 8/1: all four cores share a budget of 432 million instructions/s at 54 MHz (108 million/core if balanced). Architectural CPU waits may advance guest time. XP4 CPU frequency, IPC and cycle timing are unmeasured; device periods and the 54 MHz timer domain stay unchanged.");
            assumptions.push("Synthetic FF70A080 posted request: unit A of the FF70x080 control-request series reuses the existing Requests model (8-tick wholeword-zero completion, repeatable). UILIB 011BFB40 and the battery task post requests here at REC; physical unit identity and timing are unverified.");
            assumptions.push("Synthetic WAVE audio handshake: FFFA8000 word write0 schedules ready1 and FFFA8004 word write1 schedules self-clear0 after8guest ticks each. Initial five words are selected0; +8/+C/+10 retain word writes. Reads are observational; busy repeats and other commands reject, completed requests may repeat with fresh tokens. Typed checkpoints preserve words and deadlines. Processing requests started with FFFA8008.bit0=1 emit one GIC144 completion edge at the deadline; initialization with bit0=0 remains poll-only. No audio data or DMA coupling is inferred. Firmware011c4568/011c45a8 qualifies the polls; physical latency, reset values and broader reentry remain unmeasured.");
            assumptions.push("Synthetic FFFAB000 clock lock law: every word write at+0 clears ready bit0 and restarts a declared50us delay (2700 ticks at54MHz); +4/+8/+C/+10/+14 are passive word storage. Display/pixel-clock identity is unproven; no frequency, LCDC/MOVDC coupling or IRQ is inferred.");
            assumptions.push("Synthetic SD cards: both slots present and writable, each 16 GiB (0x2000000 sectors), empty FAT32 labelled FUJIFILM. FujiMod SDHC hosts use declared 4-tick transfers, 2-tick clocks and RCA 1; IRQs 230 (slot 1) and 237 (slot 2) are the GIC lines the firmware newly enables when each host raises its first interrupt.");
            assumptions.push("UNMEASURED FUNCTIONAL ASSUMPTION: linked audio RX supplies one1440-word startup FIFO at1 tick/word for each DMA activation, then silent four-byte words at2 words/1125 ticks. Physical ADC FIFO depth, lane/sample format and startup timing are unknown. Firmware WAVE startup requires >2 blocks before its20x2ms retry; this bounded burst supplies three blocks in30.027ms, then the original steady rate preserves continuous producer/consumer cadence. FIFO credit does not reset at descriptor boundaries or request reassertion. RX uses request28/channel4/FFF4F000/IRQ211, TX request30/channel8/FFF78004/IRQ215 uses2 words/1125ticks: firmware125f70c emits8bytes per48kHz stereo sample (720 samples/5760 bytes). No TX prefill. Physical ADC/DAC clock accuracy remains unmeasured. Guest descriptors, DMA cursors, enable gates, chain-end IRQs and all validation remain authoritative; no guest callbacks or flags are forced. These are functional timing/data assumptions, not measured microphone behaviour.");
            assumptions.push("UNMEASURED FUNCTIONAL ASSUMPTION: Bluetooth UART1 uses PL011 TXFE/RXFE idle flags, asserted CTS and a synchronous TX sink, with an empty receiver and no radio peer. This releases the firmware TX-empty poll without fabricating HCI replies, pairing or physical UART timing.");
        }
        let mut state = json!({
            "start": "preloaded-cold-loader", "bootVerified": false,
            "boardProfile": "xt4-212", "discovery": self.discovery,
            "boardProfileSha256": fujimod_firmware::sha256(if self.erased_flash { ERASED_FLASH_PROFILE } else { BOARD_PROFILE }.as_bytes()),
            "powerStatusOverride": self.power_status_override,
            "powerCommandModel": self.power_command_model,
            "powerRetainedWords": if self.empty_retained { vec![31, 32, 33, 34] } else { vec![] },
            "erasedFlash": self.erased_flash && !self.flash_training_fixture,
            "flashTrainingFixture": self.flash_training_fixture,
            "bootstrapInstructions": self.bootstrap_instructions,
            "flashJedec": self.flash_jedec,
            "flashIdentityBytes": self.flash_identity_bytes,
            "inactiveCountdown": self.inactive_countdown,
            "startupGpio0Low": self.startup_gpio0_low && !self.measured_inputs,
            "measuredInputs": self.measured_inputs.then(|| fujimod_firmware::sha256(MEASURED_PROFILE.as_bytes())),
            "latchedStartupCause": self.latched_startup_cause,
            "invalidClock": self.invalid_clock,
            "virtualClock": self.virtual_clock,
            "passiveDiscovery": self.passive_discovery,
            "datStorage":if self.dat_storage_inputs.is_empty(){None}else{
                let mut storage = json!({
                    "encoding":"xor-ff","flashDatBias":0x274,"containerPayloadOffset":0x27c,"ranges":self.dat_storage_inputs,
                    "unknownFill":"erasedFF","completePhysicalFlash":false
                });
                if let Some(input) = &self.packed_resource_input { storage["packedResource"] = input.clone(); }
                if !self.catalog_nor_inputs.is_empty() { storage["catalogSlots"] = json!(self.catalog_nor_inputs); }
                Some(storage)
            },
            "power14Input":self.power14_source_sha256.as_ref().map(|hash|json!({
                "register":14,"value":0,"testedMask":0x20,"sourceSha256":hash,"synthetic":true
            })),
            "power67Input":self.power67_source_sha256.as_ref().map(|hash|json!({
                "register":67,"value":0,"sourceSha256":hash,"synthetic":true,"readOnly":true
            })),
            "power17Input":self.power17_source_sha256.as_ref().map(|hash|json!({
                "register":17,"value":0,"sourceSha256":hash,"synthetic":true,"independentOfCommands":true
            })),
            "rtcStagingReadback":self.rtc_staging_source_sha256.as_ref().map(|hash|json!({
                "registers":[29,30],"sourceSha256":hash,"synthetic":true,
                "law":"last-staged-u32-latches","advancesWithCounter":false
            })),
            "powerInitReadInputs":self.power_init_source_sha256.as_ref().map(|hash|json!({
                "registers":POWER_INIT_READ_REGISTERS,"value":0,"sourceSha256":hash,
                "synthetic":true,"readOnly":true,"independentOfCountdown":true
            })),
            "rtcControlEcho":self.rtc_control_source_sha256.as_ref().map(|hash|json!({
                "register":39,"value":3,"sourceSha256":hash,"synthetic":true,
                "requiresCurrentWord":true,"requiresNoFreshData":true
            })),
            "audioResetFixture":self.audio_reset_source_sha256.as_ref().map(|hash|json!({
                "address":0xfff4f01cu32,"request":1,"latencyTicks":CALIBRATION_TICKS,
                "sourceSha256":hash,"synthetic":true,"singleRequest":self.f01c_second_completion_source_sha256.is_none()
            })),
            "calibrationFixture": self.calibration_source_sha256.as_ref().map(|hash| json!({
                "sourceSha256":hash,"qualityInputs":CALIBRATION_RESULTS,
                "mode5SourceSha256":self.calibration_mode5_sha256,
                "removalSourceSha256":self.calibration_removal_sha256,
                "removalControlRequest":{"address":0xff70c080u32,"latencyTicks":CALIBRATION_TICKS},
                "latencyTicks":CALIBRATION_TICKS,"unit0Preinitialized":true,
                "synthetic":true,"scope":if self.calibration_selector1_sha256.is_some(){"Inherited initialization and reprogram/post/mode2 paths followed by selector1 from either mode2 origin and strict alternating selector0/selector1 reentry"}else if self.calibration_post_mode2_sha256.is_some(){"Inherited initialization, one reprogram, post-control and one mode2 continuation per selected unit; later writes rejected"}else if self.calibration_post_control_sha256.is_some(){"Inherited initialization and one reprogram plus one original8-write post-control per selected unit; later writes rejected"}else if self.calibration_reprogram_sha256.is_some(){"Inherited cold/mode5/removal/mode2 plus one captured reprogram per selected Removed unit; no subsequent writes"}else if self.calibration_mode2_sha256.is_some(){"FF-mode cold initialization, one mode5, then either one removal or one mode2 per unit; mode2 completion rejects further writes"}else{"FF-mode cold initialization + one mode5 and one removal per unit; restore/reset/reentry unsupported"}
            })),
            "assumptions": assumptions,
            "edits": self.edits,
        });
        if let Some(hash) = &self.control_request_source_sha256 {
            state["controlRequestFixture"] = json!({"address":0xfffae080u32,"initialWord":0,
                "commands":[0,2,3,5,6],"latencyTicks":CALIBRATION_TICKS,"sourceSha256":hash,
                "synthetic":true,"ownsBytes":4,"independentStatusModeled":false});
        }
        if let Some(hash) = &self.ff702280_source_sha256 {
            state["ff702280RequestFixture"] = json!({"address":0xff702280u32,"ownsBytes":4,
                "initialWord":0,"sourceGuardSha256":hash,"sourceGuardBytes":mode2::REQUEST2_GUARD_BYTES,
                "observedRequest":2,"originalPollMask":7,"originalPollExpected":0,
                "modelCommands":[0,2,3,5,6],"latencyTicks":CALIBRATION_TICKS,
                "wholeWordZeroOnCompletion":true,"readsTriggerCompletion":false,"raisesIRQ":false,
                "zeroCancels":true,"resubmissionAllowed":true,"sameBusyWriteExtendsDeadline":false,
                "non32BitAccess":"unmodeled","otherWordAddress":0xff702240u32,
                "otherWordObservedValue":0x132,"otherWordEffectsModeled":false,
                "synthetic":true,"physicalFunctionKnown":false,"physicalTimingVerified":false,
                "typedCheckpointRestorationSupported":false});
        }
        if let Some(hash) = &self.fa_ready_source_sha256 {
            state["faReadyFixture"] = json!({"addresses":[0xfa000000u32,0xfa00001cu32],
                "ownsBytesPerWord":4,"initialStatus":0,"initialPhase":"Cold",
                "sourceGuardSha256":hash,"sourceGuardBytes":fa::GUARD_BYTES,
                "initializationWrite":0,"armValue":0x40,"latencyTicks":CALIBRATION_TICKS,
                "pollMask":0x10000,"readyWord":0x10000,"predictedAck":0x10001,
                "doneWord":0x10001,"automaticBit0Clear":false,"readsTriggerCompletion":false,
                "armReadsSupported":false,"reentrySupported":false,"duplicateArmsSupported":false,
                "unsupportedRoutedAccess":"reject","crossWindowAccess":"existing-board-fallback",
                "raisesIRQ":false,"RAMorDMAEffects":false,"otherFAWordsModeled":false,
                "synthetic":true,"physicalFunctionKnown":false,"physicalCausalityVerified":false,
                "physicalTimingVerified":false});
        }
        if let Some(hash) = &self.fa_completion_source_sha256 {
            state["faCompletionFixture"] = json!({
                "addresses":[0xfa000000u32,0xfa00001cu32,0xfff4aa10u32],
                "ownsBytesPerWord":4,"initialStatus":0,"initialControl":0,
                "sourceGuardSha256":hash,"sourceGuardBytes":fa_completion::GUARD_BYTES,
                "readySourceGuardSha256":fa::GUARD_SHA256,
                "readyLatencyTicks":CALIBRATION_TICKS,"completionLatencyTicks":CALIBRATION_TICKS,
                "IRQ":145,"IRQLevelHeldUntilControlStrobe":true,"strobeRequiresRisingEdge":false,
                "readsTriggerCompletion":false,"repetitionRequiresStatusWriteZero":true,
                "AA10ShadowMask":15,"RAMorDMAEffects":false,"guestEventOrR7Writes":false,
                "F01CModeled":false,"F01CPhysicalAliasKnown":false,
                "synthetic":true,"physicalAckLawProven":false,"physicalTimingVerified":false
            });
        }
        if let Some(hash) = &self.fff7801c_completion_source_sha256 {
            state["fff7801cCompletionFixture"] = json!({
                "address":0xfff7801cu32,"ownsBytes":4,"command":1,
                "pendingWord":1,"completedWord":0,"latencyTicks":CALIBRATION_TICKS,
                "sourceGuardSha256":hash,"sourceGuardBytes":fff7801c_completion::GUARD_BYTES,
                "initialReadSupported":false,"reentrySupported":self.fff7801c_second_completion,"duplicateWritesSupported":false,
                "unsupportedRoutedAccess":"reject","crossWindowAccess":"existing-board-fallback",
                "readsTriggerCompletion":false,"raisesIRQ":false,"RAMorDMAEffects":false,
                "hostTaskMaskOrRegisterWrites":false,"synthetic":true,
                "hardwareOwnerKnown":false,"physicalTimingVerified":false
            });
        }
        if let Some(hash) = &self.f01c_second_completion_source_sha256 {
            state["f01cSecondCompletionFixture"] = json!({
                "address":0xfff4f01cu32,"ownsBytes":4,"command":1,"maximumCommands":null,
                "latencyTicks":CALIBRATION_TICKS,"pendingWord":1,"completedWord":0,
                "requestOrdinalsAndTokens":"monotonic per completed request","samePendingWrite":"does-not-resubmit-or-postpone",
                "firstConsumerPredicate":"wholeword-zero","secondConsumerPollMask":1,
                "sourceGuardSha256":hash,"sourceGuardBytes":f01c_second_completion::GUARD_BYTES,
                "initialReadSupported":false,"thirdCommandSupported":true,"readsTriggerCompletion":false,
                "unsupportedRoutedAccess":"reject","crossWindowAccess":"existing-board-fallback",
                "raisesIRQ":false,"RAMorDMAEffects":false,"hostTaskMaskOrRegisterWrites":false,
                "FF70F01CModeled":false,"physicalAliasKnown":false,
                "synthetic":true,"hardwareOwnerKnown":false,"physicalTimingVerified":false
            });
        }
        if self.fff7801c_second_completion {
            state["fff7801cSecondCompletionFixture"] = json!({
                "address":0xfff7801cu32,"ownsBytes":4,"command":1,"maximumCommands":null,
                "latencyTicks":CALIBRATION_TICKS,"pendingWord":1,"completedWord":0,
                "requestOrdinalsAndTokens":"monotonic per completed request","samePendingWrite":"reject",
                "firstConsumerPredicate":"wholeword-zero","secondConsumerPollMask":1,
                "firstSourceGuardSha256":fff7801c_completion::GUARD_SHA256,
                "firstSourceGuardBytes":fff7801c_completion::GUARD_BYTES,
                "secondSourceGuardSha256":f01c_second_completion::GUARD_SHA256,
                "secondSourceGuardBytes":f01c_second_completion::GUARD_BYTES,
                "initialReadSupported":false,"thirdCommandSupported":true,"readsTriggerCompletion":false,
                "unsupportedRoutedAccess":"reject","crossWindowAccess":"existing-board-fallback",
                "raisesIRQ":false,"RAMorDMAEffects":false,"hostTaskMaskOrRegisterWrites":false,
                "synthetic":true,"hardwareOwnerKnown":false,"physicalTimingVerified":false
            });
        }
        if let Some(hash) = &self.fff50000_ready_source_sha256 {
            state["fff50000ReadyFixture"] = json!({
                "address":0xfff50000u32,"ownsBytes":4,"command":0,"maximumCommands":if self.fff50000_second_ready {json!(null)} else {json!(1)},
                "pendingWord":0,"readyWord":1,"pollMask":1,"latencyTicks":CALIBRATION_TICKS,
                "sourceGuardSha256":hash,"sourceGuardBytes":fff50000_ready::GUARD_BYTES,
                "initialReadSupported":false,"reentrySupported":self.fff50000_second_ready,"duplicateWritesSupported":false,
                "readsTriggerCompletion":false,"unsupportedRoutedAccess":"reject",
                "crossWindowAccess":"existing-board-fallback","raisesIRQ":false,
                "RAMorDMAEffects":false,"hostTaskMaskOrRegisterWrites":false,
                "SDHostReused":false,"ClocksReused":false,"synthetic":true,
                "hardwareOwnerKnown":false,"physicalTimingVerified":false
            });
        }
        if self.fff50000_second_ready {
            state["fff50000SecondReadyFixture"] = json!({
                "address":0xfff50000u32,"ownsBytes":4,"command":0,"maximumRequests":null,
                "pendingWord":0,"readyWord":1,"pollMask":1,"latencyTicks":CALIBRATION_TICKS,
                "requestTokensAndOrdinals":"monotonic per completed request","pendingRepeat":"reject","thirdRequestSupported":true,
                "sourceGuardSha256":fff50000_ready::GUARD_SHA256,"sourceGuardBytes":fff50000_ready::GUARD_BYTES,
                "initialReadSupported":false,"readsTriggerCompletion":false,
                "crossWindowAccess":"existing-board-fallback","raisesIRQ":false,"RAMorDMAEffects":false,
                "hostTaskMaskOrRegisterWrites":false,"physicalTimingVerified":false,"synthetic":true
            });
        }
        if let Some(hash) = &self.fff74000_ready_source_sha256 {
            state["fff74000ReadyFixture"] = json!({
                "address":0xfff74000u32,"ownsBytes":4,"command":0,"maximumCommands":null,
                "pendingWord":0,"readyWord":1,"pollMask":1,"latencyTicks":CALIBRATION_TICKS,
                "sourceGuardSha256":hash,"sourceGuardBytes":fff74000_ready::GUARD_BYTES,
                "initialReadSupported":false,"reentrySupported":true,"duplicateWritesSupported":false,
                "readsTriggerCompletion":false,"unsupportedRoutedAccess":"reject",
                "crossWindowAccess":"existing-board-fallback","raisesIRQ":false,
                "RAMorDMAEffects":false,"hostTaskMaskOrRegisterWrites":false,
                "synthetic":true,"hardwareOwnerKnown":false,"physicalTimingVerified":false
            });
        }
        if let Some(hash) = &self.linked_dma_source_sha256 {
            state["linkedDmaInputFixture"] = json!({
                "sourceGuardSha256":hash,"sourceGuardBytes":linked_dma::GUARD_BYTES,
                "request":28,"channel":4,"fifoAddress":0xfff4f000u32,"irq":211,
                "trackingAddresses":[0xfff4f008u32,0xfff4f018u32],"trackingReads":"framework-fallback",
                "enableMask":0x01010001u32,"enableRule":"bit0 and either bit16 or bit24",
                "wordBytes":4,"wordValue":0,"wordLimit":null,
                "wordClock":{"ticks":1125,"words":2},"initialFifoWords":1440,
                "transmitSink":{"channel":8,"request":30,"fifoAddress":0xfff78004u32,"irq":215,"wordClock":{"ticks":1125,"words":2},"initialFifoWords":0},"countUnitHypothesis":"bytes",
                "firstBlockHypothesis":"current-registers","reloadHypothesis":"fetch-next-at-block-boundary",
                "maximumBlocks":256,"completionHypothesis":"IRQ edge at chain end only",
                "pendingStatus":0x10000,"blockCompleteBusyStatus":0x10001,"finalStatus":1,
                "completionAckPreservesActiveBusy":true,"inputExhaustion":"unlimited",
                "opaqueDescriptorWords":"require zero","mutatesRAMThroughDMA":true,
                "hostPCOrTaskOrCallbackWrites":false,"hardwareBehaviorVerified":false,
                "physicalDataCaptured":false,"normalRECVerified":false,"synthetic":true
            });
        }
        if let Some(hash) = &self.power64_source_sha256 {
            state["power64ReadInputs"] = json!({"registers":POWER64_READ_REGISTERS,"value":0,
                "sourceSha256":hash,"synthetic":true,"readOnly":true,"hardwareReadClearModeled":false});
        }
        if let Some(hash) = &self.power2_echo_source_sha256 {
            state["power2InitEcho"] = json!({"register":2,"value":1,"readInput":0,
                "sourceSha256":hash,"synthetic":true,"read2Independent":true,
                "maxReceipts":256,"hardwareEffectsModeled":false});
        }
        if let Some(hash) = &self.class3_source_sha256 {
            state["class3ReadInputs"] = json!({"values":CLASS3_READS.iter().map(|&(r,v,_,_,_)|(r.to_string(),v)).collect::<BTreeMap<_,_>>(),
                "sourceSha256":hash,"inputSha256":"1fd6561b85d1938b9d88cf4a1406d27b11435f759b889c7599681938f56552b7",
                "synthetic":true,"readOnly":true,"physicalValuesVerified":false,"hardwareReadClearModeled":false});
        }
        if let Some(hash) = &self.power2_bit8_source_sha256 {
            state["power2Bit8Echo"] = json!({"register":2,"value":0x0101,"readInput":0,
                "sourceSha256":hash,"synthetic":true,"read2Independent":true,
                "maxRecordedReceipts":256,"receiptOverflow":"count-only","hardwareEffectsModeled":false});
        }
        if let Some(hash) = &self.gpio9_source_sha256 {
            state["gpio9Input"] = json!({"bank":9,"pin":5,"levelHigh":true,"bankValue":0x20,
                "selector":0x90005,"inputId":0x11,"expected":0,"sourceSha256":hash,"synthetic":true,
                "physicalMeaningKnown":false});
        }
        if let Some(hash) = &self.power3_source_sha256 {
            state["power3RawInput"] = json!({"register":3,"value":POWER3_RAW_INPUT,"previousValue":512,
                "rawMask":0x3ff,"sourceSha256":hash,"synthetic":true,"physicalValueVerified":false});
            state["word2OneEchoTrace"] =
                json!({"maxRecordedReceipts":256,"receiptOverflow":"count-only"});
        }
        if let Some(hash) = &self.calibration_reprogram_sha256 {
            state["calibrationReprogramFixture"] = json!({"sourceGuardSha256":reprogram::GUARD_SHA256,
                "sourceGuardBytes":7836,"captureSha256":reprogram::CAPTURE_SHA256,
                "captureSource":"run10 EMULATED RAM; not physical camera data",
                "inputSha256":hash,"configuredUnits":[1,2,3],"slot0Excluded":true,
                "writesPerUnit":730,"eventsPerUnit":3,"latencyTicks":CALIBRATION_TICKS,
                "requiresRemoved":true,"retainsUnwrittenRegisters":true,"synthetic":true,
                "physicalValidityVerified":false,"configuredUnitsAreNotExecutionReceipts":true,
                "scope":if self.calibration_post_control_sha256.is_some(){"single reprogram followed by configured post-control; external gate passive/unmodeled"}else{"single reprogram per configured unit; further writes rejected; external gate separately owned"}});
        }
        if let Some(hash) = &self.calibration_post_control_sha256 {
            state["calibrationPostControlFixture"] = json!({"sourceGuardSha256":hash,
                "sourceGuardBytes":reprogram::POST_CONTROL_GUARD_BYTES,
                "configuredUnits":[1,2,3],"writesPerUnit":8,"eventsPerUnit":4,
                "latencyTicks":CALIBRATION_TICKS,"requiresReprogramDone":true,
                "retainsUnwrittenRegisters":true,"synthetic":true,
                "physicalCausalityVerified":false,"physicalTimingVerified":false,
                "configuredUnitsAreNotExecutionReceipts":true,"afterCompletion":if self.calibration_post_mode2_sha256.is_some(){"one configured mode2 continuation"}else{"reject unknown writes"}});
        }
        if let Some(hash) = &self.calibration_post_mode2_sha256 {
            state["calibrationPostMode2Fixture"] = json!({"sourceGuardSha256":hash,
                "sourceGuardBytes":mode2::POST_CONTROL_GUARD_BYTES,
                "mode2SourceGuardSha256":mode2::GUARD_SHA256,"tablesSha256":mode2::TABLES_SHA256,
                "configuredUnits":[1,2,3],"writesPerUnit":71,"eventsPerUnit":1,
                "latencyTicks":CALIBRATION_TICKS,"requiresPostControlDone":true,
                "retainsUnwrittenRegisters":true,"synthetic":true,"physicalTimingVerified":false,
                "wholeStatusWordZero":true,"completionOffset":0x5384,
                "configuredUnitsAreNotExecutionReceipts":true,"afterCompletion":if self.calibration_selector1_sha256.is_some(){"one configured selector1 continuation"}else{"reject unknown writes"}});
        }
        if let Some(hash) = &self.calibration_mode2_sha256 {
            state["calibrationMode2Fixture"] = json!({"mode":2,"tableCount":4,
                "writesPerUnit":71,"tablesSha256":hash,"sourceGuardSha256":mode2::GUARD_SHA256,
                "sourceGuardBytes":2900,"latencyTicks":CALIBRATION_TICKS,"synthetic":true,
                "physicalTimingVerified":false,"wholeStatusWordZero":true,
                "completionOffset":0x5384,"requiresPreviousMode5Done":true,
                "singleUsePerUnit":true,"afterCompletion":if self.calibration_selector1_sha256.is_some(){"one configured selector1 continuation"}else{"reject unknown writes"}});
            if self.calibration_post_mode2_sha256.is_some() {
                state["calibrationMode2Fixture"]["scope"] = json!(
                    "Mode5Done-origin lifecycle only; post-control-origin lifecycle is declared by calibrationPostMode2Fixture"
                );
            }
        }
        if let Some(hash) = &self.calibration_selector1_sha256 {
            state["calibrationSelector1Fixture"] = json!({"sourceGuardSha256":hash,
                "sourceGuardBytes":mode2::SELECTOR1_GUARD_BYTES,"selector":1,
                "configuredUnits":[0,1,2,3],"writesPerUnit":3,"eventsPerUnit":1,
                "latencyTicks":CALIBRATION_TICKS,"entryStages":["Mode2Done","PostMode2Done"],
                "originRetained":true,"retainsUnwrittenRegisters":true,"synthetic":true,
                "physicalTimingVerified":false,"physicalFunctionKnown":false,
                "wholeStatusWordZero":true,"completionOffset":0x5384,"selector0Supported":true,"alternatingSelectorReentry":true,
                "configuredUnitsAreNotExecutionReceipts":true,"afterCompletion":"reject unknown writes"});
        }
        if self.linked_dma_source_sha256.is_some() {
            state["bisPeer"] = json!({"base":0xfff34000u32,
                "versionPropertyBytes":[255,255],"gpioBank":0,"gpioPin":4,
                "lowIrq":171,"highIrq":155,"synthetic":true,
                "configuration":"refused: unqualified handoff contents",
                "periodic41":"synthetic zero control flags",
                "periodic42":"synthetic zero flags/signed axis samples for requests 0/1",
                "periodic282":"synthetic zero u16 for request scalar 0x8b",
                "modeledCommands":[0,20,41,42,128,131,135,168,169,177,178,255,282]});
        } else if self.spi2_owner_ack {
            state["spi2OwnerAckFixture"] = json!({"base":0xfff34000u32,
                "receiveRequest":4,"transmitRequest":5,"beatTicks":1,
                "expectedBytes":[0,8,32,208,8,16,128,34,0,0,0,0,8,0,160,218],
                "responseBytes":[0,0,0,0,8,0,160,218,0,0,32,200,8,0,128,18],
                "headerCount":4,"maximumBytes":16,"synthetic":true,"physicalPeerCaptured":false,
                "sourceSha256":spi2::OWNER_SOURCE_SHA256,"expectedOwnerHeaderWord":0x20c8,
                "ownerMessageTag":0x199a,"ownerDeliveryProvenByInput":false,
                "emulatedContextSha256":"4dd523d0435353fcf0230739673e15c5504c3858e9a6febb11219b2f8e84e997"});
        } else if self.spi2_second_header {
            state["spi2TwoHeaderFixture"] = json!({"base":0xfff34000u32,
                "receiveRequest":4,"transmitRequest":5,"beatTicks":1,
                "expectedBytes":[0,8,32,208,8,16,128,34],
                "responseBytes":[0,0,0,0,8,0,160,218],
                "headerCount":2,"maximumBytes":8,"synthetic":true,"physicalPeerCaptured":false,
                "sourceSha256":spi2::SECOND_SOURCE_SHA256,"baselineExpectedFields9":[0,131],
                "emulatedContextsSha256":["8fc416ed84e3ba914918dd766324d35b1b6f51ff127a14e7916d04d48943bb6c",
                    "87843efbc231fb333d227dc9d1dd3f5d6e5e76b20945f59b5392e33c4b4929b3"]});
        } else if self.spi2_trace.is_some() {
            state["spi2FirstHeaderFixture"] = json!({"base":0xfff34000u32,
                "receiveRequest":4,"transmitRequest":5,"beatTicks":1,
                "expectedBytes":[0,8,32,208],"responseBytes":[0,0,0,0],
                "singleFrame":true,"synthetic":true,"physicalPeerCaptured":false,
                "sourceSha256":spi2::SOURCE_SHA256,
                "emulatedContextSha256":"8fc416ed84e3ba914918dd766324d35b1b6f51ff127a14e7916d04d48943bb6c",
                "baselineExpectedField9":0});
        }
        state
    }

    pub fn state(&self) -> Value {
        let mut state = self
            .state_template
            .get_or_init(|| self.static_state())
            .clone();
        let cpu = self.board.state(true);
        let guest_halt = self.guest_halt();
        let outputs = self.board.scanout_metadata();
        let frontier_limit = 1024;
        let (frontier_total, frontier) = self.board.frontier_window(frontier_limit);
        let live = json!({
            "quiescent": self.board.quiescent(),
            "cpu": cpu,
            "guestHalt": guest_halt,
            "outputs": outputs,
            "registers": (0..4).map(|core| {
                arm::CORE.iter().map(|&reg| self.board.read_register(core, reg))
                    .collect::<Result<Vec<_>, _>>()
            }).collect::<Result<Vec<_>, _>>().ok(),
            "frontier": frontier.iter().map(|f| json!({
                "first": f.first, "count": f.count, "core": f.cpu, "address": f.address,
                "pc": f.pc, "size": f.size, "write": f.write, "value": f.value, "kind": f.kind,
            })).collect::<Vec<_>>(),
            "interventions": self.board.interventions().iter().map(|i| json!({
                "kind": i.kind, "core": i.cpu, "address": i.address, "pc": i.pc,
                "size": i.size, "before": i.before, "value": i.value,
                "instructions": i.instructions, "outcome": i.outcome, "evidence": i.evidence,
            })).collect::<Vec<_>>(),
        });
        let Value::Object(live) = live else {
            unreachable!()
        };
        state.as_object_mut().unwrap().extend(live);
        if frontier_total > frontier.len() {
            state["frontierView"] = json!({
                "limit": frontier_limit, "totalRows": frontier_total,
                "returnedRows": frontier.len(), "omittedRows": frontier_total - frontier.len(),
                "selection": "first-and-latest-first-seen"
            });
        }
        let buttons = self.virtual_buttons.borrow();
        if !buttons.is_empty() {
            let samples: BTreeMap<_, _> = VIRTUAL_BUTTONS
                .iter()
                .filter(|(id, _, _, _)| buttons.contains_key(id))
                .map(|&(id, _, _, row)| {
                    let address = row + 0x18;
                    let sample = match self.board.read_virtual(0, address, 8) {
                        Ok(bytes) => json!({"address":address,"core":0,"bytes":bytes}),
                        Err(error) => json!({"address":address,"core":0,"unavailable":error}),
                    };
                    (id, sample)
                })
                .collect();
            state["virtualButtons"] = json!({"pressed":&*buttons,"sourceSha256":VIRTUAL_BUTTON_SOURCE_SHA256,
                "samples":samples,"sampleMeaning":"current core0 guest virtual RAM; stable byte is not a delivered UI event",
                "synthetic":true,"physicalWiringVerified":false});
        }
        state
    }

    /// Every unmodeled access since boot, totalled per 4 KiB window and kind.
    pub fn frontier_windows(&self) -> Value {
        let windows = self.board.frontier_windows();
        let accesses: u64 = windows.iter().map(|w| w.reads + w.writes).sum();
        json!({
            "accesses": accesses,
            "windows": windows.iter().map(|w| json!({
                "page": w.page, "kind": w.kind, "reads": w.reads, "writes": w.writes,
                "addresses": w.addresses, "first": w.first, "cpus": w.cpus,
            })).collect::<Vec<_>>(),
        })
    }

    pub fn diagnostics(&self) -> Result<Value, String> {
        let mut barrier = [0; 16];
        self.board.read(0xb06c, &mut barrier)?;
        let barrier_words: Vec<_> = barrier
            .as_chunks::<4>()
            .0
            .iter()
            .map(|bytes| u32::from_le_bytes(*bytes))
            .collect();
        let devices = self
            .board
            .device_state_json()
            .into_iter()
            .map(|(name, data)| {
                serde_json::from_str::<Value>(&data)
                    .map(|state| (name, state))
                    .map_err(|e| e.to_string())
            })
            .collect::<Result<BTreeMap<_, _>, _>>()?;
        let audio_task_state = if self.audio_reset_source_sha256.is_some() {
            let mut state = [0];
            self.board.read(0x1af1840, &mut state)?;
            Some(json!({"address":0x1af1840,"value":state[0]}))
        } else {
            None
        };
        let pc = self.board.read_register(0, arm::PC)?;
        let settings_lookup_frames = if self.power14_source_sha256.is_some()
            && (0x12638d8..=0x1263914).contains(&pc)
        {
            let mut fp = u32::try_from(self.board.read_register(0, arm::CORE[11])?)
                .map_err(|_| "Frame pointer exceeds ARM32")?;
            let mut frames = Vec::new();
            for expected_lr in [Some(0x1263cdc), Some(0x12f22e8), None] {
                let address = fp.checked_sub(12).ok_or("Frame pointer underflow")?;
                let bytes = self.board.read_virtual(0, address, 16)?;
                let words = bytes
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .map(|b| u32::from_le_bytes(*b))
                    .collect::<Vec<_>>();
                if expected_lr.is_some_and(|lr| lr != words[3])
                    || (expected_lr.is_some() && fp.checked_add(16) != Some(words[2]))
                {
                    frames.clear();
                    break;
                }
                frames.push(json!({"fp":fp,"savedFp":words[2],"savedLr":words[3],"r4":words[0],"r5":words[1]}));
                fp = words[2];
            }
            if frames.len() == 3 {
                Some(json!({"core":0,"pc":pc,"frames":frames}))
            } else {
                None
            }
        } else {
            None
        };
        let settings_data = if self.power14_source_sha256.is_some() {
            let read_word = |address| -> Result<u32, String> {
                let bytes = self.board.read_virtual(0, address, 4)?;
                Ok(u32::from_le_bytes(
                    bytes.try_into().map_err(|_| "Settings word width")?,
                ))
            };
            let observed = (|| -> Result<Value, String> {
                let base = read_word(0x17b2764)?;
                let row0 = if base == 0x45e8000 {
                    let byte = |offset| -> Result<u8, String> {
                        Ok(self.board.read_virtual(0, base + offset, 1)?[0])
                    };
                    let index = byte(0x434600)?;
                    let lookup = if index == 255 {
                        None
                    } else {
                        let bytes = self.board.read_virtual(
                            0,
                            base + 0x422530 + u32::from(index) * 2,
                            2,
                        )?;
                        Some(u16::from_le_bytes(
                            bytes.try_into().map_err(|_| "Settings u16 width")?,
                        ))
                    };
                    let magic = self.board.read_virtual(0, base + 0x202, 2)?;
                    Some(json!({"upper":byte(0x42232d)?,"lower":byte(0x4222c8)?,
                        "rowCountRaw":byte(0x420f80)?,"columnLimitRaw":byte(0x420f82)?,
                        "index":index,"lookupValue":lookup,"magic":u16::from_le_bytes(magic.try_into().map_err(|_|"Settings magic width")?)}))
                } else {
                    None
                };
                let table = read_word(0xe20000)?;
                let mut ids = Vec::new();
                let mut object8 = None;
                for index in 0..32u32 {
                    let address = table
                        .checked_add(index * 36)
                        .ok_or("Object directory overflow")?;
                    if address.checked_add(36).is_none_or(|end| end > 0x20000000) {
                        return Err("Object directory outside RAM".into());
                    }
                    let bytes = self.board.read_virtual(0, address, 36)?;
                    let id = bytes[0];
                    ids.push(id);
                    if id == 255 {
                        break;
                    }
                    if id == 8 {
                        object8 = Some(json!({"address":address,
                            "flashOffset":u32::from_le_bytes(bytes[20..24].try_into().unwrap()),
                            "declaredBytes":u32::from_le_bytes(bytes[24..28].try_into().unwrap())}));
                        break;
                    }
                }
                Ok(
                    json!({"core":0,"basePointerCell":0x17b2764,"base":base,"row0Column0":row0,
                    "directory":{"anchorCell":0xe20000,"table":table,"scanLimit":32,"seenIds":ids,"object8":object8}}),
                )
            })();
            Some(match observed {
                Ok(value) => value,
                Err(error) => json!({"unavailable":error}),
            })
        } else {
            None
        };
        let mut state = json!({"devices": devices, "records": self.board.device_records(),
            "settingsData":settings_data,
            "settingsLookupFrames":settings_lookup_frames,
            "audioTaskState":audio_task_state,
            "loaderBarrier": {"address": 0xb06c, "words": barrier_words}});
        if self.power3_source_sha256.is_some() {
            state["spi2ClientObservation"] = observe_spi2_client_ram(&self.board);
        }
        if let Some(trace) = &self.spi2_trace {
            let trace = trace.borrow();
            state["spi2HeaderTrace"] = json!({"records":trace.records,"dropped":trace.dropped,
                "limit":64,"sourceSha256":if self.spi2_owner_ack {spi2::OWNER_SOURCE_SHA256}else if self.spi2_second_header {spi2::SECOND_SOURCE_SHA256}else{spi2::SOURCE_SHA256},
                "meaning":"passive original-PC entry/action-return samples and current virtual RAM"});
        }
        if let Some(trace) = &self.rec_input_trace {
            let trace = trace.borrow();
            if trace.enabled {
                state["recInputTrace"] = trace.value();
            }
            if trace.display.enabled {
                state["recDisplayTrace"] = trace.display.value();
            }
        }
        if self.rec_state_memory_watch_started.get() {
            state["recStateMemoryTrace"] = self.rec_state_memory_trace()?;
        }
        if self.board.ram_write_watch_snapshot().is_some() {
            state["kernelWriteWatch"] = self.kernel_write_watch()?;
        }
        if let Some(trace) = self.board.irq_trace_snapshot() {
            state["kernelIrqTrace"] = trace;
        }
        if self.rtc_control_source_sha256.is_some()
            && (self.packed_resource_input.is_some()
                || self.board.read_register(1, arm::PC)? == 0x11b1670)
        {
            let observed = (|| -> Result<Value, String> {
                let words = |address: u32, count: usize| -> Result<Vec<u32>, String> {
                    let bytes = self.board.read_virtual(1, address, count * 4)?;
                    if bytes.len() != count * 4 {
                        return Err("Record observation width differs".into());
                    }
                    Ok(bytes
                        .as_chunks::<4>()
                        .0
                        .iter()
                        .copied()
                        .map(u32::from_le_bytes)
                        .collect())
                };
                let pointer = words(0x178c2ec, 1)?[0];
                let dividend = words(0x178c2e4, 1)?[0];
                let record = words(0x178c2c8, 5)?;
                let source = if pointer.checked_add(20).is_some_and(|end| end <= 0x20000000) {
                    words(pointer, 5).map(|value| json!(value))
                } else {
                    Err("Record source outside declared RAM".into())
                };
                let frame = (|| -> Result<Value, String> {
                    let fp = u32::try_from(self.board.read_register(1, arm::CORE[11])?)
                        .map_err(|_| "Record FP exceeds ARM32")?;
                    let sp = u32::try_from(self.board.read_register(1, arm::CORE[13])?)
                        .map_err(|_| "Record SP exceeds ARM32")?;
                    if fp.checked_sub(12) != Some(sp) {
                        return Err("Record frame differs from source prologue".into());
                    }
                    let saved = words(sp, 4)?;
                    let source_matched = saved[3] == 0x1264554;
                    let parent = if source_matched
                        && saved[2] > fp
                        && saved[2] - fp <= 0x1000
                        && saved[2] & 3 == 0
                    {
                        let parent_words = words(saved[2] - 4, 2)?;
                        Some(
                            json!({"fp":saved[2],"callerFp":parent_words[0],"savedLr":parent_words[1]}),
                        )
                    } else {
                        None
                    };
                    Ok(
                        json!({"fp":fp,"sp":sp,"savedR4":saved[0],"savedR5":saved[1],
                        "callerFp":saved[2],"savedLr":saved[3],"callbackCallsiteVerified":source_matched,"parent":parent}),
                    )
                })();
                Ok(
                    json!({"core":1,"pc":self.board.read_register(1, arm::PC)?,"pointerCell":0x178c2ec,"sourcePointer":pointer,
                    "dividendAddress":0x178c2e4,"dividend":dividend,"recordAddress":0x178c2c8,
                    "recordWords":record,"divisor":record[3],"remainder":dividend.checked_rem(record[3]),
                    "sourceWords":source.unwrap_or_else(|error|json!({"unavailable":error})),
                    "frame":frame.unwrap_or_else(|error|json!({"unavailable":error}))}),
                )
            })();
            state["recordCallback"] = observed.unwrap_or_else(|error| json!({"unavailable":error}));
        }
        Ok(state)
    }

    /// Bounded, passive RAM access tracing while the real loader executes.
    pub fn observe_loader_barrier(&self) -> Result<Value, String> {
        self.board
            .memory_watch_begin(vec![(0xb05c, 0xb07c)], 4096)?;
        let result = self.execute(100_000);
        let trace =
            serde_json::to_value(self.board.memory_watch_state()).map_err(|e| e.to_string());
        self.board.memory_watch_end()?;
        let mut state = result?;
        state["startupMemoryTrace"] = trace?;
        state["diagnostics"] = self.diagnostics()?;
        state["perCoreBarrierBytes"] = json!(
            (0..4)
                .map(|core| self.board.read_virtual(core, 0xb06c, 16))
                .collect::<Result<Vec<_>, _>>()?
        );
        Ok(state)
    }

    /// Passive, bounded return-value capture for flash and startup-condition checks.
    pub fn observe_main_initialization(&self) -> Result<Value, String> {
        self.observe_main_initialization_budget(if self.inactive_countdown { 80 } else { 40 })
    }

    pub fn observe_a00b_memory(&self, batches: u32) -> Result<Value, String> {
        if self.gpio9_source_sha256.is_none() || !(1..=4096).contains(&batches) {
            return Err(
                "A00B memory observation requires the explicit high scenario and1..=4096 batches"
                    .into(),
            );
        }
        self.board
            .memory_watch_begin(vec![(0x1be6af0, 0x1be6af1)], 4096)?;
        let result = self.observe_main_initialization_budget(batches);
        let trace =
            serde_json::to_value(self.board.memory_watch_state()).map_err(|e| e.to_string());
        self.board.memory_watch_end()?;
        let mut state = result?;
        state["a00bMemoryObservation"] = json!({"softwareFieldAddress":0x1be6af0,
            "limit":4096,"addressMappingVerified":false,
            "dataAddressContract":"backend physical address with an additional board translation; original guest VA is not recorded",
            "writeContract":"pre-store operand and before bytes; not a committed-store receipt",
            "watch":trace?});
        Ok(state)
    }

    pub fn observe_main_initialization_budget(&self, batches: u32) -> Result<Value, String> {
        if self.spi2_trace.is_some() {
            return Err(
                "SPI2 first-header sessions use their dedicated passive trace; use run".into(),
            );
        }
        if !(1..=4096).contains(&batches) {
            return Err("Main-init observation requires1..=4096 batches".into());
        }
        if self.bootstrap_instructions == 0
            || self.board.instructions().iter().sum::<u64>() != self.bootstrap_instructions
        {
            return Err("Main-init observation requires a fresh ordered-startup session".into());
        }
        let records = Rc::new(RefCell::new((Vec::<Value>::new(), 0usize)));
        let captured = records.clone();
        let board = Rc::downgrade(&self.board);
        let class3 = self.class3_source_sha256.is_some();
        let bit8_echo = self.power2_bit8_source_sha256.is_some();
        let gpio9_high = self.gpio9_source_sha256.is_some();
        let trace_limit = if bit8_echo {
            4096
        } else if class3 {
            512
        } else {
            256
        };
        for core in 0..4 {
            let mut points = vec![
                0x11d539c, 0x11d5b24, 0x11dbffc, 0x11daef8, 0x11daf40, 0x11dc13c, 0x11c19f0,
                0x11c1a18, 0x11c1a28, 0x11c1a4c, 0x1291194, 0x11c1914, 0x11c253c, 0x11c33a8,
                0x11c1b20, 0x1291190, 0x1291198, 0x129119c, 0x11c1b38, 0x1280c14, 0x11c1ce4,
                0x11c1fac, 0x11c2c98, 0x11c1e40, 0x1291184, 0x1291170, 0x1275968, 0x126d450,
                0x1256d70, 0x125948c, 0x12595b8, 0x12595d4, 0x125a358, 0x125a35c, 0x11d5544,
                0x1203204, 0x1203708, 0x11d5b74, 0x11d5ce8, 0x11d5cfc, 0x11d5d40, 0x11d5d54,
                0x11d5d60, 0x11d5d74, 0x11d5d80, 0x11d5d94, 0x11d5e20, 0x11d5e40, 0x11d5e60,
                0x11d5e80, 0x11d5ea0, 0x11d5ec0, 0x11d5ed4, 0x11d5b34, 0x11d5b70, 0x11d5f04,
                0x11de210, 0x11de228, 0x11de248, 0x11df61c, 0x11df620,
            ];
            if self.rtc_control_source_sha256.is_some() {
                points.extend([0x11bfd44, 0x11bfb18]);
            }
            if self.packed_resource_input.is_some() {
                points.push(0x11b1674);
            }
            if self.control_request_source_sha256.is_some() {
                points.extend([0x11bfd84, 0x11bfd8c]);
            }
            if self.power64_source_sha256.is_some() {
                points.extend([0x11d5584, 0x11d5598, 0x11d55a4, 0x11d55b8]);
            }
            if self.power2_echo_source_sha256.is_some() {
                points.extend([0x11d61cc, 0x11d61d4, 0x11d61ec, 0x11d61f4]);
            }
            if class3 {
                for (_, _, returned, stored, _) in CLASS3_READS {
                    points.extend([u64::from(returned), u64::from(stored)]);
                }
                points.extend([0x11d4024, 0x11d42e8, 0x11d4300, 0x11d4310]);
                points.extend([0x11d3968, 0x11d39a0]);
            }
            if bit8_echo {
                points.extend([0x11d6500, 0x11d6504]);
                points.extend(UI_MODE_OBSERVATION_POINTS);
                points.extend(PROPERTY9_OBSERVATION_POINTS);
                points.extend(INPUT11_OBSERVATION_POINTS);
            }
            if gpio9_high {
                points.extend(SETUP_REC_OBSERVATION_POINTS);
                points.extend(POWEROFF_OBSERVATION_POINTS);
                points.extend(POWEROFF_EVENT_OBSERVATION_POINTS);
                points.extend(A00B_CLASS_OBSERVATION_POINTS);
                points.push(A00B_COPY_SOURCE_PC);
                points.extend(BATTERY_STATUS_OBSERVATION_POINTS);
                points.extend(BATTERY_CACHE_OBSERVATION_POINTS);
            }
            if core == 0 {
                points.extend([
                    0x11a6e28, 0x11a8620, 0x11a6e4c, 0x11a6f28, 0x11a6f04, 0x1291728, 0x1291730,
                    0x129134c, 0x1291358, 0x1291364, 0x12913c8, 0x12913e4, 0x1291384, 0x12917e4,
                    0x1291818,
                ]);
            }
            self.board.instruction_points(core, points)?;
        }
        self.board
            .set_instruction_observer(Some(Rc::new(move |core, pc| {
                let board = board.upgrade().ok_or("Observed board closed")?;
                let r0 = board.read_register(core, arm::CORE[0])?;
                let r1 = board.read_register(core, arm::CORE[1])?;
                if gpio9_high && pc==0x12944ac && r0!=0x1be4150 {return Ok(());}
                if gpio9_high && pc==0x11de6e8 && board.read_register(core,arm::CORE[4])?!=0x1be4128 {return Ok(());}
                let battery_cache = gpio9_high && BATTERY_CACHE_OBSERVATION_POINTS.contains(&pc);
                if battery_cache && board.read_register(core,arm::CORE[3])?!=0x1790fb8 {return Ok(());}
                let battery_cache_context = if battery_cache {
                    Some((|| ->Result<Value,String> {
                        Ok(json!({"word2CurrentBytes":board.read_virtual(core,0x1790ed4,2)?,
                            "fe7Fe8CurrentBytes":board.read_virtual(core,0x1790fe7,2)?,
                            "counter0CurrentByte":board.read_virtual(core,0x1790fdc,1)?[0],
                            "cpsr":board.read_register(core,arm::CPSR)?}))
                    })().unwrap_or_else(|error|json!({"unavailable":error})))
                } else {None};
                let input11 = bit8_echo && INPUT11_OBSERVATION_POINTS.contains(&pc);
                if input11 && [0x12036a4,0x12036a8].contains(&pc) && board.read_register(core,arm::CORE[4])?!=0x172c314 {return Ok(());}
                if input11 && pc==0x1167e40 && r0!=0x11 {return Ok(());}
                if input11 && pc==0x12b0920 && (board.read_register(core,arm::CORE[6])?!=0x194bfd0 || board.read_register(core,arm::CORE[5])?!=0x1935fb8) {return Ok(());}
                let property9 = bit8_echo && PROPERTY9_OBSERVATION_POINTS.contains(&pc);
                let property9_post = [0x11242b0,0x11242ec].contains(&pc);
                if property9 && (if property9_post {board.read_register(core,arm::CORE[5])?}else{r0})!=9 {return Ok(());}
                let client_set = class3 && [0x11d3968,0x11d39a0].contains(&pc);
                if client_set && r0>2 {return Ok(());}
                if [0x11d61cc,0x11d61d4,0x11d61ec,0x11d61f4].contains(&pc)
                    && board.read_register(core,arm::CORE[4])? != 0x1790ed0 {
                    return Ok(());
                }
                if [0x11bfd84,0x11bfd8c].contains(&pc) && board.read_register(core, arm::CORE[4])? != 0xfffae000 {
                    return Ok(());
                }
                if (pc==0x11bfd44 && r0!=12) || (pc==0x11bfb18 && r0!=0xfffae000) {
                    return Ok(());
                }
                if [0x11d539c, 0x11d5b24].contains(&pc) && ![14, 17, 64, 65, 67, 69, 70, 71].contains(&r0) && !(29..=39).contains(&r0) && !(pc==0x11d5b24 && r0==2) && !(pc==0x11d539c && (r0==47 || (class3 && CLASS3_READS.iter().any(|r|u64::from(r.0)==r0)))) {
                    return Ok(());
                }
                if class3 && pc==0x11d4024 && r0>2 {return Ok(());}
                if pc == 0x126d450
                    && ![0x1291184, 0x1291170].contains(&board.read_register(core, arm::CORE[6])?)
                {
                    return Ok(());
                }
                if pc == 0x1203708 && board.read_register(core, arm::CORE[4])? != 0x172c0f4 {
                    return Ok(());
                }
                let lr = board.read_register(core, arm::CORE[14])?;
                let copy_source = gpio9_high && pc==A00B_COPY_SOURCE_PC;
                if copy_source && u32::try_from(lr).ok().and_then(|address|address.checked_add(8))!=Some(0x1be6af0) {return Ok(());}
                let copy_source_context = if copy_source {
                    Some((|| ->Result<Value,String> {
                        let r11=board.read_register(core,arm::CORE[11])?;
                        let sp=board.read_register(core,arm::CORE[13])?;
                        let r12=board.read_register(core,arm::CORE[12])?;
                        let source=(|| ->Result<Value,String> {
                            let address=u32::try_from(r12).map_err(|_|"Copy source cursor exceeds ARM32")?.checked_sub(8).ok_or("Copy source cursor underflow")?;
                            Ok(json!({"virtualAddress":address,"bytes":board.read_virtual(core,address,8)?}))
                        })().unwrap_or_else(|error|json!({"unavailable":error}));
                        let frame=(|| ->Result<Value,String> {
                            let fp=u32::try_from(r11).map_err(|_|"Copy R11 exceeds ARM32")?;
                            let sp=u32::try_from(sp).map_err(|_|"Copy SP exceeds ARM32")?;
                            if (fp|sp)&3!=0 || sp.checked_add(16)!=Some(fp) {return Err("Copy frame differs from normal source prologue".into());}
                            let address=fp.checked_sub(4).ok_or("Copy saved frame underflow")?;
                            let words=board.read_virtual(core,address,8)?;
                            Ok(json!({"savedCallerR11":u32::from_le_bytes(words[0..4].try_into().map_err(|_|"Copy saved R11 width")?),
                                "savedCallerLR":u32::from_le_bytes(words[4..8].try_into().map_err(|_|"Copy saved LR width")?)}))
                        })().unwrap_or_else(|error|json!({"unavailable":error}));
                        Ok(json!({"destinationVirtual":0x1be6af0,"r11":r11,"r12":r12,
                            "cpsr":board.read_register(core,arm::CPSR)?,"instructions":board.instructions().iter().sum::<u64>(),
                            "loadedWords":[board.read_register(core,arm::CORE[4])?,board.read_register(core,arm::CORE[5])?],
                            "sourceSample":source,"frame":frame}))
                    })().unwrap_or_else(|error|json!({"unavailable":error})))
                } else {None};
                let a00b_class = gpio9_high && A00B_CLASS_OBSERVATION_POINTS.contains(&pc);
                if a00b_class && ((pc==0x1131c04 && r0!=6) || (pc==0x1131c4c && board.read_register(core,arm::CORE[4])?!=6)) {return Ok(());}
                let a00b_class_context = if a00b_class {
                    Some((|| -> Result<Value,String> {
                        let fp=u32::try_from(board.read_register(core,arm::CORE[11])?).map_err(|_|"A00B class FP exceeds ARM32")?;
                        let sp=u32::try_from(board.read_register(core,arm::CORE[13])?).map_err(|_|"A00B class SP exceeds ARM32")?;
                        if (fp|sp)&3!=0 {return Err("A00B class frame is not word-aligned".into());}
                        let post=pc==0x1131c4c;
                        if (post && sp.checked_add(12)!=Some(fp)) || (pc==0x1fa4cdc && sp.checked_add(4)!=Some(fp)) {return Err("A00B class frame differs from source".into());}
                        let mut context=json!({"phase":match pc {0x1fa4cdc=>"registration-before-call",0x1131c04=>"class6-call",_=>"class6-post-store"},
                            "fp":fp,"uiClassAddress":0x1b7a1dd,"uiClassByte":board.read_virtual(core,0x1b7a1dd,1)?[0]});
                        if post {
                            let address=fp.checked_sub(4).ok_or("A00B class saved frame underflow")?;
                            let words=board.read_virtual(core,address,8)?;
                            context["savedCallerFP"]=json!(u32::from_le_bytes(words[0..4].try_into().map_err(|_|"A00B class saved FP width")?));
                            context["savedCallerLR"]=json!(u32::from_le_bytes(words[4..8].try_into().map_err(|_|"A00B class saved LR width")?));
                        }
                        Ok(context)
                    })().unwrap_or_else(|error|json!({"unavailable":error})))
                } else {None};
                let poweroff_event_context = if gpio9_high && POWEROFF_EVENT_OBSERVATION_POINTS.contains(&pc) {
                    Some((|| -> Result<Value,String> {
                        let fp=u32::try_from(board.read_register(core,arm::CORE[11])?).map_err(|_|"Poweroff event FP exceeds ARM32")?;
                        let sp=u32::try_from(board.read_register(core,arm::CORE[13])?).map_err(|_|"Poweroff event SP exceeds ARM32")?;
                        if (fp|sp)&3!=0 || sp.checked_add(12)!=Some(fp) {return Err("Poweroff event handler frame differs from source".into());}
                        let mut context=json!({"phase":if pc==0x1e28480 {"raw-message-selected"}else{"local-event-dispatch"},"fp":fp});
                        if pc==0x1e28480 {
                            context["matchedRawR3"]=json!(board.read_register(core,arm::CORE[3])?);
                        } else {
                            let halfword=|address| ->Result<u16,String> {
                                Ok(u16::from_le_bytes(board.read_virtual(core,address,2)?.try_into().map_err(|_|"Poweroff event halfword width")?))
                            };
                            context["localEventAddress"]=json!(0x2d8c0e0);context["localEvent"]=json!(halfword(0x2d8c0e0)?);
                            context["stateAddress"]=json!(0x2d167a4);context["state"]=json!(halfword(0x2d167a4)?);
                        }
                        Ok(context)
                    })().unwrap_or_else(|error|json!({"unavailable":error})))
                } else {None};
                let poweroff = gpio9_high && POWEROFF_OBSERVATION_POINTS.contains(&pc);
                if poweroff && (if pc==0x2016854 {r0}else{board.read_register(core,arm::CORE[4])?})!=20 {return Ok(());}
                let poweroff_context = if poweroff {
                    Some((|| -> Result<Value,String> {
                        let fp=u32::try_from(board.read_register(core,arm::CORE[11])?).map_err(|_|"Poweroff FP exceeds ARM32")?;
                        let sp=u32::try_from(board.read_register(core,arm::CORE[13])?).map_err(|_|"Poweroff SP exceeds ARM32")?;
                        if (fp|sp)&3!=0 {return Err("Poweroff frame is not word-aligned".into());}
                        let post=pc==0x2016ea0;
                        if post && sp.checked_add(12)!=Some(fp) {return Err("Poweroff callee frame differs from source".into());}
                        let mut context=json!({"phase":if post {"app-post-store"}else{"app-action-entry"},
                            "action":20,"fp":fp,"modeAddress":0x1b79eb8,"modeBytes":board.read_virtual(core,0x1b79eb8,4)?});
                        if post {
                            let address=fp.checked_sub(4).ok_or("Poweroff saved frame underflow")?;
                            let words=board.read_virtual(core,address,8)?;
                            context["savedCallerFP"]=json!(u32::from_le_bytes(words[0..4].try_into().map_err(|_|"Poweroff saved FP width")?));
                            context["savedCallerLR"]=json!(u32::from_le_bytes(words[4..8].try_into().map_err(|_|"Poweroff saved LR width")?));
                        }
                        Ok(context)
                    })().unwrap_or_else(|error|json!({"unavailable":error})))
                } else {None};
                let setup_rec_context = if gpio9_high && SETUP_REC_OBSERVATION_POINTS.contains(&pc) {
                    Some((|| -> Result<Value,String> {
                        let fp=board.read_register(core,arm::CORE[11])?;
                        let sp=board.read_register(core,arm::CORE[13])?;
                        if (fp|sp)&3!=0 || sp.checked_add(12)!=Some(fp) {return Err("Setup caller frame differs from source".into());}
                        Ok(json!({"phase":match pc {0x1131e48=>"ui-initial-state",0x1131f40=>"interval-call",_=>"interval-return"},
                            "fp":fp,"originalR3":board.read_register(core,arm::CORE[3])?,
                            "uiStateAddress":0x1b7a1de,"uiStateByte":board.read_virtual(core,0x1b7a1de,1)?[0],
                            "profileByteAddress":0x1aca4a8,"profileByte":board.read_virtual(core,0x1aca4a8,1)?[0],"profileMask":0x20}))
                    })().unwrap_or_else(|error|json!({"unavailable":error})))
                } else {None};
                if input11 && pc==0x12b0a10 && lr!=0x1167e70 {return Ok(());}
                let input11_context = if input11 {
                    Some((|| -> Result<Value,String> {
                        let word=|address| ->Result<u32,String> {
                            Ok(u32::from_le_bytes(board.read_virtual(core,address,4)?.try_into().map_err(|_|"Input11 word width")?))
                        };
                        let fp=u32::try_from(board.read_register(core,arm::CORE[11])?).map_err(|_|"Input11 FP exceeds ARM32")?;
                        let sp=u32::try_from(board.read_register(core,arm::CORE[13])?).map_err(|_|"Input11 SP exceeds ARM32")?;
                        let mut context=json!({"fp":fp});
                        if [0x12036a4,0x12036a8].contains(&pc) {
                            if (fp|sp)&3!=0 || sp.checked_add(12)!=Some(fp) {return Err("Input11 GPIO frame differs from source".into());}
                            context["phase"]=json!(if pc==0x12036a4 {"gpio-call"}else{"gpio-return"});
                            context["rowAddress"]=json!(0x172c314);
                            context["expectedR5"]=json!(board.read_register(core,arm::CORE[5])?);
                            if pc==0x12036a4 {context["selectorArgument"]=json!(r0);}else{
                                context["selectorCurrent"]=json!(word(0x172c320)?);
                                context["expectedCurrent"]=json!(word(0x172c328)?);
                            }
                        } else if pc==0x1167e40 {
                            context["phase"]=json!("callback");context["stateArgument"]=json!(r1);
                            if lr==0x1203a14 && ((fp|sp)&3!=0 || sp.checked_add(12)!=Some(fp)) {return Err("Input11 dispatcher frame differs from source".into());}
                            context["dispatcherFrameVerified"]=json!(lr==0x1203a14);
                        } else if pc==0x12b0a10 {
                            if (fp|sp)&3!=0 || fp.checked_sub(60)!=Some(sp) || r0!=u64::from(sp) {return Err("Input11 append frame/source differs from source".into());}
                            context["phase"]=json!("pending-append");context["sourceAddress"]=json!(r0);
                            context["sourceWords"]=json!(board.read_virtual(core,sp,8)?.as_chunks::<4>().0.iter().map(|b|u32::from_le_bytes(*b)).collect::<Vec<_>>());
                            context["pendingCount"]=json!(word(0x1936db8)?);
                        } else {
                            if (fp|sp)&3!=0 || sp.checked_add(20)!=Some(fp) {return Err("Input11 drain frame differs from source".into());}
                            let source_count=word(0x1936db8)?;let destination_count=word(0x194cdd0)?;
                            let valid=(12..=64).contains(&source_count) && (12..=64).contains(&destination_count);
                            context["phase"]=json!("pending-drain");context["sourceCount"]=json!(source_count);
                            context["destinationCount"]=json!(destination_count);context["row11Valid"]=json!(valid);
                            if valid {context["sourceRow11"]=json!(word(0x1936220)?);context["destinationRow11"]=json!(word(0x194c238)?);}
                        }
                        Ok(context)
                    })().unwrap_or_else(|error|json!({"unavailable":error})))
                } else {None};
                let property9_context = if property9 {
                    Some((|| -> Result<Value, String> {
                        let word=|address| -> Result<u32,String> {
                            Ok(u32::from_le_bytes(board.read_virtual(core,address,4)?.try_into().map_err(|_|"Property9 word width")?))
                        };
                        let fp=u32::try_from(board.read_register(core,arm::CORE[11])?).map_err(|_|"Property9 FP exceeds ARM32")?;
                        let sp=u32::try_from(board.read_register(core,arm::CORE[13])?).map_err(|_|"Property9 SP exceeds ARM32")?;
                        if (fp|sp)&3!=0 {return Err("Property9 frame is not word-aligned".into());}
                        let mut context=json!({"id":9,"postStore":property9_post,"set":pc==0x1124278 || pc==0x11242b0,
                            "wordAddress":0x1b79710,"word":word(0x1b79710)?,"mask":0x200,"fp":fp});
                        if property9_post {
                            if sp.checked_add(12)!=Some(fp) {return Err("Property9 callee frame differs from source".into());}
                            context["savedCallerWords"]=json!(board.read_virtual(core,sp,16)?.as_chunks::<4>().0.iter().map(|b|u32::from_le_bytes(*b)).collect::<Vec<_>>());
                        } else if [0x1133b5c,0x1133bd0].contains(&lr) {
                            if fp.checked_sub(36)!=Some(sp) {return Err("Property9 init frame differs from source".into());}
                            let index=u32::try_from(board.read_register(core,arm::CORE[5])?).map_err(|_|"Property9 index exceeds ARM32")?;
                            let source=board.read_register(core,arm::CORE[6])?;
                            let count=word(0x194cdd0)?;
                            if source!=0x194bfd0 || index>=64 || index>=count || count>64 {return Err("Property9 init record outside source-qualified bounds".into());}
                            let address=0x194bfd0+index*56;
                            context["producer"]=json!({"kind":"initialization","source":source,"index":index,"count":count,
                                "recordAddress":address,"recordWord":word(address)?,"flagsR9":board.read_register(core,arm::CORE[9])?});
                        } else if [0x2018e4c,0x2018e58].contains(&lr) {
                            if fp.checked_sub(124)!=Some(sp) {return Err("Property9 event frame differs from source".into());}
                            context["producer"]=json!({"kind":"event","idR4":board.read_register(core,arm::CORE[4])?,"flagsR5":board.read_register(core,arm::CORE[5])?});
                        }
                        if !property9_post && [0x1133b5c,0x1133bd0,0x2018e4c,0x2018e58].contains(&lr) {
                            let address=fp.checked_sub(4).ok_or("Property9 caller frame underflow")?;
                            context["parentFrameWords"]=json!(board.read_virtual(core,address,8)?.as_chunks::<4>().0.iter().map(|b|u32::from_le_bytes(*b)).collect::<Vec<_>>());
                        }
                        Ok(context)
                    })().unwrap_or_else(|error|json!({"unavailable":error})))
                } else {None};
                let ui_mode_context = if bit8_echo && UI_MODE_OBSERVATION_POINTS.contains(&pc) {
                    Some((|| -> Result<Value, String> {
                        Ok(json!({"modeAddress":0x1b79eb8,"modeBytes":board.read_virtual(core,0x1b79eb8,4)?,
                            "featureAddress":0x1bb18f1,"featureByte":board.read_virtual(core,0x1bb18f1,1)?[0],
                            "postCommit":pc==0x1141b34,"fp":board.read_register(core,arm::CORE[11])?}))
                    })().unwrap_or_else(|error|json!({"unavailable":error})))
                } else {None};
                let rewrite_context=if bit8_echo && [0x11d6500,0x11d6504].contains(&pc) {
                    Some((|| ->Result<Value,String> {
                        let mirror=board.read_virtual(core,0x1790ed4,2)?;
                        let errors=board.read_virtual(core,0x1790ec0,4)?;
                        Ok(json!({"postReturn":pc==0x11d6504,"mirrorAddress":0x1790ed4,"mirrorWord":u16::from_le_bytes(mirror.try_into().map_err(|_|"Rewrite mirror width")?),
                            "policyAddress":0x172bacc,"policyBytes":board.read_virtual(core,0x172bacc,6)?,
                            "errorCounterAddress":0x1790ec0,"errorCounter":u32::from_le_bytes(errors.try_into().map_err(|_|"Rewrite counter width")?),
                            "fp":board.read_register(core,arm::CORE[11])?}))
                    })().unwrap_or_else(|error|json!({"unavailable":error})))
                }else{None};
                let numeric_client=if class3 && [0x11d4024,0x11d42e8,0x11d4300,0x11d4310].contains(&pc) {
                    Some(if pc==0x11d4024 {r0}else{board.read_register(core,arm::CORE[7])?})
                }else{None};
                if numeric_client.is_some_and(|client|client>2) {return Ok(());}
                let (records, dropped) = &mut *captured.borrow_mut();
                if let Some(ref context)=copy_source_context && records.iter().any(|r|
                    r["pc"]==pc && r["core"]==core && r["r0"]==r0 && r["lr"]==lr && r["copySourceContext"]==*context) {return Ok(());}
                if let Some(ref context)=a00b_class_context && records.iter().any(|r|
                    r["pc"]==pc && r["core"]==core && r["r0"]==r0 && r["lr"]==lr && r["a00bClassContext"]==*context) {return Ok(());}
                if let Some(ref context)=poweroff_event_context && records.iter().any(|r|
                    r["pc"]==pc && r["core"]==core && r["r0"]==r0 && r["lr"]==lr && r["poweroffEventContext"]==*context) {return Ok(());}
                if let Some(ref context)=poweroff_context && records.iter().any(|r|
                    r["pc"]==pc && r["core"]==core && r["r0"]==r0 && r["lr"]==lr && r["poweroffContext"]==*context) {return Ok(());}
                if let Some(ref context)=setup_rec_context && records.iter().any(|r|
                    r["pc"]==pc && r["core"]==core && r["r0"]==r0 && r["lr"]==lr && r["setupRecContext"]==*context) {return Ok(());}
                if let Some(ref context)=input11_context && records.iter().any(|r|
                    r["pc"]==pc && r["core"]==core && r["r0"]==r0 && r["lr"]==lr && r["input11Context"]==*context) {return Ok(());}
                if let Some(ref context)=property9_context && records.iter().any(|r|
                    r["pc"]==pc && r["core"]==core && r["r0"]==r0 && r["lr"]==lr && r["property9Context"]==*context) {return Ok(());}
                if let Some(ref context)=ui_mode_context && records.iter().any(|r|
                    r["pc"]==pc && r["core"]==core && r["r0"]==r0 && r["lr"]==lr && r["uiModeContext"]==*context) {return Ok(());}
                if let Some(ref context)=rewrite_context && records.iter().any(|r|r["pc"]==pc && r["core"]==core && r["r0"]==r0 && (pc!=0x11d6500 || r["r1"]==r1) && r["power2RewriteContext"]==*context) {return Ok(());}
                if client_set && records.iter().any(|r|r["pc"]==pc && r["core"]==core && r["r0"]==r0 && r["r1"]==r1 && r["lr"]==lr) {return Ok(());}
                if ((pc==0x11d539c && [14,47,64,65].contains(&r0)) || [0x1203204,0x1203708].contains(&pc)) && records.iter().any(|r|
                    r["pc"]==pc && r["r0"]==r0 && r["core"]==core && r["lr"]==lr) {
                    return Ok(());
                }
                if class3 && ((pc==0x11d539c && CLASS3_READS.iter().any(|r|u64::from(r.0)==r0))
                    || CLASS3_READS.iter().any(|r|u64::from(r.2)==pc || u64::from(r.3)==pc) || [0x11d4024,0x11d42e8,0x11d4300,0x11d4310].contains(&pc))
                    && records.iter().any(|r|r["pc"]==pc && r["core"]==core && (pc!=0x11d539c && pc!=0x11d4024 || r["r0"]==r0) && (pc!=0x11d539c || r["lr"]==lr) && (numeric_client.is_none() || r["class3NumericClient"]==json!(numeric_client))) {return Ok(());}
                if [0x11d5584,0x11d5598,0x11d55a4,0x11d55b8].contains(&pc)
                    && records.iter().any(|r|r["pc"]==pc && r["core"]==core) {
                    return Ok(());
                }
                if [0x11d61cc,0x11d61d4,0x11d61ec,0x11d61f4].contains(&pc)
                    && records.iter().any(|r|r["pc"]==pc && r["core"]==core) {
                    return Ok(());
                }
                if pc == 0x12595b8 && records.iter().any(|r|r["pc"]==pc && r["r0"]==r0 && r["core"]==core) {
                    return Ok(());
                }
                if [0x1291730, 0x1291818].contains(&pc)
                    && records.last().is_some_and(|record| record["pc"] == pc)
                {
                    return Ok(());
                }
                if records.len() == trace_limit {
                    *dropped += 1;
                    return Ok(());
                }
                let mut handoff = [0; 4];
                board.read(0x830a8, &mut handoff)?;
                let mut boot_flags = [0; 4];
                board.read(0x82c1c, &mut boot_flags)?;
                let mut gpio_flags = [0; 4];
                board.read(0x82adc, &mut gpio_flags)?;
                let calendar = if pc == 0x11daf40 {
                    Some(board.read_virtual(core, u32::try_from(r0).map_err(|_| "Calendar pointer exceeds ARM32")?, 6)?)
                } else { None };
                let manager = if [0x11c1b38, 0x129119c, 0x11c1fac, 0x11c1e40].contains(&pc) {
                    let mut bytes = [0; 0x84];
                    board.read(0x178ff10, &mut bytes)?;
                    let word = |at:usize| u32::from_le_bytes(bytes[at..at+4].try_into().unwrap());
                    let saved = (0..4).map(|unit| {
                        let primary_words: BTreeMap<_,_> = ["4660","4664","4480","4484","4488","448c"]
                            .into_iter().enumerate().map(|(i,offset)|(offset,word(0x24+unit*24+i*4))).collect();
                        json!({"unit":unit,"primaryWords":primary_words})
                    }).collect::<Vec<_>>();
                    Some(json!({"profile":bytes[8],"command":bytes[9],"activeMask":word(12),
                        "demands":(0..5).map(|i|word(0x10+i*4)).collect::<Vec<_>>(),
                        "saved":saved}))
                } else { None };
                let power14 = if [0x1203204,0x1203708].contains(&pc) {
                    let read_word = |address| -> Result<u32,String> {
                        let mut bytes=[0;4]; board.read(address,&mut bytes)?;
                        Ok(u32::from_le_bytes(bytes))
                    };
                    let input = if pc == 0x1203204 {
                        let frame = board.read_register(core,arm::CORE[11])?;
                        let address=u32::try_from(frame.checked_sub(8).ok_or("Power14 frame underflow")?).map_err(|_|"Power14 frame exceeds ARM32")?;
                        let bytes=board.read_virtual(core,address,2)?;
                        Some(u16::from_le_bytes(bytes.try_into().map_err(|_|"Power14 input width")?))
                    } else { None };
                    Some(json!({"returnedInput":input,"gpioDerived":read_word(0x1791bac)?,"bit5Flag":read_word(0x1791d6c)?}))
                } else {None};
                let mut record = json!({"core":core, "pc":pc, "tick":board.now(),
                "kind": match pc { 0x11d539c => "power-read", 0x11d5b24 => "power-write", 0x126d450 => "worker-return", _ => "main-init" },
                "r0":r0,
                "r1":r1,
                "r2":board.read_register(core, arm::CORE[2])?,
                "r3":board.read_register(core, arm::CORE[3])?,
                "r4":board.read_register(core, arm::CORE[4])?,
                "r5":board.read_register(core, arm::CORE[5])?,
                "r6":board.read_register(core, arm::CORE[6])?,
                "lr":lr,
                "sp":board.read_register(core, arm::CORE[13])?,
                "power14":power14,
                "calendar":calendar,
                "manager":manager,
                "handoffJedec":u32::from_le_bytes(handoff),
                "bootFlags":u32::from_le_bytes(boot_flags),
                "gpioFlags":u32::from_le_bytes(gpio_flags)});
                if let Some(client)=numeric_client {record["class3NumericClient"]=json!(client);}
                if let Some(context)=ui_mode_context {record["uiModeContext"]=context;}
                if let Some(context)=property9_context {record["property9Context"]=context;}
                if let Some(context)=input11_context {record["input11Context"]=context;}
                if let Some(context)=setup_rec_context {record["setupRecContext"]=context;}
                if let Some(context)=poweroff_context {record["poweroffContext"]=context;}
                if let Some(context)=poweroff_event_context {record["poweroffEventContext"]=context;}
                if let Some(context)=a00b_class_context {record["a00bClassContext"]=context;}
                if let Some(context)=copy_source_context {record["copySourceContext"]=context;}
                if let Some(context)=battery_cache_context {record["batteryCacheContext"]=context;}
                if let Some(context)=rewrite_context {record["power2RewriteContext"]=context;}
                if client_set {
                    let context=(|| ->Result<Value,String> {
                        let mirrors=board.read_virtual(core,0x1790ed4,4)?;
                        let base=board.read_virtual(core,0x17b2764,4)?;
                        let base=u32::from_le_bytes(base.try_into().map_err(|_|"Client settings base width")?);
                        if base!=0x45e8000 {return Err("Client settings base differs from source-backed layout".into());}
                        let frame=if pc==0x11d3968 && [0x11d484c,0x11d48a8,0x11d48dc,0x11d48fc].contains(&lr) {
                            let fp=u32::try_from(board.read_register(core,arm::CORE[11])?).map_err(|_|"Client caller FP exceeds ARM32")?;
                            let sp=u32::try_from(board.read_register(core,arm::CORE[13])?).map_err(|_|"Client caller SP exceeds ARM32")?;
                            if fp.checked_sub(44)!=Some(sp) {return Err("Client caller frame differs from source prologue".into());}
                            let locals=board.read_virtual(core,sp,8)?;
                            Some(json!({"fp":fp,"sp":sp,"averagedLocal":u32::from_le_bytes(locals[0..4].try_into().map_err(|_|"Client mean width")?),
                                "thresholdLocal":u32::from_le_bytes(locals[4..8].try_into().map_err(|_|"Client threshold width")?)}))
                        }else{None};
                        Ok(json!({"postStore":pc==0x11d39a0,"mirrorAddress":0x1790ed4,"mirrorWord":u16::from_le_bytes([mirrors[0],mirrors[1]]),
                            "rawMirror3":u16::from_le_bytes([mirrors[2],mirrors[3]]),"selector64Address":0x1790f32,"selector64Low":board.read_virtual(core,0x1790f32,1)?[0],
                            "stateAddress":0x1790fdc,"stateBytes":board.read_virtual(core,0x1790fdc,0x48)?,
                            "settingsBaseCell":0x17b2764,"settingsBase":base,"thresholdSettingId":0x388,"thresholdSettingByte":board.read_virtual(core,base+0x388,1)?[0],
                            "r7ToR10":(7..=10).map(|index|board.read_register(core,arm::CORE[index])).collect::<Result<Vec<_>,_>>()?,"counterCallerFrame":frame}))
                    })();
                    record["power2ClientContext"]=context.unwrap_or_else(|error|json!({"unavailable":error}));
                }
                if (pc==0x11d539c && r0==47) || (class3 && [0x11d4024,0x11d42e8,0x11d4300,0x11d4310].contains(&pc)) {
                    let context=(|| ->Result<Value,String> {
                        let bytes=board.read_virtual(core,0x17b2764,4)?;
                        let base=u32::from_le_bytes(bytes.try_into().map_err(|_|"Class3 settings base width")?);
                        if base!=0x45e8000 {return Err("Class3 settings base differs from source-backed layout".into());}
                        let mut settings=BTreeMap::new();
                        for id in [0x3b9u32,0x3ba,0x3f6,0x3f7,0x3f8,0x3f9,0x4176,0x4177,0x4178] {
                            settings.insert(format!("{id:04x}"),board.read_virtual(core,base+id,1)?[0]);
                        }
                        let derived=board.read_virtual(core,0x1790f70,12)?;
                        let flags=board.read_virtual(core,0x1790ec8,7)?;
                        Ok(json!({"settingsBaseCell":0x17b2764,"settingsBase":base,"settingsBytes":settings,
                            "clientValidBytes":[flags[0],flags[1],flags[2]],"overrideFlag":flags[6],
                            "derivedSignedWords":derived.as_chunks::<4>().0.iter().map(|b|i32::from_le_bytes(*b)).collect::<Vec<_>>(),
                            "policyAddress":0x172bac0+47*6,"policyBytes":board.read_virtual(core,0x172bac0+47*6,6)?,
                            "mirrorAddress":0x1790f22,"mirrorBytes":board.read_virtual(core,0x1790f22,2)?}))
                    })();
                    record["powerClass3Context"]=context.unwrap_or_else(|error|json!({"unavailable":error}));
                }
                if (pc==0x11d5b24 && r0==2) || [0x11d61cc,0x11d61d4,0x11d61ec,0x11d61f4].contains(&pc) {
                    let context=(|| ->Result<Value,String> {
                        let word=|address| ->Result<u16,String> {
                            let bytes=board.read_virtual(core,address,2)?;
                            Ok(u16::from_le_bytes(bytes.try_into().map_err(|_|"Write2 context width")?))
                        };
                        Ok(json!({"mirrorAddress":0x1790ed4,"mirrorWord":word(0x1790ed4)?,
                            "cached14Address":0x1790ee4,"cached14Word":word(0x1790ee4)?,
                            "skipAddress":0x179106d,"skipByte":board.read_virtual(core,0x179106d,1)?[0]}))
                    })();
                    record["power2WriteContext"]=context.unwrap_or_else(|error|json!({"unavailable":error}));
                }
                if class3 && let Some(&(register,_,returned,_,mirror))=CLASS3_READS.iter().find(|r|u64::from(r.2)==pc || u64::from(r.3)==pc) {
                        let observed=(|| ->Result<Value,String> {
                            let fp=u32::try_from(board.read_register(core,arm::CORE[11])?).map_err(|_|"Class3 FP exceeds ARM32")?;
                            let address=if pc==u64::from(returned) {fp.checked_sub(6).ok_or("Class3 local underflow")?}else{mirror};
                            let bytes=board.read_virtual(core,address,2)?;
                            Ok(json!({"register":register,"address":address,"value":u16::from_le_bytes(bytes.try_into().map_err(|_|"Class3 width")?),
                                "postStore":pc!=u64::from(returned),"policyAddress":0x172bac0+u32::from(register)*6,
                                "policyBytes":board.read_virtual(core,0x172bac0+u32::from(register)*6,6)?,
                                "dataAddress":0x1790f68,"dataBytes":board.read_virtual(core,0x1790f68,56)?}))
                        })();
                        record["class3ReadResult"]=observed.unwrap_or_else(|error|json!({"unavailable":error}));
                }
                if [0x11d5584,0x11d55a4].contains(&pc) {
                    let fp=u32::try_from(board.read_register(core,arm::CORE[11])?).map_err(|_|"Refresh FP exceeds ARM32")?;
                    let address=fp.checked_sub(6).ok_or("Refresh local address underflow")?;
                    let bytes=board.read_virtual(core,address,2)?;
                    record["powerRefreshReturnedInput"]=json!({"address":address,
                        "value":u16::from_le_bytes(bytes.try_into().map_err(|_|"Refresh local width")?)});
                }
                if [0x11d5598,0x11d55b8].contains(&pc) {
                    let register=if pc==0x11d5598 {64u32}else{65};
                    let address=0x1790f2c+(register-64)*2;
                    let bytes=board.read_virtual(core,address,2)?;
                    let event=board.read_virtual(core,0x1790eb8,2)?;
                    record["powerRefreshCache"]=json!({"register":register,"address":address,
                        "value":u16::from_le_bytes(bytes.try_into().map_err(|_|"Refresh cache width")?),
                        "policyAddress":0x172bac0+register*6,
                        "policyBytes":board.read_virtual(core,0x172bac0+register*6,6)?,
                        "event65Word":u16::from_le_bytes(event.try_into().map_err(|_|"Event65 width")?),
                        "event65Consumed":board.read_virtual(core,0x1790eb6,1)?[0]});
                }
                if pc == 0x11d5cfc {
                    let bytes = board.read_virtual(core, 0x1790f32, 2)?;
                    record["power67Cached"] = json!(u16::from_le_bytes(bytes.try_into().map_err(|_|"Power67 cache width")?));
                }
                if pc == 0x11d5d54 {
                    let bytes = board.read_virtual(core, 0x1790ee8, 2)?;
                    record["power17Cached"] = json!(u16::from_le_bytes(bytes.try_into().map_err(|_|"Power17 cache width")?));
                }
                if pc == 0x11d5d94 {
                    let bytes = board.read_virtual(core, 0x1790f02, 4)?;
                    record["rtcStagingCached"] = json!({
                        "low":u16::from_le_bytes([bytes[0],bytes[1]]),
                        "high":u16::from_le_bytes([bytes[2],bytes[3]])
                    });
                }
                if pc == 0x11d5ed4 {
                    let mut cached = BTreeMap::new();
                    for (register, address) in [(37,0x1790f12),(38,0x1790f14),(39,0x1790f16),
                        (69,0x1790f36),(70,0x1790f38),(71,0x1790f3a)] {
                        let bytes = board.read_virtual(core, address, 2)?;
                        cached.insert(register,u16::from_le_bytes(bytes.try_into().map_err(|_|"Power initialization cache width")?));
                    }
                    record["powerInitCached"] = json!(cached);
                }
                if pc == 0x11df61c {
                    let request = (|| -> Result<Value,String> {
                        if r0 != 0x1582c92 || record["r1"] != 3 {
                            return Err("I2C arguments differ from the source-qualified three-byte call".into());
                        }
                        let address = u32::try_from(board.read_register(core,arm::CORE[2])?)
                            .map_err(|_|"I2C buffer exceeds ARM32")?;
                        Ok(json!({"descriptorAddress":r0,
                            "descriptorBytes":board.read_virtual(core,0x1582c92,3)?,
                            "payload":board.read_virtual(core,address,3)?,"length":3}))
                    })();
                    record["i2cRequest"] = request.unwrap_or_else(|error|json!({"unavailable":error}));
                }
                records.push(record);
                Ok(())
            })));
        let result = (|| {
            let mut state = self.state();
            for batch in 1..=batches {
                state = self.execute(1_000_000)?;
                eprintln!(
                    "main-init trace batch{batch}: {} instructions",
                    state["cpu"]["instructions"]
                );
                if !state["guestHalt"].is_null()
                    || !state["cpu"]["failure"].is_null()
                    || !state["error"].is_null()
                {
                    break;
                }
            }
            Ok::<_, String>(state)
        })();
        self.board.set_instruction_observer(None);
        for core in 0..4 {
            self.board.instruction_points(core, vec![])?;
        }
        let mut state = result?;
        let (records, dropped) = &*records.borrow();
        state["mainInitTrace"] = json!({"records": records, "dropped": dropped,
            "limit":trace_limit,
            "repeatedFatalLoopEntriesOmitted": true});
        state["diagnostics"] = self.diagnostics()?;
        Ok(state)
    }

    /// A short passive window at the verified X-T4 read32 helper, after a long run.
    pub fn observe_mmio_helper_read(&self, address: u32) -> Result<Value, String> {
        if self.spi2_trace.is_some() {
            return Err(
                "SPI2 first-header sessions use their dedicated passive trace; use run".into(),
            );
        }
        if address & 3 != 0 {
            return Err("Read32 observation requires an aligned address".into());
        }
        let records = Rc::new(RefCell::new((Vec::<Value>::new(), 0usize)));
        let captured = records.clone();
        let board = Rc::downgrade(&self.board);
        for core in 0..4 {
            self.board.instruction_points(core, vec![0x5aa40])?;
        }
        self.board.set_instruction_observer(Some(Rc::new(move |core, _| {
            let board = board.upgrade().ok_or("Observed board closed")?;
            if board.read_register(core, arm::CORE[0])? != u64::from(address) {
                return Ok(());
            }
            let lr = board.read_register(core, arm::CORE[14])?;
            let (records, dropped) = &mut *captured.borrow_mut();
            if records.iter().any(|r| r["core"] == core && r["lr"] == lr) {
                return Ok(());
            }
            if records.len() == 16 {
                *dropped += 1;
                return Ok(());
            }
            records.push(json!({"core":core,"pc":0x5aa40,"address":address,"lr":lr,
                "tick":board.now(),"r4":board.read_register(core,arm::CORE[4])?,
                "r5":board.read_register(core,arm::CORE[5])?,"r6":board.read_register(core,arm::CORE[6])?}));
            Ok(())
        })));
        let result = self.execute(1_000_000);
        self.board.set_instruction_observer(None);
        for core in 0..4 {
            self.board.instruction_points(core, vec![])?;
        }
        let mut state = result?;
        let (records, dropped) = &*records.borrow();
        state["readCallTrace"] = json!({"address":address,"wrapperPc":0x5aa40,"records":records,
            "dropped":dropped,"instructionBudget":1_000_000,"limit":16});
        state["diagnostics"] = self.diagnostics()?;
        Ok(state)
    }

    pub fn set_gpio(&self, port: usize, pin: usize, high: bool) -> Result<(), String> {
        self.board.set_gpio_input(port, pin, high)
    }

    pub fn set_adc(&self, channel: usize, value: u16) -> Result<(), String> {
        self.board.set_adc_input(channel, value)
    }

    fn guest_halt(&self) -> Option<&'static str> {
        [
            (0x1291730, "Main flash initialization rejected at0x01291730"),
            (
                0x1291818,
                "Main startup condition selected the stop path at0x01291818",
            ),
        ]
        .into_iter()
        .find_map(|(pc, message)| self.board.milestone_reached(0, pc).then_some(message))
    }

    pub fn run_state(&self) -> Execution {
        Execution {
            cpu: self.board.progress(),
            quiescent: self.board.quiescent(),
            guest_halt: self.guest_halt(),
            outputs: self.board.scanout_metadata(),
            error: None,
        }
    }

    pub fn execute_batch(&self, budget: u32) -> Result<Execution, String> {
        if budget == 0 || budget > 1_000_000 {
            return Err("Instruction budget must be 1..1,000,000".into());
        }
        // Preserve the stopped CPU state even when execution reaches an unsupported device.
        let result = self.board.run_board(u64::from(budget), 1024);
        let mut state = self.run_state();
        state.error = result.err();
        Ok(state)
    }

    pub fn execute(&self, budget: u32) -> Result<Value, String> {
        let result = self.execute_batch(budget)?;
        let mut state = self.state();
        state["error"] = json!(result.error);
        Ok(state)
    }
}

impl Xp4Session {
    pub fn frame_rgba(&self, output: u32) -> Result<Vec<u8>, String> {
        self.board
            .scanout_frames()
            .into_iter()
            .find(|frame| frame.output == output as usize)
            .map(|frame| frame.rgba)
            .ok_or_else(|| "No active guest frame for this output".into())
    }

    pub fn read_virtual_memory(
        &self,
        core: u32,
        address: u32,
        length: u32,
    ) -> Result<Vec<u8>, String> {
        read_virtual_memory(&self.board, core, address, length)
    }

    pub fn read_physical_memory(&self, address: u32, length: u32) -> Result<Vec<u8>, String> {
        read_physical_memory(&self.board, address, length)
    }

    pub fn checkpoint_regions(&self) -> Vec<(u32, u32)> {
        self.board
            .checkpoint_regions()
            .into_iter()
            .map(|(base, size)| (base as u32, size as u32))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "requires the official X-T4 2.12 DAT"]
    fn state_template_keeps_live_fields_fresh() {
        let firmware = std::fs::read(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../FWUP0024.DAT"),
        )
        .unwrap();
        for (probe, measured) in [
            (Probe::Strict, false),
            (Probe::LinkedDmaInput, false),
            (Probe::LinkedDmaInput, true),
        ] {
            let mut session =
                Xp4Session::prepare_inputs(&firmware, probe, measured, false).unwrap();
            let before = session.state();
            assert_eq!(session.state(), before);
            session.execute(100).unwrap();
            let live = session.state();
            let progress = serde_json::to_value(session.run_state()).unwrap();
            for key in ["instructions", "virtual_ticks", "failure", "abort_failure"] {
                assert_eq!(progress["cpu"][key], live["cpu"][key]);
            }
            for key in ["guestHalt", "outputs", "quiescent"] {
                assert_eq!(progress[key], live[key]);
            }
            assert!(session.execute_batch(0).is_err());
            assert!(session.execute_batch(1_000_001).is_err());
            assert_eq!(session.state(), live);
            assert_ne!(live["cpu"]["instructions"], before["cpu"]["instructions"]);
            session.state_template.take();
            assert_eq!(session.state(), live);
            assert_eq!(
                session.state_template.get().unwrap(),
                &session.static_state()
            );
        }
    }

    #[test]
    fn measured_inputs_replace_the_banks_and_bind_checkpoint_identity() {
        let inputs = measured_inputs().unwrap();
        // Virtual navigation buttons are active-low: released reads high.
        for &(_, port, pin, _) in &VIRTUAL_BUTTONS {
            assert_ne!(inputs.gpio[port] & 1 << pin, 0);
        }
        assert!(inputs.adc.iter().all(|&value| value <= 4095));
        let source = [0u8; 4];
        assert_ne!(
            checkpoint::identity(Probe::LinkedDmaInput, &source, false),
            checkpoint::identity(Probe::LinkedDmaInput, &source, true)
        );
        assert!(
            checkpoint::identity(Probe::LinkedDmaInput, &source, false).ends_with(&format!(
                "{}/bis-peer:1/clock:{}:{}",
                fujimod_firmware::sha256(ERASED_FLASH_PROFILE.as_bytes()),
                LINKED_DMA_INSTRUCTIONS_PER_TICK.0,
                LINKED_DMA_INSTRUCTIONS_PER_TICK.1
            ))
        );
    }

    #[test]
    #[ignore = "requires the official X-T4 2.12 DAT"]
    fn actual_firmware_linked_dma_entry_uses_aggregate_clock_before_dma() {
        let firmware = std::fs::read(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../FWUP0024.DAT"),
        )
        .unwrap();
        let baseline = Xp4Session::discover_fff74000_ready(&firmware).unwrap();
        let candidate = Xp4Session::discover_linked_dma_input(&firmware).unwrap();
        assert_eq!(
            candidate.state()["registers"],
            baseline.state()["registers"]
        );
        assert_eq!(
            candidate.state()["boardProfileSha256"],
            baseline.state()["boardProfileSha256"]
        );
        for budget in [0, 1_000_000] {
            let expected = if budget == 0 {
                baseline.state()
            } else {
                baseline.execute(budget).unwrap()
            };
            let actual = if budget == 0 {
                candidate.state()
            } else {
                candidate.execute(budget).unwrap()
            };
            let extra = &actual["linkedDmaInputFixture"];
            assert_eq!(extra["sourceGuardSha256"], linked_dma::GUARD_SHA256);
            assert!(extra["wordLimit"].is_null());
            assert_eq!(extra["wordClock"], json!({"ticks":1125,"words":2}));
            assert_eq!(extra["initialFifoWords"], 1440);
            assert_eq!(extra["wordBytes"], 4);
            let declares_clock = |state: &Value| {
                state["assumptions"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|value| {
                        value
                            .as_str()
                            .unwrap()
                            .starts_with("Synthetic aggregate A7/peripheral clock ratio ")
                    })
            };
            assert!(declares_clock(&actual));
            assert!(!declares_clock(&expected));
            assert_eq!(
                actual["cpu"]["instructions"],
                expected["cpu"]["instructions"]
            );
            assert!(
                actual["cpu"]["virtual_ticks"].as_u64().unwrap()
                    < expected["cpu"]["virtual_ticks"].as_u64().unwrap()
            );
            assert!(actual["cpu"]["failure"].is_null());
            assert!(actual["cpu"]["abort_failure"].is_null());
            assert!(actual["error"].is_null());
            assert!(actual["guestHalt"].is_null());
        }
        let diagnostics = candidate.diagnostics().unwrap();
        assert_eq!(
            diagnostics["devices"]["xp4-linked-dma-input"]["received"],
            0
        );
        assert!(
            diagnostics["devices"]["xp4-dma"]["channels"][4]
                .get("linked")
                .is_none()
        );
    }

    #[test]
    #[ignore = "requires the official X-T4 2.12 DAT"]
    fn actual_firmware_fff74000_entry_preserves_the_inherited_scenario() {
        let firmware = std::fs::read(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../FWUP0024.DAT"),
        )
        .unwrap();
        let baseline = Xp4Session::discover_fff50000_second_ready(&firmware)
            .unwrap()
            .state();
        assert!(baseline.get("fff74000ReadyFixture").is_none());
        let mut candidate = Xp4Session::discover_fff74000_ready(&firmware)
            .unwrap()
            .state();
        let extra = candidate
            .as_object_mut()
            .unwrap()
            .remove("fff74000ReadyFixture")
            .unwrap();
        assert_eq!(extra["address"], 0xfff74000u32);
        assert_eq!(extra["sourceGuardSha256"], fff74000_ready::GUARD_SHA256);
        assert_eq!(extra["sourceGuardBytes"], 100);
        assert!(extra["maximumCommands"].is_null());
        assert_eq!(extra["pendingWord"], 0);
        assert_eq!(extra["readyWord"], 1);
        assert_eq!(extra["latencyTicks"], 8);
        let added = candidate["assumptions"]
            .as_array_mut()
            .unwrap()
            .pop()
            .unwrap();
        assert!(
            added
                .as_str()
                .unwrap()
                .starts_with("Synthetic FFF74000 ready experiment:")
        );
        assert_eq!(candidate, baseline);
    }

    #[test]
    #[ignore = "requires the official X-T4 2.12 DAT"]
    fn actual_firmware_second_ready_entry_preserves_the_single_request_scenario() {
        let firmware = std::fs::read(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../FWUP0024.DAT"),
        )
        .unwrap();
        let baseline = Xp4Session::discover_fff50000_ready(&firmware)
            .unwrap()
            .state();
        assert!(baseline.get("fff50000SecondReadyFixture").is_none());
        let mut candidate = Xp4Session::discover_fff50000_second_ready(&firmware)
            .unwrap()
            .state();
        let extra = candidate
            .as_object_mut()
            .unwrap()
            .remove("fff50000SecondReadyFixture")
            .unwrap();
        assert!(extra["maximumRequests"].is_null());
        assert_eq!(extra["sourceGuardSha256"], fff50000_ready::GUARD_SHA256);
        assert_eq!(extra["sourceGuardBytes"], 120);
        assert_eq!(
            extra["requestTokensAndOrdinals"],
            "monotonic per completed request"
        );
        assert_eq!(extra["pendingRepeat"], "reject");
        assert!(candidate["fff50000ReadyFixture"]["maximumCommands"].is_null());
        assert_eq!(candidate["fff50000ReadyFixture"]["reentrySupported"], true);
        candidate["fff50000ReadyFixture"]["maximumCommands"] = json!(1);
        candidate["fff50000ReadyFixture"]["reentrySupported"] = json!(false);
        let old = baseline["assumptions"].as_array().unwrap();
        let assumptions = candidate["assumptions"].as_array_mut().unwrap();
        assert_eq!(assumptions.len(), old.len());
        let index = old
            .iter()
            .position(|v| {
                v.as_str()
                    .unwrap()
                    .starts_with("Synthetic FFF50000 ready experiment:")
            })
            .unwrap();
        assert!(
            assumptions[index]
                .as_str()
                .unwrap()
                .starts_with("Synthetic second FFF50000 ready experiment:")
        );
        assumptions[index] = old[index].clone();
        assert_eq!(candidate, baseline);
    }

    #[test]
    #[ignore = "requires the official X-T4 2.12 DAT"]
    fn actual_firmware_second_1c_entry_preserves_the_inherited_scenario() {
        let firmware = std::fs::read(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../FWUP0024.DAT"),
        )
        .unwrap();
        let baseline = Xp4Session::discover_f01c_second_completion(&firmware)
            .unwrap()
            .state();
        let mut candidate = Xp4Session::discover_fff7801c_second_completion(&firmware)
            .unwrap()
            .state();
        assert_eq!(
            baseline["fff7801cCompletionFixture"]["reentrySupported"],
            false
        );
        assert!(baseline.get("fff7801cSecondCompletionFixture").is_none());
        let fixture = candidate
            .as_object_mut()
            .unwrap()
            .remove("fff7801cSecondCompletionFixture")
            .unwrap();
        assert_eq!(
            fixture["firstSourceGuardSha256"],
            fff7801c_completion::GUARD_SHA256
        );
        assert_eq!(
            fixture["secondSourceGuardSha256"],
            f01c_second_completion::GUARD_SHA256
        );
        assert!(fixture["maximumCommands"].is_null());
        assert_eq!(fixture["secondConsumerPollMask"], 1);
        assert_eq!(fixture["samePendingWrite"], "reject");
        assert_eq!(
            candidate["fff7801cCompletionFixture"]["reentrySupported"],
            true
        );
        candidate["fff7801cCompletionFixture"]["reentrySupported"] = json!(false);
        let old = baseline["assumptions"].as_array().unwrap();
        let assumptions = candidate["assumptions"].as_array_mut().unwrap();
        assert_eq!(assumptions.len(), old.len());
        let index = old
            .iter()
            .position(|x| {
                x.as_str()
                    .unwrap()
                    .starts_with("Synthetic single FFF7801C command experiment:")
            })
            .unwrap();
        assert!(
            assumptions[index]
                .as_str()
                .unwrap()
                .starts_with("Synthetic second FFF7801C command experiment:")
        );
        assumptions[index] = old[index].clone();
        assert_eq!(candidate, baseline);
    }

    #[test]
    fn passive_discovery_observes_a_real_poll_without_forcing_bits() {
        for passive in [false, true] {
            let definition = serde_json::from_value(json!({
                "ram_size":65536, "cores":1, "discovery":true, "discovery_passive":passive
            }))
            .unwrap();
            let board = Session::new(definition, None).unwrap();
            let program: Vec<_> = [0xe59f0004u32, 0xe5901000, 0xeafffffd, 0xfff4a400]
                .into_iter()
                .flat_map(u32::to_le_bytes)
                .collect();
            board.write(0, &program).unwrap();
            board.write_register(0, arm::PC, 0).unwrap();
            board.run(0, 10_000).unwrap();
            assert!(
                board
                    .frontier(false)
                    .iter()
                    .any(|row| row.address == 0xfff4a400 && !row.write && row.count > 1024)
            );
            assert_eq!(board.interventions().is_empty(), passive);
            if passive {
                assert_eq!(board.read_register(0, arm::CORE[1]).unwrap(), 0);
            }
        }
    }

    #[test]
    fn rec_state_memory_hooks_coexist_with_four_core_spi_and_host_observation() {
        let target = 0x1b7c9c8u32;
        let make = || {
            let board=Rc::new(Session::new(serde_json::from_value(json!({"ram_size":67108864,"cores":4,"discovery":false,"cbar":4294901760u32})).unwrap(),None).unwrap());
            // Synthetic LDR literal; MOV R1,R1; STRB; LDRB; spin; target literal.
            let program: Vec<_> = [
                0xe59f000cu32,
                0xe1a01001,
                0xe5c01000,
                0xe5d02000,
                0xeafffffe,
                target,
            ]
            .into_iter()
            .flat_map(u32::to_le_bytes)
            .collect();
            board.write(0x11fb000, &program).unwrap();
            for core in 0..4 {
                board.write_register(core, arm::PC, 0x11fb000).unwrap();
                board
                    .write_register(core, arm::CORE[1], core as u64 + 1)
                    .unwrap();
            }
            board
        };
        let observed = make();
        let control = make();
        let observed_spi = spi2::install_trace(&observed, true, true, None).unwrap();
        let control_spi = spi2::install_trace(&control, true, true, None).unwrap();
        observed
            .memory_watch_begin(vec![(u64::from(target), u64::from(target) + 1)], 4096)
            .unwrap();
        for core in 0..4 {
            observed.run(core, 4).unwrap();
            control.run(core, 4).unwrap();
            assert_eq!(
                serde_json::to_value(observed.state(true)).unwrap(),
                serde_json::to_value(control.state(true)).unwrap()
            );
            assert_eq!(observed_spi.borrow().records, control_spi.borrow().records);
            assert_eq!(observed_spi.borrow().dropped, control_spi.borrow().dropped);
        }
        let watch = observed.memory_watch_state();
        assert!(watch.violation.is_none());
        assert_eq!(watch.records.len(), 8);
        assert_eq!(observed_spi.borrow().records.len(), 4);
        for core in 0..4 {
            let write = &watch.records[core * 2];
            let read = &watch.records[core * 2 + 1];
            assert_eq!(
                (
                    write.source,
                    write.core,
                    write.pc,
                    write.address,
                    write.physical,
                    write.size,
                    write.value
                ),
                (
                    "cpu-write",
                    core,
                    0x11fb008,
                    u64::from(target),
                    u64::from(target),
                    1,
                    Some(core as u64 + 1)
                )
            );
            assert_eq!(write.before.as_deref(), Some([core as u8].as_slice()));
            assert_eq!(
                (read.source, read.core, read.pc, read.value),
                ("cpu-read", core, 0x11fb00c, Some(core as u64 + 1))
            );
        }
        let before = serde_json::to_value(observed.memory_watch_state()).unwrap();
        let _ = observed.state(true);
        let _ = observed.device_state_json();
        let _ = observed.device_records();
        assert_eq!(observed.read_virtual(0, target, 1).unwrap(), vec![4]);
        let mut actual = [0];
        observed.read(target as usize, &mut actual).unwrap();
        assert_eq!(actual, [4]);
        assert_eq!(
            before,
            serde_json::to_value(observed.memory_watch_state()).unwrap()
        );
    }

    #[test]
    fn memory_watch_records_an_original_store_operand_before_commit() {
        let target = 0x1be6af0u32;
        let definition = serde_json::from_value(
            json!({"ram_size":33554432,"cores":1,"discovery":false,"cbar":4294901760u32}),
        )
        .unwrap();
        let board = Session::new(definition, None).unwrap();
        let program: Vec<_> = [0xe59f0008u32, 0xe3a01004, 0xe5c01000, 0xeafffffe, target]
            .into_iter()
            .flat_map(u32::to_le_bytes)
            .collect();
        board.write(0, &program).unwrap();
        board.write_register(0, arm::PC, 0).unwrap();
        board
            .memory_watch_begin(vec![(u64::from(target), u64::from(target) + 1)], 4096)
            .unwrap();
        board.run(0, 8).unwrap();
        let trace = board.memory_watch_state();
        assert!(trace.violation.is_none());
        assert_eq!(trace.records.len(), 1);
        let row = &trace.records[0];
        assert_eq!(
            (row.source, row.pc, row.core, row.size),
            ("cpu-write", 8, 0, 1)
        );
        assert_eq!(
            (row.address, row.physical, row.value),
            (u64::from(target), u64::from(target), Some(4))
        );
        assert_eq!(row.before.as_deref(), Some(&[0][..]));
        let mut value = [0];
        board.read(target as usize, &mut value).unwrap();
        assert_eq!(value, [4]);
        assert_eq!(board.memory_watch_state().records.len(), 1);
        board.memory_watch_end().unwrap();
        assert!(!board.memory_watch_enabled());
        assert!(board.memory_watch_state().records.is_empty());
    }

    #[test]
    fn virtual_memory_inspection_is_bounded_and_does_not_touch_devices_or_cpu_state() {
        let definition = serde_json::from_value(json!({
            "ram_size":131072,"cores":1,"discovery":true,"cbar":4294901760u32
        }))
        .unwrap();
        let board = Session::new(definition, None).unwrap();
        board.write(0x1000, &[1, 2, 3, 4]).unwrap();
        let before = serde_json::to_value(board.state(true)).unwrap();
        assert_eq!(
            read_virtual_memory(&board, 0, 0x1000, 4).unwrap(),
            [1, 2, 3, 4]
        );
        assert_eq!(
            read_virtual_memory(&board, 0, 0, 65536).unwrap().len(),
            65536
        );
        for (core, address, length) in [
            (1, 0, 1),
            (0, 0, 0),
            (0, 0, 65537),
            (0, u32::MAX, 2),
            (0, 0xfff34008, 1),
        ] {
            assert!(read_virtual_memory(&board, core, address, length).is_err());
        }
        assert_eq!(serde_json::to_value(board.state(true)).unwrap(), before);
        assert!(board.frontier(false).is_empty());
        assert!(board.device_records().is_empty());
    }

    #[test]
    fn spi_dma_waits_for_serial_data_and_receives_each_byte() {
        let definition = serde_json::from_value(json!({
            "ram_size":65536,"cores":1,"discovery":false,"cbar":4294901760u32,
            "storage_jedec":44,"storage_dma_routes":vec![37;16],
            "storage_geometry":[4096,256,8,8],"storage_irq":37,
            "spi_dma_exchange":{"base":0xfff34000u32,"receive_request":4,
                "transmit_request":5,"beat_ticks":1,
                "expected":[0xa5,0x5a,0x3c,0xc3],"response":[0xc3,0x3c,0xa5,0x5a]}
        }))
        .unwrap();
        let board = Session::new(definition, None).unwrap();
        let run = |pc: u32, program: &[u32], registers: &[u32], budget| {
            let bytes: Vec<_> = program.iter().copied().flat_map(u32::to_le_bytes).collect();
            board.write(pc as usize, &bytes).unwrap();
            for (reg, value) in registers.iter().enumerate() {
                board
                    .write_register(0, arm::CORE[reg], u64::from(*value))
                    .unwrap();
            }
            board.write_register(0, arm::PC, u64::from(pc)).unwrap();
            let end = board.state(false).instructions + budget;
            for _ in 0..64 {
                let remaining = end - board.state(false).instructions;
                if remaining == 0 {
                    break;
                }
                // A DMA RAM write yields so the CPU can invalidate translated code.
                board.run(0, usize::try_from(remaining).unwrap()).unwrap();
                assert_eq!(board.state(false).failure, None);
            }
            assert_eq!(board.state(false).instructions, end);
        };
        let dma =
            || -> Value { serde_json::from_str(&board.device_state_json()["xp4-dma"]).unwrap() };
        board.write(0x2000, &[0xa5, 0x5a, 0x3c, 0xc3]).unwrap();
        board.write(0x3000, &[0x5a; 4]).unwrap();
        run(
            0,
            &[
                0xe5801000, 0xe5802004, 0xe5803010, 0xe5804020, 0xe5804024, 0xe5805004, 0xe5886000,
                0xe5897000, 0xeafffffe,
            ],
            &[
                0xfff34000, 0xc7, 8, 36, 3, 0xa, 0x01000400, 0x00010005, 0xfff4a508, 0xfff4a50c,
            ],
            100,
        );
        let stores = [
            0xe5801010, 0xe5802014, 0xe5803018, 0xe580401c, 0xe5805020, 0xe5806028, 0xeafffffe,
        ];
        run(
            0x100,
            &stores,
            &[0xfffe5000, 3, 0xfff34008, 0x3000, 4, 0, 0x10201011],
            1000,
        );
        let waiting = dma();
        assert_eq!(waiting["channels"][5]["submitted"], true);
        assert_eq!(waiting["channels"][5]["completed"], 0);
        assert!(waiting["channels"][5]["next"].is_null());
        let mut received = [0; 4];
        board.read(0x3000, &mut received).unwrap();
        assert_eq!(received, [0x5a; 4]);
        run(
            0x100,
            &stores,
            &[0xfffe6000, 3, 0x2000, 0xfff34008, 0, 4, 0x10201011],
            10000,
        );
        board.read(0x3000, &mut received).unwrap();
        assert_eq!(received, [0xc3, 0x3c, 0xa5, 0x5a]);
        let complete = dma();
        for channel in [5, 6] {
            assert_eq!(complete["channels"][channel]["completed"], 1);
            assert_eq!(complete["channels"][channel]["submitted"], false);
        }
    }

    #[test]
    fn spi2_ram_observation_reads_the_fixed_fields_without_advancing_the_guest() {
        for ram_size in [65536, 33554432] {
            let definition = serde_json::from_value(json!({
                "ram_size":ram_size,"cores":1,"discovery":false,"cbar":4294901760u32
            }))
            .unwrap();
            let board = Session::new(definition, None).unwrap();
            if ram_size > 65536 {
                board.write(0x177b480, &[0xa5, 0x5a, 0x3c, 0xc3]).unwrap();
                board.write(0x177bcc0, &[0x12, 0x34, 0x56, 0x78]).unwrap();
                board.write(0x1ab65a4, &[12]).unwrap();
                board
                    .write(0x1ab20bc, &0x44556677u32.to_le_bytes())
                    .unwrap();
            }
            let before = serde_json::to_value(board.state(true)).unwrap();
            let observed = observe_spi2_client_ram(&board);
            assert_eq!(serde_json::to_value(board.state(true)).unwrap(), before);
            assert_eq!(observed["maximumBytesRead"], 13);
            let sample = &observed["sample"];
            if ram_size > 65536 {
                assert_eq!(sample["txBytes"], json!([0xa5, 0x5a, 0x3c, 0xc3]));
                assert_eq!(sample["rxBytes"], json!([0x12, 0x34, 0x56, 0x78]));
                assert_eq!(sample["clientState"], 12);
                assert_eq!(sample["serialHandle"], 0x44556677);
            } else {
                assert!(sample["unavailable"].is_string());
                assert!(sample.get("txBytes").is_none());
            }
        }
    }

    #[test]
    fn failed_dma_keeps_its_original_error_and_does_not_write_destination() {
        for source in [0x2000u32, 0xfff34008] {
            let definition = serde_json::from_value(json!({
                "ram_size":65536, "cores":1, "discovery":false,
                "cbar":4294901760u32, "storage_jedec":44,
                "storage_dma_routes":vec![37;16],
                "storage_geometry":[4096,256,8,8], "storage_irq":37
            }))
            .unwrap();
            let board = Session::new(definition, None).unwrap();
            let program: Vec<_> = [
                0xe5801010u32,
                0xe5802014,
                0xe5803018,
                0xe580401c,
                0xe5805020,
                0xe5806028,
                0xeafffffe,
            ]
            .into_iter()
            .flat_map(u32::to_le_bytes)
            .collect();
            board.write(0, &program).unwrap();
            board.write(0x2000, &[1, 2, 3, 4]).unwrap();
            board.write(0x1000, &[0x5a; 4]).unwrap();
            for (reg, value) in [0xfffe5000, 3, source, 0x1000, 4, 0, 0x10000001]
                .into_iter()
                .enumerate()
            {
                board
                    .write_register(0, arm::CORE[reg], value.into())
                    .unwrap();
            }
            board.write_register(0, arm::PC, 0).unwrap();
            board.run(0, 1000).unwrap();
            let mut destination = [0; 4];
            board.read(0x1000, &mut destination).unwrap();
            if source == 0x2000 {
                assert_eq!(board.state(false).failure, None);
                assert_eq!(destination, [1; 4]);
            } else {
                assert_eq!(destination, [0x5a; 4]);
                assert_eq!(
                    board.state(false).failure.as_deref(),
                    Some("Outside declared ARM32 physical memory")
                );
            }
        }
    }

    #[test]
    #[ignore = "requires the user-provided X-T4 2.12 DAT; run bun run test:xt4:scenarios"]
    fn zero_retained_words_select_loader_invalid_data_path() {
        let source =
            std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../../FWUP0024.DAT")).unwrap();
        let session = Xp4Session::discover_empty_retained(&source).unwrap();
        let state = session.execute(1_000_000).unwrap();
        assert!(state["cpu"]["failure"].is_null());
        assert_eq!(state["bootVerified"], false);
        assert_eq!(state["assumptions"].as_array().unwrap().len(), 2);
        let mut retained = [0xff; 12];
        session.board.read(0x10c30, &mut retained).unwrap();
        // The loader read all four words and rejected their absent 0xaa marker.
        assert_eq!(retained, [0; 12]);
        let diagnostics = session.diagnostics().unwrap();
        let peer = &diagnostics["devices"]["xp4-spi1"]["input"];
        assert_eq!(peer["retained_registers"], json!([31, 32, 33, 34]));
        assert_eq!(peer["retained_written"], json!([]));
        assert!(peer["rtc"].is_null());
        let tx: Vec<_> = diagnostics["records"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|record| record["name"] == "XP4 SPI word complete")
            .filter_map(|record| {
                record["fields"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|field| field[0] == "tx")
                    .and_then(|field| field[1]["Int"].as_u64())
            })
            .collect();
        for register in 31..=34 {
            assert!(
                tx.as_chunks::<4>()
                    .0
                    .iter()
                    .any(|frame| *frame == [register, 0, 0, 255 - register])
            );
        }
    }

    #[test]
    #[ignore = "requires the user-provided X-T4 2.12 DAT; run bun run test:xt4:scenarios"]
    fn erased_nor_completes_jedec_before_training_wait() {
        let source =
            std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../../FWUP0024.DAT")).unwrap();
        let session = Xp4Session::discover_erased_flash(&source).unwrap();
        let state = session.execute(1_000_000).unwrap();
        assert!(state["cpu"]["failure"].is_null());
        assert_eq!(state["bootVerified"], false);
        assert_eq!(state["interventions"], json!([]));
        let diagnostics = session.diagnostics().unwrap();
        let flash = &diagnostics["devices"]["xp4-serial-flash"];
        assert_eq!(flash["operations"], 1);
        assert!(flash["next"].is_null());
        assert_eq!(flash["jedec"], 0);
        assert_eq!(flash["registers"][0x90 / 4], 0x9f800000u32);
        assert_eq!(flash["registers"][0x40 / 4], 0x4000);
        assert_eq!(flash["registers"][0xa0 / 4], 0);
        // JEDEC completed; the next unimplemented handshake is training bit15.
        assert_eq!(flash["registers"][0xb4 / 4], 0x40000000);
        assert_eq!(flash["registers"][0xbc / 4], 0);
    }

    #[test]
    #[ignore = "requires the user-provided X-T4 2.12 DAT; run bun run test:xt4:scenarios"]
    fn ideal_training_reads_the_source_pattern_and_selects_the_midpoint() {
        let source =
            std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../../FWUP0024.DAT")).unwrap();
        let session = Xp4Session::discover_training(&source).unwrap();
        let state = session.execute(1_000_000).unwrap();
        assert!(state["cpu"]["failure"].is_null());
        assert_eq!(state["bootVerified"], false);
        assert_eq!(state["interventions"], json!([]));
        let mut pattern = [0; 128];
        session.board.read(0xca00, &mut pattern).unwrap();
        assert_eq!(
            fujimod_firmware::sha256(&pattern),
            "10fa7918107458a1b72e18ef91b9bfc27b6eb76e3b562aa6566c5561dcf4929a"
        );
        let diagnostics = session.diagnostics().unwrap();
        let flash = &diagnostics["devices"]["xp4-serial-flash"];
        assert_eq!(flash["registers"][0xb4 / 4], 0xc01f003eu32);
        assert_eq!(flash["registers"][0xbc / 4], 0x8001);
        assert_eq!(
            diagnostics["records"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|record| record["name"] == "XP4 flash training handshake")
                .count(),
            130 // Two variants × 32 tap trials, plus the final tap; two stages each.
        );
    }

    #[test]
    #[ignore = "requires the user-provided X-T4 2.12 DAT; run bun run test:xt4:scenarios"]
    fn primary_initialization_before_secondary_entry_reaches_the_main_image() {
        let source =
            std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../../FWUP0024.DAT")).unwrap();
        let session = Xp4Session::discover_ordered(&source).unwrap();
        let initial = session.state();
        assert_eq!(initial["registers"][0][15], 0x4cc);
        assert_eq!(initial["cpu"]["per_core"][1], 0);
        assert_eq!(initial["cpu"]["per_core"][2], 0);
        assert_eq!(initial["cpu"]["per_core"][3], 0);
        let mut entered_main = false;
        for _ in 0..10 {
            let state = session.execute(1_000_000).unwrap();
            assert!(state["cpu"]["failure"].is_null());
            assert_eq!(state["bootVerified"], false);
            entered_main = state["cpu"]["history"]["milestones"]
                .as_array()
                .unwrap()
                .iter()
                .any(|event| event[0] == 0 && event[1] == 0x012910c4);
            if entered_main {
                break;
            }
        }
        assert!(
            entered_main,
            "guest must leave its barrier and execute the real main entry"
        );
        assert_eq!(
            session.diagnostics().unwrap()["loaderBarrier"]["words"],
            json!([0, 0, 0, 0])
        );
    }
}

#[cfg(test)]
mod kernel_write_watch_tests {
    use super::*;

    #[test]
    #[ignore = "requires the official X-T4 2.12 DAT"]
    fn physical_guard_arm_and_readback_use_the_actual_firmware() {
        let firmware = std::fs::read(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../FWUP0024.DAT"),
        )
        .unwrap();
        {
            let session = Xp4Session::discover_fa_completion(&firmware).unwrap();
            let before = session.state();
            let diagnostics = session.diagnostics().unwrap();
            assert!(read_physical_memory(&session.board, 0, 0).is_err());
            assert!(read_physical_memory(&session.board, 0, 65537).is_err());
            assert!(read_physical_memory(&session.board, u32::MAX, 2).is_err());
            assert!(read_physical_memory(&session.board, 0xffff2000, 4).is_err());
            let armed = session.begin_kernel_write_watch().unwrap();
            assert_eq!(armed["watch"]["limit"], 64);
            assert_eq!(armed["watch"]["records"], json!([]));
            assert!(
                armed["readback"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .all(|row| row["matchesOriginal"] == true)
            );
            assert_eq!(session.state(), before);
            let mut after_diagnostics = session.diagnostics().unwrap();
            assert!(
                after_diagnostics
                    .as_object_mut()
                    .unwrap()
                    .remove("kernelWriteWatch")
                    .is_some()
            );
            assert_eq!(after_diagnostics, diagnostics);
            assert!(session.begin_kernel_write_watch().is_err());
            session.board.write(0x49408, &[0; 4]).unwrap();
            let trace = session.kernel_write_watch().unwrap();
            assert_eq!(trace["watch"]["records"][0]["origin"], "host-session-write");
            assert_eq!(
                trace["watch"]["records"][0]["after"]["Ok"],
                json!([0, 0, 0, 0])
            );
            assert_eq!(trace["readback"][0]["matchesOriginal"], false);
        }
        let session = Xp4Session::discover_fa_completion(&firmware).unwrap();
        session.board.write(0x49408, &[0; 4]).unwrap();
        assert!(session.begin_kernel_write_watch().is_err());
        assert!(session.board.ram_write_watch_snapshot().is_none());
    }
}

#[cfg(test)]
mod kernel_irq_trace_tests {
    use super::*;

    #[test]
    #[ignore = "requires the official X-T4 2.12 DAT"]
    fn physical_guard_and_actual_irq_trace_arm_are_observational() {
        let firmware = std::fs::read(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../FWUP0024.DAT"),
        )
        .unwrap();
        {
            let session = Xp4Session::discover_fa_completion(&firmware).unwrap();
            let before = session.state();
            let diagnostics = session.diagnostics().unwrap();
            let armed = session.begin_kernel_irq_trace().unwrap();
            assert_eq!(armed["limit"], 64);
            assert_eq!(armed["records"], json!([]));
            assert_eq!(armed["frozen"], false);
            assert_eq!(session.state(), before);
            let mut after = session.diagnostics().unwrap();
            assert!(
                after
                    .as_object_mut()
                    .unwrap()
                    .remove("kernelIrqTrace")
                    .is_some()
            );
            assert_eq!(after, diagnostics);
            assert!(session.begin_kernel_irq_trace().is_err());
            assert!(
                kernel_write_readback(&session.board)
                    .unwrap()
                    .iter()
                    .all(|r| r["matchesOriginal"] == true)
            );
        }
        let session = Xp4Session::discover_fa_completion(&firmware).unwrap();
        session.board.write(0x49408, &[0; 4]).unwrap();
        assert!(session.begin_kernel_irq_trace().is_err());
        assert!(session.board.irq_trace_snapshot().is_none());
    }
}
