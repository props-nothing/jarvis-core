//! Durable background coding tasks: start now, ask for status later, stop without losing the branch (`ADR-0209`).

#![allow(missing_docs, clippy::missing_errors_doc)]

use crate::code_delegate::{Arguments, CodeDelegateTool, DELEGATE_SCOPE, DELEGATE_TOOL};
use async_trait::async_trait;
use jarvis_core::{IsolatedText, Sensitivity, SystemClock, UtcTimestamp};
use jarvis_runtimes::acp::{ToolActivity, ToolNote};
use jarvis_storage::{DelegatedTaskFinish, NewDelegatedTask, SqliteDatabase, StoredDelegatedTask};
use jarvis_tools::{
    AdapterError, ApprovalPolicy, Availability, DelegationBoundary, EffectSet, Idempotency,
    ProviderEvidence, RetryDeclaration, Scope, ScopeSet, ToolCallResult, ToolDefinition,
    ToolDefinitionParts, ToolEffect, ToolExecutionRequest, ToolExecutor, ToolId, ToolOutcomeRecord,
    ToolSchema, ToolSensitivity, ToolSource,
};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::sync::{Notify, mpsc};

pub const START: &str = "jarvis.code.delegate_start";
pub const STATUS: &str = "jarvis.code.delegate_status";
pub const WAIT: &str = "jarvis.code.delegate_wait";
pub const LIST: &str = "jarvis.code.delegate_list";
pub const STOP: &str = "jarvis.code.delegate_stop";
pub const RESUME: &str = "jarvis.code.delegate_resume";
const START_INPUT: &str = crate::code_delegate::INPUT_SCHEMA;
const ID_INPUT: &str = r#"{"$schema":"https://json-schema.org/draft/2020-12/schema","type":"object","additionalProperties":false,"required":["task_id"],"properties":{"task_id":{"type":"string","minLength":36,"maxLength":36}}}"#;
const LIST_INPUT: &str = r#"{"$schema":"https://json-schema.org/draft/2020-12/schema","type":"object","additionalProperties":false,"properties":{"limit":{"type":"integer","minimum":1,"maximum":50}}}"#;
const WAIT_INPUT: &str = r#"{"$schema":"https://json-schema.org/draft/2020-12/schema","type":"object","additionalProperties":false,"required":["task_id"],"properties":{"task_id":{"type":"string","minLength":36,"maxLength":36},"seconds":{"type":"integer","minimum":1,"maximum":30,"default":30}}}"#;
const OUTPUT: &str =
    r#"{"$schema":"https://json-schema.org/draft/2020-12/schema","type":"object"}"#;

pub struct AsyncCodeDelegateTool {
    inner: Arc<CodeDelegateTool>,
    db: Arc<SqliteDatabase>,
    workspace: String,
    cancellations: Arc<Mutex<HashMap<String, Arc<Notify>>>>,
    notifications: bool,
}
impl AsyncCodeDelegateTool {
    pub fn new(inner: Arc<CodeDelegateTool>, db: Arc<SqliteDatabase>, workspace: String) -> Self {
        Self {
            inner,
            db,
            workspace,
            cancellations: Arc::new(Mutex::new(HashMap::new())),
            notifications: true,
        }
    }
    #[cfg(test)]
    fn without_notifications(mut self) -> Self {
        self.notifications = false;
        self
    }
    pub fn definitions() -> Result<Vec<ToolDefinition>, crate::code_delegate::CodeDelegateToolError>
    {
        let scope = ScopeSet::single(Scope::new(DELEGATE_SCOPE)?);
        let make = |id: &str,
                    title: &str,
                    description: &str,
                    input: &str,
                    effect: ToolEffect,
                    risk: u8,
                    approval: ApprovalPolicy,
                    timeout: u32|
         -> Result<ToolDefinition, crate::code_delegate::CodeDelegateToolError> {
            let boundary =
                (effect == ToolEffect::Delegation).then_some(DelegationBoundary::HostProcess);
            Ok(ToolDefinition::new(ToolDefinitionParts {
                id: ToolId::new(id)?,
                version: if boundary.is_some() { "1.1.0" } else { "1.0.0" }.into(),
                title: title.into(),
                description: description.into(),
                input_schema: ToolSchema::parse(input)?,
                output_schema: ToolSchema::parse(OUTPUT)?,
                effects: boundary
                    .map_or_else(|| EffectSet::single(effect), DelegationBoundary::effects),
                risk: boundary.map_or(risk, |boundary| boundary.effects().risk_floor()),
                required_scopes: scope.clone(),
                approval: boundary.map_or(approval, DelegationBoundary::approval),
                timeout_seconds: timeout,
                retry: RetryDeclaration::none(),
                idempotency: Idempotency::Unsupported,
                source: ToolSource::Native,
                availability: Availability::Available,
                sensitivity: ToolSensitivity::new(Sensitivity::Internal, Sensitivity::Internal),
            })?)
        };
        Ok(vec![
            CodeDelegateTool::definition()?,
            make(
                START,
                "Start a background coding task",
                "Starts a durable queued coding job in the folder permitted in Settings, in place by default and without an overall deadline. Commands and dependency setup are enabled by default for the approved task. Use delegate_status for progress and independent verification, delegate_stop to cancel. No Git operations unless worktree mode is explicitly selected. Finish website delivery with a managed preview and real browser checks; an agent summary is not evidence.",
                START_INPUT,
                ToolEffect::Delegation,
                3,
                ApprovalPolicy::Ask,
                60,
            )?,
            make(
                STATUS,
                "Check a coding task",
                "Returns the durable status, recent agent-reported tool activity and result of one background coding task. Activity is untrusted data, not independent verification.",
                ID_INPUT,
                ToolEffect::ReadOnly,
                0,
                ApprovalPolicy::Auto,
                30,
            )?,
            make(
                WAIT,
                "Wait for a coding task",
                "Waits up to 30 seconds for a coding job to end, then returns status and verification. A wait timeout leaves the job running. Prefer this over repeated status polling. Quiet activity and individual setup failures are not evidence of a stuck job; let the agent recover. Stop only at the owner's request or for a concrete unsafe effect.",
                WAIT_INPUT,
                ToolEffect::ReadOnly,
                0,
                ApprovalPolicy::Auto,
                40,
            )?,
            make(
                LIST,
                "List coding tasks",
                "Lists recent background coding tasks, newest first.",
                LIST_INPUT,
                ToolEffect::ReadOnly,
                0,
                ApprovalPolicy::Auto,
                30,
            )?,
            make(
                STOP,
                "Stop a coding task",
                "Cancels a queued or running coding job at the owner's request or to prevent a concrete unsafe effect. Do not cancel for quiet progress, an individual failed command, or to take over the implementation; use delegate_wait. No files are rolled back; queued cancellation starts no agent.",
                ID_INPUT,
                ToolEffect::Write,
                1,
                ApprovalPolicy::Auto,
                30,
            )?,
            make(
                RESUME,
                "Continue an interrupted coding task",
                "Starts a new agent on the saved request and existing files of an interrupted, failed or cancelled in-place job. Requires fresh owner approval. Not restoration of its old process or conversation; inspect current state before repeating effects.",
                ID_INPUT,
                ToolEffect::Delegation,
                3,
                ApprovalPolicy::Ask,
                60,
            )?,
        ])
    }
    #[cfg(test)]
    async fn start(&self, v: &Value) -> Result<Value, AdapterError> {
        self.start_for_call(v, None).await
    }
    async fn start_for_call(
        &self,
        v: &Value,
        call_id: Option<&str>,
    ) -> Result<Value, AdapterError> {
        let mut args = Arguments::parse(v).map_err(|reason| refused(&reason))?;
        args.repository = self
            .inner
            .resolve(&args.repository)?
            .to_string_lossy()
            .into_owned();
        let now = UtcTimestamp::now(&SystemClock);
        let request = serde_json::to_string(&args).map_err(|error| refused(&error.to_string()))?;
        let new = NewDelegatedTask {
            workspace_id: &self.workspace,
            repository: &args.repository,
            task: &args.task,
            allow_commands: args.allow_commands,
            minutes: args.minutes,
            now,
        };
        let (stored, created) = match call_id {
            Some(call_id) => {
                jarvis_storage::create_delegated_task_for_call(&self.db, &new, &request, call_id)
                    .await
            }
            None => {
                jarvis_storage::create_delegated_task_with_request(&self.db, &new, Some(&request))
                    .await
                    .map(|task| (task, true))
            }
        }
        .map_err(|error| {
            refused(&format!(
                "a background coding task could not be recorded: {error}"
            ))
        })?;
        if !created {
            return Ok(
                json!({"outcome":"existing","task_id":stored.id,"status":stored.status,
                "parent_run_id":stored.parent_run_id,"origin_call_id":stored.origin_call_id,
                "next":"This call already dispatched a task. Inspect its status; it was not launched again."}),
            );
        }
        self.spawn_task(stored.id.clone(), args)?;
        Ok(
            json!({"outcome":"started","task_id":stored.id,"parent_run_id":stored.parent_run_id,"origin_call_id":stored.origin_call_id,"status":"queued","next":"Use jarvis.code.delegate_wait with this task_id (30 seconds per wait). Quiet progress or a failed individual command is not job failure. Let the agent recover; do not stop it to take over. The owner will also get a desktop notification when it ends."}),
        )
    }
    fn spawn_task(&self, id: String, args: Arguments) -> Result<(), AdapterError> {
        let cancel = Arc::new(Notify::new());
        self.cancellations
            .lock()
            .map_err(|_| refused("the coding task manager is not available"))?
            .insert(id.clone(), Arc::clone(&cancel));
        let db = Arc::clone(&self.db);
        let inner = Arc::clone(&self.inner);
        let workspace = self.workspace.clone();
        let active = Arc::clone(&self.cancellations);
        let notifications = self.notifications;
        tokio::spawn(async move {
            let result = run_with_cancellation(&db, &workspace, &id, &inner, &args, &cancel).await;
            let Completion {
                status,
                branch,
                worktree,
                summary,
                result_json,
                error,
            } = completion(result);
            let finish = DelegatedTaskFinish {
                status,
                branch: branch.as_deref(),
                worktree: worktree.as_deref(),
                summary: summary.as_deref(),
                result_json: result_json.as_deref(),
                error: error.as_deref(),
                now: UtcTimestamp::now(&SystemClock),
            };
            let persisted =
                jarvis_storage::finish_delegated_task(&db, &workspace, &id, &finish).await;
            if let Ok(mut held) = active.lock() {
                held.remove(&id);
            }
            if let Err(error) = persisted {
                tracing::error!(task_id = %id, %error, "coding task result could not be recorded");
                let fallback = DelegatedTaskFinish {
                    status: "failed",
                    branch: None,
                    worktree: None,
                    summary: None,
                    result_json: None,
                    error: Some(
                        "The agent stopped, but its final result could not be saved. Inspect files before continuing.",
                    ),
                    now: UtcTimestamp::now(&SystemClock),
                };
                if let Err(error) =
                    jarvis_storage::finish_delegated_task(&db, &workspace, &id, &fallback).await
                {
                    tracing::error!(task_id = %id, %error, "coding task failure state could not be saved");
                }
                return;
            }
            if notifications {
                let status = jarvis_storage::find_delegated_task(&db, &workspace, &id)
                    .await
                    .map_or_else(|_| "unknown".to_owned(), |task| task.status);
                crate::notify::show(
                    "JARVIS coding task",
                    if status == "completed" {
                        "The coding agent finished. Open Coding tasks to inspect its files and verification."
                    } else if status == "cancelled" {
                        "The coding task was stopped. Its files are kept for review."
                    } else {
                        "The coding agent did not finish. Open Coding tasks for the reason."
                    },
                );
            }
        });
        Ok(())
    }
    async fn status(&self, v: &Value) -> Result<Value, AdapterError> {
        let id = id(v)?;
        let t = jarvis_storage::find_delegated_task(&self.db, &self.workspace, id)
            .await
            .map_err(|_| refused("no coding task has that id"))?;
        Ok(render(&t))
    }
    async fn list(&self, v: &Value) -> Result<Value, AdapterError> {
        let limit = v
            .get("limit")
            .and_then(Value::as_u64)
            .unwrap_or(10)
            .clamp(1, 50) as u32;
        let tasks = jarvis_storage::list_delegated_tasks(&self.db, &self.workspace, limit)
            .await
            .map_err(|_| refused("coding tasks could not be read"))?;
        let rows: Vec<_> = tasks
            .iter()
            .map(|task| {
                let result = task
                    .result_json
                    .as_deref()
                    .and_then(|text| serde_json::from_str::<Value>(text).ok());
                json!({"task_id":task.id,"parent_run_id":task.parent_run_id,"origin_call_id":task.origin_call_id,"cancellation_requested_at":task.cancellation_requested_at.map(|time|time.to_string()),"status":task.status,
                "verification":result.as_ref().map(|v|&v["verification"]["status"]),
                "created_at":task.created_at.to_string()})
            })
            .collect();
        Ok(json!({"outcome":"listed","count":rows.len(),"tasks":rows,
            "next":"Use delegate_status with a task_id for its brief, activity and verification."}))
    }

    async fn wait_for_task(&self, v: &Value) -> Result<Value, AdapterError> {
        let seconds = v
            .get("seconds")
            .and_then(Value::as_u64)
            .unwrap_or(30)
            .clamp(1, 30);
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(seconds);
        loop {
            let mut view = self.status(v).await?;
            if !matches!(view["status"].as_str(), Some("queued" | "running")) {
                view["wait_timed_out"] = json!(false);
                return Ok(view);
            }
            if tokio::time::Instant::now() >= deadline {
                view["wait_timed_out"] = json!(true);
                view["next"] = json!(
                    "The job is still active. Continue delegate_wait; this wait timeout is not a task failure or a reason to stop it."
                );
                return Ok(view);
            }
            tokio::time::sleep_until(
                (tokio::time::Instant::now() + std::time::Duration::from_millis(250)).min(deadline),
            )
            .await;
        }
    }
    async fn stop(&self, v: &Value) -> Result<Value, AdapterError> {
        let id = id(v)?;
        let requested = jarvis_storage::request_delegated_task_cancellation(
            &self.db,
            &self.workspace,
            id,
            UtcTimestamp::now(&SystemClock),
        )
        .await
        .map_err(|_| refused("the coding task cancellation could not be recorded"))?;
        if !requested {
            return Err(refused("the coding task is already finished"));
        }
        self.signal_stop(id);
        Ok(
            json!({"outcome":"stop_requested","task_id":id,"note":"Cancellation is recorded. Existing files are kept; queued work starts no agent."}),
        )
    }
    fn signal_stop(&self, id: &str) {
        if let Ok(active) = self.cancellations.lock()
            && let Some(cancel) = active.get(id)
        {
            cancel.notify_one();
        }
    }
    #[cfg(test)]
    async fn resume(&self, v: &Value) -> Result<Value, AdapterError> {
        self.resume_for_call(v, None).await
    }
    async fn resume_for_call(
        &self,
        value: &Value,
        call_id: Option<&str>,
    ) -> Result<Value, AdapterError> {
        let previous = jarvis_storage::find_delegated_task(&self.db, &self.workspace, id(value)?)
            .await
            .map_err(|_| refused("no coding task has that id"))?;
        if matches!(previous.status.as_str(), "queued" | "running" | "completed") {
            return Err(refused(
                "only interrupted, failed or cancelled tasks can be continued",
            ));
        }
        let mut request: Value =
            serde_json::from_str(previous.request_json.as_deref().ok_or_else(|| {
                refused(
                    "this older task has no saved approved request; start a new task explicitly",
                )
            })?)
            .map_err(|_| refused("the saved coding request is unreadable"))?;
        if request["mode"] == "worktree" {
            return Err(refused(
                "review the isolated worktree before continuing it; start a new in-place task at its explicitly permitted path",
            ));
        }
        let original = request["task"].as_str().unwrap_or("");
        if original.chars().count() > 49_600 {
            return Err(refused(
                "The saved brief is too long to add safe continuation instructions. Start a new task with a shorter remaining-work brief.",
            ));
        }
        request["task"] = json!(format!(
            "Continue an earlier task that stopped. First inspect existing files, installed dependencies and verification results; do not replay completed installs, external effects or overwrite user edits. Repair and complete only remaining work. Original brief: {original}"
        ));
        let mut result = self.start_for_call(&request, call_id).await?;
        result["continued_from"] = json!(previous.id);
        result["recovery"] = json!(
            "A new agent inspects existing files. This is not restoration of the old process or conversation; previously completed effects are not automatically replayed by JARVIS."
        );
        Ok(result)
    }
}
struct Completion {
    status: &'static str,
    branch: Option<String>,
    worktree: Option<String>,
    summary: Option<String>,
    result_json: Option<String>,
    error: Option<String>,
}

fn completion(result: Result<Value, AdapterError>) -> Completion {
    match result {
        Ok(value) => {
            let status = match value["outcome"].as_str() {
                Some("completed") => "completed",
                Some("cancelled") => "cancelled",
                _ => "failed",
            };
            Completion {
                status,
                branch: value["branch"].as_str().map(str::to_owned),
                worktree: value["worktree"].as_str().map(str::to_owned),
                summary: value
                    .get("agent_message")
                    .map(|message| message.to_string().chars().take(12_000).collect()),
                result_json: Some(bounded_result(value).to_string()),
                error: None,
            }
        }
        Err(error) => Completion {
            status: "failed",
            branch: None,
            worktree: None,
            summary: None,
            result_json: None,
            error: Some(error.to_string().chars().take(1000).collect()),
        },
    }
}

fn bounded_result(value: Value) -> Value {
    if value.to_string().len() <= 240_000 {
        return value;
    }
    let checks = value["verification"]["checks"].as_array().into_iter().flatten().take(16)
        .map(|check| json!({"command":check["command"],"passed":check["passed"],"result_omitted":true})).collect::<Vec<_>>();
    json!({"outcome":value["outcome"],"delivery_status":value["delivery_status"],"browser_verified":false,"directory":value["directory"],"mode":value["mode"],
        "branch":value["branch"],"worktree":value["worktree"],"details_omitted":true,
        "verification":{"status":value["verification"]["status"],"checks":checks},
        "next":"Detailed agent output exceeded storage limits. Inspect the files and rerun checks; no browser verification is implied."})
}

async fn run_with_cancellation(
    db: &Arc<SqliteDatabase>,
    workspace: &str,
    id: &str,
    inner: &CodeDelegateTool,
    args: &Arguments,
    cancel: &Notify,
) -> Result<Value, AdapterError> {
    // The row is authoritative. Polling also closes the persist/register gap and
    // reaches workers launched through other manager instances. Never drop the
    // running future to cancel it: the existing driver owns process-tree cleanup.
    let run = run_recorded_task(db, workspace, id, inner, args, cancel);
    tokio::pin!(run);
    let mut poll = tokio::time::interval(std::time::Duration::from_millis(100));
    loop {
        tokio::select! {
            result = &mut run => return result,
            _ = poll.tick() => {
                match jarvis_storage::delegated_task_cancellation_requested(db, workspace, id).await {
                    Ok(false) => {}
                    Ok(true) => cancel.notify_one(),
                    Err(error) => {
                        tracing::warn!(task_id = id, %error, "coding cancellation state unreadable; stopping the worker");
                        cancel.notify_one();
                    }
                }
            }
        }
    }
}

async fn run_recorded_task(
    db: &Arc<SqliteDatabase>,
    workspace: &str,
    id: &str,
    inner: &CodeDelegateTool,
    args: &Arguments,
    cancel: &Notify,
) -> Result<Value, AdapterError> {
    let Some(_permit) = inner.acquire(cancel).await? else {
        return Ok(
            json!({"outcome":"cancelled", "next":"Cancelled in the queue; no agent started."}),
        );
    };
    let _admission = match db.work_admission().admit() {
        Ok(permit) => permit,
        Err(jarvis_core::WorkAdmissionError::Stopping) => {
            return Ok(json!({"outcome":"cancelled","next":"Daemon stopping; no agent started."}));
        }
        Err(_) => {
            return Err(refused(
                "work admission unavailable; no coding agent was started",
            ));
        }
    };
    if let Err(error) = jarvis_storage::mark_delegated_task_running(
        db,
        workspace,
        id,
        UtcTimestamp::now(&SystemClock),
    )
    .await
    {
        if matches!(
            jarvis_storage::delegated_task_cancellation_requested(db, workspace, id).await,
            Ok(true)
        ) {
            return Ok(
                json!({"outcome":"cancelled","next":"Cancelled before dispatch; no agent started."}),
            );
        }
        tracing::error!(task_id = %id, %error, "coding task could not be marked running");
        return Err(refused(
            "the coding task could not be marked running; no agent was started",
        ));
    }
    let (sender, updates) = mpsc::channel(32);
    let recorder = tokio::spawn(record_activity(
        Arc::clone(db),
        workspace.to_owned(),
        id.to_owned(),
        updates,
    ));
    let result = inner.run_observing(args, cancel, Some(sender)).await;
    let history_saved = matches!(recorder.await, Ok(Ok(())));
    if !history_saved {
        tracing::warn!(task_id = %id, "coding task activity could not be recorded");
    }
    result.map(|mut value| {
        if !history_saved {
            value["activity_error"] = json!("Some coding-task activity could not be recorded.");
        }
        value
    })
}

async fn record_activity(
    db: Arc<SqliteDatabase>,
    workspace: String,
    id: String,
    mut updates: mpsc::Receiver<ToolActivity>,
) -> Result<(), jarvis_storage::DatabaseError> {
    while let Some(update) = updates.recv().await {
        jarvis_storage::append_delegated_task_activity(
            &db,
            &workspace,
            &id,
            update.sequence,
            &activity_summary(&update.tool),
            UtcTimestamp::now(&SystemClock),
        )
        .await?;
    }
    Ok(())
}

fn activity_summary(tool: &ToolNote) -> String {
    let mut summary = format!("[{}] {} ({})", tool.kind, tool.title, tool.status);
    for path in &tool.paths {
        summary.push('\n');
        summary.push_str(path);
    }
    summary
        .chars()
        .filter(|character| !character.is_control() || matches!(character, '\n' | '\t'))
        .take(jarvis_storage::MAX_TASK_ACTIVITY_CHARS)
        .collect()
}
fn id(v: &Value) -> Result<&str, AdapterError> {
    v.get("task_id")
        .and_then(Value::as_str)
        .filter(|id| id.len() == 36)
        .ok_or_else(|| refused("a 36-character task_id is required"))
}
pub(crate) fn render_console(task: &StoredDelegatedTask) -> Value {
    let request = task
        .request_json
        .as_deref()
        .and_then(|text| serde_json::from_str::<Value>(text).ok());
    let result = task
        .result_json
        .as_ref()
        .and_then(|text| serde_json::from_str::<Value>(text).ok());
    let expected = result
        .as_ref()
        .and_then(|value| value.get("activity_updates"))
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let limited = task
        .activity
        .first()
        .is_some_and(|entry| entry.sequence > 1)
        || task
            .activity
            .windows(2)
            .any(|pair| pair[0].sequence + 1 != pair[1].sequence)
        || task.activity.last().map_or(0, |entry| entry.sequence) < expected
        || result
            .as_ref()
            .and_then(|value| value.get("tool_notes_dropped"))
            .and_then(Value::as_u64)
            .unwrap_or(0)
            > 0;
    let activity: Vec<Value> = task.activity.iter().map(|entry| {
        json!({"sequence":entry.sequence,"summary":entry.summary,"created_at":entry.created_at.to_string()})
    }).collect();
    json!({"task_id":task.id,"parent_run_id":task.parent_run_id,"origin_call_id":task.origin_call_id,"cancellation_requested_at":task.cancellation_requested_at.map(|time|time.to_string()),"repository":task.repository,"task":task.task,"allow_commands":task.allow_commands,"minutes":task.minutes,"mode":request.as_ref().map(|request|&request["mode"]),"status":task.status,"branch":task.branch,"worktree":task.worktree,"summary":task.summary,"result":result,"error":task.error,"created_at":task.created_at.to_string(),"updated_at":task.updated_at.to_string(),"finished_at":task.finished_at.map(|time|time.to_string()),"activity":activity,"activity_limited":limited})
}

fn render(task: &StoredDelegatedTask) -> Value {
    let mut view = render_console(task);
    view["task"] = json!(task.task.chars().take(800).collect::<String>());
    view["task_truncated"] = json!(task.task.chars().count() > 800);
    // The summary duplicates agent_message and can dominate the entire response.
    if let Some(object) = view.as_object_mut() {
        object.remove("summary");
    }
    let mut limited = view["activity_limited"].as_bool().unwrap_or(false);
    if let Some(entries) = view["activity"].as_array_mut() {
        if entries.len() > 4 {
            entries.drain(..entries.len() - 4);
            limited = true;
        }
        for entry in entries {
            let text = entry["summary"].as_str().unwrap_or("");
            limited |= text.chars().count() > 600;
            let text: String = text.chars().take(600).collect();
            entry["summary"] = json!(IsolatedText::new(&text).map_or_else(
                |_| "Activity text could not be safely included.".to_owned(),
                |isolated| isolated.render(),
            ));
        }
    }
    view["activity_limited"] = json!(limited);
    compact_status(&mut view);
    view
}

fn compact_status(view: &mut Value) {
    if fits_model(view) {
        return;
    }
    let result = &view["result"];
    let checks: Vec<_> = result["verification"]["checks"].as_array().into_iter().flatten()
        .take(16).enumerate().map(|(index, check)| json!({"index":index,"passed":check["passed"].as_bool(),
            "reported_check_failure_count":check["result"]["reported_check_failures"].as_array().map_or(0, Vec::len),
            "exit_code":check["result"]["exit_code"].as_i64()})).collect();
    let bounded = |value: &Value| {
        value
            .as_str()
            .map(|text| text.chars().take(80).collect::<String>())
    };
    view["result"] = json!({"outcome":bounded(&result["outcome"]),"delivery_status":bounded(&result["delivery_status"]),
        "verification":{"status":bounded(&result["verification"]["status"]),"checks":checks,
            "issue_count":result["verification"]["issues"].as_array().map_or(0,Vec::len)},
        "browser_verified":false,"details_omitted":true});
    view["details_omitted"] = json!(true);
    view["next"] = json!(
        "Inspect retained details in the Coding tasks console; details_omitted there also discloses storage limits. Check statuses are preserved; omitted evidence is not success."
    );
    if !fits_model(view) {
        // Preserve the durable identity and truth even for maximal Unicode paths.
        *view = json!({"task_id":view["task_id"],"status":view["status"],"result":view["result"],
            "details_omitted":true,"next":"Read the Coding tasks console for full paths, brief and output."});
    }
}

fn fits_model(value: &Value) -> bool {
    let text = value.to_string();
    // Leave space for wait metadata in both UTF-8 bytes and model characters.
    text.len() <= jarvis_tools::MAX_TOOL_OUTPUT_BYTES - 500
        && text.chars().count() <= jarvis_tools::MAX_MODEL_FACING_RESULT_CHARS - 500
}
fn refused(reason: &str) -> AdapterError {
    AdapterError::RefusedBeforeReaching {
        reason: reason.into(),
    }
}
impl std::fmt::Debug for AsyncCodeDelegateTool {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AsyncCodeDelegateTool")
            .finish_non_exhaustive()
    }
}
#[async_trait]
impl ToolExecutor for AsyncCodeDelegateTool {
    fn adapter_id(&self) -> &'static str {
        "code-delegate-async"
    }
    async fn execute(&self, r: &ToolExecutionRequest) -> Result<ToolCallResult, AdapterError> {
        let now = UtcTimestamp::now(&SystemClock);
        if r.is_past_deadline(now) {
            return Err(refused("the call was already past its deadline"));
        }
        let body = match r.tool().to_string().as_str() {
            DELEGATE_TOOL | START => {
                self.start_for_call(r.arguments(), Some(r.call_id()))
                    .await?
            }
            STATUS => self.status(r.arguments()).await?,
            WAIT => self.wait_for_task(r.arguments()).await?,
            LIST => self.list(r.arguments()).await?,
            STOP => self.stop(r.arguments()).await?,
            RESUME => {
                self.resume_for_call(r.arguments(), Some(r.call_id()))
                    .await?
            }
            _ => {
                return Err(AdapterError::NotImplemented {
                    tool: r.tool().to_string(),
                });
            }
        };
        let ev = ProviderEvidence::new("code:background").ok();
        let rec = ev
            .as_ref()
            .and_then(|x| ToolOutcomeRecord::confirmed(x.as_str()).ok())
            .unwrap_or_else(|| {
                ToolOutcomeRecord::failed("evidence")
                    .unwrap_or_else(|_| unreachable!("a literal reason"))
            });
        Ok(ToolCallResult::new(
            rec,
            ev,
            Some(crate::skill_bundles::tool_output(&body)),
            now,
        ))
    }
}

#[cfg(test)]
#[path = "code_delegate_async_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "code_delegate_live_tests.rs"]
mod live_tests;
