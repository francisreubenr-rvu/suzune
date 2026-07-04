# LinkedIn announcement — draft

Status: DRAFT. Held for your review — do not post until you have decided the
naming question in `docs/legal-review.md`. Professional-but-personal tone,
only measured claims, no named competitor.

---

I spent the last few days building whispr — voice dictation that runs
entirely on my Mac. Nothing it hears ever leaves the machine.

The idea was simple and a little stubborn: press a key, talk, and have clean
text appear in whatever app I'm in — email, terminal, chat — without a single
byte going to a server. Most dictation tools are wonderful right up until you
read the privacy policy. I wanted the privacy to be a property of the
architecture, not a promise in a document. If there's no network code in the
audio path, there's nothing to trust.

How it works, end to end, on-device:

- Capture the mic and hand it to a small speech model (NVIDIA's Parakeet)
  running locally.
- Pass the raw transcript through a tiny local language model (Qwen2.5-1.5B)
  that strips the "um"s, applies your mid-sentence corrections, and fixes
  punctuation — one quiet cleanup pass.
- Drop the finished text into the app you're already using.

On my MacBook M1 Pro it transcribes a ten-second sentence in about 400
milliseconds, the cleanup adds a couple hundred more, and the whole thing
lands in roughly a second. No account, no subscription, no cloud — and it's
open source under the MIT license.

The parts I did not expect to be the hard parts: an iPhone quietly stealing
the microphone over Continuity, terminal apps that accept text and then
silently drop it, and a first-run experience that fetches its own models so
there's nothing to install by hand. The unglamorous 80%.

It's early, and macOS-first, but it works and I use it every day. If
local-first tools are your thing, I'd love your thoughts.

#localfirst #privacy #opensource #macOS #ondevice #speechrecognition

---

## Notes for posting (when cleared)

- Attach the demo video (`docs/demo/whispr-demo.mp4`).
- Optionally link the repo once it is made public and the name is settled.
- Keep the comparison implicit ("most dictation tools") — do not name or tag
  any competitor, per the legal review.
