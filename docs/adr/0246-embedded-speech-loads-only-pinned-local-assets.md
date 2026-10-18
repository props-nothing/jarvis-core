# ADR-0246: Embedded speech loads only pinned local assets

Status: Accepted
Date: 2026-10-18

Extended by [ADR-0248](0248-local-voice-design-saves-a-versioned-blend-recipe.md)
for bounded engine sharing across local blend recipes.

## Context

P8-022 removes the separate speech server requirement. The original sherpa-onnx
candidate links GPL eSpeak in 1.13.8; see the [research](../research/integrations/embedded-speech.md).
Rust remains the control plane, with replaceable speech adapters.

## Decision

The `local-tts` feature adds `embedded-kokoro` to the existing provider
chain. Kokoro runs in the daemon through ort and an English Rust phonemizer with
eSpeak disabled. Public model, voice and native runtime assets are versioned,
size-bounded and SHA-256 pinned. Download them on first use into the private profile
cache, never into source or the release package. An offline setting refuses missing
assets without attempting network. No arbitrary model, native library or URL is
accepted from configuration. A build without the feature rejects this provider at
startup with an actionable message.

As of the P8-024 continuation on 2026-10-18, both application packages enable
`local-tts` by default. A normal release rebuild must retain the capability of
the owner's embedded-speech profile. `--no-default-features` explicitly opts out;
the CLI disables its daemon dependency's defaults so that opt-out is effective.
The adapter crate retains its optional feature, and compilation does not download
models or native runtime assets. This changes packaging defaults, not asset trust
or first-use download policy.

Native inference is a trusted in-process dependency, not an OS sandbox. Check the
runtime archive and extracted library before loading, and hash the exact model bytes
passed to the runtime. Disable telemetry and raw runtime logging. Limit inference
concurrency and CPU threads; cancellation requests terminate the native run. The
asset preparation task may finish after the initiating request times out, but text
synthesis is cancelled and never detached indefinitely behind a request queue.

Return bounded mono PCM in a WAV container. The console uses its audio-blob path;
MPEG providers retain MediaSource streaming. Provider selection, cooldowns, sentence
affinity, approval boundaries and secret ownership remain in the existing layers.

## Consequences

First use requires a download and model initialization. Later speech works from
verified cached assets without a separate process or port. CPU/RAM and full-utterance
latency exceed a hosted streaming provider; measure them explicitly. English US/GB
are the initial supported languages. Names absent from the phonemizer dictionary may
be spelled. Native acceptance is required on each supported OS; the implementation
does not itself prove disconnected-network acceptance across all platforms.
