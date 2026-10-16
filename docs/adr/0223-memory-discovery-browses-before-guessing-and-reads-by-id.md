# ADR-0223: Memory discovery browses before guessing and reads by ID

- Status: Accepted
- Date: 2026-10-15
- Scope: owner-requested memory discovery, explicit model readers and HUD inspection
- Related: [memory architecture](../architecture/memory-and-context.md), [native recall](0215-native-recall-searches-the-task-before-falling-back-to-recent-memory.md), [tool contracts](../api/contracts.md#explicit-model-memory-readers)

## Context

An agent that only has keyword search must guess how the owner worded a claim. Empty results cannot distinguish different wording from missing memory. The former explicit reader also searched only the newest 200 records, so an older claim could be invisible. A short search snippet is not a sufficient source for a detailed answer. The owner needs the same discover-to-inspect path in the HUD without exposing historical or private records to model tools.

## Decision

1. Register three version-2 read-only tools under the existing `memory.read` scope: `jarvis.memory.list` for keyword-free previews, `jarvis.memory.search` for lexical matching, and `jarvis.memory.read` for full sourced details by exact ID. Tool descriptions direct the agent from unknown wording to list, then read; previews are explicitly incomplete data.
2. List is live keyset pagination: newest whole-second creation time, then descending memory ID. Copy `next_cursor` unchanged, reset after filter changes, and stop at null. Default 10, maximum 20, with exact memory-type/entity filters. Storage scans bounded raw batches and applies domain currency/eligibility before the page limit. Same-timestamp pages cannot skip or duplicate stable records. New inserts before the cursor do not shift later pages; this is not a snapshot export.
3. Explicit model search scans the eligible corpus in bounded pages rather than a newest-200 window, retaining only the best 1–10 matches (default 5). Match whole Unicode-normalized words, rank coverage first, then importance, recency and stable ID; preserve related partial matches. Report coverage, match counts, previews and truncation. The compatibility `memories` field has an intermediate 16,000-rendered-character budget. Final serialized responses fit the shared 12,000-character and 32 KiB UTF-8 caps: omit search detail blocks first, then whole entries with accurate counts/truncation; adjust browse cursors to the last retained ID. Never cut JSON or fences; refuse an individually oversized response. Read an exact ID for full detail. No embedding service or new index is introduced.
4. Authority is out of band. The deterministic tool pipeline attaches authenticated workspace and verified model-destination ceiling to each execution request, including resumed calls. They are never model arguments. Missing context refuses the read; remote/unknown placement defaults to Internal, while verified local placement can use Restricted. Existing scope, schema, policy, deadline and audit checks still apply. This context is not a new permission grant.
5. Every model reader applies the current run's eligibility, excluding proposed, deleted, superseded, expired, future-valid, working/conversation, inference-source and task-plan claims. Domain retrieval now explicitly refuses `valid_from > query.at`, accepting equality. Missing, foreign and withheld exact IDs return the same unavailable response. Full provenance and previews are neutralized/fenced data, never instructions; isolation failure refuses/omits the content. Result definitions allow Restricted output so they do not mislabel local-only data as Internal.
6. Add authenticated `POST /api/v1/memories/browse` for owner-facing plain-text previews with the same page/filter shape. The HUD Operations memory inspector browses, reads complete details through the existing owner route, and uses existing version-guarded correction/forget routes. Forget requires inline confirmation and prevents relearning. Stale async responses cannot overwrite newer selections. No HTML from memory is inserted.

## Consequences

- Discovery no longer requires a successful keyword guess, and an exact read avoids answering from a truncated preview.
- Owner inspection and model disclosure remain different surfaces. Existing owner list/read/export retain historical access; the owner browse route is current durable discovery with the local ceiling. Existing owner API search keeps its prior ranking/candidate behavior; this ADR changes explicit model search, not every search implementation.
- Total corpus-scan work is not constant, despite bounded pages/top-N storage. The ordinary tool deadline still applies. Measure realistic latency before selecting an index or changing the timeout. Lexical search does not resolve aliases, synonyms, relationship traversal or semantic translations.
- Pagination is live; edits/deletions during traversal can alter later pages. Refresh for a new view; use owner export when historical completeness is required.
- No dependency, migration, new provider flow or automatic memory admission is added. Native recall keeps its separate task-aware ranking and context budget.
- Explicit readers independently fence provenance and claim when their combined rendering exceeds one isolation block, preserving discoverability and full detail for valid maximum-size claims without widening native recall budgets.

## Evidence and remaining acceptance

The 2026-10-16 HUD follow-up restored the missing Operations memory inspector against the existing owner routes, without changing the protocol or authority boundary. It loads on opening Operations, keeps submitted pagination filters and unsaved drafts, refuses writes without the inspected identity/version, and ignores stale browse/read results. Late write completion cannot replace another selection. The original seven UI cases and ten additional failure/race regressions now execute; current gates and limitations are recorded in [the UI repair ledger](../../TODO.md#owner-requested-memory-inspector-repair-2026-10-16).

The 2026-10-16 release repair reconnected the documented owner browse route and native tool-round destination ceiling after their callers were removed. Router regressions check credential enforcement, rejected authority fields, bounds and pagination. An executor regression verifies that restricted exact-ID reads reach a verified local model, while remote and unknown models receive only an unavailable result. Current repair validation is recorded in [the release repair ledger](../../TODO.md#owner-requested-release-build-repair-2026-10-16).

Final settled Windows gates on 2026-10-15 passed on the frozen source:

- `cargo build --workspace --all-features --locked`
- `cargo fmt --all --check`
- `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings`
- `cargo test --workspace --no-fail-fast --locked`: **3,092 passed, 0 failed, 4 ignored** across **61 result summaries**.

The settled test log includes passing maximum-4,096-character discovery/full-read, serialized character/UTF-8 cap and no-skip cursor regressions. The four local logs (`jarvis-coding-settled-{build,format,clippy,tests}.log` in the validation environment's temporary directory) were inspected before recording this evidence. No daemon restart or private-profile access was performed.

Real-store tests cover records older than 500 newer rows, whole-word/Unicode matching, words late in long claims, equal-timestamp paging, raw-batch expiry starvation, exact type/entity/workspace filters, withheld states, malformed authority arguments, missing trusted context, read provenance, correction and forget. Gateway tests exercise authentication and bounds; pipeline tests exercise actual reader dispatch and audit. A core boundary test proves future-valid claims fail closed until their validity instant. Isolated Node HUD tests cover browsing, paging, full safe text, versioned correction, confirmed forget, stale responses and errors.

These are automated fixtures, not a live-model answer-quality study or desktop/mobile visual acceptance. Real owner-corpus latency, provider answer quality and native macOS/Linux acceptance remain open in [the backlog](../../TODO.md#owner-selected-memory-discovery-2026-10-15).
