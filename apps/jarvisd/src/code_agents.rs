//! The coding agents JARVIS knows how to start, how they are chosen and tested from the console, and how one is started (`ADR-0208`).
//!
//! `jarvis.code.delegate` (`crate::code_delegate`) needs an agent that speaks the Agent Client Protocol. This module is everything about
//! *which* agent: a short list of presets read from the ACP registry (each started either as the program the owner installed or, if
//! the owner has Node, through a pinned `npx` package), the saved setting, resolving it to a program on this machine, starting it
//! with a clean environment, and the console's three calls:
//!
//! * `GET /api/v1/code-agents`: the presets, which can be started on this computer, which is selected, and what else a task needs;
//! * `POST /api/v1/code-agents/test`: starts the **saved** agent in an empty temporary folder, completes the handshake and says what
//!   it found, in a sentence a person can act on. No task is run and nothing in any repository is touched;
//! * `POST /api/v1/code-agents/sign-in`: the same, but with a sign-in method of the agent's own, because the agent asked for one.
//!   Signing in is the agent's own flow (it may open a browser page); on success the method is saved so a task can use it again if the
//!   agent asks. JARVIS stores no credential: the agent keeps its own.
//!
//! The settings are read at each call, so a change applies at once with no restart.

use std::collections::BTreeMap;
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use axum::{
    Json,
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use jarvis_runtimes::acp::{AcpError, AuthMethod, Connection, Limits};
use jarvis_storage::{AppPaths, ConfigStore, settings};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::command_run::{TreeGuard, child_environment, kill_tree, resolve_program};
use crate::gateway::GatewayState;
use crate::settings_service::{context, refused};

/// How long the Test may wait for an agent to start. Longer than a task's start, because the first run through `npx` downloads it.
const TEST_HANDSHAKE: Duration = Duration::from_secs(120);
/// How long signing in may take: a person completing a page in a browser.
const SIGN_IN_HANDSHAKE: Duration = Duration::from_secs(300);

/// The agents looked for when nothing is chosen, in order. Only agents that start in ACP mode on their own: an adapter that has to be
/// downloaded is never started without the owner choosing it.
const DETECTED: [(&str, &[&str]); 2] = [("copilot", &["--acp"]), ("gemini", &["--acp"])];

/// An agent JARVIS can start. Launch commands are the ones in the ACP registry (read 2026-10-13, `docs/research/integrations/acp.md`).
pub struct Preset {
    /// The identifier the console uses.
    pub id: &'static str,
    /// What the owner knows it as.
    pub name: &'static str,
    /// One line on what it is and whose account it uses.
    pub summary: &'static str,
    /// The program the owner installs and its arguments, when the agent ships one that speaks ACP.
    pub native: Option<(&'static str, &'static [&'static str])>,
    /// A package `npx` can run (pinned to the registry's version) and its arguments, for an agent that needs an adapter or that the
    /// owner has not installed.
    pub npx: Option<(&'static str, &'static [&'static str])>,
    /// What the owner does once so the agent can work, in a sentence.
    pub sign_in: &'static str,
    /// Names of environment variables that sign the agent in instead (the owner lists them in `code_agent_env`).
    pub env: &'static [&'static str],
}

/// The presets, in the order the console shows them.
pub const PRESETS: [Preset; 10] = [
    Preset {
        id: "codex",
        name: "ChatGPT (OpenAI Codex)",
        summary: "OpenAI's Codex, signed in with your ChatGPT account or an OpenAI key, through the official ACP adapter.",
        native: Some(("codex-acp", &[])),
        npx: Some(("@agentclientprotocol/codex-acp@2.2.2", &[])),
        sign_in: "Press Sign in below and choose ChatGPT (it opens a page in your browser), or list OPENAI_API_KEY under environment names.",
        env: &["OPENAI_API_KEY"],
    },
    Preset {
        id: "claude",
        name: "Claude",
        summary: "Anthropic's Claude Agent, through the ACP adapter built on the Claude Agent SDK.",
        native: Some(("claude-agent-acp", &[])),
        npx: Some(("@agentclientprotocol/claude-agent-acp@0.90.0", &[])),
        sign_in: "Sign in to Claude once (run `claude` in a terminal), or list ANTHROPIC_API_KEY under environment names.",
        env: &["ANTHROPIC_API_KEY"],
    },
    Preset {
        id: "copilot",
        name: "GitHub Copilot",
        summary: "GitHub Copilot's command line agent (in public preview for ACP).",
        native: Some(("copilot", &["--acp"])),
        npx: Some(("@github/copilot@1.0.96", &["--acp"])),
        sign_in: "Run `copilot` once in a terminal and sign in with GitHub.",
        env: &["GH_TOKEN"],
    },
    Preset {
        id: "gemini",
        name: "Google Gemini",
        summary: "Google's Gemini CLI.",
        native: Some(("gemini", &["--acp"])),
        npx: Some(("@google/gemini-cli@0.63.0", &["--acp"])),
        sign_in: "Run `gemini` once in a terminal and sign in with Google, or list GEMINI_API_KEY under environment names.",
        env: &["GEMINI_API_KEY"],
    },
    Preset {
        id: "opencode",
        name: "OpenCode",
        summary: "The open source OpenCode agent, with the model provider you set up in it.",
        native: Some(("opencode", &["acp"])),
        npx: None,
        sign_in: "Set up a provider in OpenCode once (run `opencode` in a terminal).",
        env: &[],
    },
    Preset {
        id: "goose",
        name: "goose",
        summary: "Block's goose agent, with the provider you set up in it.",
        native: Some(("goose", &["acp"])),
        npx: None,
        sign_in: "Run `goose configure` once in a terminal.",
        env: &[],
    },
    Preset {
        id: "cursor",
        name: "Cursor",
        summary: "Cursor's command line agent.",
        native: Some(("cursor-agent", &["acp"])),
        npx: None,
        sign_in: "Run `cursor-agent login` once in a terminal.",
        env: &["CURSOR_API_KEY"],
    },
    Preset {
        id: "qwen",
        name: "Qwen Code",
        summary: "Alibaba's Qwen Code.",
        native: Some(("qwen", &["--acp"])),
        npx: Some(("@qwen-code/qwen-code@0.25.1", &["--acp"])),
        sign_in: "Run `qwen` once in a terminal and sign in.",
        env: &[],
    },
    Preset {
        id: "kimi",
        name: "Kimi",
        summary: "Moonshot's Kimi CLI.",
        native: Some(("kimi", &["acp"])),
        npx: None,
        sign_in: "Run `kimi` once in a terminal and sign in.",
        env: &[],
    },
    Preset {
        id: "cline",
        name: "Cline",
        summary: "The Cline agent, with the provider you set up in it.",
        native: Some(("cline", &["--acp"])),
        npx: Some(("cline@3.0.71", &["--acp"])),
        sign_in: "Set up a provider in Cline once (run `cline` in a terminal).",
        env: &[],
    },
];

/// What the owner saved about the coding agent.
#[derive(Clone, Debug, Default)]
pub struct AgentSettings {
    /// The command: a program and its arguments, or empty to look for an installed agent.
    pub command: Vec<String>,
    /// Names of the environment variables the agent may inherit, beyond the safe set.
    pub env_names: Vec<String>,
    /// The sign-in method to use if the agent asks for one.
    pub sign_in: Option<String>,
}

impl AgentSettings {
    /// Reads the saved settings.
    ///
    /// # Errors
    ///
    /// Returns a sentence when the saved configuration cannot be read.
    pub fn load(paths: &AppPaths) -> Result<Self, String> {
        let loaded = ConfigStore::from_paths(paths)
            .load()
            .map_err(|_| "the saved configuration could not be read".to_owned())?;
        let daemon = loaded.config().daemon();
        Ok(Self {
            command: daemon.code_agent().to_vec(),
            env_names: daemon.code_agent_env().to_vec(),
            sign_in: daemon.code_agent_auth().map(str::to_owned),
        })
    }
}

/// The command a preset would be started with on this computer: the program the owner installed if it is on the search path, else the
/// pinned `npx` package if `npx` is. `None` when neither can be started.
fn launch(
    preset: &Preset,
    search_path: &OsStr,
    pathext: Option<&OsStr>,
) -> Option<(Vec<String>, &'static str)> {
    if let Some((program, arguments)) = preset.native
        && resolve_program(program, search_path, pathext).is_some()
    {
        let mut command = vec![program.to_owned()];
        command.extend(arguments.iter().map(|argument| (*argument).to_owned()));
        return Some((command, "installed"));
    }
    if let Some((package, arguments)) = preset.npx
        && resolve_program("npx", search_path, pathext).is_some()
    {
        let mut command = vec!["npx".to_owned(), "-y".to_owned(), package.to_owned()];
        command.extend(arguments.iter().map(|argument| (*argument).to_owned()));
        return Some((command, "npx"));
    }
    None
}

/// Every command line that means this preset, so a saved command can be recognised.
fn spellings(preset: &Preset) -> Vec<Vec<String>> {
    let mut all = Vec::new();
    if let Some((program, arguments)) = preset.native {
        let mut command = vec![program.to_owned()];
        command.extend(arguments.iter().map(|argument| (*argument).to_owned()));
        all.push(command);
    }
    if let Some((package, arguments)) = preset.npx {
        let mut command = vec!["npx".to_owned(), "-y".to_owned(), package.to_owned()];
        command.extend(arguments.iter().map(|argument| (*argument).to_owned()));
        all.push(command);
    }
    all
}

/// The preset a saved command belongs to, if any.
#[must_use]
pub fn preset_of(command: &[String]) -> Option<&'static str> {
    PRESETS
        .iter()
        .find(|preset| spellings(preset).iter().any(|spelling| spelling == command))
        .map(|preset| preset.id)
}

/// The program and arguments to start: the saved command, or the first installed agent that starts in ACP mode by itself.
///
/// # Errors
///
/// Returns a sentence saying what is missing and how to set an agent up.
pub fn resolve(
    command: &[String],
    search_path: &OsStr,
    pathext: Option<&OsStr>,
) -> Result<(PathBuf, Vec<String>), String> {
    if let Some((program, arguments)) = command.split_first() {
        let path = if Path::new(program).is_absolute() {
            Some(PathBuf::from(program))
        } else {
            resolve_program(program, search_path, pathext)
        };
        return path
            .map(|path| (path, arguments.to_vec()))
            .ok_or_else(|| {
                format!(
                    "the coding agent `{program}` was not found on the search path: install it, or choose another under Settings, Folders & code, Coding agent"
                )
            });
    }
    for (name, arguments) in DETECTED {
        if let Some(path) = resolve_program(name, search_path, pathext) {
            return Ok((
                path,
                arguments
                    .iter()
                    .map(|argument| (*argument).to_owned())
                    .collect(),
            ));
        }
    }
    Err("no coding agent is set up. Choose one under Settings, Folders & code, Coding agent (or install GitHub Copilot CLI or Gemini CLI and sign in to it once)"
        .to_owned())
}

/// The environment an agent starts with: the safe set a build gets, plus the variables the owner named.
#[must_use]
pub fn environment(env_names: &[String]) -> BTreeMap<String, OsString> {
    let mut environment = child_environment(std::env::vars_os());
    for name in env_names {
        if let Some(value) = std::env::var_os(name) {
            environment.insert(name.clone(), value);
        }
    }
    environment
}

/// Starts the agent in `folder` with a clean environment, with piped standard streams.
///
/// # Errors
///
/// Returns a sentence when the program cannot be started.
pub fn spawn(
    program: &Path,
    arguments: &[String],
    folder: &Path,
    environment: &BTreeMap<String, OsString>,
) -> Result<tokio::process::Child, String> {
    let mut command = tokio::process::Command::new(program);
    command
        .args(arguments)
        .current_dir(folder)
        .env_clear()
        .envs(environment)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    #[cfg(unix)]
    command.process_group(0);
    #[cfg(windows)]
    command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    command
        .spawn()
        .map_err(|error| format!("the coding agent could not be started: {}", error.kind()))
}

/// Why an agent could not be started, in words for the owner. The agent's own words are bounded and come last.
#[must_use]
pub fn describe_error(error: &AcpError) -> String {
    match error {
        AcpError::UnsupportedVersion { .. } => {
            format!("{error}. This agent needs a newer or older ACP adapter; nothing was changed")
        }
        AcpError::AuthRequired { methods, .. } if methods.is_empty() => {
            format!("{error}. Run the agent once in a terminal and sign in; nothing was changed")
        }
        AcpError::AuthRequired { methods, .. } => format!(
            "{error}. It offers: {}. Use Sign in under Settings, Folders & code, Coding agent; nothing was changed",
            methods
                .iter()
                .map(|method| method.name.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        ),
        AcpError::Handshake { stderr_tail, .. } if !stderr_tail.is_empty() => {
            let tail: String = stderr_tail
                .chars()
                .rev()
                .take(300)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect();
            format!("{error}; it wrote: {}", tail.replace('\n', " | "))
        }
        AcpError::Handshake { .. } => format!(
            "{error}. If it was started through npx for the first time it may still have been downloading: run the same command once in a terminal"
        ),
        _ => error.to_string(),
    }
}

/// What a probe of the saved agent found.
struct Probe {
    name: String,
    version: String,
    methods: Vec<AuthMethod>,
    signed_in: bool,
}

/// Why a probe did not complete.
struct ProbeFailure {
    message: String,
    methods: Vec<AuthMethod>,
    sign_in_needed: bool,
}

impl ProbeFailure {
    fn plain(message: String) -> Self {
        Self {
            message,
            methods: Vec::new(),
            sign_in_needed: false,
        }
    }
}

/// Starts the saved agent in an empty temporary folder, completes the handshake and closes it. Nothing is asked of the agent but to
/// start a session.
async fn probe(
    settings: &AgentSettings,
    sign_in: Option<&str>,
    handshake: Duration,
) -> Result<Probe, ProbeFailure> {
    probe_under(&std::env::temp_dir(), settings, sign_in, handshake).await
}

/// [`probe`], with the temporary folder made (and removed) under `root`.
async fn probe_under(
    root: &Path,
    settings: &AgentSettings,
    sign_in: Option<&str>,
    handshake: Duration,
) -> Result<Probe, ProbeFailure> {
    let environment = environment(&settings.env_names);
    let search_path = environment
        .get("PATH")
        .map(OsString::from)
        .unwrap_or_default();
    let pathext = environment.get("PATHEXT").cloned();
    let (program, arguments) = resolve(&settings.command, &search_path, pathext.as_deref())
        .map_err(ProbeFailure::plain)?;
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| since.as_nanos());
    let folder = root.join(format!("jarvis-agent-test-{nanos:x}"));
    std::fs::create_dir_all(&folder)
        .map_err(|_| ProbeFailure::plain("a temporary folder could not be made".to_owned()))?;
    let outcome = async {
        let child =
            spawn(&program, &arguments, &folder, &environment).map_err(ProbeFailure::plain)?;
        let pid = child.id().unwrap_or(0);
        let _guard = TreeGuard { pid, armed: true };
        let limits = Limits {
            handshake,
            ..Limits::default()
        };
        let result = Connection::open(child, &folder, limits, sign_in).await;
        match result {
            Ok(mut connection) => {
                // Asked to sign in: do it now even if the agent has not asked (some only check at the first prompt).
                let signed = match sign_in {
                    Some(method) if !connection.signed_in() => {
                        if let Err(error) = connection.authenticate(method).await {
                            let methods = connection.auth_methods().to_vec();
                            kill_tree(pid);
                            return Err(ProbeFailure {
                                message: describe_error(&error),
                                methods,
                                sign_in_needed: false,
                            });
                        }
                        true
                    }
                    _ => connection.signed_in(),
                };
                let probe = Probe {
                    name: connection.agent().name.clone(),
                    version: connection.agent().version.clone(),
                    methods: connection.auth_methods().to_vec(),
                    signed_in: signed,
                };
                connection.close().await;
                kill_tree(pid);
                Ok(probe)
            }
            Err(error) => {
                kill_tree(pid);
                let methods = match &error {
                    AcpError::AuthRequired { methods, .. }
                    | AcpError::UnknownAuthMethod { methods, .. } => methods.clone(),
                    _ => Vec::new(),
                };
                Err(ProbeFailure {
                    message: describe_error(&error),
                    sign_in_needed: matches!(error, AcpError::AuthRequired { .. }),
                    methods,
                })
            }
        }
    }
    .await;
    let _ = std::fs::remove_dir_all(&folder);
    outcome
}

fn methods_json(methods: &[AuthMethod]) -> Vec<Value> {
    methods
        .iter()
        .map(|method| {
            json!({ "id": method.id, "name": method.name, "description": method.description })
        })
        .collect()
}

/// The routes of this module, under `/api/v1`.
pub fn routes() -> axum::Router<GatewayState> {
    axum::Router::new()
        .route("/code-agents", axum::routing::get(status))
        .route("/code-agents/test", axum::routing::post(test))
        .route("/code-agents/sign-in", axum::routing::post(sign_in))
        .route("/code-agents/tasks", axum::routing::get(tasks))
}

/// `GET /api/v1/code-agents`
pub async fn status(State(state): State<GatewayState>) -> Response {
    let context = match context(&state) {
        Ok(context) => context,
        Err(response) => return response,
    };
    let paths = context.paths();
    let Ok(saved) = AgentSettings::load(paths) else {
        return refused("the saved configuration could not be read");
    };
    let environment = environment(&[]);
    let search_path = environment
        .get("PATH")
        .map(OsString::from)
        .unwrap_or_default();
    let pathext = environment.get("PATHEXT").cloned();
    let selected = preset_of(&saved.command);
    let presets: Vec<Value> = PRESETS
        .iter()
        .map(|preset| {
            let launched = launch(preset, &search_path, pathext.as_deref());
            json!({
                "id": preset.id,
                "name": preset.name,
                "summary": preset.summary,
                "sign_in": preset.sign_in,
                "env": preset.env,
                "available": launched.is_some(),
                "via": launched.as_ref().map_or("missing", |(_, via)| *via),
                "command": launched.map(|(command, _)| command),
                "selected": selected == Some(preset.id),
            })
        })
        .collect();
    let folders = ConfigStore::from_paths(paths)
        .load()
        .is_ok_and(|loaded| !loaded.config().daemon().tool_workspace_roots().is_empty());
    (
        StatusCode::OK,
        Json(json!({
            "presets": presets,
            "selected": selected,
            "custom": selected.is_none() && !saved.command.is_empty(),
            "command": saved.command,
            "env": saved.env_names,
            "sign_in": saved.sign_in,
            "detected": DETECTED
                .iter()
                .find(|(name, _)| resolve_program(name, &search_path, pathext.as_deref()).is_some())
                .map(|(name, _)| *name),
            "needs": {
                "folder": folders,
                "git": resolve_program("git", &search_path, pathext.as_deref()).is_some(),
                "node": resolve_program("npx", &search_path, pathext.as_deref()).is_some(),
            },
        })),
    )
        .into_response()
}

/// `GET /api/v1/code-agents/tasks`: recent durable background tasks for the Coding agent card.
pub async fn tasks(State(state): State<GatewayState>) -> Response {
    let Ok(identity) = jarvis_storage::load_local_identity(&state.database_handle()).await else {
        return refused("the local workspace could not be read");
    };
    let Ok(tasks) =
        jarvis_storage::list_delegated_tasks(&state.database_handle(), identity.workspace_id(), 20)
            .await
    else {
        return refused("coding tasks could not be read");
    };
    let shown: Vec<Value> = tasks
        .into_iter()
        .map(|task| crate::code_delegate_async::render_console(&task))
        .collect();
    (StatusCode::OK, Json(json!({ "tasks": shown }))).into_response()
}

/// One check or sign-in at a time: each starts a process, and the second would only fight the first for the agent's own files.
static PROBING: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

fn probe_reply(result: Result<Probe, ProbeFailure>, saved_method: Option<&str>) -> Response {
    let body = match result {
        Ok(found) => {
            let who = if found.name.is_empty() {
                "The agent".to_owned()
            } else {
                format!("{} {}", found.name, found.version)
                    .trim()
                    .to_owned()
            };
            json!({
                "ok": true,
                "message": if found.signed_in {
                    format!("{who} is signed in and ready. JARVIS will use this method again if it asks.")
                } else if found.methods.is_empty() {
                    format!("{who} started and answered. It is ready for a task.")
                } else {
                    format!(
                        "{who} started and answered. It offers a sign-in ({}), and cannot say whether you are signed in until a task starts: use Sign in below if you have not signed in yet.",
                        found.methods.iter().map(|method| method.name.as_str()).collect::<Vec<_>>().join(", ")
                    )
                },
                "agent": { "name": found.name, "version": found.version },
                "methods": methods_json(&found.methods),
                "signed_in": found.signed_in,
                "saved_method": saved_method,
            })
        }
        Err(failure) => json!({
            "ok": false,
            "message": failure.message,
            "sign_in_needed": failure.sign_in_needed,
            "methods": methods_json(&failure.methods),
        }),
    };
    (StatusCode::OK, Json(body)).into_response()
}

/// `POST /api/v1/code-agents/test`
pub async fn test(State(state): State<GatewayState>) -> Response {
    let context = match context(&state) {
        Ok(context) => context,
        Err(response) => return response,
    };
    let Ok(_one_at_a_time) = PROBING.try_lock() else {
        return refused("another check of the coding agent is still running");
    };
    let Ok(saved) = AgentSettings::load(context.paths()) else {
        return refused("the saved configuration could not be read");
    };
    probe_reply(probe(&saved, None, TEST_HANDSHAKE).await, None)
}

/// The body of a sign-in request.
#[derive(Deserialize)]
pub struct SignInBody {
    method_id: String,
}

/// `POST /api/v1/code-agents/sign-in`
pub async fn sign_in(State(state): State<GatewayState>, Json(body): Json<SignInBody>) -> Response {
    let context = match context(&state) {
        Ok(context) => context,
        Err(response) => return response,
    };
    let method = body.method_id.trim().to_owned();
    if method.is_empty() || method.len() > 80 || method.chars().any(char::is_control) {
        return refused("that is not a sign-in method");
    }
    let Ok(_one_at_a_time) = PROBING.try_lock() else {
        return refused("another check of the coding agent is still running");
    };
    let Ok(saved) = AgentSettings::load(context.paths()) else {
        return refused("the saved configuration could not be read");
    };
    let result = probe(&saved, Some(&method), SIGN_IN_HANDSHAKE).await;
    // The method is kept only when it worked, so a typo never becomes the setting.
    let kept = match &result {
        Ok(found) if found.signed_in => settings::set(
            context.paths(),
            "code_agent_auth",
            std::slice::from_ref(&method),
        )
        .ok()
        .map(|()| method.as_str()),
        _ => None,
    };
    probe_reply(result, kept)
}

#[cfg(test)]
#[path = "code_agents_tests.rs"]
mod tests;
