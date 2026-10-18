use super::*;
use axum::{
    body::{Body, to_bytes},
    http::Request,
};
use tower::ServiceExt;

struct Scratch(std::path::PathBuf);
impl Scratch {
    fn new() -> (Self, AppPaths) {
        let root = std::env::temp_dir().join(format!(
            "jarvis-voice-library-{}",
            jarvis_core::scratch_tag()
        ));
        let paths = AppPaths::from_root(&root).unwrap_or_else(|e| panic!("{e}"));
        paths.prepare().unwrap_or_else(|e| panic!("{e}"));
        (Self(root), paths)
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        jarvis_core::remove_scratch_dir(&self.0);
    }
}

#[derive(Debug)]
struct Fixture {
    voice: String,
    fail: bool,
}
impl SpeechProvider for Fixture {
    fn describe(&self) -> SpeechDescription {
        SpeechDescription {
            id: "fixture".into(),
            provider: "openai-compatible".into(),
            voice: self.voice.clone(),
            model: "tts-1".into(),
        }
    }
    fn configured(&self, voice: &SavedVoice) -> Result<Provider, SpeechError> {
        jarvis_voice::validate_voice("openai-compatible", voice)?;
        Ok(Arc::new(Self {
            voice: voice.voice.clone(),
            fail: self.fail,
        }))
    }
    fn open<'a>(&'a self, _: &'a str, _: Option<&'a str>) -> SpeechFuture<'a> {
        let voice = self.voice.clone();
        Box::pin(async move {
            if self.fail {
                return Err(SpeechError::KeyRejected);
            }
            Ok(jarvis_voice::SpeechStream::new(
                futures_util::stream::once(async move { Ok(voice.into()) }),
                "audio/mpeg",
            ))
        })
    }
}

fn providers() -> Vec<(String, Provider)> {
    vec![
        (
            "primary".into(),
            Arc::new(Fixture {
                voice: "original".into(),
                fail: false,
            }),
        ),
        (
            "backup".into(),
            Arc::new(Fixture {
                voice: "backup".into(),
                fail: true,
            }),
        ),
    ]
}
fn library(runtime: &VoiceRuntime) -> VoiceLibrary {
    serde_json::from_value(runtime.response()["library"].clone()).unwrap_or_else(|e| panic!("{e}"))
}

#[tokio::test]
async fn streaming_preview_holds_admission_until_body_drop_and_clears_lease() {
    let (_scratch, paths) = Scratch::new();
    let runtime =
        Arc::new(VoiceRuntime::new(&paths, providers()).unwrap_or_else(|e| panic!("{e}")));
    let permit = runtime
        .preview
        .clone()
        .try_acquire_owned()
        .unwrap_or_else(|e| panic!("{e}"));
    let lease = PreviewLease::new(&runtime, Some("stream-fixture".into()), permit)
        .unwrap_or_else(|e| panic!("{e}"));
    let response = streaming::preview(
        runtime.providers[0].1.clone(),
        "Hello".into(),
        lease,
        std::time::Duration::from_secs(1),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert!(
        runtime.preview.try_acquire().is_err(),
        "headers must not release admission"
    );
    assert!(
        runtime
            .active_preview
            .lock()
            .unwrap_or_else(|e| panic!("{e}"))
            .is_some()
    );
    drop(response);
    assert!(runtime.preview.try_acquire().is_ok());
    assert!(
        runtime
            .active_preview
            .lock()
            .unwrap_or_else(|e| panic!("{e}"))
            .is_none()
    );
}

#[test]
fn editor_reports_missing_local_setup_without_exposing_paths_or_breaking_other_voices() {
    let (_scratch, paths) = Scratch::new();
    let missing = paths.data().join("missing-inference-installation");
    let designer = jarvis_voice::DescriptionSpeech::new(&missing).unwrap_or_else(|e| panic!("{e}"));
    let mut configured = providers();
    configured.push(("designer".into(), Arc::new(designer)));
    let runtime = VoiceRuntime::new(&paths, configured).unwrap_or_else(|e| panic!("{e}"));
    let reply = runtime.response();
    assert_eq!(reply["providers"][2]["description_design"], true);
    assert!(
        reply["providers"][2]["design_unavailable_reason"]
            .as_str()
            .is_some_and(|message| message.contains("Python runtime"))
    );
    assert!(reply["providers"][0]["design_unavailable_reason"].is_null());
    assert!(!reply.to_string().contains("missing-inference-installation"));
    assert_eq!(library(&runtime).voices.len(), 2);
    assert!(runtime.route().is_some());
}

#[tokio::test]
async fn saved_edits_apply_to_new_speech_and_survive_restart_without_changing_a_snapshot() {
    let (_scratch, paths) = Scratch::new();
    let runtime = VoiceRuntime::new(&paths, providers()).unwrap_or_else(|e| panic!("{e}"));
    let old = runtime.route().unwrap_or_else(|| panic!("seeded route"));
    let mut edited = library(&runtime);
    edited.voices[0].voice = "edited".into();
    edited.voices[0].name = "Reading".into();
    edited.voices[0].settings.speed = Some(1.2);
    runtime
        .save(edited.clone())
        .unwrap_or_else(|e| panic!("{e:?}"));
    assert_eq!(runtime.describe().voice, "edited");
    assert_eq!(old.describe().voice, "original");
    assert_eq!(
        runtime.save(edited).err().map(|e| e.0),
        Some(StatusCode::CONFLICT)
    );
    let reopened = VoiceRuntime::new(&paths, providers()).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(library(&reopened), library(&runtime));
    assert_eq!(reopened.describe().voice, "edited");
    let audio = jarvis_voice::collect_audio(
        reopened
            .open("Hello", None)
            .await
            .unwrap_or_else(|e| panic!("{e}")),
    )
    .await
    .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(audio.bytes().as_ref(), b"edited");
}

#[test]
fn invalid_library_missing_provider_and_external_edit_fail_closed() {
    let (_scratch, paths) = Scratch::new();
    let runtime = VoiceRuntime::new(&paths, providers()).unwrap_or_else(|e| panic!("{e}"));
    let initial = library(&runtime);
    let mut bad = initial.clone();
    bad.voices[0].settings.style = Some(0.2);
    assert!(runtime.save(bad).is_err());
    let mut bad = initial.clone();
    bad.voices[0].provider = "unconfigured".into();
    assert!(runtime.save(bad).is_err());
    let mut bad = initial.clone();
    bad.voices.remove(0);
    assert!(runtime.save(bad).is_err());
    assert_eq!(library(&runtime), initial);
    runtime.save(initial).unwrap_or_else(|e| panic!("{e:?}"));
    let mut external = library(&runtime);
    external.voices[0].name = "External edit".into();
    runtime
        .store
        .save(&external)
        .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(
        runtime.save(library(&runtime)).err().map(|e| e.0),
        Some(StatusCode::CONFLICT)
    );
    assert!(VoiceRuntime::new(&paths, vec![]).is_err());
    std::fs::write(paths.config().join("voice-library.json"), b"broken")
        .unwrap_or_else(|e| panic!("{e}"));
    assert!(VoiceRuntime::new(&paths, providers()).is_err());
}

#[tokio::test]
async fn routes_require_auth_preview_draft_without_save_or_fallback_and_reject_unsupported_fields()
{
    let (_scratch, paths) = Scratch::new();
    let database = Arc::new(
        jarvis_storage::SqliteDatabase::test_fixture(&paths.data().join("test.sqlite3"))
            .await
            .unwrap_or_else(|e| panic!("{e}")),
    );
    let credential = jarvis_core::ClientCredential::generate().unwrap_or_else(|e| panic!("{e}"));
    let token = credential.expose().to_owned();
    let runtime =
        Arc::new(VoiceRuntime::new(&paths, providers()).unwrap_or_else(|e| panic!("{e}")));
    let app = crate::gateway::router(
        GatewayState::new(database.clone(), credential)
            .with_speech(runtime.clone())
            .with_voice_library(runtime.clone()),
    );
    for (method, path) in [
        ("GET", "/api/v1/settings/voices"),
        ("GET", "/api/v1/settings/voices/runtime"),
        ("GET", "/api/v1/settings/voices/blend/export"),
        ("PUT", "/api/v1/settings/voices"),
        ("POST", "/api/v1/settings/voices/preview"),
        ("POST", "/api/v1/settings/voices/design"),
        ("POST", "/api/v1/settings/voices/cancel"),
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(method)
                    .uri(path)
                    .body(Body::empty())
                    .unwrap_or_else(|e| panic!("{e}")),
            )
            .await
            .unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }
    let before = library(&runtime);
    assert_runtime_metadata(app.clone(), &token).await;
    let mut draft = before.voices[0].clone();
    draft.voice = "draft".into();
    let request = |body: Value| {
        Request::builder()
            .method("POST")
            .uri("/api/v1/settings/voices/preview")
            .header("authorization", format!("Bearer {token}"))
            .header("content-type", "application/json")
            .body(Body::from(body.to_string()))
            .unwrap_or_else(|e| panic!("{e}"))
    };
    let response = app
        .clone()
        .oneshot(request(json!({"voice":draft,"text":"Preview"})))
        .await
        .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()["cache-control"], "no-store");
    assert_eq!(
        to_bytes(response.into_body(), 1024)
            .await
            .unwrap_or_default()
            .as_ref(),
        b"draft"
    );
    assert_eq!(library(&runtime), before);
    let response = app
        .clone()
        .oneshot(request(json!({"voice":before.voices[1],"text":"Preview"})))
        .await
        .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(response.status(), StatusCode::BAD_GATEWAY); // no fallback to the healthy primary
    let mut body = json!({"voice":draft,"text":"Preview"});
    body["voice"]["settings"]["pitch"] = json!(1);
    assert_eq!(
        app.clone()
            .oneshot(request(body))
            .await
            .unwrap_or_else(|e| panic!("{e}"))
            .status(),
        StatusCode::UNPROCESSABLE_ENTITY
    );
    assert_eq!(
        app.oneshot(request(json!({"voice":draft,"text":"x".repeat(301)})))
            .await
            .unwrap_or_else(|e| panic!("{e}"))
            .status(),
        StatusCode::UNPROCESSABLE_ENTITY
    );
    database.close().await;
}

async fn assert_runtime_metadata(app: axum::Router, token: &str) {
    let status = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/settings/voices/runtime")
                .header("authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap_or_else(|e| panic!("{e}")),
        )
        .await
        .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(status.status(), StatusCode::OK);
    assert_eq!(status.headers()["cache-control"], "no-store");
    let bytes = to_bytes(status.into_body(), 4096).await.unwrap_or_default();
    let metadata: Value = serde_json::from_slice(&bytes).unwrap_or_else(|e| panic!("{e}"));
    assert!(metadata["providers"].is_array());
    assert!(metadata.get("library").is_none());
}

#[cfg(feature = "local-tts")]
#[tokio::test]
async fn local_recipe_persists_exports_without_profile_secrets_and_deletes() {
    let (_scratch, paths) = Scratch::new();
    let provider: Provider = Arc::new(
        jarvis_voice::EmbeddedSpeech::new(paths.cache(), "af_heart", false)
            .unwrap_or_else(|e| panic!("{e}")),
    );
    let providers = || vec![("local".into(), provider.clone())];
    let runtime =
        Arc::new(VoiceRuntime::new(&paths, providers()).unwrap_or_else(|e| panic!("{e}")));
    let mut document = library(&runtime);
    let mut design = document.voices[0].clone();
    design.id = "evening".into();
    design.name = "Evening".into();
    design.voice = "blend".into();
    design.blend = Some(jarvis_voice::default_voice_blend());
    document.voices.push(design.clone());
    document.channels.console = Some("evening".into());
    runtime.save(document).unwrap_or_else(|e| panic!("{e:?}"));
    let reopened = VoiceRuntime::new(&paths, providers()).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(library(&reopened).voices[1], design);
    assert_eq!(reopened.describe().voice, "blend");
    let database = Arc::new(
        jarvis_storage::SqliteDatabase::test_fixture(&paths.data().join("test.sqlite3"))
            .await
            .unwrap_or_else(|e| panic!("{e}")),
    );
    let credential = jarvis_core::ClientCredential::generate().unwrap_or_else(|e| panic!("{e}"));
    let token = credential.expose().to_owned();
    let app = crate::gateway::router(
        GatewayState::new(database.clone(), credential).with_voice_library(runtime.clone()),
    );
    let request = || {
        Request::builder()
            .uri("/api/v1/settings/voices/evening/export")
            .header("authorization", format!("Bearer {token}"))
            .body(Body::empty())
            .unwrap_or_else(|e| panic!("{e}"))
    };
    let response = app
        .clone()
        .oneshot(request())
        .await
        .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()["cache-control"], "no-store");
    let bytes = to_bytes(response.into_body(), 4096)
        .await
        .unwrap_or_default();
    let export: Value = serde_json::from_slice(&bytes).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(export["format"], "jarvis-voice-design");
    assert_eq!(export["blend"], json!(design.blend));
    assert!(export.get("provider").is_none());
    assert!(export.get("id").is_none());
    let mut deletion = library(&runtime);
    deletion.voices.pop();
    assert!(runtime.save(deletion.clone()).is_err());
    deletion.channels.console = None;
    runtime.save(deletion).unwrap_or_else(|e| panic!("{e:?}"));
    assert_eq!(
        app.oneshot(request())
            .await
            .unwrap_or_else(|e| panic!("{e}"))
            .status(),
        StatusCode::NOT_FOUND
    );
    let reopened = VoiceRuntime::new(&paths, providers()).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(library(&reopened).voices.len(), 1);
    let file = std::fs::read_to_string(paths.config().join("voice-library.json"))
        .unwrap_or_else(|e| panic!("{e}"));
    assert!(!file.contains("evening"));
    database.close().await;
}

#[tokio::test]
async fn only_generated_or_saved_references_can_be_previewed_or_saved() {
    use base64::{Engine as _, engine::general_purpose::STANDARD};
    let (_scratch, paths) = Scratch::new();
    let provider: Provider = Arc::new(
        jarvis_voice::DescriptionSpeech::new(paths.cache()).unwrap_or_else(|e| panic!("{e}")),
    );
    let providers = || vec![("designer".into(), provider.clone())];
    let runtime = VoiceRuntime::new(&paths, providers()).unwrap_or_else(|e| panic!("{e}"));
    assert!(library(&runtime).voices.is_empty());
    let mut wav = Vec::new();
    wav.extend(b"RIFF");
    wav.extend(38_u32.to_le_bytes());
    wav.extend(b"WAVEfmt ");
    wav.extend(16_u32.to_le_bytes());
    wav.extend([1, 0, 1, 0]);
    wav.extend(48000_u32.to_le_bytes());
    wav.extend(96000_u32.to_le_bytes());
    wav.extend([2, 0, 16, 0]);
    wav.extend(b"data");
    wav.extend(2_u32.to_le_bytes());
    wav.extend([1, 0]);
    let design = jarvis_core::DesignedVoice {
        quality: jarvis_core::VoiceQuality::default(),
        version: 1,
        model_revision: jarvis_voice::DESIGN_MODEL_REVISION.into(),
        description: "Warm narrator".into(),
        sample_text: "Hello".into(),
        seed: 1,
        reference_audio: STANDARD.encode(wav),
    };
    let voice = SavedVoice {
        id: "designed".into(),
        name: "Narrator".into(),
        provider: "designer".into(),
        model: "voxcpm2".into(),
        voice: "designed".into(),
        settings: VoiceSettings::default(),
        blend: None,
        design: Some(design.clone()),
    };
    let mut edited = library(&runtime);
    edited.voices.push(voice.clone());
    edited.default_voice = Some(voice.id.clone());
    assert!(
        runtime.save(edited.clone()).is_err(),
        "unminted references must fail closed"
    );
    assert!(
        runtime.draft(&voice).is_err(),
        "preview must not bypass reference admission"
    );
    runtime
        .generated
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .push_back(design_fingerprint(&design).unwrap_or_else(|e| panic!("{e}")));
    assert!(runtime.draft(&voice).is_ok());
    runtime.save(edited).unwrap_or_else(|e| panic!("{e:?}"));
    let reopened = VoiceRuntime::new(&paths, providers()).unwrap_or_else(|e| panic!("{e}"));
    assert!(reopened.draft(&voice).is_ok());
    let mut tampered = voice;
    tampered
        .design
        .as_mut()
        .unwrap_or_else(|| panic!("design"))
        .seed += 1;
    assert!(reopened.draft(&tampered).is_err());
    let mut edited = library(&reopened);
    edited.voices[0] = tampered;
    assert!(reopened.save(edited).is_err());
}

#[tokio::test]
async fn cancellation_matches_the_request_and_retains_early_cancel() {
    let (_scratch, paths) = Scratch::new();
    let runtime =
        Arc::new(VoiceRuntime::new(&paths, providers()).unwrap_or_else(|e| panic!("{e}")));
    runtime
        .cancelled
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .push_back("early".into());
    let lease = PreviewLease::new(
        &runtime,
        Some("other".into()),
        runtime
            .preview
            .clone()
            .try_acquire_owned()
            .unwrap_or_else(|e| panic!("{e}")),
    )
    .unwrap_or_else(|e| panic!("{e}"));
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(5), lease.notify.notified())
            .await
            .is_err()
    );
    drop(lease);
    let lease = PreviewLease::new(
        &runtime,
        Some("early".into()),
        runtime
            .preview
            .clone()
            .try_acquire_owned()
            .unwrap_or_else(|e| panic!("{e}")),
    )
    .unwrap_or_else(|e| panic!("{e}"));
    assert!(
        tokio::time::timeout(
            std::time::Duration::from_millis(50),
            lease.notify.notified()
        )
        .await
        .is_ok()
    );
    drop(lease);
    assert!(
        runtime
            .active_preview
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .is_none()
    );
    assert!(
        PreviewLease::new(
            &runtime,
            Some("x".repeat(65)),
            runtime
                .preview
                .clone()
                .try_acquire_owned()
                .unwrap_or_else(|e| panic!("{e}"))
        )
        .is_err()
    );
    assert!(
        PreviewLease::new(
            &runtime,
            Some("bad/id".into()),
            runtime
                .preview
                .clone()
                .try_acquire_owned()
                .unwrap_or_else(|e| panic!("{e}"))
        )
        .is_err()
    );
}
