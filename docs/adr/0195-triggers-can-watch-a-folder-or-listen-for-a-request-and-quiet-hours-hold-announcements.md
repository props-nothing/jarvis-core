# ADR-0195: Triggers can watch a folder or listen for a request, and quiet hours hold announcements

Status: Accepted
Date: 2026-10-11

## Context

`ADR-0193` made a new mail start an ordinary run. The rest of `P6-012` was other events (a file appears, a service calls) and the rule that autonomy must not become noise (`P6-007`, quiet hours). Research: `docs/research/integrations/webhook-and-file-triggers.md`.

## Decision

1. **One record, three kinds** (migration `0023`, schema 23): `mail` (a Gmail search), `file` (a folder inside a granted folder) and `webhook` (a request to a secret address). Every kind starts an ordinary run with the owner's objective and no extra authority (`ADR-0132`, `ADR-0193`), is deduplicated by an event key claimed before the run starts, and has the same bounds: three runs a look (or a minute), 50 a day, the project and token caps. The objective names the kind (`This task was started by a file trigger ...`) so a run, a list and a notification say what started it.
2. **A file trigger polls a folder; it never reads a file.** The folder must be relative and plain, is resolved **inside** a granted folder (canonicalised, so a link cannot lead out), and only regular files directly in it are listed. A file is an event by its **name, size and modification time**, so an edited file is a new event and a listed-again one is not; the first look is a baseline. A file changed in the last five seconds is left for the next look (it may be mid-write). A name that is not plain (letters, digits, space, `_ - . ( )`, not hidden, at most 120 characters) is ignored rather than quoted, because a name is written by whoever put the file there. The run is told the path and to read it with `jarvis.files.read_document`; the content reaches it only through the file tools, which fence it.
3. **A webhook is `POST /hooks/{secret}`**, the one route that needs no console credential. The secret is 32 random bytes as 64 hex digits, shown **once** when the trigger is made and stored only as its SHA-256, so a copy of the database cannot start a run. An unknown address, one of the wrong shape and a paused one all answer the same `404`, so nothing says which addresses exist; a `GET` is not a delivery and still needs the credential. A body over 128 KB is refused; at most three runs a minute and 50 a day (`429`, `Retry-After: 60`); a held project answers `503`. A delivery id (`Idempotency-Key` or `X-GitHub-Delivery`) makes a retry start nothing; without one every request is its own event. The body reaches the run only after the owner's objective, as **fenced untrusted data cut to 3,000 characters**.
4. **Quiet hours hold the telling, not the work.** `daemon.quiet_hours = "22:00-07:00"` (this machine's time zone, so it follows daylight saving; a window may cross midnight; equal ends are refused). While it is on, the result notifier holds a finished run's notice (desktop, Telegram, push) and an approval's notice, and Telegram holds its approval cards. Nothing is dropped: they are announced by the first pass after the window if still relevant. Runs, triggers and schedules are unaffected. What existed when the daemon started is recorded without announcement, as before, so a restart inside the window does not announce old things at its end.
5. **The console shows triggers** (a panel beside Scheduled: what each watches, how often, project, runs started, the last failed look, Pause/Resume/Remove). Triggers are made from the CLI or API, not from the console: a webhook's address is shown once, and the console should not be a place a secret is displayed.

## Consequences

- A webhook's only credential is the secrecy of its address. A request that is captured can be replayed unless the sender supplies a delivery id. HMAC verification (`X-Hub-Signature-256`) is the recorded next step.
- The daemon listens on loopback; a webhook is reachable from other programs on this computer. Reaching it from the internet needs the owner's own tunnel to the console port, which also serves the credential-protected console; that is not built and not recommended without a separate listener.
- A file trigger sees only plain names directly in one folder. A sub-folder, a file with an unusual name and a file still being written are not events until they qualify.
- Quiet hours depend on the saved configuration being readable; with no configuration, announcements are not held.
- Not built: quiet hours per kind or per project, a budget of proactive messages per day, and a console form to make a trigger.
