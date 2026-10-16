# Installation And Releases

**A native program today. Signed, verified distribution is the release goal.**

[Documentation](../README.md) / [Quick start](../user/quick-start.md) / [Platform evidence](../user/platforms.md) / [Roadmap](../../ROADMAP.md)

## Available Today

| Path | Status |
| --- | --- |
| Build the CLI from source | `cargo build --release -p jarvis-cli --locked`; the same `jarvis` program runs its daemon. |
| Install for your user | `jarvis install` copies the program and sets up PATH; no administrator rights required. |
| Start at login | `jarvis service install` / `uninstall`; native platform details are in the [platform guide](../user/platforms.md). |
| Portable profile | `--root` keeps managed state in an explicit directory and does not install a service. |
| CI archives | Platform workflow artifacts and automatic draft prereleases after all five packages pass; inspect the [release workflow](https://github.com/props-nothing/jarvis-core/actions/workflows/release.yml) for the revision's result. |
| Public signed binaries and updates | Not shipped; Apache-2.0 is selected, but signing, clean-machine evidence and update/rollback work remain. |

The separate product website serves working source-building scripts at `https://www.jarvisd.dev/install.sh` and `https://www.jarvisd.dev/install.ps1`, from `frontend/public/`. These clone/update a protected source checkout, build with the committed lockfile, then delegate per-user installation to `jarvis install`. They are not signed-binary downloaders. Source builds require Git, Rust via rustup and [native build tools](../development/getting-started.md#build-from-source); the installed executable needs no Rust. Build/install failure exits nonzero, without setup or a success claim. A successful installer prints interactive and noninteractive setup commands; it does not collect keys or run onboarding through piped stdin. New setup requires an ElevenLabs key and explicit folder grants before starting the assistant. Do not infer signed distribution or clean-machine readiness from this source path.

## Current CI Archive Flow

The [CI workflow](../../.github/workflows/ci.yml) runs native quality checks on Windows, macOS and Linux plus dependency policy. After a successful **push** run on this repository's `main`, the [release workflow](../../.github/workflows/release.yml) packages that run's exact commit, not whichever commit happens to be newest when packaging starts. Failed/cancelled CI, pull requests and foreign repositories cannot trigger packaging. Documentation-only main pushes also follow this flow.

Manual release runs and `v*` tag pushes call the same CI workflow before packaging. Five platform archives and their SHA-256 files are retained as workflow artifacts for 14 days. These artifacts are downloadable by signed-in users of this public repository; “private artifacts” in the workflow name does **not** make them private.

CI separates source/HUD checks, native Clippy, native tests and dependency policy. Windows tests use three jobs (storage, daemon, remaining workspace); every package still runs on every native OS. Each OS builds current acceptance binaries once before all three phase gates. Windows Rust and packaging jobs allow 60 minutes. Pinned Rust dependency caches are separated by check/partition or release target and never substitute for a locked build or quality gate. Failed runs retain diagnostics and Cargo build timings. Ordinary main pushes and PRs run CI directly; tag validation runs through Release's reusable CI call.

The 2026-10-16 repair also removes the Unix Clippy failure and yanked ZIP dependencies without weakening dependency policy. Ordinary test fixtures reuse a closed migration image while keeping independent files and production durability checks. Commit `8bf6cbb641fa8ada46bb39d0bfc09ac8cb5a1ee2` passed all ten [native CI jobs](https://github.com/props-nothing/jarvis-core/actions/runs/38927374174) and all five [package jobs](https://github.com/props-nothing/jarvis-core/actions/runs/38929525053). All five downloaded artifacts and their archive checksums were verified. Four native archive smoke tests passed; Intel macOS is cross-built with its runtime smoke skipped. Warm native cache restores were observed. [Research and measurements](../research/integrations/github-actions-rust-ci.md) retain the earlier failures and local controls separately. That run predates automatic draft attachment and does not prove upload behavior.

After all five packages succeed, every eligible run attaches the complete set of five archives and five SHA-256 files to a **draft prerelease**. Checksums and the expected platform names are verified again before attachment. Main and manual branch builds use the pending tag name `ci-<full tested commit SHA>` and target that exact commit. Each commit has its own draft; a newer build cannot overwrite another commit's assets. Version-tag builds retain their existing `v*` tag and require it to exist.

Open the repository's [Releases page](https://github.com/props-nothing/jarvis-core/releases) with write access to see drafts. Their notes identify the source commit and Actions run. The workflow does not publish or create/move a Git tag ref; automatic drafts refuse an already-existing tag ref, and every attachment refuses a published release. Existing automatic drafts must still target the tested SHA. Release assets remain with the draft beyond the 14-day Actions retention. A failed network upload can leave a partial draft; rerun the failed attachment job to replace its assets after the same checks. A permission failure against an older source commit stays a failure; the workflow never substitutes the newer main tip. Signing and the other public-release gates remain manual acceptance requirements.

Local regression checks: `node --test scripts/test-release-flow.mjs` and `bash scripts/test-package-release.sh`. The release tests execute the CI lint/test pipelines under Bash `-eo pipefail`, proving both live diagnostic output and nonzero Cargo exits survive `tee`. CI streams workspace output as well as retaining annotation files, so a runner timeout no longer hides which tests were still running. Do not bypass quality or merely extend a timeout without that evidence.

CI run `38640916407` (2026-10-14) failed the Linux non-owner phone-disclosure contracts and the macOS coding-activity test; Windows timed out with redirected output. Release run `38644719396` correctly skipped packaging. The disclosure was restored without changing owner-call wording; the activity test now waits for both snapshots before cancelling, with bounded readiness/completion waits for loaded runners. The Windows timeout did not reproduce in the local complete suite. A successful hosted native matrix and packaging run still require these repairs to land on the default branch; local tests do not prove those runner or upload paths.

Local Windows evidence (2026-10-15): all 2,916 workspace tests passed (one ignored), all 109 console/release JavaScript tests passed, and native Phase 1/2/3 gates, formatting, strict Clippy and workflow lint passed. An isolated locked release build was packaged with the production script, checksummed, unpacked into an empty directory and smoke-tested using `jarvis version` and `jarvis daemon --version`. Isolation avoided replacing the owner's running executable. This proves the native Windows archive path, not signing, the other platforms or hosted draft attachment.

The post-push macOS run `38732260620` exposed two additional causes, not a release-script error: the activity test treated legitimate `/private/var` paths as secret fixture content, and the workbook XML-depth test overflowed the recursive parser's stack before its old depth check ran. The former now exercises a `private` path on every OS while checking the exact secret markers; the latter rejects excessive nesting iteratively before parsing, with bounded-stack subprocess regressions. Fatal stack/abort evidence is separately annotated even when the process cannot print its panic summary. Both failures have portable regression coverage; a native macOS rerun is still required. Final stable-tree Windows verification after both repairs passed all 2,932 workspace tests (one ignored), all 111 combined JavaScript tests, formatting, strict workspace Clippy and workflow lint. No release gate was bypassed; the source repairs must be pushed before rerunning hosted CI.

A subsequent macOS run (`38738538886`, job `116433947041`) found the remaining gap: **valid** XML at the allowed 128-element boundary still overflowed the recursive parser on the caller's stack. The over-limit preflight cannot protect that case. The adapter now gives parsing its own finite 16 MiB scoped worker stack, joined before returning the borrowed document; it does not change CI or caller stack settings, reduce accepted depth or relax any validation. The 256 KiB caller-stack subprocess regression now exercises accepted boundary/lexical cases too: it reproduced the old overflow safely and passes with the parser budget. All 31 document-write tests and all 2,932 locked workspace tests (one ignored), formatting and strict workspace Clippy pass on Windows. Native macOS verification still requires pushing and rerunning CI; the successful zero-test doctest summaries at the end of the log are not the failure.

## Target Release Contract

The target/future sections specify required product behavior and retain the original planning design where identified. The source bootstrap below is available today; signed downloads, complete service reconciliation and deployment profiles remain release goals, not implementation claims.

## Artifact Matrix

Initial targets:

| OS | Architecture | CLI/daemon | Desktop |
| --- | --- | --- | --- |
| Windows | x86_64 | required | required when Phase 9 ships |
| Windows | ARM64 | evaluate/native CI required before claim | evaluate |
| macOS | Apple Silicon | required | required |
| macOS | Intel | best effort only if CI/signing remains viable | best effort |
| Linux | x86_64 | required | required |
| Linux | ARM64 | required for server/edge | evaluate desktop |

Build each release on native or officially supported CI. Do not claim a target from an untested cross-compile alone.

Artifacts include version, target triple, archive/package type, checksum, signature, SBOM, and provenance. `jarvis` and `jarvisd` versions are released together in v1.

## Installation Modes

### Source bootstrap (available today)

```bash
# macOS / Ubuntu, after installing the source-build prerequisites
curl -fsSL https://www.jarvisd.dev/install.sh | bash
```

```powershell
# Windows PowerShell 5.1+, after installing the source-build prerequisites
irm https://www.jarvisd.dev/install.ps1 | iex
```

Inspect the same-host scripts before running them. They build source, preserve checkout origin/local-change protections and leave service startup off by default. Missing prerequisites give platform-native instructions; no system packages are installed automatically. Bash retains an explicit `JARVIS_INSTALL_RUST=1` opt-in for per-user rustup. See the [prerequisite evidence and limits](../research/integrations/source-install-prerequisites.md).

Open a new terminal and run `jarvis` for setup. Without a terminal, follow the printed `jarvis init` command with private absolute key-file paths; run `jarvis start` and `jarvis doctor --live` only if setup succeeds. An installer success is not a completed profile or provider test.

### Signed-download bootstrap (future)

The target bootstrap verifies a signed release manifest and binary before execution. It will not require a Rust source toolchain. That download/signature contract is not implemented by today's website scripts.

### Native packages

- Windows installer/package with signed binaries
- macOS signed and notarized app/package
- Linux tarball plus appropriate AppImage/deb/rpm where maintained
- Tauri desktop bundles include or coordinate the matching daemon/CLI

### Portable

Extract and run with an explicit profile/data directory. No PATH, service, registry, LaunchAgent, or systemd changes.

Implemented as `jarvisd --root <absolute-directory>` and `jarvis <command> --root <absolute-directory>`. The root must be absolute: a relative value is a usage error rather than a path resolved against the current directory. Every managed directory (`config`, `data`, `cache`, `state`, `runtime`, `logs`) is created inside the root, and the local endpoint is derived from the root as well as the profile name, so a portable profile and a natively installed profile with the same name can run concurrently without contending for one socket or pipe. Portable mode installs no service.

### Container/server

Optional image for headless server mode. It is not the recommended personal-desktop install and does not grant host desktop/audio control automatically.

## Application Paths

Use platform path APIs. Exact application identifiers are finalized before implementation.

| OS | Config | Data/state | Cache/logs | Local transport |
| --- | --- | --- | --- | --- |
| Windows | `%APPDATA%\JARVIS` | `%LOCALAPPDATA%\JARVIS` | local app data subdirs | named pipe |
| macOS | `~/Library/Application Support/JARVIS` | same owned tree | `~/Library/Caches/JARVIS`, `~/Library/Logs/JARVIS` | Unix socket in secure runtime/data dir |
| Linux | `$XDG_CONFIG_HOME/jarvis` | `$XDG_DATA_HOME/jarvis`, `$XDG_STATE_HOME/jarvis` | `$XDG_CACHE_HOME/jarvis` and state logs | `$XDG_RUNTIME_DIR` Unix socket |

Directories containing state or IPC endpoints use user-only permissions/ACLs. `doctor` detects unsafe ownership/permissions and path mismatch between CLI and installed service.

## Per-User Service

- Linux: systemd user service.
- macOS: launchd LaunchAgent.
- Windows: per-user Scheduled Task/logon launcher by default to avoid forced elevation; an optional Windows Service mode may be designed for server/shared-machine deployments.

Service installation is a reconcile operation:

1. Resolve the exact verified binary and profile paths.
2. Read existing owned definition and determine drift/ownership.
3. Stage new definition atomically.
4. Load/start through the platform manager.
5. Wait for process plus RPC readiness.
6. Verify service binary version, config/state path, and daemon protocol.
7. Roll back staged definition when activation fails.

Never rewrite an externally managed service without explicit operator action.

### Implemented Planning Surface (P1-011)

This records the original read-only planning slice, not the ceiling of the current service commands. Later slices added `jarvis service install` and `uninstall`; see the [platform guide](../user/platforms.md) for current behavior and verification limits.

The original `jarvis service` planning surface implements steps 1, 2, and 6 and is strictly read-only: it resolves
the daemon binary (beside the client, never the client itself), renders the
definition the platform manager would use, and classifies the existing definition.

| Drift | Meaning | Replaceable without operator action |
| --- | --- | --- |
| `absent` | no definition exists | yes |
| `current` | an owned definition matches exactly | yes (no-op) |
| `drifted` | an owned definition differs | yes |
| `foreign` | a definition without the `jarvis-managed-service` marker | **no** |
| `unreadable` | the definition could not be read | **no** |

Ownership is decided by the marker inside the definition, not by its path. Staging,
activation, readiness waiting, and rollback were outside this original slice. Do not read that historical exclusion as a claim that service installation is still absent, or that every target reconciliation guarantee has since been proven.

## Installer Flow

1. Detect OS, architecture, shell, install scope, existing installations, and profile.
2. Resolve the requested stable/beta/pinned release from a signed manifest.
3. Download to a temporary owned directory with size/time bounds.
4. Verify manifest trust, artifact checksum, and signature before extraction/execution.
5. Stage binaries; keep an existing working version.
6. Add/update PATH using platform-safe methods.
7. Run version/config/schema preflight.
8. Launch onboarding or print a non-interactive next command.
9. Optionally reconcile the per-user service.
10. Run `jarvis doctor` and health probe.
11. Write an install receipt containing no secrets.

The installer is idempotent and accepts non-interactive flags. It never hides an elevation prompt; elevated modes require the user to run an explicit platform command.

## Onboarding

First launch:

1. Choose local/portable/server mode.
2. Create local user and workspace.
3. Select a model provider or local endpoint and test it.
4. Initialize storage and show data location.
5. Optionally install background service.
6. Optionally connect a first account.
7. Optionally configure voice after text works.
8. Show privacy/data-flow summary and tool approval defaults.
9. Run doctor and one deterministic local self-test.

Onboarding is resumable and never stores a connector account before successful identity/connectivity verification.

## Doctor

Checks include:

- binary/client/daemon/protocol version alignment
- config parse/schema/future-version errors
- config/data/cache/log path agreement and permissions
- singleton lock, local socket/pipe, ports, readiness
- database integrity/migration/backup status
- secret-provider access without displaying values
- model/runtime/connector health and missing scopes
- MCP server definitions, executable identity, protocol compatibility
- service definition drift and duplicate installations
- update state, available rollback, signing trust
- disk space, clock/timezone, TLS/proxy configuration
- redaction self-test and unsafe remote exposure

Checks emit stable IDs, severity, facts, remediation, whether repair is safe, and post-repair verification. `--json` is suitable for support automation.

## Update

1. Acquire an update/maintenance lock and record a durable update run.
2. Fetch and verify a signed release manifest/artifact.
3. Check target schema/config compatibility and free space.
4. Create/verify required backup.
5. Stage the new binary separately.
6. Ask the daemon to drain/handoff; stop only after bounded wait.
7. Atomically switch launcher/current pointer.
8. Start and probe the new daemon plus client protocol.
9. Run post-update doctor checks and migrations.
10. Finalize, retaining rollback according to policy.

On failure, restore service definition/binary and compatible state. A database migration that prevents binary rollback must be explicitly identified before activation and provide a supported restore path.

## Signing And Supply Chain

- protect release signing in a dedicated CI/environment with least privilege
- pin third-party actions by immutable revision
- publish checksums, signatures, SBOM, build provenance, source revision, and dependency lock
- scan dependencies, licenses, secrets, and artifacts
- verify installers from clean machines using only published trust roots
- document key rotation and compromise response before public release

## Release CI

Stages:

1. format/lint/unit/repository tests
2. contract/integration/security checks
3. native target builds
4. package/install smoke in clean environments
5. database upgrade/backup/restore and update/rollback
6. selected opt-in live-provider and end-to-end lanes
7. artifact signing/provenance/SBOM
8. publish candidate channel
9. post-publish install verification
10. promote immutable candidate to stable

Never rebuild a different artifact during promotion.

## Uninstall

Uninstall separates:

- stop/remove service
- remove binaries/PATH/desktop app
- retain or delete config
- retain or delete canonical data/artifacts/logs/backups
- delete keychain/secret entries
- revoke connector/provider credentials where supported

Defaults preserve user data and explain its location. Full deletion requires explicit confirmation or non-interactive flags and emits a manifest/receipt without sensitive content.

## Operations Before Stable

- backup/restore runbook and rehearsed recovery objectives
- remote exposure and trusted-proxy runbook
- incident response and signing-key rotation
- support bundle schema and privacy review
- release channel/rollback support policy
- supported OS/architecture and database compatibility table
- provider-cost and live-test budgets
