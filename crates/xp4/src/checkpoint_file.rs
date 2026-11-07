//! FBXP4CK1 files: durable state and deflated non-zero memory chunks.
use crate::{Xp4Session, ram_image::ImageSession};
use flate2::{Compression, read::DeflateDecoder, write::DeflateEncoder};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    io::{Read, Write},
};

#[derive(Deserialize, Serialize)]
struct Header {
    format: u32,
    meta: serde_json::Map<String, serde_json::Value>,
    regions: Vec<(u32, u32)>,
    state: usize,
    chunks: Vec<Chunk>,
}
#[derive(Deserialize, Serialize)]
struct Chunk {
    address: u32,
    bytes: usize,
}

/// Sessions whose durable state and RAM fit an FBXP4CK1 file.
pub trait Checkpointable {
    fn checkpoint(&self) -> Result<Vec<u8>, String>;
    fn restore_checkpoint(&self, bytes: &[u8]) -> Result<(), String>;
    fn checkpoint_regions(&self) -> Vec<(u32, u32)>;
    fn checkpoint_memory(&self, address: u32, length: u32) -> Result<Vec<u8>, String>;
    fn restore_memory(&self, address: u32, length: u32, bytes: &[u8]) -> Result<(), String>;
}

macro_rules! checkpointable {
    ($session:ty) => {
        impl Checkpointable for $session {
            fn checkpoint(&self) -> Result<Vec<u8>, String> {
                <$session>::checkpoint(self)
            }
            fn restore_checkpoint(&self, bytes: &[u8]) -> Result<(), String> {
                <$session>::restore_checkpoint(self, bytes)
            }
            fn checkpoint_regions(&self) -> Vec<(u32, u32)> {
                <$session>::checkpoint_regions(self)
            }
            fn checkpoint_memory(&self, address: u32, length: u32) -> Result<Vec<u8>, String> {
                <$session>::checkpoint_memory(self, address, length)
            }
            fn restore_memory(
                &self,
                address: u32,
                length: u32,
                bytes: &[u8],
            ) -> Result<(), String> {
                <$session>::restore_memory(self, address, length, bytes)
            }
        }
    };
}
checkpointable!(Xp4Session);
checkpointable!(ImageSession);

fn parse(file: &[u8]) -> Result<(Header, usize), String> {
    if file.len() < 12 || &file[..8] != b"FBXP4CK1" {
        return Err("Not an XP4 checkpoint file".into());
    }
    let body = 12 + u32::from_le_bytes(file[8..12].try_into().unwrap()) as usize;
    let header: Header =
        serde_json::from_slice(file.get(12..body).ok_or("Checkpoint header is truncated")?)
            .map_err(|e| e.to_string())?;
    if header.format != 1 || header.state == 0 || header.chunks.iter().any(|c| c.bytes == 0) {
        return Err("Invalid checkpoint header".into());
    }
    let end = header
        .chunks
        .iter()
        .fold(body.checked_add(header.state), |offset, c| {
            offset.and_then(|o| o.checked_add(c.bytes))
        });
    if end != Some(file.len()) {
        return Err("Checkpoint file length differs".into());
    }
    Ok((header, body))
}

pub fn restore(cpu: &impl Checkpointable, file: &[u8]) -> Result<(), String> {
    let (header, body) = parse(file)?;
    if header.regions != cpu.checkpoint_regions() {
        return Err("Checkpoint memory layout differs".into());
    }
    let mut layout = BTreeMap::new();
    for &(base, size) in &header.regions {
        for offset in (0..size).step_by(1 << 20) {
            layout.insert(base + offset, (size - offset).min(1 << 20));
        }
    }
    let mut stored = BTreeMap::new();
    let mut offset = body + header.state;
    for chunk in &header.chunks {
        if !layout.contains_key(&chunk.address) || stored.contains_key(&chunk.address) {
            return Err("Invalid checkpoint chunk address".into());
        }
        stored.insert(chunk.address, &file[offset..offset + chunk.bytes]);
        offset += chunk.bytes;
    }
    for (address, length) in layout {
        let mut bytes = Vec::new();
        if let Some(packed) = stored.get(&address) {
            let mut decoder = DeflateDecoder::new(*packed);
            decoder
                .by_ref()
                .take(u64::from(length) + 1)
                .read_to_end(&mut bytes)
                .map_err(|e| e.to_string())?;
            if bytes.len() != length as usize || decoder.total_in() != packed.len() as u64 {
                return Err("Checkpoint chunk length differs".into());
            }
        }
        cpu.restore_memory(address, length, &bytes)?;
    }
    let _ = header.meta;
    cpu.restore_checkpoint(&file[body..body + header.state])?;
    Ok(())
}

pub fn save(
    cpu: &impl Checkpointable,
    meta: serde_json::Map<String, serde_json::Value>,
) -> Result<Vec<u8>, String> {
    let state = cpu.checkpoint()?;
    let mut header = Header {
        format: 1,
        meta,
        regions: cpu.checkpoint_regions(),
        state: state.len(),
        chunks: Vec::new(),
    };
    let mut payload = state;
    for &(base, size) in &header.regions {
        for offset in (0..size).step_by(1 << 20) {
            let address = base + offset;
            let bytes = cpu.checkpoint_memory(address, (size - offset).min(1 << 20))?;
            if bytes.is_empty() {
                continue;
            }
            let mut encoder = DeflateEncoder::new(Vec::new(), Compression::fast());
            encoder.write_all(&bytes).map_err(|e| e.to_string())?;
            let packed = encoder.finish().map_err(|e| e.to_string())?;
            header.chunks.push(Chunk {
                address,
                bytes: packed.len(),
            });
            payload.extend(packed);
        }
    }
    let json = serde_json::to_vec(&header).map_err(|e| e.to_string())?;
    let mut file = b"FBXP4CK1".to_vec();
    file.extend((json.len() as u32).to_le_bytes());
    file.extend(json);
    file.extend(payload);
    Ok(file)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_invalid_and_truncated_files() {
        assert!(parse(b"FBXP4CK1").is_err());
        for (header, body, valid) in [
            (
                r#"{"format":1,"meta":{},"regions":[],"state":1,"chunks":[]}"#,
                vec![0],
                true,
            ),
            (
                r#"{"format":2,"meta":{},"regions":[],"state":1,"chunks":[]}"#,
                vec![0],
                false,
            ),
            (
                r#"{"format":1,"meta":{},"regions":[],"state":1,"chunks":[]}"#,
                vec![],
                false,
            ),
            (
                r#"{"format":1,"meta":{},"regions":[],"state":1,"chunks":[{"address":0,"bytes":0}]}"#,
                vec![0],
                false,
            ),
        ] {
            let mut file = b"FBXP4CK1".to_vec();
            file.extend((header.len() as u32).to_le_bytes());
            file.extend(header.as_bytes());
            file.extend(body);
            assert_eq!(parse(&file).is_ok(), valid);
            file[8..12].copy_from_slice(&u32::MAX.to_le_bytes());
            assert!(parse(&file).is_err());
        }
    }
}
