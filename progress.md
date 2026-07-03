# whispr — Progress

Local-first, open-source alternative to Wispr Flow. Multi-session project; this file plus tasks.json plus git log is the resume state. On "resume tasks": read this file, tasks.json, and `git log --oneline -20` before doing anything.

## Phase status

| Phase | Status | Output |
|---|---|---|
| 1. Research (Wispr Flow teardown, complaints, local ASR stacks) | DONE 2026-07-03 — 3 reports + docs/findings.md | docs/ |
| 2. Planning | DONE 2026-07-03 — AWAITING USER APPROVAL of implementation_plan.md (decisions D1-D5) | implementation_plan.md |
| 3. Implementation | not started — blocked on user approval of plan | app code in this repo |
| 4. Landing page | not started | separate directory/branch |
| 5. Demo video + LinkedIn post | not started | — |

## Key context for future sessions

- Local reference repos (read `brain/repos/index.md`): `repos/Handy` — offline dictation, Tauri + Rust, whisper-rs + Parakeet (transcribe-rs), Silero VAD (vad-rs), cpal audio, rdev shortcuts. `repos/voicebox` — MLX GPU acceleration on Apple Silicon, TS/Python/Rust. Study Handy before architecting; it is 90% of the problem already solved.
- Target hardware: MacBook M1 Pro. Local models live at `/Volumes/1TB SSD/LM` (HF_HOME).
- UI: paperback theme — warm paper surfaces, visible grid, serif/literary type, light/dark toggle, no Inter/Roboto/Arial. Floating always-on-top indicator: minimal, smoothly animated.
- Deliverables checklist: (1) working app in raw/whispr pushed to GitHub, (2) 3D animated landing page, (3) demo video, (4) LinkedIn post.
- Safety: confirm before force-push / history rewrite / making repo public. Routine commits fine.
- SSD spawns AppleDouble `._*` files — delete before staging (`find . -name '._*' -delete`).

## Handoff notes

2026-07-03, session 1 (phase 3 underway):
- Plan approved (all recommendations D1-D5 accepted: greenfield, model download on first run, macOS-only v1, no privacy-incident citations in marketing, private repo first).
- M0 DONE: Rust 1.96.1 installed; Tauri 2 + React-TS scaffold builds; cargo target-dir on internal disk (~/.cache/cargo-targets/whispr) because SSD AppleDouble files corrupt tauri build output — keep .cargo/config.toml git-ignored.
- Spikes DONE, see docs/spike-results.md. Binding decisions: Parakeet v2 int8 CPU default (RTF 0.035); model stays resident (17s reload); cleanup LLM = Qwen3-4B-Instruct-2507 Q4_K_M via llama-server (Metal), prompt v2 in spikes/s3-cleanup-bench/bench.py, prompt v3 must add no-code-conversion rule.
- Models on disk at /Volumes/1TB SSD/LM/whispr-models/: parakeet-tdt-0.6b-v2-int8/, ggml-large-v3-turbo.bin, Qwen3-4B-Instruct-2507-Q4_K_M.gguf (+ rejected Qwen3-1.7B, Llama-3.2-3B GGUFs).
- llama.cpp installed via Homebrew (llama-server on PATH).
- M1 IN PROGRESS: workspace crates scaffolded (audio, vad, asr, pipeline-cli). Agent W1 implementing crates/audio + crates/vad; agent W2 implementing crates/asr. After they land: main session reviews, writes pipeline-cli integration (record -> VAD endpoint -> transcribe -> stdout), commits.
