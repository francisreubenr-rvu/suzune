# fude — Progress

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
- Deliverables checklist: (1) working app in raw/fude pushed to GitHub, (2) 3D animated landing page, (3) demo video, (4) LinkedIn post.
- Safety: confirm before force-push / history rewrite / making repo public. Routine commits fine.
- SSD spawns AppleDouble `._*` files — delete before staging (`find . -name '._*' -delete`).

## Handoff notes

2026-07-03, session 1 (phase 3 underway):
- Plan approved (all recommendations D1-D5 accepted: greenfield, model download on first run, macOS-only v1, no privacy-incident citations in marketing, private repo first).
- M0 DONE: Rust 1.96.1 installed; Tauri 2 + React-TS scaffold builds; cargo target-dir on internal disk (~/.cache/cargo-targets/fude) because SSD AppleDouble files corrupt tauri build output — keep .cargo/config.toml git-ignored.
- Spikes DONE, see docs/spike-results.md. Binding decisions: Parakeet v2 int8 CPU default (RTF 0.035); model stays resident (17s reload); cleanup LLM = Qwen3-4B-Instruct-2507 Q4_K_M via llama-server (Metal), prompt v2 in spikes/s3-cleanup-bench/bench.py, prompt v3 must add no-code-conversion rule.
- Models on disk at /Volumes/1TB SSD/LM/fude-models/: parakeet-tdt-0.6b-v2-int8/, ggml-large-v3-turbo.bin, Qwen3-4B-Instruct-2507-Q4_K_M.gguf (+ rejected Qwen3-1.7B, Llama-3.2-3B GGUFs).
- llama.cpp installed via Homebrew (llama-server on PATH).
- M1 DONE (commit 594ba6f): audio/vad/asr crates + pipeline-cli verified end to end on real audio (jfk.wav -> correct transcript, 125ms ASR; live mic opens/times out cleanly). Silero v4 not v5 (vad-rs hardcodes v4 graph — documented in crates/vad). transcribe-rs 0.3.11 fixed the 17s Parakeet load (now ~1s).
- Design note from M1: push-to-talk must transcribe the WHOLE buffer (VAD only trims edges); VAD endpointing (~1s+ trailing) is for hands-free toggle mode only — 700ms cuts at rhetorical pauses.
- M2 DONE (commits d1c9e3c, 8375360): cleanup crate (llama-server manager + client + SYSTEM_PROMPT_V3, real-model tests pass), inject crate (AxInsert -> ClipboardPaste chain, typed errors), coordinator + global shortcut wired (push-to-talk alt+space default, toggle mode, dictation-state events, cancel_dictation command). App boots clean end to end.
- BLOCKED ON USER: macOS Accessibility permission for the terminal (System Settings > Privacy & Security > Accessibility) — needed for the live full-loop test (speak -> text lands in a focused field). Also expect a one-time Microphone prompt.
- NEXT (M3): paperback UI — floating overlay window (always-on-top, non-activating, states: idle/recording waveform/processing shimmer, listens to dictation-state events), settings window (shortcut, engine, cleanup toggle, model manager, history), light/dark. Use frontend-design skill; verify-live at desktop + ~390px. Then M4 polish/README/GitHub push (private), M5 landing page, M6 demo video + LinkedIn post.
- Settings file materializes at ~/Library/Application Support/dev.fude.app/settings.json (models_root defaults to /Volumes/1TB SSD/LM/fude-models on this machine).
- M3 CORE DONE (commit ac8f9de): tray-only app, paperback overlay pill (visually verified), read-only settings page. BUILD RULE: always `bun run tauri build --no-bundle` — plain `cargo build` produces a binary whose webviews load about:blank. Overlay window must stay listed in src-tauri/capabilities/default.json. Cleanup model is now Qwen2.5-1.5B (e96fce9) after user RAM feedback; mic fallback chain added.
- Session 2026-07-04 side-note: a SEPARATE Swift/WhisperKit fude implementation exists at /Users/maverick/codex/fude (different Claude session, bundle id com.fude.Fude). Do not confuse the two; watch for global-shortcut and TCC permission crossfire when both run.
- 2026-07-04 session 2: codex Swift project deleted per user. Terminal-dictation complaint root-caused: injection into terminal PROVEN working (inject_demo lands in prompt); real culprit was macOS Continuity re-grabbing the default mic for the iPhone -> silence -> silent idle. Fixed: settings.input_device pin (user's set to "MacBook Pro Microphone"), empty transcript now shows an error pill. Committed 3938eec.
- M4 DONE (55c151b): paperback app icon, README (measured claims only), Info.plist (mic usage + LSUIElement), fude.app + dmg bundles, pushed PRIVATE to https://github.com/francisreubenr-rvu/fude. AppleDouble files also corrupt .git/objects/pack — run find .git -name '._*' -delete if git errors appear.
- M3 remaining (deferred): settings editor UI, waveform level metering, engine-load stagger. Bundled .app has its own TCC identity — user currently runs the bare binary (inherits terminal permissions).
- M5 IN PROGRESS: agent W5 building landing/index.html (single self-contained file, paperback theme, CSS-3D book hero, scroll transitions). Main session must visually verify (desktop + 390px) and iterate before calling it done.
- M6 after M5: demo video + LinkedIn post (professional-personal tone, no unverified competitor claims per D4).
