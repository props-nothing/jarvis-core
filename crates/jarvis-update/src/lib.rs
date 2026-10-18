//! Signed staging and recoverable offline binary activation (ADR-0242).

mod files;
mod manifest;
mod release;

use jarvis_core::{BinaryImage, BinaryUpdate, BinaryUpdateState, UpdateCompatibility, UpdateId};
use jarvis_storage::{AppPaths, InstallationGuard, OfflineProfile, PathMode, UPDATE_DIRECTORY};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
};
use tokio::process::Command;

/// A maintenance operation refused before effects, or failed with retained evidence.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// A deterministic safety/preflight check refused the operation.
    #[error("{0}")]
    Refused(&'static str),
    /// Filesystem/process error.
    #[error("binary maintenance I/O failed: {0}")]
    Io(#[from] std::io::Error),
    /// Public release transport failed; request URLs never contain credentials.
    #[error("public release download failed: {0}")]
    Http(#[from] reqwest::Error),
    /// Local metadata was malformed.
    #[error("invalid binary maintenance metadata: {0}")]
    Json(#[from] serde_json::Error),
    /// Profile/installation ownership or database verification failed.
    #[error(transparent)]
    Storage(#[from] jarvis_storage::BackupError),
}

type Result<T> = std::result::Result<T, Error>;

/// Exact binary filename on the current platform.
#[must_use]
pub const fn program_name() -> &'static str {
    if cfg!(windows) {
        "jarvis.exe"
    } else {
        "jarvis"
    }
}

/// Read-only maintenance protocol reply. Does not initialize a profile.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Probe {
    /// Package version of this build.
    pub version: String,
    /// Database/configuration/protocol compatibility contract.
    pub compatibility: UpdateCompatibility,
}

/// The contract advertised by this build.
#[must_use]
pub fn probe() -> Probe {
    Probe {
        version: env!("CARGO_PKG_VERSION").to_owned(),
        compatibility: UpdateCompatibility {
            maintenance: 1,
            database: jarvis_storage::CURRENT_SCHEMA_VERSION,
            configuration: jarvis_storage::CURRENT_CONFIG_VERSION,
            protocol: jarvis_protocol::PROTOCOL_VERSION,
        },
    }
}

/// A staged release identity, retained alongside original signed evidence.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct StagedRelease {
    /// Opaque stage identifier.
    pub id: UpdateId,
    /// Exact release tag selected by the owner.
    pub tag: String,
    /// Package version bound by the signed manifest.
    pub version: String,
    /// Executable digest measured after verified extraction.
    pub sha256: String,
    /// Executable byte length.
    pub bytes: u64,
}

fn directory(path: &Path) -> Result<PathBuf> {
    files::regular(path, true)?;
    Ok(fs::canonicalize(path)?)
}

fn stage_dir(install: &Path, id: UpdateId) -> Result<PathBuf> {
    let parent = install.join(UPDATE_DIRECTORY);
    files::regular(&parent, true)?;
    let stage = parent.join(id.to_string());
    files::regular(&stage, true)?;
    Ok(stage)
}

/// Path of the independent updater copied during staging.
#[must_use]
pub fn helper_path(install: &Path, id: UpdateId) -> PathBuf {
    install
        .join(UPDATE_DIRECTORY)
        .join(id.to_string())
        .join(if cfg!(windows) {
            "updater.exe"
        } else {
            "updater"
        })
}

/// Downloads and verifies one exact signed release without replacing the installation.
///
/// # Errors
/// Refuses invalid selectors, signatures, archives, unsafe paths and process failures.
pub async fn stage(install: &Path, tag: &str, version: &str) -> Result<StagedRelease> {
    manifest::selector(tag, version)?;
    let install = directory(install)?;
    files::regular(&install.join(program_name()), false)?;
    let _guard = InstallationGuard::acquire(&install, false)?;
    let id = UpdateId::new();
    let stage = install.join(UPDATE_DIRECTORY).join(id.to_string());
    fs::create_dir(&stage)?;
    let selected = release::fetch(&stage, tag, version).await?;
    let candidate = stage.join(program_name());
    release::extract(&stage, version, &selected, &candidate)?;
    check_version(&candidate, version).await?;
    let (bytes, sha256) = files::fingerprint(&candidate)?;
    files::copy_new(&std::env::current_exe()?, &helper_path(&install, id))?;
    let record = StagedRelease {
        id,
        tag: tag.to_owned(),
        version: version.to_owned(),
        sha256,
        bytes,
    };
    files::write_json(&stage.join("stage.json"), &record)?;
    Ok(record)
}

async fn check_version(program: &Path, version: &str) -> Result<()> {
    let output = release::output(Command::new(program).arg("version"), 15).await?;
    if String::from_utf8_lossy(&output).trim_end() != format!("jarvis-cli {version}") {
        return Err(Error::Refused(
            "executable version differs from signed package",
        ));
    }
    Ok(())
}

async fn image(program: &Path) -> Result<BinaryImage> {
    let before = files::fingerprint(program)?;
    let output = release::output(Command::new(program).args(["update", "probe"]), 15)
        .await
        .map_err(|_| Error::Refused("executable maintenance protocol probe failed or timed out"))?;
    let contract: Probe = serde_json::from_slice(&output)?;
    if !manifest::version(&contract.version)
        || !probe().compatibility.permits(&contract.compatibility)
    {
        return Err(Error::Refused(
            "binary does not support this updater's schema and installation-lock protocol",
        ));
    }
    check_version(program, &contract.version).await?;
    if files::fingerprint(program)? != before {
        return Err(Error::Refused("executable changed during preflight"));
    }
    Ok(BinaryImage {
        bytes: before.0,
        sha256: before.1,
        version: contract.version,
        compatibility: contract.compatibility,
    })
}

fn identity(image: &BinaryImage) -> (u64, String) {
    (image.bytes, image.sha256.clone())
}

fn service_preflight(install: &Path, paths: &AppPaths) -> Result<()> {
    use jarvis_diagnostics::{ServiceKind, ServicePlan};
    let plan = ServicePlan::build(ServiceKind::native(), &install.join(program_name()), None)
        .map_err(|_| Error::Refused("cannot resolve service ownership"))?;
    service_plan_preflight(&plan, paths.mode())
}

fn service_plan_preflight(plan: &jarvis_diagnostics::ServicePlan, mode: PathMode) -> Result<()> {
    use jarvis_diagnostics::{ServiceDrift, detect_drift};
    let drift = detect_drift(plan);
    if matches!(drift, ServiceDrift::Absent)
        || (mode == PathMode::Native && matches!(drift, ServiceDrift::Current))
    {
        return Ok(());
    }
    // A different, owned service must not be adopted; even portable mode cannot
    // assume an installed service is unrelated without inspecting its contract.
    Err(Error::Refused(
        "service ownership/path is not an exact match; reconcile it explicitly before binary maintenance",
    ))
}

fn independent_updater(target: &Path) -> Result<()> {
    if fs::canonicalize(std::env::current_exe()?)? == fs::canonicalize(target)? {
        return Err(Error::Refused(
            "run activation/rollback/recovery with the retained updater path printed by update stage; the installed executable cannot replace itself on Windows",
        ));
    }
    Ok(())
}

async fn verified_candidate(install: &Path, id: UpdateId) -> Result<BinaryImage> {
    let stage = stage_dir(install, id)?;
    let record: StagedRelease =
        serde_json::from_slice(&files::bounded(&stage.join("stage.json"), 4096)?)?;
    if record.id != id {
        return Err(Error::Refused("staged identity mismatch"));
    }
    let selected = release::verify(&stage, &record.tag, &record.version).await?;
    let checked = stage.join(format!("checked-{}", jarvis_core::scratch_tag()));
    release::extract(&stage, &record.version, &selected, &checked)?;
    let expected = (record.bytes, record.sha256);
    let candidate = stage.join(program_name());
    if files::fingerprint(&checked)? != expected || files::fingerprint(&candidate)? != expected {
        return Err(Error::Refused(
            "staged executable differs from signed archive",
        ));
    }
    let image = image(&candidate).await?;
    if image.version != record.version {
        return Err(Error::Refused("candidate version mismatch"));
    }
    Ok(image)
}

/// Activates a signed candidate while holding installation and profile ownership.
///
/// # Errors
/// Refuses unsupported compatibility, service drift, active profiles or altered bytes.
/// A crash/storage failure after intent requires `update recover` using the helper.
pub async fn activate(install: &Path, paths: &AppPaths, id: UpdateId) -> Result<BinaryUpdate> {
    let install = directory(install)?;
    let target = install.join(program_name());
    independent_updater(&target)?;
    let guard = InstallationGuard::acquire(&install, true)?;
    jarvis_storage::require_settled_binary_update(&install)?;
    service_preflight(&install, paths)?;
    let offline = OfflineProfile::acquire(paths)?;
    if let Some(record) = jarvis_storage::binary_update_status(&install)?
        && record.id == id
    {
        if record.state == BinaryUpdateState::Applied
            && record.profile_data == fs::canonicalize(paths.data())?
            && image(&target).await? == record.candidate
        {
            offline.check().await?;
            return Ok(record);
        }
        return Err(Error::Refused(
            "this update already has a terminal outcome; stage a new release explicitly",
        ));
    }
    let candidate = verified_candidate(&install, id).await?;
    activate_prepared(&install, paths, id, &guard, &offline, candidate).await
}

async fn activate_prepared(
    install: &Path,
    paths: &AppPaths,
    id: UpdateId,
    guard: &InstallationGuard,
    offline: &OfflineProfile,
    candidate: BinaryImage,
) -> Result<BinaryUpdate> {
    let target = install.join(program_name());
    let previous = image(&target).await?;
    if !previous.compatibility.permits(&candidate.compatibility) {
        return Err(Error::Refused(
            "schema/protocol-changing activation is unsupported",
        ));
    }
    let stage = stage_dir(install, id)?;
    files::copy_new(&target, &stage.join("previous"))?;
    let backup = offline.snapshot().await?;
    if files::fingerprint(&target)? != identity(&previous) {
        return Err(Error::Refused("installation changed before activation"));
    }
    let mut record = BinaryUpdate {
        format: 1,
        id,
        profile_data: fs::canonicalize(paths.data())?,
        backup: backup.id,
        database_sha256: backup.sha256.clone(),
        database_bytes: backup.bytes,
        previous,
        candidate,
        state: BinaryUpdateState::Activating,
    };
    files::write_json(&stage.join("activation.json"), &record)?;
    guard.write_update(&record)?;
    let result = apply_image(
        &stage.join(program_name()),
        &target,
        &record.candidate,
        offline,
    )
    .await;
    if result.is_ok() && offline.check().await? != (backup.bytes, backup.sha256.clone()) {
        return Err(Error::Refused(
            "database changed during activation; pending evidence retained",
        ));
    }
    if result.is_err() {
        // Never guess after unexpected modification or a database change.
        let actual = files::fingerprint(&target)?;
        if (actual != identity(&record.previous) && actual != identity(&record.candidate))
            || offline.check().await? != (backup.bytes, backup.sha256)
        {
            return Err(Error::Refused(
                "activation failed with ambiguous evidence; preserve state and run update recover",
            ));
        }
        record.state = BinaryUpdateState::RollingBack;
        guard.write_update(&record)?;
        apply_image(&stage.join("previous"), &target, &record.previous, offline).await?;
        record.state = BinaryUpdateState::RolledBack;
        guard.write_update(&record)?;
        return Err(Error::Refused(
            "activation failed; previous executable restored, database unchanged",
        ));
    }
    record.state = BinaryUpdateState::Applied;
    guard.write_update(&record)?;
    Ok(record)
}

async fn apply_image(
    source: &Path,
    target: &Path,
    expected: &BinaryImage,
    offline: &OfflineProfile,
) -> Result<()> {
    files::replace(source, target, &identity(expected))?;
    if image(target).await? != *expected {
        return Err(Error::Refused(
            "activated executable health/identity check failed",
        ));
    }
    offline.check().await?;
    Ok(())
}

/// Restores the latest retained executable, preserving the current healthy database.
///
/// # Errors
/// Refuses unknown executable bytes, incompatible data, active owners or missing evidence.
pub async fn rollback(install: &Path, paths: &AppPaths) -> Result<BinaryUpdate> {
    settle(install, paths, true).await
}

/// Reconciles interrupted activation using recorded before/after executable evidence.
///
/// # Errors
/// Unknown executable or database state remains blocked without deleting evidence.
pub async fn recover(install: &Path, paths: &AppPaths) -> Result<BinaryUpdate> {
    settle(install, paths, false).await
}

async fn settle(install: &Path, paths: &AppPaths, rollback: bool) -> Result<BinaryUpdate> {
    let install = directory(install)?;
    let target = install.join(program_name());
    independent_updater(&target)?;
    let guard = InstallationGuard::acquire(&install, true)?;
    service_preflight(&install, paths)?;
    let offline = OfflineProfile::acquire(paths)?;
    settle_owned(&install, paths, &guard, &offline, rollback).await
}

async fn settle_owned(
    install: &Path,
    paths: &AppPaths,
    guard: &InstallationGuard,
    offline: &OfflineProfile,
    rollback: bool,
) -> Result<BinaryUpdate> {
    let target = install.join(program_name());
    let mut record = jarvis_storage::binary_update_status(install)?
        .ok_or(Error::Refused("no binary update to recover/roll back"))?;
    if record.profile_data != fs::canonicalize(paths.data())?
        || !probe()
            .compatibility
            .permits(&record.previous.compatibility)
    {
        return Err(Error::Refused("recovery profile or schema mismatch"));
    }
    let database = offline.check().await?;
    jarvis_storage::verify_database_backup(paths, record.backup).await?;
    if record.state.is_pending()
        && database != (record.database_bytes, record.database_sha256.clone())
    {
        return Err(Error::Refused(
            "database changed during interrupted activation; preserve all evidence",
        ));
    }
    let actual = files::fingerprint(&target)?;
    if actual != identity(&record.previous) && actual != identity(&record.candidate) {
        return Err(Error::Refused(
            "installed executable is neither recorded image; preserve all evidence",
        ));
    }
    let stage = stage_dir(install, record.id)?;
    if files::fingerprint(&stage.join("previous"))? != identity(&record.previous) {
        return Err(Error::Refused(
            "retained previous executable is missing or altered",
        ));
    }
    if rollback
        || record.state == BinaryUpdateState::RollingBack
        || actual == identity(&record.previous)
    {
        record.state = BinaryUpdateState::RollingBack;
        record.database_bytes = database.0;
        record.database_sha256 = database.1.clone();
        guard.write_update(&record)?;
        apply_image(&stage.join("previous"), &target, &record.previous, offline).await?;
        record.state = BinaryUpdateState::RolledBack;
    } else {
        if image(&target).await? != record.candidate {
            return Err(Error::Refused(
                "candidate health check failed; use explicit rollback",
            ));
        }
        record.state = BinaryUpdateState::Applied;
    }
    if offline.check().await? != database {
        return Err(Error::Refused(
            "database changed during recovery; preserve all evidence",
        ));
    }
    guard.write_update(&record)?;
    Ok(record)
}

#[cfg(test)]
mod tests;
