# Capability wishlist

An out-of-the-box wishlist, written by an AI agent working with JARVIS (an audit of what it could not do), reviewed 2026-10-11 and mapped onto the ledger. The aim is the most capable assistant that still sets up in minutes (`ROADMAP.md`). Where a protocol exists, the first answer is an MCP server (`ADR-0167`), not new code; every new tool still passes the same pipeline (policy, approvals, audit).

Status words: **done** (built and run), **partly** (a first form exists), **planned** (has a ledger item), **new** (added to the ledger on 2026-10-11).

## Connectivity

| # | Wish | Status | Ledger |
| --- | --- | --- | --- |
| 1 | MCP client layer: one interface to thousands of servers | **done**: servers added from Settings, a built-in browser and docs, tool discovery keeps a large catalog cheap | `P11-001`, `P11-002` (`ADR-0168`, `ADR-0176`) |
| 2 | Inbound webhooks: the outside world triggers JARVIS | planned: the `webhook` kind of a trigger; other apps can already call JARVIS over MCP | `P6-012`, `P11-017` |
| 3 | Messaging beyond email: WhatsApp, Telegram, Slack, SMS | **partly**: Telegram built (`ADR-0184`, needs one live run); Slack, WhatsApp, SMS new | `P11-005`, `P11-019` |
| 4 | Voice: speech to text, text to speech, phone calls | **partly**: spoken answers (ElevenLabs or the browser) and push-to-talk exist; local recognition and calls planned | `P8-010`, `P8-007` |
| 5 | Authenticated browsing: logged-in sessions, cookies, OAuth API calls | new | `P11-012` |

## Documents and media

| # | Wish | Status | Ledger |
| --- | --- | --- | --- |
| 6 | **Writing** PDF, Word, Excel, PowerPoint | **partly**: Excel, CSV and Word built and opened in the real Office (`ADR-0185`); PDF and PowerPoint open | `P11-010` |
| 7 | Offline OCR for scans and photos | **done** (needs `tesseract` installed; one live run still open) | `P9-082` |
| 8 | Image generation, inline images in chat | new | `P11-011` |
| 9 | Screenshots shown inline | new | `P11-011` |
| 10 | Charts and graphs | new | `P11-010` |

## Runtime and code

| # | Wish | Status | Ledger |
| --- | --- | --- | --- |
| 11 | Python runtime, or a WASM/JS fallback | **partly**: code runs in a Docker container the owner chose; no Docker means no code tool | `P11-013` |
| 12 | Sandboxed long-running processes (dev servers, watchers) with ports | new | `P11-013` |
| 13 | Desktop automation: keyboard, mouse, native windows | planned, parked until a concrete job | `P11-009` |

## Data and knowledge

| # | Wish | Status | Ledger |
| --- | --- | --- | --- |
| 14 | Local SQL plus vector search over the owner's files (real RAG) | **partly**: memory has exact, full-text and entity retrieval, embeddings are researched not built; nothing indexes files | `P11-014` |
| 15 | Spreadsheet and CSV writing | **done** (`ADR-0185`) | `P11-010` |
| 16 | Knowledge-graph memory linking facts across projects | **partly**: entities and relations are stored per workspace; no cross-project view | `P11-014` |
| 17 | Encrypted secrets vault for API keys | new: keys are private files today | `P11-015` |

## Productivity and ops

| # | Wish | Status | Ledger |
| --- | --- | --- | --- |
| 18 | Calendar conflict detection: free/busy before booking | new | `P11-016` |
| 19 | Contact dedupe and merge | new (contacts exist, merge does not) | `P11-016` |
| 20 | Trend export: per-cycle CSV/JSON of audit scores | new (`jarvis eval` prints, does not export a series) | `P11-022` |
| 21 | Integrated skill discovery (`npx skills find` crashes on exit) | new | `P11-021` |
| 22 | Monitoring and uptime checks with alerts | new (schedules, web fetch and Telegram are the parts) | `P11-018` |
| 23 | Maps, routing and location data | new, MCP first | `P11-020` |
| 24 | Invoicing and payment helpers for the sales project | new, MCP first, outward effects always ask | `P11-020` |
| 25 | Scheduled self-update: re-research the wishlist periodically | new (a scheduled task over this file) | `P11-022` |

## How the new ones are ordered

By how often they block real work and how little they need: **writing documents** (`P11-010`) first, because the owner's sales project already builds a spreadsheet with a hand-written script; **images inline** (`P11-011`) next, because a screenshot nobody can see is wasted; then **the ones that make JARVIS reachable and watchful** (`P11-017`, `P11-018`, `P11-019`); then the **data** items (`P11-014`, `P11-015`, `P11-016`); the rest as the owner has a job for them.

## From the audit's own findings (cycles 1 to 7, read 2026-10-11)

The audit folder also records what failed. Fixed the same day (`ADR-0187`, `TODO.md` `P11-023`): the empty `unknown` result of a failed browser upload, `tools.search` ignoring tools it already offered, invisible sub-agent slots (`jarvis.agent.list`), the unexplained 32-word limit of a memory correction, `shutdown.exe` failing in the command tool, and `npx skills find` reading as a crash. Candidates it researched and what became of them:

| Candidate | Outcome |
| --- | --- |
| MCP client layer | done (`P11-001`) |
| `anthropics/skills` pdf, docx, xlsx (Python-based) | xlsx and docx written natively, no Python needed (`ADR-0185`); pdf open |
| `ocrs` offline OCR | not needed yet: `tesseract` is supported (`ADR-0162`); revisit if installing it proves a barrier |
| `croner` cron with time zones | not needed: schedules with zones are built in (`ADR-0182`) |
| `vercel-labs/agent-browser`, `playwright-cli` | not needed: the Playwright MCP browser is built in (`ADR-0169`) |
| `usecomputer` desktop control | parked with `P11-009` until the owner names a job |
| `hns` local voice input | with `P8-010` |
| `backlog.md` task board | not adopted: the project journal and `TODO.md` already do this; revisit if the audit outgrows them |
| react-email / MJML | outreach is drafted as plain text on purpose; revisit with a concrete mailing |