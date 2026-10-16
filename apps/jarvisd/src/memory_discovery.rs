//! Shared typed browsing filters for owner and model memory discovery.

use jarvis_core::{MemoryId, MemoryQuery, MemoryType};
use jarvis_protocol::MemoryBrowseRequest;
use jarvis_storage::{DatabaseError, SqliteDatabase, StoredMemory, read_memory_discovery_page};

pub(crate) async fn browse(
    database: &SqliteDatabase,
    mut query: MemoryQuery,
    request: &MemoryBrowseRequest,
) -> Result<(Vec<StoredMemory>, Option<String>), DatabaseError> {
    let invalid = |field| DatabaseError::InvalidMemoryRequest { field };
    let limit = request.limit.unwrap_or(10);
    if !(1..=20).contains(&limit) {
        return Err(invalid("limit"));
    }
    if let Some(kind) = &request.memory_type {
        let kind = kind
            .parse::<MemoryType>()
            .map_err(|_| invalid("memory_type"))?;
        if !query.allowed_types.is_empty() && !query.allowed_types.contains(&kind) {
            return Err(invalid("memory_type"));
        }
        query = query.with_allowed_types(vec![kind]);
    }
    if let Some(entity) = &request.entity_id {
        query = query.with_entities(vec![entity.parse().map_err(|_| invalid("entity_id"))?]);
    }
    let cursor = request
        .cursor
        .as_ref()
        .map(|id| id.parse::<MemoryId>().map_err(|_| invalid("cursor")))
        .transpose()?;
    let mut memories = read_memory_discovery_page(database, &query, cursor, limit + 1).await?;
    let more = memories.len() > limit as usize;
    memories.truncate(limit as usize);
    let next = more
        .then(|| {
            memories
                .last()
                .map(|memory| memory.record().id().to_string())
        })
        .flatten();
    Ok((memories, next))
}
