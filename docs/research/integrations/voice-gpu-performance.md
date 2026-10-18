---
integration: voice-gpu-performance
status: implemented; Windows AMD release acceptance and main activation verified
last_verified: 2026-10-18
selected_sdk: VoxCPM 2.0.3; torch 2.9.1+rocm7.2.1 on Windows RX 9070 XT
---

# Voice latency review

## Selected queue repair contract, 2026-10-18

The owner reports intermittent 429 fallback and speech cut off as new text arrives.
Before changing playback, rechecked the official Web Audio 1.1 specification
https://www.w3.org/TR/webaudio/#AudioBufferSourceNode,
the Web Speech API https://webaudio.github.io/web-speech-api/#tts-section and
HTTP Retry-After https://www.rfc-editor.org/rfc/rfc9110.html#name-retry-after.
Accessed 2026-10-18. Browser APIs are built in; no package/runtime version changes.
Audio source end means scheduled playback ended, not HTTP EOF; speech synthesis
queues utterances, while cancel removes them. Retry-After expresses delay seconds
or an HTTP date. Existing bearer authentication and local speech limits remain.

Code review found that sentence lookahead advances when the first PCM arrives,
then admits a fourth request while three local worker slots are still occupied.
Local saturation is incorrectly classified as a provider/account rate limit and
opens the provider circuit breaker. Browser fallback invalidates the session, so
later model deltas disappear; browser speech also truncates at 1,800 characters.
A delayed wake-listening callback checks speaking but not preparation/queued work,
so it can cancel a newly started or between-sentences speech session.

Repair: keep at most one unfinished synthesis request per Console session, retain
bounded lookahead after synthesis EOF, and advance playback only once per item's
actual end. Local Busy returns 503 with Retry-After and does not poison provider
health or switch voices; bounded cancellable retries happen before audio only.
Browser fallback keeps the same text queue and accepts later deltas, without the
1,800-character truncation. Automatic wake listening waits until the speech queue
ends. Explicit Stop, microphone, new user message and navigation still cancel.
No model/provider SDK call is changed. Test slow streamed multi-line replies,
local saturation/recovery, 429 then success, late text after browser fallback,
duplicate completion callbacks and Stop during backoff, plus real AMD playback.
This cannot guarantee a model pronounces every word or recover word alignment
after a genuinely broken partial audio stream; no automatic partial replay is added.

### Queue repair verification

The Windows release passed `scripts/accept-voice-queue.mjs` against the RX 9070 XT.
Evidence: `target/voice-queue-acceptance/profile-AA50p8/verification.json` and
`target/voice-queue-live-final.log`. The model text stream is synthetic and timed;
successful synthesis and browser Web Audio playback use the real daemon and GPU.

- Nine consecutive reply segments produced nine playback contexts and 313 audio
  buffers. Every generated PCM sample was scheduled and drained, each segment
  waited for the previous one, only one synthesis request was unfinished at a
  time, and no source was stopped or sent to browser fallback.
- Two controlled 503 Busy responses recovered in the same selected AMD voice;
  a controlled 429 did likewise. Their later lines remained intact. Actual
  overlapping preparation requests also returned 503 with Retry-After: 1 and
  then completed real AMD preparation. Provider-local saturation/health behavior
  is additionally covered by Rust admission and circuit-breaker tests.
- A controlled provider failure followed by later model deltas preserved more
  than 1,800 characters through 24 browser utterances. Browser synthesis events
  were simulated for this case; this proves text retention and completion
  handling, not operating-system speech quality. Stop during backoff sent no
  further synthesis request and started no browser voice.
- No browser errors occurred. The disposable daemon and its worker exited.

All 3,316 workspace tests passed (eight opt-in tests ignored), as did strict
all-target/all-feature Clippy, formatting and 158 JavaScript/documentation tests.
The browser regressions include slow HTTP EOF versus playback, duplicate end
callbacks, MP3 completion, long unpunctuated text, bounded retry, Stop and the
delayed wake-listening race. No cloud request, model/runtime upgrade, or changes
to the owner's library were needed. The first harness attempt
`profile-uKK1no` omitted the required shutdown configuration and exited before
running the tests; it is retained as a failed attempt, not a passing result.

The verified release was activated in the main profile and passed real AMD
preparation, unsaved design generation and new-text preview. Configuration and
voice-library file hashes match their values before shutdown. The main daemon
remains running with the AMD model ready. Evidence:
`target/voice-queue-main-activation.json`, `target/voice-queue-main-before.json`
and `target/voice-queue-main-activation.mjs`. Existing browser tabs must reload
to use the new queue code. Genuine provider failures may still use fallback;
explicit user interruption remains available.

The owner reports slow GPU generation and requests an online review. The working
GPU installation and fallback acceptance are recorded in [voice-gpu.md](voice-gpu.md).
This review measures model loading, warm generation, reference encoding and first
streamed audio separately. Do not equate faster-than-realtime throughput with
first-audio latency or assume local execution beats an optimized cloud service.

## Primary sources accessed before experiments, 2026-10-18

Previously checked repository llms.txt locations for VoxCPM and AMD were absent.
The versioned SDK and current official guides were opened again:

- https://raw.githubusercontent.com/OpenBMB/VoxCPM/2.0.3/README.md
- https://raw.githubusercontent.com/OpenBMB/VoxCPM/2.0.3/src/voxcpm/core.py
- https://raw.githubusercontent.com/OpenBMB/VoxCPM/2.0.3/src/voxcpm/model/voxcpm2.py
- https://voxcpm.readthedocs.io/en/latest/usage_guide.html
- https://voxcpm.readthedocs.io/en/latest/reference/api.html
- https://rocm.docs.amd.com/projects/radeon-ryzen/en/latest/docs/compatibility/compatibilityrad/windows/windows_compatibility.html
- https://rocm.docs.amd.com/projects/radeon-ryzen/en/latest/docs/limitations/limitationsrad.html
- https://www.amd.com/en/resources/support-articles/release-notes/RN-AMDGPU-WINDOWS-PYTORCH-7-2-1.html
- https://elevenlabs.io/docs/eleven-api/guides/how-to/best-practices/latency-optimization
- https://elevenlabs.io/docs/overview/capabilities/text-to-speech
- https://raw.githubusercontent.com/triton-lang/triton-windows/readme/README.md
- https://raw.githubusercontent.com/ROCm/TheRock/main/RELEASES.md
- https://raw.githubusercontent.com/debpalash/VoiceStudio/main/docs/performance.md

VoxCPM2 is a 2B diffusion/autoregressive voice-design model. Its published RTX4090
RTF figures (~0.3 standard, ~0.13 Nano-vLLM) are vendor benchmarks on NVIDIA, not
evidence for this Windows AMD setup. The installed model uses BF16 for its language
model and FP32 for AudioVAE as upstream specifies. No ASR/Whisper model runs in the
reference path. Windows AMD GPU support exists; the generic claim that Windows
PyTorch is NVIDIA-only is obsolete for this supported card/runtime.

The pinned SDK supports inference_timesteps (default10, documented range4–30),
prompt-cache reuse, and streaming with generate_with_prompt_cache_streaming.
Lower steps trade quality for compute; no quality-equivalence claim is justified
without listening/comparison. Keep deployed defaults unchanged during measurement.
The optimize path requires Triton and torch.compile; the current pinned runtime
does not include Triton. The SDK checks device==cuda, so an indexed cuda:0 device
also needs careful handling before assuming optimize=True enabled compilation.
Do not apply NVIDIA TensorRT recipes to this AMD model or silently install a
different runtime. Further compiler/serving changes require their own pinned
dependency and compatibility checks.

Experiments use the same verified local model and synthetic generic test audio,
without cloud requests, credentials, listener ports or changes to saved voices.
No authentication, billing, pagination or external API rate limits apply. Measure
steps10/6/4 with repeated warm runs and streaming first-audio/total latency. Keep
model-hash verification and cold load costs separate. Existing cancellation kills
the worker for correctness, so the following request pays cold startup again.

## Code review and selected implementation

### Selected implementation contract, 2026-10-18

Before implementation, re-opened the versioned VoxCPM2 2.0.3 SDK and API reference
above; repository-local `llms.txt` returned 404. Browser authority:
https://www.w3.org/TR/webaudio/#AudioBufferSourceNode (Web Audio 1.1, accessed
2026-10-18). `AudioBufferSourceNode.start(when)` schedules PCM on the context
clock; each source is single-use and stoppable. AudioContext resume requires the
browser's user activation policy. Fetch readers deliver arbitrary byte boundaries,
so PCM samples/NDJSON records must be reassembled and bounded before playback.

Use `generate_with_prompt_cache_streaming` from the pinned SDK, default ten steps,
explicit four/six-step alternatives and the existing offline model. Warm both
unconditioned generation and synthetic-reference encoding/streaming; keep BF16,
FP32 AudioVAE, CFG 2, no ASR, no denoiser and no compilation/dependency change.
No external authentication, billing or network request is added. Existing bearer
authentication, preview admission and cancellation apply to preparation/streaming.
The HTTP/pipe contracts and required falsification/live tests are recorded in
[ADR-0251](../../adr/0251-local-voice-streams-retain-admission-until-completion.md).
Quality equivalence and native NVIDIA timings remain unresolved, not inferred.

### Findings before this implementation

- `crates/jarvis-voice/src/description_worker.py` loads lazily, keeps its model
  resident after successful requests, and caches the most recent reference's
  encoded features. It does not load Whisper, run transcription, or reload weights
  for every successful request. The language model already uses BF16.
- The worker uses `generate_with_prompt_cache` with ten diffusion steps and
  `optimize=False`. It sends a complete WAV after inference. In
  `crates/jarvis-voice/src/description.rs`, `open` awaits that render before it can
  expose any audio. `apps/jarvisd/src/voice_library.rs` also collects the complete
  preview. Switching only the SDK call would not enable browser streaming.
- Saved voice adapters share the original worker through an `Arc`; selecting a
  configured voice does not construct another model. Only the most recent encoded
  reference is retained, so alternating voices requires reference encoding again.
- Cancellation deliberately drops the owned worker. A later request must reload
  and warm it. Preserve cancellation and CPU fallback correctness in any future
  attempt to keep a cancelled worker alive.
- Triton is absent in the installed GPU environment. The upstream Windows fork
  now documents AMD/RDNA4 support and a PyTorch/Triton version matrix (2.9/3.5),
  but this alone does not establish compatibility with our pinned AMD 7.2.1 wheels
  or VoxCPM2 graphs. Do not interpret `optimize=True` as a verified acceleration
  switch. No compiler, runtime upgrade, driver or new dependency was installed.

VoiceStudio's performance page does contain the Windows NVIDIA-only statement
quoted by the owner. Treat that as a limitation of its documented package path,
not a universal PyTorch limitation: AMD's support matrix and this machine's live
execution contradict that generalization. F5-TTS diffusion/TensorRT and Parler-TTS
compiler measurements are not measurements of this VoxCPM2 adapter.

## Local measurement, 2026-10-18

Machine: Windows 11, RX 9070 XT (17,095,983,104 reported GPU bytes), Ryzen 9
9950X3D, installed `torch 2.9.1+rocm7.2.1`, `voxcpm 2.0.3`. Model revision
`32279effe8c19989596f05d353d1447f51d9e915`; all six pinned asset hashes verified
before each process. Device `cuda:0` maps to the AMD card through HIP. Eight CPU
threads, one interop thread, CFG 2.0, seed 42, retry_badcase disabled, no denoiser,
offline assets and unchanged upstream model precision. No other test workload
was deliberately run concurrently; background desktop GPU use was not controlled.

Generic sample: `Hello. I am here to help you with whatever comes next.`
Design description: `A calm adult male narrator with a clear British accent.`
The generated synthetic reference was reused for all speech comparisons.

Initial process timings, measured separately:

| Stage | Seconds |
| --- | ---: |
| Asset hash verification | 2.356 |
| Imports | 3.312 |
| Model load and GPU synchronization | 9.387 |
| First design generation, 4.96 seconds of audio | 25.889 |
| First reference encoding | 5.982 |
| First saved-reference generation | 18.125 |
| Next ten-step saved-reference generation | 2.322 |

The first design therefore took about **40.94 seconds** including validation,
imports and loading. A second fresh process loaded in 8.888 seconds, encoded the
reference in 2.877 seconds and rendered its first reference request in 4.432
seconds. Process-cold does not mean disk/kernel-cache-cold. These observations
show substantial first-use cost and variability; they do not identify every
underlying kernel or provide a fixed cold-start guarantee.

The second process then ran explicit non-streaming and streaming warm-up requests,
followed by three rounds with rotated step order: 10/6/4, 4/10/6, 6/4/10. Each
round measured both full rendering and streaming. Timings include transfer to CPU
and explicit GPU synchronization, excluding WAV file writing, HTTP and playback.
Medians of the three measured repetitions:

| Steps | Audio duration | Complete render | Render time / audio duration | Streaming total | First streamed chunk |
| --- | ---: | ---: | ---: | ---: | ---: |
| 10 (current default) | 4.80 s | 2.352 s | 0.490 | 2.472 s | 62.6 ms |
| 6 | 4.80 s | 1.859 s | 0.387 | 1.988 s | 46.7 ms |
| 4 | 4.64 s | 1.619 s | 0.349 | 1.728 s | 39.1 ms |

Six steps reduced render time about 21%. Four steps reduced the time per second
of generated audio about 29%; its slightly shorter output makes raw elapsed-time
comparisons less exact. Warm description-only generation at ten steps took 2.443
and 2.409 seconds for 4.96 seconds of audio. Peak PyTorch GPU allocation was
6,044,725,248 bytes in this process; total device/process memory can be higher.

Each streamed chunk contained 160 ms of audio. The first chunks were quiet;
using RMS > 0.01 solely as an energy diagnostic, the first above-threshold chunks
arrived at median 560/448/221 ms for ten/six/four steps. This threshold does not
detect words or prove intelligibility. The 39–63 ms first-chunk numbers are
**model output latency**, not time to hearing speech in JARVIS. Streaming incurs
some total compute overhead while allowing playback to overlap generation.
No listening study, intelligibility score, accent/identity comparison or quality
equivalence was established, so the production default remains ten steps.

Reproduction scripts and raw evidence in this checkout's ignored `target/`:

- `voice-gpu-research/performance.py` and `performance/results.json`
- `voice-gpu-research/performance-warm.py` and `performance-warm/results.json`
- WAVs for each configuration alongside those JSON files
- `voice-gpu-research/performance-api.mjs` and `performance-api.log`: the later
  main-profile HTTP attempt received connection refused on port 8765 because no
  main daemon was running. It did not generate audio or alter saved voices. No
  HTTP latency result is claimed from this attempt. Earlier release acceptance
  remains recorded in [voice-gpu.md](voice-gpu.md).

Both isolated benchmark processes exited successfully. Their settings do not
modify the installed worker, voice library or profile. No ElevenLabs request was
made, and no direct local-versus-cloud benchmark is claimed.

## Recommendations from the initial review

1. **Make readiness reflect warm inference.** Offer an explicit/background
   warm-up when the designer is opened or the configured voice is enabled, with
   progress for verification, load and warm-up. Successful requests already keep
   the model loaded; avoid presenting that as a missing feature. Make residency
   a conscious memory trade-off (roughly 5–6 GiB observed allocation). Loading
   alone did not remove the first-generation cost in these measurements.
2. **Stream through the entire speech path.** Preserve bounded admission,
   cancellation, audio validation and fallback-before-first-audio semantics while
   carrying chunks from the worker to browser playback. Design samples must still
   retain a complete validated reference for saving. SDK streaming alone is not
   an end-to-end implementation.
3. **Expose a quality/speed control.** Test a six-step balanced option and a
   four-step fast option against the ten-step default with listening and identity
   checks. The measured improvement is useful, but not a tenfold speed-up. Keep
   saved-reference identity and persistence behavior explicit when changing steps.
4. **Evaluate compilation separately.** Pin a compatible Triton/ROCm environment,
   verify actual compilation and indexed-device handling, then compare warm
   latency, startup/compile time, audio quality, memory, cancellation and CPU
   fallback. NVIDIA TensorRT/F5 recipes do not apply directly to AMD VoxCPM2.

ElevenLabs' published ~75 ms Flash number is model inference latency, explicitly
not end-to-end latency. Its streaming service can legitimately start playing
before this local implementation returns a whole file. Being local removes the
network round trip; it does not guarantee lower first-audio latency. Compare the
same text and voice capability, cold/warm state, time to first audible speech,
total duration, and quality before claiming a winner.

## Implemented warm-up and streaming acceptance, 2026-10-18

The owner selected the first three recommendations. The release now implements
explicit preparation with verification/loading/warming stages and elapsed time,
Fast/Balanced/Quality at four/six/ten steps, and incremental PCM playback for
design samples, saved-voice preview and Console speech. Quality remains the
default. A saved reference records its creation quality separately from the
editable delivery quality; changing the latter preserves voice identity.

The private JVD3 worker protocol, HTTP stream ownership, successful-reference
admission and failure semantics are specified in
[ADR-0251](../../adr/0251-local-voice-streams-retain-admission-until-completion.md).
The pinned runtime, model, precision and GPU drivers were not changed. Opening
Settings alone does not load the model. The Prepare action performs real design
inference, reference encoding and reference-conditioned streaming before ready.
Cancellation releases the worker, so the next request needs preparation again.

The first release acceptance passed in
`target/voice-stream-acceptance/profile-DwITzZ/verification.json`, with matching
`target/voice-stream-release-live.log`. Real Edge browser playback on the RX 9070 XT
began before HTTP completion for all three modes and preview; quality and the exact
saved reference survived restart. Warm-up cancellation recovered, Stop after PCM
released the worker, and killing the owned Python worker after first audio ended
the response without CPU replay. Hiding both HIP/CUDA devices in a disposable
process exercised the separate CPU runtime, including warm-up and streamed speech
with the same saved reference. Authentication and unknown-quality refusal passed.
Desktop/mobile controls were visually reviewed; the mobile page had no horizontal
overflow. All disposable daemons and workers exited.

The expanded final release suite also passed in
`target/voice-stream-acceptance/profile-GPRY9t/verification.json`, with log
`target/voice-stream-release-final.log`. It additionally used the actual Console
UI with a deterministic scripted text reply and real AMD speech: 94 Web Audio
buffers ran and drained, with first scheduling 8.672 seconds before the final
speech response completed. This tests the speech path, not a live language model.
No browser errors occurred.

One browser sample per mode after explicit warm-up measured:

| Mode | First buffer scheduled | HTTP generation complete | Audio duration |
| --- | ---: | ---: | ---: |
| Fast | 134 ms | 1.966 s | 4.00 s |
| Balanced | 104 ms | 2.245 s | 4.00 s |
| Quality | 143 ms | 3.168 s | 4.48 s |

Preparation took 25.343 seconds; observed model allocation was 5,767,868,928 bytes.
The browser schedules with a 40 ms lead, and initial PCM can be quiet: these are
not first-audible-word measurements. The UI generates a fresh seed for each
sample, so this is a functional acceptance run, not a controlled comparison of
quality modes or a guarantee that Balanced starts faster than Fast. The first
new-text preview using the newly generated reference scheduled audio at 3.906 s
and completed generation at 8.439 s for 8.16 s of audio. Generic warm-up does not
eliminate all new-reference encoding or shape-dependent first-use costs. An
earlier passing release run's preview started at 201 ms; report that variability
rather than promising subsecond latency for every preview.

The initial debug attempt with `passed: false` remains in
`profile-BSh0H1/verification.json`: its product checks passed, but an intentionally
aborted fetch clone in test instrumentation created an unhandled promise rejection.
The harness now catches that instrumentation abort and measures browser fetch EOF
directly; Playwright's media-response completion tracking had also stalled in two
earlier attempts. Those attempts are retained and are not acceptance evidence.

The baseline passed: 3,314 Rust workspace tests, eight opt-in tests ignored,
strict all-target/all-feature Clippy, formatting, 151 JavaScript/documentation
tests, and four device-selection fixtures. These results do not establish NVIDIA
hardware or native macOS/Linux acceptance. No cloud call or ElevenLabs comparison
was made. Fast and Balanced are speed/quality trade-offs; no listening study or
identity-equivalence claim is made.

The tested release was then activated in the owner's main profile. Authenticated
preparation reported AMD Radeon RX 9070 XT and ready; an unsaved Balanced design
returned first PCM in 80 ms and completed in 2.375 s. New-text reuse of that
reference returned first PCM in 3.522 s and completed in 5.570 s. These main-profile
figures are HTTP receipt timings, not browser playback or listening measurements.
The configuration and voice-library file hashes were unchanged. The daemon was
left running with the model warm; no test voice was saved or selected. Evidence:
`target/voice-stream-main-activation.json` and
`target/voice-stream-main-activation.mjs`. The running executable is this checkout's
`target/release/jarvis.exe`, serving the normal loopback port 8765.
