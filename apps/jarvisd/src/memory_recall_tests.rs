use std::sync::Arc;

use jarvis_core::SystemClock;
use jarvis_protocol::{CorrectMemoryRequest, ForgetMemoryRequest, RememberRequest};
use jarvis_storage::load_local_identity;

use super::*;
use crate::memory_service::MemoryService;

#[test]
fn task_questions_ignore_glue_but_keep_negations_and_late_distinctive_terms() {
    assert_eq!(task_text("What did we agree with ÉLODIE?"), "agree élodie");
    assert_eq!(
        task_text("Wat weet je over het contract van Élodie?"),
        "contract élodie"
    );
    assert_eq!(
        task_text("Do not renew Acme, niet betalen"),
        "not renew acme niet betalen"
    );
    assert_eq!(
        task_text(&format!("{} Acme", "what did we ".repeat(40))),
        "acme"
    );
}

struct Scratch(std::path::PathBuf);

impl Drop for Scratch {
    fn drop(&mut self) {
        jarvis_core::remove_scratch_dir(&self.0);
    }
}

async fn fixture() -> (Scratch, Arc<SqliteDatabase>, WorkspaceId) {
    let path = std::env::temp_dir().join(format!("jrecall-{}", jarvis_core::scratch_tag()));
    std::fs::create_dir_all(&path).unwrap_or_else(|error| panic!("scratch: {error}"));
    let database = Arc::new(
        SqliteDatabase::test_fixture(&path.join("jarvis.sqlite3"))
            .await
            .unwrap_or_else(|error| panic!("database: {error}")),
    );
    let identity = load_local_identity(&database)
        .await
        .unwrap_or_else(|error| panic!("identity: {error}"));
    let workspace = identity
        .workspace_id()
        .parse()
        .unwrap_or_else(|error| panic!("workspace: {error}"));
    (Scratch(path), database, workspace)
}

async fn remember(database: &Arc<SqliteDatabase>, content: &str) -> String {
    MemoryService::new(Arc::clone(database))
        .remember(&RememberRequest {
            content: content.to_owned(),
            memory_type: "semantic".to_owned(),
            source_kind: "user_statement".to_owned(),
            importance: Some(2),
            entity_ids: Vec::new(),
            claim: None,
            supersedes: None,
        })
        .await
        .unwrap_or_else(|error| panic!("remember: {error}"))
        .memory_id
}

#[tokio::test]
async fn an_old_task_match_precedes_newer_unrelated_memories_with_provenance() {
    let (_scratch, database, workspace) = fixture().await;
    let target = remember(&database, "Acme renews its contract in November").await;
    for number in 0..40 {
        remember(
            &database,
            &format!("Unrelated inventory observation number {number}"),
        )
        .await;
    }
    let result = recall(
        &database,
        workspace,
        "When does ACME renew?",
        UtcTimestamp::now(&SystemClock),
        Sensitivity::Internal,
    )
    .await
    .unwrap_or_else(|error| panic!("recall: {error}"));
    assert_eq!(result.memories.len(), SELECTED);
    let first = &result.memories[0];
    assert!(first.reference().contains(&target));
    let body = first.isolated().body();
    for expected in [
        "Acme renews",
        "memory_id:",
        "source_kind: user_statement",
        "source:",
        "confidence: confirmed",
        "created_at:",
        "valid_from:",
    ] {
        assert!(body.contains(expected), "missing {expected}: {body}");
    }
    assert_eq!(result.summary["considered"], 41);
    assert_eq!(result.summary["selected"][0]["reason"], "task_match");
    assert_eq!(result.summary["selected"][1]["reason"], "context_fallback");
    assert!(!result.summary.to_string().contains("November"));
    database.close().await;
}

#[tokio::test]
async fn corrections_and_forgetting_remove_old_claims_from_task_recall() {
    let (_scratch, database, workspace) = fixture().await;
    let id = remember(&database, "Acme renews in November").await;
    let service = MemoryService::new(Arc::clone(&database));
    let original = service
        .read(&id)
        .await
        .unwrap_or_else(|error| panic!("read: {error}"));
    let replacement = service
        .correct(
            &id,
            &CorrectMemoryRequest {
                content: "Acme renews in December".to_owned(),
                expected_version: original.reference.version,
                entity_ids: None,
            },
        )
        .await
        .unwrap_or_else(|error| panic!("correct: {error}"));
    let current = recall(
        &database,
        workspace,
        "Acme renewal",
        UtcTimestamp::now(&SystemClock),
        Sensitivity::Internal,
    )
    .await
    .unwrap_or_else(|error| panic!("recall: {error}"));
    assert_eq!(current.memories.len(), 1);
    assert!(current.memories[0].isolated().body().contains("December"));
    assert!(!current.memories[0].isolated().body().contains("November"));
    let detail = service
        .read(&replacement.memory_id)
        .await
        .unwrap_or_else(|error| panic!("read replacement: {error}"));
    service
        .forget(
            &replacement.memory_id,
            &ForgetMemoryRequest {
                expected_version: detail.reference.version,
                allow_relearn: false,
            },
        )
        .await
        .unwrap_or_else(|error| panic!("forget: {error}"));
    let gone = recall(
        &database,
        workspace,
        "Acme renewal",
        UtcTimestamp::now(&SystemClock),
        Sensitivity::Internal,
    )
    .await
    .unwrap_or_else(|error| panic!("recall: {error}"));
    assert!(gone.memories.is_empty());
    database.close().await;
}

#[tokio::test]
async fn thirty_fixture_questions_recover_old_facts_that_recency_only_loading_misses() {
    let (_scratch, database, workspace) = fixture().await;
    let mut cases = Vec::new();
    for number in 0..30 {
        let name = format!("Client{number:02}");
        let id = remember(
            &database,
            &format!("{name} renews its contract in November"),
        )
        .await;
        let question = match number % 3 {
            0 => format!("When does {name} renew?"),
            1 => format!("Wat weet je over het contract van {name}?"),
            _ => format!("What did we agree with {}?", name.to_uppercase()),
        };
        cases.push((question, id));
    }
    for number in 0..140 {
        remember(
            &database,
            &format!("We have the unrelated inventory observation {number}"),
        )
        .await;
    }
    let baseline = jarvis_storage::read_retrievable_memories(&database, workspace, 24)
        .await
        .unwrap_or_else(|error| panic!("baseline: {error}"));
    let mut baseline_hits = 0;
    let mut task_hits = 0;
    for (question, id) in &cases {
        baseline_hits += usize::from(
            baseline
                .iter()
                .any(|row| row.record().id().to_string() == *id),
        );
        let result = recall(
            &database,
            workspace,
            question,
            UtcTimestamp::now(&SystemClock),
            Sensitivity::Internal,
        )
        .await
        .unwrap_or_else(|error| panic!("recall: {error}"));
        task_hits += usize::from(
            result
                .memories
                .first()
                .is_some_and(|memory| memory.reference().contains(id)),
        );
        assert_eq!(result.memories.len(), SELECTED);
        assert_eq!(result.summary["considered"], CANDIDATES);
    }
    assert_eq!(
        baseline_hits, 0,
        "all fixture facts must be outside the old window"
    );
    assert_eq!(
        task_hits,
        cases.len(),
        "all 30 expected facts must rank first"
    );
    database.close().await;
}

#[tokio::test]
async fn punctuation_and_unicode_case_find_the_same_claim_without_substring_matches() {
    let (_scratch, database, workspace) = fixture().await;
    let target = remember(&database, "Élodie bespreekt het contract in België").await;
    remember(&database, "The contractor reviewed the cafe").await;
    for question in ["ÉLODIE?", "Wat weet je over Élodie?", "Het CONTRACT?"] {
        let result = recall(
            &database,
            workspace,
            question,
            UtcTimestamp::now(&SystemClock),
            Sensitivity::Internal,
        )
        .await
        .unwrap_or_else(|error| panic!("recall: {error}"));
        assert!(
            result.memories[0].reference().contains(&target),
            "{question}"
        );
        assert_eq!(result.summary["selected"][0]["reason"], "task_match");
    }
    let result = recall(
        &database,
        workspace,
        "tractor",
        UtcTimestamp::now(&SystemClock),
        Sensitivity::Internal,
    )
    .await
    .unwrap_or_else(|error| panic!("recall: {error}"));
    assert!(
        result.summary["selected"]
            .as_array()
            .unwrap_or_else(|| panic!("selections"))
            .iter()
            .all(|item| item["reason"] == "context_fallback")
    );
    database.close().await;
}
