//! Owner-invoked signed binary maintenance; policy lives in jarvis-update.

use crate::output::ExitStatus;
use std::path::PathBuf;

const USAGE: &str = "usage: jarvis update stage TAG VERSION --dir ABSOLUTE_DIR\n       jarvis update activate ID --dir ABSOLUTE_DIR [--root PROFILE_ROOT]\n       jarvis update rollback|recover|repair|status --dir ABSOLUTE_DIR [--root PROFILE_ROOT]\n       jarvis update probe\nStop JARVIS before maintenance and use the retained updater path printed by stage.";

#[derive(Debug, PartialEq)]
enum Operation {
    Stage(String, String),
    Activate(jarvis_core::UpdateId),
    Rollback,
    Recover,
    Repair,
    Status,
    Probe,
}

fn parse(args: &[String]) -> Option<(Operation, Option<PathBuf>, Option<PathBuf>)> {
    let mut index = 2;
    let operation = match args.get(1)?.as_str() {
        "stage" => {
            index = 4;
            Operation::Stage(args.get(2)?.clone(), args.get(3)?.clone())
        }
        "activate" => {
            index = 3;
            Operation::Activate(args.get(2)?.parse().ok()?)
        }
        "rollback" => Operation::Rollback,
        "recover" => Operation::Recover,
        "repair" => Operation::Repair,
        "status" => Operation::Status,
        "probe" if args.len() == 2 => return Some((Operation::Probe, None, None)),
        _ => return None,
    };
    let mut dir = None;
    let mut root = None;
    while let Some(flag) = args.get(index) {
        let path = PathBuf::from(args.get(index + 1)?);
        if !path.is_absolute() {
            return None;
        }
        match flag.as_str() {
            "--dir" if dir.is_none() => dir = Some(path),
            "--root" if root.is_none() && !matches!(operation, Operation::Stage(..)) => {
                root = Some(path);
            }
            _ => return None,
        }
        index += 2;
    }
    dir.as_ref()?;
    Some((operation, dir, root))
}

pub(crate) async fn run(args: &[String]) -> ExitStatus {
    let Some((operation, dir, root)) = parse(args) else {
        eprintln!("{USAGE}");
        return ExitStatus::Usage;
    };
    let result = execute(operation, dir, root).await;
    match result {
        Ok(value) => {
            println!("{value}");
            ExitStatus::Ok
        }
        Err(error) => {
            eprintln!("jarvis update: {error}");
            ExitStatus::Rejected
        }
    }
}

async fn execute(
    operation: Operation,
    dir: Option<PathBuf>,
    root: Option<PathBuf>,
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    if operation == Operation::Probe {
        return Ok(serde_json::to_value(jarvis_update::probe())?);
    }
    let dir = dir.ok_or("missing installation directory")?;
    if let Operation::Stage(tag, version) = operation {
        let stage = jarvis_update::stage(&dir, &tag, &version).await?;
        return Ok(
            serde_json::json!({ "stage": stage, "updater": jarvis_update::helper_path(&dir, stage.id), "next": "Stop JARVIS, then run the updater with: update activate ID --dir INSTALL_DIR [--root PROFILE_ROOT]" }),
        );
    }
    if operation == Operation::Status {
        return Ok(serde_json::json!({ "update": jarvis_storage::binary_update_status(&dir)? }));
    }
    let paths = match root {
        Some(root) => jarvis_storage::AppPaths::from_root(&root)?,
        None => jarvis_storage::AppPaths::resolve_native()?,
    };
    let record = match operation {
        Operation::Activate(id) => jarvis_update::activate(&dir, &paths, id).await?,
        Operation::Rollback => jarvis_update::rollback(&dir, &paths).await?,
        Operation::Recover => jarvis_update::recover(&dir, &paths).await?,
        Operation::Repair => jarvis_update::repair(&dir, &paths).await?,
        _ => return Err("invalid operation".into()),
    };
    Ok(
        serde_json::json!({ "update": record, "next": "Start JARVIS explicitly and check health; binary maintenance did not restore the database or start services." }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn explicit_selection_rejects_unknown_or_ambiguous_arguments() {
        for input in [
            "update",
            "update stage latest",
            "update activate ../escape",
            "update rollback --force",
            "update probe extra",
            "update status --dir relative",
        ] {
            assert!(
                parse(
                    &input
                        .split_whitespace()
                        .map(str::to_owned)
                        .collect::<Vec<_>>()
                )
                .is_none(),
                "{input}"
            );
        }
        assert_eq!(
            parse(&["update".into(), "probe".into()]),
            Some((Operation::Probe, None, None))
        );
    }
}
