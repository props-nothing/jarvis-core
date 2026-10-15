use super::*;
use jarvis_core::{MemoryQuery, MemoryTrust};

fn task_query(text: &str) -> MemoryQuery {
    MemoryQuery::new(workspace(), at(1_000))
        .with_text(text)
        .with_destination(Sensitivity::Internal)
        .with_allowed_types(vec![MemoryType::Semantic, MemoryType::Preference])
        .with_minimum_trust(MemoryTrust::Untrusted)
        .with_minimum_confidence(MemoryConfidence::Unverified)
}

async fn save(database: &SqliteDatabase, record: &MemoryRecord) {
    must(record_memory(database, record, &key_for(record)).await);
}

#[tokio::test]
async fn task_candidates_find_old_matches_before_limiting_recent_rows() {
    let (_directory, database) = seeded_database().await;
    let subject = a_subject(&database).await;
    let target = record_at(
        MemoryType::Preference,
        "Élodie reviews the contract",
        user_source(),
        MemoryConfidence::Confirmed,
        subject,
        at(0),
    );
    save(&database, &target).await;
    for number in 1..40 {
        let unrelated = record_at(
            MemoryType::Semantic,
            &format!("Unrelated inventory observation {number}"),
            user_source(),
            MemoryConfidence::Confirmed,
            subject,
            at(number),
        );
        save(&database, &unrelated).await;
    }
    let newest = must(read_retrievable_memories(&database, workspace(), 24).await);
    assert!(
        !newest
            .iter()
            .any(|stored| stored.record().id() == target.id())
    );
    for text in ["ÉLODIE?", "CONTRACT!", "What did Élodie agree?"] {
        let found = must(read_task_memory_candidates(&database, &task_query(text), 1).await);
        assert_eq!(found[0].record().id(), target.id(), "{text}");
        assert_eq!(found[0].record().entities(), target.entities());
    }
    database.close().await;
}

#[tokio::test]
async fn task_candidates_do_not_confuse_key_metadata_or_substrings_with_claim_words() {
    let (_directory, database) = seeded_database().await;
    let subject = a_subject(&database).await;
    let target = record_at(
        MemoryType::Preference,
        "The semantic contract is signed",
        user_source(),
        MemoryConfidence::Confirmed,
        subject,
        at(0),
    );
    let unrelated = record_at(
        MemoryType::Semantic,
        "The contractor serves tea",
        user_source(),
        MemoryConfidence::Confirmed,
        subject,
        at(1),
    );
    save(&database, &target).await;
    save(&database, &unrelated).await;
    for text in ["semantic", "contract"] {
        let found = must(read_task_memory_candidates(&database, &task_query(text), 1).await);
        assert_eq!(found[0].record().id(), target.id(), "{text}");
    }
    let recent = must(read_task_memory_candidates(&database, &task_query("% _ \""), 1).await);
    assert_eq!(recent[0].record().id(), unrelated.id());
    database.close().await;
}

#[tokio::test]
async fn expired_matches_are_filtered_before_the_limit_across_candidate_pages() {
    let (_directory, database) = seeded_database().await;
    let subject = a_subject(&database).await;
    let target = record_at(
        MemoryType::Semantic,
        "Acme current contract",
        user_source(),
        MemoryConfidence::Confirmed,
        subject,
        at(0),
    );
    save(&database, &target).await;
    for number in 0..130 {
        let expired = record_at(
            MemoryType::Semantic,
            &format!("Acme expired contract {number}"),
            user_source(),
            MemoryConfidence::Confirmed,
            subject,
            at(1),
        );
        save(&database, &expired).await;
        must(
            sqlx::query("UPDATE memories SET valid_until = ?2 WHERE id = ?1")
                .bind(expired.id().to_string())
                .bind(at(2).to_string())
                .execute(database.pool())
                .await,
        );
    }
    let found = must(read_task_memory_candidates(&database, &task_query("Acme contract"), 1).await);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].record().id(), target.id());
    database.close().await;
}

#[tokio::test]
async fn task_recall_fails_closed_for_scope_sensitivity_type_and_inferred_facts() {
    let (_directory, database) = seeded_database().await;
    let subject = a_subject(&database).await;
    let allowed = record_at(
        MemoryType::Semantic,
        "Acme public contract",
        user_source(),
        MemoryConfidence::Confirmed,
        subject,
        at(0),
    );
    save(&database, &allowed).await;
    let confidential = record_at(
        MemoryType::Semantic,
        "Acme confidential contract",
        user_source(),
        MemoryConfidence::Confirmed,
        subject,
        at(1),
    );
    save(&database, &confidential).await;
    must(
        sqlx::query("UPDATE memories SET sensitivity = 'confidential' WHERE id = ?1")
            .bind(confidential.id().to_string())
            .execute(database.pool())
            .await,
    );
    for kind in [MemoryType::Working, MemoryType::Conversation] {
        let excluded = record_at(
            kind,
            "Acme contract task state",
            user_source(),
            MemoryConfidence::Confirmed,
            subject,
            at(2),
        );
        save(&database, &excluded).await;
    }
    let inference = record_of(
        MemoryType::Semantic,
        "Acme inferred contract",
        must(MemorySource::of_kind(
            MemorySourceKind::ModelInference,
            "run:fixture",
        )),
        MemoryConfidence::Unverified,
        subject,
    );
    save(&database, &inference).await;
    must(
        apply_memory_transition(
            &database,
            &inference.id().to_string(),
            MemoryTransition::Confirm {
                approver_actor_id: LOCAL_USER_ID.to_owned(),
            },
            at(3),
        )
        .await,
    );
    let plan = record_at(
        MemoryType::Semantic,
        "Acme contract next action",
        user_source(),
        MemoryConfidence::Confirmed,
        subject,
        at(4),
    );
    save(&database, &plan).await;
    must(sqlx::query("UPDATE memories SET claim_subject = 'Acme', claim_predicate = 'plan', claim_object = 'contract' WHERE id = ?1")
        .bind(plan.id().to_string()).execute(database.pool()).await);
    let found = must(read_task_memory_candidates(&database, &task_query("Acme contract"), 1).await);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].record().id(), allowed.id());
    let local = must(
        read_task_memory_candidates(
            &database,
            &task_query("Acme confidential").with_destination(Sensitivity::Restricted),
            1,
        )
        .await,
    );
    assert_eq!(local[0].record().id(), confidential.id());
    let mut query = task_query("Acme contract");
    query.workspace_id = other_workspace();
    assert!(must(read_task_memory_candidates(&database, &query, 10).await).is_empty());
    for limit in [0, MAX_RECALL_CANDIDATES + 1] {
        assert!(matches!(
            read_task_memory_candidates(&database, &query, limit).await,
            Err(DatabaseError::InvalidMemoryRequest { field: "limit" })
        ));
    }
    database.close().await;
}
