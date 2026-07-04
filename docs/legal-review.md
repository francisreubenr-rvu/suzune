# Legal Review

A plain-language risk assessment for whispr. Not legal advice; consult a
qualified attorney before any public launch or commercial distribution.

## Summary table

| Area | Risk | Status |
|---|---|---|
| Software license clarity | Low | Resolved — MIT `LICENSE` added |
| Model licenses (Parakeet, Qwen2.5) | Low | Resolved — both permit commercial use + redistribution with attribution; attributed in `THIRD_PARTY_NOTICES.md` |
| Code attribution (Handy, crates) | Low | Resolved — Handy (MIT) and dependencies attributed |
| False advertising / defamation | Low | Resolved by design — only measured figures; generic "cloud dictation tools" phrasing; the single-sourced 2025 privacy incident is not cited |
| **Name: "whispr" vs "Wispr Flow"** | **Elevated** | **Open — requires your decision (see below)** |
| Model hosting dependency | Low/operational | Noted — first-run download of Parakeet uses a public community mirror; self-host before a wide public launch |

## Details

### 1. Software and model licenses (resolved)

- whispr's own code is now MIT-licensed (`LICENSE`).
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
- The 2025 Wispr Flow privacy incident is single-sourced and is deliberately
  **not** referenced in any whispr material, avoiding defamation risk.
- An independence/trademark disclaimer is in the README.

### 3. The name — elevated risk, your call

"whispr" is one letter from **"Wispr"** (Wispr AI's product, "Wispr Flow"),
and whispr is positioned as a direct alternative to it. Under trademark law,
the test is *likelihood of confusion*, and a near-identical name in the same
product category is the classic fact pattern for a dispute. "whispr" also
evokes OpenAI's "Whisper" speech model, though that term is more descriptive.

This cannot be resolved in code. Options, roughly in order of safety:

1. **Rename** before any public launch (e.g. a distinct coined word). Safest.
2. **Keep the name but stay private / personal-use**, which is the current
   posture (repo is private, nothing published). Low exposure while not
   public.
3. **Keep the name and launch publicly** after a trademark clearance search
   and, ideally, counsel sign-off. Highest exposure without that.

Recommendation: do not make the repository public, publish the landing page,
or post the announcement under this name until you have decided on 1–3. The
deliverables are prepared but held for your go-ahead precisely so this
decision is yours.

### 4. Model hosting (operational)

The first-run downloader fetches the Parakeet ONNX build from a public
community mirror. The model's CC-BY-4.0 license permits this, but before a
wide public launch you should host the model files yourself (or pull from the
publisher's own repository) rather than relying on a third party's bandwidth.
