//! A request to stop the daemon: `POST /api/v1/shutdown`.
//!
//! It wakes the **same** graceful path an operating-system signal does (transports stop accepting, MCP connections
//! close, the database closes, the single-instance lock is released), so `jarvis stop` and a Ctrl-C end the same way.
//! The route sits behind the local credential like the rest of the API; nothing else can ask for it.
//!
//! # A stop does not silently throw work away
//!
//! A task the daemon is driving is lost to a stop (it is settled as interrupted on the next start). So a stop or a restart that
//! would interrupt native runs, queued/running coding jobs or managed processes is **refused with counts** unless the caller says
//! `force=true`. An unreadable work state also refuses the stop, without claiming the daemon is idle. A native run waiting for an
//! approval is not counted: that wait is durable and survives. This is a preflight snapshot, not an atomic admission/drain barrier.

use std::time::Duration;

use axum::{
    Json,
    extract::{RawQuery, State},
    http::StatusCode,
    response::IntoResponse,
    response::Response,
};
use serde_json::json;
use tokio::sync::Notify;

use crate::gateway::GatewayState;

static STOP: Notify = Notify::const_new();

/// Completes when a stop has been requested over the API.
pub(crate) async fn requested() {
    STOP.notified().await;
}

/// Whether a query string carries `force=true`.
pub(crate) fn wants_force(raw: Option<&str>) -> bool {
    raw.is_some_and(|query| query.split('&').any(|pair| pair == "force=true"))
}

/// The refusal to give when tasks are working and the caller did not force, or `None` to go ahead.
pub(crate) async fn refusal_if_working(state: &GatewayState, force: bool) -> Option<Response> {
    if force {
        return None;
    }
    let work = match jarvis_storage::read_shutdown_work(state.database()).await {
        Ok(work) => work,
        Err(error) => {
            tracing::warn!(%error, "could not inspect work before stopping");
            return Some(work_state_unavailable());
        }
    };
    let managed_processes = match state.active_process_count() {
        Ok(count) => u32::try_from(count).unwrap_or(u32::MAX),
        Err(reason) => {
            tracing::warn!(
                reason,
                "could not inspect managed processes before stopping"
            );
            return Some(work_state_unavailable());
        }
    };
    let working = work
        .runs
        .saturating_add(work.coding_jobs)
        .saturating_add(managed_processes);
    if working == 0 {
        return None;
    }
    let message = format!(
        "Stopping now would interrupt unfinished work (agent runs: {}, coding jobs: {}, managed processes: {managed_processes}). The daemon was left running.",
        work.runs, work.coding_jobs
    );
    Some(
        (
            StatusCode::CONFLICT,
            Json(json!({ "code": "runs_in_flight", "in_flight": working, "agent_runs": work.runs,
                "coding_jobs": work.coding_jobs, "managed_processes": managed_processes, "message": message })),
        )
            .into_response(),
    )
}

fn work_state_unavailable() -> Response {
    (
        StatusCode::SERVICE_UNAVAILABLE,
        Json(json!({
            "code": "work_state_unavailable",
            "message": "Work state could not be checked. The daemon was left running; retry when its state is readable, or use force=true only if you intend to interrupt any unfinished work."
        })),
    ).into_response()
}

/// `POST /api/v1/shutdown[?force=true]`
pub async fn request(State(state): State<GatewayState>, RawQuery(raw): RawQuery) -> Response {
    if let Some(refusal) = refusal_if_working(&state, wants_force(raw.as_deref())).await {
        return refusal;
    }
    tracing::info!("a stop was requested over the API");
    // After the reply has had time to leave, so the caller is told "stopping" rather than seeing the connection drop.
    tokio::spawn(async {
        tokio::time::sleep(Duration::from_millis(200)).await;
        STOP.notify_one();
    });
    (StatusCode::ACCEPTED, Json(json!({ "stopping": true }))).into_response()
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use jarvis_core::{RunState, RunTransition, SystemClock, UtcTimestamp};
    use jarvis_storage::{DelegatedTaskFinish, NewDelegatedTask};
    use std::sync::Arc;

    use super::*;

    async fn state() -> (GatewayState, std::path::PathBuf) {
        let tag = jarvis_core::scratch_tag();
        let path = std::env::temp_dir().join(format!("jst-{}", &tag[tag.len() - 12..]));
        std::fs::create_dir_all(&path).unwrap_or_else(|error| panic!("{error}"));
        let database = Arc::new(
            jarvis_storage::SqliteDatabase::test_fixture(&path.join("jarvis.sqlite3"))
                .await
                .unwrap_or_else(|error| panic!("{error}")),
        );
        let credential =
            jarvis_core::ClientCredential::generate().unwrap_or_else(|error| panic!("{error}"));
        (GatewayState::new(database, credential), path)
    }

    /// A request wakes whoever is waiting for a stop, which is what lets the daemon's shutdown select complete.
    #[tokio::test]
    async fn a_request_wakes_the_waiter() {
        let (state, path) = state().await;
        let waiter = tokio::spawn(requested());
        let reply = request(State(state), RawQuery(None)).await;
        assert_eq!(reply.status(), StatusCode::ACCEPTED);
        let woke = tokio::time::timeout(Duration::from_secs(3), waiter).await;
        assert!(
            woke.is_ok(),
            "the waiter must be woken within the grace period"
        );
        jarvis_core::remove_scratch_dir(&path);
    }

    #[test]
    fn only_force_true_forces() {
        assert!(wants_force(Some("force=true")));
        assert!(wants_force(Some("a=1&force=true")));
        assert!(!wants_force(Some("force=false")));
        assert!(!wants_force(Some("forced=true")));
        assert!(!wants_force(None));
    }

    /// **A stop that would interrupt a working task is refused with the count, forced past on request, and a parked task is not counted.**
    #[tokio::test]
    async fn a_stop_that_would_interrupt_work_is_refused_unless_forced() {
        let (state, path) = state().await;
        assert!(
            refusal_if_working(&state, false).await.is_none(),
            "nothing is working"
        );

        crate::run_service::RunService::new(state.database_handle())
            .start(&jarvis_protocol::StartRunRequest {
                objective: "work".to_owned(),
                session_id: None,
                idempotency_key: None,
                project_id: None,
            })
            .await
            .unwrap_or_else(|_| panic!("start a run"));
        let refusal = refusal_if_working(&state, false)
            .await
            .unwrap_or_else(|| panic!("a run that has started is working"));
        assert_eq!(refusal.status(), StatusCode::CONFLICT);
        assert!(
            refusal_if_working(&state, true).await.is_none(),
            "force goes past it"
        );
        jarvis_core::remove_scratch_dir(&path);
    }

    async fn start_run(state: &GatewayState, objective: &str) -> jarvis_storage::StoredRun {
        let reply = crate::run_service::RunService::new(state.database_handle())
            .start(&jarvis_protocol::StartRunRequest {
                objective: objective.to_owned(),
                session_id: None,
                idempotency_key: None,
                project_id: None,
            })
            .await
            .unwrap();
        jarvis_storage::find_run(state.database(), &reply.run_id)
            .await
            .unwrap()
    }

    async fn advance(
        state: &GatewayState,
        run: jarvis_storage::StoredRun,
        next: RunState,
    ) -> jarvis_storage::StoredRun {
        jarvis_storage::transition_run(
            state.database(),
            run.id(),
            run.expectation(),
            &RunTransition::new(next, next.required_outcome(), None).unwrap(),
            UtcTimestamp::now(&SystemClock),
        )
        .await
        .unwrap()
    }

    async fn body(response: Response) -> serde_json::Value {
        serde_json::from_slice(
            &axum::body::to_bytes(response.into_body(), 16_384)
                .await
                .unwrap(),
        )
        .unwrap()
    }

    #[tokio::test]
    async fn queued_and_running_coding_jobs_block_after_parent_completion_but_terminal_jobs_do_not()
    {
        let (state, path) = state().await;
        let mut parent = start_run(&state, "delegate coding").await;
        for next in [
            RunState::ContextBuilding,
            RunState::Planning,
            RunState::Executing,
            RunState::Observing,
            RunState::Responding,
            RunState::Completed,
        ] {
            parent = advance(&state, parent, next).await;
        }
        for terminal in ["completed", "failed", "cancelled", "interrupted"] {
            let job = jarvis_storage::create_delegated_task(
                state.database(),
                &NewDelegatedTask {
                    workspace_id: jarvis_storage::LOCAL_WORKSPACE_ID,
                    repository: "fixture",
                    task: "background coding",
                    allow_commands: false,
                    minutes: 0,
                    now: UtcTimestamp::now(&SystemClock),
                },
            )
            .await
            .unwrap();
            for running in [false, true] {
                if running {
                    jarvis_storage::mark_delegated_task_running(
                        state.database(),
                        jarvis_storage::LOCAL_WORKSPACE_ID,
                        &job.id,
                        UtcTimestamp::now(&SystemClock),
                    )
                    .await
                    .unwrap();
                }
                let refusal = request(State(state.clone()), RawQuery(None)).await;
                assert_eq!(refusal.status(), StatusCode::CONFLICT);
                let counts = body(refusal).await;
                assert_eq!(counts["agent_runs"], 0);
                assert_eq!(counts["coding_jobs"], 1);
                assert_eq!(counts["in_flight"], 1);
                assert!(refusal_if_working(&state, true).await.is_none());
            }
            jarvis_storage::finish_delegated_task(
                state.database(),
                jarvis_storage::LOCAL_WORKSPACE_ID,
                &job.id,
                &DelegatedTaskFinish {
                    status: terminal,
                    branch: None,
                    worktree: None,
                    summary: None,
                    result_json: None,
                    error: None,
                    now: UtcTimestamp::now(&SystemClock),
                },
            )
            .await
            .unwrap();
            assert!(
                refusal_if_working(&state, false).await.is_none(),
                "terminal status: {terminal}"
            );
        }
        state.database().close().await;
        jarvis_core::remove_scratch_dir(&path);
    }

    #[tokio::test]
    async fn native_children_count_but_durable_approval_waits_survive_without_blocking() {
        let (state, path) = state().await;
        let mut child = start_run(
            &state,
            &format!("{}review", crate::delegate::DELEGATED_NOTICE),
        )
        .await;
        let refusal = refusal_if_working(&state, false).await.unwrap();
        assert_eq!(body(refusal).await["agent_runs"], 1);
        for next in [
            RunState::ContextBuilding,
            RunState::Planning,
            RunState::AwaitingApproval,
        ] {
            child = advance(&state, child, next).await;
        }
        assert!(refusal_if_working(&state, false).await.is_none());
        state.database().close().await;
        let reopened = jarvis_storage::SqliteDatabase::open(&path.join("jarvis.sqlite3"))
            .await
            .unwrap();
        assert_eq!(
            jarvis_storage::find_run(&reopened, child.id())
                .await
                .unwrap()
                .state(),
            RunState::AwaitingApproval
        );
        reopened.close().await;
        jarvis_core::remove_scratch_dir(&path);
    }

    #[tokio::test]
    async fn unreadable_work_state_refuses_shutdown_without_an_idle_count_but_force_remains_explicit()
     {
        let (state, path) = state().await;
        state.database().close().await;
        let response = request(State(state.clone()), RawQuery(None)).await;
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        let response = body(response).await;
        assert_eq!(response["code"], "work_state_unavailable");
        assert!(response.get("in_flight").is_none());
        assert!(refusal_if_working(&state, true).await.is_none());
        jarvis_core::remove_scratch_dir(&path);
    }
}
