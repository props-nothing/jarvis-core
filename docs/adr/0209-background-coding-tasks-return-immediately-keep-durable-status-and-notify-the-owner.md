# ADR-0209: Background coding tasks return immediately, keep durable status and notify the owner

Status: Accepted
Date: 2026-10-13

## Context

`jarvis.code.delegate` waits for an ACP coding agent inside one tool call, whose platform limit is ten minutes (the task itself was capped at seven). Real coding jobs take longer and the owner should not need an open chat while the agent works. The result, branch and reason for a failure must survive a daemon restart, but restarting an agent automatically after a crash could repeat edits or commands.

## Decision

1. Four tools beside the synchronous one: `jarvis.code.delegate_start` (risk 3, asks once, starts a task of 1–60 minutes and returns a durable task id immediately), `delegate_status`, `delegate_list`, and `delegate_stop`. Status/list are read-only; stop is risk 1 and keeps the worktree and branch.
2. Migration 26 stores the approved intent, status (`queued`, `running`, `completed`, `failed`, `cancelled`, `interrupted`), branch, worktree, bounded result/error and timestamps. A partial unique index permits only one queued/running task per workspace.
3. The daemon owns the background future and a cancellation `Notify`. Cancellation goes through ACP `session/cancel`, then the existing grace period and process-tree kill. The worktree is always finished into a branch where possible.
4. On daemon start, queued/running rows from the prior daemon become `interrupted`; they are never silently restarted. The child belonged to the prior process and its outcome is unknown.
5. Completion shows an OS desktop notification. The Coding agent card lists the latest ten durable tasks and their branch or error. The task result remains available through the status tool even if the notification was missed.
6. The existing synchronous tool remains for small jobs and compatibility. Both paths use the same agent, environment, worktree, policy and fenced result implementation.
7. Recording `running` is required before the agent starts. Completion must update exactly one active task before a completion notification is shown; failed durable writes are logged, not treated as success. A settled result cannot be overwritten.

## Consequences

- A model can start a long coding task, continue the conversation, and check later. The owner can close the browser.
- State is durable, process execution is not. A daemon crash leaves an interrupted record and the worktree for manual inspection.
- Desktop notification is local convenience, not delivery proof. Push/Telegram delivery and a dedicated task detail panel remain open.
- Live evidence 2026-10-13: `delegate_start` returned task `01a136ed-c9bb-75d5-8112-eae6aad47d6c` immediately; the Coding agents API reported `running`, then `completed`; real Codex produced branch `jarvis/in-greet-py-add-a-function-exc-e5ba4dbc` with the requested two-file diff, and `main` remained clean.
- The process manager is intentionally one-at-a-time. Parallel coding agents would multiply spend and git contention; add it only with a measured need.
