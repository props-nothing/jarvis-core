# ADR-0190: PDF files are written with the standard fonts and no new dependency

Status: Accepted
Date: 2026-10-11

## Context

`ADR-0185` let JARVIS write `.xlsx`, `.csv` and `.docx` and left `.pdf` open (`P11-010`): the owner asks for quotations and reports that are sent to customers, and a PDF is what gets sent. PDF crates bring a dependency tree and need a font file for any layout, and a font that does not cover the owner's language is worse than none.

## Decision

1. **`jarvis.files.write_document` accepts `.pdf`** with the same `title` and `blocks` a `.docx` takes (headings, paragraphs, bullet and numbered lists, tables).
2. **No new dependency.** The file is built with `lopdf`, already here to read PDFs: A4 pages, the standard fonts Helvetica and Helvetica-Bold (present in every reader, so nothing is embedded), `WinAnsiEncoding`. Line widths come from Adobe's published AFM metrics, generated into `pdf_metrics.rs`.
3. **Layout is plain and predictable**: wrapped text within the margins (a word wider than the line is cut, no character lost), a heading is never the last thing on a page, a table has a shaded header that repeats on each page it runs onto, a cell shows at most 50 lines, "Page n of m" at the foot, at most 300 pages.
4. **Text is drawn, never interpreted**: parentheses, backslashes and non-ASCII bytes are escaped in the content stream; no action, script or link object is ever written.
5. **What the fonts cannot show is said, not hidden.** A character outside `WinAnsiEncoding` is drawn as `?` and counted; the tool answer carries `replaced_characters` and a note telling the model to tell the user or write a `.docx` instead.
6. The file is created only, never replaces one, and stays inside a granted folder, like the other formats.

## Consequences

Tests: 8 writer tests and one tool test (see `docs/research/integrations/office-writers.md`). Run live with a real model, and the file rendered by pdf.js: the Dutch quotation drew correctly, including the euro sign, bullets, numbers and a table with a shaded header.

Not done: other scripts (a font would have to be embedded: a licensed font file and subsetting), right-to-left text, images and charts, links, a table cell taller than 50 lines (cut with an ellipsis), `.pptx`. Not opened in Acrobat.
