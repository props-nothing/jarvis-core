# ADR-0233: Coding cancellation is final and process ownership outlives the root

- Status: Accepted
- Date: 2026-10-16
- Extends: ADR-0222, ADR-0224 and ADR-0232; narrows the process-lifetime limitation recorded in ADR-0041.

## Context

The coding lifecycle review found that an agent exiting or returning an RPC error while cancelling could replace the owner's cancellation with a failure. An authentication-required response could even cause a new prompt after cancellation. Sign-in retry also needed to observe cancellation while waiting.

Process cleanup depended on a root PID, and ordinary command completion disarmed cleanup as soon as that root exited. Descendants could therefore survive their command. On Windows, traversing a dead root with taskkill cannot establish ownership of its former children. Timed-out output-reader tasks also detached instead of being cancelled.

## Decision

Cancellation and an explicit turn deadline are terminal JARVIS decisions. Once selected, an agent's exit, error or late response cannot replace them or authorize authentication and another prompt. Authentication retry remains bounded by the same turn deadline and observes owner cancellation. Persist the resulting cancelled outcome through the ordinary coding-job record.

This finality governs protocol handling. If checked process cleanup fails or root exit cannot be confirmed, the returned outcome is `cleanup_failed` and the durable job is `failed`; verification is skipped and JARVIS does not claim the task stopped. The cleanup failure does not authorize another prompt or authentication attempt.

Own the process lifetime using an operating-system resource created at launch. Windows uses the reviewed, pinned process-wrap 10.0.0 adapter already present in the dependency graph: create suspended, assign the process to a kill-on-close Job Object, then resume. If setup fails, refuse the launch; do not fall back to unowned execution. Keep the job handle until cleanup, including after root exit. Unix retains a separately owned process group. Finish, cancellation, timeout and dropped execution must clean up the owned descendants. Abort output-reader tasks when their bounded collection or enclosing execution ends.

This ownership applies to JARVIS-launched commands, ACP agents and managed project processes. It does not authorize killing a PID loaded from an old record. It is process-lifetime management, not filesystem/network confinement or an OS sandbox. Unix descendants that deliberately escape their process group and abrupt daemon death remain different guarantees from graceful cancellation. No automatic replay, parent/child run schema, coordinated shutdown admission barrier or new overall coding deadline is introduced.

## Verification and limits

Require discriminating ACP cases for exit/error/authentication responses during cancellation and timeout, cancellation during sign-in, and no second prompt after Stop. Verify durable cancelled status through the async coding manager. Process tests must observe real descendant PIDs after root exit, cancellation and timeout; a successful command exit alone is insufficient.

A separate process gate starts the real daemon with a scripted ACP peer in a disposable profile. It verifies ordinary stop refusal, explicit cancellation, forced graceful shutdown, process cleanup, preserved files, interrupted queued/running records after restart, no automatic launch, and freshly approved continuation. Native Windows, macOS and Linux CI runs this gate after building its binaries. This proves JARVIS orchestration, not provider behavior.

A separately enabled real Codex cancellation check uses an isolated project and existing agent authentication. Its result must be recorded separately from scripted acceptance and whole-website delivery. Exact executed checks and outstanding platform/provider gates belong in [TODO](../../TODO.md).

Controlling external contracts and limits: [ACP](../research/integrations/acp.md) and [process lifetime](../research/integrations/process-lifetime.md).
