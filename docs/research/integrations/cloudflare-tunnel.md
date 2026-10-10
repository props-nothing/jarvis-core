---
integration: cloudflare-tunnel
status: implemented
last_verified: 2026-10-10
owners: []
selected_spec_version: null
selected_sdk: cloudflared 2026.10.0 (the released program; no SDK)
---

# Cloudflare quick tunnel (`cloudflared`)

## Scope

Making JARVIS's inbound MCP endpoint (`ADR-0170`, `ADR-0172`) reachable from the internet with one switch, so a cloud agent (for example an ElevenLabs phone agent) can connect to it. **In scope:** a **quick tunnel** run by the daemon (`cloudflared tunnel --url`), downloading and verifying the program. **Out of scope:** named tunnels on the owner's Cloudflare account (stable hostname), Tailscale Funnel, ngrok, any Cloudflare API call, Cloudflare Access.

## Official Sources

| Source | URL/version | Accessed | Purpose |
| --- | --- | --- | --- |
| docs index | https://developers.cloudflare.com/tunnel/llms.txt (named by every page; the individual pages below were read) | 2026-10-10 | discovery |
| Quick Tunnels | https://developers.cloudflare.com/tunnel/get-started/quick-tunnels/ (last updated 2026-09-30) | 2026-10-10 | the service, its limits |
| Downloads | https://developers.cloudflare.com/tunnel/downloads/index.md (last updated 2026-09-11) | 2026-10-10 | where releases live, Windows has no auto-update |
| Run parameters | https://developers.cloudflare.com/tunnel/reference/run-parameters/ | 2026-10-10 | flags |
| Release | https://github.com/cloudflare/cloudflared/releases/tag/2026.10.0 (published 2026-10-05) and `api.github.com/repos/cloudflare/cloudflared/releases/latest` | 2026-10-10 | per-asset SHA-256 digests |
| The program's own `--help` | `cloudflared version 2026.10.0 (built 2026-10-05T08:39 UTC)`, run on this machine | 2026-10-10 | exact flag names |

## Verified Contract

### Operations And Transport

- `cloudflared tunnel --url http://127.0.0.1:PORT` makes an **outbound** connection and prints a temporary `https://<words>.trycloudflare.com` address in its log (standard error). No account or domain is needed. Anyone with the address can reach the local service; it stops working when the process stops. (Quick Tunnels page.)
- `--http-host-header VALUE` sets the Host header sent to the local server (flag text from `--help`; it applies to a tunnel defined with `--url`). `--no-autoupdate` disables the update check. `--metrics ADDR` sets the metrics listener; `127.0.0.1:0` was accepted.
- A quick tunnel started 2026-10-10 became reachable about **5 seconds** after launch.

### Authentication And Authorization

- The tunnel itself has **no** authentication for a quick tunnel (an optional `--allowed-mail` email-PIN gate exists but needs an interactive browser, which a cloud agent is not). Protection is therefore JARVIS's: the access key is required on every request, and the endpoint answers 401 otherwise.

### Limits And Failure Semantics (documented)

- No uptime guarantee; up to 200 in-flight requests per quick tunnel (more get 429); **no Server-Sent Events**; the hostname **changes each time** a quick tunnel is created. Cloudflare calls quick tunnels "for testing and development".
- On Windows `cloudflared` does not update itself; JARVIS pins a version and the owner is told it is pinned.

### Data And Compliance

- Everything a cloud agent asks of JARVIS, and every answer, crosses Cloudflare's network. Cloudflare terms apply to the tunnel. The daemon never logs the program's output (only the address is read from it).

### Versions And Deprecations

- Selected: **2026.10.0**. Cloudflare supports versions within one year of the latest.
- Release digests (GitHub `digest` field), single-file assets only: `cloudflared-windows-amd64.exe` `86aee4017b26625cee8484c113558f48effa4cd47f7aa05fcf425604e5d2b23c`; `cloudflared-linux-amd64` `d33ff2d14475178d2012c2c56beba87389ac5ded27649519f198a7d3134a99db`; `cloudflared-linux-arm64` `e6422b9d4f72d3194bc5a38676f13667c06666523217b842a877d72a80b5ac08`. macOS is a `.tgz` (and Homebrew); not downloaded by JARVIS.

## Verified live (2026-10-10, Windows, throwaway profile)

- The downloaded Windows asset's SHA-256 equalled the published digest.
- Started by the daemon with the **allowlisted environment** and no console window: address in ~9 s from a cold start including the 55 MB download.
- Through the public address: no key 401, a wrong key 401, `Bearer KEY` 200, bare `KEY` 200, `tools/list` 21 tools, `tools/call` of a memory search returned; round trip about 100 ms. The endpoint's Host check passed because of `--http-host-header` (no server change).
- Switching off made the old address stop answering (Cloudflare 502); `jarvis stop` left **no** `cloudflared` process; after a restart the tunnel came back by itself with a **different** address.
- Not tested: macOS; Linux; Server-Sent Events behaviour (the endpoint answered each POST with a complete event-stream body, which passed); a long call; behaviour after a network loss.

## JARVIS Mapping

- `apps/jarvisd/src/mcp_tunnel.rs`: the program, its supervision, the pinned downloads. `POST /api/v1/mcp/access/public`; `jarvis mcp access public on|off`; the "From the internet" card in Connect apps. Marker file `config/mcp-public.on` keeps the wish across restarts. The managed program lives in `config/tools/`.

## Decisions

- **Quick tunnel first** because it needs nothing from the owner. Its changing address is stated everywhere the address is shown.
- **`--http-host-header`, not a looser Host check**, so the endpoint's rebinding guard is untouched.
- **Pinned download verified against a digest in the source tree**, refusing anything else; an installed `cloudflared` on PATH is used as is.
- **The endpoint accepts the key as `Bearer KEY` or the bare value.** ElevenLabs documents its field as "Secret Token (optional): Authorization header value" and does not say whether it adds `Bearer`; both forms are accepted and the exact comparison is unchanged. **Not confirmed against ElevenLabs.**

## Rejected Alternatives

- Opening a router port or binding the endpoint to all interfaces: puts a key-only endpoint on the open internet with no TLS.
- Tailscale Funnel and ngrok: both need an account and an installed client; recorded as options, not built.
- Writing a tunnel relay: needs a hosted service.

## Verification Plan

Contract tests (done): address extraction, flag set, digest check refuses a changed file, marker, a child that prints an address and is stopped. Live (done, above). **Still owed:** a real ElevenLabs agent connecting (needs their account), macOS and Linux runs, a named tunnel.

## Unresolved Questions

- Whether ElevenLabs sends the secret token with or without `Bearer`. Blocks nothing (both accepted) but should be recorded after a real connection.
- Whether quick tunnels are acceptable for a phone number that must stay up: Cloudflare says no uptime guarantee and the address changes, so a stable address (named tunnel, `P11-001g`) is needed for a permanent phone agent.
- Server-Sent Events through a quick tunnel (documented as unsupported); ElevenLabs lists streamable HTTP as supported, which is what was tested.
