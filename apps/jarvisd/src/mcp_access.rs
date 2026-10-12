//! Letting other apps use JARVIS: the on/off switch, the key, and the check in front of the endpoint (`ADR-0170`).
//!
//! JARVIS can already be an MCP server (`ADR-0039`): another agent (Claude, Cursor, VS Code, a script) connects, lists JARVIS's tools
//! and calls one, including `jarvis.agent.delegate`, which hands a whole task to JARVIS and returns its answer. What was missing was a way to
//! say who is allowed. The endpoint admitted **nobody** (a request has no peer address to prove it is local, so every caller is treated as remote
//! and none had a credential), and turning it on meant editing a port setting and granting folders by hand.
//!
//! **One key, held by the owner.** A random key is generated, stored beside the console credential (`mcp-access.key`) and shown in the console so it
//! can be pasted into the other app. A request must carry it as `Authorization: Bearer <key>`; the comparison is constant-time and happens *before*
//! the endpoint sees the request. Once it passes, the endpoint's own admission is given **one fixed caller** (a constant fingerprint), so the existing
//! allowlist, origin and rate rules still run and nothing was bypassed. Because the fixed caller does not depend on the key, **a new key takes effect at
//! once** and the old one stops working without a restart.
//!
//! **The key is not the console credential.** The console credential can approve and deny; this key can only reach the served tools. An agent holding it
//! cannot answer its own approvals (those stay with the owner). A tool that needs one **waits** for the owner's answer, which appears with the console's other approvals, and is refused if the owner says no or does not answer in two minutes (`ADR-0180`).
//!
//! **Only this computer can connect, until the owner asks for more.** The endpoint binds loopback (`ServingConfig`). A cloud agent can reach it only
//! through the optional public tunnel (`mcp_tunnel`, `ADR-0173`), which forwards to the same key-checked endpoint.

use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

use axum::{
    Json,
    extract::{Request, State},
    http::{HeaderValue, StatusCode, header},
    middleware::Next,
    response::{IntoResponse, Response},
};
use jarvis_core::{ClientCredential, ErrorCode};
use jarvis_storage::{CredentialStore, settings};
use serde::Deserialize;
use serde_json::json;

use crate::gateway::{GatewayState, error_response};
use crate::settings_service::{context, refused};

/// The file the key lives in, beside `client.credential`.
pub const KEY_FILE_NAME: &str = "mcp-access.key";

/// The first port from `start` that nothing is listening on, trying a few. Falls back to `start` itself (the daemon then reports the clash by name).
fn first_free_port(start: u16) -> u16 {
    (start..start.saturating_add(30))
        .find(|port| std::net::TcpListener::bind(("127.0.0.1", *port)).is_ok())
        .unwrap_or(start)
}

/// The path an MCP client connects to.
pub const ENDPOINT_PATH: &str = "/mcp";

/// The port offered when the owner turns it on and has not chosen one.
pub const DEFAULT_PORT: u16 = 8766;

/// The one caller the endpoint's admission is told about. Not a secret: knowing it admits nothing, because the key is checked first.
pub const ACCESS_FINGERPRINT: &str = "jarvisapikeycaller1";

/// Requests per minute the key holder may make. An agent turn is a handful of round trips; this is far above that and well below a runaway.
pub const REQUESTS_PER_MINUTE: u32 = 120;

/// The key, and what is true of the endpoint in this daemon's life.
pub struct McpAccess {
    directory: PathBuf,
    key: RwLock<Option<ClientCredential>>,
    serving_port: RwLock<Option<u16>>,
    tunnel: Arc<crate::mcp_tunnel::McpTunnel>,
    approvals: Arc<crate::inbound_approvals::InboundApprovals>,
}

impl McpAccess {
    /// Opens the access state for a configuration directory, reading the key if one exists. Creates nothing.
    #[must_use]
    pub fn open(directory: &Path) -> Self {
        let key = CredentialStore::at(directory.join(KEY_FILE_NAME))
            .load()
            .ok();
        Self {
            directory: directory.to_path_buf(),
            key: RwLock::new(key),
            serving_port: RwLock::new(None),
            tunnel: crate::mcp_tunnel::McpTunnel::new(directory),
            approvals: Arc::new(crate::inbound_approvals::InboundApprovals::default()),
        }
    }

    /// The calls from other apps that are waiting for the owner's answer (`ADR-0180`).
    #[must_use]
    pub const fn approvals(&self) -> &Arc<crate::inbound_approvals::InboundApprovals> {
        &self.approvals
    }

    /// The public tunnel (`ADR-0173`).
    #[must_use]
    pub const fn tunnel(&self) -> &Arc<crate::mcp_tunnel::McpTunnel> {
        &self.tunnel
    }

    fn store(&self) -> CredentialStore {
        CredentialStore::at(self.directory.join(KEY_FILE_NAME))
    }

    /// The key as text, when one exists.
    #[must_use]
    pub fn key(&self) -> Option<String> {
        self.key
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .as_ref()
            .map(|key| key.expose().to_owned())
    }

    /// Creates the key if there is none, and returns it.
    ///
    /// # Errors
    ///
    /// Returns a sentence when the key cannot be created or stored.
    pub fn ensure_key(&self) -> Result<String, String> {
        if let Some(key) = self.key() {
            return Ok(key);
        }
        let created = self
            .store()
            .load_or_create(&self.directory)
            .map_err(|error| format!("the key could not be created: {error}"))?;
        let text = created.expose().to_owned();
        *self
            .key
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(created);
        Ok(text)
    }

    /// Replaces the key. The old one stops working at once.
    ///
    /// # Errors
    ///
    /// Returns a sentence when the new key cannot be stored.
    pub fn regenerate(&self) -> Result<String, String> {
        let path = self.directory.join(KEY_FILE_NAME);
        match std::fs::remove_file(&path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(format!("the old key could not be removed: {error}")),
        }
        *self
            .key
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = None;
        self.ensure_key()
    }

    /// Whether a presented key is the current one. Constant-time; false when there is no key.
    #[must_use]
    pub fn matches(&self, presented: &str) -> bool {
        self.key
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .as_ref()
            .is_some_and(|key| key.matches(presented))
    }

    /// Records the port the endpoint is serving on in this daemon's life.
    pub fn set_serving_port(&self, port: u16) {
        *self
            .serving_port
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(port);
    }

    /// The port the endpoint is serving on now, if it is.
    #[must_use]
    pub fn serving_port(&self) -> Option<u16> {
        *self
            .serving_port
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

/// Requires the key, then tells the endpoint's admission who is calling.
///
/// A request without a valid `Authorization: Bearer <key>` never reaches the endpoint. A valid one has the header **replaced** by the fixed caller
/// fingerprint, so what the endpoint compares is never what the client sent: a client cannot present the fingerprint to get in, because the key is
/// checked first.
pub async fn require_key(
    State(access): State<Arc<McpAccess>>,
    mut request: Request,
    next: Next,
) -> Response {
    let presented = request
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        // A client may send the key as `Bearer KEY` or, as some products do with a "secret token" field, as the bare value.
        .map(|value| value.strip_prefix("Bearer ").unwrap_or(value));
    let admitted = presented.is_some_and(|key| access.matches(key));
    if !admitted {
        let mut response = (
            StatusCode::UNAUTHORIZED,
            Json(json!({
                "jsonrpc": "2.0",
                "id": null,
                "error": { "code": -32001, "message": "this needs JARVIS's access key: send it as `Authorization: Bearer <key>`" }
            })),
        )
            .into_response();
        response.headers_mut().insert(
            header::WWW_AUTHENTICATE,
            HeaderValue::from_static("Bearer realm=\"JARVIS\""),
        );
        return response;
    }
    if let Ok(value) = HeaderValue::from_str(&format!("Bearer {ACCESS_FINGERPRINT}")) {
        request.headers_mut().insert(header::AUTHORIZATION, value);
    }
    // A request that came through the owner's named tunnel carries the owner's public hostname as its Host. The key has been checked, so the
    // endpoint is told what it always sees (its own loopback address); the endpoint's own Host guard is not loosened for anyone else.
    let through_named_tunnel = request
        .headers()
        .get(header::HOST)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|host| access.tunnel().is_public_host(host));
    if through_named_tunnel
        && let Some(port) = access.serving_port()
        && let Ok(value) = HeaderValue::from_str(&format!("127.0.0.1:{port}"))
    {
        request.headers_mut().insert(header::HOST, value);
    }
    next.run(request).await
}

/// The routes of this module, under `/api/v1`.
pub fn routes() -> axum::Router<GatewayState> {
    use axum::routing::{get, post};
    axum::Router::new()
        .route("/mcp/access", get(overview).post(switch))
        .route("/mcp/access/key", post(new_key))
        .route("/mcp/access/public", post(set_public))
        .route(
            "/mcp/access/public/named",
            post(set_named).delete(clear_named),
        )
}

fn access(state: &GatewayState) -> Result<&Arc<McpAccess>, Box<Response>> {
    state.mcp_access().ok_or_else(|| {
        Box::new(error_response(
            StatusCode::NOT_FOUND,
            ErrorCode::UnavailableCapability,
            "letting other apps connect is not available on this daemon",
        ))
    })
}

fn configured_port(state: &GatewayState) -> Option<u16> {
    let context = context(state).ok()?;
    settings::get(context.paths(), "daemon.mcp_serve_port")
        .ok()
        .and_then(|value| value.trim().parse().ok())
}

fn overview_json(state: &GatewayState, access: &McpAccess) -> serde_json::Value {
    let configured = configured_port(state);
    let serving = access.serving_port();
    let port = configured.or(serving);
    json!({
        "enabled": configured.is_some(),
        "running": serving.is_some(),
        "port": port,
        "address": port.map(|port| format!("http://127.0.0.1:{port}{ENDPOINT_PATH}")),
        // The key is the owner's to copy into the other app, so unlike a provider key it is shown. It reaches only this page, which holds the console credential.
        "key": if configured.is_some() { access.key() } else { None },
        "restart_needed": configured != serving,
        "public": public_json(access),
    })
}

/// What is true of reaching the endpoint from the internet.
fn public_json(access: &McpAccess) -> serde_json::Value {
    let tunnel = access.tunnel();
    let status = tunnel.status();
    let url = match &status {
        crate::mcp_tunnel::Tunnel::On { url } => Some(format!("{url}{ENDPOINT_PATH}")),
        _ => None,
    };
    let stable_host = tunnel.stable_host();
    json!({
        "wanted": tunnel.wanted(),
        "available": tunnel.available(),
        "download_megabytes": tunnel.download_megabytes(),
        "status": status,
        "url": url,
        // The owner's own Cloudflare tunnel (`ADR-0202`). The token is never shown; only that there is one, and the name it serves.
        "stable": {
            "configured": stable_host.is_some(),
            "host": stable_host,
            "service_url": access.serving_port().map(|port| format!("http://127.0.0.1:{port}")),
        },
    })
}

/// The body of `POST /api/v1/mcp/access/public/named`.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NamedBody {
    hostname: String,
    token: String,
}

/// `POST /api/v1/mcp/access/public/named`: save the owner's own Cloudflare tunnel (its token and the hostname published for it) and use it in
/// place of the quick tunnel. When the endpoint is being served this also switches the internet address on, and a running tunnel restarts at once.
pub async fn set_named(State(state): State<GatewayState>, Json(body): Json<NamedBody>) -> Response {
    let access = match access(&state) {
        Ok(access) => access,
        Err(response) => return *response,
    };
    let tunnel = access.tunnel();
    if let Err(message) = tunnel.set_named(&body.hostname, &body.token) {
        return refused(&message);
    }
    if let Some(port) = access.serving_port() {
        if let Err(message) = tunnel.set_wanted(true) {
            return refused(&message);
        }
        tunnel.start(port);
    }
    (StatusCode::OK, Json(overview_json(&state, access))).into_response()
}

/// `DELETE /api/v1/mcp/access/public/named`: forget the owner's tunnel. A running tunnel goes back to the quick tunnel at once.
pub async fn clear_named(State(state): State<GatewayState>) -> Response {
    let access = match access(&state) {
        Ok(access) => access,
        Err(response) => return *response,
    };
    if let Err(message) = access.tunnel().clear_named() {
        return refused(&message);
    }
    (StatusCode::OK, Json(overview_json(&state, access))).into_response()
}

/// The body of `POST /api/v1/mcp/access/public`.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublicBody {
    enabled: bool,
}

/// `POST /api/v1/mcp/access/public`: make the endpoint reachable from the internet, or stop doing so. Applies at once.
pub async fn set_public(
    State(state): State<GatewayState>,
    Json(body): Json<PublicBody>,
) -> Response {
    let access = match access(&state) {
        Ok(access) => access,
        Err(response) => return *response,
    };
    let tunnel = access.tunnel();
    if body.enabled {
        let Some(port) = access.serving_port() else {
            return refused(
                "switch on Connect apps and restart first; the internet address points at that",
            );
        };
        if !tunnel.available() {
            return refused(
                "this computer needs Cloudflare's free `cloudflared` program: install it (macOS: `brew install cloudflared`) and try again",
            );
        }
        if let Err(message) = tunnel.set_wanted(true) {
            return refused(&message);
        }
        tunnel.start(port);
    } else {
        if let Err(message) = tunnel.set_wanted(false) {
            return refused(&message);
        }
        tunnel.stop();
    }
    (StatusCode::OK, Json(overview_json(&state, access))).into_response()
}

/// `GET /api/v1/mcp/access`: whether other apps may connect, where, and the key.
pub async fn overview(State(state): State<GatewayState>) -> Response {
    let access = match access(&state) {
        Ok(access) => access,
        Err(response) => return *response,
    };
    (StatusCode::OK, Json(overview_json(&state, access))).into_response()
}

/// The body of `POST /api/v1/mcp/access`.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SwitchBody {
    enabled: bool,
}

/// `POST /api/v1/mcp/access`: switch it on (making the key) or off. Applied on the next start.
pub async fn switch(State(state): State<GatewayState>, Json(body): Json<SwitchBody>) -> Response {
    let access = match access(&state) {
        Ok(access) => access,
        Err(response) => return *response,
    };
    let context = match context(&state) {
        Ok(context) => context,
        Err(response) => return response,
    };
    if body.enabled {
        if let Err(message) = access.ensure_key() {
            return error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                ErrorCode::Internal,
                &message,
            );
        }
        if configured_port(&state).is_none()
            && let Err(message) = settings::set(
                context.paths(),
                "daemon.mcp_serve_port",
                // Another profile, or another program, may already hold the usual port; take the next free one rather than fail at start.
                &[first_free_port(DEFAULT_PORT).to_string()],
            )
        {
            return refused(&message);
        }
    } else if let Err(message) = settings::unset(context.paths(), "daemon.mcp_serve_port") {
        return refused(&message);
    }
    (StatusCode::OK, Json(overview_json(&state, access))).into_response()
}

/// `POST /api/v1/mcp/access/key`: make a new key. The old one stops working at once.
pub async fn new_key(State(state): State<GatewayState>) -> Response {
    let access = match access(&state) {
        Ok(access) => access,
        Err(response) => return *response,
    };
    match access.regenerate() {
        Ok(_) => (StatusCode::OK, Json(overview_json(&state, access))).into_response(),
        Err(message) => error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            ErrorCode::Internal,
            &message,
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{Router, body::Body, middleware, routing::post};
    use tower::ServiceExt;

    fn scratch() -> PathBuf {
        let path =
            std::env::temp_dir().join(format!("jarvis-access-{}", jarvis_core::scratch_tag()));
        std::fs::create_dir_all(&path).unwrap_or_else(|e| panic!("{e}"));
        path
    }

    /// A stand-in endpoint that reports the `Authorization` it was handed, so the test can see what the real one would see.
    fn app(access: Arc<McpAccess>) -> Router {
        Router::new()
            .route(
                "/mcp",
                post(|request: Request| async move {
                    request
                        .headers()
                        .get(header::AUTHORIZATION)
                        .and_then(|value| value.to_str().ok())
                        .unwrap_or("none")
                        .to_owned()
                }),
            )
            .layer(middleware::from_fn_with_state(access, require_key))
    }

    async fn call(app: &Router, authorization: Option<&str>) -> (StatusCode, String) {
        let mut request = axum::http::Request::post("/mcp");
        if let Some(value) = authorization {
            request = request.header(header::AUTHORIZATION, value);
        }
        let response = app
            .clone()
            .oneshot(
                request
                    .body(Body::empty())
                    .unwrap_or_else(|e| panic!("{e}")),
            )
            .await
            .unwrap_or_else(|e| panic!("{e}"));
        let status = response.status();
        let bytes = axum::body::to_bytes(response.into_body(), 4096)
            .await
            .unwrap_or_else(|e| panic!("{e}"));
        (status, String::from_utf8_lossy(&bytes).into_owned())
    }

    /// **Fails closed**: no key, a wrong key, the endpoint's own fixed fingerprint, and a padded or extended key all get a 401. The exact key is accepted with or without the `Bearer` word.
    #[tokio::test]
    async fn only_the_exact_key_gets_in() {
        let directory = scratch();
        let access = Arc::new(McpAccess::open(&directory));
        let key = access.ensure_key().unwrap_or_else(|e| panic!("{e}"));
        let app = app(Arc::clone(&access));

        assert_eq!(call(&app, None).await.0, StatusCode::UNAUTHORIZED);
        assert_eq!(
            call(&app, Some("Bearer wrong")).await.0,
            StatusCode::UNAUTHORIZED
        );
        assert_eq!(
            call(&app, Some(&format!("Bearer {ACCESS_FINGERPRINT}")))
                .await
                .0,
            StatusCode::UNAUTHORIZED,
            "knowing the fixed caller admits nothing"
        );
        assert_eq!(
            call(&app, Some(&key)).await.0,
            StatusCode::OK,
            "some products send a secret token as the bare header value, so the exact key is accepted that way too"
        );
        assert_eq!(
            call(&app, Some(&format!("{key}x"))).await.0,
            StatusCode::UNAUTHORIZED,
            "a bare value that is not exactly the key is refused"
        );
        assert_eq!(
            call(&app, Some(&format!("Bearer {key} "))).await.0,
            StatusCode::UNAUTHORIZED,
            "padding is not tolerated"
        );

        let (status, seen) = call(&app, Some(&format!("Bearer {key}"))).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(
            seen,
            format!("Bearer {ACCESS_FINGERPRINT}"),
            "the endpoint sees the fixed caller, never the key"
        );
        let _ = std::fs::remove_dir_all(&directory);
    }

    /// With no key at all (the feature never switched on) nothing gets in, not even an empty credential.
    #[tokio::test]
    async fn with_no_key_nobody_gets_in() {
        let directory = scratch();
        let access = Arc::new(McpAccess::open(&directory));
        assert!(access.key().is_none());
        let app = app(access);
        assert_eq!(
            call(&app, Some("Bearer ")).await.0,
            StatusCode::UNAUTHORIZED
        );
        assert_eq!(
            call(&app, Some("Bearer x")).await.0,
            StatusCode::UNAUTHORIZED
        );
        let _ = std::fs::remove_dir_all(&directory);
    }

    /// A new key works at once and the old one stops, with no restart; a restart keeps the new one.
    #[tokio::test]
    async fn a_new_key_replaces_the_old_one_immediately() {
        let directory = scratch();
        let access = Arc::new(McpAccess::open(&directory));
        let old = access.ensure_key().unwrap_or_else(|e| panic!("{e}"));
        let app = app(Arc::clone(&access));
        assert_eq!(
            call(&app, Some(&format!("Bearer {old}"))).await.0,
            StatusCode::OK
        );

        let new = access.regenerate().unwrap_or_else(|e| panic!("{e}"));
        assert_ne!(old, new);
        assert_eq!(
            call(&app, Some(&format!("Bearer {old}"))).await.0,
            StatusCode::UNAUTHORIZED
        );
        assert_eq!(
            call(&app, Some(&format!("Bearer {new}"))).await.0,
            StatusCode::OK
        );

        let reopened = McpAccess::open(&directory);
        assert_eq!(
            reopened.key().as_deref(),
            Some(new.as_str()),
            "the new key is what a restart finds"
        );
        let _ = std::fs::remove_dir_all(&directory);
    }

    #[test]
    fn a_port_that_is_taken_is_skipped_for_the_next_free_one() {
        let held = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap_or_else(|e| panic!("{e}"));
        let taken = held.local_addr().unwrap_or_else(|e| panic!("{e}")).port();
        let chosen = first_free_port(taken);
        assert_ne!(chosen, taken, "the held port must not be offered");
        assert!(chosen > taken);
        // A free port is taken as asked.
        drop(held);
        assert_eq!(first_free_port(taken), taken);
    }

    #[test]
    fn the_fixed_caller_is_a_valid_fingerprint() {
        assert!(jarvis_mcp_transport::Fingerprint::parse(ACCESS_FINGERPRINT).is_ok());
    }

    fn host_app(access: Arc<McpAccess>) -> Router {
        Router::new()
            .route(
                "/mcp",
                post(|request: Request| async move {
                    request
                        .headers()
                        .get(header::HOST)
                        .and_then(|value| value.to_str().ok())
                        .unwrap_or("none")
                        .to_owned()
                }),
            )
            .layer(middleware::from_fn_with_state(access, require_key))
    }

    async fn call_with_host(app: &Router, key: Option<&str>, host: &str) -> (StatusCode, String) {
        let mut request = axum::http::Request::post("/mcp").header(header::HOST, host);
        if let Some(key) = key {
            request = request.header(header::AUTHORIZATION, key);
        }
        let response = app
            .clone()
            .oneshot(
                request
                    .body(Body::empty())
                    .unwrap_or_else(|e| panic!("{e}")),
            )
            .await
            .unwrap_or_else(|e| panic!("{e}"));
        let status = response.status();
        let bytes = axum::body::to_bytes(response.into_body(), 4096)
            .await
            .unwrap_or_else(|e| panic!("{e}"));
        (status, String::from_utf8_lossy(&bytes).into_owned())
    }

    /// Falsified by rewriting before the key check (a stranger would be answered), by rewriting every Host, or by not rewriting at all (the
    /// endpoint's own Host guard would refuse the owner's public name).
    #[tokio::test]
    async fn the_owners_public_name_reaches_the_endpoint_as_loopback_only_with_the_key() {
        let directory = scratch();
        let access = Arc::new(McpAccess::open(&directory));
        let key = access.ensure_key().unwrap_or_else(|e| panic!("{e}"));
        access.set_serving_port(8766);
        let token = {
            use base64::Engine as _;
            base64::engine::general_purpose::STANDARD
                .encode(br#"{"a":"account","t":"tunnel-id","s":"c2VjcmV0"}"#)
        };
        access
            .tunnel()
            .set_named("jarvis.example.com", &token)
            .unwrap_or_else(|e| panic!("{e}"));
        let app = host_app(Arc::clone(&access));

        let (status, seen) = call_with_host(&app, Some(&key), "jarvis.example.com").await;
        assert_eq!((status, seen.as_str()), (StatusCode::OK, "127.0.0.1:8766"));
        let (_, seen) = call_with_host(&app, Some(&key), "Jarvis.Example.com:443").await;
        assert_eq!(seen, "127.0.0.1:8766");
        for other in [
            "evil.example.com",
            "jarvis.example.com.evil.test",
            "localhost",
        ] {
            let (status, seen) = call_with_host(&app, Some(&key), other).await;
            assert_eq!(status, StatusCode::OK);
            assert_eq!(seen, other, "only the owner's own name is rewritten");
        }
        let (status, seen) = call_with_host(&app, None, "jarvis.example.com").await;
        assert_eq!(
            status,
            StatusCode::UNAUTHORIZED,
            "no key, no answer: {seen}"
        );
        let (status, _) = call_with_host(&app, Some("wrong"), "jarvis.example.com").await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);

        access
            .tunnel()
            .clear_named()
            .unwrap_or_else(|e| panic!("{e}"));
        let (_, seen) = call_with_host(&app, Some(&key), "jarvis.example.com").await;
        assert_eq!(
            seen, "jarvis.example.com",
            "forgotten, the name means nothing"
        );
        let _ = std::fs::remove_dir_all(&directory);
    }
}
