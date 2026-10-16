//! The coding agent presets, how a saved command is resolved and started, and the console's Test and Sign in, against a scripted agent.

use super::*;

struct Scratch(PathBuf);

impl Drop for Scratch {
    fn drop(&mut self) {
        jarvis_core::remove_scratch_dir(&self.0);
    }
}

fn scratch() -> Scratch {
    let tag = jarvis_core::scratch_tag();
    let path = std::env::temp_dir().join(format!("jca-{}", &tag[tag.len() - 12..]));
    std::fs::create_dir_all(&path).unwrap_or_else(|error| panic!("{error}"));
    Scratch(path)
}

/// A search path holding empty files with these program names, so `resolve_program` finds them the way it finds real ones.
fn path_with(directory: &Scratch, programs: &[&str]) -> OsString {
    for program in programs {
        let name = if cfg!(windows) {
            format!("{program}.exe")
        } else {
            (*program).to_owned()
        };
        std::fs::write(directory.0.join(name), b"").unwrap_or_else(|error| panic!("{error}"));
    }
    directory.0.clone().into_os_string()
}

/// The extensions Windows tries for a bare program name, as the daemon passes them.
fn pathext() -> &'static OsStr {
    OsStr::new(".EXE")
}

fn fake_agent() -> PathBuf {
    crate::code_agents::test_agent_executable()
}

fn settings(script: &str) -> AgentSettings {
    AgentSettings {
        command: vec![
            fake_agent().to_string_lossy().into_owned(),
            script.to_owned(),
        ],
        ..AgentSettings::default()
    }
}

#[test]
fn every_preset_can_be_started_and_nothing_floats() {
    let mut seen = std::collections::HashSet::new();
    for preset in &PRESETS {
        assert!(seen.insert(preset.id), "{} is listed twice", preset.id);
        assert!(
            preset.native.is_some() || preset.npx.is_some(),
            "{} cannot be started at all",
            preset.id
        );
        if let Some((program, _)) = preset.native {
            assert!(
                !program.contains(['/', '\\', ' ']),
                "{program} is a bare program name, found on the search path"
            );
        }
        if let Some((package, _)) = preset.npx {
            // A package run through npx is pinned to the registry's version, so what is downloaded is what was read.
            let (name, version) = package.rsplit_once('@').unwrap_or((package, ""));
            assert!(
                !name.is_empty()
                    && version.split('.').count() == 3
                    && version.chars().all(|c| c.is_ascii_digit() || c == '.'),
                "{package} is not pinned to an exact version"
            );
        }
        for name in preset.env {
            assert!(!name.to_ascii_uppercase().starts_with("JARVIS_"), "{name}");
        }
    }
    assert!(seen.contains("codex"), "ChatGPT (Codex) is offered");
    assert!(seen.contains("claude") && seen.contains("copilot") && seen.contains("gemini"));
}

#[test]
fn a_preset_is_started_as_the_installed_program_or_through_npx_or_not_at_all() {
    let codex = PRESETS
        .iter()
        .find(|preset| preset.id == "codex")
        .unwrap_or_else(|| panic!("codex"));
    let opencode = PRESETS
        .iter()
        .find(|preset| preset.id == "opencode")
        .unwrap_or_else(|| panic!("opencode"));

    let nothing = scratch();
    let empty = path_with(&nothing, &[]);
    assert!(launch(codex, &empty, Some(pathext())).is_none());

    let with_npx = scratch();
    let path = path_with(&with_npx, &["npx"]);
    let (command, via) =
        launch(codex, &path, Some(pathext())).unwrap_or_else(|| panic!("npx runs it"));
    assert_eq!(via, "npx");
    assert_eq!(command[..2], ["npx".to_owned(), "-y".to_owned()]);
    assert!(
        command[2].starts_with("@agentclientprotocol/codex-acp@"),
        "{command:?}"
    );
    assert!(
        launch(opencode, &path, Some(pathext())).is_none(),
        "an agent with no package needs its own program"
    );

    let installed = scratch();
    let path = path_with(&installed, &["npx", "codex-acp", "opencode"]);
    let (command, via) =
        launch(codex, &path, Some(pathext())).unwrap_or_else(|| panic!("installed"));
    assert_eq!((via, command), ("installed", vec!["codex-acp".to_owned()]));
    let (command, _) =
        launch(opencode, &path, Some(pathext())).unwrap_or_else(|| panic!("installed"));
    assert_eq!(command, ["opencode", "acp"]);
}

#[test]
fn a_saved_command_is_recognised_as_a_preset_or_called_custom() {
    assert_eq!(
        preset_of(&["copilot".to_owned(), "--acp".to_owned()]),
        Some("copilot")
    );
    assert_eq!(
        preset_of(&["npx", "-y", "@agentclientprotocol/codex-acp@2.2.2"].map(str::to_owned)),
        Some("codex")
    );
    assert_eq!(preset_of(&["my-agent".to_owned()]), None);
    assert_eq!(preset_of(&[]), None);
}

#[test]
fn the_command_is_resolved_or_the_refusal_says_where_to_set_one_up() {
    let none = scratch();
    let nowhere = path_with(&none, &[]);
    let reason = resolve(&[], &nowhere, Some(pathext()))
        .err()
        .unwrap_or_default();
    assert!(reason.contains("no coding agent is set up"), "{reason}");
    assert!(reason.contains("Settings"), "{reason}");

    let reason = resolve(&["no-such-agent-xyz".to_owned()], &nowhere, Some(pathext()))
        .err()
        .unwrap_or_default();
    assert!(reason.contains("no-such-agent-xyz"), "{reason}");

    // With nothing chosen, an installed agent that starts in ACP mode by itself is found; an adapter is never started unasked.
    let installed = scratch();
    let path = path_with(&installed, &["gemini", "codex-acp"]);
    let (program, arguments) =
        resolve(&[], &path, Some(pathext())).unwrap_or_else(|error| panic!("{error}"));
    assert!(program.to_string_lossy().contains("gemini"), "{program:?}");
    assert_eq!(arguments, ["--acp"]);

    let only_adapter = scratch();
    let path = path_with(&only_adapter, &["codex-acp", "npx"]);
    assert!(
        resolve(&[], &path, Some(pathext())).is_err(),
        "an adapter is only started when the owner chose it"
    );
}

#[test]
fn the_agent_gets_the_safe_environment_plus_only_the_names_the_owner_listed() {
    let environment = environment(&[
        "PATH".to_owned(),
        "JARVIS_SECRET_THAT_DOES_NOT_EXIST".to_owned(),
    ]);
    assert!(environment.contains_key("PATH"));
    assert!(
        environment
            .keys()
            .all(|name| !name.to_ascii_uppercase().starts_with("JARVIS_"))
    );
    assert!(!environment.contains_key("JARVIS_SECRET_THAT_DOES_NOT_EXIST"));
    // Nothing else of the daemon's own environment leaks in: no variable that looks like a key is there unless it was named.
    assert!(environment.keys().all(|name| {
        let upper = name.to_ascii_uppercase();
        !(upper.ends_with("_KEY") || upper.ends_with("_TOKEN") || upper.ends_with("_SECRET"))
    }));
}

#[test]
fn what_went_wrong_is_said_in_words_the_owner_can_act_on() {
    let methods = vec![
        AuthMethod {
            id: "chatgpt".to_owned(),
            name: "ChatGPT login".to_owned(),
            description: String::new(),
        },
        AuthMethod {
            id: "key".to_owned(),
            name: "API key".to_owned(),
            description: String::new(),
        },
    ];
    let said = describe_error(&AcpError::AuthRequired {
        message: "sign in".to_owned(),
        methods,
    });
    assert!(said.contains("ChatGPT login, API key"), "{said}");
    assert!(said.contains("Sign in"), "{said}");
    let said = describe_error(&AcpError::AuthRequired {
        message: "sign in".to_owned(),
        methods: Vec::new(),
    });
    assert!(said.contains("terminal"), "{said}");
    let said = describe_error(&AcpError::Handshake {
        reason: "gone".to_owned(),
        stderr_tail: "npm ERR! 404".to_owned(),
    });
    assert!(said.contains("npm ERR! 404"), "{said}");
    let said = describe_error(&AcpError::Handshake {
        reason: "slow".to_owned(),
        stderr_tail: String::new(),
    });
    assert!(said.contains("downloading"), "{said}");
}

#[tokio::test]
async fn the_test_starts_the_agent_and_reports_who_answered() {
    let found = probe(&settings("chat"), None, Duration::from_secs(30))
        .await
        .unwrap_or_else(|failure| panic!("{}", failure.message));
    assert_eq!(found.name, "fake-agent");
    assert_eq!(found.version, "0.0.1");
    assert!(!found.signed_in);
}

#[tokio::test]
async fn a_missing_program_is_reported_without_starting_anything() {
    let missing = AgentSettings {
        command: vec!["no-such-agent-xyz".to_owned()],
        ..AgentSettings::default()
    };
    let failure = probe(&missing, None, Duration::from_secs(5))
        .await
        .err()
        .unwrap_or_else(|| panic!("must fail"));
    assert!(
        failure.message.contains("no-such-agent-xyz"),
        "{}",
        failure.message
    );
    assert!(!failure.sign_in_needed);
}

/// An agent that wants a sign-in is told apart from one that is broken, and its own sign-in methods are offered; choosing one signs in
/// and the agent is then ready. The terminal kind of method is not offered, because JARVIS does not run terminals.
#[tokio::test]
async fn an_agent_that_wants_a_sign_in_offers_its_methods_and_the_chosen_one_works() {
    let failure = probe(&settings("auth"), None, Duration::from_secs(30))
        .await
        .err()
        .unwrap_or_else(|| panic!("must ask"));
    assert!(failure.sign_in_needed, "{}", failure.message);
    assert_eq!(failure.methods.len(), 1);
    assert_eq!(failure.methods[0].id, "fake-login");
    assert!(
        failure.message.contains("Fake login"),
        "{}",
        failure.message
    );

    let found = probe(
        &settings("auth"),
        Some("fake-login"),
        Duration::from_secs(30),
    )
    .await
    .unwrap_or_else(|failure| panic!("{}", failure.message));
    assert!(found.signed_in);

    // An agent that does not ask until the first prompt (the Codex adapter) starts fine, says so honestly, and still signs in on request.
    let passive = probe(&settings("auth_prompt"), None, Duration::from_secs(30))
        .await
        .unwrap_or_else(|failure| panic!("{}", failure.message));
    assert!(!passive.signed_in);
    assert_eq!(passive.methods.len(), 1);
    let reply = probe_reply(Ok(passive), None);
    let body = axum::body::to_bytes(reply.into_body(), 4096)
        .await
        .unwrap_or_default();
    let body = String::from_utf8_lossy(&body);
    assert!(
        body.contains("cannot say whether you are signed in"),
        "{body}"
    );
    let found = probe(
        &settings("auth_prompt"),
        Some("fake-login"),
        Duration::from_secs(30),
    )
    .await
    .unwrap_or_else(|failure| panic!("{}", failure.message));
    assert!(
        found.signed_in,
        "signing in on request works even though the agent never asked"
    );
    let failure = probe(
        &settings("auth_never"),
        Some("fake-login"),
        Duration::from_secs(30),
    )
    .await
    .err()
    .unwrap_or_else(|| panic!("must fail"));
    assert!(
        failure.message.contains("login failed"),
        "{}",
        failure.message
    );
    assert!(!failure.sign_in_needed);
}

/// The probe leaves nothing behind, whether the agent answered or failed: its temporary folder is removed.
#[tokio::test]
async fn a_test_leaves_no_folder_behind() {
    let root = scratch();
    for (script, sign_in, succeeds) in [
        ("chat", None, true),
        ("auth", None, false),
        ("version:2", None, false),
        ("auth_never", Some("fake-login"), false),
        ("auth_prompt", Some("unknown-method"), false),
        ("auth,auth_wait", Some("fake-login"), false),
        ("auth_prompt,auth_wait", Some("fake-login"), false),
    ] {
        let result = probe_under(
            &root.0,
            &settings(script),
            sign_in,
            Duration::from_secs(if script.contains("auth_wait") { 1 } else { 30 }),
        )
        .await;
        assert_eq!(result.is_ok(), succeeds, "unexpected outcome for {script}");
        if let Err(failure) = result {
            if script.contains("auth_wait") {
                assert!(
                    failure.message.contains("did not complete the handshake"),
                    "{script} must exercise the sign-in deadline: {}",
                    failure.message
                );
            }
            assert!(
                !failure.message.contains("cleanup failed")
                    && !failure.message.contains("could not be removed"),
                "{script}: {}",
                failure.message
            );
        }
        let left = std::fs::read_dir(&root.0)
            .unwrap_or_else(|error| panic!("cannot inspect scratch root: {error}"))
            .count();
        assert_eq!(left, 0, "{script} left a probe folder behind");
    }
}

#[tokio::test]
async fn probe_folder_collisions_preserve_existing_directories_and_files() {
    let root = scratch();
    let directory_id = jarvis_core::SessionId::new();
    let file_id = jarvis_core::SessionId::new();
    let fresh_id = jarvis_core::SessionId::new();
    let directory = root.0.join(format!("jarvis-agent-test-{directory_id}"));
    let file = root.0.join(format!("jarvis-agent-test-{file_id}"));
    std::fs::create_dir(&directory).unwrap_or_else(|error| panic!("{error}"));
    std::fs::write(directory.join("sentinel"), b"existing directory")
        .unwrap_or_else(|error| panic!("{error}"));
    std::fs::write(&file, b"existing file").unwrap_or_else(|error| panic!("{error}"));
    let mut candidates = [directory_id, file_id, fresh_id].into_iter();
    let created = create_probe_folder(&root.0, || {
        candidates
            .next()
            .unwrap_or_else(|| panic!("the third candidate must be fresh"))
    })
    .unwrap_or_else(|error| panic!("a collision must be retried: {error}"));
    assert_eq!(
        created,
        root.0.join(format!("jarvis-agent-test-{fresh_id}"))
    );
    assert!(candidates.next().is_none());
    remove_probe_folder(&created)
        .await
        .unwrap_or_else(|error| panic!("{error}"));
    assert!(!created.exists());
    assert_eq!(
        std::fs::read(directory.join("sentinel")).unwrap_or_else(|error| panic!("{error}")),
        b"existing directory"
    );
    assert_eq!(
        std::fs::read(&file).unwrap_or_else(|error| panic!("{error}")),
        b"existing file"
    );
}

#[test]
fn probe_folder_collisions_are_bounded_and_never_adopt_the_occupied_path() {
    let root = scratch();
    let id = jarvis_core::SessionId::new();
    let occupied = root.0.join(format!("jarvis-agent-test-{id}"));
    std::fs::create_dir(&occupied).unwrap_or_else(|error| panic!("{error}"));
    std::fs::write(occupied.join("sentinel"), b"keep").unwrap_or_else(|error| panic!("{error}"));
    let mut attempts = 0;
    let error = create_probe_folder(&root.0, || {
        attempts += 1;
        id
    })
    .err()
    .unwrap_or_else(|| panic!("occupied candidates must be refused"));
    assert_eq!(error.kind(), std::io::ErrorKind::AlreadyExists);
    assert_eq!(attempts, PROBE_FOLDER_ATTEMPTS);
    assert_eq!(
        std::fs::read(occupied.join("sentinel")).unwrap_or_else(|error| panic!("{error}")),
        b"keep"
    );
}

#[test]
fn a_missing_probe_root_is_reported_immediately_and_never_created() {
    let root = scratch();
    let missing = root.0.join("missing-root");
    let mut attempts = 0;
    let error = create_probe_folder(&missing, || {
        attempts += 1;
        jarvis_core::SessionId::new()
    })
    .err()
    .unwrap_or_else(|| panic!("a missing parent must be refused"));
    assert_eq!(error.kind(), std::io::ErrorKind::NotFound);
    assert_eq!(attempts, 1, "non-collision failures must not be retried");
    assert!(!missing.exists());
}

#[tokio::test]
async fn concurrent_probe_folder_allocation_never_shares_ownership() {
    let root = scratch();
    let first_id = jarvis_core::SessionId::new();
    let barrier = std::sync::Barrier::new(8);
    let folders = std::thread::scope(|scope| {
        let workers: Vec<_> = (0..8)
            .map(|_| {
                scope.spawn(|| {
                    barrier.wait();
                    let mut first = true;
                    create_probe_folder(&root.0, || {
                        if std::mem::replace(&mut first, false) {
                            first_id
                        } else {
                            jarvis_core::SessionId::new()
                        }
                    })
                    .unwrap_or_else(|error| panic!("concurrent allocation: {error}"))
                })
            })
            .collect();
        workers
            .into_iter()
            .map(|worker| {
                worker
                    .join()
                    .unwrap_or_else(|_| panic!("allocation panicked"))
            })
            .collect::<Vec<_>>()
    });
    let unique: std::collections::HashSet<_> = folders.iter().collect();
    assert_eq!(unique.len(), 8, "a scratch directory has exactly one owner");
    for folder in folders {
        remove_probe_folder(&folder)
            .await
            .unwrap_or_else(|error| panic!("{error}"));
    }
    assert_eq!(
        std::fs::read_dir(&root.0)
            .unwrap_or_else(|error| panic!("{error}"))
            .count(),
        0
    );
}

#[test]
fn codex_task_permissions_choose_reviewable_execution_without_full_access() {
    for command in [
        vec!["codex-acp".to_owned()],
        vec![
            "npx".to_owned(),
            "-y".to_owned(),
            "@agentclientprotocol/codex-acp@2.2.2".to_owned(),
        ],
    ] {
        let settings = AgentSettings {
            command,
            ..AgentSettings::default()
        };
        let allowed = task_environment(&settings, true, true);
        assert_eq!(
            allowed.get("INITIAL_AGENT_MODE").and_then(|v| v.to_str()),
            Some("workspace-write")
        );
        assert_eq!(
            allowed.get("npm_config_offline").and_then(|v| v.to_str()),
            Some("false")
        );
        let denied = task_environment(&settings, false, false);
        assert_eq!(
            denied.get("INITIAL_AGENT_MODE").and_then(|v| v.to_str()),
            Some("read-only")
        );
        assert_eq!(
            denied.get("npm_config_offline").and_then(|v| v.to_str()),
            Some("true")
        );
    }
    let custom = task_environment(
        &AgentSettings {
            command: vec!["custom-acp".to_owned()],
            ..AgentSettings::default()
        },
        true,
        true,
    );
    assert!(!custom.contains_key("INITIAL_AGENT_MODE"));
}
