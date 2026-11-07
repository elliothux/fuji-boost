use fujimod_usb::{
    connection::{self, Connection, Device, DeviceFilter, TransferOptions},
    ff80::{
        Session, SessionState,
        commands::{
            info::InfoSelector,
            memory::{Address, DebugMemoryLimits, MemoryRange},
        },
    },
    operations::memory::MemoryReader,
};
use serde_json::{Value, json};
use std::{collections::BTreeMap, error::Error, num::NonZeroU32, path::PathBuf, time::Duration};

mod output;
mod ptp;
use output::Capture;
type Result<T> = std::result::Result<T, Box<dyn Error>>;
const RAM_END: u64 = 0x2000_0000;
const MAX_CHUNK: u32 = 65536;
const HELP: &str = "fuji-capture list\nfuji-capture ptp-info --serial SERIAL --out NEW_DIRECTORY\nfuji-capture ptp-firmware --serial SERIAL --out NEW_DIRECTORY\nfuji-capture service-info --serial SERIAL --out PREFIX\nfuji-capture ram --serial SERIAL --address 0xADDRESS --size BYTES --read-limit BYTES [--chunk 256] --out FILE\n\nOnly local 04cb:ff80 service devices are accepted for service-info/ram. PTP commands accept only 04cb:02e6. ptp-info sends GetDeviceInfo without OpenSession; ptp-firmware opens a temporary owned session to read only the raw D153 value (datatype unknown) then closes it when completion is certain. No mode switch, debug enable, camera property writes, reset, or flash commands. RAM range must be explicitly selected inside [0,0x20000000). --read-limit is a caller-selected command bound, not an assertion that this firmware supports it. Outputs must not exist. No retries or resume.";

#[derive(Debug, PartialEq)]
enum Command {
    List,
    Info(Options),
    Ptp(Options),
    PtpFirmware(Options),
    Ram(Options),
}
#[derive(Debug, PartialEq)]
struct Options {
    serial: String,
    out: PathBuf,
    address: u32,
    size: u32,
    chunk: u32,
    read_limit: u32,
}
fn number(value: &str) -> Result<u32> {
    Ok(if let Some(hex) = value.strip_prefix("0x") {
        u32::from_str_radix(hex, 16)?
    } else {
        value.parse()?
    })
}
fn parse(args: Vec<String>) -> Result<Command> {
    let Some(kind) = args.first() else {
        return Err(HELP.into());
    };
    if kind == "list" && args.len() == 1 {
        return Ok(Command::List);
    }
    if !matches!(
        kind.as_str(),
        "ram" | "service-info" | "ptp-info" | "ptp-firmware"
    ) {
        return Err(HELP.into());
    }
    let mut values = BTreeMap::new();
    let (pairs, remainder) = args[1..].as_chunks::<2>();
    for pair in pairs {
        let key = pair[0].as_str();
        let valid = matches!(key, "--serial" | "--out")
            || (kind == "ram"
                && matches!(key, "--address" | "--size" | "--chunk" | "--read-limit"));
        if !valid || values.insert(key, pair[1].as_str()).is_some() {
            return Err(format!("unknown or duplicate option {key}").into());
        }
    }
    if !remainder.is_empty() {
        return Err("option requires a value".into());
    }
    let required = |key| {
        values
            .get(key)
            .copied()
            .ok_or_else(|| format!("missing {key}"))
    };
    let mut options = Options {
        serial: required("--serial")?.into(),
        out: required("--out")?.into(),
        address: 0,
        size: 0,
        chunk: 256,
        read_limit: 0,
    };
    if options.serial.trim().is_empty() || options.out.as_os_str().is_empty() {
        return Err("serial and output must be nonempty".into());
    }
    if kind == "service-info" {
        return Ok(Command::Info(options));
    }
    if kind == "ptp-info" {
        return Ok(Command::Ptp(options));
    }
    if kind == "ptp-firmware" {
        return Ok(Command::PtpFirmware(options));
    }
    options.address = number(required("--address")?)?;
    options.size = number(required("--size")?)?;
    options.read_limit = number(required("--read-limit")?)?;
    options.chunk = number(values.get("--chunk").copied().unwrap_or("256"))?;
    validate_range(&options)?;
    Ok(Command::Ram(options))
}
fn validate_range(o: &Options) -> Result<()> {
    if o.size == 0 || u64::from(o.address) + u64::from(o.size) > RAM_END {
        return Err("RAM range must be nonempty and wholly inside [0,0x20000000); no MMIO or inferred capacity".into());
    }
    if o.read_limit == 0 || o.chunk == 0 || o.chunk > o.read_limit || o.chunk > MAX_CHUNK {
        return Err("chunk must be 1..=65536 and <= explicit --read-limit".into());
    }
    Ok(())
}
fn device_json(d: &Device) -> Value {
    json!({"vendor_id":d.vendor_id(),"product_id":d.product_id(),"serial":d.serial_number(),"manufacturer":d.manufacturer_string(),"product":d.product_string(),"bus":d.bus_id(),"address":d.device_address(),"ports":d.port_chain()})
}
fn service_id(vendor: u16, product: u16) -> bool {
    vendor == 0x04cb && product == 0xff80
}
fn select(devices: Vec<Device>, serial: &str) -> Result<Device> {
    let mut found = devices
        .into_iter()
        .filter(|d| service_id(d.vendor_id(), d.product_id()) && d.serial_number() == Some(serial));
    let d = found.next().ok_or("no unique local 04cb:ff80 device with this serial; normal 02e6 is deliberately refused; no mode transition is performed")?;
    if found.next().is_some() {
        return Err(
            "multiple service devices have this serial; refusing ambiguous selection".into(),
        );
    }
    Ok(d)
}
async fn list() -> Result<()> {
    let mut records = Vec::new();
    for d in connection::discover(DeviceFilter::Fujifilm).await? {
        let mut value = device_json(&d);
        value["bulk_interface_selections"] = match connection::interfaces(&d).await {
            Ok(list) => json!(list.iter().map(|i| json!({"configuration":i.configuration,"interface":i.interface,"alternate":i.alternate})).collect::<Vec<_>>()),
            Err(e) => json!({"descriptor_error":e.to_string()}),
        };
        records.push(value);
    }
    println!("{}", serde_json::to_string_pretty(&records)?);
    Ok(())
}
async fn metadata(session: &mut Session, output: &mut Capture) -> Result<()> {
    for (name, selector) in [
        ("product", InfoSelector::Product),
        ("internal-version", InfoSelector::InternalVersion),
    ] {
        let reply = session.device_info(selector).await?;
        output.info(name, &reply.raw, &reply.text)?;
    }
    Ok(())
}
async fn ram(session: &mut Session, options: &Options, output: &mut Capture) -> Result<()> {
    let range = MemoryRange::new(
        Address(options.address),
        NonZeroU32::new(options.size).unwrap(),
    )?;
    let limits = DebugMemoryLimits {
        read: NonZeroU32::new(options.chunk).unwrap(),
        write: NonZeroU32::new(1).unwrap(),
    };
    let maximum = options
        .chunk
        .min(session.info().advertised_packet_bytes.get())
        .min(session.max_reply_bytes().get().saturating_sub(36));
    if maximum == 0 {
        return Err("device advertised no usable memory-read budget".into());
    }
    let mut reader = MemoryReader::new(session, range, limits);
    loop {
        let remaining = options.size - reader.bytes_completed();
        let expected = remaining.min(maximum) as usize;
        match reader.next_chunk().await? {
            Some(data) => output.append(&data, expected)?,
            None => break,
        }
    }
    Ok(())
}
async fn capture(options: Options, is_ram: bool) -> Result<()> {
    let device = select(
        connection::discover(DeviceFilter::Fujifilm).await?,
        &options.serial,
    )?;
    let selections = connection::interfaces(&device).await?;
    if selections.len() != 1 {
        return Err(format!(
            "expected exactly one bulk interface selection, found {}; refusing to guess",
            selections.len()
        )
        .into());
    }
    let selection = selections[0];
    let mut output = Capture::new(
        &options.out,
        json!({"schema":1,"kind":if is_ram {"ram"}else{"service-info"},"device":device_json(&device),"selection":{"configuration":selection.configuration,"interface":selection.interface,"alternate":selection.alternate},"address":if is_ram {Some(options.address)}else{None},"requested_bytes":options.size,"chunk":options.chunk,"caller_read_limit":options.read_limit,"ram_range_policy":"explicit range within [0,0x20000000); not hardware capacity verification","no_retry":true}),
        is_ram,
    )?;
    let result: Result<()> = async {
        let connection = Connection::open(&device, selection, TransferOptions { timeout: Duration::from_secs(3), max_reply_bytes: NonZeroU32::new(MAX_CHUNK + 512).unwrap() }).await?;
        let mut session = connection.begin_session(None).await?;
        let info = session.info();
        output.event(json!({"event":"session-open","id":info.id,"advertised_packet_bytes":info.advertised_packet_bytes.get(),"raw_open_reply":info.raw_open_reply}))?;
        let work: Result<()> = async {
            metadata(&mut session, &mut output).await?;
            if is_ram { ram(&mut session, &options, &mut output).await?; }
            Ok(())
        }.await;
        let state = session.state();
        output.event(json!({"event":"session-state","state":format!("{state:?}")}))?;
        // An uncertain transfer may still be running; dropping releases only host resources.
        let close = if state == SessionState::Ready { session.close().await.map(Some) } else { drop(session); Ok(None) };
        if let Err(e) = close { output.event(json!({"event":"close-failed","error":e.to_string()}))?; return Err(e.into()); }
        work
    }.await;
    match result {
        Ok(()) => {
            output.finish(true, None)?;
            println!("complete: {}", options.out.display());
            Ok(())
        }
        Err(e) => {
            let original = e.to_string();
            output.finish(false, Some(&original))?;
            Err(original.into())
        }
    }
}
fn main() {
    let result = (|| -> Result<()> {
        let args: Vec<_> = std::env::args().skip(1).collect();
        if args == ["--help"] || args == ["-h"] {
            println!("{HELP}");
            return Ok(());
        }
        let command = parse(args)?;
        // Avoid the SDK's implicit remote selection: this acquisition tool is local-only.
        if std::env::var_os("FUJIMOD_USB_PROXY_URL").is_some() {
            return Err("unset FUJIMOD_USB_PROXY_URL; this tool accepts local devices only".into());
        }
        futures_lite::future::block_on(async {
            match command {
                Command::List => list().await,
                Command::Info(o) => capture(o, false).await,
                Command::Ptp(o) => ptp::capture(&o.serial, &o.out).await,
                Command::PtpFirmware(o) => ptp::firmware(&o.serial, &o.out).await,
                Command::Ram(o) => capture(o, true).await,
            }
        })
    })();
    if let Err(error) = result {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn args(s: &str) -> Vec<String> {
        s.split_whitespace().map(String::from).collect()
    }
    #[test]
    fn normal_ptp_device_is_never_eligible_for_ff80() {
        assert!(!service_id(0x04cb, 0x02e6));
        assert!(!service_id(0x1234, 0xff80));
        assert!(service_id(0x04cb, 0xff80));
    }
    #[test]
    fn parser_requires_explicit_range_and_read_limit() {
        assert!(parse(args("ram --serial S --out x --address 0 --size 1")).is_err());
        assert!(parse(args("ram --serial S --out x --size 1 --read-limit 256")).is_err());
        assert!(
            parse(args(
                "ram --serial S --out x --address 0 --size 0 --read-limit 256"
            ))
            .is_err()
        );
        assert!(
            parse(args(
                "ram --serial S --out x --address 0x1fffffff --size 2 --read-limit 256"
            ))
            .is_err()
        );
        assert!(
            parse(args(
                "ram --serial S --out x --address 0xfffffff0 --size 32 --read-limit 256"
            ))
            .is_err()
        );
        assert!(
            parse(args(
                "ram --serial S --out x --address 0x1fffffff --size 1 --read-limit 256"
            ))
            .is_ok()
        );
    }
    #[test]
    fn parser_rejects_guessing_and_extra_options() {
        for s in [
            "list --serial S",
            "flash --serial S",
            "service-info --serial S --out x --address 0",
            "service-info --serial S --serial T --out x",
            "ram --serial S --out x --address 0 --size 1 --read-limit 128",
            "ram --serial S --out x --address 0 --size 1 --read-limit 100000 --chunk 100000",
        ] {
            assert!(parse(args(s)).is_err(), "{s}");
        }
        assert_eq!(parse(args("list")).unwrap(), Command::List);
    }
}
