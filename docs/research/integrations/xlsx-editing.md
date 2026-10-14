---
integration: xlsx-editing
status: implemented
last_verified: 2026-10-14
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
| 2026-10-14 | zip 9.0.1; roxmltree 0.21.1; Open XML SDK reference 3.0.1 | Live official docs/source/changelogs reviewed before implementation. `cargo test -p jarvisd document_write_tool --lib -- --test-threads=1`: **28 passed**, including 19 pure package tests and 9 tool/creation tests. Isolated helper `clippy-driver --test --edition=2024 - --crate-name xlsx_workbook_tests -L dependency=target\debug\deps --extern "zip=$zip" --extern "roxmltree=$xml" --extern "serde_json=$json" -W clippy::all -W clippy::pedantic -D warnings -o target\debug\xlsx-workbook-tests.exe`: **passed** (stdin wrapper includes `xlsx_workbook.rs`; variables select existing pinned workspace rlibs), then `target\debug\xlsx-workbook-tests.exe --test-threads=1`: **19 passed**. Owned files formatted. Workspace `cargo fmt --check` blocked by concurrent parent-owned Google test formatting; `cargo clippy -p jarvisd --lib --all-features -- -D warnings` blocked by parent-owned ACP `question_mark` lint at `acp.rs:1016`. Composition-root registration/index links are parent-owned. No live Excel/LibreOffice check performed. | Copilot |
