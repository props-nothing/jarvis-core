---
integration: source-install-prerequisites
status: researched
last_verified: 2026-10-14
owners: []
selected_spec_version: "repository Rust 1.98.1; current Rust/rustup/Cargo installation docs"
selected_sdk: "none; existing Git, rustup/Cargo and OS developer tools"
---

# Source Install Prerequisites

## Scope

Bounded preflight and failure guidance for the website's existing source-building Bash and PowerShell scripts on Windows, macOS and Ubuntu. Build `jarvis-cli` with the committed lockfile, then invoke the built program's per-user `install`. Print the separate first-run/setup handoff, including unattended use. Do not implement CLI onboarding, install system packages, publish releases, add binary downloads, or claim signed distribution.

## Official Sources

All sources below were accessed on **2026-10-14 (UTC)** before changing installer behavior.

| Source | URL/version | Purpose / observation |
| --- | --- | --- |
| Rust docs index | https://rust-lang.org/tools/install/ | rustup, Cargo location and restarting the shell for PATH |
| Rust installation book (official source) | https://raw.githubusercontent.com/rust-lang/book/main/src/ch01-01-installation.md | C compiler/linker; macOS command-line tools; Ubuntu `build-essential`; Windows Visual Studio prerequisites |
| rustup Windows MSVC | https://rust-lang.github.io/rustup/installation/windows-msvc.html | MSVC linker/libraries, C++ tools and Windows SDK; Visual Studio edition/licensing caveats |
| rustup toolchain file | https://rust-lang.github.io/rustup/overrides.html | repository `rust-toolchain.toml` selects the toolchain |
| Cargo build | https://doc.rust-lang.org/cargo/commands/cargo-build.html | `--locked` fails on missing or changed `Cargo.lock`; it is not an offline/network prohibition |
| PowerShell command discovery | https://learn.microsoft.com/en-us/powershell/module/microsoft.powershell.core/get-command?view=powershell-5.1 | command discovery includes functions/aliases as well as applications; the compiler preflight must filter to `Application` |
| Visual Studio discovery (official source) | https://raw.githubusercontent.com/microsoft/vswhere/main/README.md | installed locator at `%ProgramFiles(x86)%\Microsoft Visual Studio\Installer\vswhere.exe`, VS 2017 15.2+ |
| Visual C++ discovery (official source) | https://raw.githubusercontent.com/wiki/microsoft/vswhere/Find-VC.md | `-latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath` includes Build Tools, without requiring `cl` on ordinary shell PATH |
| Apple current docs | https://developer.apple.com/documentation/xcode/installing-the-command-line-tools | page accessible, but fetched content exposes only its title; detailed commands verified below |
| Apple command-line tools FAQ | https://developer.apple.com/library/archive/technotes/tn2339/_index.html | official archived reference: CLT contains SDK/Clang; `xcode-select --install` and `--print-path`; Xcode tool shims can exist before tools work |
| Rust release page | https://github.com/rust-lang/rust/releases/tag/1.98.1 | fetch returned an error-page message; release availability/deprecation could not be established from it |

`llms.txt` was **not found**: https://rust-lang.org/llms.txt and https://rust-lang.github.io/rustup/llms.txt returned 404. Official installation/index pages and source were used instead. No wire schema or provider SDK is applicable. No vendor implementation was copied.

## Verified Contract

### Operations And Transport

The scripts clone/fetch a caller-selected Git repository/ref, detach at `FETCH_HEAD`, execute `cargo build --release -p jarvis-cli --locked`, and run the resulting program's `install` (optionally `--service`). The website serves `frontend/public/install.sh` and `install.ps1`; these are source installers, not signed-release verifiers.

Rust source builds need the native linker and C/C++ build tools; an already installed JARVIS executable does **not** need Rust to run. On macOS, check the selected developer-tools path rather than trusting the presence of the `cc` shim alone. On Windows, accept `cl` on PATH or a C++ installation reported by the existing Visual Studio locator. The actual Cargo build remains the definitive Windows SDK/linker/target compatibility check. Ubuntu guidance is Git plus `build-essential`, with curl for the website/rustup command.

### Authentication And Authorization

Public Git/rustup/crate downloads need no JARVIS or ElevenLabs key. Private repository overrides use the owner's existing Git authentication; scripts neither collect nor print credentials. Prerequisite hints are suggestions, never administrator/package-manager commands executed by the script. Existing Bash `JARVIS_INSTALL_RUST=1` is explicit consent for per-user rustup installation; default is off. No new auto-install option is introduced on Windows.

New profile setup is a separate CLI operation requiring the owner's ElevenLabs API key. That requirement and default voice ID `liN4AsrhF3CylqMDWK6p` are **JARVIS decisions supplied by the coordinating task**, not facts inferred from Rust documentation. The voice ID is not a key. The installer does not read keys, run `init`, answer prompts or retry failed setup. Handoff uses private environment input or an absolute `--voice-key-file`, never a real key value in command arguments. Folders remain explicit owner grants.

### Limits And Failure Semantics

`--locked` refuses dependency re-resolution rather than silently rewriting `Cargo.lock`; it does not prove signed provenance or an immutable Git ref. Network/proxy errors and unavailable toolchains remain build failures. Downloads/builds use existing Git/Cargo timeout/retry behavior: no new bounded-network guarantee, automatic retry or offline mode is claimed. Mutable `main` remains the default; use `JARVIS_REF` for an explicit ref.

Source checkout origin, dirty-tree/untracked-file, nonrepository-directory and invalid-ref protections remain in place. Preflight refusal must occur before clone/update/build effects. Build/install failures must exit nonzero and must not print success, start setup or silently continue to a daemon. There are no webhooks, pagination or provider request IDs in this integration.

### Data And Compliance

Git, Rust distribution hosts and Cargo registry/git dependency hosts receive ordinary source/toolchain download requests (including network metadata). No model key, voice key, transcript or personal memory is needed for installation. Source/build caches and the per-user executable/PATH are local effects; service registration is opt-in. Builds execute dependency build scripts and are not a sandbox. Optional rustup installation runs downloaded upstream code only after explicit opt-in. Model/voice provider egress belongs to CLI/runtime setup, not this bootstrap.

Distribution/signing/licensing policy is owned by the parent task. Original platform/website code is now Apache-2.0; source installation is still not proof of satisfied dependency-notice, signing, publication or clean-machine release gates.

### Versions And Deprecations

At research time the repository pins **Rust 1.98.1**, `clippy`, `rustfmt` and the `minimal` profile in `rust-toolchain.toml`; that file, not a hardcoded installer version, controls the selected checkout. Cargo is the matching rustup toolchain component. Rustup and OS build tools are owner-installed (not newly pinned/downloaded adapters). PowerShell remains 5.1+ and Bash remains compatible with macOS's system Bash. No relevant installation-contract change was observed in the fetched current documentation. The pinned Rust release's live availability was not verified; it must fail visibly if unavailable.

## JARVIS Mapping

This is a local bootstrap outside model tool execution. Deterministic checks precede source mutation; Git/Cargo/OS errors remain owner-visible. `jarvis install` owns executable/PATH/service effects. CLI setup owns provider validation, private keys, profile state and workspace grants. Bootstrap success means **program installed, setup still to be done**, not a configured/running assistant. No provider SDK types cross a domain boundary.

## Decisions

- Preserve explicit source reuse protections and default service-off behavior.
- Require Cargo via rustup for repository toolchain selection; explain that this is source-only.
- Check macOS developer-tool selection and Windows C++ discovery; let Cargo diagnose final linker/SDK compatibility with actionable hints.
- Use `--locked` in both scripts and the website source command.
- Print interactive and noninteractive next commands without executing onboarding through piped stdin.
- Keep Ubuntu package commands advisory. Keep existing explicit rustup opt-in, never add unsolicited global installs.

## Rejected Alternatives

- A guessed binary download/signature path: no public signed release contract exists.
- Installing Git, Xcode, Visual Studio or Ubuntu packages automatically: privileged/unrequested effects.
- Requiring `cl` on ordinary PowerShell PATH: valid Visual Studio installations are discoverable outside a developer shell.
- Running or ignoring `init` inside `curl | bash` / `irm | iex`: consumed/nonterminal stdin is not safe interactive setup.
- Removing `--locked` to get around build failures: hides source/lockfile inconsistency.

## Verification Plan

**Cheapest falsifier:** use an offline real-Git fixture and stand-in Cargo/binary. Refuse missing prerequisites before source mutation; verify the build receives `--locked`, the binary receives only `install`, and failed builds/installs cannot print success or launch setup.

- Preserve fresh clone/rerun/ref/dirty-tree/foreign-origin tests.
- Add missing-rustup, missing Windows C++ discovery and macOS developer-tools refusal tests, with inert stand-ins.
- Assert successful piped installs print key-safe `init`, `start`, `doctor` handoff and do not run those commands.
- Run existing Bash/PowerShell syntax checks and frontend tests/typecheck/build.
- Native clean-machine Windows/macOS/Ubuntu builds and real CLI onboarding are separate live checks; stand-ins and Git Bash do not prove them.
- Auth refresh/revocation, webhook replay and provider rate limits are not applicable to this bootstrap; private Git credentials and real provider keys are not exercised.

## Unresolved Questions

- Does pinned Rust 1.98.1 download/build on all supported clean machines? No live source build was attempted here; blocks a clean-machine claim, not explicit preflight/failure guidance.
- Does every Windows SDK/ARM64/custom Visual Studio arrangement work? Locator checks are conservative; a compiler on PATH is an escape for custom layouts. Final target/SDK validity requires a native Cargo build.
- Are website production routes reachable for the final deployed revision? Repository route/script evidence exists; this task does not publish or verify deployment.
- Are public redistribution/signing prerequisites resolved? Parent-owned policy; blocks public binary claims/download implementation.

## Verification Log

| Date | Versions checked | Evidence | Researcher |
| --- | --- | --- | --- |
| 2026-10-14 | Repository Rust 1.98.1; current rustup/Cargo/Rust docs; VS locator for 2017 15.2+ | Official prerequisite/toolchain/locked-build contracts read before implementation; release page inconclusive; no clean-machine install claim | Copilot |
| 2026-10-14 | Windows PowerShell 5.1 and Git Bash; Node 24.19.0 | From `frontend`: `node --test --test-name-pattern='powershell: (missing source\|discover C)' tests\installer.test.mjs` passed 2 tests; `npm test` passed 38/38; `npm run check:install` passed both parsers/contracts; `npm run build` passed typecheck/Vite with a nonfatal chunk-size warning. Initial Windows prerequisite fixtures exposed host VS discovery leaking into the test environment; explicit fixture-only ProgramFiles assignment fixed it. No actual JARVIS source build, OS package install, service registration, live model/voice call or native macOS/Ubuntu run performed. | Copilot |
