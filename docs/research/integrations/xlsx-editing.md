---
integration: xlsx-editing
status: implemented
last_verified: 2026-10-15
owners: []
selected_spec_version: "ECMA-376 / ISO 29500 SpreadsheetML transitional; Microsoft Open XML SDK reference 3.0.1"
selected_sdk: "zip 9.0.1; roxmltree 0.21.1 (existing workspace dependencies)"
---

# Editing existing Excel workbooks without rebuilding them

## Scope

Extend `P11-010` with `jarvis.files.edit_spreadsheet`: bounded literal cell edits in an **existing** `.xlsx` inside granted folders. Preserve other worksheets, formatting, formulas and unknown ZIP parts. This is separate from the create-only `write_document` tool. No Excel process, network service, formula execution, row/column insertion, formatting changes, `.xls`, `.xlsm` or encrypted files.

## Official Sources

All sources below were fetched live on **2026-10-14 (UTC)** before implementation.

| Source | Exact URL/version | Purpose |
| --- | --- | --- |
| Repository `llms.txt` | https://raw.githubusercontent.com/zip-rs/zip2/v9.0.1/llms.txt and https://raw.githubusercontent.com/RazrFalcon/roxmltree/v0.21.1/llms.txt | Both 404; not found. Official versioned source and docs used instead. |
| Format index | https://ecma-international.org/publications-and-standards/standards/ecma-376/ | 403; normative excerpts in Microsoft's official reference below control the supported subset. |
| Package structure | https://learn.microsoft.com/en-us/office/open-xml/spreadsheet/structure-of-a-spreadsheetml-document | Workbook relationships identify worksheets, not assumed `sheet1.xml` numbering. |
| Cell schema and normative excerpt | https://learn.microsoft.com/en-us/dotnet/api/documentformat.openxml.spreadsheet.cell?view=openxml-3.0.1 and https://raw.githubusercontent.com/OfficeDev/open-xml-docs-ref-dotnet/live/docs/xml/DocumentFormat.OpenXml.Spreadsheet/Cell.xml | Cell `r`, `s`, `t`, `v`, `is`; `inlineStr` permits `is` rather than `v`/`f`. |
| Cell insertion/order | https://raw.githubusercontent.com/OfficeDev/open-xml-docs/main/docs/spreadsheet/how-to-insert-text-into-a-cell-in-a-spreadsheet.md | Insert cells in coordinate order, preserve existing rows/cells. |
| Worksheet contract | https://raw.githubusercontent.com/OfficeDev/open-xml-docs/main/docs/spreadsheet/working-with-sheets.md | Existing worksheets and attached objects remain package parts. |
| Formulas and caches | https://raw.githubusercontent.com/OfficeDev/open-xml-docs/main/docs/spreadsheet/working-with-formulas.md | `<f>` is formula text; `<v>` is only a cached result. This adapter does not calculate formulas. |
| Calculation chain | https://raw.githubusercontent.com/OfficeDev/open-xml-docs/main/docs/spreadsheet/working-with-the-calculation-chain.md | Chain lists formula cells; preserving formula cells allows retaining the chain. |
| ZIP API/source | https://docs.rs/zip/9.0.1/zip/write/struct.ZipWriter.html and https://raw.githubusercontent.com/zip-rs/zip2/v9.0.1/src/write.rs | `raw_copy_file` copies compressed contents and entry metadata **without CRC verification**; JARVIS must validate by fully reading first. |
| ZIP release notes | https://raw.githubusercontent.com/zip-rs/zip2/v9.0.1/CHANGELOG.md | 9.0.1 (2026-10-10): rejects uncompressed reads exceeding declared sizes, multiple archive/extraction fixes. No extraction used. |
| XML API/source | https://docs.rs/roxmltree/0.21.1/roxmltree/struct.Node.html and https://raw.githubusercontent.com/RazrFalcon/roxmltree/v0.21.1/src/lib.rs | Namespace-aware read-only XML tree, original node/attribute byte ranges for surgical edits. |
| XML release notes | https://raw.githubusercontent.com/RazrFalcon/roxmltree/v0.21.1/CHANGELOG.md | 0.21.1 fixes entity-resolver lifetimes; DTD disabled and no resolver supplied here. |
| ZIP central directory | https://pkware.cachefly.net/webdocs/casestudies/APPNOTE.TXT | EOCD section 4.3.16 count/offset bounds, little-endian fields; reject ZIP64/multi-disk and compare declared count to parsed unique entries. |
| Optional dimensions | https://learn.microsoft.com/en-us/dotnet/api/documentformat.openxml.spreadsheet.sheetdimension?view=openxml-3.0.1 | Normative excerpt: worksheet `dimension` is optional. Remove stale derived bounds on edited worksheets. |
| Excel string escaping source | https://raw.githubusercontent.com/jmcnamara/rust_xlsxwriter/v0.99.1/src/xmlwriter.rs | `_xHHHH_` literal sequences require protecting the leading underscore as `_x005F_`; CR is encoded `_x000D_`. No source copied. |

## Verified Contract

### Operations And Transport

Local ZIP and UTF-8 XML only. Resolve the workbook through `_rels/.rels`, and sheet names through the workbook's relationship IDs. Replace only requested literal cell values, adding absent cells/rows in order. Strings are inline strings, never formulas; numbers/booleans use typed values; null clears a value while retaining its cell style. Existing cell styles and untouched XML byte spans remain unchanged. Untouched ZIP members use raw compressed copying, not a lossy document reader/writer round trip. Workbook calculation flags request recalculation on next open; cached formula results can remain stale until a spreadsheet application recalculates.

### Authentication And Authorization

No provider credentials, OAuth, remote endpoints, callbacks or webhooks. Existing local authentication, `files.write` scope, granted-root confined handles and tool pipeline policy apply. Risk **2** and `ApprovalPolicy::Policy`, like destructive plain-text editing, ask by default; an owner may configure the existing standing policy. The model cannot bypass approval by calling the adapter in the normal dispatch path.

### Limits And Failure Semantics

JARVIS decisions: 1–500 edits, Excel A1 coordinates (A1–XFD1048576), explicit sheet names (1–31 characters), strings ≤32,000 Unicode scalars and 32,767 UTF-16 units, request ≤1 MiB of serialized edit data; input/output ≤10 MiB; ≤4,000 ZIP members; ≤24 MiB per decompressed member and ≤48 MiB total, counted during reads; XML ≤2,000,000 nodes, nesting ≤128. Archive names and internal relationship targets must remain canonical package paths. Reject duplicate/ambiguous names, overlapping entries, unsupported compression/encryption, malformed/DTD XML, signatures and macro workbooks. Reject protected worksheets, formula targets or formula result ranges, merged non-anchor cells and target-cell metadata/unsupported children rather than silently damaging features.

Validate the entire archive and every requested edit before staging any replacement. Existing confined atomic replacement writes a sibling staging file and renames only after success; validation errors leave the original byte-for-byte unchanged. No retries, no idempotency key, no automatic repair. A returned I/O failure is not success; concurrent edits retain the filesystem helper's documented semantics.

### Data And Compliance

No egress, subprocesses, additional service cost or persistent copies. The original file is replaced only after successful validation; no backup/history is promised. Cell values must not be copied into audit/error messages. Local file retention remains the owner's policy. `zip` is MIT; `roxmltree` is MIT OR Apache-2.0. No third-party implementation or examples copied.

### Versions And Deprecations

Reuse workspace-constrained `zip = 9.0.1` with only `deflate-flate2` and `roxmltree = 0.21.1`; no new crate ecosystem. Versioned source/changelogs checked; no relevant API deprecation observed. The daemon must promote its existing test-only ZIP dependency and add its already-workspace XML dependency as normal dependencies.

## XML depth boundary recheck (2026-10-15 UTC)

Narrow question: refuse excessive XML nesting before the selected parser consumes recursive stack, including untouched `.xml` and `.rels` members. No dependency/version, package format, authorization, egress or effect changes.

Live official sources re-fetched before implementation:

- https://raw.githubusercontent.com/RazrFalcon/roxmltree/v0.21.1/llms.txt — 404, no index.
- https://docs.rs/roxmltree/0.21.1/roxmltree/struct.ParsingOptions.html — only `allow_dtd`, `nodes_limit` and `entity_resolver`; no element-depth setting. Node-count limits bound allocation, not recursive stack.
- https://raw.githubusercontent.com/RazrFalcon/roxmltree/v0.21.1/src/tokenizer.rs — `parse_element` calls `parse_content`, which calls `parse_element` for each nested element. The existing post-parse ancestor check cannot protect this recursion. Stack overflow aborts rather than becoming a `catch_unwind` error.
- https://raw.githubusercontent.com/RazrFalcon/roxmltree/v0.21.1/src/parse.rs — DTD remains disabled and no entity resolver is supplied. The API documentation's empty-DTD exception conflicts with the pinned changelog's 0.19.0 fix; refuse declarations during preflight as well as retaining parser DTD refusal.
- https://raw.githubusercontent.com/RazrFalcon/roxmltree/v0.21.1/CHANGELOG.md — no relevant released fix or API deprecation in the selected version.
- https://raw.githubusercontent.com/RazrFalcon/roxmltree/master/CHANGELOG.md — an **unreleased** fix replaces nested-element recursion with iteration. This is not an available pinned-version guarantee and is not adopted.

JARVIS decision: use an allocation-free, iterative lexical preflight before `Document::parse_with_options`. Count element openings (including empty elements), skip quoted attributes, comments, CDATA and processing instructions, and reject declarations. The parser still performs authoritative XML/namespace validation with `allow_dtd: false`, no resolver and the unchanged 2,000,000-node ceiling. Retain the existing tree-node ancestor limit as a second check, including text/comment/PI nodes. Never enlarge stacks or ignore tests.

Falsification plan: run over-limit input at 129, 130 and thousands of element levels on an explicitly small stack inside a subprocess. Require the exact depth refusal and successful child completion; an overflow is a parent-test failure, not an abort of the entire test suite. Check the inclusive 128-element boundary, empty-element counting, XML lexical decoys and malformed markup, and retain package-wide rejection tests. Mutate away only the preflight call in an isolated build to prove the new guard test fails against the old parser ordering. Native macOS remains required to verify the original hosted runner; bounded-stack Windows execution is portable regression evidence, not a macOS run.

## Allowed-depth stack budget correction (2026-10-15 UTC)

CI `38738538886`, macOS job `116433947041`, proves that even **allowed** nesting at the 128-element boundary can overflow
roxmltree 0.21.1 on the caller's stack. The previous preflight fixes over-limit input but does not bound the recursive
parser's stack consumption for accepted input; its old small-stack test exercised only refusals and missed this case.

Official sources re-read live before this repair:

- https://raw.githubusercontent.com/RazrFalcon/roxmltree/v0.21.1/llms.txt — 404; no index.
- https://raw.githubusercontent.com/RazrFalcon/roxmltree/v0.21.1/src/tokenizer.rs — nested `parse_element` / `parse_content` calls recurse.
- https://docs.rs/roxmltree/0.21.1/roxmltree/struct.ParsingOptions.html — no recursion/stack-budget option.
- https://raw.githubusercontent.com/RazrFalcon/roxmltree/v0.21.1/CHANGELOG.md and https://raw.githubusercontent.com/RazrFalcon/roxmltree/master/CHANGELOG.md — iterative parsing remains an unreleased change; do not adopt a floating Git dependency.
- https://doc.rust-lang.org/1.98.1/std/thread/struct.Builder.html#method.stack_size — explicit stack size in bytes; the platform may allocate a larger minimum.
- https://doc.rust-lang.org/1.98.1/std/thread/struct.Builder.html#method.spawn_scoped and https://doc.rust-lang.org/std/thread/fn.scope.html — safe scoped borrowing, `Send` return values, OS thread-creation failure returned as `io::Result`, and manual join allows panic-to-error mapping.

JARVIS decision: preserve the existing 128-level/node/DTD/package limits and give the selected recursive parser its own
fixed 16 MiB scoped worker stack, joined before returning the borrowed document. The allocation-free preflight remains
on the caller; rejected nesting never starts a worker. No global or CI/test-runner stack setting is changed, no depth
allowance is raised, and no dependency or SDK version changes. This supersedes the earlier blanket stack-enlargement
rejection: an adapter-owned finite parser budget, unlike raising `RUST_MIN_STACK`, also protects small-stack production
callers while keeping previously accepted workbooks accepted. Thread creation or parser panic fails closed before any
file replacement; errors do not contain workbook data. There is one joined worker per parsed part, not a detached task,
new process or background service. Peak reserved parser stack is fixed per concurrent parse; thread-start overhead is
accepted for this bounded local operation until a released iterative parser can replace the worker.

Cheapest falsifier: run **both valid boundary and rejected over-limit fixtures** from a 256 KiB caller stack in a child
process. Before the worker fix, accepted-depth recursion must fail this regression without aborting the parent harness;
afterward, valid 127/128-element forms and lexical decoys must still parse, while depth/DTD/malformed input still refuses.
A native macOS run remains required; local Windows execution is not claimed as native macOS evidence.

## JARVIS Mapping

`document_write_tool::EditSpreadsheetTool` owns definition, schema, confined file replacement and bounded output. A dedicated pure workbook helper owns parser types and surgical ZIP/XML edits; no provider types cross domain boundaries. Output is a receipt (path, edit count, bytes, recalculation notice), not workbook contents. Registration/executor wiring remains with the composition root.

## Decisions

- Surgical XML byte-range edits and raw ZIP copying, preserving unsupported parts rather than recreating a workbook.
- Literal edits only; refuse to change formula cells or shared/array/data-table result ranges. Preserve existing formulas and their calculation chain.
- Fail closed on unsupported trust-sensitive workbook features rather than silently dropping them.
- Request workbook recalculation without evaluating formulas or following external relationships.
- Separate risk-2 edit tool; retain create-only tool's risk-1/non-replacement behavior.

## Rejected Alternatives

- `rust_xlsxwriter` reconstruction: create-only library loses charts, styles, unknown parts and other existing workbook content.
- Office/LibreOffice automation: external runtime/process lifecycle, unavailable on many installations, substantially larger trust boundary.
- A new all-purpose workbook SDK: unnecessary dependency footprint for a deliberately narrow literal-cell operation.
- In-place ZIP writes before validation: can corrupt the original on a late validation/I/O error.

## Verification Plan

**Cheapest falsifying test:** edit one styled cell in a fixture containing two worksheets, shared strings, formulas and an opaque custom binary part; compare every untouched member's decompressed **and compressed** bytes, and untouched worksheet XML spans. This disproves the central preservation claim if anything is rebuilt/dropped.

- Positive: typed values, string escaping/formula-looking text, clear values with styles, missing cells/rows, prefixed namespaces, relationship-based sheet routing, independent existing document reader.
- Fail closed: malformed package/XML, archive/path traversal, DTD, archive bounds, duplicate/overlapping entries, invalid coordinates/duplicate edits, missing sheet, protected/formula/array/merged targets. Tool tests prove every failure leaves original bytes unchanged and no sibling staging file remains.
- Tool metadata and JSON schema: scoped write, risk 2, policy approval and finite bounds; existing create-only tests stay green.
- Integration: focused daemon tests, formatter, focused lint, then parent's workspace gates.
- Opt-in application check: open a copied edited fixture in real Microsoft Excel/LibreOffice, verify no repair and preserved formula/style/chart behavior. **Not run** unless explicitly recorded below. Synthetic fixtures/reader tests do not prove real Excel compatibility.

## Unresolved Questions

- Real Excel/LibreOffice has not yet opened files edited by this new path; blocks a live-application compatibility claim, not the bounded offline implementation.
- Strict SpreadsheetML, digitally signed packages, macros, formula editing, protected sheets and concurrent-writer coordination are deliberately unsupported or remain the underlying filesystem helper's responsibility.

## Verification Log

| Date | Versions checked | Evidence | Researcher |
| --- | --- | --- | --- |
| 2026-10-15 | roxmltree 0.21.1; Rust 1.98.1 scoped threads | Expanded the bounded-stack child with accepted-depth boundary/lexical cases; before the worker repair, exact regression failed with child stack overflow `0xc00000fd` while the parent returned ordinary exit 101. With the fixed 16 MiB parser budget, `cargo test -p jarvisd --lib document_write_tool::xlsx_workbook::tests::xml_depth_is_refused_before_recursive_parser_on_bounded_stack --all-features --locked -- --exact --nocapture` **passed**; `cargo test -p jarvisd --lib document_write_tool --all-features --locked -- --test-threads=1` **31 passed**. `cargo fmt --check` and `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` **passed**; `cargo test --workspace --all-features --locked --no-fail-fast` **2,932 passed, 0 failed, 1 ignored**. Native macOS rerun pending; no global stack or CI gate change. | Copilot |
| 2026-10-15 | roxmltree 0.21.1; existing zip 9.0.1 | `cargo test -p jarvisd --lib document_write_tool -- --test-threads=1`: **31 passed**, including 22 workbook tests, exact 128/129-element boundaries, lexical decoys, untouched XML/rels refusal and the 256 KiB stack/subprocess guard. Isolated `clippy-driver --test --edition=2024 - --crate-name xlsx_depth_guard -L dependency=target\debug\deps --extern "zip=$zip" --extern "roxmltree=$xml" --extern "serde_json=$json" -W clippy::all -W clippy::pedantic -D warnings -o target\debug\xlsx-depth-guard.exe`: **passed**, with stdin `#[path = r"<absolute workbook source path>"] mod xlsx_workbook;` and `$zip`, `$xml`, `$json` selecting existing workspace rlibs. `target\debug\xlsx-depth-guard.exe --test-threads=1`: **22 passed**. Isolated old-order mutation (remove only `check_xml_depth(text)?;`, no production-file mutation): new regression **failed as intended**, child stack overflow `0xc00000fd`, parent test exit 101 without aborting the suite. `rustfmt --edition 2024 --check apps\jarvisd\src\xlsx_workbook.rs apps\jarvisd\src\xlsx_workbook_tests.rs` and scoped `git diff --check`: **passed**. `cargo clippy -p jarvisd --lib --tests -- -D warnings`: **blocked by unrelated concurrent memory code**, `clippy::cast_possible_truncation` at `crates\jarvis-storage\src\memory_recall_repository.rs:87`; untouched. Windows execution only; native macOS rerun still required. | Copilot |
| 2026-10-14 | zip 9.0.1; roxmltree 0.21.1; Open XML SDK reference 3.0.1 | Live official docs/source/changelogs reviewed before implementation. `cargo test -p jarvisd document_write_tool --lib -- --test-threads=1`: **28 passed**, including 19 pure package tests and 9 tool/creation tests. Isolated helper `clippy-driver --test --edition=2024 - --crate-name xlsx_workbook_tests -L dependency=target\debug\deps --extern "zip=$zip" --extern "roxmltree=$xml" --extern "serde_json=$json" -W clippy::all -W clippy::pedantic -D warnings -o target\debug\xlsx-workbook-tests.exe`: **passed** (stdin wrapper includes `xlsx_workbook.rs`; variables select existing pinned workspace rlibs), then `target\debug\xlsx-workbook-tests.exe --test-threads=1`: **19 passed**. Owned files formatted. Workspace `cargo fmt --check` blocked by concurrent parent-owned Google test formatting; `cargo clippy -p jarvisd --lib --all-features -- -D warnings` blocked by parent-owned ACP `question_mark` lint at `acp.rs:1016`. Composition-root registration/index links are parent-owned. No live Excel/LibreOffice check performed. | Copilot |
