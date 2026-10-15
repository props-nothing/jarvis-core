# Connections: sign in instead of pasting tokens

[Documentation](../README.md) / [Settings](settings.md) / [Advanced MCP setup](mcp.md)

Open **Settings → Connections → GitHub**. This is the account connection JARVIS uses, not **Connect apps**, which lets another AI app use JARVIS.

## GitHub app setup, once

A real GitHub OAuth app must be registered before browser sign-in works. This slice does not ship a shared client ID or create an app automatically.

1. Open **One-time JARVIS app setup** and follow **Register a GitHub OAuth app**. Use a name such as JARVIS and your appropriate application homepage.
2. Copy the exact **Authorization callback URL** shown in Settings into the GitHub app registration. It is `http://127.0.0.1:<your daemon port>/oauth/github/callback`; the usual port is 8765.
3. Copy the app's **Client ID** and generate its **Client secret** in GitHub. Enter both in Settings and press **Save GitHub app**. Do not paste the secret in a chat or commit it to code. The password field clears when submitted, including on failure; saving never returns the secret.
4. Leave **Private repositories** off for basic account/public repository reads. If you need private repositories, opt in and save: GitHub's `repo` scope is broad and also grants writes at the provider. This initial JARVIS connection still exposes only read-only tools.

The client secret is app setup, **not** a PAT to regenerate for each sign-in. It is saved in the private profile. When editing the same app, an empty secret field preserves the existing secret; changing the app needs its new secret and a new account authorization. A scope change also needs new consent.

## Connect

Press **Connect GitHub**, complete GitHub's page in your browser and return to Settings. Your password stays with GitHub. If the browser blocks the new tab, use **Continue on GitHub**. **Check connection** reloads saved status.

JARVIS verifies the account with GitHub before saving its authorization. A successful browser callback alone is not a connected account. Cancelled, stale, replayed, mismatched-app or missing-permission answers are refused.

The connection survives an ordinary daemon restart. Access tokens and refresh material stay in the private profile; expiring grants are refreshed when needed. Lost/revoked/expired refresh permission requires a new sign-in rather than silently dropping down to another credential.

When a first connection changes the registered tool set, Settings says **Restart JARVIS to register its tools** and enables the normal restart footer. Restart is explicit; connecting does not restart or stop running work automatically. No server command, Docker or local GitHub MCP installation is needed: this path uses GitHub's hosted read-only MCP endpoint.

## Test and disconnect

- **Test GitHub tools** independently connects and lists remote tools. It does not run a tool or model, change a repository, or restart JARVIS. Saved account status alone is not a live health check.
- **Disconnect GitHub** removes local access and pending sign-in, while retaining app setup for later. JARVIS also attempts provider revocation. If that fails, local access remains removed; check [GitHub's authorized OAuth apps](https://github.com/settings/applications) to revoke it there.
- A stale catalog may remain visible until restart, but its next credential-bearing request must fail after disconnect. Sign-in does not waive normal policy, approvals or tool limits.
- A custom program already named `github` is not silently overwritten. Remove or rename it under **Advanced → Programs** before selecting this OAuth connection.

## Boundaries and verification

The OAuth grant is bound to the selected profile/app and the exact hosted GitHub read-only destination. JARVIS does not forward it to arbitrary URLs, models, other MCP servers, browser storage or normal status output. It uses S256 PKCE and a one-use, short-lived local callback transaction. Private profile files are OS-permission-protected, not a claim that keychain encryption has shipped; keep the profile under the trusted user's control.

Other services do not inherit this GitHub grant. For a compatible remote MCP service, **Settings → Advanced → Programs → Browser OAuth** now supports a separate [pre-registered OAuth app and persistent browser sign-in](mcp.md#a-remote-server-with-browser-sign-in). Supply that service's expected issuer, client ID/optional secret and minimal scopes; do not reuse GitHub or an editor's credentials. This generic path has offline fixture evidence, not real-provider/browser acceptance, and does not automatically register clients.

Existing Google sign-in remains under **Settings → Google**, also linked from Connections. Advanced local MCP servers can use [named environment credentials](mcp.md#a-local-server-that-needs-a-key); remote servers that accept an externally provisioned bearer token can use [named HTTP credentials](mcp.md#a-remote-server-that-needs-a-token).

Provider integration/live-consent evidence and exact local test results are tracked in [the acceptance ledger](../../TODO.md#oauth-first-settings-connections-2026-10-15). Until a registered app and the owner's browser consent have been exercised against GitHub, this is not a claim that the owner's account is already connected. Rebuild/start the updated daemon to see the new Settings page; no existing daemon is restarted by a source edit.
