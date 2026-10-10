# ADR-0183: The model proposes skills, the owner approves them with one click

Status: Accepted
Date: 2026-10-11

## Context

`P11-004`. JARVIS had a store and a lifecycle for skills (procedures with provenance, `ADR-0117`), and it already offered an approved one to a run. Two things were missing. **Nothing let the model write one**, so nothing ever filled the store except a person at a terminal. And **selection could not fire in practice**: a skill was offered only if every word of the run's objective appeared in the skill's text, so a real request ("Please research the Dutch company Coolblue and summarize what they do") never matched a procedure approved the week before.

## Decision

1. **`jarvis.skill.propose`** (native tool, risk 1, no approval, scope `skill.propose`): a description whose first line says when to use it, and one to twelve steps, each a tool and an instruction. `SkillService::propose` records it as `Proposed`, source `ModelInference`, authored by the actor `agent:model`, so the owner's approval is never a self-approval (`ADR-0117` §4). Each step must name a registered tool and records that tool's current version. The same description twice returns the one already waiting; at most ten wait at once.
2. **Approval is one click.** `POST /api/v1/skills/{id}/approve` and `/dismiss` take no body. The approver is the local owner, read from the identity, and the version counter is read by the daemon, so a client carries nothing to get wrong. Dismiss archives (kept for audit, never offered). The console shows a "learn this?" card with the steps in the same place as "remember?".
3. **A second selection rule.** `matches_trigger`: the first line of the description is the trigger; a skill is offered when the request contains at least 75% of its distinctive words (at least two; short words, stop words in English and Dutch, and a plural `s` do not count). The literal rule remains and either one offers. It stays word-bounded and literal; an offer costs little because the skill reaches the prompt fenced as data, only after approval, at most eight at a time.

## Consequences

Run live on a throwaway profile with a real model (`deepseek-v4.1-flash:cloud`): asked to write down how to research a Dutch company, the model called the tool and proposed five steps; the console showed the card; Approve made the skill active, promoted by the owner. **A/B:** "research the Dutch company Coolblue and summarize" then ran the procedure's last two steps (`contacts.search` three times, `contacts.save` three times); after Dismiss, "research Bol.com and summarize" did neither. So the skill changed what the run did, and dismissing it removed the effect. Tests: 2 domain (trigger, one-word trigger), 5 tool (not used until approved, owner is approver, dismissed never offered, dedupe and cap, bad input stores nothing).

Not measured: fewer steps or tokens on a task the skill makes shorter (the `jarvis eval` comparison `P11-004` asks for); the live run shows a behaviour change, not an efficiency one. Not built: editing a proposal before approving; a Telegram approval button (`P11-005`); the model revising an existing skill (the store supports a declared correction, the tool does not yet); skills made of other skills. Matching is still by words, not meaning.
