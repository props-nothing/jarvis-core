# ADR-0193: A new mail can start an ordinary run, and mail is treated as written by a stranger

Status: Accepted
Date: 2026-10-11

## Context

JARVIS acts only when asked or on a schedule. `P6-012` asks for triggers: an event that starts a run. The first, most useful event is mail ("when a mail like this arrives"). `ADR-0167` parked the Gmail push framework (Pub/Sub, `users.watch`, channel renewal) because it needs a public HTTPS endpoint and a Cloud project the owner would have to run, and has no caller in the daemon. A poll of a Gmail search needs nothing beyond the Google sign-in the owner already has. Research: `docs/research/integrations/google.md`, refresh of 2026-10-11.

Mail is the most hostile input JARVIS reads: anyone can send it, and a trigger lets a sender cause a run to start without the owner doing anything.

## Decision

1. **A trigger is a request to start an ordinary run, like a schedule (`ADR-0132`).** It has no authority a run lacks. A tool the policy holds for a person is held for a person and the run parks; the trigger approves nothing.
2. **The `mail` trigger polls one Gmail search.** The owner writes the query and the objective. Every interval (one minute to one day, default five minutes) the daemon lists the ids of the newest ten matching messages (`users.messages.list`, `q`, five quota units; the existing `gmail.readonly` sign-in). The first look is a **baseline**: what already matches is recorded and starts nothing. Resuming a paused trigger baselines again.
3. **At most once per message.** The message id is claimed under a primary key (`trigger_events`, migration 0022, schema 22) before a run starts. A replay, a second pass or a restart claims nothing and starts nothing. A crash between claim and start loses one event and never doubles one.
4. **A failed look is a failed look.** An unreachable or refusing Google, a missing sign-in, or an unreadable count is recorded on the trigger and retried at the next interval. It is never read as "no mail" and claims nothing.
5. **The objective is the owner's.** From a message the daemon takes only its **id**, and only when it is plain (letters, digits, `_`, `-`; at most 64). The message text reaches the run only through `jarvis.gmail.read`, which fences it as untrusted data (`ADR-0149`).
6. **A stranger's mail has a bounded cost.** One look starts at most three runs (the rest wait unclaimed for the next look); one trigger starts at most 50 runs in 24 hours; a paused or finished project, a project over its token cap (`ADR-0165`) and a workspace over its cap (`ADR-0191`) all stop runs. A held message is not claimed, so it is handled when the hold ends and is neither lost nor answered against the owner's wish.
7. **Only the owner makes a trigger.** `POST /api/v1/triggers` (and `jarvis trigger`) carry no tool, scope or approval field (`deny_unknown_fields`). There is no tool for a model to create or change one: a model that read a hostile mail could otherwise make the mailbox start more work.

## Consequences

- The trigger path is **polling**, so mail is noticed up to one interval late. Push (`ADR-0167`) remains parked until a requirement measures this.
- A burst of more than ten matching messages between two looks, or more than three per look, is spread over later looks; the oldest beyond the ten newest are not seen. This is a limit of this slice, not a guarantee, recorded in the research file.
- The dedupe table grows by one small row per matching message and is removed with the trigger.
- **Later work (`ADR-0195`):** the `file` and `webhook` kinds, quiet hours and a console panel were built after this decision, on the same pipeline; a daily budget of proactive messages (`P6-007`) is still open. A triggered run's result is announced like a scheduled one's (desktop notification, Telegram, content-free push), found by the trigger framing in the run's objective because one look can start several runs. **Run live against the owner's mailbox, 2026-10-11** (see `TODO.md` `P6-012`): one new message started one run and a re-listing started none.
