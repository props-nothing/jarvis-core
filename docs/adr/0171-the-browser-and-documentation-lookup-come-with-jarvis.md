# ADR-0171: The web browser and documentation lookup come with JARVIS

Status: Accepted
Date: 2026-10-10

## Context

`ADR-0169` made the browser and documentation lookup one-click **abilities**. The owner's review: they are so basic that a switch makes no sense, because a person who wants a capable assistant would always want them. Needing to find a tab, press a button and restart is a step that should not exist.

They remain separate programs (Playwright MCP, context7) because a native browser driver in Rust is a project of its own and `ADR-0167` prefers open protocols to bespoke code. What changes is **who switches them on**.

## Decision

1. **On by default.** Every built-in ability is added to a profile the first time the daemon starts without it, so the tools are there with no setup. There is **no tab for them**: a separate screen listing the same tools Permissions already lists was redundant (the owner said so), so the Abilities tab and its switches were removed. Each tool appears under Permissions with the same ask/trust/off rules (reading runs on its own, clicking and typing ask). Only the developer's "add your own program" form remains, under Advanced.
2. **Offered once.** The ids already offered are recorded in `config/abilities-seeded.txt`. An ability the owner switches off stays off; one the owner configured by hand is recorded and never replaced. A new ability added to the catalog in a later version is offered once.
3. **A missing Node.js is not an error and not recorded.** Nothing is added, no message interrupts, and installing Node later is enough: the next start offers them.
4. **The first start waits for the download, bounded.** Up to 30 seconds (inside the CLI's 60-second start wait) for each pinned package to be fetched, so the tools exist from the first minute. If it has not finished (slow network), the daemon starts from the document as it was and the entries connect on the next start; the tab shows "ready after restart".
5. **Opt out.** `JARVIS_NO_BUILTIN_ABILITIES=1` skips all of it. The end-to-end gates that start the daemon set it, so a test run never downloads packages. Setting its tools to Off under Permissions or `jarvis mcp off NAME` is the owner's opt-out.
6. **Same pins, same posture.** Versions stay pinned (`ADR-0169` point 2). Starting a package from the internet as the owner is now automatic, which is why the pins, the fenced output, the allowlisted environment and the acting-tools-ask rule matter more, not less.

## Consequences

Verified live on a fresh profile on 2026-10-10: the first `jarvis start` took 13 seconds including the download, wrote both entries and the record, and registered 25 browser tools and 2 documentation tools with nothing switched on by hand; no browser or Node process remained after the daemon stopped. Unit tests cover: offered once, a switched-off ability does not return, a missing program adds nothing and is retried, a hand-configured entry is left alone.

An existing profile gets the two entries at its next start, which downloads two packages from npm. This is the intended default, and the opt-out above is the way to refuse it.

Not done: a native browser (no Node dependency), origin allow lists, and applying a change without a restart (`P11-001b`).

## Agents JARVIS launches (recorded here so it is not lost)

The inbound endpoint of `ADR-0170` is the way a coding agent that JARVIS itself launches (`P7`, over ACP) will use JARVIS's tools on this machine. Two things are still missing and are the next slice (`P11-001e`):

- **No key to copy.** JARVIS should hand the agent the address and a key made for that one launch, and revoke it when the agent stops. Today there is one key the owner pastes.
- **Approvals.** A tool that asks is refused for a remote caller (`ADR-0039`) because a remote call has no run to park. The decision is that the owner should be asked **like any other approval** (one click in the console, `ADR-0136`) and the agent's call waits for the answer, with a time limit. That needs the call to belong to a run record so the approval, the audit row and the resume have somewhere to live, and a falsification test that a denial or a timeout never runs the tool. It is **not** built, and until it is, a connected agent reaches only what runs without asking.
