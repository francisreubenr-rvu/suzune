//! Integration test against a real llama-server + real GGUF model. Not run by
//! default (spawns a real process, loads a ~2.5GB model, takes real wall
//! time) — run explicitly with:
//!   cargo test -p whispr-cleanup -- --ignored

use std::path::PathBuf;
use whispr_cleanup::{CleanupClient, LlamaServer, LlamaServerConfig};

const PORT: u16 = 8544;

fn config() -> LlamaServerConfig {
    LlamaServerConfig {
        server_binary_path: PathBuf::from("/opt/homebrew/bin/llama-server"),
        model_gguf_path: PathBuf::from(
            "/Volumes/1TB SSD/LM/whispr-models/Qwen2.5-1.5B-Instruct-Q4_K_M.gguf",
        ),
        port: PORT,
        log_path: std::env::temp_dir().join("whispr-cleanup-test.log"),
    }
}

#[test]
#[ignore]
fn cleans_filler_words_and_self_correction() {
    let _server = LlamaServer::spawn(config()).expect("llama-server should spawn and become healthy");
    let client = CleanupClient::new(format!("http://127.0.0.1:{PORT}"));

    let out = client
        .clean("um so basically i think we should uh move the meeting to thursday")
        .expect("cleanup call should succeed");

    assert!(out.contains("Thursday"), "expected 'Thursday' in output: {out}");
    assert!(!out.to_lowercase().contains("um "), "filler 'um' leaked into output: {out}");
    assert!(!out.to_lowercase().contains("uh "), "filler 'uh' leaked into output: {out}");
}

#[test]
#[ignore]
fn does_not_convert_dictated_instruction_into_code() {
    let _server = LlamaServer::spawn(config()).expect("llama-server should spawn and become healthy");
    let client = CleanupClient::new(format!("http://127.0.0.1:{PORT}"));

    let out = client
        .clean("add a todo comment above the parse function saying this needs error handling")
        .expect("cleanup call should succeed");

    assert!(!out.contains("//"), "output was converted into a code comment: {out}");
    assert!(!out.contains("```"), "output contains a code fence: {out}");
}
