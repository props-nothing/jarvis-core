use super::*;
use bytes::Bytes;
use std::sync::atomic::{AtomicUsize, Ordering};

#[derive(Debug)]
struct Fake {
    calls: AtomicUsize,
    fail: AtomicUsize,
}

impl Fake {
    fn new(fail: usize) -> Arc<Self> {
        Arc::new(Self {
            calls: AtomicUsize::new(0),
            fail: AtomicUsize::new(fail),
        })
    }
}

impl SpeechProvider for Fake {
    fn describe(&self) -> SpeechDescription {
        SpeechDescription {
            id: "fake".into(),
            provider: "test".into(),
            voice: "voice".into(),
            model: "model".into(),
        }
    }
    fn open<'a>(&'a self, _: &'a str, _: Option<&'a str>) -> SpeechFuture<'a> {
        Box::pin(async move {
            self.calls.fetch_add(1, Ordering::SeqCst);
            match self.fail.load(Ordering::SeqCst) {
                401 => Err(SpeechError::KeyRejected),
                402 => Err(SpeechError::OutOfCredit),
                429 => Err(SpeechError::OverLimit),
                503 => Err(SpeechError::Busy),
                500 => Err(SpeechError::Provider(500)),
                1 => Err(SpeechError::Unreachable),
                2 => Ok(SpeechStream::new(
                    futures_util::stream::empty(),
                    "audio/wav",
                )),
                3 => Ok(SpeechStream::new(
                    futures_util::stream::iter([
                        Ok(Bytes::from_static(b"first")),
                        Err(SpeechError::Unreachable),
                    ]),
                    "audio/wav",
                )),
                4 => std::future::pending().await,
                _ => Ok(SpeechStream::new(
                    futures_util::stream::iter([Ok(Bytes::from_static(b"audio"))]),
                    "audio/wav",
                )),
            }
        })
    }
}

fn chain(primary: &Arc<Fake>, backup: &Arc<Fake>) -> FallbackSpeech {
    FallbackSpeech::new(vec![
        ("primary".into(), primary.clone()),
        ("local".into(), backup.clone()),
    ])
    .unwrap_or_else(|e| panic!("{e}"))
}

#[tokio::test]
async fn busy_admission_keeps_the_selected_voice_and_does_not_open_a_breaker() {
    let primary = Fake::new(503);
    let backup = Fake::new(0);
    let chain = chain(&primary, &backup);
    assert!(matches!(
        chain.open("first", None).await,
        Err(SpeechError::Busy)
    ));
    assert_eq!(backup.calls.load(Ordering::SeqCst), 0);
    assert_ne!(chain.statuses()[0].health, "cooling_down");
    primary.fail.store(0, Ordering::SeqCst);
    let stream = chain
        .open("retry", None)
        .await
        .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(stream.selection(), Some(("primary", false)));
    assert_eq!(primary.calls.load(Ordering::SeqCst), 2);
}

#[tokio::test(start_paused = true)]
async fn an_attempt_without_first_audio_times_out_and_all_failed_providers_are_skipped() {
    let primary = Fake::new(4);
    let backup = Fake::new(500);
    let chain = chain(&primary, &backup);
    let start = Instant::now();
    assert!(chain.open("hello", None).await.is_err());
    assert_eq!(Instant::now() - start, crate::REQUEST_TIMEOUT);
    assert!(chain.open("again", None).await.is_err());
    assert_eq!(primary.calls.load(Ordering::SeqCst), 1);
    assert_eq!(backup.calls.load(Ordering::SeqCst), 1);
}

#[tokio::test(start_paused = true)]
async fn failures_skip_then_recover_and_answers_keep_their_voice() {
    for code in [1, 2, 401, 402, 429, 500] {
        let primary = Fake::new(code);
        let backup = Fake::new(0);
        let chain = chain(&primary, &backup);
        let stream = chain
            .open("hello", None)
            .await
            .unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(stream.selection(), Some(("local", true)));
        assert_eq!(stream.content_type(), "audio/wav");
        assert_eq!(chain.active().as_deref(), Some("local"));
        assert!(chain.open("again", None).await.is_ok());
        assert_eq!(primary.calls.load(Ordering::SeqCst), 1);
        primary.fail.store(0, Ordering::SeqCst);
        tokio::time::advance(Duration::from_secs(31)).await;
        assert!(chain.open("again", None).await.is_ok());
        let long = matches!(code, 401 | 402);
        assert_eq!(
            primary.calls.load(Ordering::SeqCst),
            if long { 1 } else { 2 }
        );
        tokio::time::advance(Duration::from_secs(600)).await;
        let preferred = chain
            .open_preferred("same answer", None, Some("local"))
            .await
            .unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(preferred.selection(), Some(("local", true)));
        let recovered = chain
            .open("new answer", None)
            .await
            .unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(recovered.selection(), Some(("primary", false)));
        assert_eq!(chain.statuses()[0].health, "healthy");
    }
}

#[tokio::test]
async fn mid_stream_failure_never_splices_audio_and_opens_the_breaker() {
    let primary = Fake::new(3);
    let backup = Fake::new(0);
    let chain = chain(&primary, &backup);
    let mut stream = chain
        .open("hello", None)
        .await
        .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(
        stream.next_chunk().await.ok().flatten().as_deref(),
        Some(b"first".as_slice())
    );
    assert!(matches!(
        stream.next_chunk().await,
        Err(SpeechError::Unreachable)
    ));
    assert!(matches!(stream.next_chunk().await, Ok(None)));
    assert_eq!(backup.calls.load(Ordering::SeqCst), 0);
    assert_eq!(chain.statuses()[0].health, "cooling_down");
    assert_eq!(
        chain
            .open_preferred("next", None, Some("primary"))
            .await
            .unwrap_or_else(|e| panic!("{e}"))
            .selection(),
        Some(("local", true))
    );
}

#[tokio::test(start_paused = true)]
async fn only_one_recovery_probe_runs_and_cancellation_releases_it() {
    let primary = Fake::new(500);
    let backup = Fake::new(0);
    let chain = Arc::new(chain(&primary, &backup));
    assert!(chain.open("first", None).await.is_ok());
    tokio::time::advance(Duration::from_secs(31)).await;
    primary.fail.store(4, Ordering::SeqCst);
    let running = Arc::clone(&chain);
    let probe = tokio::spawn(async move { running.open("probe", None).await });
    tokio::task::yield_now().await;
    assert_eq!(chain.statuses()[0].health, "probing");
    assert!(chain.open("concurrent", None).await.is_ok());
    assert_eq!(primary.calls.load(Ordering::SeqCst), 2);
    probe.abort();
    let _ = probe.await;
    primary.fail.store(0, Ordering::SeqCst);
    assert!(chain.open("recovery", None).await.is_ok());
    assert_eq!(primary.calls.load(Ordering::SeqCst), 3);
}

#[tokio::test]
async fn invalid_text_never_calls_a_provider_and_late_success_cannot_clear_failure() {
    let primary = Fake::new(0);
    let backup = Fake::new(0);
    let chain = chain(&primary, &backup);
    assert!(matches!(
        chain.open("  ", None).await,
        Err(SpeechError::EmptyText)
    ));
    assert!(matches!(
        chain
            .open(&"a".repeat(crate::MAX_TEXT_CHARS + 1), None)
            .await,
        Err(SpeechError::TooLong)
    ));
    assert_eq!(primary.calls.load(Ordering::SeqCst), 0);
    assert_eq!(backup.calls.load(Ordering::SeqCst), 0);
    let health = Arc::default();
    let mut old = Attempt::reserve(&health).unwrap_or_else(|| panic!("reserve"));
    let new = Attempt::reserve(&health).unwrap_or_else(|| panic!("reserve"));
    new.failure(&SpeechError::KeyRejected);
    old.success();
    assert!(Attempt::reserve(&health).is_none());
}
