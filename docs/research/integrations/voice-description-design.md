---
integration: voice-description-design
status: live-verified on Windows CPU; other native platforms and GPU pending
last_verified: 2026-10-18
owners: []
selected_spec_version: VoxCPM2 revision 32279effe8c19989596f05d353d1447f51d9e915
selected_sdk: voxcpm 2.0.3; Python 3.12; CPU PyTorch
---

# Voice design from a description

## Owner correction

The owner clarified P8-024: type a natural-language description, generate a sample,
save that generated identity and select it for subsequent speech, as in ElevenLabs
and VoiceStudio. Kokoro preset mixing does not fulfill that requirement. Keep
P8-024 open until this path works with real audio. The earlier blend implementation
is a separate optional capability, not a substitute for description conditioning.

## Official sources accessed 2026-10-18 UTC

- https://raw.githubusercontent.com/debpalash/VoiceStudio/main/llms.txt and
  https://raw.githubusercontent.com/debpalash/VoiceStudio/main/llms-full.txt: 404.
- https://github.com/debpalash/VoiceStudio
- https://raw.githubusercontent.com/debpalash/VoiceStudio/main/docs/engines/voxcpm2.md
- https://raw.githubusercontent.com/debpalash/VoiceStudio/main/docs/engines/omnivoice.md
- https://raw.githubusercontent.com/debpalash/VoiceStudio/main/docs/engines/omnivoice-gguf.md
- https://raw.githubusercontent.com/OpenBMB/VoxCPM/main/llms.txt: unavailable.
- https://voxcpm.readthedocs.io/en/latest/llms.txt: unavailable.
- https://voxcpm.readthedocs.io/en/latest/quickstart.html
- https://voxcpm.readthedocs.io/en/latest/deployment/voxcpm_cpp.html
- https://voxcpm.readthedocs.io/en/latest/deployment/voxcpm_rs.html
- https://raw.githubusercontent.com/OpenBMB/VoxCPM/2.0.3/README.md
- https://raw.githubusercontent.com/OpenBMB/VoxCPM/2.0.3/pyproject.toml
- https://raw.githubusercontent.com/OpenBMB/VoxCPM/2.0.3/src/voxcpm/core.py
- https://raw.githubusercontent.com/OpenBMB/VoxCPM/2.0.3/src/voxcpm/model/voxcpm2.py
- https://github.com/OpenBMB/VoxCPM/releases/tag/2.0.3
- https://pypi.org/pypi/voxcpm/2.0.3/json
- https://huggingface.co/openbmb/VoxCPM2/raw/32279effe8c19989596f05d353d1447f51d9e915/README.md
- https://huggingface.co/api/models/openbmb/VoxCPM2?blobs=true
- https://pytorch.org/get-started/previous-versions/

## Contract and candidate

VoxCPM 2.0.3 is the latest published release observed. Its local Python API supports
description-conditioned generation with `(description)spoken text`, and reuse of
generated reference audio through `reference_wav_path`. There is no separate
`voice_description` argument. Output is 48 kHz mono. Load from an explicit local
directory with denoising and compilation disabled; select CPU explicitly for the
first Windows/AMD probe. No HTTP server is required. Supported CPU execution is
documented; measured Windows latency is recorded below.

The official C++/Rust alternative guides currently exclude VoxCPM2. VoiceStudio's
OmniVoice GGUF guide excludes voice design. Qwen3-TTS supports description design,
but its examined third-party Rust port labels itself experimental; it also needs
a separate Base model for reference reuse. These are not selected for this probe.
VoiceStudio is an interaction reference; no AGPL application code is copied.
VoxCPM code and model are Apache-2.0. Inspect and pin the minimal inference
dependencies separately; do not install its web UI, denoiser or training stack.

Model weights are about 4.96 GB. Published large-file SHA-256 digests are:

| File | Bytes | SHA-256 |
| --- | ---: | --- |
| model.safetensors | 4580080592 | f7f964cfa9da23653baec6e6f7750719977ad944ed9f95fe52fe3a620506891d |
| audiovae.pth | 376951122 | 94b5d51e107e0507d4acc976cfdadb64edd6fd06d1f751dadbf2fd1594274bf1 |

The SDK loads the AudioVAE with `torch.load(weights_only=True)`, the main weights
with safetensors and a local `LlamaTokenizerFast`. No remote tokenizer code is
needed. Pin and verify configuration/tokenizer files before inference too.
The SDK wheel SHA-256 is
`24da58a30d094a9e9a7ead450ae9cffda0d31eaeba620b61ad99179dd87e486b`.

## Proposed JARVIS boundary and tests

Rust retains validation, saved voices, authorization, routing and process lifetime.
A private local worker owns the Python model, with no listener, shell execution
or independent startup task. It must receive bounded requests over pipes, have
bounded concurrency, output and deadlines, and be killed on cancellation. This
extends process topology under [ADR-0249](../../adr/0249-description-designed-voices-retain-the-generated-reference.md).
Generation and ordinary use must prohibit downloads and omit raw SDK logs.
Install/download is separate from synthesis and explicit. No hosted credentials,
paid calls, remote synthesis, webhooks, scopes or pagination apply.

Save the chosen synthetic reference with its description, seed and pinned model;
do not merely resample a fresh identity from the description on every sentence.
Generated references are profile data, exported/deleted with the voice, unlike
shared public model files. Reject arbitrary paths and references from other profiles.
No human recordings, external cloning, fine-tuning or training are in this slice.

Before claiming completion: generate materially different descriptions with real
audio, preview through the browser, save/select/restart and synthesize different
text from the same reference. Test export/delete, invalid inputs, authentication,
tampered assets, cancellation and bounded worker lifetime. Record Windows timing
and memory; do not infer other native platforms or GPU acceleration from it.

## Installation and in-memory reference follow-up (2026-10-18)

The pinned SDK's public `tts_model.build_prompt_cache` accepts an in-memory
`BytesIO` as `reference_wav_path` through librosa 0.11.0. Its
`generate_with_prompt_cache` returns the waveform tensor, features and context.
Official source: the pinned `src/voxcpm/model/voxcpm2.py` above and
https://librosa.org/doc/0.11.0/generated/librosa.load.html . This avoids writing
transient recordings to disk during synthesis.

The installation uses existing uv 0.13.1 and CPython 3.12.15 to create a private
venv. Official uv pip semantics were checked at
https://docs.astral.sh/uv/pip/compatibility/ and
https://docs.astral.sh/uv/reference/cli/ . All inference wheels are version- and
hash-pinned; `--no-deps --require-hashes --only-binary :all:` intentionally omits
the upstream package's training, Gradio and denoising extras. Torch/torchaudio
2.10.0 CPU wheels come from https://download.pytorch.org/whl/cpu; other published
wheel digests come from https://pypi.org/pypi/<package>/<version>/json . The lock
is Windows x64 / CPython 3.12 specific, not a portable GPU environment claim.


## Live Windows evidence (2026-10-18)

The initial SDK feasibility probe and the production worker both passed on an
AMD Ryzen 9 9950X3D / 30.9 GiB Windows machine using CPU inference. The AMD GPU
was not used. Real browser acceptance ran with Edge 155.0.4283.62 and Playwright
1.62.1, creating a disposable profile and fixed, non-personal samples:

```powershell
node scripts/accept-voice-description.mjs --run-live --runtime C:/path/to/voxcpm2-2.0.3 --playwright C:/path/to/playwright/index.mjs
```

Evidence is retained locally at `target/voice-description-acceptance/profile-AHN5af`:
`verification.json`, desktop/mobile screenshots, exported recipe and five WAVs.
The first description generated a 3.04-second, 48 kHz mono PCM sample in 37.629
seconds including verified model loading. Browser playback reached its end.
A distinct female/American description generated different audio; subjective
accent/identity accuracy is not asserted solely from different bytes.

Saving and selecting for Console caused new text to synthesize in 14.784 seconds.
After restart, the exact generated reference was retained and speech reproduced
the same bytes (33.150 seconds including loading). After cancelling active
generation, a replacement worker synthesized successfully (33.461 seconds).
Early cancellation, export, deletion and deletion after restart also passed.
No live cloud request was needed. The separately installed permanent environment
uses the same hash-pinned inference wheels and verified model revision.

Fixture tests cover invalid WAV headers/revisions/settings, unsupported adapters,
missing installation, authentication, reference admission and cancellation IDs.
Independently removing the save and preview generated-reference guards made the
security test fail; both guards were restored. The actual Python worker also
refused a tampered local model before readiness with no raw SDK output. Browser
fixtures cover cancellation on navigation, late replies, generation/playback,
selection and saving the exact reference. Other native platforms, offline network
isolation at the OS level, GPU performance and perceptual evaluation remain open;
offline flags and verified local-only model paths are not an OS network sandbox.


## Degradation and removal

The runtime reports missing assets or incompatible versions on use without
installing anything or substituting a different identity. Normal speech can use
the configured fallback providers; design/preview never falls back. For removal,
choose another default/Console/backup voice, delete the affected designs, then
remove the `local-voxcpm2` provider and restart. Keep or remove the private runtime
and shared public model directory separately. Existing exports and profile
backups retain their generated references until the owner removes those copies.
The previous main configuration is backed up before activation.


## Startup correction found during activation

Installing a large Python environment beneath the managed Windows state folder
added substantial startup permission-maintenance work. The permanent installer
now places public runtime/model assets in `%LOCALAPPDATA%/JARVIS-VoiceDesigner`,
outside private profile directories. The CLI readiness budget is three minutes;
a competing start waits for the owning daemon, while genuine startup errors
still return immediately. The singleton lock remains enforced.


Main-profile activation also generated a real four-second sample in 30.444 seconds,
previewed embedded Kokoro, and confirmed all observed worker processes exited after
cancellation. Loaded worker RSS totalled 5,739,974,656 bytes (about 5.35 GiB).
The library and current voice assignments were unchanged. Evidence:
`target/voice-description-main-activation-initial.json`. The superseded installation is
retained at `%LOCALAPPDATA%/JARVIS-VoiceDesigner/superseded-20261018` for rollback;
it is not used by the provider. A relocated venv was not reused: the active runtime
was freshly installed from the same wheel lock at its final path.


The final release's concurrent-start acceptance passed against the main profile:
two `jarvis start --no-open` invocations exited successfully after 129.246 and
128.289 seconds, leaving one listening daemon. The losing startup attempt printed
that the profile was already starting and waited, exercising the user's reported
failure path. Evidence: `target/voice-startup-concurrent.json`. Startup on this
profile can still take around two minutes; the bounded wait accommodates it.
Final local checks: 3,305 workspace tests passed (8 opt-in ignored), all 120 CLI
tests passed after the final startup edit, strict all-feature Clippy, formatting,
documentation links and dependency policy passed. Plain `cargo build --release`
includes embedded Kokoro; the explicit no-default-features build was also checked.


The final relocated-runtime activation also passed in the running release profile:
embedded backup preview, a four-second generated sample, cancellation with all
observed workers exiting, and an unchanged voice library. Cold generation took
69.454 seconds and loaded worker RSS totalled 7,767,232,512 bytes (about 7.23 GiB).
These results are recorded in `target/voice-description-main-activation.json`.
Cold latency and memory therefore vary; the earlier 30–38 second measurements are
not a guaranteed startup budget. The UI's bounded operation permits three minutes.
A subsequent `jarvis start --no-open` immediately reported the existing listener.

## CORRECTION — relocated runtime reference playback (2026-10-18)

The earlier main-profile activation proved description generation but did not
exercise saved-reference speech in the newly installed runtime. Live acceptance
now found `soundfile`'s universal wheel in the Windows lock: it omits libsndfile,
so generation succeeds but reference loading fails. The earlier complete
reference-reuse acceptance used the research environment, which had the Windows
wheel. Do not treat that result as verification of the relocated installation.

Official sources checked again on 2026-10-18, before changing the installer:

- Repository-local [llms.txt](https://raw.githubusercontent.com/bastibe/python-soundfile/0.13.1/llms.txt): not found (404).
- [SoundFile 0.13.1 README](https://github.com/bastibe/python-soundfile/blob/0.13.1/README.rst): platform wheels bundle libsndfile; the universal/source wheel requires a system installation. SoundFile is BSD-3-Clause; libsndfile is LGPL.
- [Pinned loader source](https://github.com/bastibe/python-soundfile/blob/0.13.1/soundfile.py): loads `_soundfile_data/libsndfile_x64.dll` first on Windows, then tries the system library.
- [PyPI 0.13.1 release metadata](https://pypi.org/pypi/soundfile/0.13.1/json): `soundfile-0.13.1-py2.py3-none-win_amd64.whl`, 1,019,162 bytes, SHA-256 `1e70a05a0626524a69e9f0f4dd2ec174b4e9567f4d8b6c11d38b5c289be36ee9`. The previous `a23c...` hash selects `none-any` (25,751 bytes).

No authentication or network operation is needed for audio decoding. Pin the
Windows wheel and reinstall this same-version package to repair existing installs.
The installer must perform an in-memory WAV read through `librosa.load`, rather
than only importing its lazy top-level module. This loads the native decoder and
the reference path's resampling dependencies without running model inference.
Other native platforms still need their own wheels and acceptance.

## GPU inspection requested by the owner (2026-10-18)

The installed runtime reports `torch=2.10.0+cpu`, `cuda_build=null`,
`hip_build=null`, `gpu_available=false`, `device_count=0`. Windows reports an
AMD Radeon RX 9070 XT (driver 32.0.31041.1004) and integrated AMD Radeon Graphics.
The worker explicitly supplies `device='cpu'`; there is currently no automatic
GPU selection in JARVIS, including on NVIDIA hosts. Embedded Kokoro likewise
constructs the default ONNX Runtime CPU session.

Live official checks for the requested compatibility review:

- [VoxCPM 2.0.3 device contract](https://raw.githubusercontent.com/OpenBMB/VoxCPM/2.0.3/src/voxcpm/core.py): supports `auto`/`None`, CUDA, MPS and CPU; automatic choice prefers CUDA then MPS then CPU. JARVIS overrides that choice today.
- [AMD Windows support matrix](https://rocm.docs.amd.com/projects/radeon-ryzen/en/latest/docs/compatibility/compatibilityrad/windows/windows_compatibility.html): the documented ROCm 7.2.1 matrix lists RX 9070 XT, Windows 11, Python 3.12 and PyTorch 2.9. The page points to newer unified documentation for ROCm Core SDK 7.13.0; this is hardware eligibility evidence, not proof of this model's end-to-end compatibility.
- [PyTorch Windows installation](https://pytorch.org/get-started/locally/): NVIDIA acceleration requires a matching CUDA-enabled PyTorch installation and runtime availability verification.

This pass checks the current implementation; it does not install GPU drivers or
replace the pinned CPU runtime. Automatic GPU selection requires a separate
runtime contract, compatible per-platform locks, observable device selection and
real generation/reference-reuse tests with CPU fallback. NVIDIA and AMD inference
performance are unverified here; no GPU speedup is claimed.

## Settings repair acceptance (2026-10-18)

The original main-profile Generate request reproduced the reported setup error in
5 ms, while the configured runtime files existed. Restarting the daemon restored
generation (32.987 seconds); the exact cause of the old process's stale setup
failure is unconfirmed. The editor previously displayed the error below all
routing controls. It now presents setup readiness, elapsed time, errors and the
sample player beside Generate, and gates unavailable installations. Incomplete
drafts are reused and excluded from routing choices; advanced settings are
collapsed; one action saves and selects the voice for Console.

After installing the corrected Windows decoder wheel, the full description
acceptance passed in `target/voice-description-acceptance/profile-aoMMwn`:
generation and playback, saved-reference new-text speech, exact reuse after
restart, export, deletion, cancellation/recovery and reference admission guards.
An injected setup failure confirmed inline error visibility and retry; the real
generation took 45.193 seconds while other checks ran. Desktop/mobile screenshots
were inspected. The installer also passed an actual in-memory reference decode
and resample (`target/voice-ux-runtime-repair.log`).

Existing voice editing passed browser/audio/restart acceptance with Kokoro in
`target/voice-settings-acceptance/profile-3Wx3jL`, including speed controls inside
the advanced disclosure, saving Console selection and mobile layout. UI and
documentation fixtures pass (148 tests). Main-profile verification is recorded
separately after the release update.

Final release activation passed: plain `cargo build --release`, 3,307 workspace
tests (8 opt-in ignored), strict all-feature Clippy, formatting and documentation
links. The running main profile generated a four-second sample in 28.539 seconds,
then spoke different text using that exact reference. Embedded Kokoro preview and
cancellation passed; the canonical library was unchanged. The final release UI
also passed readiness, CPU-label, draft reuse/routing exclusion and desktop/mobile
checks. Evidence: `target/voice-description-main-activation.json`,
`target/voice-ux-main-final.log`, `target/voice-ux-main-ui.json` and screenshots.
