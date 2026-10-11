//! The pictures tools produce, kept so the person can see them (`P11-011`, ADR-0188).
//!
//! A screenshot is the clearest answer to "what is on that page?", but it reaches the daemon as bytes inside a tool
//! result the model never sees. The store keeps those bytes in the profile's data directory under a random name, and the
//! console fetches them over the authenticated route, so the picture is shown without being added to the model's context.
//!
//! What is kept is decided from the **bytes** (a PNG, JPEG, GIF or WebP signature), never from what the server declared,
//! and is served with the media type the signature gave and `nosniff`. SVG and anything else that can carry script is
//! not a picture here. The folder is bounded: the oldest pictures go first.

use std::path::{Path, PathBuf};

use axum::Router;
use axum::extract::{Path as RoutePath, State};
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use jarvis_core::ErrorCode;

use crate::gateway::{GatewayState, error_response};

/// Pictures kept before the oldest are removed.
const MAX_KEPT_FILES: usize = 120;

/// Bytes kept in total before the oldest pictures are removed.
const MAX_KEPT_BYTES: u64 = 256 * 1024 * 1024;

/// A picture's identifier: 32 lowercase hexadecimal characters.
const ID_CHARS: usize = 32;

/// The picture kinds that are accepted, found from their first bytes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Kind {
    Png,
    Jpeg,
    Gif,
    Webp,
}

impl Kind {
    const ALL: [Self; 4] = [Self::Png, Self::Jpeg, Self::Gif, Self::Webp];

    fn detect(bytes: &[u8]) -> Option<Self> {
        if bytes.starts_with(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]) {
            Some(Self::Png)
        } else if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
            Some(Self::Jpeg)
        } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
            Some(Self::Gif)
        } else if bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
            Some(Self::Webp)
        } else {
            None
        }
    }

    const fn extension(self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Jpeg => "jpg",
            Self::Gif => "gif",
            Self::Webp => "webp",
        }
    }

    const fn media_type(self) -> &'static str {
        match self {
            Self::Png => "image/png",
            Self::Jpeg => "image/jpeg",
            Self::Gif => "image/gif",
            Self::Webp => "image/webp",
        }
    }
}

/// A picture read back.
pub(crate) struct StoredImage {
    pub(crate) media_type: &'static str,
    pub(crate) bytes: Vec<u8>,
}

/// The folder of kept pictures.
#[derive(Debug)]
pub struct ImageStore {
    directory: PathBuf,
}

impl ImageStore {
    /// A store in `<data>/images`. The folder is made when the first picture arrives.
    #[must_use]
    pub fn new(data: &Path) -> Self {
        Self {
            directory: data.join("images"),
        }
    }

    /// Reads a picture by identifier. `None` for anything that is not an identifier this store made.
    pub(crate) fn read(&self, id: &str) -> Option<StoredImage> {
        if !is_id(id) {
            return None;
        }
        Kind::ALL.iter().find_map(|kind| {
            let bytes =
                std::fs::read(self.directory.join(format!("{id}.{}", kind.extension()))).ok()?;
            // Read back through the same check, so a file someone placed here by hand is not served as a picture.
            (Kind::detect(&bytes) == Some(*kind)).then_some(StoredImage {
                media_type: kind.media_type(),
                bytes,
            })
        })
    }

    fn prune(&self) {
        let Ok(entries) = std::fs::read_dir(&self.directory) else {
            return;
        };
        let mut files: Vec<(std::time::SystemTime, u64, PathBuf)> = entries
            .filter_map(Result::ok)
            .filter_map(|entry| {
                let metadata = entry.metadata().ok()?;
                metadata.is_file().then(|| {
                    (
                        metadata.modified().ok().unwrap_or(std::time::UNIX_EPOCH),
                        metadata.len(),
                        entry.path(),
                    )
                })
            })
            .collect();
        files.sort_by_key(|(modified, _, _)| *modified);
        let mut count = files.len();
        let mut total: u64 = files.iter().map(|(_, size, _)| *size).sum();
        for (_, size, path) in files {
            if count <= MAX_KEPT_FILES && total <= MAX_KEPT_BYTES {
                break;
            }
            if std::fs::remove_file(&path).is_ok() {
                count -= 1;
                total = total.saturating_sub(size);
            }
        }
    }
}

impl jarvis_mcp_transport::ImageSink for ImageStore {
    fn keep(&self, _declared_mime: &str, bytes: &[u8]) -> Option<String> {
        let kind = Kind::detect(bytes)?;
        let id = new_id()?;
        std::fs::create_dir_all(&self.directory).ok()?;
        let target = self.directory.join(format!("{id}.{}", kind.extension()));
        let partial = self.directory.join(format!("{id}.partial"));
        // Written whole and then renamed, so a reader never meets half a picture.
        let written =
            std::fs::write(&partial, bytes).and_then(|()| std::fs::rename(&partial, &target));
        if written.is_err() {
            let _ = std::fs::remove_file(&partial);
            return None;
        }
        self.prune();
        Some(id)
    }
}

fn new_id() -> Option<String> {
    use std::fmt::Write as _;
    let mut bytes = [0_u8; ID_CHARS / 2];
    getrandom::fill(&mut bytes).ok()?;
    let mut id = String::with_capacity(ID_CHARS);
    for byte in bytes {
        let _ = write!(id, "{byte:02x}");
    }
    Some(id)
}

fn is_id(candidate: &str) -> bool {
    candidate.len() == ID_CHARS
        && candidate
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

/// The authenticated routes, mounted under /api/v1.
pub fn routes() -> Router<GatewayState> {
    Router::new().route("/images/{id}", axum::routing::get(read_image))
}

/// `GET /api/v1/images/{id}`
async fn read_image(
    State(state): State<GatewayState>,
    RoutePath(id): RoutePath<String>,
) -> Response {
    let Some(store) = state.images() else {
        return error_response(
            StatusCode::NOT_FOUND,
            ErrorCode::Validation,
            "this daemon keeps no pictures",
        );
    };
    let Some(image) = store.read(&id) else {
        return error_response(
            StatusCode::NOT_FOUND,
            ErrorCode::Validation,
            "no such picture",
        );
    };
    (
        [
            (
                header::CONTENT_TYPE,
                HeaderValue::from_static(image.media_type),
            ),
            (
                header::X_CONTENT_TYPE_OPTIONS,
                HeaderValue::from_static("nosniff"),
            ),
            (
                header::CONTENT_SECURITY_POLICY,
                HeaderValue::from_static("default-src 'none'; sandbox"),
            ),
            (
                header::CACHE_CONTROL,
                HeaderValue::from_static("private, max-age=31536000, immutable"),
            ),
        ],
        image.bytes,
    )
        .into_response()
}

#[cfg(test)]
#[path = "image_store_tests.rs"]
mod tests;
