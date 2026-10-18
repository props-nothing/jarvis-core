---
integration: voice-gpu
status: live-verified on Windows AMD with CPU fallback; NVIDIA hardware pending
last_verified: 2026-10-18
selected_sdk: VoxCPM 2.0.3; AMD PyTorch 2.9.1 ROCm 7.2.1; NVIDIA PyTorch 2.10.0 CUDA 12.8
---

# Local voice GPU execution

## Official sources checked before implementation, 2026-10-18

- https://rocm.docs.amd.com/en/latest/llms.txt (unavailable)
- https://raw.githubusercontent.com/OpenBMB/VoxCPM/2.0.3/llms.txt (unavailable)
- https://rocm.docs.amd.com/projects/radeon-ryzen/en/latest/docs/install/installrad/windows/install-pytorch.html
- https://rocm.docs.amd.com/projects/radeon-ryzen/en/latest/docs/compatibility/compatibilityrad/windows/windows_compatibility.html
- https://repo.radeon.com/rocm/windows/rocm-rel-7.2.1/
- https://docs.pytorch.org/docs/2.9/notes/hip.html
- https://docs.pytorch.org/docs/2.10/notes/cuda.html
- https://pytorch.org/get-started/previous-versions/
- https://download.pytorch.org/whl/cu128/torch/
- https://download.pytorch.org/whl/cu128/torchaudio/
- https://raw.githubusercontent.com/OpenBMB/VoxCPM/2.0.3/src/voxcpm/core.py
- https://raw.githubusercontent.com/OpenBMB/VoxCPM/2.0.3/src/voxcpm/model/voxcpm2.py
- https://raw.githubusercontent.com/OpenBMB/VoxCPM/2.0.3/src/voxcpm/model/utils.py

The selected AMD release is deliberately pinned, not the newest release. Its
Windows matrix lists RX 9070 XT, Windows 11 and CPython 3.12. The installation
guide requires driver 26.2.2; this host has Adrenalin 26.8.1, RX 9070 XT driver
32.0.31041.1004 and a second integrated Radeon adapter. AMD distributes the ROCm
SDK core, development and custom libraries, a small rocm Python wrapper, and
torch/torchaudio 2.9.1+rocm7.2.1. Exact wheel hashes will be recorded in locks after
retrieval from the official archive. NVIDIA uses official cu128 Windows cp312
torch/torchaudio 2.10.0 wheels. The existing 2.10.0+cpu environment is retained.

HIP exposes the same torch.cuda API and cuda:index device names as NVIDIA CUDA;
torch.version.hip distinguishes AMD. Availability must be tested at runtime with
a tensor operation, then actual model inference. Prefer the adapter with most
dedicated memory, rather than blindly selecting device zero. SDK device selection
supports indexed CUDA devices. Do not enable compilation or assume FlashAttention
support. Keep model revision 32279effe8c19989596f05d353d1447f51d9e915 and the
verified Windows SoundFile decoder from the description-design integration.

## Contract and limits

Installation is explicit, public HTTPS download without credentials. No hosted
API, scopes, billing, pagination or rate-limit contract applies. Offline synthesis
starts a daemon-owned pipe worker; no server or startup task. GPU packages live in
a separate environment beside the CPU environment and share verified model files.
Rust owns bounded metadata validation, cancellation and a single CPU retry if GPU
startup or synthesis fails. No partial audio is emitted before fallback. Metadata
reports actual backend, device and allocated GPU memory; hardware detection alone
must never be presented as GPU execution. The selected backend does not change
saved references or their admission rules. Exact audio can differ across devices.

Unresolved before acceptance: AMD kernel/model compatibility, memory/latency and
reference decoding on this host; NVIDIA hardware unavailable; other OS installers
and Intel/Apple acceleration outside this Windows slice. Verify description
generation, saved-reference new-text synthesis, restart, cancellation and forced
CPU fallback end to end. NVIDIA selection/failure behavior requires fixtures and
must not be described as hardware-tested here.

## Installed runtime and first hardware probe

The AMD archive was downloaded over official HTTPS; exact SHA-256 values are now
in `scripts/voice-designer-amd.lock` and `scripts/voice-designer-rocm.lock`.
The source-only ROCm loader (15,940 bytes, SHA-256
`9084902eaa69213a00a90784ad89e6e5fe73c702df0cc6cc3a70d777c7a6142b`)
uses the already pinned setuptools 78.1.0 with no isolated build dependency
download. Its wheel dependencies are installed first. Torch requires sympy>=1.13.3,
which the existing common 1.14.0 pin satisfies. Torchvision is not required by
VoxCPM inference and is not installed. The AMD torch wheel hash is
`e88bf270163b48f7f27f7ea3db5ffb3be4ba107301933022bcb3c6ddedfeeabb`.
NVIDIA hashes come directly from the official cu128 package index and are stored
in `scripts/voice-designer-nvidia.lock`. Common dependencies and the Windows
decoder are shared by all three locks; no unpinned resolver upgrades are allowed.

The installer completed against this host with `-Backend AMD`, including native
reference decode/resample and a GPU matrix calculation. It reported
`torch=2.9.1+rocm7.2.1`, HIP `7.2.53211-158bd99533`, `cuda:0`,
`AMD Radeon RX 9070 XT`, 17,095,983,104 bytes of device memory. The integrated GPU
was not exposed by this runtime. Model loading selected bfloat16 on that device.
A real pipe-worker probe generated a 3.36-second sample, then a 3.04-second
different sentence conditioned on that reference. Peak PyTorch GPU allocation
reached 5,993,031,680 bytes. Cold loading took 19.656 seconds; first generation
27.079 seconds and first reference reuse 36.656 seconds. These are observed
first-use figures, not a speedup guarantee. Evidence is in ignored
`target/voice-gpu-research/worker-probe.log` and `installer-acceptance.log`.

An initial browser run confirmed actual GPU metadata, playback, saving, export,
new-text speech and preserved reference across restart. Its old CPU-only exact
PCM-hash assertion failed on GPU: kernels can produce different waveforms even
with the same seed and reference. The GPU acceptance now requires exact persisted
reference equality, valid audible output and the expected execution backend;
CPU keeps its additional exact-waveform regression assertion. This does not claim
perceptual identity evaluation or bit-identical GPU output. Full lifecycle and
forced-fallback acceptance are recorded below.

## Full Windows AMD acceptance, 2026-10-18

`scripts/accept-voice-description.mjs --run-live --expect-backend amd
--test-cpu-fallback` passed against the installed runtime, using Edge
155.0.4283.62 / Playwright 1.62.1. Evidence:
`target/voice-description-acceptance/profile-vUzhUl/verification.json` and
`target/voice-gpu-research/acceptance-complete.log`.

- Browser description generation, sample playback to completion, save and Console
  selection, export and new-text speech passed. The UI displayed RX 9070 XT and
  GPU allocation; desktop/mobile screenshots were inspected.
- A second description produced different real audio. Reference injection and
  unauthenticated routes failed closed. The new runtime-status route omits the
  library and sends `Cache-Control: no-store`.
- Restart retained the exact saved reference and produced new speech. Cancelling
  generation stopped its worker; the next synthesis recovered on AMD.
- Terminating only the disposable daemon's Python worker caused the next request
  to succeed on CPU with the same saved reference, in 35.807 seconds.
- Restarting that disposable daemon with HIP/CUDA devices masked caused GPU
  startup to fail and CPU synthesis to succeed in 45.456 seconds. The next request
  stayed on CPU (7.518 seconds). CPU output matched between these fallback paths.
  The saved library remained exactly equal throughout both tests.
- Early cancellation and deletion/restart passed. The disposable daemon stopped.

Observed initial GPU generation took 46.574 seconds including startup, producing
2.88 seconds of audio; warm saved-reference speech took 9.703 seconds. PyTorch
allocation was 5,572,731,904 bytes with a 5,785,360,384-byte peak for that sample.
Cold restart speech took 42.701 seconds. Build/tests were running concurrently;
these are acceptance timings, not a controlled GPU-vs-CPU benchmark.

Selection fixtures cover AMD, NVIDIA, missing GPUs, failed kernels/drivers,
fallback to a second compatible adapter and CPU without touching a GPU runtime.
Rust tests cover metadata bounds/backend rejection, GPU timeout/error cleanup,
single sticky CPU retry, no retry on cancellation/success and no repeated CPU
retry. Formatting, all-feature strict Clippy and all 3,310 workspace tests pass
(8 opt-in ignored); 149 UI/documentation tests and 4 Python selection tests pass.
Plain `cargo build --release` succeeds with embedded Kokoro still included.
NVIDIA device selection is implemented and fixture-tested, not hardware-verified.

The release was activated in the main profile and verified with actual AMD
generation and new-text reference reuse, embedded Kokoro backup preview and the
GPU device label on desktop/mobile. A warm sample took 5.416 seconds for 4.64
seconds of audio. The timed cancellation check was tightened to stop a longer
request at 150 ms; the observed worker tree exited. The earlier one-second stop
check was inconclusive and is not counted as evidence of worker cancellation.
The main library and assignments remained unchanged. Evidence:
`target/voice-gpu-main-activation.json`, `target/voice-gpu-main-ui.json`,
`target/voice-gpu-research/main-smoke-final.log` and main desktop/mobile screenshots.
