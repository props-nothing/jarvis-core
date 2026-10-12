//! Phone calls: JARVIS rings the owner when it needs to, and the owner rings JARVIS (`ADR-0200`).
//!
//! # Who does what
//!
//! The provider (an `ElevenLabs` agent with a phone number) runs the call: the telephony, the speech and the turn-taking. JARVIS decides
//! **whether** a call is placed and **to whom**, keeps the record, and stays the only holder of the tools: the agent reaches them through
//! the Connect apps endpoint like any other app, so every call it makes passes the same policy and approvals.
//!
//! # What a call may be
//!
//! - **Only a number the owner listed.** A model names a reason, never a number; an optional number must already be on the owner's list.
//! - **Never silently.** The tool asks the owner every time unless the owner trusted it by name (`ADR-0178`), and quiet hours, a rate
//!   bound and a single open call apply even then.
//! - **Recorded before it rings.** A record in the database precedes the provider request, so a request that timed out still counts.
//! - **Not retried.** A second request would ring the phone again.
//!
//! # Approving by phone
//!
//! The agent may ask the owner on the phone before it uses a tool, and the same tool also waits in the console (`ADR-0180`). The owner can
//! let a yes on the phone count (`daemon.phone_approves`), but only while a call **JARVIS placed to the owner's own number** is answered,
//! only for tools of risk 2 or less, and never for the phone tools themselves. A call someone else places to the provider's number is not
//! the owner; a caller's number is not proof of who is speaking.

use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use axum::{
    Json,
    extract::{RawQuery, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use jarvis_core::{
    ErrorCode, SystemClock, UtcTimestamp, mask_phone_number, normalize_phone_number,
};
use jarvis_storage::{
    AppPaths, ConfigStore, NewPhoneCall, PhoneCallUpdate, SqliteDatabase, StoredPhoneCall,
};
use jarvis_voice::phone::{
    CallRequest, CallState, CallStatus, ElevenLabsPhone, NumberProvider, PhoneError, Speaker,
};
use serde_json::{Value, json};
use tokio::sync::Mutex as AsyncMutex;

use crate::gateway::{GatewayState, error_response};

/// The most calls placed in one hour. A runaway loop of approved calls stops here.
pub const MAX_CALLS_PER_HOUR: u32 = 3;
/// The most calls placed in one day.
pub const MAX_CALLS_PER_DAY: u32 = 10;
/// The highest tool risk a yes spoken on the phone may approve.
pub const MAX_PHONE_APPROVAL_RISK: u8 = 2;
/// How long after it was placed a call can still count as the owner being on the line.
const LIVE_CALL_WINDOW: Duration = Duration::from_mins(30);
/// How long a call may stay open in the records before it is written off, so one that the provider forgot cannot block every later call.
const WRITE_OFF_AFTER: Duration = Duration::from_hours(2);
/// How long a record may stay `starting` before the daemon gives up waiting for the provider's answer.
const STARTING_GRACE: Duration = Duration::from_secs(120);
/// How often unsettled calls are looked at.
const POLL_EVERY: Duration = Duration::from_secs(10);
/// How long a look at the provider is reused when a tool asks whether the owner is on the line.
const LIVE_CACHE: Duration = Duration::from_secs(5);
/// How long call records are kept.
const RETENTION: Duration = Duration::from_hours(90 * 24);
/// Records shown in Settings.
const RECENT_CALLS: u32 = 10;

/// Why a call could not be started.
#[derive(Debug)]
pub enum StartError {
    /// Phone calls are not set up yet; says what is missing.
    NotSetUp(String),
    /// A rule refused it: a number not on the list, quiet hours, the rate bound, a call already open.
    NotAllowed(String),
    /// The provider refused it or could not be asked.
    Provider(PhoneError),
    /// The record could not be written or read.
    Database,
}

/// What the owner configured, read fresh from the saved configuration on every use so Settings applies at once.
#[derive(Clone, Debug)]
pub struct PhoneSetup {
    /// The agent that speaks.
    pub agent_id: String,
    /// The number calls are placed from.
    pub number_id: String,
    /// The numbers that may be called.
    pub owner_numbers: Vec<String>,
    /// Whether a yes on the phone approves a low-risk tool.
    pub approves: bool,
    /// Whether JARVIS writes the agent's first sentence.
    pub opening_line: bool,
    key: String,
    quiet: Option<jarvis_core::QuietHours>,
}

/// What one call looks like to the model and the console.
#[derive(Debug)]
pub struct CallView {
    /// The stored record, freshly reconciled.
    pub call: StoredPhoneCall,
    /// The provider's current view, when it was asked and answered.
    pub state: Option<CallState>,
}

/// A yes that came from the owner on the phone.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PhoneYes {
    /// The call the owner is on.
    pub call_id: String,
}

/// Lets a call from another app be approved by the owner on the phone (`daemon.phone_approves`).
#[async_trait]
pub trait PhoneApproval: Send + Sync {
    /// Whether the owner's yes on a live call JARVIS placed counts for this tool, which has this risk.
    async fn approves(&self, tool: &str, risk: u8) -> Option<PhoneYes>;
}

/// The phone calls of one daemon.
pub struct PhoneService {
    paths: AppPaths,
    database: Arc<SqliteDatabase>,
    base_url: Option<String>,
    /// One start at a time, so the rate bound and the single open call cannot be raced.
    starting: AsyncMutex<()>,
    live: Mutex<Option<(Instant, Option<StoredPhoneCall>)>>,
}

impl std::fmt::Debug for PhoneService {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PhoneService")
            .finish_non_exhaustive()
    }
}

fn now() -> UtcTimestamp {
    UtcTimestamp::now(&SystemClock)
}

fn before(at: UtcTimestamp, by: Duration) -> UtcTimestamp {
    UtcTimestamp::from_unix_nanos(
        at.unix_nanos() - i128::try_from(by.as_nanos()).unwrap_or(i128::MAX / 4),
    )
    .unwrap_or(at)
}

fn age(of: UtcTimestamp, at: UtcTimestamp) -> Duration {
    Duration::from_nanos(
        u64::try_from((at.unix_nanos() - of.unix_nanos()).max(0)).unwrap_or(u64::MAX),
    )
}

impl PhoneService {
    /// Builds the service over the profile and its database.
    #[must_use]
    pub fn new(paths: AppPaths, database: Arc<SqliteDatabase>) -> Self {
        Self {
            paths,
            database,
            base_url: None,
            starting: AsyncMutex::new(()),
            live: Mutex::new(None),
        }
    }

    /// Points the service at another provider address, for a test.
    #[cfg(test)]
    #[must_use]
    pub fn with_base_url(mut self, base_url: &str) -> Self {
        self.base_url = Some(base_url.to_owned());
        self
    }

    async fn workspace(&self) -> Result<String, StartError> {
        jarvis_storage::load_local_identity(&self.database)
            .await
            .map(|identity| identity.workspace_id().to_owned())
            .map_err(|_| StartError::Database)
    }

    /// What is configured, or a sentence naming the first thing still missing.
    ///
    /// # Errors
    ///
    /// Returns the sentence the owner needs: which setting to fill in, and where.
    pub fn setup(&self) -> Result<PhoneSetup, String> {
        let loaded = ConfigStore::from_paths(&self.paths)
            .load()
            .map_err(|_| "the saved configuration could not be read".to_owned())?;
        let daemon = loaded.config().daemon();
        let key = daemon
            .speech_api_key_ref()
            .and_then(|path| std::fs::read_to_string(path).ok())
            .map(|text| text.trim().to_owned())
            .filter(|text| !text.is_empty())
            .ok_or("Phone calls need the ElevenLabs key: add it under Settings, Voice.")?;
        let agent_id = daemon
            .phone_agent_id()
            .ok_or("Phone calls need the ElevenLabs agent id: add it under Settings, Phone.")?;
        let number_id = daemon.phone_number_id().ok_or(
            "Phone calls need the ElevenLabs phone number id: add it under Settings, Phone.",
        )?;
        if daemon.phone_owner_numbers().is_empty() {
            return Err(
                "Phone calls need the number to ring: add yours under Settings, Phone.".to_owned(),
            );
        }
        Ok(PhoneSetup {
            agent_id: agent_id.to_owned(),
            number_id: number_id.to_owned(),
            owner_numbers: daemon.phone_owner_numbers().to_vec(),
            approves: daemon.phone_approves(),
            opening_line: daemon.phone_opening_line(),
            key,
            quiet: daemon.quiet_hours(),
        })
    }

    fn client(&self, setup: &PhoneSetup) -> Result<ElevenLabsPhone, PhoneError> {
        ElevenLabsPhone::new(
            self.base_url.as_deref(),
            &setup.key,
            &setup.agent_id,
            &setup.number_id,
        )
    }

    /// Places a call to the owner and returns its record. Every rule is checked here, in one place, so the tool, the Settings test button
    /// and any future caller are held to the same ones.
    ///
    /// `to` must be on the owner's list; without it the first listed number is rung. `urgent` lets a call ring during quiet hours.
    ///
    /// # Errors
    ///
    /// Returns a [`StartError`] saying which rule refused it, or what the provider said.
    pub async fn start_call(
        &self,
        reason: &str,
        urgent: bool,
        to: Option<&str>,
    ) -> Result<StoredPhoneCall, StartError> {
        let setup = self.setup().map_err(StartError::NotSetUp)?;
        let target = choose_number(&setup, to)?;
        let reason = reason.trim();
        if reason.is_empty() || reason.chars().count() > jarvis_storage::MAX_PHONE_REASON_CHARS {
            return Err(StartError::NotAllowed(format!(
                "say why the call is needed in 1 to {} characters",
                jarvis_storage::MAX_PHONE_REASON_CHARS
            )));
        }
        let at = now();
        if !urgent
            && setup
                .quiet
                .is_some_and(|window| window.is_quiet_at(at, None))
        {
            return Err(StartError::NotAllowed(
                "it is the owner's quiet hours, so the phone will not ring; send a message instead, or call again with urgent set only if it cannot wait"
                    .to_owned(),
            ));
        }
        let client = self.client(&setup).map_err(StartError::Provider)?;
        let workspace = self.workspace().await?;

        // One start at a time: the checks and the record below are one decision.
        let _only = self.starting.lock().await;
        self.check_room(&setup, &workspace, at).await?;
        let record = jarvis_storage::create_phone_call(
            &self.database,
            &NewPhoneCall {
                workspace_id: &workspace,
                number: &target,
                reason,
                urgent,
                now: at,
            },
        )
        .await
        .map_err(|_| StartError::Database)?;
        let request = CallRequest {
            to: target,
            variables: vec![
                ("jarvis_reason".to_owned(), reason.to_owned()),
                ("jarvis_call_id".to_owned(), record.id().to_owned()),
                ("jarvis_urgent".to_owned(), urgent.to_string()),
            ],
            first_message: setup.opening_line.then(|| opening_line(reason)),
        };
        self.hand_over(&client, &request, &workspace, record.id())
            .await
    }

    /// Refuses a call while another is open, or when the phone has been rung too often lately.
    async fn check_room(
        &self,
        setup: &PhoneSetup,
        workspace: &str,
        at: UtcTimestamp,
    ) -> Result<(), StartError> {
        for open in jarvis_storage::unsettled_phone_calls(&self.database, workspace)
            .await
            .map_err(|_| StartError::Database)?
        {
            if self.reconcile(setup, open).await.call.is_live() {
                return Err(StartError::NotAllowed(
                    "a call is already open; wait for it to end (jarvis.voice.status says how it is going)".to_owned(),
                ));
            }
        }
        let since = |window: Duration| {
            jarvis_storage::count_phone_calls_since(&self.database, workspace, before(at, window))
        };
        let hour = since(Duration::from_hours(1))
            .await
            .map_err(|_| StartError::Database)?;
        let day = since(Duration::from_hours(24))
            .await
            .map_err(|_| StartError::Database)?;
        if hour >= MAX_CALLS_PER_HOUR || day >= MAX_CALLS_PER_DAY {
            return Err(StartError::NotAllowed(format!(
                "JARVIS has already rung the owner {MAX_CALLS_PER_HOUR} times in the last hour or {MAX_CALLS_PER_DAY} times today; it will not ring again for now"
            )));
        }
        Ok(())
    }

    /// Asks the provider to ring, once, and records the answer on the call's record.
    async fn hand_over(
        &self,
        client: &ElevenLabsPhone,
        request: &CallRequest,
        workspace: &str,
        id: &str,
    ) -> Result<StoredPhoneCall, StartError> {
        match client.place_call(request).await {
            Ok(placed) => {
                jarvis_storage::attach_provider_call(
                    &self.database,
                    workspace,
                    id,
                    &placed.conversation_id,
                    placed.call_id.as_deref(),
                    now(),
                )
                .await
                .map_err(|_| StartError::Database)?;
                self.forget_live();
                jarvis_storage::find_phone_call(&self.database, workspace, id)
                    .await
                    .map_err(|_| StartError::Database)
            }
            Err(error) => {
                // An uncertain request may still be ringing, so it stays open until the poller writes it off; a refused one is over.
                if !matches!(error, PhoneError::Uncertain) {
                    let words = error.to_string();
                    let _ = jarvis_storage::update_phone_call(
                        &self.database,
                        workspace,
                        id,
                        &PhoneCallUpdate {
                            status: "failed",
                            error: Some(&words),
                            ..PhoneCallUpdate::default()
                        },
                        now(),
                    )
                    .await;
                }
                tracing::warn!(%error, "a phone call could not be placed");
                Err(StartError::Provider(error))
            }
        }
    }
    /// One call as it is now: the record reconciled with the provider.
    ///
    /// # Errors
    ///
    /// Returns a sentence when there is no such call or the records cannot be read.
    pub async fn view(&self, id: Option<&str>) -> Result<CallView, String> {
        let workspace = self
            .workspace()
            .await
            .map_err(|_| "the records cannot be read".to_owned())?;
        let call = match id {
            Some(id) => jarvis_storage::find_phone_call(&self.database, &workspace, id)
                .await
                .map_err(|_| "no call has that id".to_owned())?,
            None => jarvis_storage::list_phone_calls(&self.database, &workspace, 1)
                .await
                .ok()
                .and_then(|mut calls| calls.pop())
                .ok_or("JARVIS has not placed any call yet")?,
        };
        let setup = self.setup().ok();
        Ok(match setup {
            Some(setup) => self.reconcile(&setup, call).await,
            None => CallView { call, state: None },
        })
    }

    /// Brings one record in line with the provider. A final record is returned as it is.
    async fn reconcile(&self, setup: &PhoneSetup, call: StoredPhoneCall) -> CallView {
        if call.is_final() {
            return CallView { call, state: None };
        }
        let at = now();
        let open_for = age(call.created_at(), at);
        let Ok(workspace) = self.workspace().await else {
            return CallView { call, state: None };
        };
        let call_id = call.id().to_owned();
        let write_off = |words: &'static str| {
            let database = Arc::clone(&self.database);
            let workspace = workspace.clone();
            let id = call_id.clone();
            async move {
                let _ = jarvis_storage::update_phone_call(
                    &database,
                    &workspace,
                    &id,
                    &PhoneCallUpdate {
                        status: "failed",
                        error: Some(words),
                        ..PhoneCallUpdate::default()
                    },
                    now(),
                )
                .await;
            }
        };
        let mut state = None;
        match call.conversation_id() {
            None if open_for > STARTING_GRACE => {
                write_off("the provider never confirmed the call").await;
            }
            None => {}
            Some(conversation) => match self.client(setup) {
                Ok(client) => match client.call_state(conversation).await {
                    Ok(seen) => {
                        let error = (seen.status == CallStatus::Failed).then(|| {
                            seen.ended_because
                                .clone()
                                .unwrap_or_else(|| "the call failed".to_owned())
                        });
                        let _ = jarvis_storage::update_phone_call(
                            &self.database,
                            &workspace,
                            call.id(),
                            &PhoneCallUpdate {
                                status: seen.status.word(),
                                duration_seconds: seen.duration_seconds,
                                ended_because: seen.ended_because.as_deref(),
                                summary: seen.summary.as_deref(),
                                error: error.as_deref(),
                            },
                            now(),
                        )
                        .await;
                        state = Some(seen);
                    }
                    Err(PhoneError::CallNotFound) if open_for > STARTING_GRACE => {
                        write_off("the provider has no record of the call").await;
                    }
                    Err(error) => tracing::debug!(%error, "a phone call could not be looked at"),
                },
                Err(error) => tracing::debug!(%error, "a phone call could not be looked at"),
            },
        }
        let mut call = jarvis_storage::find_phone_call(&self.database, &workspace, call.id())
            .await
            .unwrap_or(call);
        if call.is_live() && open_for > WRITE_OFF_AFTER {
            write_off("the provider stopped reporting the call").await;
            if let Ok(fresh) =
                jarvis_storage::find_phone_call(&self.database, &workspace, call.id()).await
            {
                call = fresh;
            }
        }
        CallView { call, state }
    }

    /// Looks at every call that may still change. Cheap when there is none: one local query.
    pub async fn poll(&self) {
        let Ok(workspace) = self.workspace().await else {
            return;
        };
        let Ok(open) = jarvis_storage::unsettled_phone_calls(&self.database, &workspace).await
        else {
            return;
        };
        if open.is_empty() {
            return;
        }
        let Ok(setup) = self.setup() else {
            return;
        };
        for call in open {
            self.reconcile(&setup, call).await;
        }
    }

    /// Looks at open calls every few seconds for as long as the daemon runs, and removes records past their retention once at start.
    pub async fn run(self: Arc<Self>) {
        let _ = jarvis_storage::prune_phone_calls(&self.database, before(now(), RETENTION)).await;
        let mut ticker = tokio::time::interval(POLL_EVERY);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            ticker.tick().await;
            self.poll().await;
        }
    }

    fn forget_live(&self) {
        *self.live.lock().unwrap_or_else(PoisonError::into_inner) = None;
    }

    /// The call the owner is on right now, if JARVIS placed one to a number on the list and it has been answered.
    async fn live_owner_call(&self, setup: &PhoneSetup) -> Option<StoredPhoneCall> {
        if let Some((looked_at, answer)) =
            &*self.live.lock().unwrap_or_else(PoisonError::into_inner)
            && looked_at.elapsed() < LIVE_CACHE
        {
            return answer.clone();
        }
        let workspace = self.workspace().await.ok()?;
        let mut found = None;
        for call in jarvis_storage::unsettled_phone_calls(&self.database, &workspace)
            .await
            .ok()?
        {
            let call = self.reconcile(setup, call).await.call;
            if call.status() == "in_progress"
                && age(call.created_at(), now()) < LIVE_CALL_WINDOW
                && setup
                    .owner_numbers
                    .iter()
                    .any(|number| number == call.number())
            {
                found = Some(call);
                break;
            }
        }
        *self.live.lock().unwrap_or_else(PoisonError::into_inner) =
            Some((Instant::now(), found.clone()));
        found
    }

    /// Clears every call record.
    ///
    /// # Errors
    ///
    /// Returns a sentence when the records cannot be removed.
    pub async fn clear(&self) -> Result<u64, String> {
        let workspace = self
            .workspace()
            .await
            .map_err(|_| "the records cannot be reached".to_owned())?;
        self.forget_live();
        jarvis_storage::delete_phone_calls(&self.database, &workspace)
            .await
            .map_err(|_| "the records could not be removed".to_owned())
    }

    /// What Settings shows. With `check`, the provider is asked which phone numbers the account has, so a wrong id is found here and not
    /// by the first call.
    pub async fn status(&self, check: bool) -> Value {
        let setup = self.setup();
        let workspace = self.workspace().await.ok();
        let calls = match &workspace {
            Some(workspace) => {
                jarvis_storage::list_phone_calls(&self.database, workspace, RECENT_CALLS)
                    .await
                    .unwrap_or_default()
            }
            None => Vec::new(),
        };
        let mut body = json!({
            "ready": setup.is_ok(),
            "missing": setup.as_ref().err(),
            "calls": calls.iter().map(describe_stored).collect::<Vec<_>>(),
            "limits": { "per_hour": MAX_CALLS_PER_HOUR, "per_day": MAX_CALLS_PER_DAY },
        });
        if let Ok(setup) = &setup {
            body["numbers"] = json!(
                setup
                    .owner_numbers
                    .iter()
                    .map(|number| mask_phone_number(number))
                    .collect::<Vec<_>>()
            );
            body["phone_approves"] = json!(setup.approves);
            if check {
                body["check"] = self.check(setup).await;
            }
        }
        body
    }

    /// Asks the provider whether the agent and the phone number exist and fit together.
    async fn check(&self, setup: &PhoneSetup) -> Value {
        let client = match self.client(setup) {
            Ok(client) => client,
            Err(error) => return json!({ "ok": false, "problem": error.to_string() }),
        };
        match client.phone_numbers().await {
            Err(error) => json!({ "ok": false, "problem": error.to_string() }),
            Ok(numbers) => {
                let Some(ours) = numbers.iter().find(|number| number.id == setup.number_id) else {
                    return json!({
                        "ok": false,
                        "problem": "the phone number id is not one of the numbers in the ElevenLabs account",
                        "numbers": numbers.iter().map(describe_number).collect::<Vec<_>>(),
                    });
                };
                let supported = ours.provider != NumberProvider::Other;
                let assigned = ours.agent_id.as_deref() == Some(setup.agent_id.as_str());
                let problem = if supported {
                    None
                } else {
                    Some(
                        "that number is attached through a provider JARVIS cannot call with (Twilio and SIP trunks work)",
                    )
                };
                json!({
                    "ok": supported,
                    "problem": problem,
                    "number": describe_number(ours),
                    "answers_with_this_agent": assigned,
                    "numbers": numbers.iter().map(describe_number).collect::<Vec<_>>(),
                })
            }
        }
    }
}

#[async_trait]
impl PhoneApproval for PhoneService {
    async fn approves(&self, tool: &str, risk: u8) -> Option<PhoneYes> {
        // Never for the phone tools: a phone call must not approve the next phone call.
        if tool.starts_with("jarvis.voice.") || risk > MAX_PHONE_APPROVAL_RISK {
            return None;
        }
        let setup = self.setup().ok().filter(|setup| setup.approves)?;
        let call = self.live_owner_call(&setup).await?;
        Some(PhoneYes {
            call_id: call.id().to_owned(),
        })
    }
}

/// The number a call rings: the one named, if it is on the owner's list, or the first on the list. A model names a reason, never a number.
fn choose_number(setup: &PhoneSetup, to: Option<&str>) -> Result<String, StartError> {
    let Some(text) = to.map(str::trim).filter(|text| !text.is_empty()) else {
        return Ok(setup.owner_numbers[0].clone());
    };
    let number = normalize_phone_number(text)
        .map_err(|why| StartError::NotAllowed(format!("that is not a usable number: {why}")))?;
    if setup.owner_numbers.contains(&number) {
        Ok(number)
    } else {
        Err(StartError::NotAllowed(
            "that number is not on the owner's list of numbers JARVIS may call (Settings, Phone); only the owner can add one".to_owned(),
        ))
    }
}

/// The agent's first sentence when JARVIS writes it: names JARVIS as an AI and says why it calls.
fn opening_line(reason: &str) -> String {
    let short: String = reason.chars().take(200).collect();
    format!(
        "Hello, this is JARVIS, your AI assistant. I am calling because: {}",
        short.trim().trim_end_matches(['.', '!', '?'])
    ) + "."
}

fn describe_number(number: &jarvis_voice::phone::PhoneNumber) -> Value {
    json!({
        "id": number.id,
        "number": mask_phone_number(&number.number),
        "label": number.label,
        "provider": match number.provider {
            NumberProvider::Twilio => "twilio",
            NumberProvider::SipTrunk => "sip_trunk",
            NumberProvider::Other => "other",
        },
        "agent_id": number.agent_id,
    })
}

/// A stored call as the console shows it: the number masked, the reason and the provider's summary as text.
#[must_use]
pub fn describe_stored(call: &StoredPhoneCall) -> Value {
    json!({
        "call_id": call.id(),
        "to": mask_phone_number(call.number()),
        "reason": call.reason(),
        "urgent": call.urgent(),
        "status": call.status(),
        "placed_at": call.created_at().to_string(),
        "duration_seconds": call.duration_seconds(),
        "ended_because": call.ended_because(),
        "summary": call.summary(),
        "error": call.error(),
    })
}

/// The transcript as one block of text with who said each line, for the model to read as data (the caller fences it).
#[must_use]
pub fn transcript_text(state: &CallState) -> String {
    state
        .transcript
        .iter()
        .map(|line| {
            let who = match line.speaker {
                Speaker::Agent => "JARVIS",
                Speaker::Person => "Owner",
            };
            format!("[{}s] {who}: {}", line.at_seconds, line.text)
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// The authenticated routes, mounted under /api/v1.
pub fn routes() -> axum::Router<GatewayState> {
    use axum::routing::{delete, get, post};
    axum::Router::new()
        .route("/phone", get(status))
        .route("/phone/test", post(test_call))
        .route("/phone/calls", delete(clear))
}

fn unavailable() -> Response {
    error_response(
        StatusCode::NOT_IMPLEMENTED,
        ErrorCode::Unsupported,
        "phone calls are not available on this daemon",
    )
}

/// `GET /api/v1/phone`, and `GET /api/v1/phone?check=1` to also ask the provider about the agent and the number.
async fn status(State(state): State<GatewayState>, RawQuery(query): RawQuery) -> Response {
    let check = query
        .as_deref()
        .is_some_and(|query| query.split('&').any(|pair| pair == "check=1"));
    match state.phone() {
        Some(phone) => Json(phone.status(check).await).into_response(),
        None => unavailable(),
    }
}

/// `POST /api/v1/phone/test`: the owner pressed "Call me now", so the call is the owner's own request and needs no approval. Every other
/// rule (the list of numbers, the rate bound, one open call) still applies.
async fn test_call(State(state): State<GatewayState>) -> Response {
    let Some(phone) = state.phone() else {
        return unavailable();
    };
    match phone
        .start_call(
            "This is a test call that you asked for in JARVIS settings.",
            true,
            None,
        )
        .await
    {
        Ok(call) => Json(describe_stored(&call)).into_response(),
        Err(StartError::NotSetUp(words) | StartError::NotAllowed(words)) => error_response(
            StatusCode::UNPROCESSABLE_ENTITY,
            ErrorCode::Validation,
            &words,
        ),
        Err(StartError::Provider(error)) => error_response(
            StatusCode::BAD_GATEWAY,
            ErrorCode::TransientUpstream,
            &error.to_string(),
        ),
        Err(StartError::Database) => error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            ErrorCode::Internal,
            "the call could not be recorded",
        ),
    }
}

/// `DELETE /api/v1/phone/calls`
async fn clear(State(state): State<GatewayState>) -> Response {
    let Some(phone) = state.phone() else {
        return unavailable();
    };
    match phone.clear().await {
        Ok(removed) => Json(json!({ "removed": removed })).into_response(),
        Err(words) => error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            ErrorCode::Internal,
            &words,
        ),
    }
}

#[cfg(test)]
#[path = "phone_service_tests.rs"]
pub(crate) mod tests;
