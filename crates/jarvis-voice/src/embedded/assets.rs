//! Fixed public assets. No configured URL, key, native path or hash is accepted.

use std::{
    fs,
    io::{Cursor, Read, Write},
    path::{Path, PathBuf},
    time::Duration,
};

use futures_util::StreamExt;
use sha2::{Digest, Sha256};

use crate::SpeechError;

const MODEL_BASE: &str = "https://huggingface.co/onnx-community/Kokoro-82M-v1.0-ONNX/resolve/1939ad2a8e416c0acfeecc08a694d14ef25f2231";
const RUNTIME_BASE: &str = "https://github.com/microsoft/onnxruntime/releases/download/v1.31.0";

struct Asset {
    name: &'static str,
    size: usize,
    sha256: &'static str,
}

const MODEL: Asset = Asset {
    name: "onnx/model_quantized.onnx",
    size: 92_361_116,
    sha256: "fbae9257e1e05ffc727e951ef9b9c98418e6d79f1c9b6b13bd59f5c9028a1478",
};
const TOKENIZER: Asset = Asset {
    name: "tokenizer.json",
    size: 3497,
    sha256: "77a02c8e164413299b4b4c403b14f8e0e1c1b727db4d46a09d6327b861060a34",
};
const HEART: Asset = Asset {
    name: "voices/af_heart.bin",
    size: 522_240,
    sha256: "d583ccff3cdca2f7fae535cb998ac07e9fcb90f09737b9a41fa2734ec44a8f0b",
};
const GEORGE: Asset = Asset {
    name: "voices/bm_george.bin",
    size: 522_240,
    sha256: "c4b235a4c1f2cd3b939fed08b899ce9385638b763f7b73a59616c4fc9bd6c9bc",
};

fn runtime() -> Result<Asset, SpeechError> {
    let (name, size, sha256) = match (std::env::consts::OS, std::env::consts::ARCH) {
        ("windows", "x86_64") => (
            "onnxruntime-win-x64-1.31.0.zip",
            82_887_349,
            "3958d8a44984160692c1b4ffd0de3cb48f8c39f9d34bb31e433db06e30b82195",
        ),
        ("windows", "aarch64") => (
            "onnxruntime-win-arm64-1.31.0.zip",
            85_806_468,
            "ef6206fa8d6975878d93a97035d280240ce5764c126a7430e5427e18923316d5",
        ),
        ("linux", "x86_64") => (
            "onnxruntime-linux-x64-1.31.0.tgz",
            11_408_003,
            "cc5c72baf5ae5c8238a6841f0897227be2d02826b9cf98eaf02fdefaeeb03a57",
        ),
        ("linux", "aarch64") => (
            "onnxruntime-linux-aarch64-1.31.0.tgz",
            10_331_104,
            "c5b8b3cca31f3d643a3b313b8f42e2d0f16a4fcefd02cda343d97bd5306afaab",
        ),
        ("macos", "aarch64") => (
            "onnxruntime-osx-arm64-1.31.0.tgz",
            36_549_009,
            "1031b36dd3bdaa8d976a6130d9cf2a05e9be320b93ae804f8427b91dc3432afc",
        ),
        _ => {
            return Err(SpeechError::Settings(
                "embedded speech is unsupported on this OS/architecture",
            ));
        }
    };
    Ok(Asset { name, size, sha256 })
}

pub(super) fn supported() -> Result<(), SpeechError> {
    runtime().map(|_| ())
}

pub(super) struct Prepared {
    pub library: PathBuf,
    pub model: Vec<u8>,
    pub tokenizer: Vec<u8>,
    pub voice: Vec<u8>,
}

pub(super) async fn prepare(
    root: &Path,
    voice: &str,
    download: bool,
) -> Result<Prepared, SpeechError> {
    let native = runtime()?;
    fs::create_dir_all(root).map_err(|_| cache_error())?;
    if fs::symlink_metadata(root)
        .map_err(|_| cache_error())?
        .file_type()
        .is_symlink()
    {
        return Err(cache_error());
    }
    let client = asset_client()?;
    let archive = fetch(root, &client, &native, RUNTIME_BASE, download).await?;
    // Extract exactly the named regular member from the hash-verified archive, never unpack paths.
    let library = install_runtime(root, &native, &archive)?;
    let model = fetch(root, &client, &MODEL, MODEL_BASE, download).await?;
    let tokenizer = fetch(root, &client, &TOKENIZER, MODEL_BASE, download).await?;
    let voice = fetch(root, &client, voice_asset(voice)?, MODEL_BASE, download).await?;
    Ok(Prepared {
        library,
        model,
        tokenizer,
        voice,
    })
}

fn asset_client() -> Result<reqwest::Client, SpeechError> {
    reqwest::Client::builder()
        .no_proxy()
        .connect_timeout(Duration::from_secs(15))
        .timeout(Duration::from_secs(300))
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            if attempt.previous().len() < 5 && allowed_url(attempt.url()) {
                attempt.follow()
            } else {
                attempt.error("untrusted speech asset redirect")
            }
        }))
        .build()
        .map_err(|_| SpeechError::Unreachable)
}

fn voice_asset(voice: &str) -> Result<&'static Asset, SpeechError> {
    match voice {
        "af_heart" => Ok(&HEART),
        "bm_george" => Ok(&GEORGE),
        _ => Err(SpeechError::Settings("unknown pinned voice preset")),
    }
}

pub(super) async fn prepare_voice(
    root: &Path,
    voice: &str,
    download: bool,
) -> Result<Vec<u8>, SpeechError> {
    let asset = voice_asset(voice)?;
    let metadata = fs::symlink_metadata(root).map_err(|_| cache_error())?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err(cache_error());
    }
    fetch(root, &asset_client()?, asset, MODEL_BASE, download).await
}

fn allowed_url(url: &reqwest::Url) -> bool {
    url.scheme() == "https"
        && url.username().is_empty()
        && url.password().is_none()
        && url.port_or_known_default() == Some(443)
        && url.host_str().is_some_and(|host| {
            matches!(
                host,
                "github.com"
                    | "release-assets.githubusercontent.com"
                    | "huggingface.co"
                    | "cdn-lfs.huggingface.co"
            ) || host.ends_with(".hf.co")
        })
}

async fn fetch(
    root: &Path,
    client: &reqwest::Client,
    asset: &Asset,
    base: &str,
    download: bool,
) -> Result<Vec<u8>, SpeechError> {
    let path = root.join(asset.name.replace('/', "-"));
    match read_regular(&path, asset.size) {
        Ok(bytes) => {
            verify(asset, &bytes)?;
            return Ok(bytes);
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(_) => return Err(cache_error()),
    }
    if !download {
        return Err(SpeechError::Settings(
            "embedded speech assets are missing; enable allow_download and run jarvis doctor --live once",
        ));
    }
    let response = client
        .get(format!("{base}/{}", asset.name))
        .send()
        .await
        .map_err(|_| SpeechError::Unreachable)?;
    if !response.status().is_success() {
        return Err(SpeechError::Unreachable);
    }
    if response
        .content_length()
        .is_some_and(|size| size != asset.size as u64)
    {
        return Err(cache_error());
    }
    let mut stream = response.bytes_stream();
    let mut bytes = Vec::with_capacity(asset.size);
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|_| SpeechError::Unreachable)?;
        if chunk.len() > asset.size.saturating_sub(bytes.len()) {
            return Err(cache_error());
        }
        bytes.extend_from_slice(&chunk);
    }
    verify(asset, &bytes)?;
    publish(&path, &bytes)?;
    Ok(bytes)
}

fn verify(asset: &Asset, bytes: &[u8]) -> Result<(), SpeechError> {
    if bytes.len() == asset.size && format!("{:x}", Sha256::digest(bytes)) == asset.sha256 {
        Ok(())
    } else {
        Err(cache_error())
    }
}

fn read_regular(path: &Path, limit: usize) -> std::io::Result<Vec<u8>> {
    let meta = fs::symlink_metadata(path)?;
    if !meta.is_file() || meta.file_type().is_symlink() || meta.len() > limit as u64 {
        return Err(std::io::ErrorKind::InvalidData.into());
    }
    let mut bytes = Vec::new();
    fs::File::open(path)?
        .take(limit as u64 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > limit {
        return Err(std::io::ErrorKind::InvalidData.into());
    }
    Ok(bytes)
}

fn publish(path: &Path, bytes: &[u8]) -> Result<(), SpeechError> {
    // Concurrent preparations can publish identical assets; never overwrite different cached bytes.
    match read_regular(path, bytes.len()) {
        Ok(existing) if existing == bytes => return Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        _ => return Err(cache_error()),
    }
    let mut file = atomic_write_file::AtomicWriteFile::open(path).map_err(|_| cache_error())?;
    file.write_all(bytes).map_err(|_| cache_error())?;
    file.commit().map_err(|_| cache_error())
}

fn install_runtime(root: &Path, asset: &Asset, archive: &[u8]) -> Result<PathBuf, SpeechError> {
    let stem = asset.name.trim_end_matches(".zip").trim_end_matches(".tgz");
    let native = match std::env::consts::OS {
        "windows" => "onnxruntime.dll",
        "macos" => "libonnxruntime.1.31.0.dylib",
        _ => "libonnxruntime.so.1.31.0",
    };
    let directory = root.join(stem);
    fs::create_dir_all(&directory).map_err(|_| cache_error())?;
    if fs::symlink_metadata(&directory)
        .map_err(|_| cache_error())?
        .file_type()
        .is_symlink()
    {
        return Err(cache_error());
    }
    for member in [
        format!("lib/{native}"),
        "LICENSE".into(),
        "ThirdPartyNotices.txt".into(),
    ] {
        let bytes = member_bytes(
            archive,
            Path::new(asset.name)
                .extension()
                .is_some_and(|ext| ext == "zip"),
            &format!("{stem}/{member}"),
        )?;
        let name = member.rsplit('/').next().ok_or_else(cache_error)?;
        publish(&directory.join(name), &bytes)?;
    }
    Ok(directory.join(native))
}

fn member_bytes(archive: &[u8], zip: bool, name: &str) -> Result<Vec<u8>, SpeechError> {
    const LIMIT: u64 = 128 * 1024 * 1024;
    let mut bytes = Vec::new();
    if zip {
        let mut zip = zip::ZipArchive::new(Cursor::new(archive)).map_err(|_| cache_error())?;
        let file = zip.by_name(name).map_err(|_| cache_error())?;
        if !file.is_file() || file.is_symlink() || file.size() > LIMIT {
            return Err(cache_error());
        }
        file.take(LIMIT + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| cache_error())?;
    } else {
        let mut tar = tar::Archive::new(flate2::read::GzDecoder::new(archive));
        for entry in tar.entries().map_err(|_| cache_error())? {
            let entry = entry.map_err(|_| cache_error())?;
            let path = entry.path().map_err(|_| cache_error())?;
            // Apple's official tarball prefixes members with './'. This is still an exact member,
            // never an extraction destination or permission to accept parent traversal.
            if path.strip_prefix(".").unwrap_or(&path) != Path::new(name) {
                continue;
            }
            if !entry.header().entry_type().is_file() || entry.size() > LIMIT {
                return Err(cache_error());
            }
            entry
                .take(LIMIT + 1)
                .read_to_end(&mut bytes)
                .map_err(|_| cache_error())?;
            break;
        }
    }
    if bytes.is_empty() || bytes.len() as u64 > LIMIT {
        return Err(cache_error());
    }
    Ok(bytes)
}

fn cache_error() -> SpeechError {
    SpeechError::Settings(
        "embedded speech asset verification failed; remove the damaged speech cache and prepare again",
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cached_native_bytes_cannot_be_replaced_or_redirected()
    -> Result<(), Box<dyn std::error::Error>> {
        let root = std::env::temp_dir().join(format!("jarvis-asset-guard-{}", std::process::id()));
        fs::create_dir_all(&root)?;
        let path = root.join("native.fixture");
        fs::write(&path, b"verified native bytes")?;
        assert!(publish(&path, b"verified native bytes").is_ok());
        fs::write(&path, b"modified native bytes")?;
        assert!(publish(&path, b"verified native bytes").is_err());
        assert!(read_regular(&path, 1).is_err());
        assert!(read_regular(&root, 100).is_err());
        #[cfg(unix)]
        {
            let link = root.join("linked.fixture");
            std::os::unix::fs::symlink(&path, &link)?;
            assert!(read_regular(&link, 100).is_err());
            assert!(publish(&link, b"modified native bytes").is_err());
            fs::remove_file(link)?;
        }
        fs::remove_file(path)?;
        fs::remove_dir(root)?;
        Ok(())
    }

    #[test]
    fn integrity_fails_closed_for_mutation_truncation_and_oversize() {
        let asset = Asset {
            name: "fixture",
            size: 3,
            sha256: "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
        };
        assert!(verify(&asset, b"abc").is_ok());
        for bytes in [b"abd".as_slice(), b"ab", b"abcd"] {
            assert!(verify(&asset, bytes).is_err());
        }
    }

    #[test]
    fn redirects_cannot_downgrade_or_escape_asset_hosts() {
        for url in [
            "http://huggingface.co/a",
            "https://huggingface.co.evil.test/a",
            "https://evil.test/a",
            "https://x@github.com/a",
            "https://github.com:8443/a",
        ] {
            assert!(!allowed_url(
                &reqwest::Url::parse(url).unwrap_or_else(|error| panic!("{error}"))
            ));
        }
        for url in [
            "https://github.com/a",
            "https://release-assets.githubusercontent.com/a",
            "https://cas-bridge.xethub.hf.co/a",
        ] {
            assert!(allowed_url(
                &reqwest::Url::parse(url).unwrap_or_else(|error| panic!("{error}"))
            ));
        }
    }

    #[test]
    fn archives_never_supply_unnamed_files() {
        assert!(member_bytes(b"invalid", true, "runtime/lib/runtime.dll").is_err());
        assert!(member_bytes(b"invalid", false, "runtime/lib/runtime.so").is_err());
    }
}
