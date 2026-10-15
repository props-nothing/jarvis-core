//! Profile-scoped resolution for explicitly configured MCP child and HTTP credentials.

use jarvis_core::McpSecretRef;
use jarvis_mcp_transport::{McpSecretResolver, SecretUnavailable};
use jarvis_storage::{AppPaths, mcp_secrets};

pub(crate) struct ProfileSecrets(pub(crate) AppPaths);

pub(crate) struct ProfileHttpSecrets {
    paths: Option<AppPaths>,
    account: Option<std::sync::Arc<crate::github_account::GithubAccount>>,
    oauth: Option<std::sync::Arc<crate::mcp_oauth_account::Accounts>>,
}

impl ProfileHttpSecrets {
    #[cfg(test)]
    pub(crate) fn with_account(
        account: std::sync::Arc<crate::github_account::GithubAccount>,
    ) -> Self {
        Self {
            paths: None,
            account: Some(account),
            oauth: None,
        }
    }

    pub(crate) fn new(paths: &AppPaths) -> Self {
        Self {
            paths: Some(paths.clone()),
            account: crate::github_account::GithubAccount::shared(paths).ok(),
            oauth: Some(crate::mcp_oauth_account::Accounts::shared(paths)),
        }
    }
}

#[async_trait::async_trait]
impl jarvis_mcp_transport::HttpCredentialProvider for ProfileHttpSecrets {
    async fn token(
        &self,
        binding: &str,
        server: &str,
        endpoint: &str,
    ) -> Result<String, SecretUnavailable> {
        jarvis_mcp_transport::validate_http_binding(server, endpoint, binding)
            .map_err(|_| SecretUnavailable)?;
        if binding.starts_with("oauth:") {
            return self
                .oauth
                .as_ref()
                .ok_or(SecretUnavailable)?
                .account(server)
                .map_err(|_| SecretUnavailable)?
                .access_token(endpoint)
                .await
                .map_err(|_| SecretUnavailable);
        }
        if binding != "github" {
            let reference = McpSecretRef::parse(binding).map_err(|_| SecretUnavailable)?;
            return mcp_secrets::resolve(self.paths.as_ref().ok_or(SecretUnavailable)?, &reference)
                .map_err(|_| SecretUnavailable);
        }
        self.account
            .as_ref()
            .ok_or(SecretUnavailable)?
            .access_token()
            .await
            .map_err(|_| SecretUnavailable)
    }
}

impl McpSecretResolver for ProfileSecrets {
    fn resolve(&self, reference: &McpSecretRef) -> Result<String, SecretUnavailable> {
        mcp_secrets::resolve(&self.0, reference).map_err(|_| SecretUnavailable)
    }
}

#[cfg(test)]
#[path = "mcp_credentials_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "mcp_http_credentials_tests.rs"]
mod http_tests;

pub(crate) fn http_state(
    paths: &AppPaths,
    server: &str,
    endpoint: &str,
    binding: &str,
) -> Option<serde_json::Value> {
    jarvis_mcp_transport::validate_http_binding(server, endpoint, binding).ok()?;
    let reference = McpSecretRef::parse(binding).ok()?;
    Some(serde_json::json!({
        "name": "Authorization",
        "reference": reference.as_str(),
        "state": mcp_secrets::state(paths, &reference),
    }))
}

pub(crate) fn states(
    paths: &AppPaths,
    environment: &std::collections::BTreeMap<String, String>,
) -> Vec<serde_json::Value> {
    environment
        .iter()
        .map(|(name, reference)| {
            let state = McpSecretRef::parse(reference)
                .map_or("not set", |reference| mcp_secrets::state(paths, &reference));
            serde_json::json!({ "name": name, "reference": reference, "state": state })
        })
        .collect()
}
