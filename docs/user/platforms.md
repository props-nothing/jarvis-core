# Platforms And Deployment

**The same core on three operating systems. Evidence, not assumptions.**

[Documentation](../README.md) / [Quick start](quick-start.md) / [Settings](settings.md) / [Build from source](../development/getting-started.md)

JARVIS is built to run the same way on all three. This page says what that means today, **how each claim was checked**, how to run it
on a server with no screen, and what is still missing. It is deliberately specific about what has and has not been run.

## Support status

| | Windows 10/11 | Linux (x86_64, aarch64) | macOS (Apple silicon, Intel) |
| --- | --- | --- | --- |
| Compiles, clippy clean with warnings denied | yes | **yes** (CI and a local cross-check) | **yes** (local cross-check; CI) |
| Automated tests | pass locally | run in CI on every push | run in CI on every push |
| Run end to end by a person | **yes**, throughout development | **Ubuntu 24.04 x86_64**, recorded 2026-10-11; other targets not yet | not yet |
| Pure-Rust TLS (no OpenSSL to install) | yes | yes | yes |
| Native config/data/log locations | `%APPDATA%`/`%LOCALAPPDATA%` | XDG (`~/.config`, `~/.local/share`) | `~/Library/Application Support` |
| Private key and credential files | owner-only ACL | mode `0600` | mode `0600` |
| Run at login / as a service | registry Run entry (`jarvis service install`) | systemd user unit (`jarvis service install`) | launchd agent (`jarvis service install`) |
| Code sandbox (optional) | Docker Desktop | Docker | Docker Desktop |
| Prebuilt archive | CI builds an unsigned workflow artifact (`release.yml`) | CI builds an unsigned workflow artifact | CI builds unsigned workflow artifacts (Apple silicon and Intel) |
| Signed installers, public downloads | not yet | not yet | not yet (an unsigned binary needs `xattr -d com.apple.quarantine` or a notarized build, `P9-005`) |

Ubuntu 24.04 x86_64 has a recorded source install, local-model run, SSH-console access and service setup (2026-10-11, `P9-022`).
The same run found a service-ownership handoff issue: stop a manually started daemon before installing its service. This is not proof
of a reboot/update cycle, every Linux distribution or aarch64. macOS still needs an end-to-end owner run; CI alone is not that evidence.

The first release workflow run built all five archives on GitHub; its smoke test (unpack, run `jarvis version` and `jarvis daemon --version`) failed
on every platform because of a shell glob in the script itself, since fixed and verified by packaging and running the Windows archive locally.
An earlier macOS CI run also failed at the Phase 3 gate (a real daemon and an MCP child process). These are historical observations, not the status of the current revision; check the [latest native CI runs](https://github.com/props-nothing/jarvis-core/actions/workflows/ci.yml) and [release workflow](https://github.com/props-nothing/jarvis-core/actions/workflows/release.yml) before relying on an artifact.

What running the tests on real Linux and macOS runners has already found (and fixed): a socket path longer than macOS allows
(103 bytes) failed at bind with a message that named nothing, so JARVIS now refuses it by name and says to use `--root` with a short
directory; a sandbox test that assumed a delegated cgroup on any Linux host; and three tests that used Windows-style paths. None of
these was a defect on Windows, which is why only a second platform finds them.

How the first two rows are checked: CI (`.github/workflows/ci.yml`) runs formatting, clippy with warnings denied on all targets, and the
whole test suite on Ubuntu, macOS and Windows; and from a Windows machine the same clippy gate can be run against the Linux and macOS
targets by pointing the C compiler at a stub (the C dependencies are only compiled, never linked, by `cargo check`).

## Source install prerequisites

The [website bootstrap scripts](../operations/install-and-release.md#source-bootstrap-available-today) exist and build from source; they do not download signed binaries. **Git, Rust via rustup and native build tools are source-only requirements.** Once the executable is installed, Rust is not needed to run it.

| Platform | Native source prerequisites / preflight |
| --- | --- |
| Windows | Git for Windows; Visual Studio C++ Build Tools with Desktop development with C++ and a Windows SDK. The script accepts `cl` on PATH or C++ tools reported by the installed VS locator; Cargo verifies the final linker/SDK. |
| macOS | Apple's command-line tools (`xcode-select --install`) or an existing selected Xcode. The script checks `xcode-select --print-path`; the compiler shim alone does not establish a selected toolchain. |
| Ubuntu | Git, `build-essential` and curl. Explicit commands and rustup installation guidance are in [Build from source](../development/getting-started.md#build-from-source). No separate OpenSSL package is required for the CLI's pure-Rust TLS. |

All platforms use the checked-out `rust-toolchain.toml` and `cargo build --release -p jarvis-cli --locked`. Scripts preserve source-origin/local-change protections, fail visibly on build/install errors, never install OS packages, and leave service startup off unless explicitly requested. Bash's existing `JARVIS_INSTALL_RUST=1` is an explicit opt-in to per-user rustup, not the default.

The installer prints next commands rather than running setup through piped input. Open a new terminal and run `jarvis`, or supply private absolute model/voice key files to `jarvis init`; start only after success. New setup requires an ElevenLabs API key and explicit folder grants. Existing profiles and headless runtime remain supported.

Offline installer regression tests use real disposable Git repositories with stand-in tools/binaries. Windows PowerShell and Git Bash runs do not verify a fresh Windows build, macOS or Ubuntu installation, nor do mocked developer tools prove an installed SDK. The [research record](../research/integrations/source-install-prerequisites.md) separates these limits from the earlier platform evidence above.

## A Linux server (a VPS with no screen)

JARVIS listens **only on `127.0.0.1`**. That is deliberate (an exposed assistant is an attack surface) and it means the console is
reached through an SSH tunnel, not a public address. A remote, TLS-terminated mode is a separate, explicit piece of work (`P10-004`);
until it exists, **do not forward the port to the internet**.

```text
# on the server
jarvis init --base-url https://api.example.com/v1 --model NAME --api-key-file /home/me/model.key --voice-key-file /home/me/voice.key --workspace /home/me/work
# Continue only if init succeeded; keys must be existing private absolute files
jarvis start                  # starts in the background and keeps running after you log out
jarvis stop && jarvis service install  # hand ownership to the login service (also enables lingering)
jarvis status
jarvis doctor --live          # is the model reachable and the key accepted? is a folder missing? each finding says how to fix it

# on your own computer
ssh -L 8765:127.0.0.1:8765 me@your-server
#   then on the server: jarvis hud --print-url      (prints the address with your credential)
#   and open that address in your own browser
```

Notes:

- With no display, `jarvis` and `jarvis hud` print the tunnel instructions instead of failing to open a browser.
- The voice (microphone, spoken answers) runs in the browser on your computer, not on the server, so it works through the tunnel.
- A local Ollama on the server works the same as on a desktop; a hosted model needs `--api-key-file`.
- `jarvis service install` writes `~/.config/systemd/user/jarvisd.service` and runs `systemctl --user enable --now`, then
  `loginctl enable-linger` so the service survives the last SSH session ending. If lingering is refused on your host, ask the
  administrator to enable it for your user.
- Only the default profile can be installed as a service; a `--root` portable profile deliberately cannot.

## macOS

- `jarvis` opens the console with `open`. `jarvis service install` writes a launchd agent
  (`~/Library/LaunchAgents/dev.jarvis.jarvisd.plist`) and loads it.
- A downloaded, unsigned build is quarantined by Gatekeeper; until signed and notarized builds exist (`P9-005`), build from source or
  remove the quarantine attribute from both programs.

## Windows

`jarvis service install` adds a per-user Run entry (no administrator rights) that starts the daemon hidden at login; `jarvis service
uninstall` removes it. `jarvis start` launches the daemon so that scripts which capture its output do not hang.

## Known gaps

- CI builds unsigned archives and uses a **draft** GitHub release for tags; drafts are visible only to repository writers. Original JARVIS
  code is now [Apache-2.0](../../LICENSE), but a generated dependency-license inventory, signing/notarization, publication and clean-machine
  verification remain release gates (`P9-004` to `P9-007`, `P9-017b`). Website bootstrap installers build from source, not signed downloads.
  Each archive includes the single `jarvis` program, `LICENSE`, `NOTICE`, provenance and a SHA-256 file. Consult the latest workflow result
  for the revision you intend to use; license selection alone is not release or platform-verification evidence.
- The Linux sandbox backend without a container runtime has no wait-for-exit support recorded in the sandbox research; use the
  Docker-backed code tool.
- [Telegram](telegram.md) provides chat and approvals without exposing the daemon. Optional [phone calls](phone.md) use an ElevenLabs agent and your configured telephone provider; connecting the agent to JARVIS needs the authenticated app endpoint and its documented tunnel setup. Optional push and Google tools remain available. Native Microsoft/GitHub connectors are not shipped; prefer [MCP](mcp.md) where supported.
- Browser-independent local speech is still pending. [Coding-agent delegation](code-agents.md) needs an ACP agent, its login and git; ordinary/background Codex tasks have recorded verification, but console sign-in and real-agent cancellation remain open. Its worktree is not a sandbox.