//! Owner-authenticated generic MCP OAuth setup and a one-use public browser return.

use std::sync::Arc;

use axum::{
    Json,
    extract::{Path, State},
    http::{Method, StatusCode, Uri, header},
    response::{IntoResponse, Response},
};
use jarvis_core::{ErrorCode, McpSecretRef};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::gateway::{GatewayState, error_response};
use crate::mcp_oauth_account::Account;
use crate::mcp_oauth_wire::OAuthError;

pub(crate) fn routes() -> axum::Router<GatewayState> {
    axum::Router::new()
        .route("/mcp/servers/{server}/oauth", axum::routing::get(status))
        .route(
            "/mcp/servers/{server}/oauth/setup",
            axum::routing::put(setup),
        )
        .route(
            "/mcp/servers/{server}/oauth/connect",
            axum::routing::post(connect),
        )
        .route(
            "/mcp/servers/{server}/oauth/disconnect",
            axum::routing::post(disconnect),
        )
        .route(
            "/mcp/servers/{server}/oauth/test",
            axum::routing::post(test),
        )
}

pub(crate) fn is_oauth_callback(method: &Method, path: &str) -> bool {
    if method != Method::GET {
        return false;
    }
    let Some(server) = path
        .strip_prefix("/oauth/mcp/")
        .and_then(|path| path.strip_suffix("/callback"))
    else {
        return false;
    };
    McpSecretRef::parse(&format!("mcp:{server}:oauth-client")).is_ok()
}

fn configured(state: &GatewayState, name: &str) -> Result<(Arc<Account>, String), OAuthError> {
    let paths = state.settings().ok_or(OAuthError::InvalidSetup)?.paths();
    let current =
        crate::mcp_host::load_document(paths.config()).map_err(|_| OAuthError::Storage)?;
    let selected = crate::mcp_admin::configured_servers(current.as_deref())
        .into_iter()
        .find(|server| {
            server.name == name
                && server.authorization.as_deref() == Some(format!("oauth:{name}").as_str())
        })
        .ok_or(OAuthError::InvalidSetup)?;
    let endpoint = jarvis_mcp_transport::McpHttpEndpoint::parse(
        selected
            .endpoint
            .as_deref()
            .ok_or(OAuthError::InvalidSetup)?,
    )
    .map_err(|_| OAuthError::InvalidSetup)?;
    let account = state
        .mcp_oauth()
        .ok_or(OAuthError::InvalidSetup)?
        .account(name)?;
    Ok((account, endpoint.as_str().to_owned()))
}

fn port(state: &GatewayState) -> u16 {
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

pub(crate) fn status_value(state: &GatewayState, name: &str, endpoint: &str) -> Option<Value> {
    let account = state.mcp_oauth()?.account(name).ok()?;
    let endpoint = jarvis_mcp_transport::McpHttpEndpoint::parse(endpoint).ok()?;
    let mut status = serde_json::to_value(account.status(endpoint.as_str())).ok()?;
    status["callback_url"] = json!(format!(
        "http://127.0.0.1:{}{}",
        port(state),
        crate::mcp_oauth_account::callback_path(name)
    ));
    let current = state.settings().and_then(|settings| {
        crate::mcp_host::load_document(settings.paths().config())
            .ok()
            .flatten()
    });
    status["restart_needed"] = json!(
        current.as_deref().unwrap_or_default().trim()
            != state.mcp().loaded.as_deref().unwrap_or_default().trim()
            || !state
                .mcp()
                .servers
                .iter()
                .any(|server| server.name == name && server.state == "connected")
    );
    Some(status)
}

fn failure(error: OAuthError) -> Response {
    error_response(
        StatusCode::UNPROCESSABLE_ENTITY,
        ErrorCode::Validation,
        &error.to_string(),
    )
}

async fn status(State(state): State<GatewayState>, Path(name): Path<String>) -> Response {
    match configured(&state, &name) {
        Ok((_, endpoint)) => Json(status_value(&state, &name, &endpoint)).into_response(),
        Err(error) => failure(error),
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SetupBody {
    issuer: String,
    client_id: String,
    #[serde(default)]
    client_secret: Option<String>,
    #[serde(default)]
    scopes: Vec<String>,
}

async fn setup(
    State(state): State<GatewayState>,
    Path(name): Path<String>,
    Json(body): Json<SetupBody>,
) -> Response {
    let (account, endpoint) = match configured(&state, &name) {
        Ok(selected) => selected,
        Err(error) => return failure(error),
    };
    match account.save_setup(
        &endpoint,
        &body.issuer,
        &body.client_id,
        body.client_secret.as_deref(),
        &body.scopes,
    ) {
        Ok(()) => Json(status_value(&state, &name, &endpoint)).into_response(),
        Err(error) => failure(error),
    }
}

async fn connect(State(state): State<GatewayState>, Path(name): Path<String>) -> Response {
    let (account, endpoint) = match configured(&state, &name) {
        Ok(selected) => selected,
        Err(error) => return failure(error),
    };
    match tokio::time::timeout(
        std::time::Duration::from_secs(60),
        account.begin(&endpoint, port(&state)),
    )
    .await
    {
        Ok(Ok(url)) => {
            let Ok(mut base) = reqwest::Url::parse(&url) else {
                return failure(OAuthError::Refused);
            };
            base.set_query(None);
            Json(json!({"auth_url": url, "authorization_endpoint": base.as_str(), "issuer": account.status(&endpoint).issuer})).into_response()
        }
        Ok(Err(error)) => failure(error),
        Err(_) => failure(OAuthError::Unreachable),
    }
}

async fn disconnect(State(state): State<GatewayState>, Path(name): Path<String>) -> Response {
    let (account, endpoint) = match configured(&state, &name) {
        Ok(selected) => selected,
        Err(error) => return failure(error),
    };
    match account.disconnect(&endpoint).await {
        Ok(revoked) => {
            let mut reply = status_value(&state, &name, &endpoint)
                .unwrap_or_else(|| json!({"connected": false}));
            reply["revoked"] = json!(revoked);
            Json(reply).into_response()
        }
        Err(error) => failure(error),
    }
}

async fn test(State(state): State<GatewayState>, Path(name): Path<String>) -> Response {
    let (account, endpoint) = match configured(&state, &name) {
        Ok(selected) => selected,
        Err(error) => return failure(error),
    };
    if !account.status(&endpoint).connected {
        return failure(OAuthError::NotConnected);
    }
    let Some(settings) = state.settings() else {
        return failure(OAuthError::InvalidSetup);
    };
    let body = crate::mcp_admin::ServerBody {
        name: name.clone(),
        endpoint: Some(endpoint),
        authorization: Some(format!("oauth:{name}")),
        command: Vec::new(),
        env: std::collections::BTreeMap::new(),
        read_only: false,
        read_only_tools: Vec::new(),
    };
    let Ok(document) =
        crate::mcp_admin::add_to_document(None, &body, &|_| Err("no local command".to_owned()))
    else {
        return failure(OAuthError::InvalidSetup);
    };
    match tokio::time::timeout(
        std::time::Duration::from_secs(60),
        crate::mcp_host::try_connect(&document, settings.paths()),
    )
    .await
    {
        Ok(report) => Json(report).into_response(),
        Err(_) => failure(OAuthError::Unreachable),
    }
}

pub(crate) async fn callback(
    State(state): State<GatewayState>,
    Path(name): Path<String>,
    uri: Uri,
) -> Response {
    let target = uri
        .path_and_query()
        .map_or_else(|| uri.path().to_owned(), ToString::to_string);
    let success = match configured(&state, &name) {
        Ok((account, endpoint)) => account.complete(&endpoint, &target).await.is_ok(),
        Err(_) => false,
    };
    let (status, heading, detail) = if success {
        (
            StatusCode::OK,
            "The service is connected",
            "Return to Settings. Your authorization is saved; restart JARVIS when prompted to register tools.",
        )
    } else {
        (
            StatusCode::BAD_REQUEST,
            "The service was not connected",
            "Sign-in was cancelled, expired or refused. Return to Settings and try again.",
        )
    };
    let page = format!(
        "<!doctype html><meta charset=\"utf-8\"><title>JARVIS</title><h1>{heading}</h1><p>{detail}</p><a href=\"/\">Back to JARVIS</a>"
    );
    let mut response = (status, page).into_response();
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
