//! Durable records of the phone calls JARVIS placed (`ADR-0200`).
//!
//! A row is written **before** the provider is asked to ring ([`create_phone_call`], status `starting`), so a request that times out
//! after the provider accepted it still leaves a record of a call that may be ringing, and the rate limit counts it. The provider's
//! identifiers are attached when it answers. A call that reached a final state (`done` or `failed`) is never changed again, so a late
//! or repeated look at the provider cannot bring a finished call back to life.

use jarvis_core::{PhoneCallId, UtcTimestamp};
use sqlx::Row;

use crate::database::{DatabaseError, SqliteDatabase};

/// The most characters of the reason a call is placed for.
pub const MAX_PHONE_REASON_CHARS: usize = 500;

/// The statuses a stored call may have, in the order a call passes through them.
const STATUSES: [&str; 6] = [
    "starting",
    "initiated",
    "in_progress",
    "processing",
    "done",
    "failed",
];

/// What a call to be placed is: who, and why.
#[derive(Clone, Debug)]
pub struct NewPhoneCall<'a> {
    /// The workspace that owns it.
    pub workspace_id: &'a str,
    /// The number being called, in international format.
    pub number: &'a str,
    /// What the call is for, as the owner approved it.
    pub reason: &'a str,
    /// Whether the caller marked it urgent (it may then ring during quiet hours).
    pub urgent: bool,
    /// When it is placed.
    pub now: UtcTimestamp,
}

/// What the provider reported about a call since it was last looked at.
#[derive(Clone, Debug, Default)]
pub struct PhoneCallUpdate<'a> {
    /// The new status word (`initiated`, `in_progress`, `processing`, `done` or `failed`).
    pub status: &'a str,
    /// Seconds the call lasted.
    pub duration_seconds: Option<u64>,
    /// Why it ended, in the provider's words.
    pub ended_because: Option<&'a str>,
    /// The provider's summary of the call.
    pub summary: Option<&'a str>,
    /// Why it failed, when it did.
    pub error: Option<&'a str>,
}

/// One stored phone call.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StoredPhoneCall {
    id: String,
    number: String,
    reason: String,
    urgent: bool,
    status: String,
    conversation_id: Option<String>,
    provider_call_id: Option<String>,
    duration_seconds: Option<u64>,
    ended_because: Option<String>,
    summary: Option<String>,
    error: Option<String>,
    created_at: UtcTimestamp,
    updated_at: UtcTimestamp,
}

impl StoredPhoneCall {
    /// The record's identifier.
    #[must_use]
    pub fn id(&self) -> &str {
        &self.id
    }

    /// The number called.
    #[must_use]
    pub fn number(&self) -> &str {
        &self.number
    }

    /// What the call was for.
    #[must_use]
    pub fn reason(&self) -> &str {
        &self.reason
    }

    /// Whether it was marked urgent.
    #[must_use]
    pub const fn urgent(&self) -> bool {
        self.urgent
    }

    /// The status word.
    #[must_use]
    pub fn status(&self) -> &str {
        &self.status
    }

    /// Whether the call may still be open (`starting`, `initiated` or `in_progress`).
    #[must_use]
    pub fn is_live(&self) -> bool {
        matches!(
            self.status.as_str(),
            "starting" | "initiated" | "in_progress"
        )
    }

    /// Whether nothing more will change.
    #[must_use]
    pub fn is_final(&self) -> bool {
        matches!(self.status.as_str(), "done" | "failed")
    }

    /// The provider's conversation identifier, once it accepted the call.
    #[must_use]
    pub fn conversation_id(&self) -> Option<&str> {
        self.conversation_id.as_deref()
    }

    /// The telephony provider's own identifier for the call.
    #[must_use]
    pub fn provider_call_id(&self) -> Option<&str> {
        self.provider_call_id.as_deref()
    }

    /// Seconds the call lasted.
    #[must_use]
    pub const fn duration_seconds(&self) -> Option<u64> {
        self.duration_seconds
    }

    /// Why it ended.
    #[must_use]
    pub fn ended_because(&self) -> Option<&str> {
        self.ended_because.as_deref()
    }

    /// The provider's summary.
    #[must_use]
    pub fn summary(&self) -> Option<&str> {
        self.summary.as_deref()
    }

    /// Why it failed.
    #[must_use]
    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    /// When it was placed.
    #[must_use]
    pub const fn created_at(&self) -> UtcTimestamp {
        self.created_at
    }

    /// When it last changed.
    #[must_use]
    pub const fn updated_at(&self) -> UtcTimestamp {
        self.updated_at
    }
}

macro_rules! columns {
    () => {
        "id, number, reason, urgent, status, conversation_id, provider_call_id, duration_seconds, ended_because, summary, error, \
         created_at, updated_at"
    };
}

fn nanos(value: UtcTimestamp) -> i64 {
    i64::try_from(value.unix_nanos()).unwrap_or(i64::MAX)
}

fn sqlite(operation: &'static str) -> impl FnOnce(sqlx::Error) -> DatabaseError {
    move |source| DatabaseError::Sqlite { operation, source }
}

/// Records a call about to be placed, with the status `starting`.
///
/// # Errors
///
/// Returns [`DatabaseError::InvalidPhoneCall`] for a blank number or reason or one longer than allowed, and [`DatabaseError::Sqlite`]
/// when the write fails.
pub async fn create_phone_call(
    database: &SqliteDatabase,
    call: &NewPhoneCall<'_>,
) -> Result<StoredPhoneCall, DatabaseError> {
    let reason = call.reason.trim();
    if reason.is_empty() || reason.chars().count() > MAX_PHONE_REASON_CHARS {
        return Err(DatabaseError::InvalidPhoneCall { field: "reason" });
    }
    if !(8..=20).contains(&call.number.len()) {
        return Err(DatabaseError::InvalidPhoneCall { field: "number" });
    }
    let id = PhoneCallId::new().to_string();
    sqlx::query(
        "INSERT INTO phone_calls (id, workspace_id, direction, provider, number, reason, urgent, status, created_nanos, \
            created_at, updated_at) \
         VALUES (?1, ?2, 'outbound', 'elevenlabs', ?3, ?4, ?5, 'starting', ?6, ?7, ?7)",
    )
    .bind(&id)
    .bind(call.workspace_id)
    .bind(call.number)
    .bind(reason)
    .bind(i64::from(call.urgent))
    .bind(nanos(call.now))
    .bind(call.now.to_string())
    .execute(database.pool())
    .await
    .map_err(sqlite("record a phone call"))?;
    find_phone_call(database, call.workspace_id, &id).await
}

/// Reads one call, scoped to its workspace.
///
/// # Errors
///
/// Returns [`DatabaseError::PhoneCallNotFound`] when no call has that identifier in this workspace.
pub async fn find_phone_call(
    database: &SqliteDatabase,
    workspace_id: &str,
    id: &str,
) -> Result<StoredPhoneCall, DatabaseError> {
    let row = sqlx::query(concat!(
        "SELECT ",
        columns!(),
        " FROM phone_calls WHERE id = ?1 AND workspace_id = ?2"
    ))
    .bind(id)
    .bind(workspace_id)
    .fetch_optional(database.pool())
    .await
    .map_err(sqlite("read a phone call"))?
    .ok_or(DatabaseError::PhoneCallNotFound)?;
    decode(&row)
}

/// Lists a workspace's calls, newest first, at most `limit`.
///
/// # Errors
///
/// Returns [`DatabaseError::StoredPhoneCallInvalid`] when a stored row cannot be decoded.
pub async fn list_phone_calls(
    database: &SqliteDatabase,
    workspace_id: &str,
    limit: u32,
) -> Result<Vec<StoredPhoneCall>, DatabaseError> {
    let rows = sqlx::query(concat!(
        "SELECT ",
        columns!(),
        " FROM phone_calls WHERE workspace_id = ?1 ORDER BY created_nanos DESC, id DESC LIMIT ?2"
    ))
    .bind(workspace_id)
    .bind(i64::from(limit))
    .fetch_all(database.pool())
    .await
    .map_err(sqlite("list phone calls"))?;
    rows.iter().map(decode).collect()
}

/// The calls that may still change, oldest first: the ones the poller has to look at.
///
/// # Errors
///
/// Returns [`DatabaseError::StoredPhoneCallInvalid`] when a stored row cannot be decoded.
pub async fn unsettled_phone_calls(
    database: &SqliteDatabase,
    workspace_id: &str,
) -> Result<Vec<StoredPhoneCall>, DatabaseError> {
    let rows = sqlx::query(concat!(
        "SELECT ",
        columns!(),
        " FROM phone_calls WHERE workspace_id = ?1 \
         AND status IN ('starting', 'initiated', 'in_progress', 'processing') ORDER BY created_nanos ASC, id ASC"
    ))
    .bind(workspace_id)
    .fetch_all(database.pool())
    .await
    .map_err(sqlite("list unsettled phone calls"))?;
    rows.iter().map(decode).collect()
}

/// How many calls were placed at or after `since`, counting every one that was started, whatever became of it.
///
/// # Errors
///
/// Returns [`DatabaseError::Sqlite`] when the read fails.
pub async fn count_phone_calls_since(
    database: &SqliteDatabase,
    workspace_id: &str,
    since: UtcTimestamp,
) -> Result<u32, DatabaseError> {
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM phone_calls WHERE workspace_id = ?1 AND created_nanos >= ?2",
    )
    .bind(workspace_id)
    .bind(nanos(since))
    .fetch_one(database.pool())
    .await
    .map_err(sqlite("count phone calls"))?;
    Ok(u32::try_from(count).unwrap_or(u32::MAX))
}

/// Attaches the provider's identifiers once it accepted the call, and moves it to `initiated`.
///
/// # Errors
///
/// Returns [`DatabaseError::PhoneCallNotFound`] when no such call exists, and [`DatabaseError::Sqlite`] when the write fails.
pub async fn attach_provider_call(
    database: &SqliteDatabase,
    workspace_id: &str,
    id: &str,
    conversation_id: &str,
    provider_call_id: Option<&str>,
    now: UtcTimestamp,
) -> Result<(), DatabaseError> {
    let result = sqlx::query(
        "UPDATE phone_calls SET conversation_id = ?3, provider_call_id = ?4, status = 'initiated', updated_at = ?5 \
         WHERE id = ?1 AND workspace_id = ?2 AND status = 'starting'",
    )
    .bind(id)
    .bind(workspace_id)
    .bind(conversation_id)
    .bind(provider_call_id)
    .bind(now.to_string())
    .execute(database.pool())
    .await
    .map_err(sqlite("attach a provider call"))?;
    if result.rows_affected() == 0 {
        // Either it does not exist or it already moved on; the first is the error worth reporting.
        find_phone_call(database, workspace_id, id).await?;
    }
    Ok(())
}

/// Records what the provider reported. A call that is already `done` or `failed` is left exactly as it is, and an update never moves a
/// call backwards (`in_progress` never replaces `processing`).
///
/// Returns whether the stored call changed.
///
/// # Errors
///
/// Returns [`DatabaseError::InvalidPhoneCall`] for an unknown status word and [`DatabaseError::Sqlite`] when the write fails.
pub async fn update_phone_call(
    database: &SqliteDatabase,
    workspace_id: &str,
    id: &str,
    update: &PhoneCallUpdate<'_>,
    now: UtcTimestamp,
) -> Result<bool, DatabaseError> {
    let Some(rank) = STATUSES.iter().position(|word| *word == update.status) else {
        return Err(DatabaseError::InvalidPhoneCall { field: "status" });
    };
    let result = sqlx::query(
        "UPDATE phone_calls SET status = ?3, duration_seconds = COALESCE(?4, duration_seconds), \
            ended_because = COALESCE(?5, ended_because), summary = COALESCE(?6, summary), error = COALESCE(?7, error), \
            updated_at = ?8 \
         WHERE id = ?1 AND workspace_id = ?2 AND status NOT IN ('done', 'failed') \
           AND (CASE status WHEN 'starting' THEN 0 WHEN 'initiated' THEN 1 WHEN 'in_progress' THEN 2 WHEN 'processing' THEN 3 ELSE 4 END) <= ?9",
    )
    .bind(id)
    .bind(workspace_id)
    .bind(update.status)
    .bind(update.duration_seconds.and_then(|seconds| i64::try_from(seconds).ok()))
    .bind(update.ended_because)
    .bind(update.summary)
    .bind(update.error)
    .bind(now.to_string())
    .bind(i64::try_from(rank).unwrap_or(i64::MAX))
    .execute(database.pool())
    .await
    .map_err(sqlite("update a phone call"))?;
    Ok(result.rows_affected() > 0)
}

/// Removes every call record of a workspace and returns how many there were.
///
/// # Errors
///
/// Returns [`DatabaseError::Sqlite`] when the write fails.
pub async fn delete_phone_calls(
    database: &SqliteDatabase,
    workspace_id: &str,
) -> Result<u64, DatabaseError> {
    sqlx::query("DELETE FROM phone_calls WHERE workspace_id = ?1")
        .bind(workspace_id)
        .execute(database.pool())
        .await
        .map(|result| result.rows_affected())
        .map_err(sqlite("delete phone calls"))
}

/// Removes the records placed before `before`, and returns how many.
///
/// # Errors
///
/// Returns [`DatabaseError::Sqlite`] when the write fails.
pub async fn prune_phone_calls(
    database: &SqliteDatabase,
    before: UtcTimestamp,
) -> Result<u64, DatabaseError> {
    sqlx::query("DELETE FROM phone_calls WHERE created_nanos < ?1")
        .bind(nanos(before))
        .execute(database.pool())
        .await
        .map(|result| result.rows_affected())
        .map_err(sqlite("prune phone calls"))
}

fn decode(row: &sqlx::sqlite::SqliteRow) -> Result<StoredPhoneCall, DatabaseError> {
    let invalid = |field: &'static str| DatabaseError::StoredPhoneCallInvalid { field };
    let text = |field: &'static str| row.try_get::<String, _>(field).map_err(|_| invalid(field));
    let optional = |field: &'static str| {
        row.try_get::<Option<String>, _>(field)
            .map_err(|_| invalid(field))
    };
    let timestamp = |field: &'static str| {
        text(field)?
            .parse::<UtcTimestamp>()
            .map_err(|_| invalid(field))
    };
    let status = text("status")?;
    if !STATUSES.contains(&status.as_str()) {
        return Err(invalid("status"));
    }
    Ok(StoredPhoneCall {
        id: text("id")?,
        number: text("number")?,
        reason: text("reason")?,
        urgent: row
            .try_get::<i64, _>("urgent")
            .map_err(|_| invalid("urgent"))?
            == 1,
        status,
        conversation_id: optional("conversation_id")?,
        provider_call_id: optional("provider_call_id")?,
        duration_seconds: row
            .try_get::<Option<i64>, _>("duration_seconds")
            .map_err(|_| invalid("duration_seconds"))?
            .and_then(|seconds| u64::try_from(seconds).ok()),
        ended_because: optional("ended_because")?,
        summary: optional("summary")?,
        error: optional("error")?,
        created_at: timestamp("created_at")?,
        updated_at: timestamp("updated_at")?,
    })
}

#[cfg(test)]
#[path = "phone_call_tests.rs"]
mod tests;
