---
integration: mcp-oauth
status: researched
last_verified: 2026-10-15
owners: []
selected_spec_version: "MCP 2026-07-28; RFC 9728/8414/8707/9207/7636/7009; OIDC discovery 1.0"
selected_sdk: "Existing reqwest =0.13.5, rmcp =3.4.0 and JARVIS PKCE transactions; no new dependency"
---

# Generic remote MCP OAuth with pre-registered clients

## Scope

The owner selected generic browser OAuth next, and explicitly chose **one-time provider app registration followed by persistent sign-in**, rather than automatic registration. Implement remote MCP protected-resource/authorization-server discovery, a user-supplied expected issuer and registered client ID/optional secret, S256 PKCE, one-use local callback, resource indicators, persisted authorization, serialized refresh and fail-closed disconnect. Settings and the existing HTTP credential port are the entry points. This is a generic pre-registered OAuth client, not OIDC identity verification, a connector catalog, automatic DCR/CIMD, hosted JARVIS identity, hot registry replacement or an arbitrary OAuth proxy.

## Official Sources

All accessed live 2026-10-15 UTC before implementation.

| Source | Exact URL/version | Purpose |
| --- | --- | --- |
| Index | https://modelcontextprotocol.io/llms.txt | Selected versioned pages |
| Authorization | https://modelcontextprotocol.io/specification/2026-07-28/basic/authorization.md | Resource indicators, minimal scopes, issuer response validation, per-request bearer use and refresh |
| Discovery | https://modelcontextprotocol.io/specification/2026-07-28/basic/authorization/authorization-server-discovery.md | Challenge PRM, path/root well-known fallbacks, OAuth/OIDC metadata order and exact issuer |
| Registration | https://modelcontextprotocol.io/specification/2026-07-28/basic/authorization/client-registration.md | Pre-registration; credentials must belong to issuer; DCR deprecated; CIMD requires hosted HTTPS metadata |
| Security | https://modelcontextprotocol.io/specification/2026-07-28/basic/authorization/security-considerations.md | Mandatory S256 support advertisement, mix-up defense, transport and token secrecy |
| SDK auth source | https://raw.githubusercontent.com/modelcontextprotocol/rust-sdk/rmcp-v3.4.0/crates/rmcp/src/transport/auth.rs | Available SDK behavior/cost; default HTTP/discovery policy is not JARVIS's constrained policy |
| SDK per-request backend | https://raw.githubusercontent.com/modelcontextprotocol/rust-sdk/rmcp-v3.4.0/crates/rmcp/src/transport/streamable_http_client.rs | Existing async current-token HTTP credential port |
| SDK release | https://api.github.com/repos/modelcontextprotocol/rust-sdk/releases/tags/rmcp-v3.4.0 | Selected release; no dependency upgrade needed |
| Revocation | https://www.rfc-editor.org/rfc/rfc7009.html | Optional advertised revocation endpoint, authenticated form request, local removal first |

## Verified Contract

### Operations And Transport

An unauthenticated MCP endpoint can advertise `resource_metadata` in a Bearer `WWW-Authenticate` challenge. Prefer the parsed challenge URI when present; otherwise try `/.well-known/oauth-protected-resource` with the MCP path appended, then the root variant. Metadata must identify the intended resource and list its authorization servers. For the owner's selected issuer try OAuth metadata with path insertion, OIDC with path insertion, then OIDC path appending. Reject metadata whose `issuer` does not exactly equal the expected issuer. Bound response bodies/time and never use a successful HTML/error response as metadata. Do not silently choose a different issuer.

### Authentication And Authorization

Pre-register JARVIS at the provider with the exact IPv4 loopback callback displayed in Settings. Native distribution cannot treat a bundled client secret as a confidential-client security boundary. A client secret is optional and write-only; public clients use `none`, confidential registrations may use advertised `client_secret_basic` or `client_secret_post`. Credentials remain bound to profile, local server, resource, exact issuer, client configuration and frozen token destination.

Require `code_challenge_methods_supported` containing S256 before opening a browser. Reuse the JARVIS PKCE/state transaction. Record issuer before redirect. Both authorization and token requests carry `resource`; only explicitly requested scopes are used, never the entire advertised scope catalog. The callback compares an included `iss` exactly (no URL normalization), requires it when metadata advertises support, checks state and expiry, rejects duplicate/oversized/malformed parameters and consumes a transaction once. Reject errors before exchanging a code, without displaying provider error text. A successful callback/token exchange establishes an OAuth grant, not a verified account name or OIDC identity.

Token responses must contain a bounded RFC 6750 bearer, a compatible token type, valid lifetimes and sufficient granted scopes. Refresh reuses the same resource/client/issuer and never broadens scopes; preserve an existing refresh token only when the response legitimately omits a replacement. Remove the old grant durably before a potentially consuming refresh request; failure/cancellation requires sign-in, not replay of possibly spent refresh material. Do not automatically replay a tool effect on an HTTP 401/403 or silently increase scopes.

### Limits And Failure Semantics

Twenty-second HTTP operations, bounded metadata/token bodies and bounded URL/client/scope/token records; ten-minute in-memory sign-in transactions. No unbounded retries, arbitrary headers or model-triggered consent. Discovery/registration is not tool execution. A missing/expired/malformed/mismatched grant sends no bearer. OAuth revocation is best effort where advertised; local disconnect takes effect first and cannot be undone by late callback/refresh. Already dispatched calls/open streams cannot be recalled. No webhook/sync semantics are introduced.

### Data And Compliance

The configured resource and explicit issuer receive metadata requests; token destinations receive registered client credentials, authorization codes or refresh material. Models, normal status, logs, browser storage and normal errors never receive these values. Browser consent URLs are returned only to authenticated setup, not model tools, and must come from the validated selected issuer metadata. OAuth HTTP disables redirects, ambient proxies and HTTP retries, bounds bodies, rejects non-HTTPS/private-network destinations, and pins validated public DNS answers for each request. Loopback peers are test-constructor-only. Provider retention, residency, pricing and account scopes remain provider-specific. Private atomic profile files are OS-permission-protected, not OS keychain encryption.

### Versions And Deprecations

MCP 2026-07-28, existing dual-era MCP transport, rmcp 3.4.0 and reqwest 0.13.5 remain selected. Live current docs confirm preregistration support; DCR is deprecated and optional, CIMD needs an HTTPS metadata document. Neither automatic mechanism is part of this owner-selected slice. Existing GitHub OAuth and named bearer references remain unchanged.

## JARVIS Mapping And Decisions

- HTTP config selects `authorization = 'oauth:SERVER'`, same-server only. Remote token/GitHub/local environment/inbound access bindings remain distinct and cannot fall back to one another.
- Persist one issuer-bound client setup and grant per configured server in private profile records. Settings shows expected issuer, client ID, requested scopes, saved-grant state and generic Connect/Check/Test/Disconnect; it does not invent verified user identity.
- Use a generic request module with existing reqwest rather than enable a large SDK auth feature whose HTTP defaults/logging/persistence do not match this boundary. Reuse existing JARVIS PKCE/state and private storage, no copied implementation.
- Refresh resolves the current saved grant immediately before outbound MCP operations. Revocation/removal invalidates late work using an epoch/binding guard; configuration changes invalidate old grants. Sign-in never changes tool policy or auto-registers a new tool set without explicit restart.
- The owner supplies a trusted issuer URL and minimal scopes during the one-time setup. This also prevents an untrusted MCP challenge from silently choosing where a pre-registered client secret is sent. Discovery must confirm the issuer/resource match before using credentials.

## Rejected Alternatives

- Borrowing an editor/Claude account or transmitting a GitHub token to a discovered server: wrong audience and ownership.
- Automatic DCR/CIMD or an assumed hosted client identity: not selected by owner, incompatible with current local-only installation assumptions.
- Raw provider responses, secret-bearing status or account labels inferred from a bearer: unsupported identity claims/leakage.
- Retaining a usable old grant across ambiguous refresh or accepting late completion after disconnect: can resurrect or duplicate authority.
- Weakening policy, automatic broad scopes or automatic unknown-effect retry: OAuth is authentication, not permission.

## Verification Plan

Cheapest falsifier: a correct code/state with the wrong callback `iss` must cause zero token requests and persist no grant. Independent HTTP fixtures must inspect actual authorization/token parameters and prove:

- Challenge and path/root PRM discovery; OAuth/OIDC path fallback; issuer/resource mismatch, missing S256, private-IP/redirect/oversized metadata refusals.
- Pre-registered public/basic/post auth, exact callback/S256/resource, scopes, stale/replayed/denied/duplicate callbacks and no provider text leakage.
- Persist/restart, expiry, serialized refresh, optional refresh-token replacement, wrong client/resource/issuer, cancellation and disconnect races.
- Metadata-only authenticated setup routes and Settings controls; wrong credential cannot initiate consent, Test or Disconnect.
- Existing GitHub/named bearer/stdio/tool policy regressions, targeted mutation then restored source, formatting/lint/workspace tests.
- Explicitly opt-in live provider/browser acceptance remains open until performed with owner's registered app/account; fixtures do not prove all providers.

## Unresolved Questions

- No real provider app/issuer/account is selected yet; service-specific consent, redirect acceptance, scopes, quotas and live token lifecycle remain open.
- Some providers require CIMD, DCR, device flow or nonstandard metadata. This slice refuses with setup guidance rather than fabricating support.
- OS keychain encryption, hot registry refresh and native macOS/Linux evidence remain separate work.

## Verification Log

| Date | Versions | Evidence |
| --- | --- | --- |
| 2026-10-15 | MCP 2026-07-28; rmcp 3.4.0; reqwest 0.13.5 | Official index/versioned discovery, registration, security and pinned SDK source/release rechecked before implementation. No relevant change requiring an SDK upgrade observed; generic pre-registered path implemented and verified offline on Windows. |

### Implemented bounds and evidence

The first slice permits 16 live account services per profile; 16 explicit scopes (128 characters each, 512 aggregate); 256-byte client IDs; 1,024-byte access/refresh/client-secret values; 4,096-byte private serialized records; 8,192-byte callback targets; ten-minute pending transactions; 20-second OAuth HTTP operations and 64 KiB response bodies. OAuth endpoints must be query-free HTTPS with public pinned destinations; literal loopback is constructor-only test support. Connect/Test routes have a 60-second deadline. These constraints intentionally refuse incompatible providers rather than silently weaken the boundary.

| Exact command/check | Result |
| --- | --- |
| `cargo test -p jarvisd --lib --locked mcp_oauth` | **41 passed**: 30 independent wire/discovery/auth/network tests, account/grant/refresh/race/revocation tests, real fixture MCP host/provider integration and authenticated route tests |
| `cargo test -p jarvis-mcp-transport --lib --locked http_auth::tests` | **14 passed**, including same-server OAuth binding and existing GitHub/named-token HTTP guards |
| `node --test scripts\test-hud-settings.mjs scripts\test-hud-console.mjs` | **111 passed**, including 17 generic OAuth UI regressions and saved-token/GitHub/local backward compatibility |
| Callback issuer guard mutation | Replacing issuer mismatch rejection with false makes `wrong_issuer_state_replayed_or_duplicate_callback_never_exchanges_a_code` fail because a token request is observed. Original source restored; final tests pass |
| `cargo fmt --check` | Passed |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | Passed after fixing new integration-test lint findings |
| `cargo test --workspace` | **3,049 passed, 0 failed, 3 ignored** in the observed tree. Initial run found a new hardcoded URL placeholder violating the bundled HUD origin guard; placeholder fixed without weakening the guard, and full suite rerun successfully |
| Real provider/browser and native macOS/Linux | **Not run**; no owner app/account selected. Follow the setup guide and record actual consent, token refresh/revocation and MCP read/effect acceptance separately |

No dependency or daemon runtime was installed/restarted, no real credential was used, and no model/provider account call was made by the offline tests. This evidence does not establish automatic client registration, OIDC identity verification, universal service compatibility or live approval/write acceptance.
