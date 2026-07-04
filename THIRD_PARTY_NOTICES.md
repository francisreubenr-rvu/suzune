# Third-Party Notices

suzune is distributed under the MIT License (see `LICENSE`). It builds on
open-source software and on-device models that carry their own licenses.
Those licenses are reproduced or linked below, and their attribution
requirements are honored here.

## Models (downloaded at first run, not redistributed in this repository)

| Model | Role | Author | License | Terms |
|---|---|---|---|---|
| Parakeet TDT 0.6B v2 | Speech recognition | NVIDIA | CC-BY-4.0 | Commercial use and redistribution permitted with attribution to NVIDIA. |
| Qwen2.5-1.5B-Instruct | Transcript cleanup | Alibaba Cloud (Qwen team) | Apache-2.0 | Commercial use and redistribution permitted; license and notice retained. |
| Silero VAD | Voice activity detection (dev tooling only; not shipped in the app path) | Silero Team | MIT | Permissive. |
| Whisper (large-v3-turbo, optional) | Alternative speech recognition | OpenAI | MIT | Permissive. |

Attribution: **Parakeet TDT 0.6B v2 © NVIDIA, licensed under CC-BY-4.0**
(https://huggingface.co/nvidia/parakeet-tdt-0.6b-v2). **Qwen2.5-1.5B-Instruct
© Alibaba Cloud, licensed under Apache-2.0**
(https://huggingface.co/Qwen/Qwen2.5-1.5B-Instruct). Model files are fetched
from their publishers at first run; suzune does not host or redistribute the
model weights.

## Architecture and code

suzune's process architecture adapts patterns from **Handy**
(https://github.com/cjpais/Handy), MIT License, © cjpais — specifically the
audio capture/worker structure, the single-threaded transcription
coordinator, the model idle-unload approach, and the clipboard
save/paste/restore injection sequence. Handy's MIT license permits this reuse
with attribution.

```
MIT License — Handy

Copyright (c) cjpais

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction ... (full text at
https://github.com/cjpais/Handy/blob/main/LICENSE)
```

## Rust and JavaScript dependencies

suzune depends on the Tauri framework and Rust/JS crates including
`transcribe-rs`, `cpal`, `rubato`, `hound`, `enigo`, `arboard`,
`accessibility-sys`, `core-foundation`, `ureq`, `serde`, `flate2`, `tar`,
`vad-rs`, React, and Vite. These are distributed under permissive licenses
(predominantly MIT and Apache-2.0). Their full license texts are available in
the respective package repositories and, for Rust crates, can be regenerated
with `cargo about` or `cargo-license`.

## Fonts

The UI uses system fonts only (Iowan Old Style / Palatino / Georgia and the
platform monospace face). No third-party font files are bundled or
redistributed.

## llama.cpp

The transcript-cleanup pass runs the model through a local `llama-server`
(llama.cpp, MIT License, © The ggml authors and contributors,
https://github.com/ggml-org/llama.cpp). suzune invokes it as a local server;
it is not redistributed as part of this repository.
