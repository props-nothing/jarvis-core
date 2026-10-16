//! Native process-tree probes shared by the command and managed-process tests.

#![allow(clippy::unwrap_used)]

use std::path::Path;
use std::time::Duration;

use serde_json::{Value, json};

/// Prepares a root and child which live until the test releases the root.
pub(crate) fn command(folder: &Path) -> Value {
    #[cfg(windows)]
    {
        std::fs::write(
            folder.join("child.ps1"),
            r"param([string]$Folder)
[System.IO.File]::WriteAllText((Join-Path $Folder 'child-ready'), [string]$PID)
Start-Sleep -Seconds 60
",
        )
        .unwrap();
        std::fs::write(folder.join("root.ps1"), r#"param([string]$Folder)
$info = New-Object System.Diagnostics.ProcessStartInfo
$info.FileName = (Get-Process -Id $PID).Path
$info.Arguments = '-NoProfile -NonInteractive -File "' + (Join-Path $Folder 'child.ps1') + '" "' + $Folder + '"'
$info.UseShellExecute = $false
$info.CreateNoWindow = $true
$child = [System.Diagnostics.Process]::Start($info)
[System.IO.File]::WriteAllText((Join-Path $Folder 'pids'), ([string]$PID + ' ' + [string]$child.Id))
$until = [DateTime]::UtcNow.AddSeconds(60)
while (-not (Test-Path -LiteralPath (Join-Path $Folder 'release')) -and [DateTime]::UtcNow -lt $until) { Start-Sleep -Milliseconds 20 }
"#).unwrap();
        json!({"program":"powershell", "arguments":["-NoProfile","-NonInteractive","-File",folder.join("root.ps1").to_string_lossy(),folder.to_string_lossy()],"timeout_seconds":30})
    }
    #[cfg(not(windows))]
    {
        std::fs::write(
            folder.join("child.sh"),
            "printf '%s' \"$$\" > \"$1/child-ready\"\nexec sleep 60\n",
        )
        .unwrap();
        std::fs::write(folder.join("root.sh"), "sh \"$1/child.sh\" \"$1\" &\nprintf '%s %s' \"$$\" \"$!\" > \"$1/pids\"\ncount=0\nwhile [ ! -f \"$1/release\" ] && [ \"$count\" -lt 1200 ]; do sleep 0.05; count=$((count+1)); done\n").unwrap();
        json!({"program":"sh", "arguments":[folder.join("root.sh").to_string_lossy(),folder.to_string_lossy()],"timeout_seconds":30})
    }
}

/// Cleanup authority for the two freshly observed fixture processes only. It is
/// disarmed after independent exit checks, and also runs when an assertion fails.
pub(crate) struct LiveTree {
    parent: u32,
    child: u32,
    armed: bool,
}

pub(crate) async fn ready(folder: &Path) -> LiveTree {
    tokio::time::timeout(Duration::from_secs(20), async {
        loop {
            if let Ok(pids) = std::fs::read_to_string(folder.join("pids")) {
                let pids: Vec<u32> = pids
                    .split_whitespace()
                    .filter_map(|pid| pid.parse().ok())
                    .collect();
                if let [parent, child] = pids.as_slice()
                    && std::fs::read_to_string(folder.join("child-ready"))
                        .ok()
                        .and_then(|pid| pid.parse::<u32>().ok())
                        == Some(*child)
                {
                    assert!(
                        *parent > 1
                            && *child > 1
                            && parent != child
                            && *parent != std::process::id()
                            && *child != std::process::id(),
                        "fixture must name distinct child processes"
                    );
                    let tree = LiveTree {
                        parent: *parent,
                        child: *child,
                        armed: true,
                    };
                    assert!(running(tree.parent), "root was never observed alive");
                    assert!(running(tree.child), "descendant was never observed alive");
                    return tree;
                }
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("root and descendant must become ready"))
}

pub(crate) fn running(pid: u32) -> bool {
    #[cfg(windows)]
    {
        let output = std::process::Command::new("tasklist")
            .args(["/FI", &format!("PID eq {pid}"), "/FO", "CSV", "/NH"])
            .output()
            .unwrap();
        assert!(output.status.success());
        String::from_utf8_lossy(&output.stdout).contains(&format!("\"{pid}\""))
    }
    #[cfg(not(windows))]
    {
        // A reparented zombie has stopped executing but may await its new parent's
        // reap. It is not a surviving worker and cannot hold a listening socket.
        let output = std::process::Command::new("ps")
            .args(["-o", "stat=", "-p", &pid.to_string()])
            .output()
            .unwrap();
        let status = String::from_utf8_lossy(&output.stdout);
        !status.trim().is_empty() && !status.trim().starts_with('Z')
    }
}

impl LiveTree {
    pub(crate) async fn assert_gone(mut self) {
        tokio::time::timeout(Duration::from_secs(8), async {
            while running(self.parent) || running(self.child) {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .unwrap_or_else(|_| panic!("neither the root nor its descendant may survive cleanup"));
        self.armed = false;
    }
}

impl Drop for LiveTree {
    fn drop(&mut self) {
        if !self.armed {
            return;
        }
        // Test failure cleanup never reads durable records or enumerates other
        // processes: these IDs were just witnessed in this private fixture.
        for pid in [self.child, self.parent] {
            if !running(pid) {
                continue;
            }
            #[cfg(windows)]
            let _ = std::process::Command::new("taskkill")
                .args(["/PID", &pid.to_string(), "/T", "/F"])
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status();
            #[cfg(not(windows))]
            let _ = std::process::Command::new("kill")
                .args(["-KILL", &pid.to_string()])
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status();
        }
    }
}
