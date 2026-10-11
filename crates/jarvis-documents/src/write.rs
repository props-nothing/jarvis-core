//! Writing documents from structured content: `.xlsx`, `.csv` and `.docx` (`P11-010`, `docs/research/integrations/office-writers.md`).
//!
//! Content arrives as JSON a model wrote, so it is checked and bounded before anything is built: how many sheets, rows, cells and blocks,
//! how long a cell, what a sheet may be called. Nothing here touches a file; bytes come out and the caller decides where they go.
//!
//! # Text is text
//!
//! Every string a model hands over is written as **text**, never as a formula. In a spreadsheet that is `write_string`, not the crate's
//! generic `write`; in a CSV, a value that starts with `=`, `+`, `-`, `@`, a tab or a carriage return gets a leading apostrophe, because
//! a spreadsheet program would otherwise run it. A number is only a number when the content says it is one.

use std::io::Cursor;

use docx_rs::{
    AbstractNumbering, BreakType, Docx, IndentLevel, Level, LevelJc, LevelText, NumberFormat,
    Numbering, NumberingId, Paragraph, Run, SpecialIndentType, Start, Style, StyleType, Table,
    TableCell, TableRow,
};
use rust_xlsxwriter::{Format, FormatBorder, Workbook};
use serde_json::Value;
use thiserror::Error;

/// The most sheets in a workbook.
const MAX_SHEETS: usize = 20;
/// The most cells (rows times columns) in one document.
const MAX_CELLS: usize = 200_000;
/// The most columns in a sheet or a table.
const MAX_COLUMNS: usize = 100;
/// The most characters in a cell. Excel's own limit is 32,767.
const MAX_CELL_CHARS: usize = 32_000;
/// The most blocks in a Word document.
const MAX_BLOCKS: usize = 2000;
/// The most items in one list.
const MAX_ITEMS: usize = 500;
/// The widest a column is made, in characters.
const MAX_WIDTH: f64 = 60.0;

/// Why content could not be written.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum WriteError {
    /// The content does not have the shape asked for. The message says which part.
    #[error("{0}")]
    Invalid(String),
    /// The content is larger than one document may be.
    #[error("{0}")]
    TooLarge(String),
    /// The writer itself failed.
    #[error("the document could not be built: {0}")]
    Engine(String),
}

fn invalid(message: impl Into<String>) -> WriteError {
    WriteError::Invalid(message.into())
}

/// One cell, already checked.
enum Cell {
    Empty,
    Text(String),
    Number(f64),
    Bool(bool),
}

/// A table of rows with an optional header.
struct Grid {
    name: String,
    header: Vec<String>,
    rows: Vec<Vec<Cell>>,
}

fn parse_cell(value: &Value, place: &str) -> Result<Cell, WriteError> {
    match value {
        Value::Null => Ok(Cell::Empty),
        Value::Bool(flag) => Ok(Cell::Bool(*flag)),
        Value::Number(number) => number
            .as_f64()
            .filter(|number| number.is_finite())
            .map(Cell::Number)
            .ok_or_else(|| invalid(format!("{place} is not a usable number"))),
        Value::String(text) => {
            if text.chars().count() > MAX_CELL_CHARS {
                return Err(WriteError::TooLarge(format!(
                    "{place} is longer than {MAX_CELL_CHARS} characters"
                )));
            }
            Ok(Cell::Text(text.clone()))
        }
        _ => Err(invalid(format!(
            "{place} must be text, a number, true or false, or null"
        ))),
    }
}

fn parse_grid(value: &Value, fallback_name: &str, cells: &mut usize) -> Result<Grid, WriteError> {
    let object = value
        .as_object()
        .ok_or_else(|| invalid("a sheet must be an object with header and rows"))?;
    let name = object
        .get("name")
        .and_then(Value::as_str)
        .map_or_else(|| fallback_name.to_owned(), sheet_name);
    let header: Vec<String> = match object.get("header") {
        None | Some(Value::Null) => Vec::new(),
        Some(Value::Array(items)) => items
            .iter()
            .enumerate()
            .map(
                |(index, item)| match parse_cell(item, &format!("header column {}", index + 1))? {
                    Cell::Text(text) => Ok(text),
                    Cell::Number(number) => Ok(number.to_string()),
                    Cell::Bool(flag) => Ok(flag.to_string()),
                    Cell::Empty => Ok(String::new()),
                },
            )
            .collect::<Result<_, _>>()?,
        Some(_) => return Err(invalid("header must be a list of column names")),
    };
    let rows_value = object
        .get("rows")
        .and_then(Value::as_array)
        .ok_or_else(|| invalid("a sheet needs rows: a list of rows, each a list of cells"))?;
    let mut rows = Vec::with_capacity(rows_value.len());
    let mut widest = header.len();
    for (row_index, row) in rows_value.iter().enumerate() {
        let items = row
            .as_array()
            .ok_or_else(|| invalid(format!("row {} must be a list of cells", row_index + 1)))?;
        widest = widest.max(items.len());
        *cells += items.len();
        if *cells > MAX_CELLS {
            return Err(WriteError::TooLarge(format!(
                "more than {MAX_CELLS} cells; split it across several documents"
            )));
        }
        rows.push(
            items
                .iter()
                .enumerate()
                .map(|(column, item)| {
                    parse_cell(
                        item,
                        &format!("row {} column {}", row_index + 1, column + 1),
                    )
                })
                .collect::<Result<Vec<_>, _>>()?,
        );
    }
    if widest > MAX_COLUMNS {
        return Err(WriteError::TooLarge(format!(
            "more than {MAX_COLUMNS} columns"
        )));
    }
    *cells += header.len();
    Ok(Grid { name, header, rows })
}

/// A name Excel accepts: at most 31 characters, none of `[ ] : * ? / \`, not empty.
fn sheet_name(raw: &str) -> String {
    let cleaned: String = raw
        .chars()
        .filter(|character| {
            !matches!(character, '[' | ']' | ':' | '*' | '?' | '/' | '\\')
                && !character.is_control()
        })
        .take(31)
        .collect();
    let trimmed = cleaned.trim().trim_matches('\'').to_owned();
    if trimmed.is_empty() {
        "Sheet".to_owned()
    } else {
        trimmed
    }
}

/// The sheets of a workbook: `sheets`, or one sheet given by `header` and `rows` directly.
fn parse_sheets(content: &Value) -> Result<Vec<Grid>, WriteError> {
    let mut cells = 0;
    if let Some(list) = content.get("sheets") {
        let list = list
            .as_array()
            .filter(|list| !list.is_empty())
            .ok_or_else(|| invalid("sheets must be a non-empty list"))?;
        if list.len() > MAX_SHEETS {
            return Err(WriteError::TooLarge(format!(
                "more than {MAX_SHEETS} sheets"
            )));
        }
        let mut grids = Vec::with_capacity(list.len());
        for (index, sheet) in list.iter().enumerate() {
            let mut grid = parse_grid(sheet, &format!("Sheet{}", index + 1), &mut cells)?;
            // Two sheets cannot share a name; the later one gets a number.
            let taken = |name: &str, grids: &[Grid]| {
                grids
                    .iter()
                    .any(|other| other.name.eq_ignore_ascii_case(name))
            };
            let base = grid.name.clone();
            let mut attempt = 2;
            while taken(&grid.name, &grids) {
                let suffix = format!(" {attempt}");
                grid.name = format!(
                    "{}{suffix}",
                    base.chars().take(31 - suffix.len()).collect::<String>()
                );
                attempt += 1;
            }
            grids.push(grid);
        }
        Ok(grids)
    } else if content.get("rows").is_some() {
        Ok(vec![parse_grid(content, "Sheet1", &mut cells)?])
    } else {
        Err(invalid(
            "content needs sheets (a list of {name, header, rows}), or header and rows for one sheet",
        ))
    }
}

/// An `.xlsx` workbook: a bold, bordered, frozen header row with filters, columns sized to their content.
///
/// # Errors
///
/// [`WriteError`] for content that is the wrong shape or too large.
pub fn write_xlsx(content: &Value) -> Result<Vec<u8>, WriteError> {
    let grids = parse_sheets(content)?;
    let mut workbook = Workbook::new();
    let header_format = Format::new()
        .set_bold()
        .set_border_bottom(FormatBorder::Thin);
    let engine = |error: rust_xlsxwriter::XlsxError| WriteError::Engine(error.to_string());
    for grid in &grids {
        let sheet = workbook.add_worksheet();
        sheet.set_name(&grid.name).map_err(engine)?;
        let offset = u32::from(!grid.header.is_empty());
        let mut widths: Vec<f64> = Vec::new();
        let mut widen = |column: usize, characters: usize| {
            if widths.len() <= column {
                widths.resize(column + 1, 8.0);
            }
            #[allow(clippy::cast_precision_loss)]
            let wanted = (characters as f64 + 2.0).min(MAX_WIDTH);
            widths[column] = widths[column].max(wanted);
        };
        for (column, name) in grid.header.iter().enumerate() {
            let column_index = u16::try_from(column).map_err(|_| invalid("too many columns"))?;
            sheet
                .write_string_with_format(0, column_index, name, &header_format)
                .map_err(engine)?;
            widen(column, name.chars().count());
        }
        for (row_index, row) in grid.rows.iter().enumerate() {
            let row_number =
                u32::try_from(row_index).map_err(|_| invalid("too many rows"))? + offset;
            for (column, cell) in row.iter().enumerate() {
                let column_index =
                    u16::try_from(column).map_err(|_| invalid("too many columns"))?;
                match cell {
                    Cell::Empty => {}
                    Cell::Text(text) => {
                        sheet
                            .write_string(row_number, column_index, text)
                            .map_err(engine)?;
                        // The longest line decides, not the whole text.
                        widen(
                            column,
                            text.lines()
                                .map(|line| line.chars().count())
                                .max()
                                .unwrap_or(0),
                        );
                    }
                    Cell::Number(number) => {
                        sheet
                            .write_number(row_number, column_index, *number)
                            .map_err(engine)?;
                        widen(column, number.to_string().len());
                    }
                    Cell::Bool(flag) => {
                        sheet
                            .write_boolean(row_number, column_index, *flag)
                            .map_err(engine)?;
                        widen(column, 5);
                    }
                }
            }
        }
        for (column, width) in widths.iter().enumerate() {
            let column_index = u16::try_from(column).map_err(|_| invalid("too many columns"))?;
            sheet
                .set_column_width(column_index, *width)
                .map_err(engine)?;
        }
        if !grid.header.is_empty() {
            sheet.set_freeze_panes(1, 0).map_err(engine)?;
            if let (Ok(last_row), Ok(last_column)) = (
                u32::try_from(grid.rows.len()),
                u16::try_from(grid.header.len().saturating_sub(1)),
            ) {
                sheet
                    .autofilter(0, 0, last_row, last_column)
                    .map_err(engine)?;
            }
        }
    }
    workbook.save_to_buffer().map_err(engine)
}

fn csv_field(text: &str, delimiter: char) -> String {
    let guarded = if text.starts_with(['=', '+', '-', '@', '\t', '\r']) {
        format!("'{text}")
    } else {
        text.to_owned()
    };
    if guarded.contains([delimiter, '"', '\n', '\r']) {
        format!("\"{}\"", guarded.replace('"', "\"\""))
    } else {
        guarded
    }
}

/// A `.csv` file: UTF-8 with a byte-order mark and CRLF line ends, which is what spreadsheet programs expect.
///
/// `delimiter` is `,` or `;` (Dutch and German Excel split on `;`). One sheet only: the first, or the content's own `header` and `rows`.
///
/// # Errors
///
/// [`WriteError`] for content that is the wrong shape or too large, or a delimiter that is neither.
pub fn write_csv(content: &Value, delimiter: char) -> Result<Vec<u8>, WriteError> {
    if !matches!(delimiter, ',' | ';') {
        return Err(invalid("the delimiter must be a comma or a semicolon"));
    }
    let grids = parse_sheets(content)?;
    let grid = grids.first().ok_or_else(|| invalid("nothing to write"))?;
    let mut out = String::from('\u{feff}');
    let separator = delimiter.to_string();
    let mut line = |fields: Vec<String>| {
        out.push_str(&fields.join(&separator));
        out.push_str("\r\n");
    };
    if !grid.header.is_empty() {
        line(
            grid.header
                .iter()
                .map(|name| csv_field(name, delimiter))
                .collect(),
        );
    }
    for row in &grid.rows {
        line(
            row.iter()
                .map(|cell| match cell {
                    Cell::Empty => String::new(),
                    Cell::Text(text) => csv_field(text, delimiter),
                    Cell::Number(number) => number.to_string(),
                    Cell::Bool(flag) => flag.to_string(),
                })
                .collect(),
        );
    }
    Ok(out.into_bytes())
}

fn text_of(block: &Value, key: &str, place: &str) -> Result<String, WriteError> {
    let text = block
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| invalid(format!("{place} needs {key}")))?;
    if text.chars().count() > MAX_CELL_CHARS {
        return Err(WriteError::TooLarge(format!("{place} is too long")));
    }
    Ok(text.to_owned())
}

/// A run of text, with line breaks kept.
fn run_of(text: &str, bold: bool) -> Run {
    let mut run = if bold { Run::new().bold() } else { Run::new() };
    for (index, line) in text.split('\n').enumerate() {
        if index > 0 {
            run = run.add_break(BreakType::TextWrapping);
        }
        run = run.add_text(line.trim_end_matches('\r'));
    }
    run
}

const BULLETS: usize = 1;
const NUMBERS: usize = 2;

/// A Word document with the styles and list definitions the blocks use.
fn base_docx() -> Docx {
    let list = |id: usize, format: &str, text: &str| {
        AbstractNumbering::new(id).add_level(
            Level::new(
                0,
                Start::new(1),
                NumberFormat::new(format),
                LevelText::new(text),
                LevelJc::new("left"),
            )
            .indent(Some(720), Some(SpecialIndentType::Hanging(360)), None, None),
        )
    };
    Docx::new()
        .add_style(
            Style::new("Title", StyleType::Paragraph)
                .name("Title")
                .size(52)
                .bold(),
        )
        .add_style(
            Style::new("Heading1", StyleType::Paragraph)
                .name("heading 1")
                .size(36)
                .bold(),
        )
        .add_style(
            Style::new("Heading2", StyleType::Paragraph)
                .name("heading 2")
                .size(30)
                .bold(),
        )
        .add_style(
            Style::new("Heading3", StyleType::Paragraph)
                .name("heading 3")
                .size(26)
                .bold(),
        )
        .add_abstract_numbering(list(BULLETS, "bullet", "\u{2022}"))
        .add_abstract_numbering(list(NUMBERS, "decimal", "%1."))
        .add_numbering(Numbering::new(BULLETS, BULLETS))
        .add_numbering(Numbering::new(NUMBERS, NUMBERS))
}

fn cell_text(cell: &Cell) -> String {
    match cell {
        Cell::Empty => String::new(),
        Cell::Text(text) => text.clone(),
        Cell::Number(number) => number.to_string(),
        Cell::Bool(flag) => flag.to_string(),
    }
}

fn word_cell(text: &str, bold: bool) -> TableCell {
    TableCell::new().add_paragraph(Paragraph::new().add_run(run_of(text, bold)))
}

fn word_table(grid: &Grid, place: &str) -> Result<Table, WriteError> {
    let mut rows = Vec::new();
    if !grid.header.is_empty() {
        rows.push(TableRow::new(
            grid.header
                .iter()
                .map(|name| word_cell(name, true))
                .collect(),
        ));
    }
    for row in &grid.rows {
        rows.push(TableRow::new(
            row.iter()
                .map(|cell| word_cell(&cell_text(cell), false))
                .collect(),
        ));
    }
    if rows.is_empty() {
        return Err(invalid(format!("{place} is a table with no rows")));
    }
    Ok(Table::new(rows))
}

/// Adds one block to the document.
fn add_block(
    docx: Docx,
    block: &Value,
    place: &str,
    cells: &mut usize,
) -> Result<Docx, WriteError> {
    let kind = block
        .get("type")
        .and_then(Value::as_str)
        .ok_or_else(|| invalid(format!("{place} needs a type")))?;
    match kind {
        "heading" => {
            let level = block
                .get("level")
                .and_then(Value::as_u64)
                .unwrap_or(1)
                .clamp(1, 3);
            let text = text_of(block, "text", place)?;
            Ok(docx.add_paragraph(
                Paragraph::new()
                    .style(&format!("Heading{level}"))
                    .add_run(run_of(&text, false)),
            ))
        }
        "paragraph" => {
            let text = text_of(block, "text", place)?;
            let bold = block.get("bold").and_then(Value::as_bool).unwrap_or(false);
            Ok(docx.add_paragraph(Paragraph::new().add_run(run_of(&text, bold))))
        }
        "bullets" | "numbered" => {
            let items = block
                .get("items")
                .and_then(Value::as_array)
                .filter(|items| items.len() <= MAX_ITEMS)
                .ok_or_else(|| {
                    invalid(format!(
                        "{place} needs a list of items (at most {MAX_ITEMS})"
                    ))
                })?;
            let id = if kind == "bullets" { BULLETS } else { NUMBERS };
            let mut docx = docx;
            for item in items {
                let text = item
                    .as_str()
                    .ok_or_else(|| invalid(format!("every item of {place} must be text")))?;
                docx = docx.add_paragraph(
                    Paragraph::new()
                        .numbering(NumberingId::new(id), IndentLevel::new(0))
                        .add_run(run_of(text, false)),
                );
            }
            Ok(docx)
        }
        "table" => {
            let table = word_table(&parse_grid(block, "Table", cells)?, place)?;
            // A paragraph after a table keeps Word from running the next block into it.
            Ok(docx.add_table(table).add_paragraph(Paragraph::new()))
        }
        other => Err(invalid(format!(
            "{place} has an unknown type {other:?}: use heading, paragraph, bullets, numbered or table"
        ))),
    }
}

/// A `.docx` document from `title` and `blocks`: headings (levels 1 to 3), paragraphs, bullet and numbered lists, and tables.
///
/// # Errors
///
/// [`WriteError`] for content that is the wrong shape or too large.
pub fn write_docx(content: &Value) -> Result<Vec<u8>, WriteError> {
    let blocks = content
        .get("blocks")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            invalid("content needs blocks: a list of headings, paragraphs, lists and tables")
        })?;
    if blocks.len() > MAX_BLOCKS {
        return Err(WriteError::TooLarge(format!(
            "more than {MAX_BLOCKS} blocks"
        )));
    }
    let mut docx = base_docx();
    if let Some(title) = content
        .get("title")
        .and_then(Value::as_str)
        .filter(|title| !title.trim().is_empty())
    {
        docx = docx.add_paragraph(
            Paragraph::new()
                .style("Title")
                .add_run(run_of(title, false)),
        );
    }
    let mut cells = 0;
    for (index, block) in blocks.iter().enumerate() {
        docx = add_block(docx, block, &format!("block {}", index + 1), &mut cells)?;
    }
    let mut buffer = Cursor::new(Vec::new());
    docx.build()
        .pack(&mut buffer)
        .map_err(|error| WriteError::Engine(error.to_string()))?;
    Ok(buffer.into_inner())
}
#[cfg(test)]
#[path = "write_tests.rs"]
mod tests;
