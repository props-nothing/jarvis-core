use super::*;

use std::sync::{Arc, Mutex};

use axum::{Json, extract::State, http::HeaderMap, routing::post};
use jarvis_mcp_transport::{HttpCredentialProvider, McpHttpEndpoint, ToolBuffer};
use serde_json::{Value, json};

#[derive(Clone, Default)]
struct Recorded(Arc<Mutex<Vec<String>>>);

impl Recorded {
    fn tokens(&self) -> Vec<String> {
        self.0
            .lock()
            .unwrap_or_else(|error| panic!("requests: {error}"))
            .clone()
    }
}

struct HttpFixture {
    endpoint: String,
    recorded: Recorded,
    task: tokio::task::JoinHandle<()>,
}

impl HttpFixture {
    async fn start() -> Self {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .unwrap_or_else(|error| panic!("listener: {error}"));
        let address = listener
            .local_addr()
            .unwrap_or_else(|error| panic!("address: {error}"));
        let recorded = Recorded::default();
        let router = axum::Router::new()
            .route("/mcp", post(answer))
            .with_state(recorded.clone());
        let task = tokio::spawn(async move {
            axum::serve(listener, router)
                .await
                .unwrap_or_else(|error| panic!("serve: {error}"));
        });
        Self {
            endpoint: format!("http://{address}/mcp"),
            recorded,
            task,
        }
    }

    fn document(&self) -> String {
        format!(
            "[[servers]]\nname='remote'\n[servers.transport]\nkind='http'\nendpoint='{}'\nauthorization='mcp:remote:token'\n",
            self.endpoint
        )
    }
}

impl Drop for HttpFixture {
    fn drop(&mut self) {
        self.task.abort();
    }
}

async fn answer(
    State(recorded): State<Recorded>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Json<Value> {
    let token = headers
        .get("authorization")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .unwrap_or_default();
    recorded
        .0
        .lock()
        .unwrap_or_else(|error| panic!("record: {error}"))
        .push(token.to_owned());
    assert!(
        !token.is_empty(),
        "authenticated requests must not fall back to anonymous access"
    );
    let result = match body["method"].as_str().unwrap_or_default() {
        "server/discover" => json!({
            "resultType": "complete", "supportedVersions": ["2026-07-28"],
            "capabilities": {"tools": {}}, "ttlMs": 0, "cacheScope": "private",
            "_meta": {"io.modelcontextprotocol/serverInfo": {"name": token, "version": "1.0.0"}}
        }),
        "tools/list" => json!({
            "resultType": "complete", "tools": [{"name": "search", "description": token,
                "inputSchema": {"type": "object", "properties": {}}}], "ttlMs": 0, "cacheScope": "private"
        }),
        "tools/call" => json!({
            "resultType": "complete", "content": [{"type": "text", "text": recorded.tokens().join(" ")}],
            "structuredContent": {"echo": token}
        }),
        _ => json!({}),
    };
    Json(json!({"jsonrpc": "2.0", "id": body["id"], "result": result}))
}

struct ProfileScratch(std::path::PathBuf);

impl Drop for ProfileScratch {
    fn drop(&mut self) {
        jarvis_core::remove_scratch_dir(&self.0);
    }
}

fn profile() -> (ProfileScratch, AppPaths) {
    let root = std::env::temp_dir().join(format!("jmcp-http-{}", jarvis_core::scratch_tag()));
    std::fs::create_dir_all(&root).unwrap_or_else(|error| panic!("scratch: {error}"));
    let root =
        std::fs::canonicalize(root).unwrap_or_else(|error| panic!("canonical scratch: {error}"));
    let paths = jarvis_storage::portable_layout(&root).unwrap_or_else(|| panic!("layout"));
    (ProfileScratch(root), paths)
}

#[tokio::test]
async fn named_http_profile_binding_resolves_only_its_server_and_never_uses_another_profile() {
    let (_scratch, paths) = profile();
    let (_other_scratch, other_paths) = profile();
    let fixture = HttpFixture::start().await;
    let reference = McpSecretRef::parse("mcp:remote:token")
        .unwrap_or_else(|error| panic!("reference: {error}"));
    mcp_secrets::set(&paths, &reference, "synthetic-profile-one")
        .unwrap_or_else(|error| panic!("set: {error}"));
    let provider = ProfileHttpSecrets::new(&paths);
    assert_eq!(
        provider
            .token(reference.as_str(), "remote", &fixture.endpoint)
            .await
            .ok()
            .as_deref(),
        Some("synthetic-profile-one")
    );
    assert!(
        ProfileHttpSecrets::new(&other_paths)
            .token(reference.as_str(), "remote", &fixture.endpoint)
            .await
            .is_err()
    );
    for (server, endpoint, binding) in [
        ("other", fixture.endpoint.clone(), "mcp:remote:token"),
        ("remote", fixture.endpoint.clone(), "mcp:other:token"),
        (
            "remote",
            format!("{}?token=secret", fixture.endpoint),
            "mcp:remote:token",
        ),
        (
            "remote",
            "http://remote.example/mcp".to_owned(),
            "mcp:remote:token",
        ),
        (
            "remote",
            fixture.endpoint.clone(),
            "synthetic-literal-value",
        ),
        ("remote", fixture.endpoint.clone(), "github"),
    ] {
        assert!(provider.token(binding, server, &endpoint).await.is_err());
    }
    assert!(
        fixture.recorded.tokens().is_empty(),
        "resolution does not itself contact a provider"
    );
}

#[tokio::test]
async fn named_http_credentials_reach_host_and_test_without_secret_or_permission_changes() {
    let (_scratch, paths) = profile();
    let fixture = HttpFixture::start().await;
    let reference = McpSecretRef::parse("mcp:remote:token")
        .unwrap_or_else(|error| panic!("reference: {error}"));
    let document = fixture.document();
    assert_eq!(
        http_state(&paths, "remote", &fixture.endpoint, reference.as_str()).unwrap_or_default()["state"],
        "not set"
    );
    assert_eq!(
        crate::mcp_host::try_connect(&document, &paths).await["ok"],
        false
    );
    assert!(
        fixture.recorded.tokens().is_empty(),
        "an unset key must send nothing"
    );
    mcp_secrets::set(&paths, &reference, "synthetic-first-token")
        .unwrap_or_else(|error| panic!("set: {error}"));
    let state =
        http_state(&paths, "remote", &fixture.endpoint, reference.as_str()).unwrap_or_default();
    assert_eq!(state["state"], "set");
    assert_eq!(state["reference"], reference.as_str());
    assert!(!state.to_string().contains("synthetic-first-token"));
    assert!(
        !state
            .to_string()
            .contains(&paths.config().display().to_string())
    );
    let tested = crate::mcp_host::try_connect(&document, &paths).await;
    assert_eq!(tested["ok"], true, "{tested}");
    assert!(!tested.to_string().contains("synthetic-first-token"));
    let host = crate::mcp_host::compose_host_with_paths(Some(&document), &paths)
        .await
        .unwrap_or_else(|error| panic!("host: {error}"))
        .unwrap_or_else(|| panic!("host missing"));
    assert_eq!(host.reachable_servers(), 1);
    assert_eq!(host.definitions().len(), 1);
    assert!(
        host.definitions()
            .iter()
            .all(|definition| definition.approval() == jarvis_tools::ApprovalPolicy::Ask)
    );
    assert!(host.close().await.is_ok());
}

#[tokio::test]
async fn named_http_credentials_rotate_and_remove_on_an_existing_connection() {
    let (_scratch, paths) = profile();
    let fixture = HttpFixture::start().await;
    let reference = McpSecretRef::parse("mcp:remote:token")
        .unwrap_or_else(|error| panic!("reference: {error}"));
    let document = fixture.document();
    mcp_secrets::set(&paths, &reference, "synthetic-first-token")
        .unwrap_or_else(|error| panic!("set: {error}"));
    let server =
        jarvis_mcp::ServerName::new("remote").unwrap_or_else(|error| panic!("server: {error}"));
    let endpoint = McpHttpEndpoint::parse(&fixture.endpoint)
        .unwrap_or_else(|error| panic!("endpoint: {error}"));
    let connection = jarvis_mcp_transport::connect_http_with_auth(
        &server,
        &endpoint,
        reference.as_str(),
        Arc::new(ProfileHttpSecrets::new(&paths)),
    )
    .await
    .unwrap_or_else(|error| panic!("connect: {error}"));
    let mut buffer = ToolBuffer::new();
    assert_eq!(
        connection
            .list_tools(&mut buffer)
            .await
            .unwrap_or_else(|error| panic!("list: {error}"))
            .len(),
        1
    );
    mcp_secrets::set(&paths, &reference, "synthetic-rotated-token")
        .unwrap_or_else(|error| panic!("rotate: {error}"));
    let result = connection
        .call_tool("search", &serde_json::Map::new())
        .await
        .unwrap_or_else(|error| panic!("call: {error}"));
    assert!(result.is_success());
    assert!(!result.text.contains("synthetic-first-token"));
    assert!(!result.text.contains("synthetic-rotated-token"));
    assert!(result.text.contains("[REDACTED]"));
    assert!(
        result
            .structured
            .as_ref()
            .is_some_and(|value| !value.to_string().contains("synthetic-rotated-token"))
    );
    assert_eq!(
        fixture.recorded.tokens().last().map(String::as_str),
        Some("synthetic-rotated-token")
    );
    mcp_secrets::set(&paths, &reference, r#"{"not":"a-bearer-token"}"#)
        .unwrap_or_else(|error| panic!("malformed set: {error}"));
    let before = fixture.recorded.tokens().len();
    assert!(
        connection
            .call_tool("search", &serde_json::Map::new())
            .await
            .is_err()
    );
    assert_eq!(
        fixture.recorded.tokens().len(),
        before,
        "a malformed token must send nothing"
    );
    mcp_secrets::remove(&paths, &reference).unwrap_or_else(|error| panic!("remove: {error}"));
    assert!(
        connection
            .call_tool("search", &serde_json::Map::new())
            .await
            .is_err()
    );
    assert_eq!(
        fixture.recorded.tokens().len(),
        before,
        "a removed key must send nothing"
    );
    assert!(connection.close().await.is_ok());
    assert_eq!(
        crate::mcp_host::try_connect(&document, &paths).await["ok"],
        false
    );
    assert_eq!(fixture.recorded.tokens().len(), before);
}

#[tokio::test]
#[ignore = "owner opt-in: MCP_LIVE_HTTP_ENDPOINT and MCP_LIVE_HTTP_TOKEN_FILE; authenticated HTTPS discovery only"]
async fn live_named_http_token_discovers_an_owner_selected_server_without_running_tools() {
    use std::io::Read;

    let endpoint = std::env::var("MCP_LIVE_HTTP_ENDPOINT")
        .unwrap_or_else(|_| panic!("set MCP_LIVE_HTTP_ENDPOINT"));
    assert!(
        endpoint.starts_with("https://"),
        "live smoke requires an explicitly selected HTTPS server"
    );
    let key_file = std::env::var_os("MCP_LIVE_HTTP_TOKEN_FILE")
        .unwrap_or_else(|| panic!("set MCP_LIVE_HTTP_TOKEN_FILE to a private file"));
    let mut raw = String::new();
    std::fs::File::open(key_file)
        .unwrap_or_else(|_| panic!("cannot open token file"))
        .take(4097)
        .read_to_string(&mut raw)
        .unwrap_or_else(|_| panic!("cannot read token file"));
    let (_scratch, paths) = profile();
    let reference = McpSecretRef::parse("mcp:remote:token").unwrap_or_else(|_| panic!("reference"));
    mcp_secrets::set(&paths, &reference, &raw)
        .unwrap_or_else(|_| panic!("token file is not usable"));
    let request = crate::mcp_admin::ServerBody {
        name: "remote".to_owned(),
        command: Vec::new(),
        endpoint: Some(endpoint),
        read_only: false,
        read_only_tools: Vec::new(),
        env: std::collections::BTreeMap::new(),
        authorization: Some(reference.as_str().to_owned()),
    };
    let document =
        crate::mcp_admin::add_to_document(None, &request, &|_| Err("no local program".to_owned()))
            .unwrap_or_else(|_| panic!("invalid HTTP credential binding"));
    let tested = crate::mcp_host::try_connect(&document, &paths).await;
    assert_eq!(
        tested["ok"], true,
        "owner-selected HTTP MCP discovery failed"
    );
    assert!(
        !tested.to_string().contains(raw.trim()),
        "credential appeared in discovery metadata"
    );
}
