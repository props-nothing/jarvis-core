//! The `jarvis mcp` verbs: the MCP servers JARVIS can use (`P11-001`, `ADR-0168`).
//!
//! A server is another program's tools. `add` writes it to `mcp-servers.toml` (checked by the daemon's own parser first),
//! `test` connects once and shows what it offers without saving anything, and a change applies on the next `jarvis restart`.
//! A new server's tools **ask before they run** until you decide otherwise; `--read-only` is your own statement that a
//! server only reads, never something JARVIS infers from what the server says about itself.

use std::fmt::Write as _;

use serde_json::{Value, json};

use crate::api_client::ApiClient;
use crate::output::ExitStatus;
use crate::schedule::{print_json, report};

const USAGE: &str = "usage: jarvis mcp <list|on|off|add|test|remove|access> [...]
       jarvis mcp access [on|off|key]                              # let other AI apps (Claude Code, Cursor, VS Code) hand JARVIS tasks: shows the address and key\n       jarvis mcp access public on|off                             # reach it from the internet (a cloud phone agent): a free temporary address\n       jarvis mcp access public named HOSTNAME [--from-env NAME]   # a stable address on your own Cloudflare tunnel (token from the environment, a file, or typed in)\n       jarvis mcp access public named off                          # back to the free temporary address\n       jarvis mcp list [--json]                                    # the abilities you can switch on, and any program you added\n       jarvis mcp on browser                                       # switch an ability on (JARVIS does the setup)\n       jarvis mcp off browser\n       jarvis mcp add NAME [--read-only] -- PROGRAM [ARGS...]      # a program on this computer, e.g. -- npx -y @playwright/mcp@latest\n       jarvis mcp add NAME [--read-only] --url https://host/mcp    # a remote server (HTTPS)\n       jarvis mcp test NAME                                        # connect once and show its tools; nothing is saved\n       jarvis mcp test -- PROGRAM [ARGS...]                        # try a server before adding it\n       jarvis mcp remove NAME\n       A change applies on the next `jarvis restart`.";

/// Runs one MCP verb.
pub async fn run_mcp(client: &ApiClient, arguments: &[String]) -> ExitStatus {
    let rest = arguments.get(2..).unwrap_or_default();
    match arguments.get(1).map(String::as_str) {
        Some("list") | None => list(client, rest).await,
        Some("access") => access(client, rest).await,
        Some("on") => ability(client, rest, true).await,
        Some("off") => ability(client, rest, false).await,
        Some("add") => add(client, rest).await,
        Some("test") => test(client, rest).await,
        Some("remove") => remove(client, rest).await,
        Some(other) => {
            eprintln!("jarvis: unknown mcp command {other:?}");
            eprintln!("{USAGE}");
            ExitStatus::Usage
        }
    }
}

/// Splits the words before a lone `--` from the command after it.
fn split_command(arguments: &[String]) -> (&[String], &[String]) {
    arguments
        .iter()
        .position(|argument| argument == "--")
        .map_or((arguments, &[][..]), |index| {
            (&arguments[..index], &arguments[index + 1..])
        })
}

fn flag<'a>(arguments: &'a [String], name: &str) -> Option<&'a str> {
    arguments
        .iter()
        .position(|argument| argument == name)
        .and_then(|index| arguments.get(index + 1))
        .map(String::as_str)
}

/// The first word that is not a flag or the value of one.
fn name_of(before: &[String]) -> Option<&str> {
    let mut skip = false;
    for argument in before {
        if skip {
            skip = false;
        } else if argument == "--url" || argument == "--root" {
            skip = true;
        } else if !argument.starts_with("--") {
            return Some(argument.as_str());
        }
    }
    None
}

fn body_from(before: &[String], command: &[String]) -> Value {
    json!({
        "name": name_of(before).unwrap_or_default(),
        "command": command,
        "endpoint": flag(before, "--url"),
        "read_only": before.iter().any(|argument| argument == "--read-only"),
    })
}

async fn list(client: &ApiClient, rest: &[String]) -> ExitStatus {
    let overview = match client.mcp_overview().await {
        Ok(overview) => overview,
        Err(error) => return report(&error),
    };
    if rest.iter().any(|argument| argument == "--json") {
        return print_json(&overview);
    }
    print!("{}", render(&overview));
    ExitStatus::Ok
}

/// Renders the abilities: what a person can switch on, and where each one stands.
fn render_abilities(overview: &Value) -> String {
    let mut text = String::new();
    for ability in overview["abilities"].as_array().into_iter().flatten() {
        let state = match ability["state"].as_str().unwrap_or("off") {
            "on" => "ON",
            "preparing" => "getting ready",
            "restart" => "ready after a restart",
            "problem" => "PROBLEM",
            _ => "off",
        };
        let _ = writeln!(
            text,
            "{:8} {:22} {}\n         {}",
            ability["id"].as_str().unwrap_or("?"),
            state,
            ability["title"].as_str().unwrap_or(""),
            ability["blurb"].as_str().unwrap_or("")
        );
        if let Some(needs) = ability["needs"].as_str() {
            let _ = writeln!(text, "         needs {needs} (free, nodejs.org)");
        }
    }
    if !text.is_empty() {
        text.push_str("\nswitch one on with: jarvis mcp on NAME\n");
    }
    text
}

/// The internet-address part of `jarvis mcp access`.
fn render_public(public: &Value) -> String {
    let state = public["status"]["state"].as_str().unwrap_or("off");
    let stable = public["stable"]["configured"] == true;
    match state {
        "on" if stable => format!(
            "\nreachable from the internet at: {}\n  (your own Cloudflare tunnel, so the address stays the same; anyone with it and the key can use the tools)\n  back to the temporary address with: jarvis mcp access public named off\n\nAnything that needs your approval waits for you (see `jarvis approvals list`) and is refused after two minutes.\n",
            public["url"].as_str().unwrap_or("")
        ),
        "on" => format!(
            "\nreachable from the internet at: {}\n  (the address changes every time JARVIS starts; anyone with it and the key can use the tools)\n  stop it with: jarvis mcp access public off\n\nAnything that needs your approval waits for you (see `jarvis approvals list`) and is refused after two minutes.\n",
            public["url"].as_str().unwrap_or("")
        ),
        "getting" | "starting" => "\nthe internet address is starting; run `jarvis mcp access` again in a few seconds.\n".to_owned(),
        "problem" => format!(
            "\nthe internet address has a problem: {}\n",
            public["status"]["reason"].as_str().unwrap_or("it could not start")
        ),
        _ => "\nOnly this computer can connect. Anything that needs your approval waits for you (see `jarvis approvals list`) and is refused after two minutes.\nto let a cloud app (such as a phone agent) reach it: jarvis mcp access public on\n".to_owned(),
    }
}

/// What `jarvis mcp access` says: the state, the address and the key, with a line to paste.
fn render_access(info: &Value) -> String {
    let mut text = String::new();
    let enabled = info["enabled"] == true;
    let running = info["running"] == true;
    let _ = writeln!(
        text,
        "other apps may connect: {}",
        match (enabled, running) {
            (true, true) => "on",
            (true, false) => "on after a restart (`jarvis restart`)",
            (false, true) => "off after a restart (`jarvis restart`)",
            (false, false) => "off",
        }
    );
    if let (true, Some(address), Some(key)) =
        (enabled, info["address"].as_str(), info["key"].as_str())
    {
        let _ = writeln!(text, "address: {address}");
        let _ = writeln!(text, "key:     {key}   (keep it secret)");
        let _ = writeln!(
            text,
            "\nClaude Code:\n  claude mcp add --transport http jarvis {address} --header \"Authorization: Bearer {key}\""
        );
        text.push_str(&render_public(&info["public"]));
    } else if !enabled {
        text.push_str("switch it on with: jarvis mcp access on\n");
    }
    text
}

/// The words of a command that are not flags or the values of `--url`, `--root`, `--from-env` and `--file`.
fn words_of(arguments: &[String]) -> Vec<&str> {
    let mut words = Vec::new();
    let mut skip = false;
    for argument in arguments {
        if skip {
            skip = false;
        } else if matches!(
            argument.as_str(),
            "--url" | "--root" | "--from-env" | "--file"
        ) {
            skip = true;
        } else if !argument.starts_with("--") {
            words.push(argument.as_str());
        }
    }
    words
}

/// Reads the tunnel token from an environment variable, a file, or standard input. It is never a command-line value, which would be kept in
/// shell history and visible to other processes.
fn read_tunnel_token(arguments: &[String]) -> Result<String, String> {
    use std::io::{IsTerminal as _, Read as _, Write as _};
    if let Some(name) = flag(arguments, "--from-env") {
        return std::env::var(name)
            .map_err(|_| format!("the environment variable {name} is not set"));
    }
    if let Some(path) = flag(arguments, "--file") {
        return std::fs::read_to_string(path).map_err(|_| format!("{path} could not be read"));
    }
    let mut text = String::new();
    if std::io::stdin().is_terminal() {
        eprint!(
            "In Cloudflare open the tunnel, choose Add a replica and copy the install command (do not run it). Paste it here and press Enter (use --from-env NAME or --file PATH to avoid it showing): "
        );
        let _ = std::io::stderr().flush();
        std::io::stdin()
            .read_line(&mut text)
            .map_err(|error| error.to_string())?;
    } else {
        std::io::stdin()
            .take(4096)
            .read_to_string(&mut text)
            .map_err(|error| error.to_string())?;
    }
    Ok(text)
}

async fn access(client: &ApiClient, rest: &[String]) -> ExitStatus {
    let result = match words_of(rest).as_slice() {
        ["public", "on"] => client.mcp_access_public(true).await,
        ["public", "off"] => client.mcp_access_public(false).await,
        ["public", "named", "off"] => client.mcp_access_named_off().await,
        ["public", "named", hostname] => match read_tunnel_token(rest) {
            Ok(token) => client.mcp_access_named(hostname, &token).await,
            Err(message) => {
                eprintln!("jarvis: {message}");
                return ExitStatus::Usage;
            }
        },
        [] => client.mcp_access().await,
        ["public", ..] => {
            eprintln!("{USAGE}");
            return ExitStatus::Usage;
        }
        _ => match name_of(rest) {
            None => client.mcp_access().await,
            Some("on") => client.mcp_access_switch(true).await,
            Some("off") => client.mcp_access_switch(false).await,
            Some("key") => client.mcp_access_new_key().await,
            Some(_) => {
                eprintln!("{USAGE}");
                return ExitStatus::Usage;
            }
        },
    };
    match result {
        Ok(info) => {
            print!("{}", render_access(&info));
            ExitStatus::Ok
        }
        Err(error) => report(&error),
    }
}

async fn ability(client: &ApiClient, rest: &[String], on: bool) -> ExitStatus {
    let Some(id) = name_of(rest) else {
        eprintln!("{USAGE}");
        return ExitStatus::Usage;
    };
    match client.mcp_ability(id, on).await {
        Ok(overview) => {
            println!(
                "{id} is now {}.",
                if on { "switched on" } else { "switched off" }
            );
            if overview["restart_needed"] == true {
                println!(
                    "  one `jarvis restart` finishes it (the first start downloads the program, which can take a minute)."
                );
            }
            ExitStatus::Ok
        }
        Err(error) => report(&error),
    }
}

/// Renders the overview for a terminal.
fn render(overview: &Value) -> String {
    let mut abilities = render_abilities(overview);
    let ids: Vec<&str> = overview["abilities"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|ability| ability["id"].as_str())
        .collect();
    let servers: Vec<Value> = overview["servers"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter(|server| !ids.contains(&server["name"].as_str().unwrap_or_default()))
        .collect();
    if servers.is_empty() {
        return abilities;
    }
    abilities.push_str("\nprograms you added yourself:\n");
    let rest = render_servers(overview, &servers);
    abilities.push_str(&rest);
    abilities
}

/// Renders the servers the owner added by hand.
fn render_servers(overview: &Value, servers: &[Value]) -> String {
    if servers.is_empty() {
        return "no MCP servers yet.\n  add one: jarvis mcp add NAME -- PROGRAM [ARGS...]   (for example: jarvis mcp add files -- npx -y @modelcontextprotocol/server-filesystem C:\\Users\\you\\Documents)\n".to_owned();
    }
    let mut text = String::new();
    for server in servers {
        let name = server["name"].as_str().unwrap_or("?");
        let state = server["state"].as_str().unwrap_or("?");
        let tools = server["tools"].as_array().map_or(0, Vec::len);
        let asks = if server["posture"] == "read-only" {
            "runs without asking (you marked it read-only)"
        } else {
            "asks before each call"
        };
        let how = match state {
            "connected" => format!(
                "connected, {} {}",
                server["era"].as_str().unwrap_or("?"),
                server["revision"].as_str().unwrap_or("")
            ),
            "unreachable" => format!(
                "NOT REACHABLE: {}",
                server["reason"].as_str().unwrap_or("no reason given")
            ),
            _ => "not started yet (restart to apply)".to_owned(),
        };
        let _ = writeln!(text, "{name}  {how}\n  {tools} tool(s), {asks}");
        for tool in server["tools"].as_array().into_iter().flatten() {
            let _ = writeln!(
                text,
                "    {}  {}",
                tool["id"].as_str().unwrap_or("?"),
                tool["description"].as_str().unwrap_or("")
            );
        }
        for excluded in server["excluded"].as_array().into_iter().flatten() {
            let _ = writeln!(
                text,
                "    left out: {}: {}",
                excluded["tool"].as_str().unwrap_or("?"),
                excluded["reason"].as_str().unwrap_or("")
            );
        }
    }
    if overview["restart_needed"] == true {
        text.push_str(
            "\nthe file has changed since JARVIS started: `jarvis restart` applies it.\n",
        );
    }
    text
}

async fn add(client: &ApiClient, rest: &[String]) -> ExitStatus {
    let (before, command) = split_command(rest);
    if name_of(before).is_none() {
        eprintln!("{USAGE}");
        return ExitStatus::Usage;
    }
    let overview = match client.mcp_add(&body_from(before, command)).await {
        Ok(overview) => overview,
        Err(error) => return report(&error),
    };
    println!("added {}.", name_of(before).unwrap_or_default());
    println!(
        "  check it first with: jarvis mcp test {}",
        name_of(before).unwrap_or_default()
    );
    if overview["restart_needed"] == true {
        println!("  then apply it with: jarvis restart");
    }
    ExitStatus::Ok
}

async fn test(client: &ApiClient, rest: &[String]) -> ExitStatus {
    let (before, command) = split_command(rest);
    let body = if command.is_empty() {
        let Some(name) = name_of(before) else {
            eprintln!("{USAGE}");
            return ExitStatus::Usage;
        };
        let overview = match client.mcp_overview().await {
            Ok(overview) => overview,
            Err(error) => return report(&error),
        };
        let Some(server) = overview["servers"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|server| server["name"] == name)
        else {
            eprintln!("jarvis: there is no server called {name}");
            return ExitStatus::Usage;
        };
        json!({
            "name": name,
            "command": server["command"],
            "endpoint": server["endpoint"],
            "read_only": server["posture"] == "read-only",
        })
    } else {
        body_from(before, command)
    };
    eprintln!("connecting (a first start with npx may download the program and take a minute)...");
    let report_value = match client.mcp_test(&body).await {
        Ok(value) => value,
        Err(error) => return report(&error),
    };
    if report_value["ok"] != true {
        eprintln!(
            "jarvis: could not use that server: {}",
            report_value["message"]
                .as_str()
                .unwrap_or("no reason given")
        );
        return ExitStatus::Unavailable;
    }
    println!(
        "reached it ({} {}{}).",
        report_value["era"].as_str().unwrap_or("?"),
        report_value["revision"].as_str().unwrap_or(""),
        report_value["reported_name"]
            .as_str()
            .map_or_else(String::new, |name| format!(", it calls itself {name}"))
    );
    let tools = report_value["tools"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    println!("{} tool(s) JARVIS can offer:", tools.len());
    for tool in &tools {
        println!(
            "  {}  {}",
            tool["id"].as_str().unwrap_or("?"),
            tool["description"].as_str().unwrap_or("")
        );
    }
    for excluded in report_value["excluded"].as_array().into_iter().flatten() {
        println!(
            "  left out: {}: {}",
            excluded["tool"].as_str().unwrap_or("?"),
            excluded["reason"].as_str().unwrap_or("")
        );
    }
    ExitStatus::Ok
}

async fn remove(client: &ApiClient, rest: &[String]) -> ExitStatus {
    let Some(name) = name_of(rest) else {
        eprintln!("{USAGE}");
        return ExitStatus::Usage;
    };
    match client.mcp_remove(name).await {
        Ok(overview) => {
            println!("removed {name}.");
            if overview["restart_needed"] == true {
                println!("  `jarvis restart` applies it.");
            }
            ExitStatus::Ok
        }
        Err(error) => report(&error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(parts: &[&str]) -> Vec<String> {
        parts.iter().map(|part| (*part).to_owned()).collect()
    }

    #[test]
    fn the_command_is_everything_after_the_lone_double_dash() {
        let arguments = words(&[
            "files",
            "--read-only",
            "--",
            "npx",
            "-y",
            "--yes-flag",
            "dir",
        ]);
        let (before, command) = split_command(&arguments);
        assert_eq!(before, ["files", "--read-only"]);
        assert_eq!(command, ["npx", "-y", "--yes-flag", "dir"]);
        assert_eq!(name_of(before), Some("files"));
    }

    #[test]
    fn a_remote_server_has_no_command_and_read_only_is_explicit() {
        let arguments = words(&["docs", "--url", "https://example.invalid/mcp"]);
        let (before, command) = split_command(&arguments);
        let body = body_from(before, command);
        assert_eq!(body["name"], "docs");
        assert_eq!(body["endpoint"], "https://example.invalid/mcp");
        assert_eq!(body["read_only"], false, "nothing is read-only unless said");
        assert_eq!(body["command"], json!([]));
    }

    #[test]
    fn the_list_says_why_a_server_is_unreachable_and_what_was_left_out() {
        let overview = json!({
            "restart_needed": true,
            "servers": [
                { "name": "good", "state": "connected", "era": "legacy", "revision": "2025-06-18",
                  "posture": "unclassified",
                  "tools": [{ "id": "mcp.good.echo", "description": "Echoes" }],
                  "excluded": [{ "tool": "odd", "reason": "its schema is unusable" }] },
                { "name": "bad", "state": "unreachable", "reason": "connection closed", "posture": "read-only",
                  "tools": [], "excluded": [] }
            ]
        });
        let text = render(&overview);
        assert!(
            text.contains("good  connected, legacy 2025-06-18"),
            "{text}"
        );
        assert!(text.contains("asks before each call"), "{text}");
        assert!(
            text.contains("left out: odd: its schema is unusable"),
            "{text}"
        );
        assert!(
            text.contains("bad  NOT REACHABLE: connection closed"),
            "{text}"
        );
        assert!(text.contains("jarvis restart"), "{text}");
    }

    #[test]
    fn the_access_line_gives_the_address_the_key_and_a_line_to_paste() {
        let on = render_access(
            &json!({ "enabled": true, "running": true, "address": "http://127.0.0.1:8766/mcp", "key": "abc123" }),
        );
        assert!(on.contains("other apps may connect: on"), "{on}");
        assert!(on.contains("claude mcp add --transport http jarvis http://127.0.0.1:8766/mcp --header \"Authorization: Bearer abc123\""), "{on}");
        let off = render_access(
            &json!({ "enabled": false, "running": false, "address": null, "key": null }),
        );
        assert!(off.contains("jarvis mcp access on"), "{off}");
        assert!(
            !off.contains("Bearer"),
            "no key is shown while it is off: {off}"
        );
        let pending = render_access(
            &json!({ "enabled": true, "running": false, "address": "http://127.0.0.1:8766/mcp", "key": "k" }),
        );
        assert!(pending.contains("on after a restart"), "{pending}");
    }

    #[test]
    fn abilities_lead_the_list_in_plain_words_and_say_what_is_missing() {
        let text = render(&json!({
            "restart_needed": false,
            "servers": [],
            "abilities": [
                { "id": "browser", "state": "off", "title": "Browse the web", "blurb": "Opens pages.", "needs": "Node.js" },
                { "id": "docs", "state": "restart", "title": "Look up docs", "blurb": "Finds docs.", "needs": null }
            ]
        }));
        assert!(text.contains("browser  off"), "{text}");
        assert!(text.contains("Browse the web"), "{text}");
        assert!(text.contains("needs Node.js (free, nodejs.org)"), "{text}");
        assert!(text.contains("docs     ready after a restart"), "{text}");
        assert!(text.contains("jarvis mcp on NAME"), "{text}");
        assert!(
            !text.contains("programs you added yourself"),
            "nothing custom: {text}"
        );
    }

    #[test]
    fn a_program_added_by_hand_is_listed_apart_from_the_abilities() {
        let text = render(&json!({
            "restart_needed": false,
            "abilities": [{ "id": "browser", "state": "on", "title": "Browse the web", "blurb": "x", "needs": null }],
            "servers": [
                { "name": "browser", "state": "connected", "era": "legacy", "revision": "r", "posture": "unclassified", "tools": [], "excluded": [] },
                { "name": "mine", "state": "connected", "era": "legacy", "revision": "r", "posture": "unclassified", "tools": [], "excluded": [] }
            ]
        }));
        assert!(
            text.contains("programs you added yourself:\nmine"),
            "{text}"
        );
        assert!(
            !text.contains("browser  connected"),
            "an ability is not repeated as a custom program: {text}"
        );
    }

    #[test]
    fn a_stable_address_says_it_stays_and_a_temporary_one_says_it_changes() {
        let stable = render_public(&serde_json::json!({
            "status": { "state": "on" },
            "url": "https://jarvis.example.com/mcp",
            "stable": { "configured": true, "host": "jarvis.example.com" }
        }));
        assert!(stable.contains("https://jarvis.example.com/mcp"));
        assert!(stable.contains("stays the same"));
        assert!(!stable.contains("changes every time"));
    }

    #[test]
    fn the_token_value_after_from_env_is_not_taken_for_a_hostname() {
        let arguments: Vec<String> = [
            "public",
            "named",
            "jarvis.example.com",
            "--from-env",
            "TOKEN_VAR",
        ]
        .iter()
        .map(|word| (*word).to_owned())
        .collect();
        assert_eq!(
            words_of(&arguments),
            vec!["public", "named", "jarvis.example.com"]
        );
    }

    #[test]
    fn the_internet_address_is_shown_only_when_it_is_up_and_says_it_changes() {
        let up = serde_json::json!({ "status": { "state": "on" }, "url": "https://x-y.trycloudflare.com/mcp" });
        let text = render_public(&up);
        assert!(text.contains("https://x-y.trycloudflare.com/mcp"));
        assert!(text.contains("changes every time"));
        let off = render_public(&serde_json::json!({ "status": { "state": "off" } }));
        assert!(off.contains("jarvis mcp access public on"));
        assert!(!off.contains("trycloudflare"));
        let bad = render_public(
            &serde_json::json!({ "status": { "state": "problem", "reason": "no network" } }),
        );
        assert!(bad.contains("no network"));
    }
}
