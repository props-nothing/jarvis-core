# Tools, policy and MCP evidence through 2026-10-16

Archived on 2026-10-16 from the implementation ledger. This preserves the original dated claims, failures, corrections and unchecked items; it is **not current delivery status or work order**. Later entries and accepted ADRs may supersede earlier statements. Use the [active backlog](../../TODO.md) and [history index](README.md) for current disposition. Relative Markdown links were adjusted when this material moved.

---

## P3: Tools, Policy, Approvals, And MCP

<a id="p3-001"></a>

- [x] `P3-001` Define canonical tool identifiers, JSON Schema 2020-12 input/output, effects, risk, scopes, source, version, timeout, retry, and idempotency metadata.
      Implemented as the `jarvis-tools` crate: `ToolId` (`namespace.name`, split on the last dot,
      source derived from the namespace), `ToolSchema` (2020-12 only, compiled once, refusing any
      non-local `$ref`), `EffectSet`, `Risk` (typed 0..=3, refused below the effect floor),
      `ScopeSet` (`resource.action`, JARVIS capabilities rather than provider OAuth scopes),
      `ApprovalPolicy`, `RetryPolicy`/`RetryDeclaration`, `Idempotency`, `Availability`,
      `ToolSensitivity`, `ToolOutcome`/`ToolOutcomeRecord`, and the aggregate `ToolDefinition`.
      Three cross-field rules are enforced at construction, including through a parsed manifest:
      risk ≥ the effect floor, no blind retry of an ambiguous effect, and a declared source that
      must agree with the identifier. 96 in-crate tests. ADR-0015 records the durable decisions;
      `docs/research/integrations/json-schema-validation.md` records the `jsonschema` 0.57.0
      selection and why its default features (HTTP/file `$ref` resolution) are disabled.
      Deliberately not a placeholder: this slice declares and validates metadata and decides nothing.
      Policy evaluation is `P3-003`; registration is `P3-002`; execution is `P3-005`. No adapter is
      wired to it yet, so no tool can actually be called.
<a id="p3-002"></a>

- [x] `P3-002` Implement the capability registry with collision detection, namespacing, dynamic availability, and compact model-facing discovery.
      `ToolRegistry` in `crates/jarvis-tools/src/registry.rs`: `define` refuses a collision and
      **leaves the first tool intact**; `define_all` loads a manifest atomically, with a collision
      inside the batch refused rather than last-writer-wins; `replace` requires a version change, so
      new behaviour cannot hide under an unchanged version. Availability has two sources — the
      definition's declaration and an optional runtime state — and the restrictive one always wins,
      so a health check cannot enable what the build declared impossible. `discover` is model-facing
      and bounded (`MAX_DISCOVERY_TOOLS`), offering only callable tools and reporting omissions by
      count; `inventory` is operator-facing and lists everything with its reason. `ToolSummary`
      carries selection fields only, asserted by a serialized-field-list test.
      Also closes `P3-001`'s deferred cost: `DocumentSet` supplies the documents a JARVIS-authored
      schema may compose against, and a contract test proves a supplied `$ref` resolves **and its
      constraints are enforced**, with the control that the same reference with nothing supplied is
      refused. External manifests stay self-contained deliberately (ADR-0016) — a manifest cannot
      influence which document is loaded, and `$id` shadowing is refused. 127 in-crate tests plus 7
      contract tests. ADR-0016.
      Deliberately not a placeholder: this slice holds and lists capabilities and decides nothing.
      Policy evaluation is `P3-003`; execution is `P3-005`. The registry is populated from code, not
      persisted, and no adapter is wired to it, so no tool can actually be called.
<a id="p3-003"></a>

- [x] `P3-003` Implement deterministic policy evaluation with deny-overrides, workspace grants, actor identity, channel constraints, and reason codes.
      `crates/jarvis-tools/src/evaluation.rs`: `evaluate` is a pure function over a **borrowed**
      `ToolDefinition` (no clock, no I/O, no repository), returning `Allow` / `RequireApproval` /
      `Deny` plus an `effective_risk`, a `DenyReason` code, and
      the escalation signals that raised the risk. Deny overrides is a control-flow property: the
      checks are ordered and the first refusal ends the evaluation, so no permissive finding can
      mask a later refusal. The denials precede the approval steps, so a call that is denied *and*
      would need approval reports the denial rather than sending an operator to approve something
      that cannot run. Workspace policy can
      only narrow what a tool declares. 150 in-crate tests. ADR-0017 records the ownership tension:
      `repository-layout.md` gives core "policy decisions and reason codes" while `jarvis-tools` is
      named for the policy pipeline, and the decision must read a definition holding compiled JSON
      Schema validators — so core cannot own it without either gaining a vendor SDK or inverting the
      adapter direction.
      Deliberately not a placeholder: nothing calls this yet and nothing is persisted. Recording an
      approval obligation (expiry, resume) is `P3-004`; the execution receipt is `P3-005`.
<a id="p3-004"></a>

- [x] `P3-004` Implement durable approval requests, expiry, approve/deny/cancel, authenticated approvers, and resume semantics.
      `jarvis_core::approval` (the domain) + `jarvis_storage::approval_repository` + migration
      `0006_approvals.sql` (schema v6). An approval binds to a **canonical intent digest** computed
      from the tool, its version, and a sorted-key rendering of the arguments, domain-separated so
      two parts cannot bleed into each other. A decision records its outcome, channel, instant and approver and
      enforces the expiry. Expiry is
      evaluated in Rust from `unix_nanos` and is a **read-side** fact, never a stored state and never
      a SQL comparison. One approval per intent per run (unique index), and a second decision is
      refused, so a denial cannot be overwritten. 113 core tests plus 19 in-crate storage tests, with
      one named after each category `security.md` requires (forgery, replay, expiry, mutation, stale
      state). ADR-0018; `docs/research/integrations/sha2-intent-hashing.md`.
      Deliberately not a placeholder: nothing requests or answers an approval yet, and no run parks in
      `awaiting_approval`. A caller that creates and decides one exists in tests only. `P3-005` now
      stores an approval identifier on a call row, but **does not consume an approved intent**: the
      revalidation-on-resume path that re-reads an `Approved` approval and compares its `intent()`
      digest is still unbuilt.
<a id="p3-005"></a>

- [x] `P3-005` Implement tool execution lifecycle, bounded output, idempotency ledger, audit receipt, and honest outcome states (`requested`, `submitted`, `confirmed`, `failed`, `unknown`).
      Split across three crates along the dependency direction. `jarvis_core::tool_outcome` holds the
      honest vocabulary — `ToolOutcome` with `may_have_had_an_effect`, `is_safe_to_repeat_from_outcome`,
      `is_reported`, and the strict `can_advance_to` table that refuses `requested -> submitted` — plus
      `ToolOutcomeRecord`, which cannot be built as `confirmed` without evidence or as `failed` without
      a reason, and reports "no evidence supplied" and "evidence too long" as different errors.
      `jarvis_tools::execution` holds the bounded invocation: `BoundedOutput` (32 KiB, `strict()` refuses
      and `truncating()` cuts on a char boundary), `ProviderEvidence` (256 chars) kept **separate from**
      the output column so a success-sounding sentence cannot become proof, `IdempotencyKey` (32
      lowercase hex), and `AuthorizationReceipt` whose four approval fields move together from one
      citation and whose validity is compared in Rust from `unix_nanos`. `jarvis_tools::executor` is the
      async `ToolExecutor` port, whose `AdapterError` distinguishes `RefusedBeforeReaching` from
      `AmbiguousAfterReaching` because that is what maps onto `failed` versus `unknown`.
      `jarvis_storage::tool_call_repository` + migration `0007_tool_calls.sql` (schema v7) make a call
      durable. **The idempotency ledger is the `UNIQUE (run_id, idempotency_key)` index, not a second
      table**: admission is one `INSERT ... ON CONFLICT DO NOTHING` and a suppressed insert returns the
      **existing** call id so the caller adopts it. The key is generated at admission, not derived from
      the intent, because the digest must be deterministic (an approval binds to it) while the key must
      be unique per logical call — two calls with an identical intent hash and different keys are
      asserted to be two calls. **A terminal outcome cannot be replaced** and a repeat of the same
      outcome is a no-op, which is what stops an `Unknown` becoming a `Failed` and one sent message
      becoming two. The 32 KiB bound is enforced twice (the type and the migration's `CHECK`), and the
      `CHECK` also refuses a `confirmed` row with no evidence.
      170 `jarvis-tools` tests, 125 `jarvis-core` tests, 143 in-crate `jarvis-storage` tests — including
      one named after the illegal `requested -> submitted` edge, which was added **because** a test
      author wrote it and the domain correctly refused. ADR-0019.
      Deliberately not a placeholder, and deliberately not a working tool: this is the lifecycle and the
      port. **No adapter implements `ToolExecutor` until `P3-006`** (the only implementation here is a
      test `ScriptedExecutor`), **no caller admits a tool call**, no run reaches any outcome, and
      `apps/jarvisd` has no tool pipeline — no schema validation, no policy evaluation call, no approval
      request, no execution. Nothing is written to `run_events` for a tool call either, so a call is a
      stored document rather than an entry in the durable event log. The audit receipt is stored as a
      document produced upstream; nothing here generates one. **`P3-006` added the first real adapter and
      the first consumer of this lifecycle, but still with no composition root**: no caller admits a
      call, so the lifecycle remains driven by tests. `P3-012` is where the Phase 3 gate proves the whole
      path.
<a id="p3-006"></a>

- [x] `P3-006` Add a read-only filesystem tool constrained to explicit workspace roots; test traversal, links, races, and oversized output.
      `cap_std::fs::Dir` handle per granted root and **resolves through the handle** rather than
      validating a path. The obvious join-canonicalize-prefix-check scheme is rejected because it has a
      two-syscall window — the path is resolved and then opened, so a component swapped for a link in
      between escapes — which is the "race" the requirement names. A handle resolves each component
      beneath itself, so an escape is an error rather than a file. `cap-std` was required because
      `unsafe_code = "forbid"` rules out a hand-written `openat`/`NtCreateFile` wrapper, and `rustix`'s
      `openat2` + `RESOLVE_BENEATH` is unix-only, so Windows would need its own implementation.
      `Dir::open_ambient_dir` — the grant, which has no confinement of its own — appears exactly once,
      in `WorkspaceRoots::new`. An unusable grant is refused rather than narrowed: empty, relative,
      duplicate, missing, or not-a-directory are all errors, because a skipped missing root makes an
      unmounted disk read as an empty workspace.
      `files.rs` is the first real `ToolExecutor` adapter: `jarvis.files.read` and `jarvis.files.list`,
      both `read_only`, risk 0, `Auto` approval, idempotency `Required`. A refused path is a **`Failed`
      outcome, not `RefusedBeforeReaching`** — the resolution reached the filesystem and failed, so
      "nothing happened at all" would be false; the refusal vocabulary is reserved for a malformed
      argument or a lapsed deadline, where nothing was touched. Output is read through a limited reader
      that takes one byte past the bound, so a 4 GB file is truncated rather than allocated. 179
      `jarvis-tools` tests. ADR-0020; `docs/research/integrations/cap-std-filesystem-confinement.md`.
      **Two defects were found by running the tests, not by review.** (a) A binary file was reported as
      `Confirmed` with **no content**, because `String::from_utf8` reports the valid prefix and a file
      invalid from its first byte has a valid prefix of zero — which decodes to an empty `String`, which
      *is* valid UTF-8. (b) **The truncation flag was discarded, so a cut read was recorded as
      complete**: the reader bounded at the source (correct) and then handed already-limited text to
      `BoundedOutput::truncating`, which infers truncation **by length** and reads a value of exactly
      the limit as "fits". Fixed by adding `BoundedOutput::from_bounded(content, truncated)`, because a
      source-bounded reader holds a fact the type cannot infer. The general lesson: when a type infers a
      property from a value's shape, a caller that knows the property directly must be able to state it,
      or the inference silently overrules the knowledge.
      Deliberately not a placeholder, and deliberately not wired up: **no entry point drives the tool**.
      Nothing in `apps/jarvisd` registers the two definitions, evaluates policy, requests an approval, or
      calls `execute`, so no model can read a file yet. `definitions()` returns the canonical pair but
      nothing inserts them into a `ToolRegistry` in production code — the composition step joining the
      registry, the policy engine, the approval repository, and this adapter is unwritten. The deadline
      is checked, not enforced: a read that begins in time and then blocks on a slow mount is not
      interrupted, because that needs cancellation (`P3-011`), which also owns the resource limits that
      belong under this blocking I/O. No write, move, delete, or trash/undo — the read half was chosen
      first because its failure mode is disclosure, which confinement prevents, rather than destruction,
      which needs an undo design. `P3-012` is where the gate proves the path end to end.
<a id="p3-006a"></a>

- [x] `P3-006a` Close the authorization seam: make an authorization receipt derive from the policy decision it came from.
      **Added out of ledger order, because composing the pieces found a security defect.** `P3-001`..`P3-006`
      each built one part and **nothing composed them**: the only `AuthorizationReceipt` constructions in the
      workspace were six test fixtures, so no adapter had ever been handed a receipt from a real decision.
      Closing that gap to write the first seam test showed it was a security gap, not only an integration one.
      `AuthorizationReceiptParts` took the risk as a **value**, so a composer could evaluate a decision at
      `High` and then build a receipt declaring `Risk::Minimal` — and nothing would refuse it, because nothing
      held the decision and the receipt at once. The receipt is what an adapter treats as permission, so the
      declaration an adapter acted on could understate the risk the decision was taken at. The digest had the
      same shape of problem: `intent_hash: String` cannot express "this is the digest an approval binds to".
      Fixed by making both **derived rather than stated**: the parts now carry the `PolicyDecision` and the
      `arguments`, and `AuthorizationReceipt::new` reads `effective_risk()` from the decision and
      **recomputes** `CanonicalIntentHash::compute` to refuse a mismatch. Two new refusals make the remaining
      ways back in unrepresentable: `DecisionNotAuthorizing` (a `Deny`, or a `RequireApproval` citing no
      approval, can never produce a receipt) and `IntentMismatch`. The digest stays pre-computed at the call
      site because `jarvis-tools` does not depend on `sha2` and a second implementation of the canonical form
      would get the `\u{1f}` separator wrong — so it is verified at the boundary instead. 182 `jarvis-tools`
      tests; the seam has a named test with controls proving each refusal is about the intended cause. ADR-0021.
      **No production code builds a receipt**, so this is a library fix, not a pipeline: there is still no
      composition root (no registry lookup, no `evaluate` call, no approval round-trip, no admission, no
      adapter call) and the seam is correct and tested but **not driven**. `P3-012` owns that. Also recorded:
      the digest covers tool, version, and arguments but **not the actor or workspace**, so two actors with the
      same tool and arguments currently produce the same digest — a question `P3-012` should answer.
<a id="p3-006b"></a>

- [x] `P3-006b` Bind a tool execution request to the receipt it carries.
      Found by deliberately hunting the `P3-006a` defect class in the next seam. `ToolExecutionRequest::new`
      checked only that the receipt had not expired; it never compared the request's `tool`, `tool_version`,
      or `arguments` against what the receipt authorized. An adapter cannot tell an authorized call from an
      unauthorized one, so that constructor is the last place the binding can be enforced — the
      `NotImplemented` path only catches a tool an adapter happens not to implement. Fixed with
      `ToolNotAuthorized` (tool AND version as a pair) and `ArgumentsNotAuthorized`, which **recomputes**
      `CanonicalIntentHash` rather than comparing JSON, so key order is not significant. **The evidence it
      was live rather than theoretical: five existing tests started failing**, because they built requests
      whose arguments did not match their receipt's digest — one asked for an extra `subject` key the
      receipt never covered, and had been passing.
<a id="p3-006c"></a>

- [x] `P3-006c` Fix the cancellation contract: operator intent cannot be stale.
      Surfaced from the P3-006b work as an intermittent test failure, which turned out to be a **real
      user-facing defect in existing code**. `POST /api/v1/runs/{id}/cancel` required `expected_version`,
      justified in `acceptance-tests.md` as "a client cannot cancel a run it has not read". That is wrong as
      a control: the executor advances a running run's version at **every state-machine step**, so a
      client's version is stale almost immediately and the cancel was refused with a `409` the client could
      not resolve — the user asked to stop a run and was told the run had changed. `RunService::cancel` made
      it worse by **re-reading the run and then discarding what it read**. `ADR-0013` already establishes
      that a cancellation request is operator intent which "already outranks the interruption", and `COALESCE`
      already made a repeat idempotent, so the version guard had no safety role left. Now: no version; the
      guard is the one that cannot go stale (**a settled run refuses**, because there is no work left to
      stop); zero-rows-affected is resolved by reading, so `RunNotFound` and `TerminalStateImmutable` stay
      distinguishable; `CancelRunRequest` is **deleted** from the protocol rather than left with a permissive
      decoder, and the route takes no body. The flakiness is gone **structurally** — verified by 12
      consecutive passes — because there is no read-then-write window left. ADR-0022.
      Honest limits: `expected_version` remains on run start and on approval decisions, and neither has been
      re-examined against the same reasoning. No `jarvis cancel` CLI verb exists, so the endpoint is
      exercised only by the gateway test and the e2e gate. Whether a cancellation *request* should emit a
      `run_events` row is not answered here.
      **CORRECTION (2026-09-22): the `expected_version` half of that limit was FALSE.** Neither case exists
      — `ApprovalDecision` has no version field and never did, and starting a run creates a row so there is
      no prior version to expect. A grep for `expected_version` across the workspace now finds only ADR-0022,
      a doc quotation of it, and the message of the test that replaced the removed cancel field. Three
      documents asserted the field as live (`ADR-0022`, `docs/api/contracts.md`, and this entry) while **no
      code, test, or protocol type ever had it** — a plausible-sounding claim written as a *deferral*, which
      made it read as a live risk and survive three slices of review. Corrected in all three, with the ADR
      struck rather than quietly edited. The general rule the ADR established generalises and is now recorded
      there: **where a durable identity already names the subject of a decision — a stable `id` plus a state
      machine that refuses a second decision — a version expectation adds no safety and can only add a
      refusal.** Still true: no `jarvis cancel` verb (the verbs are `status`, `health`, `ask`, `chat`,
      `logs`, `doctor`), and whether a cancellation *request* should emit a `run_events` row remains
      unanswered.
<a id="p3-006d"></a>

- [x] `P3-006d` Compose the tool pipeline in the daemon and make it reachable over HTTP.
      **The first production caller of the whole tool path.** `P3-001`..`P3-006c` each built one layer and
      **nothing composed them**, so no route reached a tool adapter and every seam was unverified — and the
      three preceding slices found a real defect in every seam they examined. This closes that: route →
      handler → registry → schema → policy → receipt → admit → authorize → submit → adapter → recorded
      outcome. Composed in `apps/jarvisd`, because `repository-layout.md` forbids an arrow from
      core/application into an adapter. ADR-0023.
      The pipeline registers the **adapter's own** `definitions()` rather than restating schemas and risk
      levels, because policy deciding about a declared copy would be policy deciding about a tool other than
      the one being run — the `P3-006a` defect class one level up. The route accepts only `run_id` and
      `arguments`: the **workspace comes from the stored run's row** and the scope from the daemon's actor
      constructor, never from the request, because `identity-and-workspaces.md` requires access to follow
      from authentication rather than from a client-supplied identifier. No roots means **no pipeline**, not
      an empty one, which would advertise a tool that fails every call. An unusable grant fails **startup**,
      per ADR-0020's rule that a grant is refused rather than narrowed.
      `call_tool` at 117 lines was split into `validate`, `authorize_and_admit`, and `execute_and_record`,
      with `authorize_and_admit` returning `Option<PreparedCall>` so that "held" is a value rather than a
      hidden early return, and `PreparedCall` grouping the values that must agree.
      Four route-level tests, two of them **falsification** tests: a tool call cannot name its own
      workspace (positive control first — the granted root really is read — then `422` for a named
      workspace, `404` for an unknown run, and a traversal asserted to produce `state: "failed"` with
      `output: null`), and a call with no composed pipeline is `404` naming the config key. **One assertion
      was wrong and the error was instructive**: it asserted "a traversal must not be 2xx" and got `200`,
      because ADR-0020 puts a refused path in the *outcome* rather than in a transport error. The assertion
      was conflating HTTP success with "the read happened" — exactly the confusion `P3-005` exists to
      remove — so it now asserts on the outcome and on the absence of content.
      **Honest limits.** **There is no approval round-trip**: a held decision returns `202` with `call_id`
      and the reason and then nothing happens — no `ApprovalRequest` is persisted, no route lets a
      human decide, and nothing resumes the call, so the row stays truthfully `requested` forever. That is
      a gap, not a design. **No `run_events` row is written for a tool call** (`P3-012` owns it), so a call
      is absent from any stream a client is watching. **`policy_version` is a label, not a verifiable
      version** — the handler passes the literal `"policy-1"` and nothing checks it, so a receipt citing it
      proves which string was supplied rather than which rules were applied. The route takes its workspace from the run but **does not
      check that the run is the caller's**, because this transport has one local identity; that becomes a
      real question when a second identity exists. No CLI verb drives the route, so it is exercised by the
      gateway tests and no end-to-end process gate yet.
<a id="p3-007"></a>

- [x] `P3-007` Research the current MCP specification and selected Rust SDK; record negotiated versions and features.
      **The research found the previous record was substantially WRONG about the current protocol**, so this
      slice's value was mostly in what it corrected. `docs/research/integrations/mcp.md` (re-verified
      `2026-09-22` against the live `llms.txt`, changelog, versioning page, Streamable HTTP page, and the
      official Rust SDK source) replaces an architecture snapshot that assumed the `initialize` handshake.
      **Revision `2026-07-28` is a stateless rewrite**: the `initialize`/`notifications/initialized`
      handshake is **gone**, the `Mcp-Session-Id` session is **gone**, and every request now carries its
      protocol version, capabilities, and identity in `_meta`
      (`io.modelcontextprotocol/protocolVersion`, `.../clientCapabilities`, `.../clientInfo`). `server/discover`
      is **mandatory for servers**. Also removed: the Streamable HTTP GET endpoint, SSE resumability
      (`Last-Event-ID` + event ids), `ping`, `logging/setLevel`, `notifications/roots/list_changed`, and
      `resources/subscribe`/`unsubscribe`. New: `subscriptions/listen` for change notifications, and
      **multi round-trip requests** replace server-initiated JSON-RPC requests (a server returns
      `InputRequiredResult` with `inputRequests`; the client retries the original request with
      `inputResponses`). Every result now carries a required `resultType`. Error codes were renumbered
      (`UnsupportedProtocolVersion` `-32004` → `-32022`; resource-not-found `-32002` → `-32602`) and the
      server-error range was partitioned so `-32020`..`-32099` is spec-reserved.
      **Selected: protocol `2026-07-28`, SDK `rmcp` 3.4.0** (published `2026-09-15`, `Apache-2.0`, MSRV
      `1.88`, edition 2024 — this workspace is on `1.98.1`/edition 2024, and `Apache-2.0` is already in
      `deny.toml`). The Rust SDK is now **Tier 1** (100% conformance pass rate, features before a spec
      release, triage in two business days, relegation if any conformance test fails four weeks running),
      which is a concrete commitment rather than an aspiration. **A trap worth recording: the SDK's
      `ProtocolVersion::LATEST` is `V_2025_11_25`, NOT the current revision** — so relying on it would
      silently negotiate the deprecated handshake while appearing to target the new spec. Both the client
      and the server must name `V_2026_07_28` explicitly.
      **Decisions, each with a reason recorded:** JARVIS is a **modern client** (`ClientLifecycleMode::Discover`,
      not `serve()`, which is the legacy handshake) and a **dual-era server**, because the spec's own
      compatibility matrix states a legacy client has **no fall-forward mechanism** — serving modern-only
      would break older clients silently for no gain. `Auto` is the fallback for an unknown-era peer, and the
      SDK treats only a **correlated, non-modern JSON-RPC error** as evidence of a legacy peer; a transport
      error stays a hard error rather than a silent fallback. **Roots, Sampling, and Logging are deprecated**
      (twelve-month removal window) and are deliberately **not adopted** — their spec-named migrations (pass
      directories via parameters/configuration, call the provider directly, log to `stderr`/OpenTelemetry) are
      already JARVIS's shape. Tasks moved out of core to an extension and is **not adopted**; long-running work
      is a JARVIS run. `x-mcp-header` is recorded as an adapter-only wire hint that must never leak into a
      `ToolDefinition`, and the spec's strict reachability rules (primitives only, `properties`-chain only, no
      `$ref`/`items`/`oneOf`) mean an invalid annotation **excludes that one tool** from `tools/list` rather
      than failing the list.
      **Honest limits.** This slice produced **no code and no dependency** — `rmcp` is not in the workspace
      yet, so nothing here is proven by a test; the verification plan names ten falsifying tests, none of which
      are written. Six unresolved questions are recorded rather than answered, including whether the
      dual-era server requirement conflicts with the daemon's single-transport shape (`P3-009`), whether the
      full `transport-streamable-http-server` feature set pulls something `cargo deny` refuses (unmeasured),
      and whether `rmcp`'s `auth` machinery can be confined to the adapter. **No `llms.txt` exists for the
      docs separately** — one index covers both, which is recorded as `not found` rather than glossed.
<a id="p3-008a"></a>

- [x] `P3-008a` Build the MCP translation layer: server identity, canonical tool naming, and schema conformance.
      **Added out of ledger order, because `P3-008`'s first real decision is not a transport decision.** New
      crate `crates/jarvis-mcp` (depends only on `jarvis-core` + `jarvis-tools`). ADR-0024. Three rules, each
      about *authority* rather than bytes, so each is settled once for every MCP server that will ever be
      configured. **33 tests; 725 workspace tests.**
      **1. A server does not name itself.** `ServerName` is an **operator-chosen** local name, validated as
      narrowly as a tool-name segment (lowercase ASCII, digits, `-`, `_`), with dots refused because
      `mcp.a.b` would otherwise be ambiguous between "server `a.b`" and "server `a`". The server's own claim
      is `ReportedIdentity` — bounded, kept as **evidence**, never an identifier — because the specification
      says the server name "is not guaranteed to be unique across servers and **SHOULD NOT** be relied upon
      for disambiguation". It is a self-assertion by the party being identified, and the **namespace prefix
      is load-bearing**: `mcp.` is what classifies a definition `ToolSource::Mcp`, i.e. third-party, so
      letting a server choose it would let a hostile server claim `jarvis.files` and be classified native.
      **2. Two MCP tool names never become one identifier.** MCP names may legally contain a **dot** and
      uppercase; JARVIS names are lowercase-only and forbid a dot in the name half. Every lossy fix merges
      two tools, and the dangerous direction is that a call for one tool would then run the other *with the
      other's declared risk, effects, and scopes*. So: use the name unchanged when a segment can hold it
      **exactly**, otherwise **hash the whole name** (SHA-256, `h` + 8 hex, length-prefixed input) — never
      truncate, because a shortened name *is* a different name and can collide with a real one, whereas a
      digest is obviously not a name. `Prefixed` (`mcp.github.list_issues`), `Bare` (`mcp.list_issues`, which
      collides across servers by design), and `Hashed` differ in **namespace**, not by mangling the tool
      name. `CanonicalToolName` keeps the **server's** name alongside the canonical id, because `tools/call`
      must send the server its own name back — sending the canonical id would be the translation applied
      twice, invisible until a real server rejects a call.
      **3. A server-supplied schema is untrusted input.** `$ref` is refused wherever it appears (this
      revision loosened schemas to any 2020-12 keyword, so that refusal is **more** load-bearing, not less —
      resolving one means fetching a URL a server chose). All four `x-mcp-header` constraints are enforced:
      primitives only (**`number` explicitly forbidden**; an integer bound must sit inside the JS-safe range
      ±2^53−1, and an *exclusive* bound of 2^53 is accepted because a bound is what must fit), reachable only
      through `properties` keys (not `items`/`oneOf`/`anyOf`/`allOf`/`if`/`then`/`else`/`$ref`/wildcards),
      unique case-insensitively, and a valid HTTP field name. The **reachability rule is the interesting one**
      and is not arbitrary: the spec defines extraction as reading the value at "the exact property path",
      which has a single answer only when the path is unique — under `oneOf` there are two answers and under
      `items` the path needs an array index a header cannot carry. A failing tool is **excluded by itself**,
      per the spec's explicit requirement that one malformed definition not remove the others.
      **THE DESIGN BUG MY OWN TESTS FOUND.** `NameAssignments` first keyed the collision check on the tool
      name alone, so it **refused a legitimate `tools/list` refresh** while **silently permitting two
      different servers to share one identifier** — a collision check that failed in both directions. The
      keys must be `(server, tool)`: same server + same name = refresh (accept); different server + same name
      = the collision `Bare` causes (refuse). Two of the four failing tests were that bug, and the other two
      were my own wrong expectations about the identifier shape (I had assumed the prefix belonged in the
      *name* half, which would give that half a dot and is not even expressible).
      **Honest limits.** **Nothing has been exercised against a real MCP server** — these are pure functions
      tested as pure functions, and "the translation is correct" and "a real server's tool list translates"
      are different claims; only the first is proven, and the field names used (`serverInfo`,
      `x-mcp-header`, `inputSchema`) come from the recorded specification rather than from bytes. **`rmcp` is
      not a dependency yet**, deliberately (these rules are about JARVIS identifiers), so nothing is checked
      against a real payload. **The hash is a disambiguator, not a security boundary** — 8 hex characters,
      and an attacker who can grind a colliding digest can forge one; the authority is the stored canonical
      id, not the hash. **No operator surface for the naming strategy exists**, so `Bare` cannot currently be
      selected — and when it can be, a collision refuses the *second* server's tool at translation time
      rather than at configuration time, which is later than an operator would want to learn it.
      `ReportedIdentity::agrees_with` has a test and **no caller**, because the host adapter that would use
      it is `P3-008`. The conformance module checks the *annotation* rules, not the schema's validity — a
      schema with no annotation gets no structural opinion at all, which is correct for this layer and worth
      not mistaking for validation.
<a id="p3-008b"></a>

- [x] `P3-008b` Translate a server's tool listing into canonical definitions, with the effects decided by an operator.
      The second half of `P3-008a`: that slice settled what an MCP tool is *called*, this one settles **what it may
      do and at what risk**. New module `crates/jarvis-mcp/src/definition.rs`. ADR-0025. **54 crate tests; 746
      workspace tests.**
      **Effects, risk, and approval come from an operator, never from the server.** An MCP server supplies
      `name`/`title`/`description`/`inputSchema` and an optional `annotations` object — and that object carries
      `readOnlyHint`/`destructiveHint`/`idempotentHint`, which are precisely the three facts policy needs. The
      specification warns of it: clients "**MUST** consider tool annotations to be untrusted unless they come
      from trusted servers". `McpToolListing` has **no field for annotations**, so the translation cannot consult
      them even by accident, and a test asserts that smuggling them into the input schema changes nothing. **The
      incentive is what makes this decisive**: `EffectSet::risk_floor` is a MAXIMUM, so a server that admits one
      outward effect lands at risk 2 → `Ask` and can never be silently auto-approved, while a server that
      under-reports has no such problem. The asymmetry runs opposite to the server's incentive.
      **An unclassified server fails closed.** `ToolEffectPolicy::unclassified()` declares
      `ExternalCommunication` + `Write` (floor 2) and then risk **3** with `ApprovalPolicy::Ask` — which
      workspace policy cannot lower the way `Policy` could — plus no automatic retry and
      `Idempotency::Unsupported`. A read-only server is mildly inconvenienced by a prompt; a mass-mail server
      presumed harmless is not inconvenienced at all. `read_only()` is the one convenience constructor.
      **Effects are an operator statement, not a schema inference.** "It takes a `query` string, so it reads" is
      inference presented as knowledge, and this project does not persist unsupported inference as fact.
      `ToolEffectPolicy::new` refuses a risk below the effects' floor and a retry that would repeat an
      un-deduplicated outward effect — **at configuration time**, so a policy that would send a payment twice
      cannot be *held*, let alone used.
      **The dialect rule composes two documents that disagree.** MCP says a schema with no `$schema` "defaults
      to 2020-12"; `jarvis-tools` **requires** the keyword and refuses to default it (because
      `exclusiveMinimum` is a boolean in earlier drafts and a number in 2020-12, so defaulting silently
      reinterprets an older-draft document). Both are right: a server's **omission** is supplied with 2020-12,
      and a server's **declaration of a different dialect** is refused rather than reinterpreted. The refusal
      names both dialects, or an operator cannot act on it.
      **Server text is sanitised before it reaches a model.** Whitespace collapses, control characters are
      dropped, and **bidirectional / zero-width characters are removed** — not typographic tidiness: a bidi
      override changes how text *reads* without changing what it *is*, which in the field that decides whether
      a tool gets called is a deception primitive. A missing description says the server supplied none rather
      than inventing a plausible one.
      `translate_listing` excludes per tool for names as well as schemas, and a collision is an **exclusion,
      never a rename** — any rename is a mapping the operator did not choose. The derived `version` is
      `schema-<8 hex>` over the **input schema** (not the whole listing), so a cosmetic description edit does
      not invalidate a stored intent while a schema change does; it is a digest, not a counter, because a
      version must survive a restart. Clippy's `too_many_arguments` (8/7) was **right, not noise**: `risk` and
      `timeout_seconds` are two adjacent numbers a caller could transpose silently, so the inputs were grouped
      into `ToolEffectPolicyParts` / `ToolPosture` / `ToolExecutionLimits` — the same reasoning `P3-005` applied
      to its call parts.
      **Honest limits.** **No configuration surface exists**, so `ToolEffectPolicy` cannot be built from
      `config.toml` and in practice every MCP server would get `unclassified` — the safe direction, but it means
      `read_only()` has **no caller outside tests** and the operator story is incomplete. Still **nothing
      exercised against a real MCP server**. The scope vocabulary is **one coarse `mcp.call`** — an MCP server is
      treated as one trust unit because no grant model can yet express more. A tool with no declared output
      schema gets a **permissive** one, so an unvalidated result reaches the model and the protection is the
      output-size bound, not schema validation. **The version excludes the tool name**, so two servers'
      identically-schemaed tools share a version string. **`Idempotency` is never `Required`/`ProviderKey`**
      because MCP cannot express provider-side dedup, so an outward MCP call can never retry automatically —
      safe, but a transient network failure is always a failure. **`translate_listing` builds its own
      `NameAssignments`**, so it cannot detect a collision between two different servers; that is the
      aggregate's job, and the test drives the cross-server case directly rather than pretending one call sees
      two servers.
<a id="p3-008c"></a>

- [x] `P3-008c` Aggregate several servers into one catalog, and refuse to serve an ambiguous one.
      **Closes a limit recorded by `P3-008a`/`P3-008b` rather than deferring it again**: a per-listing
      translation builds its own assignment set, so it *structurally* could not see a collision between two
      servers. New module `crates/jarvis-mcp/src/catalog.rs`. **67 crate tests; 759 workspace tests.**
      `McpCatalog` owns **one** `NameAssignments` across every server, so a collision is detected exactly
      where the information exists. `McpCatalog::build` is the first function in this crate that can answer
      "what happens when two servers offer the same name", and the answer is the interesting part:
      **a cross-server collision is reported, not resolved.** Dropping one side would mean the set of tools a
      model can call is a function of **configuration order** — the spec requires each server's own
      `tools/list` to be deterministic but says nothing about the order a client iterates *servers*, and since
      a collision is refused rather than renamed, the loser is **absent from discovery entirely**. A catalog
      whose contents depend on an ordering nobody declared meaningful is worse than one that refuses, so
      `has_cross_server_collision()` is the signal to **not serve**. A test drives both orderings and asserts
      the collision is reported either way, which is what makes refusing the right answer rather than an
      arbitrary winner.
      **Collisions are their own list, not a flag derived from message text.** The first version asked
      `exclusions.iter().any(|e| e.reason.contains("both translate to the identifier"))` — matching on a
      `Display` string, so a reworded error would have **silently stopped the catalog refusing**. The category
      is data (`collisions()`), not prose. Same family as the "two values that must agree" defects: a safety
      property that depends on a string a message happens to contain is a safety property nobody is holding.
      Other decisions: a **truncation is reported separately from an exclusion** (the causes and remedies
      differ — an exclusion is about that tool, a truncation is about the *size* of the configuration, and an
      operator fixing one tool would learn nothing); a server with no listing is **reported as an exclusion**
      so its absence does not look like a server that was never configured; a server configured twice is
      **refused**, because the second translation would look like an idempotent refresh, which is the case
      `NameAssignments` deliberately accepts; entries are sorted by identifier so a catalog built twice is
      equal; `MAX_MCP_SERVERS` is 16 so a config file cannot make startup unbounded; and the registry bound is
      enforced **here** rather than discovered at registration, because a catalog that silently exceeded it
      would fail later in a different component with an error naming the registry rather than the
      configuration that caused it. `route()` returns `None` for an unknown identifier, so a lookup cannot
      default to some other server.
      **Honest limits.** Still **no configuration surface**: `McpCatalog::build` takes servers and listings as
      arguments, so nothing constructs one from `config.toml` and `MAX_MCP_SERVERS`/the strategy cannot be
      set by an operator. Still **nothing exercised against a real MCP server**. The catalog holds
      **routing, not authority** — it carries definitions and server names, and a caller still passes them
      through the pipeline. `has_cross_server_collision` is a method, and **nothing calls it yet** — the
      daemon that would refuse to serve is `P3-009`.
      **CORRECTION (2026-09-22): this entry's first line and last sentence were both FALSE when written.**
      The commit message for this slice claimed it "closes two limits ... including `ReportedIdentity::agrees_with`
      having a test and no caller" — a grep at `1ecbb8e` showed `agrees_with` appeared only in `server.rs`
      (definition plus a test) and the `lib.rs` re-export, i.e. **still no production caller**. It also claimed a
      "duplicate entry in `listings`" is **resolved by first-match rather than refused**; that limit is **also
      false now**, because duplicates are refused (`CatalogError::DuplicateListing`) — but the claim was written
      as a *deferral to the config layer*, so it read as a live gap and made the slice look honest while the
      code it described was changing underneath it. This is the **second** time this session that a
      plausible-sounding recorded claim was false (`expected_version` was the first, at `P3-006c`), and the
      pattern is the same: **a commit message or ADR reads to the next reader as verified evidence**, so an
      overclaim propagates exactly like a wrong external doc — the thing this repository's external-research
      rule exists to prevent, arriving through the door marked "our own notes". The rule that follows is
      recorded in `docs/development/definition-of-done.md`: **before building on a recorded claim, check the
      claim; before writing one, check the code.**
      The work that made the first half true, and closed the second, is the identity-drift slice below.
<a id="p3-008d"></a>

- [x] `P3-008d` Report a server whose self-description changed, and refuse two listings for one server.
      **This is the slice that makes `P3-008c`'s commit message true a day late**, and it is written as its own
      entry rather than folded into `P3-008c` so that the overclaim stays legible. `ReportedIdentity::agrees_with`
      now has a **non-test caller** — verified by grep, not by recollection: it appears at exactly one non-test
      site, `observe_identity`, which is reached from the public `McpCatalog::build`. That is a narrower claim
      than "it is used in production" and deliberately so, because the wider one would be false: see the limits
      below. The first-match gap is closed by a refusal.
      `ServerListing` gives `McpCatalog::build` the server's **`ReportedIdentity`** alongside its tools, and
      `build` takes `seen_before: &[ObservedServer]`. Where a server's self-report **disagrees with a previous
      observation**, the difference is recorded as an `IdentityDrift { server, before, after }` and exposed via
      `drifts()`. **The drift is deliberately not fatal**: a vendor legitimately renames a product, and turning
      that into a refusal would make an ordinary upgrade an outage. The point is visibility, exactly as with
      `P3-008a`'s decision that a server does not name itself — an operator classified *a server by name*, and if
      the process behind the name changed, the posture they chose applies to something they may never have seen.
      Nothing else in the protocol records that, because the server's own name is explicitly not an identity, so
      this is the only place the fact can be observed. `agrees_with` is the right predicate for it because it is
      **deliberately tolerant of a changed `title` and only strict about a changed `name`** — the name is what
      the operator's decision is bound to.
      **A duplicate listing is now refused, not first-matched.** Two listings for one server are two
      *observations of one thing*, and which one wins would depend on argument order — the same
      order-dependence the collision rule refuses one level down, and the same reason a server configured twice
      is refused. `CatalogError::DuplicateListing { server }` names the server rather than the index, because an
      operator acts on a config file, not on a `Vec` position.
      Four tests, each aimed at a way the check could be wrong rather than a way it could pass: a **first build
      has nothing to compare** and must report no drift (or the check would be vacuous); a **changed** report
      produces exactly one drift naming before and after; an **unchanged** report produces none (the positive
      control — without it, a check that fired on every refresh would be ignored and therefore absent); and a
      drift **does not disturb routing** (it is about the report, not the tools) and **does not stop the catalog
      being built**. A server present in `seen_before` but absent now is **not** a drift — it simply was not
      listed, and that is already reported as an exclusion.
      **Honest limits.** The drift is reported to whoever calls `build`; **nothing calls it with a prior
      observation yet**, so in practice every build is a first build until `P3-009` owns the process that
      persists an observation. `ObservedServer` is not stored anywhere, so a daemon restart loses the baseline
      and a drift across a restart is invisible. A drift records **that** the report changed and the two texts;
      it does not classify *how* (rename vs. a different server that reuses a name), because the protocol
      supplies nothing that could distinguish them — which is precisely why it is a report and not a refusal.
      Clippy's `too_many_lines` (122/100) on `build` was **right**: the function had grown a second
      responsibility, so it was split into `admit_configuration`, `observe_identity`, and `finalize` — each one
      now states a single decision, and `admit_configuration` is where the three "refuse before translating"
      checks can be read together.
<a id="p3-008e"></a>

- [x] `P3-008e` Adopt the MCP SDK and carry a real negotiation: `server/discover`, not the handshake.
      **The first slice where JARVIS speaks the MCP wire**, and the first time `rmcp` is a dependency.
      New crate `crates/jarvis-mcp-transport` (depends on `jarvis-mcp` + `rmcp` + `tokio`). **19 crate
      tests; 783 workspace tests.**
      **Split into two crates deliberately.** `jarvis-mcp` stays pure, offline, and free of the SDK, so
      the authority rules a security reviewer must read are still testable without a peer standing and
      without any third-party type in scope — and so the transport can be reimplemented against the
      wire without any naming, posture, or conformance rule moving. `AGENTS.md`'s "provider SDK types
      must not cross JARVIS domain boundaries" is what forces this: a *transport* is where an SDK
      belongs, and an authority decision is where it does not.
      **The SDK's own README contradicts its own source, and the source is the contract.** Its README
      says `ProtocolVersion::LATEST` is "newest stable version this SDK defaults to" and that the SDK
      "implements the stable `2026-07-28` specification"; `LATEST` is **`V_2025_11_25`**, the *legacy*
      era, and the removal of `initialize` is precisely what makes `2026-07-28` modern. So the trap
      `P3-007` recorded is now an **executable fact**: `revision.rs` asserts `LATEST` is *not* the
      modern revision, and every connection names `V_2026_07_28` explicitly. An SDK bump that changes
      this produces a failing test and a decision, not a silent change of era.
      **`Discover`, never `Auto`.** `Auto` falls back to the legacy handshake when the peer does not
      answer `server/discover` within ten seconds (its constant is `DEFAULT_AUTO_DISCOVER_TIMEOUT`),
      which would negotiate a different era while looking healthy — and nothing in this crate
      implements the legacy era. `Discover` has no fallback path, verified by reading the SDK's
      `serve_client_with_lifecycle`: the timeout is passed in the `Auto` arm **only**.
      **That reading found a real defect in our own first version.** The same verification showed
      `Discover` has *no deadline at all*, so a peer that never replies left the client waiting
      forever — a daemon starting against a dead or legacy server would hang at startup rather than
      report. Found by a test that asserted a refusal and instead **hung**, which is why that test now
      asserts a bounded `DiscoveryTimedOut` naming the deadline and pointing at the protocol era as
      the likely cause.
      **A server's `annotations` are structurally unreachable.** `Tool` carries `readOnlyHint`,
      `destructiveHint`, and `idempotentHint`, and a test sends a tool that declares itself read-only
      and idempotent — exactly what a server would assert to get an effect auto-approved. The hints are
      dropped **by construction**: `McpToolListing` has five fields and none is an annotation, so the
      translation cannot consult them even by accident. A comment saying "do not read these" is a
      convention; a type with nowhere to put them is a guarantee.
      **The tests are driven against the wire, not against the SDK's server.** `tests/support` is a
      hand-written scripted peer that reads and writes newline-delimited JSON-RPC itself — the framing
      `rmcp` uses, verified in its `transport/async_rw.rs` (`read_until(b'\n', ..)` and
      `put_u8(b'\n')`). Using the SDK's own server would have been less code and worthless as
      evidence: a client and server from the same library share their assumptions, so a disagreement
      with the *specification* would pass. The peer also **refuses to answer a method it was not
      scripted for**, so a client calling the wrong method cannot appear to negotiate.
      What the tests prove, each aimed at a way the code could be wrong: `server/discover` is sent and
      `initialize`/`notifications/initialized` **never** are; a silent peer yields a bounded refusal
      rather than a connection or a hang; a server offering only `2025-11-25` is refused by name; a
      tool list arrives over the wire and translates (with a missing title/description staying absent
      rather than invented); a server claiming its tool is safe gets no say; the operator's name and
      the server's reported identity stay separate; a modern server that reports **no identity** still
      connects (the SDK's own source notes `server/discover` responses are "not required to provide
      it"), which matters because `P3-008d` must be able to see a server that *stops* naming itself; a
      discovery request actually carries its per-request metadata, which is the stateless contract's
      core requirement; and a JSON-RPC error on `tools/list` is a `PeerError` rather than
      `Unavailable`, because the two have different remedies.
      **Measured, not assumed: the SDK is admissible.** `cargo deny check` reports advisories, bans,
      licenses, and sources **all ok** with `rmcp` in the graph, which answers unresolved question 5 in
      `docs/research/integrations/mcp.md` (recorded there as "not yet measured"). The feature set is
      the narrowest that serves a client — `client`, `transport-child-process`,
      `transport-streamable-http-client-reqwest` — and the graph gained **no duplicate** of any crate
      the workspace already pins (`tokio`, `reqwest`, `serde`, `serde_json`, `tracing` all resolve to
      the same version, and `process-wrap` is genuinely new). `server` is off because the daemon is a
      client until `P3-009`; `auth` is off because JARVIS authorization is not MCP authorization and
      the OAuth machinery would be a second, unowned token lifecycle.
      **Honest limits.** **Still nothing exercised against a real MCP server.** A scripted peer proves
      the client agrees with the documented framing; it cannot prove a third-party server behaves.
      `connect_stdio` and `connect_http` are **unexercised** — every test drives `connect_over` over an
      in-process duplex pair, so the child-process spawn and the HTTP transport have no test at all,
      and `StdioCommand` being spawned is the one path where a real OS error could appear. **No
      `tools/call`**: this slice lists tools and stops, so the transport cannot yet run anything, and
      the MRTR round-driving the SDK offers is unused. **No connection is pooled or reused**, and
      nothing reconnects — `P3-009` owns the daemon that would. **Nothing calls any of this**: there is
      still no configuration surface for MCP servers, so the crate is a capability a caller can use and
      not one an operator can reach — the same limit `P3-008a`..`P3-008d` each recorded. The
      `resources` and `prompts` capabilities are not modelled because nothing consumes them, and the
      SDK's automatic response caching is left at its default rather than deliberately configured.
<a id="p3-008"></a>

- [ ] `P3-008` Implement MCP client/host adapters for stdio and Streamable HTTP behind canonical tools.
      **Most of this is delivered, split across five slices that each answered one question; what remains
      is named at the end of this entry rather than being folded into a checkmark.**
      - `P3-008a` settled **what a tool is called** (`jarvis-mcp`: `ServerName`, `ReportedIdentity`,
        `canonical_tool_name`, `NamingStrategy`, `NameAssignments`, schema conformance). ADR-0024.
      - `P3-008b` settled **what a tool may do** (`ToolEffectPolicy`, the translation, per-tool
        exclusion). ADR-0025.
      - `P3-008c`/`P3-008d` settled **what happens when servers collide or a server changes its
        self-description** (`McpCatalog`, `IdentityDrift`, `CatalogError::DuplicateListing`).
      - `P3-008e` settled **the wire** (`jarvis-mcp-transport`: `rmcp` 3.4.0, `Discover` never `Auto`,
        a bounded negotiation, `tools/list`). ADR research in `docs/research/integrations/mcp.md`.
      - `P3-008f` settled **the join** and closed the two limits the five above each recorded. ADR-0026.
      - `P3-008g` settled **the remote endpoint policy** and closed the `connect_http` limit. ADR-0027.
      - `P3-008h` made a server's tool a real `ToolExecutor` (the outcome mapping). ADR-0028.
      - `P3-008i` added the **host role**: the `[mcp]` configuration surface, and the first caller of the
        catalog's collision check. ADR-0029.
      - `P3-008j` put the host role **in the daemon's startup path**: `mcp-servers.toml`, the dispatch table,
        and the two-scope actor. ADR-0030. **This is the slice that makes an MCP server an operator-reachable
        tool**, which is the limit all ten entries above recorded.
      **`build_catalog` is the composition that was missing.** `jarvis-mcp` could translate a listing
      nobody had fetched and `jarvis-mcp-transport` could fetch a listing nobody had translated, and
      **nothing joined them** — so every slice recorded the same honest limit, "a capability a caller can
      use, not one an operator can reach". `HostedServer { configured, connection }` carries the
      operator's name and posture with the live connection in one value, so a policy cannot be paired
      with a different server's tools; `ServerListing`'s reported identity is read from the
      **connection** rather than from the tool result, because `P3-008d`'s drift check compares that
      value against a previous observation and a host that defaulted it would make the check compare a
      value with itself. A server that cannot be read does not fail the build — one flaky third-party
      process must not empty the model's tool list — and its reason is kept in `HostBuild::unreadable`,
      which distinguishes "unreachable" from "refused" from "declared no tools capability" rather than
      letting all three look like the catalog's generic "offered no listing". A server that declared no
      `tools` capability is **not asked**, and a test asserts the request was never sent.
      **`tools/call` now exists**, which `P3-008e` recorded as absent. It uses the SDK's **single-round**
      request deliberately: the high-level helper drives MRTR rounds through a client `ClientHandler`,
      and JARVIS registers none because the only human answer comes through JARVIS's own approval path.
      So the modes JARVIS cannot serve are **refused by name** (`CallError::InputRequired`,
      `CallError::Task`) instead of failing on a missing handler or, worse, reading as an empty success
      for work that had not started. A tool returning `isError: true` is a **result, not an error** — it
      ran and refused, which is an outcome, not a broken connection.
      **A real defect was found by writing a fixture wrongly, and the message a user would have seen is
      why the fix matters.** The protocol's result union is `#[serde(untagged)]` (read in the pinned
      SDK's `model.rs`, `ts_union!`), so a result whose declared `resultType` disagrees with its fields
      does not fail as "a malformed `task`" — it becomes the SDK's generic `UnexpectedResponse`. The
      first `CreateTaskResult` fixture nested the task instead of **flattening** it (the SDK uses
      `#[serde(flatten)]`), so nothing matched and the refusal arrived as `Unavailable`, whose text is
      "the call did not complete" — describing an unreachable peer and sending an operator to inspect a
      connection that is working. That is now its own variant, `CallError::Undecodable`, whose text
      names the untagged-union cause. Same family as the other opaque-diagnostic defects this project has
      recorded.
      **`connect_stdio` is now exercised against a real child process**, closing `P3-008e`'s "entirely
      unexercised" limit. `src/bin/fixture_peer.rs` is a hand-written server behind the off-by-default
      `fixture-peer` feature — so `cargo build --workspace` never builds it and it cannot be mistaken
      for a shipped binary — and it writes the wire framing itself rather than using the SDK, for the
      reason the scripted peer does: a client and server from one library share their assumptions, so a
      disagreement with the specification would pass. Tests cover spawning, negotiating, listing,
      calling, that the configured arguments reach the child, and that a **program which cannot be
      spawned is refused rather than hung** — the failure a user hits first. The skip path was
      falsified: deleting the fixture under `ACCEPTANCE_REQUIRE_BINARIES=1` fails with the actionable
      message, and `cargo test` alone does not rebuild it (the recorded stale-binary trap).
      **Also removed a dead error variant found during review.** `ListError::NoIdentity` was declared,
      documented as "a signal worth surfacing", and constructed by nothing — absence of an identity is
      an empty `ReportedIdentity` on a working value, which is precisely what makes a server that
      *stops* naming itself observable. Removing it made a test's wildcard arm match exactly one
      variant, which clippy caught, so the removal also tightened an assertion. A declared-but-
      unconstructed variant reads downstream as a live condition, which is the `expected_version` class
      corrected at `P3-006c`.
      **Honest limits.** `connect_http` was covered by `P3-008g` below, which also closed this slice's
      "nothing reads a `config.toml`" note only in part: the MCP servers were not read from any daemon
      document, so nothing constructed a `HostedServer` outside a test and `apps/jarvisd` had
      **zero** references to either MCP crate. **CORRECTION (2026-09-23): `P3-008j` closed that** — the daemon
      loads `mcp-servers.toml` and constructs the host, so an MCP server is reachable from a running daemon.
      A **cross-server
      collision** is detected and reported but nothing acts on it, because the daemon that would refuse
      to serve is `P3-009`. `tools/call` is proven over the wire and against a child process, and
      `P3-008h` makes it an adapter a policy-authorized request reaches — but **nothing composes that
      adapter into the daemon**, so no run drives an MCP tool yet, and no `ToolOutcome` from an MCP call
      is stored. `P3-012` is the gate for that. Connection **pooling and reconnect** are unbuilt, so
      every build reconnects. The catalog's `MAX_MCP_SERVERS` bound is enforced but the server count is
      still the caller's to supply.
<a id="p3-008f"></a>

- [x] `P3-008f` Join discovery to authority: build a catalog from live connections, and add `tools/call`.
      **Recorded as its own slice rather than folded into `P3-008`, so the five preceding entries stay
      legible as five questions answered.** New code: `crates/jarvis-mcp-transport/src/host.rs`
      (`HostedServer`, `HostBuild`, `UnreadableServer`, `build_catalog`), `McpConnection::call_tool`
      with `ToolCallResult`, `CallError`, `src/bin/fixture_peer.rs`, and `tests/{host,stdio}.rs` plus
      five new `wire.rs` tests. 42 transport tests; **806 workspace tests, 37 suites**. ADR-0026.
      Findings, the defect the tests found, the falsified skip guard, and every honest limit are in the
      `P3-008` entry above; this line exists so the ledger shows the slice happened.
<a id="p3-008g"></a>

- [x] `P3-008g` Make a remote endpoint a validated value, and build the HTTP client in this crate.
      **Closes `P3-008f`'s recorded limit that `connect_http` was "entirely unexercised", and closes a
      real client-policy gap found while researching it.** `connect_http` took a bare `&str`, so every
      rule `docs/architecture/security.md` lists for "SSRF and unsafe redirects" was implicit and
      unenforced. New code: `crates/jarvis-mcp-transport/src/endpoint.rs` (`McpHttpEndpoint`,
      `EndpointError`, `build_http_client`) and `tests/http.rs` (10 tests against a hand-written HTTP/1.1
      server). `connect_http` now takes `&McpHttpEndpoint`. 62 transport tests; ADR-0027.
      **The client-policy gap is the finding worth recording.** The SDK's `default_http_client` disables
      redirects but **does not call `no_proxy`**, so proxy support there is off *only* because the SDK's
      manifest pins `reqwest` with `default-features = false` — a fact in a dependency's `Cargo.toml`,
      not a property of anything in this workspace. A feature unification elsewhere in the graph could
      re-enable an unchosen intermediary with nothing here changing. `jarvis-models` already refuses the
      same thing explicitly for the model client, so the two adapters were making different choices
      about the same hazard. This crate now builds the client itself and passes it through the SDK's
      `with_client`, making each choice a statement rather than an inherited default.
      **The endpoint rules are properties of the value**: scheme is `http`/`https`; no userinfo (a
      credential in a URL is a substring of every log line naming it, the rule `ApiKey::new` already
      enforces); no `#` fragment (never sent to a server, so a URL carrying one expresses something other
      than what the request will do); no interior whitespace; and **TLS off loopback**, since a plaintext
      remote MCP session carries tool arguments and results in the clear. Loopback recognition uses
      whole-host comparison, so `127.0.0.1.evil.example` is correctly remote.
      **The redirect test's assertion order is load-bearing, and falsifying it is what showed why.**
      Allowing redirects made the test fail on a *message* assertion first (the refusal that arrives is a
      protocol error whose text does not mention `302`), so the run ended before the security property was
      examined — the test would have failed for a formatting reason while the thing it exists to catch
      went unobserved. With the property asserted first, re-running the falsification reports the
      redirect target actually receiving the request (a `GET` carrying `mcp-protocol-version` and a
      `referer` for the original host), which is the vulnerability stated as evidence.
      **Honest limits.** **DNS resolution and private-range blocking are deliberately NOT claimed** — a
      point-in-time answer is the wrong shape for a question where a name can resolve differently when
      connected, which is why `security.md` lists rebinding defense separately; the module records the
      residual rather than implying coverage. The standalone `GET`-for-SSE path is declined by the test
      server and is therefore **not covered**. `Mcp-Method`/`Mcp-Name` agreement is asserted on the
      client's *outgoing* request only — the server side's `-32020 HeaderMismatch` handling is `P3-009`.
      At the time of this slice the MCP servers were not read from any daemon document, so an endpoint was
      constructed by a caller and nothing loaded one from `config.toml`. **CORRECTION (2026-09-23):**
      `P3-008j` made `mcp-servers.toml` load endpoints, so an operator now writes one. The endpoint rules
      here are what make that safe to write, which is why the slice order was right: the policy existed
      before the reachability did.
<a id="p3-008h"></a>

- [x] `P3-008h` Make a remote server's tool a real `ToolExecutor`, and get the outcome mapping right.
      **Closes the limit every preceding MCP slice recorded — "nothing drives it from a run" — at its last
      step.** New code: `crates/jarvis-mcp-transport/src/adapter.rs` (`McpToolAdapter`) and
      `tests/adapter.rs` (15 tests). `McpCallResult` was renamed from `ToolCallResult`, because
      `jarvis-tools` has a different type of that name and the two must not be conflated: one is a raw wire
      result, the other is an established outcome with its evidence and bounded output. ADR-0028.
      **The conversion is the subject, and its table is in the module.** A tool that reports failure is
      `Failed` with the server's reason (it ran and refused); a JSON-RPC error is `ProviderRefused` (the
      peer answered, so nothing happened); an undecodable answer is `AmbiguousAfterReaching` (the server
      answered, so it ran something); a transport failure after sending is `AmbiguousAfterReaching`; MRTR
      and Tasks are `ProviderRefused`; an unrouted identifier is `NotImplemented`; a past deadline is
      `RefusedBeforeReaching`. Evidence is a **locator** (`mcp:<operator's server>/<server's tool>`) and
      output is content, kept separate because `tools-and-connectors.md` requires it — and the operator's
      name is used rather than the server's own claim, since ADR-0024 makes that claim evidence and not an
      identifier.
      **A falsification found a missing test, which is the most useful outcome here.** Changing the
      `Unavailable` mapping from `AmbiguousAfterReaching` to `RefusedBeforeReaching` left the suite
      **green**: no test covered a transport failure at all, so the one row deciding whether a
      non-idempotent effect may be repeated was unpinned. The scripted peer gained `HangUp` — a method
      answered with silence and a **closed** connection, the only way to express "sent, no answer" — and
      that test now fails the falsification with the exact wrong claim ("refused the call before reaching a
      provider: Transport closed"). **A row in a mapping table is a claim until a test pins it.**
      **The design was wrong once and the reason is already load-bearing elsewhere.** The first version put
      the server's own tool name into the request's `arguments` under a reserved key. That violates the
      tool's input schema *and* the authorization digest, which `ToolExecutionRequest::new` recomputes and
      compares — so every request would have failed its own binding check. Both properties exist
      deliberately (`P3-001`'s contract, `P3-006b`'s binding), so the routing moved into the adapter,
      supplied from the catalog entries at construction and filtered to one server so an identifier cannot
      be sent to a server it does not belong to.
      **A fixture mistake is recorded in the test rather than fixed silently**: an MCP tool requires the
      `mcp.call` scope, and an empty grant turned every test into a refusal, visible only because the
      fixture asserts an allowance instead of assuming one.
      **Honest limits.** **At the time of this slice nothing composed `McpToolAdapter` into the daemon** — no
      run drove an MCP tool, no `ToolOutcome` from an MCP call was stored, and `apps/jarvisd` had zero
      references to either MCP crate. **CORRECTION (2026-09-23): `P3-008j` closed the composition**, so the
      daemon now dispatches a policy-authorized request to an MCP adapter and stores its outcome. No
      `run_events` row is written for an MCP call (`P3-012` links calls to the event log). The `Confirmed` for a successful call rests on the server's own `isError: false`, which
      the protocol gives no way to verify — the honest statement is that it is the strongest available
      evidence, and the design keeps it separate from the output so a reader can weigh it.
<a id="p3-008i"></a>

- [x] `P3-008i` Add the MCP host role: the configuration surface, and the first caller of the collision check.
      **Closes the limit nine slices recorded — "a capability a caller can use, not one an operator can
      reach" — at its last step.** New code: `crates/jarvis-mcp-transport/src/host_config.rs`
      (`McpHostConfig`, `McpHost`, `ServerTransport`, `HostConfigError`, `HostError`) and
      `tests/host_config.rs` (7 tests including the whole journey). 103 transport tests. ADR-0029.
      **A test starts from TOML text and ends with a call**: configuration → spawn a real child process →
      negotiate → list → translate → catalog → adapter → `Confirmed`, all through the fixture binary. A
      stdio server is spawned by the configuration itself, so the test proves the *program string an
      operator wrote* is what runs.
      **`has_cross_server_collision()` has its first caller**, held since `P3-008c` with a documented
      instruction and nobody to follow it. A collision refuses the **whole host**, and the same test's
      `Prefixed` control proves the refusal is about the collision rather than about an unusable
      configuration.
      **The posture vocabulary is two classes and the default is the severe one.** An operator writes
      `class = "read-only"` or nothing; nothing means `unclassified` — risk 3, every call held — so
      **omitting something fails closed**. A full `ToolEffectPolicy` in a file would be a vocabulary with no
      consumer; the one thing an operator can usefully narrow today is "this only reads". An unknown class is
      **refused**, because a typo must not silently leave a server at the permissive value an operator was
      trying to set.
      **Two findings, both from getting something wrong first.**
      1. **The collision message named one server twice.** It was built from `CatalogExclusion`'s `server`
         field, but a collision entry records only the server that *lost* the assignment, so two colliding
         tools produced `["bravo", "bravo"]` — while the remedy ("rename one of them") needs **both**. The
         refusal now carries the catalog's own `NameCollision` text, which names both sides. A refusal that
         does not identify what must change is the opaque-diagnostic defect in the one place an operator has
         nothing else to go on.
      2. **A parse error claimed a cause it could not know.** `InvalidDocument` said "an unknown key is
         refused rather than ignored", but a syntax error, a misspelled key, and a wrong value type all arrive
         as the same deserialization failure. A fixture using a Windows path in a TOML **basic** string — where
         `\U` begins an invalid escape — reported a syntax error as a key mistake, sending an operator to
         check keys that were correct. The message now carries a **character offset** and warns about the most
         likely cause; the parser's own text is not forwarded, because it can echo a document that holds paths
         and hostnames.
      **`HostedServer` changed shape** (`McpConnection` → `Arc<McpConnection>`) so the catalog build and the
      adapters share **one** connection per server. That is a statement, not tidiness: two connections would be
      two sessions (or two child processes) whose lifecycles diverge, and shutdown would close only one; it
      moved ~13 test construction sites. `McpHost::close` drops the adapters before closing, because a
      connection can only be closed by its last owner, and a still-shared connection is **reported** rather
      than claimed as stopped.
      **Honest limits.** At the time of this slice the daemon did not read an `mcp-servers.toml` document and
      `apps/jarvisd` had **zero references to either MCP crate**, so this was a configuration surface a caller
      could drive and not one the daemon loaded. **CORRECTION (2026-09-23): `P3-008j` closed that.** The daemon
      now reads `mcp-servers.toml`, connects the host, registers its adapters, and dispatches to them, so the
      sentence above describes this slice's boundary rather than the platform's. `P3-009` remains the gate for
      the **other** direction (JARVIS as a server). No `run_events` row is
      written for an MCP call (`P3-012`). **No credential can appear in the document**: a remote server needing
      authentication needs the credential store and is its own slice. Connection **pooling and reconnect** are
      still unbuilt, so every host build reconnects.
<a id="p3-008j"></a>

- [x] `P3-008j` Put the MCP host in the daemon: `mcp-servers.toml`, the dispatch table, and the two-scope actor.
      **This closes the limit all ten preceding MCP slices recorded** — "a capability a caller can use, not one
      an operator can reach" — because it is the first slice where `apps/jarvisd` references either MCP crate.
      New code: `apps/jarvisd/src/dispatch.rs`, `apps/jarvisd/src/mcp_host.rs`, and `apps/jarvisd/src/tool_pipeline_tests.rs`;
      changed: `tool_pipeline.rs`, `tool_actor.rs`, `gateway.rs`, `main.rs`. 103 transport + 14 new daemon
      tests; **880 workspace tests, 40 suites**. ADR-0030.
      **The dispatch table is its own type, and it refuses two things rather than resolving them.** A tool the
      registry offers that no adapter can run (`DispatchError::Uncovered`) would be authorized by policy and
      then fail, so it is a **startup** refusal; and a tool two adapters claim (`DispatchError::Duplicate`)
      would make which one ran depend on registration order, so the same call could reach different providers
      on different builds. The table is keyed by identifier because that is the key the receipt binds — a scan
      would return the first match, which is an implicit ordering decision of exactly the kind this type exists
      to make impossible.
      **The falsification of the routing check PASSED when it should have failed, and that is the finding.**
      Changing `adapter_for` to `self.adapters.values().next()` left
      `a_call_reaches_the_adapter_that_owns_its_definition` **green**, because the test registered only **one**
      adapter — "return any adapter" then finds the right one, so the test proved the dispatch table was
      non-empty and nothing about routing. The fix was to register the filesystem adapter beside the MCP one
      and assert the precondition explicitly (`dispatchable_tools() == 3`); re-running the falsification now
      fails with `NotImplemented { tool: "mcp.test.search" }`, which is the misroute stated as evidence. **A
      routing test needs at least two candidates, and the count has to be asserted or the second adapter can
      be dropped later without the test noticing.** This is the same shape as `P3-008h`'s unpinned row: a
      check that cannot fail is not a check.
      **The MCP servers live in their own document, and the reason is that a failure there must not stop the
      daemon.** `mcp-servers.toml` sits beside `config.toml` rather than as a section inside it, because the
      daemon's schema is validated by **key allowlist** — an `[mcp]` section would mean `jarvis-storage`'s
      schema learns one protocol's configuration vocabulary, which is a dependency from an adapter into a
      protocol shape that `repository-layout.md` forbids in the other direction and would be just as wrong
      here. Keeping the documents separate also means a third-party program that is missing, a server that
      hangs, or a collision between two configured servers are all reasons for the **MCP tools** to be
      unavailable rather than reasons for the daemon to refuse to start; each is logged so the absence is
      visible rather than silent. A **pipeline** failure is fatal by contrast, because it covers the filesystem
      grant and the registry: starting anyway would serve a tool set nobody declared.
      **A missing document is not an error; an unreadable one is.** A daemon nobody configured servers on is
      the normal case, and requiring the file would make every existing profile fail to start. But a document
      that exists and cannot be read is reported, because an operator who wrote one and has it unreadable would
      otherwise conclude their servers were configured and empty.
      **`WorkspaceRoots` refuses an empty list, so the pipeline takes an `Option`.** `RootError::NoRoots` exists
      precisely so a tool cannot silently read nothing while looking like a tool that works. A daemon with MCP
      servers and no filesystem grant therefore registers **no filesystem adapter at all** rather than one over
      zero roots — the tool is absent rather than present-and-failing. This surfaced as a *test* failure
      (`NoRoots`) while writing the routing test, and the failure was the composition being wrong rather than
      the test.
      **The actor grants both scopes, and the narrow constructor had to stay.** `tool_actor.rs` gained
      `workspace_and_mcp`, granting `files.read` **and** `mcp.call`; the gateway uses it, because
      `workspace_reader` grants only `files.read` and **every MCP call would have been denied for a missing
      scope**. Widening the existing constructor was rejected: a profile with no filesystem roots has no
      filesystem tool to read, so granting `files.read` there would be a grant with no consumer. A test asserts
      `MCP_CALL_SCOPE` equals `jarvis_mcp_transport::DEFAULT_MCP_CALL_SCOPE`, so the two copies of that literal
      cannot drift apart silently.
      **Honest limits.** No `run_events` row is written for an MCP call (`P3-012` links calls to the event
      log), so the **call row** is the only durable record of it. The host is composed once at startup and
      **never reconnects or pools**, so a server that dies stays unavailable until the daemon restarts.
      `seen_before` is always empty, so an identity drift across a restart is **invisible** — `P3-009`'s
      persistence problem, unchanged. The **`NamingStrategy` is hardcoded to `Prefixed`** and the file cannot
      set it, so `Bare` is unreachable from configuration — **CLOSED by `P3-008k` below.** A
      **collision refuses the MCP host and its tools are
      simply absent** — the correct outcome, but the caller is not told which servers were dropped, only a log
      line. And the daemon is proven to **compose** a host from a document, not to have serviced an MCP call
      through a **run**: the routing test drives the pipeline directly rather than over HTTP.
      **The gate failed on a pre-existing defect, and fixing it turned out to be this slice's most
      consequential work.** `cargo test --workspace` failed `a_cancellation_requested_in_flight_settles_the_run_once`
      with `the agent run was changed by another writer`. `apps/jarvisd/src/executor.rs` was **not modified by
      this slice**, and the test passed 12/12 in isolation, so it was initially tempting to call it flaky and
      re-run. It is a real lost-update race, and ADR-0022 — which claims to have fixed exactly this test
      "structurally" and "verified by 12 consecutive passes" — **had not**: it removed the *expectation* from
      the cancellation request but left `version = version + 1` on the write.
      **A version is a guard for a write that depends on the version it read; this write depends on nothing.**
      Its predicate is a state and its effect is idempotent under `COALESCE`, so advancing the version protects
      nothing while invalidating the expectation every other writer holds — the executor holds one across the
      whole model call. The fix is that the cancellation no longer advances the version at all (ADR-0022
      decision 7; the superseded decision and the false sentence are both struck in place rather than edited
      away).
      **The reproduction is deterministic, and building it corrected a claim of my own.** `a_progress_write_racing_a_cancellation_neither_conflicts_nor_loses_the_request`
      stages the read → cancel → write interleaving explicitly rather than sleeping, because the defect is an
      **ordering** rather than a timing — which is precisely why "12 consecutive isolated passes" could not
      have found it. Falsifying it found that my first test comment was **false**: I asserted that dropping the
      version bump alone would lose the cancellation, and running it showed the transition's own `COALESCE`
      re-preserves the request, so the test simply passes. Dropping the `COALESCE` is what fails it
      (`left: None`), so the two properties are pinned separately and the comments now state the mechanism that
      actually holds. **A test comment is a claim like any other, and the only way to know which property a
      test pins is to break each one in turn.**
      **The lesson supersedes the one the earlier slice recorded.** "It passed 6/6 after the change" and "12
      consecutive passes" are not evidence about a *concurrency* defect: the window only opens under load, so
      isolated repetition samples the wrong thing. `P2-009a`'s finding 3 ("writes now re-read first") was
      necessary but not sufficient, and it was recorded as if it were the fix.
<a id="p3-008k"></a>

- [x] `P3-008k` Make the naming strategy the document's, so an operator can choose it and a collision is
      reachable from configuration.
      **Closes the limit `P3-008j` recorded** — "the `NamingStrategy` is hardcoded to `Prefixed` and the file
      cannot set it, so `Bare` is unreachable from configuration" — and with it three defects rather than one
      gap: an operator could not express the choice, **a caller could pass a strategy the document disagreed
      with**, and `HostError::Collision` was **unreachable through the daemon's own path** because `Prefixed`
      cannot collide. New: `NamingDocument`, `StrategyName` and the `naming_strategy()` accessor in
      `crates/jarvis-mcp-transport/src/host_config.rs`; `connect` lost its `strategy` parameter and eight call
      sites were updated. `ADR-0127`.
      - **`connect` takes no strategy, so the disagreement is unrepresentable rather than refused.** That is the
        shape of the fix: `McpHostConfig` owns the value and reads its own field, so two builders of one
        configuration cannot produce two tool sets — the property `dispatch.rs` refuses for adapters and the
        catalog refuses for duplicate names.
      - **An absent section means `Prefixed`, and the default is stated on `StrategyName` rather than derived
        onto `NamingStrategy`.** `NamingStrategy` already derives `Deserialize`, so reusing it was shorter — but
        a `#[serde(default)]` on the domain enum would be **that type choosing a security-relevant default for
        one consumer**. The default belongs where the consequence is, and the consequence is which tools a model
        is offered. `StrategyName` is also written out rather than derived, for the reason every vocabulary here
        is: the domain's `as_str` is its wire form, and a rename there must not silently change what a document
        means.
      - **The daemon's test changed from asserting a constant to asserting a read.** The old
        `the_daemons_naming_strategy_is_the_one_that_avoids_collisions` passed a document and checked that two
        unreachable servers composed — which **a daemon that ignored the section would also pass**. The new pair
        is `the_daemon_reads_the_configured_naming_strategy` (the parse, asserted directly, because an
        unreachable server cannot collide so the collision path cannot distinguish the strategies) and
        `a_configured_bare_strategy_can_collide_and_is_then_refused`. **A test that asserts a hardcoding is a
        test of a constant.**
      - **The collision test now writes the strategy into its document**, and asserts both halves: `bare` with
        two servers offering one name is a **refusal**, and the same two servers under the default **compose**.
        A test of the refusal alone would pass if every document composed.
      - **Both directions falsified, in both layers.** Replacing the parsed strategy with `Prefixed` in `parse`
        fails the transport's collision test *and* the daemon's read test (`left: Prefixed, right: Bare`);
        making `Bare` the default fails `a_configured_stdio_server_becomes_a_callable_tool` — the identifiers
        lose the prefix (`["mcp.fetch", "mcp.search"]` where `["mcp.local.fetch", "mcp.local.search"]` is
        required) — and the absent-section assertion in the collision test.
      - **⭐⭐ The same over-correction mistake as `P4-016`, made again, and caught by the compiler this time.**
        Restoring mutation 1 with a **global** replace rewrote *both* `naming:` initializers, so `none()` — which
        has no document — tried to read `parsed`. `P4-016` recorded this exact lesson ("a mutation restore must
        be scoped to its occurrence") and it recurred, because the anchor was a line that legitimately appears
        twice. The compiler caught it here; in `P4-016` a silent narrowing of three queries did not. **The rule
        that works: when the anchor appears more than once, count the occurrences first and refuse to replace
        unless the count is the one you expect.**
      - **Limits:** `Hashed` is expressible but **not exercised against a real server**, because no fixture
        reports a name that cannot be represented. The strategy is **per host, not per server**, so a setup
        where one server's names need hashing has to hash both or be split across two profiles — the section
        shape is where a per-server override would go.
      - Gates: fmt, clippy `-D warnings` over the workspace, the workspace suite, `cargo deny check` ok.
<a id="p3-009"></a>

- [ ] `P3-009` Implement authenticated, scoped JARVIS MCP server exposure with per-client allowlists and rate limits.
      **Split into three recorded parts, because this reverses a platform decision and adds an auth surface.**
      `P3-009a` (done below) is the policy value; `P3-009d` (done below) is the served surface, recorded under
      its own number because "which tools may a caller reach" is a different question from "from where";
      `P3-009b` is the server transport wired to both; `P3-009c` is the client allowlist and rate limiting. The
      slice is deliberately **not** one change: the research found that the SDK's server defaults are
      permissive on the specification's MUSTs, so the policy has to exist and be testable before anything
      inherits a default.
<a id="p3-009a"></a>

- [x] `P3-009a` Make server exposure a policy value: origin trust decisions the SDK cannot make.
      **The first server-side slice, and it exists because reading the SDK's server transport contradicted its
      own documentation.** New code: `crates/jarvis-mcp/src/exposure.rs` (`ServerExposure`, `AllowedOrigin`,
      `OriginVerdict`, `OriginError`, `ExposureError`, `is_loopback_host`) and 17 tests. 92 `jarvis-mcp` tests;
      **900 workspace tests, 40 suites**. ADR-0031. Research recorded live in
      `docs/research/integrations/mcp.md`.
      **The finding: `StreamableHttpServerConfig::default()` is permissive on exactly the MUSTs a security
      story rests on.** From the vendored source — `allowed_origins: vec![]` with
      `validate_empty_origin_allowlist: false` makes `validate_origin_header` return `Ok(())` **before reading
      the header**, so the spec's "servers **MUST** validate `Origin`" is unenforced; its own doc comment says
      it "disables Origin validation for backward compatibility". `legacy_session_mode: true` mints an
      `Mcp-Session-Id` that `2026-07-28` (SEP-2567) removed. `stateless_protocol_metadata_required: false`
      "preserv[es] today's legacy behavior where an absent header is treated as protocol version
      `2025-03-26`". **The SDK is not uniformly permissive** — `allowed_hosts` defaults to loopback only and
      the `-32020` header↔body check runs unconditionally — and that is what makes inheritance dangerous: a
      fail-closed field and a fail-open field are indistinguishable at a call site.
      **The second finding, from reading the comparison rather than its description.** The SDK's doc says an
      `Origin` "must match per RFC 6454 `(scheme, host, port)`"; its `origin_is_allowed` is
      `a_scheme == o_scheme && a_host == o_host && (a_port.is_none() || a_port == o_port)`. The
      `a_port.is_none()` arm makes **a portless entry a wildcard over every port**, and browsers omit a default
      port — so `https://x` admits `https://x:8443` (far broader than it reads) while `https://x:443`
      false-rejects the normal traffic it was written to allow. **A control whose narrowest setting is a
      wildcard and whose exact setting is wrong is not a control**, so JARVIS owns the comparison: a default
      port and an omitted port are one origin, and no other port is.
      **A real defect was found by a test rather than by inspection.** `AllowedOrigin` derived `Eq`, which
      compares `port` as written, so the duplicate check accepted `https://x` and `https://x:443` as two
      entries while `matches` correctly called them one origin — the operator's policy would hold one origin
      twice with the file reading as two. `PartialEq`/`Eq`/`Ord` are now hand-written against the **effective**
      port.
      **Both central properties are pinned by falsification.** Replacing the default-port implication with
      `unwrap_or(0)` fails `an_omitted_port_and_the_schemes_default_port_are_one_origin` and the duplicate test;
      folding `Malformed` into `Absent` fails `a_malformed_origin_is_refused_rather_than_treated_as_absent` and
      `the_default_policy_refuses_every_present_origin` — the second because a malformed `Origin` would become
      the admitted shape, which **inverts the control** rather than merely weakening it.
      **Decisions worth naming.** An empty allowlist is **enforced**, which is the opposite of the SDK's empty
      default, so the accessor is `is_loopback_only()` and not `allowed_origins().is_empty()` — a reader
      checking only for emptiness would conclude the reverse of what they hold. `Origin` is **not** an
      authentication boundary (a non-browser client sends anything, and a client omitting the header is
      admitted by the spec's own rule), so the real boundary is `requires_remote_bind()` and the refusal to be
      reachable off-host at all. Every refusal answers the same `403`, so a hostile caller cannot enumerate the
      allowlist by the refusal received. `null` is refused at both ends, because admitting the opaque origin
      admits every sandboxed frame at once.
      **Honest limits.** **Nothing serves anything yet**: this is a policy value with no transport behind it,
      so `P3-009b` is what makes JARVIS reachable as a server, and the SDK field mapping is not written. There
      is **no remote bind and therefore no OAuth resource server** — RFC 9728 Protected Resource Metadata and
      RFC 8707 audience binding are MUSTs for a server that adopts OAuth, and they are not built, so a
      remotely reachable JARVIS MCP server would be an **unauthenticated control plane**. That is why the
      slice refuses exposure off-host rather than exposing and relying on JARVIS authorization in front of it.
      **No per-client allowlist and no rate limiting** (`P3-009c`). The policy does **not** model
      `allowed_hosts`, because the SDK's loopback-only default is already fail-closed and a remote bind is
      refused outright. `Origin` is checked against a configured list only — there is no support for
      wildcard subdomains, deliberately, since a wildcard in an allowlist is the broadest possible reading of
      a narrow intent.
<a id="p3-009d"></a>

- [x] `P3-009d` Decide what JARVIS serves: the served surface, with a transitive tool refused and named.
      **Answers the question that actually decides whether exposure is safe** — which of our tools may a
      remote client call — after `P3-009a` answered *from where*. New code:
      `crates/jarvis-mcp/src/served.rs` (`served_tools`, `ServedTool`, `ExposureExclusion`, `ServableName`,
      `MAX_EXPOSED_TOOLS`) and 11 tests. 103 `jarvis-mcp` tests; **911 workspace tests, 40 suites**.
      ADR-0032.
      **The central rule: a tool whose source is third-party code is never re-exposed.** The test is
      `ToolSource::is_third_party()` (`Mcp`, `Runtime`, `Extension`). A **connector** tool *is* servable,
      because JARVIS wrote the adapter and our own operator declared its risk — which is why the rule is not
      "anything but `Native`". The reason is worth stating: our exposure decisions (origin allowlist,
      loopback bind, `P3-009c`'s per-client allowlist) describe **this daemon**. A caller reaching
      `mcp.github.search` through us has driven a call through our policy and then through a third party's
      tool that policy never classified, in a context we did not choose — the `ToolEffectPolicy` governing it
      was written for *our* use of that server on *this* machine. **Nested exposure is deliberately not
      built**: consent, whose credentials execute the call, and which audit record owns it are unanswered
      questions, and a federation feature must not arrive as a side effect of "expose my tools".
      **Exposure gets its own type rather than reusing `McpCatalog`, because the rules are the opposite
      direction's rules in three places.** Naming: outbound, rule 1 says a server does not name itself so we
      invent the name; inbound, the only name a remote operator can be sure of is the **canonical identifier
      transmitted verbatim**. Posture: outbound it is an operator's declaration about a third party
      (ADR-0025); inbound it is irrelevant, because what matters is whether the source is code this project
      wrote. Absence: outbound a server being unreachable excludes its tools; inbound a **known but
      unavailable** tool is withheld, because advertising it offers a tool that fails on every call. One
      plausible reuse would have given three wrong answers.
      **A constructible tool that cannot be transmitted is excluded, not renamed.** The protocol
      `SHOULD`-constrains a tool name to `A-Za-z0-9_.-`, and `ToolId`'s name segment legally contains `:` — so
      `jarvis.files:read` is a real, registerable tool the wire cannot carry. Renaming would give a remote
      caller a name the daemon's own operator cannot find in their configuration. The check is **stricter**
      than the wire's alphabet: it admits exactly what the canonical rules produce, so uppercase is refused
      too and a future identifier vocabulary cannot become transmittable by accident.
      **The most consequential reason wins, and that ordering was falsified.** A transitive tool that is also
      unavailable is reported as `Transitive`. Checking availability first makes the reason "the account is
      not connected", so an operator reconnects an account and fixes nothing — the tool would still not be
      exposed.
      **Three falsifications, one per rule.** Removing the transitive guard serves `mcp.github.search`;
      checking availability before source reports the unfixable reason; removing the bound serves 35 tools
      instead of 32. The bound is checked **after** every eligibility check, so `dropped` means "withheld for
      size" rather than "withheld", and the truncation is reported once as a **list** problem with `tool()`
      returning `None` — an operator fixing one tool's availability would learn nothing from a size bound.
      **Also fixed from a real inconsistency:** the name check's first version admitted uppercase while its
      documentation claimed to enforce the canonical lowercase alphabet. The test asserting the documented
      property failed, and the check was tightened to match its own description.
      **Honest limits.** **Nothing serves anything yet** — `P3-009b` wires the transport to this value and to
      `ServerExposure`, so these are two policy values with no listener behind them. **The served schema is
      transmitted verbatim**, so a schema the protocol's own `x-mcp-header` rules reject is **not filtered on
      the server side**; `check_tool_schema` is the client-side model for it and the server-side placement is
      `P3-009b`'s. No per-client allowlist and no rate limiting (`P3-009c`). The posture attached to a served
      tool is the **local** declaration unchanged, which is honest only because a third-party-sourced tool is
      excluded outright — so there is no case where a posture about someone else's code is published.
<a id="p3-009e"></a>

- [x] `P3-009e` Serve the handler: refuse an unadvertised tool before anything runs, and name one revision.
      **The first place JARVIS is the one being called, and the slice whose title is its central rule.** New
      code: `crates/jarvis-mcp-transport/src/serve.rs` (`JarvisMcpServer`, `ServedToolRunner`,
      `call_result`, `SERVED_PROTOCOL_VERSION`) and 11 tests. 47 transport tests; **922 workspace tests, 40
      suites**. ADR-0033.
      **Filtering a list is not authorization, and that is the finding.** An MCP client may call **any** tool
      name whether or not the server advertised it — nothing in the protocol couples `tools/list` to
      `tools/call`. A server that only filtered the list would have a **catalogue, not a control**. So the
      handler holds the served set and `invoke` refuses an unserved name **before the runner is consulted**,
      and the test's runner **panics if it is reached** — because "the call was refused" and "the call was
      refused before anything ran" are different claims and only the second is the security property.
      **Falsified: removing the check fails with `the runner must not be reached for mcp.github.search`.**
      **Two SDK server defaults are wrong for this revision, and the research had recorded both before the
      code existed.** `ProtocolVersion::default()` is `LATEST`, and `LATEST` is **`V_2025_11_25`** — the legacy
      handshake era — so `ServerConfig::default()` advertises a revision this build does not implement; the
      falsification prints `left: ProtocolVersion("2025-11-25")` as evidence. And
      `supported_protocol_versions` defaults to `KNOWN_VERSIONS`, i.e. **every** version the SDK knows
      including the legacy ones, so a server inheriting it would accept an `initialize` handshake whose
      semantics it does not drive and the client could not tell. Both are narrowed to `V_2026_07_28`.
      **A refused call travels as a result, not as a protocol error.** The SDK's own documentation says
      `Err(McpError)` is rendered **opaquely** — the caller sees "Tool result missing due to internal error" —
      so a refusal that travelled that way would be unreadable. The outcome table is the honesty boundary one
      level above `McpToolAdapter`'s, and its important row is **`Unknown`**: it must *say* the outcome is
      unknown rather than report a failure, because a caller reading "failed" would reasonably retry and a
      retry of a non-idempotent effect is a second effect. Falsified by mapping `Unknown` onto the failure
      text. A **truncation** is likewise stated in the content rather than silently applied.
      **No annotations are advertised.** `readOnlyHint`/`destructiveHint`/`idempotentHint` are exactly what a
      policy engine wants, and the specification warns clients to treat them as untrusted — so a tool's
      declared effects stay in the JARVIS contract (ADR-0025) rather than being restated where the protocol
      says not to believe them.
      **The `server` feature's cost was measured before enabling it.** Five new packages (`schemars` 1.2.2,
      `schemars_derive` 1.2.2, `serde_derive_internals` 0.30.0, `dyn-clone` 1.0.20, `pastey` 0.2.3), nothing
      removed, and **no new duplicate** — the duplicate `name@version` sets before and after are identical
      (`chrono`, `tower`, `sse-stream`, `http-body`, and `uuid` were already locked). `cargo deny` reports all
      four categories ok with the features on. Recorded in `docs/research/integrations/mcp.md` because a
      dependency decision should cite measurement rather than preference.
      **A fixture mistake was caught by the type it was testing.** The first `served` fixture declared
      `ToolSource::Native` for `gmail.messages.send`, and `ToolDefinition::new` **refused** it because the
      namespace implies a connector — so a test cannot construct a definition the production path would
      reject. The fixture now derives the source from the identifier.
      **Honest limits.** **Nothing is bound.** There is no loopback HTTP listener, no stdio serve, and no
      daemon wiring, so a remote client cannot reach this handler — `P3-009b` is the transport and `P3-009c`
      is the per-client allowlist and rate limiting. `ServedToolRunner` has **no daemon implementation**, so a
      served call cannot yet pass the JARVIS pipeline: its actor, scopes, approvals, idempotency key, and
      durable call row are all unexercised from this direction, and that wiring needs a run-versus-remote
      attribution decision a slice must make explicitly. `CallToolResponse::Complete` is the only variant
      JARVIS answers, because approval-shaped interaction belongs to the JARVIS approval path rather than a
      remote client (ADR-0025) — so the protocol's `input_required` and `task` paths are deliberately
      unreachable rather than unimplemented. No `run_events` row for a served call (`P3-012`).
<a id="p3-009b"></a>

- [x] `P3-009b` Build the serving configuration, and drive the handler through the real SDK service.
      **`P3-009a`'s design argument became a functional one, and a gap `P3-009e` recorded is closed.** New code:
      `crates/jarvis-mcp-transport/src/serving.rs` (`ServingConfig`, `ServiceError`, `MCP_ENDPOINT_PATH`,
      `MAX_REQUEST_BODY_BYTES`) and `tests/serving.rs` (11 tests driving the **real** service). 56 transport
      tests; **942 workspace tests, 41 suites**. ADR-0034.
      **The finding: origin validation cannot be delegated at all.** The SDK's `validate_origin_header`
      compares against `allowed_origins` directly and has **no extension point** — `with_allowed_origins` is
      the whole surface — so the comparison `P3-009a` rejected cannot be replaced. `ServingConfig::sdk`
      therefore calls `disable_allowed_origins`, and `origin_check` is the decision; **enabling both would be
      worse than disabling the SDK's**, because two checks with different rules would disagree about a
      portless entry and the permissive one would be part of the answer, while a populated `allowed_origins`
      would read as the control. A disabled field with the reason recorded is honest; an enabled field whose
      rule is wrong is a false assurance.
      **Every permissive SDK default is a stated value, and a test reads the SDK's `Default` alongside it**, so
      an upgrade that changes a default fails rather than silently changing the policy: `legacy_session_mode`
      `false`, `stateless_protocol_metadata_required` `true`, the body bound stated, `allowed_hosts` loopback,
      and `NeverSessionManager` in place of an in-memory store for a protocol with no sessions.
      **A configuration allowing a public origin is refused outright** rather than warned about, because serving
      off-host needs RFC 9728 metadata and RFC 8707 audience binding that are not built, and a remotely
      reachable MCP server without them is an unauthenticated control plane. The test is on the **host**, so
      `http://localhost:3000` is served and `https://jarvis.example.com` is refused.
      **Three facts were discovered by driving the real service rather than assumed.** The `Accept` header must
      list both `application/json` **and** `text/event-stream` — sending only the first is answered
      `406 Not Acceptable`, so `P3-009a`'s client obligation is enforced server-side too. A refused tool call is
      **`200` with `result.isError: true`**, not a `4xx`, which is `P3-009e`'s decision seen from outside. And a
      `GET` is `405` while an oversized body is `413`, both now pinned.
      **This closes the gap `P3-009e` recorded**: `list_tools`, `call_tool`, and `server/discover` were
      compiled but **never executed** through the SDK, so a response-shape mismatch had nowhere to fail. They
      are now driven with a real `http::Request` and `http::Response`, which is the path a client's POST takes.
      `server/discover` also confirms the modern revision is advertised and `2025-11-25` is not.
      **A falsification corrected a test's own claim, for the second time this phase.**
      `a_request_without_a_protocol_version_header_is_refused` was written as evidence for
      `stateless_protocol_metadata_required(true)`; removing that line did **not** fail it, because the refusal
      comes from the SDK's body-versus-header agreement rule — a body `_meta` version requires the matching
      header whatever the flag says. `a_request_with_no_protocol_signals_at_all_is_refused` was added to isolate
      the flag, and the first test's comment now names the property it actually pins.
      **Honest limits.** **Nothing is bound**, deliberately: `ServingConfig` produces the SDK's service and
      binding a socket is the daemon's, because `repository-layout.md` gives network listeners to the
      composition root — so this is reachable from a test and not yet from a client. **Origin enforcement over
      a real request is `P3-009c`'s** (the decision is here, the layer owning the request is there). No daemon
      wiring, no per-client allowlist, no rate limiting. The `server` feature's five packages are recorded in
      `docs/research/integrations/mcp.md` with the measurement that admitted them.
<a id="p3-009f"></a>

- [x] `P3-009f` Enforce the SDK-boundary invariant, and remove the leak `P3-009b` added.
      **Written because reviewing `P3-009b` against `repository-layout.md` found that its public `sdk()` and
      `service()` returned SDK types, which the docs had explicitly forbidden.** New code:
      `crates/jarvis-mcp-transport/src/boundary_tests.rs` (5 tests) and `src/serving_transport_tests.rs`, which
      is the SDK-facing transport proof moved **inside** the crate. **947 workspace tests, 40 suites**. ADR-0035.
      **A documented invariant with no test is a convention, and this one had been wrong for two phases.**
      `connect_over` (`P3-008e`) is public with `rmcp::transport::IntoTransport` and `rmcp::RoleClient` in its
      signature, and `revision.rs` exposes three functions returning or taking `ProtocolVersion`. So the
      sentence "no provider SDK type appears in this crate's public surface" was **not merely violated by
      `P3-009b` — it had been false since `P3-008e`**, and every reader in between was misled in the reassuring
      direction: someone asking "does an SDK type cross this boundary?" would have concluded no.
      **The first version of the test did not catch the violation it was written for, and falsifying found it.**
      It searched for the literal crate name `rmcp`, which finds a *fully-qualified* path and misses an
      *imported* name — so `pub fn sdk(&self) -> StreamableHttpServerConfig` was reported clean, and restoring
      that signature left the test green. The scan now collects the names a file imports from the SDK and checks
      both forms, with unit tests for each: a public declaration naming the SDK in either form must be reported,
      and a private one must not.
      **The justified exposures are an exception list with reasons, and a companion test keeps it honest** —
      each entry must still match a real declaration, so a rename or removal fails and deleting an exception is
      a deliberate act. `connect_over` is a documented test seam; the three revision helpers exist to make the
      `LATEST = V_2025_11_25` trap executable, and `describe_negotiated` takes the SDK's type deliberately so a
      caller cannot pass a bare string that disagrees with what was negotiated.
      **`P3-009b`'s own leak was removed rather than recorded.** `sdk()` and `service()` are private;
      `tool_list`/`invoke` are `pub(crate)` because their return types are the SDK's `Tool` and `CallToolResult`,
      with `serves()` as the public `bool` statement of the same rule; `served_protocol_version()` returns the
      **wire string** rather than `ProtocolVersion`. Four dead items went with it (`bounded_reason`,
      `method_not_found_code`, `SdkSurfacePosture`, and a `ServedEndpoint` wrapper that existed **only** to reach
      an SDK type publicly — so removing the leak removed its reason to exist).
      **The SDK-facing transport proof moved in-crate** to `src/serving_transport_tests.rs`, because an
      integration test sees only the public surface and the accessor it needed would have been the violation. A
      crate-internal module compiling against the SDK is not the same as the SDK being part of this crate's
      contract, and only the second is what the invariant forbids.
      **The one not-yet-consumed configuration carries `#[cfg_attr(not(test), expect(dead_code, ..))]`**, because
      `sdk`/`service` are reached only from tests while nothing binds. An `expect` fails once the lint stops
      firing, so the binding slice must delete it; the scoping is needed because the claim is true in only one
      configuration, and an unconditional `expect` is unfulfilled in a test build.
      **Honest limits.** `connect_over`'s visibility should move behind an off-by-default feature so the
      **shipped** surface is SDK-free; that is recorded as a follow-up rather than half-built. The four recorded
      exceptions remain, with their reasons and their guard test. Nothing is bound, so `ServingConfig` is still
      reachable from a test and not from a client (`P3-009c`).
<a id="p3-009g"></a>

- [x] `P3-009g` Decide who may call: an allowlist whose subject is a credential, never a label.
      **The inbound half of `ADR-0024`, and the design was decided by reading the protocol's shape before
      drawing it.** New code: `crates/jarvis-mcp-transport/src/admission.rs` (`CallerAdmission`, `Fingerprint`,
      `CallerLabel`, `AdmittedCaller`, `AdmissionVerdict`, `AdmissionError`) and 13 tests. **960 workspace tests,
      40 suites**. ADR-0036.
      **The finding: `clientInfo` is entirely client-chosen, so an allowlist keyed on it is a control a client
      names itself into.** `Implementation` in the pinned SDK is `{name, title, version, description, icons,
      website_url}`, decoded from per-request `_meta`, and nothing verifies any field — so a caller sending
      `name = "vscode"` would be admitted **as VS Code** with no credential at all. That is *a server does not
      name itself* arriving inbound, where it is worse because the label decides **permission** rather than
      identity. The rule is now stated in the type: **a self-reported name is evidence or nothing, never a
      permit** — the inbound mirror of `jarvis_mcp::ReportedIdentity`.
      **A label mismatch is reported, never refused** — falsified by forcing `label_matches` to `true`, which
      fails `a_label_mismatch_is_reported_rather_than_refused` with `left: Some(true)`. Refusing would make the
      label a **second permit after the module says it is not one**, and would break every call from a valid
      credential on a client version upgrade. A mismatch is worth *seeing*, so the operator decides.
      **A pasted credential is refused locally as a fingerprint**, because `Fingerprint::parse` accepts only the
      digest alphabet — a JWT (`eyJ…eyJ…`) or a `Bearer …` value fails with "it is probably not a fingerprint"
      rather than being stored, compared, formatted into diagnostics, and committed. `Display` prints a
      **prefix and a length**, so a log line names the caller without carrying a collectable digest.
      **`local_only` is the default and admits nobody remotely**, which is the same decision `ServingConfig`
      already makes for the bind — stated **twice on purpose**, because a control that depends on another
      control having worked is not a control (a proxy or a changed bind would otherwise be enough). The two empty
      states are opposites, so the accessor is `is_local_only()` rather than an emptiness check.
      **Rate limiting is a bound here and the counting is the daemon's**: `decide` takes `spent_budget: bool`,
      because a policy that mutates per request cannot be compared, logged, or reused. The budget is a **rate**
      (twelve per minute), so a burst is not punished for the rest of a window. Refusals answer **`401`** for a
      missing or unknown credential rather than `403` — the two are the same situation to a caller, and
      distinguishing them would reveal which fingerprints exist — and `429` for a spent budget.
      **Honest limits.** **Nothing binds**, so no request reaches this policy yet; binding plus applying both
      decisions at the request layer is the next slice. **The token itself is unvalidated**: audience binding
      (RFC 8707) and Protected Resource Metadata (RFC 9728) are the OAuth slice, and until they exist a remote
      caller cannot be admitted at all — which is why `is_local_only` is the default rather than a warning.
      `spent_budget` is **supplied by the caller**, so a daemon that never sets it has a rate limit that never
      fires — recorded rather than implied, because a bound nothing enforces reads exactly like one that works.
      The clock is not modelled, because a budget window needs one and that belongs with the counting.
<a id="p3-009h"></a>

- [x] `P3-009h` Fix `local_only` admitting nobody, and make a local caller's origin explicit.
      **A review of `P3-009g`'s shipped policy against its own docs found that its central default refused
      everybody.** `decide` took only a credential fingerprint, so `None` meant "no credential" and was refused —
      but a **local** caller is precisely the case the credential apparatus does not apply to. So the default
      **refused the operator on their own machine**, while its doc said "the only admitted caller is a local
      one". Fixed in place rather than in a new slice: `CallerOrigin` is now a parameter, the falsification
      reproduces the original bug exactly (`left: NoCredential, right: Admitted`), and the ADR's claim is
      corrected with the reason. 16 admission tests; **963 workspace tests, 40 suites**.
      **`CallerOrigin` is supplied by the request layer and derived from nothing a caller sends**, which is the
      rule `CallerLabel` exists for one level down: there is deliberately no `Deserialize` and no constructor
      from a header or body, so a remote caller cannot admit itself by claiming to be local. A test asserts the
      type has exactly two states, so adding a deserializer is a compile failure rather than a silent widening.
      **The rate limit applies to a local caller too**, and that ordering is load-bearing: `decide` checks the
      budget *before* the origin, because an early return for `Local` would make the bound reachable by anyone
      who could reach the socket.
      **The lesson is about what a test asserts.** The original tests checked that a *remote* caller is refused —
      which the bug satisfied perfectly. Nothing asserted the doc's own words about a local caller, so a shipped
      document described behaviour the code did not have. That is the same defect class this phase has found five
      times now, and the remedy is the same: **write the test that asserts the claim, in the claim's terms.**
<a id="p3-009i"></a>

- [x] `P3-009i` Apply both inbound decisions to one request, in one place, and make an admitted request a type.
      **Closes the limit every slice above recorded: "a policy value nothing enforces".** New code:
      `crates/jarvis-mcp-transport/src/enforcement.rs` (`RequestGate`, `RequestAdmission`, `RequestRefusal`) and
      11 tests. 99 transport tests; **974 workspace tests, 40 suites**. ADR-0037.
      **The gate holds both policies and consults both**, so "both were applied" is structural rather than a
      convention a caller keeps: there is no way to hold one policy and call it a gate.
      **`RequestAdmission` has private fields and no public constructor**, so the only way to hold one is
      `decide` returning `Ok`. A `bool` carries the same information and none of the guarantee — the caller
      decides what `true` means, and `true` is also what a default-initialized field holds. This is `P3-009e`'s
      "a catalogue is not a control" applied to admission rather than to tools.
      **`Origin` is checked first, and that order is the specification's**: the revision says servers **MUST**
      validate `Origin` on **all** incoming connections, so a request failing **both** is reported as an origin
      refusal (`403`) rather than a `401` that tells the caller about its credential while the origin went
      unexamined. Swapping the two blocks fails `a_hostile_origin_is_refused_before_the_credential_is_considered`
      **and nothing else**, which is what makes that test the one holding the rule. The refusals keep different
      statuses because they carry different remedies.
      **`RequestAdmission` carries *how* the origin was decided, not only that it was** — `Absent` and `Allowed`
      are different justifications for the same outcome, and an audit record that could not tell them apart
      could not answer "was this a browser request", the question a loopback-only deployment most needs answered.
      **The gate takes values, never a request.** A gate reading a `HeaderMap` could not be tested without one,
      and deriving those values is the daemon's because the listener is.
      **No consistency check between the two policies.** A loopback-only `ServingConfig` beside an allowlist with
      remote entries looks contradictory and is not: the bind is one control, the admission policy another, and a
      control that depends on another having worked is not a control (`P3-009g`'s rule). Reconciling them would
      remove the second.
      **Two bugs were found by this slice's own tests rather than by review.** `also_refused_admission` returned
      `false` whenever the allowlist was `local_only`, conflating "the allowlist is empty" with "this request
      carried nothing to check" — the same confusion `decide` had before `CallerOrigin`, reproduced within one
      commit of being fixed; its test failed immediately. And a fixture used `https://jarvis.example.com` as an
      allowed origin, which `ServingConfig::new` correctly refuses, so the fixture moved to a loopback origin —
      the check working one layer up.
      **The falsification needed two attempts and the first was wrong.** Disabling the admission refusal *and*
      swapping the order left the order test **passing**, because that request was never refused on admission.
      The first attempt proved "the pair is broken" rather than "the order is reversed"; only the second
      isolates the property the test names. Worth remembering as a general shape.
      **Honest limits.** **Nothing binds**, so no request reaches this gate — the binding is `P3-009c`, and the
      four values are derived from nothing yet. `spent_budget` is still supplied by the caller, so a daemon that
      never sets it has a rate limit that never fires. The **token remains unvalidated**: no RFC 8707 audience
      binding and no RFC 9728 Protected Resource Metadata, so a remote caller can only be admitted against a
      fingerprint an operator configured by hand.
<a id="p3-009c-a"></a>

- [x] `P3-009c-a` Bind the endpoint: a tower service that derives both decisions from a real request.
      **The layer `P3-009i` recorded as missing — "nothing binds, so no request reaches this gate."** New code:
      `crates/jarvis-mcp-transport/src/binding.rs` (`ServedEndpoint`, `ResponseBody`, `CREDENTIAL_HEADER`,
      `BEARER_SCHEME`, `MAX_ORIGIN_HEADER_CHARS`, `REFUSAL_CODE`) and 12 tests. 112 transport tests.
      ADR-0038.
      **It is a `tower` service wrapping the SDK's own service, not a route handler.** The SDK's Streamable HTTP
      service *is* a `Service<Request<Body>>`, so wrapping it is one `impl Service` rather than a second routing
      layer and a second body type; `axum::Router::fallback_service` accepts any `Service`. The wrapping is also
      what makes the layer provable — a real request is the only evidence the four values were **derived** rather
      than handed over.
      **A network request is `CallerOrigin::Remote`, always, and that is the slice's central decision.** `Local`
      means the operator on their own machine, established by the daemon's **transport** (a named pipe, a socket
      it holds). A loopback *bind* is not that evidence: any local process, and a browser on the same machine,
      can reach `127.0.0.1`. Deriving `Local` from a peer address would be `CallerOrigin`'s own defect one layer
      down — a property of the request deciding admission — and a daemon wanting a local caller admitted does so
      through the **allowlist**. Falsified: changing the literal to `Local` fails three tests, including the
      anonymous remote caller being **admitted**.
      **An oversized `Origin` is truncated, never treated as absent** — because absent is *admitted* by the spec's
      own rule, so collapsing an oversized value into `None` **inverts** the control. Falsified by adding a
      `.filter(…)`: the response became **`200 OK` with the full tool list served**.
      **The credential requires its scheme, compared case-insensitively.** A bare digest in `Authorization` is not
      valid HTTP. Falsified in both directions: removing the scheme check admits the bare digest (`200` where
      `401` is required); removing the case-insensitivity would refuse a client sending `bearer`.
      **The refusal names the policy class and never the verdict**, because on the wire a verdict helps a hostile
      caller enumerate the allowlist. `RequestRefusal::reason` names it and is for the log.
      **The boundary test was extended to the shape that would leak next, and the probe found it.** `P3-009c`
      adds a `pub type` alias (`ResponseBody`), and a `pub type` is a signature-like public declaration with no
      `fn` — which the existing scan's `pub fn` probe would not have exercised. A `pub type Leaked =
      StreamableHttpService<…>` was added to the **real source** as a live probe and the scanner reported it by
      line; the probe was removed and replaced with a test over the same text. **That test failed on its first
      run**, because the scan keys imported names off the `use rmcp::…` line *in the same file* and the fixture
      had none — correct behaviour, discovered rather than assumed.
      **Honest limits.** **The daemon does not mount this yet** (`P3-009c-b`, which now does), so at the time this
      slice closed the layer was proven by tests and not by a socket. `spent_budget` is still a literal `false`,
      recorded at the call site rather than left to inference: the rate limit the gate could apply never fires.
      The admitted `RequestAdmission` is logged on a span and dropped — an admission is observable but **not
      attributable**, because attributing an MCP call
      needs a correlation id the request does not carry (`P3-012`). The **token remains unvalidated** (no RFC
      8707 audience binding, no RFC 9728 Protected Resource Metadata), so a remote caller can only be admitted
      against a fingerprint an operator configured by hand.
<a id="p3-009c-b"></a>

- [x] `P3-009c-b` Mount the endpoint in the daemon: choose the loopback port, wire the policies from configuration, and prove a real socket refuses an anonymous remote caller.
      `apps/jarvisd/src/mcp_serve.rs` (the runner, the endpoint, the bound listener), `mcp_serve_tests.rs`, plus
      `ToolActor::remote`, `ToolPipeline::call_remote_tool`/`definitions`, `Running`/`start`/`run`/`wait_for_exit`
      in `apps/jarvisd/src/main.rs`, and `mcp_serve_port` in `crates/jarvis-storage/src/config.rs`. ADR-0039.
      **The endpoint is bound on loopback on its own port and served under both inbound policies.** The REST
      transport's flag is separate: the two listeners have different trust boundaries and different answers to "who
      may call", so sharing a port would make one admission policy govern the other's traffic. `Some(0)` and a port
      with no granted workspace roots are refused at configuration time, because either would report a daemon ready
      on a listener that cannot answer.
      **A remote call has no run, and the schema is the authority.** `0007_tool_calls.sql` declares
      `run_id TEXT NOT NULL REFERENCES agent_runs (id)`, so a call with no run cannot be written as a `tool_calls`
      row. Inventing a run identifier to satisfy the column was the tempting shortcut and the foreign key refuses
      it. `call_remote_tool` therefore writes **no** row, and that is recorded as a limit rather than presented as
      completeness.
      **A second entrypoint rather than a `record: bool`**, so "may this call write a row" is a property of which
      function was called and cannot be set wrongly by an argument whose name does not say what is lost.
      **Parity is by calling the same code**: the same `validate`, the same `evaluate` over the same definition,
      workspace policy, and dispatcher. The differences are exactly two — the audit record and the approval outcome.
      **An approval is refused, not bypassed, and there are two independent checks**: the engine's
      `Decision::RequireApproval` (a *workspace* threshold) and `definition.approval() != ApprovalPolicy::Auto`
      (a *tool* declaration), both refused with `mcp_call_cannot_hold_an_approval` — because reporting the
      workspace's own code would tell a client an approval is obtainable when this path cannot hold one.
      **⚠ The second check had no test, and that was measured rather than suspected: mutating it to `false && …`
      left the suite green at 13/13.** A test now registers a real `ToolDefinition` declaring
      `ApprovalPolicy::Ask` at risk 0 — so the default workspace threshold of `Moderate` cannot hold the call,
      making the declaration the only thing under test — beside a **counting** adapter that must not be reached.
      The mutation now fails exactly that one test.
      **⚠ The caller policy was not covered either**: replacing `build_endpoint`'s `caller_policy` argument with
      `CallerAdmission::local_only()` left 14/14 green, because every other test already passes `local_only()` and
      a parameter that is always given one value is indistinguishable from one that is ignored. A test that passes
      an *admitted caller* now pins the pass-through, and it asserts its own fixture is not the default so it
      cannot silently become a duplicate of its neighbours.
      **The remote actor's scopes are the union of the served definitions' `required_scopes`** — exactly what the
      advertised tools demand. Never `mcp.call`, which is this daemon's grant to call someone else's server: on the
      inbound direction that is a scope with the right name and the wrong direction. **The first version granted
      nothing and every call was refused with `missing_scope`**; its own test caught it, and reverting to
      `ScopeSet::none()` fails four tests.
      **An adapter's own error is propagated unchanged.** `map_pipeline_error` returns `AdapterCall(error)` as-is,
      preserving `AdapterError`'s three-way claim — `RefusedBeforeReaching`, `ProviderRefused`, and
      `AmbiguousAfterReaching`. Rewriting all three into "nothing was reached" would tell a client to retry a call
      whose effect is unknown, and for a non-idempotent tool the retry is **a second effect**. Every other pipeline
      error is raised before the dispatcher is consulted, so mapping those to `RefusedBeforeReaching` is a
      statement the code supports. Tested by **constructing** the ambiguous value, since no served adapter returns
      one. Also `into_service` now pins `Future = ResponseFuture` with `Clone + Send + Sync + 'static`, because an
      opaque `impl Service` says nothing about `Service::Future` being `Send` and the omission otherwise surfaces
      at the **mount site** in an error naming its generic parameter rather than this layer.
      **Five mutations were run, one property each**, and each failed exactly the test naming it: the scope union
      (4 tests), the held-decision refusal (1), the tool-declaration refusal (1), the empty-surface refusal (1), and
      the caller policy (1).
      **Honest limits.** A remote call is **observable but not attributable**: it is logged and has no `tool_calls`
      row, because that row needs a run — `P3-012` owns the durable link. `spent_budget` is still a literal
      `false`, so the rate limit the gate can apply never fires. The origin list is **not configurable**:
      `build_endpoint` builds `ServerExposure::loopback_only()` through `ServingConfig::new` rather than reading a
      configured list, because a configurable list is a value that can make a startup either fail or serve
      something nobody chose; the caller allowlist is where a deployment says *who* may call. The **token remains
      unvalidated** (no RFC 8707 audience binding, no RFC 9728 Protected Resource Metadata), so a remote caller can
      only be admitted against a fingerprint an operator configured by hand.
<a id="p3-013"></a>

- [x] `P3-013` Close the test-scratch directory leak: a fixture's `Drop` cannot await the close that releases the file.
      **Found while running the `P3-009c-a` gate suite, and it is a real defect in the test fixtures rather than
      in shipped code.** Two causes were behind one symptom, and both are now fixed. New code:
      `jarvis_core::remove_scratch_dir` plus the `MAX_SCRATCH_REMOVAL_ATTEMPTS`/`SCRATCH_REMOVAL_INTERVAL`
      constants in `crates/jarvis-core/src/testkit.rs`, all 27 guard `Drop` impls converted, and a source scan that
      keeps future fixtures from reverting. Also `docs/development/testing.md` (the rule) and
      `docs/architecture/repository-layout.md`.
      **Fixed: the naming collision.** ~19 scratch-directory helpers built a path as
      `temp_dir() / format!("jarvis-<what>-{pid}-{sequence}")` with `static …: AtomicU64 = AtomicU64::new(0)`
      beside it — so the sequence starts at 0 in every process and a pid is **reusable**. A directory survives
      whenever a test fails or the suite is killed (no `Drop` runs), and the next run with the same pid reopened
      the previous run's database. Measured: `jarvis-storage` passed **144/144 alone** and produced ~20
      `UNIQUE constraint failed: sessions.id` failures inside a full-workspace run. Now one helper,
      `jarvis_core::scratch_tag()` (a `UUIDv7`), used by all 22 sites, with a test asserting 1,000 tags are
      distinct — because two distinct values would have passed for the broken scheme too.
      **Fixed: the directory is never removed — and the recorded mechanism was wrong in two places.** The entry
      above used to say `sqlx-core-0.9.0/src/pool/mod.rs` has **no `impl Drop for Pool`** and that `Pool::close` is
      an `async fn`. Both are false: `PoolInner` **has** a `Drop` (`src/pool/inner.rs`) and `PoolConnection`
      **has** one (`src/pool/connection.rs`), and `Pool::close` is a **sync fn returning a lazy future**
      (`pool/mod.rs:441`). The real behaviour: `PoolInner::close()` calls `mark_closed()` and closes the
      connections **in the future's body**, `PoolInner::Drop` **never awaits** that future, and
      `PoolConnection::Drop` hands the connection back by **spawning a task**. So the file is released **late and
      off-thread**, and the teardown failure is a **race**, not a permanent hold.
      **Six shapes were measured, and each obvious fix fails for a different reason** — this is the part worth
      keeping:
      - drop then `remove_dir_all` immediately ⇒ **fails 5/5**; drop, `await` 250 ms, remove ⇒ **always Ok**;
      - a bounded **blocking** retry (50 × 10 ms) ⇒ **still fails**: `std::thread::sleep` starves the
        current-thread runtime, so the spawn that returns the connection never runs;
      - a **background thread** retrying `remove_dir_all` for 1 s ⇒ **still fails**, for the same reason: it only
        *waits* for a release that needs the runtime to make progress;
      - dropping the **runtime** first, or `shutdown_timeout(5s)` after ⇒ **fails**; no runtime alive + a real 1 s
        delay ⇒ **Ok**;
      - driving `database.close()` on a **fresh runtime inside a worker thread** ⇒ **DEADLOCKS**, because the close
        future waits on a semaphore only the *original* runtime can release. **A deadlock in teardown is worse
        than the leaked directory**, so that shape must never be shipped.
      **What is shipped:** a **detached thread** that retries, spawned from `Drop` (which cannot await). It does not
      starve the runtime, and it keeps trying across the point where the runtime tears down. The window is
      **measured**: no retry left **124** directories per full-workspace run and this leaves **7–10**. Two variants
      measured worse or no better and are recorded so they are not retried — a **ten-second** per-call window left
      the same handful (a window covers a *delay*, not a release scheduled after it has closed), and an
      **always-running** retry loop left **22**, because a thread competing for CPU across the whole suite delays
      the teardowns that actually release the handles.
      **What remains, honestly:** the residual handful are fixtures that hold an `Arc<SqliteDatabase>` — through a
      `ToolPipeline` — past the directory guard, so the handle is released at the **end of the test** (covered by
      the window) or at **process exit** (which nothing in-process can reach). Those directories **are** removable
      once the process has exited, which is how the race was confirmed rather than inferred. The old "28,226"
      figure is also misleading as a measure of the current suite: **546 of 615** sampled directories came from
      three fixture families (`fnd007`, `fnd013`, `fnd014`) that **no longer exist in the source**, so a historical
      total answers "how bad did it get" rather than "what does the current code do".
      **The guard is a source scan, not a behavioural test**, because no assertion can observe a directory *not*
      leaking without measuring the filesystem around a whole suite. It requires the **discarded-result** form
      (`let _ = …remove_dir_all(&self.0)`), so it catches a bypass without flagging its own explanation or an
      unrelated removal — and it **names the offending file and line** when it fires (falsified by reverting one
      fixture, which it reported at `crates/jarvis-tools/src/workspace.rs:303`). The scan initially matched its own
      prose and read every file in `target/`, which cost **85 s**; skipping comments and reading only `.rs` files
      brought it to **0.4 s**.
      **Test-only**: no shipped path relies on a pool dropping.
<a id="p3-010"></a>

- [x] `P3-010` Add MCP Inspector conformance tests and cross-SDK interoperability tests.
      **Split into what is provable now and what is blocked, and the provable half found a real defect.**
      New code: `crates/jarvis-mcp-transport/src/conformance.rs` (4 tests), a vendored derived schema slice at
      `crates/jarvis-mcp-transport/tests/spec/` (11 definitions, 41 KB, with provenance and an extraction
      script), `jsonschema` as a dev-dependency, and `JarvisMcpServer::tools_list_result`. ADR-0040.
      **The defect: `tools/list` was emitting a document the revision's schema rejects.** Read from the official
      **machine-readable** schema rather than the human-readable pages: `$defs.ListToolsResult` declares
      `"required": ["cacheScope", "resultType", "tools", "ttlMs"]`, and `DiscoverResult` requires the same two
      among five. `rmcp` 3.4.0 models both fields and **deliberately leaves them unset** —
      `with_all_items` sets `ttl_ms: None, cache_scope: None`, with the field doc saying *"Required by spec
      version 2026-07-28, but optional here to maintain compatibility with older spec versions"*, pinned by the
      SDK's own `cache_hints_are_omitted_when_absent` test. That default is correct for a **multi-era** server,
      because a field this revision requires would be a spurious field on an older wire. This server narrows to
      one era, so the compatibility argument does not apply and the omission was simply non-conformant.
      **Two asymmetries show it is a per-type default rather than a policy, and they are why the defect was
      isolated rather than systemic:** `DiscoverResult::from_server_info` **does** set `ttl_ms: 0` and
      `cache_scope: Private`, so `server/discover` always conformed; and `CallToolResult` requires only
      `["content", "resultType"]`, so `tools/call` was never affected. The requirement is on *cacheable*
      results, which is why only the list method was wrong.
      **Both values are posture, not tuning.** `cacheScope: "private"` because the endpoint is admission-gated:
      the schema's own distinction is whether a response "does not contain user-specific data" and may be cached
      "across authorization contexts", so answering `public` would tell a caching proxy it MAY serve one
      caller's tool list to another. `ttlMs: 0` because the served set derives from a policy an operator can
      change, so a cached list would keep offering a tool this server had stopped serving — `0` is the schema's
      own wording for "immediately stale".
      **The authority is the specification, not the SDK, and that is the point.** The check has three layers
      that share no assumption: the revision's own schema slice, `jsonschema` (a general-purpose validator with
      no MCP knowledge), and JARVIS's serialization. An MCP-aware validator would have agreed with the bug,
      because the bug **is** an SDK default — the same reasoning `P3-007` recorded for negotiation fixtures,
      one level up.
      **⚠ The first version of the conformance test was wrong and the falsification attempt found it.** It
      assembled its own JSON from `tool_list()` plus the two constants, so **removing the builder call changed
      nothing** — the test never read the builder. The construction was extracted into
      `JarvisMcpServer::tools_list_result`, reachable by both the trait method and the test, so the validated
      document is the value the transport actually sends. *A test that restates the code it checks is not
      checking it*, and a falsification that does **not** fail is a finding about the test rather than a
      reprieve. Removing `.with_ttl_ms(..)` now fails with `"ttlMs" is a required property`, and the slice
      carries a **negative control** proving it rejects the omitted shape, so the check is not vacuous.
      **Honest limits — the live client-level test is blocked, and the reason is recorded rather than glossed.**
      The official `modelcontextprotocol/conformance` suite (Apache-2.0, `npx`) and the MCP Inspector are both
      **third-party clients**, and `jarvisd` builds its endpoint with `CallerAdmission::local_only()`, which
      `P3-009g` made *enforce*: a remote caller is refused even on loopback. So neither can reach an MCP method
      against this daemon until audience-bound tokens (RFC 8707) and Protected Resource Metadata (RFC 9728)
      exist, which remain unbuilt. The live run is therefore named as the **next step** rather than implied to
      have happened. What was researched and kept: the suite's `--requirements <revision>` flag is the form a
      tier claim needs (`--suite`/`--spec-version` describe the suite as it grows, while
      `requirements/<revision>.yaml` is frozen at release), only *scored* scenarios affect the exit code
      (`extension`, `added-after-release`, and `pending` run and are reported but cannot fail), and a scenario
      shared between revisions must run **twice** — once per era — because `2025-11-25` and earlier use the
      stateful `initialize` handshake while `2026-07-28` is stateless with per-request `_meta`.
      Cross-SDK interoperability is likewise blocked for the same reason: it needs a foreign client to connect.
<a id="p3-011"></a>

- [x] `P3-011` Define sandbox contracts and implement one restricted process backend before exposing code execution.
  - **Shape of the crate.** `crates/jarvis-sandbox` with `policy.rs` (the request, the guarantees, and the
    refusal), `backend.rs` (the `SandboxBackend` contract, `GuaranteeSupport`, `backend_for_host`, the launcher
    seam), and `linux.rs` (the cgroup v2 backend, `#[cfg(target_os = "linux")]`). `docs/adr/0041` records the
    decision; `docs/research/integrations/os-process-sandboxing.md` records the live documentation evidence.
  - **A guarantee is an enum, not a boolean, and it is refused when it cannot be enforced.** `Guarantee` names
    `TreeTermination`, `ProcessCountCeiling`, `MemoryCeiling`, `CpuRateCeiling`, and `CpuTimeCeiling`;
    `SandboxPolicy::new` resolves a request's `required` list against the backend **once, at construction**, and
    `SandboxError::UnsupportedGuarantee` names the first guarantee the backend cannot provide. There is no
    "applied but weaker" outcome, because a caller who declared a requirement has already stopped checking — so a
    silently weaker sandbox is the one configuration nobody audits.
  - **`unsafe_code = "forbid"` decided the platform split.** Every Windows job-object call is `unsafe` FFI, so
    the job-object backend **cannot exist** in this workspace; Linux cgroup v2 is a **filesystem** interface
    (`pids.max`, `memory.max`, `cpu.max`, `cgroup.kill`), so it needs no FFI and is the backend that exists.
    `backend_for_host()` **probes the running host** rather than reporting the compiled target, and reports an
    **empty** support set on Windows and macOS, which makes every requirement a refusal there.
  - **`CpuTimeCeiling` is deliberately absent from the Linux backend**, and the omission is the load-bearing
    assertion. cgroup v2 accounts CPU time in `cpu.stat` but has **no limit file for a cumulative total** — only
    the rate form in `cpu.max` — while a Windows job object has `PerJobUserTimeLimit` and no rate form. That
    asymmetry is why CPU is two `Guarantee` variants rather than one, and `cgroup_v2_guarantees()` lives in the
    always-compiled `backend.rs` so a **non-Linux** host can still falsify the omission.
  - **`RLIMIT_NPROC` is rejected as a fallback, by construction.** It is counted **per user id**, so a child that
    reaches its own limit can still `fork` — each new process gets a fresh budget under the same uid. Only a
    tree-scoped counter makes a fork bomb fail. Reporting `ProcessCountCeiling` on its strength would be a
    guarantee a fork bomb walks straight through.
  - **Confined before the child exists, with the one unclosable window stated.** Limits are written before the
    spawn (which is what `memory.max` needs, since a limit applied afterwards is applied after the allocation),
    and the spawn is **injected** by the caller so only it decides about pipes. cgroup v2 has no atomic
    spawn-into-cgroup, so the migrate window is recorded as a limit rather than described as closed, and a test
    reads `/proc/<pid>/cgroup` to prove the child really landed in the launch's cgroup.
  - **First caller: `doctor`.** `check_sandbox` reports `sandbox.available` / `sandbox.unavailable` with the
    **facility and the guarantee list** as evidence — the list, not a boolean, because a boolean would let a host
    enforcing one of four read identically to one enforcing all four. Both are `Severity::Info`, following
    `ServiceNotApplicable`: a capability absent **by design** is informational, and `sandbox.unavailable`'s
    remediation names the `systemd Delegate=yes` action that would change the answer.
  - **Falsified.** 24 mutations across the crate and the doctor check, applied one at a time and each restored in
    a `try/finally`: 13/13 for the sandbox crate, 11/11 for the doctor check, with the tree re-verified clean and
    the restored suite re-run green afterwards. Two tests were **strengthened because a mutation survived them**:
    the kill test asserted only `Ok`, which a no-op `kill` also satisfies, so it now asks the operating system
    whether the pid is still alive; and the working-directory test now asserts the temp directory differs from the
    daemon's first, so dropping `current_dir` cannot pass.
  - **Honest limits.** (1) The Windows/macOS backend **does not exist**, so no guarantee is available there and
    enabling code execution needs either a lint relaxation for a job-object crate or a separate helper process,
    with its own ADR. (2) **No network policy, no child filesystem confinement, and no privilege reduction** —
    `Isolation::Restricted` bounds *resources*, and a confined child can still open any file and reach any host
    this process can. (3) The Linux tests are **compiled but have not been executed** in this development
    environment; they skip loudly when no cgroup is delegated, printing what could not be checked. (4)
    `connect_stdio` in `jarvis-mcp-transport` is **not wired** to this crate: that needs a trait seam so the
    transport does not depend on `jarvis-sandbox`, and the slice's phrase "before exposing code execution" is
    satisfied by the contract existing with refusal working, not by an execution path using it.
  - **`cargo clippy --target x86_64-unknown-linux-gnu` is part of this crate's gate**, and earned its place: it
    found six defects invisible on Windows, including an import unused only on Linux, two collapsible `if let`
    chains, identical `match` arms, an error-type mismatch in the kill path, and a duplicated `linux` module from
    declaring `mod linux;` in both `lib.rs` and `backend.rs` (a `#[path]` module declared twice includes the file
    **twice**, as two modules with distinct copies of every type).
<a id="p3-014"></a>

- [x] `P3-014` Split the overloaded acceptance skip-guard, because one variable described two artifacts and made CI red on every OS.
      **CI had been red for four consecutive completed runs, and the cause was a configuration value rather than a
      code defect.** `ACCEPTANCE_REQUIRE_BINARIES` was read by two guards that require *different artifacts produced
      by different commands*: the process-level phase gates need the application binaries (`jarvisd`, `jarvis`), and
      the MCP stdio tests need the `fixture-peer` child process. Commit `42f7017` set that single variable on the
      *Test workspace* step, whose command is `cargo test --workspace --all-features`. That builds each package's test
      harness and — because `--all-features` enables the `fixture-peer` feature — the fixture binary, but it **never
      builds `target/<profile>/jarvisd`**, which is one directory above `target/<profile>/deps` where the gate looks.
      So the skip cannot legitimately happen for the fixture and **always** happened for the application binaries,
      and the phase 1 gate inside that step asserted on all three runners.
      **Measured rather than reasoned:** with the application binaries removed, `cargo test -p jarvisd --no-run`
      compiles `jarvisd` but produces **no `target/debug/jarvisd.exe`** — `cargo test` does not place a package's
      binary beside its test harness. The failing assertion was reproduced locally by deleting the two binaries and
      running the gate with the variable set, and it printed the variable's own message ("requires built jarvisd and
      jarvis binaries") while the real question — which binary, and which command produces it — stayed unstated.
      **The fix is per-artifact variables.** The MCP guards now read `ACCEPTANCE_REQUIRE_FIXTURE_PEER`; only the two
      phase-gate steps, which run `cargo build --workspace` first, set `ACCEPTANCE_REQUIRE_BINARIES`. The workspace
      test step sets the fixture variable only. A skip-guard that turns absence into a failure must name the one
      artifact it is about, because the failure it reports is otherwise about a build step the reader cannot see.
      **Both directions were falsified, one property each:** with the fixture binary moved aside and
      `ACCEPTANCE_REQUIRE_FIXTURE_PEER=1`, the stdio suite fails with its actionable message; with the *old* variable
      set and the fixture absent, it skips — so the rename is effective rather than merely documented. The phase gate
      still fails closed when `ACCEPTANCE_REQUIRE_BINARIES=1` and the applications are missing, and skips when it is
      unset.
      **Two failure messages were also made diagnostic.** Each phase gate now names the specific absent binary
      (`jarvisd` or `jarvis`), because the gate cannot know *why* a build is missing and should not imply it does.
      Docs updated: `docs/development/testing.md` states the per-artifact rule and that `cargo test` does not produce
      an application binary in `target/<profile>` at all, and
      `docs/research/integrations/github-actions-rust-supply-chain.md` records the observed failure.
      **Gates:** `cargo fmt --check`, `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings`,
      `cargo test --workspace --all-features --locked` **with the application binaries absent** (the exact state the
      workspace step runs in — 0 skips), both phase gates with `ACCEPTANCE_REQUIRE_BINARIES=1`, and `cargo deny check`
      all green.
<a id="p3-012"></a>

- [ ] `P3-012` Prove approval restart and duplicate-delivery safety; pass the Phase 3 gate.
  - [x] `P3-012a` A held call persists the durable approval it is waiting on.
        `ToolPipeline::authorize_and_admit` returns `Admission::{Runnable,Held}`; a held decision writes an
        `ApprovalRequest` bound to the call's own canonical intent, and `AwaitingApproval` carries the `approval_id`.
        A held call has no receipt, because a receipt is what an adapter treats as permission and none exists yet.
        The requester is the run; the answer comes from the owner.
  - [x] `P3-012b` Let the owner answer a stored approval. (Simplified by `P3-037` / `ADR-0136`: an answer needs only
        the authenticated request. The decision route and the resumption of the held call are `P3-012c`.)
  - [x] `P3-012c` Link calls to approvals and events, and pass the Phase 3 process gate.
        Delivered as four slices, because each step exposed the next gap:
        - `P3-016` **linked a held call to its approval** (the `approval_id` column existed since `0007` with
          no writer) and wrote `tool_requested` / `approval_requested` into the run's stream.
        - `P3-017` made the **decision carry its approver** — `decode_approval` parsed `decided_by`,
          validated it, and discarded it, so no resume could build an `ApprovalCitation`. `ADR-0043`.
        - `P3-018` built **the resume path and its route**: `ToolPipeline::resume` and
          `POST /api/v1/calls/{id}/resume`.
        - `P3-019` built **`tests/e2e/tests/phase3_gate.rs`**, the roadmap exit gate as a process-level test,
          and wired it into CI.
        **The gate drives the real platform end to end**: it configures one real MCP server over stdio (the
        hand-written `fixture-peer`) with the daemon's **unclassified** posture — risk 3, effects
        `Write`+`ExternalCommunication`, `ApprovalPolicy::Ask`, which is what an operator gets by naming a
        server and saying nothing else — holds a call,
        **kills the daemon**, restarts it, decides the approval, resumes the call, and asserts the
        second resumption is refused. So the hold comes from the product's own configuration path rather
        than from a writable test adapter compiled into `jarvisd`.
        **Falsified end to end**: configuring the server `read-only` instead of unclassified makes the gate
        fail at the hold step (`409` where `202` is required), because a read is risk 0 and is not held.
        **⚠ Two recorded limits, and the gate names them rather than papering over them:**
        1. **An external MCP client cannot yet be admitted.** The daemon builds its served endpoint with
           `CallerAdmission::local_only()`, and `CallerAdmission::new` — the allowlist that would admit a
           credential — has **no production caller**: nothing reads one from configuration. `ADR-0038` makes
           every request over a network listener `Remote`, so no external client can authenticate at all.
           The gate therefore asserts the half that exists — an unauthenticated remote caller is refused
           `401` **through the real listener**, the fail-closed direction — and says in its module docs that
           it is not asserting a `200`. `ADR-0038` records why this is deliberate: a remotely reachable MCP
           server needs **audience-bound** credentials (RFC 8707), so the allowlist's credential vocabulary
           belongs to the OAuth slice, and inventing one here would pre-empt a documented decision about a
           trust boundary.
        2. **A resume does not re-evaluate policy.** The approval was decided against the policy in force
           when the call was held. Whether a policy tightened afterwards should refuse a resume is a real
           question — re-evaluating makes an approval lapse silently; not re-evaluating lets a decided
           action run under a superseded policy — and it is deliberately unsettled rather than settled by
           accident.
        **A falsification that found defence in depth.** Removing the resume's "still `requested`" check
        left the route test **green** with the adapter still reached once, because `execute_and_record`
        advances the call before the adapter and `record_tool_outcome` refuses to replace a terminal
        outcome. So the single-effect property is defended **twice**, and a test written as if one guard
        carried it would be claiming a guarantee two mechanisms provide. The assertion that carries it is
        the adapter's own call **count**, which both the route test and the pipeline test now assert.
        Gates: fmt, clippy `-D warnings`, 44 suites with the application binaries absent and **zero skips**,
        all three phase gates with `ACCEPTANCE_REQUIRE_BINARIES=1`, `cargo deny` ok.
<a id="p3-020"></a>

- [x] `P3-020` Add a restricted-execution sandbox backend behind the `P3-011` contracts (a container or an
      equivalent isolated worker), naming the guarantees the backend can actually enforce per `ADR-0041`.
      No new capability surface: the existing code-execution tool gains a backend, and a guarantee the
      backend cannot enforce is refused rather than declared. The `P3-011` backend stays available so a
      deployment without a container runtime degrades to it rather than to nothing.
  - **Shape.** `crates/jarvis-sandbox/src/container.rs` (`ContainerBackend`, `container_arguments`,
    `container_guarantees`), `Support::Container`, `SandboxRequest::image`, and a `wait` method added to
    `LaunchedProcess`. `backend_for_host()` now prefers a **probed** container where the host-process backend
    has nothing. `docs/adr/0128` records the decisions and `docs/research/integrations/container-sandboxing.md`
    the evidence, written before the code.
  - **Three of five guarantees, and the two refusals differ in kind.** `TreeTermination`,
    `ProcessCountCeiling` (`--pids-limit`, container-scoped), and `MemoryCeiling` (`--memory`). `CpuTimeCeiling`
    is refused because the only flag (`--ulimit cpu`) is `RLIMIT_CPU`, **per process**, so a tree of N
    processes gets N budgets — the same argument `ADR-0041` used to reject `RLIMIT_NPROC`. `CpuRateCeiling` is
    refused because the `--cpus` translation is **not written**, which is a fact about this file rather than
    about the facility, and is recorded as such.
  - **A memory ceiling has a floor, and zero is refused separately.** `--memory` below 6 MiB is rejected **by
    the daemon** (exit 125), so it is refused by value rather than passed on as an opaque launch failure; and
    `--memory=0` is *accepted* and read as **unlimited**, the inverse of the field's meaning. Found by probing
    the boundary. A mutation that deleted the explicit zero test **survived**, because zero is arithmetically
    below the floor — so the distinction now lives in the refusal's explanation and is asserted on its own.
  - **The environment boundary is `env -i`, and the first design for it was a no-op.** `-e` **adds** to an
    image's environment (`PATH` and `HOME` survive; probed), while `SandboxRequest::environment` is documented
    as the complete one. A `/bin/sh -c 'export -n -p; …'` wrapper was written and believed correct; probed,
    BusyBox `ash` ignores `export -n` and `-p` only *prints*, so the image's environment survived — and in
    `dash` it would have worked, making confinement a property of the image. The same argv also dropped the
    program, because `shift` skipped a marker `"$@"` had already excluded.
  - **The kill path removes the container, not the CLI.** `Child::kill` on the `docker` CLI leaves the container
    running in the daemon, so `kill` runs `docker rm -f <name>` and then reaps the client. The generated name is
    therefore a **handle**.
  - **Two defects the acceptance test found that the unit tests could not.** (1) The launcher started
    `request.program` — an **in-image** path — instead of the runtime, failing with a bare `entity not found`.
    (2) Because of that, the container argv was built **twice**, so the name generator ran twice and the
    container was launched under one name while `kill` removed another; the leftover sat in state `Created`
    while the kill reported success. Both are why the injected launcher is now `stdio_launcher(piped)` — a
    closure that only decides whether to capture the streams — and the backend builds the command. The ordering
    property is unchanged: the command is built *after* the confinement exists, so `refusing_launcher` still
    proves no process was created.
  - **`LaunchedProcess` gained `wait`.** It had `pid()` and `kill()` and no way to observe a child, so no live
    property of **any** backend was assertable — a handle that could be destroyed but never read. Consuming, for
    the `'static` future; the limitation that a consuming wait cannot be cancelled is recorded on the trait.
  - **The live tests are serialised.** A container runtime keeps one container list for the whole host, so
    parallel tests saw each other's containers and read an absence as a removal. A `tokio::sync::Mutex` is held
    across each live test, and `wait_for_owned_containers` polls because a container is created by the daemon
    *after* `spawn` returns. Both were observed failures, not precautions.
  - **Limits recorded, not closed.** A daemon that ignores `--pids-limit` is undetectable from here (it depends
    on the daemon's cgroup driver and kernel support); `--network none` and `--read-only` are enforced but are
    **not** `Guarantee`s, so a caller cannot require them and `doctor` does not report them — which means a
    container is confined *more* than the guarantee list says; `--security-opt no-new-privileges` is deliberately
    not passed, because unmodelled hardening enforces something a caller cannot see. A request with an `image`
    sent to a host-process backend is refused **by name** rather than served, because confining the `docker` CLI
    would apply `pids.max` to a client while the container's processes ran unbounded — silently, with `doctor`
    reporting four guarantees.
  - **Verification.** Live on this host: `doctor --json` reports `sandbox.available` with
    `facility: container` and `guarantees: tree_termination,process_count_ceiling,memory_ceiling`, where the
    same host previously reported `unconfined`/`none`. Eight tests launch real containers (read-only refusal
    observed as `Read-only file system`, `exit 7` propagating as **7**, kill leaving nothing in `docker ps -a`,
    the image's `PATH`/`HOME` cleared, empty-environment control, both refusals before a container starts,
    argument boundaries intact), and they skip **loudly** when no runtime or image is present. Mutations proved
    caught: `--read-only` removed; the removal skipped (leaves the container, named in the failure); the
    environment boundary skipped; the memory floor disabled; the zero explanation removed.
  - Gates: `cargo fmt --all --check` clean; `cargo clippy --workspace --all-targets --all-features --locked --
    -D warnings` clean; `cargo test --workspace --all-features --locked` **1992 passed / 51 suites**; `cargo
    deny check` ok (advisories, bans, licenses, sources).
<a id="p3-021"></a>

- [ ] `P3-021` Add a remote or disposable sandbox backend (a hosted or short-lived worker) behind the same
      contracts, and prove resource limits, crash isolation, output bounding, and cleanup on the adapter
      boundary. The `P3-011` port is unchanged, so this is an adapter slice and not a protocol change.
<a id="p3-022"></a>

- [x] `P3-022` Make the model a caller of the tool pipeline: offer it the registry's tools and drive the
      documented tool round trip, so the pipeline's model-facing consumer exists.
      **THE GAP THIS CLOSES WAS RECORDED BY `P2-009` ITSELF.** `MAX_MODEL_CALLS` was `1` with the comment
      *"One, because `P2-009` does not plan or use tools"*; `ChatRequest` had **no `tools` field**, so
      `ToolRegistry::discover()` — the model-facing tool surface — had no consumer; and the `OpenAI` adapter
      deliberately did not decode streaming `tool_calls`. The platform could serve tools over HTTP and as an
      MCP server and could run a tool a caller named, but **the model could not ask for one**: a user asking
      JARVIS to read a file received an answer generated without it. The whole policy/approval/audit pipeline
      was a capability a caller could use and not one the agent could reach.
      **Delivered in two crates, split along the dependency direction:**
      - `jarvis-models` gained the provider-neutral contract: `ToolSpec` (name, description, input schema),
        `ChatRequest::with_tools`, an assistant turn that carries `tool_calls`, and the `OpenAI` wire
        mappings (`WireToolDefinition`, outgoing `WireToolCall`, `WireMessage.tool_calls`). Streaming
        tool-call fragments are reassembled by `WireToolAccumulator` — grouped by the provider's `index`, not
        arrival order — and emitted **once**, at the turn's finish, so a partially written invocation never
        reaches the domain. `tool_choice` is deliberately **not** sent (`auto` is the server default with
        `tools` present, and the research record excludes the field as non-interoperable).
      - `apps/jarvisd` turned `execute_run` into `execute_run_with_tools`: a bounded loop over the documented
        state machine. The offered set is re-derived from `ToolRegistry::discover()` on every call, so it is
        always the current registry and the tool `ToolSpec` schema is the definition the pipeline validates
        against — one statement of the contract. Each invocation goes through `ToolPipeline::call_tool`, and
        the actor is built from the **stored run** (workspace + run id), never from the model.
      **⭐⭐ A TEST FOUND THE RE-ENTRY DEFECT, WHICH IS THE POINT OF WRITING THE TEST FIRST.** The first
      version of the loop ran the tool, observed it, and then went `Observing → Responding` — answering from
      a transcript the model had **not yet seen**. The tool ran and the answer ignored it. The test asserting
      two model calls failed with `left: 1, right: 2`, and the fix is the planning re-entry
      (`Executing → Observing → Planning`) that makes the loop a loop. **A tool round trip has three edges,
      not two, and only the missing third produces the defect** — the run looked successful either way.
      **⭐ THE OFFERED TOOL SURFACE IS A PROPERTY OF THE REQUEST, SO IT IS ASSERTED ON THE REQUEST.**
      `ScriptedModel::seen_tools()` was added beside `seen_messages()` for exactly the reason the messages
      one exists: an executor that attached no tools would satisfy every result-shaped assertion, so "the
      model was offered the registered tools" is unassertable without recording the request's own tool list.
      **⭐ A REFUSAL IS FED BACK, A CONFIGURATION FAULT IS NOT.** A policy denial, schema violation, unknown
      tool, or adapter fault becomes the tool **result** the model reads (with its reason code) so the loop
      continues and the model can recover — a test asserts an unknown tool does not fail the run. A **missing
      tool surface**, by contrast, fails the run, because an answer claiming to have used a tool nothing ran
      is the failure the system prompt warns against. **⭐ A HELD CALL PARKS THE RUN** at `AwaitingApproval`
      and stops; the approval and resume routes complete it, so the executor does not hold a promise
      `security.md` assigns to those routes.
      **Preserved deliberately:** the old contract test asserted a *fragment* is never surfaced; that
      property is kept and strengthened — an invocation missing its id or name is still dropped, while a
      complete one is now surfaced exactly once. **`ADR-0119`.** Gates: `cargo fmt --check`,
      `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings`, `cargo test
      --workspace --all-features --locked` (adds 20 model tests and 3 daemon loop tests, including the
      end-to-end "the model calls a real filesystem tool and answers from its result"), `cargo deny check` ok.
      **Limits, recorded rather than glossed:** the executor does not itself resume a decided call (the routes
      do); `MAX_TOOL_CALLS` is a fixed bound rather than policy-derived; the tool surface is offered whole
      with no selection rule (fine at the current tool count, named in `ADR-0119` as the condition for a
      selection or pagination rule); and no live provider has been called, so the loop is proven against the
      scripted adapter and the offline wire fixtures, not a real streaming provider.
<a id="p3-023"></a>

- [x] `P3-023` Continue the run a decided approval released: a paused conversation must produce its answer.
      **THE LIMIT `P3-022` RECORDED, CLOSED.** `P3-022` parked a run at `AwaitingApproval` when a tool call
      was held; the approval and resume routes executed the decided effect, but **nothing advanced the run**
      — it stayed at `AwaitingApproval` with its effect already made and its user still waiting. A
      conversation that paused for approval never finished.
      **Delivered:** `apps/jarvisd` gains `resume_run_with_tools` (a second entry point sharing one loop
      body) and `resume_held_call` (reads the decided call's outcome, feds it back as a **fenced**
      observation, and re-enters the model call); `gateway.rs`'s `resume_call` now spawns
      `continue_parked_run` so the run answers after the route executes the effect. Context assembly was
      split into `assemble_context_messages` (returns the messages) from the transition that moved the run,
      so the resume reuses it.
      **⭐⭐ A STATE-MACHINE VIOLATION, FOUND BY A TEST.** The first `park_for_approval` advanced
      `Executing → AwaitingApproval` and was refused by the domain (`run: the agent run transition was
      refused`). **The documented table has `Planning → AwaitingApproval` and NOT `Executing →
      AwaitingApproval`** — a hold is decided when the run *plans* a step, not while one is *running*, and a
      model call runs in `Executing`. Fixed by walking the legal path and narrating the hold honestly:
      `Executing → Observing → Planning → AwaitingApproval`. **A shortcut to a legal state by an illegal
      path is not a state-machine fix; the table encoded the semantics.**
      **⭐⭐ THE RUN CONTINUATION MUST NOT RE-RUN THE CALL.** The resume **route** owns the effect
      (`ToolPipeline::resume` refuses any call past `requested`, `P3-012c`). The first version resumed the
      call from inside the executor too — a test caught it as *"the approved effect must happen exactly
      once — left: 0"*, because the route had not run yet in the test. The fix separates the two: the route
      executes, the run continuation **reads** the stored outcome. Two execution paths is how a single
      effect becomes two; the test asserts the adapter's own call count, not a status.
      **⭐ A RESUMED RUN RE-ASSEMBLES ITS CONTEXT.** A resumed run begins with an empty loop state, so
      without re-assembly the model call would be sent *only* the observation — no policy, no objective, no
      history — and it would answer a message about a tool with no idea what was asked. The test asserts all
      three are present.
      **⭐ THE OBSERVATION IS FENCED, NOT A PROVIDER `tool` MESSAGE.** `tool_calls` stores no arguments
      (`0007`), so the assistant turn that requested a call is not reconstructible and a `tool` result
      cannot be correlated. The outcome is reported through `IsolatedText` (`ADR-0049`) — the vocabulary
      for content originating outside JARVIS. **A recorded limit, not an oversight.**
      **⭐ A RUN PARKED WITH NO PENDING CALL STAYS PARKED.** `recover_interrupted_runs` settles every
      non-terminal run at startup, so a parked run does not survive a restart; reaching the arm without a
      pending call means a caller re-drove a run it has no decision for. Advancing there would act on a
      decision never made. And the route's continuation spawns only when the run **is** parked, so a
      resume of a settled run is a no-op. **`ADR-0120`.** Gates: fmt, clippy `-D warnings`, `cargo test
      --workspace --all-features --locked` (adds 2 daemon park/resume tests, 112 daemon tests total), `cargo
      deny check` ok.
      **Limits, recorded rather than glossed:** the continuation is **in-memory**, so an approval decided
      *after a daemon restart* finds no parked run — persisted approval across a restart is the `P6`
      workflow slice; a resumed run **answers** over the observation rather than re-planning (so it cannot
      chain a second tool call within the same resume); and the observation is fenced data, not a provider
      `tool` message, per the `0007` decision.
<a id="p3-024"></a>

- [x] `P3-024` Compose the live model provider from configuration: a configured profile must answer a run
      with a real model instead of the scripted one.
      **THE `P3-022`/`P3-023` LIMIT, CLOSED.** Both slices recorded the same limit — *"no live provider has
      been called"*. The `openai-compatible` adapter existed and passed offline wire fixtures, but **nothing
      in a running daemon could select it**: `executor_model` was compared against one scripted constant, so
      a user could install JARVIS, hold a conversation, and receive a deterministic scripted answer with no
      configuration key that changed that.
      **Delivered:** `jarvis-storage`'s `DaemonConfig` gains `executor_base_url`, `executor_model_name`, and
      `executor_api_key_ref` with matching environment variables and three validation rules;
      `LIVE_PROVIDER_MODEL_NAME` (`"openai-compatible"`) is the single spelling that selects the adapter,
      exported so configuration validation and the composition root cannot disagree. `jarvisd` gains
      `compose_executor` and `compose_model_provider`; `Executor::build(name, provider)` dispatches on the
      name and validates each coordinate through the adapter's own constructors. The configured `model_id`
      is threaded through every executor entry point rather than hardcoded.
      **⭐⭐ THE KEY IS A FILE PATH, NEVER A VALUE.** `to_toml` round-trips a profile and `Debug` is derived,
      so a key **field** would be republished by the very operations that exist to *inspect* configuration
      — and a committed config would contain a live credential. `executor_api_key_ref` is a `PathBuf` read
      **once** at composition; the file's contents never become a `DaemonConfig` field, so they cannot reach
      `to_toml`, the derived `Debug`, or the SQLite profile row. **`ADR-0121`.**
      **⭐ A PARTIAL PROVIDER IS REFUSED, NOT COMPLETED FROM DEFAULTS.** Each field is independently optional,
      which is what makes a half-configured provider *start cleanly and fail on the first run with a vendor
      error instead of a local one*. `IncompleteModelProvider` refuses the live implementation without all
      three coordinates; `ModelProviderWithoutImplementation` refuses coordinates with no implementation —
      the opposite error, a settings block with no effect that makes an operator believe their setting is
      used. The default is "refuse to start", because a daemon whose provider is half-configured is a daemon
      whose answers cannot be trusted.
      **⭐ THE RELATIVE PATH IS REFUSED AT LOAD.** Resolved against a service's working directory, "which
      file?" would depend on the process; `RelativeModelApiKeyRef` answers it once, where the operator can
      see the error.
      **⭐ AN ERROR ABOUT A CREDENTIAL IS A PLACE A CREDENTIAL GETS PRINTED.** `InvalidProviderField` names
      the **field** and not the value, and `Executor`'s `Debug` is hand-written to redact it, so `{:?}` on
      the composed executor is safe by construction rather than by discipline. **Two tests pin this with
      distinct canaries**: a key that is actually a URL is refused while the error renders no part of it,
      and `format!("{executor:?}")` on a composed live executor contains the model id and no credential —
      the latter fails if the hand-written `Debug` is ever replaced by a derive.
      Gates: fmt, clippy `-D warnings`, `cargo test --workspace --all-features --locked` (adds 7 executor
      composition tests and 4 configuration tests; 119 daemon tests), `cargo deny check` ok.
      **Limits, recorded rather than glossed:** **no live provider was called by any test** — composition is
      proven against the scripted adapter and the adapter's own constructor validation, so a live smoke
      against Ollama or OpenAI remains opt-in and credential-gated; the key file is read **once at startup**
      and a rotation on disk needs a restart; there is **no keyring integration** (the path is a plain file
      path); there is **no per-model or per-session routing** (one composed executor, one credential); and
      `executor_model` selects the transport while `executor_model_name` is the id sent to the provider, so
      an operator must set both. Two stale statements were corrected alongside: the scripted model's answer
      text and the `SCRIPTED_MODEL_NAME` doc comment both told the reader to set `executor_model` "once a
      provider adapter is available", which this slice makes false.

## P3-025: Configurable tool approval policy

<a id="p3-025"></a>

- [x] `P3-025` Let an operator configure which tools need approval, and per tool.
      **THE VOCABULARY EXISTED AND THE SURFACE TO USE IT DID NOT.** The engine already honored a per-tool
      `ApprovalPolicy::Ask` (`P3-015`), a workspace could deny a tool by identifier, and `WorkspacePolicy::new`
      refused a threshold above its ceiling — but `WorkspacePolicy::default()` was constructed at the
      composition site, `denying()` had **no production caller**, and `config.toml` had no policy field at all.
      So every install decided every tool by the same compiled-in posture, and the "producer with no consumer"
      shape this repository removes was sitting in the authorization path.
      **Delivered:** `jarvis-core` gains `Risk` and `ApprovalPolicy`; `jarvis-tools` re-exports both and keeps
      the effect-floor binding; `WorkspacePolicy` gains `requiring(id, policy)`, `effective_approval`, and
      `approval_overrides`; `evaluate` applies the effective policy at the tool-policy step; `jarvis-storage`
      gains a `[policy]` section (`max_risk`, `approval_threshold`, `deny`, `[policy.approval]`); and
      `compose_workspace_policy` in `jarvisd` translates the document into the policy the pipeline decides
      with. **`ADR-0122`.**
      **⭐⭐ A SHARED DOMAIN TYPE NEEDED BY TWO ADAPTER CRATES BELONGS IN CORE, NOT IN EITHER.** `Risk` and
      `ApprovalPolicy` moved to `jarvis-core` because `repository-layout.md` allows an adapter to depend on
      core and **not on another adapter** — so `jarvis-storage`, which parses the operator's `max_risk`, could
      not name `jarvis-tools`'s type. The workaround would have been a second risk vocabulary in storage:
      **two values that must agree with nothing holding both**, where a drift would compile and an operator's
      configured ceiling would be silently ignored. Core already held `Sensitivity` for the same reason.
      `Risk::declared_for` now takes the floor as a **number** (an `EffectSet` is `jarvis-tools`'s type), and
      `jarvis-tools::declared_for_effects` is the single binding that supplies `EffectSet::risk_floor()`, so
      the rule keeps exactly one home.
      **⭐⭐ AN OVERRIDE IS A `max` APPLIED AT EVALUATION, NEVER AT CONSTRUCTION.** `requiring` stores what the
      operator wrote; `effective_approval(id, declared)` returns `declared.tighter(override)` inside
      `evaluate`. Applying the override when the policy is built would make the stored value the effective
      value, so the override would **replace** the declaration and a looser override would remove a guard —
      exactly what `ADR-0017` rejects by name. The rule lives with the policy, so a future runtime editor
      inherits it rather than reimplementing it.
      **⭐⭐ MY "SAFETY" TRANSLATION WAS ITSELF THE DEFECT.** The first `requiring` rewrote `Ask` into `Deny`,
      reasoning that an override must be unable to lose to a tool declaring `Deny`. But because the override
      is a `max`, `tighter(Deny, Ask)` is **already** `Deny` — no translation was needed — and the rewrite
      broke the common case, turning "hold this tool for approval" into "refuse this call outright". A test
      named for the *other* direction caught it: *"a stricter override must win, or the override does
      nothing"*. **A second guard that also changes the meaning is not a guard.**
      **⭐ A PREDICATE'S DIRECTION IS NOT INFERABLE FROM ITS NAME.** `tighter` reads correct whether it
      returns the stricter or the looser operand. `ApprovalPolicy::strictness()` (Auto 0 → Policy 1 → Ask 2 →
      Deny 3) is the mechanism, and the tests state the adversarial case **in their names** (*"a workspace
      override must not relax a tool that declares Ask"*, *"an override cannot make a denying tool
      runnable"*) plus a **sweep over every (declared, override) pair**, because a single hand-picked
      assertion written the wrong way round survives an inverted comparison. Both complements are asserted:
      the sweep proves nothing relaxes, and *"a stricter override wins"* is the control that stops "always
      return the declaration" from passing.
      **⭐ THE DOCUMENT IS A SEED, AND ITS ABSENCE IS THE DEFAULT.** `[policy]` is `#[serde(default)]`, so a
      document written before the section existed still loads — the safe direction, because an absent section
      means "no operator opinion" rather than a refusal to start. Two mistakes are refused as
      self-contradictions: a threshold above the ceiling (every risk that could be approved is already
      refused) and a blank identifier (applies to nothing while reading as a restriction). An entry naming
      an **unregistered** tool is deliberately **not** refused: MCP servers are discovered at startup and one
      may be down, so an inert entry is honest whereas a refusal would make a valid policy unable to start.
      Gates: fmt, clippy `-D warnings`, `cargo test --workspace --all-features --locked` (adds 8 core
      vocabulary tests, 6 direction-rule and override tests, 4 config tests, 5 composition tests; 124 daemon
      tests, `jarvis-tools` at 187), `cargo deny check` ok.
      **⭐⭐ A COMPOSITION-ORDER DEFECT, FOUND BY ASKING WHERE THE NEW CODE IS REACHED.** `compose_tool_pipeline`
      returned `Ok(None)` early when no roots and no MCP servers were configured, and the policy was built
      **after** that return — so a document with an unusable policy was **accepted and silently ignored** by
      exactly the daemons most likely to have a stale one. Building the policy before the return fixes it, and
      the test for it was proven to falsify: reintroducing the early return made the test fail. **A new
      validation on one path is not a validation unless you check which paths reach it.**
      **Limits, recorded rather than glossed:** the document is read **at startup** and there is **no runtime
      policy surface** — a CLI or control-plane UI that edits policy while the daemon runs needs a durable
      workspace row and a reload path, and that is the next slice; the tool-identifier vocabulary is split
      (`PolicyConfig` holds `String`s and the composition root parses them, because `ToolId` belongs to
      `jarvis-tools`), so a malformed identifier is a **startup** error rather than a parse error; an override
      can never relax a tool's declaration by design; and `[policy.approval]` cannot express "ask, whatever
      the tool declares" for a tool that declares `Deny`, because `Deny` is absolute.

## P3-026: The tool control plane

<a id="p3-026"></a>

- [x] `P3-026` Let an operator read the policy in force and preview a decision, from HTTP and the CLI.
      **THE QUESTION `P3-025` CREATED AND COULD NOT ANSWER.** `P3-025` made a tool's approval policy
      configurable, which made *"did my configuration take effect?"* answerable in principle and
      unanswerable in practice: a configuration document is the **input**, while the workspace policy is what
      `evaluate` enforces. An override is applied as a `max` at evaluation time, a denial short-circuits
      before several checks, and the actor's scopes are derived rather than configured — so a correct document
      still does not describe the posture in force, and the only way to learn it was to attempt a call and
      read the refusal. There was also **no `GET /tools` at all**: `ToolRegistry::discover()` existed with no
      route, so a CLI or control-plane UI could not enumerate what was configurable.
      **Delivered:** `jarvis-protocol` gains `tool_api` (`ToolReply`, `ToolListReply`,
      `ToolPreviewRequest`, `ToolPreviewReply`); `ToolPipeline` gains `policy_inventory`, `preview_call`, and
      `workspace_policy`; `jarvisd` gains `GET /api/v1/tools` and `POST /api/v1/tools/{tool}/preview`; the CLI
      gains a `jarvis tools <list|preview>` group with `--escalation`, `--channel`, and
      `--json`. **`ADR-0123`.**
      **⭐⭐ A PREVIEW IS A DECISION, NOT A PREDICTION, AND IT WRITES NOTHING.** `evaluate` is a pure function
      of declared facts, so `preview_call` calls the **same** function the tool-call path calls over the same
      workspace policy and definitions — the answer is exact for the context supplied rather than an estimate.
      It takes no `run_id` and touches no table, because a preview that recorded a call would let an operator
      fill the ledger by looking at it, and one that consumed an idempotency key would make the real call a
      duplicate. The property is asserted by **counting `tool_calls` rows** before and after, because a
      `requested` row would look identical in the preview's own response — which is why
      `jarvis_storage::count_tool_calls` was added: an assertion on the response cannot see the write.
      **⭐⭐ THE LIST REPORTS THE DECLARED AND THE EFFECTIVE POLICY, AND NAMES AN OVERRIDE AS ONE.** Both values
      are reported because they differ for a reason worth seeing: a tool held by the workspace *threshold*
      looks identical to one held by its own *declaration* unless both are visible. `overridden` is **derived**
      rather than stored, so the flag cannot disagree with the two policies it describes. The reply also
      carries the workspace ceiling and threshold, so one tool's posture is readable against the ceiling it
      sits under. **Falsified**: making the projection report `declared` as `effective` fails with
      `left: Auto, right: Ask`, so the assertion is not vacuous.
      **⭐ THE CLOSED SETS ARE `jarvis-core`'s TYPES ON THE WIRE, WHICH IS `ADR-0122`'s PAYOFF.** This crate
      depends on `jarvis-core` and **not on `jarvis-tools`**, so a `String` field plus a hand-written
      membership check was the only alternative — and three such checks were written and then **deleted** once
      the move made `Risk`, `ApprovalPolicy`, and `EscalationSignal` nameable. A mistyped policy is now a `422`
      naming the field rather than a default, and a default here would be the *most permissive* reading of a
      value the caller mistyped. `EscalationSignal` moved to core in this slice for exactly that reason.
      **⭐ THREE STATUSES STAY DISTINCT BECAUSE THE REMEDIES DIFFER.** An unknown tool is `404` rather than a
      refusal, because a typo and a policy denial have opposite remedies. No tool surface at all is `404`
      rather than an empty list, because "nothing is configured" and "this daemon cannot serve tools" are
      different deployment facts. A **denied** tool is still `callable: true`, because the denial is a policy
      fact rather than an availability one — conflating them would send an operator to fix a grant when the
      remedy is a configuration line.
      **⭐ A CALLER SUPPLIES CONTEXT; THE DAEMON SUPPLIES AUTHORITY.** A preview body carries only `channel` and
      `escalation` — the parts of the call only the caller knows. Scopes, the workspace
      policy, and the definition come from the daemon, and `deny_unknown_fields` makes an attempt to name them
      a `422` rather than an ignored value. A test asserts a body naming `scopes` is refused, so the preview
      cannot become an authorization oracle.
      **⭐⭐ A TEST CAUGHT AN OFF-BY-ONE IN THE CLI FLAG PARSER.** `preview_request` scanned from index 2, which
      is the **tool identifier** rather than the first flag — so every preview would have failed with
      `unknown tools option "jarvis.files.read"`. It was caught because the parser is tested **directly**
      rather than through the rendered output: a rendering test would have shown a plausible error and the
      cause would have looked like a bad identifier. Two more CLI tests assert that an unknown escalation,
      or channel is a **usage error** rather than an ignored flag, because silently dropping an
      escalation computes the preview for a call *without* it and reads as more permissive than the user asked
      about — with a `--json` control so a parser that refused every flag cannot pass.
      Gates: fmt, clippy `-D warnings`, `cargo test --workspace --all-features --locked` (adds 10 gateway
      route tests, 3 CLI parser tests, 3 core signal tests, and the storage count; 134 daemon tests, 50
      suites), `cargo deny check` ok.
      **Limits, recorded rather than glossed:** the surface is **read-only** — editing policy at runtime needs
      the durable workspace row and reload path `ADR-0122` records as absent; the list is **unbounded**,
      relying on `MAX_REGISTERED_TOOLS` rather than a page parameter; a preview **cannot express a
      hypothetical authority**, deliberately, since accepting scopes or a workspace policy would make it an
      authorization oracle; and a preview does **not** read a target out of the tool's arguments, so the
      escalation signals are the caller's to supply — the same limitation `call_remote_tool` records for the
      same reason. Two CLI observations checked rather than assumed: an **unknown sub-verb** reports daemon
      unreachability before naming the sub-verb, because the client is built before `tools::run` is called —
      **verified to match `jarvis memory` exactly**, so this is the established convention rather than a bug in
      the new group, and changing it here alone would make the two groups inconsistent; and `jarvis tools list`
      against a daemon with **no tool surface** exits through the `Denied` path, which is the documented
      mapping for a refusal (the `404` body's remedy text is what distinguishes it).

## P3-027: Web fetch tool

<a id="p3-027"></a>

- [x] `P3-027` Give the model a way to read the public web, behind the same policy gate as every other tool.
  - Delivered: `crates/jarvis-web` (new) with `jarvis.web.fetch`, composed in `jarvisd` beside the memory tool. A
    bare `GET` of a public `http`/`https` URL; the body comes back as fenced untrusted text. 29 tests in the crate,
    plus 2 through the real pipeline in `jarvisd` (held by default; unattended is still refused by the address guard).
  - Research: [web-fetch.md](../research/integrations/web-fetch.md). Decision: [ADR-0129](../adr/0129-a-fetch-is-checked-on-the-address-it-connects-to.md).
  - Boundary: addresses are checked after resolution (allowlist of global unicast, embedded IPv4 judged by what it
    embeds), the connection is pinned to the checked answers, every redirect hop is checked again, only ports 80/443,
    no credentials/headers/cookies/proxy/decompression, body read stops at 256 KiB.
  - **Held for approval by default** (`read_only`, risk 2, approval `policy`). Unattended use is the operator's opt-in
    (`policy.approval_threshold = "high"`).
  - Falsified: the address rule mutated to accept everything fails 7 tests, including the one asserting a loopback
    server received **zero** connections. Gates: fmt, clippy `-D warnings`, `cargo test --workspace` (2,023 tests),
    `cargo deny check`.
  - **Limits:** a model-chosen URL can still carry data out once approved or opted in (not solvable by the tool);
    the model sees at most 1,800 characters because the executor truncates every tool result to 4,000 (follow-up:
    per-tool result budget and an `offset` argument); non-UTF-8 charsets are decoded lossily; no live-internet test;
    `jarvis tools list` and the CLI were not exercised against a running daemon with this tool.

## P3-028: Approve from the CLI, and the defects that flow exposed

<a id="p3-028"></a>

- [x] `P3-028` Let a person see, decide and finish a held action from the shipped product. Found by driving a real
  model (Ollama `glm-5.3:cloud` through a local Ollama) and the real network through `jarvisd` and the CLI.
  - **Live result:** "fetch https://example.com" → model requests `jarvis.web.fetch` → run parks (CLI exit 11) →
    `jarvis approvals approve` → the page is really fetched → the model answers "Example Domain". Also verified live:
    deny, a refused loopback URL, and kill-and-restart of the daemon while an approval was pending.
  - **Four defects found by that walk, each fixed with a test that failed first or asserts the fix:**
    1. `jarvis-models`: **a stream whose events arrive in one network chunk was reported `incomplete`.** The SSE
       decoder returns one event per call and the adapter never drained it before awaiting the next chunk, so the
       finish reason and `[DONE]` sat unread. Every fixture sent one event per chunk. Test:
       `several_events_in_one_network_chunk_are_all_delivered`. *Every live run against Ollama failed before this.*
    2. **A denied approval left its run parked forever.** Now the executor tells the model it was declined and the
       run answers (`resume_run_declined`; test asserts the tool ran zero times).
    3. **A daemon with a parked run would not start** (recovery tried `awaiting_approval → failed`, which the state
       table forbids). Recovery now leaves a parked run alone. Amends `ADR-0013`.
    4. **A person could not see what they were approving, or release a held call without a client-side copy of its
       arguments.** A pending approval now holds its arguments until decided ([ADR-0130](../adr/0130-a-pending-approval-holds-what-it-is-waiting-on.md),
       migration `0013`, schema version 13).
  - Delivered: `GET /api/v1/approvals`; `jarvis approvals list|approve|deny`; `jarvis ask` exits 11 with the approval
    id instead of "stalled"; docs (`getting-started.md` was also stale about what exists).
  - **Limits:** an expired pending approval keeps its payload until a retention sweep exists; a declined call's
    `tool_calls` row stays `requested`; `approve` resumes the call from the arguments the CLI was shown, so a daemon
    restart *between* the decision and the resume left the call approved but not run (closed by `P3-030`); SQLite only (no Postgres `approvals` migration
    exists yet).

## P3-029: The model can run code, in a disposable container

<a id="p3-029"></a>

- [x] `P3-029` Give the model a way to calculate, behind the sandbox and the approval gate.
  - Delivered: `jarvis.code.run` (`apps/jarvisd/src/code_run.rs`), `LaunchedProcess::wait_for` (bounded wait that
    stops the work), operator config `daemon.code_sandbox_image` / `daemon.code_sandbox_interpreter`, a shared
    per-result budget (`MAX_MODEL_FACING_RESULT_CHARS` 12,000, from 4,000; the fetch cap rose from 1,800 to 4,000),
    and multi-line programs shown as lines by `jarvis approvals`. [ADR-0131](../adr/0131-model-authored-code-runs-in-a-disposable-container.md).
  - **Live:** asked for the sum of the first 100 primes, the model (Ollama `glm-5.3:cloud`) wrote JavaScript, I read and
    approved it, it ran in `node:22-alpine` with no network, and the answer (24,133) was right.
  - **Always held:** `code_execution` at risk 3 with `ApprovalPolicy::Ask`, which a workspace cannot relax
    (`ADR-0122`); test raises ceiling and threshold to `High` and still gets a hold.
  - **Falsified against a real runtime:** network blocked, root and `/tmp` read-only, host environment absent, a
    runaway snippet stopped and its container *removed* (asked of `docker ps`), unbounded output cut and still inside
    the budget, output unable to close its own fence. Tests skip loudly without a runtime or image.
  - **Limits:** the cgroup backend has no `wait_for` yet (the default refuses rather than waiting unbounded); no files in
    or out and no state between runs; limits (30 s, 64 processes, 256 MiB) are constants; a program too large for the
    8 KiB approval payload cannot be approved from the CLI; `jarvis.code.run` only exists when an image is configured,
    pulled, and a runtime is reachable.

## P3-030: An approved call survives a daemon restart

<a id="p3-030"></a>

- [x] `P3-030` Close the limit `P3-028` recorded: a daemon that died between a decision and the release left a call that
  was approved, never run, and impossible to finish.
  - Approved approvals keep their arguments until the call runs (denials clear them at once); the decision route takes
    an opt-in `resume: true` and releases the call from the held arguments; `POST /approvals/{id}/resume` and
    `jarvis approvals resume ID` finish one that was not released; `GET /approvals` lists `approved` ones. The CLI no
    longer re-sends any payload. Amends [ADR-0130](../adr/0130-a-pending-approval-holds-what-it-is-waiting-on.md).
  - **Live:** approve, kill the daemon, restart, `list` shows approved-not-run, `resume` fetches the page, run completes.
  - Tests: 3 gateway tests (release exactly once and arguments cleared; finish later without re-sending; a denial can
    never be released), 2 storage tests (approved keeps, denied clears).
  - **Limits:** nothing releases an approved call *automatically* at startup (a person runs `resume`); an approved
    approval whose call is never released keeps its payload until a retention sweep exists.

### P3-031: First run — `jarvis init` and `jarvis start`, and a multi-call run bug found by using it

- [x] A fresh install reaches a conversation in three commands. `init` detects a local Ollama, refuses a model it does
  not list, writes the key file and a configuration validated by the daemon's own parser, never overwrites without
  `--force`, and grants no folder unless one is named. `start` spawns the sibling `jarvisd` detached and waits until it
  listens, or reports that it exited. (`apps/jarvis-cli/src/init.rs`, 6 unit tests.)
- [x] **Defect found by the live run:** a second tool call in one run failed with "failed to admit a tool call". The
  call's identity is its correlation identifier, and the executor passed the run's single one to every call, so the
  second collided on `tool_calls.id`. Every earlier test and live run made one call per run. Fixed in
  `executor::run_tool_call` (a fresh identifier per call); regression test `a_run_can_make_two_tool_calls`,
  falsified by restoring the shared identifier.
  - **Live:** `init` → `start` → `ask` with no environment variables; the model listed a folder and read a file in one run.
  - **Limits:** `init` needs an existing `--root` directory; a hosted provider is checked only for a readable key file,
    not a live call; no Windows service or autostart.

### P3-032: Less asking — approval is for what can hurt, and the owner can decide once

<a id="jarvis-web-fetch"></a>

- [x] `jarvis.web.fetch` is risk 1 and runs under the default workspace (`ADR-0133`, amends `ADR-0129`). Residual
  exfiltration-by-URL risk is bounded (2,048-character URL cap, address rule, audit) and documented in `security.md`;
  `"jarvis.web.fetch" = "ask"` restores the hold.
<a id="-policy--trust--------"></a>

- [x] `[policy] trust = [...]` / `WorkspacePolicy::trusting`: the owner's advance decision waives a tool's `Ask`, the
  risk threshold — and not the ceiling, a denial, scopes or the external-communication
  rule; never consulted for externally communicating tools; a denial or approval override wins in either order.
<a id="jarvis-init---code-image-image----code-interpreter--node--e------trust-code-"></a>

- [x] `jarvis init --code-image IMAGE [--code-interpreter "node -e"] [--trust-code]` writes the code sandbox and, only
  on request, the trust.
  - Tests: `standing_trust_waives_the_ask_and_nothing_else` (7 limits), `a_trusted_code_tool_runs_without_a_hold`,
    `a_web_fetch_runs_by_default_and_an_ask_override_holds_it`, a composition test, 2 init tests.
  - **Live:** with `--trust-code`, one prompt fetched example.com and ran a prime-sum snippet in `node:22-alpine`
    (answer 5117, correct) with **no approval requested**.
  - **Limits:** no "always allow" at the approval prompt (needs a writable policy); trust is per tool, not per argument.

### P3-033: The kill switch — `jarvis cancel`, and a parked run that actually stops

<a id="jarvis-cancel-run"></a>

- [x] `jarvis cancel RUN` (identifier prefix; ambiguous or finished refused) and `jarvis cancel --all` (everything not
  yet settled). Reports the request, not a stop that has not happened.
- [x] **Defect found by the live run:** cancelling a run parked at `awaiting_approval` recorded a request nobody read (a
  parked run has no driver), so the run stayed in the approvals list for ever. `settle_parked_run_cancelled` now settles
  it directly (`awaiting_approval → cancelled`, refused for a run with a driver) and
  `withdraw_cancelled_run_approvals` expires its pending approvals (payload gone) and drops approved-but-
  unreleased arguments, so `resume` cannot run an action for a cancelled run. Restart recovery runs the same sweep, which
  also repaired the orphan the live run had already left. Amends `ADR-0013`.
  - Tests: storage `cancelling_a_parked_run_settles_it_and_a_running_one_is_refused`, gateway
    `cancelling_a_parked_run_withdraws_its_approval` (cancelled, unlisted, undecidable, tool never ran), 2 CLI selection tests.
  - **Limits:** a run in the middle of a model call stops at its next step boundary, not instantly; no `jarvis stop`
    for the daemon itself; no sub-agent supervision yet (nothing to supervise).

### P3-034: Sub-agents — delegation to a staff of ordinary runs

<a id="jarvis-agent-delegate"></a>

- [x] `jarvis.agent.delegate` (optionally `background`, for parallel work) and `jarvis.agent.result`
  (`apps/jarvisd/src/delegate.rs`, `ADR-0134`): a sub-agent is an ordinary run with the same policy, approvals, audit and
  budgets, visible in `jarvis runs` as `[sub-agent]` and stoppable with `jarvis cancel`; delegation is one level deep (the
  executor withholds `jarvis.agent.*` and the scope from a run whose objective carries the notice); at most 4 active; the
  answer is fenced untrusted data; `result` refuses any run that is not a sub-agent of the workspace; a parked
  sub-agent is reported as waiting for the person.
  - Tests (7): delegate end to end with a fenced answer, a sub-agent is offered and granted no delegation, `result`
    refuses an ordinary run, a parked sub-agent is reported waiting and not moved, definitions, marker, objective fit.
  - **Live:** two sub-agents in parallel with `background: true` (fetch a page; compute with code), both collected by the
    parent and reported correctly.
  - **Limits:** the parent is not notified when a parked sub-agent is later approved; sub-agents share the parent's
    model and tool set minus delegation; cost scales with the number of sub-agents (cap 4).

### P3-035: Visual presence, first step — `jarvis watch`

<a id="jarvis-watch----once-----interval-n-"></a>

- [x] `jarvis watch [--once] [--interval N]` (`apps/jarvis-cli/src/watch.rs`): one screen of the work, not the transcript —
  **waiting for you** (each approval with its tool, risk, arguments and the exact approve/deny commands), **working**
  (runs and sub-agents, state and age, with the cancel commands), **scheduled** (next fire and cadence) and **recent**
  (outcome and answer). Reads `/runs`, `/approvals` and `/schedules` and keeps no state of its own, so it cannot disagree
  with `jarvis runs`, `jarvis approvals` or `jarvis schedule`. Plain ASCII; prints once when output is not a terminal.
  - Tests (4): spans and clipping, idle screen, and the section order and content of a busy screen.
  - **Live:** during a parent run with two background sub-agents the screen showed all three working with ages and
    `[sub-agent]` labels, then the parked code run under WAITING FOR YOU with its approve/deny commands.
  - **Limits:** a terminal view, not yet the heads-up display the product is aiming at (no browser or overlay surface);
    no keypress approvals; no per-run event detail.

### P3-036: Visual presence — the heads-up display (`jarvis hud`)

- [x] The daemon serves a static display at `/` (alias `/hud`) (`apps/jarvisd/src/hud.rs`, `ADR-0135`): an orb that is idle, working,
  or amber and pulsing when something **needs you**, plus waiting-for-you (with the exact approve command and a copy
  button), working (with **Stop** per run and **Stop everything**), scheduled and recent. `jarvis hud` opens it with the
  credential in the URL fragment, which the page moves to session storage and removes from the address bar. No
  dependency, no build step.
  - Tests: public assets are exactly two `GET` paths and nothing else became public (route test, both sides, including a
    `POST` and sibling paths), the script never inserts markup / inline script / query credential, the fragment form.
  - **Live in a browser:** the page rendered the parked code approval and its run, Stop cancelled the run and the
    approval vanished, the orb returned to idle; a reload kept working from session storage.
  - **Limits:** polling every 1.5 s rather than a
    push stream; one workspace; no per-run event detail yet.
- [x] **Upgraded to a full console** (same slice): animated canvas orb whose waveform follows the mic or the spoken
  answer, a streaming conversation over the run's SSE (tool chips, safe markdown, session kept across turns), the four
  panels, keyboard shortcuts (`/`, `M`, `Esc`), `/hud.css` with `style-src 'self'` and a microphone-only
  `Permissions-Policy`. **Voice (first slice of `P8-010`, browser-native):** push-to-talk, wake word "Jarvis", spoken
  answers, interruption, and the hands-free kill switch ("Jarvis, stop"). Tests: no markup insertion, no outside
  origin, no `MediaRecorder`, no inline style. **Live:** a streamed answer with tool chips and two parallel sub-agents
  visible in the panels; the stale-cursor and raw-markdown-in-panel defects found by looking at it were fixed.
- [x] **The face** (2026-10-06): the orb became a film-style interface (concentric ticked rings, segmented arcs, name
  ring that fills one turn, radar sweep, radial voice spectrum, arc-reactor core, sight lines and brackets) in a
  three-column HUD layout with framed panels and a telemetry panel; state shown by colour and tempo. **Live:** idle and
  working states checked in a browser while two sub-agents ran; a name-ring overprint and an undersized face found by
  looking at it were fixed. See `ADR-0135`.
  - **Limits:** the microphone path could not be exercised here (no audio device); browser recognition may use the
    vendor's cloud (Chrome/Edge); no file attach; no spoken approval; Firefox has no speech recognition.

### P3-037: Light approvals — an approval is a yes or no from the owner

<a id="adr-0136"></a>

- [x] `ADR-0136`: deciding whether to ask stays deterministic policy (risk threshold, per-tool `ask`/`deny`, always-ask for
  tools that talk to other people, `trust`); answering is one click, one command or one spoken word from any surface that
  holds the local credential. `POST /api/v1/approvals/{id}/decision` takes `{ decision, resume, channel? }` and the bearer
  credential alone; an old-style body that still sends a code is refused with a validation error.
- [x] Removed from the code and the docs: the one-time decision code and its delivery file, the authentication-strength
  levels with the per-channel ceiling, and the distinct-approver rule. Kept: the digest that binds an approval to the
  exact arguments shown, arguments held until the call runs, expiry, withdrawal on cancel, restart survival, and a decision
  that records the outcome, the surface, the instant and a plain approver label. Three older database columns stay in the
  table (they cannot be dropped safely) and are written with fixed values in one isolated place.
- [x] Console: **Approve** / **Deny** buttons on the waiting panel, a spoken announcement when something new needs an answer
  (spoken answers on), and a bare "yes"/"no" that answers the single waiting question (several waiting: it asks you to use
  the buttons). A conversation that no longer exists starts a fresh one instead of failing.
  - Tests: storage approval tests restored and adapted after review found a dozen deleted tests of behaviour that stays;
    gateway tests for deciding with the credential alone, 401 without it, and refusing an old-style body; policy and
    pipeline tests updated. 2,103 tests pass.
  - **Live, real model:** a held fetch appeared amber with its exact arguments; one click on Approve ran it and the answer
    came back; a second held fetch was denied from the CLI and the model said it had not fetched the page. No approval-code
    directory exists on disk any more.
  - **Limits:** a spoken answer is as reliable as the browser's recognition (the microphone path could not be exercised
    here); with several approvals waiting a word cannot pick one; no per-surface restriction on who may answer, deliberately.

### P3-038: Write and edit files in granted folders

<a id="adr-0137"></a>

- [x] `ADR-0137`: `jarvis.files.write` (create or append, never replaces; runs by default) and `jarvis.files.edit`
  (replace one exact text; asked about once, waived by `trust`), both confined by the same directory handles as reads,
  content capped so a held call stays decidable, and not served to remote MCP callers.
- [x] A failed tool call now tells the model why (also after an approval), and a repeated identical request that was already
  answered is refused with a sentence instead of a database error (both found live).
  - Tests: create/parents/no-overwrite/append, traversal + absolute + link escapes for write and edit, oversize, unique
    replace, not-found/ambiguous/identical, CRLF, no temp file left, MCP exclusion, repeated request.
  - **Live, real model:** asked to create `shopping/list.txt` it ran without asking; an edit was held with its exact
    arguments, approved from the CLI and applied with no temp file left; an edit of text that is not there came back as
    "the text to replace was not found" and the model reported exactly that.
  - **Limits:** no delete/move/overwrite; one approval per edit; 4,000 characters per write call.

### P3-039: The console gets a real head, a "can do" panel, and opens on start

- [x] The face is now a real human head: MediaPipe's canonical face mesh (Apache-2.0, from upstream, recorded in
  `docs/research/integrations/mediapipe-canonical-face-model.md`; not copied from `example/`) drawn as a hologram in plain 2D
  canvas inside the existing rings, with a wire skull and neck. Jaw opens with speech, eyes glow and follow the pointer,
  brows lift when something needs you, it leans in while listening, offline it dims with eyes shut.
- [x] A "Can do" panel lists every tool as **runs**, **asks** or **off**. The daemon says which (`asks_first` on
  `GET /api/v1/tools`); a test pins it to `evaluate` over 180 effect/risk/approval/posture combinations (it caught a
  divergence: a workspace denial is checked apart from the approval policy).
<a id="jarvis-start"></a>

- [x] `jarvis start` opens the console (`--no-open` skips it); an already-running daemon just opens it.
  - Tests: head asset is public and `GET` only, pinned to 468 vertices / 898 in-range triangles with attribution kept,
    no outside origin or markup insertion; `asks_first` parity.
  - **Live, real daemon:** the head renders in all states (mouth open, amber brows, green lean); the panel showed 8 tools
    with only "Edit a file" asking.
  - **Limits:** the head is a mask plus wire skull (no hair or shoulders); speech mouth motion is a synthetic envelope, not
    phoneme-driven; no settings editing in the console yet (policy and folders are still `config.toml` / `jarvis init`).

### P3-040: A real voice (neural speech through the daemon)

<a id="adr-0138"></a>

- [x] `ADR-0138`: `crates/jarvis-voice` (ElevenLabs text-to-speech adapter, verified against the live docs), `GET/POST
  /api/v1/speech` behind the credential, `daemon.speech_api_key_ref` / `speech_voice_id` / `speech_model`, and
  `jarvis init --elevenlabs` (key from `ELEVENLABS_API_KEY`) / `--voice-key-file` / `--voice-id` / `--voice-model`.
- [x] The page plays the daemon's audio with the next sentence group made while one plays, interruption aborts it, the head's
  jaw follows the real audio level, and the browser voice is the fallback, now ranked sensibly (Natural/Online first,
  British male preferred, voices re-read as they load, no pitch hack) and honest when it only has a basic one.
  - Tests: adapter contract tests against a local fake provider (request shape, key never in errors or `Debug`, status
    mapping, bounds, malformed settings); route tests (401 without the credential, 404 when not configured, audio proxied,
    key never reported); config validation; init rendering and flags.
  - **Live:** the page path ran end to end with the speech endpoints mocked in the browser (violet speaking state, mouth open
    with the audio, a new message aborting the in-flight request); later against a real account (below).
- [x] Streamed speech (found by the user: the face "spoke" before any sound, and the voice lagged the text). The daemon passes
  the provider's `/stream` chunks through instead of collecting them (`SpeechStream`); the page speaks each sentence as soon
  as it is complete, plays progressively (Media Source, whole-blob fallback), keeps two pieces loading ahead, sends the previous
  sentence as `previous_text` (not for `eleven_v3*`, which refuses it), and shows "speaking" only while sound plays ("preparing
  voice" before). Measured live: first sentence complete at 2.6 s, request 2.64, first byte 2.92, playing 2.96, face "speaking"
  3.03 s. `eleven_v3` (set by mistake while chasing a stale tab) was 2.4 times slower and was put back to `eleven_v4_turbo`.
  Tests: first chunk available before the provider finishes; `previous_text` sent only to a model that accepts it, trimmed.
  Not done: the WebSocket input-streaming endpoint (a candidate, not a bottleneck).
  - **Limits:** text spoken leaves the machine when a key is set; per-character cost with no cap yet; regional endpoints are
    not configurable; speech to text is still the browser's.
