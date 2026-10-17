# ADR-0238: Shutdown drains owned work before closing dependencies

- Status: Accepted
- Date: 2026-10-17
- Scope: P7-011, graceful drain and delegation failure/audit acceptance
- Amends: [ADR-0233](0233-coding-cancellation-is-final-and-process-ownership-outlives-the-root.md), [ADR-0236](0236-shutdown-reserves-work-admission-before-checking-idleness.md), [ADR-0237](0237-delegation-dispatch-and-family-stop-are-durable.md)

## Context

Closing admission prevents new work but does not settle detached native runs,
coding workers or managed previews. Previously the daemon could close its database
while these workers still needed to record their outcome. Force and OS shutdown
need a bounded, truthful drain, while ordinary stop/restart must retain their busy
refusal. A native failure or external agent crash must retain its own audit and
leave unrelated work usable.

## Decision

After closing the shared admission gate and marking health as stopping, the daemon
keeps transports, database and outbound adapters available while draining work.
The existing `daemon.shutdown_timeout_seconds` setting (default 15, range 1–300)
bounds this phase. A single transaction requests cancellation of every unfinished
native run except `awaiting_approval`, and every queued/running coding task. It
withdraws held arguments only from runs already carrying cancellation intent.
Unlike parent Stop, shutdown preserves durable owner approval waits. Terminal
history is unchanged. Cancellation is repeated to cover publishers that acquired
admission before the gate closed.

The coordinator reads active permits before durable and live-process counts. It
requests cancellation through the existing managed-process registry; workers keep
ownership of process-tree cleanup and outcome persistence. Coding workers observe
durable intent through their existing polling loop. A queued worker denied
admission reports cancellation without starting an agent. Native runs finish an
already-started inference/effect and observe cancellation at their established
boundaries; shutdown does not invent rollback or claim that dropping a future
stopped its external effect.

API-created `received` runs may have no executor. Once the gate is permanently
closed and every permit is released, the coordinator settles only those unstarted
cancelled runs with a terminal cancellation event. It never takes settlement away
from an executing run or a coding worker; an orphaned coding row still times out.

Drain succeeds only when permits, active native runs, queued/running coding tasks
and live managed processes are all zero. Unreadable state fails closed. Managed
cleanup/persistence failures remain sticky after registry removal. A failed coding
settlement carrying cancellation intent and finished since drain began also
prevents success. These conservative checks may report a shutdown failure when the
underlying process has exited but its result cannot be certified.

Only the coordinator is timed out, never an effect future. On timeout or failure,
the daemon records `shutdown-failed` when storage permits, exits unsuccessfully,
and retains unfinished records for conservative startup recovery. No persisted PID
is used for cleanup. Successful cancellation is committed before database close.
Once work drains (or the budget expires), background pollers and transports stop,
MCP connections close, then the database closes. Admission keeps pollers from
starting work during drain; aborting them earlier could drop an in-flight effect.
The existing eight-second HTTP connection grace and adapter teardown are separate
from the work budget. Tokio deadlines are cooperative, not hard real-time bounds.

## Acceptance and limits

Storage tests cover atomic rollback, preserved parked/terminal state, durable intent
on reopen and failed-cleanup evidence. Daemon tests hold permits over a deadline,
await delayed settlement, refuse unreadable state, stop a real managed process tree,
and preserve an actual held approval while a native child cancels. Native audit
acceptance checks child-owned calls, intent/policy/receipt identity, correlated run
events and unchanged audit after reopen. A normalized native model failure leaves
its parent and unrelated run usable; parent failure alone does not cancel a child.

The real daemon gate crashes a scripted ACP process, checks the failed task's
parent, origin call and approved intent, then uses the parent and a fresh approved
coding task. Forced graceful shutdown settles running and queued tasks as cancelled
before any restart. A separate abrupt-kill phase retains interruption/no-replay
coverage. Fixture PID cleanup on Unix is explicit: abrupt daemon death does not
execute a process-group owner's Drop.

Native execution is in-process: normalized adapter failures are contained, but this
is not memory, panic-abort, resource or OS security isolation. ACP crash tests use a
scripted peer, not a live vendor. Escaped Unix descendants and abrupt host death
remain outside graceful-cleanup guarantees (ADR-0233). Duplicate HTTP requests,
exactly-once external effects and automatic worker takeover remain excluded
(ADR-0237). No schema, public protocol or SDK version changes are needed. Native
Windows/macOS/Linux CI and live-provider checks retain their own evidence gates.

Native acceptance: all ten jobs in [CI run 39069495553](https://github.com/props-nothing/jarvis-core/actions/runs/39069495553)
passed for commit `962cf0796bce1695e8cdc8a818d107c7c233e6ee` on 2026-10-17,
including Windows/macOS/Linux tests and process gates. This closes the native
P7-011 gate without changing the live-provider and isolation limits above.
