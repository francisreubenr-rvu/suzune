//! Engine wrapper around transcribe-rs backends (Parakeet ONNX / Whisper Metal).

use std::any::Any;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::Path;
use std::time::{Duration, Instant};

use anyhow::{anyhow, Result};

use transcribe_rs::onnx::parakeet::{ParakeetModel, ParakeetParams};
use transcribe_rs::onnx::Quantization;
use transcribe_rs::whisper_cpp::{WhisperEngine, WhisperInferenceParams};

use crate::paths::{resolve, EngineKind};

/// Result of one transcription call: text plus wall-clock inference time.
#[derive(Debug, Clone)]
pub struct Transcript {
    pub text: String,
    pub duration: Duration,
}

enum Backend {
    Parakeet(ParakeetModel),
    Whisper(WhisperEngine),
}

/// A loaded ASR engine. Construct with [`Engine::load`].
pub struct Engine {
    kind: EngineKind,
    backend: Backend,
}

impl Engine {
    /// Loads the model for `kind` from `models_root`. Fails with a clear
    /// error (naming the expected path) if the model is not downloaded.
    pub fn load(kind: EngineKind, models_root: &Path) -> Result<Self> {
        let path = resolve(kind, models_root)?;
        let backend = match kind {
            EngineKind::ParakeetV2 | EngineKind::ParakeetV3 => {
                let model = ParakeetModel::load(&path, &Quantization::Int8).map_err(|e| {
                    anyhow!(
                        "failed to load {} from {}: {e}",
                        kind.label(),
                        path.display()
                    )
                })?;
                Backend::Parakeet(model)
            }
            EngineKind::WhisperLargeV3Turbo => {
                let engine = WhisperEngine::load(&path).map_err(|e| {
                    anyhow!(
                        "failed to load {} from {}: {e}",
                        kind.label(),
                        path.display()
                    )
                })?;
                Backend::Whisper(engine)
            }
        };
        Ok(Self { kind, backend })
    }

    pub fn kind(&self) -> EngineKind {
        self.kind
    }

    /// Transcribes 16kHz mono PCM audio.
    ///
    /// The underlying engine call is wrapped in `catch_unwind` (the Handy
    /// `transcription.rs` pattern): a panic inside transcribe-rs is caught
    /// and converted into an `Err` instead of unwinding through the
    /// caller's stack. A panic leaves the backend's internal state
    /// unspecified, so callers that hold engines behind e.g.
    /// `Mutex<Option<Engine>>` should drop the `Engine` (not put it back)
    /// when this returns an error whose message contains "panicked" —
    /// this crate has no shared state of its own to poison.
    pub fn transcribe(
        &mut self,
        audio_16k_mono: &[f32],
        language: Option<&str>,
    ) -> Result<Transcript> {
        let backend = &mut self.backend;
        let start = Instant::now();

        let outcome = catch_unwind(AssertUnwindSafe(|| match backend {
            Backend::Parakeet(model) => {
                let params = ParakeetParams {
                    language: language.map(str::to_string),
                    ..Default::default()
                };
                model
                    .transcribe_with(audio_16k_mono, &params)
                    .map_err(|e| anyhow!("parakeet transcription failed: {e}"))
            }
            Backend::Whisper(engine) => {
                let params = WhisperInferenceParams {
                    language: language.map(str::to_string),
                    ..Default::default()
                };
                engine
                    .transcribe_with(audio_16k_mono, &params)
                    .map_err(|e| anyhow!("whisper transcription failed: {e}"))
            }
        }));

        let duration = start.elapsed();

        match outcome {
            Ok(inner) => inner.map(|r| Transcript {
                text: r.text,
                duration,
            }),
            Err(payload) => {
                let msg = panic_message(payload);
                log::error!("ASR engine panicked during transcription: {msg}");
                Err(anyhow!("ASR engine panicked: {msg}"))
            }
        }
    }
}

fn panic_message(payload: Box<dyn Any + Send>) -> String {
    if let Some(s) = payload.downcast_ref::<&str>() {
        s.to_string()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        "unknown panic".to_string()
    }
}
