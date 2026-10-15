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
    let exe = std::env::current_exe().unwrap();
    exe.parent()
        .and_then(Path::parent)
        .unwrap()
        .join("examples")
        .join(format!("fake_acp_agent{}", std::env::consts::EXE_SUFFIX))
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
            // Stopping after the first snapshot races the agent's second update against cancellation.
            if task.activity.len() == 2 {
                break task;
            }
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
    assert!(done["branch"].as_str().unwrap().starts_with("jarvis/"));
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
    assert!(!process_running(pid), "the setup process survived Stop");
    assert!(
        !process_running(child_pid),
        "the setup process's child survived Stop"
    );
    assert_eq!(done["status"], "cancelled");
    assert_eq!(done["result"]["outcome"], "cancelled");
    let worktree = Path::new(done["worktree"].as_str().unwrap());
    assert!(worktree.exists());
    assert!(!worktree.join("done.txt").exists());
    assert!(done["activity"].as_array().unwrap().is_empty());
    db.close().await;
}

#[test]
fn definitions_separate_start_read_and_stop() {
    let d = AsyncCodeDelegateTool::definitions().unwrap();
    assert_eq!(d.len(), 5);
    let start = d.iter().find(|x| x.id().to_string() == START).unwrap();
    assert_eq!(start.approval(), ApprovalPolicy::Ask);
    let status = d.iter().find(|x| x.id().to_string() == STATUS).unwrap();
    assert_eq!(status.approval(), ApprovalPolicy::Auto);
}

#[tokio::test]
async fn an_interrupted_task_cannot_launch_an_agent() {
    let (scratch, db, manager) = rig("write:{cwd}/done.txt,say:finished").await;
    let args = Arguments::parse_with_max(
        &json!({"repository":"repo","task":"add done","minutes":20}),
        60,
    )
    .unwrap();
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
