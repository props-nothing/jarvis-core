# External access

**Other programs can use JARVIS. They cannot become its owner.**

[Documentation](../README.md) / [Abilities](abilities.md) / [MCP servers](mcp.md) / [Security](../../SECURITY.md)

Let another program use JARVIS: Claude Code, Cursor, VS Code, or your own script.

This is different from [Abilities](abilities.md) (what JARVIS can use) and from Permissions (which of JARVIS's tools ask first). External access is the other direction: *other programs using JARVIS*.

## Turn it on

Open **Settings → External access** and use the switch, then **Restart now**. Or from a terminal:

```text
jarvis mcp access on
jarvis restart
jarvis mcp access
```

The last command shows the address (`http://127.0.0.1:8766/mcp`) and, if you ask, the key. The tab shows copy-paste lines for each program.

## Connect a program

```text
claude mcp add --transport http jarvis http://127.0.0.1:8766/mcp --header "Authorization: Bearer YOUR_KEY"
```

Cursor and VS Code take the same address and an `Authorization` header in their `mcp.json`; the tab fills in the exact text. Those two were written from their documentation and not yet run in the products. Claude Desktop only speaks to programs it starts itself, so it is not offered.

## What a connected program can do

- See every tool JARVIS has, including the built-in browser and documentation lookup (about 48 tools on a fresh profile).
- Use the ones that run without asking: start a JARVIS run with `jarvis.agent.delegate` and read its answer, open and read web pages, look up documentation, search and save memory, contacts and projects.

A tool they call directly is not yet recorded in your history (the tasks they hand JARVIS are). It **cannot** answer approvals. When a tool needs your yes (clicking or typing in the browser, sending mail, changing your schedule, running commands), the call **waits** and the question appears with your other approvals: one click, `jarvis approvals approve`, or a spoken yes answers it, and the app gets the result. If you say no, or do not answer within two minutes, the call is refused and nothing runs. At most eight calls wait at once.

## Keeping it safe

- It listens on this computer only (127.0.0.1) unless you switch on the internet address above.
- Anyone who has the key can use everything above. **Make a new key** in the tab (or `jarvis mcp access key`) and the old key stops working at once, with no restart.
- Switch it off when you do not use it.

## From the internet (for cloud apps such as a phone agent)

A cloud app connects from its own servers, so an address that only works on this computer is no use to it. Under **Settings, External access, From the internet** press **Make it reachable** (or run `jarvis mcp access public on`). JARVIS opens a free Cloudflare address, downloading Cloudflare's `cloudflared` program once if needed (about 55 MB, checked against a fixed checksum), and shows the address.

For ElevenLabs: Agents, Integrations, **Add custom MCP server**, Server URL = the address shown, Secret token = your key. Then attach it to your agent.

- **Without your own Cloudflare tunnel (below) the address changes every time JARVIS starts.** Paste the new one into the app after a restart. That is fine for trying it, not for a phone number people call.
- Anyone with the address **and** the key can use the tools, so treat both as passwords. Every request without the key gets 401.
- Switch it off the same way and the address stops working at once. It also stops when JARVIS stops.
- Not tested yet against a real ElevenLabs agent. Details: [cloudflare-tunnel.md](../research/integrations/cloudflare-tunnel.md).

Reasons and decisions: [ADR-0170](../adr/0170-other-apps-connect-to-jarvis-with-one-switch-and-one-key.md), [ADR-0172](../adr/0172-the-endpoint-for-other-agents-serves-the-built-in-browser-and-documentation-tools.md), [ADR-0173](../adr/0173-one-switch-makes-the-endpoint-for-other-agents-reachable-from-the-internet.md), [ADR-0202](../adr/0202-the-owner-may-give-jarvis-a-named-cloudflare-tunnel-for-an-address-that-never-changes.md).

## An address that never changes (your own Cloudflare tunnel)

For a phone agent you want an address that is the same after every restart, so you paste it into ElevenLabs once. You need a free Cloudflare account with a domain on it. Nothing changes if you skip this: the free temporary address above keeps working.

1. In the Cloudflare dashboard open **Networking, Tunnels, Create tunnel** and name it (for example `jarvis`). Pick any operating system on the next page: **you do not install or run anything**, JARVIS runs Cloudflare's connector itself.
2. On that page, under **Install and Run**, copy the command (`cloudflared.exe service install eyJ...`). **Do not run it.** The long text starting with `eyJ` is the tunnel token.
3. In JARVIS, Settings, External access, From the internet, open **Want an address that never changes?**, enter the name you want (for example `jarvis.yourdomain.com`), paste the command (or just the token) and press **Save and connect**. Or run `jarvis mcp access public named jarvis.yourdomain.com --from-env CLOUDFLARE_TUNNEL_TOKEN` (the token comes from the environment variable, a `--file`, or is typed in; it is never a command-line value). Within a few seconds Cloudflare's **Connection Status** turns to connected.
4. Back in Cloudflare continue to the route: add a **published application** with the same name and, as the **Service URL**, the address shown in JARVIS (usually `http://127.0.0.1:8766`).
5. The address is now `https://jarvis.yourdomain.com/mcp`. Put it in ElevenLabs once as the MCP server URL, with your key as the secret token.

- The token is kept privately on this computer, is not shown again and is never written to a log. If it leaks, rotate it in Cloudflare and paste the new one.
- The name is guessable, so the protection is still the key: every request without it gets 401.
- If the endpoint's port changes (it takes the next free port when 8766 is taken), update the route's Service URL in Cloudflare.
- **Go back** with the button next to the name, or `jarvis mcp access public named off`.
- If Cloudflare refuses the tunnel (a wrong or rotated token, a deleted tunnel) the card shows the reason.
