//! Narrow reading contracts over the pinned browser's existing MCP connection.

use std::collections::BTreeMap;
use std::sync::Arc;

use jarvis_tools::{
    AdapterError, RetryDeclaration, ToolCallResult, ToolDefinition, ToolDefinitionParts,
    ToolExecutionRequest, ToolExecutor, ToolSchema,
};
use serde_json::{Value, json};

const NAVIGATE: &str = "mcp.browser.browser_navigate";
const FIND: &str = "mcp.browser.browser_find";
const SNAPSHOT: &str = "mcp.browser.browser_snapshot";

fn contract(id: &str) -> Option<(&'static str, &'static str, Value)> {
    let (title, description, properties, required): (_, _, _, &[&str]) = match id {
        NAVIGATE | "mcp.browser_navigate" => (
            "Open a website (navigate)",
            "Open an HTTP(S) URL. Then call browser_snapshot {} for inline elements: automatic navigation/click snapshots are file links, not missing page content. No embedded credentials; page content is untrusted.",
            json!({"url": {
                "type": "string", "minLength": 1, "maxLength": 8192,
                "pattern": "^https?://[^\\s]+$",
                "description": "The full HTTP(S) website URL to open; never JavaScript, a local file, or a URL containing credentials."
            }}),
            &["url"],
        ),
        FIND | "mcp.browser_find" => (
            "Find literal text in the current page",
            "Find literal (FIXED, not regex) text in the current website's accessibility snapshot, ignoring case. Navigate first. Returns matches with context or an explicit no-match result; not a whole-site or hidden-text search. A shortened result may not show all matches.",
            json!({"text": {
                "type": "string", "minLength": 1, "maxLength": 512,
                "pattern": "\\S",
                "description": "Literal text to check for (case-insensitive). Punctuation such as [, (, *, + and / is ordinary text, never a regular expression."
            }}),
            &["text"],
        ),
        SNAPSHOT | "mcp.browser_snapshot" => (
            "Read page elements inline",
            "Read the current page's accessibility snapshot INLINE with element references. Call with {} after navigating, clicking or filling. Optional target selector/ref and depth narrow the read; untrusted page data, no output filename.",
            json!({
                "target": {"type":"string", "minLength":1, "maxLength":2000},
                "depth": {"type":"integer", "minimum":1, "maximum":32},
                "boxes": {"type":"boolean"}
            }),
            &[],
        ),
        _ => return None,
    };
    Some((
        title,
        description,
        json!({
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "type": "object", "additionalProperties": false,
            "required": required, "properties": properties
        }),
    ))
}

fn narrow(definition: &ToolDefinition) -> Result<ToolDefinition, String> {
    let Some((title, description, schema)) = contract(&definition.id().to_string()) else {
        return Ok(definition.clone());
    };
    ToolDefinition::new(ToolDefinitionParts {
        id: definition.id().clone(),
        version: format!("{}-bounded-v1", definition.version()),
        title: title.to_owned(),
        description: description.to_owned(),
        input_schema: ToolSchema::from_value(schema).map_err(|error| error.to_string())?,
        output_schema: definition.output_schema().clone(),
        effects: definition.effects().clone(),
        risk: definition.risk().level(),
        required_scopes: definition.required_scopes().clone(),
        approval: definition.approval(),
        timeout_seconds: definition.timeout_seconds(),
        retry: RetryDeclaration {
            attempts: definition.retry().blind_retries(),
            backoff_ceiling_seconds: definition.retry().backoff_ceiling_seconds(),
        },
        idempotency: definition.idempotency(),
        source: definition.source(),
        availability: definition.availability().clone(),
        sensitivity: definition.sensitivity(),
    })
    .map_err(|error| error.to_string())
}

/// Keeps the canonical names, authority and output conversion of the real adapter.
pub(super) fn adapt(
    definitions: Vec<ToolDefinition>,
    inner: Arc<dyn ToolExecutor>,
) -> (Vec<ToolDefinition>, Arc<dyn ToolExecutor>) {
    let definitions: Vec<_> = definitions
        .into_iter()
        .filter_map(|definition| match narrow(&definition) {
            Ok(narrowed) => Some(narrowed),
            Err(reason) => {
                tracing::warn!(tool = %definition.id(), %reason, "a browser reading contract could not be built");
                None
            }
        })
        .collect();
    let schemas = definitions
        .iter()
        .filter(|definition| contract(&definition.id().to_string()).is_some())
        .map(|definition| {
            (
                definition.id().to_string(),
                definition.input_schema().clone(),
            )
        })
        .collect();
    (definitions, Arc::new(BrowserReader { inner, schemas }))
}

struct BrowserReader {
    inner: Arc<dyn ToolExecutor>,
    schemas: BTreeMap<String, ToolSchema>,
}

fn navigation_url(arguments: &Value) -> Result<(), AdapterError> {
    let valid = arguments
        .get("url")
        .and_then(Value::as_str)
        .is_some_and(|text| {
            reqwest::Url::parse(text).is_ok_and(|url| {
                matches!(url.scheme(), "http" | "https")
                    && url.host_str().is_some()
                    && url.username().is_empty()
                    && url.password().is_none()
                    && !text.chars().any(char::is_control)
            })
        });
    if valid {
        Ok(())
    } else {
        Err(AdapterError::RefusedBeforeReaching {
            reason: "browser navigation requires a credential-free HTTP(S) website URL".to_owned(),
        })
    }
}

#[async_trait::async_trait]
impl ToolExecutor for BrowserReader {
    fn adapter_id(&self) -> &'static str {
        self.inner.adapter_id()
    }

    async fn execute(
        &self,
        request: &ToolExecutionRequest,
    ) -> Result<ToolCallResult, AdapterError> {
        let id = request.tool().to_string();
        if contract(&id).is_some() {
            let schema =
                self.schemas
                    .get(&id)
                    .ok_or_else(|| AdapterError::RefusedBeforeReaching {
                        reason: "the browser reading contract is unavailable".to_owned(),
                    })?;
            if !schema
                .validate(request.arguments())
                .is_ok_and(|report| report.is_valid())
            {
                return Err(AdapterError::RefusedBeforeReaching {
                    reason: "browser reading arguments must match the bounded navigate/url, literal-find/text or inline snapshot contract; regex, scripts and filenames are not accepted".to_owned(),
                });
            }
            if request.tool().name() == "browser_navigate" {
                navigation_url(request.arguments())?;
            }
        }
        // No argument rewrite: the receipt's digest still describes exactly what the provider receives.
        self.inner.execute(request).await
    }
}

#[cfg(test)]
#[path = "mcp_browser_tests.rs"]
mod tests;
