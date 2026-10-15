//! Query-first candidates over canonical memory, without a new index or embedding provider.

use jarvis_core::{MemoryQuery, Sensitivity, memory_query_terms};

use super::{
    DatabaseError, SqliteDatabase, StoredMemory, attach_entity_links, decode_stored_memory,
};

const PAGE_SIZE: i64 = 128;
/// The largest candidate set a task recall may request.
pub const MAX_RECALL_CANDIDATES: u32 = 256;

/// Finds task-relevant eligible memories across the workspace, then recent fallback records.
///
/// The stored search key supplies normalized words; matching rows precede recent unrelated rows.
/// Eligibility is checked before the result limit, so expired or sensitive matches cannot crowd out
/// an eligible older claim. Pages share a read transaction. No additional derived text is retained.
///
/// # Errors
///
/// Returns a validation error for an invalid limit, or a storage error when a read or decode fails.
pub async fn read_task_memory_candidates(
    database: &SqliteDatabase,
    query: &MemoryQuery,
    limit: u32,
) -> Result<Vec<StoredMemory>, DatabaseError> {
    if limit == 0 || limit > MAX_RECALL_CANDIDATES {
        return Err(DatabaseError::InvalidMemoryRequest { field: "limit" });
    }
    let json = |value: &[String]| {
        serde_json::to_string(value).map_err(|_| DatabaseError::InvalidMemoryRequest {
            field: "recall_query",
        })
    };
    let terms = json(&memory_query_terms(&query.text))?;
    let types = json(
        &query
            .allowed_types
            .iter()
            .map(|kind| kind.as_str().to_owned())
            .collect::<Vec<_>>(),
    )?;
    let sensitivities = json(
        &[
            Sensitivity::Public,
            Sensitivity::Internal,
            Sensitivity::Confidential,
            Sensitivity::Restricted,
        ]
        .into_iter()
        .filter(|level| level.can_flow_to(query.destination))
        .map(|level| level.as_str().to_owned())
        .collect::<Vec<_>>(),
    )?;
    let result_limit = usize::try_from(limit)
        .map_err(|_| DatabaseError::InvalidMemoryRequest { field: "limit" })?;
    let page_size = usize::try_from(PAGE_SIZE).unwrap_or(128);
    let mut transaction = database.pool().begin().await.map_err(storage)?;
    let mut memories = Vec::new();
    let mut offset = 0_i64;
    while memories.len() < result_limit {
        let rows = sqlx::query(
            "WITH scoped AS ( \
                 SELECT m.*, substr(m.search_key, instr(m.search_key, '|') + 1) AS entity_and_words \
                 FROM memories m \
                 WHERE m.workspace_id = ?1 AND m.status = 'active' \
                   AND m.source_kind <> 'model_inference' \
                   AND (m.claim_predicate IS NULL OR lower(m.claim_predicate) NOT IN ( \
                       'objective', 'current_objective', 'pending_call', 'planned_step', 'plan', 'next_action')) \
                   AND m.sensitivity IN (SELECT value FROM json_each(?3)) \
                   AND (?4 = '[]' OR m.memory_type IN (SELECT value FROM json_each(?4))) \
             ) \
             SELECT scoped.*, \
                 (SELECT COUNT(*) FROM json_each(?2) t \
                  WHERE instr(' ' || substr(entity_and_words, instr(entity_and_words, '|') + 1) || ' ', \
                              ' ' || t.value || ' ') > 0) AS task_matches \
             FROM scoped \
             ORDER BY task_matches DESC, unixepoch(created_at) DESC, id DESC \
             LIMIT ?5 OFFSET ?6",
        )
        .bind(query.workspace_id.to_string())
        .bind(&terms)
        .bind(&sensitivities)
        .bind(&types)
        .bind(PAGE_SIZE)
        .bind(offset)
        .fetch_all(&mut *transaction)
        .await
        .map_err(storage)?;
        let count = rows.len();
        offset += i64::try_from(count).unwrap_or(PAGE_SIZE);
        for row in &rows {
            let stored = decode_stored_memory(row)?;
            if query.is_eligible(stored.record()).is_ok() {
                memories.push(stored);
                if memories.len() == result_limit {
                    break;
                }
            }
        }
        if count < page_size {
            break;
        }
    }
    transaction.commit().await.map_err(storage)?;
    attach_entity_links(database, &mut memories).await?;
    Ok(memories)
}

fn storage(source: sqlx::Error) -> DatabaseError {
    DatabaseError::Sqlite {
        operation: "read task memory candidates",
        source,
    }
}
