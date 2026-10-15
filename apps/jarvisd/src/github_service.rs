//! Owner-authenticated GitHub setup and one-use public OAuth callback.

use std::sync::Arc;

use axum::{
    Json,
    extract::State,
    http::{Method, StatusCode, Uri, header},
    response::{IntoResponse, Response},
};
use jarvis_core::ErrorCode;
use serde::Deserialize;
use serde_json::json;

use crate::gateway::{GatewayState, error_response};
use crate::github_account::{CALLBACK_PATH, GithubAccount};

pub(crate) const MCP_ENDPOINT: &str = "https://api.githubcopilot.com/mcp/readonly";

#[cfg(test)]
#[path = "github_service_tests.rs"]
mod tests;

pub(crate) fn routes() -> axum::Router<GatewayState> {
    axum::Router::new()
        .route("/github", axum::routing::get(status))
        .route("/github/app", axum::routing::put(save_app))
        .route("/github/connect", axum::routing::post(connect))
        .route("/github/disconnect", axum::routing::post(disconnect))
        .route("/github/test", axum::routing::post(test))
}

pub(crate) fn is_oauth_callback(method: &Method, path: &str) -> bool {
    method == Method::GET && path == CALLBACK_PATH
}

#[allow(clippy::result_large_err)]
fn account(state: &GatewayState) -> Result<&Arc<GithubAccount>, Response> {
    state.github().ok_or_else(|| {
        error_response(
            StatusCode::NOT_FOUND,
            ErrorCode::UnavailableCapability,
            "GitHub sign-in is not available on this daemon",
        )
    })
}

fn failure(error: &impl std::fmt::Display) -> Response {
    error_response(
        StatusCode::UNPROCESSABLE_ENTITY,
        ErrorCode::Validation,
        &error.to_string(),
    )
}

fn listening_port(state: &GatewayState) -> u16 {
    state.http_port().unwrap_or_else(|| {
        state
            .settings()
            .and_then(|settings| {
                jarvis_storage::ConfigStore::from_paths(settings.paths())
                    .load()
                    .ok()
            })
            .map_or(8765, |loaded| loaded.config().daemon().http_port())
    })
}

fn status_value(state: &GatewayState, account: &GithubAccount) -> serde_json::Value {
    let mut reply = serde_json::to_value(account.status())
        .unwrap_or_else(|_| json!({ "configured": false, "connected": false }));
    let paths = state
        .settings()
        .map(crate::settings_service::SettingsContext::paths);
    let current = paths.and_then(|paths| {
        crate::mcp_host::load_document(paths.config())
            .ok()
            .flatten()
    });
    let snapshot = state.mcp();
    let registered = snapshot
        .servers
        .iter()
        .any(|server| server.name == "github" && server.state == "connected");
    reply["restart_needed"] = json!(
        !registered
            || current.as_deref().unwrap_or_default().trim()
                != snapshot.loaded.as_deref().unwrap_or_default().trim()
    );
    let port = listening_port(state);
    reply["callback_url"] = json!(format!("http://127.0.0.1:{port}{CALLBACK_PATH}"));
    reply["read_only"] = json!(true);
    reply
}

async fn status(State(state): State<GatewayState>) -> Response {
    match account(&state) {
        Ok(account) => Json(status_value(&state, account)).into_response(),
        Err(response) => response,
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AppBody {
    client_id: String,
    #[serde(default)]
    client_secret: Option<String>,
    #[serde(default)]
    private_repositories: bool,
}

async fn save_app(State(state): State<GatewayState>, Json(body): Json<AppBody>) -> Response {
    let account = match account(&state) {
        Ok(account) => account,
        Err(response) => return response,
    };
    match account.save_app(
        &body.client_id,
        body.client_secret.as_deref(),
        body.private_repositories,
    ) {
        Ok(()) => Json(status_value(&state, account)).into_response(),
        Err(error) => failure(&error),
    }
}

async fn connect(State(state): State<GatewayState>) -> Response {
    let account = match account(&state) {
        Ok(account) => account,
        Err(response) => return response,
    };
    let settings = match crate::settings_service::context(&state) {
        Ok(settings) => settings,
        Err(response) => return response,
    };
    if !account.status().configured {
        return failure(&"Complete the one-time GitHub app setup first");
    }
    if let Err(message) = crate::mcp_admin::configure_github(settings.paths()).await {
        return failure(&message);
    }
    let port = listening_port(&state);
    match account.begin(port) {
        Ok(url) => Json(json!({ "auth_url": url })).into_response(),
        Err(error) => failure(&error),
    }
}

async fn disconnect(State(state): State<GatewayState>) -> Response {
    let account = match account(&state) {
        Ok(account) => account,
        Err(response) => return response,
    };
    match account.disconnect().await {
        Ok(()) => Json(status_value(&state, account)).into_response(),
        Err(error) => failure(&error),
    }
}

async fn test(State(state): State<GatewayState>) -> Response {
    let account = match account(&state) {
        Ok(account) => account,
        Err(response) => return response,
    };
    if !account.status().connected {
        return failure(&"Connect GitHub before testing its tools");
    }
    let settings = match crate::settings_service::context(&state) {
        Ok(settings) => settings,
        Err(response) => return response,
    };
    let document = match crate::mcp_admin::github_document(None) {
        Ok(document) => document,
        Err(message) => return failure(&message),
    };
    match tokio::time::timeout(
        std::time::Duration::from_secs(60),
        crate::mcp_host::try_connect(&document, settings.paths()),
    )
    .await
    {
        Ok(report) => Json(report).into_response(),
        Err(_) => failure(&"GitHub did not complete the connection test within 60 seconds"),
    }
}

pub(crate) async fn callback(State(state): State<GatewayState>, uri: Uri) -> Response {
    let target = uri
        .path_and_query()
        .map_or_else(|| uri.path().to_owned(), ToString::to_string);
    let success = match state.github() {
        Some(account) => account.complete(&target).await.is_ok(),
        None => false,
    };
    let (code, heading, detail) = if success {
        (
            StatusCode::OK,
            "GitHub is connected",
            "Return to Settings. Your sign-in is saved. Restart JARVIS when prompted to enable the read-only tools.",
        )
    } else {
        (
            StatusCode::BAD_REQUEST,
            "GitHub was not connected",
            "The sign-in was cancelled, expired or refused. Return to Settings and try Connect GitHub again.",
        )
    };
    let page = format!(
        "<!doctype html><meta charset=\"utf-8\"><title>JARVIS</title><h1>{heading}</h1><p>{detail}</p><a href=\"/\">Back to JARVIS</a>"
    );
    let mut response = (code, page).into_response();
    let headers = response.headers_mut();
    headers.insert(
        header::CONTENT_TYPE,
        axum::http::HeaderValue::from_static("text/html; charset=utf-8"),
    );
    headers.insert(
        header::CACHE_CONTROL,
        axum::http::HeaderValue::from_static("no-store"),
    );
    headers.insert(
        header::REFERRER_POLICY,
        axum::http::HeaderValue::from_static("no-referrer"),
    );
    headers.insert(
        header::CONTENT_SECURITY_POLICY,
        axum::http::HeaderValue::from_static(
            "default-src 'none'; base-uri 'none'; frame-ancestors 'none'; form-action 'none'",
        ),
    );
    response
}
