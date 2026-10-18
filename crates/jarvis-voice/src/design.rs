//! The supported local design contract, independent of the optional native runtime.

use crate::SpeechError;
use jarvis_core::{VoiceBlend, VoiceBlendSource};

/// Model and preset asset revision whose style vectors this recipe identifies.
pub const BLEND_MODEL_REVISION: &str = "1939ad2a8e416c0acfeecc08a694d14ef25f2231";

/// A balanced recipe for the editor; construction performs no synthesis or downloads.
#[must_use]
pub fn default_voice_blend() -> VoiceBlend {
    VoiceBlend {
        version: 1,
        model_revision: BLEND_MODEL_REVISION.into(),
        language: "en-us".into(),
        sources: vec![
            VoiceBlendSource {
                voice: "af_heart".into(),
                weight: 50,
            },
            VoiceBlendSource {
                voice: "bm_george".into(),
                weight: 50,
            },
        ],
    }
}

pub(crate) fn validate_blend(blend: &VoiceBlend) -> Result<(), SpeechError> {
    blend.validate().map_err(SpeechError::Settings)?;
    if blend.model_revision != BLEND_MODEL_REVISION
        || !matches!(blend.language.as_str(), "en-us" | "en-gb")
        || blend.sources.len() != 2
        || !blend
            .sources
            .iter()
            .all(|s| matches!(s.voice.as_str(), "af_heart" | "bm_george"))
    {
        return Err(SpeechError::Settings(
            "local blends require the pinned Kokoro v1.0 revision, Heart and George, and en-us or en-gb pronunciation",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use jarvis_core::{SavedVoice, VoiceSettings};

    #[test]
    fn blends_cannot_be_sent_to_cloud_or_substituted_for_unknown_assets() {
        let mut voice = SavedVoice {
            id: "blend-test".into(),
            name: "Blend".into(),
            provider: "local".into(),
            model: "kokoro-v1.0".into(),
            voice: "blend".into(),
            settings: VoiceSettings::default(),
            blend: Some(default_voice_blend()),
            design: None,
        };
        assert!(crate::validate_voice("embedded-kokoro", &voice).is_ok());
        assert!(crate::validate_voice("elevenlabs", &voice).is_err());
        assert!(crate::validate_voice("openai-compatible", &voice).is_err());
        voice.voice = "af_heart".into();
        assert!(crate::validate_voice("embedded-kokoro", &voice).is_err());
        voice.voice = "blend".into();
        voice.blend = None;
        assert!(crate::validate_voice("embedded-kokoro", &voice).is_err());
        for field in ["revision", "language", "source"] {
            let mut recipe = default_voice_blend();
            match field {
                "revision" => recipe.model_revision = "future".into(),
                "language" => recipe.language = "nl".into(),
                _ => recipe.sources[0].voice = "unavailable".into(),
            }
            voice.blend = Some(recipe);
            assert!(crate::validate_voice("embedded-kokoro", &voice).is_err());
        }
    }
}
