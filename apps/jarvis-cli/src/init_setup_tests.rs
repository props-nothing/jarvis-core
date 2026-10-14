use std::sync::{Arc, Mutex};

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

use super::*;

fn must<T, E: std::fmt::Display>(result: Result<T, E>) -> T {
    result.unwrap_or_else(|error| panic!("{error}"))
}

struct Profile {
    root: PathBuf,
    paths: AppPaths,
    key: PathBuf,
}

impl Profile {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "jarvis-setup-contract-{}",
            jarvis_core::CorrelationId::new()
        ));
        must(std::fs::create_dir_all(&root));
        let key = root.join("external-speech.key");
        must(std::fs::write(&key, "synthetic-speech-key"));
        let paths = must(AppPaths::from_root(&root.join("profile")));
        Self { root, paths, key }
    }

    fn request(&self, base: &str) -> InitRequest {
        InitRequest {
            base_url: Some(format!("{base}/v1")),
            model: Some("test-model".to_owned()),
            voice_key_file: Some(self.key.clone()),
            ..InitRequest::default()
        }
    }

    fn assert_unsaved(&self) {
        assert!(!ConfigStore::from_paths(&self.paths).path().exists());
        assert!(!self.paths.config().join(KEY_FILE_NAME).exists());
        assert!(!self.paths.config().join(SPEECH_KEY_FILE_NAME).exists());
        assert_eq!(
            must(std::fs::read_to_string(&self.key)),
            "synthetic-speech-key"
        );
    }
}

impl Drop for Profile {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

struct Provider {
    base: String,
    requests: Arc<Mutex<Vec<String>>>,
    task: tokio::task::JoinHandle<()>,
}

impl Drop for Provider {
    fn drop(&mut self) {
        self.task.abort();
    }
}

async fn provider(
    model_status: u16,
    speech_status: u16,
    empty_audio: bool,
    list_status: u16,
) -> Provider {
    let listener = must(TcpListener::bind("127.0.0.1:0").await);
    let base = format!("http://{}", must(listener.local_addr()));
    let requests = Arc::new(Mutex::new(Vec::new()));
    let capture = Arc::clone(&requests);
    let task = tokio::spawn(async move {
        while let Ok((mut socket, _)) = listener.accept().await {
            let mut request = Vec::new();
            loop {
                let mut buffer = [0_u8; 2048];
                let count = must(socket.read(&mut buffer).await);
                if count == 0 {
                    break;
                }
                request.extend_from_slice(&buffer[..count]);
                let text = String::from_utf8_lossy(&request);
                if let Some(end) = text.find("\r\n\r\n") {
                    let length = text[..end]
                        .lines()
                        .find_map(|line| {
                            line.to_ascii_lowercase()
                                .strip_prefix("content-length: ")
                                .and_then(|number| number.parse::<usize>().ok())
                        })
                        .unwrap_or(0);
                    if request.len() >= end + 4 + length {
                        break;
                    }
                }
            }
            let text = String::from_utf8_lossy(&request).into_owned();
            let (status, content_type, body) = if text.starts_with("GET /v1/models ") {
                (
                    list_status,
                    "application/json",
                    r#"{"object":"list","data":[{"id":"test-model"}]}"#,
                )
            } else if text.starts_with("POST /v1/chat/completions ") {
                (
                    model_status,
                    "application/json",
                    r#"{"id":"test","object":"chat.completion","model":"test-model","choices":[{"index":0,"message":{"role":"assistant","content":"OK"},"finish_reason":"stop"}],"usage":{"prompt_tokens":1,"completion_tokens":1,"total_tokens":2}}"#,
                )
            } else if text.starts_with("POST /v1/text-to-speech/") {
                (
                    speech_status,
                    "audio/mpeg",
                    if empty_audio { "" } else { "synthetic-audio" },
                )
            } else {
                panic!(
                    "unexpected request: {}",
                    text.lines().next().unwrap_or_default()
                );
            };
            must(capture.lock()).push(text);
            let head = format!(
                "HTTP/1.1 {status} Test\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            must(socket.write_all(head.as_bytes()).await);
            must(socket.write_all(body.as_bytes()).await);
            must(socket.shutdown().await);
        }
    });
    Provider {
        base,
        requests,
        task,
    }
}

#[tokio::test(flavor = "current_thread")]
async fn setup_tests_model_and_exact_default_voice_before_persisting() {
    let upstream = provider(200, 200, false, 200).await;
    let profile = Profile::new();
    must(
        configure_at(
            &profile.paths,
            &profile.request(&upstream.base),
            &upstream.base,
        )
        .await,
    );
    let loaded = must(ConfigStore::from_paths(&profile.paths).load());
    let daemon = loaded.config().daemon();
    assert_eq!(daemon.executor_model_name(), Some("test-model"));
    assert_eq!(daemon.speech_voice_id(), Some("liN4AsrhF3CylqMDWK6p"));
    assert!(daemon.tool_workspace_roots().is_empty());
    let requests = must(upstream.requests.lock());
    assert_eq!(requests.len(), 3);
    assert!(requests[2].starts_with("POST /v1/text-to-speech/liN4AsrhF3CylqMDWK6p/stream"));
    assert!(requests[2].contains("xi-api-key: synthetic-speech-key"));
    assert!(requests[2].contains("Systems online."));
    assert!(
        !must(std::fs::read_to_string(
            ConfigStore::from_paths(&profile.paths).path()
        ))
        .contains("synthetic-speech-key")
    );
}

#[tokio::test(flavor = "current_thread")]
async fn refused_model_or_voice_and_empty_audio_leave_setup_unsaved() {
    for (model_status, speech_status, empty_audio) in [
        (401, 200, false),
        (200, 401, false),
        (200, 429, false),
        (200, 200, true),
    ] {
        let upstream = provider(model_status, speech_status, empty_audio, 200).await;
        let profile = Profile::new();
        let error = configure_at(
            &profile.paths,
            &profile.request(&upstream.base),
            &upstream.base,
        )
        .await
        .err()
        .unwrap_or_else(|| panic!("setup must refuse"));
        assert!(!error.contains("synthetic-speech-key"));
        profile.assert_unsaved();
    }
}

#[tokio::test(flavor = "current_thread")]
async fn explicit_voice_model_and_folder_override_defaults_without_granting_siblings() {
    let upstream = provider(200, 200, false, 200).await;
    let profile = Profile::new();
    let granted = profile.root.join("Development");
    let sibling = profile.root.join("Projects");
    must(std::fs::create_dir_all(&granted));
    must(std::fs::create_dir_all(&sibling));
    let mut request = profile.request(&upstream.base);
    request.voice_id = Some("chosen-voice".to_owned());
    request.voice_model = Some("chosen-model".to_owned());
    request.workspaces = vec![granted.clone(), granted.clone()];
    must(configure_at(&profile.paths, &request, &upstream.base).await);
    let loaded = must(ConfigStore::from_paths(&profile.paths).load());
    assert_eq!(
        loaded.config().daemon().speech_voice_id(),
        Some("chosen-voice")
    );
    assert_eq!(
        loaded.config().daemon().speech_model(),
        Some("chosen-model")
    );
    assert_eq!(
        loaded.config().daemon().tool_workspace_roots(),
        &[strip_verbatim(must(std::fs::canonicalize(granted)))]
    );
    let requests = must(upstream.requests.lock());
    assert!(requests[2].starts_with("POST /v1/text-to-speech/chosen-voice/stream"));
    assert!(requests[2].contains("chosen-model"));
}

#[tokio::test(flavor = "current_thread")]
async fn failed_model_listing_does_not_prevent_testing_an_explicit_model() {
    let upstream = provider(200, 200, false, 500).await;
    let profile = Profile::new();
    must(
        configure_at(
            &profile.paths,
            &profile.request(&upstream.base),
            &upstream.base,
        )
        .await,
    );
    assert!(ConfigStore::from_paths(&profile.paths).path().exists());
}

#[tokio::test(flavor = "current_thread")]
async fn empty_voice_key_and_bad_folder_cannot_save_or_replace_configuration() {
    let upstream = provider(200, 200, false, 200).await;
    let profile = Profile::new();
    let store = ConfigStore::from_paths(&profile.paths);
    must(store.save(&Config::default()));
    let before = must(std::fs::read(store.path()));
    must(std::fs::write(&profile.key, ""));
    assert!(
        configure_at(
            &profile.paths,
            &profile.request(&upstream.base),
            &upstream.base
        )
        .await
        .is_err()
    );
    assert_eq!(must(std::fs::read(store.path())), before);
    assert!(!profile.paths.config().join(KEY_FILE_NAME).exists());
    must(std::fs::write(&profile.key, "synthetic-speech-key"));
    let mut request = profile.request(&upstream.base);
    request.workspaces = vec![profile.root.join("missing-folder")];
    assert!(
        configure_at(&profile.paths, &request, &upstream.base)
            .await
            .is_err()
    );
    assert_eq!(must(std::fs::read(store.path())), before);
    assert!(!profile.paths.config().join(KEY_FILE_NAME).exists());
}

#[test]
fn missing_key_source_and_existing_folder_suggestions_are_not_defaults_or_grants() {
    let profile = Profile::new();
    let env = format!(
        "JARVIS_SETUP_TEST_UNSET_{}",
        jarvis_core::CorrelationId::new()
    );
    assert!(key_input(None, profile.root.join("new.key"), &env, "unused", false).is_err());
    assert!(!profile.root.join("new.key").exists());
    must(std::fs::create_dir_all(profile.root.join("Development")));
    must(std::fs::create_dir_all(profile.root.join("code")));
    let suggestions = suggested_folders(&profile.root);
    assert_eq!(
        suggestions,
        vec![profile.root.join("Development"), profile.root.join("code")]
    );
    assert!(!profile.root.join("Projects").exists());
    assert!(!ConfigStore::from_paths(&profile.paths).path().exists());
}
