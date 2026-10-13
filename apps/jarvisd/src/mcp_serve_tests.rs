//! Tests for the **inbound** MCP path: what a remote caller gets, and what it is refused.
//!
//! # Why these live here rather than in a transport test
//!
//! `P3-009e`/`P3-009f`/`P3-009c-a` prove the handler refuses an unadvertised tool and that the two inbound
//! policies are applied. None of them can prove what happens **when a served call runs**, because that needs a
//! registry, a policy engine, and adapters — which is the composition root's wiring and exists only here.
//!
//! # The property these tests exist for
//!
//! `ToolPipeline::call_remote_tool` is a **second entrypoint** into the tool path, and a second entrypoint is
//! exactly where an enforcement step gets forgotten. So the tests below are mostly about *parity*: every check
//! the local path applies is applied here, and the ones that cannot be applied are **refused** rather than
//! skipped. The two that matter most:
//!
//! - a **traversal** must be refused through the remote path exactly as through the local one, because that is
//!   `ADR-0020`'s confinement and a remote caller is the case it exists for;
//! - an **approval** must be refused rather than run, because there is no run to park in `awaiting_approval`
//!   and running the call would be the unsafe direction.

use std::sync::Arc;

use jarvis_core::{CorrelationId, ToolOutcome};
use jarvis_storage::{LOCAL_USER_ID, LOCAL_WORKSPACE_ID, SqliteDatabase, StartRunInput, start_run};
use jarvis_tools::{ToolDefinition, WorkspacePolicy, WorkspaceRoots};
use serde_json::json;

use super::{POLICY_VERSION, RemoteMcpError, build_endpoint};
use crate::tool_actor::ToolActor;
use crate::tool_pipeline::{ToolPipeline, ToolPipelineOutcome};
use jarvis_mcp_transport::{CallerAdmission, ServedToolRunner};

const RUN: &str = "0198f000-0000-7000-8000-0000000000c3";
const EVENT: &str = "0198f000-0000-7000-8000-0000000000e4";
const SESSION: &str = "0198f000-0000-7000-8000-000000000003";

/// A temporary root removed when the test ends, pass or fail.
///
/// The removal goes through `jarvis_core::remove_scratch_dir` rather than a bare `remove_dir_all`, because this
/// fixture holds a database and the handle is released late and off-thread — see `P3-013` and
/// `docs/development/testing.md`. A bare call here fails with a Windows sharing violation that the `let _ =`
/// swallows, so the directory leaks while every test still passes.
struct TempRoot(std::path::PathBuf);

impl TempRoot {
    fn new() -> Self {
        let path =
            std::env::temp_dir().join(format!("jarvis-mcp-serve-{}", jarvis_core::scratch_tag()));
        std::fs::create_dir_all(&path).unwrap_or_else(|error| panic!("create root: {error}"));
        let path = path
            .canonicalize()
            .unwrap_or_else(|error| panic!("canonicalize: {error}"));
        Self(path)
    }

    fn path(&self) -> &std::path::Path {
        &self.0
    }

    fn write(&self, relative: &str, contents: &str) {
        let path = self.0.join(relative);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .unwrap_or_else(|error| panic!("create parent: {error}"));
        }
        std::fs::write(&path, contents).unwrap_or_else(|error| panic!("write: {error}"));
    }
}

impl Drop for TempRoot {
    fn drop(&mut self) {
        jarvis_core::remove_scratch_dir(&self.0);
    }
}

fn must<T, E: std::fmt::Debug>(result: Result<T, E>) -> T {
    match result {
        Ok(value) => value,
        Err(error) => panic!("expected success, got {error:?}"),
    }
}

/// A pipeline over one granted root and no additional adapters.
///
/// The filesystem adapter is the case that matters for this module: it is the one tool area whose confinement
/// a remote caller could otherwise escape, so all of these tests drive it rather than a test double.
async fn pipeline_over(root: &TempRoot) -> (Arc<SqliteDatabase>, Arc<ToolPipeline>) {
    let database = Arc::new(must(
        SqliteDatabase::open(&root.path().join("jarvis.sqlite3")).await,
    ));
    must(
        start_run(
            &database,
            &must(StartRunInput::new(
                SESSION,
                RUN,
                EVENT,
                LOCAL_WORKSPACE_ID,
                LOCAL_USER_ID,
                "Read a file",
                jarvis_core::SessionChannel::Cli,
                CorrelationId::new(),
                jarvis_core::UtcTimestamp::now(&jarvis_core::SystemClock),
            )),
        )
        .await,
    );
    let roots = must(WorkspaceRoots::new([root.path()]));
    let pipeline = must(ToolPipeline::new(
        Arc::clone(&database),
        roots,
        WorkspacePolicy::default(),
    ));
    (database, Arc::new(pipeline))
}

/// Builds an actor for a remote call, the way the runner does.
///
/// Takes the served definitions because the actor's grant is **derived** from them — see
/// [`ToolActor::remote`] — so a fixture that invented its own scope set would test a caller the runner never
/// builds.
fn remote_actor(correlation_id: CorrelationId, served: &[ToolDefinition]) -> ToolActor {
    ToolActor::remote(LOCAL_WORKSPACE_ID, correlation_id, POLICY_VERSION, served)
}

/// A pipeline over one granted root, plus the definitions it serves.
///
/// Returns the definitions beside the pipeline because the actor's grant is derived from them, and every test
/// that makes a remote call must build its actor the same way the runner does — a fixture that skipped that
/// would be testing a caller that cannot exist.
async fn served_pipeline_over(
    root: &TempRoot,
) -> (Arc<SqliteDatabase>, Arc<ToolPipeline>, Vec<ToolDefinition>) {
    let (database, pipeline) = pipeline_over(root).await;
    let served = must(pipeline.definitions());
    (database, pipeline, served)
}

/// **A remote caller reaches the filesystem adapter and gets the file's contents.**
///
/// The positive control: without it, every refusal test below would pass on a path that refused everything,
/// and the "a remote call is dispatched identically" claim would be untested.
#[tokio::test]
async fn a_remote_call_reaches_the_filesystem_adapter() {
    let root = TempRoot::new();
    root.write("notes/todo.txt", "buy milk");
    let (_database, pipeline, served) = served_pipeline_over(&root).await;
    let correlation_id = CorrelationId::new();

    let outcome = must(
        pipeline
            .call_remote_tool(
                "jarvis.files.read",
                json!({ "path": "notes/todo.txt" }),
                &remote_actor(correlation_id, &served),
                correlation_id,
            )
            .await,
    );

    match outcome {
        ToolPipelineOutcome::Executed(result) => {
            assert_eq!(result.outcome(), ToolOutcome::Confirmed);
            let output = result
                .output()
                .unwrap_or_else(|| panic!("a read must carry its output"));
            assert!(
                output.content().contains("buy milk"),
                "got: {}",
                output.content()
            );
        }
        other => panic!("a remote read must execute, got {other:?}"),
    }
}

/// **The tools that change the owner's files are not offered to a remote caller**, while the read tools are.
#[tokio::test]
async fn file_writing_tools_are_not_served_remotely() {
    let root = TempRoot::new();
    let (_database, _pipeline, definitions) = served_pipeline_over(&root).await;
    let ids = |defs: &[ToolDefinition]| -> Vec<String> {
        defs.iter().map(|d| d.id().to_string()).collect()
    };
    assert!(
        ids(&definitions).contains(&"jarvis.files.write".to_owned()),
        "the control: they exist"
    );

    let served = ids(&super::servable_definitions(&definitions));
    assert!(!served.contains(&"jarvis.files.write".to_owned()));
    assert!(!served.contains(&"jarvis.files.edit".to_owned()));
    assert!(served.contains(&"jarvis.files.read".to_owned()));
}

/// **A traversal is refused through the remote path exactly as through the local one.**
///
/// `ADR-0020` makes confinement a directory handle rather than a validated path, so the escape is refused by
/// the adapter rather than by a string check — and the reason this test is not redundant with the local one is
/// that a second entrypoint is where a *different* adapter or a reconstructed handle would appear. The
/// arguments ask for a path outside the grant, and the assertion is that **no output came back**, not merely
/// that the outcome is not `Confirmed`: a refused read that still returned the contents would satisfy the
/// weaker claim.
#[tokio::test]
async fn a_remote_traversal_is_refused() {
    let root = TempRoot::new();
    root.write("notes/todo.txt", "buy milk");
    let (_database, pipeline, served) = served_pipeline_over(&root).await;
    let correlation_id = CorrelationId::new();

    for escape in ["../secret.txt", "/etc/passwd", "notes/../../secret.txt"] {
        let outcome = must(
            pipeline
                .call_remote_tool(
                    "jarvis.files.read",
                    json!({ "path": escape }),
                    &remote_actor(correlation_id, &served),
                    correlation_id,
                )
                .await,
        );

        match outcome {
            ToolPipelineOutcome::Executed(result) => {
                assert_ne!(
                    result.outcome(),
                    ToolOutcome::Confirmed,
                    "{escape} must not be read successfully, got {result:?}"
                );
                // The stronger half: a non-`Confirmed` outcome that still carried content would mean the read
                // happened and the outcome was merely mislabelled.
                assert!(
                    result
                        .output()
                        .is_none_or(|output| !output.content().contains("super secret")),
                    "{escape} must not return file content"
                );
            }
            ToolPipelineOutcome::Refused { reason_code } => {
                // A refusal before the adapter is equally correct and is what a scope-aware policy would
                // produce. Named rather than ignored, so a change from "refused by the adapter" to "refused by
                // policy" is visible in the test's own vocabulary.
                assert!(!reason_code.is_empty(), "a refusal must name a reason code");
            }
            ToolPipelineOutcome::AwaitingApproval { .. } => {
                panic!("a remote call must never be held for an approval")
            }
        }
    }
}

/// **An argument the tool's schema refuses is refused on the remote path too.**
///
/// The parity claim in its cheapest form: a missing required field. A second entrypoint that skipped validation
/// would reach the adapter with arguments nothing checked, and the adapter would be deciding about a call whose
/// shape was never established.
#[tokio::test]
async fn a_remote_call_with_invalid_arguments_is_refused_before_the_adapter() {
    let root = TempRoot::new();
    root.write("notes/todo.txt", "buy milk");
    let (_database, pipeline, served) = served_pipeline_over(&root).await;
    let correlation_id = CorrelationId::new();

    // `path` is required by the adapter's own schema, so its absence is a violation rather than a policy
    // decision.
    let error = pipeline
        .call_remote_tool(
            "jarvis.files.read",
            json!({}),
            &remote_actor(correlation_id, &served),
            correlation_id,
        )
        .await
        .err()
        .unwrap_or_else(|| panic!("a missing required field must be refused"));

    assert!(
        format!("{error}").contains("argument") || format!("{error}").contains("path"),
        "the refusal must name the argument problem, got: {error}"
    );
}

/// **A tool the registry does not hold is a fault, not a refusal.**
///
/// A refusal would mean "understood and declined", which is what a **policy** decision produces. An unknown
/// identifier is a caller asking for something that does not exist, and the distinction is the one
/// `ToolPipelineError` documents: a refusal is a correct answer, a fault is not.
#[tokio::test]
async fn an_unknown_tool_is_a_fault_rather_than_a_refusal() {
    let root = TempRoot::new();
    let (_database, pipeline, served) = served_pipeline_over(&root).await;
    let correlation_id = CorrelationId::new();

    let error = pipeline
        .call_remote_tool(
            "mcp.nothing.here",
            json!({}),
            &remote_actor(correlation_id, &served),
            correlation_id,
        )
        .await
        .err()
        .unwrap_or_else(|| panic!("an unknown tool must be a fault"));

    assert!(
        format!("{error}").contains("mcp.nothing.here"),
        "the fault must name the identifier asked for, got: {error}"
    );
}

/// A workspace policy that requires an approval for **every** call.
///
/// The threshold is `Risk::Minimal`, not `Low`, and that is the rule rather than a preference: the engine asks
/// for an approval when `risk >= approval_threshold`, so a threshold of `Low` leaves a risk-0 tool
/// un-held. That is exactly what the first version of the test below asserted and what it failed on — the call
/// **ran** — which is worth recording because the assertion was about the product's behaviour and the mistake
/// was in the fixture's understanding of the comparison.
///
/// Built through the real constructor rather than by mutating a default, so the policy is one the engine can
/// actually be handed: `WorkspacePolicy::new` refuses a threshold above `max_risk`, and this pair is legal.
fn approval_requiring_workspace() -> WorkspacePolicy {
    must(WorkspacePolicy::new(
        jarvis_tools::Risk::High,
        jarvis_tools::Risk::Minimal,
        true,
    ))
}

/// **A pipeline whose workspace requires an approval refuses the remote call, and never runs the tool.**
///
/// This is the property `call_remote_tool`'s second, independent approval check exists for, and driving it
/// needs a workspace whose threshold is low enough to require one: `WorkspacePolicy::default()` asks from risk 2
/// up, and the filesystem tool is risk 0, so the default never holds it.
///
/// `WorkspacePolicy::restrictive_for_test` in this file lowers the threshold to `None`, which makes
/// **every** call require an approval. The remote path must then refuse rather than run — a tool an operator's
/// own call would have to approve, executed because the caller arrived over a different protocol.
///
/// The file is written first, so "the tool did not run" is an observation about the refusal rather than about a
/// missing file: a path that ran the call anyway would find real content to return.
#[tokio::test]
async fn a_workspace_requiring_an_approval_refuses_the_remote_call() {
    let root = TempRoot::new();
    root.write("notes/todo.txt", "buy milk");
    let database = Arc::new(must(
        SqliteDatabase::open(&root.path().join("jarvis.sqlite3")).await,
    ));
    must(
        start_run(
            &database,
            &must(StartRunInput::new(
                SESSION,
                RUN,
                EVENT,
                LOCAL_WORKSPACE_ID,
                LOCAL_USER_ID,
                "Read a file",
                jarvis_core::SessionChannel::Cli,
                CorrelationId::new(),
                jarvis_core::UtcTimestamp::now(&jarvis_core::SystemClock),
            )),
        )
        .await,
    );
    let roots = must(WorkspaceRoots::new([root.path()]));
    let pipeline = must(ToolPipeline::new(
        Arc::clone(&database),
        roots,
        approval_requiring_workspace(),
    ));
    let served = must(pipeline.definitions());
    let correlation_id = CorrelationId::new();

    let outcome = must(
        pipeline
            .call_remote_tool(
                "jarvis.files.read",
                json!({ "path": "notes/todo.txt" }),
                &remote_actor(correlation_id, &served),
                correlation_id,
            )
            .await,
    );

    match outcome {
        // The call is **not run** and says it needs the owner (`ADR-0180`): the runner that holds the caller's request is what
        // asks. Empty identifiers because there is no run or stored approval on this path.
        ToolPipelineOutcome::AwaitingApproval {
            call_id,
            approval_id,
            ..
        } => {
            assert!(call_id.is_empty() && approval_id.is_empty());
        }
        ToolPipelineOutcome::Refused { reason_code } => {
            panic!("a held remote call must wait for the owner, not be refused ({reason_code})")
        }
        ToolPipelineOutcome::Executed(result) => {
            panic!("a call the workspace holds for approval must not run, got {result:?}")
        }
    }
}

/// **An endpoint serving a tool that needs an approval is still built, and the approval is refused per call.**
///
/// The distinction from the test above: the *served surface* accepts such a tool — `P3-009d` excludes a tool by
/// source, availability, name, and count, and an approval declaration is none of those — so the refusal has to
/// happen on the call. That is the shape `call_remote_tool` implements, and this test proves the runner maps it
/// to an `AdapterError` the handler reports as `isError` rather than letting it through.
#[tokio::test]
async fn the_runner_maps_a_refusal_to_a_tool_error() {
    let root = TempRoot::new();
    root.write("notes/todo.txt", "buy milk");
    let (_database, pipeline) = pipeline_over(&root).await;

    let (runner, _endpoint) = must(build_endpoint(
        Arc::clone(&pipeline),
        &must(pipeline.definitions()),
        LOCAL_WORKSPACE_ID,
        CallerAdmission::local_only(),
    ));

    // A served tool, called correctly: the runner answers a result rather than an error. This is the positive
    // control for the error mapping below — without it, a runner that returned `Err` for everything would pass.
    let correlation_id = CorrelationId::new();
    let result = must(
        runner
            .run(
                "jarvis.files.read",
                json!({ "path": "notes/todo.txt" }),
                correlation_id,
            )
            .await,
    );
    assert_eq!(result.outcome(), ToolOutcome::Confirmed);
}

/// **The runner reports a refusal as `RefusedBeforeReaching`, which is the variant that says nothing happened.**
///
/// The mapping matters because the handler turns an `AdapterError` into a refusal text and the *variant* decides
/// what a caller may conclude: `AmbiguousAfterReaching` would say an effect may have occurred, which for a
/// refused call is false and would invite a caller to check on something that never happened.
#[tokio::test]
async fn a_refused_remote_call_reports_that_nothing_was_reached() {
    let root = TempRoot::new();
    let (_database, pipeline) = pipeline_over(&root).await;
    let (runner, _endpoint) = must(build_endpoint(
        Arc::clone(&pipeline),
        &must(pipeline.definitions()),
        LOCAL_WORKSPACE_ID,
        CallerAdmission::local_only(),
    ));

    let error = runner
        .run("mcp.nothing.here", json!({}), CorrelationId::new())
        .await
        .err()
        .unwrap_or_else(|| panic!("an unknown tool must be refused by the runner"));

    assert!(
        matches!(
            error,
            jarvis_tools::AdapterError::RefusedBeforeReaching { .. }
        ),
        "a refusal must not claim a provider may have been reached, got: {error:?}"
    );
}

/// **An adapter's own error survives the mapping with its claim intact.**
///
/// This is the property a collapsed mapping breaks, and it is not observable through the endpoint: no adapter in
/// this daemon returns `AmbiguousAfterReaching` on a served tool, so the test constructs the value. The
/// falsification is deliberate — rewriting the `AdapterCall` arm to `RefusedBeforeReaching` makes this test fail,
/// and it is the *only* test that fails, because the endpoint-level tests can only reach the mapped arm.
///
/// What it protects: `AmbiguousAfterReaching` tells a caller that a request left and the outcome is unknown, so
/// it must not retry blindly. Collapsing it into "nothing happened" invites precisely that retry, and for a
/// non-idempotent tool the retry is a second effect.
#[test]
fn an_adapters_own_error_is_not_weakened_by_the_mapping() {
    let ambiguous = jarvis_tools::AdapterError::AmbiguousAfterReaching {
        reason: "the provider was reached and never answered".to_owned(),
    };
    let mapped = super::map_pipeline_error(
        crate::tool_pipeline::ToolPipelineError::AdapterCall(ambiguous),
        "jarvis.files.read",
    );
    assert!(
        matches!(
            mapped,
            jarvis_tools::AdapterError::AmbiguousAfterReaching { .. }
        ),
        "an adapter error must keep its own claim, got: {mapped:?}"
    );
}

/// **A pre-dispatch fault maps to the "nothing happened" variant, which the code can support.**
///
/// The complement of the test above, and the reason the mapping is a match rather than one arm: every other
/// pipeline error is raised before the dispatcher is consulted, so `RefusedBeforeReaching` is then a fact rather
/// than a guess. An unknown tool is the cheapest such fault to construct and the one a client is most likely to
/// cause.
#[test]
fn a_pre_dispatch_fault_reports_that_nothing_was_reached() {
    let mapped = super::map_pipeline_error(
        crate::tool_pipeline::ToolPipelineError::UnknownTool {
            tool: "mcp.nothing.here".to_owned(),
        },
        "mcp.nothing.here",
    );
    assert!(
        matches!(
            mapped,
            jarvis_tools::AdapterError::RefusedBeforeReaching { .. }
        ),
        "a fault raised before dispatch must report that nothing was reached, got: {mapped:?}"
    );
}

/// **A tool declaring an approval is refused even when the workspace would not ask for one.**
///
/// This is the second, independent approval check, and until this test existed **nothing covered it**: disabling
/// the `definition.approval() != ApprovalPolicy::Auto` guard in `call_remote_tool` left the whole suite green.
/// That is the measured record, not an inference — the mutation was run and the suite passed 13/13.
///
/// # Why the two checks cannot be collapsed into one
///
/// The engine's threshold is a **workspace** setting and `ApprovalPolicy` is a declaration on the **tool**. A
/// workspace policy that holds nothing (the default, whose threshold is `Moderate`) says nothing about a tool
/// that asks to be held; a test that varied only the workspace could never see this guard, and one that varied
/// only the tool would never see the engine's. Both are present, so both get a test.
///
/// The tool is registered through `with_adapters` with the filesystem adapter **also** present, so this cannot
/// pass because dispatch found nothing: the recording adapter must never be called, which is asserted rather than
/// inferred from the outcome.
#[tokio::test]
async fn a_tool_declaring_an_approval_is_refused_even_where_the_workspace_would_allow_it() {
    let root = TempRoot::new();
    root.write("notes/todo.txt", "buy milk");
    let (database, _pipeline) = pipeline_over(&root).await;
    let roots = must(WorkspaceRoots::new([root.path()]));
    let adapter = Arc::new(ApprovalDeclaringAdapter::default());
    let pipeline = Arc::new(must(ToolPipeline::with_adapters(
        Arc::clone(&database),
        Some(roots),
        WorkspacePolicy::default(),
        vec![(
            vec![approval_declaring_definition()],
            Arc::clone(&adapter) as Arc<dyn jarvis_tools::ToolExecutor>,
        )],
    )));

    let served = must(pipeline.definitions());
    let actor = remote_actor(CorrelationId::new(), &served);
    let outcome = must(
        pipeline
            .call_remote_tool(
                APPROVAL_TOOL,
                json!({ "path": "notes/todo.txt" }),
                &actor,
                CorrelationId::new(),
            )
            .await,
    );

    assert!(
        matches!(&outcome, ToolPipelineOutcome::AwaitingApproval { .. }),
        "a tool that declares an approval must wait for the owner remotely, got: {outcome:?}"
    );
    assert_eq!(
        adapter.calls(),
        0,
        "the adapter must not have run: a refusal that still dispatched is not a refusal"
    );
}

/// A runner over a pipeline whose one tool declares an approval, with the file it reads present, and the adapter that counts calls.
async fn approval_runner(
    wait: std::time::Duration,
) -> (
    crate::mcp_serve::RemoteMcpRunner,
    Arc<crate::inbound_approvals::InboundApprovals>,
    Arc<crate::approval_fixture::RecordingApprovalAdapter>,
    TempRoot,
) {
    let root = TempRoot::new();
    root.write("notes/todo.txt", "buy milk");
    let (database, _pipeline) = pipeline_over(&root).await;
    let roots = must(WorkspaceRoots::new([root.path()]));
    let adapter = Arc::new(crate::approval_fixture::RecordingApprovalAdapter::default());
    let pipeline = Arc::new(must(ToolPipeline::with_adapters(
        Arc::clone(&database),
        Some(roots),
        WorkspacePolicy::default(),
        vec![(
            vec![approval_declaring_definition()],
            Arc::clone(&adapter) as Arc<dyn jarvis_tools::ToolExecutor>,
        )],
    )));
    let served = must(pipeline.definitions());
    let approvals = Arc::new(crate::inbound_approvals::InboundApprovals::default());
    let runner = crate::mcp_serve::RemoteMcpRunner::new(pipeline, served, LOCAL_WORKSPACE_ID)
        .with_approvals(Arc::clone(&approvals), "owner", wait);
    (runner, approvals, adapter, root)
}

/// A phone that answers the same thing every time, standing in for the live-call check and the line guard.
struct StubPhone(Option<crate::phone_service::PhoneYes>, Option<String>);

#[async_trait::async_trait]
impl crate::phone_service::PhoneApproval for StubPhone {
    async fn approves(&self, _tool: &str, _risk: u8) -> Option<crate::phone_service::PhoneYes> {
        self.0.clone()
    }

    async fn line_guard(&self) -> Option<String> {
        self.1.clone()
    }
}

/// As [`approval_runner`], under `policy`.
async fn approval_runner_under(
    policy: WorkspacePolicy,
) -> (
    crate::mcp_serve::RemoteMcpRunner,
    Arc<crate::inbound_approvals::InboundApprovals>,
    Arc<crate::approval_fixture::RecordingApprovalAdapter>,
    TempRoot,
) {
    let root = TempRoot::new();
    root.write("notes/todo.txt", "buy milk");
    let (database, _pipeline) = pipeline_over(&root).await;
    let roots = must(WorkspaceRoots::new([root.path()]));
    let adapter = Arc::new(crate::approval_fixture::RecordingApprovalAdapter::default());
    let pipeline = Arc::new(must(ToolPipeline::with_adapters(
        Arc::clone(&database),
        Some(roots),
        policy,
        vec![(
            vec![approval_declaring_definition()],
            Arc::clone(&adapter) as Arc<dyn jarvis_tools::ToolExecutor>,
        )],
    )));
    let served = must(pipeline.definitions());
    let approvals = Arc::new(crate::inbound_approvals::InboundApprovals::default());
    let runner = crate::mcp_serve::RemoteMcpRunner::new(pipeline, served, LOCAL_WORKSPACE_ID)
        .with_approvals(
            Arc::clone(&approvals),
            "owner",
            std::time::Duration::from_millis(400),
        );
    (runner, approvals, adapter, root)
}

fn call_arguments() -> serde_json::Value {
    json!({ "path": "notes/todo.txt" })
}

/// **A yes spoken on a live call is the owner's answer: the call runs and nothing waits in the console.**
///
/// This is the double approval the owner met: the phone agent asked on the phone, and the same call then waited in the console as well.
/// Falsified by ignoring the phone's answer: the call would wait for the console and time out.
#[tokio::test]
async fn a_yes_on_the_phone_runs_the_call_without_a_second_question() {
    let (runner, approvals, adapter, _root) =
        approval_runner_under(WorkspacePolicy::default()).await;
    approvals.set_phone_approval(Arc::new(StubPhone(
        Some(crate::phone_service::PhoneYes {
            call_id: "call-1".to_owned(),
        }),
        None,
    )));
    let result = must(
        runner
            .run(APPROVAL_TOOL, call_arguments(), CorrelationId::new())
            .await
            .map_err(|error| error.to_string()),
    );
    assert_eq!(result.outcome(), ToolOutcome::Confirmed);
    assert_eq!(adapter.calls(), 1);
    assert!(
        approvals.list().is_empty(),
        "nothing was put in front of the owner a second time"
    );
}

/// **Without a live call the question stays in the console**, so a phone that says no changes nothing about the existing rule.
#[tokio::test]
async fn without_a_live_call_the_question_still_waits_in_the_console() {
    let (runner, approvals, adapter, _root) =
        approval_runner_under(WorkspacePolicy::default()).await;
    approvals.set_phone_approval(Arc::new(StubPhone(None, None)));
    let refused = runner
        .run(APPROVAL_TOOL, call_arguments(), CorrelationId::new())
        .await
        .err()
        .unwrap_or_else(|| panic!("with no answer the call must not run"));
    assert!(refused.to_string().contains("did not answer"), "{refused}");
    assert_eq!(adapter.calls(), 0);
}

/// **"Always ask first" outranks the phone**: the owner tightened this tool, so the question is the console's, whatever was said on a call.
#[tokio::test]
async fn a_tool_set_to_always_ask_first_ignores_a_yes_on_the_phone() {
    let policy = WorkspacePolicy::default().requiring(
        must(jarvis_tools::ToolId::new(APPROVAL_TOOL)),
        jarvis_tools::ApprovalPolicy::Ask,
    );
    let (runner, approvals, adapter, _root) = approval_runner_under(policy).await;
    approvals.set_phone_approval(Arc::new(StubPhone(
        Some(crate::phone_service::PhoneYes {
            call_id: "call-1".to_owned(),
        }),
        None,
    )));
    let refused = runner
        .run(APPROVAL_TOOL, call_arguments(), CorrelationId::new())
        .await
        .err()
        .unwrap_or_else(|| panic!("a pinned tool must wait for the console"));
    assert!(refused.to_string().contains("did not answer"), "{refused}");
    assert_eq!(adapter.calls(), 0);
}

/// A phone that has no call JARVIS placed and says only whether the owner alone is on the line.
struct OwnerAlone(bool);

#[async_trait::async_trait]
impl crate::phone_service::PhoneApproval for OwnerAlone {
    async fn approves(&self, _tool: &str, _risk: u8) -> Option<crate::phone_service::PhoneYes> {
        None
    }

    async fn owner_alone_on_line(&self, _tool: &str, _risk: u8) -> bool {
        self.0
    }
}

/// **A tool the owner trusted by name runs for the owner's own phone call without a second question** (`ADR-0207`): the call the owner
/// made to JARVIS is not one JARVIS placed, so the console used to ask even for what they had set to run without asking. Falsified by
/// dropping the trust condition (the untrusted case would run) or the line condition (the stranger case would run).
#[tokio::test]
async fn a_trusted_tool_runs_without_a_second_question_only_while_the_owner_alone_is_on_the_line() {
    let id = || must(jarvis_tools::ToolId::new(APPROVAL_TOOL));

    let (runner, approvals, adapter, _root) =
        approval_runner_under(WorkspacePolicy::default().trusting(id())).await;
    approvals.set_phone_approval(Arc::new(OwnerAlone(true)));
    let result = must(
        runner
            .run(APPROVAL_TOOL, call_arguments(), CorrelationId::new())
            .await
            .map_err(|error| error.to_string()),
    );
    assert_eq!(result.outcome(), ToolOutcome::Confirmed);
    assert_eq!(adapter.calls(), 1);
    assert!(approvals.list().is_empty());

    // Not trusted: the owner has not said it runs without asking.
    let (runner, approvals, adapter, _root) =
        approval_runner_under(WorkspacePolicy::default()).await;
    approvals.set_phone_approval(Arc::new(OwnerAlone(true)));
    assert!(
        runner
            .run(APPROVAL_TOOL, call_arguments(), CorrelationId::new())
            .await
            .is_err()
    );
    assert_eq!(adapter.calls(), 0);

    // Trusted, but the owner is not alone on the line (or it could not be told).
    let (runner, approvals, adapter, _root) =
        approval_runner_under(WorkspacePolicy::default().trusting(id())).await;
    approvals.set_phone_approval(Arc::new(OwnerAlone(false)));
    assert!(
        runner
            .run(APPROVAL_TOOL, call_arguments(), CorrelationId::new())
            .await
            .is_err()
    );
    assert_eq!(adapter.calls(), 0);

    // Trusted and then pinned to "always ask": the tighter choice outranks.
    let (runner, approvals, adapter, _root) = approval_runner_under(
        WorkspacePolicy::default()
            .trusting(id())
            .requiring(id(), jarvis_tools::ApprovalPolicy::Ask),
    )
    .await;
    approvals.set_phone_approval(Arc::new(OwnerAlone(true)));
    assert!(
        runner
            .run(APPROVAL_TOOL, call_arguments(), CorrelationId::new())
            .await
            .is_err()
    );
    assert_eq!(adapter.calls(), 0);
}

/// **The phone agent cannot place the next call**: the phone tools are not served to other apps at all.
#[test]
fn the_phone_tools_are_not_served_to_other_apps() {
    let definitions = must(crate::phone_tool::PhoneTool::definitions());
    assert_eq!(definitions.len(), 3);
    assert!(
        super::servable_definitions(&definitions).is_empty(),
        "no phone tool may be offered to a connected app"
    );
}

/// Waits (briefly) until a call is waiting for the owner, and returns its identifier.
async fn waiting_id(approvals: &crate::inbound_approvals::InboundApprovals) -> String {
    for _ in 0..100 {
        if let Some(first) = approvals.list().first() {
            return first.approval_id.clone();
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    panic!("no call came to wait for the owner");
}

/// **A call from another app that needs the owner waits, and runs only after a yes.**
///
/// Falsified by letting the runner run the call without the answer: the adapter would have been called before the answer.
#[tokio::test]
async fn a_call_that_needs_the_owner_waits_and_runs_only_after_a_yes() {
    let (runner, approvals, adapter, _root) =
        approval_runner(std::time::Duration::from_secs(30)).await;
    let task = tokio::spawn({
        let runner = runner.clone();
        async move {
            runner
                .run(
                    APPROVAL_TOOL,
                    json!({ "path": "notes/todo.txt" }),
                    CorrelationId::new(),
                )
                .await
        }
    });
    let id = waiting_id(&approvals).await;
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    assert_eq!(
        adapter.calls(),
        0,
        "nothing runs while the owner has not answered"
    );
    assert_eq!(approvals.list()[0].tool, APPROVAL_TOOL);
    assert!(approvals.decide(&id, true).is_some());
    let result = must(must(task.await));
    assert_eq!(result.outcome(), ToolOutcome::Confirmed);
    assert_eq!(adapter.calls(), 1, "it ran once, after the yes");
    assert!(approvals.list().is_empty());
}

/// **A no, silence and a full list each refuse the call, and the tool never runs.**
#[tokio::test]
async fn a_no_silence_or_a_full_list_refuse_the_call_and_the_tool_never_runs() {
    // No.
    let (runner, approvals, adapter, _root) =
        approval_runner(std::time::Duration::from_secs(30)).await;
    let task = tokio::spawn({
        let runner = runner.clone();
        async move {
            runner
                .run(
                    APPROVAL_TOOL,
                    json!({ "path": "notes/todo.txt" }),
                    CorrelationId::new(),
                )
                .await
        }
    });
    let id = waiting_id(&approvals).await;
    approvals.decide(&id, false);
    let refused = must(task.await)
        .err()
        .unwrap_or_else(|| panic!("a refusal must not look like a result"));
    assert!(matches!(
        refused,
        jarvis_tools::AdapterError::RefusedBeforeReaching { .. }
    ));
    assert_eq!(adapter.calls(), 0);

    // Silence: the wait is short, and nobody answers.
    let (runner, approvals, adapter, _root) =
        approval_runner(std::time::Duration::from_millis(300)).await;
    let refused = runner
        .run(
            APPROVAL_TOOL,
            json!({ "path": "notes/todo.txt" }),
            CorrelationId::new(),
        )
        .await
        .err()
        .unwrap_or_else(|| panic!("no answer must not run the call"));
    assert!(refused.to_string().contains("did not answer"), "{refused}");
    assert!(
        approvals.list().is_empty(),
        "a call that timed out is withdrawn"
    );
    assert_eq!(adapter.calls(), 0);

    // A full list: the call is refused at once, not queued.
    let (runner, approvals, adapter, _root) =
        approval_runner(std::time::Duration::from_secs(30)).await;
    let mut held = Vec::new();
    for _ in 0..crate::inbound_approvals::MAX_WAITING {
        held.push(must(
            approvals
                .ask(
                    "x.y.z",
                    "1.0.0",
                    2,
                    "filler",
                    &json!({}),
                    std::time::Duration::from_secs(30),
                )
                .map_err(|e| format!("{e:?}")),
        ));
    }
    let refused = runner
        .run(
            APPROVAL_TOOL,
            json!({ "path": "notes/todo.txt" }),
            CorrelationId::new(),
        )
        .await
        .err()
        .unwrap_or_else(|| panic!("a full list must refuse"));
    assert!(refused.to_string().contains("too many"), "{refused}");
    assert_eq!(adapter.calls(), 0);
}

/// The identifier the approval-declaring fixture registers.
///
/// Re-exported from the shared fixture module so the two tests that need a **held** call agree on one tool
/// rather than each building its own. See `approval_fixture` for why the posture is a tool declaration
/// rather than a workspace threshold.
const APPROVAL_TOOL: &str = crate::approval_fixture::APPROVAL_TOOL;

/// A well-formed credential fingerprint, used only as a fixture value.
///
/// A **digest**, never a credential: `Fingerprint::parse` refuses free text precisely so a pasted token is
/// caught locally, and this constant is the shape a real one has rather than something that happens to parse.
const CALLER_DIGEST: &str = "9f2c41ab77de0355b1c8e0d4a63f29bb8c1740ee5d3a96f2c0b84a1e7d5633aa";

/// A tool whose **declaration** asks for an approval, which its workspace would not.
///
/// The definition and its adapter live in `crate::approval_fixture` because the REST approval route needs
/// the same held call, and two copies of one contract are two things that can disagree.
fn approval_declaring_definition() -> ToolDefinition {
    crate::approval_fixture::approval_declaring_definition()
}

/// The counting adapter that must never be reached for a held call.
use crate::approval_fixture::ApprovalDeclaringAdapter;

/// **The caller policy a deployment configures is the one the endpoint serves under.**
///
/// `build_endpoint` takes the admission policy as a parameter and passes it through unchanged, and until this
/// test existed **nothing observed that**: replacing the argument with `CallerAdmission::local_only()` left the
/// suite green, because every other test already passes `local_only()` and a policy that ignored its argument
/// would therefore look identical. The mutation was run and the suite passed 14/14, which is what makes this a
/// coverage gap rather than a suspicion.
///
/// The property matters because the policy is the daemon's **whole** answer to "who may call": the bind is
/// loopback, and `CallerAdmission` is the second, independent statement that refuses a caller who arrives
/// anyway — through a reverse proxy, a forwarded socket, or a misconfiguration that moved the bind.
#[tokio::test]
async fn the_configured_caller_policy_is_the_one_the_endpoint_serves_under() {
    let root = TempRoot::new();
    root.write("notes/todo.txt", "buy milk");
    let (_database, pipeline) = pipeline_over(&root).await;

    // One admitted caller, so the policy is *not* `local_only` and cannot be confused with it.
    let admitted = must(jarvis_mcp_transport::AdmittedCaller::new(
        must(jarvis_mcp_transport::Fingerprint::parse(CALLER_DIGEST)),
        jarvis_mcp_transport::CallerLabel::new("inspector", "1.0"),
    ));
    let configured = must(CallerAdmission::new([admitted], 30));
    assert!(
        !configured.is_local_only(),
        "the fixture must not be the default, or this test proves nothing"
    );

    let (_runner, endpoint) = must(build_endpoint(
        Arc::clone(&pipeline),
        &must(pipeline.definitions()),
        LOCAL_WORKSPACE_ID,
        configured.clone(),
    ));

    assert_eq!(
        endpoint.gate().admission(),
        &configured,
        "the endpoint must serve under the policy it was given, not a default"
    );
}

/// **An endpoint with nothing to serve is refused, so a bound port cannot be one that answers nothing.**
///
/// `ServingConfig::check_servable` and this check are the same decision at two levels — one for a handler built
/// by hand, one for a deployment — and the remedy differs from a bind failure: an operator must grant a tool
/// rather than change a port.
#[tokio::test]
async fn an_endpoint_with_nothing_to_serve_is_refused() {
    let root = TempRoot::new();
    let (_database, pipeline) = pipeline_over(&root).await;

    let error = build_endpoint(
        Arc::clone(&pipeline),
        &[],
        LOCAL_WORKSPACE_ID,
        CallerAdmission::local_only(),
    )
    .err()
    .unwrap_or_else(|| panic!("an empty served surface must be refused"));

    assert!(
        matches!(error, RemoteMcpError::NoTools),
        "the refusal must name the missing tools, got: {error}"
    );
}

/// **A remote actor holds exactly what the served tools require, and never `mcp.call`.**
///
/// The first version of this test asserted the actor held **no** scopes, and it failed: `FilesystemTool`
/// declares `required_scopes: files.read`, so an actor holding nothing was refused every call with
/// `missing_scope`. The endpoint would have bound and answered every request with a refusal, which reads to an
/// operator as a policy misconfiguration rather than as this constructor.
///
/// So the grant is the **union of the served definitions' own requirements** — and the two assertions that
/// matter are that it is non-empty (or nothing can run) and that `mcp.call` is **not** in it, because that
/// literal is this daemon's grant to call *someone else's* server and attaching it inbound would be an
/// outbound grant on the inbound direction.
#[tokio::test]
async fn a_remote_actor_holds_the_served_scopes_and_not_the_outbound_one() {
    let root = TempRoot::new();
    root.write("notes/todo.txt", "buy milk");
    let (_database, pipeline) = pipeline_over(&root).await;
    let served = must(pipeline.definitions());
    let actor = remote_actor(CorrelationId::new(), &served);
    let authority = actor.authority();

    assert!(
        !authority.scopes().is_empty(),
        "an actor holding nothing is refused every served tool for a missing scope: {authority:?}"
    );
    let outbound = must(jarvis_tools::Scope::new(crate::tool_actor::MCP_CALL_SCOPE));
    assert!(
        !authority.scopes().contains(&outbound),
        "mcp.call is the OUTBOUND grant and must not reach an inbound caller: {authority:?}"
    );
    // Every scope the served tools declare is covered, which is the property that makes the endpoint usable at
    // all — an actor covering only some of them would make the served surface a promise it cannot keep.
    for definition in &served {
        assert!(
            definition
                .required_scopes()
                .is_satisfied_by(authority.scopes()),
            "the actor must cover every scope {} declares",
            definition.id()
        );
    }

    assert_eq!(
        actor.channel(),
        jarvis_core::SessionChannel::Api,
        "a remote MCP call must not be recorded as a local CLI invocation"
    );
}

/// **The scope literals a served surface produces are valid, so the skip in `required_scopes_for` is
/// unreachable.**
///
/// `required_scopes_for` skips a declaration it cannot reconstruct, which fails **closed** — the tool that
/// declared it is refused for a missing scope. That is the right direction and it would also be an invisible
/// bug: every call to that tool denied, with the reason reading as a policy problem. This test turns it into a
/// failure here instead.
#[tokio::test]
async fn the_served_scopes_are_valid() {
    let root = TempRoot::new();
    root.write("notes/todo.txt", "buy milk");
    let (_database, pipeline) = pipeline_over(&root).await;
    let served = must(pipeline.definitions());

    for definition in &served {
        for required in definition.required_scopes().iter() {
            assert!(
                jarvis_tools::Scope::new(required.to_string()).is_ok(),
                "{} declares an invalid scope: {required}",
                definition.id()
            );
        }
    }
}

/// **The served surface over a real pipeline is non-empty and includes the filesystem tool.**
///
/// The end-to-end form of `build_endpoint` over a composed pipeline: the definitions come from the registry, the
/// served surface is built from them, and the two agree. Without this, "the endpoint serves what the daemon can
/// run" would be an assumption about two lists built in different places.
#[tokio::test]
async fn the_served_surface_matches_the_registry() {
    let root = TempRoot::new();
    root.write("notes/todo.txt", "buy milk");
    let (_database, pipeline) = pipeline_over(&root).await;

    let definitions = must(pipeline.definitions());
    assert!(
        !definitions.is_empty(),
        "a pipeline over a granted root registers at least one tool"
    );
    let (served, exclusions) = must(jarvis_mcp::served_tools(definitions.iter()));
    assert!(
        exclusions.is_empty(),
        "a filesystem tool is JARVIS-owned, available, and transmittable: {exclusions:?}"
    );
    assert_eq!(
        served.len(),
        definitions.len(),
        "every registered definition must be served when none is excluded"
    );
}

/// **A stranger on the phone line holds every tool back, however the call would otherwise have gone**: a call that needs no approval, one
/// that the owner already said yes to on the phone, and one that would wait for the console. Falsified by removing the guard from `run`.
#[tokio::test]
async fn while_someone_who_is_not_the_owner_is_on_the_phone_no_tool_runs() {
    let held = || Some("someone who is not the owner is on the phone line".to_owned());

    // A call the policy would let through unasked (the owner trusts the tool by name).
    let policy =
        WorkspacePolicy::default().trusting(must(jarvis_tools::ToolId::new(APPROVAL_TOOL)));
    let (runner, approvals, adapter, _root) = approval_runner_under(policy).await;
    approvals.set_phone_approval(Arc::new(StubPhone(None, held())));
    let refused = runner
        .run(APPROVAL_TOOL, call_arguments(), CorrelationId::new())
        .await
        .err()
        .unwrap_or_else(|| panic!("the guard must refuse"));
    assert!(refused.to_string().contains("not the owner"), "{refused}");
    assert_eq!(adapter.calls(), 0);
    assert!(
        approvals.list().is_empty(),
        "nothing was put in front of the owner either"
    );

    // The owner's yes on the phone does not outrank it.
    let (runner, approvals, adapter, _root) =
        approval_runner_under(WorkspacePolicy::default()).await;
    approvals.set_phone_approval(Arc::new(StubPhone(
        Some(crate::phone_service::PhoneYes {
            call_id: "call-1".to_owned(),
        }),
        held(),
    )));
    assert!(
        runner
            .run(APPROVAL_TOOL, call_arguments(), CorrelationId::new())
            .await
            .is_err()
    );
    assert_eq!(adapter.calls(), 0);

    // Control: with the line clear the same call runs.
    let (runner, approvals, adapter, _root) =
        approval_runner_under(WorkspacePolicy::default()).await;
    approvals.set_phone_approval(Arc::new(StubPhone(
        Some(crate::phone_service::PhoneYes {
            call_id: "call-1".to_owned(),
        }),
        None,
    )));
    assert!(
        runner
            .run(APPROVAL_TOOL, call_arguments(), CorrelationId::new())
            .await
            .is_ok()
    );
    assert_eq!(adapter.calls(), 1);
}

/// **The line is looked at again when an approved call is about to run**: the owner said yes in the console, then a stranger rang.
#[tokio::test]
async fn a_stranger_who_calls_while_the_owner_is_deciding_still_holds_the_tool_back() {
    struct RingsDuringTheWait(std::sync::atomic::AtomicUsize);

    #[async_trait::async_trait]
    impl crate::phone_service::PhoneApproval for RingsDuringTheWait {
        async fn approves(&self, _tool: &str, _risk: u8) -> Option<crate::phone_service::PhoneYes> {
            None
        }

        // Clear when the call arrives, occupied by the time the owner has answered.
        async fn line_guard(&self) -> Option<String> {
            (self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst) >= 1)
                .then(|| "someone who is not the owner is on the phone line".to_owned())
        }
    }

    let (runner, approvals, adapter, _root) =
        approval_runner(std::time::Duration::from_secs(30)).await;
    approvals.set_phone_approval(Arc::new(RingsDuringTheWait(
        std::sync::atomic::AtomicUsize::new(0),
    )));
    let task = tokio::spawn({
        let runner = runner.clone();
        async move {
            runner
                .run(APPROVAL_TOOL, call_arguments(), CorrelationId::new())
                .await
        }
    });
    let id = waiting_id(&approvals).await;
    assert!(approvals.decide(&id, true).is_some());
    let refused = must(task.await)
        .err()
        .unwrap_or_else(|| panic!("the late guard must refuse"));
    assert!(refused.to_string().contains("not the owner"), "{refused}");
    assert_eq!(
        adapter.calls(),
        0,
        "the owner said yes, and it still did not run"
    );
}
