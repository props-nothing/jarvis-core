//! Atomic profile-local voice-library persistence. Callers serialize save/publication.

use std::{fs, io::Read, path::PathBuf};

use jarvis_core::VoiceLibrary;

use crate::{AppPaths, secure_private_file};

/// Durable voice settings, separate from private provider credentials in config.toml.
#[derive(Clone, Debug)]
pub struct VoiceLibraryStore {
    path: PathBuf,
}

impl VoiceLibraryStore {
    /// Locate this profile's voice library.
    #[must_use]
    pub fn new(paths: &AppPaths) -> Self {
        Self {
            path: paths.config().join("voice-library.json"),
        }
    }

    /// Load a bounded, structurally valid library. Only a missing file means uninitialized.
    ///
    /// # Errors
    /// Unreadable, oversized, corrupt and unsupported files fail closed.
    pub fn load(&self) -> Result<Option<VoiceLibrary>, &'static str> {
        crate::config::reject_unsafe_write_endpoint(&self.path)
            .map_err(|_| "the voice library must be a regular file")?;
        let file = match fs::File::open(&self.path) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(_) => return Err("the voice library could not be read"),
        };
        let mut bytes = Vec::new();
        file.take(8 * 1024 * 1024 + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| "the voice library could not be read")?;
        if bytes.len() > 8 * 1024 * 1024 {
            return Err("the voice library is too large");
        }
        let library: VoiceLibrary = serde_json::from_slice(&bytes)
            .map_err(|_| "the voice library is malformed or has unsupported fields")?;
        library.validate()?;
        Ok(Some(library))
    }

    /// Persist a fully validated snapshot. The daemon holds its mutation lock through publication.
    ///
    /// # Errors
    /// Returns a redacted failure without changing an existing document on validation/write failure.
    pub fn save(&self, library: &VoiceLibrary) -> Result<(), &'static str> {
        library.validate()?;
        let bytes = serde_json::to_vec_pretty(library)
            .map_err(|_| "the voice library could not be encoded")?;
        if bytes.len() > 8 * 1024 * 1024 {
            return Err("the voice library is too large");
        }
        crate::config::reject_unsafe_write_endpoint(&self.path)
            .map_err(|_| "the voice library must be a regular file")?;
        let file = crate::config::stage_document(&self.path, &bytes)
            .map_err(|_| "the voice library could not be saved")?;
        file.commit()
            .map_err(|_| "the voice library could not be saved")?;
        secure_private_file(crate::PathKind::Config, &self.path)
            .map_err(|_| "the voice library permissions could not be secured")?;
        Ok(())
    }
}
