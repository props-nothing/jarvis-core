# Memory and context evidence through 2026-10-16

Archived on 2026-10-16 from the implementation ledger. This preserves the original dated claims, failures, corrections and unchecked items; it is **not current delivery status or work order**. Later entries and accepted ADRs may supersede earlier statements. Use the [active backlog](../../TODO.md) and [history index](README.md) for current disposition. Relative Markdown links were adjusted when this material moved.

---

## P4: Memory And Context

<a id="p4-001"></a>

- [x] `P4-001` Define memory types, provenance, confidence, validity, sensitivity, correction, supersession, and retention semantics.
      `crates/jarvis-core/src/memory.rs`, built from `docs/architecture/memory-and-context.md` rather than
      invented, and the module's central claim is the document's own sentence taken literally: **a memory is a
      sourced claim with lifecycle metadata, not an unqualified string**. `MemorySource` is a **required**
      field and `MemoryRecord::new` refuses a record without one, so the admission lifecycle's rejection step
      ("not supported by source" → do not persist as fact) is enforceable rather than a review note — there is
      no way to build the value the rule forbids.
      **The invariants are constructor rules, not advice.** Three are worth naming because each closes a path
      a caller could otherwise take:
      - **A model inference cannot exceed `Unverified`.** "Never persist unsupported inference as fact" is a
        refusal at construction, so a candidate the model produced cannot be recorded as `Confirmed` — the
        acceptance invariant "an inferred preference never appears as confirmed fact" has no way to be
        violated by a caller that tried.
      - **A provider record cannot back a preference.** The document's own example of trust that does not
        transfer between claim kinds ("authoritative for an event timestamp but not for a person's
        preference"), stated once at the constructor instead of left for each reader to apply.
      - **A source cannot claim a trust class its kind does not carry.** `MemorySourceKind::permitted_trust`
        is an equality, so `ExternalContent` cannot claim `Authoritative` — the injection boundary is a
        property of construction rather than of every consumer. The default is `ExternalContent`/`Untrusted`,
        because a default is what a deserialiser fills in when a field is absent and an unrecorded origin
        must not read as something the user said.
      **Deliberate design choices, each with a rejected alternative:**
      - **Confidence is four named levels, not a float.** A `f64` invites arithmetic that means nothing (two
        confidences averaged are not a third) and lets a caller threshold at `0.6` with a meaning that changes
        across builds. The levels are ordered for a **floor** comparison and never for arithmetic.
      - **`Expired` is not a stored status.** Same split as `ApprovalState`: a stored `expired` would need a
        sweep job for correctness and would make a row restored from a backup wrong.
        `MemoryStatus::effective_at` derives it, and `EffectiveMemoryStatus` distinguishes `Expired` (lapsed)
        from `Superseded` (replaced) because those are different answers to "why is this not being used".
      - **Both supersession directions are stored.** `supersedes` alone would make "is this claim still
        current" a scan of every later memory, and that question is on the retrieval path.
      - **`Proposed` exists.** A candidate deterministic code cannot support as fact is stored as a proposal
        rather than dropped, which is what makes "the model may propose candidates" produce something.
      - **`Relationship` memories start as proposals.** Derived from the type at construction rather than
        taken from the caller, so "high-impact identity and relationship inferences require explicit user
        confirmation" cannot be skipped. `Semantic` is deliberately **not** flagged: a semantic claim can be a
        medical or financial fact and the type alone cannot say, so flagging it would refuse an ordinary fact
        about a city — the per-claim classification is `P4-003`'s, where the content is read.
      - **Deletion clears the content in the domain as well as in storage.** A value already held cannot be
        returned by a caller that kept a reference, so the domain — not only the repository — is unable to
        produce deleted text. The row's existence is retained so a source link and an audit record resolve.
      - **A repeated entity keeps the weakest match basis.** A reader of one memory cannot tell which mention
        it is looking at, so one confident mention must not launder a later guess about the same entity.
      - **The search key normalizes case, whitespace, and word order only.** No stemming, no stop words, no
        synonyms: each would make two *different* claims collide, and over-collapsing loses information the
        user gave, while under-collapsing a duplicate costs one row.
      **Falsified, one guard each:** forcing the model-inference check false made
      `a_model_inference_cannot_claim_confidence` fail at `Uncertain`; forcing the provider-preference check
      false made `a_provider_record_cannot_back_a_preference` fail. Both restored and re-run green —
      **155 `jarvis-core` tests.**
      **⚠ Naming defect found and fixed while writing the tests:** the supersession transition and its
      accessor were both `superseded_by`, distinguishable only by arity. A reader could not tell an act from a
      query, and the act is the one that changes state — renamed to `replace_with`.
      Gates: fmt, clippy `-D warnings`, 45 suites with the application binaries absent and **zero skips**,
      `cargo deny` ok.
      **Recorded as a limit:** nothing persists a memory yet. This slice is the vocabulary and its invariants;
      `P4-002` owns the tables, and the embedding fields in the canonical record are `P4-005` — a memory is
      fully usable without them, because "embeddings are one signal" and not a requirement.
<a id="p4-002"></a>

- [x] `P4-002` Implement memories, entities, aliases, relations, sources, and deletion tombstones in SQLite.
      `crates/jarvis-storage/migrations/sqlite/0009_memories.sql` and
      `crates/jarvis-storage/src/memory_repository.rs`. Six tables — `memories`, `entities`,
      `entity_aliases`, `entity_relations`, `memory_entities`, `memory_tombstones` — and the slice's real
      work was deciding **where each rule is enforced**, which turned out to be three schema decisions and
      one domain-constructor one. Recorded as `ADR-0044`.
      **A structured claim is three columns, not one JSON document.** `docs/data/schema.md` says JSON stores
      "versioned provider payload fragments or flexible metadata, **not core relationships that need
      constraints**", and a subject/predicate/object triple is exactly such a relationship. The first cut
      stored one bounded document, which permitted an 8000-byte subject and an empty object; three columns
      make each part boundable, make "all three or none" unrepresentable rather than a reader's check, and
      turn a lookup by predicate into an index seek.
      **The tombstone stores `SHA256(search_key)`, not the key.** The acceptance invariant is "deleting it
      removes text **and derived indexes**", and a search key *is* derived text — it holds the claim's
      words, case-folded and word-sorted. Keeping it would have satisfied "resurrection is blocked" while
      violating "the text is gone", and the violation would be invisible because no retrieval reads the
      tombstone table. The same technique `ADR-0018` uses for an intent digest.
      **"A relationship memory starts as a proposal" is deliberately *not* a table `CHECK`.** A `CHECK`
      applies to every write, not to the insertion, so a constraint reading "a relationship row is never
      `active`" would forbid the *confirmation* it exists to require. The invariant is about creation, and
      only the constructor can express that — the same distinction `ADR-0041` draws about a sandbox
      guarantee: a rule enforced in the wrong place is not a stricter rule.
      **The stored status had to travel into the constructor.** The first cut decoded a record and then
      stamped the status on, which could not work: the content rule refused an empty content *before*
      anything could say that emptiness is what `deleted` means, so a tombstone could not be read back at
      all. `MemoryRecord::from_stored(parts, state)` now takes the status with the parts, which makes both
      halves of one rule expressible — empty content is permitted **only** in `Deleted`, and a `Deleted` row
      that still carries text is `InvalidMemory::DeletedRetainsText`, the decode-side counterpart of the
      schema's `CHECK`. Two independent enforcers, which is the argument the approval and tool-call
      repositories already make.
      **A compiler finding worth recording:** `read_entity_memories`'s statement takes a third parameter,
      and the shared runner had an `entity_id` argument it **never bound**. The query still ran and
      silently ignored the entity filter; only the arity check surfaced it. The runner now binds it as an
      `Option`, so a caller cannot supply the entity-scoped statement without its entity.
      **Falsified, one guard each:** forcing `is_tombstoned` to return `false` made
      `deletion_removes_text_and_blocks_a_re_ingest` fail; removing the workspace scope from the read
      statement made `a_memory_cannot_be_read_from_another_workspace` fail. Both restored, re-run green.
      **Two test-fixture defects found and fixed by the failures:** the seeded workspace insert omitted
      `mode`/`data_policy`, and `a_replacement_cannot_be_rewritten_in_storage` recorded rows with fresh
      random identifiers and then linked two different ones — so it asserted a link to nothing and passed
      only because the foreign key refused it. A fixture that cannot satisfy its own precondition is worse
      than no test, because its failure reads as a defect in the code.
      **Recorded as limits:** nothing retrieves or ranks a memory yet (`P4-004`; the reads are recency-ordered
      only, and the embedding columns are `P4-005`), duplicate detection is exact-search-key only, the entity
      links are written after the row rather than in one transaction with it (the safe direction, which is
      not the same as atomic — `P4-008` needs the atomic form), and `entity_relations` has no cycle check.
      Gates: fmt, clippy `-D warnings`, 45 suites with the application binaries absent and **zero skips**
      (1095 tests), all three phase gates with `ACCEPTANCE_REQUIRE_BINARIES=1` and
      `ACCEPTANCE_REQUIRE_FIXTURE_PEER=1`, `cargo deny` ok. `jarvis-core` 156 tests, `jarvis-storage` 167.
<a id="p4-003"></a>

- [x] `P4-003` Implement candidate extraction as a reviewable pipeline; never persist unsupported inference as fact.
      `crates/jarvis-core/src/candidate.rs` + `candidate/tests.rs` (19 tests). The document's admission
      lifecycle as stages, and the slice's central finding is that **one of its stages cannot be
      implemented the way it reads**. Recorded as `ADR-0045`.
      **`MemoryCandidate` is not a `MemoryRecord` with fewer fields.** It is the model's *proposal*, so it
      must be representable with a confidence its source cannot support, a sensitivity below the floor, and
      an entity set the store has not resolved — each of which a stage resolves or refuses. A record cannot
      be built until every rule passes (`P4-001` made that structural), so the two types cannot be merged
      without making every consumer defend against a half-valid memory.
      **⭐ THE FINDING: a correction cannot be inferred from a key match.** The first cut compared contents
      and called a same-key/different-content pair a correction. That branch is **unreachable**, because
      `P4-002`'s search key *is* the sorted, case-folded word set: two records sharing a key have the same
      words, so a key lookup can only return a memory whose words match, and `normalized_equal` agreed with
      the key by construction. The test asserting a `Correction` got `Duplicate`, and the tempting repair —
      "make the fixture differ more" — was wrong: the fixture was already as different as a shared key
      permits, which is not different at all. Inferring from "same entity, different words" was the next
      candidate and is worse: "Alice is my sister" and "Alice lives in Rotterdam" share an entity, so it
      would retire a memory for every second fact about anyone.
      So **supersession is declared** (`MemoryCandidate::supersedes`), checked before the comparison because
      it asserts the candidate's own history rather than observing the store, and it outranks the
      fact-or-proposal decision — a corrected *relationship* claim must supersede, not become a proposal.
      `compare`'s `Correction` branch is kept as a **diagnostic**: a caller supplies `existing`, so a caller
      passing a row from a wrong lookup would otherwise have it reinforced as though it held the candidate's
      claim; returning `Correction` makes that bug surface as a visible supersession instead of a silent
      merge.
      **A second finding from the same test run:** `MemorySearchKey` bounds each key *word* at 64 chars as
      well as the content at 4096, so a 4096-character single word is within the content bound and
      unkeyable. The first cut reported `Content` for both, which would send an operator shortening text
      that already fits — now `CandidateRefusal::Unkeyable`, with a test asserting both bounds (a
      single-word fixture conflates them, which is how the bug was found).
      **Confidence is lowered, never raised.** A model inference is admitted at `Unverified` **and** as a
      proposal whatever it claimed; a document claim caps at `Likely`; a caller claiming *less* certainty
      than its source could support is not overruled. The invariant is therefore a property of the stored
      value rather than a check a later reader must remember.
      **Sensitivity is the maximum of three floors** — the type's (a relationship claim is `Confidential`),
      the content's (a credential or health term raises it to `Restricted`), and the extractor's. Only one
      direction is safe to correct automatically, because exclusion from a remote model is decided from the
      *stored* level.
      **Falsified, one guard each:** removing the confidence cap failed exactly
      `a_model_inference_is_never_admitted_as_fact` and `an_overconfident_claim_is_lowered_rather_than_refused`;
      making the proposal branch unreachable failed exactly the two tests about inferences and relationships.
      Both restored, re-run green.
      **Recorded as limits:** nothing extracts a candidate from anything yet (the model-facing half is
      `P4-007`, where context assembly and the provider boundary live), **nothing writes an admission** so
      the end-to-end property is unproven, nothing reads the store (`CandidateContext` carries borrowed
      values because entity resolution needs `jarvis-storage`), the restricted-content check is a keyword
      list rather than a classifier, `supersedes` is a single link with no chain walk, and a candidate
      carries no identifier so the domain's self-supersession refusal belongs to `P4-001` where the
      identifier does.
      Gates: fmt, clippy `-D warnings`, 45 suites with the application binaries absent and **zero skips**
      (1114 tests), all three phase gates with `ACCEPTANCE_REQUIRE_BINARIES=1` and
      `ACCEPTANCE_REQUIRE_FIXTURE_PEER=1`, `cargo deny` ok. `jarvis-core` 175 tests.
<a id="p4-004"></a>

- [x] `P4-004` Implement exact, full-text, recency, importance, entity, and workspace retrieval before adding embeddings.
      `crates/jarvis-core/src/retrieval.rs` + `retrieval/tests.rs` (21 tests, `jarvis-core` 196). The
      document's two retrieval stages as pure functions over records. `ADR-0046`.
      **Eligibility is a filter, not a low score** — and three of the six rules could not be ranking signals
      at all. A foreign workspace's memory is not a poorly-ranked result; ranking it and dropping it later
      would make one user's score depend on what another user stored. A superseded or expired claim is not
      "less relevant" but "not current", and the document requires it stop being retrieved *as current* — a
      ranker that surfaced it would need every consumer to re-check. A memory above the destination's
      ceiling must not be admitted and then redacted, because a redaction transforms content already inside
      the assembly step.
      **"Actor authorization" and "workspace policy" are one check here, recorded rather than silently
      merged.** In this platform actor authorization *is* workspace membership — `P4-002`'s reads all bind
      `workspace_id` — so a separate actor rule would always pass, which is worse than an absent rule
      because a reader would count it. The two *are* separate in a server deployment, so the merge is the
      thing to look for when `P4-008`/`P4-009` add sharing.
      **The weight table is capped at a quarter of the total per signal**, which is "no single signal may
      dominate by accident" as arithmetic: the eight weights sum to 1000, none exceeds 250, and the test
      asserts the comparison directly — a memory good on several signals outranks one perfect on a single
      signal. Semantic similarity, active project/task relevance, and relationship overlap are
      **deliberately absent** from the table rather than present as zeros, because a zero row is the table
      claiming a signal it has no data to produce.
      **⭐ A test caught the explanation not explaining.** The first total scaled the whole weighted sum once
      while each contribution was scaled separately, and **integer division is not distributive** — so they
      disagreed by up to one per term (521 vs 522). The total now *is* the sum of the contributions, which
      makes an operator's displayed breakdown the arithmetic that produced the total rather than a parallel
      calculation. Integer rather than float for the same reason the weights are one table: a float score
      would make the ranking depend on platform rounding, so a stored score would be unreproducible.
      **⭐ The reason precedence was decided by an array literal.** `max_by_key` returns the **last** maximum
      on a tie, and a tie is the common case — a question naming a memory's entity usually also shares its
      words, scoring full on all three matching signals — so the reported reason depended on the order the
      arms were written. Now explicit: identifier, then overlap, then keyword.
      **A shared scaling helper, because the obvious integer form is always zero.** `numerator /
      denominator` truncates to nothing for every partial case, so a memory matching two of three words
      would score as if it matched none — and the same shape appears in four signals. One `scaled` helper
      multiplies before dividing, so eight call sites cannot each forget it.
      **Diversification is a prefix of the ranking, and a drop is not a refusal.** Caps applied in rank
      order; selecting by category first would override the ranking with a category order the scoring never
      expressed, and a category order imposed afterwards is a signal by another name. A capped and an
      excluded memory are **separate lists** because the actions differ — collapsing them would report "you
      have seen enough preferences" as "this preference is not available to you". A zero cap is refused
      because "none of this category" and "no limit" are opposite readings and the silent one is restrictive.
      **Falsified, one guard each:** disabling the destination-sensitivity rule failed
      `a_memory_above_the_destination_is_excluded`; raising a weight above the cap failed
      `no_single_signal_can_dominate` (on both the sum and the domination comparison). Both restored, green.
      **Recorded as limits:** no semantic similarity (that is `P4-005`), no project/task relevance, no
      relationship-graph traversal; the recency window is a constant rather than a per-query parameter; the
      **weights are reasoned and not calibrated** against a real result set (`P4-006`'s evaluation is where a
      measurement belongs, and the weights are one table so it is a single edit); **no function here issues a
      query**, so nothing yet turns a `MemoryQuery` into a candidate set; and nothing calls the module —
      `P4-007` is the integration and until then it is a complete, tested, uninvoked component.
      Gates: fmt, clippy `-D warnings`, 45 suites with the application binaries absent and **zero skips**
      (1135 tests), all three phase gates with `ACCEPTANCE_REQUIRE_BINARIES=1` and
      `ACCEPTANCE_REQUIRE_FIXTURE_PEER=1`, `cargo deny` ok. `jarvis-core` 196 tests.
<a id="p4-005"></a>

- [x] `P4-005` Research and implement a provider-neutral embedding adapter with dimension/version metadata.
      `docs/research/integrations/embeddings.md` + `crates/jarvis-models/src/vector.rs` (+`vector/tests.rs`) +
      `crates/jarvis-models/src/embedding.rs` (+`embedding/port_tests.rs`) +
      `crates/jarvis-models/src/openai/{embedding.rs,wire_embedding.rs}`. `ADR-0047`. 106 `jarvis-models`
      tests. **Research first, because this is an external integration**: the API index, the embeddings guide,
      and the create-embeddings reference were fetched live and recorded with the URL, the access date, the
      limits, and the unresolved questions before any code was written.
      **The central decision is that a vector is not comparable by itself.** `docs/architecture/storage.md`
      says "never compare vectors with incompatible metadata" and then lists the fields, which reads as a
      checklist — and a checklist fails, because the two fields easiest to forget are the two that produce a
      *plausible* wrong answer. A normalization mismatch does not fail, it ranks wrongly (a dot product over
      an unnormalized vector returns a number in the right range often enough to look like a score); a
      dimension mismatch fails as a length error far from the comparison, which reads as a corrupt vector.
      So `Embedding` holds the vector **and** its metadata, `Embedding::new` is the only constructor and
      refuses a vector whose length contradicts its declared dimension, and `ensure_comparable_with` is the
      guard — a method on the pair, returning `IncompatibleMetadata { field }` naming the first field that
      differed. The rejected alternative is the tempting one: `cosine_similarity(a: &[f32], b: &[f32])`,
      whose signature cannot see either field. This is the same defect class `ADR-0044` recorded as "two
      values that must agree with nothing holding both"; here something holds both and the constructor is the
      only way to make one.
      **`Normalization::Unknown` is the default, deliberately.** The researched provider documents
      length-1 vectors, but the port is what a *second* provider has to satisfy, so the dot-product shortcut
      is taken only when both sides declare `Normalized`. A default is what a caller who did not think about
      the field gets, and the permissive assumption on a distance is the one that goes wrong quietly.
      **Vectors are reassembled by the provider's `index`, never by position.** The wire defines `index` as
      "the index of the embedding in the list of embeddings"; zipping positionally would hold for an in-order
      response and silently pair every text with somebody else's vector if the provider ever reordered. An
      out-of-range index and a **duplicated** index are both refused — the duplicate was found while writing
      the ADR, which claimed the refusal the code did not yet perform; taking the later vector is the same
      defect class as positional pairing.
      **`input_hash` and `chunker_version` are stored but deliberately not compared.** The document's list
      is a set of properties a stored vector must carry, not a set of fields that must be equal for a
      comparison — two vectors of *different* text are exactly what a search compares, so requiring the hash
      to match would forbid the only meaningful comparison.
      **⭐ Three falsifications, all caught.** Disabling `ensure_comparable_with` failed
      `incompatible_metadata_is_refused_field_by_field`, `a_comparison_across_models_refuses`; disabling the
      length check in `Embedding::new` failed `a_vector_length_that_contradicts_its_metadata_is_refused`
      **and** `a_vector_of_the_wrong_length_is_refused` through the adapter path; disabling the
      duplicate-index refusal failed `a_duplicated_index_is_refused_rather_than_overwritten`. All restored,
      green.
      **A fourth thing the compiler caught:** the first scripted transport modelled a *streaming* response,
      which needed `Box<dyn ResponseBody>`, was not `Send`, and could not be constructed at all. The
      embedding call is never sent with `streaming: true`, so a test double that could produce one would
      suggest a reachable path that does not exist — the streaming half is gone and the double returns only a
      buffered response.
      **Recorded as limits:** **no provider has been called**, so the record stays `researched` and not
      `live-verified` — the provider's normalization claim, the per-model default dimensions, and the
      empty-string refusal are read from documentation, and the live smoke test in the record's verification
      plan is described rather than run; **nothing writes an embedding to storage** (there is no
      `memory_embeddings` table and no repository function — vector search and pgvector parity are `P4-009`);
      **no chunking**, so the 8192-token bound is the caller's problem; no token-count pre-check for the
      2048-element/300,000-token batch bounds either, because that needs a tokenizer this crate does not have
      and an estimate used as a *limit* would refuse valid requests while looking like a check; no retry
      inside the adapter (one attempt, as the Chat Completions record establishes, with the extra note that a
      retry costs tokens); `chunker_version` is `None` on the memory path where nothing chunks yet; `base64`
      is unsupported as an unmeasured decode path for provider-supplied data; and **nothing calls the port** —
      no daemon route, no memory path, no CLI verb — until `P4-006` consumes it.
      Gates: fmt, clippy `-D warnings` across the workspace, 45 suites with the application binaries absent
      and **zero skips** (1162 tests), all three phase gates with `ACCEPTANCE_REQUIRE_BINARIES=1` and
      `ACCEPTANCE_REQUIRE_FIXTURE_PEER=1`, `cargo deny` ok after `sha2` was added to `jarvis-models`.
<a id="p4-006"></a>

- [x] `P4-006` Add hybrid retrieval and explainable scoring; embeddings are an index, not canonical truth.
      `crates/jarvis-core/src/retrieval.rs` (+`retrieval/semantic_tests.rs`, 10 tests; `jarvis-core` 205).
      `ADR-0048`. The ninth signal, and the one the previous slice deliberately left out because it had no
      data. **Research first**, because `P4-005` was the external integration; this slice is the domain logic
      over it.
      **The vectors cannot cross the crate boundary, and the domain does not need them to.** `jarvis-models`
      depends on `jarvis-core`, so the domain naming `Embedding` would be a cycle. `jarvis-core` defines its
      own borrowed `SemanticVector` carrying provider, model, version, normalization, and a `&[f32]` — which
      is the whole contract — plus a `ScoringContext` that holds the query's vector and a lookup together.
      They are one optional value rather than two arguments because they must agree: separate arguments would
      make "a query vector with no index" and "an index with no query vector" both representable.
      **⭐ The mutation that was itself a no-op.** Zeroing the semantic signal to prove the ranking tests
      depended on it, the tests still passed — so a comment was written claiming the test had been vacuous
      and the coincidence was the tie-break. It was not: the mutation was written as
      `if false { 0 } else { real }`, which **selects the real branch**, so nothing had been changed and the
      green result carried no information. With the mutation written as a plain `0`, both ranking tests fail
      — `left: 0, right: 1000` on the signal and `Some(0)` vs `Some(1000)` on the total. The lesson is that a
      falsification which passes is first a claim about the mutation, not about the test, and the mutation has
      to be shown to compile into something different before a green run means anything.
      **⭐ The orthogonal pair found a real defect in a doc comment and a real defect in the code.** A test
      asserting an orthogonal pair scores half the scale failed with `left: 0`. The **test was wrong** — an
      orthogonal cosine is exactly `0.0`, not `0.5`, and the code was right. But the failure also exposed
      that `cosine <= 0.0` returned `absence: None`, which conflates an orthogonal pair with an opposed pair
      and with a genuine refusal; the four states that produce a zero score (no index, no memory vector,
      incomparable, zero magnitude) plus real arithmetic all collapsed into "zero, no reason" for two of
      them. `SemanticAbsence::NoSimilarity` now names the arithmetic case, which is the same principle
      `ADR-0046` applied when it made the total *be* the sum of the contributions.
      **The comparability guard is the point of the feature.** A cosine over two vectors from **different
      models** is a number with no meaning that still lands in the usual range, so a missing check does not
      look like a bug — it looks like a slightly worse ranking. Provider, model, and version are compared
      before any arithmetic and the differing field is named; the vectors' own lengths are compared too, which
      makes a declared dimension field unnecessary and would have repeated a check `P4-005`'s constructor
      already performs. A zero-magnitude vector refuses rather than dividing by zero, because a `NaN` in one
      signal would poison a whole ranking.
      **Nine weights that must still sum to `TOTAL_WEIGHT`, so the table was rebalanced and not extended.**
      Every ordering the document states is kept, and the cap that no weight exceeds a quarter of the total is
      unchanged, so `ADR-0046`'s "no single signal may dominate by accident" comparison still holds. The
      *reason* precedence is deliberately **not** the weight order: the document lists semantic similarity
      above entity overlap and the weights keep that, but a reason is what a user is told, and "this memory
      means something close to what you asked" is an inference where "this memory repeats your words" is
      present in the text — so text wins the reason and meaning keeps the weight.
      **A memory without an embedding is not a worse memory.** It scores zero on one of nine signals and is
      ranked on the other eight, which is the document's "embeddings are one signal" as arithmetic rather
      than as policy — and no index is a zero **with a recorded reason**, so a caller can tell a zero from an
      unconfigured provider, from a memory the index never saw, from an incomparable pair, and from two
      genuinely orthogonal directions.
      **Falsified, one guard each:** disabling the provider/model/version comparison and the length check
      failed `a_vector_from_another_model_is_refused_and_names_the_field` and
      `vectors_of_different_lengths_are_refused`; disabling the zero-magnitude guard failed
      `a_zero_magnitude_vector_refuses_a_comparison`; zeroing the signal in `signals_for` failed both ranking
      tests. All restored, green.
      **Recorded as limits:** **nothing produces the vectors** — the `P4-005` port is wired to nothing, no
      route embeds a memory, no job populates a lookup, and nothing writes a vector to storage, so what is
      proven is the scoring and the guards rather than an end-to-end retrieval with embeddings; **nothing
      calls `rank`** either (`P4-007` is the integration); the **weights are reasoned, not calibrated**, and
      this slice makes that limit slightly larger by moving eight values rather than only adding one; the
      lookup is a closure, so the module cannot say how a vector is *found* (`P4-009` owns pgvector);
      the cosine is `f32`, so cross-platform bit-identity is not claimed even though the stored score is an
      integer; relationship overlap and active project/task relevance remain absent as in `P4-004`; and a
      text-free query is untested because no caller constructs one yet.
      Gates: fmt, clippy `-D warnings`, 45 suites with the application binaries absent and **zero skips**
      (1172 tests), all three phase gates with `ACCEPTANCE_REQUIRE_BINARIES=1` and
      `ACCEPTANCE_REQUIRE_FIXTURE_PEER=1`, `cargo deny` ok. `jarvis-core` 205 tests.
<a id="p4-007"></a>

- [x] `P4-007` Integrate memory retrieval into context budgets with provenance and injection-resistant quoting.
      `crates/jarvis-core/src/isolation.rs` (+`isolation/tests.rs`, 10 tests) +
      `crates/jarvis-core/src/memory/context_tests.rs` (11 tests) +
      `MemoryRecord::{context_trust, context_item}` + `RetrievedMemory` + `memory_context_introduction` +
      `jarvis-storage::read_retrievable_memories` + `apps/jarvisd`'s `load_memories`/`context_summary` and
      the `messages_from_manifest` memory path (4 daemon tests). `ADR-0049`.
      **`jarvis-core` 227 tests** (up from 205), **1198 workspace tests** in 45 suites.
      **⭐ Isolation is not detection, and the module says so.** "Detect and remove instructions" is not
      implementable — deciding whether a sentence is an instruction requires understanding it, and any pattern
      list is defeated by rephrasing or by a language it does not cover. So the module makes the text have
      three properties an attacker cannot undo instead: **format characters are removed** (a bidi override
      changes how text *reads* without changing what it *is*, so in a prompt it is a deception primitive); the
      **fence token cannot appear in the payload**, so the region cannot be closed from inside; and the
      **framing is authoritative text this platform wrote**, placed before the payload. Property three is why
      the fence is not itself a security claim — properties one and two hold regardless of what the model
      decides to do.
      **⭐ The model may still obey an instruction inside the fence, and that is recorded as the design.**
      Prompt injection is *mitigated* here, not solved, and no fencing scheme solves it. What protects the
      system is that the *effect* of any instruction still passes through schema validation, authorization,
      risk classification, approval, and audit: the model may ask, and deterministic Rust decides. Stated in
      the ADR so the fence is not read as a guarantee it is not.
      **⭐ Two real defects the tests found, and the second is the more interesting.**
      (1) The fence-token check **silently did nothing for every payload, including the real marker**: it
      stripped separators from the text and then searched for a token that still had its own hyphens in it, so
      the comparison could never match. An assertion written in terms of the same normalisation had reproduced
      the bug and passed — what caught it was counting opening markers in the *rendering*. A guard that cannot
      fire reads exactly like a guard that finds nothing.
      (2) **Neutralisation removed every control character, and `\n` and `\t` are control characters** — so a
      multi-line memory silently became one run-on line, and a list rendered as a sentence is a different
      claim. Line structure is kept; a carriage return is not, since a lone one is a line-overwrite primitive.
      The failure mode was a *quiet reshaping* rather than a loss, which is why the test asserts the exact
      text.
      **⭐ A third defect: the token estimate was two bytes short of what would be sent.** It added the two
      marker lengths and forgot the newlines `render` inserts. That is the one direction a budget must not err
      in. The estimate now takes the rendering itself, so there is no length arithmetic left to get wrong, and
      the test asserts an inequality over four payload shapes rather than an equality against one hand-computed
      length.
      **Eligibility is split between the read and the conversion, deliberately.** SQL excludes what only SQL
      can decide (deleted/proposed/superseded rows, **model inferences**, task-shaped predicates); conversion
      excludes what needs the clock or a policy (not current truth at this instant, a type the use case does
      not allow). Two rules deserve their reasons recorded: a **model inference is filtered by source kind,
      not by status**, because a *confirmed* inference is `active` and is still the model's own claim — the
      self-feeding loop the inference boundary exists to prevent; and **`Working`/`Conversation` are excluded
      by the allow-list**, the first because a plan replayed into a prompt reads as an instruction to continue
      it, the second because the history replay already provides it and offering it twice would make one turn
      look like independent corroboration of itself.
      **The trust mapping is deliberately not the identity.** A model inference is `Untrusted`, never
      `Derived`, or the model could reach a future prompt by first writing a memory. An unconfirmed claim from
      an authoritative source is `Derived`, not `User`, because a user statement recorded `Uncertain` is this
      platform's uncertain reading of something rather than the person speaking. Nothing maps to
      `Authoritative`, because a memory is never JARVIS's own policy — and only `Authoritative` is
      instruction-bearing.
      **Retrieved records go in one message, after the history and before the question.** One message per
      record would interleave untrusted text with the conversation's turns, so a record could be read as a
      turn; appending them to the user's own turn would make them indistinguishable from something the user
      typed. Membership comes from the manifest and order from the conversation, and a record the manifest
      included but which cannot be found is an **error** rather than a skip, or the request would disagree
      with its own audit record.
      **Falsified, one guard each:** disabling the newline exemption failed
      `line_structure_survives_but_a_carriage_return_does_not`; restoring the un-normalised needle failed two
      isolation tests; sending `isolated().body()` instead of `.render()` failed three of the four daemon
      memory tests; neutralising the `source_kind` filter failed `a_model_inference_and_a_task_claim_are_not_offered`.
      All restored, green.
      **A test weakness the falsification exposed and fixed:** `a_stored_memory_reaches_the_request_inside_a_fence`
      *passed* with the fence removed, because the introduction itself names both markers, so "the message
      contains the marker" was satisfied by prose about the fence. Extracted a shared `fenced_record_message`
      helper that requires the marker **on its own line**, which is the structural property `render` produces
      and prose cannot satisfy.
      **Recorded as limits:** no ranking, scoring, or diversity is applied — `P4-004`'s eligibility and
      `P4-006`'s ranking exist and are tested and **nothing calls them on this path**, so the read is a recency
      window rather than a relevance one and `MAX_MEMORIES_LOADED` is the only bound; `retrieval_count` is
      **never incremented** and `last_accessed_at` stays null (`reinforce_memory` exists, nothing calls it), so
      the reinforcement signal is zero for every memory; **no vector search**, because nothing writes an
      embedding, so the hybrid ranking is not yet hybrid; `read_entity_memories` exists and is unused, so
      "what do I know about this person" is still answered by ranking rather than an index seek; **expiry is
      evaluated in Rust**, so expired-but-active rows still fill the window (`ADR-0034`'s timestamp comparison
      defect would have to be fixed first); the format-character range set is **duplicated** between
      `jarvis-core` and `jarvis-mcp` because the dependency runs one way; the event says how many memories were
      offered/included/altered but **not which**, beyond the manifest's references not being stored; and no
      CLI or route exposes any of it (`P4-008`).
      Gates: fmt, clippy `-D warnings`, 45 suites with the application binaries absent and **zero skips**
      (1198 tests), all three phase gates with `ACCEPTANCE_REQUIRE_BINARIES=1` and
      `ACCEPTANCE_REQUIRE_FIXTURE_PEER=1`, `cargo deny` ok. `jarvis-core` 227 tests, `jarvisd` 93.
<a id="p4-008"></a>

- [x] `P4-008` Add inspect, search, remember, correct, forget, export, retention, and full user-deletion APIs/CLI.
      `crates/jarvis-protocol/src/memory_api.rs` (the wire DTOs) + `apps/jarvisd/src/memory_service.rs`
      (8 tests) + six routes in `apps/jarvisd/src/gateway.rs` (3 tests) + `apps/jarvis-cli/src/memory.rs`
      (seven verbs) and `api_client.rs`'s seven memory methods. `jarvis-storage`'s `purge_memory`,
      `read_all_memories`, `find_memory_including_deleted`, `count_memory_entity_links`, and
      `write_tombstone_on`. `ADR-0050`.
      **`jarvisd` 105 tests** (up from 93). **45 suites, 1210 workspace tests, 0 failed, 0 ignored.**
      **⭐ This slice was mostly wiring, and the wiring is what found the defects.** The domain, storage,
      pipeline, retrieval, and embedding port all existed and were tested, and **nothing could reach any of
      them**. Every defect below was found by *connecting* a tested component to a caller — which is the case
      for these tests existing at all.
      **⭐ A guard that was unreachable because its input was never supplied.** The deduplication check lives in
      the pipeline's `compare(context.existing, …)`, and `remember` passed `existing: None` — so the entire
      duplicate path was dead code and a re-statement fell through to the unique index and surfaced as
      `MemoryDuplicate` → `409` "the memory was changed by another writer". **A user restating their own claim
      was told another writer had edited it.** The gateway test asserting `200` for a repeated claim found it.
      The mapping had even carried a comment saying the variant "is reachable only as a race": a comment
      explaining why an error is unlikely is how a bug gets a justification instead of a fix.
      **⭐ The pipeline's outcome and the fact of a write are different answers.** Fixing the above exposed that
      the reply was built from the admission alone, so a caught duplicate reported `outcome: "remembered"` —
      right shape, wrong claim. `Stored { memory, wrote }` now travels out of the store, and `outcome_of`
      reconciles the two: nothing stored means nothing was remembered, whatever the pipeline decided. Pairing
      them makes the divergence unrepresentable rather than merely documented.
      **⭐ Three reads silently skipped a join, and the refusal it caused looked like the caller's fault.**
      `find_memory_including_deleted`, `read_all_memories`, and `read_memories` all decoded the memory row
      without loading `memory_entities`. A correction inheriting `existing.record().entities()` inherited
      **nothing** and was refused with *"a memory must name between 1 and 16 distinct entities"* — a message
      about the caller's request for a defect in a read. A missing join is invisible: the row decodes, every
      field is present and typed, and the empty vec looks like a legitimate "about nothing". Only `find_memory`
      had loaded the links.
      **⭐ SQLite in WAL fails a deferred transaction that reads then writes across connections.** The purge
      read its row inside a transaction, wrote the tombstone through the **pool** (a second connection), then
      tried to `DELETE` on the transaction — `SQLITE_BUSY_SNAPSHOT`, reported as a bare "failed to purge a
      memory the SQLite database". `write_tombstone_on` takes the `Transaction` rather than the database, so
      splitting the operation across connections is now a **compile** error.
      **⭐ An operation's own doc described a path the code does not take.** The export's exclusion text said
      "deleted claims appear as an empty record", which describes the `Delete` **transition** (clear text, keep
      the row). The only deletion verb this surface has **purges**. The doc and the text agreed with each other
      while both described something no verb does — an assertion that the purged id is *absent* from the export
      found it, and agreement between two statements by the same author is not evidence.
      **⭐ A correction cannot inherit the original's window, and the domain is right to refuse it.** A memory's
      `valid_from` may not precede its `created_at`: a window that opens before the record existed is a
      backdated claim, and it is how a correction would fail to outrank the thing it corrects. So a correction
      takes `valid_from` from its own creation instant and carries `valid_until` over unchanged. The limit is
      recorded rather than papered over: correcting a claim that had **not yet taken effect** makes the
      correction effective from now, so the pair reads as overlapping; and a claim whose window has **already
      closed** cannot be corrected at all (the correction would be born expired), refused with `lapsed` rather
      than stored.
      **⭐ A tombstone was reported as an infrastructure failure.** `DatabaseError::MemoryTombstoned` fell
      through to `Storage` in the error mapping, so a client that restated a deleted claim was told *"the local
      database is not available"* — sending an operator to check a daemon that is running.
      **⭐ Deleting is two operations, not one with a flag.** `Delete` clears the text and keeps the row, because
      a source link has to keep resolving and the audit trail has to survive; `purge_memory` deletes the row and
      `memory_entities` cascades. Collapsing them would make "delete" mean whichever the caller intended, and
      the one it did not mean would be unrequestable.
      **The tombstone is written *before* the delete**, so a crash between the two leaves a tombstone for a
      memory that still exists (resolvable by deleting again) rather than a deleted memory with no tombstone
      (resurrected by the next ingest). The safe failure direction is the one that **over-blocks**.
      **`allow_relearn` is an explicit undo**, not a skipped write: it removes any existing tombstone *and*
      skips writing a new one, because a flag that merely suppressed the write would leave an older tombstone
      in place and the undo would appear to work and then not.
      **`expected_version` is required on correct and forget, and that is the opposite of `ADR-0022`.** A run
      cancellation carries none, because a run's terminal-state rule already refuses a second one; correct and
      forget change **what will be retrieved as current truth**, so the write's subject matters. The version is
      on the **reference**, not only on a reply, because the caller obtains an expectation by reading and a
      reply that omitted it would leave a client re-reading and hoping. Its absence from the CLI is a usage
      error rather than an implicit re-read. **Two staleness reasons exist and both are tested**: a version the
      caller never saw, and a version read *before a correction archived the claim* — which advanced that
      claim's own version.
      **A search hit reports version `0`, and a purge guard refuses it.** The ranking holds a `MemoryRecord`,
      which carries no concurrency version, so `hit_of` can only fabricate a value; `0` cannot match a stored
      version (they start at one), so it is safe as "I do not know" instead of dangerous as a plausible guess.
      Recorded limit: a search result cannot be corrected without a `show` first.
      **No scope in a request body**, and `deny_unknown_fields` is what makes it structural: a `workspace_id`
      in a body is a `422` naming the field, so "client A's memory cannot enter client B's context" is a
      transport property rather than a per-handler check. The read-side check is a comparison **after** the
      read, and a claim in another workspace is `404`, never `403` — "not yours" would confirm that something
      exists.
      **A reference never carries content**; `show` and `export` are the two exceptions for different reasons
      (one claim deliberately requested vs. `GDPR`-shaped portability). The export lists its **exclusions**,
      because each entry is a different reason a reader might assume completeness.
      **An empty entity list is refused rather than filled with a placeholder** — a claim attached to a
      placeholder *looks* resolved, so a later question about the real subject will not find it and nothing in
      the store says why.
      **Falsified, one guard each:** setting the tombstone write to `if false` failed
      `a_forgotten_claim_does_not_return_unless_relearning_was_allowed` (the claim came back with a fresh id);
      replacing `load_scoped`'s workspace comparison with `false &&` failed
      `a_claim_is_only_readable_under_the_workspace_that_holds_it`. Both restored, green.
      **Recorded as limits:** **no entity resolution**, so a remember must name an entity identifier the caller
      already holds; `importance` defaults to `2`, the middle of the range, so a default cannot outrank explicit
      user statements; **no retention policy is implemented** — the verbs exist and the sweeper does not, so
      nothing expires on its own; **no provider-side deletion** is possible and the receipt says so; the CLI
      verb module has no test, so its argument parsing is unverified (the daemon service and the routes are
      tested); and an entity-less remember is refused by the service rather than resolved.
      Gates: fmt, clippy `-D warnings` over the workspace, 45 suites with the application binaries present and
      `ACCEPTANCE_REQUIRE_FIXTURE_PEER=1` (**1210 tests, 0 failed, 0 ignored**), all three phase gates with
      `ACCEPTANCE_REQUIRE_BINARIES=1`, `cargo deny` ok.
<a id="p4-009"></a>

- [x] `P4-009` Implement PostgreSQL plus pgvector backend parity for the completed memory behavior.
      **DONE. The backend exists, was measured against a live server, and its limits are recorded.**
      `crates/jarvis-storage/migrations/postgres/0001_memory_embeddings.sql` +
      `crates/jarvis-storage/src/embedding_repository.rs` (+`embedding_tests.rs`, 9 tests) +
      `crates/jarvis-storage/tests/postgres_embeddings.rs` (14 live tests) + the SQLx `postgres` feature +
      `docs/research/integrations/postgres-pgvector.md`'s "Live-Server Findings". `ADR-0053` (the backend) and
      `ADR-0051` (the text codec, which this slice reuses unchanged).
      **`jarvis-storage` 191 tests** (up from 182), **47 suites, 1252 workspace tests, 0 failed, 0 ignored.**
      **The codec's own findings, from `ADR-0051` and unchanged here.** SQLx 0.9.0 has **no `vector` type
      mapping** — verified against the driver's own type table — so the value crosses as a text literal and
      `pgvector.rs` is the code that has to be right. The round trip is asserted **by bit pattern** (`-0.0 ==
      0.0` is true, so a value comparison cannot see a sign lost in the text form) **and** against the
      documented form literally (`encode(&[1.0, 2.0, 3.0]) == "[1,2,3]"`), because a round trip alone cannot
      tell a correct form from a self-consistent wrong one. **Falsified:** with the dimension cap removed
      `encode` accepted 2,001 dimensions; with the **encode-side** finite check removed
      `encode(&[1.0, f32::NAN])` returned `Ok("[1,NaN]")`, because `NaN` **formats successfully** and
      `"NaN".parse::<f32>()` **succeeds** — so a read-only guard would let this platform **write a row it
      cannot read**. **A guard on one side of a codec is not a guard on the pair.** The codec takes `&[f32]`
      rather than `EmbeddingVector` because `repository-layout.md` forbids an adapter-to-adapter edge and
      `jarvis-storage` and `jarvis-models` are both adapters.
      **⭐ This slice closed because the ENVIRONMENT changed, and the change is worth recording.** `P4-009`
      was partial because Docker Desktop was installed but **its daemon was not running** and no `psql` or
      Postgres service existed — so the live-server tests could not run and the slice honestly stayed open.
      Starting the daemon (`Docker Desktop.exe`, ~45 s) made `pgvector/pgvector:pg17` available:
      **PostgreSQL 17.11, vector 0.8.6** — the exact version the research record had selected. **A blocked
      slice can be blocked by a daemon, not by a decision**, and the fix was to try the thing that was
      assumed unavailable.
      **⭐ The README does not mention the finding the schema depends on.** A cast-expression HNSW index
      **without a predicate** over an unconstrained `vector` column creates successfully and then **refuses
      every insert at any other dimension** (`expected 1536 dimensions, not 3`). Measured: a table of
      1536-dimension rows became **entirely uninsertable**. Adding `WHERE vector_dims(embedding) = 1536`
      fixes it, because a partial index only covers rows matching its predicate: **3-, 4-, 1536-, 2000- and
      16001-dimension vectors then all stored in one column**, and the query still produced
      `Index Scan using ...` — the plan a fixed `vector(1536)` column gets. So the predicate is load-bearing
      in the index **and** in the query, and the control test shows what it earns: the same query without the
      predicate does not name the index at all.
      **⭐ Three more measured facts the documentation does not state plainly.** (1) `vector(n) <=> other(m)`
      across widths is an **error** (`different vector dimensions 1536 and 3`), not an empty result, so the
      cast in the query is not an optimisation. (2) All three distance operators return **`double
      precision`**, not `real` — binding the distance as `f32` **rounds rather than failing**, so
      `SimilarMemory::distance` is `f64` and the one narrowing happens at the retrieval boundary. (3) A row
      declaring `dimensions = 1536` while holding a 3-component vector **inserted cleanly**, and adding the
      constraint afterwards failed *on that row*; `vector_dims` is `IMMUTABLE` (verified in `pg_proc`), which
      is what makes it legal in the CHECK and in the index predicate.
      **⭐ A defect this slice found in ITSELF, via a live test.** The first migration bounded `dimensions` to
      **16,000** (pgvector's *storage* cap) while `pgvector::encode` refuses above **2,000** (the *index*
      cap) — so the schema allowed 2,001..16,000, a range **no code path can produce**, and a direct insert
      there would create a row the partial index cannot cover. The live test that tried to store a
      2,001-dimension vector is what surfaced it. Both bounds are now 2,000, and an offline test asserts the
      migration does not contain the storage-cap clause. **A bound nothing can reach reads as capability
      while being an unverified claim — and the two caps belong to different things, so "the server accepts
      it" was never the question.**
      **⭐ The live tests skip without a server and can be made to fail.** `ACCEPTANCE_POSTGRES_URL` supplies
      the connection; **`ACCEPTANCE_REQUIRE_POSTGRES=1`** turns the absence into a failure naming the
      `docker run` that fixes it — the same shape as `ACCEPTANCE_REQUIRE_FIXTURE_PEER` and
      `ACCEPTANCE_REQUIRE_BINARIES`. **Both directions falsified:** no server and no variable → 14 tests pass
      by skipping; no server and the variable set → the run fails with an actionable message. Each live test
      gets **its own schema** (cargo runs integration tests in parallel) and **keeps `public` on the search
      path** — measured: a path holding only the scratch schema made the migration fail with
      `type "vector" does not exist` while the extension was installed in `public`, because an extension
      lives in a schema.
      **⭐ The migration is verified by executing it, not by reading it.** The live test applies the embedded
      file and then reads `pg_indexes` for the index definition, asserting `USING hnsw`, `vector_cosine_ops`
      and the `WHERE (vector_dims(embedding) = 1536)` clause. **What the server built is the evidence, not the
      text that asked for it.**
      **Metadata guard, asserted with a positive control:** four rows identical as vectors and differing in
      exactly one metadata field each (model, normalization, workspace, dimension). The comparable row must be
      found, or the refusals prove nothing.
      **⭐ The SQLx `postgres` feature cost, measured before enabling.** `sqlx-postgres` was **already in the
      lock file**; the feature adds `hmac 0.13`, `md-5 0.11`, `stringprep 0.1.5`, `whoami 2.1.3` and a
      **second `sha2` (0.11.0)** beside the pinned 0.10.9 — `cargo tree --invert sha2@0.11.0` names
      `sqlx-postgres` as the only consumer. `cargo deny` → advisories, bans, licenses, sources **all ok**; the
      duplicate is policy `warn`, as the pre-existing eight are. `tls-*` stays **off** deliberately: a
      networked TLS server mode is `requirements.md`'s multi-device story and no record covers its
      authentication, so a plaintext **loopback** connection is the only one this slice can justify.
      **Every `sqlx` statement that must interpolate is `AssertSqlSafe`-wrapped, and the compiler refuses it
      otherwise.** An operator is a token and `vector(1536)` is a *type* in a cast, so neither can be a
      placeholder; `sqlx` 0.9 rejects a `format!`-built statement with "dynamic SQL strings should be audited"
      and `AssertSqlSafe` is the assertion that gets past it. The audit: `operator()` returns one of three
      fixed tokens from an enum match and `dimensions` is an `i64` from a shared helper, so neither can carry
      a quote or a statement separator, and every caller-supplied value is bound.
      **Recorded as limits:** the extension is **not trusted** (`pg_available_extension_versions.trusted =
      false`), so `CREATE EXTENSION` needs a superuser and an incapable role fails there — with the downstream
      symptom `type "vector" does not exist`, which does not say an extension is missing, and no pre-flight
      check gives a better message. **No server-mode composition**: nothing in `apps/jarvisd` opens a
      `PgPool`, so this repository has **no production caller**. No migration of existing SQLite data, no
      `memories`/`entities` tables in PostgreSQL, and **no foreign key** from `memory_embeddings.memory_id`
      (a `REFERENCES` naming a table nobody creates would fail for a reason unrelated to embeddings).
      **`hnsw.ef_search` is left at its default (40)** and measured as a real recall bound: a 10%-selective
      filter over 400 rows returned at most 40. **One index per width is hand-managed** — `index_ddl` makes
      the second correct-by-construction, but nothing calls it when a new model appears and nothing detects a
      width whose rows are unindexed. **`Normalization` is not recomputed**, only stored and required to
      match. **No `halfvec`, binary quantization, subvector indexing or dimensionality reduction**, so a
      model returning more than 2,000 dimensions cannot be used at all — the refusal names all four.
      Gates: fmt, clippy `-D warnings` over the workspace `--all-targets --all-features --locked`, 47 suites
      with the application binaries present and `ACCEPTANCE_REQUIRE_FIXTURE_PEER=1`, the Postgres suite with
      `ACCEPTANCE_POSTGRES_URL` and `ACCEPTANCE_REQUIRE_POSTGRES=1`, all four phase gates with
      `ACCEPTANCE_REQUIRE_BINARIES=1`, and `cargo deny check` ok.
<a id="p4-010"></a>

- [x] `P4-010` Pass isolation, correction, deletion, stale-memory, and adversarial-source acceptance tests.
      `tests/e2e/tests/phase4_gate.rs` (2 process-level cases) + `crates/jarvis-storage/src/workspace_repository.rs`
      (+`workspace_tests.rs`, 5 tests) + `jarvis_storage::record_workspace` + the entity guards in
      `apps/jarvisd/src/memory_service.rs` (2 daemon tests). `ADR-0052`.
      **`jarvis-storage` 182 tests** (up from 177), **`jarvisd` 107** (up from 105).
      **46 suites, 1229 workspace tests, 0 failed, 0 ignored.**
      **⭐ The gate found three defects, and the first is security-relevant.** All three were **invisible to the
      library suites**, which were green throughout:
      **(1) `remember` invented any entity the caller named.** `resolve_entities` only *parsed* identifiers, and
      the link step then called `record_entity` — so `POST /api/v1/memories` naming a subject that did not exist
      **created that subject** and answered `201`. Two consequences: a claim about a fabricated entity is
      **indistinguishable from a claim about a real person**, so a typo produced a memory nobody could find or
      correct; and the **identity vocabulary became caller-controlled**, which
      `identity-and-workspaces.md` forbids (an entity must be established through resolution — verified provider
      IDs, exact identifiers, user confirmation, or a probabilistic match recorded as such). `ADR-0050` had
      refused the placeholder *subject* while the placeholder *mechanism* stayed one layer down.
      **(2) A claim could name another workspace's entity.** Nothing compared the entity's workspace against the
      caller's, so a caller could **write** a claim into its own workspace pointing at another workspace's
      subject — a cross-workspace link dressed as a local claim.
      **(3) Nothing could create a second workspace**, which `A09` requires; every profile has one, seeded by
      migration `0005`. So this slice added `jarvis_storage::record_workspace` — and it is **not** a multi-tenant
      feature: no session, no credential, no route, and no actor can name a workspace.
      **The fix is a read, not a check.** `resolve_entities` now reads the entity and refuses it unless it
      exists, belongs to **this** workspace, and is **usable** (not merged, not deleted — a merged entity's
      claims belong to the winner and a deleted one's to nobody, so a new claim against either attaches itself
      to a name that no longer denotes anything). `is_usable` is the domain's own predicate rather than a
      comparison restated at the edge. `apps/jarvisd` no longer calls `record_entity` at all: the
      `memory_entities` rows are written by `record_memory` in the same call as the memory row, so a claim
      cannot exist without the subjects it named and there is no second place an entity can appear.
      **⭐ Every refusal is paired with an independent second fact**, because a status-code-only assertion can
      pass for the wrong reason: the unknown-entity `422` is paired with a store read proving the entity was
      **not created**; the cross-workspace entity `422` with a search proving no claim **matched**; and the
      cross-workspace read `404` with a **positive** read of this workspace's own claim, so "refuses everything"
      cannot pass.
      **⭐ The gate's first run failed on a real wire-shape change, which is what it is for.**
      `superseded["reference"]["status"]` was `null` — `MemoryDetailReply` **flattens** its reference, so the
      fields are top-level. A client written against the nested shape would have broken in production, and the
      gate reads raw JSON rather than the product's DTOs precisely so a field rename or re-nesting is caught.
      **⭐ Two assertions the gate got wrong, and both were corrected rather than relaxed.** (a) A search for the
      refused wording returned the *stored* claim for **recency** — `is_a_match` is the field that says whether
      something answered the query, and `ADR-0046` deliberately includes recent claims that matched nothing, so
      the assertion now filters on `is_a_match` rather than asserting an empty result. (b) The duplicate check
      would have masked the entity bug if the refused wording had matched a stored claim, so the negative cases
      use **distinct words**: with identical text the claim is refused whatever the entity check does.
      **Falsified, one guard each:** with the entity-workspace comparison disabled the gate stored a claim naming
      another workspace's entity and answered `201`; with the memory scope comparison disabled a read of another
      workspace's claim answered `200` with its content. Both restored, green.
      **Recorded as limits.** **No entity-creation surface exists**, so a remember is still unreachable by a user
      of the shipped product — the gate reaches `record_entity` and then asserts the refusal, so the gap is
      measured rather than hidden. No multi-tenant surface: `record_workspace` has no route and its doc says so.
      No `memory_embeddings` table. `A08` does not exercise the model's context (that needs a configured
      executor model and a provider; it is covered by the daemon's own tests), and `A09` does not cover "context
      build", a model call, a tool, a trace, or diagnostics — those surfaces either take no workspace or do not
      exist.
      Gates: fmt, clippy `-D warnings` over the workspace, 46 suites with the binaries present and
      `ACCEPTANCE_REQUIRE_FIXTURE_PEER=1`, all four phase gates with `ACCEPTANCE_REQUIRE_BINARIES=1`, and
      `cargo deny check` ok (advisories, bans, licenses, sources — eight `duplicate` warnings, all pre-existing
      and none an error).
<a id="p4-011"></a>

- [x] `P4-011` Define the skill format and its lifecycle. **`ADR-0117` was recorded first** (accepted): a
      skill is a procedure naming already-granted tools, loading authorizes nothing, a self-authored skill is
      `Derived` context fenced by `ADR-0049`, and promotion is a durable approval that names its approver
      (`ADR-0043`).
      **Recorded as its own slice because the gap is an unmade trust decision, not a missing feature.**
      `requirements.md` lists `procedural` among the memory types and `ROADMAP.md` names "event-triggered
      skills", and **nothing connected them** — no shape, no creation step, no statement of what a skill *is*.
      The reason that matters: "skill" is the name two other designs in this class give to **text that is
      loaded and then acted on**, injected beside the tool list with its effects gated by a pattern list or a
      coarse approval. That step is incompatible with `AGENTS.md`'s boundary — *the model may request an
      effect; deterministic Rust policy decides whether it may happen* — and a skill is a new route by which
      an instruction reaches the model.
      **Delivered:** `jarvis-core` gains `skill` (`SkillRevision`, `SkillStep`, `SkillState`, `DropReason`,
      `SkillDroppedField`, `InvalidSkill`, the `MAX_SKILL_*` bounds) and `SkillId`; a migration
      (`0010_skill_revisions.sql`) makes three of `ADR-0117`'s rules properties of a row; and
      `jarvis-storage` gains `skill_repository` (record, find, three scoped reads, promote, archive, restore,
      supersede). `CURRENT_SCHEMA_VERSION` moves 9 → 10.
      **⭐⭐ AUTHORITY IS UNREPRESENTABLE, WHICH IS THE POINT OF THE WHOLE SLICE.** There is deliberately **no
      field** for a grant, a scope, a pre-approved effect, or a chosen approver — no constructor accepts
      authority and there is nowhere one could arrive. That is the `P3-006c` shape: the rule is enforced by
      what the type can *express* rather than by a check every reader must remember. A skill is an `SkillId`,
      a source, a version, a step list, and prose.
      **⭐⭐ THE PROMOTION RULE HAS THREE ENFORCERS, AND THEY ARE NOT REDUNDANT.** `ADR-0117` §4 makes
      promotion an approval. So (a) `SkillRevision::new` refuses a **model-authored** revision recorded
      `Active`; (b) `promote_skill_revision` writes under `WHERE state = 'proposed'`, so a second promotion
      affects zero rows and is *reported* rather than silently overwriting the approver a first decision named
      (`ADR-0043`); and (c) the **schema** requires that an active model-authored row names its approver —
      `CHECK (state <> 'active' OR source_kind <> 'model_inference' OR promoted_by_actor_id IS NOT NULL)`.
      **(c) is the only layer that can state it.** A promoted revision legitimately decodes as `active`, so a
      decode must bypass the construction check — which is why `SkillRevision::from_stored` exists — and the
      gap that opens is closed in SQL, where a row that reached that state some other way is refused at write.
      A test writes SQL directly to reach it, because a repository-mediated write cannot produce the row.
      **⭐⭐ A FIXTURE PUT ITS DATABASE IN THE WRONG DIRECTORY AND EVERY TEST TOOK FIVE MINUTES.** The first
      version wrote each test's database **directly into the system temp directory**, and one test took
      **306 seconds**. The cause is not the code under test: `SqliteDatabase::open` calls
      `prepare_private_directory` on the database's *parent*, so the fixture was re-securing `%TEMP%` — a
      shared directory with many entries and a broad ACL — on every open. Moving the file into a private
      subdirectory its own `Drop` removes brought sixteen tests from 763 s to 2.6 s. **It looked like a hang
      rather than a slowdown, and the cost was in where the fixture put its file.** The approval fixture
      already used a subdirectory, which is why it was fast; the new fixture did not copy that.
      **⭐ A DECODE RE-APPLIES THE RULES, AND THE TOOL-IDENTIFIER RULE ARRIVES AS A PARAMETER.** A step names a
      tool, and the rule for a valid identifier lives in `jarvis-tools`, which `jarvis-storage` may not depend
      on (`repository-layout.md`: an adapter may depend on core and **not on another adapter**). So every
      function that builds a `SkillStep` takes a `validate_tool` function — the rule keeps **one** home in the
      crate that owns it rather than being restated as a second pattern here. It also means a decode
      **re-applies** the rule: a row written by another build, restored from a backup, or hand-edited is
      refused on read exactly as on write, asserted with the real validator so the failure is the rule rather
      than a parse error. The **provenance equality** is re-applied the same way, and restated in SQL, because
      a model inference that could claim authoritative trust would be a self-authored procedure read as the
      user's own instruction.
      **⭐ A DROPPED EXTERNAL FIELD IS RECORDED WITH A REASON.** `ADR-0117` §7 requires any field that would
      grant authority, preselect an ungranted tool, or pre-approve an effect to be dropped **and the drop
      recorded**, because a field accepted-then-ignored is worse than one never accepted — a reader of the
      stored skill cannot otherwise tell the format's intent from this platform's behaviour. `DropReason`
      carries the four authority-bearing categories plus `Unrepresented`, and `is_authority_bearing()`
      partitions them so a reviewer can see how much of a document was refused.
      **⭐ REPLACEMENT IS DECLARED, SO BOTH LEGS ARE THE CALLER'S.** `ADR-0117` §5 and `ADR-0045`: nothing
      infers a supersession from comparing prose. The successor names what it replaced at insert time and
      `supersede_skill_revision` writes the forward pointer — and it **checks the successor exists first**,
      because a dangling `superseded_by` would make the chain unwalkable and the failure would only surface
      when something tried to follow it.
      **⭐ A STEP'S VERSION IS EVIDENCE, NOT A GRANT.** `SkillStep::tool_version` records the version a
      revision was authored against, so "which version of the tool did this procedure address" is answerable
      from the record (`ADR-0117` §5) rather than reconstructed from today's registry. Whether the actor's
      *current* grant covers a step is decided when the step runs, never at load — a grant revoked between the
      two must refuse the step, and only an execution-time check can see that.
      **⭐ `procedural` MEMORIES AND SKILLS ARE SEPARATE TABLES, RECORDED AS A DEPARTURE.** A skill *could*
      have been a `memories` row with a JSON body, since `procedural` is a valid `memory_type`. It is not,
      because a skill is not a claim: a claim has confidence, decay, retrieval ranking, and a search key,
      while a procedure has versioned steps, a promotion, and a supersession chain. Putting one in `memories`
      would make every retrieval query carry a type predicate to skip the non-claims and would let a skill
      silently acquire a claim's semantics — and collapsing two different revision bodies onto one
      `search_key` would refuse the second as a duplicate.
      Gates: fmt, clippy `-D warnings`, `cargo test --workspace --all-features --locked` with the binaries
      present (adds 20 core tests and 16 repository tests; `jarvis-core` at 262, `jarvis-storage` at 215),
      `cargo deny check` ok — one `yoke-derive` version had to be updated because the advisory database began
      reporting the pinned `0.8.3` as yanked, which is the database moving rather than this change.
      **Limits, recorded rather than glossed:** **no external skill format is read yet.** The *mechanism* for
      recording a drop exists and is tested, but no parser for any external document does — and adopting one
      requires the research `.github/instructions/external-integrations.instructions.md` mandates, including
      deciding which fields a real format carries and which of them are authority-bearing. So `P4-011`'s
      "an external skill format is readable only under `ADR-0117` §7" is **unimplemented by choice**, and the
      drop vocabulary is the seam a format would plug into. Also: there is **no creation surface** (no route
      and no tool writes a skill, so a skill is reachable only from a test), which `P4-013` owns; `steps` and
      `dropped_fields` are JSON columns with `json_valid` bounds rather than child tables, recorded in the
      migration with the reasoning; and no `P4-012` retrieval selection existed at the time of this slice —
      selection and envelope inclusion were delivered later (see `P4-012`), and **the execution path still does
      not**, so nothing has run a skill's steps and nothing in the daemon calls `select_skills`.
<a id="p4-012"></a>

- [x] `P4-012` Retrieve and use a skill: selection by relevance to the task, inclusion in the context envelope
      as derived content, and an execution path in which **every step is an ordinary tool request** through
      the full gateway. There is deliberately **no "skill execution" path**, so a step whose grant was revoked
      between load and run must refuse at execution — the check can only be an execution-time one.
      **SELECTION AND ENVELOPE INCLUSION ARE DELIVERED; THE EXECUTION PATH IS NOT, AND THE REMAINDER IS NAMED
      AT THE END RATHER THAN FOLDED INTO A CHECKMARK.**
      **Delivered:** `jarvis-core::skill` gains `SkillQuery`, `SkillIneligibility`, `ExcludedSkill`,
      `SelectedSkill`, `SkillSelection`, `SkillSelectionReason`, `select_skills`, `matches_text`,
      `estimate_skill_tokens`, and `skill_context_item`; `SkillRevision` gains a **`sensitivity`** (with the
      migration column, repository threading, decode rule, and a `CHECK`); and `ContextSourceKind` gains
      **`Skill`** so a procedure's trust rule is representable.
      **⭐⭐ THE `Skill` SOURCE KIND EXISTS BECAUSE THE RULE IS OTHERWISE UNREPRESENTABLE.** Its allowed trusts
      are `[Derived, Untrusted]` — **`User` and `Authoritative` are deliberately absent** — because
      `ADR-0117` §3 says a self-authored procedure must not re-enter a prompt as the person's own
      instruction. **`Memory` does permit `User`**, since a memory can be the person speaking, so reusing the
      memory kind would have made the skill rule unexpressible: a table that allows `User` cannot refuse it.
      The rule is enforced at construction (a test builds a skill item with `User` trust and asserts
      `TrustNotAllowedForSource`), and falsified — widening the arm to include `User` fails the test.
      **⭐ A SKILL IS NEVER POLICY, EVEN WHEN THE PERSON WROTE IT.** `skill_context_item` maps
      `MemoryTrust::Authoritative` **down** to `ContextTrust::Derived`, which is the one place a skill's rule
      is *stricter* than a memory's: a procedure the person wrote is their earlier writing rather than the
      person speaking now. Both directions are asserted, so an implementation that mapped everything to
      `Untrusted` fails too — the distinction between "trusted enough to be derived" and "untrusted" is real.
      **⭐⭐ MY FIRST RANKING DESIGN WAS INCOHERENT, AND THE TEST THAT REPLACED IT RECORDS WHY.** The first
      `select_skills` ranked by **how many query words a revision matched**. That cannot work: eligibility
      already requires **every** query word to appear, so every offered skill matches the whole query and every
      score is identical. **A conjunctive filter cannot also be graded.** The design only appeared to function
      because the test exercising it used a query a *candidate* did not fully match — which the eligibility
      rule refuses outright, so the case could never arise through the function at all. The ordering now
      answers the question a caller actually has when two procedures both apply: **which is current**. So
      revisions order by **creation instant, newest first**, with the identifier as a tie-break so the order is
      total and stable across runs. A memory is still ranked by nine weighted signals and that remains right
      *there*: a claim's eligibility is a set of **floors** rather than a conjunction, so "which claim is most
      relevant" is genuinely graded. **The two retrievals differ because their questions differ.**
      **⭐ THE CLASSIFICATION IS WHAT MAKES THE DISCLOSURE RULE ENFORCEABLE.** `is_eligible` refuses a revision
      above the destination's ceiling and names **both** values in the reason, and the projection feeds the
      same field into `ContextItem`, so `assemble_context` applies it a second time. The schema column is
      `NOT NULL` with **no default**, deliberately: a default would be a value nobody chose applied to every
      pre-existing row, and the only safe default — the most restrictive — would silently refuse to offer every
      skill written before the column existed. A round-trip test asserts each of the four levels survives,
      because a column that silently defaulted would make the rule refuse nothing.
      **⭐⭐ THREE GUARDS AND A PRE-EXISTING LIMIT, FOUND BY ASKING WHERE THE RULE WAS REACHED.** Writing
      `a_skill_cannot_be_required_context` I expected `RequiredMustBeTrusted`, and the test reported, in order:
      `PriorityReasonMismatch` (a retrieved reason implies `Optional`), then `ReasonSourceMismatch` (three of
      the four required-implying reasons name their own source kind), then — for `ReservedPolicy`, which names
      no kind — **`None`, meaning the item is constructible**. The cause is that `RequiredMustBeTrusted` fires
      on `ContextTrust::is_external()`, so it refuses **`Untrusted` rather than every non-authoritative
      class**. A hand-built `Derived`-trust `ReservedPolicy` item on a skill source therefore **can** reserve
      policy budget for a procedure. **That is a pre-existing limit in `context.rs`, not one this slice
      introduced** — the identical construction works with a `Memory` source — and it is now an **asserted
      finding** rather than a latent surprise, with the production route recorded as unaffected
      (`skill_context_item` always supplies `RetrievedMatch`). **Asserting the behaviour found beats asserting
      the behaviour assumed**, and the assumed version would have left the code looking stricter than it is.
      Gates: fmt, clippy `-D warnings`, `cargo test --workspace --all-features --locked` (adds 16 selection and
      projection tests in `jarvis-core`, 2 repository tests, and 2 context-kind tests; `jarvis-core` at 278,
      `jarvis-storage` at 217), `cargo deny check` ok.
      **⚠ A STALE BINARY MADE THE PHASE-1 GATE FAIL WITH A MIGRATION ERROR.** Editing migration `0010` after a
      previous `cargo build` left `jarvisd` embedding the **old checksum**, so SQLx's checksum guard refused to
      re-migrate the database the stale daemon had written (`open database: failed to migrate the SQLite
      database`). `cargo build --workspace` resolved it, and the gate went green with no code change.
      **SQLx checksums embedded migrations, so editing an applied one without rebuilding every binary that
      embeds it produces a failure that reads like a schema bug** — the recorded trap, met again.
      **⭐⭐⭐ A PROMOTION BY THE REVISION'S OWN AUTHOR IS REFUSED, AND THAT RULE HAD NO ENFORCEMENT AT ALL
      UNTIL IT WAS WRITTEN.** `ADR-0117` §4 states the boundary in words — *"an agent that could author a
      procedure and promote it would have authored its own effect"* — and the schema states half of it as a
      `CHECK` (a model-authored **active** row must record who promoted it). **Neither was enforcement of the
      second half.** The construction check refuses a model-authored revision *recorded* `Active`, but its own
      author could simply call `promote` to put it there, and the `CHECK` was satisfied because the row now
      named an approver — the author. `SkillRevision::promote` now refuses `approver == created_by_actor_id`
      with `InvalidSkill::PromotionSelfApproval`, and refuses a blank approver with
      `InvalidSkill::PromotionUnattributed` — **separate variants on purpose**, because the remedies differ: one
      needs an approver *named*, the other needs a *different* actor to decide, and collapsing them sends an
      operator looking for a missing field that is not missing. The comparison happens **after trimming**, and a
      test asserts a whitespace-padded self-promotion is still refused, because the guard exists to be bypassed
      and a padded string is the first bypass an author would reach for.
      **This is deliberately the same rule `jarvis_core::ApprovalRequest` applies to a tool call**
      (`InvalidApprovalField::SelfApproval`), including its scope: there the rule is **unconditional**, so a
      *user* cannot answer an approval they requested either — checked against `approval.rs` rather than
      assumed before mirroring it, because a guard copied at the wrong scope is a guard that refuses the
      legitimate case. `promoted_by_actor_id` is also bounded by `MAX_APPROVER_ID_CHARS` — **the `decided_by`
      column's own bound**, imported rather than restated — because a promotion *is* an approval and one
      identity must not have two lengths; a domain that accepted any length would report a revision as valid
      and then fail on write. The bound test asserts **at** the limit is accepted and **one past** it is not, so
      an implementation that refused every approver fails the first half rather than passing on the second.
      **⭐ EVERY TRANSITION REACHES THE DOMAIN, WHICH IS WHY THE GUARD CANNOT BE BYPASSED FROM STORAGE.**
      `promote_skill_revision`, `archive_skill_revision`, `restore_skill_revision`, and
      `supersede_skill_revision` each `find` the row, call the domain transition, and only then write — so no
      repository function can move a revision to `Active` without constructing the value that holds the rule.
      The `UPDATE`'s own `WHERE state = 'proposed'` is **defence in depth against a concurrent writer**, not a
      second copy of the rule, which is the shape this repository prefers: the domain decides, the schema and
      the predicate catch a race.
      **⚠ THE LESSON IS THE `ADR-0035` SHAPE, MET AGAIN: a decision that names a rule in prose is not
      enforcement.** §4's words were quoted in the migration, in the constructor, and in `promote`'s own
      doc comment — three places that *described* the boundary — while nothing anywhere *checked* it. **Read
      each boundary a decision states as reasoning and ask which check owns it**; a sentence explaining why
      something is forbidden is evidence that it was considered, never evidence that it is refused.
      **⭐⭐ `select_skills` IS NOW CALLED BY THE DAEMON, AND A STORED PROCEDURE REACHES A REAL PROMPT
      FENCED.** `apps/jarvisd`'s `assemble_context_messages` reads the run's own workspace's **active**
      revisions through `read_usable_skill_revisions`, builds a `SkillQuery` whose **text is the objective**
      and whose **destination is the model's placement ceiling**, selects, and offers each as an item from
      `skill_context_item` at **`Optional`** priority — a procedure that does not fit must be dropped rather
      than failing the run, exactly as a conversation turn is. The message builder renders each included
      skill's **prose and every step instruction** inside the same fence the memories use, under the same
      introduction, because a procedure is retrieved content for the same reason a claim is (`ADR-0117` §3);
      the introduction's own promise — *"the policy and the request win"* — is exactly what a procedure
      needs. The run event's summary gained `skills_offered`/`skills_included`, so "why was this procedure not
      used" is answerable from the record without storing the procedure. Deliberately **no `skills_altered`**:
      a count that is always zero is a field a reader learns to skip.
      **⭐⭐⭐ THREE DEFECTS FOUND BY WIRING IT, AND THE THIRD WAS INTRODUCED BY FIXING THE FIRST.** (1)
      **`ORDER BY created_at` is wrong for these timestamps** — they are RFC 3339 text and a whole second
      **omits its fraction**, so `'…:20Z'` sorts *after* `'…:20.5Z'` (`.` is 0x2E, `Z` is 0x5A). With a window
      applied the read dropped a **middle** row and kept the oldest. (2) **`julianday` does not fix it** — it
      resolves to milliseconds, so the boundary case still ties or mis-orders. (3) **⭐⭐ THE TIE-BREAK'S
      DIRECTION WAS WRONG, AND IT ONLY APPEARS ONCE (1) IS FIXED.** `ORDER BY unixepoch(created_at) DESC, id
      ASC` orders a **tie** — two rows inside one second — **oldest-first**, which is the exact opposite of
      what a newest-first read is for. It was introduced by adding the tie-break to make the order total, and
      no fixture of evenly-spaced whole seconds can see it: **the tie-break is part of the same ordering, not
      a separate concern, and an ordering has ONE direction.** Every newest-first statement in `jarvis-storage`
      is now `unixepoch(<col>) DESC, <tie-break> DESC`: three in `skill_repository` and four in
      `memory_repository`, the latter including the **paginating** `read_all_memories` export.
      **⭐ THE ASSERTION THAT FOUND ALL THREE WAS "WHICH ROW IS ABSENT", NOT "HOW MANY CAME BACK".** Two of
      the three defects returned the **right count with the wrong set**, so a count-only assertion is satisfied
      by every one of them. `the_usable_read_returns_at_most_the_limit_newest_active_revisions` and
      `the_retrieval_window_returns_the_newest_memories` both assert identity, and the skill one was **proven
      to falsify** against the original ordering. Its fixture is deliberately awkward: three instants **inside
      one second** with the **oldest carrying a whole-second timestamp**, and identifiers whose milliseconds
      match their instants (as `Uuid::now_v7` writes them) so the tie-break agrees with the row.
      **⚠ The sub-second limit is recorded rather than hidden.** `unixepoch` resolves to whole seconds and
      `julianday` only to milliseconds, so genuine nanosecond ordering needs an **integer-nanoseconds column**
      — a migration across every timestamp column, which is `ADR-0034`'s decision and is still **open**;
      `jarvis-core/src/timestamp.rs` already pins the underlying trap with
      `the_stored_form_is_not_lexicographically_sortable`.
      (2) **A conjunctive match is brittle by construction, and the run looks successful either way.** A user
      asking about *"my notes"* is not offered a procedure that says *"the user's notes"*, because
      `matches_text` requires **every** word. Both behaviours are asserted — the positive case (the procedure
      arrives fenced, prose **and** step text) and the negative (`an_objective_with_an_unmatched_word_offers_no_skill`),
      because the failure mode of this rule is **silence**. The fence assertion was falsified by sending the
      item's reference instead of the rendered procedure; the test failed, then passed after restoring.
      `ToolValidator` also gained `Send + Sync`, because these reads are `async` and the daemon calls them from
      a **spawned task** — the bound belongs in the alias, or it surfaces as a call-site error.
      **Limits, recorded rather than glossed:** **the execution path is not built**, which is the half of this
      slice's own acceptance that says every step must re-enter the ordinary gateway — no "skill execution"
      path exists, so nothing runs a skill's steps at all, and no model-facing surface tells the model what to
      call first; there is still **no creation surface** and no `P4-013` inspection verbs, so a skill remains
      reachable only from a test; text matching is whole-word containment over the prose and step
      instructions, deliberately **not** the nine-signal ranking a memory gets, for the reason recorded above.

      **⭐ PART 2 — a procedure reached the model with NO TOOL IN IT, and three artefacts hid it.** `P4-013`'s
      creation route made a stored procedure reachable for the first time, so I checked what a *selected*
      procedure actually sends. It sent the prose and each step's **instruction** and **dropped the tool**:
      - **One cause, three silent consequences.** The executor concatenated the body itself; `estimate_skill_tokens`
      counted the same fields; `skill_haystack` indexed the same fields. So (1) the prompt had no tool, and
      `P4-012` is literally "every step is an **ordinary tool request**"; (2) the budget **under-measured** the
      text it sent, so a skill reporting itself as fitting could overflow; (3) `matches_word`'s own doc comment
      — "what makes a query naming a tool find a procedure that calls it" — was **false**, because an objective
      containing `jarvis.files.read` splits into segments the haystack did not contain.
      - **The fix is one function, `jarvis_core::render_procedure`**, which the executor, the estimate, and the
      haystack all derive from. Three artefacts that must agree can no longer disagree.
      - **The e2e test asserted three fields of the rendering and never the tool**, which is why it passed: a
      test that checks several fields is not a test of the rendering, and the absent field had nothing
      asserting it. It now asserts `jarvis.files.read@1.0.0` reaches the prompt.
      - **Falsified at both layers** — reverting the renderer failed the exact-string test, the tool-matcher
      test, **and** the `jarvisd` prompt test.
      - **⭐ THE RENDERING MADE A LATENT INJECTION REACHABLE: a step's `tool_version` was validated by LENGTH
      ONLY.** It is interpolated into the body, so a version containing a newline forges a second step that the
      author never wrote — in a body a model then follows as a procedure. The rule was length-only in
      `SkillStep::new`, length-only in `0010`'s `CHECK`, and the migration's own comment claimed the two
      "cannot disagree about what a version looks like" while **neither looked at a character**. Now validated
      by its characters (`[A-Za-z0-9.\-_+]`, the set `ToolId::validate_version` accepts, restated because
      `jarvis-core` cannot depend on `jarvis-tools`). The decode goes through the same constructor, so a row
      that arrived another way is refused on read too.
      - **And the §6 property is now asserted:** a procedure naming a tool that **declares** an approval still
      parks the run with zero adapter calls. There is deliberately no "skill execution" path to test — the step
      is an ordinary tool request through the whole pipeline, and that is how the property is true.
      - **Still not built, and the remainder is now narrowed:** nothing gives a model a *first step*. The
      procedure now says which tool each step needs, so a model following it can issue the call; what no surface
      does is tell the model to begin, and a procedure whose step names a tool the model is not offered cannot
      be performed by it. The ADR's "a grant revoked between load and run must refuse the step" is also
      **unasserted** because nothing revokes a grant while a run is live and the registry is fixed at
      composition — recorded as a limit rather than claimed.
      - **Verified live:** a procedure created through `jarvis skills create` is `active`, and
      `jarvis skills show` renders `1. jarvis.memory.propose (1.0.0)`, so the tool and version survive the store.
<a id="p4-013"></a>

- [x] `P4-013` Add skill inspection and control: list, inspect (what it is, what it names, its source), correct,
      forget, export, and disable — the `FR-MEM-005` lifecycle surface, applied to a stored procedure.
      **Delivered:** `jarvis-protocol::skill_api` (14 DTOs), `apps/jarvisd/src/skill_service.rs`,
      **8 routes** (`GET`/`POST /skills`, `GET /skills/export`, `GET`/`DELETE /skills/{id}`,
      `POST /skills/{id}/{promote,disable,enable}`), `apps/jarvis-cli/src/skills.rs` (8 verbs), and the storage
      operations they need (`find_skill_revision_state`, `read_workspace_skill_revision_states`,
      `delete_skill_revision`, and an optimistic-locking guard on all four transitions). **The creation surface
      `P4-012` recorded as missing is now the `POST /skills` route**, so a skill is no longer reachable only
      from a test — which is what the retrieval path needed to have a producer at all.
      **⭐⭐⭐ THE CREATION SURFACE IMMEDIATELY FOUND A DEFECT THAT MADE `enable` DESTROY A USER'S PROCEDURE.**
      `SkillRevision::restore` derived the restored state from the **promotion record alone**, so a
      user-authored revision archived and re-enabled came back `Proposed` — and **no actor could ever fix it**,
      because promotion refuses an approver equal to the revision's author and that author *is* the user. A
      disable/enable pair permanently destroyed a procedure the person wrote, and nothing in the domain, the
      storage layer, or the earlier tests could see it: the shape was only reachable once a route could create a
      user-authored revision. The derivation now uses the same three facts construction does (a model-produced
      revision is usable only if promoted; a user-authored one needs no promotion), tested both ways so a fix
      that returned everything to `Active` fails.
      **⭐⭐ A SUPERSESSION CHAIN COULD LEAVE ITS OWN PROCEDURE.** `supersede_skill_revision` confirmed the
      successor **exists** (`acknowledge_skill_revision`) and never confirmed it was **related** — so
      `superseded_by` could point from one procedure to a revision of a **different** one, and "what replaced
      this" would answer with another skill's revision. The `ON DELETE SET NULL` columns made the deletion case
      equally wrong: deleting either end quietly **blanks the other's link**, erasing one leg of the declaration
      `ADR-0117` §5 exists to make readable. The transition now requires both ends to share a `skill_id`, checked
      in **one statement** rather than a read plus a comparison (a property of the *pair*, so two round trips
      leave a window), and `delete_skill_revision` refuses a revision any other revision links to. Both refused
      **before** any write, asserted on the row so a check that reports an error and writes anyway fails.
      **⭐⭐ `promote` HAD TWO STATE DEFECTS AND ONE `SelfReference` REUSE.** It reported
      `ModelAuthoredTrust` — the *provenance* error — for an already-active revision, sending an operator to
      inspect a procedure's provenance for a state problem; and it **accepted an archived revision**, making
      `promote` a second undocumented route to `Active` that bypassed `restore`. Meanwhile `archive`/`restore`
      reused `InvalidSkill::SelfReference` ("a value referring to itself") to mean "wrong state". All three are
      now one vocabulary: `InvalidSkill::WrongState { transition, state }`, with the verb as a field because the
      verb differs and the fact does not. The storage mapping keeps a **per-verb reason** (`already active`,
      `archived; restore it instead`) because each remedy is a different action.
      **⭐⭐ A `version_counter` WITH NO READER, AND THE GUARD IT WAS FOR.** The column was written by
      `record_skill_revision` and incremented by every transition, and **no read returned it** — so the
      optimistic guard a control surface needs had no way to obtain the value it must present. Added
      `StoredSkillRevision`, a narrow wrapper (`revision` + `version_counter`) used by the **control** reads
      only: the retrieval path deliberately does not return it, because a counter has no meaning to a selection
      that is a pure function of the revision set, and widening a hot read would put a value with no reader on
      it (`ADR-0092`). All four transitions now guard on `version_counter` **and** the state they start from.
      **⭐ ZERO ROWS HAS TWO CAUSES AND THE REMEDIES ARE OPPOSITE.** A guarded `UPDATE` affecting nothing is
      either a **stale counter** (the row exists and changed — re-read and retry) or a **missing row** (stop),
      so `classify_skill_write_failure` resolves which by reading, and both are asserted separately; an
      implementation reporting one for both fails one of them.
      **⭐ THE LISTING CARRIES NO PROCEDURE TEXT, AND THE FENCE DISCIPLINE FROM `P4-012` IS WHY.** A skill's
      prose and step instructions both reach a prompt, so a reference that leaked them would be a second, less
      careful path into a model's context than retrieval's. The test asserts the prose is **absent from the
      listing and present on the detail read**. `detail_of` also reports usability from the **domain's own**
      `is_eligible` rather than from `state` + `superseded_by` — the first version re-derived it and **disagreed
      with retrieval** on a revision that is both archived and superseded.
      **Verified live, not only in tests:** `jarvis skills create/list/show/disable/enable/forget` were run
      against a real `jarvisd`, and the `enable` output (`returned to active from its promotion record`) is the
      user-authored stranding defect observed as **fixed** on a real profile. A stale `--version` returned the
      `409` conflict with the re-read remedy, an already-active `promote` returned the state refusal rather than
      a provenance one, and the two CLI step-pairing refusals named the actual problem.
      **⚠ A HINT THE DAEMON REFUSES.** The client's own message said *"set `JARVIS_HTTP_ENABLED=1`"*, and the
      daemon parses that variable as a **strict `bool`** — so following the hint produced
      "invalid value for configuration environment variable" with no indication of what would be valid. The hint
      now spells out `true`. **A hint that names a value the daemon rejects sends an operator to look for a
      problem that is in the hint.**
      **⚠ AND THE DAEMON'S OWN MESSAGE HAD THE SAME GAP, which cost two failed attempts to find.** While
      verifying against a live daemon I set `JARVIS_HTTP_ENABLED=1` and read
      `invalid value for configuration environment variable JARVIS_HTTP_ENABLED` — naming the variable and
      nothing about what would be accepted. `ConfigError::InvalidEnvironmentValue` now carries an `expected`
      field, and every site states the vocabulary **its own parser** accepts: `true or false`,
      `one of trace, debug, info, warn, error`, `a whole number of seconds`, `a port number from 1 to 65535`,
      `valid Unicode text`. The real daemon now answers `…: expected true or false`.
      - **The expected string is bounded by what that check actually parses, not by the rule that matters most.**
        A value like `JARVIS_SHUTDOWN_TIMEOUT_SECONDS=0` parses fine and is refused later by
        `InvalidShutdownTimeout`, whose own message names the range — so writing "1 to 300" in the parse refusal
        would claim coverage it does not have. Both refusals are asserted, because they arrive from one variable
        and are easy to conflate.
      - **The rejected value is never echoed**, and that is asserted: an environment value can be a credential,
        and a startup log is read by more people than the shell that set it. The two properties are one test with
        two assertions because either alone is satisfiable by the wrong message.
      - **Class of defect worth naming: a message that identifies the problem and not the remedy.** This is the
        third instance in this slice (the `JARVIS_HTTP_ENABLED=1` hint, this message, and the absent-route
        report), and the shape is always the same: the text says *what* failed and leaves *what would work* to
        be guessed, and the obvious guess is the one the code rejects.
      **Limits, recorded rather than glossed:** **the execution path is still not built**, so nothing runs a
      skill's steps and the lifecycle ends at "this is available"; `correct` is expressed as a creation with
      `--supersedes` + `--skill` rather than a dedicated route, and a correction's `skill_id` must be **stated**
      by the caller because the domain compares the predecessor against the skill and neither is derivable
      without the store; the export takes no offset; `sensitivity` is fixed at `internal` on creation (no flag
      to classify a procedure, so a `public` one cannot be authored here yet); and the tool-membership check
      needs a composed pipeline, so with no tool surface only the structural identifier rule applies.
<a id="p4-014"></a>

- [ ] `P4-014` Add agent-proposed memory admission: the model may submit memory candidates, a candidate that
      deterministic code supports becomes a `Proposed` record, and admission is a decision that names its
      approver. The inference boundary is unchanged (`P4-001`, `ADR-0049` §6): a model inference is never
      admitted above `Unverified`.
      **⭐ PARTIALLY DONE — the "becomes a `Proposed` record" half was BROKEN and is now fixed, and the
      decision that names its approver now exists.** Starting the slice, I checked the boundary's *claim*
      against the code rather than adding to it, and the two layers that are supposed to agree disagreed:
      - `MemoryCandidate::admit` decides `MemoryAdmission::Proposal` for a model inference. Its own comment
        says one "is never written as current truth", and `CandidateClassification::requires_proposal` is the
        predicate. `MemoryAdmission`'s doc says writing a `Proposal` and reading it back "is what proves the
        two layers agree rather than merely intending to" — **nothing did that**.
      - `MemoryRecord::build` derived the status from `memory_type.requires_confirmation()` **alone**, so it
        answered `Active` for every inference. A `source_kind` of `model_inference` sent to
        `POST /api/v1/memories` was answered `201` with status `active`: the model's own claim stored as the
        workspace's current truth, contradicting `ADR-0049` §6 and the module comment in both crates.
      - **The harm is concrete, not a labelling nicety.** `compare` reports a candidate whose text differs
        from the current claim at its key as a `Correction`, and a correction **supersedes**. So one model
        inference silently retired an earlier claim — an effect produced by the disagreement.
      - **The fix is one condition**, `|| source.kind().is_model_produced()`, which is the rule
        `jarvis-core/src/skill.rs` already applies to procedures in two places — so the memory layer was the
        one that lacked it. The confidence ceiling (`Unverified`) and the status were two different rules, and
        only the first was enforced: bounding the level is not the same as refusing the claim as current truth.
      - **Three tests encoded the old behaviour, and one had the invariant as its NAME.** `memory/tests.rs`'s
        inference test asserted a raised confidence is refused and that `Unverified` is `is_ok()` — so a
        derivation returning `Active` passed it. `retrieval/tests.rs` used an inference as a convenient
        `Derived`-trust fixture and was actually refused by the **trust** rule, so the currency rule was never
        reached; it now uses a tool observation and the inference has its own test asserting the exclusion
        holds even against a query relaxed on every requirement. `jarvis-storage`'s test named
        `a_model_inference_cannot_be_recorded_as_a_fact` while asserting, in a comment, that the claim **was**
        `is_current_truth()` and that the *presentation* predicate was the thing keeping it from being
        asserted — "which is why the two are separate". Two layers disagreeing about one question is not a
        separation of concerns.
      - **Falsified at both layers.** Reverting the one condition failed the new `jarvis-core` test
        (`a claim nothing can support is a proposal, not a fact`) **and** the new gateway test
        (`a claim no source can support must not be the workspace's current truth`), each in the binary a
        developer would run. A control asserts a user-stated `Semantic` claim is still `Active`, so the rule
        is not satisfied by proposing everything.

      **Delivered for the second half — the decision that names its approver:**
      - **Migration `0011_memory_admission.sql`** adds `admitted_by_actor_id` and `admitted_at`, modelled on
        `skill_revisions.promoted_by_actor_id`/`promoted_at`. `CURRENT_SCHEMA_VERSION` 10 → 11. The backfill is
        deliberately **empty**: rows that predate admission were confirmed by a route that recorded nobody, and
        inventing an approver would fabricate an authorization for a decision nobody made.
      - **`MemoryRecord::confirm_by(approver, at)`** records the decision; `confirm` is kept as the **decode**
        companion, because a row written before admission decodes to `active` with no approver and re-applying
        the stored status must not invent one.
      - **`MemoryTransition::Confirm { approver_actor_id }`** — the approver is part of the variant, not an
        argument beside it, so a state change cannot be separated from the decision that justifies it.
      - **`POST /api/v1/memories/{id}/confirm`** plus `ConfirmMemoryRequest` (which carries **only**
        `expected_version`) and a `jarvis memory confirm <id> --version N` verb. `MemoryTransition::Confirm`
        existed from `P4-002` and **no route called it**, so a `Proposed` claim could be created over HTTP and
        never accepted — a claim the workspace held, offered to nobody, with no way to make it current.
      - **A defect found on the way: the author identity was fabricated.** `remember` hardcoded the string
        `"local-user"` while every other write path uses `LOCAL_USER_ID` (`0198f000-…-0000000000b1`, what
        `0005` actually seeds). It now reads the identity, as `SkillService::actor_id` does.

      **⭐ A GUARD I WROTE AND THEN DELETED, BECAUSE IT BLOCKED THE ARCHITECTURE'S OWN FLOW.** I first added a
      self-admission refusal mirroring `ADR-0117` §4 — an approver must not be the claim's own author. It made
      the live route answer `503` and, once that was diagnosed, `422` on the *intended* case:
      `memory-and-context.md` says a high-impact inference "requires explicit user confirmation", so the person
      confirming **is** the person whose statement produced the candidate. With one seeded identity the author
      and approver are always the same value, so the guard refused every legitimate confirmation. The rule worth
      wanting — *an agent must not admit what it authored* — needs an actor vocabulary that distinguishes a
      model from a person, and this build has one seeded human; even a run's model-produced candidate is stamped
      with that human's identifier. **`ADR-0117`'s rule is about a procedure's promotion, where the author can
      be a model; copying it onto a memory, where the author is a person, inverts it.** Two tests now assert the
      *non*-refusal so the symmetry cannot be restored without seeing why.
      - **Also fixed: a corrupt row answered `422`.** `decode_memory` mapped a domain refusal to
        `InvalidMemoryRequest`, which names a **request** field for a value the caller never sent — sending an
        operator to inspect their request instead of their data, and telling a caller to change what they ask
        for when what they need is a backup. It now reports `StoredMemoryInvalid`, and the field table is shared
        between the two mappings rather than duplicated.
      - **Falsified:** binding `None` for `admitted_at` (the "did not write the column" bug) failed the new
        storage round-trip test **and** both gateway tests. The half-decision rule catches it too — the mutant
        produces a row the decoder refuses, which is the property that makes the pair safe to store.

      **Still unbuilt, and now the whole of the remaining work:** no **tool** touches memory at all (every
      module in `jarvis-tools` is files/documents/policy/registry), so the model has no submission path; the
      route therefore needs a `source_kind` a model cannot set itself, and the submission path is what makes
      "a model may propose" true. `SkillService` documents a `propose` path for an agent-authored revision that
      still does not exist. The admission columns carry only their own bounds — SQLite cannot add a
      column-level `CHECK` mentioning `status`, and a table-level one needs the twelve-step rebuild that
      recreates four indexes and two self-referencing foreign keys — so the cross-column rule is enforced on
      the read path, which is the only enforcer; see `ADR-0124`.

      **⭐ PART 3 — the model's submission path now exists, and building it exposed the reachability defect the
      slice would otherwise have shipped.** `apps/jarvisd/src/memory_propose.rs` is the adapter:
      `jarvis.memory.propose`, a `write` at risk 1, approval `auto`, scope `memory.propose`.
      - **The arguments are narrower than `RememberRequest`, and every omission is load-bearing.**
        `additionalProperties: false` refuses `source_kind`, `confidence`, and `supersedes` — each of which
        exists on the HTTP request and each of which would defeat the boundary **through a field** rather than a
        bug: a claim labelled `user_statement` at `confirmed` is a model's output recorded as something the
        person said. The source is set in code, the confidence is `Unverified`, the locator is derived from the
        correlation identity, and supersession is impossible.
      - **Asserted through the pipeline's own schema validation, not the adapter's parser**, because the parser
        never sees those fields — testing it alone would prove nothing.
      - **Falsified:** switching the source to `UserStatement` failed the test, which reported the claim was
        stored as **`remembered`** — a status only a confirmed user statement can have. The mutant is caught by
        the outcome string, not by a provenance check.
      - **A judged candidate is a completed call, not a failure.** An unresolved entity or a refused rule is
        reported as `refused` with the rule's name, because telling a model the platform broke when the platform
        judged its input produces a retry loop. `UnknownValue` — the *common* case for a model naming a subject
        it only inferred — is in this arm rather than treated as an adapter fault.

      **⭐⭐⭐ THE TOOL SHIPPED REGISTERED, OFFERED, AND DENIED — because the actor's authority was a hand-written
      scope list.** `ToolActor` had `workspace_and_mcp` granting `files.read` + `mcp.call`; no constructor
      granted `memory.propose`, so every call would have been refused `MissingScope`. Nothing catches this:
      `Dispatch::verify_covers` checks the opposite direction (a registered tool with no adapter), and the
      adapter's own tests pass because they call the adapter directly.
      - **I then made the same mistake a second time within one change.** Adding `workspace_and_memory` with
        `files.read` + `memory.propose` *replaced* `mcp.call` at the HTTP site, and `phase_3_gate` failed:
        **`a write tool must pause for a human decision, got 403 missing_scope`**. I had reasoned that the
        surface served only native tools — it serves MCP tools too, and I had not checked.
      - **The fix is the pattern `ToolActor::remote` already used**: derive the authority from the **composed
        tools' own declarations**. A tool that is composed is callable; a tool that is not cannot be addressed;
        so the union *is* the daemon's authority over its own surface. No list, and a new adapter is reachable
        the moment it is registered.
      - **`workspace_and_memory` was then deleted**, because its only caller was the test I had just written.
        `for_composed_tools` superseded it, and a public constructor nothing calls is the shape this file's own
        comment warns about. `workspace_and_mcp` and `MCP_CALL_SCOPE` became `#[cfg(test)]` — the established
        pattern — because they survive only as the fixture proving the grants are distinct.
      - **And `jarvis tools preview` had never worked.** Its path used `path_segment`, the **run** identifier
        rule, which permits only alphanumerics and hyphens — so it rejected **every** tool identifier, since
        `namespace.name` contains dots, and the error named a *run*. Added `dotted_path_segment`, whose
        permitted set is the tool grammar (`[a-z0-9_\-.:]`, lowercase only, which the first attempt got wrong in
        both directions: it refused `list_issues` and accepted `Jarvis.memory`). `..` and the leading/trailing
        dot are refused as path-traversal and grammar cases — the traversal one found by writing the
        adversarial case into the test's name and watching it pass when it should have failed.
      - **Why the preview mattered more than it looks:** it answered `deny / missing_scope` for a tool the daemon
        was offering. A preview that lies in the refusals direction is worse than no preview, because it sends
        an operator to change a policy that is already correct.
      - **Verified live:** `jarvis tools list` → `jarvis.memory.propose … callable`; `tools preview` → `allow /
        allowed`; a run reaches `outcome=succeeded` with the tool present. Both defects were found by running the
        CLI against a real daemon, not by the suite.
      - **Limits:** with one seeded identity and no entity-creation surface, no end-to-end propose through a live
        model was possible — the adapter's behaviour is proven by its tests and the registration and preview by
        the daemon. The tool is `auto` because the claim is stored `proposed` and excluded from retrieval at any
        status; that reasoning is asserted beside the declaration, so changing the status rule forces a decision
        about the approval.

<a id="p4-015"></a>

- [x] `P4-015` Add session summarization as derived memory: a compressed summary of a session is stored as a
      `Derived` claim with provenance and a retention rule, and is never presented as user-authored fact.
      **Domain vocabulary** in `crates/jarvis-core/src/summary.rs` (`SessionSummary`, `SummarySpan`,
      `SummaryLoss`, `spans_overlap`, `is_offerable_as_context`, **15 tests**); **storage** in
      `crates/jarvis-storage/src/summary_repository.rs` + migration `0012` (**14 tests**); **surface** as
      `POST|GET /api/v1/sessions/{id}/summaries` and `POST …/summaries/retire`, with **4 route tests**.
      `ADR-0125`.
      - **A summary is a memory plus a span table, and the split is the decision.** The memory half is what every
        memory rule already covers (workspace, entity links, tombstones, sensitivity), produced by
        `into_record` as `Conversation` + `Document` → `Derived`. The span half is what no `memories` column can
        hold, in a table whose **primary key is the memory identifier**, so "this row is a summary" and "this row
        has a span" are one fact rather than a nullable pair meaningless for six of seven types.
      - **Its own rules live in the writer, because SQLite cannot state them.** `session_summaries` cannot see
        `messages`, so it cannot refuse a span naming turns that do not exist; and "two intervals in one session
        must not intersect" needs a trigger. Both are checked in `record_summary`, which reads the message count
        and the session's spans on the **same connection** as its two inserts — a deferred read-then-write over
        the pool is the `SQLITE_BUSY_SNAPSHOT` failure `purge_memory` already records.
      - **The retention rule had a predicate and no caller.** `MemoryType::is_durable` states that `Working` and
        `Conversation` do not survive, and only a test named it; `P4-008` recorded "no retention policy is
        implemented — the verbs exist and the sweeper does not". The sweep now **generates** its SQL predicate
        from `is_durable` rather than listing type names, so the rule and the sweep cannot drift when a type is
        added. Archive rather than delete, because `Archived` is "retained for audit, not retrieved as current
        truth" — and deleting would additionally clear the search key and write a tombstone, turning a retention
        decision into a *forget*.
      - **A predicate whose answer never varied was a decision that decided nothing.** `is_offerable_as_context`
        first returned `false` unconditionally, which reads as a policy and encodes none; it now takes the spans
        a caller is **replaying** and answers the real rule, which also gave `spans_overlap` the production
        caller it lacked.
      - **Two defects the tests found, both by mutation rather than by review.** An archived summary was still
        returned by `read_session_summaries` — it filtered `deleted` and admitted `archived`, the same
        admitted-but-never-current shape `P4-007` recorded — so the read now filters `active` while the *span*
        read deliberately does not, or archiving a summary would re-open its turns to a second claim.
      - **A test named for the wrong guard.** `the_same_sequences_in_another_session_do_not_overlap` claimed to
        catch a missing query filter; mutating `spans_overlap` to ignore session identity **survived** it, and
        only `jarvis_core`'s `overlapping_spans_are_detected_in_both_directions` caught that. Mutating the
        query's `session_id` filter *did* fail it. Two guards, two detectors, and the comment now says which is
        which — a test named for a rule nothing checks reads as coverage.
      - **A fixture that promised a count it did not deliver.** `seed_session` appended N messages, but starting
        the run already writes the objective as message 0, so every session held N+1 and every expected span was
        shifted by one while still looking plausible. It now asserts the count it promises.
      - **Verified live** against a real daemon (`database_schema=12` in the readiness line, so the migration
        ran): `GET /sessions/{id}/summaries` → `{"summaries":[],"returned":0,"limit":128}`; a summary whose
        subject is a fabricated entity → `404` (the service's subject check, reached); a malformed session
        identifier → `422`; `?limit=0` → `422`; `POST …/summaries/retire` → `{"archived":0}`.
      - **Limits:** with no entity-creation or entity-listing route (the `P4-008` gap, unchanged), a real
        end-to-end **store** over the wire was not possible — the write path is proven by 14 storage tests and
        the routes by 4, and everything before the subject check is proven live. Nothing yet moves a history
        window past a span, so `is_offerable_as_context`'s `true` branch is reachable only from a caller
        supplying non-overlapping spans: the rule is complete and falsified, and what is missing is a windowing
        component. The summary route is HTTP-only — no CLI verb, matching `P4-014`'s tool, because both need an
        entity identifier the CLI has no way to obtain.

<a id="p4-016"></a>

- [x] `P4-016` Add the entity surface: an operator can create, list, and look up the entities a memory is
      `about`, and attach an alias to one so a later lookup resolves by name rather than by identifier.
      **The gap three slices recorded.** `P4-008`'s note is that "**No entity-creation surface exists**, so a
      remember is still unreachable by a user of the shipped product"; `P4-014` and `P4-015` each record the
      same limit from a different direction. The measurement behind this slice: seven entity-repository
      functions had **no production caller at all** — `merge_entities`, `record_alias`, `resolve_alias`,
      `read_alias_candidates`, `read_entity_memories`, `record_relation`, `read_subject_relations` — and the
      last-named two are exactly the "entity resolution" that note names as missing.
      **New:** `crates/jarvis-protocol/src/entity_api.rs` (13 DTOs), `apps/jarvisd/src/entity_service.rs`,
      `apps/jarvis-cli/src/entity.rs` (**3 tests**), 5 routes with **6 route tests**, 4 storage reads and one
      counter. `ADR-0126`.
      - **Its own surface, not a sub-resource of `memory`.** An entity is not a memory: different lifecycle
        (created, aliased, merged versus proposed, confirmed, corrected, forgotten), different owner (the
        operator's vocabulary, not the model's), and a different role — it is what a claim is *about*. As a
        sub-resource it would put "create the subject" behind the verb that requires one, which **is** the
        recorded ordering problem.
      - **A lookup returns every candidate with its evidence, and the CLI renderer is where that rule could be
        undone.** `resolve_alias` documents it — "ambiguous aliases remain separate candidates" — so the reply
        is a list and each match says whether it was `verified`. A client printing `matches[0]` would present a
        guess as an identity at the last step, so the renderer has its own test and the daemon exposes **one**
        route for both lookup kinds rather than two that could diverge.
      - **The verdict is the conjunction**: an entity holding a verified name *and* a probabilistic one for the
        same value is not verified, because the question is whether this name denotes this entity and one of the
        matching names is a guess.
      - **⭐⭐⭐ The schema's rule and the nearest domain predicate were DIFFERENT SETS, one line apart from being
        treated as one.** `0009` permits a `confirmed` alias only from `user_statement`/`user_correction`, while
        `MemorySourceKind::permitted_trust()` returns `Authoritative` for those **and** `provider_record`. Trust
        asks "may this content instruct"; identity asks "who established this". Added
        `MemorySourceKind::is_user_stated()`, and **falsified the mistake**: substituting the trust predicate
        into the guard fails the route test with `503` where `422` is required — the caller's error surfacing as
        "the database is unavailable", sending an operator to debug the wrong thing.
      - **Five rules falsified**, each in the binary a developer runs: the verification conjunction (→
        disjunction); the listing's `status = 'active'` filter (→ `<> 'deleted'`, which re-listed a merged
        entity); the alias-source guard substituted with the trust predicate; the same guard disabled entirely
        (**the complement direction** — a guard that never fires must also fail a test); and
        `is_verified_alias`'s probabilistic arm (→ `true`, caught by two tests).
      - **⭐⭐ A mutation experiment corrupted three unrelated queries, and only a row count caught it.** The
        restore step used a **global** string replace, so `status <> 'deleted'` → `status = 'active'` was applied
        to `read_workspace_memories`, `read_entity_memories`, and `read_subject_relations` as well — silently
        narrowing each. `a_correction_supersedes_the_old_claim` failed because it **counted rows**; every other
        memory test passed, because they assert on `is_current_truth` and a narrowed filter drops exactly the
        rows those assertions ignore. The three queries were restored and an assertion was added beside the read:
        *the superseded claim must remain readable — an audit trail nobody can read is not one*. Re-applying the
        exact mutation then failed that assertion, so the detector is proven rather than assumed.
        **Generalise: a mutation restore must be scoped to its occurrence, and a read's filter needs a test that
        counts what it excludes.**
      - **⭐ `parse_summary_limit` became `parse_limit(raw, default)`** because there are now three listing bounds
        (200, 128, 50); three parsers would be three places for a bound to be wrong.
      - **Verified live** through the CLI against a real daemon (`database_schema=12`): `entity create "Ada
        Lovelace" --kind person --confidence confirmed` → an identifier; **`memory remember "Prefers morning
        meetings" --type preference --source user_statement --entity <id>` → `status active`**, which is the
        recorded gap closed; `entity alias … --verification confirmed --source user_statement` → attached;
        `entity lookup --alias-kind email` → `verified … matched alias`; `entity merge <other> --into <id>` →
        the winner, after which `entity list` no longer offers the loser; and
        `--verification confirmed --source provider_record` → `422` naming the remedy, exit `4` (`Denied`).
      - **Limits, recorded rather than glossed:** a label lookup is **exact** (case-insensitive, whole-label) and
        not a text search, because a fuzzy match would turn a guess into an identity — a caller wanting partial
        names has no surface yet; there is **no entity deletion**, so `merge` is the only way to retire one and an
        entity created in error stays listed until merged; `entity_relations` is still unreachable
        (`record_relation`, `read_subject_relations`), because a relation is a claim *between* entities and needs
        a surface of its own; and the CLI's argument parsing has one unit test per helper rather than an
        end-to-end one per verb.
