# ADR-0167: Capability and reach come from open protocols before bespoke code

Status: Accepted
Date: 2026-10-10

## Context

A review of `P5` to `P9` on 2026-10-10 asked whether the open items still describe the best way to build an autonomous assistant that is easy to set up. What is built: a durable run loop with approvals and a kill switch, 37 tools, Google mail and calendar, projects, schedules, sub-agents (one level), memory, a skill store, a console with voice, an MCP host, per-user service install, and an evaluation harness. What the open items still plan was written in September, before most of that existed:

- **A connector per provider** (`P5-006` to `P5-008`: Microsoft, GitHub) on a framework (`crates/jarvis-connectors`, about 1.7 MB) of which the daemon uses only the sign-in and form pieces. Every Google feature the owner uses was built directly in the daemon in a few hundred lines.
- **A durable workflow engine** (`P6-001` to `P6-008`: outbox, inbox, compensation, crash-at-every-transition) before any measured need. A run is already durable and resumable, a schedule starts an ordinary run, and what is missing is *triggers* (a time of day, a mail arriving), not a second execution model.
- **A JARVIS-specific runtime protocol and an adapter per agent framework** (`P7-001`, `P7-004` to `P7-007`).
- **An ElevenLabs-shaped voice stack** (`P8-003` to `P8-006`, `P8-012` to `P8-015`, `P8-017`): five items tied to one vendor's transport.
- **A Tauri client** (`P9-001`, `P9-019`) as the way to be always present, while the console already runs in a browser and `jarvis service install` already starts the daemon at login.

What the open items do not plan, and what makes an assistant feel like the one in the films: hands on the open web (a browser), the ability to learn a procedure once and keep it, being reachable from a phone without a tunnel, acting on its own when something happens, and using a cheap model for cheap work.

External facts this rests on (read live on 2026-10-10):

- **ACP** (Agent Client Protocol, `https://agentclientprotocol.com/llms.txt`): JSON-RPC over stdio, the client launches the agent as a subprocess; v1 and v2 documents exist; the agents page lists Claude (through an adapter), Codex CLI (adapter), Gemini CLI, GitHub Copilot (public preview), Cursor, Cline, Goose, OpenClaw, Hermes Agent, OpenCode, OpenHands, Kimi, Qwen Code and others. One client implementation reaches all of them.
- **Playwright MCP** (`https://github.com/microsoft/playwright-mcp`): an MCP server run with `npx @playwright/mcp@latest`, Node 18 or newer, driving a browser through accessibility snapshots rather than screenshots. Its own README says the CLI form is cheaper for coding agents and that MCP suits long-running autonomous workflows that keep browser state, which is this product.
- **Telegram Bot API** (`https://core.telegram.org/bots/api`): an HTTPS interface; the current version at the access date is 10.3 (2026-08-24). Receiving by long polling needs no public address, which is what makes it easy to set up. The exact methods and limits are to be recorded in a research record before the slice (`docs/research/integrations/telegram.md`).

## Decision

1. **Prefer an open protocol to bespoke code.** A new outward capability is first sought as an MCP server (hands), a new coding agent as an ACP agent (delegation), a new place to talk to JARVIS as a gateway client. Bespoke code is written when the protocol route cannot meet a requirement, and the reason is recorded. Policy does not change: every MCP tool still passes schema validation, effect classification, approval and audit (`ADR-0024`, `ADR-0025`, `ADR-0030`), and its results are untrusted content.
2. **Capability must not cost tokens on every call.** The owner's rule is that JARVIS is not limited by tools. A catalog of hundreds of MCP tools cannot be offered in full on every model call (37 tools are already about 7,200 tokens). So the answer is **discovery, not removal**: every tool stays available, the model is offered the ones relevant to the task plus a way to search the rest by name and description, and a found tool is offered from then on. The selection is deterministic and tested; nothing is hidden from a model that asks (`P11-002`).
3. **Triggers, not a second execution model.** Proactivity is a trigger that starts an *ordinary* run (as a schedule does today): a time of day in a time zone, a mailbox poll that matches, a file that changes, a webhook. Deduplicated, at most once, same authority as an interactive run. The durable workflow engine is **parked** until a process needs to wait days across steps and a run plus a schedule cannot express it.
4. **The model can propose a skill; a person decides.** After a task that took many steps, the model may propose a skill (`ADR-0117`: a procedure that names tools it already has; loading it authorizes nothing). The owner promotes it with one click. This is the loop by which JARVIS gets better at the owner's work without a model update.
5. **Reach comes before telephony.** Telegram first (a bot token and a one-time pairing, no public address), then the other chat platforms, each as an authenticated gateway client that binds a sender to the owner without trusting a caller-supplied identity (`ADR-0038`). Phone calls and local voice follow.
6. **Retire what nothing runs.** The Google push and watch stack in `jarvis-connectors` has no caller and no measured requirement (`P5-005b`). Decision: retire it in its own slice and keep the contracts, OAuth and the readiness checklist. Not extended meanwhile. The owner confirms before the deletion.
7. **Setup stays one command and one decision.** Nothing in this list may add a required step to a first run. A capability that needs a key or a program (Node for the browser, Docker, tesseract) is an absent capability with a plain "turn it on" instruction (`P9-024`), never a failing start.

### What happens to each stale item

| Item | Outcome |
| --- | --- |
| `P5-006`, `P5-007` Microsoft | Superseded: a Microsoft 365 MCP server first (`P11-001`); native only if it cannot meet a named requirement. |
| `P5-008` GitHub | Superseded: the GitHub MCP server through `P11-001`. |
| `P5-005b` push and watch framework | Decided: retire, owner to confirm (decision 6). |
| `P6-001`, `P6-002`, `P6-004` to `P6-006`, `P6-008` envelope, outbox, workflow engine | Parked (decision 3); the original wording is in `docs/history/todo-p6-p8-original.md`. |
| `P6-003` time zones | Kept and promoted: "every day at 08:00" is the first trigger. |
| `P6-009` skill-candidate events | Merged into `P11-004` (the model proposes a skill). |
| `P7-001`, `P7-005`, `P7-007`, `P7-004` own protocol, OpenAI Agents, LangGraph, OpenClaw adapter | Superseded by an ACP client (`P7-001` rewritten). |
| `P8-003` to `P8-006`, `P8-012` to `P8-015`, `P8-017` | Parked: only if the owner uses ElevenLabs agents or a realtime room. `P8-016` goes with them. |
| `P9-001`, `P9-019` Tauri | Demoted: only to host an always-on wake word and a tray. The console and the service already work. |
| `P9-003` service install | Mostly done by `P9-018`; only the documented system-wide modes remain. |
| `P9-024`, `P9-069` | Closed and split into named items below. |

## Consequences

The next slices are, in order: MCP made real and manageable (`P11-001`), tool discovery (`P11-002`), the browser (`P11-003`), the skill loop (`P11-004`), Telegram (`P11-005`), time-zone schedules and triggers (`P6-003`, `P6-012`), model routing (`P11-006`), coding agents over ACP (`P7`), then local voice and calls. Each starts with a research record under `docs/research/integrations/` and ends with a live run against the owner's own setup. Nothing here is built by this ADR; it changes the ledger and the order.

The risk of decision 1 is that third-party servers are third-party code. An MCP server runs as the owner's user, so adding one is an owner decision shown with what it can do, its tools default to asking until the owner trusts them one at a time, and a tool that declares itself read-only is believed only for a server the owner marked trusted (`ADR-0025`). A browser reaches hostile pages: its profile is a throwaway one, it holds no owner sessions unless the owner signs in inside it, and page text is fenced as untrusted like any fetched page.
