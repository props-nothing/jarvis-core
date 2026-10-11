//! Tests for writing documents. The strongest check available offline is the crate's own reader (written independently of the writers),
//! plus a look inside the archives for what must not be there. The files were also opened in the real Excel and Word (`ADR-0185`).

use std::io::Read;

use serde_json::json;

use super::*;
use crate::{Kind, extract};

fn archive_part(bytes: &[u8], name: &str) -> String {
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes))
        .unwrap_or_else(|error| panic!("not a zip: {error}"));
    let mut text = String::new();
    archive
        .by_name(name)
        .unwrap_or_else(|error| panic!("{name} missing: {error}"))
        .read_to_string(&mut text)
        .unwrap_or_else(|error| panic!("{error}"));
    text
}

fn prospects() -> Value {
    json!({
        "sheets": [
            {
                "name": "Prospects",
                "header": ["Bedrijf", "Plaats", "Medewerkers", "Actief"],
                "rows": [
                    ["Café Zoë B.V.", "Den Haag", 12, true],
                    ["Müller & Zn", null, 3.5, false],
                    ["=SUM(A1:A9)", "@home", 0, true]
                ]
            },
            { "name": "Notes", "rows": [["one line"], ["twee"]] }
        ]
    })
}

#[test]
fn a_workbook_reads_back_with_its_sheets_text_and_numbers() {
    let bytes = write_xlsx(&prospects()).unwrap_or_else(|error| panic!("{error}"));
    let read = extract("prospects.xlsx", &bytes).unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(read.kind, Kind::Spreadsheet);
    assert_eq!(read.parts, 2);
    for wanted in [
        "Prospects",
        "Notes",
        "Bedrijf",
        "Café Zoë B.V.",
        "Den Haag",
        "Müller & Zn",
        "12",
        "3.5",
        "twee",
    ] {
        assert!(
            read.text.contains(wanted),
            "{wanted:?} missing from {}",
            read.text
        );
    }
}

/// **Text is text: a value that looks like a formula is stored as a string and can never be evaluated.**
#[test]
fn a_formula_looking_value_is_stored_as_text_never_as_a_formula() {
    let bytes = write_xlsx(&prospects()).unwrap_or_else(|error| panic!("{error}"));
    let sheet = archive_part(&bytes, "xl/worksheets/sheet1.xml");
    assert!(
        !sheet.contains("<f>") && !sheet.contains("<f "),
        "a formula was written: {sheet}"
    );
    let strings = archive_part(&bytes, "xl/sharedStrings.xml");
    assert!(strings.contains("=SUM(A1:A9)") && strings.contains("@home"));
}

#[test]
fn a_header_is_bold_frozen_and_filtered_and_columns_fit_their_content() {
    let bytes = write_xlsx(&prospects()).unwrap_or_else(|error| panic!("{error}"));
    let sheet = archive_part(&bytes, "xl/worksheets/sheet1.xml");
    assert!(sheet.contains("<pane"), "no frozen header row");
    assert!(sheet.contains("<autoFilter"), "no filter");
    assert!(sheet.contains("<col "), "no column widths");
    let styles = archive_part(&bytes, "xl/styles.xml");
    assert!(
        styles.contains("<b/>") || styles.contains("<b "),
        "no bold header font"
    );
}

#[test]
fn sheet_names_are_made_valid_and_unique() {
    let content = json!({ "sheets": [
        { "name": "A/B:C*?[x]", "rows": [[1]] },
        { "name": "A/B:C*?[x]", "rows": [[2]] },
        { "name": "x".repeat(60), "rows": [[3]] },
        { "name": "   ", "rows": [[4]] },
        { "rows": [[5]] }
    ]});
    let bytes = write_xlsx(&content).unwrap_or_else(|error| panic!("{error}"));
    let workbook = archive_part(&bytes, "xl/workbook.xml");
    assert!(workbook.contains("name=\"ABCx\""), "{workbook}");
    assert!(workbook.contains("name=\"ABCx 2\""), "{workbook}");
    assert!(
        workbook.contains(&format!("name=\"{}\"", "x".repeat(31))),
        "{workbook}"
    );
    assert_eq!(workbook.matches("<sheet ").count(), 5);
}

#[test]
fn one_sheet_may_be_given_directly() {
    let bytes = write_xlsx(&json!({ "header": ["a"], "rows": [[1], [2]] }))
        .unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(
        extract("x.xlsx", &bytes).map(|read| read.parts).ok(),
        Some(1)
    );
}

#[test]
fn content_of_the_wrong_shape_or_size_is_refused_in_words() {
    let refused = |content: Value| {
        write_xlsx(&content)
            .err()
            .unwrap_or_else(|| panic!("accepted {content}"))
    };
    assert!(matches!(refused(json!({})), WriteError::Invalid(_)));
    assert!(matches!(
        refused(json!({ "sheets": [] })),
        WriteError::Invalid(_)
    ));
    assert!(matches!(
        refused(json!({ "rows": "no" })),
        WriteError::Invalid(_)
    ));
    assert!(matches!(
        refused(json!({ "rows": [1] })),
        WriteError::Invalid(_)
    ));
    assert!(matches!(
        refused(json!({ "rows": [[{ "a": 1 }]] })),
        WriteError::Invalid(_)
    ));
    assert!(matches!(
        refused(json!({ "rows": [[[1]]] })),
        WriteError::Invalid(_)
    ));
    let sheets: Vec<Value> = (0..21).map(|_| json!({ "rows": [[1]] })).collect();
    assert!(matches!(
        refused(json!({ "sheets": sheets })),
        WriteError::TooLarge(_)
    ));
    assert!(matches!(
        refused(json!({ "rows": [["x".repeat(32_001)]] })),
        WriteError::TooLarge(_)
    ));
    let wide: Vec<Value> = (0..101).map(|_| json!(1)).collect();
    assert!(matches!(
        refused(json!({ "rows": [wide] })),
        WriteError::TooLarge(_)
    ));
    let many: Vec<Value> = (0..2100)
        .map(|_| Value::Array((0..100).map(|_| json!(1)).collect()))
        .collect();
    assert!(matches!(
        refused(json!({ "rows": many })),
        WriteError::TooLarge(_)
    ));
}

#[test]
fn a_csv_has_a_byte_order_mark_crlf_and_quoting_and_never_a_live_formula() {
    let bytes = write_csv(&prospects(), ',').unwrap_or_else(|error| panic!("{error}"));
    let text = String::from_utf8(bytes).unwrap_or_else(|error| panic!("{error}"));
    assert!(text.starts_with('\u{feff}'));
    let lines: Vec<&str> = text.trim_start_matches('\u{feff}').split("\r\n").collect();
    assert_eq!(lines[0], "Bedrijf,Plaats,Medewerkers,Actief");
    assert_eq!(lines[1], "Café Zoë B.V.,Den Haag,12,true");
    assert_eq!(lines[2], "Müller & Zn,,3.5,false");
    assert_eq!(lines[3], "'=SUM(A1:A9),'@home,0,true");
    let quoted = write_csv(
        &json!({ "rows": [["a,b", "say \"hi\"", "two\nlines", "-5"]] }),
        ',',
    )
    .unwrap_or_else(|error| panic!("{error}"));
    let quoted = String::from_utf8(quoted).unwrap_or_default();
    assert!(
        quoted.contains("\"a,b\",\"say \"\"hi\"\"\",\"two\nlines\",'-5"),
        "{quoted:?}"
    );
}

#[test]
fn a_semicolon_csv_is_for_dutch_excel_and_a_number_stays_a_number() {
    let bytes = write_csv(&json!({ "header": ["a", "b"], "rows": [["x;y", -5]] }), ';')
        .unwrap_or_else(|error| panic!("{error}"));
    let text = String::from_utf8(bytes).unwrap_or_default();
    assert!(text.contains("a;b\r\n\"x;y\";-5\r\n"), "{text:?}");
    assert!(matches!(
        write_csv(&prospects(), '\t'),
        Err(WriteError::Invalid(_))
    ));
}

fn report() -> Value {
    json!({
        "title": "Weekrapport",
        "blocks": [
            { "type": "heading", "level": 1, "text": "Samenvatting" },
            { "type": "paragraph", "text": "Eerste regel\nTweede regel met é en €." },
            { "type": "bullets", "items": ["appel", "peer"] },
            { "type": "numbered", "items": ["eerst", "dan"] },
            { "type": "heading", "level": 2, "text": "Cijfers" },
            { "type": "table", "header": ["Naam", "Aantal"], "rows": [["Jan", 3], ["Piet", 4.5]] },
            { "type": "paragraph", "text": "Klaar.", "bold": true }
        ]
    })
}

#[test]
fn a_document_reads_back_in_order() {
    let bytes = write_docx(&report()).unwrap_or_else(|error| panic!("{error}"));
    let read = extract("report.docx", &bytes).unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(read.kind, Kind::Word);
    let order = [
        "Weekrapport",
        "Samenvatting",
        "Eerste regel",
        "Tweede regel met é en €.",
        "appel",
        "peer",
        "eerst",
        "dan",
        "Cijfers",
        "Naam",
        "Jan",
        "4.5",
        "Klaar.",
    ];
    let mut from = 0;
    for wanted in order {
        let at = read.text[from..]
            .find(wanted)
            .unwrap_or_else(|| panic!("{wanted:?} missing or out of order in {:?}", read.text));
        from += at + wanted.len();
    }
}

#[test]
fn a_document_declares_its_heading_styles_lists_and_a_table() {
    let bytes = write_docx(&report()).unwrap_or_else(|error| panic!("{error}"));
    let body = archive_part(&bytes, "word/document.xml");
    assert!(body.contains("Heading1") && body.contains("Heading2") && body.contains("Title"));
    assert!(body.contains("<w:tbl>"), "no table");
    assert!(body.contains("<w:numPr>"), "no list numbering");
    let styles = archive_part(&bytes, "word/styles.xml");
    assert!(styles.contains("Heading1") && styles.contains("Title"));
    let numbering = archive_part(&bytes, "word/numbering.xml");
    assert!(numbering.contains("bullet") && numbering.contains("decimal"));
}

#[test]
fn a_document_of_the_wrong_shape_is_refused_in_words() {
    let refused = |content: Value| {
        write_docx(&content)
            .err()
            .unwrap_or_else(|| panic!("accepted {content}"))
    };
    assert!(matches!(refused(json!({})), WriteError::Invalid(_)));
    assert!(matches!(
        refused(json!({ "blocks": [{}] })),
        WriteError::Invalid(_)
    ));
    assert!(matches!(
        refused(json!({ "blocks": [{ "type": "script", "text": "x" }] })),
        WriteError::Invalid(_)
    ));
    assert!(matches!(
        refused(json!({ "blocks": [{ "type": "paragraph" }] })),
        WriteError::Invalid(_)
    ));
    assert!(matches!(
        refused(json!({ "blocks": [{ "type": "bullets", "items": [1] }] })),
        WriteError::Invalid(_)
    ));
    assert!(matches!(
        refused(json!({ "blocks": [{ "type": "table", "rows": [] }] })),
        WriteError::Invalid(_)
    ));
    let many: Vec<Value> = (0..2001)
        .map(|_| json!({ "type": "paragraph", "text": "x" }))
        .collect();
    assert!(matches!(
        refused(json!({ "blocks": many })),
        WriteError::TooLarge(_)
    ));
}

#[test]
fn markup_in_text_is_escaped_not_interpreted() {
    let bytes = write_docx(
        &json!({ "blocks": [{ "type": "paragraph", "text": "<w:p>& \"quoted\" </w:t><script>" }] }),
    )
    .unwrap_or_else(|error| panic!("{error}"));
    let body = archive_part(&bytes, "word/document.xml");
    assert!(!body.contains("<script>"), "{body}");
    let read = extract("x.docx", &bytes).unwrap_or_else(|error| panic!("{error}"));
    assert!(
        read.text.contains("<w:p>& \"quoted\" </w:t><script>"),
        "{}",
        read.text
    );
}
