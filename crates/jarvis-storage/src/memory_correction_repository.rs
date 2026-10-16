//! One versioned transaction for a memory correction and its entity links.

use super::{
    DatabaseError, MemoryRecord, MemorySearchKey, SqliteDatabase, UtcTimestamp,
    decode_stored_memory, insert_memory_on, link_memory_entity_on, memory_error, search_key_hash,
};

/// Replaces one inspected memory, atomically archiving its previous version.
///
/// No replacement, entity link or supersession survives a refused or failed correction.
/// The workspace, predecessor, version, lifecycle and tombstone are checked while the
/// same immediate transaction owns the write lock.
///
/// # Errors
///
/// Returns a conflict for a stale/already-superseded original, a not-found error for
/// a missing/foreign original, and the usual memory validation/storage errors otherwise.
pub async fn replace_memory(
    database: &SqliteDatabase,
    original_id: &str,
    expected_version: i64,
    replacement: &MemoryRecord,
    search_key: &MemorySearchKey,
    at: UtcTimestamp,
) -> Result<(), DatabaseError> {
    let mut transaction = database
        .pool()
        .begin_with("BEGIN IMMEDIATE")
        .await
        .map_err(storage)?;
    let row = sqlx::query("SELECT * FROM memories WHERE id = ?1 AND workspace_id = ?2")
        .bind(original_id)
        .bind(replacement.workspace_id().to_string())
        .fetch_optional(&mut *transaction)
        .await
        .map_err(storage)?
        .ok_or(DatabaseError::MemoryNotFound)?;
    let original = decode_stored_memory(&row)?;
    if original.version() != expected_version || original.record().superseded_by().is_some() {
        return Err(DatabaseError::MemoryConflict);
    }
    if replacement.supersedes() != Some(original.record().id()) {
        return Err(DatabaseError::InvalidMemoryRequest {
            field: "supersedes_memory_id",
        });
    }
    let next = original
        .record()
        .replace_with(replacement.id(), at)
        .map_err(|error| memory_error(&error))?;
    let tombstoned: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM memory_tombstones WHERE workspace_id = ?1 AND search_key_hash = ?2)",
    )
    .bind(replacement.workspace_id().to_string())
    .bind(search_key_hash(search_key.as_str()))
    .fetch_one(&mut *transaction)
    .await
    .map_err(storage)?;
    if tombstoned {
        return Err(DatabaseError::MemoryTombstoned);
    }
    if insert_memory_on(&mut *transaction, replacement, search_key).await? == 0 {
        let existing_memory_id = sqlx::query_scalar::<_, String>(
            "SELECT id FROM memories WHERE workspace_id = ?1 AND search_key = ?2",
        )
        .bind(replacement.workspace_id().to_string())
        .bind(search_key.as_str())
        .fetch_one(&mut *transaction)
        .await
        .map_err(storage)?;
        return Err(DatabaseError::MemoryDuplicate { existing_memory_id });
    }
    for entity in replacement.entities() {
        link_memory_entity_on(&mut *transaction, replacement.id(), entity.clone()).await?;
    }
    let changed = sqlx::query(
        "UPDATE memories SET status = ?3, superseded_by_memory_id = ?4, updated_at = ?5, version = version + 1 \
         WHERE id = ?1 AND version = ?2 AND superseded_by_memory_id IS NULL",
    )
    .bind(original_id)
    .bind(expected_version)
    .bind(next.status().as_str())
    .bind(replacement.id().to_string())
    .bind(next.updated_at().to_string())
    .execute(&mut *transaction)
    .await
    .map_err(storage)?;
    if changed.rows_affected() != 1 {
        return Err(DatabaseError::MemoryConflict);
    }
    transaction.commit().await.map_err(storage)
}

fn storage(source: sqlx::Error) -> DatabaseError {
    DatabaseError::Sqlite {
        operation: "replace a memory atomically",
        source,
    }
}
