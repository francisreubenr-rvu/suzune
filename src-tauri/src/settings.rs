use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Persisted app settings. Loaded from `<app-config-dir>/settings.json`;
/// missing file or fields fall back to defaults.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Directory holding ASR/VAD/LLM model files.
    pub models_root: PathBuf,
    /// Exact input-device name to pin (None = system default with
    /// fallback). Pinning defeats macOS Continuity silently re-grabbing
    /// the default mic for a nearby iPhone.
    pub input_device: Option<String>,
    /// Global dictation shortcut (tauri-plugin-global-shortcut syntax).
    pub shortcut: String,
    /// `true`: hold to talk, release to transcribe (push-to-talk).
    /// `false`: press once to start, again to stop (continuous).
    pub push_to_talk: bool,
    /// How to place text into the focused app: "clipboard" (default,
    /// reliable everywhere incl. terminals/Electron), "ax" (write-only
    /// Accessibility insert, no clipboard use but fails silently in some
    /// apps), or "type" (simulate keystrokes).
    pub injection_method: String,
    /// Run the transcript through the local cleanup LLM before injecting.
    pub cleanup_enabled: bool,
    /// Cleanup model GGUF filename, resolved under `models_root`.
    pub cleanup_model: String,
    /// llama-server binary for the cleanup layer.
    pub llama_server_path: PathBuf,
    /// Port the embedded llama-server listens on (localhost only).
    pub cleanup_port: u16,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            models_root: PathBuf::from("/Volumes/1TB SSD/LM/whispr-models"),
            input_device: None,
            shortcut: "alt+space".to_string(),
            push_to_talk: true,
            injection_method: "clipboard".to_string(),
            cleanup_enabled: true,
            cleanup_model: "Qwen2.5-1.5B-Instruct-Q4_K_M.gguf".to_string(),
            llama_server_path: PathBuf::from("/opt/homebrew/bin/llama-server"),
            cleanup_port: 8542,
        }
    }
}

impl Settings {
    pub fn load(config_dir: &std::path::Path) -> Self {
        let path = config_dir.join("settings.json");
        match std::fs::read_to_string(&path) {
            Ok(raw) => serde_json::from_str(&raw).unwrap_or_else(|e| {
                log::warn!("settings.json invalid ({}), using defaults", e);
                Settings::default()
            }),
            Err(_) => Settings::default(),
        }
    }

    pub fn save(&self, config_dir: &std::path::Path) -> anyhow::Result<()> {
        std::fs::create_dir_all(config_dir)?;
        std::fs::write(
            config_dir.join("settings.json"),
            serde_json::to_string_pretty(self)?,
        )?;
        Ok(())
    }
}
