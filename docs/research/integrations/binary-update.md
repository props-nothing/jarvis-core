---
integration: binary-update
status: implemented
last_verified: 2026-10-18
selected_sdk: "Rust 1.98.1; Cosign 3.1.3; reqwest 0.13.5; atomic-write-file 0.3.1; zip 9.0.4; tar 0.4.46; flate2 1.1.10"
---

# Signed staging and offline binary activation

## Native acceptance follow-up (2026-10-18)

Live authority rechecked before implementing the native acceptance harness:

| Contract | Exact official source | Version / operations |
| --- | --- | --- |
| systemd | https://www.freedesktop.org/software/systemd/man/latest/systemctl.html | 262 reference; user daemon-reload, enable/start/stop, show MainPID/ExecStart, is-active; runner version recorded by the gate |
| launchd | https://developer.apple.com/library/archive/documentation/MacOSX/Conceptual/BPSystemStartup/Chapters/CreatingLaunchdJobs.html and https://raw.githubusercontent.com/apple-oss-distributions/launchd/main/man/launchctl.1 | LaunchAgent ProgramArguments, RunAtLoad, KeepAlive; load/unload/list; runner macOS version recorded |
| Windows login launcher | https://learn.microsoft.com/en-us/windows/win32/setupapi/run-and-runonce-registry-keys | HKCU Run executes at login; registration alone does not prove a running daemon |
| PowerShell | https://learn.microsoft.com/en-us/powershell/module/microsoft.powershell.management/start-process?view=powershell-5.1 | 5.1; inherited environment, hidden process launch; no administrator credential needed |
| Node SQLite harness | https://raw.githubusercontent.com/nodejs/node/v24.19.0/doc/api/sqlite.md | Node 24.19.0; DatabaseSync, exec, prepared statements, close; disposable database sentinel only |
| Test executable size | https://sourceware.org/binutils/docs/binutils/strip.html and https://raw.githubusercontent.com/apple-oss-distributions/cctools/main/man/strip.1 | Native strip removes debug symbols from a disposable unsigned development copy only; signed archives and candidates stay byte-for-byte intact |
| GitHub | https://docs.github.com/en/actions/reference/workflows-and-actions/events-that-trigger-workflows and https://docs.github.com/en/rest/actions/workflow-runs?apiVersion=2022-11-28 | Release calls reusable acceptance after publication with the exact tested source, tag and version; manual dispatch supports a pinned candidate; read-only permissions |

The systemd repository `llms.txt` and Apple developer `llms.txt` returned 404;
Microsoft's index and Node's HTML reference were inaccessible, so the official
pages and versioned Node source above control. No relevant deprecation affecting
the existing launcher was observed; launchctl load/unload are legacy operations
and their actual behavior must pass the native runner gate.

No provider credentials, external effects or live onboarding are involved. Tests
use a provider-free profile and authenticate status/health with its generated
local credential, never printed or uploaded. Service tests require a disposable
GitHub-hosted runner and an absent JARVIS profile/definition/registry entry before
mutation. Local execution uses a portable temporary profile only. Manager and
client commands have timeouts; failure is never converted to a skipped pass.

Discriminating checks: distinct old/new executable digests; official signature
reverification; actual manager-launched process and authenticated readiness before
and after activation/rollback; active-daemon exclusion; pending-journal startup
refusal; retained pre-update snapshot; database writes after activation retained
by rollback; service-definition preservation and foreign-definition refusal.
The Windows test executes and inspects the registered login command without
claiming a reboot/logon test. Hosted runners are native machines with development
tools, not clean-machine onboarding or Authenticode/notarization evidence.

The Windows path-spelling regression was reproduced before its fix: an ordinary
8.3 path and `std::fs::canonicalize` of the same executable rendered different
service owners. `ServicePlan::build` now resolves existing Windows executable
paths consistently and removes only the drive-path verbatim prefix for launcher
rendering. UNC paths retain their canonical prefix. The test checks both the
definition and registration commands; it failed before the fix and passes after.

The first native macOS run found that `launchctl load -w` returned success for an
already-loaded agent after a clean stop, but did not restart it. The authenticated
readiness deadline failed on the second `service install`. The installer now
explicitly starts the loaded agent after loading; native verification must repeat
both installation calls. Linux systemd passed these initial readiness/reconcile
checks before the not-yet-published candidate correctly returned HTTP 404. Those
partial probes are not full signed-update acceptance.

## Question and authority

Verify an exact public release, stage one executable, and replace it while keeping
previous bytes and database state recoverable. Live official sources checked on
2026-10-18 UTC before implementation:

- https://docs.sigstore.dev/llms.txt and https://doc.rust-lang.org/llms.txt were
  inaccessible. https://raw.githubusercontent.com/alexcrichton/tar-rs/main/llms.txt
  returned 404. Continued to official reference and selected source.
- https://docs.sigstore.dev/cosign/verifying/verify/ and
  https://raw.githubusercontent.com/sigstore/cosign/v3.1.3/cmd/cosign/cli/verify/verify_blob.go
  control bundle, certificate issuer/identity and trust verification.
  https://github.com/sigstore/cosign/releases/tag/v3.1.3 was rechecked; reuse
  [the reviewed executable pins](release-signing.md). No relevant change observed.
- https://docs.rs/reqwest/0.13.5/reqwest/struct.ClientBuilder.html controls HTTPS,
  proxy/redirect policy and timeouts. Only official GitHub asset URLs are built;
  redirects remain HTTPS and limited. No account, token, webhook or pagination.
- https://doc.rust-lang.org/std/fs/struct.File.html#method.try_lock and
  https://doc.rust-lang.org/std/fs/fn.rename.html control OS lock and replacement
  semantics; current reference may be newer than pinned Rust, so native build
  and contention/process tests must verify the selected toolchain.
- https://docs.rs/atomic-write-file/0.3.1/atomic_write_file/struct.AtomicWriteFile.html
  documents staged writes, synchronization and commit. Reuse existing dependency.
- https://docs.rs/tar/0.4.46/tar/ and
  https://docs.rs/tar/0.4.46/tar/struct.Archive.html document sequential entries
  and extraction races. Never call unpack: select a single exact regular member
  and stream its bytes into a JARVIS-selected file. Pin tar, disable default xattr.
- https://github.com/zip-rs/zip2/releases/tag/v9.0.4 records path-validation fixes.
  Versioned docs.rs and raw src/read.rs were unavailable; the installed pinned
  SDK and the corrected official source at
  https://raw.githubusercontent.com/zip-rs/zip2/v9.0.4/src/read/zipfile.rs confirm
  raw member names, regular/symlink classification and sizes. zip is already pinned.
- https://docs.rs/flate2/1.1.9/flate2/read/struct.GzDecoder.html documents bounded
  streaming gzip decoding; the 1.1.10 page was unavailable. Inspect locked 1.1.10
  source. The selected tagged source was subsequently fetched at
  https://raw.githubusercontent.com/rust-lang/flate2-rs/1.1.10/src/gz/read.rs;
  no relevant contract difference observed. No copied vendor implementation.

## Contract, boundary and limits

The strict JARVIS-RELEASE-1 format and five-platform membership are unchanged.
Verification uses a fresh private trust home and cleared environment, public
Sigstore trust, exact workflow identity and issuer, and no insecure switches.
Downloads are bounded at 8 KiB manifest, 1 MiB bundle, 256 MiB archive/verifier;
decompression at 512 MiB, executable at 256 MiB. Public network failure refuses
staging/activation; no retry can cause activation. Local journal/receipt reads and
process output are bounded. Staging may be retried as a new retained identity.
No secrets/profile data are uploaded. Cosign is Apache-2.0; archive libraries are
MIT/Apache-2.0; workspace dependency/license checks remain required.

Domain compatibility/state contracts remain JARVIS types. Storage owns profile
exclusion and journals. The update adapter owns HTTP, verification processes,
archive decoding and atomic installation changes. CLI only selects and reports.
No model/HTTP/MCP update tool is added. Local owner commands are authorization.

## Discriminating verification and unresolved gates

Test real signed public release staging and rejection of foreign/tampered evidence,
strict manifest selection, wrong verifier pin, archive/member corruption, changed
staged bytes, profile/installation lock contention, unsupported schema/protocol,
service ownership refusal, before/after crash recovery, unknown-image refusal and
retained binary rollback. Mutate a security guard and require its negative test
to fail. Run native tests on Windows/macOS/Linux through existing workspace CI.

Old signed releases do not advertise the new maintenance protocol; staging can
verify them, but activation must refuse them. Native service-manager readiness,
schema-changing upgrades/downgrades, clean-machine installation and OS certificate
signing remain distinct gates. Never claim those from a version smoke test.

## Local evidence (2026-10-18)

The Windows workspace passed 3,259 tests across 62 targets, zero failures and six
existing opt-in ignores, with freshly rebuilt binaries and
ACCEPTANCE_REQUIRE_BINARIES=1 / ACCEPTANCE_REQUIRE_FIXTURE_PEER=1. Phase 1–4 and
both coding lifecycle process tests passed. After adding the final TAR-link and
duplicate-member case, the focused updater/CLI/diagnostics run passed 166 tests,
including all 18 updater tests. Strict workspace all-target/all-feature Clippy,
formatting, 72 release/manifest/documentation tests, documentation links, workflow
lint, and dependency advisory/license/source/bans checks passed.

The tests exercise real OS executable replacement, retained previous bytes,
verified SQLite snapshots, concurrent profile/installation locks, same-schema
refusal, current-data-preserving repeatable rollback, foreign/drifted service
files, process timeouts, unknown bytes and missing evidence. Actual subprocess
termination before and after replacement leaves a pending journal and releases
the lock; recovery classifies both outcomes. These lifecycle executables are
fixtures; they do not establish publication of a compatible signed new build.

Removing only the archive digest predicate made the real-archive negative test
fail at its refusal assertion. Exact source bytes were restored and the final
focused suite passed. Logs are retained under artifacts/p12-001/binary-update/.

The production CLI staged the real public signed 70cfc6e release on Windows,
verified it with the checksum-pinned Cosign binary, rejected a modified manifest,
and preserved the installed executable. It refused activation of that historical
release, which lacks the new maintenance protocol. A malformed installation
journal blocked the real daemon and produced installation.update_unsettled in
real doctor output. The test uses an isolated home/profile and changes no owner
service, PATH, keys or canonical data. The committed gate runs on each native CI
OS after rebuilding the CLI; no hosted result for this revision is claimed.
The final CLI build and a second live gate also passed after separating signature
failure from maintenance-protocol failure in diagnostics. The gate now asserts
the specific legacy-protocol refusal, so a verification outage cannot substitute
for that evidence. All 18 updater tests and strict workspace Clippy passed again.

Open acceptance: publish a compatible signed build and verify full real-release
activation/rollback; verify native service reconciliation and daemon readiness;
prove clean-machine installation/repair/uninstall; design schema-changing updates
before allowing downgrade. Native Authenticode/notarization remain credential
blocked. The initial offline slice deliberately leaves those backlog boxes open.
