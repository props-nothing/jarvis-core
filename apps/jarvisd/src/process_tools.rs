use std::collections::BTreeMap;
use std::ffi::OsString;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use jarvis_core::{DelegatedTaskId, IsolatedText, SystemClock, UtcTimestamp};
use jarvis_runtimes::owned_process::{AbortOnDrop, OwnedProcess};
use jarvis_tools::{
    AdapterError, ToolCallResult, ToolDefinition, ToolEffect, ToolExecutionRequest, ToolExecutor,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};
use tokio::sync::Notify;

use crate::command_run::{
    CommandArgs, CommandRunTool, CommandRunToolError, child_environment, resolve_program,
};
use crate::skill_bundles::{definition, refused, result};

const LOG_LIMIT: usize = 16 * 1024 * 1024;

#[derive(Clone, Serialize, Deserialize)]
struct Record {
    process_id: String,
    directory: String,
    program: String,
    arguments: Vec<String>,
    port: Option<u16>,
    status: String,
    exit_code: Option<i32>,
    error: Option<String>,
    created_at: String,
    finished_at: Option<String>,
    capture_complete: bool,
}

struct Active {
    record: Mutex<Record>,
    cancel: Notify,
}

pub(crate) struct ProcessTools {
    folders: CommandRunTool,
    storage: PathBuf,
    active: Arc<Mutex<BTreeMap<String, Arc<Active>>>>,
}

impl ProcessTools {
    /// Only this daemon's live registry counts; old request files are interrupted history.
    pub(crate) fn active_count(&self) -> Result<usize, &'static str> {
        self.active
            .lock()
            .map(|processes| processes.len())
            .map_err(|_| "managed process registry unavailable")
    }

    pub(crate) fn new(roots: &[PathBuf], storage: PathBuf) -> Result<Self, CommandRunToolError> {
        Ok(Self {
            folders: CommandRunTool::new(roots)?,
            storage,
            active: Arc::new(Mutex::new(BTreeMap::new())),
        })
    }

    pub(crate) fn definitions() -> Result<Vec<ToolDefinition>, CommandRunToolError> {
        let start = r#"{"type":"object","additionalProperties":false,"required":["program"],"properties":{"program":{"type":"string"},"arguments":{"type":"array","items":{"type":"string"},"maxItems":24},"directory":{"type":"string"},"port":{"type":"integer","minimum":1,"maximum":65535,"description":"Optional loopback preview port. Configure the server itself to bind 127.0.0.1 and this port."}}}"#;
        let id = r#"{"type":"object","additionalProperties":false,"required":["process_id"],"properties":{"process_id":{"type":"string","minLength":36,"maxLength":36}}}"#;
        Ok(vec![
            definition(
                "jarvis.process.start",
                "Start a managed project process",
                "Starts an authorized host command in a Settings-permitted folder and returns a process id. For dev servers use this instead of hidden detached children or the bounded command runner. No total lifetime limit; stop explicitly. Captures bounded logs; no automatic restart or credential forwarding. A reachable preview is not browser verification. Configure a loopback bind in the actual command.",
                start,
                ToolEffect::CodeExecution,
                3,
            )?,
            definition(
                "jarvis.process.status",
                "Inspect a managed process",
                "Reads process state, recent output, capture limits and loopback port reachability. Interrupted records after restart are not live processes and are never killed by a stale PID. Use the real browser to verify a website.",
                id,
                ToolEffect::ReadOnly,
                0,
            )?,
            definition(
                "jarvis.process.stop",
                "Stop a managed process",
                "Requests stop of a live managed process and its child tree. Files are retained. Check status for completion; never replays an old process after restart.",
                id,
                ToolEffect::Write,
                1,
            )?,
            definition(
                "jarvis.process.list",
                "List managed project processes",
                "Lists up to 20 process records, newest first. Logs are available with status. No process is automatically restarted.",
                r#"{"type":"object","additionalProperties":false,"properties":{}}"#,
                ToolEffect::ReadOnly,
                0,
            )?,
        ])
    }

    async fn start(&self, value: &Value) -> Result<Value, AdapterError> {
        let parsed = CommandArgs::parse(value).map_err(refused)?;
        let directory = self
            .folders
            .resolve_directory(&parsed.directory)
            .map_err(refused)?;
        let environment = child_environment(std::env::vars_os());
        let search = environment
            .get("PATH")
            .map(OsString::from)
            .unwrap_or_default();
        let executable = resolve_program(
            &parsed.program,
            &search,
            environment.get("PATHEXT").cloned().as_deref(),
        )
        .ok_or_else(|| refused(format!("{} is not installed on PATH", parsed.program)))?;
        let port = match value.get("port") {
            None => None,
            Some(port) => Some(
                port.as_u64()
                    .and_then(|port| u16::try_from(port).ok())
                    .filter(|port| *port > 0)
                    .ok_or_else(|| refused("port must be 1 to 65535"))?,
            ),
        };
        if let Some(port) = port
            && port_reachable(port).await
        {
            return Err(refused(
                "preview port is already in use; select another port so an unrelated service is not presented as this process",
            ));
        }
        let id = DelegatedTaskId::new().to_string();
        let record = Record {
            process_id: id.clone(),
            directory: directory.to_string_lossy().into_owned(),
            program: parsed.program.clone(),
            arguments: parsed.arguments.clone(),
            port,
            status: "running".to_owned(),
            exit_code: None,
            error: None,
            created_at: UtcTimestamp::now(&SystemClock).to_string(),
            finished_at: None,
            capture_complete: true,
        };
        let folder = self.storage.join(&id);
        std::fs::create_dir_all(&folder).map_err(|error| refused(error.to_string()))?;
        std::fs::write(
            folder.join("request.json"),
            serde_json::to_vec(&record).map_err(|error| refused(error.to_string()))?,
        )
        .map_err(|error| refused(error.to_string()))?;
        let mut command = tokio::process::Command::new(executable);
        command
            .args(&parsed.arguments)
            .current_dir(directory)
            .env_clear()
            .envs(environment)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let child = match OwnedProcess::spawn(command) {
            Ok(child) => child,
            Err(error) => {
                let mut failed = record;
                "failed".clone_into(&mut failed.status);
                failed.error = Some(error.to_string());
                failed.finished_at = Some(UtcTimestamp::now(&SystemClock).to_string());
                save_result(&folder, &failed).map_err(refused)?;
                return Ok(json!({"process_id":id,"status":"failed","error":failed.error}));
            }
        };
        let active = Arc::new(Active {
            record: Mutex::new(record),
            cancel: Notify::new(),
        });
        self.active
            .lock()
            .map_err(|_| refused("process manager lock unavailable"))?
            .insert(id.clone(), Arc::clone(&active));
        let processes = Arc::clone(&self.active);
        let process_id = id.clone();
        tokio::spawn(async move {
            supervise(child, active, folder, processes, process_id).await;
        });
        Ok(
            json!({"process_id":id,"status":"running","note":"No overall deadline. Inspect status for logs/preview reachability; stop explicitly. Port reachability is not browser verification."}),
        )
    }

    fn load(&self, id: &str) -> Result<Record, AdapterError> {
        if !valid_id(id) {
            return Err(refused("invalid process id"));
        }
        if let Some(active) = self
            .active
            .lock()
            .map_err(|_| refused("process manager lock unavailable"))?
            .get(id)
        {
            return active
                .record
                .lock()
                .map(|record| record.clone())
                .map_err(|_| refused("process state lock unavailable"));
        }
        let folder = self.storage.join(id);
        let result = folder.join("result.json");
        let finished = result.is_file();
        let path = if finished {
            result
        } else {
            folder.join("request.json")
        };
        let bytes = std::fs::read(path)
            .map_err(|error| refused(format!("process record unavailable: {error}")))?;
        let mut record: Record =
            serde_json::from_slice(&bytes).map_err(|error| refused(error.to_string()))?;
        if !finished {
            "interrupted".clone_into(&mut record.status);
            record.error = Some("Daemon restarted or process outcome could not be recorded. Not resumed; no stale PID will be signalled.".to_owned());
        }
        Ok(record)
    }

    async fn status(&self, id: &str) -> Result<Value, AdapterError> {
        let record = self.load(id)?;
        let reachable = if record.status == "running" {
            if let Some(port) = record.port {
                port_reachable(port).await
            } else {
                false
            }
        } else {
            false
        };
        Ok(
            json!({"process":record,"stdout_tail":tail(&self.storage.join(id).join("stdout.log")),"stderr_tail":tail(&self.storage.join(id).join("stderr.log")),"preview_url":if reachable {record.port.map(|port|format!("http://127.0.0.1:{port}/"))} else {None},"port_reachable":reachable,"browser_verified":false,"note":"Host process, not a sandbox. Log tails and port checks are not evidence that the page or interactions work."}),
        )
    }

    fn stop(&self, id: &str) -> Result<Value, AdapterError> {
        self.load(id)?;
        let active = self
            .active
            .lock()
            .map_err(|_| refused("process manager lock unavailable"))?
            .get(id)
            .cloned();
        if let Some(active) = active {
            active.cancel.notify_one();
            return Ok(json!({"process_id":id,"stop_requested":true}));
        }
        Ok(
            json!({"process_id":id,"stop_requested":false,"note":"Not managed by this running daemon; no stale PID was killed."}),
        )
    }

    fn list(&self) -> Result<Value, AdapterError> {
        if !self.storage.exists() {
            return Ok(json!({"processes":[]}));
        }
        let mut ids = std::fs::read_dir(&self.storage)
            .map_err(|error| refused(error.to_string()))?
            .filter_map(Result::ok)
            .filter_map(|entry| {
                entry
                    .file_name()
                    .to_str()
                    .filter(|id| valid_id(id))
                    .map(str::to_owned)
            })
            .collect::<Vec<_>>();
        ids.sort_unstable_by(|left, right| right.cmp(left));
        let more = ids.len() > 20;
        let records = ids
            .iter()
            .take(20)
            .map(|id| self.load(id))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(json!({"processes":records,"more":more}))
    }
}

async fn supervise(
    mut child: OwnedProcess,
    active: Arc<Active>,
    folder: PathBuf,
    processes: Arc<Mutex<BTreeMap<String, Arc<Active>>>>,
    process_id: String,
) {
    let stdout = child
        .take_stdout()
        .map(|stream| AbortOnDrop::new(tokio::spawn(capture(stream, folder.join("stdout.log")))));
    let stderr = child
        .take_stderr()
        .map(|stream| AbortOnDrop::new(tokio::spawn(capture(stream, folder.join("stderr.log")))));
    let (mut status, code, mut error) = tokio::select! {
        biased;
        () = active.cancel.notified() => ("stopped", None, None),
        exit = child.wait() => match exit {
            Ok(exit) => (if exit.success() {"exited"} else {"failed"}, exit.code(), None),
            Err(error) => ("failed", None, Some(error.to_string())),
        }
    };
    if let Err(cleanup) = child.start_kill() {
        status = "failed";
        error = Some(format!("process-tree cleanup failed: {cleanup}"));
    } else if !matches!(
        tokio::time::timeout(Duration::from_secs(3), child.wait()).await,
        Ok(Ok(_))
    ) {
        status = "failed";
        error = Some("process exit could not be confirmed after cleanup".to_owned());
    }
    let complete = joined_capture(stdout).await & joined_capture(stderr).await;
    if let Ok(mut record) = active.record.lock() {
        status.clone_into(&mut record.status);
        record.exit_code = code;
        record.error = error;
        record.capture_complete = complete;
        record.finished_at = Some(UtcTimestamp::now(&SystemClock).to_string());
        if let Err(error) = save_result(&folder, &record) {
            tracing::warn!(%error, "managed process result could not be saved");
        }
    }
    if let Ok(mut processes) = processes.lock() {
        processes.remove(&process_id);
    }
}

fn valid_id(id: &str) -> bool {
    id.len() == 36
        && id.bytes().enumerate().all(|(index, byte)| {
            if matches!(index, 8 | 13 | 18 | 23) {
                byte == b'-'
            } else {
                byte.is_ascii_hexdigit()
            }
        })
}

fn save_result(folder: &Path, record: &Record) -> Result<(), String> {
    let bytes = serde_json::to_vec(record).map_err(|error| error.to_string())?;
    std::fs::write(folder.join("result.tmp"), bytes).map_err(|error| error.to_string())?;
    std::fs::rename(folder.join("result.tmp"), folder.join("result.json"))
        .map_err(|error| error.to_string())
}

async fn capture(mut stream: impl AsyncRead + Unpin, path: PathBuf) -> bool {
    let mut file = tokio::fs::File::create(path).await.ok();
    let mut count = 0;
    let mut complete = file.is_some();
    let mut buffer = [0_u8; 8192];
    loop {
        let size = match stream.read(&mut buffer).await {
            Ok(0) => break,
            Ok(size) => size,
            Err(_) => {
                complete = false;
                break;
            }
        };
        let keep = size.min(LOG_LIMIT.saturating_sub(count));
        if let Some(file) = &mut file
            && file.write_all(&buffer[..keep]).await.is_err()
        {
            complete = false;
        }
        count += keep;
        if keep != size {
            complete = false;
        }
    }
    if let Some(file) = &mut file {
        complete &= file.flush().await.is_ok();
    }
    complete
}

async fn joined_capture(handle: Option<AbortOnDrop<bool>>) -> bool {
    let Some(mut handle) = handle else {
        return false;
    };
    if let Ok(Ok(complete)) = tokio::time::timeout(Duration::from_secs(3), &mut handle).await {
        complete
    } else {
        false
    }
}

fn tail(path: &Path) -> String {
    let Ok(mut file) = std::fs::File::open(path) else {
        return String::new();
    };
    let _ = file
        .seek(SeekFrom::End(-2048))
        .or_else(|_| file.seek(SeekFrom::Start(0)));
    let mut bytes = Vec::new();
    let _ = file.take(2048).read_to_end(&mut bytes);
    let text = crate::command_run::clean(&String::from_utf8_lossy(&bytes));
    IsolatedText::new(&text).map_or_else(
        |_| "Log text could not be safely included.".to_owned(),
        |text| text.render(),
    )
}

async fn port_reachable(port: u16) -> bool {
    matches!(
        tokio::time::timeout(
            Duration::from_millis(300),
            tokio::net::TcpStream::connect((std::net::Ipv4Addr::LOCALHOST, port))
        )
        .await,
        Ok(Ok(_))
    )
}

#[async_trait]
impl ToolExecutor for ProcessTools {
    fn adapter_id(&self) -> &'static str {
        "managed-project-processes"
    }
    async fn execute(
        &self,
        request: &ToolExecutionRequest,
    ) -> Result<ToolCallResult, AdapterError> {
        if request.is_past_deadline(UtcTimestamp::now(&SystemClock)) {
            return Err(refused("call expired before execution"));
        }
        let tool = request.tool().to_string();
        let value = request.arguments();
        let body = match tool.as_str() {
            "jarvis.process.start" => self.start(value).await?,
            "jarvis.process.list" => self.list()?,
            "jarvis.process.status" => {
                self.status(value["process_id"].as_str().unwrap_or(""))
                    .await?
            }
            "jarvis.process.stop" => self.stop(value["process_id"].as_str().unwrap_or(""))?,
            _ => return Err(AdapterError::NotImplemented { tool }),
        };
        Ok(result(body))
    }
}

#[cfg(test)]
#[path = "process_tools_tests.rs"]
mod tests;
