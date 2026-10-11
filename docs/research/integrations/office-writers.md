---
integration: office-writers
status: implemented
last_verified: 2026-10-11
owners: []
selected_spec_version: "ECMA-376 Office Open XML (SpreadsheetML, WordprocessingML); RFC 4180 for CSV"
selected_sdk: "rust_xlsxwriter 0.99.1 and docx-rs 0.4.22 (crates.io, pinned)"
---

# Writing Excel, Word and CSV files

## Scope

`P11-010` (`ADR-0185`): create a new `.xlsx`, `.docx` or `.csv` from structured content a model wrote. **Out of scope:** PDF and `.pptx` (open), charts, images, formulas, editing an existing file, styles beyond a header row and headings.

## Official Sources

| Source | URL/version | Accessed | Purpose |
| --- | --- | --- | --- |
| `llms.txt` | none for either crate (crates.io libraries); not applicable | 2026-10-11 | |
| `rust_xlsxwriter` | https://crates.io/api/v1/crates/rust_xlsxwriter (0.99.1, 2026-09-14, `MIT OR Apache-2.0`, 5.1 M downloads, repo github.com/jmcnamara/rust_xlsxwriter) and https://docs.rs/rust_xlsxwriter/0.99.1/rust_xlsxwriter/worksheet/struct.Worksheet.html | 2026-10-11 | API, licence, features, limits |
| `docx-rs` | https://crates.io/api/v1/crates/docx-rs (0.4.22, 2026-07-21, `MIT`, 3.6 M downloads, repo github.com/bokuweb/docx-rs) | 2026-10-11 | API, licence, dependencies |
| the format | ECMA-376 (the files are checked in the real applications, below) | 2026-10-11 | normative result |

## Verified Contract

- `rust_xlsxwriter`: default features are empty; the only required dependency is `zip ^8.3` (with deflate). `Worksheet::write(row, col, "text")` writes a string; a formula needs an explicit `Formula::new`. The **generic `write` was still avoided**: `write_string`, `write_number` and `write_boolean` are used so that a string can never become anything else. A string over 32,767 characters is an error (`MaxStringLengthExceeded`); a sheet name must be 31 characters at most and may not hold `[ ] : * ? / \`; a header or footer over 255 characters is dropped (not used).
- `docx-rs`: default feature `image` (an `image` dependency) is **off** here; requires `zip ^8.6`, `quick-xml`, `serde`, `base64`. Styles are declared by id (`Heading1`) and a numbered or bulleted list needs an abstract numbering and a numbering instance, both declared.
- Both pull `zip` 8; the workspace's reader uses `zip` 9. Two versions of one crate are compiled (`cargo deny` warns on duplicates and allows them). The cost is build size, accepted over hand-written OOXML.
- CSV: UTF-8, a byte-order mark and CRLF are what Excel needs to open accents correctly; the Dutch and German Excel list separator is `;`, so `delimiter` is a parameter.

## JARVIS Mapping

`jarvis-documents::{write_xlsx, write_docx, write_csv}` (pure: JSON in, bytes out, bounded) and the tool `jarvis.files.write_document` (risk 1, scope `files.write`, no approval, **creates only, never replaces**, inside a granted folder through the same confined handles as `jarvis.files.write`).

## Decisions

- **Libraries over hand-written XML**: a spreadsheet or document Office refuses to open is worse than a larger build, and these two are tested against Office by their authors.
- **Text is text.** Strings are written as strings; in CSV a value starting with `=`, `+`, `-`, `@`, tab or CR gets a leading apostrophe (the OWASP CSV-injection rule). No formulas are offered at all.
- **Limits**: 20 sheets, 200,000 cells, 100 columns, 32,000 characters a cell, 2,000 blocks in a document, 500 list items, 10 MB a file.
- A header row is bold, bordered, frozen and filtered; columns are sized to their longest line, up to 60 characters.

## Rejected Alternatives

Hand-written OOXML with the `zip` already in the tree (one version of zip, but no guarantee Office opens it); an MCP office server (a Node or Python runtime the owner would have to install); `printpdf` for PDF in this slice (fonts and Unicode need their own research).

## Verification Plan

- Done: 12 writer tests (read back with the independent reader, looking inside the archives for no `<f>` formula, sheet names, limits, markup escaping, CSV quoting) and 4 tool tests (formats, never replaces, bad paths and content write nothing).
- Done, live: a real model asked for a workbook and a report with a note starting `=`; both files were **opened in Microsoft Excel and Word 16** by COM automation with no repair: Excel showed the sheet, a bold header, the filter, the `=check later` note as text (`HasFormula` false); Word showed `Title` and `Heading 1` styles, three list paragraphs and one table.
- Open: LibreOffice and Google Sheets were not tried; PDF and PowerPoint are not written.

## Unresolved Questions

- Whether the header formatting survives Google Sheets import (cosmetic). Blocks nothing.
