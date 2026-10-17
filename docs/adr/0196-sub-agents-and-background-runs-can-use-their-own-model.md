# ADR-0196: Sub-agents and background runs can use their own model

Status: Accepted
Date: 2026-10-11

**Later amendment:** [ADR-0234](0234-native-subagents-have-durable-lineage-and-parent-stop.md) replaces native child text identity with persisted lineage, defines parent Stop and restart behavior, and uses the stored role for child model routing. The original decision and evidence below remain historical.

## Context

One model answered everything: the owner's question, a sub-agent's bounded errand, a scheduled check at 03:00 and a run a new mail started. The work differs. A scheduled sweep or a sub-agent that reads one file is often well served by a cheaper (or a faster) model, and the owner's own questions may deserve the strongest one. Autonomy raises the number of background runs, so the cost of using one model for all of them rises with it (`P11-006`). Nothing about this needs a new provider: the models are at the provider already configured.

## Decision

1. **Two optional settings, at the main provider:** `daemon.executor_subagent_model_name` and `daemon.executor_background_model_name` (Settings, Brain). Unset, nothing changes: every run uses `executor_model_name`. They are validated like the fallback model name (a plain identifier, a live provider) and read at start, so a change needs a restart.
2. **The kind of a run is read from how it began** (`model_routing::role_of`): a sub-agent's objective carries the delegation framing; a scheduled run carries the unattended framing; a mail, file or webhook trigger carries its trigger framing; a resumed run keeps the kind it had. Everything else is the owner's own and uses the main model. No run column or migration is needed, and a run keeps one model for its whole life, including after an approval parks and resumes it, because the kind is a function of the stored objective.
3. **Routing chooses a model, never authority.** The tools, policy, approvals, project caps and spend guard are identical for every kind. A forged framing in an objective can only move a run to a model the owner configured for background work.
4. **Usage records the model.** Every `usage_updated` event now carries `model` (the identifier the run asked for), and `jarvis digest` lists calls and tokens per model when more than one was asked. Events written before this have no model and are shown as their own unnamed group, never attributed by guess.
5. **The fallback is unchanged.** When a routed model is rate limited, the fallback model is asked for that call, as before. The usage event names the model the run asked for, not the fallback that may have answered; the fallback is an exception, and a different provider's tokens were never added to this provider's account anyway.

## Consequences

- A cheap model for the 24 scheduled and triggered runs a day, with a strong one kept for the owner's questions, is one setting each.
- A weaker model on unattended work is a quality choice the owner makes; the tools and approvals around it do not weaken. How much a given model degrades on a given task is measured with `jarvis eval`, not assumed.
- Deliberately not built: a model per project, per tool, or chosen by the model itself (routing by task difficulty). Each needs measurements that do not exist yet.
