//! The `jarvis trigger` verbs: a mail that arrives starts a run, while you are away (`P6-012`, `ADR-0193`).
//!
//! A trigger is a request to start an **ordinary** run when a new message matches a Gmail search. It has exactly the authority an ordinary
//! run has, so a trigger run that wants to send mail or fetch a page **stops and waits for you**, and `jarvis approvals` is where you
//! answer. Nothing here can approve anything, and a model has no way to make a trigger: only you, here or in the API.
//!
//! The first look at a new trigger records what already matches and answers none of it; only mail that arrives later starts a run.

use jarvis_protocol::{CreateTriggerRequest, TriggerReply};

use crate::api_client::ApiClient;
use crate::output::ExitStatus;
use crate::schedule::{print_json, report};

/// Runs one trigger verb.
pub async fn run_trigger(client: &ApiClient, arguments: &[String]) -> ExitStatus {
    let json = crate::json_requested(arguments);
    match arguments.get(1).map(String::as_str) {
        Some("add") => add(client, arguments).await,
        Some("list") | None => list(client, json).await,
        Some("pause") => toggle(client, arguments, false).await,
        Some("resume") => toggle(client, arguments, true).await,
        Some("remove") => remove(client, arguments).await,
        Some(other) => {
            eprintln!("jarvis: unknown trigger command {other:?}");
            eprintln!("{}", trigger_usage());
            ExitStatus::Usage
        }
    }
}

/// The usage text for `jarvis trigger`.
pub(crate) const fn trigger_usage() -> &'static str {
    "usage: jarvis trigger <add|list|pause|resume|remove> [...]\n       jarvis trigger add --mail \"is:unread from:client.example\" [--every 5m] [--project NAME] <objective...>\n       jarvis trigger add --file scans/new [--every 5m] [--project NAME] <objective...>   # a new or changed file in a folder inside a granted folder\n       jarvis trigger add --webhook [--project NAME] <objective...>   # prints a secret local address once; a request to it starts a task\n       jarvis trigger list [--json]\n       jarvis trigger pause|resume|remove ID"
}

/// Splits `add` arguments into what to watch, the flags and the objective words.
fn parse_add(arguments: &[String]) -> Result<CreateTriggerRequest, String> {
    let mut words: Vec<&str> = Vec::new();
    let mut watched: Option<(&'static str, Option<String>)> = None;
    let mut every: Option<String> = None;
    let mut project: Option<String> = None;
    let mut index = 2;
    while index < arguments.len() {
        let value = |name: &str, what: &str| -> Result<String, String> {
            arguments
                .get(index + 1)
                .filter(|next| !next.starts_with("--"))
                .cloned()
                .ok_or_else(|| format!("{name} needs {what}"))
        };
        let mut watch = |kind: &'static str, query: Option<String>| -> Result<(), String> {
            if watched.is_some() {
                return Err("name one thing to watch: --mail, --file or --webhook".to_owned());
            }
            watched = Some((kind, query));
            Ok(())
        };
        match arguments[index].as_str() {
            "--mail" => {
                watch(
                    "mail",
                    Some(value(
                        "--mail",
                        "a Gmail search in quotes, such as \"is:unread from:acme.example\"",
                    )?),
                )?;
                index += 2;
            }
            "--file" => {
                watch(
                    "file",
                    Some(value(
                        "--file",
                        "a folder inside a granted folder, such as scans/new",
                    )?),
                )?;
                index += 2;
            }
            "--webhook" => {
                watch("webhook", None)?;
                index += 1;
            }
            "--every" => {
                every = Some(value("--every", "an interval such as 5m or 1h")?);
                index += 2;
            }
            "--project" => {
                project = Some(value("--project", "a project name")?);
                index += 2;
            }
            "--root" => index += 2,
            "--json" => index += 1,
            word => {
                words.push(word);
                index += 1;
            }
        }
    }
    let Some((kind, query)) = watched else {
        return Err("trigger add needs what to watch: --mail \"is:unread\", --file scans/new or --webhook, for example: jarvis trigger add --mail \"is:unread\" note each subject in the project journal".to_owned());
    };
    if words.is_empty() {
        return Err("trigger add needs an objective: what to do about each event".to_owned());
    }
    if kind == "webhook" && every.is_some() {
        return Err("a webhook is not polled, so --every does not apply to it".to_owned());
    }
    Ok(CreateTriggerRequest {
        kind: Some(kind.to_owned()),
        query,
        objective: words.join(" "),
        every,
        project_id: project,
    })
}

async fn add(client: &ApiClient, arguments: &[String]) -> ExitStatus {
    let request = match parse_add(arguments) {
        Ok(request) => request,
        Err(message) => {
            eprintln!("jarvis: {message}");
            eprintln!("{}", trigger_usage());
            return ExitStatus::Usage;
        }
    };
    match client.create_trigger(&request).await {
        Ok(created) => {
            println!("trigger {}", created.trigger_id);
            println!("  {}", describe(&created));
            if let Some(path) = &created.webhook_path {
                println!(
                    "  address: {path}   (on this computer's port; shown only now, keep it secret)"
                );
                eprintln!(
                    "jarvis: anyone who has this address can start a task, so give it only to the service you mean. The request body reaches the task as untrusted data. It is not shown again; make a new trigger to change it"
                );
            } else {
                eprintln!(
                    "jarvis: what already exists is only recorded; each new event starts a run while you are away, and anything it needs approval for waits in `jarvis approvals list`"
                );
            }
            ExitStatus::Ok
        }
        Err(error) => report(&error),
    }
}

async fn list(client: &ApiClient, json: bool) -> ExitStatus {
    let reply = match client.list_triggers().await {
        Ok(reply) => reply,
        Err(error) => return report(&error),
    };
    if json {
        return print_json(&reply);
    }
    if reply.triggers.is_empty() {
        println!("no triggers");
        return ExitStatus::Ok;
    }
    println!("{} trigger(s)", reply.total);
    for trigger in &reply.triggers {
        println!("  {}  {}", trigger.trigger_id, describe(trigger));
        println!("      {}", trigger.objective);
        let mut history = vec![format!("started {} run(s)", trigger.fire_count)];
        if let Some(last) = &trigger.last_run_id {
            history.push(format!("last run {last}"));
        }
        if let Some(problem) = &trigger.last_error {
            history.push(format!("last look failed: {problem}"));
        }
        println!("      {}", history.join(", "));
    }
    ExitStatus::Ok
}

/// Describes what a trigger watches and when it looks next, in the words a person would use.
fn describe(trigger: &TriggerReply) -> String {
    let every = humanize(trigger.interval_seconds);
    let watching = match trigger.kind.as_str() {
        "file" => format!("new files in {:?}, every {every}", trigger.query),
        "webhook" => "requests to its secret address".to_owned(),
        _ => format!("mail matching {:?}, every {every}", trigger.query),
    };
    let watching = match &trigger.project {
        Some(project) => format!("{watching}, project {project}"),
        None => watching,
    };
    match trigger.next_poll_at {
        Some(next) if trigger.baselined => format!("{watching}, next look at {next}"),
        Some(next) => {
            format!("{watching}, first look at {next} (records what exists, starts nothing)")
        }
        None if trigger.kind == "webhook" && trigger.enabled => format!("{watching}, listening"),
        None => format!("{watching}, paused"),
    }
}

fn humanize(seconds: u64) -> String {
    for (unit, size) in [("d", 86_400), ("h", 3600), ("m", 60)] {
        if seconds.is_multiple_of(size) {
            return format!("{}{unit}", seconds / size);
        }
    }
    format!("{seconds}s")
}

/// Resolves what the person typed to a trigger: the whole identifier, or a prefix that names exactly one.
async fn resolve(client: &ApiClient, typed: &str) -> Result<String, ExitStatus> {
    let reply = client
        .list_triggers()
        .await
        .map_err(|error| report(&error))?;
    let matches: Vec<&TriggerReply> = reply
        .triggers
        .iter()
        .filter(|trigger| trigger.trigger_id.starts_with(typed))
        .collect();
    match matches.as_slice() {
        [only] => Ok(only.trigger_id.clone()),
        [] => {
            eprintln!("jarvis: no trigger starts with {typed:?} (see `jarvis trigger list`)");
            Err(ExitStatus::Rejected)
        }
        _ => {
            eprintln!(
                "jarvis: {typed:?} matches more than one trigger; type more of the identifier"
            );
            Err(ExitStatus::Rejected)
        }
    }
}

fn identifier(arguments: &[String]) -> Option<&String> {
    arguments.get(2).filter(|value| !value.starts_with("--"))
}

async fn toggle(client: &ApiClient, arguments: &[String], enabled: bool) -> ExitStatus {
    let Some(typed) = identifier(arguments) else {
        eprintln!("{}", trigger_usage());
        return ExitStatus::Usage;
    };
    let id = match resolve(client, typed).await {
        Ok(id) => id,
        Err(status) => return status,
    };
    match client.set_trigger_enabled(&id, enabled).await {
        Ok(trigger) => {
            println!("{}  {}", trigger.trigger_id, describe(&trigger));
            ExitStatus::Ok
        }
        Err(error) => report(&error),
    }
}

async fn remove(client: &ApiClient, arguments: &[String]) -> ExitStatus {
    let Some(typed) = identifier(arguments) else {
        eprintln!("{}", trigger_usage());
        return ExitStatus::Usage;
    };
    let id = match resolve(client, typed).await {
        Ok(id) => id,
        Err(status) => return status,
    };
    match client.remove_trigger(&id).await {
        Ok(()) => {
            println!("removed {id}");
            ExitStatus::Ok
        }
        Err(error) => report(&error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(line: &str) -> Vec<String> {
        line.split(' ').map(str::to_owned).collect()
    }

    #[test]
    fn the_query_the_flags_and_the_objective_are_separated() {
        let mut arguments = words("trigger add");
        arguments.extend([
            "--mail".to_owned(),
            "is:unread from:acme.example".to_owned(),
        ]);
        arguments.extend(words("--every 10m note the subject --project Support"));
        let request = parse_add(&arguments).unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(request.kind.as_deref(), Some("mail"));
        assert_eq!(
            request.query.as_deref(),
            Some("is:unread from:acme.example")
        );
        assert_eq!(request.objective, "note the subject");
        assert_eq!(request.every.as_deref(), Some("10m"));
        assert_eq!(request.project_id.as_deref(), Some("Support"));
    }

    #[test]
    fn a_file_folder_and_a_webhook_are_parsed_and_exactly_one_thing_is_watched() {
        let file = parse_add(&words("trigger add --file scans/new read it"))
            .unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(file.kind.as_deref(), Some("file"));
        assert_eq!(file.query.as_deref(), Some("scans/new"));
        let hook = parse_add(&words("trigger add --webhook log the body"))
            .unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(hook.kind.as_deref(), Some("webhook"));
        assert_eq!(hook.query, None);
        assert_eq!(hook.objective, "log the body");
        // Two things, or an interval for a webhook, are refused.
        assert!(parse_add(&words("trigger add --webhook --file scans do it")).is_err());
        assert!(parse_add(&words("trigger add --webhook --every 5m do it")).is_err());
    }

    #[test]
    fn a_missing_query_objective_or_flag_value_is_refused() {
        assert!(parse_add(&words("trigger add note it")).is_err());
        let mut no_objective = words("trigger add");
        no_objective.extend(["--mail".to_owned(), "is:unread".to_owned()]);
        assert!(parse_add(&no_objective).is_err());
        assert!(parse_add(&words("trigger add --mail")).is_err());
        // A flag is never taken as another flag's value.
        assert!(parse_add(&words("trigger add --mail --every 5m do it")).is_err());
    }
}
