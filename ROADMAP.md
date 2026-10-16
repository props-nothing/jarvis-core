# JARVIS Roadmap

**A capable assistant first. Infrastructure when the evidence demands it.**

[Home](README.md) / [Documentation](docs/README.md) / [Acceptance backlog](TODO.md) / [Architecture](ARCHITECTURE.md)

This roadmap is ordered by risk. Each phase must leave a runnable vertical slice. A later phase may start only when the previous phase's exit gate passes or an ADR explains the exception.

## Owner-workspace coding delivery

The owner reprioritized coding delivery on 2026-10-15 ([ADR-0222](docs/adr/0222-coding-delivery-uses-owner-workspaces-and-durable-evidence.md)). The first Settings-permitted folder is the in-place default; Git/worktrees are explicit and no total job deadline is imposed unless requested. Complete skill resources, independently reported checks and managed previews replace self-reported completion as the delivery path. Local validation and remaining real-agent/browser/platform gates are tracked in [the acceptance ledger](TODO.md#owner-requested-coding-delivery-2026-10-15); this does not close the scoped integration bridge or sandbox backlog.

## At A Glance

Documentation snapshot: **2026-10-15**. This summarizes recorded delivery evidence, not a fresh run of every provider or platform gate. The platform and separate website now use Apache-2.0 for original code and docs; third-party and prototype terms remain separate.

| Track | Current position | Next proof |
| --- | --- | --- |
| Personal assistant | Browser/CLI, memory, files, documents, Google, projects and oversight are delivered slices. | Close the remaining owner-account checks recorded in the backlog. |
| Reach and initiative | Telegram, approved skills, schedules, triggers and model routing are built. | Measure quality and cost; finish setup polish and remote/MCP credential gaps. |
| Coding delegation | Settings-first in-place jobs, opt-in worktrees, no automatic commits or overall deadline, full skill bundles and managed previews are implemented (ADR-0222). Earlier ordinary/background Codex runs have recorded verification. | Verify the new path against the owner's real agent: dependency setup, cancellation, desktop/mobile website behavior and scoped integration access. |
| Voice and calls | Browser voice and optional synthesis; owner outbound calls reported working live. | Browser-independent local voice and broader call-path evidence. |
| Distribution | Apache-2.0 selected; user-level install/service paths and CI archives exist. | Dependency notices, public signed distribution and clean-machine evidence. |
| Scale | Local SQLite is the default. | Server/multi-device work only with a named requirement and proven ownership semantics. |

The phase descriptions below are gates and dependency order, **not a list of completed features**. [TODO.md](TODO.md) records the individual slices and unresolved checks.

## Current Acceptance Slice (selected 2026-10-15)

**Generic pre-registered MCP OAuth** was selected next: Settings → Advanced → Programs supports one-time provider app setup, expected-issuer/resource discovery, S256 browser sign-in, persistent grants/refresh, Test and fail-closed Disconnect ([ADR-0220](docs/adr/0220-generic-mcp-oauth-binds-preregistered-clients-to-resource-and-issuer.md)). It is verified with independent HTTP fixtures, a real fixture MCP host and owner-authenticated setup routes, not a real provider/browser account. The owner chose pre-registration; automatic DCR/CIMD and no-restart tool registration remain separate. See [setup](docs/user/mcp.md#a-remote-server-with-browser-sign-in) and [acceptance evidence](TODO.md#generic-pre-registered-mcp-oauth-2026-10-15).

**Remote MCP token credentials** were selected next by the owner: reusable same-server named bearer references now extend the HTTP credential boundary, with per-request rotation/removal, reference-only CLI/Settings/Test and a mutation-proven cross-server guard ([ADR-0219](docs/adr/0219-remote-mcp-tokens-are-named-server-bound-request-credentials.md)). Tested through the real HTTP backend and private profile store against loopback peers; no real remote account acceptance is claimed. Generic OAuth and hot registration remain separate. See [setup](docs/user/mcp.md#a-remote-server-that-needs-a-token) and [acceptance evidence](TODO.md#named-remote-mcp-token-credentials-2026-10-15).

**OAuth-first Settings** was selected after the local credential slice: Settings → Connections now provides persistent GitHub app sign-in, verified account storage/refresh and a fixed hosted read-only MCP binding ([ADR-0217](docs/adr/0217-github-settings-use-persistent-pkce-sign-in-and-current-token-mcp-requests.md)). One-time registration of the owner's GitHub OAuth app remains necessary; live browser consent/provider acceptance and native platform checks stay open. This provider-specific grant is separate from the newer generic pre-registered flow above; automatic client registration and hot tool registration are not claimed. See [Connections](docs/user/connections.md) and [the acceptance evidence](TODO.md#oauth-first-settings-connections-2026-10-15).

The owner selected **task-aware memory** (`P11-026`, [ADR-0215](docs/adr/0215-native-recall-searches-the-task-before-falling-back-to-recent-memory.md)) after the capability review. The native executor now searches canonical memory for the current task before falling back to recent context, supplies fenced provenance, and records its selection. A deterministic 30-question fixture benchmark retrieves the expected old fact first in 30/30 cases, versus 0/30 in the former newest-24 candidate set. This is built and tested offline; live-model answer quality and corpus latency/token comparisons remain open. It does not complete semantic, project/file or relationship retrieval under `P11-014`. See [memory behavior and limits](docs/user/memory.md) and [the acceptance ledger](TODO.md#owner-selected-task-aware-memory-2026-10-15).

## Current Direction (reviewed 2026-10-10)

The phases below remain the dependency order of the *platform*; the order of *product slices* was changed on purpose,
because a platform that cannot yet be talked to, watched or trusted with a task is not the product. Delivered ahead of
their phases, each with an ADR and a live run against a real model: web fetch (`ADR-0129`), approval from the CLI that
survives a restart (`ADR-0130`), code in a disposable container (`ADR-0131`), scheduled tasks (`ADR-0132`), first-run
`init`/`start`, a kill switch, standing trust for low-risk tools (`ADR-0133`), sub-agents (`ADR-0134`), the browser
console with its holographic head and browser-native voice (`ADR-0135`), light approvals (`ADR-0136`), file write, edit
and move, a natural voice, onboarding and settings (`ADR-0137` to `ADR-0139`), a web search (`ADR-0146`), **Google sign-in from Settings
with Gmail and Calendar tools, including send, attachments, replies and drafts** (`ADR-0149`, `ADR-0150`, `ADR-0157`,
`ADR-0161`), **projects** with a journal, daily run and token caps, a digest and push (`ADR-0151`, `ADR-0155`, `ADR-0165`),
a live mission view (`ADR-0152`), a contacts list, **documents and OCR** (`ADR-0158`, `ADR-0162`), a **fallback model at
a second provider** and a provider and model picker (`ADR-0159`, `ADR-0163`, `ADR-0164`), and token usage recorded for
every model call. The Python prototype was removed on 2026-10-13; its migration and licence history remains in
`docs/migration/python-prototype.md` (`P9-009`).

Measured against the four hallmarks of a real J.A.R.V.I.S.:

| Hallmark | State | Next slice |
| --- | --- | --- |
| Oversight | Strong. Approvals, `jarvis cancel`, one-level sub-agents, `jarvis watch`, the console, an audit trail, per-project caps, a count of tokens per run. | Coding agents as supervised runtimes (`P7`, via ACP); stop a run midway at a spend limit. |
| Visual presence | The console at `/`: a face that speaks, listens and reacts, a live mission view of what it reads and calls, panels with Stop, Approve and Deny. | A push stream instead of polling; the Tauri desktop client (`P9-019`). |
| Voice | Browser-native push-to-talk, wake word, spoken answers, interruption; a neural voice with an ElevenLabs key. | Local (non-cloud) recognition and synthesis, then telephony (`P8`). |
| Hands-free | A spoken "Jarvis, stop" and a spoken "yes"/"no" for approvals (`ADR-0136`). | Always-on local wake word so it works without the browser tab in front. |

**Order of work from here** (the same list, with detail, is at the top of `TODO.md`; the reasoning is `ADR-0167`: open protocols before bespoke code):

0. **Check what is only tested against stand-ins** with the owner's own accounts (OCR, the second provider, the push, a reply from another sender).
1. ~~**MCP made real and manageable** (`P11-001`)~~ **built and run live on two real servers** (`ADR-0168`): the door to every tool that already exists as an MCP server. Local server-bound credential references are built/tested through real child processes (`P11-001b(a)`, `ADR-0216`), and named remote bearer references are built/tested against real loopback HTTP (`ADR-0219`). Generic pre-registered remote OAuth is built/tested (`ADR-0220`); GitHub account read/approval/write evidence, real provider/browser acceptance, automatic registration and hot apply remain open. See [credential setup](docs/user/mcp.md#a-local-server-that-needs-a-key).
2. ~~**Tool discovery** (`P11-002`)~~ **delivered** (`ADR-0176`): tools are selected for the request without making any unreachable. Meaning-based ranking and broader model comparisons remain open.
2b. ~~**Connect apps**~~ **built** (`ADR-0170`): one switch and one key let Claude Code, Cursor, VS Code or a script use JARVIS, including starting a run with `jarvis.agent.delegate`; approvals stay with the owner.
3. ~~**A browser**~~ **built as a one-click ability** (`P11-001c`, `ADR-0169`): JavaScript pages and acting on the web; a measured hostile-page eval is still owed (`P11-003`).
4. ~~**Skills the model proposes and the owner approves**~~ delivered (`P11-004`, `ADR-0183`): it learns the owner's procedures.
5. **Telegram** (`P11-005`): built (`ADR-0184`), configured in Settings, reachable from a phone with no tunnel; run on the owner's phone; answers are formatted (`ADR-0186`).
6. ~~**Time-zone schedules**~~ delivered (`P6-003`, `ADR-0182`): "every weekday at 08:00 Europe/Amsterdam". **Triggers** (`P6-012`) are delivered (`ADR-0193`, `ADR-0195`): a new mail, file or request starts an ordinary run, and quiet hours hold notices (`P6-007` in part).
7. ~~**Model routing** (`P11-006`)~~ **built and run live** (`ADR-0196`): sub-agents and scheduled or triggered runs can use their own model, and the digest shows tokens per model. Whether a cheaper model is good enough is still measured with `jarvis eval`, not assumed.
8. **Coding agents over ACP** (`P7`): delegate to ChatGPT/Codex, Claude, Copilot, Gemini and others in a worktree, reviewed before anything merges. **First slice built (`ADR-0207` to `ADR-0209`)**, chosen and tested from the console with a sign-in flow available. The handshake and ordinary/background tasks have recorded live Codex evidence; the console Sign in click remains unverified.
9. Setup polish (`P11-007`); ~~a spend guard in the run~~ is delivered (`P11-008`, `ADR-0191`). Apache-2.0 is selected for original platform and website code/docs. Then local voice and calls (`P8`), distribution (`P9-004` to `P9-007`; dependency notices, signing and clean-machine gates remain), and the server profile (`P10`).

Parked until there is a named use: the durable workflow engine, a connector per provider, a runtime adapter per agent framework, ElevenLabs-shaped voice transports, Tauri.

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
- the agent's permission requests become JARVIS approvals; its work happens in a git worktree and nothing is merged without approval (`P7-003`, `P7-004`)
- a parent run recorded, a delegation effect class, cancellation that propagates (`P7-009` to `P7-011`)

No JARVIS-specific runtime protocol and no per-framework adapter (OpenAI Agents, LangGraph): they are reachable through ACP or MCP when wanted.

**Exit gate:** a coding task is delegated to an installed agent, runs in a worktree, asks JARVIS for permission, and returns a diff while JARVIS retains policy, tool, memory, and audit ownership.

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

Deliver, in order ([ADR-0167](docs/adr/0167-capability-and-reach-come-from-open-protocols-before-bespoke-code.md), `TODO.md` `P11`): MCP made real and manageable, tool discovery so a large catalog costs no tokens until needed, a browser (Playwright MCP), skills the model proposes and the owner approves, Telegram, model routing, the first five minutes of setup, a spend guard inside the run. Each is only done after a live run on the owner's own setup.

Then the **capability wishlist** ([docs/product/capability-wishlist.md](docs/product/capability-wishlist.md), `TODO.md` `P11-010` to `P11-022`), in this order: JARVIS writes documents (xlsx, docx, pdf, pptx, charts); images shown inline and generated; authenticated browsing; a runtime without Docker and long-running processes; retrieval over the owner's own files; a vault for secrets; calendar free/busy and contact merge; inbound webhooks; monitoring with alerts; Slack, WhatsApp and SMS; maps and invoicing through MCP; skill discovery that works; a self-maintained wishlist and an eval trend export.
