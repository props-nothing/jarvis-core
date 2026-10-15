//! Bounded, issuer-bound MCP OAuth discovery and credential-bearing HTTP.
//! Contracts and deliberate restrictions: `docs/research/integrations/mcp-oauth.md`.

use std::net::{IpAddr, SocketAddr};
use std::time::Duration;

use reqwest::{Method, StatusCode, Url, header::HeaderMap};
use serde::{Deserialize, Deserializer, Serialize, de::DeserializeOwned};
use thiserror::Error;

#[path = "mcp_oauth_wire_discovery.rs"]
mod discovery;

const HTTP_TIMEOUT: Duration = Duration::from_secs(20);
const DNS_TIMEOUT: Duration = Duration::from_secs(5);
const MAX_BODY: usize = 64 * 1024;
const MAX_URL: usize = 4096;
const MAX_ADDRESSES: usize = 64;

/// Fixed errors: never retain a provider value, response body, URL or credential.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub(crate) enum OAuthError {
    #[error("OAuth setup is invalid; check the resource, issuer and registered client in Settings")]
    InvalidSetup,
    #[error("OAuth discovery failed; check the server's published authorization metadata")]
    Discovery,
    #[error("OAuth issuer or resource did not match; check the expected issuer and server setup")]
    Issuer,
    #[error("the authorization server must advertise S256 PKCE before sign-in can start")]
    Pkce,
    #[error("OAuth sign-in or destination was refused; start a new sign-in from Settings")]
    Refused,
    #[error(
        "the authorization server refused the exchange or returned an invalid token reply; sign in again"
    )]
    Token,
    #[error("the requested permissions were not granted; review consent and sign in again")]
    Scopes,
    #[error("OAuth credentials could not be stored securely; check the local profile permissions")]
    Storage,
    #[error(
        "the OAuth server could not be reached within safe limits; check connectivity and try again"
    )]
    Unreachable,
    #[error("this server is not connected; sign in from Settings")]
    NotConnected,
    #[error("this server's sign-in is no longer usable; sign in again from Settings")]
    Expired,
    #[error(
        "local access was removed but remote revocation failed; review authorized apps at the provider"
    )]
    Revocation,
}

/// Non-secret discovery facts frozen into an issuer-bound account setup.
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct Metadata {
    pub issuer: String,
    pub authorization_endpoint: String,
    pub token_endpoint: String,
    pub revocation_endpoint: Option<String>,
    pub authorization_response_iss_parameter_supported: bool,
    pub token_endpoint_auth_methods_supported: Vec<String>,
}

/// A pre-registered client. Intentionally has no debug representation.
#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct Client {
    pub client_id: String,
    pub client_secret: Option<String>,
}

/// Wire values only; the account layer validates bearer syntax, lifetimes and scopes.
#[derive(Deserialize)]
#[serde(try_from = "RawTokenReply")]
pub(crate) struct TokenReply {
    pub access_token: String,
    pub token_type: String,
    pub expires_in: Option<u64>,
    pub refresh_token: Option<String>,
    pub refresh_token_expires_in: Option<u64>,
    pub scope: Option<String>,
}

#[derive(Deserialize)]
struct RawTokenReply {
    access_token: String,
    token_type: String,
    expires_in: Option<u64>,
    refresh_token: Option<String>,
    refresh_token_expires_in: Option<u64>,
    scope: Option<String>,
    #[serde(default, deserialize_with = "error_present")]
    error: bool,
}

fn error_present<'de, D: Deserializer<'de>>(deserializer: D) -> Result<bool, D::Error> {
    let _ = serde::de::IgnoredAny::deserialize(deserializer)?;
    Ok(true)
}

impl TryFrom<RawTokenReply> for TokenReply {
    type Error = OAuthError;

    fn try_from(raw: RawTokenReply) -> Result<Self, Self::Error> {
        if raw.error {
            return Err(OAuthError::Token);
        }
        Ok(Self {
            access_token: raw.access_token,
            token_type: raw.token_type,
            expires_in: raw.expires_in,
            refresh_token: raw.refresh_token,
            refresh_token_expires_in: raw.refresh_token_expires_in,
            scope: raw.scope,
        })
    }
}

#[derive(Clone)]
pub(crate) struct Wire {
    #[cfg(test)]
    allow_loopback: bool,
}

struct Reply {
    status: StatusCode,
    headers: HeaderMap,
    body: Vec<u8>,
}

impl Wire {
    pub(crate) fn new() -> Result<Self, OAuthError> {
        let _ = http_builder()
            .build()
            .map_err(|_| OAuthError::Unreachable)?;
        Ok(Self {
            #[cfg(test)]
            allow_loopback: false,
        })
    }

    #[cfg(test)]
    pub(crate) fn loopback() -> Result<Self, OAuthError> {
        let mut wire = Self::new()?;
        wire.allow_loopback = true;
        Ok(wire)
    }

    /// Syntax-only validation: no resolver or network access.
    pub(crate) fn validate_url(&self, text: &str) -> Result<(), OAuthError> {
        self.parse_url(text).map(|_| ())
    }

    fn parse_url(&self, text: &str) -> Result<Url, OAuthError> {
        if text.len() > MAX_URL
            || text
                .bytes()
                .any(|byte| byte.is_ascii_whitespace() || byte.is_ascii_control())
            || text.contains('\\')
        {
            return Err(OAuthError::InvalidSetup);
        }
        let url = Url::parse(text).map_err(|_| OAuthError::InvalidSetup)?;
        let authority = text
            .split_once("://")
            .map(|(_, tail)| tail.split('/').next().unwrap_or(""));
        let host = url.host_str().ok_or(OAuthError::InvalidSetup)?;
        if authority.is_none_or(|value| value.is_empty() || value.contains('@'))
            || !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
            || host.trim_end_matches('.').eq_ignore_ascii_case("localhost")
            || host
                .trim_end_matches('.')
                .to_ascii_lowercase()
                .ends_with(".localhost")
            || url.port_or_known_default().is_none_or(|port| port == 0)
        {
            return Err(OAuthError::InvalidSetup);
        }
        if url.scheme() != "https" && !(url.scheme() == "http" && self.loopback_literal(host)) {
            return Err(OAuthError::InvalidSetup);
        }
        if literal_ip(host).is_some_and(|ip| !jarvis_web::is_public(ip))
            && !self.loopback_literal(host)
        {
            return Err(OAuthError::Refused);
        }
        Ok(url)
    }

    #[cfg(test)]
    fn loopback_literal(&self, host: &str) -> bool {
        self.allow_loopback && literal_ip(host).is_some_and(|ip| ip.is_loopback())
    }

    #[cfg(not(test))]
    #[allow(clippy::unused_self)]
    fn loopback_literal(&self, _host: &str) -> bool {
        false
    }

    pub(crate) async fn discover(
        &self,
        resource: &str,
        expected_issuer: &str,
    ) -> Result<Metadata, OAuthError> {
        discovery::discover(self, resource, expected_issuer).await
    }

    pub(crate) async fn token(
        &self,
        metadata: &Metadata,
        client: &Client,
        fields: &[(String, String)],
    ) -> Result<TokenReply, OAuthError> {
        validate_fields(fields)?;
        let (fields, basic) = authenticate(metadata, client, fields.to_vec())?;
        let reply = self
            .exchange(&metadata.token_endpoint, Method::POST, &fields, basic)
            .await?;
        if reply.status != StatusCode::OK {
            return Err(OAuthError::Token);
        }
        decode(&reply, OAuthError::Token)
    }

    pub(crate) async fn revoke(
        &self,
        metadata: &Metadata,
        client: &Client,
        token: &str,
    ) -> Result<(), OAuthError> {
        let endpoint = metadata
            .revocation_endpoint
            .as_deref()
            .ok_or(OAuthError::Revocation)?;
        if token.is_empty() || token.len() > MAX_BODY {
            return Err(OAuthError::Revocation);
        }
        let fields = vec![("token".to_owned(), token.to_owned())];
        let (fields, basic) =
            authenticate(metadata, client, fields).map_err(|_| OAuthError::Revocation)?;
        let reply = self
            .exchange(endpoint, Method::POST, &fields, basic)
            .await
            .map_err(|_| OAuthError::Revocation)?;
        if reply.status != StatusCode::OK {
            return Err(OAuthError::Revocation);
        }
        Ok(())
    }

    async fn exchange(
        &self,
        endpoint: &str,
        method: Method,
        fields: &[(String, String)],
        basic: Option<(String, String)>,
    ) -> Result<Reply, OAuthError> {
        tokio::time::timeout(HTTP_TIMEOUT, async {
            let url = self.parse_url(endpoint)?;
            let addresses = self.resolve(&url).await?;
            let host = url.host_str().ok_or(OAuthError::InvalidSetup)?;
            let client = http_builder()
                .resolve_to_addrs(host, &addresses)
                .build()
                .map_err(|_| OAuthError::Unreachable)?;
            let mut request = client
                .request(method.clone(), url)
                .header("Accept", "application/json");
            if method == Method::POST {
                request = request
                    .header("Content-Type", "application/x-www-form-urlencoded")
                    .body(form_body(fields));
            }
            if let Some((id, secret)) = basic {
                request = request.basic_auth(id, Some(secret));
            }
            let response = request.send().await.map_err(|_| OAuthError::Unreachable)?;
            read_reply(response).await
        })
        .await
        .map_err(|_| OAuthError::Unreachable)?
    }

    async fn resolve(&self, url: &Url) -> Result<Vec<SocketAddr>, OAuthError> {
        let host = url.host_str().ok_or(OAuthError::InvalidSetup)?;
        let port = url
            .port_or_known_default()
            .ok_or(OAuthError::InvalidSetup)?;
        let addresses = if let Some(ip) = literal_ip(host) {
            vec![SocketAddr::new(ip, port)]
        } else {
            tokio::time::timeout(DNS_TIMEOUT, tokio::net::lookup_host((host, port)))
                .await
                .map_err(|_| OAuthError::Unreachable)?
                .map_err(|_| OAuthError::Unreachable)?
                .take(MAX_ADDRESSES + 1)
                .collect()
        };
        self.check_addresses(host, &addresses)?;
        Ok(addresses)
    }

    fn check_addresses(&self, host: &str, addresses: &[SocketAddr]) -> Result<(), OAuthError> {
        if addresses.is_empty() || addresses.len() > MAX_ADDRESSES {
            return Err(OAuthError::Unreachable);
        }
        if addresses.iter().any(|address| {
            !(jarvis_web::is_public(address.ip())
                || self.loopback_literal(host) && address.ip().is_loopback())
        }) {
            return Err(OAuthError::Refused);
        }
        Ok(())
    }
}

fn literal_ip(host: &str) -> Option<IpAddr> {
    host.trim_start_matches('[')
        .trim_end_matches(']')
        .parse()
        .ok()
}

fn http_builder() -> reqwest::ClientBuilder {
    reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .retry(reqwest::retry::never())
        .referer(false)
        .timeout(HTTP_TIMEOUT)
}

async fn read_reply(mut response: reqwest::Response) -> Result<Reply, OAuthError> {
    if response
        .content_length()
        .is_some_and(|length| length > MAX_BODY as u64)
    {
        return Err(OAuthError::Unreachable);
    }
    let status = response.status();
    let headers = response.headers().clone();
    let mut body = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| OAuthError::Unreachable)?
    {
        if chunk.len() > MAX_BODY - body.len() {
            return Err(OAuthError::Unreachable);
        }
        body.extend_from_slice(&chunk);
    }
    Ok(Reply {
        status,
        headers,
        body,
    })
}

fn decode<T: DeserializeOwned>(reply: &Reply, error: OAuthError) -> Result<T, OAuthError> {
    serde_json::from_slice(&reply.body).map_err(|_| error)
}

fn validate_fields(fields: &[(String, String)]) -> Result<(), OAuthError> {
    let mut seen = std::collections::HashSet::new();
    let mut size = 0_usize;
    for (key, value) in fields {
        size = size.saturating_add(key.len()).saturating_add(value.len());
        if matches!(key.as_str(), "client_id" | "client_secret")
            || !seen.insert(key)
            || size > MAX_BODY
        {
            return Err(OAuthError::InvalidSetup);
        }
    }
    Ok(())
}

type AuthenticatedForm = (Vec<(String, String)>, Option<(String, String)>);

fn authenticate(
    metadata: &Metadata,
    client: &Client,
    mut fields: Vec<(String, String)>,
) -> Result<AuthenticatedForm, OAuthError> {
    if client.client_id.is_empty() || client.client_id.len() > MAX_URL {
        return Err(OAuthError::InvalidSetup);
    }
    let supports = |method| {
        metadata
            .token_endpoint_auth_methods_supported
            .iter()
            .any(|item| item == method)
    };
    let Some(secret) = &client.client_secret else {
        if !supports("none") {
            return Err(OAuthError::InvalidSetup);
        }
        fields.push(("client_id".to_owned(), client.client_id.clone()));
        return Ok((fields, None));
    };
    if secret.is_empty() || secret.len() > MAX_URL {
        return Err(OAuthError::InvalidSetup);
    }
    if supports("client_secret_basic") {
        return Ok((
            fields,
            Some((form_component(&client.client_id), form_component(secret))),
        ));
    }
    if supports("client_secret_post") {
        fields.push(("client_id".to_owned(), client.client_id.clone()));
        fields.push(("client_secret".to_owned(), secret.clone()));
        return Ok((fields, None));
    }
    Err(OAuthError::InvalidSetup)
}

fn form_body(fields: &[(String, String)]) -> String {
    fields
        .iter()
        .map(|(key, value)| format!("{}={}", form_component(key), form_component(value)))
        .collect::<Vec<_>>()
        .join("&")
}

// RFC 6749 section 2.3.1 encodes each credential separately before HTTP Basic.
fn form_component(value: &str) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut encoded = String::new();
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'*' | b'-' | b'.' | b'_' => {
                encoded.push(char::from(byte));
            }
            b' ' => encoded.push('+'),
            _ => {
                encoded.push('%');
                encoded.push(char::from(HEX[usize::from(byte >> 4)]));
                encoded.push(char::from(HEX[usize::from(byte & 15)]));
            }
        }
    }
    encoded
}

#[cfg(test)]
#[path = "mcp_oauth_wire_tests.rs"]
mod tests;
