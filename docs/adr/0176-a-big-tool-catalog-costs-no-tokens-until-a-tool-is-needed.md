# ADR-0176: A big tool catalog costs no tokens until a tool is needed

Status: Accepted
Date: 2026-10-10

## Context

Every model call carries the description and input schema of every tool it is offered. JARVIS's own tools are a modest, fixed set, but a program added through MCP can bring dozens, and the built-in browser alone is 25. Measured on the same profile: one plain request ("Reply with the single word OK") cost **8,097 input tokens** with the browser's tools offered, on every call of every run, whether or not the browser was needed. `ADR-0167` §2 promised discovery so that a large catalog stays affordable without ever removing a tool.

## Decision

1. **Groups, not single tools.** The tools of one MCP server form a group (`mcp.<server>.<tool>`). A group with **more than six tools is deferred**: not offered until the run asks. A small group (the two documentation tools) is cheaper to offer than to search for, and native tools are always offered.
2. **`jarvis.tools.search`.** A native, read-only, unasked tool. The model calls it with a few words (or a group's name); it ranks the deferred tools deterministically (group name 3, tool name 2, description 1 per query word) and returns up to ten. **The whole group is then offered for the rest of the run**, because finding `browser_navigate` and lacking `browser_snapshot` is the worse trade.
3. **The model is told what is held back.** The search tool's description lists each waiting group with its size and some tool names, so the model knows what to ask for. With nothing held back, the search tool is not offered at all.
4. **Nothing is stored and nothing changes about authority.** Discovery changes only what is *offered*. Policy, approvals, scopes and audit see every call exactly as before, offered or not. A run's findings are read back from its own transcript (the result carries them, and a tool the model already called counts), so a run that parks for an approval and resumes keeps them.
5. **Outside agents are unaffected.** The Connect apps endpoint still serves every tool (the search tool is not served; a caller there sees the whole list anyway).

## Consequences

Measured live with the repository's own `efficiency-baseline` eval case, on identical fresh profiles with the browser's 25 tools registered: **8,097 → 4,282 input tokens per plain call (−47%)**. A new suite, `evals/discovery.toml`, checks the other direction: asked to open a page, the model searched, found the browser, navigated and answered correctly in **3 of 3** attempts with `deepseek-v4.1-flash:cloud`; asked an arithmetic question it did not search. A task that needs the browser costs one extra model round (the search) and about 25 tools' worth of tokens afterwards, as before. The `basics` suite scored 7 of 9 before and after (the two failures need a granted folder and a web-search key the test profile lacks). A model that never searches would not find the browser: that is the risk, and the eval case is how it is watched across models.

Not done: ranking by meaning (embeddings) rather than words; deferring native tools (they total about 4,000 tokens and stay offered); a per-owner setting for the group-size limit.
