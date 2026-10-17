# ADR-0236: Shutdown reserves work admission before checking idleness

- Status: Accepted
- Date: 2026-10-17
- Scope: P7-011, local daemon shutdown admission
- Amends: [ADR-0232](0232-stop-and-restart-check-background-work-and-fail-closed.md)

## Context

The stop/restart preflight counted durable native runs, queued/running coding jobs
and managed processes. New work could arrive after that snapshot. Direct remote
tool calls also have no durable run call row, and a process launch can be underway
before its managed-process registration becomes visible.

## Decision

`jarvis-core::WorkAdmission` defines a process-local gate shared by every service
through the daemon's single `Arc<SqliteDatabase>`. The existing single-instance
lock still owns the profile. This gate is neither a SQL lock nor a cross-process
lease. A newly opened daemon incarnation gets an open gate; task state, approvals,
audit and recovery remain in SQLite without a schema migration.

An operation holds an RAII permit through admission/publication or execution:

- Native run insertion holds a permit through its transaction; native execution
  holds one until completion or a durable approval wait, including approval resume.
- Coding insertion holds one through publication of the queued row. Dispatch
  acquires another **after** waiting for agent capacity and keeps it through the
  job. A queued job rejected at dispatch starts no agent and is recorded as failed
  if settlement completes; startup recovery handles rows left by process exit.
- Ordinary tool calls, approval resumes and direct remote calls hold permits
  through dispatch and outcome recording. This covers managed process launch up
  to registry publication and remote calls without a durable ledger row.
- Schedule/trigger passes and webhook admission hold permits before claims so
  an ordinary accepted stop cannot consume a new delivery without starting it.
  Existing crash/forced-stop claim-to-run gaps remain; this is not exactly-once
  delivery.

Ordinary shutdown first checks active permits atomically. If busy, it leaves
admission open, so a refusal cannot disrupt nested work. Otherwise it reserves
the gate, blocking new admissions while reading durable counts and the process
registry. Refusal, read failure or cancellation drops the reservation and reopens
admission. Success commits it closed **before** returning HTTP 202. Force bypasses
active-work checks but closes admission as well. A poisoned gate fails closed.
OS signals and listener exit close the gate before transport teardown.

Settings restart holds the reservation through helper launch. The synchronous
launch result and commit share the gate lock: the CLI helper cannot observe a
partially published restart. Launch failure reopens admission; success leaves it
closed. Repeated Stop acknowledges the committed shutdown so the helper can use
the normal CLI stop path. Repeated Settings restart refuses without launching
another helper. A helper that starts but later fails retains the existing launch
failure limitation: the owner can explicitly stop the old process and start it
again; no automatic reopen risks admitting work during an uncertain shutdown.

Active permits return HTTP 409 with the existing `runs_in_flight` code,
`in_flight`, the existing per-kind work counts and additive `active_operations`.
The total includes both records and permits, which can overlap; it is not a count
of unique tasks. With active permits the snapshot is used only to explain a
refusal, never to authorize shutdown. An overlapping shutdown check returns `shutdown_in_progress`. New native
starts return HTTP 503; tool launch/resume paths return `daemon_stopping` without
creating a new call or advancing a held call's execution state. Query and durable decision
routes remain usable until listener shutdown. No new owner approval step exists.

## Verification and limits

Tests exercise competing admission/shutdown threads, cancelled reservations,
poisoning, forced shutdown, restart launch publication/failure, blocked native and
coding inserts, remote execution without a call row, fresh/remote/approved tool
calls, parked native resume, and queued coding dispatch after capacity release.
The Windows workspace and native process gates pass stop/restart integration.
Removing the closed-admission predicate fails three domain tests; it was restored
before final validation.

This prevents **new admissions** after shutdown acceptance. Force and OS signals
may interrupt operations admitted earlier; this does not promise to wait for,
cancel or roll back every effect they have already begun. Coordinated cancellation
across native/coding families, broader duplicate-dispatch/audit/isolation
acceptance and other P7-011 work remain open. ACP host execution remains the
ADR-0222 boundary, not an OS sandbox. No external API, dependency, provider
contract or live-provider acceptance changes here.
