//! S3-follow-up bake-off: runs the 20 existing bench samples through every
//! grammar level and every non-neutral tone, against a real llama-server
//! already running with the shipped cleanup model. Unlike bench.py/
//! bench_v31.py (which duplicate the prompt text in Python), this links
//! `suzune_cleanup` directly — the actual Rust source of truth, so there
//! is nothing to keep in sync as grammar/tone prompts evolve.
//!
//! Prerequisite: llama-server already running on `--port` with the
//! cleanup GGUF loaded (see crates/cleanup/tests/live_server.rs for the
//! spawn pattern, or just run the real app once).
//!
//! Usage (from src-tauri/):
//!   cargo run -p suzune-cleanup --example mode_bench -- \
//!     --samples ../spikes/s3-cleanup-bench/samples.jsonl \
//!     --out-dir ../spikes/s3-cleanup-bench \
//!     --port 8544
//!
//! Writes results-grammar-<level>.jsonl and results-tone-<tone>.jsonl next
//! to samples.jsonl, in the same {id, ms, input, output, expect} shape
//! bench.py/bench_v31.py already use, for side-by-side eyeballing — this
//! is a regression net a human reviews, same as the original bake-off,
//! not an automated pass/fail oracle.

use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::time::Instant;
use suzune_cleanup::{CleanupClient, GrammarLevel, Tone};

#[derive(Deserialize)]
struct Sample {
    id: u32,
    input: String,
    expect: String,
}

#[derive(Serialize)]
struct ResultRow<'a> {
    id: u32,
    ms: u128,
    input: &'a str,
    output: String,
    expect: &'a str,
}

fn arg_value(args: &[String], flag: &str) -> Option<String> {
    args.iter().position(|a| a == flag).and_then(|i| args.get(i + 1).cloned())
}

fn load_samples(path: &std::path::Path) -> Vec<Sample> {
    let file = File::open(path).unwrap_or_else(|e| panic!("open {}: {e}", path.display()));
    BufReader::new(file)
        .lines()
        .map_while(Result::ok)
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(&l).unwrap_or_else(|e| panic!("parse sample line: {e}\n{l}")))
        .collect()
}

fn run_one(client: &CleanupClient, samples: &[Sample], label: &str, out_dir: &std::path::Path) {
    let out_path = out_dir.join(format!("results-{label}.jsonl"));
    let mut out = File::create(&out_path).unwrap_or_else(|e| panic!("create {}: {e}", out_path.display()));
    let mut latencies = Vec::with_capacity(samples.len());

    for s in samples {
        let t0 = Instant::now();
        let output = client.clean(&s.input, &[]).unwrap_or_else(|e| format!("<error: {e}>"));
        let ms = t0.elapsed().as_millis();
        latencies.push(ms);
        let row = ResultRow { id: s.id, ms, input: &s.input, output: output.clone(), expect: &s.expect };
        writeln!(out, "{}", serde_json::to_string(&row).unwrap()).expect("write result row");
        println!("[{label}] #{:02} {:5}ms  {}", s.id, ms, truncate(&output, 70));
    }

    latencies.sort_unstable();
    let n = latencies.len();
    println!(
        "[{label}] n={n} median={}ms p90={}ms max={}ms",
        latencies[n / 2],
        latencies[(n as f64 * 0.9) as usize],
        latencies[n - 1]
    );
}

fn truncate(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        s.chars().take(n).collect::<String>() + "…"
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let samples_path = PathBuf::from(
        arg_value(&args, "--samples").expect("required: --samples <path to samples.jsonl>"),
    );
    let out_dir = arg_value(&args, "--out-dir")
        .map(PathBuf::from)
        .unwrap_or_else(|| samples_path.parent().unwrap_or(std::path::Path::new(".")).to_path_buf());
    let port: u16 = arg_value(&args, "--port")
        .expect("required: --port <llama-server port>")
        .parse()
        .expect("--port must be a number");
    let base_url = format!("http://127.0.0.1:{port}");

    let samples = load_samples(&samples_path);
    println!("loaded {} samples from {}", samples.len(), samples_path.display());

    for level in [
        GrammarLevel::Butler,
        GrammarLevel::Casual,
        GrammarLevel::Standard,
        GrammarLevel::Formal,
        GrammarLevel::Oxford,
    ] {
        let client = CleanupClient::new(&base_url, level, Tone::Neutral);
        run_one(&client, &samples, &format!("grammar-{level}"), &out_dir);
    }

    for tone in [Tone::Playful, Tone::Enthusiastic, Tone::Direct, Tone::Dramatic] {
        let client = CleanupClient::new(&base_url, GrammarLevel::Standard, tone);
        run_one(&client, &samples, &format!("tone-{tone}"), &out_dir);
    }
}
