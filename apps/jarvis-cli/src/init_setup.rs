use std::path::{Path, PathBuf};

use jarvis_storage::settings::validate_secret;

use super::{
    AppPaths, Config, ConfigStore, InitRequest, KEY_FILE_NAME, OLLAMA_BASE_URL,
    OLLAMA_PLACEHOLDER_KEY, SPEECH_KEY_FILE_NAME, VoiceSetup, ask, can_ask, choose_model,
    code_setup, ollama_models, render_config, report, resolve_folders, write_secret,
};
#[cfg(test)]
use super::{strip_verbatim, suggested_folders};

struct PreparedKey {
    path: PathBuf,
    value: String,
    write: bool,
}

impl PreparedKey {
    fn from_file(path: &Path) -> Result<Self, String> {
        if !path.is_absolute() || !path.is_file() {
            return Err("a key file must be the absolute path of an existing file".to_owned());
        }
        let raw = std::fs::read_to_string(path)
            .map_err(|_| "the key file could not be read".to_owned())?;
        Ok(Self {
            path: path.to_owned(),
            value: validate_secret(&raw)?,
            write: false,
        })
    }

    fn persist(&self) -> Result<(), String> {
        if self.write {
            write_secret(&self.path, &self.value)
                .map_err(|_| "the private key file could not be written".to_owned())?;
        }
        Ok(())
    }
}

fn key_input(
    file: Option<&Path>,
    destination: PathBuf,
    env: &str,
    prompt: &str,
    placeholder: bool,
) -> Result<PreparedKey, String> {
    if let Some(file) = file {
        return PreparedKey::from_file(file);
    }
    let raw = if placeholder {
        OLLAMA_PLACEHOLDER_KEY.to_owned()
    } else if let Some(value) = std::env::var(env)
        .ok()
        .filter(|value| !value.trim().is_empty())
    {
        value
    } else if can_ask() {
        rpassword::prompt_password(prompt).map_err(|_| {
            "the key could not be read securely; use a key file or environment variable".to_owned()
        })?
    } else {
        return Err(format!(
            "setup requires a key: set {env} or provide an absolute key-file path"
        ));
    };
    Ok(PreparedKey {
        path: destination,
        value: validate_secret(&raw)?,
        write: true,
    })
}

fn preset_url(choice: &str) -> Result<&'static jarvisd::model_probe::Preset, String> {
    let index = if choice.is_empty() {
        1
    } else {
        choice
            .parse::<usize>()
            .map_err(|_| "choose a provider number from the list".to_owned())?
    };
    index
        .checked_sub(1)
        .and_then(|index| jarvisd::model_probe::PRESETS.get(index))
        .ok_or_else(|| "choose a provider number from the list".to_owned())
}

async fn provider_url(request: &InitRequest) -> Result<String, String> {
    if let Some(url) = &request.base_url {
        return Ok(url.clone());
    }
    if !can_ask() {
        return ollama_models().await.map(|_| OLLAMA_BASE_URL.to_owned())
            .ok_or_else(|| "no local Ollama answered; provide --base-url, --model and --api-key-file for another provider".to_owned());
    }
    eprintln!("Choose a model provider (model names are loaded from that provider):");
    for (index, preset) in jarvisd::model_probe::PRESETS.iter().enumerate() {
        eprintln!("  {}. {}", index + 1, preset.name);
    }
    let answer = ask("Provider [default 1: local Ollama]: ").unwrap_or_default();
    let preset = preset_url(&answer)?;
    eprintln!("{}", preset.help);
    if preset.url.is_empty() {
        ask("OpenAI-compatible server address (ending in /v1): ")
            .filter(|url| !url.is_empty())
            .ok_or_else(|| "give the model server address".to_owned())
    } else {
        Ok(preset.url.to_owned())
    }
}

fn model_env(base: &str) -> &'static str {
    let normalized = base.trim().trim_end_matches('/');
    let id = jarvisd::model_probe::PRESETS
        .iter()
        .find(|preset| preset.url.trim_end_matches('/') == normalized)
        .map(|preset| preset.id);
    match id {
        Some("openai") => "OPENAI_API_KEY",
        Some("ollama-cloud") => "OLLAMA_API_KEY",
        Some("gemini") => "GEMINI_API_KEY",
        Some("anthropic") => "ANTHROPIC_API_KEY",
        _ => "MODEL_API_KEY",
    }
}

fn loopback(base: &str) -> Result<bool, String> {
    let url =
        reqwest::Url::parse(base).map_err(|_| "the model address is not usable".to_owned())?;
    if !matches!(url.scheme(), "http" | "https")
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err("the model address must be HTTP(S) without a login".to_owned());
    }
    Ok(url.host_str().is_some_and(|host| {
        host == "localhost"
            || host
                .trim_matches(['[', ']'])
                .parse::<std::net::IpAddr>()
                .is_ok_and(|ip| ip.is_loopback())
    }))
}

async fn model_setup(
    paths: &AppPaths,
    request: &InitRequest,
) -> Result<(String, String, PreparedKey), String> {
    let base = provider_url(request).await?;
    let local = loopback(&base)?;
    let key = key_input(
        request.api_key_file.as_deref(),
        paths.config().join(KEY_FILE_NAME),
        model_env(&base),
        "Model API key (hidden): ",
        local,
    )?;
    let model = if let Some(model) = &request.model {
        model.clone()
    } else {
        let listed = jarvisd::model_probe::probe(&base, &key.value, None).await?;
        let models: Vec<_> = listed.models.into_iter().map(|model| model.id).collect();
        if models.is_empty() {
            if let Some(problem) = listed.list_problem {
                eprintln!("Model list: {problem}");
            }
            ask("Model name (the provider could not list models): ")
                .filter(|name| !name.is_empty())
                .ok_or_else(|| "provide --model or enter a model name".to_owned())?
        } else {
            choose_model(None, &models)?
        }
    };
    eprintln!("Testing the model with a short fixed message (may use provider credits)...");
    let checked = jarvisd::model_probe::probe(&base, &key.value, Some(&model)).await?;
    require_model_test(checked.test.as_ref())?;
    Ok((model, base, key))
}

fn require_model_test(test: Option<&jarvisd::model_probe::Test>) -> Result<(), String> {
    match test {
        Some(test) if test.ok => Ok(()),
        Some(test) => Err(format!(
            "model test failed: {} No setup was saved.",
            test.message
        )),
        None => Err("the model was not tested; no setup was saved".to_owned()),
    }
}

async fn check_voice(base: &str, voice: &VoiceSetup, key: &str) -> Result<(), String> {
    let speech = jarvis_voice::ElevenLabsSpeech::new(
        base,
        voice
            .voice_id
            .as_deref()
            .unwrap_or(jarvis_voice::DEFAULT_VOICE_ID),
        voice
            .model
            .as_deref()
            .unwrap_or(jarvis_voice::DEFAULT_MODEL),
        key,
    )
    .map_err(|error| format!("voice settings are not usable: {error}"))?;
    let mut stream = speech.open("Systems online.", None).await.map_err(|error| format!(
        "ElevenLabs test failed: {error}. Check TTS access, voice ID, model and credits; no setup was saved."))?;
    match stream.next_chunk().await {
        Ok(Some(_)) => Ok(()),
        _ => Err("ElevenLabs returned no usable audio; no setup was saved".to_owned()),
    }
}

async fn voice_setup(
    paths: &AppPaths,
    request: &InitRequest,
    voice_base: &str,
) -> Result<(VoiceSetup, PreparedKey), String> {
    eprintln!(
        "ElevenLabs is required to finish new setup. Create a key at https://elevenlabs.io with text-to-speech access."
    );
    if request.elevenlabs {
        eprintln!(
            "Using ELEVENLABS_API_KEY, an explicit voice key file, or a hidden terminal prompt."
        );
    }
    let key = key_input(
        request.voice_key_file.as_deref(),
        paths.config().join(SPEECH_KEY_FILE_NAME),
        "ELEVENLABS_API_KEY",
        "ElevenLabs API key (hidden, required): ",
        false,
    )?;
    let voice = VoiceSetup {
        key_file: key.path.clone(),
        voice_id: Some(
            request
                .voice_id
                .clone()
                .unwrap_or_else(|| jarvis_voice::DEFAULT_VOICE_ID.to_owned()),
        ),
        model: request.voice_model.clone(),
    };
    eprintln!(
        "Testing voice {} with one short sentence (sent to ElevenLabs; may use credits)...",
        voice.voice_id.as_deref().unwrap_or_default()
    );
    check_voice(voice_base, &voice, &key.value).await?;
    Ok((voice, key))
}

pub(super) async fn configure(paths: &AppPaths, request: &InitRequest) -> Result<(), String> {
    configure_at(paths, request, jarvis_voice::ELEVENLABS_BASE_URL).await
}

async fn configure_at(
    paths: &AppPaths,
    request: &InitRequest,
    voice_base: &str,
) -> Result<(), String> {
    let code = code_setup(request)?;
    let (model, base, model_key) = model_setup(paths, request).await?;
    let (voice, voice_key) = voice_setup(paths, request, voice_base).await?;
    let roots = resolve_folders(&request.workspaces)?;
    let document = render_config(
        &model,
        &base,
        &model_key.path,
        &roots,
        code.as_ref(),
        Some(&voice),
    )?;
    let config = Config::parse_with_environment(&document, std::iter::empty::<(&str, &str)>())
        .map_err(|error| format!("the setup configuration is not usable: {error}"))?
        .into_config();
    model_key.persist()?;
    voice_key.persist()?;
    let store = ConfigStore::from_paths(paths);
    store
        .save(&config)
        .map_err(|error| format!("the configuration could not be saved: {error}"))?;
    report(
        store.path(),
        &model,
        &base,
        &roots,
        code.as_ref(),
        Some(&voice),
    );
    Ok(())
}

#[cfg(test)]
#[path = "init_setup_tests.rs"]
mod contract_tests;

#[cfg(test)]
mod tests {
    use super::*;

    fn must<T, E: std::fmt::Display>(result: Result<T, E>) -> T {
        result.unwrap_or_else(|error| panic!("{error}"))
    }

    fn problem<T>(result: Result<T, String>) -> String {
        result.err().unwrap_or_else(|| panic!("expected a refusal"))
    }

    #[test]
    fn provider_choices_share_the_console_presets_and_refuse_out_of_range() {
        assert_eq!(must(preset_url("")).id, "ollama-local");
        assert_eq!(must(preset_url("3")).url, "https://api.openai.com/v1");
        for choice in ["0", "7", "junk"] {
            assert!(preset_url(choice).is_err());
        }
    }

    #[test]
    fn keys_are_validated_without_echoing_the_value() {
        let secret = "secret with spaces";
        assert!(!problem(validate_secret(secret)).contains(secret));
        assert!(must(loopback("http://127.0.0.1:1234/v1")));
        assert!(must(loopback("http://[::1]:1234/v1")));
        assert!(!must(loopback("https://api.openai.com/v1")));
        assert!(loopback("https://user:secret@host/v1").is_err());
    }

    #[test]
    fn a_test_is_required_even_when_a_model_was_named() {
        assert!(require_model_test(None).is_err());
        let test = jarvisd::model_probe::Test {
            ok: false,
            message: "key rejected".to_owned(),
            millis: 0,
        };
        assert!(problem(require_model_test(Some(&test))).contains("No setup was saved"));
    }

    #[test]
    fn reading_a_key_does_not_persist_or_replace_it() {
        let dir = std::env::temp_dir().join(format!(
            "jarvis-setup-key-{}",
            jarvis_core::CorrelationId::new()
        ));
        must(std::fs::create_dir_all(&dir));
        let source = dir.join("provided.key");
        must(std::fs::write(&source, "synthetic-key"));
        let key = must(PreparedKey::from_file(&source));
        assert!(!key.write);
        assert_eq!(key.value, "synthetic-key");
        must(key.persist());
        assert_eq!(must(std::fs::read_to_string(&source)), "synthetic-key");
        assert!(PreparedKey::from_file(Path::new("relative.key")).is_err());
        must(std::fs::remove_dir_all(dir));
    }
}
