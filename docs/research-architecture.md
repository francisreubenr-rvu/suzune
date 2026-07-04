# Wispr Flow — Technical Architecture Research

Research date: 2026-07-03. Compiled for the `suzune` project (local/open-source alternative to Wispr Flow).

All claims are sourced. Where a claim is Wispr's own marketing/documentation, it is marked **[Wispr-stated]**. Where it comes from a third-party reviewer, blog, or reverse-engineering effort, it is marked **[Reviewer/third-party]** — treat as unverified speculation, not fact. Where nothing public was found, it is marked **[UNKNOWN]**.

---

## 1. Audio capture pipeline

| Aspect | Finding | Source |
|---|---|---|
| Activation model | Hotkey-based (press to activate, not always-on). "Flow Sessions" feature lets users configure microphone-access timeout windows. | [Wispr Flow Wikipedia](https://en.wikipedia.org/wiki/Wispr_Flow) |
| Streaming vs batch | Audio is **streamed to the backend**, not batched after full utterance, and is **not persisted locally** on the client. | [Security & Compliance FAQ](https://docs.wisprflow.ai/articles/3467817258-security-and-compliance-faq) |
| Capture pipeline internals (buffering, sample rate, VAD, codec) | **[UNKNOWN]** — not disclosed in any public doc found. Wispr's own technical blog post explicitly does not cover audio capture mechanism or streaming pipeline specifics. | [Technical challenges post](https://wisprflow.ai/post/technical-challenges) |
| Sub-audible/quiet speech handling | Wispr claims to handle "sub-audible speech (subvocalization)" — users speaking quietly into the mic — but gives no implementation detail. | [Technical challenges post](https://wisprflow.ai/post/technical-challenges) [Wispr-stated] |

## 2. Speech recognition backend

- **Cloud-only, no on-device ASR for transcription.** Wispr's own privacy page states plainly: *"Transcription always happens in the cloud to provide the best speed and accuracy."* This directly rules out an offline/local transcription mode. [[Wispr Flow Privacy]](https://wisprflow.ai/privacy) [Wispr-stated]
- Wispr describes its ASR as proprietary/custom: *"The world's best ASR models (context aware, personalized, and code-switched)"* — not a bare wrapper around a third-party model. No specific model name, architecture, or base model (e.g., whether it's a fine-tune of Whisper, Conformer, etc.) is disclosed anywhere in official material. [[Technical challenges post]](https://wisprflow.ai/post/technical-challenges) [Wispr-stated, unverifiable]
- A post-ASR layer of "personalized LLMs with token-level formatting control" handles cleanup/formatting — see section 5. [[Technical challenges post]](https://wisprflow.ai/post/technical-challenges) [Wispr-stated]
- Third-party/independent reviewers note Wispr layers **proprietary models on top of a speech-recognition base** and continuously retrains on user feedback, but do not identify the underlying ASR vendor. [[Blockchain Council overview]](https://www.blockchain-council.org/ai/wispr-flow-explained-real-time-speech-to-text-ai-productivity-workflows/) [Reviewer/third-party, unverified]
- No public evidence was found confirming or denying use of any named third-party ASR API (Whisper API, Deepgram, Gladia, AssemblyAI, etc.) as the base engine. Comparative benchmark posts (e.g., Coval's STT provider roundup) discuss Deepgram/ElevenLabs/OpenAI Realtime latency figures as **industry context**, not as claims about Wispr's own stack. [[Coval STT benchmarks]](https://www.coval.ai/blog/best-speech-to-text-providers-in-2026-independent-benchmarks-and-how-to-choose/) — **do not attribute this to Wispr; it is general market data.**
- Language support: 104 languages; roughly 40% of dictations are English, 60% other languages (Spanish, French, German, Dutch, Hindi, Mandarin cited). [[Wispr Flow Wikipedia]](https://en.wikipedia.org/wiki/Wispr_Flow)
- Accuracy claim: Wispr markets "90% zero-edit accuracy" versus self-reported comparison figures of OpenAI 71%, ElevenLabs 63%, Siri 52% — this is a **Wispr-published benchmark**, not an independent one; no methodology was found. [[Why Flow]](https://wisprflow.ai/why-flow) [Wispr-stated, treat skeptically — self-serve comparison]

## 3. Text injection mechanism (macOS/Windows)

- **Primary path: macOS Accessibility API**, with a **clipboard/paste fallback** for apps that don't support Accessibility-based insertion. [[VibeWhisper comparison page]](https://vibewhisper.dev/comparison/wispr-flow-alternative/) [Reviewer/third-party — plausible but not confirmed by Wispr's own docs]
- macOS requires the user to grant **Microphone** and **Accessibility** permissions (System Settings → Privacy & Security → Accessibility). [[Re-verify permissions]](https://docs.wisprflow.ai/articles/5510622673-re-verify-wispr-flow-permissions-after-updating) [Wispr-stated]
- **Windows differs**: Flow does *not* require app-level Accessibility permissions on Windows — only system-level microphone access. Implies a different (likely simulated-keystroke or Windows UI Automation / SendInput-based) injection mechanism than macOS. Exact Windows mechanism is **[UNKNOWN]** — not documented publicly.
- iOS: implemented as a **third-party system keyboard**, not accessibility-based injection (different OS constraint). [[Wispr Flow Wikipedia]](https://en.wikipedia.org/wiki/Wispr_Flow)
- **Controversy**: Wispr AI has itself acknowledged the macOS app "can read the device user's keystrokes" — a byproduct of using the Accessibility API for text insertion — which raised security concerns among advisors/users per Wikipedia's sourcing. This is a **Wispr-acknowledged fact**, worth flagging for the local-alternative design (Accessibility-API-based injection tools often get broad read access, not just write). [[Wispr Flow Wikipedia]](https://en.wikipedia.org/wiki/Wispr_Flow)

## 4. Latency

- **Wispr's own stated engineering targets** [Wispr-stated, from their technical blog]:
  - End-to-end: full transcription **and** LLM formatting delivered within **700ms** of the user stopping speaking.
  - Budget breakdown: ASR inference <200ms, LLM inference <200ms, network <200ms (leaves ~100ms slack against the 700ms total).
  - Stated strategy: push larger/more capable models into that fixed latency budget rather than relaxing the budget.
  - Scale claim: processing "1 billion words a month" in production with 99.99% uptime target.
  - Source: [Technical challenges behind Flow](https://wisprflow.ai/post/technical-challenges)
- No independent/reviewer-measured latency benchmarks of Wispr Flow itself were found in this research pass (only general-market ASR provider latency figures, which are not about Wispr). Any specific "X ms measured" claim about Wispr Flow beyond the company's own post should be treated as **[UNKNOWN]**.

## 5. AI-edits layer (auto-formatting, filler removal, tone, context/dictionary)

[Wispr-stated, from marketing/help docs — consistent across multiple official pages]

- **Filler-word removal & self-correction handling**: recognizes mid-utterance corrections, e.g. "5pm, actually 6" → "6pm". [[Why Flow]](https://wisprflow.ai/why-flow)
- **Auto-formatting**: structures lists, paragraphs, emails automatically per destination app.
- **Context awareness**: uses on-screen context (what app/content the user is looking at) plus a personal dictionary to correctly transcribe uncommon names/terms; app-context detection is described as happening **device-side** "for privacy" in the technical post — contrast with section 6 below, where screenshots taken for this feature are uploaded and then stripped server-side, which is a distinct, less private mechanism than pure on-device detection. [[Technical challenges post]](https://wisprflow.ai/post/technical-challenges); [[Security & Compliance FAQ]](https://docs.wisprflow.ai/articles/3467817258-security-and-compliance-faq)
- **Developer/code mode**: recognizes camelCase, snake_case, and code syntax conventions when dictating into code editors. [[Why Flow]](https://wisprflow.ai/why-flow)
- **Personalization**: "personalized LLMs with token-level formatting control" adapt to individual user style over time, aiming to reduce manual post-edit rate. [[Technical challenges post]](https://wisprflow.ai/post/technical-challenges)
- **Tone matching**: referenced in general marketing copy but no mechanism detail beyond "context-aware formatting." [[Why Flow]](https://wisprflow.ai/why-flow)

## 6. Privacy model — what leaves the machine, retention, incidents

- **Audio always leaves the machine.** All transcription happens in the cloud; there is no offline/local ASR mode. [[Privacy page]](https://wisprflow.ai/privacy) [Wispr-stated]
- **Screenshots**: The Context Awareness feature captures periodic screenshots and **uploads them** to the backend for processing; they are "stripped on upload regardless of Privacy Mode or Cloud Sync setting" and stated to never be persisted server-side. [[Security & Compliance FAQ]](https://docs.wisprflow.ai/articles/3467817258-security-and-compliance-faq) [Wispr-stated]
- **Default retention (Privacy Mode OFF, the default for non-enterprise/non-HIPAA users)**: audio and transcripts "may be used to evaluate, train, and improve Wispr's models." Dictation history is retained if Cloud Sync is enabled. [[Security & Compliance FAQ]](https://docs.wisprflow.ai/articles/3467817258-security-and-compliance-faq); [[Privacy page]](https://wisprflow.ai/privacy)
- **Privacy Mode + Cloud Sync off = Zero Data Retention (ZDR)**: no dictation content stored server-side, no training use. This combination is the **default and irrevocable** for Enterprise/HIPAA BAA customers, but **off by default** for regular users — i.e., most users are opted into data retention/training-use unless they manually change the setting. [[Security & Compliance FAQ]](https://docs.wisprflow.ai/articles/3467817258-security-and-compliance-faq)
- **Third-party LLM subprocessors**: Customer content sent to third-party LLM providers "is not used to train their models" and is "generally deleted within 30 days," subject to that provider's own retention practices. The full subprocessor list is not public — it's kept in a DPA annex "available under NDA." [[Security & Compliance FAQ]](https://docs.wisprflow.ai/articles/3467817258-security-and-compliance-faq) — **note the vagueness**: "generally deleted" is not a hard guarantee, and the identity of the third-party LLM provider(s) is undisclosed.
- **Encryption**: TLS 1.2+ in transit; AES-256 at rest at DB/object-storage layer; HSM-backed key management (FIPS 140-2). Because the backend must decrypt audio to transcribe it, **true end-to-end encryption is not possible/offered** — the provider always has plaintext access to raw audio at time of processing. [[Security & Compliance FAQ]](https://docs.wisprflow.ai/articles/3467817258-security-and-compliance-faq) [Wispr-stated]
- **Compliance certifications**: SOC 2 Type I completed April 2026 (A-LIGN) — note this is Type I, not Type II, despite some marketing pages elsewhere claiming "SOC2 Type II." ISO 27001:2022 Stage 1 complete, Stage 2 in progress as of the FAQ's writing. HIPAA BAA available. [[Security & Compliance FAQ]](https://docs.wisprflow.ai/articles/3467817258-security-and-compliance-faq) — **This directly conflicts with other Wispr marketing copy (e.g., "Why Flow" and pricing pages) that states "SOC2 Type II, HIPAA and ISO 27001 compliant" outright** — flag this discrepancy; the FAQ (more technical/detailed page) is likely the more accurate/current source, but this is an internal inconsistency in Wispr's own public materials, not something this research can resolve.
- **Documented privacy incident (late 2025)** [Reviewer/third-party — see caveats]:
  - A developer (Ryan Shrott) publicly reported that Wispr Flow was transmitting audio and screenshots to cloud servers, including routing through third-party infrastructure (reported as OpenAI's), without what he considered clear disclosure, and cancelled his subscription.
  - Wispr Flow banned the user who raised the issue; CTO Sahaj Garg later apologized publicly for the ban — which is treated by the reporting source as an implicit confirmation the underlying privacy concern had merit.
  - Following the incident, Wispr added Privacy Mode (explicit "nothing stored" mode), made AI-training use explicit opt-in (previously implied opt-out), and pursued SOC2/HIPAA/ISO27001 certification.
  - Source is a third-party blog analysis (ModelPiper), not Wispr's own account — treat the narrative framing as the author's interpretation. The article also notes: compliance certifications attest to *stored*-data handling, not to what happens to data *in transit/during processing*, and there is no user-facing audit mechanism to verify Privacy Mode actually results in zero retention. [[Wispr Flow Privacy Incident — ModelPiper]](https://modelpiper.com/blog/wispr-flow-privacy-incident) [Reviewer/third-party]
  - **This incident could not be cross-verified against a second independent source in this research pass** — flagging as single-sourced. Recommend independent verification before citing as established fact in the suzune project's own materials.

## 7. Platform support and pricing

**Platform support** [[Wispr Flow Wikipedia]](https://en.wikipedia.org/wiki/Wispr_Flow):

| Platform | Status |
|---|---|
| macOS | Supported, system-level, Accessibility-API text injection |
| Windows | Supported, system-level, no Accessibility permission required |
| iOS | Supported via third-party system keyboard |
| Android | Supported |
| Linux | In development, not yet released |
| Web | In development, not yet released |

**Pricing** [[Wispr Flow Pricing page]](https://wisprflow.ai/pricing) [Wispr-stated]:

| Plan | Price | Notes |
|---|---|---|
| Flow Basic | Free | 2,000 words/week cap on Mac and Windows |
| Flow Pro | $15/mo billed monthly, or $12/mo ($144/yr) billed annually | Unlimited dictation |
| Flow Teams | $12/user/mo monthly, $10/user/mo annually | 3-seat minimum |
| Student | $6/mo | Requires verified .edu email |
| Enterprise | Custom | Contact sales; includes SSO, advanced security, bulk pricing |
| Free trial | 14 days of Flow Pro | No credit card required upfront; reverts to Basic after trial |

## 8. Company background (context)

- Wispr AI founded 2021 by Tanay Kothari and Sahaj Garg; originally building a non-invasive wearable for touchless smartphone control, pivoted to software (Wispr Flow) in 2024 after concluding hardware/AI maturity wasn't sufficient. [[Wispr Flow Wikipedia]](https://en.wikipedia.org/wiki/Wispr_Flow)
- Funding: $30M Series A (June 2025, Menlo Ventures) + $25M Series A extension (Nov 2025, Notable Capital) = $81M total raised. [[Wispr Flow Wikipedia]](https://en.wikipedia.org/wiki/Wispr_Flow)
- Reported revenue ~$3.8M (July 2024–July 2025), 50%+ monthly growth, 80% six-month retention, ~19% payment conversion — all self-reported per Wikipedia's sourcing, not independently audited. [[Wispr Flow Wikipedia]](https://en.wikipedia.org/wiki/Wispr_Flow) [Wispr-stated/unverified]

---

## Key implications for the `suzune` local-alternative project

1. Wispr Flow has **no on-device/offline transcription mode at all** — cloud dependency is total. This is the clearest differentiation opportunity for a local-first alternative.
2. Their stated latency budget (700ms end-to-end, 200/200/200ms split) is a useful target to benchmark against, but it is self-reported, not independently measured — build your own benchmark rather than trusting it as ground truth.
3. Accessibility-API-based text injection on macOS is the industry-standard approach (also implied by Wispr's own keystroke-reading admission) — worth being deliberate about scope of Accessibility permissions requested, given this was Wispr's own privacy controversy trigger.
4. The proprietary "ASR + personalization LLM" two-stage architecture (raw ASR, then a separate context/personalization LLM pass for formatting/corrections) is the core technical shape to replicate — Whisper/Parakeet-class open models plus a local small-LLM cleanup pass is the direct open-source analog.
5. Default-on data retention/training-use (Privacy Mode is opt-in, not opt-out) plus the undisclosed third-party LLM subprocessor is the most concrete privacy gap a local alternative should market against.

---

## Sources

- [Wispr Flow — Wikipedia](https://en.wikipedia.org/wiki/Wispr_Flow)
- [Wispr Flow Privacy page](https://wisprflow.ai/privacy)
- [Security and compliance FAQ — Wispr Flow Help Center](https://docs.wisprflow.ai/articles/3467817258-security-and-compliance-faq)
- [Re-verify Wispr Flow permissions after updating — Help Center](https://docs.wisprflow.ai/articles/5510622673-re-verify-wispr-flow-permissions-after-updating)
- [Why Wispr Flow over built-in voice mode](https://wisprflow.ai/why-flow)
- [Technical challenges and breakthroughs behind Flow](https://wisprflow.ai/post/technical-challenges)
- [Wispr Flow Pricing](https://wisprflow.ai/pricing)
- [Wispr Flow's Privacy Incident: What Happened, What Changed, and What It Means — ModelPiper](https://modelpiper.com/blog/wispr-flow-privacy-incident)
- [Wispr Flow Explained: Real-Time Speech-to-Text AI for Productivity — Blockchain Council](https://www.blockchain-council.org/ai/wispr-flow-explained-real-time-speech-to-text-ai-productivity-workflows/)
- [VibeWhisper vs Wispr Flow comparison](https://vibewhisper.dev/comparison/wispr-flow-alternative/)
- [Best STT Providers 2026: Independent Benchmarks — Coval](https://www.coval.ai/blog/best-speech-to-text-providers-in-2026-independent-benchmarks-and-how-to-choose/) (general market context only, not a Wispr-specific source)

### Attempted but inaccessible sources (404 at time of research)
- `docs.wisprflow.ai/articles/1922179110-data-security-encryption` (content recovered via cache/alternate query; see Section 6)
- `docs.wisprflow.ai/articles/6274675613-privacy-mode-data-retention` (content recovered via alternate query; see Section 6)
