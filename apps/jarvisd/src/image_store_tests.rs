//! The picture store and its route: what is kept, what is refused, and that only the owner can read it.

use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use jarvis_core::ClientCredential;
use jarvis_mcp_transport::ImageSink;
use tower::ServiceExt;

use super::*;
use crate::gateway::{GatewayState, router};

struct Scratch(PathBuf);

impl Drop for Scratch {
    fn drop(&mut self) {
        jarvis_core::remove_scratch_dir(&self.0);
    }
}

fn scratch() -> Scratch {
    let directory =
        std::env::temp_dir().join(format!("jarvis-images-{}", jarvis_core::scratch_tag()));
    std::fs::create_dir_all(&directory).unwrap_or_else(|error| panic!("{error}"));
    Scratch(directory)
}

fn png() -> Vec<u8> {
    let mut bytes = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    bytes.extend_from_slice(&[0; 32]);
    bytes
}

#[test]
fn a_picture_is_kept_and_read_back_by_its_signature() {
    let scratch = scratch();
    let store = ImageStore::new(&scratch.0);
    let id = store
        .keep("image/png", &png())
        .unwrap_or_else(|| panic!("kept"));
    assert!(is_id(&id), "{id}");
    let read = store.read(&id).unwrap_or_else(|| panic!("read"));
    assert_eq!(read.media_type, "image/png");
    assert_eq!(read.bytes, png());
}

#[test]
fn what_the_server_declared_does_not_decide() {
    let scratch = scratch();
    let store = ImageStore::new(&scratch.0);
    // A script declared as a PNG is not a picture; a PNG declared as text is one.
    assert!(
        store
            .keep("image/png", b"<script>alert(1)</script>")
            .is_none()
    );
    assert!(
        store
            .keep("image/png", b"<svg xmlns='http://www.w3.org/2000/svg'/>")
            .is_none()
    );
    assert!(store.keep("image/svg+xml", b"<svg/>").is_none());
    let id = store
        .keep("text/plain", &png())
        .unwrap_or_else(|| panic!("kept"));
    assert_eq!(
        store.read(&id).map(|image| image.media_type),
        Some("image/png")
    );
}

#[test]
fn every_accepted_kind_is_recognised() {
    let jpeg = [0xFF, 0xD8, 0xFF, 0xE0, 0, 0];
    let gif = *b"GIF89a\x01\0\x01\0";
    let webp = *b"RIFF\x10\0\0\0WEBPVP8 ";
    assert_eq!(Kind::detect(&jpeg), Some(Kind::Jpeg));
    assert_eq!(Kind::detect(&gif), Some(Kind::Gif));
    assert_eq!(Kind::detect(&webp), Some(Kind::Webp));
    // RIFF without WEBP is a WAV or an AVI, not a picture.
    assert_eq!(Kind::detect(b"RIFF\x10\0\0\0WAVEfmt "), None);
    assert_eq!(Kind::detect(b""), None);
}

#[test]
fn only_identifiers_the_store_made_are_looked_up() {
    let scratch = scratch();
    let store = ImageStore::new(&scratch.0);
    let id = store
        .keep("image/png", &png())
        .unwrap_or_else(|| panic!("kept"));
    for hostile in [
        "..",
        "../jarvis.sqlite3",
        "..\\..\\secret",
        "",
        &id.to_uppercase(),
        &format!("{id}.png"),
        &id[..31],
    ] {
        assert!(store.read(hostile).is_none(), "{hostile}");
    }
}

#[test]
fn a_file_placed_by_hand_is_not_served_as_a_picture() {
    let scratch = scratch();
    let store = ImageStore::new(&scratch.0);
    let id = "a".repeat(ID_CHARS);
    std::fs::create_dir_all(&store.directory).unwrap_or_else(|error| panic!("{error}"));
    std::fs::write(
        store.directory.join(format!("{id}.png")),
        b"<html><script>1</script>",
    )
    .unwrap_or_else(|error| panic!("{error}"));
    assert!(store.read(&id).is_none());
}

#[test]
fn the_oldest_pictures_go_first() {
    let scratch = scratch();
    let store = ImageStore::new(&scratch.0);
    let mut first = None;
    for index in 0..=MAX_KEPT_FILES {
        let id = store
            .keep("image/png", &png())
            .unwrap_or_else(|| panic!("kept"));
        if index == 0 {
            first = Some(id);
            // The clock's resolution can be coarse; make the first one clearly older.
            std::thread::sleep(std::time::Duration::from_millis(30));
        }
    }
    let count = std::fs::read_dir(&store.directory).map_or(0, Iterator::count);
    assert_eq!(count, MAX_KEPT_FILES);
    assert!(
        store.read(&first.unwrap_or_default()).is_none(),
        "the oldest was removed"
    );
}

struct Rig {
    app: axum::Router,
    credential: String,
    id: String,
    _scratch: Scratch,
}

async fn rig() -> Rig {
    let scratch = scratch();
    let store = Arc::new(ImageStore::new(&scratch.0));
    let id = store
        .keep("image/png", &png())
        .unwrap_or_else(|| panic!("kept"));
    let database = Arc::new(
        jarvis_storage::SqliteDatabase::open(&scratch.0.join("jarvis.sqlite3"))
            .await
            .unwrap_or_else(|error| panic!("{error}")),
    );
    let credential = ClientCredential::generate().unwrap_or_else(|error| panic!("{error}"));
    let presented = credential.expose().to_owned();
    let state = GatewayState::new(database, credential).with_images(store);
    Rig {
        app: router(state),
        credential: presented,
        id,
        _scratch: scratch,
    }
}

fn get(path: &str, credential: Option<&str>) -> Request<Body> {
    let mut builder = Request::builder().uri(path).method("GET");
    if let Some(credential) = credential {
        builder = builder.header(header::AUTHORIZATION, format!("Bearer {credential}"));
    }
    builder
        .body(Body::empty())
        .unwrap_or_else(|error| panic!("{error}"))
}

#[tokio::test]
async fn the_route_serves_the_picture_with_safe_headers_to_the_owner() {
    let rig = rig().await;
    let response = rig
        .app
        .clone()
        .oneshot(get(
            &format!("/api/v1/images/{}", rig.id),
            Some(&rig.credential),
        ))
        .await
        .unwrap_or_else(|error| panic!("{error:?}"));
    assert_eq!(response.status(), StatusCode::OK);
    let headers = response.headers();
    assert_eq!(headers[header::CONTENT_TYPE], "image/png");
    assert_eq!(headers[header::X_CONTENT_TYPE_OPTIONS], "nosniff");
    assert!(
        headers[header::CONTENT_SECURITY_POLICY]
            .to_str()
            .unwrap_or_default()
            .contains("sandbox")
    );
    let body = axum::body::to_bytes(response.into_body(), 1 << 20)
        .await
        .unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(body.to_vec(), png());
}

#[tokio::test]
async fn the_route_fails_closed_without_the_credential_and_for_unknown_ids() {
    let rig = rig().await;
    let path = format!("/api/v1/images/{}", rig.id);
    for (path, credential, expected) in [
        (path.as_str(), None, StatusCode::UNAUTHORIZED),
        (
            path.as_str(),
            Some("not-the-credential"),
            StatusCode::UNAUTHORIZED,
        ),
        (
            "/api/v1/images/..%2F..%2Fjarvis.sqlite3",
            Some(rig.credential.as_str()),
            StatusCode::NOT_FOUND,
        ),
        (
            "/api/v1/images/00000000000000000000000000000000",
            Some(rig.credential.as_str()),
            StatusCode::NOT_FOUND,
        ),
    ] {
        let response = rig
            .app
            .clone()
            .oneshot(get(path, credential))
            .await
            .unwrap_or_else(|error| panic!("{error:?}"));
        assert_eq!(response.status(), expected, "{path}");
    }
}
