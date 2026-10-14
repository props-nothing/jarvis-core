# Onboarding, settings and distribution

Reviewed 2026-10-14. This is the target experience for getting from "I have JARVIS" to "I am talking to it", for a
person who has never opened a config file, and the control a professional user keeps. It records what exists today,
what does not, and the ordered slices (`P9-012` to `P9-019`) that close the gap. The product rule that governs all of it:
**the person is never asked to edit a file, run a second program, or understand a port to get a working assistant.**

## The experience we want

1. **Get it.** Build from source with the website bootstrap script, or extract a CI archive. It contains one `jarvis` program; `jarvis daemon` is its daemon mode. Public signed downloads remain a release gate.
2. **Run it.** Double-click `jarvis` (or type `jarvis`). Nothing else is needed.
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

A professional user gets the same things as commands, all with `--json`, none of them interactive unless asked:

| Need | Command |
| --- | --- |
| Set up non-interactively | `jarvis init --base-url ... --model ... --api-key-file ABSOLUTE_PATH --voice-key-file ABSOLUTE_PATH [--workspace ...]`; `ELEVENLABS_API_KEY` can supply the voice key instead |
| See settings (secrets shown only as set / not set) | `jarvis config show [--json]` |
| Change one setting | `jarvis config set <key> <value>`, validated by the daemon's own parser |
| Set, test, or remove a key | `jarvis keys set model\|voice`, `jarvis keys test`, `jarvis keys remove voice` |
| Start, stop, restart, status | `jarvis start [--no-open]`, `jarvis stop`, `jarvis restart`, `jarvis status` (all exist) |
| Run at login | `jarvis service install\|uninstall` |
| Diagnose | `jarvis doctor` (checks the model answers, the key works, Docker for the code tool, the port is free) |

## Where it is today

| Step | Today | Verdict |
| --- | --- | --- |
| Get it | `cargo build --release -p jarvis-cli` gives the single `jarvis` program (daemon mode: `jarvis daemon`), and CI packages it with the Apache-2.0 license, attribution and a checksum (`P9-017a`). Website bootstrap scripts build from source; published downloads, signing and updates still need release gates (`P9-017b`, `P9-004` to `P9-007`). | **Partial** |
| Run it | `jarvis` with no arguments sets up on first run, starts its own daemon mode and opens the console. First run needs a terminal; `jarvis init` with explicit references also works noninteractively. Later launches reuse saved configuration. | **Built** |
| Pick a brain | **In the console:** Settings, Brain has a card for the main model and one for the fallback, with provider presets (Ollama on this computer, Ollama Cloud, OpenAI, Google Gemini, Anthropic, other), the provider's own list of models to choose from, a **Test** that makes one real call and says what failed, and a fallback at a different provider (`ADR-0163`, `ADR-0164`). **In the terminal:** `init` now reuses the console presets and bounded model-list/completion checks, with hidden key entry, explicit file/env alternatives and required ElevenLabs voice verification (`ADR-0214`). | **Built; hosted-account/native-terminal checks pending** |
| Keys | `jarvis keys status\|set\|remove\|test` for the model and voice keys: written to a private file, never printed, read from an environment variable, a file or standard input (never an argument), and tested against the provider. Terminal key entry is hidden using pinned `rpassword`; a terminal error refuses rather than echoing the secret. | **Built (`P9-014`, `ADR-0214`)** |
| Change a setting | `jarvis config show\|get\|set\|unset` changes one setting at a time, validated by the daemon's own parser, leaving the file untouched on refusal; no more `init --force` to change one thing. Comments in a hand-edited file are not preserved (the file is rewritten from its parsed form). | **Done (`P9-014`)** |
| Settings screen | **Settings** in the console header: a side list in four groups with ten tabs (Brain, Voice & search, Folders & code, Permissions; Google, Telegram, Phone; Connect apps; Alerts, Advanced), a dot beside each tab that can be set up (green when set, amber when not), write-only key fields, each setting with a one-line explanation and the rest under **More**, on/off settings as real switches, a provider and model picker with a test (Brain), Google sign-in as one button (`docs/user/google.md`), a folder list and the code sandbox with presets, a filterable per-tool permission list (default / ask / run without asking / off), the daemon's own refusal reasons, and a footer that says which changes still wait for **Restart to apply** (`ADR-0139`, `ADR-0164`, `ADR-0203`). Reference: `docs/user/settings.md`. Voice and search keys now have a Test button that checks the saved key; the result is separate from configuration status. Extra abilities (browser, documentation lookup) are one click in the Abilities tab (`ADR-0169`). | **Done (`P9-015`)** |
| Stop / restart | `jarvis stop` and `jarvis restart` (an authenticated `POST /api/v1/shutdown` that runs the daemon's normal graceful shutdown, then wait for the port). `jarvis start` also no longer hangs a script that captures its output. | **Done (`P9-012`)** |
| Autostart | `jarvis service install\|uninstall` applies per-user platform registration; `jarvis install --service` installs and registers the program. Native clean-machine end-to-end evidence remains a gate. | **Built, not proven on every clean platform** |
| Diagnose | `jarvis doctor` checks the installation; `jarvis doctor --live` also asks the model server (key accepted? model listed?), checks folders, Docker and the running daemon, each with its fix. | **Done (`P9-016`)** |
| Voice | New CLI setup requires a working ElevenLabs key and uses `liN4AsrhF3CylqMDWK6p` by default. Existing/headless profiles and later key removal still permit browser voice; no runtime cloud dependency was added. | **Built; exact default voice needs owner-account evidence** |
| Docs | The only "getting started" is a developer document mixed with the contributor loop. | **Fixed here**: `docs/user/quick-start.md` |

## The slices, in order

Each is a vertical slice with a test that fails closed where a secret or a trust boundary is involved.

- **`P9-012` Stop and restart (done).** An authenticated `POST /api/v1/shutdown` that runs the daemon's existing graceful shutdown;
  `jarvis stop` and `jarvis restart` use it and wait until the port is free. Without this nothing else here is pleasant, because
  every setting change needs a restart.
- **`P9-013` Guided brain and key setup (built, verification partial).** First launch and `jarvis init` share console
  provider presets, model listing, hidden key prompts and a mandatory model/ElevenLabs test before saving (`ADR-0214`).
  Ten new offline checks plus the existing init tests cover refusals/defaults/grants. Hosted-provider accounts,
  the selected voice on an owner account, native hidden-entry checks and clean-machine first-run flows remain open.
- **`P9-014` `jarvis config` and `jarvis keys` (done).** Show, get, set and unset one setting; set, test and remove a key; secrets never
  printed (only "set", length class and last four characters on request); every change validated by the daemon's parser and
  written atomically; `--json` everywhere. This needs a TOML editing approach that preserves comments (an ADR).
- **`P9-015` Settings in the console (done).** A Settings view: brain and model, keys (status, replace, test), voice, folders, which tools
  ask first and what is trusted, with a "restart to apply" button using `P9-012`. Backed by `GET/PUT /api/v1/settings`, which
  **never returns a secret** and accepts a new one write-only. The same validation as `P9-014`, one code path.
- **`P9-016` A real `doctor`.** Adds: the model answers, the key is accepted, the voice key works, Docker is usable for the
  code tool, the port is free, and a browser can be opened; each with the fix to run.
- **`P9-017` One folder to run.** A packaging step producing `jarvis-<version>-<os>-<arch>.zip` or `.tar.gz` (both binaries,
  licence, checksums), a CI release job on the three platforms, and `jarvis` locating `jarvisd` beside itself or on `PATH` with
  a clear message if neither. A second step (`P9-004`) adds the signed installers.
- **`P9-018` Autostart.** `jarvis service install|uninstall` really install the per-user service (`P9-003`), and the console says
  whether it is running at login.
- **`P9-019` The desktop shell.** Tauri wraps the same console (`P9-001`), with a tray icon for stop, restart and "needs you".

Order rationale: `P9-012` unblocks the rest; `P9-013` and `P9-014` fix keys for both audiences; `P9-015` is the screen the user
asked for and depends on both; `P9-016` makes failures understandable; `P9-017` to `P9-019` are distribution.

## Principles for these slices

- One validation path for setup, `config set` and the Settings screen, so a setting cannot be valid in one place and not another.
- A secret is written to a private file and referenced by path; it is never in the configuration document, a log line, an error,
  an API response or a command-line argument. The console and the CLI show only whether it is set.
- Setup never loses configuration: changing one thing changes one thing.
- Every question has a default, so pressing Enter all the way through produces a working assistant when a local model exists.
- Approvals stay light (`ADR-0136`): none of this adds steps to answering a yes or no.