# ADR-0235: Delegation declares work and its execution authority

- Status: Accepted
- Date: 2026-10-17
- Scope: P7-010, canonical tool effects and native/coding policy
- Amends: [ADR-0134](0134-a-sub-agent-is-an-ordinary-run-one-level-deep.md), [ADR-0234](0234-native-subagents-have-durable-lineage-and-parent-stop.md)
- Preserves: [ADR-0222](0222-coding-delivery-uses-owner-workspaces-and-durable-evidence.md)

## Context

Native delegation declared only a write. Coding delegation declared code execution,
with the write effect missing from its start and continuation tools. This obscured
their common effect and made their different approval defaults look arbitrary.
Native children also lost the native management scope but retained the coding
management scope, allowing a second delegation path despite the one-level rule.

## Decision

Add the canonical `delegation` effect: starting independent work is mutating and
has a minimum risk of 1. It does not grant authority over subsequent effects.
`DelegationBoundary` in `jarvis-tools` defines the two supported contracts:

| Boundary | Effects | Risk floor | Default approval |
| --- | --- | --- | --- |
| Native run | write, delegation | 1 | auto, subject to workspace policy |
| Host process | write, delegation, code_execution | 3 | ask, subject to explicit owner trust |

Native `jarvis.agent.delegate` and coding `jarvis.code.delegate`,
`delegate_start` and `delegate_resume` use these contracts. Their tool version is
1.1.0. Inspection/wait tools remain read-only; coding Stop remains a risk-1 write.
They do not launch work and therefore do not declare delegation or code execution.
The new effect is an additive wire name. Existing persisted receipts and tool
versions are retained; no storage migration or rewriting of history is needed.
The existing receipt/version binding refuses a held 1.0.0 launch against a 1.1.0
definition. Inspect and cancel the old request before requesting the remaining
work again; approval for one version is not silently applied to another.

All launches use ordinary schema validation, actor grants, policy, approval where
required, and durable native-call accounting. Native admission does not approve
its child's later calls: those re-enter the ordinary pipeline. Coding task
authorization permits the external agent's host execution. Its native tools are
not all intercepted by JARVIS. Worktrees, task wording and cooperative command
settings do not lower that risk floor or establish OS confinement.

Explicit owner trust remains per tool ID. Trusting native delegation cannot
release coding delegation; trusting coding start cannot release continuation.
Trust never bypasses missing scopes, workspace denial, tool denial or a risk
ceiling. Workspace policy can require approval for native admission too. No new
approval ceremony or authority level is introduced.

Both launch contracts remain non-idempotent and declare no automatic retries.
An uncertain launch is not safe to repeat just because it is called delegation.
The existing effect-aware retry validator rejects blind retries without a
deduplication contract. This decision does not implement duplicate dispatch
prevention or claim exactly-once execution.

Native children, including legacy children identified by stored role, are offered
neither native nor coding management tools. Their actor also lacks both management
scopes, so naming a hidden tool is refused before an approval or adapter call.
The pipeline independently reads stored role for any delegation effect, both on
direct calls and on approval resume. Full local-surface grants and an earlier
approval cannot elevate a child. Unreadable/missing run state fails closed.
The entire management family is withheld because its inspection and Stop tools
share its launch scope. Ordinary child tools keep their existing policy. This is
a restriction on the JARVIS tool surface, not confinement of external processes.

## Verification and limits

Tests use the actual native and coding definitions to check all four launches,
effect serialization, floors, approval defaults, per-ID trust, denial, grants and
retry refusal. Real executor/pipeline/storage tests prove coding launch and
continuation park without reaching an adapter, even with cooperative commands
disabled. A child that directly names each trusted coding launch is refused,
creates no approval and never reaches the probe adapter. The role restriction is
also tested with full direct-call grants and a decided approval seeded through
the historical coding catalog. Existing native child
creation, lineage, approval and restart tests continue to exercise ordinary runs.

This changes internal JARVIS policy metadata and scope selection only; no ACP wire
operation, provider SDK, external runtime invocation or dependency is changed.
Fixture tests establish policy behavior, not new live-provider acceptance.
Native parent Stop and coding Stop retain their separate lifecycle contracts.
Cross-runtime cancellation, broader concurrency/duplicate-dispatch coordination
and shutdown draining remain P7-011; the scoped ACP bridge and sandbox remain
P7-003.

## Cross-runtime coordination amendment

[ADR-0237](0237-delegation-dispatch-and-family-stop-are-durable.md) adds durable coding ownership and Stop, per-call dispatch identity for both runtimes, and restart adoption without automatic relaunch. Earlier remaining-work notes above describe the preceding slice; broader audit, isolation and shutdown draining acceptance remain open.
