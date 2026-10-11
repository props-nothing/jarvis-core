# ADR-0192: The built-in browser keeps automatic output in profile runtime state

Status: Accepted
Date: 2026-10-11

## Context

The built-in Playwright MCP server writes automatically named files under `.playwright-mcp/` when no output directory is configured. A daemon started from a source checkout therefore left browser artifacts in the repository. The built-in ability is also identified by its exact pinned command before its tools may be served to another agent, so changing the command without migrating existing profiles would silently remove that capability.

Playwright MCP `0.0.83` supports `--output-dir <path>`. Official documentation says this controls automatically named output; an explicit filename continues to resolve against the workspace root.

## Decision

1. The built-in browser command includes `--output-dir <profile runtime>/playwright-mcp`.
2. Playwright MCP owns creation and errors for that directory. A browser output failure must not prevent unrelated MCP servers from starting.
3. On startup, only the exact old built-in browser command is replaced. A same-named server with any owner-edited argument is left unchanged.
4. Catalog matching and `vetted_prefixes` derive the expected command from the same active profile runtime path. The old command is not vetted after this change; the migrated command is.

## Consequences

- Automatically named screenshots and snapshots are transient profile state and no longer dirty a repository or whichever directory launched the daemon.
- Existing profiles migrate without requiring the owner to switch the browser off and on.
- Explicit filenames are not redirected by this option. Their existing tool policy and path handling remain in force.
- Moving a portable profile after migration can leave the stored browser command pointing at its former runtime path. JARVIS deliberately does not rewrite an arbitrary `--output-dir`, because it cannot distinguish an owner edit from the former built-in value; switching the ability off and on regenerates it.
