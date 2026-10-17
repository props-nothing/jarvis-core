//! Close admission, request cancellation, and retain dependencies until work settles.
use crate::process_tools::ProcessTools;
use jarvis_core::{SystemClock, UtcTimestamp};
use jarvis_storage::SqliteDatabase;
use std::time::Duration;

pub(crate) async fn drain_work(
    database: &SqliteDatabase,
    processes: Option<&ProcessTools>,
    grace: Duration,
) -> Result<(), &'static str> {
    database.work_admission().close();
    let started = UtcTimestamp::now(&SystemClock);
    // Timeout owns only this coordinator, never an effect's future. Workers keep
    // their cleanup/settlement ownership until completion or process teardown.
    tokio::time::timeout(grace, async {
        loop {
            // Signal owned host processes even if durable state has become unreadable.
            // Keep the result until after attempting durable cancellation as well.
            let process_signal = processes.map_or(Ok(0), ProcessTools::request_shutdown);
            // Repeat to include admissions that already held a permit when close won.
            jarvis_storage::request_shutdown_cancellation(
                database,
                UtcTimestamp::now(&SystemClock),
            )
            .await
            .map_err(|_| "shutdown cancellation could not be recorded")?;
            // Read permits BEFORE durable/registry counts. If the last publisher
            // finishes between snapshots, its published work must still be seen.
            let active = database
                .work_admission()
                .active_count()
                .map_err(|_| "shutdown admission state could not be read")?;
            if active == 0 {
                jarvis_storage::settle_unstarted_shutdown_runs(
                    database,
                    UtcTimestamp::now(&SystemClock),
                )
                .await
                .map_err(|_| "unstarted runs could not settle during shutdown")?;
            }
            let work = jarvis_storage::read_shutdown_work(database)
                .await
                .map_err(|_| "shutdown work state could not be read")?;
            let managed = processes.map_or(Ok(0), ProcessTools::request_shutdown)?;
            process_signal?;
            if work.runs == 0 && work.coding_jobs == 0 && managed == 0 && active == 0 {
                if jarvis_storage::shutdown_coding_failures(database, started)
                    .await
                    .map_err(|_| "coding cleanup evidence could not be read")?
                    != 0
                {
                    return Err("a coding task could not confirm shutdown cleanup or settlement");
                }
                return Ok(());
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .map_err(|_| "shutdown deadline reached with unfinished work")?
}

#[cfg(test)]
#[path = "shutdown_tests.rs"]
mod tests;
