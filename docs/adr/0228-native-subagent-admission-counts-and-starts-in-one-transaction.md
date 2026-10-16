# ADR-0228: Native subagent admission counts and starts in one transaction

- Status: Accepted
- Date: 2026-10-16
- Scope: native subagent admission and discovery
- Related: [native subagents](0134-a-sub-agent-is-an-ordinary-run-one-level-deep.md)

## Context

Native delegation counted unfinished children only after loading the latest fifty runs of any kind. Enough unrelated activity hid older working or approval-waiting children from both admission and listing. Separate count and start operations also allowed concurrent parents to claim the same remaining slot.

## Decision

The existing run-start transaction accepts an internal, validated capacity policy. Native delegation supplies its durable objective prefix and the existing limits: four working and sixteen unfinished runs. Inside `BEGIN IMMEDIATE`, storage counts every matching unfinished run in the workspace before creating the session, run, message and first event. Approval-waiting children count toward the unfinished limit but not the working admission limit. A full class returns a capacity refusal and writes none of those records.

The objective prefix must match the admitted run. The policy is supplied by the daemon, never by model arguments or the public start request. Ordinary run starts keep their existing behavior. No migration or new infrastructure is introduced.

The subagent list filters the complete workspace history before limiting results and puts unfinished children before recent terminal children. New unrelated runs cannot hide a child that is holding an admission slot.

## Limits

This is admission control, not a scheduler that suspends already-admitted work. Resuming approved children retains existing behavior. Objective prefixes remain the compatibility identity; a durable parent/child relation, propagated parent cancellation and crash-resumed delegation remain separate work. Capacity is workspace-wide, not per parent.

## Verification

Real-store regressions retain working and parked children behind sixty newer unrelated runs, prove capacity refusal writes no session/run/message/event, and race twelve admissions for one remaining slot. Listing keeps the four unfinished children first even after twenty-five newer completed children exceed the twenty-row result limit. A daemon pipeline regression checks that the old children both prevent another delegation and remain visible through `jarvis.agent.list`. Existing parked-child behavior remains covered.
