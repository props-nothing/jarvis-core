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

What to know: a program starts with a **small operational environment**, plus only the credentials you explicitly map below. What it returns is **untrusted data**; a program that stops is started again on the next call; `jarvis mcp list` shows what each offers and why a tool was left out. Still not built: remote programs that need a login, and applying server configuration without a restart.

## Easier GitHub setup: browser sign-in

For GitHub, prefer **Settings → Connections → GitHub**. Register a JARVIS OAuth app once in the expandable setup section, save its client ID and write-only client secret, then use **Connect GitHub**. The account authorization persists and refreshes when needed; no PAT, native helper, Docker or server command is required. This first connection exposes only the fixed hosted read-only tool set. See [Connections](connections.md) for registration, permissions, Test, Disconnect and outstanding live verification. The daemon still asks for an explicit restart when its tool registry needs updating; there is no automatic restart or generic OAuth support for arbitrary servers.

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

## Verification status

The key store, CLI/API metadata round-trip, console Test and real child-process injection/redaction/restart are tested offline. **GitHub account access and approved/denied writes are not yet verified live.** The configuration above is a researched, pinned setup path, not a claim that your account has already connected.

An explicit read-only check is available after setting `MCP_LIVE_GITHUB_BINARY` to the verified native v2.0.2 executable and `MCP_LIVE_GITHUB_TOKEN_FILE` to a private PAT file:

```powershell
cargo test -p jarvisd --lib --locked mcp_credentials::tests::live_github_v2_0_2_get_me_with_a_profile_owned_pat -- --ignored --exact
```

It creates a throwaway profile, checks the binary version, lists `get_me`, performs that read and cleans up; it does not write to GitHub or print account data. A writable server and a disposable issue/repository must be selected separately before testing a denied and then approved write through the normal JARVIS approval UI. The read-only example cannot perform that write.

Decisions: [ADR-0168](../adr/0168-mcp-servers-are-managed-from-settings-speak-either-protocol-era-and-are-untrusted.md), [ADR-0169](../adr/0169-abilities-are-one-click-and-a-server-can-have-read-only-tools.md), [ADR-0216](../adr/0216-local-mcp-credentials-are-named-profile-secrets-not-inherited-environment.md). Research: [MCP](../research/integrations/mcp.md), [stdio credentials and GitHub](../research/integrations/mcp-stdio-credentials.md).