//! The PDF writer: what it writes reads back, wraps within the page, and fails in words.

use serde_json::json;

use super::*;
use crate::extract;

fn report() -> Value {
    json!({
        "title": "Quarterly report",
        "blocks": [
            { "type": "heading", "level": 1, "text": "Summary" },
            { "type": "paragraph", "text": "Revenue grew in every region." },
            { "type": "bullets", "items": ["First point", "Second point"] },
            { "type": "numbered", "items": ["Do this", "Then that"] },
            { "type": "table", "header": ["Region", "Revenue"], "rows": [["North", 1200], ["South", 950.5]] }
        ]
    })
}

#[test]
fn a_document_reads_back_with_its_title_blocks_and_footer() {
    let (bytes, replaced) = write_pdf(&report()).unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(replaced, 0);
    assert!(bytes.starts_with(b"%PDF-"));
    let found = extract("report.pdf", &bytes).unwrap_or_else(|error| panic!("{error}"));
    for wanted in [
        "Quarterly report",
        "Summary",
        "Revenue grew in every region.",
        "First point",
        "Second point",
        "Do this",
        "Region",
        "North",
        "1200",
        "950.5",
        "Page 1 of 1",
    ] {
        assert!(
            found.text.contains(wanted),
            "{wanted:?} missing from {}",
            found.text
        );
    }
    assert_eq!(found.parts, 1);
}

#[test]
fn accents_the_euro_sign_and_typographic_quotes_are_kept() {
    let content = json!({ "blocks": [{ "type": "paragraph", "text": "Café Zoë kost €12 – “mooi” \u{2018}ja\u{2019}" }] });
    let (bytes, replaced) = write_pdf(&content).unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(replaced, 0);
    let found = extract("a.pdf", &bytes).unwrap_or_else(|error| panic!("{error}"));
    for wanted in ["Café", "Zoë", "€12", "“mooi”"] {
        assert!(
            found.text.contains(wanted),
            "{wanted:?} missing from {}",
            found.text
        );
    }
}

#[test]
fn what_the_fonts_cannot_show_is_counted_not_hidden() {
    let content =
        json!({ "blocks": [{ "type": "paragraph", "text": "ok \u{4F60}\u{597D} \u{1F600}" }] });
    let (bytes, replaced) = write_pdf(&content).unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(replaced, 3);
    let found = extract("a.pdf", &bytes).unwrap_or_else(|error| panic!("{error}"));
    assert!(found.text.contains("ok ?? ?"), "{}", found.text);
}

#[test]
fn brackets_backslashes_and_markup_do_not_break_the_file() {
    let nasty = r"a (b) c \ d ) ( \) <script>alert(1)</script> /JavaScript";
    let content = json!({ "blocks": [{ "type": "paragraph", "text": nasty }] });
    let (bytes, _) = write_pdf(&content).unwrap_or_else(|error| panic!("{error}"));
    let document = lopdf::Document::load_mem(&bytes).unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(document.get_pages().len(), 1);
    let found = extract("a.pdf", &bytes).unwrap_or_else(|error| panic!("{error}"));
    assert!(found.text.contains(nasty), "{}", found.text);
    // The text is drawn, never interpreted: no action or script object exists.
    let text = String::from_utf8_lossy(&bytes);
    assert!(!text.contains("/OpenAction") && !text.contains("/JS"));
}

#[test]
fn the_published_widths_are_the_ones_in_use() {
    assert_eq!(
        (
            HELVETICA[usize::from(b'A') - 32],
            HELVETICA[usize::from(b'i') - 32]
        ),
        (667, 222)
    );
    assert_eq!(HELVETICA_BOLD[usize::from(b'A') - 32], 722);
    assert_eq!(HELVETICA[usize::from(b' ') - 32], 278);
}

#[test]
fn lines_wrap_within_the_width_and_a_long_word_is_cut() {
    let mut replaced = 0;
    let text = "lorem ipsum dolor sit amet ".repeat(40) + &"x".repeat(300);
    let width = 200.0;
    let lines = wrap(&text, Face::Regular, BODY, width, &mut replaced);
    assert!(lines.len() > 5);
    for line in &lines {
        assert!(
            text_width(line, Face::Regular, BODY) <= width + 0.01,
            "{}",
            String::from_utf8_lossy(line)
        );
    }
    let joined: Vec<u8> = lines.concat();
    assert!(
        String::from_utf8_lossy(&joined).matches('x').count() == 300,
        "no character is lost"
    );
    // Line breaks in the text are kept.
    assert_eq!(
        wrap(
            "one\ntwo\n\nfour",
            Face::Regular,
            BODY,
            400.0,
            &mut replaced
        )
        .len(),
        4
    );
}

#[test]
fn a_long_document_runs_onto_more_pages_and_a_table_repeats_its_header() {
    let mut blocks = Vec::new();
    for index in 0..60 {
        blocks.push(json!({ "type": "paragraph", "text": format!("Paragraph {index}. {}", "word ".repeat(80)) }));
    }
    let rows: Vec<Value> = (0..120)
        .map(|index| json!([format!("row {index}"), index]))
        .collect();
    blocks.push(json!({ "type": "table", "header": ["Label", "Value"], "rows": rows }));
    let (bytes, _) =
        write_pdf(&json!({ "blocks": blocks })).unwrap_or_else(|error| panic!("{error}"));
    let document = lopdf::Document::load_mem(&bytes).unwrap_or_else(|error| panic!("{error}"));
    let pages = document.get_pages().len();
    assert!(pages >= 4, "{pages}");
    let last = document
        .extract_text(&[u32::try_from(pages).unwrap_or(1)])
        .unwrap_or_default();
    assert!(last.contains(&format!("Page {pages} of {pages}")), "{last}");
    assert!(
        last.contains("Label"),
        "the header is drawn again on the last page: {last}"
    );
    assert!(last.contains("row 119"));
}

#[test]
fn content_of_the_wrong_shape_or_size_is_refused_in_words() {
    for (content, wanted) in [
        (json!({}), "needs blocks"),
        (json!({ "blocks": [{ "text": "x" }] }), "needs a type"),
        (json!({ "blocks": [{ "type": "video" }] }), "unknown type"),
        (json!({ "blocks": [{ "type": "paragraph" }] }), "needs text"),
        (
            json!({ "blocks": [{ "type": "bullets", "items": [1] }] }),
            "must be text",
        ),
        (
            json!({ "blocks": [{ "type": "table", "rows": [] }] }),
            "no rows",
        ),
    ] {
        let error = write_pdf(&content)
            .err()
            .unwrap_or_else(|| panic!("{content} must be refused"));
        assert!(error.to_string().contains(wanted), "{error}");
    }
    let big = json!({ "blocks": (0..80).map(|_| json!({ "type": "paragraph", "text": "word ".repeat(6000) })).collect::<Vec<_>>() });
    assert!(matches!(write_pdf(&big), Err(WriteError::TooLarge(_))));
}
