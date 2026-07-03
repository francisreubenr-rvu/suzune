//! Utterance endpointing: pure state-machine logic that turns a stream of
//! per-frame speech probabilities into `SpeechStart`/`SpeechEnd` boundary
//! events, debounced by a minimum-speech-duration onset filter and a
//! trailing-silence hangover.
//!
//! [`Endpointer`] is generic over anything that can turn a frame into a
//! speech probability ([`SpeechProbability`]), so the state machine itself
//! can be unit-tested with a constructed probability sequence instead of a
//! real Silero model — see the tests below.

use anyhow::Result;

use crate::vad::Vad;

/// Anything that can estimate the probability that a 30ms frame contains
/// speech. Implemented for the real [`Vad`]; test code implements it with
/// a canned sequence of probabilities.
pub trait SpeechProbability {
    fn probability(&mut self, frame: &[f32]) -> Result<f32>;
}

impl SpeechProbability for Vad {
    fn probability(&mut self, frame: &[f32]) -> Result<f32> {
        self.is_speech(frame)
    }
}

/// Boundary event yielded by [`Endpointer::push`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EndpointEvent {
    /// No boundary crossed on this frame.
    None,
    /// Enough consecutive speech frames were just observed to confirm the
    /// start of an utterance.
    SpeechStart,
    /// Enough consecutive silence frames were just observed after speech
    /// to confirm the utterance has ended.
    SpeechEnd,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    Silence,
    Speech,
}

/// Debounced speech/silence state machine driven by per-frame probabilities.
///
/// - A run of consecutive frames at or above `speech_threshold` must total
///   at least `min_speech_ms` before `SpeechStart` fires (filters out
///   brief noise blips).
/// - Once in speech, a run of consecutive below-threshold frames must
///   total at least `trailing_silence_ms` before `SpeechEnd` fires (keeps
///   short pauses mid-utterance from splitting it).
pub struct Endpointer<V: SpeechProbability> {
    vad: V,
    speech_threshold: f32,
    min_speech_frames: usize,
    trailing_silence_frames: usize,

    state: State,
    speech_run: usize,
    silence_run: usize,
}

impl<V: SpeechProbability> Endpointer<V> {
    /// `frame_ms` is the duration of one frame pushed to [`Endpointer::push`]
    /// (30ms for whispr's pipeline). Thresholds are rounded up to whole
    /// frames.
    pub fn new(
        vad: V,
        speech_threshold: f32,
        min_speech_ms: u32,
        trailing_silence_ms: u32,
        frame_ms: u32,
    ) -> Self {
        assert!(frame_ms > 0, "frame_ms must be > 0");
        let frames_for = |ms: u32| ((ms + frame_ms - 1) / frame_ms).max(1) as usize;

        Endpointer {
            vad,
            speech_threshold,
            min_speech_frames: frames_for(min_speech_ms),
            trailing_silence_frames: frames_for(trailing_silence_ms),
            state: State::Silence,
            speech_run: 0,
            silence_run: 0,
        }
    }

    /// Feed one frame and advance the state machine.
    pub fn push(&mut self, frame: &[f32]) -> Result<EndpointEvent> {
        let prob = self.vad.probability(frame)?;
        Ok(self.push_probability(prob))
    }

    fn push_probability(&mut self, prob: f32) -> EndpointEvent {
        let is_speech = prob >= self.speech_threshold;

        match (self.state, is_speech) {
            (State::Silence, true) => {
                self.speech_run += 1;
                if self.speech_run >= self.min_speech_frames {
                    self.state = State::Speech;
                    self.speech_run = 0;
                    self.silence_run = 0;
                    EndpointEvent::SpeechStart
                } else {
                    EndpointEvent::None
                }
            }
            (State::Silence, false) => {
                self.speech_run = 0;
                EndpointEvent::None
            }
            (State::Speech, true) => {
                self.silence_run = 0;
                EndpointEvent::None
            }
            (State::Speech, false) => {
                self.silence_run += 1;
                if self.silence_run >= self.trailing_silence_frames {
                    self.state = State::Silence;
                    self.speech_run = 0;
                    self.silence_run = 0;
                    EndpointEvent::SpeechEnd
                } else {
                    EndpointEvent::None
                }
            }
        }
    }

    /// Reset to the initial silence state, discarding any in-progress run.
    pub fn reset(&mut self) {
        self.state = State::Silence;
        self.speech_run = 0;
        self.silence_run = 0;
    }

    pub fn is_in_speech(&self) -> bool {
        self.state == State::Speech
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Feeds a fixed queue of probabilities regardless of frame content, so
    /// the endpointer state machine can be exercised without a real model.
    struct MockVad {
        probs: std::collections::VecDeque<f32>,
    }

    impl MockVad {
        fn new(probs: impl IntoIterator<Item = f32>) -> Self {
            MockVad {
                probs: probs.into_iter().collect(),
            }
        }
    }

    impl SpeechProbability for MockVad {
        fn probability(&mut self, _frame: &[f32]) -> Result<f32> {
            Ok(self.probs.pop_front().unwrap_or(0.0))
        }
    }

    const FRAME_MS: u32 = 30;
    const DUMMY_FRAME: [f32; 480] = [0.0; 480];

    fn push_all<V: SpeechProbability>(ep: &mut Endpointer<V>, n: usize) -> Vec<EndpointEvent> {
        (0..n)
            .map(|_| ep.push(&DUMMY_FRAME).expect("mock vad never errors"))
            .collect()
    }

    #[test]
    fn short_speech_blip_is_filtered_out() {
        // min_speech_ms=90 -> needs 3 frames at 30ms. Only 2 speech frames
        // then back to silence: should never fire SpeechStart.
        let mock = MockVad::new([0.9, 0.9, 0.0, 0.0, 0.0]);
        let mut ep = Endpointer::new(mock, 0.5, 90, 300, FRAME_MS);
        let events = push_all(&mut ep, 5);
        assert!(events.iter().all(|e| *e == EndpointEvent::None));
        assert!(!ep.is_in_speech());
    }

    #[test]
    fn sustained_speech_fires_start_once() {
        let mock = MockVad::new([0.9, 0.9, 0.9, 0.9, 0.9]);
        let mut ep = Endpointer::new(mock, 0.5, 90, 300, FRAME_MS);
        let events = push_all(&mut ep, 5);
        // 3 frames to cross min_speech_frames -> Start on frame 3, then None.
        assert_eq!(
            events,
            vec![
                EndpointEvent::None,
                EndpointEvent::None,
                EndpointEvent::SpeechStart,
                EndpointEvent::None,
                EndpointEvent::None,
            ]
        );
        assert!(ep.is_in_speech());
    }

    #[test]
    fn brief_silence_mid_utterance_does_not_end_it() {
        // trailing_silence_ms=300 -> needs 10 frames of silence.
        // Speech starts, dips for 2 frames (below the hangover), resumes.
        let mut probs = vec![0.9; 3]; // reach speech start
        probs.extend(vec![0.0; 2]); // brief dip, not enough to end
        probs.extend(vec![0.9; 3]); // resumes
        let mock = MockVad::new(probs);
        let mut ep = Endpointer::new(mock, 0.5, 90, 300, FRAME_MS);
        let events = push_all(&mut ep, 8);
        assert!(!events.contains(&EndpointEvent::SpeechEnd));
        assert!(ep.is_in_speech());
    }

    #[test]
    fn sustained_trailing_silence_ends_utterance() {
        let mut probs = vec![0.9; 3]; // start
        probs.extend(vec![0.0; 10]); // 300ms silence -> end
        let mock = MockVad::new(probs);
        let mut ep = Endpointer::new(mock, 0.5, 90, 300, FRAME_MS);
        let events = push_all(&mut ep, 13);
        assert_eq!(
            events.iter().filter(|e| **e == EndpointEvent::SpeechStart).count(),
            1
        );
        assert_eq!(
            events.iter().filter(|e| **e == EndpointEvent::SpeechEnd).count(),
            1
        );
        assert!(!ep.is_in_speech());
        // End must land after the start.
        let start_idx = events.iter().position(|e| *e == EndpointEvent::SpeechStart).unwrap();
        let end_idx = events.iter().position(|e| *e == EndpointEvent::SpeechEnd).unwrap();
        assert!(end_idx > start_idx);
    }

    #[test]
    fn reset_clears_in_progress_state() {
        let mock = MockVad::new([0.9, 0.9, 0.9, 0.9]);
        let mut ep = Endpointer::new(mock, 0.5, 90, 300, FRAME_MS);
        push_all(&mut ep, 3); // now in speech
        assert!(ep.is_in_speech());
        ep.reset();
        assert!(!ep.is_in_speech());
    }

    #[test]
    fn threshold_boundary_is_inclusive() {
        let mock = MockVad::new([0.5, 0.5, 0.5]);
        let mut ep = Endpointer::new(mock, 0.5, 90, 300, FRAME_MS);
        let events = push_all(&mut ep, 3);
        assert_eq!(events[2], EndpointEvent::SpeechStart);
    }
}
