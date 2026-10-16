# ADR-0178: A tool that reaches other people can be trusted by name

Status: Accepted (amends `ADR-0175` and `ADR-0177`)
Date: 2026-10-10

**Amended by:** [ADR-0180](0180-a-call-from-another-app-that-needs-the-owner-waits-for-the-owner.md) allows an eligible external MCP call to wait for the owner's decision rather than being refused merely because it needs approval. The external-call refusal in decision 3 below is historical; the external client still cannot approve its own call.

## Context

After `ADR-0175` (third-party tools) and `ADR-0177` (memory and project tools), one carve-out remained: a native tool with an external-communication effect (`jarvis.calendar.create`, `jarvis.gmail.send`) ignored "Run without asking". The owner's saved settings showed `calendar.create` as trusted while it still asked, and the owner asked, twice, why these tools were not configurable "like the rest". A choice the screen offers, saves and then ignores is the defect `ADR-0175` already named; leaving it for native senders was inconsistent.

## Decision

1. **One rule for every tool.** The external-communication rule is the default (a tool that reaches other people asks every time, and a tool nobody has classified is assumed to), and the owner's explicit, per-tool "Run without asking" replaces it for that tool. `WorkspacePolicy::external_rule_applies` is true for an outward tool unless it is trusted by name.
2. **The one choice that gets a second look.** In Permissions, choosing "Run without asking" for a tool whose effects include external communication asks for a confirmation that says why (a hostile page or email can try to steer it) and how to undo it. Declining leaves the setting unchanged.
3. **Unchanged.** Tightening outranks trust ("Always ask first", "Off"); the risk ceiling and scopes still apply; nothing is released that the owner did not name; and a tool the owner has not chosen keeps asking. Calls from an outside agent through Connect apps still cannot hold an approval and are refused for any tool that would ask.
4. **The mail switch is separate.** Sending mail also needs the Google "sending" option and its grant (`daemon.google_actions`); this decision does not touch that precondition, and it does not change any saved setting. `jarvis.gmail.send` stays as the owner set it (ask).

## Consequences

Tested: untrusted, a sender is held; trusted by name, it runs; trusted then tightened, it is held again; trusting one tool does not release another (`evaluation.rs`). Checked in a browser: the confirmation appears for an outward tool with the stated wording, declining reverts the dropdown, accepting saves and applies, and a read-only tool shows no dialog. The cost is the one every trusted tool carries, at its sharpest here: with `calendar.create` or `gmail.send` trusted, a hostile email or page that steers the model can invite people or send mail unasked. The owner takes that on per tool and can undo it per tool. The owner's live settings had `calendar.create` trusted, so it now runs without asking after this change.
