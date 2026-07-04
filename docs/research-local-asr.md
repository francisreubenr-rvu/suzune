# Local ASR Stack Research — fude (M1 Pro, 16GB)

Date: 2026-07-03
Scope: reference-implementation study (Handy) + web research on local ASR/VAD/LLM options for a Wispr Flow alternative on Apple Silicon.

---

## Part A — Handy reference implementation

Repo: `/Volumes/1TB SSD/brain/repos/Handy/` (Tauri 2 + Rust backend, React/TS frontend). All line refs below are `src-tauri/src/...` unless noted.

### A1. Audio capture, resampling, VAD wiring

| Stage | File:lines | What it does |
|---|---|---|
| Capture | `audio_toolkit/audio/recorder.rs:41-222` | `AudioRecorder` wraps `cpal`. Opens the **device's native/default sample rate** (not forced to 16kHz) — comment at `recorder.rs:285-291` explains this avoids breaking Bluetooth codecs / some ALSA drivers. Worker thread owns the `cpal::Stream`; commands (`Start`/`Stop`/`Shutdown`) flow via `mpsc` channel (`recorder.rs:22-26`). |
| Format handling | `recorder.rs:224-280` | `build_stream<T>` is generic over `cpal::Sample` types (U8/I8/I16/I32/F32); downmixes multi-channel to mono by averaging channels per frame (`recorder.rs:250-264`). |
| Resampling | `audio_toolkit/audio/resampler.rs:1-99` | `FrameResampler` wraps `rubato::FftFixedIn` to resample from device rate to `WHISPER_SAMPLE_RATE = 16000` (`audio_toolkit/constants.rs:1`), fixed chunk size 1024 samples, emits fixed-duration frames (30ms) for VAD consumption. |
| VAD | `audio_toolkit/vad/silero.rs:1-52` | `SileroVad` wraps the `vad-rs` crate (Silero ONNX model). Requires exact 30ms/16kHz frames (`SILERO_FRAME_SAMPLES`, computed from `WHISPER_SAMPLE_RATE`). Threshold-gated: `prob > threshold` → `VadFrame::Speech`, else `VadFrame::Noise` (`silero.rs:41-50`). Model file downloaded separately: `silero_vad_v4.onnx` (see `AGENTS.md` build step) — Handy ships **v4**, not v5. |
| Pipeline glue | `recorder.rs:395-529` (`run_consumer`) | Runs on the audio worker thread: raw samples → spectrum visualizer (for UI level meter) → `FrameResampler.push()` → per-frame VAD gate → accumulate into `processed_samples`. On `Stop`, drains remaining samples until an `EndOfStream` sentinel confirms the cpal callback has gone silent (`recorder.rs:493-510`), guaranteeing no dropped tail audio. |

Design notes: resampling happens *after* capture at native rate rather than forcing cpal to open at 16kHz — a defensive choice against exotic/Bluetooth devices. VAD is applied only while `recording == true`; frames classified as `Noise` are simply dropped from the buffer sent to the ASR engine (not gated in real time against a separate wake window — it's a post-capture filter, not a turn-detector).

### A2. Transcription backends

File: `managers/transcription.rs`, `managers/model.rs`.

Handy supports **8 engine types** via the `transcribe-rs` crate (whisper-cpp + onnx feature flags, `Cargo.toml:59`): Whisper, Parakeet, Moonshine, MoonshineStreaming, SenseVoice, GigaAM, Canary, Cohere (`transcription.rs:17-29`, `LoadedEngine` enum at `39-48`).

**Whisper (whisper.cpp via whisper-rs/transcribe-rs)**
- Loaded via `WhisperEngine::load()` or `load_with_params()` for explicit GPU device selection (`transcription.rs:322-343`).
- Models are pre-quantized GGML files hosted by the project: Small (465MB, fp16-ish baseline), Medium **q4_1** (469MB — 4-bit quantized), Turbo (large-v3-turbo, 1549MB), Large (**q5_0**, 1031MB) — see `managers/model.rs:126-261`. Quantization choice is baked into the downloaded file, not user-configurable at load time.
- GPU/Metal acceleration is controlled by a global accelerator setting (`WhisperAcceleratorSetting::Auto/Cpu/Gpu`) applied via `transcribe_rs::accel` atomics at `transcription.rs:778-809`. `describe_compute_devices()` (`812-826`) enumerates GPUs (Metal on macOS) for manual device selection; index 0 is always CPU.
- x86 FMA3 CPUs are special-cased to skip GPU enumeration entirely to avoid a SIGILL crash in ggml's Vulkan backend (`transcription.rs:888-894`) — not relevant to Apple Silicon but shows defensive engineering around ggml quirks.

**Parakeet (NVIDIA TDT, via transcribe-rs onnx feature)**
- Loaded with `ParakeetModel::load(&model_path, &Quantization::Int8)` — **hardcoded Int8** (`transcription.rs:344-353`). All ONNX-backed engines (Parakeet, Moonshine, SenseVoice, GigaAM, Canary, Cohere) load Int8 by default except plain Moonshine which uses `Quantization::default()`.
- Two variants shipped: `parakeet-tdt-0.6b-v2` (English-only, 451MB, accuracy_score 0.85, marked fastest for English) and `parakeet-tdt-0.6b-v3` (25 EU languages, 456MB, marked `is_recommended: true` — Handy's *default* recommendation, `model.rs:264-326`).
- GPU acceleration for ONNX models goes through `OrtAccelerator` (Auto/CpuOnly/Cuda/DirectMl/Rocm) — **notably no explicit Metal/CoreML EP option is exposed in the settings enum** (`settings.rs` `OrtAcceleratorSetting`), meaning Parakeet/ONNX models on macOS likely run CPU-only via ONNX Runtime's default EP unless transcribe-rs internally wires CoreML — this is a gap worth verifying upstream before assuming GPU accel for Parakeet on Mac.

**Text injection is downstream of transcription** — see A3.

**Model lifecycle (`managers/model.rs`, `managers/transcription.rs`)**
- Idle-unload watcher thread (`transcription.rs:92-161`) checks every 10s and unloads the model after a configurable idle timeout (or immediately after each transcription if `ModelUnloadTimeout::Immediately`), freeing RAM between dictation bursts — relevant for 16GB machines.
- Downloads are resumable (HTTP Range requests), SHA256-verified, and directory-based models (Parakeet/Moonshine/SenseVoice/etc.) are shipped as `.tar.gz` extracted atomically via a temp-dir-then-rename pattern (`model.rs:987-1325`).
- Engine calls are wrapped in `catch_unwind` (`transcription.rs:566-722`) so an engine panic unloads the model instead of poisoning the mutex and hanging the app — solid error-handling pattern worth copying.

### A3. Text injection on macOS

File: `clipboard.rs`, `input.rs`. Both are cross-platform; macOS-relevant paths only.

- Injection is **not** rdev-based despite `rdev` being a Cargo dependency (`Cargo.toml:44`) — grep confirms `rdev` is unused in `src-tauri/src/*.rs`; it's likely a transitive/vestigial dependency (shortcut handling actually uses `tauri-plugin-global-shortcut` + `handy-keys`, see A4). Text injection is done with **`enigo`** (`input.rs:1-124`).
- Two paste strategies, user-selectable (`PasteMethod` enum, `clipboard.rs:591-663`):
  1. **Clipboard-and-keystroke** (default-ish, `PasteMethod::CtrlV`/`CtrlShiftV`/`ShiftInsert`): save current clipboard → write transcript to clipboard via `tauri_plugin_clipboard_manager` → sleep `paste_delay_ms` → send Cmd+V via `enigo` using a hardcoded virtual keycode (`Key::Other(9)` for macOS V, `input.rs:31`) so it works across keyboard layouts (Cyrillic, AZERTY, Dvorak) → sleep 50ms → restore original clipboard (`clipboard.rs:16-79`).
  2. **Direct typing** (`PasteMethod::Direct`): `enigo.text(text)` (`input.rs:117-123`) — simulates keystrokes/uses system input injection directly, no clipboard round-trip, but slower for long text and more prone to dropped characters in some apps.
- Optional auto-submit: sends Return/Ctrl+Return/Cmd+Return after paste if configured (`clipboard.rs:544-585`).
- **Shortcomings observed**: (1) clipboard round-trip is a well-known source of race conditions and "your clipboard got clobbered" bugs — mitigated here by save/restore but there's an inherent window where the system clipboard briefly contains the dictated text, visible to any other app polling the clipboard; (2) `enigo` requires macOS Accessibility permission, and the code has no macOS-specific fallback if that permission is revoked mid-session beyond generic error propagation; (3) hardcoded macOS V keycode (`Other(9)`) is a brittle magic number tied to the US ANSI physical key location, not the current layout's V character — this is *intentional* (layout-independent) but means it silently assumes a standard-ish layout mapping still routes physical key 9 to paste. Linux gets much richer native-tool fallback chains (wtype/dotool/ydotool/xdotool/kwtype); macOS has no equivalent native-tool escape hatch, it's enigo-or-nothing.

### A4. Global shortcuts, push-to-talk vs toggle

Files: `shortcut/handler.rs`, `shortcut/tauri_impl.rs`, `transcription_coordinator.rs`.

- Shortcut backend: `tauri-plugin-global-shortcut` (`tauri_impl.rs:1-199`), with a secondary `handy-keys` crate mentioned in `AGENTS.md`/Cargo.toml for platforms where the Tauri plugin is insufficient.
- All keyboard/signal events funnel through **`TranscriptionCoordinator`**, a single-threaded serializer (`transcription_coordinator.rs:36-185`) that owns a `Stage` state machine (`Idle → Recording → Processing → Idle`). This exists specifically to eliminate race conditions between keyboard shortcuts, OS signals (CLI remote-control flags), and the async transcribe-paste pipeline — a good pattern to replicate; naive apps that let shortcut callbacks directly mutate recording state race under rapid key-repeat or overlapping triggers.
- **Push-to-talk**: hold-to-record — press starts recording only if `Stage::Idle`, release stops only if currently `Recording` with matching binding (`transcription_coordinator.rs:72-79`).
- **Toggle mode**: press once to start, press again (same binding) to stop; a press on a *different* binding while busy is ignored (`80-92`).
- 30ms debounce on presses to absorb OS key-repeat/double-tap (`transcription_coordinator.rs:10, 61-70`).
- Separate "cancel" binding is dynamically registered only while recording is active (`tauri_impl.rs:157-198`) — reduces the chance of shortcut collisions when not recording.

### A5. AI post-processing / cleanup layer

Files: `llm_client.rs`, `apple_intelligence.rs`, `settings.rs:520-613`.

- This is the closest analogue to Wispr Flow's "AI edits" layer. It's implemented as an **OpenAI-chat-completions-compatible client** (`llm_client.rs`) that can point at: OpenAI, Z.AI, OpenRouter, Anthropic, Groq, Cerebras, AWS Bedrock (Mantle), or a **user-supplied custom endpoint defaulting to `http://localhost:11434/v1`** (Ollama's default port) — `settings.rs:524-613`. So local LLM cleanup is supported, but only by pointing the generic HTTP client at a locally-running Ollama (or any OpenAI-compatible local server) — Handy does not embed an LLM runtime itself.
- **Apple Intelligence integration** (macOS ARM64 only): `apple_intelligence.rs` is a thin FFI wrapper (`extern "C"`) around Swift code (`src-tauri/swift/`, not read in this pass) calling `SystemLanguageModel` via `process_text_with_system_prompt_apple`. This is registered as a first-class provider (`APPLE_INTELLIGENCE_PROVIDER_ID`, `settings.rs:576-590`) alongside the cloud providers, and is the one **fully local, on-device, zero-setup** cleanup path Handy ships for Mac users on supported hardware/OS (requires Apple Intelligence enabled, macOS 15.1+ roughly). Availability is checked lazily at use-time rather than at startup, to dodge a SIGABRT crash seen on macOS 26 betas when probing `SystemLanguageModel.default` too early (`settings.rs:576-580`).
- No bundled/embedded local LLM runtime (no MLX or llama.cpp linked into the Rust binary) — this is a **gap**: every non-Apple-Intelligence path requires either a cloud API key or a separately-running local server the user must set up themselves (Ollama, LM Studio, etc.).

### A6. Overall architecture assessment

**Strengths**
- Clean manager/coordinator separation; `TranscriptionCoordinator`'s single-thread state machine is a genuinely good pattern for shortcut-race-condition avoidance.
- Idle-unload watcher + immediate-unload option is well suited to 16GB machines — keeps a ~500MB-1.5GB model resident only while needed.
- Resumable, checksum-verified model downloads; atomic tar.gz extraction.
- `catch_unwind` around engine calls prevents a single bad inference from poisoning shared state and hanging the app.
- Broad engine support (8 backends) gives real accuracy/speed/language tradeoffs to the user rather than betting on one model.
- Native-tool fallback chains on Linux (wtype/ydotool/xdotool/etc.) show attention to platform quirks — macOS injection is comparatively thin (enigo only).

**Weaknesses / gaps relevant to building "fude"**
- No embedded local LLM for cleanup — relies on external Ollama or cloud APIs except for Apple Intelligence (Mac-only, requires specific OS/hardware support, quality/latency characteristics not user-controllable).
- ONNX/Parakeet path has no explicit CoreML/Metal execution provider surfaced in settings — likely CPU-bound on Mac despite the model itself being fast; unverified without runtime testing.
- No streaming/partial-transcript UX for Whisper/Parakeet (only the dedicated Moonshine "Streaming" variant does incremental decode) — meaning perceived latency for the dominant engines is capture-stop → full-buffer transcribe → paste, not live partial captions.
- macOS text injection has a single fallback path (enigo); if Accessibility permission issues or app-specific paste quirks arise there's no secondary strategy the way Linux has 5 native tools to try.
- Clipboard-based paste inherently exposes the dictated text to any concurrent clipboard-watching app for a short window, despite save/restore.
- VAD ships Silero **v4**, not v5 (which is reported ~3x faster in TorchScript / ~10% faster ONNX, see Part B) — an easy upgrade lever.

---

## Part B — Local ASR landscape for Apple Silicon (web research, July 2026)

### B1. Engine comparison

| Engine | Approach | Accuracy (WER, English unless noted) | Speed on M-series (RTF / real-time multiple) | Memory | Streaming | License | Language coverage |
|---|---|---|---|---|---|---|---|
| whisper.cpp (Metal) | C/C++ port, GGML quantized weights, Metal GPU backend | Depends on model size; large-v3 ~2-3% on clean English (widely cited, not independently reproduced here) | Tiny: RTF ≈0.02 (~50x RT) on M2 w/ Metal; Small: RTF ≈0.08 (30s clip in 2.4s); large-v3 ~2-3x RT on M3/M4 with Metal; Metal gives ~30-60% speedup over CPU-only, gap widens with model size ([getspeakup.app](https://getspeakup.app/blog/whisper-cpp-benchmark-mac/), [voicci.com](https://www.voicci.com/blog/apple-silicon-whisper-performance.html)) | Scales with model: ~75MB (tiny) to ~3GB (large-v3 fp16) / less if quantized | No native streaming (chunked); can be adapted with sliding windows | MIT | 99 languages (Whisper multilingual) |
| WhisperKit (CoreML, Argmax) | CoreML conversion of Whisper, targets ANE + GPU | 2.2% WER reported vs cloud systems in one benchmark; large-v3-turbo comparison shows Qwen3-ASR MLX beating it slightly (1.52% vs 1.71%) ([arxiv 2507.10860](https://arxiv.org/html/2507.10860v1)) | 0.46s latency figure cited for lowest-latency config; ANE-accelerated forks squeeze 1.3-1.8x more than raw Metal on M3/M4; M5 Pro large-v3 ≈10x RT | Similar to whisper.cpp for equivalent model size | Designed for real-time/streaming use cases (Argmax markets it as "on-device real-time ASR") | MIT (WhisperKit itself); Whisper weights MIT | Same as Whisper (99 langs) |
| mlx-whisper | MLX-native Whisper port (Python), Metal via MLX | Standard Whisper WER for the underlying checkpoint (not independently changed) | M1 Pro: 18.7s for a clip = 29.7x realtime reported by one benchmark; "close to CUDA" per multiple sources; **lightning-whisper-mlx** claims 4x faster than mlx-whisper baseline ([owehrens.com](https://owehrens.com/whisper-nvidia-rtx-4090-vs-m1pro-with-mlx/), [github mustafaaljadery/lightning-whisper-mlx](https://github.com/mustafaaljadery/lightning-whisper-mlx)) | Same as Whisper model size, MLX lazy-loads/unifies memory with system RAM | No | MIT | 99 languages |
| faster-whisper (CTranslate2) | CTranslate2 optimized inference | Same underlying Whisper WER | **CPU-only on macOS** — confirmed no MPS/Metal support; CTranslate2 uses Apple Accelerate on ARM64 CPU backend, no GPU path ([SYSTRAN/faster-whisper#911](https://github.com/SYSTRAN/faster-whisper/issues/911), [SYSTRAN/faster-whisper#515](https://github.com/SYSTRAN/faster-whisper/issues/515)) | CPU RAM only | No | MIT | 99 languages |
| NVIDIA Parakeet TDT (v2/v3, via transcribe-rs/onnx or MLX port) | FastConformer-TDT, ONNX or MLX | v2 (English): 6.05% WER, RTFx 3386 on Hugging Face Open ASR Leaderboard (datacenter GPU number, not Mac-specific); v3 (25 EU langs): 6.34% avg WER ([NVIDIA Technical Blog](https://developer.nvidia.com/blog/nvidia-speech-and-translation-ai-models-set-records-for-speed-and-accuracy/), [HF nvidia/parakeet-tdt-0.6b-v2](https://huggingface.co/nvidia/parakeet-tdt-0.6b-v2)) | MLX port: 1h08m audio in 62s on Apple Silicon (~66x RT); parakeet-rs (ONNX) reports CPU alone faster than Whisper+Metal on M3 16GB in one comparison ([github altunenes/parakeet-rs](https://github.com/altunenes/parakeet-rs)) | 0.6B params, ~450MB int8 quantized (per Handy's shipped model sizes) | Not natively streaming (TDT is offline/chunked by design in most ports) | CC-BY-4.0 (NVIDIA NGC models) — verify per-checkpoint license before commercial use | v2: English only; v3: 25 EU languages + auto-detect |
| Moonshine (useful-sensors) / Moonshine v2 streaming | Small transformer, sliding-window streaming encoder in v2 | Medium Streaming: 6.65% WER, better than Whisper large-v3 per project's own claim ([arxiv 2602.12241](https://arxiv.org/abs/2602.12241)) | Up to 5x faster than Whisper; sub-200ms latency on constrained hardware (Raspberry Pi cited); processes only the audio given (no zero-padding waste) | Small (tens of MB — Handy's Moonshine models are 31-192MB) | **Yes** — v2 purpose-built for low-latency streaming via sliding-window attention, caches encoder/decoder state incrementally | Apache 2.0-ish (useful-sensors OSS; verify exact license per release) | Primarily English (per Handy's model list; check upstream for multilingual variants) |
| Vosk | Kaldi-based, offline, small footprint | Highly variable: 5.2% WER (American English, custom model) up to 12.3% (Australian English dialect) in one study; general accuracy trails Whisper-family models significantly without domain-specific tuning ([videosdk.live](https://www.videosdk.live/developer-hub/stt/vosk-speech-recognition)) | Lightweight/CPU-friendly, but not benchmarked against Whisper/Parakeet on Apple Silicon in sources found — unknown RTF on M1 Pro | Small models ~50MB, larger ~1-2GB | Yes — Vosk has native streaming API (chunk-based partial results), one of its main selling points | Apache 2.0 | 20+ languages, quality varies a lot by language pack |
| Apple SFSpeechRecognizer (on-device) | Apple's built-in on-device ASR (Speech framework) | "Good" per Apple, but explicitly **no continuous learning on-device** (server variant improves over time, on-device does not) — no independently published WER found | Not benchmarked in sources found; presumably fast (native, ANE-optimized) but unknown RTF | Minimal — system-managed, no extra download | On-device buffer-based recognition has an undocumented, unreliable ~1-minute practical limit; not a good fit for longer dictation | Proprietary (Apple, free to use as system API) | ~10 languages for on-device mode (per search finding) — a real limitation vs Whisper's 99 |

Unknowns (no reliable published number found — do not assume): exact WER for whisper.cpp large-v3-turbo/small/medium in a controlled independent benchmark; Parakeet ONNX-vs-MLX WER delta on Apple Silicon specifically; Vosk RTF on M-series; SFSpeechRecognizer WER at all.

### B2. Streaming vs chunked tradeoffs for dictation latency

- **Chunked (record-until-stop, then transcribe)** — what Handy does for Whisper/Parakeet/SenseVoice/GigaAM/Canary/Cohere: simplest to implement, gives the ASR model full utterance context (generally *better* accuracy than streaming for the same model family), but user-perceived latency = full transcribe time after they stop talking. For a Small/Medium Whisper model on M1 Pro with Metal this is sub-second to a few seconds for typical dictation-length utterances (10-30s) based on the RTF figures above — acceptable for a dictation tool, not acceptable for live captioning.
- **Streaming (incremental decode)** — what Moonshine v2 and MoonshineStreaming (Handy's implementation) do: sliding-window/cached-state encoders emit partial results as audio arrives, sub-200ms TTFT reported. Better perceived responsiveness, typically a small accuracy cost vs full-context chunked decoding, and meaningfully more implementation complexity (managing partial-result UI, revision/backtracking when the streaming decoder corrects itself).
- For a **dictation** use case specifically (not live captioning), chunked transcription triggered by VAD-based endpointing (silence detection ends the utterance) is usually the better latency/complexity tradeoff — this is what Handy does, and what Wispr Flow's underlying architecture likely resembles minus the cloud round-trip.
- **VAD choice**: Silero VAD v5 is reported 3x faster (TorchScript) / ~10% faster (ONNX) than v4, processes ~30ms chunks in <1ms CPU, now supports 6000+ languages, at the cost of 2x model size (2MB vs 1MB) — trivial cost, clear upgrade over Handy's shipped v4 ([snakers4/silero-vad discussion #471](https://github.com/snakers4/silero-vad/discussions/471)). No head-to-head accuracy numbers (precision/recall) between v4 and v5 were found in this pass — treat "more accurate" as unverified, speed gain is verified.

### B3. Local LLM options for the "AI edits" cleanup layer (M1 Pro, 16GB)

Goal: filler-word removal, casing/punctuation cleanup, light reformatting — a small, fast, low-context-window task, not general chat.

| Option | Notes |
|---|---|
| MLX (Apple's framework) | 15-25% faster than llama.cpp on Apple Silicon per one comparison due to native Metal optimization; gap widens on higher-tier chips, negligible on base M1/M2 tier ([groundy.com](https://groundy.com/articles/mlx-vs-llamacpp-on-apple-silicon-which-runtime-to-use-for-local-llm-inference/)) |
| llama.cpp (Metal backend) | Competitive with MLX for decode/token-generation on M1/M2-class chips; broadest GGUF model compatibility (Llama, Mistral, Qwen, Phi, Gemma, etc.); Q4_K_M quantization is the commonly recommended sweet spot for quality/size on constrained RAM |
| Ollama | Wraps llama.cpp; simplest setup/ecosystem, what Handy's "custom" post-process provider defaults to (`localhost:11434/v1`) — pragmatic default if not embedding a runtime directly |
| Apple Intelligence (`SystemLanguageModel`, on-device Apple Foundation Model) | Zero extra download, zero extra RAM budget beyond the OS's own allocation, fully local — but macOS-version- and Apple-Intelligence-enablement-gated, and Handy's own code treats even *probing availability* as crash-risky on beta OS builds; not usable for text cleanup on a headless/dev machine without Apple Intelligence turned on |
| Model size guidance | For pure text-cleanup (not reasoning), a small instruction-tuned model (1-4B params, Q4 quantized) should comfortably fit 16GB unified memory alongside a loaded ASR model (Whisper Small ≈500MB-1GB, Parakeet ≈450MB) with headroom to spare; no specific benchmarked "best" model for filler-word removal was found in this pass — this is a task better solved with a short deterministic prompt on a small general model (e.g., Qwen or Llama 3.2 class, 1-3B) than a specialized fine-tune, since no dedicated filler-removal model surfaced in research |

Practical recommendation path: mirror Handy's approach (OpenAI-compatible HTTP client that can point at localhost) but default-bundle a small MLX or llama.cpp runtime with a 1-3B instruct model rather than requiring the user to separately install Ollama — this closes Handy's biggest cleanup-layer gap (A5) while staying within the 16GB budget.

---

## Recommended stack summary (see final chat message)

Sources consulted (Part B): getspeakup.app, voicci.com, promptquorum.com, justvoice.ai, arxiv.org/2507.10860 (WhisperKit), soniqo.audio/benchmarks, owehrens.com, github.com/mustafaaljadery/lightning-whisper-mlx, github.com/SYSTRAN/faster-whisper (issues #515, #911, #1086, #1401), developer.nvidia.com blog, huggingface.co/nvidia/parakeet-tdt-0.6b-v2 and v3, github.com/altunenes/parakeet-rs, arxiv.org/2602.12241 (Moonshine v2), videosdk.live (Vosk), developer.apple.com (SFSpeechRecognizer docs + forums), github.com/snakers4/silero-vad (discussion #471, releases), groundy.com (MLX vs llama.cpp), weesperneonflow.ai / tldv.io / getvoibe.com (Wispr Flow architecture).
