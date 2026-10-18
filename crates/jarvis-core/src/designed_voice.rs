//! A synthetic voice identity created from a description, not a human recording.

use serde::{Deserialize, Serialize};

/// A saved generated reference and the recipe that created it.
#[derive(Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct DesignedVoice {
    /// Quality used to create this reference; independent of later delivery settings.
    #[serde(default)]
    pub quality: crate::VoiceQuality,
    /// Format version.
    pub version: u32,
    /// Exact supported model revision.
    pub model_revision: String,
    /// The owner's requested voice characteristics.
    pub description: String,
    /// Words spoken in the generated reference.
    pub sample_text: String,
    /// Reproduction seed.
    pub seed: u32,
    /// Base64 PCM WAV, bounded to ten seconds by the adapter. Never a file path.
    pub reference_audio: String,
}

impl std::fmt::Debug for DesignedVoice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DesignedVoice")
            .field("version", &self.version)
            .field("reference_bytes", &self.reference_audio.len())
            .finish_non_exhaustive()
    }
}

impl DesignedVoice {
    /// Validate storage bounds before adapter validation.
    ///
    /// # Errors
    /// Refuses missing, oversized or unsupported fields without echoing content.
    pub fn validate(&self) -> Result<(), &'static str> {
        Self::validate_prompt(&self.description, &self.sample_text)?;
        if self.version != 1
            || self.model_revision.len() != 40
            || !self.model_revision.bytes().all(|b| b.is_ascii_hexdigit())
            || self.reference_audio.len() < 64
            || self.reference_audio.len() > 1_280_064
            || !self
                .reference_audio
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"+/=".contains(&b))
        {
            return Err("generate a valid local voice sample before saving");
        }
        Ok(())
    }

    /// Validate text for a bounded voice-design operation.
    ///
    /// # Errors
    /// Descriptions and sample text must be short, nonempty ordinary text.
    pub fn validate_prompt(description: &str, text: &str) -> Result<(), &'static str> {
        if description.trim().is_empty()
            || description.chars().count() > 500
            || text.trim().is_empty()
            || text.chars().count() > 120
            || description
                .chars()
                .chain(text.chars())
                .any(|c| c.is_control() && c != '\n')
        {
            return Err(
                "describe the voice in 1–500 characters and use 1–120 characters of sample text",
            );
        }
        Ok(())
    }
}
