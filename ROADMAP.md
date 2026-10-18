# JARVIS Roadmap

**A working local assistant; remaining work is reliability, verified integrations and distribution.**

[Home](README.md) / [Active acceptance backlog](TODO.md) / [Historical evidence](docs/history/README.md) / [Architecture](ARCHITECTURE.md)

## At a glance

Reviewed 2026-10-18. The [active backlog](TODO.md) controls current acceptance order. Earlier test transcripts and original phase wording remain in dated history; they do not override later accepted ADRs or represent current test failures.

| Area | Built now | Remaining acceptance or capability |
| --- | --- | --- |
| Core and control | Local Rust daemon/CLI, SQLite, durable approvals, audit, cancellation, a browser console, bounded/redacted logs, atomic shutdown admission and bounded cross-runtime draining | Native-platform acceptance for each new lifecycle change |
| Everyday work | Google mail/calendar, files/documents/XLSX edits, contacts, browser/search, projects and Telegram | Selected owner-account failure paths, OCR, provider fallback, image generation, PowerPoint/charts |
| Automation | Time-zone schedules, mail/file/webhook triggers, atomic webhook capacity reservations, quiet hours, durable native/coding parents and family Stop, per-call dispatch identity, shared delegation policy, contained-failure/audit tests, coordinated shutdown, routing and spend guards | Restart-safe event-to-run handoff, native-platform delegation acceptance and notification budgets |
| Memory | Owner-controlled admission, inspect/atomic correction/forget, task-aware lexical recall and granted-file full-text search | Owner-corpus answer/latency evaluation, automatic file/entity context and measured semantic retrieval |
| MCP accounts | Named stdio/HTTP secrets, persistent GitHub OAuth and generic pre-registered OAuth | Real provider/browser acceptance, durable direct-call history, automatic client registration, per-client credentials and hot registration |
| Coding | Settings-first in-place queued jobs, opt-in worktrees, full skill bundles, independent checks, managed previews, Windows Codex cancellation and a verified disposable website with explicit continuation | Fresh sign-in and other-provider/platform acceptance, scoped integration bridge and optional enforceable sandbox |
| Voice | Browser voice, configured cloud/local-server/embedded fallback, saved console voices with preview and supported controls; owner-reported working outbound calls | New setup requires ElevenLabs per owner choice; browser-independent voice client, Telegram/call voice assignments and broader telephony remain |
| Distribution | Apache-2.0, user installation/services, signed release manifests, hosted prebuilt-install acceptance, native-tested database recovery and compatible signed update/rollback with service readiness, a deployed signed website default with native CI and hosted byte/header verification | Native signing credentials, complete notices/SBOM, schema-changing updates and clean-machine onboarding/repair/uninstall |
| Scale | Local SQLite profile | Server, PostgreSQL parity, multiple devices and roles only for a named requirement |

<a id="current-acceptance-slice"></a>

## Latest acceptance and next work

The signed binary update/rollback slice is accepted under
[ADR-0242](docs/adr/0242-binary-activation-retains-evidence-and-requires-offline-ownership.md).
Release `b9d8773` passes signed activation, explicit native service readiness,
rollback preserving newer data, and recovery on Windows x86_64, macOS ARM64 and
Linux x86_64. Protocol 2 checks exact migration history as well as numeric schemas;
a real incompatible signed build is refused without replacing bytes. All 14 jobs in
[CI 39122471338](https://github.com/props-nothing/jarvis-core/actions/runs/39122471338)
and the five packages, four public-install checks and three update gates in
[Release 39123649427](https://github.com/props-nothing/jarvis-core/actions/runs/39123649427)
passed. See the [runbook](docs/operations/binary-update.md) and [bounded evidence](docs/research/integrations/binary-update.md).

The lifecycle continuation adds exact retained-image repair and guarded removal
under [ADR-0243](docs/adr/0243-repair-and-removal-preserve-installation-evidence.md).
All three [hosted native lifecycle jobs](https://github.com/props-nothing/jarvis-core/actions/runs/39126418196)
pass missing/damaged binary repair, repeated removal and data preservation.
[Release `a176e79`](https://github.com/props-nothing/jarvis-core/releases/tag/ci-a176e79843a22eae2802d97aafba74a0bf6e282f)
also passes all fourteen quality jobs, five packages, signed publication, four public
install checks and all three extended native lifecycle gates in
[Release 39127486126](https://github.com/props-nothing/jarvis-core/actions/runs/39127486126). The
owner will perform the [clean-machine self-test](docs/operations/clean-machine-lifecycle.md),
entering provider keys in hidden prompts. Those reports remain required.

`P12-001` still needs clean-machine onboarding/install/repair/uninstall, native
certificate signing and complete notices/SBOM. Hosted runners with provider-free
profiles do not close those gates. Schema-changing updates and release channels
also remain separate. [ADR-0241](docs/adr/0241-database-restores-require-offline-ownership-and-a-recovery-journal.md)
continues to govern separately invoked database restore; binary rollback never
rewinds newer canonical data.

The owner-selected [release verification and coding lifecycle](TODO.md#current-acceptance-slice-release-verification-and-coding-lifecycle-2026-10-16) is implemented and verified. Both `c48da888` and the lifecycle revision `dd65479` passed native CI, five-platform packaging, public publication and ten matching asset downloads. [ADR-0233](docs/adr/0233-coding-cancellation-is-final-and-process-ownership-outlives-the-root.md) preserves cancellation across agent errors/sign-in and retains descendant ownership after root exit, with cleanup failures reported explicitly. Both final local workspace configurations passed 3,170 tests each; all ten native jobs and the Windows/macOS/Linux lifecycle gates passed. Real Windows Codex cancellation is separately recorded. Original failures, falsification controls and archive hashes remain in TODO's evidence.

The [disposable website acceptance](TODO.md#completed-acceptance-slice-complete-coding-website-delivery-2026-10-17) is now verified on Windows with Codex ACP 2.2.2: independent checks, a managed preview, desktop/mobile-emulated browser interactions, explicit cleanup and unrelated-file preservation. The initial ten-minute test deadline left an honest timeout; a freshly approved continuation completed from retained files. Fresh sign-in, notification reception, physical devices and other providers/platforms retain separate gates. Durable native parent/child identity, restart recovery and parent Stop are implemented in [ADR-0234](docs/adr/0234-native-subagents-have-durable-lineage-and-parent-stop.md), with migration, cancellation falsification and real CLI/daemon restart checks on Windows. Native/coding delegation policy is aligned in [ADR-0235](docs/adr/0235-delegation-declares-work-and-its-execution-authority.md): shared delegation effects, preserved host-execution risk and stored-child launch refusal. All 3,187 Windows workspace tests, strict Clippy and falsification checks passed. Shutdown now reserves admission before checking idleness ([ADR-0236](docs/adr/0236-shutdown-reserves-work-admission-before-checking-idleness.md)). The final Windows workspace suite passes 3,202 tests with rebuilt process gates, strict Clippy and guard falsification; hosted CI and live-provider acceptance remain separate. [ADR-0237](docs/adr/0237-delegation-dispatch-and-family-stop-are-durable.md) adds durable cross-runtime family Stop and at-most-one worker dispatch per stored launch call, with conservative restart recovery. The selected slice passes 3,215 Windows workspace tests, rebuilt process gates, strict Clippy and guard falsification. [ADR-0238](docs/adr/0238-shutdown-drains-owned-work-before-closing-dependencies.md) adds bounded shutdown draining, child audit/reopen checks, normalized native failure isolation and a real scripted ACP crash gate. Graceful shutdown persists cancellation before restart; abrupt death remains a separate interruption/no-replay case. The selected slice passes 3,228 Windows workspace tests, all rebuilt process gates, strict lint and guard falsification. Native-platform and live-provider evidence retain separate gates.

The owner-selected [runtime and guide follow-up](TODO.md#current-acceptance-slice-runtime-and-guide-follow-up-2026-10-16) is implemented and validated locally and in native CI: atomic webhook capacity admission, stop/restart checks covering native runs, coding jobs and live managed processes, refusal on unreadable state, and all confirmed documentation corrections. Concurrency and unavailable-storage falsification, narrow regressions, both workspace test configurations and all three process gates passed on Windows; `c48da888` then passed the full native CI and release matrix. Provider-specific acceptance remains separate.

The earlier atomic memory/sub-agent admission and bounded/redacted logging repairs remain complete. Their [original evidence](TODO.md#current-acceptance-slice-review-corrections-2026-10-16) is preserved, with a correction noting that structural documentation checks did not establish complete semantic freshness. Native CI/release and real-provider acceptance remain separate from local validation.

The next work after those corrections is evidence-led: close the chosen MCP/provider/browser checks, coding delivery and native installation gates before expanding the provider list. Credential configuration and offline tests are not live account acceptance. The [capability wishlist](docs/product/capability-wishlist.md) contains residual opportunities, not a second execution queue. The owner-requested [Phase 12 competitive catch-up](#phase-12-competitive-catch-up) orders the gaps found against Hermes Agent and OpenClaw.

Coding defaults are controlled by [ADR-0222](docs/adr/0222-coding-delivery-uses-owner-workspaces-and-durable-evidence.md) and [ADR-0224](docs/adr/0224-coding-waits-and-verification-preserve-real-task-outcomes.md): use the first permitted folder in place, require Git only for explicit worktrees, do not automatically commit, and impose no total deadline unless requested. Host processes and worktrees are not an OS sandbox. See [the guide](docs/user/code-agents.md).

## Phase gates and dependency order

The phases below describe platform goals, including deliberately unshipped scope. They are not a completed-feature list or a command to redo delivered slices. MCP/ACP come before bespoke connectors/runtimes; ordinary runs and schedules come before a multi-step workflow engine (ADR-0167). Tauri, vendor-shaped voice transports and server infrastructure remain demand-gated.

## Phase 0: Architecture Baseline

**Outcome:** future contributors share one product definition, trust model, repository map, research method, and testable backlog.

**Exit gate:** all canonical documents linked from `ARCHITECTURE.md` exist, agree on ownership, and identify the first implementation slice.

## Phase 1: Installable Local Foundation

**Outcome:** a native `jarvisd` and `jarvis` can be built and run with no cloud account.

Deliver:

- Rust workspace and pinned toolchain
- daemon lifecycle, local IPC, health, status, and graceful shutdown
- platform-aware config/data/cache/log paths with restrictive permissions
- versioned configuration and SQLite migrations
- structured logging, correlation IDs, and secret redaction
- `jarvis status`, `jarvis doctor`, and portable foreground mode
- CI on Windows, macOS, and Linux

**Exit gate:** on all three operating systems, a clean build starts the daemon, the CLI reaches health over local transport, state survives restart, and `doctor` diagnoses a deliberately broken configuration.

## Phase 2: First Safe Conversation

**Outcome:** text input produces a streamed response through a provider-neutral model gateway, with durable sessions and runs.

Deliver:

- model/provider contracts and one OpenAI-compatible adapter
- explicit native agent state machine
- session, message, run, step, and model-call persistence
- context budgets and activity events
- cancellation, timeout, retry classification, and usage accounting
- deterministic scripted-model tests

**Exit gate:** a run streams to CLI, survives daemon restart at a persisted boundary, can be cancelled, and is fully testable without network credentials.

## Phase 3: Safe Capabilities And MCP

**Outcome:** JARVIS can use local and remote capabilities without granting the model authority.

Deliver:

- canonical tool schemas, effect classes, risk levels, and grants
- schema validation, policy evaluation, approvals, idempotency, and audit
- MCP client/host for stdio and Streamable HTTP
- scoped MCP server for external clients
- first safe filesystem read tool and sandboxed execution boundary

**Exit gate:** an external MCP client can call only its granted read tool; a write tool pauses for a human decision; restart and duplicate delivery do not execute the effect twice.

## Phase 4: Canonical Memory And Context

**Outcome:** JARVIS remembers deliberately, transparently, and within workspace boundaries.

Deliver:

- typed memory lifecycle with provenance, confidence, correction, expiry, and deletion
- entities and cautious identity resolution
- hybrid full-text, semantic, recency, importance, and relationship retrieval
- embedding provider abstraction and pgvector implementation for server mode
- context selection explanations and token budgets
- memory search, inspect, correct, forget, export, and retention APIs
- procedural memory: a skill format in which a procedure names already-granted tools and never grants
  authority ([ADR-0117](docs/adr/0117-a-skill-is-a-procedure-not-a-permission.md)), its promotion approval,
  and agent-proposed memory admission

**Exit gate:** a remembered preference survives restart, is retrieved with provenance, can be corrected and forgotten, and never crosses workspace boundaries.

## Phase 5: First-Party Connectors

**Outcome:** users can connect real services through auditable, least-privilege accounts.

Implement in this order:

1. Google OAuth, Gmail, Google Calendar, drafts and sends behind approval (built directly in the daemon)
2. Every other service through MCP servers (Microsoft 365, GitHub, and the rest), managed from Settings (`P11-001`); a native connector only where a named requirement cannot be met that way ([ADR-0167](docs/adr/0167-capability-and-reach-come-from-open-protocols-before-bespoke-code.md))
3. Generic webhook ingress as a trigger (`P6-012`)

**Exit gate:** setup tests connectivity before saving, refresh is resilient, scopes are visible, revocation works, diagnostics are redacted, and contract/live tests prove the supported operations.

## Phase 6: Events And Durable Automation

**Outcome:** JARVIS reacts proactively and resumes long-running work after failure.

Deliver:

- scheduler with timezone and daylight-saving behavior (`P6-003`)
- triggers that start ordinary runs: a time of day, a mail matching a query, a file, a webhook, each deduplicated (`P6-012`)
- notification routing with quiet hours and a budget (`P6-007`)
- delegation recorded as a parent and child run (`P6-010`, `P7-009`)
- approval and timer restart recovery (exists for approvals and schedules)

The durable workflow engine (event outbox and inbox, step records, compensation) is parked until a process needs to wait days across steps and a run plus a schedule cannot express it.

**Target exit gate, not yet satisfied:** a trigger durably reaches its run across daemon restart without duplicate effects, and replay starts nothing. Current schedules/triggers claim before starting work: this provides at-most-once admission with a crash-loss window, not exactly-once delivery. Atomic webhook capacity admission closes the concurrent-limit defect; a durable handoff/outbox remains separate demand-gated work.

## Phase 7: Runtime Ecosystem

**Outcome:** external agent engines are replaceable execution strategies.

Deliver:

- an ACP client (the open Agent Client Protocol over stdio) with a supervisor: one adapter reaches Claude, Codex, Gemini, Copilot, Cursor, OpenClaw and others (`P7-001`, `P7-002`)
- a scripted fake ACP agent for tests (`P7-006`)
- task approval and cooperative ACP permissions; in-place work in an owner-permitted directory by default, opt-in Git worktrees, no automatic commits or merges (`P7-003`, `P7-004`); a scoped JARVIS tool bridge and enforceable sandbox remain follow-up work
- a parent run recorded, a delegation effect class, cancellation that propagates (`P7-009` to `P7-011`)

No JARVIS-specific runtime protocol and no per-framework adapter (OpenAI Agents, LangGraph): they are reachable through ACP or MCP when wanted.

**Exit gate:** a real coding task completes in the selected permitted directory, respects owner approval/cancellation, preserves unrelated files and returns independent check evidence plus reviewable changes. Verify desktop/mobile behavior for a website. Record native-tool limits explicitly; a successful agent summary or worktree is not sandbox or delivery proof.

## Phase 8: Voice And Telephony

**Outcome:** users can speak to JARVIS, call it, and receive approved calls from it.

Deliver:

- local speech recognition and synthesis that need no key, voice activity detection and echo handling, and a client that is not a browser tab (`P8-010`); voice notes through Telegram are the cheapest first one
- a backup voice when ElevenLabs is unavailable: a provider-neutral speech interface with a fallback chain, an OpenAI-compatible adapter for hosted and local servers, and a second native cloud provider (`P8-018` to `P8-021`). Owner-requested 2026-10-17; plan in [voice backup and local voices](docs/research/voice-backup-and-local-voices-2026-10-17.md)
- native local voice generation in the daemon, with an editable voice library, voice design, consented cloning of the owner's voice and a pronunciation dictionary, in the style of ElevenLabs (`P8-022` to `P8-026`). P8-022 now has an optional in-process Kokoro engine, pinned assets and real Windows fallback/WAV playback evidence; disconnected-network and remaining native-platform acceptance stay open ([record](docs/research/integrations/embedded-speech.md)). P8-023 has a saved console voice picker, exact draft preview, supported controls and live routing/restart persistence evidence; Telegram/call assignments remain open ([record](docs/research/integrations/voice-settings.md)). P8-024 now supports a written description, local sample generation, saving/selecting the generated identity, restart reuse, export and deletion through VoxCPM2; Kokoro mixing remains a separate optional control ([record](docs/research/integrations/voice-description-design.md)).
- local voice GPU execution now uses optional pinned AMD/NVIDIA Windows runtimes with CPU retry and visible device metadata. RX 9070 XT generation, saved-reference playback and forced CPU fallback are live-verified; NVIDIA hardware and non-Windows GPU acceptance remain pending ([record](docs/research/integrations/voice-gpu.md)).
- the owner-selected latency continuation adds explicit warm-up progress, incremental playback and saved Fast/Balanced/Quality choices; Windows RX 9070 XT sample/preview/Console playback, cancellation, restart and CPU fallback pass. See [measurements](docs/research/integrations/voice-gpu-performance.md#implemented-warm-up-and-streaming-acceptance-2026-10-18) and [ADR-0251](docs/adr/0251-local-voice-streams-retain-admission-until-completion.md).
- provider-neutral outbound calls (Twilio, SIP) to numbers the owner approves, signed webhooks, call records, consent flags and retention (`P8-007` to `P8-009`, `P8-011`). **First slice built (`ADR-0200`):** JARVIS rings the owner through an ElevenLabs agent, to listed numbers only, with records and a yes on the phone that can answer the approval; run live by the owner on a real account (2026-10-13).

Parked until the owner uses them: ElevenLabs Custom LLM and Speech Engine transports, model-based turn detection ([ADR-0010](docs/adr/0010-model-based-turn-detection.md) stays the policy if built), and a realtime room.

**Exit gate:** the owner can talk to JARVIS hands-free without a browser tab, and a policy-approved task can place an outbound call with correlated audit and call records; with ElevenLabs blocked, JARVIS still answers in a natural backup voice, including a local one that works offline.

## Phase 9: Desktop And Product Installation

**Outcome:** JARVIS feels like an application, not a source checkout.

Deliver:

- the browser console as the main client; a Tauri shell only if an always-on wake word or a tray needs it (`P9-019`)
- messaging surfaces as authenticated gateway clients, one per platform, Telegram first (`P11-005`)
- onboarding and recovery UX
- signed installers, checksums, update channels, rollback, and uninstall
- Linux systemd user unit, macOS LaunchAgent, Windows per-user scheduled task; optional elevated system service modes
- clean-machine installer tests on native CI runners

**Exit gate:** a non-developer can install, configure, use, update, diagnose, and uninstall JARVIS on every supported OS without Node, Python, or Rust.

## Phase 10: Server, Multi-Device, And Scale

**Outcome:** the same core supports an always-on server and multiple trusted devices.

Deliver only when demanded by evidence:

- PostgreSQL plus pgvector production profile
- object storage, backups, restore drills, and disaster recovery
- device pairing, token rotation, revocation, and remote access hardening
- team roles and stronger tenant isolation
- optional Redis, distributed bus, Temporal, or Kubernetes behind existing ports

**Exit gate:** recovery objectives, load targets, isolation tests, and operational ownership are documented and proven.

## Explicitly Deferred

- Kafka, NATS, Redis, Qdrant, Elasticsearch, Kubernetes, and Temporal in the local v1
- unreviewed marketplace plugins
- unrestricted host shell execution
- silent autonomous financial, destructive, or mass-communication actions
- canonical memory owned by an external runtime or provider

## Phase 11: Capability And Reach

**Outcome:** the assistant can do more each month without a rewrite and without a longer setup.

MCP management, tool discovery, browser tools, owner-approved skills, Telegram, time-zone triggers, model routing and spend guards are implemented. Current credential, memory and coding follow-ups extend those paths. Close each slice with the kind of evidence it requires; fixture success does not substitute for owner-account acceptance.

Use the [active residual tasks](TODO.md#capability-and-reach-p11) and [wishlist](docs/product/capability-wishlist.md): account verification and hot registration, authenticated browsing, owner-file recall evaluation, keychain storage, remaining document/media output, notification budgets and named additional channels. Managed host processes and complete skill bundles are already built; confinement, private/global catalogs and safe updates remain distinct work.

## Phase 12: Competitive Catch-Up

**Outcome:** JARVIS matches Hermes Agent and OpenClaw on ease of install, learning, memory and reach, while keeping the authority boundary that both of them lack.

Owner-requested on 2026-10-17 after the [competitive gap review](docs/research/competitive-gap-2026-10-17.md). This phase orders existing demand-gated work (Phases 4, 6, 7, 9 and 11) and adds a few new items; it does not reopen delivered slices. The [P12 backlog](TODO.md#competitive-catch-up-p12) holds acceptance for each item.

Deliver, in order:

1. Signed, toolchain-free installation and updates (`P12-001`), and an owner decision on whether guided setup still requires a voice key (`P12-002`); the backup and local voice (`P8-018` to `P8-022`) make a no-key setup possible
2. Skills chosen by meaning, skills that improve after use, an always-loaded owner profile and semantic memory (`P12-003` to `P12-006`)
3. Discord, Slack and WhatsApp channels, an enforceable execution sandbox and a native Anthropic adapter (`P12-007` to `P12-009`)
4. Image generation, PowerPoint and charts; restart-safe proactive delivery; skill ecosystem import with scanning; completed live acceptance (`P12-010` to `P12-013`)

**Exit gate:** a non-developer installs JARVIS on a clean machine without a toolchain, reaches it from at least three chat platforms, gets an approved skill selected for a paraphrased request, and runs a coding job inside an enforced sandbox, while every effect still passes deterministic policy and approval.
