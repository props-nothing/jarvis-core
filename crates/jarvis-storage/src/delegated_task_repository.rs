//! Durable records for coding tasks handed to external agents (`ADR-0209`).

#![allow(missing_docs, clippy::missing_errors_doc)]

use jarvis_core::{DelegatedTaskId, UtcTimestamp};
use sqlx::Row;

use crate::{DatabaseError, SqliteDatabase};

pub const MAX_TASK_ACTIVITY_RETAINED: u64 = 128;
pub const MAX_TASK_ACTIVITY_SHOWN: i64 = 32;
pub const MAX_TASK_ACTIVITY_CHARS: usize = 1600;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StoredDelegatedTaskActivity {
    pub sequence: u64,
    pub summary: String,
    pub created_at: UtcTimestamp,
}

/// A new task, before its process starts.
pub struct NewDelegatedTask<'a> {
    pub workspace_id: &'a str,
    pub repository: &'a str,
    pub task: &'a str,
    pub allow_commands: bool,
    pub minutes: u64,
    pub now: UtcTimestamp,
}

/// One stored delegated task.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StoredDelegatedTask {
    pub id: String,
    pub parent_run_id: Option<String>,
    pub origin_call_id: Option<String>,
    pub cancellation_requested_at: Option<UtcTimestamp>,
    pub repository: String,
    pub task: String,
    pub allow_commands: bool,
    pub minutes: u64,
    pub status: String,
    pub branch: Option<String>,
    pub worktree: Option<String>,
    pub summary: Option<String>,
    pub result_json: Option<String>,
    pub request_json: Option<String>,
    pub error: Option<String>,
    pub created_at: UtcTimestamp,
    pub updated_at: UtcTimestamp,
    pub finished_at: Option<UtcTimestamp>,
    pub activity: Vec<StoredDelegatedTaskActivity>,
}

fn sqlite(operation: &'static str) -> impl FnOnce(sqlx::Error) -> DatabaseError {
    move |source| DatabaseError::Sqlite { operation, source }
}
fn nanos(value: UtcTimestamp) -> i64 {
    i64::try_from(value.unix_nanos()).unwrap_or(i64::MAX)
}
fn parse_time(row: &sqlx::sqlite::SqliteRow, name: &str) -> Result<UtcTimestamp, DatabaseError> {
    let text: String = row.try_get(name).map_err(sqlite("read a delegated task"))?;
    text.parse()
        .map_err(|_| DatabaseError::StoredDelegatedTaskInvalid)
}
fn decode(row: &sqlx::sqlite::SqliteRow) -> Result<StoredDelegatedTask, DatabaseError> {
    let finished: Option<String> = row
        .try_get("finished_at")
        .map_err(sqlite("read a delegated task"))?;
    Ok(StoredDelegatedTask {
        id: row.try_get("id").map_err(sqlite("read a delegated task"))?,
        parent_run_id: row
            .try_get("parent_run_id")
            .map_err(sqlite("read task parent"))?,
        origin_call_id: row
            .try_get("origin_call_id")
            .map_err(sqlite("read task origin"))?,
        cancellation_requested_at: row
            .try_get::<Option<String>, _>("cancellation_requested_at")
            .map_err(sqlite("read task cancellation"))?
            .map(|value| {
                value
                    .parse()
                    .map_err(|_| DatabaseError::StoredDelegatedTaskInvalid)
            })
            .transpose()?,
        repository: row
            .try_get("repository")
            .map_err(sqlite("read a delegated task"))?,
        task: row
            .try_get("task")
            .map_err(sqlite("read a delegated task"))?,
        allow_commands: row
            .try_get::<i64, _>("allow_commands")
            .map_err(sqlite("read a delegated task"))?
            != 0,
        minutes: u64::try_from(
            row.try_get::<i64, _>("minutes")
                .map_err(sqlite("read a delegated task"))?,
        )
        .unwrap_or(0),
        status: row
            .try_get("status")
            .map_err(sqlite("read a delegated task"))?,
        branch: row
            .try_get("branch")
            .map_err(sqlite("read a delegated task"))?,
        worktree: row
            .try_get("worktree")
            .map_err(sqlite("read a delegated task"))?,
        summary: row
            .try_get("summary")
            .map_err(sqlite("read a delegated task"))?,
        result_json: row
            .try_get("result_json")
            .map_err(sqlite("read a delegated task"))?,
        request_json: row
            .try_get("request_json")
            .map_err(sqlite("read a delegated task"))?,
        error: row
            .try_get("error")
            .map_err(sqlite("read a delegated task"))?,
        created_at: parse_time(row, "created_at")?,
        updated_at: parse_time(row, "updated_at")?,
        finished_at: finished
            .map(|text| {
                text.parse()
                    .map_err(|_| DatabaseError::StoredDelegatedTaskInvalid)
            })
            .transpose()?,
        activity: Vec::new(),
    })
}
macro_rules! columns {
    () => {
        "id, parent_run_id, origin_call_id, cancellation_requested_at, repository, task, allow_commands, minutes, status, branch, worktree, summary, result_json, request_json, error, created_at, updated_at, finished_at"
    };
}

pub async fn create_delegated_task(
    db: &SqliteDatabase,
    new: &NewDelegatedTask<'_>,
) -> Result<StoredDelegatedTask, DatabaseError> {
    create_delegated_task_with_request(db, new, None).await
}

/// Atomically records a queued task together with its approved request before starting effects.
pub async fn create_delegated_task_with_request(
    db: &SqliteDatabase,
    new: &NewDelegatedTask<'_>,
    request: Option<&str>,
) -> Result<StoredDelegatedTask, DatabaseError> {
    create_task(db, new, request, None)
        .await
        .map(|(task, _)| task)
}

/// Creates at most one coding task per durable submitted launch call.
/// Replays return the stored identity without granting another process dispatch.
pub async fn create_delegated_task_for_call(
    db: &SqliteDatabase,
    new: &NewDelegatedTask<'_>,
    request: &str,
    call_id: &str,
) -> Result<(StoredDelegatedTask, bool), DatabaseError> {
    create_task(db, new, Some(request), Some(call_id)).await
}

async fn create_task(
    db: &SqliteDatabase,
    new: &NewDelegatedTask<'_>,
    request: Option<&str>,
    call_id: Option<&str>,
) -> Result<(StoredDelegatedTask, bool), DatabaseError> {
    let _admission = db.work_admission().admit()?;
    let mut transaction = db
        .pool()
        .begin_with("BEGIN IMMEDIATE")
        .await
        .map_err(sqlite("begin coding dispatch"))?;
    let mut parent: Option<String> = None;
    if let Some(call_id) = call_id {
        let existing: Option<(String, Option<String>)> = sqlx::query_as(
            "SELECT id,request_json FROM delegated_tasks WHERE workspace_id=?1 AND origin_call_id=?2")
            .bind(new.workspace_id).bind(call_id).fetch_optional(&mut *transaction).await.map_err(sqlite("find coding dispatch"))?;
        if let Some((id, saved)) = existing {
            if saved.as_deref() != request {
                return Err(DatabaseError::DelegationOriginRefused);
            }
            transaction
                .commit()
                .await
                .map_err(sqlite("read coding dispatch"))?;
            return Ok((find_delegated_task(db, new.workspace_id, &id).await?, false));
        }
        // Direct calls on an uncancelled completed/interrupted root remain supported.
        // Children and stopped families never acquire new host-execution authority.
        parent = sqlx::query_scalar("SELECT r.id FROM tool_calls c JOIN agent_runs r ON r.id=c.run_id WHERE c.id=?1 AND c.workspace_id=?2 AND r.workspace_id=?2 AND c.tool IN ('jarvis.code.delegate','jarvis.code.delegate_start','jarvis.code.delegate_resume') AND c.outcome='submitted' AND r.delegation_kind='none' AND r.state<>'cancelled' AND r.cancellation_requested_at IS NULL AND NOT EXISTS (SELECT 1 FROM run_family_stops s WHERE s.run_id=r.id)")
            .bind(call_id).bind(new.workspace_id).fetch_optional(&mut *transaction).await.map_err(sqlite("validate coding dispatch"))?;
        if parent.is_none() {
            return Err(DatabaseError::DelegationOriginRefused);
        }
    }
    let id = DelegatedTaskId::new().to_string();
    sqlx::query("INSERT INTO delegated_tasks (id,workspace_id,repository,task,allow_commands,minutes,request_json,status,created_nanos,created_at,updated_at,parent_run_id,origin_call_id) VALUES (?1,?2,?3,?4,?5,?6,?7,'queued',?8,?9,?9,?10,?11)")
        .bind(&id).bind(new.workspace_id).bind(new.repository).bind(new.task).bind(i64::from(new.allow_commands))
        .bind(i64::try_from(new.minutes).unwrap_or(i64::MAX)).bind(request).bind(nanos(new.now)).bind(new.now.to_string())
        .bind(parent).bind(call_id).execute(&mut *transaction).await.map_err(sqlite("create a delegated task"))?;
    transaction
        .commit()
        .await
        .map_err(sqlite("commit coding dispatch"))?;
    Ok((find_delegated_task(db, new.workspace_id, &id).await?, true))
}

/// Persists Stop even when no in-memory worker is registered. Terminal tasks are unchanged.
pub async fn request_delegated_task_cancellation(
    db: &SqliteDatabase,
    workspace: &str,
    id: &str,
    now: UtcTimestamp,
) -> Result<bool, DatabaseError> {
    let changed = sqlx::query("UPDATE delegated_tasks SET cancellation_requested_at=COALESCE(cancellation_requested_at,?3) WHERE workspace_id=?1 AND id=?2 AND status IN ('queued','running')")
        .bind(workspace).bind(id).bind(now.to_string()).execute(db.pool()).await.map_err(sqlite("request coding task cancellation"))?.rows_affected();
    if changed == 0 {
        find_delegated_task(db, workspace, id).await?;
    }
    Ok(changed != 0)
}

/// A worker polls only durable cancellation intent, not potentially large task output.
pub async fn delegated_task_cancellation_requested(
    db: &SqliteDatabase,
    workspace: &str,
    id: &str,
) -> Result<bool, DatabaseError> {
    sqlx::query_scalar("SELECT cancellation_requested_at IS NOT NULL FROM delegated_tasks WHERE workspace_id=?1 AND id=?2")
        .bind(workspace).bind(id).fetch_optional(db.pool()).await.map_err(sqlite("read coding cancellation"))?
        .ok_or(DatabaseError::DelegatedTaskNotFound)
}

pub(crate) async fn cancel_coding_family(
    transaction: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    parent: &str,
    now: UtcTimestamp,
) -> Result<u64, DatabaseError> {
    Ok(sqlx::query("UPDATE delegated_tasks SET cancellation_requested_at=COALESCE(cancellation_requested_at,?2) WHERE status IN ('queued','running') AND parent_run_id IN (SELECT id FROM agent_runs WHERE id=?1 OR parent_run_id=?1)")
        .bind(parent).bind(now.to_string()).execute(&mut **transaction).await.map_err(sqlite("cancel coding family"))?.rows_affected())
}

pub async fn find_delegated_task(
    db: &SqliteDatabase,
    workspace: &str,
    id: &str,
) -> Result<StoredDelegatedTask, DatabaseError> {
    let row = sqlx::query(concat!(
        "SELECT ",
        columns!(),
        " FROM delegated_tasks WHERE workspace_id=?1 AND id=?2"
    ))
    .bind(workspace)
    .bind(id)
    .fetch_optional(db.pool())
    .await
    .map_err(sqlite("read a delegated task"))?
    .ok_or(DatabaseError::DelegatedTaskNotFound)?;
    let mut task = decode(&row)?;
    task.activity = read_activity(db, workspace, id).await?;
    Ok(task)
}

pub async fn list_delegated_tasks(
    db: &SqliteDatabase,
    workspace: &str,
    limit: u32,
) -> Result<Vec<StoredDelegatedTask>, DatabaseError> {
    let rows = sqlx::query(concat!(
        "SELECT ",
        columns!(),
        " FROM delegated_tasks WHERE workspace_id=?1 ORDER BY created_nanos DESC LIMIT ?2"
    ))
    .bind(workspace)
    .bind(i64::from(limit))
    .fetch_all(db.pool())
    .await
    .map_err(sqlite("list delegated tasks"))?;
    let mut tasks: Vec<StoredDelegatedTask> = rows.iter().map(decode).collect::<Result<_, _>>()?;
    for task in &mut tasks {
        task.activity = read_activity(db, workspace, &task.id).await?;
    }
    Ok(tasks)
}

async fn read_activity(
    db: &SqliteDatabase,
    workspace: &str,
    id: &str,
) -> Result<Vec<StoredDelegatedTaskActivity>, DatabaseError> {
    let rows = sqlx::query("SELECT sequence,summary,created_at FROM delegated_task_activity WHERE workspace_id=?1 AND task_id=?2 ORDER BY sequence DESC LIMIT ?3")
        .bind(workspace).bind(id).bind(MAX_TASK_ACTIVITY_SHOWN)
        .fetch_all(db.pool()).await.map_err(sqlite("read delegated task activity"))?;
    rows.iter()
        .rev()
        .map(|row| {
            Ok(StoredDelegatedTaskActivity {
                sequence: u64::try_from(
                    row.try_get::<i64, _>("sequence")
                        .map_err(sqlite("read delegated task activity"))?,
                )
                .map_err(|_| DatabaseError::StoredDelegatedTaskInvalid)?,
                summary: row
                    .try_get("summary")
                    .map_err(sqlite("read delegated task activity"))?,
                created_at: parse_time(row, "created_at")?,
            })
        })
        .collect()
}

pub async fn append_delegated_task_activity(
    db: &SqliteDatabase,
    workspace: &str,
    id: &str,
    sequence: u64,
    summary: &str,
    now: UtcTimestamp,
) -> Result<bool, DatabaseError> {
    let position = i64::try_from(sequence)
        .ok()
        .filter(|position| *position > 0)
        .ok_or(DatabaseError::StoredDelegatedTaskInvalid)?;
    if summary.is_empty()
        || summary.chars().count() > MAX_TASK_ACTIVITY_CHARS
        || summary
            .chars()
            .any(|character| character.is_control() && !matches!(character, '\n' | '\t'))
    {
        return Err(DatabaseError::StoredDelegatedTaskInvalid);
    }
    let mut transaction = db
        .pool()
        .begin()
        .await
        .map_err(sqlite("record delegated task activity"))?;
    let changed = sqlx::query("INSERT INTO delegated_task_activity (task_id,workspace_id,sequence,summary,created_at) SELECT id,workspace_id,?3,?4,?5 FROM delegated_tasks WHERE workspace_id=?1 AND id=?2 AND status='running' AND ?3 > COALESCE((SELECT max(sequence) FROM delegated_task_activity WHERE task_id=?2),0)")
        .bind(workspace).bind(id).bind(position).bind(summary).bind(now.to_string())
        .execute(&mut *transaction).await.map_err(sqlite("record delegated task activity"))?.rows_affected();
    if changed == 1 {
        let before = i64::try_from(sequence.saturating_sub(MAX_TASK_ACTIVITY_RETAINED))
            .map_err(|_| DatabaseError::StoredDelegatedTaskInvalid)?;
        sqlx::query("DELETE FROM delegated_task_activity WHERE workspace_id=?1 AND task_id=?2 AND sequence<=?3")
            .bind(workspace).bind(id).bind(before).execute(&mut *transaction)
            .await.map_err(sqlite("bound delegated task activity"))?;
        sqlx::query("UPDATE delegated_tasks SET updated_at=?3 WHERE workspace_id=?1 AND id=?2")
            .bind(workspace)
            .bind(id)
            .bind(now.to_string())
            .execute(&mut *transaction)
            .await
            .map_err(sqlite("update delegated task activity time"))?;
    }
    transaction
        .commit()
        .await
        .map_err(sqlite("record delegated task activity"))?;
    Ok(changed == 1)
}

pub async fn mark_delegated_task_running(
    db: &SqliteDatabase,
    workspace: &str,
    id: &str,
    now: UtcTimestamp,
) -> Result<(), DatabaseError> {
    let changed = sqlx::query("UPDATE delegated_tasks SET status='running', updated_at=?3 WHERE workspace_id=?1 AND id=?2 AND status='queued' AND cancellation_requested_at IS NULL")
        .bind(workspace).bind(id).bind(now.to_string()).execute(db.pool()).await.map_err(sqlite("start a delegated task"))?.rows_affected();
    if changed == 1 {
        Ok(())
    } else {
        Err(DatabaseError::DelegatedTaskNotFound)
    }
}

pub struct DelegatedTaskFinish<'a> {
    pub status: &'a str,
    pub branch: Option<&'a str>,
    pub worktree: Option<&'a str>,
    pub summary: Option<&'a str>,
    pub result_json: Option<&'a str>,
    pub error: Option<&'a str>,
    pub now: UtcTimestamp,
}

pub async fn finish_delegated_task(
    db: &SqliteDatabase,
    workspace: &str,
    id: &str,
    finish: &DelegatedTaskFinish<'_>,
) -> Result<(), DatabaseError> {
    if !matches!(
        finish.status,
        "completed" | "failed" | "cancelled" | "interrupted"
    ) {
        return Err(DatabaseError::StoredDelegatedTaskInvalid);
    }
    // Stop intent cannot certify process cleanup. Keep explicit cleanup failure or
    // an unavailable execution result visible, even when cancellation was requested.
    let failed_cleanup_or_result = finish.error.is_some()
        || finish
            .result_json
            .and_then(|text| serde_json::from_str::<serde_json::Value>(text).ok())
            .is_some_and(|value| value["outcome"] == "cleanup_failed");
    let changed = sqlx::query("UPDATE delegated_tasks SET status=CASE WHEN ?10 THEN 'failed' WHEN cancellation_requested_at IS NOT NULL THEN 'cancelled' ELSE ?3 END,branch=?4,worktree=?5,summary=?6,result_json=CASE WHEN cancellation_requested_at IS NOT NULL AND NOT ?10 THEN json_set(COALESCE(?7,'{}'),'$.outcome','cancelled') ELSE ?7 END,error=?8,updated_at=?9,finished_at=?9 WHERE workspace_id=?1 AND id=?2 AND status IN ('queued','running')")
        .bind(workspace).bind(id).bind(finish.status).bind(finish.branch).bind(finish.worktree).bind(finish.summary).bind(finish.result_json).bind(finish.error).bind(finish.now.to_string())
        .bind(failed_cleanup_or_result)
        .execute(db.pool()).await.map_err(sqlite("finish a delegated task"))?.rows_affected();
    if changed == 1 {
        Ok(())
    } else {
        Err(DatabaseError::DelegatedTaskNotFound)
    }
}

/// Marks work whose process belonged to an earlier daemon as interrupted. Returns how many.
pub async fn interrupt_delegated_tasks(
    db: &SqliteDatabase,
    now: UtcTimestamp,
) -> Result<u64, DatabaseError> {
    Ok(sqlx::query("UPDATE delegated_tasks SET status=CASE WHEN cancellation_requested_at IS NOT NULL THEN 'cancelled' ELSE 'interrupted' END,error=CASE WHEN cancellation_requested_at IS NOT NULL THEN 'cancellation was requested before the daemon stopped' ELSE 'the daemon stopped while the coding agent was working' END,updated_at=?1,finished_at=?1 WHERE status IN ('queued','running')")
        .bind(now.to_string()).execute(db.pool()).await.map_err(sqlite("interrupt delegated tasks"))?.rows_affected())
}

#[cfg(test)]
#[path = "delegated_task_tests.rs"]
mod tests;
