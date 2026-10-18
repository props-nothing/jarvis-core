# ADR-0245: Speech fallback selects a provider before audio starts

Status: Accepted

Date: 2026-10-18

## Context

The owner selected P8-018–020: preserve natural speech during an ElevenLabs outage
and permit an independently running local speech server. ADR-0138 currently couples
the daemon to one hosted adapter.

## Decision

Amend ADR-0138 with a provider-neutral `SpeechProvider` and bounded `SpeechStream`.
The daemon composes an explicit ordered list from `daemon.speech.providers`.
Without that list the legacy ElevenLabs settings remain valid. Each provider has
a unique public id, voice/model and private optional key-file reference. An explicit
list replaces the legacy speech choice, while legacy phone credentials remain usable.

Fallback happens only before the first nonempty audio chunk. Per-provider breakers
skip outages for 30 seconds and rejected credentials/exhausted credit for ten minutes;
one half-open request probes recovery. Cancellation releases that probe. Mid-stream
failure ends the response and opens the breaker, never splices another voice into it.
The console carries the selected provider id across sentence requests and serializes
selection before prefetch. A configured provider id is a preference, never a URL or
authority grant. Provider status exposes metadata/health and last selected id only.

The OpenAI-compatible adapter accepts an explicit API base ending in `/v1`, requests
MP3 binary audio, disables redirects/proxies, bounds time/output and permits plain
HTTP only on loopback. No local server or model is installed automatically.
`jarvis doctor --live` probes every configured provider with fixed text. Configuration
errors fail startup; remote outages degrade speech while the daemon stays usable.

## Consequences

Existing setup remains ElevenLabs-specific under ADR-0214. This is local-server
support, not embedded synthesis or a changed first-run decision. Browser speech
remains the last resort and the console labels a backup voice. Raw provider errors,
keys and base URLs are excluded from API status and failure messages. A fallback
may incur a second charge if an upstream failure occurs after it accepted the text.

See the [research contract](../research/integrations/speech-fallback.md) and
[configuration guide](../user/speech.md). P8-020's hosted OpenAI and local Kokoro
live acceptance was recorded on 2026-10-18, including daemon fallback, recovery
and browser playback. Embedded engines and first-run changes remain separate work.
