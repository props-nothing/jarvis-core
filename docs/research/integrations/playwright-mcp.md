---
integration: playwright-mcp
status: researched
last_verified: 2026-10-14
owners: []
selected_spec_version: "MCP legacy 2025-06-18 (negotiated by the selected server)"
selected_sdk: "@playwright/mcp 0.0.83; playwright-core 1.64.0-alpha-1790635538000"
---

# Playwright MCP: explicit navigation and literal find

## Scope

Repair the built-in browser's model-facing navigation/find/snapshot contracts and complete tool discovery without adding another browser process or a policy bypass. Existing fill/type/evaluate/screenshot operations remain available under their existing permissions; no arbitrary-script authority is added. Origin allowlists remain outside this slice.

## 2026-10-14 follow-up: the model's 64-tool cut hid browser core tools

The owner's normal live profile registers **83 callable tools**. Identifier order places `browser_click` at 59 and `browser_emulate_media` at 64; navigate is 71, fill is 67, snapshot is 79. Both the native model offering and tool-search catalog incorrectly consumed `ToolRegistry::discover()`'s first-64 summary. This exactly explains the six working tools and the missing core tools even though the underlying server is connected. It is not a Playwright capability flag problem.

Live official sources rechecked during this follow-up (2026-10-14 UTC):

- https://raw.githubusercontent.com/microsoft/playwright-mcp/v0.0.83/README.md and
  https://api.github.com/repos/microsoft/playwright-mcp/releases/latest (still 0.0.83).
- https://raw.githubusercontent.com/microsoft/playwright-mcp/v0.0.83/llms.txt still returned 404.
- https://raw.githubusercontent.com/microsoft/playwright/e8149b8257d32dcf8f72573ecc43e72439da7080/packages/playwright-core/src/tools/backend/response.ts:
  automatic snapshots from actions/navigation are deliberately written as `.yml` artifacts, even in full snapshot mode. No guessed output flag fixes this.
- https://raw.githubusercontent.com/microsoft/playwright/e8149b8257d32dcf8f72573ecc43e72439da7080/packages/playwright-core/src/tools/backend/snapshot.ts:
  explicit `browser_snapshot` without `filename` returns the snapshot inline; optional `target`, `depth`, `boxes` scope it.
- https://raw.githubusercontent.com/microsoft/playwright/e8149b8257d32dcf8f72573ecc43e72439da7080/packages/playwright-core/src/tools/backend/form.ts:
  `browser_fill_form` takes `fields` with `name`, `type`, `target`, `value`; target is a snapshot ref or unique selector, **not `ref`**. Textbox fill does not submit by itself.

**Decisions:** native discovery enumerates the complete bounded registry (maximum 512), then applies existing group deferral. Keep the first-64 summary API unchanged for its other consumers. Once found, the complete browser group is offered, including later identifiers. Narrow snapshot to inline readback (no output filename), with bounded target/depth, preserving the existing read-only posture. Navigate tells the model to call explicit snapshot for inline elements. No automatic file loading, extra script execution, hidden second tool call, permission change or dependency upgrade.

**Falsifiers:** register more than 64 callable tools and prove a search finds late browser tools and the next actual model request includes navigate, snapshot, fill/type/evaluate/screenshot. A synthetic loopback page must navigate, return inline element references, fill under explicit test-owner trust, and reflect the changed value in a second inline snapshot. Invalid snapshot/file-output inputs must not reach the provider. No owner's browser page or form is needed.

## Official Sources

All sources were fetched live on **2026-10-14 (UTC)** before implementation.

| Source | URL/version | Purpose |
| --- | --- | --- |
| AI index | https://playwright.dev/llms.txt and https://raw.githubusercontent.com/microsoft/playwright-mcp/v0.0.83/llms.txt | Both returned 404; official versioned README/source used instead. |
| Versioned tool documentation | https://raw.githubusercontent.com/microsoft/playwright-mcp/v0.0.83/README.md | `browser_navigate`, `browser_find`, isolated browser and CLI configuration. |
| Package and dependency pin | https://raw.githubusercontent.com/microsoft/playwright-mcp/v0.0.83/package.json | Apache-2.0, exact Playwright version; wrapper declares Node >=18. |
| Published implementation revision | https://registry.npmjs.org/playwright-core/1.64.0-alpha-1790635538000 | `gitHead=e8149b8257d32dcf8f72573ecc43e72439da7080`; transitive package actually requires **Node >=20**. |
| Navigation implementation | https://raw.githubusercontent.com/microsoft/playwright/e8149b8257d32dcf8f72573ecc43e72439da7080/packages/playwright-core/src/tools/backend/navigate.ts | Core-navigation tool accepts `{url:string}`, uses `checkUrlAndNavigate`, includes a snapshot. |
| Find implementation | https://raw.githubusercontent.com/microsoft/playwright/e8149b8257d32dcf8f72573ecc43e72439da7080/packages/playwright-core/src/tools/backend/find.ts | Text lowercases and uses `includes`; regex is separate; three context lines; fresh accessibility snapshot. |
| Upstream independent tests | https://raw.githubusercontent.com/microsoft/playwright/e8149b8257d32dcf8f72573ecc43e72439da7080/tests/mcp/find.spec.ts | Text, case-insensitivity, no-match, invalid regex, and file-output contracts. |
| Current release/deprecations | https://api.github.com/repos/microsoft/playwright-mcp/releases/latest | Latest remains v0.0.83; no relevant deprecation observed. Notes mention find filename support and navigation dialogs. |
| Browser API reference | https://playwright.dev/docs/api/class-page | Browser/page lifecycle and navigation. |

## Verified Contract

### Operations And Transport

The cached, real pinned server was started over stdio during research. It negotiated `2025-06-18` and listed **25 tools**, including both `browser_navigate` and `browser_find`. Navigation is not missing at the provider wire boundary. Its JSON Schema requires `url`, forbids additional properties. Find exposes optional `text`, `regex`, `filename` but does **not** encode the exactly-one-query rule in its schema: `{}` passes JSON Schema yet fails in the handler. Regex errors and optional file output make this too broad for a dependable literal-reading contract.

`browser_find` searches serialized accessibility snapshot lines, not arbitrary HTML, hidden DOM text, image pixels, PDFs, or a whole website. Its `text` branch is a case-insensitive **literal substring**, including regex metacharacters. It scans a fresh snapshot, reports matching nodes/context or `No matches found for ...`. It is not streaming or paginated. Navigation uses the existing session/tab and returns a page snapshot. No arbitrary JS is needed for either operation.

### Authentication And Authorization

Local stdio has no provider key, OAuth, token refresh, callback, or webhook. The built-in isolated browser does not import the owner's normal cookies/profile. Existing allowlisted child environment and MCP authentication/authorization/audit pipeline remain unchanged. Login, typing, clicking, upload and arbitrary scripts remain acting operations; server hints do not grant permission.

### Limits And Failure Semantics

The vendor query strings have no documented input-size or match-count bound. JARVIS will require nonblank `text` of at most 512 characters, reject regex/file-output fields, and bound navigation URLs to 8,192 characters and credential-free HTTP(S). These are **JARVIS decisions**, not vendor quotas. Existing bounded MCP result conversion and tool deadline apply; large output may be truncated or the vendor may refer to a full-output file. Such output does not prove that every match was shown. No automatic filesystem read of that file is added.

Existing transport conversion distinguishes `isError`/JSON-RPC refusal from ambiguous transport failure. No adapter-level retry is added. Navigation can cause network requests/page scripts and is not globally idempotent; read-only is the existing owner-facing policy classification, not a promise that a website has no side effects. Find is read-only. Both still require `mcp.call`; unclassified/acting tools continue asking. No rate quota is published for the local server. Shared-session concurrency remains the existing behavior, not a new per-run isolated-tab guarantee.

### Data And Compliance

The target website receives navigation requests; URLs must not carry credentials. Page content, snippets and diagnostics are untrusted third-party data fenced by the existing MCP adapter. The literal query is sent to the local browser server, not an external search service. Automatically named artifacts remain in profile runtime state. No owner's websites, credentials, browser state, or personal data are needed for verification. No vendor implementation is copied.

### Versions And Deprecations

No package/dependency upgrade. Pin remains 0.0.83 / 1.64.0-alpha-1790635538000 (alpha transitive runtime). The Node >=18 wrapper claim conflicts with its Node >=20 dependency requirement; user documentation must state Node 20+. The current release endpoint confirms no newer release or relevant migration.

## JARVIS Mapping

`mcp.browser.browser_navigate` stays the explicit navigate tool; `mcp.browser.browser_find` becomes the fixed/literal text-only tool for the built-in pinned browser. Improved descriptions make open/read/find intents discoverable through existing `jarvis.tools.search`. Canonical IDs, `ToolSource::Mcp`, effects, risk, scopes, approvals, evidence locators and untrusted-result conversion are preserved. Schema validation is repeated before forwarding the unchanged authorized request. A disabled/missing browser does not acquire synthetic tools. Owner-customized server commands are not silently reclassified.

## Decisions

- Narrow the built-in browser's schema instead of inventing another engine, tool namespace or permission grant.
- Require explicit navigate `{url}` and fixed find `{text}`; no regex, JS, selectors or output filenames on these two contracts.
- Reuse the same MCP adapter/connection and its result/error handling without modifying authorization-bound arguments.
- Preserve acting-vs-reading classification and all owner permission overrides.

## Rejected Alternatives

- Arbitrary JS evaluate/run-code for reading: unnecessary and would enlarge authority.
- Regex escaping heuristics or trying regex after text fails: not a fixed-string contract.
- Silent file reads after vendor output truncation: introduces a second, unauthorized capability.
- A stand-in-only completion claim: must run the real pinned server on a disposable loopback synthetic page when available.

## Verification Plan

Cheapest falsifier: pass `{"regex":"("}` or `{}` to the new find contract; it must fail **before** calling the remote executor. A literal `[` must validate and be forwarded unchanged.

- Contract/regression: real server listing fixtures translate; required/nonblank/bounded text; URL schema and semantic validation; regex/filename/JS/credential rejection; read/act posture unchanged; unrelated/custom tools unchanged; tool search locates navigate/find.
- Real-server opt-in test: create a fresh in-process HTTP page, connect through JARVIS's configured MCP host, navigate, find regex metacharacters as literal text, case-insensitive match and explicit no match; verify bounded/untrusted results; close browser and synthetic server.
- Run focused Rust tests, formatting, and lint checks; record exact results below. Owner-site evaluation and cross-platform proof remain separate.

## Unresolved Questions

- Reported missing navigation was not reproduced in the real provider catalog; stale daemon configuration, startup failure, disabled permissions, or discovery/model selection remain possible. New discovery/registration tests must distinguish those from a wire omission.
- Accessibility serialization can escape punctuation/quotes/backslashes, and structural markup such as `[ref=...]` can match punctuation. This is a snapshot-line search, not proof of presence/absence in all rendered/hidden page text. A no-match must not be described as site-wide absence.
- Full hostile-page measurement, origin lists and per-run tab isolation remain outside this bounded repair.

## Validation Commands

- `cargo test -p jarvisd --lib mcp_host::browser::tests:: -- --include-ignored --nocapture`: **passed, 8 tests**, including the real pinned server on a synthetic loopback page (Windows Edge).
- `cargo test -p jarvisd --lib mcp_ -- --include-ignored`: **passed, 89 tests**, including the real browser check and existing MCP host/ability/admin/serving regressions. `TEMP` and `TMP` were set to a repository-local scratch directory for this command; it was removed afterwards.
- `rustfmt --check --edition 2024 apps\jarvisd\src\mcp_browser.rs apps\jarvisd\src\mcp_browser_tests.rs apps\jarvisd\src\mcp_host.rs apps\jarvisd\src\mcp_abilities.rs`: **passed**.
- Workspace `cargo fmt --check`: **not clean**, because concurrent unrelated edits were unformatted. No browser-owned file appeared in its diagnostics.
- `cargo clippy -p jarvisd --all-targets --all-features -- -D warnings`: **blocked** by an unrelated `jarvis-runtimes/src/acp.rs` Clippy diagnostic. A later daemon-only `--no-deps` run had no browser diagnostics but was still blocked by unrelated in-progress code (coding-task helpers, contacts/shared composition).
- Full workspace tests and cross-platform CI are left to the coordinating change; no owner website or owner browser state was used.

## Verification Log

| Date | Versions checked | Evidence | Researcher |
| --- | --- | --- | --- |
| 2026-10-14 | 0.0.83 / pinned Playwright alpha | Live official sources refreshed; cached CLI help and real stdio tools/list verified. | Copilot |
| 2026-10-14 | Same pins; Windows Edge, isolated/headless | Real synthetic loopback-page test passed through `compose_host` and its canonical executor: 25 registered tools, navigate, literal metacharacters, Unicode case-insensitive match, JavaScript-created text, explicit no-match, 500 matches bounded/fenced, and explicit close. First close attempt exposed retained composed adapter references; those are now dropped before host close. No owner site or normal browser profile was used. | Copilot |
