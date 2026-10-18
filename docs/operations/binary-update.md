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
jarvis update stage ci-<40-character-commit> 0.1.0 --dir ABSOLUTE_INSTALL_DIR
```

The JSON reply contains a stage ID and an absolute `updater` path. Keep that path:
it is a retained copy of the invoking updater, so Windows can replace the installed
executable after the original command exits. Staging preserves the installation.
It may run alongside a daemon that holds a shared installation lock.

The existing historical signed release can be staged, but cannot be activated:
both the installed and candidate binaries must advertise maintenance protocol 1,
which includes the installation lock and startup recovery guard. A signed release
containing this updater must be published before production activation acceptance.

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
exactly. It retains `previous` and a verified database snapshot, journals intent,
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
with update evidence. Uninstall/repair of this retained layout remains an explicit
acceptance gate.

## Acceptance limits

Windows local tests and real signed staging evidence are in the
[research record](../research/integrations/binary-update.md). Native CI includes
the same lifecycle tests and production staging/startup gate on Windows, macOS
and Linux; adding a gate does not mean it has passed on hosted runners.
Full signed new-release activation, native service reconciliation/readiness,
schema-changing upgrade/downgrade, clean-machine lifecycle and native certificate
signing are not established by this initial offline slice.
