# Edit an existing spreadsheet

`jarvis.files.edit_spreadsheet` changes literal cells in an existing `.xlsx` **inside a granted folder**. Unlike `jarvis.files.write_document`, it replaces the existing file. It is a risk-2 write: the normal workspace approval policy applies (asks by default).

Example tool arguments:

```json
{
  "path": "reports\\prospects.xlsx",
  "edits": [
    { "sheet": "Prospects", "cell": "B2", "value": "Utrecht" },
    { "sheet": "Prospects", "cell": "C2", "value": 125.5 },
    { "sheet": "Prospects", "cell": "D2", "value": true },
    { "sheet": "Prospects", "cell": "E2", "value": null }
  ]
}
```

Read the workbook first, then use its **exact worksheet names** and uppercase Excel coordinates. Missing cells/rows can be added; a missing file or worksheet cannot. `null` clears a value while retaining the cell's formatting. Text starting with `=` stays text, not a formula.

Other worksheets, cell styles, formulas, charts and unknown archive parts are retained rather than recreated. The tool edits worksheet XML surgically and raw-copies untouched ZIP members. Edited worksheets' optional used-range and row-span hints are removed so they cannot become stale. New cells inherit the workbook's normal row/column/default formatting; the tool does not choose new styles.

## Limits and refusals

- At most **500 distinct cells**, **1 MiB** of edits and **10 MiB** per workbook.
- Cell text: at most **32,000 characters**, also within Excel's **32,767 UTF-16-unit** limit.
- Only plain `.xlsx`: not `.xls`, `.xlsm`, password-encrypted or digitally signed files.
- Formula cells and formula result ranges, protected worksheets, non-anchor merged cells and cells with unsupported metadata are refused. Editing a formula is not supported.
- Malformed, ambiguous or excessive archives/XML and paths outside the granted folders are refused. Unsupported structural features may also be refused rather than damaged.
- A validation failure preserves the original bytes. Successful changes are staged beside the workbook and atomically replace it; no backup or undo history is created. Keep an independent backup when needed, and avoid concurrent writers.

**Formulas are preserved, not calculated.** The workbook is marked for full recalculation when opened. The owner's automatic/manual calculation setting is retained; manual-mode workbooks may need an explicit recalculation in Excel. Cached formula results shown by JARVIS's document reader can remain stale until then. Existing external links are preserved but never followed by this tool.

Close the workbook in Excel if Windows refuses replacement. This operation does not use Excel/COM. Offline preservation and rejection tests are not evidence of a successful live Excel or LibreOffice opening; that compatibility check remains unverified for this new edit path.

For a new workbook, CSV, Word document or PDF, continue to use `jarvis.files.write_document`; its existing create-only behavior is unchanged.
