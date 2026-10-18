//! Binary maintenance compatibility and durable intent (ADR-0242).

use serde::{Deserialize, Serialize};

/// Exact compatibility required by the first, non-migrating updater.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct UpdateCompatibility {
    /// Maintenance protocol; version one requires installation locking.
    pub maintenance: u32,
    /// SQLite schema owned by the binary.
    pub database: i64,
    /// Configuration schema owned by the binary.
    pub configuration: u32,
    /// Local daemon protocol.
    pub protocol: u16,
}

impl UpdateCompatibility {
    /// Whether switching binaries requires no schema/protocol migration.
    #[must_use]
    pub fn permits(&self, other: &Self) -> bool {
        self.maintenance == 1 && self == other
    }
}

/// Retained executable identity, independent of a provider SDK.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BinaryImage {
    /// Complete executable SHA-256.
    pub sha256: String,
    /// Complete executable size.
    pub bytes: u64,
    /// Reported package version.
    pub version: String,
    /// Reported maintenance contract.
    pub compatibility: UpdateCompatibility,
}

/// A transition is pending until binary and database evidence are checked.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BinaryUpdateState {
    /// Activation may or may not have replaced the executable.
    Activating,
    /// Previous-image restoration may or may not have replaced the executable.
    RollingBack,
    /// Candidate passed offline health and executable checks.
    Applied,
    /// Retained previous image passed the checks.
    RolledBack,
}

impl BinaryUpdateState {
    /// Pending state forbids daemon startup.
    #[must_use]
    pub const fn is_pending(self) -> bool {
        matches!(self, Self::Activating | Self::RollingBack)
    }
}

/// Latest installation transition, binding retained images to one profile.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BinaryUpdate {
    /// Durable format version.
    pub format: u32,
    /// Opaque `UUIDv7` update directory identity.
    pub id: crate::UpdateId,
    /// Canonical data directory that was checked during admission.
    pub profile_data: std::path::PathBuf,
    /// Retained database snapshot; never restored by binary maintenance.
    pub backup: crate::BackupId,
    /// Database identity at the start of the currently recorded transition.
    pub database_sha256: String,
    /// Standalone database length at that boundary.
    pub database_bytes: u64,
    /// Retained executable before activation.
    pub previous: BinaryImage,
    /// Signed executable selected for activation.
    pub candidate: BinaryImage,
    /// Durable transition state.
    pub state: BinaryUpdateState,
}
