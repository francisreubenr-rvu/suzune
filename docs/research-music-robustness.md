# Background Music Robustness Research — suzune

Date: 2026-07-07
Scope: empirical WER/VAD measurement of background-music degradation on the real Parakeet TDT v2 int8 engine and Silero v4 endpointer, plus web research comparing DeepFilterNet, RNNoise/`nnnoiseless`, and a do-nothing baseline as the pre-ASR mitigation.

---

## Part A — Empirical degradation measurement

### A0. Methodology

**Speech reference.** `spikes/s1-asr-bench/samples/jfk3x.wav` (16kHz mono PCM16, 33.00s, confirmed via `afinfo`) — the JFK inaugural line "And so, my fellow Americans: ask not what your country can do for you — ask what you can do for your country," repeated 3x. Ground truth was **not** assumed from the nominal transcript; it was derived per-harness from a clean-run transcription (see A2), because the clean run itself turned out to only capture part of the buffer under one of the two code paths tested — that's a finding in its own right, not a measurement artifact to paper over.

**Music source.** `/Volumes/1TB SSD/f Personal/Listen.mp3` — a real, pre-existing 1-hour AAC file (128kbps, 44.1kHz stereo, `ISO Media file produced by Google Inc.` container metadata typical of a yt-dlp-style download) already present on this machine. This is genuine music, not a noise bed: two other candidate sources found via `mdfind` (`raw/flip/public/audio/{rain,forest,cafe,binaural,brown}.mp3`) were inspected and rejected — they are ambient/focus-timer soundscapes (rain, forest, cafe chatter, binaural tone, brown noise) for the `flip` project, not music with chords/rhythm. `Listen.mp3` was verified as tonal/harmonic content (not broadband noise) by rendering `ffmpeg -lavfi showspectrumpic` spectrograms of two 30s regions and visually confirming discrete horizontal harmonic bands (concentrated ~1.5-7kHz) rather than a flat noise floor. A 35s segment was extracted at `-ss 610 -t 35`, downmixed to 16kHz mono PCM16 to match `jfk3x.wav`:
```
ffmpeg -ss 610 -t 35 -i "/Volumes/1TB SSD/f Personal/Listen.mp3" -ac 1 -ar 16000 -c:a pcm_s16le music_raw.wav
```
No synthesized audio was needed — real music was available, so the synthesis fallback in the task brief was not used.

**SNR mixing.** Global RMS of both signals was computed directly from PCM samples with `numpy` (`~/.venvs/data/bin/python`, no `soundfile`/`librosa` needed — stdlib `wave` + numpy on 16-bit PCM is exact). Measured: speech RMS = **-16.95 dBFS**, raw music segment RMS = **-12.86 dBFS**. Music was linearly scaled so `20*log10(speech_rms / scaled_music_rms) = target_dB`, mixed, and — only where the sum would clip (targets at or below 0dB) — both components were rescaled *together* by the same headroom factor before 16-bit quantization, which preserves the SNR ratio exactly rather than distorting it. The script re-measured the actual RMS of both components as written to the output file and recomputed the realized SNR; in every case the realized value matched the nominal target to 3 decimal places (e.g. target -10.00dB -> measured -10.000dB), because the headroom step is a linear joint scale, not a hard clip. Six levels were mixed: the three requested (+15, +5, 0dB) plus -10, -15, -20dB, added after the 0dB result was still perfect under one of the two code paths — see A2.

**Two code paths were tested, not one**, because reading `src/coordinator.rs` (the app's actual push-to-talk handler) revealed it does **not** call into the VAD/endpointer crate at all:
```
/// Push-to-talk/toggle both have an explicit stop, so the whole buffer
/// is transcribed — no VAD endpointing here (M1 finding: endpointing is
/// for a future hands-free mode).
```
(`src/coordinator.rs:210-212`; confirmed further by `src/models.rs:9-11`, "push-to-talk / continuous dictation transcribes the whole buffer and does not use VAD"). `pipeline-cli --wav`, by contrast, explicitly drives `frames -> VAD endpointing (Endpointer/Silero v4) -> ASR` per its own doc comment (`crates/pipeline-cli/src/main.rs:1-4`) and its `collect_utterance()` returns on the **first** `SpeechEnd` event — i.e. it tests the endpointer path reserved for a future hands-free mode, not today's push-to-talk flow. Running only `pipeline-cli` (as literally specified) would have measured the wrong code path for the product's current default behavior, so a second harness was built to close that gap:

- **Harness 1 — `pipeline-cli --wav`** (unmodified, built via `cargo build --release -p suzune-pipeline-cli`, run with no `--cleanup` flag): tests the VAD-endpointer path.
- **Harness 2 — `asr-direct-probe`**: a throwaway Rust binary in the scratchpad dir (`/private/tmp/.../scratchpad/asr-direct-probe/`), `Cargo.toml` path-depends on `suzune-asr` exactly as `crates/asr/tests/parakeet_transcribe.rs` already does, calling `Engine::transcribe(&audio, Some("en"))` directly on the raw wav samples with **zero VAD**, replicating `coordinator.rs::process()` line-for-line. This lives entirely outside the repo — no product files were created, modified, or depended-on-in-tree; it's a read-only consumer of `suzune-asr`'s existing public API, the same as the existing ignored test.

VAD internals were also independently probed: `silero_vad_v4.onnx` was loaded directly via `onnxruntime` (installed into the sanctioned `~/.venvs/data` venv) and the exact `Endpointer` state machine from `crates/vad/src/endpointer.rs` (threshold 0.5, `min_speech_frames`=5 @ 150ms, `trailing_silence_frames`=24 @ 700ms, 30ms/480-sample frames) was reimplemented frame-by-frame in Python to log every `SpeechStart`/`SpeechEnd` transition and timestamp per SNR level. This is a read-only diagnostic against the existing model file, not a code change.

WER was computed with a standard word-level Levenshtein DP (substitutions/deletions/insertions), lowercased and punctuation-stripped, no external library.

### A1. Harness 1 results — VAD-endpointer path (`pipeline-cli`, future hands-free mode)

Reference: the harness's own **clean-run** transcript, per the task brief's instruction. That clean-run transcript turned out to be truncated: even with zero added noise, `collect_utterance()` returns on the first `SpeechEnd`, and JFK's real inaugural delivery has a >700ms dramatic pause after "my fellow Americans:" before "ask not..." — enough to fire `SpeechEnd` on its own. This reproduces identically on the un-repeated `jfk.wav` (11s, single sentence), so it is not an artifact of the 3x-repeat file. **Reference transcript: "And so, my fellow Americans."** (5 words).

| SNR (measured) | Transcript | Captured utterance length | WER | S/D/I |
|---|---|---|---|---|
| clean (no music) | "And so, my fellow Americans." | 2.7s | 0.0% | 0/0/0 |
| +15dB | "And so, my fellow Americans." | 2.6s | 0.0% | 0/0/0 |
| +5dB | "And so, my fellow Americans." | 2.6s | 0.0% | 0/0/0 |
| 0dB | "As not." | 1.9s | 100.0% | 2/3/0 |
| -10dB | (no speech detected) | — | 100.0% | 0/5/0 |
| -15dB | (no speech detected) | — | 100.0% | 0/5/0 |
| -20dB | (no speech detected) | — | 100.0% | 0/5/0 |

The frame-level VAD probe explains the 0dB collapse precisely rather than leaving it as a black box. In clean/+15dB/+5dB, `SpeechStart` fires at t≈0.45-0.54s (the true utterance onset) and `SpeechEnd` at t≈2.79-2.85s. At 0dB, `SpeechStart` does not fire until **t=3.48s** — the true onset's speech probability never crosses the 0.5 threshold under the mixed music, so the endpointer silently skips it and instead locks onto what was, in the clean recording, a *different*, later utterance boundary (t=3.51-4.41s in the clean probe). The captured 1.9s buffer is a real slice of the recording, just the wrong one — the engine transcribes it faithfully as "As not.", a plausible-sounding but entirely wrong result with no error surfaced to the caller. At -10dB and below, no frame ever crosses the speech-probability threshold across the full 33s buffer, so `SpeechStart` never fires at all: total, silent utterance loss.

### A2. Harness 2 results — direct whole-buffer path (`asr-direct-probe`, matches today's push-to-talk exactly)

Reference: clean-run transcript, the full 3x-repeated sentence (66 words after normalization), transcribed **exactly** by the direct path (no truncation — this harness has no endpointer to fire early).

| SNR (measured) | WER | S/D/I | Notes |
|---|---|---|---|
| clean (no music) | 0.0% | 0/0/0 | exact 3x match |
| +15dB | 0.0% | 0/0/0 | exact 3x match |
| +5dB | 0.0% | 0/0/0 | exact 3x match |
| 0dB | 0.0% | 0/0/0 | exact 3x match |
| -10dB | 0.0% | 0/0/0 | exact 3x match — music 10dB **louder** than speech, still zero WER |
| -15dB | 27.3% | 17/1/0 | garbled but partially recognizable: "And so myself, America, and not what your country can do for you..." |
| -20dB | 100.0% | 0/66/0 | empty transcript — total ASR failure |

This is the headline empirical result: on this one music bed and this one speech sample, Parakeet TDT v2 int8, fed the whole buffer with no VAD in front of it (i.e. today's real push-to-talk code path, not a simulation of it), produced a **perfect** transcript down to -10dB SNR — music ten decibels louder than the speaker. Measurable degradation only appeared at -15dB, and total failure only at -20dB.

### A3. VAD/endpointing behavior under music — direct answer

Yes, the pipeline can trim/truncate under music, but **only on the endpointer code path that today's push-to-talk does not use**. Two distinct, measured failure modes on that path: (1) at 0dB, the endpointer silently substitutes a plausible-but-wrong segment of the buffer rather than the intended one, with no error signal; (2) at -10dB and below, it drops the entire utterance with no speech detected at all. Today's push-to-talk flow is immune to both failure modes as currently implemented, because `coordinator.rs::process()` never calls the endpointer — the whole buffer always reaches the ASR engine untrimmed, verified by direct code inspection and by Harness 2's clean/lightly-noisy results matching bit-for-bit. This is exactly the risk profile the working design position (enhancement, not gating) is arguing against — the measurement confirms the concern is real, on the endpointer path that exists in this codebase for a future feature, not a hypothetical.

### A4. Caveats

- Single music excerpt, single speaker (JFK, unusually clear historical recording), single 66-word reference sentence — not a statistically powered benchmark. Do not extrapolate these exact dB thresholds to other music genres, speakers, or microphones without re-measuring.
- Music source is a personal file with unverified provenance/licensing (`f Personal/Listen.mp3`); used only for local, non-distributed measurement, kept entirely in scratchpad, never copied into the repo.
- Headroom rescaling at 0dB and below reduces the mixed file's overall loudness (both components scaled down together) to avoid 16-bit clipping; this preserves the SNR ratio exactly but does not model a physically clipped microphone signal, which a genuinely deafening live room could produce and this synthetic mix does not.
- Only one -15dB and one -20dB point were measured per harness — no attempt to pin the exact WER-vs-SNR cliff shape between -10 and -20dB more finely than that.

---

## Part B — Mitigation research

### B1. DeepFilterNet (`deep_filter` Rust crate)

- **License**: dual MIT/Apache-2.0, confirmed in `libDF/Cargo.toml` (`license = "MIT/Apache-2.0"`) and the repo's `LICENSE-MIT`/`LICENSE-APACHE` files ([github.com/Rikorose/DeepFilterNet/tree/main/libDF](https://github.com/Rikorose/DeepFilterNet/tree/main/libDF)).
- **Sample rate**: native full-band 48kHz — the paper's own subtitle is "A Low Complexity Speech Enhancement Framework for Full-Band Audio (48kHz) using Deep Filtering" ([arxiv.org/abs/2205.05474](https://arxiv.org/abs/2205.05474)). In suzune's capture chain this means DFN would need to sit in `crates/audio/src/lib.rs`, before the `FrameResampler` step that currently downsamples device-rate audio straight to the 16kHz all downstream stages expect (`crates/audio/src/lib.rs:1-8`) — exactly where the task brief placed it.
- **Real-time factor**: reported figures are inconsistent across the project's own papers and are reported here as-is rather than force-reconciled. DeepFilterNet2 paper: RTF **0.04** on a notebook i5-8250U (single-threaded), RTF **0.42** on Raspberry Pi 4, 2.306M params, 0.356G MACs; DeepFilterNet (v1) on the same i5-8250U: RTF 0.11, 1.778M params ([ar5iv.labs.arxiv.org/html/2205.05474](https://ar5iv.labs.arxiv.org/html/2205.05474)). The DeepFilterNet3/Interspeech-2023 paper separately states RTF **0.19** on the same i5-8250U class of hardware, without reconciling the discrepancy against the 0.04 DFN2 figure ([arxiv.org/abs/2305.08227](https://arxiv.org/abs/2305.08227), via [ar5iv.labs.arxiv.org/html/2305.08227](https://ar5iv.labs.arxiv.org/html/2305.08227), PESQ 3.17 / STOI 0.944 on Voicebank+DEMAND). **No Apple Silicon/M-series RTF numbers were found in any source searched** — flagged as an explicit unknown, not assumed. [Judgment] M-series CPUs substantially outperform both an i5-8250U and a Raspberry Pi 4 on comparable single-thread workloads, so real-time headroom is likely, but this is inference, not measurement.
- **Model size / shipping**: models live in the repo's `models/` directory as downloadable archives, not embedded by default at arbitrary size — verified via the GitHub contents API: `DeepFilterNet3_onnx.tar.gz` ≈ 7.98MB (current recommended default), `DeepFilterNet3_ll_onnx.tar.gz` (low-latency variant) ≈ 36.4MB, `DeepFilterNet2.zip` ≈ 8.6MB, `DeepFilterNet.zip` (v1) ≈ 6.6MB. The crate's `default-model` Cargo feature embeds the DFN3 weights into the compiled binary at build time (`libDF/Cargo.toml`: `default-model = [] # Include default DFN3 model`); no separate runtime download is required if that feature is enabled.
- **Maintenance status**: the crates.io package is stale. Direct crates.io API query: `deep_filter` max published version is **0.2.5**, published **2022-07-28** — roughly four years old relative to today. GitHub source has moved well past that (`Cargo.toml` on `main` shows `version = "0.5.7-pre"`, with a tagged `v0.5.6` release on 2023-08-31 and the last commit to the repo on 2024-10-17, ~1 year 9 months of inactivity as of this writing; not archived, 4,424 stars, 55 open issues). **Practical implication**: `cargo add deep_filter` from crates.io pulls a version roughly four years behind upstream; using current code requires a git dependency pinned to a commit, not the published crate.
- **Integration cost specific to suzune**: the crate's default Rust path runs on `tract` (`tract-core`/`tract-onnx`/`tract-pulse`/`tract-hir`), a *different* inference engine from `ort` (ONNX Runtime), which is what suzune already depends on for both the Parakeet ASR engine (`transcribe-rs` -> `ort` 2.0.0-rc.12, confirmed in `Cargo.lock`) and the Silero VAD (`vad-rs` -> `ort`, same version, confirmed in `Cargo.lock`). Pulling in `deep_filter` as-is would add a second ML inference runtime to the dependency tree and binary. Because DFN also publishes ONNX-exported weights (the `*_onnx.tar.gz` files above), an alternative integration path exists: load the ONNX graph through the `ort` dependency suzune already ships, reimplementing DFN's STFT/deep-filter pre/post-processing in suzune's own code instead of depending on `deep_filter`'s `tract`-based runtime. More up-front engineering, zero added inference-engine dependency.

### B2. RNNoise / `nnnoiseless`

- **License**: BSD-3-Clause, confirmed via the GitHub API's license field for [jneem/nnnoiseless](https://github.com/jneem/nnnoiseless).
- **Sample rate / architecture**: 48kHz, 480-sample (10ms) frames, same as the original Xiph RNNoise it ports — confirmed via crates.io's own description ("RAW 16-bit little-endian mono PCM files sampled at 48 kHz") and cross-checked against the upstream C project ([github.com/xiph/rnnoise](https://github.com/xiph/rnnoise), frame-size discussion in [issue #102](https://github.com/xiph/rnnoise/issues/102)). Architecturally it is a hybrid DSP/RNN system estimating per-band **gain masks** across 22 Bark-scale frequency bands (J.-M. Valin, "A Hybrid DSP/Deep Learning Approach to Real-Time Full-Band Speech Enhancement," arXiv:1709.08243, 2018) — simpler than DeepFilterNet's per-frame complex deep-filter taps, and a materially smaller model (0.06M params, 0.04G MACs vs DeepFilterNet2's 2.306M params / 0.356G MACs, per [ar5iv.labs.arxiv.org/html/2205.05474](https://ar5iv.labs.arxiv.org/html/2205.05474)).
- **RTF**: 0.027 single-threaded on the same i5-8250U reference point used in the DeepFilterNet papers ([ar5iv.labs.arxiv.org/html/2205.05474](https://ar5iv.labs.arxiv.org/html/2205.05474)) — roughly 1.5x faster than DeepFilterNet2's 0.04 and ~4x faster than DeepFilterNet3's 0.19, tracking its much smaller parameter count.
- **Quality on music vs steady noise**: [Judgment, weaker source] a third-party comparison blog (not peer-reviewed) reports RNNoise "struggled" on a next-room-TV-audio test case (speech-like, non-stationary interference) where DeepFilterNet3 "handled it noticeably better, suppressing the TV audio while leaving the voice intact" ([noisereducerai.com/blogs/rnnoise](https://noisereducerai.com/blogs/rnnoise)) — directionally consistent with RNNoise's simpler gain-masking design being less able to track fast-changing musical/speech-like spectral content than DFN's deep filtering, but this specific claim is not from a peer-reviewed benchmark and should be weighted accordingly. On PESQ specifically, RNNoise scores 2.33 in the DeepFilterNet2 paper's own comparison table ([ar5iv.labs.arxiv.org/html/2205.05474](https://ar5iv.labs.arxiv.org/html/2205.05474)) versus DeepFilterNet3's 3.17 on a different test set (Voicebank+DEMAND, [arxiv.org/abs/2305.08227](https://arxiv.org/abs/2305.08227)) — different corpora, so treat this as directional, not a controlled head-to-head.
- **Maintenance status**: markedly healthier than `deep_filter`'s crates.io situation. Direct API queries: `nnnoiseless` newest crates.io version **0.5.2**, last published **2025-12-18**; GitHub's last push is the same date, repo not archived, 354 stars, 5 open issues.

### B3. Do-nothing baseline — tie to Part A measurements

- This project's own Harness 2 measurement (A2) is itself the strongest available evidence for the do-nothing case: Parakeet TDT v2 int8, given the whole utterance with no VAD in front of it, produced **zero WER from clean through -10dB SNR** on this music bed, only degrading at -15dB (27.3% WER) and failing at -20dB. That is 10dB+ of native headroom under this specific test before enhancement would have changed the outcome at all.
- NVIDIA's own published robustness benchmark for **Parakeet-TDT-0.6B-v3** (not the v2 int8 suzune ships, but the same FastConformer-TDT architecture family) on LibriSpeech Clean with added **MUSAN music and noise** samples shows a materially steeper curve: WER 1.92% at 100dB (effectively clean) → 1.96% at 25dB → 2.62% at 5dB → 4.82% at 0dB → 12.21% at -5dB ([arxiv.org/html/2509.14128v1](https://arxiv.org/html/2509.14128v1), Table 6). This disagrees with this report's own 0.0% WER at 0dB on a magnitude basis, though it agrees on direction (mild degradation near 0dB, real degradation only becoming severe below -5 to -10dB). Plausible reasons for the gap, none confirmed: a single very clearly enunciated historical recording (JFK) vs LibriSpeech's more varied speaker population; one specific music excerpt vs MUSAN's diverse noise/music library; v2 vs v3 model differences; and possible differences in how "SNR" was computed between the two methodologies. Both are reported as-is rather than reconciled.
- On *why* Parakeet tolerates noise at all: the Canary-1B-v2 paper (same authors/family) attributes some noise tolerance to training on "large-scale datasets that naturally include noisy conditions, such as the YouTube subset" ([arxiv.org/html/2509.14128v1](https://arxiv.org/html/2509.14128v1)) rather than describing an explicit synthetic noise-augmentation step for Parakeet-TDT specifically — this is the paper's own framing, not a confirmed statement that Parakeet-TDT-0.6B-v2 was trained with deliberate noise/SpecAugment-style augmentation. General Conformer-family literature confirms noise-mixing augmentation (background noise/music added at varying SNR during training, often combined with SpecAugment time/frequency masking) is a standard, well-established technique for improving robustness in this model family broadly, but this is general-literature context, not a Parakeet-TDT-specific confirmation.

### B4. Option comparison

| | DeepFilterNet (via `ort` + ONNX weights) | RNNoise (`nnnoiseless`) | Do nothing |
|---|---|---|---|
| License | MIT/Apache-2.0 dual ([libDF/Cargo.toml](https://github.com/Rikorose/DeepFilterNet/tree/main/libDF)) | BSD-3-Clause ([jneem/nnnoiseless](https://github.com/jneem/nnnoiseless)) | — |
| Native sample rate | 48kHz full-band ([arxiv 2205.05474](https://arxiv.org/abs/2205.05474)); sits in `crates/audio` before the 16kHz downsample | 48kHz, 480-sample/10ms frames ([xiph/rnnoise](https://github.com/xiph/rnnoise)) | — |
| Model size | ~8MB (DFN3 ONNX tar.gz, GitHub contents API); `default-model` feature can embed at build time | 0.06M params, weights compiled into the crate | 0 |
| RTF (i5-8250U, single thread; no Apple Silicon numbers found) | 0.04 (DFN2) to 0.19 (DFN3) — papers disagree, both cited in B1 | 0.027 ([arxiv 2205.05474](https://ar5iv.labs.arxiv.org/html/2205.05474)) | 0 |
| Reported quality on music-like interference | Best of the three crates researched; deep filtering tracks non-stationary/speech-like interference ([noisereducerai.com](https://noisereducerai.com/blogs/rnnoise), weaker source, flagged in B2) | Weakest reported; gain-mask design targets steady noise | Measured here: zero WER to -10dB SNR on push-to-talk path (A2) |
| Crate maintenance | crates.io 4 years stale (0.2.5, 2022-07-28); GitHub last commit 2024-10-17 — use git-pinned dep or `ort`+ONNX route | Healthy: 0.5.2 published 2025-12-18 | — |
| Inference runtime cost | `tract` (new dep) via crate, or zero new runtime via existing `ort` + reimplemented pre/post-processing | None (pure Rust, no ML runtime) | None |
| Word-drop risk | None — continuous per-frame transform, no gating | None — same | None on push-to-talk path; severe on future VAD path (A1/A3) |

---

## Recommendation

[Judgment] Enhancement remains the architecturally correct choice **in principle**, for the reason the working design position states: DeepFilterNet is a continuous per-frame spectral transform, not a gate or segmentation mechanism, so it structurally cannot drop words the way VAD-based endpointing can — and this report's own measurement (A1/A3) shows that risk is real and severe on the endpointer path this codebase already has wired up for a future hands-free mode (silent wrong-segment substitution at 0dB, total utterance loss at -10dB and below).

But the urgency differs by code path, and this report's measurement changes where the priority should land:

- **Today's push-to-talk path (A2) does not need enhancement urgently.** It already tolerates this specific music bed down to roughly -10dB SNR — music louder than the speaker — with zero measured WER impact, because it sends the whole buffer straight to Parakeet with no VAD in front of it. Typical "music playing in the room while dictating" scenarios are unlikely to reach -10 to -15dB relative to a mic held at normal dictation distance from the mouth.
- **A future VAD/hands-free mode is where enhancement earns its keep**, because that is the path this report measured actually breaking, and breaking silently (wrong output with no error, or total utterance loss) well before the ASR itself degrades.

If/when a hands-free mode is built on the existing `Endpointer`, DeepFilterNet is the better of the two mitigation crates researched, despite its worse maintenance signal — its deep-filtering approach is reported to hold up better than RNNoise's simpler gain-masking on music/speech-like interference (B2), which is precisely the failure mode this report measured. The recommended integration path is to consume DeepFilterNet's **ONNX-exported weights through suzune's existing `ort` dependency** (B1) rather than pulling in the `deep_filter` crate's own `tract` runtime, which avoids both the second-inference-engine cost and the four-years-stale crates.io package, at the cost of reimplementing DFN's pre/post-processing in-repo. `nnnoiseless` remains a reasonable fallback if the DFN integration proves too costly — it is smaller, faster (RTF ~0.027 vs ~0.04-0.19), BSD-3-Clause, and far better maintained on crates.io — but its weaker reported performance on music-like interference specifically (the exact scenario this research spike targets) makes it the second choice, not the default.

Do not spend engineering effort wiring an enhancement stage into today's push-to-talk flow on the strength of this measurement alone — re-measure with the app's real device-rate capture pipeline and a range of music genres/volumes before deciding that path needs it too.

---

Sources consulted: crates.io API (`deep_filter`, `nnnoiseless`), GitHub API (`Rikorose/DeepFilterNet`, `jneem/nnnoiseless`, `xiph/rnnoise`), [github.com/Rikorose/DeepFilterNet](https://github.com/Rikorose/DeepFilterNet), [arxiv.org/abs/2205.05474](https://arxiv.org/abs/2205.05474) / [ar5iv.labs.arxiv.org/html/2205.05474](https://ar5iv.labs.arxiv.org/html/2205.05474) (DeepFilterNet2), [arxiv.org/abs/2305.08227](https://arxiv.org/abs/2305.08227) / [ar5iv.labs.arxiv.org/html/2305.08227](https://ar5iv.labs.arxiv.org/html/2305.08227) (DeepFilterNet3/Interspeech 2023), [github.com/jneem/nnnoiseless](https://github.com/jneem/nnnoiseless), [github.com/xiph/rnnoise](https://github.com/xiph/rnnoise) (incl. issue #102), arXiv:1709.08243 (Valin, RNNoise), [arxiv.org/html/2509.14128v1](https://arxiv.org/html/2509.14128v1) (Canary-1B-v2 & Parakeet-TDT-0.6B-v3), [noisereducerai.com/blogs/rnnoise](https://noisereducerai.com/blogs/rnnoise) (third-party, weaker source, flagged inline). Local sources: `src/coordinator.rs`, `src/models.rs`, `crates/pipeline-cli/src/main.rs`, `crates/vad/src/endpointer.rs`, `crates/vad/src/vad.rs`, `crates/asr/tests/parakeet_transcribe.rs`, `crates/audio/src/lib.rs`, `Cargo.lock` — all in this repo, read directly, unmodified.
