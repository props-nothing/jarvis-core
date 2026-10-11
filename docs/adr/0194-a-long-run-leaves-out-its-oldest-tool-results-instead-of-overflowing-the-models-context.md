# ADR-0194: A long run leaves out its oldest tool results instead of overflowing the model's context

Status: Accepted
Date: 2026-10-11

## Context

A run is a loop, and the whole transcript is sent on every model call. A tool result may be up to 12,000 characters (`MAX_MODEL_FACING_RESULT_CHARS`) and every one stayed in the transcript for the rest of the run. The loop allows 400 model calls and 1,200 tool calls, but about forty large results already exceed a 128,000-token window, and the provider answers `model_context_overflow`: the run fails with the work half done. This is the kind of task an autonomous assistant exists for (read many files, search many pages, work through a mailbox), so it is the limit that matters most for autonomy. Only a *parked* transcript was bounded (`fit_transcript`, `P3`); a live one was not, and `ADR-0165`/`ADR-0191` bound spend, not size.

Options considered: (a) summarise old turns with a model call, which costs tokens on every long run and can invent; (b) drop whole old turns, as the parked transcript does, which loses the model's own reasoning and calls; (c) leave out the bulk of old **observations** and keep everything the model said and did. (c) is deterministic, free, and loses only what can be fetched again.

## Decision

1. **Before each model call**, if the transcript is over a soft budget of 200,000 bytes (about 50,000 tokens; bytes over-estimate tokens, the direction `estimate_tokens` already errs), the oldest large tool results are replaced by one line (`[earlier tool result left out to keep the working context small: N characters. Run the tool again if it is still needed.]`) until the transcript is at three quarters of the budget. Down to a low-water mark, not one result per call, so a provider's prompt cache is broken once and then left alone for many calls.
2. **Never touched:** the six newest tool results; **every result the model has not yet answered** (those after its newest assistant turn, however many calls it made at once); every assistant turn and its tool calls (so each result still answers a call that is still there); the messages assembled before the run's own turns (brief, journal, memory, history); discovery results (`jarvis.tools.search`), because the offered tools are re-derived from them on every call; results under 600 bytes.
3. **No part of the old result is quoted in the placeholder.** A shortened head of fenced untrusted text could lose its closing fence, so only the size is stated.
4. **The record is unchanged.** The full result is in the run's stored events and the call's audit entry. The trim is visible as an `ActivityUpdated` event (`context_trimmed`, with how many results and bytes).
5. **No new model call, no new setting.** The constants are code; a profile with a small-context model is a reason to measure and then add one.

## Consequences

- A run can take far more steps than fit in the window, provided no single step needs more than the remaining room.
- The model may re-run a tool for something it no longer sees; that is a cost in steps, not a failure. A model that does so repeatedly is stopped by the existing repeat guard.
- Large *tool call arguments* (a file written in one call) were not shortened by this change. **Closed by `ADR-0199`** (2026-10-11): found in the owner's own runs (a research sub-agent that saved its findings reached 176,535 tokens in 219 calls), a second pass now leaves out long text in old, already-answered calls' arguments.
- A run whose single turn asks for more result than the window holds (thirty large reads at once) is not helped: the model must be able to read what it asked for, so those are never left out, and the request can still overflow. That is the old failure, not a new one.
- **Found by running it live, and fixed (CORRECTION to the first version of this ADR, 2026-10-11):** the first version protected only the six newest results. A real model (`glm-5.3-flash`) asked for thirty file reads in **one turn**; the trim ran before its next call and left out twenty-four of them before it had read them. It said so, re-read in batches, and then listed values it had not seen. The rule "results the model has not yet answered are never left out" (decision 2) and two tests came from that run.
- **Not measured:** how often a real model needs a result it no longer sees, and whether 200,000 bytes is the right budget for each provider. `jarvis eval` has no long-task suite yet; adding one is the way to tune this.
