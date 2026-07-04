//! Model path resolution for the ASR engines.
//!
//! v1 scope: given a models root directory, resolve the expected on-disk
//! location for each engine and validate that it is actually present. This
//! crate does not download models — that is a later milestone (the model
//! manager). If a model is missing, `resolve` returns a clear error naming
//! the exact path that was expected.

use std::path::{Path, PathBuf};

use anyhow::{anyhow, Result};

/// ASR engines suzune can use. Each variant maps to one on-disk model
/// resolved relative to a models root directory (e.g. `/Volumes/1TB SSD/LM/suzune-models`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EngineKind {
    /// Parakeet TDT 0.6B v2, int8 ONNX. English-only default (S1 spike: RTF 0.033-0.039, CPU).
    ParakeetV2,
    /// Parakeet TDT 0.6B v3, int8 ONNX. Multilingual (25 EU languages).
    ParakeetV3,
    /// Whisper large-v3-turbo, GGML, whisper.cpp/Metal. 99 languages, optional download.
    WhisperLargeV3Turbo,
}

impl EngineKind {
    /// Human-readable label used in error messages and logs.
    pub fn label(&self) -> &'static str {
        match self {
            EngineKind::ParakeetV2 => "Parakeet TDT 0.6B v2 (int8)",
            EngineKind::ParakeetV3 => "Parakeet TDT 0.6B v3 (int8)",
            EngineKind::WhisperLargeV3Turbo => "Whisper large-v3-turbo",
        }
    }

    /// Expected on-disk location for this engine's model files, relative to `models_root`.
    pub fn expected_path(&self, models_root: &Path) -> PathBuf {
        match self {
            EngineKind::ParakeetV2 => models_root.join("parakeet-tdt-0.6b-v2-int8"),
            EngineKind::ParakeetV3 => models_root.join("parakeet-tdt-0.6b-v3-int8"),
            EngineKind::WhisperLargeV3Turbo => models_root.join("ggml-large-v3-turbo.bin"),
        }
    }

    fn is_parakeet(&self) -> bool {
        matches!(self, EngineKind::ParakeetV2 | EngineKind::ParakeetV3)
    }
}

/// Files that must be present inside a Parakeet model directory for it to be
/// considered downloaded. Mirrors the layout produced by the spike model
/// (`/Volumes/1TB SSD/LM/suzune-models/parakeet-tdt-0.6b-v2-int8`).
const PARAKEET_FILES: &[&str] = &[
    "encoder-model.int8.onnx",
    "decoder_joint-model.int8.onnx",
    "nemo128.onnx",
    "vocab.txt",
    "config.json",
];

/// Checks whether `kind`'s model is fully present under `models_root`.
pub fn is_downloaded(kind: EngineKind, models_root: &Path) -> bool {
    let path = kind.expected_path(models_root);
    if kind.is_parakeet() {
        path.is_dir() && PARAKEET_FILES.iter().all(|f| path.join(f).is_file())
    } else {
        path.is_file()
    }
}

/// Resolves the on-disk path for `kind`, validating that the model is
/// actually downloaded. Returns an error naming the expected path when it
/// is not, so the caller can tell the user exactly what is missing.
pub fn resolve(kind: EngineKind, models_root: &Path) -> Result<PathBuf> {
    let path = kind.expected_path(models_root);
    if is_downloaded(kind, models_root) {
        Ok(path)
    } else {
        Err(anyhow!(
            "{} model not found. Expected at: {}. Download it into the models directory before loading this engine.",
            kind.label(),
            path.display()
        ))
    }
}

/// Small ergonomic wrapper binding a models root directory so callers don't
/// have to thread `models_root` through every call.
pub struct ModelPaths {
    models_root: PathBuf,
}

impl ModelPaths {
    pub fn new(models_root: impl Into<PathBuf>) -> Self {
        Self {
            models_root: models_root.into(),
        }
    }

    pub fn expected_path(&self, kind: EngineKind) -> PathBuf {
        kind.expected_path(&self.models_root)
    }

    pub fn is_downloaded(&self, kind: EngineKind) -> bool {
        is_downloaded(kind, &self.models_root)
    }

    pub fn resolve(&self, kind: EngineKind) -> Result<PathBuf> {
        resolve(kind, &self.models_root)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn tempdir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "suzune-asr-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn expected_path_parakeet_v2() {
        let root = Path::new("/models");
        assert_eq!(
            EngineKind::ParakeetV2.expected_path(root),
            root.join("parakeet-tdt-0.6b-v2-int8")
        );
    }

    #[test]
    fn expected_path_parakeet_v3() {
        let root = Path::new("/models");
        assert_eq!(
            EngineKind::ParakeetV3.expected_path(root),
            root.join("parakeet-tdt-0.6b-v3-int8")
        );
    }

    #[test]
    fn expected_path_whisper() {
        let root = Path::new("/models");
        assert_eq!(
            EngineKind::WhisperLargeV3Turbo.expected_path(root),
            root.join("ggml-large-v3-turbo.bin")
        );
    }

    #[test]
    fn is_downloaded_false_when_missing() {
        let root = tempdir();
        assert!(!is_downloaded(EngineKind::ParakeetV2, &root));
        assert!(!is_downloaded(EngineKind::WhisperLargeV3Turbo, &root));
        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn is_downloaded_true_when_whisper_file_present() {
        let root = tempdir();
        fs::write(root.join("ggml-large-v3-turbo.bin"), b"stub").unwrap();
        assert!(is_downloaded(EngineKind::WhisperLargeV3Turbo, &root));
        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn is_downloaded_true_only_when_all_parakeet_files_present() {
        let root = tempdir();
        let model_dir = root.join("parakeet-tdt-0.6b-v2-int8");
        fs::create_dir_all(&model_dir).unwrap();
        // Write all but one required file.
        for f in &PARAKEET_FILES[..PARAKEET_FILES.len() - 1] {
            fs::write(model_dir.join(f), b"stub").unwrap();
        }
        assert!(!is_downloaded(EngineKind::ParakeetV2, &root));

        fs::write(model_dir.join(PARAKEET_FILES.last().unwrap()), b"stub").unwrap();
        assert!(is_downloaded(EngineKind::ParakeetV2, &root));
        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn resolve_errors_with_expected_path_when_missing() {
        let root = tempdir();
        let err = resolve(EngineKind::ParakeetV2, &root).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("Parakeet TDT 0.6B v2"));
        assert!(msg.contains(&root.join("parakeet-tdt-0.6b-v2-int8").display().to_string()));
        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn model_paths_wrapper_matches_free_functions() {
        let root = tempdir();
        let paths = ModelPaths::new(root.clone());
        assert_eq!(
            paths.expected_path(EngineKind::WhisperLargeV3Turbo),
            EngineKind::WhisperLargeV3Turbo.expected_path(&root)
        );
        assert!(!paths.is_downloaded(EngineKind::WhisperLargeV3Turbo));
        assert!(paths.resolve(EngineKind::WhisperLargeV3Turbo).is_err());
        fs::remove_dir_all(&root).ok();
    }
}
