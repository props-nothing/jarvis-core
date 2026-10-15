use super::*;

#[test]
fn credential_names_are_not_an_arbitrary_child_environment_api() {
    for name in [
        "GITHUB_PERSONAL_ACCESS_TOKEN",
        "API_KEY",
        "CLIENT_SECRET",
        "DB_PASSWORD",
        "TOKEN",
    ] {
        assert!(is_credential_variable(name), "{name}");
    }
    for name in [
        "PATH",
        "NODE_OPTIONS",
        "JARVIS_MODEL_KEY",
        "MCP_SERVER_TOKEN",
        "LD_PRELOAD",
        "DYLD_LIBRARY_PATH",
        "PYTHONPATH",
        "GIT_CONFIG_KEY",
        "NPM_CONFIG_KEY",
        "HTTP_PROXY_KEY",
        "ENV_KEY",
        "A=B_TOKEN",
        "1TOKEN",
        "API_KEY\n",
    ] {
        assert!(!is_credential_variable(name), "{name}");
    }
}

#[test]
fn literal_values_other_server_refs_and_case_collisions_fail_before_resolution() {
    for mappings in [
        BTreeMap::from([("API_KEY".to_owned(), "synthetic-not-a-reference".to_owned())]),
        BTreeMap::from([("API_KEY".to_owned(), "mcp:other:token".to_owned())]),
        BTreeMap::from([
            ("API_KEY".to_owned(), "mcp:github:token".to_owned()),
            ("api_key".to_owned(), "mcp:github:token".to_owned()),
        ]),
        BTreeMap::from([("PATH".to_owned(), "mcp:github:token".to_owned())]),
    ] {
        let error = validate_references("github", &mappings)
            .err()
            .unwrap_or_else(|| panic!("must fail"));
        assert!(!error.contains("synthetic-not-a-reference"));
    }
}

#[test]
fn host_parser_accepts_only_same_server_stdio_references() {
    let document = r"[[servers]]
name = 'github'
[servers.transport]
kind = 'stdio'
program = 'github-mcp-server'
args = ['stdio']
[servers.env]
GITHUB_PERSONAL_ACCESS_TOKEN = 'mcp:github:token'
";
    let config =
        crate::McpHostConfig::parse(document).unwrap_or_else(|error| panic!("parse: {error}"));
    assert_eq!(config.len(), 1);
    assert!(!format!("{config:?}").contains("mcp:github:token"));
    for bad in [
        document.replace("mcp:github:token", "mcp:other:token"),
        document.replace("mcp:github:token", "synthetic-key-value"),
        document.replace("GITHUB_PERSONAL_ACCESS_TOKEN", "NODE_OPTIONS"),
        document.replace(
            "kind = 'stdio'\nprogram = 'github-mcp-server'\nargs = ['stdio']",
            "kind = 'http'\nendpoint = 'https://example.test/mcp'",
        ),
    ] {
        let error = crate::McpHostConfig::parse(&bad)
            .err()
            .unwrap_or_else(|| panic!("must refuse"));
        assert!(!error.to_string().contains("synthetic-key-value"));
    }
}

#[test]
fn known_credentials_are_removed_from_nested_output_and_json_keys() {
    let mut redactor = CredentialRedactor::default();
    redactor.extend(["synthetic-token-123".to_owned()]);
    let mut value = serde_json::json!({ "synthetic-token-123": ["before synthetic-token-123 after", { "token": "synthetic-token-123" }] });
    redactor.value(&mut value);
    assert!(!value.to_string().contains("synthetic-token-123"));
    assert!(value.to_string().contains("[REDACTED]"));
}
