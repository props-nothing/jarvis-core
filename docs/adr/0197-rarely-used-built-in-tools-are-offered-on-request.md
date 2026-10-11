# ADR-0197: Rarely used built-in tools are offered on request

Status: Accepted
Date: 2026-10-11

## Context

Every model call carries the definition of every tool it is offered. `ADR-0176` held back the tools of a big MCP server (the browser's 25) until the model searches for them, but JARVIS's own 44 tools were always offered: about 9,000 estimated tokens on every call of every run. A measured run (`jarvis eval run evals/basics.toml`, 2026-10-11) showed 8,214 input tokens for the request "reply OK". Autonomous work multiplies the number of calls, so a fixed cost per call is the cheapest thing to remove.

## Decision

1. **Two built-in families are deferred like a big server:** `calendar` and `schedule` (about 1,450 estimated tokens). They appear as the groups `jarvis-calendar` and `jarvis-schedule`; the `jarvis-` prefix means they cannot be confused with an MCP server called `calendar`. The model sees them named in the description of `jarvis.tools.search`, a search or a call of one of their tools offers the whole group for the rest of the run (read back from the transcript, so a run that parks for an approval and resumes keeps it), exactly as for the browser.
2. **Chosen by use, not by size.** A family stays offered when routine work uses it: files, web, mail, memory, contacts, projects, delegation, and the skill proposal that makes the learning loop work. One search costs about one extra model call, so only families that most runs never touch are worth deferring. The list is one constant (`DEFERRED_NATIVE_FAMILIES`), changed with a measurement.
3. **Authority is unchanged.** Discovery changes only what is *offered*. Policy, approvals, scopes, caps and the audit see every call the same way whether it was offered or not.
4. **Search ignores filler.** Ranking now drops words of one or two letters and common filler words (`what`, `when`, ...). A question about the calendar had also opened the 25-tool browser group because "on" occurs in one of its descriptions; that cost more than it saved.

## Measured

`basics`, `glm-5.3-flash`, one run each: per-call baseline 8,214 to 6,987 input tokens (−15%), suite total 117,552 to 100,224, 9/9 both times. `discovery` (browser, schedule and calendar found by search, plus a case that must not search for nothing): 4/4 on two runs. Finding a deferred family costs extra model calls (a "list my schedules" run used 30k input tokens), so a run that really needs calendar or schedule tools spends more than before; a run that does not spends less on every call.

## Consequences

- A cheaper model that never searches would answer "I cannot schedule" where it used to act. `evals/discovery.toml` now holds a case for each family; run it before a model is made the main or background one.
- Not done: deferring more families (contacts, projects, Gmail) or shortening tool descriptions. Both need the same measurement, and a project run uses the project tools on most calls.
