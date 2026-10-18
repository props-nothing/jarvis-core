//! In-process CPU speech. Public assets are pinned; user text never leaves the host.

mod assets;
mod engine;

use std::{
    fmt,
    future::Future,
    path::PathBuf,
    pin::Pin,
    sync::{Arc, Mutex},
};

use ort::session::RunOptions;
use tokio::sync::{Mutex as AsyncMutex, OnceCell, Semaphore};

use crate::{SpeechDescription, SpeechError, SpeechFuture, SpeechProvider, SpeechStream, speech};
use engine::Engine;

type Ready = Arc<Mutex<Engine>>;
type Initialization = tokio::task::JoinHandle<Result<Ready, SpeechError>>;

/// Embedded Kokoro synthesis with verified assets and no separate server.
pub struct EmbeddedSpeech {
    voice: String,
    speed: f32,
    blend: Option<jarvis_core::VoiceBlend>,
    shared: Arc<Shared>,
}

struct Shared {
    cache: PathBuf,
    download: bool,
    engines: [OnceCell<Ready>; 2],
    initialization: [AsyncMutex<Option<Initialization>>; 2],
    presets: [OnceCell<Arc<Vec<f32>>>; 2],
    capacity: Arc<Semaphore>,
    admission: Arc<Semaphore>,
}

impl fmt::Debug for EmbeddedSpeech {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EmbeddedSpeech")
            .field("voice", &self.voice)
            .field(
                "ready",
                &self.shared.engines[self.voice_index()].initialized(),
            )
            .finish_non_exhaustive()
    }
}

impl EmbeddedSpeech {
    /// Configure lazy local synthesis. Construction performs no network or native loading.
    ///
    /// # Errors
    /// Rejects unsupported platforms, voices or a relative cache directory.
    pub fn new(
        cache: &std::path::Path,
        voice: &str,
        allow_download: bool,
    ) -> Result<Self, SpeechError> {
        assets::supported()?;
        if !cache.is_absolute() || !matches!(voice, "af_heart" | "bm_george") {
            return Err(SpeechError::Settings(
                "embedded speech needs an absolute cache and voice af_heart or bm_george",
            ));
        }
        Ok(Self {
            voice: voice.into(),
            speed: 1.0,
            blend: None,
            shared: Arc::new(Shared {
                cache: cache.join("speech-kokoro-v1"),
                download: allow_download,
                engines: std::array::from_fn(|_| OnceCell::new()),
                initialization: std::array::from_fn(|_| AsyncMutex::new(None)),
                presets: std::array::from_fn(|_| OnceCell::new()),
                capacity: Arc::new(Semaphore::new(1)),
                admission: Arc::new(Semaphore::new(3)),
            }),
        })
    }

    fn voice_index(&self) -> usize {
        usize::from(
            self.blend
                .as_ref()
                .map_or(self.voice == "bm_george", |b| b.language == "en-gb"),
        )
    }

    async fn ready(&self) -> Result<Ready, SpeechError> {
        let slot = &self.shared.engines[self.voice_index()];
        if let Some(engine) = slot.get() {
            return Ok(Arc::clone(engine));
        }
        let mut pending = self.shared.initialization[self.voice_index()].lock().await;
        if let Some(engine) = slot.get() {
            return Ok(Arc::clone(engine));
        }
        if pending.is_none() {
            let (cache, voice, download) = (
                self.shared.cache.clone(),
                if self.voice_index() == 1 {
                    "bm_george".to_owned()
                } else {
                    "af_heart".to_owned()
                },
                self.shared.download,
            );
            *pending = Some(tokio::spawn(async move {
                let assets = assets::prepare(&cache, &voice, download).await?;
                tokio::task::spawn_blocking(move || {
                    Engine::new(&assets, &voice).map(|e| Arc::new(Mutex::new(e)))
                })
                .await
                .map_err(|_| SpeechError::BadAudio)?
            }));
        }
        // Borrow the handle: cancellation drops this waiter, not the initialization task.
        let result = pending.as_mut().ok_or(SpeechError::Unreachable)?.await;
        *pending = None;
        let engine = result.map_err(|_| SpeechError::BadAudio)??;
        let _ = slot.set(Arc::clone(&engine));
        Ok(engine)
    }

    async fn synthesize(&self, text: &str) -> Result<SpeechStream, SpeechError> {
        let text = speech::validate_text(text)?.to_owned();
        // One active sentence plus the console's two lookahead requests; no unbounded queue.
        let admission = Arc::clone(&self.shared.admission)
            .try_acquire_owned()
            .map_err(|_| SpeechError::Busy)?;
        let engine = self.ready().await?;
        let permit = Arc::clone(&self.shared.capacity)
            .acquire_owned()
            .await
            .map_err(|_| SpeechError::Unreachable)?;
        let options = Arc::new(RunOptions::new().map_err(|_| SpeechError::BadAudio)?);
        let cancellation = Cancel(Arc::clone(&options));
        let blend = self.blended_style().await?;
        let speed = self.speed;
        let job = tokio::task::spawn_blocking(move || {
            let _permit = permit;
            let _admission = admission;
            let mut engine = engine.lock().map_err(|_| SpeechError::BadAudio)?;
            engine.synthesize(&text, speed, blend.as_deref(), &options)
        });
        let bytes = tokio::time::timeout(crate::REQUEST_TIMEOUT, job)
            .await
            .map_err(|_| SpeechError::Unreachable)?
            .map_err(|_| SpeechError::BadAudio)??;
        drop(cancellation);
        Ok(SpeechStream::new(
            futures_util::stream::once(async { Ok(bytes.into()) }),
            "audio/wav",
        ))
    }

    async fn blended_style(&self) -> Result<Option<Vec<f32>>, SpeechError> {
        let Some(blend) = &self.blend else {
            return Ok(None);
        };
        let mut packs = Vec::with_capacity(2);
        for (index, voice) in ["af_heart", "bm_george"].into_iter().enumerate() {
            let pack = self.shared.presets[index]
                .get_or_try_init(|| async {
                    let bytes =
                        assets::prepare_voice(&self.shared.cache, voice, self.shared.download)
                            .await?;
                    engine::decode_voice(&bytes).map(Arc::new)
                })
                .await?;
            packs.push(Arc::clone(pack));
        }
        let heart = blend
            .sources
            .iter()
            .find(|s| s.voice == "af_heart")
            .ok_or(SpeechError::BadAudio)?
            .weight;
        engine::mix_voice(&packs[0], &packs[1], heart).map(Some)
    }
}

// Held by the async caller. Aborting a task or dropping its future terminates native inference.
struct Cancel(Arc<RunOptions>);
impl Drop for Cancel {
    fn drop(&mut self) {
        let _ = self.0.terminate();
    }
}

impl SpeechProvider for EmbeddedSpeech {
    // Validation bounds speed to 0.5–2 before conversion to the model's float32 tensor.
    #[allow(clippy::cast_possible_truncation)]
    fn configured(
        &self,
        voice: &jarvis_core::SavedVoice,
    ) -> Result<Arc<dyn SpeechProvider>, SpeechError> {
        crate::validate_voice("embedded-kokoro", voice)?;
        Ok(Arc::new(Self {
            voice: voice.voice.clone(),
            speed: voice.settings.speed.unwrap_or(1.0) as f32,
            blend: voice.blend.clone(),
            shared: Arc::clone(&self.shared),
        }))
    }
    fn describe(&self) -> SpeechDescription {
        SpeechDescription {
            id: "embedded-kokoro".into(),
            provider: "embedded-kokoro".into(),
            voice: self.voice.clone(),
            model: "kokoro-v1.0".into(),
        }
    }

    fn prepare(&self) -> Pin<Box<dyn Future<Output = Result<(), SpeechError>> + Send + '_>> {
        Box::pin(async { self.ready().await.map(|_| ()) })
    }

    fn open<'a>(&'a self, text: &'a str, _previous_text: Option<&'a str>) -> SpeechFuture<'a> {
        Box::pin(async {
            tokio::time::timeout(crate::REQUEST_TIMEOUT, self.synthesize(text))
                .await
                .map_err(|_| SpeechError::Unreachable)?
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn lookahead_queue_is_bounded_and_cancellation_releases_admission()
    -> Result<(), Box<dyn std::error::Error>> {
        let provider = Arc::new(EmbeddedSpeech::new(
            &std::env::temp_dir(),
            "af_heart",
            false,
        )?);
        let _block_preparation = provider.shared.initialization[0].lock().await;
        let mut jobs = Vec::new();
        for _ in 0..3 {
            let p = Arc::clone(&provider);
            jobs.push(tokio::spawn(async move {
                p.open("Systems online.", None).await
            }));
        }
        tokio::time::timeout(std::time::Duration::from_secs(2), async {
            while provider.shared.admission.available_permits() != 0 {
                tokio::task::yield_now().await;
            }
        })
        .await?;
        assert!(matches!(
            provider.open("Fourth sentence.", None).await,
            Err(SpeechError::Busy)
        ));
        for job in jobs {
            job.abort();
            let _ = job.await;
        }
        assert_eq!(provider.shared.admission.available_permits(), 3);
        Ok(())
    }

    #[tokio::test]
    async fn offline_missing_assets_and_invalid_input_fail_without_loading_native_code() {
        let dir =
            std::env::temp_dir().join(format!("jarvis-embedded-offline-{}", std::process::id()));
        let provider =
            EmbeddedSpeech::new(&dir, "af_heart", false).unwrap_or_else(|error| panic!("{error}"));
        assert!(matches!(
            provider.open(" ", None).await,
            Err(SpeechError::EmptyText)
        ));
        assert!(matches!(
            provider.open(&"x".repeat(1501), None).await,
            Err(SpeechError::TooLong)
        ));
        assert!(matches!(
            provider.prepare().await,
            Err(SpeechError::Settings(_))
        ));
        assert!(
            !dir.join("speech-kokoro-v1/onnx-model_quantized.onnx")
                .exists()
        );
        assert!(EmbeddedSpeech::new(&dir, "../other", true).is_err());
        assert!(EmbeddedSpeech::new(std::path::Path::new("relative"), "af_heart", true).is_err());
    }
}
