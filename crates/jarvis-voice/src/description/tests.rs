use super::*;

#[tokio::test(start_paused = true)]
async fn gpu_failure_or_timeout_drops_attempt_before_one_sticky_cpu_retry() {
    struct Lease(Arc<AtomicBool>);
    impl Drop for Lease {
        fn drop(&mut self) {
            self.0.store(true, Ordering::Relaxed);
        }
    }
    for timeout in [false, true] {
        let provider = DescriptionSpeech::new(&std::env::temp_dir().join("missing-gpu-fixture"))
            .unwrap_or_else(|e| panic!("{e}"));
        provider.shared.cpu_only.store(false, Ordering::Relaxed);
        let dropped = Arc::new(AtomicBool::new(false));
        let calls = AtomicBool::new(false);
        let result = provider
            .retry_on_cpu(
                true,
                async {
                    let _lease = Lease(dropped.clone());
                    if timeout {
                        std::future::pending::<()>().await;
                    }
                    Err(SpeechError::BadAudio)
                },
                || async {
                    assert!(dropped.load(Ordering::Relaxed));
                    assert!(!calls.swap(true, Ordering::Relaxed));
                    Ok(42)
                },
            )
            .await;
        assert_eq!(result.ok(), Some(42));
        assert!(provider.shared.cpu_only.load(Ordering::Relaxed));
        assert!(
            provider
                .execution()
                .and_then(|e| e.fallback_reason)
                .is_some()
        );
        let failed_cpu = provider
            .retry_on_cpu(
                false,
                async { Err::<(), _>(SpeechError::BadAudio) },
                || async { panic!("CPU must not retry itself") },
            )
            .await;
        assert!(failed_cpu.is_err());
    }
}

#[tokio::test(start_paused = true)]
async fn cancellation_and_success_never_start_cpu_retry() {
    let provider = DescriptionSpeech::new(&std::env::temp_dir().join("missing-gpu-fixture"))
        .unwrap_or_else(|e| panic!("{e}"));
    provider.shared.cpu_only.store(false, Ordering::Relaxed);
    let cancelled = tokio::time::timeout(
        Duration::from_secs(1),
        provider.retry_on_cpu(
            true,
            std::future::pending::<Result<(), SpeechError>>(),
            || async { panic!("cancellation must not retry") },
        ),
    )
    .await;
    assert!(cancelled.is_err());
    assert!(!provider.shared.cpu_only.load(Ordering::Relaxed));
    assert!(
        provider
            .retry_on_cpu(true, async { Ok(()) }, || async {
                panic!("success must not retry")
            })
            .await
            .is_ok()
    );
}

#[test]
fn execution_metadata_rejects_wrong_backends_unbounded_or_fabricated_allocations() {
    let good = serde_json::json!({"backend":"amd","device":"AMD Radeon RX 9070 XT","allocated_bytes":5000,"peak_bytes":6000});
    let parse =
        |value: &serde_json::Value, gpu| WorkerInfo::parse(value.to_string().as_bytes(), gpu);
    assert!(parse(&good, true).is_ok());
    assert!(parse(&good, false).is_err());
    let mut nvidia = good.clone();
    nvidia["backend"] = "nvidia".into();
    assert!(parse(&nvidia, true).is_ok());
    for (key, value) in [
        ("backend", serde_json::json!("unknown")),
        ("device", serde_json::json!("bad\nname")),
        ("device", serde_json::json!("x".repeat(161))),
        ("allocated_bytes", serde_json::json!(0)),
        ("peak_bytes", serde_json::json!(4999)),
        ("peak_bytes", serde_json::json!(u64::MAX)),
        ("unexpected", serde_json::json!("private path")),
    ] {
        let mut invalid = good.clone();
        invalid[key] = value;
        assert!(parse(&invalid, true).is_err(), "accepted invalid {key}");
    }
    let cpu =
        serde_json::json!({"backend":"cpu","device":"CPU","allocated_bytes":0,"peak_bytes":0});
    assert!(parse(&cpu, false).is_ok());
    assert!(parse(&cpu, true).is_err());
}

fn reference() -> DesignedVoice {
    let mut wav = Vec::new();
    wav.extend(b"RIFF");
    wav.extend(38_u32.to_le_bytes());
    wav.extend(b"WAVEfmt ");
    wav.extend(16_u32.to_le_bytes());
    wav.extend([1, 0, 1, 0]);
    wav.extend(48000_u32.to_le_bytes());
    wav.extend(96000_u32.to_le_bytes());
    wav.extend([2, 0, 16, 0]);
    wav.extend(b"data");
    wav.extend(2_u32.to_le_bytes());
    wav.extend([1, 0]);
    DesignedVoice {
        quality: jarvis_core::VoiceQuality::default(),
        version: 1,
        model_revision: DESIGN_MODEL_REVISION.into(),
        description: "Warm narrator".into(),
        sample_text: "Hello.".into(),
        seed: 7,
        reference_audio: STANDARD.encode(wav),
    }
}

#[test]
fn quality_defaults_are_backward_compatible_and_other_adapters_refuse_it() {
    let mut serialized = serde_json::to_value(reference()).unwrap_or_else(|e| panic!("{e}"));
    serialized
        .as_object_mut()
        .unwrap_or_else(|| panic!("expected object"))
        .remove("quality");
    assert_eq!(
        serde_json::from_value::<DesignedVoice>(serialized.clone())
            .unwrap_or_else(|e| panic!("{e}"))
            .quality,
        VoiceQuality::Quality
    );
    serialized["quality"] = "unbounded".into();
    assert!(serde_json::from_value::<DesignedVoice>(serialized).is_err());
    assert_eq!(VoiceQuality::Fast.steps(), 4);
    assert_eq!(VoiceQuality::Balanced.steps(), 6);
    assert_eq!(VoiceQuality::Quality.steps(), 10);
    let mut voice: SavedVoice = serde_json::from_value(serde_json::json!({"id":"test","name":"Test","provider":"local","model":"kokoro-v1.0","voice":"af_heart","settings":{"quality":"fast"}})).unwrap_or_else(|e| panic!("{e}"));
    assert!(crate::validate_voice("embedded-kokoro", &voice).is_err());
    voice.model = "voxcpm2".into();
    voice.voice = "designed".into();
    voice.design = Some(reference());
    assert!(crate::validate_voice("local-voxcpm2", &voice).is_ok());
}

#[test]
fn generated_references_reject_other_adapters_tampered_audio_and_unsupported_controls() {
    let design = reference();
    assert!(validate_design(&design).is_ok());
    assert!(!format!("{design:?}").contains("Warm narrator"));
    let mut voice = SavedVoice {
        id: "synthetic".into(),
        name: "Narrator".into(),
        provider: "designer".into(),
        model: "voxcpm2".into(),
        voice: "designed".into(),
        settings: jarvis_core::VoiceSettings::default(),
        blend: None,
        design: Some(design.clone()),
    };
    assert!(crate::validate_voice("local-voxcpm2", &voice).is_ok());
    for kind in ["elevenlabs", "openai-compatible", "embedded-kokoro"] {
        assert!(crate::validate_voice(kind, &voice).is_err());
    }
    voice.settings.speed = Some(1.1);
    assert!(crate::validate_voice("local-voxcpm2", &voice).is_err());
    for field in ["revision", "audio", "description", "sample", "version"] {
        let mut changed = design.clone();
        match field {
            "revision" => changed.model_revision = "0".repeat(40),
            "audio" => changed.reference_audio = STANDARD.encode(b"not a WAV file"),
            "description" => changed.description = "x".repeat(501),
            "sample" => changed.sample_text = "x".repeat(121),
            _ => changed.version = 2,
        }
        assert!(validate_design(&changed).is_err());
    }
    let bytes = STANDARD
        .decode(design.reference_audio)
        .unwrap_or_else(|e| panic!("{e}"));
    for offset in [0, 4, 8, 16, 20, 22, 24, 28, 32, 34, 36, 40] {
        let mut corrupt = bytes.clone();
        corrupt[offset] ^= 1;
        assert!(validate_wav(&corrupt, 10).is_err());
    }
    assert!(validate_wav(&vec![1; 960_045], 10).is_err());
    assert!(DescriptionSpeech::new(Path::new("relative")).is_err());
}

#[tokio::test]
async fn missing_installation_and_empty_input_fail_without_starting_a_worker() {
    let provider =
        DescriptionSpeech::new(&std::env::temp_dir().join("jarvis-no-designer-installed"))
            .unwrap_or_else(|e| panic!("{e}"));
    assert!(
        provider
            .design_unavailable_reason()
            .is_some_and(|reason| reason.contains("Python runtime"))
    );
    assert!(provider.design("", "Hello", 1).await.is_err());
    assert!(provider.design("Warm narrator", "Hello", 1).await.is_err());
    assert!(provider.shared.worker.lock().await.is_none());
    assert_eq!(provider.shared.admission.available_permits(), 3);
}

#[test]
fn setup_checks_the_configured_installation_without_loading_a_model() {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "jarvis-designer-setup-{}-{nonce}",
        std::process::id()
    ));
    let python = DescriptionSpeech::python(&root);
    std::fs::create_dir_all(python.parent().unwrap_or(&root)).unwrap_or_else(|e| panic!("{e}"));
    std::fs::write(&python, b"fixture").unwrap_or_else(|e| panic!("{e}"));
    let provider = DescriptionSpeech::new(&root).unwrap_or_else(|e| panic!("{e}"));
    assert!(
        provider
            .design_unavailable_reason()
            .is_some_and(|reason| reason.contains("model is missing or incomplete"))
    );
    std::fs::create_dir(root.join("model")).unwrap_or_else(|e| panic!("{e}"));
    assert!(
        provider.design_unavailable_reason().is_some(),
        "an empty model directory is not an installation"
    );
    std::fs::remove_dir_all(root).unwrap_or_else(|e| panic!("{e}"));
}
