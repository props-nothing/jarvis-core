use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use jarvis_core::McpSecretRef;
use jarvis_mcp_transport::{McpSecretResolver, SecretUnavailable};

use super::*;

const CANARY: &str = "synthetic-mcp-credential-not-live-837261";

#[derive(Default)]
struct Secrets(Mutex<BTreeMap<String, String>>);

impl Secrets {
    fn set(&self, reference: &str, value: &str) {
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(reference.to_owned(), value.to_owned());
    }
}
impl McpSecretResolver for Secrets {
    fn resolve(&self, reference: &McpSecretRef) -> Result<String, SecretUnavailable> {
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(reference.as_str())
            .cloned()
            .ok_or(SecretUnavailable)
    }
}

fn authenticated(secrets: Arc<Secrets>, extra: &[&str]) -> StdioCommand {
    let mut args = vec![
        "--env-report",
        "GITHUB_PERSONAL_ACCESS_TOKEN",
        "--crash-on",
        "crash",
    ];
    args.extend(extra);
    fixture_command(&args)
        .with_credentials(
            "github",
            &BTreeMap::from([(
                "GITHUB_PERSONAL_ACCESS_TOKEN".to_owned(),
                "mcp:github:token".to_owned(),
            )]),
        )
        .unwrap_or_else(|error| panic!("references: {error}"))
        .with_secret_resolver(secrets)
}

#[tokio::test]
async fn a_secret_reaches_only_its_child_and_its_echo_is_redacted() {
    if fixture_binary_or_skip().is_none() {
        return;
    }
    let secrets = Arc::new(Secrets::default());
    secrets.set("mcp:github:token", CANARY);
    let command = authenticated(secrets, &[]);
    assert!(!format!("{command:?}").contains(CANARY));
    let connection = connect_stdio(&server("github"), &command)
        .await
        .unwrap_or_else(|error| panic!("connect: {error}"));
    assert!(!connection.reported_identity().name.contains(CANARY));
    let mut buffer = ToolBuffer::new();
    let listings = connection
        .list_tools(&mut buffer)
        .await
        .unwrap_or_else(|error| panic!("list: {error}"));
    assert!(
        listings
            .iter()
            .all(|tool| !tool.description.unwrap_or_default().contains(CANARY))
    );
    let error = connection
        .call_tool("credential_error", &serde_json::Map::new())
        .await
        .err()
        .unwrap_or_else(|| panic!("peer error expected"));
    assert!(!format!("{error:?}").contains(CANARY));
    assert!(error.to_string().contains("[REDACTED]"));
    let result = connection
        .call_tool("search", &serde_json::Map::new())
        .await
        .unwrap_or_else(|error| panic!("call: {error}"));
    assert_eq!(result.text, "credential: [REDACTED]");
    assert!(!format!("{result:?}").contains(CANARY));
    let sibling = connect_stdio(
        &server("sibling"),
        &fixture_command(&["--env-report", "GITHUB_PERSONAL_ACCESS_TOKEN"]),
    )
    .await
    .unwrap_or_else(|error| panic!("sibling: {error}"));
    let result = sibling
        .call_tool("search", &serde_json::Map::new())
        .await
        .unwrap_or_else(|error| panic!("sibling call: {error}"));
    assert_eq!(result.text, "credential: not set");
    let _ = connection.close().await;
    let _ = sibling.close().await;
}

#[tokio::test]
async fn a_missing_required_key_prevents_spawn_and_leaves_the_marker_absent() {
    if fixture_binary_or_skip().is_none() {
        return;
    }
    let marker = std::env::temp_dir().join(format!("mcp-unspawned-{}", jarvis_core::scratch_tag()));
    let text = marker.to_string_lossy().into_owned();
    let command = authenticated(Arc::new(Secrets::default()), &["--once", &text]);
    let error = connect_stdio(&server("github"), &command)
        .await
        .err()
        .unwrap_or_else(|| panic!("missing key must fail"));
    assert!(error.to_string().contains("not set or unavailable"));
    assert!(
        !marker.exists(),
        "credential failure must precede launching a program"
    );
    assert!(!format!("{error:?}").contains(CANARY));
}

#[tokio::test]
async fn rotation_and_removal_are_resolved_again_before_restart() {
    if fixture_binary_or_skip().is_none() {
        return;
    }
    let secrets = Arc::new(Secrets::default());
    secrets.set("mcp:github:token", CANARY);
    let command = authenticated(Arc::clone(&secrets), &[]);
    let connection = connect_stdio(&server("github"), &command)
        .await
        .unwrap_or_else(|error| panic!("connect: {error}"));
    assert!(
        connection
            .call_tool("crash", &serde_json::Map::new())
            .await
            .is_err()
    );
    tokio::time::sleep(Duration::from_millis(100)).await;
    secrets.set("mcp:github:token", "synthetic-rotated-credential-329103");
    let result = connection
        .call_tool("search", &serde_json::Map::new())
        .await
        .unwrap_or_else(|error| panic!("restart: {error}"));
    assert_eq!(result.text, "credential: [REDACTED]");
    assert!(!result.text.contains("synthetic-rotated"));
    assert!(
        connection
            .call_tool("crash", &serde_json::Map::new())
            .await
            .is_err()
    );
    tokio::time::sleep(Duration::from_millis(100)).await;
    secrets
        .0
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clear();
    assert!(matches!(
        connection
            .call_tool("search", &serde_json::Map::new())
            .await,
        Err(CallError::NotSent(_))
    ));
    let _ = connection.close().await;
}

#[test]
fn a_credential_child_cannot_print_its_key_into_parent_stderr() {
    if fixture_binary_or_skip().is_none() {
        return;
    }
    let output = std::process::Command::new(
        std::env::current_exe().unwrap_or_else(|error| panic!("exe: {error}")),
    )
    .args([
        "--exact",
        "credentials::a_secret_reaches_only_its_child_and_its_echo_is_redacted",
        "--nocapture",
    ])
    .output()
    .unwrap_or_else(|error| panic!("probe: {error}"));
    assert!(
        output.status.success(),
        "probe: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!String::from_utf8_lossy(&output.stdout).contains(CANARY));
    assert!(!String::from_utf8_lossy(&output.stderr).contains(CANARY));
}
