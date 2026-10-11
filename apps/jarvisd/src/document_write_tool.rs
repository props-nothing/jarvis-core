//! `jarvis.files.write_document`: a new spreadsheet, CSV or Word file inside a granted folder (`P11-010`, `ADR-0185`).
//!
//! Reading documents exists (`jarvis.files.read_document`); this is the other half, so the owner's reports, price lists and prospect sheets
//! are made by JARVIS rather than by a script someone has to write. The file type comes from the extension. The content is a JSON
//! description (sheets of rows, or headings, paragraphs, lists and tables) that `jarvis-documents` checks, bounds and renders.
//!
//! It **creates a new file and never replaces one**, exactly like `jarvis.files.write`: an existing name is refused and the model is told to
//! choose another. That is why it carries the same risk (1) and needs no approval: it can add a file inside a folder the owner granted, and
//! cannot destroy anything. Everything the model gives is written as text, never as a formula.

use std::sync::Arc;

use async_trait::async_trait;
use jarvis_core::{Sensitivity, SystemClock, UtcTimestamp};
use jarvis_documents::{WriteError, write_csv, write_docx, write_pdf, write_xlsx};
use jarvis_tools::{
    AdapterError, ApprovalPolicy, Availability, BoundedOutput, EffectSet, Idempotency,
    ProviderEvidence, RetryDeclaration, SchemaError, Scope, ScopeError, ScopeSet, ToolCallResult,
    ToolDefinition, ToolDefinitionError, ToolDefinitionParts, ToolEffect, ToolExecutionRequest,
    ToolExecutor, ToolId, ToolIdError, ToolOutcomeRecord, ToolSchema, ToolSensitivity, ToolSource,
    WorkspaceRoots,
};
use serde_json::{Value, json};
use thiserror::Error;

/// Writes a spreadsheet, CSV or Word file.
pub const WRITE_DOCUMENT_TOOL: &str = "jarvis.files.write_document";

/// The largest file written, in bytes.
const MAX_OUTPUT_BYTES: usize = 10 * 1024 * 1024;

const INPUT: &str = r#"{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "type": "object",
  "additionalProperties": false,
  "required": ["path", "content"],
  "properties": {
    "path": { "type": "string", "minLength": 1, "maxLength": 260, "description": "A NEW path inside a granted folder ending in .xlsx, .csv, .docx or .pdf. It must not exist yet: this never replaces a file." },
    "content": {
      "type": "object",
      "description": "What to write. For .xlsx: {\"sheets\":[{\"name\":\"Prospects\",\"header\":[\"Company\",\"City\"],\"rows\":[[\"Acme\",\"Utrecht\"],[\"Beta\",null]]}]} (or header and rows directly for one sheet). Cells are text, numbers, true/false or null; everything is stored as given, never as a formula. For .csv: the same, one sheet. For .docx and .pdf: {\"title\":\"Report\",\"blocks\":[{\"type\":\"heading\",\"level\":1,\"text\":\"...\"},{\"type\":\"paragraph\",\"text\":\"...\"},{\"type\":\"bullets\",\"items\":[\"...\"]},{\"type\":\"numbered\",\"items\":[\"...\"]},{\"type\":\"table\",\"header\":[\"A\",\"B\"],\"rows\":[[\"1\",\"2\"]]}]}."
    },
    "delimiter": { "type": "string", "enum": [",", ";"], "description": "Only for .csv: the separator. Dutch and German Excel split on a semicolon; the default is a comma." }
  }
}"#;

const OUTPUT: &str = r#"{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "type": "object"
}"#;

/// Why the tool's own contract could not be built.
#[derive(Debug, Error)]
pub enum WriteDocumentToolError {
    /// An identifier constant was rejected.
    #[error(transparent)]
    Id(#[from] ToolIdError),
    /// A schema constant was rejected.
    #[error(transparent)]
    Schema(#[from] SchemaError),
    /// A scope constant was rejected.
    #[error(transparent)]
    Scope(#[from] ScopeError),
    /// A definition was rejected.
    #[error(transparent)]
    Definition(#[from] ToolDefinitionError),
}

/// The adapter behind `jarvis.files.write_document`.
pub struct WriteDocumentTool {
    roots: Arc<WorkspaceRoots>,
}

impl WriteDocumentTool {
    /// Builds the adapter over the folders the owner granted.
    #[must_use]
    pub const fn new(roots: Arc<WorkspaceRoots>) -> Self {
        Self { roots }
    }

    /// The tool's definition.
    ///
    /// # Errors
    ///
    /// Returns [`WriteDocumentToolError`] when a constant of the contract is rejected.
    pub fn definition() -> Result<ToolDefinition, WriteDocumentToolError> {
        Ok(ToolDefinition::new(ToolDefinitionParts {
            id: ToolId::new(WRITE_DOCUMENT_TOOL)?,
            version: "1.0.0".to_owned(),
            title: "Write a document".to_owned(),
            description: "Creates a NEW Excel workbook (.xlsx), CSV (.csv), Word document (.docx) or PDF (.pdf) inside a granted folder from the content you give: \
                          sheets of rows with a header, or headings, paragraphs, lists and tables. It never replaces a file: if the name exists, \
                          choose another. Text is written as text and never as a formula. Use it for reports, lists and spreadsheets the user asked for; \
                          for plain notes use jarvis.files.write."
                .to_owned(),
            input_schema: ToolSchema::parse(INPUT)?,
            output_schema: ToolSchema::parse(OUTPUT)?,
            effects: EffectSet::single(ToolEffect::Write),
            risk: 1,
            required_scopes: ScopeSet::single(Scope::new("files.write")?),
            approval: ApprovalPolicy::Auto,
            timeout_seconds: 60,
            retry: RetryDeclaration::none(),
            idempotency: Idempotency::Unsupported,
            source: ToolSource::Native,
            availability: Availability::Available,
            sensitivity: ToolSensitivity::new(Sensitivity::Internal, Sensitivity::Internal),
        })?)
    }

    fn write(&self, arguments: &Value) -> Result<Value, AdapterError> {
        let text = arguments
            .get("path")
            .and_then(Value::as_str)
            .ok_or_else(|| refused("a path is required"))?;
        let path = crate::google_files::plain_relative(text).ok_or_else(|| {
            refused("the path must be inside a granted folder (no drive, no .., not absolute)")
        })?;
        let content = arguments
            .get("content")
            .filter(|content| content.is_object())
            .ok_or_else(|| refused("content is required, an object describing what to write"))?;
        let extension = path
            .extension()
            .and_then(|extension| extension.to_str())
            .map(str::to_ascii_lowercase)
            .unwrap_or_default();
        let mut replaced = 0_usize;
        let (format, bytes) = match extension.as_str() {
            "xlsx" => ("xlsx", write_xlsx(content)),
            "docx" => ("docx", write_docx(content)),
            // The standard PDF fonts cover Western European text; what they cannot show is counted and reported below.
            "pdf" => (
                "pdf",
                write_pdf(content).map(|(bytes, count)| {
                    replaced = count;
                    bytes
                }),
            ),
            "csv" => {
                let delimiter = match arguments.get("delimiter").and_then(Value::as_str) {
                    None | Some(",") => ',',
                    Some(";") => ';',
                    Some(_) => return Err(refused("the delimiter must be , or ;")),
                };
                ("csv", write_csv(content, delimiter))
            }
            _ => {
                return Err(refused(
                    "the file must end in .xlsx, .csv, .docx or .pdf (slides are not written yet)",
                ));
            }
        };
        let bytes = bytes.map_err(|error| match error {
            WriteError::Engine(_) => AdapterError::ProviderRefused {
                reason: error.to_string(),
            },
            other => refused(&other.to_string()),
        })?;
        if bytes.len() > MAX_OUTPUT_BYTES {
            return Err(refused("the document would be larger than 10 MB"));
        }
        let (created, _) = self
            .roots
            .write_file(&path, &bytes, false)
            .map_err(|error| match error.kind() {
                std::io::ErrorKind::AlreadyExists => {
                    refused("a file with that name already exists; choose another name (this never replaces a file)")
                }
                std::io::ErrorKind::IsADirectory => refused("that path is a folder"),
                _ => refused("the file could not be written inside the granted folders"),
            })?;
        let mut answer = json!({
            "path": text,
            "format": format,
            "bytes": bytes.len(),
            "created": created,
        });
        if replaced > 0 {
            answer["replaced_characters"] = json!(replaced);
            answer["note"] = json!(
                "the PDF fonts cover Western European text only: those characters were drawn as ? . Tell the user, or write a .docx instead."
            );
        }
        Ok(answer)
    }
}

fn refused(reason: &str) -> AdapterError {
    AdapterError::RefusedBeforeReaching {
        reason: reason.to_owned(),
    }
}

impl std::fmt::Debug for WriteDocumentTool {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("WriteDocumentTool")
            .field("adapter_id", &self.adapter_id())
            .finish_non_exhaustive()
    }
}

#[async_trait]
impl ToolExecutor for WriteDocumentTool {
    fn adapter_id(&self) -> &'static str {
        "documents-write"
    }

    async fn execute(
        &self,
        request: &ToolExecutionRequest,
    ) -> Result<ToolCallResult, AdapterError> {
        let now = UtcTimestamp::now(&SystemClock);
        if request.is_past_deadline(now) {
            return Err(refused("the call was already past its deadline"));
        }
        if request.tool().to_string() != WRITE_DOCUMENT_TOOL {
            return Err(AdapterError::NotImplemented {
                tool: request.tool().to_string(),
            });
        }
        let arguments = request.arguments().clone();
        let roots = Arc::clone(&self.roots);
        // Building a large workbook is real work: off the async threads.
        let body =
            tokio::task::spawn_blocking(move || WriteDocumentTool { roots }.write(&arguments))
                .await
                .map_err(|_| refused("the document could not be built"))??;
        let evidence = ProviderEvidence::new("documents:write").ok();
        let record = evidence
            .as_ref()
            .and_then(|value| ToolOutcomeRecord::confirmed(value.as_str()).ok())
            .unwrap_or_else(|| {
                ToolOutcomeRecord::failed("evidence")
                    .unwrap_or_else(|_| unreachable!("a literal reason"))
            });
        Ok(ToolCallResult::new(
            record,
            evidence,
            Some(BoundedOutput::from_bounded(body.to_string(), false)),
            now,
        ))
    }
}

#[cfg(test)]
#[path = "document_write_tool_tests.rs"]
mod tests;
