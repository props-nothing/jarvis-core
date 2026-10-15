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

pub(crate) fn validate_binding(server: &str, endpoint: &str, binding: &str) -> Result<(), String> {
    if binding != "github" || server != "github" || endpoint != GITHUB_ENDPOINT {
        return Err(
            "HTTP authorization supports only binding github, server github and the exact GitHub read-only endpoint"
                .to_owned(),
        );
    }
    Ok(())
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
        validate_binding(server, endpoint.as_str(), binding).map_err(ConnectError::Unreachable)?;
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
                "HTTP authorization is unavailable; connect the GitHub account in Settings".into(),
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
        if token.is_empty()
            || token.len() > 4096
            || !token.bytes().all(|byte| byte.is_ascii_graphic())
        {
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
