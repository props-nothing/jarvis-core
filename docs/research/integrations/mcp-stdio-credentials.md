---
integration: mcp-stdio-credentials
status: researched
last_verified: 2026-10-15
owners: []
selected_spec_version: "2026-07-28 (dual-era client; legacy peers remain supported)"
selected_sdk: "rmcp =3.4.0; GitHub MCP Server v2.0.2 for owner-run live acceptance"
---

# Credential-backed local MCP servers

## Scope

`P11-001b(a)`: supply explicitly selected, profile-owned credentials to one local stdio MCP server without inheriting the daemon's secrets. The owner selected GitHub as the real-service acceptance target. Rust local deployment; existing rmcp version and normal policy/approval pipeline. Remote HTTP authorization/OAuth, automatic installation, hot host replacement, provider token creation and unattended writes are out of scope.

## Official Sources

| Source | Exact URL/version | Accessed (UTC) | Purpose |
| --- | --- | --- | --- |
| MCP index | https://modelcontextprotocol.io/llms.txt | 2026-10-15 | Current versioned documentation discovery |
| stdio binding | https://modelcontextprotocol.io/specification/2026-07-28/basic/transports/stdio.md | 2026-10-15 | Child lifecycle, newline JSON-RPC, stderr and restart |
| Authorization | https://modelcontextprotocol.io/specification/2026-07-28/basic/authorization.md | 2026-10-15 | HTTP OAuth differs from stdio credentials in the environment |
| SDK source | https://raw.githubusercontent.com/modelcontextprotocol/rust-sdk/rmcp-v3.4.0/crates/rmcp/src/transport/child_process.rs | 2026-10-15 | Tokio command handed to SDK, stdio configuration and stderr |
| SDK release | https://api.github.com/repos/modelcontextprotocol/rust-sdk/releases/tags/rmcp-v3.4.0 | 2026-10-15 | Pinned release; changes include lifecycle/cancellation and auth discovery, not a new credential transport requirement |
| GitHub local server | https://raw.githubusercontent.com/github/github-mcp-server/v2.0.2/README.md | 2026-10-15 | Official local PAT environment variable and minimal permissions |
| GitHub release | https://api.github.com/repos/github/github-mcp-server/releases/latest | 2026-10-15 | Latest stable v2.0.2; immutable release with native archives/checksums |
| GitHub tool configuration | https://raw.githubusercontent.com/github/github-mcp-server/v2.0.2/docs/server-configuration.md | 2026-10-15 | Local `--tools=get_me`, `--read-only`; read-only mode excludes writes even when explicitly requested |
| GitHub CLI source | https://raw.githubusercontent.com/github/github-mcp-server/v2.0.2/cmd/github-mcp-server/main.go | 2026-10-15 | `stdio` command, PAT lookup, mutually exclusive auth modes and version flag |
| GitHub repository index | https://raw.githubusercontent.com/github/github-mcp-server/main/llms.txt | 2026-10-15 | Not found (404); official pinned README used instead |

The attempted pinned SDK `CHANGELOG.md` raw URL returned 404; the official release API above supplies the release notes instead. No SDK upgrade or copied SDK implementation is required.

## Verified Contract

### Operations And Transport

MCP stdio launches a child whose stdin/stdout carry newline-delimited JSON-RPC. stdout cannot contain ordinary logs. The client may capture, forward or ignore stderr; stderr does not itself prove failure. Closing stdin is the portable graceful shutdown signal; unexpected exit permits restart. JARVIS keeps its existing dual-era discovery, listing pagination and tool-call behavior.

### Authentication And Authorization

The MCP authorization specification explicitly says stdio implementations should obtain credentials from the environment instead of following the HTTP OAuth mechanism. GitHub's pinned local server accepts `GITHUB_PERSONAL_ACCESS_TOKEN`; a supplied PAT takes precedence over its optional local OAuth login. PAT permission must be chosen for the intended repository/operation, not inferred from tool discovery. JARVIS does not create a PAT or reuse its own GitHub tooling credential. The owner will supply a separate least-privilege credential through a hidden prompt, a private file or a named environment input.

No refresh, OAuth callback or webhook is introduced. Rotation replaces the named stored value; revocation removes it and/or revokes it at GitHub.

### Limits And Failure Semantics

No new protocol quota or retry is introduced. Existing bounded handshake, listing and call timeouts apply. Credential resolution must complete before spawning. A missing/unreadable/invalid key makes only that server unavailable and sends no request; the daemon and unrelated servers keep running. Restart resolves references again, so removal fails closed instead of silently using a cached secret. Existing ambiguous-effect policy remains: a provider may have accepted a write before transport loss, so never automatically replay an unknown effect.

Provider quotas are determined by GitHub credential/account/API; this generic transport makes no promise about a fixed rate. Native execution avoids adding Docker as an install requirement. A program added by the owner runs with the owner's privileges; this is not an OS sandbox.

### Data And Privacy

Only environment mappings explicitly bound to a server reach that child in addition to the existing operational allowlist. Never put values in arguments, TOML, status, prompts, audit, errors or logs. Credential-bearing children must not inherit stderr into daemon output. A trusted server can still intentionally return or transmit its environment; child provisioning is not a DLP guarantee or a sandbox. Do not install untrusted servers. Ordinary tool results remain untrusted data and normal policy authorizes each effect.

### Versions And Deprecations

Retain rmcp =3.4.0, no dependency changes. Current MCP authorization confirms environment-based stdio credentials; no relevant deprecation observed in the selected SDK release. GitHub target is v2.0.2 rather than `latest`. Real account calls require explicit owner setup; none has been made as part of research.

## JARVIS Mapping And Decisions

- Configuration `[servers.env]` maps a portable environment variable to `mcp:<server>:<key>`, never to a literal value or arbitrary file path. The named reference is bound to the same configured server; cross-server references and case-colliding variable names are refused.
- A canonical validated reference names the server/key. Storage owns private profile files; the transport accepts a resolver port and has no storage/provider dependency. The existing keys CLI gains named MCP targets; ordinary key kinds keep their behavior.
- Only credential-style environment names are accepted: portable identifier syntax, no inherited operational names, interpreter/loader/runtime injection variables, `JARVIS_*` or `MCP_*` controls. The initial allowlist requires a credential suffix (`TOKEN`, `KEY`, `SECRET`, `PASSWORD`, `CREDENTIAL`, optionally `API_KEY`); no arbitrary runtime configuration API is introduced.
- Resolve just before spawn/restart; retain references and a resolver, not values, in the connection recipe. Default/no-resolver connections refuse credential-requiring servers.
- Admin/API and CLI add/test round-trip reference metadata. Status shows mapping and set/not-set only; existing permission posture does not become read-only merely because a PAT exists.
- GitHub read and approved-write live acceptance remain distinct from credential plumbing tests; use an owner-named disposable issue/repository before any outward write.

## Rejected Alternatives

- Inheriting all environment variables reproduces the existing live-discovered leakage defect.
- Plaintext environment maps or arbitrary absolute key-file paths make ordinary configuration/status or unrelated files into credential channels.
- Caching resolved values in restart recipes ignores rotation/removal.
- Combining remote OAuth, live registry rebuild and local credentials makes a narrow usable slice unnecessarily broad.

## Verification Plan

Cheapest falsifier: parse two server entries where one tries to use the other's secret reference; it must fail before any process launches. Also prove an unset required reference does not spawn a marker-writing child.

- Reference/variable validation: unsafe components, cross-server, reserved variables, duplicates and HTTP/env refusal.
- Private named-key write/status/remove tests and no raw values in normal formatting or errors.
- Real child process checks configured environment receives only the intended canary; a sibling receives none. No secrets in returned test metadata or child stderr.
- Rotation/removal between real process restarts changes the next child or prevents it starting.
- Admin/API/CLI configuration round-trip, auth gate and normal policy/approval regression coverage.
- Owner-run opt-in GitHub check: native v2.0.2 starts with a separately stored PAT, lists tools, completes a read; a write waits for approval, denial has no effect, then a separately approved disposable write has provider evidence. Do not claim live acceptance without those results.

## Unresolved Questions

- The owner's GitHub PAT and disposable acceptance target are not available. This blocks real-account read/write evidence only, not offline/real-child credential plumbing. Never print or solicit the token in chat.
- Native platform file-protection/spawn behavior requires Windows/macOS/Linux CI; a local Windows pass does not prove all platforms.
- Remote HTTP authorization and hot apply remain separate unchecked `P11-001b` slices.

## Verification Log

| Date | Versions checked | Evidence | Researcher |
| --- | --- | --- | --- |
| 2026-10-15 | MCP 2026-07-28; rmcp 3.4.0; GitHub MCP 2.0.2 | Live official index/spec/source/release pages checked. Named refs/private store/CLI/admin/console and spawn/restart injection implemented. Real-child tests prove isolation, missing-key refusal, rotation/removal and stderr/text/structured/identity/description/error literal redaction. Transport suite 202 passed; latest stdio 12 passed, profile-host composition 1 passed, opt-in GitHub read ignored, console 54 passed; fmt/strict workspace Clippy passed. No real GitHub account read or write performed. Full baseline recorded in TODO. | Copilot |
