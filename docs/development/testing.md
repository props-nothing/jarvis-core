# Testing Strategy

## Principle

Test at the boundary that owns the behavior. A synthetic model proves JARVIS orchestration; it does not prove a provider. A recorded provider fixture proves decoding; it does not prove current authentication or service availability. A live smoke proves one real path; it does not replace deterministic edge-case tests.

## Test Classes

### Unit

Pure domain invariants, state transitions, policy tables, ranking, parsing, scheduling, redaction, and error mapping. No network, wall-clock dependence, global profile, or real secret.

### Repository

Run each repository contract against SQLite and PostgreSQL where supported. Cover constraints, transactions, concurrency conflicts, migrations, pagination, deletion, leases, and outbox atomicity.

### Adapter contract

Exercise a model/runtime/tool/connector/voice adapter against an official schema, independent reference implementation, or sanitized recorded wire fixture. Verify normalization, not only happy-path deserialization.

### Integration

Run multiple JARVIS components together with controlled fakes: daemon/API/application/storage, policy/approval/tool, event/workflow, runtime protocol, and voice-session binding.

### Platform

Run on native disposable Windows, macOS, and Linux environments for paths, permissions, IPC, services, keychains, notifications, audio devices, installers, updates, and uninstall. Do not emulate these conclusions from one OS.

### Live provider

Opt-in, credential-gated, cost-labelled tests against real external services. They verify auth, current wire shape, permissions, quotas, and provider outcomes. They use dedicated test accounts/resources and clean up safely.

### End-to-end

Start from a packaged or release-like installation and complete a user journey through a real client. Assertions include durable records, visible output, policy, audit, restart, and cleanup.

#### Implemented: the Phase 1 acceptance gate

`tests/e2e` is a workspace member holding the process-level gate. It builds and runs
the real `jarvisd` and `jarvis` binaries against a temporary portable root, because a
library test proves libraries and the roadmap gate names an installation:

```text
cargo build --workspace
cargo test -p jarvis-acceptance --test phase1_gate
```

Set `ACCEPTANCE_REQUIRE_BINARIES=1` to make a missing build fail instead of
skipping. CI sets it. The variable deliberately avoids the `JARVIS_` prefix, because
the daemon rejects unknown `JARVIS_*` variables as configuration errors.

**A variable that turns a skip into a failure must name the artifact it is about.** A
second skip-guard exists for a *different* artifact: the stdio tests in
`jarvis-mcp-transport` spawn the `fixture-peer` binary, which `--all-features` builds
and a plain workspace run does not. That guard reads
`ACCEPTANCE_REQUIRE_FIXTURE_PEER=1`. The two were once one variable named
`ACCEPTANCE_REQUIRE_BINARIES`, and because the workspace test step runs
`cargo test --workspace --all-features` — which builds the fixture but **not**
`target/<profile>/jarvisd` — the phase gate in that step asserted a binary the step
never produces. It failed on all three operating systems for four consecutive pushes.
`cargo test` does not build a package's binary into `target/<profile>` at all, so any
check for an application executable belongs in a step that runs `cargo build` first.

Two properties are worth preserving if this gate is edited:

- **Readiness is scoped to the process.** A hard-terminated daemon never records its
  `stopped` transition, so its last state stays `ready`. Matching any `ready` row
  accepts a stale row from a previous run; the gate matches the process ID it started.
- **Drain piped output only after termination.** Reading a child's piped stdout or
  stderr while it runs blocks on EOF that arrives only at exit, which hangs the gate
  instead of failing it.

## Documentation consistency

```text
node --test scripts/test-docs.mjs
node scripts/check-docs.mjs
```

CI checks repository-owned Markdown for local file/heading links and duplicate or mismatched canonical ADR identifiers. Dated evidence belongs in `docs/history/`; an archived result does not prove a current acceptance gate. The checker excludes vendored documentation and remote URLs, so capability claims and external contracts still need review against code and official sources. For the separate website checkout, also run `node scripts/check-docs.mjs --root frontend` from the core repository.

## Browser console regressions

The embedded HUD has dependency-free Node tests for configuration-overview honesty, settings search and tab focus, offline appearance migration/reset, storage failures, stale loads, restart availability, draft safety, live-counter truthfulness, accent-independent approval colors, shortcut boundaries, dialog focus restoration, quiet atmosphere and stationary reduced-motion orbits:

```text
node --test scripts/test-hud-console.mjs scripts/test-hud-settings.mjs
cargo test -p jarvisd hud::tests --lib
```

CI runs these JavaScript tests, syntax checks and the deterministic typed-HUD rebuild once on Linux. Native Rust tests on Windows, macOS and Linux still check the production assets and content-security boundaries. The Node tests use controlled DOM shims; neither suite proves a real provider connection or microphone path. For visual changes, additionally open the actual HUD assets in a browser, check narrow and desktop layouts, dialogs, picker Escape behavior, and offline/approval states against isolated fixtures. Do not send test requests through an owner's live model or change their saved permissions.

## Release-flow regressions

```text
node --test scripts/test-release-flow.mjs
bash scripts/test-package-release.sh
```

These dependency-free fixtures exercise eligible CI events, exact commit selection, quality/draft job wiring, checksum and license refusal, and draft-only attachment. They also execute the production CI lint/test commands with a failing Cargo stand-in under Bash `-eo pipefail`: diagnostics must reach both the live log and the annotation file, and the step must still fail. Real hosted triggers, all platform archives and GitHub uploads need native Actions evidence.

The workflow regressions also compare the native test matrix against workspace membership: every package must run exactly once per OS, with exactly one acceptance lane per OS. They retain all three acceptance gates, both missing-binary guards and the single preceding workspace build. Release/archive shell regressions continue to run on each native OS.

The isolated daemon job explicitly builds `jarvis-mcp-transport`'s `fixture-peer` binary before testing. `cargo test -p jarvisd --all-features` enables the selected package's features; it does not build another package's executable. Keep `ACCEPTANCE_REQUIRE_FIXTURE_PEER=1` and the build step together, including their ordering regression. A developer's existing `target/debug/fixture-peer` must not hide a missing CI prerequisite.

The real managed-preview fixture allows 30 seconds for Windows child startup and ten seconds on other platforms, polling until ready without a fixed delay. Timeout failures include the last process state and captured output. This allowance is only fixture setup: real HTTP content, occupied-port refusal, honest browser-verification state, explicit stop and closed-port checks all remain required. A child that never listens must still fail.

The background coding-activity test waits for both public snapshots before requesting Stop. Cancelling after just the first snapshot can prevent the second from arriving, contradicting its later two-update assertion. Bounded 30-second readiness/completion waits allow loaded native runners without changing production deadlines or retrying a failing assertion. Its scratch path includes `private` on every OS, matching macOS canonical temporary paths: secrecy assertions reject the exact fixture thought/input/output markers, never a legitimate path component.

CI annotates stack overflows, fatal runtime errors and aborted harness exits separately from ordinary panics. An abort can prevent libtest from printing its accumulated failure details. The annotation fixtures place crash evidence before the final log tail and include a successful test named with `assertion`; neither the fatal error nor the distinction between success and panic may be lost.

Workbook XML nesting must be checked before entering the recursive XML parser, not only on the parsed tree. The selected parser also needs a fixed adapter-owned stack budget for **accepted** nesting: the 128-level bound alone does not guarantee that it fits macOS or another small-stack caller. Parsing runs on a joined scoped worker with a 16 MiB stack; caller/CI stack settings, DTD and node/depth bounds stay unchanged. Bounded-stack child regressions cover accepted 127/128-level forms, empty elements, text/comment/PI boundary behavior and lexical decoys as well as over-limit refusal. A refusal-only small-stack test misses the accepted-depth overflow. Restoring caller-stack parsing must fail the expanded child regression without aborting the parent harness.

## Deterministic Agent Tests

Build a scripted model/runtime that emits a sequence of normalized events. Use it to test:

- streamed text and activity
- one and multiple tool requests
- malformed/unknown tool calls
- approval pause, approve, deny, expire, and resume
- model retry and context overflow
- cancellation at every state
- runtime crash and fallback
- usage/cost accounting
- restart from each persisted boundary

Never call a real model in the default test suite.

## Falsification

Security and correctness guards require a control showing the test can fail. Examples:

- remove workspace filtering and prove the isolation test catches a cross-workspace row
- change an approval intent after decision and prove execution is rejected
- alter one webhook byte and prove signature verification fails
- simulate provider acceptance plus lost response and prove state is `unknown`, not failed/success
- reintroduce path traversal/symlink escape and prove the sandbox/filesystem suite detects it
- disable idempotency uniqueness and prove duplicate event/tool delivery creates an assertion failure

A test that can pass without observing the claimed boundary is not evidence.

## Fixtures

Every external wire fixture includes:

- provider/protocol and version/date
- official source or sanitized capture procedure
- operation represented
- removed/redacted fields
- expected normalized result
- license/terms note when relevant

No fixture contains real tokens, cookies, emails, names, phone numbers, calendar content, transcripts, audio, private URLs, or provider account IDs. Use structurally valid reserved example values.

Keep malformed, failed, paginated, rate-limited, empty, duplicated, out-of-order, and provider-upgrade fixtures alongside happy paths.

## Scratch Directories And Asynchronous Teardown

Ordinary storage and daemon behavior fixtures use `SqliteDatabase::test_fixture`, available only to storage tests or through the opt-in `test-support` feature (a daemon dev-dependency). The first call runs the production migrations and closes the pool before caching the database bytes. Each fixture copies those bytes into its own new file and reopens through production validation, ACLs and connection settings. No live pool or mutable database is shared across test runtimes. `synchronous=FULL`, WAL, foreign keys and the production trust settings remain in force.

Migration, startup and permission tests must continue to call `SqliteDatabase::open` on fresh databases. Fixture regressions check current schema/identity, independent writes, persistence after reopen, production pragmas and refusal to overwrite an existing file. On local Windows, the same 21 approval tests with four threads fell from 5.45s to 1.71s; this is a fixture measurement, not a hosted-run estimate.

Credential fixtures must canonicalize their own newly created temporary root before constructing `AppPaths`. macOS's temporary path can contain a symlinked ancestor; the production credential store deliberately rejects such ancestors, even above `config`. Do not relax that guard to accommodate a fixture. Native symlink/junction tests explicitly cover an aliased portable root as well as managed directories and key files.

A fixture's scratch directory **must** be removed through `jarvis_core::remove_scratch_dir`, and its name **must**
contain `jarvis_core::scratch_tag()`. Both rules exist because violating either is invisible at runtime: a
violation leaks a directory and every test still passes.

**Naming.** A name built from a pid, a timestamp, or a counter that resets is reusable, so a directory left behind
by a killed run can be reopened by the next one — which is how a whole suite's fixtures began failing with
`UNIQUE constraint failed`. A `UUIDv7` is unique across processes and across runs.

**Removal.** `std::fs::remove_dir_all` in a guard's `Drop` **cannot** work for a fixture holding a database, and
the reason is worth knowing rather than rediscovering: `sqlx` releases the SQLite file late and on a spawned task,
so the failure is a race. Measured on Windows — drop then remove immediately fails **every** time, while drop,
wait 250 ms, then remove always succeeds. Two tempting fixes are worse than the problem:

- a bounded **blocking** retry still fails, because sleeping starves the current-thread runtime the pool needs;
- awaiting a close on a **fresh runtime inside a worker thread deadlocks**, and a deadlock in teardown is worse
  than a leaked directory.

Handing the removal to a **detached** thread that retries works: it does not starve the runtime, and it keeps
trying across the point where the runtime tears down. The window is **measured**: no retry at all left **124**
directories per full-workspace run, and the shipped version leaves **7–10**. Two designs measured *worse* or no
better and are recorded so they are not retried — a **ten-second** per-call window left the same handful, and an
**always-running** retry loop left **22**, because a thread competing for CPU across the whole suite delays the
runtime teardowns that are what actually release the handles. What remains are fixtures holding an
`Arc<SqliteDatabase>` — through a `ToolPipeline` — past the directory guard, so the handle is released at the end
of the test (which the window covers) or at process exit (which nothing in-process can reach). Such a fixture
should await its own close before the guard drops.

The rule is enforced by a source scan in `jarvis-core`'s testkit rather than by a behavioural test, because no
assertion can observe a directory *not* leaking without measuring the filesystem around a whole suite. The scan
requires the discarded-result form (`let _ = …remove_dir_all(&self.0)`), so it catches a bypass without flagging
its own explanation or an unrelated removal.

## Time And IDs

Inject clocks, UUID sources, randomness, and retry schedules. Tests use deterministic IDs and explicit time advancement. Scheduler suites cover daylight-saving gaps/overlaps and missed-run policy using real IANA zones.

## Concurrency And Crash Tests

- race two optimistic updates
- expire and reacquire a lease with fencing
- crash before/after each transaction commit and external effect
- deliver the same event concurrently
- cancel during model stream, tool call, approval wait, and workflow wait
- restart with orphaned external runtime process
- verify no terminal state regresses

Use model checking/property tests for small state machines where practical.

## Security Tests

Follow the list in [security.md](../architecture/security.md). Security fixtures are content-minimized. Secret redaction tests insert canary values into every structural location (headers, path, query, userinfo, JSON, errors) and assert no normal log/trace/diagnostic contains them.

## API And Protocol Tests

- OpenAPI compatibility diff
- local protocol handshake/version window
- SSE sequencing/reconnect/resync
- WebSocket auth, cancellation, backpressure, and frame bounds
- runtime worker protocol conformance for each language worker
- MCP Inspector plus independent SDK cross-tests
- ElevenLabs Custom LLM SSE fixtures for both supported endpoint shapes
- turn-event fixtures covering `EndOfTurn`, `EagerEndOfTurn`, `TurnResumed`, `FalseInterruption`, and `Backchannel`. A scripted pause-heavy utterance must produce a boundary at the model's pause, not at the earliest silence; a silence-window implementation passes every framing test and fails only this one.
- webhook raw-body signature and replay

## UI Tests

When the UI exists:

- component tests for rendering/state
- generated-client contract tests
- Playwright user journeys against a real test daemon
- desktop smoke on native OS
- screenshots for responsive desktop/mobile widths
- accessibility checks for keyboard, focus, labels, contrast, motion, and screen readers

Approval UI tests verify the exact target/effect shown matches the intent hash executed.

## Commands

The Phase 1 Rust baseline will be:

```text
cargo fmt --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
cargo deny check advisories bans licenses sources
```

CI runs formatting once on Linux, and Clippy plus all-feature locked tests on native Windows, macOS and Linux. Lint and tests are separate jobs. Windows tests are split into storage, daemon and the remaining workspace; the latter builds the acceptance binaries once and runs all three phase gates. Linux/macOS each run the full workspace and the same acceptance gates. Windows Rust jobs allow 60 minutes; other native Rust jobs allow 30. The test step stops after 50/25 minutes to leave room for diagnostics and cache saving.

Pinned Rust-cache actions retain dependency downloads/build products, keyed separately for Clippy, test partitions and release targets. PRs restore but do not save caches. Cache misses still run the same locked checks; no test or gate is conditional on a cache hit. Logs and Cargo build timings are uploaded even after failure. Main pushes and PRs trigger CI; tags call it through Release, avoiding a duplicate full tag run. Manual CI remains available. If repository rules require named job checks, update them to the new job names before merging.

The dependency-policy command runs once on Linux against the same committed lockfile. Later commands must be added to a root task runner only after they exist. CI and this document stay synchronized. Live tests require explicit flags and never run merely because a developer happens to have a credential in the environment.

## Release Evidence

A release candidate needs:

- unit/repository/contract/integration suites
- native platform/install/update/uninstall lanes
- database upgrade and restore drill
- security/static/dependency/license/secret scans
- selected live provider smokes
- end-to-end acceptance scenarios
- signed artifact verification from a clean machine

Coverage percentage is a signal, not the release gate. Boundary and failure-path evidence is the gate.
