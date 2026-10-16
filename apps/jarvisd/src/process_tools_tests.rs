#![allow(clippy::unwrap_used)]
use super::*;

struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("jpr-{}", jarvis_core::scratch_tag()));
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }
    fn tool(&self) -> ProcessTools {
        ProcessTools::new(std::slice::from_ref(&self.0), self.0.join("processes")).unwrap()
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        jarvis_core::remove_scratch_dir(&self.0);
    }
}

#[test]
fn contracts_validate_and_starts_require_owner_approval() {
    let definitions = ProcessTools::definitions().unwrap();
    assert_eq!(definitions.len(), 4);
    assert_eq!(definitions[0].approval(), jarvis_tools::ApprovalPolicy::Ask);
}

async fn terminal(tool: &ProcessTools, id: &str) -> Record {
    for _ in 0..200 {
        let record = tool.load(id).unwrap();
        if record.status != "running" {
            return record;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("managed process did not settle");
}

#[tokio::test]
async fn commands_run_in_settings_folder_and_completed_logs_survive_recreation() {
    let scratch = Scratch::new();
    let tool = scratch.tool();
    let command = if cfg!(windows) {
        json!({"program":"cmd","arguments":["/C","echo managed-output & cd"]})
    } else {
        json!({"program":"sh","arguments":["-c","echo managed-output; pwd"]})
    };
    let answer = tool.start(&command).await.unwrap();
    let id = answer["process_id"].as_str().unwrap();
    assert_eq!(terminal(&tool, id).await.exit_code, Some(0));
    let reloaded = scratch.tool().status(id).await.unwrap();
    assert_eq!(reloaded["process"]["status"], "exited");
    assert!(
        reloaded["stdout_tail"]
            .as_str()
            .unwrap()
            .contains("managed-output")
    );
    assert_eq!(reloaded["browser_verified"], false);
}

#[tokio::test]
async fn long_lived_process_has_no_task_deadline_and_stop_settles() {
    let scratch = Scratch::new();
    let tool = scratch.tool();
    let command = if cfg!(windows) {
        json!({"program":"ping","arguments":["-t","127.0.0.1"]})
    } else {
        json!({"program":"sleep","arguments":["60"]})
    };
    let answer = tool.start(&command).await.unwrap();
    let id = answer["process_id"].as_str().unwrap();
    assert_eq!(tool.load(id).unwrap().status, "running");
    assert_eq!(tool.stop(id).unwrap()["stop_requested"], true);
    assert_eq!(terminal(&tool, id).await.status, "stopped");
    assert_eq!(scratch.tool().stop(id).unwrap()["stop_requested"], false);
}

#[tokio::test]
async fn managed_root_exit_terminates_a_descendant_before_losing_the_live_record() {
    use crate::command_run::process_fixture;

    let scratch = Scratch::new();
    let tool = scratch.tool();
    let command = process_fixture::command(&scratch.0);
    let answer = tool.start(&command).await.unwrap();
    let id = answer["process_id"].as_str().unwrap();
    let tree = process_fixture::ready(&scratch.0).await;
    std::fs::write(scratch.0.join("release"), "exit root only").unwrap();
    let completed = terminal(&tool, id).await;
    assert_eq!(completed.status, "exited");
    assert_eq!(completed.exit_code, Some(0));
    assert!(completed.capture_complete);
    tree.assert_gone().await;
    assert_eq!(scratch.tool().stop(id).unwrap()["stop_requested"], false);
}

#[tokio::test]
async fn managed_stop_terminates_both_observed_members_of_the_process_tree() {
    use crate::command_run::process_fixture;

    let scratch = Scratch::new();
    let tool = scratch.tool();
    let command = process_fixture::command(&scratch.0);
    let answer = tool.start(&command).await.unwrap();
    let id = answer["process_id"].as_str().unwrap();
    let tree = process_fixture::ready(&scratch.0).await;
    assert_eq!(tool.stop(id).unwrap()["stop_requested"], true);
    assert_eq!(terminal(&tool, id).await.status, "stopped");
    tree.assert_gone().await;
}

#[tokio::test]
async fn outside_workspaces_and_stale_ids_never_start_or_signal_processes() {
    let scratch = Scratch::new();
    let tool = scratch.tool();
    assert!(
        tool.start(&json!({"program":"node","directory":"../outside"}))
            .await
            .is_err()
    );
    assert!(tool.stop("../../some-file").is_err());
    let id = DelegatedTaskId::new().to_string();
    let folder = tool.storage.join(&id);
    std::fs::create_dir_all(&folder).unwrap();
    let record = Record {
        process_id: id.clone(),
        directory: scratch.0.to_string_lossy().into_owned(),
        program: "node".to_owned(),
        arguments: vec![],
        port: None,
        status: "running".to_owned(),
        exit_code: None,
        error: None,
        created_at: "now".to_owned(),
        finished_at: None,
        capture_complete: true,
    };
    std::fs::write(
        folder.join("request.json"),
        serde_json::to_vec(&record).unwrap(),
    )
    .unwrap();
    assert_eq!(tool.load(&id).unwrap().status, "interrupted");
    assert_eq!(
        tool.active_count().unwrap(),
        0,
        "old request files are not live processes"
    );
    assert_eq!(tool.stop(&id).unwrap()["stop_requested"], false);
}

async fn shutdown_state(
    scratch: &Scratch,
    tool: Arc<ProcessTools>,
) -> crate::gateway::GatewayState {
    let database = Arc::new(
        jarvis_storage::SqliteDatabase::test_fixture(&scratch.0.join("shutdown.sqlite3"))
            .await
            .unwrap(),
    );
    crate::gateway::GatewayState::new(database, jarvis_core::ClientCredential::generate().unwrap())
        .with_processes(tool)
}

#[tokio::test]
async fn a_live_managed_process_blocks_shutdown_until_its_supervisor_finishes() {
    let scratch = Scratch::new();
    let tool = Arc::new(scratch.tool());
    let state = shutdown_state(&scratch, Arc::clone(&tool)).await;
    let command = if cfg!(windows) {
        json!({"program":"ping","arguments":["-t","127.0.0.1"]})
    } else {
        json!({"program":"sleep","arguments":["60"]})
    };
    let answer = tool.start(&command).await.unwrap();
    let id = answer["process_id"].as_str().unwrap();
    let refusal = crate::stop::refusal_if_working(&state, false)
        .await
        .unwrap();
    assert_eq!(refusal.status(), axum::http::StatusCode::CONFLICT);
    let body: Value = serde_json::from_slice(
        &axum::body::to_bytes(refusal.into_body(), 16_384)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(body["managed_processes"], 1);
    assert_eq!(body["agent_runs"], 0);
    assert!(
        crate::stop::refusal_if_working(&state, true)
            .await
            .is_none()
    );
    tool.stop(id).unwrap();
    assert_eq!(terminal(&tool, id).await.status, "stopped");
    tokio::time::timeout(Duration::from_secs(5), async {
        while tool.active_count().unwrap() != 0 {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    assert!(
        crate::stop::refusal_if_working(&state, false)
            .await
            .is_none()
    );
    state.database().close().await;
}

#[tokio::test]
async fn an_unreadable_live_process_registry_blocks_shutdown_instead_of_claiming_idle() {
    let scratch = Scratch::new();
    let tool = Arc::new(scratch.tool());
    let state = shutdown_state(&scratch, Arc::clone(&tool)).await;
    let registry = Arc::clone(&tool.active);
    let poisoned = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _guard = registry.lock().unwrap();
        panic!("fixture: registry owner failed");
    }));
    assert!(poisoned.is_err());
    assert!(tool.active_count().is_err());
    assert_eq!(
        crate::stop::refusal_if_working(&state, false)
            .await
            .unwrap()
            .status(),
        axum::http::StatusCode::SERVICE_UNAVAILABLE
    );
    assert!(
        crate::stop::refusal_if_working(&state, true)
            .await
            .is_none()
    );
    state.database().close().await;
}

#[tokio::test]
async fn a_real_loopback_preview_is_reachable_but_never_claimed_browser_verified() {
    let scratch = Scratch::new();
    let listener = std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    let tool = scratch.tool();
    // Separate child startup/bind evidence from the production TCP readiness probe.
    std::fs::write(
        scratch.0.join("preview.cjs"),
        r"const fs = require('node:fs');
const state = (phase, extra = {}) => fs.writeFileSync('preview-state.json', JSON.stringify({
  phase, pid: process.pid, version: process.version, executable: process.execPath, ...extra
}));
state('starting');
const server = require('node:http').createServer((q, s) => s.end('preview-ready'));
server.on('error', error => {
  state('error', {code: error.code, message: error.message});
  process.exit(1);
});
server.listen(Number(process.argv[2]), '127.0.0.1', () => state('listening'));
",
    )
    .unwrap();
    let answer = tool
        .start(&json!({"program":"node","arguments":["preview.cjs",port.to_string()],"port":port}))
        .await
        .unwrap();
    let id = answer["process_id"].as_str().unwrap();
    // This is a real child startup check, not a ten-second latency contract.
    // Loaded Windows runners need more headroom; successful starts do not wait.
    let startup_limit = Duration::from_secs(if cfg!(windows) { 30 } else { 10 });
    let mut status = Value::Null;
    let mut child_state = Value::Null;
    let readiness = tokio::time::timeout(startup_limit, async {
        loop {
            if let Ok(bytes) = std::fs::read(scratch.0.join("preview-state.json"))
                && let Ok(state) = serde_json::from_slice::<Value>(&bytes)
            {
                child_state = state;
            }
            assert_eq!(tool.load(id).unwrap().status, "running", "{child_state}");
            assert_ne!(child_state["phase"], "error", "{child_state}");
            if child_state["phase"] == "listening" {
                status = tool.status(id).await.unwrap();
                if status["port_reachable"] == true {
                    break;
                }
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await;
    if let Err(error) = readiness {
        // Diagnostic only: a slower independent connect cannot turn failure into success.
        let direct_probe = std::net::TcpStream::connect_timeout(
            &std::net::SocketAddr::from((std::net::Ipv4Addr::LOCALHOST, port)),
            Duration::from_secs(2),
        )
        .map(|_| ());
        let stop = tool.stop(id);
        let settled = tokio::time::timeout(Duration::from_secs(10), async {
            while tool.load(id).unwrap().status == "running" {
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        })
        .await;
        panic!(
            "preview startup exceeded {startup_limit:?}: {error}; child: {child_state}; last status: {status}; independent TCP probe: {direct_probe:?}; stop: {stop:?}; settled: {settled:?}"
        );
    }
    assert_eq!(status["browser_verified"], false);
    let body = reqwest::get(status["preview_url"].as_str().unwrap())
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert_eq!(body, "preview-ready");
    assert!(
        tool.start(&json!({"program":"node","port":port}))
            .await
            .is_err()
    );
    tool.stop(id).unwrap();
    assert_eq!(terminal(&tool, id).await.status, "stopped");
    assert!(!port_reachable(port).await);
}
