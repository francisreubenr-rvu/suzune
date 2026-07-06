//! suzune-cleanup: embedded llama-server process management plus an
//! OpenAI-compatible chat-completions client for LLM-based dictation
//! cleanup (filler removal, self-corrections, punctuation) with an
//! optional second tone-restyling pass.

mod client;
mod prompt;
mod server;

pub use client::{CleanupClient, FewShotExample};
pub use prompt::{build_grammar_prompt, build_tone_prompt, GrammarLevel, Tone, SYSTEM_PROMPT_V3};
pub use server::{LlamaServer, LlamaServerConfig};
