# Installation And Releases

**A native program with signed release verification and accepted compatible updates. Native OS signing and clean-machine lifecycle remain open.**

[Documentation](../README.md) / [Quick start](../user/quick-start.md) / [Platform evidence](../user/platforms.md) / [Roadmap](../../ROADMAP.md)

## Available Today

| Path | Status |
| --- | --- |
| Build the CLI from source | `cargo build --release -p jarvis-cli --locked`; the same `jarvis` program runs its daemon. |
| Install for your user | `jarvis install` copies the program and sets up PATH; no administrator rights required. |
| Start at login | `jarvis service install` / `uninstall`; native platform details are in the [platform guide](../user/platforms.md). |
| Portable profile | `--root` keeps managed state in an explicit directory and does not install a service. |
| Database recovery | `jarvis backup create`, `verify ID`, `restore ID` and `recover`; stopped-profile, same-schema snapshots and undo. See the [runbook](database-recovery.md). |
| Pinned prebuilt first install | Opt-in scripts below verify a signed manifest for an exact release/version before downloading the selected archive. Explicit SHA-256 mode supports historical unsigned releases. No development toolchain; curl 8.4+ required. |
| CI archives | Platform workflow artifacts and automatically published prereleases with a Sigstore-signed manifest after all five packages pass; inspect the [release workflow](https://github.com/props-nothing/jarvis-core/actions/workflows/release.yml) for the revision's result. |
| Signed binary update and rollback | Explicit staging, compatible offline activation, retained-binary rollback and recovery; native service/readiness gates pass on Windows x86_64, macOS ARM64 and Linux x86_64. See the [runbook](binary-update.md). |
| Native OS signing and clean-machine lifecycle | Authenticode/notarization credentials are not configured. Clean-machine onboarding and complete repair/uninstall remain open. |

The separate product website serves working source-building scripts at `https://www.jarvisd.dev/install.sh` and `https://www.jarvisd.dev/install.ps1`, from `frontend/public/`. These clone/update a protected source checkout, build with the committed lockfile, then delegate per-user installation to `jarvis install`. They are not signed-binary downloaders. Source builds require Git, Rust via rustup and [native build tools](../development/getting-started.md#build-from-source); the installed executable needs no Rust. Build/install failure exits nonzero, without setup or a success claim. A successful installer prints interactive and noninteractive setup commands; it does not collect keys or run onboarding through piped stdin. New setup requires an ElevenLabs key and explicit folder grants before starting the assistant. Do not infer signed distribution or clean-machine readiness from this source path.

## Current CI Archive Flow

Current accepted signed release (2026-10-18):
[`b9d8773`](https://github.com/props-nothing/jarvis-core/releases/tag/ci-b9d8773cb26a3ae7be722a89fc7b3acbd31a05a0)
passes all 14 native quality jobs, five packages, twelve-asset publication, four
public signed-install checks and Windows/macOS/Linux service/update/rollback
acceptance in [Release 39123649427](https://github.com/props-nothing/jarvis-core/actions/runs/39123649427).
Maintenance protocol 2 requires exact migration history; the real signed mismatch
refusal also preserves the executable/database and leaves the old daemon ready.
See the [update runbook](binary-update.md) for compatibility and platform limits.

The [CI workflow](../../.github/workflows/ci.yml) runs native quality checks on Windows, macOS and Linux plus dependency policy. After a successful **push** run on this repository's `main`, the [release workflow](../../.github/workflows/release.yml) packages that run's exact commit, not whichever commit happens to be newest when packaging starts. Failed/cancelled CI, pull requests and foreign repositories cannot trigger packaging. Documentation-only main pushes also follow this flow.

Manual release runs and `v*` tag pushes call the same CI workflow before packaging. Five platform archives and their SHA-256 files are retained as workflow artifacts for 14 days. These artifacts are downloadable by signed-in users of this public repository. The workflow is named Release.

CI separates source/HUD checks, native Clippy, native tests and dependency policy. Windows tests use three jobs (storage, daemon, remaining workspace); every package still runs on every native OS. Each OS builds current acceptance binaries and the scripted ACP peer before the three phase gates and coding lifecycle gate. Windows Rust and packaging jobs allow 60 minutes. Pinned Rust dependency caches are separated by check/partition or release target and never substitute for a locked build or quality gate. Failed runs retain diagnostics and Cargo build timings. Ordinary main pushes and PRs run CI directly; tag validation runs through Release's reusable CI call.

Current run/revision results belong in the [active CI and release ledger](../../TODO.md#owner-requested-ci-and-release-reliability-2026-10-16), which records later hosted follow-ups. Read the outcome of each job and all five packages for the selected revision; an earlier failure narrative or a local pass does not establish its present status. Native CI success remains separate from signed public release and clean-machine acceptance.

Verified signed public prerelease (2026-10-18): [`70cfc6e`](https://github.com/props-nothing/jarvis-core/releases/tag/ci-70cfc6eecb4eb2c69e227fc4599e73234b4a630d) includes all five archives, checksums, the manifest and repository-workflow signature. The corrected bootstrap/harness at `40fc573` passes public installation on Windows x86_64, macOS ARM64 and Linux x86_64/ARM64 in [CI 39076425958](https://github.com/props-nothing/jarvis-core/actions/runs/39076425958), including both versions, fresh-profile diagnosis and preservation of an existing install with development tools hidden from PATH. This release predates the database backup commands; those require a build containing ADR-0241. Restricted PATH acceptance does not establish a clean VM or live-provider onboarding. [Signing evidence](../research/integrations/release-signing.md#hosted-follow-up-2026-10-18) retains the first harness failure and passing follow-up.

Earlier verified public prerelease (2026-10-17): [`dd65479`](https://github.com/props-nothing/jarvis-core/releases/tag/ci-dd65479d988ebf7caa9bddfb803c13cae1b0ddbf) includes the coding lifecycle repairs. All ten native CI jobs, five packages and publication passed; the exact tag, archive checksums and ten anonymous asset downloads match. Four native archive smokes passed; Intel macOS is cross-built without runtime smoke. Source bootstrap behavior and its default ref are unchanged.

Earlier verified public prerelease (2026-10-16): [`c48da888`](https://github.com/props-nothing/jarvis-core/releases/tag/ci-c48da8880e20304da4b13e4016d896ad22fc2e04) passed all ten native CI jobs, five package builds and automatic publication. Its exact tag target and all ten anonymous asset downloads match the checksummed Actions artifacts. Four native archive smokes passed; Intel macOS is cross-built without runtime smoke. [Exact evidence and run links](../../TODO.md#current-acceptance-slice-release-verification-and-coding-lifecycle-2026-10-16) distinguish this release from subsequent lifecycle changes.

The 2026-10-16 repair also removes the Unix Clippy failure and yanked ZIP dependencies without weakening dependency policy. Ordinary test fixtures reuse a closed migration image while keeping independent files and production durability checks. Commit `8bf6cbb641fa8ada46bb39d0bfc09ac8cb5a1ee2` passed all ten [native CI jobs](https://github.com/props-nothing/jarvis-core/actions/runs/38927374174) and all five [package jobs](https://github.com/props-nothing/jarvis-core/actions/runs/38929525053). All five downloaded artifacts and their archive checksums were verified. Four native archive smoke tests passed; Intel macOS is cross-built with its runtime smoke skipped. Warm native cache restores were observed. [Research and measurements](../research/integrations/github-actions-rust-ci.md) retain the earlier failures and local controls separately. That run predates automatic draft attachment and does not prove upload behavior. Later runtime changes require their own native CI and package evidence.

After all five packages succeed, every eligible run signs a manifest binding the exact repository, tested commit, tag, package version and five archive digests. It uploads the five archives, five SHA-256 files, manifest and signature bundle into a draft, verifies the uploaded assets, and **automatically publishes a prerelease**. Checksums and expected platform names are verified before upload. GitHub's received asset names, sizes and SHA-256 digests must then match all twelve local files, with the exact draft tag and target commit. Main builds (including manual main runs) use `ci-<full tested commit SHA>`; publication creates that tag at the tested commit. Each commit has its own prerelease. Version-tag builds keep their existing numeric `vMAJOR.MINOR.PATCH` tag, requiring its version to match the archives and verifying that it still points to the tested commit. Manual feature-branch publication is refused because it cannot use the trusted main identity.

Published packages are visible on the repository's [Releases page](https://github.com/props-nothing/jarvis-core/releases). Notes identify the source commit, Actions run, manifest signing, absence of native Authenticode/notarization and the Intel macOS runtime-smoke limitation. Prereleases are not marked as the latest stable release. Automatic builds refuse an existing tag before publication, and no tag is moved. Existing published releases are never overwritten. A retry can replace assets only in a draft still targeting the tested SHA; a failed upload or metadata check leaves publication blocked. If publication fails ambiguously, inspect the release before retrying. The workflow verifies published metadata and the resulting tag's commit. Four subsequent native jobs install the signed public assets with a fresh home and restricted PATH, run versions/first-run diagnosis and verify existing-install preservation. These jobs are release acceptance, not a clean-VM or live-provider onboarding claim; inspect their result before relying on that release.

After publication, three more native jobs verify the exact signed candidate through service startup, authenticated readiness, offline activation, rollback preserving newer data, recovery and service removal. They build a distinct previous executable from the tested source and retain a bounded summary. The [accepted release evidence](../research/integrations/binary-update.md) distinguishes these hosted checks from clean-machine or two-version signed-upgrade acceptance.

Release assets persist beyond the 14-day Actions retention. A permission failure against an older source commit stays a failure; the workflow never substitutes the newer main tip. Automatic prereleases are owner-authorized; native signing/notarization, complete dependency notices/SBOM, schema-changing updates and clean-machine acceptance remain unfinished stable-distribution requirements.

Local regression checks: `node --test scripts/test-release-flow.mjs scripts/test-release-manifest.mjs` and `bash scripts/test-package-release.sh`. The release tests execute the CI lint/test pipelines under Bash `-eo pipefail`, proving both live diagnostic output and nonzero Cargo exits survive `tee`. CI streams workspace output as well as retaining annotation files, so a runner timeout no longer hides which tests were still running. Do not bypass quality or merely extend a timeout without that evidence.

## Dated CI repair evidence (2026-10-14–16)

The records below preserve the findings, test counts and next steps at the time each repair was made. Statements such as “must be pushed” or “native macOS rerun required” are historical, not a claim that those commits are still unpushed or that later native runs failed. The active ledger above records subsequent runs; do not erase a failure because a later revision fixed it.

The 2026-10-16 repair also removes the Unix Clippy failure and yanked ZIP dependencies without weakening dependency policy. Ordinary test fixtures reuse a closed migration image while keeping independent files and production durability checks. Local Windows verification passed 3,110 tests in each default/all-feature workspace run, 179 JavaScript tests and all three acceptance gates, plus formatting, Clippy, dependency policy and workflow lint. [Research and measurements](../research/integrations/github-actions-rust-ci.md) distinguish this local evidence from the pending hosted native/cache/package run.

CI run `38640916407` (2026-10-14) failed the Linux non-owner phone-disclosure contracts and the macOS coding-activity test; Windows timed out with redirected output. Release run `38644719396` correctly skipped packaging. The disclosure was restored without changing owner-call wording; the activity test now waits for both snapshots before cancelling, with bounded readiness/completion waits for loaded runners. The Windows timeout did not reproduce in the local complete suite. A successful hosted native matrix and packaging run still require these repairs to land on the default branch; local tests do not prove those runner or upload paths.

Local Windows evidence (2026-10-15): all 2,916 workspace tests passed (one ignored), all 109 console/release JavaScript tests passed, and native Phase 1/2/3 gates, formatting, strict Clippy and workflow lint passed. An isolated locked release build was packaged with the production script, checksummed, unpacked into an empty directory and smoke-tested using `jarvis version` and `jarvis daemon --version`. Isolation avoided replacing the owner's running executable. This proves the native Windows archive path, not signing, the other platforms or hosted draft attachment.

The post-push macOS run `38732260620` exposed two additional causes, not a release-script error: the activity test treated legitimate `/private/var` paths as secret fixture content, and the workbook XML-depth test overflowed the recursive parser's stack before its old depth check ran. The former now exercises a `private` path on every OS while checking the exact secret markers; the latter rejects excessive nesting iteratively before parsing, with bounded-stack subprocess regressions. Fatal stack/abort evidence is separately annotated even when the process cannot print its panic summary. Both failures have portable regression coverage; a native macOS rerun is still required. Final stable-tree Windows verification after both repairs passed all 2,932 workspace tests (one ignored), all 111 combined JavaScript tests, formatting, strict workspace Clippy and workflow lint. No release gate was bypassed; the source repairs must be pushed before rerunning hosted CI.

A subsequent macOS run (`38738538886`, job `116433947041`) found the remaining gap: **valid** XML at the allowed 128-element boundary still overflowed the recursive parser on the caller's stack. The over-limit preflight cannot protect that case. The adapter now gives parsing its own finite 16 MiB scoped worker stack, joined before returning the borrowed document; it does not change CI or caller stack settings, reduce accepted depth or relax any validation. The 256 KiB caller-stack subprocess regression now exercises accepted boundary/lexical cases too: it reproduced the old overflow safely and passes with the parser budget. All 31 document-write tests and all 2,932 locked workspace tests (one ignored), formatting and strict workspace Clippy pass on Windows. Native macOS verification still requires pushing and rerunning CI; the successful zero-test doctest summaries at the end of the log are not the failure.

## Target Release Contract

The target/future sections specify required product behavior and retain the original planning design where identified. Source bootstrap and opt-in signed downloads are available today; complete service reconciliation and deployment profiles remain release goals.

## Artifact Matrix

Initial targets:

| OS | Architecture | CLI/daemon | Desktop |
| --- | --- | --- | --- |
| Windows | x86_64 | required | conditional on a selected desktop-shell requirement |
| Windows | ARM64 | evaluate/native CI required before claim | evaluate |
| macOS | Apple Silicon | required | conditional on a selected desktop-shell requirement |
| macOS | Intel | best effort only if CI/signing remains viable | best effort |
| Linux | x86_64 | required | conditional on a selected desktop-shell requirement |
| Linux | ARM64 | required for server/edge | evaluate desktop |

Build each release on native or officially supported CI. Do not claim a target from an untested cross-compile alone. A Tauri shell is demand-gated by ADR-0167; it is not required to release the CLI/daemon and browser console.

Artifacts include version, target triple, archive/package type, checksum, signature, SBOM, and provenance. The installed product uses one `jarvis` executable for CLI and daemon mode (`jarvis daemon`); the separately buildable `jarvisd` development/acceptance binary is not a required sibling in the user archive.

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

### Pinned prebuilt first install

The separate [Bash](../../scripts/install-prebuilt.sh) and
[PowerShell](../../scripts/install-prebuilt.ps1) scripts need no Git, Rust, Node,
Python or GitHub account. Download/review the bootstrap from a trusted repository
revision, then select an exact published release and its package version:

```bash
# Linux/macOS: Bash 3.2+, curl 8.4+, tar, gzip and sha256sum or shasum
bash install-prebuilt.sh ci-FULL_40_CHARACTER_COMMIT 0.1.0
```

```powershell
# Windows x86_64: PowerShell 5.1+, curl.exe 8.4+
.\install-prebuilt.ps1 -Tag ci-FULL_40_CHARACTER_COMMIT -Version 0.1.0
```

Signed mode downloads a small manifest and Sigstore bundle, then a temporary
SHA-256-pinned Cosign v3.1.3 verifier. It accepts only this repository's release
workflow identity on main (or the exact selected version tag), with the GitHub
Actions issuer. Verification uses fresh public trust metadata, isolated from
ambient Cosign configuration. GitHub/Sigstore network access is required; an
outage fails closed. The bootstrap and verifier pins are the initial trust root.

The signed manifest must match the requested tag, commit, version and complete
platform set. The selected archive's hash is verified before opening it; only one
exact regular executable is materialized, version-checked and passed to
`jarvis install`. Failed signatures never fall back to unsigned installation.
Sigstore signatures authenticate the publishing workflow; they do not provide
Windows Authenticode or Apple notarization. Those credentials are not configured.

Historical releases without a signature remain explicitly opt-in: append the
independently checked archive SHA-256 as the third Bash argument, or use
`-Sha256 PLATFORM_ARCHIVE_SHA256` in PowerShell. This mode checks bytes only.
A checksum downloaded from the same server is not a separate publisher signature.

Replace the placeholders; `latest`, branches, malformed digests and unsupported
platforms are refused. Linux/macOS x86_64 and ARM64 are selected automatically.
Intel macOS remains cross-built without native binary smoke. A missing or older
curl produces a prerequisite error. Downloads and extraction are bounded.
An existing destination executable is refused: this first-install path does not
implement schema-aware update/rollback. `--dir ABSOLUTE_DIR --no-path` (PowerShell
`-Dir ... -NoPath`) supports isolated installs. Failure remains nonzero and staging
is removed, including temporary verifier/trust metadata.

Setup and service startup stay separate. Follow the printed executable path for
guided setup, requiring a model key, tested ElevenLabs key and explicit folder
grants; after successful setup run `jarvis start` and `jarvis doctor --live`.
Installer success alone does not prove provider configuration or clean-machine
readiness. The website's default source scripts remain unchanged.
[ADR-0239](../adr/0239-prebuilt-installation-pins-the-release-and-digest.md),
[ADR-0240](../adr/0240-release-manifests-bind-artifacts-to-the-publishing-workflow.md)
and the [signature evidence record](../research/integrations/release-signing.md)
separate script fixtures, real verifier tests and hosted publication evidence.

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

The first offline binary maintenance implementation is documented in the
[binary update runbook](binary-update.md) and [ADR-0242](../adr/0242-binary-activation-retains-evidence-and-requires-offline-ownership.md).
It stages exact signed releases, retains the previous executable and a database
snapshot, and activates/rolls back only between binaries with identical schema
and protocol contracts. It requires a stopped profile and installation ownership.
Its version/protocol/offline database checks do not establish daemon readiness.
Service-manager reconciliation, schema-changing updates and clean-machine
acceptance remain open. The sequence below describes the broader target:

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
