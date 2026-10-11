---
integration: telegram
status: implemented-against-a-stand-in
last_verified: 2026-10-11
owners: []
selected_spec_version: "Telegram Bot API 10.3 (announced 2026-08-24)"
selected_sdk: null
---

# Telegram (the Bot API)

## Scope

JARVIS in the owner's pocket (`P11-005`, `ADR-0184`): receive text from one paired private chat by long polling, send answers and approval cards, take a button tap. **Out of scope:** webhooks, groups and channels, files, photos, voice notes (`P8-010`), inline mode, payments, business and community features, Mini Apps, a Local Bot API server beyond allowing its address.

## Official Sources

| Source | URL/version | Accessed | Purpose |
| --- | --- | --- | --- |
| docs index | `https://core.telegram.org/llms.txt` and `/bots/llms.txt` answer HTTP 200 with a "Page not found" HTML page: **not found**; docs discovered from https://core.telegram.org/bots | 2026-10-11 | discovery |
| API reference | https://core.telegram.org/bots/api (changelog top entry: Bot API 10.3, August 24, 2026) | 2026-10-11 | normative contract |
| Bot FAQ | https://core.telegram.org/bots/faq | 2026-10-11 | rate limits |
| OpenAPI/schema | none official | 2026-10-11 | not found |
| official SDK | none used: five methods over HTTPS | 2026-10-11 | |

The page is one very large HTML document; it was downloaded and read by section (`Making requests`, `getUpdates`, `sendMessage`, `editMessageText`, `answerCallbackQuery`, `InlineKeyboardMarkup`, `InlineKeyboardButton`, `CallbackQuery`, `ResponseParameters`). Changes since 10.2 (July 14, 2026) concern rich and ephemeral messages and communities: none touches these methods.

## Verified Contract

### Operations And Transport

- Every call is `https://api.telegram.org/bot<token>/METHOD`, GET or POST; parameters may be JSON. The reply always has a boolean `ok`; on `true`, `result` holds the value; on `false`, `description` and an integer `error_code`, and sometimes `parameters` (`ResponseParameters`, with `retry_after` seconds after flood control, `migrate_to_chat_id`).
- **`getUpdates`** (long polling): `offset` (first update wanted; an update is confirmed by asking with an offset above its `update_id`), `limit` 1 to 100, `timeout` seconds (positive for long polling), `allowed_updates`. "Will not work if an outgoing webhook is set up." Updates are kept at most 24 hours. Recalculate `offset` after every response to avoid duplicates.
- **`deleteWebhook`** returns the bot to polling.
- **`getMe`** tests the token and returns the bot's `username`.
- **`sendMessage`**: `chat_id`, `text` of **1 to 4096 characters**, optional `reply_markup` (`InlineKeyboardMarkup`). Returns the `Message` (`message_id`).
- **`InlineKeyboardMarkup`**: `inline_keyboard`, an array of rows of `InlineKeyboardButton`; a button with `callback_data` of **1 to 64 bytes** sends a `callback_query` when tapped.
- **`CallbackQuery`**: `id`, `from`, `message`, `data`. "It is necessary to react by calling `answerCallbackQuery` even if no notification is needed", otherwise the client shows a progress bar.
- **`editMessageText`**: `chat_id`, `message_id`, `text`, optional `reply_markup`.
- **`sendChatAction`** (`typing`) is used for the typing indicator; its page was not read in full and its failure is ignored.

### Authentication And Authorization

The token is the credential and is part of every request's address. The bot has no notion of an owner: **anyone** may message a bot. JARVIS therefore authorises by chat identity after a pairing proof (decision below), never by the bot's token.

### Limits And Failure Semantics

- FAQ: "In a single chat, avoid sending more than one message per second ... eventually you'll begin receiving 429 errors"; groups 20 messages a minute; bulk about 30 a second.
- `error_code` is "subject to change"; the daemon branches only on 401/404 (bad token) and 409 (a webhook or another reader) from `getMe` and `getUpdates`, and on `retry_after`.
- Updates confirmed by `offset` are not redelivered; unconfirmed ones can be, for up to 24 hours.

### Data And Compliance

Messages, answers and approval cards (tool name, risk, arguments up to 700 characters) pass through Telegram's cloud servers; bot chats are not end-to-end encrypted. The owner chooses this by pasting a token. Nothing is sent before a token is saved.

### Versions And Deprecations

The Bot API is versioned by announcement, not by URL. Nothing used is marked deprecated in 10.3.

## JARVIS Mapping

- A message from the paired chat becomes an ordinary run through `GatewayState::start_and_drive`; the answer is `executor::last_answer`. The conversation is one session, kept in `telegram-session.txt`, restarted by `/new`.
- A pending approval (`gateway::pending_approvals`, which includes calls from other apps) becomes a card; a tap calls `decide_approval` with the `api` channel and `resume: true`.
- Settings: `daemon.telegram_token_ref` (absolute path of a private key file), `jarvis keys set telegram`, `PUT /settings/keys/telegram`. Pairing state: `telegram-chat.json`, `telegram-offset.txt` in the configuration directory.
- Routes: `GET /telegram` (state), `POST /telegram/pair` (`{code}`), `POST /telegram/unpair`.

## Decisions

- **Long polling, not a webhook**: no public address, no tunnel, works behind any router (the owner's stated need).
- **Pairing by code**: the first writer is told a six digit code which the owner types into Settings (ten minutes, five tries). It proves the person holding the phone is at the computer. A pending code held by one chat is not replaced by another's message.
- **After pairing, silence for everyone else**, including taps on cards; groups, channels and bots are never answered.
- **Offset confirmed before handling** (at most once, as for schedules) and persisted.
- **Answers are Markdown converted to Telegram HTML** (`ADR-0186`, `parse_mode: HTML`), because plain text showed `**` and `|---|` on a real phone. HTML over MarkdownV2: MarkdownV2 needs 18 characters escaped everywhere and one missed `.` or `-` makes Telegram refuse the whole message; HTML needs only `<`, `>` and `&`. Approval cards stay plain (they hold raw arguments).
- **`deleteWebhook` once on a 409**: the bot is JARVIS's; a second 409 is reported ("another program is reading this bot's messages").
- **Errors are stripped of their address** (`reqwest::Error::without_url`) because the address holds the token.
- A different token unpairs; removing the token unpairs.

## Rejected Alternatives

A Telegram SDK crate (a large dependency for five methods and a second type vocabulary); webhooks (need a public address); a shared secret in the bot's `/start` deep link (needs the owner to build a link); answering strangers with "not authorised" (confirms the bot exists and answers).

## Verification Plan

- Done, offline: a stand-in that follows the contract above drives the daemon through pairing (wrong code, five tries, a second writer, a stranger, a restart), an approval card with two buttons and one tap, a stranger's tap, a late tap, a rejected token never echoed, chunking and the code's shape (9 tests in `telegram_tests.rs`).
- Done, live against the real daemon and a real model, with the stand-in as Telegram: Settings tab to paired, a question answered, a stranger ignored, an approval card approved by a tap that really created the schedule.
- **Run by the owner on a real phone (2026-10-11): works** (pairing, answers). That run showed the formatting problem `ADR-0186` fixes. The rest of this item is the original plan and is **partly confirmed by that run**; the keyboard clean-up and typing indicator below were not reported either way.`n- **Formatting facts used (Bot API HTML style, read 2026-10-11):** supported tags `b strong i em u ins s strike del span.tg-spoiler a code pre blockquote`; `pre` may hold `code class="language-x"`; all `<`, `>`, `&` outside tags must be entities; only `&lt; &gt; &amp; &quot;` are named entities; entities may nest except inside `pre` and `code`; no headings, lists or tables exist, so JARVIS draws them. Not proven against Telegram's parser here (no way to call it without a token): what happens with a tag nested in `code`; that is why a message answered `400 ... can't parse entities` is resent as plain text.`n- **(Earlier, unconfirmed at the time:) not done: any call to the real Telegram.** That needs a bot token. Until it is run, these assumptions are unproven: `editMessageText` with an empty `inline_keyboard` removes the buttons (the page states `reply_markup` is optional and does not say what an omitted or empty one does); `sendChatAction` accepts `typing`; a private chat's `chat.id` equals the user's `from.id` (documented elsewhere for private chats, relied on here for the tap check).

## Unresolved Questions

- Whether `editMessageText` clears an inline keyboard given an empty `inline_keyboard`. Blocks only the card clean-up; a failed edit is ignored and the approval itself is unaffected.
- Behaviour when Telegram is blocked by the network: the loop reports an error and retries every 5 to 10 seconds.
