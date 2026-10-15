use axum::{
    body::{Body, to_bytes},
    http::{Request, header},
};
use jarvis_core::ClientCredential;
use jarvis_storage::SqliteDatabase;
use tower::ServiceExt;

use super::*;

struct Scratch(std::path::PathBuf);
impl Drop for Scratch {
    fn drop(&mut self) {
        jarvis_core::remove_scratch_dir(&self.0);
    }
}

async fn fixture() -> (Scratch, GatewayState, jarvis_storage::AppPaths, String) {
    let root = std::env::temp_dir().join(format!("jgh-settings-{}", jarvis_core::scratch_tag()));
    std::fs::create_dir_all(&root).unwrap_or_else(|error| panic!("scratch: {error}"));
    let paths = jarvis_storage::portable_layout(&root).unwrap_or_else(|| panic!("layout"));
    std::fs::create_dir_all(paths.config()).unwrap_or_else(|error| panic!("config: {error}"));
    let database = Arc::new(
        SqliteDatabase::open(&root.join("jarvis.sqlite3"))
            .await
            .unwrap_or_else(|error| panic!("database: {error}")),
    );
    let credential =
        ClientCredential::generate().unwrap_or_else(|error| panic!("credential: {error}"));
    let token = credential.expose().to_owned();
    let account = Arc::new(
        GithubAccount::new(paths.clone()).unwrap_or_else(|error| panic!("account: {error}")),
    );
    let state = GatewayState::new(database, credential)
        .with_settings(crate::settings_service::SettingsContext::new(
            paths.clone(),
            Some(root.clone()),
        ))
        .with_github(account);
    (Scratch(root), state, paths, token)
}

async fn request(
    state: &GatewayState,
    method: &str,
    path: &str,
    token: Option<&str>,
    body: serde_json::Value,
) -> (StatusCode, String) {
    let mut request = Request::builder()
        .method(method)
        .uri(path)
        .header(header::CONTENT_TYPE, "application/json");
    if let Some(token) = token {
        request = request.header(header::AUTHORIZATION, format!("Bearer {token}"));
    }
    let response = crate::gateway::router(state.clone())
        .oneshot(
            request
                .body(Body::from(body.to_string()))
                .unwrap_or_else(|error| panic!("request: {error}")),
        )
        .await
        .unwrap_or_else(|error| panic!("route: {error}"));
    let status = response.status();
    let bytes = to_bytes(response.into_body(), 16_384)
        .await
        .unwrap_or_else(|error| panic!("body: {error}"));
    (status, String::from_utf8_lossy(&bytes).into_owned())
}

#[tokio::test]
async fn setup_status_and_mutations_require_owner_auth_and_never_return_the_secret() {
    let (_scratch, state, paths, credential) = fixture().await;
    for (method, route) in [
        ("GET", "/api/v1/github"),
        ("PUT", "/api/v1/github/app"),
        ("POST", "/api/v1/github/connect"),
        ("POST", "/api/v1/github/disconnect"),
        ("POST", "/api/v1/github/test"),
    ] {
        assert_eq!(
            request(&state, method, route, None, json!({})).await.0,
            StatusCode::UNAUTHORIZED
        );
    }
    let secret = "synthetic-github-app-secret-87614";
    let (status, reply) = request(&state, "PUT", "/api/v1/github/app", Some(&credential), json!({ "client_id": "fixtureclient", "client_secret": secret, "private_repositories": false })).await;
    assert_eq!(status, StatusCode::OK, "{reply}");
    assert!(!reply.contains(secret));
    assert!(!reply.contains(&paths.config().display().to_string()));
    let status: serde_json::Value =
        serde_json::from_str(&reply).unwrap_or_else(|error| panic!("JSON: {error}"));
    assert_eq!(status["configured"], true);
    assert_eq!(status["connected"], false);
    assert_eq!(status["client_secret_set"], true);
    assert_eq!(
        status["callback_url"],
        "http://127.0.0.1:8765/oauth/github/callback"
    );
    let (status, tested) = request(
        &state,
        "POST",
        "/api/v1/github/test",
        Some(&credential),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(!tested.contains(secret));
}

#[tokio::test]
async fn sign_in_uses_the_live_listener_port_instead_of_unapplied_saved_defaults() {
    let (_scratch, state, _paths, credential) = fixture().await;
    let state = state.with_http_port(48291);
    let (_, status) = request(
        &state,
        "GET",
        "/api/v1/github",
        Some(&credential),
        json!({}),
    )
    .await;
    assert!(status.contains("http://127.0.0.1:48291/oauth/github/callback"));
    let (saved, _) = request(&state, "PUT", "/api/v1/github/app", Some(&credential), json!({ "client_id": "fixtureclient", "client_secret": "synthetic-app-port-secret-4216", "private_repositories": false })).await;
    assert_eq!(saved, StatusCode::OK);
    let (started, body) = request(
        &state,
        "POST",
        "/api/v1/github/connect",
        Some(&credential),
        json!({}),
    )
    .await;
    assert_eq!(started, StatusCode::OK);
    let body: serde_json::Value =
        serde_json::from_str(&body).unwrap_or_else(|error| panic!("JSON: {error}"));
    let url = reqwest::Url::parse(body["auth_url"].as_str().unwrap_or_default())
        .unwrap_or_else(|error| panic!("URL: {error}"));
    assert!(url.query_pairs().any(|(name, value)| name == "redirect_uri"
        && value == "http://127.0.0.1:48291/oauth/github/callback"));
}

#[tokio::test]
async fn only_the_get_callback_is_public_and_a_forgery_is_inert() {
    let (_scratch, state, _paths, credential) = fixture().await;
    let (status, body) = request(
        &state,
        "GET",
        "/oauth/github/callback?state=forged-canary&code=forged-code-canary",
        None,
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(!body.contains("forged-canary") && !body.contains("forged-code-canary"));
    assert_eq!(
        request(&state, "POST", "/oauth/github/callback", None, json!({}))
            .await
            .0,
        StatusCode::UNAUTHORIZED
    );
    let (_, body) = request(
        &state,
        "GET",
        "/api/v1/github",
        Some(&credential),
        json!({}),
    )
    .await;
    assert!(!body.contains("forged-code-canary"));
}

#[tokio::test]
async fn connect_seeds_only_the_fixed_read_only_server_and_never_overwrites_another_program() {
    let (_scratch, state, paths, credential) = fixture().await;
    let (status, _) = request(&state, "PUT", "/api/v1/github/app", Some(&credential), json!({ "client_id": "fixtureclient", "client_secret": "synthetic-app-secret-68194", "private_repositories": false })).await;
    assert_eq!(status, StatusCode::OK);
    let (status, body) = request(
        &state,
        "POST",
        "/api/v1/github/connect",
        Some(&credential),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(body.contains("https://github.com/login/oauth/authorize?"));
    assert!(body.contains("code_challenge") && !body.contains("synthetic-app-secret-68194"));
    let document = crate::mcp_host::load_document(paths.config())
        .unwrap_or_else(|error| panic!("document: {error}"))
        .unwrap_or_else(|| panic!("configured server"));
    let server = crate::mcp_admin::configured_servers(Some(&document)).remove(0);
    assert_eq!(server.endpoint.as_deref(), Some(MCP_ENDPOINT));
    assert_eq!(server.authorization.as_deref(), Some("github"));
    assert_eq!(server.posture, "read-only");
    assert!(server.env.is_empty());
    assert_eq!(
        crate::mcp_admin::github_document(Some(&document))
            .unwrap_or_else(|error| panic!("idempotent: {error}")),
        document
    );
    let unrelated =
        "[[servers]]\nname='github'\n[servers.transport]\nkind='stdio'\nprogram='owner-program'\n";
    assert!(crate::mcp_admin::github_document(Some(unrelated)).is_err());
}
