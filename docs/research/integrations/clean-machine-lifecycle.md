# Clean-machine lifecycle

- Last verified: 2026-10-18 UTC.
- Scope: offline repair/removal of the installed executable, native service cleanup,
  and acceptance using published binaries without Rust, Node or Python on the target.
- Versions: repository Rust 1.98.1; existing Windows PowerShell 5.1, per-user systemd
  and launchd contracts. No new SDK or dependency.

## Authority checked before implementation

- [Windows DeleteFileW](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-deletefilew):
  deletion can fail for open/mapped files; a return value must be checked. Use a
  separate retained updater, not a claimed successful self-removal.
- [RegistryKey.GetValue](https://learn.microsoft.com/en-us/dotnet/api/microsoft.win32.registrykey.getvalue?view=netframework-4.8.1)
  and [reg delete](https://learn.microsoft.com/en-us/windows-server/administration/windows-commands/reg-delete):
  inspect only the named HKCU JARVIS value, require exact command/type ownership,
  and verify absence after deletion. A lookup/deletion error is not absence.
- [Rust remove_file](https://doc.rust-lang.org/std/fs/fn.remove_file.html): platform
  filesystem deletion and its errors. The versioned 1.98.0 page was unavailable;
  verify behavior through the repository-pinned compiler and native tests.
- [systemctl](https://www.freedesktop.org/software/systemd/man/latest/systemctl.html):
  disable with `--now` also stops the unit; reload the manager after removing a unit.
- [Apple launchd jobs](https://developer.apple.com/library/archive/documentation/MacOSX/Conceptual/BPSystemStartup/Chapters/CreatingLaunchdJobs.html):
  per-user agents belong to the logged-in user's launchd context. Removing a plist
  alone does not prove a loaded job is gone.
- [GitHub hosted runners](https://docs.github.com/en/actions/reference/runners/github-hosted-runners):
  fresh hosted VMs have preinstalled development software; macOS ARM runners do not
  support nested virtualization. A fresh runner or restricted PATH is not a clean OS.
- Index attempts: `https://docs.github.com/llms.txt` returned an unsupported content
  type in the reader; `https://learn.microsoft.com/llms.txt` was inaccessible. Used
  the exact official contracts above, not search summaries or third-party examples.

## Boundary, authentication and limits

Use existing per-user OS authority and installation/profile locks; no elevation,
machine-wide deletion, new network protocol or native credential API. Repair uses
ADR-0242 retained identities and the existing signed-manifest verifier for a candidate.
It never runs damaged bytes or replaces the database. Keep the 256 MiB executable
bound. Removal retains settings, keys, history and recovery evidence and deletes only
the exact installation executable after service/PATH cleanup succeeds. No recursive
directory deletion. Failed cleanup remains an error with evidence available for retry.

Live onboarding continues ADR-0214: a model completion and ElevenLabs audio test may
consume credits; use test-account credentials supplied to the clean host, never logs,
command-line values or repository files. No provider adapter changes are proposed.

## Discriminating verification and unresolved access

Test stopped/active owners, missing/damaged target repair, altered retained image,
pending journal, profile mismatch, repeated removal, foreign service and cleanup
failure. Remove a guard deliberately and require its negative test to fail.
Then run native CI and the published-release lifecycle; these remain separate from
clean-machine onboarding and reboot/logon evidence.

Repository infrastructure discovery on 2026-10-18 found zero self-hosted runners,
zero Actions secrets and zero deployment environments. After discovery, the owner
chose to run the clean-machine tests personally and enter keys in hidden prompts.
Local virtualization inventory found Docker Desktop WSL only, not clean Windows,
macOS and Linux guest machines. Do not install a development runtime on a target or
label a provider-free profile as completed onboarding. Native certificates remain
unavailable as previously recorded.

The shell drivers therefore support interactive `init` without key files; optional
batch key-file arguments remain available. They copy the verified activated candidate
into a separate maintenance helper, so an older compatible baseline helper need not
already implement repair. No provider protocol or onboarding policy was changed.
