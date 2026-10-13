# ADR-0206: The phone agent gets a callback through a sub-agent, not a phone tool

Status: Accepted
Date: 2026-10-13

## Context

When the owner rang JARVIS and said "do this and call me back", the phone agent could not: it has no phone tool. That is deliberate (`ADR-0200` §6): `jarvis.voice.*` is never served to other apps, because a phone agent that could place the next call could keep the owner's phone ringing with no person in the loop. Nothing told the agent what to do instead, so it either invented a promise or said it could not.

## Decision

1. **The phone tools stay unserved.** A call still cannot start a call.
2. **The route that already exists is named to the agent.** A sub-agent started with `jarvis.agent.delegate` is an ordinary run (`ADR-0134`) that does have `jarvis.voice.call`, under the same approval, quiet-hours, rate and one-open-call rules as any model. For an inbound call from the owner the door (`POST /phone/initiate`, `ADR-0201`) now also returns `conversation_config_override.agent.prompt.prompt`: the agent's own prompt plus a note that it has no phone tool, must never ask whether to arrange a callback, and has two ways to get one. A callback **at a time** ("in two minutes") uses `jarvis.schedule.add` with a one-off `at` (the note gives the current UTC time). A callback **after a job** uses `jarvis.agent.delegate` (`background: true`) with a task ending "ring the owner with `jarvis.voice.call` and tell them the result". It may say a callback is coming only when the tool confirmed it.
3. **Fail open for the owner, closed for everyone else.** The brief is added only after the door said yes (the owner's number). It is skipped, and the call goes on with the agent as it is, when the agent's prompt cannot be overridden, cannot be read within four seconds, or the provider errors. The door never switches the override on (a call placed by JARVIS does that already). A stranger is refused before any brief is built.
3. **Approval is unchanged.** `jarvis.voice.call` asks every time unless the owner trusted it by name (Settings, Permissions, Phone). For a hands-free callback the owner trusts it; otherwise the sub-agent waits for the owner's yes in the console, as any held call does.

## Consequences

- The agent follows a prompt, not a rule: it can still forget. The first real try (2026-10-13) showed it understood the note but asked "would you like me to?" and the owner hung up first; the note now says never to ask, and covers a timed callback. The guarantee is what holds even then: no phone tool for the agent, and the limits on the call a later run places.
- A callback loop needs a person each time (the owner must answer), and is bounded by the hourly and daily limits (`ADR-0205`) and one open call.
- If the sub-agent finishes while the owner is still on the line, its call is refused as "a call is already open"; the result stays in the run.
- Source: Conversation initiation data and overrides in ElevenLabs' Twilio personalization page (`conversation_config_override.agent.prompt.prompt`), read 2026-10-13. **Not verified live** with a real inbound call.
