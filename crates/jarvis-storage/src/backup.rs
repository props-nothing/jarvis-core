//! Offline database recovery. Never opens the canonical database for migration.

use std::{
    fs::{self, File, OpenOptions, TryLockError},
    io::{self, Read, Seek, Write},
    path::{Path, PathBuf},
};

use atomic_write_file::AtomicWriteFile;
use jarvis_core::{BackupId, DAEMON_LOCK_FILE_NAME};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::{
    AppPaths, CURRENT_SCHEMA_VERSION, DEFAULT_DATABASE_FILENAME, DatabaseError, DatabaseState,
    PathError, PathKind,
    inspect::inspect_standalone_database,
    paths::{prepare_private_directory, secure_private_file},
};

const FORMAT: u32 = 1;
const MAX_RECORD_BYTES: u64 = 4096;
const JOURNAL: &str = "database-restore.json";

/// A refused or failed offline recovery operation. Messages never include database content.
#[derive(Debug, Error)]
pub enum BackupError {
    /// A cooperating daemon or maintenance operation holds the profile.
    #[error("the profile is in use; run `jarvis stop` before database maintenance")]
    InUse,
    /// The state is incomplete and must be explicitly reconciled.
    #[error("a database restore is pending; run `jarvis backup recover` for this profile")]
    Pending,
    /// Recorded and observed state cannot be safely reconciled.
    #[error(
        "database recovery evidence is ambiguous; preserve the database, snapshots and journal"
    )]
    Ambiguous,
    /// A journal/manifest has an unsupported format or invalid evidence.
    #[error("invalid database backup metadata")]
    InvalidRecord,
    /// A selected file or directory is not a regular managed endpoint.
    #[error(
        "database backup paths must be absolute regular files/directories, without symbolic links"
    )]
    InvalidPath,
    /// Sidecars may contain committed data or indicate another SQLite connection.
    #[error(
        "SQLite WAL/SHM/journal sidecars exist; close database users cleanly, never delete sidecars to bypass this check"
    )]
    Sidecars,
    /// A missing, corrupt, foreign or incompatible schema must never be copied/restored.
    #[error(
        "offline recovery requires a healthy current JARVIS database schema; no automatic migration or downgrade is allowed"
    )]
    Incompatible,
    /// The independently measured snapshot does not match its metadata.
    #[error("database snapshot size or SHA-256 mismatch")]
    DigestMismatch,
    /// An operating-system write/read failed.
    #[error("database maintenance I/O failed: {0}")]
    Io(#[from] io::Error),
    /// Private directory/file preparation failed.
    #[error(transparent)]
    Path(#[from] PathError),
    /// Read-only inspection failed.
    #[error(transparent)]
    Database(#[from] DatabaseError),
}

/// Retained, independently verified database-only snapshot metadata.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DatabaseBackup {
    /// Version of this local manifest format.
    pub format: u32,
    /// Opaque snapshot directory identity; never an arbitrary path.
    pub id: BackupId,
    /// SQLite schema validated at snapshot creation.
    pub schema: i64,
    /// Verified file byte length.
    pub bytes: u64,
    /// Lowercase SHA-256 of the complete standalone SQLite image.
    pub sha256: String,
}

/// Result of explicit restore or interrupted-restore recovery.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RestoreState {
    /// The replacement outcome has not yet been recorded.
    Pending,
    /// The selected snapshot was installed and validated.
    Applied,
    /// The previous database is intact; replacement did not occur.
    NotApplied,
}

/// Latest durable restore record. Prior contents remain in the retained snapshot.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DatabaseRestore {
    /// Journal format version.
    pub format: u32,
    /// Selected snapshot.
    pub selected: BackupId,
    /// Verified snapshot made immediately before replacement, available for undo.
    pub previous: BackupId,
    /// Digest of the database before replacement.
    pub before_sha256: String,
    /// Digest of the requested replacement.
    pub after_sha256: String,
    /// Current outcome; pending forbids daemon startup.
    pub state: RestoreState,
}

/// Holds the exact OS lock used by daemon startup, without rewriting its metadata.
struct MaintenanceGuard(File);

impl MaintenanceGuard {
    fn acquire(paths: &AppPaths) -> Result<Self, BackupError> {
        paths.prepare()?;
        let path = paths.runtime().join(DAEMON_LOCK_FILE_NAME);
        if path.try_exists()? {
            regular_file(&path)?;
        } else if fs::symlink_metadata(&path).is_ok() {
            return Err(BackupError::InvalidPath);
        }
        let mut options = OpenOptions::new();
        options.read(true).write(true).create(true).truncate(false);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let file = options.open(&path)?;
        match file.try_lock() {
            Ok(()) => {}
            Err(TryLockError::WouldBlock) => return Err(BackupError::InUse),
            Err(TryLockError::Error(error)) => return Err(error.into()),
        }
        secure_private_file(PathKind::Runtime, &path)?;
        Ok(Self(file))
    }
}

impl Drop for MaintenanceGuard {
    fn drop(&mut self) {
        let _ = self.0.unlock();
    }
}

fn database_path(paths: &AppPaths) -> PathBuf {
    paths.data().join(DEFAULT_DATABASE_FILENAME)
}

fn backup_directory(paths: &AppPaths, id: BackupId) -> Result<PathBuf, BackupError> {
    let parent = paths.data().join("backups");
    regular_directory(&parent)?;
    let directory = parent.join(id.to_string());
    regular_directory(&directory)?;
    Ok(directory)
}

fn regular_directory(path: &Path) -> Result<(), BackupError> {
    let metadata = fs::symlink_metadata(path)?;
    if !path.is_absolute() || metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(BackupError::InvalidPath);
    }
    Ok(())
}

fn regular_file(path: &Path) -> Result<(), BackupError> {
    let metadata = fs::symlink_metadata(path)?;
    if !path.is_absolute() || metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(BackupError::InvalidPath);
    }
    Ok(())
}

fn no_sidecars(path: &Path) -> Result<(), BackupError> {
    for suffix in ["-wal", "-shm", "-journal"] {
        let mut name = path.as_os_str().to_os_string();
        name.push(suffix);
        match fs::symlink_metadata(PathBuf::from(name)) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
            Ok(_) => return Err(BackupError::Sidecars),
        }
    }
    Ok(())
}

async fn healthy(path: &Path) -> Result<(), BackupError> {
    regular_file(path)?;
    no_sidecars(path)?;
    let inspection = inspect_standalone_database(path).await?;
    if !matches!(inspection.state, DatabaseState::Current { version } if version == CURRENT_SCHEMA_VERSION)
        || !inspection.integrity_ok
        || inspection.foreign_key_violations
    {
        return Err(BackupError::Incompatible);
    }
    no_sidecars(path)?;
    Ok(())
}

fn fingerprint(path: &Path) -> Result<(u64, String), BackupError> {
    regular_file(path)?;
    fingerprint_reader(&mut File::open(path)?)
}

fn fingerprint_reader(input: &mut impl Read) -> Result<(u64, String), BackupError> {
    let mut digest = Sha256::new();
    let mut bytes = 0;
    let mut buffer = vec![0_u8; 65536];
    loop {
        let count = input.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        bytes += count as u64;
        digest.update(&buffer[..count]);
    }
    Ok((bytes, format!("{:x}", digest.finalize())))
}

fn valid_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn read_record<T: DeserializeOwned>(path: &Path) -> Result<T, BackupError> {
    regular_file(path)?;
    let mut bytes = Vec::new();
    File::open(path)?
        .take(MAX_RECORD_BYTES + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_RECORD_BYTES {
        return Err(BackupError::InvalidRecord);
    }
    serde_json::from_slice(&bytes).map_err(|_| BackupError::InvalidRecord)
}

fn atomic_file(path: &Path) -> Result<AtomicWriteFile, BackupError> {
    let mut options = AtomicWriteFile::options();
    options.read(true);
    #[cfg(unix)]
    {
        use atomic_write_file::unix::OpenOptionsExt;
        std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
        options.preserve_mode(false);
    }
    Ok(options.open(path)?)
}

fn write_record(path: &Path, record: &impl Serialize) -> Result<(), BackupError> {
    let bytes = serde_json::to_vec(record).map_err(|_| BackupError::InvalidRecord)?;
    if bytes.len() as u64 > MAX_RECORD_BYTES {
        return Err(BackupError::InvalidRecord);
    }
    let mut file = atomic_file(path)?;
    file.write_all(&bytes)?;
    file.sync_all()?;
    file.commit()?;
    secure_private_file(PathKind::Data, path)?;
    Ok(())
}

fn journal(paths: &AppPaths) -> Result<Option<DatabaseRestore>, BackupError> {
    let path = paths.state().join(JOURNAL);
    match fs::symlink_metadata(&path) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
        Ok(_) => {}
    }
    let record: DatabaseRestore = read_record(&path)?;
    if record.format != FORMAT
        || !valid_digest(&record.before_sha256)
        || !valid_digest(&record.after_sha256)
    {
        return Err(BackupError::InvalidRecord);
    }
    Ok(Some(record))
}

/// Reads the latest restore outcome without opening SQLite or changing the profile.
/// This is observational; callers coordinating mutations must hold the profile lock.
///
/// # Errors
/// Refuses unreadable, linked or malformed journal evidence.
pub fn database_restore_status(paths: &AppPaths) -> Result<Option<DatabaseRestore>, BackupError> {
    journal(paths)
}

/// Refuses an unresolved restore before the daemon opens or migrates SQLite.
///
/// # Errors
/// Returns an error for pending, malformed or unreadable recovery state. The caller
/// must hold the daemon profile lock through this check and database opening.
pub fn require_settled_database_restore(paths: &AppPaths) -> Result<(), BackupError> {
    if journal(paths)?.is_some_and(|record| record.state == RestoreState::Pending) {
        return Err(BackupError::Pending);
    }
    Ok(())
}

/// Verifies a retained snapshot without opening the canonical database.
///
/// # Errors
/// Refuses missing/linked files, invalid metadata, byte tampering or incompatible SQLite.
pub async fn verify_database_backup(
    paths: &AppPaths,
    id: BackupId,
) -> Result<DatabaseBackup, BackupError> {
    let directory = backup_directory(paths, id)?;
    let record: DatabaseBackup = read_record(&directory.join("manifest.json"))?;
    if record.format != FORMAT
        || record.id != id
        || record.schema != CURRENT_SCHEMA_VERSION
        || record.bytes == 0
        || !valid_digest(&record.sha256)
    {
        return Err(BackupError::InvalidRecord);
    }
    let file = directory.join(DEFAULT_DATABASE_FILENAME);
    if fingerprint(&file)? != (record.bytes, record.sha256.clone()) {
        return Err(BackupError::DigestMismatch);
    }
    healthy(&file).await?;
    Ok(record)
}

async fn snapshot_locked(paths: &AppPaths) -> Result<DatabaseBackup, BackupError> {
    let source = database_path(paths);
    healthy(&source).await?;
    let (bytes, sha256) = fingerprint(&source)?;
    let id = BackupId::new();
    let parent = paths.data().join("backups");
    prepare_private_directory(PathKind::Data, &parent)?;
    let directory = parent.join(id.to_string());
    fs::create_dir(&directory)?;
    prepare_private_directory(PathKind::Data, &directory)?;
    let target = directory.join(DEFAULT_DATABASE_FILENAME);
    let mut output = File::create_new(&target)?;
    secure_private_file(PathKind::Data, &target)?;
    io::copy(&mut File::open(&source)?, &mut output)?;
    output.sync_all()?;
    drop(output);
    no_sidecars(&source)?;
    let record = DatabaseBackup {
        format: FORMAT,
        id,
        schema: CURRENT_SCHEMA_VERSION,
        bytes,
        sha256,
    };
    if fingerprint(&target)? != (record.bytes, record.sha256.clone()) {
        return Err(BackupError::DigestMismatch);
    }
    healthy(&target).await?;
    write_record(&directory.join("manifest.json"), &record)?;
    verify_database_backup(paths, id).await
}

/// Creates a verified snapshot while exclusively owning a stopped profile.
///
/// # Errors
/// Refuses active/pending profiles, sidecars, incompatible databases or storage failures.
pub async fn create_database_backup(paths: &AppPaths) -> Result<DatabaseBackup, BackupError> {
    let _guard = MaintenanceGuard::acquire(paths)?;
    require_settled_database_restore(paths)?;
    snapshot_locked(paths).await
}

/// Restores a verified current-schema snapshot, retaining the prior database for undo.
///
/// # Errors
/// Refuses busy/pending profiles or invalid snapshots. A failure after the pending
/// journal is written requires explicit recovery before daemon startup.
pub async fn restore_database_backup(
    paths: &AppPaths,
    selected: BackupId,
) -> Result<DatabaseRestore, BackupError> {
    let _guard = MaintenanceGuard::acquire(paths)?;
    require_settled_database_restore(paths)?;
    let selected_record = verify_database_backup(paths, selected).await?;
    let previous = snapshot_locked(paths).await?;
    let target = database_path(paths);
    let source = backup_directory(paths, selected)?.join(DEFAULT_DATABASE_FILENAME);
    let mut staging = atomic_file(&target)?;
    io::copy(&mut File::open(&source)?, &mut staging)?;
    staging.sync_all()?;
    staging.rewind()?;
    if fingerprint_reader(staging.as_file_mut())?
        != (selected_record.bytes, selected_record.sha256.clone())
    {
        return Err(BackupError::DigestMismatch);
    }
    // Recheck selected bytes and the canonical pre-image before admitting replacement.
    verify_database_backup(paths, selected).await?;
    no_sidecars(&target)?;
    if fingerprint(&target)?.1 != previous.sha256 {
        return Err(BackupError::Ambiguous);
    }
    let mut record = DatabaseRestore {
        format: FORMAT,
        selected,
        previous: previous.id,
        before_sha256: previous.sha256,
        after_sha256: selected_record.sha256,
        state: RestoreState::Pending,
    };
    write_record(&paths.state().join(JOURNAL), &record)?;
    #[cfg(test)]
    tests::crash_at("pending");
    staging.commit()?;
    #[cfg(test)]
    tests::crash_at("replacement");
    secure_private_file(PathKind::Data, &target)?;
    healthy(&target).await?;
    if fingerprint(&target)?.1 != record.after_sha256 {
        return Err(BackupError::Ambiguous);
    }
    record.state = RestoreState::Applied;
    write_record(&paths.state().join(JOURNAL), &record)?;
    Ok(record)
}

/// Reconciles an interrupted restore using verified before/after bytes, without replacing data.
///
/// # Errors
/// Refuses active profiles, invalid evidence or a database matching neither recorded image.
pub async fn recover_database_restore(
    paths: &AppPaths,
) -> Result<Option<DatabaseRestore>, BackupError> {
    let _guard = MaintenanceGuard::acquire(paths)?;
    let Some(mut record) = journal(paths)? else {
        return Ok(None);
    };
    if record.state != RestoreState::Pending {
        return Ok(Some(record));
    }
    let before = verify_database_backup(paths, record.previous).await?;
    let after = verify_database_backup(paths, record.selected).await?;
    if before.sha256 != record.before_sha256 || after.sha256 != record.after_sha256 {
        return Err(BackupError::InvalidRecord);
    }
    let target = database_path(paths);
    healthy(&target).await?;
    let actual = fingerprint(&target)?.1;
    record.state = if actual == record.after_sha256 {
        RestoreState::Applied
    } else if actual == record.before_sha256 {
        RestoreState::NotApplied
    } else {
        return Err(BackupError::Ambiguous);
    };
    write_record(&paths.state().join(JOURNAL), &record)?;
    Ok(Some(record))
}

#[cfg(test)]
#[path = "backup_tests.rs"]
mod tests;
