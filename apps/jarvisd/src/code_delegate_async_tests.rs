#![allow(clippy::unwrap_used, clippy::cloned_ref_to_slice_refs)]

use std::path::{Path, PathBuf};
use std::process::Command;

use super::*;
use crate::code_agents::AgentSettings;
use crate::code_delegate::AgentSource;

struct Scratch(PathBuf);
impl Drop for Scratch {
    fn drop(&mut self) {
        jarvis_core::remove_scratch_dir(&self.0);
    }
}
fn fake() -> PathBuf {
    crate::code_agents::test_agent_executable()
}

async fn launch_origin(db: &Arc<SqliteDatabase>) -> (String, String) {
    use jarvis_core::{CorrelationId, ToolOutcome};
    use jarvis_storage::{CallBinding, CallOrigin, CallTarget, NewToolCall};
    let parent = crate::run_service::RunService::new(Arc::clone(db))
        .start(&jarvis_protocol::StartRunRequest {
            objective: "parent".into(),
            session_id: None,
            idempotency_key: None,
            project_id: None,
        })
        .await
        .unwrap();
    let id = jarvis_core::RunId::new().to_string();
    let now = UtcTimestamp::now(&SystemClock);
    let call = NewToolCall::new(
        &id,
        CallOrigin::new(&parent.workspace_id, &parent.run_id, None),
        CallTarget::new(START, "1.1.0"),
        CallBinding::new("a".repeat(64), id.replace('-', ""), "{}", "policy-1", None),
        CorrelationId::new(),
        now,
    )
    .unwrap();
    jarvis_storage::admit_tool_call(db, &call).await.unwrap();
    for state in [ToolOutcome::Authorized, ToolOutcome::Submitted] {
        jarvis_storage::advance_tool_call(db, &id, state, now)
            .await
            .unwrap();
    }
    (parent.run_id, id)
}

#[tokio::test]
async fn duplicate_coding_dispatch_returns_one_worker_and_durable_stop_crosses_managers() {
    let (scratch, db, manager) = rig("write:{cwd}/once.txt,progress,wait").await;
    let (_parent, call) = launch_origin(&db).await;
    let args = json!({"repository":"repo","task":"one worker","mode":"in_place"});
    let (a, b) = tokio::join!(
        manager.start_for_call(&args, Some(&call)),
        manager.start_for_call(&args, Some(&call))
    );
    let (a, b) = (a.unwrap(), b.unwrap());
    assert_eq!(a["task_id"], b["task_id"]);
    assert_ne!(a["outcome"], b["outcome"]);
    assert_eq!(manager.cancellations.lock().unwrap().len(), 1);
    let id = a["task_id"].as_str().unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(30), async {
        while !scratch.0.join("repo/once.txt").exists() {
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
    })
    .await
    .unwrap();
    // A second manager has no in-memory Notify for the running worker.
    let other = AsyncCodeDelegateTool::new(
        Arc::clone(&manager.inner),
        Arc::clone(&db),
        manager.workspace.clone(),
    )
    .without_notifications();
    assert!(other.cancellations.lock().unwrap().is_empty());
    other.stop(&json!({"task_id":id})).await.unwrap();
    assert_eq!(wait(&manager, id).await["status"], "cancelled");
    std::fs::remove_file(scratch.0.join("repo/once.txt")).unwrap();
    let replay = other.start_for_call(&args, Some(&call)).await.unwrap();
    assert_eq!(replay["outcome"], "existing");
    assert!(other.cancellations.lock().unwrap().is_empty());
    assert!(!scratch.0.join("repo/once.txt").exists());
    assert_eq!(
        jarvis_storage::list_delegated_tasks(&db, &manager.workspace, 50)
            .await
            .unwrap()
            .len(),
        1
    );
    db.close().await;
}

#[tokio::test]
async fn parent_stop_cancels_queued_coding_without_an_agent_or_local_notification() {
    let (scratch, db, manager) = rig("write:{cwd}/should-not-exist.txt").await;
    let cancel = Notify::new();
    let held = manager.inner.acquire(&cancel).await.unwrap().unwrap();
    let (parent, call) = launch_origin(&db).await;
    let started = manager
        .start_for_call(
            &json!({"repository":"repo","task":"queued child"}),
            Some(&call),
        )
        .await
        .unwrap();
    jarvis_storage::request_run_cancellation(&db, &parent, UtcTimestamp::now(&SystemClock))
        .await
        .unwrap();
    let done = wait(&manager, started["task_id"].as_str().unwrap()).await;
    assert_eq!(done["status"], "cancelled");
    assert_eq!(done["parent_run_id"], parent);
    drop(held);
    assert!(!scratch.0.join("trees").exists());
    assert!(!scratch.0.join("repo/should-not-exist.txt").exists());
    db.close().await;
}

#[tokio::test]
async fn unreadable_cancellation_state_stops_the_queued_worker_without_dispatch() {
    let (scratch, db, manager) = rig("write:{cwd}/should-not-exist.txt").await;
    let cancel = Notify::new();
    let held = manager.inner.acquire(&cancel).await.unwrap().unwrap();
    manager
        .start(&json!({"repository":"repo","task":"unreadable Stop state"}))
        .await
        .unwrap();
    db.close().await;
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            if manager.cancellations.lock().unwrap().is_empty() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
    })
    .await
    .unwrap();
    drop(held);
    assert!(!scratch.0.join("repo/should-not-exist.txt").exists());
}
fn git(dir: &Path, args: &[&str]) {
    let o = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args([
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@t",
            "-c",
            "commit.gpgsign=false",
        ])
        .args(args)
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
}
async fn rig(script: &str) -> (Scratch, Arc<SqliteDatabase>, AsyncCodeDelegateTool) {
    // Canonical macOS temporary paths contain `private`; reproduce that on every platform.
    let root = std::env::temp_dir().join(format!("jasync-private-{}", jarvis_core::scratch_tag()));
    std::fs::create_dir_all(root.join("repo")).unwrap();
    git(&root.join("repo"), &["init", "-q", "-b", "main"]);
    std::fs::write(root.join("repo/a.txt"), "a\n").unwrap();
    git(&root.join("repo"), &["add", "-A"]);
    git(&root.join("repo"), &["commit", "-q", "-m", "first"]);
    let db = Arc::new(
        SqliteDatabase::open(&root.join("jarvis.sqlite3"))
            .await
            .unwrap(),
    );
    let identity = jarvis_storage::load_local_identity(&db).await.unwrap();
    let inner = Arc::new(
        CodeDelegateTool::new(
            &[root.clone()],
            root.join("trees"),
            AgentSource::Fixed(AgentSettings {
                command: vec![
                    fake().to_string_lossy().into_owned(),
                    script.replace("{root}", &root.to_string_lossy()),
                ],
                ..AgentSettings::default()
            }),
        )
        .unwrap(),
    );
    let manager =
        AsyncCodeDelegateTool::new(inner, Arc::clone(&db), identity.workspace_id().into())
            .without_notifications();
    (Scratch(root), db, manager)
}
async fn wait(manager: &AsyncCodeDelegateTool, id: &str) -> Value {
    tokio::time::timeout(std::time::Duration::from_secs(30), async {
        loop {
            let v = manager.status(&json!({"task_id":id})).await.unwrap();
            if !matches!(v["status"].as_str(), Some("queued" | "running")) {
                break v;
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("task {id} did not end within 30 seconds"))
}

#[cfg(windows)]
fn process_running(pid: u32) -> bool {
    let output = Command::new("tasklist")
        .args(["/FI", &format!("PID eq {pid}"), "/FO", "CSV", "/NH"])
        .output()
        .unwrap();
    assert!(output.status.success());
    String::from_utf8_lossy(&output.stdout).contains(&format!("\"{pid}\""))
}

#[cfg(unix)]
fn process_running(pid: u32) -> bool {
    Command::new("kill")
        .args(["-s", "0", &pid.to_string()])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .unwrap()
        .success()
}

#[tokio::test]
async fn live_activity_is_persisted_before_completion_and_fenced_for_models() {
    let (_scratch, db, manager) = rig("progress,wait").await;
    let started = manager
        .start(&json!({"repository":"repo", "task":"read config", "minutes":20}))
        .await
        .unwrap();
    let id = started["task_id"].as_str().unwrap();
    let task = tokio::time::timeout(std::time::Duration::from_secs(30), async {
        loop {
            let task = jarvis_storage::find_delegated_task(&db, &manager.workspace, id)
                .await
                .unwrap();
            // Status and activity are separate reads: the worker can leave the queue and publish
            // progress between them. Wait for both facts before cancelling the agent.
            if task.status == "running" && task.activity.len() == 2 {
                break task;
            }
            assert!(
                matches!(task.status.as_str(), "queued" | "running"),
                "the agent ended before both activity updates were observed: {task:?}"
            );
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("both activity updates must be visible before the agent ends"));
    assert_eq!(task.status, "running");
    assert!(task.result_json.is_none());
    assert!(task.activity[0].summary.contains("a.txt"));
    assert!(task.activity[0].summary.contains("jasync-private-"));
    assert!(
        task.activity.iter().all(|entry| [
            "private thought",
            "private input",
            "private output",
            "unrelated session",
            "Internal planning",
        ]
        .iter()
        .all(|hidden| !entry.summary.contains(hidden))),
        "{:?}",
        task.activity
    );
    let model = manager.status(&json!({"task_id":id})).await.unwrap();
    let console = render_console(&task);
    assert_eq!(console["activity"][0]["summary"], task.activity[0].summary);
    assert_eq!(
        model["activity"][0]["summary"],
        IsolatedText::new(&task.activity[0].summary)
            .unwrap()
            .render()
    );
    assert_eq!(console["activity"][0]["sequence"], 1);
    manager.stop(&json!({"task_id":id})).await.unwrap();
    let done = wait(&manager, id).await;
    assert_eq!(done["status"], "cancelled");
    assert!(!done["activity"].as_array().unwrap().is_empty());
    assert_eq!(done["result"]["activity_updates"], 2);
    assert!(done["result"]["activity_error"].is_null());
    db.close().await;
}

#[tokio::test]
async fn start_returns_immediately_and_the_result_becomes_durable() {
    let (_s, _db, m) = rig("write:{cwd}/done.txt,say:finished").await;
    let started = m
        .start(&json!({"repository":"repo","task":"add done","minutes":20}))
        .await
        .unwrap();
    assert_eq!(started["outcome"], "started");
    let id = started["task_id"].as_str().unwrap();
    let done = wait(&m, id).await;
    assert_eq!(done["status"], "completed");
    assert!(done["branch"].is_null());
    assert_eq!(done["mode"], "in_place");
    assert_eq!(done["result"]["verification"]["status"], "not_run");
    assert_eq!(m.list(&json!({})).await.unwrap()["count"], 1);
}

#[tokio::test]
async fn stop_cancels_the_agent_and_keeps_a_durable_record() {
    let (_s, _db, m) = rig("wait").await;
    let started = m
        .start(&json!({"repository":"repo","task":"wait","minutes":20}))
        .await
        .unwrap();
    let id = started["task_id"].as_str().unwrap().to_owned();
    tokio::time::sleep(std::time::Duration::from_millis(150)).await;
    assert_eq!(
        m.stop(&json!({"task_id":id})).await.unwrap()["outcome"],
        "stop_requested"
    );
    let done = wait(&m, &id).await;
    assert_eq!(done["status"], "cancelled");
    assert!(m.stop(&json!({"task_id":id})).await.is_err());
}

#[tokio::test]
async fn agent_exit_or_error_during_stop_keeps_durable_cancellation() {
    for ending in ["cancel_exit", "cancel_error", "cancel_auth"] {
        let (_scratch, db, manager) = rig(&format!("progress,{ending}")).await;
        let started = manager
            .start(&json!({"repository":"repo", "task":"wait for Stop"}))
            .await
            .unwrap();
        let id = started["task_id"].as_str().unwrap();
        tokio::time::timeout(std::time::Duration::from_secs(30), async {
            loop {
                let task = jarvis_storage::find_delegated_task(&db, &manager.workspace, id)
                    .await
                    .unwrap();
                if task.activity.len() == 2 {
                    break;
                }
                assert!(
                    matches!(task.status.as_str(), "queued" | "running"),
                    "{ending}: {task:?}"
                );
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            }
        })
        .await
        .unwrap();
        manager.stop(&json!({"task_id":id})).await.unwrap();
        let done = wait(&manager, id).await;
        assert_eq!(done["status"], "cancelled", "{ending}: {done}");
        assert_eq!(done["result"]["outcome"], "cancelled", "{ending}: {done}");
        assert!(done["finished_at"].is_string());
        db.close().await;
    }
}

#[tokio::test]
async fn stop_before_the_worker_starts_creates_no_worktree() {
    let (scratch, db, manager) = rig("write:{cwd}/done.txt,say:finished").await;
    let started = manager
        .start(&json!({"repository":"repo","task":"add done","minutes":20}))
        .await
        .unwrap();
    let id = started["task_id"].as_str().unwrap();
    assert_eq!(
        manager.stop(&json!({"task_id":id})).await.unwrap()["outcome"],
        "stop_requested"
    );
    let done = wait(&manager, id).await;
    assert!(
        !scratch.0.join("trees").exists(),
        "a task stopped before its worker starts must not create a worktree: {done}"
    );
    assert_eq!(done["status"], "cancelled");
    assert_eq!(done["result"]["outcome"], "cancelled");
    assert!(done["worktree"].is_null());
    assert!(done["finished_at"].is_string());
    assert!(done["activity"].as_array().unwrap().is_empty());
    assert!(manager.stop(&json!({"task_id":id})).await.is_err());
    db.close().await;
}

#[tokio::test]
async fn stop_during_initialization_terminates_the_agent_and_its_child() {
    let (scratch, db, manager) =
        rig("initialize_wait:{root}/setup.json,initialize_child,write:{cwd}/done.txt").await;
    let marker = scratch.0.join("setup.json");
    let started = manager
        .start(&json!({"repository":"repo","task":"add done","minutes":20}))
        .await
        .unwrap();
    let id = started["task_id"].as_str().unwrap();
    let setup: Value = tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            if let Ok(text) = std::fs::read_to_string(&marker)
                && let Ok(setup) = serde_json::from_str(&text)
            {
                break setup;
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
    })
    .await
    .unwrap();
    let pid = u32::try_from(setup["pid"].as_u64().unwrap()).unwrap();
    let child_pid = u32::try_from(setup["child_pid"].as_u64().unwrap()).unwrap();
    let _parent_cleanup = crate::command_run::TreeGuard { pid, armed: true };
    let _child_cleanup = crate::command_run::TreeGuard {
        pid: child_pid,
        armed: true,
    };
    assert!(process_running(pid));
    assert!(process_running(child_pid));
    manager.stop(&json!({"task_id":id})).await.unwrap();
    let done = wait(&manager, id).await;
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        while process_running(pid) || process_running(child_pid) {
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("the setup process tree must exit within five seconds of Stop"));
    assert!(!process_running(pid), "the setup process survived Stop");
    assert!(
        !process_running(child_pid),
        "the setup process's child survived Stop"
    );
    assert_eq!(done["status"], "cancelled");
    assert_eq!(done["result"]["outcome"], "cancelled");
    assert!(done["worktree"].is_null());
    assert!(scratch.0.join("repo").exists());
    assert!(!scratch.0.join("repo/done.txt").exists());
    assert!(done["activity"].as_array().unwrap().is_empty());
    db.close().await;
}

#[test]
fn definitions_separate_start_read_and_stop() {
    let d = AsyncCodeDelegateTool::definitions().unwrap();
    assert_eq!(d.len(), 7);
    let start = d.iter().find(|x| x.id().to_string() == START).unwrap();
    assert_eq!(start.approval(), ApprovalPolicy::Ask);
    let status = d.iter().find(|x| x.id().to_string() == STATUS).unwrap();
    assert_eq!(status.approval(), ApprovalPolicy::Auto);
}

#[tokio::test]
async fn an_interrupted_task_cannot_launch_an_agent() {
    let (scratch, db, manager) = rig("write:{cwd}/done.txt,say:finished").await;
    let args =
        Arguments::parse(&json!({"repository":"repo","task":"add done","minutes":20})).unwrap();
    let stored = jarvis_storage::create_delegated_task(
        &db,
        &NewDelegatedTask {
            workspace_id: &manager.workspace,
            repository: &args.repository,
            task: &args.task,
            allow_commands: args.allow_commands,
            minutes: args.minutes,
            now: UtcTimestamp::now(&SystemClock),
        },
    )
    .await
    .unwrap();
    jarvis_storage::interrupt_delegated_tasks(&db, UtcTimestamp::now(&SystemClock))
        .await
        .unwrap();
    let result = run_recorded_task(
        &db,
        &manager.workspace,
        &stored.id,
        &manager.inner,
        &args,
        &Notify::new(),
    )
    .await;
    assert!(matches!(
        result,
        Err(AdapterError::RefusedBeforeReaching { .. })
    ));
    assert!(!scratch.0.join("trees").exists());
    assert_eq!(
        manager.status(&json!({"task_id":stored.id})).await.unwrap()["status"],
        "interrupted"
    );
    db.close().await;
}

#[tokio::test]
async fn queued_tasks_can_be_cancelled_without_starting_an_agent() {
    let (scratch, db, manager) = rig("write:{cwd}/must-not-exist.txt").await;
    let cancel = Notify::new();
    let held = manager.inner.acquire(&cancel).await.unwrap().unwrap();
    let first = manager
        .start(&json!({"repository":"repo","task":"first"}))
        .await
        .unwrap();
    let second = manager
        .start(&json!({"repository":"repo","task":"second"}))
        .await
        .unwrap();
    let first_id = first["task_id"].as_str().unwrap();
    let second_id = second["task_id"].as_str().unwrap();
    assert_eq!(
        manager.status(&json!({"task_id":second_id})).await.unwrap()["status"],
        "queued"
    );
    manager.stop(&json!({"task_id":second_id})).await.unwrap();
    assert_eq!(wait(&manager, second_id).await["status"], "cancelled");
    assert!(!scratch.0.join("repo/must-not-exist.txt").exists());
    manager.stop(&json!({"task_id":first_id})).await.unwrap();
    assert_eq!(wait(&manager, first_id).await["status"], "cancelled");
    drop(held);
    db.close().await;
}

#[tokio::test]
async fn continuation_reuses_the_saved_directory_and_capabilities_without_replaying_a_process() {
    let (_scratch, db, manager) = rig("say:continued").await;
    let directory = manager
        .inner
        .resolve("repo")
        .unwrap()
        .to_string_lossy()
        .into_owned();
    let request = json!({"repository":directory,"task":"finish remaining work","mode":"in_place","allow_commands":false,"allow_network":false,"minutes":0});
    let previous = jarvis_storage::create_delegated_task_with_request(
        &db,
        &NewDelegatedTask {
            workspace_id: &manager.workspace,
            repository: &directory,
            task: "finish remaining work",
            allow_commands: false,
            minutes: 0,
            now: UtcTimestamp::now(&SystemClock),
        },
        Some(&request.to_string()),
    )
    .await
    .unwrap();
    jarvis_storage::interrupt_delegated_tasks(&db, UtcTimestamp::now(&SystemClock))
        .await
        .unwrap();
    let answer = manager
        .resume(&json!({"task_id":previous.id}))
        .await
        .unwrap();
    assert_eq!(answer["continued_from"], previous.id);
    let task_id = answer["task_id"].as_str().unwrap();
    let done = wait(&manager, task_id).await;
    assert_eq!(done["status"], "completed");
    let saved = jarvis_storage::find_delegated_task(&db, &manager.workspace, task_id)
        .await
        .unwrap();
    let saved: Value = serde_json::from_str(saved.request_json.as_deref().unwrap()).unwrap();
    assert_eq!(saved["repository"], directory);
    assert_eq!(saved["allow_commands"], false);
    assert_eq!(saved["allow_network"], false);
    assert_eq!(saved["minutes"], 0);
    assert!(saved["task"].as_str().unwrap().contains("do not replay"));
    assert!(manager.resume(&json!({"task_id":task_id})).await.is_err());
    db.close().await;
}

#[test]
fn large_agent_reports_remain_valid_json_without_changing_verification_status() {
    let value = json!({"outcome":"verification_failed","agent_message":"x".repeat(300_000),
        "verification":{"status":"failed","checks":[{"passed":false,"command":{"program":"npm"}}]}});
    let stored = completion(Ok(value));
    assert_eq!(stored.status, "failed");
    let stored: Value = serde_json::from_str(stored.result_json.as_deref().unwrap()).unwrap();
    assert_eq!(stored["verification"]["status"], "failed");
    assert_eq!(stored["details_omitted"], true);
    let output = crate::skill_bundles::tool_output(
        &json!({"task_id":"retained","task":"x".repeat(100_000)}),
    );
    assert!(output.is_truncated());
    let parsed: Value = serde_json::from_str(output.content()).unwrap();
    assert_eq!(parsed["task_id"], "retained");
}

#[tokio::test]
async fn wait_timeout_leaves_quiet_agents_running_until_explicit_stop() {
    let (_scratch, db, manager) = rig("wait").await;
    let started = manager
        .start(&json!({"repository":"repo","task":"long build"}))
        .await
        .unwrap();
    let id = started["task_id"].as_str().unwrap();
    for _ in 0..2 {
        let view = manager
            .wait_for_task(&json!({"task_id":id,"seconds":1}))
            .await
            .unwrap();
        assert_eq!(view["wait_timed_out"], true);
        assert!(matches!(
            view["status"].as_str(),
            Some("queued" | "running")
        ));
        assert_eq!(view["minutes"], 0);
    }
    manager.stop(&json!({"task_id":id})).await.unwrap();
    let done = manager
        .wait_for_task(&json!({"task_id":id,"seconds":30}))
        .await
        .unwrap();
    assert_eq!(done["status"], "cancelled");
    assert_eq!(done["wait_timed_out"], false);
    db.close().await;
}

#[tokio::test]
async fn listing_fifty_long_briefs_retains_every_task_identity() {
    let (_scratch, db, manager) = rig("chat").await;
    let brief = "long brief ".repeat(1500);
    for _ in 0..50 {
        jarvis_storage::create_delegated_task(
            &db,
            &NewDelegatedTask {
                workspace_id: &manager.workspace,
                repository: "repo",
                task: &brief,
                allow_commands: true,
                minutes: 0,
                now: UtcTimestamp::now(&SystemClock),
            },
        )
        .await
        .unwrap();
    }
    let list = manager.list(&json!({"limit":50})).await.unwrap();
    let output = crate::skill_bundles::tool_output(&list);
    assert!(!output.is_truncated(), "{}", output.content());
    let parsed: Value = serde_json::from_str(output.content()).unwrap();
    assert_eq!(parsed["tasks"].as_array().unwrap().len(), 50);
    assert!(
        parsed["tasks"]
            .as_array()
            .unwrap()
            .iter()
            .all(|task| task["task_id"].as_str().is_some_and(|id| id.len() == 36))
    );
    db.close().await;
}

#[test]
fn status_compaction_preserves_failed_verification_with_large_unicode_evidence() {
    let near_byte_limit = json!({"activity":"界".repeat(10_800)});
    assert!(
        near_byte_limit.to_string().chars().count()
            < jarvis_tools::MAX_MODEL_FACING_RESULT_CHARS - 500
    );
    assert!(
        !fits_model(&near_byte_limit),
        "wait metadata needs a UTF-8 byte reserve too"
    );
    let mut view = json!({"task_id":"0198f000-0000-7000-8000-0000000000b3","status":"failed","repository":"界".repeat(20_000),
        "result":{"outcome":"verification_failed","agent_message":"界".repeat(100_000),"delivery_status":"verification_failed",
        "verification":{"status":"failed","checks":[{"passed":false,"result":{"exit_code":0,"stderr":"界".repeat(100_000),"reported_check_failures":["eslint_unavailable"]}}]}}});
    compact_status(&mut view);
    assert!(fits_model(&view));
    assert_eq!(view["task_id"], "0198f000-0000-7000-8000-0000000000b3");
    assert_eq!(view["result"]["verification"]["status"], "failed");
    assert_eq!(view["result"]["verification"]["checks"][0]["passed"], false);
    assert_eq!(
        view["result"]["verification"]["checks"][0]["reported_check_failure_count"],
        1
    );
}

#[tokio::test]
async fn shutdown_admission_blocks_queued_dispatch_after_capacity_is_released() {
    let (scratch, db, manager) = rig("write:{cwd}/must-not-exist.txt").await;
    let cancel = Notify::new();
    let held = manager.inner.acquire(&cancel).await.unwrap().unwrap();
    let task = manager
        .start(&json!({"repository":"repo","task":"queued at shutdown"}))
        .await
        .unwrap();
    let id = task["task_id"].as_str().unwrap();
    assert_eq!(
        manager.status(&json!({"task_id":id})).await.unwrap()["status"],
        "queued"
    );
    db.work_admission()
        .prepare_shutdown(true)
        .unwrap()
        .unwrap()
        .commit();
    drop(held);
    let done = wait(&manager, id).await;
    assert_eq!(done["status"], "cancelled");
    assert!(
        !scratch.0.join("trees").exists(),
        "no worktree or agent may start"
    );
    assert!(!scratch.0.join("repo/must-not-exist.txt").exists());
    assert!(
        manager
            .start(&json!({"repository":"repo","task":"late"}))
            .await
            .is_err()
    );
    db.close().await;
}
