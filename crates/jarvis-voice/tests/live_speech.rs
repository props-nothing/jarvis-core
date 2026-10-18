//! Opt-in real speech acceptance; secrets use an ephemeral environment or private external file.

use jarvis_voice::{OpenAiCompatibleSpeech, SpeechProvider, collect_audio};

#[tokio::test]
#[ignore = "requires an explicitly configured speech server; cloud synthesis may cost credits"]
async fn configured_server_produces_live_mp3() {
    let setting = |name| std::env::var(name).unwrap_or_else(|_| panic!("set {name}"));
    let base = setting("JARVIS_LIVE_SPEECH_BASE_URL");
    let voice = setting("JARVIS_LIVE_SPEECH_VOICE");
    let model = setting("JARVIS_LIVE_SPEECH_MODEL");
    let key = std::env::var("JARVIS_LIVE_SPEECH_KEY").ok().or_else(|| {
        std::env::var_os("JARVIS_LIVE_SPEECH_KEY_FILE").map(|path| {
            std::fs::read_to_string(path)
                .unwrap_or_else(|_| panic!("the live key file could not be read"))
        })
    });
    let provider = OpenAiCompatibleSpeech::new(&base, &voice, &model, key.as_deref())
        .unwrap_or_else(|e| panic!("{e}"));
    let start = std::time::Instant::now();
    let mut stream = provider
        .open("JARVIS speech is online. Your backup voice is ready.", None)
        .await
        .unwrap_or_else(|e| panic!("{e}"));
    let first = stream
        .next_chunk()
        .await
        .unwrap_or_else(|e| panic!("{e}"))
        .unwrap_or_else(|| panic!("no audio"));
    let first_millis = start.elapsed().as_millis();
    let mut bytes = first.to_vec();
    // The first chunk is consumed above for latency; collect only the remaining stream.
    let rest = collect_audio(stream)
        .await
        .unwrap_or_else(|e| panic!("{e}"));
    bytes.extend_from_slice(rest.bytes());
    assert_eq!(rest.content_type(), "audio/mpeg");
    assert!(
        bytes.len() > 1000,
        "a spoken sentence needs more than a header"
    );
    assert!(
        bytes.starts_with(b"ID3") || (bytes[0] == 0xff && bytes[1] & 0xe0 == 0xe0),
        "expected MP3 framing"
    );
    if let Some(output) = std::env::var_os("JARVIS_LIVE_SPEECH_OUTPUT") {
        std::fs::write(output, &bytes).unwrap_or_else(|_| panic!("could not save live audio"));
    }
    println!(
        "speech acceptance: first_audio_ms={first_millis}, total_ms={}, bytes={}, content_type=audio/mpeg",
        start.elapsed().as_millis(),
        bytes.len()
    );
}
