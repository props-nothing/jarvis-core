# ADR-0168: MCP servers are managed from Settings, speak either protocol era, and are untrusted

Status: Accepted
Date: 2026-10-10

## Context

`ADR-0167` made MCP the route to new tools. The host had existed since `P3-008j` and had never met a real server. Connecting the first two (the reference `server-everything` and `server-memory`, `docs/research/integrations/mcp.md`) found that the host could use none of them, and that several things the earlier design treated as safe were not:

- The client refused every server that did not speak the newest protocol revision, and every published server answers only the older `initialize` handshake.
- Every tool of those servers was dropped silently, because they declare JSON Schema draft-07 and the rule accepted only 2020-12. The daemon logged "reachable 1, tools 0" and nothing else.
- A server's output reached the model **unfenced**, so a page a browser server read or a file a search server found could instruct the model. `jarvis.web.fetch` already fenced its text; this path did not.
- A server was started with the daemon's whole environment, and `server-everything` has a tool that returns it, so any key the owner had exported was one approval away from the model.
- A server that exited mid-run stayed dead until the daemon restarted.
- Adding one meant hand-writing `mcp-servers.toml` and restarting to find out whether it worked.

## Decision

1. **Either protocol era.** `ClientLifecycleMode::Auto`: probe `server/discover`, fall back to `initialize`, which is the stdio procedure the specification itself gives. The era is recorded on the connection (`McpEra`) and shown to the owner, never hidden. A peer that names no revision is refused.
2. **Draft-07 is read, not refused, where it means the same.** A schema declaring draft-07 is read as 2020-12 after two checks: the keywords 2020-12 dropped (`dependencies`, `additionalItems`) are refused by name, and anything whose shape changed fails the 2020-12 metaschema check at construction. Draft-04 and 2019-09 stay refused. Arguments are validated against the read schema, so `required`, types and `additionalProperties` are enforced.
3. **A server's output is untrusted data and is fenced.** Text and structured content are split into pieces, each neutralised and fenced with the existing `IsolatedText`, behind a sentence saying what it is; at most three pieces, under the model-facing limit, so a closing fence is never cut off and a forged closing token inside the text is removed.
4. **A server starts with an allowlisted environment.** What a program needs to start (`PATH`, the profile and temp directories, locale, proxies and certificate paths) and nothing else. A server that needs a credential will get it by a deliberate route (open in `P11-001`), never by inheritance.
5. **A stopped server is started again.** Before a call or a listing, a connection whose transport has closed is restarted from the recipe that made it, serialised, and not retried for ten seconds after a failure. A call that fails because the server could not be started is **not sent** (`CallError::NotSent`, adapter `RefusedBeforeReaching`, outcome `Failed`, safe to retry). A call that was in flight when the server died stays ambiguous (`Unknown`), as before.
6. **Servers connect at the same time**, so one slow or dead server delays the daemon's start by its own wait and not the sum.
7. **Managing servers is a surface of its own.** `GET /api/v1/mcp`, `POST /api/v1/mcp/servers`, `DELETE /api/v1/mcp/servers/{name}` and `POST /api/v1/mcp/test` (daemon), `jarvis mcp list|add|test|remove` (CLI) and a management tab in the console (first called Tools (MCP); since `ADR-0169` the Abilities tab, with the command form under Advanced). Every edit is validated by the parser the daemon starts with, the program is resolved to an absolute path when it is added, the file is replaced atomically, and a change applies on the next start (the page says so). A test connects once, lists what the server would offer and what would be left out and why, and saves nothing. No model-facing tool reaches these routes.
8. **A new server's tools ask.** The posture `unclassified` (risk 3, held for approval) stays the default. `read-only` is the owner's own statement (`--read-only`, a checkbox) and is never inferred from what a server says about itself; a server's `annotations` are still never read.
9. **What happened is visible.** The daemon logs each unreadable server and each tool left out, with the reason, and the overview shows them.

## Consequences

Verified live on 2026-10-10 against the two real servers: connected as legacy; thirteen tools registered, risk high, approval ask; the model called `echo`, the call parked, was approved, and the result came back (before and after fencing); the server's process chain was killed and the next call restarted it; a variable set in the daemon's environment did not appear in `get-env`'s output; the console tab listed, tested, added and removed a server.

Not done, and recorded in `TODO.md` `P11-001`: Streamable HTTP against a real remote server and its authorization; a deliberate route for a server's credential (a secret reference per variable, never a value in the file); tool discovery for a large catalog (`P11-002`); applying a change without a restart; a failed first start of `npx` that exceeds the twenty-second handshake (the second run is cached); `tools/list_changed`. Adding a server is running a program as the owner, and the page says so.
