# Triggers: something happens, a task starts

**New mail. A changed file. A local event. Ordinary runs, under ordinary policy.**

[Documentation](../README.md) / [Projects](projects.md) / [Google](google.md) / [Settings](settings.md)

A trigger waits for something and starts a task for each new thing, while you are away. There are three kinds.

```text
# A new mail (needs the Google sign-in, Settings, Google)
jarvis trigger add --mail "is:unread from:client.example" --every 10m --project Support note the subject in the project journal

# A new or changed file in a folder inside a folder you granted (Settings, Folders & code)
jarvis trigger add --file scans/new --every 5m read the new file and file it under the right client

# A request to a secret address on this computer (for a program or service you connect)
jarvis trigger add --webhook --project Ops summarise what the request says and note anything urgent

jarvis trigger list
jarvis trigger pause ID      # a prefix of the id is enough
jarvis trigger resume ID
jarvis trigger remove ID
```

The console shows them too, next to **Scheduled** (what each watches, when it last looked, why a look failed), and can pause, resume and remove them. They are made here, in the terminal.

## What every kind does

- **The first look starts nothing.** It records what already exists (mail that matches, files in the folder), so a trigger does not answer your backlog. Only what arrives afterwards starts a task. Resuming a paused trigger does the same, so a pause is never followed by a burst.
- **One event, one task, once.** An event is never run twice, however often it is listed and across restarts. If JARVIS stops at the wrong moment, one task may be lost; it is never doubled.
- **The task has your authority and no more.** A tool that needs approval under your current policy waits in `jarvis approvals list` or the console. Outward actions and host commands ask by default; web fetch normally runs without a question, and explicit per-tool standing trust can change the default. A trigger approves nothing, and JARVIS has no tool to make one.
- **What starts it is written by someone else.** The task is told your objective and a reference (a mail's id, a file's path); it reads the content itself, as untrusted data. For a webhook, the request body reaches it only after your objective, fenced and cut to 3,000 characters.
- **A flood is bounded.** Mail/file polling admits at most three tasks per look and applies its daily trigger budget. Webhooks atomically reserve at most three new deliveries per rolling minute and 50 per rolling 24 hours per trigger. Duplicate IDs consume no additional reservation; a reservation whose run fails to start still counts. A paused project or exhausted token budget prevents a new start; a webhook sender must retry a request refused before reservation when the budget permits it.
- **A failed look is shown, not hidden.** `jarvis trigger list` says why the last look failed (Google not connected, folder not found) and looks again at the next interval.

## Mail

Polls your mailbox with a Gmail search, as typed in Gmail's search box. Mail is noticed up to one interval after it arrives; the ten newest matches are looked at each time.

## Files

`--file scans/new` watches the folder `scans/new` inside the first granted folder that has it. It looks only at plain files directly in that folder: not sub-folders, not hidden files, and not a file whose name has anything but letters, digits, spaces and `_ - . ( )` in it. A file counts by its name, size and time, so a file you replace or edit starts a new task, and one that is only listed again does not. A file changed in the last few seconds waits for the next look in case it is still being written. The task reads the file with the document tools, which treat what is in it as data.

## Webhooks

`--webhook` prints an address like `/hooks/3f9c…` **once**. Anything on this computer that can send a `POST` to it (on the daemon's port, `http://127.0.0.1:8765` by default) starts a task, with no other credential, so **give the address only to the program you mean**. Only a hash of it is kept, so it cannot be shown again: make a new trigger to change it.

- An unknown, wrong or paused address answers `404`. New delivery reservations are limited atomically to three per rolling minute and 50 per rolling 24 hours (`429` with `Retry-After: 60` when full); these are not calendar-day counters. A body over 128 KB is refused.
- A delivery ID (`Idempotency-Key`, or GitHub's `X-GitHub-Delivery`) is claimed once. Replaying it starts nothing and uses no additional slot, including after a failed start. Reservation and run creation are separate: if the daemon stops or run creation fails after reservation, that delivery can be lost and is not automatically replayed. This is at-most-once dispatch, not exactly-once completion.
- Not built: checking a sender's signature (for example GitHub's `X-Hub-Signature-256`), and reaching the address from the internet. Do not put the daemon's port on the internet to do it.

```text
curl -X POST http://127.0.0.1:8765/hooks/<the address> -H "Idempotency-Key: build-42" -d "build 42 failed on main"
```

## Quiet hours

`jarvis config set quiet_hours 22:00-07:00` (or Settings, Advanced) holds **notices** between those times, in this computer's time zone: a finished task, a task waiting for your approval, and the Telegram cards for it. Work goes on; nothing is dropped. What was held is announced when the window ends, if it still matters. Without quiet hours you are told at once, as before.

When a triggered task finishes you are told the same way as for a scheduled one: a desktop notification (unless turned off) and your Telegram chat carry the answer, a push topic carries only "a mail-triggered task finished", and nothing is sent while the console is open and showing it.

## Limits

Triggers poll (mail and files) or are called (webhooks); there is no Gmail push. Other things to know: a webhook's only credential is the secrecy of its address; the daemon listens on this computer only; quiet hours are one window for everything. Why it is built this way: `docs/adr/0193-…` and `docs/adr/0195-…`.
