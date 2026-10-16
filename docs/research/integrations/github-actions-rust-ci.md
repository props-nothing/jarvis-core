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
