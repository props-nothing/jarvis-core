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
    assert_eq!(tool.stop(&id).unwrap()["stop_requested"], false);
}

#[tokio::test]
async fn a_real_loopback_preview_is_reachable_but_never_claimed_browser_verified() {
    let scratch = Scratch::new();
    let listener = std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    let tool = scratch.tool();
    let script = format!(
        "require('http').createServer((q,s)=>s.end('preview-ready')).listen({port},'127.0.0.1')"
    );
    let answer = tool
        .start(&json!({"program":"node","arguments":["-e",script],"port":port}))
        .await
        .unwrap();
    let id = answer["process_id"].as_str().unwrap();
    let status = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let status = tool.status(id).await.unwrap();
            if status["port_reachable"] == true {
                break status;
            }
            assert_eq!(status["process"]["status"], "running", "{status}");
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .unwrap();
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
