# ADR-0219: Remote MCP tokens are named server-bound request credentials

Status: Accepted
Date: 2026-10-15

## Context

The owner selected reusable remote MCP token credentials as the next connector-ecosystem slice. Local stdio keys already use private profile references (`ADR-0216`); HTTP credentials previously supported only the fixed GitHub OAuth account (`ADR-0217`). A broad connector catalog cannot help when a remote server requires a bearer that the host cannot supply. This does not justify weakening GitHub's destination restrictions or building a cloud broker, generic OAuth and hot registration in one change.

Official protocol, RFC and pinned SDK evidence is recorded in [the HTTP credential research](../research/integrations/mcp-http-token-credentials.md).

## Decision

1. Reuse the existing HTTP credential-provider port. `transport.authorization` accepts either the existing `github` OAuth account binding or a validated `mcp:SERVER:KEY` reference owned by the same configured server. Literal tokens and arbitrary header maps are not configuration options.
2. The owner-written server configuration explicitly binds the reference, server name and normalized endpoint. Freeze that endpoint in the connection recipe and refuse a different outgoing URI before resolving a credential. Changing trusted configuration and restarting explicitly creates a new destination binding; a named key does not independently encode an endpoint ACL.
3. Generic token destinations are validated, query-free MCP URLs. Keep HTTPS off loopback, disabled redirects and disabled ambient proxies. This is not a new claim of DNS rebinding/private-address confinement or an OS sandbox.
4. Resolve from the active profile immediately before each outbound operation, including legacy GET/reconnect/DELETE. Do not cache the token for execution. Missing, removed or malformed tokens fail closed with no anonymous or GitHub-account fallback. Accept only bounded RFC 6750 bearer syntax. Existing connection redaction retains observed old/new literal values as defense in depth.
5. Provision, rotate and remove values through existing hidden `jarvis keys` inputs. CLI `--bearer-ref`, Settings add/Test and the server document carry references only; status returns reference plus `set`/`not set`. Stored metadata is not proof of provider acceptance. Private profile files are not encrypted or OS-keychain-backed.
6. Leave tool effects unclassified unless the owner explicitly states otherwise. Credentials do not authorize effects, bypass approvals, imply read-only behavior, cause unknown actions to retry, or automatically expose tools to inbound clients.
7. Configuration/tool registration still requires explicit restart. Credential rotation/removal affects the next outbound operation without restart; it cannot undo a dispatched request or close an already-authorized response stream. Removing a local key does not revoke it at the issuing provider.
8. Add only generic manually provisioned bearer support. Preserve GitHub OAuth, unauthenticated remote MCP and local stdio behavior. Full MCP OAuth discovery/registration, a connector catalog, broker data processing and atomic hot replacement remain separate slices.

## Consequences

A manually authenticated MCP service no longer needs a bespoke provider adapter. The owner must still obtain a valid least-privilege token, choose the correct destination and manage issuer-side expiry/revocation. Query-dependent endpoints and non-bearer header schemes are deliberately unsupported by this slice. A failing server does not prevent unrelated tools from loading.

Tests cover configuration/API/CLI/Settings metadata, real loopback HTTP headers, profile isolation, missing/rotated/removed credentials, wrong destinations, redirects, redaction and unchanged approval posture. A guard-removal mutation must make the cross-server test fail. Real-service acceptance requires explicit owner opt-in; synthetic wire tests do not establish that every remote provider works.

## Alternatives

- Removing GitHub's endpoint allowlist: rejected; provider account grants must never silently become generic credentials.
- Literal headers, values in TOML or URL tokens: rejected; ordinary configuration/history/status must not become a secret channel.
- A new client or SDK: rejected; the current per-operation backend already owns redirects, reconnects, event bounds and redaction.
- Generic OAuth or a required hosted broker first: deferred by the owner's choice of this smaller usable slice.

## Revisit When

A named service requires OAuth discovery, query routing, another header scheme, secret-manager storage or a no-restart lifecycle. Add the smallest separately researched contract and fail-closed tests rather than broadening this binding implicitly.
