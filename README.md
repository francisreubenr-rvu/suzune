# whispr

Press a key, speak, get clean formatted text in any app — and nothing ever leaves your machine.

whispr is a fully local voice-dictation app for macOS. Hold a global hotkey, talk, release: your speech is transcribed on-device, tidied up by a small local language model (fillers removed, self-corrections applied, punctuation fixed), and typed into whatever app has focus. There is no cloud, no account, no telemetry, and no subscription — the privacy claim is architectural, not a policy promise.

## Why

Cloud dictation tools stream your voice — and in some cases periodic screenshots of your active window — to remote servers, retain data by default, and charge monthly for the privilege. whispr exists to prove the same experience runs locally on Apple Silicon with nothing to verify and nothing to trust: the audio path is auditable source code.

## How it works

```
global hotkey (hold to talk / toggle)
  -> cpal mic capture -> rubato resample to 16kHz mono
  -> Parakeet TDT 0.6B v2 (int8 ONNX, on-device) speech recognition
  -> Qwen2.5-1.5B-Instruct (Q4 GGUF via llama.cpp, on-device) cleanup pass
     fillers, stutters, self-corrections, punctuation - prompt-versioned
  -> injection into the focused app (Accessibility insert, clipboard-paste fallback)
```

Measured on a MacBook M1 Pro (16GB): a 10-second utterance transcribes in ~400ms (RTF 0.035, CPU only) and the cleanup pass adds a ~260ms median — end-to-end from key-release to injected text in roughly one second. Idle footprint is dominated by the resident models (~2.3GB RSS total including the cleanup server).

## Status

Early but functional. macOS only for now (the architecture — Rust + Tauri 2 — is portable, and Linux/Windows are planned). Settings are read from `settings.json` (created on first run in the app config directory); the in-app settings editor is in progress.

| Setting | Default | Meaning |
|---|---|---|
| `shortcut` | `alt+space` | Global dictation hotkey |
| `push_to_talk` | `true` | Hold-to-talk; `false` = press to toggle |
| `cleanup_enabled` | `true` | Local LLM cleanup pass on/off |
| `cleanup_model` | `Qwen2.5-1.5B-Instruct-Q4_K_M.gguf` | GGUF under `models_root` |
| `input_device` | `null` | Pin an exact input-device name (defeats macOS Continuity grabbing the default mic for a nearby iPhone) |
| `models_root` | — | Folder holding the ASR / VAD / LLM model files |

## Building

Requires Rust (stable), Bun, and `llama-server` (`brew install llama.cpp`) for the cleanup pass.

```bash
bun install
bun run tauri build        # .app bundle
bun run tauri build --no-bundle   # bare binary
```

Note: build through the Tauri CLI, not bare `cargo build` — the CLI step embeds the frontend assets.

Model files are downloaded separately into `models_root` (automatic download manager is planned):
Parakeet TDT 0.6B v2 int8 (ONNX), Silero VAD v4 (ONNX), and a cleanup GGUF (Qwen2.5-1.5B-Instruct Q4_K_M by default).

## Permissions

whispr asks for exactly two macOS permissions: Microphone (to hear you) and Accessibility (to type into the focused app). Both stay on your machine — there is no third place for the data to go.

## Acknowledgements

The architecture owes a debt to [Handy](https://github.com/cjpais/Handy) (MIT) — whispr adapts several of its proven patterns (coordinator state machine, multi-engine ASR via transcribe-rs, clipboard save/paste/restore injection) and adds an embedded local LLM cleanup layer, a pinned-microphone capture chain, and a paperback-themed UI.

## License

MIT
