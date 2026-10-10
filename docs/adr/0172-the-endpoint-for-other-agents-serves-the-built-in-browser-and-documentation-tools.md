# ADR-0172: The endpoint for other agents serves the built-in browser and documentation tools too

Status: Accepted
Date: 2026-10-10

## Context

`ADR-0170` let another program use JARVIS through one switch and one key, and `ADR-0171` made the browser and documentation lookup ordinary built-in tools. The owner's intent for the endpoint (stated twice): an outside agent, for example a future ElevenLabs phone agent, should **see JARVIS's tools and abilities and use them on this machine**. Measured live, it did not: the endpoint served 21 tools, none of them the browser or documentation lookup, because the served-surface rule (`jarvis-mcp/src/served.rs`) withholds every tool that comes from someone else's MCP server. That rule is sound for an arbitrary server: its tools carry a posture nobody here wrote.

## Decision

1. **A catalog tool is served; nothing else third-party is.** `served_tools_vetting` takes a predicate that only the daemon can supply (nothing a server reports can make a tool vetted). The daemon vets a server only if it is configured as **exactly** a catalog entry: stdio, the same pinned package and arguments, named as the catalog names it (`mcp_abilities::vetted_prefixes`). An owner's own server called `browser` that runs something else is not vetted. A hand-added GitHub or database server stays withheld, as before.
2. **Same policy, same gates.** A served browser tool goes through the same validation, policy decision and untrusted-output fence as any tool. Reading tools run; acting tools (click, type, upload, run code) need an approval, and a remote call cannot hold one (`ADR-0039`), so they are **refused** with `mcp_call_cannot_hold_an_approval` and nothing is left waiting.
3. **The cap rises from 32 to 96 tools.** The built-in browser alone is 25. A long list costs the outside agent's context, which is what tool discovery (`P11-002`) is for.

## Consequences

Verified live on a fresh profile on 2026-10-10 over HTTP with the access key: `tools/list` returned 48 tools (21 JARVIS tools, 25 browser, 2 documentation); `mcp.browser.browser_navigate` to example.com ran and returned the page, fenced as untrusted data; `mcp.browser.browser_evaluate` (acting) was refused with `mcp_call_cannot_hold_an_approval` and `jarvis approvals list` showed nothing waiting. Unit tests: a vetted tool is served and an unvetted one is withheld (falsified by making the predicate always true); only an exact catalog entry is vetted; a same-named custom server is not.

## What a phone agent such as ElevenLabs would still need (recorded, not built)

Read from the vendor's page on 2026-10-10 (`elevenlabs.io/docs/eleven-agents/customization/tools/mcp`): the agent platform connects **from its own cloud** to a *Server URL*, with an optional *Secret Token* sent as the `Authorization` header and extra headers, over **SSE or streamable HTTP**; the platform has its own approval policy per server, and MCP is unavailable to Zero-Retention and HIPAA workspaces. Consequences for JARVIS:

- **Reach.** The endpoint binds 127.0.0.1 only, so a cloud service cannot reach it. A telephony slice needs a deliberate, owner-set public HTTPS address in front of the endpoint (a tunnel), with the access key as the secret token. Built afterwards as a managed quick tunnel: see `ADR-0173` and `docs/research/integrations/cloudflare-tunnel.md`. Its address changes every start.
- **Approvals.** Acting tools are refused until `P11-001e` lets an outside agent's call wait for the owner's yes (`ADR-0136`: one click or one spoken word).
- **Per-agent keys.** One shared key means a leaked key exposes everything served. A key per agent with its own revoke is part of `P11-001e`.
- **Transport fit.** The access key is a Bearer value, which matches an `Authorization` secret token, and the server speaks streamable HTTP. Neither was run against ElevenLabs itself.
