//! Document creation remains create-only; spreadsheet editing is a separate policy-controlled replacement.

use super::*;

struct Folder(std::path::PathBuf);

impl Drop for Folder {
    fn drop(&mut self) {
        jarvis_core::remove_scratch_dir(&self.0);
    }
}

fn tool_over_a_folder() -> (WriteDocumentTool, Folder) {
    let folder = std::env::current_dir()
        .unwrap_or_else(|error| panic!("{error}"))
        .join("target")
        .join(format!("jwd-{}", jarvis_core::scratch_tag()));
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
        json!({ "path": "C:escape.xlsx", "content": sheet() }),
        json!({ "path": r"C:\escape.xlsx", "content": sheet() }),
        json!({ "path": r"\\server\share\escape.xlsx", "content": sheet() }),
        json!({ "path": r"\escape.xlsx", "content": sheet() }),
        json!({ "path": r"out\..\escape.xlsx", "content": sheet() }),
        json!({ "path": "out/../escape.xlsx", "content": sheet() }),
        json!({ "path": "out.xlsx:stream", "content": sheet() }),
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

#[test]
fn spreadsheet_editing_is_a_scoped_moderate_risk_policy_write() {
    let definition = EditSpreadsheetTool::definition().unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(definition.approval(), ApprovalPolicy::Policy);
    assert!(definition.effects().contains(ToolEffect::Write));
    assert_eq!(definition.risk().level(), 2);
    assert!(
        definition
            .required_scopes()
            .iter()
            .any(|scope| scope.to_string() == "files.write")
    );
    let schema = ToolSchema::parse(EDIT_INPUT).unwrap_or_else(|error| panic!("{error}"));
    for (arguments, valid) in [
        (
            json!({"path":"book.xlsx","edits":[{"sheet":"Prospects","cell":"A1","value":"=text"}]}),
            true,
        ),
        (json!({"path":"book.xlsx","edits":[]}), false),
        (
            json!({"path":"book.xlsx","edits":[{"sheet":"Prospects","cell":"a1","value":1}]}),
            false,
        ),
        (
            json!({"path":"book.xlsx","edits":[{"sheet":"Prospects","cell":"A1","value":1,"formula":true}]}),
            false,
        ),
        (
            json!({"path":"book.xlsx","edits":[{"sheet":"Prospects","cell":"A1","value":1}],"overwrite":true}),
            false,
        ),
    ] {
        assert_eq!(
            schema
                .validate(&arguments)
                .unwrap_or_else(|error| panic!("{error}"))
                .is_valid(),
            valid
        );
    }
}

#[test]
fn an_existing_workbook_is_edited_in_place_and_reads_back_independently() {
    let (writer, folder) = tool_over_a_folder();
    writer
        .write(&json!({"path":"existing.xlsx","content":sheet()}))
        .unwrap_or_else(|error| panic!("{error}"));
    let tool = EditSpreadsheetTool::new(Arc::clone(&writer.roots));
    let result = tool
        .edit(&json!({"path":"existing.xlsx","edits":[
            {"sheet":"Prospects","cell":"A2","value":"Changed café"},
            {"sheet":"Prospects","cell":"B3","value":123},
            {"sheet":"Prospects","cell":"C6","value":"=not_a_formula"}
        ]}))
        .unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(result["created"], false);
    assert_eq!(result["edited_cells"], 3);
    assert!(
        result["note"]
            .as_str()
            .unwrap_or_default()
            .contains("stale")
    );
    let bytes =
        std::fs::read(folder.0.join("existing.xlsx")).unwrap_or_else(|error| panic!("{error}"));
    let read = jarvis_documents::extract("existing.xlsx", &bytes)
        .unwrap_or_else(|error| panic!("{error}"));
    assert!(
        read.text.contains("Changed café")
            && read.text.contains("123")
            && read.text.contains("=not_a_formula")
    );
    assert!(read.text.contains("Bedrijf") && read.text.contains("Utrecht"));
    assert_eq!(
        std::fs::read_dir(&folder.0)
            .unwrap_or_else(|error| panic!("{error}"))
            .count(),
        1
    );
}

#[test]
fn any_invalid_edit_preserves_the_original_bytes_and_creates_no_staging_files() {
    let (writer, folder) = tool_over_a_folder();
    writer
        .write(&json!({"path":"existing.xlsx","content":sheet()}))
        .unwrap_or_else(|error| panic!("{error}"));
    let path = folder.0.join("existing.xlsx");
    let original = std::fs::read(&path).unwrap_or_else(|error| panic!("{error}"));
    let tool = EditSpreadsheetTool::new(Arc::clone(&writer.roots));
    for arguments in [
        json!({"path":"existing.xlsx","edits":[{"sheet":"Prospects","cell":"A2","value":"must not persist"},{"sheet":"Missing","cell":"A1","value":1}]}),
        json!({"path":"existing.xlsx","edits":[{"sheet":"Prospects","cell":"XFE1","value":1}]}),
        json!({"path":"existing.xlsx","edits":[{"sheet":"Prospects","cell":"A2","value":1},{"sheet":"Prospects","cell":"A2","value":2}]}),
        json!({"path":"existing.xlsx","edits":[{"sheet":"Prospects","cell":"A2","value":{}}]}),
        json!({"path":"existing.xlsx","edits":[]}),
        json!({"path":"missing.xlsx","edits":[{"sheet":"Prospects","cell":"A2","value":1}]}),
        json!({"path":"../existing.xlsx","edits":[{"sheet":"Prospects","cell":"A2","value":1}]}),
        json!({"path":"C:\\existing.xlsx","edits":[{"sheet":"Prospects","cell":"A2","value":1}]}),
        json!({"path":"\\\\localhost\\share\\existing.xlsx","edits":[{"sheet":"Prospects","cell":"A2","value":1}]}),
        json!({"path":"existing.xlsx:stream","edits":[{"sheet":"Prospects","cell":"A2","value":1}]}),
        json!({"path":"existing.xlsm","edits":[{"sheet":"Prospects","cell":"A2","value":1}]}),
        json!({"path":"existing.xlsx","edits":[{"sheet":"Prospects","cell":"A2","value":1}],"extra":true}),
    ] {
        assert!(
            matches!(
                tool.edit(&arguments),
                Err(AdapterError::RefusedBeforeReaching { .. })
            ),
            "{arguments}"
        );
        assert_eq!(std::fs::read(&path).unwrap_or_default(), original);
        assert_eq!(
            std::fs::read_dir(&folder.0)
                .unwrap_or_else(|error| panic!("{error}"))
                .count(),
            1
        );
    }
}

#[test]
fn invalid_workbook_is_never_replaced_or_repaired() {
    let (writer, folder) = tool_over_a_folder();
    let original = b"not an Excel workbook";
    std::fs::write(folder.0.join("broken.xlsx"), original)
        .unwrap_or_else(|error| panic!("{error}"));
    let tool = EditSpreadsheetTool::new(Arc::clone(&writer.roots));
    assert!(
        tool.edit(
            &json!({"path":"broken.xlsx","edits":[{"sheet":"Sheet1","cell":"A1","value":1}]})
        )
        .is_err()
    );
    assert_eq!(
        std::fs::read(folder.0.join("broken.xlsx")).unwrap_or_default(),
        original
    );
    assert_eq!(
        std::fs::read_dir(&folder.0)
            .unwrap_or_else(|error| panic!("{error}"))
            .count(),
        1
    );
}
