//! Dispatch identity and Stop must survive races and process loss.
use super::*;
use crate::{CallBinding, CallOrigin, CallTarget, NewDelegatedTask, NewToolCall};
use jarvis_core::{CorrelationId, ToolOutcome};

async fn call(db: &SqliteDatabase, parent: &StartedRun, tool: &str) -> String {
    let id = jarvis_core::RunId::new().to_string();
    let input = must(NewToolCall::new(
        &id,
        CallOrigin::new(parent.workspace_id(), parent.run_id(), None),
        CallTarget::new(tool, "1.1.0"),
        CallBinding::new("a".repeat(64), id.replace('-', ""), "{}", "policy-1", None),
        CorrelationId::new(),
        at(1),
    ));
    must(crate::admit_tool_call(db, &input).await);
    must(crate::advance_tool_call(db, &id, ToolOutcome::Authorized, at(1)).await);
    must(crate::advance_tool_call(db, &id, ToolOutcome::Submitted, at(1)).await);
    id
}

fn coding(parent: &StartedRun) -> NewDelegatedTask<'_> {
    NewDelegatedTask {
        workspace_id: parent.workspace_id(),
        repository: "repo",
        task: "inspect",
        allow_commands: false,
        minutes: 1,
        now: at(1),
    }
}

#[tokio::test]
async fn native_dispatch_requires_its_submitted_parent_call_and_unchanged_objective() {
    let (_profile, db) = database().await;
    let parent = root(&db, "parent").await;
    let other = root(&db, "other").await;
    let foreign = call(&db, &other, "jarvis.agent.delegate").await;
    let wrong = call(&db, &parent, "jarvis.code.delegate_start").await;
    let ended = call(&db, &parent, "jarvis.agent.delegate").await;
    must(
        crate::record_tool_outcome(
            &db,
            &ended,
            &must(jarvis_core::ToolOutcomeRecord::confirmed("done")),
            None,
            at(2),
        )
        .await,
    );
    let before = start_counts(&db).await;
    for origin in [foreign, wrong, ended, jarvis_core::RunId::new().to_string()] {
        let input = must(
            child_input(&db, parent.run_id())
                .await
                .with_admission_limits(must(
                    must(RunAdmissionLimits::new(parent.run_id(), 4, 16)).with_origin_call(&origin),
                )),
        );
        assert!(matches!(
            start_run(&db, &input).await,
            Err(DatabaseError::DelegationOriginRefused)
        ));
        assert_eq!(start_counts(&db).await, before);
    }
    let origin = call(&db, &parent, "jarvis.agent.delegate").await;
    let mut input = must(
        child_input(&db, parent.run_id())
            .await
            .with_admission_limits(must(
                must(RunAdmissionLimits::new(parent.run_id(), 4, 16)).with_origin_call(&origin),
            )),
    );
    must(start_run(&db, &input).await);
    input.objective = "different work".into();
    assert!(matches!(
        start_run(&db, &input).await,
        Err(DatabaseError::DelegationOriginRefused)
    ));
    db.close().await;
}

#[tokio::test]
async fn concurrent_native_dispatch_adopts_one_child_and_reopen_never_relaunches() {
    let (profile, db) = database().await;
    let parent = root(&db, "parent").await;
    let origin = call(&db, &parent, "jarvis.agent.delegate").await;
    let input = must(
        child_input(&db, parent.run_id())
            .await
            .with_admission_limits(must(
                must(RunAdmissionLimits::new(parent.run_id(), 4, 16)).with_origin_call(&origin),
            )),
    );
    // Separate adapter invocations generate fresh run/session IDs for the same call.
    let other = must(
        child_input(&db, parent.run_id())
            .await
            .with_admission_limits(must(
                must(RunAdmissionLimits::new(parent.run_id(), 4, 16)).with_origin_call(&origin),
            )),
    );
    let (a, b) = tokio::join!(start_run(&db, &input), start_run(&db, &other));
    let (a, b) = (must(a), must(b));
    assert_eq!(a.run_id(), b.run_id());
    assert_ne!(a.was_created(), b.was_created());
    assert!(
        sqlx::query("UPDATE agent_runs SET origin_call_id=NULL WHERE id=?1")
            .bind(a.run_id())
            .execute(db.pool())
            .await
            .is_err()
    );
    must(crate::request_run_cancellation(&db, parent.run_id(), at(2)).await);
    db.close().await;
    let db = must(SqliteDatabase::open(&profile.database_path()).await);
    must(crate::recover_interrupted_runs(&db, at(3)).await);
    let counts = start_counts(&db).await;
    let replay = must(start_run(&db, &input).await);
    assert_eq!(replay.run_id(), a.run_id());
    assert!(!replay.was_created());
    assert_eq!(start_counts(&db).await, counts);
    db.close().await;
}

#[tokio::test]
async fn concurrent_coding_dispatch_adopts_one_job_and_rejects_changed_request() {
    let (profile, db) = database().await;
    let parent = root(&db, "parent").await;
    let origin = call(&db, &parent, "jarvis.code.delegate_start").await;
    let new = coding(&parent);
    let (a, b) = tokio::join!(
        crate::create_delegated_task_for_call(&db, &new, "{}", &origin),
        crate::create_delegated_task_for_call(&db, &new, "{}", &origin)
    );
    let (a, b) = (must(a), must(b));
    assert_eq!(a.0.id, b.0.id);
    assert_ne!(a.1, b.1);
    assert_eq!(a.0.parent_run_id.as_deref(), Some(parent.run_id()));
    assert!(matches!(
        crate::create_delegated_task_for_call(&db, &new, "{\"changed\":true}", &origin).await,
        Err(DatabaseError::DelegationOriginRefused)
    ));
    assert!(
        sqlx::query(
            "UPDATE delegated_tasks SET parent_run_id=NULL,origin_call_id=NULL WHERE id=?1"
        )
        .bind(&a.0.id)
        .execute(db.pool())
        .await
        .is_err()
    );
    // A deliberate new call is independent, even with identical arguments.
    let other = call(&db, &parent, "jarvis.code.delegate").await;
    let second = must(crate::create_delegated_task_for_call(&db, &new, "{}", &other).await);
    assert!(second.1);
    assert_ne!(a.0.id, second.0.id);
    db.close().await;
    let db = must(SqliteDatabase::open(&profile.database_path()).await);
    must(crate::interrupt_delegated_tasks(&db, at(3)).await);
    let replay = must(crate::create_delegated_task_for_call(&db, &new, "{}", &origin).await);
    assert!(!replay.1);
    assert_eq!(replay.0.id, a.0.id);
    assert_eq!(replay.0.status, "interrupted");
    db.close().await;
}

#[tokio::test]
async fn coding_origin_validation_refuses_missing_wrong_tool_child_and_unsubmitted_calls() {
    let (_profile, db) = database().await;
    let parent = root(&db, "parent").await;
    let native = child(&db, parent.run_id()).await;
    let wrong = call(&db, &parent, "jarvis.agent.delegate").await;
    let nested = call(&db, &native, "jarvis.code.delegate_start").await;
    let ended = call(&db, &parent, "jarvis.code.delegate_start").await;
    must(
        crate::record_tool_outcome(
            &db,
            &ended,
            &must(jarvis_core::ToolOutcomeRecord::confirmed("done")),
            None,
            at(2),
        )
        .await,
    );
    for origin in [jarvis_core::RunId::new().to_string(), wrong, nested, ended] {
        assert!(
            matches!(
                crate::create_delegated_task_for_call(&db, &coding(&parent), "{}", &origin).await,
                Err(DatabaseError::DelegationOriginRefused)
            ),
            "{origin}"
        );
    }
    let origin = call(&db, &parent, "jarvis.code.delegate_start").await;
    let mut foreign = coding(&parent);
    foreign.workspace_id = "0198f000-0000-7000-8000-000000000099";
    assert!(
        crate::create_delegated_task_for_call(&db, &foreign, "{}", &origin)
            .await
            .is_err()
    );
    assert!(must(crate::list_delegated_tasks(&db, parent.workspace_id(), 50).await).is_empty());
    db.close().await;
}

#[tokio::test]
async fn family_stop_is_atomic_across_runtimes_preserves_terminal_history_and_blocks_late_launch() {
    let (profile, db) = database().await;
    let parent = root(&db, "parent").await;
    let native = child(&db, parent.run_id()).await;
    let other = root(&db, "other").await;
    let launch = call(&db, &parent, "jarvis.code.delegate_start").await;
    let late = call(&db, &parent, "jarvis.code.delegate_start").await;
    let job =
        must(crate::create_delegated_task_for_call(&db, &coding(&parent), "{}", &launch).await).0;
    let unrelated = must(crate::create_delegated_task(&db, &coding(&other)).await);
    finish_fixture(&db, parent.run_id()).await;
    let before = must(crate::find_run(&db, parent.run_id()).await);
    must(crate::request_run_cancellation(&db, parent.run_id(), at(2)).await);
    assert_eq!(before, must(crate::find_run(&db, parent.run_id()).await));
    assert_eq!(
        must(crate::find_run(&db, native.run_id()).await).cancellation_requested_at(),
        Some(at(2))
    );
    assert!(must(
        crate::delegated_task_cancellation_requested(&db, parent.workspace_id(), &job.id).await
    ));
    assert!(!must(
        crate::delegated_task_cancellation_requested(&db, other.workspace_id(), &unrelated.id)
            .await
    ));
    assert!(
        crate::mark_delegated_task_running(&db, parent.workspace_id(), &job.id, at(3))
            .await
            .is_err()
    );
    // Even a completed root cannot admit a launch submitted before Stop.
    assert!(
        crate::create_delegated_task_for_call(&db, &coding(&parent), "{}", &late)
            .await
            .is_err()
    );
    db.close().await;
    let db = must(SqliteDatabase::open(&profile.database_path()).await);
    must(crate::interrupt_delegated_tasks(&db, at(3)).await);
    assert_eq!(
        must(crate::find_delegated_task(&db, parent.workspace_id(), &job.id).await).status,
        "cancelled"
    );
    assert_eq!(
        must(crate::find_delegated_task(&db, other.workspace_id(), &unrelated.id).await).status,
        "interrupted"
    );
    assert!(
        crate::create_delegated_task_for_call(&db, &coding(&parent), "{}", &late)
            .await
            .is_err()
    );
    db.close().await;
}

#[tokio::test]
async fn coding_admission_racing_parent_stop_never_leaves_uncancelled_work() {
    let (_profile, db) = database().await;
    for _ in 0..12 {
        let parent = root(&db, "racing parent").await;
        let origin = call(&db, &parent, "jarvis.code.delegate_start").await;
        let new = coding(&parent);
        let (dispatch, stopped) = tokio::join!(
            crate::create_delegated_task_for_call(&db, &new, "{}", &origin),
            crate::request_run_cancellation(&db, parent.run_id(), at(2))
        );
        must(stopped);
        match dispatch {
            Ok((job, true)) => assert!(must(
                crate::delegated_task_cancellation_requested(&db, parent.workspace_id(), &job.id)
                    .await
            )),
            Err(DatabaseError::DelegationOriginRefused) => {}
            other => panic!("unexpected dispatch: {other:?}"),
        }
    }
    db.close().await;
}

#[tokio::test]
async fn coding_stop_wins_late_success_preserves_evidence_and_is_task_local() {
    let (_profile, db) = database().await;
    let parent = root(&db, "parent").await;
    let origin = call(&db, &parent, "jarvis.code.delegate_start").await;
    let job =
        must(crate::create_delegated_task_for_call(&db, &coding(&parent), "{}", &origin).await).0;
    must(crate::mark_delegated_task_running(&db, parent.workspace_id(), &job.id, at(1)).await);
    assert!(must(
        crate::request_delegated_task_cancellation(&db, parent.workspace_id(), &job.id, at(2))
            .await
    ));
    assert!(must(
        crate::request_delegated_task_cancellation(&db, parent.workspace_id(), &job.id, at(3))
            .await
    ));
    let finish = crate::DelegatedTaskFinish {
        status: "completed",
        branch: None,
        worktree: None,
        summary: Some("files kept"),
        result_json: Some(r#"{"outcome":"completed","verification":{"status":"passed"}}"#),
        error: None,
        now: at(4),
    };
    must(crate::finish_delegated_task(&db, parent.workspace_id(), &job.id, &finish).await);
    let stored = must(crate::find_delegated_task(&db, parent.workspace_id(), &job.id).await);
    assert_eq!(stored.status, "cancelled");
    assert_eq!(stored.cancellation_requested_at, Some(at(2)));
    let result: serde_json::Value = must(serde_json::from_str(
        stored.result_json.as_deref().unwrap_or(""),
    ));
    assert_eq!(result["outcome"], "cancelled");
    assert_eq!(result["verification"]["status"], "passed");
    assert!(
        must(crate::find_run(&db, parent.run_id()).await)
            .cancellation_requested_at()
            .is_none()
    );
    assert!(!must(
        crate::request_delegated_task_cancellation(&db, parent.workspace_id(), &job.id, at(5))
            .await
    ));
    assert!(
        crate::finish_delegated_task(&db, parent.workspace_id(), &job.id, &finish)
            .await
            .is_err()
    );
    db.close().await;
}

#[tokio::test]
async fn interrupted_parent_with_coding_dispatch_is_never_automatically_continued() {
    let (_profile, db) = database().await;
    let parent = root(&db, "parent").await;
    let origin = call(&db, &parent, "jarvis.code.delegate_start").await;
    must(crate::create_delegated_task_for_call(&db, &coding(&parent), "{}", &origin).await);
    let recovered = must(crate::recover_interrupted_runs(&db, at(3)).await);
    assert!(
        must(crate::continuable_interrupted_runs(&db, &recovered, at(0), &[], 10).await).is_empty()
    );
    db.close().await;
}

#[tokio::test]
async fn cancellation_intent_never_hides_cleanup_failure_or_an_unavailable_result() {
    let (_profile, db) = database().await;
    let parent = root(&db, "parent").await;
    for (result, error) in [
        (
            Some(r#"{"outcome":"cleanup_failed","cleanup_error":"access denied"}"#),
            None,
        ),
        (None, Some("final result could not be saved; inspect files")),
    ] {
        let job = must(crate::create_delegated_task(&db, &coding(&parent)).await);
        must(
            crate::request_delegated_task_cancellation(&db, parent.workspace_id(), &job.id, at(2))
                .await,
        );
        let finish = crate::DelegatedTaskFinish {
            status: "failed",
            branch: None,
            worktree: None,
            summary: None,
            result_json: result,
            error,
            now: at(3),
        };
        must(crate::finish_delegated_task(&db, parent.workspace_id(), &job.id, &finish).await);
        let stored = must(crate::find_delegated_task(&db, parent.workspace_id(), &job.id).await);
        assert_eq!(stored.status, "failed");
        assert_eq!(stored.result_json.as_deref(), result);
        assert_eq!(stored.error.as_deref(), error);
        assert_eq!(stored.cancellation_requested_at, Some(at(2)));
    }
    db.close().await;
}
