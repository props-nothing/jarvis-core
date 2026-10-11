//! Writing a `.pdf` from the same blocks a Word document takes (`P11-010`, `docs/research/integrations/office-writers.md`).
//!
//! The file is built directly with `lopdf`, which is already here to read PDFs, so nothing new is added to the build: A4 pages, the two
//! standard fonts Helvetica and Helvetica-Bold (every PDF reader has them, so nothing is embedded), `WinAnsiEncoding`, and the published
//! Adobe widths in [`crate::pdf_metrics`] to wrap lines. That covers Western European text, the euro sign and typographic quotes. A
//! character outside it (another script, an emoji) is drawn as `?` and **counted**, so the caller can say so rather than hand over a
//! document that quietly differs from what was asked for.
//!
//! Layout is deliberately plain: a title, headings, paragraphs, bullet and numbered lists, tables with a shaded header that repeats on
//! each page, and "Page n of m" at the foot.

use std::fmt::Write as _;

use lopdf::{Document, Object, Stream, dictionary};
use serde_json::Value;

use crate::pdf_metrics::{HELVETICA, HELVETICA_BOLD};
use crate::write::{
    Cell, Grid, MAX_BLOCKS, MAX_ITEMS, WriteError, cell_text, invalid, parse_grid, text_of,
};

const PAGE_WIDTH: f32 = 595.0;
const PAGE_HEIGHT: f32 = 842.0;
const MARGIN: f32 = 56.0;
const FOOTER_BASELINE: f32 = 30.0;
/// The most pages one document may have.
const MAX_PAGES: usize = 300;
/// The most lines one table cell shows; a longer cell is cut and ends in an ellipsis.
const MAX_CELL_LINES: usize = 50;
const BODY: f32 = 11.0;
const SMALL: f32 = 9.5;
const PAD: f32 = 4.0;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Face {
    Regular,
    Bold,
}

impl Face {
    const fn name(self) -> &'static str {
        match self {
            Self::Regular => "F1",
            Self::Bold => "F2",
        }
    }

    fn width(self, byte: u8) -> f32 {
        let table = match self {
            Self::Regular => &HELVETICA,
            Self::Bold => &HELVETICA_BOLD,
        };
        let index = usize::from(byte.clamp(32, 255)) - 32;
        f32::from(table[index]) / 1000.0
    }
}

/// The `WinAnsiEncoding` byte for a character, or `None` when the encoding has none.
fn win_ansi(character: char) -> Option<u8> {
    let code = u32::from(character);
    if (32..127).contains(&code) || (160..256).contains(&code) {
        return u8::try_from(code).ok();
    }
    Some(match code {
        0x20AC => 128,
        0x201A => 130,
        0x0192 => 131,
        0x201E => 132,
        0x2026 => 133,
        0x2020 => 134,
        0x2021 => 135,
        0x02C6 => 136,
        0x2030 => 137,
        0x0160 => 138,
        0x2039 => 139,
        0x0152 => 140,
        0x017D => 142,
        0x2018 => 145,
        0x2019 => 146,
        0x201C => 147,
        0x201D => 148,
        0x2022 => 149,
        0x2013 => 150,
        0x2014 => 151,
        0x02DC => 152,
        0x2122 => 153,
        0x0161 => 154,
        0x203A => 155,
        0x0153 => 156,
        0x017E => 158,
        0x0178 => 159,
        0x2212 => b'-',
        _ => return None,
    })
}

/// Text as `WinAnsiEncoding` bytes. A tab or other control becomes a space; what the encoding cannot show becomes `?` and is counted.
fn encode(text: &str, replaced: &mut usize) -> Vec<u8> {
    text.chars()
        .map(|character| {
            if character.is_control() {
                b' '
            } else if let Some(byte) = win_ansi(character) {
                byte
            } else {
                *replaced += 1;
                b'?'
            }
        })
        .collect()
}

/// A count of lines as a length factor; no page holds more lines than fit in a `u16`.
fn lines_f32(count: usize) -> f32 {
    f32::from(u16::try_from(count).unwrap_or(u16::MAX))
}

fn text_width(bytes: &[u8], face: Face, size: f32) -> f32 {
    bytes.iter().map(|byte| face.width(*byte)).sum::<f32>() * size
}

/// Breaks one paragraph (no line breaks inside) into lines that fit `width`, at spaces, and inside a word only when the word alone is wider.
fn wrap_line(bytes: &[u8], face: Face, size: f32, width: f32) -> Vec<Vec<u8>> {
    let mut lines: Vec<Vec<u8>> = Vec::new();
    let mut current: Vec<u8> = Vec::new();
    for word in bytes
        .split(|byte| *byte == b' ')
        .filter(|word| !word.is_empty())
    {
        let mut word = word.to_vec();
        loop {
            let joined = if current.is_empty() {
                word.clone()
            } else {
                [current.as_slice(), b" ", word.as_slice()].concat()
            };
            if text_width(&joined, face, size) <= width {
                current = joined;
                break;
            }
            if !current.is_empty() {
                lines.push(std::mem::take(&mut current));
                continue;
            }
            // A single word wider than the line: cut it where it stops fitting.
            let mut fit = 1;
            while fit < word.len() && text_width(&word[..=fit], face, size) <= width {
                fit += 1;
            }
            lines.push(word[..fit].to_vec());
            word = word[fit..].to_vec();
            if word.is_empty() {
                break;
            }
        }
    }
    if !current.is_empty() || lines.is_empty() {
        lines.push(current);
    }
    lines
}

fn wrap(text: &str, face: Face, size: f32, width: f32, replaced: &mut usize) -> Vec<Vec<u8>> {
    text.split('\n')
        .flat_map(|line| {
            let bytes = encode(line.trim_end_matches('\r'), replaced);
            wrap_line(&bytes, face, size, width)
        })
        .collect()
}

/// A string for a content stream: parentheses and backslashes escaped, anything outside printable ASCII as an octal escape.
fn literal(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() + 2);
    out.push('(');
    for byte in bytes {
        match byte {
            b'(' | b')' | b'\\' => {
                out.push('\\');
                out.push(char::from(*byte));
            }
            32..=126 => out.push(char::from(*byte)),
            other => {
                let _ = write!(out, "\\{other:03o}");
            }
        }
    }
    out.push(')');
    out
}

struct Layout {
    pages: Vec<String>,
    /// The distance from the top of the page to the next free line.
    cursor: f32,
    replaced: usize,
}

impl Layout {
    fn new() -> Self {
        Self {
            pages: vec![String::new()],
            cursor: MARGIN,
            replaced: 0,
        }
    }

    const fn floor() -> f32 {
        PAGE_HEIGHT - MARGIN
    }

    fn new_page(&mut self) -> Result<(), WriteError> {
        if self.pages.len() >= MAX_PAGES {
            return Err(WriteError::TooLarge(format!("more than {MAX_PAGES} pages")));
        }
        self.pages.push(String::new());
        self.cursor = MARGIN;
        Ok(())
    }

    /// Makes room for `height` more points, starting a page when it does not fit (unless the page is still empty).
    fn ensure(&mut self, height: f32) -> Result<(), WriteError> {
        if self.cursor + height > Self::floor() && self.cursor > MARGIN {
            self.new_page()?;
        }
        Ok(())
    }

    fn page(&mut self) -> &mut String {
        let last = self.pages.len() - 1;
        &mut self.pages[last]
    }

    fn put(&mut self, x: f32, baseline_from_top: f32, face: Face, size: f32, bytes: &[u8]) {
        let y = PAGE_HEIGHT - baseline_from_top;
        let out = format!(
            "BT /{} {size:.1} Tf {x:.2} {y:.2} Td {} Tj ET\n",
            face.name(),
            literal(bytes)
        );
        self.page().push_str(&out);
    }

    /// Wrapped lines at `x`, each `size * 1.35` high; a page is started between lines when they run out of room.
    fn paragraph(
        &mut self,
        lines: &[Vec<u8>],
        x: f32,
        face: Face,
        size: f32,
    ) -> Result<(), WriteError> {
        let height = size * 1.35;
        for line in lines {
            self.ensure(height)?;
            let baseline = self.cursor + size;
            self.put(x, baseline, face, size, line);
            self.cursor += height;
        }
        Ok(())
    }

    fn gap(&mut self, points: f32) {
        self.cursor = (self.cursor + points).min(Self::floor());
    }
}

fn heading_size(level: u64) -> f32 {
    match level {
        1 => 16.0,
        2 => 13.5,
        _ => 12.0,
    }
}

fn add_list(
    layout: &mut Layout,
    block: &Value,
    numbered: bool,
    place: &str,
) -> Result<(), WriteError> {
    let items = block
        .get("items")
        .and_then(Value::as_array)
        .filter(|items| items.len() <= MAX_ITEMS)
        .ok_or_else(|| {
            invalid(format!(
                "{place} needs a list of items (at most {MAX_ITEMS})"
            ))
        })?;
    let width = PAGE_WIDTH - 2.0 * MARGIN - 24.0;
    for (index, item) in items.iter().enumerate() {
        let text = item
            .as_str()
            .ok_or_else(|| invalid(format!("every item of {place} must be text")))?;
        let lines = wrap(text, Face::Regular, BODY, width, &mut layout.replaced);
        layout.ensure(BODY * 1.35)?;
        let baseline = layout.cursor + BODY;
        let marker = if numbered {
            encode(&format!("{}.", index + 1), &mut layout.replaced)
        } else {
            vec![149]
        };
        layout.put(MARGIN + 4.0, baseline, Face::Regular, BODY, &marker);
        layout.paragraph(&lines, MARGIN + 24.0, Face::Regular, BODY)?;
        layout.gap(2.0);
    }
    layout.gap(4.0);
    Ok(())
}

/// How wide each column is: its longest cell, bounded, then scaled to fill the line.
fn column_widths(grid: &Grid, columns: usize, replaced: &mut usize) -> Vec<f32> {
    let available = PAGE_WIDTH - 2.0 * MARGIN;
    let mut natural = vec![40.0_f32; columns];
    let mut consider = |column: usize, text: &str, face: Face| {
        let widest = text
            .split('\n')
            .map(|line| text_width(&encode(line, replaced), face, SMALL))
            .fold(0.0_f32, f32::max);
        natural[column] = natural[column].max(widest + 2.0 * PAD).min(available);
    };
    for (column, name) in grid.header.iter().enumerate().take(columns) {
        consider(column, name, Face::Bold);
    }
    for row in &grid.rows {
        for (column, cell) in row.iter().enumerate().take(columns) {
            consider(column, &cell_text(cell), Face::Regular);
        }
    }
    let total: f32 = natural.iter().sum();
    natural
        .iter()
        .map(|width| width * available / total)
        .collect()
}

fn table_row(
    layout: &mut Layout,
    widths: &[f32],
    cells: &[String],
    face: Face,
    shaded: bool,
    repeat: Option<&[String]>,
) -> Result<(), WriteError> {
    let height_of = |lines: usize| lines_f32(lines) * SMALL * 1.35 + 2.0 * PAD;
    let mut wrapped: Vec<Vec<Vec<u8>>> = Vec::new();
    for (index, width) in widths.iter().enumerate() {
        let text = cells.get(index).map_or("", String::as_str);
        let mut lines = wrap(text, face, SMALL, width - 2.0 * PAD, &mut layout.replaced);
        if lines.len() > MAX_CELL_LINES {
            lines.truncate(MAX_CELL_LINES);
            if let Some(last) = lines.last_mut() {
                last.extend_from_slice(&[133]);
            }
        }
        wrapped.push(lines);
    }
    let rows = wrapped.iter().map(Vec::len).max().unwrap_or(1).max(1);
    let height = height_of(rows);
    if layout.cursor + height > Layout::floor() && layout.cursor > MARGIN {
        layout.new_page()?;
        // The header is drawn again on the new page.
        if let Some(header) = repeat {
            table_row(layout, widths, header, Face::Bold, true, None)?;
        }
    }
    let top = layout.cursor;
    let mut x = MARGIN;
    for (width, lines) in widths.iter().zip(&wrapped) {
        let y = PAGE_HEIGHT - top - height;
        let rectangle = format!("{x:.2} {y:.2} {width:.2} {height:.2} re");
        let paint = if shaded {
            format!("0.92 g {rectangle} f 0 g 0.6 G {rectangle} S\n")
        } else {
            format!("0.6 G 0.5 w {rectangle} S\n")
        };
        layout.page().push_str(&paint);
        for (line_index, line) in lines.iter().enumerate() {
            let baseline = top + PAD + SMALL + lines_f32(line_index) * SMALL * 1.35;
            layout.put(x + PAD, baseline, face, SMALL, line);
        }
        x += width;
    }
    layout.cursor += height;
    Ok(())
}

fn add_table(
    layout: &mut Layout,
    block: &Value,
    place: &str,
    cells: &mut usize,
) -> Result<(), WriteError> {
    let grid = parse_grid(block, "Table", cells)?;
    let columns = grid
        .rows
        .iter()
        .map(Vec::len)
        .chain([grid.header.len()])
        .max()
        .unwrap_or(0);
    if columns == 0 || (grid.header.is_empty() && grid.rows.is_empty()) {
        return Err(invalid(format!("{place} is a table with no rows")));
    }
    let widths = column_widths(&grid, columns, &mut layout.replaced);
    let header: Vec<String> = grid.header.clone();
    let text_row = |row: &[Cell]| row.iter().map(cell_text).collect::<Vec<_>>();
    if !header.is_empty() {
        table_row(layout, &widths, &header, Face::Bold, true, None)?;
    }
    for row in &grid.rows {
        let texts = text_row(row);
        table_row(
            layout,
            &widths,
            &texts,
            Face::Regular,
            false,
            (!header.is_empty()).then_some(header.as_slice()),
        )?;
    }
    layout.gap(10.0);
    Ok(())
}

fn add_block(
    layout: &mut Layout,
    block: &Value,
    place: &str,
    cells: &mut usize,
) -> Result<(), WriteError> {
    let kind = block
        .get("type")
        .and_then(Value::as_str)
        .ok_or_else(|| invalid(format!("{place} needs a type")))?;
    let width = PAGE_WIDTH - 2.0 * MARGIN;
    match kind {
        "heading" => {
            let level = block
                .get("level")
                .and_then(Value::as_u64)
                .unwrap_or(1)
                .clamp(1, 3);
            let size = heading_size(level);
            let text = text_of(block, "text", place)?;
            let lines = wrap(&text, Face::Bold, size, width, &mut layout.replaced);
            layout.gap(8.0);
            // A heading is never the last thing on a page: room for it and two lines after it.
            layout.ensure(size * 1.35 * lines_f32(lines.len()) + 2.0 * BODY * 1.35)?;
            layout.paragraph(&lines, MARGIN, Face::Bold, size)?;
            layout.gap(4.0);
        }
        "paragraph" => {
            let text = text_of(block, "text", place)?;
            let face = if block.get("bold").and_then(Value::as_bool).unwrap_or(false) {
                Face::Bold
            } else {
                Face::Regular
            };
            let lines = wrap(&text, face, BODY, width, &mut layout.replaced);
            layout.paragraph(&lines, MARGIN, face, BODY)?;
            layout.gap(6.0);
        }
        "bullets" => add_list(layout, block, false, place)?,
        "numbered" => add_list(layout, block, true, place)?,
        "table" => add_table(layout, block, place, cells)?,
        other => {
            return Err(invalid(format!(
                "{place} has an unknown type {other:?}: use heading, paragraph, bullets, numbered or table"
            )));
        }
    }
    Ok(())
}

/// A `.pdf` from `title` and `blocks`, as [`crate::write_docx`] takes them. Returns the file and how many characters the standard fonts
/// could not show (drawn as `?`).
///
/// # Errors
///
/// [`WriteError`] for content that is the wrong shape or too large.
pub fn write_pdf(content: &Value) -> Result<(Vec<u8>, usize), WriteError> {
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
    let mut layout = Layout::new();
    let title = content
        .get("title")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|title| !title.is_empty());
    if let Some(title) = title {
        let lines = wrap(
            title,
            Face::Bold,
            22.0,
            PAGE_WIDTH - 2.0 * MARGIN,
            &mut layout.replaced,
        );
        layout.paragraph(&lines, MARGIN, Face::Bold, 22.0)?;
        layout.gap(10.0);
    }
    let mut cells = 0;
    for (index, block) in blocks.iter().enumerate() {
        add_block(
            &mut layout,
            block,
            &format!("block {}", index + 1),
            &mut cells,
        )?;
    }
    let total = layout.pages.len();
    for number in 0..total {
        let label = encode(
            &format!("Page {} of {total}", number + 1),
            &mut layout.replaced,
        );
        let x = (PAGE_WIDTH - text_width(&label, Face::Regular, 8.0)) / 2.0;
        let footer = format!(
            "0.4 g BT /F1 8.0 Tf {x:.2} {FOOTER_BASELINE:.2} Td {} Tj ET 0 g\n",
            literal(&label)
        );
        layout.pages[number].push_str(&footer);
    }
    let replaced = layout.replaced;
    Ok((assemble(&layout.pages, title)?, replaced))
}

fn assemble(pages: &[String], title: Option<&str>) -> Result<Vec<u8>, WriteError> {
    let mut document = Document::with_version("1.5");
    let pages_id = document.new_object_id();
    let regular = document.add_object(dictionary! {
        "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica", "Encoding" => "WinAnsiEncoding",
    });
    let bold = document.add_object(dictionary! {
        "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica-Bold", "Encoding" => "WinAnsiEncoding",
    });
    let resources = document
        .add_object(dictionary! { "Font" => dictionary! { "F1" => regular, "F2" => bold } });
    let mut kids = Vec::new();
    for content in pages {
        let mut stream = Stream::new(dictionary! {}, content.clone().into_bytes());
        // A stream that cannot be compressed is kept as it is: only the size differs.
        let _ = stream.compress();
        let stream_id = document.add_object(stream);
        let page = document.add_object(dictionary! {
            "Type" => "Page", "Parent" => pages_id, "Contents" => stream_id, "Resources" => resources,
            "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()],
        });
        kids.push(Object::Reference(page));
    }
    let count =
        i64::try_from(kids.len()).map_err(|_| WriteError::TooLarge("too many pages".into()))?;
    document.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! { "Type" => "Pages", "Kids" => kids, "Count" => count }),
    );
    let catalog = document.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
    document.trailer.set("Root", catalog);
    let mut info = dictionary! { "Producer" => Object::string_literal("JARVIS") };
    // The title only when it is plain ASCII: the information dictionary's strings have their own encoding rules.
    if let Some(title) =
        title.filter(|title| title.is_ascii() && !title.chars().any(char::is_control))
    {
        info.set("Title", Object::string_literal(title));
    }
    let info_id = document.add_object(info);
    document.trailer.set("Info", info_id);
    let mut bytes = Vec::new();
    document
        .save_to(&mut bytes)
        .map_err(|error| WriteError::Engine(error.to_string()))?;
    Ok(bytes)
}

#[cfg(test)]
#[path = "pdf_write_tests.rs"]
mod tests;
