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
    manager.stop(&json!({"task_id":id})).unwrap();
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
        m.stop(&json!({"task_id":id})).unwrap()["outcome"],
        "stop_requested"
    );
    let done = wait(&m, &id).await;
    assert_eq!(done["status"], "cancelled");
    assert!(m.stop(&json!({"task_id":id})).is_err());
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
        manager.stop(&json!({"task_id":id})).unwrap()["outcome"],
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
    assert!(manager.stop(&json!({"task_id":id})).is_err());
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
    manager.stop(&json!({"task_id":id})).unwrap();
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
    manager.stop(&json!({"task_id":second_id})).unwrap();
    assert_eq!(wait(&manager, second_id).await["status"], "cancelled");
    assert!(!scratch.0.join("repo/must-not-exist.txt").exists());
    manager.stop(&json!({"task_id":first_id})).unwrap();
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
    manager.stop(&json!({"task_id":id})).unwrap();
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
