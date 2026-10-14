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
//! * `hang`: never answer the prompt and ignore `session/cancel`.
//! * `crash`: exit with code 3 when the prompt arrives.
//! * `junk:N`: write N lines that are not JSON before answering.
//! * `longline`: write one line of three megabytes.
//! * `request:METHOD`: send the client a request for METHOD and report whether it was refused.
//! * `stderr`: write a line to the error stream first.
//! * `unknown`: send an update variant nobody knows.
//! * `progress`: public tool snapshots plus private and wrong-session updates that a client must not expose as progress.

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

fn main() {
    let cwd = std::env::current_dir()
        .map(|path| path.to_string_lossy().into_owned())
        .unwrap_or_default();
    let script = std::env::args()
        .nth(1)
        .or_else(|| std::env::var("FAKE_ACP").ok())
        .unwrap_or_else(|| "chat".to_owned())
        .replace("{cwd}", &cwd);
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

    let (sender, receiver) = mpsc::channel::<Value>();
    std::thread::spawn(move || {
        let stdin = std::io::stdin();
        for line in stdin.lock().lines().map_while(Result::ok) {
            if let Ok(value) = serde_json::from_str::<Value>(&line)
                && sender.send(value).is_err()
            {
                return;
            }
        }
    });

    let session = "sess_fake";
    let needs_sign_in = has("auth") || has("auth_never") || has("auth_prompt");
    let mut signed_in = false;
    while let Ok(message) = receiver.recv() {
        let method = message.get("method").and_then(Value::as_str).unwrap_or("");
        let id = message.get("id").cloned();
        match (method, id) {
            ("initialize", Some(id)) => {
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
                let method = message["params"]["methodId"].as_str().unwrap_or("");
                if (has("auth") || has("auth_prompt")) && method == "fake-login" {
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

fn wait_for_cancel(receiver: &mpsc::Receiver<Value>, session: &str, id: &Value) {
    chunk(session, "working");
    while let Ok(message) = receiver.recv() {
        if message.get("method").and_then(Value::as_str) == Some("session/cancel") {
            send(&json!({"jsonrpc": "2.0", "id": id, "result": {"stopReason": "cancelled"}}));
            return;
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
    if has("progress") {
        emit_progress(session);
    }
    if has("wait") {
        wait_for_cancel(receiver, session, id);
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
    send(&json!({"jsonrpc": "2.0", "id": id, "result": {"stopReason": "end_turn"}}));
}
