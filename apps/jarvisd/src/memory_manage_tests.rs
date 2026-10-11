//! Correcting and forgetting memories: the owner's yes names which claim, and a quotation that is not in it changes nothing.

use jarvis_core::CorrelationId;
use jarvis_protocol::RememberRequest;

use super::*;

struct Scratch(std::path::PathBuf);

impl Drop for Scratch {
    fn drop(&mut self) {
        jarvis_core::remove_scratch_dir(&self.0);
    }
}

async fn fixture() -> (Scratch, Arc<SqliteDatabase>, MemoryManageTool) {
    let tag = jarvis_core::scratch_tag();
    let path = std::env::temp_dir().join(format!("jmm-{}", &tag[tag.len() - 12..]));
    std::fs::create_dir_all(&path).unwrap_or_else(|error| panic!("create scratch: {error}"));
    let database = Arc::new(
        SqliteDatabase::open(&path.join("jarvis.sqlite3"))
            .await
            .unwrap_or_else(|error| panic!("open the database: {error}")),
    );
    let tool = MemoryManageTool::new(Arc::clone(&database));
    (Scratch(path), database, tool)
}

async fn remember(database: &Arc<SqliteDatabase>, content: &str) -> String {
    MemoryService::new(Arc::clone(database))
        .remember(&RememberRequest {
            content: content.to_owned(),
            memory_type: "preference".to_owned(),
            source_kind: "user_statement".to_owned(),
            importance: Some(5),
            entity_ids: Vec::new(),
            claim: None,
            supersedes: None,
        })
        .await
        .unwrap_or_else(|error| panic!("remember: {error}"))
        .memory_id
}

#[test]
fn both_tools_ask_and_a_quotation_must_really_be_in_the_claim() {
    let definitions = MemoryManageTool::definitions().unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(definitions.len(), 2);
    for definition in &definitions {
        assert_eq!(definition.approval(), ApprovalPolicy::Ask);
        assert!(definition.effects().contains(ToolEffect::Write));
    }
    let claim = "The user prefers dark roast coffee";
    assert!(quotation_matches("prefers  DARK roast", claim));
    assert!(quotation_matches(claim, claim));
    assert!(!quotation_matches("prefers tea", claim));
    assert!(
        !quotation_matches("coffee", claim),
        "a short quotation stands for nothing"
    );
    assert!(
        quotation_matches("tea", "tea"),
        "unless it is the whole claim"
    );
    assert!(!quotation_matches("", claim));
    let _ = CorrelationId::new();
}

/// **What the owner read is what is deleted, or nothing is.**
#[tokio::test]
async fn forgetting_needs_the_quoted_claim_to_be_in_the_memory() {
    let (_scratch, database, tool) = fixture().await;
    let id = remember(&database, "The user prefers dark roast coffee").await;
    let service = MemoryService::new(Arc::clone(&database));

    let wrong = tool
        .forget(&json!({ "memory_id": id, "claim": "lives in Rotterdam" }))
        .await;
    assert!(
        matches!(wrong, Err(AdapterError::RefusedBeforeReaching { .. })),
        "{wrong:?}"
    );
    assert!(
        service.read(&id).await.is_ok(),
        "a mismatch leaves the memory alone"
    );

    let gone = tool
        .forget(&json!({ "memory_id": id, "claim": "prefers dark roast" }))
        .await
        .unwrap_or_else(|error| panic!("{error:?}"));
    assert_eq!(gone["forgotten"], id.as_str());
    assert_eq!(gone["tombstone_written"], true);
    assert!(service.read(&id).await.is_err());
    let again = tool
        .forget(&json!({ "memory_id": id, "claim": "prefers dark roast" }))
        .await;
    assert!(
        again.is_err(),
        "a forgotten memory cannot be forgotten twice"
    );
}

#[tokio::test]
async fn correcting_replaces_the_claim_with_the_corrected_text() {
    let (_scratch, database, tool) = fixture().await;
    let id = remember(&database, "The user prefers dark roast coffee").await;
    let service = MemoryService::new(Arc::clone(&database));

    let wrong = tool
        .correct(&json!({ "memory_id": id, "claim": "prefers green tea", "corrected": "The user prefers tea" }))
        .await;
    assert!(wrong.is_err());

    let done = tool
        .correct(&json!({ "memory_id": id, "claim": "dark roast coffee", "corrected": "The user prefers light roast coffee" }))
        .await
        .unwrap_or_else(|error| panic!("{error:?}"));
    let new_id = done["replaced_by"].as_str().unwrap_or_default().to_owned();
    assert_ne!(new_id, id);
    let stored = service
        .read(&new_id)
        .await
        .unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(stored.content, "The user prefers light roast coffee");
}

/// **A correction that is too long for one memory is refused in plain words, before anything is changed.**
///
/// The audit saw a correction refused with "a search key must be 1 to 32 words" and read it as a limit on the quotation.
#[tokio::test]
async fn a_correction_longer_than_one_short_fact_is_refused_in_plain_words() {
    let (_scratch, database, tool) = fixture().await;
    let id = remember(&database, "The user prefers dark roast coffee").await;
    let long = (0..40)
        .map(|index| format!("word{index}"))
        .collect::<Vec<_>>()
        .join(" ");
    let refused = tool
        .correct(&json!({ "memory_id": id, "claim": "dark roast coffee", "corrected": long }))
        .await
        .err()
        .unwrap_or_else(|| panic!("a 40-word correction was accepted"));
    let said = format!("{refused:?}");
    assert!(
        said.contains("40 different words") && said.contains("at most 32"),
        "{said}"
    );
    assert!(said.contains("project note"), "{said}");
    // Nothing changed: the old memory is still there.
    let service = MemoryService::new(Arc::clone(&database));
    assert!(
        service
            .read(&id)
            .await
            .is_ok_and(|memory| memory.content.contains("dark roast"))
    );
    // Thirty-two different words is the limit and is accepted; repeats do not count twice.
    let exactly = (0..32)
        .map(|index| format!("w{index}"))
        .collect::<Vec<_>>()
        .join(" ")
        + " w1 W2 w3.";
    assert!(
        tool.correct(
            &json!({ "memory_id": id, "claim": "dark roast coffee", "corrected": exactly })
        )
        .await
        .is_ok(),
        "32 different words must be accepted"
    );
}
