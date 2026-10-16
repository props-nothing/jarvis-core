//! A client for the Agent Client Protocol (ACP), version 1, over a child process (`docs/research/integrations/acp.md`).
//!
//! JSON-RPC 2.0, one message per line, on the child's standard input and output. The client:
//!
//! 1. sends `initialize` for protocol version **1** and refuses an agent that answers with another version, by number;
//! 2. advertises **no** client capabilities, so the agent uses its own tools in the working directory it was given and asks
//!    permission through the one method every agent must support;
//! 3. creates a session in a working directory and sends one prompt;
//! 4. follows the streamed updates into a bounded [`Report`], and answers every `session/request_permission` through a
//!    [`PermissionPolicy`] (never `allow_always`: remembering a choice would widen it);
//! 5. cancels on request or at the time limit, answering pending permission requests with `cancelled` first, and gives the agent a
//!    short grace period to end the turn before reporting the end itself.
//!
//! # What it does not trust
//!
//! The agent is a program somebody else wrote. A line over the size limit, too many lines that are not JSON, a request for a method
//! that was not advertised, a hang, a crash and a wrong protocol version all end in a [`Report`] or an [`AcpError`] naming the
//! cause; none of them blocks the caller or goes unrecorded. Everything the agent says is returned as data.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use serde_json::{Value, json};
use thiserror::Error;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout};
use tokio::sync::{Notify, mpsc};
use tokio::time::Instant;

/// The only protocol version spoken. Version 2 is a draft; an agent that answers with it is refused.
pub const PROTOCOL_VERSION: u64 = 1;

/// How much of the agent's standard error is kept, for the error shown when it fails to start.
const STDERR_TAIL_BYTES: usize = 4096;

/// What bounds a connection.
#[derive(Clone, Debug)]
pub struct Limits {
    /// The longest line accepted from the agent, in bytes.
    pub max_line_bytes: usize,
    /// How many lines that are not JSON objects are tolerated before the agent is declared not to speak ACP.
    pub max_stray_lines: u32,
    /// How many tool calls are noted in the report.
    pub max_tool_notes: usize,
    /// How many characters of the agent's own words are kept (the end, where the summary is).
    pub max_message_chars: usize,
    /// How long `initialize` and `session/new` may take together. An agent started through a package runner may be downloaded
    /// first, so this is generous.
    pub handshake: Duration,
    /// How long a prompt may run before it is cancelled. Zero means no overall deadline.
    pub turn: Duration,
    /// How long an agent is given to end a cancelled turn before the client reports the end itself.
    pub cancel_grace: Duration,
    /// How long one write to the agent may block.
    pub write: Duration,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_line_bytes: 2 * 1024 * 1024,
            max_stray_lines: 20,
            max_tool_notes: 200,
            max_message_chars: 6_000,
            handshake: Duration::from_mins(3),
            turn: Duration::ZERO,
            cancel_grace: Duration::from_secs(10),
            write: Duration::from_secs(15),
        }
    }
}

/// Why a connection could not be opened.
#[derive(Debug, Error)]
pub enum AcpError {
    /// The child was not started with piped standard input, output and error.
    #[error("the agent process was not started with piped standard streams")]
    NotPiped,
    /// The agent answered `initialize` with a protocol version this client does not speak.
    #[error("the agent speaks ACP version {offered}, and JARVIS speaks version {PROTOCOL_VERSION}")]
    UnsupportedVersion {
        /// The version the agent offered.
        offered: u64,
    },
    /// The agent needs the owner to sign in first (error `-32000`).
    #[error("the agent needs you to sign in first: {message}")]
    AuthRequired {
        /// The agent's own words.
        message: String,
        /// The sign-in methods the agent offers (empty when it offers none, which means signing in outside JARVIS).
        methods: Vec<AuthMethod>,
    },
    /// The method asked for is not one the agent offers.
    #[error("the agent does not offer the sign-in method {asked}")]
    UnknownAuthMethod {
        /// The method that was asked for.
        asked: String,
        /// The methods the agent does offer.
        methods: Vec<AuthMethod>,
    },
    /// The agent refused a handshake request.
    #[error("the agent refused {method} (code {code}): {message}")]
    Rejected {
        /// The method refused.
        method: &'static str,
        /// The JSON-RPC error code.
        code: i64,
        /// The agent's own words.
        message: String,
    },
    /// The agent ended, or stopped answering, during the handshake.
    #[error("the agent did not complete the handshake: {reason}")]
    Handshake {
        /// What happened, in JARVIS's words.
        reason: String,
        /// The last of what the agent wrote on its error stream, which is where a missing sign-in or package is usually said.
        stderr_tail: String,
    },
}

/// A way the agent offers to sign in. Only the kind the agent handles itself (`authenticate`) is kept: JARVIS does not offer the
/// terminal kind, so an agent does not advertise it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AuthMethod {
    /// The agent's identifier for the method.
    pub id: String,
    /// What the agent calls it.
    pub name: String,
    /// The agent's description of it, when it gave one.
    pub description: String,
}

/// How an option offered with a permission request is to be read.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OptionKind {
    /// Allow this once.
    AllowOnce,
    /// Allow and remember. Never chosen by this client.
    AllowAlways,
    /// Reject this once.
    RejectOnce,
    /// Reject and remember.
    RejectAlways,
    /// A kind this client does not know.
    Unknown,
}

/// One option of a permission request.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PermissionOption {
    /// The agent's identifier for the option.
    pub id: String,
    /// The option's label.
    pub name: String,
    /// What the option means.
    pub kind: OptionKind,
}

/// What an agent asks leave to do.
#[derive(Clone, Debug, PartialEq)]
pub struct PermissionRequest {
    /// The agent's identifier for the tool call.
    pub tool_call_id: String,
    /// The agent's description of it.
    pub title: String,
    /// The kind of tool: `read`, `edit`, `delete`, `move`, `search`, `execute`, `think`, `fetch`, `switch_mode` or `other`. Empty when
    /// the agent said nothing.
    pub kind: String,
    /// The file paths the call says it touches (`locations`), as the agent wrote them.
    pub paths: Vec<String>,
    /// The raw input of the call, as the agent sent it.
    pub raw_input: Option<Value>,
    /// What the agent offered.
    pub options: Vec<PermissionOption>,
}

/// A policy's answer.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Decision {
    /// Whether to allow this one call.
    pub allow: bool,
    /// Why, in JARVIS's words, for the report.
    pub reason: String,
}

/// Decides what to do with an agent's permission requests.
#[async_trait]
pub trait PermissionPolicy: Send + Sync {
    /// Decides one request. Deterministic policy only: a model never answers this.
    async fn decide(&self, request: &PermissionRequest) -> Decision;
}

/// Why a turn ended.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StopReason {
    /// The agent finished.
    EndTurn,
    /// The agent ran out of tokens.
    MaxTokens,
    /// The agent reached its limit of requests between user turns.
    MaxTurnRequests,
    /// The agent refused to continue.
    Refusal,
    /// The turn was cancelled.
    Cancelled,
    /// A reason this client does not know, as the agent wrote it.
    Other(String),
}

impl StopReason {
    fn from_wire(text: &str) -> Self {
        match text {
            "end_turn" => Self::EndTurn,
            "max_tokens" => Self::MaxTokens,
            "max_turn_requests" => Self::MaxTurnRequests,
            "refusal" => Self::Refusal,
            "cancelled" => Self::Cancelled,
            other => Self::Other(other.chars().take(40).collect()),
        }
    }

    /// The reason as a word.
    #[must_use]
    pub fn as_str(&self) -> &str {
        match self {
            Self::EndTurn => "end_turn",
            Self::MaxTokens => "max_tokens",
            Self::MaxTurnRequests => "max_turn_requests",
            Self::Refusal => "refusal",
            Self::Cancelled => "cancelled",
            Self::Other(other) => other,
        }
    }
}

/// How a prompt ended.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Ended {
    /// The agent ended the turn itself.
    Finished(StopReason),
    /// The caller cancelled it.
    Cancelled,
    /// The time limit passed and the turn was cancelled.
    TimedOut,
    /// The agent's process ended before the turn did (the exit code, when known).
    AgentExited(Option<i32>),
    /// The agent broke the protocol or failed in a way it reported.
    Failed(String),
}

/// One tool call the agent reported.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ToolNote {
    /// The agent's identifier for the call.
    pub id: String,
    /// What it said the call was.
    pub title: String,
    /// The kind of tool.
    pub kind: String,
    /// The last status: `pending`, `in_progress`, `completed` or `failed`.
    pub status: String,
    /// Up to four file paths reported by the agent, not independently verified.
    pub paths: Vec<String>,
}

/// A bounded public tool snapshot observed before the prompt ends. This never contains internal reasoning or raw tool payloads.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ToolActivity {
    /// Increasing position within this prompt; gaps indicate updates the observer could not receive.
    pub sequence: u64,
    /// The merged tool metadata, with omitted update fields preserved.
    pub tool: ToolNote,
}

/// A permission request and what was decided.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PermissionRecord {
    /// What the agent asked to do.
    pub title: String,
    /// The kind of tool.
    pub kind: String,
    /// Whether it was allowed.
    pub allowed: bool,
    /// Why.
    pub reason: String,
}

/// What the agent said about itself.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct AgentInfo {
    /// Its name.
    pub name: String,
    /// Its version.
    pub version: String,
}

/// What happened during one prompt. Everything the agent wrote is untrusted data.
#[derive(Clone, Debug, PartialEq)]
pub struct Report {
    /// How the turn ended.
    pub ended: Ended,
    /// The end of what the agent said in its own words.
    pub message: String,
    /// Whether the start of the message was dropped to keep it within bounds.
    pub message_cut: bool,
    /// The tool calls the agent reported, in order (up to the limit).
    pub tools: Vec<ToolNote>,
    /// Tool calls not noted because of the limit.
    pub tools_dropped: u32,
    /// Every permission request and what was decided.
    pub permissions: Vec<PermissionRecord>,
    /// How many `session/update` notifications arrived.
    pub updates: u32,
    /// Public tool snapshots observed in this prompt.
    pub activity_updates: u64,
    /// Snapshots dropped because the bounded observer queue was full or closed.
    pub activity_dropped: u64,
    /// How many lines that were not protocol messages the agent wrote to its output.
    pub stray_lines: u32,
}

impl Report {
    fn new() -> Self {
        Self {
            ended: Ended::Failed("the turn did not start".to_owned()),
            message: String::new(),
            message_cut: false,
            tools: Vec::new(),
            tools_dropped: 0,
            permissions: Vec::new(),
            updates: 0,
            activity_updates: 0,
            activity_dropped: 0,
            stray_lines: 0,
        }
    }
}

/// An open connection to an agent, with a session created.
pub struct Connection {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    line: Vec<u8>,
    next_id: u64,
    limits: Limits,
    stderr_tail: Arc<Mutex<VecDeque<u8>>>,
    stray: u32,
    session_id: String,
    agent: AgentInfo,
    auth_methods: Vec<AuthMethod>,
    signed_in: bool,
    /// The method to sign in with if the agent asks during a prompt.
    sign_in: Option<String>,
    activity_sender: Option<mpsc::Sender<ToolActivity>>,
}

/// Why waiting for a response ended without one.
enum Stop {
    /// The agent answered with an error.
    Rpc { code: i64, message: String },
    /// The wait ended some other way.
    Ended(Ended),
}

/// What a wait is for.
struct Wait<'a> {
    id: u64,
    policy: Option<&'a dyn PermissionPolicy>,
    cancel: Option<&'a Notify>,
    deadline: Option<Instant>,
}

impl Connection {
    /// Completes the handshake with an agent already started, and creates a session in `cwd`.
    ///
    /// The child must have been started with piped standard input, output and error. Dropping the connection does not kill the child;
    /// the caller that started it decides how it is killed (the daemon starts it so that the whole process tree goes).
    ///
    /// `sign_in` names a sign-in method (as the agent advertised it) to use **only if** the agent says it needs one, whether when the
    /// session is made or at the first prompt (some agents only check then): the agent gets `authenticate` with that method and the
    /// request is made again once. With `None` an agent that needs a sign-in ends the handshake with [`AcpError::AuthRequired`], which
    /// lists what the agent offers. Signing in can take a person minutes (a browser page), so the caller sets [`Limits::handshake`] for it.
    ///
    /// # Errors
    ///
    /// Returns [`AcpError`] when the streams are not piped, the agent speaks another version, needs a sign-in, refuses, ends or stops
    /// answering within [`Limits::handshake`].
    pub async fn open(
        mut child: Child,
        cwd: &std::path::Path,
        limits: Limits,
        sign_in: Option<&str>,
    ) -> Result<Self, AcpError> {
        let (Some(stdin), Some(stdout), Some(stderr)) =
            (child.stdin.take(), child.stdout.take(), child.stderr.take())
        else {
            return Err(AcpError::NotPiped);
        };
        let stderr_tail = Arc::new(Mutex::new(VecDeque::new()));
        drain_stderr(stderr, Arc::clone(&stderr_tail));
        let mut connection = Self {
            child,
            stdin,
            stdout: BufReader::new(stdout),
            line: Vec::new(),
            next_id: 0,
            limits,
            stderr_tail,
            stray: 0,
            session_id: String::new(),
            agent: AgentInfo::default(),
            auth_methods: Vec::new(),
            signed_in: false,
            sign_in: sign_in.map(str::to_owned),
            activity_sender: None,
        };
        let deadline = Instant::now() + connection.limits.handshake;
        connection.handshake(cwd, deadline, sign_in).await?;
        Ok(connection)
    }

    /// What the agent said about itself.
    #[must_use]
    pub const fn agent(&self) -> &AgentInfo {
        &self.agent
    }

    /// The sign-in methods the agent advertised.
    #[must_use]
    pub fn auth_methods(&self) -> &[AuthMethod] {
        &self.auth_methods
    }

    /// Whether this handshake had to sign in (the agent asked, and `authenticate` was sent and accepted).
    #[must_use]
    pub const fn signed_in(&self) -> bool {
        self.signed_in
    }

    /// Observes public tool activity through a bounded queue. A slow or closed observer never blocks protocol handling.
    pub fn set_activity_sender(&mut self, sender: mpsc::Sender<ToolActivity>) {
        self.activity_sender = Some(sender);
    }

    /// Signs in now with one of the methods the agent advertised, whether or not it has asked: the owner pressed Sign in. The agent's own
    /// flow runs (it may open a browser page) and the call returns when it ends, within [`Limits::handshake`].
    ///
    /// # Errors
    ///
    /// Returns [`AcpError::UnknownAuthMethod`] for a method the agent did not advertise, and [`AcpError`] when the agent refuses or
    /// stops answering.
    pub async fn authenticate(&mut self, method: &str) -> Result<(), AcpError> {
        if !self.auth_methods.iter().any(|offered| offered.id == method) {
            return Err(AcpError::UnknownAuthMethod {
                asked: method.chars().take(80).collect(),
                methods: self.auth_methods.clone(),
            });
        }
        let deadline = Instant::now() + self.limits.handshake;
        self.call("authenticate", json!({ "methodId": method }), deadline)
            .await
            .map_err(|stop| self.handshake_error("authenticate", stop))?;
        self.signed_in = true;
        Ok(())
    }

    /// The agent process's identifier, while it runs.
    #[must_use]
    pub fn pid(&self) -> Option<u32> {
        self.child.id()
    }

    /// The last of what the agent wrote on its error stream.
    #[must_use]
    pub fn stderr_tail(&self) -> String {
        tail_text(&self.stderr_tail)
    }

    async fn handshake(
        &mut self,
        cwd: &std::path::Path,
        deadline: Instant,
        sign_in: Option<&str>,
    ) -> Result<(), AcpError> {
        let initialize = json!({
            "protocolVersion": PROTOCOL_VERSION,
            "clientCapabilities": {},
            "clientInfo": { "name": "jarvis", "title": "JARVIS", "version": env!("CARGO_PKG_VERSION") },
        });
        let result = self
            .call("initialize", initialize, deadline)
            .await
            .map_err(|stop| self.handshake_error("initialize", stop))?;
        let offered = result
            .get("protocolVersion")
            .and_then(Value::as_u64)
            .ok_or_else(|| AcpError::Handshake {
                reason: "the answer to initialize named no protocol version".to_owned(),
                stderr_tail: self.stderr_tail(),
            })?;
        if offered != PROTOCOL_VERSION {
            return Err(AcpError::UnsupportedVersion { offered });
        }
        if let Some(info) = result.get("agentInfo") {
            self.agent = AgentInfo {
                name: text_of(info.get("name"), 80),
                version: text_of(info.get("version"), 40),
            };
        }
        self.auth_methods = parse_auth_methods(&result);
        let new_session = json!({ "cwd": cwd.to_string_lossy(), "mcpServers": [] });
        let mut attempt = self
            .call("session/new", new_session.clone(), deadline)
            .await;
        if let (Err(Stop::Rpc { code: -32000, .. }), Some(method)) = (&attempt, sign_in) {
            if !self.auth_methods.iter().any(|offered| offered.id == method) {
                return Err(AcpError::UnknownAuthMethod {
                    asked: method.chars().take(80).collect(),
                    methods: self.auth_methods.clone(),
                });
            }
            self.call("authenticate", json!({ "methodId": method }), deadline)
                .await
                .map_err(|stop| self.handshake_error("authenticate", stop))?;
            self.signed_in = true;
            attempt = self.call("session/new", new_session, deadline).await;
        }
        let session = attempt.map_err(|stop| self.handshake_error("session/new", stop))?;
        let id = session
            .get("sessionId")
            .and_then(Value::as_str)
            .filter(|id| !id.is_empty() && id.len() <= 256)
            .ok_or_else(|| AcpError::Handshake {
                reason: "the answer to session/new named no session".to_owned(),
                stderr_tail: self.stderr_tail(),
            })?;
        id.clone_into(&mut self.session_id);
        Ok(())
    }

    fn handshake_error(&self, method: &'static str, stop: Stop) -> AcpError {
        match stop {
            Stop::Rpc {
                code: -32000,
                message,
            } => AcpError::AuthRequired {
                message,
                methods: self.auth_methods.clone(),
            },
            Stop::Rpc { code, message } => AcpError::Rejected {
                method,
                code,
                message,
            },
            Stop::Ended(ended) => AcpError::Handshake {
                reason: match ended {
                    Ended::TimedOut | Ended::Cancelled => {
                        format!("{method} took too long")
                    }
                    Ended::AgentExited(code) => format!(
                        "the agent ended during {method}{}",
                        code.map_or_else(String::new, |code| format!(" (exit code {code})"))
                    ),
                    Ended::Failed(reason) => reason,
                    Ended::Finished(_) => "the turn ended during the handshake".to_owned(),
                },
                stderr_tail: self.stderr_tail(),
            },
        }
    }

    /// A request answered while nothing may be asked of the owner: the handshake. Requests from the agent are refused.
    async fn call(
        &mut self,
        method: &str,
        params: Value,
        deadline: Instant,
    ) -> Result<Value, Stop> {
        let id = self.send_request(method, params).await?;
        let mut report = Report::new();
        self.wait(
            Wait {
                id,
                policy: None,
                cancel: None,
                deadline: Some(deadline),
            },
            &mut report,
        )
        .await
    }

    /// Sends one prompt and follows the turn to its end.
    ///
    /// `cancel` ends the turn early (a permit is kept, so cancelling just before the call counts). The turn also ends at
    /// [`Limits::turn`]. Either way the agent is sent `session/cancel`, pending permission requests are answered `cancelled`, and
    /// [`Limits::cancel_grace`] is allowed for the agent to end the turn itself.
    pub async fn prompt(
        &mut self,
        text: &str,
        policy: &dyn PermissionPolicy,
        cancel: &Notify,
    ) -> Report {
        let mut report = Report::new();
        let params = json!({
            "sessionId": self.session_id,
            "prompt": [{ "type": "text", "text": text }],
        });
        let mut attempt = 0;
        let outcome = loop {
            let id = match self.send_request("session/prompt", params.clone()).await {
                Ok(id) => id,
                Err(stop) => {
                    report.ended = ended_of(stop);
                    return report;
                }
            };
            let deadline = (!self.limits.turn.is_zero()).then(|| Instant::now() + self.limits.turn);
            let outcome = self
                .wait(
                    Wait {
                        id,
                        policy: Some(policy),
                        cancel: Some(cancel),
                        deadline,
                    },
                    &mut report,
                )
                .await;
            // Some agents only check the sign-in when the first prompt arrives. With a method chosen, sign in once and ask again.
            if attempt == 0
                && matches!(&outcome, Err(Stop::Rpc { code: -32000, .. }))
                && let Some(method) = self.sign_in.clone()
                && self.authenticate(&method).await.is_ok()
            {
                attempt = 1;
                continue;
            }
            break outcome;
        };
        report.stray_lines = self.stray;
        match outcome {
            Ok(result) => {
                let reason = result
                    .get("stopReason")
                    .and_then(Value::as_str)
                    .map_or(StopReason::Other("none".to_owned()), StopReason::from_wire);
                // A turn the client cancelled or timed out is reported as that, whatever word the agent used.
                if matches!(report.ended, Ended::Cancelled | Ended::TimedOut) {
                    return report;
                }
                report.ended = Ended::Finished(reason);
            }
            Err(stop) => report.ended = ended_of(stop),
        }
        report
    }

    /// Ends the connection: closes the agent's input, gives it a moment to leave, and kills it if it stays.
    pub async fn close(mut self) {
        drop(self.stdin);
        if tokio::time::timeout(Duration::from_secs(2), self.child.wait())
            .await
            .is_err()
        {
            let _ = self.child.start_kill();
            let _ = tokio::time::timeout(Duration::from_secs(3), self.child.wait()).await;
        }
    }

    async fn send_request(&mut self, method: &str, params: Value) -> Result<u64, Stop> {
        self.next_id += 1;
        let id = self.next_id;
        self.send(&json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }))
            .await?;
        Ok(id)
    }

    async fn send(&mut self, message: &Value) -> Result<(), Stop> {
        // `serde_json` writes compact text with the newlines inside strings escaped, so one message is one line.
        let mut bytes = message.to_string().into_bytes();
        bytes.push(b'\n');
        let write = async {
            self.stdin.write_all(&bytes).await?;
            self.stdin.flush().await
        };
        match tokio::time::timeout(self.limits.write, write).await {
            Ok(Ok(())) => Ok(()),
            Ok(Err(_)) => Err(Stop::Ended(self.exited().await)),
            Err(_) => Err(Stop::Ended(Ended::Failed(
                "the agent stopped reading its input".to_owned(),
            ))),
        }
    }

    /// How the process ended, once its output has closed.
    async fn exited(&mut self) -> Ended {
        match tokio::time::timeout(Duration::from_secs(2), self.child.wait()).await {
            Ok(Ok(status)) => Ended::AgentExited(status.code()),
            _ => Ended::AgentExited(None),
        }
    }

    /// Reads messages until the response to `wait.id`, answering what the agent asks and noting what it reports.
    async fn wait(&mut self, wait: Wait<'_>, report: &mut Report) -> Result<Value, Stop> {
        let mut deadline = wait.deadline;
        let mut winding_down: Option<Ended> = None;
        loop {
            let message = tokio::select! {
                biased;
                () = async { if let Some(cancel) = wait.cancel { cancel.notified().await } else { std::future::pending().await } },
                    if winding_down.is_none() =>
                {
                    self.begin_cancel(Ended::Cancelled, &mut winding_down, &mut deadline, report).await;
                    continue;
                }
                () = async {
                    if let Some(deadline) = deadline {
                        tokio::time::sleep_until(deadline).await;
                    } else {
                        std::future::pending::<()>().await;
                    }
                } => {
                    if winding_down.is_none() && wait.cancel.is_some() {
                        self.begin_cancel(Ended::TimedOut, &mut winding_down, &mut deadline, report).await;
                        continue;
                    }
                    let ended = winding_down.unwrap_or(Ended::TimedOut);
                    return Err(Stop::Ended(ended));
                }
                read = self.read_message() => read,
            };
            let message = match message {
                Ok(Some(message)) => message,
                Ok(None) => return Err(Stop::Ended(self.exited().await)),
                Err(reason) => return Err(Stop::Ended(Ended::Failed(reason))),
            };
            report.stray_lines = self.stray;
            let method = message.get("method").and_then(Value::as_str);
            let id = message.get("id");
            match (method, id) {
                (Some(method), Some(request_id)) => {
                    let request_id = request_id.clone();
                    self.answer_request(
                        method,
                        &request_id,
                        &message,
                        wait.policy,
                        winding_down.is_some(),
                        report,
                    )
                    .await?;
                }
                (Some("session/update"), None) => {
                    let session = message
                        .get("params")
                        .and_then(|params| params.get("sessionId"))
                        .and_then(Value::as_str);
                    if wait.policy.is_some()
                        && session == Some(self.session_id.as_str())
                        && let Some(tool) = note_update(&message, report, &self.limits)
                    {
                        report.activity_updates = report.activity_updates.saturating_add(1);
                        if let Some(sender) = &self.activity_sender
                            && sender
                                .try_send(ToolActivity {
                                    sequence: report.activity_updates,
                                    tool,
                                })
                                .is_err()
                        {
                            report.activity_dropped = report.activity_dropped.saturating_add(1);
                        }
                    }
                }
                (None, Some(response_id)) => {
                    if *response_id != json!(wait.id) {
                        continue;
                    }
                    if let Some(error) = message.get("error") {
                        let code = error.get("code").and_then(Value::as_i64).unwrap_or(0);
                        // The cancelled error is how an agent may end a cancelled request.
                        if code == -32800
                            && let Some(ended) = winding_down
                        {
                            return Err(Stop::Ended(ended));
                        }
                        return Err(Stop::Rpc {
                            code,
                            message: text_of(error.get("message"), 300),
                        });
                    }
                    if let Some(ended) = winding_down {
                        // The agent answered the cancelled turn: the end is the one the client chose.
                        report.ended = ended;
                    }
                    return Ok(message.get("result").cloned().unwrap_or(Value::Null));
                }
                (_, None) => {}
            }
        }
    }

    async fn begin_cancel(
        &mut self,
        reason: Ended,
        winding_down: &mut Option<Ended>,
        deadline: &mut Option<Instant>,
        report: &mut Report,
    ) {
        *winding_down = Some(reason.clone());
        report.ended = reason;
        *deadline = Some(Instant::now() + self.limits.cancel_grace);
        let message = json!({
            "jsonrpc": "2.0",
            "method": "session/cancel",
            "params": { "sessionId": self.session_id },
        });
        let _ = self.send(&message).await;
    }

    async fn answer_request(
        &mut self,
        method: &str,
        request_id: &Value,
        message: &Value,
        policy: Option<&dyn PermissionPolicy>,
        cancelling: bool,
        report: &mut Report,
    ) -> Result<(), Stop> {
        let response = if method == "session/request_permission" {
            let request = parse_permission(message);
            match (policy, request) {
                (_, None) => error_response(request_id, -32602, "invalid permission request"),
                (None, Some(_)) => error_response(request_id, -32601, "not available now"),
                (Some(_), Some(_)) if cancelling => json!({
                    "jsonrpc": "2.0", "id": request_id,
                    "result": { "outcome": { "outcome": "cancelled" } },
                }),
                (Some(policy), Some(request)) => {
                    let decision = policy.decide(&request).await;
                    let option = pick_option(&request.options, decision.allow);
                    report.permissions.push(PermissionRecord {
                        title: request.title.chars().take(200).collect(),
                        kind: request.kind.clone(),
                        // An allowing decision with no allow-once option on offer is not an allowance.
                        allowed: decision.allow
                            && option.is_some_and(|option| option.kind == OptionKind::AllowOnce),
                        reason: decision.reason,
                    });
                    match option {
                        Some(option) => json!({
                            "jsonrpc": "2.0", "id": request_id,
                            "result": { "outcome": { "outcome": "selected", "optionId": option.id } },
                        }),
                        None => json!({
                            "jsonrpc": "2.0", "id": request_id,
                            "result": { "outcome": { "outcome": "cancelled" } },
                        }),
                    }
                }
            }
        } else {
            // Anything else the agent asks was not advertised, so it is not available.
            error_response(request_id, -32601, "method not found")
        };
        self.send(&response).await
    }

    /// Reads one line from the agent and parses it. `Ok(None)` is the end of its output. Safe to cancel: a partial line is kept.
    async fn read_message(&mut self) -> Result<Option<Value>, String> {
        loop {
            let available = self.stdout.fill_buf().await.map_err(|error| {
                format!("the agent's output could not be read: {}", error.kind())
            })?;
            if available.is_empty() {
                return Ok(None);
            }
            let end = available.iter().position(|byte| *byte == b'\n');
            let take = end.unwrap_or(available.len());
            if self.line.len() + take > self.limits.max_line_bytes {
                return Err(format!(
                    "the agent wrote a line longer than {} bytes",
                    self.limits.max_line_bytes
                ));
            }
            self.line.extend_from_slice(&available[..take]);
            self.stdout.consume(take + usize::from(end.is_some()));
            if end.is_none() {
                continue;
            }
            let line = std::mem::take(&mut self.line);
            let text = String::from_utf8_lossy(&line);
            let text = text.trim();
            if text.is_empty() {
                continue;
            }
            match serde_json::from_str::<Value>(text) {
                Ok(value) if value.is_object() => return Ok(Some(value)),
                _ => {
                    self.stray += 1;
                    if self.stray > self.limits.max_stray_lines {
                        return Err(
                            "the agent wrote lines that are not ACP messages, so it is not speaking ACP"
                                .to_owned(),
                        );
                    }
                }
            }
        }
    }
}

fn ended_of(stop: Stop) -> Ended {
    match stop {
        Stop::Ended(ended) => ended,
        Stop::Rpc { code, message } => {
            Ended::Failed(format!("the agent reported error {code}: {message}"))
        }
    }
}

/// The sign-in methods an agent advertised that it handles itself; a method of another type (terminal) is not offered by this client.
fn parse_auth_methods(result: &Value) -> Vec<AuthMethod> {
    result
        .get("authMethods")
        .and_then(Value::as_array)
        .map(|methods| {
            methods
                .iter()
                .filter(|method| {
                    method
                        .get("type")
                        .and_then(Value::as_str)
                        .is_none_or(|kind| kind == "agent")
                })
                .filter_map(|method| {
                    let id = method.get("id")?.as_str()?;
                    (!id.is_empty() && id.len() <= 80).then(|| AuthMethod {
                        id: id.to_owned(),
                        name: text_of(method.get("name"), 80),
                        description: text_of(method.get("description"), 300),
                    })
                })
                .take(10)
                .collect()
        })
        .unwrap_or_default()
}

fn error_response(id: &Value, code: i64, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

fn text_of(value: Option<&Value>, limit: usize) -> String {
    value
        .and_then(Value::as_str)
        .map(|text| text.chars().take(limit).collect())
        .unwrap_or_default()
}

fn parse_permission(message: &Value) -> Option<PermissionRequest> {
    let params = message.get("params")?;
    let call = params.get("toolCall")?;
    let options = params
        .get("options")?
        .as_array()?
        .iter()
        .filter_map(|option| {
            Some(PermissionOption {
                id: option.get("optionId")?.as_str()?.to_owned(),
                name: text_of(option.get("name"), 80),
                kind: match option.get("kind").and_then(Value::as_str) {
                    Some("allow_once") => OptionKind::AllowOnce,
                    Some("allow_always") => OptionKind::AllowAlways,
                    Some("reject_once") => OptionKind::RejectOnce,
                    Some("reject_always") => OptionKind::RejectAlways,
                    _ => OptionKind::Unknown,
                },
            })
        })
        .collect();
    let paths = call
        .get("locations")
        .and_then(Value::as_array)
        .map(|locations| {
            locations
                .iter()
                .filter_map(|location| location.get("path").and_then(Value::as_str))
                .take(50)
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default();
    Some(PermissionRequest {
        tool_call_id: text_of(call.get("toolCallId"), 200),
        title: text_of(call.get("title"), 500),
        kind: text_of(call.get("kind"), 40),
        paths,
        raw_input: call.get("rawInput").cloned(),
        options,
    })
}

/// The option to answer with: for an allowance, the first allow-once option (never allow-always, which would remember the choice);
/// for a refusal, the first reject-once, else reject-always.
fn pick_option(options: &[PermissionOption], allow: bool) -> Option<&PermissionOption> {
    let find = |kind: OptionKind| options.iter().find(|option| option.kind == kind);
    if allow {
        find(OptionKind::AllowOnce).or_else(|| find(OptionKind::RejectOnce))
    } else {
        find(OptionKind::RejectOnce).or_else(|| find(OptionKind::RejectAlways))
    }
}

/// Folds one `session/update` into the report. Variants this client does not know are counted and otherwise ignored.
fn note_update(message: &Value, report: &mut Report, limits: &Limits) -> Option<ToolNote> {
    report.updates = report.updates.saturating_add(1);
    let update = message
        .get("params")
        .and_then(|params| params.get("update"))?;
    match update.get("sessionUpdate").and_then(Value::as_str) {
        Some("agent_message_chunk") => {
            let chunk = update
                .get("content")
                .filter(|content| content.get("type").and_then(Value::as_str) == Some("text"))
                .and_then(|content| content.get("text"))
                .and_then(Value::as_str);
            if let Some(chunk) = chunk {
                report.message.push_str(chunk);
                let length = report.message.chars().count();
                if length > limits.max_message_chars {
                    let skip = length - limits.max_message_chars;
                    report.message = report.message.chars().skip(skip).collect();
                    report.message_cut = true;
                }
            }
        }
        Some("tool_call" | "tool_call_update") => {
            let id = text_of(update.get("toolCallId"), 200);
            if id.is_empty() {
                return None;
            }
            let position = report.tools.iter().position(|note| note.id == id);
            let position = if let Some(position) = position {
                position
            } else if report.tools.len() < limits.max_tool_notes {
                report.tools.push(ToolNote {
                    id,
                    title: String::new(),
                    kind: "other".to_owned(),
                    status: "pending".to_owned(),
                    paths: Vec::new(),
                });
                report.tools.len() - 1
            } else {
                report.tools_dropped = report.tools_dropped.saturating_add(1);
                return None;
            };
            let note = &mut report.tools[position];
            if let Some(status) = update.get("status").and_then(Value::as_str) {
                note.status = status.chars().take(40).collect();
            }
            if let Some(title) = update.get("title").and_then(Value::as_str) {
                note.title = title.chars().take(200).collect();
            }
            if let Some(kind) = update.get("kind").and_then(Value::as_str) {
                note.kind = kind.chars().take(40).collect();
            }
            if let Some(locations) = update.get("locations").and_then(Value::as_array) {
                note.paths = locations
                    .iter()
                    .filter_map(|location| location.get("path").and_then(Value::as_str))
                    .take(4)
                    .map(|path| path.chars().take(260).collect())
                    .collect();
            }
            return (note.kind != "think").then(|| note.clone());
        }
        _ => {}
    }
    None
}

/// Reads the agent's error stream so its pipe never fills, keeping only the last few kilobytes.
fn drain_stderr(mut stderr: tokio::process::ChildStderr, tail: Arc<Mutex<VecDeque<u8>>>) {
    tokio::spawn(async move {
        let mut buffer = [0_u8; 1024];
        loop {
            match stderr.read(&mut buffer).await {
                Ok(0) | Err(_) => return,
                Ok(count) => {
                    if let Ok(mut held) = tail.lock() {
                        held.extend(&buffer[..count]);
                        while held.len() > STDERR_TAIL_BYTES {
                            held.pop_front();
                        }
                    }
                }
            }
        }
    });
}

fn tail_text(tail: &Mutex<VecDeque<u8>>) -> String {
    tail.lock()
        .map(|held| {
            let bytes: Vec<u8> = held.iter().copied().collect();
            String::from_utf8_lossy(&bytes).trim().to_owned()
        })
        .unwrap_or_default()
}

#[cfg(test)]
#[path = "acp_tests.rs"]
mod tests;
