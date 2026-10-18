//! Explicit opt-in real-model acceptance. No cloud key or speech server is used.
#![cfg(feature = "local-tts")]

use jarvis_voice::{
    EmbeddedSpeech, FallbackSpeech, OpenAiCompatibleSpeech, SpeechProvider, collect_audio,
};
use std::{path::PathBuf, sync::Arc, time::Instant};

#[tokio::test]
#[ignore = "downloads pinned public assets; set JARVIS_EMBEDDED_TEST_CACHE to an absolute disposable cache"]
async fn embedded_real_model_and_offline_fallback() -> Result<(), Box<dyn std::error::Error>> {
    let cache = PathBuf::from(std::env::var("JARVIS_EMBEDDED_TEST_CACHE")?);
    if !cache.is_absolute() {
        return Err("test cache must be absolute".into());
    }
    let voice = std::env::var("JARVIS_EMBEDDED_TEST_VOICE").unwrap_or_else(|_| "af_heart".into());
    let downloads = std::env::var("JARVIS_EMBEDDED_TEST_OFFLINE").is_err();
    let local = Arc::new(EmbeddedSpeech::new(&cache, &voice, downloads)?);
    let prepared = Instant::now();
    local.prepare().await?;
    eprintln!("embedded preparation_ms={}", prepared.elapsed().as_millis());
    let cold = Instant::now();
    let audio = collect_audio(
        local
            .open("Systems online. The local voice is ready.", None)
            .await?,
    )
    .await?;
    eprintln!(
        "embedded first_audio_ms={} wav_bytes={}",
        cold.elapsed().as_millis(),
        audio.bytes().len()
    );
    assert_eq!(audio.content_type(), "audio/wav");
    assert_eq!(&audio.bytes()[..4], b"RIFF");
    assert!(audio.bytes()[44..].iter().any(|byte| *byte != 0));
    std::fs::write(cache.join(format!("acceptance-{voice}.wav")), audio.bytes())?;
    // Closed ephemeral port gives a real connection failure, then the real local engine supplies WAV.
    let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
    let address = listener.local_addr()?;
    drop(listener);
    let primary = Arc::new(OpenAiCompatibleSpeech::new(
        &format!("http://{address}/v1"),
        "unused",
        "unused",
        None,
    )?);
    let route = FallbackSpeech::new(vec![
        ("primary".into(), primary),
        ("local".into(), local.clone()),
    ])?;
    let warm = Instant::now();
    let stream = route
        .open("The backup voice works without a separate server.", None)
        .await?;
    assert_eq!(stream.selection(), Some(("local", true)));
    let audio = collect_audio(stream).await?;
    eprintln!(
        "embedded fallback_first_audio_ms={} wav_bytes={}",
        warm.elapsed().as_millis(),
        audio.bytes().len()
    );
    std::fs::write(cache.join(format!("fallback-{voice}.wav")), audio.bytes())?;
    // The console prefetches two sentences. All three requests must finish as WAV.
    let lookahead = futures_util::future::try_join_all((0..3).map(|_| async {
        collect_audio(local.open("The next sentence is ready.", None).await?).await
    }))
    .await?;
    assert_eq!(lookahead.len(), 3);
    // Cancel a long native run, then prove capacity becomes available promptly.
    let worker = Arc::clone(&local);
    let cancelled = tokio::spawn(async move {
        worker
            .open(&"This is a long sentence to cancel. ".repeat(40), None)
            .await
    });
    tokio::time::sleep(std::time::Duration::from_millis(30)).await;
    cancelled.abort();
    let _ = cancelled.await;
    let recovered = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        local.open("Cancellation recovered.", None),
    )
    .await??;
    assert_eq!(collect_audio(recovered).await?.content_type(), "audio/wav");
    // Recreate from the verified cache with downloads prohibited.
    let offline = EmbeddedSpeech::new(&cache, &voice, false)?;
    offline.prepare().await?;
    let audio = collect_audio(
        offline
            .open("Offline synthesis is available.", None)
            .await?,
    )
    .await?;
    assert_eq!(audio.content_type(), "audio/wav");
    Ok(())
}
