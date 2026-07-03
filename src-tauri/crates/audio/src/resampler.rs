//! Fixed-size-frame resampler.
//!
//! Adapted from the buffering pattern in Handy's `audio_toolkit::audio::resampler`
//! (MIT, cjpais/Handy): feed arbitrary-length input chunks at the device's native
//! rate, and get fixed-size output frames at the target rate via a callback. The
//! resampler itself only runs when input and output rates differ; mono input at
//! the target rate is passed straight through the framing buffer.

use rubato::{FftFixedIn, Resampler};

/// Input chunk size fed to the FFT resampler. Rubato's `FftFixedIn` requires a
/// fixed chunk length; 1024 input samples is a reasonable balance between
/// resampling latency and FFT overhead at typical device rates (44.1/48kHz).
const RESAMPLER_CHUNK_SAMPLES: usize = 1024;

/// Buffers mono input samples, resamples to `out_hz` if needed, and re-frames
/// the result into fixed-size chunks of `frame_samples` length before invoking
/// the caller's callback.
pub struct FrameResampler {
    resampler: Option<FftFixedIn<f32>>,
    chunk_in: usize,
    in_buf: Vec<f32>,
    frame_samples: usize,
    pending: Vec<f32>,
}

impl FrameResampler {
    /// `frame_samples` is the exact output frame length (e.g. 480 for 30ms @ 16kHz).
    pub fn new(in_hz: u32, out_hz: u32, frame_samples: usize) -> Self {
        assert!(frame_samples > 0, "frame_samples must be > 0");

        let resampler = (in_hz != out_hz).then(|| {
            FftFixedIn::<f32>::new(in_hz as usize, out_hz as usize, RESAMPLER_CHUNK_SAMPLES, 1, 1)
                .expect("failed to construct rubato resampler")
        });

        Self {
            resampler,
            chunk_in: RESAMPLER_CHUNK_SAMPLES,
            in_buf: Vec::with_capacity(RESAMPLER_CHUNK_SAMPLES),
            frame_samples,
            pending: Vec::with_capacity(frame_samples),
        }
    }

    /// Feed a chunk of mono samples at the input rate. Emits every complete
    /// output frame via `emit` as soon as it is available.
    pub fn push(&mut self, mut src: &[f32], mut emit: impl FnMut(&[f32])) {
        if self.resampler.is_none() {
            self.emit_frames(src, &mut emit);
            return;
        }

        while !src.is_empty() {
            let space = self.chunk_in - self.in_buf.len();
            let take = space.min(src.len());
            self.in_buf.extend_from_slice(&src[..take]);
            src = &src[take..];

            if self.in_buf.len() == self.chunk_in {
                if let Ok(out) = self
                    .resampler
                    .as_mut()
                    .unwrap()
                    .process(&[&self.in_buf[..]], None)
                {
                    self.emit_frames(&out[0], &mut emit);
                }
                self.in_buf.clear();
            }
        }
    }

    /// Flush any buffered-but-incomplete input and pending output frame,
    /// zero-padding to reach a full final frame. Call once at end-of-stream.
    pub fn finish(&mut self, mut emit: impl FnMut(&[f32])) {
        if let Some(resampler) = self.resampler.as_mut() {
            if !self.in_buf.is_empty() {
                self.in_buf.resize(self.chunk_in, 0.0);
                if let Ok(out) = resampler.process(&[&self.in_buf[..]], None) {
                    self.emit_frames(&out[0], &mut emit);
                }
                self.in_buf.clear();
            }
        }

        if !self.pending.is_empty() {
            self.pending.resize(self.frame_samples, 0.0);
            emit(&self.pending);
            self.pending.clear();
        }
    }

    fn emit_frames(&mut self, mut data: &[f32], emit: &mut impl FnMut(&[f32])) {
        while !data.is_empty() {
            let space = self.frame_samples - self.pending.len();
            let take = space.min(data.len());
            self.pending.extend_from_slice(&data[..take]);
            data = &data[take..];

            if self.pending.len() == self.frame_samples {
                emit(&self.pending);
                self.pending.clear();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn passthrough_frames_at_matching_rate() {
        let mut r = FrameResampler::new(16000, 16000, 4);
        let mut frames = Vec::new();
        r.push(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0], |f| frames.push(f.to_vec()));
        r.finish(|f| frames.push(f.to_vec()));
        assert_eq!(frames, vec![vec![1.0, 2.0, 3.0, 4.0], vec![5.0, 6.0, 0.0, 0.0]]);
    }

    #[test]
    fn resamples_and_frames_downsample() {
        // 48kHz -> 16kHz: 3x downsample. Feed a few seconds of a sine-ish
        // signal and confirm we get well-formed 480-sample frames out.
        let mut r = FrameResampler::new(48000, 16000, 480);
        let input: Vec<f32> = (0..48000)
            .map(|i| (i as f32 * 0.01).sin() * 0.5)
            .collect();
        let mut frames = Vec::new();
        r.push(&input, |f| frames.push(f.to_vec()));
        r.finish(|f| frames.push(f.to_vec()));

        assert!(!frames.is_empty());
        for f in &frames {
            assert_eq!(f.len(), 480);
        }
    }
}
