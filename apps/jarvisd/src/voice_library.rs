//! Validated, hot-applied voice snapshots and authenticated settings/preview routes.

use std::sync::{Arc, Mutex, RwLock};

use axum::{
    Json,
    extract::{Path, State},
    http::{StatusCode, header},
    response::{IntoResponse, Response},
};
use jarvis_core::{ErrorCode, SavedVoice, VoiceChannels, VoiceLibrary, VoiceSettings};
use jarvis_storage::{AppPaths, voice_library::VoiceLibraryStore};
use jarvis_voice::{
    FallbackSpeech, SpeechDescription, SpeechError, SpeechFuture, SpeechProvider, SpeechStatus,
};
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tokio::sync::{Notify, OwnedSemaphorePermit, Semaphore};

use crate::gateway::{GatewayState, error_response};

type Provider = Arc<dyn SpeechProvider>;

pub(crate) fn routes() -> axum::Router<GatewayState> {
    use axum::routing::{get, post};
    axum::Router::new()
        .route(
            "/speech",
            get(crate::speech_service::status).post(crate::speech_service::speak),
        )
        .route("/settings/voices", get(list).put(save))
        .route("/settings/voices/runtime", get(execution))
        .route("/settings/voices/prepare", post(streaming::prepare))
        .route("/settings/voices/preview", post(preview))
        .route("/settings/voices/design", post(design))
        .route("/settings/voices/cancel", post(cancel))
        .route("/settings/voices/{id}/export", get(export))
}

mod streaming;
#[cfg(test)]
mod tests;

#[derive(Debug)]
struct Snapshot {
    library: VoiceLibrary,
    speech: Option<Arc<FallbackSpeech>>,
}

/// Profile-owned speech router. Native resources stay in the base adapters across edits.
#[derive(Debug)]
pub struct VoiceRuntime {
    providers: Vec<(String, Provider)>,
    store: VoiceLibraryStore,
    state: RwLock<Snapshot>,
    preview: Arc<Semaphore>,
    generated: Mutex<std::collections::VecDeque<[u8; 32]>>,
    active_preview: Mutex<Option<(String, Arc<Notify>)>>,
    cancelled: Mutex<std::collections::VecDeque<String>>,
}

impl VoiceRuntime {
    pub(crate) fn new(
        paths: &AppPaths,
        providers: Vec<(String, Provider)>,
    ) -> Result<Self, SpeechError> {
        let store = VoiceLibraryStore::new(paths);
        let library = store
            .load()
            .map_err(SpeechError::Settings)?
            .unwrap_or_else(|| seed(&providers));
        let speech = compose(&providers, &library)?;
        Ok(Self {
            providers,
            store,
            state: RwLock::new(Snapshot { library, speech }),
            preview: Arc::new(Semaphore::new(1)),
            generated: Mutex::new(std::collections::VecDeque::new()),
            active_preview: Mutex::new(None),
            cancelled: Mutex::new(std::collections::VecDeque::new()),
        })
    }

    fn route(&self) -> Option<Arc<FallbackSpeech>> {
        self.state
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .speech
            .clone()
    }

    fn response(&self) -> Value {
        let library = self
            .state
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .library
            .clone();
        let providers: Vec<_> = self.providers.iter().map(|(id, provider)| {
            let description = provider.describe();
            let mut models: Vec<String> = jarvis_voice::voice_models(&description.provider).iter().map(|m| (*m).to_owned()).collect();
            for model in std::iter::once(&description.model).chain(library.voices.iter().filter(|v| &v.provider == id).map(|v| &v.model)) {
                if !models.contains(model) { models.push(model.clone()); }
            }
            let models: Vec<_> = models.iter().map(|model| json!({"id": model, "controls": jarvis_voice::voice_controls(&description.provider, model)})).collect();
            let presets = if description.provider == "embedded-kokoro" {
                json!([{"id":"af_heart", "name":"Heart · American English"}, {"id":"bm_george", "name":"George · British English"}])
            } else { json!([]) };
            let design = if description.provider == "embedded-kokoro" {
                json!({"recipe": jarvis_voice::default_voice_blend(), "languages": [{"id":"en-us","name":"American English"},{"id":"en-gb","name":"British English"}]})
            } else { Value::Null };
            json!({"id": id, "kind": description.provider, "current": description, "models": models, "presets": presets, "design": design,
                "description_design": description.provider == "local-voxcpm2",
                "design_unavailable_reason": provider.design_unavailable_reason(), "execution": provider.execution()})
        }).collect();
        json!({"library": library, "providers": providers, "max_preview_chars": 300,
            "channels": [{"id":"console", "available":true},
                {"id":"telegram", "available":false, "reason":"Telegram currently sends text only."},
                {"id":"calls", "available":false, "reason":"Phone calls use the voice configured on your ElevenLabs agent."}]})
    }

    fn save(&self, mut library: VoiceLibrary) -> Result<Value, (StatusCode, &'static str)> {
        library
            .validate()
            .map_err(|e| (StatusCode::UNPROCESSABLE_ENTITY, e))?;
        let mut state = self
            .state
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if library.revision != state.library.revision {
            return Err((
                StatusCode::CONFLICT,
                "the voice library changed in another window; reload it before saving",
            ));
        }
        // The API admits only this daemon's generated samples or an unchanged saved design.
        // It does not turn arbitrary uploaded audio into a voice-cloning endpoint.
        for design in library.voices.iter().filter_map(|v| v.design.as_ref()) {
            let existing = state
                .library
                .voices
                .iter()
                .any(|v| v.design.as_ref() == Some(design));
            let generated = design_fingerprint(design).is_ok_and(|hash| {
                self.generated
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .contains(&hash)
            });
            if !existing && !generated {
                return Err((
                    StatusCode::UNPROCESSABLE_ENTITY,
                    "generate this voice in this JARVIS session before saving; uploaded reference audio is unsupported",
                ));
            }
        }
        // Also refuse external replacement instead of overwriting a newer file from a local editor.
        let disk = self
            .store
            .load()
            .map_err(|e| (StatusCode::UNPROCESSABLE_ENTITY, e))?;
        if disk.as_ref().is_some_and(|v| v != &state.library)
            || (disk.is_none() && state.library.revision != 0)
        {
            return Err((
                StatusCode::CONFLICT,
                "the saved voice library changed outside JARVIS; restart before editing",
            ));
        }
        let speech = compose(&self.providers, &library).map_err(|_| (StatusCode::UNPROCESSABLE_ENTITY, "a voice references an unavailable provider, invalid voice/model, or unsupported control"))?;
        library.revision = library
            .revision
            .checked_add(1)
            .ok_or((StatusCode::CONFLICT, "voice library revision exhausted"))?;
        self.store
            .save(&library)
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;
        *state = Snapshot { library, speech };
        drop(state);
        Ok(self.response())
    }

    fn draft(&self, voice: &SavedVoice) -> Result<Provider, SpeechError> {
        if let Some(design) = &voice.design {
            let state = self
                .state
                .read()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let existing = state
                .library
                .voices
                .iter()
                .any(|v| v.design.as_ref() == Some(design));
            let generated = design_fingerprint(design).is_ok_and(|hash| {
                self.generated
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .contains(&hash)
            });
            if !existing && !generated {
                return Err(SpeechError::Settings(
                    "generate this voice before previewing; uploaded reference audio is unsupported",
                ));
            }
        }
        configure(&self.providers, voice)
    }
}

fn seed(providers: &[(String, Provider)]) -> VoiceLibrary {
    let voices: Vec<_> = providers
        .iter()
        .filter(|(_, provider)| provider.describe().provider != "local-voxcpm2")
        .map(|(id, provider)| {
            let info = provider.describe();
            SavedVoice {
                id: id.clone(),
                name: if info.provider == "embedded-kokoro" {
                    "Kokoro local".into()
                } else {
                    id.clone()
                },
                provider: id.clone(),
                model: info.model,
                voice: info.voice,
                settings: VoiceSettings::default(),
                blend: None,
                design: None,
            }
        })
        .collect();
    VoiceLibrary {
        schema_version: 1,
        revision: 0,
        default_voice: voices.first().map(|v| v.id.clone()),
        backups: voices.iter().skip(1).map(|v| v.id.clone()).collect(),
        voices,
        channels: VoiceChannels::default(),
    }
}

fn configure(
    providers: &[(String, Provider)],
    voice: &SavedVoice,
) -> Result<Provider, SpeechError> {
    voice.validate().map_err(SpeechError::Settings)?;
    providers
        .iter()
        .find(|(id, _)| id == &voice.provider)
        .ok_or(SpeechError::Settings(
            "the voice's provider is not configured",
        ))?
        .1
        .configured(voice)
}

fn compose(
    providers: &[(String, Provider)],
    library: &VoiceLibrary,
) -> Result<Option<Arc<FallbackSpeech>>, SpeechError> {
    library.validate().map_err(SpeechError::Settings)?;
    let mut configured = std::collections::BTreeMap::new();
    for voice in &library.voices {
        configured.insert(voice.id.as_str(), configure(providers, voice)?);
    }
    let mut chain = Vec::new();
    for id in library
        .console_voice()
        .into_iter()
        .chain(library.backups.iter().map(String::as_str))
    {
        let voice = library
            .voices
            .iter()
            .find(|v| v.id == id)
            .ok_or(SpeechError::Settings("unknown routed voice"))?;
        // A different voice at the same destination is not an independent outage fallback.
        if chain
            .iter()
            .any(|(provider, _)| provider == &voice.provider)
        {
            continue;
        }
        let adapter = configured
            .get(id)
            .ok_or(SpeechError::Settings("unknown routed voice"))?;
        chain.push((voice.provider.clone(), Arc::clone(adapter)));
    }
    if chain.is_empty() {
        return Ok(None);
    }
    Ok(Some(Arc::new(FallbackSpeech::new(chain)?)))
}

impl SpeechProvider for VoiceRuntime {
    fn describe(&self) -> SpeechDescription {
        self.route().map_or_else(
            || SpeechDescription {
                id: String::new(),
                provider: String::new(),
                voice: String::new(),
                model: String::new(),
            },
            |r| r.describe(),
        )
    }
    fn open<'a>(&'a self, text: &'a str, previous: Option<&'a str>) -> SpeechFuture<'a> {
        self.open_preferred(text, previous, None)
    }
    fn open_preferred<'a>(
        &'a self,
        text: &'a str,
        previous: Option<&'a str>,
        preferred: Option<&'a str>,
    ) -> SpeechFuture<'a> {
        let route = self.route();
        Box::pin(async move {
            route
                .ok_or(SpeechError::Settings("no voice is selected"))?
                .open_preferred(text, previous, preferred)
                .await
        })
    }
    fn statuses(&self) -> Vec<SpeechStatus> {
        self.route().map_or_else(Vec::new, |r| r.statuses())
    }
    fn active(&self) -> Option<String> {
        self.route().and_then(|r| r.active())
    }
}

fn unavailable() -> Response {
    error_response(
        StatusCode::NOT_FOUND,
        ErrorCode::UnavailableCapability,
        "voice settings are not available on this daemon",
    )
}

/// `GET /api/v1/settings/voices`: safe metadata and the saved library.
pub async fn list(State(state): State<GatewayState>) -> Response {
    state.voice_library().map_or_else(unavailable, |runtime| {
        (
            [(header::CACHE_CONTROL, "no-store")],
            Json(runtime.response()),
        )
            .into_response()
    })
}

/// Small authenticated snapshot for progress polling; excludes saved references.
pub async fn execution(State(state): State<GatewayState>) -> Response {
    state.voice_library().map_or_else(unavailable, |runtime| {
        let providers: Vec<_> = runtime
            .providers
            .iter()
            .map(|(id, provider)| json!({"id":id, "execution":provider.execution()}))
            .collect();
        (
            [(header::CACHE_CONTROL, "no-store")],
            Json(json!({"providers":providers})),
        )
            .into_response()
    })
}

/// Export the saved design only, without profile-local provider references or credentials.
pub async fn export(State(state): State<GatewayState>, Path(id): Path<String>) -> Response {
    let Some(runtime) = state.voice_library() else {
        return unavailable();
    };
    let snapshot = runtime
        .state
        .read()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let Some(voice) = snapshot
        .library
        .voices
        .iter()
        .find(|voice| voice.id == id && (voice.blend.is_some() || voice.design.is_some()))
    else {
        return error_response(
            StatusCode::NOT_FOUND,
            ErrorCode::Validation,
            "no saved voice design has this ID",
        );
    };
    ([(header::CACHE_CONTROL, "no-store")], Json(json!({
        "format": "jarvis-voice-design", "version": 1, "name": voice.name,
        "model": voice.model, "voice": voice.voice, "settings": voice.settings, "blend": voice.blend, "design": voice.design
    }))).into_response()
}

/// `PUT /api/v1/settings/voices`: validate, persist and hot-apply a complete revision.
pub async fn save(
    State(state): State<GatewayState>,
    Json(library): Json<VoiceLibrary>,
) -> Response {
    let Some(runtime) = state.voice_library() else {
        return unavailable();
    };
    match runtime.save(library) {
        Ok(body) => ([(header::CACHE_CONTROL, "no-store")], Json(body)).into_response(),
        Err((status, message)) => error_response(status, ErrorCode::Validation, message),
    }
}

/// Exact unsaved draft and a short sample. No fallback, persistence or key fields.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PreviewRequest {
    voice: SavedVoice,
    text: String,
    request_id: Option<String>,
    #[serde(default)]
    stream: bool,
}

/// `POST /api/v1/settings/voices/preview`: render the exact draft or fail honestly.
pub async fn preview(
    State(state): State<GatewayState>,
    Json(request): Json<PreviewRequest>,
) -> Response {
    let Some(runtime) = state.voice_library() else {
        return unavailable();
    };
    if request.text.trim().is_empty() || request.text.chars().count() > 300 {
        return crate::settings_service::refused("preview text must contain 1–300 characters");
    }
    let provider = match runtime.draft(&request.voice) {
        Ok(provider) => provider,
        Err(error) => return crate::settings_service::refused(&error.to_string()),
    };
    let Ok(permit) = runtime.preview.clone().try_acquire_owned() else {
        return error_response(
            StatusCode::TOO_MANY_REQUESTS,
            ErrorCode::RateLimited,
            "another voice preview is still running",
        );
    };
    // Collect the bounded preview while holding its permit: an HTTP stream must not release admission early.
    let deadline = provider
        .synthesis_timeout()
        .min(std::time::Duration::from_secs(180));
    let lease = match PreviewLease::new(runtime, request.request_id.clone(), permit) {
        Ok(lease) => lease,
        Err(message) => return crate::settings_service::refused(message),
    };
    if request.stream {
        return streaming::preview(provider, request.text, lease, deadline).await;
    }
    let result = tokio::time::timeout(deadline, async {
        tokio::select! {
            () = lease.notify.notified() => Err(SpeechError::Unreachable),
            result = async { jarvis_voice::collect_audio(provider.open(&request.text, None).await?).await } => result,
        }
    })
    .await;
    match result {
        Ok(Ok(audio)) => (
            StatusCode::OK,
            [
                (header::CONTENT_TYPE, audio.content_type()),
                (header::CACHE_CONTROL, "no-store"),
            ],
            audio.bytes().clone(),
        )
            .into_response(),
        Ok(Err(error)) => crate::speech_service::speech_error(&error),
        Err(_) => crate::speech_service::speech_error(&SpeechError::Unreachable),
    }
}

fn design_fingerprint(design: &jarvis_core::DesignedVoice) -> Result<[u8; 32], SpeechError> {
    let bytes = serde_json::to_vec(design).map_err(|_| SpeechError::BadAudio)?;
    Ok(Sha256::digest(bytes).into())
}

/// Only text is accepted: creating synthetic voices does not accept human recordings or paths.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DesignRequest {
    provider: String,
    description: String,
    text: String,
    seed: u32,
    request_id: Option<String>,
    #[serde(default)]
    quality: jarvis_core::VoiceQuality,
    #[serde(default)]
    stream: bool,
}

/// Generate an unsaved local voice identity. Saving still uses the revisioned library endpoint.
pub async fn design(
    State(state): State<GatewayState>,
    Json(request): Json<DesignRequest>,
) -> Response {
    let Some(runtime) = state.voice_library() else {
        return unavailable();
    };
    if let Err(error) =
        jarvis_core::DesignedVoice::validate_prompt(&request.description, &request.text)
    {
        return crate::settings_service::refused(error);
    }
    let Some((_, provider)) = runtime
        .providers
        .iter()
        .find(|(id, _)| id == &request.provider)
    else {
        return crate::settings_service::refused("the local voice designer is not configured");
    };
    if provider.describe().provider != "local-voxcpm2" {
        return crate::settings_service::refused(
            "choose the local description-capable voice designer",
        );
    }
    let Ok(permit) = runtime.preview.clone().try_acquire_owned() else {
        return error_response(
            StatusCode::TOO_MANY_REQUESTS,
            ErrorCode::RateLimited,
            "another voice preview or generation is running",
        );
    };
    let lease = match PreviewLease::new(runtime, request.request_id.clone(), permit) {
        Ok(lease) => lease,
        Err(message) => return crate::settings_service::refused(message),
    };
    streaming::design(provider.clone(), request, lease).await
}

struct PreviewLease {
    runtime: Arc<VoiceRuntime>,
    notify: Arc<Notify>,
    _permit: OwnedSemaphorePermit,
}
impl PreviewLease {
    fn new(
        runtime: &Arc<VoiceRuntime>,
        id: Option<String>,
        permit: OwnedSemaphorePermit,
    ) -> Result<Self, &'static str> {
        let id = id.unwrap_or_default();
        if id.len() > 64
            || !id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'))
        {
            return Err("invalid voice request identifier");
        }
        let notify = Arc::new(Notify::new());
        *runtime
            .active_preview
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) =
            Some((id.clone(), notify.clone()));
        let mut cancelled = runtime
            .cancelled
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(index) = cancelled.iter().position(|value| value == &id) {
            cancelled.remove(index);
            notify.notify_one();
        }
        Ok(Self {
            runtime: runtime.clone(),
            notify,
            _permit: permit,
        })
    }
}
impl Drop for PreviewLease {
    fn drop(&mut self) {
        *self
            .runtime
            .active_preview
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = None;
    }
}

/// Cancel just the originating preview/design request, using the same local credential.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CancelRequest {
    request_id: String,
}

/// Stop local work as well as browser playback; disconnecting HTTP alone is not cancellation.
pub async fn cancel(
    State(state): State<GatewayState>,
    Json(request): Json<CancelRequest>,
) -> Response {
    let Some(runtime) = state.voice_library() else {
        return unavailable();
    };
    if request.request_id.is_empty()
        || request.request_id.len() > 64
        || !request
            .request_id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'))
    {
        return crate::settings_service::refused("invalid voice request identifier");
    }
    {
        let mut cancelled = runtime
            .cancelled
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if cancelled.len() == 32 {
            cancelled.pop_front();
        }
        cancelled.push_back(request.request_id.clone());
    }
    if !request.request_id.is_empty()
        && let Some((id, notify)) = runtime
            .active_preview
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .as_ref()
        && id == &request.request_id
    {
        notify.notify_one();
    }
    StatusCode::NO_CONTENT.into_response()
}
