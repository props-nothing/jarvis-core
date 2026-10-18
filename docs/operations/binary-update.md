# Signed binary updates and rollback

[Installation](install-and-release.md) / [Database recovery](database-recovery.md) /
[ADR-0242](../adr/0242-binary-activation-retains-evidence-and-requires-offline-ownership.md)

The updater stages an exact release from the official repository. It verifies the
publisher signature, release identity, archive size/hash and extracted executable
before running a version check. Activation verifies that evidence again. There is
no unsigned update mode and no implicit channel/latest selection.

## Stage

Use an absolute existing installation directory containing `jarvis` (`jarvis.exe`
on Windows):

```text
jarvis update stage ci-b9d8773cb26a3ae7be722a89fc7b3acbd31a05a0 0.1.0 --dir ABSOLUTE_INSTALL_DIR
```

The JSON reply contains a stage ID and an absolute `updater` path. Keep that path:
it is a retained copy of the invoking updater, so Windows can replace the installed
executable after the original command exits. Staging preserves the installation.
It may run alongside a daemon that holds a shared installation lock.

The published [protocol-2 release](https://github.com/props-nothing/jarvis-core/releases/tag/ci-b9d8773cb26a3ae7be722a89fc7b3acbd31a05a0)
contains this updater. Both installed and candidate binaries must advertise
maintenance protocol 2, including the installation lock, startup recovery guard
and identical `database_migrations_sha256` fingerprints. Historical releases
without this contract, including the protocol-1 trial `0042b09`, can be staged
but are refused during activation. Do not rewrite migration records to bypass a
refusal. Retain an older installation's original helper for its existing journal.

## Activate

Stop JARVIS normally, then invoke the returned updater:

```text
ABSOLUTE_UPDATER_PATH update activate STAGE_ID --dir ABSOLUTE_INSTALL_DIR
```

In PowerShell, invoke a quoted path with `& 'ABSOLUTE_UPDATER_PATH'`. For a portable
profile, append `--root ABSOLUTE_PROFILE_ROOT` to stop, activation and recovery
commands. Use the same profile throughout. Other profiles running from this
installation must also stop. The binary directory must be writable by its owner:
daemon startup keeps `.jarvis-update/lock` beside the executable, including when
using a portable profile. No OS sandbox is implied by these cooperating locks.

Activation checks the installed binary, candidate, service ownership and current
healthy database. Database/configuration/local-protocol versions must match
exactly, including the embedded SQLx migration checksum history. The current
database's successful migration records must match that history. Builds from
checkouts with different migration line endings may be incompatible despite an
equal schema number; refusal never rewrites checksums or existing data.
It retains `previous` and a verified database snapshot, journals intent,
then atomically replaces the installed executable at the same path. It checks
the executable version/maintenance protocol and offline database again. A failed
check restores the previous executable only when the recorded images and unchanged
database prove that restoration is safe.

Only an absent service definition or an exact JARVIS-owned definition for the
native installation is accepted. Drift, foreign/unreadable definitions and
portable operation with an existing service definition are refused. Reconcile
the service explicitly first. This command does not modify the service manager
or request daemon startup. Start JARVIS explicitly and run `jarvis health` and
`jarvis doctor --live` afterward; offline checks are not a live readiness claim.

## Inspect, roll back, recover

```text
jarvis update status --dir ABSOLUTE_INSTALL_DIR
ABSOLUTE_UPDATER_PATH update rollback --dir ABSOLUTE_INSTALL_DIR
ABSOLUTE_UPDATER_PATH update recover --dir ABSOLUTE_INSTALL_DIR
```

Rollback restores the latest retained previous executable while preserving the
current healthy database, including content written after the update. It refuses
incompatible schemas, changed binaries or missing recovery evidence. Database
restore is a separate explicit [backup operation](database-recovery.md).

A pending/malformed journal blocks daemon startup and produces doctor's
`installation.update_unsettled` finding. Recovery recognizes only the recorded
previous/candidate binary and checks that a pending transition's database bytes
are unchanged. It settles an intact previous image as rolled back, or a healthy
candidate as applied. An interrupted rollback finishes restoring the previous
binary. Unknown bytes remain blocked: preserve the installation, journal and
snapshots for diagnosis; never delete evidence to force startup.

Repetition after a settled activation is harmless when the stage ID and current
image still match. Rollback/recovery are repeatable. A failed staging attempt or
pre-journal interruption may leave an incomplete retained stage; stage again to
get a new identity. No automatic cleanup removes archives, verifier/trust material,
previous binaries or snapshots. `install` refuses to overwrite an installation
with update evidence. Retained-image repair and guarded removal are defined by
[ADR-0243](../adr/0243-repair-and-removal-preserve-installation-evidence.md).

## Repair and removal

Use a separate executable containing ADR-0243's maintenance commands, normally the
retained updater printed by staging. Stop JARVIS first. A helper copied from an older
release may predate `repair`; after verified activation, retain a separate copy of the
new executable if necessary. On Windows this avoids replacing a running executable.

```text
UPDATER update repair --dir INSTALL_DIR
UPDATER uninstall --dir INSTALL_DIR
```

Append `--root PROFILE_ROOT` for a portable profile; that path does not change native
services or PATH. Native uninstall verifies service cleanup, removes only this
installation's PATH entry/links, and removes the installed executable. It refuses an
active owner or failed cleanup. Running uninstall from the installed executable fails
before cleanup and points to retained helper evidence when available.

Repair requires a settled update journal and restores its selected executable,
including a missing target. It rechecks signed evidence for an applied candidate and
the retained digest for a rolled-back previous image, preserves damaged bytes, and
does not change the current database. Pending journals need `recover`; an installation
without retained update evidence has no automatic binary-repair source. Repeating
repair/removal is supported. Uninstall leaves settings, keys, memory, history and
update evidence (including helper copies) in place and never recursively deletes an
installation or profile. A missing executable after uninstall can be restored through
explicit repair while its evidence remains intact.

[Clean-machine self-test](clean-machine-lifecycle.md) remains an independent gate.

## Native readiness after an offline transition

`applied` means the executable and offline database checks passed. Starting the
daemon is a separate explicit action: it can resume configured work. The updater
does not start a trial daemon against canonical data while its journal is pending.

For a current owned service, start the stable installed path through its manager:

| Platform | Start after activation or rollback | Stop before maintenance |
| --- | --- | --- |
| Linux | `systemctl --user start jarvisd.service` | `jarvis stop`, then `systemctl --user stop jarvisd.service` |
| macOS | `launchctl start dev.jarvis.jarvisd` | `jarvis stop` (the owned agent does not restart a successful exit) |
| Windows | `jarvis start --no-open` | `jarvis stop` |

Windows registers a per-user login command, not a Windows SCM service. For a
portable installation use `jarvis start --no-open --root ABSOLUTE_PROFILE_ROOT`.
Keep the same profile selection on every command. Do not force-stop busy work to
make an update proceed; wait for its completion or explicitly cancel that work.

Run `jarvis status --json` and `jarvis health --json`. Both must report `live: true`
and `ready: true`; a successful diagnostic command alone is not a ready result.
Compare status's daemon version, database schema, configuration schema and protocol
with `jarvis update probe`. `jarvis service --json` must report `drift: current`
and the intended stable binary path. A manager entry alone does not prove readiness.

If startup/readiness fails, stop the selected profile and use the retained helper's
`update rollback` command above, then repeat startup and readiness checks. Preserve
the journal, previous binary and snapshots if rollback itself is refused. Binary
rollback preserves the current database and does not repair an invalid configuration.

The release workflow invokes `scripts/test-update-lifecycle.mjs` against the exact
published tag. Its native mode requires a disposable hosted runner and refuses an
existing profile/service. It checks the manager's actual process path, authenticated
status/health, stable service definition, retained snapshot, current-data rollback,
and recovery startup. Local portable execution is:

```text
node scripts/test-update-lifecycle.mjs ABSOLUTE_TEST_BINARY ci-COMMIT VERSION
```

## Evidence limits

The protocol-2 `b9d8773` release passed all native quality checks in
[CI 39122471338](https://github.com/props-nothing/jarvis-core/actions/runs/39122471338)
and all publication, installation and native update gates in
[Release 39123649427](https://github.com/props-nothing/jarvis-core/actions/runs/39123649427).
Windows x86_64, macOS ARM64 and Linux x86_64 verified actual service startup,
authenticated readiness, signed activation, rollback retaining newer database
writes, pending-journal refusal, recovery and service removal. A real Windows
signed candidate with mismatched migration history was refused without mutation,
and the previous daemon remained ready. [Research](../research/integrations/binary-update.md)
retains the failures, fixes and local falsification evidence.

The previous executable is a distinct source build; both package versions are
0.1.0. Hosted runners contain development tools, and Windows executes the persisted
login command without a reboot/logon. This is not acceptance of schema-changing
updates, two different signed package versions, clean-machine onboarding,
complete binary repair/uninstall, Linux ARM64 updates, Intel macOS runtime or
native Authenticode/notarization. Startup remains explicit after offline settlement.
