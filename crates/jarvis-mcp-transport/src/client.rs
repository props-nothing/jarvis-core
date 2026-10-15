//! Connecting to an MCP server and reading what it offers.
//!
//! This is the only place in the workspace that speaks the MCP wire. Everything it returns is either
//! a `jarvis_mcp` type or plain `serde_json`, so no SDK type escapes this module — see `AGENTS.md`'s
//! "provider SDK types must not cross JARVIS domain boundaries".
//!
//! # Two decisions worth knowing before reading the code
//!
//! **The lifecycle is named, never defaulted, and it is dual-era.** The SDK's `serve()` performs the
//! legacy `initialize` handshake, which protocol revision `2026-07-28` removed. `Auto` is used
//! explicitly: probe with `server/discover` and fall back to `initialize` when the peer does not know
//! it, which is the stdio backward-compatibility procedure the specification itself gives
//! (`basic/versioning`, read 2026-10-10). The first version of this module refused every legacy
//! server, and the first real server it met, the reference `server-everything` 2026.8.31, answers
//! only `initialize`, so it could reach nothing that exists today. Which era was used is recorded
//! ([`McpEra`]), never hidden, and only the era-independent parts of the protocol are used (`tools/list`,
//! `tools/call`).
//!
//! **A server's `annotations` never reach the translation.** [`rmcp::model::Tool`] carries
//! `annotations`, and `AGENTS.md` plus ADR-0025 require the posture of a tool to come from an
//! operator. The field is *structurally unreachable* here because [`McpToolListing`] has no place to
//! put it, so the translation cannot consult it even by accident — which is a stronger guarantee
//! than a comment saying not to.

use std::borrow::Cow;
use std::time::Duration;

use jarvis_mcp::{McpToolListing, ReportedIdentity, ServerName};
use rmcp::model::{
    CallToolRequestParams, CallToolResponse, ClientCapabilities, ClientConfig, Implementation, Tool,
};
use rmcp::service::{ClientLifecycleMode, ClientServiceExt, RunningService, ServiceError};
use rmcp::transport::StreamableHttpClientTransport;
use rmcp::transport::streamable_http_client::StreamableHttpClientTransportConfig;
use serde_json::Value;

use crate::credentials::{
    CredentialEnvironment, CredentialRedactor, McpSecretResolver, UnavailableSecrets,
    validate_references,
};
use crate::endpoint::{McpHttpEndpoint, build_http_client};
use crate::error::{CallError, ConnectError, ListError};
use crate::http_auth::{AuthorizedHttpClient, HttpCredentialProvider};
use crate::revision::{MODERN_REVISION, modern_revision};

/// Identity JARVIS advertises to a server, and the version it advertises it with.
const CLIENT_NAME: &str = "jarvis";
const CLIENT_VERSION: &str = env!("CARGO_PKG_VERSION");

/// How long a server has to complete the whole handshake before the connection is abandoned.
///
/// Covers both attempts: the SDK waits up to ten seconds for `server/discover` in `Auto` mode and
/// then runs the legacy `initialize` with no deadline of its own, so a peer that answers neither
/// would otherwise hold a daemon at startup. Twenty seconds leaves the second attempt about as long
/// as the first.
const DISCOVERY_TIMEOUT: Duration = Duration::from_secs(20);

/// Which generation of the protocol a connection speaks.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum McpEra {
    /// `2026-07-28` or later: stateless, per-request metadata, no `initialize`.
    Modern,
    /// `2025-11-25` or earlier: an `initialize` handshake and a session. Every server published
    /// before the modern revision speaks this, and many still do.
    Legacy,
}

/// How a local MCP server is launched.
///
/// A command and its arguments, deliberately not a shell string: `sh -c`/`cmd /c` would make every
/// argument a quoting question and would let a configured server name become a command. The program
/// is spawned directly, so the worst case of a malformed configuration is a failed spawn rather than
/// an executed string.
#[derive(Clone, Debug)]
pub struct StdioCommand {
    program: String,
    args: Vec<String>,
    credentials: CredentialEnvironment,
}

impl StdioCommand {
    /// Records a program and its arguments.
    ///
    /// Refuses an empty program, because spawning `""` fails at the OS with a message that does not
    /// say which configured server was at fault.
    ///
    /// # Errors
    ///
    /// Returns [`ConnectError::Unreachable`] when the program name is empty or whitespace.
    pub fn new(program: &str, args: &[&str]) -> Result<Self, ConnectError> {
        let program = program.trim();
        if program.is_empty() {
            return Err(ConnectError::Unreachable(
                "an MCP server command needs a program to run".to_owned(),
            ));
        }
        Ok(Self {
            program: program.to_owned(),
            args: args.iter().map(|arg| (*arg).to_owned()).collect(),
            credentials: CredentialEnvironment {
                references: std::collections::BTreeMap::new(),
                resolver: std::sync::Arc::new(UnavailableSecrets),
            },
        })
    }

    /// Adds credential references for this server; values are resolved only when it starts.
    ///
    /// # Errors
    ///
    /// Refuses non-credential variable names, literal values, or another server's references.
    pub fn with_credentials(
        mut self,
        server: &str,
        environment: &std::collections::BTreeMap<String, String>,
    ) -> Result<Self, ConnectError> {
        self.credentials.references =
            validate_references(server, environment).map_err(ConnectError::Unreachable)?;
        Ok(self)
    }

    /// Supplies the profile-owned resolver; no secret is read or retained in the command.
    #[must_use]
    pub fn with_secret_resolver(mut self, resolver: std::sync::Arc<dyn McpSecretResolver>) -> Self {
        self.credentials.resolver = resolver;
        self
    }

    /// Returns the program this command runs.
    #[must_use]
    pub fn program(&self) -> &str {
        &self.program
    }

    /// Returns the arguments, in order.
    #[must_use]
    pub fn args(&self) -> &[String] {
        &self.args
    }
}

/// What a connected server declared it can do.
///
/// Only the facts this crate acts on. A server also declares `resources` and `prompts`, and they are
/// deliberately **not** modelled: nothing consumes them, and a capability field with no reader gets
/// its meaning invented by its first caller — the rule `P3-005` recorded for the adapter port. When
/// `P3-009` reports why a server is uninteresting to JARVIS those become readable facts and belong
/// here then.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ServerCapabilities {
    /// The server declared a `tools` capability.
    pub tools: bool,
    /// The server declared it will notify when its tool list changes.
    ///
    /// Recorded separately because a server may offer tools without promising to say when they
    /// change, and a client that assumed the notification would cache a stale list forever.
    pub tool_list_changed: bool,
}

impl ServerCapabilities {
    /// Returns whether the server offers the tool listing this crate reads.
    ///
    /// Callers should check this before listing: a server that declared no `tools` capability may
    /// still answer `tools/list` with an empty list, and "offers no tools" and "does not do tools"
    /// are different facts an operator needs to tell apart.
    #[must_use]
    pub fn offers_tools(&self) -> bool {
        self.tools
    }
}

/// A live connection to one MCP server.
///
/// Holds the SDK's running service. The type is public so a caller can keep a connection open across
/// several listings, but no SDK type appears in its public surface.
pub struct McpConnection {
    server: ServerName,
    /// The running service. Replaced when the server stopped and was started again; held in an `Arc` so a call in
    /// flight keeps the service it started on, and never across an `await` while locked.
    service: std::sync::RwLock<std::sync::Arc<Service>>,
    /// How to start the server again, when this connection knows. `None` for a connection made over a
    /// transport handed in by a test.
    recipe: Option<Recipe>,
    /// When the last attempt to start it again failed. Also serialises the attempts.
    restart: tokio::sync::Mutex<Option<std::time::Instant>>,
    redactor: std::sync::Arc<std::sync::RwLock<CredentialRedactor>>,
}

/// The SDK's running service for a client.
type Service = RunningService<rmcp::RoleClient, ClientConfig>;

/// How long to wait after a failed restart before trying again, so a server that crashes at start is not
/// hammered once per tool call.
const RESTART_BACKOFF: Duration = Duration::from_secs(10);

/// How a server is started, kept so it can be started again.
#[derive(Clone, Debug)]
enum Recipe {
    Stdio(StdioCommand),
    Http(McpHttpEndpoint),
    HttpWithAuth(AuthorizedHttpClient),
}

impl std::fmt::Debug for McpConnection {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("McpConnection")
            .field("server", &self.server.as_str())
            .finish_non_exhaustive()
    }
}

impl McpConnection {
    /// Returns the operator-chosen name of the server this connection is to.
    #[must_use]
    pub fn server(&self) -> &ServerName {
        &self.server
    }

    /// Returns the protocol revision that was **actually negotiated**.
    ///
    /// An empty string means the SDK reported no revision, which is surfaced rather than defaulted:
    /// a caller recording this cannot invent a revision the peer never agreed to, and `server_info`
    /// being optional under `server/discover` is exactly the kind of absence this must not paper
    /// over.
    #[must_use]
    pub fn negotiated_revision(&self) -> String {
        self.live().peer_info().map_or_else(String::new, |info| {
            info.protocol_version.as_str().to_owned()
        })
    }

    /// Returns the service this connection is on now.
    fn live(&self) -> std::sync::Arc<Service> {
        std::sync::Arc::clone(
            &self
                .service
                .read()
                .unwrap_or_else(std::sync::PoisonError::into_inner),
        )
    }

    /// Whether the server's side of the connection is gone. `is_closed` alone is not enough: it reports only a
    /// cancellation, and a child that exited leaves the transport closed with nothing cancelled.
    fn has_stopped(&self) -> bool {
        let service = self.live();
        service.is_closed() || service.peer().is_transport_closed()
    }

    /// Starts the server again when it has stopped, so a crash costs one failed call instead of every call
    /// until the daemon restarts.
    ///
    /// Nothing has been sent when this fails, so a caller may report the call as **not sent** (safe to retry)
    /// rather than as ambiguous. A server that fails to start is not tried again for [`RESTART_BACKOFF`].
    async fn ensure_live(&self) -> Result<(), String> {
        if !self.has_stopped() {
            return Ok(());
        }
        let Some(recipe) = &self.recipe else {
            return Err(
                "the server has stopped and this connection cannot start it again".to_owned(),
            );
        };
        let mut last_failure = self.restart.lock().await;
        // Another call may have restarted it while this one waited for the lock.
        if !self.has_stopped() {
            return Ok(());
        }
        if last_failure.is_some_and(|at| at.elapsed() < RESTART_BACKOFF) {
            return Err(
                "the server has stopped and did not restart a moment ago; it will be tried again shortly"
                    .to_owned(),
            );
        }
        let started = match recipe {
            Recipe::Stdio(command) => match stdio_transport(command) {
                Ok((transport, redactor)) => {
                    self.redactor
                        .write()
                        .unwrap_or_else(std::sync::PoisonError::into_inner)
                        .extend(redactor.values());
                    negotiate(transport).await.map_err(|error| {
                        let guard = self
                            .redactor
                            .read()
                            .unwrap_or_else(std::sync::PoisonError::into_inner);
                        redact_connect_error(error, &guard)
                    })
                }
                Err(error) => Err(error),
            },
            Recipe::Http(endpoint) => match http_transport(endpoint) {
                Ok(transport) => negotiate(transport).await,
                Err(error) => Err(error),
            },
            Recipe::HttpWithAuth(client) => negotiate(authorized_http_transport(client))
                .await
                .map_err(|error| {
                    let guard = self
                        .redactor
                        .read()
                        .unwrap_or_else(std::sync::PoisonError::into_inner);
                    redact_connect_error(error, &guard)
                }),
        };
        match started {
            Ok(service) => {
                *self
                    .service
                    .write()
                    .unwrap_or_else(std::sync::PoisonError::into_inner) =
                    std::sync::Arc::new(service);
                *last_failure = None;
                Ok(())
            }
            Err(error) => {
                *last_failure = Some(std::time::Instant::now());
                Err(format!(
                    "the server has stopped and could not be started again: {error}"
                ))
            }
        }
    }

    /// Returns the era this connection speaks.
    #[must_use]
    pub fn era(&self) -> McpEra {
        if self.negotiated_revision() == MODERN_REVISION {
            McpEra::Modern
        } else {
            McpEra::Legacy
        }
    }

    /// Returns what the server reported about itself, when it reported anything.
    ///
    /// This is the input to `P3-008d`'s identity-drift check: the operator chose the local name, so
    /// the server's own claim is evidence and nothing else.
    ///
    /// **`server_info` is optional in the modern lifecycle.** The SDK's own source notes that
    /// `server/discover` responses "are not required to provide it", so a fully conforming modern
    /// server may report no identity at all. That absence is returned as an empty
    /// [`ReportedIdentity`] rather than as an error, because `P3-008d` must be able to compare "no
    /// claim" against a previous claim — a server that *stops* naming itself is a drift worth
    /// seeing, and treating it as a failure would discard exactly that observation.
    #[must_use]
    pub fn reported_identity(&self) -> ReportedIdentity {
        self.live()
            .peer_info()
            .and_then(|info| info.server_info.clone())
            .map_or_else(
                || ReportedIdentity::new("", None),
                |reported| {
                    ReportedIdentity::new(
                        &self.redact(&reported.name),
                        reported
                            .title
                            .as_deref()
                            .map(|title| self.redact(title))
                            .as_deref(),
                    )
                },
            )
    }

    /// Returns what the server declared it can do.
    #[must_use]
    pub fn capabilities(&self) -> ServerCapabilities {
        const NONE: ServerCapabilities = ServerCapabilities {
            tools: false,
            tool_list_changed: false,
        };
        self.live()
            .peer_info()
            .map_or(NONE, |info| ServerCapabilities {
                tools: info.capabilities.tools.is_some(),
                tool_list_changed: info
                    .capabilities
                    .tools
                    .as_ref()
                    .is_some_and(|tools| tools.list_changed == Some(true)),
            })
    }

    /// Lists every tool the server offers, across all pages.
    ///
    /// Returns `jarvis_mcp` listings whose borrows live as long as `buffer`. Passing the buffer in
    /// rather than owning it means the caller decides how long the schema documents live, which is
    /// what lets a caller translate immediately and drop the wire representation.
    ///
    /// # Errors
    ///
    /// Returns [`ListError::Unavailable`] when the request did not complete and
    /// [`ListError::PeerError`] when the peer answered with a protocol error.
    pub async fn list_tools<'a>(
        &self,
        buffer: &'a mut ToolBuffer,
    ) -> Result<Vec<McpToolListing<'a>>, ListError> {
        self.ensure_live().await.map_err(ListError::Unavailable)?;
        let tools =
            self.live()
                .list_all_tools()
                .await
                .map_err(|error| match classify_list(&error) {
                    ListError::Unavailable(text) => ListError::Unavailable(self.redact(&text)),
                    ListError::PeerError(text) => ListError::PeerError(self.redact(&text)),
                })?;
        let tools = tools
            .into_iter()
            .map(|tool| {
                let mut value = serde_json::to_value(tool).map_err(|_| {
                    ListError::Unavailable("the tool list could not be normalized".to_owned())
                })?;
                self.redact_value(&mut value);
                serde_json::from_value(value).map_err(|_| {
                    ListError::Unavailable(
                        "the redacted tool list could not be normalized".to_owned(),
                    )
                })
            })
            .collect::<Result<Vec<Tool>, ListError>>()?;
        // The buffer is rebuilt in one pass so every borrow handed out refers to it, not to the
        // SDK's values. `list_all_tools` has already resolved pagination, so no cursor is left
        // dangling here — the SDK's own `next_cursor` handling is what `P3-007` verified.
        buffer.adopt(tools);
        Ok(buffer.listings())
    }

    /// Closes the connection and waits for the service to stop.
    ///
    /// # Errors
    ///
    /// Returns the SDK's message when cancellation failed, as text: a caller deciding whether to log
    /// a shutdown failure does not need the SDK's error type to do it.
    pub async fn close(self) -> Result<(), String> {
        let redactor = self.redactor;
        let service = self
            .service
            .into_inner()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        match std::sync::Arc::try_unwrap(service) {
            Ok(service) => service.cancel().await.map(|_reason| ()).map_err(|error| {
                redactor
                    .read()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .text(&error.to_string())
            }),
            // A call still holds the service: stop it by token, which ends the child the same way.
            Err(shared) => {
                shared.cancellation_token().cancel();
                Ok(())
            }
        }
    }

    /// Calls one of the server's tools by its **server-side** name.
    ///
    /// `remote` is the name the server listed, not the canonical JARVIS identifier: `tools/call`
    /// must carry the name the server knows, and sending the canonical id would be the translation
    /// applied twice. A caller gets that name from `McpCatalog`'s routing, which is why the catalog
    /// keeps it beside the definition rather than discarding it.
    ///
    /// Uses the single-round request rather than the SDK's MRTR-driving helper. That is deliberate:
    /// the helper fulfils `input_required` rounds by calling a client `ClientHandler`, and this crate
    /// registers none — the only human answer in JARVIS comes through its own approval path, not a
    /// third-party server's form. A single round makes `input_required` and `task` **visible** as
    /// refusals (see [`CallError::InputRequired`] and [`CallError::Task`]) instead of leaving the SDK
    /// to fail on a missing handler.
    ///
    /// # Errors
    ///
    /// Returns [`CallError::Unavailable`] when the request did not complete,
    /// [`CallError::PeerError`] when the peer answered with a protocol error,
    /// [`CallError::InputRequired`] when the server needs an MRTR round this crate cannot supply, and
    /// [`CallError::Task`] when the server answered with a task this crate does not poll.
    pub async fn call_tool(
        &self,
        remote: &str,
        arguments: &serde_json::Map<String, Value>,
    ) -> Result<McpCallResult, CallError> {
        let params =
            CallToolRequestParams::new(remote.to_owned()).with_arguments(arguments.clone());
        // `call_tool_once` returns the union of complete/input_required/task. The last two are modes
        // this crate declared no capability for, so they are refused by name rather than silently
        // treated as a result — a server that returned `task` and got an empty success back would
        // make the caller believe work was finished that had not started.
        // Started again first if it had stopped. Failing here means nothing was sent.
        self.ensure_live().await.map_err(CallError::NotSent)?;
        let response = self.live().call_tool_once(params).await.map_err(|error| {
            redact_call_error(CallError::from_sdk(&error), |text| self.redact(text))
        })?;
        match response {
            CallToolResponse::Complete(result) => {
                let mut result = McpCallResult::from_sdk(&result);
                result.text = self.redact(&result.text);
                if let Some(structured) = &mut result.structured {
                    self.redact_value(structured);
                }
                Ok(result)
            }
            CallToolResponse::InputRequired(_) => Err(CallError::InputRequired),
            CallToolResponse::Task(_) => Err(CallError::Task),
            // `CallToolResponse` is `#[non_exhaustive]`, so an SDK bump can add a response kind. It
            // is refused rather than treated as complete: a new kind is by definition one whose
            // result this crate cannot describe, and defaulting it to success would hand the caller
            // an empty result for a call that may have done nothing or may not have finished.
            _ => Err(CallError::Unavailable(
                "the server answered the tool call with a response kind this client does not \
                 understand"
                    .to_owned(),
            )),
        }
    }
}

fn redact_connect_error(error: ConnectError, redactor: &CredentialRedactor) -> ConnectError {
    match error {
        ConnectError::Unreachable(text) => ConnectError::Unreachable(redactor.text(&text)),
        ConnectError::Refused(text) => ConnectError::Refused(redactor.text(&text)),
        other @ ConnectError::DiscoveryTimedOut { .. } => other,
    }
}

fn redact_call_error(error: CallError, redact: impl Fn(&str) -> String) -> CallError {
    match error {
        CallError::NotSent(text) => CallError::NotSent(redact(&text)),
        CallError::Unavailable(text) => CallError::Unavailable(redact(&text)),
        CallError::PeerError(text) => CallError::PeerError(redact(&text)),
        CallError::Undecodable(text) => CallError::Undecodable(redact(&text)),
        other @ (CallError::InputRequired | CallError::Task) => other,
    }
}

/// What a tool call produced, in JARVIS terms rather than the SDK's.
///
/// # The name, and the collision it avoids
///
/// `jarvis-tools` has a type called `ToolCallResult` too, and it is a different thing: that one is an
/// adapter's **established outcome** with its evidence and bounded output. This one is a **raw wire
/// result** — what the server sent, before any outcome decision. The two are deliberately separate
/// because the conversion between them is where the honesty rules live, and it is the MCP adapter's
/// job rather than the transport's: a transport that guessed an outcome would be guessing about a
/// provider it does not know.
///
/// The names are therefore kept distinct on purpose. `McpCallResult` names the protocol; a name shared
/// with the domain type would invite exactly the conflation the split exists to prevent.
///
/// # Why a tool that reports failure is not a `CallError`
///
/// The protocol carries two different kinds of bad news and they belong in different layers. A
/// **JSON-RPC error** means the request itself was refused — an unknown tool, a malformed argument
/// object — and no effect happened, so it is a [`CallError`]. A **`CallToolResult` with `isError`
/// set** means the tool *ran* and reported that it could not do what was asked, which is an outcome
/// and not a transport failure. Folding the second into the first would lose the distinction
/// `ToolOutcome` exists to make, and would make a tool's own honest refusal look like a broken
/// connection.
///
/// `structured_content` is surfaced separately from `text` because the protocol allows a tool to
/// return either, and a caller that wanted the structured object should not have to re-parse a
/// stringified copy of it.
#[derive(Clone, Debug, PartialEq)]
pub struct McpCallResult {
    /// The tool reported a failure. The call completed; the work did not.
    pub is_error: bool,
    /// The pictures the tool returned, decoded and bounded (`P11-011`): at most [`MAX_RESULT_IMAGES`], each at most [`MAX_IMAGE_BYTES`].
    pub images: Vec<McpImage>,
    /// The concatenated text blocks, which is what a model-facing tool result usually is.
    pub text: String,
    /// The structured object, when the server supplied one.
    pub structured: Option<Value>,
}

impl McpCallResult {
    /// Returns whether the tool reported success.
    #[must_use]
    pub fn is_success(&self) -> bool {
        !self.is_error
    }

    /// Converts an SDK result into JARVIS terms, dropping the SDK type.
    ///
    /// `is_error` is read as `false` when absent, because the protocol says an absent flag means the
    /// call succeeded — the same "absent means the default" rule `resultType` follows. Text is the
    /// concatenation of the text blocks: a result may carry images or embedded resources, and those
    /// are **not** flattened into text here. Doing so would invent a representation JARVIS has not
    /// decided on (an image belongs in the content pipeline, not in a string), so a non-text block
    /// contributes nothing to `text` rather than a placeholder like `[image]` that a model would
    /// then treat as data.
    fn from_sdk(result: &rmcp::model::CallToolResult) -> Self {
        let text = result
            .content
            .iter()
            .filter_map(|block| block.as_text())
            .map(|text| text.text.clone())
            .collect::<Vec<_>>()
            .join("\n");
        let images = result
            .content
            .iter()
            .filter_map(|block| block.as_image())
            .filter_map(|image| {
                // Encoded length first: a result that claims a huge picture is not decoded at all.
                if image.data.len() > MAX_IMAGE_BYTES * 4 / 3 + 8 {
                    return None;
                }
                let bytes = base64::Engine::decode(
                    &base64::engine::general_purpose::STANDARD,
                    image.data.trim(),
                )
                .ok()?;
                (bytes.len() <= MAX_IMAGE_BYTES).then(|| McpImage {
                    mime_type: image.mime_type.clone(),
                    bytes,
                })
            })
            .take(MAX_RESULT_IMAGES)
            .collect();
        Self {
            is_error: result.is_error.unwrap_or(false),
            images,
            text,
            structured: result.structured_content.clone(),
        }
    }
}

/// One picture a tool returned: what the server said it was, and its bytes. Nothing has checked the bytes yet.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct McpImage {
    /// The media type the server declared. Untrusted: the store decides from the bytes.
    pub mime_type: String,
    /// The decoded bytes.
    pub bytes: Vec<u8>,
}

/// The most pictures kept from one result.
pub const MAX_RESULT_IMAGES: usize = 4;

/// The largest picture kept, in bytes once decoded. A full-page screenshot is a few hundred kilobytes.
pub const MAX_IMAGE_BYTES: usize = 6 * 1024 * 1024;

/// Owns the wire form of a server's tool list so [`McpToolListing`] can borrow from it.
///
/// The SDK hands back `Tool` values owned by the caller. `McpToolListing` borrows its strings and
/// schemas, so something must own them for as long as the listings live. Putting that ownership in a
/// named buffer rather than leaking or cloning makes the lifetime explicit at the call site.
#[derive(Debug, Default)]
pub struct ToolBuffer {
    names: Vec<String>,
    titles: Vec<Option<String>>,
    descriptions: Vec<Option<String>>,
    input_schemas: Vec<Value>,
    output_schemas: Vec<Option<Value>>,
}

impl ToolBuffer {
    /// Creates an empty buffer.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns how many tools the buffer currently holds.
    #[must_use]
    pub fn len(&self) -> usize {
        self.names.len()
    }

    /// Returns whether the buffer holds no tools.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.names.is_empty()
    }

    /// Replaces the contents with a freshly read tool list.
    fn adopt(&mut self, tools: Vec<Tool>) {
        self.names.clear();
        self.titles.clear();
        self.descriptions.clear();
        self.input_schemas.clear();
        self.output_schemas.clear();
        for tool in tools {
            self.names.push(tool.name.into_owned());
            self.titles.push(tool.title);
            self.descriptions
                .push(tool.description.map(Cow::into_owned));
            self.input_schemas
                .push(Value::Object((*tool.input_schema).clone()));
            self.output_schemas.push(
                tool.output_schema
                    .map(|schema| Value::Object((*schema).clone())),
            );
        }
    }

    /// Returns listings that borrow from the buffer.
    ///
    /// Note what is **absent**: `annotations`. The SDK's `Tool` carries `readOnlyHint`,
    /// `destructiveHint`, and `idempotentHint`, and the specification requires clients to treat all
    /// three as untrusted. They are dropped here by construction, so no later change can consult
    /// them without adding a field to `McpToolListing` — which ADR-0025 forbids.
    fn listings(&self) -> Vec<McpToolListing<'_>> {
        (0..self.names.len())
            .map(|index| McpToolListing {
                name: &self.names[index],
                title: self.titles[index].as_deref(),
                description: self.descriptions[index].as_deref(),
                input_schema: &self.input_schemas[index],
                output_schema: self.output_schemas[index].as_ref(),
            })
            .collect()
    }
}

/// Classifies a tool-list failure without retaining the SDK's type.
fn classify_list(error: &ServiceError) -> ListError {
    match error {
        // A protocol error is a fact about the server that a retry will not change, so it is
        // reported separately from a peer that could not be reached.
        ServiceError::McpError(_) => ListError::PeerError(error.to_string()),
        _ => ListError::Unavailable(error.to_string()),
    }
}

/// Builds the client identity JARVIS advertises.
fn client_config() -> ClientConfig {
    // Elicitation, sampling, and roots are deliberately NOT declared. The first two are deprecated
    // by SEP-2577 and their spec-named migrations are already JARVIS's shape (the model provider is
    // called directly, and a human answers through JARVIS's own approval path, not through the
    // server). Declaring a capability JARVIS would answer with a stub tells the server to depend on
    // behaviour that does not exist.
    let capabilities = ClientCapabilities::default();
    let identity = Implementation::new(CLIENT_NAME, CLIENT_VERSION);
    ClientConfig::new(capabilities, identity)
}

/// Connects to a local MCP server over stdio, launching it as a child process.
///
/// # Errors
///
/// Returns [`ConnectError::Unreachable`] when the child cannot be spawned,
/// [`ConnectError::Refused`] when the negotiation fails at the protocol level, and
/// [`ConnectError::DiscoveryTimedOut`] when the peer completes neither handshake.
pub async fn connect_stdio(
    server: &ServerName,
    command: &StdioCommand,
) -> Result<McpConnection, ConnectError> {
    if command
        .credentials
        .references
        .values()
        .any(|reference| reference.server() != server.as_str())
    {
        return Err(ConnectError::Unreachable(
            "credential reference belongs to another server".to_owned(),
        ));
    }
    let (transport, redactor) = stdio_transport(command)?;
    let service = negotiate(transport)
        .await
        .map_err(|error| redact_connect_error(error, &redactor))?;
    let connection = McpConnection::new(server, service, Some(Recipe::Stdio(command.clone())));
    *connection
        .redactor
        .write()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = redactor;
    Ok(connection)
}

/// Builds the child-process transport for a command.
fn stdio_transport(
    command: &StdioCommand,
) -> Result<(rmcp::transport::TokioChildProcess, CredentialRedactor), ConnectError> {
    use rmcp::transport::TokioChildProcess;
    use tokio::process::Command;

    let environment = command.credentials.resolve()?;
    let mut redactor = CredentialRedactor::default();
    redactor.extend(environment.values().cloned());
    let mut child = Command::new(command.program());
    child.args(command.args());
    // **The server starts with an allowlisted environment, not the daemon's.** The first real server
    // tried (`server-everything`) has a `get-env` tool that returns every variable it was given, and
    // inheriting the daemon's environment would hand it, and through it the model, whatever key the
    // owner exported (found 2026-10-10). A server that needs a credential gets it by a deliberate route,
    // not by leaking.
    child.env_clear();
    child.envs(allowed_environment(std::env::vars_os()));
    child.envs(environment);
    // The SDK's convenience constructor inherits stderr even if Command configured it otherwise.
    // Credential-bearing children must not print their environment into the daemon's logs.
    let mut builder = TokioChildProcess::builder(child);
    if !command.credentials.references.is_empty() {
        builder = builder.stderr(std::process::Stdio::null());
    }
    let (transport, _) = builder
        .spawn()
        .map_err(|error| ConnectError::Unreachable(redactor.text(&error.to_string())))?;
    Ok((transport, redactor))
}

/// The variables a server is given: what a program needs to start and find its way around, and nothing
/// that is a credential. Compared without regard to case, because Windows names are case-insensitive.
///
/// The proxy variables are included because `npx` cannot download a package without them behind a
/// corporate proxy; they can carry a proxy login, which is the owner's own network configuration.
const INHERITED_ENVIRONMENT: &[&str] = &[
    "PATH",
    "PATHEXT",
    "SYSTEMROOT",
    "SYSTEMDRIVE",
    "WINDIR",
    "COMSPEC",
    "TEMP",
    "TMP",
    "TMPDIR",
    "HOME",
    "HOMEDRIVE",
    "HOMEPATH",
    "USERPROFILE",
    "APPDATA",
    "LOCALAPPDATA",
    "PROGRAMDATA",
    "PROGRAMFILES",
    "PROGRAMFILES(X86)",
    "PROGRAMW6432",
    "NUMBER_OF_PROCESSORS",
    "PROCESSOR_ARCHITECTURE",
    "OS",
    "USER",
    "USERNAME",
    "LOGNAME",
    "SHELL",
    "LANG",
    "LC_ALL",
    "LC_CTYPE",
    "TERM",
    "TZ",
    "XDG_CONFIG_HOME",
    "XDG_CACHE_HOME",
    "XDG_DATA_HOME",
    "HTTP_PROXY",
    "HTTPS_PROXY",
    "NO_PROXY",
    "ALL_PROXY",
    "SSL_CERT_FILE",
    "SSL_CERT_DIR",
    "NODE_EXTRA_CA_CERTS",
];

/// The environment a server is started with, taken from this process: [`INHERITED_ENVIRONMENT`] and nothing else.
///
/// Public so the one-time preparation of a program (downloading it before its first start) runs under the same
/// rule as the server itself.
#[must_use]
pub fn server_environment() -> Vec<(std::ffi::OsString, std::ffi::OsString)> {
    allowed_environment(std::env::vars_os())
}

/// Filters an environment down to [`INHERITED_ENVIRONMENT`].
pub(crate) fn allowed_environment<I>(variables: I) -> Vec<(std::ffi::OsString, std::ffi::OsString)>
where
    I: IntoIterator<Item = (std::ffi::OsString, std::ffi::OsString)>,
{
    variables
        .into_iter()
        .filter(|(name, _)| {
            let name = name.to_string_lossy().to_uppercase();
            INHERITED_ENVIRONMENT.contains(&name.as_str())
        })
        .collect()
}

/// Connects to a remote MCP server over Streamable HTTP.
///
/// Takes a **validated** [`McpHttpEndpoint`] rather than a string, so the scheme, credential, fragment,
/// and plaintext-remote rules are properties of the value instead of conventions this call site is
/// trusted to follow. `docs/architecture/security.md` lists SSRF and unsafe redirects as a threat with
/// a URL-policy mitigation; a `&str` satisfied none of it.
///
/// The HTTP client is built by [`build_http_client`] rather than by the SDK's own convenience
/// constructor, so that no-proxy, no-redirect, and TLS-only-off-loopback are **statements this crate
/// makes** rather than defaults inherited from a dependency's manifest. See `endpoint.rs` for why that
/// distinction matters: proxy support in the SDK's client is off today only because of a feature flag in
/// the SDK's `Cargo.toml`, which a feature unification elsewhere in the graph could change with nothing
/// here changing.
///
/// # Errors
///
/// As [`connect_stdio`], plus [`ConnectError::Unreachable`] when the HTTP client cannot be built.
pub async fn connect_http(
    server: &ServerName,
    endpoint: &McpHttpEndpoint,
) -> Result<McpConnection, ConnectError> {
    let transport = http_transport(endpoint)?;
    let service = negotiate(transport).await?;
    Ok(McpConnection::new(
        server,
        service,
        Some(Recipe::Http(endpoint.clone())),
    ))
}

/// Connects using a current token for every HTTP operation, including reconnect and cleanup.
///
/// Accepts a same-server `mcp:SERVER:KEY` reference or `oauth:SERVER` account bound to this query-free
/// endpoint, or the fixed `github` OAuth binding. The recipe stores the binding, not a reusable credential; every outbound
/// operation resolves again and a provider refusal prevents the HTTP request.
///
/// # Errors
///
/// As [`connect_http`], plus an opaque refusal for an invalid binding or unavailable credential.
pub async fn connect_http_with_auth(
    server: &ServerName,
    endpoint: &McpHttpEndpoint,
    binding: &str,
    provider: std::sync::Arc<dyn HttpCredentialProvider>,
) -> Result<McpConnection, ConnectError> {
    let client = AuthorizedHttpClient::new(server.as_str(), endpoint, binding, provider)?;
    connect_authorized_http(server, client).await
}

pub(crate) async fn connect_authorized_http(
    server: &ServerName,
    client: AuthorizedHttpClient,
) -> Result<McpConnection, ConnectError> {
    let service = negotiate(authorized_http_transport(&client))
        .await
        .map_err(|error| {
            let guard = client
                .redactor
                .read()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            redact_connect_error(error, &guard)
        })?;
    let mut connection = McpConnection::new(server, service, None);
    connection.redactor = std::sync::Arc::clone(&client.redactor);
    connection.recipe = Some(Recipe::HttpWithAuth(client));
    Ok(connection)
}

fn authorized_http_transport(
    client: &AuthorizedHttpClient,
) -> StreamableHttpClientTransport<AuthorizedHttpClient> {
    let config = StreamableHttpClientTransportConfig::with_uri(client.endpoint().to_owned());
    StreamableHttpClientTransport::with_client(client.clone(), config)
}

/// Builds the Streamable HTTP transport for an endpoint.
fn http_transport(
    endpoint: &McpHttpEndpoint,
) -> Result<StreamableHttpClientTransport<reqwest::Client>, ConnectError> {
    // No client-level total timeout: an MCP connection is long-lived and a total bound would kill a
    // healthy session. This is the same defect `docs/research/integrations/mcp.md` records for the
    // model client's stream, where a total `timeout` terminated every healthy stream at the deadline.
    let client = build_http_client(endpoint)?;
    let config = StreamableHttpClientTransportConfig::with_uri(endpoint.as_str().to_owned());
    Ok(StreamableHttpClientTransport::with_client(client, config))
}

/// Runs the negotiation over any transport the SDK supports.
///
/// Public so a test can drive the real negotiation over an in-process duplex pair: one peer that
/// answers `server/discover` and nothing else (it must never be sent `initialize`), and one that
/// refuses `server/discover` and answers only the legacy handshake (it must still connect, as
/// `Legacy`).
///
/// # Errors
///
/// Returns [`ConnectError::Unreachable`] or [`ConnectError::Refused`] when the transport or the
/// negotiation fails (both `server/discover` and the legacy `initialize`), and
/// [`ConnectError::DiscoveryTimedOut`] when the peer completes neither in time.
pub async fn connect_over<T, E, A>(
    server: &ServerName,
    transport: T,
) -> Result<McpConnection, ConnectError>
where
    T: rmcp::transport::IntoTransport<rmcp::RoleClient, E, A>,
    E: std::error::Error + Send + Sync + 'static,
{
    let service = negotiate(transport).await?;
    Ok(McpConnection::new(server, service, None))
}

/// Runs the handshake and checks the peer named a revision.
async fn negotiate<T, E, A>(transport: T) -> Result<Service, ConnectError>
where
    T: rmcp::transport::IntoTransport<rmcp::RoleClient, E, A>,
    E: std::error::Error + Send + Sync + 'static,
{
    let config = client_config();
    // Bounded as a whole. See `DISCOVERY_TIMEOUT`.
    let negotiation = config.serve_with_lifecycle(
        transport,
        // `Auto`: ask for the modern revision, and fall back to the legacy `initialize` handshake
        // when the peer reports it does not know `server/discover` (or is silent for ten seconds).
        // The era that resulted is read back and exposed as `McpConnection::era`, so the fallback
        // is a recorded fact rather than a silent substitution. `legacy_version: None` keeps the
        // SDK's own legacy version; the server answers with the one it supports.
        ClientLifecycleMode::Auto {
            preferred_versions: vec![modern_revision()],
            legacy_version: None,
        },
    );
    let service = tokio::time::timeout(DISCOVERY_TIMEOUT, negotiation)
        .await
        .map_err(|_elapsed| ConnectError::DiscoveryTimedOut {
            seconds: DISCOVERY_TIMEOUT.as_secs(),
        })?
        .map_err(|error| ConnectError::from_sdk(&error))?;

    // The server states which revision it settled on. A peer that reports none is refused rather than
    // assumed to be either era, so `McpConnection::era` is always a statement about something the
    // peer said.
    let negotiated = service.peer_info().map_or_else(String::new, |info| {
        info.protocol_version.as_str().to_owned()
    });
    if negotiated.is_empty() {
        let _ = service.cancel().await;
        return Err(ConnectError::Refused(
            "the server completed the handshake without naming a protocol revision".to_owned(),
        ));
    }

    Ok(service)
}

impl McpConnection {
    fn new(server: &ServerName, service: Service, recipe: Option<Recipe>) -> Self {
        Self {
            server: server.clone(),
            service: std::sync::RwLock::new(std::sync::Arc::new(service)),
            recipe,
            restart: tokio::sync::Mutex::new(None),
            redactor: std::sync::Arc::new(std::sync::RwLock::new(CredentialRedactor::default())),
        }
    }

    fn redact(&self, text: &str) -> String {
        self.redactor
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .text(text)
    }

    fn redact_value(&self, value: &mut Value) {
        self.redactor
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .value(value);
    }
}

/// A tool listing that owns its data, for a caller that cannot hold a buffer.
///
/// The borrowed [`McpToolListing`] is the right shape for translation, which is immediate. This
/// owned form exists for the one case where a caller must keep the list across an await point and
/// cannot borrow.
#[derive(Clone, Debug, PartialEq)]
pub struct OwnedListing {
    /// The server's own tool name.
    pub name: String,
    /// The optional display title.
    pub title: Option<String>,
    /// The optional model-facing description.
    pub description: Option<String>,
    /// The input schema.
    pub input_schema: Value,
    /// The optional output schema.
    pub output_schema: Option<Value>,
}

impl From<&McpToolListing<'_>> for OwnedListing {
    fn from(listing: &McpToolListing<'_>) -> Self {
        Self {
            name: listing.name.to_owned(),
            title: listing.title.map(str::to_owned),
            description: listing.description.map(str::to_owned),
            input_schema: listing.input_schema.clone(),
            output_schema: listing.output_schema.cloned(),
        }
    }
}

impl OwnedListing {
    /// Borrows this listing in the shape the translator expects.
    #[must_use]
    pub fn as_listing(&self) -> McpToolListing<'_> {
        McpToolListing {
            name: &self.name,
            title: self.title.as_deref(),
            description: self.description.as_deref(),
            input_schema: &self.input_schema,
            output_schema: self.output_schema.as_ref(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn server(name: &str) -> ServerName {
        ServerName::new(name).unwrap_or_else(|error| panic!("fixture server {name}: {error}"))
    }

    fn sdk_result(blocks: &[serde_json::Value]) -> rmcp::model::CallToolResult {
        serde_json::from_value(json!({ "content": blocks }))
            .unwrap_or_else(|error| panic!("fixture result: {error}"))
    }

    fn image_block(bytes: &[u8]) -> serde_json::Value {
        json!({
            "type": "image",
            "data": base64::Engine::encode(&base64::engine::general_purpose::STANDARD, bytes),
            "mimeType": "image/png",
        })
    }

    /// A picture comes out decoded; text and picture arrive together; broken or oversized ones are dropped
    /// and the rest of the result is untouched (`P11-011`).
    #[test]
    fn a_result_keeps_its_pictures_decoded_and_bounded() {
        let result = McpCallResult::from_sdk(&sdk_result(&[
            json!({ "type": "text", "text": "Took a screenshot" }),
            image_block(&[1, 2, 3]),
            json!({ "type": "image", "data": "!!not base64!!", "mimeType": "image/png" }),
            image_block(&vec![0; MAX_IMAGE_BYTES + 1]),
        ]));
        assert_eq!(result.text, "Took a screenshot");
        assert_eq!(result.images.len(), 1);
        assert_eq!(result.images[0].bytes, vec![1, 2, 3]);

        let many: Vec<_> = (0..MAX_RESULT_IMAGES + 3)
            .map(|_| image_block(&[9]))
            .collect();
        assert_eq!(
            McpCallResult::from_sdk(&sdk_result(&many)).images.len(),
            MAX_RESULT_IMAGES
        );
    }

    /// **A credential in the daemon's environment never reaches a server.** The allowlist keeps what a
    /// program needs to start and drops everything else, whatever the variable is called.
    #[test]
    fn a_server_is_not_given_the_daemons_secrets() {
        let environment = [
            ("PATH", "/usr/bin"),
            ("Path", "C:\\Windows"),
            ("SystemRoot", "C:\\Windows"),
            ("ELEVENLABS_API_KEY", "secret-1"),
            ("OPENAI_API_KEY", "secret-2"),
            ("GITHUB_TOKEN", "secret-3"),
            ("JARVIS_ANYTHING", "secret-4"),
            ("AWS_SECRET_ACCESS_KEY", "secret-5"),
        ]
        .map(|(name, value)| (name.into(), value.into()));
        let kept = allowed_environment(environment);
        let names: Vec<String> = kept
            .iter()
            .map(|(name, _)| name.to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, ["PATH", "Path", "SystemRoot"], "{names:?}");
        assert!(
            kept.iter()
                .all(|(_, value)| !value.to_string_lossy().starts_with("secret")),
            "no secret value may be passed on"
        );
    }

    /// **The property this whole module is arranged around.** A server's `annotations` claim what its
    /// tools may do — `readOnlyHint`, `destructiveHint`, `idempotentHint` — and the specification
    /// requires clients to treat them as untrusted, while JARVIS policy treats them as exactly the
    /// facts it must not accept from the server. This asserts the translation has no field to put
    /// them in, so the omission is structural rather than a convention someone could forget.
    #[test]
    fn a_tool_listing_carries_no_annotation_field() {
        let buffer = ToolBuffer {
            names: vec!["search".to_owned()],
            titles: vec![None],
            descriptions: vec![None],
            input_schemas: vec![json!({ "type": "object" })],
            output_schemas: vec![None],
        };
        let listings = buffer.listings();
        assert_eq!(listings.len(), 1);
        let serialised = serde_json::to_value(listings[0].name)
            .unwrap_or_else(|error| panic!("fixture name is a string: {error}"));
        assert!(serialised.is_string());
        // The listing exposes exactly five fields; there is no sixth to carry a hint. If someone
        // adds one, this test fails and the reviewer re-reads ADR-0025.
        let listing = &listings[0];
        assert_eq!(listing.name, "search");
        assert!(listing.title.is_none());
        assert!(listing.description.is_none());
        assert!(listing.output_schema.is_none());
    }

    /// An empty program is refused rather than handed to the OS, whose error would name neither the
    /// server nor the configuration that produced it.
    #[test]
    fn a_command_without_a_program_is_refused() {
        let error = StdioCommand::new("   ", &[])
            .err()
            .unwrap_or_else(|| panic!("a blank program must be refused"));
        assert!(error.to_string().contains("needs a program"), "{error}");
    }

    /// The arguments are kept in order and unmodified: a client that reordered or quoted them would
    /// change which command runs.
    #[test]
    fn a_command_preserves_its_arguments_exactly() {
        let command = StdioCommand::new("npx", &["-y", "@modelcontextprotocol/server-everything"])
            .unwrap_or_else(|error| panic!("fixture command: {error}"));
        assert_eq!(command.program(), "npx");
        assert_eq!(
            command.args(),
            ["-y", "@modelcontextprotocol/server-everything"]
        );
        // A program name with surrounding whitespace is trimmed, because a value copied out of a
        // config file routinely carries a trailing newline and an untrimmed value fails to spawn
        // with a message about a missing file.
        let padded = StdioCommand::new("  npx  ", &[])
            .unwrap_or_else(|error| panic!("fixture command: {error}"));
        assert_eq!(padded.program(), "npx");
    }

    /// **Two listings for one server must remain distinguishable**, because `P3-008d` keys the
    /// catalog's collision check on `(server, tool)` and the identity drift on the server name. A
    /// connection that lost its server name would silently make every listing look like it came from
    /// the same place.
    #[test]
    fn a_buffered_listing_stays_borrowed_and_ordered() {
        let buffer = ToolBuffer {
            names: vec!["b".to_owned(), "a".to_owned()],
            titles: vec![Some("B".to_owned()), None],
            descriptions: vec![None, Some("does a".to_owned())],
            input_schemas: vec![json!({"type": "object"}), json!({"type": "object"})],
            output_schemas: vec![Some(json!({"type": "object"})), None],
        };
        let listings = buffer.listings();
        assert_eq!(listings.len(), 2);
        // Order is the wire order, not sorted: the catalog sorts by canonical identifier, and
        // sorting here would hide a server that returns an unstable order, which the specification
        // requires it not to do.
        assert_eq!(listings[0].name, "b");
        assert_eq!(listings[1].name, "a");
        assert_eq!(listings[0].title, Some("B"));
        assert_eq!(listings[1].description, Some("does a"));
        assert!(listings[0].output_schema.is_some());
        assert!(listings[1].output_schema.is_none());

        let owned = OwnedListing::from(&listings[0]);
        let borrowed = owned.as_listing();
        assert_eq!(borrowed.name, "b");
        assert_eq!(borrowed.title, Some("B"));
        assert_eq!(borrowed.input_schema, &json!({"type": "object"}));
        assert_eq!(owned.name, buffer.listings()[0].name);
    }

    /// `adopt` replaces rather than appends. A refresh that appended would double every tool on each
    /// poll, and `P3-008d`'s drift check compares two builds of the same server.
    #[test]
    fn adopting_a_list_replaces_the_previous_one() {
        let mut buffer = ToolBuffer::new();
        assert!(buffer.is_empty());
        buffer.adopt(Vec::new());
        assert!(buffer.is_empty());
    }

    /// The server name a connection carries is the operator's, never the server's own claim — the
    /// rule ADR-0024 exists to hold, asserted at the transport boundary so a future refactor cannot
    /// quietly start using the reported name as the identifier.
    #[test]
    fn a_connection_is_named_by_the_operator_not_the_server() {
        let operator_name = server("acme-mcp");
        assert_eq!(operator_name.as_str(), "acme-mcp");
        // The reported identity is separate evidence, and a server may claim anything.
        let claimed = ReportedIdentity::new("totally-different", Some("Totally Different"));
        assert_ne!(claimed.name, operator_name.as_str());
        assert!(
            !claimed.agrees_with(&ReportedIdentity::new("acme-mcp", None)),
            "a server that renamed itself must not agree with the operator's record"
        );
    }
}
