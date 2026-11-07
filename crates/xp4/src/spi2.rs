use crate::{rec_display, rec_input};
use fujimod_emulator_cpu::arm;
use fujimod_emulator_xp4_board::Session;
use serde_json::{Value, json};
use std::{cell::RefCell, rc::Rc};

pub const SOURCE_SHA256: &str = "ab99a379c924ceae0fbafe40c7d84b0a4bfbe08b61cde397d1c06f4b02d26d8b";
pub const SECOND_SOURCE_SHA256: &str =
    "38ae83e3359c207fb6b1cfdbf643e9fb0b14fd2501d772b512abe5f86b942a99";
pub const OWNER_SOURCE_SHA256: &str =
    "e88ed2005ce0bbf52adfe23a60d82432d3db8b85565d9f527478611a8465debb";
const RANGES: [(usize, usize); 22] = [
    (0x11fb5cc, 0x11fb5e0),
    (0x119f68c, 0x119f6a4),
    (0x1582008, 0x1582024),
    (0x119e75c, 0x119e76c),
    (0x119e78c, 0x119e7cc),
    (0x1581e3c, 0x1581e3e),
    (0x1582d30, 0x1582d48),
    (0x11ae554, 0x11ae56c),
    (0x119e878, 0x119e954),
    (0x119f6f4, 0x119f754),
    (0x119f7b0, 0x119f840),
    (0x119fbd4, 0x119fbe8),
    (0x119fc20, 0x119fc34),
    (0x119e468, 0x119e4c8),
    (0x119e344, 0x119e35c),
    (0x11fa554, 0x11fa98c),
    (0x11fbaf8, 0x11fbbf4),
    (0x11fae74, 0x11faf64),
    (0x11fb000, 0x11fb084),
    (0x11fb6a4, 0x11fb790),
    (0x11fb4f0, 0x11fb59c),
    (0x11fbbf4, 0x11fbc8c),
];
const SECOND_RANGES: [(usize, usize); 9] = [
    (0x11fa98c, 0x11fadd4),
    (0x11fb790, 0x11fb8b8),
    (0x11fbe44, 0x11fbf5c),
    (0x11fa40c, 0x11fa4b4),
    (0x15ee3c0, 0x15ee3c8),
    (0x15ee408, 0x15ee410),
    (0x15ee430, 0x15ee438),
    (0x15ee440, 0x15ee448),
    (0x15ee2d0, 0x15ee2d8),
];
const OWNER_RANGES: [(usize, usize); 15] = [
    (0x11f8f6c, 0x11f8ff8),
    (0x11f910c, 0x11f91fc),
    (0x11f91fc, 0x11f9258),
    (0x11f5bc4, 0x11f5d04),
    (0x11f9280, 0x11f9394),
    (0x11f9e9c, 0x11f9fec),
    (0x11f95b0, 0x11f96d0),
    (0x11f6bfc, 0x11f6d38),
    (0x127e784, 0x127e914),
    (0x1596bec, 0x1596bf4),
    (0x1596c0c, 0x1596c14),
    (0x1596c14, 0x1596c1c),
    (0x1596c44, 0x1596c4c),
    (0x1596c64, 0x1596c6c),
    (0x1596c6c, 0x1596c74),
];
const POINTS: [u64; 8] = [
    0x11fb000, 0x11fa554, 0x11faf58, 0x11fb6a4, 0x11fb4f0, 0x11fbc80, 0x11fbb60, 0x11fbb80,
];
const OWNER_POINTS: [u64; 5] = [0x11f8f6c, 0x11f8fd4, 0x11f9280, 0x11f9ebc, 0x11f95b0];

pub fn qualify(ram: &[u8], second_header: bool, owner_ack: bool) -> Result<(), String> {
    if owner_ack && !second_header {
        return Err("SPI2 owner experiment requires both earlier headers".into());
    }
    let mut original = Vec::new();
    for (start, end) in RANGES
        .into_iter()
        .chain(SECOND_RANGES.into_iter().filter(|_| second_header))
        .chain(OWNER_RANGES.into_iter().filter(|_| owner_ack))
    {
        original.extend_from_slice(ram.get(start..end).ok_or("Missing SPI2 header source")?);
    }
    if fujimod_firmware::sha256(&original)
        != if owner_ack {
            OWNER_SOURCE_SHA256
        } else if second_header {
            SECOND_SOURCE_SHA256
        } else {
            SOURCE_SHA256
        }
    {
        return Err("SPI2 header/driver source differs".into());
    }
    Ok(())
}

#[derive(Default)]
pub struct Trace {
    pub records: Vec<Value>,
    pub dropped: usize,
}

fn owner_context(board: &Session, core: usize, pc: u64) -> Result<Value, String> {
    let r0 = board.read_register(core, arm::CORE[0])?;
    let fp = board.read_register(core, arm::CORE[11])?;
    let sp = board.read_register(core, arm::CORE[13])?;
    let lr = board.read_register(core, arm::CORE[14])?;
    let mut context = json!({"r1":board.read_register(core,arm::CORE[1])?,
        "r2":board.read_register(core,arm::CORE[2])?,
        "r3":board.read_register(core,arm::CORE[3])?,"fp":fp,"sp":sp});
    let frame = |distance: u32| -> Result<(u32, u32), String> {
        let fp = u32::try_from(fp).map_err(|_| "Owner FP exceeds ARM32")?;
        let sp = u32::try_from(sp).map_err(|_| "Owner SP exceeds ARM32")?;
        if (fp | sp) & 3 != 0 || fp.checked_sub(sp) != Some(distance) {
            return Err("Owner stack frame is unqualified".into());
        }
        fp.checked_add(4).ok_or("Owner saved LR extent overflows")?;
        Ok((fp, sp))
    };
    let reads = (|| -> Result<Value, String> {
        Ok(match pc {
            0x11f8f6c => json!({"priorHeaderAndCallbackBytes":
                board.read_virtual(core,0x1ab2094,8)?}),
            0x11f8fd4 => {
                let (fp, _) = frame(0x24)?;
                json!({"savedCallerLrBytes":board.read_virtual(core,fp,4)?})
            }
            0x11f9280 => {
                let history = board.read_virtual(core, 0x1be46ac, 4)?;
                let header = board.read_virtual(core, 0x1be67c8, 12)?;
                let qualified = u16::from_le_bytes(header[0..2].try_into().unwrap()) == 0x199a
                    && u32::from_le_bytes(header[4..8].try_into().unwrap()) == 8
                    && u32::from_le_bytes(header[8..12].try_into().unwrap()) == 0x1be67d4;
                let mut value = json!({"stateHistory":history,"messageHeaderBytes":header,
                    "payloadQualified":qualified});
                if qualified {
                    value["payloadPrefixBytes"] = json!(board.read_virtual(core, 0x1be67d4, 5)?);
                }
                value
            }
            0x11f9ebc => {
                let (fp, sp) = frame(0x54)?;
                let mut value = json!({"savedCallerLrBytes":board.read_virtual(core,fp,4)?,
                    "popSucceeded":r0==0});
                if r0 == 0 {
                    let record = fp.checked_sub(0x30).ok_or("Owner record underflows")?;
                    if record < sp || record.checked_add(20).is_none_or(|end| end > fp) {
                        return Err("Owner record exceeds stack frame".into());
                    }
                    value["poppedRecordBytes"] = json!(board.read_virtual(core, record, 20)?);
                }
                value
            }
            0x11f95b0 => {
                let (_, sp) = frame(0x54)?;
                if !matches!(lr, 0x11f9ee4 | 0x11f9f40) {
                    return Err("Owner formatter caller is unqualified".into());
                }
                json!({"callerPath":if lr==0x11f9ee4 {"first-pop"}else{"drain"},
                    "stackArgumentBytes":board.read_virtual(core,sp,12)?})
            }
            _ => return Err("Unknown owner observation point".into()),
        })
    })();
    context["reads"] = reads.unwrap_or_else(|error| json!({"unavailable":error}));
    Ok(context)
}

fn points(second_header: bool, owner_ack: bool, rec_input: bool, display: bool) -> Vec<u64> {
    let mut points = POINTS.to_vec();
    if second_header {
        points.extend([0x11fa98c, 0x11fb790, 0x11fbe44, 0x11fbb5c]);
    }
    if owner_ack {
        points.extend(OWNER_POINTS);
    }
    if rec_input {
        points.extend(rec_input::POINTS);
        points.extend(rec_input::PRODUCER_POINTS);
    }
    if display {
        points.extend(rec_display::POINTS);
    }
    if rec_input || display {
        points.sort_unstable();
        points.dedup();
    }
    points
}

pub fn enable_rec_input_trace(
    board: &Session,
    trace: &Rc<RefCell<rec_input::Trace>>,
    second_header: bool,
    owner_ack: bool,
) -> Result<Value, String> {
    if trace.borrow().enabled {
        return Err("REC input trace already enabled".into());
    }
    for core in 0..board.instructions().len() {
        board.instruction_points(
            core,
            points(
                second_header,
                owner_ack,
                true,
                trace.borrow().display.enabled,
            ),
        )?;
    }
    let mut trace = trace.borrow_mut();
    trace.enabled = true;
    Ok(trace.value())
}

pub fn enable_rec_display_trace(
    board: &Session,
    trace: &Rc<RefCell<rec_input::Trace>>,
    second_header: bool,
    owner_ack: bool,
) -> Result<Value, String> {
    if trace.borrow().display.enabled {
        return Err("REC display trace already enabled".into());
    }
    for core in 0..board.instructions().len() {
        board.instruction_points(
            core,
            points(second_header, owner_ack, trace.borrow().enabled, true),
        )?;
    }
    let mut trace = trace.borrow_mut();
    trace.display.enabled = true;
    Ok(trace.display.value())
}

pub fn install_trace(
    board: &Rc<Session>,
    second_header: bool,
    owner_ack: bool,
    rec_input: Option<Rc<RefCell<rec_input::Trace>>>,
) -> Result<Rc<RefCell<Trace>>, String> {
    let trace = Rc::new(RefCell::new(Trace::default()));
    let captured = trace.clone();
    let weak = Rc::downgrade(board);
    for core in 0..board.instructions().len() {
        board.instruction_points(core, points(second_header, owner_ack, false, false))?;
    }
    board.set_instruction_observer(Some(Rc::new(move |core, pc| {
        let board = weak.upgrade().ok_or("Observed SPI2 board closed")?;
        let rec_point = rec_input::is_observation_point(pc);
        let display_point = rec_display::POINTS.contains(&pc);
        if rec_point || display_point {
            if let Some(trace) = &rec_input {
                let mut trace = trace.borrow_mut();
                if rec_point {
                    rec_input::observe(&board, &mut trace, core, pc);
                }
                if display_point {
                    rec_display::observe(&board, &mut trace.display, core, pc);
                }
            }
            return Ok(());
        }
        if pc == 0x11faf58 {
            let pair = (
                board.read_register(core, arm::CORE[4])?,
                board.read_register(core, arm::CORE[8])?,
            );
            if pair != (2, 4) && !(second_header && matches!(pair, (4, 4) | (5, 5))) {
                return Ok(());
            }
        }
        let context = (|| -> Result<Value, String> {
            let history = board.read_virtual(core, 0x1ab65a4, 4)?;
            if pc == 0x11fb4f0 && history[0] != 4 && !(second_header && history[0] == 6) {
                return Ok(Value::Null);
            }
            let mut context = json!({"stateHistory":history,
                "headerHistoryBytes":board.read_virtual(core,0x1ab208c,8)?,
                "outgoingHeaderBytes":board.read_virtual(core,0x1ab2080,4)?,
                "flag98":board.read_virtual(core,0x1ab20d8,1)?[0],
                "txBytes":board.read_virtual(core,0x177b480,4)?,
                "rxBytes":board.read_virtual(core,0x177bcc0,4)?});
            if second_header {
                context["priorHeaderAndCallbackBytes"] =
                    json!(board.read_virtual(core, 0x1ab2094, 8)?);
                context["sequenceAndCountersBytes"] =
                    json!(board.read_virtual(core, 0x1ab20cc, 12)?);
                context["limit1cBytes"] = json!(board.read_virtual(core, 0x1ab205c, 4)?);
                context["flag49"] = json!(board.read_virtual(core, 0x1ab2089, 1)?[0]);
            }
            Ok(context)
        })();
        if matches!(context, Ok(Value::Null)) {
            return Ok(());
        }
        let mut trace = captured.borrow_mut();
        if trace.records.len() == 64 {
            trace.dropped = trace.dropped.saturating_add(1);
            return Ok(());
        }
        let mut record = json!({"core":core,"pc":pc,"tick":board.now(),
            "instructions":board.instructions().iter().sum::<u64>(),
            "r0":board.read_register(core,arm::CORE[0])?,
            "lr":board.read_register(core,arm::CORE[14])?,
            "context":context.unwrap_or_else(|error|json!({"unavailable":error}))});
        if second_header {
            record["r4"] = json!(board.read_register(core, arm::CORE[4])?);
            record["r8"] = json!(board.read_register(core, arm::CORE[8])?);
        }
        if owner_ack && OWNER_POINTS.contains(&pc) {
            record["owner"] = owner_context(&board, core, pc)
                .unwrap_or_else(|error| json!({"unavailable":error}));
        }
        trace.records.push(record);
        Ok(())
    })));
    Ok(trace)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_and_rec_arm_orders_preserve_old_spi_rec_hooks_and_four_core_state() {
        assert_eq!(points(true, true, false, false).len(), 17);
        assert_eq!(points(true, true, true, false).len(), 1152);
        assert_eq!(points(true, true, false, true).len(), 215);
        assert_eq!(points(true, true, true, true).len(), 1350);
        assert!(
            rec_display::POINTS
                .iter()
                .all(|pc| !rec_input::is_observation_point(*pc)
                    && !points(true, true, false, false).contains(pc))
        );
        for display_first in [false, true] {
            let make = || {
                let board=Rc::new(Session::new(serde_json::from_value(json!({"ram_size":67108864,"cores":4,"discovery":false,"cbar":4294901760u32})).unwrap(),None).unwrap());
                let program: Vec<_> = [0xe1a00000u32, 0xeafffffd]
                    .into_iter()
                    .flat_map(u32::to_le_bytes)
                    .collect();
                for (core, pc) in [0x122e66c, 0x11fb000, 0x20f2038, 0x202403c]
                    .into_iter()
                    .enumerate()
                {
                    board.write(pc, &program).unwrap();
                    board.write_register(core, arm::PC, pc as u64).unwrap();
                }
                board
            };
            let observed = make();
            let control = make();
            let rec = Rc::new(RefCell::new(rec_input::Trace::default()));
            let control_rec = Rc::new(RefCell::new(rec_input::Trace::default()));
            let spi = install_trace(&observed, true, true, Some(rec.clone())).unwrap();
            let control_spi =
                install_trace(&control, true, true, Some(control_rec.clone())).unwrap();
            let original = rec.borrow().value();
            observed.run(0, 2).unwrap();
            control.run(0, 2).unwrap();
            assert!(!rec.borrow().display.enabled && rec.borrow().display.records.is_empty());
            let before = serde_json::to_value(observed.state(true)).unwrap();
            if display_first {
                enable_rec_display_trace(&observed, &rec, true, true).unwrap();
                assert_eq!(rec.borrow().value(), original);
                enable_rec_input_trace(&observed, &rec, true, true).unwrap();
            } else {
                enable_rec_input_trace(&observed, &rec, true, true).unwrap();
                let old = rec.borrow().value();
                enable_rec_display_trace(&observed, &rec, true, true).unwrap();
                assert_eq!(rec.borrow().value(), old);
            }
            enable_rec_input_trace(&control, &control_rec, true, true).unwrap();
            assert_eq!(before, serde_json::to_value(observed.state(true)).unwrap());
            assert!(enable_rec_display_trace(&observed, &rec, true, true).is_err());
            assert!(enable_rec_input_trace(&observed, &rec, true, true).is_err());
            observed.run_board(80, 8).unwrap();
            control.run_board(80, 8).unwrap();
            assert_eq!(
                serde_json::to_value(observed.state(true)).unwrap(),
                serde_json::to_value(control.state(true)).unwrap()
            );
            assert_eq!(rec.borrow().value(), control_rec.borrow().value());
            assert!(!rec.borrow().records.is_empty() && !rec.borrow().producer.records.is_empty());
            let prior_display_len = {
                let borrowed = rec.borrow();
                let display = &borrowed.display;
                assert!(!display.records.is_empty());
                assert_eq!(display.records[0]["core"], 0);
                assert_eq!(display.records[0]["pc"], 0x122e66c);
                assert_eq!(
                    display.records[0]["registers"].as_array().unwrap().len(),
                    16
                );
                assert!(display.records[0]["preCpsr"].is_number());
                display.records.len()
            };
            assert_eq!(spi.borrow().records, control_spi.borrow().records);
            assert_eq!(spi.borrow().dropped, control_spi.borrow().dropped);
            assert!(!spi.borrow().records.is_empty());
            // Run a synthetic NOP at the real write-helper PC; it must filter by original R0.
            for selected in [false, true] {
                for board in [&observed, &control] {
                    board.write(0x5a9f8, &0xe1a00000u32.to_le_bytes()).unwrap();
                    board.write_register(0, arm::PC, 0x5a9f8).unwrap();
                    board
                        .write_register(
                            0,
                            arm::CORE[0],
                            if selected { 0xfffa9008 } else { 0xfffa900c },
                        )
                        .unwrap();
                }
                observed.run(0, 1).unwrap();
                control.run(0, 1).unwrap();
                assert_eq!(
                    rec.borrow().display.records.len(),
                    prior_display_len + usize::from(selected)
                );
            }
            let old_rec = rec.borrow().value();
            let before_composition = rec.borrow().display.records.len();
            for (index, &pc) in rec_display::POINTS[6..].iter().enumerate() {
                let core = index % 4;
                let r0 = if pc == 0x121de40 {
                    0x140e
                } else if pc == 0x2251560 {
                    2
                } else {
                    index as u64
                };
                for board in [&observed, &control] {
                    board
                        .write(pc as usize, &0xe1a00000u32.to_le_bytes())
                        .unwrap();
                    board.write_register(core, arm::PC, pc).unwrap();
                    board.write_register(core, arm::CORE[0], r0).unwrap();
                    match pc {
                        0x121e13c => board.write_register(core, arm::CORE[3], 0x140e).unwrap(),
                        0x2120edc | 0x2120ef8 | 0x2120efc => {
                            board.write_register(core, arm::CORE[4], 87).unwrap()
                        }
                        0x2120f3c => {
                            board.write_register(core, arm::CORE[4], 0xae).unwrap();
                            board.write_register(core, arm::CORE[2], 0x31ecc4c).unwrap();
                        }
                        0x216d768 => board
                            .write_register(core, arm::CORE[14], 0x2182100)
                            .unwrap(),
                        0x2170c78 => board
                            .write_register(core, arm::CORE[14], 0x2153a54)
                            .unwrap(),
                        0x21f1f84 | 0x21f1f98 | 0x21f1f9c => {
                            board.write_register(core, arm::CORE[7], 0).unwrap();
                        }
                        0x2256460 | 0x22521f0 => {
                            board.write_register(core, arm::CORE[5], 0x1260).unwrap();
                        }
                        0x2252274 => {
                            board.write_register(core, arm::CORE[5], 0x1287).unwrap();
                        }
                        0x2249258 => {
                            board.write_register(core, arm::CORE[2], 0x1293).unwrap();
                        }
                        0x2245fcc => {
                            board
                                .write_register(core, arm::CORE[14], 0x2252380)
                                .unwrap();
                        }
                        0x2246008 => {
                            board.write_register(core, arm::CORE[3], 0x21d67ec).unwrap();
                        }
                        0x212cfc4 => {
                            board
                                .write_register(core, arm::CORE[14], 0x2182008)
                                .unwrap();
                            board
                                .write_register(core, arm::CORE[13], 0x1f00000)
                                .unwrap();
                            board
                                .write_register(core, arm::CORE[11], 0x1f00004)
                                .unwrap();
                            board.write_register(core, arm::CORE[3], 3).unwrap();
                        }
                        _ => {}
                    }
                }
                observed.run(core, 1).unwrap();
                control.run(core, 1).unwrap();
                let borrowed = rec.borrow();
                let row = borrowed.display.records.last().unwrap();
                assert_eq!(row["core"], core);
                assert_eq!(row["pc"], pc);
                assert_eq!(row["registers"][0], r0);
                assert!(
                    [
                        "composition-",
                        "list-",
                        "video-",
                        "config-",
                        "app-",
                        "work-",
                        "init-",
                        "async-",
                        "raw1266-",
                        "queue25-"
                    ]
                    .iter()
                    .any(|prefix| row["phase"].as_str().unwrap().starts_with(prefix))
                );
                assert_eq!(
                    row.as_object().unwrap().len(),
                    if pc == 0x212cfc4 { 10 } else { 9 }
                );
                if pc == 0x212cfc4 {
                    assert_eq!(row["appMessageHostContext"]["bytes"], json!([0, 0]));
                }
                if pc == 0x2249258 {
                    assert_eq!(row["registers"][15], pc);
                }
            }
            assert_eq!(rec.borrow().display.records.len(), before_composition + 192);
            assert_eq!(rec.borrow().value(), old_rec);
            assert_eq!(rec.borrow().value(), control_rec.borrow().value());
            for core in 0..4 {
                for r in arm::CORE.iter().copied().chain([arm::CPSR]) {
                    assert_eq!(
                        observed.read_register(core, r).unwrap(),
                        control.read_register(core, r).unwrap()
                    );
                }
            }
            assert_eq!(
                serde_json::to_value(observed.state(true)).unwrap(),
                serde_json::to_value(control.state(true)).unwrap()
            );
            assert!(observed.device_records().is_empty() && observed.frontier(false).is_empty());
        }
    }

    #[test]
    fn rec_input_arm_keeps_old_hooks_and_has_its_own_capacity() {
        let make = || {
            let board = Rc::new(
                Session::new(
                    serde_json::from_value(json!({
                        "ram_size":67108864,"cores":4,"discovery":false,"cbar":4294901760u32
                    }))
                    .unwrap(),
                    None,
                )
                .unwrap(),
            );
            let program: Vec<_> = [0xe1a00000u32, 0xeafffffd]
                .into_iter()
                .flat_map(u32::to_le_bytes)
                .collect();
            board.write(0x20f2038, &program).unwrap();
            board.write(0x11fb000, &program).unwrap();
            // Synthetic ARM MOV R0,R4: hook must retain the pre-instruction R0.
            let mov_program: Vec<_> = [0xe1a00004u32, 0xeafffffd]
                .into_iter()
                .flat_map(u32::to_le_bytes)
                .collect();
            board.write(0x2024074, &mov_program).unwrap();
            board.write_register(2, arm::PC, 0x2024074).unwrap();
            board.write_register(2, arm::CORE[0], 7).unwrap();
            board.write_register(2, arm::CORE[4], 17).unwrap();
            board.write_register(0, arm::PC, 0x20f2038).unwrap();
            board.write_register(0, arm::CORE[0], 0x801).unwrap();
            board.write_register(1, arm::PC, 0x11fb000).unwrap();
            board
        };
        let observed = make();
        let control = make();
        let rec = Rc::new(RefCell::new(rec_input::Trace::default()));
        let old = install_trace(&observed, true, true, Some(rec.clone())).unwrap();
        assert_eq!(points(true, true, false, false).len(), 17);
        assert_eq!(points(true, true, true, false).len(), 1152);
        observed.run(0, 2).unwrap();
        control.run(0, 2).unwrap();
        assert!(rec.borrow().records.is_empty());
        assert!(!rec.borrow().enabled);
        let before = serde_json::to_value(observed.state(true)).unwrap();
        enable_rec_input_trace(&observed, &rec, true, true).unwrap();
        assert_eq!(before, serde_json::to_value(observed.state(true)).unwrap());
        assert!(enable_rec_input_trace(&observed, &rec, true, true).is_err());
        observed.run(2, 1).unwrap();
        control.run(2, 1).unwrap();
        {
            let trace = rec.borrow();
            assert_eq!(trace.records.len(), 1);
            assert_eq!(trace.records[0]["pc"], 0x2024074);
            assert_eq!(trace.records[0]["core"], 2);
            assert_eq!(trace.records[0]["registers"][0], 7);
            assert_eq!(trace.records[0]["registers"][4], 17);
        }
        assert_eq!(observed.read_register(2, arm::CORE[0]).unwrap(), 17);
        assert_eq!(
            serde_json::to_value(observed.state(true)).unwrap(),
            serde_json::to_value(control.state(true)).unwrap()
        );
        observed.run(0, 2200).unwrap();
        control.run(0, 2200).unwrap();
        observed.run(1, 200).unwrap();
        control.run(1, 200).unwrap();
        assert_eq!(
            serde_json::to_value(observed.state(true)).unwrap(),
            serde_json::to_value(control.state(true)).unwrap()
        );
        let rec = rec.borrow();
        let old = old.borrow();
        assert_eq!((rec.records.len(), rec.dropped), (1024, 77));
        assert_eq!((old.records.len(), old.dropped), (64, 36));
        assert_eq!(rec.records[0]["registers"].as_array().unwrap().len(), 16);
        assert_eq!(rec.records[1]["pc"], 0x20f2038);
        assert_eq!(rec.records[1]["registers"][0], 0x801);
        assert_eq!(old.records[0]["pc"], 0x11fb000);
        assert!(observed.device_records().is_empty());
        assert!(observed.frontier(false).is_empty());
    }

    #[test]
    fn rec_producer_overlap_and_independent_capacities_preserve_control_state() {
        for producer_first in [false, true] {
            let make = || {
                let board=Rc::new(Session::new(serde_json::from_value(json!({"ram_size":67108864,"cores":1,"discovery":false,"cbar":4294901760u32})).unwrap(),None).unwrap());
                let program: Vec<_> = [0xe1a00000u32, 0xeafffffd]
                    .into_iter()
                    .flat_map(u32::to_le_bytes)
                    .collect();
                for pc in [0x202403c, 0x2024074, 0x2020d6c] {
                    board.write(pc, &program).unwrap();
                }
                board
            };
            let observed = make();
            let control = make();
            let trace = Rc::new(RefCell::new(rec_input::Trace::default()));
            let spi = install_trace(&observed, true, true, Some(trace.clone())).unwrap();
            enable_rec_input_trace(&observed, &trace, true, true).unwrap();
            let stages = if producer_first {
                vec![
                    (0x2020d6c, 8200, 0, 0, 4096, 4),
                    (0x202403c, 2, 1, 0, 4096, 5),
                    (0x2024074, 2200, 1024, 77, 4096, 5),
                ]
            } else {
                vec![
                    (0x202403c, 2, 1, 0, 1, 0),
                    (0x2024074, 2200, 1024, 77, 1, 0),
                    (0x202403c, 2, 1024, 78, 2, 0),
                    (0x2020d6c, 8200, 1024, 78, 4096, 6),
                ]
            };
            for (pc, budget, old_len, old_drop, new_len, new_drop) in stages {
                for board in [&observed, &control] {
                    board.write_register(0, arm::PC, pc).unwrap();
                }
                observed.run(0, budget).unwrap();
                control.run(0, budget).unwrap();
                assert_eq!(
                    serde_json::to_value(observed.state(true)).unwrap(),
                    serde_json::to_value(control.state(true)).unwrap()
                );
                let trace = trace.borrow();
                assert_eq!((trace.records.len(), trace.dropped), (old_len, old_drop));
                assert_eq!(
                    (trace.producer.records.len(), trace.producer.dropped),
                    (new_len, new_drop)
                );
                if !producer_first && budget == 2 && old_len == 1 {
                    let old = &trace.records[0];
                    let new = &trace.producer.records[0];
                    for field in [
                        "core",
                        "pc",
                        "tick",
                        "instructions",
                        "registers",
                        "sp",
                        "fp",
                    ] {
                        assert_eq!(old[field], new[field]);
                    }
                    assert_eq!(
                        new["preCpsr"],
                        observed.read_register(0, arm::CPSR).unwrap()
                    );
                }
            }
            assert!(spi.borrow().records.is_empty());
            assert!(observed.device_records().is_empty() && observed.frontier(false).is_empty());
        }
    }

    #[test]
    fn rec_producer_conditional_mov_keeps_pre_cpsr_and_observes_native_hook_behavior() {
        for zero in [false, true] {
            let make = || {
                let board=Rc::new(Session::new(serde_json::from_value(json!({"ram_size":67108864,"cores":1,"discovery":false,"cbar":4294901760u32})).unwrap(),None).unwrap());
                // Synthetic MOVNE R0,R4; original firmware bytes are not executed.
                let program: Vec<_> = [0x11a00004u32, 0xeafffffe]
                    .into_iter()
                    .flat_map(u32::to_le_bytes)
                    .collect();
                board.write(0x2020d6c, &program).unwrap();
                board.write_register(0, arm::PC, 0x2020d6c).unwrap();
                board.write_register(0, arm::CORE[0], 7).unwrap();
                board.write_register(0, arm::CORE[4], 17).unwrap();
                let cpsr = (board.read_register(0, arm::CPSR).unwrap() & !((1 << 30) | 0x20))
                    | if zero { 1 << 30 } else { 0 };
                board.write_register(0, arm::CPSR, cpsr).unwrap();
                board
            };
            let observed = make();
            let control = make();
            let before_cpsr = observed.read_register(0, arm::CPSR).unwrap();
            let trace = Rc::new(RefCell::new(rec_input::Trace::default()));
            install_trace(&observed, true, true, Some(trace.clone())).unwrap();
            enable_rec_input_trace(&observed, &trace, true, true).unwrap();
            observed.run(0, 1).unwrap();
            control.run(0, 1).unwrap();
            let after_r0 = observed.read_register(0, arm::CORE[0]).unwrap();
            assert_eq!(after_r0, if zero { 7 } else { 17 });
            assert_eq!(
                serde_json::to_value(observed.state(true)).unwrap(),
                serde_json::to_value(control.state(true)).unwrap()
            );
            let trace = trace.borrow();
            let records = &trace.producer.records;
            // Current native backend reports both condition outcomes; Wasm is separately qualified.
            assert_eq!(records.len(), 1);
            if let Some(record) = records.first() {
                assert_eq!(record["registers"][0], 7);
                assert_eq!(record["registers"][4], 17);
                assert_eq!(record["preCpsr"], before_cpsr);
                assert_eq!(record["phase"], rec_input::PRODUCER_PHASE);
            }
            println!(
                "{}",
                json!({"nativeSyntheticMovne":true,"zeroFlag":zero,"conditionPassed":!zero,"producerPreHookRecords":records.len(),"seedPreCpsr":before_cpsr,"postR0":after_r0,"controlStateExact":true})
            );
        }
    }

    #[test]
    fn passive_trace_is_bounded_and_preserves_the_executed_board() {
        let make = || {
            let definition = serde_json::from_value(json!({"ram_size":33554432,
                "cores":1,"discovery":false,"cbar":4294901760u32}))
            .unwrap();
            let board = Rc::new(Session::new(definition, None).unwrap());
            let program: Vec<_> = [0xe3a00004u32, 0xeafffffd]
                .into_iter()
                .flat_map(u32::to_le_bytes)
                .collect();
            board.write(0x11fb000, &program).unwrap();
            board.write(0x1ab65a4, &[3, 2, 1, 0]).unwrap();
            board.write(0x177bcc0, &[1, 2, 3, 4]).unwrap();
            board.write_register(0, arm::PC, 0x11fb000).unwrap();
            board
        };
        let observed = make();
        let control = make();
        let trace = install_trace(&observed, false, false, None).unwrap();
        observed.run(0, 200).unwrap();
        control.run(0, 200).unwrap();
        assert_eq!(
            serde_json::to_value(observed.state(true)).unwrap(),
            serde_json::to_value(control.state(true)).unwrap()
        );
        let trace = trace.borrow();
        assert_eq!((trace.records.len(), trace.dropped), (64, 36));
        assert_eq!(trace.records[0]["context"]["rxBytes"], json!([1, 2, 3, 4]));
        assert_eq!(
            trace.records[0]["context"]["stateHistory"],
            json!([3, 2, 1, 0])
        );
        assert!(observed.device_records().is_empty());
        assert!(observed.frontier(false).is_empty());
        assert!(qualify(&[], false, false).is_err());
        assert!(qualify(&[], true, false).is_err());
    }

    #[test]
    fn second_trace_filters_exact_return_pairs_and_preserves_four_core_execution() {
        for (second, r4, r8, selected) in [
            (false, 2, 4, true),
            (false, 4, 4, false),
            (true, 4, 4, true),
            (true, 5, 5, true),
            (true, 4, 5, false),
        ] {
            let make = || {
                let definition = serde_json::from_value(json!({"ram_size":33554432,
                    "cores":4,"discovery":false,"cbar":4294901760u32}))
                .unwrap();
                let board = Rc::new(Session::new(definition, None).unwrap());
                let program: Vec<_> = [0xe3a00004u32, 0xeafffffd]
                    .into_iter()
                    .flat_map(u32::to_le_bytes)
                    .collect();
                board.write(0x11faf58, &program).unwrap();
                board.write(0x1ab65a4, &[5, 4, 3, 2]).unwrap();
                board.write(0x1ab2094, &[1, 2, 3, 4, 5, 6, 7, 8]).unwrap();
                board
                    .write(0x1ab20cc, &[9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20])
                    .unwrap();
                board.write(0x1ab205c, &[21, 22, 23, 24]).unwrap();
                board.write(0x1ab2089, &[25]).unwrap();
                for core in 0..4 {
                    board.write_register(core, arm::PC, 0x11faf58).unwrap();
                    board.write_register(core, arm::CORE[4], r4).unwrap();
                    board.write_register(core, arm::CORE[8], r8).unwrap();
                }
                board
            };
            let observed = make();
            let control = make();
            let trace = install_trace(&observed, second, false, None).unwrap();
            observed.run_board(200, 8).unwrap();
            control.run_board(200, 8).unwrap();
            assert_eq!(
                serde_json::to_value(observed.state(true)).unwrap(),
                serde_json::to_value(control.state(true)).unwrap()
            );
            let trace = trace.borrow();
            assert_eq!(
                (trace.records.len(), trace.dropped),
                if selected { (64, 36) } else { (0, 0) }
            );
            if second && selected {
                let row = &trace.records[0];
                assert_eq!(row["r4"], r4);
                assert_eq!(row["r8"], r8);
                assert_eq!(
                    row["context"]["priorHeaderAndCallbackBytes"],
                    json!([1, 2, 3, 4, 5, 6, 7, 8])
                );
                assert_eq!(
                    row["context"]["sequenceAndCountersBytes"],
                    json!([9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20])
                );
                assert_eq!(row["context"]["limit1cBytes"], json!([21, 22, 23, 24]));
                assert_eq!(row["context"]["flag49"], 25);
            }
            assert!(observed.device_records().is_empty());
            assert!(observed.frontier(false).is_empty());
        }
    }

    #[test]
    fn owner_trace_qualifies_frames_records_and_fixed_payload_without_advancing_devices() {
        for (pc, r0, fp, sp, lr, body, expected) in [
            (
                0x11f8f6c,
                0x20c8,
                0,
                0x1f00000,
                0x11fad60,
                0x1be67d4u32,
                "priorHeaderAndCallbackBytes",
            ),
            (
                0x11f8fd4,
                0,
                0x1f00024,
                0x1f00000,
                0,
                0x1be67d4,
                "savedCallerLrBytes",
            ),
            (
                0x11f8fd4,
                0,
                0x1f00028,
                0x1f00000,
                0,
                0x1be67d4,
                "unavailable",
            ),
            (
                0x11f8fd4,
                0,
                0xfff34024,
                0xfff34000,
                0,
                0x1be67d4,
                "unavailable",
            ),
            (
                0x11f9280,
                5,
                0,
                0x1f00000,
                0x11f91f0,
                0x1be67d4,
                "payloadPrefixBytes",
            ),
            (
                0x11f9280,
                5,
                0,
                0x1f00000,
                0x11f91f0,
                0xfff34008,
                "messageHeaderBytes",
            ),
            (
                0x11f9ebc,
                0,
                0x1f00054,
                0x1f00000,
                0,
                0x1be67d4,
                "poppedRecordBytes",
            ),
            (
                0x11f9ebc,
                1,
                0x1f00054,
                0x1f00000,
                0,
                0x1be67d4,
                "popSucceeded",
            ),
            (
                0x11f9ebc,
                0,
                0x1f00050,
                0x1f00000,
                0,
                0x1be67d4,
                "unavailable",
            ),
            (
                0x11f95b0,
                0x1978,
                0x1f00054,
                0x1f00000,
                0x11f9ee4,
                0x1be67d4,
                "stackArgumentBytes",
            ),
            (
                0x11f95b0,
                0x1978,
                0x1f00054,
                0x1f00000,
                0x11f9f40,
                0x1be67d4,
                "stackArgumentBytes",
            ),
            (
                0x11f95b0,
                0x1978,
                0x1f00054,
                0x1f00000,
                0,
                0x1be67d4,
                "unavailable",
            ),
        ] {
            let make = || {
                let definition = serde_json::from_value(json!({"ram_size":33554432,
                    "cores":4,"discovery":false,"cbar":4294901760u32}))
                .unwrap();
                let board = Rc::new(Session::new(definition, None).unwrap());
                let program: Vec<_> = [0xe1a00000u32, 0xeafffffd]
                    .into_iter()
                    .flat_map(u32::to_le_bytes)
                    .collect();
                board.write(pc as usize, &program).unwrap();
                let mut header = [0u8; 12];
                header[..2].copy_from_slice(&0x199au16.to_le_bytes());
                header[4..8].copy_from_slice(&8u32.to_le_bytes());
                header[8..12].copy_from_slice(&body.to_le_bytes());
                board.write(0x1be67c8, &header).unwrap();
                board.write(0x1be67d4, &[0xc8, 0x20, 0, 0, 0]).unwrap();
                board.write(0x1f00024, &[0x78; 20]).unwrap();
                board.write(0x1f00054, &0x11f91f0u32.to_le_bytes()).unwrap();
                for core in 0..4 {
                    for (register, value) in [
                        (15, pc),
                        (0, r0),
                        (1, 1),
                        (2, 2),
                        (3, 3),
                        (11, fp),
                        (13, sp),
                        (14, lr),
                    ] {
                        board
                            .write_register(core, arm::CORE[register], value)
                            .unwrap();
                    }
                }
                board
            };
            let observed = make();
            let control = make();
            let trace = install_trace(&observed, true, true, None).unwrap();
            observed.run_board(64, 8).unwrap();
            control.run_board(64, 8).unwrap();
            assert_eq!(
                serde_json::to_value(observed.state(true)).unwrap(),
                serde_json::to_value(control.state(true)).unwrap()
            );
            let trace = trace.borrow();
            for core in 0..4 {
                let row = trace.records.iter().find(|r| r["core"] == core).unwrap();
                assert_eq!(row["r0"], r0);
                assert_eq!(row["owner"]["r1"], 1);
                assert_eq!(row["owner"]["fp"], fp);
                let reads = &row["owner"]["reads"];
                assert!(reads.get(expected).is_some(), "{pc:x}: {row}");
                if pc == 0x11f9ebc && r0 != 0 {
                    assert_eq!(reads["popSucceeded"], false);
                    assert!(reads.get("poppedRecordBytes").is_none());
                }
                if pc == 0x11f9280 && body != 0x1be67d4 {
                    assert_eq!(reads["payloadQualified"], false);
                    assert!(reads.get("payloadPrefixBytes").is_none());
                }
            }
            assert!(observed.device_records().is_empty());
            assert!(observed.frontier(false).is_empty());
        }
        assert!(qualify(&[], false, true).is_err());
    }
}
