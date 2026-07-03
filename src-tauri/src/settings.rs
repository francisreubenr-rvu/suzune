use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Persisted app settings. Loaded from `<app-config-dir>/settings.json`;
/// missing file or fields fall back to defaults.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Directory holding ASR/VAD/LLM model files.
    pub models_root: PathBuf,
    /// Global dictation shortcut (tauri-plugin-global-shortcut syntax).
    pub shortcut: String,
    /// `true`: hold to talk, release to transcribe. `false`: press toggles.
    pub push_to_talk: bool,
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
            shortcut: "alt+space".to_string(),
            push_to_talk: true,
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
