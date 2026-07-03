//! whispr-asr: ASR engine selection, model path resolution, and transcription.
//!
//! v1 scope: resolve and validate model paths for the three supported
//! engines, load a chosen engine, and transcribe 16kHz mono audio. Model
//! *downloading* is a later milestone (the model manager) — this crate only
//! checks whether a model is already present and errors clearly when it is
//! not.

mod engine;
mod paths;

pub use engine::{Engine, Transcript};
pub use paths::{is_downloaded, resolve, EngineKind, ModelPaths};
