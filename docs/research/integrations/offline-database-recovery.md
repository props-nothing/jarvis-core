---
integration: offline-database-recovery
status: implemented-native-accepted
last_verified: 2026-10-18
selected_sdk: "Rust 1.98.1 std; atomic-write-file 0.3.1; existing pinned SQLx/SQLite"
---

# Offline database snapshots and interrupted restore recovery

This P12-001 / P9-006 slice supplies a usable data recovery primitive before
binary activation. No external service, credentials or new dependency is added.
The owner explicitly invokes offline snapshot/restore commands; models cannot
request them through a new tool surface. Configuration, keys, artifacts and the
program itself are outside this database-only snapshot.

Official sources read live on 2026-10-18 (Europe/Amsterdam):

- `https://doc.rust-lang.org/llms.txt` and `https://www.sqlite.org/llms.txt` were
  inaccessible; continued to official reference documentation.
- https://doc.rust-lang.org/std/fs/struct.File.html#method.try_lock : exclusive
  OS locks return WouldBlock when held; closing the handle releases the lock.
  The current reference is 1.99.1; the 1.98.0 versioned page was inaccessible.
  The existing daemon already uses this API with the repository's pinned 1.98.1;
  compile/native tests verify this selected version. Locks coordinate cooperating
  JARVIS processes, not arbitrary owner applications or an OS sandbox.
- https://doc.rust-lang.org/std/fs/fn.rename.html : replacement semantics vary
  by OS; a multi-file rename sequence is not a transactional restore.
- https://docs.rs/atomic-write-file/0.3.1/atomic_write_file/ and
  https://docs.rs/atomic-write-file/0.3.1/atomic_write_file/struct.AtomicWriteFile.html :
  write staging separately, synchronize, then commit by replacement; before commit
  the prior target remains visible. Reuse the existing dependency/private-file
  conventions, streaming file bytes rather than loading a database into memory.
- https://www.sqlite.org/wal.html : WAL may contain committed data absent from
  the main file; copying/replacing only the main file in that state is unsafe.
  Offline operations hold the daemon's actual profile lock and refuse any WAL,
  SHM or rollback-journal sidecar. Never delete a sidecar to make a guard pass.
- https://www.sqlite.org/lang_vacuum.html#vacuum_with_an_into_clause : VACUUM INTO
  is a consistent snapshot alternative, already used before migration. This
  slice deliberately copies a stopped, sidecar-free database without opening it
  for migration or changing its journaling mode.
- https://www.sqlite.org/uri.html#uriimmutable : immutable inspection skips SQLite
  locking/change detection and must only read an unchanging standalone image.
  An initial real WAL-mode test showed ordinary read-only inspection creating
  WAL/SHM files. Offline inspection therefore additionally uses `immutable(true)`
  after refusing all sidecars, with profile ownership for the canonical image.
  Ordinary live `doctor` inspection retains normal SQLite locking/WAL behavior.
- https://raw.githubusercontent.com/launchbadge/sqlx/v0.9.0/sqlx-sqlite/src/options/mod.rs
  and https://raw.githubusercontent.com/launchbadge/sqlx/v0.9.0/sqlx-sqlite/src/connection/establish.rs :
  selected SQLx 0.9.0 exposes `immutable(bool)` and translates it to SQLite's URI
  flag. The versioned docs.rs options page was inaccessible; verified official
  tagged source and the installed dependency instead. No authentication or remote
  rate limit applies; disk capacity and exclusive offline ownership bound use.

## Contract and limits

Use the standalone mode of the read-only inspector to require a healthy current schema and
consistent application/migration/metadata markers. Require regular files, private
profile directories, bounded strict JSON manifests, UUID-only backup identities,
streamed SHA-256/size verification and durable writes. A manifest is local integrity
evidence, not a publisher signature. No implicit schema downgrade is supported.

Before replacement retain an independently verified snapshot of the current
database, stage verified requested bytes, and persist a pending restore journal.
The daemon refuses a pending or malformed journal before opening SQLite. Explicit
recovery classifies the database as the recorded before/after digest and validates
it; unknown bytes remain blocked, without guessing or deleting evidence. Recovery
after process termination and storage errors is required; no claim of power-loss
atomicity across multiple files or protection against faulty hardware follows.

Acceptance must include real SQLite values before/after restore, surviving
pre-restore state, daemon-lock contention, corruption/foreign/future-schema and
sidecar refusal, pending-before/pending-after recovery, ambiguous-state refusal,
and schema guard falsification. Native CI must exercise all three OSes;
process tests stay on hosted runners when they could disturb the owner's desktop.

## Local evidence (2026-10-18)

Ten focused Windows storage tests passed using migrated SQLite images: restore
and undo recover actual persisted values, current-schema copies create no sidecars,
OS lock contention refuses maintenance, and corruption/foreign schema/future
schema/broken foreign keys/sidecars fail closed. Pending-before, pending-after and
unknown-image recovery preserve evidence. Actual child-process exits at both
restore boundaries leave a pending journal and release the OS lock; explicit
recovery identifies the outcome without replacing bytes. These test subprocesses
use `CREATE_NO_WINDOW`; fault injection exists only in the unit-test binary.

The daemon startup refusal, strict CLI selector and doctor recovery-diagnosis tests
also passed. Removing only the schema/health guard made the valid-digest foreign
database refusal test fail at `PRAGMA application_id = 42`; the guard was restored.
Formatting, strict workspace Clippy and dependency policy pass. The full Windows
workspace passed 3,241 tests across 62 targets, with zero failures and six existing
opt-in ignores. The CLI was rechecked after adding absolute output paths; diagnostic
code/remediation tests passed with both new findings included. All 66 release tests,
six documentation tests, 381-document/1,174-link checks and workflow lint pass.

## Native evidence (2026-10-18)

[CI 39076425958](https://github.com/props-nothing/jarvis-core/actions/runs/39076425958)
passed all 14 jobs for `40fc573c8dc4018131bbfd52fe251ae6515e7dfc`.
Windows storage, daemon and remaining-workspace partitions all passed; Linux and
macOS passed their complete workspace suites, including recovery and the Unix
symlink guards. Native strict Clippy, dependency policy, documentation checks and
freshly rebuilt phase/coding-lifecycle process gates passed. This closes the
offline database recovery slice's native gate. Binary update/health rollback,
schema downgrade, whole-profile disaster recovery and clean-machine onboarding
remain separate work.
