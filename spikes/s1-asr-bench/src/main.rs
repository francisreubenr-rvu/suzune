// S1 spike: measure model load time and transcription RTF on this machine
// (M1 Pro) for Parakeet TDT v2 int8 (ONNX, CPU) vs Whisper large-v3-turbo
// (whisper.cpp, Metal). Usage: s1-asr-bench <models-dir> <wav-16k-mono>

use anyhow::{Context, Result};
use std::time::Instant;
use transcribe_rs::{
    onnx::{
        parakeet::{ParakeetModel, ParakeetParams},
        Quantization,
    },
    whisper_cpp::{WhisperEngine, WhisperInferenceParams},
};

fn load_wav(path: &str) -> Result<Vec<f32>> {
    let mut reader = hound::WavReader::open(path).context("open wav")?;
    let spec = reader.spec();
    anyhow::ensure!(
        spec.channels == 1 && spec.sample_rate == 16000,
        "expected 16kHz mono, got {}ch {}Hz",
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
    Ok(samples)
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let models_dir = &args[1];
    let wav_path = &args[2];

    let audio = load_wav(wav_path)?;
    let dur_s = audio.len() as f32 / 16000.0;
    println!("audio: {} ({:.1}s)", wav_path, dur_s);

    // --- Parakeet v2 int8 (ONNX) ---
    let pk_dir = format!("{}/parakeet-tdt-0.6b-v2-int8", models_dir);
    let t0 = Instant::now();
    let mut pk = ParakeetModel::load(std::path::Path::new(&pk_dir), &Quantization::Int8)
        .map_err(|e| anyhow::anyhow!("parakeet load: {e}"))?;
    println!("parakeet load: {:.0}ms", t0.elapsed().as_millis());
    for run in 0..3 {
        let t = Instant::now();
        let res = pk
            .transcribe_with(&audio, &ParakeetParams::default())
            .map_err(|e| anyhow::anyhow!("parakeet transcribe: {e}"))?;
        let ms = t.elapsed().as_millis();
        println!(
            "parakeet run{}: {}ms  RTF={:.3}  text: {}",
            run,
            ms,
            ms as f32 / 1000.0 / dur_s,
            res.text.trim()
        );
    }
    drop(pk);

    // --- Whisper large-v3-turbo (Metal) ---
    let wh_path = format!("{}/ggml-large-v3-turbo.bin", models_dir);
    let t0 = Instant::now();
    let mut wh =
        WhisperEngine::load(std::path::Path::new(&wh_path))
            .map_err(|e| anyhow::anyhow!("whisper load: {e}"))?;
    println!("whisper load: {:.0}ms", t0.elapsed().as_millis());
    for run in 0..3 {
        let t = Instant::now();
        let res = wh
            .transcribe_with(&audio, &WhisperInferenceParams::default())
            .map_err(|e| anyhow::anyhow!("whisper transcribe: {e}"))?;
        let ms = t.elapsed().as_millis();
        println!(
            "whisper run{}: {}ms  RTF={:.3}  text: {}",
            run,
            ms,
            ms as f32 / 1000.0 / dur_s,
            res.text.trim()
        );
    }
    Ok(())
}
