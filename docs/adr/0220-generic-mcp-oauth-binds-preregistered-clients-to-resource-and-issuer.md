# ADR-0220: Generic MCP OAuth binds pre-registered clients to resource and issuer

Status: Accepted
Date: 2026-10-15

## Context

After reusable remote token references (`ADR-0219`), the owner selected generic browser OAuth. The owner explicitly chose one-time registration of an OAuth app with each service rather than automatic registration or a hosted JARVIS identity service. The fixed GitHub account (`ADR-0217`) cannot be generalized by forwarding its grant to another provider. Every additional connection must keep Rust policy, approvals and canonical state authoritative.

Official versioned discovery, issuer/resource, PKCE and token contracts were checked before implementation; see [the research record](../research/integrations/mcp-oauth.md).

## Decision

1. A remote server selects `authorization = 'oauth:SERVER'`. This must name the same configured server and a query-free MCP endpoint. The fixed GitHub account, named bearer references, stdio environment credentials and inbound JARVIS access key remain separate, with no fallback between them.
2. The owner registers an app at the service and saves the expected issuer, client ID, optional write-only client secret and minimal scopes in Settings. Saving setup does not establish connectivity. Support public clients (`none`) and advertised secret Basic/post methods, without claiming a local app secret is a confidential-client security boundary.
3. Discover protected-resource metadata from a parsed Bearer challenge or the path/root well-known locations. Confirm the exact resource and explicitly selected issuer. Discover issuer metadata in the specified OAuth/OIDC path order, reject issuer mismatches and require advertised S256 support. Do not select an arbitrary issuer, adopt the entire scope catalog, or send credentials during discovery.
4. OAuth HTTP requests enforce HTTPS/public network destinations, validate and pin DNS answers per request, disable ambient proxies, redirects and retries, and bound operations and bodies. Literal loopback exceptions are test constructors only. Existing MCP transport network behavior is unchanged; this is not a claim that its older endpoint layer gained DNS confinement.
5. Keep one short-lived, single-use S256/state transaction per server in memory. Include the resource in authorization and token requests. Compare an included callback issuer exactly and require it when advertised. Reject malformed/repeated/expired/wrong-state/wrong-issuer/provider-error answers before token exchange. Callback pages are inert without pending state, do not echo provider inputs, and use no-store/referrer/CSP restrictions.
6. Persist client setup and an issuer/resource/client-bound grant in private atomic profile records. These are permission-protected files, not keychain encryption. Do not invent a verified username or OIDC identity from an opaque OAuth token. Status describes saved metadata and a usable local grant, not provider health.
7. HTTP credential consumers and Settings share a profile-scoped account service. Resolve immediately before outbound MCP operations; serialize refresh, freeze validated endpoints, recheck discovery metadata before refreshing and never silently broaden scopes. Durably delete old grant material before a refresh can consume it. Ambiguous/failed/cancelled refresh requires sign-in rather than retrying an old refresh token. Revalidate the setup epoch before saving late results.
8. Disconnect invalidates local access and pending work first, preserves registered app setup, then attempts advertised provider revocation. Failure reports that issuer-side revocation was not confirmed and does not restore local access. A late callback or refresh cannot resurrect access after disconnect or app changes. Already dispatched calls/open streams cannot be recalled.
9. Settings → Advanced → Programs offers Browser OAuth, one-time setup, Connect, Check sign-in, Test tools and Disconnect. Secrets clear before a save request and never enter browser storage. Browser navigation is limited to the returned validated authorization endpoint. First registration of tools still requires explicit restart; connecting never restarts the daemon automatically. Every tool retains normal schema/policy/approval/idempotency/audit gates.
10. Automatic DCR/CIMD, OIDC identity verification, device flow, hosted client metadata, provider catalogs and atomic hot tool replacement are outside this owner-selected slice. Refuse incompatible metadata or registration requirements with setup guidance, rather than claim universal OAuth support.

## Consequences

The generic pre-registered path reaches compatible remote services without a bespoke account adapter. The owner must still supply the correct issuer/registered client and scopes; each provider may enforce its own redirect restrictions, consent, quotas and retention. Strict endpoint and response bounds can refuse otherwise-valid providers; widen only through a separately researched contract/test.

The initial limits are 16 account services per live profile, 16 scopes with 512 aggregate characters, 1,024-byte access/refresh/client-secret values, 4,096-byte serialized private records, 8,192-byte callback targets, ten-minute pending sign-in, 20-second HTTP operations and 64 KiB HTTP bodies. Setup/connect/Test routes retain the gateway's owner authentication and body limits; Connect/Test have a 60-second deadline. Incomplete discovery, storage failures or unsupported auth do not grant authority.

## Verification

Independent loopback wire fixtures exercise discovery, public/Basic/post token requests, resource/S256/issuer checks, private-network/redirect/body limits and masked errors. Account tests exercise persistence, concurrent refresh, omission of a replacement refresh token, cancelled refresh, client-change/disconnect races and a real MCP host connection through the shared credential provider. Router tests require the owner credential and prove forged public callbacks inert; Settings DOM tests cover secret clearing and safe provider navigation. Removing the callback issuer guard makes its no-token-request test fail; restore it before final regression checks.

Real provider/browser consent and native macOS/Linux checks remain unverified. No full-OAuth or connector-count claim follows from fixture success.

## Alternatives

- Reuse a provider-specific/GitHub/editor token: rejected for wrong ownership/audience.
- Automatic DCR/CIMD: not selected; DCR is deprecated and CIMD needs a real hosted client-metadata document.
- Generic SDK auth defaults/persistence: rejected for this slice; reuse existing reqwest, PKCE transaction and private store behind a small HTTP module with explicit network policy.
- Automatically refresh on every 401 and replay the tool: rejected; a transmitted non-idempotent effect may already have happened.

## Revisit When

A named provider requires automatic client registration, another grant/auth method, different endpoint policies or stronger credential storage, or measured account churn requires lifecycle eviction. Preserve explicit owner consent and fail-closed resource/issuer/epoch binding when extending the contract.
