# ADR-0207: A tool the owner trusted runs without a console question during the owner's own phone call

Status: Accepted
Date: 2026-10-13

## Context

`ADR-0200` lets a yes spoken on a call **JARVIS placed** stand as the owner's answer when another app (the phone agent, over MCP) calls a tool. A call the owner makes **to** JARVIS is not one JARVIS placed, so it never qualified. A call arriving over MCP also ignores the owner's per-tool "run without asking" (`ADR-0175`, `ADR-0178`): the tool's own `Ask` declaration holds it for the owner. The result, on 2026-10-13: the owner rang JARVIS, asked for a callback, and had to approve `jarvis.schedule.add` in the console although they had set every schedule tool to run without asking.

## Decision

When a call from another app would wait for the console, it runs instead, citing the approver `local-owner:phone-line`, if **all** of these hold:

1. the owner **trusted that tool by name** (Settings, Permissions); a tool left at its default still asks;
2. the owner switched on the yes on the phone (`daemon.phone_approves`, off by default);
3. the tool's risk is 2 or less and it is not a `jarvis.voice.*` tool (a phone call never approves a phone call);
4. the owner has not pinned the tool to "Always ask first" (tightening outranks, as in `ADR-0200`);
5. the provider lists at least one open conversation on the phone agent and **every** one is from a number on the owner's list. An unreadable list, an empty one or any other caller is "no", and the line guard still refuses all tools when a stranger is on the line.

## Consequences

- This extends trust, not the question: nothing the owner has not already said may run without asking is affected, and nothing in the console is bypassed for a tool they left at "ask".
- **Caller ID can be forged** (`ADR-0201`). Someone who fakes the owner's number is the owner as far as the door and this rule go, and could run the tools the owner trusted, up to risk 2, for as long as the call lasts. The exposure is the owner's own trust list, bounded by the risk ceiling, the call limits and the opt-in setting; keep high-impact tools at "ask". Turning on authentication in the agent's Security tab narrows who can reach the agent at all.
- Tested: a stand-in for the phone (trusted and alone runs; untrusted, not alone and pinned do not) with a mutation check that dropping the trust condition fails the test, and the line check against the stand-in provider (nobody, a stranger among others, an unreadable list, the setting off, a high risk, a phone tool). Not run with a real call.
