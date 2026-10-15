//! Profile-scoped resolution for explicitly configured MCP child credentials.

use jarvis_core::McpSecretRef;
use jarvis_mcp_transport::{McpSecretResolver, SecretUnavailable};
use jarvis_storage::{AppPaths, mcp_secrets};

pub(crate) struct ProfileSecrets(pub(crate) AppPaths);

pub(crate) struct ProfileHttpSecrets {
    account: Option<std::sync::Arc<crate::github_account::GithubAccount>>,
}

impl ProfileHttpSecrets {
    #[cfg(test)]
    pub(crate) fn with_account(
        account: std::sync::Arc<crate::github_account::GithubAccount>,
    ) -> Self {
        Self {
            account: Some(account),
        }
    }

    pub(crate) fn new(paths: &AppPaths) -> Self {
        Self {
            account: crate::github_account::GithubAccount::shared(paths).ok(),
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
        if binding != "github"
            || server != "github"
            || endpoint != crate::github_service::MCP_ENDPOINT
        {
            return Err(SecretUnavailable);
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
