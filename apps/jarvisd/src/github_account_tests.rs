//! Canary-only OAuth contract tests using real loopback HTTP, never provider accounts.

use std::collections::VecDeque;
use std::fs;
use std::future::Future;
use std::pin::Pin;
use std::task::Poll;

use base64::Engine;
use sha2::{Digest, Sha256};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::Notify;
use tokio::task::JoinHandle;

use super::*;

const CLIENT: &str = "canary-client";
const SECRET: &str = "canary-app-secret";
const ACCESS: &str = "canary-access";
const REFRESH: &str = "canary-refresh";

fn must<T, E: std::fmt::Debug>(result: Result<T, E>) -> T {
    result.unwrap_or_else(|error| panic!("fixture operation failed: {error:?}"))
}

/// Excludes synchronous credential ACL work from a deliberately held HTTP reply's
/// clock. Resume on unwind too; never await pending network work in this scope.
struct MutationClock;

impl MutationClock {
    fn pause() -> Self {
        tokio::time::pause();
        Self
    }
}

impl Drop for MutationClock {
    fn drop(&mut self) {
        tokio::time::resume();
    }
}

async fn poll_once<F: Future>(mut future: Pin<&mut F>) -> Poll<F::Output> {
    std::future::poll_fn(|context| Poll::Ready(future.as_mut().poll(context))).await
}

async fn disconnect_without_network(account: &GithubAccount) {
    // A callback held before persistence has no grant to revoke. Assert that
    // disconnect finishes synchronously rather than auto-advancing paused time.
    let mut disconnect = Box::pin(account.disconnect());
    let Poll::Ready(result) = poll_once(disconnect.as_mut()).await else {
        panic!("held-callback disconnect unexpectedly awaited network work");
    };
    must(result);
}

struct Profile {
    root: PathBuf,
    paths: AppPaths,
}

impl Profile {
    fn new() -> Self {
        let root = must(std::env::current_dir()).join("target").join(format!(
            "github-account-tests-{}",
            jarvis_core::scratch_tag()
        ));
        must(fs::create_dir_all(&root));
        Self {
            paths: must(AppPaths::from_root(&root)),
            root,
        }
    }

    fn account(&self, fixture: &Fixture) -> Arc<GithubAccount> {
        Arc::new(must(GithubAccount::with_endpoints(
            self.paths.clone(),
            fixture.endpoints(),
        )))
    }
}

impl Drop for Profile {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

#[derive(Clone)]
struct Recorded {
    head: String,
    body: String,
}

struct Answer {
    status: u16,
    body: String,
    headers: String,
    chunked: bool,
    release: Option<Arc<Notify>>,
}

impl Answer {
    fn json(status: u16, body: &str) -> Self {
        Self {
            status,
            body: body.to_owned(),
            headers: String::new(),
            chunked: false,
            release: None,
        }
    }
}

struct Fixture {
    base: String,
    requests: Arc<StdMutex<Vec<Recorded>>>,
    answers: Arc<StdMutex<HashMap<String, VecDeque<Answer>>>>,
    seen: Arc<Notify>,
    task: JoinHandle<()>,
}

impl Fixture {
    async fn new() -> Self {
        let listener = must(TcpListener::bind("127.0.0.1:0").await);
        let base = format!("http://{}", must(listener.local_addr()));
        let requests = Arc::new(StdMutex::new(Vec::new()));
        let answers = Arc::new(StdMutex::new(HashMap::<String, VecDeque<Answer>>::new()));
        let seen = Arc::new(Notify::new());
        let (recorded, routes, observed) = (
            Arc::clone(&requests),
            Arc::clone(&answers),
            Arc::clone(&seen),
        );
        let task = tokio::spawn(async move {
            while let Ok((mut socket, _)) = listener.accept().await {
                let (recorded, routes, observed) = (
                    Arc::clone(&recorded),
                    Arc::clone(&routes),
                    Arc::clone(&observed),
                );
                tokio::spawn(async move {
                    let mut received = Vec::new();
                    let mut buffer = [0_u8; 4096];
                    loop {
                        let count = must(socket.read(&mut buffer).await);
                        if count == 0 {
                            return;
                        }
                        received.extend_from_slice(&buffer[..count]);
                        assert!(received.len() < 64 * 1024, "fixture request exceeds bound");
                        if let Some(split) =
                            received.windows(4).position(|bytes| bytes == b"\r\n\r\n")
                        {
                            let header = String::from_utf8_lossy(&received[..split]);
                            let length = header
                                .lines()
                                .find_map(|line| {
                                    line.to_ascii_lowercase()
                                        .strip_prefix("content-length:")
                                        .and_then(|length| length.trim().parse::<usize>().ok())
                                })
                                .unwrap_or(0);
                            if received.len() >= split + 4 + length {
                                break;
                            }
                        }
                    }
                    let text = must(String::from_utf8(received));
                    let (head, body) = text.split_once("\r\n\r\n").unwrap_or((text.as_str(), ""));
                    let key = head
                        .lines()
                        .next()
                        .unwrap_or_default()
                        .split_whitespace()
                        .take(2)
                        .collect::<Vec<_>>()
                        .join(" ");
                    recorded
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner)
                        .push(Recorded {
                            head: head.to_owned(),
                            body: body.to_owned(),
                        });
                    let answer = routes
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner)
                        .get_mut(&key)
                        .and_then(VecDeque::pop_front)
                        .unwrap_or_else(|| Answer::json(500, "{}"));
                    observed.notify_one();
                    if let Some(release) = answer.release {
                        release.notified().await;
                    }
                    let framing = if answer.chunked {
                        "Transfer-Encoding: chunked".to_owned()
                    } else {
                        format!("Content-Length: {}", answer.body.len())
                    };
                    let body = if answer.chunked {
                        format!("{:x}\r\n{}\r\n0\r\n\r\n", answer.body.len(), answer.body)
                    } else {
                        answer.body
                    };
                    let response = format!(
                        "HTTP/1.1 {} Fixture\r\nContent-Type: application/json\r\n{framing}\r\n{}Connection: close\r\n\r\n{body}",
                        answer.status, answer.headers
                    );
                    let _ = socket.write_all(response.as_bytes()).await;
                });
            }
        });
        Self {
            base,
            requests,
            answers,
            seen,
            task,
        }
    }

    fn endpoints(&self) -> Endpoints {
        Endpoints {
            token: format!("{}/token", self.base),
            user: format!("{}/user", self.base),
            applications: format!("{}/applications", self.base),
        }
    }

    fn answer(&self, method: &str, path: &str, answer: Answer) {
        self.answers
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .entry(format!("{method} {path}"))
            .or_default()
            .push_back(answer);
    }

    fn token(&self, value: impl Serialize) {
        self.answer(
            "POST",
            "/token",
            Answer::json(200, &must(serde_json::to_string(&value))),
        );
    }

    fn user(&self) {
        self.answer(
            "GET",
            "/user",
            Answer::json(200, r#"{"id":123456,"login":"canary-owner"}"#),
        );
    }

    fn count(&self) -> usize {
        self.requests
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .len()
    }

    fn recorded(&self) -> Vec<Recorded> {
        self.requests
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    async fn wait_for(&self, count: usize) {
        must(
            tokio::time::timeout(Duration::from_secs(5), async {
                while self.count() < count {
                    self.seen.notified().await;
                }
            })
            .await,
        );
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        self.task.abort();
    }
}

fn legacy() -> serde_json::Value {
    serde_json::json!({ "access_token": ACCESS, "token_type": "bearer", "scope": "" })
}

fn expiring(access: &str, refresh: &str, expires: u64) -> serde_json::Value {
    serde_json::json!({ "access_token": access, "token_type": "bearer", "scope": "",
        "expires_in": expires, "refresh_token": refresh, "refresh_token_expires_in": 15_897_600 })
}

fn setup(account: &GithubAccount) {
    must(account.save_app(CLIENT, Some(SECRET), false));
}

fn query(url: &str) -> HashMap<String, String> {
    must(reqwest::Url::parse(url))
        .query_pairs()
        .into_owned()
        .collect()
}

fn callback(account: &GithubAccount) -> String {
    let parameters = query(&must(account.begin(8123)));
    format!(
        "{CALLBACK_PATH}?code=canary-code&state={}",
        parameters["state"]
    )
}

fn fields(body: &str) -> HashMap<String, String> {
    query(&format!("http://127.0.0.1/?{body}"))
}

async fn connect(account: &GithubAccount, fixture: &Fixture, token: serde_json::Value) {
    fixture.token(token);
    fixture.user();
    must(account.complete(&callback(account)).await);
}

#[tokio::test]
async fn scopes_pkce_and_exact_callback_are_explicit() {
    let profile = Profile::new();
    let fixture = Fixture::new().await;
    let account = profile.account(&fixture);
    assert_eq!(account.begin(8123), Err(GithubError::NotConfigured));
    setup(&account);
    let url = must(account.begin(8123));
    assert!(url.starts_with("https://github.com/login/oauth/authorize?"));
    let parameters = query(&url);
    assert_eq!(
        parameters["redirect_uri"],
        format!("http://127.0.0.1:8123{CALLBACK_PATH}")
    );
    assert_eq!(parameters["code_challenge_method"], "S256");
    assert_eq!(parameters["scope"], "offline_access");
    assert!(parameters["state"].len() >= 43);
    assert!(!parameters.contains_key("client_secret"));
    fixture.token(legacy());
    fixture.user();
    must(
        account
            .complete(&format!(
                "{CALLBACK_PATH}?state={}&code=canary-code&login=forged",
                parameters["state"]
            ))
            .await,
    );
    let requests = fixture.recorded();
    let exchange = fields(&requests[0].body);
    assert_eq!(exchange["client_id"], CLIENT);
    assert_eq!(exchange["client_secret"], SECRET);
    assert_eq!(exchange["redirect_uri"], parameters["redirect_uri"]);
    let challenge = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .encode(Sha256::digest(exchange["code_verifier"].as_bytes()));
    assert_eq!(challenge, parameters["code_challenge"]);
    assert!(
        requests[0]
            .head
            .to_lowercase()
            .contains("accept: application/json")
    );
    assert!(
        requests[1]
            .head
            .to_lowercase()
            .contains("authorization: bearer canary-access")
    );
    assert!(
        requests[1]
            .head
            .to_lowercase()
            .contains("x-github-api-version: 2026-03-10")
    );
    assert_eq!(account.status().login.as_deref(), Some("canary-owner"));
    assert_eq!(
        account
            .complete(&format!(
                "{CALLBACK_PATH}?state={}&code=canary-code",
                parameters["state"]
            ))
            .await,
        Err(GithubError::Refused)
    );
    assert_eq!(fixture.count(), 2);
}

#[tokio::test]
async fn forged_replayed_expired_malformed_and_changed_app_callbacks_send_nothing() {
    let profile = Profile::new();
    let fixture = Fixture::new().await;
    let account = profile.account(&fixture);
    setup(&account);
    let valid = callback(&account);
    assert_eq!(
        account
            .complete(&format!("{CALLBACK_PATH}?code=canary-code&state=forged"))
            .await,
        Err(GithubError::Refused)
    );
    assert_eq!(account.complete(&valid).await, Err(GithubError::Refused));
    let valid = callback(&account);
    {
        let mut memory = account.memory();
        let pending = memory.pending.as_mut().unwrap_or_else(|| panic!("pending"));
        pending.started = Instant::now()
            .checked_sub(PENDING_LIFETIME)
            .unwrap_or_else(|| panic!("monotonic clock cannot represent expiry fixture"));
    }
    assert!(!account.status().pending);
    assert_eq!(account.complete(&valid).await, Err(GithubError::Refused));
    for target in [
        format!("{CALLBACK_PATH}?state=x&state=y&code=z"),
        format!("{CALLBACK_PATH}?state=%gg&code=z"),
        format!("{CALLBACK_PATH}?state=x&code=z#fragment"),
        "/wrong?state=x&code=z".to_owned(),
        format!("https://github.com{CALLBACK_PATH}?code=z&state=x"),
        format!("{CALLBACK_PATH}?state={}", "x".repeat(8192)),
    ] {
        let _ = callback(&account);
        assert_eq!(account.complete(&target).await, Err(GithubError::Refused));
    }
    let valid = callback(&account);
    must(account.save_app("canary-other-client", Some("canary-other-secret"), false));
    assert_eq!(account.complete(&valid).await, Err(GithubError::Refused));
    let valid = callback(&account);
    must(account.save_app("canary-other-client", None, true));
    assert_eq!(account.complete(&valid).await, Err(GithubError::Refused));
    assert_eq!(fixture.count(), 0);
    assert!(!account.status().connected);
}

#[tokio::test]
async fn provider_browser_error_requires_state_and_consumes_the_transaction() {
    let profile = Profile::new();
    let fixture = Fixture::new().await;
    let account = profile.account(&fixture);
    setup(&account);
    let _ = callback(&account);
    assert_eq!(
        account
            .complete(&format!(
                "{CALLBACK_PATH}?error=canary-private-provider-error&state=forged"
            ))
            .await,
        Err(GithubError::Refused)
    );
    let parameters = query(&must(account.begin(8123)));
    assert_eq!(account.complete(&format!("{CALLBACK_PATH}?error=access_denied&error_description=canary-private-error&state={}", parameters["state"])).await, Err(GithubError::ProviderError));
    assert!(!account.status().pending);
    assert_eq!(fixture.count(), 0);
}

#[tokio::test]
async fn token_errors_and_ambiguous_lifetimes_never_reach_identity_or_storage() {
    let profile = Profile::new();
    let fixture = Fixture::new().await;
    let account = profile.account(&fixture);
    setup(&account);
    let invalid = [
        serde_json::json!({ "error": "canary-raw-error", "error_description": "canary-raw-description" }),
        serde_json::json!({ "access_token": ACCESS, "scope": "", "token_type": "bearer", "error": "canary-raw-error" }),
        serde_json::json!({ "access_token": ACCESS, "scope": "" }),
        serde_json::json!({ "access_token": ACCESS, "scope": "", "token_type": "mac" }),
        serde_json::json!({ "access_token": "canary with space", "scope": "", "token_type": "bearer" }),
        serde_json::json!({ "access_token": "x".repeat(1025), "scope": "", "token_type": "bearer" }),
        serde_json::json!({ "access_token": ACCESS, "scope": "", "token_type": "bearer", "expires_in": 600 }),
        serde_json::json!({ "access_token": ACCESS, "scope": "", "token_type": "bearer", "refresh_token": REFRESH }),
        expiring(ACCESS, REFRESH, 0),
        expiring(ACCESS, REFRESH, u64::MAX),
    ];
    for value in invalid {
        fixture.token(value);
        assert_eq!(
            account.complete(&callback(&account)).await,
            Err(GithubError::ExchangeFailed)
        );
        assert!(!account.status().connected);
    }
    assert_eq!(fixture.count(), 10);
    assert!(
        fixture
            .recorded()
            .iter()
            .all(|request| request.head.starts_with("POST /token "))
    );
}

#[tokio::test]
async fn repo_scope_is_opt_in_and_missing_repo_is_refused() {
    let profile = Profile::new();
    let fixture = Fixture::new().await;
    let account = profile.account(&fixture);
    must(account.save_app(CLIENT, Some(SECRET), true));
    assert_eq!(
        query(&must(account.begin(8123)))["scope"],
        "offline_access repo"
    );
    fixture.token(legacy());
    assert_eq!(
        account.complete(&callback(&account)).await,
        Err(GithubError::ScopesMissing)
    );
    assert_eq!(fixture.count(), 1);
    let mut granted = legacy();
    granted["scope"] = serde_json::json!("repo,gist");
    connect(&account, &fixture, granted).await;
    assert!(account.status().connected);
    must(account.save_app(CLIENT, None, false));
    assert!(!account.status().connected);
    assert_eq!(account.access_token().await, Err(GithubError::NotConnected));
}

#[tokio::test]
async fn invalid_user_or_failed_lookup_is_never_durably_connected() {
    let profile = Profile::new();
    let fixture = Fixture::new().await;
    let account = profile.account(&fixture);
    setup(&account);
    for body in [
        r#"{"id":0,"login":"canary-owner"}"#,
        r#"{"id":"123456","login":"canary-owner"}"#,
        r#"{"id":-1,"login":"canary-owner"}"#,
        r#"{"id":123456,"login":"canary owner"}"#,
        r#"{"id":123456,"login":"<script>"}"#,
        r#"{"id":123456,"login":"é"}"#,
        r#"{"id":123456,"login":"canary-owner","login":"forged"}"#,
        r#"{"login":"canary-owner"}"#,
    ] {
        fixture.token(legacy());
        fixture.answer("GET", "/user", Answer::json(200, body));
        assert_eq!(
            account.complete(&callback(&account)).await,
            Err(GithubError::IdentityFailed)
        );
        assert!(!account.status().connected);
    }
    fixture.token(legacy());
    fixture.answer("GET", "/user", Answer::json(401, "canary-provider-error"));
    assert_eq!(
        account.complete(&callback(&account)).await,
        Err(GithubError::IdentityFailed)
    );
    drop(account);
    assert!(!profile.account(&fixture).status().connected);
}

#[tokio::test]
async fn legacy_nonexpiring_grant_restores_and_lost_or_malformed_grants_fail_closed() {
    let profile = Profile::new();
    let fixture = Fixture::new().await;
    let account = profile.account(&fixture);
    setup(&account);
    connect(&account, &fixture, legacy()).await;
    let stored: StoredGrant = account
        .read(GRANT_REF)
        .unwrap_or_else(|| panic!("saved grant"));
    assert_eq!(stored.expires_at, None);
    assert_eq!(stored.refresh_expires_at, None);
    assert_eq!(stored.refresh_token, None);
    drop(account);
    let account = profile.account(&fixture);
    assert!(account.status().connected);
    assert_eq!(must(account.access_token().await), ACCESS);
    assert_eq!(fixture.count(), 2);
    let reference = must(McpSecretRef::parse(GRANT_REF));
    must(mcp_secrets::remove(&profile.paths, &reference));
    assert_eq!(account.access_token().await, Err(GithubError::NotConnected));
    assert!(!account.status().connected);
    for value in ["{}", "not-json", r#"{"version":1,"access_token":"canary"}"#] {
        must(mcp_secrets::set(&profile.paths, &reference, value));
        assert_eq!(account.access_token().await, Err(GithubError::NotConnected));
    }
    assert_eq!(fixture.count(), 2);
}

#[tokio::test]
async fn app_secret_is_write_only_and_never_inherited_by_a_different_client() {
    let profile = Profile::new();
    let fixture = Fixture::new().await;
    let account = profile.account(&fixture);
    setup(&account);
    connect(&account, &fixture, legacy()).await;
    must(account.save_app(CLIENT, Some("  "), false));
    assert!(account.status().connected);
    assert!(account.status().client_secret_set);
    must(account.save_app(CLIENT, None, false));
    assert!(account.status().connected);
    let pending = callback(&account);
    for replacement in [None, Some(""), Some("  ")] {
        assert_eq!(
            account.save_app("canary-new-client", replacement, false),
            Err(GithubError::InvalidApp)
        );
        let status = account.status();
        assert!(
            status.connected && status.pending && status.client_secret_set && status.configured
        );
        assert_eq!(status.client_id.as_deref(), Some(CLIENT));
    }
    must(account.save_app("canary-new-client", Some("canary-new-secret"), false));
    assert_eq!(account.complete(&pending).await, Err(GithubError::Refused));
    assert!(account.status().configured);
    assert!(!account.status().connected && !account.status().pending);
    let raw = must(serde_json::to_string(&account.status()));
    for secret in [SECRET, "canary-new-secret", ACCESS, REFRESH] {
        assert!(!raw.contains(secret));
        assert!(!format!("{account:?}").contains(secret));
    }
    for error in [
        GithubError::ExchangeFailed,
        GithubError::ProviderError,
        GithubError::IdentityFailed,
        GithubError::Storage,
    ] {
        assert!(!format!("{error:?} {error}").contains("canary"));
    }
    assert_eq!(
        account.save_app("../canary", Some("canary"), false),
        Err(GithubError::InvalidApp)
    );
    assert_eq!(
        account.save_app(CLIENT, Some("has whitespace"), false),
        Err(GithubError::InvalidApp)
    );
    assert_eq!(fixture.count(), 2);
}

#[tokio::test]
async fn refresh_rotates_atomically_revalidates_identity_and_restores_after_restart() {
    let profile = Profile::new();
    let fixture = Fixture::new().await;
    let account = profile.account(&fixture);
    setup(&account);
    connect(&account, &fixture, expiring(ACCESS, REFRESH, 60)).await;
    fixture.token(expiring(
        "canary-access-rotated",
        "canary-refresh-rotated",
        28800,
    ));
    fixture.user();
    assert_eq!(must(account.access_token().await), "canary-access-rotated");
    let requests = fixture.recorded();
    let refresh = fields(&requests[2].body);
    assert_eq!(refresh["grant_type"], "refresh_token");
    assert_eq!(refresh["refresh_token"], REFRESH);
    assert_eq!(refresh["client_id"], CLIENT);
    assert_eq!(refresh["client_secret"], SECRET);
    assert!(!refresh.contains_key("scope"));
    let stored: StoredGrant = account
        .read(GRANT_REF)
        .unwrap_or_else(|| panic!("rotated grant"));
    assert_eq!(stored.access_token, "canary-access-rotated");
    assert_eq!(
        stored.refresh_token.as_deref(),
        Some("canary-refresh-rotated")
    );
    drop(account);
    let restored = profile.account(&fixture);
    assert_eq!(must(restored.access_token().await), "canary-access-rotated");
    assert_eq!(fixture.count(), 4);
}

#[tokio::test]
async fn refresh_error_ambiguous_pair_scope_change_or_identity_switch_requires_reauth() {
    let profile = Profile::new();
    let fixture = Fixture::new().await;
    for replacement in [
        serde_json::json!({ "error": "bad_refresh_token", "error_description": "canary-private" }),
        legacy(),
        expiring(ACCESS, REFRESH, 28800),
        serde_json::json!({ "access_token":"canary-next", "token_type":"bearer", "scope":"repo",
            "expires_in":28_800,"refresh_token":"canary-next-refresh","refresh_token_expires_in":15_897_600 }),
    ] {
        let account = profile.account(&fixture);
        setup(&account);
        connect(&account, &fixture, expiring(ACCESS, REFRESH, 60)).await;
        fixture.token(replacement);
        assert_eq!(
            account.access_token().await,
            Err(GithubError::SignInExpired)
        );
        assert!(!account.status().connected);
        assert_eq!(account.access_token().await, Err(GithubError::NotConnected));
        drop(account);
        assert!(!profile.account(&fixture).status().connected);
    }
    let account = profile.account(&fixture);
    setup(&account);
    connect(&account, &fixture, expiring(ACCESS, REFRESH, 60)).await;
    fixture.token(expiring("canary-next", "canary-next-refresh", 28800));
    fixture.answer(
        "GET",
        "/user",
        Answer::json(200, r#"{"id":999999,"login":"canary-other"}"#),
    );
    assert_eq!(
        account.access_token().await,
        Err(GithubError::SignInExpired)
    );
    assert!(!profile.account(&fixture).status().connected);
}

#[tokio::test]
async fn expired_access_with_valid_refresh_is_saved_connected_and_restorable() {
    let profile = Profile::new();
    let fixture = Fixture::new().await;
    let account = profile.account(&fixture);
    assert_eq!(
        account.save_app(CLIENT, None, false),
        Err(GithubError::InvalidApp)
    );
    setup(&account);
    connect(&account, &fixture, expiring(ACCESS, REFRESH, 60)).await;
    let mut stored: StoredGrant = account.read(GRANT_REF).unwrap_or_else(|| panic!("grant"));
    stored.expires_at = Some(now() - 10);
    stored.refresh_expires_at = Some(now() + 10_000);
    must(account.write(GRANT_REF, &stored));
    drop(account);
    let restored = profile.account(&fixture);
    assert!(restored.status().connected);
    assert_eq!(restored.status().login.as_deref(), Some("canary-owner"));
    assert_eq!(fixture.count(), 2);
    fixture.token(expiring(
        "canary-restored-access",
        "canary-restored-refresh",
        28_800,
    ));
    fixture.user();
    assert_eq!(
        must(restored.access_token().await),
        "canary-restored-access"
    );
    assert_eq!(fixture.count(), 4);
}

#[tokio::test]
async fn expired_refresh_sends_nothing_and_is_not_reported_connected() {
    let profile = Profile::new();
    let fixture = Fixture::new().await;
    let account = profile.account(&fixture);
    setup(&account);
    connect(&account, &fixture, expiring(ACCESS, REFRESH, 60)).await;
    let mut stored: StoredGrant = account.read(GRANT_REF).unwrap_or_else(|| panic!("grant"));
    stored.expires_at = Some(now() - 10);
    stored.refresh_expires_at = Some(now() - 1);
    must(account.write(GRANT_REF, &stored));
    assert!(!account.status().connected);
    assert_eq!(
        account.access_token().await,
        Err(GithubError::SignInExpired)
    );
    assert_eq!(fixture.count(), 2);
}

#[tokio::test]
async fn disconnect_clears_local_state_even_when_revocation_fails() {
    let profile = Profile::new();
    let fixture = Fixture::new().await;
    let account = profile.account(&fixture);
    setup(&account);
    connect(&account, &fixture, legacy()).await;
    let pending = callback(&account);
    fixture.answer(
        "DELETE",
        &format!("/applications/{CLIENT}/token"),
        Answer::json(503, "canary-raw-error"),
    );
    assert_eq!(
        account.disconnect().await,
        Err(GithubError::RevocationFailed)
    );
    assert!(!account.status().connected && !account.status().pending);
    assert_eq!(account.access_token().await, Err(GithubError::NotConnected));
    assert_eq!(account.complete(&pending).await, Err(GithubError::Refused));
    let requests = fixture.recorded();
    let revoke = &requests[2];
    assert!(
        revoke
            .head
            .starts_with(&format!("DELETE /applications/{CLIENT}/token "))
    );
    let encoded = base64::engine::general_purpose::STANDARD.encode(format!("{CLIENT}:{SECRET}"));
    assert!(
        revoke
            .head
            .to_lowercase()
            .contains(&format!("authorization: basic {}", encoded.to_lowercase()))
    );
    assert_eq!(
        must(serde_json::from_str::<serde_json::Value>(&revoke.body))["access_token"],
        ACCESS
    );
    drop(account);
    assert!(!profile.account(&fixture).status().connected);
}

#[tokio::test]
async fn complete_cannot_resurrect_access_after_app_change_or_disconnect() {
    for change_app in [false, true] {
        let profile = Profile::new();
        let fixture = Fixture::new().await;
        let account = profile.account(&fixture);
        setup(&account);
        let release = Arc::new(Notify::new());
        let mut answer = Answer::json(200, &legacy().to_string());
        answer.release = Some(Arc::clone(&release));
        fixture.answer("POST", "/token", answer);
        let target = callback(&account);
        let worker = Arc::clone(&account);
        let task = tokio::spawn(async move { worker.complete(&target).await });
        fixture.wait_for(1).await;
        let clock = MutationClock::pause();
        if change_app {
            must(account.save_app("canary-new-client", Some(SECRET), false));
        } else {
            disconnect_without_network(&account).await;
        }
        drop(clock);
        release.notify_one();
        assert_eq!(must(task.await), Err(GithubError::Refused));
        assert!(!account.status().connected);
        assert!(!profile.account(&fixture).status().connected);
        assert_eq!(fixture.count(), 1);
    }
}

#[tokio::test]
async fn refresh_is_serialized_and_cannot_resurrect_after_disconnect_or_app_change() {
    for change_app in [false, true] {
        let profile = Profile::new();
        let fixture = Fixture::new().await;
        let account = profile.account(&fixture);
        setup(&account);
        connect(&account, &fixture, expiring(ACCESS, REFRESH, 60)).await;
        let release = Arc::new(Notify::new());
        let mut answer = Answer::json(
            200,
            &expiring("canary-new-access", "canary-new-refresh", 28800).to_string(),
        );
        answer.release = Some(Arc::clone(&release));
        fixture.answer("POST", "/token", answer);
        let first = Arc::clone(&account);
        let first = tokio::spawn(async move { first.access_token().await });
        fixture.wait_for(3).await;
        let second = Arc::clone(&account);
        let second = tokio::spawn(async move { second.access_token().await });
        if change_app {
            must(account.save_app("canary-new-client", Some(SECRET), false));
        } else {
            must(account.disconnect().await);
        }
        assert!(!account.status().connected);
        release.notify_one();
        assert_eq!(must(first.await), Err(GithubError::SignInExpired));
        assert_eq!(must(second.await), Err(GithubError::NotConnected));
        assert!(!profile.account(&fixture).status().connected);
        assert_eq!(fixture.count(), 3);
    }
}

#[tokio::test]
async fn cancelled_refresh_cannot_reuse_old_refresh_token_after_restart() {
    let profile = Profile::new();
    let fixture = Fixture::new().await;
    let account = profile.account(&fixture);
    setup(&account);
    connect(&account, &fixture, expiring(ACCESS, REFRESH, 60)).await;
    let release = Arc::new(Notify::new());
    let mut answer = Answer::json(
        200,
        &expiring("canary-next", "canary-refresh-next", 28800).to_string(),
    );
    answer.release = Some(Arc::clone(&release));
    fixture.answer("POST", "/token", answer);
    let worker = Arc::clone(&account);
    let task = tokio::spawn(async move { worker.access_token().await });
    fixture.wait_for(3).await;
    task.abort();
    assert!(must(task.await.err().ok_or("expected cancellation")).is_cancelled());
    release.notify_one();
    assert_eq!(
        profile.account(&fixture).access_token().await,
        Err(GithubError::NotConnected)
    );
    assert_eq!(fixture.count(), 3);
}

#[tokio::test]
async fn oversized_fixed_and_chunked_responses_and_redirects_are_refused() {
    let profile = Profile::new();
    let fixture = Fixture::new().await;
    let account = profile.account(&fixture);
    setup(&account);
    for chunked in [false, true] {
        let mut answer = Answer::json(200, &"x".repeat(MAX_RESPONSE_BYTES + 1));
        answer.chunked = chunked;
        fixture.answer("POST", "/token", answer);
        assert_eq!(
            account.complete(&callback(&account)).await,
            Err(GithubError::ExchangeFailed)
        );
    }
    let mut answer = Answer::json(302, "{}");
    answer.headers = format!("Location: {}/must-not-follow\r\n", fixture.base);
    fixture.answer("POST", "/token", answer);
    assert_eq!(
        account.complete(&callback(&account)).await,
        Err(GithubError::ExchangeFailed)
    );
    fixture.token(legacy());
    fixture.answer(
        "GET",
        "/user",
        Answer::json(200, &"x".repeat(MAX_RESPONSE_BYTES + 1)),
    );
    assert_eq!(
        account.complete(&callback(&account)).await,
        Err(GithubError::IdentityFailed)
    );
    assert_eq!(fixture.count(), 5);
    assert!(!account.status().connected);
}

#[tokio::test]
async fn successful_concurrent_consumers_share_exactly_one_refresh() {
    let profile = Profile::new();
    let fixture = Fixture::new().await;
    let account = profile.account(&fixture);
    setup(&account);
    connect(&account, &fixture, expiring(ACCESS, REFRESH, 60)).await;
    let release = Arc::new(Notify::new());
    let mut answer = Answer::json(
        200,
        &expiring("canary-next-access", "canary-next-refresh", 28_800).to_string(),
    );
    answer.release = Some(Arc::clone(&release));
    fixture.answer("POST", "/token", answer);
    fixture.user();
    let worker = Arc::clone(&account);
    let first = tokio::spawn(async move { worker.access_token().await });
    fixture.wait_for(3).await;
    let worker = Arc::clone(&account);
    let second = tokio::spawn(async move { worker.access_token().await });
    release.notify_one();
    assert_eq!(must(must(first.await)), "canary-next-access");
    assert_eq!(must(must(second.await)), "canary-next-access");
    assert_eq!(fixture.count(), 4);
}

#[tokio::test]
async fn final_identity_await_cannot_resurrect_a_disconnected_account() {
    let profile = Profile::new();
    let fixture = Fixture::new().await;
    let account = profile.account(&fixture);
    setup(&account);
    fixture.token(legacy());
    let release = Arc::new(Notify::new());
    let mut answer = Answer::json(200, r#"{"id":123456,"login":"canary-owner"}"#);
    answer.release = Some(Arc::clone(&release));
    fixture.answer("GET", "/user", answer);
    let target = callback(&account);
    let worker = Arc::clone(&account);
    let task = tokio::spawn(async move { worker.complete(&target).await });
    fixture.wait_for(2).await;
    let clock = MutationClock::pause();
    disconnect_without_network(&account).await;
    drop(clock);
    release.notify_one();
    assert_eq!(must(task.await), Err(GithubError::Refused));
    assert!(!profile.account(&fixture).status().connected);
}

#[tokio::test]
async fn disconnect_stops_local_access_before_a_slow_revocation_finishes() {
    let profile = Profile::new();
    let fixture = Fixture::new().await;
    let account = profile.account(&fixture);
    setup(&account);
    connect(&account, &fixture, legacy()).await;
    let release = Arc::new(Notify::new());
    let mut answer = Answer::json(204, "");
    answer.release = Some(Arc::clone(&release));
    fixture.answer("DELETE", &format!("/applications/{CLIENT}/token"), answer);
    let worker = Arc::clone(&account);
    let mut disconnect = Box::pin(async move { worker.disconnect().await });
    // Finish synchronous local invalidation before starting the bounded HTTP
    // observation phase; the held revocation response must leave it pending.
    assert!(poll_once(disconnect.as_mut()).await.is_pending());
    let task = tokio::spawn(disconnect);
    fixture.wait_for(3).await;
    assert_eq!(account.access_token().await, Err(GithubError::NotConnected));
    assert!(!account.status().connected);
    assert!(!profile.account(&fixture).status().connected);
    release.notify_one();
    must(must(task.await));
}

#[tokio::test]
async fn wrong_persisted_app_binding_and_unreadable_grant_fail_closed_without_network() {
    let profile = Profile::new();
    let fixture = Fixture::new().await;
    let account = profile.account(&fixture);
    setup(&account);
    connect(&account, &fixture, legacy()).await;
    let mut stored: StoredGrant = account.read(GRANT_REF).unwrap_or_else(|| panic!("grant"));
    stored.app_binding = must(random_value());
    must(account.write(GRANT_REF, &stored));
    assert_eq!(account.access_token().await, Err(GithubError::NotConnected));
    assert!(!profile.account(&fixture).status().connected);
    let path = profile
        .paths
        .config()
        .join("mcp-keys")
        .join("github")
        .join("oauth-grant.key");
    must(fs::remove_file(&path));
    must(fs::create_dir(&path));
    assert_eq!(account.access_token().await, Err(GithubError::NotConnected));
    assert!(!account.status().connected);
    assert_eq!(fixture.count(), 2);
}

#[tokio::test]
async fn http_provider_binds_saved_and_rotated_tokens_and_observes_disconnect() {
    use jarvis_mcp_transport::HttpCredentialProvider;

    let profile = Profile::new();
    let fixture = Fixture::new().await;
    let account = profile.account(&fixture);
    setup(&account);
    connect(&account, &fixture, expiring(ACCESS, REFRESH, 28_800)).await;
    let provider = crate::mcp_credentials::ProfileHttpSecrets::with_account(Arc::clone(&account));
    let endpoint = crate::github_service::MCP_ENDPOINT;
    assert_eq!(
        must(provider.token("github", "github", endpoint).await),
        ACCESS
    );
    assert_eq!(fixture.count(), 2);

    let mut stored: StoredGrant = account
        .read(GRANT_REF)
        .unwrap_or_else(|| panic!("saved grant"));
    stored.expires_at = Some(now() - 10);
    must(account.write(GRANT_REF, &stored));
    let query_endpoint = format!("{endpoint}?canary=1");
    for (binding, server, endpoint) in [
        ("other", "github", endpoint),
        ("github", "other", endpoint),
        ("github", "github", "https://canary.invalid/mcp/readonly"),
        ("github", "github", query_endpoint.as_str()),
    ] {
        assert!(provider.token(binding, server, endpoint).await.is_err());
        assert_eq!(
            fixture.count(),
            2,
            "wrong binding must not initiate refresh"
        );
    }

    fixture.token(expiring(
        "canary-provider-rotated",
        "canary-provider-refresh",
        28_800,
    ));
    fixture.user();
    assert_eq!(
        must(provider.token("github", "github", endpoint).await),
        "canary-provider-rotated"
    );
    assert_eq!(fixture.count(), 4);
    assert_eq!(
        must(provider.token("github", "github", endpoint).await),
        "canary-provider-rotated"
    );
    assert_eq!(fixture.count(), 4);

    fixture.answer(
        "DELETE",
        &format!("/applications/{CLIENT}/token"),
        Answer::json(204, ""),
    );
    must(account.disconnect().await);
    assert!(provider.token("github", "github", endpoint).await.is_err());
    assert_eq!(fixture.count(), 5);
}

#[test]
fn shared_accounts_are_keyed_by_profile_and_do_not_keep_profiles_alive() {
    let first = Profile::new();
    let second = Profile::new();
    let one = must(GithubAccount::shared(&first.paths));
    let again = must(GithubAccount::shared(&first.paths));
    let other = must(GithubAccount::shared(&second.paths));
    assert!(Arc::ptr_eq(&one, &again));
    assert!(!Arc::ptr_eq(&one, &other));
    let weak = Arc::downgrade(&one);
    drop(one);
    drop(again);
    assert!(weak.upgrade().is_none());
    let _ = must(GithubAccount::shared(&first.paths));
}
