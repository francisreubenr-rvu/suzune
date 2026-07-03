# Spike Results

Measured on this machine (MacBook M1 Pro, 16GB, macOS 26.5.1). All numbers real measurements from `spikes/` binaries — no borrowed or estimated figures.

## S1 — ASR engine benchmark (2026-07-03)

Binary: `spikes/s1-asr-bench` (transcribe-rs 0.3.8). Audio: whisper.cpp `jfk.wav` (11.0s, real speech, 16kHz mono) and a 3x concatenation (33.0s). 3 warm runs each; run0 shown separately where it differed.

| Engine | Config | 11s audio | 33s audio | RTF | Transcript quality |
|---|---|---|---|---|---|
| Parakeet TDT 0.6B v2 int8 | ONNX, **CPU only** | 359–386ms | 1252–1295ms | 0.033–0.039 | Perfect on sample |
| Whisper large-v3-turbo | whisper.cpp, **Metal** | 1544–2160ms | 2469–2586ms | 0.075–0.196 | Perfect on sample |

Model load time (both cold and warm page cache — dominated by session init, not disk):

| Engine | Load time |
|---|---|
| Parakeet v2 int8 | 17.0–17.3s |
| Whisper large-v3-turbo | 33.8–62.0s |

### Decisions driven by S1

1. **Q1 answered: Parakeet CPU-only is fast enough.** RTF 0.035 without any CoreML/Metal EP work. A 10s utterance transcribes in ~400ms — inside the latency budget with room for the LLM pass. No ONNX execution-provider work in v1.
2. **Plan revision — idle-unload softened.** Reload costs 17s (Parakeet) / 30s+ (Whisper); an unloaded model means a dictation attempt stalls unacceptably. v1 keeps the ASR model resident for the app's lifetime (~700MB for Parakeet is acceptable); idle-unload becomes an opt-in setting with a long default window (>=15 min) and eager reload on hotkey-down.
3. Whisper stays the optional multilingual engine; its 2.5s on 33s audio is usable but its load time makes engine hot-swapping a deliberate user action, not automatic.

## S2 — End-to-end latency budget (updated as components are measured)

| Stage | Measured | Source |
|---|---|---|
| ASR (10s utterance, Parakeet) | ~400ms | S1 |
| LLM cleanup (Qwen3-4B, typical utterance) | 400–760ms (median 578ms) | S3 |
| VAD + capture flush | TBD (expected <50ms) | M1 pipeline |
| Injection (clipboard paste) | TBD | M2 |

Projected end-to-end for a 10s utterance: **~1.0–1.2s** — inside the 1.5s target, stretch 1s within reach when cleanup is disabled (~500ms).

## S3 — Cleanup LLM bake-off (2026-07-03)

Harness: `spikes/s3-cleanup-bench/bench.py` against `llama-server` (Homebrew llama.cpp, Metal, `-ngl 99`), temperature 0, 20 constructed dictation samples (`samples.jsonl` — synthetic test inputs written for this spike, covering fillers, stutters, self-corrections, register preservation, and instruction-like content). Raw outputs in `results-*.jsonl`.

| Model | Prompt | Median | p90 | Max | Quality verdict |
|---|---|---|---|---|---|
| Qwen3-1.7B Q4_K_M (thinking disabled) | v1 | 300ms | 560ms | 655ms | FAIL — echoes input, no cleanup, leaks `</think>` |
| Llama-3.2-3B-Instruct Q4_K_M | v1 | 395ms | 3814ms | 3838ms | FAIL — answered code-related dictation with Python code; dropped content clauses |
| Llama-3.2-3B-Instruct Q4_K_M | v2 (few-shot + never-execute guard + no-omission rule) | 460ms | 1237ms | 3795ms | Good — 18/20; fails #16 (emits code block), #14 meaning flip |
| **Qwen3-4B-Instruct-2507 Q4_K_M** | v2 | **578ms** | **758ms** | **1252ms** | **Best — 19/20 clean; only #16 (converts dictated instruction to code comment)** |

### Decision

**Qwen3-4B-Instruct-2507, Q4_K_M (~2.3GB disk, ~2.5GB loaded)** with prompt v2. Rationale: most consistent quality, no multi-second latency outliers (Llama's max was 3.8s), and Apache-2.0 licensing fits an MIT product better than the Llama community license. Peak concurrent RAM with Parakeet resident: ~3.2GB — within budget.

Known residual defect for M2: sample #16 ("add a todo comment saying...") gets converted into code by every model tried. Prompt v3 must add an explicit "never convert the text into code or another format; clean the words only" rule — validate at integration.

Prompt v2 is versioned in `bench.py` (SYSTEM_PROMPT).
