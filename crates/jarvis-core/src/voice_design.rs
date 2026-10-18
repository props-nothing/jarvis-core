//! Versioned local voice recipes. Adapters validate their supported assets and languages.

use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

/// One preset's whole percentage in a blend.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct VoiceBlendSource {
    /// Provider preset identifier, never a file or URL.
    pub voice: String,
    /// Integer percentage from zero to one hundred.
    pub weight: u16,
}

/// Reproducible linear style-vector blend; no audio or native paths are stored.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct VoiceBlend {
    /// Recipe arithmetic/format version.
    pub version: u32,
    /// Exact asset revision understood by the selected adapter.
    pub model_revision: String,
    /// Explicit pronunciation language, independent of blend weights.
    pub language: String,
    /// Two to eight unique preset percentages, totalling one hundred.
    pub sources: Vec<VoiceBlendSource>,
}

impl VoiceBlend {
    /// Validate bounded recipe structure before provider/native work.
    ///
    /// # Errors
    /// Invalid versions, identities, repeated sources or non-convex weights are rejected.
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.version != 1 || !(2..=8).contains(&self.sources.len()) {
            return Err("a voice blend needs recipe version 1 and two to eight presets");
        }
        if self.model_revision.is_empty()
            || self.model_revision.len() > 128
            || !self.model_revision.bytes().all(|b| b.is_ascii_graphic())
            || self.language.is_empty()
            || self.language.len() > 16
            || !self
                .language
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b == b'-')
        {
            return Err("a voice blend needs a bounded model revision and language");
        }
        let mut ids = BTreeSet::new();
        let mut total = 0_u16;
        for source in &self.sources {
            if !super::voice::identifier(&source.voice)
                || !ids.insert(&source.voice)
                || source.weight > 100
            {
                return Err(
                    "blend presets must be unique plain identifiers with percentages from 0 to 100",
                );
            }
            total += source.weight;
        }
        if total != 100 {
            return Err("blend percentages must add up to 100");
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recipe_rejects_unknown_fields_paths_duplicates_invalid_weights_and_versions() {
        let input = r#"{"version":1,"model_revision":"pinned","language":"en-us","sources":[{"voice":"one","weight":35},{"voice":"two","weight":65}]}"#;
        let mut recipe: VoiceBlend = serde_json::from_str(input).unwrap_or_else(|e| panic!("{e}"));
        assert!(recipe.validate().is_ok());
        recipe.sources[0].weight = 0;
        assert!(recipe.validate().is_err());
        recipe.sources[1].weight = 100;
        assert!(recipe.validate().is_ok());
        recipe.sources[0].voice = "../one".into();
        assert!(recipe.validate().is_err());
        recipe.sources[0].voice = "two".into();
        assert!(recipe.validate().is_err());
        recipe.sources[0].voice = "one".into();
        recipe.version = 2;
        assert!(recipe.validate().is_err());
        let mut value: serde_json::Value =
            serde_json::from_str(input).unwrap_or_else(|e| panic!("{e}"));
        value["path"] = "/model".into();
        assert!(serde_json::from_value::<VoiceBlend>(value).is_err());
        assert!(serde_json::from_str::<VoiceBlend>(&input.replace("35", "35.5")).is_err());
        assert!(serde_json::from_str::<VoiceBlend>(&input.replace("35", "-35")).is_err());
    }
}
