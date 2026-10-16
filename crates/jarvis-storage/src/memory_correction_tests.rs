use super::*;

fn replacement(original: &MemoryRecord, subject: EntityId, content: &str) -> MemoryRecord {
    must(MemoryRecord::new(MemoryRecordParts {
        id: MemoryId::new(),
        workspace_id: workspace(),
        memory_type: MemoryType::Preference,
        content: content.to_owned(),
        structured_claim: None,
        source: user_source(),
        confidence: MemoryConfidence::Confirmed,
        importance: 1,
        sensitivity: Sensitivity::Internal,
        entities: vec![EntityRef::confirmed(subject)],
        valid_from: None,
        valid_until: None,
        supersedes: Some(original.id()),
        run_id: None,
        created_by_actor_id: LOCAL_USER_ID.to_owned(),
        correlation_id: CorrelationId::new(),
        created_at: at(1),
    }))
}

async fn counts(database: &SqliteDatabase) -> (i64, i64) {
    must(
        sqlx::query_as(
            "SELECT (SELECT COUNT(*) FROM memories), (SELECT COUNT(*) FROM memory_entities)",
        )
        .fetch_one(database.pool())
        .await,
    )
}

#[tokio::test]
async fn rejected_and_repeated_corrections_write_nothing() {
    let (_directory, database) = seeded_database().await;
    let subject = a_subject(&database).await;
    let original = preference(subject);
    must(record_memory(&database, &original, &key_for(&original)).await);
    let first = replacement(&original, subject, "Prefers light roast coffee");
    must(
        replace_memory(
            &database,
            &original.id().to_string(),
            1,
            &first,
            &key_for(&first),
            at(1),
        )
        .await,
    );
    let unwanted = replacement(&original, subject, "Prefers herbal tea");
    for (version, candidate) in [(1, &first), (1, &unwanted), (2, &unwanted)] {
        assert!(matches!(
            replace_memory(
                &database,
                &original.id().to_string(),
                version,
                candidate,
                &key_for(candidate),
                at(2)
            )
            .await,
            Err(DatabaseError::MemoryConflict)
        ));
        assert_eq!(counts(&database).await, (2, 2));
    }
    let stored = must(find_memory(&database, &original.id().to_string()).await);
    assert_eq!(stored.record().superseded_by(), Some(first.id()));
    assert_eq!(stored.version(), 2);
    assert!(matches!(
        find_memory(&database, &unwanted.id().to_string()).await,
        Err(DatabaseError::MemoryNotFound)
    ));
}

#[tokio::test]
async fn a_failed_supersession_rolls_back_the_replacement_and_entity_links() {
    let (_directory, database) = seeded_database().await;
    let subject = a_subject(&database).await;
    let original = preference(subject);
    must(record_memory(&database, &original, &key_for(&original)).await);
    must(sqlx::query("CREATE TRIGGER reject_correction BEFORE UPDATE OF superseded_by_memory_id ON memories BEGIN SELECT RAISE(ABORT, 'synthetic write failure'); END")
        .execute(database.pool()).await);
    let candidate = replacement(&original, subject, "Prefers herbal tea");
    assert!(
        replace_memory(
            &database,
            &original.id().to_string(),
            1,
            &candidate,
            &key_for(&candidate),
            at(1)
        )
        .await
        .is_err()
    );
    assert_eq!(counts(&database).await, (1, 1));
    let stored = must(find_memory(&database, &original.id().to_string()).await);
    assert_eq!(stored.record().status(), MemoryStatus::Active);
    assert_eq!(stored.record().superseded_by(), None);
    assert_eq!(stored.version(), 1);
}

#[tokio::test]
async fn concurrent_corrections_commit_only_one_replacement() {
    let (_directory, database) = seeded_database().await;
    let subject = a_subject(&database).await;
    let original = preference(subject);
    must(record_memory(&database, &original, &key_for(&original)).await);
    let first = replacement(&original, subject, "Prefers light roast coffee");
    let second = replacement(&original, subject, "Prefers herbal tea");
    let id = original.id().to_string();
    let first_key = key_for(&first);
    let second_key = key_for(&second);
    let (left, right) = tokio::join!(
        replace_memory(&database, &id, 1, &first, &first_key, at(1)),
        replace_memory(&database, &id, 1, &second, &second_key, at(1)),
    );
    assert_eq!(usize::from(left.is_ok()) + usize::from(right.is_ok()), 1);
    let loser = if left.is_ok() { right } else { left };
    assert!(matches!(loser, Err(DatabaseError::MemoryConflict)));
    assert_eq!(counts(&database).await, (2, 2));
    let current = must(read_workspace_memories(&database, workspace(), 20).await);
    assert_eq!(
        current
            .iter()
            .filter(|memory| memory.record().status() == MemoryStatus::Active)
            .count(),
        1
    );
}
