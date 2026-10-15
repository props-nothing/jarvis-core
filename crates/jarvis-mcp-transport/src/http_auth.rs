//! Current, server-bound HTTP credentials, resolved immediately before every outbound operation.

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use async_trait::async_trait;
use http::{HeaderName, HeaderValue};
use rmcp::model::ClientJsonRpcMessage;
use rmcp::transport::common::client_side_sse::BoxedSseResponse;
use rmcp::transport::streamable_http_client::{
    StreamableHttpClient, StreamableHttpError, StreamableHttpPostResponse,
};

use crate::credentials::CredentialRedactor;
use crate::endpoint::build_http_client;
use crate::{ConnectError, McpHttpEndpoint, SecretUnavailable};

const GITHUB_ENDPOINT: &str = "https://api.githubcopilot.com/mcp/readonly";

/// Supplies a current token without exposing account storage or OAuth machinery to MCP.
#[async_trait]
pub trait HttpCredentialProvider: Send + Sync {
    /// Resolves or refreshes the current token for this exact server and endpoint.
    ///
    /// # Errors
    ///
    /// Returns an opaque refusal when the account cannot authorize this operation.
    async fn token(
        &self,
        binding: &str,
        server: &str,
        endpoint: &str,
    ) -> Result<String, SecretUnavailable>;
}

/// Default provider: no HTTP credentials are available.
#[derive(Debug, Default)]
pub struct UnavailableHttpCredentials;

#[async_trait]
impl HttpCredentialProvider for UnavailableHttpCredentials {
    async fn token(&self, _: &str, _: &str, _: &str) -> Result<String, SecretUnavailable> {
        Err(SecretUnavailable)
    }
}

/// Validates an HTTP credential reference and its configured destination.
///
/// # Errors
///
/// Refuses cross-server/literal references, query-bearing credential destinations and invalid endpoints.
/// `oauth:SERVER` selects only that server's account; `github` keeps its fixed read-only destination.
pub fn validate_http_binding(server: &str, endpoint: &str, binding: &str) -> Result<(), String> {
    if binding == "github" {
        return if server == "github" && endpoint == GITHUB_ENDPOINT {
            Ok(())
        } else {
            Err("the GitHub account binding requires server github and the exact GitHub read-only endpoint".to_owned())
        };
    }
    if let Some(owner) = binding.strip_prefix("oauth:") {
        let reference = jarvis_core::McpSecretRef::parse(&format!("mcp:{owner}:oauth-client"))
            .map_err(|_| "HTTP OAuth must name oauth:SERVER for this server".to_owned())?;
        if reference.server() != server {
            return Err("an MCP server may only use its own OAuth account".to_owned());
        }
        let endpoint = McpHttpEndpoint::parse(endpoint)
            .map_err(|_| "the OAuth destination is not a valid MCP endpoint".to_owned())?;
        if endpoint.as_str().contains('?') {
            return Err("OAuth requires a query-free MCP endpoint".to_owned());
        }
        return Ok(());
    }
    let reference = jarvis_core::McpSecretRef::parse(binding)
        .map_err(|_| "HTTP authorization must name an account binding or mcp:SERVER:KEY reference, never a token value".to_owned())?;
    if reference.server() != server {
        return Err("an MCP server may only use its own HTTP credential references".to_owned());
    }
    let endpoint = McpHttpEndpoint::parse(endpoint)
        .map_err(|_| "the HTTP credential destination is not a valid MCP endpoint".to_owned())?;
    if endpoint.as_str().contains('?') {
        return Err("an HTTP token credential requires a query-free MCP endpoint; never put a token in a URL".to_owned());
    }
    Ok(())
}

fn is_bearer_token(token: &str) -> bool {
    let unpadded = token.trim_end_matches('=');
    !unpadded.is_empty()
        && token.len() <= 4096
        && unpadded.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~' | b'+' | b'/')
        })
}

#[derive(Clone)]
pub(crate) struct AuthorizedHttpClient {
    client: reqwest::Client,
    endpoint: Arc<str>,
    binding: String,
    server: String,
    provider: Arc<dyn HttpCredentialProvider>,
    pub(crate) redactor: Arc<RwLock<CredentialRedactor>>,
}

impl std::fmt::Debug for AuthorizedHttpClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AuthorizedHttpClient")
            .field("server", &self.server)
            .field("binding", &self.binding)
            .finish_non_exhaustive()
    }
}

impl AuthorizedHttpClient {
    pub(crate) fn new(
        server: &str,
        endpoint: &McpHttpEndpoint,
        binding: &str,
        provider: Arc<dyn HttpCredentialProvider>,
    ) -> Result<Self, ConnectError> {
        validate_http_binding(server, endpoint.as_str(), binding)
            .map_err(ConnectError::Unreachable)?;
        Self::build(server, endpoint, binding, provider)
    }

    fn build(
        server: &str,
        endpoint: &McpHttpEndpoint,
        binding: &str,
        provider: Arc<dyn HttpCredentialProvider>,
    ) -> Result<Self, ConnectError> {
        Ok(Self {
            client: build_http_client(endpoint)?,
            endpoint: Arc::from(endpoint.as_str()),
            binding: binding.to_owned(),
            server: server.to_owned(),
            provider,
            redactor: Arc::new(RwLock::new(CredentialRedactor::default())),
        })
    }

    #[cfg(test)]
    pub(crate) fn loopback(
        endpoint: &McpHttpEndpoint,
        provider: Arc<dyn HttpCredentialProvider>,
    ) -> Self {
        assert!(endpoint.is_loopback());
        Self::build("github", endpoint, "github", provider)
            .unwrap_or_else(|error| panic!("loopback HTTP client: {error}"))
    }

    pub(crate) fn endpoint(&self) -> &str {
        &self.endpoint
    }

    async fn current_token(
        &self,
        uri: &str,
    ) -> Result<String, StreamableHttpError<reqwest::Error>> {
        let unavailable = || {
            StreamableHttpError::UnexpectedServerResponse(
                if self.binding == "github" {
                    "HTTP authorization is unavailable; connect the GitHub account in Settings"
                } else if self.binding.starts_with("oauth:") {
                    "HTTP authorization is unavailable; connect this server's OAuth account in Settings"
                } else {
                    "HTTP authorization is unavailable; set the configured named MCP key with jarvis keys set"
                }
                .into(),
            )
        };
        if uri != self.endpoint.as_ref() {
            return Err(unavailable());
        }
        let token = self
            .provider
            .token(&self.binding, &self.server, &self.endpoint)
            .await
            .map_err(|_| unavailable())?;
        if !is_bearer_token(&token) {
            return Err(unavailable());
        }
        // Record before sending, including during the handshake and background SSE reconnects.
        self.redactor
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .extend([token.clone()]);
        Ok(token)
    }
}

impl StreamableHttpClient for AuthorizedHttpClient {
    type Error = reqwest::Error;

    async fn post_message(
        &self,
        uri: Arc<str>,
        message: ClientJsonRpcMessage,
        session_id: Option<Arc<str>>,
        _: Option<String>,
        custom_headers: HashMap<HeaderName, HeaderValue>,
    ) -> Result<StreamableHttpPostResponse, StreamableHttpError<Self::Error>> {
        let token = self.current_token(&uri).await?;
        self.client
            .post_message(uri, message, session_id, Some(token), custom_headers)
            .await
    }

    async fn post_message_with_max_sse_event_size(
        &self,
        uri: Arc<str>,
        message: ClientJsonRpcMessage,
        session_id: Option<Arc<str>>,
        _: Option<String>,
        custom_headers: HashMap<HeaderName, HeaderValue>,
        max_sse_event_size: usize,
    ) -> Result<StreamableHttpPostResponse, StreamableHttpError<Self::Error>> {
        let token = self.current_token(&uri).await?;
        self.client
            .post_message_with_max_sse_event_size(
                uri,
                message,
                session_id,
                Some(token),
                custom_headers,
                max_sse_event_size,
            )
            .await
    }

    async fn get_stream(
        &self,
        uri: Arc<str>,
        session_id: Option<Arc<str>>,
        last_event_id: Option<String>,
        _: Option<String>,
        custom_headers: HashMap<HeaderName, HeaderValue>,
    ) -> Result<BoxedSseResponse, StreamableHttpError<Self::Error>> {
        let token = self.current_token(&uri).await?;
        self.client
            .get_stream(uri, session_id, last_event_id, Some(token), custom_headers)
            .await
    }

    async fn get_stream_with_max_sse_event_size(
        &self,
        uri: Arc<str>,
        session_id: Option<Arc<str>>,
        last_event_id: Option<String>,
        _: Option<String>,
        custom_headers: HashMap<HeaderName, HeaderValue>,
        max_sse_event_size: usize,
    ) -> Result<BoxedSseResponse, StreamableHttpError<Self::Error>> {
        let token = self.current_token(&uri).await?;
        self.client
            .get_stream_with_max_sse_event_size(
                uri,
                session_id,
                last_event_id,
                Some(token),
                custom_headers,
                max_sse_event_size,
            )
            .await
    }

    async fn delete_session(
        &self,
        uri: Arc<str>,
        session_id: Arc<str>,
        _: Option<String>,
        custom_headers: HashMap<HeaderName, HeaderValue>,
    ) -> Result<(), StreamableHttpError<Self::Error>> {
        let token = self.current_token(&uri).await?;
        self.client
            .delete_session(uri, session_id, Some(token), custom_headers)
            .await
    }
}

#[cfg(test)]
#[path = "http_auth_tests.rs"]
mod tests;
