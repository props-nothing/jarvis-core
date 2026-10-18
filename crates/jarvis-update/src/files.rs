use crate::{Error, Result, manifest::MAX_BINARY};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File},
    io::{Read, Seek, Write},
    path::Path,
};

pub(crate) fn regular(path: &Path, directory: bool) -> Result<()> {
    if !path.is_absolute() {
        return Err(Error::Refused("maintenance paths must be absolute"));
    }
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink()
        || if directory {
            !metadata.is_dir()
        } else {
            !metadata.is_file()
        }
    {
        return Err(Error::Refused(
            "maintenance requires regular files/directories without links",
        ));
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes() & 0x400 != 0 {
            return Err(Error::Refused("reparse points are not maintenance targets"));
        }
    }
    Ok(())
}

pub(crate) fn fingerprint(path: &Path) -> Result<(u64, String)> {
    regular(path, false)?;
    let mut input = File::open(path)?.take(MAX_BINARY + 1);
    let mut hasher = Sha256::new();
    let mut total = 0;
    let mut buffer = vec![0u8; 65536];
    loop {
        let count = input.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        total += count as u64;
        if total > MAX_BINARY {
            return Err(Error::Refused("executable/archive exceeds size limit"));
        }
        hasher.update(&buffer[..count]);
    }
    Ok((total, format!("{:x}", hasher.finalize())))
}

pub(crate) fn bounded(path: &Path, limit: u64) -> Result<Vec<u8>> {
    regular(path, false)?;
    let mut bytes = Vec::new();
    File::open(path)?.take(limit + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > limit {
        return Err(Error::Refused("metadata exceeds size limit"));
    }
    Ok(bytes)
}

#[allow(clippy::unnecessary_wraps)] // Unix permission changes can fail.
pub(crate) fn executable(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    }
    #[cfg(not(unix))]
    {
        let _ = path;
    }
    Ok(())
}

pub(crate) fn copy_new(source: &Path, target: &Path) -> Result<()> {
    let expected = fingerprint(source)?;
    let mut output = File::create_new(target)?;
    std::io::copy(&mut File::open(source)?.take(MAX_BINARY + 1), &mut output)?;
    output.sync_all()?;
    executable(target)?;
    if fingerprint(target)? != expected {
        return Err(Error::Refused("retained executable copy mismatch"));
    }
    Ok(())
}

pub(crate) fn replace(source: &Path, target: &Path, expected: &(u64, String)) -> Result<()> {
    if fingerprint(source)? != *expected {
        return Err(Error::Refused("retained executable digest mismatch"));
    }
    regular(target, false)?;
    let mut options = atomic_write_file::AtomicWriteFile::options();
    options.read(true);
    #[cfg(unix)]
    {
        use atomic_write_file::unix::OpenOptionsExt;
        std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o700);
        options.preserve_mode(false);
    }
    let mut file = options.open(target)?;
    std::io::copy(&mut File::open(source)?.take(MAX_BINARY + 1), &mut file)?;
    file.sync_all()?;
    // Verify exactly the staged replacement, not only the input file.
    file.rewind()?;
    let mut hasher = Sha256::new();
    let mut bytes = 0;
    let mut buffer = vec![0u8; 65536];
    loop {
        let n = file.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        bytes += n as u64;
        hasher.update(&buffer[..n]);
    }
    if (bytes, format!("{:x}", hasher.finalize())) != *expected {
        return Err(Error::Refused("replacement digest mismatch"));
    }
    file.commit()?;
    Ok(())
}

pub(crate) fn write_json(path: &Path, value: &impl serde::Serialize) -> Result<()> {
    let bytes = serde_json::to_vec(value)?;
    let mut output = File::create_new(path)?;
    output.write_all(&bytes)?;
    output.sync_all()?;
    Ok(())
}
