use std::{
    collections::BTreeMap,
    sync::{Arc, OnceLock},
};

use misaki_rs::{G2P, Language};
use ort::{
    session::{RunOptions, Session},
    value::Tensor,
};

use super::assets::Prepared;
use crate::{MAX_AUDIO_BYTES, SpeechError};

static RUNTIME: OnceLock<Result<(), ()>> = OnceLock::new();
const SEGMENT_TOKENS: usize = 500;

pub(super) struct Engine {
    session: Session,
    phonemizer: G2P,
    vocabulary: BTreeMap<String, i64>,
    voice: Vec<f32>,
}

impl Engine {
    pub(super) fn new(assets: &Prepared, voice: &str) -> Result<Self, SpeechError> {
        RUNTIME
            .get_or_init(|| {
                let environment = ort::init_from(&assets.library)
                    .map_err(|_| ())?
                    .with_telemetry(false)
                    .with_logger(Arc::new(|_, _, _, _, _| {}));
                if environment.commit() {
                    Ok(())
                } else {
                    Err(())
                }
            })
            .as_ref()
            .map_err(|()| {
                SpeechError::Settings("the verified embedded runtime could not be initialized")
            })?;
        let session = Session::builder()
            .map_err(native_error)?
            .with_intra_threads(2)
            .map_err(native_error)?
            .with_inter_threads(1)
            .map_err(native_error)?
            .with_parallel_execution(false)
            .map_err(native_error)?
            .commit_from_memory(&assets.model)
            .map_err(native_error)?;
        let tokenizer: serde_json::Value =
            serde_json::from_slice(&assets.tokenizer).map_err(|_| SpeechError::BadAudio)?;
        let vocabulary = serde_json::from_value(tokenizer["model"]["vocab"].clone())
            .map_err(|_| SpeechError::BadAudio)?;
        let voice_data = decode_voice(&assets.voice)?;
        let language = if voice == "bm_george" {
            Language::EnglishGB
        } else {
            Language::EnglishUS
        };
        Ok(Self {
            session,
            phonemizer: G2P::new(language),
            vocabulary,
            voice: voice_data,
        })
    }

    pub(super) fn synthesize(
        &mut self,
        text: &str,
        speaking_rate: f32,
        blend: Option<&[f32]>,
        options: &RunOptions,
    ) -> Result<Vec<u8>, SpeechError> {
        let (phonemes, _) = self
            .phonemizer
            .g2p(text)
            .map_err(|_| SpeechError::BadAudio)?;
        if phonemes.contains('❓') {
            return Err(SpeechError::Settings(
                "embedded speech could not pronounce this text",
            ));
        }
        // Match the model's character tokenizer and normalizer, without a second tokenizer runtime.
        let tokens: Vec<i64> = phonemes
            .chars()
            .filter_map(|c| self.vocabulary.get(&c.to_string()).copied())
            .collect();
        if tokens.is_empty() {
            return Err(SpeechError::BadAudio);
        }
        let space = self
            .vocabulary
            .get(" ")
            .copied()
            .ok_or(SpeechError::BadAudio)?;
        let mut wav = vec![0; 44];
        for segment in segments(&tokens, space) {
            let mut ids = Vec::with_capacity(segment.len() + 2);
            ids.push(0);
            ids.extend_from_slice(segment);
            ids.push(0);
            let input = Tensor::from_array(([1, ids.len()], ids)).map_err(native_error)?;
            let start = segment.len() * 256;
            let voice = blend.unwrap_or(&self.voice);
            let style = Tensor::from_array(([1, 256], voice[start..start + 256].to_vec()))
                .map_err(native_error)?;
            let speed = Tensor::from_array(([1], vec![speaking_rate])).map_err(native_error)?;
            let output = self
                .session
                .run_with_options(
                    ort::inputs!["input_ids" => input, "style" => style, "speed" => speed],
                    options,
                )
                .map_err(native_error)?;
            let (_, samples) = output[0]
                .try_extract_tensor::<f32>()
                .map_err(native_error)?;
            append_pcm(&mut wav, samples)?;
        }
        finish_wav(&mut wav)?;
        Ok(wav)
    }
}

pub(super) fn decode_voice(bytes: &[u8]) -> Result<Vec<f32>, SpeechError> {
    if bytes.len() != 510 * 256 * 4 {
        return Err(SpeechError::BadAudio);
    }
    let values: Vec<_> = bytes
        .as_chunks::<4>()
        .0
        .iter()
        .map(|b| f32::from_le_bytes(*b))
        .collect();
    if values.iter().any(|n| !n.is_finite()) {
        return Err(SpeechError::BadAudio);
    }
    Ok(values)
}

// Convex weights bound every result to finite float32 inputs; the single rounding is intentional.
#[allow(clippy::cast_possible_truncation)]
pub(super) fn mix_voice(
    heart: &[f32],
    george: &[f32],
    heart_percent: u16,
) -> Result<Vec<f32>, SpeechError> {
    if heart.len() != 510 * 256
        || george.len() != heart.len()
        || heart_percent > 100
        || heart.iter().chain(george).any(|v| !v.is_finite())
    {
        return Err(SpeechError::BadAudio);
    }
    if heart_percent == 100 {
        return Ok(heart.to_vec());
    }
    if heart_percent == 0 {
        return Ok(george.to_vec());
    }
    Ok(heart
        .iter()
        .zip(george)
        .map(|(h, g)| {
            ((f64::from(*h) * f64::from(heart_percent)
                + f64::from(*g) * f64::from(100 - heart_percent))
                / 100.0) as f32
        })
        .collect())
}

fn native_error<T>(_: ort::Error<T>) -> SpeechError {
    SpeechError::BadAudio
}

fn segments(mut tokens: &[i64], space: i64) -> Vec<&[i64]> {
    let mut out = Vec::new();
    while !tokens.is_empty() {
        let mut end = tokens.len().min(SEGMENT_TOKENS);
        if end < tokens.len()
            && let Some(index) = tokens[..end].iter().rposition(|&id| id == space)
        {
            end = index + 1;
        }
        out.push(&tokens[..end]);
        tokens = &tokens[end..];
    }
    out
}

#[allow(clippy::cast_possible_truncation)] // Clamp to the signed 16-bit PCM range before quantization.
fn append_pcm(wav: &mut Vec<u8>, samples: &[f32]) -> Result<(), SpeechError> {
    if samples.is_empty()
        || samples.len() > MAX_AUDIO_BYTES.saturating_sub(wav.len()) / 2
        || samples.iter().any(|sample| !sample.is_finite())
    {
        return Err(SpeechError::BadAudio);
    }
    for sample in samples {
        wav.extend_from_slice(&((sample.clamp(-1., 1.) * 32767.).round() as i16).to_le_bytes());
    }
    Ok(())
}

fn finish_wav(wav: &mut [u8]) -> Result<(), SpeechError> {
    if wav.len() <= 44 || wav.len() > MAX_AUDIO_BYTES {
        return Err(SpeechError::BadAudio);
    }
    let size = u32::try_from(wav.len() - 8).map_err(|_| SpeechError::BadAudio)?;
    let data_size = u32::try_from(wav.len() - 44).map_err(|_| SpeechError::BadAudio)?;
    wav[..4].copy_from_slice(b"RIFF");
    wav[4..8].copy_from_slice(&size.to_le_bytes());
    wav[8..16].copy_from_slice(b"WAVEfmt ");
    wav[16..20].copy_from_slice(&16_u32.to_le_bytes());
    wav[20..22].copy_from_slice(&1_u16.to_le_bytes());
    wav[22..24].copy_from_slice(&1_u16.to_le_bytes());
    wav[24..28].copy_from_slice(&24000_u32.to_le_bytes());
    wav[28..32].copy_from_slice(&48000_u32.to_le_bytes());
    wav[32..34].copy_from_slice(&2_u16.to_le_bytes());
    wav[34..36].copy_from_slice(&16_u16.to_le_bytes());
    wav[36..40].copy_from_slice(b"data");
    wav[40..44].copy_from_slice(&data_size.to_le_bytes());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn weighted_style_rows_and_pure_endpoints_match_independent_values() {
        let heart: Vec<_> = (0_u16..510)
            .flat_map(|row| vec![f32::from(row); 256])
            .collect();
        let george: Vec<_> = heart.iter().map(|v| v + 4.0).collect();
        let blend = mix_voice(&heart, &george, 25).unwrap_or_else(|e| panic!("{e}"));
        for row in [0_usize, 1, 127, 500, 509] {
            assert_eq!(
                &blend[row * 256..(row + 1) * 256],
                vec![heart[row * 256] + 3.0; 256]
            );
        }
        assert_eq!(mix_voice(&heart, &george, 100).ok(), Some(heart.clone()));
        assert_eq!(mix_voice(&heart, &george, 0).ok(), Some(george.clone()));
        assert!(mix_voice(&heart, &george, 101).is_err());
        assert!(mix_voice(&heart[..256], &george, 50).is_err());
        assert!(mix_voice(&vec![f32::NAN; heart.len()], &george, 0).is_err());
        assert!(decode_voice(&[0; 4]).is_err());
    }

    #[test]
    fn long_phonemes_are_split_without_losing_or_duplicating_tokens() {
        let mut tokens = vec![2; 1600];
        tokens[399] = 1;
        let parts = segments(&tokens, 1);
        assert_eq!(parts[0].len(), 400);
        assert!(parts.iter().all(|part| part.len() <= SEGMENT_TOKENS));
        assert_eq!(parts.concat(), tokens);
    }

    #[test]
    fn wav_has_correct_pcm_format_and_rejects_nonfinite_or_excess_audio() {
        let mut wav = vec![0; 44];
        append_pcm(&mut wav, &[0., 1., -1.]).unwrap_or_else(|error| panic!("{error}"));
        finish_wav(&mut wav).unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(&wav[..4], b"RIFF");
        assert_eq!(&wav[8..16], b"WAVEfmt ");
        assert_eq!(
            u32::from_le_bytes(
                wav[24..28]
                    .try_into()
                    .unwrap_or_else(|error| panic!("{error}"))
            ),
            24000
        );
        assert_eq!(
            u32::from_le_bytes(
                wav[40..44]
                    .try_into()
                    .unwrap_or_else(|error| panic!("{error}"))
            ),
            6
        );
        assert_eq!(&wav[44..], &[0, 0, 255, 127, 1, 128]);
        assert!(append_pcm(&mut wav, &[f32::NAN]).is_err());
        assert!(append_pcm(&mut vec![0; MAX_AUDIO_BYTES], &[0.]).is_err());
        assert!(finish_wav(&mut [0; 44]).is_err());
    }
}
