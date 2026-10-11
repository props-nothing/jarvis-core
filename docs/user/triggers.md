# Triggers: a new mail starts a task

A trigger watches your mailbox for a Gmail search and starts a task for each **new** mail that matches, while you are away. It needs the Google sign-in (Settings, Google); nothing else.

```text
jarvis trigger add --mail "is:unread from:client.example" --every 10m --project Support \
    note the subject of the mail in the project journal
jarvis trigger list
jarvis trigger pause ID      # prefix of the id is enough
jarvis trigger resume ID
jarvis trigger remove ID
```

The Gmail search is the one you would type in Gmail's search box. `--every` is between `1m` and `1d` (default `5m`). `--project` files the tasks under a project, so they know its goal and journal.

## What it does and does not do

- **The first look starts nothing.** It records the mail that already matches, so a trigger on `is:unread` does not answer the last ten unread mails. Only mail that arrives afterwards starts a task. Resuming a paused trigger does the same, so a pause is never followed by a burst.
- **One mail, one task, once.** A mail is never run twice, however often it is listed and across restarts. If JARVIS stops at the wrong moment, one task may be lost; it is never doubled.
- **The task has your authority and no more.** It is an ordinary task: anything that needs your approval (sending mail, fetching a page, running a command) waits in `jarvis approvals list` or the console. A trigger approves nothing.
- **The mail is treated as written by a stranger.** The task is told your objective and the mail's id, and reads the text itself, as untrusted data. Only you can make a trigger; JARVIS has no tool to make one.
- **A flood is bounded.** At most three tasks per look and 50 per day per trigger. A paused project, or a project or workspace that used its daily token limit, holds the mail until it can run; held mail is not lost.
- **A failed look is shown, not hidden.** `jarvis trigger list` says why the last look failed (for example, Google is not connected). It looks again at the next interval.

## Limits

Mail is noticed up to one interval after it arrives (this polls; it does not use Gmail push). Only the ten newest matching messages are looked at each time. Not built yet: other kinds (a file appears, a webhook), quiet hours, a console screen, and a desktop notification when a triggered task finishes.

Why it is built this way: `docs/adr/0193-a-new-mail-can-start-an-ordinary-run-and-mail-is-treated-as-written-by-a-stranger.md`.
