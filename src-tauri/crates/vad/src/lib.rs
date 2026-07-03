//! Voice activity detection and utterance endpointing for whispr.
//!
//! See [`vad::Vad`] for the Silero model wrapper (a version-compatibility
//! note there explains why v4, not v5, is wired up) and [`endpointer`] for
//! the pure state machine that turns a probability stream into utterance
//! boundaries.

mod endpointer;
mod vad;

pub use endpointer::{EndpointEvent, Endpointer, SpeechProbability};
pub use vad::{Vad, FRAME_SAMPLES};
