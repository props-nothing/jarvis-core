//! Persistent pre-registered MCP OAuth clients; token consumers and setup share one profile service.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex as StdMutex, MutexGuard, OnceLock, Weak};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use jarvis_connectors::{
    AuthFlow, AuthMethod, AuthorizationTransaction, Callback, LoopbackHost, LoopbackRedirect,
    PkceMethod, PkceVerifier, SecretValue,
};
use jarvis_core::{McpSecretRef, SystemClock, UtcTimestamp};
use jarvis_storage::{AppPaths, mcp_secrets};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use tokio::sync::Mutex;

use crate::mcp_oauth_wire::{Client, Metadata, OAuthError, TokenReply, Wire};

const SETUP_KEY: &str = "oauth-client";
const GRANT_KEY: &str = "oauth-grant";
const MAX_TOKEN: usize = 1024;

pub(crate) fn callback_path(server: &str) -> String {
    format!("/oauth/mcp/{server}/callback")
}

#[derive(Serialize)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "status reports independent saved setup, grant, pending consent and secret-presence facts"
)]
pub(crate) struct Status {
    pub configured: bool,
    pub connected: bool,
    pub pending: bool,
    pub issuer: Option<String>,
    pub client_id: Option<String>,
    pub client_secret_set: bool,
    pub scopes: Vec<String>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Setup {
    version: u8,
    resource: String,
    issuer: String,
    client: Client,
    scopes: Vec<String>,
    binding: String,
    metadata: Option<Metadata>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Grant {
    version: u8,
    binding: String,
    access_token: String,
    refresh_token: Option<String>,
    expires_at: Option<u64>,
    refresh_expires_at: Option<u64>,
    scopes: Vec<String>,
}

struct Pending {
    transaction: AuthorizationTransaction,
    verifier: PkceVerifier,
    started: Instant,
    binding: String,
    epoch: u64,
}

#[derive(Default)]
struct Memory {
    epoch: u64,
    pending: Option<Pending>,
    blocked: bool,
}

pub(crate) struct Account {
    paths: AppPaths,
    server: String,
    wire: Wire,
    memory: StdMutex<Memory>,
    operation: Mutex<()>,
}

pub(crate) struct Accounts {
    paths: AppPaths,
    accounts: StdMutex<HashMap<String, Arc<Account>>>,
}

impl Accounts {
    pub(crate) fn shared(paths: &AppPaths) -> Arc<Self> {
        static PROFILES: OnceLock<StdMutex<HashMap<PathBuf, Weak<Accounts>>>> = OnceLock::new();
        let mut profiles = PROFILES
            .get_or_init(StdMutex::default)
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        profiles.retain(|_, profile| profile.strong_count() > 0);
        if let Some(profile) = profiles.get(paths.config()).and_then(Weak::upgrade) {
            return profile;
        }
        let profile = Arc::new(Self {
            paths: paths.clone(),
            accounts: StdMutex::new(HashMap::new()),
        });
        profiles.insert(paths.config().to_owned(), Arc::downgrade(&profile));
        profile
    }

    pub(crate) fn account(&self, server: &str) -> Result<Arc<Account>, OAuthError> {
        McpSecretRef::parse(&format!("mcp:{server}:{SETUP_KEY}"))
            .map_err(|_| OAuthError::InvalidSetup)?;
        let mut accounts = self
            .accounts
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(account) = accounts.get(server) {
            return Ok(Arc::clone(account));
        }
        if accounts.len() >= 16 {
            return Err(OAuthError::InvalidSetup);
        }
        let account = Arc::new(Account::build(self.paths.clone(), server, Wire::new()?));
        accounts.insert(server.to_owned(), Arc::clone(&account));
        Ok(account)
    }
}

impl Account {
    fn build(paths: AppPaths, server: &str, wire: Wire) -> Self {
        Self {
            paths,
            server: server.to_owned(),
            wire,
            memory: StdMutex::new(Memory::default()),
            operation: Mutex::new(()),
        }
    }

    #[cfg(test)]
    pub(crate) fn loopback(paths: AppPaths, server: &str) -> Self {
        Self::build(
            paths,
            server,
            Wire::loopback().unwrap_or_else(|_| panic!("test wire")),
        )
    }

    fn memory(&self) -> MutexGuard<'_, Memory> {
        self.memory
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    pub(crate) fn status(&self, resource: &str) -> Status {
        let memory = self.memory();
        let setup = self.setup(resource);
        let grant = setup.as_ref().and_then(|setup| self.grant(setup, &memory));
        Status {
            configured: setup.is_some(),
            connected: grant.is_some_and(|grant| {
                grant.expires_at.is_none_or(|time| time > now()) || refresh_usable(&grant)
            }),
            pending: memory.pending.as_ref().is_some_and(|pending| {
                pending.epoch == memory.epoch
                    && pending.started.elapsed().as_secs() < 600
                    && setup
                        .as_ref()
                        .is_some_and(|setup| setup.binding == pending.binding)
            }),
            issuer: setup.as_ref().map(|setup| setup.issuer.clone()),
            client_id: setup.as_ref().map(|setup| setup.client.client_id.clone()),
            client_secret_set: setup
                .as_ref()
                .is_some_and(|setup| setup.client.client_secret.is_some()),
            scopes: setup.map_or_else(Vec::new, |setup| setup.scopes),
        }
    }

    pub(crate) fn save_setup(
        &self,
        resource: &str,
        issuer: &str,
        client_id: &str,
        secret: Option<&str>,
        scopes: &[String],
    ) -> Result<(), OAuthError> {
        self.wire.validate_url(resource)?;
        self.wire.validate_url(issuer)?;
        if !graphic(client_id, 256)
            || !valid_scopes(scopes)
            || secret.is_some_and(|value| !value.is_empty() && !graphic(value, MAX_TOKEN))
        {
            return Err(OAuthError::InvalidSetup);
        }
        let mut memory = self.memory();
        let old = self.setup(resource);
        let same_client = old
            .as_ref()
            .filter(|old| old.issuer == issuer && old.client.client_id == client_id);
        let client_secret = secret
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
            .or_else(|| same_client.and_then(|old| old.client.client_secret.clone()));
        if old.as_ref().is_some_and(|old| {
            old.issuer == issuer
                && old.client.client_id == client_id
                && old.client.client_secret == client_secret
                && old.scopes == scopes
        }) {
            return Ok(());
        }
        let setup = Setup {
            version: 1,
            resource: resource.to_owned(),
            issuer: issuer.to_owned(),
            client: Client {
                client_id: client_id.to_owned(),
                client_secret,
            },
            scopes: scopes.to_vec(),
            binding: random()?,
            metadata: None,
        };
        invalidate(&mut memory);
        let saved = self.write(SETUP_KEY, &setup);
        let removed = self.remove_grant();
        saved.and(removed)
    }

    pub(crate) async fn begin(&self, resource: &str, port: u16) -> Result<String, OAuthError> {
        let _operation = self.operation.lock().await;
        let (mut setup, epoch) = {
            let mut memory = self.memory();
            let setup = self.setup(resource).ok_or(OAuthError::InvalidSetup)?;
            invalidate(&mut memory);
            self.remove_grant()?;
            (setup, memory.epoch)
        };
        let metadata = self.wire.discover(resource, &setup.issuer).await?;
        let path = callback_path(&self.server);
        let redirect = LoopbackRedirect::listening(LoopbackHost::V4, port, &path)
            .map_err(|_| OAuthError::InvalidSetup)?;
        let flow = AuthFlow::new(
            AuthMethod::OAuthPkce,
            Some(PkceMethod::S256),
            Some(format!("http://127.0.0.1{path}")),
            &metadata.authorization_endpoint,
        )
        .map_err(|_| OAuthError::InvalidSetup)?;
        let verifier = PkceVerifier::generate().map_err(|_| OAuthError::Refused)?;
        let transaction = AuthorizationTransaction::begin(
            &flow,
            verifier.clone(),
            SecretValue::new(random()?).map_err(|_| OAuthError::Refused)?,
            None,
            redirect,
            UtcTimestamp::now(&SystemClock),
        )
        .map_err(|_| OAuthError::Refused)?;
        let mut url = reqwest::Url::parse(&metadata.authorization_endpoint)
            .map_err(|_| OAuthError::Discovery)?;
        url.query_pairs_mut()
            .extend_pairs(transaction.parameters(&setup.client.client_id, &setup.scopes))
            .append_pair("resource", resource);
        setup.metadata = Some(metadata);
        let mut memory = self.memory();
        self.check(&memory, epoch, &setup)?;
        self.write(SETUP_KEY, &setup)?;
        memory.pending = Some(Pending {
            transaction,
            verifier,
            started: Instant::now(),
            binding: setup.binding,
            epoch,
        });
        Ok(url.to_string())
    }

    pub(crate) async fn complete(&self, resource: &str, target: &str) -> Result<(), OAuthError> {
        let callback = self.parse_callback(target)?;
        let _operation = self.operation.lock().await;
        let (pending, setup) = {
            let mut memory = self.memory();
            let pending = memory.pending.take().ok_or(OAuthError::Refused)?;
            let setup = self.setup(resource).ok_or(OAuthError::Refused)?;
            self.check(&memory, pending.epoch, &setup)?;
            if pending.binding != setup.binding {
                return Err(OAuthError::Refused);
            }
            (pending, setup)
        };
        let metadata = setup.metadata.as_ref().ok_or(OAuthError::Refused)?;
        if pending.started.elapsed().as_secs() >= 600
            || pending
                .transaction
                .is_expired(UtcTimestamp::now(&SystemClock), 600)
            || !callback
                .state
                .as_deref()
                .is_some_and(|state| pending.transaction.state().matches(state))
            || callback
                .issuer
                .as_ref()
                .is_some_and(|issuer| issuer != &setup.issuer)
            || (metadata.authorization_response_iss_parameter_supported
                && callback.issuer.is_none())
            || callback.error.is_some()
        {
            return Err(OAuthError::Refused);
        }
        let grant = pending
            .transaction
            .consume(&callback)
            .map_err(|_| OAuthError::Refused)?;
        if !graphic(grant.code.expose(), 2048) {
            return Err(OAuthError::Refused);
        }
        let issued = now();
        let reply = self
            .wire
            .token(
                metadata,
                &setup.client,
                &[
                    ("grant_type".to_owned(), "authorization_code".to_owned()),
                    ("code".to_owned(), grant.code.expose().to_owned()),
                    ("redirect_uri".to_owned(), grant.redirect.as_uri()),
                    (
                        "code_verifier".to_owned(),
                        pending.verifier.expose().to_owned(),
                    ),
                    ("resource".to_owned(), resource.to_owned()),
                ],
            )
            .await?;
        let stored = token_grant(reply, &setup, issued, None)?;
        let mut memory = self.memory();
        self.check(&memory, pending.epoch, &setup)?;
        self.write(GRANT_KEY, &stored)?;
        memory.blocked = false;
        Ok(())
    }

    fn parse_callback(&self, target: &str) -> Result<Callback, OAuthError> {
        if target.len() > 8192
            || target.contains('#')
            || target.split('?').next() != Some(callback_path(&self.server).as_str())
        {
            return Err(OAuthError::Refused);
        }
        Callback::from_request_target(target).map_err(|_| OAuthError::Refused)
    }

    pub(crate) async fn access_token(&self, resource: &str) -> Result<String, OAuthError> {
        let _operation = self.operation.lock().await;
        let (setup, old, epoch) = {
            let mut memory = self.memory();
            let setup = self.setup(resource).ok_or(OAuthError::NotConnected)?;
            let grant = self
                .grant(&setup, &memory)
                .ok_or(OAuthError::NotConnected)?;
            if grant
                .expires_at
                .is_none_or(|time| time > now().saturating_add(60))
            {
                return Ok(grant.access_token);
            }
            if !refresh_usable(&grant) {
                return Err(OAuthError::Expired);
            }
            memory.blocked = true;
            self.remove_grant()?;
            (setup, grant, memory.epoch)
        };
        let metadata = setup.metadata.as_ref().ok_or(OAuthError::Expired)?;
        let current = self.wire.discover(resource, &setup.issuer).await?;
        if current != *metadata {
            return Err(OAuthError::Expired);
        }
        let issued = now();
        let reply = self
            .wire
            .token(
                metadata,
                &setup.client,
                &[
                    ("grant_type".to_owned(), "refresh_token".to_owned()),
                    (
                        "refresh_token".to_owned(),
                        old.refresh_token.clone().ok_or(OAuthError::Expired)?,
                    ),
                    ("resource".to_owned(), resource.to_owned()),
                ],
            )
            .await?;
        let refreshed = token_grant(reply, &setup, issued, Some(&old))?;
        let mut memory = self.memory();
        self.check(&memory, epoch, &setup)?;
        self.write(GRANT_KEY, &refreshed)?;
        memory.blocked = false;
        Ok(refreshed.access_token)
    }

    pub(crate) async fn disconnect(&self, resource: &str) -> Result<bool, OAuthError> {
        let (setup, grant) = {
            let mut memory = self.memory();
            let mut setup = self.setup(resource).ok_or(OAuthError::InvalidSetup)?;
            let grant = self.grant(&setup, &memory);
            invalidate(&mut memory);
            let changed = random().and_then(|binding| {
                setup.binding = binding;
                self.write(SETUP_KEY, &setup)
            });
            let removed = self.remove_grant();
            changed.and(removed)?;
            (setup, grant)
        };
        let Some(grant) = grant else {
            return Ok(true);
        };
        let Some(metadata) = setup.metadata.as_ref() else {
            return Ok(false);
        };
        let token = grant
            .refresh_token
            .as_deref()
            .unwrap_or(&grant.access_token);
        Ok(self
            .wire
            .revoke(metadata, &setup.client, token)
            .await
            .is_ok())
    }

    fn setup(&self, resource: &str) -> Option<Setup> {
        let setup: Setup = self.read(SETUP_KEY)?;
        (setup.version == 1
            && setup.resource == resource
            && self.wire.validate_url(resource).is_ok()
            && self.wire.validate_url(&setup.issuer).is_ok()
            && graphic(&setup.client.client_id, 256)
            && setup
                .client
                .client_secret
                .as_deref()
                .is_none_or(|value| graphic(value, MAX_TOKEN))
            && valid_scopes(&setup.scopes)
            && graphic(&setup.binding, 128)
            && setup
                .metadata
                .as_ref()
                .is_none_or(|metadata| metadata.issuer == setup.issuer))
        .then_some(setup)
    }

    fn grant(&self, setup: &Setup, memory: &Memory) -> Option<Grant> {
        if memory.blocked {
            return None;
        }
        let grant: Grant = self.read(GRANT_KEY)?;
        (grant.version == 1
            && grant.binding == setup.binding
            && setup.metadata.is_some()
            && bearer(&grant.access_token)
            && grant
                .refresh_token
                .as_deref()
                .is_none_or(|token| graphic(token, MAX_TOKEN))
            && valid_scopes(&grant.scopes)
            && setup
                .scopes
                .iter()
                .all(|scope| grant.scopes.contains(scope)))
        .then_some(grant)
    }

    fn check(&self, memory: &Memory, epoch: u64, setup: &Setup) -> Result<(), OAuthError> {
        if memory.epoch != epoch
            || self
                .setup(&setup.resource)
                .is_none_or(|current| current.binding != setup.binding)
        {
            return Err(OAuthError::Refused);
        }
        Ok(())
    }

    fn reference(&self, key: &str) -> Result<McpSecretRef, OAuthError> {
        McpSecretRef::parse(&format!("mcp:{}:{key}", self.server)).map_err(|_| OAuthError::Storage)
    }

    fn read<T: DeserializeOwned>(&self, key: &str) -> Option<T> {
        let raw = mcp_secrets::resolve(&self.paths, &self.reference(key).ok()?).ok()?;
        serde_json::from_str(&raw).ok()
    }

    fn write<T: Serialize>(&self, key: &str, value: &T) -> Result<(), OAuthError> {
        let raw = serde_json::to_string(value).map_err(|_| OAuthError::Storage)?;
        mcp_secrets::set(&self.paths, &self.reference(key)?, &raw).map_err(|_| OAuthError::Storage)
    }

    fn remove_grant(&self) -> Result<(), OAuthError> {
        mcp_secrets::remove(&self.paths, &self.reference(GRANT_KEY)?)
            .map_err(|_| OAuthError::Storage)
    }
}

fn invalidate(memory: &mut Memory) {
    memory.epoch = memory.epoch.wrapping_add(1);
    memory.pending = None;
    memory.blocked = true;
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(u64::MAX, |elapsed| elapsed.as_secs())
}

fn random() -> Result<String, OAuthError> {
    PkceVerifier::generate()
        .map(|value| value.expose().to_owned())
        .map_err(|_| OAuthError::Refused)
}

fn graphic(value: &str, max: usize) -> bool {
    !value.is_empty() && value.len() <= max && value.bytes().all(|byte| byte.is_ascii_graphic())
}

fn bearer(value: &str) -> bool {
    let raw = value.trim_end_matches('=');
    !raw.is_empty()
        && value.len() <= MAX_TOKEN
        && raw
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"-._~+/".contains(&byte))
}

fn valid_scopes(scopes: &[String]) -> bool {
    scopes.len() <= 16
        && scopes
            .iter()
            .all(|scope| graphic(scope, 128) && !scope.contains(['"', '\\']))
        && scopes.iter().map(String::len).sum::<usize>() <= 512
}

fn refresh_usable(grant: &Grant) -> bool {
    grant.refresh_token.is_some() && grant.refresh_expires_at.is_none_or(|time| time > now())
}

fn token_grant(
    reply: TokenReply,
    setup: &Setup,
    issued: u64,
    old: Option<&Grant>,
) -> Result<Grant, OAuthError> {
    if old.is_some_and(|old| old.expires_at.is_some()) && reply.expires_in.is_none() {
        return Err(OAuthError::Token);
    }
    if !reply.token_type.eq_ignore_ascii_case("bearer")
        || !bearer(&reply.access_token)
        || reply
            .refresh_token
            .as_deref()
            .is_some_and(|token| !graphic(token, MAX_TOKEN))
        || reply.expires_in == Some(0)
        || reply.refresh_token_expires_in == Some(0)
    {
        return Err(OAuthError::Token);
    }
    let scopes = reply.scope.map_or_else(
        || old.map_or_else(|| setup.scopes.clone(), |old| old.scopes.clone()),
        |scopes| scopes.split_whitespace().map(str::to_owned).collect(),
    );
    if !valid_scopes(&scopes)
        || !setup.scopes.iter().all(|scope| scopes.contains(scope))
        || old.is_some_and(|old| scopes.iter().any(|scope| !old.scopes.contains(scope)))
    {
        return Err(OAuthError::Scopes);
    }
    let expires_at = reply
        .expires_in
        .map(|seconds| issued.checked_add(seconds).ok_or(OAuthError::Token))
        .transpose()?;
    let refresh_expires_at = reply
        .refresh_token_expires_in
        .map(|seconds| issued.checked_add(seconds).ok_or(OAuthError::Token))
        .transpose()?;
    Ok(Grant {
        version: 1,
        binding: setup.binding.clone(),
        access_token: reply.access_token,
        refresh_token: reply
            .refresh_token
            .or_else(|| old.and_then(|old| old.refresh_token.clone())),
        expires_at,
        refresh_expires_at: refresh_expires_at
            .or_else(|| old.and_then(|old| old.refresh_expires_at)),
        scopes,
    })
}

#[cfg(test)]
#[path = "mcp_oauth_account_tests.rs"]
mod tests;
