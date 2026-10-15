use super::*;

use std::collections::BTreeMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Duration;

use futures_util::StreamExt;
use jarvis_mcp::ServerName;
use rmcp::model::{ClientRequest, PingRequest, RequestId};
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};

#[derive(Default)]
struct ChangingProvider {
    issued: AtomicUsize,
    denied: AtomicBool,
    bindings: Mutex<Vec<(String, String, String)>>,
}

#[async_trait]
impl HttpCredentialProvider for ChangingProvider {
    async fn token(
        &self,
        binding: &str,
        server: &str,
        endpoint: &str,
    ) -> Result<String, SecretUnavailable> {
        self.bindings
            .lock()
            .unwrap_or_else(|error| panic!("bindings: {error:?}"))
            .push((binding.to_owned(), server.to_owned(), endpoint.to_owned()));
        if self.denied.load(Ordering::SeqCst) {
            return Err(SecretUnavailable);
        }
        let generation = self.issued.fetch_add(1, Ordering::SeqCst);
        Ok(format!("synthetic-http-token-{generation}"))
    }
}

struct FixedProvider(String);

#[async_trait]
impl HttpCredentialProvider for FixedProvider {
    async fn token(&self, _: &str, _: &str, _: &str) -> Result<String, SecretUnavailable> {
        Ok(self.0.clone())
    }
}

#[derive(Clone, Debug)]
struct Request {
    method: String,
    headers: BTreeMap<String, String>,
    body: Value,
}

#[derive(Clone)]
enum Reply {
    Rpc,
    Legacy,
    Sse,
    Redirect(String),
    RefuseDiscover,
    Resume,
}

struct HttpServer {
    endpoint: McpHttpEndpoint,
    seen: Arc<Mutex<Vec<Request>>>,
    task: tokio::task::JoinHandle<()>,
}

impl HttpServer {
    async fn start(reply: Reply) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .unwrap_or_else(|error| panic!("bind loopback: {error:?}"));
        let address = listener
            .local_addr()
            .unwrap_or_else(|error| panic!("listener address: {error:?}"));
        let seen = Arc::new(Mutex::new(Vec::new()));
        let recorder = Arc::clone(&seen);
        let task = tokio::spawn(async move {
            loop {
                let Ok((stream, _)) = listener.accept().await else {
                    break;
                };
                let recorder = Arc::clone(&recorder);
                let reply = reply.clone();
                tokio::spawn(async move {
                    let _ = respond(stream, &recorder, &reply).await;
                });
            }
        });
        Self {
            endpoint: McpHttpEndpoint::parse(&format!("http://{address}/mcp"))
                .unwrap_or_else(|error| panic!("endpoint: {error:?}")),
            seen,
            task,
        }
    }

    fn requests(&self) -> Vec<Request> {
        self.seen
            .lock()
            .unwrap_or_else(|error| panic!("recorded requests: {error:?}"))
            .clone()
    }

    fn client(&self, provider: Arc<dyn HttpCredentialProvider>) -> AuthorizedHttpClient {
        AuthorizedHttpClient::loopback(&self.endpoint, provider)
    }

    fn uri(&self) -> Arc<str> {
        Arc::from(self.endpoint.as_str())
    }
}

impl Drop for HttpServer {
    fn drop(&mut self) {
        self.task.abort();
    }
}

async fn read_request(stream: &mut BufReader<TcpStream>) -> std::io::Result<Request> {
    let mut first = String::new();
    stream.read_line(&mut first).await?;
    let method = first
        .split_whitespace()
        .next()
        .unwrap_or_default()
        .to_owned();
    let mut headers = BTreeMap::new();
    loop {
        let mut line = String::new();
        stream.read_line(&mut line).await?;
        if line.trim().is_empty() {
            break;
        }
        if let Some((key, value)) = line.split_once(':') {
            headers.insert(key.to_ascii_lowercase(), value.trim().to_owned());
        }
    }
    let size = headers
        .get("content-length")
        .and_then(|size| size.parse().ok())
        .unwrap_or(0);
    let mut body = vec![0; size];
    stream.read_exact(&mut body).await?;
    let body: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);
    Ok(Request {
        method,
        headers,
        body,
    })
}

async fn respond(
    stream: TcpStream,
    seen: &Mutex<Vec<Request>>,
    reply: &Reply,
) -> std::io::Result<()> {
    let mut stream = BufReader::new(stream);
    let request = read_request(&mut stream).await?;
    seen.lock()
        .unwrap_or_else(|error| panic!("record request: {error}"))
        .push(request.clone());
    let Request {
        method,
        headers,
        body,
    } = request;
    let token = headers
        .get("authorization")
        .and_then(|value| value.strip_prefix("Bearer "))
        .unwrap_or_default();
    let rpc_method = body["method"].as_str().unwrap_or_default();
    let id = &body["id"];
    let error = |code, message: &str| json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message}});
    let mut extra_headers = String::new();
    let (status, content_type, payload) = match reply {
        Reply::Redirect(location) => {
            extra_headers = format!("Location: {location}\r\n");
            (302, "text/plain", String::new())
        }
        Reply::Sse => (
            200,
            "text/event-stream",
            format!("id: fixture-cursor\ndata: {}\n\n", "x".repeat(256)),
        ),
        Reply::Resume if method == "POST" && rpc_method == "tools/call" => (
            200,
            "text/event-stream",
            "event: ping\nid: resume-cursor\nretry: 1\ndata: {}\n\n".to_owned(),
        ),
        Reply::Resume if method == "GET" => {
            let requests = seen
                .lock()
                .unwrap_or_else(|error| panic!("requests: {error}"));
            let id = requests
                .iter()
                .find(|request| request.body["method"] == "tools/call")
                .map_or(Value::Null, |request| request.body["id"].clone());
            let message = json!({"jsonrpc": "2.0", "id": id, "result": {"resultType": "complete", "content": [{"type": "text", "text": token}]}});
            (200, "text/event-stream", format!("data: {message}\n\n"))
        }
        _ if method == "DELETE" => (204, "text/plain", String::new()),
        _ if method == "GET" => (405, "text/plain", String::new()),
        Reply::RefuseDiscover => (200, "application/json", error(-32600, token).to_string()),
        _ if id.is_null() => (202, "text/plain", String::new()),
        _ => {
            let result = match rpc_method {
                "server/discover" if matches!(reply, Reply::Legacy) => {
                    let payload = error(-32601, "method not found").to_string();
                    let response = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{payload}",
                        payload.len()
                    );
                    return stream.get_mut().write_all(response.as_bytes()).await;
                }
                "server/discover" => json!({
                    "resultType": "complete", "supportedVersions": ["2026-07-28"],
                    "capabilities": {"tools": {}}, "ttlMs": 0, "cacheScope": "private",
                    "_meta": {"io.modelcontextprotocol/serverInfo": {"name": token, "version": "1.0.0"}}
                }),
                "initialize" => {
                    extra_headers = "Mcp-Session-Id: fixture-session\r\n".to_owned();
                    json!({"protocolVersion": "2025-11-25", "capabilities": {"tools": {}}, "serverInfo": {"name": token, "version": "1.0.0"}})
                }
                "tools/list" => json!({
                    "resultType": "complete", "tools": [{"name": "search", "title": token, "description": token,
                        "inputSchema": {"type": "object", "properties": {token: {"type": "string", "description": token}}}}],
                    "ttlMs": 0, "cacheScope": "private"
                }),
                "tools/call" if body["params"]["name"] == "error" => {
                    let payload = error(-32600, token).to_string();
                    let response = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{payload}",
                        payload.len()
                    );
                    return stream.get_mut().write_all(response.as_bytes()).await;
                }
                "tools/call" => {
                    json!({"resultType": "complete", "content": [{"type": "text", "text": token}], "structuredContent": {token: token}})
                }
                _ => json!({}),
            };
            (
                200,
                "application/json",
                json!({"jsonrpc": "2.0", "id": id, "result": result}).to_string(),
            )
        }
    };
    let response = format!(
        "HTTP/1.1 {status} Fixture\r\nContent-Type: {content_type}\r\n{extra_headers}Content-Length: {}\r\nConnection: close\r\n\r\n{payload}",
        payload.len()
    );
    stream.get_mut().write_all(response.as_bytes()).await
}

fn ping() -> ClientJsonRpcMessage {
    ClientJsonRpcMessage::request(
        ClientRequest::PingRequest(PingRequest::default()),
        RequestId::Number(1),
    )
}

async fn post(
    client: &AuthorizedHttpClient,
    uri: Arc<str>,
) -> Result<StreamableHttpPostResponse, StreamableHttpError<reqwest::Error>> {
    client
        .post_message(
            uri,
            ping(),
            None,
            Some("stale-sdk-token".to_owned()),
            HashMap::new(),
        )
        .await
}

#[tokio::test]
async fn every_post_get_reconnect_and_delete_resolves_a_fresh_token() {
    let server = HttpServer::start(Reply::Rpc).await;
    let provider = Arc::new(ChangingProvider::default());
    let client = server.client(provider.clone());
    post(&client, server.uri())
        .await
        .unwrap_or_else(|error| panic!("POST: {error:?}"));
    let _ = client
        .get_stream(server.uri(), None, None, None, HashMap::new())
        .await;
    let _ = client
        .get_stream_with_max_sse_event_size(
            server.uri(),
            Some(Arc::from("session")),
            Some("cursor".to_owned()),
            None,
            HashMap::new(),
            128,
        )
        .await;
    client
        .delete_session(server.uri(), Arc::from("session"), None, HashMap::new())
        .await
        .unwrap_or_else(|error| panic!("DELETE: {error:?}"));
    let requests = server.requests();
    assert_eq!(
        requests
            .iter()
            .map(|request| request.method.as_str())
            .collect::<Vec<_>>(),
        ["POST", "GET", "GET", "DELETE"]
    );
    for (index, request) in requests.iter().enumerate() {
        assert_eq!(
            request.headers["authorization"],
            format!("Bearer synthetic-http-token-{index}")
        );
    }
    assert_eq!(requests[2].headers["last-event-id"], "cursor");
    assert_eq!(requests[2].headers["mcp-session-id"], "session");
    let bindings = provider
        .bindings
        .lock()
        .unwrap_or_else(|error| panic!("bindings: {error:?}"));
    assert_eq!(bindings.len(), 4);
    assert!(
        bindings
            .iter()
            .all(|(binding, name, endpoint)| binding == "github"
                && name == "github"
                && endpoint == server.endpoint.as_str())
    );
    assert!(!format!("{client:?}").contains("synthetic-http-token"));
}

#[tokio::test]
async fn denial_and_invalid_tokens_prevent_all_outbound_operations() {
    let server = HttpServer::start(Reply::Rpc).await;
    let provider = Arc::new(ChangingProvider::default());
    let client = server.client(provider.clone());
    post(&client, server.uri())
        .await
        .unwrap_or_else(|error| panic!("positive control: {error:?}"));
    provider.denied.store(true, Ordering::SeqCst);
    assert!(post(&client, server.uri()).await.is_err());
    assert!(
        client
            .post_message_with_max_sse_event_size(
                server.uri(),
                ping(),
                None,
                None,
                HashMap::new(),
                128
            )
            .await
            .is_err()
    );
    assert!(
        client
            .get_stream(server.uri(), None, None, None, HashMap::new())
            .await
            .is_err()
    );
    assert!(
        client
            .get_stream_with_max_sse_event_size(
                server.uri(),
                None,
                Some("cursor".to_owned()),
                None,
                HashMap::new(),
                128
            )
            .await
            .is_err()
    );
    assert!(
        client
            .delete_session(server.uri(), Arc::from("session"), None, HashMap::new())
            .await
            .is_err()
    );
    assert_eq!(server.requests().len(), 1);
    for token in [
        String::new(),
        "x".repeat(4097),
        "contains space".to_owned(),
        "line\r\ninjection".to_owned(),
        "non-ascii-\u{e9}".to_owned(),
    ] {
        let client = server.client(Arc::new(FixedProvider(token.clone())));
        let error = post(&client, server.uri())
            .await
            .err()
            .unwrap_or_else(|| panic!("invalid credential"));
        if !token.is_empty() {
            assert!(!format!("{error:?}").contains(&token));
        }
    }
    let client = server.client(Arc::new(UnavailableHttpCredentials));
    assert!(post(&client, server.uri()).await.is_err());
    assert_eq!(server.requests().len(), 1);
}

#[tokio::test]
async fn destination_change_and_redirect_never_leak_bearer() {
    let target = HttpServer::start(Reply::Rpc).await;
    let server = HttpServer::start(Reply::Redirect(target.endpoint.as_str().to_owned())).await;
    let provider = Arc::new(ChangingProvider::default());
    let client = server.client(provider.clone());
    assert!(post(&client, target.uri()).await.is_err());
    assert_eq!(provider.issued.load(Ordering::SeqCst), 0);
    assert!(post(&client, server.uri()).await.is_err());
    assert!(
        client
            .get_stream(server.uri(), None, None, None, HashMap::new())
            .await
            .is_err()
    );
    let _ = client
        .delete_session(server.uri(), Arc::from("session"), None, HashMap::new())
        .await;
    assert_eq!(server.requests().len(), 3);
    assert!(target.requests().is_empty());
}

#[tokio::test]
async fn configured_sse_event_size_is_preserved_on_post_and_get() {
    let server = HttpServer::start(Reply::Sse).await;
    let client = server.client(Arc::new(ChangingProvider::default()));
    let response = client
        .post_message_with_max_sse_event_size(server.uri(), ping(), None, None, HashMap::new(), 64)
        .await
        .unwrap_or_else(|error| panic!("SSE POST: {error:?}"));
    let StreamableHttpPostResponse::Sse(mut stream, _) = response else {
        panic!("SSE response")
    };
    let error = stream
        .next()
        .await
        .unwrap_or_else(|| panic!("POST event"))
        .err()
        .unwrap_or_else(|| panic!("oversized POST event"));
    assert!(
        error.to_string().contains("maximum size of 64 bytes"),
        "{error}"
    );
    let mut stream = client
        .get_stream_with_max_sse_event_size(
            server.uri(),
            None,
            Some("cursor".to_owned()),
            None,
            HashMap::new(),
            64,
        )
        .await
        .unwrap_or_else(|error| panic!("SSE GET: {error:?}"));
    let error = stream
        .next()
        .await
        .unwrap_or_else(|| panic!("GET event"))
        .err()
        .unwrap_or_else(|| panic!("oversized GET event"));
    assert!(
        error.to_string().contains("maximum size of 64 bytes"),
        "{error}"
    );
}

#[tokio::test]
async fn actual_http_handshake_listing_results_and_peer_errors_share_rotating_redaction() {
    let server = HttpServer::start(Reply::Rpc).await;
    let provider = Arc::new(ChangingProvider::default());
    let client = server.client(provider.clone());
    let connection = crate::client::connect_authorized_http(
        &ServerName::new("github").unwrap_or_else(|error| panic!("server name: {error:?}")),
        client,
    )
    .await
    .unwrap_or_else(|error| panic!("handshake: {error:?}"));
    assert_eq!(connection.reported_identity().name, "[REDACTED]");
    let mut buffer = crate::ToolBuffer::new();
    let tools = connection
        .list_tools(&mut buffer)
        .await
        .unwrap_or_else(|error| panic!("list tools: {error:?}"));
    assert_eq!(tools[0].description, Some("[REDACTED]"));
    assert!(!format!("{:?}", tools[0].input_schema).contains("synthetic-http-token"));
    let result = connection
        .call_tool("search", &serde_json::Map::new())
        .await
        .unwrap_or_else(|error| panic!("call: {error:?}"));
    assert_eq!(result.text, "[REDACTED]");
    assert!(!format!("{:?}", result.structured).contains("synthetic-http-token"));
    let error = connection
        .call_tool("error", &serde_json::Map::new())
        .await
        .err()
        .unwrap_or_else(|| panic!("peer refusal"));
    assert!(error.to_string().contains("[REDACTED]"), "{error}");
    assert!(!format!("{error:?}").contains("synthetic-http-token"));
    let before = server.requests().len();
    provider.denied.store(true, Ordering::SeqCst);
    assert!(
        connection
            .call_tool("search", &serde_json::Map::new())
            .await
            .is_err()
    );
    assert_eq!(server.requests().len(), before);
    connection
        .close()
        .await
        .unwrap_or_else(|error| panic!("close: {error:?}"));
    let requests = server.requests();
    assert_eq!(requests[0].body["method"], "server/discover");
    assert!(requests.iter().all(|request| {
        request
            .headers
            .get("authorization")
            .is_some_and(|value| value.starts_with("Bearer synthetic-http-token-"))
    }));
}

#[tokio::test]
async fn sdk_resumes_an_interrupted_response_with_a_new_token_without_replaying_post() {
    let server = HttpServer::start(Reply::Resume).await;
    let connection = crate::client::connect_authorized_http(
        &ServerName::new("github").unwrap_or_else(|error| panic!("server: {error}")),
        server.client(Arc::new(ChangingProvider::default())),
    )
    .await
    .unwrap_or_else(|error| panic!("handshake: {error}"));
    let result = tokio::time::timeout(
        Duration::from_secs(3),
        connection.call_tool("search", &serde_json::Map::new()),
    )
    .await
    .unwrap_or_else(|error| panic!("resume deadline: {error}"))
    .unwrap_or_else(|error| panic!("resumed response: {error}"));
    assert_eq!(result.text, "[REDACTED]");
    let requests = server.requests();
    assert_eq!(
        requests
            .iter()
            .filter(|request| request.body["method"] == "tools/call")
            .count(),
        1
    );
    let get = requests
        .iter()
        .find(|request| request.method == "GET")
        .unwrap_or_else(|| panic!("SDK must resume via GET"));
    assert_eq!(get.headers["last-event-id"], "resume-cursor");
    assert_eq!(
        get.headers["authorization"],
        "Bearer synthetic-http-token-2"
    );
    connection
        .close()
        .await
        .unwrap_or_else(|error| panic!("close: {error}"));
}

#[tokio::test]
async fn refused_handshake_redacts_the_issued_token() {
    let server = HttpServer::start(Reply::RefuseDiscover).await;
    let error = crate::client::connect_authorized_http(
        &ServerName::new("github").unwrap_or_else(|error| panic!("server: {error:?}")),
        server.client(Arc::new(ChangingProvider::default())),
    )
    .await
    .err()
    .unwrap_or_else(|| panic!("refused handshake"));
    assert!(!format!("{error:?}").contains("synthetic-http-token"));
}

#[tokio::test]
async fn real_legacy_session_cleanup_obtains_a_current_token_and_denial_sends_nothing() {
    for deny in [false, true] {
        let server = HttpServer::start(Reply::Legacy).await;
        let provider = Arc::new(ChangingProvider::default());
        let connection = crate::client::connect_authorized_http(
            &ServerName::new("github").unwrap_or_else(|error| panic!("server: {error:?}")),
            server.client(provider.clone()),
        )
        .await
        .unwrap_or_else(|error| panic!("legacy handshake: {error:?}"));
        assert_eq!(connection.era(), crate::McpEra::Legacy);
        // Wait until the SDK has made its standalone GET before isolating cleanup.
        tokio::time::timeout(Duration::from_secs(3), async {
            while !server
                .requests()
                .iter()
                .any(|request| request.method == "GET")
            {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap_or_else(|error| panic!("standalone GET: {error:?}"));
        let before = provider.issued.load(Ordering::SeqCst);
        provider.denied.store(deny, Ordering::SeqCst);
        connection
            .close()
            .await
            .unwrap_or_else(|error| panic!("close: {error:?}"));
        let requests = server.requests();
        let deletes = requests
            .iter()
            .filter(|request| request.method == "DELETE")
            .collect::<Vec<_>>();
        if deny {
            assert!(deletes.is_empty());
        } else {
            assert_eq!(deletes.len(), 1);
            assert_eq!(
                deletes[0].headers["authorization"],
                format!("Bearer synthetic-http-token-{before}")
            );
        }
    }
}

#[tokio::test]
async fn public_boundary_refuses_other_bindings_servers_and_loopback_before_resolving() {
    let provider = Arc::new(ChangingProvider::default());
    for (server, endpoint, binding) in [
        ("other", GITHUB_ENDPOINT, "github"),
        ("github", GITHUB_ENDPOINT, "other"),
        ("github", "http://127.0.0.1:12345/mcp", "github"),
        (
            "github",
            "https://api.githubcopilot.com/mcp/readonly?token=synthetic-private",
            "github",
        ),
        (
            "github",
            "https://api.githubcopilot.com:443/mcp/readonly",
            "github",
        ),
        ("github", "https://other.example/mcp/readonly", "github"),
    ] {
        let endpoint = McpHttpEndpoint::parse(endpoint)
            .unwrap_or_else(|error| panic!("valid endpoint: {error:?}"));
        let error = crate::connect_http_with_auth(
            &ServerName::new(server).unwrap_or_else(|error| panic!("server: {error:?}")),
            &endpoint,
            binding,
            provider.clone(),
        )
        .await
        .err()
        .unwrap_or_else(|| panic!("unauthorized destination"));
        assert!(!format!("{error:?}").contains("synthetic-private"));
    }
    assert!(
        provider
            .bindings
            .lock()
            .unwrap_or_else(|error| panic!("bindings: {error:?}"))
            .is_empty()
    );
}

#[test]
fn config_authorization_is_exact_server_bound_and_never_a_local_test_shortcut() {
    let config = |server: &str, endpoint: &str, binding: &str| {
        format!(
            "[[servers]]\nname='{server}'\n[servers.transport]\nkind='http'\nendpoint='{endpoint}'\nauthorization='{binding}'\n"
        )
    };
    assert!(crate::McpHostConfig::parse(&config("github", GITHUB_ENDPOINT, "github")).is_ok());
    for (server, endpoint, binding) in [
        ("other", GITHUB_ENDPOINT, "github"),
        ("github", GITHUB_ENDPOINT, "other"),
        ("github", GITHUB_ENDPOINT, ""),
        ("github", "http://127.0.0.1:12345/mcp", "github"),
        (
            "github",
            "https://api.githubcopilot.com/mcp/readonly?x=y",
            "github",
        ),
        (
            "github",
            "https://api.githubcopilot.com/mcp/readonly#fragment",
            "github",
        ),
        (
            "github",
            "https://user@api.githubcopilot.com/mcp/readonly",
            "github",
        ),
        (
            "github",
            "https://api.githubcopilot.com:443/mcp/readonly",
            "github",
        ),
        (
            "github",
            "https://api.githubcopilot.com.evil.example/mcp/readonly",
            "github",
        ),
        (
            "github",
            "https://api.githubcopilot.com/mcp/readonly/",
            "github",
        ),
    ] {
        assert!(
            crate::McpHostConfig::parse(&config(server, endpoint, binding)).is_err(),
            "must refuse binding"
        );
    }
}
