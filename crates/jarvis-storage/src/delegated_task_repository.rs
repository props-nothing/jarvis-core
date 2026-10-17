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
        "id, repository, task, allow_commands, minutes, status, branch, worktree, summary, result_json, request_json, error, created_at, updated_at, finished_at"
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
    let _admission = db.work_admission().admit()?;
    let id = DelegatedTaskId::new().to_string();
    sqlx::query("INSERT INTO delegated_tasks (id,workspace_id,repository,task,allow_commands,minutes,request_json,status,created_nanos,created_at,updated_at) VALUES (?1,?2,?3,?4,?5,?6,?7,'queued',?8,?9,?9)")
        .bind(&id).bind(new.workspace_id).bind(new.repository).bind(new.task).bind(i64::from(new.allow_commands))
        .bind(i64::try_from(new.minutes).unwrap_or(i64::MAX)).bind(request).bind(nanos(new.now)).bind(new.now.to_string())
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
    let changed = sqlx::query("UPDATE delegated_tasks SET status=?3,branch=?4,worktree=?5,summary=?6,result_json=?7,error=?8,updated_at=?9,finished_at=?9 WHERE workspace_id=?1 AND id=?2 AND status IN ('queued','running')")
        .bind(workspace).bind(id).bind(finish.status).bind(finish.branch).bind(finish.worktree).bind(finish.summary).bind(finish.result_json).bind(finish.error).bind(finish.now.to_string())
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
    Ok(sqlx::query("UPDATE delegated_tasks SET status='interrupted',error='the daemon stopped while the coding agent was working',updated_at=?1,finished_at=?1 WHERE status IN ('queued','running')")
        .bind(now.to_string()).execute(db.pool()).await.map_err(sqlite("interrupt delegated tasks"))?.rows_affected())
}

#[cfg(test)]
#[path = "delegated_task_tests.rs"]
mod tests;
