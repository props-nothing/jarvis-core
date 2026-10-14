//! The ACP client against a scripted fake agent (`examples/fake_acp_agent.rs`): the handshake, a turn, permission requests, cancellation
//! and every way an agent can misbehave.

use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;

use super::*;

fn fake_agent() -> PathBuf {
    let executable = std::env::current_exe().unwrap_or_else(|error| panic!("{error}"));
    let debug = executable
        .parent()
        .and_then(std::path::Path::parent)
        .unwrap_or_else(|| panic!("the test binary sits in target/<profile>/deps"));
    let path = debug
        .join("examples")
        .join(format!("fake_acp_agent{}", std::env::consts::EXE_SUFFIX));
    assert!(
        path.is_file(),
        "the fake agent is built with the tests: {}",
        path.display()
    );
    path
}

fn spawn(script: &str) -> Child {
    tokio::process::Command::new(fake_agent())
        .env("FAKE_ACP", script)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .unwrap_or_else(|error| panic!("the fake agent could not be started: {error}"))
}

fn quick() -> Limits {
    Limits {
        handshake: Duration::from_secs(20),
        turn: Duration::from_secs(20),
        cancel_grace: Duration::from_millis(400),
        ..Limits::default()
    }
}

async fn open(script: &str, limits: Limits) -> Result<Connection, AcpError> {
    open_signing_in(script, limits, None).await
}

async fn open_signing_in(
    script: &str,
    limits: Limits,
    sign_in: Option<&str>,
) -> Result<Connection, AcpError> {
    Connection::open(spawn(script), &std::env::temp_dir(), limits, sign_in).await
}

async fn opened(script: &str) -> Connection {
    open(script, quick())
        .await
        .unwrap_or_else(|error| panic!("the handshake must succeed: {error}"))
}

struct Fixed(bool);

#[async_trait]
impl PermissionPolicy for Fixed {
    async fn decide(&self, request: &PermissionRequest) -> Decision {
        Decision {
            allow: self.0,
            reason: format!(
                "fixed answer for a {} request on {:?}",
                request.kind, request.paths
            ),
        }
    }
}

#[tokio::test]
async fn progress_arrives_before_the_prompt_finishes_and_cancellation_still_works() {
    let mut connection = opened("permission:read:/work/a.txt,wait").await;
    let (sender, mut updates) = mpsc::channel(4);
    connection.set_activity_sender(sender);
    let cancel = Arc::new(Notify::new());
    let trigger = Arc::clone(&cancel);
    let worker = tokio::spawn(async move {
        let report = connection.prompt("read", &Fixed(true), &cancel).await;
        connection.close().await;
        report
    });
    let update = tokio::time::timeout(Duration::from_secs(3), updates.recv())
        .await
        .unwrap_or_else(|error| panic!("progress timeout: {error}"))
        .unwrap_or_else(|| panic!("progress channel closed"));
    assert_eq!(update.sequence, 1);
    assert_eq!(update.tool.kind, "read");
    assert_eq!(update.tool.status, "pending");
    assert!(!worker.is_finished(), "progress must precede completion");
    trigger.notify_one();
    let report = worker
        .await
        .unwrap_or_else(|error| panic!("worker: {error}"));
    assert_eq!(report.ended, Ended::Cancelled);
}

#[tokio::test]
async fn progress_backpressure_does_not_block_the_prompt() {
    let mut connection = opened("chat").await;
    let (sender, _updates) = mpsc::channel(1);
    connection.set_activity_sender(sender);
    let report = connection
        .prompt("read", &Fixed(false), &Notify::new())
        .await;
    assert_eq!(report.ended, Ended::Finished(StopReason::EndTurn));
    assert_eq!(report.activity_updates, 2);
    assert_eq!(report.activity_dropped, 1);
    connection.close().await;
}

#[tokio::test]
async fn progress_is_session_scoped_and_never_exposes_internal_reasoning() {
    let mut connection = opened("progress,wait").await;
    let (sender, mut updates) = mpsc::channel(8);
    connection.set_activity_sender(sender);
    let cancel = Arc::new(Notify::new());
    let trigger = Arc::clone(&cancel);
    let worker = tokio::spawn(async move {
        let report = connection.prompt("read", &Fixed(false), &cancel).await;
        connection.close().await;
        report
    });
    let first = tokio::time::timeout(Duration::from_secs(3), updates.recv())
        .await
        .unwrap_or_else(|error| panic!("progress timeout: {error}"))
        .unwrap_or_else(|| panic!("progress closed"));
    let second = tokio::time::timeout(Duration::from_secs(3), updates.recv())
        .await
        .unwrap_or_else(|error| panic!("progress timeout: {error}"))
        .unwrap_or_else(|| panic!("progress closed"));
    assert_eq!(first.sequence, 1);
    assert_eq!(first.tool.title, "Read config");
    assert_eq!(second.sequence, 2);
    assert_eq!(second.tool.title, first.tool.title);
    assert_eq!(second.tool.paths, first.tool.paths);
    assert!(second.tool.paths[0].ends_with("a.txt"));
    assert_eq!(second.tool.status, "in_progress");
    assert!(!format!("{first:?}{second:?}").contains("private"));
    assert!(matches!(
        updates.try_recv(),
        Err(mpsc::error::TryRecvError::Empty)
    ));
    trigger.notify_one();
    let report = worker
        .await
        .unwrap_or_else(|error| panic!("worker: {error}"));
    assert_eq!(report.ended, Ended::Cancelled);
    assert_eq!(report.activity_updates, 2);
    assert!(!report.message.contains("private thought"));
}

#[tokio::test]
async fn progress_full_queue_does_not_prevent_cancellation() {
    let mut connection = opened("progress,wait").await;
    let (sender, _updates) = mpsc::channel(1);
    connection.set_activity_sender(sender.clone());
    let cancel = Arc::new(Notify::new());
    let trigger = Arc::clone(&cancel);
    let worker = tokio::spawn(async move {
        let report = connection.prompt("read", &Fixed(false), &cancel).await;
        connection.close().await;
        report
    });
    tokio::time::timeout(Duration::from_secs(3), async {
        while sender.capacity() != 0 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap_or_else(|error| panic!("no progress: {error}"));
    trigger.notify_one();
    let report = tokio::time::timeout(Duration::from_secs(3), worker)
        .await
        .unwrap_or_else(|error| panic!("cancellation blocked: {error}"))
        .unwrap_or_else(|error| panic!("worker: {error}"));
    assert_eq!(report.ended, Ended::Cancelled);
}

#[test]
fn progress_preserves_partial_metadata_and_excludes_private_payloads() {
    let mut report = Report::new();
    let limits = Limits::default();
    let update = |value| json!({"params": {"update": value}});
    assert!(note_update(&update(json!({"sessionUpdate":"agent_thought_chunk", "content":{"type":"text", "text":"private thought"}})), &mut report, &limits).is_none());
    let first = update(
        json!({"sessionUpdate":"tool_call", "toolCallId":"read-1", "title":"Read config", "kind":"read", "status":"pending", "locations":[{"path":"/work/config.toml"}], "rawInput":{"secret":"private input"}, "rawOutput":"private output", "content":[{"type":"text","text":"private content"}]}),
    );
    assert!(note_update(&first, &mut report, &limits).is_some());
    let changed = note_update(&update(json!({"sessionUpdate":"tool_call_update", "toolCallId":"read-1", "status":"completed"})), &mut report, &limits)
        .unwrap_or_else(|| panic!("a tool snapshot"));
    assert_eq!(changed.title, "Read config");
    assert_eq!(changed.kind, "read");
    assert_eq!(changed.paths, ["/work/config.toml"]);
    assert_eq!(changed.status, "completed");
    assert!(!format!("{changed:?}").contains("private"));
    assert!(report.message.is_empty());
}

#[tokio::test]
async fn a_turn_streams_its_words_and_tool_calls_into_the_report() {
    let mut connection = opened("chat").await;
    assert_eq!(connection.agent().name, "fake-agent");
    let report = connection
        .prompt("do the thing", &Fixed(false), &Notify::new())
        .await;
    assert_eq!(report.ended, Ended::Finished(StopReason::EndTurn));
    assert_eq!(report.message, "Hello world");
    assert_eq!(report.tools.len(), 1);
    assert_eq!(report.tools[0].title, "read a file");
    assert_eq!(
        report.tools[0].status, "completed",
        "the update changed the status of the same call"
    );
    assert_eq!(report.updates, 4);
    assert!(report.permissions.is_empty());
    connection.close().await;
}

#[tokio::test]
async fn an_agent_that_speaks_another_version_is_refused_by_name() {
    let error = open("version:2", quick()).await.err();
    assert!(
        matches!(error, Some(AcpError::UnsupportedVersion { offered: 2 })),
        "{error:?}"
    );
    assert!(
        error
            .map(|error| error.to_string())
            .unwrap_or_default()
            .contains("version 2")
    );
}

#[tokio::test]
async fn an_agent_that_needs_a_sign_in_says_so() {
    let error = open("auth", quick()).await.err();
    match &error {
        Some(AcpError::AuthRequired { message, methods }) => {
            assert!(message.contains("sign in"), "{message}");
            // Only the kind the agent handles itself is offered; the terminal kind is not something this client does.
            assert_eq!(methods.len(), 1, "{methods:?}");
            assert_eq!(methods[0].id, "fake-login");
            assert_eq!(methods[0].name, "Fake login");
        }
        other => panic!("{other:?}"),
    }
}

/// With a method named, a sign-in is done only because the agent asked for one, and the session is then made.
#[tokio::test]
async fn a_named_sign_in_method_is_used_when_the_agent_asks_and_the_session_follows() {
    let mut connection = open_signing_in("auth", quick(), Some("fake-login"))
        .await
        .unwrap_or_else(|error| panic!("the sign-in must work: {error}"));
    assert_eq!(connection.auth_methods().len(), 1);
    assert!(connection.signed_in());
    let report = connection.prompt("go", &Fixed(false), &Notify::new()).await;
    assert_eq!(report.ended, Ended::Finished(StopReason::EndTurn));
    connection.close().await;

    // An agent that needs nothing is never sent `authenticate`: the named method is unused and the handshake works as before.
    let connection = open_signing_in("chat", quick(), Some("fake-login"))
        .await
        .unwrap_or_else(|error| panic!("{error}"));
    assert!(
        !connection.signed_in(),
        "nothing was asked, so no sign-in was done"
    );
    connection.close().await;
}

#[tokio::test]
async fn a_sign_in_method_the_agent_does_not_offer_is_refused_by_name() {
    let error = open_signing_in("auth", quick(), Some("terminal-only"))
        .await
        .err();
    assert!(
        matches!(&error, Some(AcpError::UnknownAuthMethod { asked, methods }) if asked == "terminal-only" && methods.len() == 1),
        "{error:?}"
    );
    let error = open_signing_in("auth", quick(), Some("nope")).await.err();
    assert!(matches!(error, Some(AcpError::UnknownAuthMethod { .. })));
}

/// Some agents (the Codex adapter among them) make the session without asking and only refuse at the first prompt. With a method chosen,
/// the sign-in is done then and the prompt asked again; without one, the turn fails with the agent's own words.
#[tokio::test]
async fn an_agent_that_asks_for_a_sign_in_at_the_prompt_is_signed_in_once_and_asked_again() {
    let mut connection = open_signing_in("auth_prompt", quick(), Some("fake-login"))
        .await
        .unwrap_or_else(|error| panic!("the session is made without asking: {error}"));
    assert!(!connection.signed_in());
    let report = connection.prompt("go", &Fixed(false), &Notify::new()).await;
    assert_eq!(report.ended, Ended::Finished(StopReason::EndTurn));
    assert!(connection.signed_in(), "the sign-in happened at the prompt");
    connection.close().await;

    let mut connection = open("auth_prompt", quick())
        .await
        .unwrap_or_else(|error| panic!("{error}"));
    let report = connection.prompt("go", &Fixed(false), &Notify::new()).await;
    assert!(
        matches!(&report.ended, Ended::Failed(reason) if reason.contains("sign in first")),
        "{:?}",
        report.ended
    );
    connection.close().await;
}

/// The Sign in button signs in whether or not the agent has asked, with a method the agent offered.
#[tokio::test]
async fn signing_in_on_request_works_and_only_with_an_offered_method() {
    let mut connection = open("auth_prompt", quick())
        .await
        .unwrap_or_else(|error| panic!("{error}"));
    let unknown = connection.authenticate("terminal-only").await.err();
    assert!(
        matches!(unknown, Some(AcpError::UnknownAuthMethod { .. })),
        "{unknown:?}"
    );
    connection
        .authenticate("fake-login")
        .await
        .unwrap_or_else(|error| panic!("{error}"));
    assert!(connection.signed_in());
    let report = connection.prompt("go", &Fixed(false), &Notify::new()).await;
    assert_eq!(report.ended, Ended::Finished(StopReason::EndTurn));
    connection.close().await;
}

#[tokio::test]
async fn a_sign_in_the_agent_refuses_ends_the_handshake_with_its_words() {
    let error = open_signing_in("auth_never", quick(), Some("fake-login"))
        .await
        .err();
    assert!(
        matches!(&error, Some(AcpError::Rejected { method: "authenticate", message, .. }) if message.contains("login failed")),
        "{error:?}"
    );
}

#[tokio::test]
async fn a_handshake_that_runs_out_of_time_is_reported_not_waited_for() {
    let limits = Limits {
        handshake: Duration::ZERO,
        ..quick()
    };
    let error = open("chat", limits).await.err();
    assert!(
        matches!(error, Some(AcpError::Handshake { .. })),
        "{error:?}"
    );
}

/// The policy's yes is an allow-once, never the allow-always the agent also offered, which would remember the choice.
#[tokio::test]
async fn an_allowance_is_one_call_only_and_a_refusal_is_a_refusal() {
    let mut connection = opened("permission:edit:/work/a.txt").await;
    let allowed = connection
        .prompt("edit", &Fixed(true), &Notify::new())
        .await;
    assert_eq!(allowed.message, "permission outcome: allow-once");
    assert_eq!(allowed.permissions.len(), 1);
    assert!(allowed.permissions[0].allowed);
    assert_eq!(allowed.permissions[0].kind, "edit");
    assert!(allowed.permissions[0].reason.contains("/work/a.txt"));
    connection.close().await;

    let mut connection = opened("permission:execute").await;
    let refused = connection
        .prompt("run", &Fixed(false), &Notify::new())
        .await;
    assert_eq!(refused.message, "permission outcome: reject-once");
    assert!(!refused.permissions[0].allowed);
    assert_eq!(refused.ended, Ended::Finished(StopReason::EndTurn));
    connection.close().await;
}

#[test]
fn with_no_allow_once_on_offer_an_allowance_is_not_one() {
    let only_always = [PermissionOption {
        id: "always".to_owned(),
        name: "Always".to_owned(),
        kind: OptionKind::AllowAlways,
    }];
    assert!(pick_option(&only_always, true).is_none());
    assert!(pick_option(&only_always, false).is_none());
    let reject_always = [PermissionOption {
        id: "never".to_owned(),
        name: "Never".to_owned(),
        kind: OptionKind::RejectAlways,
    }];
    assert_eq!(
        pick_option(&reject_always, false).map(|option| option.id.as_str()),
        Some("never")
    );
    assert!(pick_option(&[], true).is_none());
}

#[tokio::test]
async fn a_cancelled_turn_ends_as_cancelled_when_the_agent_answers() {
    let mut connection = opened("wait").await;
    let cancel = Arc::new(Notify::new());
    let trigger = Arc::clone(&cancel);
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(150)).await;
        trigger.notify_one();
    });
    let report = connection.prompt("slow", &Fixed(false), &cancel).await;
    assert_eq!(report.ended, Ended::Cancelled);
    assert_eq!(report.message, "working");
    connection.close().await;
}

#[tokio::test]
async fn an_agent_that_ignores_cancel_is_given_a_grace_period_and_then_the_end_is_reported_anyway()
{
    let mut connection = opened("hang").await;
    let cancel = Notify::new();
    cancel.notify_one();
    let started = Instant::now();
    let report = connection.prompt("stuck", &Fixed(false), &cancel).await;
    assert_eq!(report.ended, Ended::Cancelled);
    assert!(
        started.elapsed() < Duration::from_secs(10),
        "the grace period bounds the wait"
    );
    connection.close().await;
}

#[tokio::test]
async fn a_turn_that_runs_past_its_limit_is_cancelled_and_says_it_timed_out() {
    let mut connection = open(
        "hang",
        Limits {
            turn: Duration::from_millis(300),
            ..quick()
        },
    )
    .await
    .unwrap_or_else(|error| panic!("{error}"));
    let report = connection
        .prompt("stuck", &Fixed(false), &Notify::new())
        .await;
    assert_eq!(report.ended, Ended::TimedOut);
    connection.close().await;
}

#[tokio::test]
async fn a_crash_is_reported_with_the_exit_code() {
    let mut connection = opened("crash").await;
    let report = connection
        .prompt("boom", &Fixed(false), &Notify::new())
        .await;
    assert_eq!(report.ended, Ended::AgentExited(Some(3)));
}

#[tokio::test]
async fn a_few_stray_lines_are_counted_and_many_mean_it_is_not_speaking_acp() {
    let mut connection = opened("junk:5").await;
    let report = connection.prompt("x", &Fixed(false), &Notify::new()).await;
    assert_eq!(report.ended, Ended::Finished(StopReason::EndTurn));
    assert_eq!(report.stray_lines, 5);
    connection.close().await;

    let mut connection = opened("junk:60").await;
    let report = connection.prompt("x", &Fixed(false), &Notify::new()).await;
    assert!(
        matches!(&report.ended, Ended::Failed(reason) if reason.contains("not speaking ACP")),
        "{:?}",
        report.ended
    );
}

#[tokio::test]
async fn a_line_over_the_limit_ends_the_turn_without_reading_it_all() {
    let mut connection = opened("longline").await;
    let report = connection.prompt("x", &Fixed(false), &Notify::new()).await;
    assert!(
        matches!(&report.ended, Ended::Failed(reason) if reason.contains("longer than")),
        "{:?}",
        report.ended
    );
}

/// Only the permission method was advertised, so a request for anything else (the file and terminal methods among them) is refused.
#[tokio::test]
async fn a_request_for_a_method_that_was_not_advertised_is_refused() {
    for method in ["fs/read_text_file", "fs/write_text_file", "terminal/create"] {
        let mut connection = opened(&format!("request:{method}")).await;
        let report = connection.prompt("x", &Fixed(true), &Notify::new()).await;
        assert!(
            report
                .message
                .contains(&format!("request {method} refused: true")),
            "{method}: {report:?}"
        );
        connection.close().await;
    }
}

#[tokio::test]
async fn an_update_variant_nobody_knows_is_counted_and_ignored() {
    let mut connection = opened("unknown").await;
    let report = connection.prompt("x", &Fixed(false), &Notify::new()).await;
    assert_eq!(report.ended, Ended::Finished(StopReason::EndTurn));
    assert!(report.updates >= 5);
    connection.close().await;
}

#[test]
fn what_is_kept_of_a_long_turn_is_bounded() {
    let limits = Limits {
        max_message_chars: 50,
        max_tool_notes: 3,
        ..Limits::default()
    };
    let mut report = Report::new();
    for number in 0..10 {
        let chunk = json!({"params": {"update": {"sessionUpdate": "agent_message_chunk", "content": {"type": "text", "text": format!("part {number} of the story. ")}}}});
        note_update(&chunk, &mut report, &limits);
        let call = json!({"params": {"update": {"sessionUpdate": "tool_call", "toolCallId": format!("c{number}"), "title": "t"}}});
        note_update(&call, &mut report, &limits);
    }
    assert_eq!(report.message.chars().count(), 50);
    assert!(report.message_cut);
    assert!(
        report.message.ends_with("part 9 of the story. "),
        "the end is kept: {}",
        report.message
    );
    assert_eq!(report.tools.len(), 3);
    assert_eq!(report.tools_dropped, 7);
    assert_eq!(report.updates, 20);
}
