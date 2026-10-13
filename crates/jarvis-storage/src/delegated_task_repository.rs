//! Durable records for coding tasks handed to external agents (`ADR-0209`).

#![allow(missing_docs, clippy::missing_errors_doc)]

use jarvis_core::{DelegatedTaskId, UtcTimestamp};
use sqlx::Row;

use crate::{DatabaseError, SqliteDatabase};

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
    pub repository: String,
    pub task: String,
    pub allow_commands: bool,
    pub minutes: u64,
    pub status: String,
    pub branch: Option<String>,
    pub worktree: Option<String>,
    pub summary: Option<String>,
    pub result_json: Option<String>,
    pub error: Option<String>,
    pub created_at: UtcTimestamp,
    pub updated_at: UtcTimestamp,
    pub finished_at: Option<UtcTimestamp>,
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
    })
}
macro_rules! columns {
    () => {
        "id, repository, task, allow_commands, minutes, status, branch, worktree, summary, result_json, error, created_at, updated_at, finished_at"
    };
}

pub async fn create_delegated_task(
    db: &SqliteDatabase,
    new: &NewDelegatedTask<'_>,
) -> Result<StoredDelegatedTask, DatabaseError> {
    let id = DelegatedTaskId::new().to_string();
    sqlx::query("INSERT INTO delegated_tasks (id,workspace_id,repository,task,allow_commands,minutes,status,created_nanos,created_at,updated_at) VALUES (?1,?2,?3,?4,?5,?6,'queued',?7,?8,?8)")
        .bind(&id).bind(new.workspace_id).bind(new.repository).bind(new.task).bind(i64::from(new.allow_commands))
        .bind(i64::try_from(new.minutes).unwrap_or(i64::MAX)).bind(nanos(new.now)).bind(new.now.to_string())
        .execute(db.pool()).await.map_err(sqlite("create a delegated task"))?;
    find_delegated_task(db, new.workspace_id, &id).await
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
    decode(&row)
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
    rows.iter().map(decode).collect()
}

pub async fn mark_delegated_task_running(
    db: &SqliteDatabase,
    workspace: &str,
    id: &str,
    now: UtcTimestamp,
) -> Result<(), DatabaseError> {
    let changed = sqlx::query("UPDATE delegated_tasks SET status='running', updated_at=?3 WHERE workspace_id=?1 AND id=?2 AND status='queued'")
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
    sqlx::query("UPDATE delegated_tasks SET status=?3,branch=?4,worktree=?5,summary=?6,result_json=?7,error=?8,updated_at=?9,finished_at=?9 WHERE workspace_id=?1 AND id=?2 AND status IN ('queued','running')")
        .bind(workspace).bind(id).bind(finish.status).bind(finish.branch).bind(finish.worktree).bind(finish.summary).bind(finish.result_json).bind(finish.error).bind(finish.now.to_string())
        .execute(db.pool()).await.map_err(sqlite("finish a delegated task"))?;
    Ok(())
}

/// Marks work whose process belonged to an earlier daemon as interrupted. Returns how many.
pub async fn interrupt_delegated_tasks(
    db: &SqliteDatabase,
    now: UtcTimestamp,
) -> Result<u64, DatabaseError> {
    Ok(sqlx::query("UPDATE delegated_tasks SET status='interrupted',error='the daemon stopped while the coding agent was working',updated_at=?1,finished_at=?1 WHERE status IN ('queued','running')")
        .bind(now.to_string()).execute(db.pool()).await.map_err(sqlite("interrupt delegated tasks"))?.rows_affected())
}

#[cfg(test)]
#[path = "delegated_task_tests.rs"]
mod tests;
