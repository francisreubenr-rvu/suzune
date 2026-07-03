//! Integration test: loads the real Parakeet v2 model from the models root
//! on this machine and transcribes a real sample WAV. Marked `#[ignore]` so
//! `cargo test` stays fast without the model present; run explicitly with
//! `cargo test -p whispr-asr -- --ignored`.

use std::path::{Path, PathBuf};

use whispr_asr::{Engine, EngineKind};

const MODELS_ROOT: &str = "/Volumes/1TB SSD/LM/whispr-models";

fn sample_wav_path() -> PathBuf {
    // CARGO_MANIFEST_DIR = .../src-tauri/crates/asr
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../spikes/s1-asr-bench/samples/jfk.wav")
}

fn load_wav_16k_mono(path: &Path) -> Vec<f32> {
    let mut reader = hound::WavReader::open(path).expect("open wav");
    let spec = reader.spec();
    assert_eq!(spec.channels, 1, "expected mono wav");
    assert_eq!(spec.sample_rate, 16000, "expected 16kHz wav");
    match spec.sample_format {
        hound::SampleFormat::Int => reader
            .samples::<i16>()
            .map(|s| s.expect("sample") as f32 / 32768.0)
            .collect(),
        hound::SampleFormat::Float => {
            reader.samples::<f32>().map(|s| s.expect("sample")).collect()
        }
    }
}

#[test]
#[ignore]
fn parakeet_v2_transcribes_jfk_sample() {
    let models_root = Path::new(MODELS_ROOT);
    let audio = load_wav_16k_mono(&sample_wav_path());

    let mut engine =
        Engine::load(EngineKind::ParakeetV2, models_root).expect("load parakeet v2 engine");

    let transcript = engine
        .transcribe(&audio, Some("en"))
        .expect("transcription should succeed");

    let text = transcript.text.to_lowercase();
    assert!(
        text.contains("ask not what your country can do for you"),
        "unexpected transcript: {}",
        transcript.text
    );
}
