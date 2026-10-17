//! Global shutdown intent is atomic and distinct from a parent Stop.
use super::*;
use crate::{NewDelegatedTask, request_shutdown_cancellation, shutdown_coding_failures};

#[tokio::test]
async fn shutdown_cancels_active_families_but_preserves_approval_waits_and_history() {
    let (profile, db) = database().await;
    let parent = root(&db, "parent").await;
    let running = child(&db, parent.run_id()).await;
    let waiting = child(&db, parent.run_id()).await;
    park(&db, waiting.run_id()).await;
    let finished = root(&db, "finished").await;
    finish_fixture(&db, finished.run_id()).await;
    let job = must(
        crate::create_delegated_task(
            &db,
            &NewDelegatedTask {
                workspace_id: parent.workspace_id(),
                repository: "repo",
                task: "queued",
                allow_commands: false,
                minutes: 1,
                now: at(1),
            },
        )
        .await,
    );
    must(request_shutdown_cancellation(&db, at(2)).await);
    must(request_shutdown_cancellation(&db, at(3)).await);
    db.close().await;
    let db = must(SqliteDatabase::open(&profile.database_path()).await);
    for id in [parent.run_id(), running.run_id()] {
        assert_eq!(
            must(crate::find_run(&db, id).await).cancellation_requested_at(),
            Some(at(2))
        );
    }
    for id in [waiting.run_id(), finished.run_id()] {
        assert_eq!(
            must(crate::find_run(&db, id).await).cancellation_requested_at(),
            None
        );
    }
    assert!(must(
        crate::delegated_task_cancellation_requested(&db, parent.workspace_id(), &job.id).await
    ));
    assert_eq!(
        must(crate::find_run(&db, waiting.run_id()).await).state(),
        RunState::AwaitingApproval
    );
    assert_eq!(
        must(crate::read_shutdown_work(&db).await),
        crate::ShutdownWork {
            runs: 2,
            coding_jobs: 1
        }
    );
    db.close().await;
}

#[tokio::test]
async fn shutdown_write_failure_rolls_back_native_intent_and_never_certifies_idle() {
    let (_profile, db) = database().await;
    let parent = root(&db, "parent").await;
    must(
        sqlx::query("DROP TABLE delegated_tasks")
            .execute(db.pool())
            .await,
    );
    assert!(request_shutdown_cancellation(&db, at(2)).await.is_err());
    assert_eq!(
        must(crate::find_run(&db, parent.run_id()).await).cancellation_requested_at(),
        None
    );
    assert!(crate::read_shutdown_work(&db).await.is_err());
    assert!(shutdown_coding_failures(&db, at(2)).await.is_err());
    db.close().await;
}

#[tokio::test]
async fn shutdown_failure_evidence_excludes_old_failures_and_successful_cancellations() {
    let (_profile, db) = database().await;
    let parent = root(&db, "parent").await;
    for (status, finished_at) in [("failed", at(1)), ("cancelled", at(3)), ("failed", at(3))] {
        let job = must(
            crate::create_delegated_task(
                &db,
                &NewDelegatedTask {
                    workspace_id: parent.workspace_id(),
                    repository: "repo",
                    task: "cleanup",
                    allow_commands: false,
                    minutes: 1,
                    now: at(1),
                },
            )
            .await,
        );
        must(sqlx::query("UPDATE delegated_tasks SET status=?1,cancellation_requested_at=?2,finished_at=?2 WHERE id=?3")
            .bind(status).bind(finished_at.to_string()).bind(job.id).execute(db.pool()).await);
    }
    assert_eq!(must(shutdown_coding_failures(&db, at(2)).await), 1);
    db.close().await;
}

#[tokio::test]
async fn shutdown_only_settles_unstarted_native_runs_after_every_permit_is_released() {
    let (_profile, db) = database().await;
    let parent = root(&db, "unstarted").await;
    let waiting = child(&db, parent.run_id()).await;
    park(&db, waiting.run_id()).await;
    must(request_shutdown_cancellation(&db, at(2)).await);
    must(crate::settle_unstarted_shutdown_runs(&db, at(2)).await);
    assert_eq!(
        must(crate::find_run(&db, parent.run_id()).await).state(),
        RunState::Received
    );
    let permit = must(db.work_admission().admit());
    db.work_admission().close();
    must(crate::settle_unstarted_shutdown_runs(&db, at(2)).await);
    assert_eq!(
        must(crate::find_run(&db, parent.run_id()).await).state(),
        RunState::Received
    );
    drop(permit);
    must(crate::settle_unstarted_shutdown_runs(&db, at(3)).await);
    assert_eq!(
        must(crate::find_run(&db, parent.run_id()).await).state(),
        RunState::Cancelled
    );
    assert_eq!(
        must(crate::find_run(&db, waiting.run_id()).await).state(),
        RunState::AwaitingApproval
    );
    let events = must(
        crate::read_run_events(
            &db,
            parent.run_id(),
            must(jarvis_core::ReplayRequest::new(
                must(jarvis_core::RunEventSequence::new(1)),
                100,
            )),
        )
        .await,
    );
    assert_eq!(
        events.last().map(crate::StoredRunEvent::kind),
        Some(jarvis_core::RunEventKind::RunCancelled)
    );
    db.close().await;
}
