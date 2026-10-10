# ADR-0173: One switch makes the endpoint for other agents reachable from the internet

Status: Accepted
Date: 2026-10-10

## Context

`ADR-0170` and `ADR-0172` gave other agents a key-protected endpoint with JARVIS's tools, but it listens on this computer only. A cloud agent (an ElevenLabs phone agent, a hosted assistant) connects from the provider's servers and cannot reach it. The owner asked for the easiest setup to make it available. Evidence and live results: `docs/research/integrations/cloudflare-tunnel.md`.

## Decision

1. **A managed Cloudflare quick tunnel**, switched on from Connect apps ("From the internet") or `jarvis mcp access public on|off`. The daemon runs `cloudflared tunnel --url http://127.0.0.1:PORT` with `--http-host-header 127.0.0.1:PORT`, reads the public `https://….trycloudflare.com` address from its log, and shows it with copy-paste steps for ElevenLabs.
2. **Nothing about authentication loosens.** The key is still required on every request (401 otherwise), the rate bound still applies, and the endpoint's Host check is satisfied by the header override rather than relaxed. Approvals still cannot be answered by the caller.
3. **Zero setup, with a pinned download.** If `cloudflared` is installed it is used. Otherwise JARVIS downloads one fixed release (2026.10.0; Windows and Linux x86-64/arm64) from Cloudflare's GitHub releases and **refuses to write or run it unless its SHA-256 equals the digest pinned in the source** (a changed or truncated download is thrown away). The owner is told the size before it happens. macOS gets the Homebrew command.
4. **Run small.** The program starts with the same allowlisted environment an MCP server gets, no console window, no auto-update, its log read for the address and never kept, and it is killed with the daemon.
5. **The wish survives restarts.** A marker file (`config/mcp-public.on`) brings the tunnel back with the daemon. A tunnel that stops on its own is restarted after 10 seconds, and the Problem state with a sentence is shown meanwhile.
6. **Both header forms.** The key is accepted as `Authorization: Bearer KEY` or as the bare value, because a "secret token" field in some products is sent as the bare value (ElevenLabs does not say which). The comparison is still exact and constant-time.

## Consequences

Verified live on 2026-10-10 on Windows: cold start to a public address in about 9 seconds including the 55 MB download (checksum equal to the pin); no key and a wrong key got 401 through the public address, both key forms got 200, tools listed and a call returned; switching off ended the address; `jarvis stop` left no process; a restart brought the tunnel back with a **new** address.

**Costs and limits, stated honestly:**

- **The address changes every time JARVIS starts.** The owner must paste the new one into the cloud app. That is acceptable for trying it and **not** for a phone number people call. A stable address needs a named tunnel on the owner's Cloudflare account with a hostname (`P11-001g`; the Host header must then be handled on the server side, which is a trust-boundary change of its own).
- Cloudflare calls quick tunnels a testing service with no uptime guarantee, 200 in-flight requests and no server-sent events.
- **Anyone holding the address and the key can use the served tools**, including starting agent runs that spend model credit. One shared key and a long random one are the protection; a key per agent and revocation come with `P11-001e`.
- Acting tools (clicks, sending mail) are still refused for the caller until approvals can be answered (`P11-001e`), so a phone agent can look things up and read, not yet act.
- Not run against a real ElevenLabs agent, and not on macOS or Linux.
