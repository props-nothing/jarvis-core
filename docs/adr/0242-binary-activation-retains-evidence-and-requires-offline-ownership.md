# ADR-0242: Binary activation retains evidence and requires offline ownership

Status: Accepted

Date: 2026-10-18

## Context

P12-001 / P9-006 has signed first installation and offline database recovery.
Replacing the executable introduces a separate failure boundary. Windows cannot
replace the executing updater, and multiple profiles can use one installation.
A database snapshot is neither a binary rollback nor permission to erase newer
memory, task state or external-effect records.

## Decision

Rust owns update policy and durable state. A dedicated infrastructure adapter
downloads exact official releases, invokes the checksum-pinned Cosign verifier
with ADR-0240's issuer and workflow identity, validates the entire manifest, and
extracts only the selected regular executable into a private retained directory.
There is no unsigned update mode. Archive paths never become output paths.

The owner stages a release, stops the daemon, and invokes activation through a
retained copy of the existing updater outside the installed executable path.
The helper is necessary for Windows replacement; it is not a second approval.
Activation owns an exclusive installation lock and the existing profile lock.
Daemon startup holds the shared installation lock for its entire lifetime, and
refuses pending or malformed activation evidence before SQLite access. Only
installed binaries advertising this maintenance protocol can be updated.

Before replacement, reverify signed release evidence, candidate bytes and the
installed pre-image, check candidate/previous maintenance protocol and schema,
retain the previous executable and a verified database snapshot, and persist a
pending journal. Atomically replace the executable at its stable path. Bounded
version/protocol checks and offline database health checks determine settlement.
A failed check restores the retained executable when its identity and the
unchanged database are proven. A crash leaves pending evidence; explicit recovery
accepts only the recorded before/after executable and healthy compatible database.
Unknown bytes remain blocked. No evidence or previous versions are pruned.

The first implementation requires identical database, configuration and daemon
protocol versions. It never migrates or restores data during activation or binary
rollback. An explicit rollback checks the *current* database; the snapshot remains
available through ADR-0241's separately invoked restore operation. Schema-changing
updates require another compatibility decision and acceptance evidence.

Services retain the stable installed path. Only an absent definition or an exact
JARVIS-owned definition for that path is admissible. Foreign, unreadable or drifted
definitions are refused, including portable use of a service-owned installation.
Activation does not rewrite, start or stop service-manager definitions. Native
service reconciliation/readiness and clean-machine lifecycle remain acceptance
gates; version/protocol and offline checks are not live daemon readiness.

## Consequences

Update records are local integrity evidence, not an adversarial-owner boundary.
Private directories, regular-file checks, bounded reads and process deadlines
defend untrusted release inputs; OS locks coordinate cooperating JARVIS programs.
The installation lock is shared by all profiles of that executable. Another copy
launched manually is a distinct installation, not an OS sandbox escape guarantee.
Interrupted multi-file writes require recovery; no whole-profile or power-loss
transaction is claimed. Native signing/notarization still needs owner credentials.

See [research](../research/integrations/binary-update.md) for contracts and tests.
