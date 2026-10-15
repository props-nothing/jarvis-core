//! Explicit stdio credential references, resolved only at a child-process boundary.

use std::collections::BTreeMap;
use std::sync::Arc;

use jarvis_core::McpSecretRef;
use serde_json::Value;

use crate::ConnectError;

/// Resolves a profile-owned reference. Failure deliberately carries no secret-store diagnostic.
pub trait McpSecretResolver: Send + Sync {
    /// Returns a credential for this reference, or refuses before a child is started.
    ///
    /// # Errors
    ///
    /// Returns an opaque refusal if the credential is missing, invalid or unreadable.
    fn resolve(&self, reference: &McpSecretRef) -> Result<String, SecretUnavailable>;
}

/// A reference could not be resolved; never contains a value, path or provider error.
#[derive(Clone, Copy, Debug)]
pub struct SecretUnavailable;

pub(crate) struct UnavailableSecrets;
impl McpSecretResolver for UnavailableSecrets {
    fn resolve(&self, _: &McpSecretRef) -> Result<String, SecretUnavailable> {
        Err(SecretUnavailable)
    }
}

/// Validates one explicitly mapped credential variable, excluding runtime/loader controls.
#[must_use]
pub fn is_credential_variable(name: &str) -> bool {
    if name.len() > 128
        || !name
            .bytes()
            .next()
            .is_some_and(|first| first.is_ascii_alphabetic())
        || !name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
    {
        return false;
    }
    let upper = name.to_ascii_uppercase();
    if [
        "JARVIS_",
        "MCP_",
        "LD_",
        "DYLD_",
        "NODE_",
        "PYTHON",
        "RUBY",
        "PERL",
        "BASH_",
        "ENV",
        "GIT_CONFIG",
        "JAVA_",
        "JDK_",
        "DOTNET_",
        "COMPLUS_",
        "NPM_",
        "CARGO_",
        "RUST",
        "OPENSSL_",
        "SSL_",
        "HTTP_",
        "HTTPS_",
        "ALL_PROXY",
        "NO_PROXY",
    ]
    .iter()
    .any(|prefix| upper.starts_with(prefix))
    {
        return false;
    }
    ["TOKEN", "KEY", "SECRET", "PASSWORD", "CREDENTIAL"]
        .iter()
        .any(|suffix| upper == *suffix || upper.ends_with(&format!("_{suffix}")))
}

pub(crate) fn validate_references(
    server: &str,
    environment: &BTreeMap<String, String>,
) -> Result<BTreeMap<String, McpSecretRef>, String> {
    if environment.len() > 16 {
        return Err("at most 16 credential environment mappings are allowed per server".to_owned());
    }
    let mut names = std::collections::BTreeSet::new();
    environment.iter().map(|(name, value)| {
        if !is_credential_variable(name) || !names.insert(name.to_ascii_uppercase()) {
            return Err("credential environment names must be unique portable credential names, not runtime controls".to_owned());
        }
        let reference = McpSecretRef::parse(value)
            .map_err(|_| "environment values must be named references: mcp:SERVER:KEY".to_owned())?;
        if reference.server() != server {
            return Err("an MCP server may only use its own credential references".to_owned());
        }
        Ok((name.clone(), reference))
    }).collect()
}

#[derive(Clone)]
pub(crate) struct CredentialEnvironment {
    pub(crate) references: BTreeMap<String, McpSecretRef>,
    pub(crate) resolver: Arc<dyn McpSecretResolver>,
}

impl std::fmt::Debug for CredentialEnvironment {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CredentialEnvironment")
            .field("names", &self.references.keys())
            .finish_non_exhaustive()
    }
}

impl CredentialEnvironment {
    pub(crate) fn resolve(&self) -> Result<BTreeMap<String, String>, ConnectError> {
        self.references
            .iter()
            .map(|(name, reference)| {
                let value = self.resolver.resolve(reference).map_err(|_| {
                    ConnectError::Unreachable(format!(
                        "credential {name} is not set or unavailable; use jarvis keys set {}",
                        reference.as_str()
                    ))
                })?;
                if value.is_empty()
                    || value.len() > 4096
                    || !value.bytes().all(|byte| byte.is_ascii_graphic())
                {
                    return Err(ConnectError::Unreachable(
                        "a configured MCP credential is invalid".to_owned(),
                    ));
                }
                Ok((name.clone(), value))
            })
            .collect()
    }
}

#[derive(Default)]
pub(crate) struct CredentialRedactor(Vec<String>);

impl CredentialRedactor {
    pub(crate) fn extend(&mut self, values: impl IntoIterator<Item = String>) {
        for value in values {
            if !self.0.contains(&value) {
                self.0.push(value);
            }
        }
        self.0.sort_by_key(|value| std::cmp::Reverse(value.len()));
    }

    pub(crate) fn values(&self) -> Vec<String> {
        self.0.clone()
    }

    pub(crate) fn text(&self, text: &str) -> String {
        self.0.iter().fold(text.to_owned(), |text, secret| {
            text.replace(secret, "[REDACTED]")
        })
    }

    pub(crate) fn value(&self, value: &mut Value) {
        match value {
            Value::String(text) => *text = self.text(text),
            Value::Array(values) => {
                for value in values {
                    self.value(value);
                }
            }
            Value::Object(values) => {
                let original = std::mem::take(values);
                for (key, mut value) in original {
                    self.value(&mut value);
                    values.insert(self.text(&key), value);
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
#[path = "credentials_tests.rs"]
mod tests;

impl Drop for CredentialRedactor {
    fn drop(&mut self) {
        for secret in &mut self.0 {
            let bytes = std::mem::take(secret).into_bytes();
            let mut bytes = bytes;
            bytes.fill(0);
        }
    }
}
