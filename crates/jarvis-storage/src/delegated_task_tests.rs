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
    let db = SqliteDatabase::test_fixture(&p.join(DEFAULT_DATABASE_FILENAME))
        .await
        .unwrap();
    (Scratch(p), db)
}
fn at(n: i128) -> UtcTimestamp {
    UtcTimestamp::from_unix_nanos(1_800_000_000_000_000_000 + n).unwrap()
}

#[tokio::test]
async fn activity_is_scoped_running_only_and_survives_reopening_and_interruption() {
    let (scratch, database) = db().await;
    let task = create_delegated_task(
        &database,
        &NewDelegatedTask {
            workspace_id: LOCAL_WORKSPACE_ID,
            repository: "repo",
            task: "read config",
            allow_commands: false,
            minutes: 10,
            now: at(0),
        },
    )
    .await
    .unwrap();
    assert!(
        !append_delegated_task_activity(
            &database,
            LOCAL_WORKSPACE_ID,
            &task.id,
            1,
            "too early",
            at(1)
        )
        .await
        .unwrap()
    );
    mark_delegated_task_running(&database, LOCAL_WORKSPACE_ID, &task.id, at(2))
        .await
        .unwrap();
    assert!(
        append_delegated_task_activity(
            &database,
            LOCAL_WORKSPACE_ID,
            &task.id,
            1,
            "Read config [in progress]",
            at(3)
        )
        .await
        .unwrap()
    );
    assert!(
        !append_delegated_task_activity(
            &database,
            LOCAL_WORKSPACE_ID,
            &task.id,
            1,
            "duplicate",
            at(4)
        )
        .await
        .unwrap()
    );
    assert!(
        !append_delegated_task_activity(&database, "other", &task.id, 2, "wrong workspace", at(4))
            .await
            .unwrap()
    );
    database.close().await;
    let database = SqliteDatabase::open(&scratch.0.join(DEFAULT_DATABASE_FILENAME))
        .await
        .unwrap();
    let found = find_delegated_task(&database, LOCAL_WORKSPACE_ID, &task.id)
        .await
        .unwrap();
    assert_eq!(found.status, "running");
    assert!(found.result_json.is_none());
    assert_eq!(found.activity.len(), 1);
    assert_eq!(found.activity[0].summary, "Read config [in progress]");
    interrupt_delegated_tasks(&database, at(5)).await.unwrap();
    assert!(
        !append_delegated_task_activity(
            &database,
            LOCAL_WORKSPACE_ID,
            &task.id,
            2,
            "too late",
            at(6)
        )
        .await
        .unwrap()
    );
    assert_eq!(
        find_delegated_task(&database, LOCAL_WORKSPACE_ID, &task.id)
            .await
            .unwrap()
            .activity,
        found.activity
    );
    assert!(
        list_delegated_tasks(&database, "other", 10)
            .await
            .unwrap()
            .is_empty()
    );
    database.close().await;
}

#[tokio::test]
async fn activity_payload_guards_reject_invalid_sequences_sizes_and_ownership() {
    let (_scratch, database) = db().await;
    let task = create_delegated_task(
        &database,
        &NewDelegatedTask {
            workspace_id: LOCAL_WORKSPACE_ID,
            repository: "repo",
            task: "read config",
            allow_commands: false,
            minutes: 10,
            now: at(0),
        },
    )
    .await
    .unwrap();
    mark_delegated_task_running(&database, LOCAL_WORKSPACE_ID, &task.id, at(1))
        .await
        .unwrap();
    for (sequence, summary) in [
        (0, "invalid sequence".to_owned()),
        (1, "x".repeat(MAX_TASK_ACTIVITY_CHARS + 1)),
        (1, "invalid\0control".to_owned()),
    ] {
        assert!(
            append_delegated_task_activity(
                &database,
                LOCAL_WORKSPACE_ID,
                &task.id,
                sequence,
                &summary,
                at(2)
            )
            .await
            .is_err()
        );
    }
    assert!(sqlx::query("INSERT INTO delegated_task_activity (task_id,workspace_id,sequence,summary,created_at) VALUES (?1,'other',2,'wrong owner',?2)")
        .bind(&task.id).bind(at(3).to_string()).execute(database.pool()).await.is_err());
    assert!(
        find_delegated_task(&database, LOCAL_WORKSPACE_ID, &task.id)
            .await
            .unwrap()
            .activity
            .is_empty()
    );
    database.close().await;
}

#[tokio::test]
async fn activity_history_is_bounded_and_returned_in_sequence_order() {
    let (_scratch, database) = db().await;
    let task = create_delegated_task(
        &database,
        &NewDelegatedTask {
            workspace_id: LOCAL_WORKSPACE_ID,
            repository: "repo",
            task: "many updates",
            allow_commands: false,
            minutes: 10,
            now: at(0),
        },
    )
    .await
    .unwrap();
    mark_delegated_task_running(&database, LOCAL_WORKSPACE_ID, &task.id, at(1))
        .await
        .unwrap();
    let last = MAX_TASK_ACTIVITY_RETAINED + 5;
    for sequence in 1..=last {
        assert!(
            append_delegated_task_activity(
                &database,
                LOCAL_WORKSPACE_ID,
                &task.id,
                sequence,
                &format!("update {sequence}"),
                at(i128::from(sequence))
            )
            .await
            .unwrap()
        );
    }
    let found = find_delegated_task(&database, LOCAL_WORKSPACE_ID, &task.id)
        .await
        .unwrap();
    assert_eq!(
        found.activity.len(),
        usize::try_from(MAX_TASK_ACTIVITY_SHOWN).unwrap()
    );
    assert_eq!(found.activity.last().unwrap().sequence, last);
    assert!(
        found
            .activity
            .windows(2)
            .all(|pair| pair[0].sequence + 1 == pair[1].sequence)
    );
    let count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM delegated_task_activity WHERE task_id=?1")
            .bind(&task.id)
            .fetch_one(database.pool())
            .await
            .unwrap();
    assert_eq!(count, i64::try_from(MAX_TASK_ACTIVITY_RETAINED).unwrap());
    database.close().await;
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
    for (workspace, id) in [
        (LOCAL_WORKSPACE_ID, task.id.as_str()),
        ("other", task.id.as_str()),
        (LOCAL_WORKSPACE_ID, "missing"),
    ] {
        assert!(matches!(
            finish_delegated_task(
                &db,
                workspace,
                id,
                &DelegatedTaskFinish {
                    status: "failed",
                    branch: None,
                    worktree: None,
                    summary: None,
                    result_json: None,
                    error: Some("must not overwrite the result"),
                    now: at(3),
                },
            )
            .await,
            Err(DatabaseError::DelegatedTaskNotFound)
        ));
    }
    assert_eq!(
        find_delegated_task(&db, LOCAL_WORKSPACE_ID, &task.id)
            .await
            .unwrap(),
        found
    );
    db.close().await;
}

#[tokio::test]
async fn tasks_can_queue_and_restart_marks_all_interrupted() {
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
    assert!(second.is_ok(), "a second task must be queued");
    assert_eq!(interrupt_delegated_tasks(&db, at(1)).await.unwrap(), 2);
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

#[tokio::test]
async fn schema_29_preserves_queued_work_and_activity_from_28_and_keeps_workspace_guards() {
    use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(
            SqliteConnectOptions::new()
                .in_memory(true)
                .foreign_keys(true),
        )
        .await
        .unwrap();
    let current = sqlx::migrate!("./migrations/sqlite");
    let previous = sqlx::migrate::Migrator::with_migrations(
        current
            .iter()
            .filter(|migration| migration.version <= 28)
            .cloned()
            .collect(),
    );
    previous.run(&pool).await.unwrap();
    let approval_id = seed_pre_migration_approval(&pool).await;

    let task_id = "00000000-0000-0000-0000-000000000002";
    sqlx::query("INSERT INTO delegated_tasks (id,workspace_id,repository,task,allow_commands,minutes,status,created_nanos,created_at,updated_at) VALUES (?1,?2,'repo','old task',0,7,'running',1,?3,?3)")
        .bind(task_id).bind(LOCAL_WORKSPACE_ID).bind(at(0).to_string()).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO delegated_task_activity VALUES (?1,?2,1,'retained progress',?3)")
        .bind(task_id)
        .bind(LOCAL_WORKSPACE_ID)
        .bind(at(1).to_string())
        .execute(&pool)
        .await
        .unwrap();
    current.run(&pool).await.unwrap();
    let progress: String =
        sqlx::query_scalar("SELECT summary FROM delegated_task_activity WHERE task_id=?1")
            .bind(task_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(progress, "retained progress");
    let approval: (String, String, String) =
        sqlx::query_as("SELECT state,intent_hash,arguments_json FROM approvals WHERE id=?1")
            .bind(approval_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        approval,
        (
            "pending".to_owned(),
            "a".repeat(64),
            "{\"task\":\"retained brief\"}".to_owned()
        )
    );
    sqlx::query("INSERT INTO delegated_tasks (id,workspace_id,repository,task,allow_commands,minutes,status,created_nanos,created_at,updated_at,request_json) VALUES (?1,?2,'folder','new task',1,0,'queued',2,?3,?3,'{}')")
        .bind("00000000-0000-0000-0000-000000000003").bind(LOCAL_WORKSPACE_ID).bind(at(2).to_string()).execute(&pool).await.unwrap();
    assert!(
        sqlx::query(
            "INSERT INTO delegated_task_activity VALUES (?1,'wrong-workspace',2,'escape',?2)"
        )
        .bind(task_id)
        .bind(at(2).to_string())
        .execute(&pool)
        .await
        .is_err()
    );
    let violations = sqlx::query("PRAGMA foreign_key_check")
        .fetch_all(&pool)
        .await
        .unwrap();
    assert!(violations.is_empty());
    pool.close().await;
}

async fn seed_pre_migration_approval(pool: &sqlx::SqlitePool) -> &'static str {
    let session = "00000000-0000-0000-0000-000000000011";
    let run = "00000000-0000-0000-0000-000000000012";
    let approval = "00000000-0000-0000-0000-000000000013";
    let now = at(0).to_string();
    sqlx::query("INSERT INTO sessions (id,workspace_id,user_id,channel,status,created_at,updated_at,version) VALUES (?1,?2,?3,'cli','active',?4,?4,1)")
        .bind(session).bind(LOCAL_WORKSPACE_ID).bind(crate::LOCAL_USER_ID).bind(&now).execute(pool).await.unwrap();
    sqlx::query("INSERT INTO agent_runs (id,session_id,workspace_id,user_id,objective,state,started_at,updated_at,correlation_id,version) VALUES (?1,?2,?3,?4,'pending coding task','awaiting_approval',?5,?5,?1,1)")
        .bind(run).bind(session).bind(LOCAL_WORKSPACE_ID).bind(crate::LOCAL_USER_ID).bind(&now).execute(pool).await.unwrap();
    sqlx::query("INSERT INTO approvals (id,workspace_id,run_id,actor_id,tool,tool_version,intent_hash,preview,risk_level,required_strength,nonce_hash,state,correlation_id,created_at,expires_at,arguments_json) VALUES (?1,?2,?3,'agent','jarvis.code.delegate','1.0.0',?4,'Build in the selected folder',3,'credential',?4,'pending',?1,?5,?5,?6)")
        .bind(approval).bind(LOCAL_WORKSPACE_ID).bind(run).bind("a".repeat(64)).bind(&now).bind(r#"{"task":"retained brief"}"#).execute(pool).await.unwrap();
    approval
}
