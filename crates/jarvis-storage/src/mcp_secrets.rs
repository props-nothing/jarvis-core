//! Private profile-owned files for explicitly selected MCP credential references.

use std::{
    fs::{self, Metadata},
    io::{self, Read, Write},
    path::{Path, PathBuf},
};

use atomic_write_file::AtomicWriteFile;
use jarvis_core::McpSecretRef;

use crate::{AppPaths, PathKind, paths::prepare_private_directory, secure_private_file};

const MAX_SECRET_BYTES: u64 = 4096;
const UNAVAILABLE: &str = "the MCP key is unavailable or could not be managed securely";
const INVALID: &str =
    "the MCP key must contain 1..4096 ASCII graphic characters without whitespace";

/// Atomically stores a trimmed credential in its managed profile location.
///
/// # Errors
///
/// Refuses invalid values, symlink/reparse or non-regular endpoints, Unix hard
/// links, and failures to establish private directories/files. Errors never
/// contain values or paths.
pub fn set(paths: &AppPaths, reference: &McpSecretRef, raw: &str) -> Result<(), String> {
    let secret = validate(raw)?;
    let path = managed_path(paths, reference, true)?.ok_or_else(unavailable)?;
    let existing = inspect_file(&path)?;
    #[cfg(unix)]
    let permissions = existing.as_ref().and_then(private_permissions);
    if existing.is_some() {
        secure_private_file(PathKind::Config, &path).map_err(|_| unavailable())?;
    }
    let mut staged = open_atomic_file(&path).map_err(|_| unavailable())?;
    staged
        .write_all(secret.as_bytes())
        .map_err(|_| unavailable())?;
    managed_path(paths, reference, false)?.ok_or_else(unavailable)?;
    inspect_file(&path)?;
    staged.commit().map_err(|_| unavailable())?;
    secure_private_file(PathKind::Config, &path).map_err(|_| unavailable())?;
    #[cfg(unix)]
    if let Some(permissions) = permissions {
        fs::set_permissions(&path, permissions).map_err(|_| unavailable())?;
    }
    Ok(())
}

/// Resolves a credential using bounded reads of a regular managed file only.
///
/// # Errors
///
/// Refuses missing, unreadable, symlink/reparse, Unix hard-linked, empty,
/// oversized or malformed files with a value-free and path-free generic error.
pub fn resolve(paths: &AppPaths, reference: &McpSecretRef) -> Result<String, String> {
    let path = managed_path(paths, reference, false)?.ok_or_else(unavailable)?;
    let metadata = inspect_file(&path)?.ok_or_else(unavailable)?;
    if !(1..=MAX_SECRET_BYTES).contains(&metadata.len()) {
        return Err(unavailable());
    }
    let file = fs::File::open(path).map_err(|_| unavailable())?;
    let metadata = file.metadata().map_err(|_| unavailable())?;
    if !regular_file(&metadata) || !(1..=MAX_SECRET_BYTES).contains(&metadata.len()) {
        return Err(unavailable());
    }
    let mut raw = String::new();
    file.take(MAX_SECRET_BYTES + 1)
        .read_to_string(&mut raw)
        .map_err(|_| unavailable())?;
    if raw.len() as u64 > MAX_SECRET_BYTES {
        return Err(unavailable());
    }
    validate(&raw).map(str::to_owned).map_err(|_| unavailable())
}

/// Reports only `set` or `not set` using metadata, without opening the key file.
#[must_use]
pub fn state(paths: &AppPaths, reference: &McpSecretRef) -> &'static str {
    let present = managed_path(paths, reference, false)
        .ok()
        .flatten()
        .and_then(|path| inspect_file(&path).ok().flatten())
        .is_some_and(|metadata| (1..=MAX_SECRET_BYTES).contains(&metadata.len()));
    if present { "set" } else { "not set" }
}

/// Removes a managed credential. An already missing credential is a no-op.
///
/// # Errors
///
/// Refuses symlink/reparse, Unix hard-linked or non-regular endpoints and
/// filesystem failures without exposing the credential value or its path.
pub fn remove(paths: &AppPaths, reference: &McpSecretRef) -> Result<(), String> {
    let Some(path) = managed_path(paths, reference, false)? else {
        return Ok(());
    };
    if inspect_file(&path)?.is_none() {
        return Ok(());
    }
    fs::remove_file(path).map_err(|_| unavailable())
}

fn unavailable() -> String {
    UNAVAILABLE.to_owned()
}

fn validate(raw: &str) -> Result<&str, String> {
    let secret = raw.trim();
    if secret.is_empty()
        || secret.len() as u64 > MAX_SECRET_BYTES
        || !secret.bytes().all(|byte| byte.is_ascii_graphic())
    {
        return Err(INVALID.to_owned());
    }
    Ok(secret)
}

fn managed_path(
    paths: &AppPaths,
    reference: &McpSecretRef,
    create: bool,
) -> Result<Option<PathBuf>, String> {
    let keys = paths.config().join("mcp-keys");
    let server = keys.join(reference.server());
    let mut directories: Vec<&Path> = paths.config().ancestors().collect();
    directories.reverse();
    directories.extend([keys.as_path(), server.as_path()]);
    for directory in directories {
        let metadata = match fs::symlink_metadata(directory) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == io::ErrorKind::NotFound && create => {
                fs::create_dir(directory).map_err(|_| unavailable())?;
                fs::symlink_metadata(directory).map_err(|_| unavailable())?
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(_) => return Err(unavailable()),
        };
        if linked(&metadata) || !metadata.is_dir() {
            return Err(unavailable());
        }
        if create && directory.starts_with(paths.config()) {
            prepare_private_directory(PathKind::Config, directory).map_err(|_| unavailable())?;
        }
    }
    Ok(Some(server.join(format!("{}.key", reference.key()))))
}

fn inspect_file(path: &Path) -> Result<Option<Metadata>, String> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if regular_file(&metadata) => Ok(Some(metadata)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Ok(_) | Err(_) => Err(unavailable()),
    }
}

fn regular_file(metadata: &Metadata) -> bool {
    if linked(metadata) || !metadata.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if metadata.nlink() != 1 {
            return false;
        }
    }
    true
}

fn linked(metadata: &Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
        if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return true;
        }
    }
    metadata.file_type().is_symlink()
}

#[cfg(unix)]
fn private_permissions(metadata: &Metadata) -> Option<fs::Permissions> {
    use std::os::unix::fs::PermissionsExt;
    let permissions = metadata.permissions();
    (permissions.mode().trailing_zeros() >= 6).then_some(permissions)
}

#[cfg(unix)]
fn open_atomic_file(path: &Path) -> io::Result<AtomicWriteFile> {
    use atomic_write_file::unix::OpenOptionsExt as AtomicOpenOptionsExt;
    use std::os::unix::fs::OpenOptionsExt as StandardOpenOptionsExt;
    let mut options = AtomicWriteFile::options();
    StandardOpenOptionsExt::mode(&mut options, 0o600);
    AtomicOpenOptionsExt::preserve_mode(&mut options, false);
    options.open(path)
}

#[cfg(not(unix))]
fn open_atomic_file(path: &Path) -> io::Result<AtomicWriteFile> {
    AtomicWriteFile::open(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Profile {
        root: PathBuf,
        paths: AppPaths,
        reference: McpSecretRef,
    }

    impl Profile {
        fn new() -> Self {
            let root = std::env::current_dir()
                .unwrap_or_else(|error| panic!("working directory: {error}"))
                .join("target")
                .join(format!("mcp-secret-tests-{}", jarvis_core::scratch_tag()));
            fs::create_dir_all(&root).unwrap_or_else(|error| panic!("test profile: {error}"));
            Self {
                paths: AppPaths::from_root(&root)
                    .unwrap_or_else(|error| panic!("isolated paths: {error}")),
                reference: McpSecretRef::parse("mcp:github:canary_locator")
                    .unwrap_or_else(|error| panic!("reference: {error}")),
                root,
            }
        }

        fn file(&self) -> PathBuf {
            self.paths
                .config()
                .join("mcp-keys")
                .join("github")
                .join("canary_locator.key")
        }

        fn rejects_all_operations(&self) {
            assert_eq!(state(&self.paths, &self.reference), "not set");
            for result in [
                set(&self.paths, &self.reference, "unpersisted-canary"),
                resolve(&self.paths, &self.reference).map(|_| ()),
                remove(&self.paths, &self.reference),
            ] {
                assert_eq!(result, Err(UNAVAILABLE.to_owned()));
            }
        }
    }

    impl Drop for Profile {
        fn drop(&mut self) {
            jarvis_core::remove_scratch_dir(&self.root);
        }
    }

    #[test]
    fn round_trip_rotation_isolation_and_idempotent_removal() {
        let profile = Profile::new();
        let other = Profile::new();
        assert_eq!(state(&profile.paths, &profile.reference), "not set");
        assert!(resolve(&profile.paths, &profile.reference).is_err());
        assert_eq!(remove(&profile.paths, &profile.reference), Ok(()));
        assert_eq!(
            set(&profile.paths, &profile.reference, "  canary-token\n"),
            Ok(())
        );
        assert_eq!(
            resolve(&profile.paths, &profile.reference),
            Ok("canary-token".to_owned())
        );
        assert_eq!(state(&profile.paths, &profile.reference), "set");
        assert_eq!(state(&other.paths, &other.reference), "not set");
        assert!(profile.file().is_file());
        assert_eq!(
            set(&profile.paths, &profile.reference, "rotated-canary"),
            Ok(())
        );
        assert_eq!(
            resolve(&profile.paths, &profile.reference),
            Ok("rotated-canary".to_owned())
        );
        assert_eq!(remove(&profile.paths, &profile.reference), Ok(()));
        assert_eq!(state(&profile.paths, &profile.reference), "not set");
        assert_eq!(remove(&profile.paths, &profile.reference), Ok(()));
    }

    #[test]
    fn values_are_bounded_and_invalid_diagnostics_never_echo_values_or_paths() {
        let profile = Profile::new();
        for secret in [
            String::new(),
            " \n".to_owned(),
            "canary with spaces".to_owned(),
            "canary\tvalue".to_owned(),
            "canary\0value".to_owned(),
            "cánary".to_owned(),
            "a".repeat(4097),
        ] {
            assert_eq!(
                set(&profile.paths, &profile.reference, &secret),
                Err(INVALID.to_owned())
            );
        }
        assert!(!profile.file().exists());
        assert_eq!(
            set(&profile.paths, &profile.reference, &"a".repeat(4096)),
            Ok(())
        );
        assert_eq!(
            resolve(&profile.paths, &profile.reference).map(|value| value.len()),
            Ok(4096)
        );
        for bytes in [
            vec![],
            vec![b'a'; 4097],
            vec![0xff],
            b"canary with spaces".to_vec(),
        ] {
            fs::write(profile.file(), &bytes)
                .unwrap_or_else(|error| panic!("test fixture: {error}"));
            assert_eq!(
                resolve(&profile.paths, &profile.reference),
                Err(UNAVAILABLE.to_owned())
            );
        }
    }

    #[test]
    fn state_only_inspects_metadata_and_non_files_are_refused() {
        let profile = Profile::new();
        assert_eq!(set(&profile.paths, &profile.reference, "canary"), Ok(()));
        fs::write(profile.file(), [0xff]).unwrap_or_else(|error| panic!("fixture: {error}"));
        assert_eq!(state(&profile.paths, &profile.reference), "set");
        assert!(resolve(&profile.paths, &profile.reference).is_err());
        fs::remove_file(profile.file()).unwrap_or_else(|error| panic!("fixture: {error}"));
        fs::create_dir(profile.file()).unwrap_or_else(|error| panic!("fixture: {error}"));
        profile.rejects_all_operations();
    }

    #[cfg(unix)]
    #[test]
    fn public_permissions_are_not_preserved_on_overwrite() {
        use std::os::unix::fs::PermissionsExt;
        let profile = Profile::new();
        assert_eq!(set(&profile.paths, &profile.reference, "canary"), Ok(()));
        for public_mode in [0o640, 0o604, 0o666] {
            fs::set_permissions(profile.file(), fs::Permissions::from_mode(public_mode))
                .unwrap_or_else(|error| panic!("fixture permissions: {error}"));
            assert_eq!(set(&profile.paths, &profile.reference, "rotated"), Ok(()));
            let mode = fs::metadata(profile.file())
                .unwrap_or_else(|error| panic!("metadata: {error}"))
                .permissions()
                .mode()
                & 0o777;
            assert_eq!(mode, 0o600, "public permissions must not be restored");
        }
    }

    #[cfg(unix)]
    #[test]
    fn private_permissions_are_preserved_on_overwrite() {
        use std::os::unix::fs::PermissionsExt;
        let profile = Profile::new();
        assert_eq!(set(&profile.paths, &profile.reference, "canary"), Ok(()));
        let mode = || {
            fs::metadata(profile.file())
                .unwrap_or_else(|error| panic!("metadata: {error}"))
                .permissions()
                .mode()
                & 0o777
        };
        assert_eq!(mode(), 0o600);
        fs::set_permissions(profile.file(), fs::Permissions::from_mode(0o400))
            .unwrap_or_else(|error| panic!("fixture permissions: {error}"));
        assert_eq!(set(&profile.paths, &profile.reference, "rotated"), Ok(()));
        assert_eq!(mode(), 0o400);
    }

    #[cfg(unix)]
    #[test]
    fn symbolic_and_hard_linked_files_and_directory_components_fail_closed() {
        use std::os::unix::fs::symlink;
        for component in ["root", "config", "mcp-keys", "github", "canary_locator.key"] {
            let mut profile = Profile::new();
            let outside = profile.root.join("outside");
            fs::create_dir(&outside).unwrap_or_else(|error| panic!("fixture: {error}"));
            let path = match component {
                "root" => profile.root.join("alias"),
                "config" => profile.paths.config().to_path_buf(),
                "mcp-keys" => profile.paths.config().join(component),
                "github" => profile.paths.config().join("mcp-keys").join(component),
                _ => profile.file(),
            };
            fs::create_dir_all(path.parent().unwrap_or_else(|| panic!("parent")))
                .unwrap_or_else(|error| panic!("fixture: {error}"));
            symlink(&outside, &path).unwrap_or_else(|error| panic!("fixture link: {error}"));
            if component == "root" {
                profile.paths = AppPaths::from_root(&path)
                    .unwrap_or_else(|error| panic!("aliased profile: {error}"));
            }
            profile.rejects_all_operations();
            assert!(
                fs::read_dir(outside)
                    .unwrap_or_else(|error| panic!("fixture: {error}"))
                    .next()
                    .is_none()
            );
        }
        let profile = Profile::new();
        assert_eq!(set(&profile.paths, &profile.reference, "canary"), Ok(()));
        let outside = profile.root.join("outside.key");
        fs::hard_link(profile.file(), &outside)
            .unwrap_or_else(|error| panic!("hard link: {error}"));
        profile.rejects_all_operations();
        assert_eq!(fs::read_to_string(outside).ok().as_deref(), Some("canary"));
    }

    #[cfg(windows)]
    #[test]
    fn windows_directory_junctions_fail_closed_without_touching_the_target() {
        for component in ["root", "config", "mcp-keys", "github"] {
            let mut profile = Profile::new();
            let outside = profile.root.join("outside");
            fs::create_dir(&outside).unwrap_or_else(|error| panic!("fixture: {error}"));
            let path = match component {
                "root" => profile.root.join("alias"),
                "config" => profile.paths.config().to_path_buf(),
                "mcp-keys" => profile.paths.config().join(component),
                _ => profile.paths.config().join("mcp-keys").join(component),
            };
            fs::create_dir_all(path.parent().unwrap_or_else(|| panic!("parent")))
                .unwrap_or_else(|error| panic!("fixture: {error}"));
            let output = std::process::Command::new("cmd")
                .args(["/C", "mklink", "/J"])
                .arg(&path)
                .arg(&outside)
                .output()
                .unwrap_or_else(|error| panic!("junction fixture: {error}"));
            assert!(output.status.success(), "could not create fixture junction");
            if component == "root" {
                profile.paths = AppPaths::from_root(&path)
                    .unwrap_or_else(|error| panic!("aliased profile: {error}"));
            }
            profile.rejects_all_operations();
            assert!(
                fs::read_dir(outside)
                    .unwrap_or_else(|error| panic!("fixture: {error}"))
                    .next()
                    .is_none()
            );
            fs::remove_dir(path).unwrap_or_else(|error| panic!("remove junction: {error}"));
        }
    }
}
