# Offline database backup and restore

[Documentation](../README.md) / [Installation and releases](install-and-release.md) / [ADR-0241](../adr/0241-database-restores-require-offline-ownership-and-a-recovery-journal.md)

Use a build containing ADR-0241 or later. The first signed prerelease,
`ci-70cfc6eecb4eb2c69e227fc4599e73234b4a630d`, predates these commands.

`jarvis backup` saves and restores the canonical SQLite database in a stopped
profile. It retains memory, tasks, schedules and other database state. Settings,
key files, keychain entries, artifacts and binaries are outside this snapshot.
Keep snapshots private: they contain personal data and are not encrypted by this
command. This is a same-schema recovery step; automatic binary update/rollback
and restoration into a missing or corrupt database remain unimplemented.

## Save a snapshot

Stop JARVIS and close other programs inspecting its database. For a portable
profile, pass the same absolute `--root` directory to every command below.

```text
jarvis stop
jarvis backup create --json
jarvis backup verify BACKUP_ID --json
```

Replace `BACKUP_ID` with the `backup.id` returned by create. Successful output
includes the schema version, byte count and SHA-256. The snapshot and its manifest
are retained under the selected profile's data directory. The `backups_directory`
and `restore_journal` output fields give the absolute locations. Record the ID
with your recovery notes. There is no automatic retention deletion or implicit
selection of the latest snapshot.

Verification reads the snapshot and does not open the canonical database. Creation
requires the same exclusive profile lock used by daemon startup, a healthy current
schema, consistent application/version markers and no SQLite sidecars. Neither
operation migrates the database. A local digest detects accidental changes; it is
not a publisher signature or protection from someone controlling your account.

## Restore or undo

Restore rewinds **all database state**, including approvals, pending work and
schedules. External effects already performed cannot be undone. Choose a snapshot
whose pending work you intend to resume: schedules due in that snapshot may run
again after startup. The existing interrupted-task recovery still applies.

```text
jarvis stop
jarvis backup verify BACKUP_ID --json
jarvis backup restore BACKUP_ID --json
```

Restore verifies the selected snapshot, creates and verifies a snapshot of the
current database, stages the selected bytes, records a pending restore, and replaces
the database atomically. A successful result has `restore.state` equal to `applied`.
Save `restore.previous`: that snapshot can undo this restore with the same command.
The configuration and running program are unchanged. Start JARVIS explicitly when
ready, using `jarvis start --no-open`, then check `jarvis doctor`.

The current database and selected snapshot must both be healthy and match the
running binary's current schema. A newer/older schema, missing database, invalid
manifest, corrupt bytes, foreign SQLite file or active profile is refused. Do not
use this command as a schema downgrade or whole-profile disaster recovery tool.

## Recover an interrupted restore

The latest restore record lives in `state/database-restore.json`. If replacement
was interrupted, startup refuses to open SQLite until its outcome is reconciled:

```text
jarvis backup recover --json
```

`jarvis doctor` reports `database.restore_pending` or `database.restore_invalid`
before inspecting SQLite, preserving unresolved evidence. Close diagnostic runs
before maintenance; diagnostics are observations, not maintenance ownership.

Recovery verifies both retained snapshots and the current database. It records
`applied` when the selected bytes are present, or `not_applied` when the previous
bytes remain. It never replaces the database. Repeating recovery returns the
settled result; no journal returns `restore: null`. After `not_applied`, you may
retry the original restore explicitly. After either settled result, startup is
allowed again. Unfinished staging files from process death may remain; they are
never selected for restore and no automatic cleanup guesses whether to use them.

If evidence is missing, malformed or matches neither image, leave the daemon
stopped and preserve the profile, snapshots and journal for diagnosis. Do not
delete the journal or SQLite `-wal`, `-shm` or `-journal` files to bypass a refusal:
sidecars may hold committed data. A sidecar refusal requires its SQLite users to
close cleanly; this command deliberately does not repair an unclean SQLite shutdown.

## Verification and limits

The acceptance tests use migrated SQLite files, real persisted values and the OS
profile lock. They cover byte tampering, incompatible schemas, broken foreign keys,
sidecars, malformed journals, unknown recovery images and process termination just
before/after replacement. Windows subprocesses use `CREATE_NO_WINDOW`. Native CI
runs the storage and daemon guards on Linux, macOS and Windows. This does not prove
power-loss durability across multiple files or hardware failures. The dated
[research record](../research/integrations/offline-database-recovery.md) tracks results.
