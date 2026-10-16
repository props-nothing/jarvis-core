# JARVIS Roadmap

**A working local assistant; remaining work is reliability, verified integrations and distribution.**

[Home](README.md) / [Active acceptance backlog](TODO.md) / [Historical evidence](docs/history/README.md) / [Architecture](ARCHITECTURE.md)

## At a glance

Reviewed 2026-10-16. The [active backlog](TODO.md) controls current acceptance order. Earlier test transcripts and original phase wording remain in dated history; they do not override later accepted ADRs or represent current test failures.

| Area | Built now | Remaining acceptance or capability |
| --- | --- | --- |
| Core and control | Local Rust daemon/CLI, SQLite, durable approvals, audit, cancellation, a browser console and bounded/redacted logs | Native lifecycle and release evidence |
| Everyday work | Google mail/calendar, files/documents/XLSX edits, contacts, browser/search, projects and Telegram | Selected owner-account failure paths, OCR, provider fallback, image generation, PowerPoint/charts |
| Automation | Time-zone schedules, mail/file/webhook triggers, quiet hours, atomic sub-agent admission, routing and spend guards | Structured parent/child lifecycle, cancellation propagation and notification budgets |
| Memory | Owner-controlled admission, inspect/atomic correction/forget, task-aware lexical recall and granted-file full-text search | Owner-corpus answer/latency evaluation, automatic file/entity context and measured semantic retrieval |
| MCP accounts | Named stdio/HTTP secrets, persistent GitHub OAuth and generic pre-registered OAuth | Real provider/browser acceptance, automatic client registration, per-client credentials and hot registration |
| Coding | Settings-first in-place queued jobs, opt-in worktrees, full skill bundles, independent checks and managed previews | Complete real-agent website/cancellation/sign-in acceptance, scoped integration bridge and optional enforceable sandbox |
| Voice | Browser voice/optional runtime synthesis; owner-reported working outbound calls | New setup requires ElevenLabs per owner choice; browser-independent local speech and broader telephony remain |
| Distribution | Apache-2.0, user installation/services, CI archives and source-building website scripts | Complete notices/SBOM, signed public packages, update/rollback and clean-machine verification |
| Scale | Local SQLite profile | Server, PostgreSQL parity, multiple devices and roles only for a named requirement |

## Current acceptance slice

The 2026-10-16 review corrections are implemented and validated locally on Windows: atomic memory correction and sub-agent admission, bounded/redacted logging with singleton-safe startup, current documentation/website claims, preserved historical evidence and a documentation CI guard. The [completed slice](TODO.md#current-acceptance-slice-review-corrections-2026-10-16) records exact checks and limitations. Existing macOS credential-fixture and CI/release follow-up remains in the active ledger; fresh hosted native acceptance is still required.

The next work after those corrections is evidence-led: close the chosen MCP/provider/browser checks, coding delivery and native installation gates before expanding the provider list. Credential configuration and offline tests are not live account acceptance. The [capability wishlist](docs/product/capability-wishlist.md) contains residual opportunities, not a second execution queue.

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

**Exit gate:** a trigger starts a run exactly once, survives a daemon restart, and a replayed event starts nothing.

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
- provider-neutral outbound calls (Twilio, SIP) to numbers the owner approves, signed webhooks, call records, consent flags and retention (`P8-007` to `P8-009`, `P8-011`). **First slice built (`ADR-0200`):** JARVIS rings the owner through an ElevenLabs agent, to listed numbers only, with records and a yes on the phone that can answer the approval; run live by the owner on a real account (2026-10-13).

Parked until the owner uses them: ElevenLabs Custom LLM and Speech Engine transports, model-based turn detection ([ADR-0010](docs/adr/0010-model-based-turn-detection.md) stays the policy if built), and a realtime room.

**Exit gate:** the owner can talk to JARVIS hands-free without a browser tab, and a policy-approved task can place an outbound call with correlated audit and call records.

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
