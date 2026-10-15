use super::*;

#[tokio::test]
#[ignore = "owner opt-in: MCP_LIVE_GITHUB_BINARY and MCP_LIVE_GITHUB_TOKEN_FILE; makes a read-only GitHub request"]
async fn live_github_v2_0_2_get_me_with_a_profile_owned_pat() {
    use jarvis_mcp_transport::{StdioCommand, ToolBuffer, connect_stdio};
    use std::io::Read;

    let binary = std::env::var("MCP_LIVE_GITHUB_BINARY")
        .unwrap_or_else(|_| panic!("set MCP_LIVE_GITHUB_BINARY"));
    let key_file = std::env::var_os("MCP_LIVE_GITHUB_TOKEN_FILE")
        .unwrap_or_else(|| panic!("set MCP_LIVE_GITHUB_TOKEN_FILE to a private file"));
    let version = std::process::Command::new(&binary)
        .arg("--version")
        .output()
        .unwrap_or_else(|_| panic!("cannot run selected server"));
    assert!(
        version.status.success() && String::from_utf8_lossy(&version.stdout).contains("2.0.2"),
        "use the verified GitHub MCP Server v2.0.2 binary"
    );
    let root = std::env::temp_dir().join(format!("jmcp-live-{}", jarvis_core::scratch_tag()));
    std::fs::create_dir_all(&root).unwrap_or_else(|_| panic!("cannot create throwaway profile"));
    let _scratch = Scratch(root.clone());
    let paths = jarvis_storage::portable_layout(&root).unwrap_or_else(|| panic!("layout"));
    let reference = McpSecretRef::parse("mcp:github:token").unwrap_or_else(|_| panic!("reference"));
    let mut raw = String::new();
    std::fs::File::open(key_file)
        .unwrap_or_else(|_| panic!("cannot open supplied key file"))
        .take(8193)
        .read_to_string(&mut raw)
        .unwrap_or_else(|_| panic!("cannot read supplied key file"));
    assert!(
        mcp_secrets::set(&paths, &reference, &raw).is_ok(),
        "key file is not usable"
    );
    let env = std::collections::BTreeMap::from([(
        "GITHUB_PERSONAL_ACCESS_TOKEN".to_owned(),
        reference.as_str().to_owned(),
    )]);
    let command = StdioCommand::new(&binary, &["stdio", "--read-only", "--tools=get_me"])
        .unwrap_or_else(|_| panic!("command"))
        .with_credentials("github", &env)
        .unwrap_or_else(|_| panic!("mapping"))
        .with_secret_resolver(std::sync::Arc::new(ProfileSecrets(paths)));
    let server = jarvis_mcp::ServerName::new("github").unwrap_or_else(|_| panic!("server"));
    let connection = connect_stdio(&server, &command)
        .await
        .unwrap_or_else(|_| panic!("GitHub MCP handshake failed"));
    let mut buffer = ToolBuffer::new();
    let tools = connection
        .list_tools(&mut buffer)
        .await
        .unwrap_or_else(|_| panic!("GitHub tool discovery failed"));
    assert!(
        tools.iter().any(|tool| tool.name == "get_me"),
        "get_me was not offered"
    );
    let result = connection
        .call_tool("get_me", &serde_json::Map::new())
        .await
        .unwrap_or_else(|_| panic!("GitHub read failed"));
    assert!(
        result.is_success() && !result.text.is_empty(),
        "GitHub did not confirm a nonempty read result"
    );
    assert!(
        !result.text.contains(raw.trim()),
        "credential must not appear in a tool result"
    );
    assert!(
        connection.close().await.is_ok(),
        "GitHub MCP shutdown failed"
    );
}

struct Scratch(std::path::PathBuf);
impl Drop for Scratch {
    fn drop(&mut self) {
        jarvis_core::remove_scratch_dir(&self.0);
    }
}

#[tokio::test]
async fn profile_keys_reach_host_and_test_connection_without_entering_status_or_posture() {
    let mut directory = std::env::current_exe().unwrap_or_else(|error| panic!("exe: {error}"));
    directory.pop();
    if directory.file_name().is_some_and(|name| name == "deps") {
        directory.pop();
    }
    let fixture = directory.join(format!("fixture-peer{}", std::env::consts::EXE_SUFFIX));
    if !fixture.is_file() {
        assert!(
            !std::env::var("ACCEPTANCE_REQUIRE_FIXTURE_PEER").is_ok_and(|value| value == "1"),
            "build fixture-peer with --all-features"
        );
        eprintln!(
            "SKIP: build fixture-peer with --all-features for MCP credential composition test"
        );
        return;
    }
    let root = std::env::temp_dir().join(format!("jmcp-key-{}", jarvis_core::scratch_tag()));
    std::fs::create_dir_all(&root).unwrap_or_else(|error| panic!("scratch: {error}"));
    let _scratch = Scratch(root.clone());
    let paths = jarvis_storage::portable_layout(&root).unwrap_or_else(|| panic!("layout"));
    let reference = McpSecretRef::parse("mcp:github:token")
        .unwrap_or_else(|error| panic!("reference: {error}"));
    let env = std::collections::BTreeMap::from([(
        "GITHUB_PERSONAL_ACCESS_TOKEN".to_owned(),
        reference.as_str().to_owned(),
    )]);
    assert_eq!(states(&paths, &env)[0]["state"], "not set");
    let document = format!(
        "[[servers]]\nname = 'github'\n[servers.transport]\nkind = 'stdio'\nprogram = '{}'\nargs = ['--env-report', 'GITHUB_PERSONAL_ACCESS_TOKEN']\n[servers.env]\nGITHUB_PERSONAL_ACCESS_TOKEN = 'mcp:github:token'\n",
        fixture.display()
    );
    let missing = crate::mcp_host::try_connect(&document, &paths).await;
    assert_eq!(missing["ok"], false);
    mcp_secrets::set(&paths, &reference, "synthetic-profile-credential-21873")
        .unwrap_or_else(|error| panic!("set: {error}"));
    let metadata = serde_json::to_string(&states(&paths, &env))
        .unwrap_or_else(|error| panic!("status: {error}"));
    assert!(metadata.contains("set"));
    assert!(!metadata.contains("synthetic-profile-credential"));
    assert!(!metadata.contains(&paths.config().display().to_string()));
    let tested = crate::mcp_host::try_connect(&document, &paths).await;
    assert_eq!(tested["ok"], true, "{tested}");
    assert!(!tested.to_string().contains("synthetic-profile-credential"));
    let host = crate::mcp_host::compose_host_with_paths(Some(&document), &paths)
        .await
        .unwrap_or_else(|error| panic!("compose: {error}"))
        .unwrap_or_else(|| panic!("host"));
    assert_eq!(host.reachable_servers(), 1);
    assert!(
        host.definitions()
            .iter()
            .all(|definition| definition.approval() == jarvis_tools::ApprovalPolicy::Ask)
    );
    assert!(host.close().await.is_ok());
    mcp_secrets::remove(&paths, &reference).unwrap_or_else(|error| panic!("remove: {error}"));
    assert_eq!(states(&paths, &env)[0]["state"], "not set");
    assert_eq!(
        crate::mcp_host::try_connect(&document, &paths).await["ok"],
        false
    );
}
