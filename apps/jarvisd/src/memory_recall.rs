//! Task-aware native recall. Eligibility precedes ranking; provenance remains fenced data.

use jarvis_core::{
    MemoryConfidence, MemoryQuery, MemoryTrust, RetrievedMemory, SIGNAL_WEIGHTS, ScoringContext,
    Sensitivity, UtcTimestamp, WorkspaceId, memory_query_terms, rank,
};
use jarvis_storage::{DatabaseError, SqliteDatabase, StoredMemory, read_task_memory_candidates};
use serde_json::{Value, json};

const CANDIDATES: u32 = 128;
const SELECTED: usize = 24;
const QUERY_FILLER: &str = "a about an and are as at be been can could did do does for from has have \
    how i in is it its me my of on or our please should tell that the their them these they this \
    to us was we were what when where which who why will with would you your aan als bij dat de \
    deze die dit een en er heeft heb hebben het hoe ik je kun kan met mij mijn om ons onze op \
    over van vertel voor waar wanneer wat weet welke wie wij ze zijn";

pub(crate) struct MemoryRecall {
    pub(crate) memories: Vec<RetrievedMemory>,
    pub(crate) summary: Value,
}

pub(crate) fn query_for(
    workspace: WorkspaceId,
    now: UtcTimestamp,
    destination: Sensitivity,
) -> MemoryQuery {
    MemoryQuery::new(workspace, now)
        .with_destination(destination)
        .with_allowed_types(crate::executor::MODEL_MEMORY_TYPES.to_vec())
        .with_minimum_trust(MemoryTrust::Untrusted)
        .with_minimum_confidence(MemoryConfidence::Unverified)
}

pub(crate) async fn recall(
    database: &SqliteDatabase,
    workspace: WorkspaceId,
    objective: &str,
    now: UtcTimestamp,
    destination: Sensitivity,
) -> Result<MemoryRecall, DatabaseError> {
    let query = query_for(workspace, now, destination).with_text(task_text(objective));
    let records = read_task_memory_candidates(database, &query, CANDIDATES)
        .await?
        .into_iter()
        .map(StoredMemory::into_record)
        .collect::<Vec<_>>();
    let selection = rank(
        &records,
        &query,
        &ScoringContext::without_semantics(),
        &SIGNAL_WEIGHTS,
    );
    let mut ranked = selection.scored().iter().collect::<Vec<_>>();
    // Task matches have their own tier: an unrelated recent claim cannot displace an old answer.
    // Within a tier the existing multi-signal ranking, including importance and currency, applies.
    ranked.sort_by(|left, right| {
        right
            .signals()
            .keyword
            .cmp(&left.signals().keyword)
            .then_with(|| right.total().cmp(&left.total()))
            .then_with(|| left.record().id().cmp(&right.record().id()))
    });
    let mut memories = Vec::new();
    let mut selected = Vec::new();
    for scored in ranked {
        let Ok(memory) = RetrievedMemory::with_provenance(
            scored.record(),
            &crate::executor::MODEL_MEMORY_TYPES,
            now,
        ) else {
            continue;
        };
        selected.push(json!({
            "reference": memory.reference(),
            "reason": if scored.signals().keyword > 0 { "task_match" } else { "context_fallback" },
            "keyword": scored.signals().keyword,
            "score": scored.total(),
        }));
        memories.push(memory);
        if memories.len() == SELECTED {
            break;
        }
    }
    Ok(MemoryRecall {
        memories,
        summary: json!({
            "strategy": "task_lexical_v1",
            "considered": records.len(),
            "selected": selected,
            "candidate_limit": CANDIDATES,
            "result_limit": SELECTED,
        }),
    })
}

fn task_text(objective: &str) -> String {
    let mut terms = Vec::new();
    for term in objective.split_whitespace().flat_map(memory_query_terms) {
        // Query-only English/Dutch question glue, never a change to canonical memory or its keys.
        let filler = QUERY_FILLER
            .split_ascii_whitespace()
            .any(|word| word == term);
        if !filler && !terms.contains(&term) {
            terms.push(term);
            if terms.len() == jarvis_core::MAX_SEARCH_KEY_WORDS {
                break;
            }
        }
    }
    terms.join(" ")
}

#[cfg(test)]
#[path = "memory_recall_tests.rs"]
mod tests;
