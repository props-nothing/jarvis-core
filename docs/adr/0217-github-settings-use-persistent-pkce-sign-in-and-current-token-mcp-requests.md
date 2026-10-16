# ADR-0217: GitHub Settings use persistent PKCE sign-in and current-token MCP requests

**Status:** Accepted

**Date:** 2026-10-15

## Context

After the named MCP key slice, the owner asked for easier Settings setup, preferably OAuth rather than copying tokens or commands. The owner explicitly selected a persistent JARVIS OAuth app registered once, rather than the official local helper (whose OAuth tokens are memory-only and lost at restart) or a token-only field.

[Official research](../research/integrations/github-oauth-settings.md) records the GitHub web/code/refresh/identity/revocation contracts and pinned rmcp HTTP backend before implementation. This advances a provider-specific part of `P11-001b(b)`, not generic MCP OAuth discovery or dynamic registration.

## Decision

1. Add **Settings → Connections → GitHub**. One-time app setup saves the owner's GitHub OAuth client ID and write-only client secret in the private profile; later setup is Connect GitHub, browser consent, then an explicit restart when tool registration needs it. No PAT, Docker, downloaded helper or server command is required for this path. No default client ID is invented, and no editor/account credential is borrowed.
2. Rust owns the short-lived pending login, mandatory S256 verifier/state checks, exact loopback callback, verified provider identity, persisted grant, refresh and disconnect. Reuse the connector crate's authorization transaction rather than a second nonce/PKCE implementation. Public GET `/oauth/github/callback` is inert without a transaction started through the owner-authenticated API; replay/expiry/app changes must fail closed.
3. Bind saved authorization to the app and requested permissions. Default consent is persistent basic account/public repository access (`offline_access`); private repositories require an explicit scope option (`repo`, whose broad GitHub write capability is explained). The initial remote MCP endpoint remains read-only regardless of that provider scope. A sign-in does not approve an effect.
4. Persist token/refresh/expiry/identity metadata atomically and privately, not in browser storage, prompts, logs or ordinary status. Restore usable grants after restart, refresh before expiry and persist the rotated pair before use. Reject invalid/ambiguous refresh state rather than replaying a possibly consumed refresh token. Expiring and documented legacy non-expiring grants are distinct cases.
5. Settings and transport share one account service per profile so application changes, callback completion, refresh and disconnect cannot race separate caches into resurrecting authorization. Disconnect removes local access even if remote revocation fails, and reports that provider cleanup may still be needed.
6. Add a narrow HTTP account binding to MCP configuration: `authorization = 'github'` only for server `github` and the fixed HTTPS endpoint `https://api.githubcopilot.com/mcp/readonly`. Resolve the current token before every HTTP operation, including SSE reconnect and cleanup, preserving the SDK's raw SSE event-size bounds. Do not cache a token in a restart recipe or let changing a URL carry it to a different origin. Existing anonymous HTTP and stdio named-key behavior remain intact.
7. The Connect action seeds the validated read-only GitHub server without overwriting an unrelated `github` program. Existing registry/daemon startup semantics remain: the UI says when a restart is needed, never claims hot activation, and never restarts automatically. Test independently checks the remote handshake/listing without running a model or a tool.
8. Keep advanced named-key configuration available for services without OAuth. The new Connections page is a first provider slice, not a claim that every arbitrary MCP server now has zero-configuration OAuth. Existing Google setup remains reachable through its own panel and a shortcut.

## Consequences

- Ordinary reconnection uses a browser, not a personal token. Registering a GitHub OAuth app and saving its client settings once remains an owner prerequisite; shipping a publicly registered default is separate work.
- A distributed native OAuth app cannot treat its client secret as a confidential security boundary; mandatory PKCE and transaction validation still apply. The saved local secret is write-only and OS-permission-protected, not a claim of keychain encryption.
- Saved identity is labelled as a saved connection, not live health. Failed, cancelled, expired or missing-scope authorization cannot become a connected account by HTTP status alone.
- Read-only remote tools can be registered only after an explicit restart when new configuration is introduced. Disconnect/revocation still blocks the next outgoing token lookup even if a stale tool catalog remains visible.
- Provider endpoints are fixed in production. Loopback endpoint injection exists only in constructors/tests, not Settings.
- No generic DCR/PRM discovery, GitHub Enterprise, write-tool activation or automatic application registration is added.

## Acceptance

The implementation is accepted only with callback forgery/replay/expiry tests, real local HTTP token/identity/refresh failure fixtures, persisted grant restore and disconnect/concurrency checks; HTTP request tests prove current-token lookup, endpoint confinement and SSE bounds; console tests cover write-only secret clearing, validated out-of-band provider navigation, no browser storage, saved-versus-live status and no automatic restart. The callback and all mutation/status endpoints retain their separate public/owner-authenticated boundaries.

Exact commands/results and outstanding provider checks belong in [the acceptance ledger](../history/todo-owner-evidence-through-2026-10-16.md#oauth-first-settings-connections-2026-10-15). Live browser consent requires the owner's registered app and is not proven by mocks. Do not call the GitHub connection live-verified until that check and a remote read through the real provider pass.

## Alternatives

- A password field alone leaves the ordinary user managing PATs and environment mappings; keep it an advanced fallback rather than the primary path.
- The official native helper opens the browser without registering an app, but loses its token on restart and would own account lifecycle outside JARVIS.
- Reusing an editor's app ID/login or inventing an app registration is not an authorization model.
- Static bearer configuration until restart fails refresh/removal semantics for long-lived connections.
- Combining this with generic OAuth registration, writable GitHub automation or hot registry replacement makes the acceptance boundary unnecessarily broad.

## Revisit when

Another provider requires a reusable connection flow, generic MCP discovery is selected, a registered public JARVIS app is available, or a named workflow requires additional scopes/write tools. Preserve profile authority, current-token lookup, explicit scope consent and deterministic effect policy.
