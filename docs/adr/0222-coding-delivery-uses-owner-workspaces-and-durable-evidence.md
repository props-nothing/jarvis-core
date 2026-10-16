# ADR-0222: Coding delivery uses owner workspaces and durable evidence

- Status: Accepted
- Date: 2026-10-15
- Supersedes: the mandatory-worktree, automatic-commit and fixed-total-deadline defaults in ADR-0230/0209; other protocol and ownership decisions remain.

> **Extended 2026-10-16:** [ADR-0232](0232-stop-and-restart-check-background-work-and-fail-closed.md) includes queued/running coding jobs and live managed processes in ordinary stop/restart preflight, and refuses unreadable work state. Explicit force and interrupted-job continuation keep their existing meaning.

## Context

The owner reports coding work appearing in unfamiliar profile checkouts, unnecessary Git operations, missing skill resources and weak website results. The intended default is the first folder permitted in Settings, not JARVIS's source tree. Long tasks must remain stoppable without an arbitrary minute cap. Agent self-report is not delivery evidence.

## Decision

Keep Rust as the durable control plane. Default coding to in-place execution in a canonical Settings-permitted directory; reject ambiguous relative paths. Worktrees remain an explicit opt-in. Do not auto-stage or commit. Both public delegation start names are durable queued jobs; zero means no overall deadline, while protocol operations/cancellation retain finite bounds. Serialize coding jobs and cancel queued jobs before spawn.

Schema 29 retains task/activity ownership, stores the approved request atomically and supports queued work, full briefs and bounded results. The approval payload column is replaced in-place with a byte-bounded 1 MiB column, preserving pending arguments, hashes and foreign keys while removing the previous hidden 8 KiB brief blocker. Decisions still bind exact canonical arguments; denial/execution clear held payloads. Restart interrupts rather than replays. Explicit continuation is a newly approved in-place job, not ACP session restoration.

Separate Agent Skills bundles from native learned procedures. Install complete local or immutable public GitHub bundles with strict resource/path limits and provenance. Activation injects selected manifests; it does not execute setup scripts or grant capabilities. Keep project dependency installation as approved host execution, not a special unrestricted downloader. Public GitHub uses versioned REST contracts, no forwarded credentials or redirects.

Managed project processes have explicit start/status/list/stop, bounded log capture, durable completion records and no lifetime deadline. They receive a minimal toolchain environment. A stale record never authorizes signalling a PID or restarting a process. Loopback reachability is not ownership, HTTP, visual or interaction verification; report it separately and require real browser review.

Independent checks run after agent completion; failed checks fail delivery and missing checks remain unverified. Parent orchestration must inspect desktop/mobile behavior instead of declaring success from an agent summary. Preserve valid bounded JSON and disclose omitted detail.

## Consequences and boundaries

This intentionally grants normal host development capability under owner approval, not OS confinement. Cooperative path/network decisions cannot constrain an agent's native tools or an approved executable's whole-host effects. Package scripts remain code; account credentials and external quotas remain real blockers. No editor-token sharing, automatic global installation, publishing, scoped agent-to-JARVIS MCP bridge or automatic browser verification is implied.

Handshake, HTTP, file/output sizes and per-check deadlines remain resource/recovery guards, not a total delegation budget. Local project catalogs and public GitHub bundles are supported; global/private catalogs and installation updates need explicit follow-up design. Windows evidence does not replace native macOS/Linux acceptance.

## Verification

The 2026-10-16 release repair restores the shared resolver's support for canonical absolute directories retained by queued tasks and continuation, with current granted-root checks reapplied before execution. New regressions refuse outside directories and ambiguous relative matches, while duplicate grants resolving to the same directory remain unambiguous. See the [release repair ledger](../history/todo-owner-evidence-through-2026-10-16.md#owner-requested-release-build-repair-2026-10-16) for focused and workspace results.

Regression coverage must demonstrate non-Git/dirty-folder in-place editing, no automatic commit, ambiguous/outside roots refused, no omitted-deadline expiry, startup/queued cancellation, request recovery, schema-28 task/activity retention and full-brief approval/denial semantics. Skill resources, hostile paths/links/frontmatter and oversized reports must fail closed. Real managed HTTP preview readiness must remain browser-unverified. Public GitHub download is an explicit ignored network smoke into disposable storage, not package/script execution or owner-account acceptance.

See [user workflow](../user/code-agents.md) and [versioned research](../research/integrations/coding-delivery.md).


## Recorded Windows evidence (2026-10-15)

- Coding delegation: 25 focused regressions passed, including Settings-first non-Git execution, explicit worktree behavior, permission framing, cancellation, queued jobs and independent failed-check reporting.
- Startup cancellation observes parent/child exit within five seconds rather than asserting immediate process-list disappearance under concurrent load. Removing pre-drop process-tree cleanup makes this bounded regression fail; the intact cleanup passes.
- Skill bundles: 8 offline tests passed; the separately enabled public GitHub smoke passed discovery and immutable installation into disposable storage without running scripts.
- Managed processes: 5 tests passed, including a real Node HTTP preview, actual response-body fetch, busy-port refusal and stop/port closure. This is not desktop/mobile browser acceptance.
- Commands: 18 focused tests passed; ACP/worktree runtimes: 34 passed; approval regressions: 25 passed. The actual schema-28-to-29 migration preserves task/activity and pending approval bindings, and the legacy upgrade regression passed.
- Coding Settings: 7 focused Node tests passed. Full unchanged-HEAD Settings source/tests reproduce 32 failures (51 passed of 83); those pre-existing failures are not a coding-delivery regression or a passing UI gate.
- Final settled Rust gates passed on Windows: `cargo build --workspace --all-features --locked`; `cargo fmt --all --check`; `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings`; `cargo test --workspace --no-fail-fast --locked` (**3,092 passed, 0 failed, 4 ignored** across 61 result summaries). This includes the concurrent memory changes and the bounded cancellation regression. Four coding documents have 11 checked local links, none missing.
- No real owner-agent website, dependency-install workflow, desktop/mobile visual acceptance or native macOS/Linux lifecycle run is claimed. The shared owner daemon was not restarted by this slice; compiled code is not evidence of activation.
