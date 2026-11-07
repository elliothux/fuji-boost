use fujimod_firmware as firmware;
use serde::Serialize;
mod xt4;

const MAX_FILE: usize = 128 << 20;
const MAX_SEGMENT: usize = 64 << 20;
const MAX_DECODED: usize = 128 << 20;

#[derive(Clone, serde::Deserialize, Serialize)]
pub struct Profile {
    pub id: String,
    pub model: String,
    pub version: String,
    pub processor: String,
    pub architecture: String,
    pub size: usize,
    pub sha256: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Segment {
    pub index: usize,
    pub offset: usize,
    pub packed_size: usize,
    pub size: usize,
    pub sha256: String,
    pub base: Option<u64>,
    pub entry: Option<u64>,
    pub initialized_size: Option<u64>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Inspection {
    pub profile: Option<Profile>,
    pub family: Option<&'static str>,
    pub sha256: String,
    pub size: usize,
    pub container_type: u32,
    pub version: String,
    pub payload_offset: usize,
    pub segments: Vec<Segment>,
    pub decoded_size: usize,
}

pub struct FirmwareSession {
    report: Inspection,
    images: Vec<Vec<u8>>,
}

impl FirmwareSession {
    pub fn inspect(source: &[u8]) -> Result<Self, String> {
        if source.len() < 0x20c || source.len() > MAX_FILE {
            return Err("Firmware must be between 524 bytes and 128 MiB.".into());
        }
        let outer = firmware::outer_header(source).map_err(|e| e.to_string())?;
        let sha256 = firmware::sha256(source);
        let profiles: Vec<Profile> =
            serde_json::from_str(include_str!("../../../profiles/index.json"))
                .map_err(|e| format!("Invalid built-in profiles: {e}"))?;
        let profile = profiles
            .into_iter()
            .find(|p| p.sha256 == sha256 && p.size == source.len());
        let inverted = firmware::invert(source);
        let mut images = Vec::new();
        let mut segments = Vec::new();
        let mut pos = 12;
        let mut decoded_size = 0;
        while pos + 8 <= inverted.len() {
            if inverted[pos..pos + 8] != [0, 0x40, 0, 0, 4, 0, 0, 0] {
                pos += 1;
                continue;
            }
            let offset = pos - 12;
            pos += 1;
            let size =
                u32::from_le_bytes(inverted[offset..offset + 4].try_into().unwrap()) as usize;
            if size > MAX_SEGMENT || size.saturating_add(decoded_size) > MAX_DECODED {
                return Err(format!(
                    "Decoded image at {offset:#x} exceeds the memory budget."
                ));
            }
            if let Ok((bytes, compression)) = firmware::decompress(&inverted[offset..]) {
                if images.len() >= 32 {
                    return Err(
                        "More than 32 compressed images; refusing unbounded decoding.".into(),
                    );
                }
                let header = firmware::inner_header(&bytes);
                decoded_size += bytes.len();
                segments.push(Segment {
                    index: images.len(),
                    offset,
                    packed_size: compression.packed_size as usize,
                    size: bytes.len(),
                    sha256: firmware::sha256(&bytes),
                    base: header.as_ref().map(|h| h.base),
                    entry: header.as_ref().map(|h| h.entry),
                    initialized_size: header.as_ref().map(|h| h.initialized_size),
                });
                images.push(bytes);
                pos = offset + compression.packed_size as usize + 12;
            }
        }
        if segments.is_empty() {
            return Err("No valid method-4 images found in this container.".into());
        }
        Ok(Self {
            report: Inspection {
                profile,
                family: xt4::validate(source).ok().map(|()| "xt4-212"),
                sha256,
                size: source.len(),
                container_type: outer.kind,
                version: format!("{:x}.{:02x}", outer.version_major, outer.version_minor),
                payload_offset: outer.payload_offset,
                segments,
                decoded_size,
            },
            images,
        })
    }

    pub fn report(&self) -> &Inspection {
        &self.report
    }

    pub fn read_image(
        &self,
        index: usize,
        offset: usize,
        length: usize,
    ) -> Result<Vec<u8>, String> {
        let image = self.images.get(index).ok_or("Unknown segment.")?;
        if length > 4096 {
            return Err("Reads are limited to 4096 bytes.".into());
        }
        let end = offset.checked_add(length).ok_or("Read overflow.")?;
        let bytes = image
            .get(offset..end)
            .ok_or("Read outside decoded image.")?;
        Ok(bytes.to_vec())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_truncated_unsupported_and_allocation_bombs() {
        assert!(FirmwareSession::inspect(&[]).is_err());
        let mut dat = vec![0; 0x300];
        assert!(FirmwareSession::inspect(&dat).is_err());
        dat[..4].copy_from_slice(&6u32.to_le_bytes());
        let words = [u32::MAX, 20, 1, 0x4000, 4];
        for (i, word) in words.iter().enumerate() {
            dat[0x280 + i * 4..0x284 + i * 4].copy_from_slice(&word.to_le_bytes().map(|b| !b));
        }
        assert!(
            FirmwareSession::inspect(&dat)
                .err()
                .unwrap()
                .contains("memory budget")
        );
    }
}
