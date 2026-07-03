# whispr — Progress

Local-first, open-source alternative to Wispr Flow. Multi-session project; this file plus tasks.json plus git log is the resume state. On "resume tasks": read this file, tasks.json, and `git log --oneline -20` before doing anything.

## Phase status

| Phase | Status | Output |
|---|---|---|
| 1. Research (Wispr Flow teardown, complaints, local ASR stacks) | IN PROGRESS — 3 Sonnet agents launched 2026-07-03 | docs/findings.md (Opus synthesis) |
| 2. Planning | not started — blocked on phase 1 | implementation_plan.md (requires user approval before phase 3) |
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

(none yet)
