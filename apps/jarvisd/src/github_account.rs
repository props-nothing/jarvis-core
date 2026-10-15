//! Profile-owned GitHub OAuth credentials. Status describes saved metadata, not an online check.
//!
//! Wire contracts: `docs/research/integrations/github-oauth-settings.md`. All credential-bearing
//! requests use fixed production endpoints; alternate endpoints exist only in unit tests.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex as StdMutex, MutexGuard, OnceLock, Weak};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use jarvis_connectors::{
    AuthFlow, AuthMethod, AuthorizationTransaction, Callback, LoopbackHost, LoopbackRedirect,
    PkceMethod, PkceVerifier, SecretValue,
};
use jarvis_core::{McpSecretRef, SystemClock, UtcTimestamp};
use jarvis_storage::{AppPaths, mcp_secrets};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use thiserror::Error;
use tokio::sync::Mutex;

/// The origin-form path served by the daemon's loopback OAuth callback.
pub const CALLBACK_PATH: &str = "/oauth/github/callback";
const APP_REF: &str = "mcp:github:oauth-app";
const GRANT_REF: &str = "mcp:github:oauth-grant";
const PENDING_LIFETIME: Duration = Duration::from_secs(600);
const REFRESH_MARGIN: u64 = 60;
const HTTP_TIMEOUT: Duration = Duration::from_secs(20);
const MAX_RESPONSE_BYTES: usize = 32 * 1024;
const MAX_RECORD_BYTES: usize = 4096;
const MAX_TOKEN_BYTES: usize = 1024;
const API_VERSION: &str = "2026-03-10";

/// Fixed, value-free errors safe for Settings and diagnostics.
#[derive(Debug, Error, Eq, PartialEq)]
pub enum GithubError {
    /// The registered OAuth application's credentials are incomplete.
    #[error("GitHub is not set up; save the registered OAuth app client id and secret in Settings")]
    NotConfigured,
    /// No usable saved grant exists.
    #[error("GitHub is not connected; sign in from Settings")]
    NotConnected,
    /// Refresh failed or the saved grant is no longer usable.
    #[error("the GitHub sign-in is no longer usable; sign in again from Settings")]
    SignInExpired,
    /// A callback was not part of a current, one-use sign-in.
    #[error("that GitHub sign-in answer did not match a current sign-in started here")]
    Refused,
    /// Consent was denied or GitHub reported a browser-flow error.
    #[error("GitHub did not complete the sign-in")]
    ProviderError,
    /// The token endpoint did not return a valid bearer grant.
    #[error("GitHub refused the sign-in exchange or returned an invalid grant")]
    ExchangeFailed,
    /// The optional repository scope was not granted.
    #[error("GitHub did not grant the requested repository permissions")]
    ScopesMissing,
    /// The authenticated account could not be verified.
    #[error("GitHub did not return a valid authenticated account")]
    IdentityFailed,
    /// A credential-bearing request failed or exceeded its bounds.
    #[error("GitHub could not be reached or answered unexpectedly")]
    Unreachable,
    /// The private profile could not be managed securely.
    #[error("the GitHub sign-in could not be managed securely on this machine")]
    Storage,
    /// App settings do not fit the supported bounded credential format.
    #[error("the GitHub OAuth app settings are invalid")]
    InvalidApp,
    /// Secure random generation failed.
    #[error("the system could not produce a secure random value")]
    Random,
    /// Local access was removed, but server-side revocation was unsuccessful.
    #[error(
        "GitHub was disconnected locally, but token revocation failed; review authorized apps on GitHub"
    )]
    RevocationFailed,
}

/// Saved account metadata only. No secret or token is exposed.
#[allow(clippy::struct_excessive_bools)]
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct GithubStatus {
    /// Both app credentials are saved.
    pub configured: bool,
    /// A locally valid, app-bound grant is saved (not proof of current online validity).
    pub connected: bool,
    /// Login returned by GitHub's authenticated `/user` endpoint.
    pub login: Option<String>,
    /// The non-secret, registered app client id.
    pub client_id: Option<String>,
    /// Whether a write-only client secret is saved.
    pub client_secret_set: bool,
    /// Whether the owner opted into GitHub's broad `repo` scope.
    pub private_repositories: bool,
    /// An unexpired sign-in transaction is awaiting its browser answer.
    pub pending: bool,
}

// The secret and its binding are one atomic record, so changing apps can never inherit another app's secret.
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct App {
    version: u8,
    client_id: String,
    client_secret: Option<String>,
    private_repositories: bool,
    binding: String,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct StoredGrant {
    version: u8,
    app_binding: String,
    login: String,
    id: u64,
    scopes: Vec<String>,
    access_token: String,
    expires_at: Option<u64>,
    refresh_token: Option<String>,
    refresh_expires_at: Option<u64>,
}

struct Pending {
    transaction: AuthorizationTransaction,
    verifier: PkceVerifier,
    started: Instant,
    app_binding: String,
    epoch: u64,
}

#[derive(Default)]
struct Memory {
    epoch: u64,
    pending: Option<Pending>,
    grants_blocked: bool,
}

pub(crate) struct Endpoints {
    pub(crate) token: String,
    pub(crate) user: String,
    pub(crate) applications: String,
}

impl Endpoints {
    fn github() -> Self {
        Self {
            token: "https://github.com/login/oauth/access_token".to_owned(),
            user: "https://api.github.com/user".to_owned(),
            applications: "https://api.github.com/applications".to_owned(),
        }
    }
}

#[derive(Deserialize)]
struct TokenReply {
    access_token: String,
    token_type: String,
    scope: String,
    expires_in: Option<u64>,
    refresh_token: Option<String>,
    refresh_token_expires_in: Option<u64>,
    // Even an HTTP 200 with a token and an error is a refusal. Keep the untrusted value unformatted.
    error: Option<serde_json::Value>,
}

#[derive(Deserialize)]
struct UserReply {
    id: u64,
    login: String,
}

/// One account service per active profile, shared by Settings and token consumers.
///
/// Synchronous settings mutations hold `memory` only. Network operations serialize through `operation`,
/// release `memory` before awaiting, and must revalidate the epoch before saving or returning a token.
pub struct GithubAccount {
    paths: AppPaths,
    client: reqwest::Client,
    endpoints: Endpoints,
    memory: StdMutex<Memory>,
    operation: Mutex<()>,
}

impl std::fmt::Debug for GithubAccount {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("GithubAccount")
            .finish_non_exhaustive()
    }
}

impl GithubAccount {
    /// Builds a service using only GitHub's fixed HTTPS endpoints.
    ///
    /// # Errors
    /// Returns a fixed error when the HTTP client cannot be constructed.
    pub fn new(paths: AppPaths) -> Result<Self, GithubError> {
        Self::build(paths, Endpoints::github())
    }

    /// Returns the same service for all consumers of a profile's configuration path.
    ///
    /// # Errors
    /// As [`Self::new`]. The registry holds weak references, not accounts from abandoned profiles.
    pub fn shared(paths: &AppPaths) -> Result<Arc<Self>, GithubError> {
        static ACCOUNTS: OnceLock<StdMutex<HashMap<PathBuf, Weak<GithubAccount>>>> =
            OnceLock::new();
        let mut accounts = ACCOUNTS
            .get_or_init(StdMutex::default)
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        accounts.retain(|_, account| account.strong_count() > 0);
        if let Some(account) = accounts.get(paths.config()).and_then(Weak::upgrade) {
            return Ok(account);
        }
        let account = Arc::new(Self::new(paths.clone())?);
        accounts.insert(paths.config().to_owned(), Arc::downgrade(&account));
        Ok(account)
    }

    fn build(paths: AppPaths, endpoints: Endpoints) -> Result<Self, GithubError> {
        let client = reqwest::Client::builder()
            .timeout(HTTP_TIMEOUT)
            .connect_timeout(HTTP_TIMEOUT)
            .redirect(reqwest::redirect::Policy::none())
            .no_proxy()
            .retry(reqwest::retry::never())
            .user_agent("JARVIS-GitHub-OAuth/1")
            .build()
            .map_err(|_| GithubError::Unreachable)?;
        Ok(Self {
            paths,
            client,
            endpoints,
            memory: StdMutex::new(Memory::default()),
            operation: Mutex::new(()),
        })
    }

    #[cfg(test)]
    pub(crate) fn with_endpoints(
        paths: AppPaths,
        endpoints: Endpoints,
    ) -> Result<Self, GithubError> {
        Self::build(paths, endpoints)
    }

    fn memory(&self) -> MutexGuard<'_, Memory> {
        self.memory
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Returns validated saved metadata, without any provider request.
    #[must_use]
    pub fn status(&self) -> GithubStatus {
        let memory = self.memory();
        let app = self.app();
        let grant = app.as_ref().and_then(|app| self.grant(app, &memory));
        let connected = grant.as_ref().is_some_and(|grant| usable(grant, now()));
        GithubStatus {
            configured: app.as_ref().is_some_and(|app| app.client_secret.is_some()),
            connected,
            login: grant.filter(|_| connected).map(|grant| grant.login),
            client_id: app.as_ref().map(|app| app.client_id.clone()),
            client_secret_set: app.as_ref().is_some_and(|app| app.client_secret.is_some()),
            private_repositories: app.as_ref().is_some_and(|app| app.private_repositories),
            pending: memory.pending.as_ref().is_some_and(|pending| {
                pending.started.elapsed() < PENDING_LIFETIME
                    && !pending
                        .transaction
                        .is_expired(UtcTimestamp::now(&SystemClock), 600)
                    && app
                        .as_ref()
                        .is_some_and(|app| app.binding == pending.app_binding)
                    && pending.epoch == memory.epoch
            }),
        }
    }

    /// Saves the owner's registered app. Empty/absent secrets preserve only the same client's secret.
    /// A new client requires its own nonempty secret; a refused save leaves the current app unchanged.
    /// App, scope or secret changes invalidate the previous grant and pending transaction.
    ///
    /// # Errors
    /// Returns a fixed validation, randomness or private-storage error; never echoes the input.
    pub fn save_app(
        &self,
        client_id: &str,
        client_secret: Option<&str>,
        private_repositories: bool,
    ) -> Result<(), GithubError> {
        let client_id = client_id.trim();
        if !valid_client_id(client_id) {
            return Err(GithubError::InvalidApp);
        }
        let replacement = client_secret
            .map(str::trim)
            .filter(|secret| !secret.is_empty());
        if replacement.is_some_and(|secret| !graphic(secret, MAX_TOKEN_BYTES)) {
            return Err(GithubError::InvalidApp);
        }
        let mut memory = self.memory();
        let old = self.app();
        let secret = replacement.map(str::to_owned).or_else(|| {
            old.as_ref()
                .filter(|app| app.client_id == client_id)
                .and_then(|app| app.client_secret.clone())
        });
        if secret.is_none() {
            return Err(GithubError::InvalidApp);
        }
        if old.as_ref().is_some_and(|app| {
            app.client_id == client_id
                && app.client_secret == secret
                && app.private_repositories == private_repositories
        }) {
            return Ok(());
        }
        let app = App {
            version: 1,
            client_id: client_id.to_owned(),
            client_secret: secret,
            private_repositories,
            binding: random_value()?,
        };
        invalidate(&mut memory);
        // Change the binding first: even a failed grant deletion cannot make an old grant valid after restart.
        let saved = self.write(APP_REF, &app);
        let removed = self.remove_grant();
        saved.and(removed)
    }

    /// Starts a ten-minute, one-use S256 transaction on the daemon's exact IPv4 loopback callback.
    ///
    /// # Errors
    /// Returns a fixed setup, random-generation or transaction-validation error.
    pub fn begin(&self, port: u16) -> Result<String, GithubError> {
        let mut memory = self.memory();
        let app = self.configured_app()?;
        let redirect = LoopbackRedirect::listening(LoopbackHost::V4, port, CALLBACK_PATH)
            .map_err(|_| GithubError::Refused)?;
        let flow = AuthFlow::new(
            AuthMethod::OAuthPkce,
            Some(PkceMethod::S256),
            Some(format!("http://127.0.0.1{CALLBACK_PATH}")),
            "https://github.com/login/oauth/authorize",
        )
        .map_err(|_| GithubError::Refused)?;
        let verifier = PkceVerifier::generate().map_err(|_| GithubError::Random)?;
        let state = SecretValue::new(random_value()?).map_err(|_| GithubError::Random)?;
        let transaction = AuthorizationTransaction::begin(
            &flow,
            verifier.clone(),
            state,
            None,
            redirect,
            UtcTimestamp::now(&SystemClock),
        )
        .map_err(|_| GithubError::Refused)?;
        let mut scopes = vec!["offline_access".to_owned()];
        if app.private_repositories {
            scopes.push("repo".to_owned());
        }
        let parameters = transaction.parameters(&app.client_id, &scopes);
        let url = reqwest::Url::parse_with_params(
            "https://github.com/login/oauth/authorize",
            parameters.iter().map(|(key, value)| (*key, value.as_str())),
        )
        .map_err(|_| GithubError::Refused)?;
        memory.epoch = memory.epoch.wrapping_add(1);
        memory.pending = Some(Pending {
            transaction,
            verifier,
            started: Instant::now(),
            app_binding: app.binding,
            epoch: memory.epoch,
        });
        Ok(url.to_string())
    }

    /// Consumes the browser answer, exchanges its code, verifies `/user`, then saves one atomic grant.
    /// Browser-provided labels and provider error bodies are never trusted or exposed.
    ///
    /// # Errors
    /// Returns a fixed callback, token, scope, identity or storage error.
    pub async fn complete(&self, target: &str) -> Result<(), GithubError> {
        let _operation = self.operation.lock().await;
        let (pending, app) = {
            let mut memory = self.memory();
            let pending = memory.pending.take().ok_or(GithubError::Refused)?;
            let app = self.configured_app()?;
            if pending.epoch != memory.epoch || pending.app_binding != app.binding {
                return Err(GithubError::Refused);
            }
            (pending, app)
        };
        if pending.started.elapsed() >= PENDING_LIFETIME
            || pending
                .transaction
                .is_expired(UtcTimestamp::now(&SystemClock), 600)
            || target.len() > 8192
            || target.contains('#')
            || target.split('?').next() != Some(CALLBACK_PATH)
        {
            return Err(GithubError::Refused);
        }
        let callback = Callback::from_request_target(target).map_err(|_| GithubError::Refused)?;
        // The connector checks provider errors first; here even an error must prove its state first.
        if !callback
            .state
            .as_deref()
            .is_some_and(|state| pending.transaction.state().matches(state))
        {
            return Err(GithubError::Refused);
        }
        if callback.error.is_some() {
            return Err(GithubError::ProviderError);
        }
        let grant = pending
            .transaction
            .consume(&callback)
            .map_err(|_| GithubError::Refused)?;
        if !graphic(grant.code.expose(), 2048) {
            return Err(GithubError::Refused);
        }
        let started = now();
        let reply = self
            .token_request(&[
                ("client_id", app.client_id.clone()),
                (
                    "client_secret",
                    app.client_secret
                        .clone()
                        .ok_or(GithubError::NotConfigured)?,
                ),
                ("code", grant.code.expose().to_owned()),
                ("redirect_uri", grant.redirect.as_uri()),
                ("code_verifier", pending.verifier.expose().to_owned()),
            ])
            .await?;
        let mut stored = token_grant(reply, &app, started)?;
        self.check_epoch(pending.epoch, &app)?;
        let user = self.user(&stored.access_token).await?;
        stored.id = user.id;
        stored.login = user.login;
        let mut memory = self.memory();
        self.check_epoch_locked(&memory, pending.epoch, &app)?;
        self.write(GRANT_REF, &stored)?;
        memory.grants_blocked = false;
        Ok(())
    }

    /// Restores a validated app-bound grant, refreshing near expiry and durably saving the rotated pair.
    /// Refresh failure or cancellation cannot leave potentially consumed refresh material reusable.
    ///
    /// # Errors
    /// Returns a fixed disconnected, reauthentication, network or storage error.
    pub async fn access_token(&self) -> Result<String, GithubError> {
        let _operation = self.operation.lock().await;
        let (app, old, epoch) = {
            let mut memory = self.memory();
            let app = self.configured_app()?;
            let grant = self.grant(&app, &memory).ok_or(GithubError::NotConnected)?;
            let current = now();
            if grant
                .expires_at
                .is_none_or(|expiry| expiry > current.saturating_add(REFRESH_MARGIN))
            {
                return Ok(grant.access_token);
            }
            if !grant.refresh_token.as_ref().is_some_and(|_| {
                grant
                    .refresh_expires_at
                    .is_some_and(|expiry| expiry > current)
            }) {
                invalidate(&mut memory);
                self.remove_grant()?;
                return Err(GithubError::SignInExpired);
            }
            // Before a request can consume the refresh token, remove it durably. This also covers dropped futures.
            memory.grants_blocked = true;
            self.remove_grant()?;
            (app, grant, memory.epoch)
        };
        let started = now();
        let refreshed = async {
            let reply = self
                .token_request(&[
                    ("client_id", app.client_id.clone()),
                    (
                        "client_secret",
                        app.client_secret
                            .clone()
                            .ok_or(GithubError::NotConfigured)?,
                    ),
                    ("grant_type", "refresh_token".to_owned()),
                    (
                        "refresh_token",
                        old.refresh_token
                            .clone()
                            .ok_or(GithubError::SignInExpired)?,
                    ),
                ])
                .await?;
            let mut grant = token_grant(reply, &app, started)?;
            if grant.refresh_token.is_none()
                || grant.expires_at.is_none()
                || grant.access_token == old.access_token
                || grant.refresh_token == old.refresh_token
                || grant.scopes != old.scopes
            {
                return Err(GithubError::SignInExpired);
            }
            self.check_epoch(epoch, &app)?;
            let user = self.user(&grant.access_token).await?;
            if user.id != old.id {
                return Err(GithubError::IdentityFailed);
            }
            grant.login = user.login;
            grant.id = user.id;
            Ok(grant)
        }
        .await
        .map_err(|_| GithubError::SignInExpired)?;
        let mut memory = self.memory();
        self.check_epoch_locked(&memory, epoch, &app)?;
        self.write(GRANT_REF, &refreshed)?;
        memory.grants_blocked = false;
        Ok(refreshed.access_token)
    }

    /// Immediately invalidates local access, then best-effort revokes the token with app Basic auth.
    /// Network failure never restores the local grant or pending transaction.
    ///
    /// # Errors
    /// Returns a fixed storage or revocation error, including failed server-side revocation.
    pub async fn disconnect(&self) -> Result<(), GithubError> {
        let (app, grant, local_result) = {
            let mut memory = self.memory();
            let mut app = self.app();
            let grant = app.as_ref().and_then(|app| self.grant(app, &memory));
            invalidate(&mut memory);
            let changed_binding = if let Some(app) = app.as_mut() {
                match random_value() {
                    Ok(binding) => {
                        app.binding = binding;
                        self.write(APP_REF, app)
                    }
                    Err(error) => Err(error),
                }
            } else {
                Ok(())
            };
            let removed = self.remove_grant();
            (app, grant, changed_binding.and(removed))
        };
        let revoked = if let (Some(app), Some(grant)) = (app, grant) {
            self.revoke(&app, &grant.access_token).await
        } else {
            Ok(())
        };
        local_result.and(revoked)
    }

    fn app(&self) -> Option<App> {
        let app: App = self.read(APP_REF)?;
        (app.version == 1
            && valid_client_id(&app.client_id)
            && app
                .client_secret
                .as_deref()
                .is_none_or(|secret| graphic(secret, MAX_TOKEN_BYTES))
            && valid_binding(&app.binding))
        .then_some(app)
    }

    fn configured_app(&self) -> Result<App, GithubError> {
        self.app()
            .filter(|app| app.client_secret.is_some())
            .ok_or(GithubError::NotConfigured)
    }

    fn grant(&self, app: &App, memory: &Memory) -> Option<StoredGrant> {
        if memory.grants_blocked {
            return None;
        }
        let grant: StoredGrant = self.read(GRANT_REF)?;
        (grant.version == 1
            && grant.app_binding == app.binding
            && valid_user(grant.id, &grant.login)
            && graphic(&grant.access_token, MAX_TOKEN_BYTES)
            && valid_token_times(&grant)
            && valid_scopes(&grant.scopes, app.private_repositories))
        .then_some(grant)
    }

    fn read<T: DeserializeOwned>(&self, reference: &str) -> Option<T> {
        let reference = McpSecretRef::parse(reference).ok()?;
        let raw = mcp_secrets::resolve(&self.paths, &reference).ok()?;
        serde_json::from_str(&raw).ok()
    }

    fn write<T: Serialize>(&self, reference: &str, value: &T) -> Result<(), GithubError> {
        let reference = McpSecretRef::parse(reference).map_err(|_| GithubError::Storage)?;
        let raw = serde_json::to_string(value).map_err(|_| GithubError::Storage)?;
        if raw.len() > MAX_RECORD_BYTES || !raw.bytes().all(|byte| byte.is_ascii_graphic()) {
            return Err(GithubError::Storage);
        }
        mcp_secrets::set(&self.paths, &reference, &raw).map_err(|_| GithubError::Storage)
    }

    fn remove_grant(&self) -> Result<(), GithubError> {
        let reference = McpSecretRef::parse(GRANT_REF).map_err(|_| GithubError::Storage)?;
        mcp_secrets::remove(&self.paths, &reference).map_err(|_| GithubError::Storage)
    }

    fn check_epoch(&self, epoch: u64, app: &App) -> Result<(), GithubError> {
        self.check_epoch_locked(&self.memory(), epoch, app)
    }

    fn check_epoch_locked(
        &self,
        memory: &Memory,
        epoch: u64,
        app: &App,
    ) -> Result<(), GithubError> {
        if memory.epoch != epoch
            || self
                .app()
                .is_none_or(|current| current.binding != app.binding)
        {
            return Err(GithubError::Refused);
        }
        Ok(())
    }

    async fn token_request(&self, fields: &[(&str, String)]) -> Result<TokenReply, GithubError> {
        let owned: Vec<_> = fields
            .iter()
            .map(|(key, value)| ((*key).to_owned(), value.clone()))
            .collect();
        let response = self
            .client
            .post(&self.endpoints.token)
            .header(reqwest::header::ACCEPT, "application/json")
            .header(
                reqwest::header::CONTENT_TYPE,
                "application/x-www-form-urlencoded",
            )
            .body(jarvis_connectors::form::encode_body(&owned))
            .send()
            .await
            .map_err(|_| GithubError::Unreachable)?;
        if response.status() != reqwest::StatusCode::OK {
            return Err(GithubError::ExchangeFailed);
        }
        let reply: TokenReply = bounded_json(response)
            .await
            .map_err(|_| GithubError::ExchangeFailed)?;
        if reply.error.is_some() {
            return Err(GithubError::ExchangeFailed);
        }
        Ok(reply)
    }

    async fn user(&self, token: &str) -> Result<UserReply, GithubError> {
        let response = self
            .client
            .get(&self.endpoints.user)
            .bearer_auth(token)
            .header(reqwest::header::ACCEPT, "application/vnd.github+json")
            .header("X-GitHub-Api-Version", API_VERSION)
            .send()
            .await
            .map_err(|_| GithubError::IdentityFailed)?;
        if response.status() != reqwest::StatusCode::OK {
            return Err(GithubError::IdentityFailed);
        }
        let user: UserReply = bounded_json(response)
            .await
            .map_err(|_| GithubError::IdentityFailed)?;
        if !valid_user(user.id, &user.login) {
            return Err(GithubError::IdentityFailed);
        }
        Ok(user)
    }

    async fn revoke(&self, app: &App, token: &str) -> Result<(), GithubError> {
        let secret = app
            .client_secret
            .as_deref()
            .ok_or(GithubError::RevocationFailed)?;
        let response = self
            .client
            .delete(format!(
                "{}/{}/token",
                self.endpoints.applications, app.client_id
            ))
            .basic_auth(&app.client_id, Some(secret))
            .header(reqwest::header::ACCEPT, "application/vnd.github+json")
            .header("X-GitHub-Api-Version", API_VERSION)
            .json(&serde_json::json!({ "access_token": token }))
            .send()
            .await
            .map_err(|_| GithubError::RevocationFailed)?;
        if response.status() != reqwest::StatusCode::NO_CONTENT {
            return Err(GithubError::RevocationFailed);
        }
        Ok(())
    }
}

fn invalidate(memory: &mut Memory) {
    memory.epoch = memory.epoch.wrapping_add(1);
    memory.pending = None;
    memory.grants_blocked = true;
}

fn random_value() -> Result<String, GithubError> {
    PkceVerifier::generate()
        .map(|value| value.expose().to_owned())
        .map_err(|_| GithubError::Random)
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(u64::MAX, |elapsed| elapsed.as_secs())
}

fn graphic(value: &str, maximum: usize) -> bool {
    !value.is_empty() && value.len() <= maximum && value.bytes().all(|byte| byte.is_ascii_graphic())
}

fn valid_client_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
}

fn valid_binding(value: &str) -> bool {
    value.len() == 43
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
}

fn valid_user(id: u64, login: &str) -> bool {
    id > 0
        && i64::try_from(id).is_ok()
        && !login.is_empty()
        && login.len() <= 39
        && login
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        && !login.starts_with('-')
        && !login.ends_with('-')
        && !login.contains("--")
}

fn valid_scopes(scopes: &[String], private: bool) -> bool {
    scopes.len() <= 32
        && scopes.iter().all(|scope| {
            !scope.is_empty()
                && scope.len() <= 64
                && scope.bytes().all(|byte| {
                    byte.is_ascii_lowercase() || matches!(byte, b'0'..=b'9' | b':' | b'_' | b'-')
                })
        })
        && (!private || scopes.iter().any(|scope| scope == "repo"))
}

fn valid_token_times(grant: &StoredGrant) -> bool {
    match (
        &grant.refresh_token,
        grant.expires_at,
        grant.refresh_expires_at,
    ) {
        (None, None, None) => true,
        (Some(token), Some(access), Some(refresh)) => {
            graphic(token, MAX_TOKEN_BYTES) && access > 0 && refresh > access
        }
        _ => false,
    }
}

fn usable(grant: &StoredGrant, current: u64) -> bool {
    grant.expires_at.is_none_or(|expiry| expiry > current)
        || (grant.refresh_token.is_some()
            && grant
                .refresh_expires_at
                .is_some_and(|expiry| expiry > current))
}

fn token_grant(reply: TokenReply, app: &App, issued: u64) -> Result<StoredGrant, GithubError> {
    if !reply.token_type.eq_ignore_ascii_case("bearer")
        || !graphic(&reply.access_token, MAX_TOKEN_BYTES)
    {
        return Err(GithubError::ExchangeFailed);
    }
    let mut scopes: Vec<_> = reply
        .scope
        .split(|ch: char| ch == ',' || ch.is_ascii_whitespace())
        .filter(|scope| !scope.is_empty())
        .map(str::to_owned)
        .collect();
    scopes.sort_unstable();
    scopes.dedup();
    // GitHub's offline_access is an expiry opt-in, not a normal returned scope. Empty scope is valid for /user.
    if !valid_scopes(&scopes, app.private_repositories) {
        return Err(GithubError::ScopesMissing);
    }
    let expiry = |seconds: Option<u64>| -> Result<Option<u64>, GithubError> {
        seconds
            .map(|seconds| {
                if seconds == 0 {
                    return Err(GithubError::ExchangeFailed);
                }
                issued
                    .checked_add(seconds)
                    .ok_or(GithubError::ExchangeFailed)
            })
            .transpose()
    };
    let grant = StoredGrant {
        version: 1,
        app_binding: app.binding.clone(),
        login: String::new(),
        id: 0,
        scopes,
        access_token: reply.access_token,
        expires_at: expiry(reply.expires_in)?,
        refresh_token: reply.refresh_token,
        refresh_expires_at: expiry(reply.refresh_token_expires_in)?,
    };
    if !valid_token_times(&grant) || grant.expires_at.is_some_and(|expiry| expiry <= now()) {
        return Err(GithubError::ExchangeFailed);
    }
    Ok(grant)
}

async fn bounded_json<T: DeserializeOwned>(
    mut response: reqwest::Response,
) -> Result<T, GithubError> {
    if response
        .content_length()
        .is_some_and(|length| length > MAX_RESPONSE_BYTES as u64)
    {
        return Err(GithubError::Unreachable);
    }
    let mut body = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| GithubError::Unreachable)?
    {
        if chunk.len() > MAX_RESPONSE_BYTES.saturating_sub(body.len()) {
            return Err(GithubError::Unreachable);
        }
        body.extend_from_slice(&chunk);
    }
    serde_json::from_slice(&body).map_err(|_| GithubError::Unreachable)
}

#[cfg(test)]
#[path = "github_account_tests.rs"]
mod tests;
