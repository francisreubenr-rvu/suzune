//! Microphone capture for suzune.
//!
//! Records from the default input device at its native rate/channel count,
//! downmixes to mono, and resamples to 16kHz f32 (the rate every downstream
//! stage — VAD, ASR — expects). Live 16kHz mono frames (30ms / 480 samples)
//! are broadcast to any subscriber via [`Recorder::sample_receiver`] as they
//! are produced, for VAD gating and level metering; [`Recorder::stop`]
//! returns the full accumulated utterance.
//!
//! Capture pattern (worker thread owns the `cpal::Stream`, command channel
//! drives start/stop, frames flow back over a channel) is adapted from
//! Handy's `audio_toolkit::audio::recorder` (MIT, cjpais/Handy), trimmed to
//! suzune's scope: no built-in VAD gating or spectrum visualizer in this
//! crate — those live in `suzune-vad` and the UI layer respectively.

mod resampler;

use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc, Arc,
};
use std::thread::JoinHandle;

use anyhow::{anyhow, Context, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Device, Sample, SizedSample};

use resampler::FrameResampler;

/// Target sample rate for every stage downstream of capture.
pub const SAMPLE_RATE: u32 = 16_000;
/// Frame duration used for live VAD/level frames.
pub const FRAME_MS: u32 = 30;
/// Frame length in samples at [`SAMPLE_RATE`] for [`FRAME_MS`].
pub const FRAME_SAMPLES: usize = (SAMPLE_RATE * FRAME_MS / 1000) as usize; // 480

enum Cmd {
    Start,
    Stop(mpsc::Sender<Vec<f32>>),
    Shutdown,
}

enum RawChunk {
    Samples(Vec<f32>),
    EndOfStream,
}

/// Captures audio from the default input device and produces a 16kHz mono
/// `f32` utterance buffer, plus a live stream of 30ms frames while recording.
///
/// Not `Clone`; share via `Arc` if multiple owners are needed. Safe to use
/// from any thread — the audio callback runs on cpal's own thread, and all
/// communication with it goes through channels.
pub struct Recorder {
    cmd_tx: Option<mpsc::Sender<Cmd>>,
    worker: Option<JoinHandle<()>>,
    frame_tx: crossbeam_channel::Sender<Vec<f32>>,
    frame_rx: crossbeam_channel::Receiver<Vec<f32>>,
    preferred_device: Option<String>,
    /// Name of the input device actually opened, set once `open_device`
    /// succeeds. Diagnostic-only — lets callers report which mic silence
    /// came from instead of just "no speech detected".
    device_name: Option<String>,
}

impl Recorder {
    /// Construct a recorder. Does not touch the microphone yet — device
    /// access is deferred to the first [`Recorder::start`] call so that
    /// constructing a `Recorder` can never fail due to missing hardware or
    /// denied permissions.
    pub fn new() -> Self {
        Self::with_preferred_device(None)
    }

    /// Like [`Recorder::new`], but tries the named input device first —
    /// pinning a mic regardless of the system default (macOS Continuity
    /// silently re-grabs the default for a nearby iPhone). Falls back to
    /// the usual candidate order if the named device is absent or broken.
    pub fn with_preferred_device(preferred_device: Option<String>) -> Self {
        let (frame_tx, frame_rx) = crossbeam_channel::unbounded();
        Recorder {
            cmd_tx: None,
            worker: None,
            frame_tx,
            frame_rx,
            preferred_device,
            device_name: None,
        }
    }

    /// Live 16kHz mono frames of [`FRAME_SAMPLES`] length, emitted while
    /// recording is active. Clone freely — every clone gets every frame.
    pub fn sample_receiver(&self) -> crossbeam_channel::Receiver<Vec<f32>> {
        self.frame_rx.clone()
    }

    /// Name of the input device actually opened for capture, once `start()`
    /// has succeeded at least once. `None` before the first successful
    /// `start()` (device access is deferred, see [`Recorder::new`]).
    pub fn device_name(&self) -> Option<&str> {
        self.device_name.as_deref()
    }

    /// Open the microphone (first call only) and begin recording. Returns a
    /// clear error if no input device is available or the OS denies mic
    /// permission — never panics on device failure.
    pub fn start(&mut self) -> Result<()> {
        if self.cmd_tx.is_none() {
            self.open_device()?;
        }
        self.cmd_tx
            .as_ref()
            .expect("device opened above")
            .send(Cmd::Start)
            .map_err(|_| anyhow!("audio worker thread is not running"))?;
        Ok(())
    }

    /// Stop recording and return the full 16kHz mono utterance captured
    /// since [`Recorder::start`]. The microphone stream stays open so a
    /// subsequent `start()` is fast.
    pub fn stop(&mut self) -> Result<Vec<f32>> {
        let cmd_tx = self
            .cmd_tx
            .as_ref()
            .ok_or_else(|| anyhow!("recorder was never started"))?;
        let (resp_tx, resp_rx) = mpsc::channel();
        cmd_tx
            .send(Cmd::Stop(resp_tx))
            .map_err(|_| anyhow!("audio worker thread is not running"))?;
        resp_rx
            .recv()
            .context("audio worker thread dropped without replying")
    }

    fn open_device(&mut self) -> Result<()> {
        let (raw_tx, raw_rx) = mpsc::channel::<RawChunk>();
        let (cmd_tx, cmd_rx) = mpsc::channel::<Cmd>();
        let (init_tx, init_rx) = mpsc::sync_channel::<Result<String, String>>(1);
        let frame_tx = self.frame_tx.clone();
        let preferred = self.preferred_device.clone();

        let worker = std::thread::spawn(move || {
            let stop_flag = Arc::new(AtomicBool::new(false));
            let stream_stop_flag = stop_flag.clone();

            // The default device can advertise configs that fail at
            // build time (Continuity/Bluetooth mics that have gone
            // away or renegotiated). Try every candidate device until
            // one actually yields a playing stream.
            let init_result = (|| -> Result<(cpal::Stream, u32, String), String> {
                let host = cpal::default_host();
                let mut last_err = "no input microphone found".to_string();
                for device in candidate_devices(&host, preferred.as_deref()) {
                    let name = device
                        .name()
                        .unwrap_or_else(|_| "unknown device".to_string());
                    match try_open_stream(
                        &device,
                        raw_tx.clone(),
                        stream_stop_flag.clone(),
                    ) {
                        Ok((stream, in_rate)) => return Ok((stream, in_rate, name)),
                        Err(e) => {
                            log::warn!("suzune-audio: device {name:?} unusable: {e}");
                            last_err = e;
                        }
                    }
                }
                Err(last_err)
            })();

            match init_result {
                Ok((stream, in_rate, name)) => {
                    let _ = init_tx.send(Ok(name));
                    run_worker(in_rate, raw_rx, cmd_rx, frame_tx, stop_flag);
                    drop(stream);
                }
                Err(msg) => {
                    log::error!("suzune-audio: {msg}");
                    let _ = init_tx.send(Err(msg));
                }
            }
        });

        match init_rx.recv() {
            Ok(Ok(name)) => {
                self.cmd_tx = Some(cmd_tx);
                self.worker = Some(worker);
                self.device_name = Some(name);
                Ok(())
            }
            Ok(Err(msg)) => {
                let _ = worker.join();
                Err(classify_device_error(&msg))
            }
            Err(recv_err) => {
                let _ = worker.join();
                Err(anyhow!("audio worker failed to initialize: {recv_err}"))
            }
        }
    }
}

impl Default for Recorder {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for Recorder {
    fn drop(&mut self) {
        if let Some(tx) = self.cmd_tx.take() {
            let _ = tx.send(Cmd::Shutdown);
        }
        if let Some(h) = self.worker.take() {
            let _ = h.join();
        }
    }
}

/// Names of all available input devices, for the settings UI mic picker.
/// Returns an empty list if the host cannot be enumerated.
pub fn input_device_names() -> Vec<String> {
    let host = cpal::default_host();
    match host.input_devices() {
        Ok(devices) => devices.filter_map(|d| d.name().ok()).collect(),
        Err(e) => {
            log::warn!("suzune-audio: could not list input devices: {e}");
            Vec::new()
        }
    }
}

fn classify_device_error(msg: &str) -> anyhow::Error {
    let normalized = msg.to_lowercase();
    if normalized.contains("access is denied")
        || normalized.contains("permission denied")
        || normalized.contains("0x80070005")
    {
        anyhow!("microphone permission denied: {msg}")
    } else if normalized.contains("no input device") {
        anyhow!("no input microphone found: {msg}")
    } else {
        anyhow!("microphone error: {msg}")
    }
}

/// Candidate input devices in preference order: the user-pinned device
/// (if any) first, then the system default, then the built-in microphone
/// (the device most likely to actually work when an external/Continuity
/// mic has gone away), then the rest.
fn candidate_devices(host: &cpal::Host, preferred: Option<&str>) -> Vec<Device> {
    let default_name = host
        .default_input_device()
        .and_then(|d| d.name().ok())
        .unwrap_or_default();
    let mut pinned = Vec::new();
    let mut default_dev = Vec::new();
    let mut builtin = Vec::new();
    let mut rest = Vec::new();
    let iter = match host.input_devices() {
        Ok(it) => it,
        Err(e) => {
            log::warn!("suzune-audio: could not enumerate input devices: {e}");
            return host.default_input_device().into_iter().collect();
        }
    };
    for device in iter {
        let name = device.name().unwrap_or_default();
        if Some(name.as_str()) == preferred {
            pinned.push(device);
        } else if name == default_name {
            default_dev.push(device);
        } else if name.to_lowercase().contains("macbook") || name.to_lowercase().contains("built-in")
        {
            builtin.push(device);
        } else {
            rest.push(device);
        }
    }
    if preferred.is_some() && pinned.is_empty() {
        log::warn!("suzune-audio: preferred device {:?} not found, using fallback order", preferred);
    }
    pinned
        .into_iter()
        .chain(default_dev)
        .chain(builtin)
        .chain(rest)
        .collect()
}

/// Try the device's preferred config first, then its default config,
/// then every supported config range at its max rate, until one both
/// builds and plays.
fn try_open_stream(
    device: &Device,
    raw_tx: mpsc::Sender<RawChunk>,
    stop_flag: Arc<AtomicBool>,
) -> Result<(cpal::Stream, u32), String> {
    let mut configs: Vec<cpal::SupportedStreamConfig> = Vec::new();
    if let Ok(c) = preferred_input_config(device) {
        configs.push(c);
    }
    if let Ok(c) = device.default_input_config() {
        if !configs.iter().any(|x| x == &c) {
            configs.push(c);
        }
    }
    if let Ok(ranges) = device.supported_input_configs() {
        for r in ranges {
            let c = r.with_max_sample_rate();
            if !configs.iter().any(|x| x == &c) {
                configs.push(c);
            }
        }
    }
    if configs.is_empty() {
        return Err("device reports no usable input configs".to_string());
    }

    let mut last_err = String::new();
    for config in configs {
        let in_rate = config.sample_rate().0;
        let channels = config.channels() as usize;
        match build_input_stream(device, &config, raw_tx.clone(), channels, stop_flag.clone()) {
            Ok(stream) => match stream.play() {
                Ok(()) => {
                    log::info!(
                        "suzune-audio: device={:?} rate={} channels={} format={:?}",
                        device.name(),
                        in_rate,
                        channels,
                        config.sample_format()
                    );
                    return Ok((stream, in_rate));
                }
                Err(e) => last_err = format!("failed to start microphone stream: {e}"),
            },
            Err(e) => last_err = format!("failed to build input stream: {e}"),
        }
    }
    Err(last_err)
}

/// Pick the device's native/default sample rate and best available sample
/// format. We deliberately do not force the hardware into a non-native
/// rate — some devices (Bluetooth codecs, certain drivers) misbehave when
/// asked to run outside their default — and instead resample in software.
fn preferred_input_config(device: &Device) -> Result<cpal::SupportedStreamConfig> {
    let default_config = device
        .default_input_config()
        .context("failed to read default input config")?;
    let target_rate = default_config.sample_rate();

    let supported = match device.supported_input_configs() {
        Ok(configs) => configs,
        Err(e) => {
            log::warn!("suzune-audio: could not enumerate input configs ({e}), using default");
            return Ok(default_config);
        }
    };

    let mut best: Option<cpal::SupportedStreamConfigRange> = None;
    for candidate in supported {
        if candidate.min_sample_rate() <= target_rate && candidate.max_sample_rate() >= target_rate
        {
            let score = |fmt: cpal::SampleFormat| match fmt {
                cpal::SampleFormat::F32 => 4,
                cpal::SampleFormat::I16 => 3,
                cpal::SampleFormat::I32 => 2,
                _ => 1,
            };
            let better = match &best {
                None => true,
                Some(current) => score(candidate.sample_format()) > score(current.sample_format()),
            };
            if better {
                best = Some(candidate);
            }
        }
    }

    Ok(best
        .map(|c| c.with_sample_rate(target_rate))
        .unwrap_or(default_config))
}

fn build_input_stream(
    device: &Device,
    config: &cpal::SupportedStreamConfig,
    raw_tx: mpsc::Sender<RawChunk>,
    channels: usize,
    stop_flag: Arc<AtomicBool>,
) -> Result<cpal::Stream, cpal::BuildStreamError> {
    match config.sample_format() {
        cpal::SampleFormat::F32 => {
            build_typed_stream::<f32>(device, config, raw_tx, channels, stop_flag)
        }
        cpal::SampleFormat::I16 => {
            build_typed_stream::<i16>(device, config, raw_tx, channels, stop_flag)
        }
        cpal::SampleFormat::I32 => {
            build_typed_stream::<i32>(device, config, raw_tx, channels, stop_flag)
        }
        cpal::SampleFormat::U8 => {
            build_typed_stream::<u8>(device, config, raw_tx, channels, stop_flag)
        }
        other => {
            log::error!("suzune-audio: unsupported sample format {other:?}");
            Err(cpal::BuildStreamError::StreamConfigNotSupported)
        }
    }
}

fn build_typed_stream<T>(
    device: &Device,
    config: &cpal::SupportedStreamConfig,
    raw_tx: mpsc::Sender<RawChunk>,
    channels: usize,
    stop_flag: Arc<AtomicBool>,
) -> Result<cpal::Stream, cpal::BuildStreamError>
where
    T: Sample + SizedSample + Send + 'static,
    f32: cpal::FromSample<T>,
{
    let mut eos_sent = false;

    let callback = move |data: &[T], _: &cpal::InputCallbackInfo| {
        if stop_flag.load(Ordering::Relaxed) {
            if !eos_sent {
                let _ = raw_tx.send(RawChunk::EndOfStream);
                eos_sent = true;
            }
            return;
        }
        eos_sent = false;

        let mono: Vec<f32> = if channels <= 1 {
            data.iter().map(|&s| s.to_sample::<f32>()).collect()
        } else {
            data.chunks_exact(channels)
                .map(|frame| {
                    frame.iter().map(|&s| s.to_sample::<f32>()).sum::<f32>() / channels as f32
                })
                .collect()
        };

        if raw_tx.send(RawChunk::Samples(mono)).is_err() {
            log::error!("suzune-audio: sample channel closed, dropping audio");
        }
    };

    device.build_input_stream(
        &config.clone().into(),
        callback,
        |err| log::error!("suzune-audio: stream error: {err}"),
        None,
    )
}

/// Consumer loop: resamples raw device-rate mono chunks to 16kHz/480-sample
/// frames, broadcasts them live while recording, and accumulates them for
/// the buffer returned by `stop()`.
fn run_worker(
    in_rate: u32,
    raw_rx: mpsc::Receiver<RawChunk>,
    cmd_rx: mpsc::Receiver<Cmd>,
    frame_tx: crossbeam_channel::Sender<Vec<f32>>,
    stop_flag: Arc<AtomicBool>,
) {
    let mut resampler = FrameResampler::new(in_rate, SAMPLE_RATE, FRAME_SAMPLES);
    let mut utterance = Vec::<f32>::new();
    let mut recording = false;

    let emit_frame = |frame: &[f32], recording: bool, utterance: &mut Vec<f32>| {
        if !recording {
            return;
        }
        utterance.extend_from_slice(frame);
        if frame_tx.send(frame.to_vec()).is_err() {
            log::trace!("suzune-audio: no live-frame subscribers");
        }
    };

    loop {
        let chunk = match raw_rx.recv() {
            Ok(c) => c,
            Err(_) => return, // stream/device gone
        };

        let raw = match chunk {
            RawChunk::Samples(s) => s,
            RawChunk::EndOfStream => continue,
        };

        resampler.push(&raw, |frame| emit_frame(frame, recording, &mut utterance));

        while let Ok(cmd) = cmd_rx.try_recv() {
            match cmd {
                Cmd::Start => {
                    stop_flag.store(false, Ordering::Relaxed);
                    utterance.clear();
                    recording = true;
                }
                Cmd::Stop(reply_tx) => {
                    recording = false;
                    stop_flag.store(true, Ordering::Relaxed);

                    // Drain remaining audio until the callback confirms EOS,
                    // so every captured sample lands in `utterance` first.
                    loop {
                        match raw_rx.recv_timeout(std::time::Duration::from_secs(2)) {
                            Ok(RawChunk::Samples(remaining)) => {
                                resampler.push(&remaining, |frame| {
                                    emit_frame(frame, true, &mut utterance)
                                });
                            }
                            Ok(RawChunk::EndOfStream) => break,
                            Err(_) => {
                                log::warn!(
                                    "suzune-audio: timed out waiting for end-of-stream"
                                );
                                break;
                            }
                        }
                    }

                    resampler.finish(|frame| emit_frame(frame, true, &mut utterance));

                    let _ = reply_tx.send(std::mem::take(&mut utterance));
                    stop_flag.store(false, Ordering::Relaxed);
                }
                Cmd::Shutdown => {
                    stop_flag.store(true, Ordering::Relaxed);
                    return;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constants_are_consistent() {
        assert_eq!(FRAME_SAMPLES, 480);
        assert_eq!(SAMPLE_RATE, 16_000);
    }

    #[test]
    fn recorder_construction_never_touches_hardware() {
        // Regression guard: `new()` must not attempt device access, so it
        // can never fail in a headless/CI environment without a mic.
        let _r = Recorder::new();
    }

    #[test]
    fn stop_before_start_errors_clearly() {
        let mut r = Recorder::new();
        let err = r.stop().unwrap_err();
        assert!(err.to_string().contains("never started"));
    }
}
