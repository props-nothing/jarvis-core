# JARVIS Implementation Backlog

**Current acceptance work and remaining gates. Historical test narratives live in [docs/history](docs/history/README.md).**

[Home](README.md) / [Roadmap](ROADMAP.md) / [Documentation](docs/README.md) / [Definition of done](docs/development/definition-of-done.md)

Work from the first unchecked acceptance slice unless the owner selects another. A checked box requires its stated test and documentation evidence. Built, fixture-tested, live-verified and accepted are different states; do not turn an old partial implementation into a completion claim by moving its notes.

## Current acceptance slice: signed binary update and rollback (2026-10-18)

Continue owner-selected `P12-001` / `P9-006` from the accepted signed first-install
and offline database recovery foundations below.

- [ ] Define and implement verified binary staging/activation, retained previous-version recovery and health checks, with an ADR for durable update state and service ownership.
- [ ] Prove failed/interrupted activation preserves a recoverable installation and database; define schema compatibility before supporting a downgrade.
- [ ] Verify service reconciliation and update/rollback on Windows, macOS and Linux, then complete clean-machine onboarding/install/repair/uninstall acceptance (`P9-007`, `P9-022`).

Native certificate signing still needs owner-provided credentials. Database
snapshots alone do not complete binary rollback or whole-profile recovery.

Initial offline implementation: [ADR-0242](docs/adr/0242-binary-activation-retains-evidence-and-requires-offline-ownership.md)
and [runbook](docs/operations/binary-update.md). `update stage|activate|status|rollback|recover`
now has signed staging/reverification, installation/profile exclusion, retained
previous binaries and database snapshots, same-schema activation, recovery journals
and startup/doctor refusal. Rollback preserves current database content. Ordinary
install refuses to overwrite retained update evidence. The helper avoids Windows
self-replacement; the installed service path is preserved and uncertain ownership
is refused.

This is the first offline portion, not completion of the acceptance boxes above.
Executable/version/protocol and offline database checks are implemented; real
daemon readiness, service-manager reconciliation, schema-changing updates,
new-release signed activation and clean-machine lifecycle remain open. Native CI
now includes the new tests and public signed-staging/startup-refusal gate; hosted
results for this revision are still required. Local evidence is recorded in
[binary-update research](docs/research/integrations/binary-update.md).

## Completed acceptance slice: offline database recovery (2026-10-18)

Owner-selected `P12-001` / `P9-006` continuation: establish a verified database
recovery path before binary activation. Full updates and clean-machine onboarding
remain separate acceptance work.

- [x] Add owner-invoked `backup create|verify ID|restore ID|recover`, actual daemon-lock exclusion, current-schema/sidecar guards and retained pre-restore snapshots.
- [x] Persist a pending restore before atomic replacement; block daemon SQLite access until explicit recovery reconciles the before/after image without guessing. Doctor diagnoses pending/malformed journals before inspecting SQLite.
- [x] Pass real-value restore/undo, corruption/schema/foreign-key/sidecar refusal, actual process-death recovery, daemon-start refusal and guard falsification; 3,241 Windows workspace tests pass (six existing opt-in ignores), plus formatting, strict Clippy, 66 release tests, workflow lint, docs and dependency policy.
- [x] Native CI acceptance on Windows, macOS and Linux for `40fc573`: all 14 jobs passed in [CI 39076425958](https://github.com/props-nothing/jarvis-core/actions/runs/39076425958), including recovery tests, strict lint and rebuilt process gates.

[Design](docs/adr/0241-database-restores-require-offline-ownership-and-a-recovery-journal.md),
[research/evidence](docs/research/integrations/offline-database-recovery.md),
[recovery runbook](docs/operations/database-recovery.md).

## Completed acceptance slice: signed release manifests (2026-10-18)

Owner-selected `P12-001` continuation. The owner confirmed no native signing
credentials yet; Authenticode/notarization remain open without blocking keyless
publisher verification.

- [x] Record official Cosign v3.1.3/GitHub OIDC contracts and pins; bind all five archives to repository, tested commit, tag and version (ADR-0240).
- [x] Sign and verify before publication; verify twelve uploaded assets. Installer signed mode checks publisher identity before archive download/execution and never falls back to unsigned mode.
- [x] Exercise real upstream signature acceptance and tamper/wrong issuer/wrong identity rejection on Windows, plus rejection through the production installer. Add native Linux/macOS/Windows CI coverage.
- [x] Pass 66 release/manifest tests, native Windows/Git Bash installer fixtures, real verifier tests, signature-guard falsification, 3,228 workspace tests (six existing opt-in ignores), formatting, strict Clippy, documentation and dependency policy.
- [x] Hosted repository-identity publication at `70cfc6e`, followed by public signed installation acceptance using the `40fc573` bootstrap/harness on Linux x86_64/ARM64, macOS ARM64 and Windows. All four compatibility jobs passed in [CI 39076425958](https://github.com/props-nothing/jarvis-core/actions/runs/39076425958); restricted PATH/fresh-profile checks do not establish a clean VM or live-provider onboarding.

[Design](docs/adr/0240-release-manifests-bind-artifacts-to-the-publishing-workflow.md),
[research/evidence](docs/research/integrations/release-signing.md),
[usage](docs/operations/install-and-release.md#pinned-prebuilt-first-install).

Hosted follow-up: `70cfc6e` passed all native CI jobs and published twelve signed
assets in [Release 39074537468](https://github.com/props-nothing/jarvis-core/actions/runs/39074537468).
Post-publish installation acceptance failed in the harness: Linux/macOS reached
doctor with a nonexistent portable root; Windows timed out discovering absent
tools through PowerShell. The corrected harness creates the empty root and checks
Windows executable search paths directly. All four compatibility jobs subsequently
passed at `40fc573`, verifying the pinned public `70cfc6e` release, both version
commands, fresh-profile diagnosis and existing-install preservation without
Git/Rust/Node/Python/GitHub CLI on PATH. This does not establish publication or
post-publication installation of `40fc573`; each new release retains that gate.

## Completed local slice: pinned prebuilt first installation (2026-10-17)

First bounded `P12-001` / `P9-004` slice in the owner's competitive catch-up order.

- [x] Add opt-in Bash and PowerShell first-install scripts using an exact official release, package version and supplied SHA-256, without Git/Rust/Node/Python.
- [x] Bound downloads/decompression; verify before opening/executing; select one regular executable, check its version, preserve existing installations, and hand off PATH/setup/doctor through existing commands.
- [x] Execute real archive fixtures, corrupt-hash guard falsification and a real public Windows archive install into an isolated directory without PATH/profile changes.
- [x] Pass focused scripts, 61 release tests, all 3,228 workspace tests (six existing opt-in ignores), formatting, strict Clippy, six documentation tests/link checks and dependency policy. Native Bash/PowerShell CI passed in all ten jobs for `0b7712d` ([run](https://github.com/props-nothing/jarvis-core/actions/runs/39070807329)); the signed continuation needs its own hosted evidence.

Design and evidence: [ADR-0239](docs/adr/0239-prebuilt-installation-pins-the-release-and-digest.md),
[research](docs/research/integrations/prebuilt-install.md),
[usage](docs/operations/install-and-release.md#pinned-prebuilt-first-install).
At that slice, signed publisher verification, notarization, complete dependency notices/SBOM,
atomic updates/schema rollback and clean Windows/macOS/Linux lifecycle remain
open under `P12-001`. Website source bootstrap and ADR-0214 voice setup are unchanged.
The public Windows archive also installed and ran both version commands with PATH
restricted to Windows system tools, with no Git/Rust/Node/Python/GitHub CLI visible.
This is not a clean-machine onboarding claim.

## Completed acceptance slice: audit, failure isolation and coordinated shutdown (2026-10-17)

Owner-selected `P7-011` follow-up to durable cancellation and dispatch.

- [x] Drain admitted native/coding/managed work before closing dependencies; preserve approval waits and fail closed on deadlines or unavailable cleanup evidence.
- [x] Verify child audit ownership/reopen, contained native model failure and a real scripted ACP process crash with parent and unrelated work still usable.
- [x] Distinguish graceful cancellation before restart from abrupt interruption without replay, with retained files and fresh approved continuation.
- [x] Pass focused tests, guard falsification, fresh binary process gates, full workspace formatting/lint/tests, documentation and dependency checks.

Design: [ADR-0238](docs/adr/0238-shutdown-drains-owned-work-before-closing-dependencies.md).

Windows validation: **3,228 workspace tests passed**, zero failures and six existing
opt-in checks ignored, with freshly rebuilt binaries and
`ACCEPTANCE_REQUIRE_BINARIES=1`. All Phase 1-4 process gates and both coding lifecycle
gates passed. Focused coverage passes 20 daemon shutdown tests, four storage
shutdown tests and 15 native delegation end-to-end tests. Strict all-target,
all-feature Clippy, formatting, six documentation tests, local links and
advisory/dependency/license checks passed.

Removing the quiescence guard settled a received run while admission was open;
removing the approval-wait exclusion cancelled a parked run. Both falsification
assertions failed, exact source bytes were restored, and the restored tests passed.
The real daemon gate exposed undriven API-created received runs; they now receive
an audited cancellation only after closed admission is idle. A lost coding worker
still times out, exits unsuccessfully and records `shutdown-failed`. Initial
process/lint failures and final evidence are retained under
`artifacts/p7-011/draining/`.

This completes the selected implementation/acceptance slice.
Windows/macOS/Linux CI passed all ten jobs for `962cf079` in
[run 39069495553](https://github.com/props-nothing/jarvis-core/actions/runs/39069495553),
closing the native P7-011 gate. Scripted ACP tests do not
certify live vendors, and native execution is not memory or panic-abort isolation.

## Completed acceptance slice: cross-runtime cancellation and dispatch (2026-10-17)

Owner-selected `P7-011`: coordinate native/coding parent Stop and prevent repeated
worker launches for one durable tool call.

- [x] Add migration 0031 with immutable per-call dispatch identity, coding ownership and durable cancellation intent; preserve unknown legacy provenance and verified upgrade backups.
- [x] Atomically cancel linked native/coding work, block late family admission, preserve completed parent history and keep individual Stop local.
- [x] Adopt duplicate dispatches without respawning, preserve cancellation on settlement/restart, and exclude delegated parents from automatic continuation.
- [x] Pass adapter/process races, guard falsification, full workspace formatting/lint/tests and documentation/dependency checks.

Design: [ADR-0237](docs/adr/0237-delegation-dispatch-and-family-stop-are-durable.md).
The subsequent [ADR-0238](docs/adr/0238-shutdown-drains-owned-work-before-closing-dependencies.md) slice adds audit/failure acceptance and coordinated draining.
Per-call admission does not promise exactly-once external effects, duplicate HTTP
request handling or automatic crash takeover.

Windows validation: **3,215 workspace tests passed**, zero failures and six existing
opt-in tests ignored. Binaries and examples were rebuilt; the full suite ran with
`ACCEPTANCE_REQUIRE_BINARIES=1`. All Phase 1-4 and coding lifecycle process gates
passed. Focused coverage includes nine coordination tests, the schema-30 upgrade,
18 coding-manager tests and 13 native delegation end-to-end tests. Cleanup failures
and unavailable execution results remain failed even when Stop was recorded.

Removing queued cancellation protection failed the no-launch assertion. Removing
submitted-call checks admitted invalid origins and failed their guards. Removing
dispatch lookup and uniqueness produced distinct native and coding identities for
one call; restored source passed. The native race uses independent proposed run
and session IDs. Exact byte restoration and mutation logs are retained under
`artifacts/p7-011/coordination/`, alongside the final workspace evidence.

Formatting, strict all-target/all-feature Clippy, six documentation tests, local
links and advisory/dependency/license checks passed. Native hosted CI and fresh
live-provider acceptance remain separate; the new cases run in the existing
Windows/macOS/Linux suites and process gates. This completes the selected
cancellation/dispatch slice, not the broader P7-011 acceptance item.

## Completed acceptance slice: shutdown work admission (2026-10-17)

Continue `P7-011` after committing and pushing the existing P7-009/P7-010 and HUD
work as `1fcd99e`. Close the gap between the stop/restart snapshot and new work.

- [x] Share a domain admission gate across native creation/execution, coding queue/dispatch, tool calls/resumes and automation claims.
- [x] Freeze admission before shutdown checks, reopen on refusal/cancellation/launch failure, and commit before success; preserve the restart helper handshake.
- [x] Prove concurrent admission, unavailable state, held approvals, remote calls and queued dispatch fail closed; run falsification and required workspace/process/documentation checks.

Design: [ADR-0236](docs/adr/0236-shutdown-reserves-work-admission-before-checking-idleness.md).
This slice closes new admission on accepted shutdown. Cross-runtime cancellation
and remaining duplicate-dispatch/audit/isolation acceptance stay in `P7-011`.

Windows validation: **3,202 workspace tests passed**, zero failures and six
existing opt-in tests ignored. All CLI/daemon binaries and examples were rebuilt;
`ACCEPTANCE_REQUIRE_BINARIES=1` required actual process gates. The coding lifecycle,
Phase 1-4 gates, six domain admission tests and nine daemon regressions passed.
Formatting, strict all-target/all-feature Clippy, six documentation tests,
documentation links and advisory/dependency/license checks passed.

Removing the closed-admission predicate made three domain guard tests fail. The
predicate was restored before final checks. Regression coverage includes the
restart helper handshake, rollback on refusal/failed launch, approved-call and
parked-run refusal, remote execution without a durable call row, queued coding
work after capacity release, and schedule/webhook claims retained for retry.
Evidence is retained under `artifacts/p7-011/`. New hosted CI/platform results and
live-provider acceptance remain separate; this slice changes no external API or
runtime contract and does not complete the broader P7-011 acceptance item.

## Completed acceptance slice: consistent delegation policy (2026-10-17)

Owner-selected `P7-010`: declare native/coding delegation consistently while
preserving their different execution authority and the owner's approval controls.

- [x] Add the canonical delegation effect and shared native/host contracts; use them for native delegation, coding launch aliases and continuation.
- [x] Keep host execution at risk 3, ordinary native admission at risk 1, per-tool trust, scope/denial/ceiling checks and no blind non-idempotent retries.
- [x] Withhold both management families from native children and enforce stored role on direct pipeline calls and approval resume.
- [x] Complete workspace formatting, strict Clippy, rebuilt process gates, workspace tests and documentation/dependency checks; preserve staged changes.

Design and upgrade behavior: [ADR-0235](docs/adr/0235-delegation-declares-work-and-its-execution-authority.md).
The launch tools are version 1.1.0; the existing receipt binding requires an old
pending 1.0.0 launch to be requested again after inspection and cancellation.
Cross-runtime cancellation, duplicate-dispatch coordination and shutdown draining
remain `P7-011`; the ACP bridge and enforceable sandbox remain `P7-003`.

Windows evidence: **3,187 workspace tests passed**, zero failures and six existing
opt-in tests ignored. The CLI/daemon binaries and examples were rebuilt and the
process gates ran with `ACCEPTANCE_REQUIRE_BINARIES=1`. Focused tests cover all
four launch definitions, effect serialization, no blind retries, per-ID trust,
risk ceilings, child scope refusal, full-grant direct calls and an earlier decided
approval. Removing the stored-child guard failed its negative test; removing
host code execution let trust bypass a risk-2 ceiling and failed its negative
test. Both guards were restored before final checks.

Formatting, strict all-target/all-feature Clippy, six documentation tests,
documentation links and advisory/dependency/license checks passed. Logs and
falsification evidence are retained under `artifacts/p7-010/`. The staged HUD
patch has the same before/after SHA-256. No fresh macOS/Linux CI or live-provider
acceptance is claimed; no external protocol or provider call changed.

## Completed acceptance slice: durable native parent/child tasks (2026-10-17)

Owner-selected `P7-009`: replace objective-based identity with immutable stored
lineage, define restart behavior and propagate explicit parent Stop. Preserve
unrelated staged HUD work and the completed coding acceptance evidence.

- [x] Add migration 0030, conservative legacy-role preservation and atomic parent/capacity admission.
- [x] Use stored lineage for permissions, model routing and discovery; expose parent IDs in run replies, agent results and CLI listings.
- [x] Propagate parent cancellation to unfinished children and held approvals, enforce cancellation at tool submission, and preserve restart identity without automatic family replay.
- [x] Pass focused migration/lifecycle/falsification tests, workspace formatting, strict Clippy, workspace tests and documentation checks; record evidence and preserve staged changes.

Design and operator semantics: [ADR-0234](docs/adr/0234-native-subagents-have-durable-lineage-and-parent-stop.md)
and [native sub-agents](docs/user/sub-agents.md). Native/coding policy alignment
is delivered in `P7-010` above; broader coordination and shutdown draining remain `P7-011`.

Windows evidence: **3,179 workspace tests passed**, zero failures and six existing
opt-in tests ignored, with rebuilt CLI/daemon binaries required by the process
gates. The Phase 2 gate now reopens a native family, reads lineage through HTTP
and uses the real CLI to stop a completed parent older than the recent-run page.
Its parked child stops while completed history remains unchanged. The Phase 3
gate preserves explicitly approved direct-call recovery after an interruption.
The cancellation gate therefore checks cancellation intent, rather than refusing
every terminal owning run.

Removing parent validation allowed forbidden nested delegation and failed its
negative test; removing the final cancellation predicate allowed a stopped call
to advance and failed its negative test. Both guards were restored. The final
storage suite passed **374 tests**, and the shutdown regression passed after its
fixture was upgraded from a text marker to a real stored child. Formatting,
strict all-target/all-feature Clippy, six documentation tests, the 368-file
documentation check, and advisory/dependency/license checks passed. Logs and
the original failed attempts are retained under `artifacts/p7-009/`.

The owner's staged HUD patch is byte-for-byte unchanged. The new tests run in the
existing native CI suites and process gates; no new hosted CI, macOS or Linux
result is claimed. Legacy children keep their restricted role with no invented
parent and can be stopped individually. External coding jobs retain their own
Stop controls.

## Completed acceptance slice: complete coding website delivery (2026-10-17)

Continue the next coding acceptance gate under `P7-004`/`P7-008`. Use a disposable
profile and a real signed-in agent; preserve unrelated staged HUD work. Structured
parent/child records (`P7-009`) remain the following architecture slice.

- [x] Add an explicitly enabled [real website acceptance harness](docs/quality/coding-website-acceptance.md) through actual JARVIS denial/approval, durable coding jobs and independent checks. Require a pinned browser verifier before account work.
- [x] Verify a complete responsive website with desktop/mobile-emulated interactions, reload persistence, managed preview cleanup and unchanged unrelated files; retain screenshots and failed attempts. Fresh sign-in, physical devices, notification reception and other native platforms/providers remain separate.
- [x] Pass harness refusal controls, documentation checks, workspace formatting, strict Clippy and workspace tests; preserve exact evidence and the owner's staged changes.

Windows evidence: pinned Codex ACP 2.2.2 built the disposable Harbor task board.
The first run reached the harness's explicit ten-minute deadline while reviewing
the site; JARVIS retained `timed_out`/unverified and skipped independent checks.
A fresh approved continuation used the existing files and a new task ID, completed,
and passed JARVIS's independent check. The original failed record remains unchanged.
Both 1440×1000 desktop and 390×844 mobile-emulated Chromium contexts passed add,
completion, filters, whitespace rejection, literal HTML-looking input, reload
persistence, overflow and browser-error checks. Managed preview Stop was confirmed
by terminal state and a closed port; the daemon exited successfully and sentinel/
verifier hashes stayed unchanged. Visual review confirmed the responsive layouts.
An offscreen fixed skip link appeared in a scrolled full-page mobile capture;
capturing from the top removes that screenshot artifact, with before/after viewport
evidence retained. This is a completed journey with explicit continuation, not an
uninterrupted ten-minute delivery claim.

The harness initially expected HTTP 403 when resuming a denied call. The real
gateway correctly returned non-retryable HTTP 409 with its existing approval
refusal; the assertion now requires that exact response and unchanged job IDs.
That failed harness attempt and the original timed-out run remain under
`artifacts/reviews/coding-website/run-CnJtlK/`; passing continuation evidence and
screenshots are in `continuation-uO3KmW/`. Five offline controls cover explicit
live opt-in, verifier version, profile ownership and a real symlink/junction escape.
Formatting, strict all-target/all-feature Clippy and **3,170 workspace tests**
passed (zero failures, six existing opt-in tests ignored); documentation, workflow
and dependency checks passed. Docker was unavailable. The staged HUD patch is
byte-for-byte unchanged. No new native macOS/Linux or hosted CI result is claimed;
the new offline guards are wired into CI.

## Completed acceptance slice: release verification and coding lifecycle (2026-10-17)

<a id="current-acceptance-slice-release-verification-and-coding-lifecycle-2026-10-16"></a>

Owner-selected follow-up: verify the exact pushed release, fix observed failures, then improve coding-task lifecycle using parallel review and implementation. Preserve unrelated changes and dated evidence. This slice extends `P7-004`/`P7-008`; structured native parent/child records (`P7-009`), coordinated shutdown draining and whole-website delivery remain separate.

- [x] Verify native CI and automatic public prerelease publication for `c48da8880e20304da4b13e4016d896ad22fc2e04`: all five packages, ten public assets, checksums and exact tag target. Record actual hosted results below.
- [x] [ADR-0233](docs/adr/0233-coding-cancellation-is-final-and-process-ownership-outlives-the-root.md): preserve Stop and selected timeout across agent exit/error/sign-in, prevent post-cancellation authentication or prompt replay, and retain the original deadline across retry. Prove durable cancelled status and refusal of late permissions.
- [x] Retain process ownership after root exit for commands, coding agents and managed processes; refuse unenforceable setup, disclose cleanup failures and abort abandoned readers. Prove real descendant cleanup and an unrelated-process control.
- [x] Run a real-daemon process gate for cancellation, forced graceful shutdown, restart interruption/no replay and freshly approved continuation with files preserved. Run the separately gated real Codex cancellation check; record provider or native-platform limits separately.
- [x] Run narrow regressions, formatting, strict all-target/all-feature Clippy, workspace tests, process gates and documentation/dependency/workflow checks. Preserve falsification logs and historical archive hashes.

Release verification (2026-10-16): `c48da8880e20304da4b13e4016d896ad22fc2e04` passed all ten [native CI jobs](https://github.com/props-nothing/jarvis-core/actions/runs/38964612120) and all five [package jobs plus automatic publication](https://github.com/props-nothing/jarvis-core/actions/runs/38966399553). The [public unsigned prerelease](https://github.com/props-nothing/jarvis-core/releases/tag/ci-c48da8880e20304da4b13e4016d896ad22fc2e04) has exactly ten assets and its tag resolves to the tested commit. All five Actions ZIP digests, contained archive checksums, release asset digests/sizes and ten anonymous public downloads match. Four native archive smokes passed; Intel macOS was cross-built with its runtime smoke skipped. No release repair was needed. Evidence: `artifacts/reviews/release-c48da8880e20304da4b13e4016d896ad22fc2e04/verification.json` and adjacent metadata/downloads. This closes the preceding runtime/guide revision's hosted gate, not the new lifecycle code's checks below.

Live cancellation (2026-10-16, Windows): the explicitly enabled test with pinned `codex-acp 2.2.2` passed in 16.02 seconds. Real Codex launched supplied Node parent/child processes; both were independently observed alive before Stop and gone afterward. Supplied files stayed unchanged, and the cancelled job survived SQLite reopening. Existing agent authentication was used; no owner profile was changed. This does not verify fresh browser sign-in, other agents/platforms or complete website delivery. Sanitized evidence: `artifacts/reviews/coding-lifecycle-2026-10-16/live-codex-cancellation.log`.

Focused lifecycle evidence: runtime **46 passed**, command runner **23**, managed processes **9**, and daemon delegation **31** with two opt-in checks ignored in that group. Removing the selected cancellation guard reproduced agent-exit failure and authentication prompt replay after Stop. Removing process ownership reproduced a completed root leaving its child's output pipe open; restored source passed again. Suspended Windows job-assignment refusal, sticky cleanup failure, reader cancellation and an unrelated live process are covered. Both falsification records include exact source restoration. The fresh real-daemon gate passed cancellation, ordinary shutdown refusal for two coding jobs, forced graceful shutdown, restart interruption without replay, fresh approval/identity and retained files. It also passed with an inherited conflicting HTTP setting. Initial strict-lint and fixture-configuration failures are retained; the fixes preserve every acceptance assertion. Evidence: `artifacts/reviews/coding-lifecycle-2026-10-16/`.

Companion website update: [`76e53fa`](https://github.com/props-nothing/jarvis-frontend/commit/76e53faa46561ac967c1f98590754d25315cfc55) is pushed and all three [native frontend jobs](https://github.com/props-nothing/jarvis-frontend/actions/runs/38969701855) passed. The ten scoped files correct release/cancellation/platform claims and current TODO wording. All 38 local tests passed, followed by 14 focused tests and a fresh typecheck/build after the final wording adjustment; installer and documentation checks passed. Source installer files and their default ref are unchanged. Historical measurements remain, including the existing optional face-chunk advisory and separate native-device/clean-machine gates. Local evidence: `artifacts/reviews/coding-lifecycle-2026-10-16/frontend-validation.json`.

First full lifecycle baseline: formatting, strict Clippy, fresh builds and all four process gates passed. The default workspace run then found four daemon failures (849 daemon tests passed; six opt-in checks ignored). Failed ACP setup requested termination but returned before exit, leaving probe scratch directories open; bounded checked shutdown and reported directory cleanup now pass all 11 agent-settings tests and 46 runtime tests. Three GitHub account race tests also exceeded their fixture deadlines while Windows credential ACL operations blocked their test runtime. Test-only timing repair passed all 23 account tests with the same 21-second mutation and six-second disconnect-start delays that reproduced the three original failures. Removing the authority guards still caused the intended resurrection/request-count failures; exact production bytes were restored. Production request timeouts and authority checks remain unchanged. The original failed run is preserved in `workspace-default-initial-failure.log` and `baseline-default-initial-failure.json`, with controlled experiments under `github-race-controls/`, in the lifecycle evidence directory. Full workspace and native lifecycle acceptance still require the final reruns.

First pushed lifecycle revision [`18b8585`](https://github.com/props-nothing/jarvis-core/commit/18b8585375a4e99558259468268ba921dac62697): the repaired local default workspace passed **3,164 tests**, zero failures, six opt-in checks ignored. Its [native CI](https://github.com/props-nothing/jarvis-core/actions/runs/38971773277) passed Linux tests and all four process gates, but macOS found one probe-folder creation failure (853 daemon tests passed, one failed, six ignored). The old diagnostic discarded the I/O error kind, so a collision between concurrent timestamp-only names is plausible rather than proven. The repair uses unique-ID candidates, exclusive creation, bounded collision retries and error-kind diagnostics; tests preserve pre-existing files/directories and exercise concurrent allocation. The first all-feature build was interrupted to validate this repair first; it is not a passing all-feature run. Preserve the original native log and `workspace-default-18b8585-passed.log`; final repaired baseline/native results remain pending.

Repaired verification (2026-10-17, [`99179ce`](https://github.com/props-nothing/jarvis-core/commit/99179ceaa669030f2336f8b3bcd6be5a6bb62889)): all 15 agent-settings tests, formatting, strict workspace Clippy, fresh builds and all four local process gates passed. Both default and all-feature workspace runs passed **3,168 tests** each, zero failures, six opt-in checks ignored. All three native lifecycle gates and nine of ten [hosted jobs](https://github.com/props-nothing/jarvis-core/actions/runs/38973344518) passed. The Windows daemon job failed five shared PowerShell parent/child readiness checks (852 passed, five failed, six ignored). The generic readiness diagnostic did not expose early exit or stderr, so the hosted cause is not proven. Fixture-only module-loading/policy controls and a repair are in progress, with the same readiness deadlines and process assertions. All 11 historical archive hashes and the 26 implementation-file hashes matched their recorded snapshots before this fixture repair. Current and original failure logs remain under `artifacts/reviews/coding-lifecycle-2026-10-16/`.

Windows fixture repair (2026-10-17): the private parent/child scripts now use direct .NET calls with module autoload disabled and process-scoped execution policy, leaving the production environment and owner policy unchanged. Readiness reports early exit and bounded stderr instead of hiding them behind a generic timeout. The original five lifecycle assertions and all deadlines remain. All **24 command tests**, **10 managed-process tests**, strict all-target/all-feature daemon Clippy and workspace formatting passed. Restricted-policy and disabled-autoload controls demonstrate the removed dependencies; they do not prove the hosted timeout's cause. The first new diagnostic assertion failed to account for the existing untrusted-output fence; its log is preserved and the corrected assertion requires actual stderr. Independent review found no remaining blocker. Full repaired workspace/native verification follows; evidence is under `windows-fixture-*` in the lifecycle review directory.

Final code revision (2026-10-17, [`dd65479`](https://github.com/props-nothing/jarvis-core/commit/dd65479d988ebf7caa9bddfb803c13cae1b0ddbf)): all ten [native CI jobs](https://github.com/props-nothing/jarvis-core/actions/runs/38976054653) passed, including all three coding lifecycle gates and the repaired Windows daemon suite. Both final local default and all-feature workspace runs passed **3,170 tests** each, zero failures, six opt-in checks ignored; formatting, strict workspace Clippy, fresh builds and all four local process gates passed. All five [package jobs and publication](https://github.com/props-nothing/jarvis-core/actions/runs/38977383098) passed. The [public unsigned lifecycle release](https://github.com/props-nothing/jarvis-core/releases/tag/ci-dd65479d988ebf7caa9bddfb803c13cae1b0ddbf) has the exact tag target and ten anonymous downloads matching all artifact/archive checksums and sizes; four native archive smokes passed, with Intel macOS runtime smoke skipped. Release evidence is retained in `artifacts/reviews/release-dd65479d988ebf7caa9bddfb803c13cae1b0ddbf/verification.json`. Docker was unavailable locally, so Docker-dependent paths were not exercised there. Earlier failed runs remain preserved; this native result supersedes their pending lifecycle CI gates. Independent scope review confirms all eleven historical archive hashes, the 26 implementation-file hashes and unchanged frontend installers, with no generated logs, credentials or local state committed.

Final documentation checks cover all **376 owned Markdown files** across core and frontend, with zero link/anchor/ADR errors; all six documentation regressions passed. The 214 core JavaScript tests, dependency policy, workflow/archive checks and deterministic HUD build also passed for the unchanged implementation. The final evidence-only documentation commits leave the tested Rust sources, manifests, lockfile, workflows, frontend implementation and installers unchanged.

Final frontend evidence update: [`6889c31`](https://github.com/props-nothing/jarvis-frontend/commit/6889c3107a5fd1eec2793971bbe13a77fa3712d5) changes only README, TODO and content-accuracy documentation to record the verified lifecycle release. All three [native frontend jobs](https://github.com/props-nothing/jarvis-frontend/actions/runs/38978558557) passed. Earlier c48 observations and website measurements remain intact; installers and their default ref remain unchanged.

## Completed acceptance slice: runtime and guide follow-up (2026-10-16)

<a id="current-acceptance-slice-runtime-and-guide-follow-up-2026-10-16"></a>

The owner selected both reproduced runtime defects and all confirmed documentation drift from the follow-up review. Preserve the earlier evidence and unrelated changes; no cache cleanup, obsolete connector deletion, provider expansion or release publication belongs to this slice.

- [x] [ADR-0231](docs/adr/0231-webhook-deliveries-reserve-budget-with-their-deduplication-key.md): reserve webhook delivery identity and minute/day capacity atomically before starting work. Prove concurrent distinct deliveries cannot exceed either limit, duplicates consume no additional slot, and storage failures fail closed. Retain explicit at-most-once crash/failure semantics.
- [x] [ADR-0232](docs/adr/0232-stop-and-restart-check-background-work-and-fail-closed.md): protect ordinary stop/restart from interrupting queued/running coding tasks and applicable managed processes as well as ordinary runs. Refuse unreadable work state, preserve the explicit force action, and cover both shutdown and Settings restart.
- [x] Correct current permission, project-budget, MCP credential/exposure, scheduling, composer/navigation, onboarding, research-status and release/acceptance guidance. Add ADR amendment pointers and replace obsolete prose references; preserve historical observations under their original revision.
- [x] Run narrow regressions, required formatting/strict Clippy/workspace tests, process acceptance gates, documentation checks and relevant frontend/dependency/release checks. Record exact local evidence and retain native/live-provider gates separately.

Commit preparation (2026-10-16): rebased onto `5a92081`, preserving the three intervening upstream commits as separate history. On the integrated Rust sources, default and all-feature workspace runs each passed **3,144 tests**, zero failures, five existing opt-in tests ignored; the seven managed-process tests, formatting and strict all-target/all-feature Clippy also passed. A fresh all-feature build passed all three process gates with both missing-binary guards enabled. The latest release automation passes **214 core Node tests**, workflow lint and archive/license checks; all **374 owned Markdown files** pass documentation checks. The final upstream change touched only release automation/docs: the Rust source, manifests and lockfile match the tested tree. Earlier dependency, HUD and website checks below therefore remain applicable. Native CI and release results for this new runtime commit remain pending. Detailed results: `artifacts/reviews/commit-integration-2026-10-16/validation.json` and adjacent logs.

The verified c48da888 native matrix and public release above supersede the pending native/release statement in that dated integration record.

The first integrated all-feature attempt reported missing storage exports that were present in unchanged source. A forced rebuild with `CARGO_INCREMENTAL=0` compiled and passed; the initial failure log is retained separately. No production change or weakened check was used to obtain the passing result.

Pre-integration local evidence on Windows (2026-10-16, `8bf6cbb` plus the original follow-up working tree): default and all-feature workspace runs each passed **3,144 tests**, zero failures, five existing opt-in tests ignored. Formatting, strict all-target/all-feature Clippy, a fresh all-feature workspace build and all three process acceptance gates passed; the gates enabled both missing-binary guards. Dependency policy, archive/license fixtures, workflow lint, **185 core Node tests**, HUD syntax/typecheck/build and **38 website tests** plus website typecheck/build/installer checks passed. All **374 owned Markdown files** across both repositories pass the link/anchor/ADR checks; all eleven archived history files match their pre-edit hashes. The existing optional website face-chunk size advisory remains. Docker was unavailable, so Docker-dependent runtime cases skipped. This is not new native macOS/Linux, real-provider, PostgreSQL or release-package acceptance.

The regressions are falsified against temporary old behavior: omitting unlinked webhook reservations admitted all 24 callers for one remaining slot; restoring the old native-only/error-to-zero stop guard made queued coding and unreadable state incorrectly return 202. Corrected source was restored and retested. The HTTP webhook concurrency test now requires three admissions and 21 rate-limit refusals. Independent review confirmed the shared managed-process registry and common stop/restart guard. Evidence: `artifacts/reviews/runtime-followup-2026-10-16/validation.json` and its logs, plus the retained shutdown controls in `artifacts/reviews/shutdown-followup-2026-10-16/`.

Narrow regression results: trigger storage **14 passed**, webhook service **17 passed**, stop **6 passed**, managed processes **7 passed**, Settings restart **2 passed**, and CLI lifecycle **2 passed**. The companion [frontend commit `a6b2775`](https://github.com/props-nothing/jarvis-frontend/commit/a6b2775) is pushed to `main` and contains only the four reviewed Markdown corrections; the 38 website tests, build and installer checks above cover that unchanged website implementation. Local logs and generated state stay outside both commits.

The preceding review reproduced 12 accepted requests from 24 concurrent webhook deliveries despite the three-per-minute limit, and a non-forced shutdown accepted with a synthetic running coding-task record. Those observations describe the pre-fix revision `8bf6cbb`, not a real ACP cancellation or owner-account test. The original reproduction is preserved alongside the new validation; atomic delivery-to-run handoff and coordinated shutdown draining remain separate work.

<a id="current-acceptance-slice-review-corrections-2026-10-16"></a>

## Completed review corrections (2026-10-16)

> **CORRECTION (2026-10-16):** the earlier documentation reconciliation passed structural checks but did not eliminate every stale current claim. The follow-up slice above corrects the confirmed semantic drift. The original implementation and validation evidence below remains preserved.

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
- [x] After pushing, verify native Linux/macOS/Windows CI, cold/warm cache behavior and all five release packages on GitHub. Commit `8bf6cbb641fa8ada46bb39d0bfc09ac8cb5a1ee2` passed all ten [CI jobs](https://github.com/props-nothing/jarvis-core/actions/runs/38927374174) and all five [package jobs](https://github.com/props-nothing/jarvis-core/actions/runs/38929525053), with five same-commit artifacts and verified outer/inner checksums. Cold builds and warm native cache restores were observed; no required-status rules needed migration. Four native package smoke tests passed; Intel macOS runtime smoke remains skipped. Evidence: `artifacts/ci-review/verified-8bf6cbb/`. Draft uploads were outside that tag-only run.

### Owner-requested automatic draft attachment (2026-10-16)

- [x] Attach all five verified archives and checksums automatically after successful main/manual packaging, in a draft prerelease tied to the exact tested SHA. Preserve version-tag verification, published-release refusal and every CI/package gate; create no Git tag ref and publish nothing. Run the attachment falsification tests, workflow/documentation checks and required Rust baseline, then verify the first real draft contains all ten assets for its successful CI/package commit. [Contract and limits](docs/research/integrations/github-actions-rust-ci.md#automatic-draft-attachment-2026-10-16).

Completed at `041b25049c31eb8c9facbd4790512ba9d389ea14`: all ten [CI jobs](https://github.com/props-nothing/jarvis-core/actions/runs/38939389627), five [package jobs and draft upload](https://github.com/props-nothing/jarvis-core/actions/runs/38941204133) passed. Draft 415227765 contained exactly ten assets matching the downloaded Actions artifact bytes, with the exact target SHA and no actual Git tag ref. Evidence: `artifacts/ci-review/verified-041b250/`. Four native package smokes passed; Intel macOS remains cross-built without runtime smoke. This completion supersedes the pending-upload notes in the historical evidence below.

### Owner-requested automatic publication (2026-10-16)

- [x] Publish unsigned prereleases automatically after the existing CI/package gates and verification of all ten uploaded assets. Preserve exact tested-SHA targeting, checksum checks, version-tag verification and refusal to overwrite published releases. Publication may create the commit-specific tag; it must never move a tag. Run publication refusal controls and required local checks, then verify a real public prerelease, ten matching assets and its exact tag target. [Contract and limits](docs/research/integrations/github-actions-rust-ci.md#automatic-prerelease-publication-2026-10-16).

Hosted completion: the `c48da888` public release, native matrix, packages, exact tag and all ten public asset downloads are verified in the [current acceptance record](#current-acceptance-slice-release-verification-and-coding-lifecycle-2026-10-16). This supersedes the pending-publication status in the dated local record below. The older `041b250` draft remains unchanged.

Local publication validation (2026-10-16): all 61 release-flow regressions and six documentation regressions passed, along with the 361-file documentation check, archive/license/checksum refusal tests, actionlint 1.7.12, formatting and strict all-target/all-feature Clippy. A first workspace test build hit a Windows linker lock in the primary checkout's shared target directory; no process was stopped or user source changed. A fresh isolated worktree target, with an explicitly built and required MCP peer, passed all 3,129 locked workspace tests (zero failures, five existing ignored). Evidence is retained outside the worktree in `artifacts/ci-review/auto-publish/`. Native hosted CI, all five packages and actual automatic publication remain required.

### Earlier repair evidence

Automatic-draft hosted follow-up: `7f9d161` / CI `38935376522` passed nine jobs but Windows preview startup exceeded 30 seconds again (836 daemon tests passed, one failed, five ignored). Release `38938013450` correctly skipped. Twenty isolated local runs passed; the hosted cause remains unproven. The fixture now waits for the real child's bind acknowledgement before its production port check, within the unchanged deadline, and records startup/bind errors plus an independent diagnostic on timeout. All original HTTP/occupied-port/browser/stop assertions remain. An 11-second delayed start passed; a false ready marker without a listening socket failed with both probes unsuccessful and cleanup complete. No production process behavior or release gate changed. The automatic upload is still pending hosted verification.

Automatic attachment local validation: 43 release-flow tests, six documentation regressions, 361 Markdown files, archive/license refusal checks and actionlint 1.7.12 passed. Required formatting, strict all-target/all-feature Clippy and all 3,129 locked workspace tests passed (zero failures, five existing ignored), with the MCP peer explicitly built and required. Logs are retained in the primary checkout under `artifacts/ci-review/auto-draft-release/`. The real automatic draft upload remains pending; the earlier package run does not satisfy that gate.

Hosted follow-up: `8fcd458` / CI `38920832954` passed all native Clippy jobs, dependency policy, Linux tests/gates and Windows storage (354 unit tests in 89.98s). macOS found 18 credential/OAuth fixture failures due to symlinked temporary ancestors. The repair canonicalizes only fixture roots and extends the unchanged store's symlink/junction refusal tests to the portable root. Final native/package verification remains open. Repository metadata reports no required checks/rulesets needing migration.

The isolated daemon job now explicitly builds its sibling MCP peer executable. With the old executable moved aside, the missing-peer guard failed as expected; the exact new build command then made the same test pass. The repair passed 138 MCP tests, four GitHub settings tests, four storage credential tests, all 3,110 default workspace tests (five existing ignored), formatting, strict Clippy, all 32 release-flow regressions and workflow lint locally. No production credential guard or CI assertion was relaxed.

Second hosted follow-up: `b930f73` / CI `38923320820` passed nine jobs, including macOS and all native acceptance gates. The Windows daemon suite passed 834 tests but its real preview fixture exceeded a ten-second startup wait. Give only that Windows fixture 30 seconds and report the last process state/output on failure; preserve all HTTP, occupied-port, browser-honesty and process-stop assertions. A temporary 11-second child-start control failed before the change and passed afterward; a child that never listens still failed at 30 seconds with its stderr in the diagnostic. The ordinary five process tests pass in 1.19s. Final hosted/package verification remains open.

Preview repair validation: formatting, strict all-target/all-feature workspace Clippy and all 3,110 locked default workspace tests passed, with zero failures and five existing ignored tests. The MCP peer was explicitly built and its missing-binary guard enabled. Logs and temporary controls are retained under `artifacts/ci-review/preview-startup-2026-10-16/` in the primary checkout. No workflow, release check or production behavior changed in this repair.

Pre-follow-up native evidence (checked 2026-10-16): [`8bf6cbb` / CI `38927374174`](https://github.com/props-nothing/jarvis-core/actions/runs/38927374174) completed all ten jobs successfully, including Linux/macOS tests and acceptance lanes, all Windows test partitions, all native Clippy jobs, source checks and dependency policy. This supersedes the earlier unresolved native failures for that revision. The later package/cache verification recorded above also closes those scopes for `8bf6cbb`; neither run verifies the runtime/guide follow-up or automatic draft uploads, which require their own hosted evidence.

## What is built and what still needs acceptance

The current local product has a Rust daemon/CLI, a browser console, Telegram, Google mail/calendar tools, managed MCP clients and an opt-in MCP server, policy approvals, SQLite memory, file knowledge search, schedules/triggers, project budgets, coding delegation and managed host processes. See [README](README.md#built-experimental-next) for the product summary. This section names remaining evidence rather than repeating completed implementation logs.

### Coding delivery and memory discovery

<a id="owner-requested-coding-delivery-2026-10-15"></a>

Coding uses the first Settings-permitted folder **in place** by default; Git/worktrees are opt-in, automatic commits are absent, and no total deadline is imposed unless requested. Durable queued jobs, explicit fresh continuation, complete skill bundles, independent checks and managed previews are built. Ordinary/background Codex runs, a real dependency-install/check smoke, real Windows Codex cancellation and a complete disposable website with explicit continuation are recorded. Fresh sign-in, other-provider/platform acceptance and a scoped agent-to-JARVIS integration bridge remain open. Daemon/native lifecycle acceptance is recorded separately. See [coding guide](docs/user/code-agents.md), [ADR-0222](docs/adr/0222-coding-delivery-uses-owner-workspaces-and-durable-evidence.md), [ADR-0224](docs/adr/0224-coding-waits-and-verification-preserve-real-task-outcomes.md) and [repair evidence](docs/history/todo-owner-evidence-through-2026-10-16.md#owner-requested-delegation-repair-2026-10-16).

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
- [~] `P3-009` Authenticated external MCP access is delivered; per-client credentials/allowlists, audience-bound remote authorization and associated rate-limit acceptance remain. Direct calls still need a durable approval/tool-call/outcome ledger; their current pending approvals are in memory, while delegated JARVIS runs retain normal durable history. [Original evidence](docs/history/todo-p3-evidence-through-2026-10-16.md#p3-009).
- [~] `P3-012` Approval restart and duplicate-delivery gates exist. Keep current acceptance tests as the gate; external-client authorization and policy-at-resume semantics must be reconciled against later ADRs before closing the remaining umbrella. [Original evidence](docs/history/todo-p3-evidence-through-2026-10-16.md#p3-012).
- [ ] `P3-021` Additional remote/disposable sandbox adapter only for a named requirement; prove declared resource guarantees, crash isolation, output bounds and cleanup. Existing Docker execution does not sandbox ACP host processes. [Original evidence](docs/history/todo-p3-evidence-through-2026-10-16.md#p3-021).
- [~] `P4-014` Conversational memory proposal/admission is delivered (including later owner Keep). Close residual acceptance across native platforms and current correction/forget behavior; do not reimplement the former missing model surface. [Original evidence](docs/history/todo-p4-evidence-through-2026-10-16.md#p4-014).

### Connectors and automation (P5–P6)

- [ ] `P5-005b` Retire the unused Google push/watch framework in a separate, dependency-verified deletion slice only after owner confirmation required by ADR-0167 §6. Keep contracts, OAuth/readiness and everything production calls. Polling already serves mail triggers. No deletion is part of this review correction.
- [~] `P5-009` Google write/draft tools are delivered. For each new MCP/provider write, prove its policy/idempotency/outcome boundary and real approved/denied behavior rather than adding a bespoke connector by default.
- [ ] `P5-010` Complete provider-seam tests and selected live checks for expiry, revoked scope, pagination, throttling, reauthorization and redacted diagnostics.
- [~] `P6-007` Quiet hours and notifications exist. Add per-day proactive-message budgets, deduplication, per-kind/project controls and an owner-facing disable action when required.
- [~] `P6-010` Native structured parent/child identity, explicit restart behavior and linked cancellation are delivered in `P7-009` / ADR-0234. Shared native/coding policy is delivered in `P7-010` / ADR-0235; broader delegation coordination/acceptance is delivered in `P7-011`.
- [~] `P6-012` Mail, file and webhook triggers exist; preserve their deduplication and ordinary-run policy. Atomic webhook budget reservations fix concurrent admission, not the claim-to-run crash-loss window. Reliable restart delivery remains a separate demand-gated handoff/outbox slice under the Phase 6 target; external-service acceptance remains `P11-017`.

See [P5–P8 historical evidence](docs/history/todo-p5-p8-evidence-through-2026-10-16.md). Time-zone schedules and model-selected approved skills are delivered; the multi-step workflow engine remains parked.

### Coding agents and delegation (P7)

- [~] `P7-001` ACP v1 client, handshake and bounded task activity are built, with recorded real Codex tasks. Complete the current activity/event and real-agent acceptance rather than implementing a private JARVIS runtime protocol.
- [~] `P7-002` Sanitized environment, operation/output bounds and owned descendant cleanup exist. Scripted lifecycle gates pass on Windows, macOS and Linux; real Windows Codex cancellation is recorded. Remaining supervisor work includes abrupt-crash/orphan recovery, stderr redaction review, backoff and real-agent evidence on other native platforms. No default total deadline or OS confinement is claimed.
- [ ] `P7-003` Task approval and cooperative ACP permissions exist. A scoped JARVIS tool bridge, interactive policy re-entry where needed and an enforceable optional sandbox remain; a worktree is not a security boundary.
- [~] `P7-004` In-place durable coding, optional worktrees, complete skills, independent checks and managed previews are built (ADR-0222/0224). Scripted native lifecycle, real Windows Codex cancellation, and a complete Windows Codex website with desktop/mobile-emulated behavior, explicit continuation and preview cleanup are verified. Fresh sign-in, other-provider/platform acceptance, notification reception and physical-device checks remain.
- [~] `P7-008` Adversarial Stop/timeout, agent error/exit, authentication retry, late permissions, owned descendant cleanup and setup-refusal tests are implemented, with cancellation and ownership falsification. Complete capability-skew, abrupt-crash and broader audit coverage for the supported ACP surface; native tools are not all intercepted.
- [x] `P7-009` Replaced native objective-text identity with immutable parent/child records, conservative legacy migration, restart handling and parent Stop. Run/agent replies and CLI listings expose lineage; the completed acceptance slice above and [ADR-0234](docs/adr/0234-native-subagents-have-durable-lineage-and-parent-stop.md) record evidence and limits.
- [x] `P7-010` Shared delegation effects and native/host execution contracts, per-tool trust and risk floors, and stored-child refusal on direct/approval-resumed launches are implemented. The completed acceptance slice above and [ADR-0235](docs/adr/0235-delegation-declares-work-and-its-execution-authority.md) record verification and upgrade behavior.
- [x] `P7-011` Complete broader depth/concurrency, duplicate-delivery, isolation, audit and cancellation coordination acceptance. ADR-0228 covers atomic native capacity; ADR-0234 adds stored one-level lineage, parent-admission falsification and native parent Stop/restart checks. ADR-0236 reserves admission before the stop/restart snapshot and closes it on acceptance. ADR-0237 adds cross-runtime family Stop and at-most-one dispatch per durable call. ADR-0238 adds child-audit/reopen, normalized native failure and scripted ACP crash acceptance plus bounded shutdown draining. All ten native CI jobs passed for `962cf079` in [run 39069495553](https://github.com/props-nothing/jarvis-core/actions/runs/39069495553), including Windows/macOS/Linux tests and process gates. In-process memory/panic-abort isolation and exactly-once external effects are not claimed.

The disposable Windows Codex website gate above is verified through actual approval, independent checks, a managed loopback preview, desktop/mobile-emulated interactions and explicit cleanup. Fresh sign-in, notification reception and other-provider/platform acceptance remain separate. Native parent/child identity, restart and parent Stop are implemented in the acceptance slice above. Native/coding delegation policy alignment is complete in `P7-010`; ADR-0237 adds cross-runtime family cancellation and dispatch identity. ADR-0238 adds the audit/failure and coordinated-drain slice; its native CI acceptance now completes `P7-011` within the documented isolation limits.

### Voice and channels (P8)

- [ ] `P8-010` Local English/Dutch recognition and synthesis, VAD/echo handling and a browser-independent client. Research measured candidates first; Telegram voice notes can be an initial surface. A tray client is conditional on need.
- [ ] `P8-001` Extend provider-neutral voice/session/identity/retention contracts when local or broader call paths require them.
- [~] `P8-007` Owner outbound calls through ElevenLabs have owner-reported live evidence. Broader provider-neutral calls, additional call paths and real-account failure cases remain; this is not general telephony acceptance.
- [ ] `P8-008` Add raw-body signature verification, deduplication and bounded asynchronous webhook handling when inbound calls require it. Current outbound status polling needs no provider webhook.
- [~] `P8-009` Outbound masked call records, 90-day cleanup and deletion exist; additional consent/disclosure, inbound records and correlated approval-audit evidence remain.
- [ ] `P8-011` Verify inbound/outbound, no-answer/voicemail, interruption, latency, replay, privacy and provider outages for each supported call path.

#### Voice backup and local voices (owner-requested 2026-10-17)

The owner asked for both a cloud provider backup and native local voice generation with editable, customisable voices, in the style of ElevenLabs. Plan and sources: [voice backup and local voices](docs/research/voice-backup-and-local-voices-2026-10-17.md). Each external provider or model still needs its dated integration record first (`docs/development/external-research.md`). Voice providers stay replaceable adapters, and no provider type crosses the `jarvis-voice` boundary.

- [ ] `P8-018` **Provider-neutral speech interface.** A `SpeechProvider` trait in `jarvis-voice` with a neutral audio stream and real content type; `ElevenLabsSpeech` moves behind it with no behaviour change, and `GatewayState` holds `Arc<dyn SpeechProvider>`. Acceptance: existing speech, gateway and setup tests pass unchanged; the key stays redacted and daemon-only (ADR-0138).
- [ ] `P8-019` **Fallback chain.** Ordered providers tried before the first audio chunk, so a `200` still means audio is coming; a per-provider circuit breaker (short skip for outages, longer for rejected keys or exhausted credit) with a single probe to recover; one voice per answer unless that provider fails; browser speech stays the last resort. `GET /api/v1/speech` lists providers, health and the active one; the console shows a "backup voice" note; `jarvis doctor` probes each provider. Acceptance: loopback-fake tests for outage, 401/402/429/5xx, mid-stream failure, recovery after the breaker and no key in any response or log.
- [ ] `P8-020` **OpenAI-compatible speech adapter.** `POST /v1/audio/speech` (MP3, so console streaming is unchanged) with an optional key; plain `http` only for loopback, `https` otherwise; no redirects, timeout and size cap. One adapter covers OpenAI's hosted TTS and local servers (Kokoro-FastAPI, VoiceStudio, Speaches, LocalAI). Acceptance: research record, fixture tests, and live speech from OpenAI and one local server on the owner's machine.
- [ ] `P8-021` **Second native cloud provider.** Research Cartesia, Azure Neural, Google Cloud TTS and Deepgram Aura (latency, streaming, price, voice quality, data retention); the owner picks one and it gets its own adapter. Acceptance: research record, adapter tests, and live failover from ElevenLabs to it with ElevenLabs blocked.
- [ ] `P8-022` **Native local synthesis in the daemon.** The synthesis half of `P8-010`: an embedded engine behind a `local-tts` cargo feature (`sherpa-onnx` with Kokoro-82M as the first candidate, Apache-2.0 code and weights), with models downloaded on first use and pinned by hash, never bundled, and a WAV/PCM playback path in the console next to Media Source. Acceptance: speech with the network disconnected on Windows, macOS and Linux, with first-audio latency and CPU/RAM measured per platform.
- [ ] `P8-023` **Voice library with editable settings.** Voices stored in the profile with name, provider, model, voice id and per-voice settings (speed, pitch, and stability/style/expressiveness where the engine supports them), plus preview, default voice and a voice per channel (console, Telegram, calls). Settings API and console editor. Acceptance: edits survive restart, preview plays the edited voice, and every setting the engine cannot honour is hidden or labelled rather than silently ignored.
- [ ] `P8-024` **Voice design.** Create a new voice without a recording: blend local preset voices by weight (Kokoro voice mixing) and, if an instruct-capable local engine is selected, from a text description; save it to the library. Acceptance: a designed voice is reproducible from its saved recipe, exportable and deletable.
- [ ] `P8-025` **Voice cloning from the owner's sample, with consent.** A local cloning engine chosen after research (candidates: Chatterbox, CosyVoice 3, VoxCPM2; record code and weight licenses separately and reject non-commercial weights as a default). The owner records or uploads a sample, a consent record is kept with the voice, and samples stay private, profile-local and deletable along with the voice. Cloning a voice that is not the owner's needs explicit attested consent. Acceptance: clone, use, and delete with the sample and derived files gone; GPU-free behaviour documented (slower or unavailable, never a silent cloud upload).
- [ ] `P8-026` **Pronunciation dictionary.** Per-profile and per-voice word replacements (names, acronyms, Dutch and English terms) applied before synthesis for every provider. Acceptance: a listed word is spoken as configured on each provider.
- [ ] `P8-027` **Phone-call voice fallback.** Calls run through ElevenLabs Agents (`P8-007`), so this needs a provider-neutral call path (Twilio media streams with the `P8-019` chain and local or cloud STT) rather than a TTS swap. Acceptance follows `P8-011`, including a provider outage.

Order: `P8-018`–`P8-020` first (backup voice with no new account); `P8-022` next, which meets ADR-0214's revisit condition for `P12-002`; then `P8-023`–`P8-026`, and `P8-021`/`P8-027` when the owner picks a provider.

### Product, installation and distribution (P9)

- [~] `P9-002` Browser chat, settings, activity, approvals, memory and tool navigation exist. Continue user-flow/accessibility verification, canonical transcript browsing across browsers/devices and the remaining provider-specific setup work. A Tauri rewrite is not required.
- [~] `P9-003` User service install/uninstall exists. Resolve documented service/manual-daemon ownership cases and verify native lifecycle; system-wide modes remain optional.
- [~] `P9-004` Opt-in prebuilt first install has native OS/architecture detection, explicit release/SHA-256, bounded extraction and existing PATH/setup/doctor handoff (ADR-0239). Signed manifests and hosted publisher/public-install acceptance pass in the opt-in path (ADR-0240); default distribution and clean-machine evidence remain. Website source bootstrap still requires a toolchain.
- [ ] `P9-005` Signed native artifacts, complete dependency notices/SBOM, provenance, checksums and release notes.
- [~] `P9-006` Offline database snapshots, guarded same-schema restore/undo and interrupted-restore reconciliation are implemented (ADR-0241; acceptance above). The initial same-schema binary staging/activation/rollback and recovery guards are implemented under ADR-0242; published/native acceptance, schema-change rollback, daemon readiness, channels and broader version/split-brain diagnosis remain.
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

### Competitive catch-up (P12)

Owner-requested on 2026-10-17 after the [competitive gap review](docs/research/competitive-gap-2026-10-17.md) against Hermes Agent and OpenClaw. This is the owner's named requirement for the demand-gated items it references; it orders existing IDs rather than duplicating them. Every item keeps the authority boundary: no new path lets a model, page, skill or channel grant itself permission. External APIs/providers still need an official-source research record first.

- [~] `P12-001` **Install without a toolchain.** Opt-in prebuilt installation (ADR-0239) now verifies signed release manifests (ADR-0240); native signing credentials are not configured. Still deliver the rest of `P9-004`–`P9-007`: signed packages, default install, onboarding acceptance, update and rollback. Acceptance: a non-developer installs, configures, updates and uninstalls on a clean Windows, macOS and Linux machine with no Rust, Node or Python (`P9-007`, `P9-022`).
- [ ] `P12-002` **Owner decision: voice at first run.** ADR-0214 records the owner's choice to require ElevenLabs for guided setup; both rivals start without a voice key. Ask the owner whether to keep it. If changed, amend ADR-0214 and prove setup completes with voice skipped and voice can be added later from Settings. ADR-0214 says to revisit once a verified local voice adapter exists; `P8-018`–`P8-022` deliver that, so setup can then offer ElevenLabs, a local voice or skip.
- [ ] `P12-003` **Skills chosen by meaning.** Replace or complement the 75% word-overlap match with retrieval that finds an approved skill phrased differently from the request. Acceptance: a measured owner question set where paraphrased requests select the right skill without raising false selections; skills still never grant authority (ADR-0117).
- [ ] `P12-004` **Skills that improve after use.** After a run that used an approved skill and needed corrections, JARVIS proposes a revised version (a diff the owner approves or dismisses). Acceptance: revision is proposal-only, history is kept, and a dismissed revision never applies.
- [ ] `P12-005` **Always-loaded owner profile.** A small owner-approved profile (preferences, names, working style) included in every run within a fixed token budget, edited through the existing memory admission/correction path (`P4-014`). Acceptance: survives restart, is inspectable, correctable and forgettable, and its token cost is reported.
- [ ] `P12-006` **Semantic memory and file recall.** Close `P11-026` and `P11-014` measurement, then add semantic retrieval and automatic file/entity context where word-only recall measurably misses. Acceptance: owner-corpus answer quality, latency and token figures before and after.
- [ ] `P12-007` **More channels.** Under `P11-019`, add Discord, Slack and WhatsApp first, then Signal/iMessage if feasible, each as an authenticated client. Acceptance per platform: paired identity, one-action approval (ADR-0136), formatted answers, images and failure paths on a real account.
- [ ] `P12-008` **Enforceable execution sandbox.** Deliver `P7-003`'s optional sandbox and `P11-013`'s confined runtime: run coding agents and long-running processes in a container or remote backend (Docker first, SSH next) with declared limits. Acceptance: escape and resource-limit falsification tests per backend (`P3-021`); host execution remains available only as an explicit, labelled choice.
- [ ] `P12-009` **Native model adapters.** Add a native Anthropic adapter (and Gemini if needed): the [provider research](docs/research/integrations/model-providers.md) records that Anthropic's OpenAI-compatible layer is not production-ready and lacks prompt caching, strict tool schemas and thinking. This is the documented unmet requirement `P5-006`–`P5-008` ask for. Acceptance: research record, scripted tests, a live tool-use run with caching measured, and no provider types crossing domain boundaries.
- [ ] `P12-010` **Media output.** Image generation, PowerPoint and charts (`P11-010`, `P11-011`). Acceptance: files open in their real applications and images show inline in the console and Telegram.
- [ ] `P12-011` **Reliable proactive delivery.** Close the claim-to-run crash-loss window for schedules and triggers (`P6-012`, Phase 6 gate) and add notification budgets (`P6-007`). Acceptance: a trigger survives daemon restart without duplicate effects and replay starts nothing.
- [ ] `P12-012` **Skill ecosystem import.** Discover and install skills from the agentskills.io format and public registries (`P11-021`), with static scanning, pinned versions, owner approval per install and safe updates. Acceptance: a malicious-skill fixture is refused and an update cannot change a skill without a new approval.
- [ ] `P12-013` **Finish live acceptance.** Close the open owner-account checks listed under "Provider and user-flow verification" so built features count as accepted. Acceptance: each listed check has recorded live evidence.

Order: `P12-001` and `P12-002` remove the largest adoption barrier; `P12-003`–`P12-006` close the learning and memory gap; `P12-007`–`P12-009` add reach and isolation; the rest follow. The owner may reorder.

## Deliberately deferred

- `P6-001`, `P6-002`, `P6-004`–`P6-006`, `P6-008`: multi-step workflow engine/outbox/compensation until ordinary runs and schedules cannot satisfy a named requirement.
- `P5-006`–`P5-008`, `P7-005`, `P7-007`: provider/framework-specific adapters are superseded by MCP/ACP-first integration. Native adapters need a documented unmet requirement.
- `P8-002`–`P8-006`, `P8-012`–`P8-017`: vendor-shaped voice transports/realtime room until used; provider-neutral policy still applies.
- `P9-001`, `P9-019`, `P11-009`: desktop shell/native desktop automation until a concrete local-voice/tray/task requirement.
- `P10-001` server profile/TLS/runbooks; `P10-002` PostgreSQL parity; `P10-003` device enrollment; `P10-004` secure remote access; `P10-005` roles; `P10-006` service objectives; `P10-007` extra infrastructure: all remain demand-gated. The local daemon/tunnel is not the complete multi-device profile.

## Evidence and maintenance

Completed detail and original wording remain in the [history index](docs/history/README.md); research records retain source URLs, versions and access dates. An archive's unchecked box or failed historical run is not automatically today's next task or current test status. Update the active summary and relevant guide when behavior changes, add an ADR amendment link when superseding a decision, and run the documentation link/ADR checks with the normal required gates.
