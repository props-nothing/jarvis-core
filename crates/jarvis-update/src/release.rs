//! Network/signature/archive adapters. No caller-supplied URLs or verifier paths.

use crate::{
    Error, Result, files,
    manifest::{self, MAX_BINARY},
};
use std::{
    fs::{self, File},
    io::{Read, Write},
    path::Path,
    process::Stdio,
    time::Duration,
};
use tokio::{io::AsyncReadExt, process::Command};

pub(crate) fn platform() -> Result<&'static str> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("windows", "x86_64") => Ok("windows-x86_64"),
        ("linux", "x86_64") => Ok("linux-x86_64"),
        ("linux", "aarch64") => Ok("linux-aarch64"),
        ("macos", "x86_64") => Ok("macos-x86_64"),
        ("macos", "aarch64") => Ok("macos-aarch64"),
        _ => Err(Error::Refused(
            "no signed package/verifier for this platform",
        )),
    }
}

fn verifier() -> Result<(&'static str, &'static str)> {
    Ok(match platform()? {
        "windows-x86_64" => (
            "cosign-windows-amd64.exe",
            "9fe59be0eca1271873ce019061335eb1ac419b7059202e797828467ddabe33be",
        ),
        "linux-x86_64" => (
            "cosign-linux-amd64",
            "4629c757b7618056f8ddd7e2625ae9fdd94c0372a65049520bc7d9df9efc7f71",
        ),
        "linux-aarch64" => (
            "cosign-linux-arm64",
            "c5d324e091826b0d7a78eb16fef316450b4eb9aaec045611c08ba06f5e73220a",
        ),
        "macos-x86_64" => (
            "cosign-darwin-amd64",
            "2347488e5d5b25336644024dfeca5601b190e91197a71a917bda44744aff106c",
        ),
        "macos-aarch64" => (
            "cosign-darwin-arm64",
            "5cf948c2f4dfe59687bdd0b8523709067383e03982cc543475c8a7dc70e92a76",
        ),
        _ => return Err(Error::Refused("unsupported verifier")),
    })
}

async fn download(url: &str, path: &Path, limit: u64) -> Result<()> {
    let client = reqwest::Client::builder()
        .https_only(true)
        .no_proxy()
        .redirect(reqwest::redirect::Policy::limited(5))
        .connect_timeout(Duration::from_secs(15))
        .timeout(Duration::from_secs(120))
        .build()?;
    let mut response = client.get(url).send().await?.error_for_status()?;
    if response.content_length().is_some_and(|n| n > limit) {
        return Err(Error::Refused("download exceeds limit"));
    }
    let mut file = File::create_new(path)?;
    let mut total = 0;
    while let Some(bytes) = response.chunk().await? {
        total += bytes.len() as u64;
        if total > limit {
            return Err(Error::Refused("download exceeds limit"));
        }
        file.write_all(&bytes)?;
    }
    file.sync_all()?;
    Ok(())
}

/// Fixed-argument, bounded process execution, with no terminal window on Windows.
pub(crate) async fn output(command: &mut Command, seconds: u64) -> Result<Vec<u8>> {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    #[cfg(windows)]
    {
        command.creation_flags(0x0800_0000);
    }
    let mut child = command.spawn()?;
    let mut stdout = child
        .stdout
        .take()
        .ok_or(Error::Refused("child output unavailable"))?
        .take(8193);
    let work = async {
        let mut bytes = Vec::new();
        stdout.read_to_end(&mut bytes).await?;
        if bytes.len() > 8192 {
            return Err(Error::Refused("child output exceeds limit"));
        }
        if !child.wait().await?.success() {
            return Err(Error::Refused(
                "signature verification or executable check failed",
            ));
        }
        Ok(bytes)
    };
    let result = tokio::time::timeout(Duration::from_secs(seconds), work).await;
    match result {
        Ok(Ok(bytes)) => Ok(bytes),
        other => {
            let _ = child.kill().await;
            match other {
                Ok(Err(error)) => Err(error),
                _ => Err(Error::Refused("verification/executable check timed out")),
            }
        }
    }
}

pub(crate) async fn verify(stage: &Path, tag: &str, version: &str) -> Result<manifest::Archive> {
    let (asset, pin) = verifier()?;
    let program = stage.join(if cfg!(windows) {
        "cosign.exe"
    } else {
        "cosign"
    });
    if files::fingerprint(&program)?.1 != pin {
        return Err(Error::Refused("signature verifier checksum mismatch"));
    }
    let manifest_path = stage.join("release-manifest.txt");
    let text = files::bounded(&manifest_path, 8192)?;
    files::bounded(&stage.join("release-manifest.sigstore.json"), 1_048_576)?;
    let trust = stage.join(format!("trust-{}", jarvis_core::scratch_tag()));
    fs::create_dir(&trust)?;
    let reference = if tag.starts_with('v') {
        format!("refs/tags/{tag}")
    } else {
        "refs/heads/main".to_owned()
    };
    let mut command = Command::new(&program);
    command.env_clear().current_dir(stage);
    for name in [
        "HOME",
        "USERPROFILE",
        "LOCALAPPDATA",
        "APPDATA",
        "XDG_CACHE_HOME",
    ] {
        command.env(name, &trust);
    }
    command.env("TMPDIR", &trust).env("TEMP", &trust);
    if let Some(root) = std::env::var_os("SystemRoot") {
        command.env("SystemRoot", root);
    }
    command.args(["verify-blob", "--timeout", "2m", "--bundle"])
        .arg(stage.join("release-manifest.sigstore.json"))
        .args(["--certificate-identity", &format!("https://github.com/props-nothing/jarvis-core/.github/workflows/release.yml@{reference}"), "--certificate-oidc-issuer", "https://token.actions.githubusercontent.com"])
        .arg(&manifest_path);
    let _ = asset;
    output(&mut command, 130)
        .await
        .map_err(|_| Error::Refused("publisher signature verification failed or timed out"))?;
    // The parser consumes exactly the signed bytes; a concurrent change refuses.
    if files::bounded(&manifest_path, 8192)? != text {
        return Err(Error::Refused("manifest changed during verification"));
    }
    manifest::parse(
        std::str::from_utf8(&text).map_err(|_| Error::Refused("manifest is not UTF-8"))?,
        tag,
        version,
        platform()?,
    )
}

pub(crate) async fn fetch(stage: &Path, tag: &str, version: &str) -> Result<manifest::Archive> {
    manifest::selector(tag, version)?;
    let base = format!("https://github.com/props-nothing/jarvis-core/releases/download/{tag}");
    download(
        &format!("{base}/release-manifest.txt"),
        &stage.join("release-manifest.txt"),
        8192,
    )
    .await?;
    download(
        &format!("{base}/release-manifest.sigstore.json"),
        &stage.join("release-manifest.sigstore.json"),
        1_048_576,
    )
    .await?;
    let (asset, _) = verifier()?;
    let program = stage.join(if cfg!(windows) {
        "cosign.exe"
    } else {
        "cosign"
    });
    download(
        &format!("https://github.com/sigstore/cosign/releases/download/v3.1.3/{asset}"),
        &program,
        MAX_BINARY,
    )
    .await?;
    files::executable(&program)?;
    let selected = verify(stage, tag, version).await?;
    download(
        &format!("{base}/{}", selected.name),
        &stage.join("archive"),
        selected.bytes,
    )
    .await?;
    if files::fingerprint(&stage.join("archive"))? != (selected.bytes, selected.sha256.clone()) {
        return Err(Error::Refused("archive size or SHA-256 mismatch"));
    }
    Ok(selected)
}

pub(crate) fn extract(
    stage: &Path,
    version: &str,
    selected: &manifest::Archive,
    destination: &Path,
) -> Result<()> {
    if files::fingerprint(&stage.join("archive"))? != (selected.bytes, selected.sha256.clone()) {
        return Err(Error::Refused("archive size or SHA-256 mismatch"));
    }
    let name = format!("jarvis-{version}-{}/{}", platform()?, crate::program_name());
    let mut output = File::create_new(destination)?;
    let archive = File::open(stage.join("archive"))?;
    if Path::new(&selected.name)
        .extension()
        .is_some_and(|ext| ext == "zip")
    {
        extract_zip(archive, &name, &mut output)?;
    } else {
        extract_tar(archive, &name, &mut output)?;
    }
    output.sync_all()?;
    files::executable(destination)?;
    Ok(())
}

fn copy_entry(reader: impl Read, bytes: u64, output: &mut File) -> Result<()> {
    if bytes == 0 || bytes > MAX_BINARY {
        return Err(Error::Refused("archive member size invalid"));
    }
    if std::io::copy(&mut reader.take(MAX_BINARY + 1), output)? != bytes {
        return Err(Error::Refused("archive member is truncated/oversized"));
    }
    Ok(())
}

fn extract_zip(input: File, name: &str, output: &mut File) -> Result<()> {
    let mut archive =
        zip::ZipArchive::new(input).map_err(|_| Error::Refused("invalid ZIP archive"))?;
    let mut found = false;
    if archive.len() > 4096 {
        return Err(Error::Refused("too many archive members"));
    }
    for index in 0..archive.len() {
        let entry = archive
            .by_index(index)
            .map_err(|_| Error::Refused("invalid ZIP member"))?;
        if entry.name_raw() == name.as_bytes() {
            if found
                || !entry.is_file()
                || entry
                    .unix_mode()
                    .is_some_and(|mode| mode & 0o170_000 != 0 && mode & 0o170_000 != 0o100_000)
            {
                return Err(Error::Refused("expected one regular executable"));
            }
            let bytes = entry.size();
            copy_entry(entry, bytes, output)?;
            found = true;
        }
    }
    if !found {
        return Err(Error::Refused("executable missing from archive"));
    }
    Ok(())
}

fn extract_tar(input: File, name: &str, output: &mut File) -> Result<()> {
    let decoder = flate2::read::GzDecoder::new(input).take(536_870_913);
    let mut archive = tar::Archive::new(decoder);
    let mut found = false;
    for (index, entry) in archive.entries()?.enumerate() {
        if index >= 4096 {
            return Err(Error::Refused("too many archive members"));
        }
        let entry = entry?;
        if entry.path_bytes().as_ref() == name.as_bytes() {
            if found || !entry.header().entry_type().is_file() {
                return Err(Error::Refused("expected one regular executable"));
            }
            let size = entry.size();
            copy_entry(entry, size, output)?;
            found = true;
        }
    }
    let mut decoder = archive.into_inner();
    std::io::copy(&mut decoder, &mut std::io::sink())?;
    if decoder.limit() == 0 || !found {
        return Err(Error::Refused(
            "archive decompression exceeded limit or executable missing",
        ));
    }
    Ok(())
}
