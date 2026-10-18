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

## Implementation evidence (2026-10-18)

- Local Windows workspace: 3,267 tests passed, six existing opt-in ignores; 23
  updater tests and 74 script/documentation tests passed. Formatting, strict
  workspace/all-targets/all-features Clippy, workflow lint and script syntax passed.
  The subsequent Windows PATH canonicalization passed its five focused tests and
  strict Clippy, and is covered by the native lifecycle below.
- Falsification: removing the uninstall target-digest guard made
  `removal_refuses_unknown_bytes_wrong_profile_and_active_owners` fail at the expected
  unknown-image assertion. Restoring the exact source restored the pass. Injected
  service cleanup failure preserves the definition and returns failure.
- [Native lifecycle 39126418196](https://github.com/props-nothing/jarvis-core/actions/runs/39126418196)
  passed Windows x86_64, macOS ARM64 and Linux x86_64 using source `82e27be` and the
  real compatible signed `b9d8773` candidate. All three summaries report signed
  candidate repair, missing/damaged previous-image repair, executable removal,
  repeat uninstall, snapshot verification, data-preserving rollback, recovery,
  service preservation/removal and authenticated readiness. Windows checked seven
  distinct daemon lifecycles; macOS and Linux checked nine. Windows also checked
  removal of the exact installation's persisted PATH entry.
- These are hosted developer runners and provider-free profiles. The clean drivers'
  syntax and developer-host refusal checks pass; their positive live-provider runs
  and actual logon observations remain for the owner's clean machines.
- Final implementation `a176e79843a22eae2802d97aafba74a0bf6e282f` passed all fourteen
  quality jobs in [CI 39126454548](https://github.com/props-nothing/jarvis-core/actions/runs/39126454548).
  Its `.gitattributes` rule preserves the Unix clean-driver line endings on Windows.
- [Release 39127486126](https://github.com/props-nothing/jarvis-core/actions/runs/39127486126)
  completed successfully: five packages, twelve signed-release assets, four public
  first-install jobs and all three extended lifecycle jobs against published
  `ci-a176e79843a22eae2802d97aafba74a0bf6e282f`. Release metadata independently
  confirms exact target SHA, public prerelease/non-draft state and twelve assets.
  The same seven Windows/nine Unix startup counts and all repair/removal/data
  preservation assertions pass. Intel macOS is cross-built; its runtime and Linux
  ARM64 update/repair remain outside these three lifecycle jobs.
- The owner self-test pins signed `b9d8773` as its baseline and signed `a176e79` as
  candidate, both version 0.1.0. Its actual two-signed-build/live-onboarding run is
  still pending. The Windows driver captures configuration after initial startup,
  matching Unix and allowing legitimate first-start initialization before comparing
  preservation across maintenance; syntax and developer-host refusal checks pass.
