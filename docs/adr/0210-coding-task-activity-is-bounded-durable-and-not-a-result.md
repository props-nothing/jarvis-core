# ADR-0210: Coding-task activity is bounded, durable and not a result

Status: Accepted
Date: 2026-10-14

## Context

ACP v1 tool updates already reach the adapter but were visible only in the final report. A background task can outlive the ordinary run that launched it, so its progress cannot be appended to that run after settlement. Reusing `result_json` for unfinished work would also make progress look like a final result.

## Decision

- Keep the existing v1 adapter and add a bounded non-blocking observer for normalized public tool snapshots. Merge partial updates by tool identifier within the active session. Exclude thought chunks, thinking-tool metadata, raw inputs/outputs, tool content, messages and authentication data from this progress path.
- Record background activity separately in `delegated_task_activity` (SQLite schema 27), with a positive increasing sequence, bounded summary, timestamp and composite task/workspace foreign key. Only running tasks accept a newer sequence. Activity never settles or reopens a task.
- Retain at most the latest 128 sequence positions and return at most 32 entries in order. Gaps and omitted history are disclosed. These are bounded observations, not a complete immutable audit or independently verified tool outcomes.
- The authenticated console task view reads persisted activity. Model-facing status/list output fences agent-provided summaries as untrusted data. The console uses text nodes, never markup.
- Automatically refresh visible running task reviews without starting an agent, widening permissions, disturbing focus or replacing open disclosures. The existing read-only refresh remains available. Stop background refresh when the card is no longer active.
- No parent/child cancellation, process recovery, independent test runner, merge, new provider or phone change is included. Existing sign-in, policy and task outcomes remain unchanged.

## Verification

The adapter must emit activity before a prompt ends, preserve omitted metadata, ignore internal reasoning and other sessions, and remain cancellable with a full observer queue. Storage tests prove workspace isolation, sequence and terminal-state guards, bounded retention and persistence across reopening/interruption. Daemon and UI tests must observe activity while a fake agent is still running, preserve it after completion, and keep updates inert and accessible. New live-provider verification remains separate evidence.