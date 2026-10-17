#![allow(clippy::unwrap_used)]
use super::*;
use std::sync::Arc;

struct Scratch(std::path::PathBuf);
impl Drop for Scratch {
    fn drop(&mut self) {
        jarvis_core::remove_scratch_dir(&self.0);
    }
}
async fn database() -> (Scratch, Arc<SqliteDatabase>) {
    let path = std::env::temp_dir().join(format!("jdr-{}", jarvis_core::scratch_tag()));
    std::fs::create_dir_all(&path).unwrap();
    let db = SqliteDatabase::open(&path.join("jarvis.sqlite3"))
        .await
        .unwrap();
    (Scratch(path), Arc::new(db))
}

#[tokio::test]
async fn shutdown_waits_for_admitted_publishers_and_does_not_drop_them_on_deadline() {
    let (_profile, db) = database().await;
    let permit = db.work_admission().admit().unwrap();
    assert!(
        drain_work(&db, None, Duration::from_millis(80))
            .await
            .is_err()
    );
    assert_eq!(db.work_admission().active_count().unwrap(), 1);
    assert!(db.work_admission().admit().is_err());
    drop(permit);
    drain_work(&db, None, Duration::from_secs(2)).await.unwrap();
    db.close().await;
}

#[tokio::test]
async fn shutdown_waits_for_admitted_work_and_its_durable_settlement() {
    settle_during_drain(false).await;
}

#[tokio::test]
async fn shutdown_refuses_failed_coding_cleanup_even_after_its_active_count_reaches_zero() {
    settle_during_drain(true).await;
}

async fn settle_during_drain(cleanup_failed: bool) {
    let (_profile, db) = database().await;
    let permit = db.work_admission().admit().unwrap();
    let worker_db = Arc::clone(&db);
    let job = jarvis_storage::create_delegated_task(
        &db,
        &jarvis_storage::NewDelegatedTask {
            workspace_id: jarvis_storage::LOCAL_WORKSPACE_ID,
            repository: "repo",
            task: "late",
            allow_commands: false,
            minutes: 1,
            now: UtcTimestamp::now(&SystemClock),
        },
    )
    .await
    .unwrap();
    let worker = tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(100)).await;
        drop(permit);
        loop {
            if jarvis_storage::delegated_task_cancellation_requested(
                &worker_db,
                jarvis_storage::LOCAL_WORKSPACE_ID,
                &job.id,
            )
            .await
            .unwrap()
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        jarvis_storage::finish_delegated_task(
            &worker_db,
            jarvis_storage::LOCAL_WORKSPACE_ID,
            &job.id,
            &jarvis_storage::DelegatedTaskFinish {
                status: if cleanup_failed {
                    "failed"
                } else {
                    "cancelled"
                },
                branch: None,
                worktree: None,
                summary: None,
                result_json: None,
                error: cleanup_failed.then_some("cleanup failed"),
                now: UtcTimestamp::now(&SystemClock),
            },
        )
        .await
        .unwrap();
        job.id
    });
    let drained = drain_work(&db, None, Duration::from_secs(3)).await;
    assert_eq!(drained.is_err(), cleanup_failed, "{drained:?}");
    worker.await.unwrap();
    assert_eq!(
        jarvis_storage::read_shutdown_work(&db)
            .await
            .unwrap()
            .coding_jobs,
        0
    );
    db.close().await;
}

#[tokio::test]
async fn shutdown_refuses_unreadable_database() {
    let (_profile, db) = database().await;
    db.close().await;
    assert!(drain_work(&db, None, Duration::from_secs(1)).await.is_err());
}
