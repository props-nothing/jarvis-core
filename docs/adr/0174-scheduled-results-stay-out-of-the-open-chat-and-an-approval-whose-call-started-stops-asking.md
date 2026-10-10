# ADR-0174: Scheduled results stay out of the open chat, and an approval whose call started stops asking

Status: Accepted
Date: 2026-10-10

## Context

Two things the owner hit while testing live on 2026-10-10:

1. **A question that could not be answered.** After an approval was given, the daemon was stopped while the call was running. The call had moved past `requested`, but its approval stayed `approved` and kept its stored arguments, which is exactly how a call that never started (daemon died between decision and release) looks. The list showed it as waiting, every press of Approve returned `409` (resume refuses a call that has started, so a second delivery cannot become a second effect), and the console said "It is still waiting".
2. **Scheduled results in the middle of a conversation.** When a schedule fired, the console wrote the task's objective and answer into whichever conversation was open.

## Decision

1. **A started call is not listed as waiting.** The approvals list hides an approved approval whose linked call is no longer `requested`, and the daemon drops the stored arguments of such approvals at start (`clear_arguments_of_started_calls`). The arguments are never used again, and keeping an action's payload on disk for nothing is a cost. `resume` still refuses such a call; that guard is unchanged.
2. **A `409` on an answer means "no longer waiting".** The console says so and refreshes instead of claiming it is still waiting.
3. **Scheduled results go to their own conversation.** A "Scheduled results" conversation in History receives each finished scheduled run; the History button shows how many are unread; the open conversation is never written to. If that conversation is the open one, the result is appended live. A spoken answer is no longer read out unprompted.

## Consequences

Tested: a gateway test approves without releasing, moves the call to `authorized` (as a dying daemon leaves it), and shows the approval is no longer listed and its payload is dropped. Run live in a browser on a throwaway profile: with a conversation open, a schedule fired every minute; the open conversation was unchanged, History showed `(1)`, opening "Scheduled results" showed the task and its answer and cleared the count.

Not done: a call left `authorized` or `submitted` still has no recorded outcome. Whether its effect happened is unknown and is not shown to the owner; recording such calls as `unknown` at start, and telling the owner so, is open work. Scheduled results are kept in the browser's storage with the other conversations, so they are not shared between browsers.
