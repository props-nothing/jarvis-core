use std::fmt;

use thiserror::Error;

/// Indicates that secret-reference metadata is empty, oversized, or structurally unsafe.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum SecretRefValidationError {
    /// A required component was empty.
    #[error("secret reference components cannot be empty")]
    Empty,
    /// A component exceeded its documented bound.
    #[error("secret reference component is too long")]
    TooLong,
    /// A component contained whitespace, controls, or unsupported punctuation.
    #[error("secret reference component contains unsupported characters")]
    InvalidCharacter,
}

/// Metadata that locates a secret without containing the secret value.
///
/// Generic serialization is intentionally unavailable because it could expose
/// the locator through diagnostics or an unrelated wire contract. Storage
/// adapters must map the fields explicitly.
///
/// ```compile_fail
/// use jarvis_core::SecretRef;
/// let secret_ref = SecretRef::new("os-keychain", "item-id", "local-ipc")?;
/// let _json = serde_json::to_string(&secret_ref)?;
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[derive(Clone, Eq, Hash, PartialEq)]
pub struct SecretRef {
    provider: String,
    locator: String,
    purpose: String,
}

impl SecretRef {
    /// Creates a validated secret reference.
    ///
    /// # Errors
    ///
    /// Returns [`SecretRefValidationError`] when a component is empty, exceeds
    /// its bound, or contains whitespace, control characters, or unsupported punctuation.
    pub fn new(
        provider: impl Into<String>,
        locator: impl Into<String>,
        purpose: impl Into<String>,
    ) -> Result<Self, SecretRefValidationError> {
        let provider = provider.into();
        let locator = locator.into();
        let purpose = purpose.into();

        validate_component(&provider, 32)?;
        validate_component(&locator, 256)?;
        validate_component(&purpose, 64)?;

        Ok(Self {
            provider,
            locator,
            purpose,
        })
    }

    /// Returns the secret-provider identifier.
    #[must_use]
    pub fn provider(&self) -> &str {
        &self.provider
    }

    /// Returns the non-secret purpose label.
    #[must_use]
    pub fn purpose(&self) -> &str {
        &self.purpose
    }

    /// Explicitly exposes the locator to the final secret-store adapter.
    ///
    /// Callers must not place this value into normal logs, prompts, URLs, or errors.
    #[must_use]
    pub fn expose_locator(&self) -> &str {
        &self.locator
    }
}

impl fmt::Debug for SecretRef {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SecretRef")
            .field("provider", &self.provider)
            .field("locator", &"[REDACTED]")
            .field("purpose", &self.purpose)
            .finish()
    }
}

impl fmt::Display for SecretRef {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}://[REDACTED]", self.provider)
    }
}

/// Profile-owned MCP credential metadata, never a credential value or arbitrary path.
#[derive(Clone, Eq, Hash, PartialEq)]
pub struct McpSecretRef {
    reference: String,
    separator: usize,
}

impl McpSecretRef {
    /// Parses `mcp:<server>:<key>` with bounded, lowercase portable components.
    ///
    /// # Errors
    ///
    /// Returns a value-free error for malformed references. Each component must
    /// start with an ASCII lowercase letter and contain at most 64 bytes of
    /// lowercase letters, digits, underscores or hyphens.
    pub fn parse(reference: &str) -> Result<Self, SecretRefValidationError> {
        let body = reference
            .strip_prefix("mcp:")
            .ok_or(SecretRefValidationError::InvalidCharacter)?;
        let (server, key) = body
            .split_once(':')
            .ok_or(SecretRefValidationError::InvalidCharacter)?;
        for component in [server, key] {
            if component.is_empty() {
                return Err(SecretRefValidationError::Empty);
            }
            if component.len() > 64 {
                return Err(SecretRefValidationError::TooLong);
            }
            if !component.as_bytes()[0].is_ascii_lowercase()
                || !component.bytes().all(|byte| {
                    byte.is_ascii_lowercase()
                        || byte.is_ascii_digit()
                        || matches!(byte, b'_' | b'-')
                })
            {
                return Err(SecretRefValidationError::InvalidCharacter);
            }
        }
        Ok(Self {
            reference: reference.to_owned(),
            separator: 4 + server.len(),
        })
    }

    /// Returns the configured MCP server name.
    #[must_use]
    pub fn server(&self) -> &str {
        &self.reference[4..self.separator]
    }

    /// Explicitly exposes the managed key locator to storage and configuration surfaces.
    #[must_use]
    pub fn key(&self) -> &str {
        &self.reference[self.separator + 1..]
    }

    /// Explicitly exposes the canonical reference metadata, not its secret value.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.reference
    }
}

impl fmt::Debug for McpSecretRef {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("McpSecretRef")
            .field("server", &self.server())
            .field("key", &"[REDACTED]")
            .finish()
    }
}

impl fmt::Display for McpSecretRef {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "mcp:{}:[REDACTED]", self.server())
    }
}

fn validate_component(value: &str, max_len: usize) -> Result<(), SecretRefValidationError> {
    if value.is_empty() {
        return Err(SecretRefValidationError::Empty);
    }
    if value.len() > max_len {
        return Err(SecretRefValidationError::TooLong);
    }
    if value
        .chars()
        .any(|character| !(character.is_ascii_alphanumeric() || "._:/-".contains(character)))
    {
        return Err(SecretRefValidationError::InvalidCharacter);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ordinary_formatting_redacts_the_locator() {
        let canary = "canary-secret-reference-123";
        let secret_ref = SecretRef::new("os-keychain", canary, "local-ipc")
            .unwrap_or_else(|error| panic!("valid secret reference fixture: {error}"));

        let debug = format!("{secret_ref:?}");
        let display = secret_ref.to_string();
        assert!(!debug.contains(canary));
        assert!(!display.contains(canary));
        assert_eq!(secret_ref.expose_locator(), canary);
    }

    #[test]
    fn mcp_secret_references_are_validated_metadata_with_redacted_formatting() {
        let reference = McpSecretRef::parse("mcp:github-server:canary_key-123")
            .unwrap_or_else(|error| panic!("valid reference: {error}"));
        assert_eq!(reference.server(), "github-server");
        assert_eq!(reference.key(), "canary_key-123");
        assert_eq!(reference.as_str(), "mcp:github-server:canary_key-123");
        assert!(!format!("{reference:?}").contains(reference.key()));
        assert!(!reference.to_string().contains(reference.key()));
        let component = "a".repeat(64);
        assert!(McpSecretRef::parse(&format!("mcp:{component}:{component}")).is_ok());
    }

    #[test]
    fn mcp_secret_references_refuse_paths_traversal_and_ambiguous_components() {
        for reference in [
            "",
            "mcp",
            "mcp:",
            "mcp::key",
            "mcp:server:",
            "MCP:server:key",
            "mcp:Server:key",
            "mcp:server:Key",
            "mcp:1server:key",
            "mcp:server:_key",
            "mcp:../server:key",
            "mcp:server:../key",
            "mcp:server:key/child",
            "mcp:server:key\\child",
            "mcp:server:key:extra",
            "mcp:server:key.key",
            "mcp:server:key\n",
            "mcp:server:kéy",
            " mcp:server:key",
        ] {
            let error = McpSecretRef::parse(reference)
                .err()
                .unwrap_or_else(|| panic!("malformed reference was accepted"));
            assert!(!format!("{error:?} {error}").contains("mcp:"));
        }
        let component = "a".repeat(65);
        assert!(McpSecretRef::parse(&format!("mcp:{component}:key")).is_err());
        assert!(McpSecretRef::parse(&format!("mcp:server:{component}")).is_err());
    }

    #[test]
    fn unsafe_reference_components_fail_closed() {
        assert_eq!(
            SecretRef::new("os keychain", "item", "purpose"),
            Err(SecretRefValidationError::InvalidCharacter)
        );
    }
}
