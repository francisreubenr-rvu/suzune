//! Integration test against a real llama-server + real GGUF model. Not run by
//! default (spawns a real process, loads a real model, takes real wall
//! time) — run explicitly with:
//!   cargo test -p suzune-cleanup -- --ignored

use std::path::PathBuf;
use suzune_cleanup::{CleanupClient, GrammarLevel, LlamaServer, LlamaServerConfig, Tone};

const PORT: u16 = 8544;

fn config() -> LlamaServerConfig {
    LlamaServerConfig {
        server_binary_path: PathBuf::from("/opt/homebrew/bin/llama-server"),
        model_gguf_path: PathBuf::from(
            "/Volumes/1TB SSD/LM/suzune-models/Qwen2.5-1.5B-Instruct-Q4_K_M.gguf",
        ),
        port: PORT,
        log_path: std::env::temp_dir().join("suzune-cleanup-test.log"),
    }
}

#[test]
#[ignore]
fn cleans_filler_words_and_self_correction() {
    let _server = LlamaServer::spawn(config()).expect("llama-server should spawn and become healthy");
    let client = CleanupClient::new(format!("http://127.0.0.1:{PORT}"), GrammarLevel::Standard, Tone::Neutral);

    let out = client
        .clean("um so basically i think we should uh move the meeting to thursday", &[])
        .expect("cleanup call should succeed");

    assert!(out.contains("Thursday"), "expected 'Thursday' in output: {out}");
    assert!(!out.to_lowercase().contains("um "), "filler 'um' leaked into output: {out}");
    assert!(!out.to_lowercase().contains("uh "), "filler 'uh' leaked into output: {out}");
}

#[test]
#[ignore]
fn does_not_convert_dictated_instruction_into_code() {
    let _server = LlamaServer::spawn(config()).expect("llama-server should spawn and become healthy");
    let client = CleanupClient::new(format!("http://127.0.0.1:{PORT}"), GrammarLevel::Standard, Tone::Neutral);

    let out = client
        .clean("add a todo comment above the parse function saying this needs error handling", &[])
        .expect("cleanup call should succeed");

    assert!(!out.contains("//"), "output was converted into a code comment: {out}");
    assert!(!out.contains("```"), "output contains a code fence: {out}");
}

#[test]
#[ignore]
fn butler_preserves_contractions_and_casual_phrasing() {
    let _server = LlamaServer::spawn(config()).expect("llama-server should spawn and become healthy");
    let client = CleanupClient::new(format!("http://127.0.0.1:{PORT}"), GrammarLevel::Butler, Tone::Neutral);

    let out = client
        .clean("so yeah i think were gonna need a few more days", &[])
        .expect("cleanup call should succeed");

    assert!(out.to_lowercase().contains("gonna"), "Butler should preserve 'gonna': {out}");
    assert!(out.to_lowercase().starts_with("so"), "Butler should keep the opening word 'so': {out}");
}

#[test]
#[ignore]
fn oxford_expands_contractions_and_drops_casual_openers() {
    let _server = LlamaServer::spawn(config()).expect("llama-server should spawn and become healthy");
    let client = CleanupClient::new(format!("http://127.0.0.1:{PORT}"), GrammarLevel::Oxford, Tone::Neutral);

    let out = client
        .clean("so yeah i think were gonna need a few more days", &[])
        .expect("cleanup call should succeed");

    assert!(!out.to_lowercase().contains("gonna"), "Oxford should expand 'gonna': {out}");
    assert!(!out.to_lowercase().starts_with("so yeah"), "Oxford should drop the casual opener: {out}");
}

#[test]
#[ignore]
fn oxford_still_applies_self_correction_and_never_converts_to_code() {
    // The invariant safety rules must hold at every grammar level, not just
    // the validated Casual/Standard baseline.
    let _server = LlamaServer::spawn(config()).expect("llama-server should spawn and become healthy");
    let client = CleanupClient::new(format!("http://127.0.0.1:{PORT}"), GrammarLevel::Oxford, Tone::Neutral);

    let out = client
        .clean("book the seven pm show no wait the nine pm one", &[])
        .expect("cleanup call should succeed");
    assert!(out.contains("nine"), "expected the corrected time to survive: {out}");
    assert!(!out.contains("seven"), "the corrected-away time leaked into output: {out}");

    let out = client
        .clean("add a todo comment above the parse function saying this needs error handling", &[])
        .expect("cleanup call should succeed");
    assert!(!out.contains("//"), "Oxford converted the instruction into a code comment: {out}");
}

#[test]
#[ignore]
fn neutral_tone_skips_the_restyle_pass() {
    let _server = LlamaServer::spawn(config()).expect("llama-server should spawn and become healthy");
    let client = CleanupClient::new(format!("http://127.0.0.1:{PORT}"), GrammarLevel::Standard, Tone::Neutral);

    let out = client
        .clean("I think we should ship this on Monday.", &[])
        .expect("cleanup call should succeed");
    // A faithful cleanup pass on already-clean text should return it
    // essentially unchanged — no restyle flourish should appear.
    assert!(!out.contains('!'), "neutral tone should not add exclamatory flair: {out}");
}

#[test]
#[ignore]
fn dramatic_tone_restyles_without_dropping_facts() {
    let _server = LlamaServer::spawn(config()).expect("llama-server should spawn and become healthy");
    let client = CleanupClient::new(format!("http://127.0.0.1:{PORT}"), GrammarLevel::Standard, Tone::Dramatic);

    let out = client
        .clean("I think we should ship this on Monday.", &[])
        .expect("cleanup call should succeed");
    assert!(
        out.to_lowercase().contains("monday"),
        "dramatic restyle must preserve the factual content (Monday): {out}"
    );
}

#[test]
#[ignore]
fn few_shot_examples_influence_a_specific_correction() {
    // A personalized correction (a name the ASR consistently mishears)
    // should be picked up from the injected few-shot example even though
    // it is not one of the 5 baked-in examples.
    use suzune_cleanup::FewShotExample as Fs;
    let _server = LlamaServer::spawn(config()).expect("llama-server should spawn and become healthy");
    let client = CleanupClient::new(format!("http://127.0.0.1:{PORT}"), GrammarLevel::Standard, Tone::Neutral);

    let few_shot = [Fs {
        input: "call fransisco about the report",
        output: "Call Francisco about the report.",
    }];
    let out = client
        .clean("can you call fransisco tomorrow", &few_shot)
        .expect("cleanup call should succeed");
    assert!(out.contains("Francisco"), "expected the personalized spelling correction to apply: {out}");
}
