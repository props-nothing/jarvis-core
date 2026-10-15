# MCP: Extend Your Assistant

**Bring an existing tool server. Keep JARVIS in charge of its effects.**

[Documentation](../README.md) / [Built-in abilities](abilities.md) / [Connect apps](connect-apps.md) / [Settings](settings.md)

Most people never need this page: the browser and documentation lookup come built in and appear under **Settings, Permissions** like any other tool ([abilities.md](abilities.md)).

This is for adding **your own** program that speaks [MCP](https://modelcontextprotocol.io). Adding one runs that program as you, so only add programs you trust. Its tools **ask before every call** until you decide otherwise in Settings, Permissions.

```text
jarvis mcp add files -- npx -y @modelcontextprotocol/server-filesystem C:\Users\you\Documents
jarvis mcp add docs --read-only -- some-program --flag   # your statement that it only reads: its tools run without asking
jarvis mcp add remote --url https://host/mcp             # a remote server (HTTPS)
jarvis mcp test files            # connects once, shows what it offers and what it left out and why; saves nothing
jarvis mcp list
jarvis mcp remove files
jarvis restart                   # a change applies on the next start
```

By hand, a program is an entry in `mcp-servers.toml` next to `config.toml`. A server that mixes reading and acting can name the tools that only read, by the server's own names; every other tool keeps asking:

```toml
[[servers]]
name = "mixed"
[servers.transport]
kind = "stdio"
program = 'C:\Program Files\nodejs\npx.cmd'
args = ["-y", "some-package@1.2.3"]
[servers.posture]
read_only_tools = ["search"]
```

What to know: a program starts with a **small operational environment**, plus only the credentials you explicitly map below. What it returns is **untrusted data**; a program that stops is started again on the next call; `jarvis mcp list` shows what each offers and why a tool was left out. Remote servers can use a [named bearer token](#a-remote-server-that-needs-a-token) or [browser OAuth with a registered app](#a-remote-server-with-browser-sign-in). Automatic OAuth client registration and applying server configuration without a restart are not built.

## Easier GitHub setup: browser sign-in

For GitHub, prefer **Settings → Connections → GitHub**. Register a JARVIS OAuth app once in the expandable setup section, save its client ID and write-only client secret, then use **Connect GitHub**. The account authorization persists and refreshes when needed; no PAT, native helper, Docker or server command is required. This first connection exposes only the fixed hosted read-only tool set. See [Connections](connections.md) for registration, permissions, Test, Disconnect and outstanding live verification. The daemon still asks for an explicit restart when its tool registry needs updating; there is no automatic restart. Other compatible services use the separate [pre-registered OAuth path](#a-remote-server-with-browser-sign-in) below, not this GitHub grant.

The named-key path below remains available for local servers without OAuth.

## A local server that needs a key

Keep the value out of TOML, commands and chat. Give it a **named reference** for that server:

```powershell
jarvis keys set mcp:github:token       # hidden terminal prompt; never put the token after this command
jarvis keys status mcp:github:token
```

For noninteractive setup, use `--file` with a private file, `--from-env` with an existing variable name, or piped stdin. These are input sources for the keys command, not an instruction to inherit that variable into every server. Do not paste a token into an issue, chat or configuration example.

Then add the server with a reference. For **GitHub**, obtain the official native [GitHub MCP Server v2.0.2](https://github.com/github/github-mcp-server/releases/tag/v2.0.2), verify its release checksum, and put its executable at an explicit path you control. This first configuration exposes only a read, not issue creation:

```powershell
jarvis mcp add github --env-ref GITHUB_PERSONAL_ACCESS_TOKEN=mcp:github:token -- 'C:\Tools\github-mcp-server.exe' stdio --read-only --tools=get_me
jarvis mcp test github
jarvis restart
```

Use the corresponding native executable path on macOS/Linux. This does not install Docker or download a program automatically. The PAT must belong to the intended account and have only the permissions needed for the intended repository/operation; JARVIS does not create it or reuse an editor's GitHub login.

The equivalent server document contains metadata only:

```toml
[[servers]]
name = 'github'
[servers.transport]
kind = 'stdio'
program = 'C:\Tools\github-mcp-server.exe'
args = ['stdio', '--read-only', '--tools=get_me']
[servers.env]
GITHUB_PERSONAL_ACCESS_TOKEN = 'mcp:github:token'
```

- Each reference is `mcp:SERVER:KEY`; components start with a lowercase ASCII letter and contain at most 64 lowercase letters/digits/`_`/`-`. A server cannot use another server's reference.
- Repeat `--env-ref` for multiple credentials (up to 16). Values are never accepted here. Portable credential variable names must end in `_TOKEN`, `_KEY`, `_SECRET`, `_PASSWORD` or `_CREDENTIAL` (or be that word); operational/interpreter/loader/JARVIS control variables are refused.
- Keys use private profile files, **not encryption or an OS keychain**. Status in `jarvis mcp list` and Settings → Advanced → Programs shows reference and `set`/`not set`, not a value or file path. Provisioning is through `jarvis keys`; the browser Test button preserves your references.
- A missing, empty or unreadable required key prevents that server starting. Other servers and the assistant remain available. New tools still ask according to the ordinary permission policy; a credential is not an approval.
- Replace a key with the same `keys set` command; remove it with `jarvis keys remove mcp:github:token`. JARVIS resolves it again when the child next starts. **Restart JARVIS to stop a running child using its old environment immediately.** Revoke the PAT at GitHub when appropriate. Removing a server does not silently delete its named keys.
- Credential-bearing child stderr is suppressed, and known literal credentials are redacted from surfaced textual/structured results, tool descriptions, identity and peer errors. This is not a sandbox or a guarantee against a program encoding/exfiltrating a key. Add only trusted programs; keep the profile private. Windows hardlinks are not detected and file checks are not race-proof confinement.

## A remote server that needs a token

For a trusted Streamable HTTP server that accepts a bearer token, obtain a least-privilege token from its provider. Replace the example address with that server's documented MCP endpoint. JARVIS does not create the token or connect a cloud broker automatically.

```powershell
jarvis keys set mcp:remote:token
jarvis mcp add remote --url https://service.example/mcp --bearer-ref mcp:remote:token
jarvis mcp test remote
jarvis restart
```

The first command uses a hidden prompt. Only the **reference**, never the token value, belongs after `--bearer-ref`. Existing `keys set --file`, `--from-env` and piped-input options remain available. Test connects, lists tools and closes; it does not run a tool or model. A new server's tools still ask before running. Sign-in or a valid token is not permission to act.

The equivalent configuration contains metadata only:

```toml
[[servers]]
name = 'remote'
[servers.transport]
kind = 'http'
endpoint = 'https://service.example/mcp'
authorization = 'mcp:remote:token'
```

**Settings → Advanced → Programs** also accepts an optional **Bearer key reference** for an HTTP(S) address. Provision the value through `jarvis keys set`; the browser form accepts references only. Test preserves the selected reference. Status shows `Authorization`, the reference and `set`/`not set`, not a value or key-file path. `set` describes stored metadata, not proof that the provider accepts it. GitHub browser sign-in remains a separate option under Connections.

- The reference must belong to the same configured server. Token destinations must be query-free; HTTPS is required outside loopback. Put no token in a URL, command argument or chat. Values must fit the provider's bearer-token format (at most 4,096 bytes), not an entire `Bearer ...` header or JSON credential record.
- The configuration explicitly binds the named key to the normalized server address. Every outbound request uses that connection's destination; redirects and ambient proxies are disabled. Editing trusted configuration and restarting explicitly selects a new destination, so verify the address before assigning the reference. These checks do not establish DNS/private-network confinement.
- Rotate with `jarvis keys set mcp:remote:token`; remove local access with `jarvis keys remove mcp:remote:token`. The next outbound HTTP operation resolves again without a daemon restart. Missing, unreadable or malformed keys refuse before sending, with no anonymous fallback. Already-dispatched requests and open response streams cannot be recalled by removing a key.
- Keys remain private profile files, not OS-keychain encryption. Tokens and tool data go to the selected server. Choose a trusted service and minimum provider permissions. Expiry, provider-side refresh/revocation and quotas remain the provider's responsibility; this slice adds no generic OAuth flow or automatic token refresh.
- Removing the server still requires restart to update the tool registry and does not delete its key. Remove the key separately when appropriate, and revoke it with the issuing provider when needed.

## A remote server with browser sign-in

For a compatible public HTTPS MCP service, **Settings → Advanced → Programs** now offers **Remote authorization → Browser OAuth**. This path uses a one-time OAuth app registration with that service, then persistent browser sign-in. It does not reuse Claude, an editor's login or JARVIS's GitHub authorization.

1. Enter the server's name and documented MCP address, select **Browser OAuth**, then add it. The saved server card shows its exact **Callback URL**, including the daemon port and server name.
2. Register an OAuth app with that provider using that exact callback. Follow its native/local application instructions. Obtain the issuer URL from its official documentation and choose the minimum scopes needed. Public clients may have no client secret; JARVIS also supports advertised client-secret Basic/post authentication.
3. Expand **One-time OAuth app setup**. Enter **Expected issuer URL**, **Client ID**, optional write-only **Client secret**, and space-separated **Scopes**. Press **Save OAuth app**. An empty secret preserves the same issuer/client's existing secret; changing the client or issuer does not inherit another client's secret. A changed setup invalidates old local authorization and pending sign-in.
4. Press **Connect**, review the provider's consent page and return to Settings. If the new tab is blocked, use **Continue sign-in**. The callback accepts only the pending, unexpired, one-use PKCE/state answer and validates its issuer before sending a code.
5. Use **Check sign-in** to reload saved state and **Test tools** to connect and list tools without calling them. Follow **Restart to apply** when tool registration needs updating; sign-in never restarts JARVIS automatically.

The equivalent server configuration is metadata only; setup and grants stay in private profile storage:

```toml
[[servers]]
name = 'remote'
[servers.transport]
kind = 'http'
endpoint = 'https://service.example/mcp'
authorization = 'oauth:remote'
```

The binding must name the same server. `jarvis mcp test remote` also preserves a saved OAuth binding. This is pre-registered OAuth, **not** automatic DCR/CIMD, device flow, OIDC identity verification or a claim that every service in a connector directory works with JARVIS. A saved grant is not a live provider health check or a verified account name.

### Sign-in boundaries

- The resource and expected issuer must match the service metadata exactly. JARVIS supports challenge/path/root resource metadata and OAuth/OIDC issuer-discovery fallbacks, but refuses missing advertised S256, issuer mismatch or incompatible authentication instead of guessing. It requests only the scopes you entered and does not silently increase them after a permission error.
- OAuth discovery/token/revocation requests require public HTTPS destinations, validate and pin DNS answers, disable redirects/proxies/retries and limit response bytes/time. Production cannot enable the loopback fixture exception. The older MCP transport's separate endpoint policy is not a new claim of DNS confinement.
- Client secrets, authorization codes and access/refresh tokens stay out of normal status, model context, normal errors and browser storage. The password field clears before a save request, including failure. These are private permission-protected files, **not OS-keychain encryption**. The selected service and issuer receive the OAuth requests; provider-side privacy, retention, scopes and costs remain their responsibility.
- An ordinary restart restores usable saved grants. Pending browser sign-ins are intentionally not restored; start Connect again if JARVIS restarted during consent. Near expiry, refresh is serialized and bound to the same resource/client/issuer. Failed or cancelled refresh requires sign-in; it does not retry potentially consumed refresh material or replay a tool effect.
- **Disconnect** removes local access and pending work before attempting advertised provider revocation. If revocation is unavailable or fails, Settings says local access was removed but provider revocation was not confirmed; revoke the app at the provider when needed. App setup stays for next time. Late callbacks/refreshes cannot restore disconnected access, but already dispatched calls and open response streams cannot be recalled.
- Initial limits: 16 live server-account services per profile; 16 scopes with 512 characters in total; 1,024-byte access/refresh/client-secret values; 4,096-byte private records; 64 KiB HTTP responses. A provider requiring larger values, query-bearing OAuth endpoints or unsupported registration needs a separately verified adapter change rather than an unsafe fallback.

## Verification status

Generic pre-registered OAuth is tested offline with independent HTTP peers: discovery, PKCE/resource/issuer checks, persistence/refresh, client-change/disconnect races, malformed callbacks, a real MCP host credential path, authenticated setup routes and Settings DOM controls. An issuer-guard removal mutation is detected by its no-token-request test. **Real provider registration/browser consent and native macOS/Linux acceptance have not been run.** See [research and exact test evidence](../research/integrations/mcp-oauth.md).

Named HTTP credentials are verified with the real HTTP backend and profile store against a deterministic loopback peer: missing/rotated/removed keys, every HTTP operation, endpoint isolation, redirects, API/CLI/Settings metadata and literal redaction. A cross-server guard-removal mutation is detected by its regression test. This is not real-provider acceptance.

An opt-in, discovery-only HTTPS smoke test is available after explicitly setting `MCP_LIVE_HTTP_ENDPOINT` and `MCP_LIVE_HTTP_TOKEN_FILE` to an owner's chosen endpoint and private token file:

```powershell
cargo test -p jarvisd --lib --locked mcp_credentials::http_tests::live_named_http_token_discovers_an_owner_selected_server_without_running_tools -- --ignored --exact
```

It uses a throwaway profile, lists tools without executing them, prints no provider/account data and cleans up. It has not been run against a real account for this slice.

The key store, CLI/API metadata round-trip, console Test and real child-process injection/redaction/restart are tested offline. **GitHub account access and approved/denied writes are not yet verified live.** The configuration above is a researched, pinned setup path, not a claim that your account has already connected.

An explicit read-only check is available after setting `MCP_LIVE_GITHUB_BINARY` to the verified native v2.0.2 executable and `MCP_LIVE_GITHUB_TOKEN_FILE` to a private PAT file:

```powershell
cargo test -p jarvisd --lib --locked mcp_credentials::tests::live_github_v2_0_2_get_me_with_a_profile_owned_pat -- --ignored --exact
```

It creates a throwaway profile, checks the binary version, lists `get_me`, performs that read and cleans up; it does not write to GitHub or print account data. A writable server and a disposable issue/repository must be selected separately before testing a denied and then approved write through the normal JARVIS approval UI. The read-only example cannot perform that write.

Decisions: [ADR-0168](../adr/0168-mcp-servers-are-managed-from-settings-speak-either-protocol-era-and-are-untrusted.md), [ADR-0169](../adr/0169-abilities-are-one-click-and-a-server-can-have-read-only-tools.md), [ADR-0216](../adr/0216-local-mcp-credentials-are-named-profile-secrets-not-inherited-environment.md), [ADR-0219](../adr/0219-remote-mcp-tokens-are-named-server-bound-request-credentials.md), [ADR-0220](../adr/0220-generic-mcp-oauth-binds-preregistered-clients-to-resource-and-issuer.md). Research: [MCP](../research/integrations/mcp.md), [stdio credentials and GitHub](../research/integrations/mcp-stdio-credentials.md), [remote HTTP tokens](../research/integrations/mcp-http-token-credentials.md), [generic pre-registered OAuth](../research/integrations/mcp-oauth.md).