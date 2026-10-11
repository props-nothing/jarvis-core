//! Where the pictures an MCP tool returns go (`P11-011`).
//!
//! A browser server's screenshot arrives as an `image` block: base64 bytes and a media type. The model cannot use
//! those bytes, and putting them in its context would cost far more than the picture is worth, but the **person**
//! should see them. This module is the seam: the adapter hands each picture to a sink, the sink (the daemon's image
//! store) keeps it and returns an identifier, and the adapter tells the model only that a picture was shown.
//!
//! The sink is process-wide because the daemon type-erases its tool adapters, and there is one daemon per process.
//! Without a sink installed (a library user, most tests) pictures are simply dropped, as they were before.

use std::sync::{Arc, OnceLock};

use crate::client::McpCallResult;

/// Keeps a picture and says where it went.
pub trait ImageSink: Send + Sync {
    /// Keeps one picture. `None` when it was not kept (it is not a picture this sink accepts, or storing failed):
    /// the call does not fail for it.
    fn keep(&self, declared_mime: &str, bytes: &[u8]) -> Option<String>;
}

static SINK: OnceLock<Arc<dyn ImageSink>> = OnceLock::new();

/// Installs the sink. Returns `false` when one was already installed (the first one stays).
pub fn install_image_sink(sink: Arc<dyn ImageSink>) -> bool {
    SINK.set(sink).is_ok()
}

/// Hands a result's pictures to the installed sink and returns the identifiers it gave back.
#[must_use]
pub fn keep_images(result: &McpCallResult) -> Vec<String> {
    keep_images_with(SINK.get().map(Arc::as_ref), result)
}

fn keep_images_with(sink: Option<&dyn ImageSink>, result: &McpCallResult) -> Vec<String> {
    let Some(sink) = sink else {
        return Vec::new();
    };
    result
        .images
        .iter()
        .filter_map(|image| sink.keep(&image.mime_type, &image.bytes))
        .collect()
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;
    use crate::client::McpImage;

    struct Recording(Mutex<Vec<usize>>);

    impl ImageSink for Recording {
        fn keep(&self, _declared_mime: &str, bytes: &[u8]) -> Option<String> {
            let mut kept = self
                .0
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            kept.push(bytes.len());
            // The second picture is refused, to show a refusal does not stop the rest.
            (kept.len() != 2).then(|| format!("id{}", kept.len()))
        }
    }

    fn result_with(sizes: &[usize]) -> McpCallResult {
        McpCallResult {
            is_error: false,
            images: sizes
                .iter()
                .map(|size| McpImage {
                    mime_type: "image/png".to_owned(),
                    bytes: vec![0; *size],
                })
                .collect(),
            text: String::new(),
            structured: None,
        }
    }

    #[test]
    fn without_a_sink_pictures_are_dropped() {
        assert!(keep_images_with(None, &result_with(&[10])).is_empty());
    }

    #[test]
    fn each_picture_is_offered_and_a_refusal_skips_only_that_one() {
        let sink = Recording(Mutex::new(Vec::new()));
        let ids = keep_images_with(Some(&sink), &result_with(&[1, 2, 3]));
        assert_eq!(ids, vec!["id1".to_owned(), "id3".to_owned()]);
    }
}
