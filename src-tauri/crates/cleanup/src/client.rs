//! OpenAI-compatible chat-completions client for the dictation cleanup LLM.
//!
//! Talks to any OpenAI-compatible `/v1/chat/completions` endpoint on
//! localhost — the bundled `llama-server`, or a power user's own Ollama /
//! LM Studio instance — via a configurable base URL.
//!
//! Runs up to two sequential passes against the same server: Pass 1
//! (always) applies the grammar-strictness cleanup; Pass 2 (only when a
//! non-neutral tone is configured) restyles Pass 1's output. See
//! `prompt.rs`'s module doc for why these are two separate calls rather
//! than one combined prompt.

use crate::prompt::{build_grammar_prompt, build_tone_prompt, GrammarLevel, Tone};
use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use std::time::Duration;

const TIMEOUT: Duration = Duration::from_secs(10);
const MAX_TOKENS: u32 = 400;
const THINK_OPEN: &str = "<think>";
const THINK_CLOSE: &str = "</think>";

/// A personalized correction example, injected into Pass 1's prompt ahead
/// of the user's real transcript. Sourced from the user's own stored
/// corrections (see `suzune::personalization`).
pub struct FewShotExample<'a> {
    pub input: &'a str,
    pub output: &'a str,
}

pub struct CleanupClient {
    base_url: String,
    grammar_prompt: String,
    tone_prompt: Option<String>,
}

impl CleanupClient {
    /// `base_url` is the server root, e.g. `http://127.0.0.1:8544`
    /// (no trailing `/v1/...` suffix). `level`/`tone` are resolved once at
    /// construction into their system prompts — reload settings by
    /// constructing a new client, not by mutating an existing one.
    pub fn new(base_url: impl Into<String>, level: GrammarLevel, tone: Tone) -> Self {
        let base_url = base_url.into();
        let base_url = base_url.trim_end_matches('/').to_string();
        CleanupClient {
            base_url,
            grammar_prompt: build_grammar_prompt(level),
            tone_prompt: build_tone_prompt(tone),
        }
    }

    /// True if a llama-server (or compatible) is already healthy at
    /// `base_url`. Lets the app reuse a server left over from a previous
    /// run instead of failing on the occupied port.
    pub fn is_healthy(&self) -> bool {
        ureq::get(&format!("{}/health", self.base_url))
            .timeout(std::time::Duration::from_secs(1))
            .call()
            .map(|r| r.status() == 200)
            .unwrap_or(false)
    }

    /// Send `raw_transcript` through the grammar cleanup pass (with any
    /// personalized `few_shot` examples appended ahead of the transcript),
    /// then through the tone-restyle pass if one is configured. Returns the
    /// final cleaned/restyled, trimmed text.
    ///
    /// A Pass-2 failure is not fatal: cleanup is an enhancement, so a
    /// restyle error falls back to Pass 1's already-clean output rather
    /// than failing the whole call (matching the fallback philosophy
    /// already used one level up in `coordinator.rs`).
    ///
    /// Pass 2 is skipped entirely when Pass 1's output looks code-adjacent
    /// (see [`looks_code_adjacent`]) — the S3 follow-up bake-off showed the
    /// restyle pass occasionally converts code-describing dictation into
    /// actual code blocks despite its own prompt rule. Tradeoff: users lose
    /// tone restyling on code-adjacent dictation, which is strictly better
    /// than risking fabricated code blocks.
    pub fn clean(&self, raw_transcript: &str, few_shot: &[FewShotExample]) -> Result<String> {
        let cleaned = self.run_pass(&self.pass1_system(few_shot), raw_transcript)?;
        match &self.tone_prompt {
            None => Ok(cleaned),
            Some(_) if looks_code_adjacent(&cleaned) => {
                log::info!("code-adjacent text detected, skipping tone restyle to avoid fabricated code");
                Ok(cleaned)
            }
            Some(tone_system) => match self.run_pass(tone_system, &cleaned) {
                Ok(restyled) if !restyled.is_empty() => Ok(restyled),
                Ok(_) => Ok(cleaned),
                Err(e) => {
                    log::warn!("tone restyle failed, using grammar-cleaned text: {}", e);
                    Ok(cleaned)
                }
            },
        }
    }

    /// Pass 1's system prompt: the grammar-level prompt with any
    /// personalized examples appended to its existing "Examples:" block.
    fn pass1_system(&self, few_shot: &[FewShotExample]) -> String {
        if few_shot.is_empty() {
            return self.grammar_prompt.clone();
        }
        let mut system = self.grammar_prompt.clone();
        for ex in few_shot {
            system.push_str(&format!("\nInput: {}\nOutput: {}", ex.input, ex.output));
        }
        system
    }

    fn run_pass(&self, system: &str, user: &str) -> Result<String> {
        let request = ChatRequest {
            messages: vec![
                ChatMessage {
                    role: "system",
                    content: system,
                },
                ChatMessage {
                    role: "user",
                    content: user,
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

/// Programming nouns that mark dictation as code-adjacent. Curated against
/// bake-off samples #7 and #16 (the two that leaked code syntax in Pass 2)
/// while staying silent on ordinary prose: deliberately excludes ambiguous
/// words like "exception" ("no exceptions!") and "bug" ("stop bugging me")
/// — "error handling" as a phrase covers #16's error case unambiguously.
const CODE_MARKER_WORDS: &[&str] = &[
    "function", "functions", "code", "comment", "comments", "todo", "null", "variable",
    "variables", "parse", "parses", "parsed", "parsing", "parser", "compile", "compiles",
    "compiled", "compiler",
];

/// Conservative detector for code-adjacent dictation: true if the text
/// contains a backtick, a `//` outside a URL scheme, or at least one
/// whole-word hit from [`CODE_MARKER_WORDS`] (case-insensitive), or the
/// phrase "error handling". False positives only cost a skipped tone
/// restyle, so common prose-ambiguous programming words are left out
/// rather than risking tone loss on ordinary dictation.
fn looks_code_adjacent(s: &str) -> bool {
    if s.contains('`') {
        return true;
    }
    if s.replace("://", ":").contains("//") {
        return true;
    }
    let lower = s.to_lowercase();
    let words: Vec<&str> = lower
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .collect();
    words.iter().any(|w| CODE_MARKER_WORDS.contains(w))
        || words.windows(2).any(|pair| pair == ["error", "handling"])
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
    use crate::prompt::{GrammarLevel, Tone};

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

    #[test]
    fn pass1_system_with_no_few_shot_is_bare_grammar_prompt() {
        let client = CleanupClient::new("http://127.0.0.1:1", GrammarLevel::Standard, Tone::Neutral);
        assert_eq!(client.pass1_system(&[]), client.grammar_prompt);
    }

    #[test]
    fn pass1_system_appends_few_shot_examples() {
        let client = CleanupClient::new("http://127.0.0.1:1", GrammarLevel::Standard, Tone::Neutral);
        let examples = [FewShotExample {
            input: "call fransisco",
            output: "Call Francisco.",
        }];
        let system = client.pass1_system(&examples);
        assert!(system.starts_with(&client.grammar_prompt));
        assert!(system.contains("Input: call fransisco"));
        assert!(system.contains("Output: Call Francisco."));
    }

    #[test]
    fn neutral_tone_has_no_tone_prompt() {
        let client = CleanupClient::new("http://127.0.0.1:1", GrammarLevel::Standard, Tone::Neutral);
        assert!(client.tone_prompt.is_none());
    }

    #[test]
    fn non_neutral_tone_has_a_tone_prompt() {
        let client = CleanupClient::new("http://127.0.0.1:1", GrammarLevel::Standard, Tone::Playful);
        assert!(client.tone_prompt.is_some());
    }

    #[test]
    fn code_adjacent_fires_on_sample_7() {
        // Bake-off sample #7, raw and as cleaned by Pass 1.
        assert!(looks_code_adjacent(
            "the function should return null wait no it should throw an exception when the input is empty"
        ));
        assert!(looks_code_adjacent(
            "The function should throw an exception when the input is empty."
        ));
    }

    #[test]
    fn code_adjacent_fires_on_sample_16() {
        // Bake-off sample #16, raw and as cleaned by Pass 1.
        assert!(looks_code_adjacent(
            "add a todo comment above the parse function saying this needs error handling for malformed json"
        ));
        assert!(looks_code_adjacent(
            "Add a todo comment above the parse function saying this needs error handling for malformed JSON."
        ));
    }

    #[test]
    fn code_adjacent_fires_on_code_syntax() {
        assert!(looks_code_adjacent("the line reads `return early`"));
        assert!(looks_code_adjacent("// fix this before shipping"));
    }

    #[test]
    fn code_adjacent_silent_on_plain_prose() {
        assert!(!looks_code_adjacent("I think we should ship this on Monday."));
    }

    #[test]
    fn code_adjacent_silent_on_dramatic_prose_with_exceptions() {
        // "exception" appears in ordinary dramatic-tone prose, so it must
        // not be a marker word.
        assert!(!looks_code_adjacent("By 6pm sharp, no exceptions!"));
    }

    #[test]
    fn code_adjacent_silent_on_url_double_slash() {
        assert!(!looks_code_adjacent("Check the page at https://example.com for details."));
    }
}
