---
integration: rpassword
status: implemented-minimal
last_verified: 2026-10-14
owners: []
selected_spec_version: "Rust console API, rpassword 7.5.4"
selected_sdk: "rpassword =7.5.4"
---

# Hidden terminal key entry

## Scope

Read model and ElevenLabs API keys during CLI setup and `jarvis keys set` without echoing them. No OS credential-store change, network protocol, or background terminal interaction.

## Official Sources

| Source | URL/version | Accessed | Purpose |
| --- | --- | --- | --- |
| Index | https://raw.githubusercontent.com/conradkleinespel/rpassword/master/llms.txt (404; not found) | 2026-10-14 | discovery |
| Version metadata | https://crates.io/api/v1/crates/rpassword | 2026-10-14 | latest stable 7.5.4, Apache-2.0 |
| Versioned API | https://docs.rs/rpassword/7.5.4/rpassword/ | 2026-10-14 | `prompt_password`, configurable test readers |
| Versioned source | https://docs.rs/crate/rpassword/7.5.4/source/src/lib.rs | 2026-10-14 | source index |
| Official source | https://github.com/conradkleinespel/rpassword/blob/11515101cf108ce825dcd5ef5bac703a591f36c1/src/lib.rs | 2026-10-14 | prompt opens TTY, returns I/O failure, no echoed fallback |
| Manifest | https://github.com/conradkleinespel/rpassword/blob/11515101cf108ce825dcd5ef5bac703a591f36c1/Cargo.toml | 2026-10-14 | version 7.5.4; Rust 1.85; libc/windows-sys/rtoolbox |
| Upgrade policy | https://github.com/conradkleinespel/rpassword/blob/11515101cf108ce825dcd5ef5bac703a591f36c1/UPGRADE.md | 2026-10-14 | semver compatibility; no standalone changelog found |

## Verified Contract

### Operations And Transport

`prompt_password` writes the prompt to the terminal and reads from the TTY with feedback disabled by default. It returns `io::Result<String>`. Custom `ConfigBuilder` readers and discarded output support offline tests. There is no HTTP API, schema, pagination, webhook or retry contract.

### Authentication And Authorization

No library authentication. JARVIS only prompts when stdin is a terminal. Noninteractive setup supplies keys through an absolute file reference or environment variable, never a command-line key. Terminal failure must refuse, never fall back to an echoed reader.

### Limits And Failure Semantics

JARVIS validates trimmed keys as nonempty printable ASCII, at most 256 bytes. Cancellation/I/O errors produce a fixed redacted message. The library does not zeroize returned strings; no such guarantee is claimed.

### Data And Compliance

The library performs no network calls. Keys stay in process memory until written to private profile files; only references enter configuration. Provider tests use fixed synthetic text, may incur provider credits, and never log raw responses or keys.

### Versions And Deprecations

Pin 7.5.4, Apache-2.0, Rust 1.85 minimum (compatible with repository Rust 1.98.1). Use the current nondeprecated `prompt_password`, not deprecated BufRead wrappers. No relevant change observed in the upgrade policy.

## JARVIS Mapping

CLI adapter only; no provider types cross domain interfaces. Key validation uses `jarvis_storage::settings::validate_secret`. No folder permission is implied by key entry. No additional approval mechanics.

## Decisions

Use the pinned crate rather than handwritten unsafe platform terminal code. Reuse hidden entry for setup and key replacement. Do not persist untested setup keys/configuration.

## Rejected Alternatives

Echoed stdin as an error fallback; raw key flags; platform-specific terminal shell scripts; prompting in noninteractive processes.

## Verification Plan

Cheapest falsification: a failed setup provider check must leave a fresh profile without configuration or secret files; a malformed key must not appear in an error. Exercise file/environment noninteractive paths and mocked provider refusals offline. Manually verify hidden entry in native Windows, macOS and Ubuntu terminals; not proven by a fixture. No live credential is used by automated tests.

## Unresolved Questions

Native hidden-entry/terminal-restoration checks remain unperformed on macOS and Ubuntu. This blocks claims of clean-machine validation, not the cross-platform implementation. Zeroization is not provided.

## Verification Log

| Date | Versions checked | Relevant change or no-change evidence | Researcher |
| --- | --- | --- | --- |
| 2026-10-14 | rpassword 7.5.4, official source at 11515101 | Hidden TTY API and platform dependencies confirmed; crate pinned | Copilot |
