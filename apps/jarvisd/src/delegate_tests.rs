//! End-to-end tests for delegation: a parent run starts a sub-agent through the real pipeline, store and executor.

use std::sync::Arc;

use jarvis_core::{
    ApprovalChannel, ApprovalDecision, ApprovalDecisionOutcome, CorrelationId, RunOutcome,
    SessionChannel,
};
use jarvis_models::{Role, Turn, scripted};
use jarvis_protocol::StartRunRequest;
use jarvis_storage::SqliteDatabase;

use super::*;
use crate::executor::{Executor, SCRIPTED_MODEL_NAME, execute_run_with_tools};
use crate::tool_pipeline::{ToolPipelineError, ToolPipelineOutcome};

struct Profile(std::path::PathBuf);

impl Drop for Profile {
    fn drop(&mut self) {
        jarvis_core::remove_scratch_dir(&self.0);
    }
}

struct Fixture {
    profile: Profile,
    database: Arc<SqliteDatabase>,
    runs: RunService,
    pipeline: Arc<ToolPipeline>,
    executor: Arc<Executor>,
    coding_calls: Arc<std::sync::atomic::AtomicUsize>,
}

struct CodingBoundaryProbe(Arc<std::sync::atomic::AtomicUsize>);

fn legacy_coding_definition(current: &ToolDefinition) -> ToolDefinition {
    ToolDefinition::new(ToolDefinitionParts {
        id: current.id().clone(),
        version: "1.0.0".into(),
        title: current.title().into(),
        description: current.description().into(),
        input_schema: current.input_schema().clone(),
        output_schema: current.output_schema().clone(),
        effects: EffectSet::single(ToolEffect::CodeExecution),
        risk: 3,
        required_scopes: current.required_scopes().clone(),
        approval: ApprovalPolicy::Ask,
        timeout_seconds: 60,
        retry: RetryDeclaration::none(),
        idempotency: Idempotency::Unsupported,
        source: ToolSource::Native,
        availability: Availability::Available,
        sensitivity: ToolSensitivity::new(Sensitivity::Internal, Sensitivity::Internal),
    })
    .unwrap_or_else(|e| panic!("{e}"))
}

#[tokio::test]
async fn delegation_child_role_refuses_direct_and_previously_approved_coding_calls() {
    let fixture = fixture().await;
    let child = start_child(&fixture, "stored child role").await;
    let definitions = fixture
        .pipeline
        .definitions()
        .unwrap_or_else(|e| panic!("{e}"));
    // Full owner-surface grants deliberately bypass the executor's discovery/scope filter.
    let actor = crate::tool_actor::ToolActor::for_composed_tools(
        &child.workspace_id,
        &child.run_id,
        SessionChannel::Cli,
        "policy-1",
        &definitions,
    );
    let args = json!({"task":"nested coding"});
    let direct = fixture
        .pipeline
        .call_tool(
            crate::code_delegate_async::START,
            args.clone(),
            &actor,
            CorrelationId::new(),
        )
        .await
        .unwrap_or_else(|e| panic!("{e}"));
    assert!(matches!(
        direct,
        ToolPipelineOutcome::Refused {
            reason_code: "child_cannot_delegate"
        }
    ));

    // Seed an approval under the historical catalog, which had no delegation effect.
    // Reopen with today's catalog: an old yes cannot elevate the stored child's role.
    let current = definitions
        .iter()
        .find(|d| d.id().to_string() == crate::code_delegate_async::START)
        .unwrap_or_else(|| panic!("start definition"));
    let legacy = legacy_coding_definition(current);
    let old = ToolPipeline::with_adapters(
        Arc::clone(&fixture.database),
        None,
        jarvis_tools::WorkspacePolicy::default(),
        vec![(
            vec![legacy],
            Arc::new(CodingBoundaryProbe(Arc::clone(&fixture.coding_calls)))
                as Arc<dyn ToolExecutor>,
        )],
    )
    .unwrap_or_else(|e| panic!("{e}"));
    let held = old
        .call_tool(
            crate::code_delegate_async::START,
            args.clone(),
            &actor,
            CorrelationId::new(),
        )
        .await
        .unwrap_or_else(|e| panic!("{e}"));
    let ToolPipelineOutcome::AwaitingApproval {
        call_id,
        approval_id,
        ..
    } = held
    else {
        panic!("expected old approval: {held:?}")
    };
    let decision = ApprovalDecision::new(
        ApprovalDecisionOutcome::Approve,
        ApprovalChannel::Cli,
        UtcTimestamp::now(&SystemClock),
        jarvis_storage::LOCAL_USER_ID,
    )
    .unwrap_or_else(|e| panic!("{e}"));
    jarvis_storage::record_decision(&fixture.database, &approval_id, &decision)
        .await
        .unwrap_or_else(|e| panic!("{e}"));
    let resumed = fixture
        .pipeline
        .resume(&call_id, args, &actor, CorrelationId::new())
        .await;
    assert!(
        matches!(
            resumed,
            Err(ToolPipelineError::ResumeRefused {
                reason: "a native child cannot delegate, including through an earlier approval",
                ..
            })
        ),
        "{resumed:?}"
    );
    assert_eq!(
        fixture
            .coding_calls
            .load(std::sync::atomic::Ordering::SeqCst),
        0
    );
    let stored = jarvis_storage::find_tool_call(&fixture.database, &call_id)
        .await
        .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(stored.outcome(), jarvis_core::ToolOutcome::Requested);
}

#[async_trait]
impl ToolExecutor for CodingBoundaryProbe {
    fn adapter_id(&self) -> &'static str {
        "coding-boundary-probe"
    }
    async fn execute(&self, _: &ToolExecutionRequest) -> Result<ToolCallResult, AdapterError> {
        self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Err(AdapterError::RefusedBeforeReaching {
            reason: "probe reached".into(),
        })
    }
}

async fn fixture() -> Fixture {
    fixture_with_policy(jarvis_tools::WorkspacePolicy::default()).await
}

async fn fixture_with_policy(policy: jarvis_tools::WorkspacePolicy) -> Fixture {
    let path = std::env::temp_dir().join(format!(
        "jarvis-delegate-{}-{}",
        std::process::id(),
        jarvis_core::RunId::new()
    ));
    std::fs::create_dir_all(&path).unwrap_or_else(|error| panic!("create profile: {error}"));
    let profile = Profile(path.clone());
    let database = Arc::new(
        SqliteDatabase::open(&path.join(jarvis_storage::DEFAULT_DATABASE_FILENAME))
            .await
            .unwrap_or_else(|error| panic!("open database: {error}")),
    );
    let executor = Arc::new(
        Executor::build(SCRIPTED_MODEL_NAME, None)
            .unwrap_or_else(|error| panic!("scripted executor: {error}")),
    );
    let agent = Arc::new(AgentTool::new(Arc::clone(&database), Arc::clone(&executor)));
    let coding_calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let pipeline = Arc::new(
        ToolPipeline::with_adapters(
            Arc::clone(&database),
            None,
            policy,
            vec![
                (
                    AgentTool::definitions().unwrap_or_else(|error| panic!("definitions: {error}")),
                    Arc::clone(&agent) as Arc<dyn ToolExecutor>,
                ),
                (
                    crate::code_delegate_async::AsyncCodeDelegateTool::definitions()
                        .unwrap_or_else(|e| panic!("coding definitions: {e}")),
                    Arc::new(CodingBoundaryProbe(Arc::clone(&coding_calls)))
                        as Arc<dyn ToolExecutor>,
                ),
            ],
        )
        .unwrap_or_else(|error| panic!("compose the pipeline: {error}")),
    );
    agent.bind(&pipeline);
    Fixture {
        profile,
        runs: RunService::new(Arc::clone(&database)),
        database,
        pipeline,
        executor,
        coding_calls,
    }
}

#[tokio::test]
async fn delegation_coding_launches_park_before_reaching_the_adapter() {
    let fixture = fixture().await;
    for tool in [
        crate::code_delegate::DELEGATE_TOOL,
        crate::code_delegate_async::START,
        crate::code_delegate_async::RESUME,
    ] {
        let parent = start(&fixture, "coding launch boundary").await;
        let args = if tool == crate::code_delegate_async::RESUME {
            json!({"task_id": jarvis_core::RunId::new().to_string()})
        } else {
            // Cooperative settings cannot turn host execution into ordinary native delegation.
            json!({"task":"inspect the project", "mode":"worktree", "allow_commands":false})
        };
        let model = parent_model(vec![Turn::tool_call("launch", tool, &args.to_string())]);
        let settled = drive_parent(&fixture, &model, &parent.run_id).await;
        assert_eq!(
            settled.state(),
            jarvis_core::RunState::AwaitingApproval,
            "{tool}"
        );
        let calls = jarvis_storage::read_run_tool_calls(&fixture.database, &parent.run_id)
            .await
            .unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].tool_version(), "1.1.0");
    }
    assert_eq!(
        fixture
            .coding_calls
            .load(std::sync::atomic::Ordering::SeqCst),
        0
    );
}

#[tokio::test]
async fn delegation_child_cannot_launch_coding_even_when_owner_trusts_it() {
    let mut policy = jarvis_tools::WorkspacePolicy::default();
    for tool in [
        crate::code_delegate::DELEGATE_TOOL,
        crate::code_delegate_async::START,
        crate::code_delegate_async::RESUME,
    ] {
        policy = policy.trusting(ToolId::new(tool).unwrap_or_else(|e| panic!("{e}")));
    }
    let fixture = fixture_with_policy(policy).await;
    let child = start_child(&fixture, "try coding without an objective marker").await;
    let offered = crate::executor::tool_specs_for_test(&fixture.pipeline, true);
    assert!(!offered.iter().any(|name| is_agent_management_tool(name)));
    let model = parent_model(vec![
        Turn::tool_call(
            "alias",
            crate::code_delegate::DELEGATE_TOOL,
            r#"{"task":"nested"}"#,
        ),
        Turn::tool_call(
            "start",
            crate::code_delegate_async::START,
            r#"{"task":"nested"}"#,
        ),
        Turn::tool_call(
            "resume",
            crate::code_delegate_async::RESUME,
            &json!({"task_id": jarvis_core::RunId::new().to_string()}).to_string(),
        ),
        Turn::answer("refused"),
    ]);
    let settled = drive_parent(&fixture, &model, &child.run_id).await;
    assert_eq!(settled.terminal_outcome(), Some(RunOutcome::Succeeded));
    assert_eq!(
        fixture
            .coding_calls
            .load(std::sync::atomic::Ordering::SeqCst),
        0
    );
    let results = tool_results(&model);
    assert_eq!(results.len(), 3);
    assert!(
        results
            .iter()
            .all(|result| result.contains("refused") || result.contains("could not be completed")),
        "{results:?}"
    );
    assert!(
        jarvis_storage::read_run_approvals(&fixture.database, &child.run_id)
            .await
            .unwrap_or_else(|e| panic!("{e}"))
            .is_empty(),
        "a scope denial must not become an approval"
    );
}

async fn start(fixture: &Fixture, objective: &str) -> jarvis_protocol::RunReply {
    fixture
        .runs
        .start(&StartRunRequest {
            objective: objective.to_owned(),
            session_id: None,
            idempotency_key: None,
            project_id: None,
        })
        .await
        .unwrap_or_else(|error| panic!("start: {error:?}"))
}

async fn start_child(fixture: &Fixture, objective: &str) -> jarvis_protocol::RunReply {
    let parent = start(fixture, "fixture parent").await;
    fixture
        .runs
        .start_delegated(
            &StartRunRequest {
                objective: objective.to_owned(),
                session_id: None,
                idempotency_key: None,
                project_id: None,
            },
            &parent.run_id,
        )
        .await
        .unwrap_or_else(|error| panic!("start child: {error:?}"))
}

fn parent_model(turns: Vec<Turn>) -> jarvis_models::ScriptedModel {
    scripted("scripted", "scripted-small", turns)
        .unwrap_or_else(|error| panic!("scripted model: {error}"))
}

async fn drive_parent(
    fixture: &Fixture,
    model: &jarvis_models::ScriptedModel,
    run_id: &str,
) -> jarvis_storage::StoredRun {
    execute_run_with_tools(
        &fixture.database,
        model,
        fixture.executor.model_id(),
        Some(&fixture.pipeline),
        run_id,
    )
    .await
    .unwrap_or_else(|error| panic!("drive: {error}"))
}

fn tool_results(model: &jarvis_models::ScriptedModel) -> Vec<String> {
    model
        .seen_messages()
        .last()
        .map(|messages| {
            messages
                .iter()
                .filter(|message| message.role() == Role::Tool)
                .map(|message| message.text().clone())
                .collect()
        })
        .unwrap_or_default()
}

#[tokio::test]
async fn retained_context_recovers_a_large_input_without_reading_another_conversation() {
    let fixture = fixture().await;
    let foreign = start(&fixture, "PRIVATE-OTHER-CONVERSATION").await;
    let messages = jarvis_storage::read_messages(&fixture.database, &foreign.session_id, 10)
        .await
        .unwrap_or_else(|error| panic!("{error}"));
    let original = format!("{}TAIL-OF-THE-FULL-REQUEST", "a".repeat(80_000));
    let parent = start(&fixture, &original).await;
    let model = parent_model(vec![
        Turn::tool_call("page", CONTEXT_TOOL, r#"{"source":"input","offset":80000}"#),
        Turn::tool_call(
            "foreign",
            CONTEXT_TOOL,
            &json!({"source":"message", "id": messages[0].id()}).to_string(),
        ),
        Turn::answer("finished"),
    ]);
    let settled = drive_parent(&fixture, &model, &parent.run_id).await;
    assert_eq!(settled.objective(), original);
    assert_eq!(settled.terminal_outcome(), Some(RunOutcome::Succeeded));
    let results = tool_results(&model);
    let page: serde_json::Value =
        serde_json::from_str(&results[0]).unwrap_or_else(|error| panic!("{error}"));
    assert!(
        page["content"]
            .as_str()
            .unwrap_or_default()
            .contains("TAIL-OF-THE-FULL-REQUEST")
    );
    assert!(page["next_offset"].is_null());
    assert!(results[1].contains("not available in this conversation"));
    assert!(!results[1].contains("PRIVATE-OTHER-CONVERSATION"));
    let seen = model.seen_messages();
    let initial = &seen[0];
    assert!(
        initial
            .iter()
            .any(|message| message.text().contains("jarvis.context.read"))
    );
    assert!(
        !initial
            .iter()
            .any(|message| message.text().contains("TAIL-OF-THE-FULL-REQUEST"))
    );
    fixture.database.close().await;
}

#[tokio::test]
async fn history_paging_discovers_messages_older_than_the_context_window() {
    let fixture = fixture().await;
    let first = start(&fixture, "oldest-message").await;
    for index in 0..45 {
        fixture
            .runs
            .start(&StartRunRequest {
                objective: format!("later-message-{index}"),
                session_id: Some(first.session_id.clone()),
                idempotency_key: None,
                project_id: None,
            })
            .await
            .unwrap_or_else(|error| panic!("{error:?}"));
    }
    let run = jarvis_storage::find_run(&fixture.database, &first.run_id)
        .await
        .unwrap_or_else(|error| panic!("{error}"));
    let agent = AgentTool::new(Arc::clone(&fixture.database), Arc::clone(&fixture.executor));
    let mut offset = 0;
    let mut found = String::new();
    loop {
        let page = agent
            .context_history(&run, &json!({"offset": offset}))
            .await
            .unwrap_or_else(|error| panic!("{error}"));
        found.push_str(
            page["content"]
                .as_str()
                .unwrap_or_else(|| panic!("missing fenced history")),
        );
        let Some(next) = page["next_offset"].as_i64() else {
            break;
        };
        assert!(next > offset);
        offset = next;
    }
    let messages = jarvis_storage::read_messages(&fixture.database, &first.session_id, 100)
        .await
        .unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(messages.len(), 46);
    for message in messages {
        assert!(found.contains(message.id()));
    }
    assert!(found.contains("oldest-message"));
    assert!(found.contains("later-message-44"));
    fixture.database.close().await;
}

#[tokio::test]
async fn a_long_delegated_answer_has_recoverable_pages() {
    let fixture = fixture().await;
    let child = start_child(&fixture, &format!("{DELEGATED_NOTICE}read everything")).await;
    let original = format!("{}LAST-PAGE", "full answer\n".repeat(1000));
    let model = parent_model(vec![Turn::answer(&original)]);
    let run = drive_parent(&fixture, &model, &child.run_id).await;
    let agent = AgentTool::new(Arc::clone(&fixture.database), Arc::clone(&fixture.executor));
    let mut offset = 0;
    loop {
        let page = agent
            .describe(&run, offset)
            .await
            .unwrap_or_else(|error| panic!("{error}"));
        assert!(page.to_string().chars().count() <= MAX_MODEL_FACING_RESULT_CHARS);
        assert_eq!(page["total_chars"], original.chars().count());
        let expected: String = original
            .chars()
            .skip(offset)
            .take(MAX_ANSWER_CHARS)
            .collect();
        assert_eq!(
            page["answer"],
            json!(IsolatedText::new(&expected).ok().map(|text| text.render()))
        );
        let Some(next) = page["next_offset"].as_u64() else {
            break;
        };
        assert_eq!(page["truncated"], true);
        let next = usize::try_from(next).unwrap_or_else(|error| panic!("{error}"));
        assert!(next > offset);
        offset = next;
    }
    assert!(offset > 3000);
    assert_eq!(model.calls(), 1);
    fixture.database.close().await;
}

/// **A parent run delegates, the sub-agent runs as an ordinary run, and its answer comes back fenced.**
#[tokio::test]
async fn a_delegated_task_runs_as_a_sub_agent_and_its_answer_is_fenced() {
    let fixture = fixture().await;
    let parent = start(&fixture, "split this work").await;
    let model = parent_model(vec![
        Turn::tool_call("c1", DELEGATE_TOOL, r#"{"task":"say hello"}"#),
        Turn::answer("The sub-agent finished."),
    ]);
    let settled = drive_parent(&fixture, &model, &parent.run_id).await;
    assert_eq!(settled.terminal_outcome(), Some(RunOutcome::Succeeded));

    let results = tool_results(&model);
    assert_eq!(results.len(), 1, "{results:?}");
    assert!(
        results[0].contains(r#""outcome":"completed""#),
        "{}",
        results[0]
    );
    // The sub-agent's answer (the scripted executor's fixed one) arrives inside the untrusted-data fence.
    assert!(
        results[0].contains(jarvis_core::FENCE_OPEN),
        "{}",
        results[0]
    );
    assert!(
        results[0].contains("No language model is configured"),
        "{}",
        results[0]
    );

    // The sub-agent is an ordinary, settled run in the same workspace, marked as one.
    let recent = jarvis_storage::read_recent_runs(&fixture.database, &parent.workspace_id, 10)
        .await
        .unwrap_or_else(|error| panic!("read runs: {error}"));
    let children: Vec<_> = recent.iter().filter(|run| run.is_delegated()).collect();
    assert_eq!(children.len(), 1);
    assert!(children[0].objective().ends_with("say hello"));
    assert_eq!(children[0].parent_run_id(), Some(parent.run_id.as_str()));
    let reply = fixture
        .runs
        .read(children[0].id())
        .await
        .unwrap_or_else(|error| panic!("{error:?}"));
    assert!(reply.is_delegated);
    assert_eq!(reply.parent_run_id, Some(parent.run_id.clone()));
    let calls = jarvis_storage::read_run_tool_calls(&fixture.database, &parent.run_id)
        .await
        .unwrap_or_else(|error| panic!("{error}"));
    let agent = AgentTool::new(Arc::clone(&fixture.database), Arc::clone(&fixture.executor));
    agent.bind(&fixture.pipeline);
    let adopted = agent
        .delegate(
            calls[0].id(),
            &json!({"task":"say hello","background":true}),
        )
        .await
        .unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(adopted["outcome"], "existing");
    assert_eq!(adopted["run_id"], children[0].id());
    let after = jarvis_storage::read_delegated_runs(&fixture.database, &parent.workspace_id, 10)
        .await
        .unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(after.len(), 1);
    assert_eq!(&after[0], children[0]);
}

/// **Delegation is one level deep: a sub-agent is offered no delegation tool and is refused one it names anyway.**
#[tokio::test]
async fn a_sub_agent_cannot_delegate() {
    let fixture = fixture().await;
    let offered = |sub_agent: bool| -> Vec<String> {
        crate::executor::tool_specs_for_test(&fixture.pipeline, sub_agent)
    };
    assert!(
        offered(false).iter().any(|name| name == DELEGATE_TOOL),
        "a parent must be offered delegation"
    );
    assert!(
        !offered(true)
            .iter()
            .any(|name| name.starts_with(AGENT_TOOL_PREFIX)),
        "a sub-agent must be offered no delegation tool: {:?}",
        offered(true)
    );

    // And a call it makes anyway is refused, because its authority has no `agent.delegate` scope.
    let child = start_child(&fixture, "try to delegate without any objective marker").await;
    let model = parent_model(vec![
        Turn::tool_call("c1", DELEGATE_TOOL, r#"{"task":"again"}"#),
        Turn::answer("I could not."),
    ]);
    let settled = drive_parent(&fixture, &model, &child.run_id).await;
    assert_eq!(settled.terminal_outcome(), Some(RunOutcome::Succeeded));
    let results = tool_results(&model);
    assert!(
        results[0].contains("refused") || results[0].contains("could not be completed"),
        "a sub-agent's call to delegate must be refused: {results:?}"
    );
    let recent = jarvis_storage::read_recent_runs(&fixture.database, &child.workspace_id, 10)
        .await
        .unwrap_or_else(|error| panic!("read runs: {error}"));
    assert_eq!(
        recent.iter().filter(|run| run.is_delegated()).count(),
        1,
        "no second sub-agent may have been started"
    );
}

/// **`result` reads only sub-agents.** Asked for an ordinary run, it refuses and shows nothing of it.
#[tokio::test]
async fn result_refuses_a_run_that_is_not_a_sub_agent() {
    let fixture = fixture().await;
    let ordinary = start(
        &fixture,
        &format!("{DELEGATED_NOTICE}an ordinary private run"),
    )
    .await;
    assert!(
        !ordinary.is_delegated,
        "objective text cannot mint child identity"
    );
    let parent = start(&fixture, "collect it").await;
    let model = parent_model(vec![
        Turn::tool_call(
            "c1",
            RESULT_TOOL,
            &format!(r#"{{"run_id":"{}"}}"#, ordinary.run_id),
        ),
        Turn::answer("ok"),
    ]);
    drive_parent(&fixture, &model, &parent.run_id).await;
    let results = tool_results(&model);
    assert!(results[0].contains(r#""outcome":"refused""#), "{results:?}");
    assert!(!results[0].contains("private"), "{results:?}");
    let _ = (SystemClock, UtcTimestamp::now(&SystemClock));
}

/// **A sub-agent parked for approval is reported as waiting for the person, never approved on the parent's behalf.**
#[tokio::test]
async fn a_parked_sub_agent_is_reported_as_waiting_for_the_user() {
    let fixture = fixture().await;
    let child = start_child(&fixture, &format!("{DELEGATED_NOTICE}do something held")).await;
    let mut run = jarvis_storage::find_run(&fixture.database, &child.run_id)
        .await
        .unwrap_or_else(|error| panic!("read the child: {error}"));
    for next in [
        jarvis_core::RunState::ContextBuilding,
        jarvis_core::RunState::Planning,
        jarvis_core::RunState::AwaitingApproval,
    ] {
        run = jarvis_storage::transition_run(
            &fixture.database,
            run.id(),
            run.expectation(),
            &jarvis_core::RunTransition::new(next, next.required_outcome(), None)
                .unwrap_or_else(|error| panic!("transition: {error}")),
            jarvis_core::UtcTimestamp::now(&jarvis_core::SystemClock),
        )
        .await
        .unwrap_or_else(|error| panic!("advance to {next:?}: {error}"));
    }

    let parent = start(&fixture, "collect it").await;
    let model = parent_model(vec![
        Turn::tool_call(
            "c1",
            RESULT_TOOL,
            &format!(r#"{{"run_id":"{}","wait_seconds":5}}"#, child.run_id),
        ),
        Turn::answer("It is waiting."),
    ]);
    drive_parent(&fixture, &model, &parent.run_id).await;
    let results = tool_results(&model);
    assert!(
        results[0].contains(r#""outcome":"waiting_for_approval""#),
        "{results:?}"
    );
    assert_eq!(
        jarvis_storage::find_run(&fixture.database, &child.run_id)
            .await
            .unwrap_or_else(|error| panic!("read the child: {error}"))
            .state(),
        jarvis_core::RunState::AwaitingApproval,
        "reading a sub-agent must not move it"
    );
}

/// **`list` names the sub-agents that hold the slots, with their ids and state, and shows nothing of an ordinary run.**
///
/// The audit (cycle 7): four invisible sub-agents blocked every new delegation and nothing could say which they were.
#[tokio::test]
async fn list_names_the_sub_agents_holding_the_slots_and_nothing_else() {
    let fixture = fixture().await;
    let _private = start(&fixture, "an ordinary private run").await;
    let waiting = start_child(
        &fixture,
        &format!("{DELEGATED_NOTICE}research the first company"),
    )
    .await;
    let second = start_child(
        &fixture,
        &format!("{DELEGATED_NOTICE}research the second company"),
    )
    .await;
    let parent = start(&fixture, "what is running").await;
    let model = parent_model(vec![
        Turn::tool_call("c1", LIST_TOOL, "{}"),
        Turn::answer("Two are going."),
    ]);
    drive_parent(&fixture, &model, &parent.run_id).await;
    let results = tool_results(&model);
    assert!(results[0].contains(r#""outcome":"listed""#), "{results:?}");
    assert!(results[0].contains(&waiting.run_id), "{results:?}");
    assert!(results[0].contains(&second.run_id), "{results:?}");
    assert!(
        results[0].contains("research the first company"),
        "{results:?}"
    );
    // The task is data a model wrote: fenced. An ordinary run is not listed, and its words never appear.
    assert!(results[0].contains(jarvis_core::FENCE_OPEN), "{results:?}");
    assert!(!results[0].contains("private"), "{results:?}");
    assert!(!results[0].contains(&parent.run_id), "{results:?}");
    assert!(results[0].contains(r#""limit":4"#), "{results:?}");
}

/// **A sub-agent parked for an approval holds no slot, so it cannot block new work; a pile of them is still capped.**
#[tokio::test]
async fn parked_sub_agents_do_not_hold_the_working_slots() {
    let fixture = fixture().await;
    let mut parked = Vec::new();
    for index in 0..MAX_ACTIVE_SUBAGENTS {
        let child = start_child(&fixture, &format!("{DELEGATED_NOTICE}held {index}")).await;
        let mut run = jarvis_storage::find_run(&fixture.database, &child.run_id)
            .await
            .unwrap_or_else(|error| panic!("{error}"));
        for next in [
            jarvis_core::RunState::ContextBuilding,
            jarvis_core::RunState::Planning,
            jarvis_core::RunState::AwaitingApproval,
        ] {
            run = jarvis_storage::transition_run(
                &fixture.database,
                run.id(),
                run.expectation(),
                &jarvis_core::RunTransition::new(next, next.required_outcome(), None)
                    .unwrap_or_else(|error| panic!("{error}")),
                jarvis_core::UtcTimestamp::now(&jarvis_core::SystemClock),
            )
            .await
            .unwrap_or_else(|error| panic!("{error}"));
        }
        parked.push(child.run_id);
    }
    let parent = start(&fixture, "delegate despite four waiting").await;
    let model = parent_model(vec![
        Turn::tool_call(
            "c1",
            DELEGATE_TOOL,
            r#"{"task":"a fifth","background":true}"#,
        ),
        Turn::answer("started"),
    ]);
    drive_parent(&fixture, &model, &parent.run_id).await;
    let results = tool_results(&model);
    assert!(
        results[0].contains(r#""outcome":"started""#),
        "four parked sub-agents blocked a new one: {results:?}"
    );
    // The list still shows the parked ones, as waiting.
    let listing = start(&fixture, "list them").await;
    let model = parent_model(vec![
        Turn::tool_call("c1", LIST_TOOL, "{}"),
        Turn::answer("ok"),
    ]);
    drive_parent(&fixture, &model, &listing.run_id).await;
    let listed = tool_results(&model);
    assert_eq!(
        listed[0].matches("waiting_for_approval").count(),
        MAX_ACTIVE_SUBAGENTS,
        "{listed:?}"
    );
    assert!(parked.iter().all(|id| listed[0].contains(id)));
}

/// **A refused delegation says where to look.**
#[tokio::test]
async fn a_full_house_points_at_the_list() {
    let fixture = fixture().await;
    for index in 0..MAX_ACTIVE_SUBAGENTS {
        start_child(&fixture, &format!("{DELEGATED_NOTICE}task {index}")).await;
    }
    for index in 0..60 {
        start(&fixture, &format!("unrelated newer work {index}")).await;
    }
    let parent = start(&fixture, "one more").await;
    let model = parent_model(vec![
        Turn::tool_call(
            "c1",
            DELEGATE_TOOL,
            r#"{"task":"a fifth","background":true}"#,
        ),
        Turn::answer("blocked"),
    ]);
    drive_parent(&fixture, &model, &parent.run_id).await;
    let results = tool_results(&model);
    assert!(results[0].contains(r#""outcome":"refused""#), "{results:?}");
    assert!(results[0].contains("jarvis.agent.list"), "{results:?}");
    let listing = start(&fixture, "find the older children").await;
    let model = parent_model(vec![
        Turn::tool_call("list", LIST_TOOL, "{}"),
        Turn::answer("listed"),
    ]);
    drive_parent(&fixture, &model, &listing.run_id).await;
    let listed = tool_results(&model);
    let page: serde_json::Value =
        serde_json::from_str(&listed[0]).unwrap_or_else(|error| panic!("list JSON: {error}"));
    assert_eq!(page["running"], 4);
    assert_eq!(page["agents"].as_array().map(Vec::len), Some(4));
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn native_child_audit_and_failure_are_local_and_survive_reopen() {
    let fixture = fixture().await;
    let child = start_child(&fixture, "read the delegated input then fail").await;
    let stored = jarvis_storage::find_run(&fixture.database, &child.run_id)
        .await
        .unwrap();
    let parent = stored.parent_run_id().unwrap().to_owned();
    let other = start(&fixture, "unrelated work").await;
    let model = parent_model(vec![
        Turn::tool_call("read", CONTEXT_TOOL, r#"{"source":"input","offset":0}"#),
        Turn::refusal(),
    ]);
    let failed = drive_parent(&fixture, &model, &child.run_id).await;
    assert_eq!(failed.state(), jarvis_core::RunState::Failed);
    let calls = jarvis_storage::read_run_tool_calls(&fixture.database, &child.run_id)
        .await
        .unwrap();
    assert_eq!(calls.len(), 1);
    let call = &calls[0];
    assert_eq!(call.run_id(), child.run_id);
    assert_eq!(call.workspace_id(), child.workspace_id);
    assert_eq!(call.outcome(), jarvis_core::ToolOutcome::Confirmed);
    assert_eq!(call.receipt(), call.id());
    assert_eq!(
        call.intent_hash(),
        jarvis_core::CanonicalIntentHash::compute(
            CONTEXT_TOOL,
            call.tool_version(),
            &json!({"source":"input","offset":0})
        )
        .unwrap()
        .to_hex()
    );
    assert!(!call.policy_version().is_empty());
    assert!(
        jarvis_storage::read_run_tool_calls(&fixture.database, &parent)
            .await
            .unwrap()
            .is_empty()
    );
    assert!(
        jarvis_storage::read_run_tool_calls(&fixture.database, &other.run_id)
            .await
            .unwrap()
            .is_empty()
    );
    let events = audit_events(&fixture.database, &child.run_id).await;
    assert!(events.iter().all(|event| event.run_id() == child.run_id));
    assert!(
        events
            .iter()
            .any(|event| event.correlation_id() == call.correlation_id())
    );
    for id in [&parent, &other.run_id] {
        assert_eq!(
            jarvis_storage::find_run(&fixture.database, id)
                .await
                .unwrap()
                .state(),
            jarvis_core::RunState::Received
        );
        assert_eq!(
            drive_parent(
                &fixture,
                &parent_model(vec![Turn::answer("still usable")]),
                id
            )
            .await
            .terminal_outcome(),
            Some(RunOutcome::Succeeded)
        );
    }
    fixture.database.close().await;
    let db = SqliteDatabase::open(
        &fixture
            .profile
            .0
            .join(jarvis_storage::DEFAULT_DATABASE_FILENAME),
    )
    .await
    .unwrap();
    assert_eq!(
        jarvis_storage::find_run(&db, &child.run_id).await.unwrap(),
        failed
    );
    assert_eq!(
        jarvis_storage::read_run_tool_calls(&db, &child.run_id)
            .await
            .unwrap(),
        calls
    );
    assert_eq!(audit_events(&db, &child.run_id).await, events);
    db.close().await;
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn shutdown_waits_for_a_native_child_and_preserves_a_real_approval_wait() {
    let fixture = fixture().await;
    let child = start_child(&fixture, "finish in-flight inference").await;
    let parent = jarvis_storage::find_run(&fixture.database, &child.run_id)
        .await
        .unwrap()
        .parent_run_id()
        .unwrap()
        .to_owned();
    // Failure does not implicitly stop children; explicit Stop and shutdown do.
    assert_eq!(
        drive_parent(&fixture, &parent_model(vec![Turn::refusal()]), &parent)
            .await
            .state(),
        jarvis_core::RunState::Failed
    );
    assert!(
        jarvis_storage::find_run(&fixture.database, &child.run_id)
            .await
            .unwrap()
            .cancellation_requested_at()
            .is_none()
    );
    let parked = start(&fixture, "approval survives shutdown").await;
    let parked_model = parent_model(vec![Turn::tool_call(
        "held",
        crate::code_delegate_async::START,
        r#"{"task":"held"}"#,
    )]);
    assert_eq!(
        drive_parent(&fixture, &parked_model, &parked.run_id)
            .await
            .state(),
        jarvis_core::RunState::AwaitingApproval
    );
    let approvals = jarvis_storage::read_run_approvals(&fixture.database, &parked.run_id)
        .await
        .unwrap();
    assert_eq!(approvals.len(), 1);
    let model = parent_model(vec![Turn::Slow {
        delay_ms: 200,
        then: Box::new(Turn::answer("finished inference")),
    }]);
    let drain = async {
        while model.seen_messages().is_empty() {
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
        crate::shutdown::drain_work(&fixture.database, None, std::time::Duration::from_secs(3))
            .await
            .unwrap();
    };
    let (finished, ()) = tokio::join!(drive_parent(&fixture, &model, &child.run_id), drain);
    assert_eq!(finished.state(), jarvis_core::RunState::Cancelled);
    assert_eq!(
        jarvis_storage::read_run_approvals(&fixture.database, &parked.run_id)
            .await
            .unwrap(),
        approvals
    );
    assert_eq!(
        jarvis_storage::find_run(&fixture.database, &parked.run_id)
            .await
            .unwrap()
            .state(),
        jarvis_core::RunState::AwaitingApproval
    );
    fixture.database.close().await;
}

#[allow(clippy::unwrap_used)]
async fn audit_events(db: &SqliteDatabase, id: &str) -> Vec<jarvis_storage::StoredRunEvent> {
    jarvis_storage::read_run_events(
        db,
        id,
        jarvis_core::ReplayRequest::new(jarvis_core::RunEventSequence::new(1).unwrap(), 100)
            .unwrap(),
    )
    .await
    .unwrap()
}
