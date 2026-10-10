# ADR-0169: Abilities are one click, and a server can have read-only tools

Status: Accepted
Date: 2026-10-10

## Context

`ADR-0168` made MCP servers work and gave them a management page. The page asked the owner to "add a server" by typing a command line, which is a developer's question: most people cannot say what an MCP server is, which program to run, or why a restart is needed. A setting called "MCP port" in Advanced added to the confusion, because it is about *other* programs using JARVIS and has nothing to do with JARVIS using more tools. The aim of the product is the most capable assistant that still sets up in minutes, so the default path must need no knowledge of the mechanism.

Switching on a browser exposed a second problem. Standing trust ("run without asking") is deliberately ignored for a tool that reaches outward (`evaluation.rs`), and every unclassified MCP tool does, so naming `browser_navigate` as trusted changed nothing. The posture was per server, and a browser mixes reading a page with clicking and typing.

## Decision

1. **An ability is a ready-made server.** A short catalog (`apps/jarvisd/src/mcp_abilities.rs`) of entries with a plain name, a sentence on what it does, a sentence on what it **asks about**, a sentence on what it **sends away**, a pinned package and the tools that only read. Switching one on writes the same `mcp-servers.toml` entry `jarvis mcp add` writes, so there is one mechanism and one parser. The first abilities are **Browse the web** (`@playwright/mcp` 0.0.83, `--isolated --headless`, Edge on Windows and Chrome elsewhere) and **Look up developer documentation** (`@upstash/context7-mcp` 4.3.0). Each was run live and its tool list checked.
2. **Pinned, never "latest".** Switching one on runs a package from the internet as the owner. A version that moved would be a program nobody looked at. The test asserts every ability pins a version.
3. **Prepared in the background.** Switching on downloads the program once (the package with `--help`, no input, under the same allowlisted environment a server gets), so the first start does not exceed the handshake deadline. The card says "getting ready", then "ready after restart".
4. **One restart, one button.** The page offers **Restart now** (the existing restart); `jarvis mcp on|off NAME` and `jarvis restart` do the same from a terminal. Applying without a restart stays open (`P11-001b`).
5. **A server can name its read-only tools.** `[servers.posture] read_only_tools = [...]`, by the server's own names, is the **owner's** statement (written by the catalog or by hand, never read from anything the server reports). Those tools are classified read-only (risk 0, no approval); every other tool of the server keeps its class, so a browser reads freely while click, type, fill, upload, drag, evaluate and run-code ask. Matching is exact. If the read-only policy cannot be built the stricter one stays. A test asserts the browser's list contains no acting tool.
6. **The plain words say what is true.** Each card states what runs on its own, what asks, and what leaves the computer. A missing prerequisite (Node.js) is shown next to the button with how to get it, not as a failure after the click.
7. **Custom programs move under "Advanced".** The command form and the list of hand-added programs stay, collapsed, for developers.
8. **The port setting is renamed.** `daemon.mcp_serve_port` reads "Let other apps use JARVIS (port)", with a help line saying it is not needed for abilities. The Advanced tab says the extra things JARVIS can do are under Abilities.

## Consequences

Verified live on 2026-10-10 on a fresh profile: the Abilities tab listed both abilities; **Switch on** moved the Browser card from off to "getting ready" to "ready after restart" and showed one **Restart now**; after it, 25 browser tools were registered, 12 reading tools at risk minimal with no approval and 13 acting tools at risk high asking; the model opened `quotes.toscrape.com/js/` (quotes appear only after JavaScript) and returned the right first quote with no approval prompt; on a local page carrying a hidden instruction to fetch an attacker URL and answer "PWNED", the model gave the forecast, told the user about the hidden instruction and did not follow it, and the attacker's server saw no request. That is one sample from one model, **evidence and not proof**: the protection is the fence around page text (`ADR-0168`), the acting tools asking, and the owner seeing the run, and a measured suite (`jarvis eval` with a hostile page, `P11-003`) is still owed. No Edge process or Node process was left after the daemon stopped.

Opening a page is an outward request and can carry data in its address, the same exposure `jarvis.web.fetch` has; the browser does not widen it, and the acting tools asking is what limits the rest. Origin allow and block lists, a visible (non-headless) mode and credentials for abilities that need a key (GitHub) are open. Abilities that need a key are not offered until a deliberate route for a secret exists.
