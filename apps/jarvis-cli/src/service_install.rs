//! `jarvis service install` and `jarvis service uninstall`: run JARVIS as a per-user service (`P9-018`).
//!
//! The definition (a systemd unit, a launchd agent, a Windows logon entry) is written to the plan's per-user path and the
//! platform manager is told about it with a fixed list of commands (`ServicePlan::install_steps`). A definition this
//! program did not write is never replaced or removed: ownership is decided by the marker inside it.

use std::process::{Command, Stdio};

use jarvis_diagnostics::{
    OWNERSHIP_MARKER, ServiceCommand, ServiceDrift, ServicePlan, detect_drift,
};

use crate::output::ExitStatus;

fn run(step: &ServiceCommand) -> Result<(), String> {
    println!("  - {}", step.purpose);
    let output = Command::new(&step.program)
        .args(&step.args)
        .stdin(Stdio::null())
        .output()
        .map_err(|error| format!("{} could not be run: {error}", step.program))?;
    if output.status.success() {
        return Ok(());
    }
    let detail = String::from_utf8_lossy(&output.stderr);
    Err(format!(
        "{} {} failed: {}",
        step.program,
        step.args.join(" "),
        detail.trim()
    ))
}

/// `jarvis service install`
pub fn install(plan: &ServicePlan) -> ExitStatus {
    match detect_drift(plan) {
        ServiceDrift::Foreign { path } => {
            eprintln!(
                "jarvis: {} exists and was not written by JARVIS, so it was left alone",
                path.display()
            );
            return ExitStatus::Rejected;
        }
        ServiceDrift::Unreadable { detail } => {
            eprintln!("jarvis: the existing service definition could not be read: {detail}");
            return ExitStatus::Rejected;
        }
        ServiceDrift::Absent | ServiceDrift::Current | ServiceDrift::Drifted { .. } => {}
    }
    if let Some(parent) = plan.definition_path.parent()
        && let Err(error) = std::fs::create_dir_all(parent)
    {
        eprintln!("jarvis: {} could not be created: {error}", parent.display());
        return ExitStatus::Internal;
    }
    if let Err(error) = std::fs::write(&plan.definition_path, plan.render()) {
        eprintln!("jarvis: the service definition could not be written: {error}");
        return ExitStatus::Internal;
    }
    println!("wrote {}", plan.definition_path.display());
    for step in plan.install_steps() {
        if let Err(message) = run(&step) {
            if step.tolerate_failure {
                println!("    (skipped: {message})");
            } else {
                eprintln!("jarvis: {message}");
                eprintln!(
                    "jarvis: the definition was written; fix the problem above and run `jarvis service install` again"
                );
                return ExitStatus::Unavailable;
            }
        }
    }
    if cfg!(windows) {
        println!("installed: JARVIS starts when you log in (start it now with `jarvis start`).");
    } else {
        println!("installed: JARVIS starts at login and is restarted if it stops.");
    }
    ExitStatus::Ok
}

/// `jarvis service uninstall`
pub fn uninstall(plan: &ServicePlan) -> ExitStatus {
    uninstall_with(plan, run, manager_absent)
}

fn uninstall_with(
    plan: &ServicePlan,
    run_step: impl Fn(&ServiceCommand) -> Result<(), String>,
    absent: impl Fn(&ServicePlan) -> Result<bool, String>,
) -> ExitStatus {
    match detect_drift(plan) {
        ServiceDrift::Absent => {
            if absent(plan) == Ok(true) {
                println!("not installed");
                return ExitStatus::Ok;
            }
            eprintln!(
                "jarvis: service definition is absent but manager absence is not verified; reconcile it before removal"
            );
            return ExitStatus::Rejected;
        }
        ServiceDrift::Foreign { path } => {
            eprintln!(
                "jarvis: {} was not written by JARVIS (it has no {OWNERSHIP_MARKER} marker), so it was left alone",
                path.display()
            );
            return ExitStatus::Rejected;
        }
        ServiceDrift::Unreadable { detail } => {
            eprintln!("jarvis: the service definition could not be read: {detail}");
            return ExitStatus::Rejected;
        }
        ServiceDrift::Drifted { .. } => {
            eprintln!(
                "jarvis: service definition differs from this installation; reconcile it before removal"
            );
            return ExitStatus::Rejected;
        }
        ServiceDrift::Current => {}
    }
    if let Err(message) = absent(plan) {
        eprintln!("jarvis: {message}");
        return ExitStatus::Rejected;
    }
    for step in plan.uninstall_steps() {
        if let Err(message) = run_step(&step) {
            // launchctl unload and reg delete also fail when already absent.
            // A failed disable must never leave an enabled systemd link behind.
            if plan.kind == jarvis_diagnostics::ServiceKind::SystemdUser || absent(plan) != Ok(true)
            {
                eprintln!("jarvis: {message}; service removal did not verify");
                return ExitStatus::Unavailable;
            }
        }
    }
    if plan.definition_path.exists()
        && let Err(error) = std::fs::remove_file(&plan.definition_path)
    {
        eprintln!(
            "jarvis: {} could not be removed: {error}",
            plan.definition_path.display()
        );
        return ExitStatus::Internal;
    }
    for step in plan.after_uninstall_steps() {
        if let Err(message) = run_step(&step) {
            eprintln!("jarvis: {message}; service removal did not verify");
            return ExitStatus::Unavailable;
        }
    }
    if absent(plan) != Ok(true) {
        eprintln!("jarvis: the service manager still has the job or could not be checked");
        return ExitStatus::Unavailable;
    }
    println!(
        "uninstalled: JARVIS no longer starts at login (a running copy keeps running; stop it with `jarvis stop`)."
    );
    ExitStatus::Ok
}

fn manager_absent(plan: &ServicePlan) -> Result<bool, String> {
    use jarvis_diagnostics::ServiceKind;
    let mut command = match plan.kind {
        ServiceKind::SystemdUser => {
            let mut c = Command::new("systemctl");
            c.args([
                "--user",
                "show",
                "jarvisd.service",
                "--property=LoadState",
                "--value",
            ]);
            c
        }
        ServiceKind::LaunchdAgent => {
            let mut c = Command::new("launchctl");
            c.arg("list");
            c
        }
        ServiceKind::WindowsUserLauncher => {
            let steps = plan.install_steps();
            let expected = steps[0]
                .args
                .windows(2)
                .find(|pair| pair[0] == "/d")
                .ok_or("missing Windows launcher command")?[1]
                .replace('\'', "''");
            let script = format!(
                "$ErrorActionPreference='Stop'; $k=[Microsoft.Win32.Registry]::CurrentUser.OpenSubKey('Software\\Microsoft\\Windows\\CurrentVersion\\Run'); \
                 if($null -eq $k){{'absent'; exit 0}}; try {{ \
                 if($k.GetValueNames() -notcontains 'JARVIS'){{'absent'}} \
                 elseif($k.GetValueKind('JARVIS') -eq [Microsoft.Win32.RegistryValueKind]::String -and $k.GetValue('JARVIS') -ceq '{expected}'){{'current'}} \
                 else {{throw 'JARVIS login entry belongs to another command; left unchanged'}} \
                 }} finally {{$k.Dispose()}}"
            );
            let mut c = Command::new("powershell.exe");
            c.args(["-NoProfile", "-NonInteractive", "-Command", &script]);
            c
        }
    };
    let output = command
        .stdin(Stdio::null())
        .output()
        .map_err(|e| e.to_string())?;
    let text = String::from_utf8_lossy(&output.stdout);
    match plan.kind {
        ServiceKind::SystemdUser
            if text.trim() == "not-found"
                && (output.status.success() || output.status.code() == Some(4)) =>
        {
            Ok(true)
        }
        ServiceKind::SystemdUser if output.status.success() => Ok(false),
        ServiceKind::LaunchdAgent if output.status.success() => Ok(!text
            .lines()
            .any(|line| line.split_whitespace().last() == Some("dev.jarvis.jarvisd"))),
        ServiceKind::WindowsUserLauncher if output.status.success() => match text.trim() {
            "absent" => Ok(true),
            "current" => Ok(false),
            _ => Err("invalid login entry check".into()),
        },
        _ => Err(
            "service manager lookup failed or login entry ownership differs; left unchanged".into(),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn failed_cleanup_keeps_definition_and_cannot_report_success() {
        let root =
            std::env::temp_dir().join(format!("service-remove-{}", jarvis_core::scratch_tag()));
        std::fs::create_dir(&root).unwrap_or_else(|e| panic!("{e}"));
        let plan = ServicePlan {
            kind: jarvis_diagnostics::ServiceKind::SystemdUser,
            definition_path: root.join("unit"),
            binary_path: root.join("jarvis"),
            root: None,
            root_argument: None,
        };
        std::fs::write(&plan.definition_path, plan.render()).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(
            uninstall_with(
                &plan,
                |_| Err("injected manager failure".into()),
                |_| Ok(false)
            ),
            ExitStatus::Unavailable
        );
        assert_eq!(
            std::fs::read_to_string(&plan.definition_path).unwrap_or_default(),
            plan.render()
        );
        std::fs::write(&plan.definition_path, "foreign").unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(
            uninstall_with(&plan, |_| panic!("must not run"), |_| Ok(true)),
            ExitStatus::Rejected
        );
        jarvis_core::remove_scratch_dir(&root);
    }
}
