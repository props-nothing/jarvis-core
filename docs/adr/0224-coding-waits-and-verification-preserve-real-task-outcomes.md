# ADR-0224: Coding waits and verification preserve real task outcomes

- Status: Accepted
- Date: 2026-10-16
- Extends: ADR-0222; selected by the owner after reviewing recent delegation failures.

## Context

Recent coding agents were explicitly cancelled during setup despite the absence of an overall deadline. Content-only ACP updates filled the activity queue, task listings exceeded model output limits, and an exit-zero build was described as clean despite missing lint tooling. The pinned Codex runtime's default reviewer and offline sandbox complicated authorized dependency installation.

## Decision

Add the read-only `jarvis.code.delegate_wait` contract: wait 1–30 seconds, returning early on a terminal state. A wait timeout reports the still-active durable job and never cancels it. The executor resets its repeated-call counter only after this native tool actually returns an active wait timeout. Errors, completed jobs, other tools and total run budgets retain their guards. Owner Stop remains immediate. Orchestration instructions require allowing recovery and prohibit taking over active coding work solely because it is quiet or one command failed.

For the recognized Codex ACP preset, select the documented `workspace-write` mode for authorized commands, or `read-only` otherwise. Route runtime permission requests through the existing Rust policy. Set task-specific npm offline configuration and explain explicit online/project-local-cache recovery. Never select full access, replace global configuration or copy credentials to make an install succeed. This does not create OS confinement or guarantee network access for other runtimes. Research and pinned versions are recorded in [coding-delivery.md](../research/integrations/coding-delivery.md).

Deduplicate unchanged public ACP tool metadata without losing partial-update fields or changed status. Listing returns compact identities/statuses; bounded status summaries retain independent verification while full reports remain in the console. Large durable results retain delivery classification and disclose omitted detail.

Separate lifecycle completion from delivery: `checks_passed`, `verification_failed` or `unverified`, with `browser_verified: false`. Auto-detect build, lint and typecheck scripts. Independent checks require an exit-zero completed command, complete output capture and no recognized explicit check-skip diagnostics. Missing direct Node verification scripts fail preflight. Record known missing-ESLint/skipped-lint/typecheck messages separately from process exit status; do not classify arbitrary warnings as failures. This is conservative evidence for listed commands, not a general proof of application correctness or a parser for every build tool.

Screenshot destinations supplied to the pinned browser adapter must be absolute and contain no parent traversal; preserve the authorized arguments unchanged. The brief directs artifacts into the selected project. The absolute-path guard prevents dependence on server cwd; it does not independently enforce project-root confinement.

## Verification and limits

Regression tests cover quiet-agent waits and explicit cancellation, repeated-wait handling, duplicate progress bursts, fifty long task listings, large Unicode reports, exit-zero missing lint, missing browser scripts, permission-mode selection and screenshot refusal before forwarding. A separately enabled real Codex ACP 2.2.2 run installed `is-number@7.0.0` into a disposable project and passed JARVIS's independent Node import/behavior check, preserving the supplied package and check files (100 seconds on Windows).

Full gate results are recorded in [TODO.md](../history/todo-owner-evidence-through-2026-10-16.md#owner-requested-delegation-repair-2026-10-16). Native macOS/Linux behavior and a full owner website acceptance run remain separate. This change does not automatically verify browser behavior or activate a new daemon binary.
