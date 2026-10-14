//! Durable background coding tasks: start now, ask for status later, stop without losing the branch (`ADR-0209`).

#![allow(missing_docs, clippy::missing_errors_doc)]

use crate::code_delegate::{Arguments, CodeDelegateTool, DELEGATE_SCOPE, DELEGATE_TOOL};
use async_trait::async_trait;
use jarvis_core::{Sensitivity, SystemClock, UtcTimestamp};
use jarvis_storage::{DelegatedTaskFinish, NewDelegatedTask, SqliteDatabase, StoredDelegatedTask};
use jarvis_tools::{
    AdapterError, ApprovalPolicy, Availability, BoundedOutput, EffectSet, Idempotency,
    ProviderEvidence, RetryDeclaration, Scope, ScopeSet, ToolCallResult, ToolDefinition,
    ToolDefinitionParts, ToolEffect, ToolExecutionRequest, ToolExecutor, ToolId, ToolOutcomeRecord,
    ToolSchema, ToolSensitivity, ToolSource,
};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::sync::Notify;

pub const START: &str = "jarvis.code.delegate_start";
pub const STATUS: &str = "jarvis.code.delegate_status";
pub const LIST: &str = "jarvis.code.delegate_list";
pub const STOP: &str = "jarvis.code.delegate_stop";
const START_INPUT: &str = r#"{"$schema":"https://json-schema.org/draft/2020-12/schema","type":"object","additionalProperties":false,"required":["repository","task"],"properties":{"repository":{"type":"string","minLength":1,"maxLength":260},"task":{"type":"string","minLength":1,"maxLength":4000},"allow_commands":{"type":"boolean"},"minutes":{"type":"integer","minimum":1,"maximum":60}}}"#;
const ID_INPUT: &str = r#"{"$schema":"https://json-schema.org/draft/2020-12/schema","type":"object","additionalProperties":false,"required":["task_id"],"properties":{"task_id":{"type":"string","minLength":36,"maxLength":36}}}"#;
const LIST_INPUT: &str = r#"{"$schema":"https://json-schema.org/draft/2020-12/schema","type":"object","additionalProperties":false,"properties":{"limit":{"type":"integer","minimum":1,"maximum":50}}}"#;
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
            Ok(ToolDefinition::new(ToolDefinitionParts {
                id: ToolId::new(id)?,
                version: "1.0.0".into(),
                title: title.into(),
                description: description.into(),
                input_schema: ToolSchema::parse(input)?,
                output_schema: ToolSchema::parse(OUTPUT)?,
                effects: EffectSet::single(effect),
                risk,
                required_scopes: scope.clone(),
                approval,
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
                "Starts the coding agent in its own git worktree and returns immediately with a task id. Use this instead of jarvis.code.delegate when the work may take longer than a few minutes. The owner is asked first. Poll jarvis.code.delegate_status; nothing is merged or pushed.",
                START_INPUT,
                ToolEffect::CodeExecution,
                3,
                ApprovalPolicy::Ask,
                60,
            )?,
            make(
                STATUS,
                "Check a coding task",
                "Returns the durable status and result of one background coding task.",
                ID_INPUT,
                ToolEffect::ReadOnly,
                0,
                ApprovalPolicy::Auto,
                30,
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
                "Cancels a running coding agent. Its worktree and branch are kept for review.",
                ID_INPUT,
                ToolEffect::Write,
                1,
                ApprovalPolicy::Auto,
                30,
            )?,
        ])
    }
    async fn start(&self, v: &Value) -> Result<Value, AdapterError> {
        let args = Arguments::parse_with_max(v, 60).map_err(|reason| refused(&reason))?;
        let now = UtcTimestamp::now(&SystemClock);
        let stored = jarvis_storage::create_delegated_task(
            &self.db,
            &NewDelegatedTask {
                workspace_id: &self.workspace,
                repository: &args.repository,
                task: &args.task,
                allow_commands: args.allow_commands,
                minutes: args.minutes,
                now,
            },
        )
        .await
        .map_err(|e| {
            refused(&format!(
                "a background coding task could not be recorded: {e}"
            ))
        })?;
        let cancel = Arc::new(Notify::new());
        self.cancellations
            .lock()
            .map_err(|_| refused("the coding task manager is not available"))?
            .insert(stored.id.clone(), Arc::clone(&cancel));
        let id = stored.id.clone();
        let db = Arc::clone(&self.db);
        let inner = Arc::clone(&self.inner);
        let workspace = self.workspace.clone();
        let active = Arc::clone(&self.cancellations);
        let notifications = self.notifications;
        tokio::spawn(async move {
            let result = run_recorded_task(&db, &workspace, &id, &inner, &args, &cancel).await;
            let (status, branch, worktree, summary, result_json, error) = match result {
                Ok(value) => {
                    let outcome = value
                        .get("outcome")
                        .and_then(Value::as_str)
                        .unwrap_or("failed");
                    let status = if outcome == "completed" {
                        "completed"
                    } else if outcome == "cancelled" {
                        "cancelled"
                    } else {
                        "failed"
                    };
                    (
                        status,
                        value
                            .get("branch")
                            .and_then(Value::as_str)
                            .map(str::to_owned),
                        value
                            .get("worktree")
                            .and_then(Value::as_str)
                            .map(str::to_owned),
                        value.get("agent_message").map(Value::to_string),
                        Some(value.to_string()),
                        None,
                    )
                }
                Err(error) => ("failed", None, None, None, None, Some(error.to_string())),
            };
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
                return;
            }
            if notifications {
                crate::notify::show(
                    "JARVIS coding task",
                    if status == "completed" {
                        "The coding agent finished. Open Coding tasks to review its branch."
                    } else if status == "cancelled" {
                        "The coding task was stopped. Its branch is kept for review."
                    } else {
                        "The coding agent did not finish. Open Coding tasks for the reason."
                    },
                );
            }
        });
        Ok(
            json!({"outcome":"started","task_id":stored.id,"status":"queued","next":"Use jarvis.code.delegate_status with this task_id. The owner will also get a desktop notification when it ends."}),
        )
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
        Ok(
            json!({"outcome":"listed","count":tasks.len(),"tasks":tasks.iter().map(render).collect::<Vec<_>>() }),
        )
    }
    fn stop(&self, v: &Value) -> Result<Value, AdapterError> {
        let id = id(v)?;
        let found = self
            .cancellations
            .lock()
            .map_err(|_| refused("the coding task manager is not available"))?
            .get(id)
            .cloned();
        if let Some(cancel) = found {
            cancel.notify_one();
            Ok(
                json!({"outcome":"stop_requested","task_id":id,"note":"The worktree and branch will be kept."}),
            )
        } else {
            Err(refused("that coding task is not running"))
        }
    }
}
async fn run_recorded_task(
    db: &SqliteDatabase,
    workspace: &str,
    id: &str,
    inner: &CodeDelegateTool,
    args: &Arguments,
    cancel: &Notify,
) -> Result<Value, AdapterError> {
    jarvis_storage::mark_delegated_task_running(db, workspace, id, UtcTimestamp::now(&SystemClock))
        .await
        .map_err(|error| {
            tracing::error!(task_id = %id, %error, "coding task could not be marked running");
            refused("the coding task could not be marked running; no agent was started")
        })?;
    inner.run_with_cancel(args, cancel).await
}
fn id(v: &Value) -> Result<&str, AdapterError> {
    v.get("task_id")
        .and_then(Value::as_str)
        .filter(|id| id.len() == 36)
        .ok_or_else(|| refused("a 36-character task_id is required"))
}
pub(crate) fn render(t: &StoredDelegatedTask) -> Value {
    json!({"task_id":t.id,"repository":t.repository,"task":t.task,"allow_commands":t.allow_commands,"minutes":t.minutes,"status":t.status,"branch":t.branch,"worktree":t.worktree,"summary":t.summary,"result":t.result_json.as_ref().and_then(|s|serde_json::from_str::<Value>(s).ok()),"error":t.error,"created_at":t.created_at.to_string(),"updated_at":t.updated_at.to_string(),"finished_at":t.finished_at.map(|x|x.to_string())})
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
            DELEGATE_TOOL => return self.inner.execute(r).await,
            START => self.start(r.arguments()).await?,
            STATUS => self.status(r.arguments()).await?,
            LIST => self.list(r.arguments()).await?,
            STOP => self.stop(r.arguments())?,
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
            Some(BoundedOutput::from_bounded(body.to_string(), false)),
            now,
        ))
    }
}

#[cfg(test)]
#[path = "code_delegate_async_tests.rs"]
mod tests;
