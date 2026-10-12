//! Phone calls through an `ElevenLabs` agent: place one to a number, and read how it went.
//!
//! The provider runs the call (the telephony, the speech and the agent's turn-taking); JARVIS decides **whether** a call may be
//! placed and to whom, and keeps the record. The key stays in the daemon, like the speech key. See
//! `docs/research/integrations/elevenlabs.md` (the telephony section) for the sources behind each field.
//!
//! Nothing here retries a placement: a second request would ring the owner's phone twice. Every error is worded without the key and
//! without the provider's raw body; the one provider sentence kept (why a request was refused) is cut to a short length and has the
//! key removed.

use std::fmt;
use std::time::Duration;

use serde_json::{Value, json};

use crate::{ApiKey, ELEVENLABS_BASE_URL};

/// The most characters of a provider sentence kept in an error.
const MAX_REASON_CHARS: usize = 160;
/// The most bytes read from one provider answer, so a misbehaving endpoint cannot fill memory.
const MAX_ANSWER_BYTES: usize = 2 * 1024 * 1024;
/// The most characters kept of one spoken line when a transcript is read back.
const MAX_LINE_CHARS: usize = 600;
/// The most lines kept of a transcript.
pub const MAX_TRANSCRIPT_LINES: usize = 120;
/// The most characters kept of the provider's summary of a call.
pub const MAX_SUMMARY_CHARS: usize = 1_500;
/// How long the phone rings before the provider gives up.
pub const RINGING_TIMEOUT_SECONDS: u32 = 45;

const REQUEST_TIMEOUT: Duration = Duration::from_secs(20);

/// Why a phone request failed. None of these carries the key or the provider's raw body.
#[derive(Debug, thiserror::Error)]
pub enum PhoneError {
    /// A setting was not usable (a malformed key, identifier, number or address).
    #[error("the phone settings are not usable: {0}")]
    Settings(&'static str),
    /// The provider could not be reached or did not answer in time.
    #[error("the phone provider could not be reached")]
    Unreachable,
    /// The provider refused the key (a 401 or 403). A key limited to speech cannot place calls.
    #[error(
        "the phone provider rejected the key; it needs access to the agents platform (conversations and phone numbers)"
    )]
    KeyRejected,
    /// The account is out of credit or over a rate limit (a 402 or 429).
    #[error("the phone provider says the account is out of credit or rate limited")]
    OverLimit,
    /// The phone number id is not one of the account's phone numbers.
    #[error("the phone number id is not one of the numbers in the ElevenLabs account")]
    NumberNotFound,
    /// The number belongs to a provider this adapter does not place calls through.
    #[error(
        "the phone number is attached through a provider JARVIS cannot call with (Twilio and SIP trunks are supported)"
    )]
    UnsupportedProvider,
    /// The provider could not find the conversation.
    #[error("the provider has no call with that id")]
    CallNotFound,
    /// The provider refused the request as invalid, with its (shortened) reason.
    #[error("the phone provider refused the request: {0}")]
    Invalid(String),
    /// The provider accepted the request but did not start the call, with its (shortened) reason.
    #[error("the phone provider did not start the call: {0}")]
    NotStarted(String),
    /// The provider answered with another failure status.
    #[error("the phone provider answered with status {0}")]
    Provider(u16),
    /// The answer was not what the documentation describes.
    #[error("the phone provider's answer could not be understood")]
    BadAnswer,
    /// The placement request was sent and no usable answer came back, so the phone may be ringing. Never retried automatically.
    #[error("the call request was sent but no answer came back; the phone may still ring")]
    Uncertain,
}

/// The provider a phone number is attached through, which decides the endpoint that places the call.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NumberProvider {
    /// A Twilio number.
    Twilio,
    /// A SIP trunk.
    SipTrunk,
    /// Any other provider, which this adapter does not call through.
    Other,
}

/// One phone number of the account.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PhoneNumber {
    /// The identifier a call is placed from.
    pub id: String,
    /// The number, as the provider shows it.
    pub number: String,
    /// The label the owner gave it.
    pub label: String,
    /// How it is attached.
    pub provider: NumberProvider,
    /// The agent that answers it, when one is assigned.
    pub agent_id: Option<String>,
}

/// What to ask the agent to do on one call.
#[derive(Clone, Debug)]
pub struct CallRequest {
    /// The number to ring, in international format (`+31...`), already checked by the caller.
    pub to: String,
    /// Values the agent's prompt can use as `{{name}}`.
    pub variables: Vec<(String, String)>,
    /// The first thing the agent says, when the agent allows it to be overridden. `None` leaves the agent's own.
    pub first_message: Option<String>,
}

/// A call the provider accepted.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlacedCall {
    /// The provider's conversation id, which every later question about the call uses.
    pub conversation_id: String,
    /// The telephony provider's own id for the call, when given.
    pub call_id: Option<String>,
}

/// How far a call has got.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CallStatus {
    /// Placed and ringing, or connecting.
    Initiated,
    /// Answered and talking.
    InProgress,
    /// Hung up; the provider is analysing it.
    Processing,
    /// Finished and analysed.
    Done,
    /// The call could not be made or broke.
    Failed,
}

impl CallStatus {
    /// The word stored and shown.
    #[must_use]
    pub const fn word(self) -> &'static str {
        match self {
            Self::Initiated => "initiated",
            Self::InProgress => "in_progress",
            Self::Processing => "processing",
            Self::Done => "done",
            Self::Failed => "failed",
        }
    }

    /// Reads a stored word.
    #[must_use]
    pub fn from_word(word: &str) -> Option<Self> {
        match word {
            "initiated" => Some(Self::Initiated),
            "in_progress" | "in-progress" => Some(Self::InProgress),
            "processing" => Some(Self::Processing),
            "done" => Some(Self::Done),
            "failed" => Some(Self::Failed),
            _ => None,
        }
    }

    /// Whether the line may still be open: someone could be speaking to the agent.
    #[must_use]
    pub const fn is_live(self) -> bool {
        matches!(self, Self::Initiated | Self::InProgress)
    }

    /// Whether nothing more will change.
    #[must_use]
    pub const fn is_final(self) -> bool {
        matches!(self, Self::Done | Self::Failed)
    }
}

/// Who spoke a line.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Speaker {
    /// The agent (JARVIS's voice).
    Agent,
    /// The person on the phone.
    Person,
}

/// One line of a call, as the provider transcribed it. It is what a person said into a phone, so it is data and never an instruction.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Line {
    /// Who spoke.
    pub speaker: Speaker,
    /// Seconds into the call.
    pub at_seconds: u64,
    /// What was said, cut to a bounded length.
    pub text: String,
}

/// What the provider knows about one call.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CallState {
    /// How far the call has got.
    pub status: CallStatus,
    /// Seconds the call lasted, when it has.
    pub duration_seconds: Option<u64>,
    /// Why it ended, in the provider's words, bounded.
    pub ended_because: Option<String>,
    /// The provider's summary, once it has analysed the call.
    pub summary: Option<String>,
    /// The transcript so far, bounded.
    pub transcript: Vec<Line>,
}

/// Phone calls through the agents platform of `ElevenLabs`.
#[derive(Clone)]
pub struct ElevenLabsPhone {
    client: reqwest::Client,
    base_url: String,
    agent_id: String,
    number_id: String,
    key: ApiKey,
}

impl fmt::Debug for ElevenLabsPhone {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ElevenLabsPhone")
            .field("base_url", &self.base_url)
            .field("agent_id", &self.agent_id)
            .field("number_id", &self.number_id)
            .field("key", &self.key)
            .finish_non_exhaustive()
    }
}

fn plain(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
}

impl ElevenLabsPhone {
    /// Builds the adapter.
    ///
    /// # Errors
    ///
    /// Returns [`PhoneError::Settings`] for a key with whitespace or non-ASCII, an agent or number id that is not a plain identifier
    /// (it becomes part of a request body), or an address that is not an `http` or `https` origin.
    pub fn new(
        base_url: Option<&str>,
        key: &str,
        agent_id: &str,
        number_id: &str,
    ) -> Result<Self, PhoneError> {
        let key = key.trim();
        if key.is_empty() || key.len() > 256 || !key.bytes().all(|byte| byte.is_ascii_graphic()) {
            return Err(PhoneError::Settings("the key is empty or malformed"));
        }
        if !plain(agent_id) {
            return Err(PhoneError::Settings(
                "the agent id must be letters, digits, '-' or '_'",
            ));
        }
        if !plain(number_id) {
            return Err(PhoneError::Settings(
                "the phone number id must be letters, digits, '-' or '_'",
            ));
        }
        let base = base_url
            .unwrap_or(ELEVENLABS_BASE_URL)
            .trim_end_matches('/');
        if !(base.starts_with("https://") || base.starts_with("http://"))
            || base.contains(['?', '#', ' '])
        {
            return Err(PhoneError::Settings(
                "the address must be an http(s) origin",
            ));
        }
        let client = reqwest::Client::builder()
            .timeout(REQUEST_TIMEOUT)
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|_| PhoneError::Settings("the HTTP client could not be built"))?;
        Ok(Self {
            client,
            base_url: base.to_owned(),
            agent_id: agent_id.to_owned(),
            number_id: number_id.to_owned(),
            key: ApiKey(key.to_owned()),
        })
    }

    /// The phone numbers of the account.
    ///
    /// # Errors
    ///
    /// Returns a [`PhoneError`] when the provider cannot be reached, refuses the key, or answers something unexpected.
    pub async fn phone_numbers(&self) -> Result<Vec<PhoneNumber>, PhoneError> {
        let answer = self.get("/v1/convai/phone-numbers").await?;
        let items = answer.as_array().ok_or(PhoneError::BadAnswer)?;
        Ok(items.iter().filter_map(read_number).collect())
    }

    /// Places a call from the configured number, as the configured agent, and returns once the provider has accepted it.
    ///
    /// The endpoint depends on how the number is attached, which is looked up first. The call is **not retried**: a request that
    /// timed out may still have started, and a second one would ring the phone again.
    ///
    /// # Errors
    ///
    /// Returns a [`PhoneError`] when the number is unknown or attached through an unsupported provider, the provider refuses the
    /// request, or it accepts it without starting a call.
    pub async fn place_call(&self, request: &CallRequest) -> Result<PlacedCall, PhoneError> {
        let number = self
            .phone_numbers()
            .await?
            .into_iter()
            .find(|number| number.id == self.number_id)
            .ok_or(PhoneError::NumberNotFound)?;
        let (path, call_key) = match number.provider {
            NumberProvider::Twilio => ("/v1/convai/twilio/outbound-call", "callSid"),
            NumberProvider::SipTrunk => ("/v1/convai/sip-trunk/outbound-call", "sip_call_id"),
            NumberProvider::Other => return Err(PhoneError::UnsupportedProvider),
        };
        let mut data = json!({
            "dynamic_variables": request
                .variables
                .iter()
                .map(|(name, value)| (name.clone(), Value::String(value.clone())))
                .collect::<serde_json::Map<_, _>>(),
        });
        if let Some(first) = request
            .first_message
            .as_deref()
            .filter(|text| !text.trim().is_empty())
        {
            data["conversation_config_override"] = json!({ "agent": { "first_message": first } });
        }
        let body = json!({
            "agent_id": self.agent_id,
            "agent_phone_number_id": self.number_id,
            "to_number": request.to,
            "conversation_initiation_client_data": data,
            "telephony_call_config": { "ringing_timeout_secs": RINGING_TIMEOUT_SECONDS },
        });
        let answer = match self.send(self.post(path).json(&body)).await {
            Ok(answer) => answer,
            Err(PhoneError::Unreachable | PhoneError::BadAnswer) => {
                return Err(PhoneError::Uncertain);
            }
            Err(other) => return Err(other),
        };
        if answer.get("success").and_then(Value::as_bool) != Some(true) {
            let reason = answer
                .get("message")
                .and_then(Value::as_str)
                .map_or_else(|| "no reason given".to_owned(), |text| self.shorten(text));
            return Err(PhoneError::NotStarted(reason));
        }
        let conversation_id = answer
            .get("conversation_id")
            .and_then(Value::as_str)
            .filter(|id| plain(id))
            .ok_or(PhoneError::Uncertain)?;
        Ok(PlacedCall {
            conversation_id: conversation_id.to_owned(),
            call_id: answer
                .get(call_key)
                .and_then(Value::as_str)
                .filter(|id| plain(id))
                .map(str::to_owned),
        })
    }

    /// What the provider knows about one call.
    ///
    /// # Errors
    ///
    /// Returns [`PhoneError::CallNotFound`] for an id the provider does not know, and another [`PhoneError`] when it cannot be asked.
    pub async fn call_state(&self, conversation_id: &str) -> Result<CallState, PhoneError> {
        if !plain(conversation_id) {
            return Err(PhoneError::CallNotFound);
        }
        let answer = self
            .get(&format!("/v1/convai/conversations/{conversation_id}"))
            .await?;
        let status = answer
            .get("status")
            .and_then(Value::as_str)
            .and_then(CallStatus::from_word)
            .ok_or(PhoneError::BadAnswer)?;
        let metadata = answer.get("metadata");
        let duration_seconds = metadata
            .and_then(|meta| meta.get("call_duration_secs"))
            .and_then(Value::as_u64);
        let ended_because = metadata
            .and_then(|meta| meta.get("termination_reason"))
            .and_then(Value::as_str)
            .filter(|text| !text.is_empty())
            .map(|text| bounded(text, MAX_REASON_CHARS));
        let summary = answer
            .get("analysis")
            .and_then(|analysis| analysis.get("transcript_summary"))
            .and_then(Value::as_str)
            .filter(|text| !text.trim().is_empty())
            .map(|text| bounded(text, MAX_SUMMARY_CHARS));
        let transcript = answer
            .get("transcript")
            .and_then(Value::as_array)
            .map(|turns| turns.iter().filter_map(read_line).collect::<Vec<_>>())
            .unwrap_or_default();
        let skip = transcript.len().saturating_sub(MAX_TRANSCRIPT_LINES);
        Ok(CallState {
            status,
            duration_seconds,
            ended_because,
            summary,
            transcript: transcript.into_iter().skip(skip).collect(),
        })
    }

    fn post(&self, path: &str) -> reqwest::RequestBuilder {
        self.client
            .post(format!("{}{path}", self.base_url))
            .header("xi-api-key", &self.key.0)
    }

    async fn get(&self, path: &str) -> Result<Value, PhoneError> {
        self.send(
            self.client
                .get(format!("{}{path}", self.base_url))
                .header("xi-api-key", &self.key.0),
        )
        .await
    }

    async fn send(&self, request: reqwest::RequestBuilder) -> Result<Value, PhoneError> {
        let mut response = request.send().await.map_err(|_| PhoneError::Unreachable)?;
        let status = response.status();
        let mut bytes: Vec<u8> = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|_| PhoneError::Unreachable)?
        {
            bytes.extend_from_slice(&chunk);
            if bytes.len() > MAX_ANSWER_BYTES {
                return Err(PhoneError::BadAnswer);
            }
        }
        if !status.is_success() {
            return Err(match status.as_u16() {
                401 | 403 => PhoneError::KeyRejected,
                402 | 429 => PhoneError::OverLimit,
                404 => PhoneError::CallNotFound,
                400 | 422 => PhoneError::Invalid(self.invalid_reason(&bytes)),
                other => PhoneError::Provider(other),
            });
        }
        serde_json::from_slice(&bytes).map_err(|_| PhoneError::BadAnswer)
    }

    /// The provider's reason for refusing a request: the first `detail` message, or nothing useful.
    fn invalid_reason(&self, body: &[u8]) -> String {
        let detail = serde_json::from_slice::<Value>(body)
            .ok()
            .and_then(|value| value.get("detail").cloned());
        let text = match detail {
            Some(Value::String(text)) => Some(text),
            Some(Value::Array(items)) => items
                .iter()
                .find_map(|item| item.get("msg").and_then(Value::as_str).map(str::to_owned)),
            Some(Value::Object(object)) => object
                .get("message")
                .and_then(Value::as_str)
                .map(str::to_owned),
            _ => None,
        };
        text.map_or_else(|| "no reason given".to_owned(), |text| self.shorten(&text))
    }

    /// A provider sentence made safe to show: control characters gone, the key removed, cut short.
    fn shorten(&self, text: &str) -> String {
        bounded(&text.replace(&self.key.0, "[key]"), MAX_REASON_CHARS)
    }
}

fn bounded(text: &str, limit: usize) -> String {
    let cleaned: String = text
        .chars()
        .map(|character| {
            if character.is_control() {
                ' '
            } else {
                character
            }
        })
        .collect();
    let cleaned = cleaned.trim();
    if cleaned.chars().count() <= limit {
        cleaned.to_owned()
    } else {
        let mut cut: String = cleaned.chars().take(limit).collect();
        cut.push('\u{2026}');
        cut
    }
}

fn read_number(item: &Value) -> Option<PhoneNumber> {
    let provider = match item.get("provider")?.as_str()? {
        "twilio" => NumberProvider::Twilio,
        "sip_trunk" => NumberProvider::SipTrunk,
        _ => NumberProvider::Other,
    };
    let id = item.get("phone_number_id")?.as_str()?;
    if !plain(id) {
        return None;
    }
    Some(PhoneNumber {
        id: id.to_owned(),
        number: bounded(item.get("phone_number")?.as_str()?, 32),
        label: bounded(
            item.get("label")
                .and_then(Value::as_str)
                .unwrap_or_default(),
            80,
        ),
        provider,
        agent_id: item
            .get("assigned_agent")
            .and_then(|agent| agent.get("agent_id"))
            .and_then(Value::as_str)
            .map(str::to_owned),
    })
}

fn read_line(turn: &Value) -> Option<Line> {
    let speaker = match turn.get("role")?.as_str()? {
        "agent" => Speaker::Agent,
        "user" => Speaker::Person,
        _ => return None,
    };
    let text = turn.get("message")?.as_str()?;
    if text.trim().is_empty() {
        return None;
    }
    Some(Line {
        speaker,
        at_seconds: turn
            .get("time_in_call_secs")
            .and_then(Value::as_u64)
            .unwrap_or(0),
        text: bounded(text, MAX_LINE_CHARS),
    })
}

#[cfg(test)]
#[path = "phone_tests.rs"]
mod tests;
