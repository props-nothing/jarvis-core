//! Installation-wide exclusion and fail-closed update journals.

use crate::{
    BackupError, PathKind,
    backup::{read_record, regular_directory, regular_file, valid_digest, write_record},
    paths::prepare_private_directory,
};
use jarvis_core::BinaryUpdate;
use std::{
    fs::{self, File, OpenOptions},
    path::{Path, PathBuf},
};

/// Private installation state; separate from any one profile.
pub const UPDATE_DIRECTORY: &str = ".jarvis-update";

/// Held for a daemon's lifetime, or exclusively for binary maintenance.
pub struct InstallationGuard {
    file: File,
    directory: PathBuf,
    exclusive: bool,
}

impl InstallationGuard {
    /// Opens a regular per-installation lock. Shared owners also reject pending journals.
    ///
    /// # Errors
    /// Refuses unsafe paths, contention, I/O failure or unresolved update evidence.
    pub fn acquire(directory: &Path, exclusive: bool) -> Result<Self, BackupError> {
        regular_directory(directory)?;
        let directory = fs::canonicalize(directory)?;
        let state = directory.join(UPDATE_DIRECTORY);
        prepare_private_directory(PathKind::State, &state)?;
        let lock = state.join("lock");
        if fs::symlink_metadata(&lock).is_ok() {
            regular_file(&lock)?;
        }
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&lock)?;
        let result = if exclusive {
            file.try_lock()
        } else {
            file.try_lock_shared()
        };
        result.map_err(|_| BackupError::InUse)?;
        crate::secure_private_file(PathKind::State, &lock)?;
        let guard = Self {
            file,
            directory,
            exclusive,
        };
        if !exclusive {
            require_settled_binary_update(&guard.directory)?;
        }
        Ok(guard)
    }

    /// Writes a transition while holding exclusive installation ownership.
    ///
    /// # Errors
    /// Refuses invalid evidence or failed durable writes.
    pub fn write_update(&self, record: &BinaryUpdate) -> Result<(), BackupError> {
        if !self.exclusive {
            return Err(BackupError::InUse);
        }
        validate(record)?;
        write_record(
            &self.directory.join(UPDATE_DIRECTORY).join("journal.json"),
            record,
        )
    }
}

impl Drop for InstallationGuard {
    fn drop(&mut self) {
        let _ = self.file.unlock();
    }
}

fn validate(record: &BinaryUpdate) -> Result<(), BackupError> {
    if record.format != 1
        || !record.profile_data.is_absolute()
        || !valid_digest(&record.database_sha256)
        || record.database_bytes == 0
        || !record
            .previous
            .compatibility
            .permits(&record.candidate.compatibility)
        || [&record.previous, &record.candidate].iter().any(|image| {
            !valid_digest(&image.sha256)
                || image.bytes == 0
                || image.bytes > 268_435_456
                || image.version.len() > 64
        })
    {
        return Err(BackupError::InvalidRecord);
    }
    Ok(())
}

/// Observes the latest installation record without creating files.
///
/// # Errors
/// Invalid or unreadable evidence fails closed.
pub fn binary_update_status(directory: &Path) -> Result<Option<BinaryUpdate>, BackupError> {
    let state = directory.join(UPDATE_DIRECTORY);
    match fs::symlink_metadata(&state) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
        Ok(_) => regular_directory(&state)?,
    }
    let path = state.join("journal.json");
    match fs::symlink_metadata(&path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
        Ok(_) => {}
    }
    let record = read_record(&path)?;
    validate(&record)?;
    Ok(Some(record))
}

/// Prevents daemon startup after an interrupted activation/rollback.
///
/// # Errors
/// Pending, unreadable or malformed evidence must be explicitly recovered.
pub fn require_settled_binary_update(directory: &Path) -> Result<(), BackupError> {
    if let Some(record) = binary_update_status(directory)? {
        if record.state.is_pending() {
            return Err(BackupError::Ambiguous);
        }
        let expected = if record.state == jarvis_core::BinaryUpdateState::Applied {
            record.candidate
        } else {
            record.previous
        };
        let binary = directory.join(if cfg!(windows) {
            "jarvis.exe"
        } else {
            "jarvis"
        });
        if crate::backup::fingerprint(&binary)? != (expected.bytes, expected.sha256) {
            return Err(BackupError::DigestMismatch);
        }
    }
    Ok(())
}
