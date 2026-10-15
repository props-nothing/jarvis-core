---
integration: mcp-http-token-credentials
status: researched
last_verified: 2026-10-15
owners: []
selected_spec_version: "MCP 2026-07-28 with existing legacy compatibility; RFC 6750 section 2.1"
selected_sdk: "rmcp =3.4.0; reqwest =0.13.5; no dependency changes"
---

# Named token credentials for remote MCP servers

## Scope

Owner-selected continuation of `P11-001b(b)`: explicitly provision a bearer token for a configured Streamable HTTP server, using existing private profile keys, CLI and Settings metadata. Rust local deployment. Preserve fixed GitHub OAuth and unauthenticated HTTP behavior. Generic OAuth discovery/registration, broker integration, automatic installation, hot registration and provider-side credential creation/refresh/revocation are outside this slice.

## Official Sources

All sources accessed 2026-10-15 UTC. Official source was inspected, not copied into implementation.

| Source | URL/version | Purpose |
| --- | --- | --- |
| MCP index | https://modelcontextprotocol.io/llms.txt | Current versioned documentation discovery |
| Authorization | https://modelcontextprotocol.io/specification/2026-07-28/basic/authorization.md | Authorization on every HTTP operation; no URI-query tokens; distinguishes provisioning from authenticated use |
| Streamable HTTP | https://modelcontextprotocol.io/specification/2026-07-28/basic/transports/streamable-http.md | POST/SSE framing and compatibility; modern sessions/standalone GET removed |
| Bearer standard | https://www.rfc-editor.org/rfc/rfc6750.html#section-2.1 | Authorization header and token grammar; general bearer use is independent of how the credential was provisioned |
| SDK transport source | https://raw.githubusercontent.com/modelcontextprotocol/rust-sdk/rmcp-v3.4.0/crates/rmcp/src/transport/streamable_http_client.rs | Per-operation HTTP backend, reconnect and SSE-size hooks |
| SDK HTTP backend | https://raw.githubusercontent.com/modelcontextprotocol/rust-sdk/rmcp-v3.4.0/crates/rmcp/src/transport/common/reqwest/streamable_http_client.rs | Per-request bearer headers, reserved-header refusal and selected reqwest implementation |
| SDK release | https://api.github.com/repos/modelcontextprotocol/rust-sdk/releases/tags/rmcp-v3.4.0 | Pinned release notes and auth/lifecycle fixes; no SDK upgrade |

## Verified Contract

### Operations And Transport

An MCP HTTP endpoint accepts JSON-RPC POST and can return JSON or a request-scoped SSE response. Retain existing dual-era negotiation, bounded discovery, pagination and calls. Existing legacy GET/reconnect/DELETE operations must receive the current authorization too; modern peers do not require those session operations. Forward the SDK's configured SSE event-size limits unchanged.

### Authentication And Authorization

Provisioning a token manually is not an implementation of the MCP OAuth authorization flow. For a server that accepts the owner's externally provisioned bearer, send it only in the Authorization header on every request. RFC 6750 permits alphanumeric token characters plus `-._~+/`, followed by optional trailing `=` padding. Never accept a raw token as a configuration binding or put it in a URL, argument, tool description, model prompt, normal error, status or log. Scope/expiry/audience are determined by the issuing provider, not by successful discovery. No webhook or OAuth callback is introduced.

### Limits And Failure Semantics

Retain existing connection/read/handshake/call deadlines, quotas and ambiguous-outcome rules. Bound tokens to 4,096 bytes. A missing/unreadable/malformed token refuses before the HTTP operation; it must not fall back to anonymous access or the GitHub account. No auth retry/automatic token refresh or replay of an unknown tool effect is added. A request already dispatched cannot be recalled by removing its key. Open streams already authorized may finish; subsequent outbound operations must resolve again and refuse.

### Data And Compliance

The explicitly configured endpoint receives the bearer, MCP arguments and ordinary MCP results. Profile files are OS-permission-protected, not encrypted or OS-keychain-backed. No new processor, broker, telemetry, retention policy or fixed provider price/quota is introduced. Existing HTTP policy disables ambient proxies and redirects. URL syntax checks are not DNS/private-range/rebinding confinement; local operator configuration remains a trust boundary. Literal token redaction is defense in depth, not protection against deliberate encoding or exfiltration by a server.

### Versions And Deprecations

Keep rmcp =3.4.0 and reqwest =0.13.5. Current MCP index and pinned SDK release rechecked: no relevant change to per-request bearer use observed. MCP 2026-07-28 removes standalone GET/session cleanup for modern peers; existing legacy compatibility still exercises them. Full OAuth discovery and registration remain separately scoped.

## JARVIS Mapping

- `transport.authorization = "mcp:SERVER:KEY"` selects an existing validated named reference. Only the configured server's own reference is accepted. The existing literal `github` continues to select its fixed OAuth binding and exact hosted read-only endpoint.
- The owner-written configuration explicitly binds reference, server and exact endpoint together. That endpoint is frozen in the connection recipe; outgoing URI changes are refused before resolving a secret. Editing trusted configuration and restarting creates a new explicit destination binding; tokens do not independently encode a destination ACL.
- Generic token endpoints must be validated, query-free URLs. HTTPS is required off literal loopback under the existing endpoint policy. No auth in URI query, userinfo, custom headers or child environment is accepted through this option.
- Reuse `HttpCredentialProvider` and the private profile key store. Resolve immediately before every POST, legacy GET/reconnect and DELETE. Retain all issued literal tokens for existing connection redaction, including rotated values, without caching them as credentials for execution.
- CLI accepts `--bearer-ref mcp:SERVER:KEY`, never a token value. Settings accepts the reference only. Existing hidden `jarvis keys set` provisioning, rotation/removal and reference/set/not-set status are reused. A status is stored metadata, not a live provider health check.
- Every discovered tool retains ordinary schema validation, unclassified default posture, risk, approval, timeout, idempotency and audit. A valid credential never classifies tools as read-only or authorizes their effects.

## Decisions

Extend the existing HTTP credential port rather than add a new HTTP client, SDK or generic arbitrary-header facility. Keep account OAuth, manually provisioned references, local child environment and inbound JARVIS access credentials separate. Test connects/lists/closes and does not run a model or tool. Host changes still require explicit restart; rotating/removing a named HTTP key applies on the next outbound operation without a daemon restart.

## Rejected Alternatives

- Relaxing the GitHub allowlist without introducing validated named references would make a provider-specific grant usable at unrelated destinations.
- Literal tokens in configuration/CLI or URL query parameters leak through normal metadata/history.
- Caching a resolved bearer in the connection recipe silently ignores rotation/removal.
- Automatically forwarding tokens on redirects, using ambient proxies, or retrying failed non-idempotent actions broadens authority without consent.
- Combining generic OAuth, a cloud broker and catalog/hot-registry changes would not be a small independently verifiable slice.

## Verification Plan

Cheapest falsifier: an otherwise-valid remote server referencing another server's key must be rejected before the provider is consulted or a socket request is sent.

- Config/API/CLI/Settings round-trip named references without values or changes to permission posture; reject literal, cross-server, stdio misuse and query-bearing authenticated destinations.
- Through the real HTTP backend, verify headers and fresh credentials on POST, legacy GET/reconnect/DELETE; refuse changed URI/redirect and missing/invalid/removed keys with no fallback request.
- Exercise real profile storage plus host/Test composition against a deterministic loopback MCP peer; rotate/remove the key while a connection exists and verify redaction of old/new token text and structured content.
- Preserve GitHub OAuth tests and unauthenticated HTTP/stdio tests.
- Falsify a guard by a controlled temporary mutation and show the targeted test fails; restore immediately.
- Opt-in live smoke: owner-selected HTTPS server and private token file, discovery only with no tool execution or returned account data. No provider call is allowed without that explicit opt-in.

## Unresolved Questions

- No owner-selected real remote account/server token has been supplied. Real provider acceptance, least-privilege scopes and provider limits remain unverified; this blocks a claim of live integration, not offline credential plumbing.
- Generic OAuth, token expiry discovery, external broker data/price decisions and no-restart registry replacement are future slices.
- OS keychain encryption, DNS rebinding confinement and non-Windows native evidence are not established by this change.

## Verification Log

| Date | Versions checked | Evidence | Researcher |
| --- | --- | --- | --- |
| 2026-10-15 | MCP 2026-07-28; RFC 6750; rmcp 3.4.0; reqwest 0.13.5 | Live official index/spec/release and pinned transport source checked before implementation. Named token implementation verified offline on Windows; no real-service call. | Copilot |

Final validation in the observed working tree:

| Command/check | Result |
| --- | --- |
| `cargo test -p jarvis-mcp-transport --lib --locked http_auth::tests` | 13 passed, including named binding, per-operation headers, redirects, legacy cleanup, rotation redaction and GitHub-bound regression checks |
| `cargo test -p jarvis-cli mcp::tests` | 15 passed |
| `node --test scripts\test-hud-settings.mjs scripts\test-hud-console.mjs` | 94 passed |
| `cargo fmt --check` | Passed |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | Passed after fixing new test-structure lint findings |
| `cargo test --workspace` | 3,007 passed, 0 failed, 3 ignored; includes profile/host/Test and authenticated admin-route integration tests |
| Cross-server guard mutation | Removing the reference/server comparison makes `invalid_named_bindings_never_resolve_credentials_or_send_a_request` fail; original bytes restored and final tests pass |
| Real remote HTTPS acceptance | Not run; discovery-only ignored test requires explicit owner endpoint/token opt-in |

These results prove the local credential path and regressions, not arbitrary-provider compatibility, provider scopes, full OAuth, native macOS/Linux behavior or every previously recorded acceptance gate.
