use super::*;
use crate::{DesignEvent, DesignStream, PCM_CONTENT_TYPE};
use tokio::sync::{OwnedMutexGuard, OwnedSemaphorePermit};

impl Shared {
    fn phase(&self, phase: &'static str) {
        let mut status = self
            .execution
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        status.phase = phase;
        if phase == "cold" {
            status.allocated_bytes = 0;
        }
    }
}

/// Reset readiness even when cancellation drops an in-flight future.
struct Busy(Arc<Shared>, bool);
impl Busy {
    fn ready(&mut self) {
        self.0.phase("ready");
        self.1 = false;
    }
}
impl Drop for Busy {
    fn drop(&mut self) {
        if self.1 {
            self.0.phase("cold");
        }
    }
}

impl DescriptionSpeech {
    pub(super) async fn warm(&self) -> Result<(), SpeechError> {
        let _permit = self
            .shared
            .admission
            .clone()
            .try_acquire_owned()
            .map_err(|_| SpeechError::Busy)?;
        tokio::time::timeout(DEADLINE, async {
            let mut slot = self.shared.worker.lock().await;
            if slot.is_some() {
                return Ok(());
            }
            let mut busy = Busy(self.shared.clone(), true);
            self.shared.phase("checking");
            let gpu = !self.shared.cpu_only.load(Ordering::Relaxed);
            let worker = self
                .retry_on_cpu(
                    gpu,
                    Worker::start(&self.shared.root, gpu, &self.shared),
                    || Worker::start(&self.shared.root, false, &self.shared),
                )
                .await?;
            self.observe(&worker.info);
            *slot = Some(worker);
            busy.ready();
            Ok(())
        })
        .await
        .map_err(|_| SpeechError::Unreachable)?
    }

    pub(super) async fn render_stream(
        &self,
        text: &str,
        description: Option<&str>,
        reference: Option<&str>,
        seed: u32,
        quality: VoiceQuality,
    ) -> Result<SpeechStream, SpeechError> {
        crate::speech::validate_text(text)?;
        let permit = self
            .shared
            .admission
            .clone()
            .try_acquire_owned()
            .map_err(|_| SpeechError::Busy)?;
        let deadline = tokio::time::Instant::now() + DEADLINE;
        tokio::time::timeout_at(deadline, async {
            let mut slot = self.shared.worker.clone().lock_owned().await;
            let busy = Busy(self.shared.clone(), true);
            let mut cached = slot.take();
            let request = serde_json::json!({"text":text,"description":description,"reference":reference,"seed":seed,"steps":quality.steps()});
            let mut encoded = serde_json::to_vec(&request).map_err(|_| SpeechError::BadAudio)?;
            encoded.push(b'\n');
            if encoded.len() > 2 * 1024 * 1024 { return Err(SpeechError::BadAudio); }
            let gpu = !self.shared.cpu_only.load(Ordering::Relaxed);
            let first = async {
                let mut worker = match cached.take() {
                    Some(worker) => worker,
                    None => Worker::start(&self.shared.root, gpu, &self.shared).await?,
                };
                self.observe(&worker.info);
                worker.send(&encoded).await?;
                let first = worker.next_audio(&self.shared).await?.ok_or(SpeechError::BadAudio)?;
                Ok((worker, first))
            };
            let (worker, first) = self.retry_on_cpu(gpu, first, || async {
                let mut worker = Worker::start(&self.shared.root, false, &self.shared).await?;
                self.observe(&worker.info);
                worker.send(&encoded).await?;
                let first = worker.next_audio(&self.shared).await?.ok_or(SpeechError::BadAudio)?;
                Ok((worker, first))
            }).await?;
            let state = Running { worker: Some(worker), slot, _permit: permit, busy, deadline,
                first: Some(first), received: 0, limit: 96000 * if description.is_some() { 10 } else { 60 } };
            Ok(SpeechStream::new(futures_util::stream::unfold(Some(state), |state| async {
                let mut state = state?;
                match state.next().await {
                    Ok(Some(bytes)) => Some((Ok(bytes.into()), Some(state))),
                    Ok(None) => None,
                    Err(error) => Some((Err(error), None)),
                }
            }), PCM_CONTENT_TYPE))
        }).await.map_err(|_| SpeechError::Unreachable)?
    }

    pub(super) async fn stream_design(
        &self,
        description: &str,
        sample: &str,
        seed: u32,
        quality: VoiceQuality,
    ) -> Result<DesignStream, SpeechError> {
        DesignedVoice::validate_prompt(description, sample).map_err(SpeechError::Settings)?;
        let audio = self
            .render_stream(sample, Some(description), None, seed, quality)
            .await?;
        let recipe = DesignedVoice {
            version: 1,
            model_revision: DESIGN_MODEL_REVISION.into(),
            description: description.into(),
            sample_text: sample.into(),
            seed,
            quality,
            reference_audio: String::new(),
        };
        Ok(Box::pin(futures_util::stream::unfold(
            Some((audio, Vec::new(), recipe)),
            |state| async {
                let (mut audio, mut pcm, mut recipe) = state?;
                match audio.next_chunk().await {
                    Ok(Some(chunk)) if pcm.len() + chunk.len() <= 960_000 => {
                        pcm.extend_from_slice(&chunk);
                        Some((Ok(DesignEvent::Audio(chunk)), Some((audio, pcm, recipe))))
                    }
                    Ok(None) => {
                        let result = crate::speech::pcm_wav(&pcm).and_then(|wav| {
                            validate_wav(&wav, 10)?;
                            recipe.reference_audio = STANDARD.encode(wav);
                            Ok(DesignEvent::Complete(recipe))
                        });
                        Some((result, None))
                    }
                    Ok(Some(_)) => Some((Err(SpeechError::BadAudio), None)),
                    Err(error) => Some((Err(error), None)),
                }
            },
        )))
    }
}

struct Running {
    busy: Busy,
    worker: Option<Worker>,
    slot: OwnedMutexGuard<Option<Worker>>,
    _permit: OwnedSemaphorePermit,
    deadline: tokio::time::Instant,
    first: Option<Vec<u8>>,
    received: usize,
    limit: usize,
}
impl Running {
    async fn next(&mut self) -> Result<Option<Vec<u8>>, SpeechError> {
        let bytes = if let Some(first) = self.first.take() {
            Some(first)
        } else {
            let worker = self.worker.as_mut().ok_or(SpeechError::BadAudio)?;
            tokio::time::timeout_at(self.deadline, worker.next_audio(&self.busy.0))
                .await
                .map_err(|_| SpeechError::Unreachable)??
        };
        if let Some(bytes) = bytes {
            self.received += bytes.len();
            if self.received > self.limit {
                return Err(SpeechError::BadAudio);
            }
            Ok(Some(bytes))
        } else {
            *self.slot = self.worker.take();
            self.busy.ready();
            Ok(None)
        }
    }
}

async fn frame(
    reader: &mut (impl tokio::io::AsyncRead + Unpin),
) -> Result<(u8, Vec<u8>), SpeechError> {
    let tag = reader
        .read_u8()
        .await
        .map_err(|_| SpeechError::Unreachable)?;
    let length = reader
        .read_u32_le()
        .await
        .map_err(|_| SpeechError::Unreachable)? as usize;
    let valid = match tag {
        b'P' => (1..=16).contains(&length),
        b'M' => (1..=1024).contains(&length),
        b'A' => (2..=96000).contains(&length) && length.is_multiple_of(2),
        b'D' | b'E' => length == 0,
        _ => false,
    };
    if !valid {
        return Err(SpeechError::BadAudio);
    }
    let mut bytes = vec![0; length];
    reader
        .read_exact(&mut bytes)
        .await
        .map_err(|_| SpeechError::Unreachable)?;
    Ok((tag, bytes))
}

impl Worker {
    fn status(&mut self, tag: u8, data: &[u8], shared: &Shared) -> Result<(), SpeechError> {
        match tag {
            b'P' => shared.phase(match data {
                b"checking" => "checking",
                b"loading" => "loading",
                b"warming" => "warming",
                b"encoding" => "encoding",
                b"generating" => "generating",
                _ => return Err(SpeechError::BadAudio),
            }),
            b'M' => {
                self.info = WorkerInfo::parse(data, self.gpu)?;
                let mut status = shared
                    .execution
                    .write()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                status.backend.clone_from(&self.info.backend);
                status.device.clone_from(&self.info.device);
                status.allocated_bytes = self.info.allocated_bytes;
                status.peak_bytes = self.info.peak_bytes;
            }
            _ => return Err(SpeechError::BadAudio),
        }
        Ok(())
    }
    pub(super) async fn boot(&mut self, shared: &Shared) -> Result<(), SpeechError> {
        for _ in 0..12 {
            let (tag, bytes) = frame(&mut self.output).await?;
            if tag == b'D' && !self.info.backend.is_empty() {
                return Ok(());
            }
            self.status(tag, &bytes, shared)?;
        }
        Err(SpeechError::BadAudio)
    }
    async fn send(&mut self, encoded: &[u8]) -> Result<(), SpeechError> {
        self.input
            .write_all(encoded)
            .await
            .map_err(|_| SpeechError::Unreachable)?;
        self.input
            .flush()
            .await
            .map_err(|_| SpeechError::Unreachable)
    }
    async fn next_audio(&mut self, shared: &Shared) -> Result<Option<Vec<u8>>, SpeechError> {
        for _ in 0..8 {
            let (tag, bytes) = frame(&mut self.output).await?;
            match tag {
                b'A' => return Ok(Some(bytes)),
                b'D' => return Ok(None),
                _ => self.status(tag, &bytes, shared)?,
            }
        }
        Err(SpeechError::BadAudio)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn framing_rejects_invalid_sizes_types_and_truncation_before_allocation() {
        for (tag, length, data) in [
            (b'A', 96002, vec![]),
            (b'A', 3, vec![1; 3]),
            (b'M', 1025, vec![]),
            (b'D', 1, vec![0]),
            (b'X', 0, vec![]),
            (b'A', 4, vec![1; 2]),
        ] {
            let mut wire = vec![tag];
            wire.extend_from_slice(&u32::to_le_bytes(length));
            wire.extend(data);
            assert!(frame(&mut wire.as_slice()).await.is_err());
        }
        assert!(frame(&mut b"A\x02\0\0\0\x01\0".as_slice()).await.is_ok());
    }
    #[test]
    fn cancelled_preparation_does_not_leave_false_ready_status() {
        let provider = DescriptionSpeech::new(&std::env::temp_dir().join("voice-warm-guard"))
            .unwrap_or_else(|e| panic!("{e}"));
        provider.shared.phase("warming");
        drop(Busy(provider.shared.clone(), true));
        assert_eq!(
            provider
                .execution()
                .unwrap_or_else(|| panic!("expected execution"))
                .phase,
            "cold"
        );
    }
}
