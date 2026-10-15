//! Managing the MCP servers from the console and the CLI (`P11-001`, `ADR-0168`).
//!
//! Before this, a server existed only if the owner hand-wrote `mcp-servers.toml`, and the daemon logged a count: "reachable 0,
//! tools 0" could mean a server that was down, one that offered nothing, or thirteen tools refused for a reason nobody saw.
//! This module is the one place that reads and edits that file, and it reports what actually happened to each server.
//!
//! **Every edit is validated by the parser the daemon starts with** ([`McpHostConfig::parse`]), so a document that is accepted
//! here cannot be refused at startup, and a document that the daemon would refuse is never written. A change is applied on
//! the next start (the page says so); the live host is not rebuilt, because a half-applied tool set is worse than a restart.
//!
//! **Adding a server is running a program as the owner.** The routes sit behind the local credential like every settings
//! route, a model has no tool that reaches them, and a new server's tools are held for approval until the owner says
//! otherwise (`ToolEffectPolicy::unclassified`). `read_only` is the owner's own statement and is never inferred from what the
//! server claims about itself.

use std::path::{Path, PathBuf};
use std::time::Duration;

use axum::{Json, extract::Path as UrlPath, extract::State, http::StatusCode, response::Response};
use jarvis_core::ErrorCode;
use jarvis_mcp_transport::McpHostConfig;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::gateway::{GatewayState, error_response};
use crate::mcp_host::MCP_SERVERS_FILE_NAME;
use crate::settings_service::{context, refused};

/// How long a test connection may take before it is abandoned.
///
/// Longer than the handshake deadline in the transport, because a first run of `npx` may download the package.
const TEST_DEADLINE: Duration = Duration::from_secs(60);

/// Most characters of a tool description shown in the list.
const DESCRIPTION_CHARS: usize = 160;

/// Serialises edits of the document, so two requests cannot read the same file and overwrite each other.
static EDIT_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// What one configured server turned out to be, as of the daemon's start.
#[derive(Clone, Debug, Serialize)]
pub struct ServerStatus {
    /// The owner's name for the server.
    pub name: String,
    /// `connected`, or `unreachable` with a reason.
    pub state: &'static str,
    /// Why it could not be read, when it could not.
    pub reason: Option<String>,
    /// `modern` or `legacy`, when connected.
    pub era: Option<&'static str>,
    /// The protocol revision it settled on, when connected.
    pub revision: Option<String>,
    /// The name the server gave itself. Evidence, never an identifier.
    pub reported_name: Option<String>,
    /// The tools offered to the model.
    pub tools: Vec<ToolStatus>,
    /// Tools the server listed that were left out, each with the reason.
    pub excluded: Vec<Excluded>,
}

/// One tool the model is offered.
#[derive(Clone, Debug, Serialize)]
pub struct ToolStatus {
    /// The identifier the model and the permission list use.
    pub id: String,
    /// The first line of its description, cut.
    pub description: String,
}

/// A tool that was left out.
#[derive(Clone, Debug, Serialize)]
pub struct Excluded {
    /// The server's own name for the tool.
    pub tool: String,
    /// Why it was left out.
    pub reason: String,
}

/// What the daemon found when it started: the document it read and what became of each server in it.
#[derive(Clone, Debug, Default)]
pub struct McpSnapshot {
    /// The document as the daemon read it, to tell whether the file has changed since.
    pub loaded: Option<String>,
    /// One entry per server that connected or failed to.
    pub servers: Vec<ServerStatus>,
}

impl McpSnapshot {
    /// Builds the snapshot from the composed host, when there is one.
    #[must_use]
    pub fn capture(host: Option<&crate::mcp_host::ComposedHost>, loaded: Option<String>) -> Self {
        let Some(host) = host else {
            return Self {
                loaded,
                servers: Vec::new(),
            };
        };
        let mut servers = host.statuses();
        for unreadable in host.unreadable() {
            servers.push(ServerStatus {
                name: unreadable.server.as_str().to_owned(),
                state: "unreachable",
                reason: Some(unreadable.reason.clone()),
                era: None,
                revision: None,
                reported_name: None,
                tools: Vec::new(),
                excluded: Vec::new(),
            });
        }
        Self { loaded, servers }
    }
}

/// Cuts a description to its first line and a bounded length.
pub(crate) fn short_description(text: &str) -> String {
    let line = text.lines().next().unwrap_or_default().trim();
    let mut shortened: String = line.chars().take(DESCRIPTION_CHARS).collect();
    if line.chars().count() > DESCRIPTION_CHARS {
        shortened.push('…');
    }
    shortened
}

// ---------------------------------------------------------------------------------------------------------------------
// The document
// ---------------------------------------------------------------------------------------------------------------------

/// One server as the owner wrote it.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Configured {
    /// The owner's name for it.
    pub name: String,
    /// `stdio` or `http`.
    pub kind: String,
    /// The program and its arguments, for `stdio`.
    pub command: Vec<String>,
    /// The URL, for `http`.
    pub endpoint: Option<String>,
    /// `read-only` or `unclassified`.
    pub posture: String,
    /// Credential variable to named-reference mappings. No values or key file paths.
    pub env: std::collections::BTreeMap<String, String>,
    /// A supported account binding, never an access token.
    pub authorization: Option<String>,
}

/// What a request asks to add or test.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ServerBody {
    /// The owner's name for the server. Not used by a test.
    #[serde(default)]
    pub name: String,
    /// The program and its arguments, for a local server.
    #[serde(default)]
    pub command: Vec<String>,
    /// The URL, for a remote one.
    #[serde(default)]
    pub endpoint: Option<String>,
    /// The owner's statement that this server only reads, so its tools may run without asking.
    #[serde(default)]
    pub read_only: bool,
    /// Tools of this server that only read, by the server's own names, while the rest keep asking.
    #[serde(default)]
    pub read_only_tools: Vec<String>,
    /// Credential variables mapped only to this server's named references.
    #[serde(default)]
    pub env: std::collections::BTreeMap<String, String>,
    /// The explicitly selected OAuth account for a supported remote server.
    #[serde(default)]
    pub authorization: Option<String>,
}

/// Finds a program the way the operating system would, and returns the absolute path.
///
/// A bare name is looked up in `directories` (with each of `extensions` on Windows, where `npx` is `npx.cmd`). The result is
/// absolute, which matters twice: Windows cannot start a `.cmd` by a bare name from here (found 2026-10-10 with `npx`), and a
/// later change to `PATH` cannot swap the program the owner approved for another.
///
/// # Errors
///
/// Returns a sentence saying what is missing when nothing matches.
pub fn find_program(
    program: &str,
    directories: &[PathBuf],
    extensions: &[String],
) -> Result<String, String> {
    let given = Path::new(program);
    if given.components().count() > 1 || given.is_absolute() {
        return if given.is_file() {
            Ok(given.to_string_lossy().into_owned())
        } else {
            Err(format!("there is no program at {program}"))
        };
    }
    // Where executables have extensions (Windows), a file without one cannot be started: Node ships an
    // extension-less `npx` shell script beside `npx.cmd`, and picking it fails with "not a valid
    // application" (found 2026-10-10). So the bare name is tried only when it already ends in one.
    let lowered = program.to_lowercase();
    let has_extension = extensions
        .iter()
        .any(|extension| lowered.ends_with(&extension.to_lowercase()));
    for directory in directories {
        let bare =
            std::iter::once(String::new()).filter(|_| extensions.is_empty() || has_extension);
        let suffixes = bare.chain(extensions.iter().cloned());
        for extension in suffixes {
            let candidate = directory.join(format!("{program}{extension}"));
            if candidate.is_file() {
                return Ok(candidate.to_string_lossy().into_owned());
            }
        }
    }
    Err(format!(
        "`{program}` was not found on this computer's PATH; install it first (for `npx`, install Node.js from nodejs.org)"
    ))
}

/// [`find_program`] against the real environment.
///
/// # Errors
///
/// As [`find_program`].
pub fn find_program_on_path(program: &str) -> Result<String, String> {
    let directories: Vec<PathBuf> = std::env::var_os("PATH")
        .map(|path| std::env::split_paths(&path).collect())
        .unwrap_or_default();
    let extensions: Vec<String> = if cfg!(windows) {
        std::env::var("PATHEXT")
            .unwrap_or_else(|_| ".COM;.EXE;.BAT;.CMD".to_owned())
            .split(';')
            .filter(|extension| !extension.is_empty())
            .map(str::to_lowercase)
            .collect()
    } else {
        Vec::new()
    };
    find_program(program, &directories, &extensions)
}

/// Builds the table for one server, resolving the program to an absolute path.
fn entry_table(
    body: &ServerBody,
    resolve: &dyn Fn(&str) -> Result<String, String>,
) -> Result<toml::Table, String> {
    let mut transport = toml::Table::new();
    match (body.command.is_empty(), body.endpoint.as_deref()) {
        (false, None) => {
            let program = resolve(body.command[0].trim())?;
            transport.insert("kind".into(), "stdio".into());
            transport.insert("program".into(), program.into());
            transport.insert(
                "args".into(),
                toml::Value::Array(
                    body.command[1..]
                        .iter()
                        .map(|argument| toml::Value::String(argument.clone()))
                        .collect(),
                ),
            );
        }
        (true, Some(endpoint)) => {
            transport.insert("kind".into(), "http".into());
            transport.insert("endpoint".into(), endpoint.trim().into());
        }
        (true, None) => return Err("give a command to run, or an endpoint URL".to_owned()),
        (false, Some(_)) => return Err("give a command or an endpoint, not both".to_owned()),
    }
    if let Some(authorization) = &body.authorization {
        if !body.command.is_empty() {
            return Err("OAuth account binding requires a supported remote server".to_owned());
        }
        transport.insert("authorization".into(), authorization.clone().into());
    }
    let mut entry = toml::Table::new();
    entry.insert("name".into(), body.name.trim().into());
    entry.insert("transport".into(), toml::Value::Table(transport));
    if !body.env.is_empty() {
        entry.insert(
            "env".into(),
            toml::Value::Table(
                body.env
                    .iter()
                    .map(|(name, reference)| (name.clone(), toml::Value::String(reference.clone())))
                    .collect(),
            ),
        );
    }
    if body.read_only || !body.read_only_tools.is_empty() {
        let mut posture = toml::Table::new();
        if body.read_only {
            posture.insert("class".into(), "read-only".into());
        }
        if !body.read_only_tools.is_empty() {
            posture.insert(
                "read_only_tools".into(),
                toml::Value::Array(
                    body.read_only_tools
                        .iter()
                        .map(|tool| toml::Value::String(tool.clone()))
                        .collect(),
                ),
            );
        }
        entry.insert("posture".into(), toml::Value::Table(posture));
    }
    Ok(entry)
}

/// Parses the document as a generic table, refusing one that is not valid.
fn parse_table(document: Option<&str>) -> Result<toml::Table, String> {
    match document {
        None => Ok(toml::Table::new()),
        Some(text) if text.trim().is_empty() => Ok(toml::Table::new()),
        Some(text) => text.parse::<toml::Table>().map_err(|_| {
            format!(
                "{MCP_SERVERS_FILE_NAME} is not valid TOML, so it was left alone; fix or delete it first"
            )
        }),
    }
}

/// Renders a table and checks the result with the daemon's own parser.
fn render_checked(table: &toml::Table) -> Result<String, String> {
    let text =
        toml::to_string(table).map_err(|_| "the document could not be written".to_owned())?;
    McpHostConfig::parse(&text).map_err(|error| error.to_string())?;
    Ok(text)
}

/// Returns the document with one server added.
///
/// # Errors
///
/// Returns a sentence for a name already in use, an unusable command or endpoint, or a document the daemon would refuse.
pub fn add_to_document(
    document: Option<&str>,
    body: &ServerBody,
    resolve: &dyn Fn(&str) -> Result<String, String>,
) -> Result<String, String> {
    if body.name.trim().is_empty() {
        return Err("give the server a name".to_owned());
    }
    let mut table = parse_table(document)?;
    let entry = entry_table(body, resolve)?;
    let servers = table
        .entry("servers")
        .or_insert_with(|| toml::Value::Array(Vec::new()));
    let toml::Value::Array(list) = servers else {
        return Err(format!(
            "`servers` in {MCP_SERVERS_FILE_NAME} is not a list"
        ));
    };
    let taken = list.iter().any(|existing| {
        existing
            .get("name")
            .and_then(toml::Value::as_str)
            .is_some_and(|name| name == body.name.trim())
    });
    if taken {
        return Err(format!(
            "a server called {} already exists",
            body.name.trim()
        ));
    }
    list.push(toml::Value::Table(entry));
    render_checked(&table)
}

/// The owner-selected GitHub OAuth integration. Never replaces an unrelated server entry.
pub(crate) fn github_document(document: Option<&str>) -> Result<String, String> {
    if let Some(existing) = configured_servers(document)
        .into_iter()
        .find(|server| server.name == "github")
    {
        if existing.kind == "http"
            && existing.endpoint.as_deref() == Some(crate::github_service::MCP_ENDPOINT)
            && existing.authorization.as_deref() == Some("github")
            && existing.posture == "read-only"
        {
            return Ok(document.unwrap_or_default().to_owned());
        }
        return Err("A different program named github is already configured. Remove or rename it under Advanced, Programs before connecting GitHub here.".to_owned());
    }
    add_to_document(
        document,
        &ServerBody {
            name: "github".to_owned(),
            command: Vec::new(),
            endpoint: Some(crate::github_service::MCP_ENDPOINT.to_owned()),
            read_only: true,
            read_only_tools: Vec::new(),
            env: std::collections::BTreeMap::new(),
            authorization: Some("github".to_owned()),
        },
        &find_program_on_path,
    )
}

pub(crate) async fn configure_github(paths: &jarvis_storage::AppPaths) -> Result<(), String> {
    let _guard = EDIT_LOCK.lock().await;
    let current = read_document(paths.config())?;
    let updated = github_document(current.as_deref())?;
    write_document(paths.config(), &updated)
}

/// Returns the document with one server removed.
///
/// # Errors
///
/// Returns a sentence when there is no such server or the document cannot be read.
pub fn remove_from_document(document: Option<&str>, name: &str) -> Result<String, String> {
    let mut table = parse_table(document)?;
    let Some(toml::Value::Array(list)) = table.get_mut("servers") else {
        return Err(format!("there is no server called {name}"));
    };
    let before = list.len();
    list.retain(|existing| {
        existing
            .get("name")
            .and_then(toml::Value::as_str)
            .is_none_or(|existing_name| existing_name != name)
    });
    if list.len() == before {
        return Err(format!("there is no server called {name}"));
    }
    render_checked(&table)
}

/// Lists the servers a document configures.
#[must_use]
pub fn configured_servers(document: Option<&str>) -> Vec<Configured> {
    if McpHostConfig::parse(document.unwrap_or_default()).is_err() {
        return Vec::new();
    }
    let Ok(table) = parse_table(document) else {
        return Vec::new();
    };
    let Some(toml::Value::Array(list)) = table.get("servers") else {
        return Vec::new();
    };
    list.iter()
        .filter_map(|entry| {
            let name = entry.get("name")?.as_str()?.to_owned();
            let transport = entry.get("transport")?;
            let kind = transport.get("kind")?.as_str()?.to_owned();
            let mut command = Vec::new();
            if let Some(program) = transport.get("program").and_then(toml::Value::as_str) {
                command.push(program.to_owned());
            }
            if let Some(args) = transport.get("args").and_then(toml::Value::as_array) {
                command.extend(args.iter().filter_map(|a| a.as_str().map(str::to_owned)));
            }
            Some(Configured {
                authorization: transport
                    .get("authorization")
                    .and_then(toml::Value::as_str)
                    .map(str::to_owned),
                env: entry
                    .get("env")
                    .and_then(toml::Value::as_table)
                    .map(|env| {
                        env.iter()
                            .filter_map(|(name, reference)| {
                                Some((name.clone(), reference.as_str()?.to_owned()))
                            })
                            .collect()
                    })
                    .unwrap_or_default(),
                name,
                kind,
                command,
                endpoint: transport
                    .get("endpoint")
                    .and_then(toml::Value::as_str)
                    .map(str::to_owned),
                posture: entry
                    .get("posture")
                    .and_then(|posture| posture.get("class"))
                    .and_then(toml::Value::as_str)
                    .unwrap_or("unclassified")
                    .to_owned(),
            })
        })
        .collect()
}

fn read_document(directory: &Path) -> Result<Option<String>, String> {
    crate::mcp_host::load_document(directory).map_err(|error| error.to_string())
}

/// Writes the document through a temporary file, so a crash cannot leave half of it.
pub(crate) fn write_document(directory: &Path, text: &str) -> Result<(), String> {
    let target = directory.join(MCP_SERVERS_FILE_NAME);
    let temporary = directory.join(format!("{MCP_SERVERS_FILE_NAME}.tmp"));
    std::fs::write(&temporary, text)
        .map_err(|error| format!("the file could not be written: {error}"))?;
    std::fs::rename(&temporary, &target)
        .map_err(|error| format!("the file could not be replaced: {error}"))
}

/// Tells whether the file now differs from what the daemon started with.
fn restart_needed(loaded: Option<&str>, current: Option<&str>) -> bool {
    loaded.unwrap_or_default().trim() != current.unwrap_or_default().trim()
}

// ---------------------------------------------------------------------------------------------------------------------
// Routes
// ---------------------------------------------------------------------------------------------------------------------

fn overview_json(state: &GatewayState, current: Option<&str>) -> serde_json::Value {
    let snapshot = state.mcp();
    let settings = state.settings();
    let servers: Vec<_> = configured_servers(current)
        .into_iter()
        .map(|configured| {
            let status = snapshot
                .servers
                .iter()
                .find(|status| status.name == configured.name);
            json!({
                "name": configured.name,
                "kind": configured.kind,
                "command": configured.command,
                "endpoint": configured.endpoint,
                "posture": configured.posture,
                "env": configured.env,
                "authorization": configured.authorization,
                "credentials": settings.map(|settings| crate::mcp_credentials::states(settings.paths(), &configured.env)).unwrap_or_default(),
                // A server in the file the daemon did not start with has not been tried yet.
                "state": status.map_or("not_started", |status| status.state),
                "reason": status.and_then(|status| status.reason.clone()),
                "era": status.and_then(|status| status.era),
                "revision": status.and_then(|status| status.revision.clone()),
                "reported_name": status.and_then(|status| status.reported_name.clone()),
                "tools": status.map(|status| status.tools.clone()).unwrap_or_default(),
                "excluded": status.map(|status| status.excluded.clone()).unwrap_or_default(),
            })
        })
        .collect();
    json!({
        "servers": servers,
        "abilities": abilities_json(snapshot, current),
        "restart_needed": restart_needed(snapshot.loaded.as_deref(), current),
    })
}

/// Each ability with what is true of it now, in words a person can act on.
///
/// `state` is one of `off`, `preparing`, `restart` (on in the file, starts after a restart), `on`, or `problem`.
fn abilities_json(snapshot: &McpSnapshot, current: Option<&str>) -> Vec<serde_json::Value> {
    let configured = configured_servers(current);
    let program_ready = find_program_on_path("npx").is_ok();
    crate::mcp_abilities::ABILITIES
        .iter()
        .map(|ability| {
            let enabled = configured.iter().any(|server| server.name == ability.id);
            let status = snapshot
                .servers
                .iter()
                .find(|status| status.name == ability.id);
            let (state, detail) = if !enabled {
                ("off", None)
            } else if let Some(status) = status {
                if status.state == "connected" {
                    ("on", None)
                } else {
                    ("problem", status.reason.clone())
                }
            } else if crate::mcp_abilities::preparation(ability.id)
                == Some(crate::mcp_abilities::Preparation::Running)
            {
                ("preparing", None)
            } else {
                ("restart", None)
            };
            json!({
                "id": ability.id,
                "title": ability.title,
                "blurb": ability.blurb,
                "asks": ability.asks,
                "shares": ability.shares,
                "state": state,
                "detail": detail,
                "tools": status.map_or(0, |status| status.tools.len()),
                "needs": if program_ready { serde_json::Value::Null } else { json!("Node.js") },
                "preparation": crate::mcp_abilities::preparation(ability.id),
            })
        })
        .collect()
}

/// `POST /api/v1/mcp/abilities/{id}`: switch an ability on.
pub async fn enable_ability(
    State(state): State<GatewayState>,
    UrlPath(id): UrlPath<String>,
) -> Response {
    let context = match context(&state) {
        Ok(context) => context,
        Err(response) => return response,
    };
    let Some(ability) = crate::mcp_abilities::find(&id) else {
        return refused("there is no such ability");
    };
    let Ok(program) = find_program_on_path(ability.program) else {
        return refused(
            "this needs Node.js, which is free: install it from nodejs.org, then switch this on again",
        );
    };
    let directory = context.paths().config().to_path_buf();
    let _guard = EDIT_LOCK.lock().await;
    let current = match read_document(&directory) {
        Ok(document) => document,
        Err(message) => return refused(&message),
    };
    // Already on is not an error: pressing the button twice should leave it on.
    let updated = if configured_servers(current.as_deref())
        .iter()
        .any(|server| server.name == ability.id)
    {
        current.unwrap_or_default()
    } else {
        match add_to_document(
            current.as_deref(),
            &ability.body(context.paths().runtime()),
            &find_program_on_path,
        ) {
            Ok(text) => text,
            Err(message) => return refused(&message),
        }
    };
    if let Err(message) = write_document(&directory, &updated) {
        return error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            ErrorCode::Internal,
            &message,
        );
    }
    crate::mcp_abilities::prepare(ability, program, context.paths().runtime());
    json_ok(&overview_json(&state, Some(&updated)))
}

/// `DELETE /api/v1/mcp/abilities/{id}`: switch an ability off.
pub async fn disable_ability(
    State(state): State<GatewayState>,
    UrlPath(id): UrlPath<String>,
) -> Response {
    let context = match context(&state) {
        Ok(context) => context,
        Err(response) => return response,
    };
    if crate::mcp_abilities::find(&id).is_none() {
        return refused("there is no such ability");
    }
    let directory = context.paths().config().to_path_buf();
    let _guard = EDIT_LOCK.lock().await;
    let current = match read_document(&directory) {
        Ok(document) => document,
        Err(message) => return refused(&message),
    };
    let updated = match remove_from_document(current.as_deref(), &id) {
        Ok(text) => text,
        // Already off is not an error either.
        Err(_) => current.unwrap_or_default(),
    };
    if let Err(message) = write_document(&directory, &updated) {
        return error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            ErrorCode::Internal,
            &message,
        );
    }
    json_ok(&overview_json(&state, Some(&updated)))
}

/// The routes of this module, under `/api/v1`.
pub fn routes() -> axum::Router<GatewayState> {
    use axum::routing::{delete, get, post};
    axum::Router::new()
        .route("/mcp", get(overview))
        .route("/mcp/servers", post(add))
        .route("/mcp/servers/{name}", delete(remove))
        .route("/mcp/test", post(test))
        .route(
            "/mcp/abilities/{id}",
            post(enable_ability).delete(disable_ability),
        )
}

/// `GET /api/v1/mcp`: the configured servers and what became of each.
pub async fn overview(State(state): State<GatewayState>) -> Response {
    let context = match context(&state) {
        Ok(context) => context,
        Err(response) => return response,
    };
    let current = match read_document(context.paths().config()) {
        Ok(document) => document,
        Err(message) => return refused(&message),
    };
    json_ok(&overview_json(&state, current.as_deref()))
}

/// `POST /api/v1/mcp/servers`: add a server to the document.
pub async fn add(State(state): State<GatewayState>, Json(body): Json<ServerBody>) -> Response {
    let context = match context(&state) {
        Ok(context) => context,
        Err(response) => return response,
    };
    let directory = context.paths().config().to_path_buf();
    let _guard = EDIT_LOCK.lock().await;
    let current = match read_document(&directory) {
        Ok(document) => document,
        Err(message) => return refused(&message),
    };
    let updated = match add_to_document(current.as_deref(), &body, &find_program_on_path) {
        Ok(text) => text,
        Err(message) => return refused(&message),
    };
    if let Err(message) = write_document(&directory, &updated) {
        return error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            ErrorCode::Internal,
            &message,
        );
    }
    json_ok(&overview_json(&state, Some(&updated)))
}

/// `DELETE /api/v1/mcp/servers/{name}`: remove a server from the document.
pub async fn remove(State(state): State<GatewayState>, UrlPath(name): UrlPath<String>) -> Response {
    let context = match context(&state) {
        Ok(context) => context,
        Err(response) => return response,
    };
    let directory = context.paths().config().to_path_buf();
    let _guard = EDIT_LOCK.lock().await;
    let current = match read_document(&directory) {
        Ok(document) => document,
        Err(message) => return refused(&message),
    };
    let updated = match remove_from_document(current.as_deref(), &name) {
        Ok(text) => text,
        Err(message) => return refused(&message),
    };
    if let Err(message) = write_document(&directory, &updated) {
        return error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            ErrorCode::Internal,
            &message,
        );
    }
    json_ok(&overview_json(&state, Some(&updated)))
}

/// `POST /api/v1/mcp/test`: connect to a server once, list what it offers, and disconnect. Nothing is saved.
pub async fn test(State(state): State<GatewayState>, Json(mut body): Json<ServerBody>) -> Response {
    let context = match context(&state) {
        Ok(context) => context,
        Err(response) => return response,
    };
    if body.name.trim().is_empty() {
        "test".clone_into(&mut body.name);
    }
    let document = match add_to_document(None, &body, &find_program_on_path) {
        Ok(text) => text,
        Err(message) => return refused(&message),
    };
    let report = match tokio::time::timeout(
        TEST_DEADLINE,
        crate::mcp_host::try_connect(&document, context.paths()),
    )
    .await
    {
        Ok(report) => report,
        Err(_elapsed) => {
            return json_ok(&json!({
                "ok": false,
                "message": format!(
                    "no answer within {} seconds. A program run with `npx` may be downloading on its first start; try again.",
                    TEST_DEADLINE.as_secs()
                ),
            }));
        }
    };
    json_ok(&report)
}

fn json_ok(value: &serde_json::Value) -> Response {
    use axum::response::IntoResponse;
    (StatusCode::OK, Json(value.clone())).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[allow(
        clippy::unnecessary_wraps,
        reason = "the resolver the module takes returns a Result"
    )]
    fn found(program: &str) -> Result<String, String> {
        Ok(format!("/usr/bin/{program}"))
    }

    fn body(name: &str, command: &[&str]) -> ServerBody {
        ServerBody {
            name: name.to_owned(),
            command: command.iter().map(|part| (*part).to_owned()).collect(),
            endpoint: None,
            read_only: false,
            read_only_tools: Vec::new(),
            env: std::collections::BTreeMap::new(),
            authorization: None,
        }
    }

    #[test]
    fn a_server_is_added_with_its_program_resolved_and_listed_back() {
        let document = add_to_document(None, &body("files", &["npx", "-y", "some-server"]), &found)
            .unwrap_or_else(|error| panic!("{error}"));
        let listed = configured_servers(Some(&document));
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].name, "files");
        assert_eq!(listed[0].command, ["/usr/bin/npx", "-y", "some-server"]);
        assert_eq!(listed[0].posture, "unclassified", "no class is inferred");
    }

    #[test]
    fn credential_references_round_trip_without_changing_permission_posture() {
        let mut request = body("github", &["github-mcp-server", "stdio"]);
        request.env.insert(
            "GITHUB_PERSONAL_ACCESS_TOKEN".to_owned(),
            "mcp:github:token".to_owned(),
        );
        let document =
            add_to_document(None, &request, &found).unwrap_or_else(|error| panic!("{error}"));
        let servers = configured_servers(Some(&document));
        assert_eq!(servers[0].env, request.env);
        assert_eq!(servers[0].posture, "unclassified");
        for invalid in ["synthetic-literal-key", "mcp:other:token"] {
            request.env.insert(
                "GITHUB_PERSONAL_ACCESS_TOKEN".to_owned(),
                invalid.to_owned(),
            );
            let error = add_to_document(None, &request, &found)
                .err()
                .unwrap_or_else(|| panic!("must reject"));
            assert!(!error.contains(invalid));
        }
        assert!(
            configured_servers(Some(
                &document.replace("mcp:github:token", "synthetic-literal-key")
            ))
            .is_empty()
        );
    }

    #[test]
    fn read_only_is_only_what_the_owner_said() {
        let mut read_only = body("docs", &["server"]);
        read_only.read_only = true;
        let document = add_to_document(None, &read_only, &found).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(configured_servers(Some(&document))[0].posture, "read-only");
    }

    #[test]
    fn a_second_server_is_appended_and_a_duplicate_name_is_refused() {
        let first =
            add_to_document(None, &body("one", &["a"]), &found).unwrap_or_else(|e| panic!("{e}"));
        let second = add_to_document(Some(&first), &body("two", &["b"]), &found)
            .unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(configured_servers(Some(&second)).len(), 2);
        let error = add_to_document(Some(&second), &body("one", &["c"]), &found)
            .err()
            .unwrap_or_else(|| panic!("a duplicate name must be refused"));
        assert!(error.contains("already exists"), "{error}");
    }

    /// **Fail closed**: a document the daemon would refuse is never produced.
    #[test]
    fn a_name_the_daemon_would_refuse_is_refused_before_anything_is_written() {
        let error = add_to_document(None, &body("Bad Name!", &["a"]), &found)
            .err()
            .unwrap_or_else(|| panic!("an invalid name must be refused"));
        assert!(!error.is_empty());
        let empty = add_to_document(None, &body("  ", &["a"]), &found).err();
        assert!(empty.is_some_and(|message| message.contains("name")));
    }

    #[test]
    fn a_missing_program_says_what_to_install() {
        let missing = |program: &str| find_program(program, &[], &[]).map(|_| program.to_owned());
        let error = add_to_document(None, &body("x", &["npx"]), &missing)
            .err()
            .unwrap_or_else(|| panic!("a missing program must be refused"));
        assert!(error.contains("Node.js"), "{error}");
    }

    #[test]
    fn a_document_that_is_not_toml_is_left_alone() {
        let error = add_to_document(Some("servers = ["), &body("x", &["a"]), &found)
            .err()
            .unwrap_or_else(|| panic!("a broken file must not be overwritten"));
        assert!(error.contains("left alone"), "{error}");
    }

    #[test]
    fn a_command_and_an_endpoint_together_or_neither_are_refused() {
        let mut both = body("x", &["a"]);
        both.endpoint = Some("https://example.invalid/mcp".to_owned());
        assert!(add_to_document(None, &both, &found).is_err());
        assert!(add_to_document(None, &body("x", &[]), &found).is_err());
    }

    #[test]
    fn removing_a_server_keeps_the_others() {
        let first =
            add_to_document(None, &body("one", &["a"]), &found).unwrap_or_else(|e| panic!("{e}"));
        let second = add_to_document(Some(&first), &body("two", &["b"]), &found)
            .unwrap_or_else(|e| panic!("{e}"));
        let left = remove_from_document(Some(&second), "one").unwrap_or_else(|e| panic!("{e}"));
        let listed = configured_servers(Some(&left));
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].name, "two");
        assert!(remove_from_document(Some(&left), "one").is_err());
    }

    #[test]
    fn a_program_is_found_with_its_windows_extension_and_returned_absolute() {
        let directory = jarvis_core::scratch_tag();
        let root = std::env::temp_dir().join(format!("jarvis-find-{directory}"));
        std::fs::create_dir_all(&root).unwrap_or_else(|e| panic!("{e}"));
        let script = root.join("npx.cmd");
        std::fs::write(&script, "x").unwrap_or_else(|e| panic!("{e}"));
        let found = find_program(
            "npx",
            std::slice::from_ref(&root),
            &[".exe".to_owned(), ".cmd".to_owned()],
        );
        let _ = std::fs::remove_dir_all(&root);
        let path = found.unwrap_or_else(|e| panic!("{e}"));
        assert!(Path::new(&path).is_absolute(), "{path}");
        assert!(path.ends_with("npx.cmd"), "{path}");
    }

    /// The Windows trap: `npx` (a shell script) and `npx.cmd` sit side by side, and only the second can be started.
    #[test]
    fn an_extensionless_sibling_is_not_chosen_where_executables_have_extensions() {
        let root = std::env::temp_dir().join(format!("jarvis-find-{}", jarvis_core::scratch_tag()));
        std::fs::create_dir_all(&root).unwrap_or_else(|e| panic!("{e}"));
        std::fs::write(root.join("npx"), "#!/bin/sh").unwrap_or_else(|e| panic!("{e}"));
        std::fs::write(root.join("npx.cmd"), "x").unwrap_or_else(|e| panic!("{e}"));
        let windows = find_program("npx", std::slice::from_ref(&root), &[".cmd".to_owned()]);
        let unix = find_program("npx", std::slice::from_ref(&root), &[]);
        let _ = std::fs::remove_dir_all(&root);
        assert!(
            windows
                .unwrap_or_else(|e| panic!("{e}"))
                .ends_with("npx.cmd")
        );
        assert!(unix.unwrap_or_else(|e| panic!("{e}")).ends_with("npx"));
    }

    #[test]
    fn a_changed_file_needs_a_restart_and_an_unchanged_one_does_not() {
        assert!(!restart_needed(Some("a"), Some("a\n")));
        assert!(!restart_needed(None, Some("")));
        assert!(restart_needed(None, Some("[[servers]]")));
    }

    #[test]
    fn descriptions_are_one_bounded_line() {
        let long = format!("first line {}\nsecond", "x".repeat(400));
        let short = short_description(&long);
        assert!(!short.contains("second"));
        assert!(short.chars().count() <= DESCRIPTION_CHARS + 1);
    }
}
