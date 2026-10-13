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
    let root = std::env::temp_dir().join(format!("jasync-{}", jarvis_core::scratch_tag()));
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
                command: vec![fake().to_string_lossy().into_owned(), script.into()],
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
    for _ in 0..100 {
        let v = manager.status(&json!({"task_id":id})).await.unwrap();
        if !matches!(v["status"].as_str(), Some("queued" | "running")) {
            return v;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    panic!("task did not end")
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
