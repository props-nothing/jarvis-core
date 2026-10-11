//! The document-writing tool: it creates new files in a granted folder, never replaces one, and refuses what it must.

use super::*;

struct Folder(std::path::PathBuf);

impl Drop for Folder {
    fn drop(&mut self) {
        jarvis_core::remove_scratch_dir(&self.0);
    }
}

fn tool_over_a_folder() -> (WriteDocumentTool, Folder) {
    let folder = std::env::temp_dir().join(format!("jwd-{}", jarvis_core::scratch_tag()));
    std::fs::create_dir_all(&folder).unwrap_or_else(|error| panic!("{error}"));
    let roots = WorkspaceRoots::new([&folder]).unwrap_or_else(|error| panic!("{error}"));
    (WriteDocumentTool::new(Arc::new(roots)), Folder(folder))
}

fn sheet() -> Value {
    json!({ "sheets": [{ "name": "Prospects", "header": ["Bedrijf", "Plaats"], "rows": [["Café", "Utrecht"], ["Beta", null]] }] })
}

#[test]
fn the_tool_creates_files_like_the_plain_write_and_needs_no_approval() {
    let definition = WriteDocumentTool::definition().unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(definition.approval(), ApprovalPolicy::Auto);
    assert!(definition.effects().contains(ToolEffect::Write));
    assert_eq!(definition.risk().level(), 1);
    assert!(INPUT.contains(r#""additionalProperties": false"#));
}

/// **A workbook, a CSV and a document are written inside the folder and read back by the reader.**
#[tokio::test]
async fn each_format_is_written_and_reads_back() {
    let (tool, folder) = tool_over_a_folder();
    let made = tool
        .write(&json!({ "path": "out/prospects.xlsx", "content": sheet() }))
        .unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(made["format"], "xlsx");
    assert_eq!(made["created"], true);
    let bytes = std::fs::read(folder.0.join("out").join("prospects.xlsx"))
        .unwrap_or_else(|error| panic!("{error}"));
    let read = jarvis_documents::extract("prospects.xlsx", &bytes)
        .unwrap_or_else(|error| panic!("{error}"));
    assert!(read.text.contains("Café") && read.text.contains("Utrecht"));

    tool.write(&json!({ "path": "out/p.csv", "delimiter": ";", "content": sheet() }))
        .unwrap_or_else(|error| panic!("{error}"));
    let csv = std::fs::read_to_string(folder.0.join("out").join("p.csv")).unwrap_or_default();
    assert!(
        csv.contains("Bedrijf;Plaats\r\nCafé;Utrecht\r\n"),
        "{csv:?}"
    );

    tool.write(&json!({ "path": "report.DOCX", "content": { "title": "T", "blocks": [{ "type": "paragraph", "text": "Hallo" }] } }))
        .unwrap_or_else(|error| panic!("{error}"));
    let docx =
        std::fs::read(folder.0.join("report.DOCX")).unwrap_or_else(|error| panic!("{error}"));
    assert!(
        jarvis_documents::extract("report.docx", &docx)
            .is_ok_and(|read| read.text.contains("Hallo"))
    );
}

/// **It never replaces a file, and says what to do instead.**
#[tokio::test]
async fn an_existing_file_is_never_replaced() {
    let (tool, folder) = tool_over_a_folder();
    std::fs::write(folder.0.join("keep.xlsx"), b"precious")
        .unwrap_or_else(|error| panic!("{error}"));
    let error = tool
        .write(&json!({ "path": "keep.xlsx", "content": sheet() }))
        .err()
        .unwrap_or_else(|| panic!("a file was replaced"));
    assert!(error.to_string().contains("already exists"), "{error}");
    assert_eq!(
        std::fs::read(folder.0.join("keep.xlsx")).unwrap_or_default(),
        b"precious"
    );
    // The same name twice through the tool: the second is refused too.
    tool.write(&json!({ "path": "new.xlsx", "content": sheet() }))
        .unwrap_or_else(|error| panic!("{error}"));
    assert!(
        tool.write(&json!({ "path": "new.xlsx", "content": sheet() }))
            .is_err()
    );
}

/// **Paths that leave the folder, unknown types and bad content are refused and write nothing.**
#[tokio::test]
async fn bad_requests_write_nothing() {
    let (tool, folder) = tool_over_a_folder();
    for arguments in [
        json!({ "path": "../escape.xlsx", "content": sheet() }),
        json!({ "path": "C:/escape.xlsx", "content": sheet() }),
        json!({ "path": "/etc/escape.xlsx", "content": sheet() }),
        json!({ "path": "report.pdf", "content": sheet() }),
        json!({ "path": "noextension", "content": sheet() }),
        json!({ "path": "a.xlsx" }),
        json!({ "path": "a.xlsx", "content": "text" }),
        json!({ "path": "a.xlsx", "content": {} }),
        json!({ "path": "a.docx", "content": { "blocks": [{ "type": "script" }] } }),
        json!({ "path": "a.csv", "delimiter": "\t", "content": sheet() }),
    ] {
        let result = tool.write(&arguments);
        assert!(
            matches!(result, Err(AdapterError::RefusedBeforeReaching { .. })),
            "{arguments}: {result:?}"
        );
    }
    let left: Vec<_> = std::fs::read_dir(&folder.0)
        .unwrap_or_else(|error| panic!("{error}"))
        .collect();
    assert!(
        left.is_empty(),
        "a refused request left files behind: {left:?}"
    );
}

/// **A PDF is written, reads back, and says when characters could not be shown.**
#[tokio::test]
async fn a_pdf_is_written_and_reports_what_it_could_not_draw() {
    let (tool, folder) = tool_over_a_folder();
    let made = tool
        .write(&json!({ "path": "rapport.pdf", "content": { "title": "Rapport", "blocks": [{ "type": "paragraph", "text": "Café in Utrecht kost €5" }] } }))
        .unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(made["format"], "pdf");
    assert!(made.get("replaced_characters").is_none());
    let bytes =
        std::fs::read(folder.0.join("rapport.pdf")).unwrap_or_else(|error| panic!("{error}"));
    let read =
        jarvis_documents::extract("rapport.pdf", &bytes).unwrap_or_else(|error| panic!("{error}"));
    assert!(read.text.contains("Café in Utrecht kost €5") && read.text.contains("Rapport"));

    let odd = tool
        .write(&json!({ "path": "odd.pdf", "content": { "blocks": [{ "type": "paragraph", "text": "ok \u{4F60}" }] } }))
        .unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(odd["replaced_characters"], 1);
    assert!(
        odd["note"]
            .as_str()
            .unwrap_or_default()
            .contains("Western European")
    );
    assert!(
        tool.write(&json!({ "path": "rapport.pdf", "content": { "blocks": [] } }))
            .is_err(),
        "never replaces"
    );
}
