//! Manual smoke test: records `SECONDS` of audio from the default input
//! device, prints live 30ms frame levels as they arrive, then reports the
//! total buffer length returned by `stop()`. Not run in CI — requires a
//! real microphone and OS permission grant. Run with:
//!
//!   cargo run -p suzune-audio --example live_mic

use std::time::Duration;
use suzune_audio::Recorder;

const SECONDS: u64 = 8;

fn main() -> anyhow::Result<()> {
    env_logger::init();

    let mut recorder = Recorder::new();
    let frames = recorder.sample_receiver();

    println!("Recording {SECONDS}s from the default input device...");
    recorder.start()?;

    let deadline = std::time::Instant::now() + Duration::from_secs(SECONDS);
    let mut frame_count = 0usize;
    while std::time::Instant::now() < deadline {
        if let Ok(frame) = frames.recv_timeout(Duration::from_millis(100)) {
            frame_count += 1;
            let peak = frame.iter().fold(0.0f32, |m, &s| m.max(s.abs()));
            if frame_count % 10 == 0 {
                println!("frame {frame_count}: peak={peak:.3}");
            }
        }
    }

    let utterance = recorder.stop()?;
    println!(
        "Done. {} live frames observed, {} total samples ({:.2}s at 16kHz).",
        frame_count,
        utterance.len(),
        utterance.len() as f32 / suzune_audio::SAMPLE_RATE as f32
    );

    Ok(())
}
