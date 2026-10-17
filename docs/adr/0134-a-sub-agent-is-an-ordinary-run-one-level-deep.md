# ADR-0134: A sub-agent is an ordinary run, one level deep

Status: Accepted
Date: 2026-10-03

**Amended by:** [ADR-0228](0228-native-subagent-admission-counts-and-starts-in-one-transaction.md) makes admission count/start atomic across the relevant workspace history and prioritizes unfinished children in listing. The limits apply at admission; approval resume is unchanged, and structured parent linkage/cancellation remain separate work.

**Later amendment:** [ADR-0234](0234-native-subagents-have-durable-lineage-and-parent-stop.md) replaces native child text identity with persisted lineage, defines parent Stop and restart behavior, and uses the stored role for child model routing. The original decision and evidence below remain historical.

## Context

One agent in one conversation is the ceiling of a chat window. The product's promise is an assistant that can hand
work to others, several at once, and report on them. The risk of doing that carelessly is a second, privileged way
for work to happen: a delegate that skips approvals, that a model can multiply without bound, or that lets a model
read any run's answer.

## Decision

1. **A sub-agent is started and driven exactly like any run** (`RunService::start`, `execute_run_with_tools`). It gets
   the same policy, approvals, audit and budgets, appears in `jarvis runs` (as `[sub-agent] …`) and stops with
   `jarvis cancel`. A sub-agent that needs an approval **parks**; the parent is told it is waiting for the person,
   who decides it. Nothing is approved on the parent's behalf.
2. **Delegation is one level deep.** A sub-agent's objective begins with a fixed notice, which is how the executor
   knows to offer it no `jarvis.agent.*` tool and to derive its authority without the `agent.delegate` scope, so a
   call to one it names anyway is refused for a missing scope. The marker is durable (it is the stored objective), so
   it survives an approval and a restart, and it can only *reduce* a run's tools.
3. **At most four sub-agents are active at once**, so a model cannot fan out without bound.
4. **Two tools:** `jarvis.agent.delegate` (`write`, risk 1, `Auto`: starting one changes nothing by itself, because
   everything the sub-agent does is gated as usual; `background: true` returns a run id at once so several run in
   parallel) and `jarvis.agent.result` (`read_only`, risk 0; waits at most 60 s).
5. **A sub-agent's answer is untrusted data**, fenced as a fetched page is. `result` reads **only** sub-agents of this
   workspace and refuses any other run, so it is not a way to read an arbitrary run's answer.
6. Composed only when the daemon has an executor (a model to drive a sub-agent with). The adapter holds the pipeline
   through a `Weak`, set after composition, because the pipeline holds the adapter.

## Consequences

- The assistant can split work and run it in parallel; verified live against a real model (two sub-agents, one
  fetching a page, one running code, collected by the parent).
- Cost is multiplied by the number of sub-agents; the cap of four and the per-run model-call budget bound it.
- Not done: the parent does not learn automatically when a parked sub-agent is later approved (it can ask again with
  `result`); no per-sub-agent tool restriction (a sub-agent has the parent's tools minus delegation); sub-agents share
  the parent's model.

## Policy amendment

[ADR-0235](0235-delegation-declares-work-and-its-execution-authority.md) defines the shared delegation effect and execution boundary, and withholds both native and coding management scopes from native children. Broader coordination remains P7-011.
