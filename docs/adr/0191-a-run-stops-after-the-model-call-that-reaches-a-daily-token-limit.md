# ADR-0191: A run stops after the model call that reaches a daily token limit

Status: Accepted
Date: 2026-10-11

## Context

`ADR-0165` limits how many tokens scheduled project work may use, but checks only before a scheduled run starts. A single long run, an interactive run, or a sub-agent can therefore continue spending after the project reaches its daily cap. There is no equivalent limit for work outside a project.

Every billable model call already records a `usage_updated` event. The executor can enforce a limit at that durable boundary without estimating provider-specific tokenization or hiding usage.

## Decision

1. Before a model call and after each `usage_updated` event is persisted, the executor totals input plus output tokens in the rolling 24-hour window. Cached input is not added separately, matching `ADR-0165`.
2. A project run stops with `daily_token_limit` when its project total reaches or exceeds `daily_token_limit`. The call that reaches the cap remains recorded; no later model or tool call starts.
3. The stop is a failed terminal run with a plain explanation. A project stop is also written as a `blocker` journal entry linked to the run.
4. `daemon.workspace_daily_token_limit` is an optional workspace-wide cap. Zero is unlimited. It applies to interactive, scheduled and delegated runs and can be set in Settings, Brain or with `jarvis config set workspace_daily_token_limit N`; a restart applies it.
5. A configured guard fails closed. If its usage or scope cannot be read, the run stops with `token_limit_unavailable` rather than continuing unmetered.

## Consequences

- A model call can overshoot a limit because usage is known only after the provider returns it. That call is billed and counted.
- Concurrent runs can each have a call in flight when another reaches the limit. Each checks again after its own call and stops before further work.
- Limits count tokens from runs that started inside the rolling window, preserving the query semantics introduced by `ADR-0165`.
- This is a token limit, not a price limit. Provider prices are not encoded in JARVIS.
