//! Durable storage for triggers, and the two operations that must not repeat: claiming a poll and claiming an event.
//!
//! # Two guarded writes
//!
//! * [`claim_due_triggers`] advances a trigger's next poll in one guarded `UPDATE` (`WHERE id = ? AND next_poll_nanos = <what it read>`)
//!   *before* the poll happens, so two passes cannot both poll it and a poll lost to a crash is simply the next one.
//! * [`claim_trigger_event`] inserts the event key under the primary key and reports whether this caller was first. A run starts only
//!   for the caller that was, so a message listed again, a restart, or a second pass starts nothing.
//!
//! An event is claimed **before** its run starts, which makes the guarantee *at most once*: a crash in between loses one event and never
//! doubles one, the same direction a schedule fire takes (`ADR-0132`).

use jarvis_core::{TriggerId, UtcTimestamp};
use sqlx::Row;

use crate::database::{DatabaseError, SqliteDatabase};

/// The most triggers one workspace may hold.
pub const MAX_TRIGGERS_PER_WORKSPACE: usize = 20;

/// The longest Gmail query a trigger may carry.
pub const MAX_TRIGGER_QUERY_CHARS: usize = 300;

/// The longest objective a trigger may carry: the run's framing and the message id are added to it.
pub const MAX_TRIGGER_OBJECTIVE_CHARS: usize = 2000;

/// The shortest interval between two polls, in seconds.
pub const MIN_TRIGGER_INTERVAL_SECONDS: u64 = 60;

/// The longest interval between two polls, in seconds (one day).
pub const MAX_TRIGGER_INTERVAL_SECONDS: u64 = 86_400;

/// What a new trigger is made from.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NewTrigger {
    /// The Gmail search, as typed in Gmail's search box.
    pub query: String,
    /// What each run is asked to do about one matching message.
    pub objective: String,
    /// Seconds between polls.
    pub interval_seconds: u64,
    /// The project every run belongs to, by identifier.
    pub project_id: Option<String>,
}

/// One stored trigger.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StoredTrigger {
    /// The trigger's identifier.
    pub id: String,
    /// The workspace that owns it.
    pub workspace_id: String,
    /// The kind of event it waits for (`mail`).
    pub kind: String,
    /// The Gmail search it polls.
    pub query: String,
    /// What each run is asked to do.
    pub objective: String,
    /// Seconds between polls.
    pub interval_seconds: u64,
    /// The project its runs belong to.
    pub project_id: Option<String>,
    /// Whether it polls.
    pub enabled: bool,
    /// When it polls next.
    pub next_poll: UtcTimestamp,
    /// Whether the messages that already matched were recorded, so only later ones start runs.
    pub baselined: bool,
    /// When it last polled.
    pub last_polled_at: Option<UtcTimestamp>,
    /// Why the last poll failed, in a fixed sentence; empty after a poll that worked.
    pub last_error: Option<String>,
    /// The most recent run it started.
    pub last_run_id: Option<String>,
    /// How many runs it has started.
    pub fire_count: u32,
    /// When it was created.
    pub created_at: UtcTimestamp,
}

macro_rules! columns {
    () => {
        "id, workspace_id, kind, query, objective, interval_seconds, project_id, enabled, next_poll_nanos, baselined, \
         last_polled_at, last_error, last_run_id, fire_count, created_at"
    };
}

fn invalid(field: &'static str) -> DatabaseError {
    DatabaseError::InvalidTrigger { field }
}

fn nanos(value: UtcTimestamp) -> i64 {
    i64::try_from(value.unix_nanos()).unwrap_or(i64::MAX)
}

/// Creates a trigger that first polls one interval from now.
///
/// # Errors
///
/// Returns [`DatabaseError::InvalidTrigger`] for a blank or over-long query or objective, a control character, or an interval outside
/// the allowed range, and [`DatabaseError::TriggerLimit`] when the workspace holds the maximum.
pub async fn create_trigger(
    database: &SqliteDatabase,
    workspace_id: &str,
    new: &NewTrigger,
    now: UtcTimestamp,
) -> Result<StoredTrigger, DatabaseError> {
    let query = new.query.trim();
    if query.is_empty()
        || query.chars().count() > MAX_TRIGGER_QUERY_CHARS
        || query.chars().any(char::is_control)
    {
        return Err(invalid("query"));
    }
    let objective = new.objective.trim();
    if objective.is_empty()
        || objective.chars().count() > MAX_TRIGGER_OBJECTIVE_CHARS
        || objective.contains('\0')
    {
        return Err(invalid("objective"));
    }
    if !(MIN_TRIGGER_INTERVAL_SECONDS..=MAX_TRIGGER_INTERVAL_SECONDS)
        .contains(&new.interval_seconds)
    {
        return Err(invalid("interval"));
    }
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM triggers WHERE workspace_id = ?1")
        .bind(workspace_id)
        .fetch_one(database.pool())
        .await
        .map_err(|source| DatabaseError::Sqlite {
            operation: "count a workspace's triggers",
            source,
        })?;
    if usize::try_from(count).unwrap_or(usize::MAX) >= MAX_TRIGGERS_PER_WORKSPACE {
        return Err(DatabaseError::TriggerLimit);
    }
    let id = TriggerId::new().to_string();
    let interval = i64::try_from(new.interval_seconds).unwrap_or(i64::MAX);
    let first_poll = nanos(now).saturating_add(interval.saturating_mul(1_000_000_000));
    sqlx::query(
        "INSERT INTO triggers (id, workspace_id, kind, query, objective, interval_seconds, project_id, enabled, \
            next_poll_nanos, baselined, fire_count, created_at) \
         VALUES (?1, ?2, 'mail', ?3, ?4, ?5, ?6, 1, ?7, 0, 0, ?8)",
    )
    .bind(&id)
    .bind(workspace_id)
    .bind(query)
    .bind(objective)
    .bind(interval)
    .bind(new.project_id.as_deref())
    .bind(first_poll)
    .bind(now.to_string())
    .execute(database.pool())
    .await
    .map_err(|source| DatabaseError::Sqlite {
        operation: "create a trigger",
        source,
    })?;
    find_trigger(database, workspace_id, &id).await
}

/// Reads one trigger, scoped to its workspace.
///
/// # Errors
///
/// Returns [`DatabaseError::TriggerNotFound`] when none has that identifier in this workspace.
pub async fn find_trigger(
    database: &SqliteDatabase,
    workspace_id: &str,
    id: &str,
) -> Result<StoredTrigger, DatabaseError> {
    let row = sqlx::query(concat!(
        "SELECT ",
        columns!(),
        " FROM triggers WHERE id = ?1 AND workspace_id = ?2"
    ))
    .bind(id)
    .bind(workspace_id)
    .fetch_optional(database.pool())
    .await
    .map_err(|source| DatabaseError::Sqlite {
        operation: "read a trigger",
        source,
    })?
    .ok_or(DatabaseError::TriggerNotFound)?;
    decode(&row)
}

/// Lists a workspace's triggers, oldest first.
///
/// # Errors
///
/// Returns [`DatabaseError`] when the read or a decode fails.
pub async fn list_triggers(
    database: &SqliteDatabase,
    workspace_id: &str,
) -> Result<Vec<StoredTrigger>, DatabaseError> {
    let rows = sqlx::query(concat!(
        "SELECT ",
        columns!(),
        " FROM triggers WHERE workspace_id = ?1 ORDER BY created_at ASC, id ASC"
    ))
    .bind(workspace_id)
    .fetch_all(database.pool())
    .await
    .map_err(|source| DatabaseError::Sqlite {
        operation: "list triggers",
        source,
    })?;
    rows.iter().map(decode).collect()
}

/// Removes a trigger and, with it, its dedupe record.
///
/// # Errors
///
/// Returns [`DatabaseError::TriggerNotFound`] when there is no such trigger in this workspace.
pub async fn delete_trigger(
    database: &SqliteDatabase,
    workspace_id: &str,
    id: &str,
) -> Result<(), DatabaseError> {
    let found = find_trigger(database, workspace_id, id).await?;
    // Both statements are explicit so the removal reads as what it is: the history goes with the trigger.
    for statement in [
        "DELETE FROM trigger_events WHERE trigger_id = ?1",
        "DELETE FROM triggers WHERE id = ?1",
    ] {
        sqlx::query(statement)
            .bind(&found.id)
            .execute(database.pool())
            .await
            .map_err(|source| DatabaseError::Sqlite {
                operation: "delete a trigger",
                source,
            })?;
    }
    Ok(())
}

/// Pauses or resumes a trigger. Resuming polls one interval from now and records the messages that arrived while it was paused as the
/// baseline again, so a pause is never followed by a burst of runs.
///
/// # Errors
///
/// Returns [`DatabaseError::TriggerNotFound`] when there is no such trigger in this workspace.
pub async fn set_trigger_enabled(
    database: &SqliteDatabase,
    workspace_id: &str,
    id: &str,
    enabled: bool,
    now: UtcTimestamp,
) -> Result<StoredTrigger, DatabaseError> {
    let found = find_trigger(database, workspace_id, id).await?;
    if enabled {
        let interval = i64::try_from(found.interval_seconds).unwrap_or(i64::MAX);
        let next = nanos(now).saturating_add(interval.saturating_mul(1_000_000_000));
        sqlx::query(
            "UPDATE triggers SET enabled = 1, baselined = 0, next_poll_nanos = ?3 WHERE id = ?1 AND workspace_id = ?2",
        )
        .bind(id)
        .bind(workspace_id)
        .bind(next)
        .execute(database.pool())
        .await
    } else {
        sqlx::query("UPDATE triggers SET enabled = 0 WHERE id = ?1 AND workspace_id = ?2")
            .bind(id)
            .bind(workspace_id)
            .execute(database.pool())
            .await
    }
    .map_err(|source| DatabaseError::Sqlite {
        operation: "pause or resume a trigger",
        source,
    })?;
    find_trigger(database, workspace_id, id).await
}

/// Claims every trigger whose poll is due, advancing each one's next poll, and returns the claimed triggers.
///
/// # Errors
///
/// Returns [`DatabaseError`] when a read, a decode or a write fails.
pub async fn claim_due_triggers(
    database: &SqliteDatabase,
    now: UtcTimestamp,
    limit: u32,
) -> Result<Vec<StoredTrigger>, DatabaseError> {
    let rows = sqlx::query(concat!(
        "SELECT ",
        columns!(),
        " FROM triggers WHERE enabled = 1 AND next_poll_nanos <= ?1 ORDER BY next_poll_nanos ASC, id ASC LIMIT ?2"
    ))
    .bind(nanos(now))
    .bind(i64::from(limit))
    .fetch_all(database.pool())
    .await
    .map_err(|source| DatabaseError::Sqlite {
        operation: "find due triggers",
        source,
    })?;
    let mut claimed = Vec::with_capacity(rows.len());
    for row in &rows {
        let trigger = decode(row)?;
        let interval = i64::try_from(trigger.interval_seconds).unwrap_or(i64::MAX);
        let next = nanos(now).saturating_add(interval.saturating_mul(1_000_000_000));
        let result = sqlx::query(
            "UPDATE triggers SET next_poll_nanos = ?3 WHERE id = ?1 AND next_poll_nanos = ?2 AND enabled = 1",
        )
        .bind(&trigger.id)
        .bind(nanos(trigger.next_poll))
        .bind(next)
        .execute(database.pool())
        .await
        .map_err(|source| DatabaseError::Sqlite {
            operation: "claim a trigger poll",
            source,
        })?;
        if result.rows_affected() == 1 {
            claimed.push(trigger);
        }
    }
    Ok(claimed)
}

/// Claims one event for a trigger. `true` means this caller is the first to see it and may start its run; `false` means it was
/// already claimed (a replay), and nothing must start.
///
/// # Errors
///
/// Returns [`DatabaseError::InvalidTrigger`] for an empty or over-long key, and [`DatabaseError::Sqlite`] when the write fails.
pub async fn claim_trigger_event(
    database: &SqliteDatabase,
    trigger_id: &str,
    event_key: &str,
    now: UtcTimestamp,
) -> Result<bool, DatabaseError> {
    if event_key.is_empty() || event_key.len() > 128 {
        return Err(invalid("event_key"));
    }
    let result = sqlx::query(
        "INSERT OR IGNORE INTO trigger_events (trigger_id, event_key, run_id, seen_nanos) VALUES (?1, ?2, NULL, ?3)",
    )
    .bind(trigger_id)
    .bind(event_key)
    .bind(nanos(now))
    .execute(database.pool())
    .await
    .map_err(|source| DatabaseError::Sqlite {
        operation: "claim a trigger event",
        source,
    })?;
    Ok(result.rows_affected() == 1)
}

/// Records that a claimed event started a run.
///
/// # Errors
///
/// Returns [`DatabaseError::Sqlite`] when the write fails.
pub async fn record_trigger_run(
    database: &SqliteDatabase,
    trigger_id: &str,
    event_key: &str,
    run_id: &str,
) -> Result<(), DatabaseError> {
    for (statement, operation) in [
        (
            "UPDATE trigger_events SET run_id = ?3 WHERE trigger_id = ?1 AND event_key = ?2",
            "record a trigger event's run",
        ),
        (
            "UPDATE triggers SET last_run_id = ?3, fire_count = fire_count + 1 WHERE id = ?1 AND ?2 IS NOT NULL",
            "record a trigger's run",
        ),
    ] {
        sqlx::query(statement)
            .bind(trigger_id)
            .bind(event_key)
            .bind(run_id)
            .execute(database.pool())
            .await
            .map_err(|source| DatabaseError::Sqlite { operation, source })?;
    }
    Ok(())
}

/// Records one poll: when it happened, and why it failed, if it did. A poll that worked clears the earlier reason.
///
/// # Errors
///
/// Returns [`DatabaseError::Sqlite`] when the write fails.
pub async fn record_trigger_poll(
    database: &SqliteDatabase,
    trigger_id: &str,
    now: UtcTimestamp,
    error: Option<&str>,
) -> Result<(), DatabaseError> {
    let error: Option<String> = error.map(|text| text.chars().take(300).collect());
    sqlx::query("UPDATE triggers SET last_polled_at = ?2, last_error = ?3 WHERE id = ?1")
        .bind(trigger_id)
        .bind(now.to_string())
        .bind(error)
        .execute(database.pool())
        .await
        .map_err(|source| DatabaseError::Sqlite {
            operation: "record a trigger poll",
            source,
        })?;
    Ok(())
}

/// Marks a trigger as baselined: the messages that already matched were recorded, so only later ones start runs.
///
/// # Errors
///
/// Returns [`DatabaseError::Sqlite`] when the write fails.
pub async fn mark_trigger_baselined(
    database: &SqliteDatabase,
    trigger_id: &str,
) -> Result<(), DatabaseError> {
    sqlx::query("UPDATE triggers SET baselined = 1 WHERE id = ?1")
        .bind(trigger_id)
        .execute(database.pool())
        .await
        .map_err(|source| DatabaseError::Sqlite {
            operation: "baseline a trigger",
            source,
        })?;
    Ok(())
}

/// How many runs a trigger started since `since`: what its daily cap counts.
///
/// # Errors
///
/// Returns [`DatabaseError::Sqlite`] when the read fails.
pub async fn count_trigger_runs_since(
    database: &SqliteDatabase,
    trigger_id: &str,
    since: UtcTimestamp,
) -> Result<u32, DatabaseError> {
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM trigger_events WHERE trigger_id = ?1 AND run_id IS NOT NULL AND seen_nanos >= ?2",
    )
    .bind(trigger_id)
    .bind(nanos(since))
    .fetch_one(database.pool())
    .await
    .map_err(|source| DatabaseError::Sqlite {
        operation: "count a trigger's runs",
        source,
    })?;
    Ok(u32::try_from(count).unwrap_or(u32::MAX))
}

fn decode(row: &sqlx::sqlite::SqliteRow) -> Result<StoredTrigger, DatabaseError> {
    let bad = |field: &'static str| DatabaseError::StoredScheduleInvalid { field };
    let text = |field: &'static str| row.try_get::<String, _>(field).map_err(|_| bad(field));
    let optional = |field: &'static str| {
        row.try_get::<Option<String>, _>(field)
            .map_err(|_| bad(field))
    };
    let integer = |field: &'static str| row.try_get::<i64, _>(field).map_err(|_| bad(field));
    let stamp =
        |value: &str, field: &'static str| value.parse::<UtcTimestamp>().map_err(|_| bad(field));
    Ok(StoredTrigger {
        id: text("id")?,
        workspace_id: text("workspace_id")?,
        kind: text("kind")?,
        query: text("query")?,
        objective: text("objective")?,
        interval_seconds: u64::try_from(integer("interval_seconds")?)
            .map_err(|_| bad("interval_seconds"))?,
        project_id: optional("project_id")?,
        enabled: integer("enabled")? == 1,
        next_poll: UtcTimestamp::from_unix_nanos(i128::from(integer("next_poll_nanos")?))
            .map_err(|_| bad("next_poll_nanos"))?,
        baselined: integer("baselined")? == 1,
        last_polled_at: optional("last_polled_at")?
            .map(|value| stamp(&value, "last_polled_at"))
            .transpose()?,
        last_error: optional("last_error")?,
        last_run_id: optional("last_run_id")?,
        fire_count: u32::try_from(integer("fire_count")?).map_err(|_| bad("fire_count"))?,
        created_at: stamp(&text("created_at")?, "created_at")?,
    })
}

#[cfg(test)]
#[path = "trigger_tests.rs"]
mod tests;
