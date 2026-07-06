// M1 integration harness: exercises the full suzune core pipeline
// (frames -> VAD endpointing -> ASR) outside the Tauri app. Optionally
// chains the grammar/tone cleanup pass too (--cleanup), so the whole
// ASR + grammar + tone pipeline can be smoke-tested headlessly against a
// sample WAV without needing the Tauri app or a live mic.
//
// Live mode (default): records from the default mic until the endpointer
// fires SpeechEnd (or --timeout expires), then transcribes and prints.
// Wav mode (--wav <file>): drives the identical pipeline from a 16kHz mono
// wav instead of the mic.
//
// Usage: suzune-pipeline-cli --models <dir> [--wav <file>] [--timeout <secs>]
//          [--cleanup [--grammar <level>] [--tone <tone>]
//           [--cleanup-model <gguf-filename>] [--cleanup-port <port>]
//           [--llama-server <path>]]

use anyhow::{Context, Result};
use std::time::{Duration, Instant};
use suzune_asr::{Engine, EngineKind};
use suzune_audio::{Recorder, FRAME_SAMPLES};
use suzune_cleanup::{CleanupClient, GrammarLevel, LlamaServer, LlamaServerConfig, Tone};
use suzune_vad::{EndpointEvent, Endpointer, Vad};

/// Dedicated test port, distinct from the app's default 8542, so running
/// this alongside a live suzune instance never collides on the port.
const DEFAULT_CLEANUP_PORT: u16 = 8599;
const DEFAULT_CLEANUP_MODEL: &str = "Qwen2.5-1.5B-Instruct-Q4_K_M.gguf";
const DEFAULT_LLAMA_SERVER: &str = "/opt/homebrew/bin/llama-server";

const SPEECH_THRESHOLD: f32 = 0.5;
const MIN_SPEECH_MS: u32 = 150;
const TRAILING_SILENCE_MS: u32 = 700;
const FRAME_MS: u32 = 30;
// Frames of audio kept from before SpeechStart so onset consonants survive.
const PRE_ROLL_FRAMES: usize = 10; // 300ms

struct Utterance {
    samples: Vec<f32>,
}

/// Feed frames through the endpointer, accumulating pre-roll + speech.
/// Returns Some(utterance) on SpeechEnd, None if the source ran dry first.
fn collect_utterance(
    frames: impl Iterator<Item = Vec<f32>>,
    endpointer: &mut Endpointer<Vad>,
) -> Result<Option<Utterance>> {
    let mut pre_roll: Vec<Vec<f32>> = Vec::new();
    let mut speech: Vec<f32> = Vec::new();
    let mut in_speech = false;

    for frame in frames {
        match endpointer.push(&frame)? {
            EndpointEvent::SpeechStart => {
                in_speech = true;
                for f in pre_roll.drain(..) {
                    speech.extend_from_slice(&f);
                }
                speech.extend_from_slice(&frame);
            }
            EndpointEvent::SpeechEnd => {
                speech.extend_from_slice(&frame);
                return Ok(Some(Utterance { samples: speech }));
            }
            EndpointEvent::None => {
                if in_speech {
                    speech.extend_from_slice(&frame);
                } else {
                    pre_roll.push(frame);
                    if pre_roll.len() > PRE_ROLL_FRAMES {
                        pre_roll.remove(0);
                    }
                }
            }
        }
    }
    // Source ended mid-speech (e.g. wav file with no trailing silence).
    if in_speech && !speech.is_empty() {
        return Ok(Some(Utterance { samples: speech }));
    }
    Ok(None)
}

fn wav_frames(path: &str) -> Result<Vec<Vec<f32>>> {
    let mut reader = hound::WavReader::open(path).context("open wav")?;
    let spec = reader.spec();
    anyhow::ensure!(
        spec.channels == 1 && spec.sample_rate == 16000,
        "expected 16kHz mono wav, got {}ch {}Hz",
        spec.channels,
        spec.sample_rate
    );
    let samples: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Int => reader
            .samples::<i16>()
            .map(|s| s.map(|v| v as f32 / 32768.0))
            .collect::<Result<_, _>>()?,
        hound::SampleFormat::Float => reader.samples::<f32>().collect::<Result<_, _>>()?,
    };
    Ok(samples.chunks(FRAME_SAMPLES).map(|c| c.to_vec()).collect())
}

fn arg_value(args: &[String], flag: &str) -> Option<String> {
    args.iter()
        .position(|a| a == flag)
        .and_then(|i| args.get(i + 1).cloned())
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let models_root = arg_value(&args, "--models")
        .context("required: --models <dir> (e.g. '/Volumes/1TB SSD/LM/suzune-models')")?;
    let models_root = std::path::Path::new(&models_root).to_path_buf();
    let timeout_s: u64 = arg_value(&args, "--timeout")
        .map(|v| v.parse())
        .transpose()?
        .unwrap_or(30);

    let vad_path = models_root.join("silero_vad_v4.onnx");
    let vad = Vad::new(&vad_path)
        .with_context(|| format!("load VAD model at {}", vad_path.display()))?;
    let mut endpointer = Endpointer::new(
        vad,
        SPEECH_THRESHOLD,
        MIN_SPEECH_MS,
        TRAILING_SILENCE_MS,
        FRAME_MS,
    );

    let t0 = Instant::now();
    let mut engine = Engine::load(EngineKind::ParakeetV2, &models_root)?;
    eprintln!("engine loaded in {:.1}s", t0.elapsed().as_secs_f32());

    let utterance = if let Some(wav) = arg_value(&args, "--wav") {
        eprintln!("wav mode: {}", wav);
        collect_utterance(wav_frames(&wav)?.into_iter(), &mut endpointer)?
    } else {
        eprintln!(
            "live mode: speak now (endpoints after {}ms silence, {}s timeout)",
            TRAILING_SILENCE_MS, timeout_s
        );
        let mut recorder = Recorder::new();
        let rx = recorder.sample_receiver();
        recorder.start()?;
        let deadline = Instant::now() + Duration::from_secs(timeout_s);
        let frames = std::iter::from_fn(move || {
            if Instant::now() >= deadline {
                return None;
            }
            rx.recv_timeout(Duration::from_millis(200)).ok()
        });
        let u = collect_utterance(frames, &mut endpointer)?;
        recorder.stop()?;
        u
    };

    match utterance {
        Some(u) => {
            let secs = u.samples.len() as f32 / 16000.0;
            let t = engine.transcribe(&u.samples, None)?;
            let raw = t.text.trim().to_string();
            eprintln!("utterance: {:.1}s  asr: {}ms", secs, t.duration.as_millis());

            if args.iter().any(|a| a == "--cleanup") {
                let cleaned = run_cleanup(&args, &models_root, &raw)?;
                println!("raw:     {}", raw);
                println!("cleaned: {}", cleaned);
            } else {
                println!("{}", raw);
            }
        }
        None => eprintln!("no speech detected"),
    }
    Ok(())
}

/// Spin up (or reuse) a llama-server for the cleanup pass and run the raw
/// transcript through it — the same grammar+tone two-pass pipeline the
/// real app's coordinator uses, minus few-shot personalization (this is a
/// headless smoke test, not a personalization harness).
fn run_cleanup(args: &[String], models_root: &std::path::Path, raw: &str) -> Result<String> {
    let level = GrammarLevel::from_setting(&arg_value(args, "--grammar").unwrap_or_default());
    let tone = Tone::from_setting(&arg_value(args, "--tone").unwrap_or_default());
    let port: u16 = arg_value(args, "--cleanup-port")
        .map(|v| v.parse())
        .transpose()?
        .unwrap_or(DEFAULT_CLEANUP_PORT);
    let model = arg_value(args, "--cleanup-model").unwrap_or_else(|| DEFAULT_CLEANUP_MODEL.to_string());
    let server_binary_path = arg_value(args, "--llama-server")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from(DEFAULT_LLAMA_SERVER));

    let base_url = format!("http://127.0.0.1:{port}");
    let probe = CleanupClient::new(&base_url, level, tone);
    let _server; // held only if we spawned one, to keep it alive for the call below
    if probe.is_healthy() {
        eprintln!("reusing existing cleanup server on port {port}");
    } else {
        let gguf = models_root.join(&model);
        let log_path = std::env::temp_dir().join("suzune-pipeline-cli-cleanup.log");
        eprintln!("spawning llama-server (model={model}, port={port})");
        _server = LlamaServer::spawn(LlamaServerConfig {
            server_binary_path,
            model_gguf_path: gguf,
            port,
            log_path,
        })?;
    }
    let client = CleanupClient::new(&base_url, level, tone);
    client.clean(raw, &[])
}
