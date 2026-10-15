use super::*;
use serde_json::json;
use zip::write::SimpleFileOptions;

fn package(parts: &[(&str, &[u8])]) -> Vec<u8> {
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    writer
        .set_comment("preserve archive comment")
        .unwrap_or_else(|e| panic!("{e}"));
    for (name, bytes) in parts {
        writer
            .start_file(
                *name,
                SimpleFileOptions::default()
                    .compression_method(CompressionMethod::Deflated)
                    .unix_permissions(0o600),
            )
            .unwrap_or_else(|e| panic!("{e}"));
        writer.write_all(bytes).unwrap_or_else(|e| panic!("{e}"));
    }
    writer
        .finish()
        .unwrap_or_else(|e| panic!("{e}"))
        .into_inner()
}

const TYPES: &str = r#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/><Override PartName="/xl/worksheets/nonstandard.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/><Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/></Types>"#;
const ROOT_RELS: &str = r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="root" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/></Relationships>"#;
const BOOK: &str = r#"<?xml version="1.0"?><workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><bookViews><workbookView activeTab="1"/></bookViews><sheets><sheet name="Finance" sheetId="7" r:id="finance"/><sheet name="Keep" sheetId="1" r:id="keep"/></sheets><calcPr calcId="191029" calcMode="manual"/><extLst><ext uri="test"/></extLst></workbook>"#;
const BOOK_RELS: &str = r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="keep" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/><Relationship Id="finance" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/nonstandard.xml"/></Relationships>"#;
const SHEET: &str = r#"<?xml version="1.0"?><worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><dimension ref="A1:F4"/><sheetViews><sheetView workbookViewId="0"><pane ySplit="1" state="frozen"/></sheetView></sheetViews><cols><col min="1" max="2" width="24" customWidth="1"/></cols><sheetData>
<row r="1" spans="1:6" ht="25" customHeight="1"><c r="A1" s="2" t="s"><v>0</v></c><!--keep this comment--><c r="C1" s="1"><v>42</v></c><c r="F1" s="3"><f>SUM(C1:C4)</f><v>47</v></c></row>
<row r="3" s="4" customFormat="1"/>
<row r="4"><c r="C4"><v>5</v></c></row>
</sheetData><mergeCells count="1"><mergeCell ref="A8:C8"/></mergeCells><autoFilter ref="A1:F4"/><extLst><ext uri="unchanged"/></extLst></worksheet>"#;
const KEEP: &str = r#"<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetData><row r="1"><c r="A1" s="1"><f>Finance!C1*2</f><v>84</v></c></row></sheetData></worksheet>"#;

fn fixture(sheet: &str) -> Vec<u8> {
    package(&[
        ("[Content_Types].xml", TYPES.as_bytes()),
        ("_rels/.rels", ROOT_RELS.as_bytes()),
        ("xl/workbook.xml", BOOK.as_bytes()),
        ("xl/_rels/workbook.xml.rels", BOOK_RELS.as_bytes()),
        ("xl/worksheets/nonstandard.xml", sheet.as_bytes()),
        ("xl/worksheets/sheet1.xml", KEEP.as_bytes()),
        ("xl/sharedStrings.xml", br#"<sst xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" count="1" uniqueCount="1"><si><t>Original</t></si></sst>"#),
        ("xl/styles.xml", br#"<styleSheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><fonts count="1"><font><b/></font></fonts></styleSheet>"#),
        ("custom/opaque.bin", b"\0opaque\xff\x01\x02metadata"),
        ("xl/drawings/drawing1.xml", br#"<drawing xmlns="urn:untouched"><object id="chart-1"/></drawing>"#),
    ])
}

fn change(bytes: &[u8], name: &str, replacement: &[u8]) -> Vec<u8> {
    let mut parts = read_parts(bytes).unwrap_or_else(|e| panic!("{e}"));
    parts.insert(name.into(), replacement.into());
    package(
        &parts
            .iter()
            .map(|(name, value)| (name.as_str(), value.as_slice()))
            .collect::<Vec<_>>(),
    )
}

#[allow(clippy::needless_pass_by_value)]
fn request(edits: Value) -> Vec<CellEdit> {
    parse_edits(&edits).unwrap_or_else(|e| panic!("{e}"))
}

#[allow(clippy::needless_pass_by_value)]
fn single(cell: &str, value: Value) -> Vec<CellEdit> {
    request(json!([{ "sheet": "Finance", "cell": cell, "value": value }]))
}

fn text(bytes: &[u8], name: &str) -> String {
    String::from_utf8(read_parts(bytes).unwrap_or_else(|e| panic!("{e}"))[name].clone())
        .unwrap_or_else(|e| panic!("{e}"))
}

fn compressed(bytes: &[u8], name: &str) -> Vec<u8> {
    let mut archive = ZipArchive::new(Cursor::new(bytes)).unwrap_or_else(|e| panic!("{e}"));
    let mut file = archive.by_name(name).unwrap_or_else(|e| panic!("{e}"));
    let mut data = Vec::new();
    file.read_to_end(&mut data)
        .unwrap_or_else(|e| panic!("{e}"));
    let start = usize::try_from(
        file.data_start()
            .unwrap_or_else(|| panic!("no data offset")),
    )
    .unwrap_or_else(|e| panic!("{e}"));
    let length = usize::try_from(file.compressed_size()).unwrap_or_else(|e| panic!("{e}"));
    bytes[start..start + length].to_vec()
}

#[test]
fn surgical_edit_preserves_every_untouched_part_including_compressed_payloads() {
    let original = fixture(SHEET);
    let result = edit(&original, &single("A1", json!("Changed"))).unwrap_or_else(|e| panic!("{e}"));
    let before = read_parts(&original).unwrap_or_else(|e| panic!("{e}"));
    let after = read_parts(&result).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(before.len(), after.len());
    for (name, data) in before {
        if name == "xl/workbook.xml" || name == "xl/worksheets/nonstandard.xml" {
            continue;
        }
        assert_eq!(data, after[&name], "changed member {name}");
        assert_eq!(
            compressed(&original, &name),
            compressed(&result, &name),
            "recompressed {name}"
        );
    }
    let sheet = text(&result, "xl/worksheets/nonstandard.xml");
    assert!(sheet.contains(
        r#"<c r="A1" s="2" t="inlineStr"><is><t xml:space="preserve">Changed</t></is></c>"#
    ));
    assert!(sheet.contains(r#"<c r="F1" s="3"><f>SUM(C1:C4)</f><v>47</v></c>"#));
    assert!(sheet.contains("<!--keep this comment-->"));
    assert!(sheet.contains(r#"<cols><col min="1" max="2" width="24" customWidth="1"/></cols>"#));
    let book = text(&result, "xl/workbook.xml");
    assert!(book.contains("fullCalcOnLoad=\"1\"") && book.contains("forceFullCalc=\"1\""));
    assert!(
        book.contains(r#"calcMode="manual""#),
        "the owner's calculation mode is not changed"
    );
    assert_eq!(
        ZipArchive::new(Cursor::new(result))
            .unwrap_or_else(|e| panic!("{e}"))
            .comment(),
        b"preserve archive comment"
    );
}

#[test]
fn values_are_typed_strings_are_literal_and_null_keeps_style() {
    let edits = request(json!([
        {"sheet":"Finance","cell":"A1","value":null},
        {"sheet":"Finance","cell":"B1","value":" =SUM(A1) <&>\"\r\n café "},
        {"sheet":"Finance","cell":"C1","value":false},
        {"sheet":"Finance","cell":"D1","value":-12.5},
        {"sheet":"Finance","cell":"E1","value":true}
    ]));
    let result = edit(&fixture(SHEET), &edits).unwrap_or_else(|e| panic!("{e}"));
    let sheet = text(&result, "xl/worksheets/nonstandard.xml");
    let doc = parse_xml(sheet.as_bytes()).unwrap_or_else(|e| panic!("{e}"));
    let cells: BTreeMap<_, _> = doc
        .descendants()
        .filter(|n| n.has_tag_name((MAIN, "c")))
        .map(|n| (n.attribute("r").unwrap_or(""), n))
        .collect();
    assert_eq!(cells["A1"].attribute("s"), Some("2"));
    assert!(cells["A1"].children().all(|n| !n.is_element()));
    assert_eq!(cells["B1"].attribute("t"), Some("inlineStr"));
    assert_eq!(
        cells["B1"]
            .descendants()
            .find(|n| n.has_tag_name((MAIN, "t")))
            .and_then(|n| n.text()),
        Some(" =SUM(A1) <&>\"_x000D_\n café ")
    );
    assert_eq!(cells["C1"].attribute("t"), Some("b"));
    assert_eq!(
        cells["C1"].first_element_child().and_then(|n| n.text()),
        Some("0")
    );
    assert_eq!(cells["D1"].attribute("t"), None);
    assert_eq!(
        cells["D1"].first_element_child().and_then(|n| n.text()),
        Some("-12.5")
    );
    assert_eq!(
        cells["E1"].first_element_child().and_then(|n| n.text()),
        Some("1")
    );
}

#[test]
fn office_escape_sequences_are_stored_as_literal_text_not_encoded_controls() {
    let result = edit(
        &fixture(SHEET),
        &single("A1", json!("_x0041_ _x000D_ _x005F_\r")),
    )
    .unwrap_or_else(|e| panic!("{e}"));
    let sheet = text(&result, "xl/worksheets/nonstandard.xml");
    assert!(sheet.contains("_x005F_x0041_ _x005F_x000D_ _x005F_x005F__x000D_"));
}

#[test]
fn missing_rows_and_cells_are_inserted_in_coordinate_order() {
    let edits = request(json!([
        {"sheet":"Finance","cell":"Z9","value":9},
        {"sheet":"Finance","cell":"D2","value":2},
        {"sheet":"Finance","cell":"B2","value":"two"},
        {"sheet":"Finance","cell":"B3","value":"three"},
        {"sheet":"Finance","cell":"A1","value":"new"},
        {"sheet":"Finance","cell":"A8","value":"merged anchor"}
    ]));
    let result = edit(&fixture(SHEET), &edits).unwrap_or_else(|e| panic!("{e}"));
    let sheet = text(&result, "xl/worksheets/nonstandard.xml");
    let doc = parse_xml(sheet.as_bytes()).unwrap_or_else(|e| panic!("{e}"));
    let rows: Vec<_> = doc
        .descendants()
        .filter(|n| n.has_tag_name((MAIN, "row")))
        .filter_map(|n| n.attribute("r"))
        .collect();
    assert_eq!(rows, ["1", "2", "3", "4", "8", "9"]);
    assert!(sheet.contains(r#"<row r="2"><c r="B2""#));
    assert!(sheet.contains(r#"<row r="3" s="4" customFormat="1"><c r="B3""#));
    assert!(!sheet.contains("<dimension"));
    assert!(!sheet.contains(" spans="));
}

#[test]
fn empty_self_closing_sheet_data_and_prefixed_namespaces_work() {
    for data in [
        "<s:sheetData/>",
        "<s:sheetData></s:sheetData>",
        "<s:sheetData><s:row r='2'/></s:sheetData>",
    ] {
        let sheet = format!("<s:worksheet xmlns:s='{MAIN}'>{data}</s:worksheet>");
        let result =
            edit(&fixture(&sheet), &single("D2", json!("a > b"))).unwrap_or_else(|e| panic!("{e}"));
        let result = text(&result, "xl/worksheets/nonstandard.xml");
        let doc = parse_xml(result.as_bytes()).unwrap_or_else(|e| panic!("{e}"));
        assert!(
            doc.descendants()
                .any(|n| n.has_tag_name((MAIN, "c")) && n.attribute("r") == Some("D2")),
            "{result}"
        );
        assert!(result.contains("<s:t xml:space=\"preserve\">a &gt; b</s:t>"));
    }
}

#[test]
fn calc_settings_are_added_in_schema_order_when_absent() {
    let bytes = change(
        &fixture(SHEET),
        "xl/workbook.xml",
        BOOK.replace(r#"<calcPr calcId="191029" calcMode="manual"/>"#, "")
            .as_bytes(),
    );
    let result = edit(&bytes, &single("A1", json!(1))).unwrap_or_else(|e| panic!("{e}"));
    let book = text(&result, "xl/workbook.xml");
    assert!(book.find("<calcPr").unwrap_or(usize::MAX) < book.find("<extLst").unwrap_or(0));
}

#[test]
fn package_relationships_not_worksheet_numbering_select_the_sheet() {
    let mut edits = single("C1", json!(100));
    edits.push(CellEdit {
        sheet: "Keep".into(),
        address: "B1".into(),
        row: 1,
        column: 2,
        value: json!(9),
    });
    let result = edit(&fixture(SHEET), &edits).unwrap_or_else(|e| panic!("{e}"));
    assert!(text(&result, "xl/worksheets/nonstandard.xml").contains("<v>100</v>"));
    assert!(text(&result, "xl/worksheets/sheet1.xml").contains("<v>9</v>"));
}

#[test]
fn formula_cells_and_formula_result_ranges_are_refused() {
    assert!(edit(&fixture(SHEET), &single("F1", json!(1))).is_err());
    for formula in [
        r#"<f t="array" ref="C1:D2">SUM(A1:A2)</f>"#,
        r#"<f t="shared" ref="C1:D2" si="0">A1*2</f>"#,
        r#"<f t="dataTable" ref="C1:D2"/>"#,
    ] {
        let sheet = SHEET.replace("<v>42</v>", &format!("{formula}<v>42</v>"));
        assert!(
            edit(&fixture(&sheet), &single("D2", json!(1))).is_err(),
            "{formula}"
        );
    }
}

#[test]
fn protected_merged_and_metadata_cells_are_refused_without_touching_other_features() {
    assert!(edit(&fixture(SHEET), &single("B8", json!("not anchor"))).is_err());
    for sheet in [
        SHEET.replace("<sheetData>", "<sheetProtection sheet=\"1\"/><sheetData>"),
        SHEET.replace("s=\"2\"", "s=\"2\" cm=\"1\""),
        SHEET.replace("<v>0</v>", "<v>0</v><extLst/>"),
    ] {
        assert!(edit(&fixture(&sheet), &single("A1", json!(1))).is_err());
    }
}

#[test]
fn edit_request_bounds_coordinates_and_types_fail_closed() {
    for bad in [
        json!(null),
        json!([]),
        json!([{}]),
        json!([{"sheet":"Finance","cell":"A1","value":{},"extra":1}]),
        json!([{"sheet":"Finance","cell":"A1","value":[]}]),
        json!([{"sheet":"Finance","cell":"A1","value":"\u{0}"}]),
        json!([{"sheet":"Finance","cell":"A1","value":1},{"sheet":"Finance","cell":"A1","value":2}]),
        json!([{"sheet":"Finance","cell":"A1","value":"x".repeat(32_001)}]),
        json!([{"sheet":"Finance","cell":"A1","value":"😀".repeat(16_384)}]),
    ] {
        assert!(parse_edits(&bad).is_err(), "accepted bad edits");
    }
    for address in [
        "a1", "A0", "A01", "XFE1", "A1048577", "A1:B2", "$A$1", "AAAA1", "1", "A", "A-1",
    ] {
        assert!(
            parse_edits(&json!([{"sheet":"Finance","cell":address,"value":1}])).is_err(),
            "{address}"
        );
    }
    assert!(coordinate("XFD1048576").is_ok());
    assert!(
        parse_edits(&json!(
            (0..501)
                .map(|n| json!({"sheet":"Finance","cell":format!("A{}", n+1),"value":n}))
                .collect::<Vec<_>>()
        ))
        .is_err()
    );
    let large: Vec<_> = (1..=40)
        .map(|n| json!({"sheet":"Finance","cell":format!("A{n}"),"value":"x".repeat(32_000)}))
        .collect();
    assert!(parse_edits(&json!(large)).is_err());
}

#[test]
fn malformed_missing_wrong_type_and_unknown_sheet_are_refused() {
    for bytes in [
        Vec::new(),
        b"not a ZIP".to_vec(),
        fixture(SHEET)[..50].to_vec(),
        change(
            &fixture(SHEET),
            "xl/worksheets/nonstandard.xml",
            b"<worksheet>",
        ),
        change(
            &fixture(SHEET),
            "[Content_Types].xml",
            TYPES
                .replace(
                    "spreadsheetml.sheet.main+xml",
                    "wordprocessingml.document.main+xml",
                )
                .as_bytes(),
        ),
    ] {
        assert!(edit(&bytes, &single("A1", json!(1))).is_err());
    }
    assert!(
        edit(
            &fixture(SHEET),
            &request(json!([{"sheet":"Missing","cell":"A1","value":1}]))
        )
        .is_err()
    );
}

#[test]
fn xml_dtd_invalid_untouched_parts_and_depth_are_refused() {
    let dtd = br#"<!DOCTYPE root [<!ENTITY x "expanded">]><root>&x;</root>"#;
    for xml in [
        dtd.to_vec(),
        b"<!DOCTYPE root><root/>".to_vec(),
        b"<broken>".to_vec(),
        nested_xml(MAX_XML_DEPTH + 1).into_bytes(),
        nested_xml(130).into_bytes(),
        nested_xml(16_384).into_bytes(),
    ] {
        for name in ["custom/hostile.xml", "custom/hostile.rels"] {
            assert!(
                edit(
                    &change(&fixture(SHEET), name, &xml),
                    &single("A1", json!(1))
                )
                .is_err()
            );
        }
    }
}

fn nested_xml(depth: usize) -> String {
    format!("{}{}", "<a>".repeat(depth), "</a>".repeat(depth))
}

#[test]
fn xml_depth_boundary_includes_empty_elements_and_non_element_nodes() {
    for depth in [1, MAX_XML_DEPTH - 1, MAX_XML_DEPTH] {
        let xml = nested_xml(depth);
        assert!(parse_xml(xml.as_bytes()).is_ok(), "depth {depth}");
        let xml = format!(
            "{}<a/>{}",
            "<a>".repeat(depth - 1),
            "</a>".repeat(depth - 1)
        );
        assert!(parse_xml(xml.as_bytes()).is_ok(), "empty at depth {depth}");
    }
    let xml = format!(
        "{}<a/>{}",
        "<a>".repeat(MAX_XML_DEPTH),
        "</a>".repeat(MAX_XML_DEPTH)
    );
    assert_eq!(
        parse_xml(xml.as_bytes()).err().as_deref(),
        Some("workbook XML is nested too deeply")
    );
    for leaf in ["text", "<!--comment-->", "<![CDATA[text]]>", "<?pi value?>"] {
        for depth in [MAX_XML_DEPTH - 1, MAX_XML_DEPTH] {
            let xml = format!("{}{leaf}{}", "<a>".repeat(depth), "</a>".repeat(depth));
            assert_eq!(parse_xml(xml.as_bytes()).is_ok(), depth < MAX_XML_DEPTH);
        }
    }
}

#[test]
fn xml_depth_preflight_ignores_lexical_decoys_but_keeps_parser_validation() {
    let decoy = format!(
        "{}{}<!DOCTYPE ignored>",
        "<a>".repeat(256),
        "</a>".repeat(256)
    );
    let xml = format!(
        "<?xml version='1.0'?><?pi {decoy}?><!--{decoy}--><root a='>/>&lt;/a>' b=\">/&gt;&quot;\"><![CDATA[{decoy}]]>{}</root>",
        nested_xml(MAX_XML_DEPTH - 1)
    );
    assert!(parse_xml(xml.as_bytes()).is_ok());
    for xml in [
        "<a></b>",
        "<a><b/></a></b>",
        "<a><b></a>",
        "<a attr='unterminated>",
        "<a><!--unterminated</a>",
        "<a><![CDATA[unterminated</a>",
        "<a><?pi unterminated</a>",
        "<a>&unknown;</a>",
        "<a attr='<b>'/>",
        "<a/><a/>",
        "<!DOCTYPE a><a/>",
        "<!DOCTYPE a [<!ENTITY x '<b/>'>]><a>&x;</a>",
    ] {
        assert!(parse_xml(xml.as_bytes()).is_err(), "{xml}");
    }
    assert!(parse_xml(&[0xff]).is_err());
}

#[test]
fn xml_depth_is_refused_before_recursive_parser_on_bounded_stack() {
    const CHILD_ENV: &str = "JARVIS_XLSX_XML_DEPTH_TEST_CHILD";
    if std::env::var_os(CHILD_ENV).is_some() {
        std::thread::Builder::new()
            .name("xlsx-depth-small-stack".into())
            .stack_size(256 * 1024)
            .spawn(|| {
                xml_depth_boundary_includes_empty_elements_and_non_element_nodes();
                xml_depth_preflight_ignores_lexical_decoys_but_keeps_parser_validation();
                for depth in [MAX_XML_DEPTH + 1, 130, 16_384] {
                    let xml = nested_xml(depth);
                    assert_eq!(
                        parse_xml(xml.as_bytes()).err().as_deref(),
                        Some("workbook XML is nested too deeply"),
                        "depth {depth}"
                    );
                }
            })
            .unwrap_or_else(|e| panic!("spawn bounded-stack guard: {e}"))
            .join()
            .unwrap_or_else(|e| panic!("bounded-stack guard panicked: {e:?}"));
        return;
    }
    // libtest names omit the crate prefix included in module_path!().
    let qualified_name = concat!(
        module_path!(),
        "::xml_depth_is_refused_before_recursive_parser_on_bounded_stack"
    );
    let (_, test_name) = qualified_name
        .split_once("::")
        .unwrap_or_else(|| panic!("missing test module"));
    // A parser overflow aborts the child, not the rest of the daemon's test suite.
    let output = std::process::Command::new(
        std::env::current_exe().unwrap_or_else(|e| panic!("test executable: {e}")),
    )
    .args(["--exact", test_name, "--nocapture"])
    .env(CHILD_ENV, "1")
    .output()
    .unwrap_or_else(|e| panic!("run bounded-stack child: {e}"));
    assert!(
        output.status.success(),
        "bounded-stack child failed ({}):\n{}\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("1 passed"));
}

#[test]
fn path_traversal_external_sheet_and_ambiguous_relationships_fail_closed() {
    for target in [
        "../../outside.xml",
        "C:/outside.xlsx",
        "worksheets\\nonstandard.xml",
        "worksheets/%2e%2e/outside.xml",
    ] {
        let rels = BOOK_RELS.replace("worksheets/nonstandard.xml", target);
        assert!(
            edit(
                &change(
                    &fixture(SHEET),
                    "xl/_rels/workbook.xml.rels",
                    rels.as_bytes()
                ),
                &single("A1", json!(1))
            )
            .is_err(),
            "{target}"
        );
    }
    let rels = BOOK_RELS.replace("Id=\"finance\"", "Id=\"finance\" TargetMode=\"External\"");
    assert!(
        edit(
            &change(
                &fixture(SHEET),
                "xl/_rels/workbook.xml.rels",
                rels.as_bytes()
            ),
            &single("A1", json!(1))
        )
        .is_err()
    );
    let rels = BOOK_RELS.replace("Id=\"finance\"", "Id=\"keep\"");
    assert!(
        edit(
            &change(
                &fixture(SHEET),
                "xl/_rels/workbook.xml.rels",
                rels.as_bytes()
            ),
            &single("A1", json!(1))
        )
        .is_err()
    );
    for name in [
        "../outside.bin",
        "C:/outside.bin",
        "custom\\outside.bin",
        "/absolute.bin",
        "xl/./extra.bin",
    ] {
        let bytes = change(&fixture(SHEET), name, b"unsafe");
        assert!(edit(&bytes, &single("A1", json!(1))).is_err(), "{name}");
    }
}

#[test]
fn duplicate_case_alias_names_signatures_and_macros_are_refused() {
    for (name, bytes) in [
        ("XL/WORKBOOK.XML", BOOK.as_bytes()),
        ("_xmlsignatures/sig1.xml", b"<signature/>".as_slice()),
        ("xl/vbaProject.bin", b"macro".as_slice()),
    ] {
        assert!(
            edit(
                &change(&fixture(SHEET), name, bytes),
                &single("A1", json!(1))
            )
            .is_err()
        );
    }
    let types = TYPES.replace(
        "spreadsheetml.sheet.main+xml",
        "ms-excel.sheet.macroEnabled.main+xml",
    );
    assert!(
        edit(
            &change(&fixture(SHEET), "[Content_Types].xml", types.as_bytes()),
            &single("A1", json!(1))
        )
        .is_err()
    );
}

#[test]
fn excessive_archive_entry_and_decompression_bounds_are_refused() {
    let huge = vec![b'0'; 24 * 1024 * 1024 + 1];
    assert!(
        edit(
            &change(&fixture(SHEET), "custom/large.bin", &huge),
            &single("A1", json!(1))
        )
        .is_err()
    );
    let chunk = vec![b'0'; 17 * 1024 * 1024];
    let bytes = package(&[("a.bin", &chunk), ("b.bin", &chunk), ("c.bin", &chunk)]);
    assert!(edit(&bytes, &single("A1", json!(1))).is_err());
    let names: Vec<_> = (0..4001).map(|n| format!("{n}.bin")).collect();
    let bytes = package(
        &names
            .iter()
            .map(|name| (name.as_str(), b"".as_slice()))
            .collect::<Vec<_>>(),
    );
    assert!(edit(&bytes, &single("A1", json!(1))).is_err());
    assert!(edit(&vec![0; MAX_BYTES + 1], &single("A1", json!(1))).is_err());
}

#[test]
fn unordered_duplicate_cells_rows_and_bad_namespace_are_refused() {
    for sheet in [
        SHEET.replace("r=\"C1\"", "r=\"A1\""),
        SHEET.replace("r=\"3\"", "r=\"1\""),
        SHEET.replace("r=\"C4\"", "r=\"C5\""),
        SHEET.replace(MAIN, "urn:wrong"),
    ] {
        assert!(edit(&fixture(&sheet), &single("A1", json!(1))).is_err());
    }
}

#[test]
fn corrupt_untouched_binary_payload_crc_is_validated_before_raw_copying() {
    let mut bytes = fixture(SHEET);
    let mut archive = ZipArchive::new(Cursor::new(&bytes)).unwrap_or_else(|e| panic!("{e}"));
    let file = archive
        .by_name("custom/opaque.bin")
        .unwrap_or_else(|e| panic!("{e}"));
    let offset = usize::try_from(file.data_start().unwrap_or_else(|| panic!("offset")))
        .unwrap_or_else(|e| panic!("{e}"));
    drop(file);
    drop(archive);
    bytes[offset] ^= 0xff;
    assert!(edit(&bytes, &single("A1", json!(1))).is_err());
}

#[test]
fn local_header_name_mismatch_is_refused_before_replacement() {
    let mut bytes = fixture(SHEET);
    bytes[30] = b'!';
    assert!(edit(&bytes, &single("A1", json!(1))).is_err());
}

#[test]
fn duplicate_and_overlapping_central_directory_records_are_refused() {
    let mut bytes = fixture(SHEET);
    let mut archive = ZipArchive::new(Cursor::new(&bytes)).unwrap_or_else(|e| panic!("{e}"));
    let first = archive.by_index(0).unwrap_or_else(|e| panic!("{e}"));
    let offset = first.header_start();
    drop(first);
    let second = archive.by_index(1).unwrap_or_else(|e| panic!("{e}"));
    let central = usize::try_from(second.central_header_start()).unwrap_or_else(|e| panic!("{e}"));
    drop(second);
    drop(archive);
    bytes[central + 42..central + 46].copy_from_slice(
        &u32::try_from(offset)
            .unwrap_or_else(|e| panic!("{e}"))
            .to_le_bytes(),
    );
    assert!(edit(&bytes, &single("A1", json!(1))).is_err());

    let mut bytes = package(&[("a.bin", b"same"), ("b.bin", b"same")]);
    for index in 0..bytes.len() - 5 {
        if &bytes[index..index + 5] == b"b.bin" {
            bytes[index] = b'a';
        }
    }
    assert!(read_parts(&bytes).is_err());
}
