//! Work whose current execution would be interrupted by stopping this daemon.

use crate::{DatabaseError, SqliteDatabase};
use jarvis_core::UtcTimestamp;

/// Settles received runs without drivers only after admission is closed and idle.
/// Executing runs and coding tasks retain their own settlement/cleanup authority.
///
/// # Errors
/// Unreadable admission, run or event state cannot certify settlement.
pub async fn settle_unstarted_shutdown_runs(
    database: &SqliteDatabase,
    now: UtcTimestamp,
) -> Result<(), DatabaseError> {
    use jarvis_core::{CorrelationId, RunEventKind, RunEventPayload, RunId, RunTransition};
    if !database.work_admission().is_drained()? {
        return Ok(());
    }
    let ids: Vec<String> = sqlx::query_scalar("SELECT id FROM agent_runs WHERE state='received' AND cancellation_requested_at IS NOT NULL")
        .fetch_all(database.pool()).await.map_err(|source|DatabaseError::Sqlite {operation:"read unstarted shutdown runs",source})?;
    for id in ids {
        let run = crate::find_run(database, &id).await?;
        if run.state() != jarvis_core::RunState::Received {
            continue;
        }
        let event = crate::NewRunEvent::new(
            RunId::new().to_string(),
            &id,
            RunEventKind::RunCancelled,
            None,
            RunEventPayload::new(r#"{"outcome":"cancelled","while":"shutdown_before_execution"}"#)
                .map_err(|_| DatabaseError::InvalidRunEventRequest { field: "payload" })?,
            CorrelationId::new(),
            now,
        )?;
        let transition =
            crate::TerminalTransition::new(run.expectation(), RunTransition::cancelled(), &event)?;
        match crate::settle_run(database, &transition).await {
            Ok(_) | Err(DatabaseError::RunConflict) => {}
            Err(error) => return Err(error),
        }
    }
    Ok(())
}

/// Records graceful-shutdown intent without cancelling durable approval waits.
/// Callers must close work admission first. Effects already submitted retain their
/// real outcomes; this write neither settles a worker nor claims process cleanup.
///
/// # Errors
/// Both runtime families and approval withdrawal commit together or not at all.
pub async fn request_shutdown_cancellation(
    database: &SqliteDatabase,
    now: UtcTimestamp,
) -> Result<(), DatabaseError> {
    let error = |source| DatabaseError::Sqlite {
        operation: "record shutdown cancellation",
        source,
    };
    let mut transaction = database
        .pool()
        .begin_with("BEGIN IMMEDIATE")
        .await
        .map_err(error)?;
    sqlx::query("UPDATE agent_runs SET cancellation_requested_at=COALESCE(cancellation_requested_at,?1) WHERE state NOT IN ('completed','cancelled','failed','awaiting_approval')")
        .bind(now.to_string()).execute(&mut *transaction).await.map_err(error)?;
    sqlx::query("UPDATE delegated_tasks SET cancellation_requested_at=COALESCE(cancellation_requested_at,?1) WHERE status IN ('queued','running')")
        .bind(now.to_string()).execute(&mut *transaction).await.map_err(error)?;
    sqlx::query("UPDATE approvals SET state=CASE WHEN state='pending' THEN 'expired' ELSE state END,arguments_json=NULL WHERE state IN ('pending','approved') AND run_id IN (SELECT id FROM agent_runs WHERE cancellation_requested_at IS NOT NULL)")
        .execute(&mut *transaction).await.map_err(error)?;
    transaction.commit().await.map_err(error)
}

/// Durable work counted by the stop/restart guard, in one database snapshot.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ShutdownWork {
    /// Ordinary and native delegated runs, excluding durable approval waits.
    pub runs: u32,
    /// Queued or running external coding jobs, including jobs whose parent has finished.
    pub coding_jobs: u32,
}

/// Counts cancellation results that could not confirm cleanup/settlement during this drain.
///
/// # Errors
/// Unreadable completion evidence cannot certify a successful drain.
pub async fn shutdown_coding_failures(
    database: &SqliteDatabase,
    since: UtcTimestamp,
) -> Result<u32, DatabaseError> {
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM delegated_tasks WHERE status='failed' AND cancellation_requested_at IS NOT NULL AND julianday(finished_at)>=julianday(?1)")
        .bind(since.to_string()).fetch_one(database.pool()).await.map_err(|source| DatabaseError::Sqlite {operation:"read shutdown coding failures",source})?;
    Ok(u32::try_from(count).unwrap_or(u32::MAX))
}

/// Reads work that startup would otherwise settle as interrupted.
///
/// Managed host processes live in the daemon's own registry and are counted there.
/// This query is global to the profile: stopping its daemon affects every workspace.
///
/// # Errors
///
/// Returns [`DatabaseError`] when either work table cannot be read. An unreadable
/// table is not evidence that the daemon is idle.
pub async fn read_shutdown_work(database: &SqliteDatabase) -> Result<ShutdownWork, DatabaseError> {
    let (runs, coding_jobs): (i64, i64) = sqlx::query_as(
        "SELECT \
         (SELECT COUNT(*) FROM agent_runs \
          WHERE state NOT IN ('completed', 'cancelled', 'failed', 'awaiting_approval')), \
         (SELECT COUNT(*) FROM delegated_tasks WHERE status IN ('queued', 'running'))",
    )
    .fetch_one(database.pool())
    .await
    .map_err(|source| DatabaseError::Sqlite {
        operation: "read work before stopping",
        source,
    })?;
    Ok(ShutdownWork {
        runs: u32::try_from(runs).unwrap_or(u32::MAX),
        coding_jobs: u32::try_from(coding_jobs).unwrap_or(u32::MAX),
    })
}
