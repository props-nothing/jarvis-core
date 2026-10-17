//! The Settings screen's API (`P9-015`): read the settings, change one, set or remove a key, and restart to apply.
//!
//! All of it is `jarvis_storage::settings`, the same code `jarvis config` and `jarvis keys` use, so a change cannot be valid
//! on the screen and refused at startup. Every route sits behind the local credential. **No response contains a key**: a
//! key is accepted write-only and only its state (`set`, `not set`, `FILE MISSING`, `EMPTY FILE`) is ever reported.

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use axum::{
    Json,
    extract::{Path as UrlPath, RawQuery, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use jarvis_core::ErrorCode;
use jarvis_storage::settings::{self, SecretKind};
use jarvis_storage::{AppPaths, ConfigStore};
use serde::Deserialize;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::fmt::Write as _;

use crate::gateway::{GatewayState, error_response};

/// Where this daemon's profile is, held so the routes read and write the same configuration the daemon started from.
#[derive(Clone)]
pub struct SettingsContext {
    paths: AppPaths,
    root: Option<PathBuf>,
    startup: Option<BTreeMap<String, String>>,
}

impl SettingsContext {
    /// The profile directories the settings are read from.
    #[must_use]
    pub const fn paths(&self) -> &AppPaths {
        &self.paths
    }

    /// Builds the context from the daemon's resolved paths and its `--root`, if any.
    #[must_use]
    pub fn new(paths: AppPaths, root: Option<PathBuf>) -> Self {
        let startup = configuration_snapshot(&paths).ok();
        Self {
            paths,
            root,
            startup,
        }
    }
}

// Private fingerprints are compared only in memory; responses contain setting names, never hashes or secret bytes.
fn configuration_snapshot(paths: &AppPaths) -> Result<BTreeMap<String, String>, String> {
    settings::list(paths)?
        .into_iter()
        .filter(|view| {
            // These switches are read live; tool postures have their own applied response.
            !view.key.starts_with("policy.")
                && !matches!(
                    view.key.as_str(),
                    "daemon.phone_approves"
                        | "daemon.phone_opening_line"
                        | "daemon.phone_call_anyone"
                        | "daemon.phone_screen_callers"
                        | "daemon.phone_calls_per_hour"
                        | "daemon.phone_calls_per_day"
                )
        })
        .map(|view| {
            let mut value = view
                .value
                .unwrap_or_else(|| view.default.unwrap_or_default().to_owned());
            if view.kind == "file" && !value.is_empty() {
                let bytes = std::fs::read(&value)
                    .map_err(|_| "a configured credential file cannot be read".to_owned())?;
                let _ = write!(value, "/{:x}", Sha256::digest(bytes));
            }
            Ok((view.key, value))
        })
        .collect()
}

/// The body of `PUT /api/v1/settings/{key}`.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SetBody {
    values: Vec<String>,
}

/// One change in `PUT /api/v1/settings`: set to these values, or remove (`values: null`).
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChangeBody {
    key: String,
    #[serde(default)]
    values: Option<Vec<String>>,
}

/// The body of `PUT /api/v1/settings`.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BatchBody {
    changes: Vec<ChangeBody>,
}

/// The body of `PUT /api/v1/settings/tools/{tool}`.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PostureBody {
    posture: String,
}

/// The body of `PUT /api/v1/settings/keys/{which}`.
///
/// No `Debug`: a derived one would print the key into any log that formatted the request.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KeyBody {
    key: String,
}

/// An entire provider form. Secret input deliberately has no Debug implementation.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProviderBody {
    slot: String,
    model: String,
    base_url: Option<String>,
    key: Option<String>,
}

/// `PUT /api/v1/settings/provider`: commit address, model and credential together.
pub async fn set_provider(
    State(state): State<GatewayState>,
    Json(body): Json<ProviderBody>,
) -> Response {
    if body
        .base_url
        .as_deref()
        .is_some_and(|address| jarvis_models::openai::BaseUrl::new(address).is_err())
    {
        return refused(
            "That model address is not usable; use an http or https address without a login.",
        );
    }
    let context = match context(&state) {
        Ok(context) => context,
        Err(response) => return response,
    };
    let fallback = match body.slot.as_str() {
        "main" => false,
        "fallback" => true,
        _ => return refused("unknown model slot"),
    };
    match settings::set_provider(
        &context.paths,
        fallback,
        &body.model,
        body.base_url.as_deref(),
        body.key.as_deref(),
    ) {
        Ok(()) => (
            StatusCode::OK,
            Json(json!({"saved": true, "restart_to_apply": true})),
        )
            .into_response(),
        Err(message) => refused(&message),
    }
}

// The error is the response itself, as every handler here returns it; boxing it would only add an unwrap at each use.
#[allow(clippy::result_large_err)]
pub(crate) fn context(state: &GatewayState) -> Result<&SettingsContext, Response> {
    state.settings().ok_or_else(|| {
        error_response(
            StatusCode::NOT_FOUND,
            ErrorCode::UnavailableCapability,
            "settings are not available on this daemon",
        )
    })
}

/// A refusal with its reason. An API error message is one bounded line, so a longer explanation (the CLI lists every
/// setting) is cut to its first line.
pub(crate) fn refused(message: &str) -> Response {
    let first_line: String = message
        .lines()
        .next()
        .unwrap_or_default()
        .chars()
        .take(300)
        .collect();
    error_response(
        StatusCode::UNPROCESSABLE_ENTITY,
        ErrorCode::Validation,
        &first_line,
    )
}

fn key_json(paths: &AppPaths, kind: SecretKind) -> serde_json::Value {
    settings::key_state(paths, kind).map_or_else(
        |_| json!({ "state": "unknown" }),
        |state| json!({ "state": state.state }),
    )
}

/// `GET /api/v1/settings`
pub async fn list(State(state): State<GatewayState>) -> Response {
    let context = match context(&state) {
        Ok(context) => context,
        Err(response) => return response,
    };
    match settings::list(&context.paths) {
        Ok(views) => {
            let items: Vec<_> = views
                .into_iter()
                .map(|view| {
                    json!({
                        "key": view.key,
                        "kind": view.kind,
                        "help": view.help,
                        "value": view.value,
                        "file_state": view.file_state,
                        "group": view.group,
                        "default": view.default,
                        "unset_means": view.unset_means,
                        "example": view.example,
                    })
                })
                .collect();
            let postures = settings::tool_postures(&context.paths).unwrap_or_default();
            let providers: Vec<_> = crate::model_probe::PRESETS
                .iter()
                .map(|preset| {
                    json!({ "id": preset.id, "name": preset.name, "url": preset.url, "needs_key": preset.needs_key, "help": preset.help })
                })
                .collect();
            let current = configuration_snapshot(&context.paths).ok();
            let restart_known = context.startup.is_some() && current.is_some();
            let changes: Vec<_> = context
                .startup
                .as_ref()
                .zip(current)
                .map(|(startup, current)| {
                    current
                        .into_iter()
                        .filter_map(|(key, value)| {
                            (startup.get(&key) != Some(&value)).then_some(key)
                        })
                        .collect()
                })
                .unwrap_or_default();
            let body = json!({
                "profile_id": format!("{:x}", Sha256::digest(context.paths.config().to_string_lossy().as_bytes())),
                "restart_state_known": restart_known,
                "restart_changes": changes,
                "settings": items,
                "providers": providers,
                "postures": postures,
                "keys": {
                    "model": key_json(&context.paths, SecretKind::Model),
                    "voice": key_json(&context.paths, SecretKind::Voice),
            "search": key_json(&context.paths, SecretKind::Search),
            "google": key_json(&context.paths, SecretKind::Google),
            "fallback": key_json(&context.paths, SecretKind::Fallback),
            "telegram": key_json(&context.paths, SecretKind::Telegram),
                },
            });
            (StatusCode::OK, Json(body)).into_response()
        }
        Err(message) => refused(&message),
    }
}

/// `PUT /api/v1/settings/{key}`
pub async fn set(
    State(state): State<GatewayState>,
    UrlPath(key): UrlPath<String>,
    Json(body): Json<SetBody>,
) -> Response {
    let context = match context(&state) {
        Ok(context) => context,
        Err(response) => return response,
    };
    match settings::set(&context.paths, &key, &body.values) {
        Ok(()) => (
            StatusCode::OK,
            Json(json!({ "saved": true, "restart_to_apply": true })),
        )
            .into_response(),
        Err(message) => refused(&message),
    }
}

/// `PUT /api/v1/settings`: several changes, applied together or not at all.
pub async fn batch(State(state): State<GatewayState>, Json(body): Json<BatchBody>) -> Response {
    let context = match context(&state) {
        Ok(context) => context,
        Err(response) => return response,
    };
    if body.changes.is_empty() || body.changes.len() > 16 {
        return refused("send between 1 and 16 changes");
    }
    let changes: Vec<settings::Change> = body
        .changes
        .into_iter()
        .map(|change| settings::Change {
            key: change.key,
            values: change.values,
        })
        .collect();
    match settings::apply(&context.paths, &changes) {
        Ok(()) => (
            StatusCode::OK,
            Json(json!({ "saved": true, "restart_to_apply": true })),
        )
            .into_response(),
        Err(message) => refused(&message),
    }
}

/// Rebuilds the workspace policy from the saved configuration and hands it to the running pipeline, so a change to a tool's
/// permission (including "off" and "always ask") applies to the next call. Returns whether it did.
///
/// Fails closed in the useful direction: when the saved file cannot be read or composed, the running policy is left exactly as it
/// was and the caller says a restart is needed, so a half-understood file never loosens anything.
fn apply_policy_now(state: &GatewayState, context: &SettingsContext) -> bool {
    let Some(tools) = state.tools() else {
        return false;
    };
    let Ok(loaded) = ConfigStore::from_paths(&context.paths).load() else {
        return false;
    };
    match crate::compose_workspace_policy(loaded.config()) {
        Ok(policy) => {
            tools.replace_workspace_policy(policy);
            true
        }
        Err(_) => false,
    }
}

/// `PUT /api/v1/settings/tools/{tool}`: what the owner has decided about one tool (default, ask, trusted or off).
pub async fn set_posture(
    State(state): State<GatewayState>,
    UrlPath(tool): UrlPath<String>,
    Json(body): Json<PostureBody>,
) -> Response {
    let context = match context(&state) {
        Ok(context) => context,
        Err(response) => return response,
    };
    let posture = match settings::Posture::parse(&body.posture) {
        Ok(posture) => posture,
        Err(message) => return refused(&message),
    };
    // Only a tool this daemon actually has: a typo in a trust list would otherwise be a silent no-op.
    if let Some(tools) = state.tools()
        && !tools
            .policy_inventory()
            .is_ok_and(|entries| entries.iter().any(|entry| entry.id == tool))
    {
        return refused("this daemon has no tool with that id");
    }
    match settings::set_tool_posture(&context.paths, &tool, posture) {
        Ok(()) => {
            tracing::info!(%tool, posture = posture.name(), "a tool's permission was changed from the settings screen");
            let applied = apply_policy_now(&state, context);
            (
                StatusCode::OK,
                Json(json!({ "saved": true, "restart_to_apply": !applied, "applied": applied })),
            )
                .into_response()
        }
        Err(message) => refused(&message),
    }
}

/// `DELETE /api/v1/settings/{key}`
pub async fn unset(State(state): State<GatewayState>, UrlPath(key): UrlPath<String>) -> Response {
    let context = match context(&state) {
        Ok(context) => context,
        Err(response) => return response,
    };
    match settings::unset(&context.paths, &key) {
        Ok(()) => (
            StatusCode::OK,
            Json(json!({ "saved": true, "restart_to_apply": true })),
        )
            .into_response(),
        Err(message) => refused(&message),
    }
}

/// The body of `POST /api/v1/settings/models`.
///
/// No `Debug`: it may carry a key being tried out.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProbeBody {
    /// `main` or `fallback`: which provider slot this is for.
    slot: String,
    /// An address to look at instead of the configured one.
    #[serde(default)]
    base_url: Option<String>,
    /// A key typed on the screen, not yet saved. Without one, the stored key is used for the configured address only.
    #[serde(default)]
    key: Option<String>,
    /// A model to ask for one short answer, as a test.
    #[serde(default)]
    test_model: Option<String>,
}

/// `POST /api/v1/settings/models`: the models a provider serves, and optionally a test of one.
///
/// The daemon makes the request, so a stored key is never sent to the browser, and it is only ever sent to the address it was stored for
/// (`model_probe::resolve`). The answer holds model names and plain sentences, never a key.
pub async fn probe_models(
    State(state): State<GatewayState>,
    Json(body): Json<ProbeBody>,
) -> Response {
    let context = match context(&state) {
        Ok(context) => context,
        Err(response) => return response,
    };
    let paths = context.paths();
    let (url_field, key_field) = match body.slot.as_str() {
        "main" => ("executor_base_url", "executor_api_key_ref"),
        "fallback" => (
            "executor_fallback_base_url",
            "executor_fallback_api_key_ref",
        ),
        _ => return refused("the slot must be main or fallback"),
    };
    // A fallback with no address of its own is asked at the main provider, with the main key.
    let (configured_url, key_file) = match settings::get(paths, url_field) {
        Ok(url) => (Some(url), settings::get(paths, key_field).ok()),
        Err(_) if body.slot == "fallback" => (
            settings::get(paths, "executor_base_url").ok(),
            settings::get(paths, "executor_api_key_ref").ok(),
        ),
        Err(_) => (None, None),
    };
    let resolved = crate::model_probe::resolve(
        configured_url.as_deref(),
        body.base_url.as_deref(),
        body.key.as_deref(),
        || key_file.and_then(|file| std::fs::read_to_string(file).ok()),
    );
    let (base, key) = match resolved {
        Ok(found) => found,
        Err(message) => return refused(&message),
    };
    let Ok(transport) = jarvis_models::openai::HttpTransport::new() else {
        return error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            ErrorCode::Internal,
            "the network client could not be built",
        );
    };
    let transport: std::sync::Arc<dyn jarvis_models::openai::Transport> =
        std::sync::Arc::new(transport);
    let test_model = body
        .test_model
        .as_deref()
        .map(str::trim)
        .filter(|name| !name.is_empty());
    match crate::model_probe::run(transport, base, &key, test_model).await {
        Ok(report) => {
            let models: Vec<_> = report
                .models
                .iter()
                .map(|model| json!({ "id": model.id, "likely_chat": model.likely_chat }))
                .collect();
            let test = report.test.map(
                |test| json!({ "ok": test.ok, "message": test.message, "millis": test.millis }),
            );
            (
                StatusCode::OK,
                Json(
                    json!({ "models": models, "list_problem": report.list_problem, "test": test }),
                ),
            )
                .into_response()
        }
        Err(message) => refused(&message),
    }
}

/// `PUT /api/v1/settings/keys/{which}`: write-only.
pub async fn set_key(
    State(state): State<GatewayState>,
    UrlPath(which): UrlPath<String>,
    Json(body): Json<KeyBody>,
) -> Response {
    let context = match context(&state) {
        Ok(context) => context,
        Err(response) => return response,
    };
    let kind = match SecretKind::parse(&which) {
        Ok(kind) => kind,
        Err(message) => return refused(&message),
    };
    match settings::set_secret(&context.paths, kind, &body.key) {
        Ok(_) => {
            // A different bot is a different chat: the old pairing means nothing to it.
            if kind == SecretKind::Telegram
                && let Some(bridge) = state.telegram()
            {
                bridge.unpair();
            }
            tracing::info!(key = %which, "a key was set from the settings screen");
            let body = json!({ "saved": true, "restart_to_apply": true, "key": key_json(&context.paths, kind) });
            (StatusCode::OK, Json(body)).into_response()
        }
        Err(message) => refused(&message),
    }
}

/// `DELETE /api/v1/settings/keys/{which}`
pub async fn remove_key(
    State(state): State<GatewayState>,
    UrlPath(which): UrlPath<String>,
) -> Response {
    let context = match context(&state) {
        Ok(context) => context,
        Err(response) => return response,
    };
    let kind = match SecretKind::parse(&which) {
        Ok(kind) => kind,
        Err(message) => return refused(&message),
    };
    match settings::remove_secret(&context.paths, kind) {
        Ok(()) if kind == SecretKind::Telegram => {
            if let Some(bridge) = state.telegram() {
                bridge.unpair();
            }
            (
                StatusCode::OK,
                Json(json!({ "saved": true, "restart_to_apply": false })),
            )
                .into_response()
        }
        Ok(()) => (
            StatusCode::OK,
            Json(json!({ "saved": true, "restart_to_apply": true })),
        )
            .into_response(),
        Err(message) => refused(&message),
    }
}

/// `POST /api/v1/restart`: starts `jarvis restart` (the `jarvis` program beside this one) and returns at once; that command
/// stops this daemon gracefully and starts a new one with the saved settings.
pub async fn restart(State(state): State<GatewayState>, RawQuery(raw): RawQuery) -> Response {
    restart_if_ready(&state, crate::stop::wants_force(raw.as_deref()), |root, force| {
        let Some(cli) = std::env::current_exe()
            .ok()
            .and_then(|exe| sibling_cli(&exe))
        else {
            return error_response(
                StatusCode::NOT_IMPLEMENTED,
                ErrorCode::Unsupported,
                "the `jarvis` program was not found beside this one; run `jarvis restart` in a terminal",
            );
        };
        match spawn_restart(&cli, root, force) {
            Ok(()) => (StatusCode::ACCEPTED, Json(json!({ "restarting": true }))).into_response(),
            Err(message) => error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                ErrorCode::Internal,
                &message,
            ),
        }
    }).await
}

async fn restart_if_ready(
    state: &GatewayState,
    force: bool,
    launch: impl FnOnce(Option<&Path>, bool) -> Response,
) -> Response {
    let context = match context(state) {
        Ok(context) => context,
        Err(response) => return response,
    };
    let permit = match crate::stop::prepare_shutdown(state, force).await {
        Ok(Some(permit)) => permit,
        Ok(None) => return crate::stop::shutdown_in_progress(),
        Err(refusal) => return *refusal,
    };
    let mut response = crate::stop::shutdown_in_progress();
    if permit
        .commit_if(|| {
            response = launch(context.root.as_deref(), force);
            response.status().is_success()
        })
        .is_err()
    {
        return crate::stop::shutdown_in_progress();
    }
    response
}

fn sibling_cli(daemon: &Path) -> Option<PathBuf> {
    let name = if cfg!(windows) {
        "jarvis.exe"
    } else {
        "jarvis"
    };
    let candidate = daemon.parent()?.join(name);
    candidate.is_file().then_some(candidate)
}

/// Starts `jarvis restart [--root DIR]` detached, sharing no handle with this process.
fn spawn_restart(cli: &Path, root: Option<&Path>, force: bool) -> Result<(), String> {
    #[cfg(windows)]
    {
        let quote = |text: &str| format!("'{}'", text.replace('\'', "''"));
        let mut arguments = vec![quote("restart")];
        if force {
            arguments.push(quote("--force"));
        }
        if let Some(root) = root {
            arguments.push(quote("--root"));
            arguments.push(quote(&root.display().to_string()));
        }
        let script = format!(
            "Start-Process -FilePath {} -ArgumentList @({}) -WindowStyle Hidden",
            quote(&cli.display().to_string()),
            arguments.join(",")
        );
        std::process::Command::new("powershell.exe")
            .args(["-NoProfile", "-NonInteractive", "-Command", &script])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .map(|_| ())
            .map_err(|error| error.to_string())
    }
    #[cfg(not(windows))]
    {
        let mut command = std::process::Command::new(cli);
        command.arg("restart");
        if force {
            command.arg("--force");
        }
        if let Some(root) = root {
            command.arg("--root").arg(root);
        }
        command
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .map(|_| ())
            .map_err(|error| error.to_string())
    }
}

#[cfg(test)]
#[path = "settings_restart_tests.rs"]
mod restart_tests;
