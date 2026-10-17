# ADR-0237: Delegation dispatch and family Stop are durable

- Status: Accepted
- Date: 2026-10-17
- Scope: P7-011, native and coding task coordination
- Amends: [ADR-0233](0233-coding-cancellation-is-final-and-process-ownership-outlives-the-root.md), [ADR-0234](0234-native-subagents-have-durable-lineage-and-parent-stop.md), [ADR-0235](0235-delegation-declares-work-and-its-execution-authority.md), [ADR-0236](0236-shutdown-reserves-work-admission-before-checking-idleness.md)

## Context

Native children had durable parents, but coding jobs had no owning run and Stop
relied on an in-memory notification. Re-entering a delegation adapter with the
same submitted tool call could also create a second task. A crash between storing
a task and launching its worker must not be repaired by blindly launching again.

## Decision

SQLite migration 0031 stores an immutable, unique `origin_call_id` on new native
children and coding jobs. Coding jobs also store immutable `parent_run_id` and
durable `cancellation_requested_at`. Admission uses `BEGIN IMMEDIATE` to adopt an
existing dispatch or insert one new task. Only the insertion winner may spawn a
worker. A replay returns the saved identity, including after Stop or restart; it
does not reacquire execution authority. Changed native objectives or saved coding
requests for that call are refused. Different deliberately admitted tool calls
remain separate work even when their arguments match.

New coding dispatch resolves the parent from the persisted submitted launch call,
checks its workspace and root role, and refuses stopped families. Native dispatch
also checks its submitted call and retains the existing active-parent, one-level
and capacity rules. An uncancelled completed or interrupted root still supports
explicit approved coding calls, as the existing recovery flow requires. Neither
adapter accepts model-supplied parent identity. Direct inbound MCP calls without
a durable run/tool-call row cannot launch or resume coding jobs; durable remote
call history remains separate work. No new approval interaction is introduced.

Parent Stop records intent on unfinished native descendants and linked queued or
running coding jobs in one transaction. It also withdraws held family approvals.
The `run_family_stops` table preserves intent without changing a completed
parent's outcome, version or timestamps. It prevents a late submitted coding
launch from entering that stopped family. A fully settled family still refuses
Stop. Ordinary parent completion or failure does not stop background children.
Stopping one coding task remains local to that task, preserving siblings and the
parent. Repeated requests preserve the first cancellation timestamp.

Coding Stop writes intent before sending an optional fast in-memory hint. Workers
poll the row every 100 milliseconds while retaining the existing runtime future;
they signal its established cancellation path and await its process cleanup.
Unreadable cancellation state requests cancellation rather than permitting
continued work. Queued-to-running admission checks cancellation atomically, so a
Stop that wins that write prevents launch. Work already admitted may begin before
the worker observes Stop and must be cancelled through the existing driver.
This is not a hard 100-millisecond cleanup guarantee.

If cancellation wins before terminal settlement, status and result outcome stay
cancelled when the runtime returns an ordinary terminal result. Explicit cleanup
failure or an unavailable execution result stays failed with its evidence and
cancellation intent retained: Stop alone cannot certify process exit. Saved files
and verification evidence are retained. Once terminal settlement wins, later Stop
does not rewrite the result. On restart, requested jobs settle as cancelled and
other unfinished jobs as interrupted; neither is automatically launched again.
Native restart continuation excludes parents with recorded coding jobs as well
as native children. An explicit continuation is a fresh approved tool call and
task identity, with the existing inspect-before-repeating instructions.

Legacy coding jobs and native dispatches retain unknown origin; provenance is
never inferred from text. Legacy coding jobs remain individually stoppable. The
normal verified pre-migration backup is retained. Coding task views expose parent,
origin and cancellation timestamp as additive fields.

## Verification and limits

Storage tests cover concurrent duplicate admission, Stop/admission races, foreign
or invalid origins, immutable lineage, cancelled settlement, task-local Stop,
reopen/no replay, parent continuation exclusion and the schema-30 upgrade.
Adapter tests cover native call adoption, one coding worker, queued cancellation
and Stop through a manager that has no local worker notification. The coding
lifecycle process gate exercises real daemon parent Stop on running and queued
jobs and checks process-tree exit and retained files using a scripted ACP peer.

This is at-most-one dispatch per durable call, not exactly-once execution of
external effects or deduplication of separate HTTP requests. A crash after insert
but before spawn can leave work unexecuted; recovery reports interruption. The
single-profile daemon lock remains the process ownership boundary. No distributed
lease, automatic takeover, provider protocol change or new runtime is added.
The ADR-0222 host-execution boundary remains: a worktree is not an OS sandbox and
native ACP tools are not all intercepted. Broader child-audit/crash-isolation
acceptance, coordinated shutdown draining and provider/platform acceptance remain
separate P7-011 work.

Amended by [ADR-0238](0238-shutdown-drains-owned-work-before-closing-dependencies.md) for coordinated graceful shutdown draining and delegation failure/audit acceptance.
