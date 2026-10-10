# ADR-0175: Trusting a third-party tool by name releases it

Status: Accepted
Date: 2026-10-10

## Context

The owner set the browser's click to "Run without asking" in Permissions and it still asked. The policy never waives approval for a tool whose effects include external communication (`ADR-0017`, `ADR-0122`), and every unclassified MCP tool is treated as outward (risk 3, asks) because nothing is known about someone else's tool. So the choice was offered, saved and silently ignored for exactly the tools the owner most wants to control, and the screen said nothing.

## Decision

1. **Trust counts for a third-party tool.** `WorkspacePolicy::external_rule_applies`: the rule holds for every tool with the effect **except** a third-party (MCP) tool the owner trusted by name. For a native sender the effect is a fact about the tool, so mail and calendar still always ask. For a third-party tool the effect is a default guess; the owner's explicit per-tool choice replaces it.
2. **Only what was chosen.** Trusting `browser_type` does not release `browser_click`. A denial or an "always ask" override still outranks trust, the risk ceiling and scopes are unchanged, and the memory and project tools can be trusted by name since `ADR-0177`.
3. **The screen is honest.** A tool saved as "Run without asking" that still asks shows "still asks: it sends things to other people". The Permissions note and `docs/user/settings.md` say which tools are never waived.
4. **Not changed:** a call from an outside agent through Connect apps still cannot hold an approval and is refused for any tool that would ask; trust is the owner's, not the caller's.

## Consequences

Tested: a trusted third-party tool is allowed and `asks_first` agrees; an untrusted one is held; a different tool of the same server is unaffected; a trusted native sender is still held. Prompt injection is the cost: a page can try to steer a trusted click. The browser's reading tools already run freely and the output is fenced as untrusted data (`ADR-0168`); the owner chose the trade for the acting tools per tool, and can set any of them back to "Always ask first".
