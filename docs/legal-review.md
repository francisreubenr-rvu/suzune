# Legal Review

A plain-language risk assessment for suzune. Not legal advice; consult a
qualified attorney before any public launch or commercial distribution.

## Summary table

| Area | Risk | Status |
|---|---|---|
| Software license clarity | Low | Resolved — MIT `LICENSE` added |
| Model licenses (Parakeet, Qwen2.5) | Low | Resolved — both permit commercial use + redistribution with attribution; attributed in `THIRD_PARTY_NOTICES.md` |
| Code attribution (Handy, crates) | Low | Resolved — Handy (MIT) and dependencies attributed |
| False advertising / defamation | Low | Resolved by design — only measured figures; generic "cloud dictation tools" phrasing; the single-sourced 2025 privacy incident is not cited |
| Name / trademark | Low | Resolved — "suzune"; see below and `naming-decision.md` |
| Model hosting dependency | Low/operational | Noted — first-run download of Parakeet uses a public community mirror; self-host before a wide public launch |

## Details

### 1. Software and model licenses (resolved)

- suzune's own code is now MIT-licensed (`LICENSE`).
- Parakeet TDT 0.6B v2 is **CC-BY-4.0** (© NVIDIA): commercial use and
  redistribution allowed with attribution. Attribution present.
- Qwen2.5-1.5B-Instruct is **Apache-2.0** (© Alibaba Cloud): commercial use
  and redistribution allowed. Attribution present.
- Models are downloaded from their publishers/mirrors at first run, not
  re-hosted in this repository, which further limits redistribution
  obligations.
- Architecture patterns adapted from Handy (MIT © cjpais) are attributed in
  `THIRD_PARTY_NOTICES.md`, satisfying MIT's attribution requirement.

### 2. Marketing claims (resolved by design)

- Every performance number is a measurement on the author's hardware,
  labelled as such — no invented benchmarks, no claims about competitors'
  internals.
- Comparisons use the generic phrase "cloud dictation tools," name no
  competitor, reproduce no competitor logo, and claim no affiliation.
- No competitor is named or referenced in any suzune material, avoiding
  defamation risk.
- An independence/trademark disclaimer is in the README.

### 3. The name — "suzune"

The product is named **suzune** (鈴音, Japanese for "the sound of a bell"),
chosen for phonetic fit, meaning, and — critically — verified availability:
no voice/dictation/AI product or software trademark uses it. Full rationale
is in `naming-decision.md`.

Residual good practice before or shortly after a public launch: run a formal
USPTO/registrar trademark clearance on "suzune" in the software class (the
availability check here was web-search-based, not a registrar lookup), and
secure the GitHub/npm/domain handles you care about.

### 4. Model hosting (operational)

The first-run downloader fetches the Parakeet ONNX build from a public
community mirror. The model's CC-BY-4.0 license permits this, but before a
wide public launch you should host the model files yourself (or pull from the
publisher's own repository) rather than relying on a third party's bandwidth.
