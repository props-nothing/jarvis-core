# TODO.md: P6, P7 and P8 as first written

Replaced on 2026-10-10 by the revised sections in [TODO.md](../../TODO.md) under [ADR-0167](../adr/0167-capability-and-reach-come-from-open-protocols-before-bespoke-code.md). The task wording below is the original; the new sections say which of these were kept, merged, parked or superseded and why. `P6-011` (scheduled tasks) was done and stayed in TODO.md.

## P6: Events And Workflows (original)

- `P6-001` Define event envelope, source identity, schema version, causation/correlation IDs, dedupe key, visibility, and sensitivity.
- `P6-002` Implement transactional outbox/inbox, leasing, retry schedule, dead letters, and replay tooling.
- `P6-003` Implement timezone-aware schedules and deterministic next-run calculation across daylight-saving transitions.
- `P6-004` Define workflow, run, step, attempt, wait, approval, cancellation, and compensation records.
- `P6-005` Implement sequential and parallel steps, conditions, retries, timeouts, delays, event waits, and cancellation.
- `P6-006` Require idempotency or an explicit non-retryable classification for every effectful workflow step.
- `P6-007` Implement notifications and proactive suggestion budgets, quiet hours, dedupe, and user controls.
  > Status 2026-10-06: a pull view exists (`jarvis watch`, the console's waiting panel, the page title count); no push notification, quiet hours or budgets yet.
- `P6-008` Add crash-at-every-transition and duplicate-event tests; pass the Phase 6 gate.
- `P6-009` Define the skill-candidate and memory-nudge events in the envelope **before `P6-001` freezes it**: a complex task that completed, a repeated procedure worth proposing, and a nudge to persist knowledge. Each is an ordinary event with causation, correlation, and a dedupe key, and **none carries authority**: a candidate is a proposal (`ADR-0117` §4), decided by a person and not by the event.
- `P6-010` Implement delegation as a durable workflow step: a step whose effect is starting a child run, with the parent/child link recorded, the child's tool calls re-entering the policy gateway (`P7-003`), and parallel workstreams tested for single-effect on duplicate delivery and for cancellation. The effect class "starts a run" is named explicitly rather than folded into an existing one.
  > Status 2026-10-06: the starts-a-child-run effect exists as `jarvis.agent.delegate` (`P3-034`, ADR-0134) but not yet as a durable workflow step; a parent/child link is recorded only by the objective marker, and a crash mid-delegation is not resumed.

## P7: External Runtimes (original)

- `P7-001` Define runtime protocol v1: handshake, capabilities, start, event stream, tool request, input request, resume, cancel, settle, health, and errors.
- `P7-002` Implement process supervisor with resource limits, environment allowlist, crash backoff, health, log redaction, and orphan cleanup.
- `P7-003` Ensure external runtime tool requests re-enter the JARVIS policy gateway; disable unmediated native tools by default.
- `P7-004` Research and implement the current OpenClaw runtime or ACP integration.
- `P7-005` Research and implement an OpenAI Agents worker adapter.
- `P7-006` Implement a generic ACP adapter and conformance fixture.
- `P7-007` Add LangGraph only for a named use case that benefits from its checkpoint/interrupt model.
- `P7-008` Test crash isolation, cancellation, protocol skew, unavailable runtime fallback, and canonical audit ownership.
- `P7-009` Add delegate/subagent semantics to runtime protocol v1 **before it is frozen**: a start request that names a parent run, the child run's lifecycle and settlement, cancellation that propagates, and a bounded depth. Whether `agent_runs` gains a `parent_run_id` is decided here and recorded before the protocol ships, because the run table already exists and a child run's parentage cannot be added cheaply afterwards.
- `P7-010` Add a subagent delegation tool: an ordinary tool whose effect is "starts a run", classified with its own risk level and approval policy, so a delegated run is authorized like any other capability and a model cannot spawn work the actor was not granted.
  > Status 2026-10-06: delivered in a first form by `P3-034` (ADR-0134): `jarvis.agent.delegate`/`result`, depth limit one, at most four active, sub-agent answers fenced. Not yet classified with its own effect class, and not routed to external runtimes.
- `P7-011` Test delegation: depth limiting, crash isolation between parent and child, cancellation propagation, single-effect on duplicate delivery, and canonical audit ownership of a child run's steps.

## P8: Voice And Calls (original)

- `P8-001` Define voice provider, voice session, call, transcript, interruption, turn-detection, identity-evidence, and retention contracts. Turn events and their policy are fixed by ADR-0010.
- `P8-002` Research the current ElevenLabs `llms.txt`, Custom LLM, Speech Engine, MCP, telephony, turn-taking, webhook, privacy, and breaking-change docs; refresh the dated research record. (Record refreshed 2026-09-21 with the Speech Engine transport and turn-taking settings; implementation must re-verify before release.)
- `P8-003` Implement authenticated `/v1/responses` SSE compatibility for ElevenLabs with contract fixtures.
- `P8-004` Implement `/v1/chat/completions` SSE compatibility only after the Responses path; normalize ElevenLabs system tools without bypassing JARVIS policy.
- `P8-005` Issue short-lived scoped voice-session credentials and map verified caller/conversation identity to user and workspace.
- `P8-006` Expose a dedicated least-privilege MCP client profile for ElevenLabs-owned-agent mode.
- `P8-007` Implement Twilio and SIP outbound call adapters through provider-neutral `voice.call.*` tools.
- `P8-008` Verify HMAC signatures against raw webhook bodies, deduplicate events, return promptly, and process durable work asynchronously.
- `P8-009` Implement call records, consent/disclosure flags, transcript/audio retention, deletion, and redacted audit events.
- `P8-010` Add local wake word, push-to-talk, VAD, STT/TTS provider ports, echo handling, and interruption as client capabilities.
  > Status 2026-10-06: partly delivered ahead of order by `P3-036`: browser-native push-to-talk, wake word, spoken answers and interruption in the console. Still open: VAD, local (non-cloud) STT/TTS provider ports, echo handling, and a non-browser client.
- `P8-011` Pass inbound, outbound, voicemail, no-answer, interruption, latency, replay, privacy, and provider-outage tests.
- `P8-012` Implement the turn-detection and interruption port: canonical turn events with confidence, backchannel suppression, false-interruption resume, and a silence setting used only as a maximum cap.
- `P8-013` Implement a `flux`-class turn-detecting STT adapter and assert that a pause-heavy request is not cut off at the silence threshold.
- `P8-014` Report turn-detection latency separately from perceived response latency, and record discarded speculative work.
- `P8-015` Implement the Speech Engine brain WebSocket transport (shared-secret upgrade, per-call signed URL, `ulaw_8000`) behind the same application command as the SSE path.
- `P8-016` Place a boundary and placement ADR for any external agent framework used in the voice path (runtime worker under the supervisor versus a `jarvis-voice` adapter), before implementing it.
- `P8-017` Decide and record realtime session scope before any realtime transport work: whether JARVIS adopts a room as a session concept, which participants and media types a session may carry, whether data/RPC channels are permitted as transport-only, and the E2EE-versus-inspection policy. Record explicit exclusions where the answer is "no".
