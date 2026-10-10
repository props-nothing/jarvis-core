# Tool programs (MCP), for developers

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

What to know: a program starts with a **small environment** (no keys); what it returns is **untrusted data**; a program that stops is started again on the next call; `jarvis mcp list` shows what each offers and why a tool was left out. Not built: a key for a program that needs one, remote programs that need a login, applying a change without a restart.

Decisions: [ADR-0168](../adr/0168-mcp-servers-are-managed-from-settings-speak-either-protocol-era-and-are-untrusted.md), [ADR-0169](../adr/0169-abilities-are-one-click-and-a-server-can-have-read-only-tools.md). Research: [mcp.md](../research/integrations/mcp.md).