//! The spoken voice: `GET /api/v1/speech` says whether one is configured, `POST /api/v1/speech` turns text into audio.
//!
//! The provider's key lives only in the daemon (`ADR-0138`). The page asks for speech and plays the audio it gets back,
//! so the key is in no browser, no page script and no URL. Both routes sit behind the same bearer credential as the
//! rest of the API.

use axum::{
    Json,
    extract::State,
    http::{HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use futures_util::StreamExt;
use jarvis_core::ErrorCode;
use jarvis_voice::SpeechError;
use serde::Deserialize;
use serde_json::json;

use crate::gateway::{GatewayState, error_response};

/// The body of `POST /api/v1/speech`.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpeakRequest {
    /// Opt in to incremental PCM when supported; older callers retain complete WAV.
    #[serde(default)]
    stream: bool,
    text: String,
    /// The sentence spoken just before this one, so the voice carries on in the same tone. Optional.
    #[serde(default)]
    previous_text: Option<String>,
    /// Provider selected for earlier sentences of this answer.
    #[serde(default)]
    provider: Option<String>,
}

/// `GET /api/v1/speech`
pub async fn status(State(state): State<GatewayState>) -> Response {
    let body = state
        .speech()
        .filter(|speech| !speech.statuses().is_empty())
        .map_or_else(
            || json!({ "enabled": false }),
            |speech| {
                let description = speech.describe();
                json!({
                    "enabled": true,
                    "provider": description.provider,
                    "voice": description.voice,
                    "model": description.model,
                    "active": speech.active(),
                    "providers": speech.statuses(),
                    "max_text_chars": jarvis_voice::MAX_TEXT_CHARS,
                })
            },
        );
    (
        StatusCode::OK,
        [(header::CACHE_CONTROL, "no-store")],
        Json(body),
    )
        .into_response()
}

/// `POST /api/v1/speech`
pub async fn speak(
    State(state): State<GatewayState>,
    Json(request): Json<SpeakRequest>,
) -> Response {
    let Some(speech) = state
        .speech()
        .filter(|speech| !speech.statuses().is_empty())
    else {
        return error_response(
            StatusCode::NOT_FOUND,
            ErrorCode::UnavailableCapability,
            "no speech provider is configured; configure daemon.speech.providers or daemon.speech_api_key_ref",
        );
    };
    // The provider is asked to stream, and the first chunk is awaited here, so a refusal is still an honest status code and
    // a `200` always means audio is on its way. The rest is passed on as it arrives rather than collected, so the browser can
    // start playing while the sentence is still being made.
    let opened = match speech
        .open_preferred(
            &request.text,
            request.previous_text.as_deref(),
            request.provider.as_deref(),
        )
        .await
    {
        Ok(stream) => {
            async {
                let mut stream =
                    if !request.stream && stream.content_type() == jarvis_voice::PCM_CONTENT_TYPE {
                        stream.buffered().await?
                    } else {
                        stream
                    };
                match stream.next_chunk().await {
                    Ok(Some(first)) => Ok((stream, first)),
                    Ok(None) => Err(SpeechError::BadAudio),
                    Err(error) => Err(error),
                }
            }
            .await
        }
        Err(error) => Err(error),
    };
    match opened {
        Ok((stream, first)) => {
            let content_type = stream.content_type();
            let selection = stream
                .selection()
                .map(|(id, backup)| (id.to_owned(), backup));
            let rest = futures_util::stream::unfold(stream, |mut stream| async move {
                match stream.next_chunk().await {
                    Ok(Some(chunk)) => Some((Ok::<_, std::io::Error>(chunk), stream)),
                    Ok(None) => None,
                    // Ends the response abnormally, which the browser sees as a failed read and falls back from.
                    Err(error) => Some((Err(std::io::Error::other(error.to_string())), stream)),
                }
            });
            let body = axum::body::Body::from_stream(
                futures_util::stream::once(async move { Ok::<_, std::io::Error>(first) })
                    .chain(rest),
            );
            let mut response = (StatusCode::OK, body).into_response();
            let headers = response.headers_mut();
            headers.insert(header::CONTENT_TYPE, HeaderValue::from_static(content_type));
            headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
            if let Some((id, backup)) = selection {
                if let Ok(value) = HeaderValue::from_str(&id) {
                    headers.insert("x-jarvis-speech-provider", value);
                }
                headers.insert(
                    "x-jarvis-speech-backup",
                    HeaderValue::from_static(if backup { "true" } else { "false" }),
                );
            }
            response
        }
        Err(error) => speech_error(&error),
    }
}

pub(crate) fn speech_error(error: &SpeechError) -> Response {
    // The error text names the failure and never the key or the provider's body (`jarvis-voice`).
    if !matches!(error, SpeechError::Busy) {
        tracing::warn!(%error, "speech could not be produced");
    }
    let (status, code) = match &error {
        SpeechError::EmptyText | SpeechError::TooLong => {
            (StatusCode::UNPROCESSABLE_ENTITY, ErrorCode::Validation)
        }
        SpeechError::OverLimit | SpeechError::OutOfCredit => {
            (StatusCode::TOO_MANY_REQUESTS, ErrorCode::RateLimited)
        }
        SpeechError::KeyRejected | SpeechError::Settings(_) => {
            (StatusCode::BAD_GATEWAY, ErrorCode::PermanentUpstream)
        }
        SpeechError::Unreachable | SpeechError::Provider(_) | SpeechError::BadAudio => {
            (StatusCode::BAD_GATEWAY, ErrorCode::TransientUpstream)
        }
        SpeechError::Busy => (StatusCode::SERVICE_UNAVAILABLE, ErrorCode::RateLimited),
    };
    let mut response = error_response(status, code, &error.to_string());
    if matches!(error, SpeechError::Busy) {
        response
            .headers_mut()
            .insert(header::RETRY_AFTER, HeaderValue::from_static("1"));
    }
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn local_busy_is_retryable_without_claiming_exhausted_credit() {
        let response = speech_error(&SpeechError::Busy);
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(response.headers()[header::RETRY_AFTER], "1");
        let bytes = axum::body::to_bytes(response.into_body(), 4096)
            .await
            .unwrap_or_default();
        let text = String::from_utf8_lossy(&bytes);
        assert!(text.contains("busy"));
        assert!(!text.contains("credit"));
        assert_eq!(
            speech_error(&SpeechError::OverLimit).status(),
            StatusCode::TOO_MANY_REQUESTS
        );
    }
}
