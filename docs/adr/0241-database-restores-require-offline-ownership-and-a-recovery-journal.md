# ADR-0241: Database restores require offline ownership and a recovery journal

Status: Accepted

Date: 2026-10-18

## Context

P12-001 requires update/rollback without losing canonical data. Pre-migration
backups exist, but there is no owner-facing verified snapshot/restore operation.
A filesystem copy over a live SQLite database can lose WAL data; a crash between
replacement and recording success must not silently resume with ambiguous state.

## Decision

Rust storage owns database-only snapshots and restore journals. Explicit CLI
commands acquire the same profile lock as the daemon, refuse active profiles and
any SQLite sidecars, and require a healthy schema supported by the current binary.
Snapshots live under private profile data, with strict bounded metadata and a
verified byte digest. They contain personal canonical data and are never committed.
They do not copy settings, keys, files or binaries.

This is an explicit offline exception to the ordinary CLI request path. The CLI
only parses/renders; storage owns SQL and recovery policy. Owner filesystem access
and the profile lock authorize this local operation, with no model tool, HTTP
endpoint or remotely supplied path. Doctor reports unresolved journals before
SQLite inspection; operators must close diagnostic readers during maintenance.

Restore first retains a verified snapshot of the current database. It stages the
selected verified snapshot, writes a pending journal, then atomically replaces the
database and settles the journal. The daemon checks the journal while holding its
profile lock and refuses pending or malformed state before opening SQLite.

`backup recover` settles only when the actual healthy database matches the recorded
before or after bytes. Unknown bytes, missing evidence, unsupported schema and
inconsistent metadata fail closed. Recovery never deletes a WAL or guesses which
copy should win. A later binary updater must use compatible restoration policy;
this decision does not authorize automatic schema downgrade.

## Consequences

The owner stops JARVIS before snapshot/restore and starts it afterward. Restoring
reverts canonical database content, including task state and memory, to the chosen
snapshot; the pre-restore copy permits undo. Normal restart recovery remains
responsible for interrupted work, and cannot promise to undo external effects.
Restored schedules and queued work may become due again; the owner must choose
the intended state before explicitly restarting. Restore is not external-effect
compensation. The current database must also be healthy and current-schema; this
slice does not repair missing/corrupt canonical files or migrate restored snapshots.
The latest journal is retained for diagnosis; snapshots are retained until the
owner deliberately removes them. No automatic retention deletion is added.

This is a data recovery slice, not completed binary update/rollback, service
reconciliation, whole-profile backup or clean-machine onboarding. See the
[research record](../research/integrations/offline-database-recovery.md).
