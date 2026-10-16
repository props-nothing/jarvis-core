# Projects

**Give long-running work a goal, a brief and a memory of its own progress.**

[Documentation](../README.md) / [Quick start](quick-start.md) / [Triggers](triggers.md) / [Evaluations](evals.md)

A **project** is long-running work you want JARVIS to keep doing: a goal, your standing guidance, where its files live, and a journal of what has happened. Every run that belongs to a
project is told all of that, so you write it once instead of pasting it into each request, and tomorrow's run knows what today's run decided.

A project changes what JARVIS is **told**, never what it may **do**. Approvals, permission postures and the folders you granted are exactly what they were.

## Make one

In the console: **Ops → Projects → New project**. Or in a terminal:

```text
jarvis project add Prospecting --goal "Book five demos with Dutch logistics firms" --folder sales --guidance-file brief.txt
```

* **Goal**: what done looks like, in a sentence or two.
* **Guidance**: how you want it done. Language, tone, who it may contact, what to avoid, how often to bother you. Up to 12,000 characters.
* **Folder**: where its files live, relative to a folder you granted in Settings → Folders & code. It cannot point outside that folder.

## Work in it

| Where | How |
| --- | --- |
| Console | Pick the project in the selector next to "Conversation", or press **Chat** on a project. A conversation stays in its project. |
| Terminal | `jarvis ask --project Prospecting find three new leads`, or `jarvis chat --project Prospecting` |
| On a schedule | `jarvis schedule add check replies and follow up --every 6h --project Prospecting`, or at a clock time: `jarvis schedule add send the briefing --at-time 08:00 --days mon-fri --zone Europe/Amsterdam` (it stays at 08:00 through daylight saving; leave `--zone` out for this computer's) |
| By JARVIS | A task it schedules from inside a project joins it, and a sub-agent it starts there gets the project's brief. |

## Let JARVIS make one

Just ask: "make this a project". JARVIS creates it (you see the goal and guidance it wrote on an approval card and answer yes or no), the conversation joins it, and the journal works straight away. It can also
change a project, file a schedule under one, and join an existing one with `jarvis.project.use`. In a chat that belongs to no project it is told which projects exist.

## The journal

JARVIS is asked to record a line when it finishes a stage, decides something, is blocked, or knows what comes next (`jarvis.project.note`; it needs no approval, because it only writes one
short line in your own journal). The newest entries are shown to every run. You can add your own: `jarvis project note Prospecting skip anyone at the competitor`, or in the console's
project window. Journal entries are shown to the model as data to read, never as instructions, so a note written after reading a web page cannot take over a later run.

## Pause, finish, delete

`jarvis project pause|resume|done NAME`. While a project is paused or done its scheduled tasks stop firing (you can still chat in it). (JARVIS can pause a project too, with your yes.) `jarvis project remove NAME` deletes the project and its
journal; its conversations and schedules stay.

## Cap how much it runs by itself

Two caps: a run-count cap for scheduled work, and a token cap for all runs in the project, including interactive messages and sub-agents. The project window also shows how many tokens the project used in the last 24 hours, whether or not a cap is set.

**Tokens.** `jarvis project set NAME --token-limit 500000` (or **Daily tokens** in the project window) caps input plus output tokens in a rolling 24-hour window. Scheduled starts at the cap are skipped and counted. The executor also checks before a model call and after recorded model usage for every project run: reaching the cap stops the run with `daily_token_limit` before another model or tool call, and records a blocker in the project journal (`ADR-0191`). This can stop your interactive request too. A model call can overshoot because its usage is known only after it returns; concurrent calls already in flight can also overshoot. If usage cannot be read, the guard stops work with `token_limit_unavailable`. Zero means no cap, and JARVIS cannot change the cap itself. The optional workspace-wide token cap applies as well; see [Settings](settings.md#brain). These are token limits, not money limits.

**Runs.**
`jarvis project set NAME --daily-limit 12` (or **Daily runs** in the project window) stops the project's *scheduled* tasks from starting more than that many runs in any 24 hours; the rest wait, and are counted as skipped in
`jarvis schedule list`. Zero means no cap. Your own messages in the project are never refused, and JARVIS cannot change the cap itself.

## What it decided long ago

Every run is shown the newest journal entries and, before them, older **decision, result, blocker and owner** entries, so a long project keeps what it settled even after the progress notes have scrolled away. Write down what you
want remembered as a decision.

## What it did while you were away

`jarvis digest` (or **Ops → Last 24 hours**): runs, outcomes, how many wait for you, tokens, and each project's runs with its newest decisions, results and blockers. `jarvis digest 72` looks further back.

## Leads and contacts

For prospecting, JARVIS keeps a contact list instead of a spreadsheet it has to rewrite: `jarvis.contacts.save|search|stats`. Two entries that are really one person (the same person under two spellings of the company, or the same phone number) are found with `jarvis.contacts.duplicates`, which shows what a merge would make of them and changes nothing; `jarvis.contacts.merge` folds one into the other after you approve it: the kept entry's words win, the other fills the gaps, a different value goes into the notes, and do-not-contact on either side stays. The removed entry is shown whole in the answer. Saving the same lead twice updates it, a status tracks where it stands
(`new`, `contacted`, `replied`, `meeting`, `won`, `lost`, `do_not_contact`), and a lead found inside a project is filed under it. Save/import cannot move an existing contact out of `do_not_contact`; only your owner status controls can, and `jarvis.gmail.send` refuses a stored suppressed address (you still approve every send). [Contact deletion](contacts.md) follows normal approval/standing-trust policy even for `do_not_contact`: deleting removes the record and its restriction.
In the console, **Ops → Contacts** lists them with a status selector on each row (this is also where you lift a `do_not_contact`). `jarvis contacts list [--status S] [--find TEXT]`, `jarvis contacts stats`, `jarvis contacts add COMPANY --email ... --status ...`, `jarvis contacts remove ID`, and `jarvis contacts export > contacts.csv`.
The list is local: nothing in it is sent anywhere, and writing to a contact does not email them.

## Moving existing work into a project

If you already run something by hand (a long objective on a schedule, a worklog file the model keeps), create the project, move the standing instructions out of the objective into the guidance, and file the
existing schedule under it: `jarvis schedule project ID NAME` (or the project selector on the schedule's card on the Ops page; `--none` takes it out again). Then shorten the schedule's objective to what is different each time ("check for replies and follow up").
