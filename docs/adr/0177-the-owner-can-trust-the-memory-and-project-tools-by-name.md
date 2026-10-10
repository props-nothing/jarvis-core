# ADR-0177: The owner can trust the memory and project tools by name

Status: Accepted (amends `ADR-0155` point 1)
Date: 2026-10-10

## Context

`ADR-0155` put `jarvis.project.create|update|assign_schedule` and `jarvis.memory.correct|forget` on a list that policy never waives and that the Permissions screen refuses ("this tool changes what JARVIS is told to do or remember, so every call asks you"). The reason was sound: these tools write text that later runs receive as standing instructions or change what the owner remembers, and a prompt-injected model could use a blanket "always" to plant either silently.

The owner set every tool to run without asking, asked JARVIS to test every tool, and watched six approvals appear for these five tools and `gmail.send`. A refusal that stands in the way of the owner's own explicit, per-tool setting is a limit on the owner, not on the model, and the product's aim is a capable assistant with light approvals (`ADR-0136`).

## Decision

1. **The list becomes `STANDING_POLICY_TOOLS`: ask by default, trustable on purpose.** The five tools still declare an approval and still ask. The Permissions screen now accepts "Run without asking" for each, and the policy honours it.
2. **It stays deliberate.** The approval card never offers a one-click "Always allow" for them, and a spoken "yes" still cannot approve them (the text is read, not heard). The only way to release one is choosing it by name in Permissions, where it is the same dropdown as every other tool.
3. **Tightening still wins.** "Always ask first" and "Off" outrank trust, as for every tool.
4. **(Superseded by `ADR-0178`.) Native senders.** `jarvis.gmail.send` and `jarvis.calendar.create` still ask every call whatever is chosen (`ADR-0175`); the Permissions screen says "still asks: it sends things to other people". Sending mail or inviting people unasked is the one case where an injected page could do harm to someone else, and it needs its own decision.

## Consequences

Tested: each of the five asks by default, can be trusted by name, then runs, and is held again by an "always ask" override (`evaluation.rs`). Live, on a fresh profile: setting `project.create` and `memory.forget` to trusted was accepted and `/tools` reported they no longer ask while `memory.correct`, untouched, still did; JARVIS then created a project from a plain request with no approval waiting. The cost is the one `ADR-0155` named: with these trusted, a model steered by hostile text can create projects, rewrite a project's guidance or alter or forget memories without a question. The owner takes that on per tool, and can undo it per tool.

Separately, "I held back the hard-to-undo tools" in the owner's smoke test was the model's own caution, not a policy block; nothing prevented it from calling them. Whether a "test every tool" request should make the model run them without asking again is a prompt question, not a policy one.
