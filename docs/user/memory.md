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

## Browse first, then read details

If you do not know how a fact was worded, the agent can call `jarvis.memory.list` with `{}` instead of guessing keywords. It receives up to 10 current previews and IDs, with `next_cursor` for the next page. An optional exact type or entity ID narrows discovery; the maximum page size is 20. Follow the cursor until null, or restart without it when changing filters. Pages are live, not a frozen export.

When you do know topic words, `jarvis.memory.search` scans eligible workspace memory, including claims older than 200 newer rows. It matches whole normalized words, not substrings or meanings. Complete matches rank first and related partial matches remain visible; use one to ten results, default five. Coverage and truncation fields distinguish incomplete output from absence of a match.

Both discovery tools return previews. Use `jarvis.memory.read` with the returned `memory_id` for the full claim, source, confidence and validity metadata before answering a detailed question. Full details and previews are fenced as data. These read-only tools require the existing `memory.read` scope and do not ask for a write approval. Workspace and model sensitivity are assigned by JARVIS, never by tool arguments; unknown/remote model placement cannot read local-only claims. Missing, withheld and foreign IDs all return unavailable.

Serialized model responses fit both the 12,000-character context limit and the 32 KiB UTF-8 tool-output limit without cutting JSON or fences. Large search responses omit compatibility detail blocks first, then whole preview entries if needed, reporting truncation; read returned IDs for details. Browse pages may contain fewer entries than requested: the adjusted cursor preserves every remaining record for later pages. An individual response that cannot fit safely is refused rather than silently cut.

A maximum-size claim remains discoverable and readable: when provenance and claim cannot share one isolation block, each is independently fenced rather than hiding or shortening the claim. Native automatic recall retains its existing combined-record budget.

### Inspect and correct in the HUD

Open **Operations → Knowledge → Memory**. Choose an optional type or exact entity ID, then **Browse / refresh**. **Older** and **Newer** move between current pages. **Read details** shows the complete claim and provenance, not only the preview. **Save correction** uses the inspected version, so concurrent edits are not silently overwritten. **Forget this memory** asks for inline confirmation and prevents relearning; cancel creates no write. Refresh/reopen details after a conflict.

Correction commits the replacement, entity links and supersession of the inspected original together. A stale version, already-superseded claim or failed write leaves no partial replacement behind; inspect the current claim before retrying. See [ADR-0227](../adr/0227-transactional-memory-corrections-commit-one-versioned-change.md).

The panel loads when Operations first opens and keeps an unsaved correction when you close and reopen it. Filters take effect on **Browse / refresh**; paging keeps the filters used for that page. A failed page request leaves the previous page available for reading or retry. **Reload details** deliberately replaces the current draft with the latest saved record. A failed save retains the draft and reports the error; a reply to an earlier selection cannot replace a newer selection or draft. Changes are disabled if the record identity or inspected version is missing.

The owner inspector uses the local sensitivity ceiling. Current discovery excludes proposals, historical states, future-valid claims, conversations/working memory, inference-source and task-plan records. Existing owner read/export remain the historical inspection surfaces.

## What does not change

- Remembering still follows existing admission and approval rules. This does not automatically store conversations or facts inferred by a model.
- Corrected, deleted, expired or ineligible claims are not promoted back into current recall by a word match.
- Workspace boundaries and the configured model's sensitivity ceiling still apply. Unknown model placement uses the remote ceiling.
- No new key, model, embedding service, migration or setting is required.
- Project journals and the owner API search retain their existing behavior; the owner search still has its older candidate limit. Granted files remain accessible through the file tools and `jarvis.knowledge.search`; this change does not automatically send file or mailbox contents to a model.

## Verification status

Final settled Windows validation on 2026-10-15 passed the locked all-feature workspace build, formatting, strict all-target/all-feature Clippy and locked workspace tests: **3,092 passed, 0 failed, 4 ignored** across **61 result summaries**. This includes the maximum-4,096-character full-read and safe-output/pagination regressions. The final logs were verified; no existing daemon was restarted and no private owner profile was accessed.

The discovery follow-up adds real-store coverage for old facts, equal-timestamp cursor pages, exact filters, source/sensitivity/workspace boundaries, immediate corrections/deletions, trusted pipeline dispatch and authenticated browsing. The 2026-10-16 UI repair restores the missing Operations panel; 17 isolated Node tests cover browsing, safe text, versioned writes, confirmation, stale replies, failed pagination, draft preservation and identity/version refusal without the owner profile or provider. Current validation is recorded in [the UI repair ledger](../history/todo-owner-evidence-through-2026-10-16.md#owner-requested-memory-inspector-repair-2026-10-16). Full-corpus lexical search has bounded pages/output but not constant total work; realistic corpus latency and live-model use of list → read remain open in [the discovery backlog](../../TODO.md#owner-selected-memory-discovery-2026-10-15). See [ADR-0223](../adr/0223-memory-discovery-browses-before-guessing-and-reads-by-id.md).

Real-database tests cover older facts, correction/deletion, source/scope boundaries and Unicode matching. A fixed 30-question fixture benchmark recovers the expected fact first in 30/30 cases, compared with 0/30 in the former newest-24 candidate set. A scripted-model test checks the actual assembled request and stored audit event.

This is not a live-provider answer-quality measurement. Live-model behavior, latency/token comparisons on an owner-approved corpus and native macOS/Linux checks remain open in [the active backlog](../../TODO.md#provider-and-user-flow-verification). The source change applies after rebuilding and starting the updated daemon; it does not restart an existing daemon automatically.
