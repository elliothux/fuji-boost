use crate::{Result, json};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

fn side(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_os_string();
    name.push(suffix);
    name.into()
}
fn exclusive(path: &Path) -> std::io::Result<File> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path)
}
pub struct Capture {
    out: PathBuf,
    partial: PathBuf,
    file: Option<File>,
    journal: File,
    manifest: File,
    product: File,
    version: File,
    metadata: Value,
    hash: Sha256,
    completed: u64,
}
impl Capture {
    pub fn new(out: &Path, mut metadata: Value, ram: bool) -> Result<Self> {
        metadata["tool_version"] = json!(env!("CARGO_PKG_VERSION"));
        metadata["started_unix_seconds"] = json!(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_secs()
        );
        let paths = [
            out.to_path_buf(),
            side(out, ".partial"),
            side(out, ".manifest.json"),
            side(out, ".progress.jsonl"),
            side(out, ".product.raw"),
            side(out, ".internal-version.raw"),
        ];
        if paths.iter().any(|p| fs::symlink_metadata(p).is_ok()) {
            return Err("output or sidecar already exists; refusing overwrite or resume".into());
        }
        let manifest = exclusive(&paths[2])?;
        let journal = exclusive(&paths[3])?;
        let product = exclusive(&paths[4])?;
        let version = exclusive(&paths[5])?;
        let file = if ram {
            Some(exclusive(&paths[1])?)
        } else {
            None
        };
        let mut capture = Self {
            out: out.into(),
            partial: paths[1].clone(),
            file,
            journal,
            manifest,
            product,
            version,
            metadata,
            hash: Sha256::new(),
            completed: 0,
        };
        capture.event(json!({"event":"begin","metadata":capture.metadata}))?;
        Ok(capture)
    }
    pub fn event(&mut self, value: Value) -> Result<()> {
        serde_json::to_writer(&mut self.journal, &value)?;
        self.journal.write_all(b"\n")?;
        self.journal.sync_data()?;
        Ok(())
    }
    pub fn info(&mut self, name: &str, raw: &[u8], text: &str) -> Result<()> {
        let file = match name {
            "product" => &mut self.product,
            "internal-version" => &mut self.version,
            _ => return Err("unknown metadata selector".into()),
        };
        file.write_all(raw)?;
        file.sync_all()?;
        self.metadata[name] = json!({"text":text,"raw_bytes":raw.len(),"raw_sha256":format!("{:x}",Sha256::digest(raw))});
        self.event(json!({"event":"device-info","selector":name,"text":text,"raw_bytes":raw.len()}))
    }
    pub fn append(&mut self, data: &[u8], expected: usize) -> Result<()> {
        if data.is_empty()
            || data.len() != expected
            || self.completed + data.len() as u64
                > self.metadata["requested_bytes"]
                    .as_u64()
                    .ok_or("missing requested bytes")?
        {
            return Err("inexact or oversized chunk rejected before file write".into());
        }
        let file = self.file.as_mut().ok_or("not a RAM capture")?;
        if let Err(error) = file.write_all(data).and_then(|_| file.sync_data()) {
            // Keep the last fully committed prefix after a short disk write.
            file.set_len(self.completed)?;
            file.sync_all()?;
            return Err(error.into());
        }
        self.hash.update(data);
        self.completed += data.len() as u64;
        self.event(json!({"event":"chunk","completed_bytes":self.completed,"prefix_sha256":format!("{:x}",self.hash.clone().finalize())}))
    }
    pub fn finish(&mut self, mut complete: bool, error: Option<&str>) -> Result<()> {
        let mut error = error.map(String::from);
        if self.file.is_some()
            && complete
            && self.completed != self.metadata["requested_bytes"].as_u64().unwrap_or(0)
        {
            complete = false;
            error = Some("capture ended before the complete requested range".into());
        }
        if let Some(file) = self.file.take() {
            file.sync_all()?;
            drop(file);
            if complete {
                // hard_link publishes atomically without rename's overwrite race, on the same filesystem.
                if let Err(e) = fs::hard_link(&self.partial, &self.out) {
                    complete = false;
                    error = Some(format!("output publication failed: {e}"));
                } else {
                    fs::remove_file(&self.partial)?;
                }
            }
        }
        let result = json!({"metadata":self.metadata,"complete":complete,"completed_bytes":self.completed,"sha256":format!("{:x}",self.hash.clone().finalize()),"hash_scope":"committed RAM prefix only; RAM is live and not an atomic snapshot","output":if complete {self.out.clone()}else{self.partial.clone()},"error":error});
        serde_json::to_writer_pretty(&mut self.manifest, &result)?;
        self.manifest.write_all(b"\n")?;
        self.manifest.sync_all()?;
        self.event(json!({"event":"finished","complete":complete,"completed_bytes":self.completed,"error":error}))?;
        if !complete && let Some(error) = error {
            return Err(error.into());
        }
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn directory() -> PathBuf {
        static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../research/firmware/xt4-212-current/capture-test-output");
        fs::create_dir_all(&root).unwrap();
        let p = root.join(format!(
            "fuji-capture-test-{}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        fs::create_dir(&p).unwrap();
        p
    }
    #[test]
    fn short_reply_does_not_append_and_partial_is_kept() {
        let dir = directory();
        let out = dir.join("ram.bin");
        let mut c = Capture::new(&out, json!({"requested_bytes":4}), true).unwrap();
        c.append(&[1, 2], 2).unwrap();
        assert!(c.append(&[3], 2).is_err());
        assert!(c.finish(false, Some("short reply")).is_err());
        assert_eq!(fs::read(side(&out, ".partial")).unwrap(), [1, 2]);
        assert!(!out.exists());
        let m: Value =
            serde_json::from_slice(&fs::read(side(&out, ".manifest.json")).unwrap()).unwrap();
        assert_eq!(m["completed_bytes"], 2);
        assert_eq!(m["complete"], false);
        assert_eq!(m["sha256"], format!("{:x}", Sha256::digest([1, 2])));
        drop(c);
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn existing_files_and_publication_races_never_overwrite() {
        let dir = directory();
        let out = dir.join("ram.bin");
        fs::write(&out, b"precious").unwrap();
        assert!(Capture::new(&out, json!({"requested_bytes":2}), true).is_err());
        assert_eq!(fs::read(&out).unwrap(), b"precious");
        fs::remove_file(&out).unwrap();
        let mut c = Capture::new(&out, json!({"requested_bytes":2}), true).unwrap();
        c.append(&[1, 2], 2).unwrap();
        fs::write(&out, b"raced").unwrap();
        assert!(c.finish(true, None).is_err());
        assert_eq!(fs::read(&out).unwrap(), b"raced");
        assert_eq!(fs::read(side(&out, ".partial")).unwrap(), [1, 2]);
        drop(c);
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn exact_complete_capture_is_published_and_hashed() {
        let dir = directory();
        let out = dir.join("ram.bin");
        let mut c = Capture::new(&out, json!({"requested_bytes":3}), true).unwrap();
        c.append(&[1, 2], 2).unwrap();
        c.append(&[3], 1).unwrap();
        c.finish(true, None).unwrap();
        assert_eq!(fs::read(&out).unwrap(), [1, 2, 3]);
        assert!(!side(&out, ".partial").exists());
        assert!(Capture::new(&out, json!({}), true).is_err());
        drop(c);
        fs::remove_dir_all(dir).unwrap();
    }
}
