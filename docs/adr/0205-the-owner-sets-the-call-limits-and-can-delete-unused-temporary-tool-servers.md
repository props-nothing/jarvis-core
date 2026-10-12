# ADR-0205: The owner sets the call limits and can delete unused temporary tool servers

Status: Accepted
Date: 2026-10-12

## Context

The limits on calls JARVIS places to the owner (3 an hour, 10 a day) were constants, so an owner who legitimately needed more had to change the code. And every restart of the free temporary address leaves an unused MCP server in the ElevenLabs account (`ADR-0204`); the owner had six and could only remove them by hand.

## Decision

1. **Editable limits.** `phone_calls_per_hour` and `phone_calls_per_day` are settings (Settings, Phone, "How many calls"; `jarvis config set`). Defaults stay 3 and 10. Ceilings are 20 an hour and 100 a day, zero is refused, and the hourly limit cannot exceed the daily one; an invalid value fails closed at load and at save. They apply to calls to the owner's own numbers; calls to other people keep their fixed, lower limits (5 a day, 2 per number per day).
2. **Delete unused temporary servers.** `DELETE /api/v1/phone/tools/unused` (Settings, Phone checklist, "Delete them") removes MCP servers from the workspace only when **all** hold: the address is `https`, its host is exactly under `trycloudflare.com` (a word in a path or a longer host does not count), no agent uses it, and it is not the current address. The list is read again just before deleting, at most 25 are removed per press, and it stops at the first provider error and reports how many were removed. The HUD names the servers in a confirmation first. The service never deletes a server on any other address, and never one an agent uses.

## Consequences

- Raising a limit raises the number of calls a mistaken or manipulated model could place; the ceilings and the per-call approval policy still bound it.
- A temporary server that was deliberately kept (no agent, `trycloudflare` host) is deleted if the owner confirms; it is the owner's account and the owner sees the names first.
- Secrets created for deleted servers stay; ElevenLabs refuses to delete a secret that is in use and JARVIS does not try.
- Verified: delete requests against a throwaway server and secret on the owner's workspace (made and removed at once) and stand-in tests, including a test that fails if matching is loosened to "contains".
