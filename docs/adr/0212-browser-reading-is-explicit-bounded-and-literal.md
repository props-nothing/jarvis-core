# ADR-0212: Browser reading is explicit, bounded and literal

Status: Accepted
Date: 2026-10-14

## Context

The owner requested a usable browser with an explicit navigate operation and dependable fixed-string text checks. The selected Playwright MCP 0.0.83 really lists both `browser_navigate` and `browser_find`: the reported missing navigation was not reproduced at the provider wire boundary. However, its navigation description does not say open/read website, and find's JSON Schema allows an empty query, regex and file-output arguments. The handler rejects some combinations only after execution. That broad contract is unnecessary for ordinary website reading.

Official sources and limits are recorded in [playwright-mcp.md](../research/integrations/playwright-mcp.md). The existing authority boundary is unchanged: Rust policy decides whether a tool may run; browser content is untrusted data.

## Decision

1. Keep the existing MCP server, connection, canonical tool identifiers and owner permission settings. `mcp.browser.browser_navigate` explicitly opens a page; `mcp.browser.browser_find` is the fixed/literal text check. Support the owner's configured naming strategy as before.
2. For the built-in pinned browser command, narrow find's schema to required, nonblank `text`, at most 512 characters, with no regex, scripts or output filenames. Forward the authorized arguments unchanged to Playwright's literal, case-insensitive `text` operation.
3. Narrow navigate to required, credential-free HTTP(S) `url`, at most 8,192 characters. Reject script/data/file schemes, malformed URLs and embedded credentials before calling the remote executor. Do not add origin privileges or exempt the request from policy.
4. Register these same narrowed definitions for local discovery and the vetted remote MCP surface. Revalidate before dispatch. Preserve effects, risk, scopes, approval rules, timeouts, retries, source and existing bounded/fenced result/error conversion.
5. Do not modify owner-customized browser commands or silently grant their tools the built-in posture. This schema-recognition check is not the existing exact-profile vetting/trust check, which remains separate.
6. Release the composed host's adapter references before explicitly closing its connections. Real-server verification found that retaining these references prevents the transport's explicit close operation.

## Consequences

- Existing permissions and audit identities continue working. No new browser engine, JavaScript evaluator, dependency, process topology or policy-engine change is introduced.
- A bare regex query, an empty find or a find with a filename now fails before a provider call. Owners needing arbitrary scripts still use the existing acting tools under their existing approvals.
- Find checks **serialized accessibility snapshot lines**, not every rendered/hidden text source or the whole website. Snapshot markup/escaping can affect a match. No-match must not be described as site-wide absence.
- Results remain bounded; a shortened result does not display every match. No implicit filesystem read follows an output-file reference.
- The complete 25-tool real server catalog, navigation, literal metacharacters, Unicode case-insensitivity, JavaScript-generated content, explicit no-match, bounded/fenced output and clean connection close were verified on a synthetic loopback page on Windows. This is not an owner-site test or the pending measured hostile-page evaluation.
