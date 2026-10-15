# ADR-0215: Native recall searches the task before falling back to recent memory

**Status:** Accepted

**Date:** 2026-10-15

## Context

The owner selected task-aware recall after the capability review. This advances a narrow memory slice ahead of the remaining MCP and distribution work; it does not complete `P11-014`, add an embedding provider, or reopen the parked workflow engine.

The native executor previously loaded the newest 24 eligible memory rows without a question-specific candidate search. Domain scoring existed, but an older answer outside that window could never reach it. Retrieved claim text also reached the model without the memory ID and source locator needed to identify its origin.

## Decision

1. **Search canonical memory before limiting by recency.** The SQLite reader compares up to 32 distinct, Unicode-case-folded query words against the content-word portion of existing search keys. Query terms are bound values, not SQL or search syntax. The type/entity prefix of a search key is not claim text. There is no migration, extra text index, dependency, remote request or background ingestion.
2. **Eligibility precedes the candidate limit.** SQL scopes workspace, active status, permitted memory types and destination sensitivity, and excludes model-inference sources and task-like predicates. Domain eligibility checks validity, trust and confidence before adding a row. Candidate pages share a read transaction, so more than one page of expired claims cannot hide an eligible older answer. The native path considers at most 128 eligible records; the storage API refuses a requested limit outside 1–256.
3. **Task word coverage comes first.** The native query removes a small, explicit set of English/Dutch question filler words, preserving negations. Matching claims precede unrelated context; greater query-word coverage precedes lower coverage. Ties use the existing multi-signal score and stable memory ID. Up to 24 usable claims are offered, retaining ordinary contextual fallback when there is no lexical match. Existing scoring weights are unchanged. Keyword normalization now follows the same punctuation and Unicode rules as stored search keys.
4. **A citation is data, not authority.** Native recall supplies the memory ID, source kind and locator, confidence, creation time and validity bounds inside the same fence as the claim. Metadata neither upgrades source trust nor makes unusable content eligible. Context token accounting includes the complete isolated payload. The existing plain `RetrievedMemory::new` contract remains available to other readers.
5. **Record selection without copying claims into audit.** The `context assembled` event gains `memory_retrieval`: strategy, bounded candidate/result counts and selected references with coverage, total score and `task_match` or `context_fallback` reason. These are offered records, not a claim that the final context budget included every one; existing manifest inclusion counts remain authoritative.

The workspace is taken from the stored run and the destination ceiling from model placement, with an unknown placement treated as remote. Approval and memory admission/correction/deletion policy are unchanged.

## Consequences

- An old answer can now be selected even after hundreds of unrelated newer claims.
- Corrected, deleted, expired, inferred and out-of-scope claims do not become current answers merely because their words match.
- Provenance consumes prompt space, and the existing context budget accounts for it. No token-cost or live-answer-quality improvement is claimed without a measurement.
- The reader scans existing normalized keys; candidate and page memory are bounded, but total database work is not constant. Large or expiry-heavy corpora can require multiple scans. Measure latency before selecting a dedicated index.
- This is lexical recall, not semantic understanding. Synonyms without a shared word, other-language filler, aliases, relationship traversal, project-note ranking, automatic file retrieval and multi-turn query rewriting remain future slices. Existing project context and explicit file/knowledge tools are unchanged.
- Explicit memory search/API readers are not silently assigned these new native ranking semantics.

## Acceptance evidence

- Real SQLite/folder-independent fixtures show old matches outside the former window, exact token boundaries, Unicode case and punctuation, and filtering across 130 expired rows.
- Negative tests cover workspace/destination/type/source restrictions and task-like claims. Local and remote sensitivity ceilings are tested separately.
- A deterministic 30-question English/Dutch fixture benchmark retrieves the expected old fact first in 30/30 cases; the previous newest-24 candidate baseline contains 0/30. This measures retrieval postconditions, not a live model's answer quality.
- A scripted-model executor test inspects the actual model request and its stored selection event, not just an assistant's report.
- Correction/forgetting and provenance/fence/token-accounting tests exercise the real storage and conversion paths.
- Exact commands and current results are recorded in [the acceptance ledger](../../TODO.md#owner-selected-task-aware-memory-2026-10-15). Owner-account/live-model and native macOS/Linux verification remain open.

## Alternatives

- Increasing the recency window still misses older answers and inflates prompts.
- Adding vectors first adds a provider/index lifecycle without first measuring the existing lexical path.
- Indexing mail, project notes and granted files automatically in the same change widens both scope and data-flow review; those need their own acceptance slices.
- Letting recency or a weighted total outrank all task matches repeats the original failure for sufficiently old claims.

## Revisit when

A representative, owner-approved question set demonstrates misses from synonyms or aliases, a measured corpus shows unacceptable query latency, or a named workflow needs project/file evidence in automatic context. Add the smallest independently verified retrieval signal or index then, preserving the same eligibility and provenance boundary.
