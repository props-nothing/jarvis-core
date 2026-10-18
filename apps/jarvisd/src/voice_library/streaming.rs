use super::{
    DesignRequest, GatewayState, PreviewLease, Provider, Response, SpeechError, VoiceRuntime,
    design_fingerprint, unavailable,
};
use axum::body::{Body, Bytes};
use axum::{Json, extract::State, http::header, response::IntoResponse};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use futures_util::StreamExt;
use jarvis_voice::{DesignEvent, SpeechStream};
use serde::Deserialize;
use serde_json::json;
use std::time::Duration;
use tokio::time::Instant;

async fn bounded<T>(
    lease: &PreviewLease,
    deadline: Instant,
    future: impl std::future::Future<Output = Result<T, SpeechError>>,
) -> Result<T, SpeechError> {
    tokio::select! {
        () = lease.notify.notified() => Err(SpeechError::Unreachable),
        value = tokio::time::timeout_at(deadline, future) => value.map_err(|_| SpeechError::Unreachable)?,
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct PrepareRequest {
    provider: String,
    request_id: Option<String>,
}

pub(super) async fn prepare(
    State(state): State<GatewayState>,
    Json(request): Json<PrepareRequest>,
) -> Response {
    let Some(runtime) = state.voice_library() else {
        return unavailable();
    };
    let Some((_, provider)) = runtime
        .providers
        .iter()
        .find(|(id, p)| id == &request.provider && p.describe().provider == "local-voxcpm2")
    else {
        return crate::settings_service::refused("choose the local voice designer to prepare");
    };
    let Ok(permit) = runtime.preview.clone().try_acquire_owned() else {
        return crate::speech_service::speech_error(&SpeechError::Busy);
    };
    let lease = match PreviewLease::new(runtime, request.request_id, permit) {
        Ok(v) => v,
        Err(e) => return crate::settings_service::refused(e),
    };
    match bounded(
        &lease,
        Instant::now() + Duration::from_secs(180),
        provider.prepare(),
    )
    .await
    {
        Ok(()) => (
            [(header::CACHE_CONTROL, "no-store")],
            Json(json!({"execution":provider.execution()})),
        )
            .into_response(),
        Err(e) => crate::speech_service::speech_error(&e),
    }
}

struct PreviewBody {
    stream: SpeechStream,
    first: Option<Bytes>,
    lease: PreviewLease,
    deadline: Instant,
}

pub(super) async fn preview(
    provider: Provider,
    text: String,
    lease: PreviewLease,
    duration: Duration,
) -> Response {
    let deadline = Instant::now() + duration;
    let opened = bounded(&lease, deadline, async {
        let mut stream = provider.open(&text, None).await?;
        let first = stream.next_chunk().await?.ok_or(SpeechError::BadAudio)?;
        Ok((stream, first))
    })
    .await;
    let (stream, first) = match opened {
        Ok(value) => value,
        Err(e) => return crate::speech_service::speech_error(&e),
    };
    let mime = stream.content_type();
    let source = futures_util::stream::unfold(
        Some(PreviewBody {
            stream,
            first: Some(first),
            lease,
            deadline,
        }),
        |state| async {
            let mut state = state?;
            let result = if let Some(first) = state.first.take() {
                Ok(Some(first))
            } else {
                bounded(&state.lease, state.deadline, state.stream.next_chunk()).await
            };
            match result {
                Ok(Some(bytes)) => Some((Ok::<_, std::io::Error>(bytes), Some(state))),
                Ok(None) => None,
                Err(e) => Some((Err(std::io::Error::other(e.to_string())), None)),
            }
        },
    );
    (
        [
            (header::CONTENT_TYPE, mime),
            (header::CACHE_CONTROL, "no-store"),
        ],
        Body::from_stream(source),
    )
        .into_response()
}

fn admit(runtime: &VoiceRuntime, design: &jarvis_core::DesignedVoice) -> Result<(), SpeechError> {
    let fingerprint = design_fingerprint(design)?;
    let mut generated = runtime
        .generated
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if generated.len() == 8 {
        generated.pop_front();
    }
    generated.push_back(fingerprint);
    Ok(())
}

pub(super) async fn design(
    provider: Provider,
    request: DesignRequest,
    lease: PreviewLease,
) -> Response {
    let deadline = Instant::now() + Duration::from_secs(180);
    let opened = bounded(
        &lease,
        deadline,
        provider.design_stream(
            &request.description,
            &request.text,
            request.seed,
            request.quality,
        ),
    )
    .await;
    let mut stream = match opened {
        Ok(stream) => stream,
        Err(e) => return crate::speech_service::speech_error(&e),
    };
    if !request.stream {
        let result = bounded(&lease, deadline, async {
            while let Some(event) = stream.next().await {
                if let DesignEvent::Complete(design) = event? {
                    admit(&lease.runtime, &design)?;
                    return Ok(design);
                }
            }
            Err(SpeechError::BadAudio)
        })
        .await;
        return match result {
            Ok(design) => (
                [(header::CACHE_CONTROL, "no-store")],
                Json(json!({"design":design,"execution":provider.execution()})),
            )
                .into_response(),
            Err(e) => crate::speech_service::speech_error(&e),
        };
    }
    let source =
        futures_util::stream::unfold(Some((stream, lease, provider)), move |state| async move {
            let (mut stream, lease, provider) = state?;
            let result = bounded(&lease, deadline, async {
                stream.next().await.ok_or(SpeechError::BadAudio)?
            })
            .await;
            let event = match result {
                Ok(DesignEvent::Audio(pcm)) => json!({"event":"audio","pcm":STANDARD.encode(pcm)}),
                Ok(DesignEvent::Complete(design)) => {
                    if let Err(e) = admit(&lease.runtime, &design) {
                        return Some((Err(std::io::Error::other(e.to_string())), None));
                    }
                    let bytes = Bytes::from(
                        json!({"event":"complete","design":design,"execution":provider.execution()})
                            .to_string() + "\n",
                    );
                    return Some((Ok(bytes), None));
                }
                Err(e) => return Some((Err(std::io::Error::other(e.to_string())), None)),
            };
            Some((
                Ok(Bytes::from(event.to_string() + "\n")),
                Some((stream, lease, provider)),
            ))
        });
    (
        [
            (header::CONTENT_TYPE, "application/x-ndjson"),
            (header::CACHE_CONTROL, "no-store"),
        ],
        Body::from_stream(source),
    )
        .into_response()
}
