# JARVIS Implementation Backlog

**Current acceptance work and remaining gates. Historical test narratives live in [docs/history](docs/history/README.md).**

[Home](README.md) / [Roadmap](ROADMAP.md) / [Documentation](docs/README.md) / [Definition of done](docs/development/definition-of-done.md)

Work from the first unchecked acceptance slice unless the owner selects another. A checked box requires its stated test and documentation evidence. Built, fixture-tested, live-verified and accepted are different states; do not turn an old partial implementation into a completion claim by moving its notes.

## Current acceptance slice: review corrections (2026-10-16)

The owner selected the repository review follow-up: fix confirmed runtime defects, reconcile all owned Markdown and the public website, and separate current decisions from historical evidence. No obsolete connector code is deleted by this slice.

- [x] [ADR-0227](docs/adr/0227-transactional-memory-corrections-commit-one-versioned-change.md): correct memory updates atomically: expected-version checks, supersession and the new revision commit together; prove stale/concurrent failure leaves the original memory intact.
- [x] [ADR-0228](docs/adr/0228-native-subagent-admission-counts-and-starts-in-one-transaction.md): make native sub-agent admission atomic across the full relevant run history, enforce the four-working/sixteen-unfinished admission limits and show active children even after unrelated newer runs. Approval resume and parent-linked cancellation are separate semantics.
- [x] [ADR-0226](docs/adr/0226-logs-share-redaction-and-bounded-retention.md): bound and rotate daemon log files without weakening redaction; exercise rollover, recovery and failure paths with focused tests. Singleton ownership now precedes log initialization; rejected duplicate startup preserves the owner's partial log.
- [x] Reconcile guides, current architecture, integration status, ADR identifiers, website copy and the residual backlog. Preserve original acceptance/research evidence in dated archives; check local links, headings and ADR uniqueness in CI.
- [x] Run the narrow regressions, documentation checks, required workspace formatting/Clippy/tests and relevant frontend checks. Native CI and real provider/account acceptance remain separate gates.

Windows evidence (2026-10-16, `b930f73` plus this working tree): default workspace **3,128 passed**, followed by the added duplicate-start regression **1 passed**; final all-feature workspace **3,129 passed**, zero failures, five existing opt-in tests ignored in each full run. Final formatting, strict all-target/all-feature Clippy and fresh workspace build passed. All three process acceptance gates passed with both missing-binary guards enabled. Dependency policy, archive/license fixtures, workflow lint, **185 core Node tests**, HUD syntax/typecheck/build and **38 website tests** plus website typecheck/build/installer checks passed. All **372 owned Markdown files** across the two repositories pass the local-link/anchor/ADR checker. The website retains its existing optional face-chunk size advisory. Docker was unavailable, so runtime-dependent Docker cases skipped; no new live-provider, PostgreSQL, native macOS/Linux, deployment or release-package acceptance is claimed. Detailed local logs and preservation proof: `artifacts/reviews/fix-*-2026-10-16.log`, `final-cargo-results-2026-10-16.json` and `docs-reconciliation-validation-2026-10-16.json` in that directory. The macOS fixture/CI repair in `b930f73` and its acceptance notes below are preserved.

## Owner-requested CI and release reliability (2026-10-16)

Owner-selected next slice: repair the reviewed failures and timeouts while retaining the release checks. Research: [Rust CI](docs/research/integrations/github-actions-rust-ci.md).

- [x] Replace the Unix Clippy-triggering permission expression with its equivalent bit check and add a public-mode refusal regression. Replace both yanked ZIP versions with 9.0.4, using manifest-only patches to the official Office-writer sources; strict Windows Clippy, all 31 document tests and all four dependency-policy checks pass. Native Unix execution remains part of the hosted follow-up below.
- [x] Add pinned Rust dependency caches, 60-minute Windows Rust/package jobs, separate lint/tests, three Windows test partitions and retained diagnostics. Workflow lint and all 32 release-flow regressions pass, including exact package coverage and unchanged release eligibility.
- [x] Reuse closed migration-image bytes for independent ordinary database fixtures; retain fresh migration/startup/permission tests and production durability settings. The same 21 approval tests with four threads fell from 5.45s to 1.71s locally; isolation, persistence, pragma and overwrite-refusal regressions pass.
- [x] Windows verification: default-feature and all-feature locked workspace runs each passed **3,110 tests**, zero failures, five existing ignored. Formatting, strict workspace Clippy, all four dependency-policy checks, **179 JavaScript tests**, deterministic typed-HUD rebuild, archive/license regressions and actionlint passed. Freshly built binaries passed all three acceptance gates with both skip guards enabled. Evidence: `artifacts/ci-review/`; [testing notes](docs/development/testing.md) and [research/limits](docs/research/integrations/github-actions-rust-ci.md).
- [ ] After pushing, verify native Linux/macOS/Windows CI, cold/warm cache behavior and all five release packages on GitHub. Check any required-status rules against the new job names. Local evidence does not prove hosted performance, Unix-only execution or draft uploads.

Hosted follow-up: `8fcd458` / CI `38920832954` passed all native Clippy jobs, dependency policy, Linux tests/gates and Windows storage (354 unit tests in 89.98s). macOS found 18 credential/OAuth fixture failures due to symlinked temporary ancestors. The repair canonicalizes only fixture roots and extends the unchanged store's symlink/junction refusal tests to the portable root. Final native/package verification remains open. Repository metadata reports no required checks/rulesets needing migration.

The isolated daemon job now explicitly builds its sibling MCP peer executable. With the old executable moved aside, the missing-peer guard failed as expected; the exact new build command then made the same test pass. The repair passed 138 MCP tests, four GitHub settings tests, four storage credential tests, all 3,110 default workspace tests (five existing ignored), formatting, strict Clippy, all 32 release-flow regressions and workflow lint locally. No production credential guard or CI assertion was relaxed.

Second hosted follow-up: `b930f73` / CI `38923320820` passed nine jobs, including macOS and all native acceptance gates. The Windows daemon suite passed 834 tests but its real preview fixture exceeded a ten-second startup wait. Give only that Windows fixture 30 seconds and report the last process state/output on failure; preserve all HTTP, occupied-port, browser-honesty and process-stop assertions. A temporary 11-second child-start control failed before the change and passed afterward; a child that never listens still failed at 30 seconds with its stderr in the diagnostic. The ordinary five process tests pass in 1.19s. Final hosted/package verification remains open.

Preview repair validation: formatting, strict all-target/all-feature workspace Clippy and all 3,110 locked default workspace tests passed, with zero failures and five existing ignored tests. The MCP peer was explicitly built and its missing-binary guard enabled. Logs and temporary controls are retained under `artifacts/ci-review/preview-startup-2026-10-16/` in the primary checkout. No workflow, release check or production behavior changed in this repair.

## What is built and what still needs acceptance

The current local product has a Rust daemon/CLI, a browser console, Telegram, Google mail/calendar tools, managed MCP clients and an opt-in MCP server, policy approvals, SQLite memory, file knowledge search, schedules/triggers, project budgets, coding delegation and managed host processes. See [README](README.md#built-experimental-next) for the product summary. This section names remaining evidence rather than repeating completed implementation logs.

### Coding delivery and memory discovery

<a id="owner-requested-coding-delivery-2026-10-15"></a>

Coding uses the first Settings-permitted folder **in place** by default; Git/worktrees are opt-in, automatic commits are absent, and no total deadline is imposed unless requested. Durable queued jobs, explicit fresh continuation, complete skill bundles, independent checks and managed previews are built. Ordinary/background Codex runs and a real dependency-install/check smoke are recorded; real-agent cancellation, complete owner website delivery, a scoped agent-to-JARVIS integration bridge and native macOS/Linux lifecycle acceptance remain open. See [coding guide](docs/user/code-agents.md), [ADR-0222](docs/adr/0222-coding-delivery-uses-owner-workspaces-and-durable-evidence.md), [ADR-0224](docs/adr/0224-coding-waits-and-verification-preserve-real-task-outcomes.md) and [repair evidence](docs/history/todo-owner-evidence-through-2026-10-16.md#owner-requested-delegation-repair-2026-10-16).

<a id="owner-selected-memory-discovery-2026-10-15"></a>

Memory browse/read and the Operations → Knowledge inspector exist, with correction/forget controls. Task-aware lexical recall has a 30-question SQLite fixture benchmark; it is not live-model answer-quality or semantic-retrieval proof. See [memory guide](docs/user/memory.md), [ADR-0223](docs/adr/0223-memory-discovery-browses-before-guessing-and-reads-by-id.md) and [inspector repair evidence](docs/history/todo-owner-evidence-through-2026-10-16.md#owner-requested-memory-inspector-repair-2026-10-16).

### Provider and user-flow verification

- [~] `P11-001b(a)` Local MCP named credentials are built and tested with real child processes. Run the opt-in owner GitHub read, then a separately authorized disposable write with denied-write falsification; native macOS/Linux credential evidence remains open. [Recorded evidence](docs/history/todo-owner-evidence-through-2026-10-16.md#credential-backed-local-mcp-2026-10-15).
- [~] `P11-001b(b)-tokens` Remote named bearer references are built/tested. Run the discovery-only opt-in against the owner's selected real HTTPS server/token; rotation/removal and approved-write acceptance require the selected service. [Recorded evidence](docs/history/todo-owner-evidence-through-2026-10-16.md#named-remote-mcp-token-credentials-2026-10-15).
- [~] `P11-001b(b)-github` Persistent GitHub OAuth, current-token MCP requests and Settings are fixture-tested. Register the owner's app, complete real browser consent and verify listing/read, restart, refresh/revocation; native callback evidence remains open. [Recorded evidence](docs/history/todo-owner-evidence-through-2026-10-16.md#oauth-first-settings-connections-2026-10-15).
- [~] `P11-001b(b)-oauth` Generic pre-registered MCP OAuth is built/tested with independent fixtures. Verify the owner's actual provider app/browser, refresh/revocation and real read/approved write. Automatic DCR/CIMD and hot registration are separate features. [Recorded evidence](docs/history/todo-owner-evidence-through-2026-10-16.md#generic-pre-registered-mcp-oauth-2026-10-15).
- [~] `P11-025` Recoverable cleanup, browser literal-find, CSV contacts and XLSX edits are built. Remaining acceptance includes Gmail trash/restore on the owner's account, edited workbooks in real Excel and native platform checks. Contact deletion now removes suppression after normal authorization (ADR-0218). [Recorded evidence](docs/history/todo-owner-evidence-through-2026-10-16.md#owner-requested-tool-gaps-2026-10-14).
- [~] `P11-026` Task-aware memory recall: measure live answer/source quality, latency and tokens on owner-approved questions; project notes, entity aliases and file evidence are not yet automatic recall. [Recorded evidence](docs/history/todo-owner-evidence-through-2026-10-16.md#owner-selected-task-aware-memory-2026-10-15).
- [~] `P11-027` Long input and recoverable results are built; retain the 200,000-character objective limit and recoverable paged results. Complete owner long-input/result-recovery and native-platform acceptance. The console is now a compact single-line composer; the earlier multiline UI is historical. [Recorded evidence](docs/history/todo-owner-evidence-through-2026-10-16.md#owner-requested-text-recovery-2026-10-15).
- [ ] Run remaining recorded owner checks: real Tesseract OCR (`P9-082`), second-provider fallback (`P9-083`), ntfy push (`P9-065`), reply from another sender (`P9-074`), provider switching from Settings (`P9-085`), calendar overlap/delete (`P11-016`) and broader model comparisons (`P9-090`, `P9-091`). [Earlier evidence and limits](docs/history/todo-p9-p11-evidence-through-2026-10-16.md).

## Remaining capability slices

These are residual scopes, not an instruction to rebuild delivered features. Historical IDs remain stable and the archives retain their original acceptance wording.

### Tools, policy and memory (P3–P4)

- [~] `P3-008` MCP stdio/HTTP host composition is delivered. Remaining credential/provider acceptance and registration lifecycle are tracked by `P11-001b`; do not repeat the old “no production caller” implementation task. [Original evidence](docs/history/todo-p3-evidence-through-2026-10-16.md#p3-008).
- [~] `P3-009` Authenticated external MCP access is delivered; per-client credentials/allowlists, audience-bound remote authorization and associated rate-limit acceptance remain. [Original evidence](docs/history/todo-p3-evidence-through-2026-10-16.md#p3-009).
- [~] `P3-012` Approval restart and duplicate-delivery gates exist. Keep current acceptance tests as the gate; external-client authorization and policy-at-resume semantics must be reconciled against later ADRs before closing the remaining umbrella. [Original evidence](docs/history/todo-p3-evidence-through-2026-10-16.md#p3-012).
- [ ] `P3-021` Additional remote/disposable sandbox adapter only for a named requirement; prove declared resource guarantees, crash isolation, output bounds and cleanup. Existing Docker execution does not sandbox ACP host processes. [Original evidence](docs/history/todo-p3-evidence-through-2026-10-16.md#p3-021).
- [~] `P4-014` Conversational memory proposal/admission is delivered (including later owner Keep). Close residual acceptance across native platforms and current correction/forget behavior; do not reimplement the former missing model surface. [Original evidence](docs/history/todo-p4-evidence-through-2026-10-16.md#p4-014).

### Connectors and automation (P5–P6)

- [ ] `P5-005b` Retire the unused Google push/watch framework in a separate, dependency-verified deletion slice only after owner confirmation required by ADR-0167 §6. Keep contracts, OAuth/readiness and everything production calls. Polling already serves mail triggers. No deletion is part of this review correction.
- [~] `P5-009` Google write/draft tools are delivered. For each new MCP/provider write, prove its policy/idempotency/outcome boundary and real approved/denied behavior rather than adding a bespoke connector by default.
- [ ] `P5-010` Complete provider-seam tests and selected live checks for expiry, revoked scope, pagination, throttling, reauthorization and redacted diagnostics.
- [~] `P6-007` Quiet hours and notifications exist. Add per-day proactive-message budgets, deduplication, per-kind/project controls and an owner-facing disable action when required.
- [ ] `P6-010` Durable structured parent/child identity, restart semantics and linked cancellation remain with `P7-009`–`P7-011`. Atomic admission alone does not complete delegation durability.
- [~] `P6-012` Mail, file and webhook triggers exist; preserve their deduplication and ordinary-run policy. The remaining external webhook acceptance is `P11-017`.

See [P5–P8 historical evidence](docs/history/todo-p5-p8-evidence-through-2026-10-16.md). Time-zone schedules and model-selected approved skills are delivered; the multi-step workflow engine remains parked.

### Coding agents and delegation (P7)

- [~] `P7-001` ACP v1 client, handshake and bounded task activity are built, with recorded real Codex tasks. Complete the current activity/event and real-agent acceptance rather than implementing a private JARVIS runtime protocol.
- [~] `P7-002` Sanitized environment, operation/output bounds and stop/tree cleanup exist. Remaining supervisor work includes crash/orphan recovery, stderr redaction review, backoff and native-platform evidence. No default total deadline or OS confinement is claimed.
- [ ] `P7-003` Task approval and cooperative ACP permissions exist. A scoped JARVIS tool bridge, interactive policy re-entry where needed and an enforceable optional sandbox remain; a worktree is not a security boundary.
- [~] `P7-004` In-place durable coding, optional worktrees, complete skills, independent checks and managed previews are built (ADR-0222/0224). Verify real-agent cancellation/sign-in, full desktop/mobile website behavior, notification delivery and retained-work cleanup.
- [ ] `P7-008` Complete adversarial protocol/skew/crash/cancellation tests and audit coverage for the supported ACP capability surface; native tools are not all intercepted.
- [ ] `P7-009` Replace objective-text parent markers with structured persisted parent/child identity and lifecycle semantics, including restart and parent cancellation.
- [ ] `P7-010` Define delegation's effect/policy semantics consistently for native and coding jobs.
- [ ] `P7-011` Prove depth and concurrency limits, duplicate-delivery behavior, child isolation, audit and cancellation propagation. Admission repair in the current slice closes only its documented concurrency/history defect.

### Voice and channels (P8)

- [ ] `P8-010` Local English/Dutch recognition and synthesis, VAD/echo handling and a browser-independent client. Research measured candidates first; Telegram voice notes can be an initial surface. A tray client is conditional on need.
- [ ] `P8-001` Extend provider-neutral voice/session/identity/retention contracts when local or broader call paths require them.
- [~] `P8-007` Owner outbound calls through ElevenLabs have owner-reported live evidence. Broader provider-neutral calls, additional call paths and real-account failure cases remain; this is not general telephony acceptance.
- [ ] `P8-008` Add raw-body signature verification, deduplication and bounded asynchronous webhook handling when inbound calls require it. Current outbound status polling needs no provider webhook.
- [~] `P8-009` Outbound masked call records, 90-day cleanup and deletion exist; additional consent/disclosure, inbound records and correlated approval-audit evidence remain.
- [ ] `P8-011` Verify inbound/outbound, no-answer/voicemail, interruption, latency, replay, privacy and provider outages for each supported call path.

### Product, installation and distribution (P9)

- [~] `P9-002` Browser chat, settings, activity, approvals, memory and tool navigation exist. Continue user-flow/accessibility verification, canonical transcript browsing across browsers/devices and the remaining provider-specific setup work. A Tauri rewrite is not required.
- [~] `P9-003` User service install/uninstall exists. Resolve documented service/manual-daemon ownership cases and verify native lifecycle; system-wide modes remain optional.
- [ ] `P9-004` Verified prebuilt install path: native OS/architecture detection, checksums/signatures, PATH, onboarding and doctor. Source bootstrap scripts already exist and require a toolchain.
- [ ] `P9-005` Signed native artifacts, complete dependency notices/SBOM, provenance, checksums and release notes.
- [ ] `P9-006` Atomic update, schema preflight/backup, health checks, rollback, channels and version/split-brain diagnosis.
- [ ] `P9-007` Clean native install/update/rollback/repair/uninstall without a development toolchain.
- [ ] `P9-008` Legacy data importer only if the owner has a named surviving source and requests migration; dry-run, excluded keys/certificates, provenance, backup, rollback and explicit finalization. Removed prototype source is not a live data location.
- [~] `P9-009` Prototype removal is already complete (2026-10-13). Preserve historical license/provenance; resolve any requested legacy-data migration under `P9-008` rather than deleting the prototype again.
- [~] `P9-010` Telegram is delivered. Additional authenticated messaging clients are `P11-019`.
- [~] `P9-013` Guided terminal setup, hidden key entry, provider/model tests and owner-required ElevenLabs test are built under ADR-0214. Native-terminal and owner-account onboarding evidence remains with `P11-007`.
- [~] `P9-021` Model/fallback and phone/voice testing paths exist. Verify each current key-test surface and add any missing search-key check without duplicating existing tests.
- [ ] `P9-017b` Public distribution: Apache-2.0 is selected; complete dependency notices, signing/notarization, publish all intended packages and verify clean-machine installation before advertising signed downloads.
- [ ] `P9-022` Clean Linux/macOS/ARM installation and lifecycle acceptance. Ubuntu 24.04 x86_64 has an earlier end-to-end record, with unresolved service ownership cases; AlmaLinux, ARM and macOS are unverified. Preserve the [exact observations and suggested fixes](docs/history/todo-p9-p11-evidence-through-2026-10-16.md#p9-022).
- [ ] `P9-069` Residual product polish only: second search provider, broader browser CI, remaining HUD decomposition where useful, replay UI and ntfy authentication. Browser tools, routing and spend guards are delivered; semantic retrieval and distribution have their own IDs.
- [~] `P9-024` Built-in capability expansion is delivered; further capability now follows P11. The old 37-tool snapshot is historical, not a current registry count.

The completed installation/UI evidence is in [P9–P11 history](docs/history/todo-p9-p11-evidence-through-2026-10-16.md). `P9-001` / `P9-019` Tauri remain parked until a local voice/tray requirement needs them.

### Capability and reach (P11)

- [~] `P11-001b` Credential support is built; close the account-specific checks above. Automatic OAuth client registration, per-app credentials, hot registry replacement/`tools/list_changed`, cold-start package download budgets, MCP-output evaluation and platform evidence remain separate.
- [~] `P11-001f` Phone-agent endpoint integration and owner calls exist; broader telephony acceptance belongs to P8, not a second implementation of the owner-call path.
- [ ] `P11-001e` Per-launch, revoked-at-end scoped bridge from an external agent to JARVIS tools; real approvals, run linkage, cancellation/timeout and denied-call falsification. Amend ADR-0039 explicitly when implemented.
- [~] `P11-003` Built-in Playwright browsing exists. Remaining work: origin policy, intentionally visible/authenticated mode and repeated hostile-page evaluation across models; a single resisted page is insufficient.
- [~] `P11-005` Telegram has owner-phone evidence; verify current formatted answers, image/approval paths and failures on the selected real account. Broader messaging is `P11-019`.
- [~] `P11-007` First-run model/required-voice testing is built. Verify the selected default voice with an owner account, native hidden entry, clean-machine source/setup journey and clear recovery from rejected configuration.
- [~] `P11-010` XLSX/CSV/DOCX/PDF creation is delivered. PowerPoint, charts, richer Unicode PDF output and the owner's actual sales-workbook workflow remain.
- [~] `P11-011` Inline MCP screenshots/images and Telegram forwarding exist. Image generation, charts and saved-file screenshot display still need their own implementation/acceptance.
- [ ] `P11-012` Owner-managed authenticated browser profiles, site grants and explicit profile wipe; verify on an owner-named real service. MCP OAuth does not itself implement browser cookie/session ownership.
- [~] `P11-013` Managed host processes/dev previews exist under ADR-0222; these are approved host execution, not a sandbox. A no-Docker confined analysis runtime and enforceable sandboxed long-running jobs remain separate.
- [~] `P11-014` Granted-folder FTS5 knowledge search is built. Verify owner-corpus answers with source passages, add indexed-state visibility/OCR/cross-project views as needed, and add semantic retrieval only after word-only measurements justify it.
- [ ] `P11-015` OS-backed secrets vault with explicit private-file fallback and tested migration/recovery on Windows/macOS/Linux. Current named MCP keys and OAuth grants remain permission-protected files, not encrypted keychain storage.
- [~] `P11-016` Calendar overlap/delete and contact duplicate/merge tools exist. Complete owner calendar checks; preserve current suppression semantics (save/import/merge preserve; authorized deletion removes).
- [~] `P11-017` Secret-path webhook ingress is built. Verify one real external-service event, replay deduplication and invalid-path refusal. Provider signatures/HMAC are a separate declared capability, not a guarantee of the current route.
- [ ] `P11-018` Stateful monitoring: notify only on meaningful state change/recovery, with a daily budget; prove a real down/up cycle produces exactly two notifications.
- [ ] `P11-019` Add Slack/WhatsApp/SMS only for named owner jobs, through authenticated clients; verify paired identity and one-action approval per platform.
- [ ] `P11-020` Maps/routing/invoicing through MCP first, for a named provider/job. Payment capabilities remain high-risk and require explicit owner review for each outward effect.
- [~] `P11-021` Local/public-commit skill-bundle discovery and complete installation exist. Remaining scope: finding an appropriate missing MCP capability, owner-approved setup, private/global catalogs and safe updates; no automatic global/plugin installation is implied.
- [ ] `P11-022` Eval trend export (series, not only one printed result) and an explicitly configured periodic wishlist review that proposes changes. No schedule is created merely by this backlog item.
- [~] `P11-023` Old audit defects were fixed; keep residual per-project unattended permissions, parallel approval behavior, spreadsheet-coordinate output and fired one-shot handling explicit. Calendar deletion and skill bundle discovery are no longer missing features. [Original findings](docs/history/todo-p9-p11-evidence-through-2026-10-16.md#p11-023).

## Deliberately deferred

- `P6-001`, `P6-002`, `P6-004`–`P6-006`, `P6-008`: multi-step workflow engine/outbox/compensation until ordinary runs and schedules cannot satisfy a named requirement.
- `P5-006`–`P5-008`, `P7-005`, `P7-007`: provider/framework-specific adapters are superseded by MCP/ACP-first integration. Native adapters need a documented unmet requirement.
- `P8-002`–`P8-006`, `P8-012`–`P8-017`: vendor-shaped voice transports/realtime room until used; provider-neutral policy still applies.
- `P9-001`, `P9-019`, `P11-009`: desktop shell/native desktop automation until a concrete local-voice/tray/task requirement.
- `P10-001` server profile/TLS/runbooks; `P10-002` PostgreSQL parity; `P10-003` device enrollment; `P10-004` secure remote access; `P10-005` roles; `P10-006` service objectives; `P10-007` extra infrastructure: all remain demand-gated. The local daemon/tunnel is not the complete multi-device profile.

## Evidence and maintenance

Completed detail and original wording remain in the [history index](docs/history/README.md); research records retain source URLs, versions and access dates. An archive's unchecked box or failed historical run is not automatically today's next task or current test status. Update the active summary and relevant guide when behavior changes, add an ADR amendment link when superseding a decision, and run the documentation link/ADR checks with the normal required gates.
