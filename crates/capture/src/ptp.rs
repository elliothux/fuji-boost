//! Fixed standard PTP identity and read-only firmware-property queries.
use crate::{Result, json};
use nusb::{
    Endpoint,
    transfer::{Buffer, Bulk, In, Out},
};
use serde_json::Value;
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::Path,
    time::{Duration, Instant},
};
const MAX: usize = 1024 * 1024;
const FIRMWARE_PROPERTY: u32 = 0xd153;
fn save(dir: &Path, name: &str, bytes: &[u8]) -> Result<()> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(dir.join(name))?;
    file.write_all(bytes)?;
    file.sync_all()?;
    Ok(())
}
async fn deadline<T>(
    future: impl std::future::Future<Output = T>,
    duration: Duration,
) -> Result<T> {
    futures_lite::future::race(async { Ok(future.await) }, async {
        futures_timer::Delay::new(duration).await;
        Err("PTP transfer timed out; completion unknown; no reset, close, or retry sent".into())
    })
    .await
}
fn command(op: u16, tid: u32, params: &[u32]) -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&((12 + 4 * params.len()) as u32).to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&op.to_le_bytes());
    bytes.extend_from_slice(&tid.to_le_bytes());
    for p in params {
        bytes.extend_from_slice(&p.to_le_bytes());
    }
    bytes
}
fn header(bytes: &[u8]) -> Result<Option<(usize, u16, u16, u32)>> {
    if bytes.len() < 12 {
        return Ok(None);
    }
    let length = u32::from_le_bytes(bytes[..4].try_into()?) as usize;
    if !(12..=MAX).contains(&length) {
        return Err("invalid PTP container length (must be 12..=1MiB)".into());
    }
    Ok(Some((
        length,
        u16::from_le_bytes(bytes[4..6].try_into()?),
        u16::from_le_bytes(bytes[6..8].try_into()?),
        u32::from_le_bytes(bytes[8..12].try_into()?),
    )))
}
#[derive(Debug)]
struct Reply {
    code: u16,
    payload: Option<Vec<u8>>,
}
// Raw USB transfer bytes are saved before invoking this container parser.
fn consume(
    buffer: &mut Vec<u8>,
    data: &mut Option<Vec<u8>>,
    op: u16,
    tid: u32,
    requires_data: bool,
) -> Result<Option<u16>> {
    loop {
        let Some((len, kind, code, actual_tid)) = header(buffer)? else {
            return Ok(None);
        };
        if actual_tid != tid {
            return Err("PTP transaction ID mismatch".into());
        }
        if buffer.len() < len {
            return Ok(None);
        }
        match kind {
            2 if code == op && data.is_none() && requires_data => {
                *data = Some(buffer[12..len].to_vec());
            }
            3 => {
                if len > 32 || (len - 12) % 4 != 0 || !(0x2000..0x3000).contains(&code) {
                    return Err("invalid PTP response code or parameters".into());
                }
                if code == 0x2001 && requires_data && data.is_none() {
                    return Err("PTP OK without required data container".into());
                }
                if buffer.len() != len {
                    return Err("unexpected bytes after PTP response".into());
                }
                buffer.clear();
                return Ok(Some(code));
            }
            _ => return Err("unexpected PTP container type/opcode or repeated data".into()),
        }
        buffer.drain(..len);
    }
}
struct Link {
    input: Endpoint<Bulk, In>,
    output: Endpoint<Bulk, Out>,
    packet: usize,
    ready: bool,
}
impl Link {
    async fn open(serial: &str, dir: &Path) -> Result<Self> {
        let mut matches = nusb::list_devices().await?.filter(|d| {
            d.vendor_id() == 0x04cb && d.product_id() == 0x02e6 && d.serial_number() == Some(serial)
        });
        let info = matches
            .next()
            .ok_or("no local normal-mode 04cb:02e6 device matching serial")?;
        if matches.next().is_some() {
            return Err("ambiguous normal-mode device serial".into());
        }
        let device = info.open().await?;
        let config = device.active_configuration()?;
        let mut selections = Vec::new();
        for descriptor in config.interface_alt_settings().filter(|i| i.class() == 6) {
            let endpoints: Vec<_> = descriptor
                .endpoints()
                .filter(|e| e.transfer_type() == nusb::descriptors::TransferType::Bulk)
                .map(|e| e.address())
                .collect();
            let inputs: Vec<_> = endpoints
                .iter()
                .copied()
                .filter(|e| e & 0x80 != 0)
                .collect();
            let outputs: Vec<_> = endpoints
                .iter()
                .copied()
                .filter(|e| e & 0x80 == 0)
                .collect();
            if inputs.len() == 1 && outputs.len() == 1 {
                selections.push((
                    descriptor.interface_number(),
                    descriptor.alternate_setting(),
                    inputs[0],
                    outputs[0],
                ));
            }
        }
        if selections.len() != 1 {
            return Err(format!(
                "expected one class-6 bulk interface in active config; found {}",
                selections.len()
            )
            .into());
        }
        let (number, alternate, input_address, output_address) = selections[0];
        let interface = device.claim_interface(number).await?;
        if interface.get_alt_setting() != alternate {
            return Err("selected interface alternate is not active; refusing to change it".into());
        }
        let input: Endpoint<Bulk, In> = interface.endpoint(input_address)?;
        let output: Endpoint<Bulk, Out> = interface.endpoint(output_address)?;
        let packet = input.max_packet_size();
        if packet == 0 || packet > MAX {
            return Err("invalid bulk input packet size".into());
        }
        save(
            dir,
            "selection.json",
            &serde_json::to_vec_pretty(
                &json!({"vendor_id":info.vendor_id(),"product_id":info.product_id(),"serial":info.serial_number(),"configuration":config.configuration_value(),"interface":number,"alternate":alternate,"input":input_address,"output":output_address,"packet_bytes":packet}),
            )?,
        )?;
        Ok(Self {
            input,
            output,
            packet,
            ready: true,
        })
    }
}
trait Transport {
    fn ready(&self) -> bool;
    async fn request(
        &mut self,
        dir: &Path,
        op: u16,
        tid: u32,
        params: &[u32],
        requires_data: bool,
    ) -> Result<Reply>;
}
impl Transport for Link {
    fn ready(&self) -> bool {
        self.ready
    }
    async fn request(
        &mut self,
        dir: &Path,
        op: u16,
        tid: u32,
        params: &[u32],
        requires_data: bool,
    ) -> Result<Reply> {
        if !self.ready {
            return Err("PTP completion unknown; refusing further command".into());
        }
        let bytes = command(op, tid, params);
        save(dir, "command.raw", &bytes)?;
        self.ready = false;
        self.output.submit(bytes.clone().into());
        let sent = deadline(self.output.next_complete(), Duration::from_secs(3)).await?;
        save(
            dir,
            "command-transfer.json",
            &serde_json::to_vec_pretty(
                &json!({"actual_bytes":sent.actual_len,"status":format!("{:?}",sent.status)}),
            )?,
        )?;
        sent.status?;
        if sent.actual_len != bytes.len() {
            return Err("short PTP command write; no retry".into());
        }
        self.receive(dir, op, tid, requires_data).await
    }
}
impl Link {
    async fn receive(
        &mut self,
        dir: &Path,
        op: u16,
        tid: u32,
        requires_data: bool,
    ) -> Result<Reply> {
        self.ready = false;
        let mut data = None;
        let start = Instant::now();
        let mut buffer = Vec::new();
        let mut total = 0usize;
        for index in 0..64 {
            let remaining = Duration::from_secs(10)
                .checked_sub(start.elapsed())
                .ok_or("PTP transaction deadline reached; no retry")?;
            self.input
                .submit(Buffer::new(MAX / self.packet * self.packet));
            let read = deadline(
                self.input.next_complete(),
                remaining.min(Duration::from_secs(3)),
            )
            .await?;
            let bytes = read.buffer.to_vec();
            save(dir, &format!("bulk-in-{index:03}.raw"), &bytes)?;
            save(
                dir,
                &format!("bulk-in-{index:03}.json"),
                &serde_json::to_vec_pretty(
                    &json!({"actual_bytes":read.actual_len,"status":format!("{:?}",read.status)}),
                )?,
            )?;
            read.status?;
            total = total
                .checked_add(bytes.len())
                .ok_or("PTP byte count overflow")?;
            if total > MAX {
                return Err("PTP transaction exceeds 1MiB total bound".into());
            }
            buffer.extend_from_slice(&bytes);
            if let Some(code) = consume(&mut buffer, &mut data, op, tid, requires_data)? {
                self.ready = true;
                if let Some(payload) = &data {
                    save(dir, "data-payload.raw", payload)?;
                }
                save(
                    dir,
                    "response.json",
                    &serde_json::to_vec_pretty(
                        &json!({"operation_code":op,"transaction_id":tid,"response_code":code,"data_bytes":data.as_ref().map(Vec::len),"total_received_bytes":total}),
                    )?,
                )?;
                return Ok(Reply {
                    code,
                    payload: data,
                });
            }
        }
        Err("PTP receive transfer bound reached before response; no retry".into())
    }
}
fn require_ok(reply: Reply, operation: &str) -> Result<Reply> {
    if reply.code != 0x2001 {
        Err(format!("{operation}: PTP response 0x{:04x}", reply.code).into())
    } else {
        Ok(reply)
    }
}
fn require_dataset(reply: &Reply, op: u16) -> Result<()> {
    let data = reply.payload.as_deref().ok_or("missing PTP dataset")?;
    if data.is_empty() {
        return Err("empty PTP dataset is not firmware/identity evidence".into());
    }
    if op == 0x1014
        && (data.len() < 5
            || u16::from_le_bytes([data[0], data[1]]) != FIRMWARE_PROPERTY as u16
            || data[4] > 1)
    {
        return Err("D153 descriptor lacks minimum property-code/datatype/get-set fields".into());
    }
    Ok(())
}
async fn firmware_sequence(link: &mut impl Transport, dir: &Path) -> Value {
    let mut owned = false;
    let mut steps = Vec::new();
    let work:Result<()>=async{
        let step=dir.join("01-open-session");fs::create_dir(&step)?;
        let reply=link.request(&step,0x1002,0,&[1],false).await?;
        steps.push(json!({"operation_code":0x1002,"transaction_id":0,"response_code":reply.code}));
        require_ok(reply,"OpenSession")?;owned=true;
        let step=dir.join("02-get-value-d153");fs::create_dir(&step)?;
        let reply=link.request(&step,0x1015,1,&[FIRMWARE_PROPERTY],true).await?;
        steps.push(json!({"operation_code":0x1015,"transaction_id":1,"response_code":reply.code,"data_bytes":reply.payload.as_ref().map(Vec::len)}));
        let reply=require_ok(reply,"GetDevicePropValue D153")?;
        require_dataset(&reply,0x1015)?;
        Ok(())
    }.await;
    let mut close_error = None;
    let mut closed = false;
    if owned && link.ready() {
        let close: Result<()> = async {
            let step = dir.join("03-close-session");
            fs::create_dir(&step)?;
            let reply = link.request(&step, 0x1003, 2, &[], false).await?;
            steps.push(
                json!({"operation_code":0x1003,"transaction_id":2,"response_code":reply.code}),
            );
            require_ok(reply, "CloseSession")?;
            closed = true;
            Ok(())
        }
        .await;
        if let Err(error) = close {
            close_error = Some(error.to_string());
        }
    }
    json!({"complete":work.is_ok() && closed,"operation":"ReadFirmwareVersionProperty","property_code":FIRMWARE_PROPERTY,"property_name_source":"libgphoto2 PTP_DPC_FUJI_FirmwareVersion; label only, raw value not interpreted","datatype":"unknown","firmware_identity_established":false,"dataset_validation":"nonempty raw value only; descriptor not queried and datatype not established","steps":steps,"opened_own_session":owned,"closed_own_session":closed,"completion_unknown":!link.ready(),"error":work.err().map(|e|e.to_string()),"close_error":close_error,"retries":0,"writes_camera_properties":false})
}
async fn run(serial: &str, dir: &Path, firmware: bool) -> Result<()> {
    fs::create_dir(dir)?;
    let result:Result<Value>=async{
        let mut link=Link::open(serial,dir).await?;
        if firmware{return Ok(firmware_sequence(&mut link,dir).await);}
        let reply=require_ok(link.request(dir,0x1001,0,&[],true).await?,"GetDeviceInfo")?;
        require_dataset(&reply,0x1001)?;
        Ok(json!({"complete":true,"ptp_response":reply.code,"data_container_present":reply.payload.is_some(),"operation":"GetDeviceInfo","operation_code":0x1001,"transaction_id":0,"opens_session":false,"retries":0}))
    }.await;
    let report = match result {
        Ok(report) => report,
        Err(error) => {
            json!({"complete":false,"error":error.to_string(),"operation":if firmware{"ReadFirmwareVersionProperty"}else{"GetDeviceInfo"},"retries":0})
        }
    };
    save(dir, "report.json", &serde_json::to_vec_pretty(&report)?)?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    if report["complete"] == true {
        Ok(())
    } else {
        Err("PTP query failed; see report.json and raw transfers; no command retried".into())
    }
}
pub async fn capture(serial: &str, dir: &Path) -> Result<()> {
    run(serial, dir, false).await
}
pub async fn firmware(serial: &str, dir: &Path) -> Result<()> {
    run(serial, dir, true).await
}

#[cfg(test)]
mod tests {
    use super::*;
    fn container(kind: u16, code: u16, tid: u32, payload: &[u8]) -> Vec<u8> {
        let mut b = Vec::new();
        b.extend_from_slice(&((12 + payload.len()) as u32).to_le_bytes());
        b.extend_from_slice(&kind.to_le_bytes());
        b.extend_from_slice(&code.to_le_bytes());
        b.extend_from_slice(&tid.to_le_bytes());
        b.extend_from_slice(payload);
        b
    }
    #[test]
    fn rejects_empty_ok_wrong_transaction_and_oversized_header() {
        let mut data = None;
        assert!(
            consume(
                &mut container(3, 0x2001, 0, &[]),
                &mut data,
                0x1001,
                0,
                true
            )
            .is_err()
        );
        assert!(
            consume(
                &mut container(2, 0x1001, 1, &[]),
                &mut data,
                0x1001,
                0,
                true
            )
            .is_err()
        );
        let mut b = container(2, 0x1001, 0, &[]);
        b[..4].copy_from_slice(&((MAX + 1) as u32).to_le_bytes());
        assert!(consume(&mut b, &mut data, 0x1001, 0, true).is_err());
    }
    #[test]
    fn assembled_data_and_response_validate_without_usb() {
        let mut b = container(2, 0x1001, 0, &[1, 2, 3]);
        let mut data = None;
        assert_eq!(consume(&mut b, &mut data, 0x1001, 0, true).unwrap(), None);
        assert_eq!(data, Some(vec![1, 2, 3]));
        b.extend(container(3, 0x2001, 0, &[]));
        assert_eq!(
            consume(&mut b, &mut data, 0x1001, 0, true).unwrap(),
            Some(0x2001)
        );
        let mut data = None;
        assert_eq!(
            consume(
                &mut container(3, 0x2019, 0, &[]),
                &mut data,
                0x1001,
                0,
                true
            )
            .unwrap(),
            Some(0x2019)
        );
    }
    #[test]
    fn truncated_container_waits_and_duplicate_data_is_refused() {
        let full = container(2, 0x1001, 0, &[1, 2, 3]);
        let mut b = full[..13].to_vec();
        let mut data = None;
        assert_eq!(consume(&mut b, &mut data, 0x1001, 0, true).unwrap(), None);
        assert!(data.is_none());
        b.extend_from_slice(&full[13..]);
        assert_eq!(consume(&mut b, &mut data, 0x1001, 0, true).unwrap(), None);
        assert!(consume(&mut full.clone(), &mut data, 0x1001, 0, true).is_err());
    }
    #[test]
    fn lifecycle_ok_needs_no_data_and_property_reply_needs_matching_opcode() {
        assert_eq!(
            consume(
                &mut container(3, 0x2001, 3, &[]),
                &mut None,
                0x1003,
                3,
                false
            )
            .unwrap(),
            Some(0x2001)
        );
        assert!(
            consume(
                &mut container(2, 0x1014, 2, &[1]),
                &mut None,
                0x1015,
                2,
                true
            )
            .is_err()
        );
        assert_eq!(
            command(0x1002, 0, &[1]),
            [16, 0, 0, 0, 1, 0, 2, 16, 0, 0, 0, 0, 1, 0, 0, 0]
        );
    }
    struct Script {
        responses: std::collections::VecDeque<(u16, bool)>,
        calls: Vec<(u16, u32, Vec<u32>)>,
        ready: bool,
    }
    impl Transport for Script {
        fn ready(&self) -> bool {
            self.ready
        }
        async fn request(
            &mut self,
            _: &Path,
            op: u16,
            tid: u32,
            params: &[u32],
            requires_data: bool,
        ) -> Result<Reply> {
            self.calls.push((op, tid, params.to_vec()));
            let (code, ready) = self.responses.pop_front().expect("unexpected command");
            self.ready = ready;
            if !ready {
                return Err("synthetic timeout".into());
            }
            Ok(Reply {
                code,
                payload: requires_data.then(|| vec![1]),
            })
        }
    }
    fn simulate(responses: Vec<(u16, bool)>) -> (Value, Script) {
        static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../research/firmware/xt4-212-current/capture-test-output");
        fs::create_dir_all(&root).unwrap();
        let dir = root.join(format!(
            "ptp-sequence-test-{}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        fs::create_dir(&dir).unwrap();
        let mut s = Script {
            responses: responses.into(),
            calls: Vec::new(),
            ready: true,
        };
        let report = futures_lite::future::block_on(firmware_sequence(&mut s, &dir));
        fs::remove_dir_all(dir).unwrap();
        (report, s)
    }
    #[test]
    fn firmware_sequence_uses_only_fixed_reads_and_closes_owned_session() {
        let (r, s) = simulate(vec![(0x2001, true); 3]);
        assert_eq!(r["complete"], true);
        assert_eq!(r["datatype"], "unknown");
        assert_eq!(r["firmware_identity_established"], false);
        assert_eq!(
            s.calls,
            [
                (0x1002, 0, vec![1]),
                (0x1015, 1, vec![0xd153]),
                (0x1003, 2, vec![])
            ]
        );
    }
    #[test]
    fn session_already_open_does_not_close_someone_elses_session() {
        let (r, s) = simulate(vec![(0x201e, true)]);
        assert_eq!(r["opened_own_session"], false);
        assert_eq!(s.calls.len(), 1);
    }
    #[test]
    fn uncertainty_does_not_close_but_complete_error_closes_our_session() {
        let (r, s) = simulate(vec![(0x2001, true), (0, false)]);
        assert_eq!(r["completion_unknown"], true);
        assert_eq!(s.calls.len(), 2);
        let (r, s) = simulate(vec![(0x2001, true), (0x200a, true), (0x2001, true)]);
        assert_eq!(r["complete"], false);
        assert_eq!(r["closed_own_session"], true);
        assert_eq!(
            s.calls.iter().map(|v| v.0).collect::<Vec<_>>(),
            [0x1002, 0x1015, 0x1003]
        );
    }
    #[test]
    fn empty_data_then_error_is_complete_framing_but_not_successful_query() {
        let mut buffer = container(2, 0x1014, 1, &[]);
        let mut data = None;
        assert_eq!(
            consume(&mut buffer, &mut data, 0x1014, 1, true).unwrap(),
            None
        );
        assert_eq!(data, Some(vec![]));
        buffer.extend(container(3, 0x2005, 1, &[]));
        let code = consume(&mut buffer, &mut data, 0x1014, 1, true)
            .unwrap()
            .unwrap();
        assert_eq!(code, 0x2005);
        assert!(
            require_ok(
                Reply {
                    code,
                    payload: data
                },
                "GetDevicePropDesc"
            )
            .is_err()
        );
    }
    #[test]
    fn empty_ok_dataset_fails_business_validation_after_response_is_drained() {
        for op in [0x1001, 0x1014, 0x1015] {
            let mut buffer = container(2, op, 1, &[]);
            buffer.extend(container(3, 0x2001, 1, &[]));
            let mut data = None;
            let code = consume(&mut buffer, &mut data, op, 1, true)
                .unwrap()
                .unwrap();
            assert!(buffer.is_empty());
            let reply = require_ok(
                Reply {
                    code,
                    payload: data,
                },
                "read",
            )
            .unwrap();
            assert!(require_dataset(&reply, op).is_err());
        }
        assert!(
            require_dataset(
                &Reply {
                    code: 0x2001,
                    payload: Some(vec![0x53, 0xd1, 6, 0])
                },
                0x1014
            )
            .is_err()
        );
    }
}
