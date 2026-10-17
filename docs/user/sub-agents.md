# Native sub-agents

JARVIS can give a bounded task to a native sub-agent while continuing the parent
conversation. Each child has its own conversation, permissions, approvals and
durable run ID. It can use ordinary tools, but cannot delegate again.

Starting a native child normally needs no separate approval. Your workspace
policy can require one or refuse delegation. Each later child tool still follows
its own permission and approval rules. A child cannot launch or manage another
native or coding agent, even if you have trusted a coding launch tool. See
[the shared delegation policy](../adr/0235-delegation-declares-work-and-its-execution-authority.md).

After upgrading to delegation tool version 1.1.0, a pending launch approved for
1.0.0 must be requested again. Inspect and cancel the old request first so a new
attempt covers only remaining work. Existing child IDs and results are retained.

Use `jarvis runs` to see recent children and their parent IDs, or
`jarvis runs --json` for the structured `parent_run_id` and `is_delegated`
fields. JARVIS can also inspect them through `jarvis.agent.list` and
`jarvis.agent.result`. Older children created before lineage was stored show an
unknown parent; JARVIS does not guess one. Stop those legacy children by their own
run IDs.

A parent can finish while a background child continues. To stop the parent and
its unfinished native children, use `jarvis cancel <parent-run-id>`. This also
works when the parent has already finished but its children are still working.
Stopping a child by its own ID leaves its parent and siblings alone. Child
approvals are withdrawn on Stop. Work already submitted to an external service
may finish and keeps its actual outcome.

After a daemon restart, interrupted work is reported as interrupted or cancelled,
and approval waits remain available with the same child identity. JARVIS does not
automatically replay children or restart a parent that already delegated work.
Inspect the saved results and pending approvals before asking for a new attempt.

These rules cover native sub-agents. External [coding agents](code-agents.md)
have their own task IDs and Stop controls. The durable lifecycle is specified in
[ADR-0234](../adr/0234-native-subagents-have-durable-lineage-and-parent-stop.md).
