---
integration: embedded-speech
status: live-verified
last_verified: 2026-10-18
owners: []
selected_spec_version: Kokoro v1.0, ONNX Runtime C API 24
selected_sdk: ort 2.0.0-rc.12; misaki-rs 0.6.0
---

# Embedded English speech (P8-022)

## Scope

Run Kokoro-82M inside the Rust daemon on CPU, without Python or a speech server.
Download a fixed, verified model/runtime on first use; reuse it without network.
Cloning, multilingual synthesis, arbitrary models and GPU execution are out of scope.

## Official sources

All sources below were accessed on 2026-10-18. The sherpa-onnx repository root,
misaki-rs repository root and ort.pyke.io `llms.txt` endpoints returned not found;
versioned source and official model cards provided the controlling contracts.

| Source | URL/version | Purpose |
| --- | --- | --- |
| Initial candidate | https://k2-fsa.github.io/sherpa/onnx/tts/pretrained_models/kokoro.html | Kokoro engine discovery |
| Licensing defect | https://github.com/k2-fsa/sherpa-onnx/issues/3731 | Maintainer confirms bundled GPL eSpeak; removal planned for 2.0 |
| Binding source | https://github.com/pykeio/ort/tree/v2.0.0-rc.12 | Safe Rust API; supports runtime API >=24 |
| Binding package | https://crates.io/api/v1/crates/ort/2.0.0-rc.12/download | Inspected session, tensor, cancellation and dynamic loader implementations |
| Runtime release | https://github.com/microsoft/onnxruntime/releases/tag/v1.31.0 | Current security/reliability changes and native CPU packages |
| Runtime digests | https://api.github.com/repos/microsoft/onnxruntime/releases/tags/v1.31.0 | Published SHA-256 and byte sizes |
| Phonemizer source | https://github.com/MicheleYin/misaki-rs | MIT Rust port, English US/GB; default espeak feature must be OFF |
| Phonemizer package | https://crates.io/api/v1/crates/misaki-rs/0.6.0/download | Actual constructor takes Language, unlike older README example |
| Original phonemizer | https://github.com/hexgrad/misaki | Apache-2.0 pronunciation resources |
| Original model | https://huggingface.co/hexgrad/Kokoro-82M | Apache-2.0 model provenance |
| ONNX model contract | https://huggingface.co/onnx-community/Kokoro-82M-v1.0-ONNX | Tensor shapes, voice rows, 24 kHz output |
| Pinned model tree | https://huggingface.co/api/models/onnx-community/Kokoro-82M-v1.0-ONNX/tree/1939ad2a8e416c0acfeecc08a694d14ef25f2231?recursive=true&expand=true | LFS SHA-256, sizes and exact revision |

## Verified contract

### Operations and transport

Model revision `1939ad2a8e416c0acfeecc08a694d14ef25f2231`:

| File | Bytes | SHA-256 |
| --- | ---: | --- |
| onnx/model_quantized.onnx | 92361116 | fbae9257e1e05ffc727e951ef9b9c98418e6d79f1c9b6b13bd59f5c9028a1478 |
| voices/af_heart.bin | 522240 | d583ccff3cdca2f7fae535cb998ac07e9fcb90f09737b9a41fa2734ec44a8f0b |
| voices/bm_george.bin | 522240 | c4b235a4c1f2cd3b939fed08b899ce9385638b763f7b73a59616c4fc9bd6c9bc |
| tokenizer.json | 3497 | 77a02c8e164413299b4b4c403b14f8e0e1c1b727db4d46a09d6327b861060a34 |

`input_ids` is int64 `[1, N+2]`, zero-padded at both ends. `style` is float32
`[1,256]`, the Nth voice row; `speed` is float32 `[1]`. Voice files are little
endian floats and contain 510 rows: JARVIS caps each phoneme segment at 500 tokens,
avoiding the model card's ambiguous inclusive 510-token example. Audio is float32
mono at 24000 Hz, encoded by JARVIS as signed 16-bit PCM WAV.

### Authentication and authorization

Public HTTPS GETs only, no key, account, callbacks or webhooks. Owner configuration
opts into downloads. `allow_download = false` forbids downloading missing assets.
URLs, versions, sizes and SHA-256 are compiled in; configuration cannot supply
executable paths, model URLs or replacement hashes. No transcript is sent to a
download endpoint. No external speech protocol or listening socket is involved.

### Limits and failure semantics

Retain the existing 1500-character, 8 MiB audio and 25-second request bounds.
One synthesis per adapter, two queued lookahead requests, two CPU inference threads, no unbounded work queue.
First preparation can take longer than a speech request; a shared background
initialization survives the caller timing out, so later calls use the ready model.
Live doctor can wait for preparation. Failed initialization is retryable; corrupted
cached files fail closed. Cancellation terminates ONNX inference; no partial WAV
is exposed, allowing fallback before any audio has been delivered.

### Data and compliance

Only fixed public asset URLs leave the host during preparation. Text, PCM and voice
selection stay in process; no transcript files or raw native diagnostics are logged.
ONNX telemetry is disabled before creating a session. Cache contains reusable public
assets, not personal speech. ort is MIT/Apache-2.0; ONNX Runtime is MIT; misaki-rs is
MIT and its original resources Apache-2.0; Kokoro weights are Apache-2.0. Preserve
runtime LICENSE/ThirdPartyNotices alongside extracted native libraries.

### Versions and deprecations

Pin ort 2.0.0-rc.12, misaki-rs 0.6.0 (default features OFF), ONNX Runtime 1.31.0.
ort's inspected loader negotiates C API 24 and accepts newer runtimes; the pinned
combination requires live verification. 1.24.2 was inspected initially, then
superseded by current 1.31.0 to include later runtime validation/security fixes.
No CUDA/DirectML/plugin provider is enabled. Release assets cover Windows x64/ARM64,
Linux x64/ARM64, macOS ARM64. macOS Intel is explicitly unsupported for this pin.

## JARVIS mapping and decisions

Optional `local-tts` cargo feature; `kind = "embedded-kokoro"` implements the existing
SpeechProvider and returns `audio/wav`. `af_heart` (US) and `bm_george` (GB) are the
initial voices. Cache is under AppPaths.cache. Validate archives before extracting
only fixed regular members, and compare native library bytes with the verified
archive before loading. Hash model bytes before passing those same bytes to ONNX.
Assets are fetched on first use, never bundled in releases. Pronunciation dictionaries
are library resources; out-of-vocabulary spelling can sound less natural than eSpeak.

## Rejected alternatives

Dependency intake also inspected `language-tokenizer` 0.1.0, reached through
misaki-rs. Its published LICENSE.md and
https://raw.githubusercontent.com/savannstm/language-tokenizer/master/LICENSE.md
(accessed 2026-10-18) specify WTFPL v2, a permissive license without copyleft
conditions. It has a license-file field instead of an SPDX field. Add an exact
0.1.0 exception in deny.toml, not a global allow-list expansion, and retain its
copyright/license in THIRD_PARTY.md. The exception syntax was checked against
https://embarkstudios.github.io/cargo-deny/checks/licenses/cfg.html for cargo-deny
0.20.2. Audit with `cargo deny --all-features check`, since a default-feature audit
does not inspect the optional speech dependency graph.

sherpa-onnx 1.13.8 unconditionally links eSpeak/piper in its inspected native build.
The maintainer's licensing issue blocks adopting that binary under the repository's
permissive dependency policy. Do not weaken that policy or claim the entire stack
is Apache-2.0. Kokoro-FastAPI remains a supported separate-server adapter, but cannot
satisfy this slice. No historical prototype code is used.

## Verification plan

Discriminating test: generate non-silent, decodable WAV with a real pinned model,
no speech server, then repeat with downloads forbidden. Exercise native browser
WAV playback and a failed primary followed by this adapter. Mutate cached model,
voice and runtime bytes and prove refusal before inference; reject oversized data,
symlinks and unexpected archive members. Verify cancellation and input/output bounds.
Run baseline formatting, strict Clippy, workspace tests and dependency license checks.
Measure cold/warm first audio, CPU time and peak resident memory on native hosts.

## Unresolved questions

Actual disconnected-network acceptance and performance on each of Windows, macOS
and Linux need native execution evidence. Cross-compilation alone does not satisfy
P8-022. English names and uncommon words may need future pronunciation controls.

## Verification log

| Date | Versions | Evidence | Researcher |
| --- | --- | --- | --- |
| 2026-10-18 | Versions above | Inspected published crate sources, official model tree and runtime release; rejected GPL-linked candidate | Codex |

## Windows live evidence (2026-10-18)

Windows x64, AMD Ryzen 9 9950X3D (16 cores/32 logical processors), two inference
threads. The release daemon generated actual 24 kHz / 16-bit WAV with the separate
Kokoro server stopped and `allow_download = false`. A primary HTTP 503 selected
the embedded provider; the next request respected cooldown. Installed Edge
155.0.4283.62 played both console WAV segments to `ended` without errors (6.425 s
and 10.95 s), with the backup notice visible. The disposable daemon shut down
gracefully. Local evidence: `target/speech-embedded-acceptance/console-WInBXq/verification.json`.

| Request | First audio | Daemon CPU time | Peak daemon working set |
| --- | ---: | ---: | ---: |
| Cold cached initialization plus sentence | 2333 ms | 2859 ms | 398196736 bytes (380 MiB) |
| Warm same sentence | 709 ms | 1422 ms | 421224448 bytes (402 MiB) |

The measured sentence was “JARVIS can speak without a separate speech server.”
Both requests returned 181244 bytes. Measurements include daemon overhead, were
taken while other build/test work ran, and are not a general latency guarantee.
The ignored real-model Rust test also passed cold preparation, non-silent WAV,
real connection-failure fallback, three concurrent lookahead requests, cancellation
followed by new speech, and a fresh adapter with downloads prohibited. Initial
asset download/preparation took about 10.3 s in the first test run.

The host network was not disconnected. These checks prove server-free synthesis
and download-disabled reuse, not OS-level isolation from all network traffic.
Native macOS/Linux execution, disconnected-network proof and corresponding
performance measurements remain unverified, so P8-022 stays unchecked. Official
Linux x64/ARM64 and macOS ARM64 archive digests and exact library/license members
were inspected; this is packaging evidence only, not native execution evidence.

Both `af_heart` and `bm_george` passed the real-model test, including lookahead,
cancellation and fresh offline adapters. Validation: `cargo fmt --check`, strict
workspace/all-target/all-feature Clippy, `cargo test --workspace` (3282 passed,
7 opt-in ignored), feature-enabled voice/storage/daemon unit tests (1331 passed,
6 opt-in ignored), 162 relevant Node tests, documentation links and
`cargo deny --all-features check` (advisories, bans, licenses and sources) passed.
Existing native Windows/macOS/Linux CI runs the feature-enabled Rust suite;
the real-model and browser acceptance harnesses remain explicit opt-ins.

### Main-profile activation

After the owner's explicit activation approval on 2026-10-18, the main profile
retained ElevenLabs first and changed its `local` backup to `embedded-kokoro`,
`af_heart`, with downloads prohibited. Its previous configuration was backed up.
The separate Kokoro server was stopped and its Windows sign-in task disabled.
The main daemon returned real WAV from the embedded backup (1351 ms cold,
401 ms warm); independent decoding confirmed non-silent 24 kHz mono audio
(2.55 and 3.25 seconds). A default request still selected ElevenLabs and returned
MP3 in 308 ms. Verified no listener on port 8880 and disabled old startup settings.
Local evidence: `target/speech-embedded-activation.json`. This verifies activation
without a separate server, not a reboot or OS-level network-disconnection test.
