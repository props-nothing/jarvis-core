# Capability wishlist

An out-of-the-box wishlist, written by an AI agent working with JARVIS (an audit of what it could not do), initially reviewed 2026-10-11 and reconciled 2026-10-16 against implemented behavior. Current order and acceptance live only in [TODO.md](../../TODO.md). The aim is the most capable assistant that still sets up in minutes (`ROADMAP.md`). Where a protocol exists, the first answer is an MCP server (`ADR-0167`), not new code; every new tool still passes the same pipeline (policy, approvals, audit).

Status words distinguish **built** from **live evidence** and **remaining work**. A fixture-tested capability with an open owner-account check is not fully accepted. Historical “new” means proposed on 2026-10-11, not newly discovered today.

## Connectivity

| # | Wish | Status | Ledger |
| --- | --- | --- | --- |
| 1 | MCP client layer: one interface to thousands of servers | **done**: servers added from Settings, a built-in browser and docs, tool discovery keeps a large catalog cheap | `P11-001`, `P11-002` (`ADR-0168`, `ADR-0176`) |
| 2 | Inbound webhooks: the outside world triggers JARVIS | **partly**: secret-path webhook triggers are built; real external-service/replay acceptance remains, and HMAC signatures are not implemented | `P6-012`, `P11-017` |
| 3 | Messaging beyond email: WhatsApp, Telegram, Slack, SMS | **partly**: Telegram built with owner-phone evidence (`ADR-0184`/`0186`); current formatting/failure checks and Slack, WhatsApp, SMS remain | `P11-005`, `P11-019` |
| 4 | Voice: speech to text, text to speech, phone calls | **partly**: browser speech and owner-reported outbound calls exist; local speech and broader telephony remain | `P8-010`, `P8-007` |
| 5 | Authenticated browsing: logged-in sessions, cookies, OAuth API calls | new | `P11-012` |

## Documents and media

| # | Wish | Status | Ledger |
| --- | --- | --- | --- |
| 6 | **Writing** PDF, Word, Excel, PowerPoint | **partly**: Excel, CSV and Word built and opened in the real Office (`ADR-0185`); PDF built (`ADR-0190`); PowerPoint open | `P11-010` |
| 7 | Offline OCR for scans and photos | **built, acceptance pending**: needs Tesseract and a real owner OCR run | `P9-082` |
| 8 | Image generation, inline images in chat | inline display built (`ADR-0188`); generation open | `P11-011` |
| 9 | Screenshots shown inline | built in the console and Telegram (`ADR-0188`) | `P11-011` |
| 10 | Charts and graphs | new | `P11-010` |

## Runtime and code

| # | Wish | Status | Ledger |
| --- | --- | --- | --- |
| 11 | Python runtime, or a WASM/JS fallback | **partly**: container analysis uses Docker; approved host coding/managed processes also exist. A no-Docker confined analysis runtime remains | `P11-013` |
| 12 | Sandboxed long-running processes (dev servers, watchers) with ports | **partly**: managed host processes/previews are built (ADR-0222); enforceable sandboxing remains | `P11-013` |
| 13 | Desktop automation: keyboard, mouse, native windows | planned, parked until a concrete job | `P11-009` |

## Data and knowledge

| # | Wish | Status | Ledger |
| --- | --- | --- | --- |
| 14 | Local SQL plus vector search over the owner's files (real RAG) | **partly**: granted-file FTS5 search and task-aware memory recall exist; semantic retrieval, owner-corpus measurement and automatic file/entity context remain | `P11-014` |
| 15 | Spreadsheet and CSV writing | **done** (`ADR-0185`) | `P11-010` |
| 16 | Knowledge-graph memory linking facts across projects | **partly**: entities and relations are stored per workspace; no cross-project view | `P11-014` |
| 17 | Encrypted secrets vault for API keys | new: keys are private files today | `P11-015` |

## Productivity and ops

| # | Wish | Status | Ledger |
| --- | --- | --- | --- |
| 18 | Calendar conflict detection: free/busy before booking | built (`ADR-0189`) | `P11-016` |
| 19 | Contact dedupe and merge | built (`ADR-0189`) | `P11-016` |
| 20 | Trend export: per-cycle CSV/JSON of audit scores | new (`jarvis eval` prints, does not export a series) | `P11-022` |
| 21 | Integrated skill discovery and installation | **partly**: local/public-commit bundles and full resources are built; MCP capability discovery, private/global catalogs and safe updates remain | `P11-021` |
| 22 | Monitoring and uptime checks with alerts | new (schedules, web fetch and Telegram are the parts) | `P11-018` |
| 23 | Maps, routing and location data | new, MCP first | `P11-020` |
| 24 | Invoicing and payment helpers for the sales project | new, MCP first, outward effects always ask | `P11-020` |
| 25 | Scheduled self-update: re-research the wishlist periodically | new (a scheduled task over this file) | `P11-022` |

## Priority and evidence

The [active backlog](../../TODO.md) is the only execution order. Finish the owner-selected reliability/documentation corrections and chosen account/browser/native acceptance gates first. New capability follows a named job or measured limitation; delivered documents, inline images, routing, schedules, host processes and skill bundles are not new implementation tasks.

## From the audit's own findings (cycles 1 to 7, read 2026-10-11)

The audit folder also records what failed. Fixed the same day (`ADR-0187`, `TODO.md` `P11-023`): the empty `unknown` result of a failed browser upload, `tools.search` ignoring tools it already offered, invisible sub-agent slots (`jarvis.agent.list`), the unexplained 32-word limit of a memory correction, `shutdown.exe` failing in the command tool, and `npx skills find` reading as a crash. Candidates it researched and what became of them:

| Candidate | Outcome |
| --- | --- |
| MCP client layer | done (`P11-001`) |
| `anthropics/skills` pdf, docx, xlsx (Python-based) | XLSX/DOCX/PDF written natively (`ADR-0185`/`0190`); richer PDF Unicode, PowerPoint and charts remain |
| `ocrs` offline OCR | not needed yet: `tesseract` is supported (`ADR-0162`); revisit if installing it proves a barrier |
| `croner` cron with time zones | not needed: schedules with zones are built in (`ADR-0182`) |
| `vercel-labs/agent-browser`, `playwright-cli` | not needed: the Playwright MCP browser is built in (`ADR-0169`) |
| `usecomputer` desktop control | parked with `P11-009` until the owner names a job |
| `hns` local voice input | with `P8-010` |
| `backlog.md` task board | not adopted: the project journal and `TODO.md` already do this; revisit if the audit outgrows them |
| react-email / MJML | outreach is drafted as plain text on purpose; revisit with a concrete mailing |
