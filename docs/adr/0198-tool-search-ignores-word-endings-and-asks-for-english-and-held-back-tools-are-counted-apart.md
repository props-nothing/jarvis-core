# ADR-0198: Tool search ignores word endings and asks for English, held-back tools are counted apart, and found tools come last

Status: Accepted
Date: 2026-10-11

## Context

`ADR-0176` and `ADR-0197` hold tools back until a run searches for them. A comparison with OpenClaw's Tool Search and Anthropic's tool search tool (`docs/research/tool-catalogs-and-prompt-cost.md`) showed three gaps in ours:

1. The search matched exact substrings. "scheduling" did not find a tool described as "schedule", and a Dutch query found nothing, because tool names and descriptions are English. OpenClaw stems words lightly and tells the model to write in English.
2. `jarvis tools list` said each call is sent "about 9,556 tokens of tool definitions" by summing **every** tool, which stopped being true once tools were held back: it reported the cost of tools a call does not carry.
3. When a run found a group, that group landed in the middle of the tool list (and the search tool's text shrank), so the request changed from that point on. OpenClaw keeps its tool prefix identical between turns so the provider can reuse its prompt cache. JARVIS's own run records show the provider does cache: 71% of recorded input tokens were cached.

## Decision

1. **Search matches stems.** Words of one or two letters and filler words are ignored; a few English endings (`ing`, `ed`, `es`, `s`) and a final silent `e` are cut before a substring match. The description of `jarvis.tools.search` and its `query` field say to write the query in English whatever language the user writes in.
2. **`GET /api/v1/tools` says which tools are held back** (`deferred_group`), and `jarvis tools list` adds up only the tools every call carries, lists the held-back groups with their size, and marks each held-back tool "(searched)".
3. **Found tools and the search tool go last** in the tool list, so everything before them is identical from call to call and across the calls of a run.

## Not adopted

- OpenClaw's `tool_describe` and `tool_call` proxy: JARVIS policy, approvals and audit key on the real tool name, and a model that calls a proxy hides it.
- A directory of every tool name in the prompt: it costs tokens on every call, and JARVIS's held-back groups are few.

## Consequences

- A search in Dutch can still miss, because only the model's choice of words changes; `evals/discovery.toml` has a Dutch case (passed twice with `glm-5.3-flash`).
- The token figure is a rough count (characters divided by four) and over-states what the provider counts: the figure said about 9,556 while the measured total request for "reply OK", system prompt included, was 6,987. It is good for comparing tools, not for billing.
- The effect of the new order on cached tokens was not measured; the claim is only that the prefix is now stable.
