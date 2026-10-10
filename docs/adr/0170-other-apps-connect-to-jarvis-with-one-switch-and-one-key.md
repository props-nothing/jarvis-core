# ADR-0170: Other apps connect to JARVIS with one switch and one key

Status: Accepted
Date: 2026-10-10

## Context

JARVIS could already serve its tools over MCP on loopback (`mcp_serve_port`, `ADR-0074` and the phase-3 gate), but nobody could use it:

- the endpoint required an authenticated caller and admitted nobody (`CallerAdmission::local_only`), so every client got 401;
- the server spoke only the newest protocol version (2026-07-28), so every shipped client (Claude Code, Cursor, VS Code, the official SDKs) was refused with "Unsupported protocol version". This was found live, not by reading;
- the setting needed a number typed into Advanced and at least one granted folder, neither of which means anything to the owner.

The owner's request: turning this on should be a switch, and an external agent should connect with an API key.

## Decision

1. **One switch, one key, a tab called "Connect apps".** The switch writes `daemon.mcp_serve_port` (default 8766, loopback only). Switching on creates a random key stored in the owner's credential directory (`mcp-access.key`, owner-only permissions, in the redactor's secret list). The tab shows the address, a Show/Copy key control, **Make a new key**, and copy-paste setup lines for Claude Code, Cursor and VS Code. `jarvis mcp access [on|off|key]` does the same from a terminal. The port setting is hidden from Advanced.
2. **The key is the only door.** A request layer in front of the endpoint requires `Authorization: Bearer <key>`, compares in constant time, and answers 401 otherwise (no key, a wrong key, and the console credential all fail). With no key set, nobody is admitted. A new key takes effect at once, with no restart.
3. **One fixed caller.** After the key matches, the layer rewrites the header to a fixed fingerprint, so the existing admission, rate limit (120 per minute) and audit see a single caller named by the daemon rather than anything the client sent. The existing admission stays the policy; the key only decides who reaches it.
4. **The server speaks both protocol eras.** It now accepts 2026-07-28 and the 2025 versions (2025-11-25, 2025-06-18, 2025-03-26) with legacy sessions, so existing clients connect. Requests with no protocol signals are still refused, and a GET without a session is a 4xx.
5. **No folder is needed.** The rule that `mcp_serve_port` requires granted folders was removed: JARVIS's own tools (agent, memory, contacts, projects) are useful without one. The tools served are the ones the daemon has; file tools exist only if the owner granted folders.
6. **What a connected app can and cannot do.** It can call the tools that run without asking, including `jarvis.agent.delegate` (start a JARVIS run and read its result), memory search and the read-only tools. It **cannot** hold an approval: a tool that needs the owner's yes is refused with `mcp_call_cannot_hold_an_approval` and no approval is left waiting (`ADR-0039`). Anyone holding the key has this reach, so the key is shown only on request and can be replaced in one click.
7. **Loopback only.** The server binds 127.0.0.1. Reaching it from another machine needs a deliberate tunnel the owner sets up; the card says so.
8. **A port that cannot be used says so.** Startup now fails with a sentence naming the port and the setting to change, not "could not be served". (A first live failure that looked like a missing-folders bug was another process holding the port.)

## Consequences

Verified live on 2026-10-10 with the official `@modelcontextprotocol/sdk` TypeScript client: no key, the console credential and a wrong key got 401; the correct key connected (protocol 2025-11-25) and listed 26 tools with a folder granted and 21 without; `jarvis.agent.delegate` for "17 times 23" returned 391, fenced as untrusted; `jarvis.schedule.add` (needs approval) was refused with nothing left waiting; rotating the key made the old key fail at once; Switch off plus Restart closed the port. Claude Code's `claude mcp add --transport http` form was checked in its documentation; the Cursor and VS Code lines follow their documented file formats but were **not** run against those products. Claude Desktop speaks only stdio and needs a bridge, so it is not offered.

Not delivered: remote (non-loopback) access with its own authentication, a key per app with separate revocation, and a per-key tool allow list. A single key means one leaked key exposes everything the endpoint serves; the mitigations are loopback, the approval refusal and one-click rotation.
