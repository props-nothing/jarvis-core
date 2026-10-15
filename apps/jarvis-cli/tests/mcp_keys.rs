//! Isolated executable acceptance checks for managed MCP credential commands.

use std::{
    fs,
    io::Write,
    path::PathBuf,
    process::{Command, Output, Stdio},
};

use jarvis_core::McpSecretRef;
use jarvis_storage::{AppPaths, mcp_secrets};

struct Profile {
    root: PathBuf,
    paths: AppPaths,
}

impl Profile {
    fn new() -> Self {
        let root = std::env::current_dir()
            .unwrap_or_else(|error| panic!("working directory: {error}"))
            .join("target")
            .join(format!(
                "mcp-key-command-tests-{}",
                jarvis_core::scratch_tag()
            ));
        fs::create_dir_all(&root).unwrap_or_else(|error| panic!("test profile: {error}"));
        Self {
            paths: AppPaths::from_root(&root).unwrap_or_else(|error| panic!("test paths: {error}")),
            root,
        }
    }

    fn command(&self, arguments: &[&str]) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_jarvis"));
        command
            .args(arguments)
            .arg("--root")
            .arg(&self.root)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        command
    }

    fn run(&self, arguments: &[&str]) -> Output {
        self.command(arguments)
            .output()
            .unwrap_or_else(|error| panic!("command: {error}"))
    }
}

impl Drop for Profile {
    fn drop(&mut self) {
        jarvis_core::remove_scratch_dir(&self.root);
    }
}

fn output_text(output: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

#[test]
fn named_mcp_keys_accept_private_file_env_and_stdin_with_metadata_only_status() {
    let profile = Profile::new();
    let reference =
        McpSecretRef::parse("mcp:github:pat").unwrap_or_else(|error| panic!("reference: {error}"));
    let input = profile.root.join("input.key");
    fs::write(&input, "  canary-file-secret\n").unwrap_or_else(|error| panic!("input: {error}"));
    let output = profile
        .command(&["keys", "set", reference.as_str()])
        .arg("--file")
        .arg(&input)
        .output()
        .unwrap_or_else(|error| panic!("command: {error}"));
    assert!(
        output.status.success(),
        "file input failed: {}",
        output_text(&output)
    );
    assert!(output_text(&output).contains("jarvis restart"));
    assert!(!output_text(&output).contains("canary-file-secret"));
    assert_eq!(
        mcp_secrets::resolve(&profile.paths, &reference)
            .ok()
            .as_deref(),
        Some("canary-file-secret")
    );

    let output = profile
        .command(&[
            "keys",
            "set",
            reference.as_str(),
            "--from-env",
            "JARVIS_MCP_TEST_INPUT",
        ])
        .env("JARVIS_MCP_TEST_INPUT", "canary-env-secret")
        .output()
        .unwrap_or_else(|error| panic!("command: {error}"));
    assert!(
        output.status.success(),
        "environment input failed: {}",
        output_text(&output)
    );
    assert!(!output_text(&output).contains("canary-env-secret"));
    assert_eq!(
        mcp_secrets::resolve(&profile.paths, &reference)
            .ok()
            .as_deref(),
        Some("canary-env-secret")
    );

    let mut child = profile
        .command(&["keys", "set", reference.as_str()])
        .stdin(Stdio::piped())
        .spawn()
        .unwrap_or_else(|error| panic!("command: {error}"));
    let mut stdin = child.stdin.take().unwrap_or_else(|| panic!("stdin pipe"));
    stdin
        .write_all(b"  canary-stdin-secret\n")
        .unwrap_or_else(|error| panic!("stdin: {error}"));
    drop(stdin);
    let output = child
        .wait_with_output()
        .unwrap_or_else(|error| panic!("command: {error}"));
    assert!(
        output.status.success(),
        "stdin input failed: {}",
        output_text(&output)
    );
    assert!(!output_text(&output).contains("canary-stdin-secret"));
    assert_eq!(
        mcp_secrets::resolve(&profile.paths, &reference)
            .ok()
            .as_deref(),
        Some("canary-stdin-secret")
    );

    let output = profile.run(&["keys", "status", reference.as_str(), "--json"]);
    assert!(output.status.success());
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&output.stdout).ok(),
        Some(serde_json::json!({ "reference": reference.as_str(), "state": "set" }))
    );
    assert!(!output_text(&output).contains("canary-stdin-secret"));
    assert!(!output_text(&output).contains(&profile.root.display().to_string()));
    assert!(!output_text(&output).contains("mcp-keys"));

    let output = profile.run(&["keys", "remove", reference.as_str()]);
    assert!(output.status.success());
    assert!(output_text(&output).contains("jarvis restart"));
    assert!(mcp_secrets::resolve(&profile.paths, &reference).is_err());
    let output = profile.run(&["keys", "status", reference.as_str()]);
    assert!(output.status.success());
    assert_eq!(
        String::from_utf8_lossy(&output.stdout).trim(),
        "mcp:github:pat not set"
    );
    assert!(!profile.paths.config().join("config.toml").exists());
    assert!(
        !profile.paths.runtime().exists(),
        "keys commands must not start the daemon"
    );
}

#[test]
fn unsafe_named_commands_and_bad_inputs_are_refused_without_echoing_them() {
    let profile = Profile::new();
    for arguments in [
        vec!["keys", "set", "mcp:github:pat", "canary-secret-as-arg"],
        vec!["keys", "remove", "mcp:github:pat", "canary-secret-as-arg"],
        vec!["keys", "status", "mcp:github:pat", "canary-secret-as-arg"],
        vec!["keys", "set", "mcp:github:../canary-secret-as-arg"],
        vec![
            "keys",
            "set",
            "mcp:github:pat",
            "--file",
            "canary-secret-as-arg",
            "--from-env",
            "MISSING",
        ],
        vec!["keys", "set", "mcp:github:pat", "--file"],
        vec![
            "keys",
            "status",
            "mcp:github:pat",
            "--unknown",
            "canary-secret-as-arg",
        ],
    ] {
        let output = profile.run(&arguments);
        assert!(!output.status.success());
        assert!(!output_text(&output).contains("canary-secret-as-arg"));
        assert!(!output_text(&output).contains(&profile.root.display().to_string()));
    }
    for secret in [
        "canary with spaces".to_owned(),
        "a".repeat(4097),
        "b".repeat(8193),
    ] {
        let output = profile
            .command(&[
                "keys",
                "set",
                "mcp:github:pat",
                "--from-env",
                "JARVIS_MCP_TEST_INPUT",
            ])
            .env("JARVIS_MCP_TEST_INPUT", &secret)
            .output()
            .unwrap_or_else(|error| panic!("command: {error}"));
        assert!(!output.status.success());
        assert!(!output_text(&output).contains(&secret));
    }
    assert!(!profile.paths.config().exists());
}
