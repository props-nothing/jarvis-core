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
       jarvis mcp add NAME --env-ref VARIABLE=mcp:NAME:KEY -- PROGRAM [ARGS...]  # reference only; set with jarvis keys set mcp:NAME:KEY
       jarvis mcp add NAME --url https://host/mcp [--bearer-ref mcp:NAME:KEY]  # reference only; jarvis keys set mcp:NAME:KEY prompts privately
       jarvis mcp test NAME --url https://host/mcp [--bearer-ref mcp:NAME:KEY] # test these settings without saving
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
        } else if matches!(
            argument.as_str(),
            "--url" | "--root" | "--env-ref" | "--bearer-ref"
        ) {
            skip = true;
        } else if !argument.starts_with("--") {
            return Some(argument.as_str());
        }
    }
    None
}

fn body_from(before: &[String], command: &[String]) -> Result<Value, String> {
    let mut environment = std::collections::BTreeMap::new();
    let mut authorization = None;
    let mut words = before.iter();
    while let Some(word) = words.next() {
        if word == "--bearer-ref" {
            if authorization.is_some() {
                return Err("use --bearer-ref only once".to_owned());
            }
            let reference = words.next().ok_or_else(|| {
                "use --bearer-ref mcp:SERVER:KEY (a reference, never a token)".to_owned()
            })?;
            let reference = jarvis_core::McpSecretRef::parse(reference)
                .map_err(|_| "use a named MCP key reference, never its value".to_owned())?;
            if reference.as_str().split(':').nth(1) != name_of(before) {
                return Err("the bearer reference must belong to this server's name".to_owned());
            }
            authorization = Some(reference.as_str().to_owned());
            continue;
        }
        if word == "--url" {
            words
                .next()
                .filter(|value| !value.is_empty() && !value.starts_with("--"))
                .ok_or_else(|| "--url needs an HTTP server address".to_owned())?;
            continue;
        }
        if word == "--root" {
            words.next();
            continue;
        }
        if word != "--env-ref" {
            continue;
        }
        let (name, reference) = words
            .next()
            .and_then(|value| value.split_once('='))
            .ok_or_else(|| {
                "use --env-ref VARIABLE=mcp:SERVER:KEY (references, never a key value)".to_owned()
            })?;
        let reference = jarvis_core::McpSecretRef::parse(reference)
            .map_err(|_| "use a named MCP key reference, never its value".to_owned())?;
        if environment
            .insert(name.to_owned(), reference.as_str().to_owned())
            .is_some()
        {
            return Err("each credential environment variable may be mapped only once".to_owned());
        }
    }
    if authorization.is_some() {
        if !command.is_empty() {
            return Err("--bearer-ref is for a remote --url, not a local command".to_owned());
        }
        let endpoint = flag(before, "--url")
            .and_then(|endpoint| reqwest::Url::parse(endpoint).ok())
            .filter(|endpoint| matches!(endpoint.scheme(), "http" | "https"))
            .ok_or_else(|| "--bearer-ref requires --url with an HTTP server address".to_owned())?;
        if endpoint.query().is_some() {
            return Err("a server using --bearer-ref needs an address without a query".to_owned());
        }
    }
    let mut body = json!({
        "name": name_of(before).unwrap_or_default(),
        "command": command,
        "endpoint": flag(before, "--url"),
        "read_only": before.iter().any(|argument| argument == "--read-only"),
        "env": environment,
    });
    if let Some(reference) = authorization {
        body["authorization"] = json!(reference);
    }
    Ok(body)
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
            "In Cloudflare copy the install command shown when you create the tunnel (do not run it). Paste it here and press Enter (use --from-env NAME or --file PATH to avoid it showing): "
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
        for credential in server["credentials"].as_array().into_iter().flatten() {
            let _ = writeln!(
                text,
                "  {}: {} ({})",
                credential["name"].as_str().unwrap_or("?"),
                credential["state"].as_str().unwrap_or("not set"),
                credential["reference"].as_str().unwrap_or("?")
            );
        }
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
    let body = match body_from(before, command) {
        Ok(body) => body,
        Err(message) => {
            eprintln!("jarvis: {message}");
            return ExitStatus::Usage;
        }
    };
    let overview = match client.mcp_add(&body).await {
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
    let body = if command.is_empty()
        && !before
            .iter()
            .any(|argument| argument == "--url" || argument == "--bearer-ref")
    {
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
            "env": server["env"].as_object().cloned().unwrap_or_default(),
            "authorization": server["authorization"],
        })
    } else {
        match body_from(before, command) {
            Ok(body) => body,
            Err(message) => {
                eprintln!("jarvis: {message}");
                return ExitStatus::Usage;
            }
        }
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

    #[test]
    fn credential_flags_are_references_and_not_server_names_or_literal_values() {
        let args = [
            "--env-ref",
            "GITHUB_PERSONAL_ACCESS_TOKEN=mcp:github:token",
            "github",
        ]
        .map(str::to_owned);
        assert_eq!(name_of(&args), Some("github"));
        let body = body_from(&args, &["github-mcp-server".to_owned(), "stdio".to_owned()])
            .unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(
            body["env"]["GITHUB_PERSONAL_ACCESS_TOKEN"],
            "mcp:github:token"
        );
        assert_eq!(body["read_only"], false);
        for bad in [
            vec!["github", "--env-ref"],
            vec!["github", "--env-ref", "API_KEY=synthetic-literal-secret"],
            vec![
                "github",
                "--env-ref",
                "API_KEY=mcp:github:token",
                "--env-ref",
                "API_KEY=mcp:github:token",
            ],
        ] {
            let error = body_from(&bad.into_iter().map(str::to_owned).collect::<Vec<_>>(), &[])
                .err()
                .unwrap_or_else(|| panic!("must refuse"));
            assert!(!error.contains("synthetic-literal-secret"));
        }
    }

    fn words(parts: &[&str]) -> Vec<String> {
        parts.iter().map(|part| (*part).to_owned()).collect()
    }

    fn must<T, E: std::fmt::Display>(result: Result<T, E>) -> T {
        result.unwrap_or_else(|error| panic!("{error}"))
    }

    fn client_at(port: u16) -> ApiClient {
        must(ApiClient::new(
            must(jarvis_core::LoopbackHost::new(port)),
            "synthetic-daemon-credential".to_owned(),
        ))
    }

    async fn daemon(
        replies: Vec<(&'static str, Value)>,
    ) -> (ApiClient, tokio::task::JoinHandle<Vec<Value>>) {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let listener = must(tokio::net::TcpListener::bind("127.0.0.1:0").await);
        let client = client_at(must(listener.local_addr()).port());
        let task = tokio::spawn(async move {
            must(tokio::time::timeout(std::time::Duration::from_secs(5), async move {
                let mut bodies = Vec::new();
                for (route, reply) in replies {
                    let (mut socket, _) = must(listener.accept().await);
                    let mut request = Vec::new();
                    let (end, length) = loop {
                        let mut buffer = [0_u8; 2048];
                        let count = must(socket.read(&mut buffer).await);
                        assert_ne!(count, 0, "the CLI sends a complete request");
                        request.extend_from_slice(&buffer[..count]);
                        let text = String::from_utf8_lossy(&request);
                        if let Some(end) = text.find("\r\n\r\n") {
                            let length = text[..end]
                                .lines()
                                .find_map(|line| {
                                    line.to_ascii_lowercase()
                                        .strip_prefix("content-length: ")
                                        .and_then(|value| value.parse::<usize>().ok())
                                })
                                .unwrap_or(0);
                            if request.len() >= end + 4 + length {
                                break (end + 4, length);
                            }
                        }
                    };
                    assert!(String::from_utf8_lossy(&request).starts_with(route));
                    bodies.push(if length == 0 {
                        Value::Null
                    } else {
                        must(serde_json::from_slice(&request[end..end + length]))
                    });
                    let body = reply.to_string();
                    let response = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    );
                    must(socket.write_all(response.as_bytes()).await);
                }
                bodies
            }).await)
        });
        (client, task)
    }

    #[test]
    fn a_bearer_reference_is_not_a_name_or_a_read_only_claim() {
        let args = words(&[
            "--bearer-ref",
            "mcp:remote:token",
            "remote",
            "--url",
            "https://example.invalid/mcp",
        ]);
        assert_eq!(name_of(&args), Some("remote"));
        let body = must(body_from(&args, &[]));
        assert_eq!(body["authorization"], "mcp:remote:token");
        assert_eq!(body["env"], json!({}));
        assert_eq!(body["command"], json!([]));
        assert_eq!(body["read_only"], false);
        assert!(USAGE.contains("--bearer-ref mcp:NAME:KEY"));
        assert!(USAGE.contains("jarvis keys set mcp:NAME:KEY"));
    }

    fn invalid_bearer_options() -> [Vec<&'static str>; 14] {
        [
            vec![
                "remote",
                "--url",
                "https://example.invalid/mcp",
                "--bearer-ref",
            ],
            vec![
                "remote",
                "--url",
                "https://example.invalid/mcp",
                "--bearer-ref",
                "--read-only",
            ],
            vec![
                "remote",
                "--url",
                "https://example.invalid/mcp",
                "--bearer-ref",
                "synthetic-literal-secret",
            ],
            vec![
                "remote",
                "--url",
                "https://example.invalid/mcp",
                "--bearer-ref",
                "github",
            ],
            vec![
                "remote",
                "--url",
                "https://example.invalid/mcp",
                "--bearer-ref",
                "mcp:other:token",
            ],
            vec![
                "remote",
                "--url",
                "https://example.invalid/mcp",
                "--bearer-ref",
                "mcp:remote:token",
                "--bearer-ref",
                "mcp:remote:token",
            ],
            vec!["remote", "--bearer-ref", "mcp:remote:token"],
            vec!["remote", "--bearer-ref", "mcp:remote:token", "--url"],
            vec!["remote", "--url", "--bearer-ref", "mcp:remote:token"],
            vec!["remote", "--url", "", "--bearer-ref", "mcp:remote:token"],
            vec![
                "remote",
                "--url",
                "file:///mcp",
                "--bearer-ref",
                "mcp:remote:token",
            ],
            vec![
                "remote",
                "--url",
                "https://example.invalid/mcp?token=synthetic-query-secret",
                "--bearer-ref",
                "mcp:remote:token",
            ],
            vec![
                "remote",
                "--bearer-ref",
                "mcp:remote:token",
                "--",
                "local-program",
            ],
            vec![
                "remote",
                "--url",
                "https://example.invalid/mcp",
                "--bearer-ref",
                "mcp:remote:token",
                "--",
                "local-program",
            ],
        ]
    }

    #[tokio::test]
    async fn invalid_bearer_options_are_refused_before_any_daemon_request() {
        let listener = must(std::net::TcpListener::bind("127.0.0.1:0"));
        must(listener.set_nonblocking(true));
        let client = client_at(must(listener.local_addr()).port());
        for parts in invalid_bearer_options() {
            let args = words(&parts);
            let (before, command) = split_command(&args);
            let error = body_from(before, command)
                .err()
                .unwrap_or_else(|| panic!("must refuse"));
            assert!(!error.contains("synthetic-literal-secret"));
            assert!(!error.contains("synthetic-query-secret"));
            for verb in ["add", "test"] {
                let mut arguments = words(&["mcp", verb]);
                arguments.extend(args.clone());
                let status = must(
                    tokio::time::timeout(
                        std::time::Duration::from_secs(1),
                        run_mcp(&client, &arguments),
                    )
                    .await,
                );
                assert_eq!(status, ExitStatus::Usage, "{verb}: {parts:?}");
                assert_eq!(
                    listener.accept().err().map(|error| error.kind()),
                    Some(std::io::ErrorKind::WouldBlock),
                    "invalid options must not reach the daemon"
                );
            }
        }
    }

    #[tokio::test]
    async fn a_remote_bearer_reference_round_trips_through_add_and_saved_test() {
        let server = json!({
            "name": "remote", "command": [], "endpoint": "https://example.invalid/mcp",
            "posture": "unclassified", "env": {}, "authorization": "mcp:remote:token",
            "credentials": [{ "name": "Authorization", "reference": "mcp:remote:token", "state": "not set" }],
        });
        let overview = json!({ "servers": [server] });
        let (client, requests) = daemon(vec![
            ("POST /api/v1/mcp/servers ", overview.clone()),
            ("GET /api/v1/mcp ", overview.clone()),
            ("POST /api/v1/mcp/test ", json!({ "ok": true, "tools": [] })),
        ])
        .await;
        assert_eq!(
            run_mcp(
                &client,
                &words(&[
                    "mcp",
                    "add",
                    "remote",
                    "--url",
                    "https://example.invalid/mcp",
                    "--bearer-ref",
                    "mcp:remote:token",
                ])
            )
            .await,
            ExitStatus::Ok
        );
        assert_eq!(
            run_mcp(&client, &words(&["mcp", "test", "remote"])).await,
            ExitStatus::Ok
        );
        let requests = must(requests.await);
        let expected = json!({
            "name": "remote", "command": [], "endpoint": "https://example.invalid/mcp",
            "read_only": false, "env": {}, "authorization": "mcp:remote:token",
        });
        assert_eq!(requests, vec![expected.clone(), Value::Null, expected]);
        assert!(render(&overview).contains("Authorization: not set (mcp:remote:token)"));
        let mut set = overview;
        set["servers"][0]["credentials"][0]["state"] = json!("set");
        assert!(render(&set).contains("Authorization: set (mcp:remote:token)"));
    }

    #[tokio::test]
    async fn explicit_remote_tests_do_not_look_up_the_saved_server() {
        for reference in [None, Some("mcp:remote:token")] {
            let (client, requests) = daemon(vec![(
                "POST /api/v1/mcp/test ",
                json!({ "ok": true, "tools": [] }),
            )])
            .await;
            let mut args = words(&[
                "mcp",
                "test",
                "remote",
                "--url",
                "https://explicit.invalid/mcp",
            ]);
            if let Some(reference) = reference {
                args.extend(words(&["--bearer-ref", reference]));
            }
            assert_eq!(run_mcp(&client, &args).await, ExitStatus::Ok);
            let requests = must(requests.await);
            assert_eq!(requests.len(), 1);
            assert_eq!(requests[0]["endpoint"], "https://explicit.invalid/mcp");
            assert_eq!(requests[0]["authorization"], json!(reference));
        }
    }

    #[tokio::test]
    async fn saved_tests_keep_github_oauth_and_local_credential_mappings() {
        for server in [
            json!({ "name": "github", "endpoint": "https://api.githubcopilot.com/mcp/", "command": [], "authorization": "github", "posture": "read-only" }),
            json!({ "name": "local", "endpoint": null, "command": ["local-program"], "env": { "API_KEY": "mcp:local:token" }, "posture": "unclassified" }),
        ] {
            let (client, requests) = daemon(vec![
                ("GET /api/v1/mcp ", json!({ "servers": [server.clone()] })),
                ("POST /api/v1/mcp/test ", json!({ "ok": true, "tools": [] })),
            ])
            .await;
            assert_eq!(
                run_mcp(
                    &client,
                    &words(&["mcp", "test", must(server["name"].as_str().ok_or("name"))])
                )
                .await,
                ExitStatus::Ok
            );
            let requests = must(requests.await);
            assert_eq!(requests[1]["authorization"], server["authorization"]);
            assert_eq!(requests[1]["command"], server["command"]);
            assert_eq!(requests[1]["endpoint"], server["endpoint"]);
            assert_eq!(
                requests[1]["env"],
                json!(server["env"].as_object().cloned().unwrap_or_default())
            );
            assert_eq!(requests[1]["read_only"], server["posture"] == "read-only");
        }
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
        let body = body_from(before, command).unwrap_or_else(|error| panic!("{error}"));
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
