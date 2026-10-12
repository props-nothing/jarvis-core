# ADR-0204: JARVIS can connect its own tools to the phone agent, test a key, and say when something has stopped working

Status: Accepted
Date: 2026-10-12

## Context

Three things went wrong in practice this week. The owner had to add JARVIS to the ElevenLabs agent by hand, and every restart of the free temporary address left the agent with a dead server (ElevenLabs cannot change a server's address after it is made, `docs/research/integrations/elevenlabs.md`), so the agent invented answers. A wrong voice or search key was found only when the assistant first needed it. And a broken tunnel or phone door failed quietly: nothing outside Settings said so.

## Decision

1. **Connect the tools from JARVIS.** `POST /api/v1/phone/tools/connect` (Settings, Phone, "Connect them now") makes sure the workspace has an MCP server for the current internet address and that the configured agent uses it. It stores the access key as a **workspace secret** and gives the server `secret_token: {secret_id}` (the key is in no server body or answer, and is scrubbed from provider errors); lists the workspace's servers first and **reuses** one with the same address; **adds** the server to the agent's list instead of replacing it (the agent's other servers are kept); and changes nothing when everything is already right. The agent asks the caller before each tool by default (`require_approval_all`), because `ADR-0200` counts a spoken yes only when the agent asks. "Send the current key again" gives an existing server a new secret after the key was replaced. It needs the internet address up, the ElevenLabs key and the agent id, and says which is missing; a failure before the first write makes no change.
2. **The check shows it.** The Phone check also reports the address, whether a server exists for it, whether the agent uses it, and how many unused temporary (`trycloudflare`) servers are left in the account. JARVIS does **not** delete servers it did not just make.
3. **Test a saved key.** `POST /api/v1/settings/keys/{voice|search}/test` makes one small request with the saved key file (so it works before the restart that applies it) and answers `{ok, message}` in words written here, never the provider's text or the key. Model keys are tested on Brain, Google by sign-in, Telegram by pairing.
4. **Say when something stopped.** A banner under the console header shows a stopped internet address, a phone door that is not working, or Telegram in error, each with a button to the tab that fixes it. It reads only JARVIS's own status routes every 20 seconds (no provider is called) and shows nothing when all is well.

## Consequences

- The access key now also lives at ElevenLabs as a workspace secret. Typing it into their page does the same; the difference is that the secret outlives a rotation until "Send the current key again" is pressed. Old secrets are not deleted by JARVIS.
- A server made by hand in ElevenLabs' page signs in with an auth connection, not a secret; JARVIS reuses it by address and leaves its sign-in alone unless asked to send the key again.
- Verified live on the owner's workspace with a throwaway secret and server (made and deleted at once, nothing left over) and a no-op agent update. **Not run live:** attaching a new server to the agent and re-keying a server; their request bodies match the documented schema and the stand-in tests.
- Not done: a stored "which servers JARVIS made" record, and testing the model and fallback keys from the Voice & search tab. Removing unused temporary servers is in `ADR-0205`.
