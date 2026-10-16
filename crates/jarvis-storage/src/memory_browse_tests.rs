use super::*;
use jarvis_core::{MemoryQuery, MemoryTrust};

fn query() -> MemoryQuery {
    MemoryQuery::new(workspace(), at(1_000))
        .with_destination(Sensitivity::Internal)
        .with_allowed_types(vec![MemoryType::Semantic, MemoryType::Preference])
        .with_minimum_trust(MemoryTrust::Untrusted)
        .with_minimum_confidence(MemoryConfidence::Unverified)
}

#[tokio::test]
async fn browse_applies_currency_before_limit_even_across_raw_batches() {
    let (_directory, database) = seeded_database().await;
    let subject = a_subject(&database).await;
    let target = preference(subject);
    must(record_memory(&database, &target, &key_for(&target)).await);
    for number in 1..135 {
        let expired = record_at(
            MemoryType::Semantic,
            &format!("Expired claim {number}"),
            user_source(),
            MemoryConfidence::Confirmed,
            subject,
            at(number),
        );
        must(record_memory(&database, &expired, &key_for(&expired)).await);
        must(
            sqlx::query("UPDATE memories SET valid_until = ?1 WHERE id = ?2")
                .bind(at(200).to_string())
                .bind(expired.id().to_string())
                .execute(database.pool())
                .await,
        );
    }
    let found = must(read_memory_discovery_page(&database, &query(), None, 1).await);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].record().id(), target.id());
    assert_eq!(found[0].record().entities(), target.entities());
    database.close().await;
}

#[tokio::test]
async fn browse_type_entity_and_workspace_filters_are_exact_and_cursor_is_scoped() {
    let (_directory, database) = seeded_database().await;
    let subject = a_subject(&database).await;
    let other_subject = a_subject(&database).await;
    let target = preference(subject);
    let other = record_of(
        MemoryType::Semantic,
        "Other subject claim",
        user_source(),
        MemoryConfidence::Confirmed,
        other_subject,
    );
    for record in [&target, &other] {
        must(record_memory(&database, record, &key_for(record)).await);
    }
    let filtered = query()
        .with_entities(vec![subject])
        .with_allowed_types(vec![MemoryType::Preference]);
    let found = must(read_memory_discovery_page(&database, &filtered, None, 20).await);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].record().id(), target.id());
    assert!(
        must(
            read_memory_discovery_page(
                &database,
                &filtered.with_entities(vec![other_subject]),
                None,
                20
            )
            .await
        )
        .is_empty()
    );
    let mut foreign = query();
    foreign.workspace_id = other_workspace();
    assert!(must(read_memory_discovery_page(&database, &foreign, None, 20).await).is_empty());
    assert!(matches!(
        read_memory_discovery_page(&database, &foreign, Some(target.id()), 20).await,
        Err(DatabaseError::InvalidMemoryRequest { field: "cursor" })
    ));
    assert!(matches!(
        read_memory_discovery_page(&database, &query(), Some(MemoryId::new()), 20).await,
        Err(DatabaseError::InvalidMemoryRequest { field: "cursor" })
    ));
    for limit in [0, 202] {
        assert!(matches!(
            read_memory_discovery_page(&database, &query(), None, limit).await,
            Err(DatabaseError::InvalidMemoryRequest { field: "limit" })
        ));
    }
    database.close().await;
}
