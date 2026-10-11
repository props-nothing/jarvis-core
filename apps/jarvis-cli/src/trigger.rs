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
    "usage: jarvis trigger <add|list|pause|resume|remove> [...]\n       jarvis trigger add --mail \"is:unread from:client.example\" [--every 5m] [--project NAME] <objective...>\n       jarvis trigger list [--json]\n       jarvis trigger pause|resume|remove ID"
}

/// Splits `add` arguments into the mail query, the flags and the objective words.
fn parse_add(arguments: &[String]) -> Result<CreateTriggerRequest, String> {
    let mut words: Vec<&str> = Vec::new();
    let mut query: Option<String> = None;
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
        match arguments[index].as_str() {
            "--mail" => {
                query = Some(value(
                    "--mail",
                    "a Gmail search in quotes, such as \"is:unread from:acme.example\"",
                )?);
                index += 2;
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
    let query = query.ok_or("trigger add needs --mail with a Gmail search, for example: jarvis trigger add --mail \"is:unread\" note each subject in the project journal")?;
    if words.is_empty() {
        return Err("trigger add needs an objective: what to do about each new message".to_owned());
    }
    Ok(CreateTriggerRequest {
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
            eprintln!(
                "jarvis: mail that matches now is only recorded; each new one starts a run while you are away, and anything it needs approval for waits in `jarvis approvals list`"
            );
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
    let watching = format!(
        "mail matching {:?}, every {}",
        trigger.query,
        humanize(trigger.interval_seconds)
    );
    let watching = match &trigger.project {
        Some(project) => format!("{watching}, project {project}"),
        None => watching,
    };
    match trigger.next_poll_at {
        Some(next) if trigger.baselined => format!("{watching}, next look at {next}"),
        Some(next) => {
            format!("{watching}, first look at {next} (records what matches, starts nothing)")
        }
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
        assert_eq!(request.query, "is:unread from:acme.example");
        assert_eq!(request.objective, "note the subject");
        assert_eq!(request.every.as_deref(), Some("10m"));
        assert_eq!(request.project_id.as_deref(), Some("Support"));
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
