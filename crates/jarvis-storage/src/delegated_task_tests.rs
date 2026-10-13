#![allow(clippy::unwrap_used)]

use std::path::PathBuf;

use super::*;
use crate::{DEFAULT_DATABASE_FILENAME, LOCAL_WORKSPACE_ID};

struct Scratch(PathBuf);
impl Drop for Scratch {
    fn drop(&mut self) {
        jarvis_core::remove_scratch_dir(&self.0);
    }
}
async fn db() -> (Scratch, SqliteDatabase) {
    let p = std::env::temp_dir().join(format!("jdtask-{}", jarvis_core::scratch_tag()));
    std::fs::create_dir_all(&p).unwrap();
    let db = SqliteDatabase::open(&p.join(DEFAULT_DATABASE_FILENAME))
        .await
        .unwrap();
    (Scratch(p), db)
}
fn at(n: i128) -> UtcTimestamp {
    UtcTimestamp::from_unix_nanos(1_800_000_000_000_000_000 + n).unwrap()
}

#[tokio::test]
async fn lifecycle_is_durable_and_workspace_scoped() {
    let (_s, db) = db().await;
    let task = create_delegated_task(
        &db,
        &NewDelegatedTask {
            workspace_id: LOCAL_WORKSPACE_ID,
            repository: "repo",
            task: "do it",
            allow_commands: true,
            minutes: 20,
            now: at(0),
        },
    )
    .await
    .unwrap();
    assert_eq!(task.status, "queued");
    mark_delegated_task_running(&db, LOCAL_WORKSPACE_ID, &task.id, at(1))
        .await
        .unwrap();
    finish_delegated_task(
        &db,
        LOCAL_WORKSPACE_ID,
        &task.id,
        &DelegatedTaskFinish {
            status: "completed",
            branch: Some("jarvis/x"),
            worktree: Some("C:/w"),
            summary: Some("done"),
            result_json: Some("{}"),
            error: None,
            now: at(2),
        },
    )
    .await
    .unwrap();
    let found = find_delegated_task(&db, LOCAL_WORKSPACE_ID, &task.id)
        .await
        .unwrap();
    assert_eq!(found.status, "completed");
    assert_eq!(found.branch.as_deref(), Some("jarvis/x"));
    assert!(found.finished_at.is_some());
    assert!(matches!(
        find_delegated_task(&db, "other", &task.id).await,
        Err(DatabaseError::DelegatedTaskNotFound)
    ));
}

#[tokio::test]
async fn only_one_task_is_active_and_restart_marks_it_interrupted() {
    let (_s, db) = db().await;
    let new = NewDelegatedTask {
        workspace_id: LOCAL_WORKSPACE_ID,
        repository: "repo",
        task: "one",
        allow_commands: false,
        minutes: 10,
        now: at(0),
    };
    let one = create_delegated_task(&db, &new).await.unwrap();
    let second = create_delegated_task(&db, &NewDelegatedTask { task: "two", ..new }).await;
    assert!(
        second.is_err(),
        "the unique active index refuses a second task"
    );
    assert_eq!(interrupt_delegated_tasks(&db, at(1)).await.unwrap(), 1);
    assert_eq!(
        find_delegated_task(&db, LOCAL_WORKSPACE_ID, &one.id)
            .await
            .unwrap()
            .status,
        "interrupted"
    );
    assert!(
        create_delegated_task(
            &db,
            &NewDelegatedTask {
                task: "three",
                now: at(2),
                ..new
            }
        )
        .await
        .is_ok()
    );
}
