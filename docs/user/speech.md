# Speech providers and backup voices

The console asks the daemon to synthesize audio. Keys stay in the daemon. Speech
recognition still uses the browser; phone calls still use the configured ElevenLabs
agent. Normal builds include `local-tts` and can synthesize locally inside the daemon. Guided setup
is unchanged.

## Pick, edit and preview voices

Open **Settings → Voice & search**. The saved-voice picker starts with your
configured primary and backup voices. Under **More ways to add a voice**, choose
**Copy selected voice** to create another named entry using an existing provider,
or edit an entry's name, voice, model
and supported controls. Local Kokoro offers Heart (American English) and George
(British English). Cloud entries accept a voice ID from that provider's library.

**Preview voice** speaks the current unsaved edits. It does not save, change the
default or fall back to another voice if the selected one fails. Cloud previews
send the sample text to that provider and may use paid credits. Local previews
use the verified cached model; a missing preset follows `allow_download`.
**Stop preview**, changing a setting, leaving the section or closing Settings
stops playback and cancels pending preview work. Preview audio is not saved; a
designed voice retains its generated reference only when you save the voice.

**Save voices** persists the complete library and applies it to new speech without
a restart. Pick a default, optionally override it for the console, and arrange
backup voices. Each provider is attempted once; a console voice at a provider
already listed as a backup takes that provider's place. Changing provider
connections or credentials still requires a restart. Existing audio requests
keep their settings. A stale save from another window is refused; reload before
editing again. Unsaved drafts remain in this page across section changes and are
discarded by reloading the page. Delete is available after removing a voice from
default/channel/backup selections; it removes only the local library entry.

Controls follow the selected model. Kokoro supports speed (0.5–2); documented
OpenAI-compatible models support speed (0.25–4). ElevenLabs v4/v4 Turbo expose
stability and similarity; v3 exposes its three stability modes; supported v2
models also expose speed, style and speaker boost. Blank controls retain provider
defaults. Pitch and unsupported controls are hidden and rejected by the API.
Known model suggestions are not a claim that a compatible server hosts them.

Telegram currently sends text only. Calls use the voice configured on the
ElevenLabs agent. Those channel assignments are unavailable in this editor;
neither is silently changed by the console/default selection. Local description design and preset mixing
are available below; voice cloning remains separate work.

The profile's private `voice-library.json` stores versioned names, provider IDs,
voice/model IDs and settings, with no keys or endpoints. Keep it when moving or
backing up the profile. Once saved, it controls voice choices; `config.toml`
still controls destinations, credentials and initial choices for a new library.
Malformed files and removed provider references fail validation rather than
silently resetting the library. See [ADR-0247](../adr/0247-voice-previews-and-saved-voices-use-the-same-validated-adapters.md).

The authenticated API is `GET/PUT /api/v1/settings/voices` and
`POST /api/v1/settings/voices/preview`. PUT submits the `library` object returned
by GET including its revision. Preview accepts `{ "voice": <saved-voice-shaped
draft>, "text": "A sample" }`, capped at 300 characters. No destination/key fields
are accepted. Errors use 422 for invalid settings, 409 for stale saves and 429
for a concurrent preview. Successful preview responses are bounded WAV/MP3 with
`Cache-Control: no-store`.

## Design a local voice

Open **Settings → Voice & search → Create a voice**. Name it and describe the voice:
for example, “A warm adult male voice with a British accent, calm and clear.”
Enter the words for a short sample, then **Generate voice**. Listen to it;
**Generate another voice** offers a new variation. Edit the description to change
the direction. **Stop generation** cancels local work.

Use **Prepare voice model** before generating to move the cold-start wait out of
your first sample. Progress reports **Checking model files**, **Loading the model**
and **Warming up voice generation**, then **Model ready** with the actual device.
**Stop preparation** releases that work. Preparation keeps roughly 5–6 GiB of
model allocation resident on the tested AMD GPU; simply opening Settings does
not load it. Stopping generation or restarting JARVIS requires preparation again.

**Generation quality** offers **Fast**, **Balanced** and **Quality**. Quality is
the default. Fast and Balanced reduce computation, with a possible loss of detail;
listen before saving. The selection applies to the next generated sample and to
new speech after **Save voices**. Changing it preserves your existing voice's
identity; it does not regenerate the saved reference.

New samples, new-text previews and Console speech play incrementally on browsers
with Web Audio. Stop cancels both scheduled playback and generation. A sample
becomes saveable only after the whole generation succeeds. The complete sample
player remains available for replay. A failed partial stream stops rather than
repeating the beginning with another voice.

Console reply lines stay in order: new text joins the queue, and the next piece
starts only after the current audio finishes. Synthesis can prepare later audio
while the current piece plays, with only one unfinished synthesis request at a
time. Temporary local capacity waits retry the same voice; they do not mark it
unhealthy or immediately switch to the browser. A genuine provider failure can
still use a backup or browser speech, and later reply text remains queued there.
Browser speech no longer discards text after 1,800 characters. Long Console
sentences are split into shorter requests so they fit the local audio budget.
Automatic wake-word listening waits until the entire speech queue is done.
**Silence**, Escape, microphone activation or a new user message still interrupt
speech deliberately.

Choose **Save and use in console** to save and select it in one action, or
**Save voices** to keep it for later. **Default voice and backups** contains the
channel choices and fallback order. **Try this voice with different words**
opens the new-text preview after generation. The saved synthetic
sample keeps the voice consistent after restart. This uses a pretrained model
with a generated reference; it does not train a new model or clone a recording.
Descriptions guide the result, so listen before selecting it.

Generation shows elapsed time and any error directly below its button. Stop is
shown only while work is pending; the sample player appears in the same panel.
**Create a voice** returns to an unfinished draft instead of creating duplicates.
Drafts without a sample cannot be selected as defaults or backups. **Manage this
voice → Discard draft** removes an unfinished entry.

The editor checks whether the configured runtime and model files are present.
If setup is missing, Generate is disabled with repair instructions and **Check
setup again** preserves your draft. This check does not load the model or replace
the integrity checks performed during inference. If installation is present but
a running daemon still reports it missing, check `runtime_dir` and restart JARVIS
to reload provider configuration.

The local VoxCPM2 designer needs a one-time installation of approximately 5 GB of
model files plus a private inference environment. On Windows, the installer detects
AMD or NVIDIA and installs a pinned GPU environment alongside the CPU fallback.
The designer probes GPU kernels, prefers the compatible adapter with most memory,
and uses it for both voice creation and speech with saved voices. Settings shows
the observed device and model memory allocation. Before first use it says device
selection is pending. If GPU loading or generation fails before any audio is
emitted, JARVIS retries on CPU and keeps that choice until restart. Once audio has
started, a failure ends that stream without replaying it. Saved references remain unchanged.
GPU kernels can produce waveform differences between runs; saving preserves
the generated reference identity rather than guaranteeing identical audio bytes.

GPU setup adds several GB of packages. AMD uses ROCm 7.2.1 / PyTorch 2.9.1;
NVIDIA uses PyTorch 2.10.0 / CUDA 12.8. A compatible GPU and driver are required;
other GPUs use CPU. NVIDIA hardware acceptance and non-Windows GPU installers
remain pending. On the previously tested Windows CPU,
cold short samples took 30–69 seconds including loading; subsequent speech took
about 15 seconds. Loaded worker memory ranged from roughly 5–7 GiB. JARVIS owns the local worker: there is no server, open port or startup
task. Inference uses verified local assets and needs no account. Kokoro remains
the much faster option for routine speech.

Install on Windows x64 with CPython 3.12.15 and uv 0.13.1 already installed:

```powershell
.\scripts\install-voice-designer.ps1 -Uv C:/path/to/uv.exe -Python C:/path/to/python.exe
```

Use `-Backend AMD` or `-Backend NVIDIA` to select the GPU runtime explicitly.
Use `-Backend CPU` to disable GPU selection while retaining installed GPU files.
The CPU environment is always retained; a GPU setup failure leaves it selected.
Restart JARVIS after changing the installation. Embedded Kokoro remains CPU-based;
ElevenLabs and other hosted voices run at their provider.

The installer pins and verifies inference wheels and model files. It defaults to
`%LOCALAPPDATA%/JARVIS-VoiceDesigner/voxcpm2-2.0.3`. Append a provider to the existing list
in your profile, using the actual absolute installation path, then restart:

```toml
[[daemon.speech.providers]]
id = "voice-designer"
kind = "local-voxcpm2"
runtime_dir = 'C:/Users/YourName/AppData/Local/JARVIS-VoiceDesigner/voxcpm2-2.0.3'
```

A missing installation is reported on generation without preventing other voices
from starting. Descriptions accept 1–500 characters; sample text accepts 1–120.
Generated references must fit ten seconds; shorten the sample if it exceeds that
limit. Generation has a three-minute deadline and Settings admits one preview or
generation at a time. The private library has an 8 MiB limit. It stores generated
references alongside descriptions, seeds and model revisions; include it in backups. Database-only backups do not
include this configuration file; back up `voice-library.json` separately or with
the full private profile. Public model files can be reinstalled.

**Export recipe** downloads the saved design as JSON, including the synthetic
reference. It contains no credentials, paths or provider identities. Automatic
import and uploading recordings are not implemented. To delete, remove the voice's
default, Console and backup assignments, save, then **Delete voice → Save voices**.
This removes its reference from the library; exported copies and profile backups
are separate files. Shared public model assets remain installed.

Authenticated `POST /api/v1/settings/voices/design` accepts `provider`,
`description`, `text`, `seed` and an optional `request_id`; it returns an unsaved
`design`. Save/preview accept only a design generated in this daemon session or
an unchanged saved design. `POST /api/v1/settings/voices/cancel` cancels the matching
`request_id`, including early cancellation. Both require the local credential.

See [ADR-0249](../adr/0249-description-designed-voices-retain-the-generated-reference.md)
and [real description synthesis and browser verification](../research/integrations/voice-description-design.md).

### Optional preset mixing

With an embedded Kokoro provider configured, open **Settings → Voice & search →
More ways to add a voice → Mix presets**. Give the new voice a name, then blend **Heart** and **George** with
the balance slider or percentage fields. The percentages always add up to 100.
Choose American or British English pronunciation independently of the balance,
and optionally change speed. **Preview voice** plays these exact unsaved settings.
Kokoro cannot design from a written description; no recording or training is needed.

Choose **Save voices** to add the design to the library. Select it as the default,
console override or a backup to use it for ordinary speech. **Export recipe**
downloads the saved name, settings and versioned blend as JSON; save new edits
before exporting. The recipe pins the model revision, preset weights and
pronunciation, so restart reconstructs the same voice. It contains no keys,
provider identities, paths or generated audio. Automatic recipe import is not
implemented. To delete it, remove its default, console and backup assignments,
save, then choose **Delete voice → Save voices**. Shared public model assets remain.

Design uses the same local model and verified public presets as embedded speech.
The first blend needs both preset files; if downloads are disabled, both must
already be cached. Switching a draft to a cloud provider clears its local recipe.
The API rejects sending a recipe to any non-embedded provider.

Authenticated `GET /api/v1/settings/voices/{id}/export` returns the saved design
with `Cache-Control: no-store`, or 404 for a missing/non-designed voice.
See [ADR-0248](../adr/0248-local-voice-design-saves-a-versioned-blend-recipe.md)
and [real synthesis and browser verification](../research/integrations/voice-design.md).

## Provider connections

Edit your profile's `config.toml` and restart JARVIS. An explicit list replaces the
legacy `speech_api_key_ref`, `speech_voice_id`, and `speech_model` selection for
console speech. Those legacy fields still work when the list is empty; the legacy
key also remains the phone credential. Settings displays the running provider list.

Example: ElevenLabs first, then an already running local Kokoro-FastAPI server:

```toml
[[daemon.speech.providers]]
id = "primary"
kind = "elevenlabs"
key_ref = "C:/Users/YourName/keys/elevenlabs.key"
# voice and model may be omitted to retain JARVIS defaults.

[[daemon.speech.providers]]
id = "local"
kind = "openai-compatible"
base_url = "http://127.0.0.1:8880/v1"
model = "kokoro"
voice = "af_heart"
```

Use an absolute private key-file path appropriate to your platform. A key is
optional for the local server. To use OpenAI, add or substitute a provider with
`base_url = "https://api.openai.com/v1"`, `model = "gpt-4o-mini-tts"`,
`voice = "alloy"`, and its own `key_ref`. Never reuse an ElevenLabs key for OpenAI.
The base must include `/v1`; the adapter appends `/audio/speech`.

Provider ids must be unique letters/digits/underscore/hyphen, up to 64 characters.
There may be up to eight providers. Local-server models and voices must actually
exist on that server. Loopback HTTP is allowed; other destinations require HTTPS.
URL credentials, queries, fragments and redirects are refused. Model installation,
server startup, licenses and the server's own external connections are the owner's
responsibility. A loopback URL does not prove an engine operates offline.

## What happens during failure

Providers are tried in order until the first audio arrives. The console then keeps
that provider for the answer unless it fails. A backup is labelled “backup voice”.
Outages, rate limits and invalid audio skip a provider for 30 seconds; rejected keys
or HTTP 402 credit exhaustion skip it for ten minutes. One request probes recovery.
Starting a new answer can return to the primary after recovery.

Audio from different providers is never joined into a sentence. A mid-stream
failure ends playback and the browser voice handles the remaining answer. If all
configured providers fail, the browser voice is also the final fallback. Browser
voices may themselves use online services. There is no claim of offline recognition.

Each attempt is bounded to 25 seconds and 8 MiB, with at most 1500 text characters.
Multiple unavailable providers can increase first-audio delay. Text is sent only to
providers you configured, including backups attempted after failures; those attempts
can incur charges. JARVIS does not retain the generated audio.

## Embedded speech without a server

Plain `cargo build --release` includes embedded speech in both programs. A smaller
cloud/server-only build can explicitly opt out with
`cargo build --release -p jarvis-cli -p jarvisd --no-default-features`.
Such a build cannot start a profile configured with `embedded-kokoro`; rebuild
normally or add `--features local-tts`. Building does not download speech assets.
Replace the local-server entry with this entry, retaining your existing primary:

```toml
[[daemon.speech.providers]]
id = "local"
kind = "embedded-kokoro"
voice = "af_heart"
allow_download = true
```

No URL or key is needed. `af_heart` is US English; `bm_george` is British English.
The only model is `kokoro-v1.0` (the default). The editor supports speed and the
two-preset blending described above. Other languages are not supported yet.
Uncommon names may be spelled by the English phonemizer.

Run `jarvis doctor --live` once to prepare the assets and synthesize a short probe.
It can also contact other providers configured in the profile. First use downloads
about 176 MB on Windows x64, including the 92 MB quantized model and verified CPU
runtime archive. No Python installation, listener or startup task is required.
The native runtime runs inside JARVIS with telemetry disabled. Assets are kept under
the profile's `cache/speech-kokoro-v1` directory; on Windows this is
`%LOCALAPPDATA%\JARVIS\cache\speech-kokoro-v1`. Windows/Linux x64 and ARM64 and
Apple Silicon macOS have pinned runtime packages; Intel macOS is unsupported by
this selection. A build without `local-tts` refuses this configuration at startup.

After preparation, set `allow_download = false` and restart to forbid asset
downloads. Missing or modified assets then fail closed; remove the damaged cache
and explicitly permit downloads again to repair it. Synthesis checks the exact
cached model/runtime bytes before loading them. Text and generated speech stay
in process; the cache contains public assets, not transcripts or generated audio.
First initialization can outlast a 25-second speech attempt: preparation continues,
and a later attempt uses the loaded model. Doctor can wait up to five minutes.

Embedded speech returns `audio/wav`: 24 kHz mono signed 16-bit PCM. The console
plays the WAV as an audio blob alongside the existing MP3 MediaSource path.
One sentence synthesizes at a time, with room for the console's two prefetched
sentences. Cancellation terminates inference. Whole-sentence WAV generation means
first audio arrives after synthesis, rather than during it.

Real-model acceptance (downloads require this explicit opt-in):

```powershell
$env:JARVIS_EMBEDDED_TEST_CACHE = "$PWD/target/speech-embedded-acceptance/cache"
cargo test -p jarvis-voice --features local-tts --test embedded_live -- --ignored --nocapture
node scripts/accept-embedded-speech.mjs --run-live --cache $env:JARVIS_EMBEDDED_TEST_CACHE --playwright C:/path/to/playwright/index.mjs
```

The browser harness creates a disposable profile, forbids downloads, injects a
primary HTTP 503, checks real WAV playback and saves local evidence. The separate
Kokoro server can be stopped throughout. Native measurements and remaining
acceptance limits are recorded in [embedded speech research](../research/integrations/embedded-speech.md).

## Diagnose and verify

`jarvis doctor --live` synthesizes “Systems online.” independently through every
configured provider, reporting bounded audio or a redacted error. It may cost a
small amount on hosted providers. Plain `jarvis doctor` remains offline.

Authenticated `GET /api/v1/speech` reports `providers` (id, kind as `provider`, voice,
model, health, retry delay) and `active` (last selected id, initially null). Health
starts `unknown`; this is not a live probe. No URL, key or key path is returned.
`POST /api/v1/speech` accepts `text`, optional `previous_text`, and optional `provider`
as the preferred id for subsequent sentences. Response headers
`x-jarvis-speech-provider` and `x-jarvis-speech-backup` identify the selection.
MP3 streams use `audio/mpeg` and `Cache-Control: no-store`.

Local streaming clients opt in with `stream: true`. VoxCPM2 then returns
`audio/pcm;rate=48000;channels=1;format=s16le`. HTTP chunk boundaries do not
necessarily align with PCM samples. Without the flag, local speech remains a
complete WAV. Voice preview accepts the same flag; design uses bounded NDJSON
`audio` events (base64 PCM) and one `complete` event containing the validated
reference. Absence of `complete` is a failed design. Optional `quality` accepts
`fast`, `balanced` or `quality` (default). Only local designed voices support the
corresponding saved `settings.quality` field.

`POST /api/v1/settings/voices/prepare` accepts `provider` and optional
`request_id`; it shares preview admission and cancellation. Poll authenticated
`GET /api/v1/settings/voices/runtime` for `execution.phase` and actual device
metadata. Preparation and stream bodies retain cancellation/admission until they
finish. See [ADR-0251](../adr/0251-local-voice-streams-retain-admission-until-completion.md).

The implementation has fixture coverage, a [live OpenAI check](../research/integrations/speech-fallback.md#live-openai-evidence),
and [real local-server acceptance](../research/integrations/speech-fallback.md#live-local-evidence).
The local check covers actual MP3 synthesis, a controlled primary outage,
answer affinity, recovery and browser playback through the console. It can be
repeated with a running Kokoro server and Playwright 1.62.1:

```powershell
cargo build --release -p jarvis-cli -p jarvisd
node scripts/accept-local-speech.mjs --run-live --playwright C:/path/to/playwright/index.mjs
```

The harness creates a disposable profile under `target/speech-local-acceptance`,
uses two real Kokoro voices, injects HTTP 503 only at a local primary proxy, and
stops that test daemon afterward. It records fixed test audio, a screenshot and
`verification.json`. It does not edit the main profile. Windows uses installed Edge;
other platforms require Playwright's Chromium. These live checks are opt-in;
ordinary CI runs the harness guard tests and feature-enabled Rust tests. Embedded
speech and voice editing are described above.

## Installed Windows backup (2026-10-18)

The main profile now uses ElevenLabs first and **embedded Kokoro** with
`af_heart` second. The installed JARVIS binary includes `local-tts`; the backup
runs inside the daemon with `allow_download = false` and verified cached assets.
No separate speech server or speech startup task is needed. The former
**JARVIS Kokoro Speech** scheduled task is disabled, and its server is stopped.

Activation was verified against the main profile: embedded synthesis returned
valid, non-silent 24 kHz mono WAV in 1351 ms cold and 401 ms warm. The primary
ElevenLabs voice still returned real MP3 (308 ms). The old server's port 8880 had
no listener, and the startup task was confirmed disabled. Evidence is retained
locally in `target/speech-embedded-activation.json`.

The previous profile is backed up at
`%APPDATA%\JARVIS\config.before-embedded-speech-20261018-181342.toml`.
The former Kokoro-FastAPI installation and launchers remain under
`%LOCALAPPDATA%\JARVIS\speech` for rollback. To return to it, stop JARVIS,
restore that configuration backup, enable **JARVIS Kokoro Speech**, run
`Start-Kokoro.ps1` from that directory, and restart JARVIS. The older
`config.before-local-speech-20261018.toml` backup predates both backup modes.

Design: [ADR-0246](../adr/0246-embedded-speech-loads-only-pinned-local-assets.md).
Sources and remaining native acceptance: [embedded speech research](../research/integrations/embedded-speech.md).


## Startup while another command is starting JARVIS

`jarvis start` waits up to three minutes for the HTTP endpoint. If another
invocation already holds the profile while it initializes, the client waits for
that same daemon instead of reporting a competing-process failure. Other startup
errors still fail immediately. Once ready, `jarvis` connects to the existing app.
The designer's public runtime is installed outside managed profile directories
so startup permission maintenance does not traverse its many package files.
