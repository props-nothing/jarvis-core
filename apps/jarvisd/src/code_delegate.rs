//! Coding delivery in the folders permitted by the owner in Settings.
//!
//! In-place editing is the default: no Git prerequisite, worktree or automatic commit.
//! An explicit worktree mode remains available. The background manager owns the durable
//! queue and cancellation; there is no overall deadline unless the owner selects one.
//! Commands and network access belong to the approved task. The permission policy is
//! cooperative containment, not an OS sandbox. Agent output is untrusted and independent
//! verification commands are reported separately from the agent's own claims.

use std::fmt::Write as _;
use std::path::{Component, Path, PathBuf};
use std::time::Duration;

use async_trait::async_trait;
use jarvis_core::{IsolatedText, Sensitivity, SystemClock, UtcTimestamp};
use jarvis_runtimes::acp::{
    Connection, Decision, Ended, Limits, PermissionPolicy, PermissionRequest, Report, StopReason,
    ToolActivity,
};
use jarvis_runtimes::worktree::{self, Outcome, Repository, Worktree};
use jarvis_storage::AppPaths;
use jarvis_tools::{
    AdapterError, Availability, BoundedOutput, Idempotency, ProviderEvidence, RetryDeclaration,
    SchemaError, Scope, ScopeError, ScopeSet, ToolCallResult, ToolDefinition, ToolDefinitionError,
    ToolDefinitionParts, ToolExecutionRequest, ToolExecutor, ToolId, ToolIdError,
    ToolOutcomeRecord, ToolSchema, ToolSensitivity, ToolSource,
};
use serde_json::{Value, json};
use thiserror::Error;
use tokio::sync::{Notify, mpsc};

use crate::code_agents::{self, AgentSettings};
use crate::command_run::{CommandRunTool, plain};

/// The canonical identifier the model requests.
pub const DELEGATE_TOOL: &str = "jarvis.code.delegate";
/// The scope a caller must hold for the tool to be authorized at all.
pub const DELEGATE_SCOPE: &str = "code.delegate";

const DEFAULT_MINUTES: u64 = 0;
const TOOL_TIMEOUT_SECONDS: u32 = 60;
const HANDSHAKE_SECONDS: u64 = 90;
const MAX_TASK_CHARS: usize = 50_000;
const PATCH_SHOWN_CHARS: usize = 12_000;
const FENCE_PIECE_CHARS: usize = 3_500;

pub(crate) const INPUT_SCHEMA: &str = r#"{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "type": "object",
  "additionalProperties": false,
  "required": ["task"],
  "properties": {
    "repository": { "type": "string", "maxLength": 4096, "description": "Working folder, not necessarily a Git repository. Defaults to the first folder permitted in Settings. Relative paths are within permitted folders; an absolute path selects a permitted folder unambiguously." },
    "task": { "type": "string", "minLength": 1, "maxLength": 50000, "description": "The full implementation brief, acceptance criteria and verification requirements." },
    "mode": { "type": "string", "enum": ["in_place", "worktree"], "default": "in_place", "description": "in_place edits the selected permitted directory, with current uncommitted files visible and no automatic Git commands. worktree explicitly opts into an isolated branch." },
    "allow_commands": { "type": "boolean", "default": true, "description": "Allow project setup, dependency installation, builds and tests as part of this approved task. Host execution, not an OS sandbox. No unrequested global installs or deployment." },
    "allow_network": { "type": "boolean", "default": true, "description": "Allow the agent to fetch documentation and task dependencies. Does not grant account credentials or publish permission." },
    "minutes": { "type": "integer", "minimum": 0, "description": "Optional owner-selected deadline. Omit or use 0 for no overall deadline. Stop remains available." },
    "skills": { "type": "array", "maxItems": 32, "items": { "type": "string", "minLength": 1, "maxLength": 64 }, "description": "Installed project skill names to activate; missing requested skills fail before starting the agent." },
    "checks": { "type": "array", "maxItems": 16, "items": { "type": "object", "additionalProperties": false, "required": ["program"], "properties": { "program": { "type": "string", "maxLength": 64 }, "arguments": { "type": "array", "maxItems": 24, "items": { "type": "string", "maxLength": 240 } }, "timeout_seconds": { "type": "integer", "minimum": 1, "maximum": 540 } } }, "description": "Independent commands JARVIS runs in the same folder after the agent finishes, e.g. npm run build. Without checks, output is unverified, not a verified delivery. Website tasks also need browser inspection." }
  }
}"#;

const OUTPUT_SCHEMA: &str = r#"{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "type": "object"
}"#;

/// Why this adapter could not state its own contract or be built.
#[derive(Debug, Error)]
pub enum CodeDelegateToolError {
    /// The canonical identifier was rejected.
    #[error("the code delegation tool's identifier was rejected: {0}")]
    Identifier(#[from] ToolIdError),
    /// A schema was rejected.
    #[error("the code delegation tool's schema was rejected: {0}")]
    Schema(#[from] SchemaError),
    /// The required scope was rejected.
    #[error("the code delegation tool's scope was rejected: {0}")]
    Scope(#[from] ScopeError),
    /// The definition was rejected as a whole.
    #[error("the code delegation tool's definition was rejected: {0}")]
    Definition(#[from] ToolDefinitionError),
    /// No granted folder could be used.
    #[error("the code delegation tool has no usable granted folder")]
    NoFolders,
}

/// The arguments of one call, validated.
#[derive(Clone, Debug, serde::Serialize)]
pub(crate) struct Arguments {
    pub(crate) repository: String,
    pub(crate) task: String,
    pub(crate) allow_commands: bool,
    pub(crate) allow_network: bool,
    pub(crate) minutes: u64,
    pub(crate) mode: String,
    pub(crate) skills: Vec<String>,
    pub(crate) checks: Vec<Value>,
}

impl Arguments {
    pub(crate) fn parse(value: &Value) -> Result<Self, String> {
        let repository = match value.get("repository") {
            None | Some(Value::Null) => ".".to_owned(),
            Some(Value::String(text)) if text.len() <= 4096 => text.trim().to_owned(),
            _ => return Err("repository must name a permitted working directory".to_owned()),
        };
        let task = value
            .get("task")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|text| !text.is_empty() && text.chars().count() <= MAX_TASK_CHARS)
            .ok_or("task must contain 1 to 50000 characters")?;
        let minutes = match value.get("minutes") {
            None | Some(Value::Null) => DEFAULT_MINUTES,
            Some(value) => value
                .as_u64()
                .filter(|minutes| {
                    minutes
                        .checked_mul(60)
                        .and_then(|seconds| {
                            std::time::Instant::now().checked_add(Duration::from_secs(seconds))
                        })
                        .is_some()
                })
                .ok_or("minutes must be a nonnegative, representable duration")?,
        };
        let boolean = |name: &str| match value.get(name) {
            None => Ok(true),
            Some(Value::Bool(value)) => Ok(*value),
            _ => Err(format!("{name} must be a boolean")),
        };
        let mode = match value.get("mode") {
            None => "in_place",
            Some(Value::String(mode)) if matches!(mode.as_str(), "in_place" | "worktree") => mode,
            _ => return Err("mode must be in_place or worktree".to_owned()),
        };
        let skills = match value.get("skills") {
            None => Vec::new(),
            Some(Value::Array(items)) if items.len() <= 32 => items
                .iter()
                .map(|item| {
                    item.as_str()
                        .filter(|name| crate::skill_bundles::valid_name(name))
                        .map(str::to_owned)
                        .ok_or_else(|| "invalid skill name".to_owned())
                })
                .collect::<Result<Vec<_>, _>>()?,
            _ => return Err("skills must be an array of names".to_owned()),
        };
        let checks = match value.get("checks") {
            None => Vec::new(),
            Some(Value::Array(items)) if items.len() <= 16 => {
                for item in items {
                    if !item.is_object()
                        || item.as_object().is_some_and(|object| {
                            object.keys().any(|key| {
                                !matches!(key.as_str(), "program" | "arguments" | "timeout_seconds")
                            })
                        })
                    {
                        return Err(
                            "a check accepts only program, arguments and timeout_seconds"
                                .to_owned(),
                        );
                    }
                    crate::command_run::CommandArgs::parse(item)?;
                }
                items.clone()
            }
            _ => return Err("checks must be an array of commands".to_owned()),
        };
        let allow_commands = boolean("allow_commands")?;
        if !allow_commands && !checks.is_empty() {
            return Err("verification checks require allow_commands".to_owned());
        }
        Ok(Self {
            repository,
            task: task.to_owned(),
            allow_commands,
            allow_network: boolean("allow_network")?,
            minutes,
            mode: mode.to_owned(),
            skills,
            checks,
        })
    }
}

/// Answers an agent's permission requests for one task: deterministic, and never a model.
pub struct WorktreePolicy {
    root: PathBuf,
    allow_commands: bool,
    allow_network: bool,
}

impl WorktreePolicy {
    /// A policy for an agent working in `root`.
    #[must_use]
    pub fn new(root: &Path, allow_commands: bool) -> Self {
        Self {
            root: std::fs::canonicalize(root)
                .map_or_else(|_| root.to_path_buf(), |real| plain(&real)),
            allow_commands,
            allow_network: false,
        }
    }

    fn verdict(&self, request: &PermissionRequest) -> (bool, &'static str) {
        match request.kind.as_str() {
            "think" => (true, "thinking needs no files"),
            "read" | "search" => {
                if request.paths.iter().all(|path| self.confined(path)) {
                    (true, "reading inside the worktree")
                } else {
                    (false, "reading outside the worktree")
                }
            }
            "edit" | "delete" | "move" => {
                if request.paths.is_empty() {
                    (
                        false,
                        "no path was given, so it cannot be shown to stay inside the worktree",
                    )
                } else if request.paths.iter().all(|path| self.confined(path)) {
                    (true, "changing files inside the worktree")
                } else {
                    (false, "changing files outside the worktree")
                }
            }
            "execute" => {
                if self.allow_commands {
                    (true, "commands were allowed for this task")
                } else {
                    (false, "commands were not allowed for this task")
                }
            }
            "fetch" => (
                self.allow_network,
                if self.allow_network {
                    "network access was allowed for this task"
                } else {
                    "network access was not allowed for this task"
                },
            ),
            _ => (false, "that kind of request is not allowed"),
        }
    }

    /// Whether `candidate` is an absolute path that resolves inside the worktree. The part of it that exists is resolved, so a link
    /// inside the worktree that points out does not pass; the part that does not exist yet is new and lexically below it.
    fn confined(&self, candidate: &str) -> bool {
        let path = Path::new(candidate);
        if !path.is_absolute() {
            return false;
        }
        let mut normal = PathBuf::new();
        for component in path.components() {
            match component {
                Component::ParentDir => {
                    if !normal.pop() {
                        return false;
                    }
                }
                Component::CurDir => {}
                other => normal.push(other.as_os_str()),
            }
        }
        let mut ancestor = normal.as_path();
        loop {
            if ancestor.exists() {
                return std::fs::canonicalize(ancestor)
                    .is_ok_and(|real| starts_with_ignoring_case(&plain(&real), &self.root));
            }
            match ancestor.parent() {
                Some(parent) => ancestor = parent,
                None => return false,
            }
        }
    }
}

fn starts_with_ignoring_case(path: &Path, root: &Path) -> bool {
    if cfg!(windows) {
        let lower = |path: &Path| -> Vec<String> {
            path.components()
                .map(|part| part.as_os_str().to_string_lossy().to_lowercase())
                .collect()
        };
        lower(path).starts_with(&lower(root))
    } else {
        path.starts_with(root)
    }
}

#[async_trait]
impl PermissionPolicy for WorktreePolicy {
    async fn decide(&self, request: &PermissionRequest) -> Decision {
        let (allow, reason) = self.verdict(request);
        Decision {
            allow,
            reason: reason.to_owned(),
        }
    }
}

/// Where the agent to start comes from: the saved settings, read at each task so a change applies at once.
pub enum AgentSource {
    /// The saved settings of the profile.
    Saved(AppPaths),
    /// Settings fixed at construction (tests).
    #[cfg(test)]
    Fixed(AgentSettings),
}

impl AgentSource {
    fn load(&self) -> Result<AgentSettings, String> {
        match self {
            Self::Saved(paths) => AgentSettings::load(paths),
            #[cfg(test)]
            Self::Fixed(settings) => Ok(settings.clone()),
        }
    }
}

/// The adapter behind `jarvis.code.delegate`.
pub struct CodeDelegateTool {
    folders: CommandRunTool,
    worktrees: PathBuf,
    agent: AgentSource,
    limits: Limits,
    /// One coding task at a time: each is a process tree and a model's worth of spend.
    slot: tokio::sync::Semaphore,
}

fn project_checks(directory: &Path) -> Vec<Value> {
    use std::io::Read as _;
    let Ok(file) = std::fs::File::open(directory.join("package.json")) else {
        return Vec::new();
    };
    let mut text = String::new();
    if file.take(1_048_577).read_to_string(&mut text).is_err() || text.len() > 1_048_576 {
        return Vec::new();
    }
    let Ok(package) = serde_json::from_str::<Value>(&text) else {
        return Vec::new();
    };
    let manager = package["packageManager"].as_str().unwrap_or("");
    let program = if manager.starts_with("pnpm@") || directory.join("pnpm-lock.yaml").is_file() {
        "pnpm"
    } else if manager.starts_with("yarn@") || directory.join("yarn.lock").is_file() {
        "yarn"
    } else if manager.starts_with("bun@")
        || directory.join("bun.lock").is_file()
        || directory.join("bun.lockb").is_file()
    {
        "bun"
    } else {
        "npm"
    };
    ["build", "lint", "typecheck"]
        .iter()
        .filter(|name| package["scripts"][**name].is_string())
        .map(|name| json!({"program":program,"arguments":["run",name],"timeout_seconds":540}))
        .collect()
}

impl CodeDelegateTool {
    /// Builds the adapter over the granted folders. `worktrees` is where the agent's worktrees are made (inside the profile, not in a
    /// granted folder); `agent` says where the agent to start comes from.
    ///
    /// # Errors
    ///
    /// Returns [`CodeDelegateToolError::NoFolders`] when no granted folder can be resolved.
    pub fn new(
        roots: &[PathBuf],
        worktrees: PathBuf,
        agent: AgentSource,
    ) -> Result<Self, CodeDelegateToolError> {
        let folders = CommandRunTool::new(roots).map_err(|_| CodeDelegateToolError::NoFolders)?;
        Ok(Self {
            folders,
            worktrees,
            agent,
            limits: Limits::default(),
            slot: tokio::sync::Semaphore::new(1),
        })
    }

    /// The tool's definition.
    ///
    /// # Errors
    ///
    /// Returns [`CodeDelegateToolError`] when a constant of the contract is rejected: a configuration fault, so the daemon fails at
    /// startup rather than on the first call.
    pub fn definition() -> Result<ToolDefinition, CodeDelegateToolError> {
        Ok(ToolDefinition::new(ToolDefinitionParts {
            id: ToolId::new(DELEGATE_TOOL)?,
            version: "1.1.0".to_owned(),
            title: "Hand a coding task to the coding agent".to_owned(),
            description: "Starts an owner's coding-agent job and returns its task id immediately. Uses the working folder permitted in Settings (the first one by default), in place with existing files visible. No Git requirement or automatic Git mutation. Commands, project-local dependency installation and documentation fetches are enabled by default as part of this approved host-execution task. No overall deadline by default: monitor with jarvis.code.delegate_status, cancel with jarvis.code.delegate_stop. Prefer a full implementation brief and independent checks. An agent finishing is not a verified website; inspect verification, run a managed preview and check desktop/mobile behavior before claiming delivery. If skills are needed, discover/install compatible bundles first. Missing tools and sign-in are blockers, not success.".to_owned(),
            input_schema: ToolSchema::parse(INPUT_SCHEMA)?,
            output_schema: ToolSchema::parse(OUTPUT_SCHEMA)?,
            effects: jarvis_tools::DelegationBoundary::HostProcess.effects(),
            risk: jarvis_tools::DelegationBoundary::HostProcess.effects().risk_floor(),
            required_scopes: ScopeSet::single(Scope::new(DELEGATE_SCOPE)?),
            // `Ask`: a person decides, every time, until the owner marks the tool as trusted in Permissions.
            approval: jarvis_tools::DelegationBoundary::HostProcess.approval(),
            timeout_seconds: TOOL_TIMEOUT_SECONDS,
            // A retry would start a second agent on the same task.
            retry: RetryDeclaration::none(),
            idempotency: Idempotency::Unsupported,
            source: ToolSource::Native,
            availability: Availability::Available,
            sensitivity: ToolSensitivity::new(Sensitivity::Internal, Sensitivity::Internal),
        })?)
    }

    /// Runs one delegated task and describes what happened.
    #[cfg(test)]
    async fn run(&self, arguments: &Arguments) -> Result<Value, AdapterError> {
        let _permit = self
            .slot
            .acquire()
            .await
            .map_err(|_| refused("the coding queue is closed"))?;
        self.run_with_cancel(arguments, &Notify::new()).await
    }

    pub(crate) async fn run_with_cancel(
        &self,
        arguments: &Arguments,
        cancel: &Notify,
    ) -> Result<Value, AdapterError> {
        self.run_observing(arguments, cancel, None).await
    }

    async fn prepare_workspace(
        &self,
        directory: &Path,
        arguments: &Arguments,
    ) -> Result<Option<(Repository, worktree::Snapshot, Worktree)>, AdapterError> {
        if arguments.mode == "worktree" {
            let repository = worktree::open_repository(directory)
                .await
                .map_err(|error| refused(&error.to_string()))?;
            let before = worktree::snapshot(repository.top())
                .await
                .map_err(|error| refused(&error.to_string()))?;
            let tree =
                worktree::create(&repository, &self.worktrees, &arguments.task, &unique_id())
                    .await
                    .map_err(|error| refused(&error.to_string()))?;
            Ok(Some((repository, before, tree)))
        } else {
            Ok(None)
        }
    }

    pub(crate) async fn run_observing(
        &self,
        arguments: &Arguments,
        cancel: &Notify,
        activity: Option<mpsc::Sender<ToolActivity>>,
    ) -> Result<Value, AdapterError> {
        if stop_requested(cancel).await {
            return Ok(cancelled_setup(None));
        }
        let directory = self
            .folders
            .resolve_directory(&arguments.repository)
            .map_err(|reason| refused(&reason))?;
        let skill_context = crate::skill_bundles::context(&directory, &arguments.skills)
            .map_err(|reason| refused(&reason))?;
        let settings = self.agent.load().map_err(|reason| refused(&reason))?;
        let environment = code_agents::task_environment(
            &settings,
            arguments.allow_commands,
            arguments.allow_network,
        );
        let search_path = environment
            .get("PATH")
            .map(std::ffi::OsString::from)
            .unwrap_or_default();
        let pathext = environment.get("PATHEXT").cloned();
        let (program, program_arguments) =
            code_agents::resolve(&settings.command, &search_path, pathext.as_deref())
                .map_err(|reason| refused(&reason))?;

        let isolated = self.prepare_workspace(&directory, arguments).await?;
        let working = isolated
            .as_ref()
            .map_or(directory.as_path(), |(_, _, tree)| tree.path.as_path());
        if stop_requested(cancel).await {
            return Ok(cancelled_setup(isolated.as_ref().map(|(_, _, tree)| tree)));
        }
        let child = match code_agents::spawn(&program, &program_arguments, working, &environment) {
            Ok(child) => child,
            Err(reason) => {
                if let Some((repository, _, tree)) = &isolated {
                    let _ = worktree::discard(repository, tree).await;
                }
                return Err(refused(&reason));
            }
        };
        let limits = Limits {
            handshake: Duration::from_secs(HANDSHAKE_SECONDS),
            turn: Duration::from_secs(
                arguments
                    .minutes
                    .checked_mul(60)
                    .ok_or_else(|| refused("the requested duration is not representable"))?,
            ),
            ..self.limits.clone()
        };
        let opened = {
            let opening =
                Connection::open_owned(child, working, limits, settings.sign_in.as_deref());
            tokio::pin!(opening);
            tokio::select! {
                biased;
                () = cancel.notified() => None,
                connection = &mut opening => Some(connection),
            }
        };
        let mut connection = match opened {
            Some(Ok(connection)) => connection,
            Some(Err(error)) => {
                if let Some((repository, _, tree)) = &isolated {
                    let _ = worktree::discard(repository, tree).await;
                }
                return Err(refused(&code_agents::describe_error(&error)));
            }
            None => return Ok(cancelled_setup(isolated.as_ref().map(|(_, _, tree)| tree))),
        };
        if let Some(sender) = activity {
            connection.set_activity_sender(sender);
        }
        let mut policy = WorktreePolicy::new(working, arguments.allow_commands);
        policy.allow_network = arguments.allow_network;
        let report = connection
            .prompt(
                &framed_task(working, arguments, &skill_context),
                &policy,
                cancel,
            )
            .await;
        let cleanup = connection.close_checked().await;
        let mut answer = workspace_result(&program, working, isolated.as_ref(), &report).await;
        answer["directory"] = json!(working.to_string_lossy());
        answer["mode"] = json!(arguments.mode);
        answer["verification"] = json!({"status":"not_run", "checks":[]});
        if let Err(error) = cleanup {
            answer["outcome"] = json!("cleanup_failed");
            answer["cleanup_error"] = json!(error.to_string());
        } else if report.ended == Ended::Finished(StopReason::EndTurn) {
            answer["verification"] = self.verify(working, arguments, cancel).await?;
        }
        classify_delivery(&mut answer);
        Ok(answer)
    }
}

fn classify_delivery(answer: &mut Value) {
    match answer["verification"]["status"].as_str() {
        Some("failed") => answer["outcome"] = json!("verification_failed"),
        Some("cancelled") => answer["outcome"] = json!("cancelled"),
        _ => {}
    }
    answer["delivery_status"] = json!(match answer["verification"]["status"].as_str() {
        Some("passed") => "checks_passed",
        Some("failed") => "verification_failed",
        _ => "unverified",
    });
    answer["browser_verified"] = json!(false);
    answer["next"] = json!(
        "Work is in directory. No automatic commit, merge or push. Read verification: not_run is unverified, failed needs repair. Website delivery additionally requires starting a managed preview and checking real desktop/mobile behavior in the browser. Do not claim completion from the agent summary alone."
    );
}

async fn workspace_result(
    program: &Path,
    working: &Path,
    isolated: Option<&(Repository, worktree::Snapshot, Worktree)>,
    report: &Report,
) -> Value {
    if let Some((repository, before, tree)) = isolated {
        let after = worktree::snapshot(repository.top()).await;
        let original_changed = after.as_ref().map_or(true, |after| after != before);
        let outcome = worktree::review(tree).await;
        describe(program, repository, tree, report, outcome, original_changed)
    } else {
        describe_in_place(program, working, report)
    }
}

async fn stop_requested(cancel: &Notify) -> bool {
    tokio::select! {
        biased;
        () = cancel.notified() => true,
        () = std::future::ready(()) => false,
    }
}

fn cancelled_setup(tree: Option<&Worktree>) -> Value {
    let mut answer = json!({
        "outcome": "cancelled",
        "next": "No coding agent or worktree was started.",
    });
    if let Some(tree) = tree {
        answer["branch"] = json!(tree.branch);
        answer["worktree"] = json!(tree.path.to_string_lossy());
        answer["base_commit"] = json!(tree.base);
        answer["next"] = json!(
            "The coding task stopped before its prompt began. Its worktree is kept for review; nothing was merged or pushed."
        );
    }
    answer
}

fn unique_id() -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| since.as_nanos());
    format!("{:08x}", nanos & 0xffff_ffff)
}

fn refused(reason: &str) -> AdapterError {
    AdapterError::RefusedBeforeReaching {
        reason: reason.to_owned(),
    }
}

/// The brief the agent is given: where it is, what it must not do, and the task as the owner approved it.
fn framed_task(directory: &Path, arguments: &Arguments, skills: &str) -> String {
    let workspace = if arguments.mode == "worktree" {
        "This explicit Git worktree was requested for this task. Work here; do not switch to another checkout."
    } else {
        "This folder was selected from the owner's permitted folders in Settings. Work here, not in a profile/cache worktree."
    };
    let network = if arguments.allow_network {
        "Network access for documentation and task dependencies is authorized; account credentials and publication are not granted. If an npm install reports ENOTCACHED or only-if-cached, retry with --offline=false and --cache .jarvis-cache/npm. If the runtime sandbox blocks that authorized command, request command escalation through the runtime's permission mechanism so JARVIS can decide it. Do not disable the sandbox, change global configuration, or repeatedly retry a refused request. Report the exact blocker if permission or network access is unavailable."
    } else {
        "Network access is not authorized; do not fetch documentation, dependencies or other remote resources."
    };
    let commands = if arguments.allow_commands {
        "Project-local dependencies, setup, builds and tests are authorized for this task. Preserve lockfiles."
    } else {
        "Read/edit only; commands are not authorized."
    };
    format!(
        "You are implementing the owner's task through JARVIS. Your working directory is {}. \
         {workspace} \
         Existing uncommitted files belong to the owner; preserve unrelated changes. Do not initialize Git, create a branch, \
         commit, push, deploy, or install globally unless the task explicitly asks. {commands} {network} \
         Complete the actual implementation, not just a scaffold. For websites choose a coherent design, responsive layout, \
         useful content and working interactions; when authorized, install dependencies and fix build/runtime errors. Inspect desktop/mobile \
         behavior when browser access exists. Report any missing capability honestly. External accounts and JARVIS integrations \
         are not automatically available. Do not invent credentials or evidence. Do not leave hidden detached dev servers; \
         report preview argv and bind/port requirements so JARVIS can start a managed process. End with changed paths, commands/checks and their real results, preview requirements, \
         and remaining blockers. A self-reported success is not independent verification. \
         Use the existing brand and design requirements from the brief; skills do not override them. \
         Save verification artifacts using absolute paths inside this working directory, never a browser server's cwd. \
         Every advertised verification script must exist and run. A zero build exit does not prove lint, type checks or browser checks ran; \
         disclose skipped checks and inspect stderr. Repair missing tooling instead of suppressing checks.\n\nProject skills:\n{skills}\n\nTask:\n{}",
        directory.display(),
        arguments.task
    )
}

fn describe_in_place(program: &Path, directory: &Path, report: &Report) -> Value {
    let (outcome, detail) = ended_words(&report.ended);
    let mut message = report.message.clone();
    if let Ended::Failed(reason) = &report.ended {
        let _ = write!(message, "\n[agent error] {reason}");
    }
    let activity = report
        .tools
        .iter()
        .take(40)
        .map(|tool| format!("[{}] {} ({})", tool.kind, tool.title, tool.status))
        .collect::<Vec<_>>()
        .join("\n");
    json!({"outcome":outcome,"detail":detail,"directory":directory.to_string_lossy(),
        "agent_program":program.file_name().map(|name|name.to_string_lossy().into_owned()),
        "agent_message":fenced_pieces(&message),"agent_tool_calls":fenced_pieces(&activity),
        "permissions":{"allowed":report.permissions.iter().filter(|record|record.allowed).count(),
            "refused":report.permissions.iter().filter(|record|!record.allowed).count()},
        "activity_updates":report.activity_updates,"activity_dropped":report.activity_dropped,
        "tool_notes_dropped":report.tools_dropped,"stray_output_lines":report.stray_lines})
}

impl CodeDelegateTool {
    pub(crate) fn resolve(&self, directory: &str) -> Result<PathBuf, AdapterError> {
        self.folders
            .resolve_directory(directory)
            .map_err(|reason| refused(&reason))
    }

    pub(crate) async fn acquire<'a>(
        &'a self,
        cancel: &Notify,
    ) -> Result<Option<tokio::sync::SemaphorePermit<'a>>, AdapterError> {
        tokio::select! {
            biased;
            () = cancel.notified() => Ok(None),
            permit = self.slot.acquire() => permit.map(Some).map_err(|_| refused("the coding queue is closed")),
        }
    }

    async fn verify(
        &self,
        directory: &Path,
        arguments: &Arguments,
        cancel: &Notify,
    ) -> Result<Value, AdapterError> {
        let runner = CommandRunTool::new(&[directory.to_path_buf()])
            .map_err(|_| refused("the verification folder is unavailable"))?;
        let selected = if arguments.checks.is_empty() && arguments.allow_commands {
            project_checks(directory)
        } else {
            arguments.checks.clone()
        };
        let mut checks = Vec::new();
        let missing = crate::code_verification::missing_check_scripts(directory);
        if !missing.is_empty() {
            return Ok(json!({"status":"failed","checks":[],"issues":missing}));
        }
        for check in &selected {
            let parsed =
                crate::command_run::CommandArgs::parse(check).map_err(|reason| refused(&reason))?;
            let result = tokio::select! {
                biased;
                () = cancel.notified() => return Ok(json!({"status":"cancelled", "checks":checks})),
                result = runner.run(&parsed, UtcTimestamp::now(&SystemClock)) => result,
            };
            let body = match result {
                Ok(result) => result
                    .output()
                    .and_then(|output| serde_json::from_str::<Value>(output.content()).ok())
                    .unwrap_or_else(|| json!({"error":"verification output could not be decoded"})),
                Err(error) => json!({"error":error.to_string()}),
            };
            let success = body["exit_code"] == 0
                && body["outcome"] == "completed"
                && body["reported_check_failures"]
                    .as_array()
                    .is_none_or(Vec::is_empty)
                && body["capture_complete"] == true;
            checks.push(json!({"command":check,"passed":success,"result":body}));
            if !success {
                return Ok(json!({"status":"failed","checks":checks}));
            }
        }
        Ok(
            json!({"status":if checks.is_empty(){"not_run"}else{"passed"},
            "scope":"Only the listed commands passed. Unlisted lint, type, browser and behavioral checks remain not_run.",
            "checks":checks}),
        )
    }
}

/// Splits untrusted text into fenced pieces that each fit a fence.
fn fenced_pieces(text: &str) -> Vec<String> {
    let letters: Vec<char> = text.chars().collect();
    let mut pieces = Vec::new();
    for part in letters.chunks(FENCE_PIECE_CHARS) {
        let mut length = part.len();
        let mut start = 0;
        while start < part.len() {
            let slice: String = part[start..(start + length).min(part.len())]
                .iter()
                .collect();
            match IsolatedText::new(&slice) {
                Ok(isolated) => {
                    pieces.push(isolated.render());
                    start += length;
                    length = part.len();
                }
                Err(_) if length > 200 => length = length * 3 / 4,
                Err(_) => start += length,
            }
        }
    }
    pieces
}

fn ended_words(ended: &Ended) -> (&'static str, Option<String>) {
    match ended {
        Ended::Finished(StopReason::EndTurn) => ("completed", None),
        Ended::Finished(reason) => ("stopped", Some(reason.as_str().to_owned())),
        Ended::Cancelled => ("cancelled", None),
        Ended::TimedOut => ("timed_out", None),
        Ended::AgentExited(code) => ("agent_exited", code.map(|code| format!("exit code {code}"))),
        Ended::Failed(_) => ("failed", None),
    }
}

/// The answer: what happened in this adapter's own words, and what the agent said, fenced.
fn describe(
    program: &Path,
    repository: &Repository,
    tree: &Worktree,
    report: &Report,
    outcome: Result<Outcome, worktree::WorktreeError>,
    original_changed: bool,
) -> Value {
    let (label, detail) = ended_words(&report.ended);
    let allowed = report
        .permissions
        .iter()
        .filter(|record| record.allowed)
        .count();
    let refused = report.permissions.len() - allowed;

    // Everything the agent wrote, in one fenced block of lines: its words, then the tools it used and what was asked of JARVIS.
    let mut agent_text = String::new();
    if report.message_cut {
        agent_text.push_str("(the start of the agent's message was cut)\n");
    }
    agent_text.push_str(report.message.trim());
    if let Ended::Failed(reason) = &report.ended {
        let _ = write!(agent_text, "\n[agent error] {reason}");
    }
    let mut activity = String::new();
    for note in report.tools.iter().take(40) {
        let _ = writeln!(
            activity,
            "- [{}] {} ({})",
            note.kind, note.title, note.status
        );
    }
    let mut asked = String::new();
    for record in report.permissions.iter().take(40) {
        let _ = writeln!(
            asked,
            "- [{}] {} -> {}",
            record.kind,
            record.title,
            if record.allowed { "allowed" } else { "refused" }
        );
    }

    let mut answer = json!({
        "outcome": label,
        "detail": detail,
        "agent_program": program.file_name().map(|name| name.to_string_lossy().into_owned()),
        "branch": tree.branch,
        "worktree": tree.path.to_string_lossy(),
        "base_commit": tree.base,
        "repository": repository.top().to_string_lossy(),
        "permissions": { "allowed": allowed, "refused": refused },
        "agent_message": fenced_pieces(&agent_text),
        "agent_tool_calls": fenced_pieces(&activity),
        "agent_requests": fenced_pieces(&asked),
        "stray_output_lines": report.stray_lines,
        "activity_updates": report.activity_updates,
        "activity_dropped": report.activity_dropped,
        "tool_notes_dropped": report.tools_dropped,
    });
    let object = answer.as_object_mut();
    let Some(object) = object else { return answer };
    match outcome {
        Ok(result) => {
            let patch: String = result.patch.chars().take(PATCH_SHOWN_CHARS).collect();
            object.insert("commit".to_owned(), json!(result.commit));
            object.insert("files_changed".to_owned(), json!(result.files_changed));
            object.insert("stat".to_owned(), json!(fenced_pieces(&result.stat)));
            object.insert("patch".to_owned(), json!(fenced_pieces(&patch)));
            object.insert(
                "patch_was_cut".to_owned(),
                json!(result.patch_cut || result.patch.chars().count() > PATCH_SHOWN_CHARS),
            );
        }
        Err(error) => {
            object.insert(
                "result_unreadable".to_owned(),
                json!(format!("the worktree could not be read afterwards: {error}; the agent's work, if any, is still in the folder above")),
            );
        }
    }
    let mut notes: Vec<&str> = Vec::new();
    if original_changed {
        notes.push("the owner's own checkout changed while the agent ran: the agent wrote outside its worktree, so check that repository before trusting anything");
    }
    if repository.has_uncommitted_changes() {
        notes.push("the repository had uncommitted changes, which are not in the worktree (it starts from the last commit)");
    }
    if report.ended == Ended::TimedOut {
        notes.push("the time limit passed; what the agent had done by then is on the branch");
    }
    object.insert("notes".to_owned(), json!(notes));
    object.insert(
        "next".to_owned(),
        json!(format!(
            "Nothing was merged or pushed. Review with `git diff {}..{}` in the repository; merge the branch yourself, or ask for changes. The worktree stays until removed with `git worktree remove`.",
            tree.base, tree.branch
        )),
    );
    answer
}

impl std::fmt::Debug for CodeDelegateTool {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("CodeDelegateTool")
            .field("adapter_id", &self.adapter_id())
            .finish_non_exhaustive()
    }
}

#[async_trait]
impl ToolExecutor for CodeDelegateTool {
    fn adapter_id(&self) -> &'static str {
        "code-delegate"
    }

    async fn execute(
        &self,
        request: &ToolExecutionRequest,
    ) -> Result<ToolCallResult, AdapterError> {
        if request.tool().to_string() != DELEGATE_TOOL {
            return Err(AdapterError::NotImplemented {
                tool: request.tool().to_string(),
            });
        }
        let now = UtcTimestamp::now(&SystemClock);
        if request.is_past_deadline(now) {
            return Err(refused("the call was already past its deadline"));
        }
        let arguments = Arguments::parse(request.arguments()).map_err(|reason| refused(&reason))?;
        let body = self.run_with_cancel(&arguments, &Notify::new()).await?;
        let evidence = ProviderEvidence::new("code:delegate").ok();
        let record = evidence
            .as_ref()
            .and_then(|value| ToolOutcomeRecord::confirmed(value.as_str()).ok())
            .unwrap_or_else(|| {
                ToolOutcomeRecord::failed("evidence")
                    .unwrap_or_else(|_| unreachable!("a literal reason"))
            });
        Ok(ToolCallResult::new(
            record,
            evidence,
            Some(BoundedOutput::from_bounded(body.to_string(), false)),
            now,
        ))
    }
}

#[cfg(test)]
#[path = "code_delegate_tests.rs"]
mod tests;
