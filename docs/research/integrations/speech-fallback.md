---
integration: speech-fallback
status: verified
last_verified: 2026-10-18
owners: []
selected_spec_version: OpenAI HTTP v1; Kokoro-FastAPI v0.2.4
selected_sdk: none (existing reqwest =0.13.5)
---

# Speech providers and fallback

## Scope

P8-018–020: sentence synthesis, provider-neutral byte streams, circuit breakers,
and an owner-configured OpenAI-compatible speech server. Embedded engines,
recognition, cloning and phone calls are separate work.

## Official sources

All accessed 2026-10-18 (UTC). No upstream implementation is copied.

| Source | URL/version | Purpose |
| --- | --- | --- |
| ElevenLabs index | https://elevenlabs.io/docs/llms.txt | discovery |
| ElevenLabs stream | https://elevenlabs.io/docs/api-reference/text-to-speech/stream | existing POST contract, MP3 default, continuity |
| ElevenLabs SDK source | https://raw.githubusercontent.com/elevenlabs/elevenlabs-python/main/src/elevenlabs/text_to_speech/client.py | HTTP serialization; inspected, not installed |
| ElevenLabs changes | https://elevenlabs.io/docs/changelog | no relevant deprecation observed for the existing stream operation |
| OpenAI index | https://developers.openai.com/llms.txt | discovery |
| OpenAI speech | https://developers.openai.com/api/reference/resources/audio/subresources/speech/methods/create | normative request, binary audio vs SSE |
| OpenAI SDK | https://raw.githubusercontent.com/openai/openai-python/v3.35.1/src/openai/resources/audio/speech.py | binary response, request serialization |
| OpenAI release | https://github.com/openai/openai-python/releases/tag/v3.35.1 | selected inspection version; no SDK dependency |
| OpenAI data controls | https://developers.openai.com/api/docs/guides/your-data | provider-side retention is account/service policy |
| Kokoro source | https://raw.githubusercontent.com/remsky/Kokoro-FastAPI/v0.2.4/api/src/routers/openai_compatible.py | /audio/speech, stream response, MIME mapping |
| Kokoro request schema | https://raw.githubusercontent.com/remsky/Kokoro-FastAPI/v0.2.4/api/src/structures/schemas.py | model/voice defaults, streaming enabled by default |
| Kokoro release | https://github.com/remsky/Kokoro-FastAPI/releases/tag/v0.2.4 | pinned compatibility target |
| Kokoro README | https://raw.githubusercontent.com/remsky/Kokoro-FastAPI/master/README.md | discovery and local endpoint |

Kokoro repository-root `llms.txt` was not found at
https://raw.githubusercontent.com/remsky/Kokoro-FastAPI/master/llms.txt.
ElevenLabs `.md` fetch was unsupported by the browser tool; the HTML reference
was read instead. SDK source is inspection evidence, not an installed runtime.

## Verified contract

OpenAI `POST https://api.openai.com/v1/audio/speech` takes `input`, `model`,
and `voice`; `response_format: mp3` selects MP3. `stream_format: audio`
selects binary audio rather than SSE. Authentication is a Bearer header.
The reference lists a 4096-character input maximum. Kokoro v0.2.4 exposes
the same operation under `/v1`, returning `audio/mpeg` for MP3; its local
streaming extension defaults to streaming. Its voice/model catalog is server
specific. No key is required by that router; a reverse proxy may require one.
No pagination, webhook, OAuth refresh or application idempotency is involved.

ElevenLabs continues to use `xi-api-key`, `/v1/text-to-speech/{voice}/stream`,
`model_id`, and bounded `previous_text` except on `eleven_v3*`. See the
[existing record](elevenlabs.md) for measured continuity behavior.

## JARVIS decisions and limits

Use existing pinned reqwest HTTP directly: no SDK or model downloads. Keys are
read from absolute file references, kept in redacted adapters, never returned.
Explicit ordered providers replace the legacy single-provider interpretation;
the old fields still compose ElevenLabs when no list is present. Only configured
providers receive text. HTTP is allowed only for literal loopback addresses or
`localhost` pinned to loopback; all other endpoints require HTTPS. Credentials
in URLs, queries, fragments, redirects and proxy forwarding are refused/disabled.

Each attempt has a 25-second deadline, 1500-character text cap and 8 MiB audio cap.
MP3 responses accept audio/mpeg, audio/mp3, or binary octet-stream for the requested
MP3; JSON/HTML/SSE are failures. These are JARVIS bounds, not vendor quotas.
401/403 and 402 open the breaker for ten minutes; 429, transport/5xx and unusable
audio open it for 30 seconds. Only one recovery probe is admitted. No provider
is retried in one request. Once audio starts, failure terminates that stream;
audio from two providers is never concatenated. An answer prefers its previously
selected provider, and may switch only on failure/unavailability. Browser synthesis
is the last resort. Live doctor probes synthesize a short fixed sentence and can
incur provider charges; offline doctor remains offline.

Text and limited continuity go to each attempted provider; JARVIS does not persist
audio. Cloud retention, residency, cost and quotas depend on the selected account.
A local URL describes transport only: the owner must choose and operate an engine
whose own egress, model licenses and retention meet their needs. No engine is bundled.

## Rejected alternatives

No silent cloud fallback, guessed voice discovery API, automatic server installation,
whole-answer replay, or mid-stream MP3 concatenation. No phone-agent TTS replacement.

## Verification plan

Loopback wire fixtures must verify exact paths, optional Bearer auth, body fields,
content type, time/size bounds and no redirects. Falsify the URL and audio guards.
Test 401/402/429/5xx/outage/empty audio, mid-stream failure, breaker recovery,
concurrent half-open requests, cancelled probes and voice affinity. Gateway tests
prove authenticated access, first-chunk gating and no key/raw provider errors.
Console tests prove initial selection precedes lookahead and the backup note.

## Unresolved questions

Live OpenAI and Kokoro synthesis passed on 2026-10-18. Browser decoding/playback
was verified, but subjective voice quality was not assessed. Timings below are
single-machine observations, not a cold-start benchmark. Other compatible servers (VoiceStudio, Speaches,
LocalAI) need per-server verification; wire compatibility is not acceptance.
Native Windows/macOS/Linux acceptance remains distinct from this machine's checks.

## Live OpenAI evidence

On 2026-10-18, the production OpenAI-compatible Rust adapter made one short, fixed
English sentence using `gpt-4o-mini-tts` and `alloy`. First audio: 1133 ms; total:
2456 ms; 74496 bytes, `audio/mpeg`, MP3 framing verified. The owner supplied a
temporary credential, read through hidden terminal input and passed only in the
child process environment. The process cleared it on exit; no credential file,
profile change or repository secret was created. This proves real hosted synthesis,
not local-server readiness or subjective listening quality. The ignored
`live_speech` integration test can repeat the check with an explicit server.

## Owner-requested local server setup

On 2026-10-18 the owner explicitly requested installing a local speech server and
testing fallback end to end. Official sources were fetched again before setup:

- https://raw.githubusercontent.com/remsky/Kokoro-FastAPI/v0.2.4/README.md
- https://raw.githubusercontent.com/remsky/Kokoro-FastAPI/v0.2.4/api/src/core/config.py
- https://raw.githubusercontent.com/remsky/Kokoro-FastAPI/v0.2.4/api/src/routers/openai_compatible.py
- https://raw.githubusercontent.com/remsky/Kokoro-FastAPI/v0.2.4/docker/cpu/docker-compose.yml
- https://github.com/remsky/Kokoro-FastAPI/releases/tag/v0.2.4
- https://docs.docker.com/reference/cli/docker/desktop/start/
- https://docs.docker.com/engine/containers/run/
- https://docs.docker.com/engine/network/port-publishing/

The repository `llms.txt` remains absent. No relevant wire-contract change was
observed. Upstream documents Apache-2.0 for the wrapper/model and MIT for adapted
inference code. This Windows/AMD machine has 32 GB RAM; CPU inference avoids a
CUDA dependency. Docker Desktop failed before its engine started because of an
inaccessible stale `sailor-ingest.sock`; automatic approval review rejected the
repair operation. No repair or container installation was performed. The actual
installation uses upstream's native Windows CPU setup in a separate per-user
speech directory, without changing global Python or PATH. Additional official
sources fetched on 2026-10-18:

- https://raw.githubusercontent.com/remsky/Kokoro-FastAPI/v0.2.4/start-cpu.ps1
- https://raw.githubusercontent.com/remsky/Kokoro-FastAPI/v0.2.4/pyproject.toml
- https://raw.githubusercontent.com/remsky/Kokoro-FastAPI/v0.2.4/docker/scripts/download_model.py
- https://docs.astral.sh/uv/llms.txt
- https://docs.astral.sh/uv/getting-started/installation/
- https://api.github.com/repos/astral-sh/uv/releases/latest
- https://api.github.com/repos/remsky/Kokoro-FastAPI/releases/latest

Installed uv 0.13.1 (Windows archive SHA-256
`3b2661a463a5baea7f5076ea2b25ab71d77f36a6c0d809e5350651792f80a672`)
and Kokoro-FastAPI v0.2.4, the previously researched compatibility target; the
upstream release observed on this date was v0.9.0, not installed by this check.
The checked-out commit is `6b1e9d9128b9021f8b2716e213d3834be0371e1c` (its package
manifest labels itself 0.3.0; the Git tag/commit identify the tested source).
`uv sync --python 3.12 --extra cpu --no-dev` installed managed Python 3.12.15,
Kokoro 0.9.2, Misaki 0.9.3 and PyTorch 2.6.0+cpu. The generated lock file SHA-256 is
`71b3feb1f1412deb92162eecde6f8cec5c42a837ae44b837db5987497a8f0da5`;
`installed-requirements.txt` preserves the resolved package versions locally.

The upstream downloader selects the v0.1.4 release's Kokoro v1.0 asset:
https://github.com/remsky/Kokoro-FastAPI/releases/download/v0.1.4/kokoro-v1_0.pth.
The downloaded model is 327212226 bytes, SHA-256
`496dba118d1a58f5f3db2efc88dbdc216e0483fc89fe6e47ee1f2c53f18ad1e4`.
The repository's model `config.json` SHA-256 is
`5599150688395308ef8a56a2649446a9337755204c80c17265abd82a000bc14b`.
All packages/models are under `%LOCALAPPDATA%\JARVIS\speech`, outside this repository.

The native launcher binds only `127.0.0.1:8880`, disables CORS, the optional web
player and local voice saving, and suppresses upstream debug/access logging.
It uses the packaged eSpeak library and PyAV audio encoder; no global FFmpeg or
Python install was needed. The unauthenticated speech service receives no JARVIS
profile credentials. Loopback is not an OS sandbox and inference egress was not
blocked or measured, so this is not an offline-network guarantee. Model/package
installation used the network. Local `Start-Kokoro.ps1` and `Stop-Kokoro.ps1`
manage the process. Automatic sign-in startup was added at the owner's subsequent
request, as recorded below.

Browser acceptance uses the bundled Playwright 1.62.1 with Chromium, an isolated
browser context and disposable daemon profile. Official Page/Browser API docs
were rechecked at https://playwright.dev/docs/api/class-page and
https://playwright.dev/docs/api/class-browser on 2026-10-18. Media instrumentation
observes the native `Audio` element's playback events; it does not replace audio
decoding, stream bytes or the daemon. Only the primary transport failure is
injected; both successful voices come from the installed model.

## Live local evidence

On 2026-10-18, `/health` returned healthy and `/v1/audio/voices` listed 67 voices.
The production Rust adapter's ignored `live_speech` test passed against
`http://127.0.0.1:8880/v1`, model `kokoro`, voice `af_heart`: first audio 1248 ms,
total 1249 ms, 63404 MP3 bytes. PyAV decoded 3.96 seconds at 24 kHz.

`scripts/accept-local-speech.mjs --run-live` then exercised the real daemon and
console in an isolated profile. Both successful providers used the installed
model, with `bm_george` as primary and `af_heart` as backup. A loopback proxy
injected HTTP 503 for the primary; no fabricated audio was used.

| Check | Result |
| --- | --- |
| Authentication | Unauthenticated speech status returned 401. |
| Primary | Real MP3, first audio 456 ms, 64172 bytes. |
| Primary outage | Selected `local`, backup header true, first audio 366 ms, 53804 bytes; primary health became `cooling_down`. |
| Answer affinity/cooldown | Subsequent sentence and new request during cooldown used `local`, with no additional primary attempts. |
| Recovery | After the real 30-second interval, one successful probe restored primary/healthy; first audio 440 ms. |
| Console | Edge 155.0.4283.62 displayed the backup note. Both native audio elements reached `playing` and `ended` (6.072 s and 6 s), with no media or page errors; both requests selected `local`. |
| Cleanup | Disposable daemon stopped; main profile was not used for fault injection. |

Local evidence is retained in ignored `target/speech-local-acceptance/run-kPANOh/`:
`verification.json`, `backup-voice.png`, MP3 samples and daemon diagnostics.
`scripts/test-local-speech-gate.mjs` tests explicit opt-in and rejection of remote
or credential-bearing URLs before any browser, profile or daemon setup. Bypassing
that URL assertion in an ignored disposable copy made its refusal test fail at
the next pre-setup guard; the production harness was not changed. CI runs these
three tests without contacting any speech service.

After the owner's explicit "Activate the backup" instruction, the main profile
was backed up to `%APPDATA%\JARVIS\config.before-local-speech-20261018.toml`, the
tested provider list was appended, and JARVIS was restarted. Its existing
ElevenLabs key reference, voice and model were retained. Authenticated status
confirmed `elevenlabs` then `local`. Explicit local selection returned 42668 MP3
bytes (494 ms to first audio; decoded 2.664 s at 24 kHz). A normal request selected
ElevenLabs, returning 35152 MP3 bytes (1419 ms to first audio; decoded 2.194 s at
44.1 kHz). Both providers reported healthy. The bounded report is retained in
`target/speech-main-activation.json`; no credential contents were recorded.

## Automatic startup research (2026-10-18)

The owner requested automatic startup after reboot. The selected mechanism is
Windows Task Scheduler with a logon trigger for the current user, an Interactive
principal at Limited run level, and the existing hidden PowerShell launcher.
It requires no stored password or administrator token. This is sign-in startup,
not a system service running before sign-in. No JARVIS process topology changes.

Official Microsoft sources accessed before registration:

- https://raw.githubusercontent.com/MicrosoftDocs/PowerShell-Docs/main/llms.txt
  could not be fetched; the official cmdlet reference was used instead.
- https://learn.microsoft.com/en-us/powershell/module/scheduledtasks/new-scheduledtasktrigger?view=windowsserver2025-ps
- https://learn.microsoft.com/en-us/powershell/module/scheduledtasks/new-scheduledtaskaction?view=windowsserver2025-ps
- https://learn.microsoft.com/en-us/powershell/module/scheduledtasks/new-scheduledtaskprincipal?view=windowsserver2025-ps
- https://learn.microsoft.com/en-us/powershell/module/scheduledtasks/new-scheduledtasksettingsset?view=windowsserver2025-ps
- https://learn.microsoft.com/en-us/powershell/module/scheduledtasks/register-scheduledtask?view=windowsserver2025-ps

These are in-box Windows ScheduledTasks 1.0.0.0 cmdlets; no SDK dependency is added.
The installed Windows PowerShell 5.1.26100.9711 executable uses `-NoProfile`,
`-NonInteractive`, `-WindowStyle Hidden` and the absolute existing launcher path.
Existing execution policy is RemoteSigned and was not changed. Registered
`JARVIS Kokoro Speech` after checking that the exact task name was absent; no
other task was overwritten. Readback confirmed Enabled, Interactive/Limited,
current-user logon trigger, battery operation permitted, no network/idle
requirement, IgnoreNew overlap handling, unlimited execution time, and three
startup retries at one-minute intervals. The loopback listener and original
provider configuration remain unchanged. These retries concern startup failure;
the task is not a continuous process supervisor after the launcher succeeds.

Verification stopped only this installed Kokoro instance, confirmed port 8880
was free, then invoked the registered task. The task exited with result 0 and
the new process became healthy on `127.0.0.1:8880`. JARVIS's authenticated speech
endpoint selected `local`, returning 70700 MP3 bytes (first audio 2051 ms after
this fresh process start); PyAV decoded 4.416 seconds at 24 kHz. Running the task
again returned 0 and preserved the server PID. The actual reboot/logon event
was not exercised: verification used Task Scheduler's manual start of the same
registered action. Local evidence is in `target/speech-startup-task.json`,
`target/speech-startup-audio.json` and `target/speech-startup-verification.mp3`.

## Verification log

| Date | Version | Evidence |
| --- | --- | --- |
| 2026-10-18 | HTTP v1, SDK inspection v3.35.1, Kokoro v0.2.4 | Official sources rechecked before implementation; binary MP3 subset selected. |
| 2026-10-18 | Windows repository checks | 3,281 workspace tests pass; 7 opt-in tests ignored. All 35 voice unit tests, independent doctor probes, configuration/gateway/setup checks pass. Formatting and strict all-target/all-feature Clippy pass. |
| 2026-10-18 | Console and documentation | 155 script tests pass; 392 Markdown files, 1236 local links and 244 canonical ADRs checked with zero errors. |
| 2026-10-18 | Guard falsification | Removing the loopback-only HTTP check makes `compatible_speech_refuses_unsafe_destinations_and_header_injection` fail. Removing the cumulative size check makes `generic_stream_enforces_cumulative_bounds_and_permanently_ends_after_error` fail. Original source restored before the final passing checks. |
| 2026-10-18 | Live OpenAI | Production adapter returned 74496 MP3 bytes; first audio 1133 ms, total 2456 ms. |
| 2026-10-18 | Live Kokoro and browser | Real adapter synthesis, controlled outage, affinity/cooldown, recovery and full browser audio playback passed; main profile activated and both configured providers returned real MP3. See live local evidence above. |
| 2026-10-18 | Local acceptance harness | Three opt-in/URL/browser guards passed, and URL-guard falsification failed as expected. Expanded console/docs/lifecycle/acceptance-guard suite: 165 passed. |
| 2026-10-18 | Final Windows baseline | Re-ran formatting, strict all-target/all-feature Clippy and `cargo test --workspace`: 3281 passed, 7 opt-in tests ignored, no failures. Live adapter and browser acceptance ran separately as described above. |
| 2026-10-18 | Automatic sign-in startup | Registered the owner-authorized Windows logon task; verified successful start from a stopped server, healthy loopback listener, real JARVIS MP3 synthesis, and duplicate-start handling. No Rust or console implementation changed. |
