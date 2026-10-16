//! Work whose current execution would be interrupted by stopping this daemon.

use crate::{DatabaseError, SqliteDatabase};

/// Durable work counted by the stop/restart guard, in one database snapshot.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ShutdownWork {
    /// Ordinary and native delegated runs, excluding durable approval waits.
    pub runs: u32,
    /// Queued or running external coding jobs, including jobs whose parent has finished.
    pub coding_jobs: u32,
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
