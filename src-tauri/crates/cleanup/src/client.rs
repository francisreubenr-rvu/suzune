//! OpenAI-compatible chat-completions client for the dictation cleanup LLM.
//!
//! Talks to any OpenAI-compatible `/v1/chat/completions` endpoint on
//! localhost — the bundled `llama-server`, or a power user's own Ollama /
//! LM Studio instance — via a configurable base URL.

use crate::prompt::SYSTEM_PROMPT_V3;
use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use std::time::Duration;

const TIMEOUT: Duration = Duration::from_secs(10);
const MAX_TOKENS: u32 = 400;
const THINK_OPEN: &str = "<think>";
const THINK_CLOSE: &str = "</think>";

pub struct CleanupClient {
    base_url: String,
}

impl CleanupClient {
    /// `base_url` is the server root, e.g. `http://127.0.0.1:8544`
    /// (no trailing `/v1/...` suffix).
    pub fn new(base_url: impl Into<String>) -> Self {
        let base_url = base_url.into();
        let base_url = base_url.trim_end_matches('/').to_string();
        CleanupClient { base_url }
    }

    /// Send `raw_transcript` through the cleanup system prompt and return the
    /// cleaned, trimmed text.
    pub fn clean(&self, raw_transcript: &str) -> Result<String> {
        let request = ChatRequest {
            messages: vec![
                ChatMessage {
                    role: "system",
                    content: SYSTEM_PROMPT_V3,
                },
                ChatMessage {
                    role: "user",
                    content: raw_transcript,
                },
            ],
            temperature: 0.0,
            max_tokens: MAX_TOKENS,
        };

        let url = format!("{}/v1/chat/completions", self.base_url);
        let body = ureq::post(&url)
            .timeout(TIMEOUT)
            .send_json(&request)
            .with_context(|| format!("calling cleanup LLM at {url}"))?
            .into_string()
            .context("reading cleanup LLM response body")?;

        parse_response(&body)
    }
}

#[derive(Serialize)]
struct ChatRequest<'a> {
    messages: Vec<ChatMessage<'a>>,
    temperature: f32,
    max_tokens: u32,
}

#[derive(Serialize)]
struct ChatMessage<'a> {
    role: &'a str,
    content: &'a str,
}

#[derive(Deserialize)]
struct ChatResponse {
    choices: Vec<Choice>,
}

#[derive(Deserialize)]
struct Choice {
    message: ChoiceMessage,
}

#[derive(Deserialize)]
struct ChoiceMessage {
    content: String,
}

/// Parse a chat-completions JSON response body into the cleaned, trimmed
/// output text, defensively stripping any leading `<think>...</think>` block
/// some models emit despite instructions not to.
fn parse_response(body: &str) -> Result<String> {
    let parsed: ChatResponse = serde_json::from_str(body)
        .with_context(|| format!("parsing cleanup LLM response JSON: {body}"))?;
    let content = parsed
        .choices
        .first()
        .ok_or_else(|| anyhow!("cleanup LLM response had no choices: {body}"))?
        .message
        .content
        .as_str();
    Ok(strip_think(content).trim().to_string())
}

/// Strip a single leading `<think>...</think>` reasoning block, if present.
fn strip_think(s: &str) -> &str {
    let trimmed = s.trim_start();
    if let Some(rest) = trimmed.strip_prefix(THINK_OPEN) {
        if let Some(end) = rest.find(THINK_CLOSE) {
            return rest[end + THINK_CLOSE.len()..].trim_start();
        }
    }
    trimmed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_plain_response() {
        let body = r#"{"choices":[{"message":{"content":"Can you send me the report?"}}]}"#;
        assert_eq!(parse_response(body).unwrap(), "Can you send me the report?");
    }

    #[test]
    fn trims_whitespace() {
        let body = r#"{"choices":[{"message":{"content":"  Hey Mike, wait until Friday.  \n"}}]}"#;
        assert_eq!(parse_response(body).unwrap(), "Hey Mike, wait until Friday.");
    }

    #[test]
    fn strips_leading_think_block() {
        let body = r#"{"choices":[{"message":{"content":"<think>reasoning about the sentence</think>Move the meeting to Thursday."}}]}"#;
        assert_eq!(parse_response(body).unwrap(), "Move the meeting to Thursday.");
    }

    #[test]
    fn strips_think_block_with_surrounding_whitespace() {
        let body = "{\"choices\":[{\"message\":{\"content\":\"  <think>\\nstep 1\\nstep 2\\n</think>\\n\\nCleaned text.\"}}]}";
        assert_eq!(parse_response(body).unwrap(), "Cleaned text.");
    }

    #[test]
    fn no_think_block_is_unaffected() {
        assert_eq!(strip_think("Just cleaned text."), "Just cleaned text.");
    }

    #[test]
    fn missing_choices_errors_clearly() {
        let body = r#"{"choices":[]}"#;
        assert!(parse_response(body).is_err());
    }

    #[test]
    fn malformed_json_errors_clearly() {
        let body = "not json";
        assert!(parse_response(body).is_err());
    }
}
