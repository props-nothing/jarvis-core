//! Description-conditioned local synthesis in an owned, pipe-only worker.

use crate::{
    SpeechDescription, SpeechError, SpeechExecution, SpeechFuture, SpeechProvider, SpeechStream,
};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use jarvis_core::{DesignedVoice, SavedVoice, VoiceQuality};
use process_wrap::tokio::{ChildWrapper, CommandWrap, KillOnDrop};
use std::{
    fmt,
    path::{Path, PathBuf},
    process::Stdio,
    sync::{
        Arc, RwLock,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    process::{ChildStdin, ChildStdout, Command},
    sync::{Mutex, Semaphore},
};

/// The exact public assets used to create and reuse generated identities.
pub const DESIGN_MODEL_REVISION: &str = "32279effe8c19989596f05d353d1447f51d9e915";
const DEADLINE: Duration = Duration::from_secs(180);
const GPU_DEADLINE: Duration = Duration::from_secs(90);
const ASSETS: &str = include_str!("description_assets.json");
const WORKER: &str = include_str!("description_worker.py");

/// Lazy local description engine. Construction neither downloads nor starts a process.
#[derive(Clone)]
pub struct DescriptionSpeech {
    shared: Arc<Shared>,
    design: Option<DesignedVoice>,
    quality: VoiceQuality,
}
struct Shared {
    root: PathBuf,
    worker: Arc<Mutex<Option<Worker>>>,
    admission: Arc<Semaphore>,
    cpu_only: AtomicBool,
    execution: RwLock<SpeechExecution>,
}
impl fmt::Debug for DescriptionSpeech {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DescriptionSpeech")
            .field("has_reference", &self.design.is_some())
            .finish_non_exhaustive()
    }
}

impl DescriptionSpeech {
    fn python(root: &Path) -> PathBuf {
        if cfg!(windows) {
            root.join("venv").join("Scripts").join("python.exe")
        } else {
            root.join("venv").join("bin").join("python")
        }
    }

    fn setup_error(root: &Path) -> Option<&'static str> {
        if !Self::python(root).is_file() {
            return Some(
                "The local voice designer cannot find its Python runtime. Run scripts/install-voice-designer.ps1, check runtime_dir in config.toml, then restart JARVIS.",
            );
        }
        let assets: serde_json::Value = serde_json::from_str(ASSETS).ok()?;
        if assets["files"].as_array()?.iter().any(|asset| {
            let path = root
                .join("model")
                .join(asset["name"].as_str().unwrap_or_default());
            !path
                .metadata()
                .is_ok_and(|m| m.is_file() && Some(m.len()) == asset["size"].as_u64())
        }) {
            return Some(
                "The local voice designer model is missing or incomplete. Run scripts/install-voice-designer.ps1 for the configured runtime_dir, then retry.",
            );
        }
        if cfg!(windows)
            && !root
                .join("venv")
                .join("Lib")
                .join("site-packages")
                .join("_soundfile_data")
                .join("libsndfile_x64.dll")
                .is_file()
        {
            return Some(
                "The local voice designer audio decoder is missing. Run scripts/install-voice-designer.ps1 to repair the Windows runtime, then restart JARVIS.",
            );
        }
        None
    }
    /// Use an explicitly installed private inference environment.
    ///
    /// # Errors
    /// Refuses relative roots. Missing assets are reported on use, without breaking other voices.
    pub fn new(root: &Path) -> Result<Self, SpeechError> {
        if !root.is_absolute() {
            return Err(SpeechError::Settings(
                "the local voice designer requires an absolute runtime directory",
            ));
        }
        let gpu =
            root.join("gpu").join("enabled").is_file() && Self::python(&root.join("gpu")).is_file();
        Ok(Self {
            shared: Arc::new(Shared {
                root: root.to_owned(),
                worker: Arc::new(Mutex::new(None)),
                admission: Arc::new(Semaphore::new(3)),
                cpu_only: AtomicBool::new(!gpu),
                execution: RwLock::new(SpeechExecution {
                    phase: "cold",
                    backend: if gpu { "auto" } else { "cpu" }.into(),
                    device: if gpu { "GPU detection pending" } else { "CPU" }.into(),
                    allocated_bytes: 0,
                    peak_bytes: 0,
                    fallback_reason: if gpu {
                        None
                    } else {
                        Some("No enabled GPU runtime. Using CPU.")
                    },
                }),
            }),
            design: None,
            quality: VoiceQuality::default(),
        })
    }

    async fn retry_on_cpu<T, C, F>(
        &self,
        gpu: bool,
        first: impl std::future::Future<Output = Result<T, SpeechError>>,
        cpu: C,
    ) -> Result<T, SpeechError>
    where
        C: FnOnce() -> F,
        F: std::future::Future<Output = Result<T, SpeechError>>,
    {
        let result = if gpu {
            tokio::time::timeout(GPU_DEADLINE, first)
                .await
                .unwrap_or(Err(SpeechError::Unreachable))
        } else {
            first.await
        };
        match result {
            Ok(value) => Ok(value),
            Err(_) if gpu => {
                // The failed attempt has dropped its process tree. Cancellation drops this
                // entire future and therefore cannot start the CPU retry.
                self.shared.cpu_only.store(true, Ordering::Relaxed);
                *self
                    .shared
                    .execution
                    .write()
                    .unwrap_or_else(std::sync::PoisonError::into_inner) = SpeechExecution {
                    phase: "checking",
                    backend: "cpu".into(),
                    device: "CPU".into(),
                    allocated_bytes: 0,
                    peak_bytes: 0,
                    fallback_reason: Some(
                        "GPU unavailable or generation failed. Using CPU until JARVIS restarts.",
                    ),
                };
                cpu().await
            }
            Err(error) => Err(error),
        }
    }

    fn observe(&self, info: &WorkerInfo) {
        let mut status = self
            .shared
            .execution
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        status.backend.clone_from(&info.backend);
        status.device.clone_from(&info.device);
        status.allocated_bytes = info.allocated_bytes;
        status.peak_bytes = info.peak_bytes;
    }
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct WorkerInfo {
    backend: String,
    device: String,
    allocated_bytes: u64,
    peak_bytes: u64,
}
impl WorkerInfo {
    fn parse(bytes: &[u8], gpu: bool) -> Result<Self, SpeechError> {
        let info: Self = serde_json::from_slice(bytes).map_err(|_| SpeechError::BadAudio)?;
        if !matches!(
            (gpu, info.backend.as_str()),
            (true, "amd" | "nvidia") | (false, "cpu")
        ) || info.device.is_empty()
            || info.device.len() > 160
            || info.device.chars().any(char::is_control)
            || info.peak_bytes < info.allocated_bytes
            || info.peak_bytes > (1_u64 << 50)
            || (!gpu && (info.device != "CPU" || info.peak_bytes != 0))
            || (gpu && info.allocated_bytes == 0)
        {
            return Err(SpeechError::BadAudio);
        }
        Ok(info)
    }
}

struct Worker {
    child: Box<dyn ChildWrapper>,
    input: ChildStdin,
    output: ChildStdout,
    info: WorkerInfo,
    gpu: bool,
}
impl Worker {
    async fn start(root: &Path, gpu: bool, shared: &Shared) -> Result<Self, SpeechError> {
        let python = DescriptionSpeech::python(&if gpu {
            root.join("gpu")
        } else {
            root.to_owned()
        });
        let model = root.join("model");
        if let Some(reason) = DescriptionSpeech::setup_error(root) {
            return Err(SpeechError::Settings(reason));
        }
        let mut command = Command::new(python);
        command
            .args(["-I", "-u", "-c", WORKER])
            .arg(&model)
            .arg(ASSETS)
            .arg(if gpu { "gpu" } else { "cpu" })
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .env("HF_HUB_OFFLINE", "1")
            .env("TRANSFORMERS_OFFLINE", "1");
        let mut wrapped = CommandWrap::from(command);
        wrapped.wrap(KillOnDrop);
        #[cfg(unix)]
        wrapped.wrap(process_wrap::tokio::ProcessGroup::leader());
        #[cfg(windows)]
        {
            wrapped.wrap(process_wrap::tokio::CreationFlags(
                windows::Win32::System::Threading::CREATE_NO_WINDOW,
            ));
            wrapped.wrap(process_wrap::tokio::JobObject);
        }
        let mut child = wrapped.spawn().map_err(|_| SpeechError::Unreachable)?;
        let input = child.stdin().take().ok_or(SpeechError::Unreachable)?;
        let output = child.stdout().take().ok_or(SpeechError::Unreachable)?;
        let mut worker = Self {
            child,
            input,
            output,
            info: WorkerInfo {
                backend: String::new(),
                device: String::new(),
                allocated_bytes: 0,
                peak_bytes: 0,
            },
            gpu,
        };
        let mut ready = [0; 4];
        worker.output.read_exact(&mut ready).await.map_err(|_| SpeechError::Settings("the local voice designer could not load its verified model; check the installed runtime"))?;
        if &ready != b"JVD3" {
            return Err(SpeechError::BadAudio);
        }
        worker.boot(shared).await?;
        Ok(worker)
    }
}
impl Drop for Worker {
    fn drop(&mut self) {
        let _ = self.child.start_kill();
    }
}

pub(crate) fn validate_design(design: &DesignedVoice) -> Result<(), SpeechError> {
    design.validate().map_err(SpeechError::Settings)?;
    if design.model_revision != DESIGN_MODEL_REVISION {
        return Err(SpeechError::Settings(
            "the generated voice uses an unsupported model revision",
        ));
    }
    let bytes = STANDARD
        .decode(&design.reference_audio)
        .map_err(|_| SpeechError::BadAudio)?;
    validate_wav(&bytes, 10)
}

fn validate_wav(bytes: &[u8], seconds: usize) -> Result<(), SpeechError> {
    let u32_at = |start| {
        bytes
            .get(start..start + 4)
            .and_then(|b| <[u8; 4]>::try_from(b).ok())
            .map(u32::from_le_bytes)
    };
    let expected_size = u32::try_from(bytes.len().saturating_sub(8)).ok();
    let expected_data = u32::try_from(bytes.len().saturating_sub(44)).ok();
    if bytes.len() <= 44
        || bytes.len() > 44 + 96000 * seconds
        || !(bytes.len() - 44).is_multiple_of(2)
        || bytes.get(..4) != Some(b"RIFF")
        || bytes.get(8..16) != Some(b"WAVEfmt ")
        || u32_at(4) != expected_size
        || u32_at(16) != Some(16)
        || bytes.get(20..24) != Some(&[1, 0, 1, 0])
        || u32_at(24) != Some(48000)
        || u32_at(28) != Some(96000)
        || bytes.get(32..36) != Some(&[2, 0, 16, 0])
        || bytes.get(36..40) != Some(b"data")
        || u32_at(40) != expected_data
        || !bytes[44..].iter().any(|byte| *byte != 0)
    {
        return Err(SpeechError::Settings(
            "the voice sample must be non-silent 48 kHz mono PCM WAV within its duration limit; shorten the sample text",
        ));
    }
    Ok(())
}

impl SpeechProvider for DescriptionSpeech {
    fn execution(&self) -> Option<SpeechExecution> {
        Some(
            self.shared
                .execution
                .read()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .clone(),
        )
    }
    fn design_unavailable_reason(&self) -> Option<&'static str> {
        Self::setup_error(&self.shared.root)
    }
    fn synthesis_timeout(&self) -> Duration {
        DEADLINE
    }
    fn describe(&self) -> SpeechDescription {
        SpeechDescription {
            id: "local-voxcpm2".into(),
            provider: "local-voxcpm2".into(),
            voice: "designed".into(),
            model: "voxcpm2".into(),
        }
    }
    fn configured(&self, voice: &SavedVoice) -> Result<Arc<dyn SpeechProvider>, SpeechError> {
        crate::validate_voice("local-voxcpm2", voice)?;
        Ok(Arc::new(Self {
            shared: Arc::clone(&self.shared),
            design: voice.design.clone(),
            quality: voice.settings.quality.unwrap_or_default(),
        }))
    }
    fn prepare(
        &self,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), SpeechError>> + Send + '_>>
    {
        Box::pin(self.warm())
    }
    fn design_stream<'a>(
        &'a self,
        description: &'a str,
        sample: &'a str,
        seed: u32,
        quality: VoiceQuality,
    ) -> crate::DesignFuture<'a> {
        Box::pin(self.stream_design(description, sample, seed, quality))
    }
    fn design<'a>(
        &'a self,
        description: &'a str,
        sample: &'a str,
        seed: u32,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<DesignedVoice, SpeechError>> + Send + 'a>,
    > {
        Box::pin(async move {
            use futures_util::StreamExt;
            let mut stream = self
                .stream_design(description, sample, seed, VoiceQuality::default())
                .await?;
            while let Some(event) = stream.next().await {
                if let crate::DesignEvent::Complete(design) = event? {
                    return Ok(design);
                }
            }
            Err(SpeechError::BadAudio)
        })
    }
    fn open<'a>(&'a self, text: &'a str, _previous: Option<&'a str>) -> SpeechFuture<'a> {
        Box::pin(async move {
            let design = self.design.as_ref().ok_or(SpeechError::Settings(
                "generate and save a local voice first",
            ))?;
            self.render_stream(
                text,
                None,
                Some(&design.reference_audio),
                design.seed,
                self.quality,
            )
            .await
        })
    }
}

#[cfg(test)]
mod tests;

mod streaming;
