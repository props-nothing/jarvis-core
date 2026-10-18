use crate::{ApiKey, SpeechDescription, SpeechError, SpeechFuture, SpeechProvider, speech};

/// Binary MP3 speech via an owner-configured OpenAI-compatible API base.
#[derive(Debug)]
pub struct OpenAiCompatibleSpeech {
    client: reqwest::Client,
    endpoint: reqwest::Url,
    voice: String,
    model: String,
    key: Option<ApiKey>,
    speed: Option<f64>,
}

impl OpenAiCompatibleSpeech {
    /// Build an adapter. The base includes `/v1`; a local server may omit the key.
    ///
    /// # Errors
    /// Refuses insecure remote HTTP, URL credentials, query/fragment, malformed settings.
    pub fn new(
        base: &str,
        voice: &str,
        model: &str,
        key: Option<&str>,
    ) -> Result<Self, SpeechError> {
        let mut endpoint = reqwest::Url::parse(base)
            .map_err(|_| SpeechError::Settings("invalid speech API base"))?;
        let client = speech::http_client(&endpoint)?;
        for value in [voice, model] {
            if value.is_empty() || value.len() > 128 || !value.bytes().all(|c| c.is_ascii_graphic())
            {
                return Err(SpeechError::Settings(
                    "voice and model must be bounded nonempty identifiers",
                ));
            }
        }
        let key = key
            .map(|value| {
                let value = value.trim();
                if value.is_empty()
                    || value.len() > 256
                    || !value.bytes().all(|c| c.is_ascii_graphic())
                {
                    Err(SpeechError::Settings("the key is empty or malformed"))
                } else {
                    Ok(ApiKey(value.to_owned()))
                }
            })
            .transpose()?;
        endpoint.set_path(&format!(
            "{}/audio/speech",
            endpoint.path().trim_end_matches('/')
        ));
        Ok(Self {
            client,
            endpoint,
            voice: voice.into(),
            model: model.into(),
            key,
            speed: None,
        })
    }
}

impl SpeechProvider for OpenAiCompatibleSpeech {
    fn configured(
        &self,
        voice: &jarvis_core::SavedVoice,
    ) -> Result<std::sync::Arc<dyn SpeechProvider>, SpeechError> {
        crate::validate_voice("openai-compatible", voice)?;
        let mut endpoint = self.endpoint.clone();
        let base = endpoint
            .path()
            .strip_suffix("/audio/speech")
            .unwrap_or("")
            .to_owned();
        endpoint.set_path(&base);
        let mut adapter = Self::new(
            endpoint.as_str(),
            &voice.voice,
            &voice.model,
            self.key.as_ref().map(|key| key.0.as_str()),
        )?;
        adapter.client = self.client.clone();
        adapter.speed = voice.settings.speed;
        Ok(std::sync::Arc::new(adapter))
    }
    fn describe(&self) -> SpeechDescription {
        SpeechDescription {
            id: "openai-compatible".into(),
            provider: "openai-compatible".into(),
            voice: self.voice.clone(),
            model: self.model.clone(),
        }
    }

    fn open<'a>(&'a self, text: &'a str, _previous_text: Option<&'a str>) -> SpeechFuture<'a> {
        Box::pin(async move {
            let text = speech::validate_text(text)?;
            let mut body = serde_json::json!({"input": text, "model": self.model, "voice": self.voice, "response_format": "mp3", "stream_format": "audio"});
            if let Some(speed) = self.speed {
                body["speed"] = serde_json::json!(speed);
            }
            let mut request = self
                .client
                .post(self.endpoint.clone())
                .header(reqwest::header::ACCEPT, "audio/mpeg")
                .json(&body);
            if let Some(key) = &self.key {
                request = request.bearer_auth(&key.0);
            }
            let response = request.send().await.map_err(|_| SpeechError::Unreachable)?;
            speech::mp3_response(response)
        })
    }
}
