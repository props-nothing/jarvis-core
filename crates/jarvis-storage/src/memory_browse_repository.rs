//! Bounded, keyword-free discovery of current memory, with keyset pagination.

use jarvis_core::{MemoryId, MemoryQuery, Sensitivity};

use super::{
    DatabaseError, SqliteDatabase, StoredMemory, attach_entity_links, decode_stored_memory,
};

/// Reads current eligible claims after a workspace-scoped cursor, newest first.
///
/// Entity filters are exact; no keyword is required. Eligibility precedes the result limit.
/// A cursor is the last returned memory ID. Pages reflect current state, not a frozen export.
///
/// # Errors
///
/// Returns a validation error for invalid bounds or a cursor outside this workspace, or a storage error.
pub async fn read_memory_discovery_page(
    database: &SqliteDatabase,
    query: &MemoryQuery,
    after: Option<MemoryId>,
    limit: u32,
) -> Result<Vec<StoredMemory>, DatabaseError> {
    if limit == 0 || limit > 201 {
        return Err(DatabaseError::InvalidMemoryRequest { field: "limit" });
    }
    let json = |values: Vec<String>| {
        serde_json::to_string(&values).map_err(|_| DatabaseError::InvalidMemoryRequest {
            field: "browse_query",
        })
    };
    let types = json(
        query
            .allowed_types
            .iter()
            .map(|kind| kind.as_str().to_owned())
            .collect(),
    )?;
    let sensitivities = json(
        [
            Sensitivity::Public,
            Sensitivity::Internal,
            Sensitivity::Confidential,
            Sensitivity::Restricted,
        ]
        .into_iter()
        .filter(|level| level.can_flow_to(query.destination))
        .map(|level| level.as_str().to_owned())
        .collect(),
    )?;
    let entities = json(query.entity_ids.iter().map(ToString::to_string).collect())?;
    let mut transaction = database.pool().begin().await.map_err(storage)?;
    let mut cursor = after.map(|id| id.to_string());
    if let Some(id) = &cursor {
        let exists: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM memories WHERE id = ?1 AND workspace_id = ?2")
                .bind(id)
                .bind(query.workspace_id.to_string())
                .fetch_one(&mut *transaction)
                .await
                .map_err(storage)?;
        if exists == 0 {
            return Err(DatabaseError::InvalidMemoryRequest { field: "cursor" });
        }
    }
    let mut memories = Vec::new();
    while memories.len() < limit as usize {
        let rows = sqlx::query(
            "SELECT m.* FROM memories m
             WHERE m.workspace_id = ?1 AND m.status = 'active'
               AND m.source_kind <> 'model_inference'
               AND (m.claim_predicate IS NULL OR lower(m.claim_predicate) NOT IN (
                 'objective', 'current_objective', 'pending_call', 'planned_step', 'plan', 'next_action'))
               AND m.sensitivity IN (SELECT value FROM json_each(?2))
               AND (?3 = '[]' OR m.memory_type IN (SELECT value FROM json_each(?3)))
               AND (?4 = '[]' OR EXISTS (
                 SELECT 1 FROM memory_entities e WHERE e.memory_id = m.id
                   AND e.entity_id IN (SELECT value FROM json_each(?4))))
               AND (?5 IS NULL OR unixepoch(m.created_at) < (
                 SELECT unixepoch(created_at) FROM memories WHERE id = ?5 AND workspace_id = ?1)
                 OR (unixepoch(m.created_at) = (
                   SELECT unixepoch(created_at) FROM memories WHERE id = ?5 AND workspace_id = ?1)
                   AND m.id < ?5))
             ORDER BY unixepoch(m.created_at) DESC, m.id DESC LIMIT 128"
        ).bind(query.workspace_id.to_string()).bind(&sensitivities).bind(&types).bind(&entities).bind(&cursor)
            .fetch_all(&mut *transaction).await.map_err(storage)?;
        if rows.is_empty() {
            break;
        }
        for row in &rows {
            let stored = decode_stored_memory(row)?;
            cursor = Some(stored.record().id().to_string());
            if query.is_eligible(stored.record()).is_ok() {
                memories.push(stored);
                if memories.len() == limit as usize {
                    break;
                }
            }
        }
    }
    transaction.commit().await.map_err(storage)?;
    attach_entity_links(database, &mut memories).await?;
    Ok(memories)
}

fn storage(source: sqlx::Error) -> DatabaseError {
    DatabaseError::Sqlite {
        operation: "browse memory",
        source,
    }
}
