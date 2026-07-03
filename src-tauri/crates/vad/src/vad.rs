//! Silero VAD wrapper.
//!
//! **Model version note:** this wraps the `vad-rs` crate (cjpais fork),
//! whose ONNX session hardcodes the Silero **v4** graph's input/output
//! tensor names (`input`/`sr`/`h`/`c` -> `hn`/`cn`/`output`, with a 2x1x64
//! recurrent state). Silero v5's published ONNX graph uses a different,
//! simplified signature (single `state` tensor, no separate h/c) and is
//! **not** compatible with `vad-rs` as of this writing. The implementation
//! plan's stated preference for v5 could not be honored without forking
//! `vad-rs` or dropping to `ort` directly for a one-crate feature; v4 is
//! the same model Handy ships in production, so we use it here too. See
//! `implementation_plan.md` D-notes / this file for the tradeoff.
//!
//! Supply a Silero v4 ONNX model file path (e.g.
//! `https://blob.handy.computer/silero_vad_v4.onnx`, the same asset Handy
//! downloads) to [`Vad::new`]. Model files are never bundled in this repo.

use std::path::Path;

use anyhow::{Context, Result};
use vad_rs::Vad as SileroSession;

/// Expected frame length: 30ms at 16kHz.
pub const FRAME_SAMPLES: usize = 480;
const SAMPLE_RATE: usize = 16_000;

/// Thin wrapper around the Silero v4 ONNX session. Holds the model's
/// recurrent state internally, so frames must be pushed in order for a
/// single utterance; call [`Vad::reset`] between utterances.
pub struct Vad {
    session: SileroSession,
}

impl Vad {
    /// Load a Silero v4 ONNX model from `model_path`.
    pub fn new<P: AsRef<Path>>(model_path: P) -> Result<Self> {
        let session = SileroSession::new(&model_path, SAMPLE_RATE)
            .map_err(|e| anyhow::anyhow!("failed to load Silero VAD model: {e}"))
            .with_context(|| {
                format!(
                    "loading VAD model from {}",
                    model_path.as_ref().display()
                )
            })?;
        Ok(Self { session })
    }

    /// Compute the speech probability (0.0-1.0) for one 480-sample (30ms
    /// @ 16kHz) mono frame.
    pub fn is_speech(&mut self, frame_480: &[f32]) -> Result<f32> {
        if frame_480.len() != FRAME_SAMPLES {
            anyhow::bail!(
                "expected a {FRAME_SAMPLES}-sample frame, got {}",
                frame_480.len()
            );
        }
        let result = self
            .session
            .compute(frame_480)
            .map_err(|e| anyhow::anyhow!("Silero VAD inference failed: {e}"))?;
        Ok(result.prob)
    }

    /// Reset the model's recurrent state. Call between utterances so a new
    /// utterance doesn't inherit stale hidden state from the previous one.
    pub fn reset(&mut self) {
        self.session.reset();
    }
}
