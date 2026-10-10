//! JARVIS serving its own tools over MCP: the endpoint, the runner, and **which run a remote call has**.
//!
//! # The limit this closes
//!
//! Every `P3-009` slice recorded it in a different form: *"nothing binds, so no request reaches this gate."*
//! `P3-009c-a` built the layer a listener mounts and still recorded that **the daemon does not mount it**.
//! This module mounts it, which makes JARVIS reachable as an MCP server.
//!
//! # A remote MCP call has no run, and the schema is what says so
//!
//! Every other tool path in this daemon attributes a call to a run. An MCP request carries none of what that
//! needs: no run, no session, no actor. The tempting shortcut is to invent a run identifier so the pipeline's
//! `CallOrigin` can be satisfied â€” and the honest answer is that **the database refuses to store one**:
//! `0007_tool_calls.sql` declares `run_id TEXT NOT NULL REFERENCES agent_runs (id)`, so a call with no run
//! cannot be written as a `tool_calls` row at all. That is not an obstacle to work around; it is the schema
//! stating a fact about the domain.
//!
//! # What a remote call gets, checked against the architecture requirement rather than asserted
//!
//! `docs/architecture/tools-and-connectors.md` requires a served tool to have "JARVIS scopes, policy, approval,
//! output limits, and audit" applied. Reading this module against that sentence:
//!
//! | Requirement | Enforced? | How, or why not |
//! | --- | --- | --- |
//! | Argument validation | **Yes** | [`ToolPipeline::call_remote_tool`] calls the *same* `validate` the local path does, so a remote caller gets the identical check. |
//! | Policy | **Yes** | The same `evaluate` call, over the same definition, the same `WorkspacePolicy`, and the same dispatcher. |
//! | Workspace grant | **Yes** | The identical adapter, so the `cap-std` confinement of `ADR-0020` applies to a remote request exactly as to an operator's own. |
//! | Scope | **Yes, derived from the served surface** | [`ToolActor::remote`] holds the **union of the served definitions' `required_scopes`** — exactly what the tools this endpoint advertises demand, and nothing more. It does **not** hold `mcp.call`, which is this daemon's grant to call *someone else's* server: attaching it inbound would be a scope with the right name and the wrong direction. The first version granted *no* scopes and every call was refused for a missing one, which is recorded in `ADR-0039` as the error the union replaced. |
//! | The served set | **Yes** | `JarvisMcpServer` refuses a name that was never advertised **before** the runner is consulted (`P3-009e`). |
//! | The two inbound policies | **Yes** | `ServedEndpoint` applies the `Origin` and caller decisions to every request (`P3-009c-a`). |
//! | Output limits | **Yes** | The adapter's own bound, which is a property of the adapter rather than of the path. |
//! | Approval | **Asked of the owner, and refused without a yes** | There is no run to park, so the call **waits in memory** for the owner's answer (`inbound_approvals`, `ADR-0180`): listed with the owner's other approvals, answered by one click, command or word. A denial, no answer within two minutes, or a full list is a refusal; nothing runs without the yes. |
//! | Audit | **Partial, and recorded as such** | A remote call is logged and has **no `tool_calls` row**, because that row needs a run. `P3-012` owns the durable link. |
//!
//! # Why the runner is the daemon's, which is why the port exists
//!
//! `ServedToolRunner` was declared in the transport crate precisely so the attribution question stayed here
//! (`P3-009e`: *"a remote call has no JARVIS run â€¦ and the daemon alone knows what one should be attributed
//! to"*). This module is that answer, and it is stated in the type rather than in a comment: the actor is built
//! by [`ToolActor::remote`], whose doc explains why its `run_id` is a correlation identity rather than a run.

use std::sync::Arc;

use async_trait::async_trait;
use jarvis_core::CorrelationId;
use jarvis_mcp_transport::{
    CallerAdmission, JarvisMcpServer, ServedEndpoint, ServedExposureError, ServedToolRunner,
    ServerExposure, ServingConfig, served_tools_vetting,
};
use jarvis_tools::{AdapterError, ToolCallResult, ToolDefinition};
use serde_json::Value;

use crate::inbound_approvals::InboundApprovals;
use crate::tool_actor::ToolActor;
use crate::tool_pipeline::{OwnerApproval, ToolPipeline, ToolPipelineError, ToolPipelineOutcome};

/// The policy version label recorded on a remote call's receipt.
///
/// The same label the gateway states for the same reason: a **label**, not a verifiable version, which
/// `ADR-0021` records as a limit and `P3-003`'s engine is where a real one would come from. One constant rather
/// than two literals, so the two paths cannot drift into recording different policy identities for one policy.
pub const POLICY_VERSION: &str = "policy-1";

/// Runs one served MCP call against the composed tool pipeline.
///
/// Holds the **pipeline** rather than the dispatcher, which is the decision that keeps this from being a second
/// enforcement path: every gate a local call passes is passed by calling the same code, in the same order, with
/// the same definitions.
pub struct RemoteMcpRunner {
    /// The composed pipeline, reached through its remote entrypoint.
    pipeline: Arc<ToolPipeline>,
    /// The profile's own workspace identifier, which no request can influence.
    workspace_id: String,
    /// The definitions this endpoint serves, so the actor's scopes are derived from them.
    ///
    /// Held rather than recomputed per call because the served surface is fixed at construction: the actor's
    /// grant must be the scopes of the tools **this endpoint advertises**, and reading them from the pipeline on
    /// every call would let the grant follow a registry change the served surface did not.
    served: Vec<ToolDefinition>,
    /// Calls waiting for the owner's answer, shared with the console's approvals list.
    approvals: Arc<InboundApprovals>,
    /// How long a call waits for the owner before it is refused.
    wait: std::time::Duration,
    /// Who answers: the profile's own local identity, recorded on the receipt as the approver.
    owner_id: String,
}

impl RemoteMcpRunner {
    /// Builds a runner over the composed pipeline, the served definitions, and the profile's own identity.
    ///
    /// Takes the identifier as a value rather than reading it from a request, which is the rule
    /// `docs/architecture/identity-and-workspaces.md` states: a caller-supplied workspace would let a client
    /// widen its own authority over a protocol whose requests carry even less to go on.
    #[must_use]
    pub fn new(
        pipeline: Arc<ToolPipeline>,
        served: Vec<ToolDefinition>,
        workspace_id: impl Into<String>,
    ) -> Self {
        Self {
            pipeline,
            workspace_id: workspace_id.into(),
            served,
            approvals: Arc::new(InboundApprovals::default()),
            wait: crate::inbound_approvals::WAIT,
            owner_id: "local-owner".to_owned(),
        }
    }

    /// Shares the list the owner answers from, says who answers, and how long a call waits.
    #[must_use]
    pub fn with_approvals(
        mut self,
        approvals: Arc<InboundApprovals>,
        owner_id: impl Into<String>,
        wait: std::time::Duration,
    ) -> Self {
        self.approvals = approvals;
        self.owner_id = owner_id.into();
        self.wait = wait;
        self
    }

    /// Puts a call that needs the owner's yes in front of them, waits, and runs it only on a yes.
    async fn ask_owner(
        &self,
        name: &str,
        arguments: Value,
        correlation_id: CorrelationId,
    ) -> Result<ToolCallResult, AdapterError> {
        let refuse = |reason: &str| AdapterError::RefusedBeforeReaching {
            reason: reason.to_owned(),
        };
        let definition = self
            .served
            .iter()
            .find(|definition| definition.id().to_string() == name)
            .ok_or_else(|| refuse("that tool is not offered here"))?;
        let preview = format!("{} (asked by another app)", definition.title());
        let (id, answer) = self
            .approvals
            .ask(
                name,
                definition.version(),
                definition.risk().level(),
                &preview,
                &arguments,
                self.wait,
            )
            .map_err(|_| {
                refuse("too many calls are already waiting for the owner, so this one was refused")
            })?;
        // If this task is cancelled while waiting, the question goes with it.
        let _withdraw = crate::inbound_approvals::Withdraw::new(&self.approvals, &id);
        tracing::info!(tool = name, approval = %id, "a call from another app is waiting for the owner's answer");
        // Timed out, or the list was dropped: nobody said yes.
        let Ok(Ok(approved)) = tokio::time::timeout(self.wait, answer).await else {
            self.approvals.forget(&id);
            tracing::info!(tool = name, approval = %id, "no answer from the owner, so the call from another app was refused");
            return Err(refuse(
                "the owner did not answer in time, so the call was not run",
            ));
        };
        if !approved {
            tracing::info!(tool = name, approval = %id, "the owner refused a call from another app");
            return Err(refuse("the owner said no, so the call was not run"));
        }
        tracing::info!(tool = name, approval = %id, "the owner approved a call from another app");
        let now = jarvis_core::UtcTimestamp::now(&jarvis_core::SystemClock);
        let actor = ToolActor::remote(
            self.workspace_id.clone(),
            correlation_id,
            POLICY_VERSION,
            &self.served,
        );
        let owner = OwnerApproval {
            approval_id: id,
            approver_id: self.owner_id.clone(),
            approved_at: now,
            expires_at: jarvis_core::UtcTimestamp::from_unix_nanos(
                now.unix_nanos() + i128::try_from(self.wait.as_nanos()).unwrap_or(i128::MAX / 4),
            )
            .unwrap_or(now),
        };
        self.pipeline
            .call_remote_tool_with(name, arguments, &actor, correlation_id, Some(owner))
            .await
            .map_or_else(|error| Err(map_pipeline_error(error, name)), map_outcome)
    }
}

impl Clone for RemoteMcpRunner {
    fn clone(&self) -> Self {
        Self {
            pipeline: Arc::clone(&self.pipeline),
            workspace_id: self.workspace_id.clone(),
            served: self.served.clone(),
            approvals: Arc::clone(&self.approvals),
            wait: self.wait,
            owner_id: self.owner_id.clone(),
        }
    }
}

#[async_trait]
impl ServedToolRunner for RemoteMcpRunner {
    async fn run(
        &self,
        name: &str,
        arguments: Value,
        correlation_id: CorrelationId,
    ) -> Result<ToolCallResult, AdapterError> {
        let actor = ToolActor::remote(
            self.workspace_id.clone(),
            correlation_id,
            POLICY_VERSION,
            &self.served,
        );

        match self
            .pipeline
            .call_remote_tool(name, arguments.clone(), &actor, correlation_id)
            .await
        {
            // The call needs the owner's yes: it waits for it here, and runs only on one.
            Ok(ToolPipelineOutcome::AwaitingApproval { .. }) => {
                self.ask_owner(name, arguments, correlation_id).await
            }
            other => other.map_or_else(|error| Err(map_pipeline_error(error, name)), map_outcome),
        }
    }
}

/// Maps a pipeline outcome to what the MCP handler reports.
///
/// A refusal is a **result about the call**, not a transport failure, so it travels as
/// [`AdapterError::RefusedBeforeReaching`] — the variant that says nothing happened. `Failed` would be wrong: it
/// would imply the adapter ran and the tool failed.
fn map_outcome(outcome: ToolPipelineOutcome) -> Result<ToolCallResult, AdapterError> {
    match outcome {
        ToolPipelineOutcome::Executed(result) => Ok(*result),
        ToolPipelineOutcome::Refused { reason_code } => Err(AdapterError::RefusedBeforeReaching {
            reason: format!("the call was refused ({reason_code})"),
        }),
        // **Unreachable on this path, and reported rather than treated as a refusal.** `call_remote_tool`
        // refuses a held decision instead of returning it, so reaching here would mean that changed — and a
        // caller told "refused" would not learn that an approval was silently skipped. This arm is covered by a
        // test that constructs the value directly, because no path in this daemon can produce it.
        ToolPipelineOutcome::AwaitingApproval { reason_code, .. } => {
            Err(AdapterError::RefusedBeforeReaching {
                reason: format!(
                    "the call needs an approval ({reason_code}), and a remote MCP call has no run to hold one"
                ),
            })
        }
    }
}

/// Maps a pipeline fault to an adapter error **without weakening its claim**.
///
/// The distinction this function exists to preserve is `AdapterError`'s own three-way split, which says how much
/// a caller may conclude: [`AdapterError::RefusedBeforeReaching`] (nothing happened),
/// [`AdapterError::ProviderRefused`] (the provider said no), and [`AdapterError::AmbiguousAfterReaching`] (a
/// request left and the outcome is unknown). Rewriting all three into "nothing was reached" would tell a client
/// to retry a call whose effect is unknown — and for a non-idempotent tool the retry is **a second effect**,
/// which is the exact failure `AmbiguousAfterReaching` exists to prevent.
///
/// The other pipeline errors are all raised **before the dispatcher is consulted** — an ill-formed identifier, an
/// unknown tool, invalid arguments, an unintelligible intent, a rejected receipt, an uncovered tool — so saying
/// nothing was reached is then a statement the code supports rather than an assumption about it. That
/// "before dispatch" claim is what makes it falsifiable: the `AdapterCall` arm is the only one that can follow a
/// dispatch, and it is the only one that is not rewritten.
fn map_pipeline_error(error: ToolPipelineError, name: &str) -> AdapterError {
    match error {
        ToolPipelineError::AdapterCall(error) => error,
        other => AdapterError::RefusedBeforeReaching {
            reason: format!("{name} could not be run: {other}"),
        },
    }
}

/// Why the MCP endpoint could not be served.
#[derive(Debug, thiserror::Error)]
pub enum RemoteMcpError {
    /// The endpoint was enabled with nothing to serve.
    #[error("the MCP endpoint was enabled but there is no tool that could be served")]
    NoTools,
    /// The served surface was refused, which names the reason.
    #[error("the served tool surface could not be built: {0}")]
    Surface(#[from] ServedExposureError),
    /// The serving configuration was refused.
    #[error("the MCP serving configuration could not be built: {0}")]
    Serving(#[from] jarvis_mcp_transport::ServingConfigError),
    /// The caller policy for the access key was refused.
    #[error("the access key's caller policy could not be built: {0}")]
    Admission(#[from] jarvis_mcp_transport::AdmissionError),
    /// The access key could not be made.
    #[error("{0}")]
    Key(String),
    /// The endpoint could not be constructed.
    #[error("the MCP endpoint could not be built: {0}")]
    Endpoint(#[from] jarvis_mcp_transport::ServiceError),
    /// The loopback address could not be bound.
    #[error(
        "port {port} could not be used for letting other apps connect (another program may be using it); choose another with `jarvis config set daemon.mcp_serve_port N`"
    )]
    Bind {
        /// The configured port.
        port: u16,
        /// The underlying I/O failure.
        #[source]
        source: std::io::Error,
    },
}

/// The definitions a remote caller may be offered.
///
/// The tools that change the owner's own files are never served: an external client reaching them would write
/// to the owner's folders on the strength of an allowlist entry alone.
pub(crate) fn servable_definitions(definitions: &[ToolDefinition]) -> Vec<ToolDefinition> {
    definitions
        .iter()
        .filter(|definition| {
            let id = definition.id().to_string();
            id != crate::tool_search::SEARCH_TOOL
                && id != jarvis_tools::WRITE_TOOL
                && id != jarvis_tools::EDIT_TOOL
                && id != jarvis_tools::MOVE_TOOL
        })
        .cloned()
        .collect()
}

/// Builds the endpoint JARVIS serves, over an already-composed pipeline.
///
/// # Errors
///
/// Returns a [`RemoteMcpError`] for every reason the endpoint cannot be built: nothing to serve, a refused
/// exposure policy, or an empty served surface. Each is a **startup** failure, because a daemon that bound a
/// port it cannot answer on would report itself ready while being unusable.
#[cfg(test)]
pub fn build_endpoint(
    pipeline: Arc<ToolPipeline>,
    definitions: &[ToolDefinition],
    workspace_id: impl Into<String>,
    caller_policy: CallerAdmission,
) -> Result<(RemoteMcpRunner, ServedEndpoint), RemoteMcpError> {
    build_endpoint_vetting(
        pipeline,
        definitions,
        workspace_id,
        caller_policy,
        &[],
        &Arc::new(InboundApprovals::default()),
        "local-owner",
    )
}

/// As [`build_endpoint`], also serving the tools of the servers named by `vetted_prefixes` (such as `mcp.browser.`).
///
/// # Errors
///
/// As [`build_endpoint`].
pub fn build_endpoint_vetting(
    pipeline: Arc<ToolPipeline>,
    definitions: &[ToolDefinition],
    workspace_id: impl Into<String>,
    caller_policy: CallerAdmission,
    vetted_prefixes: &[String],
    approvals: &Arc<InboundApprovals>,
    owner_id: &str,
) -> Result<(RemoteMcpRunner, ServedEndpoint), RemoteMcpError> {
    let definitions = servable_definitions(definitions);
    let definitions = definitions.as_slice();
    if definitions.is_empty() {
        return Err(RemoteMcpError::NoTools);
    }

    // The served surface, built through the same function the operator-facing inventory uses, so *which tools
    // may be advertised* is one decision rather than two.
    let (served, exclusions) = served_tools_vetting(definitions.iter(), |definition| {
        let id = definition.id().to_string();
        vetted_prefixes
            .iter()
            .any(|prefix| id.starts_with(prefix.as_str()))
    })?;
    if !exclusions.is_empty() {
        // Logged rather than silent: an operator who configured a server and finds its tools absent from a
        // client's list needs to know that this is why.
        tracing::info!(
            excluded = exclusions.len(),
            "some tools are not served over MCP"
        );
    }

    let runner = RemoteMcpRunner::new(pipeline, definitions.to_vec(), workspace_id).with_approvals(
        Arc::clone(approvals),
        owner_id,
        crate::inbound_approvals::WAIT,
    );
    let handler = JarvisMcpServer::new(served, Arc::new(runner.clone()));

    // **`loopback_only`, built through `ServingConfig::new` rather than from configuration.** An operator's
    // origin list is deliberately *not* a configuration surface in this slice: `ServingConfig::new` refuses a
    // policy requiring a remote bind, and a configurable list would be a value that could make a startup either
    // fail or serve something nobody chose. The caller allowlist is where a deployment expresses *who* may
    // call, and `P3-009g` already made its default safe.
    let serving = ServingConfig::new(ServerExposure::loopback_only())?;
    let endpoint = ServedEndpoint::new(serving, caller_policy, handler)?;
    Ok((runner, endpoint))
}

/// A bound MCP endpoint, held by the daemon for its lifetime.
///
/// Owns the serving task for the same reason the REST transport does: `axum::serve` never returns an error and
/// retries socket errors itself, so its return value carries no information and the task's **completion** is
/// the only observable evidence that the listener stopped.
pub struct ServingMcp {
    port: u16,
    policy: jarvis_mcp_transport::RequestGate,
    shutdown: tokio::sync::oneshot::Sender<()>,
    handle: tokio::task::JoinHandle<()>,
}

impl ServingMcp {
    /// Binds loopback, serves the endpoint, and returns the transport and its stop signal.
    ///
    /// # Errors
    ///
    /// Returns [`RemoteMcpError::Bind`] when the loopback address cannot be bound. Binding is the step that
    /// fails fast: an occupied port is reported here rather than becoming a listener the daemon believes it has.
    pub async fn bind(
        port: u16,
        endpoint: ServedEndpoint,
        access: Arc<crate::mcp_access::McpAccess>,
    ) -> Result<(Self, tokio::sync::oneshot::Receiver<()>), RemoteMcpError> {
        // Loopback only, unconditionally. `ServingConfig::new` already refuses an exposure policy that would
        // require a remote bind, so this and that refusal are two statements of one decision.
        let address = std::net::SocketAddr::from(([127, 0, 0, 1], port));
        let listener = tokio::net::TcpListener::bind(address)
            .await
            .map_err(|source| RemoteMcpError::Bind { port, source })?;

        let policy = endpoint.gate().clone();

        // The MCP service takes over the whole request: the SDK's service dispatches on its own headers and
        // methods, so it is mounted as the router's **fallback** rather than on a route. `axum`'s `Router`
        // accepts any `Service` whose future is `Send`, which is why the binding layer pins its associated
        // future type rather than returning a bare `impl Service`.
        // The access key is required first; a request without it never reaches the endpoint (`ADR-0170`).
        let app = axum::Router::new()
            .fallback_service(endpoint.into_service())
            .layer(axum::middleware::from_fn_with_state(
                Arc::clone(&access),
                crate::mcp_access::require_key,
            ));
        access.set_serving_port(port);

        let (shutdown, shutdown_rx) = tokio::sync::oneshot::channel::<()>();
        let (stopped_tx, stopped) = tokio::sync::oneshot::channel::<()>();
        let handle = tokio::spawn(async move {
            let _ = axum::serve(listener, app)
                .with_graceful_shutdown(async move {
                    let _ = shutdown_rx.await;
                })
                .await;
            // Sent explicitly, because this is the only observable evidence that the listener stopped.
            let _ = stopped_tx.send(());
        });

        Ok((
            Self {
                port,
                policy,
                shutdown,
                handle,
            },
            stopped,
        ))
    }

    /// Returns the bound loopback port.
    #[must_use]
    pub const fn port(&self) -> u16 {
        self.port
    }

    /// Returns the inbound policy the endpoint serves under.
    ///
    /// Exposed so the daemon's startup log and a diagnostic can state the policy in force, which is what lets
    /// an operator confirm a configuration was loaded rather than comparing files to a running process.
    #[must_use]
    pub const fn policy(&self) -> &jarvis_mcp_transport::RequestGate {
        &self.policy
    }

    /// Stops the transport, letting in-flight responses close.
    ///
    /// Graceful rather than abrupt, matching the REST transport's decision and for the same reason: a response
    /// severed mid-flight looks to a client like a lost request, leaving it to decide whether to retry â€” and a
    /// retry of a non-idempotent effect is a second effect.
    pub async fn shutdown(self) {
        let _ = self.shutdown.send(());
        let _ = self.handle.await;
    }
}

#[cfg(test)]
#[path = "mcp_serve_tests.rs"]
mod tests;
