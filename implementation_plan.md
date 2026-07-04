# fude — Implementation Plan

Status: **DRAFT — awaiting user approval. No implementation until approved.**
Basis: `docs/findings.md` (phase 1 synthesis). Target: MacBook M1 Pro, 16GB, fully local.

---

## 1. Product definition

One sentence: **press a key, speak, get clean formatted text in any app — and nothing ever leaves your machine.**

fude = Handy's local-first architecture + the embedded local LLM cleanup layer Handy lacks + Wispr-grade formatting UX + a distinctive paperback visual identity. Free, open-source (MIT).

### v1 scope (and nothing more)

| In v1 | Explicitly NOT in v1 |
|---|---|
| Push-to-talk + toggle dictation, global hotkey | Streaming/live partial captions (Moonshine — v2) |
| VAD-endpointed chunked transcription, fully local | Screenshot/on-screen context (Q5 — deferred; privacy liability, unclear payoff) |
| Local LLM cleanup pass (fillers, punctuation, casing, light formatting), on/off toggle | Windows/Linux builds (architecture stays portable; ship macOS first) |
| Clipboard-paste injection with AX-insert attempt first | Custom vocabulary training, per-app tone profiles |
| Model manager (download, verify, idle-unload) | Accounts, sync, telemetry of any kind (never) |
| Paperback UI: settings window + floating overlay indicator, light/dark | Auto-update infrastructure |
| History panel (local SQLite, user-clearable) | iOS/Android |

## 2. Model selection and quantization (16GB budget)

| Role | Model | Quant / format | Size on disk | RAM (loaded) | Why |
|---|---|---|---|---|---|
| ASR default (English) | Parakeet TDT 0.6B v2 | int8 ONNX | ~450MB | ~700MB | 6.05% WER; fast even on CPU; Handy-proven |
| ASR multilingual | Parakeet TDT 0.6B v3 | int8 ONNX | ~450MB | ~700MB | 25 EU languages; Handy's recommended default |
| ASR max coverage (optional download) | Whisper large-v3-turbo | GGML q5_0 via whisper-rs/Metal | ~1.1GB | ~1.5GB | 99 languages; Metal-accelerated |
| VAD | Silero VAD v5 | ONNX | 2MB | negligible | Upgrade over Handy's v4; <1ms per 30ms frame |
| Cleanup LLM | Qwen3-1.7B-Instruct (primary candidate) vs Llama-3.2-3B-Instruct | GGUF Q4_K_M via llama.cpp + Metal | 1.0–1.9GB | 1.5–2.5GB | Deterministic text cleanup, not reasoning; bake-off decides (spike S3) |

Peak concurrent RAM (ASR + LLM + app): **~3.5GB worst case** — comfortable on 16GB. Idle-unload watcher (Handy pattern) drops both models after a configurable idle window; idle footprint target **<250MB RAM, <1% CPU** (vs Wispr's reported ~800MB/8%).

Runtime note: findings.md prefers MLX for raw speed, but MLX is Python-first and poorly embeddable in a Rust/Tauri binary. **Decision: llama.cpp with Metal via a Rust binding** — 15–25% slower than MLX but a single self-contained binary with zero user setup, which is the actual differentiator (Handy's #1 gap is requiring external Ollama). Models stored under `HF_HOME` (`/Volumes/1TB SSD/LM`) during development.

## 3. System architecture

```
global hotkey (rdev/tauri plugin)
  -> TranscriptionCoordinator (single-thread state machine: Idle -> Recording -> Processing -> Injecting -> Idle)
       -> capture: cpal @ device-native rate -> rubato resample -> 16kHz mono 30ms frames
       -> VAD gate: Silero v5 filters silence; silence-timeout endpoints the utterance
       -> ASR: engine trait { parakeet(onnx) | whisper(whisper-rs/Metal) } -> raw transcript
       -> cleanup (optional): embedded llama.cpp server, OpenAI-compat HTTP on localhost
            short deterministic system prompt: fillers, punctuation, casing, paragraphs
            (power users may point the client at Ollama/anything OpenAI-compatible)
       -> inject: try AXUIElement insertText (write-only scope) -> fallback clipboard
            save/write/Cmd+V/restore -> optional direct-type mode
  -> overlay window (always-on-top, non-activating): idle dot -> recording waveform -> processing shimmer
  -> history: local SQLite (transcripts only, opt-out, one-click wipe)
```

Crate layout (Cargo workspace inside Tauri `src-tauri/`):

| Crate/module | Responsibility | Parallel-safe workstream |
|---|---|---|
| `audio` | cpal capture, rubato resample, ring buffer | W1 |
| `vad` | Silero v5 ONNX wrapper, endpointing | W1 |
| `asr` | `Engine` trait + parakeet/whisper backends, model registry | W2 |
| `cleanup` | llama.cpp lifecycle + OpenAI-compat client + prompt | W3 |
| `inject` | AX insert, clipboard paste, direct type (macOS first) | W4 |
| `coordinator` | state machine, hotkeys, debounce, cancel binding, idle-unload | integration (sequential, main session) |
| `ui` (Tauri webview) | settings window, overlay, model manager, history | W5 |

Adopted from Handy verbatim (patterns, not code-paste): coordinator state machine, idle-unload watcher, SHA256-verified resumable model downloads, `catch_unwind` around engine calls, 30ms hotkey debounce.
Done better than Handy: embedded LLM runtime (no Ollata dependency), Silero v5, CoreML/Metal execution-provider verification for Parakeet (spike S1), AX-insert injection fallback, distinctive UI.

## 4. How fude beats Wispr Flow (mapped to findings weakness table)

| Wispr weakness (rank) | fude answer |
|---|---|
| No verifiable local processing (1) | 100% on-device by architecture; open source = auditable; a network-permission-free build is the proof |
| Subscription-only (2) | Free, MIT, no account, no word limits |
| Default-on retention/training (3) | No telemetry, no upload, history local + wipeable |
| ~800MB idle footprint (4) | Idle-unload; target <250MB idle |
| No Linux / on-prem (5) | Portable architecture (Tauri/Rust); macOS first, Linux/Windows in v1.x |
| Reliability gap (6) | Deterministic local pipeline; no server to degrade |
| Accent/multilingual (7) | User-selectable engine incl. Whisper 99-lang; honest: needs benchmarking, not promised |
| Keystroke-read scope (8) | Write-only AX injection where possible; documented permission scope |

## 5. UI — paperback theme

- Warm paper surfaces (cream/ecru), visible fine grid, ink-dark text; serif display (e.g. Newsreader/Source Serif) + mono for transcripts; zero Inter/Roboto/Arial. Considered light/dark ("daylight paper" / "lamplight sepia-dark"), not a default dark route.
- Floating overlay: small non-activating always-on-top pill; states idle (faint dot) -> recording (ink waveform) -> processing (subtle shimmer) -> done (fade). Spring-based animation, no bounce excess. Click-through except a cancel affordance.
- Settings window: book-page layout — margins, hairline rules, retro-tactile buttons. No shadcn-default look; data-driven rendering, no decorative non-functional elements.

## 6. Execution plan

### Spikes first (answer findings Q1–Q3 with measurements, not guesses)

| Spike | Question | Exit criterion |
|---|---|---|
| S1 | Parakeet ONNX with CoreML/Metal EP on M1 Pro — real RTF vs CPU | Measured RTF table; decides default engine config |
| S2 | End-to-end latency budget: 10s utterance -> injected text | Measured ms breakdown (capture flush / ASR / LLM / inject); target <1.5s total, stretch <1s |
| S3 | Cleanup bake-off: Qwen3-1.7B vs Llama-3.2-3B Q4 on 20 real dictation samples | Chosen model + frozen system prompt; zero-hallucination requirement |

### Milestones

| Milestone | Contents | Mode |
|---|---|---|
| M0 | Repo scaffold (Tauri 2 + Rust workspace), CI-less local build, spikes S1–S3 | Sequential, main session |
| M1 | Core pipeline headless: hotkey -> capture -> VAD -> ASR -> stdout | W1+W2 parallel (different crates), integrate sequentially |
| M2 | Injection + coordinator + cleanup layer wired | W3+W4 parallel, then integrate |
| M3 | UI: settings, overlay, model manager, history; paperback theme; visual verify via `verify-live` | W5 (UI) parallel with M2 hardening |
| M4 | Polish: idle-unload, error paths, README, GitHub push (private until user says public) | Sequential |
| M5 | Landing page (separate `landing/` dir, own branch) — 3D/scroll-animated, paperback-consistent | Separate deliverable |
| M6 | Demo video + LinkedIn post | After M4 |

Subagent policy (quota-aware): max 3 concurrent Sonnet agents, only on file-disjoint workstreams (W1–W5); coordinator/integration work stays in the main session. Every agent reads `progress.md` + `tasks.json` before starting; every merge is committed immediately.

### Decisions needed from you (approval gate)

| # | Decision | Recommendation |
|---|---|---|
| D1 | Fork Handy vs greenfield | **Greenfield Tauri app, porting Handy's proven patterns with attribution in README/NOTICE.** Forking inherits 24k-star project's UI/identity and gaps; fude's value is a distinct product. MIT permits pattern/code reuse with attribution. |
| D2 | Cleanup LLM ships bundled or downloaded on first run | **Downloaded on first run via model manager** (keeps repo/app small; same UX as ASR models). |
| D3 | v1 platform scope | **macOS-only v1**, portable architecture. Cross-platform now would triple injection/hotkey work before product validation. |
| D4 | Cite the 2025 privacy incident in marketing/LinkedIn? | **No — single-sourced.** Lead with "architecturally verifiable privacy" instead; no claims about Wispr we can't prove (aligns with zero-fabrication rule). |
| D5 | GitHub repo visibility at first push | **Private** until you flip it (per safety rule). |

---

Approve as-is, or amend any decision (D1–D5) / scope line, and phase 3 begins with M0.
