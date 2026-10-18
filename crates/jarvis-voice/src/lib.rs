//! Provider-neutral speech synthesis through configured cloud or local servers; and `ElevenLabs` agent calls ([`phone`]).
//!
//! The provider's key is held here, in the daemon, and never reaches a browser or any other client: the page asks the
//! daemon to speak and gets audio back. See `docs/research/integrations/elevenlabs.md` (the text-to-speech section) for
//! the sources behind each decision and `docs/adr/0138-the-voice-is-synthesized-by-the-daemon.md` for the boundary.

use std::fmt;
use std::time::Duration;

mod description;
mod design;
pub use description::{DESIGN_MODEL_REVISION, DescriptionSpeech};
#[cfg(feature = "local-tts")]
mod embedded;
mod fallback;
mod openai;
mod settings;
pub use design::{BLEND_MODEL_REVISION, default_voice_blend};
mod speech;
#[cfg(feature = "local-tts")]
pub use embedded::EmbeddedSpeech;
pub use fallback::FallbackSpeech;
pub use openai::OpenAiCompatibleSpeech;
pub use settings::{VoiceControl, validate_voice, voice_controls, voice_models};
pub use speech::{
    DesignEvent, DesignFuture, DesignStream, PCM_CONTENT_TYPE, SpeechAudio, SpeechDescription,
    SpeechExecution, SpeechFuture, SpeechProvider, SpeechStatus, SpeechStream, collect_audio,
};

pub mod phone;

/// The most characters one request may speak. A long answer is spoken in several requests, which also lets the
/// first sentence start playing while the rest is still being made.
pub const MAX_TEXT_CHARS: usize = 1_500;

/// The most audio bytes accepted back for one request, so a misbehaving endpoint cannot fill memory.
pub const MAX_AUDIO_BYTES: usize = 8 * 1024 * 1024;
/// The most of the preceding sentence sent for continuity: the provider needs the tone of the last words, not the paragraph.
pub const MAX_PREVIOUS_CHARS: usize = 300;

/// The provider endpoint used unless a test or a regional deployment names another.
pub const ELEVENLABS_BASE_URL: &str = "https://api.elevenlabs.io";

/// The model used unless configuration names another: the provider's real-time expressive model.
pub const DEFAULT_MODEL: &str = "eleven_v4_turbo";

/// The owner's preferred voice, used unless configuration names another.
pub const DEFAULT_VOICE_ID: &str = "liN4AsrhF3CylqMDWK6p";

const REQUEST_TIMEOUT: Duration = Duration::from_secs(25);

/// Why speech could not be produced. None of these carries the key or the provider's raw body.
#[derive(Debug, thiserror::Error)]
pub enum SpeechError {
    /// The text was empty after trimming.
    #[error("there is nothing to say")]
    EmptyText,
    /// The text was longer than [`MAX_TEXT_CHARS`].
    #[error("the text is longer than {MAX_TEXT_CHARS} characters; speak it in parts")]
    TooLong,
    /// A setting was not usable (a malformed key, voice id, model or address).
    #[error("the speech settings are not usable: {0}")]
    Settings(&'static str),
    /// The provider could not be reached or did not answer in time.
    #[error("the speech provider could not be reached")]
    Unreachable,
    /// The provider refused the key (a 401 or 403).
    #[error("the speech provider rejected the key; check the key file")]
    KeyRejected,
    /// The provider reported a rate limit (HTTP 429).
    #[error("the speech provider says the account is out of credit or rate limited")]
    OverLimit,
    /// Local bounded admission is full; retry without treating the provider as unhealthy.
    #[error("the speech engine is busy; wait for current speech to finish")]
    Busy,
    /// The provider explicitly reported exhausted credit (HTTP 402).
    #[error("the speech provider account is out of credit")]
    OutOfCredit,
    /// The provider answered with another failure status.
    #[error("the speech provider answered with status {0}")]
    Provider(u16),
    /// The audio was larger than [`MAX_AUDIO_BYTES`] or empty.
    #[error("the speech provider returned unusable audio")]
    BadAudio,
}

/// A key, kept out of `Debug` output so it cannot reach a log or an error.
#[derive(Clone)]
struct ApiKey(String);

impl fmt::Debug for ApiKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("ApiKey(<redacted>)")
    }
}

/// Speech synthesis through the text-to-speech endpoint of `ElevenLabs`.
#[derive(Clone, Debug)]
pub struct ElevenLabsSpeech {
    client: reqwest::Client,
    base_url: String,
    voice_id: String,
    model_id: String,
    key: ApiKey,
    settings: jarvis_core::VoiceSettings,
}

impl ElevenLabsSpeech {
    /// Builds the adapter.
    ///
    /// # Errors
    ///
    /// Returns [`SpeechError::Settings`] for a key with whitespace or non-ASCII (it would split or corrupt a header),
    /// a voice id or model that is not a plain identifier (it becomes part of a URL path or body), or an address that
    /// is not an `http` or `https` origin.
    pub fn new(
        base_url: &str,
        voice_id: &str,
        model_id: &str,
        key: &str,
    ) -> Result<Self, SpeechError> {
        let key = key.trim();
        if key.is_empty() || key.len() > 256 || !key.bytes().all(|byte| byte.is_ascii_graphic()) {
            return Err(SpeechError::Settings("the key is empty or malformed"));
        }
        let plain = |value: &str| {
            !value.is_empty()
                && value.len() <= 64
                && value
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
        };
        if !plain(voice_id) {
            return Err(SpeechError::Settings(
                "the voice id must be letters, digits, '-' or '_'",
            ));
        }
        if !plain(model_id) {
            return Err(SpeechError::Settings(
                "the model must be letters, digits, '-' or '_'",
            ));
        }
        let base = base_url.trim_end_matches('/');
        let endpoint = reqwest::Url::parse(base)
            .map_err(|_| SpeechError::Settings("invalid speech origin"))?;
        if endpoint.path() != "/" {
            return Err(SpeechError::Settings(
                "ElevenLabs requires an origin without a path",
            ));
        }
        let client = speech::http_client(&endpoint)?;
        Ok(Self {
            client,
            base_url: base.to_owned(),
            voice_id: voice_id.to_owned(),
            model_id: model_id.to_owned(),
            key: ApiKey(key.to_owned()),
            settings: jarvis_core::VoiceSettings::default(),
        })
    }

    /// The voice speech is made in, for display.
    #[must_use]
    pub fn voice_id(&self) -> &str {
        &self.voice_id
    }

    /// The model speech is made with, for display.
    #[must_use]
    pub fn model_id(&self) -> &str {
        &self.model_id
    }

    /// Turns text into audio, all of it, before returning.
    ///
    /// # Errors
    ///
    /// Returns a [`SpeechError`]: bad text, an unreachable or refusing provider, or unusable audio.
    pub async fn synthesize(&self, text: &str) -> Result<SpeechAudio, SpeechError> {
        collect_audio(self.open(text, None).await?).await
    }

    /// Starts synthesis and returns once the provider has accepted it, with the audio still arriving.
    ///
    /// The provider's `/stream` endpoint sends audio as it is generated, so the first chunk can be played long before the last
    /// exists; that is the difference between waiting for a sentence to be made and hearing it begin (about 0.2 s against about
    /// 0.6 s for a short sentence on `eleven_v4_turbo`, measured). `previous_text` is the sentence spoken just before, which the
    /// provider uses to keep the voice's intonation continuous across separate requests. It is left out for the `eleven_v3`
    /// models, which refuse it (a 400 `unsupported_model`, verified), and trimmed to the last [`MAX_PREVIOUS_CHARS`] characters.
    ///
    /// # Errors
    ///
    /// Returns a [`SpeechError`] for bad text or when the provider does not accept the request. A failure after this returns is
    /// reported by [`SpeechStream::next_chunk`].
    pub async fn open(
        &self,
        text: &str,
        previous_text: Option<&str>,
    ) -> Result<SpeechStream, SpeechError> {
        let text = speech::validate_text(text)?;
        let url = format!(
            "{}/v1/text-to-speech/{}/stream",
            self.base_url, self.voice_id
        );
        let mut body = serde_json::json!({ "text": text, "model_id": self.model_id });
        if self.settings != jarvis_core::VoiceSettings::default() {
            body["voice_settings"] = serde_json::json!(self.settings);
        }
        if let Some(previous) = previous_text
            .map(str::trim)
            .filter(|value| !value.is_empty())
            && !self.model_id.starts_with("eleven_v3")
        {
            let skip = previous.chars().count().saturating_sub(MAX_PREVIOUS_CHARS);
            body["previous_text"] =
                serde_json::Value::String(previous.chars().skip(skip).collect());
        }
        let response = self
            .client
            .post(url)
            .header("xi-api-key", &self.key.0)
            .header(reqwest::header::ACCEPT, "audio/mpeg")
            .json(&body)
            .send()
            .await
            .map_err(|_| SpeechError::Unreachable)?;
        speech::mp3_response(response)
    }
}

impl SpeechProvider for ElevenLabsSpeech {
    fn configured(
        &self,
        voice: &jarvis_core::SavedVoice,
    ) -> Result<std::sync::Arc<dyn SpeechProvider>, SpeechError> {
        validate_voice("elevenlabs", voice)?;
        let mut adapter = Self::new(&self.base_url, &voice.voice, &voice.model, &self.key.0)?;
        adapter.client = self.client.clone();
        adapter.settings = voice.settings.clone();
        Ok(std::sync::Arc::new(adapter))
    }
    fn describe(&self) -> SpeechDescription {
        SpeechDescription {
            id: "elevenlabs".into(),
            provider: "elevenlabs".into(),
            voice: self.voice_id.clone(),
            model: self.model_id.clone(),
        }
    }
    fn open<'a>(&'a self, text: &'a str, previous_text: Option<&'a str>) -> SpeechFuture<'a> {
        Box::pin(ElevenLabsSpeech::open(self, text, previous_text))
    }
}

#[cfg(test)]
mod tests;
