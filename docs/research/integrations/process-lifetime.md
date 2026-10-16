---
integration: process-lifetime
status: implementation-in-progress
last_verified: 2026-10-16
owners: [jarvis]
selected_spec_version: Windows Job Objects; POSIX process groups
selected_sdk: process-wrap 10.0.0; Tokio 1.53.1
---

# Owned host-process lifetimes

## Scope

Keep bounded commands, managed previews and ACP agents attached to an owned process lifetime, including descendants left behind when their root exits. Close reader tasks when their caller ends. This is lifecycle management of explicitly authorized host execution, not filesystem, network or resource confinement.

## Official Sources

| Source | URL/version | Accessed | Purpose |
| --- | --- | --- | --- |
| Repository indices | `https://github.com/watchexec/process-wrap/blob/main/llms.txt`, `https://raw.githubusercontent.com/watchexec/process-wrap/main/llms-full.txt`: not found; official repository/source used | 2026-10-16 | discovery |
| Windows contract | https://learn.microsoft.com/en-us/windows/win32/procthread/job-objects | 2026-10-16 | child inheritance, kill-on-close, nested jobs and limits |
| Selected package | https://raw.githubusercontent.com/watchexec/process-wrap/v10.0.0/Cargo.toml | 2026-10-16 | version, features, license and dependencies |
| Windows wrapper | https://raw.githubusercontent.com/watchexec/process-wrap/v10.0.0/src/tokio/job_object.rs | 2026-10-16 | suspended creation, handle assignment, failure and wait behavior |
| Windows implementation | https://raw.githubusercontent.com/watchexec/process-wrap/v10.0.0/src/windows.rs | 2026-10-16 | kill-on-close, resume sequencing, owned handles |
| Unix wrapper | https://raw.githubusercontent.com/watchexec/process-wrap/v10.0.0/src/tokio/process_group.rs | 2026-10-16 | process group creation and group signalling |
| Release history | https://raw.githubusercontent.com/watchexec/process-wrap/v10.0.1/CHANGELOG.md | 2026-10-16 | 10.0.1 changes concern signal-mask reset; that wrapper is not selected |
| Error mapping | https://docs.rs/nix/0.31.3/nix/errno/enum.Errno.html | 2026-10-16 | `ESRCH` means no process/group remains |
| Windows PowerShell 5.1 CLI | https://learn.microsoft.com/en-us/powershell/module/microsoft.powershell.core/about/about_powershell_exe?view=powershell-5.1 | 2026-10-16 UTC | process-only `-ExecutionPolicy`, `-File` arguments |
| Windows PowerShell 5.1 preferences | https://learn.microsoft.com/en-us/powershell/module/microsoft.powershell.core/about/about_preference_variables?view=powershell-5.1#psmoduleautoloadingpreference | 2026-10-16 UTC | disabling automatic module discovery in the private fixture |
| Exact hosted image PowerShell setup | https://raw.githubusercontent.com/actions/runner-images/win25-vs2026/20261007.264/images/windows/scripts/build/Configure-PowerShell.ps1 | 2026-10-16 UTC | image prewarms a module cache to reduce first invocation by approximately 20 seconds |
| Exact hosted image machine policy | https://raw.githubusercontent.com/actions/runner-images/win25-vs2026/20261007.264/images/windows/scripts/build/Configure-BaseImage.ps1 | 2026-10-16 UTC | image configures LocalMachine execution policy as Unrestricted |

The POSIX Issue 8 kill page returned HTTP 403 and versioned Tokio rustdoc was unavailable through the web reader. The already-installed, checksum-locked Tokio 1.53.1 source confirms that dropping a `JoinHandle` detaches its task; `abort()` cancels it. No remembered OS API is implemented directly.

The Microsoft Learn root and PowerShell-section `llms.txt` requests returned reader errors; the versioned official PowerShell pages above were read directly.

## Verified Contract

### Operations And Transport

The selected safe wrapper creates Windows children suspended, assigns their open handle to a job, and resumes after successful assignment. With `KillOnDrop`, job closure kills associated children. Setup failure terminates the suspended child. `CreationFlags` preserves `CREATE_NO_WINDOW`. Neither breakaway flag is requested. On Unix the wrapper creates a process group and signals that group. Its normal `wait()` can wait for descendants; JARVIS instead waits on the native root and then terminates remaining members before reporting completion.

### Authentication And Authorization

No credentials or new authorization are involved. Existing tool policy and approved ACP/host-command effects still decide whether to spawn. The helper receives a fully prepared command and does not expand its environment, directory grants or arguments.

### Limits And Failure Semantics

Failure to establish ownership refuses launch. Normal root exit, explicit stop, timeout and future destruction request termination of the owned tree. Explicit waits confirm root exit; successful termination requests are not a separate proof that every descendant has already exited. Cleanup refusal stays failed across repeated waits. No saved PID is used for restart cleanup. Unix cleanup on ordinary destruction remains best effort; abrupt daemon death does not execute destructors. A Unix child deliberately leaving its process group is outside this lifetime boundary. Windows job inheritance has documented exceptions, including WMI-created children; this is not an OS sandbox claim.

An ACP setup error awaits bounded checked shutdown before returning. Settings probes also close an established connection after explicit authentication failure, then remove only their newly created scratch directory with bounded retry. Cleanup errors remain visible; a destructor's termination request alone does not prove the directory can already be removed.

Probe folders use the existing JARVIS unique-ID source and exclusive creation. A bounded retry handles name collisions without adopting or removing an existing path. Other creation failures return immediately with the I/O error kind; a wall-clock timestamp is not treated as a uniqueness guarantee.

### Data And Compliance

The helper performs local process operations only. Output remains in the existing bounded captures; no new persistence, telemetry, network request or data retention is introduced.

### Versions And Deprecations

Promote the existing lockfile packages `process-wrap =10.0.0`, `windows =0.62.2` and `nix =0.31.3` to explicit dependencies where needed. `process-wrap` is Apache-2.0 OR MIT and supports Rust 1.87; workspace Rust is 1.98.1. Enable only Tokio, kill-on-drop, process-group, job-object and creation-flags. No new package or workspace `unsafe_code` exception is required. Keep the versions pinned pending a separately verified upgrade.

## JARVIS Mapping And Decisions

An infrastructure `OwnedProcess` retains the native child and its group/job wrapper. It exposes streams, root status and tree termination without exporting third-party wrapper types into domain crates. It consumes its termination authority once, immediately at root exit or stop, rather than signalling a historical numeric PID later. `AbortOnDrop` owns reader tasks through normal completion, timeout and caller cancellation. Durable coding-task states and managed-process history remain JARVIS-owned.

## Rejected Alternatives

- Retrying `taskkill /T` after the root exits cannot recover the lost ancestry.
- Enumerating and later killing saved descendants introduces PID-reuse races.
- Assigning a running child to a job leaves a spawn-before-assignment window.
- Relaxing workspace `unsafe_code = forbid` is unnecessary with the reviewed dependency.
- Waiting indefinitely for all descendants changes bounded commands into unmanaged servers.

## Verification Plan

Use a real local root process that starts a child, records both PIDs and waits for a release file. Independently establish both are alive, release the root, and require the child to exit before the command/preview is considered finished. Cover timeout, caller cancellation and ordinary exit; include an unrelated child control. On Windows hide the spawned process handle from the job wrapper to force setup failure while the child is suspended, then require that its marker never appears and its PID exits. A deliberately removed ownership wrapper must fail the descendant regression. A reader whose pipe stays open must be aborted on timeout and caller drop. Existing ACP initialization-cancellation tests and a real pinned-agent acceptance test remain separate evidence.

## Unresolved Questions

- Native Windows, Linux and macOS runs are required before claiming all-platform acceptance.
- Arbitrary Unix daemon crashes require a stronger supervisor/cgroup design; this slice does not claim that guarantee.
- Actual Codex/Claude jobs may create nested jobs; the live-agent test must check compatibility, not infer it from the fixture.

The recorded Windows Codex 2.2.2 cancellation check below now supplies that limited compatibility evidence. Claude and other native platforms still need their own real-provider checks.
- The selected wrapper's Windows `wait_on_job` returns completion on a received packet without distinguishing `ACTIVE_PROCESS_ZERO`; its asynchronous wait can also start a blocking completion-port waiter. JARVIS does not use that method as proof of an empty job or wrap an unbounded blocking waiter in a superficial async timeout. Native tests independently check the known fixture descendants.

## Verification Log

### Windows fixture startup, 2026-10-17 local / 2026-10-16 UTC

Hosted Windows daemon tests at `99179ce` failed five shared fixture readiness checks while the native process gates and macOS/Linux daemon jobs passed. The original readiness panic discarded early command outcomes. The scripts invoked PowerShell cmdlets, while JARVIS intentionally excludes the runner's `PSModuleAnalysisCachePath` and `PSModulePath` from its inherited command environment. The exact image documents a substantial first-invocation cache warmup; cold module discovery is a plausible explanation, not an established diagnosis from the incomplete hosted output. The same image configures Unrestricted policy, so local Restricted-policy failures do not prove the hosted cause.

The test fixture now uses .NET methods, disables module autoload in its own scripts, and applies `-ExecutionPolicy Bypass` only to those generated root/child processes. Microsoft's CLI contract says this option changes the process environment, not registry policy; enterprise Group Policy may still override it. No authentication, production environment allowlist, owner policy, process-ownership mechanism or dependency changed. Early child failures are captured, and Rust readiness monitors surface bounded command results or managed-process log tails. The 20-second readiness deadline and independent live-root, live-descendant, unrelated-process and exit assertions remain intact.

Controlled local Windows PowerShell 5.1 runs are retained under `artifacts/reviews/coding-lifecycle-2026-10-16/windows-fixture-policy/`: `reproduce.ps1`, the exact original/proposed scripts, and `results.json`. Restricting only the launched root yielded exit 1 and a policy error; restricting only the child prevented its readiness marker; explicit process Bypass restored readiness. With automatic module loading disabled, the original root failed on `New-Object`, while the .NET fixture reached readiness in 488 ms. These controls establish the dependencies removed by the repair. They neither emulate the hosted cache delay nor replace a fresh native CI run. Owned control processes were terminated through their live process handles; machine/user policy and environment were untouched.

| Date | Versions checked | Relevant change or no-change evidence | Researcher |
| --- | --- | --- | --- |
| 2026-10-16 | process-wrap 10.0.0/10.0.1, Tokio 1.53.1, Windows Job Objects | Reviewed live official sources and locked implementation; implementation/tests pending. Existing sandbox records concern resource confinement and are not superseded by lifetime ownership. | JARVIS agent |
| 2026-10-16 | selected dependencies above, native Windows children | Focused checks passed: runtime 46; command runner 23; managed processes 9; delegation 31 with 2 explicit provider checks ignored. These include live parent/descendant observation, root exit, timeout, caller cancellation, an unrelated-process control, suspended job-assignment refusal, sticky termination failure and reader cancellation. Full workspace, native daemon lifecycle, other platforms and live provider acceptance remain pending. | JARVIS agent |
| 2026-10-16 | process ownership falsification | Temporarily removed job/group ownership while retaining root kill-on-drop. The root-exit regression failed as expected: root exit 0 and completed status left its child's output pipe open, so capture was incomplete after 6.6 seconds. Private-fixture cleanup ran; exact source bytes/hash were restored and the regression passed again. Evidence: `artifacts/reviews/coding-lifecycle-2026-10-16/process-falsification.json`, `falsification-process-ownership.log`, `process-ownership-restored.log` (local ignored test artifacts). | JARVIS agent |
| 2026-10-16 | codex-acp 2.2.2, Windows | Explicitly enabled live test passed in 16.02 seconds: real Codex launched the supplied Node parent/child, both were observed alive before Stop and gone afterward, supplied files remained unchanged and durable cancellation survived database reopen. Existing authentication was used; no owner profile changes or new credentials were needed. Other providers/platforms and full workspace results remain separately tracked. | JARVIS agent |
| 2026-10-16 | failed ACP setup and Settings probes, Windows | The first full workspace run reproduced two scratch folders left behind after failed setup. Checked shutdown plus explicit authentication-error close repairs the race; all 11 agent-settings tests and 46 runtime tests pass. The scratch regression checks seven success/setup/authentication/deadline paths and requires an empty directory immediately after each returned result. Original failing evidence is retained as `probe-cleanup-before.log`; the full workspace rerun is tracked in TODO. | JARVIS agent |
| 2026-10-17 | probe folder allocation | Native macOS CI at `18b8585` found one creation failure; its original diagnostic erased the I/O error kind, so collision is an inference. Deterministic tests now force occupied file/directory names and simultaneous allocation while retaining exclusive ownership and existing sentinel bytes. Restoring single-attempt behavior fails on `AlreadyExists`; unique-ID allocation with bounded collision retry passes the 15-test agent-settings group. Final workspace/native reruns are tracked in TODO. | JARVIS agent |
| 2026-10-17 | Windows PowerShell 5.1 process fixture | Module-autoload and process-policy controls above discriminate the old and new scripts. After the fixture repair, 24 command tests and 10 managed-process tests pass, including all five descendant cases and two bounded early-error diagnostics; strict daemon Clippy and workspace formatting pass. The first managed diagnostic expectation omitted the existing untrusted-data fence; its preserved failed log led to an exact fenced-stderr expectation, not a weakened assertion. Logs and command metadata are recorded in `windows-fixture-verification.json` under the local review artifacts. Full workspace and fresh native CI remain separate. | JARVIS agent |
