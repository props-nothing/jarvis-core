# Third-Party Research And Provenance

This project studies public architecture and documentation but does not import third-party source code by default. Before copying or adapting code, verify the exact file's license, notices, compatibility with the intended JARVIS license, and attribution requirements.

## Bundled Assets

One third-party asset is bundled in the product, with its licence verified against the upstream repository (not the prototype):

| Asset | Where | Source and licence |
| --- | --- | --- |
| Canonical face mesh (468 vertices, 898 triangles; positions and triangle indices only, as integers) | `apps/jarvisd/src/hud/head.js`, served by the console | MediaPipe `canonical_face_model.obj`, **Apache License 2.0**, Copyright 2019-2023 The MediaPipe Authors, <https://github.com/google-ai-edge/mediapipe>. Research record: `docs/research/integrations/mediapipe-canonical-face-model.md`. The attribution is in the header of `head.js` and a test keeps it there. |

Rust dependencies are checked against the licence allowlist by `cargo deny` in CI; a shipped archive also needs a generated list of
their licences before a public release (`P9-017`).

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