//! Checking a saved key against its provider: `POST /api/v1/settings/keys/{which}/test`.
//!
//! The Settings screen offers a **Test** beside the voice and the web-search key, so a wrong key is found when it is saved and not
//! when the assistant first needs it. The check reads the key **file** the settings point at (so it works before the restart that
//! applies a new key), makes one small request to the provider, and answers `{ "ok": bool, "message": "..." }` with status `200`
//! whether the provider accepted the key or not; only a request that makes no sense is an error. **No answer contains the key or
//! the provider's own body**: the message is written here, from the provider's status. Model and fallback keys are tested on the
//! Brain tab, Google by signing in and Telegram by pairing, so they have no test here.

use std::time::Duration;

use axum::{
    Json,
    extract::{Path as UrlPath, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use jarvis_storage::settings::{self, SecretKind};
use serde_json::json;

use crate::gateway::GatewayState;
use crate::settings_service::{context, refused};

/// How long a check may take before it is called unreachable.
const CHECK_TIMEOUT: Duration = Duration::from_secs(20);

/// What a check found, in a sentence a person can act on.
#[derive(Debug, Eq, PartialEq)]
struct Outcome {
    ok: bool,
    message: String,
}

impl Outcome {
    fn ok(message: &str) -> Self {
        Self {
            ok: true,
            message: message.to_owned(),
        }
    }

    fn problem(message: &str) -> Self {
        Self {
            ok: false,
            message: message.to_owned(),
        }
    }
}

/// The route of this module, under `/api/v1`.
pub fn routes() -> axum::Router<GatewayState> {
    axum::Router::new().route("/settings/keys/{which}/test", axum::routing::post(test))
}

/// `POST /api/v1/settings/keys/{which}/test`
pub async fn test(State(state): State<GatewayState>, UrlPath(which): UrlPath<String>) -> Response {
    let context = match context(&state) {
        Ok(context) => context,
        Err(response) => return response,
    };
    let kind = match SecretKind::parse(&which) {
        Ok(kind) => kind,
        Err(message) => return refused(&message),
    };
    let paths = context.paths();
    let outcome = match kind {
        SecretKind::Voice => {
            let Some(key) = read_key(paths, "speech_api_key_ref") else {
                return refused("no voice key is saved yet");
            };
            let voice = setting_or(paths, "speech_voice_id", jarvis_voice::DEFAULT_VOICE_ID);
            let model = setting_or(paths, "speech_model", jarvis_voice::DEFAULT_MODEL);
            check_voice(jarvis_voice::ELEVENLABS_BASE_URL, &voice, &model, &key).await
        }
        SecretKind::Search => {
            let Some(key) = read_key(paths, "search_api_key_ref") else {
                return refused("no search key is saved yet");
            };
            check_search(jarvis_web::SEARCH_ENDPOINT, &key).await
        }
        _ => {
            return refused(
                "that key is checked elsewhere: the model keys on the Brain tab, Google by signing in, Telegram by pairing",
            );
        }
    };
    (
        StatusCode::OK,
        Json(json!({ "ok": outcome.ok, "message": outcome.message })),
    )
        .into_response()
}

/// The key in the file the named setting points at, when there is one.
fn read_key(paths: &jarvis_storage::AppPaths, field: &str) -> Option<String> {
    let file = settings::get(paths, field).ok()?;
    let text = std::fs::read_to_string(file).ok()?;
    let key = text.trim();
    (!key.is_empty()).then(|| key.to_owned())
}

fn setting_or(paths: &jarvis_storage::AppPaths, field: &str, default: &str) -> String {
    settings::get(paths, field)
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| default.to_owned())
}

/// Asks the speech provider for one short sentence and waits for the first piece of audio.
async fn check_voice(base: &str, voice: &str, model: &str, key: &str) -> Outcome {
    let speech = match jarvis_voice::ElevenLabsSpeech::new(base, voice, model, key) {
        Ok(speech) => speech,
        Err(error) => {
            return Outcome::problem(&format!("the voice settings are not usable: {error}"));
        }
    };
    let result = tokio::time::timeout(CHECK_TIMEOUT, async {
        let mut stream = speech.open("Systems online.", None).await?;
        stream.next_chunk().await
    })
    .await;
    match result {
        Ok(Ok(Some(_))) => Outcome::ok("ElevenLabs accepted the key and made audio."),
        Ok(Ok(None)) => Outcome::problem("ElevenLabs accepted the request but sent no audio."),
        Ok(Err(jarvis_voice::SpeechError::KeyRejected)) => Outcome::problem(
            "ElevenLabs rejected the key, or the key is not allowed to make speech. Check it in your ElevenLabs account.",
        ),
        Ok(Err(jarvis_voice::SpeechError::OverLimit | jarvis_voice::SpeechError::OutOfCredit)) => {
            Outcome::problem(
                "ElevenLabs says this account is out of credit or rate limited. The key itself may be fine.",
            )
        }
        Ok(Err(jarvis_voice::SpeechError::Provider(status))) => Outcome::problem(&format!(
            "ElevenLabs answered with status {status}. Check the voice and the model in this tab."
        )),
        Ok(Err(_)) | Err(_) => {
            Outcome::problem("ElevenLabs could not be reached. Check your internet connection.")
        }
    }
}

/// Asks the search service one question with the key.
async fn check_search(endpoint: &str, key: &str) -> Outcome {
    let Ok(client) = reqwest::Client::builder()
        .timeout(CHECK_TIMEOUT)
        .redirect(reqwest::redirect::Policy::none())
        .build()
    else {
        return Outcome::problem("the check could not be set up");
    };
    let response = client
        .post(endpoint)
        .bearer_auth(key)
        .json(&json!({ "query": "ollama", "max_results": 1 }))
        .send()
        .await;
    match response.map(|reply| reply.status().as_u16()) {
        Ok(200..=299) => Outcome::ok("The search service accepted the key and answered."),
        Ok(401 | 403) => Outcome::problem(
            "The search service rejected the key. Make a new one in your ollama.com account.",
        ),
        Ok(429) => Outcome::problem(
            "The search service is rate limiting this key. Try again later; the key itself may be fine.",
        ),
        Ok(other) => Outcome::problem(&format!("The search service answered with status {other}.")),
        Err(_) => Outcome::problem(
            "The search service could not be reached. Check your internet connection.",
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    /// A stand-in provider on the loopback address that answers every request with `status` and `body`, and returns its origin.
    async fn provider(status: u16, body: &'static [u8]) -> String {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .unwrap_or_else(|e| panic!("{e}"));
        let address = listener.local_addr().unwrap_or_else(|e| panic!("{e}"));
        tokio::spawn(async move {
            loop {
                let Ok((mut socket, _)) = listener.accept().await else {
                    return;
                };
                tokio::spawn(async move {
                    let mut buffer = vec![0_u8; 8192];
                    let _ = socket.read(&mut buffer).await;
                    let head = format!(
                        "HTTP/1.1 {status} X\r\ncontent-type: audio/mpeg\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
                        body.len()
                    );
                    let _ = socket.write_all(head.as_bytes()).await;
                    let _ = socket.write_all(body).await;
                    let _ = socket.shutdown().await;
                });
            }
        });
        format!("http://{address}")
    }

    const KEY: &str = "sk-secret-test-key-0123456789";

    #[tokio::test]
    async fn a_voice_key_the_provider_accepts_is_reported_working() {
        let base = provider(200, b"ID3audio-bytes").await;
        let outcome = check_voice(&base, "voice_1", "model_1", KEY).await;
        assert!(outcome.ok, "{outcome:?}");
    }

    /// Falsified by reading any answer as success: a rejected key would then be reported as working.
    #[tokio::test]
    async fn a_rejected_voice_key_is_reported_and_the_key_is_never_repeated() {
        for status in [401_u16, 403] {
            let base = provider(
                status,
                b"{\"detail\":\"bad key sk-secret-test-key-0123456789\"}",
            )
            .await;
            let outcome = check_voice(&base, "voice_1", "model_1", KEY).await;
            assert!(!outcome.ok);
            assert!(outcome.message.contains("rejected the key"), "{outcome:?}");
            assert!(!outcome.message.contains(KEY));
            assert!(
                !outcome.message.contains("bad key"),
                "the provider's own text is not passed on"
            );
        }
    }

    #[tokio::test]
    async fn a_voice_check_tells_credit_trouble_and_other_failures_apart_from_a_bad_key() {
        let base = provider(429, b"x").await;
        let out_of_credit = check_voice(&base, "voice_1", "model_1", KEY).await;
        assert!(
            !out_of_credit.ok && out_of_credit.message.contains("credit"),
            "{out_of_credit:?}"
        );
        let base = provider(500, b"x").await;
        let broken = check_voice(&base, "voice_1", "model_1", KEY).await;
        assert!(!broken.ok && broken.message.contains("500"), "{broken:?}");
        let nobody = check_voice("http://127.0.0.1:9", "voice_1", "model_1", KEY).await;
        assert!(
            !nobody.ok && nobody.message.contains("reached"),
            "{nobody:?}"
        );
    }

    #[tokio::test]
    async fn unusable_voice_settings_are_reported_without_a_request() {
        let outcome = check_voice("http://127.0.0.1:9", "bad voice!", "model_1", KEY).await;
        assert!(
            !outcome.ok && outcome.message.contains("not usable"),
            "{outcome:?}"
        );
    }

    #[tokio::test]
    async fn the_search_check_reads_each_kind_of_answer() {
        let accepted = check_search(&provider(200, b"{}").await, KEY).await;
        assert!(accepted.ok, "{accepted:?}");
        let rejected = check_search(&provider(401, b"{}").await, KEY).await;
        assert!(
            !rejected.ok && rejected.message.contains("rejected"),
            "{rejected:?}"
        );
        let limited = check_search(&provider(429, b"{}").await, KEY).await;
        assert!(
            !limited.ok && limited.message.contains("rate limiting"),
            "{limited:?}"
        );
        let odd = check_search(&provider(503, b"{}").await, KEY).await;
        assert!(!odd.ok && odd.message.contains("503"), "{odd:?}");
        let down = check_search("http://127.0.0.1:9", KEY).await;
        assert!(!down.ok && down.message.contains("reached"), "{down:?}");
    }

    /// A redirect is not followed, so a key cannot be sent on to an address the provider chose.
    #[tokio::test]
    async fn a_search_redirect_is_not_followed() {
        let base = provider(307, b"").await;
        let outcome = check_search(&base, KEY).await;
        assert!(
            !outcome.ok && outcome.message.contains("307"),
            "{outcome:?}"
        );
    }
}
