//! Keyword-free discovery; owner previews are distinct from fenced model results.

use crate::MemoryReference;
use serde::{Deserialize, Serialize};

/// Filters for paged discovery of current memories. No keyword or caller-supplied workspace.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MemoryBrowseRequest {
    /// Last returned ID, or absent for the first page.
    pub cursor: Option<String>,
    /// One to twenty results; defaults to ten.
    pub limit: Option<u32>,
    /// Optional exact type name.
    pub memory_type: Option<String>,
    /// Optional exact entity identifier.
    pub entity_id: Option<String>,
}

/// A bounded owner-facing preview, not a model context item.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct MemoryPreview {
    /// Canonical metadata for an exact-ID read or version-guarded owner action.
    #[serde(flatten)]
    pub reference: MemoryReference,
    /// First 240 characters of the claim, rendered as plain text by the HUD.
    pub preview: String,
}

/// A live, newest-first page; insertions before the cursor do not shift later pages.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct MemoryBrowseReply {
    /// Current claims only; historical states remain available through owner read/export.
    pub memories: Vec<MemoryPreview>,
    /// Last returned ID when more eligible rows exist.
    pub next_cursor: Option<String>,
}
