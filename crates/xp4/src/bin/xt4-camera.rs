//! Synchronous loopback backend for the camera WebSocket protocol.
// Reuse inspection without Cargo's collision between the two local firmware packages.
#[allow(dead_code)]
#[path = "../../../core/src/lib.rs"]
mod firmware;
use firmware::FirmwareSession;
use fuji_boost_xp4::{
    Contact, Xp4Session, checkpoint_file,
    ram_image::{Image, ImageSession},
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{fs::OpenOptions, io::Write, net::TcpListener, time::Duration};
use tungstenite::{
    Message,
    handshake::server::{Request as Handshake, Response as Upgrade},
    protocol::WebSocketConfig,
};

use fuji_boost_xp4::input::{BUTTONS, press_button, rotate_dial};
const MAX_MESSAGE: usize = 256 << 20;

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
enum Command {
    Load,
    Checkpoint,
    Scene {
        source: String,
        width: Option<u32>,
        height: Option<u32>,
        /// Image mode: physical address of the firmware scene header.
        address: Option<u32>,
        /// Image mode: monotonic generation; 0 is reserved for "no frame".
        generation: Option<u32>,
    },
    Read {
        segment: usize,
        offset: usize,
        length: usize,
    },
    Execute {
        budget: u32,
        mode: String,
        /// Image mode: advance this many guest ticks (54 MHz) unless `budget`
        /// instructions pass first.
        ticks: Option<u64>,
    },
    Button {
        button: String,
    },
    CommandDial {
        dial: String,
        direction: i32,
    },
    Gpio {
        port: usize,
        pin: usize,
        high: bool,
    },
    Adc {
        channel: usize,
        value: u16,
    },
    /// A raw RAM image on the bare board instead of the official DAT.
    #[serde(rename_all = "camelCase")]
    LoadImage {
        load_address: u32,
        entry: u32,
    },
    /// Image mode: hold or release a navigation key.
    ButtonState {
        button: String,
        pressed: bool,
    },
    /// Image mode: the fingers on the rear panel (at most two), in its
    /// 900×600 raw space; an empty list lifts every finger.
    Touch {
        contacts: Vec<Contact>,
    },
    ReadMemory {
        address: u32,
        length: u32,
    },
    WriteMemory {
        address: u32,
    },
    SaveCheckpoint,
}

/// Largest single image-mode execute; a longer request returns at this cap
/// and the client simply repeats.
const IMAGE_BUDGET_LIMIT: u32 = 50_000_000;
const IMAGE_TICK_LIMIT: u64 = 54_000_000;
const MAX_MEMORY_READ: u32 = 16 << 20;

fn decode(bytes: &[u8]) -> Result<(Value, &[u8]), String> {
    if bytes.len() < 4 || bytes.len() > MAX_MESSAGE {
        return Err("Invalid wire length".into());
    }
    let end = 4 + u32::from_le_bytes(bytes[..4].try_into().unwrap()) as usize;
    let value: Value = serde_json::from_slice(bytes.get(4..end).ok_or("Truncated wire JSON")?)
        .map_err(|e| e.to_string())?;
    let raw = &bytes[end..];
    let length = match value.get("buffer") {
        Some(marker) if marker.as_object().is_some_and(|o| o.len() == 1) => {
            marker["$bytes"].as_u64().ok_or("Invalid byte marker")?
        }
        Some(_) => return Err("Invalid byte marker".into()),
        None => 0,
    };
    if length != raw.len() as u64 {
        return Err("Wire byte count differs".into());
    }
    Ok((value, raw))
}
fn encode(value: &Value, raw: &[u8]) -> Vec<u8> {
    let json = serde_json::to_vec(value).unwrap();
    let mut bytes = Vec::with_capacity(4 + json.len() + raw.len());
    bytes.extend((json.len() as u32).to_le_bytes());
    bytes.extend(json);
    bytes.extend(raw);
    bytes
}
fn trace_command(file: &mut impl Write, request: &Value, raw: &[u8]) -> std::io::Result<()> {
    let mut command = request.clone();
    command.as_object_mut().unwrap().remove("id");
    command.as_object_mut().unwrap().remove("buffer");
    if !raw.is_empty() {
        command["sha256"] = json!(fujimod_firmware::sha256(raw));
    }
    serde_json::to_writer(&mut *file, &command)?;
    writeln!(file)
}
#[derive(Default)]
struct Camera {
    firmware: Option<FirmwareSession>,
    source: Vec<u8>,
    cpu: Option<Xp4Session>,
    image: Option<ImageSession>,
    mode: String,
    // Last (output, sequence) sent; unchanged LCD frames are not resent.
    sent: Option<(u64, u64)>,
}
impl Camera {
    fn cpu(&self) -> Result<&Xp4Session, String> {
        if self.image.is_some() {
            return Err("这条命令只用于原厂固件；当前运行的是 RAM 映像。".into());
        }
        self.cpu.as_ref().ok_or("请先启动 CPU 会话。".into())
    }
    fn image(&self) -> Result<&ImageSession, String> {
        self.image.as_ref().ok_or("请先加载 RAM 映像。".into())
    }
    fn frame_rgba(&self, output: u32) -> Result<Vec<u8>, String> {
        match &self.image {
            Some(image) => image.frame_rgba(output),
            None => self.cpu()?.frame_rgba(output).map_err(|e| format!("{e:?}")),
        }
    }
    fn image_executed(
        &mut self,
        state: fuji_boost_xp4::Execution,
    ) -> Result<(Value, Vec<u8>), String> {
        self.executed(serde_json::to_value(state).unwrap())
    }
    fn xt4(&self, error: &str) -> Result<(), String> {
        if self.firmware.as_ref().is_some_and(|s| {
            s.report()
                .profile
                .as_ref()
                .is_some_and(|p| p.id == "xt4-212")
                || s.report().family == Some("xt4-212")
        }) {
            Ok(())
        } else {
            Err(error.into())
        }
    }
    fn release(cpu: &Xp4Session) -> Result<(), String> {
        for input in BUTTONS {
            cpu.set_virtual_button(input, false)?;
        }
        Ok(())
    }
    fn executed(&mut self, state: Value) -> Result<(Value, Vec<u8>), String> {
        let outputs = state["outputs"].as_array().ok_or("Missing outputs")?;
        let selected = outputs
            .iter()
            .find(|o| {
                o["output"]
                    == if ["reprogram", "image"].contains(&self.mode.as_str()) {
                        1
                    } else {
                        0
                    }
            })
            .or_else(|| outputs.first());
        let mut frame = Value::Null;
        let mut raw = vec![];
        let key = selected.and_then(|o| Some((o["output"].as_u64()?, o["sequence"].as_u64()?)));
        if let Some(output) = selected.filter(|_| key.is_none() || key != self.sent) {
            raw = self.frame_rgba(output["output"].as_u64().ok_or("Invalid output")? as u32)?;
            let width = output["width"].as_u64().ok_or("Invalid width")?;
            let height = output["height"].as_u64().ok_or("Invalid height")?;
            if width == 0
                || height == 0
                || width.saturating_mul(height) > 4096 * 4096
                || width * height * 4 != raw.len() as u64
            {
                return Err("Invalid guest frame dimensions or RGBA byte count".into());
            }
            frame = output.clone();
            frame["buffer"] = json!({"$bytes":raw.len()});
            self.sent = key;
        }
        Ok((
            json!({"type":"executed", "state":state, "frame":frame}),
            raw,
        ))
    }
    fn handle(&mut self, request: Value, raw: &[u8]) -> Result<(Value, Vec<u8>), String> {
        let command: Command = serde_json::from_value(request).map_err(|e| e.to_string())?;
        match command {
            Command::Load => {
                let next = FirmwareSession::inspect(raw)?;
                let report = serde_json::to_value(next.report()).map_err(|e| e.to_string())?;
                self.cpu = None;
                self.image = None;
                self.mode.clear();
                self.firmware = Some(next);
                self.source = raw.to_vec();
                return Ok((json!({"type":"loaded", "report":report}), vec![]));
            }
            Command::Read {
                segment,
                offset,
                length,
            } => {
                let bytes = self
                    .firmware
                    .as_ref()
                    .ok_or("请先加载固件。")?
                    .read_image(segment, offset, length)?;
                return Ok((
                    json!({"type":"bytes", "bytes":{"$bytes":bytes.len()}}),
                    bytes,
                ));
            }
            Command::Checkpoint => {
                if let Some(image) = &self.image {
                    checkpoint_file::restore(image, raw)?;
                    self.sent = None;
                    let state = image.run_state();
                    return self.image_executed(state);
                }
                self.xt4("读取存档需要先加载 X-T4 2.12 固件。")?;
                self.cpu = None;
                self.mode.clear();
                let next = Xp4Session::discover_linked_dma_input(&self.source)?;
                Self::release(&next)?;
                checkpoint_file::restore(&next, raw)?;
                self.cpu = Some(next);
                self.sent = None;
                self.mode = "reprogram".into();
                let state = serde_json::to_value(self.cpu()?.run_state()).unwrap();
                return self.executed(state);
            }
            Command::Scene {
                source,
                width,
                height,
                address,
                generation,
            } => match (&self.image, source.as_str()) {
                (Some(image), "rgb") => image.set_scene_rgb(
                    address.ok_or("RAM 映像画面需要 scene 地址。")?,
                    width.ok_or("Missing scene width")?,
                    height.ok_or("Missing scene height")?,
                    raw,
                    generation.ok_or("RAM 映像画面需要 generation。")?,
                )?,
                (Some(_), _) => {
                    return Err("RAM 映像只接受 rgb 画面。".into());
                }
                (None, "gray") => self.cpu()?.set_scene_gray()?,
                (None, "analytic") => self.cpu()?.set_scene_analytic()?,
                (None, "rgb") => self.cpu()?.set_scene_rgb(
                    width.ok_or("Missing scene width")?,
                    height.ok_or("Missing scene height")?,
                    raw,
                )?,
                (None, _) => return Err("Unknown scene source.".into()),
            },
            Command::Gpio { port, pin, high } => {
                if port >= 16 || pin >= 16 {
                    return Err("Invalid GPIO input.".into());
                }
                match &self.image {
                    Some(image) => image.set_gpio(port, pin, high)?,
                    None => self.cpu()?.set_gpio(port, pin, high)?,
                }
            }
            Command::Adc { channel, value } => {
                if channel >= 12 || value > 4095 {
                    return Err("Invalid ADC input.".into());
                }
                match &self.image {
                    Some(image) => image.set_adc(channel, value)?,
                    None => self.cpu()?.set_adc(channel, value)?,
                }
            }
            Command::Execute {
                budget,
                mode,
                ticks,
            } if mode == "image" => {
                if budget == 0 || budget > IMAGE_BUDGET_LIMIT {
                    return Err(format!("Image budget must be 1..{IMAGE_BUDGET_LIMIT}"));
                }
                let ticks = ticks.ok_or("Image execute needs ticks")?;
                if ticks == 0 || ticks > IMAGE_TICK_LIMIT {
                    return Err(format!("Image ticks must be 1..{IMAGE_TICK_LIMIT}"));
                }
                let state = self.image()?.run_for(ticks, u64::from(budget))?;
                return self.image_executed(state);
            }
            Command::Execute { budget, mode, .. } => {
                if self.image.is_some() {
                    return Err("RAM 映像会话只接受 image 执行模式。".into());
                }
                if !["strict", "preloaded", "calibration", "setup", "reprogram"]
                    .contains(&mode.as_str())
                {
                    return Err("Unknown execution mode.".into());
                }
                if self.cpu.is_some() && self.mode != mode {
                    return Err("重置会话后才能切换执行场景。".into());
                }
                self.xt4("目前仅接入 X-T4 2.12 的 CPU 启动路径。")?;
                if self.cpu.is_none() {
                    let next = match mode.as_str() {
                        "reprogram" => Xp4Session::discover_linked_dma_input(&self.source),
                        "setup" => Xp4Session::discover_power3_raw768(&self.source),
                        "calibration" => Xp4Session::discover_ideal_calibration(&self.source),
                        "preloaded" => Xp4Session::discover_preloaded(&self.source),
                        _ => Xp4Session::load(&self.source),
                    }?;
                    if mode == "setup" || mode == "reprogram" {
                        Self::release(&next)?;
                    }
                    self.cpu = Some(next);
                    self.sent = None;
                    self.mode = mode;
                }
                let state = serde_json::to_value(self.cpu()?.execute_batch(budget)?).unwrap();
                return self.executed(state);
            }
            Command::Button { button } => {
                let index = ["up", "down", "left", "right", "ok", "back"]
                    .iter()
                    .position(|s| *s == button)
                    .ok_or("Unknown virtual button.")?;
                if self.cpu.is_none() || !["setup", "reprogram"].contains(&self.mode.as_str()) {
                    return Err("虚拟按键仅用于首次设置或扩展初始化实验。".into());
                }
                let state = press_button(self.cpu()?, BUTTONS[index])?;
                return self.executed(state);
            }
            Command::CommandDial { dial, direction } => {
                if self.cpu.is_none() || self.mode != "reprogram" {
                    return Err("请先恢复拍摄画面。".into());
                }
                let state = rotate_dial(self.cpu()?, &dial, direction)?;
                return self.executed(state);
            }
            Command::LoadImage {
                load_address,
                entry,
            } => {
                let session = ImageSession::new(&Image {
                    bytes: raw,
                    load_address,
                    entry,
                })?;
                self.cpu = None;
                self.firmware = None;
                self.source.clear();
                self.mode = "image".into();
                self.sent = None;
                let identity = session.identity().to_owned();
                let state = session.run_state();
                self.image = Some(session);
                let (mut response, raw) = self.image_executed(state)?;
                response["identity"] = json!(identity);
                return Ok((response, raw));
            }
            Command::ButtonState { button, pressed } => {
                self.image()?.set_button(&button, pressed)?;
            }
            Command::Touch { contacts } => {
                if contacts.len() > 2 {
                    return Err("The touch panel reports at most two fingers".into());
                }
                let mut slots = [None; 2];
                for (slot, contact) in slots.iter_mut().zip(contacts) {
                    *slot = Some(contact);
                }
                self.image()?.set_touch(slots)?;
            }
            Command::ReadMemory { address, length } => {
                if length > MAX_MEMORY_READ {
                    return Err(format!(
                        "Memory reads are limited to {MAX_MEMORY_READ} bytes"
                    ));
                }
                let bytes = self.image()?.read_physical_memory(address, length)?;
                return Ok((
                    json!({"type":"bytes", "bytes":{"$bytes":bytes.len()}}),
                    bytes,
                ));
            }
            Command::WriteMemory { address } => {
                self.image()?.write_physical_memory(address, raw)?;
            }
            Command::SaveCheckpoint => {
                let bytes = match &self.image {
                    Some(image) => checkpoint_file::save(image, Default::default())?,
                    None => checkpoint_file::save(self.cpu()?, Default::default())?,
                };
                return Ok((
                    json!({"type":"bytes", "bytes":{"$bytes":bytes.len()}}),
                    bytes,
                ));
            }
        }
        Ok((json!({"type":"ok"}), vec![]))
    }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut trace = std::env::var_os("XT4_TRACE")
        .map(|path| OpenOptions::new().create(true).append(true).open(path))
        .transpose()?;
    let port: u16 = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "5199".into())
        .parse()?;
    let listener = TcpListener::bind(("127.0.0.1", port))?;
    eprintln!(
        "X-T4 camera listening on ws://127.0.0.1:{}",
        listener.local_addr()?.port()
    );
    // One session at a time; the CPU and its JIT stay on this thread.
    for stream in listener.incoming() {
        let stream = stream?;
        stream.set_nodelay(true)?;
        stream.set_read_timeout(Some(Duration::from_secs(10)))?;
        let config = WebSocketConfig::default()
            .max_message_size(Some(MAX_MESSAGE))
            .max_frame_size(Some(MAX_MESSAGE));
        let socket = tungstenite::accept_hdr_with_config(
            stream,
            |request: &Handshake, response: Upgrade| {
                if let Some(origin) = request.headers().get("origin") {
                    let local = origin
                        .to_str()
                        .ok()
                        .and_then(|s| s.strip_prefix("http://"))
                        .is_some_and(|s| {
                            s == "localhost"
                                || s == "127.0.0.1"
                                || s.strip_prefix("localhost:")
                                    .or_else(|| s.strip_prefix("127.0.0.1:"))
                                    .is_some_and(|port| port.parse::<u16>().is_ok())
                        });
                    if !local {
                        return Err(tungstenite::http::Response::builder()
                            .status(403)
                            .body(Some("Local origins only".into()))
                            .unwrap());
                    }
                }
                Ok(response)
            },
            Some(config),
        );
        let mut socket = match socket {
            Ok(s) => s,
            Err(error) => {
                eprintln!("WebSocket handshake: {error}");
                continue;
            }
        };
        socket.get_mut().set_read_timeout(None)?;
        let mut camera = Camera::default();
        loop {
            match socket.read() {
                Ok(Message::Binary(bytes)) => {
                    let decoded = decode(&bytes);
                    let id = decoded
                        .as_ref()
                        .ok()
                        .and_then(|(v, _)| v.get("id"))
                        .filter(|v| v.as_u64().is_some_and(|id| id <= 9_007_199_254_740_991))
                        .cloned()
                        .unwrap_or(Value::Null);
                    let result = if id.is_null() {
                        Err("Invalid request id".into())
                    } else {
                        decoded.and_then(|(request, raw)| {
                            if let Some(file) = &mut trace {
                                trace_command(file, &request, raw).map_err(|e| e.to_string())?;
                            }
                            camera.handle(request, raw)
                        })
                    };
                    let (mut response, raw) = result
                        .unwrap_or_else(|error| (json!({"type":"error", "message":error}), vec![]));
                    response["id"] = id;
                    if socket
                        .send(Message::Binary(encode(&response, &raw).into()))
                        .is_err()
                    {
                        break;
                    }
                }
                Ok(Message::Close(_)) | Err(tungstenite::Error::ConnectionClosed) => break,
                Ok(Message::Ping(_)) | Ok(Message::Pong(_)) => (),
                Ok(_) => {
                    let _ = socket.close(None);
                    break;
                }
                Err(error) => {
                    eprintln!("WebSocket client: {error}");
                    break;
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn wire_checks_lengths_and_preserves_bytes() {
        let value = json!({"id":1,"type":"load","buffer":{"$bytes":3}});
        let wire = encode(&value, &[1, 0, 255]);
        assert_eq!(decode(&wire).unwrap(), (value.clone(), &[1, 0, 255][..]));
        for end in 0..wire.len() {
            assert!(decode(&wire[..end]).is_err());
        }
        let mut extra = wire.clone();
        extra.push(0);
        assert!(decode(&extra).is_err());
        let mut trace = vec![];
        trace_command(&mut trace, &value, &[1, 0, 255]).unwrap();
        assert_eq!(
            serde_json::from_slice::<Value>(&trace).unwrap(),
            json!({"type":"load","sha256":fujimod_firmware::sha256(&[1,0,255])})
        );
    }

    #[test]
    #[ignore = "browser command trace, official DAT and REC checkpoint"]
    fn replay_menu_trace() {
        use std::{fs, path::Path};
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let paths = std::env::var_os("XT4_REPLAY").map_or_else(
            || {
                ["xt4-menu-queued-350.jsonl", "xt4-menu-queued-400.jsonl"]
                    .map(|name| {
                        Path::new(env!("CARGO_MANIFEST_DIR"))
                            .join("tests/fixtures")
                            .join(name)
                    })
                    .to_vec()
            },
            |path| vec![path.into()],
        );
        for path in paths {
            let path = root.join(path);
            let path = path.as_path();
            let mut camera = Camera::default();
            for line in fs::read_to_string(path).unwrap().lines() {
                let request: Value = serde_json::from_str(line).unwrap();
                let raw = match request["type"].as_str().unwrap() {
                    "load" => fs::read(root.join("FWUP0024.DAT")).unwrap(),
                    "checkpoint" => fs::read(root.join(".cache/xt4-rec.checkpoint")).unwrap(),
                    _ => vec![],
                };
                if !raw.is_empty() {
                    assert_eq!(request["sha256"], fujimod_firmware::sha256(&raw));
                } else {
                    assert!(
                        request["sha256"].is_null(),
                        "Replay needs the hashed raw asset"
                    );
                }
                let (response, _) = camera.handle(request, &raw).unwrap();
                assert_healthy(&response);
            }
            let cpu = camera.cpu().unwrap();
            // ACK finishes the electrical pulse; the queued BACK can still be drawing REC.
            for _ in 0..400 {
                assert!(!cpu.execute_batch(1_000_000).unwrap().stopped());
            }
            let before = cpu.frame_rgba(1).unwrap();
            assert_healthy(&press_button(cpu, BUTTONS[4]).unwrap());
            for _ in 0..2000 {
                assert!(!cpu.execute_batch(1_000_000).unwrap().stopped());
            }
            let answered = cpu.frame_rgba(1).unwrap() != before;
            let out = root
                .join(".cache/setup-trace/replay")
                .join(path.file_stem().unwrap());
            fs::create_dir_all(&out).unwrap();
            fs::write(out.join("state.json"), cpu.state().to_string()).unwrap();
            fs::write(
                out.join("devices.json"),
                cpu.diagnostics().unwrap().to_string(),
            )
            .unwrap();
            fs::write(
                out.join("kernel.bin"),
                (0..0x100000)
                    .step_by(0x10000)
                    .flat_map(|at| cpu.read_physical_memory(at, 0x10000).unwrap())
                    .collect::<Vec<_>>(),
            )
            .unwrap();
            fs::write(out.join("lcd.rgba"), cpu.frame_rgba(1).unwrap()).unwrap();
            if !answered {
                fs::write(
                    out.join("failure.checkpoint"),
                    checkpoint_file::save(cpu, Default::default()).unwrap(),
                )
                .unwrap();
            }
            assert!(answered, "MENU after the trace must answer");
        }
    }

    /// The image commands with `firmware/dist`'s PocketJS image; everything
    /// image-specific (addresses, the status words) comes from its manifest.
    #[test]
    #[ignore = "needs firmware/dist (bun run firmware:build)"]
    fn image_protocol() {
        use std::{fs, path::Path};
        fn call(camera: &mut Camera, request: Value, raw: &[u8]) -> (Value, Vec<u8>) {
            camera.handle(request, raw).unwrap()
        }
        fn run(camera: &mut Camera, ticks: u64) -> (Value, Vec<u8>) {
            let request =
                json!({"type":"execute","mode":"image","budget":50_000_000,"ticks":ticks});
            let response = call(camera, request, &[]);
            assert_healthy(&response.0);
            response
        }
        fn read(camera: &mut Camera, address: u32) -> u32 {
            let request = json!({"type":"read-memory","address":address,"length":4});
            u32::from_le_bytes(call(camera, request, &[]).1.try_into().unwrap())
        }
        fn press(camera: &mut Camera) {
            for pressed in [true, false] {
                let request = json!({"type":"button-state","button":"ok","pressed":pressed});
                call(camera, request, &[]);
                run(camera, 5_400_000);
            }
        }
        let hex = |value: &Value| {
            u32::from_str_radix(value.as_str().unwrap().trim_start_matches("0x"), 16).unwrap()
        };
        let dist = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../firmware/dist");
        let manifest: Value =
            serde_json::from_slice(&fs::read(dist.join("fuji-boost.json")).unwrap()).unwrap();
        let image = &manifest["images"]["pocket"];
        let bytes = fs::read(dist.join(image["file"].as_str().unwrap())).unwrap();
        let mailbox = &manifest["protocol"]["mailbox"];
        let word = |name: &str| {
            let words = mailbox["words"].as_array().unwrap();
            let index = words.iter().position(|word| word == name).unwrap();
            hex(&mailbox["address"]) + 4 * index as u32
        };
        let mut camera = Camera::default();
        let page_zero = json!({"type":"load-image","loadAddress":0,"entry":0});
        assert!(camera.handle(page_zero, &bytes).is_err());
        let load = json!({"type":"load-image",
            "loadAddress":hex(&image["loadAddress"]),"entry":hex(&image["entry"])});
        let (loaded, _) = call(&mut camera, load, &bytes);
        assert!(loaded["identity"].as_str().unwrap().starts_with("image/"));
        // The app presents after ~100M instructions; capped requests repeat.
        let mut frame = vec![];
        for _ in 0..40 {
            let (response, raw) = run(&mut camera, 5_400_000);
            if !raw.is_empty() {
                assert_eq!(response["frame"]["width"], 900);
                frame = raw;
            }
            if read(&mut camera, word("presents")) >= 1 {
                break;
            }
        }
        assert_eq!(frame.len(), 900 * 600 * 4);
        run(&mut camera, 3_600_000);
        let ticks = |response: &Value| response["state"]["cpu"]["virtual_ticks"].as_u64().unwrap();
        let before = ticks(&run(&mut camera, 1).0);
        let advanced = ticks(&run(&mut camera, 1_800_000).0) - before;
        assert!((1_800_000..3_600_000).contains(&advanced), "{advanced}");

        press(&mut camera);
        assert_eq!(read(&mut camera, word("magic")).to_le_bytes(), *b"FBMB");
        assert_eq!(read(&mut camera, word("actionValue")), 1);
        for stock in [
            json!({"type":"execute","mode":"strict","budget":1}),
            json!({"type":"button","button":"ok"}),
            json!({"type":"scene","source":"gray"}),
        ] {
            assert!(camera.handle(stock, &[]).is_err());
        }
        let (_, checkpoint) = call(&mut camera, json!({"type":"save-checkpoint"}), &[]);
        press(&mut camera);
        assert_eq!(read(&mut camera, word("actionValue")), 2);
        let (restored, _) = call(&mut camera, json!({"type":"checkpoint"}), &checkpoint);
        assert_healthy(&restored);
        assert_eq!(read(&mut camera, word("actionValue")), 1);
    }

    fn assert_healthy(response: &Value) {
        let state = response.get("state").unwrap_or(response);
        for value in [
            &state["error"],
            &state["guestHalt"],
            &state["cpu"]["failure"],
            &state["cpu"]["abort_failure"],
        ] {
            assert!(value.is_null(), "Guest stopped: {value}");
        }
    }
}
