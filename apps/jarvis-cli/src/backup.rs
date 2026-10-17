//! Owner-invoked offline database snapshots and guarded recovery (ADR-0241).

use jarvis_core::BackupId;
use jarvis_storage::{
    AppPaths, BackupError, create_database_backup, recover_database_restore,
    restore_database_backup, verify_database_backup,
};

use crate::output::ExitStatus;

const USAGE: &str = "usage: jarvis backup <create|verify ID|restore ID|recover> [--root DIR] [--json]\nStop JARVIS before create/restore/recover. Database snapshots contain personal data; settings, keys and files are not included.";

#[derive(Debug, Eq, PartialEq)]
enum Request {
    Create,
    Verify(BackupId),
    Restore(BackupId),
    Recover,
}

fn parse(arguments: &[String]) -> Option<Request> {
    let mut index = 2;
    let request = match arguments.get(1)?.as_str() {
        "create" => Request::Create,
        "recover" => Request::Recover,
        verb @ ("verify" | "restore") => {
            let id = arguments.get(index)?.parse().ok()?;
            index += 1;
            if verb == "verify" {
                Request::Verify(id)
            } else {
                Request::Restore(id)
            }
        }
        _ => return None,
    };
    let mut root_seen = false;
    let mut json_seen = false;
    while let Some(argument) = arguments.get(index) {
        match argument.as_str() {
            "--json" if !json_seen => {
                json_seen = true;
                index += 1;
            }
            "--root" if !root_seen => {
                root_seen = true;
                let path = std::path::Path::new(arguments.get(index + 1)?);
                if !path.is_absolute() {
                    return None;
                }
                index += 2;
            }
            _ => return None,
        }
    }
    Some(request)
}

pub(crate) async fn run(paths: &AppPaths, arguments: &[String]) -> ExitStatus {
    let Some(request) = parse(arguments) else {
        eprintln!("{USAGE}");
        return ExitStatus::Usage;
    };
    let result = execute(paths, request).await;
    match result {
        Ok(value) => {
            if arguments.iter().any(|argument| argument == "--json") {
                println!("{value}");
            } else {
                println!("database backup: {value}");
                println!(
                    "Settings, keys, artifacts and binaries are unchanged. Snapshots are retained in this profile's data/backups directory."
                );
            }
            ExitStatus::Ok
        }
        Err(error) => {
            eprintln!("jarvis backup: {error}");
            ExitStatus::Rejected
        }
    }
}

async fn execute(paths: &AppPaths, request: Request) -> Result<serde_json::Value, BackupError> {
    let mut result = match request {
        Request::Create => {
            serde_json::json!({"operation": "create", "backup": create_database_backup(paths).await?})
        }
        Request::Verify(id) => {
            serde_json::json!({"operation": "verify", "backup": verify_database_backup(paths, id).await?})
        }
        Request::Restore(id) => {
            serde_json::json!({"operation": "restore", "restore": restore_database_backup(paths, id).await?})
        }
        Request::Recover => {
            serde_json::json!({"operation": "recover", "restore": recover_database_restore(paths).await?})
        }
    };
    result["backups_directory"] = serde_json::json!(paths.data().join("backups"));
    result["restore_journal"] = serde_json::json!(paths.state().join("database-restore.json"));
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(text: &str) -> Vec<String> {
        text.split_whitespace().map(str::to_owned).collect()
    }

    #[test]
    fn destructive_selection_requires_an_explicit_backup_identity_and_known_options() {
        let id = BackupId::new();
        assert_eq!(
            parse(&args(&format!("backup restore {id} --json"))),
            Some(Request::Restore(id))
        );
        for input in [
            "backup",
            "backup restore",
            "backup restore ../escape",
            "backup restore latest",
            "backup create extra",
            "backup recover --force",
            "backup create --root relative",
            "backup create --json --json",
        ] {
            assert_eq!(parse(&args(input)), None, "{input}");
        }
        assert_eq!(parse(&args("backup create")), Some(Request::Create));
        assert_eq!(parse(&args("backup recover")), Some(Request::Recover));
    }
}
