# Tool Catalogs And Prompt Cost In Other Agents

Reviewed on **2026-10-11** from official documentation and source. A structural study for `ADR-0198`; nothing here is copied code, and nothing is claimed to work in JARVIS until a JARVIS test or live run shows it.

## Sources (access date 2026-10-11)

| Source | What it is | Used for |
| --- | --- | --- |
| https://github.com/openclaw/openclaw `docs/tools/tool-search.md` (SHA `0332c239f8e0b6a4364cf345d7c9d47eb12eaa8a`, "experimental") | OpenClaw's own Tool Search design | search, describe, call; directory; ranking; cache-stable prefix |
| https://docs.openclaw.ai/reference/token-use | What fills OpenClaw's context | tool-result caps, bounded injected sections |
| https://github.com/openclaw/openclaw `docs/concepts/context.md` (SHA `eb3c58a6ae7b79fd45518ac944bf9b0f7706508a`) | `/context list`, `/context detail` | a per-tool and per-section size breakdown |
| https://platform.claude.com/docs/en/agents-and-tools/tool-use/tool-search-tool | Anthropic's tool search tool | scale and accuracy claims, `defer_loading` |

A web-search summary and community pages that cite OpenClaw were leads only and are not relied on.

## Facts

OpenClaw Tool Search:
- Gives the model three structured tools, `tool_search`, `tool_describe` and `tool_call`, plus a size-limited **directory** of tool names and short descriptions (sorted by name, at most 18,000 characters, scaled to the model's context window; descriptions shorten before names are dropped). Core file and shell tools and "direct-only" tools stay directly visible. It is on by default for embedded runs but documented as experimental and says it is **not always cheaper**: "small catalogs can gain schema overhead, and additional discovery turns can offset initial payload savings."
- Ranks with BM25 over names, descriptions and argument names, with light English stemming ("scheduling" finds "Schedule a recurring task") and a small intent expansion. **Queries must be written in English**, and the tool's own description says so. Untrusted (MCP, client) parameter schemas are never indexed.
- A hit carries a compact input signature, so the model can often call without a describe step. Invalid arguments return an actionable error, with a suggested parameter name for a misspelling.
- The rendered directory is reused for an unchanged catalog and placed **above the system-prompt cache boundary**; per-turn guesses and user text stay out of it, "to keep repeated turns eligible for prompt KV-cache reuse".
- `/context list|detail|map` shows system prompt size, per-tool schema size and per-skill size, so a person can see what each call carries. A tool result is capped from the model's window (16,000 characters under 100K tokens, 32,000 at 100K+, 64,000 at 200K+, and 30% of the window at most).

Anthropic's tool search tool: a typical five-server setup "can consume ~55k tokens in definitions"; tool search "typically reduces this by over 85 percent"; "selection accuracy degrades once you exceed 30-50 available tools"; tools carry `defer_loading: true`, and there are regex and BM25 variants.

## Mapped to JARVIS (decisions, not facts)

| Pattern | JARVIS today | Decision |
| --- | --- | --- |
| Search over a deferred catalog | `jarvis.tools.search` over deferred groups (`ADR-0176`), now with two built-in families (`ADR-0197`) | keep: it is the same idea; JARVIS offers a whole group at once instead of describe and call |
| English queries, stemming | substring match, no stemming, no language hint | **adopt** (`ADR-0198`): the owner writes Dutch; the tool says "English" and a word's ending no longer decides |
| Stable prefix | found tools landed in the middle of the tool list | **adopt**: found tools and the search tool go last. The provider does cache (see below) |
| Per-tool size breakdown | `jarvis tools list` summed every tool, including held-back ones | **adopt**: held-back tools are counted apart |
| Directory of every tool name in the prompt | group names with up to eight tool names | not adopted: costs tokens on every call and JARVIS's groups are few; revisit if the catalog grows |
| `tool_describe` and `tool_call` as one proxy | the model calls the real tool once offered | not adopted: a proxy hides the real tool name from policy, approvals and audit, which JARVIS keys on |
| Tool-result cap derived from the window | fixed bound plus `ADR-0194` masking | not changed; the 200 KB trim budget is still unmeasured |

## What the cheapest test showed

JARVIS's own run records say the provider caches prompts: over 84 calls on `glm-5.3-flash`, 530,432 of 750,444 input tokens (71%) were reported as cached. So a request whose start does not change is worth keeping stable. How much the new ordering saves was **not** measured (no before and after on cached tokens).

## Unresolved

- Whether a bounded name directory beats group names for JARVIS at 70+ tools. Needs a `jarvis eval` suite with many tools.
- Whether BM25 beats the substring score for JARVIS's catalog; the catalog is small enough that this is unlikely to matter yet.
