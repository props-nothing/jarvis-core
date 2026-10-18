use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::time::Instant;

use crate::{
    SpeechDescription, SpeechError, SpeechFuture, SpeechProvider, SpeechStatus, SpeechStream,
    speech,
};

#[derive(Debug, Default)]
struct Health {
    until: Option<Instant>,
    probing: bool,
    healthy: bool,
    generation: u64,
}

#[derive(Debug)]
struct Entry {
    id: String,
    provider: Arc<dyn SpeechProvider>,
    health: Arc<Mutex<Health>>,
}

/// Ordered providers with bounded attempts and per-provider recovery probes.
#[derive(Debug)]
pub struct FallbackSpeech {
    entries: Vec<Entry>,
    active: Mutex<Option<String>>,
}

// Cancellation must never strand a half-open breaker. This lease also fences
// older concurrent successes against a newer failure.
struct Attempt {
    health: Arc<Mutex<Health>>,
    generation: u64,
    probe: bool,
}

impl Attempt {
    fn reserve(health: &Arc<Mutex<Health>>) -> Option<Self> {
        let mut state = health
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if state.probing || state.until.is_some_and(|until| until > Instant::now()) {
            return None;
        }
        let probe = state.until.is_some();
        state.probing = probe;
        Some(Self {
            health: Arc::clone(health),
            generation: state.generation,
            probe,
        })
    }

    fn success(&mut self) {
        let mut state = self
            .health
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if state.generation == self.generation {
            state.until = None;
            state.healthy = true;
            state.probing = false;
        }
        self.probe = false;
    }

    fn failure(&self, error: &SpeechError) {
        let mut state = self
            .health
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if state.generation != self.generation {
            return;
        }
        let seconds = if matches!(error, SpeechError::KeyRejected | SpeechError::OutOfCredit) {
            600
        } else {
            30
        };
        state.until = Some(Instant::now() + Duration::from_secs(seconds));
        state.healthy = false;
        state.probing = false;
        state.generation += 1;
    }
}

impl Drop for Attempt {
    fn drop(&mut self) {
        if self.probe {
            let mut state = self
                .health
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if state.generation == self.generation {
                state.probing = false;
            }
        }
    }
}

impl FallbackSpeech {
    /// Compose up to eight independently named providers, in preference order.
    ///
    /// # Errors
    /// Refuses empty/oversized lists, duplicate ids and ids unsafe for response headers.
    pub fn new(providers: Vec<(String, Arc<dyn SpeechProvider>)>) -> Result<Self, SpeechError> {
        if providers.is_empty() || providers.len() > 8 {
            return Err(SpeechError::Settings(
                "configure between one and eight speech providers",
            ));
        }
        let mut entries: Vec<Entry> = Vec::new();
        for (id, provider) in providers {
            if id.is_empty()
                || id.len() > 64
                || !id
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_'))
                || entries.iter().any(|entry| entry.id == id)
            {
                return Err(SpeechError::Settings(
                    "speech provider ids must be unique plain identifiers",
                ));
            }
            entries.push(Entry {
                id,
                provider,
                health: Arc::default(),
            });
        }
        Ok(Self {
            entries,
            active: Mutex::new(None),
        })
    }

    async fn select(
        &self,
        text: &str,
        previous: Option<&str>,
        preferred: Option<&str>,
    ) -> Result<SpeechStream, SpeechError> {
        speech::validate_text(text)?;
        let preferred =
            preferred.and_then(|id| self.entries.iter().position(|entry| entry.id == id));
        let order = preferred
            .into_iter()
            .chain((0..self.entries.len()).filter(|index| Some(*index) != preferred));
        let mut last_error = SpeechError::Unreachable;
        for index in order {
            let entry = &self.entries[index];
            let Some(mut attempt) = Attempt::reserve(&entry.health) else {
                continue;
            };
            let deadline = entry
                .provider
                .synthesis_timeout()
                .min(Duration::from_secs(180));
            let opened = tokio::time::timeout(deadline, async {
                let mut stream = entry.provider.open(text, previous).await?;
                let first = stream.next_chunk().await?.ok_or(SpeechError::BadAudio)?;
                Ok::<_, SpeechError>((stream, first))
            })
            .await
            .unwrap_or(Err(SpeechError::Unreachable));
            match opened {
                Ok((stream, first)) => {
                    attempt.success();
                    *self
                        .active
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner) =
                        Some(entry.id.clone());
                    let mime = stream.content_type();
                    let chunks = futures_util::stream::unfold(
                        (Some(first), stream, attempt),
                        |(first, mut stream, attempt)| async move {
                            if let Some(first) = first {
                                return Some((Ok(first), (None, stream, attempt)));
                            }
                            match stream.next_chunk().await {
                                Ok(Some(chunk)) => Some((Ok(chunk), (None, stream, attempt))),
                                Ok(None) => None,
                                Err(error) => {
                                    attempt.failure(&error);
                                    Some((Err(error), (None, stream, attempt)))
                                }
                            }
                        },
                    );
                    let mut stream = SpeechStream::new(chunks, mime);
                    stream.selection = Some((entry.id.clone(), index != 0));
                    return Ok(stream);
                }
                Err(
                    error @ (SpeechError::EmptyText | SpeechError::TooLong | SpeechError::Busy),
                ) => {
                    return Err(error);
                }
                Err(error) => {
                    attempt.failure(&error);
                    last_error = error;
                }
            }
        }
        Err(last_error)
    }
}

impl SpeechProvider for FallbackSpeech {
    fn describe(&self) -> SpeechDescription {
        let active = self
            .active
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let entry = self
            .entries
            .iter()
            .find(|entry| active.as_deref() == Some(&entry.id))
            .unwrap_or(&self.entries[0]);
        let mut description = entry.provider.describe();
        description.id.clone_from(&entry.id);
        description
    }

    fn open<'a>(&'a self, text: &'a str, previous: Option<&'a str>) -> SpeechFuture<'a> {
        Box::pin(self.select(text, previous, None))
    }

    fn open_preferred<'a>(
        &'a self,
        text: &'a str,
        previous: Option<&'a str>,
        preferred: Option<&'a str>,
    ) -> SpeechFuture<'a> {
        Box::pin(self.select(text, previous, preferred))
    }

    fn statuses(&self) -> Vec<SpeechStatus> {
        self.entries
            .iter()
            .map(|entry| {
                let state = entry
                    .health
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                let mut description = entry.provider.describe();
                description.id.clone_from(&entry.id);
                SpeechStatus {
                    description,
                    health: if state.probing {
                        "probing"
                    } else if state.until.is_some() {
                        "cooling_down"
                    } else if state.healthy {
                        "healthy"
                    } else {
                        "unknown"
                    },
                    retry_after_seconds: state.until.map_or(0, |until| {
                        until.saturating_duration_since(Instant::now()).as_secs()
                    }),
                }
            })
            .collect()
    }

    fn active(&self) -> Option<String> {
        self.active
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }
}

#[cfg(test)]
mod tests;
