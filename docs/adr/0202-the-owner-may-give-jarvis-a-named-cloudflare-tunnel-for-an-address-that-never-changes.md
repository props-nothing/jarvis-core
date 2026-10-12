# ADR-0202: The owner may give JARVIS a named Cloudflare tunnel for an address that never changes

Status: Accepted
Date: 2026-10-12

## Context

`ADR-0173` made the endpoint for other agents reachable with a free Cloudflare quick tunnel, whose address **changes every time it starts**. That breaks a phone agent (`ADR-0200`, `ADR-0201`): after every restart ElevenLabs still points at the old address, so the agent has no tools and starts inventing answers. `P11-001g` named the fix: a tunnel on the owner's own Cloudflare account. Evidence: `docs/research/integrations/cloudflare-tunnel.md` (section "Named tunnel").

## Decision

1. **Optional, and the quick tunnel stays the default.** With no named tunnel saved, nothing changes: the same quick tunnel, the same switch. A named tunnel is used only when the owner has saved **both** a hostname and a token; one without the other, or a damaged file, counts as none and the quick tunnel is used.
2. **Two values, entered once.** The owner makes the tunnel in the Cloudflare dashboard (a remotely-managed tunnel with a published application route to `http://127.0.0.1:PORT`) and gives JARVIS the **hostname** they chose and the **token** (the `eyJ…` text in the install command Cloudflare shows when the tunnel is created, or under "Add a replica" later; pasting the whole install command is accepted and the token is taken from it). Entry points: Settings, Connect apps, From the internet; `jarvis mcp access public named HOSTNAME [--from-env NAME | --file PATH]`; `POST/DELETE /api/v1/mcp/access/public/named`. The token is **checked for shape** (base64 of a JSON object with `a`, `t`, `s`) so a wrong paste is refused at once, not as a tunnel that never connects. The hostname must be a real DNS name (no scheme, path, port, wildcard or IP address).
3. **The token is a secret.** It is kept in `config/cloudflare-tunnel.token`, written private (current user only) through a temporary file and a rename, **never returned by the API, never logged** (its `Debug` is hand-written to hide it), never on a command line (it goes to `cloudflared` in the `TUNNEL_TOKEN` environment variable, so other programs cannot read it from the process list). The hostname is in `config/cloudflare-tunnel.host`.
4. **"On" means Cloudflare can reach it.** The tunnel is reported on, at `https://HOSTNAME`, only after the program logs `Registered tunnel connection`; a program that merely started, or printed any other address, is not "on".
5. **The Host header: rewritten only after the key.** A named tunnel forwards the owner's public hostname as `Host` (unless the owner sets "HTTP Host Header" in the dashboard), and the endpoint's own guard accepts loopback names only. Rather than add the hostname to that guard, `require_key` (which already replaces the credential after checking it) **replaces the Host with the loopback address when, and only when, the request carried the exact key and its Host equals the saved hostname**. A different Host, a request without the key, and a forgotten tunnel are unchanged, so the guard behind it is not loosened for anyone else. This is the trust-boundary change `ADR-0173` said a named tunnel would need.
6. **Changing it needs no stop and start.** Saving or forgetting a named tunnel bumps a counter the running supervisor watches; it stops the program and starts it again with the new setting (no race with the old process). Saving while Connect apps is being served also switches the internet address on.

## Consequences

- **The phone setup stops going stale.** The MCP server address in ElevenLabs is set once. The inbound door (`ADR-0201`) registers the same webhook address every time.
- **A known, guessable hostname.** A quick tunnel's name was random; the owner's is chosen and may be guessed. The protection is unchanged and does not rely on the name being secret: every request needs the key (401 otherwise), the rate bound applies, and `POST /phone/initiate` needs its own secret. The key is long and random.
- **Cost and dependency.** The owner needs a Cloudflare account and a domain on it. Cloudflare's terms apply and all traffic crosses Cloudflare's network (as with a quick tunnel). The route's Service URL must carry the endpoint's port; if the port changes (the endpoint takes the next free one when 8766 is taken) the dashboard route must be updated. The HUD shows the exact Service URL.
- **Rotation.** A token that leaks is rotated in Cloudflare (which disconnects old copies) and pasted again here.
- **Not yet done:** creating the tunnel, route and DNS record **for** the owner through Cloudflare's API with an API token (the dashboard steps are two minutes; automation would need a second credential with DNS write rights). **Not run against a real Cloudflare account in this change:** the program was exercised only with stand-ins and with a deliberately invalid token (see the research note), so the first real connection is the owner's to confirm.
