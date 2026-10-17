# Voice backup and local voices (2026-10-17)

[Home](../../README.md) / [Documentation](../README.md) / [Roadmap Phase 8](../../ROADMAP.md#phase-8-voice-and-telephony) / [Backlog P8](../../TODO.md#voice-and-channels-p8)

The owner asked how JARVIS can keep a natural voice when ElevenLabs is unavailable, and asked for both cloud provider backups and native local voice generation with editable, customisable voices in the style of ElevenLabs. This record keeps the plan behind `P8-018`–`P8-027` and its sources. The sources are project READMEs, a third-party review and a comparison site: under `docs/development/external-research.md` they are **leads, not authoritative sources**. Each provider, model or engine still needs its own dated record in `docs/research/integrations/` from official documentation before implementation.

Accessed 2026-10-17. Version, star and license figures are as reported on that date.

## Sources

| Source | URL | Kind |
| --- | --- | --- |
| VoiceStudio README | https://github.com/debpalash/VoiceStudio | project README |
| VoiceStudio review | https://andrew.ooo/posts/voicestudio-review-local-elevenlabs-alternative/ | third-party review |
| rea README | https://github.com/morluto/rea | project README |
| TTS model comparison | https://codesota.com/guides/tts-models | comparison site |
| `sherpa-onnx` crate 1.13.8 | https://docs.rs/sherpa-onnx | crate documentation |

## Where voice is today

- One adapter, `ElevenLabsSpeech` (`crates/jarvis-voice/src/lib.rs`), held as `Option<Arc<ElevenLabsSpeech>>` in `GatewayState` (`apps/jarvisd/src/gateway.rs`). `GET /api/v1/speech` hard-codes `"provider": "elevenlabs"` (`apps/jarvisd/src/speech_service.rs`).
- When ElevenLabs fails, `POST /api/v1/speech` answers 502 or 429 and the console falls back to the browser's `speechSynthesis` (`apps/jarvisd/src/hud/hud.js`, voice section). There is no second natural voice.
- Phone calls run through ElevenLabs Agents (`crates/jarvis-voice/src/phone.rs`), which is a whole voice agent, not text-to-speech.
- ADR-0214 requires an ElevenLabs key for guided setup and says to revisit that "if ... a verified local voice adapter is delivered".

## The two repositories the owner linked

- **rea** is a reverse-engineering MCP server (TypeScript, MIT). It has no speech features and is not a voice provider.
- **VoiceStudio** is a local desktop app offered as an ElevenLabs alternative (Python and Electron, AGPL-3.0, about 73k stars), with 17 TTS engines, cloning, voice design and transcription. It serves an OpenAI-compatible API on `127.0.0.1:3900/v1` (`/v1/audio/speech`, `/v1/audio/voices`, `/v1/models`, streaming via `stream_format`). Caveats: the application is AGPL; the default OmniVoice engine has CC-BY-NC (non-commercial) weights; the review measured slower than real time on a modest GPU and about 42 s cold start; it needs 10-20 GB of disk. It is useful as an optional local endpoint through `P8-020`, not as something JARVIS bundles.

## Candidate engines

| Engine | Code / weights license (as reported) | Role |
| --- | --- | --- |
| Kokoro-82M | Apache-2.0 / Apache-2.0 | First local default: small, CPU-capable, English voices including British male; voices can be blended by weight (`P8-024`). No cloning. |
| `sherpa-onnx` (Rust crate) | Apache-2.0 | Runs Kokoro, Kitten, Matcha, VITS, Pocket and ZipVoice in-process; offline (whole-sentence) synthesis; prebuilt native libraries downloaded at build time, overridable with `SHERPA_ONNX_LIB_DIR` (`P8-022`). |
| Piper (OHF engine) | GPL-3.0 / per voice | Very light and multilingual; talk to it over HTTP rather than linking it. |
| KittenTTS | MIT | Tiny low-end fallback. |
| Chatterbox, CosyVoice 3, VoxCPM2 | reported MIT, Apache-2.0, Apache-2.0 | Cloning candidates for `P8-025`; verify weights licenses in official sources. |
| F5-TTS, XTTS-v2, OmniVoice, Fish S2 | non-commercial or research weights | Not a default. |
| OpenAI `gpt-4o-mini-tts` | hosted | Cloud backup through `P8-020`. |
| Cartesia, Azure Neural, Google Cloud TTS, Deepgram Aura | hosted | Second native cloud provider candidates (`P8-021`). |

## Design

1. **`SpeechProvider` trait** in `jarvis-voice` (`P8-018`): `id`, `describe` (voice, model) and `open(text, previous_text) -> SpeechStream`. `SpeechStream` becomes a boxed stream of byte chunks with its real content type, and the `MAX_AUDIO_BYTES` cap moves into a shared wrapper. Text checks are shared. `SpeechError` gains a "worth trying another provider" classification: yes for unreachable, rejected key, out of credit or rate limited, 5xx and bad audio; no for empty or too-long text.
2. **`FallbackSpeech`** (`P8-019`) implements the same trait over an ordered list, so `speech_service.rs` barely changes. It switches only before the first chunk, which `speak` already waits for, so a `200` still means audio is coming. A per-provider circuit breaker skips a failed provider for about 30 s (about 10 min for a rejected key or exhausted credit), then lets one probe through. An optional reply id keeps one voice per answer. A mid-stream failure ends that sentence as today, the console speaks it with the browser voice, and the next sentence goes through the chain again.
3. **`OpenAiCompatibleSpeech`** (`P8-020`): `POST {base}/v1/audio/speech` with `model`, `voice`, `input` and `response_format: "mp3"` so the console's Media Source path is unchanged. Same safety rules as the ElevenLabs adapter. Plain `http` only for loopback hosts.
4. **Configuration**, backward compatible: an ordered `[[daemon.speech.providers]]` list (`kind`, `key_ref`, `base_url`, `model`, `voice`). Existing `speech_api_key_ref`, `speech_voice_id` and `speech_model` are read as a one-entry chain.
5. **Embedded local voice** (`P8-022`) behind a `local-tts` feature, models downloaded on first use and pinned by hash; the console gets a blob-playback path for WAV/PCM.
6. **Customisable voices** (`P8-023`–`P8-026`): a profile-stored voice library with per-voice settings, preview and a voice per channel; voice design by weighted blending of local voices or a text description where an engine supports it; cloning only from the owner's own sample (or attested consent), with the consent record, sample and derived files stored locally and deleted together; a pronunciation dictionary applied before any provider.

## Out of scope here

Speech recognition stays on browser recognition; local recognition remains the other half of `P8-010`. Call fallback (`P8-027`) needs a provider-neutral call path, which the Twilio and Deepgram records cover.
