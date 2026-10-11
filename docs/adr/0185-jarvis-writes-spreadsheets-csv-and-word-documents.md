# ADR-0185: JARVIS writes spreadsheets, CSV and Word documents

Status: Accepted
Date: 2026-10-11

## Context

`P11-010`, wishes 6, 10 and 15 of the capability wishlist. JARVIS could read PDF, Word, spreadsheet and slide files (`ADR-0158`) but not write one: the sales project produced its Excel file with a hand-written script. Research: `docs/research/integrations/office-writers.md`.

## Decision

1. **`jarvis.files.write_document`**, a native tool (risk 1, scope `files.write`, no approval). The extension chooses the format: `.xlsx`, `.csv` or `.docx`. Content is a JSON description: sheets of rows with an optional header; or a title and blocks (headings 1 to 3, paragraphs, bullet and numbered lists, tables).
2. **It creates a new file and never replaces one**, through the same confined handles as `jarvis.files.write`; an existing name is refused with "choose another name". That is why it needs no more approval than a plain file write: it adds a file inside a folder the owner granted and destroys nothing. It is offered when a folder is granted, like the reading tool.
3. **The writers live in `jarvis-documents`** (pure: JSON in, bytes out, bounded; no files, no network) on `rust_xlsxwriter` 0.99.1 and `docx-rs` 0.4.22, both pinned, optional features off. They pull a second version of `zip`; accepted over hand-written Office XML, whose only test would be hoping Office opens it.
4. **Text is text.** Every string is written as a string: `write_string`, never a formula; there is no formula input at all. In CSV a value starting with `=`, `+`, `-`, `@`, tab or CR gets a leading apostrophe. CSV is UTF-8 with a byte-order mark and CRLF, with a `delimiter` choice (`;` for Dutch and German Excel).
5. **Bounds**: 20 sheets, 200,000 cells, 100 columns, 32,000 characters a cell, 2,000 blocks, 500 list items, 10 MB; sheet names made valid and unique; building runs off the async threads.

## Consequences

12 writer tests (read back by the independent reader, no `<f>` in the sheet XML, sheet names, limits, markup escaped) and 4 tool tests. **Run live and opened in the real applications:** a real model, asked for a prospects workbook with a note starting `=` and a report with a heading, paragraph, bullets and a table, called the tool twice; Microsoft Excel and Word 16, driven by COM, opened both without a repair prompt. Excel: the sheet `Prospects`, a bold header, the filter on, the `=check later` note as text. Word: `Title` and `Heading 1` styles, three list paragraphs, one table.

Not done: PDF and PowerPoint (a font and layout question, `P11-010` stays open), charts, images in documents, editing an existing file (use read, then write a new one), formulas by design, LibreOffice and Google Sheets (not tried). The sales project's own spreadsheet script is not yet replaced: the owner's existing scheduled task still runs it.
