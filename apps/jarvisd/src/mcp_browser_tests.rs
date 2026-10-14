use std::sync::Mutex;

use jarvis_core::{CorrelationId, SystemClock, ToolOutcome, ToolOutcomeRecord, UtcTimestamp};
use jarvis_tools::{
    ActorAuthority, AuthorizationReceipt, AuthorizationReceiptParts, BoundedOutput, IdempotencyKey,
    PolicyRequest, ProviderEvidence, Scope, ScopeSet, TargetAssessment, ToolExecutionRequestParts,
    WorkspacePolicy, evaluate,
};

use super::*;

fn must<T, E: std::fmt::Display>(result: Result<T, E>) -> T {
    result.unwrap_or_else(|error| panic!("{error}"))
}

// Wire shapes checked against the real pinned server's tools/list, not a substitute browser.
fn listings() -> Vec<ToolDefinition> {
    listings_with_strategy(jarvis_mcp::NamingStrategy::Prefixed)
}

fn listings_with_strategy(strategy: jarvis_mcp::NamingStrategy) -> Vec<ToolDefinition> {
    use jarvis_mcp::{McpToolListing, ServerName, ToolEffectPolicy, translate_tool};
    let server = must(ServerName::new("browser"));
    let policy = ToolEffectPolicy::unclassified().with_read_only_tools([
        "browser_navigate",
        "browser_find",
        "browser_snapshot",
    ]);
    [
        (
            "browser_navigate",
            json!({
                "type": "object", "properties": {"url": {"type": "string"}},
                "required": ["url"], "additionalProperties": false
            }),
        ),
        (
            "browser_find",
            json!({
                "type": "object", "properties": {
                    "text": {"type": "string"}, "regex": {"type": "string"},
                    "filename": {"type": "string"}
                }, "additionalProperties": false
            }),
        ),
        (
            "browser_click",
            json!({"type": "object", "properties": {"target": {"type": "string"}}}),
        ),
        (
            "browser_evaluate",
            json!({"type": "object", "properties": {"function": {"type": "string"}}}),
        ),
        (
            "browser_snapshot",
            json!({"type":"object", "properties": {
                "target":{"type":"string"}, "depth":{"type":"number"},
                "boxes":{"type":"boolean"}, "filename":{"type":"string"}
            }, "additionalProperties":false}),
        ),
    ]
    .into_iter()
    .map(|(name, input)| {
        must(translate_tool(
            &server,
            &McpToolListing {
                name,
                title: None,
                description: Some(name),
                input_schema: &input,
                output_schema: None,
            },
            strategy,
            &policy,
        ))
        .definition
    })
    .collect()
}

fn request(definition: &ToolDefinition, arguments: Value) -> ToolExecutionRequest {
    let now = UtcTimestamp::now(&SystemClock);
    let policy = WorkspacePolicy::default().trusting(definition.id().clone());
    let decision = evaluate(&PolicyRequest {
        definition,
        actor: ActorAuthority::active(ScopeSet::single(must(Scope::new("mcp.call")))),
        workspace: &policy,
        available: true,
        target: TargetAssessment::none(),
    });
    assert!(decision.is_allowed(), "{decision:?}");
    let correlation_id = CorrelationId::new();
    let receipt = must(AuthorizationReceipt::new(AuthorizationReceiptParts {
        receipt_id: "0198f000-0000-7000-8000-0000000000b1".to_owned(),
        tool: definition.id().clone(),
        tool_version: definition.version().to_owned(),
        arguments: arguments.clone(),
        intent_hash: must(jarvis_core::CanonicalIntentHash::compute(
            &definition.id().to_string(),
            definition.version(),
            &arguments,
        )),
        policy_version: "browser-test-policy".to_owned(),
        decision,
        approval: None,
        correlation_id,
        issued_at: now,
    }));
    must(ToolExecutionRequest::new(ToolExecutionRequestParts {
        call_id: "0198f000-0000-7000-8000-0000000000b3".to_owned(),
        tool: definition.id().clone(),
        tool_version: definition.version().to_owned(),
        arguments,
        receipt,
        idempotency_key: must(IdempotencyKey::generate()),
        deadline: must(UtcTimestamp::from_unix_nanos(
            now.unix_nanos() + 60_000_000_000,
        )),
        correlation_id,
    }))
}

#[derive(Default)]
struct Recorder(Mutex<Vec<Value>>);

#[async_trait::async_trait]
impl ToolExecutor for Recorder {
    fn adapter_id(&self) -> &'static str {
        "browser-test"
    }
    async fn execute(
        &self,
        request: &ToolExecutionRequest,
    ) -> Result<ToolCallResult, AdapterError> {
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(request.arguments().clone());
        Ok(ToolCallResult::new(
            must(ToolOutcomeRecord::confirmed("mcp:browser/test")),
            Some(must(ProviderEvidence::new("mcp:browser/test"))),
            Some(BoundedOutput::truncating("unchanged result".to_owned())),
            UtcTimestamp::now(&SystemClock),
        ))
    }
}

#[test]
fn browser_contract_preserves_authority_and_unrelated_tools() {
    let before = listings();
    let (after, _) = adapt(before.clone(), Arc::new(Recorder::default()));
    assert_eq!(after.len(), before.len());
    for (old, new) in before.iter().zip(&after) {
        assert_eq!(old.id(), new.id());
        assert_eq!(old.effects(), new.effects());
        assert_eq!(old.risk(), new.risk());
        assert_eq!(old.required_scopes(), new.required_scopes());
        assert_eq!(old.approval(), new.approval());
        assert_eq!(old.retry(), new.retry());
        assert_eq!(old.timeout_seconds(), new.timeout_seconds());
        assert_eq!(old.source(), new.source());
        if contract(&old.id().to_string()).is_none() {
            assert_eq!(old, new);
        }
    }
    assert_eq!(after[0].risk().level(), 0);
    assert_eq!(after[1].risk().level(), 0);
    assert_ne!(after[2].approval(), jarvis_tools::ApprovalPolicy::Auto);
    assert_ne!(after[3].approval(), jarvis_tools::ApprovalPolicy::Auto);
}

#[tokio::test]
async fn browser_literal_find_rejects_regex_empty_and_file_effects_before_execution() {
    let recorder = Arc::new(Recorder::default());
    let (definitions, executor) = adapt(listings(), recorder.clone());
    let find = &definitions[1];
    assert!(!must(find.input_schema().validate(&json!([]))).is_valid());
    for arguments in [
        json!({}),
        json!({"text": ""}),
        json!({"text": " \n\t"}),
        json!({"text": "x".repeat(513)}),
        json!({"regex": "("}),
        json!({"text": "[", "regex": "["}),
        json!({"text": "read", "filename": "outside.md"}),
        json!({"text": "read", "function": "() => 1"}),
        json!({"text": 42}),
    ] {
        assert!(!must(find.input_schema().validate(&arguments)).is_valid());
        assert!(matches!(
            executor.execute(&request(find, arguments)).await,
            Err(AdapterError::RefusedBeforeReaching { .. })
        ));
    }
    assert!(
        recorder
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .is_empty()
    );
    for text in ["[a+b]*(c)? ^$ . /path/", "Straße café", "literal [", "a\nb"] {
        let arguments = json!({"text": text});
        assert!(must(find.input_schema().validate(&arguments)).is_valid());
        let result = must(executor.execute(&request(find, arguments.clone())).await);
        assert_eq!(result.outcome(), ToolOutcome::Confirmed);
        assert_eq!(
            result.output().map(BoundedOutput::content),
            Some("unchanged result")
        );
        assert_eq!(
            recorder
                .0
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .last(),
            Some(&arguments)
        );
    }
}

#[tokio::test]
async fn browser_snapshot_is_inline_and_rejects_file_output_before_execution() {
    let recorder = Arc::new(Recorder::default());
    let (definitions, executor) = adapt(listings(), recorder.clone());
    let snapshot = definitions
        .iter()
        .find(|definition| definition.id().to_string() == SNAPSHOT)
        .unwrap_or_else(|| panic!("snapshot"));
    for arguments in [
        json!({"filename":"page.yml"}),
        json!({"depth":0}),
        json!({"depth":33}),
        json!({"depth":1.5}),
        json!({"target":"x".repeat(2001)}),
    ] {
        assert!(!must(snapshot.input_schema().validate(&arguments)).is_valid());
        assert!(matches!(
            executor.execute(&request(snapshot, arguments)).await,
            Err(AdapterError::RefusedBeforeReaching { .. })
        ));
    }
    assert!(
        recorder
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .is_empty()
    );
    for arguments in [json!({}), json!({"target":"#name","depth":3,"boxes":true})] {
        must(
            executor
                .execute(&request(snapshot, arguments.clone()))
                .await,
        );
        assert_eq!(
            recorder
                .0
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .last(),
            Some(&arguments)
        );
    }
}

#[tokio::test]
async fn browser_navigation_rejects_scripts_files_credentials_and_oversized_urls() {
    let recorder = Arc::new(Recorder::default());
    let (definitions, executor) = adapt(listings(), recorder.clone());
    let navigate = &definitions[0];
    for arguments in [
        json!({}),
        json!({"url": "javascript:alert(1)"}),
        json!({"url": "file:///C:/private"}),
        json!({"url": "data:text/html,hello"}),
        json!({"url": "https://owner:secret@example.invalid"}),
        json!({"url": "https://owner@example.invalid"}),
        json!({"url": "https://"}),
        json!({"url": "https://example.invalid/\n"}),
        json!({"url": format!("https://example.invalid/{}", "a".repeat(8192))}),
        json!({"url": "https://example.invalid", "function": "() => 1"}),
    ] {
        assert!(matches!(
            executor.execute(&request(navigate, arguments)).await,
            Err(AdapterError::RefusedBeforeReaching { .. })
        ));
    }
    assert!(
        recorder
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .is_empty()
    );
    let arguments = json!({"url": "https://example.invalid/read?q=a%2Bb#section"});
    must(
        executor
            .execute(&request(navigate, arguments.clone()))
            .await,
    );
    assert_eq!(
        recorder
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .as_slice(),
        &[arguments]
    );
}

#[tokio::test]
async fn browser_bare_and_hashed_naming_keep_the_same_guards() {
    for strategy in [
        jarvis_mcp::NamingStrategy::Bare,
        jarvis_mcp::NamingStrategy::Hashed,
    ] {
        let recorder = Arc::new(Recorder::default());
        let (definitions, executor) = adapt(listings_with_strategy(strategy), recorder.clone());
        for (definition, arguments) in [
            (&definitions[0], json!({"url": "javascript:alert(1)"})),
            (&definitions[1], json!({"regex": "("})),
        ] {
            assert!(matches!(
                executor.execute(&request(definition, arguments)).await,
                Err(AdapterError::RefusedBeforeReaching { .. })
            ));
        }
        assert!(
            recorder
                .0
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .is_empty()
        );
        let arguments = json!({"text": "literal ["});
        must(
            executor
                .execute(&request(&definitions[1], arguments.clone()))
                .await,
        );
        assert_eq!(
            recorder
                .0
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .as_slice(),
            &[arguments]
        );
    }
}

#[test]
fn browser_remote_mcp_discovery_uses_the_same_literal_schema() {
    let (definitions, _) = adapt(listings(), Arc::new(Recorder::default()));
    let (served, excluded) = must(jarvis_mcp::served_tools_vetting(&definitions, |_| true));
    assert!(excluded.is_empty());
    for definition in &definitions {
        let remote = served
            .iter()
            .find(|tool| tool.name() == definition.id().to_string())
            .unwrap_or_else(|| panic!("a vetted browser tool is served"));
        assert_eq!(remote.input_schema(), definition.input_schema().document());
        assert_eq!(remote.description(), definition.description());
    }
    let find = served
        .iter()
        .find(|tool| tool.name() == FIND)
        .unwrap_or_else(|| panic!("find is served"));
    assert_eq!(find.input_schema()["required"], json!(["text"]));
    assert!(find.input_schema()["properties"].get("regex").is_none());
    assert!(find.input_schema()["properties"].get("filename").is_none());
}

#[test]
fn browser_open_and_fixed_find_are_discoverable_by_intent() {
    let (definitions, _) = adapt(listings(), Arc::new(Recorder::default()));
    let entries: Vec<_> = definitions
        .iter()
        .map(|definition| crate::tool_search::Entry {
            id: definition.id().to_string(),
            description: definition.description().to_owned(),
        })
        .collect();
    let waiting = BTreeMap::from([("browser".to_owned(), entries)]);
    for (query, expected) in [
        ("open website", NAVIGATE),
        ("navigate URL", NAVIGATE),
        ("fixed literal find text", FIND),
    ] {
        let found = crate::tool_search::search_body(&waiting, query);
        assert!(
            found["tools"]
                .as_array()
                .is_some_and(|tools| tools.iter().any(|tool| tool["id"] == expected)),
            "{query}: {found}"
        );
        assert_eq!(found["found_groups"], json!(["browser"]));
    }
}

#[test]
fn browser_contract_only_targets_the_selected_builtin_command() {
    let ability = crate::mcp_abilities::find("browser").unwrap_or_else(|| panic!("browser exists"));
    let body = ability.body(std::path::Path::new("runtime"));
    let resolve = |program: &str| Ok(format!(r"C:\programs\{program}.cmd"));
    let document = must(crate::mcp_admin::add_to_document(None, &body, &resolve));
    assert!(crate::mcp_abilities::has_builtin_browser(&document));
    for edited in [
        document.replace("0.0.83", "0.0.82"),
        document.replace("--isolated", "--owner-mode"),
        document.replace("@playwright/mcp", "@other/browser"),
    ] {
        assert!(!crate::mcp_abilities::has_builtin_browser(&edited));
    }
}

async fn synthetic_page() -> (std::net::SocketAddr, tokio::task::JoinHandle<()>) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let listener = must(tokio::net::TcpListener::bind("127.0.0.1:0").await);
    let address = must(listener.local_addr());
    let http = tokio::spawn(async move {
        loop {
            let Ok((mut stream, _)) = listener.accept().await else {
                break;
            };
            tokio::spawn(async move {
                let mut buffer = [0_u8; 4096];
                let _ = stream.read(&mut buffer).await;
                let body = format!(
                    "<!doctype html><title>Synthetic browser contract</title><h1>Local only</h1><label for=\"name\">Your name</label><input id=\"name\" value=\"before\"><button>Submit</button><p>Literal [a+b]*(c)? $5.00 /check/ Straße café</p>{}<script>document.body.insertAdjacentHTML('beforeend', '<p>Hydrated browser evidence</p>')</script>",
                    "<p>Repeated match for bounded output</p>".repeat(500)
                );
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = stream.write_all(response.as_bytes()).await;
            });
        }
    });
    (address, http)
}

#[allow(clippy::too_many_lines)]
#[tokio::test]
#[ignore = "opt-in: starts the real cached pinned Playwright server and an isolated local browser"]
async fn browser_live_navigation_and_fixed_find_on_synthetic_loopback_page() {
    let (address, http) = synthetic_page().await;
    let root = std::env::current_dir()
        .unwrap_or_else(|error| panic!("{error}"))
        .join("target")
        .join(format!("browser-live-{}", jarvis_core::scratch_tag()));
    must(std::fs::create_dir_all(&root));
    let ability = crate::mcp_abilities::find("browser").unwrap_or_else(|| panic!("browser exists"));
    let body = ability.body(&root);
    let document = must(crate::mcp_admin::add_to_document(
        None,
        &body,
        &crate::mcp_admin::find_program_on_path,
    ));
    let host = must(crate::mcp_host::compose_host(Some(&document)).await)
        .unwrap_or_else(|| panic!("configured browser composes a host"));
    let definitions = host.definitions();
    assert!(host.unreadable().is_empty(), "{:?}", host.unreadable());
    assert_eq!(
        definitions.len(),
        25,
        "the real catalog, including navigation, must survive registration"
    );
    let definition = |id: &str| {
        definitions
            .iter()
            .find(|definition| definition.id().to_string() == id)
            .unwrap_or_else(|| panic!("missing tool {id}"))
    };
    let executor = host
        .adapters
        .iter()
        .find(|(tools, _)| tools.iter().any(|tool| tool.id().to_string() == NAVIGATE))
        .map_or_else(
            || panic!("browser adapter"),
            |(_, executor)| executor.clone(),
        );
    let navigated = must(
        executor
            .execute(&request(
                definition(NAVIGATE),
                json!({"url": format!("http://{address}/")}),
            ))
            .await,
    );
    assert_eq!(navigated.outcome(), ToolOutcome::Confirmed, "{navigated:?}");
    let snapshot = must(
        executor
            .execute(&request(definition(SNAPSHOT), json!({"target":"#name"})))
            .await,
    );
    let text = snapshot
        .output()
        .unwrap_or_else(|| panic!("inline snapshot"))
        .content();
    assert!(
        text.contains("textbox") && text.contains("before") && text.contains("ref="),
        "{text}"
    );
    assert!(text.contains(jarvis_core::FENCE_OPEN));
    let fill = must(executor.execute(&request(definition("mcp.browser.browser_fill_form"), json!({"fields":[{"name":"Your name","type":"textbox","target":"#name","value":"filled-live"}]}))).await);
    assert_eq!(fill.outcome(), ToolOutcome::Confirmed, "{fill:?}");
    let snapshot = must(
        executor
            .execute(&request(definition(SNAPSHOT), json!({"target":"#name"})))
            .await,
    );
    assert!(
        snapshot
            .output()
            .unwrap_or_else(|| panic!("readback"))
            .content()
            .contains("filled-live")
    );
    let typing = must(
        executor
            .execute(&request(
                definition("mcp.browser.browser_type"),
                json!({"target":"#name","text":"typed-live"}),
            ))
            .await,
    );
    assert_eq!(typing.outcome(), ToolOutcome::Confirmed, "{typing:?}");
    let snapshot = must(
        executor
            .execute(&request(definition(SNAPSHOT), json!({"target":"#name"})))
            .await,
    );
    assert!(
        snapshot
            .output()
            .unwrap_or_else(|| panic!("typed readback"))
            .content()
            .contains("typed-live")
    );
    for (text, expected) in [
        ("[a+b]*(c)?", "Found 1 match"),
        ("Literal [", "Found 1 match"),
        ("straße CAFÉ", "Found 1 match"),
        ("Hydrated browser evidence", "Found 1 match"),
        ("not-present-9a739def", "No matches found"),
        ("Repeated match", "Found 500 matches"),
    ] {
        let result = must(
            executor
                .execute(&request(definition(FIND), json!({"text": text})))
                .await,
        );
        assert_eq!(
            result.outcome(),
            ToolOutcome::Confirmed,
            "{text}: {result:?}"
        );
        let output = result
            .output()
            .unwrap_or_else(|| panic!("find returns a result"));
        assert!(
            output.content().contains(expected),
            "{text}: {}",
            output.content()
        );
        assert!(output.content().contains(jarvis_core::FENCE_OPEN));
        assert!(output.content().contains(jarvis_core::FENCE_CLOSE));
        assert!(output.content().len() <= jarvis_tools::MAX_TOOL_OUTPUT_BYTES);
        if text == "Repeated match" {
            assert!(
                output
                    .content()
                    .contains("[the rest of the result was cut]")
            );
        }
    }
    drop(executor);
    must(host.close().await.map_err(|errors| errors.join(", ")));
    http.abort();
    let _ = http.await;
    must(std::fs::remove_dir_all(root));
}
