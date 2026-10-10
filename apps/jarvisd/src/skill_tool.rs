//! The tool a **model** teaches itself a procedure with: `jarvis.skill.propose` (`P11-004`, `ADR-0183`).
//!
//! After a task that took many steps, and that the owner is likely to ask again, the model can write down how it
//! was done: when to use it, and the tools to call in order. That is a *proposal*. It is stored as `Proposed`,
//! authored by the model's own actor, and **offered to no run** until the owner approves it with one click in the
//! console; until then it can change nothing, which is why the call itself needs no approval.
//!
//! A skill is not a permission. A step names a tool the daemon already has, and every step still goes through the
//! tool pipeline's policy, approvals and audit when a later run takes it.

use std::sync::{Arc, OnceLock, Weak};

use async_trait::async_trait;
use jarvis_core::{CorrelationId, Sensitivity, SystemClock, UtcTimestamp};
use jarvis_storage::SqliteDatabase;
use jarvis_tools::{
    AdapterError, ApprovalPolicy, Availability, BoundedOutput, EffectSet, Idempotency,
    ProviderEvidence, RetryDeclaration, SchemaError, Scope, ScopeError, ScopeSet, ToolCallResult,
    ToolDefinition, ToolDefinitionError, ToolDefinitionParts, ToolEffect, ToolExecutionRequest,
    ToolExecutor, ToolId, ToolIdError, ToolOutcomeRecord, ToolSchema, ToolSensitivity, ToolSource,
};
use serde_json::{Value, json};
use thiserror::Error;

use crate::skill_service::{SkillService, SkillServiceError};
use crate::tool_pipeline::ToolPipeline;

/// Proposes a skill for the owner to approve.
pub const PROPOSE_TOOL: &str = "jarvis.skill.propose";
/// The scope a caller must hold to propose a skill.
pub const PROPOSE_SCOPE: &str = "skill.propose";

const MAX_STEPS: usize = 12;
const TIMEOUT_SECONDS: u32 = 15;

const INPUT_SCHEMA: &str = r#"{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "type": "object",
  "additionalProperties": false,
  "required": ["description", "steps"],
  "properties": {
    "description": { "type": "string", "minLength": 10, "maxLength": 2000, "description": "The FIRST LINE says when to use this procedure, in the words the owner would use for the request, for example 'Research a Dutch company and add it to the prospects sheet.' A later run is offered the procedure when its request contains those words. Then, after a line break, what the procedure achieves and anything to watch for." },
    "steps": {
      "type": "array", "minItems": 1, "maxItems": 12,
      "description": "The tools to call, in order.",
      "items": {
        "type": "object", "additionalProperties": false, "required": ["tool", "instruction"],
        "properties": {
          "tool": { "type": "string", "maxLength": 120, "description": "The exact id of a tool you have, such as jarvis.web.search." },
          "instruction": { "type": "string", "minLength": 1, "maxLength": 600, "description": "What to do with it, and with what, in a sentence." }
        }
      }
    }
  }
}"#;

const OUTPUT_SCHEMA: &str = r#"{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "type": "object"
}"#;

/// Why the tool's own contract could not be built.
#[derive(Debug, Error)]
pub enum SkillToolError {
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

/// The adapter behind `jarvis.skill.propose`.
pub struct SkillProposeTool {
    database: Arc<SqliteDatabase>,
    /// Set once the pipeline exists, because the pipeline holds this adapter. A `Weak`, so the two do not keep each other alive.
    pipeline: OnceLock<Weak<ToolPipeline>>,
}

impl SkillProposeTool {
    /// Builds the adapter over the daemon's database.
    #[must_use]
    pub const fn new(database: Arc<SqliteDatabase>) -> Self {
        Self {
            database,
            pipeline: OnceLock::new(),
        }
    }

    /// Gives the adapter the pipeline whose tools a step may name. Called once, after composition.
    pub fn bind(&self, pipeline: &Arc<ToolPipeline>) {
        let _ = self.pipeline.set(Arc::downgrade(pipeline));
    }

    /// The tool's definition.
    ///
    /// # Errors
    ///
    /// Returns [`SkillToolError`] when a constant of the contract is rejected: a configuration fault, so the
    /// daemon fails at startup.
    pub fn definition() -> Result<ToolDefinition, SkillToolError> {
        Ok(ToolDefinition::new(ToolDefinitionParts {
            id: ToolId::new(PROPOSE_TOOL)?,
            version: "1.0.0".to_owned(),
            title: "Propose a skill".to_owned(),
            description: "Writes down a procedure you just worked out so you can follow it next time: when to use it and the \
                          tools to call in order. Use it after finishing a task that took many steps and that the owner is \
                          likely to ask for again, never for a one-off. It is only a proposal: it is not used until the \
                          owner approves it, and you must not describe it as learned."
                .to_owned(),
            input_schema: ToolSchema::parse(INPUT_SCHEMA)?,
            output_schema: ToolSchema::parse(OUTPUT_SCHEMA)?,
            effects: EffectSet::single(ToolEffect::Write),
            risk: 1,
            required_scopes: ScopeSet::single(Scope::new(PROPOSE_SCOPE)?),
            // The proposal is stored unused; approving it is the owner's act, in the console.
            approval: ApprovalPolicy::Auto,
            timeout_seconds: TIMEOUT_SECONDS,
            retry: RetryDeclaration::none(),
            idempotency: Idempotency::Unsupported,
            source: ToolSource::Native,
            availability: Availability::Available,
            sensitivity: ToolSensitivity::new(Sensitivity::Internal, Sensitivity::Internal),
        })?)
    }

    async fn propose(
        &self,
        arguments: &Value,
        correlation: CorrelationId,
    ) -> Result<Value, AdapterError> {
        let description = arguments
            .get("description")
            .and_then(Value::as_str)
            .ok_or_else(|| refused("a description is required"))?;
        let steps = parse_steps(arguments)?;
        let pipeline = self.pipeline.get().and_then(Weak::upgrade);
        let service = SkillService::new(Arc::clone(&self.database), pipeline.as_ref());
        match service.propose(description, &steps, correlation).await {
            Ok(reply) => Ok(json!({
                "outcome": reply.outcome,
                "revision_id": reply.reference.revision_id,
                "note": "Waiting for the owner to approve it. It is not used until then.",
            })),
            Err(SkillServiceError::Refused { detail, .. }) => Err(refused(&detail)),
            Err(error) => {
                tracing::warn!(%error, "a skill proposal could not be stored");
                Err(AdapterError::ProviderRefused {
                    reason: "the proposal could not be stored".to_owned(),
                })
            }
        }
    }
}

fn parse_steps(arguments: &Value) -> Result<Vec<(String, String)>, AdapterError> {
    let steps = arguments
        .get("steps")
        .and_then(Value::as_array)
        .ok_or_else(|| refused("steps are required"))?;
    if steps.is_empty() || steps.len() > MAX_STEPS {
        return Err(refused(&format!("give between 1 and {MAX_STEPS} steps")));
    }
    steps
        .iter()
        .map(|step| {
            let tool = step.get("tool").and_then(Value::as_str);
            let instruction = step.get("instruction").and_then(Value::as_str);
            match (tool, instruction) {
                (Some(tool), Some(instruction)) => {
                    Ok((tool.trim().to_owned(), instruction.to_owned()))
                }
                _ => Err(refused("every step needs a tool and an instruction")),
            }
        })
        .collect()
}

fn refused(reason: &str) -> AdapterError {
    AdapterError::RefusedBeforeReaching {
        reason: reason.to_owned(),
    }
}

impl std::fmt::Debug for SkillProposeTool {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SkillProposeTool")
            .field("adapter_id", &self.adapter_id())
            .finish_non_exhaustive()
    }
}

#[async_trait]
impl ToolExecutor for SkillProposeTool {
    fn adapter_id(&self) -> &'static str {
        "skill-propose"
    }

    async fn execute(
        &self,
        request: &ToolExecutionRequest,
    ) -> Result<ToolCallResult, AdapterError> {
        if request.tool().to_string() != PROPOSE_TOOL {
            return Err(AdapterError::NotImplemented {
                tool: request.tool().to_string(),
            });
        }
        let now = UtcTimestamp::now(&SystemClock);
        if request.is_past_deadline(now) {
            return Err(refused("the call was already past its deadline"));
        }
        let body = self
            .propose(request.arguments(), request.correlation_id())
            .await?;
        let evidence = ProviderEvidence::new("skill:store").ok();
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
#[path = "skill_tool_tests.rs"]
mod tests;
