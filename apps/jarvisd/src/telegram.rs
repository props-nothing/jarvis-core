//! JARVIS in the owner's pocket: a Telegram chat that is the same assistant (`P11-005`, `ADR-0184`).
//!
//! The owner makes a bot with `BotFather` and pastes its token into Settings. The daemon then **polls** Telegram (`getUpdates`, long
//! polling), so no public address and no tunnel is needed. Nothing here is a second brain: a message becomes an ordinary run through
//! [`GatewayState::start_and_drive`], with the same tools, policy and approvals, and the answer is sent back.
//!
//! # Who is answered
//!
//! Only the **paired** chat. Pairing is proven with a code: the first stranger-or-owner who writes to the bot is told a six digit code,
//! and the owner types it into Settings, which proves the person holding the phone is the person at the computer. Ten minutes, five
//! wrong tries, then the code is gone. Once paired, every other sender is ignored **without a reply**, so the bot reveals nothing to
//! someone who finds it. Groups, channels and business chats are never answered.
//!
//! # Approvals
//!
//! A pending approval is sent as a message with **Approve** and **Deny** buttons. One tap decides it, as on every other surface
//! (`ADR-0136`); the decision is recorded through the same route as a click in the console. Stored under the `api` channel, because
//! the audit vocabulary is closed and widening it needs a migration this slice does not justify.
//!
//! # The token
//!
//! It is read from its private file on every pass, so a change in Settings applies without a restart, and it is never logged: the
//! token is part of every request's address, so a transport error is stripped of its address before anything records it.

use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, Mutex, PoisonError},
    time::{Duration, Instant},
};

use axum::{
    Json,
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use jarvis_core::{ApprovalChannel, ErrorCode};
use jarvis_protocol::{ApprovalDecisionBody, ApprovalDecisionRequest, StartRunRequest};
use jarvis_storage::AppPaths;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::gateway::{GatewayState, error_response};
use crate::telegram_format::{Piece, to_pieces};

/// The Bot API's address. The daemon talks to nothing else unless `JARVIS_TELEGRAM_API_BASE` names a Local Bot API server.
pub const DEFAULT_API_BASE: &str = "https://api.telegram.org";

const POLL_SECONDS: u64 = 25;
const CODE_LIFETIME: Duration = Duration::from_secs(600);
const MAX_CODE_TRIES: u8 = 5;
const HINT_EVERY: Duration = Duration::from_secs(30);
/// Telegram allows 4096 characters after tags are removed; the rest is margin.
const MAX_MESSAGE_CHARS: usize = 3900;
const MAX_FOLLOW_SECONDS: u64 = 2 * 60 * 60;
const CHAT_FILE: &str = "telegram-chat.json";
const SESSION_FILE: &str = "telegram-session.txt";
const OFFSET_FILE: &str = "telegram-offset.txt";

/// Why a Bot API call did not succeed. Never carries the token: see the module header.
#[derive(Debug)]
enum ApiError {
    /// Telegram answered `ok: false`.
    Refused {
        code: i64,
        description: String,
        retry_after: Option<u64>,
    },
    /// The request did not complete.
    Network(String),
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Refused {
                code, description, ..
            } => write!(formatter, "Telegram answered {code}: {description}"),
            Self::Network(message) => write!(formatter, "Telegram could not be reached: {message}"),
        }
    }
}

/// The Bot API over HTTPS: one POST per method, JSON in and out.
#[derive(Clone)]
struct BotApi {
    client: reqwest::Client,
    base: String,
    token: String,
}

impl BotApi {
    fn new(base: &str, token: String) -> Option<Self> {
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .ok()?;
        Some(Self {
            client,
            base: base.trim_end_matches('/').to_owned(),
            token,
        })
    }

    async fn call(&self, method: &str, body: &Value, timeout: Duration) -> Result<Value, ApiError> {
        let url = format!("{}/bot{}/{method}", self.base, self.token);
        let response = self
            .client
            .post(url)
            .timeout(timeout)
            .json(body)
            .send()
            .await
            .map_err(|error| ApiError::Network(error.without_url().to_string()))?;
        let status = response.status();
        let parsed: Value = response
            .json()
            .await
            .map_err(|error| ApiError::Network(error.without_url().to_string()))?;
        if parsed.get("ok").and_then(Value::as_bool) == Some(true) {
            return Ok(parsed.get("result").cloned().unwrap_or(Value::Null));
        }
        Err(ApiError::Refused {
            code: parsed
                .get("error_code")
                .and_then(Value::as_i64)
                .unwrap_or_else(|| i64::from(status.as_u16())),
            description: parsed
                .get("description")
                .and_then(Value::as_str)
                .unwrap_or("no description")
                .chars()
                .take(200)
                .collect(),
            retry_after: parsed
                .pointer("/parameters/retry_after")
                .and_then(Value::as_u64),
        })
    }

    async fn quick(&self, method: &str, body: &Value) -> Result<Value, ApiError> {
        self.call(method, body, Duration::from_secs(15)).await
    }

    /// Sends text, waiting out one flood-control answer. Returns the message's identifier.
    async fn send(&self, chat_id: i64, text: &str, markup: Option<Value>) -> Result<i64, ApiError> {
        self.send_with(chat_id, text, markup, None).await
    }

    async fn send_with(
        &self,
        chat_id: i64,
        text: &str,
        markup: Option<Value>,
        parse_mode: Option<&str>,
    ) -> Result<i64, ApiError> {
        let mut body = json!({ "chat_id": chat_id, "text": text });
        if let Some(markup) = markup {
            body["reply_markup"] = markup;
        }
        if let Some(mode) = parse_mode {
            body["parse_mode"] = json!(mode);
        }
        let sent = match self.quick("sendMessage", &body).await {
            Err(ApiError::Refused {
                retry_after: Some(seconds),
                ..
            }) => {
                tokio::time::sleep(Duration::from_secs(seconds.min(30))).await;
                self.quick("sendMessage", &body).await?
            }
            other => other?,
        };
        Ok(sent.get("message_id").and_then(Value::as_i64).unwrap_or(0))
    }

    /// Sends formatted text. A message Telegram will not parse is sent again as the plain text it says, so it is never lost.
    async fn send_piece(&self, chat_id: i64, piece: &Piece) -> Result<i64, ApiError> {
        match self
            .send_with(chat_id, &piece.html, None, Some("HTML"))
            .await
        {
            Err(ApiError::Refused {
                code: 400,
                description,
                ..
            }) if description.contains("parse entities") => {
                tracing::debug!(
                    "Telegram would not parse a message's formatting; sending it plain"
                );
                self.send(chat_id, &piece.plain, None).await
            }
            other => other,
        }
    }

    /// Sends Markdown of any length as formatted messages, a second apart (Telegram allows about one message a second to a chat).
    async fn send_long(&self, chat_id: i64, markdown: &str) {
        for (index, piece) in to_pieces(markdown, MAX_MESSAGE_CHARS).iter().enumerate() {
            if index > 0 {
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
            if let Err(error) = self.send_piece(chat_id, piece).await {
                tracing::warn!(%error, "a Telegram message could not be sent");
                return;
            }
        }
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
struct Paired {
    chat_id: i64,
    name: String,
}

struct PendingPair {
    code: String,
    chat_id: i64,
    name: String,
    expires: Instant,
    tries: u8,
}

#[derive(Default)]
struct Inner {
    state: &'static str,
    bot: Option<String>,
    problem: Option<String>,
    paired: Option<Paired>,
    pending: Option<PendingPair>,
    api: Option<BotApi>,
    hinted: HashMap<i64, Instant>,
    active_run: Option<String>,
    session: Option<String>,
    /// Approvals answered by a tap here, so their card is not overwritten with 'Answered' by the watcher.
    tapped: std::collections::HashSet<String>,
}

/// The Telegram side of the daemon: its state, and the loop that serves it.
pub struct TelegramBridge {
    paths: AppPaths,
    base: String,
    /// How often pending approvals are looked at.
    watch_every: Duration,
    inner: Mutex<Inner>,
}

/// Why pairing was refused. The words are shown to the owner.
#[derive(Debug, Eq, PartialEq)]
pub enum PairError {
    /// No one has written to the bot, or the code ran out.
    NoCode,
    /// The code was wrong.
    Wrong,
    /// Too many wrong tries: the code is gone.
    TooManyTries,
}

impl PairError {
    fn words(&self) -> &'static str {
        match self {
            Self::NoCode => {
                "There is no code waiting. Send your bot a message in Telegram to get one."
            }
            Self::Wrong => "That is not the code your bot sent.",
            Self::TooManyTries => {
                "Too many wrong tries. Send your bot a new message to get a new code."
            }
        }
    }
}

impl TelegramBridge {
    /// A bridge over this profile. It does nothing until a token is saved.
    #[must_use]
    pub fn new(paths: AppPaths) -> Self {
        let base = std::env::var("JARVIS_TELEGRAM_API_BASE")
            .ok()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| DEFAULT_API_BASE.to_owned());
        Self::with_base(paths, base)
    }

    fn with_base(paths: AppPaths, base: String) -> Self {
        let bridge = Self {
            paths,
            base,
            watch_every: Duration::from_secs(2),
            inner: Mutex::new(Inner {
                state: "off",
                ..Inner::default()
            }),
        };
        bridge.lock().paired = bridge.read_paired();
        bridge.lock().session = bridge.read_text(SESSION_FILE);
        bridge
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn file(&self, name: &str) -> PathBuf {
        self.paths.config().join(name)
    }

    fn read_text(&self, name: &str) -> Option<String> {
        std::fs::read_to_string(self.file(name))
            .ok()
            .map(|text| text.trim().to_owned())
            .filter(|text| !text.is_empty())
    }

    fn write_text(&self, name: &str, text: &str) {
        if let Err(error) = std::fs::write(self.file(name), text) {
            tracing::warn!(%error, file = name, "a Telegram state file could not be written");
        }
    }

    fn read_paired(&self) -> Option<Paired> {
        let value: Value = serde_json::from_str(&self.read_text(CHAT_FILE)?).ok()?;
        Some(Paired {
            chat_id: value.get("chat_id")?.as_i64()?,
            name: value.get("name")?.as_str()?.to_owned(),
        })
    }

    /// The token, read fresh from its private file, or `None` when Telegram is not set up.
    fn read_token(&self) -> Option<String> {
        let loaded = jarvis_storage::ConfigStore::from_paths(&self.paths)
            .load()
            .ok()?;
        let path = loaded.config().daemon().telegram_token_ref()?.to_owned();
        let token = std::fs::read_to_string(path).ok()?.trim().to_owned();
        (!token.is_empty()).then_some(token)
    }

    /// What Settings shows.
    #[must_use]
    pub fn status(&self) -> Value {
        let inner = self.lock();
        let state = if inner.paired.is_some() && inner.state != "off" && inner.state != "error" {
            "paired"
        } else if inner.state == "waiting_for_message" && inner.pending.is_some() {
            "waiting_for_code"
        } else {
            inner.state
        };
        json!({
            "state": state,
            "bot": inner.bot,
            "paired_as": inner.paired.as_ref().map(|paired| paired.name.clone()),
            "problem": inner.problem,
        })
    }

    /// Confirms the code the bot sent, which pairs the chat.
    ///
    /// # Errors
    ///
    /// Returns [`PairError`] when there is no code, it is wrong, or it ran out of tries.
    pub async fn pair(&self, code: &str) -> Result<String, PairError> {
        let (name, api, chat_id) = {
            let mut inner = self.lock();
            let Some(pending) = inner.pending.as_mut() else {
                return Err(PairError::NoCode);
            };
            if Instant::now() > pending.expires {
                inner.pending = None;
                return Err(PairError::NoCode);
            }
            if pending.code != code.trim() {
                pending.tries += 1;
                if pending.tries >= MAX_CODE_TRIES {
                    inner.pending = None;
                    return Err(PairError::TooManyTries);
                }
                return Err(PairError::Wrong);
            }
            let pending = inner.pending.take().ok_or(PairError::NoCode)?;
            let paired = Paired {
                chat_id: pending.chat_id,
                name: pending.name.clone(),
            };
            inner.paired = Some(paired.clone());
            inner.active_run = None;
            let api = inner.api.clone();
            (paired.name, api, paired.chat_id)
        };
        self.write_text(
            CHAT_FILE,
            &json!({ "chat_id": chat_id, "name": name }).to_string(),
        );
        if let Some(api) = api {
            let _ = api
                .send(
                    chat_id,
                    "Connected. You can talk to me here, and I will ask here when I need your approval. Send /help to see what I understand.",
                    None,
                )
                .await;
        }
        Ok(name)
    }

    /// Forgets the paired chat. The token stays; the next message to the bot starts pairing again.
    pub fn unpair(&self) {
        {
            let mut inner = self.lock();
            inner.paired = None;
            inner.pending = None;
            inner.session = None;
            inner.active_run = None;
        }
        for name in [CHAT_FILE, SESSION_FILE] {
            let _ = std::fs::remove_file(self.file(name));
        }
    }

    /// Tells the paired chat something (a scheduled task's result). Nothing happens when no chat is paired.
    pub async fn tell(&self, text: &str) {
        let (paired, api) = {
            let inner = self.lock();
            (inner.paired.clone(), inner.api.clone())
        };
        if let (Some(paired), Some(api)) = (paired, api) {
            api.send_long(paired.chat_id, text).await;
        }
    }

    fn set_state(&self, state: &'static str, problem: Option<String>) {
        let mut inner = self.lock();
        inner.state = state;
        inner.problem = problem;
    }

    /// The loop: wait for a token, serve the chat while it holds, start over when it changes.
    pub async fn run(self: Arc<Self>, state: GatewayState) {
        loop {
            let Some(token) = self.read_token() else {
                {
                    let mut inner = self.lock();
                    inner.api = None;
                    inner.bot = None;
                }
                self.set_state("off", None);
                tokio::time::sleep(Duration::from_secs(3)).await;
                continue;
            };
            let Some(api) = BotApi::new(&self.base, token.clone()) else {
                self.set_state(
                    "error",
                    Some("the network client could not start".to_owned()),
                );
                tokio::time::sleep(Duration::from_secs(30)).await;
                continue;
            };
            match api.quick("getMe", &json!({})).await {
                Ok(me) => {
                    let mut inner = self.lock();
                    inner.bot = me
                        .get("username")
                        .and_then(Value::as_str)
                        .map(str::to_owned);
                    inner.api = Some(api.clone());
                    inner.state = "waiting_for_message";
                    inner.problem = None;
                }
                Err(ApiError::Refused { code, .. }) if code == 401 || code == 404 => {
                    self.set_state(
                        "error",
                        Some(
                            "Telegram did not accept that token. Check it with @BotFather."
                                .to_owned(),
                        ),
                    );
                    tokio::time::sleep(Duration::from_secs(15)).await;
                    continue;
                }
                Err(error) => {
                    self.set_state("error", Some(error.to_string()));
                    tokio::time::sleep(Duration::from_secs(10)).await;
                    continue;
                }
            }
            self.serve(&state, &api, &token).await;
        }
    }

    /// Serves one token until it changes or is removed.
    async fn serve(self: &Arc<Self>, state: &GatewayState, api: &BotApi, token: &str) {
        let watcher = {
            let bridge = Arc::clone(self);
            let state = state.clone();
            let api = api.clone();
            tokio::spawn(async move { bridge.watch_approvals(&state, &api).await })
        };
        let mut offset = self
            .read_text(OFFSET_FILE)
            .and_then(|text| text.parse::<i64>().ok())
            .unwrap_or(0);
        let mut cleared_webhook = false;
        loop {
            if self.read_token().as_deref() != Some(token) {
                break;
            }
            let body = json!({
                "offset": offset,
                "timeout": POLL_SECONDS,
                "allowed_updates": ["message", "callback_query"],
            });
            let updates = match api
                .call("getUpdates", &body, Duration::from_secs(POLL_SECONDS + 10))
                .await
            {
                Ok(updates) => updates,
                Err(ApiError::Refused { code: 409, .. }) if !cleared_webhook => {
                    // A webhook was set on this bot, which makes polling impossible; this bot is JARVIS's now.
                    cleared_webhook = true;
                    let _ = api.quick("deleteWebhook", &json!({})).await;
                    continue;
                }
                Err(ApiError::Refused { code: 409, .. }) => {
                    self.set_state(
                        "error",
                        Some("another program is reading this bot's messages. Stop it, or make a new bot.".to_owned()),
                    );
                    tokio::time::sleep(Duration::from_secs(20)).await;
                    continue;
                }
                Err(ApiError::Refused { code: 401, .. }) => {
                    self.set_state(
                        "error",
                        Some("Telegram no longer accepts the token.".to_owned()),
                    );
                    break;
                }
                Err(error) => {
                    tracing::warn!(%error, "Telegram could not be polled");
                    tokio::time::sleep(Duration::from_secs(5)).await;
                    continue;
                }
            };
            {
                let mut inner = self.lock();
                if inner.state == "error" {
                    inner.state = "waiting_for_message";
                    inner.problem = None;
                }
            }
            for update in updates.as_array().into_iter().flatten() {
                let id = update.get("update_id").and_then(Value::as_i64).unwrap_or(0);
                if id < offset {
                    continue;
                }
                // Confirmed before it is handled: a crash loses one message and never repeats one, the scheduler's rule.
                offset = id + 1;
                self.write_text(OFFSET_FILE, &offset.to_string());
                self.handle_update(state, api, update).await;
            }
        }
        watcher.abort();
    }

    async fn handle_update(self: &Arc<Self>, state: &GatewayState, api: &BotApi, update: &Value) {
        if let Some(query) = update.get("callback_query") {
            self.handle_callback(state, api, query).await;
        } else if let Some(message) = update.get("message") {
            self.handle_message(state, api, message).await;
        }
    }

    async fn handle_message(self: &Arc<Self>, state: &GatewayState, api: &BotApi, message: &Value) {
        let chat = message.pointer("/chat/id").and_then(Value::as_i64);
        let private = message.pointer("/chat/type").and_then(Value::as_str) == Some("private");
        let from_bot = message.pointer("/from/is_bot").and_then(Value::as_bool) == Some(true);
        let (Some(chat_id), true, false) = (chat, private, from_bot) else {
            return;
        };
        let text = message
            .get("text")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .trim()
            .to_owned();
        let paired = self.lock().paired.clone();
        match paired {
            None => self.offer_pairing(api, chat_id, message).await,
            // Anyone else gets silence: the bot confirms nothing to a stranger.
            Some(paired) if paired.chat_id != chat_id => {}
            Some(_) => {
                if text.is_empty() {
                    let _ = api
                        .send(chat_id, "I can read text for now. Send me a message.", None)
                        .await;
                } else {
                    self.converse(state, api, chat_id, &text).await;
                }
            }
        }
    }

    /// Before pairing: tell the writer a code, once in a while, and only while no one else holds a live one.
    async fn offer_pairing(&self, api: &BotApi, chat_id: i64, message: &Value) {
        let name = message
            .pointer("/from/first_name")
            .and_then(Value::as_str)
            .unwrap_or("Telegram user")
            .chars()
            .take(40)
            .collect::<String>();
        let code = {
            let mut inner = self.lock();
            if inner
                .hinted
                .get(&chat_id)
                .is_some_and(|at| at.elapsed() < HINT_EVERY)
            {
                return;
            }
            let live = inner
                .pending
                .as_ref()
                .filter(|pending| Instant::now() <= pending.expires)
                .map(|pending| (pending.chat_id, pending.code.clone()));
            let code = match live {
                Some((holder, _)) if holder != chat_id => return,
                Some((_, code)) => code,
                None => {
                    let code = new_code();
                    inner.pending = Some(PendingPair {
                        code: code.clone(),
                        chat_id,
                        name,
                        expires: Instant::now() + CODE_LIFETIME,
                        tries: 0,
                    });
                    code
                }
            };
            inner.hinted.insert(chat_id, Instant::now());
            code
        };
        let _ = api
            .send(
                chat_id,
                &format!(
                    "Hi. To connect me to your JARVIS, open JARVIS, go to Settings, Telegram, and enter this code: {code}\n\nIt works for 10 minutes. If you did not ask for this, ignore it."
                ),
                None,
            )
            .await;
    }

    async fn converse(
        self: &Arc<Self>,
        state: &GatewayState,
        api: &BotApi,
        chat_id: i64,
        text: &str,
    ) {
        match text.split_whitespace().next().unwrap_or_default() {
            "/start" | "/help" => {
                let _ = api
                    .send(
                        chat_id,
                        "Write to me as you would in the console. I answer here, and when I need your approval I send a message with Approve and Deny buttons.\n\n/new starts a fresh conversation.\n/stop is not needed: ask me to stop and I will.",
                        None,
                    )
                    .await;
                return;
            }
            "/new" => {
                {
                    let mut inner = self.lock();
                    inner.session = None;
                }
                let _ = std::fs::remove_file(self.file(SESSION_FILE));
                let _ = api
                    .send(chat_id, "Fresh conversation. What now?", None)
                    .await;
                return;
            }
            _ => {}
        }
        if self.still_working(state).await {
            let _ = api
                .send(
                    chat_id,
                    "I am still working on your last message. I will answer it first.",
                    None,
                )
                .await;
            return;
        }
        let session = self.lock().session.clone();
        let request = StartRunRequest {
            objective: text.to_owned(),
            session_id: session,
            idempotency_key: None,
            project_id: None,
        };
        if let Ok(reply) = state.start_and_drive(&request).await {
            {
                {
                    let mut inner = self.lock();
                    inner.session = Some(reply.session_id.clone());
                    inner.active_run = Some(reply.run_id.clone());
                }
                self.write_text(SESSION_FILE, &reply.session_id);
                let bridge = Arc::clone(self);
                let state = state.clone();
                let api = api.clone();
                tokio::spawn(async move {
                    bridge
                        .follow_run(&state, &api, chat_id, &reply.run_id)
                        .await;
                });
            }
        } else {
            tracing::warn!("a Telegram message could not start a run");
            let _ = api
                .send(
                    chat_id,
                    "I could not start that. Try again in a moment.",
                    None,
                )
                .await;
        }
    }

    async fn still_working(&self, state: &GatewayState) -> bool {
        let Some(run_id) = self.lock().active_run.clone() else {
            return false;
        };
        match jarvis_storage::find_run(state.database(), &run_id).await {
            Ok(run) => !run.state().is_terminal(),
            Err(_) => false,
        }
    }

    /// Sends the answer when the run ends, and "typing" while it works.
    async fn follow_run(&self, state: &GatewayState, api: &BotApi, chat_id: i64, run_id: &str) {
        for tick in 0..MAX_FOLLOW_SECONDS {
            tokio::time::sleep(Duration::from_secs(1)).await;
            let Ok(run) = jarvis_storage::find_run(state.database(), run_id).await else {
                continue;
            };
            if run.state().is_terminal() {
                let answer = if run.terminal_outcome() == Some(jarvis_core::RunOutcome::Succeeded) {
                    crate::executor::last_answer(&state.database_handle(), &run)
                        .await
                        .ok()
                        .flatten()
                } else {
                    None
                };
                {
                    let mut inner = self.lock();
                    if inner.active_run.as_deref() == Some(run_id) {
                        inner.active_run = None;
                    }
                }
                api.send_long(
                    chat_id,
                    &answer.unwrap_or_else(|| "I could not finish that.".to_owned()),
                )
                .await;
                return;
            }
            if tick % 4 == 0 && run.state() != jarvis_core::RunState::AwaitingApproval {
                let _ = api
                    .quick(
                        "sendChatAction",
                        &json!({ "chat_id": chat_id, "action": "typing" }),
                    )
                    .await;
            }
        }
    }

    /// Sends each new pending approval as a card with two buttons, and retires the card of one answered elsewhere.
    async fn watch_approvals(&self, state: &GatewayState, api: &BotApi) {
        let mut sent: HashMap<String, (i64, String)> = HashMap::new();
        let mut first = true;
        loop {
            tokio::time::sleep(self.watch_every).await;
            let Some(paired) = self.lock().paired.clone() else {
                continue;
            };
            let Ok(pending) = crate::gateway::pending_approvals(state).await else {
                continue;
            };
            let live: std::collections::HashSet<&str> = pending
                .iter()
                .map(|approval| approval.approval_id.as_str())
                .collect();
            for approval in pending
                .iter()
                .filter(|approval| approval.state == "pending")
            {
                if sent.contains_key(&approval.approval_id) {
                    continue;
                }
                // What was already waiting when the daemon started is the console's to show, not a flood on the phone.
                if first {
                    sent.insert(approval.approval_id.clone(), (0, String::new()));
                    continue;
                }
                let card = approval_card(approval);
                let markup = json!({ "inline_keyboard": [[
                    { "text": "Approve", "callback_data": format!("a:{}", approval.approval_id) },
                    { "text": "Deny", "callback_data": format!("d:{}", approval.approval_id) },
                ]] });
                if let Ok(message_id) = api.send(paired.chat_id, &card, Some(markup)).await {
                    sent.insert(approval.approval_id.clone(), (message_id, card));
                }
            }
            first = false;
            let gone: Vec<String> = sent
                .keys()
                .filter(|id| !live.contains(id.as_str()))
                .cloned()
                .collect();
            for id in gone {
                let tapped = self.lock().tapped.remove(&id);
                if let Some((message_id, card)) = sent.remove(&id)
                    && message_id != 0
                    && !tapped
                {
                    retire(
                        api,
                        paired.chat_id,
                        message_id,
                        &format!("{card}\n\nAnswered."),
                    )
                    .await;
                }
            }
        }
    }

    async fn handle_callback(&self, state: &GatewayState, api: &BotApi, query: &Value) {
        let query_id = query.get("id").and_then(Value::as_str).unwrap_or_default();
        let from = query.pointer("/from/id").and_then(Value::as_i64);
        let chat = query.pointer("/message/chat/id").and_then(Value::as_i64);
        let message_id = query.pointer("/message/message_id").and_then(Value::as_i64);
        let paired = self.lock().paired.clone();
        // Only the paired chat may answer; anything else is acknowledged (the client waits for that) and ignored.
        let allowed = paired
            .as_ref()
            .is_some_and(|paired| Some(paired.chat_id) == chat && Some(paired.chat_id) == from);
        let answer = |text: &str| json!({ "callback_query_id": query_id, "text": text });
        if !allowed {
            let _ = api
                .quick(
                    "answerCallbackQuery",
                    &json!({ "callback_query_id": query_id }),
                )
                .await;
            return;
        }
        let data = query
            .get("data")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let (approve, id) = match data.split_once(':') {
            Some(("a", id)) => (true, id),
            Some(("d", id)) => (false, id),
            _ => {
                let _ = api
                    .quick("answerCallbackQuery", &answer("Unknown button"))
                    .await;
                return;
            }
        };
        let body = ApprovalDecisionBody {
            decision: if approve {
                ApprovalDecisionRequest::Approve
            } else {
                ApprovalDecisionRequest::Deny
            },
            resume: true,
            channel: Some(ApprovalChannel::Api),
        };
        // Marked before the decision, so the watcher that notices the approval is gone cannot overwrite this answer on the card.
        self.lock().tapped.insert(id.to_owned());
        let response = crate::gateway::decide_approval(
            State(state.clone()),
            axum::extract::Path(id.to_owned()),
            Json(body),
        )
        .await;
        let done = response.status().is_success();
        if !done {
            self.lock().tapped.remove(id);
        }
        let words = match (done, approve) {
            (true, true) => "Approved",
            (true, false) => "Denied",
            (false, _) => "That is no longer waiting",
        };
        let _ = api.quick("answerCallbackQuery", &answer(words)).await;
        if let (Some(chat_id), Some(message_id)) = (chat, message_id) {
            let original = query
                .pointer("/message/text")
                .and_then(Value::as_str)
                .unwrap_or("Approval");
            retire(api, chat_id, message_id, &format!("{original}\n\n{words}.")).await;
        }
    }
}

/// Replaces a card's text and removes its buttons.
async fn retire(api: &BotApi, chat_id: i64, message_id: i64, text: &str) {
    let body = json!({
        "chat_id": chat_id,
        "message_id": message_id,
        "text": text,
        "reply_markup": { "inline_keyboard": [] },
    });
    if let Err(error) = api.quick("editMessageText", &body).await {
        tracing::debug!(%error, "an approval card could not be updated");
    }
}

/// What the owner is asked, in a few lines: the tool, how risky, what it says it will do, and its arguments.
fn approval_card(approval: &jarvis_protocol::PendingApprovalReply) -> String {
    let arguments = approval.arguments.as_ref().map_or_else(
        || "(arguments not shown)".to_owned(),
        |value| {
            let text = value.to_string();
            let clipped: String = text.chars().take(700).collect();
            if text.chars().count() > 700 {
                format!("{clipped}...")
            } else {
                clipped
            }
        },
    );
    format!(
        "JARVIS wants to use {} (risk {}).\n{}\n\n{}",
        approval.tool, approval.risk_level, approval.preview, arguments
    )
}

/// Six random digits.
fn new_code() -> String {
    let mut bytes = [0_u8; 4];
    if getrandom::fill(&mut bytes).is_err() {
        // Without randomness there is no safe code: use the clock, which still cannot be guessed from outside in ten minutes of tries.
        let nanos = jarvis_core::UtcTimestamp::now(&jarvis_core::SystemClock).unix_nanos();
        bytes = u32::try_from(nanos.rem_euclid(1_000_000_007))
            .unwrap_or(0)
            .to_le_bytes();
    }
    format!("{:06}", u32::from_le_bytes(bytes) % 1_000_000)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PairBody {
    code: String,
}

/// The authenticated routes, mounted under /api/v1.
pub fn routes() -> axum::Router<GatewayState> {
    use axum::routing::{get, post};
    axum::Router::new()
        .route("/telegram", get(status))
        .route("/telegram/pair", post(pair))
        .route("/telegram/unpair", post(unpair))
}

fn unavailable() -> Response {
    error_response(
        StatusCode::NOT_IMPLEMENTED,
        ErrorCode::Unsupported,
        "Telegram is not available on this daemon",
    )
}

/// `GET /api/v1/telegram`
async fn status(State(state): State<GatewayState>) -> Response {
    match state.telegram() {
        Some(bridge) => Json(bridge.status()).into_response(),
        None => unavailable(),
    }
}

/// `POST /api/v1/telegram/pair`
async fn pair(State(state): State<GatewayState>, Json(body): Json<PairBody>) -> Response {
    let Some(bridge) = state.telegram() else {
        return unavailable();
    };
    match bridge.pair(&body.code).await {
        Ok(name) => Json(json!({ "paired_as": name })).into_response(),
        Err(error) => error_response(
            StatusCode::UNPROCESSABLE_ENTITY,
            ErrorCode::Validation,
            error.words(),
        ),
    }
}

/// `POST /api/v1/telegram/unpair`
async fn unpair(State(state): State<GatewayState>) -> Response {
    let Some(bridge) = state.telegram() else {
        return unavailable();
    };
    bridge.unpair();
    Json(json!({ "unpaired": true })).into_response()
}

#[cfg(test)]
#[path = "telegram_tests.rs"]
mod tests;
