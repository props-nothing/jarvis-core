---
integration: elevenlabs
status: researched
last_verified: 2026-10-15
owners: []
selected_spec_version: current hosted API as of verification date
selected_sdk: undecided
---

# ElevenLabs

## Phone disclosure repair (2026-10-15)

Live official sources checked on 2026-10-15 (UTC): https://elevenlabs.io/docs/llms.txt,
https://elevenlabs.io/docs/eleven-agents/api-reference/integrations/twilio/outbound-call.md,
https://elevenlabs.io/docs/eleven-agents/customization/personalization/overrides.md,
https://elevenlabs.io/docs/api-reference/authentication.md and https://elevenlabs.io/docs/changelog.md
(latest entry October 12). Hosted v1 and direct HTTP on the pinned workspace reqwest remain selected; no SDK upgrade.
No relevant first-message/prompt override or `xi-api-key` auth change observed. Overrides replace the configured values;
first-message and prompt overrides require their agent security toggles. The warning preferring dynamic variables does not
remove override support. Other newly documented telephony/tool options are not adopted by this repair.

JARVIS decision: restore the existing ADR-0201 disclosure contract for calls to non-owners. Rust always prefixes the
first message with its AI identity and instructs the agent to identify itself as an AI if asked. Owner-call wording stays
unchanged. No new scopes, provider request fields, retries, timeouts, quotas, idempotency, webhook, retention or data flows
are introduced; a failed override must not result in a call without disclosure. The provider still receives the approved
number and bounded purpose; no live call is needed to test composition.
Cheapest falsifier: a purpose claiming to be human and `phone_opening_line = false` must still yield an AI-disclosing first
message in the stand-in request. Existing refusal tests protect the agent-read/prompt-update boundary. No real recipient
is called during this task; actual spoken output remains a live-provider check, not a claim from these fixtures.

## Setup refresh (2026-10-14)

Live official sources re-read: https://elevenlabs.io/docs/llms.txt,
https://elevenlabs.io/docs/api-reference/authentication.md,
https://elevenlabs.io/docs/api-reference/text-to-speech/stream.md,
https://elevenlabs.io/docs/api-reference/voices/get.md and
https://elevenlabs.io/docs/changelog.md (latest entry October 12). No relevant TTS auth/stream change observed.
The adapter remains direct HTTP on the pinned workspace reqwest; hosted v1, no SDK upgrade.

**JARVIS decision, not a provider guarantee:** the owner selected default voice ID `liN4AsrhF3CylqMDWK6p`.
It is a public voice identifier, never an API key. Its availability/name/permissions on an arbitrary account are
unverified. New CLI setup requires an ElevenLabs key and one successful short TTS request using the chosen voice/model
before saving. Existing profiles, manual headless configuration and browser fallback remain usable without it.
The fixed test sentence leaves the machine and may consume credits; no personal text is sent. Only `xi-api-key` auth
and TTS access are required; phone/agent access is needed only if calls are enabled later. No retries or silent switch
to another voice on setup failure. Account quota/rate limits and voice access can block setup with an actionable refusal.
No webhook, pagination or idempotency contract changes. Existing text/audio limits and no-redirect policy remain.

Cheapest falsifying test: a mock 401, empty stream, invalid model/voice or network failure must leave a fresh profile
without saved setup/keys. Confirm the exact default ID in the outgoing TTS path and settings metadata. A real short
synthesis on the owner's account with this voice is still required for live availability evidence; not performed here.

## Scope

Architecture research for JARVIS-brain Custom LLM mode (both transports), JARVIS-as-MCP-server mode, inbound/outbound telephony, SIP, turn-taking configuration, and post-call event reconciliation. No SDK or account plan is selected, and no live call was made.

The 2026-09-21 refresh added the Speech Engine transport and the turn-taking configuration surface. Both were absent from the first pass, which read only the SSE compatibility pages.

## Official Sources

| Source | URL/version | Accessed | Purpose |
| --- | --- | --- | --- |
| Documentation index | https://elevenlabs.io/docs/llms.txt | 2026-09-20 | official page/API discovery and AI-agent instructions |
| Custom LLM | https://elevenlabs.io/docs/eleven-agents/customization/llm/custom-llm.md | 2026-09-20 | compatible endpoints, SSE, tools, extra body |
| Custom LLM via Twilio | https://elevenlabs.io/docs/eleven-agents/phone-numbers/twilio-integration/custom-llm-integration.md | 2026-09-21 | Speech Engine bridge: WS brain server, μ-law, signed URL, shared secret |
| Speech Engine | https://elevenlabs.io/docs/api-reference/speech-engine/speech-engine-upstream.md | 2026-09-21 | brain WebSocket contract and formats |
| Conversation flow | https://elevenlabs.io/docs/eleven-agents/customization/conversation-flow.md | 2026-09-21 | turn timeout, soft timeout, turn eagerness, max duration, interruptions |
| MCP integration | https://elevenlabs.io/docs/eleven-agents/customization/tools/mcp.md | 2026-09-20 | transports, approval modes, restrictions |
| SIP trunking | https://elevenlabs.io/docs/eleven-agents/phone-numbers/sip-trunking.md | 2026-09-20 | inbound/outbound SIP and security |
| Twilio outbound call | https://elevenlabs.io/docs/eleven-agents/api-reference/integrations/twilio/outbound-call.md | 2026-09-20 | endpoint and payload |
| Post-call webhooks | https://elevenlabs.io/docs/eleven-agents/workflows/post-call-webhooks.md | 2026-09-20 | HMAC, events, retry and payload behavior |
| OpenAPI | https://elevenlabs.io/docs/openapi.json | 2026-09-20 | machine-readable HTTP contract; not yet vendored/generated |
| AsyncAPI | https://elevenlabs.io/docs/asyncapi.json | 2026-09-20 | machine-readable WebSocket contract; not yet vendored/generated |

The docs index instructs agents to append `.md` for clean Markdown, use section `llms.txt` indexes, and exposes an official docs MCP server. Implementation still uses the exact product/API pages and schemas rather than the index summary alone.

## Verified Contract

### Custom LLM

- Supports OpenAI-style `POST /v1/chat/completions` and `POST /v1/responses` custom endpoints.
- Both stream with `Content-Type: text/event-stream`.
- Chat Completions chunks use `data: <json>\n\n` and finish with `data: [DONE]\n\n`.
- Responses requires at least `response.output_text.delta` and `response.completed` events, then `[DONE]`.
- Requests can contain standard OpenAI-format function tools for ElevenLabs system operations.
- Optional custom values arrive under `elevenlabs_extra_body`; these are caller/provider input and require an allowlist.
- Reasoning summaries, when enabled, are separate from final output. JARVIS must not expose hidden chain-of-thought.

### Speech Engine Transport (second custom-brain path)

Verified 2026-09-21. This is an alternative to the OpenAI-compatible endpoints, not a layer on them. Choosing one per deployment is required; running both against one number produces two brain sessions for one call.

- The brain runs on a **WebSocket server owned by JARVIS**. ElevenLabs connects to it to deliver transcripts and receive generated text.
- A **conversation WebSocket** is hosted by ElevenLabs; the telephony bridge connects to it using a **per-call signed URL**, so the bridge never holds the raw API key.
- The brain endpoint is protected by a **shared secret sent as a request header on the WS upgrade**. Without it, anyone who learns the URL can impersonate the provider.
- Audio is `ulaw_8000` in both directions. Twilio Media Streams speaks the same format, so a Twilio bridge relays base64 payloads with no transcoding.
- Observed event names on the conversation socket: `conversation_initiation_client_data` (bridge→provider, sent on stream start), `user_audio_chunk` (bridge→provider), `audio` (provider→bridge), `interruption` (provider→bridge), `ping`/`pong` keepalive.
- Provider `interruption` maps to a transport-level buffer clear. That is a playback action, not evidence about caller intent, and must not be recorded as an interruption by the caller.
- Documented trade-off: one WebSocket instead of a new HTTP connection per turn, described as a possible latency improvement. It is not a measured figure.
- Documented cost: the bridge adds two network hops on top of model time-to-first-token.
- Documented risk to carry into JARVIS: spoken input from a phone call is untrusted user input and must be validated before it influences tool calls or writes. This matches the existing JARVIS requirement that provider input is untrusted data.

### Turn-Taking Configuration (verified 2026-09-21)

These settings change what the caller hears and are part of JARVIS voice-session policy, not adapter internals. See [ADR-0010](../../adr/0010-model-based-turn-detection.md).

| Setting | Range / Values | Default | Notes |
| --- | --- | --- | --- |
| `conversation_config.turn.turn_timeout` | 1–30 s | — | wait during caller silence before taking the next turn |
| `conversation_config.turn.turn_eagerness` | `patient` \| `normal` \| `eager` | — | how quickly the assistant treats a pause as a turn opportunity |
| `conversation_config.turn.soft_timeout_config.timeout_seconds` | 0.5–8.0 | `-1` (disabled) | filler spoken while a slow reply is generated; recommended 3.0 |
| `conversation_config.turn.soft_timeout_config.message` | 1–200 chars | `"Hhmmmm...yeah."` | static filler; may instead be LLM-generated with a static fallback |
| `conversation_config.conversation.max_duration_seconds` | 60–7,200 | 600 | whole-conversation limit, independent of turn timeouts |

- Interruptions are a client-event selection: enabled, or disabled when complete delivery matters (disclaimers, safety instructions).
- The provider reports that the assistant may begin speaking after receiving enough words and a comma rather than a complete sentence. That means a **reply can be committed before its final clause exists**; JARVIS must not treat the first streamed segment as the complete response.
- The provider describes a **proprietary turn-taking model**. Unlike the turn settings above, it is not inspectable or swappable, so the effective boundary cannot be reproduced outside the provider. Record the provider's reported values as evidence and record the caller-interaction consequence in the call's latency report.
- WhatsApp message conversations have a separate default 15-minute inactivity timeout.

### MCP Consumer Mode

- ElevenLabs agents can connect to external MCP servers over SSE and Streamable HTTP.
- Re-read 2026-10-10: the dashboard form takes a *Server URL*, an optional *Secret Token* described as "Authorization header value", and optional extra headers; the connection is made from ElevenLabs' cloud, so JARVIS must be reachable from the internet (`cloudflare-tunnel.md`, `ADR-0173`). Whether the token is sent with or without `Bearer` is not stated; JARVIS accepts both.
- Workspace MCP use is opt-in and disabled by default.
- Server- and tool-level approval modes include always ask, fine-grained, and no approval.
- MCP is currently unavailable for Zero Retention Mode or HIPAA-required workspaces.
- JARVIS cannot delegate its final policy decision to ElevenLabs approval settings.

### Twilio Outbound Call

- `POST https://api.elevenlabs.io/v1/convai/twilio/outbound-call`.
- Required fields: `agent_id`, `agent_phone_number_id`, and `to_number`.
- Optional conversation initiation data includes dynamic variables, a user ID, custom LLM extra body, and bounded configuration overrides.
- Success response may include `conversation_id` and `callSid`.
- Provider acceptance does not prove a person answered; later events reconcile outcome.

### SIP

- Supports inbound and outbound calls through an existing SIP trunk.
- Authentication can use digest credentials or source-IP ACL; docs recommend digest over IP-only allowlisting.
- Production should use TLS 1.2+ signaling and SRTP media encryption when supported; UDP signaling is documented as experimental.
- Audio uses G711 8 kHz or G722 16 kHz at the SIP boundary.
- Custom inbound `X-` headers become dynamic variables and are untrusted context.
- Static-IP SIP endpoints are an enterprise feature; distributed/default endpoints may use changing source IPs.

### Post-Call Webhooks

- Event families include `post_call_transcription`, `post_call_audio`, and `call_initiation_failure`.
- Validate HMAC from the `ElevenLabs-Signature` header against the raw body and shared secret; official SDK helpers also validate timestamp.
- Audio callbacks can use chunked transfer and contain base64 MP3; they can be large.
- Repeated failures can disable a webhook; successful handlers return HTTP 200 promptly.
- Call-initiation failure metadata differs for Twilio and SIP.
- Events can be retried, so durable deduplication is required.

## JARVIS Mapping

- Preferred mode: ElevenLabs transports audio/telephony while `/v1/responses` creates a scoped JARVIS run.
- Compatibility mode: `/v1/chat/completions` maps to the same application command.
- Alternative brain transport: the Speech Engine brain WebSocket maps to the same application command as the HTTP endpoints, selected per deployment and recorded on the session.
- Turn and interruption policy map into the voice-session contract; the provider's effective values are stored as evidence next to the requested ones.
- Provider `interruption` maps to a transport clear, not to a caller-interruption event.
- Specialized mode: an ElevenLabs-owned agent gets a dedicated MCP client identity and explicit tool allowlist.
- Provider IDs map to `voice_sessions` and `voice_calls`; they never replace JARVIS IDs.
- `voice.call.start/status/end` are risk-classified JARVIS tools.
- ElevenLabs system tools map to a narrow voice-control adapter; general JARVIS tools remain behind policy.
- API keys and webhook secrets live behind `SecretRef`.
- Transcript/audio storage follows JARVIS retention and user policy; absence from JARVIS does not imply provider deletion.

## Decisions

1. Implement Responses compatibility before Chat Completions because it is the newer supported format.
2. Keep both endpoints behind voice-session authentication rather than exposing a generic unauthenticated model proxy.
3. Treat Custom LLM mode as the default personal-JARVIS voice architecture.
4. Keep MCP mode optional and least privilege.
5. Process verified webhooks through the durable event inbox.
6. Do not enable call recording or audio webhooks by default.
7. Implement the HTTP path first and treat Speech Engine as a later transport behind the same application command, because it adds a bridge process and a second credential (shared secret) without changing JARVIS ownership.
8. Set turn-taking policy explicitly per session rather than relying on provider defaults, and persist the effective values.
9. Treat provider `interruption` frames as transport control only; caller interruption is inferred from evidence, not from the provider's buffer flush.

## Rejected Alternatives

- ElevenLabs-owned memory as canonical memory: couples identity and retention to the voice vendor.
- Direct ElevenLabs MCP access to every JARVIS tool: violates least privilege and policy ownership.
- Caller number as authentication: vulnerable to forwarding/spoofing and insufficient for sensitive actions.
- Blind outbound-call retries: can place duplicate calls.
- Relying on provider turn-taking defaults: the values are documented to change and the turn model is proprietary, so a behaviour change would be undetectable from the JARVIS side.
- Treating the provider's `interruption` frame as a caller interruption: it is a playback clear and is emitted for non-speech input, so it would cancel replies on a cough.
- Running the SSE endpoints and the Speech Engine brain simultaneously for one number: produces two brain sessions for a single call.

## Verification Plan

- Generate/validate request fixtures from current OpenAPI without exposing generated vendor types to domain code.
- Contract-test both SSE framings, end markers, cancellation, malformed tool calls, slow first token, and disconnect.
- Contract-test the Speech Engine WS framing against a scripted brain: `conversation_initiation_client_data`, `user_audio_chunk`, `audio`, `interruption`, `ping`/`pong`, and a wrong or absent shared secret on the upgrade.
- Verify a short-lived voice-session credential cannot bind another user/workspace.
- Test MCP discovery and denied/high-risk calls with an independent ElevenLabs test agent when available.
- Mutate raw webhook bytes/signature/timestamp and replay valid events.
- Test out-of-order post-call events, no-answer/busy/unknown, and provider timeout after call acceptance.
- Place one opt-in inbound and outbound live call with recording disabled and assert correlated JARVIS/provider IDs.
- Record a real call's turn behaviour and check: a pause-heavy request is not cut off, a backchannel does not cancel a reply, and a real interruption does.
- Assert that a reply committed early (after the provider's early-speech trigger) is not surfaced as complete while more text is still arriving.

Cheapest discriminator for the central design: run a local fixture that sends an official-shape ElevenLabs Custom LLM request and rejects it unless a valid pre-created voice session maps to the expected workspace.

## Unresolved Questions

- Exact custom-server authentication headers/configuration and rotation workflow. Blocks public endpoint implementation.
- Whether the Speech Engine brain channel supports the same system-tool set as the HTTP path, and how tools are declared over the WS. Blocks Speech Engine tool parity; the HTTP path is unaffected.
- Whether a turn-boundary confidence is exposed to the brain over either transport. Blocks a measured turn-detection metric on this provider; if absent, the metric is recorded as unsupported rather than estimated.
- Whether `interruption` is emitted for backchannels and non-speech noise. Blocks correct false-interruption handling on this provider.
- Whether the selected account supports required concurrency, data residency, and outbound/SIP features. Blocks live telephony, not local voice contracts.
- Provider retry/ordering guarantees for every webhook event. Requires current webhook reference and live fixtures before reconciliation ships.
- Legal disclosure, consent, quiet-hour, and call-recording requirements by deployment jurisdiction. Blocks production outbound calling defaults.
- Selected Rust HTTP implementation versus official SDK usage. Resolve after inspecting current SDK coverage and license.

## Verification Log

| Date | Versions checked | Relevant change or no-change evidence | Researcher |
| --- | --- | --- | --- |
| 2026-09-20 | current hosted docs, OpenAPI/AsyncAPI links | Initial architecture verification; no implementation claim | GitHub Copilot |
| 2026-09-21 | `llms.txt` index, Speech Engine + Twilio custom-LLM page, conversation-flow page | Added Speech Engine transport (WS, μ-law, signed URL, shared secret) and turn-taking settings; recorded that the turn model is proprietary and the eager-response behaviour commits replies early | GitHub Copilot |

## Text to speech for the console voice (added 2026-10-06, `ADR-0138`)

Scope: the one-way `text -> audio` call used by `crates/jarvis-voice` so the browser console can speak in a neural voice.
Speech to text, agents, telephony and the Custom LLM paths above are unchanged and out of scope here.

Sources (accessed 2026-10-06): `https://elevenlabs.io/docs/llms.txt`;
`https://elevenlabs.io/docs/api-reference/text-to-speech/stream.md` and `.../convert.md`;
`https://elevenlabs.io/docs/api-reference/authentication.md`; `https://elevenlabs.io/docs/overview/models.md`;
`https://elevenlabs.io/docs/eleven-api/quickstart.md`.

Verified:

- `POST https://api.elevenlabs.io/v1/text-to-speech/{voice_id}/stream`, JSON body `{ "text": string (required),
  "model_id": string (optional, default `eleven_multilingual_v2`), "voice_settings": {...} (optional) }`; the response is
  the audio. `output_format` is a query parameter whose default `mp3_44100_128` is available on every tier. Regional servers
  exist (`api.us.`, `api.eu.residency.`, `api.in.residency.`, `api.sg.residency.elevenlabs.io`).
- Auth is an `xi-api-key` header. The docs say the key is a secret that must not be exposed in client-side code (browsers,
  apps); single-use tokens exist for some endpoints but are not used here.
- Models listed for text to speech: `eleven_v4` (10,000 characters), `eleven_v4_turbo` (about 100 ms, expressive,
  real time), `eleven_v3`, `eleven_v3_conversational`, `eleven_multilingual_v2` (10,000), `eleven_flash_v2_5`
  (about 75 ms, 40,000). A 422 carries `detail` validation errors.
- The quickstart's example voice is "George" (`JBFqnCBsd6RMkjVDRZzb`).

JARVIS decisions: the daemon holds the key (read from a file named by `daemon.speech_api_key_ref`, like the model key) and
exposes `POST /api/v1/speech` behind the local credential, so no browser sees it; default model `eleven_v4_turbo`, default
voice George, both overridable; no `voice_settings` sent (the voice's stored settings apply); text capped at 1,500
characters per request and audio at 8 MiB; 401/403 map to "key rejected", 402/429 to "over limit", all other statuses
to a generic provider failure, and no error carries the key or the provider's body.

Unresolved: regional (residency) endpoints are not configurable yet; live behaviour against a real account (latency,
the `eleven_v4_turbo` model id on a given plan) was unverified when this was written; it has since been checked against a real account (see the review below).

### Streaming and latency review (added 2026-10-06, after a live account was available)

Prompted by two symptoms in the console: the face was "speaking" before any sound, and the text appeared well before the voice.
Sources (accessed 2026-10-06): `https://elevenlabs.io/docs/eleven-api/concepts/latency.md`,
`https://elevenlabs.io/docs/eleven-api/guides/how-to/best-practices/latency-optimization.md`,
`https://elevenlabs.io/docs/overview/models.md`, `https://elevenlabs.io/docs/api-reference/text-to-speech/stream.md`.
(`.../concepts/audio-streaming` timed out and was not read; the streaming facts below come from the other four and from measurement.)

Facts from the documentation:

- Three TTS transports: the regular endpoint (one complete file), the **streaming** endpoint (`/stream`, audio returned
  progressively, "recommended for cases where the input text is available up-front"), and the **WebSocket** endpoint
  (bidirectional, for text that is still being produced, such as LLM output; `auto_mode` removes chunk-schedule management).
- Streaming does not shorten model inference; it shortens time to first audio, because playback can start at the first chunk.
- Model latency: `eleven_flash_v2_5` about 75 ms, `eleven_v4_turbo` about 100 ms ("most emotive, real-time"),
  `eleven_v3_conversational` about 280 ms, **`eleven_v3` is the quality model and is not a real-time model**: "there is no way to
  get Eleven v3 quality at Flash speeds". Professional voice clones are slower than default, synthetic and instant-clone voices.
- `previous_text` / `next_text` / `previous_request_ids` carry intonation across separate requests (`previous_request_ids`:
  at most three). `output_format` defaults to `mp3_44100_128`; `optimize_streaming_latency` is deprecated.

Measured against the real account on 2026-10-06 (this machine, voice "Jarvis", a *generated* voice, a 70-character sentence,
two runs each; time to first byte / total):

| model | first byte | complete |
| --- | --- | --- |
| `eleven_v3` | 0.56 to 0.63 s | about 1.5 s |
| `eleven_v4_turbo` | 0.18 to 0.21 s | 0.61 to 0.65 s |
| `eleven_v3_conversational` | 0.22 s (3.0 s on a cold first call) | 0.8 s |
| `eleven_flash_v2_5` | 0.19 to 0.20 s | 0.29 to 0.31 s |

- `previous_text` is accepted by `eleven_v4_turbo` and **refused by `eleven_v3`** with HTTP 400 `unsupported_model`
  ("Providing previous_text or next_text is not yet supported with the 'eleven_v3' model").

What this found in JARVIS (all fixed, `ADR-0138` amended):

1. The daemon called `/stream` but **collected the whole response** before answering, so the endpoint's one advantage was lost
   and the browser waited for the full synthesis of each group. It now passes chunks through as they arrive (`SpeechStream`).
2. The page spoke only **after the whole answer was written**, in groups of up to 140 then 500 characters, and held
   `S.speaking` true from the moment it asked for audio. It now speaks each sentence as soon as it is complete, plays the audio
   progressively (Media Source, with a whole-blob fallback), keeps the next two pieces loading, and the face is "speaking" only
   while sound is playing (it reads "preparing voice" while it waits).
3. Setting the model to `eleven_v3` (a guess made while chasing a stale-tab problem) made speech about 2.4 times slower for no
   reason; `eleven_v4_turbo` is the right default and the setting was put back.

Adopted: HTTP streaming per sentence with `previous_text` for continuity (omitted for `eleven_v3*`). Not adopted yet: the
WebSocket input-streaming endpoint, which would let the voice start from the first few words and keep one voice context for
the whole answer. It needs a WebSocket client dependency and its own contract tests; with measured first-sentence latency of
about 0.4 s end to end it is not the bottleneck, so it stays a candidate (`P3-040`).

## Phone calls placed by JARVIS (added 2026-10-11, `P8-007`, `ADR-0200`)

Research for the first telephony slice: JARVIS asks an ElevenLabs agent to ring the owner, and reads how the call went. The owner had already
connected an ElevenLabs agent to JARVIS's MCP endpoint (Mode B above) and let it call them, so the voice, the number and the agent exist; what was
missing is JARVIS deciding to call, recording it, and not asking twice.

### Sources read on 2026-10-11

| Source | URL | Used for |
| --- | --- | --- |
| Documentation index | https://elevenlabs.io/docs/llms.txt (219 KB, fetched with curl; the page tool timed out twice) | page discovery |
| Outbound call via Twilio | https://elevenlabs.io/docs/eleven-agents/api-reference/integrations/twilio/outbound-call.md | request, response, `telephony_call_config` |
| Outbound call via SIP trunk | https://elevenlabs.io/docs/eleven-agents/api-reference/sip-trunk/outbound-call.md | same shape, `sip_call_id` in the answer |
| List phone numbers | https://elevenlabs.io/docs/eleven-agents/api-reference/phone-numbers/list.md | `provider`, `phone_number_id`, `assigned_agent` |
| Get conversation details | https://elevenlabs.io/docs/eleven-agents/api-reference/conversations/get.md | `status`, `transcript`, `metadata`, `analysis` |
| Overrides | https://elevenlabs.io/docs/eleven-agents/customization/personalization/overrides.md | what a per-call override needs |
| Dynamic variables | https://elevenlabs.io/docs/eleven-agents/customization/personalization/dynamic-variables.md | `dynamic_variables`, `system__` reserved prefix |
| MCP integration | https://elevenlabs.io/docs/eleven-agents/customization/tools/mcp.md | approval modes: Always Ask, Fine-Grained, No Approval |
| API authentication | https://elevenlabs.io/docs/api-reference/authentication.md | `xi-api-key`; keys can be limited by scope, quota and IP |
| Post-call webhooks | https://elevenlabs.io/docs/eleven-agents/workflows/post-call-webhooks.md | `ElevenLabs-Signature`, three event types, retry and auto-disable |

### Contract used

- **Place a call.** `POST /v1/convai/twilio/outbound-call` for a Twilio number, `POST /v1/convai/sip-trunk/outbound-call` for a SIP trunk number. Body: `agent_id`, `agent_phone_number_id`, `to_number` (required), `conversation_initiation_client_data` (`dynamic_variables`, optional `conversation_config_override.agent.first_message`) and `telephony_call_config.ringing_timeout_secs` (default 60; JARVIS sends 45). Answer: `success`, `message`, `conversation_id`, and `callSid` (Twilio) or `sip_call_id` (SIP).
- **Which endpoint.** The number's `provider` from `GET /v1/convai/phone-numbers` (`twilio`, `sip_trunk`, `exotel`). Exotel (and WhatsApp) numbers are refused: no endpoint was verified for them.
- **How a call went.** `GET /v1/convai/conversations/{conversation_id}`: `status` is one of `initiated`, `in-progress`, `processing`, `done`, `failed`; `metadata.call_duration_secs`, `metadata.termination_reason`; `analysis.transcript_summary`; `transcript[]` with `role` (`user`|`agent`), `message`, `time_in_call_secs`.
- **Authentication.** `xi-api-key` header, the same key the console voice uses. A key restricted to speech cannot place calls; the adapter reports 401/403 as "the key needs access to the agents platform".
- **Servers.** Regional hosts exist (`api.us.elevenlabs.io`, `api.eu.residency.elevenlabs.io`, `api.in.residency.elevenlabs.io`, `api.sg.residency.elevenlabs.io`). JARVIS uses the default host; an account with data residency would need a setting for the host (**not built**).
- **Overrides are off by default.** "For most fields, an error will be thrown if an override is provided when that field does not have overrides enabled." So JARVIS sends **dynamic variables only** (`jarvis_reason`, `jarvis_call_id`, `jarvis_urgent`) and sets the opening line (an override of `agent.first_message`) only when the owner switches `daemon.phone_opening_line` on, after enabling that override in the agent's Security tab.
- **Per-call MCP tool approval** is a setting on the ElevenLabs side (Always Ask, Fine-Grained, No Approval). JARVIS cannot see whether the agent asked: a call arrives the same either way. This is why a yes on the phone is a separate, narrow, opt-in rule (`ADR-0200`), not something inferred from the arrival of a tool call.
- **Not documented / not found:** an endpoint to hang up a live conversation (no `voice.call.end`), and the retry and ordering guarantees of the status values.

### Decisions

1. **Poll, do not receive webhooks, in this slice.** `GET conversations/{id}` every 10 seconds while a call is open works on a laptop with no public address. The post-call webhook needs a reachable address (the Cloudflare tunnel exists, `ADR-0173`), an HMAC check over the raw body and a durable inbox (`P8-008`); it stays open.
2. **No retry of a placement.** A timed-out request may have started the call. The result is `Uncertain`, the record stays open, and the next call is refused until it is settled.
3. **Transcript and audio are not stored by JARVIS.** The record keeps the provider's summary; the status tool reads the transcript from the provider when asked, fenced as data.

### Unresolved

- No live call has been made by this code. Unproven until then: that undeclared `dynamic_variables` are accepted for an agent that does not reference them; the exact `termination_reason` strings; how `in-progress` is reported while the phone is still ringing (JARVIS treats only `in-progress` as "answered").
- Legal disclosure, consent, quiet-hour and recording rules by jurisdiction (this slice calls only the owner, at the owner's request).
- Retention of transcripts and audio at ElevenLabs is the account's setting, not JARVIS's.

### Falsifying tests

The phone adapter and daemon tests use a stand-in that follows this contract: Twilio and SIP endpoints are chosen by the number's provider; an unsupported or unknown number places nothing; a refusal and a 422 carry the provider's reason without the key; an unusable answer to a placement is `Uncertain` and is sent exactly once; statuses never go backwards; a number off the owner's list is refused before anything is recorded. **An opt-in live smoke test is still owed:** with the owner's key, agent and number, press "Call me now" in Settings (Phone), answer, then read the call in `jarvis.voice.status`.

| Date | Sources | Change | Who |
| --- | --- | --- | --- |
| 2026-10-11 | the pages above | Added the outbound call, number list and conversation read contracts for the telephony slice | GitHub Copilot |
| 2026-10-12 | live API (docs host answered 451 for the list page; filters tested directly) | Added the live-conversation read used by the line guard (`ADR-0201`) | GitHub Copilot |

### Reading who is on the line (added 2026-10-12, `ADR-0201`)

- **List:** `GET /v1/convai/conversations?agent_id=…&call_start_after_unix=…&page_size=50` returns `conversations[]` (newest first) with `conversation_id`, `status`, `direction` (`inbound`/`outbound`), `conversation_initiation_source` (`twilio`) and `has_more`. Tested live: `agent_id` and `call_start_after_unix` filter as expected (a future time returns none). The official list page could not be fetched on 2026-10-12 (HTTP 451 from the docs host), so these filters are verified by use, not by the page, and should be re-checked against it.
- **Detail:** `metadata.phone_call` carries `direction`, `external_number` (the other party), `agent_number`, `phone_number_id`, `call_result` (null while an inbound call is open), `type` (`twilio`), `call_sid`. A conversation that is not a phone call has no `phone_call`. Observed on two real inbound calls and several outbound ones.
- **Inbound only works with the initiation webhook flag off.** The agent had `platform_settings.overrides.enable_conversation_initiation_client_data_from_webhook = true` with no webhook, and no inbound conversation existed from 2026-09-20 until the flag was switched off on 2026-10-12; the docs say a failed webhook can prevent the conversation from starting.
- **Not found:** a way to reject or hang up a conversation from outside it (`DELETE` removes the record), and a documented value for `status` while an inbound call is still ringing.

### Connecting JARVIS's tools to the agent (added 2026-10-12, `ADR-0204`)

Sources read on 2026-10-12: `https://elevenlabs.io/docs/llms.txt`, then `…/docs/api-reference/mcp/create.md`, `…/mcp/update.md`, `…/mcp/delete.md`, `…/workspace/secrets/create.md` and `…/workspace/secrets/delete.md` (each page's own `.md`).

- **Create secret:** `POST /v1/convai/secrets` `{type:"new", name, value}` → `{type:"stored", secret_id, name}`. **Delete:** `DELETE /v1/convai/secrets/{id}` (204; refused while a server uses it).
- **Create server:** `POST /v1/convai/mcp-servers` `{config:{url (https), name, transport: "SSE"|"STREAMABLE_HTTP", approval_policy: "auto_approve_all"|"require_approval_all"|"require_approval_per_tool", secret_token, description, response_timeout_secs 5–300, …}}` → `{id, config, metadata, dependent_agents}`. **List:** `GET /v1/convai/mcp-servers` → `{mcp_servers:[…]}`. **Delete:** `DELETE /v1/convai/mcp-servers/{id}`.
- **Update cannot change the address.** `PATCH /v1/convai/mcp-servers/{id}` accepts approval policy, timeouts, headers, `secret_token` (`{secret_id}`) and `auth_connection`, and **no `url`**. A new address is a new server. This is why a changing quick-tunnel address left the agent with dead tools.
- **How the agent uses a server:** `conversation_config.agent.prompt.mcp_server_ids` (a list that a `PATCH` of the agent **replaces**, so a caller adds to what it read).
- **The docs leave the secret's form vague** ("stored as a workspace secret or in-place secret"). Verified live on the owner's workspace on 2026-10-12 with a throwaway pair (secret `jarvis-api-check-delete-me`, dummy value; server on an `example.com` address): `secret_token: {"secret_id": "<id>"}` is accepted on create and read back in that form; both objects were deleted at once and the listing showed no leftover. A no-op `PATCH` of the agent with the list it already had left it unchanged. Servers the owner made in ElevenLabs' own page sign in with an `auth_connection` instead (`{"auth_connection_id": …}`), with `secret_token: null`.
- **Not run live:** `PATCH` of a server's `secret_token`, and the attach of a new server (the adapter's bodies match the documented schema and the no-op patch above; the stand-in tests pin them).
