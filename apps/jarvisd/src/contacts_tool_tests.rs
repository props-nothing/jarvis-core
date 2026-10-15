//! Contact tools preserve suppression on save/import; authorized deletion removes the whole contact.

use super::*;
use jarvis_core::{CorrelationId, RunId};
use jarvis_storage::{
    API_SESSION_CHANNEL, CallBinding, CallOrigin, CallTarget, LOCAL_USER_ID, LOCAL_WORKSPACE_ID,
    LinkKind, NewProject, NewToolCall, SessionTarget, StartRunInput, admit_tool_call, start_run,
};

struct Scratch(std::path::PathBuf);

impl Drop for Scratch {
    fn drop(&mut self) {
        jarvis_core::remove_scratch_dir(&self.0);
    }
}

fn must<T, E: std::fmt::Debug>(result: Result<T, E>) -> T {
    match result {
        Ok(value) => value,
        Err(error) => panic!("expected success, got {error:?}"),
    }
}

fn now() -> UtcTimestamp {
    UtcTimestamp::now(&SystemClock)
}

async fn database() -> (Scratch, Arc<SqliteDatabase>) {
    let tag = jarvis_core::scratch_tag();
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("target")
        .join(format!("jct-{}", &tag[tag.len() - 12..]));
    must(std::fs::create_dir_all(&path));
    let database = must(SqliteDatabase::open(&path.join("jarvis.sqlite3")).await);
    (Scratch(path), Arc::new(database))
}

/// A run with one admitted tool call, returning `(session_id, call_id)`.
async fn run_with_call(database: &SqliteDatabase) -> (String, String) {
    let run_id = RunId::new().to_string();
    let input = must(StartRunInput::continuing(
        SessionTarget::New,
        jarvis_core::SessionId::new().to_string(),
        run_id.clone(),
        jarvis_core::RequestId::new().to_string(),
        LOCAL_WORKSPACE_ID,
        LOCAL_USER_ID,
        "find leads",
        API_SESSION_CHANNEL,
        CorrelationId::new(),
        now(),
    ));
    let started = must(start_run(database, &input).await);
    let call_id = "0198f000-0000-7000-8000-0000000000e4".to_owned();
    must(
        admit_tool_call(
            database,
            &must(NewToolCall::new(
                call_id.clone(),
                CallOrigin::new(LOCAL_WORKSPACE_ID, &run_id, None),
                CallTarget::new(SAVE_TOOL, "1.0.0"),
                CallBinding::new("a".repeat(64), "b".repeat(32), &call_id, "policy-1", None),
                CorrelationId::new(),
                now(),
            )),
        )
        .await,
    );
    (started.session_id().to_owned(), call_id)
}

#[test]
fn destructive_and_bulk_contact_tools_ask_and_import_needs_both_scopes() {
    let definitions = must(ContactsTool::definitions());
    assert_eq!(definitions.len(), 7);
    for definition in &definitions {
        let expected = if [MERGE_TOOL, DELETE_TOOL, IMPORT_TOOL]
            .contains(&definition.id().to_string().as_str())
        {
            ApprovalPolicy::Ask
        } else {
            ApprovalPolicy::Auto
        };
        assert_eq!(definition.approval(), expected, "{}", definition.id());
    }
    let save = definitions
        .iter()
        .find(|definition| definition.id().to_string() == SAVE_TOOL)
        .unwrap_or_else(|| panic!("the save tool is defined"));
    assert!(save.effects().contains(ToolEffect::Write));
    assert!(SAVE_INPUT.contains(r#""additionalProperties": false"#));
    let delete = definitions
        .iter()
        .find(|definition| definition.id().to_string() == DELETE_TOOL)
        .unwrap_or_else(|| panic!("delete is defined"));
    assert!(delete.effects().contains(ToolEffect::Destructive));
    assert_eq!(delete.version(), "1.1.0");
    assert!(
        delete
            .description()
            .contains("Contacts marked do_not_contact can also be deleted")
    );
    assert!(DELETE_INPUT.contains("including any do_not_contact restriction"));
    assert_eq!(delete.risk().level(), 3);
    assert_eq!(
        delete.required_scopes(),
        &ScopeSet::single(must(Scope::new(WRITE_SCOPE)))
    );
    let import = definitions
        .iter()
        .find(|definition| definition.id().to_string() == IMPORT_TOOL)
        .unwrap_or_else(|| panic!("import is defined"));
    assert!(import.effects().contains(ToolEffect::Write));
    assert!(!import.effects().contains(ToolEffect::Destructive));
    assert_eq!(import.risk().level(), 2);
    assert_eq!(import.required_scopes().len(), 2);
    assert!(
        import
            .required_scopes()
            .contains(&must(Scope::new(WRITE_SCOPE)))
    );
    assert!(
        import
            .required_scopes()
            .contains(&must(Scope::new("files.read")))
    );
    for (input, valid, invalid) in [
        (
            IMPORT_INPUT,
            json!({ "path": "contacts.csv" }),
            json!({ "path": "contacts.csv", "csv": "unbounded" }),
        ),
        (
            DELETE_INPUT,
            json!({ "id": "contact-id" }),
            json!({ "id": "contact-id", "owner": true }),
        ),
    ] {
        let schema = must(ToolSchema::parse(input));
        assert!(must(schema.validate(&valid)).is_valid());
        assert!(!must(schema.validate(&invalid)).is_valid());
        assert!(!must(schema.validate(&json!({}))).is_valid());
    }
}

#[tokio::test]
async fn a_lead_is_saved_updated_found_and_counted() {
    let (_scratch, database) = database().await;
    let tool = ContactsTool::new(Arc::clone(&database));
    let (_session, call) = run_with_call(&database).await;
    let made = must(
        tool.save(
            &call,
            &json!({ "company": "Acme BV", "person": "Eva", "email": "eva@acme.nl", "source": "kvk.nl" }),
            now(),
        )
        .await,
    );
    assert_eq!(made["saved"], "created");
    let again = must(
        tool.save(
            &call,
            &json!({ "email": "eva@acme.nl", "status": "contacted" }),
            now(),
        )
        .await,
    );
    assert_eq!(
        (again["saved"].as_str(), again["contact"]["status"].as_str()),
        (Some("updated"), Some("contacted"))
    );

    let found = must(
        tool.search(&json!({ "query": "acme", "status": "contacted" }))
            .await,
    );
    assert_eq!(found["count"], 1);
    let source = found["contacts"][0]["source"].as_str().unwrap_or_default();
    assert!(
        source.contains("kvk.nl") && source != "kvk.nl",
        "free text is fenced as data: {source}"
    );
    assert_eq!(
        must(tool.search(&json!({ "status": "won" })).await)["count"],
        0
    );

    let stats = must(tool.stats().await);
    assert_eq!(
        (&stats["total"], &stats["by_status"]["contacted"]),
        (&json!(1), &json!(1))
    );
    assert_eq!(stats["by_status"]["won"], 0);
}

#[tokio::test]
async fn saving_cannot_lift_do_not_contact_and_bad_input_is_refused() {
    let (_scratch, database) = database().await;
    let tool = ContactsTool::new(Arc::clone(&database));
    let call = "no-such-call";
    must(
        tool.save(
            call,
            &json!({ "company": "Acme", "email": "eva@acme.nl", "status": "do_not_contact" }),
            now(),
        )
        .await,
    );
    let refused = tool
        .save(
            call,
            &json!({ "email": "eva@acme.nl", "status": "new" }),
            now(),
        )
        .await;
    assert!(
        matches!(&refused, Err(AdapterError::RefusedBeforeReaching { reason }) if reason.contains("only the owner")),
        "{refused:?}"
    );
    for bad in [
        json!({ "person": "nobody" }),
        json!({ "company": "Acme", "email": "not-an-address" }),
        json!({ "company": "Acme", "status": "maybe" }),
    ] {
        assert!(tool.save(call, &bad, now()).await.is_err(), "{bad}");
    }
}

#[tokio::test]
async fn a_contact_found_inside_a_project_is_filed_under_it() {
    let (_scratch, database) = database().await;
    let project = must(
        jarvis_storage::create_project(
            &database,
            LOCAL_WORKSPACE_ID,
            &NewProject {
                name: "Prospecting".to_owned(),
                ..NewProject::default()
            },
            now(),
        )
        .await,
    );
    let (session, call) = run_with_call(&database).await;
    must(jarvis_storage::link_project(&database, LinkKind::Session, &session, &project.id).await);
    let tool = ContactsTool::new(Arc::clone(&database));
    must(tool.save(&call, &json!({ "company": "Acme" }), now()).await);
    let filed = must(
        jarvis_storage::search_contacts(&database, LOCAL_WORKSPACE_ID, Some("acme"), None, 5).await,
    );
    let filed = filed.first().and_then(|contact| contact.project_id.clone());
    assert_eq!(filed.as_deref(), Some(project.id.as_str()));
}

/// **Duplicates are shown with what a merge would make of them, and the merge itself folds one into the other and returns what it removed.**
#[tokio::test]
async fn duplicates_are_previewed_and_merged() {
    let (_scratch, database) = database().await;
    let tool = ContactsTool::new(Arc::clone(&database));
    let (_session, call) = run_with_call(&database).await;
    let one = must(
        tool.save(
            &call,
            &json!({ "company": "Acme B.V.", "person": "Eva Bakker", "email": "eva@acme.nl", "role": "Owner" }),
            now(),
        )
        .await,
    );
    let two = must(
        tool.save(
            &call,
            &json!({ "company": "Acme", "person": "eva bakker", "phone": "06 1234 5678", "notes": "Called once." }),
            now(),
        )
        .await,
    );
    assert_ne!(one["contact"]["id"], two["contact"]["id"]);

    let listed = must(tool.duplicates(&json!({})).await);
    assert_eq!(listed["count"], 1);
    let pair = &listed["pairs"][0];
    assert_eq!(pair["keep"]["email"], "eva@acme.nl");
    assert_eq!(pair["would_become"]["phone"], "06 1234 5678");
    // Nothing was changed by looking.
    assert_eq!(must(tool.stats().await)["total"], 2);

    let done = must(
        tool.merge(
            &json!({ "keep_id": pair["keep"]["id"], "remove_id": pair["remove"]["id"] }),
            now(),
        )
        .await,
    );
    assert_eq!(done["merged"]["phone"], "06 1234 5678");
    assert_eq!(done["removed"]["person"], "eva bakker");
    assert_eq!(must(tool.stats().await)["total"], 1);
    assert_eq!(must(tool.duplicates(&json!({})).await)["count"], 0);

    for bad in [
        json!({ "keep_id": "x", "remove_id": "x" }),
        json!({ "keep_id": "no", "remove_id": "such" }),
        json!({ "keep_id": "" }),
    ] {
        assert!(tool.merge(&bad, now()).await.is_err(), "{bad}");
    }
    assert_eq!(must(tool.stats().await)["total"], 1);
}

#[tokio::test]
async fn deleting_removes_a_local_contact_including_do_not_contact() {
    let (_scratch, database) = database().await;
    let tool = ContactsTool::new(Arc::clone(&database));
    let made = must(
        tool.save("none", &json!({ "company": "Acme" }), now())
            .await,
    );
    let id = &made["contact"]["id"];
    assert_eq!(
        must(tool.delete(&json!({ "id": id })).await)["deleted"],
        true
    );
    assert_eq!(
        must(tool.delete(&json!({ "id": id })).await)["deleted"],
        false
    );
    let barred = must(
        tool.save(
            "none",
            &json!({
                "company": "Suppressed", "email": "barred@example.test", "status": "do_not_contact"
            }),
            now(),
        )
        .await,
    );
    assert_eq!(
        must(tool.delete(&json!({ "id": barred["contact"]["id"] })).await)["deleted"],
        true
    );
    assert_eq!(must(tool.stats().await)["total"], 0);
    assert_eq!(must(tool.search(&json!({})).await)["count"], 0);
    for bad in [
        json!({}),
        json!({ "id": "" }),
        json!({ "id": "x".repeat(65) }),
        json!({ "id": "bad\nidentifier" }),
    ] {
        assert!(tool.delete(&bad).await.is_err());
    }
}

#[tokio::test]
async fn deleting_other_contacts_leaves_only_the_chosen_contact_regardless_of_status() {
    let (_scratch, database) = database().await;
    let tool = ContactsTool::new(Arc::clone(&database));
    let kept = must(
        tool.save("none", &json!({ "company": "Keep Me" }), now())
            .await,
    );
    for status in ContactStatus::ALL {
        let made = must(tool.save(
            "none",
            &json!({ "company": format!("Remove {}", status.as_str()), "status": status.as_str() }),
            now(),
        ).await);
        assert_eq!(
            must(tool.delete(&json!({ "id": made["contact"]["id"] })).await)["deleted"],
            true
        );
    }
    let remaining = must(tool.search(&json!({})).await);
    assert_eq!(remaining["count"], 1);
    assert_eq!(remaining["contacts"][0]["id"], kept["contact"]["id"]);
    assert_eq!(remaining["contacts"][0]["company"], "Keep Me");
    assert_eq!(must(tool.stats().await)["total"], 1);
}

fn importing_tool(database: &Arc<SqliteDatabase>, path: &std::path::Path) -> ContactsTool {
    ContactsTool::with_roots(
        Arc::clone(database),
        Arc::new(must(WorkspaceRoots::new([path]))),
    )
}

#[tokio::test]
async fn csv_import_supports_bom_quotes_newlines_and_the_existing_upsert_rules() {
    let (scratch, database) = database().await;
    let tool = importing_tool(&database, &scratch.0);
    let project = must(
        jarvis_storage::create_project(
            &database,
            LOCAL_WORKSPACE_ID,
            &NewProject {
                name: "CSV leads".to_owned(),
                ..NewProject::default()
            },
            now(),
        )
        .await,
    );
    let (session, call) = run_with_call(&database).await;
    must(jarvis_storage::link_project(&database, LinkKind::Session, &session, &project.id).await);
    let csv = "\u{feff} Company ,person,email,notes,status,role,phone,source\r\n\"Acme, BV\",Eva,eva@example.test,\"Line one\r\nLine \"\"two\"\"\",new,Owner,+31 6 1234 5678,CSV\r\nBeta,Tom,tom@example.test,Initial,new,,,\r\nBETA,tom,TOM@example.test,Updated,contacted,,,\r\n";
    must(std::fs::write(scratch.0.join("contacts.csv"), csv));
    let imported = must(
        tool.import(&call, &json!({ "path": "contacts.csv" }), now())
            .await,
    );
    assert_eq!(
        imported,
        json!({ "imported": 3, "created": 2, "updated": 1 })
    );
    let contacts =
        must(jarvis_storage::list_contacts(&database, LOCAL_WORKSPACE_ID, None, 50, 0).await);
    assert_eq!(contacts.len(), 2);
    assert_eq!(contacts[0].company, "Acme, BV");
    assert_eq!(contacts[0].role, "Owner");
    assert_eq!(contacts[0].phone, "+31 6 1234 5678");
    assert_eq!(contacts[0].source, "CSV");
    assert_eq!(contacts[0].notes, "Line one\nLine \"two\"");
    assert!(
        contacts
            .iter()
            .all(|contact| contact.project_id.as_deref() == Some(project.id.as_str()))
    );
    assert_eq!(contacts[1].status, ContactStatus::Contacted);
    assert_eq!(contacts[1].notes, "Updated");
}

#[tokio::test]
async fn malformed_or_invalid_csv_never_changes_any_contact() {
    let (scratch, database) = database().await;
    let tool = importing_tool(&database, &scratch.0);
    must(
        tool.save("none", &json!({ "company": "Existing" }), now())
            .await,
    );
    let before =
        must(jarvis_storage::list_contacts(&database, LOCAL_WORKSPACE_ID, None, 50, 0).await);
    let bad = vec![
        String::new(),
        "company\n".to_owned(),
        "person,email\nEva,eva@example.test".to_owned(),
        "company,Company\nA,B".to_owned(),
        "company,\nA,B".to_owned(),
        "company,unknown\nA,B".to_owned(),
        "company,email\nGood,good@example.test\nBad,not-an-address".to_owned(),
        "company,status\nGood,new\nBad,unknown".to_owned(),
        "company,email\nGood,good@example.test\nMissingColumn".to_owned(),
        "company\nGood\nBad,extra".to_owned(),
        "company\nGood\n\"Unterminated".to_owned(),
        "company\nGood\n\"Closed\"tail".to_owned(),
        "company\nGood\nUnquoted\"quote".to_owned(),
        "company\nGood\n\r".to_owned(),
        "company\nGood\n\"\"".to_owned(),
        format!("company,notes\nGood,ok\nBad,{}", "x".repeat(2_001)),
        format!("company\nGood\n{}", "x".repeat(201)),
        "company,person\nGood,OK\nBad,\"multiline\nperson\"".to_owned(),
        "company\nGood\nControl\0".to_owned(),
        "company\n".to_owned() + &"Row\n".repeat(1_001),
    ];
    for csv in bad {
        must(std::fs::write(
            scratch.0.join("contacts.csv"),
            csv.as_bytes(),
        ));
        assert!(
            tool.import("none", &json!({ "path": "contacts.csv" }), now())
                .await
                .is_err(),
            "{csv:?}"
        );
        assert_eq!(
            must(jarvis_storage::list_contacts(&database, LOCAL_WORKSPACE_ID, None, 50, 0).await),
            before
        );
    }
    for bytes in [vec![0xff, 0xfe], vec![b'x'; 2 * 1024 * 1024 + 1]] {
        must(std::fs::write(scratch.0.join("contacts.csv"), bytes));
        assert!(
            tool.import("none", &json!({ "path": "contacts.csv" }), now())
                .await
                .is_err()
        );
    }
    assert_eq!(
        must(jarvis_storage::list_contacts(&database, LOCAL_WORKSPACE_ID, None, 50, 0).await),
        before
    );
}

#[tokio::test]
async fn import_cannot_lift_suppression_and_a_late_guard_rolls_back_the_batch() {
    let (scratch, database) = database().await;
    let tool = importing_tool(&database, &scratch.0);
    must(tool.save("none", &json!({ "company": "Barred", "email": "barred@example.test", "status": "do_not_contact" }), now()).await);
    must(std::fs::write(
        scratch.0.join("contacts.csv"),
        "company,email,status\nNew,new@example.test,new\nBarred,barred@example.test,new\n",
    ));
    assert!(
        tool.import("none", &json!({ "path": "contacts.csv" }), now())
            .await
            .is_err()
    );
    assert_eq!(must(tool.stats().await)["total"], 1);
    must(std::fs::write(
        scratch.0.join("contacts.csv"),
        "company,email,notes\nBarred,barred@example.test,Still suppressed\n",
    ));
    assert_eq!(
        must(
            tool.import("none", &json!({ "path": "contacts.csv" }), now())
                .await
        )["updated"],
        1
    );
    assert_eq!(
        must(
            jarvis_storage::contact_status_for_email(
                &database,
                LOCAL_WORKSPACE_ID,
                "barred@example.test"
            )
            .await
        ),
        Some(ContactStatus::DoNotContact)
    );
}

#[tokio::test]
async fn csv_import_refuses_paths_outside_the_granted_folder_including_directory_links() {
    let (scratch, database) = database().await;
    let granted = scratch.0.join("granted");
    let outside = scratch.0.join("outside");
    must(std::fs::create_dir_all(&granted));
    must(std::fs::create_dir_all(&outside));
    must(std::fs::write(
        outside.join("contacts.csv"),
        "company\nOutside\n",
    ));
    let link = granted.join("escape");
    #[cfg(windows)]
    {
        let output = must(
            std::process::Command::new("cmd")
                .args(["/C", "mklink", "/J"])
                .arg(&link)
                .arg(&outside)
                .output(),
        );
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    #[cfg(unix)]
    must(std::os::unix::fs::symlink(&outside, &link));
    let tool = importing_tool(&database, &granted);
    for path in [
        "..\\outside\\contacts.csv".to_owned(),
        std::path::Path::new("escape")
            .join("contacts.csv")
            .to_string_lossy()
            .into_owned(),
        outside.join("contacts.csv").to_string_lossy().into_owned(),
        "C:\\outside\\contacts.csv".to_owned(),
        "\\\\server\\share\\contacts.csv".to_owned(),
        String::new(),
        "x".repeat(261),
    ] {
        assert!(
            tool.import("none", &json!({ "path": path }), now())
                .await
                .is_err()
        );
    }
    let no_roots = ContactsTool::new(Arc::clone(&database));
    assert!(
        no_roots
            .import("none", &json!({ "path": "contacts.csv" }), now())
            .await
            .is_err()
    );
    assert!(
        tool.import("none", &json!({ "path": "escape" }), now())
            .await
            .is_err()
    );
    assert_eq!(must(tool.stats().await)["total"], 0);
}
