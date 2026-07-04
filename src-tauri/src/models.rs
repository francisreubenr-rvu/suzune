//! First-run model provisioning: the app ships small and fetches the
//! on-device models into the app-data models directory on first launch,
//! so the user never manually downloads anything.
//!
//! Two assets are required for dictation + cleanup:
//!   - Parakeet TDT 0.6B v2 (int8 ONNX), a tar.gz that extracts to a dir.
//!   - the cleanup GGUF (a single file).
//!
//! The VAD model is not fetched here: push-to-talk / continuous dictation
//! transcribes the whole buffer and does not use VAD. The llama-server
//! binary for the cleanup pass is resolved separately (see settings); if
//! it is absent, cleanup degrades and dictation still works.

use anyhow::{Context, Result};
use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Emitter};

/// Public download URLs for the default model set.
const PARAKEET_URL: &str = "https://blob.handy.computer/parakeet-v2-int8.tar.gz";
/// The directory the Parakeet tar.gz extracts into, relative to models_root.
const PARAKEET_DIR: &str = "parakeet-tdt-0.6b-v2-int8";
const CLEANUP_GGUF_URL: &str =
    "https://huggingface.co/bartowski/Qwen2.5-1.5B-Instruct-GGUF/resolve/main/Qwen2.5-1.5B-Instruct-Q4_K_M.gguf";

/// Progress event payload emitted to the setup UI as "model-setup".
#[derive(Clone, serde::Serialize)]
#[serde(tag = "stage", rename_all = "snake_case")]
enum SetupEvent {
    Downloading { name: String, received: u64, total: u64 },
    Extracting { name: String },
    Done,
    Error { message: String },
}

/// True when every required model file is already present under `models_root`.
pub fn models_present(models_root: &Path, cleanup_model: &str) -> bool {
    let parakeet_ok = models_root.join(PARAKEET_DIR).join("vocab.txt").exists();
    let cleanup_ok = models_root.join(cleanup_model).exists();
    parakeet_ok && cleanup_ok
}

/// Download and install any missing default models into `models_root`,
/// emitting `model-setup` progress events for the UI. Blocks; call from a
/// background thread.
pub fn ensure_models(app: &AppHandle, models_root: &Path, cleanup_model: &str) -> Result<()> {
    std::fs::create_dir_all(models_root)
        .with_context(|| format!("create models dir {}", models_root.display()))?;

    if !models_root.join(PARAKEET_DIR).join("vocab.txt").exists() {
        let tmp = models_root.join("parakeet-v2-int8.tar.gz");
        download(app, "speech model", PARAKEET_URL, &tmp)?;
        emit(app, SetupEvent::Extracting { name: "speech model".into() });
        extract_tar_gz(&tmp, models_root)
            .with_context(|| "extract parakeet archive")?;
        let _ = std::fs::remove_file(&tmp);
    }

    let gguf_dest = models_root.join(cleanup_model);
    if !gguf_dest.exists() {
        download(app, "cleanup model", CLEANUP_GGUF_URL, &gguf_dest)?;
    }

    emit(app, SetupEvent::Done);
    Ok(())
}

fn emit(app: &AppHandle, ev: SetupEvent) {
    if let Err(e) = app.emit("model-setup", &ev) {
        log::error!("model-setup emit failed: {e}");
    }
}

/// Stream a URL to `dest` (via a `.part` temp file), emitting download
/// progress. Renames into place only on success so a partial download
/// never looks complete.
fn download(app: &AppHandle, name: &str, url: &str, dest: &Path) -> Result<()> {
    log::info!("downloading {name} from {url}");
    let resp = ureq::get(url)
        .call()
        .with_context(|| format!("GET {url}"))?;
    let total: u64 = resp
        .header("Content-Length")
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);

    let part = dest.with_extension("part");
    let mut file = File::create(&part)
        .with_context(|| format!("create {}", part.display()))?;
    let mut reader = resp.into_reader();
    let mut buf = vec![0u8; 256 * 1024];
    let mut received: u64 = 0;
    let mut last_emit = 0u64;
    loop {
        let n = reader.read(&mut buf).context("read response body")?;
        if n == 0 {
            break;
        }
        file.write_all(&buf[..n]).context("write model file")?;
        received += n as u64;
        // Throttle events to ~every 2MB so we do not flood the UI.
        if received - last_emit >= 2 * 1024 * 1024 {
            last_emit = received;
            emit(app, SetupEvent::Downloading { name: name.into(), received, total });
        }
    }
    file.flush().ok();
    drop(file);
    std::fs::rename(&part, dest)
        .with_context(|| format!("finalize {}", dest.display()))?;
    emit(app, SetupEvent::Downloading { name: name.into(), received, total });
    Ok(())
}

fn extract_tar_gz(archive: &Path, dest_dir: &Path) -> Result<()> {
    let file = File::open(archive)
        .with_context(|| format!("open {}", archive.display()))?;
    let decoder = flate2::read::GzDecoder::new(file);
    let mut tar = tar::Archive::new(decoder);
    tar.unpack(dest_dir)
        .with_context(|| format!("unpack into {}", dest_dir.display()))?;
    Ok(())
}

/// Default models directory when the user has not configured one: the app's
/// data directory. Keeps the app self-contained and cross-platform (no
/// hardcoded macOS paths).
pub fn default_models_root(app: &AppHandle) -> Result<PathBuf> {
    use tauri::Manager;
    let dir = app.path().app_data_dir().context("resolve app data dir")?;
    Ok(dir.join("models"))
}

/// Report to the setup UI that provisioning failed.
pub fn emit_error(app: &AppHandle, message: String) {
    emit(app, SetupEvent::Error { message });
}
