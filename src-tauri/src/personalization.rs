//! Local-only personalization: an opt-in corrections store the user builds
//! by fixing dictation results in the Settings window, used to (a) inject
//! a few of the most relevant past corrections as extra few-shot examples
//! into the cleanup prompt, and (b) mine a personal vocabulary of
//! consistently-corrected words/names applied as a deterministic
//! pre-processing step before the cleanup pass even runs.
//!
//! Nothing here is written to disk unless the user actively submits a
//! correction — enabling personalization alone stores nothing. Everything
//! lives under `<app-config-dir>` (plain JSON/JSONL, no database), is
//! fully inspectable/clearable by the user, and is never transmitted
//! anywhere. See `docs/legal-review.md` for the privacy note.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::io::Write;
use std::path::Path;

const CORRECTIONS_FILE: &str = "corrections.jsonl";

/// Current unix timestamp in seconds, defaulting to 0 on a clock error
/// (pre-1970 system clock) rather than panicking — this is a timestamp
/// for a local log, not something correctness-critical.
pub fn now_unix() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}
const VOCAB_FILE: &str = "vocabulary.json";

/// One user-confirmed correction: what the ASR engine heard, what the
/// cleanup LLM produced, and what the user actually meant.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CorrectionRecord {
    pub id: u64,
    pub ts: u64,
    pub raw: String,
    pub cleaned: String,
    pub corrected: String,
}

/// Load every well-formed correction record from
/// `<config_dir>/corrections.jsonl`, oldest first. A missing file is not
/// an error (personalization may never have been used) — returns empty.
/// Malformed trailing lines (a truncated write, a hand-edited file) are
/// skipped with a warning rather than failing the whole read; this store
/// is append-only and should degrade gracefully.
pub fn load_corrections(config_dir: &Path) -> Vec<CorrectionRecord> {
    let path = config_dir.join(CORRECTIONS_FILE);
    let raw = match std::fs::read_to_string(&path) {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };
    raw.lines()
        .filter(|line| !line.trim().is_empty())
        .filter_map(|line| match serde_json::from_str::<CorrectionRecord>(line) {
            Ok(rec) => Some(rec),
            Err(e) => {
                log::warn!("skipping malformed correction record: {}", e);
                None
            }
        })
        .collect()
}

/// Append one correction record to the store, creating the config
/// directory if needed.
pub fn append_correction(config_dir: &Path, rec: &CorrectionRecord) -> Result<()> {
    std::fs::create_dir_all(config_dir)
        .with_context(|| format!("creating config dir {}", config_dir.display()))?;
    let path = config_dir.join(CORRECTIONS_FILE);
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .with_context(|| format!("opening {}", path.display()))?;
    let line = serde_json::to_string(rec).context("serializing correction record")?;
    writeln!(file, "{}", line).with_context(|| format!("writing {}", path.display()))?;
    Ok(())
}

/// Delete both the corrections store and the derived vocabulary — the
/// "Clear all corrections" action in Settings. Missing files are not an
/// error.
pub fn clear_corrections(config_dir: &Path) -> Result<()> {
    for name in [CORRECTIONS_FILE, VOCAB_FILE] {
        let path = config_dir.join(name);
        if path.exists() {
            std::fs::remove_file(&path).with_context(|| format!("removing {}", path.display()))?;
        }
    }
    Ok(())
}

/// Score stored corrections against `raw` and return the top `k` — a
/// simple, dependency-free relevance heuristic appropriate for a local
/// store of tens-to-low-hundreds of entries (no embeddings/vector search
/// needed at this scale). `score = 0.5 * token-Jaccard-overlap-with(raw)
/// + 0.5 * recency-rank` (assumes `corrections` is oldest-first, i.e. the
/// order `load_corrections` returns).
pub fn select_few_shot<'a>(
    raw: &str,
    corrections: &'a [CorrectionRecord],
    k: usize,
) -> Vec<&'a CorrectionRecord> {
    if corrections.is_empty() || k == 0 {
        return Vec::new();
    }
    let raw_tokens = tokenize(raw);
    let n = corrections.len();
    let mut scored: Vec<(f64, &CorrectionRecord)> = corrections
        .iter()
        .enumerate()
        .map(|(i, rec)| {
            let overlap = jaccard(&raw_tokens, &tokenize(&rec.raw));
            let recency = if n <= 1 { 1.0 } else { i as f64 / (n - 1) as f64 };
            (0.5 * overlap + 0.5 * recency, rec)
        })
        .collect();
    scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    scored.into_iter().take(k).map(|(_, rec)| rec).collect()
}

fn tokenize(s: &str) -> HashSet<String> {
    s.split_whitespace()
        .map(|w| w.trim_matches(|c: char| !c.is_alphanumeric()).to_lowercase())
        .filter(|w| !w.is_empty())
        .collect()
}

fn jaccard(a: &HashSet<String>, b: &HashSet<String>) -> f64 {
    if a.is_empty() && b.is_empty() {
        return 0.0;
    }
    let intersection = a.intersection(b).count() as f64;
    let union = a.union(b).count() as f64;
    if union == 0.0 {
        0.0
    } else {
        intersection / union
    }
}

/// Extract the "replaced span" between two texts by trimming the common
/// word prefix and suffix; whatever differs in the middle is treated as
/// one substitution. Good enough for the personalization use case (the
/// user corrects one name or short phrase, the rest of the sentence is
/// untouched) without needing a general-purpose diff algorithm. Returns
/// nothing for identical texts, an empty side, a same-case-insensitive
/// span, or a span longer than 3 words on either side (avoids mining a
/// full-sentence rewrite into a bogus "vocabulary" entry).
fn diff_spans(cleaned: &str, corrected: &str) -> Vec<(String, String)> {
    // Words are compared and emitted with surrounding punctuation trimmed
    // (internal apostrophes kept) — `cleaned`/`corrected` are post-cleanup
    // prose with periods/commas, but the vocabulary is applied to raw,
    // unpunctuated ASR text, so a mined entry like "Fransisco." would
    // never match anything.
    let a: Vec<&str> = cleaned.split_whitespace().collect();
    let b: Vec<&str> = corrected.split_whitespace().collect();
    let eq = |x: &str, y: &str| trim_punct(x).eq_ignore_ascii_case(trim_punct(y));

    let mut prefix = 0;
    while prefix < a.len() && prefix < b.len() && eq(a[prefix], b[prefix]) {
        prefix += 1;
    }
    let mut suffix = 0;
    while suffix < a.len() - prefix
        && suffix < b.len() - prefix
        && eq(a[a.len() - 1 - suffix], b[b.len() - 1 - suffix])
    {
        suffix += 1;
    }

    let a_mid = &a[prefix..a.len() - suffix];
    let b_mid = &b[prefix..b.len() - suffix];
    if a_mid.is_empty() || b_mid.is_empty() || a_mid.len() > 3 || b_mid.len() > 3 {
        return Vec::new();
    }
    let from = a_mid.iter().map(|w| trim_punct(w)).collect::<Vec<_>>().join(" ");
    let to = b_mid.iter().map(|w| trim_punct(w)).collect::<Vec<_>>().join(" ");
    if from.is_empty() || to.is_empty() || from.eq_ignore_ascii_case(&to) {
        return Vec::new();
    }
    vec![(from, to)]
}

/// Trim leading/trailing punctuation from a word, keeping internal
/// apostrophes (so "we're" and "O'Brien" survive intact, but "Francisco."
/// and "(Francisco)" become "Francisco").
fn trim_punct(s: &str) -> &str {
    s.trim_matches(|c: char| !c.is_alphanumeric() && c != '\'')
}

/// Mine a personal vocabulary map (misheard span -> corrected span) from
/// the diff between `cleaned` and `corrected` text across all stored
/// corrections. Capitalized spans (names) promote after a single
/// occurrence — the stated real use case, a misheard name; lowercase spans
/// need 2+ occurrences across different records, to avoid overfitting a
/// one-off typo fix into a permanent global substitution.
pub fn build_vocab_map(corrections: &[CorrectionRecord]) -> HashMap<String, String> {
    let mut counts: HashMap<(String, String), u32> = HashMap::new();
    for rec in corrections {
        for (from, to) in diff_spans(&rec.cleaned, &rec.corrected) {
            *counts.entry((from, to)).or_insert(0) += 1;
        }
    }
    let mut vocab = HashMap::new();
    for ((from, to), count) in counts {
        let is_capitalized = from.chars().next().map(|c| c.is_uppercase()).unwrap_or(false)
            || to.chars().next().map(|c| c.is_uppercase()).unwrap_or(false);
        if is_capitalized || count >= 2 {
            vocab.insert(from, to);
        }
    }
    vocab
}

/// Apply a personal vocabulary map to raw ASR text: word-boundary-aware,
/// case-insensitive matching on the source side, exact replacement text on
/// the target side. Run before the cleanup LLM pass so the grammar pass
/// sees the corrected term in context.
pub fn apply_vocabulary(raw: &str, vocab: &HashMap<String, String>) -> String {
    if vocab.is_empty() {
        return raw.to_string();
    }
    let lower_vocab: HashMap<String, &String> =
        vocab.iter().map(|(k, v)| (k.to_lowercase(), v)).collect();

    let mut result = String::with_capacity(raw.len());
    let mut word = String::new();
    let flush = |word: &mut String, result: &mut String| {
        if word.is_empty() {
            return;
        }
        match lower_vocab.get(&word.to_lowercase()) {
            Some(replacement) => result.push_str(replacement),
            None => result.push_str(word),
        }
        word.clear();
    };
    for ch in raw.chars() {
        if ch.is_alphanumeric() || ch == '\'' {
            word.push(ch);
        } else {
            flush(&mut word, &mut result);
            result.push(ch);
        }
    }
    flush(&mut word, &mut result);
    result
}

/// Persist the vocab map derived from the full corrections store. Rebuilt
/// from scratch on every new correction (trivial cost at this data scale)
/// rather than incrementally updated.
pub fn save_vocab_map(config_dir: &Path, vocab: &HashMap<String, String>) -> Result<()> {
    std::fs::create_dir_all(config_dir)
        .with_context(|| format!("creating config dir {}", config_dir.display()))?;
    let path = config_dir.join(VOCAB_FILE);
    let json = serde_json::to_string_pretty(vocab).context("serializing vocabulary map")?;
    std::fs::write(&path, json).with_context(|| format!("writing {}", path.display()))?;
    Ok(())
}

/// Load the persisted vocab map, or an empty map if none exists yet.
pub fn load_vocab_map(config_dir: &Path) -> HashMap<String, String> {
    let path = config_dir.join(VOCAB_FILE);
    match std::fs::read_to_string(&path) {
        Ok(raw) => serde_json::from_str(&raw).unwrap_or_default(),
        Err(_) => HashMap::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rec(id: u64, raw: &str, cleaned: &str, corrected: &str) -> CorrectionRecord {
        CorrectionRecord {
            id,
            ts: id, // monotonic stand-in, order is what matters in tests
            raw: raw.to_string(),
            cleaned: cleaned.to_string(),
            corrected: corrected.to_string(),
        }
    }

    #[test]
    fn corrections_round_trip_through_disk() {
        let dir = tempdir();
        let r1 = rec(1, "call fransisco", "Call Fransisco.", "Call Francisco.");
        append_correction(&dir, &r1).unwrap();
        let loaded = load_corrections(&dir);
        assert_eq!(loaded, vec![r1]);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn missing_corrections_file_is_empty_not_an_error() {
        let dir = tempdir();
        assert!(load_corrections(&dir).is_empty());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn malformed_trailing_line_is_skipped_not_fatal() {
        let dir = tempdir();
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(CORRECTIONS_FILE);
        let r1 = rec(1, "a", "b", "c");
        std::fs::write(
            &path,
            format!("{}\n{{not valid json\n", serde_json::to_string(&r1).unwrap()),
        )
        .unwrap();
        let loaded = load_corrections(&dir);
        assert_eq!(loaded, vec![r1]);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn clear_corrections_removes_both_files() {
        let dir = tempdir();
        append_correction(&dir, &rec(1, "a", "b", "c")).unwrap();
        save_vocab_map(&dir, &HashMap::from([("a".to_string(), "b".to_string())])).unwrap();
        clear_corrections(&dir).unwrap();
        assert!(load_corrections(&dir).is_empty());
        assert!(load_vocab_map(&dir).is_empty());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn clear_corrections_on_empty_dir_is_not_an_error() {
        let dir = tempdir();
        assert!(clear_corrections(&dir).is_ok());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn select_few_shot_prefers_relevant_and_recent() {
        let corrections = vec![
            rec(1, "old unrelated text about weather", "Old unrelated text about weather.", "Old text about the weather."),
            rec(2, "call fransisco about the report", "Call Fransisco about the report.", "Call Francisco about the report."),
        ];
        let picked = select_few_shot("call fransisco tomorrow", &corrections, 1);
        assert_eq!(picked.len(), 1);
        assert_eq!(picked[0].id, 2);
    }

    #[test]
    fn select_few_shot_respects_k() {
        let corrections = vec![rec(1, "a", "b", "c"), rec(2, "d", "e", "f"), rec(3, "g", "h", "i")];
        assert_eq!(select_few_shot("a", &corrections, 2).len(), 2);
        assert_eq!(select_few_shot("a", &corrections, 0).len(), 0);
    }

    #[test]
    fn select_few_shot_on_empty_store_is_empty() {
        assert!(select_few_shot("anything", &[], 4).is_empty());
    }

    #[test]
    fn vocab_promotes_capitalized_span_after_one_occurrence() {
        let corrections = vec![rec(1, "call fransisco", "Call Fransisco.", "Call Francisco.")];
        let vocab = build_vocab_map(&corrections);
        assert_eq!(vocab.get("Fransisco"), Some(&"Francisco".to_string()));
    }

    #[test]
    fn vocab_requires_two_occurrences_for_lowercase_span() {
        let corrections = vec![rec(1, "a", "meet at the offside", "meet at the offsite")];
        let vocab = build_vocab_map(&corrections);
        assert!(vocab.is_empty(), "single lowercase correction should not promote yet");

        let corrections = vec![
            rec(1, "a", "meet at the offside", "meet at the offsite"),
            rec(2, "b", "back at the offside tomorrow", "back at the offsite tomorrow"),
        ];
        let vocab = build_vocab_map(&corrections);
        assert_eq!(vocab.get("offside"), Some(&"offsite".to_string()));
    }

    #[test]
    fn vocab_ignores_full_sentence_rewrites() {
        // A tone-restyle-shaped correction with no small stable substring —
        // should not mine a bogus multi-word "vocabulary" entry.
        let corrections = vec![rec(
            1,
            "a",
            "I think we should ship this on Monday.",
            "Let's absolutely ship this thing Monday, no excuses!",
        )];
        let vocab = build_vocab_map(&corrections);
        assert!(vocab.is_empty());
    }

    #[test]
    fn apply_vocabulary_replaces_whole_words_case_insensitively() {
        let vocab = HashMap::from([("fransisco".to_string(), "Francisco".to_string())]);
        assert_eq!(apply_vocabulary("call Fransisco now", &vocab), "call Francisco now");
        assert_eq!(apply_vocabulary("FRANSISCO is here", &vocab), "Francisco is here");
    }

    #[test]
    fn apply_vocabulary_does_not_touch_substrings() {
        let vocab = HashMap::from([("art".to_string(), "heart".to_string())]);
        assert_eq!(apply_vocabulary("the smart start", &vocab), "the smart start");
        assert_eq!(apply_vocabulary("modern art today", &vocab), "modern heart today");
    }

    #[test]
    fn apply_vocabulary_with_empty_map_is_identity() {
        assert_eq!(apply_vocabulary("unchanged text", &HashMap::new()), "unchanged text");
    }

    /// A unique-per-call temp dir under the OS temp root — avoids clobbering
    /// between tests running in parallel, without adding a tempfile dep.
    fn tempdir() -> std::path::PathBuf {
        let mut dir = std::env::temp_dir();
        let unique = format!(
            "suzune-personalization-test-{}-{:?}",
            std::process::id(),
            std::time::Instant::now()
        );
        dir.push(unique);
        dir
    }
}
