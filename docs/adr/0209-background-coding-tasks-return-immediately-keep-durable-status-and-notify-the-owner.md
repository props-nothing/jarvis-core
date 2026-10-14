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
5. Completion shows an OS desktop notification. The Coding agent card lists recent durable tasks and their branch or error. The task result remains available through the status tool even if the notification was missed. The console review extension below exposes that same recorded evidence.
6. The existing synchronous tool remains for small jobs and compatibility. Both paths use the same agent, environment, worktree, policy and fenced result implementation.
7. Recording `running` is required before the agent starts. Completion must update exactly one active task before a completion notification is shown; failed durable writes are logged, not treated as success. A settled result cannot be overwritten.

## Consequences

- A model can start a long coding task, continue the conversation, and check later. The owner can close the browser.
- State is durable, process execution is not. A daemon crash leaves an interrupted record and the worktree for manual inspection.
- Desktop notification is local convenience, not delivery proof. Push/Telegram delivery remains open; the console review extension below supplies task details.
- Live evidence 2026-10-13: `delegate_start` returned task `01a136ed-c9bb-75d5-8112-eae6aad47d6c` immediately; the Coding agents API reported `running`, then `completed`; real Codex produced branch `jarvis/in-greet-py-add-a-function-exc-e5ba4dbc` with the requested two-file diff, and `main` remained clean.
- The process manager is intentionally one-at-a-time. Parallel coding agents would multiply spend and git contention; add it only with a measured need.

## Console review extension, 2026-10-14

The existing authenticated `GET /api/v1/code-agents/tasks` response now uses the same task representation as the status/list tools, adding recorded result, worktree, command permission, duration and update time without removing fields or changing execution. The card exposes all twenty tasks the endpoint returns, with native expandable reviews, inert text for agent output and patches, visible errors and truncation, and a read-only refresh that preserves expanded tasks.

An agent's completed turn is not independent verification. The review labels recorded results **Not independently verified** and makes absent results explicit. This adds no test runner, resume, merge, push, cancellation or approval path. No ACP behavior, permissions, persisted schema or external integration changes.

Evidence: an authenticated-router test over real SQLite, three focused UI tests (including hostile markup and failed refresh), both console suites (78 tests), and responsive geometry checks at 1440, 390 and 320 pixels. Formatting and workspace Clippy passed. The unfiltered workspace test gate remains blocked by three unchanged phone-service AI-disclosure tests; the remaining workspace tests pass with those three cases explicitly skipped. No new live-provider verification is claimed.

## Startup cancellation correction, 2026-10-14

Decision 3 describes cancellation of an active prompt. Previously, a Stop issued before the background worker started was only consumed after repository preparation, agent launch and ACP setup. A regression proved that such an agent could write a file before its task was recorded as cancelled. A stalled initialization also ignored Stop until the handshake deadline.

Cancellation is now checked before preparation and at startup effect boundaries. Connection setup races the existing Stop signal with cancellation first. The setup future is pinned and remains alive until the cancellation handler attempts process-tree termination: dropping it first invokes the child's `kill_on_drop`, which can leave descendants alive on Windows before the existing tree guard runs. Already-created worktrees are retained; no task prompt is sent after setup cancellation. Active-prompt ACP cancellation, permissions, risk, authentication and the existing durable task states are unchanged.

Evidence: 18 focused delegation tests passed on Windows, including a queued-stop regression and a synchronized initialization test over a real scripted parent and child. The latter independently queries both PIDs before and after Stop, checks retained worktree state and proves the prompt write did not occur. Disabling the setup tree kill made that test fail with a surviving child; restoring the guard passed. Workspace Clippy passed. Final broader validation is recorded in `TODO.md`.

This is a cancellation reliability slice of `P7-008`, not OS confinement, parent-linked cancellation, independent verification or crash resumption. Real-agent setup/sign-in cancellation and macOS/Linux native cleanup remain unverified. No owner daemon was stopped or restarted, and the phone prompt is unchanged.
