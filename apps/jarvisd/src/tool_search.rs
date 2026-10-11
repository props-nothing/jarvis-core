//! Tool discovery: a large catalog costs no tokens until a tool is needed (`ADR-0176`, `P11-002`).
//!
//! Every model call carries the description and input schema of every tool it is offered. JARVIS's own tools are a fixed, modest set, but a
//! program added through MCP can bring dozens (the built-in browser has 25), and all of them would ride along on every call of every run.
//!
//! **The rule is about groups, not single tools.** The tools of one MCP server form a group (`mcp.<server>.<tool>`). A group with more than
//! [`DEFER_GROUP_OVER`] tools is **deferred**: its tools are not offered until the model asks. A small group (a documentation lookup with two
//! tools) is cheaper to offer than to search for, so it is offered as always. A few **built-in families that most runs never touch**
//! ([`DEFERRED_NATIVE_FAMILIES`]: calendar and scheduling) are deferred too, as the groups `jarvis-calendar` and `jarvis-schedule` (`ADR-0197`). The model is told, in the description of [`SEARCH_TOOL`], which
//! groups exist and what is in them, calls `jarvis.tools.search` with a few words, and from then on **the whole group** is offered for the rest of
//! the run (finding `browser_navigate` and then lacking `browser_snapshot` would be the worse trade).
//!
//! **Nothing is ever unreachable, and nothing is decided by the model about authority.** Discovery only changes what is *offered*. Policy, approval,
//! scopes and the audit trail see every call exactly as before, whether the tool was offered or not. The selection is deterministic: the same
//! catalog and the same query always give the same answer.
//!
//! **Nothing is stored.** Which groups a run has found is read back from the run's own transcript (the search result carries it), so a run that parks
//! for an approval and resumes still has them.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{OnceLock, Weak};

use async_trait::async_trait;
use jarvis_core::{Sensitivity, SystemClock, UtcTimestamp};
use jarvis_models::{ChatMessage, Role};
use jarvis_tools::{
    AdapterError, ApprovalPolicy, Availability, BoundedOutput, EffectSet, Idempotency,
    ProviderEvidence, RetryDeclaration, SchemaError, Scope, ScopeError, ScopeSet, ToolCallResult,
    ToolDefinition, ToolDefinitionError, ToolDefinitionParts, ToolEffect, ToolExecutionRequest,
    ToolExecutor, ToolId, ToolIdError, ToolOutcomeRecord, ToolSchema, ToolSensitivity, ToolSource,
};
use serde_json::{Value, json};
use thiserror::Error;

use crate::tool_pipeline::ToolPipeline;

/// The canonical identifier the model requests.
pub const SEARCH_TOOL: &str = "jarvis.tools.search";

/// The scope a caller must hold for the tool to be authorized at all.
pub const SEARCH_SCOPE: &str = "tools.search";

/// A group with more tools than this is deferred.
pub const DEFER_GROUP_OVER: usize = 6;

/// The most tools one search lists.
const MAX_LISTED: usize = 10;
const MAX_QUERY_CHARS: usize = 200;
const MAX_DESCRIPTION_CHARS: usize = 140;
const TIMEOUT_SECONDS: u32 = 10;

/// The `kind` a search result carries, which is how a run reads its own discoveries back from the transcript.
const RESULT_KIND: &str = "tool_discovery";

const INPUT_SCHEMA: &str = r#"{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "type": "object",
  "additionalProperties": false,
  "required": ["query"],
  "properties": {
    "query": { "type": "string", "minLength": 1, "maxLength": 200, "description": "A few English words about what you need to do, or the name of a group listed above." }
  }
}"#;

const OUTPUT_SCHEMA: &str = r#"{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "type": "object"
}"#;

/// Why the tool's own contract could not be built.
#[derive(Debug, Error)]
pub enum ToolSearchToolError {
    /// A tool identifier constant was rejected.
    #[error(transparent)]
    Id(#[from] ToolIdError),
    /// A schema constant was rejected.
    #[error(transparent)]
    Schema(#[from] SchemaError),
    /// A scope constant was rejected.
    #[error(transparent)]
    Scope(#[from] ScopeError),
    /// The definition was rejected.
    #[error(transparent)]
    Definition(#[from] ToolDefinitionError),
}

/// One tool as discovery sees it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Entry {
    /// The canonical identifier.
    pub id: String,
    /// The description the model reads.
    pub description: String,
}

/// Built-in tool families that are offered only when the model asks, because most runs never touch them and each rides along on every call.
/// Chosen by what the owner's runs actually use: a family used by routine work (files, web, mail, memory, contacts, projects, delegation,
/// the skills the model proposes) stays offered, since one search costs more than a few calls' worth of its definitions.
pub const DEFERRED_NATIVE_FAMILIES: &[&str] = &["calendar", "schedule"];

/// The prefix of a deferred built-in group's name, so it can never be mistaken for an MCP server of the same name.
const NATIVE_GROUP_PREFIX: &str = "jarvis-";

/// The group a tool belongs to: the server name of `mcp.<server>.<tool>`, `jarvis-<family>` for a deferrable built-in family, and nothing
/// for any other tool.
#[must_use]
pub fn group_of(id: &str) -> Option<String> {
    if let Some(rest) = id.strip_prefix("mcp.") {
        return rest.split('.').next().map(str::to_owned);
    }
    let family = id.strip_prefix("jarvis.")?.split('.').next()?;
    DEFERRED_NATIVE_FAMILIES
        .contains(&family)
        .then(|| format!("{NATIVE_GROUP_PREFIX}{family}"))
}

/// A tool's own name inside its group: `query-docs` for `mcp.docs.query-docs`, `add` for `jarvis.schedule.add`.
fn short_name(id: &str) -> &str {
    id.splitn(3, '.').nth(2).unwrap_or(id)
}

/// The deferred groups of a catalog, each with its tools, in a stable order.
#[must_use]
pub fn deferred_groups(entries: &[Entry]) -> BTreeMap<String, Vec<Entry>> {
    let mut groups: BTreeMap<String, Vec<Entry>> = BTreeMap::new();
    for entry in entries {
        if let Some(group) = group_of(&entry.id) {
            groups.entry(group).or_default().push(entry.clone());
        }
    }
    groups.retain(|group, tools| {
        group.starts_with(NATIVE_GROUP_PREFIX) || tools.len() > DEFER_GROUP_OVER
    });
    groups
}

/// Whether a tool result is a discovery answer. `found_groups` reads these back out of the transcript to decide which tools to offer, so
/// they must never be shortened (`ADR-0194`).
#[must_use]
pub fn is_discovery_result(text: &str) -> bool {
    text.starts_with('{') && text.contains(RESULT_KIND)
}

/// The groups this run has already found, read from its own transcript.
///
/// A group counts as found when a search result named it, or when the model has already called one of its tools (a run that knows a tool by
/// name does not need to search for it again, and a transcript that resumed after an approval must keep what it had).
#[must_use]
pub fn found_groups(messages: &[ChatMessage]) -> BTreeSet<String> {
    let mut found = BTreeSet::new();
    for message in messages {
        match message.role() {
            Role::Assistant => {
                for call in message.tool_calls() {
                    if let Some(group) = group_of(call.name()) {
                        found.insert(group);
                    }
                }
            }
            Role::Tool => {
                let text = message.text();
                if !text.starts_with('{') || !text.contains(RESULT_KIND) {
                    continue;
                }
                if let Ok(Value::Object(body)) = serde_json::from_str::<Value>(&text)
                    && body.get("kind").and_then(Value::as_str) == Some(RESULT_KIND)
                    && let Some(groups) = body.get("found_groups").and_then(Value::as_array)
                {
                    found.extend(groups.iter().filter_map(Value::as_str).map(str::to_owned));
                }
            }
            _ => {}
        }
    }
    found
}

/// The sentence in the search tool's description that says what is held back, so the model knows what to ask for.
#[must_use]
pub fn catalog_line(waiting: &BTreeMap<String, Vec<Entry>>) -> String {
    let mut parts = Vec::new();
    for (group, tools) in waiting {
        let names: Vec<&str> = tools
            .iter()
            .map(|entry| short_name(&entry.id))
            .take(8)
            .collect();
        parts.push(format!(
            "{group} ({} tools: {}{})",
            tools.len(),
            names.join(", "),
            if tools.len() > names.len() {
                ", ..."
            } else {
                ""
            }
        ));
    }
    parts.join("; ")
}

/// A word cut back to its stem, so "scheduling" reaches a tool described as "schedule" and "emails" one described as "email". Deliberately
/// crude (a few English endings, then a final silent `e`): the match is a substring test, so a stem only has to be a prefix of the words
/// it should find.
fn stem(word: &str) -> String {
    let mut stem = word;
    for ending in ["ing", "ed", "es", "s"] {
        if let Some(rest) = stem.strip_suffix(ending)
            && rest.chars().count() >= 4
        {
            stem = rest;
            break;
        }
    }
    if stem.chars().count() >= 5
        && let Some(rest) = stem.strip_suffix('e')
    {
        stem = rest;
    }
    stem.to_owned()
}

/// The words of a query that can say which tool is meant, as stems: longer than two letters and not a filler word.
fn query_words(query: &str) -> Vec<String> {
    query
        .split(|c: char| !c.is_alphanumeric())
        .filter(|word| word.chars().count() > 2)
        .map(str::to_lowercase)
        .filter(|word| !NOISE.contains(&word.as_str()))
        .map(|word| stem(&word))
        .collect()
}

/// Ranks the waiting tools for a query. Deterministic: the score counts query words found in the group name (3), the tool name (2) and the
/// description (1); ties keep the catalog order.
fn rank(waiting: &BTreeMap<String, Vec<Entry>>, query: &str) -> Vec<(usize, Entry)> {
    let words = query_words(query);
    let mut scored = Vec::new();
    for (group, tools) in waiting {
        for entry in tools {
            let name = short_name(&entry.id).to_lowercase();
            let description = entry.description.to_lowercase();
            let score: usize = words
                .iter()
                .map(|word| {
                    usize::from(group.to_lowercase().contains(word.as_str())) * 3
                        + usize::from(name.contains(word.as_str())) * 2
                        + usize::from(description.contains(word.as_str()))
                })
                .sum();
            if score > 0 {
                scored.push((score, entry.clone()));
            }
        }
    }
    scored.sort_by_key(|(score, _)| std::cmp::Reverse(*score));
    scored
}

/// The result of one search over what is waiting.
#[must_use]
pub fn search_body(waiting: &BTreeMap<String, Vec<Entry>>, query: &str) -> Value {
    if waiting.is_empty() {
        return json!({
            "kind": RESULT_KIND,
            "query": query,
            "found_groups": [],
            "tools": [],
            "note": "Nothing is waiting: every tool you can use is already offered to you.",
        });
    }
    let ranked = rank(waiting, query);
    if ranked.is_empty() {
        return json!({
            "kind": RESULT_KIND,
            "query": query,
            "found_groups": [],
            "tools": [],
            "note": format!("Nothing matched those words. Groups you can ask for by name: {}.", catalog_line(waiting)),
        });
    }
    let mut groups = BTreeSet::new();
    let mut listed = Vec::new();
    for (_, entry) in ranked.iter().take(MAX_LISTED) {
        if let Some(group) = group_of(&entry.id) {
            groups.insert(group);
        }
        let description: String = entry
            .description
            .chars()
            .take(MAX_DESCRIPTION_CHARS)
            .collect();
        listed.push(json!({ "id": entry.id, "description": description }));
    }
    let sizes: Vec<String> = groups
        .iter()
        .map(|group| format!("{group} ({} tools)", waiting.get(group).map_or(0, Vec::len)))
        .collect();
    json!({
        "kind": RESULT_KIND,
        "query": query,
        "found_groups": groups,
        "tools": listed,
        "note": format!("The whole set is now available to call: {}.", sizes.join(", ")),
    })
}

/// The most already-offered tools a search names.
const MAX_ALREADY_OFFERED: usize = 8;

/// Words that say nothing about which tool is meant.
const NOISE: &[&str] = &[
    "the", "and", "for", "with", "tool", "tools", "use", "want", "need", "can", "all", "any",
    "from", "that", "this", "how", "please", "what", "when", "where", "which", "who", "are", "was",
    "you", "your", "our", "not", "but", "has", "have", "does", "into", "about", "some", "get",
];

/// The tools the model **already has** that fit a query: native tools and small servers, which are always offered and so are never
/// among the held-back groups. A search for "gmail calendar contacts" used to answer "nothing matched" and send the model hunting for
/// tools it was already holding; this names them. Deterministic: id words count 3, description words 1.
#[must_use]
pub fn already_offered(
    entries: &[Entry],
    waiting: &BTreeMap<String, Vec<Entry>>,
    query: &str,
) -> Vec<Entry> {
    let words = query_words(query);
    let held: BTreeSet<&str> = waiting
        .values()
        .flatten()
        .map(|entry| entry.id.as_str())
        .collect();
    let mut scored: Vec<(usize, &Entry)> = entries
        .iter()
        .filter(|entry| !held.contains(entry.id.as_str()) && entry.id != SEARCH_TOOL)
        .filter_map(|entry| {
            let id = entry.id.to_lowercase();
            let description = entry.description.to_lowercase();
            let score: usize = words
                .iter()
                .map(|word| {
                    usize::from(id.contains(word.as_str())) * 3
                        + usize::from(description.contains(word.as_str()))
                })
                .sum();
            (score > 0).then_some((score, entry))
        })
        .collect();
    scored.sort_by_key(|(score, _)| std::cmp::Reverse(*score));
    scored
        .into_iter()
        .take(MAX_ALREADY_OFFERED)
        .map(|(_, entry)| entry.clone())
        .collect()
}

/// A search over everything: the held-back groups as before, plus the matching tools that are already offered.
#[must_use]
pub fn search_everything(
    entries: &[Entry],
    waiting: &BTreeMap<String, Vec<Entry>>,
    query: &str,
) -> Value {
    let mut body = search_body(waiting, query);
    let offered = already_offered(entries, waiting, query);
    if offered.is_empty() {
        return body;
    }
    let listed: Vec<Value> = offered
        .iter()
        .map(|entry| {
            let description: String = entry
                .description
                .chars()
                .take(MAX_DESCRIPTION_CHARS)
                .collect();
            json!({ "id": entry.id, "description": description })
        })
        .collect();
    let found_nothing_held = body["found_groups"].as_array().is_none_or(Vec::is_empty);
    if let Some(object) = body.as_object_mut() {
        object.insert("already_available".to_owned(), Value::Array(listed));
        if found_nothing_held {
            object.insert(
                "note".to_owned(),
                json!("These tools already match and are available to call now (no need to search for them again); nothing else was held back that matched."),
            );
        }
    }
    body
}

/// The adapter behind ``jarvis.tools.search``.
pub struct ToolSearchTool {
    /// Set once the pipeline exists, because the pipeline holds this adapter. A `Weak`, so the two do not keep each other alive.
    pipeline: OnceLock<Weak<ToolPipeline>>,
}

impl Default for ToolSearchTool {
    fn default() -> Self {
        Self::new()
    }
}

impl ToolSearchTool {
    /// An adapter that reads the catalog once it is bound.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            pipeline: OnceLock::new(),
        }
    }

    /// Gives the adapter the pipeline whose catalog it searches. Called once, after composition.
    pub fn bind(&self, pipeline: &std::sync::Arc<ToolPipeline>) {
        let _ = self.pipeline.set(std::sync::Arc::downgrade(pipeline));
    }

    /// The tool's definition.
    ///
    /// # Errors
    ///
    /// Returns [`ToolSearchToolError`] when a constant of the contract is rejected: a configuration fault, so the daemon fails at startup.
    pub fn definition() -> Result<ToolDefinition, ToolSearchToolError> {
        Ok(ToolDefinition::new(ToolDefinitionParts {
            id: ToolId::new(SEARCH_TOOL)?,
            version: "1.0.0".to_owned(),
            title: "Find more tools".to_owned(),
            description: "Finds tools that are not offered yet and makes them available. Say in a few words, in English, what you need to do (tool names and descriptions are English, whatever language the user writes in).".to_owned(),
            input_schema: ToolSchema::parse(INPUT_SCHEMA)?,
            output_schema: ToolSchema::parse(OUTPUT_SCHEMA)?,
            // It reads this daemon's own tool list and changes nothing.
            effects: EffectSet::single(ToolEffect::ReadOnly),
            risk: 0,
            required_scopes: ScopeSet::single(Scope::new(SEARCH_SCOPE)?),
            approval: ApprovalPolicy::Auto,
            timeout_seconds: TIMEOUT_SECONDS,
            retry: RetryDeclaration::none(),
            idempotency: Idempotency::Unsupported,
            source: ToolSource::Native,
            availability: Availability::Available,
            sensitivity: ToolSensitivity::new(Sensitivity::Internal, Sensitivity::Internal),
        })?)
    }
}

/// The catalog of a pipeline, as discovery sees it.
#[must_use]
pub fn entries_of(pipeline: &ToolPipeline) -> Vec<Entry> {
    pipeline
        .registry()
        .discover()
        .tools
        .iter()
        .map(|summary| Entry {
            id: summary.id.clone(),
            description: summary.description.clone(),
        })
        .collect()
}

impl std::fmt::Debug for ToolSearchTool {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ToolSearchTool")
            .field("adapter_id", &self.adapter_id())
            .finish_non_exhaustive()
    }
}

fn refused(reason: &str) -> AdapterError {
    AdapterError::RefusedBeforeReaching {
        reason: reason.to_owned(),
    }
}

#[async_trait]
impl ToolExecutor for ToolSearchTool {
    fn adapter_id(&self) -> &'static str {
        "tool-search"
    }

    async fn execute(
        &self,
        request: &ToolExecutionRequest,
    ) -> Result<ToolCallResult, AdapterError> {
        if request.tool().to_string() != SEARCH_TOOL {
            return Err(AdapterError::NotImplemented {
                tool: request.tool().to_string(),
            });
        }
        let now = UtcTimestamp::now(&SystemClock);
        if request.is_past_deadline(now) {
            return Err(refused("the call was already past its deadline"));
        }
        let query = request
            .arguments()
            .get("query")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|query| !query.is_empty())
            .ok_or_else(|| refused("a query is required"))?;
        if query.chars().count() > MAX_QUERY_CHARS {
            return Err(refused("the query is too long"));
        }
        let pipeline = self
            .pipeline
            .get()
            .and_then(Weak::upgrade)
            .ok_or_else(|| refused("the tool list is not available"))?;
        let entries = entries_of(&pipeline);
        let waiting = deferred_groups(&entries);
        let body = search_everything(&entries, &waiting, query);
        let evidence = ProviderEvidence::new("tools:search").ok();
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
mod tests {
    use super::*;

    fn entry(id: &str, description: &str) -> Entry {
        Entry {
            id: id.to_owned(),
            description: description.to_owned(),
        }
    }

    fn browser() -> Vec<Entry> {
        [
            ("browser_navigate", "Navigate to a URL"),
            (
                "browser_snapshot",
                "Capture an accessibility snapshot of the current page",
            ),
            ("browser_click", "Perform click on a web page"),
            ("browser_type", "Type text into an editable element"),
            (
                "browser_take_screenshot",
                "Take a screenshot of the current page",
            ),
            (
                "browser_tabs",
                "List, create, close, or select a browser tab",
            ),
            ("browser_wait_for", "Wait for text to appear or disappear"),
            ("browser_fill_form", "Fill multiple form fields"),
        ]
        .iter()
        .map(|(name, description)| entry(&format!("mcp.browser.{name}"), description))
        .collect()
    }

    fn catalog() -> Vec<Entry> {
        let mut entries = browser();
        entries.push(entry("mcp.docs.query-docs", "Look up documentation"));
        entries.push(entry("mcp.docs.resolve-library-id", "Find a library id"));
        entries.push(entry("jarvis.files.read", "Read a file"));
        entries
    }

    #[test]
    fn only_a_big_server_is_deferred_and_a_small_one_and_native_tools_are_offered() {
        let groups = deferred_groups(&catalog());
        assert_eq!(groups.keys().collect::<Vec<_>>(), ["browser"]);
        assert_eq!(groups["browser"].len(), 8);
        assert_eq!(group_of("mcp.docs.query-docs").as_deref(), Some("docs"));
        assert_eq!(group_of("jarvis.files.read"), None);
        assert_eq!(group_of("jarvis.gmail.send"), None);
    }

    /// **Rarely used built-in families are held back like a big server, under a name that cannot collide with an MCP server.**
    #[test]
    fn calendar_and_schedule_tools_are_deferred_under_their_own_group_names() {
        let mut entries = catalog();
        entries.push(entry("jarvis.calendar.events", "Lists calendar events"));
        entries.push(entry("jarvis.calendar.create", "Creates a calendar event"));
        entries.push(entry("jarvis.schedule.add", "Schedules a recurring task"));
        entries.push(entry("jarvis.schedule.list", "Lists scheduled tasks"));
        // An MCP server may be called "calendar" without being mistaken for the built-in family.
        entries.push(entry("mcp.calendar.list-events", "A server's events"));
        let groups = deferred_groups(&entries);
        assert_eq!(
            groups.keys().map(String::as_str).collect::<Vec<_>>(),
            ["browser", "jarvis-calendar", "jarvis-schedule"]
        );
        assert_eq!(groups["jarvis-calendar"].len(), 2);
        assert_eq!(
            group_of("mcp.calendar.list-events").as_deref(),
            Some("calendar")
        );
        assert_eq!(
            catalog_line(&groups),
            "browser (8 tools: browser_navigate, browser_snapshot, browser_click, browser_type, browser_take_screenshot, browser_tabs, browser_wait_for, browser_fill_form); jarvis-calendar (2 tools: events, create); jarvis-schedule (2 tools: add, list)"
        );

        let body = search_body(&groups, "what is on my calendar tomorrow");
        assert_eq!(body["found_groups"], json!(["jarvis-calendar"]));
        let body = search_body(&groups, "run this every morning on a schedule");
        assert_eq!(body["found_groups"], json!(["jarvis-schedule"]));
        // Word endings do not decide the match: "scheduling" and "events" reach "schedule" and "event".
        let body = search_body(&groups, "scheduling tasks");
        assert_eq!(body["found_groups"], json!(["jarvis-schedule"]));
        let body = search_body(&groups, "look at upcoming events");
        assert_eq!(body["found_groups"], json!(["jarvis-calendar"]));

        // Calling a tool of the family counts as having found it, so a resumed run keeps what it was using.
        let call = jarvis_models::ToolCall::new("c1", "jarvis.schedule.list", "{}");
        let messages = vec![ChatMessage::assistant_tool_calls("", vec![call])];
        assert_eq!(
            found_groups(&messages).into_iter().collect::<Vec<_>>(),
            ["jarvis-schedule"]
        );
    }

    #[test]
    fn a_stem_is_a_prefix_of_the_words_it_should_find() {
        for (word, expected) in [
            ("scheduling", "schedul"),
            ("schedule", "schedul"),
            ("scheduled", "schedul"),
            ("schedules", "schedul"),
            ("emails", "email"),
            ("events", "event"),
            ("sending", "send"),
            ("calendar", "calendar"),
            ("file", "file"),
            ("is", "is"),
        ] {
            assert_eq!(stem(word), expected, "{word}");
        }
    }

    #[test]
    fn a_search_finds_by_what_the_tool_does_and_names_the_whole_group() {
        let groups = deferred_groups(&catalog());
        let body = search_body(&groups, "open a web page and click a link");
        assert_eq!(body["kind"], RESULT_KIND);
        assert_eq!(body["found_groups"], json!(["browser"]));
        let ids: Vec<&str> = body["tools"]
            .as_array()
            .unwrap_or_else(|| panic!("tools is a list"))
            .iter()
            .filter_map(|tool| tool["id"].as_str())
            .collect();
        assert!(ids.contains(&"mcp.browser.browser_click"), "{ids:?}");
        assert!(
            body["note"]
                .as_str()
                .is_some_and(|note| note.contains("whole set"))
        );
    }

    #[test]
    fn a_group_can_be_asked_for_by_name_and_the_result_is_deterministic() {
        let groups = deferred_groups(&catalog());
        assert_eq!(
            search_body(&groups, "browser"),
            search_body(&groups, "browser")
        );
        assert_eq!(
            search_body(&groups, "browser")["found_groups"],
            json!(["browser"])
        );
    }

    #[test]
    fn a_query_that_matches_nothing_lists_what_can_be_asked_for_and_finds_nothing() {
        let groups = deferred_groups(&catalog());
        let body = search_body(&groups, "xyzzy plugh");
        assert_eq!(body["found_groups"], json!([]));
        assert!(
            body["note"]
                .as_str()
                .is_some_and(|note| note.contains("browser"))
        );
    }

    /// **A search for tools the model already holds names them, instead of saying nothing matched.**
    #[test]
    fn a_search_names_the_native_tools_already_offered() {
        let mut entries = catalog();
        entries.push(entry("jarvis.gmail.search", "Searches the owner's Gmail"));
        entries.push(entry("jarvis.gmail.send", "Sends an email"));
        entries.push(entry(
            "jarvis.memory.search",
            "Searches the owner's memories",
        ));
        entries.push(entry("jarvis.contacts.search", "Finds contacts"));
        entries.push(entry(SEARCH_TOOL, "Finds more tools"));
        let waiting = deferred_groups(&entries);
        let body = search_everything(&entries, &waiting, "gmail memories contacts");
        let ids: Vec<&str> = body["already_available"]
            .as_array()
            .unwrap_or_else(|| panic!("{body}"))
            .iter()
            .filter_map(|tool| tool["id"].as_str())
            .collect();
        for wanted in [
            "jarvis.gmail.search",
            "jarvis.gmail.send",
            "jarvis.memory.search",
        ] {
            assert!(ids.contains(&wanted), "{wanted} missing from {ids:?}");
        }
        assert!(
            !ids.iter().any(|id| id.starts_with("mcp.browser")),
            "a held-back tool is not already available"
        );
        assert!(!ids.contains(&SEARCH_TOOL));
        assert_eq!(body["found_groups"], json!([]));
        assert!(
            body["note"]
                .as_str()
                .is_some_and(|note| note.contains("already match")),
            "{body}"
        );
        // A query that finds a held-back group still opens it, and also lists what is already offered.
        let both = search_everything(&entries, &waiting, "click a link and search gmail");
        assert_eq!(both["found_groups"], json!(["browser"]));
        assert!(
            both["already_available"]
                .as_array()
                .is_some_and(|list| !list.is_empty())
        );
        // Noise words alone match nothing, and a query about nothing leaves the old answer untouched.
        assert!(already_offered(&entries, &waiting, "the tool for all").is_empty());
        assert!(
            search_everything(&entries, &waiting, "xyzzy plugh")
                .get("already_available")
                .is_none()
        );
    }

    #[test]
    fn with_nothing_deferred_the_search_says_everything_is_already_offered() {
        let body = search_body(&BTreeMap::new(), "anything");
        assert_eq!(body["found_groups"], json!([]));
        assert!(
            body["note"]
                .as_str()
                .is_some_and(|note| note.contains("already offered"))
        );
    }

    #[test]
    fn the_catalog_line_names_each_waiting_group_with_its_size_and_some_tools() {
        let line = catalog_line(&deferred_groups(&catalog()));
        assert!(
            line.starts_with("browser (8 tools: browser_navigate"),
            "{line}"
        );
    }

    /// The run finds its discoveries again from its own transcript, so a run that parked and resumed keeps them.
    #[test]
    fn a_runs_discoveries_are_read_back_from_its_transcript() {
        let groups = deferred_groups(&catalog());
        let result = search_body(&groups, "browser").to_string();
        let messages = vec![
            ChatMessage::user("open example.com"),
            ChatMessage::tool("call-1", result),
        ];
        assert_eq!(
            found_groups(&messages).into_iter().collect::<Vec<_>>(),
            ["browser"]
        );
        // Another tool's output that merely mentions the word is not a discovery.
        let other = vec![ChatMessage::tool("call-2", r#"{"note":"tool_discovery"}"#)];
        assert!(found_groups(&other).is_empty());
        assert!(found_groups(&[ChatMessage::user("hello")]).is_empty());
    }

    #[test]
    fn the_contract_is_read_only_and_unasked() {
        let definition = ToolSearchTool::definition().unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(definition.id().to_string(), SEARCH_TOOL);
        assert_eq!(definition.risk().level(), 0);
        assert_eq!(definition.approval(), ApprovalPolicy::Auto);
        assert!(definition.effects().contains(ToolEffect::ReadOnly));
    }
}
