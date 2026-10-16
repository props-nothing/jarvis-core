---
integration: model-context-protocol
status: implemented
last_verified: 2026-10-15
status_reconciled: 2026-10-16
owners: []
selected_spec_version: "2026-07-28"
selected_sdk: "rmcp 3.4.0 (Apache-2.0, Tier 1)"
---

# Model Context Protocol

**2026-10-15 focused refresh:** live official index, stdio/authorization contract and pinned rmcp source/release were rechecked for `P11-001b(a)`; no SDK change required. The [stdio credential record](mcp-stdio-credentials.md) controls that slice, including GitHub v2.0.2, named keys and real-child verification. The subsequent owner-selected [HTTP token credential slice](mcp-http-token-credentials.md) rechecked HTTP authorization, RFC 6750 and the pinned per-operation backend: manually provisioned same-server bearer references are built/tested, not generic OAuth or real-provider acceptance. The next owner-selected [generic pre-registered OAuth slice](mcp-oauth.md) checked current discovery, client-registration and issuer/PKCE security contracts before implementation; it is built/tested offline, not automatic client registration or real-provider/browser acceptance. Older unrelated sections below retain their individual access dates; this refresh does not claim every protocol surface was revalidated.


## Current supported boundary

The MCP host and opt-in authenticated JARVIS server are implemented, with recorded real-server and independent-client evidence. The selected dependency remains rmcp 3.4.0 and the dated protocol research below remains version-specific. Actual negotiation supports the adapter's documented protocol eras; no unsupported draft/extension is claimed merely because the SDK contains it.

- Host: stdio and Streamable HTTP, bounded discovery/calls, canonical naming, operator-supplied effect policy, untrusted outputs and failure/degradation reporting. JARVIS policy remains authoritative.
- Credentials: same-server named child-environment/HTTP bearer references, persistent GitHub OAuth and generic pre-registered OAuth are built/tested. Follow the separate current records for [stdio](mcp-stdio-credentials.md), [HTTP tokens](mcp-http-token-credentials.md), [GitHub](github-oauth-settings.md) and [generic OAuth](mcp-oauth.md). Real-provider/browser acceptance is still incomplete; generic OAuth is not automatic DCR/CIMD or OIDC identity proof.
- Server: explicit owner enablement and authenticated scoped tool exposure; no transitive re-export of arbitrary third-party tools. Per-client/audience-bound remote credentials and a per-launch agent bridge remain distinct work.
- Lifecycle: registration changes still require explicit restart; rotating/removing a referenced HTTP credential affects subsequent requests, not already dispatched operations.
- Unimplemented protocol features are refused explicitly. Inspector/conformance and cross-SDK proof are recorded by version and tested surface, not universal certification.

See [MCP user setup](../../user/mcp.md), [external app access](../../user/connect-apps.md) and [active acceptance](../../../TODO.md).

## Historical contract and findings

The [complete original record](../../history/mcp-research-through-2026-10-16.md) preserves the original wire contract, SDK findings, rejected alternatives, unresolved questions and verification log. Use its [verified contract](../../history/mcp-research-through-2026-10-16.md#verified-contract), [implementation findings](../../history/mcp-research-through-2026-10-16.md#contract-facts-established-during-implementation) and [verification log](../../history/mcp-research-through-2026-10-16.md#verification-log) with their recorded dates. Earlier “not implemented/no caller” statements are historical, and no new external-source verification is claimed by this 2026-10-16 reorganization.

## Official Sources

| Source | URL/version | Accessed | Purpose |
| --- | --- | --- | --- |
| `llms.txt` index | https://modelcontextprotocol.io/llms.txt | 2026-09-22 | discovery; enumerates every spec revision + SEPs |
| Current specification | https://modelcontextprotocol.io/specification/2026-07-28/index.md | 2026-09-22 | normative contract |
| Key changes | https://modelcontextprotocol.io/specification/2026-07-28/changelog.md | 2026-09-22 | what changed vs 2025-11-25 |
| Versioning & compatibility | https://modelcontextprotocol.io/specification/2026-07-28/basic/versioning.md | 2026-09-22 | negotiation, modern/legacy/dual-era, compat matrix |
| Streamable HTTP | https://modelcontextprotocol.io/specification/2026-07-28/basic/transports/streamable-http.md | 2026-09-22 | headers, validation, sessions, resumability, cancellation |
| SDK list + tiers | https://modelcontextprotocol.io/docs/2026-07-28/sdk.md | 2026-09-22 | official SDK selection |
| SDK tiering rules | https://modelcontextprotocol.io/community/sdk-tiers.md | 2026-09-22 | what Tier 1 commits a vendor to |
| Rust SDK crate metadata | https://crates.io/api/v1/crates/rmcp | 2026-09-22 | exact version, license, MSRV, feature flags |
| Rust SDK API docs | https://docs.rs/rmcp/latest/rmcp/ | 2026-09-22 | transports, features, lifecycle modes |
| Rust SDK source | https://github.com/modelcontextprotocol/rust-sdk (`main`) | 2026-09-22 | negotiation behaviour, version constants |
| Deprecated features | https://modelcontextprotocol.io/specification/2026-07-28/deprecated.md | 2026-09-22 | features new implementations must not adopt |
| Separate docs `llms.txt` | `not found` — the single `llms.txt` above covers both docs and specification | 2026-09-22 | — |
| Playwright MCP README | https://github.com/microsoft/playwright-mcp/blob/main/README.md | 2026-10-11 | Current CLI documents `--output-dir <path>` for automatically named files and `PLAYWRIGHT_MCP_OUTPUT_DIR`; explicitly named files still resolve against the workspace root. |
| Playwright MCP pinned source | https://github.com/microsoft/playwright-mcp/tree/v0.0.83 (`config.d.ts`, `package.json`, `cli.js`) | 2026-10-11 | Selected package is Apache-2.0, Node >=18, delegates its CLI to pinned `playwright-core` `1.64.0-alpha-1790635538000`, and exposes `outputDir?: string` in its public configuration. |
| Playwright MCP release | https://github.com/microsoft/playwright-mcp/releases/tag/v0.0.83 | 2026-10-11 | `0.0.83` remains the latest release and the already-selected version; no dependency change is required. |

## Playwright output containment refresh (2026-10-11, `P11-003a`)

**Question.** Keep automatically named browser snapshots and screenshots out of the directory that started `jarvisd`, while preserving the exact-command check used before built-in browser tools are re-served to other agents.

**Verified facts.**

- The current official README documents `--output-dir <path>` as the directory for automatically named output files. An explicit filename is resolved against the workspace root and is not affected by this option.
- The pinned `0.0.83` package's public `config.d.ts` contains `outputDir?: string`; `package.json` pins the CLI implementation in `playwright-core` and requires Node 18 or newer.
- The package is Apache-2.0 and `0.0.83` is still the latest release. Its release notes do not deprecate output configuration.
- No credential or authentication changes. The browser remains an isolated, headless local child process with the existing environment allowlist.

**JARVIS decision.**

- Add `--output-dir <absolute profile runtime path>/playwright-mcp` to the built-in browser command. Playwright MCP owns its output directory; an output failure affects the browser rather than preventing unrelated MCP servers from starting.
- Migrate only the exact old built-in command. A server named `browser` with any owner-edited command remains untouched.
- Derive catalog commands from the active profile paths. `vetted_prefixes` compares against that same derived command, so migration cannot accidentally grant the built-in trust posture to a different process.
- The runtime directory is transient profile state. No browser artifact is copied into a granted project folder or the repository.

**Limits and unresolved questions.**

- Explicit filenames are outside `--output-dir` according to the official README. Those tools retain their existing policy and granted-root behavior; this slice fixes only the automatic `.playwright-mcp/` output observed in the daemon's working directory.
- The selected tag's README was generated before the current option table and does not name the CLI flag, although its public configuration contains `outputDir`. The selected package was therefore checked directly: `npx -y @playwright/mcp@0.0.83 --help` advertised `--output-dir <path>` on 2026-10-11.

**Falsifying tests.**

1. The pinned package help must advertise `--output-dir`.
2. A fresh seed must contain the absolute runtime output path.
3. The exact old seeded command must migrate once; an owner-edited browser command must not change.
4. The old command must cease to be vetted and the migrated command must remain vetted.

## Live findings against real servers (2026-10-10, `P11-001`, `ADR-0168`)

Until this date the client had only met scripted peers and a hand-written fixture. Connecting the official reference servers
(`@modelcontextprotocol/server-everything` and `server-memory`, both npm version `2026.8.31`, run with `npx` on Windows) found four
differences between the documents this record was written from and what is deployed. Each is now a decision and a test.

| What the spec says | What a real server did | Decision |
| --- | --- | --- |
| Modern clients probe `server/discover`; `basic/versioning` ("Backward Compatibility with Initialization-Based Versions") lets a client fall back, and for stdio gives the procedure | `server-everything` answers `server/discover` with `-32601 Method not found` and `initialize` with `protocolVersion 2025-06-18` | The client uses `ClientLifecycleMode::Auto` and records the era ([`McpEra`]). The earlier "legacy is a hard failure" rule made every shipped server unreachable. |
| Tool input schemas are JSON Schema 2020-12 unless `$schema` says otherwise (default dialect) | Every tool of both servers declares `http://json-schema.org/draft-07/schema#` (the TypeScript SDK emits it) | Draft-07 is read as 2020-12 **only where it means the same**: `dependencies` and `additionalItems` are refused by name, and tuple `items` / boolean `exclusiveMinimum` fail the 2020-12 metaschema check. Draft-04 and 2019-09 stay refused. |
| `tools/call` returns text and structured content | A result is text that can carry anything, including instructions | MCP output is fenced as untrusted data before the model sees it (as `jarvis.web.fetch` already was). |
| A stdio server is a child process of the client | `server-everything` has a `get-env` tool that returns every variable it was given | The child starts with an allowlisted environment (no keys, no `JARVIS_*`), checked live: a variable set in the daemon did not appear in the tool's output. |

Smaller facts recorded so they are not rediscovered:

- On Windows `npx` is a shell script next to `npx.cmd`; only the second can be started, and a bare name does not resolve from a child with a cleared working
  directory (`%~dp0` fails). The program is resolved to an absolute path when a server is added.
- `ClientLifecycleMode::Auto` waits up to 10 seconds for `server/discover` before the legacy `initialize`; JARVIS bounds both together at 20 seconds. A first `npx` run
  can download the package and exceed that; the second run is cached.
- `RunningService::is_closed()` reports only a cancellation; a child that exited needs `peer().is_transport_closed()`.
- The Rust SDK's own message for a failed fallback (`discover and legacy initialize both failed`) drops both causes; JARVIS keeps them.

### The JARVIS server, against the official TypeScript client (2026-10-10, `ADR-0170`)

The server half had the same blind spot: it had only met the Rust SDK's own test client. The official `@modelcontextprotocol/sdk` TypeScript client (the one most tools embed) connected to it as follows:

- **Refused at first:** the server accepted only protocol 2026-07-28, so the client failed with "Unsupported protocol version". It now also accepts 2025-11-25, 2025-06-18 and 2025-03-26 (legacy sessions); the client negotiated 2025-11-25.
- **Authentication is JARVIS's, not the protocol's:** a Bearer key is checked before the endpoint (401 otherwise). No OAuth is offered, so a client that insists on OAuth discovery is not supported yet.
- `tools/list` returned 26 tools (21 without a granted folder); `tools/call` of `jarvis.agent.delegate` returned a fenced result; a tool needing approval returned `mcp_call_cannot_hold_an_approval`.

Still unverified: Streamable HTTP against a real remote server, authorization (`401`, OAuth) for a remote server, `tools/list_changed`, and a server that needs a
credential in its environment (the allowlist deliberately gives it none; a deliberate route for one is open work in `P11-001`).
