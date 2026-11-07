use fujimod_emulator_cpu::arm;
use fujimod_emulator_xp4_board::Session;
use serde_json::{Value, json};

#[path = "rec_producer.generated.rs"]
mod producer_source;
pub use producer_source::POINTS as PRODUCER_POINTS;

pub const ROUTE_SHA256: &str = "d85944791bf555fc0521dca397359c0e99a099b5190116045b215fcfcb14b126";
pub const CONSUMER_SHA256: &str =
    "fcda635b6971c386a70eb3a7975e55dacc915e799df3c992651cd8eacb1e77df";
pub const ADMISSION_TAIL_SHA256: &str =
    "68d38b3a3c0c98aab490c467dd17584ba8f5709894a50d647e69e8f1c4eb0606";
pub const PRODUCER_SOURCE_SHA256: &str =
    "2c7a936f7ee3caeed249eed1fbb72f9ba792808ea3ca1392dcf3622825d186fb";
pub const PRODUCER_LIMIT: usize = 4096;
pub const PRODUCER_PHASE: &str = "admission-producer-before-instruction";
pub const PRODUCER_MEANING: &str = "Passive pre-instruction CORE/CPSR samples; evaluate original ARM conditions offline. Host diagnostic capacity and dropped count bound coverage.";
const PRODUCER_RANGE: (usize, usize) = (0x2020d6c, 0x202435c);
pub const LIMIT: usize = 1024;
pub const POINTS: [u64; 11] = [
    0x20f2028, 0x20f2038, 0x1e2813c, 0x1e603ec, 0x1e603fc, 0x1e60420, 0x1e604e0, 0x1e604e4,
    0x202403c, 0x2024058, 0x2024074,
];
const ROUTE_RANGES: [(usize, usize); 7] = [
    (0x172c2f4, 0x172c314),
    (0x156c886, 0x156c88c),
    (0x1167c4c, 0x1167e38),
    (0x20f2028, 0x20f2060),
    (0x1292474, 0x1292490),
    (0x12920f8, 0x1292210),
    (0x1e28130, 0x1e28140),
];
const CONSUMER_RANGES: [(usize, usize); 16] = [
    (0x1e28108, 0x1e2916c),
    (0x1e27da0, 0x1e27e7c),
    (0x1144698, 0x11446c8),
    (0x201f4fc, 0x201f514),
    (0x20178a4, 0x20178e4),
    (0x2020b78, 0x2020b9c),
    (0x1e603ec, 0x1e60634),
    (0x2020d6c, 0x2020e00),
    (0x2021db4, 0x2021dc0),
    (0x25a2798, 0x25a279c),
    (0x25a2304, 0x25a2308),
    (0x25a2aa4, 0x25a2aa8),
    (0x25a2610, 0x25a2612),
    (0x254ef7c, 0x254ef80),
    (0x25a22d4, 0x25a22d8),
    (0x25a2300, 0x25a2304),
];

const ADMISSION_TAIL_RANGES: [(usize, usize); 7] = [
    (0x2020d6c, 0x2020d78),
    (0x2023498, 0x20234ac),
    (0x2024020, 0x2024080),
    (0x1fa5c9c, 0x1fa5ccc),
    (0x2023f54, 0x2023f5c),
    (0x2024354, 0x2024364),
    (0x1e603f8, 0x1e603fc),
];

pub fn qualify(ram: &[u8]) -> Result<(), String> {
    for (ranges, expected, size) in [
        (ROUTE_RANGES.as_slice(), ROUTE_SHA256, 910),
        (CONSUMER_RANGES.as_slice(), CONSUMER_SHA256, 5358),
        (ADMISSION_TAIL_RANGES.as_slice(), ADMISSION_TAIL_SHA256, 204),
    ] {
        let mut bytes = Vec::with_capacity(size);
        for &(start, end) in ranges {
            bytes.extend_from_slice(ram.get(start..end).ok_or("Missing REC input source")?);
        }
        if bytes.len() != size || fujimod_firmware::sha256(&bytes) != expected {
            return Err("REC input source differs".into());
        }
    }
    let producer = ram
        .get(PRODUCER_RANGE.0..PRODUCER_RANGE.1)
        .ok_or("Missing REC producer source")?;
    if producer.len() != 13808 || fujimod_firmware::sha256(producer) != PRODUCER_SOURCE_SHA256 {
        return Err("REC producer source differs".into());
    }
    // Original MOVW/MOVT at primary load: the raw message address is not the UI base.
    if ram.get(0x1e28130..0x1e28138)
        != Some([0x80, 0x3d, 0x00, 0xe3, 0xb7, 0x31, 0x40, 0xe3].as_slice())
    {
        return Err("REC raw message address differs".into());
    }
    Ok(())
}

#[derive(Default)]
pub struct ProducerTrace {
    pub records: Vec<Value>,
    pub dropped: usize,
}

impl ProducerTrace {
    pub fn value(&self) -> Value {
        json!({"sourceSha256":PRODUCER_SOURCE_SHA256,"limit":PRODUCER_LIMIT,
            "records":self.records,"dropped":self.dropped,"meaning":PRODUCER_MEANING})
    }
}

#[derive(Default)]
pub struct Trace {
    pub enabled: bool,
    pub records: Vec<Value>,
    pub dropped: usize,
    pub producer: ProducerTrace,
    pub display: crate::rec_display::Trace,
}

impl Trace {
    pub fn value(&self) -> Value {
        json!({"enabled":self.enabled,"records":self.records,"dropped":self.dropped,
            "limit":LIMIT,"routeSourceSha256":ROUTE_SHA256,"consumerSourceSha256":CONSUMER_SHA256,
            "admissionTailSourceSha256":ADMISSION_TAIL_SHA256,"producer":self.producer.value(),
            "meaning":"Passive original-PC samples; raw registers and current per-core virtual RAM. No deduplication or inferred message receipt; action164 may serve other messages."})
    }
}

pub fn is_observation_point(pc: u64) -> bool {
    POINTS.contains(&pc) || PRODUCER_POINTS.binary_search(&pc).is_ok()
}

pub fn observe(board: &Session, trace: &mut Trace, core: usize, pc: u64) {
    if !trace.enabled {
        return;
    }
    let original_point = POINTS.contains(&pc);
    let producer_point = PRODUCER_POINTS.binary_search(&pc).is_ok();
    let record_producer = if producer_point {
        if trace.producer.records.len() == PRODUCER_LIMIT {
            trace.producer.dropped = trace.producer.dropped.saturating_add(1);
            false
        } else {
            true
        }
    } else {
        false
    };
    if !original_point && !record_producer {
        return;
    }
    let registers = arm::CORE
        .iter()
        .map(|&r| board.read_register(core, r))
        .collect::<Result<Vec<_>, _>>();
    let original_selected = original_point
        && match &registers {
            Ok(r) => {
                !((pc == 0x20f2028 && r[0] != 0x0f)
                    || (pc == 0x1e2813c && !matches!(r[3], 1 | 0x801 | 0x401)))
            }
            Err(_) => true,
        };
    if original_selected {
        if trace.records.len() == LIMIT {
            trace.dropped = trace.dropped.saturating_add(1);
        } else {
            let phase = match pc {
                0x20f2028 => "scanner-sink-entry-before-push",
                0x20f2038 => "mapper-return-before-compare",
                0x1e2813c => "primary-after-raw-message-load",
                0x1e603ec => "action164-entry-before-push",
                0x1e603fc => "action164-admission-return",
                0x1e60420 => "action164-classifier-return",
                0x1e604e0 => "action164-before-first-ui-call",
                0x1e604e4 => "action164-after-first-before-second-ui-call",
                0x202403c => "admission-tail-getter-return",
                0x2024058 => "admission-tail-after-byte-load",
                0x2024074 => "admission-tail-before-mov-r0-r4",
                _ => unreachable!(),
            };
            let mut record = json!({"core":core,"pc":pc,"tick":board.now(),
                "instructions":board.instructions().iter().sum::<u64>(),"phase":phase});
            match &registers {
                Ok(registers) => {
                    record["sp"] = json!(registers[13]);
                    record["fp"] = json!(registers[11]);
                    record["registers"] = json!(registers);
                }
                Err(error) => record["unavailable"] = json!(error),
            }
            if pc == 0x1e603ec {
                record["currentRawMessage"] = match board.read_virtual(core, 0x1b70d80, 4) {
                    Ok(bytes) => json!({"address":0x1b70d80,"bytes":bytes}),
                    Err(error) => json!({"address":0x1b70d80,"unavailable":error}),
                };
            }
            trace.records.push(record);
        }
    }
    if record_producer {
        let mut record = json!({"core":core,"pc":pc,"tick":board.now(),
            "instructions":board.instructions().iter().sum::<u64>(),"phase":PRODUCER_PHASE});
        let mut unavailable = serde_json::Map::new();
        match &registers {
            Ok(registers) => {
                record["sp"] = json!(registers[13]);
                record["fp"] = json!(registers[11]);
                record["registers"] = json!(registers);
            }
            Err(error) => {
                unavailable.insert("registers".into(), json!(error));
            }
        }
        match board.read_register(core, arm::CPSR) {
            Ok(cpsr) => record["preCpsr"] = json!(cpsr),
            Err(error) => {
                unavailable.insert("preCpsr".into(), json!(error));
            }
        }
        if !unavailable.is_empty() {
            record["unavailable"] = Value::Object(unavailable);
        }
        trace.producer.records.push(record);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rec_input_guards_accept_original_fixtures_and_reject_changed_source() {
        assert!(qualify(&[]).is_err());
        assert!(qualify(&vec![0; 0x25a2aa8]).is_err());
        let mut ram = vec![0; 0x25a2aa8];
        for (ranges, bytes) in [
            (
                ROUTE_RANGES.as_slice(),
                include_bytes!("../../../profiles/fixtures/xt4-212-rec-input-route.bin").as_slice(),
            ),
            (
                CONSUMER_RANGES.as_slice(),
                include_bytes!("../../../profiles/fixtures/xt4-212-rec-input-consumer.bin")
                    .as_slice(),
            ),
            (
                ADMISSION_TAIL_RANGES.as_slice(),
                include_bytes!("../../../profiles/fixtures/xt4-212-rec-admission-tail.bin")
                    .as_slice(),
            ),
        ] {
            let mut cursor = 0;
            for &(start, end) in ranges {
                let length = end - start;
                ram[start..end].copy_from_slice(&bytes[cursor..cursor + length]);
                cursor += length;
            }
            assert_eq!(cursor, bytes.len());
        }
        ram[PRODUCER_RANGE.0..PRODUCER_RANGE.1].copy_from_slice(include_bytes!(
            "../../../profiles/fixtures/xt4-212-rec-producer.bin"
        ));
        qualify(&ram).unwrap();
        ram[ROUTE_RANGES[0].0] ^= 1;
        assert!(qualify(&ram).is_err());
        ram[ROUTE_RANGES[0].0] ^= 1;
        ram[CONSUMER_RANGES[6].0] ^= 1;
        assert!(qualify(&ram).is_err());
        ram[CONSUMER_RANGES[6].0] ^= 1;
        ram[ADMISSION_TAIL_RANGES[2].0] ^= 1;
        assert!(qualify(&ram).is_err());
        ram[ADMISSION_TAIL_RANGES[2].0] ^= 1;
        // Original three guards do not include this producer-only byte.
        ram[0x2021000] ^= 1;
        assert!(qualify(&ram).is_err());
    }

    #[test]
    fn rec_producer_points_and_unavailable_reads_are_bounded_and_passive() {
        assert_eq!(PRODUCER_POINTS.len(), 1125);
        assert!(PRODUCER_POINTS.windows(2).all(|pair| pair[0] < pair[1]));
        assert!(
            PRODUCER_POINTS.iter().all(|&pc| pc & 3 == 0
                && (PRODUCER_RANGE.0 as u64..PRODUCER_RANGE.1 as u64).contains(&pc))
        );
        assert_eq!(
            POINTS
                .iter()
                .filter(|pc| PRODUCER_POINTS.binary_search(pc).is_ok())
                .copied()
                .collect::<Vec<_>>(),
            vec![0x202403c]
        );
        let board = Session::new(
            serde_json::from_value(
                json!({"ram_size":65536,"cores":1,"discovery":false,"cbar":4294901760u32}),
            )
            .unwrap(),
            None,
        )
        .unwrap();
        let before = serde_json::to_value(board.state(true)).unwrap();
        let mut trace = Trace::default();
        observe(&board, &mut trace, 99, PRODUCER_POINTS[0]);
        assert!(trace.producer.records.is_empty());
        trace.enabled = true;
        observe(&board, &mut trace, 99, PRODUCER_POINTS[0]);
        assert_eq!(trace.producer.records.len(), 1);
        let record = &trace.producer.records[0];
        assert!(record["unavailable"]["registers"].is_string());
        assert!(record["unavailable"]["preCpsr"].is_string());
        assert!(record.get("registers").is_none() && record.get("preCpsr").is_none());
        assert!(trace.records.is_empty());
        assert_eq!(before, serde_json::to_value(board.state(true)).unwrap());
        assert!(board.device_records().is_empty() && board.frontier(false).is_empty());
    }

    #[test]
    fn rec_input_filters_preserve_raw_returns_and_contain_unavailable_context() {
        let board = Session::new(
            serde_json::from_value(json!({"ram_size":65536,
            "cores":1,"discovery":false,"cbar":4294901760u32}))
            .unwrap(),
            None,
        )
        .unwrap();
        let mut trace = Trace {
            enabled: true,
            ..Trace::default()
        };
        for (pc, r0, r3, count) in [
            (0x20f2028, 14, 0, 0),
            (0x20f2028, 15, 0, 1),
            (0x20f2038, 0x801, 0, 2),
            (0x1e2813c, 0, 2, 2),
            (0x1e2813c, 0, 1, 3),
            (0x1e2813c, 0, 0x801, 4),
            (0x1e2813c, 0, 0x401, 5),
            (0x1e603ec, 0, 0, 6),
            (0x1e603fc, 0, 0, 7),
            (0x1e60420, 0, 0, 8),
            (0x1e604e0, 0, 0, 9),
            (0x1e604e4, 0, 0, 10),
            (0x202403c, 7, 0, 11),
            (0x2024058, 7, 3, 12),
            (0x2024074, 7, 3, 13),
        ] {
            board.write_register(0, arm::CORE[0], r0).unwrap();
            board.write_register(0, arm::CORE[3], r3).unwrap();
            let before = serde_json::to_value(board.state(true)).unwrap();
            observe(&board, &mut trace, 0, pc);
            assert_eq!(trace.records.len(), count);
            assert_eq!(before, serde_json::to_value(board.state(true)).unwrap());
        }
        assert_eq!(trace.records[1]["registers"][0], 0x801);
        assert_eq!(trace.records[5]["currentRawMessage"]["address"], 0x1b70d80);
        assert!(trace.records[5]["currentRawMessage"]["unavailable"].is_string());
        assert!(board.device_records().is_empty());
        assert!(board.frontier(false).is_empty());
    }
}
