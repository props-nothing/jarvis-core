use super::*;
use serde_json::json;

#[tokio::test]
async fn speech_doctor_probes_each_provider_independently_and_redacts_failures() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .unwrap_or_else(|e| panic!("{e}"));
    let address = listener.local_addr().unwrap_or_else(|e| panic!("{e}"));
    let server = tokio::spawn(async move {
        let mut requests = Vec::new();
        for status in [401, 200] {
            let (mut socket, _) = listener.accept().await.unwrap_or_else(|e| panic!("{e}"));
            let mut request = Vec::new();
            loop {
                let mut buffer = [0; 4096];
                let count = socket.read(&mut buffer).await.unwrap_or(0);
                assert!(count > 0);
                request.extend_from_slice(&buffer[..count]);
                if request.ends_with(b"}") {
                    break;
                }
            }
            requests.push(String::from_utf8_lossy(&request).into_owned());
            let body = if status == 401 {
                "private-provider-secret"
            } else {
                "ID3-audio"
            };
            let reply = format!(
                "HTTP/1.1 {status} X\r\ncontent-type: audio/mpeg\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                body.len()
            );
            socket
                .write_all(reply.as_bytes())
                .await
                .unwrap_or_else(|e| panic!("{e}"));
        }
        requests
    });
    let root = std::env::temp_dir().join(format!(
        "jarvis-speech-doctor-{}",
        jarvis_core::scratch_tag()
    ));
    std::fs::create_dir_all(&root).unwrap_or_else(|e| panic!("{e}"));
    let paths = AppPaths::from_root(&root).unwrap_or_else(|e| panic!("{e}"));
    let mut document = "schema_version = 1\n[profile]\nname = 'default'\n[logging]\nlevel = 'warn'\n[daemon]\nshutdown_timeout_seconds = 30\n".to_owned();
    for id in ["primary", "local"] {
        use std::fmt::Write;
        writeln!(document, "[[daemon.speech.providers]]\nid = '{id}'\nkind = 'openai-compatible'\nbase_url = 'http://{address}/v1'\nmodel = 'kokoro'\nvoice = 'af_heart'").unwrap_or_else(|e| panic!("{e}"));
    }
    let config =
        jarvis_storage::Config::parse_with_environment(&document, Vec::<(String, String)>::new())
            .unwrap_or_else(|e| panic!("{e}"));
    ConfigStore::from_paths(&paths)
        .save(config.config())
        .unwrap_or_else(|e| panic!("{e}"));
    let findings = speech(&paths).await;
    assert_eq!(findings.len(), 2);
    assert_eq!(findings[0].level, Level::Fail);
    assert_eq!(findings[1].level, Level::Ok);
    assert!(findings[1].detail.contains("local"));
    assert!(!format!("{findings:?}").contains("private-provider-secret"));
    let requests = server.await.unwrap_or_else(|e| panic!("{e}"));
    assert!(
        requests
            .iter()
            .all(|request| request.contains("Systems online.")
                && request.starts_with("POST /v1/audio/speech"))
    );
    std::fs::remove_dir_all(root).unwrap_or_else(|e| panic!("{e}"));
}

#[test]
fn a_model_list_is_read_in_both_the_openai_and_the_ollama_shape() {
    let openai = json!({ "data": [{ "id": "gpt-x" }, { "id": "glm-5.3:cloud" }] });
    assert!(lists_model(&openai, "glm-5.3:cloud"));
    assert!(!lists_model(&openai, "other"));

    let ollama = json!({ "models": [{ "name": "llama3.2:latest" }] });
    assert!(lists_model(&ollama, "llama3.2:latest"));
    assert!(
        !lists_model(&ollama, "llama3.2"),
        "a name must match exactly, not by prefix"
    );

    assert!(!lists_model(&json!({}), "x"));
    assert!(!lists_model(&json!("nonsense"), "x"));
}

/// Each status maps to a verdict and, when it is not fine, to the command that fixes it.
#[test]
fn a_models_status_says_what_is_wrong_and_how_to_fix_it() {
    assert_eq!(classify_models_status(200).0, Level::Ok);
    for rejected in [401, 403] {
        let (level, _, fix) = classify_models_status(rejected);
        assert_eq!(level, Level::Fail);
        assert_eq!(fix, Some("jarvis keys set model"));
    }
    assert_eq!(
        classify_models_status(404).0,
        Level::Warn,
        "a missing list is not proof the server is broken"
    );
    assert_eq!(classify_models_status(500).0, Level::Fail);
    assert!(classify_models_status(500).2.is_some());
}

#[test]
fn every_failing_verdict_carries_a_fix() {
    for status in [401, 403, 404, 500, 502] {
        let (level, _, fix) = classify_models_status(status);
        assert!(
            level == Level::Ok || fix.is_some(),
            "{status} must say what to do"
        );
    }
}
