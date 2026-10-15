# Memory: finding the relevant past fact

[Documentation](../README.md) / [Projects](projects.md) / [Files](files.md) / [Architecture](../architecture/memory-and-context.md)

JARVIS keeps sourced memories separately from conversation history. In the native assistant, a new request now looks for relevant remembered claims before falling back to recent context. An old agreement need not be among the last 24 memories to be found.

For example:

- “When does Acme renew?”
- “What did we agree with Acme?”
- “Wat weet je over het contract van Acme?”

Naming the person, company or distinctive subject helps: this first slice matches words, not meanings. It ignores common English/Dutch question filler and handles case and punctuation, but cannot promise that unrelated synonyms or translations will match.

## What reaches the model

At most 24 usable memories are offered from a task-ranked candidate set of at most 128 eligible claims. A claim with matching words comes before unrelated context; existing importance and currency signals break ties. When no words match, fallback records are contextual information, not evidence that the question has been answered.

Each native recalled claim carries its memory ID, source kind and reference, confidence, creation time and validity bounds. The metadata and the claim are fenced together as data: neither can grant permission or override the request. The full rendered record counts against the context budget, which may exclude some offered records.

The run's `context assembled` event records the retrieval strategy and selected references/reasons without storing another copy of the remembered text. It separately records what the final context budget included.

## What does not change

- Remembering still follows existing admission and approval rules. This does not automatically store conversations or facts inferred by a model.
- Corrected, deleted, expired or ineligible claims are not promoted back into current recall by a word match.
- Workspace boundaries and the configured model's sensitivity ceiling still apply. Unknown model placement uses the remote ceiling.
- No new key, model, embedding service, migration or setting is required.
- Project journals and explicit memory search keep their existing behavior. Granted files remain accessible through the file tools and `jarvis.knowledge.search`; this change does not automatically send file or mailbox contents to a model.

## Verification status

Real-database tests cover older facts, correction/deletion, source/scope boundaries and Unicode matching. A fixed 30-question fixture benchmark recovers the expected fact first in 30/30 cases, compared with 0/30 in the former newest-24 candidate set. A scripted-model test checks the actual assembled request and stored audit event.

This is not a live-provider answer-quality measurement. Live-model behavior, latency/token comparisons on an owner-approved corpus and native macOS/Linux checks remain open in [the backlog](../../TODO.md#owner-selected-task-aware-memory-2026-10-15). The source change applies after rebuilding and starting the updated daemon; it does not restart an existing daemon automatically.
