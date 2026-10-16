# ADR-0216: Local MCP credentials are named profile secrets, not inherited environment

**Status:** Accepted

**Date:** 2026-10-15

## Context

`P11-001b(a)` needs a credential-backed local MCP server. The owner selected GitHub for real-account acceptance. Existing stdio children deliberately start with an operational environment allowlist: the first real MCP server exposed inherited variables through a tool, so removing that boundary is not an acceptable integration strategy.

The live official stdio authorization contract specifies credentials from the environment, not the HTTP OAuth flow. [Dated research](../research/integrations/mcp-stdio-credentials.md) pins rmcp 3.4.0 and GitHub MCP Server v2.0.2 and records the source URLs before implementation.

## Decision

### Named, server-bound references

`[servers.env]` maps a credential variable to `mcp:<server>:<key>`. `McpSecretRef` validates lowercase ASCII component names, bounds them and rejects traversal or extra components. The reference's server must equal the configured server. Literal values, references to another server, case-colliding variable names and HTTP/env combinations are refused before a child is launched.

The transport accepts at most 16 mappings per server. Variable names must be portable credential names with a supported suffix, and runtime/interpreter/loader/JARVIS controls are refused. This is not a general environment editor.

### Storage owns values; the transport owns injection

The existing keys CLI gains `set`, `status` and `remove` for named MCP targets, without changing ordinary provider keys. Values come from hidden terminal input, an explicit file, a named environment variable or bounded piped stdin; not command-line argument values.

Storage maps references to private profile-managed files below `mcp-keys/<server>/<key>.key`. It bounds values to 1–4096 graphic ASCII bytes, writes privately and atomically, rejects symlinks/reparse points and returns generic diagnostics. Status reports only `set`/`not set`, without reading content or returning paths. This uses the current private-file secret backend, not a claim that `P11-015`'s OS keychain vault has shipped.

The MCP transport depends on a resolver port, not storage or a provider SDK. It resolves only the selected references immediately before spawn/restart and overlays those variables on the existing operational allowlist. The daemon supplies its own profile resolver; default connections have no resolver and refuse required secrets. An unavailable credential degrades its server, not the daemon or unrelated servers.

### Restart, redaction and policy

Connection restart recipes retain references/resolvers, not resolved values. Rotation/removal is rechecked before the next process starts. An already-running server retains its process environment: restart JARVIS to make rotation/removal effective immediately, and revoke the token at its provider if required.

Credential-bearing children use the SDK's explicit stderr builder override (`null`), not its inheriting convenience constructor. Known literal credentials are redacted from returned textual/structured tool results, tool definitions, server identity and surfaced peer errors. Redaction patterns remain on the connection, including earlier values during restart, so delayed results cannot disclose a previous credential. No value enters normal status/configuration responses or the key CLI's output.

This is not a DLP guarantee. A trusted child runs with the owner's privileges and can use its credential, read files or encode/exfiltrate data. Exact literal redaction cannot detect arbitrary encodings or secrets rendered into an image. Do not add untrusted programs.

Credential presence never grants tool authority. New servers retain the normal unclassified posture and approvals. No write is automatically allowed, merged or retried. Unknown external effects remain unknown.

### Management surfaces

`jarvis mcp add/test` accepts `--env-ref VARIABLE=mcp:SERVER:KEY`; named tests preserve the saved mapping. Admin JSON accepts validated `env` references and returns mapping plus presence state, never secret contents. The console's existing server card displays presence and retains mappings for Test. Initial key provisioning is through `jarvis keys`, not a new browser key editor. Server configuration changes still need an explicit restart; no half-applied registry rebuild is introduced.

## Consequences

- A real local service can be configured without exporting its token into the daemon environment or copying it into TOML.
- Missing credentials fail before spawn. Sibling servers do not receive each other's explicit credentials.
- Private profile files are still plaintext protected by OS permissions. Windows hardlinks are not detected; Unix hardlinks are refused. Checks are path-based, not race-proof confinement. Profiles must be owned by the trusted user and not writable by untrusted principals. This is neither a secret enclave nor an OS sandbox.
- Supported suffixes are intentionally narrow: `TOKEN`, `KEY`, `SECRET`, `PASSWORD`, `CREDENTIAL` (alone or following `_`). Other credential conventions require a separately tested extension.
- Remote OAuth, hot apply, credential import, token refresh and automated GitHub installation remain separate slices.

## Acceptance

Real-child tests prove intended-child injection, sibling isolation, pre-spawn missing-key refusal, stderr suppression, result/identity/schema/error redaction, and re-resolution on rotation/removal. A real-profile-storage-to-host test checks key status, connection success, removal and unchanged approval posture. CLI process tests and the console DOM test cover value-free management paths.

An ignored opt-in test verifies a selected native GitHub MCP v2.0.2 binary, discovers `get_me` and makes only that read with an owner-supplied private key file. It is not automatically run and never prints the returned account. The separate approved/denied disposable GitHub write remains an owner-run check. [The backlog](../history/todo-owner-evidence-through-2026-10-16.md#credential-backed-local-mcp-2026-10-15) records exact commands and results; no provider acceptance is implied by fixture success.

## Alternatives

- Ambient environment inheritance repeats a proven credential leak.
- Raw values or arbitrary secret-file paths in server configuration broaden what a status page or server reference can disclose.
- Storing tokens in command arguments exposes shell/process metadata.
- Caching values in recipes defeats removal/rotation checks.
- Adding remote OAuth and hot registry replacement at the same time obscures the local process/secret boundary.

## Revisit when

A named credential convention cannot be represented, a stronger secret provider is delivered under `P11-015`, a measured lifecycle requirement needs hot replacement, or live owner evidence requires provider-specific behavior. Preserve the reference/resolver interface and ordinary tool authorization when changing the adapter.
