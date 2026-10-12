# ADR-0203: The settings dialog is a grouped side list with switches, short help and a restart footer

Status: Accepted
Date: 2026-10-12

## Context

Settings had grown to ten tabs that wrapped over two rows, with the same visual weight for everything. A review of every tab found: on/off settings shown as free-text boxes ("default: off"); long multi-sentence help under every row; raw setting names as labels (`quiet_hours`, `push_topic`); notification and quiet-hours settings hidden in Advanced; the ElevenLabs key in a "Voice" tab although the Phone tab depends on it; steps that did not say what to do next or in which tab; models for sub-agents and scheduled runs that could only be set from the command line; a flat list of about 55 tools in Permissions with 28 of them under "Other"; and a footer that always offered **Restart to apply** without saying whether anything was waiting.

## Decision

This is a console change only. No setting, API, default, permission or trust boundary changes.

1. **A grouped side list** replaces the tab row: JARVIS (Brain, Voice & search, Folders & code, Permissions), Accounts & channels (Google, Telegram, Phone), Connections (Connect apps), System (Alerts, Advanced). A dot shows set up (green) or not yet (amber), computed only from the saved settings. "Web search" is folded into Voice & search beside the ElevenLabs key; notification, push and quiet-hours settings move from Advanced to a new Alerts tab (matched by key in the console, so the daemon's own setting groups and `jarvis config` are unchanged).
2. **On/off settings are switches** that save when flipped; a few fixed choices (thinking effort) are lists. Folders are a list to add to and remove from; the code sandbox has Node and Python presets and says what it needs (Docker, an image pulled by the owner).
3. **One sentence, then More.** Each row shows the first sentence of its explanation (or a hand-written one-liner for switches) and keeps the rest one click away. Setup steps fold away once the thing is set up; status comes first on Google, Telegram and Phone.
4. **Cross-tab steps are a checklist.** Phone shows, with live state, whether the ElevenLabs key, the agent and number, the owner's number and the internet address are in place, each leading to the tab where it is done; numbers found at ElevenLabs have a "Use this number" button that fills in the number and its agent.
5. **The footer says what waits.** Changes that need a restart are listed ("Waiting for a restart: Folders, Main model"); the button is highlighted only then. Settings that apply at once never add to the list. A switch or save keeps the scroll position.
6. **Permissions get a filter, folded groups and an "only the ones I changed" view**, and the families now include Phone, Browser, Documentation and Other programs, so "Other" is not a catch-all.

## Consequences

- No new endpoint, no new dependency, and no script, style attribute or outside address in the page (the existing tests for that still pass).
- The dots are an aid, not proof: "set up" means the settings exist, not that the provider accepts them (the Phone tab's own check does that).
- Not done: a Test button beside the voice and search keys (needs a daemon endpoint; `jarvis keys test` exists), a voice picker listing the owner's ElevenLabs voices, and a per-field "changed since last restart" marker.
- The layout was checked in a browser against a throwaway profile (all ten tabs, switches, folder list, permission filter, restart footer). Mouse input is not delivered to the embedded browser used for that check, so clicks were driven through the page's own events, not a real pointer.
