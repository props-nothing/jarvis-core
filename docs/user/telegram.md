# Telegram: talk to JARVIS from your phone

JARVIS can live in a Telegram chat only you can use. It is the same assistant: same tools, same memory, same permissions. When it needs your approval it sends buttons.

## Set it up (about two minutes, once)

1. In Telegram, open **@BotFather**, send `/newbot`, pick a name. It gives you a **token**.
2. In the console open **Settings, Telegram**, paste the token and press **Save token**. No restart. The status turns to "Connected to @yourbot".
3. Open your bot in Telegram and send it any message. It replies with a **six digit code**.
4. Enter the code in Settings and press **Pair**.

That is all. From now on only your chat is answered. Anyone else who finds the bot gets no reply at all.

## What you can do

- Write as you would in the console. The answer comes back in the chat.
- `/new` starts a fresh conversation. `/help` says what it understands.
- When JARVIS wants to do something that needs your approval, you get a message with the tool, how risky it is, what it will do and its details, and two buttons: **Approve** or **Deny**. One tap. Calls from other apps that need your answer arrive the same way.
- A scheduled task's result is sent to your phone when it finishes.

## How answers look

JARVIS writes in Markdown and Telegram shows it properly: **bold**, *italic*, `code`, code blocks with a language, links, quotes, headings as bold lines, lists with bullets, and tables as aligned monospace text (or one small card per row when a table is too wide for a phone). Long answers arrive as several messages, cut between paragraphs, never through a code block. If Telegram ever refuses a message's formatting, it is sent again as plain text so nothing is lost.
## What it does not do yet

Photos, files and voice notes are not read or sent. Groups are never answered.

## Good to know

- Your messages and JARVIS's answers go through Telegram's servers. Treat the chat like any cloud chat.
- Nothing is opened on your network: JARVIS asks Telegram for new messages.
- **Disconnect this chat** in Settings forgets the pairing; the bot then offers a new code. Saving a different token, or **Remove the bot**, also forgets it.
- From a terminal: `jarvis keys set telegram` saves the token (it is not shown).
- Anyone who can unlock your phone can approve what JARVIS asks, as they could answer any notification. Use your phone's lock.
- If Settings says another program is reading the bot's messages, stop it or make a new bot: only one reader at a time is allowed.
