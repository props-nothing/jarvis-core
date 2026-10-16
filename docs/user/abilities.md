# Abilities: Browser And Documentation

**Read the live web. Look up current docs. Apply your tool permissions.**

[Documentation](../README.md) / [Quick start](quick-start.md) / [MCP servers](mcp.md) / [Permissions](settings.md)

These come with JARVIS and are **on by default**: the first time JARVIS starts it sets them up for you (it downloads two small programs once, which takes seconds). Nothing to switch on. They are ordinary tools: each is listed under **Settings, Permissions**, where you can set it to ask, run without asking, or **Off**. To remove one entirely, run `jarvis mcp off browser`; it stays off. Set `JARVIS_NO_BUILTIN_ABILITIES=1` to stop JARVIS adding them at all. Reasons: [ADR-0171](../adr/0171-the-browser-and-documentation-lookup-come-with-jarvis.md).

| Ability | What it does | What it asks about |
| --- | --- | --- |
| **Browse the web** | Opens pages and reads them, including pages that only appear after JavaScript runs. Can click and fill in forms. It uses a private window that is thrown away afterwards. | Opening and reading a page runs on its own by default. Clicking, typing, uploading and running scripts **ask by default**; explicit per-tool standing trust can change this. |
| **Look up developer documentation** | Finds current documentation and examples for a programming library. | Looking things up runs on its own. The library name and your question are sent to context7.com. |

Both need [Node.js 20 or newer](https://nodejs.org) (free; required by the pinned browser runtime). Without it nothing is added and nothing complains; install Node and the next start adds them. A slow first download can leave them for the second start, and the tab then says "ready after restart".

Ask for what you want: "open this page and tell me the price".

From a terminal:

```text
jarvis mcp list          # the abilities and where each stands
jarvis mcp on browser    # switch one on (JARVIS does the setup)
jarvis mcp off browser
jarvis restart           # one restart finishes it
```

## Open a page and check its text

The browser has two explicit reading tools (also available to connected MCP apps):

```json
{"name":"mcp.browser.browser_navigate","arguments":{"url":"https://example.com"}}
```

```json
{"name":"mcp.browser.browser_find","arguments":{"text":"Example Domain"}}
```

Open the page first, then read its elements **inline**:

```json
{"name":"mcp.browser.browser_snapshot","arguments":{}}
```

Automatic navigation/click snapshots are `.yml` links saved in the runtime folder by Playwright. They are not the page text. Explicit `browser_snapshot` returns inline content; no `filename` is accepted by JARVIS's built-in reading contract. For a smaller read use `{"target":"#name","depth":3}`. Results remain bounded and untrusted.

To fill a field (an acting operation that asks by default), use its snapshot reference or unique selector as **`target`**, not `ref`:

```json
{"name":"mcp.browser.browser_fill_form","arguments":{"fields":[{"name":"Your name","type":"textbox","target":"#name","value":"Example"}]}}
```

`browser_type` also takes `target` and `text`. Read a new snapshot after the change to verify it. Evaluate and screenshot tools already belong to the same group; scripts retain their existing permission checks.

If browser tools are held back, `jarvis.tools.search` with `{"query":"browser"}`, `{"query":"open website"}` or `{"query":"fixed literal find text"}` reveals the **whole** browser group. The ten search results are a preview, not its entire inventory. A catalog larger than 64 tools no longer hides later tools from native runs (see [ADR-0213](../adr/0213-discovery-enumerates-the-full-registry-before-deferring-tools.md)).

Find is **FIXED/literal**, not regex: `[`, `(`, `*`, `+` and `/` are ordinary characters. It ignores case, requires nonblank text of at most 512 characters, and returns matching snapshot lines with context or an explicit no-match result. It does not accept `regex`, scripts, or an output filename. Navigation requires a full HTTP(S) URL (at most 8,192 characters), without embedded credentials; `javascript:`, `data:` and `file:` are rejected before opening anything.

Find reads the **current page's accessibility snapshot**, not the whole website, hidden content, image pixels or PDF contents. Snapshot markup (such as element references) can itself match punctuation, and serialization can escape quotes/backslashes. A no-match says only that the snapshot did not contain that literal text. Large results are bounded and may be shortened or refer to a full-output file; not every match is necessarily displayed. Page content remains untrusted data. This narrower contract applies to the built-in pinned browser, not owner-customized servers. Existing permissions still decide whether a call runs; clicking, typing and arbitrary scripts ask by default, subject to explicit per-tool standing trust. Decision: [ADR-0212](../adr/0212-browser-reading-is-explicit-bounded-and-literal.md); verified provider contract: [Playwright MCP research](../research/integrations/playwright-mcp.md).

## What is safe about it

- The programs are **pinned to a version that was tested**, not "whatever is newest".
- What a program returns is treated as **information, never as instructions**. A web page that says "ignore your rules" is still just a web page. (In a live test, JARVIS read a page with a hidden instruction, told you about it and did not follow it.)
- A program starts with a **small environment**, plus only the named credentials the owner explicitly maps to that server. It does not inherit all JARVIS keys; [credential setup](mcp.md#a-local-server-that-needs-a-key) explains the boundary.
- Automatically named browser snapshots and screenshots stay under the profile's temporary runtime directory, not in the folder that launched JARVIS. Explicit filenames still use the folder the browser tool was given.
- Acting browser tools (clicking, typing, uploading) ask by default. Your explicit per-tool **Run without asking**, **Always ask** and **Off** choices apply.

## If something does not work

| You see | What to do |
| --- | --- |
| "needs Node.js" | Install it from nodejs.org, then press Switch on again. |
| The tools are missing | Check `jarvis mcp list` and `jarvis tools list`, then use `jarvis.tools.search` for the browser group. If only click/close/drag and a few early browser tools appear despite the full inventory, update to the ADR-0213 discovery fix and restart; it removes the old first-64 cutoff. A slow initial Node download can also require a restart. |

## Advanced: your own tool programs

Under **Settings → Apps & tools → Custom tools (MCP)** you can add a program that speaks [MCP](https://modelcontextprotocol.io), by entering its command or an HTTPS address. A local server runs as your user. Its tools **ask before calls** until you choose a different approval policy. Declaring a server's tools read-only is your trust decision; it does not stop the program writing or prove its metadata is safe.

```text
jarvis mcp add files -- npx -y @modelcontextprotocol/server-filesystem C:\Users\you\Documents
jarvis mcp test files          # connects once and shows what it offers; nothing is saved
jarvis mcp remove files
```

`jarvis mcp list` shows what each one offers and which tools it **left out and why**.

To let *other* programs use JARVIS instead, see [Connect apps](connect-apps.md).

## Credentials, sign-in and remaining work

Local programs can receive explicitly mapped named credentials, and remote servers can use named bearer tokens or browser OAuth with a pre-registered app. GitHub has its own persistent sign-in screen. Follow [MCP setup and limits](mcp.md) or [GitHub connections](connections.md); keys and grants are private profile files, not OS-keychain storage.

Automatic OAuth client registration, applying changed server registrations without a restart and selected real-provider/browser acceptance remain open. Saved credentials and passing fixture tests are not proof that an owner's account works. The [active backlog](../../TODO.md#provider-and-user-flow-verification) records those checks; [integration research](../research/integrations/mcp.md) retains the dated evidence.
