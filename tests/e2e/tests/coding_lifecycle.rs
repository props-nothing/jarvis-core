//! Real daemon restart and coding-process cleanup, without a model account.
//! The ACP peer is scripted; this proves JARVIS lifecycle behavior, not a provider.
#![allow(clippy::unwrap_used)]

use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use jarvis_storage::{AppPaths, portable_layout};
use serde_json::{Value, json};

const WAIT: Duration = Duration::from_secs(30);
const POLL: Duration = Duration::from_millis(50);

struct Scratch(PathBuf);
impl Drop for Scratch {
    fn drop(&mut self) {
        jarvis_core::remove_scratch_dir(&self.0);
    }
}

fn binary(name: &str) -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let debug = exe.parent()?.parent()?;
    let path = debug.join(format!("{name}{}", std::env::consts::EXE_SUFFIX));
    path.is_file().then_some(path)
}

fn scratch_root() -> Scratch {
    let tag = jarvis_core::scratch_tag();
    let root = std::env::temp_dir().join(format!("jcl-{}", &tag[tag.len() - 12..]));
    std::fs::create_dir_all(root.join("work")).unwrap();
    let root = std::fs::canonicalize(root).unwrap();
    #[cfg(windows)]
    let root = {
        let text = root.to_string_lossy();
        text.strip_prefix(r"\\?\")
            .filter(|rest| !rest.starts_with("UNC\\"))
            .map_or_else(|| root.clone(), PathBuf::from)
    };
    Scratch(root)
}

fn configure(paths: &AppPaths, folder: &Path, agent: &Path, script: &str, port: u16) {
    paths.prepare().unwrap();
    let quoted = |value: &str| serde_json::to_string(value).unwrap();
    let document = format!(
        "schema_version = 1\n[profile]\nname = \"coding-lifecycle\"\n[logging]\nlevel = \"info\"\n[daemon]\n\
         http_enabled = true\nhttp_port = {port}\nshutdown_timeout_seconds = 15\n\
         tool_workspace_roots = [{}]\ncode_agent = [{}, {}]\n",
        quoted(&folder.to_string_lossy()),
        quoted(&agent.to_string_lossy()),
        quoted(script),
    );
    std::fs::write(paths.config().join("config.toml"), document).unwrap();
}

struct Daemon {
    child: Child,
    stderr: PathBuf,
}
impl Daemon {
    async fn start(binary: &Path, root: &Path, paths: &AppPaths) -> Self {
        let tag = jarvis_core::scratch_tag();
        let stderr = root.join(format!("daemon-{tag}.stderr.log"));
        let stdout = root.join(format!("daemon-{tag}.stdout.log"));
        let mut command = Command::new(binary);
        for (name, _) in std::env::vars_os() {
            if name
                .to_string_lossy()
                .to_ascii_uppercase()
                .starts_with("JARVIS_")
            {
                command.env_remove(name);
            }
        }
        let mut daemon = Self {
            child: command
                .args(["--root"])
                .arg(root)
                .env("JARVIS_NO_BUILTIN_ABILITIES", "1")
                .stdout(std::fs::File::create(stdout).unwrap())
                .stderr(std::fs::File::create(&stderr).unwrap())
                .spawn()
                .unwrap(),
            stderr,
        };
        let deadline = Instant::now() + WAIT;
        loop {
            assert!(
                daemon.child.try_wait().unwrap().is_none(),
                "daemon exited before ready: {}",
                daemon.diagnostic()
            );
            if let Ok(state) =
                jarvis_storage::inspect_database(&paths.data().join("jarvis.sqlite3")).await
                && state.daemon_instances.iter().any(|row| {
                    row.process_id == i64::from(daemon.child.id()) && row.state == "ready"
                })
            {
                return daemon;
            }
            assert!(
                Instant::now() < deadline,
                "daemon readiness timed out: {}",
                daemon.diagnostic()
            );
            tokio::time::sleep(POLL).await;
        }
    }

    async fn exited(&mut self) {
        let deadline = Instant::now() + WAIT;
        loop {
            if let Some(status) = self.child.try_wait().unwrap() {
                assert!(
                    status.success(),
                    "forced graceful shutdown failed: {status}: {}",
                    self.diagnostic()
                );
                return;
            }
            assert!(
                Instant::now() < deadline,
                "daemon shutdown timed out: {}",
                self.diagnostic()
            );
            tokio::time::sleep(POLL).await;
        }
    }

    fn diagnostic(&self) -> String {
        use std::io::{Read, Seek, SeekFrom};
        let Ok(mut file) = std::fs::File::open(&self.stderr) else {
            return "stderr unavailable".to_owned();
        };
        let size = file.metadata().map_or(0, |metadata| metadata.len());
        let _ = file.seek(SeekFrom::Start(size.saturating_sub(8_192)));
        let mut tail = Vec::new();
        let _ = file.take(8_192).read_to_end(&mut tail);
        String::from_utf8_lossy(&tail).into_owned()
    }
}
impl Drop for Daemon {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

struct Api {
    http: reqwest::Client,
    base: String,
    credential: String,
}
impl Api {
    fn new(paths: &AppPaths, port: u16) -> Self {
        Self {
            http: reqwest::Client::builder()
                .no_proxy()
                .timeout(WAIT)
                .build()
                .unwrap(),
            base: format!("http://127.0.0.1:{port}/api/v1"),
            credential: std::fs::read_to_string(
                paths.config().join(jarvis_storage::CREDENTIAL_FILE_NAME),
            )
            .unwrap()
            .trim()
            .to_owned(),
        }
    }

    async fn post(&self, path: &str, body: Value, expected: u16) -> Value {
        let response = self
            .http
            .post(format!("{}{path}", self.base))
            .bearer_auth(&self.credential)
            .json(&body)
            .send()
            .await
            .unwrap();
        let status = response.status().as_u16();
        let body: Value = response.json().await.unwrap();
        assert_eq!(status, expected, "POST {path}: {body}");
        body
    }

    async fn tool(&self, tool: &str, args: Value, approval: bool) {
        let run = self
            .post(
                "/runs",
                json!({"objective":"coding lifecycle acceptance"}),
                201,
            )
            .await;
        self.tool_for_run(&run["run_id"], tool, args, approval)
            .await;
    }

    async fn tool_for_run(&self, run: &Value, tool: &str, args: Value, approval: bool) {
        let call = self
            .post(
                &format!("/tools/{tool}/calls"),
                json!({"run_id":run,"arguments":args}),
                if approval { 202 } else { 200 },
            )
            .await;
        if approval {
            assert_eq!(call["state"], "awaiting_approval");
            self.post(
                &format!(
                    "/approvals/{}/decision",
                    call["approval_id"].as_str().unwrap()
                ),
                json!({"decision":"approve"}),
                200,
            )
            .await;
            let result = self
                .post(
                    &format!("/calls/{}/resume", call["call_id"].as_str().unwrap()),
                    json!({"arguments":args}),
                    200,
                )
                .await;
            assert_eq!(result["state"], "confirmed", "{result}");
        }
    }

    async fn tasks(&self) -> Vec<Value> {
        let response = self
            .http
            .get(format!("{}/code-agents/tasks", self.base))
            .bearer_auth(&self.credential)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status().as_u16(), 200);
        response.json::<Value>().await.unwrap()["tasks"]
            .as_array()
            .unwrap()
            .clone()
    }

    async fn task(&self, brief: &str, status: &str) -> Value {
        let deadline = Instant::now() + WAIT;
        loop {
            if let Some(task) = self
                .tasks()
                .await
                .into_iter()
                .find(|row| row["task"] == brief)
                && task["status"] == status
            {
                return task;
            }
            assert!(
                Instant::now() < deadline,
                "task {brief} did not become {status}"
            );
            tokio::time::sleep(POLL).await;
        }
    }
}

#[cfg(windows)]
fn process_running(pid: u32) -> bool {
    let result = Command::new("tasklist")
        .args(["/FI", &format!("PID eq {pid}"), "/FO", "CSV", "/NH"])
        .output()
        .unwrap();
    assert!(result.status.success());
    String::from_utf8_lossy(&result.stdout).contains(&format!("\"{pid}\""))
}
#[cfg(unix)]
fn process_running(pid: u32) -> bool {
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

struct FixtureProcesses(Vec<u32>);
impl FixtureProcesses {
    async fn ready(marker: &Path) -> Self {
        let deadline = Instant::now() + WAIT;
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
                let processes = Self(pids.to_vec());
                assert!(
                    pids.into_iter().all(process_running),
                    "fixture tree was not alive"
                );
                return processes;
            }
            assert!(
                Instant::now() < deadline,
                "agent did not record its live process tree"
            );
            tokio::time::sleep(POLL).await;
        }
    }

    async fn assert_stopped(&mut self) {
        let deadline = Instant::now() + WAIT;
        while self.0.iter().copied().any(process_running) {
            assert!(
                Instant::now() < deadline,
                "coding process tree survived Stop: {:?}",
                self.0
            );
            tokio::time::sleep(POLL).await;
        }
        self.0.clear();
    }
}
impl Drop for FixtureProcesses {
    fn drop(&mut self) {
        // Failure cleanup targets only PIDs recorded by this disposable fixture.
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

async fn verify_owner_stop(api: &Api, marker: &Path) {
    api.tool(
        "jarvis.code.delegate_start",
        json!({"task":"cancel me","allow_commands":false,"allow_network":false}),
        true,
    )
    .await;
    let first = api.task("cancel me", "running").await;
    let mut processes = FixtureProcesses::ready(marker).await;
    api.tool(
        "jarvis.code.delegate_stop",
        json!({"task_id":first["task_id"]}),
        false,
    )
    .await;
    api.task("cancel me", "cancelled").await;
    processes.assert_stopped().await;
    assert!(marker.is_file(), "cancellation must retain work evidence");
}

async fn verify_parent_stop(api: &Api, marker: &Path) {
    let parent = api
        .post(
            "/runs",
            json!({"objective":"family cancellation acceptance"}),
            201,
        )
        .await;
    for task in ["family running", "family queued"] {
        api.tool_for_run(
            &parent["run_id"],
            "jarvis.code.delegate_start",
            json!({"task":task,"allow_commands":false,"allow_network":false}),
            true,
        )
        .await;
    }
    let active = api.task("family running", "running").await;
    let queued = api.task("family queued", "queued").await;
    assert_eq!(active["parent_run_id"], parent["run_id"]);
    assert_eq!(queued["parent_run_id"], parent["run_id"]);
    assert!(active["origin_call_id"].is_string());
    assert_ne!(active["origin_call_id"], queued["origin_call_id"]);
    let mut processes = FixtureProcesses::ready(marker).await;
    let before = std::fs::read(marker).unwrap();
    api.post(
        &format!("/runs/{}/cancel", parent["run_id"].as_str().unwrap()),
        json!({}),
        200,
    )
    .await;
    for task in ["family running", "family queued"] {
        let stopped = api.task(task, "cancelled").await;
        assert!(stopped["cancellation_requested_at"].is_string());
    }
    processes.assert_stopped().await;
    assert_eq!(
        std::fs::read(marker).unwrap(),
        before,
        "queued cancellation must not start another process"
    );
}

async fn verify_approved_continuation(
    api: &Api,
    paths: &AppPaths,
    folder: &Path,
    agent: &Path,
    port: u16,
    active: &Value,
) {
    let completed = folder.join("continued.txt");
    configure(
        paths,
        folder,
        agent,
        &format!("write:{}", completed.display()),
        port,
    );
    api.tool(
        "jarvis.code.delegate_resume",
        json!({"task_id":active["task_id"]}),
        true,
    )
    .await;
    let deadline = Instant::now() + WAIT;
    loop {
        if let Some(task) = api
            .tasks()
            .await
            .into_iter()
            .find(|row| row["status"] == "completed")
        {
            assert_ne!(task["task_id"], active["task_id"]);
            assert_eq!(task["allow_commands"], false);
            assert_eq!(task["repository"], active["repository"]);
            assert!(completed.is_file());
            break;
        }
        assert!(
            Instant::now() < deadline,
            "approved continuation did not complete"
        );
        tokio::time::sleep(POLL).await;
    }
    api.task("interrupt me", "cancelled").await;
    assert_eq!(
        std::fs::read_to_string(folder.join("owner.txt")).unwrap(),
        "preserve owner changes"
    );
}

#[tokio::test]
async fn coding_stop_and_daemon_restart_preserve_files_without_replaying_jobs() {
    let Some((daemon, agent)) = acceptance_binaries() else {
        return;
    };
    let root = scratch_root();
    let folder = root.0.join("work");
    std::fs::write(folder.join("owner.txt"), "preserve owner changes").unwrap();
    let paths = portable_layout(&root.0).unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    let first_marker = folder.join("first.json");
    configure_wait(&paths, &folder, &agent, &first_marker, port);
    let mut running = Daemon::start(&daemon, &root.0, &paths).await;
    let api = Api::new(&paths, port);
    verify_owner_stop(&api, &first_marker).await;

    let family_marker = folder.join("family.json");
    configure_wait(&paths, &folder, &agent, &family_marker, port);
    verify_parent_stop(&api, &family_marker).await;

    let restart_marker = folder.join("restart.json");
    configure_wait(&paths, &folder, &agent, &restart_marker, port);
    api.tool(
        "jarvis.code.delegate_start",
        json!({"task":"interrupt me","allow_commands":false,"allow_network":false}),
        true,
    )
    .await;
    let active = api.task("interrupt me", "running").await;
    let mut active_processes = FixtureProcesses::ready(&restart_marker).await;
    api.tool(
        "jarvis.code.delegate_start",
        json!({"task":"queued behind me","allow_commands":false,"allow_network":false}),
        true,
    )
    .await;
    api.task("queued behind me", "queued").await;
    let marker_before = std::fs::read(&restart_marker).unwrap();
    let refusal = api.post("/shutdown", json!({}), 409).await;
    assert_eq!(
        refusal["coding_jobs"], 2,
        "both running and queued jobs must protect shutdown"
    );
    api.post("/shutdown?force=true", json!({}), 202).await;
    running.exited().await;
    active_processes.assert_stopped().await;
    assert_graceful_settlement(&paths).await;

    let mut restarted = Daemon::start(&daemon, &root.0, &paths).await;
    api.task("interrupt me", "cancelled").await;
    api.task("queued behind me", "cancelled").await;
    api.task("cancel me", "cancelled").await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(
        std::fs::read(&restart_marker).unwrap(),
        marker_before,
        "restart must not launch a new agent"
    );
    assert_eq!(
        std::fs::read_to_string(folder.join("owner.txt")).unwrap(),
        "preserve owner changes"
    );

    verify_crash_isolation(&api, &paths, &folder, &agent, port).await;
    verify_approved_continuation(&api, &paths, &folder, &agent, port, &active).await;
    verify_abrupt_restart(
        &api,
        &paths,
        &folder,
        &agent,
        port,
        &daemon,
        &root.0,
        &mut restarted,
    )
    .await;
    api.post("/shutdown?force=true", json!({}), 202).await;
    restarted.exited().await;
}

async fn verify_crash_isolation(
    api: &Api,
    paths: &AppPaths,
    folder: &Path,
    agent: &Path,
    port: u16,
) {
    configure(paths, folder, agent, "crash", port);
    let parent = api
        .post("/runs", json!({"objective":"survive child crash"}), 201)
        .await;
    api.tool_for_run(
        &parent["run_id"],
        "jarvis.code.delegate_start",
        json!({"task":"crash child","allow_commands":false,"allow_network":false}),
        true,
    )
    .await;
    let failed = api.task("crash child", "failed").await;
    assert_eq!(failed["parent_run_id"], parent["run_id"]);
    let db = jarvis_storage::SqliteDatabase::open(&paths.data().join("jarvis.sqlite3"))
        .await
        .unwrap();
    let call = jarvis_storage::find_tool_call(&db, failed["origin_call_id"].as_str().unwrap())
        .await
        .unwrap();
    assert_eq!(call.run_id(), parent["run_id"].as_str().unwrap());
    assert_eq!(call.outcome(), jarvis_core::ToolOutcome::Confirmed);
    assert!(call.approval_id().is_some());
    assert_eq!(call.receipt(), call.id());
    let approval = jarvis_storage::find_approval(&db, call.approval_id().unwrap())
        .await
        .unwrap();
    assert_eq!(approval.intent().to_hex(), call.intent_hash());
    db.close().await;
    // Same parent can still invoke ordinary tools after the external process dies.
    api.tool_for_run(
        &parent["run_id"],
        "jarvis.code.delegate_list",
        json!({}),
        false,
    )
    .await;
}

#[allow(clippy::too_many_arguments)]
async fn verify_abrupt_restart(
    api: &Api,
    paths: &AppPaths,
    folder: &Path,
    agent: &Path,
    port: u16,
    daemon: &Path,
    root: &Path,
    running: &mut Daemon,
) {
    let marker = folder.join("abrupt.json");
    configure(
        paths,
        folder,
        agent,
        &format!("initialize_wait:{},initialize_child", marker.display()),
        port,
    );
    for task in ["abrupt running", "abrupt queued"] {
        api.tool(
            "jarvis.code.delegate_start",
            json!({"task":task,"allow_commands":false,"allow_network":false}),
            true,
        )
        .await;
    }
    api.task("abrupt running", "running").await;
    api.task("abrupt queued", "queued").await;
    let mut processes = FixtureProcesses::ready(&marker).await;
    let before = std::fs::read(&marker).unwrap();
    running.child.kill().unwrap();
    running.child.wait().unwrap();
    // Abrupt Unix parent death does not run the process-group owner's Drop.
    // Clean up only the fixture's observed PIDs; recovery must still never replay.
    #[cfg(unix)]
    for pid in &processes.0 {
        let _ = Command::new("kill")
            .args(["-KILL", &pid.to_string()])
            .status();
    }
    processes.assert_stopped().await;
    *running = Daemon::start(daemon, root, paths).await;
    for task in ["abrupt running", "abrupt queued"] {
        api.task(task, "interrupted").await;
    }
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(
        std::fs::read(&marker).unwrap(),
        before,
        "abrupt restart must not replay workers"
    );
}

async fn assert_graceful_settlement(paths: &AppPaths) {
    // Inspect BEFORE restart recovery: graceful drain itself persisted cancellation.
    let db = jarvis_storage::SqliteDatabase::open(&paths.data().join("jarvis.sqlite3"))
        .await
        .unwrap();
    for brief in ["interrupt me", "queued behind me"] {
        let rows =
            jarvis_storage::list_delegated_tasks(&db, jarvis_storage::LOCAL_WORKSPACE_ID, 50)
                .await
                .unwrap();
        let row = rows.iter().find(|row| row.task == brief).unwrap();
        assert_eq!(row.status, "cancelled");
        assert!(row.cancellation_requested_at.is_some());
    }
    db.close().await;
}

#[tokio::test]
async fn unfinished_work_deadline_records_failed_shutdown_without_claiming_settlement() {
    let Some((daemon, agent)) = acceptance_binaries() else {
        return;
    };
    let root = scratch_root();
    let paths = portable_layout(&root.0).unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    configure(&paths, &root.0.join("work"), &agent, "crash", port);
    let config = paths.config().join("config.toml");
    std::fs::write(
        &config,
        std::fs::read_to_string(&config).unwrap().replace(
            "shutdown_timeout_seconds = 15",
            "shutdown_timeout_seconds = 1",
        ),
    )
    .unwrap();
    let mut running = Daemon::start(&daemon, &root.0, &paths).await;
    let api = Api::new(&paths, port);
    // A durable task whose owning worker was lost cannot certify cleanup.
    let db = jarvis_storage::SqliteDatabase::open(&paths.data().join("jarvis.sqlite3"))
        .await
        .unwrap();
    let task = jarvis_storage::create_delegated_task(
        &db,
        &jarvis_storage::NewDelegatedTask {
            workspace_id: jarvis_storage::LOCAL_WORKSPACE_ID,
            repository: "repo",
            task: "lost worker",
            allow_commands: false,
            minutes: 1,
            now: jarvis_core::UtcTimestamp::now(&jarvis_core::SystemClock),
        },
    )
    .await
    .unwrap();
    api.post("/shutdown?force=true", json!({}), 202).await;
    let status = tokio::time::timeout(WAIT, async {
        loop {
            if let Some(status) = running.child.try_wait().unwrap() {
                break status;
            }
            tokio::time::sleep(POLL).await;
        }
    })
    .await
    .unwrap();
    assert!(
        !status.success(),
        "unfinished work must not report clean shutdown"
    );
    let stored =
        jarvis_storage::find_delegated_task(&db, jarvis_storage::LOCAL_WORKSPACE_ID, &task.id)
            .await
            .unwrap();
    assert_eq!(stored.status, "queued");
    assert!(stored.cancellation_requested_at.is_some());
    let inspection = jarvis_storage::inspect_database(&paths.data().join("jarvis.sqlite3"))
        .await
        .unwrap();
    let stopped = inspection
        .daemon_instances
        .iter()
        .find(|row| row.process_id == i64::from(running.child.id()))
        .unwrap();
    assert_eq!(stopped.stop_reason.as_deref(), Some("shutdown-failed"));
    db.close().await;
}

fn acceptance_binaries() -> Option<(PathBuf, PathBuf)> {
    let binaries = binary("jarvisd").zip(binary("examples/fake_acp_agent"));
    assert!(
        binaries.is_some() || std::env::var_os("ACCEPTANCE_REQUIRE_BINARIES").is_none(),
        "build jarvisd and fake_acp_agent first"
    );
    if binaries.is_none() {
        eprintln!("SKIP: coding lifecycle requires daemon and agent binaries");
    }
    binaries
}

fn configure_wait(paths: &AppPaths, folder: &Path, agent: &Path, marker: &Path, port: u16) {
    configure(
        paths,
        folder,
        agent,
        &format!("initialize_wait:{},initialize_child", marker.display()),
        port,
    );
}
