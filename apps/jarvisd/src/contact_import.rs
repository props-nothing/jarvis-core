//! Strict, bounded CSV input for contacts. This parser accepts comma-separated UTF-8, not spreadsheet dialect guessing.

use std::io::Read;

use jarvis_storage::{ContactInput, ContactStatus};
use jarvis_tools::{AdapterError, WorkspaceRoots};

use super::refused;

const MAX_BYTES: u64 = 2 * 1024 * 1024;
const MAX_ROWS: usize = 1_000;
const COLUMNS: [&str; 8] = [
    "company", "person", "role", "email", "phone", "status", "notes", "source",
];

pub(super) fn read(roots: &WorkspaceRoots, name: &str) -> Result<Vec<ContactInput>, AdapterError> {
    let path = crate::google_files::plain_relative(name).ok_or_else(|| {
        refused("the CSV path must be inside a granted folder (no drive, no .., not absolute)")
    })?;
    let (file, _) = roots
        .open_file(&path)
        .map_err(|_| refused("the CSV could not be opened inside the granted folders"))?;
    let metadata = file
        .metadata()
        .map_err(|_| refused("the CSV file could not be inspected"))?;
    if !metadata.is_file() || metadata.len() > MAX_BYTES {
        return Err(refused(
            "the CSV must be a regular file no larger than 2 MiB",
        ));
    }
    let mut bytes = Vec::new();
    file.take(MAX_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| refused("the CSV could not be read"))?;
    if bytes.len() as u64 > MAX_BYTES {
        return Err(refused(
            "the CSV is larger than 2 MiB; nothing was imported",
        ));
    }
    parse(&bytes)
}

fn parse(bytes: &[u8]) -> Result<Vec<ContactInput>, AdapterError> {
    let text = std::str::from_utf8(bytes)
        .map_err(|_| refused("the CSV must be UTF-8; nothing was imported"))?;
    let mut records = records(text.strip_prefix('\u{feff}').unwrap_or(text))?.into_iter();
    let header = records
        .next()
        .ok_or_else(|| refused("the CSV needs a header and at least one contact"))?;
    let header: Vec<String> = header
        .iter()
        .map(|name| name.trim().to_ascii_lowercase())
        .collect();
    for (index, name) in header.iter().enumerate() {
        if name.is_empty() || !COLUMNS.contains(&name.as_str()) || header[..index].contains(name) {
            return Err(refused(
                "the CSV has an unknown, blank or repeated header; nothing was imported",
            ));
        }
    }
    if !header.iter().any(|name| name == "company") {
        return Err(refused(
            "the CSV needs a company column; nothing was imported",
        ));
    }
    let mut inputs = Vec::new();
    for row in records {
        if row.len() != header.len() {
            return Err(refused(
                "a CSV row has the wrong number of columns; nothing was imported",
            ));
        }
        let mut input = ContactInput::default();
        for (name, value) in header.iter().zip(row) {
            match name.as_str() {
                "company" => input.company = Some(value),
                "person" => input.person = Some(value),
                "role" => input.role = Some(value),
                "email" => input.email = Some(value),
                "phone" => input.phone = Some(value),
                "notes" => input.notes = Some(value),
                "source" => input.source = Some(value),
                "status" => {
                    let value = value.trim();
                    if !value.is_empty() {
                        input.status = Some(ContactStatus::parse(value).ok_or_else(|| {
                            refused("a CSV row has an unknown status; nothing was imported")
                        })?);
                    }
                }
                _ => unreachable!("headers were validated"),
            }
        }
        if input
            .company
            .as_deref()
            .is_none_or(|value| value.trim().is_empty())
        {
            return Err(refused(
                "every CSV row needs a company; nothing was imported",
            ));
        }
        inputs.push(input);
    }
    if inputs.is_empty() {
        return Err(refused(
            "the CSV needs at least one contact; nothing was imported",
        ));
    }
    Ok(inputs)
}

#[derive(Clone, Copy)]
enum State {
    Start,
    Plain,
    Quoted,
    Closed,
}

fn records(text: &str) -> Result<Vec<Vec<String>>, AdapterError> {
    let malformed =
        || refused("the CSV has malformed quoting or line endings; nothing was imported");
    let mut characters = text.chars().peekable();
    let mut state = State::Start;
    let mut field = String::new();
    let mut row = Vec::new();
    let mut rows = Vec::new();
    while let Some(mut character) = characters.next() {
        if character == '\r' {
            if characters.next() != Some('\n') {
                return Err(malformed());
            }
            character = '\n';
        }
        match (state, character) {
            (State::Quoted, '"') if characters.peek() == Some(&'"') => {
                characters.next();
                field.push('"');
            }
            (State::Quoted, '"') => state = State::Closed,
            (State::Quoted, character) => field.push(character),
            (_, ',' | '\n') => {
                row.push(std::mem::take(&mut field));
                state = State::Start;
                if row.len() > COLUMNS.len() {
                    return Err(refused(
                        "the CSV has too many columns; nothing was imported",
                    ));
                }
                if character == '\n' {
                    rows.push(std::mem::take(&mut row));
                    if rows.len() > MAX_ROWS + 1 {
                        return Err(refused(
                            "the CSV exceeds 1,000 contacts; nothing was imported",
                        ));
                    }
                }
            }
            (State::Start, '"') => state = State::Quoted,
            (State::Closed, _) | (State::Plain, '"') => return Err(malformed()),
            (_, character) => {
                field.push(character);
                state = State::Plain;
            }
        }
        if field.len() > 8_000 {
            return Err(refused("a CSV field is too long; nothing was imported"));
        }
    }
    if matches!(state, State::Quoted) {
        return Err(malformed());
    }
    if !row.is_empty() || !field.is_empty() || !matches!(state, State::Start) {
        row.push(field);
        rows.push(row);
    }
    if rows.len() > MAX_ROWS + 1 || rows.iter().any(|row| row.len() > COLUMNS.len()) {
        return Err(refused(
            "the CSV exceeds 1,000 contacts or has too many columns; nothing was imported",
        ));
    }
    Ok(rows)
}
