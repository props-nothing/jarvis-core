---
integration: github-actions-rust-ci
status: implemented
last_verified: 2026-10-16
selected_sdk: Swatinem/rust-cache v2.9.2; ZIP 9.0.4; SQLx 0.9.0; Tokio 1.53.1
---

# Rust CI, dependency repair and test fixtures

## Scope

Owner-requested repair of native CI timeouts and failures, with the exact-commit release gates preserved. This does not enable publishing or alter production SQLite durability, permissions or migrations.

## Official sources (accessed 2026-10-16)

| Source | URL / version | Purpose |
| --- | --- | --- |
| GitHub index | https://docs.github.com/llms.txt | Read through HTTP; web reader rejected its Markdown content type |
| Workflow syntax | https://docs.github.com/en/actions/reference/workflows-and-actions/workflow-syntax | Branch triggers, matrix jobs, timeouts and failure conditions |
| Limits | https://docs.github.com/en/actions/reference/limits | Hosted jobs permit six hours; 30 minutes is our own limit |
| Cache contract | https://raw.githubusercontent.com/Swatinem/rust-cache/v2.9.2/action.yml | Inputs, Node 24 runtime, post-job save condition |
| Cache implementation | https://raw.githubusercontent.com/Swatinem/rust-cache/v2.9.2/src/restore.ts and https://raw.githubusercontent.com/Swatinem/rust-cache/v2.9.2/src/save.ts | Restore failure is a cache miss; dependency cleanup and save behavior |
| Cache documentation | https://raw.githubusercontent.com/Swatinem/rust-cache/v2.9.2/README.md | Compiler/lockfile keys, dependency artifacts, branch scope |
| Cache release | https://github.com/Swatinem/rust-cache/releases/tag/v2.9.2 | Tag peeled with git to `6323deb102c322ba6fcbdcafc7e3dddab59af2b6` |
| Cargo patches | https://doc.rust-lang.org/cargo/reference/overriding-dependencies.html | Root `[patch.crates-io]` overrides with local source |
| ZIP registry | https://crates.io/api/v1/crates/zip | Only 9.0.3 and 9.0.4 are unyanked at access time |
| ZIP source and release | https://raw.githubusercontent.com/zip-rs/zip2/v9.0.4/Cargo.toml and https://github.com/zip-rs/zip2/releases/tag/v9.0.4 | Rust 1.88 minimum; MIT; deflate features; path/stream fixes |
| Word dependency | https://crates.io/api/v1/crates/docx-rs/0.4.22/dependencies | Latest published writer still requires ZIP `^8.6.0` |
| Excel dependency | https://crates.io/api/v1/crates/rust_xlsxwriter/0.99.1/dependencies | Latest published writer still requires ZIP `^8.3` |
| Excel upstream | https://raw.githubusercontent.com/jmcnamara/rust_xlsxwriter/ade0a017439d169b2a9c4848fe1cb66919c0ca3d/Cargo.toml | Upstream already changes ZIP to 9.0, but is not yet published |
| SQLx pool source | https://raw.githubusercontent.com/launchbadge/sqlx/v0.9.0/sqlx-core/src/pool/mod.rs | `close().await` waits for pooled connections to close |
| Tokio cell source | https://raw.githubusercontent.com/tokio-rs/tokio/tokio-1.53.1/tokio/src/sync/once_cell.rs | Async, fallible one-time initialization; cancellation permits retry |
| Word license | https://raw.githubusercontent.com/bokuweb/docx-rs/f04cf8b47a925b2b6fff850dcb75069aae89dab4/LICENSE | MIT notice from the archive's exact source commit |
| Workflow validator | https://github.com/rhysd/actionlint/releases/tag/v1.7.12 | Local validation only; Windows binary verified against the release's SHA-256 checksum file |

Repository `llms.txt` files for rust-cache and zip2 were not found (404); their official versioned source was used. SQLx/Tokio docs.rs pages were unavailable through the web reader; the pinned official source was read instead.

## Verified contract

GitHub supplies the Actions cache credential; no personal token or additional write permission is needed. Rust-cache restores Cargo downloads and dependency build products, keys them on the compiler and manifests/lockfile, and disables incremental compilation. Its default excludes workspace artifacts. Additional keys must distinguish debug/release work and cross targets. Cache misses must always perform the ordinary locked build. Cache restoration never satisfies a quality or release gate.

Use the default GitHub cache backend, restrict saves to trusted branch/tag or successful main-CI release contexts, and do not save personal state or logs in Cargo caches. Cache entries follow GitHub branch visibility and eviction rules; the documented default repository allowance is 10 GB. Diagnostics are separate short-lived artifacts. These are build-time services only, with no JARVIS runtime mapping, OAuth flow or provider retry semantics.

ZIP 8 cannot be upgraded within either writer's published version constraint. Keep the selected writer source versions and vendor their official crate archives, preserving licenses; change only their ZIP dependency requirement to 9.0.4. Pin the direct ZIP dependency to 9.0.4 as well. Do not allow yanked versions or add an advisory exception. Remove these temporary patches when published upstream writers accept ZIP 9.

Official crate archive identities:

- https://static.crates.io/crates/docx-rs/docx-rs-0.4.22.crate — SHA-256 `7fdf00e8af6d0b3e92d4bbf9b76f773d8b84ea80f310324ad16cbdc2e653e02c`, MIT.
- https://static.crates.io/crates/rust_xlsxwriter/rust_xlsxwriter-0.99.1.crate — SHA-256 `34f2d2cda7223168a14fb86bc7411dbc3379bd7d49749ecc53b91ed98585b6b0`, MIT OR Apache-2.0.

## Decisions and verification

- Windows Rust jobs get 60 minutes. Keep native Clippy/tests on all three platforms; separate cheap frontend/format checks from compilation, and balance Windows storage/daemon/rest tests where useful.
- Keep release source selection, same-SHA checkout, all-platform completion, checksum verification and draft-only attachment unchanged. Never treat a cache hit or skipped job as successful validation.
- Reuse only a **closed, fully migrated test database image**, copied independently into each ordinary fixture and reopened through production `SqliteDatabase::open`. Production startup, migration tests and permission tests continue to use fresh databases. Do not weaken `synchronous=FULL`, skip ACL checks, share mutable databases, or cache a live connection across test runtimes.
- Verify the fixture image is isolated, contains current migration/identity rows, survives reopen, retains production pragmas and refuses overwriting an existing path. Measure the same four-thread approval-test group before and after.
- Run document read/write and hostile-archive tests, dependency policy, release refusal tests, formatting, all-target/all-feature Clippy, workspace tests and native acceptance gates. Hosted cache behavior and non-Windows platform evidence require the next GitHub run.

## Evidence and unresolved questions

Run https://github.com/props-nothing/jarvis-core/actions/runs/38899489763: Windows Clippy 4m03s, test compilation 8m23s, storage tests 713.91s, then daemon tests still passing at the 30-minute cancellation. Linux/macOS fail `mcp_secrets.rs`'s verbose bit-mask lint. Cargo-deny rejects both locked ZIP versions. These findings do not imply a release-compiler bottleneck.

Local Windows comparison: the same 21 approval tests with four test threads passed in 5.45s before fixture reuse and 1.71s after it (about 69% less elapsed time). Production permissions and durability stayed enabled. This measures the removed per-fixture migrations, not a hosted speedup; hosted disks differ. The full storage unit suite passed 354 tests in 15.22s locally after the change. The earlier hosted storage number is from a different machine/revision and is not a valid before/after comparison.

Local verification (2026-10-16): `cargo fmt --check`, strict workspace/all-target/all-feature Clippy, all four cargo-deny policy checks, all 31 document tests, all 179 JavaScript tests (32 release-flow), archive/license refusal tests, deterministic typed-HUD rebuild and actionlint 1.7.12 passed. Default-feature and all-feature locked workspace runs each passed 3,110 tests with five existing ignored tests. Fresh all-feature workspace binaries passed Phase 1/2/3 gates with both missing-binary guards enabled. Optional live PostgreSQL/provider/container probes do not become verified merely because the default harness passes.

Every vendored file was compared with its checksummed official archive: only the normalized Cargo manifest differs in each writer, and the omitted MIT license was added for docx-rs. Root `Cargo.lock` controls JARVIS resolution; the Excel crate's standalone upstream lockfile remains ignored by its own `.gitignore`.

Remaining hosted evidence: Linux/macOS execution (including the Unix permission regression), native cache save/restore and eviction behavior, all five package builds and draft upload behavior. If branch protection names the previous monolithic jobs, its required-status list needs the new job names before merge. No hosted speedup or successful release is claimed from local validation.

## Hosted follow-up: canonical credential fixtures (2026-10-16)

Commit `8fcd458` / CI run https://github.com/props-nothing/jarvis-core/actions/runs/38920832954 passed native Clippy on every OS, dependency policy, the Linux suite/gates and Windows storage. The Windows storage unit suite took 89.98s on a cold cache (354 tests), versus 713.91s in the earlier run; revisions and runner conditions differ. macOS exposed 18 daemon credential/OAuth failures: their temporary profile roots retain the OS's symlinked ancestor, which `mcp_secrets::managed_path` deliberately refuses. Storage's own credential tests use a physical workspace root and passed.

Official Rust references accessed 2026-10-16: https://doc.rust-lang.org/std/env/fn.temp_dir.html explicitly permits a symlink result; https://doc.rust-lang.org/std/fs/fn.canonicalize.html resolves intermediate links on an existing path and returns an absolute path (extended syntax on Windows). These live pages identify std 1.99.1; the equivalent 1.98.1 documentation URLs were unavailable through the reader, so the unchanged API will also be compiled/tested on our pinned 1.98.1 toolchain. No network/authentication behavior, provider limits or production path policy changes are involved.

Repair only the five affected fixture constructors (including the ignored live helper sharing that setup) by canonicalizing their own newly created scratch directory before building `AppPaths`. Preserve the credential store's ancestor/link refusal, and extend its negative tests to cover an aliased portable root above `config`. Repository branch metadata reports no required status checks and the ruleset list is empty; no settings change is needed for renamed CI jobs.

The isolated daemon partition also needs an explicit pre-test build of `jarvis-mcp-transport`'s `fixture-peer` binary. Selecting only `jarvisd` no longer builds that sibling executable, even with `--all-features`. Keep the existing missing-peer failure guard, add the narrowly scoped build and ordering regression, and verify the guard with the existing local executable moved aside before building the prerequisite.

Repair validation on Windows: 138 MCP tests, four GitHub settings tests, four credential-store tests, 3,110 workspace tests (five existing ignored), formatting, strict workspace Clippy, 32 release-flow regressions and actionlint all passed. Moving aside the old peer made the guarded composition test fail with the expected missing-peer message; `cargo build -p jarvis-mcp-transport --features fixture-peer --bin fixture-peer --locked --timings` followed by the same test passed. This proves the new pre-test step rather than relying on an executable from an earlier local build.

## Hosted follow-up: preview fixture startup (2026-10-16)

Commit `b930f73` / CI https://github.com/props-nothing/jarvis-core/actions/runs/38923320820 passed nine jobs, including native macOS tests, all native lint jobs and all three acceptance gates on every OS. Windows daemon tests finished in 564.15s with 834 passed, one failed and five existing ignored. The repaired peer test passed. The only failure was the preview fixture's ten-second wait for a child Node server to become reachable; its bare `Elapsed` panic omitted the last process state and log tails. The log establishes the expired fixture deadline, not why startup or connectivity was delayed. Release correctly remained blocked.

Official source rechecked 2026-10-16: https://raw.githubusercontent.com/tokio-rs/tokio/tokio-1.53.1/tokio/src/time/timeout.rs. The pinned Tokio timeout returns `Elapsed` and cancels the wrapped future when its bound expires; it does not report child-process diagnostics. This is a test-harness adjustment with no authentication, external service, production timeout or process-policy change.

Allow the Windows fixture up to 30 seconds for startup, returning as soon as the real port is reachable; retain ten seconds elsewhere. Include the last real status and captured output on timeout and request child cleanup. Keep the actual HTTP body, browser-verification honesty, occupied-port refusal, explicit stop and closed-port assertions. Validate a deliberately delayed child and a child that never listens as temporary local controls; neither delay belongs in the committed suite. Native CI must still pass before packaging can run.

Local controls used temporary `node.cmd` wrappers ahead of the real Node 24.19.0 executable on only the test process's PATH. An 11-second delayed startup failed the original test at 10.46s, then passed every assertion with the repair at 11.99s. A process that deliberately never listens failed at 30.44s, reporting `port_reachable: false`, its running state and the expected stderr marker. No control process remained afterward. The unmodified five-process-test group passed before and after (1.18s / 1.19s). These controls demonstrate the fixture bound and refusal behavior; they do not establish the cause of the hosted startup delay.

The isolated repair worktree passed `cargo fmt --check`, strict all-target/all-feature Clippy and 3,110 locked default workspace tests (zero failures, five existing ignored), with the explicitly built peer required. Preserve the evidence and controls in the primary checkout's `artifacts/ci-review/preview-startup-2026-10-16/` before archiving the repair worktree. Hosted verification remains required for this new revision.

## Automatic draft attachment (2026-10-16)

Owner request: automatically add the successful packages to Releases. Before this change, successful main builds produced Actions artifacts only; attachment was limited to version tags. All ten CI jobs and all five package jobs have now passed for `8bf6cbb641fa8ada46bb39d0bfc09ac8cb5a1ee2`: [CI](https://github.com/props-nothing/jarvis-core/actions/runs/38927374174), [packages](https://github.com/props-nothing/jarvis-core/actions/runs/38929525053). All five downloaded artifact ZIP digests and their contained archive checksums matched. Four native archive smoke tests passed; Intel macOS is cross-built with its runtime smoke deliberately skipped. Evidence is retained outside the managed worktree in `artifacts/ci-review/verified-8bf6cbb/`. Draft upload was not exercised by that tag-only workflow.

Official sources rechecked before implementation on 2026-10-16:

| Source | URL / version | Relevant contract |
| --- | --- | --- |
| GitHub index | https://docs.github.com/llms.txt | Index already read above; web reader still rejects Markdown content type |
| CLI create | https://cli.github.com/manual/gh_release_create | Draft/prerelease flags, exact target, existing-tag verification, asset arguments |
| CLI view | https://cli.github.com/manual/gh_release_view | Draft state and target commit JSON fields |
| CLI upload | https://cli.github.com/manual/gh_release_upload | Replace existing draft assets with `--clobber` |
| Matching refs | https://docs.github.com/en/rest/git/refs#list-matching-references | Read-only GET returns an array including prefix matches; select only the exact pending tag name |
| Releases REST API | https://docs.github.com/en/rest/releases/releases | API examples version 2026-03-10; contents-write permission, draft visibility, target semantics |
| CLI release | https://github.com/cli/cli/releases/tag/v2.102.0 | Current official CLI release at access time |
| CLI create source | https://raw.githubusercontent.com/cli/cli/v2.102.0/pkg/cmd/release/create/create.go | Sends draft/prerelease/target fields; `--draft` prevents the post-upload publish path |
| CLI fetch source | https://raw.githubusercontent.com/cli/cli/v2.102.0/pkg/cmd/release/shared/fetch.go | Finds an unpublished draft by its pending tag name |
| Workflow conditions | https://docs.github.com/en/actions/reference/workflows-and-actions/workflow-syntax | Explicit status function is needed when a successful dependency follows a skipped job |

Use the runner-provided `gh` CLI already used by the workflow, reporting its version in the log. No additional action or runtime dependency is introduced; action revisions remain pinned. Authenticate only with the job-scoped `GITHUB_TOKEN` (`contents: write`, `actions: read`); no owner token or production provider credentials are involved. Download only this run's artifacts, validate the complete platform set and checksums, then create an unpublished prerelease with all ten assets in the create command. Supplying assets at creation avoids a second immediate draft lookup. A retry may replace assets only while the release is still a draft. Failed uploads can leave a partial draft; rerunning is the recovery, never publishing or falling back to another commit.

Main/manual branch runs use the unique pending tag `ci-<full tested SHA>` and `--target <tested SHA>`, with the full source SHA and build URL in the notes. Version-tag runs keep their existing tag and `--verify-tag`. No command pushes, moves or creates a Git tag ref, and no command publishes. Existing automatic drafts must identify the same target commit; published releases are refused. Independent commits get separate drafts rather than mutating a shared nightly release. Drafts require repository write access to view and do not become the public latest release. Assets persist with their release; Actions artifacts still expire after 14 days. Release asset uploads have the documented 2 GiB per-file limit; the package files are well below it, and the attachment job retains its ten-minute bound.

Limitations to verify live: GitHub ignores `target_commitish` when a tag already exists, so automatic drafts reserve an unused Git tag name and refuse any existing exact tag ref rather than assume `--target` overrides it. The REST API may refuse creation against an older commit with workflow-file differences from the default branch because the Actions token cannot obtain workflow-write permission. Such failures must remain failures, with no fallback to the newer branch tip. Hosted evidence must confirm the pending tag has not created a Git ref, that the release is a draft for the selected SHA, and that all ten expected assets are attached. Local falsification checks cover failed/cancelled package dependencies, invalid source SHA, missing/substituted platforms, bad checksums, mismatched targets, published releases and failed creation/upload; they cannot establish real GitHub upload behavior.

Implementation validation on Windows: all 43 release-flow tests and six documentation regressions passed, along with the 361-file documentation check, archive/license refusal checks, actionlint 1.7.12, formatting, strict all-target/all-feature Clippy and all 3,129 locked workspace tests (five existing ignored). The MCP peer was explicitly built and required. Native CI and the first real automatic draft attachment remain the hosted follow-up; local CLI fixtures are not GitHub upload evidence.
