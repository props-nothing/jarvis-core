use std::{fmt, future::Future, pin::Pin};

use bytes::Bytes;
use futures_util::{Stream, StreamExt};
use serde::Serialize;

use crate::{MAX_AUDIO_BYTES, MAX_TEXT_CHARS, SpeechError};

/// A provider operation with no HTTP or SDK types at the boundary.
pub type SpeechFuture<'a> =
    Pin<Box<dyn Future<Output = Result<SpeechStream, SpeechError>> + Send + 'a>>;

/// Public voice metadata. Never includes a key, key path or endpoint.
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct SpeechDescription {
    /// Configured provider identity, also used for sentence continuity.
    pub id: String,
    /// Adapter kind.
    pub provider: String,
    /// Voice identifier.
    pub voice: String,
    /// Synthesis model.
    pub model: String,
}

/// Observed availability, not a promise that the next request will succeed.
#[derive(Clone, Debug, Serialize)]
pub struct SpeechStatus {
    /// Public adapter metadata.
    #[serde(flatten)]
    pub description: SpeechDescription,
    /// unknown, healthy, `cooling_down`, or probing.
    pub health: &'static str,
    /// Seconds until a recovery probe may be attempted.
    pub retry_after_seconds: u64,
}

/// Observed local inference execution, without installation paths or SDK diagnostics.
#[derive(Clone, Debug, Serialize)]
pub struct SpeechExecution {
    /// Actual preparation/inference stage. No estimated percentage.
    pub phase: &'static str,
    /// auto before first GPU probe, otherwise cpu, amd or nvidia.
    pub backend: String,
    /// Actual device name, or a pending-selection label.
    pub device: String,
    /// Bytes allocated by `PyTorch` on the selected GPU, not total board usage.
    pub allocated_bytes: u64,
    /// Peak `PyTorch` GPU allocation since the worker loaded.
    pub peak_bytes: u64,
    /// Fixed, safe explanation when CPU was selected after GPU failure.
    pub fallback_reason: Option<&'static str>,
}

/// Replaceable synthesis capability. Implementations keep credentials private.
pub trait SpeechProvider: fmt::Debug + Send + Sync {
    /// Incremental synthetic design audio, followed by one validated saved reference.
    fn design_stream<'a>(
        &'a self,
        _description: &'a str,
        _sample: &'a str,
        _seed: u32,
        _quality: jarvis_core::VoiceQuality,
    ) -> DesignFuture<'a> {
        Box::pin(async {
            Err(SpeechError::Settings(
                "this provider cannot stream voice designs",
            ))
        })
    }
    /// Last observed local execution, if supported; never starts a worker.
    fn execution(&self) -> Option<SpeechExecution> {
        None
    }
    /// Cheap local setup check for the editor; inference still verifies assets before use.
    /// Does not start processes, download files or expose private paths.
    fn design_unavailable_reason(&self) -> Option<&'static str> {
        None
    }
    /// A bounded first-audio budget; local description models require longer CPU inference.
    fn synthesis_timeout(&self) -> std::time::Duration {
        crate::REQUEST_TIMEOUT
    }
    /// Generate a new synthetic identity from text, without accepting uploaded recordings.
    fn design<'a>(
        &'a self,
        _description: &'a str,
        _sample: &'a str,
        _seed: u32,
    ) -> Pin<Box<dyn Future<Output = Result<jarvis_core::DesignedVoice, SpeechError>> + Send + 'a>>
    {
        Box::pin(async {
            Err(SpeechError::Settings(
                "this provider cannot design a voice from a description",
            ))
        })
    }
    /// Build an immutable voice variant while retaining the configured destination and credentials.
    ///
    /// # Errors
    /// Refuses unsupported controls or adapters that do not implement voice editing.
    fn configured(
        &self,
        _voice: &jarvis_core::SavedVoice,
    ) -> Result<std::sync::Arc<dyn SpeechProvider>, SpeechError> {
        Err(SpeechError::Settings(
            "this adapter does not support voice editing",
        ))
    }
    /// Prepare optional local assets and inference for Settings or explicit live diagnosis.
    fn prepare(&self) -> Pin<Box<dyn Future<Output = Result<(), SpeechError>> + Send + '_>> {
        Box::pin(async { Ok(()) })
    }
    /// Safe display metadata.
    fn describe(&self) -> SpeechDescription;
    /// Start a bounded audio stream. Errors never contain upstream response bodies.
    fn open<'a>(&'a self, text: &'a str, previous_text: Option<&'a str>) -> SpeechFuture<'a>;
    /// Prefer the voice already selected for this answer, falling back only on failure.
    fn open_preferred<'a>(
        &'a self,
        text: &'a str,
        previous_text: Option<&'a str>,
        _preferred: Option<&'a str>,
    ) -> SpeechFuture<'a> {
        self.open(text, previous_text)
    }
    /// Availability of individual providers.
    fn statuses(&self) -> Vec<SpeechStatus> {
        vec![SpeechStatus {
            description: self.describe(),
            health: "unknown",
            retry_after_seconds: 0,
        }]
    }
    /// Last provider that delivered a first chunk; absent before first use.
    fn active(&self) -> Option<String> {
        None
    }
}

/// Complete encoded audio and its actual media type.
#[derive(Clone, Debug)]
pub struct SpeechAudio {
    bytes: Bytes,
    content_type: &'static str,
}

impl SpeechAudio {
    /// Encoded bytes.
    #[must_use]
    pub fn bytes(&self) -> &Bytes {
        &self.bytes
    }
    /// MIME type of the encoded audio.
    #[must_use]
    pub const fn content_type(&self) -> &'static str {
        self.content_type
    }
}

/// Provider-neutral audio chunks, capped across the entire stream.
pub struct SpeechStream {
    source: Pin<Box<dyn Stream<Item = Result<Bytes, SpeechError>> + Send>>,
    received: usize,
    finished: bool,
    content_type: &'static str,
    pub(crate) selection: Option<(String, bool)>,
}

impl fmt::Debug for SpeechStream {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SpeechStream")
            .field("content_type", &self.content_type)
            .field("received", &self.received)
            .finish_non_exhaustive()
    }
}

impl SpeechStream {
    /// Buffer a bounded response for clients that require a complete encoded file.
    ///
    /// # Errors
    /// Refuses failed, malformed or oversized streams without returning partial audio.
    pub async fn buffered(self) -> Result<Self, SpeechError> {
        let selection = self.selection.clone();
        let audio = collect_audio(self).await?;
        let mut stream = Self::new(
            futures_util::stream::once(async move { Ok(audio.bytes) }),
            audio.content_type,
        );
        stream.selection = selection;
        Ok(stream)
    }
    /// Wrap an adapter's encoded audio with the shared cumulative size limit.
    pub fn new(
        source: impl Stream<Item = Result<Bytes, SpeechError>> + Send + 'static,
        content_type: &'static str,
    ) -> Self {
        Self {
            source: Box::pin(source),
            received: 0,
            finished: false,
            content_type,
            selection: None,
        }
    }

    /// The next nonempty piece, or `None` at EOF. An error permanently ends the stream.
    ///
    /// # Errors
    /// Reports transport failure, empty output or audio exceeding the shared cap.
    pub async fn next_chunk(&mut self) -> Result<Option<Bytes>, SpeechError> {
        if self.finished {
            return Ok(None);
        }
        loop {
            match self.source.next().await {
                Some(Ok(chunk)) if chunk.is_empty() => {}
                Some(Ok(chunk)) if chunk.len() <= MAX_AUDIO_BYTES.saturating_sub(self.received) => {
                    self.received += chunk.len();
                    return Ok(Some(chunk));
                }
                item => {
                    self.finished = true;
                    // Drop the connection/generator as soon as it becomes unusable.
                    self.source = Box::pin(futures_util::stream::empty());
                    return match item {
                        Some(Err(error)) => Err(error),
                        None if self.received > 0 => Ok(None),
                        _ => Err(SpeechError::BadAudio),
                    };
                }
            }
        }
    }

    /// The media type selected by the adapter.
    #[must_use]
    pub const fn content_type(&self) -> &'static str {
        self.content_type
    }

    /// Provider selected by the fallback chain, and whether it is a backup.
    #[must_use]
    pub fn selection(&self) -> Option<(&str, bool)> {
        self.selection
            .as_ref()
            .map(|(id, backup)| (id.as_str(), *backup))
    }
}

/// Collect one bounded stream (setup and live diagnosis).
///
/// # Errors
/// Returns the stream's normalized failure without partial audio.
pub async fn collect_audio(mut stream: SpeechStream) -> Result<SpeechAudio, SpeechError> {
    let mut audio = Vec::new();
    while let Some(chunk) = stream.next_chunk().await? {
        audio.extend_from_slice(&chunk);
    }
    if stream.content_type() == PCM_CONTENT_TYPE {
        return Ok(SpeechAudio {
            bytes: pcm_wav(&audio)?.into(),
            content_type: "audio/wav",
        });
    }
    Ok(SpeechAudio {
        bytes: Bytes::from(audio),
        content_type: stream.content_type(),
    })
}

/// Fixed PCM contract for incremental local audio; transport splits are arbitrary.
pub const PCM_CONTENT_TYPE: &str = "audio/pcm;rate=48000;channels=1;format=s16le";

/// Design chunks are PCM; only Complete permits saving the generated identity.
pub enum DesignEvent {
    /// A bounded piece of 48 kHz mono s16le audio.
    Audio(Bytes),
    /// Full validated reference, emitted once after successful EOF.
    Complete(jarvis_core::DesignedVoice),
}
/// Owned, cancellable design stream.
pub type DesignStream = Pin<Box<dyn Stream<Item = Result<DesignEvent, SpeechError>> + Send>>;
/// Preparation of a design stream; no audio has escaped before this resolves.
pub type DesignFuture<'a> =
    Pin<Box<dyn Future<Output = Result<DesignStream, SpeechError>> + Send + 'a>>;

pub(crate) fn pcm_wav(pcm: &[u8]) -> Result<Vec<u8>, SpeechError> {
    if pcm.is_empty() || pcm.len() > 96000 * 60 || !pcm.len().is_multiple_of(2) {
        return Err(SpeechError::BadAudio);
    }
    let length = u32::try_from(pcm.len()).map_err(|_| SpeechError::BadAudio)?;
    let mut wav = Vec::with_capacity(pcm.len() + 44);
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(length + 36).to_le_bytes());
    wav.extend_from_slice(b"WAVEfmt \x10\0\0\0\x01\0\x01\0");
    wav.extend_from_slice(&48000_u32.to_le_bytes());
    wav.extend_from_slice(&96000_u32.to_le_bytes());
    wav.extend_from_slice(b"\x02\0\x10\0data");
    wav.extend_from_slice(&length.to_le_bytes());
    wav.extend_from_slice(pcm);
    Ok(wav)
}

pub(crate) fn validate_text(text: &str) -> Result<&str, SpeechError> {
    let text = text.trim();
    if text.is_empty() {
        return Err(SpeechError::EmptyText);
    }
    if text.chars().count() > MAX_TEXT_CHARS {
        return Err(SpeechError::TooLong);
    }
    Ok(text)
}

pub(crate) fn http_client(endpoint: &reqwest::Url) -> Result<reqwest::Client, SpeechError> {
    let host = endpoint.host_str().unwrap_or("");
    let localhost = host == "localhost";
    let loopback = localhost
        || host
            .trim_matches(['[', ']'])
            .parse::<std::net::IpAddr>()
            .is_ok_and(|ip| ip.is_loopback());
    if !matches!(endpoint.scheme(), "https" | "http")
        || (endpoint.scheme() == "http" && !loopback)
        || host.is_empty()
        || !endpoint.username().is_empty()
        || endpoint.password().is_some()
        || endpoint.query().is_some()
        || endpoint.fragment().is_some()
    {
        return Err(SpeechError::Settings(
            "speech API requires HTTPS or loopback HTTP, without URL credentials, query or fragment",
        ));
    }
    let mut client = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(crate::REQUEST_TIMEOUT);
    if localhost {
        client = client.resolve(
            "localhost",
            std::net::SocketAddr::from((
                [127, 0, 0, 1],
                endpoint.port_or_known_default().unwrap_or(80),
            )),
        );
    }
    client
        .build()
        .map_err(|_| SpeechError::Settings("the HTTP client could not be built"))
}

pub(crate) fn mp3_response(response: reqwest::Response) -> Result<SpeechStream, SpeechError> {
    if !response.status().is_success() {
        return Err(match response.status().as_u16() {
            401 | 403 => SpeechError::KeyRejected,
            402 => SpeechError::OutOfCredit,
            429 => SpeechError::OverLimit,
            status => SpeechError::Provider(status),
        });
    }
    let mime = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .split(';')
        .next()
        .unwrap_or("")
        .trim();
    if !["audio/mpeg", "audio/mp3", "application/octet-stream"].contains(&mime)
        || response
            .content_length()
            .is_some_and(|length| length > MAX_AUDIO_BYTES as u64)
    {
        return Err(SpeechError::BadAudio);
    }
    Ok(SpeechStream::new(
        response
            .bytes_stream()
            .map(|item| item.map_err(|_| SpeechError::Unreachable)),
        "audio/mpeg",
    ))
}
