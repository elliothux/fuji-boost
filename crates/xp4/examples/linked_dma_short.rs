//! Export actual native states for the linked-input Wasm comparison.
use fuji_boost_xp4::Xp4Session;
use serde_json::json;
use std::{fs, io::Write, path::Path};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()?;
    let output = std::env::args_os()
        .nth(1)
        .ok_or("Pass a new output directory inside this project")?;
    let output = std::path::PathBuf::from(output);
    if output.exists()
        || !output
            .parent()
            .ok_or("Missing output parent")?
            .canonicalize()?
            .starts_with(&root)
    {
        return Err("Output directory must be new and inside this project".into());
    }
    fs::create_dir(&output)?;
    let firmware = fs::read(root.join("FWUP0024.DAT"))?;
    let mut files = Vec::new();
    let mut save = |name: String, raw: String| -> Result<(), std::io::Error> {
        fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(output.join(&name))?
            .write_all(raw.as_bytes())?;
        files.push(json!({"file":name,"bytes":raw.len(),"sha256":fujimod_firmware::sha256(raw.as_bytes())}));
        Ok(())
    };
    for name in ["baseline", "candidate"] {
        let session = if name == "baseline" {
            Xp4Session::discover_fff74000_ready(&firmware)
        } else {
            Xp4Session::discover_linked_dma_input(&firmware)
        }
        .map_err(std::io::Error::other)?;
        for (phase, budget) in [("constructed", 0), ("after-1m", 1_000_000)] {
            if budget != 0 {
                let returned = session.execute(budget).map_err(std::io::Error::other)?;
                assert!(returned["error"].is_null());
                save(
                    format!("{name}-{phase}.returned.json"),
                    returned.to_string(),
                )?;
            }
            let before = session.state().to_string();
            let diagnostics = session
                .diagnostics()
                .map_err(std::io::Error::other)?
                .to_string();
            assert_eq!(
                session.state().to_string(),
                before,
                "Native diagnostics changed public state"
            );
            save(format!("{name}-{phase}.state.json"), before)?;
            save(format!("{name}-{phase}.diagnostics.json"), diagnostics)?;
        }
    }
    let manifest = json!({"execution":"actual-native","budgetPerSession":1000000,
        "firmwareSha256":fujimod_firmware::sha256(&firmware),"files":files,
        "physicalCameraAccess":false,"normalRECProven":false});
    fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output.join("manifest.json"))?
        .write_all(manifest.to_string().as_bytes())?;
    println!("{manifest}");
    Ok(())
}
