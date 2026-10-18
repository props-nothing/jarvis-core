---
integration: voice-settings
status: implemented; Windows console live-verified
last_verified: 2026-10-18
owners: []
selected_spec_version: ElevenLabs HTTP v1; OpenAI HTTP v1; Kokoro ONNX v1.0
selected_sdk: none; existing reqwest 0.13.5 and ort 2.0.0-rc.12
---

# Voice settings and preview (P8-023)

## Scope and authority

Change voice/model and supported synthesis controls for the existing adapters.
Store named voices locally and preview unsaved edits using the same adapter as
ordinary speech. No provider account mutation, voice cloning or new destination.

Official sources fetched on 2026-10-18 UTC, in discovery/contract/source order:

- https://elevenlabs.io/docs/llms.txt
- https://elevenlabs.io/docs/api-reference/text-to-speech/stream
- https://elevenlabs.io/docs/api-reference/voices/settings/get-default
- https://elevenlabs.io/docs/eleven-creative/playground/text-to-speech
- https://raw.githubusercontent.com/elevenlabs/elevenlabs-python/main/src/elevenlabs/types/voice_settings.py
- https://elevenlabs.io/docs/changelog
- https://developers.openai.com/llms.txt
- https://developers.openai.com/api/reference/resources/audio/subresources/speech/methods/create
- https://raw.githubusercontent.com/openai/openai-python/v3.35.1/src/openai/types/audio/speech_create_params.py
- https://github.com/openai/openai-python/releases/tag/v3.35.1
- https://raw.githubusercontent.com/remsky/Kokoro-FastAPI/v0.2.4/api/src/structures/schemas.py
- https://huggingface.co/onnx-community/Kokoro-82M-v1.0-ONNX/raw/1939ad2a8e416c0acfeecc08a694d14ef25f2231/README.md

Kokoro's missing repository llms index and native dependency/license evidence are
recorded in [embedded speech](embedded-speech.md). No dependency upgrades or
relevant endpoint deprecations were observed. The ElevenLabs SDK main branch is
corroborating evidence, not a versioned dependency or controlling specification.

## Contract

ElevenLabs uses the existing xi-api-key authenticated streaming POST. Its
`voice_settings` object overrides settings for one request, without editing the
account's voice. Stability and similarity are 0–1. Older supported v2 models also
accept speed 0.7–1.2, style 0–1 and speaker boost. **The model-specific product
guide says v4/v4 Turbo support only stability and similarity; v3 omits speed and
similarity.** This narrower statement controls over the same page's generic FAQ
claim that speed works on every model. Unknown models receive no editable controls.
V3 stability is conservatively offered at 0, 0.5 and 1; other controls are hidden.

OpenAI speech accepts top-level `speed` 0.25–4, default 1, alongside existing
input/model/voice/MP3 fields and Bearer authentication. Kokoro-FastAPI v0.2.4 has
the same speed range. JARVIS exposes speed for the documented OpenAI models and
the Kokoro model; arbitrary compatible-server models get no assumed controls.
Compatibility does not establish arbitrary servers' implementation fidelity.

The pinned native Kokoro graph takes speed as one float32 tensor. JARVIS chooses
a conservative 0.5–2 range and verifies actual audio duration changes. Its two
already pinned voice assets are the picker catalog. No engine offers a verified
pitch control here, so pitch is not accepted. No upstream code is copied.

## Boundary, limits and privacy

The library contains names, provider IDs, model/voice IDs and settings, never
keys, file paths or endpoints. Only startup-configured providers can be selected.
Validation rejects unsupported controls instead of dropping them. Preview is an
authenticated explicit request with at most 300 characters, a single concurrent
preview, existing audio/timeout bounds, and no fallback to another voice. Text
goes only to the selected provider; local synthesis stays local. Browser audio
uses temporary blob URLs, revoked on replacement/close, with no audio persistence.
Cloud previews incur the same account charges/retention as ordinary synthesis.
No pagination, new scopes, webhooks, retry/idempotency protocol or new SDK is added.

## Discriminating verification

Wire fixtures inspect the chosen voice/model and exact nondefault controls;
v4 speed and local stability must fail before network/native work. Test stale
revision conflicts, malformed persisted state, missing providers, invalid defaults,
unauthenticated access, restart persistence and preview failure without fallback.
Real native previews must decode, play in a browser, change voice and change
duration with speed. Test that saving changes subsequent console speech and that
cancel/close discards late audio and frees preview resources.

## Remaining boundaries

Telegram currently has no audio delivery. Phone calls run in ElevenLabs Agents,
not the sentence-synthesis adapter. Those channel assignments must be shown as
unavailable, and rejected by the API, until their delivery paths consume the
library. Merely storing unused preferences would not meet P8-023 acceptance.

## Live Windows evidence (2026-10-18)

`scripts/accept-voice-settings.mjs --run-live` passed with the real `local-tts`
daemon, a disposable profile, already pinned assets and downloads disabled.
Edge 155.0.4283.62 played an unsaved George preview through `playing` and `ended`
events. The same sample lasted 5.375 seconds at speed 0.8 and 2.975 seconds at
speed 1.4. Saving the edited voice changed ordinary console synthesis immediately.
The saved library survived a real daemon restart; a stale revision returned 409.
An empty saved library also survived restart and could be re-enabled live.

The test first exercised failed-primary fallback, then proved that draft preview
did not call that primary or silently fall back. Desktop and 390-pixel mobile
screenshots were inspected, with no horizontal overflow or browser errors. The
disposable daemon was stopped. Local evidence is in
`target/voice-settings-acceptance/profile-32PcIC/verification.json`, alongside
screenshots and sample WAV files; these generated artifacts are not committed.

Adapter wire tests check edited model, voice and control payloads. Runtime tests
cover durable save/reopen, stale or external edits, corrupt state, unknown
providers, unavailable controls and unauthorized requests. Console tests cover
draft retention, cancelled/late previews, stale-save recovery and save completion
after navigation. Final local checks passed: 3,288 workspace tests (7 opt-in
tests ignored), all 154 console tests, 8 documentation/acceptance-guard tests,
formatting, strict workspace/all-target/all-feature Clippy, documentation links
and all-feature dependency policy checks. The 97 settings tests were rerun after
disabling the complete editor while a reload is pending.

These checks do not establish live acceptance on macOS/Linux,
Telegram audio or ElevenLabs Agents call voices.

## Main-profile rollout (2026-10-18)

Both release executables were built with `local-tts`, and the main profile was
started using the normal CLI. The configured ElevenLabs default and embedded
Kokoro backup were preserved. The pinned George preset was copied into the
existing local cache after verifying its expected SHA-256, so either preset can
run with downloads disabled. No provider credentials or saved voice choices changed.

The main-profile George preview at speed 1.15 returned 218,444 WAV bytes and
played to completion in Edge (4.55 seconds). Edited and unedited ElevenLabs
previews both returned the same provider key-rejection error. Cloud controls
therefore retain wire-fixture verification; successful live cloud preview awaits
a working provider credential. Ordinary speech did fall back to embedded Kokoro
after that real cloud failure, returning 120,044 WAV bytes with the local/backup
response headers. Preview left the saved library unchanged and produced no
browser errors. Local evidence is in
`target/voice-settings-main-verification/verification.json` and `settings.png`.
