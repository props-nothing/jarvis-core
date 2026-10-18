# Third-Party Research And Provenance

This project studies public architecture and documentation but does not import third-party source code by default. Before copying or adapting code, verify the exact file's license, notices, compatibility with the intended JARVIS license, and attribution requirements.

## Bundled Assets

One third-party asset is bundled in the product, with its licence verified against the upstream repository (not the prototype):

| Asset | Where | Source and licence |
| --- | --- | --- |
| Canonical face mesh (468 vertices, 898 triangles; positions and triangle indices only, as integers) | `apps/jarvisd/src/hud/head.js`, served by the console | MediaPipe `canonical_face_model.obj`, **Apache License 2.0**, Copyright 2019-2023 The MediaPipe Authors, <https://github.com/google-ai-edge/mediapipe>. Research record: `docs/research/integrations/mediapipe-canonical-face-model.md`. The attribution is in the header of `head.js` and a test keeps it there. |

Rust dependencies are checked against the licence allowlist by `cargo deny` in CI; a shipped archive also needs a generated list of
their licences before a public release (`P9-017`).

## Patched Office dependencies

`vendor/docx-rs-0.4.22` (MIT, bokuweb) and `vendor/rust_xlsxwriter-0.99.1`
(MIT OR Apache-2.0, John McNamara) retain their upstream sources and licenses.
Only their normalized Cargo manifests change the ZIP requirement to 9.0.4,
because their published ZIP 8 dependencies are yanked. Archive checksums,
source URLs and removal criteria are recorded in [vendor/README.md](vendor/README.md).

## Local Prototype

The former `example/` prototype identified itself as **Creative Commons BY-NC 4.0** and credited FatihMakes. It was removed from the current tree on 2026-10-13. Historical copies retain those terms; JARVIS's Apache-2.0 license does not relicense them. Do not copy its implementation into this distribution without explicit legal review or permission.

Behavioral ideas captured from the prototype include:

- interface-issued confirmation for irreversible actions
- undo for reversible local changes
- self-describing action discovery
- local wake-word gating and push-to-talk
- echo suppression and lip-sync timing
- source-labelled screen capture and multi-monitor coordinate checks
- on-demand memory recall instead of dumping all memory into prompts
- honest distinction between submitted and confirmed external effects

The clean-room migration record is [docs/migration/python-prototype.md](docs/migration/python-prototype.md).

## Upstream Architecture Study

Reviewed on **2026-09-20**:

| Project | Official source | Pattern studied |
| --- | --- | --- |
| OpenClaw | https://github.com/openclaw/openclaw and https://docs.openclaw.ai/llms.txt | Gateway/client topology, runtime and plugin ownership, doctor/repair, service lifecycle, scoped tools, updates |
| Goose | https://github.com/aaif-goose/goose | Rust workspace, CLI/desktop/API surfaces, MCP extensions, ACP, native release artifacts |
| Tailscale | https://github.com/tailscale/tailscale | Long-lived daemon plus CLI, local API, OS-specific services, state paths, update and integration tests |
| Tauri | https://github.com/tauri-apps/tauri and https://v2.tauri.app/llms.txt | Native desktop packaging, Rust/webview boundaries, updater and platform bundles |
| MCP | https://github.com/modelcontextprotocol/rust-sdk and https://modelcontextprotocol.io/llms.txt | Client/server separation, stdio, Streamable HTTP, protocol negotiation, OAuth, conformance |

| OpenAI Agents SDK | https://github.com/openai/openai-agents-python and https://openai.github.io/openai-agents-python/llms.txt | Runner boundary, tool approvals, guardrails, sessions, tracing, realtime, deterministic testing |
| LangGraph | https://github.com/langchain-ai/langgraph and https://docs.langchain.com/oss/python/langgraph/llms.txt | Checkpoints, interrupts, resume, durability modes, stateful/stateless subgraphs |
| Home Assistant Core | https://github.com/home-assistant/core | Manifest-driven integrations, setup/config flows, diagnostics, quality gates, colocated integration tests |
| ElevenLabs | https://elevenlabs.io/docs/llms.txt | Custom LLM SSE endpoints, MCP consumer mode, voice/telephony, signed post-call webhooks, turn-taking configuration |
| Deepgram | https://developers.deepgram.com/llms.txt | Flux streaming model events, model-integrated end-of-turn thresholds, eager end-of-turn cost |
| LiveKit | https://docs.livekit.io/llms.txt | Agents runtime boundary, telephony, turn-detection and interruption events, simulation-based testing |
| Twilio | https://www.twilio.com/docs | Conversation Relay, Media Streams, speech-model selection that moves turn detection provider-side |
| Temporal | https://docs.temporal.io/llms.txt | Deterministic workflows, activities, retries, signals, timers, worker versioning; deferred adapter |

These are design references, not dependencies selected by this document. The detailed findings and adopt/avoid decisions are in [upstream-patterns.md](docs/research/upstream-patterns.md).

The adopted MCP SDK is **`rmcp` 3.4.0** (`Apache-2.0`), pinned in `Cargo.toml`. Selection and supported protocol revisions are recorded in [MCP research](docs/research/integrations/mcp.md); it is a current dependency, not just an architecture reference.

## Dependency Intake

Before adding a dependency:

1. Verify the official package and repository.
2. Record selected version, license, maintenance status, advisories, features, transitive impact, and reason in the relevant research record or ADR.
3. Prefer a narrow feature set and disable unused defaults.
4. Add automated license and vulnerability checks.
5. Define how the dependency is removed or isolated if it becomes unavailable.

Original JARVIS code and documentation are licensed under [Apache-2.0](LICENSE), selected 2026-10-14 using the unmodified text from <https://www.apache.org/licenses/LICENSE-2.0.txt> (accessed 2026-10-14). [NOTICE](NOTICE) retains the bundled MediaPipe attribution and documents the geometry reduction. Apache-2.0 provides permissive reuse and a contributor patent grant and is compatible with the existing permissive dependency policy; separately licensed materials keep their terms.

The independently maintained website also uses Apache-2.0 for original code and includes license texts for its bundled React/React DOM/Scheduler (MIT), Three.js (MIT), Lucide (ISC and MIT) and MediaPipe (Apache-2.0) materials. This does not settle source rights for the removed prototype. Release archives now carry `LICENSE`, `NOTICE` and this provenance document; a generated dependency-license inventory, signing/notarization and clean-machine evidence remain open under `P9-017b`.

## HUD React runtime

React and React DOM 19.3.1 are bundled in `apps/jarvisd/src/hud/generated/controls.js`. Source: https://github.com/facebook/react. Their scheduler dependency carries the same MIT license. Build-only dependencies and exact versions are in `apps/jarvisd/ui/package-lock.json`.

```text
MIT License

Copyright (c) Meta Platforms, Inc. and affiliates.

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.

```

## Optional embedded speech

The `local-tts` feature includes ort 2.0.0-rc.12 (MIT OR Apache-2.0) and misaki-rs 0.6.0 (MIT), with its eSpeak feature disabled. Original Misaki pronunciation resources are Apache-2.0 (https://github.com/hexgrad/misaki). This is an independent Rust port, not historical JARVIS prototype code.

Kokoro-82M v1.0 weights (Apache-2.0) and ONNX Runtime 1.31.0 (MIT) are downloaded at first use, not bundled. The runtime LICENSE and ThirdPartyNotices.txt are kept with its verified native library in the profile cache. See [embedded speech research](docs/research/integrations/embedded-speech.md) for exact sources, pins and platform limits.

misaki-rs license:

```text
MIT License

Copyright (c) 2026 Michele Yin

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
```

ort MIT option:

```text
MIT License

Copyright (c) 2023-2026 pyke.io
Copyright (c) 2020 Nicolas Bigaouette

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
```

language-tokenizer 0.1.0 (transitive through misaki-rs), with an exact-version license exception:

        DO WHAT THE FUCK YOU WANT TO PUBLIC LICENSE
                    Version 2, December 2004

Copyright (C) 2026 Pavlov Danil <savannstm@gmail.com>

Everyone is permitted to copy and distribute verbatim or modified
copies of this license document, and changing it is allowed as long
as the name is changed.

            DO WHAT THE FUCK YOU WANT TO PUBLIC LICENSE

TERMS AND CONDITIONS FOR COPYING, DISTRIBUTION AND MODIFICATION

0. You just DO WHAT THE FUCK YOU WANT TO.


## Optional description-based voice runtime

VoxCPM 2.0.3 and OpenBMB VoxCPM2 are Apache-2.0:
https://github.com/OpenBMB/VoxCPM/tree/2.0.3 and
https://huggingface.co/openbmb/VoxCPM2/tree/32279effe8c19989596f05d353d1447f51d9e915 .

The optional Windows inference installation uses SoundFile 0.13.1 (BSD-3-Clause)
with its platform wheel's libsndfile DLL (LGPL-2.1-or-later). Its bundled license
is retained under `_soundfile_data/COPYING`; the runtime is installed separately
and is not linked into JARVIS executables. Upstream source and notices:
https://github.com/bastibe/python-soundfile/tree/0.13.1 and
https://github.com/libsndfile/libsndfile .
The optional Windows installer fetches wheels from their publishers and preserves
package license files in the private environment; it does not bundle that runtime
or model into the Rust executable. The inference dependency lock is
`scripts/voice-designer-windows.lock`; model digests are in
`crates/jarvis-voice/src/description_assets.json`. PyTorch 2.10.0 CPU is distributed
under its BSD-style license, with third-party notices inside its wheel. VoiceStudio
was an interaction reference only; no AGPL VoiceStudio application code was copied.

Optional Windows GPU installation uses the separately downloaded AMD ROCm 7.2.1
SDK packages and PyTorch 2.9.1 ROCm wheels, or official PyTorch 2.10.0 CUDA 12.8
wheels for NVIDIA. Publisher archives and SHA-256 digests are pinned in
`scripts/voice-designer-amd.lock`, `scripts/voice-designer-rocm.lock` and
`scripts/voice-designer-nvidia.lock`; common packages are pinned separately in
`scripts/voice-designer-common.lock`. Bundled AMD/LLVM and NVIDIA CUDA library
notices remain inside the installed packages. These dependencies are not embedded
in or redistributed with the Rust executable; GPU drivers are owner-installed.
See [runtime sources and acceptance](docs/research/integrations/voice-gpu.md).
