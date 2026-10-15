//! The tools a **model** keeps its contact list with: save, search, stats, duplicates, merge, delete and CSV import.
//! Deleting or importing contacts asks for approval by default; deletion removes the whole record, including suppression.
//!
//! A prospecting project that keeps its leads in a spreadsheet loses them whenever the file is rewritten, and a run cannot ask "who have we
//! already contacted?". These tools read and write the local contact list; nothing here sends anything to anyone.
//!
//! # Why ordinary saves do not ask
//!
//! Saving a contact is writing a row in the owner's own database, the same class of effect as a project journal entry, and it is only data:
//! a later run reads it back fenced as data, never as an instruction. The one rule that matters is enforced in storage rather than in a
//! prompt: a model cannot move a contact out of `do_not_contact`, because that is the owner's word.

use std::sync::Arc;

use async_trait::async_trait;
use jarvis_core::{IsolatedText, Sensitivity, SystemClock, UtcTimestamp};
use jarvis_storage::{ContactInput, ContactStatus, DatabaseError, SqliteDatabase, StoredContact};
use jarvis_tools::{
    AdapterError, ApprovalPolicy, Availability, BoundedOutput, EffectSet, Idempotency,
    ProviderEvidence, RetryDeclaration, SchemaError, Scope, ScopeError, ScopeSet, ToolCallResult,
    ToolDefinition, ToolDefinitionError, ToolDefinitionParts, ToolEffect, ToolExecutionRequest,
    ToolExecutor, ToolId, ToolIdError, ToolOutcomeRecord, ToolSchema, ToolSensitivity, ToolSource,
    WorkspaceRoots,
};
use serde_json::{Value, json};
use thiserror::Error;

use crate::project_context::site_of_call;

#[path = "contact_import.rs"]
mod contact_import;

/// Removes one contact, including its do-not-contact status, under the normal deletion policy.
pub const DELETE_TOOL: &str = "jarvis.contacts.delete";
/// Imports contacts atomically from a CSV in a granted folder.
pub const IMPORT_TOOL: &str = "jarvis.contacts.import";
/// Saves or updates one contact.
pub const SAVE_TOOL: &str = "jarvis.contacts.save";
/// Searches the contacts.
pub const SEARCH_TOOL: &str = "jarvis.contacts.search";
/// Lists entries that look like the same contact, with what a merge would make of them.
pub const DUPLICATES_TOOL: &str = "jarvis.contacts.duplicates";
/// Folds one contact into another.
pub const MERGE_TOOL: &str = "jarvis.contacts.merge";
/// Counts the contacts by status.
pub const STATS_TOOL: &str = "jarvis.contacts.stats";
/// The scope a caller must hold to write contacts.
pub const WRITE_SCOPE: &str = "contacts.write";
/// The scope a caller must hold to read contacts.
pub const READ_SCOPE: &str = "contacts.read";

const SAVE_INPUT: &str = r#"{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "type": "object",
  "additionalProperties": false,
  "properties": {
    "company": { "type": "string", "maxLength": 200, "description": "The company. Required for a new contact." },
    "person": { "type": "string", "maxLength": 200 },
    "role": { "type": "string", "maxLength": 200 },
    "email": { "type": "string", "maxLength": 254, "description": "Matches an existing contact by address, so saving the same lead again updates it." },
    "phone": { "type": "string", "maxLength": 40 },
    "status": { "type": "string", "enum": ["new", "contacted", "replied", "meeting", "won", "lost", "do_not_contact"], "description": "Where they stand. You cannot move a contact out of do_not_contact: that is the owner's word." },
    "notes": { "type": "string", "maxLength": 2000, "description": "Replaces the old notes, so read them first (jarvis.contacts.search) and keep what matters." },
    "source": { "type": "string", "maxLength": 300, "description": "Where the lead came from: a page, a list, a person." }
  }
}"#;

const SEARCH_INPUT: &str = r#"{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "type": "object",
  "additionalProperties": false,
  "properties": {
    "query": { "type": "string", "maxLength": 200, "description": "Text to find in the company, person, address or notes. Omit to list." },
    "status": { "type": "string", "enum": ["new", "contacted", "replied", "meeting", "won", "lost", "do_not_contact"] },
    "limit": { "type": "integer", "minimum": 1, "maximum": 50, "description": "How many to return (20 by default)." }
  }
}"#;

const STATS_INPUT: &str = r#"{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "type": "object",
  "additionalProperties": false,
  "properties": {}
}"#;

const DUPLICATES_INPUT: &str = r#"{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "type": "object",
  "additionalProperties": false,
  "properties": {
    "limit": { "type": "integer", "minimum": 1, "maximum": 20, "description": "How many pairs to return (10 by default)." }
  }
}"#;

const MERGE_INPUT: &str = r#"{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "type": "object",
  "additionalProperties": false,
  "required": ["keep_id", "remove_id"],
  "properties": {
    "keep_id": { "type": "string", "minLength": 1, "maxLength": 64, "description": "The id of the contact to keep, from jarvis.contacts.duplicates or jarvis.contacts.search." },
    "remove_id": { "type": "string", "minLength": 1, "maxLength": 64, "description": "The id of the contact to fold into it; it is removed." }
  }
}"#;

const DELETE_INPUT: &str = r#"{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "type": "object",
  "additionalProperties": false,
  "required": ["id"],
  "properties": {
    "id": { "type": "string", "minLength": 1, "maxLength": 64, "description": "The contact id from jarvis.contacts.search. Deletion removes the whole contact, including any do_not_contact restriction, under the normal deletion approval policy." }
  }
}"#;

const IMPORT_INPUT: &str = r#"{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "type": "object",
  "additionalProperties": false,
  "required": ["path"],
  "properties": {
    "path": { "type": "string", "minLength": 1, "maxLength": 260, "description": "Relative CSV path inside a granted folder. UTF-8 (optional BOM), comma-separated, with a company header and optional person, role, email, phone, status, notes, source. At most 1,000 rows and 2 MiB. All rows succeed or nothing changes." }
  }
}"#;

const OUTPUT: &str = r#"{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "type": "object"
}"#;

const TIMEOUT_SECONDS: u32 = 15;
const DEFAULT_LIMIT: u32 = 20;
const MAX_LIMIT: u32 = 50;

/// Why a tool's own contract could not be built.
#[derive(Debug, Error)]
pub enum ContactsToolError {
    /// A tool identifier constant was rejected.
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

/// The adapter behind the contact tools.
pub struct ContactsTool {
    database: Arc<SqliteDatabase>,
    roots: Option<Arc<WorkspaceRoots>>,
}

struct Spec {
    id: &'static str,
    title: &'static str,
    description: &'static str,
    input: &'static str,
    effect: ToolEffect,
    risk: u8,
    scope: &'static str,
    approval: ApprovalPolicy,
}

impl ContactsTool {
    /// Builds the adapter over the daemon's database.
    #[must_use]
    pub const fn new(database: Arc<SqliteDatabase>) -> Self {
        Self {
            database,
            roots: None,
        }
    }

    /// Builds the adapter with the granted folders from which CSV contacts may be imported.
    #[must_use]
    pub fn with_roots(database: Arc<SqliteDatabase>, roots: Arc<WorkspaceRoots>) -> Self {
        let mut tool = Self::new(database);
        tool.roots = Some(roots);
        tool
    }

    /// The tools' definitions.
    ///
    /// # Errors
    ///
    /// Returns [`ContactsToolError`] when a constant of a contract is rejected: a configuration fault, so the daemon fails at startup
    /// rather than on the first call.
    pub fn definitions() -> Result<Vec<ToolDefinition>, ContactsToolError> {
        let specs = [
            Spec {
                id: SAVE_TOOL,
                title: "Save a contact",
                description: "Saves a lead, customer or other contact in the local contact list, or updates it when the address (or company and person) \
                              is already there. Use it for every lead you find or contact, with where it stands, so the next run does not repeat the work. \
                              It sends nothing to anyone. A contact the owner marked do_not_contact stays that way.",
                input: SAVE_INPUT,
                effect: ToolEffect::Write,
                risk: 1,
                scope: WRITE_SCOPE,
                approval: ApprovalPolicy::Auto,
            },
            Spec {
                id: SEARCH_TOOL,
                title: "Search contacts",
                description: "Finds contacts by text and status, most useful before approaching anyone: check whether they are already known, what was \
                              said, and whether they are do_not_contact (never approach those).",
                input: SEARCH_INPUT,
                effect: ToolEffect::ReadOnly,
                risk: 0,
                scope: READ_SCOPE,
                approval: ApprovalPolicy::Auto,
            },
            Spec {
                id: STATS_TOOL,
                title: "Count contacts by status",
                description: "How many contacts stand at each status, to report progress.",
                input: STATS_INPUT,
                effect: ToolEffect::ReadOnly,
                risk: 0,
                scope: READ_SCOPE,
                approval: ApprovalPolicy::Auto,
            },
            Spec {
                id: DUPLICATES_TOOL,
                title: "Find duplicate contacts",
                description: "Lists pairs of contacts that look like the same person or company (the same person at a company spelled two ways, or the \
                              same phone number), with the entry that would be kept and what a merge would make of them. It changes nothing: merge with \
                              jarvis.contacts.merge only when the owner agrees.",
                input: DUPLICATES_INPUT,
                effect: ToolEffect::ReadOnly,
                risk: 0,
                scope: READ_SCOPE,
                approval: ApprovalPolicy::Auto,
            },
            Spec {
                id: MERGE_TOOL,
                title: "Merge two contacts",
                description: "Folds one contact into another: the kept contact's words win, the other fills what is empty, a different value it held goes \
                              into the notes, and do_not_contact on either side stays. The other entry is removed, and is returned whole in the answer. \
                              The user is asked first.",
                input: MERGE_INPUT,
                effect: ToolEffect::Write,
                risk: 2,
                scope: WRITE_SCOPE,
                approval: ApprovalPolicy::Ask,
            },
            Spec {
                id: DELETE_TOOL,
                title: "Delete a contact",
                description: "Removes one contact from JARVIS only; it does not delete anything from email or external address books. Asks for approval by default, under the owner's normal standing-trust policy. Contacts marked do_not_contact can also be deleted: deletion removes the whole record and its restriction, so a later save or import can create it again.",
                input: DELETE_INPUT,
                effect: ToolEffect::Destructive,
                risk: 3,
                scope: WRITE_SCOPE,
                approval: ApprovalPolicy::Ask,
            },
            Spec {
                id: IMPORT_TOOL,
                title: "Import contacts from CSV",
                description: "Imports up to 1,000 contacts from a UTF-8 CSV file in a granted folder, asking for approval by default. The CSV has company and optional person, role, email, phone, status, notes and source headers; quoted commas and newlines are supported. Matching uses the same rules as save. Nothing is changed unless every row is valid; do_not_contact cannot be lifted. Pass a bounded path, not CSV text.",
                input: IMPORT_INPUT,
                effect: ToolEffect::Write,
                risk: 2,
                scope: WRITE_SCOPE,
                approval: ApprovalPolicy::Ask,
            },
        ];
        specs.iter().map(Self::definition).collect()
    }

    fn definition(spec: &Spec) -> Result<ToolDefinition, ContactsToolError> {
        Ok(ToolDefinition::new(ToolDefinitionParts {
            id: ToolId::new(spec.id)?,
            version: if spec.id == DELETE_TOOL {
                "1.1.0"
            } else {
                "1.0.0"
            }
            .to_owned(),
            title: spec.title.to_owned(),
            description: spec.description.to_owned(),
            input_schema: ToolSchema::parse(spec.input)?,
            output_schema: ToolSchema::parse(OUTPUT)?,
            effects: EffectSet::single(spec.effect),
            risk: spec.risk,
            required_scopes: if spec.id == IMPORT_TOOL {
                ScopeSet::new([Scope::new(spec.scope)?, Scope::new("files.read")?])
            } else {
                ScopeSet::single(Scope::new(spec.scope)?)
            },
            approval: spec.approval,
            timeout_seconds: TIMEOUT_SECONDS,
            // A retry would write twice.
            retry: RetryDeclaration::none(),
            idempotency: Idempotency::Unsupported,
            source: ToolSource::Native,
            availability: Availability::Available,
            sensitivity: ToolSensitivity::new(Sensitivity::Internal, Sensitivity::Internal),
        })?)
    }

    async fn workspace(&self) -> Result<String, AdapterError> {
        jarvis_storage::load_local_identity(&self.database)
            .await
            .map(|identity| identity.workspace_id().to_owned())
            .map_err(|_| refused("the local database is not available"))
    }

    async fn save(
        &self,
        call_id: &str,
        arguments: &Value,
        now: UtcTimestamp,
    ) -> Result<Value, AdapterError> {
        let text = |name: &str| {
            arguments
                .get(name)
                .and_then(Value::as_str)
                .map(str::to_owned)
        };
        let status = match arguments.get("status").and_then(Value::as_str) {
            Some(name) => Some(
                ContactStatus::parse(name)
                    .ok_or_else(|| refused("that status is not one of the known ones"))?,
            ),
            None => None,
        };
        let input = ContactInput {
            company: text("company"),
            person: text("person"),
            role: text("role"),
            email: text("email"),
            phone: text("phone"),
            status,
            notes: text("notes"),
            source: text("source"),
        };
        let workspace = self.workspace().await?;
        // A contact found inside a project is filed under it, so a project's leads can be told apart.
        let project = site_of_call(&self.database, call_id)
            .await
            .and_then(|site| site.project)
            .map(|project| project.id);
        let (contact, created) = jarvis_storage::save_contact(
            &self.database,
            &workspace,
            project.as_deref(),
            &input,
            false,
            now,
        )
        .await
        .map_err(|error| storage_refusal(&error))?;
        Ok(
            json!({ "saved": if created { "created" } else { "updated" }, "contact": describe(&contact) }),
        )
    }

    async fn search(&self, arguments: &Value) -> Result<Value, AdapterError> {
        let status = match arguments.get("status").and_then(Value::as_str) {
            Some(name) => Some(
                ContactStatus::parse(name)
                    .ok_or_else(|| refused("that status is not one of the known ones"))?,
            ),
            None => None,
        };
        let limit = arguments
            .get("limit")
            .and_then(Value::as_u64)
            .map_or(DEFAULT_LIMIT, |limit| {
                u32::try_from(limit).unwrap_or(MAX_LIMIT)
            })
            .clamp(1, MAX_LIMIT);
        let workspace = self.workspace().await?;
        let found = jarvis_storage::search_contacts(
            &self.database,
            &workspace,
            arguments.get("query").and_then(Value::as_str),
            status,
            limit,
        )
        .await
        .map_err(|_| refused("the contacts could not be read"))?;
        Ok(
            json!({ "count": found.len(), "contacts": found.iter().map(describe).collect::<Vec<_>>() }),
        )
    }

    async fn duplicates(&self, arguments: &Value) -> Result<Value, AdapterError> {
        let limit = arguments
            .get("limit")
            .and_then(Value::as_u64)
            .map_or(10, |limit| usize::try_from(limit).unwrap_or(20))
            .clamp(1, 20);
        let workspace = self.workspace().await?;
        let pairs = jarvis_storage::find_duplicate_contacts(&self.database, &workspace, limit)
            .await
            .map_err(|_| refused("the contacts could not be read"))?;
        let described: Vec<Value> = pairs
            .iter()
            .map(|pair| {
                json!({
                    "reason": pair.reason,
                    "keep": describe(&pair.keep),
                    "remove": describe(&pair.remove),
                    "would_become": describe(&jarvis_storage::combine(&pair.keep, &pair.remove)),
                })
            })
            .collect();
        Ok(json!({ "count": described.len(), "pairs": described }))
    }

    async fn merge(&self, arguments: &Value, now: UtcTimestamp) -> Result<Value, AdapterError> {
        let id = |name: &str| {
            arguments
                .get(name)
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|id| !id.is_empty() && id.len() <= 64)
                .map(str::to_owned)
                .ok_or_else(|| refused("keep_id and remove_id are both required"))
        };
        let (keep, remove) = (id("keep_id")?, id("remove_id")?);
        let workspace = self.workspace().await?;
        let (merged, removed) = jarvis_storage::merge_contacts(
            &self.database,
            &workspace,
            &keep,
            &remove,
            now,
        )
        .await
        .map_err(|error| match error {
            DatabaseError::InvalidContact { field: "id" } => refused(
                "those two ids are not two different contacts that exist; nothing was merged",
            ),
            _ => refused("the contacts could not be merged; nothing was changed"),
        })?;
        Ok(json!({
            "merged": describe(&merged),
            "removed": describe(&removed),
            "note": "the removed contact is shown whole above; to undo, save it again with jarvis.contacts.save",
        }))
    }

    async fn delete(&self, arguments: &Value) -> Result<Value, AdapterError> {
        let id = arguments
            .get("id")
            .and_then(Value::as_str)
            .filter(|id| !id.is_empty() && id.len() <= 64 && !id.chars().any(char::is_control))
            .ok_or_else(|| refused("a contact id from jarvis.contacts.search is required"))?;
        let workspace = self.workspace().await?;
        let deleted = jarvis_storage::delete_contact_for_model(&self.database, &workspace, id)
            .await
            .map_err(|_| refused("the contact could not be deleted"))?;
        Ok(json!({ "id": id, "deleted": deleted }))
    }

    async fn import(
        &self,
        call_id: &str,
        arguments: &Value,
        now: UtcTimestamp,
    ) -> Result<Value, AdapterError> {
        let path = arguments
            .get("path")
            .and_then(Value::as_str)
            .ok_or_else(|| refused("a CSV path inside a granted folder is required"))?;
        let roots = self
            .roots
            .as_ref()
            .ok_or_else(|| refused("no folders have been granted for contact import"))?;
        let inputs = contact_import::read(roots, path)?;
        let workspace = self.workspace().await?;
        let project = site_of_call(&self.database, call_id)
            .await
            .and_then(|site| site.project)
            .map(|project| project.id);
        let imported = jarvis_storage::import_contacts(
            &self.database,
            &workspace,
            project.as_deref(),
            &inputs,
            false,
            now,
        )
        .await
        .map_err(|error| {
            let reason = storage_refusal(&error);
            match reason {
                AdapterError::RefusedBeforeReaching { reason } => {
                    refused(&format!("{reason}; nothing was imported"))
                }
                other => other,
            }
        })?;
        Ok(json!({
            "imported": inputs.len(),
            "created": imported.created,
            "updated": imported.updated,
        }))
    }

    async fn stats(&self) -> Result<Value, AdapterError> {
        let workspace = self.workspace().await?;
        let counts = jarvis_storage::contact_stats(&self.database, &workspace)
            .await
            .map_err(|_| refused("the contacts could not be counted"))?;
        let total: u32 = counts.iter().map(|(_, held)| held).sum();
        let by_status: serde_json::Map<String, Value> = counts
            .into_iter()
            .map(|(status, held)| (status.as_str().to_owned(), json!(held)))
            .collect();
        Ok(json!({ "total": total, "by_status": by_status }))
    }
}

/// One contact as the model reads it. Notes and source are text a page or a person may have supplied, so they are fenced as data.
fn describe(contact: &StoredContact) -> Value {
    let fenced = |text: &str| {
        (!text.is_empty())
            .then(|| {
                IsolatedText::new(text)
                    .ok()
                    .map(|isolated| isolated.render())
            })
            .flatten()
    };
    json!({
        "id": contact.id,
        "company": contact.company,
        "person": contact.person,
        "role": contact.role,
        "email": contact.email,
        "phone": contact.phone,
        "status": contact.status.as_str(),
        "notes": fenced(&contact.notes),
        "source": fenced(&contact.source),
        "updated_at": contact.updated_at,
    })
}

fn storage_refusal(error: &DatabaseError) -> AdapterError {
    match error {
        DatabaseError::InvalidContact { field: "status" } => {
            refused("this contact is do_not_contact, and only the owner can change that")
        }
        DatabaseError::InvalidContact { field: "company" } => {
            refused("a new contact needs a company")
        }
        DatabaseError::InvalidContact { field } => refused(&format!(
            "the contact's {field} is not acceptable (too long, or not valid)"
        )),
        DatabaseError::ContactLimit => refused("the contact list is full"),
        _ => refused("the contact could not be saved"),
    }
}

fn refused(reason: &str) -> AdapterError {
    AdapterError::RefusedBeforeReaching {
        reason: reason.to_owned(),
    }
}

impl std::fmt::Debug for ContactsTool {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ContactsTool")
            .field("adapter_id", &self.adapter_id())
            .finish_non_exhaustive()
    }
}

#[async_trait]
impl ToolExecutor for ContactsTool {
    fn adapter_id(&self) -> &'static str {
        "contacts"
    }

    async fn execute(
        &self,
        request: &ToolExecutionRequest,
    ) -> Result<ToolCallResult, AdapterError> {
        let tool = request.tool().to_string();
        let now = UtcTimestamp::now(&SystemClock);
        if request.is_past_deadline(now) {
            return Err(refused("the call was already past its deadline"));
        }
        let (call, arguments) = (request.call_id(), request.arguments());
        let body = match tool.as_str() {
            SAVE_TOOL => self.save(call, arguments, now).await?,
            SEARCH_TOOL => self.search(arguments).await?,
            STATS_TOOL => self.stats().await?,
            DUPLICATES_TOOL => self.duplicates(arguments).await?,
            MERGE_TOOL => self.merge(arguments, now).await?,
            DELETE_TOOL => self.delete(arguments).await?,
            IMPORT_TOOL => self.import(call, arguments, now).await?,
            _ => return Err(AdapterError::NotImplemented { tool }),
        };
        let evidence = ProviderEvidence::new("contacts:store").ok();
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
#[path = "contacts_tool_tests.rs"]
mod tests;
