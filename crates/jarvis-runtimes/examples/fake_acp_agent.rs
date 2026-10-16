//! A scripted stand-in for a coding agent, speaking ACP version 1 on standard input and output (`P7-006`).
//!
//! Test fixture only: the behaviour comes from the first argument, or the `FAKE_ACP` environment variable, a comma-separated list of
//! words (`{cwd}` in a word stands for the agent's working directory).
//!
//! * `say:TEXT`: stream TEXT as the agent's words.
//! * `version:N`: answer `initialize` with protocol version N.
//! * `auth`: answer `session/new` with the authentication-required error until `authenticate` is called with `fake-login`
//!   (the agent advertises that method and also a terminal method that clients must ignore).
//! * `auth_never`: the same, but `authenticate` always fails.
//! * `auth_prompt`: `session/new` works, but `session/prompt` is refused with the authentication-required error until `authenticate`
//!   is called (the way the Codex adapter behaves).
//! * `chat`: stream two message chunks and one tool call, then end the turn. (The default.)
//! * `permission:KIND[:PATH]`: ask permission for a tool call of that kind (touching PATH when given) and report the answer.
//! * `write:PATH`: write a file at PATH (an agent's own tool escaping its folder), then end the turn.
//! * `wait`: stream a chunk and then wait for `session/cancel`, answering `cancelled`.
//! * `cancel_exit`, `cancel_error`, `cancel_auth`: wait for cancellation, then exit or return an internal/auth error.
//! * `event_log:PATH`: append only incoming method names to a fixture log, proving that a cancelled prompt is not replayed.
//! * `auth_delay:N`, `reply_delay:N`: delay authentication or the final prompt response by N milliseconds.
//! * `auth_wait`: receive an authentication request without answering it.
//! * `stall_input_after_session`: stop reading stdin once session/new arrives, leaving later writes blocked.
//! * `cancel_permission`: ask permission after cancellation and report the client's outcome.
//! * `hang`: never answer the prompt and ignore `session/cancel`.
//! * `crash`: exit with code 3 when the prompt arrives.
//! * `junk:N`: write N lines that are not JSON before answering.
//! * `longline`: write one line of three megabytes.
//! * `request:METHOD`: send the client a request for METHOD and report whether it was refused.
//! * `stderr`: write a line to the error stream first.
//! * `unknown`: send an update variant nobody knows.
//! * `progress`: public tool snapshots plus private and wrong-session updates that a client must not expose as progress.
//! * `initialize_wait:PATH`: record setup PIDs at PATH and never answer initialization; `initialize_child` adds an idle child.
//! * `idle`: wait without protocol activity (the descendant in startup-cancellation tests).

use std::io::{BufRead, Write};
use std::sync::mpsc;

use serde_json::{Value, json};

fn send(value: &Value) {
    let mut out = std::io::stdout().lock();
    let _ = writeln!(out, "{value}");
    let _ = out.flush();
}

fn update(session: &str, update: Value) {
    let mut message =
        json!({"jsonrpc": "2.0", "method": "session/update", "params": {"sessionId": session}});
    message["params"]["update"] = update;
    send(&message);
}

fn chunk(session: &str, text: &str) {
    update(
        session,
        json!({"sessionUpdate": "agent_message_chunk", "messageId": "m1", "content": {"type": "text", "text": text}}),
    );
}

fn stalled_setup(marker: &str, with_child: bool) {
    let child_pid = with_child.then(|| {
        std::process::Command::new(
            std::env::current_exe().unwrap_or_else(|error| panic!("{error}")),
        )
        .arg("idle")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap_or_else(|error| panic!("{error}"))
        .id()
    });
    let marker_text = json!({"pid":std::process::id(),"child_pid":child_pid}).to_string();
    std::fs::write(marker, marker_text).unwrap_or_else(|error| panic!("{error}"));
}

fn script() -> String {
    let cwd = std::env::current_dir()
        .map(|path| path.to_string_lossy().into_owned())
        .unwrap_or_default();
    std::env::args()
        .nth(1)
        .or_else(|| std::env::var("FAKE_ACP").ok())
        .unwrap_or_else(|| "chat".to_owned())
        .replace("{cwd}", &cwd)
}

fn incoming(event_log: Option<String>, stall_input: bool) -> mpsc::Receiver<Value> {
    let (sender, receiver) = mpsc::channel::<Value>();
    std::thread::spawn(move || {
        let stdin = std::io::stdin();
        for line in stdin.lock().lines().map_while(Result::ok) {
            if let Ok(value) = serde_json::from_str::<Value>(&line) {
                let stall = stall_input && value["method"] == "session/new";
                if let Some(path) = &event_log
                    && let Some(method) = value.get("method").and_then(Value::as_str)
                {
                    let mut log = std::fs::OpenOptions::new()
                        .create(true)
                        .append(true)
                        .open(path)
                        .unwrap_or_else(|error| panic!("fixture log: {error}"));
                    writeln!(log, "{method}")
                        .unwrap_or_else(|error| panic!("fixture log: {error}"));
                }
                if sender.send(value).is_err() {
                    return;
                }
                if stall {
                    loop {
                        std::thread::park();
                    }
                }
            }
        }
    });
    receiver
}

fn main() {
    let script = script();
    let words: Vec<&str> = script.split(',').collect();
    let has = |word: &str| words.contains(&word);
    let value_of = |prefix: &str| {
        words
            .iter()
            .find_map(|word| word.strip_prefix(prefix).map(str::to_owned))
    };
    if has("stderr") {
        eprintln!("fake agent starting");
    }
    if has("idle") {
        loop {
            std::thread::park();
        }
    }
    let receiver = incoming(value_of("event_log:"), has("stall_input_after_session"));

    let session = "sess_fake";
    let needs_sign_in =
        has("auth") || has("auth_never") || has("auth_prompt") || has("cancel_auth");
    let mut signed_in = false;
    while let Ok(message) = receiver.recv() {
        let method = message.get("method").and_then(Value::as_str).unwrap_or("");
        let id = message.get("id").cloned();
        match (method, id) {
            ("initialize", Some(id)) => {
                if let Some(marker) = value_of("initialize_wait:") {
                    stalled_setup(&marker, has("initialize_child"));
                    continue;
                }
                let version: u64 = value_of("version:")
                    .and_then(|text| text.parse().ok())
                    .unwrap_or(1);
                let methods = if needs_sign_in {
                    json!([
                        {"id": "fake-login", "name": "Fake login", "description": "Sign in with the fake agent"},
                        {"id": "terminal-only", "name": "Terminal", "type": "terminal", "args": ["--login"]}
                    ])
                } else {
                    json!([])
                };
                send(&json!({"jsonrpc": "2.0", "id": id, "result": {
                    "protocolVersion": version,
                    "agentCapabilities": {},
                    "agentInfo": {"name": "fake-agent", "version": "0.0.1"},
                    "authMethods": methods
                }}));
            }
            ("authenticate", Some(id)) => {
                if has("auth_wait") {
                    continue;
                }
                if let Some(delay) = value_of("auth_delay:").and_then(|text| text.parse().ok()) {
                    std::thread::sleep(std::time::Duration::from_millis(delay));
                }
                let method = message["params"]["methodId"].as_str().unwrap_or("");
                if (has("auth") || has("auth_prompt") || has("cancel_auth"))
                    && method == "fake-login"
                {
                    signed_in = true;
                    send(&json!({"jsonrpc": "2.0", "id": id, "result": {}}));
                } else {
                    send(
                        &json!({"jsonrpc": "2.0", "id": id, "error": {"code": -32603, "message": "the login failed"}}),
                    );
                }
            }
            ("session/new", Some(id)) => {
                if (has("auth") || has("auth_never")) && !signed_in {
                    send(
                        &json!({"jsonrpc": "2.0", "id": id, "error": {"code": -32000, "message": "sign in first"}}),
                    );
                } else {
                    send(&json!({"jsonrpc": "2.0", "id": id, "result": {"sessionId": session}}));
                }
            }
            ("session/prompt", Some(id)) => {
                if has("cancel_auth") && signed_in {
                    chunk(session, "the cancelled task was incorrectly restarted");
                    send(&json!({"jsonrpc":"2.0", "id":id, "result":{"stopReason":"end_turn"}}));
                    continue;
                }
                if has("auth_prompt") && !signed_in {
                    send(
                        &json!({"jsonrpc": "2.0", "id": id, "error": {"code": -32000, "message": "sign in first (at the prompt)"}}),
                    );
                    continue;
                }
                prompt(&words, &receiver, session, &id, &value_of);
            }
            (_, Some(id)) => {
                send(
                    &json!({"jsonrpc": "2.0", "id": id, "error": {"code": -32601, "message": "method not found"}}),
                );
            }
            _ => {}
        }
    }
}

fn emit_progress(session: &str) {
    let path = std::env::current_dir().unwrap_or_default().join("a.txt");
    update(
        session,
        json!({"sessionUpdate":"agent_thought_chunk", "content":{"type":"text","text":"private thought"}}),
    );
    update(
        "sess_unrelated",
        json!({"sessionUpdate":"tool_call", "toolCallId":"other-call", "title":"unrelated session", "kind":"read", "status":"pending"}),
    );
    update(
        session,
        json!({"sessionUpdate":"tool_call", "toolCallId":"thinking-call", "title":"Internal planning", "kind":"think", "status":"in_progress"}),
    );
    update(
        session,
        json!({"sessionUpdate":"tool_call", "toolCallId":"live-call", "title":"Read config", "kind":"read", "status":"pending", "locations":[{"path":path.to_string_lossy()}], "rawInput":{"secret":"private input"}, "rawOutput":"private output"}),
    );
    update(
        session,
        json!({"sessionUpdate":"tool_call_update", "toolCallId":"live-call", "status":"in_progress"}),
    );
}

fn wait_for_cancel(receiver: &mpsc::Receiver<Value>, session: &str, id: &Value, words: &[&str]) {
    chunk(session, "working");
    while let Ok(message) = receiver.recv() {
        if message.get("method").and_then(Value::as_str) == Some("session/cancel") {
            if words.contains(&"cancel_permission") {
                send(
                    &json!({"jsonrpc":"2.0", "id":901, "method":"session/request_permission",
                    "params":{"sessionId":session,"toolCall":{"toolCallId":"after-stop","title":"write after stop","kind":"edit"},
                    "options":[{"optionId":"allow-once","name":"Allow","kind":"allow_once"}]}}),
                );
                while let Ok(reply) = receiver.recv() {
                    if reply.get("id") == Some(&json!(901)) {
                        chunk(
                            session,
                            &format!("late permission: {}", reply["result"]["outcome"]["outcome"]),
                        );
                        break;
                    }
                }
            }
            if words.contains(&"cancel_exit") {
                std::process::exit(0);
            }
            if words.contains(&"cancel_error") || words.contains(&"cancel_auth") {
                let code = if words.contains(&"cancel_auth") {
                    -32000
                } else {
                    -32603
                };
                send(
                    &json!({"jsonrpc":"2.0", "id":id, "error":{"code":code, "message":"cancelled operation surfaced as an error"}}),
                );
                return;
            }
            send(&json!({"jsonrpc": "2.0", "id": id, "result": {"stopReason": "cancelled"}}));
            return;
        }
    }
}

fn request_permission(spec: &str, receiver: &mpsc::Receiver<Value>, session: &str) {
    let mut parts = spec.splitn(2, ':');
    let kind = parts.next().unwrap_or("other").to_owned();
    let path = parts.next().map(str::to_owned);
    update(
        session,
        json!({"sessionUpdate": "tool_call", "toolCallId": "call_1", "title": format!("do a {kind}"), "kind": kind, "status": "pending"}),
    );
    let locations: Vec<Value> = path.iter().map(|path| json!({"path": path})).collect();
    send(
        &json!({"jsonrpc": "2.0", "id": 900, "method": "session/request_permission", "params": {
            "sessionId": session,
            "toolCall": {"toolCallId": "call_1", "title": format!("do a {kind}"), "kind": kind, "locations": locations},
            "options": [
                {"optionId": "allow-always", "name": "Always", "kind": "allow_always"},
                {"optionId": "allow-once", "name": "Allow", "kind": "allow_once"},
                {"optionId": "reject-once", "name": "Reject", "kind": "reject_once"}
            ]
        }}),
    );
    while let Ok(reply) = receiver.recv() {
        if reply.get("id") == Some(&json!(900)) {
            let outcome = reply["result"]["outcome"].clone();
            let text = match outcome["outcome"].as_str() {
                Some("selected") => format!(
                    "permission outcome: {}",
                    outcome["optionId"].as_str().unwrap_or("?")
                ),
                other => format!("permission outcome: {}", other.unwrap_or("none")),
            };
            chunk(session, &text);
            break;
        }
    }
}

fn prompt(
    words: &[&str],
    receiver: &mpsc::Receiver<Value>,
    session: &str,
    id: &Value,
    value_of: &dyn Fn(&str) -> Option<String>,
) {
    let has = |word: &str| words.contains(&word);
    if has("crash") {
        std::process::exit(3);
    }
    if has("hang") {
        loop {
            std::thread::sleep(std::time::Duration::from_secs(3600));
        }
    }
    if let Some(count) = value_of("junk:").and_then(|text| text.parse::<u32>().ok()) {
        for number in 0..count {
            println!("this is not json {number}");
        }
    }
    if has("longline") {
        let line = "x".repeat(3 * 1024 * 1024);
        println!("{line}");
    }
    if has("unknown") {
        update(
            session,
            json!({"sessionUpdate": "from_the_future", "anything": 1}),
        );
    }
    if let Some(text) = value_of("say:") {
        chunk(session, &text);
    }
    if let Some(path) = value_of("write:") {
        let _ = std::fs::write(&path, "written by the fake agent");
        chunk(session, "wrote a file");
    }
    if let Some(method) = value_of("request:") {
        send(&json!({"jsonrpc": "2.0", "id": 700, "method": method, "params": {}}));
        if let Ok(reply) = receiver.recv() {
            let refused = reply.get("error").is_some();
            chunk(session, &format!("request {method} refused: {refused}"));
        }
    }
    if let Some(spec) = value_of("permission:") {
        request_permission(&spec, receiver, session);
    }
    if has("progress") {
        emit_progress(session);
    }
    if has("wait")
        || has("cancel_exit")
        || has("cancel_error")
        || has("cancel_auth")
        || has("cancel_permission")
    {
        wait_for_cancel(receiver, session, id, words);
        return;
    }
    if !words.iter().any(|word| {
        word.starts_with("permission:")
            || word.starts_with("write:")
            || word.starts_with("request:")
            || *word == "progress"
    }) {
        chunk(session, "Hello ");
        chunk(session, "world");
        update(
            session,
            json!({"sessionUpdate": "tool_call", "toolCallId": "call_0", "title": "read a file", "kind": "read", "status": "pending"}),
        );
        update(
            session,
            json!({"sessionUpdate": "tool_call_update", "toolCallId": "call_0", "status": "completed"}),
        );
    }
    if let Some(delay) = value_of("reply_delay:").and_then(|text| text.parse().ok()) {
        std::thread::sleep(std::time::Duration::from_millis(delay));
    }
    send(&json!({"jsonrpc": "2.0", "id": id, "result": {"stopReason": "end_turn"}}));
}
