use jarvis_core::SavedVoice;
use serde::Serialize;

use crate::SpeechError;

/// One model-supported control, shared by API validation and the editor.
#[derive(Clone, Debug, Serialize)]
pub struct VoiceControl {
    /// JSON control field.
    pub key: &'static str,
    /// Human-readable label.
    pub label: &'static str,
    /// number or boolean.
    pub kind: &'static str,
    /// Lower bound.
    pub min: f64,
    /// Upper bound.
    pub max: f64,
    /// Editor step; v3 stability is restricted to its three modes.
    pub step: f64,
    /// Suggested explicit value; absent controls retain provider defaults.
    pub suggested: f64,
}

fn number(
    key: &'static str,
    label: &'static str,
    min: f64,
    max: f64,
    suggested: f64,
) -> VoiceControl {
    VoiceControl {
        key,
        label,
        kind: "number",
        min,
        max,
        step: 0.05,
        suggested,
    }
}

/// Known model suggestions, not an account availability claim.
#[must_use]
pub fn voice_models(kind: &str) -> &'static [&'static str] {
    match kind {
        "elevenlabs" => &[
            "eleven_v4_turbo",
            "eleven_v4",
            "eleven_multilingual_v2",
            "eleven_flash_v2_5",
            "eleven_turbo_v2_5",
            "eleven_v3",
        ],
        "embedded-kokoro" => &["kokoro-v1.0"],
        "local-voxcpm2" => &["voxcpm2"],
        "openai-compatible" => &[
            "gpt-4o-mini-tts",
            "gpt-4o-mini-tts-2025-12-15",
            "tts-1",
            "tts-1-hd",
            "kokoro",
        ],
        _ => &[],
    }
}

/// Verified controls for this model. Unknown models get no assumed controls.
#[must_use]
pub fn voice_controls(kind: &str, model: &str) -> Vec<VoiceControl> {
    let stability = || number("stability", "Stability", 0.0, 1.0, 0.5);
    let similarity = || number("similarity_boost", "Similarity", 0.0, 1.0, 0.75);
    match (kind, model) {
        ("embedded-kokoro", "kokoro-v1.0") => vec![number("speed", "Speed", 0.5, 2.0, 1.0)],
        ("openai-compatible", model) if voice_models(kind).contains(&model) => {
            vec![number("speed", "Speed", 0.25, 4.0, 1.0)]
        }
        ("elevenlabs", "eleven_v4" | "eleven_v4_turbo") => vec![stability(), similarity()],
        ("elevenlabs", "eleven_v3") => {
            let mut control = stability();
            control.step = 0.5;
            vec![control]
        }
        (
            "elevenlabs",
            "eleven_multilingual_v2"
            | "eleven_flash_v2_5"
            | "eleven_turbo_v2_5"
            | "eleven_flash_v2"
            | "eleven_turbo_v2",
        ) => vec![
            number("speed", "Speed", 0.7, 1.2, 1.0),
            stability(),
            similarity(),
            number("style", "Style exaggeration", 0.0, 1.0, 0.0),
            VoiceControl {
                key: "use_speaker_boost",
                label: "Speaker boost",
                kind: "boolean",
                min: 0.0,
                max: 1.0,
                step: 1.0,
                suggested: 1.0,
            },
        ],
        _ => vec![],
    }
}

/// Validate bounds and refuse any control the selected engine cannot honor.
///
/// # Errors
/// Returns a safe setting error before synthesis, including for unknown controls/models.
pub fn validate_voice(kind: &str, voice: &SavedVoice) -> Result<(), SpeechError> {
    voice.validate().map_err(SpeechError::Settings)?;
    if voice.settings.quality.is_some() && kind != "local-voxcpm2" {
        return Err(SpeechError::Settings(
            "generation quality is only supported by the local voice designer",
        ));
    }
    if let Some(design) = &voice.design {
        if kind != "local-voxcpm2" || voice.model != "voxcpm2" || voice.voice != "designed" {
            return Err(SpeechError::Settings(
                "generated voice references are only supported by the local voice designer",
            ));
        }
        crate::description::validate_design(design)?;
    } else if kind == "local-voxcpm2" {
        return Err(SpeechError::Settings(
            "generate a voice before previewing or saving it",
        ));
    }
    if let Some(blend) = &voice.blend {
        if kind != "embedded-kokoro" || voice.model != "kokoro-v1.0" || voice.voice != "blend" {
            return Err(SpeechError::Settings(
                "voice blends are supported only by embedded Kokoro with voice blend",
            ));
        }
        crate::design::validate_blend(blend)?;
    }
    if kind == "embedded-kokoro"
        && (voice.model != "kokoro-v1.0"
            || (voice.blend.is_none() && !matches!(voice.voice.as_str(), "af_heart" | "bm_george")))
    {
        return Err(SpeechError::Settings(
            "embedded Kokoro supports model kokoro-v1.0 and voices af_heart or bm_george",
        ));
    }
    let controls = voice_controls(kind, &voice.model);
    for (key, value) in [
        ("speed", voice.settings.speed),
        ("stability", voice.settings.stability),
        ("similarity_boost", voice.settings.similarity_boost),
        ("style", voice.settings.style),
        (
            "use_speaker_boost",
            voice
                .settings
                .use_speaker_boost
                .map(|v| if v { 1.0 } else { 0.0 }),
        ),
    ] {
        if let Some(value) = value {
            let Some(control) = controls.iter().find(|c| c.key == key) else {
                return Err(SpeechError::Settings(
                    "a voice control is unsupported by this model; reset unsupported controls",
                ));
            };
            if !value.is_finite() || value < control.min || value > control.max {
                return Err(SpeechError::Settings(
                    "a voice control is outside its supported range",
                ));
            }
            if kind == "elevenlabs"
                && voice.model == "eleven_v3"
                && ![0.0, 0.5, 1.0].contains(&value)
            {
                return Err(SpeechError::Settings(
                    "Eleven v3 stability must be 0, 0.5 or 1",
                ));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use jarvis_core::VoiceSettings;

    #[test]
    fn unsupported_controls_and_out_of_range_values_fail_closed() {
        let mut voice = SavedVoice {
            blend: None,
            design: None,
            id: "test".into(),
            name: "Test".into(),
            provider: "test".into(),
            model: "eleven_v4_turbo".into(),
            voice: "speaker".into(),
            settings: VoiceSettings {
                speed: Some(1.0),
                ..VoiceSettings::default()
            },
        };
        assert!(validate_voice("elevenlabs", &voice).is_err());
        voice.settings = VoiceSettings {
            stability: Some(0.25),
            ..VoiceSettings::default()
        };
        assert!(validate_voice("elevenlabs", &voice).is_ok());
        voice.model = "eleven_v3".into();
        assert!(validate_voice("elevenlabs", &voice).is_err());
        voice.model = "kokoro-v1.0".into();
        voice.voice = "af_heart".into();
        assert!(validate_voice("embedded-kokoro", &voice).is_err());
        voice.settings = VoiceSettings {
            speed: Some(2.1),
            ..VoiceSettings::default()
        };
        assert!(validate_voice("embedded-kokoro", &voice).is_err());
        voice.settings.speed = Some(0.8);
        assert!(validate_voice("embedded-kokoro", &voice).is_ok());
    }
}
