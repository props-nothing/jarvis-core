# ADR-0184: Telegram is a chat with the same JARVIS, set up in Settings

Status: Accepted
Date: 2026-10-11

## Context

`P11-005`. JARVIS lived in a browser tab on one computer. The owner wants it in their pocket, set up in Settings like everything else, without opening a port or running a tunnel. Research: `docs/research/integrations/telegram.md`.

## Decision

1. **A bot token in Settings, a new Telegram tab.** The token is stored like the other keys: a private file named by `daemon.telegram_token_ref`, write-only through `PUT /settings/keys/telegram` and `jarvis keys set telegram`. **No restart:** the daemon reads the token fresh on each pass, so Settings shows the bot's name within seconds of Save.
2. **Long polling** (`getUpdates`). Nothing is exposed; it works behind any router. The loop runs inside the daemon whenever runs are driven, sleeps until a token exists, and stops when the token is removed or changed.
3. **Pairing proves the person holding the phone is at the computer.** The first writer to the bot is told a six digit code, which the owner enters in Settings (ten minutes, five tries, one live code at a time). Then only that private chat is answered; every other sender, group and tap is ignored **without a reply**. A new token or Remove unpairs.
4. **One brain.** A message becomes an ordinary run through `start_and_drive`, in one conversation kept across restarts (`/new` starts afresh), with the same tools, policy and approvals. The answer is sent back, split under Telegram's 4096-character limit; a typing indicator shows while it works. Scheduled results are also sent to the phone (the console being open does not matter: the owner is probably elsewhere).
5. **Approvals are buttons, one tap** (`ADR-0136`). Each new pending approval (including calls from other apps, `ADR-0180`) is a message with the tool, its risk, what it will do, and its arguments (clipped), with Approve and Deny. A tap goes through the same decision route as a click and the card is rewritten with the outcome. What was already waiting when the daemon started is not replayed to the phone. The decision is stored under the `api` channel: the audit vocabulary is closed and widening it needs a migration this slice does not justify.
6. ~~Plain text only~~ **Superseded by `ADR-0186`**: the owner ran it on a real phone and the answers showed raw Markdown, so answers are now converted to Telegram's HTML. **Errors never carry the token**: it is part of every request address, so transport errors are stripped of their address before anything records them.

## Consequences

Tested with a stand-in that follows the documented contract: pairing (wrong code, five tries, a second writer, a stranger, a restart), a card and one tap, a stranger's tap, a late tap, a rejected token never echoed, splitting, the code's shape (9 tests). Run live against the real daemon with a real model and the stand-in as Telegram: Settings tab, Save token, "Connected to @bot" with no restart, a message, the code, Pair, "17 times 23" answered on the phone, a stranger ignored, an approval card for `schedule.add` approved by a tap (the schedule really existed) and the answer sent.

**Not run against real Telegram: that needs a bot token from the owner.** Unproven until then: that `editMessageText` with an empty keyboard removes the buttons, and `sendChatAction`. Failure of either is ignored and cannot affect an approval. Not built: files, photos and voice notes in or out (`P8-010`), groups, a button to approve a proposed skill (`ADR-0183`), per-tool limits on what a phone may approve (the same one tap as the console, by `ADR-0136`; a stolen unlocked phone can approve, as it could answer any notification), and a Telegram session channel of its own in the audit trail.
