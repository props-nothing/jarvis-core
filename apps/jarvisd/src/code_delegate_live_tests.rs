//! Opt-in account-backed cancellation acceptance. All work is disposable and local.
#![allow(clippy::unwrap_used)]

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use super::*;
use crate::code_agents::AgentSettings;
use crate::code_delegate::AgentSource;

const SCRIPT: &str = r"const fs = require('node:fs');
const { spawn } = require('node:child_process');
const child = spawn(process.execPath, ['-e', 'setInterval(() => {}, 1000)'], {stdio:'ignore'});
fs.writeFileSync('work-started.json', JSON.stringify({pid:process.pid, child_pid:child.pid}));
setInterval(() => {}, 1000);
";

struct Scratch(PathBuf);
impl Drop for Scratch {
    fn drop(&mut self) {
        jarvis_core::remove_scratch_dir(&self.0);
    }
}

#[cfg(windows)]
fn running(pid: u32) -> bool {
    let output = Command::new("tasklist")
        .args(["/FI", &format!("PID eq {pid}"), "/FO", "CSV", "/NH"])
        .output()
        .unwrap();
    assert!(output.status.success());
    String::from_utf8_lossy(&output.stdout).contains(&format!("\"{pid}\""))
}

#[cfg(unix)]
fn running(pid: u32) -> bool {
    // Zombies have stopped executing but may await their new parent's reap.
    let output = Command::new("ps")
        .args(["-o", "stat=", "-p", &pid.to_string()])
        .output()
        .unwrap();
    assert!(
        output.status.success() || (output.status.code() == Some(1) && output.stderr.is_empty()),
        "process-state probe failed"
    );
    let state = String::from_utf8_lossy(&output.stdout);
    !state.trim().is_empty() && !state.trim().starts_with('Z')
}

struct WorkProcesses(Vec<u32>);
impl Drop for WorkProcesses {
    fn drop(&mut self) {
        // Failure cleanup only: successful acceptance observes termination before disarming this guard.
        for pid in &self.0 {
            #[cfg(windows)]
            let _ = Command::new("taskkill")
                .args(["/PID", &pid.to_string(), "/T", "/F"])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
            #[cfg(unix)]
            let _ = Command::new("kill")
                .args(["-KILL", &pid.to_string()])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
        }
    }
}

struct CancelOnDrop<'a> {
    manager: &'a AsyncCodeDelegateTool,
    task_id: &'a str,
}
impl Drop for CancelOnDrop<'_> {
    fn drop(&mut self) {
        let _ = self.manager.stop(&json!({"task_id":self.task_id}));
    }
}

async fn wait_for_work(marker: &Path, manager: &AsyncCodeDelegateTool, id: &str) -> WorkProcesses {
    let deadline = Instant::now() + Duration::from_secs(140);
    loop {
        if let Ok(text) = std::fs::read_to_string(marker)
            && let Ok(value) = serde_json::from_str::<Value>(&text)
        {
            let pids = ["pid", "child_pid"]
                .map(|key| u32::try_from(value[key].as_u64().unwrap()).unwrap());
            assert!(
                pids.iter()
                    .all(|pid| *pid > 1 && *pid != std::process::id())
            );
            assert_ne!(pids[0], pids[1]);
            let processes = WorkProcesses(pids.to_vec());
            assert!(
                processes.0.iter().copied().all(running),
                "the real agent's work tree must be alive before Stop"
            );
            return processes;
        }
        let status = manager.status(&json!({"task_id":id})).await.unwrap();
        assert!(
            matches!(status["status"].as_str(), Some("queued" | "running")),
            "real agent ended before the requested work started; status={}",
            status["status"]
        );
        assert!(
            Instant::now() < deadline,
            "real agent did not start the disposable process within 140 seconds"
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

async fn verify_live_stop(manager: &AsyncCodeDelegateTool, id: &str, root: &Path) {
    let folder = root.join("work");
    let marker = folder.join("work-started.json");
    let mut processes = wait_for_work(&marker, manager, id).await;
    manager.stop(&json!({"task_id":id})).unwrap();
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let task = manager.status(&json!({"task_id":id})).await.unwrap();
        if !matches!(task["status"].as_str(), Some("queued" | "running")) {
            assert_eq!(
                task["status"], "cancelled",
                "owner Stop must persist as cancelled"
            );
            break;
        }
        assert!(
            Instant::now() < deadline,
            "real cancellation did not settle within 30 seconds"
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    while processes.0.iter().copied().any(running) {
        assert!(
            Instant::now() < deadline,
            "real agent work process survived cancellation"
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    processes.0.clear();
    assert!(marker.is_file());
    assert_eq!(
        std::fs::read_to_string(folder.join("lifecycle-wait.cjs")).unwrap(),
        SCRIPT
    );
    assert_eq!(
        std::fs::read_to_string(folder.join("owner.txt")).unwrap(),
        "preserve these unrelated changes"
    );
    let saved = jarvis_storage::find_delegated_task(&manager.db, &manager.workspace, id)
        .await
        .unwrap();
    assert_eq!(saved.status, "cancelled");
    assert!(saved.finished_at.is_some());
    let reopened = SqliteDatabase::open(&root.join("jarvis.sqlite3"))
        .await
        .unwrap();
    let retained = jarvis_storage::find_delegated_task(&reopened, &manager.workspace, id)
        .await
        .unwrap();
    assert_eq!(retained.status, "cancelled");
    reopened.close().await;
}

/// Uses the pinned adapter's existing sign-in. Never reads/copies credentials,
/// edits an owner project, installs globally, or changes agent configuration.
/// May consume model usage; ordinary CI never runs this opt-in test.
#[tokio::test]
#[ignore = "requires the owner's signed-in pinned Codex ACP adapter; may consume model usage"]
async fn live_codex_stop_terminates_started_work_and_retains_durable_cancellation() {
    let elapsed = Instant::now();
    let tag = jarvis_core::scratch_tag();
    let root = std::env::temp_dir().join(format!("jclive-{}", &tag[tag.len() - 12..]));
    std::fs::create_dir_all(root.join("work")).unwrap();
    let root = Scratch(crate::command_run::plain(
        &std::fs::canonicalize(root).unwrap(),
    ));
    let folder = root.0.join("work");
    let script_path = folder.join("lifecycle-wait.cjs");
    std::fs::write(&script_path, SCRIPT).unwrap();
    std::fs::write(folder.join("owner.txt"), "preserve these unrelated changes").unwrap();
    let db = Arc::new(
        SqliteDatabase::open(&root.0.join("jarvis.sqlite3"))
            .await
            .unwrap(),
    );
    let identity = jarvis_storage::load_local_identity(&db).await.unwrap();
    let inner = Arc::new(
        CodeDelegateTool::new(
            std::slice::from_ref(&folder),
            root.0.join("trees"),
            AgentSource::Fixed(AgentSettings {
                command: vec![
                    "npx".to_owned(),
                    "-y".to_owned(),
                    "@agentclientprotocol/codex-acp@2.2.2".to_owned(),
                ],
                ..AgentSettings::default()
            }),
        )
        .unwrap(),
    );
    let manager =
        AsyncCodeDelegateTool::new(inner, Arc::clone(&db), identity.workspace_id().into())
            .without_notifications();
    let started = manager.start(&json!({
        "task":"Cancellation acceptance only. Run exactly node lifecycle-wait.cjs in the foreground using your command tool. The supplied script intentionally waits forever and creates its own readiness marker. Leave it running until the owner cancels. Do not edit any file, start another command, install dependencies, initialize Git or contact any service. Do not finish the turn while that command is running.",
        "allow_commands":true,"allow_network":false,"minutes":3
    })).await.unwrap();
    let id = started["task_id"].as_str().unwrap();
    let _cancel_on_drop = CancelOnDrop {
        manager: &manager,
        task_id: id,
    };
    let acceptance = tokio::time::timeout(
        Duration::from_secs(175),
        verify_live_stop(&manager, id, &root.0),
    )
    .await;
    if acceptance.is_err() {
        let _ = manager.stop(&json!({"task_id":id}));
    }
    assert!(
        acceptance.is_ok(),
        "bounded live cancellation acceptance exceeded 175 seconds"
    );
    eprintln!(
        "LIVE CANCELLATION: codex-acp 2.2.2; elapsed_seconds={}; two work processes independently observed alive then gone; supplied files preserved; durable cancelled state verified after database reopening",
        elapsed.elapsed().as_secs()
    );
    db.close().await;
}
