# suzune — Consolidated Findings

Authoritative synthesis of three research reports for the `suzune` project (fully local, open-source, privacy-respecting alternative to Wispr Flow; target hardware MacBook M1 Pro, 16GB).

Compiled: 2026-07-03. Sources:
- `docs/research-architecture.md` (how Wispr Flow works)
- `docs/research-complaints.md` (weaknesses, complaints, competitive landscape)
- `docs/research-local-asr.md` (Handy source study + local ASR/VAD/LLM landscape)

Citation convention preserved from the source reports:
- **[Wispr-stated]** — Wispr's own marketing/docs; self-reported, not independently verified.
- **[Reviewer/third-party]** — external reviewer/blog; unverified speculation.
- **[UNKNOWN]** — nothing public found.
- **[Judgment]** — reasoned conclusion added in this synthesis, not a sourced claim.

---

## 1. How Wispr Flow works — consolidated technical picture

### 1.1 Pipeline overview

```
Hotkey press -> mic capture -> audio STREAMED to cloud (not persisted locally)
   -> proprietary cloud ASR -> personalization/formatting LLM pass
   -> text returned -> injected into active app (Accessibility API / clipboard paste)
Target: full transcription + LLM formatting within 700ms of speech-stop.
```

### 1.2 Stage-by-stage

| Stage | Finding | Evidence | Confidence |
|---|---|---|---|
| Activation | Hotkey-based, press-to-activate (not always-on). "Flow Sessions" configures mic-access timeout windows. | [Wikipedia](https://en.wikipedia.org/wiki/Wispr_Flow) | Sourced |
| Capture | Audio is **streamed to backend**, **not persisted locally** on client. Buffering/sample-rate/VAD/codec internals **not disclosed**. | [Security FAQ](https://docs.wisprflow.ai/articles/3467817258-security-and-compliance-faq); [Tech post](https://wisprflow.ai/post/technical-challenges) | Sourced / [UNKNOWN] internals |
| Sub-audible speech | Claims to handle subvocalization (quiet speech); no implementation detail. | [Tech post](https://wisprflow.ai/post/technical-challenges) | [Wispr-stated] |
| ASR | **Cloud-only. No on-device ASR for desktop transcription** — "Transcription always happens in the cloud." Described as proprietary/custom ("context aware, personalized, code-switched"); no base model, vendor, or architecture disclosed. Continuously retrained on user feedback. | [Privacy](https://wisprflow.ai/privacy); [Tech post](https://wisprflow.ai/post/technical-challenges); [Blockchain Council](https://www.blockchain-council.org/ai/wispr-flow-explained-real-time-speech-to-text-ai-productivity-workflows/) | [Wispr-stated, unverifiable] |
| ASR subprocessor | Speech processed via **Baseten**; storage on **AWS**. (Architecture report says subprocessor identity is NDA-gated; complaints report names them via eesel AI — see contradiction C4.) | [eesel AI](https://www.eesel.ai/blog/wispr-flow-review) | [Reviewer/third-party] |
| LLM edit pass | Post-ASR "personalized LLMs with token-level formatting control": filler-word removal, mid-utterance self-correction ("5pm, actually 6" -> "6pm"), auto-formatting (lists/paragraphs/emails per destination app), code mode (camelCase/snake_case), tone matching, per-user style adaptation. LLM post-processing routed through **OpenAI, Anthropic, Cerebras**. | [Why Flow](https://wisprflow.ai/why-flow); [Tech post](https://wisprflow.ai/post/technical-challenges); [eesel AI](https://www.eesel.ai/blog/wispr-flow-review) | [Wispr-stated] mechanism; [Reviewer] subprocessors |
| Context awareness | Captures **periodic screenshots** of active window, uploads to backend, "stripped on upload," claimed never persisted server-side. Plus a personal dictionary for uncommon names/terms. Note internal tension: tech post frames app-context detection as device-side "for privacy," but screenshots are demonstrably uploaded. | [Security FAQ](https://docs.wisprflow.ai/articles/3467817258-security-and-compliance-faq); [Tech post](https://wisprflow.ai/post/technical-challenges) | [Wispr-stated] |
| Text injection (macOS) | Primary: **macOS Accessibility API**; **clipboard/paste fallback** for non-supporting apps. Requires Microphone + Accessibility permissions. Wispr has acknowledged the macOS app "can read the device user's keystrokes" — a byproduct of Accessibility-API use. | [VibeWhisper](https://vibewhisper.dev/comparison/wispr-flow-alternative/) [Reviewer]; [Re-verify permissions](https://docs.wisprflow.ai/articles/5510622673-re-verify-wispr-flow-permissions-after-updating); [Wikipedia](https://en.wikipedia.org/wiki/Wispr_Flow) | Mixed (injection path [Reviewer]; keystroke admission Wispr-acknowledged) |
| Text injection (Windows/iOS) | Windows needs **no app-level Accessibility permission** (implies SendInput/UI-Automation; exact mechanism [UNKNOWN]). iOS is a **third-party system keyboard**. | [Wikipedia](https://en.wikipedia.org/wiki/Wispr_Flow) | Sourced / [UNKNOWN] Windows internals |
| Latency | **Stated targets**: <=700ms end-to-end after speech-stop; budget ASR <200ms + LLM <200ms + network <200ms (~100ms slack). Strategy: push bigger models into fixed budget. Scale: ~1B words/month, 99.99% uptime target. No independent measurement exists. | [Tech post](https://wisprflow.ai/post/technical-challenges) | [Wispr-stated] — treat as target, not measured |
| Privacy model | Audio **always** leaves the machine; no offline desktop mode. Default (non-enterprise): audio + transcripts "may be used to evaluate, train, and improve" models; **Privacy Mode is opt-in, not opt-out**. Privacy Mode + Cloud Sync off = Zero Data Retention (default/irrevocable only for Enterprise/HIPAA). No true end-to-end encryption possible (backend must decrypt to transcribe). TLS 1.2+, AES-256 at rest, HSM/FIPS 140-2 key mgmt. | [Privacy](https://wisprflow.ai/privacy); [Security FAQ](https://docs.wisprflow.ai/articles/3467817258-security-and-compliance-faq) | [Wispr-stated] |
| Compliance | See contradiction C1 (SOC2 Type I vs Type II). HIPAA BAA available; ISO 27001:2022 in progress. | [Security FAQ](https://docs.wisprflow.ai/articles/3467817258-security-and-compliance-faq); [ModelPiper](https://modelpiper.com/blog/wispr-flow-privacy-incident) | Conflicting sources |

### 1.3 The 2025 privacy incident (both reports; single upstream source)

Developer **Ryan Shrott** (late 2025) monitored network traffic, found Wispr Flow transmitting audio + periodic active-window screenshots to cloud servers including third-party infrastructure (reported as OpenAI) without clear prior disclosure. Wispr initially **banned the reporting user**; CTO **Sahaj Garg** later publicly apologized for the ban — widely read as implicit confirmation the finding had merit. Post-incident, Wispr added Privacy Mode (ZDR), flipped AI-training to opt-out->opt-in-off-by-default, and pursued SOC2/HIPAA/ISO27001.

**Caveat:** primary narrative traces to a single third-party blog (ModelPiper); the complaints report corroborates the ban/apology via Voibe but the core technical finding is effectively single-sourced. **Recommend independent verification before suzune cites this as established fact.** [ModelPiper](https://modelpiper.com/blog/wispr-flow-privacy-incident); [Voibe](https://www.getvoibe.com/resources/wispr-flow-review/)

### 1.4 Cross-report contradictions (flagged)

| # | Contradiction | Report A | Report B | Resolution |
|---|---|---|---|---|
| C1 | SOC 2 certification level | architecture: **Type I** completed Apr 2026 (per Wispr's own technical Security FAQ); marketing pages say Type II | complaints: post-incident acquired **Type II** (per ModelPiper) | **[Judgment]** Trust the primary Security FAQ (Type I). ModelPiper/marketing likely conflate. This is an inconsistency *within Wispr's own materials* — do not assert Type II. |
| C2 | Android on-device processing | architecture: total cloud dependency, "no on-device transcription mode at all" | complaints: Wispr's Android marketing claims on-device, offline-capable | **Unresolved.** Both reports flag it. Android claim is first-party, unverified, and contradicts desktop architecture + incident findings. Treat as marketing claim, not fact. |
| C3 | Privacy Mode default | architecture: opt-in, off by default for regular users | complaints: post-incident "AI-training opt-in changed to off-by-default" | **[Judgment]** Not a true conflict — training-*use* is now off by default, but full ZDR (Privacy Mode + Cloud Sync off) still requires manual action for non-enterprise users. Both can be true. |
| C4 | Subprocessor disclosure | architecture: third-party LLM provider identity **undisclosed** (NDA annex) | complaints: names Baseten (speech), OpenAI/Anthropic/Cerebras (LLM), AWS (storage) via eesel AI | **[Judgment]** Wispr's *official* position is non-disclosure; the named list is a third-party reviewer's finding. Cite as "[Reviewer] reports X; Wispr does not officially confirm." |

---

## 2. Wispr Flow's weaknesses, ranked by exploitability for suzune

Ranked by how directly a local-first open-source tool can beat each.

| Rank | Weakness | Evidence strength | How suzune beats it |
|---|---|---|---|
| 1 | **No verifiable local/offline desktop processing.** Audio always leaves the machine; even Privacy Mode is a policy promise with no user-facing audit path. | Strong — corroborated across eesel AI, ModelPiper, Voibe + Wispr's own privacy page | 100% on-device ASR + local LLM cleanup. Nothing leaves the machine — architecturally verifiable, not a policy promise. The core differentiator. |
| 2 | **Subscription-only pricing, no lifetime tier.** $15/mo ($12 annual); "most expensive standalone macOS productivity sub"; compounds to $720/5yr. | Strong — Voibe pricing, direct user quotes | Free / open-source (MIT-class). No account, no word limits, no recurring cost — matches Handy's positioning. |
| 3 | **Default-on data retention + training use.** Privacy Mode opt-in; screenshots uploaded; undisclosed/third-party LLM subprocessors. | Strong (Wispr's own FAQ) + incident context | No telemetry, no upload, no training on user data. Optional screenshot-context feature can be fully local or omitted. |
| 4 | **Heavy background footprint.** ~800MB idle RAM / ~8% CPU vs local competitors ~200MB / <2%; called out as impactful on 8GB machines. | Moderate — eesel AI (single detailed source) | Rust/Tauri process + idle-unload of ASR model (Handy pattern). Model resident only during dictation; lean idle state. |
| 5 | **No native Linux; no on-prem enterprise.** Unofficial community Linux port exists (signals unmet demand). Enterprise is cloud-only regardless of MDM. | Strong (official platform list + GitHub port) | Cross-platform from day one (Handy already ships Win/mac/Linux). Local-by-design = trivially self-hostable/on-prem. |
| 6 | **Reliability / trust gap.** Trustpilot 2.7/5 vs App Store 4.8/5; "works 60% after payment"; Windows freezes; forces itself into macOS login items. | Moderate — Voibe/getvoibe, viral Reddit thread | Deterministic local pipeline (no server-side degradation); respect user autostart preference; open code = auditable reliability. |
| 7 | **Weaker non-native / accented, non-English accuracy.** Advantage "most pronounced for native English"; degrades on strong/mixed accents and noise. | Moderate — Wispr's own research post (self-reported admission) | **[Judgment]** Multilingual model choice (Parakeet v3 = 25 EU langs; Whisper = 99) + user-selectable engine per language. Not a guaranteed win — needs benchmarking. |
| 8 | **Accessibility-API keystroke-read scope.** Wispr admitted its macOS app can read keystrokes as an Accessibility byproduct — its own controversy trigger. | Moderate (Wispr-acknowledged via Wikipedia) | **[Judgment]** Be deliberate about Accessibility scope; document exactly what is accessed; write-only injection where feasible. Marketing point, hard to fully eliminate given macOS injection realities. |

---

## 3. Competitive landscape matrix

| Tool | Local / Cloud | Pricing | Platforms | Key gaps / notes |
|---|---|---|---|---|
| **Wispr Flow** | Cloud-only (desktop); Android on-device claimed (unverified) | $15/mo ($12/mo annual); no lifetime | macOS, Windows, iOS, Android | Best UX polish; privacy/trust baggage; no offline desktop; no Linux; heavy footprint |
| **Superwhisper** | 100% local (on-device Whisper) | $249.99 lifetime | macOS (primary) | Reddit's privacy-first pick; Mac-centric; paid |
| **MacWhisper** | 100% local | Not specified in sources | macOS | File/recording transcription, **not** real-time system-wide dictation — different use case |
| **VoiceInk** | Local by default | $39 one-time, or free (GPLv3 OSS) | macOS, iOS | "Strongest overall Mac/iOS alternative"; custom writing modes; open + lifetime |
| **Handy** (`cjpais/Handy`) | 100% local/offline | Free, MIT, no account/limits | Windows, macOS, **Linux** | 23k+ stars; Tauri/Rust; bundles Whisper + Parakeet + Moonshine; **direct architectural precedent for suzune** |
| **Apple dictation** (SFSpeechRecognizer) | Local (recent OS) | Free (bundled) | macOS, iOS | ~10 langs on-device; no continuous on-device learning; ~1-min practical buffer limit; no WER published; weaker/fewer features (widely cited, not sourced this pass) |
| **Talon** | Local (voice-control focus) | Not detailed in sources | macOS, Linux, Windows | Full voice control + eye tracking + Python scripting; targets accessibility/devs; **not** a drop-in dictation replacement |
| **Aqua Voice** (context) | Cloud-only, no offline | $8-10/mo; free tier 1,000 words | macOS, Windows, iPhone | Strong technical/code vocab, 800-entry dictionary; stores transcripts by default |

**Positioning [Judgment]:** suzune's open competitive slot is "Handy's local-first architecture + an *embedded* local LLM cleanup layer that Handy lacks + Wispr-grade formatting UX." No existing tool combines fully-local ASR, fully-local LLM edit pass, cross-platform, and free/OSS in one polished package.

---

## 4. Recommended technical direction for suzune

Decisive recommendations. Conflicts between reports resolved inline.

### 4.1 ASR engine(s)

| Decision | Recommendation | Rationale / source |
|---|---|---|
| Primary engine (English) | **Parakeet TDT 0.6B v2** via ONNX (or MLX port) | 6.05% WER; parakeet-rs reports CPU alone beating Whisper+Metal on M3 16GB; ~450MB int8; Handy's default recommendation. [parakeet-rs](https://github.com/altunenes/parakeet-rs), [HF](https://huggingface.co/nvidia/parakeet-tdt-0.6b-v2) |
| Primary engine (multilingual) | **Parakeet TDT 0.6B v3** (25 EU langs) or **Whisper** for 99-lang coverage | v3 = Handy's `is_recommended`; Whisper large-v3-turbo GGML for widest language reach. [research-local-asr B1] |
| Acceleration | **Verify CoreML/Metal EP for Parakeet ONNX on Mac before committing** | Handy exposes no Metal/CoreML ORT EP -> Parakeet likely CPU-only on Mac in Handy. **[Open question — see section 5.]** If CPU-only is too slow, fall back to WhisperKit (CoreML/ANE) or mlx-whisper. [research-local-asr A2 line 39, B1] |
| Streaming option (later) | **Moonshine v2 streaming** if live-caption UX is wanted | Only engine in Handy's set with true incremental decode; sub-200ms TTFT. Not needed for v1 dictation. |
| Avoid | **faster-whisper (CTranslate2)** on Mac | Confirmed **CPU-only, no Metal** on macOS. Not a fit for Apple Silicon GPU acceleration. [SYSTRAN #911/#515] |

**Conflict resolution (ASR):** The local-ASR report leans Parakeet (Handy's default, fast on CPU) while noting the Mac GPU-acceleration gap; WhisperKit wins the ANE/latency story. **Resolution [Judgment]:** ship **Parakeet v2/v3 as default** for speed + footprint, with **WhisperKit (CoreML/ANE) as the latency-optimized alternative** and Whisper GGML for max language coverage. Make engine user-selectable (Handy's multi-backend philosophy) rather than betting on one.

### 4.2 VAD

- **Silero VAD v5** (ONNX), not v4. Upgrade from Handy's shipped v4. ~3x faster (TorchScript) / ~10% faster (ONNX), 30ms chunks in <1ms CPU, 6000+ languages, at 2x model size (2MB vs 1MB — trivial). Speed gain verified; "more accurate" unverified (no v4-vs-v5 precision/recall numbers found). [silero-vad #471]
- Wire it as a **post-capture filter + silence-based endpointer** (Handy's model): resample device-native rate -> 16kHz 30ms frames -> VAD gate -> accumulate speech frames -> transcribe on endpoint.

### 4.3 Chunked vs streaming

**Decision: chunked (record-until-silence, then transcribe), VAD-endpointed.** All three angles converge here:
- For dictation (not live captioning) chunked gives the model full utterance context = better accuracy, far simpler implementation.
- Handy uses chunked for all non-Moonshine engines; Wispr's underlying architecture "likely resembles" chunked minus the cloud round-trip.
- On M1 Pro with Metal, Small/Medium Whisper transcribes typical 10-30s utterances in sub-second to a few seconds — acceptable for dictation.
- Streaming (Moonshine v2) is a **phase-2 enhancement** for perceived responsiveness, not a v1 requirement; it adds partial-result/backtracking UI complexity.

### 4.4 Local LLM cleanup layer

This is the layer **Handy lacks** (its biggest gap: no embedded runtime; every non-Apple-Intelligence path needs a separately-run Ollama or a cloud key).

| Decision | Recommendation | Rationale |
|---|---|---|
| Runtime | **Embed a local runtime — MLX (preferred on Apple Silicon) or llama.cpp/Metal** | MLX 15-25% faster than llama.cpp on Apple Silicon (widens on higher tiers). Do **not** require the user to install Ollama. [groundy.com] |
| Model | **Small instruction-tuned 1-3B, Q4 quantized** (Qwen / Llama 3.2 class) | Fits 16GB alongside loaded ASR model (~450MB-1GB) with headroom. Task is deterministic text cleanup, not reasoning — a short prompt on a small general model beats hunting a fine-tune (none found). [research-local-asr B3] |
| Interface | **Keep Handy's OpenAI-compatible HTTP client** pointed at the embedded runtime, so power users can still swap in Ollama/cloud | Adopt Handy's `llm_client.rs` pattern; add the bundled runtime as the zero-setup default. [research-local-asr A5] |
| Apple Intelligence | **Optional secondary provider**, not the default | Fully local, zero extra RAM — but OS-version/enablement-gated and Handy treats even *probing* it as crash-risky on beta OS. Not reliable as the sole path. [research-local-asr A5 line 75] |
| Scope | Filler removal, casing/punctuation, light reformatting via a short deterministic system prompt | Matches Wispr's edit-pass behavior; low context window, fast. |

**What to adopt from Handy:** multi-engine ASR abstraction; `TranscriptionCoordinator` single-thread state machine (Idle->Recording->Processing->Idle) for shortcut-race avoidance; idle-unload watcher (critical for 16GB); resumable SHA256-verified atomic model downloads; `catch_unwind` around engine calls; push-to-talk + toggle with 30ms debounce; dynamically-registered cancel binding.

**What to do better than Handy:** (1) **embed the LLM runtime** (Handy's #1 gap); (2) **upgrade Silero v4->v5**; (3) **verify/enable CoreML EP for Parakeet on Mac** (Handy leaves it likely CPU-bound); (4) **strengthen macOS injection fallback** (Handy is enigo-only on Mac vs 5 native tools on Linux).

### 4.5 Text injection strategy

Adopt Handy's dual strategy, harden the Mac path:
- **Default: clipboard-save -> write transcript -> Cmd+V (layout-independent keycode) -> restore clipboard.** Fast, robust across apps. Accept the known small window where dictated text sits in the system clipboard (mitigated by save/restore).
- **Alternative: direct typing** (`enigo.text`) — no clipboard round-trip, but slower/drop-prone for long text.
- **[Judgment] Add a macOS fallback Handy lacks:** attempt Accessibility-API direct insertion (AXUIElement `setValue`/`insertText`) first for supported apps, clipboard-paste as fallback — mirroring Wispr's own primary+fallback shape and reducing clipboard exposure. Handy has no secondary macOS strategy if Accessibility permission is revoked mid-session; suzune should degrade gracefully.
- Optional auto-submit (Return variants) after paste, as Handy does.
- **Privacy positioning:** request the **minimum** Accessibility scope and document it — directly countering Wispr's keystroke-read controversy (weakness #8).

### 4.6 Reference stack summary

```
Language/shell:  Rust + Tauri 2 (fork/mirror Handy's architecture)
Capture:         cpal @ device-native rate -> rubato resample -> 16kHz/30ms frames
VAD:             Silero v5 (ONNX), silence-endpointed, post-capture filter
ASR (default):   Parakeet TDT v2 (Eng) / v3 (multiling); WhisperKit alt; Whisper GGML for 99-lang
LLM cleanup:     Embedded MLX (or llama.cpp/Metal) + 1-3B Q4 instruct, OpenAI-compat client
Injection:       AXUIElement insert -> clipboard-paste fallback -> direct-type option
Coordination:    single-thread TranscriptionCoordinator state machine
Memory:          idle-unload ASR model between bursts (16GB budget)
```

---

## 5. Open questions phase-2 planning must decide

| # | Question | Why it matters | Evidence gap |
|---|---|---|---|
| Q1 | Does Parakeet ONNX actually get **CoreML/Metal acceleration on Mac**, or is it CPU-bound like in Handy? | Determines whether Parakeet or WhisperKit is the true default; drives latency budget. | Handy exposes no Metal/CoreML ORT EP; unverified without runtime testing. Requires a Mac benchmark. |
| Q2 | **Real measured end-to-end latency** on M1 Pro for the chosen stack (capture-stop -> injected text). | Wispr's 700ms is self-reported and unmeasured; suzune needs its own benchmark, not a borrowed target. | No independent Wispr measurement exists; no suzune measurement yet. |
| Q3 | Which **1-3B cleanup model + prompt** actually removes fillers/formats without hallucinating or over-editing? | The edit pass is the quality differentiator; no benchmarked filler-removal model exists. | No dedicated model found in research; needs empirical prompt/model bake-off. |
| Q4 | **Streaming (Moonshine v2) in v1 or defer to v2?** | Affects UX (live partial captions) vs implementation complexity (backtracking UI). | Judgment says defer; needs a product-UX decision. |
| Q5 | **Screenshot/on-screen context feature** — build a fully-local version, or omit entirely? | Wispr's context-awareness drives accuracy on proper nouns but is its biggest privacy liability. | suzune could do it 100% locally (local OCR/vision) — scope/feasibility undecided. |
| Q6 | **macOS Accessibility scope** — can suzune do write-only injection (AXUIElement insert) without broad read access? | Directly counters Wispr's keystroke-read controversy; a marketing/trust point. | Feasibility per-app unverified; needs testing across target apps. |
| Q7 | **Licensing check for Parakeet** (CC-BY-4.0 NGC) and each shipped checkpoint for commercial/OSS redistribution. | suzune is open-source; must confirm redistribution rights per model. | "verify per-checkpoint license before commercial use" flagged but not resolved. |
| Q8 | **Independent verification of the 2025 privacy incident** before suzune cites it in its own materials. | Core competitive narrative rests on a single third-party source. | Could not cross-verify against a second independent source. |
| Q9 | **Fork Handy vs greenfield build?** | Handy is MIT and a near-exact architectural match ("most forkable"); forking saves months but inherits its gaps. | Judgment leans fork-and-extend; needs an explicit decision. |
