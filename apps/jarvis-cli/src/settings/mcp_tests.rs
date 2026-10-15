use super::*;

fn words(line: &str) -> Vec<String> {
    line.split_whitespace().map(str::to_owned).collect()
}

#[test]
fn named_key_commands_validate_targets_sources_and_redact_errors() {
    for (line, action, json) in [
        ("keys set mcp:github:pat", McpKeyAction::Set, false),
        ("keys remove mcp:github:pat", McpKeyAction::Remove, false),
        (
            "keys status mcp:github:pat --json",
            McpKeyAction::Status,
            true,
        ),
        (
            "keys set mcp:github:pat --file input.key",
            McpKeyAction::Set,
            false,
        ),
        (
            "keys --from-env CANARY set mcp:github:pat",
            McpKeyAction::Set,
            false,
        ),
    ] {
        let command = parse_mcp_key_command(&words(line))
            .unwrap_or_else(|error| panic!("valid command: {error}"));
        assert_eq!(command.action, action);
        assert_eq!(command.reference.as_str(), "mcp:github:pat");
        assert_eq!(command.json, json);
    }
    for line in [
        "keys set mcp:github:pat canary-secret-as-argument",
        "keys remove mcp:github:pat canary-secret-as-argument",
        "keys status mcp:github:pat canary-secret-as-argument",
        "keys set mcp:github:pat --value canary-secret-as-argument",
        "keys set mcp:github:pat --file one --from-env TWO",
        "keys set mcp:github:pat --file one --file two",
        "keys set mcp:github:pat --from-env ONE --from-env TWO",
        "keys set mcp:github:pat --file",
        "keys set mcp:github:pat --file --json",
        "keys set mcp:github:pat --from-env",
        "keys set mcp:github:pat --root",
        "keys remove mcp:github:pat --file input.key",
        "keys status mcp:github:pat --from-env CANARY",
        "keys status mcp:github:pat --json --json",
        "keys test mcp:github:pat",
        "keys set mcp:github:../canary-secret-as-argument",
        "keys set mcp:github:canary-secret-as-argument:extra",
        "keys set MCP:github:pat",
        "keys set mcp:github:",
    ] {
        let error = parse_mcp_key_command(&words(line))
            .err()
            .unwrap_or_else(|| panic!("unsafe command accepted"));
        assert!(!error.contains("canary-secret-as-argument"));
        assert!(!error.contains("input.key"));
    }
}

#[test]
fn all_reader_inputs_are_bounded_without_echoing_content() {
    let mut reader = std::io::Cursor::new(vec![b'a'; MAX_KEY_INPUT_BYTES * 2]);
    assert_eq!(
        read_bounded_key(&mut reader),
        Err(KEY_INPUT_ERROR.to_owned())
    );
    assert_eq!(reader.position(), MAX_KEY_INPUT_BYTES as u64 + 1);
    assert_eq!(
        read_bounded_key(&b"canary\n"[..]),
        Ok("canary\n".to_owned())
    );
    assert_eq!(
        read_bounded_key(&[0xff][..]),
        Err(KEY_INPUT_ERROR.to_owned())
    );
}

#[tokio::test]
async fn named_commands_store_round_trip_and_remove_without_touching_config() {
    let root = std::env::current_dir()
        .unwrap_or_else(|error| panic!("working directory: {error}"))
        .join("target")
        .join(format!("mcp-cli-key-tests-{}", jarvis_core::scratch_tag()));
    std::fs::create_dir_all(&root).unwrap_or_else(|error| panic!("test profile: {error}"));
    let paths = AppPaths::from_root(&root).unwrap_or_else(|error| panic!("test paths: {error}"));
    let reference = McpSecretRef::parse("mcp:github:pat")
        .unwrap_or_else(|error| panic!("test reference: {error}"));
    let source = root.join("input.key");
    std::fs::write(&source, "  canary-cli-secret\n")
        .unwrap_or_else(|error| panic!("test input: {error}"));
    let mut arguments = words("keys set mcp:github:pat --file");
    arguments.push(source.display().to_string());
    assert!(matches!(
        keys(&paths, &arguments, None).await,
        ExitStatus::Ok
    ));
    assert_eq!(
        mcp_secrets::resolve(&paths, &reference),
        Ok("canary-cli-secret".to_owned())
    );
    assert!(!paths.config().join("config.toml").exists());
    let output = mcp_key_status(&paths, &reference, true);
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&output).ok(),
        Some(serde_json::json!({ "reference": "mcp:github:pat", "state": "set" }))
    );
    assert!(!output.contains("canary-cli-secret"));
    assert!(!output.contains("mcp-keys"));
    assert!(!output.contains(&root.display().to_string()));
    assert_eq!(
        mcp_key_status(&paths, &reference, false),
        "mcp:github:pat set"
    );
    assert!(matches!(
        keys(&paths, &words("keys remove mcp:github:pat"), None).await,
        ExitStatus::Ok
    ));
    assert_eq!(
        mcp_key_status(&paths, &reference, false),
        "mcp:github:pat not set"
    );
    assert!(matches!(
        keys(&paths, &words("keys status mcp:github:../pat"), None).await,
        ExitStatus::Usage
    ));
    jarvis_core::remove_scratch_dir(&root);
}
