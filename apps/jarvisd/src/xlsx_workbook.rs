//! Bounded, literal-only edits of existing `SpreadsheetML` packages; untouched parts are raw-copied.

use std::collections::{BTreeMap, BTreeSet};
use std::io::{Cursor, Read, Write};
use std::ops::Range;

use roxmltree::{Document, Node, ParsingOptions};
use serde_json::Value;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

const MAIN: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
const REL: &str = "http://schemas.openxmlformats.org/package/2006/relationships";
const OFFICE_REL: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const CONTENT: &str = "http://schemas.openxmlformats.org/package/2006/content-types";
pub(super) const MAX_BYTES: usize = 10 * 1024 * 1024;
const MAX_PART: u64 = 24 * 1024 * 1024;
const MAX_UNPACKED: u64 = 48 * 1024 * 1024;
const MAX_XML_DEPTH: usize = 128;
const INVALID_XML: &str = "the workbook contains invalid or unsupported XML";

#[derive(Debug)]
pub(super) struct CellEdit {
    sheet: String,
    address: String,
    row: u32,
    column: u32,
    value: Value,
}

pub(super) fn parse_edits(value: &Value) -> Result<Vec<CellEdit>, String> {
    let edits = value.as_array().ok_or("edits must be an array")?;
    if edits.is_empty() || edits.len() > 500 || value.to_string().len() > 1024 * 1024 {
        return Err("use 1 to 500 cell edits, at most 1 MiB per call".into());
    }
    let mut seen = BTreeSet::new();
    edits
        .iter()
        .map(|edit| {
            let object = edit.as_object().ok_or("each edit must be an object")?;
            if object.len() != 3 || !object.contains_key("value") {
                return Err("each edit requires exactly sheet, cell and value".into());
            }
            let sheet = object.get("sheet").and_then(Value::as_str).ok_or("sheet is required")?;
            if sheet.is_empty() || sheet.chars().count() > 31 || !xml_text(sheet) {
                return Err("sheet must be an existing sheet name, 1 to 31 characters".into());
            }
            let address = object.get("cell").and_then(Value::as_str).ok_or("cell is required")?;
            let (row, column) = coordinate(address)?;
            if !seen.insert((sheet.to_owned(), row, column)) {
                return Err("a cell may only be edited once per call".into());
            }
            let value = &object["value"];
            match value {
                Value::Null | Value::Bool(_) | Value::Number(_) => {}
                Value::String(text)
                    if text.chars().count() <= 32_000
                        && text.encode_utf16().count() <= 32_767
                        && xml_text(text) => {}
                _ => return Err("cell values must be null, boolean, number or XML-safe text up to 32000 characters (32767 UTF-16 units)".into()),
            }
            Ok(CellEdit { sheet: sheet.into(), address: address.into(), row, column, value: value.clone() })
        })
        .collect()
}

fn xml_text(text: &str) -> bool {
    text.chars().all(|c| matches!(c, '\t' | '\n' | '\r' | '\u{20}'..='\u{d7ff}' | '\u{e000}'..='\u{fffd}' | '\u{10000}'..='\u{10ffff}'))
}

fn coordinate(address: &str) -> Result<(u32, u32), String> {
    let split = address
        .bytes()
        .position(|c| !c.is_ascii_uppercase())
        .ok_or("use an A1 cell coordinate")?;
    let (letters, digits) = address.split_at(split);
    if letters.is_empty()
        || letters.len() > 3
        || digits.is_empty()
        || digits.len() > 7
        || digits.starts_with('0')
        || !digits.bytes().all(|c| c.is_ascii_digit())
    {
        return Err("use an uppercase A1 coordinate between A1 and XFD1048576".into());
    }
    let column = letters
        .bytes()
        .fold(0_u32, |n, c| n * 26 + u32::from(c - b'A' + 1));
    let row = digits.parse::<u32>().map_err(|_| "invalid cell row")?;
    if !(1..=16_384).contains(&column) || !(1..=1_048_576).contains(&row) {
        return Err("the cell is outside Excel's A1:XFD1048576 grid".into());
    }
    Ok((row, column))
}

pub(super) fn edit(bytes: &[u8], edits: &[CellEdit]) -> Result<Vec<u8>, String> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| edit_package(bytes, edits)))
        .map_err(|_| "the workbook could not be parsed safely".to_owned())?
}

fn skip_xml_markup<'a>(bytes: &'a [u8], end: &[u8]) -> Result<&'a [u8], String> {
    let at = bytes
        .windows(end.len())
        .position(|window| window == end)
        .ok_or(INVALID_XML)?;
    Ok(&bytes[at + end.len()..])
}

fn check_xml_depth(text: &str) -> Result<(), String> {
    // roxmltree 0.21.1 recurses per element; a post-parse check cannot protect its stack.
    let mut remaining = text.as_bytes();
    let mut depth = 0_usize;
    while let Some(start) = remaining.iter().position(|&byte| byte == b'<') {
        remaining = &remaining[start..];
        remaining = if let Some(body) = remaining.strip_prefix(b"<!--") {
            skip_xml_markup(body, b"-->")?
        } else if let Some(body) = remaining.strip_prefix(b"<![CDATA[") {
            skip_xml_markup(body, b"]]>")?
        } else if let Some(body) = remaining.strip_prefix(b"<?") {
            skip_xml_markup(body, b"?>")?
        } else {
            if remaining.starts_with(b"<!") {
                return Err(INVALID_XML.into());
            }
            let closing = remaining.starts_with(b"</");
            if !closing && depth >= MAX_XML_DEPTH {
                return Err("workbook XML is nested too deeply".into());
            }
            let mut quote = None;
            let end = remaining
                .iter()
                .enumerate()
                .skip(1)
                .find_map(|(at, &byte)| {
                    match (quote, byte) {
                        (Some(delimiter), value) if delimiter == value => quote = None,
                        (None, b'\'' | b'"') => quote = Some(byte),
                        (None, b'>') => return Some(at),
                        _ => {}
                    }
                    None
                })
                .ok_or(INVALID_XML)?;
            if closing {
                depth = depth.checked_sub(1).ok_or(INVALID_XML)?;
            } else if remaining[end - 1] != b'/' {
                depth += 1;
            }
            &remaining[end + 1..]
        };
    }
    if depth != 0 {
        return Err(INVALID_XML.into());
    }
    Ok(())
}

fn parse_xml(bytes: &[u8]) -> Result<Document<'_>, String> {
    let text = std::str::from_utf8(bytes).map_err(|_| "workbook XML must be UTF-8")?;
    check_xml_depth(text)?;
    let document = Document::parse_with_options(
        text,
        ParsingOptions {
            allow_dtd: false,
            nodes_limit: 2_000_000,
            ..ParsingOptions::default()
        },
    )
    .map_err(|_| INVALID_XML)?;
    if document
        .descendants()
        .any(|n| n.ancestors().take(MAX_XML_DEPTH + 2).count() > MAX_XML_DEPTH + 1)
    {
        return Err("workbook XML is nested too deeply".into());
    }
    Ok(document)
}

fn zip_error(_: impl std::fmt::Display) -> String {
    "the workbook ZIP is invalid, encrypted or unsupported".into()
}

fn end_directory(bytes: &[u8]) -> Result<(usize, usize), String> {
    let end = bytes.len().checked_sub(22).ok_or_else(|| zip_error(""))?;
    let at = (end.saturating_sub(65_535)..=end)
        .rev()
        .find(|&at| {
            bytes.get(at..at + 4) == Some(b"PK\x05\x06")
                && at + 22 + usize::from(u16::from_le_bytes([bytes[at + 20], bytes[at + 21]]))
                    == bytes.len()
        })
        .ok_or_else(|| zip_error(""))?;
    let short = |n| u16::from_le_bytes([bytes[at + n], bytes[at + n + 1]]);
    let long = |n| u32::from_le_bytes(bytes[at + n..at + n + 4].try_into().unwrap_or([0; 4]));
    let count = usize::from(short(10));
    let start = usize::try_from(long(16)).map_err(zip_error)?;
    let size = usize::try_from(long(12)).map_err(zip_error)?;
    if short(4) != 0
        || short(6) != 0
        || short(8) != short(10)
        || count == 0
        || count > 4000
        || start.checked_add(size) != Some(at)
    {
        return Err("multi-disk, ZIP64, oversized or malformed archives cannot be edited".into());
    }
    Ok((count, start))
}

fn package_name(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with('/')
        && !name.contains(['\\', ':', '\0', '%', '?', '#'])
        && name
            .trim_end_matches('/')
            .split('/')
            .all(|p| !p.is_empty() && p != "." && p != "..")
        && !name.chars().any(char::is_control)
}

fn check_local_header(bytes: &[u8], start: usize, data: u64, name: &[u8]) -> Result<(), String> {
    let header = bytes
        .get(start..start.saturating_add(30))
        .ok_or_else(|| zip_error(""))?;
    let name_len = usize::from(u16::from_le_bytes([header[26], header[27]]));
    let extra_len = usize::from(u16::from_le_bytes([header[28], header[29]]));
    let name_start = start + 30;
    let name_end = name_start + name_len;
    if data != (name_end + extra_len) as u64
        || bytes.get(name_start..name_end) != Some(name)
        || header[6] & 1 != 0
    {
        return Err("ZIP local and central metadata disagree".into());
    }
    Ok(())
}

fn read_parts(bytes: &[u8]) -> Result<BTreeMap<String, Vec<u8>>, String> {
    if bytes.len() > MAX_BYTES {
        return Err("the workbook exceeds 10 MiB".into());
    }
    let (count, directory) = end_directory(bytes)?;
    let mut archive = ZipArchive::new(Cursor::new(bytes)).map_err(zip_error)?;
    if archive.len() != count {
        return Err("duplicate ZIP entries are not supported".into());
    }
    let mut names = BTreeSet::new();
    let mut extents = Vec::new();
    let mut parts = BTreeMap::new();
    let mut total = 0_u64;
    for index in 0..archive.len() {
        let mut file = archive.by_index(index).map_err(zip_error)?;
        let name = file.name().map_err(zip_error)?.into_owned();
        if !package_name(&name)
            || !names.insert(name.to_ascii_lowercase())
            || file.encrypted()
            || file.is_symlink()
            || !matches!(
                file.compression(),
                CompressionMethod::Stored | CompressionMethod::Deflated
            )
            || file
                .unix_mode()
                .is_some_and(|mode| !matches!(mode & 0o170_000, 0 | 0o100_000 | 0o40_000))
        {
            return Err("unsafe or ambiguous ZIP entry in workbook".into());
        }
        let lower = name.to_ascii_lowercase();
        if lower.starts_with("_xmlsignatures/") || lower.ends_with("vbaproject.bin") {
            return Err("signed or macro-enabled workbooks cannot be edited".into());
        }
        if file.size() > MAX_PART || total.saturating_add(file.size()) > MAX_UNPACKED {
            return Err("the workbook unpacks beyond the safety limits".into());
        }
        let start = file.header_start();
        let data = file.data_start().ok_or_else(|| zip_error(""))?;
        let end = data
            .checked_add(file.compressed_size())
            .ok_or_else(|| zip_error(""))?;
        let header = usize::try_from(start).map_err(zip_error)?;
        if start >= data
            || end > directory as u64
            || bytes.get(header..header.saturating_add(4)) != Some(b"PK\x03\x04")
        {
            return Err("invalid ZIP member extent".into());
        }
        check_local_header(bytes, header, data, file.name_raw())?;
        extents.push((start, end));
        let mut content = Vec::new();
        (&mut file)
            .take(MAX_PART + 1)
            .read_to_end(&mut content)
            .map_err(zip_error)?;
        total += content.len() as u64;
        if content.len() as u64 > MAX_PART
            || total > MAX_UNPACKED
            || content.len() as u64 != file.size()
        {
            return Err("the workbook unpacks beyond the safety limits".into());
        }
        if lower
            .rsplit_once('.')
            .is_some_and(|(_, extension)| matches!(extension, "xml" | "rels"))
        {
            parse_xml(&content)?;
        }
        parts.insert(name, content);
    }
    extents.sort_unstable();
    if extents.windows(2).any(|pair| pair[0].1 > pair[1].0) {
        return Err("overlapping ZIP entries cannot be edited".into());
    }
    Ok(parts)
}

fn part<'a>(parts: &'a BTreeMap<String, Vec<u8>>, name: &str) -> Result<&'a [u8], String> {
    parts
        .get(name)
        .map(Vec::as_slice)
        .ok_or_else(|| "a required workbook part is missing".into())
}

fn relationship_target(base: &str, target: &str) -> Result<String, String> {
    if target.starts_with("//")
        || target.contains(['\\', ':', '%', '?', '#', '\0'])
        || target.chars().any(char::is_control)
    {
        return Err("unsafe workbook relationship target".into());
    }
    let mut path: Vec<&str> = if target.starts_with('/') {
        Vec::new()
    } else {
        base.rsplit_once('/')
            .map_or_else(Vec::new, |(parent, _)| parent.split('/').collect())
    };
    for segment in target.trim_start_matches('/').split('/') {
        match segment {
            "" | "." => return Err("noncanonical workbook relationship target".into()),
            ".." => {
                path.pop()
                    .ok_or("workbook relationship escapes the package")?;
            }
            other => path.push(other),
        }
    }
    let result = path.join("/");
    if !package_name(&result) {
        return Err("unsafe workbook relationship target".into());
    }
    Ok(result)
}

fn relationships(bytes: &[u8], base: &str) -> Result<BTreeMap<String, (String, String)>, String> {
    let doc = parse_xml(bytes)?;
    if !doc.root_element().has_tag_name((REL, "Relationships")) {
        return Err("invalid workbook relationships".into());
    }
    let mut result = BTreeMap::new();
    for node in doc.root_element().children().filter(Node::is_element) {
        if !node.has_tag_name((REL, "Relationship")) {
            return Err("unsupported relationship element".into());
        }
        let id = node.attribute("Id").ok_or("relationship has no id")?;
        let kind = node.attribute("Type").ok_or("relationship has no type")?;
        let target = node
            .attribute("Target")
            .ok_or("relationship has no target")?;
        let target = match node.attribute("TargetMode") {
            Some("External") => String::new(),
            None | Some("Internal") => relationship_target(base, target)?,
            _ => return Err("invalid workbook relationship mode".into()),
        };
        if result.insert(id.into(), (kind.into(), target)).is_some() {
            return Err("duplicate workbook relationship id".into());
        }
    }
    Ok(result)
}

struct Workbook {
    name: String,
    sheets: BTreeMap<String, String>,
}

fn resolve_workbook(parts: &BTreeMap<String, Vec<u8>>) -> Result<Workbook, String> {
    let content = parse_xml(part(parts, "[Content_Types].xml")?)?;
    if !content.root_element().has_tag_name((CONTENT, "Types")) {
        return Err("invalid package content types".into());
    }
    if content
        .descendants()
        .filter_map(|n| n.attribute("ContentType"))
        .any(|t| t.to_ascii_lowercase().contains("macroenabled") || t.contains("digital-signature"))
    {
        return Err("signed or macro-enabled workbooks cannot be edited".into());
    }
    let rels = relationships(part(parts, "_rels/.rels")?, "")?;
    let books: Vec<_> = rels
        .values()
        .filter(|(kind, _)| kind == &format!("{OFFICE_REL}/officeDocument"))
        .collect();
    if books.len() != 1 || books[0].1.is_empty() {
        return Err("the package must contain one internal workbook".into());
    }
    let workbook = &books[0].1;
    let workbook_xml = part(parts, workbook)?;
    let doc = parse_xml(workbook_xml)?;
    if !doc.root_element().has_tag_name((MAIN, "workbook")) {
        return Err("only transitional .xlsx workbooks are supported".into());
    }
    let workbook_kind = content
        .root_element()
        .children()
        .find(|n| {
            n.attribute("PartName")
                .is_some_and(|name| name.strip_prefix('/') == Some(workbook.as_str()))
        })
        .and_then(|n| n.attribute("ContentType"));
    if workbook_kind
        != Some("application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml")
    {
        return Err("the package is not a plain XLSX workbook".into());
    }
    let (parent, filename) = workbook.rsplit_once('/').unwrap_or(("", workbook));
    let rel_name = if parent.is_empty() {
        format!("_rels/{filename}.rels")
    } else {
        format!("{parent}/_rels/{filename}.rels")
    };
    let sheet_rels = relationships(part(parts, &rel_name)?, workbook)?;
    let sheets = doc
        .root_element()
        .children()
        .find(|n| n.has_tag_name((MAIN, "sheets")))
        .ok_or("the workbook has no sheets")?;
    let mut names = BTreeSet::new();
    let mut paths = BTreeSet::new();
    let mut by_sheet: BTreeMap<String, String> = BTreeMap::new();
    for sheet in sheets.children().filter(Node::is_element) {
        if !sheet.has_tag_name((MAIN, "sheet")) {
            return Err("unsupported sheet metadata".into());
        }
        let name = sheet.attribute("name").ok_or("a sheet has no name")?;
        if !names.insert(name.to_lowercase()) {
            return Err("ambiguous sheet names".into());
        }
        let id = sheet
            .attribute((OFFICE_REL, "id"))
            .ok_or("a sheet has no relationship")?;
        let (kind, target) = sheet_rels
            .get(id)
            .ok_or("a worksheet relationship is missing")?;
        if !target.is_empty() && kind == &format!("{OFFICE_REL}/worksheet") {
            if !paths.insert(target.clone()) {
                return Err("multiple sheets share one worksheet part".into());
            }
            let worksheet = parse_xml(part(parts, target)?)?;
            if !worksheet.root_element().has_tag_name((MAIN, "worksheet")) {
                return Err("a worksheet part has an invalid root".into());
            }
            by_sheet.insert(name.into(), target.clone());
        }
    }
    Ok(Workbook {
        name: workbook.clone(),
        sheets: by_sheet,
    })
}

fn edit_package(bytes: &[u8], edits: &[CellEdit]) -> Result<Vec<u8>, String> {
    let parts = read_parts(bytes)?;
    let workbook = resolve_workbook(&parts)?;
    let workbook_xml = part(&parts, &workbook.name)?;
    let by_sheet = &workbook.sheets;
    let mut grouped: BTreeMap<&str, Vec<&CellEdit>> = BTreeMap::new();
    for edit in edits {
        let path = by_sheet
            .get(&edit.sheet)
            .ok_or("the requested worksheet does not exist or is not editable")?;
        grouped.entry(path).or_default().push(edit);
    }
    let mut changed = BTreeMap::new();
    for (path, cells) in grouped {
        changed.insert(
            path.to_owned(),
            edit_sheet(part(&parts, path)?, &cells)?.into_bytes(),
        );
    }
    changed.insert(
        workbook.name.clone(),
        recalculate(workbook_xml)?.into_bytes(),
    );
    let mut archive = ZipArchive::new(Cursor::new(bytes)).map_err(zip_error)?;
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    writer
        .set_raw_comment(archive.comment().into())
        .map_err(zip_error)?;
    for index in 0..archive.len() {
        let file = archive.by_index(index).map_err(zip_error)?;
        let name = file.name().map_err(zip_error)?.into_owned();
        if let Some(replacement) = changed.get(&name) {
            writer.start_file(name, file.options()).map_err(zip_error)?;
            writer.write_all(replacement).map_err(zip_error)?;
        } else {
            writer.raw_copy_file(file).map_err(zip_error)?;
        }
    }
    let result = writer.finish().map_err(zip_error)?.into_inner();
    if result.len() > MAX_BYTES {
        return Err("the edited workbook would exceed 10 MiB".into());
    }
    Ok(result)
}

#[derive(Debug)]
struct Patch {
    range: Range<usize>,
    text: String,
}

fn apply(text: &str, mut patches: Vec<Patch>) -> Result<String, String> {
    patches.sort_by_key(|patch| (patch.range.start, patch.range.end));
    let mut result = String::new();
    let mut cursor = 0;
    for patch in patches {
        if patch.range.start < cursor {
            return Err("ambiguous overlapping XML edits".into());
        }
        result.push_str(&text[cursor..patch.range.start]);
        result.push_str(&patch.text);
        cursor = patch.range.end;
    }
    result.push_str(&text[cursor..]);
    Ok(result)
}

fn opening_end(text: &str, node: Node<'_, '_>) -> Result<usize, String> {
    let mut quote = None;
    for (offset, ch) in text[node.range()].char_indices() {
        match (quote, ch) {
            (None, '\'' | '"') => quote = Some(ch),
            (Some(q), c) if q == c => quote = None,
            (None, '>') => return Ok(node.range().start + offset + 1),
            _ => {}
        }
    }
    Err("invalid XML opening element".into())
}

fn qname<'a>(text: &'a str, node: Node<'_, '_>) -> &'a str {
    text[node.range()]
        .trim_start_matches('<')
        .split(|c: char| c.is_whitespace() || c == '/' || c == '>')
        .next()
        .unwrap_or("")
}

fn qualified(parent: &str, name: &str) -> String {
    parent
        .split_once(':')
        .map_or_else(|| name.into(), |(prefix, _)| format!("{prefix}:{name}"))
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\r', "&#13;")
}

fn office_text(text: &str) -> String {
    let mut escaped = String::new();
    for (index, character) in text.char_indices() {
        let rest = &text.as_bytes()[index..];
        if character == '_'
            && rest.len() >= 7
            && rest[1] == b'x'
            && rest[6] == b'_'
            && rest[2..6].iter().all(u8::is_ascii_hexdigit)
        {
            escaped.push_str("_x005F_");
        } else if character == '\r' {
            escaped.push_str("_x000D_");
        } else {
            escaped.push(character);
        }
    }
    escape(&escaped)
}

fn start_tag(
    text: &str,
    node: Node<'_, '_>,
    attributes: &[(&str, Option<&str>)],
) -> Result<String, String> {
    let start = node.range().start;
    let end = opening_end(text, node)?;
    let mut patches = Vec::new();
    let mut additions = String::new();
    for (name, value) in attributes {
        if let Some(attribute) = node
            .attributes()
            .find(|a| a.namespace().is_none() && a.name() == *name)
        {
            let range = attribute.range();
            patches.push(Patch {
                range: range.start - start..range.end - start,
                text: value.map_or_else(String::new, |v| format!("{name}=\"{}\"", escape(v))),
            });
        } else if let Some(value) = value {
            additions.push(' ');
            additions.push_str(name);
            additions.push_str("=\"");
            additions.push_str(&escape(value));
            additions.push('"');
        }
    }
    let mut tag = apply(&text[start..end], patches)?;
    if tag.ends_with("/>") {
        tag.truncate(tag.len() - 2);
    } else {
        tag.pop();
    }
    tag.push_str(&additions);
    tag.push('>');
    Ok(tag)
}

fn cell_xml(
    text: &str,
    node: Option<Node<'_, '_>>,
    prefix: &str,
    edit: &CellEdit,
) -> Result<String, String> {
    let name = node.map_or_else(|| qualified(prefix, "c"), |n| qname(text, n).into());
    let kind = match edit.value {
        Value::String(_) => Some("inlineStr"),
        Value::Bool(_) => Some("b"),
        _ => None,
    };
    let opening = if let Some(node) = node {
        if node
            .attributes()
            .any(|a| a.namespace().is_some() || !matches!(a.name(), "r" | "s" | "t"))
            || node
                .children()
                .filter(Node::is_element)
                .any(|n| !n.has_tag_name((MAIN, "v")) && !n.has_tag_name((MAIN, "is")))
        {
            return Err("formula or metadata-bearing cells cannot be replaced".into());
        }
        start_tag(text, node, &[("t", kind)])?
    } else {
        format!(
            "<{name} r=\"{}\"{}>",
            edit.address,
            kind.map_or_else(String::new, |t| format!(" t=\"{t}\""))
        )
    };
    let v = qualified(&name, "v");
    let content = match &edit.value {
        Value::Null => String::new(),
        Value::Bool(value) => format!("<{v}>{}</{v}>", u8::from(*value)),
        Value::Number(value) => format!("<{v}>{value}</{v}>"),
        Value::String(value) => {
            let is = qualified(&name, "is");
            let t = qualified(&name, "t");
            format!(
                "<{is}><{t} xml:space=\"preserve\">{}</{t}></{is}>",
                office_text(value)
            )
        }
        _ => return Err("invalid literal cell value".into()),
    };
    Ok(format!("{opening}{content}</{name}>"))
}

fn contains(reference: &str, edit: &CellEdit) -> Result<bool, String> {
    let reference = reference.replace('$', "");
    let (first, last) = reference
        .split_once(':')
        .unwrap_or((&reference, &reference));
    let (r1, c1) = coordinate(first)?;
    let (r2, c2) = coordinate(last)?;
    if r1 > r2 || c1 > c2 {
        return Err("invalid worksheet cell range".into());
    }
    Ok((r1..=r2).contains(&edit.row) && (c1..=c2).contains(&edit.column))
}

fn check_sheet_policy(root: Node<'_, '_>, edits: &[&CellEdit]) -> Result<(), String> {
    if !root.has_tag_name((MAIN, "worksheet")) {
        return Err("the worksheet part is invalid".into());
    }
    for node in root.descendants().filter(Node::is_element) {
        if node.has_tag_name((MAIN, "sheetProtection")) {
            return Err("protected worksheets cannot be edited".into());
        }
        if node.has_tag_name((MAIN, "f")) {
            if let Some(reference) = node.attribute("ref") {
                for edit in edits {
                    if contains(reference, edit)? {
                        return Err("formula result ranges cannot be edited".into());
                    }
                }
            } else if matches!(node.attribute("t"), Some("array" | "dataTable")) {
                return Err("a formula result range is missing its bounds".into());
            }
        }
        if node.has_tag_name((MAIN, "mergeCell")) {
            let reference = node.attribute("ref").ok_or("a merged cell has no range")?;
            let anchor = reference.split(':').next().unwrap_or("").replace('$', "");
            for edit in edits {
                if contains(reference, edit)? && edit.address != anchor {
                    return Err("only the top-left cell of a merged range can be edited".into());
                }
            }
        }
    }
    Ok(())
}

fn ordered_rows<'a, 'input>(
    data: Node<'a, 'input>,
) -> Result<BTreeMap<u32, Node<'a, 'input>>, String> {
    let mut rows = BTreeMap::new();
    let mut previous = 0;
    for row in data.children().filter(Node::is_element) {
        if !row.has_tag_name((MAIN, "row")) {
            return Err("unsupported worksheet row structure".into());
        }
        let number = row
            .attribute("r")
            .and_then(|r| r.parse::<u32>().ok())
            .ok_or("a worksheet row has no explicit number")?;
        if number <= previous || number > 1_048_576 {
            return Err("unordered or duplicate worksheet rows".into());
        }
        previous = number;
        rows.insert(number, row);
        let mut column = 0;
        for cell in row.children().filter(Node::is_element) {
            if !cell.has_tag_name((MAIN, "c")) {
                return Err("unsupported worksheet cell structure".into());
            }
            let (r, c) = coordinate(
                cell.attribute("r")
                    .ok_or("a cell has no explicit address")?,
            )?;
            if r != number || c <= column {
                return Err("unordered, duplicate or misplaced worksheet cells".into());
            }
            column = c;
        }
    }
    Ok(rows)
}

fn edit_sheet(bytes: &[u8], edits: &[&CellEdit]) -> Result<String, String> {
    let doc = parse_xml(bytes)?;
    let text = std::str::from_utf8(bytes).map_err(|_| "invalid worksheet text")?;
    let root = doc.root_element();
    check_sheet_policy(root, edits)?;
    let data_nodes: Vec<_> = root
        .children()
        .filter(|n| n.has_tag_name((MAIN, "sheetData")))
        .collect();
    if data_nodes.len() != 1 {
        return Err("the worksheet must have exactly one sheetData element".into());
    }
    let data = data_nodes[0];
    let rows = ordered_rows(data)?;
    let mut edits_by_row: BTreeMap<u32, Vec<&CellEdit>> = BTreeMap::new();
    for edit in edits {
        edits_by_row.entry(edit.row).or_default().push(edit);
    }
    let mut patches = Vec::new();
    let mut insertions: BTreeMap<usize, String> = BTreeMap::new();
    let data_name = qname(text, data);
    let data_close = text[data.range()]
        .rfind("</")
        .map(|offset| data.range().start + offset);
    for (number, mut row_edits) in edits_by_row {
        row_edits.sort_by_key(|e| e.column);
        if let Some(row) = rows.get(&number) {
            patches.push(Patch {
                range: row.range(),
                text: edit_row(text, *row, &row_edits)?,
            });
        } else {
            let name = qualified(data_name, "row");
            let mut added = format!("<{name} r=\"{number}\">");
            for edit in row_edits {
                added.push_str(&cell_xml(text, None, &name, edit)?);
            }
            added.push_str("</");
            added.push_str(&name);
            added.push('>');
            let at = rows
                .range(number..)
                .next()
                .map(|(_, row)| row.range().start)
                .or(data_close)
                .unwrap_or(data.range().start);
            insertions.entry(at).or_default().push_str(&added);
        }
    }
    if data_close.is_none() {
        let added = insertions.remove(&data.range().start).unwrap_or_default();
        let opening = start_tag(text, data, &[])?;
        patches.push(Patch {
            range: data.range(),
            text: format!("{opening}{added}</{data_name}>"),
        });
    }
    for (at, added) in insertions {
        patches.push(Patch {
            range: at..at,
            text: added,
        });
    }
    // Dimension is optional derived metadata; omitting it avoids stale used-range bounds.
    for dimension in root
        .children()
        .filter(|n| n.has_tag_name((MAIN, "dimension")))
    {
        patches.push(Patch {
            range: dimension.range(),
            text: String::new(),
        });
    }
    apply(text, patches)
}

fn edit_row(text: &str, row: Node<'_, '_>, edits: &[&CellEdit]) -> Result<String, String> {
    let name = qname(text, row);
    let cells: BTreeMap<_, _> = row
        .children()
        .filter(Node::is_element)
        .map(|node| coordinate(node.attribute("r").unwrap_or("")).map(|(_, column)| (column, node)))
        .collect::<Result<_, _>>()?;
    let opening_end = opening_end(text, row)?;
    let close = text[row.range()]
        .rfind("</")
        .map(|offset| row.range().start + offset);
    let mut patches = Vec::new();
    let mut insertions: BTreeMap<usize, String> = BTreeMap::new();
    for edit in edits {
        if let Some(cell) = cells.get(&edit.column) {
            patches.push(Patch {
                range: cell.range(),
                text: cell_xml(text, Some(*cell), name, edit)?,
            });
        } else {
            let at = cells
                .range(edit.column..)
                .next()
                .map(|(_, cell)| cell.range().start)
                .or(close)
                .unwrap_or(opening_end);
            insertions
                .entry(at)
                .or_default()
                .push_str(&cell_xml(text, None, name, edit)?);
        }
    }
    let opening = start_tag(text, row, &[("spans", None)])?;
    if close.is_none() {
        let added = insertions.remove(&opening_end).unwrap_or_default();
        return Ok(format!("{opening}{added}</{name}>"));
    }
    patches.push(Patch {
        range: row.range().start..opening_end,
        text: opening,
    });
    for (at, text) in insertions {
        patches.push(Patch {
            range: at..at,
            text,
        });
    }
    let base = row.range().start;
    for patch in &mut patches {
        patch.range = patch.range.start - base..patch.range.end - base;
    }
    apply(&text[row.range()], patches)
}

fn recalculate(bytes: &[u8]) -> Result<String, String> {
    let doc = parse_xml(bytes)?;
    let text = std::str::from_utf8(bytes).map_err(|_| "invalid workbook XML")?;
    let root = doc.root_element();
    let nodes: Vec<_> = root
        .children()
        .filter(|n| n.has_tag_name((MAIN, "calcPr")))
        .collect();
    if nodes.len() > 1 {
        return Err("duplicate workbook calculation settings".into());
    }
    let attributes = [("fullCalcOnLoad", Some("1")), ("forceFullCalc", Some("1"))];
    let patch = if let Some(node) = nodes.first() {
        let tag = start_tag(text, *node, &attributes)?;
        Patch {
            range: node.range(),
            text: format!("{tag}</{}>", qname(text, *node)),
        }
    } else {
        let name = qualified(qname(text, root), "calcPr");
        let after_calc = [
            "oleSize",
            "customWorkbookViews",
            "pivotCaches",
            "smartTagPr",
            "smartTagTypes",
            "webPublishing",
            "fileRecoveryPr",
            "webPublishObjects",
            "extLst",
        ];
        let at = root
            .children()
            .find(|n| {
                n.tag_name().namespace() == Some(MAIN) && after_calc.contains(&n.tag_name().name())
            })
            .map(|n| n.range().start)
            .or_else(|| {
                text[root.range()]
                    .rfind("</")
                    .map(|n| root.range().start + n)
            })
            .ok_or("invalid empty workbook")?;
        Patch {
            range: at..at,
            text: format!("<{name} fullCalcOnLoad=\"1\" forceFullCalc=\"1\"/>"),
        }
    };
    apply(text, vec![patch])
}

#[cfg(test)]
#[path = "xlsx_workbook_tests.rs"]
mod tests;
