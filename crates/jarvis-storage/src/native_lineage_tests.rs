//! Durable native lineage and lifecycle falsification tests.

use super::tests::{at, database, start_counts, start_input};
use super::*;

fn must<T, E: std::fmt::Debug>(result: Result<T, E>) -> T {
    result.unwrap_or_else(|error| panic!("{error:?}"))
}

async fn root(database: &SqliteDatabase, objective: &str) -> StartedRun {
    let input = start_input(
        database,
        SessionTarget::New,
        jarvis_core::SessionId::new().to_string(),
        objective,
    )
    .await;
    must(start_run(database, &input).await)
}

async fn child_input(database: &SqliteDatabase, parent: &str) -> StartRunInput {
    let input = start_input(
        database,
        SessionTarget::New,
        jarvis_core::SessionId::new().to_string(),
        "a task with no marker",
    )
    .await;
    must(input.with_admission_limits(must(RunAdmissionLimits::new(parent, 4, 16))))
}

async fn child(database: &SqliteDatabase, parent: &str) -> StartedRun {
    must(start_run(database, &child_input(database, parent).await).await)
}

async fn park(database: &SqliteDatabase, id: &str) {
    for state in [
        RunState::ContextBuilding,
        RunState::Planning,
        RunState::AwaitingApproval,
    ] {
        let run = must(crate::find_run(database, id).await);
        must(
            crate::transition_run(
                database,
                id,
                run.expectation(),
                &must(jarvis_core::RunTransition::new(state, None, None)),
                at(1),
            )
            .await,
        );
    }
}

async fn finish_fixture(database: &SqliteDatabase, id: &str) {
    must(sqlx::query("UPDATE agent_runs SET state = 'completed', terminal_outcome = 'succeeded', completed_at = ?2 WHERE id = ?1")
        .bind(id).bind(at(2).to_string()).execute(database.pool()).await);
}

#[tokio::test]
async fn lineage_survives_reopen_is_immutable_and_does_not_depend_on_text() {
    let (profile, database) = database().await;
    let parent = root(&database, "parent").await;
    let child = child(&database, parent.run_id()).await;
    assert_eq!(child.parent_run_id(), Some(parent.run_id()));
    assert!(
        sqlx::query(
            "UPDATE agent_runs SET parent_run_id = NULL, delegation_kind = 'none' WHERE id = ?1"
        )
        .bind(child.run_id())
        .execute(database.pool())
        .await
        .is_err(),
        "lineage cannot be rewritten"
    );
    database.close().await;
    let reopened = must(SqliteDatabase::open(&profile.database_path()).await);
    let stored = must(crate::find_run(&reopened, child.run_id()).await);
    assert!(stored.is_delegated());
    assert_eq!(stored.parent_run_id(), Some(parent.run_id()));
    assert_eq!(stored.objective(), "a task with no marker");
    let listed = must(crate::read_delegated_runs(&reopened, child.workspace_id(), 20).await);
    assert_eq!(listed, vec![stored]);
    reopened.close().await;
}

#[tokio::test]
async fn invalid_parent_admission_fails_closed_without_partial_records() {
    let (_profile, database) = database().await;
    let parent = root(&database, "parent").await;
    let child = child(&database, parent.run_id()).await;
    let before = start_counts(&database).await;
    for invalid in [
        child.run_id().to_owned(),
        jarvis_core::RunId::new().to_string(),
    ] {
        let input = child_input(&database, &invalid).await;
        let result = start_run(&database, &input).await;
        assert!(
            matches!(
                &result,
                Err(DatabaseError::InvalidRunRequest {
                    field: "delegation parent"
                })
            ),
            "invalid parent must refuse admission: {result:?}"
        );
        assert_eq!(start_counts(&database).await, before);
    }
    for foreign_workspace in [true, false] {
        let mut input = child_input(&database, parent.run_id()).await;
        if foreign_workspace {
            input.workspace_id = jarvis_core::RunId::new().to_string();
        } else {
            input.user_id = jarvis_core::RunId::new().to_string();
        }
        assert!(matches!(
            start_run(&database, &input).await,
            Err(DatabaseError::InvalidRunRequest {
                field: "delegation parent"
            })
        ));
        assert_eq!(start_counts(&database).await, before);
    }
    must(crate::request_run_cancellation(&database, parent.run_id(), at(2)).await);
    assert!(
        start_run(&database, &child_input(&database, parent.run_id()).await)
            .await
            .is_err()
    );
    let terminal = root(&database, "completed parent").await;
    finish_fixture(&database, terminal.run_id()).await;
    let before = start_counts(&database).await;
    assert!(
        start_run(&database, &child_input(&database, terminal.run_id()).await)
            .await
            .is_err()
    );
    assert_eq!(start_counts(&database).await, before);
    database.close().await;
}

#[tokio::test]
async fn parent_stop_racing_admission_leaves_no_uncancelled_child() {
    let (_profile, database) = database().await;
    let database = std::sync::Arc::new(database);
    let parent = root(&database, "parent").await;
    let unrelated = root(&database, "unrelated").await;
    let barrier = std::sync::Arc::new(tokio::sync::Barrier::new(13));
    let mut tasks = Vec::new();
    for _ in 0..12 {
        let input = child_input(&database, parent.run_id()).await;
        let database = std::sync::Arc::clone(&database);
        let barrier = std::sync::Arc::clone(&barrier);
        tasks.push(tokio::spawn(async move {
            barrier.wait().await;
            start_run(&database, &input).await
        }));
    }
    barrier.wait().await;
    must(crate::request_run_cancellation(&database, parent.run_id(), at(2)).await);
    for task in tasks {
        match must(task.await) {
            Ok(run) => assert_eq!(
                must(crate::find_run(&database, run.run_id()).await).cancellation_requested_at(),
                Some(at(2))
            ),
            Err(
                DatabaseError::RunCapacityExceeded
                | DatabaseError::InvalidRunRequest {
                    field: "delegation parent",
                },
            ) => {}
            other => panic!("unexpected admission: {other:?}"),
        }
    }
    assert!(
        must(crate::find_run(&database, unrelated.run_id()).await)
            .cancellation_requested_at()
            .is_none()
    );
    database.close().await;
}

#[tokio::test]
async fn completed_parent_stop_cancels_children_but_child_stop_is_local() {
    let (_profile, database) = database().await;
    let parent = root(&database, "parent").await;
    let first = child(&database, parent.run_id()).await;
    let parked = child(&database, parent.run_id()).await;
    let completed = child(&database, parent.run_id()).await;
    park(&database, parked.run_id()).await;
    finish_fixture(&database, completed.run_id()).await;
    must(crate::request_run_cancellation(&database, first.run_id(), at(1)).await);
    assert!(
        must(crate::find_run(&database, parent.run_id()).await)
            .cancellation_requested_at()
            .is_none()
    );
    assert!(
        must(crate::find_run(&database, parked.run_id()).await)
            .cancellation_requested_at()
            .is_none()
    );
    finish_fixture(&database, parent.run_id()).await;
    let parent_before = must(crate::find_run(&database, parent.run_id()).await);
    let completed_before = must(crate::find_run(&database, completed.run_id()).await);
    let stopped = must(crate::request_run_cancellation(&database, parent.run_id(), at(3)).await);
    assert_eq!(stopped, parent_before, "parent success remains truthful");
    assert_eq!(
        must(crate::find_run(&database, completed.run_id()).await),
        completed_before
    );
    assert_eq!(
        must(crate::find_run(&database, first.run_id()).await).cancellation_requested_at(),
        Some(at(1))
    );
    let parked = must(crate::find_run(&database, parked.run_id()).await);
    assert_eq!(parked.state(), RunState::Cancelled);
    let events = must(
        crate::read_run_events(
            &database,
            parked.id(),
            must(jarvis_core::ReplayRequest::new(
                jarvis_core::RunEventSequence::first(),
                100,
            )),
        )
        .await,
    );
    assert_eq!(
        events
            .iter()
            .filter(|event| event.kind() == jarvis_core::RunEventKind::RunCancelled)
            .count(),
        1
    );
    database.close().await;
}

#[tokio::test]
async fn restart_preserves_child_roles_and_never_replays_a_family() {
    let (profile, database) = database().await;
    let parent = root(&database, "parent").await;
    let active = child(&database, parent.run_id()).await;
    let parked = child(&database, parent.run_id()).await;
    park(&database, parked.run_id()).await;
    database.close().await;
    let reopened = must(SqliteDatabase::open(&profile.database_path()).await);
    let recovered = must(crate::recover_interrupted_runs(&reopened, at(3)).await);
    assert!(recovered.contains(&parent.run_id().to_owned()));
    assert!(recovered.contains(&active.run_id().to_owned()));
    let active = must(crate::find_run(&reopened, active.run_id()).await);
    assert_eq!(active.error_code(), Some(crate::INTERRUPTED_ERROR_CODE));
    assert_eq!(active.parent_run_id(), Some(parent.run_id()));
    assert!(active.is_delegated());
    assert_eq!(
        must(crate::find_run(&reopened, parked.run_id()).await).state(),
        RunState::AwaitingApproval
    );
    assert!(
        must(crate::continuable_interrupted_runs(&reopened, &recovered, at(0), &[], 10).await)
            .is_empty()
    );
    // A failed recovered parent still names the outstanding parked work and can stop it.
    must(crate::request_run_cancellation(&reopened, parent.run_id(), at(4)).await);
    assert_eq!(
        must(crate::find_run(&reopened, parked.run_id()).await).state(),
        RunState::Cancelled
    );
    assert!(must(crate::recover_interrupted_runs(&reopened, at(5)).await).is_empty());
    reopened.close().await;
}

#[tokio::test]
async fn crash_after_parent_stop_recovers_children_as_cancelled() {
    let (profile, database) = database().await;
    let parent = root(&database, "parent").await;
    let child = child(&database, parent.run_id()).await;
    must(crate::request_run_cancellation(&database, parent.run_id(), at(2)).await);
    database.close().await;
    let reopened = must(SqliteDatabase::open(&profile.database_path()).await);
    must(crate::recover_interrupted_runs(&reopened, at(3)).await);
    for id in [parent.run_id(), child.run_id()] {
        let stored = must(crate::find_run(&reopened, id).await);
        assert_eq!(stored.state(), RunState::Cancelled);
        assert_eq!(stored.cancellation_requested_at(), Some(at(2)));
    }
    reopened.close().await;
}

#[tokio::test]
async fn parent_stop_withdraws_pending_and_approved_child_effects_only() {
    let (_profile, database) = database().await;
    let parent = root(&database, "parent").await;
    let child = child(&database, parent.run_id()).await;
    let unrelated = root(&database, "unrelated approval").await;
    park(&database, child.run_id()).await;
    let mut approvals = Vec::new();
    for (run, approved) in [
        (&child, false),
        (&child, true),
        (&parent, false),
        (&parent, true),
        (&unrelated, false),
    ] {
        let request = must(jarvis_core::ApprovalRequest::new(
            jarvis_core::ApprovalRequestParts {
                id: jarvis_core::ApprovalId::new(),
                workspace_id: must(run.workspace_id().parse()),
                run_id: must(run.run_id().parse()),
                actor_id: crate::LOCAL_USER_ID.to_owned(),
                tool: "jarvis.mail.send".to_owned(),
                tool_version: "1.0.0".to_owned(),
                intent: must(jarvis_core::CanonicalIntentHash::compute(
                    "jarvis.mail.send",
                    "1.0.0",
                    &serde_json::json!({"approved_fixture": approved}),
                )),
                preview: "Send the report".to_owned(),
                risk_level: 2,
                correlation_id: CorrelationId::new(),
                created_at: at(0),
                expires_at: at(10),
            },
        ));
        let id = request.id().to_string();
        must(crate::create_approval(&database, &request).await);
        must(crate::attach_approval_arguments(&database, &id, "{}").await);
        if approved {
            let decision = must(jarvis_core::ApprovalDecision::new(
                jarvis_core::ApprovalDecisionOutcome::Approve,
                jarvis_core::ApprovalChannel::Desktop,
                at(1),
                "owner",
            ));
            must(crate::record_decision(&database, &id, &decision).await);
        }
        approvals.push(id);
    }
    finish_fixture(&database, parent.run_id()).await;
    must(crate::request_run_cancellation(&database, parent.run_id(), at(2)).await);
    let state: String = must(
        sqlx::query_scalar("SELECT state FROM approvals WHERE id = ?1")
            .bind(&approvals[0])
            .fetch_one(database.pool())
            .await,
    );
    assert_eq!(state, "expired");
    for id in &approvals[..4] {
        assert!(must(crate::read_approval_arguments(&database, id).await).is_none());
    }
    assert_eq!(
        must(crate::read_approval_arguments(&database, &approvals[4]).await),
        Some("{}".to_owned())
    );
    assert_eq!(
        must(crate::find_run(&database, child.run_id()).await).state(),
        RunState::Cancelled
    );
    database.close().await;
}

#[path = "delegation_coordination_tests.rs"]
mod coordination;
