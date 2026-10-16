# Onboarding, settings and distribution

Reviewed 2026-10-16. This document separates the current setup from the target experience for a person who has never
edited a configuration file. The [active backlog](../../TODO.md) controls remaining acceptance and execution order;
the original `P9-012`–`P9-019` sequence is historical. The target is setup without manual configuration editing.
Today's source installation still needs build tools, and new CLI onboarding requires a working ElevenLabs key.

## The experience we want

1. **Get it.** Build from source with the website bootstrap script, or extract a CI archive. It contains one `jarvis` program; `jarvis daemon` is its daemon mode. Public signed downloads remain a release gate.
2. **Run it.** Start `jarvis` in a terminal for guided first setup, or use `jarvis init` with explicit private key-file references for unattended setup. Later launches reuse the saved profile.
3. **Answer a few questions, once.** Choose the same provider presets as Settings (local Ollama, Ollama Cloud, OpenAI,
   Gemini, Anthropic's compatibility layer, or another OpenAI-compatible server). Choose a model from the provider's
   list, or enter a name when listing is unavailable. Keys are hidden while typed; the model must answer a short test.
   **The owner-selected first-run flow requires an ElevenLabs API key** with TTS access and a successful short voice test.
   Default voice ID: `liN4AsrhF3CylqMDWK6p` (an identifier, not a key); override with `--voice-id` if the account cannot
   access it. Tests send fixed synthetic text and may consume credits. Failed checks leave setup unsaved.
   Existing `Development`, `Projects` and `code` folders under the home directory are suggestions, not grants: enter
   each path/number explicitly, or finish with none. Do not grant the whole home directory just to enable coding.
4. **It starts and opens.** The daemon starts in the background and the console opens in the browser, already signed in.
5. **Change anything later in Settings** (in the console) or with a command (below). Never by re-running setup and losing the
   rest of the configuration.
6. **It keeps running.** Optional autostart at login; `jarvis stop` and `jarvis restart` do what they say.

A professional user gets the same controls as commands. Data-reporting commands offer `--json`; use explicit input flags for unattended setup and key entry:

| Need | Command |
| --- | --- |
| Set up non-interactively | `jarvis init --base-url ... --model ... --api-key-file ABSOLUTE_PATH --voice-key-file ABSOLUTE_PATH [--workspace ...]`; `ELEVENLABS_API_KEY` can supply the voice key instead |
| See settings (secrets shown only as set / not set) | `jarvis config show [--json]` |
| Change one setting | `jarvis config set <key> <value>`, validated by the daemon's own parser |
| Set, test, or remove a key | `jarvis keys set model\|voice`, `jarvis keys test model\|voice`, `jarvis keys remove voice` |
| Start, stop, restart, status | `jarvis start [--no-open]`, `jarvis stop`, `jarvis restart`, `jarvis status` (all exist) |
| Run at login | `jarvis service install\|uninstall` |
| Diagnose | `jarvis doctor` checks the installation; `jarvis doctor --live` also checks configured model access, folders, Docker and the running daemon. |

## Where it is today

| Step | Today | Verdict |
| --- | --- | --- |
| Get it | `cargo build --release -p jarvis-cli` gives the single `jarvis` program (daemon mode: `jarvis daemon`), and CI packages it with the Apache-2.0 license, attribution and a checksum (`P9-017a`). Website bootstrap scripts build from source; published downloads, signing and updates still need release gates (`P9-017b`, `P9-004` to `P9-007`). | **Partial** |
| Run it | `jarvis` with no arguments sets up on first run, starts its own daemon mode and opens the console. First run needs a terminal; `jarvis init` with explicit references also works noninteractively. Later launches reuse saved configuration. | **Built** |
| Pick a brain | **In the console:** Settings, Brain has a card for the main model and one for the fallback, with provider presets (Ollama on this computer, Ollama Cloud, OpenAI, Google Gemini, Anthropic, other), the provider's own list of models to choose from, a **Test** that makes one real call and says what failed, and a fallback at a different provider (`ADR-0163`, `ADR-0164`). **In the terminal:** `init` now reuses the console presets and bounded model-list/completion checks, with hidden key entry, explicit file/env alternatives and required ElevenLabs voice verification (`ADR-0214`). | **Built; hosted-account/native-terminal checks pending** |
| Keys | `jarvis keys status\|set\|remove\|test` for the model and voice keys: written to a private file, never printed, read from an environment variable, a file or standard input (never an argument), and tested against the provider. Terminal key entry is hidden using pinned `rpassword`; a terminal error refuses rather than echoing the secret. | **Built (`P9-014`, `ADR-0214`)** |
| Change a setting | `jarvis config show\|get\|set\|unset` changes one setting at a time, validated by the daemon's own parser, leaving the file untouched on refusal; no more `init --force` to change one thing. Comments in a hand-edited file are not preserved (the file is rewritten from its parsed form). | **Done (`P9-014`)** |
| Settings screen | **Settings** opens on **Overview**. **Apps & tools** groups service setup, files/coding and custom MCP servers; **Channels** contains Telegram and Phone; **External access** controls other apps calling JARVIS. Search finds settings/help, provider keys are write-only, drafts survive section changes and the restart footer reflects saved changes. Configuration status is separate from a live provider test. See the [current settings guide](../user/settings.md) and ADR-0225. | **Built; provider and accessibility acceptance remains path-specific** |
| Stop / restart | `jarvis stop` and `jarvis restart` (an authenticated `POST /api/v1/shutdown` that runs the daemon's normal graceful shutdown, then wait for the port). `jarvis start` also no longer hangs a script that captures its output. | **Done (`P9-012`)** |
| Autostart | `jarvis service install\|uninstall` applies per-user platform registration; `jarvis install --service` installs and registers the program. Native clean-machine end-to-end evidence remains a gate. | **Built, not proven on every clean platform** |
| Diagnose | `jarvis doctor` checks the installation; `jarvis doctor --live` also asks the model server (key accepted? model listed?), checks folders, Docker and the running daemon, each with its fix. | **Done (`P9-016`)** |
| Voice | New CLI setup requires a working ElevenLabs key and uses `liN4AsrhF3CylqMDWK6p` by default. Existing/headless profiles and later key removal still permit browser voice; no runtime cloud dependency was added. | **Built; exact default voice needs owner-account evidence** |
| Docs | Separate [user quick start](../user/quick-start.md), [settings reference](../user/settings.md) and [development guide](../development/getting-started.md). | **Built** |

## Delivery status and remaining work

The [dated P9 evidence](../history/todo-p9-p11-evidence-through-2026-10-16.md) preserves the original slices and their checks. It is not a second task queue. Current scope is:

| IDs | Current disposition |
| --- | --- |
| `P9-012`–`P9-016` | Stop/restart, guided setup, key/config controls, Settings and diagnostics are built. Native-terminal, selected-provider, recovery and clean-machine acceptance remain in TODO. Live settings do not require a restart; the footer identifies changes that do. |
| `P9-017`, `P9-004`–`P9-007` | CI archives contain one `jarvis` executable, which also runs `jarvis daemon`, plus license/attribution/checksum files. Signed public packages, complete notices/SBOM, update/rollback and clean-machine acceptance remain. No sibling `jarvisd` binary is required by the installed CLI. |
| `P9-018`, `P9-003` | Per-user service install/uninstall is built. Native service/manual-daemon ownership and lifecycle acceptance remain. |
| `P9-019`, `P9-001` | Tauri is parked until a named tray or browser-independent local-voice requirement needs a shell; it is not the next required distribution step (ADR-0167). |

The `P9-013` setup slice recorded ten new offline checks alongside its existing init tests. That evidence does not close hosted-provider, exact default-voice, native hidden-entry or clean-machine acceptance; see [ADR-0214](../adr/0214-first-run-tests-model-and-required-voice-before-saving.md).

## Principles for these slices

- One validation path for setup, `config set` and the Settings screen, so a setting cannot be valid in one place and not another.
- A secret is written to a private file and referenced by path; it is never in the configuration document, a log line, an error,
  an API response or a command-line argument. The console and the CLI show only whether it is set.
- Setup never loses configuration: changing one thing changes one thing.
- Use defaults for ordinary choices; never invent credentials or grant suggested folders automatically. New setup requires a successful model test and an owner-supplied ElevenLabs TTS key/test before saving (ADR-0214), even when the model is local.
- Approvals stay light (`ADR-0136`): none of this adds steps to answering a yes or no.
