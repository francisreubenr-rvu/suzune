//! whispr-cleanup: embedded llama-server process management plus an
//! OpenAI-compatible chat-completions client for LLM-based dictation
//! cleanup (filler removal, self-corrections, punctuation).

mod client;
mod prompt;
mod server;

pub use client::CleanupClient;
pub use prompt::SYSTEM_PROMPT_V3;
pub use server::{LlamaServer, LlamaServerConfig};
