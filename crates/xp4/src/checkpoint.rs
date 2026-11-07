//! Session checkpoints: the board's durable state plus this wrapper's inputs.
//! A checkpoint restores only into a session from the same factory, DAT and
//! board profile; scenario fixtures and hooks are rebuilt by that factory.
use super::*;
use serde::{Deserialize, Serialize};

const SCHEMA: u32 = 1;
const MEMORY_CHUNK_LIMIT: u32 = 16 << 20;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Saved {
    schema: u32,
    identity: String,
    #[serde(with = "serde_bytes_vec")]
    board: Vec<u8>,
    virtual_buttons: BTreeMap<u32, bool>,
    spi2_trace: Option<(Vec<Value>, usize)>,
}

pub(crate) mod serde_bytes_vec {
    use serde::{Deserialize, Deserializer, Serializer};
    pub fn serialize<S: Serializer>(value: &[u8], serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_bytes(value)
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<u8>, D::Error> {
        Vec::<u8>::deserialize(deserializer)
    }
}

pub(crate) fn identity(probe: Probe, source: &[u8], measured: bool) -> String {
    let profile = if probe >= Probe::ErasedFlash {
        ERASED_FLASH_PROFILE
    } else {
        BOARD_PROFILE
    };
    let mut identity = format!(
        "{probe:?}/{}/{}",
        fujimod_firmware::sha256(source),
        fujimod_firmware::sha256(profile.as_bytes())
    );
    if measured {
        identity += &format!(
            "/measured:{}",
            fujimod_firmware::sha256(MEASURED_PROFILE.as_bytes())
        );
    }
    if probe == Probe::LinkedDmaInput {
        identity += &format!(
            "/bis-peer:1/clock:{}:{}",
            LINKED_DMA_INSTRUCTIONS_PER_TICK.0, LINKED_DMA_INSTRUCTIONS_PER_TICK.1
        );
    }
    identity
}

impl Xp4Session {
    fn checkpoint_boundary(&self) -> Result<(), String> {
        if self.rec_state_memory_watch_started.get()
            || self
                .rec_input_trace
                .as_ref()
                .is_some_and(|t| t.borrow().enabled)
        {
            return Err("Checkpoint requires REC traces and watches to be off".into());
        }
        Ok(())
    }

    pub fn checkpoint(&self) -> Result<Vec<u8>, String> {
        self.checkpoint_boundary()?;
        let saved = Saved {
            schema: SCHEMA,
            identity: self.checkpoint_identity.clone(),
            board: self.board.checkpoint()?,
            virtual_buttons: self.virtual_buttons.borrow().clone(),
            spi2_trace: self.spi2_trace.as_ref().map(|t| {
                let t = t.borrow();
                (t.records.clone(), t.dropped)
            }),
        };
        let mut bytes = Vec::new();
        ciborium::into_writer(&saved, &mut bytes).map_err(|e| format!("Checkpoint encode: {e}"))?;
        Ok(bytes)
    }

    pub fn restore_checkpoint(&self, bytes: &[u8]) -> Result<(), String> {
        self.checkpoint_boundary()?;
        let mut input = bytes;
        let saved: Saved =
            ciborium::from_reader(&mut input).map_err(|e| format!("Checkpoint decode: {e}"))?;
        if !input.is_empty() || saved.schema != SCHEMA {
            return Err("Unknown session checkpoint format".into());
        }
        if saved.identity != self.checkpoint_identity {
            return Err(format!(
                "Checkpoint belongs to {}, not {}",
                saved.identity, self.checkpoint_identity
            ));
        }
        if saved.spi2_trace.is_some() != self.spi2_trace.is_some()
            || (!saved.virtual_buttons.is_empty() && self.power3_source_sha256.is_none())
        {
            return Err("Checkpoint scenario wiring differs".into());
        }
        self.board.restore_checkpoint(&saved.board)?;
        *self.virtual_buttons.borrow_mut() = saved.virtual_buttons;
        if let (Some(trace), Some((records, dropped))) = (&self.spi2_trace, saved.spi2_trace) {
            let mut trace = trace.borrow_mut();
            trace.records = records;
            trace.dropped = dropped;
        }
        Ok(())
    }

    fn memory_extent(&self, address: u32, length: u32) -> Result<(), String> {
        let end = u64::from(address) + u64::from(length);
        if length == 0 || length > MEMORY_CHUNK_LIMIT {
            return Err("Checkpoint memory chunks are 1..=16 MiB".into());
        }
        self.board
            .checkpoint_regions()
            .iter()
            .any(|&(base, size)| base <= u64::from(address) && end <= base + size as u64)
            .then_some(())
            .ok_or_else(|| "Checkpoint memory chunk is outside one physical region".into())
    }

    pub fn checkpoint_memory(&self, address: u32, length: u32) -> Result<Vec<u8>, String> {
        self.memory_extent(address, length)?;
        let mut bytes = vec![0; length as usize];
        self.board.checkpoint_read(u64::from(address), &mut bytes)?;
        if bytes.iter().all(|&b| b == 0) {
            bytes.clear();
        }
        Ok(bytes)
    }

    pub fn restore_memory(&self, address: u32, length: u32, bytes: &[u8]) -> Result<(), String> {
        self.checkpoint_boundary()?;
        self.memory_extent(address, length)?;
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

#[cfg(test)]
mod tests {
    use super::*;

    fn copy_memory(from: &Xp4Session, to: &Xp4Session) {
        for (base, size) in from.board.checkpoint_regions() {
            let mut address = base;
            while address < base + size as u64 {
                let length =
                    (base + size as u64 - address).min(u64::from(MEMORY_CHUNK_LIMIT)) as u32;
                let bytes = from.checkpoint_memory(address as u32, length).unwrap();
                to.restore_memory(address as u32, length, &bytes).unwrap();
                address += u64::from(length);
            }
        }
    }

    #[test]
    #[ignore = "requires the official X-T4 2.12 DAT"]
    fn actual_firmware_checkpoint_resumes_identically_in_a_fresh_session() {
        let firmware = std::fs::read(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../FWUP0024.DAT"),
        )
        .unwrap();
        let batches = |key: &str, default: u32| {
            std::env::var(key)
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(default)
        };
        let source = Xp4Session::discover_linked_dma_input(&firmware).unwrap();
        for _ in 0..batches("XP4_CHECKPOINT_BEFORE", 20) {
            source.execute(1_000_000).unwrap();
        }
        let saved = source.checkpoint().unwrap();
        let target = Xp4Session::discover_linked_dma_input(&firmware).unwrap();
        copy_memory(&source, &target);
        target.restore_checkpoint(&saved).unwrap();
        assert_eq!(target.state(), source.state());
        assert_eq!(target.checkpoint().unwrap(), saved);
        for _ in 0..batches("XP4_CHECKPOINT_AFTER", 10) {
            assert_eq!(
                target.execute(1_000_000).unwrap(),
                source.execute(1_000_000).unwrap()
            );
        }
        assert_eq!(target.checkpoint().unwrap(), source.checkpoint().unwrap());
        assert_eq!(target.diagnostics().unwrap(), source.diagnostics().unwrap());
        let other = Xp4Session::discover_fff74000_ready(&firmware).unwrap();
        assert!(other.restore_checkpoint(&saved).is_err());
    }
}
