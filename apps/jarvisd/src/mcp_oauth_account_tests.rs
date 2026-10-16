use super::*;

use std::collections::{BTreeMap, VecDeque};
use std::sync::atomic::{AtomicBool, Ordering};

use axum::{
    Json,
    extract::State,
    http::{HeaderMap, StatusCode, Uri, header},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use serde_json::{Value, json};
use tokio::sync::{Notify, Semaphore};

struct Profile(PathBuf);
impl Profile {
    fn new() -> (Self, AppPaths) {
        let root = std::env::temp_dir().join(format!("jmcp-oauth-{}", jarvis_core::scratch_tag()));
        std::fs::create_dir_all(&root).unwrap_or_else(|error| panic!("profile: {error}"));
        let root = std::fs::canonicalize(root)
            .unwrap_or_else(|error| panic!("canonical profile: {error}"));
        let paths = jarvis_storage::portable_layout(&root).unwrap_or_else(|| panic!("layout"));
        (Self(root), paths)
    }
}
impl Drop for Profile {
    fn drop(&mut self) {
        jarvis_core::remove_scratch_dir(&self.0);
    }
}

struct FixtureState {
    base: String,
    replies: StdMutex<VecDeque<Value>>,
    requests: StdMutex<Vec<BTreeMap<String, String>>>,
    bearers: StdMutex<Vec<String>>,
    revoke_fails: AtomicBool,
    slow: AtomicBool,
    started: Notify,
    release: Semaphore,
}

struct Fixture {
    state: Arc<FixtureState>,
    task: tokio::task::JoinHandle<()>,
}
impl Fixture {
    async fn new() -> Self {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .unwrap_or_else(|error| panic!("bind: {error}"));
        let state = Arc::new(FixtureState {
            base: format!(
                "http://{}",
                listener
                    .local_addr()
                    .unwrap_or_else(|error| panic!("address: {error}"))
            ),
            replies: StdMutex::new(VecDeque::new()),
            requests: StdMutex::new(Vec::new()),
            bearers: StdMutex::new(Vec::new()),
            revoke_fails: AtomicBool::new(false),
            slow: AtomicBool::new(false),
            started: Notify::new(),
            release: Semaphore::new(0),
        });
        let router = axum::Router::new()
            .route("/token", post(token))
            .route("/revoke", post(revoke))
            .route("/mcp", get(metadata).post(mcp))
            .fallback(get(metadata))
            .with_state(Arc::clone(&state));
        let task = tokio::spawn(async move {
            axum::serve(listener, router)
                .await
                .unwrap_or_else(|error| panic!("serve: {error}"));
        });
        Self { state, task }
    }
    fn resource(&self) -> String {
        format!("{}/mcp", self.state.base)
    }
    fn queue(&self, reply: Value) {
        self.state
            .replies
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push_back(reply);
    }
    fn requests(&self) -> Vec<BTreeMap<String, String>> {
        self.state
            .requests
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }
    async fn wait_for_token(&self) {
        tokio::time::timeout(
            std::time::Duration::from_secs(5),
            self.state.started.notified(),
        )
        .await
        .unwrap_or_else(|_| panic!("token was not requested"));
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.task.abort();
    }
}

async fn metadata(State(state): State<Arc<FixtureState>>, uri: Uri) -> Response {
    let base = &state.base;
    if uri.path() == "/mcp" {
        return (
            StatusCode::UNAUTHORIZED,
            [(
                header::WWW_AUTHENTICATE,
                format!(
                    "Bearer resource_metadata=\"{base}/.well-known/oauth-protected-resource/mcp\""
                ),
            )],
            "",
        )
            .into_response();
    }
    if uri.path().contains("oauth-protected-resource") {
        return Json(json!({"resource": format!("{base}/mcp"), "authorization_servers": [base]}))
            .into_response();
    }
    Json(json!({
        "issuer": base, "authorization_endpoint": "https://oauth.example/authorize", "token_endpoint": format!("{base}/token"),
        "revocation_endpoint": format!("{base}/revoke"), "code_challenge_methods_supported": ["S256"],
        "response_types_supported": ["code"], "grant_types_supported": ["authorization_code", "refresh_token"],
        "token_endpoint_auth_methods_supported": ["none", "client_secret_basic", "client_secret_post"],
        "authorization_response_iss_parameter_supported": true
    })).into_response()
}

async fn token(
    State(state): State<Arc<FixtureState>>,
    headers: HeaderMap,
    body: String,
) -> Response {
    let url = reqwest::Url::parse(&format!("http://form.invalid/?{body}"))
        .unwrap_or_else(|_| panic!("form"));
    let mut fields: BTreeMap<String, String> = url
        .query_pairs()
        .map(|(key, value)| (key.into_owned(), value.into_owned()))
        .collect();
    if let Some(value) = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
    {
        fields.insert("authorization_header".to_owned(), value.to_owned());
    }
    state
        .requests
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .push(fields);
    if state.slow.swap(false, Ordering::SeqCst) {
        state.started.notify_one();
        state
            .release
            .acquire()
            .await
            .unwrap_or_else(|_| panic!("release"))
            .forget();
    }
    let reply = state.replies.lock().unwrap_or_else(std::sync::PoisonError::into_inner).pop_front().unwrap_or_else(|| json!({"error": "invalid_grant", "error_description": "canary-private-provider-error"}));
    Json(reply).into_response()
}

async fn revoke(State(state): State<Arc<FixtureState>>) -> StatusCode {
    if state.revoke_fails.load(Ordering::SeqCst) {
        StatusCode::BAD_GATEWAY
    } else {
        StatusCode::OK
    }
}

async fn mcp(
    State(state): State<Arc<FixtureState>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Response {
    let token = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .unwrap_or_default();
    if token.is_empty() {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    state
        .bearers
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .push(token.to_owned());
    let result = match body["method"].as_str().unwrap_or_default() {
        "server/discover" => json!({"resultType": "complete", "supportedVersions": ["2026-07-28"],
            "capabilities": {"tools": {}}, "ttlMs": 0, "cacheScope": "private",
            "_meta": {"io.modelcontextprotocol/serverInfo": {"name": "oauth-fixture", "version": "1.0.0"}}}),
        "tools/list" => {
            json!({"resultType": "complete", "tools": [{"name": "search", "description": token,
            "inputSchema": {"type": "object", "properties": {}}}], "ttlMs": 0, "cacheScope": "private"})
        }
        _ => json!({"resultType": "complete", "content": [{"type": "text", "text": token}]}),
    };
    Json(json!({"jsonrpc": "2.0", "id": body["id"], "result": result})).into_response()
}

fn setup(account: &Account, fixture: &Fixture) {
    account
        .save_setup(
            &fixture.resource(),
            &fixture.state.base,
            "registered-client",
            None,
            &["read".to_owned()],
        )
        .unwrap_or_else(|error| panic!("setup: {error}"));
}

async fn begin(account: &Account, fixture: &Fixture) -> reqwest::Url {
    reqwest::Url::parse(
        &account
            .begin(&fixture.resource(), 8765)
            .await
            .unwrap_or_else(|error| panic!("begin: {error}")),
    )
    .unwrap_or_else(|error| panic!("url: {error}"))
}

fn answer(url: &reqwest::Url, issuer: &str) -> String {
    let state = url
        .query_pairs()
        .find(|(key, _)| key == "state")
        .unwrap_or_else(|| panic!("state"))
        .1
        .into_owned();
    let mut callback = reqwest::Url::parse("http://127.0.0.1:8765/oauth/mcp/remote/callback")
        .unwrap_or_else(|_| panic!("callback"));
    callback
        .query_pairs_mut()
        .append_pair("state", &state)
        .append_pair("code", "synthetic-code")
        .append_pair("iss", issuer);
    format!(
        "{}?{}",
        callback.path(),
        callback.query().unwrap_or_default()
    )
}

fn grant(access: &str, refresh: Option<&str>) -> Value {
    let mut value = json!({"access_token": access, "token_type": "Bearer", "expires_in": 3600, "scope": "read"});
    if let Some(refresh) = refresh {
        value["refresh_token"] = json!(refresh);
    }
    value
}

async fn connected(account: &Account, fixture: &Fixture) {
    setup(account, fixture);
    let url = begin(account, fixture).await;
    fixture.queue(grant("synthetic-access-one", Some("synthetic-refresh-one")));
    account
        .complete(&fixture.resource(), &answer(&url, &fixture.state.base))
        .await
        .unwrap_or_else(|error| panic!("complete: {error}"));
}

fn expire(account: &Account) {
    let mut saved: Grant = account
        .read(GRANT_KEY)
        .unwrap_or_else(|| panic!("grant missing"));
    saved.expires_at = Some(now() - 1);
    account
        .write(GRANT_KEY, &saved)
        .unwrap_or_else(|error| panic!("expire: {error}"));
}

#[tokio::test]
async fn preregistered_sign_in_has_pkce_resource_and_persists_without_exposing_secrets_or_identity_claims()
 {
    let (_profile, paths) = Profile::new();
    let fixture = Fixture::new().await;
    let account = Account::loopback(paths.clone(), "remote");
    setup(&account, &fixture);
    let url = begin(&account, &fixture).await;
    let params: BTreeMap<_, _> = url
        .query_pairs()
        .map(|(key, value)| (key.into_owned(), value.into_owned()))
        .collect();
    assert_eq!(params["client_id"], "registered-client");
    assert_eq!(params["resource"], fixture.resource());
    assert_eq!(params["code_challenge_method"], "S256");
    assert_eq!(
        params["redirect_uri"],
        "http://127.0.0.1:8765/oauth/mcp/remote/callback"
    );
    assert_eq!(params["scope"], "read");
    let target = answer(&url, &fixture.state.base);
    fixture.queue(grant("synthetic-access-one", Some("synthetic-refresh-one")));
    account
        .complete(&fixture.resource(), &target)
        .await
        .unwrap_or_else(|error| panic!("complete: {error}"));
    assert!(
        account
            .complete(&fixture.resource(), &target)
            .await
            .is_err()
    );
    let requests = fixture.requests();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0]["resource"], fixture.resource());
    assert_eq!(requests[0]["grant_type"], "authorization_code");
    let verifier = PkceVerifier::new(requests[0]["code_verifier"].clone())
        .unwrap_or_else(|_| panic!("verifier"));
    assert_eq!(
        verifier.challenge(PkceMethod::S256).value(),
        params["code_challenge"]
    );
    drop(account);
    let restored = Account::loopback(paths, "remote");
    assert!(restored.status(&fixture.resource()).connected);
    assert_eq!(
        restored
            .access_token(&fixture.resource())
            .await
            .ok()
            .as_deref(),
        Some("synthetic-access-one")
    );
    let status = serde_json::to_string(&restored.status(&fixture.resource()))
        .unwrap_or_else(|_| panic!("status"));
    assert!(!status.contains("synthetic-") && !status.contains("login"));
}

#[tokio::test]
async fn wrong_issuer_state_replayed_or_duplicate_callback_never_exchanges_a_code() {
    let (_profile, paths) = Profile::new();
    let fixture = Fixture::new().await;
    let account = Account::loopback(paths, "remote");
    setup(&account, &fixture);
    for variant in 0..6 {
        let url = begin(&account, &fixture).await;
        let valid = answer(&url, &fixture.state.base);
        let target = match variant {
            0 => answer(&url, "https://wrong-issuer.example"),
            1 => valid.replace("state=", "state=wrong"),
            2 => format!("{valid}&%73tate=duplicate"),
            3 => valid.split("&iss=").next().unwrap_or_default().to_owned(),
            4 => format!("{valid}&error=denied&error_description=canary-private-provider-error"),
            _ => {
                account
                    .memory()
                    .pending
                    .as_mut()
                    .unwrap_or_else(|| panic!("pending"))
                    .started = Instant::now()
                    .checked_sub(std::time::Duration::from_secs(601))
                    .unwrap_or_else(|| panic!("fixture clock"));
                valid
            }
        };
        assert!(
            account
                .complete(&fixture.resource(), &target)
                .await
                .is_err()
        );
        assert!(fixture.requests().is_empty());
        assert!(!account.status(&fixture.resource()).connected);
    }
}

#[tokio::test]
async fn refresh_is_serialized_preserves_an_omitted_refresh_token_and_keeps_resource_binding() {
    let (_profile, paths) = Profile::new();
    let fixture = Fixture::new().await;
    let account = Arc::new(Account::loopback(paths, "remote"));
    connected(&account, &fixture).await;
    expire(&account);
    fixture.queue(grant("synthetic-access-two", None));
    let resource = fixture.resource();
    let (first, second) = tokio::join!(
        account.access_token(&resource),
        account.access_token(&resource)
    );
    assert_eq!(first.ok().as_deref(), Some("synthetic-access-two"));
    assert_eq!(second.ok().as_deref(), Some("synthetic-access-two"));
    let requests = fixture.requests();
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[1]["grant_type"], "refresh_token");
    assert_eq!(requests[1]["refresh_token"], "synthetic-refresh-one");
    assert_eq!(requests[1]["resource"], fixture.resource());
    let stored: Grant = account.read(GRANT_KEY).unwrap_or_else(|| panic!("stored"));
    assert_eq!(
        stored.refresh_token.as_deref(),
        Some("synthetic-refresh-one")
    );
    assert!(
        account
            .access_token("https://other.example/mcp")
            .await
            .is_err()
    );
    assert_eq!(fixture.requests().len(), 2);
}

#[tokio::test]
async fn failed_or_cancelled_refresh_cannot_reuse_a_consumed_grant_after_restart() {
    for cancel in [false, true] {
        let (_profile, paths) = Profile::new();
        let fixture = Fixture::new().await;
        let account = Arc::new(Account::loopback(paths.clone(), "remote"));
        connected(&account, &fixture).await;
        expire(&account);
        if cancel {
            fixture.state.slow.store(true, Ordering::SeqCst);
            let worker = Arc::clone(&account);
            let resource = fixture.resource();
            let task = tokio::spawn(async move { worker.access_token(&resource).await });
            fixture.wait_for_token().await;
            task.abort();
            let _ = task.await;
            fixture.state.release.add_permits(1);
        } else {
            assert!(account.access_token(&fixture.resource()).await.is_err());
        }
        let before = fixture.requests().len();
        let restored = Account::loopback(paths, "remote");
        assert!(restored.access_token(&fixture.resource()).await.is_err());
        assert_eq!(fixture.requests().len(), before);
    }
}

#[tokio::test]
async fn disconnect_or_changed_client_prevents_late_callback_and_refresh_from_restoring_authority()
{
    for (refresh, change_setup) in [(false, false), (true, false), (false, true), (true, true)] {
        let (_profile, paths) = Profile::new();
        let fixture = Fixture::new().await;
        let account = Arc::new(Account::loopback(paths, "remote"));
        let target = if refresh {
            connected(&account, &fixture).await;
            expire(&account);
            None
        } else {
            setup(&account, &fixture);
            let url = begin(&account, &fixture).await;
            Some(answer(&url, &fixture.state.base))
        };
        fixture.queue(grant(
            "synthetic-late-access",
            Some("synthetic-late-refresh"),
        ));
        fixture.state.slow.store(true, Ordering::SeqCst);
        let worker = Arc::clone(&account);
        let resource = fixture.resource();
        let task = tokio::spawn(async move {
            if let Some(target) = target {
                worker
                    .complete(&resource, &target)
                    .await
                    .map(|()| String::new())
            } else {
                worker.access_token(&resource).await
            }
        });
        fixture.wait_for_token().await;
        if change_setup {
            account
                .save_setup(
                    &fixture.resource(),
                    &fixture.state.base,
                    "changed-client",
                    None,
                    &["read".to_owned()],
                )
                .unwrap_or_else(|error| panic!("change: {error}"));
        } else {
            account
                .disconnect(&fixture.resource())
                .await
                .unwrap_or_else(|error| panic!("disconnect: {error}"));
        }
        fixture.state.release.add_permits(1);
        assert!(
            task.await
                .unwrap_or_else(|error| panic!("join: {error}"))
                .is_err()
        );
        assert!(!account.status(&fixture.resource()).connected);
        assert!(account.access_token(&fixture.resource()).await.is_err());
    }
}

#[tokio::test]
async fn generic_oauth_grant_reaches_the_real_mcp_host_then_disconnect_blocks_its_existing_connection()
 {
    let (_profile, paths) = Profile::new();
    let fixture = Fixture::new().await;
    let account = Arc::new(Account::loopback(paths.clone(), "remote"));
    connected(&account, &fixture).await;
    let profiles = Accounts::shared(&paths);
    profiles
        .accounts
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .insert("remote".to_owned(), Arc::clone(&account));
    let document = format!(
        "[[servers]]\nname='remote'\n[servers.transport]\nkind='http'\nendpoint='{}'\nauthorization='oauth:remote'\n",
        fixture.resource()
    );
    let report = crate::mcp_host::try_connect(&document, &paths).await;
    assert_eq!(report["ok"], true, "{report}");
    assert!(!report.to_string().contains("synthetic-access-one"));
    let host = crate::mcp_host::compose_host_with_paths(Some(&document), &paths)
        .await
        .unwrap_or_else(|error| panic!("host: {error}"))
        .unwrap_or_else(|| panic!("host missing"));
    assert_eq!(host.reachable_servers(), 1);
    assert!(
        host.definitions()
            .iter()
            .all(|definition| definition.approval() == jarvis_tools::ApprovalPolicy::Ask)
    );
    let provider = Arc::new(crate::mcp_credentials::ProfileHttpSecrets::new(&paths));
    let endpoint = jarvis_mcp_transport::McpHttpEndpoint::parse(&fixture.resource())
        .unwrap_or_else(|error| panic!("endpoint: {error}"));
    let server =
        jarvis_mcp::ServerName::new("remote").unwrap_or_else(|error| panic!("name: {error}"));
    let connection =
        jarvis_mcp_transport::connect_http_with_auth(&server, &endpoint, "oauth:remote", provider)
            .await
            .unwrap_or_else(|error| panic!("connection: {error}"));
    let result = connection
        .call_tool("search", &serde_json::Map::new())
        .await
        .unwrap_or_else(|error| panic!("call: {error}"));
    assert!(result.text.contains("[REDACTED]") && !result.text.contains("synthetic-access-one"));
    let before = fixture
        .state
        .bearers
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .len();
    assert!(account.disconnect(&fixture.resource()).await.is_ok());
    assert!(
        connection
            .call_tool("search", &serde_json::Map::new())
            .await
            .is_err()
    );
    assert_eq!(
        fixture
            .state
            .bearers
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .len(),
        before
    );
    assert!(connection.close().await.is_ok());
    assert!(host.close().await.is_ok());
}

#[tokio::test]
async fn revocation_failure_does_not_restore_local_access_after_restart() {
    let (_profile, paths) = Profile::new();
    let fixture = Fixture::new().await;
    let account = Account::loopback(paths.clone(), "remote");
    connected(&account, &fixture).await;
    fixture.state.revoke_fails.store(true, Ordering::SeqCst);
    assert_eq!(
        account.disconnect(&fixture.resource()).await.ok(),
        Some(false)
    );
    assert!(!account.status(&fixture.resource()).connected);
    drop(account);
    assert!(
        Account::loopback(paths, "remote")
            .access_token(&fixture.resource())
            .await
            .is_err()
    );
    assert_eq!(fixture.requests().len(), 1);
}

#[tokio::test]
async fn token_lifetime_scope_and_format_failures_never_create_unbounded_or_broader_grants() {
    let (_profile, paths) = Profile::new();
    let fixture = Fixture::new().await;
    let account = Account::loopback(paths, "remote");
    connected(&account, &fixture).await;
    let setup: Setup = account
        .setup(&fixture.resource())
        .unwrap_or_else(|| panic!("setup"));
    let old: Grant = account
        .read(GRANT_KEY)
        .unwrap_or_else(|| panic!("old grant"));
    for patch in [
        json!({"token_type": "Basic"}),
        json!({"access_token": "bad token"}),
        json!({"expires_in": 0}),
        json!({"expires_in": u64::MAX}),
        json!({"scope": "write"}),
        json!({"scope": "read write"}),
        json!({"refresh_token": "bad refresh token"}),
        json!({"refresh_token_expires_in": 0}),
    ] {
        let mut value = grant("synthetic-new", Some("synthetic-new-refresh"));
        value
            .as_object_mut()
            .unwrap_or_else(|| panic!("object"))
            .extend(patch.as_object().unwrap_or_else(|| panic!("patch")).clone());
        let reply = serde_json::from_value(value).unwrap_or_else(|_| panic!("reply"));
        assert!(token_grant(reply, &setup, now(), Some(&old)).is_err());
    }
    let reply = serde_json::from_value(
        json!({"access_token": "synthetic-new", "token_type": "Bearer", "scope": "read"}),
    )
    .unwrap_or_else(|_| panic!("reply"));
    assert!(
        token_grant(reply, &setup, now(), Some(&old)).is_err(),
        "refresh cannot turn an expiring grant into an unlimited one"
    );
}

#[tokio::test]
async fn changed_setup_invalidates_saved_grants_and_profile_bound_services_are_shared() {
    let (_profile, paths) = Profile::new();
    let fixture = Fixture::new().await;
    let account = Account::loopback(paths.clone(), "remote");
    connected(&account, &fixture).await;
    account
        .save_setup(
            &fixture.resource(),
            &fixture.state.base,
            "new-client",
            None,
            &["read".to_owned()],
        )
        .unwrap_or_else(|error| panic!("change: {error}"));
    assert!(!account.status(&fixture.resource()).connected);
    assert!(account.access_token(&fixture.resource()).await.is_err());
    let first = Accounts::shared(&paths);
    let second = Accounts::shared(&paths);
    assert!(Arc::ptr_eq(&first, &second));
    assert!(Arc::ptr_eq(
        &first
            .account("remote")
            .unwrap_or_else(|_| panic!("account")),
        &second
            .account("remote")
            .unwrap_or_else(|_| panic!("account"))
    ));
    let (_other_profile, other_paths) = Profile::new();
    assert!(!Arc::ptr_eq(&first, &Accounts::shared(&other_paths)));
}
