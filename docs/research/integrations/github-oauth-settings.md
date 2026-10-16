---
integration: github-oauth-settings
status: researched
last_verified: 2026-10-16
owners: []
selected_spec_version: "GitHub OAuth web application flow with S256 PKCE; GitHub REST 2026-03-10; MCP 2026-07-28"
selected_sdk: "Existing reqwest =0.13.5, rmcp =3.4.0 and JARVIS AuthorizationTransaction; no new SDK"
---

# Persistent GitHub sign-in from Settings

## Authority-race fixture timing refresh (2026-10-16)

The full Windows workspace run exposed three authority-race fixture failures: held token and identity responses exceeded the unchanged 20-second HTTP deadline while a synchronous settings mutation hardened credential files, and the five-second revocation-observation watchdog included that mutation's filesystem work. Each Windows hardening invokes native permission tools; loaded-runner duration is not a provider response or evidence that a disconnected grant was restored. Production epoch checks, credential hardening and request timeouts remain unchanged.

Official test-runtime sources fetched on 2026-10-16:

- Repository index `https://raw.githubusercontent.com/tokio-rs/tokio/tokio-1.53.1/llms.txt` returned 404. The docs.rs pause/resume pages were unavailable through the reader; the selected source `https://raw.githubusercontent.com/tokio-rs/tokio/tokio-1.53.1/tokio/src/time/clock.rs` was fetched successfully.
- Tokio 1.53.1 `pause`/`resume` require its current-thread runtime and the `test-util` feature. Pausing freezes Tokio time, not `std::time::Instant`. Idle paused runtimes can automatically advance to timers, so the paused scope must not await pending network work. A guard resumes time when the local mutation returns or unwinds.
- `https://doc.rust-lang.org/1.98.1/std/future/fn.poll_fn.html` documents driving a future through a closure. Returning `Poll::Ready` around one inner poll observes readiness without yielding to a pending operation.

Fixture decision: freeze only the synchronous credential-mutation phase while a deliberately held callback response waits. Disconnect in that phase has no saved grant to revoke and must complete on its first poll; assert that invariant instead of allowing a paused network await. Resume before releasing the response. For the separate slow-revocation case, first-poll disconnect through its synchronous local invalidation, then start the bounded network-observation phase. Preserve exact refusal, disconnected status after reopening, request-count and local-before-revocation assertions. Only the daemon's test dependency enables clock control; no SDK version, production authentication, endpoint, response limit or timeout changes.

Verification on Windows: temporary 21-second blocking delays in the held-response mutation phases and a six-second disconnect startup delay reproduced all three original failures (20 passed, three failed). The token and identity mutations measured 23.056 and 23.696 seconds; the errors were respectively `Unreachable` and `IdentityFailed`. The six-second startup delay expired the five-second fixture observer. With the same controls and repaired fixture, all 23 tests passed in 48.82 seconds, including the unchanged refusal, persisted-state and request-count assertions. No delay or diagnostic control remains in the tests.

Falsification: temporarily accepting a changed epoch failed all three targeted tests: an extra identity request after code exchange, another during refresh, and a late successful grant exposed the removed guard. The production source was restored byte-for-byte (SHA-256 `B93409D350DB9F0480F640A4C20E816A98E1809A2A92306CD5FF1DDAFAB8A1FD`). The first restoration preserved the backup's older modification time, so Cargo reused the mutated binary; that failing log is retained as stale-build evidence. Advancing the source timestamp forced a fresh compile, after which all 23 unmodified-duration tests passed in 39.23 seconds; workspace formatting also passed. Evidence, controls and hashes are retained in `artifacts/reviews/coding-lifecycle-2026-10-16/github-race-controls/`. This is independent fixture evidence, not owner-account acceptance; full workspace verification remains in TODO.

## Scope

The owner requested simpler general setup instead of a token-only field and explicitly selected a persistent OAuth connection with one-time registration of a JARVIS GitHub OAuth app. First provider: github.com. Settings owns app setup, Connect, verified-account status, Test and Disconnect; normal users do not need a PAT, native helper, Docker or server command. Remote GitHub MCP uses that grant behind normal JARVIS policy. Other providers, generic MCP dynamic registration, GitHub Enterprise and publishing a shared OAuth app are outside this slice.

## Official sources (accessed 2026-10-15 UTC)

- Index: https://docs.github.com/llms.txt (official article/body API discovery).
- OAuth contract: https://docs.github.com/api/article/body?pathname=/en/apps/oauth-apps/building-oauth-apps/authorizing-oauth-apps (authorize, code exchange, S256 PKCE, expiring/non-expiring tokens, refresh rotation and loopback redirect rules).
- Registration: https://github.com/settings/applications/new and https://docs.github.com/en/apps/oauth-apps/building-oauth-apps/creating-an-oauth-app . App registration is performed by the owner, not fabricated by JARVIS.
- Verified identity: https://docs.github.com/api/article/body?pathname=/en/rest/users/users (`GET /user`, bearer authentication; login/id from GitHub, not user-entered labels).
- Revocation: https://docs.github.com/api/article/body?pathname=/en/rest/apps/oauth-applications (`DELETE /applications/{client_id}/token`, app client credentials plus the access token; local deletion remains required even if revocation fails).
- GitHub MCP: https://raw.githubusercontent.com/github/github-mcp-server/v2.0.2/README.md (host needs its own GitHub App/OAuth App for remote OAuth; PAT is an alternative, not a requirement).
- Remote read-only endpoint: https://raw.githubusercontent.com/github/github-mcp-server/v2.0.2/docs/remote-server.md (`https://api.githubcopilot.com/mcp/readonly`; hosted tools may evolve independently of the pinned reference docs).
- Native helper alternative: https://raw.githubusercontent.com/github/github-mcp-server/v2.0.2/docs/oauth-login.md (official local binary can open browser with its baked-in app, but keeps tokens only in memory and loses them at restart; rejected for the chosen persistent UX).
- MCP index/authorization: https://modelcontextprotocol.io/llms.txt and https://modelcontextprotocol.io/specification/2026-07-28/basic/authorization.md . This is a provider-specific, pre-registered GitHub OAuth grant, not an implementation of generic MCP DCR/PRM discovery.
- Selected SDK HTTP backend: https://raw.githubusercontent.com/modelcontextprotocol/rust-sdk/rmcp-v3.4.0/crates/rmcp/src/transport/streamable_http_client.rs (backend trait accepts an auth header per HTTP operation; preserve max SSE event-size hooks).
- SDK release: https://api.github.com/repos/modelcontextprotocol/rust-sdk/releases/tags/rmcp-v3.4.0 . No SDK upgrade or relevant credential-transport deprecation observed.

## Verified contract

1. Browser request: `GET https://github.com/login/oauth/authorize`, client ID, exact loopback redirect, random state, S256 code challenge and explicit scopes. The code expires after ten minutes. State and code are not tool/model data.
2. Exchange: `POST https://github.com/login/oauth/access_token` with `client_id`, `client_secret`, `code`, `redirect_uri`, `code_verifier`; request JSON with `Accept: application/json`. A success HTTP status alone is insufficient: a JSON OAuth error still means refusal. Reject missing/invalid bearer tokens and validate granted scopes.
3. Verify the selected account using `GET https://api.github.com/user`, API version `2026-03-10`, GitHub JSON Accept and a bounded User-Agent. This returns the provider's login and numeric identity. Do not accept an unverified browser account label.
4. Persistent token shapes may include `expires_in`, `refresh_token`, `refresh_token_expires_in`. `offline_access` opts into expiry even if the app is not globally configured for it; access tokens last eight hours, refresh tokens expire after six months without use. Non-expiring tokens may still be returned; do not invent an expiry or assume refresh material always exists.
5. Refresh: POST the same token endpoint with `grant_type=refresh_token`, client ID/secret and refresh token. Persist the rotated pair atomically before returning the new access token. `bad_refresh_token` needs a new sign-in; an ambiguous refresh response must not silently reuse potentially consumed refresh material.
6. Disconnect: clear local grant and pending login so the account cannot supply tokens, then best-effort revoke with the documented app-token endpoint. Report revocation failure honestly and link GitHub's authorized-app settings; never restore local access on a network failure.
7. Use HTTPS, no redirects or ambient proxies for credential-bearing calls, bounded response bytes/time and fixed official endpoints in production. Test endpoint injection is constructor-only, not a user-configurable destination.

## JARVIS decisions

- Reuse existing `AuthorizationTransaction`, PKCE and constant-time state validation. Owner-authenticated setup starts a ten-minute, one-use transaction bound to current app settings. Public GET callback is inert without that transaction. No raw token/code/verifier/error body in normal responses, logs or UI.
- One-time app client ID and write-only client secret are kept in the active private profile. The local-distributed app secret is not a confidential-client security boundary; PKCE remains mandatory. No fake default app ID or borrowed editor credential.
- Default scope: `offline_access` for account/public repository reads; optional private-repository access adds `repo` with explicit explanation that GitHub's scope also permits writes even though this initial MCP connection exposes only read-only tools. Do not silently broaden scopes or enable write tools.
- Persist the grant atomically in private profile-managed storage, including app binding, verified account, scopes and expiry/refresh metadata. Fail closed for malformed/mismatched/expired records. Do not claim OS keychain encryption or race-proof/OS sandbox confinement.
- The remote MCP backend asks an async provider for a current token before each outbound HTTP request, including SSE/reconnect/cleanup. Fixed provider binding `github`, fixed endpoint `https://api.githubcopilot.com/mcp/readonly`; refuse another server/destination. Refresh/removal are effective on the next request, not just the next child restart. Keep no token in a connection recipe or status.
- The Settings connection seeds the validated remote server configuration; it never overwrites an unrelated server. First tool registration still needs explicit daemon restart; say "connected; restart to enable tools" rather than inventing hot apply. Test can validate the remote connection separately. Disconnect removes local access; stale catalog entries fail closed on their next call.
- Normal MCP tool validation, authentication, authorization, risk, approval, timeout and audit remain unchanged. Sign-in is not permission to write.

## Discriminating verification

Cheapest falsifier: a forged or replayed callback must cause zero token requests and no persisted grant. Additional tests: denied consent, wrong/expired state, changed app binding, missing scopes, malformed/oversized token and identity responses, failed identity lookup, restored grant after process restart, refresh rotation/expiry and disconnect under concurrent/in-flight refresh. HTTP tests prove current bearer on POST/GET/DELETE and maximum SSE event-size forwarding; wrong endpoint and missing grant send nothing. UI tests prove provider link is out of model context, write-only secret fields clear on both success/failure, no browser storage, honest saved vs verified status and no automatic restart.

Live acceptance requires registering the owner's OAuth app and clicking consent in their browser; it cannot be proven by a fixture or performed without their registered app credentials. No real provider call has been made in this slice. Windows/macOS/Linux native callback behavior and actual remote MCP OAuth-token acceptance stay open until separately run.

## Implementation verification (2026-10-15)

- `cargo test -p jarvisd --lib --locked github`: **27 passed, 1 ignored**. Includes 23 real local HTTP account/provider-seam cases and 4 authenticated-route/callback/configuration cases. Forged/replayed/expired/app-mismatched callbacks send nothing; refresh rotation, cancelled refresh, disconnect races and account binding are exercised. Actual bound listener port, not an unapplied saved port, is used for the callback.
- Pinned transport all-feature suite: **212 passed**, including current token before each HTTP operation, denial, redirects, literal redaction and SSE bounds/resumption/cleanup.
- `node --test scripts/test-hud-settings.mjs`: **60 passed**, including OAuth UI, write-only app setup, validated browser handoff, unsaved permission gating, stale-response/draft preservation, polling stop and no automatic restart.
- `cargo fmt --check` and strict locked all-feature workspace Clippy **passed**. Final `cargo test --workspace --all-features --locked --no-fail-fast -- --test-threads=4`: **2,991 passed, 0 failed, 2 ignored**. The initial parallel-run failures and their focused checks are recorded in [TODO](../../history/todo-owner-evidence-through-2026-10-16.md#oauth-first-settings-connections-2026-10-15); no unrelated cancellation behavior was changed. Related link check: 378 checked, 0 missing.
- No OAuth application registration, secret creation, live consent, real account data request or hosted provider call was performed. Local synthetic HTTP responses are contract evidence, not proof that a real GitHub account is connected.

## Unresolved questions / blockers

- No JARVIS GitHub OAuth application is registered in this task yet. App ID/secret are required once in Settings before real sign-in. Never request the secret in chat or invent a shared client ID.
- Hosted GitHub MCP tool availability depends on granted scopes, account/organization policies and the live service; recorded read-only endpoint behavior is not a promise about every future tool.
- Generic automatic OAuth setup for arbitrary MCP URLs remains separate; providers do not all support dynamic client registration.
