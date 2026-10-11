# ADR-0187: Fixes from the owner's capability audit

Status: Accepted
Date: 2026-10-11

## Context

The owner had JARVIS audit its own tools for seven cycles (`ai-capability-audit`, scored and logged by JARVIS itself against real mail, a real browser, real schedules). Most findings were wishes, now in `docs/product/capability-wishlist.md`. Six were defects with real causes, found by reproducing each.

## Decision

1. **A failed MCP call is no longer `unknown`.** `bounded_reason` built the failure record from the server's text, but the outcome recorder (`SafeMessage`) refuses control characters and counts **bytes**. Playwright's errors have line breaks, so `ToolOutcomeRecord::failed` returned an error and the adapter fell back to `undescribable`, an `unknown` outcome with no reason. The reason is now whitespace-collapsed, control-free and bounded in bytes. The model reads the real error from `browser_file_upload` and `browser_drop` (the upload failure itself was never visible; its cause is now whatever Playwright says).
2. **`jarvis.tools.search` also names tools that are already offered.** It searched only held-back MCP groups, so a query about Gmail answered "nothing matched". Matching native tools and small servers come back as `already_available`, and the note says no search is needed for them.
3. **`jarvis.agent.list`** (read-only, risk 0): sub-agents with run id, state, start and the first words of the task (fenced). A refused delegation points to it. **A sub-agent parked for an approval holds no working slot** (it uses nothing); a hard cap of 16 unsettled sub-agents remains.
4. **`memory.correct` says its limit.** A memory's de-duplication key holds 32 different words; the limit is on the *new* claim, but the refusal read "a search key must be 1 to 32 words". The schema now says it and the tool refuses in plain words before the owner is asked.
5. **The command tool inherits `COMPUTERNAME` and `USERDOMAIN`.** Without them `shutdown.exe` (and other programs that ask who the machine is) failed with error 203. Neither is a secret.
6. **`npx skills find` exit `-1073740791`**: reproduced to its cause with a scratch program: the `skills` CLI (1.7.2) exits immediately when telemetry is off (`DO_NOT_TRACK` or `DISABLE_TELEMETRY`) while a handle is closing, a libuv assertion on Windows; the output is complete. JARVIS sets `DO_NOT_TRACK` for every command on purpose, so the default stays and the result gains a `note` when, and only when, the exit code, the `UV_HANDLE_CLOSING` assertion and some output are all present.

## Consequences

Tests: the multi-line, multi-byte reason is recordable and keeps its words; `tools.search` names the native tools and still opens a held-back group; `agent.list` lists sub-agents and nothing of an ordinary run; parked sub-agents do not block new work; a full house points to the list; the 32-word rule is said in words and 32 different words (repeats not counted) is accepted; the child environment holds `COMPUTERNAME`; the Node-crash note needs its whole signature. Run live with a real model and the real command tool: `shutdown /?` printed its usage instead of error 203; `shutdown /a` returned its own message (1116, no shutdown pending); `npx skills find pdf` returned its results with the note; `agent.list` answered.

Not changed on purpose: unattended runs completing approval-gated actions follows the owner's trust settings and earlier approvals; a per-project allowlist is an owner decision (`P11-023`). A parallel batch after a held call is refused by design. Not fixed: other adapters that build outcome reasons from provider text should be checked for the same byte and control-character rule (Google operations build theirs from fixed text).