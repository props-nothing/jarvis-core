# Abilities: Browser And Documentation

**Read the live web. Look up current docs. Ask before acting.**

[Documentation](../README.md) / [Quick start](quick-start.md) / [MCP servers](mcp.md) / [Permissions](settings.md)

These come with JARVIS and are **on by default**: the first time JARVIS starts it sets them up for you (it downloads two small programs once, which takes seconds). Nothing to switch on. They are ordinary tools: each is listed under **Settings, Permissions**, where you can set it to ask, run without asking, or **Off**. To remove one entirely, run `jarvis mcp off browser`; it stays off. Set `JARVIS_NO_BUILTIN_ABILITIES=1` to stop JARVIS adding them at all. Reasons: [ADR-0171](../adr/0171-the-browser-and-documentation-lookup-come-with-jarvis.md).

| Ability | What it does | What it asks about |
| --- | --- | --- |
| **Browse the web** | Opens pages and reads them, including pages that only appear after JavaScript runs. Can click and fill in forms. It uses a private window that is thrown away afterwards. | Opening and reading a page runs on its own. Clicking, typing, uploading and running scripts **ask you first**. |
| **Look up developer documentation** | Finds current documentation and examples for a programming library. | Looking things up runs on its own. The library name and your question are sent to context7.com. |

Both need [Node.js](https://nodejs.org) (free). Without it nothing is added and nothing complains; install Node and the next start adds them. A slow first download can leave them for the second start, and the tab then says "ready after restart".

Ask for what you want: "open this page and tell me the price".

From a terminal:

```text
jarvis mcp list          # the abilities and where each stands
jarvis mcp on browser    # switch one on (JARVIS does the setup)
jarvis mcp off browser
jarvis restart           # one restart finishes it
```

## What is safe about it

- The programs are **pinned to a version that was tested**, not "whatever is newest".
- What a program returns is treated as **information, never as instructions**. A web page that says "ignore your rules" is still just a web page. (In a live test, JARVIS read a page with a hidden instruction, told you about it and did not follow it.)
- A program starts with a **small environment**: it does not see the keys JARVIS holds.
- Automatically named browser snapshots and screenshots stay under the profile's temporary runtime directory, not in the folder that launched JARVIS. Explicit filenames still use the folder the browser tool was given.
- Anything that **changes** something (clicking, typing, uploading) asks you first, as everything else does.

## If something does not work

| You see | What to do |
| --- | --- |
| "needs Node.js" | Install it from nodejs.org, then press Switch on again. |
| The tools are missing | Check Node.js is installed, then restart JARVIS once; the first start can be slow while the programs download. |

## Advanced: your own tool programs

Under **Advanced: add your own tool program** you can add any program that speaks [MCP](https://modelcontextprotocol.io), by typing the command that starts it (or an `https://` address). This is for developers: it runs that program as you. Its tools **ask before every call** until you decide otherwise in **Settings, Permissions**; "it only reads" is your own statement, never something JARVIS infers from what the program says about itself.

```text
jarvis mcp add files -- npx -y @modelcontextprotocol/server-filesystem C:\Users\you\Documents
jarvis mcp test files          # connects once and shows what it offers; nothing is saved
jarvis mcp remove files
```

`jarvis mcp list` shows what each one offers and which tools it **left out and why**.

To let *other* programs use JARVIS instead, see [Connect apps](connect-apps.md).

## Not built yet

A program that needs a key of its own (GitHub, a database) cannot be given one yet, and a remote program that needs a login is not supported. More abilities will be added as each is tested live. Reasons and decisions: [ADR-0168](../adr/0168-mcp-servers-are-managed-from-settings-speak-either-protocol-era-and-are-untrusted.md) and [ADR-0169](../adr/0169-abilities-are-one-click-and-a-server-can-have-read-only-tools.md); research: [mcp.md](../research/integrations/mcp.md).
