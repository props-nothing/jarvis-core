//! The coding agent presets, how a saved command is resolved and started, and the console's Test and Sign in, against a scripted agent.

use std::process::Command;

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
    static BUILD: std::sync::Once = std::sync::Once::new();
    BUILD.call_once(|| {
        let status = Command::new(option_env!("CARGO").unwrap_or("cargo"))
            .args([
                "build",
                "-q",
                "-p",
                "jarvis-runtimes",
                "--example",
                "fake_acp_agent",
            ])
            .status()
            .unwrap_or_else(|error| panic!("cargo could not be run: {error}"));
        assert!(status.success(), "the fake agent must build");
    });
    let executable = std::env::current_exe().unwrap_or_else(|error| panic!("{error}"));
    let debug = executable
        .parent()
        .and_then(Path::parent)
        .unwrap_or_else(|| panic!("the test binary sits in target/<profile>/deps"));
    debug
        .join("examples")
        .join(format!("fake_acp_agent{}", std::env::consts::EXE_SUFFIX))
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
    let wrong_version = settings("version:2");
    let _ = probe_under(&root.0, &settings("chat"), None, Duration::from_secs(30)).await;
    let _ = probe_under(&root.0, &settings("auth"), None, Duration::from_secs(30)).await;
    let _ = probe_under(&root.0, &wrong_version, None, Duration::from_secs(30)).await;
    let left = std::fs::read_dir(&root.0).map_or(0, Iterator::count);
    assert_eq!(
        left, 0,
        "the folders of an answer and two failures are all removed"
    );
}
