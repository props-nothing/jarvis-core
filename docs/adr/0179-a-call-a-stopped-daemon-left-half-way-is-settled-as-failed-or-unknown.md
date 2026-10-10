# ADR-0179: A call a stopped daemon left half-way is settled as failed or unknown, and the continuation is told

Status: Accepted
Date: 2026-10-10

## Context

`ADR-0174` stopped an approval from asking for an answer once its call had started, but left the call itself unfinished: a call that was running when the daemon stopped stayed at `authorized` or `submitted` for ever. Nothing recorded whether its effect happened, nothing told the owner, and the restart continuation (`ADR-0155`) told the model only to "look at what was already done". For a call that sends or changes something, "nobody can tell" is the single most important fact, and it was being lost.

## Decision

1. **Settle at start, before anything is served.** `settle_orphaned_calls` runs while no other process can be driving a call:
   - A call still at **`authorized`** never reached an adapter (the pipeline records `authorized`, then `submitted`, and only then hands the call over), so it is settled as **`failed`**: "the daemon stopped before this call was sent, so it did not run". Repeating it is safe.
   - A call at **`submitted`** may have reached the adapter with the answer lost, so it is settled as **`unknown`**: "the daemon stopped while this call was running, so it may or may not have happened". It is not safe to repeat blindly (`ToolOutcome::may_have_had_an_effect`), and it is never recorded as `failed`, which would claim it did not happen.
   - Calls at `requested` (held for an approval, durable by design) and terminal calls are untouched. Running it twice finds nothing.
2. **The continuation is told by name.** When a restart continues an interrupted run, its objective now lists the tools whose calls ended `unknown` and says to check before repeating any of them. Nothing is added when there are none.
3. **It is logged.** The daemon logs the settled calls with their outcomes at start.

## Consequences

Tested: a storage test covers all four states (requested and confirmed untouched, authorized to failed and repeatable, submitted to unknown and not repeatable, a second pass settles nothing); a unit test covers the continuation text. Run live on a throwaway profile: a real `jarvis.memory.search` call was rewritten to `submitted` as a mid-call stop leaves it, the daemon was started, and the row became `unknown` with the stated reason and the settlement was logged.

Not done: showing the owner an "unknown" call in the console (it is in the database, the log and the continuation prompt, not yet on screen); distinguishing a tool whose effect can be checked automatically (a file write) from one that cannot (a sent message); and a call interrupted while still `requested` with no approval record.
