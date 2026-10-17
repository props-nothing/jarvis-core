# ADR-0234: Native subagents have durable lineage and parent Stop

- Status: Accepted
- Date: 2026-10-17
- Scope: P7-009, native run identity, cancellation, restart and read protocols
- Amends: [ADR-0134](0134-a-sub-agent-is-an-ordinary-run-one-level-deep.md), [ADR-0228](0228-native-subagent-admission-counts-and-starts-in-one-transaction.md), [ADR-0155](0155-restarts-keep-work-the-owner-hears-and-leads-live-in-rows.md), [ADR-0196](0196-sub-agents-and-background-runs-can-use-their-own-model.md)

## Context

Native children were identified by their objective prefix. That persisted a role but
could not identify a parent, support parent Stop or distinguish new user text from
an actual delegation. A parent continuation after restart could delegate the same
work again. Approval release also needed to serialize its final admission against
durable cancellation intent.

## Decision

SQLite migration 0030 adds immutable `parent_run_id` and `delegation_kind`
(`none`, `native`, `legacy`) to `agent_runs`. A native child has a parent
foreign key; roots and legacy children have no parent. The migration recognizes
the exact historical prefix once and retains those rows as restricted legacy
children, without guessing a parent. Its newline comparison is independent of
checkout line endings. Existing profile backup and schema-version checks apply.
Deletion of a referenced parent is refused; lifecycle settlement does not delete
lineage.

The delegate adapter derives the parent from its persisted tool call, never from
model arguments or the public start request. Parent validation, workspace-wide
capacity counting, child session/run creation, first event and input transcript
share the existing immediate write transaction. The parent must be in the same
workspace and owned by the same user, nonterminal, uncancelled and a root. Children
start in separate sessions. The existing limits remain four working and sixteen
unfinished children, including migrated legacy children; approval waits occupy
only unfinished slots.

Stored role drives agent-tool visibility, scope reduction, result/list eligibility,
project context and sub-agent model routing. Storage independently rejects deeper
delegation. The task prefix remains model guidance and presentation text only.
Additive `parent_run_id` and `is_delegated` fields expose lineage in run
details/listing. Native agent tool results include the parent, and CLI run listings
show it or explicitly identify an unknown legacy parent. Child discovery and
result access remain workspace-wide, as before.

## Lifecycle

- Admission creates an independent ordinary run. Parent completion or failure does
  not implicitly stop admitted children; background and approval-waiting work can
  outlive the parent. No automatic join or result delivery is introduced.
- Explicit parent Stop records cancellation intent on the parent if live and all
  unfinished native children in one immediate transaction. The same transaction
  withdraws their pending approvals and clears held approved arguments. Completed
  child outcomes and unrelated runs are unchanged. Stopping a child is local.
- A terminal parent with unfinished children remains a valid Stop target. Its
  outcome and version stay unchanged. A family with no unfinished work refuses
  Stop. Repeated requests preserve each run's first cancellation timestamp.
- Drivers honor intent at their boundaries; parked children are settled with a
  cancellation event because they have no driver. The run service settles a
  directly stopped parked target as before. A crash between intent and parked
  settlement is completed by startup recovery.
- The guarded tool-call transitions to `authorized` and `submitted` require
  an uncancelled owning run. The existing explicit approval-resume contract for
  direct calls on interrupted runs remains available. An effect whose submission
  wins before Stop is in flight and can report its true outcome. Stop cannot retract an external
  effect already handed off; reporting that outcome remains allowed.
- On restart, in-flight runs fail with the existing interruption code unless
  cancellation was requested, in which case they settle cancelled. Uncancelled
  approval waits survive with the same child identity and restrictions, even if
  the parent was interrupted. Children and parents with recorded children are
  excluded from automatic restart continuation to avoid duplicating delegated
  work. An owner can inspect, approve or stop remaining work; no child is replayed.

## Verification and boundaries

Regression tests cover v29 migration and backup, full/legacy objective storage,
reopen, immutable lineage, text spoofing, same-workspace/user checks, missing,
terminal, stopped and child parents, admission versus Stop races, child-local
Stop, terminal-parent Stop, parked approvals, preserved in-flight outcomes and
restart without family replay. Real pipeline tests verify child linkage, scoped
tool refusal and bounded result discovery. The process-level Phase 2 gate seeds
a real storage boundary, restarts the daemon, checks lineage through HTTP and
uses the CLI to stop a completed parent older than the recent-run search page.
The Phase 3 gate preserves owner-approved direct-call recovery after restart.

This decision concerns native sub-agents whose identity previously depended on
text. External coding jobs retain their own durable lifecycle and explicit Stop
under ADR-0233. Shared native/coding delegation effects and policy are P7-010;
broader cancellation coordination, duplicate dispatch and shutdown draining remain
P7-011. No provider integration, OS sandbox, new approval step or automatic
cross-runtime child cancellation is claimed.

## Policy amendment

[ADR-0235](0235-delegation-declares-work-and-its-execution-authority.md) defines the shared delegation effect and execution boundary, and withholds both native and coding management scopes from native children. Broader coordination remains P7-011.

## Cross-runtime coordination amendment

[ADR-0237](0237-delegation-dispatch-and-family-stop-are-durable.md) adds durable coding ownership and Stop, per-call dispatch identity for both runtimes, and restart adoption without automatic relaunch. Earlier remaining-work notes above describe the preceding slice; broader audit, isolation and shutdown draining acceptance remain open.
