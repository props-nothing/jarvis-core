# Product installation and capability evidence through 2026-10-16

Archived on 2026-10-16 from the implementation ledger. This preserves the original dated claims, failures, corrections and unchecked items; it is **not current delivery status or work order**. Later entries and accepted ADRs may supersede earlier statements. Use the [active backlog](../../TODO.md) and [history index](README.md) for current disposition. Relative Markdown links were adjusted when this material moved.

---

## P9: Desktop, Installation, And Releases

<a id="p9-001"></a>

- [ ] `P9-001` **Demoted (`ADR-0167`, see `P9-019`).** Scaffold Tauri desktop as a daemon client with generated API bindings and a narrowly scoped command allowlist.
      > Status 2026-10-06: an interim browser console ships as the daemon's main page (`P3-036`, ADR-0135): it can talk, watch, answer approvals and stop. The Tauri client remains the planned native client.
<a id="p9-002"></a>

- [ ] `P9-002` Implement onboarding, chat, activity, approvals, connections, memory, tasks, models, runtimes, voice, diagnostics, and settings views.
<a id="p9-003"></a>

- [ ] `P9-003` Implement service install/start/stop/restart/uninstall for Linux systemd user, macOS LaunchAgent, and Windows per-user Scheduled Task; document optional system-wide modes. **Mostly delivered by `P9-018`** (`jarvis service install|uninstall`, per-user, ownership marker, drift detection); what remains is the documented system-wide mode and a check on a clean Linux and macOS machine (`P9-022`).
<a id="p9-004"></a>

- [ ] `P9-004` Build idempotent shell and PowerShell installers with OS/architecture detection, checksum/signature verification, PATH setup, onboarding, and doctor.
<a id="p9-005"></a>

- [ ] `P9-005` Build native signed artifacts for supported targets; generate SBOM, checksums, signatures, provenance, and release notes.
<a id="p9-006"></a>

- [ ] `P9-006` Implement atomic update, schema preflight, backup, health verification, rollback, channels, and split-brain version diagnosis.
<a id="p9-007"></a>

- [ ] `P9-007` Test install, update, rollback, repair, and uninstall on clean native VMs/runners without a development toolchain.
<a id="p9-008"></a>

- [ ] `P9-008` Implement the `jarvis migrate mark-liv` importer specified by
      `docs/migration/python-prototype.md`: dry-run manifest, source/destination display, rejection of
      API keys and certificates, `legacy_import` provenance, backup and rollback before finalization,
      truncated/invalid/duplicate reporting, and explicit user confirmation before writing canonical
      memory. Until it exists, `example/memory/long_term.json` is the only readable source for a real
      user's prototype data, so deleting `example/` first would discard it.
<a id="p9-009"></a>

- [ ] `P9-009` Retire `example/` against the documented gate: installation, conversation, safe tools,
      memory, local voice, visual geometry, proactive workflows, and cross-platform behavior all have
      equivalent or deliberately superseding acceptance evidence; license obligations are settled;
      user migration is complete or explicitly abandoned by ADR; and `THIRD_PARTY.md`, `SECURITY.md`,
      `README.md`, `AGENTS.md`, `.gitignore`, and the ~11 files linking into `example/` are updated in
      the same change. `docs/research/evaluated-prototypes.md` records the required sequence: findings
      become dated records first, then the prototype is deleted.
<a id="p9-010"></a>

- [ ] `P9-010` **Promoted (`ADR-0167`): first platform is Telegram, `P11-005`.** Add messaging surfaces as authenticated gateway clients, one slice per platform, each binding an
      inbound sender to an actor, user, and workspace **without trusting caller-supplied identity**
      (`FR-ID-001`, `ADR-0038`: a network request is never a local caller). A platform that is not configured
      is an absent capability rather than a degraded one, so a delivery for an unconfigured surface is refused
      and named.
<a id="p9-011"></a>

- [x] `P9-011` *(Done 2026-10-10 as `P9-090`, `ADR-0166`: `jarvis eval`, two committed suites. What remains is the owner's own suites for their own work.)* Add an answer-quality evaluation harness: labeled prompts with expected properties, run against
      a configured provider, asserting answer quality and regression over time. Distinct from the acceptance
      gates, which assert correctness of behavior rather than the quality of an answer.

### P9-012 to P9-019: Onboarding, settings and distribution (spec: `docs/product/onboarding.md`)

- [x] Review of the first-run path, the key and settings story, the lifecycle commands and the release path (this entry).
<a id="jarvis"></a>

- [x] `jarvis` with no arguments (or `jarvis launch`, or flags alone) sets up on the first run when a terminal is available, then
  starts the daemon and opens the console; without a terminal it explains itself. `[profile.release]` added (thin LTO, one
  codegen unit, stripped). `docs/user/quick-start.md` written; README and the developer guide link to it.
<a id="p9-012"></a>

- [x] `P9-012` `POST /api/v1/shutdown` (authenticated, graceful, wakes the same shutdown path a signal does) and `jarvis stop` /
  `jarvis restart` that wait for the port to free. Also fixed: on Windows `jarvis start` made the daemon inherit the pipe a script
  captures the output through, so `jarvis start | anything` never returned; the daemon is now started through `Start-Process`
  with its process id tracked, and a daemon that dies at startup is reported in about a second. Live: stop 0.75 s, restart from
  stopped and while running through a capturing pipe, a bad configuration reported in 1 s. Tests: 401 without the credential,
  202 with it, the waiter wakes, stopping what is not running is not an error.
<a id="p9-013"></a>

- [ ] `P9-013` *(Partly met in the console by `P9-085`: provider presets, the provider's own model list, a real test call before saving. Still open: the terminal `jarvis init`, a non-echoing key prompt, an optional voice step.)* Guided brain and key setup: provider presets, a non-echoing key prompt (dependency decided in an ADR), key to a private
  file, a real test call before saving, optional voice step; the same flow on first launch and in `jarvis init`.
<a id="p9-014"></a>

- [x] `P9-014` `jarvis config show|get|set|unset` and `jarvis keys status|set|remove|test`: one setting at a time, secrets never
  printed or taken as arguments (environment variable, file or standard input), every change validated by the daemon's own
  parser and refused without touching the file, `--json` on the reads. No new third-party dependency (the workspace's `toml`).
  Tests: one change keeps the rest (folders, key file), invalid changes leave the file byte-identical, half a pair is refused,
  a key lands in a private file and never in the configuration, malformed keys are refused without echoing them. Live: set a
  model, kept the folder, refused a voice id without a key, `keys test model` against a real Ollama, `keys set voice --file`.
  Limits: the file is rewritten from its parsed form, so hand-written comments are lost; the paste prompt echoes (hidden
  input needs a dependency decision, `P9-013`); `keys test voice` needs a running daemon and a real key.
<a id="p9-015"></a>

- [x] `P9-015` Settings screen in the console (`ADR-0139`): keys (password fields, write-only, status only), every setting with the
  daemon's own refusal reason, **Restart to apply** (`POST /api/v1/restart`). The settings logic moved into
  `jarvis_storage::settings` and the CLI is now a thin caller, so there is one validation path. Tests: storage (one change keeps
  the rest, invalid changes leave the file identical, keys never in the config or any listing), routes (401 on every one, key
  write-only and never echoed, invalid vs valid). Live: set a voice key through the password field (cleared after sending), saw
  "a port is a number from 1 to 65535" for a bad port, saved a voice id, pressed Restart: new process, page reconnected by itself,
  `GET /api/v1/speech` reported the voice enabled. Limits: no "test key" button yet (use `jarvis keys test`); the tool ask/trust
  list is edited as text (`policy.trust`), not per-tool toggles; the file loses hand-written comments on the first change.
<a id="p9-015b"></a>

- [x] `P9-015b` Settings in tabs (Brain, Voice, Folders & code, Permissions, Advanced). Every setting now carries its tab, its effective default and a
  plain "what unset means" (the review found no setting is "not built yet": each unset one is opt-in by design or needs a key,
  Docker or a folder; the screen simply did not say so and showed an unwritten default as "(not set)"). Related settings apply
  together (`PUT /api/v1/settings` is all-or-nothing, so the code image and its command are one save), and a Permissions tab sets
  one posture per tool (default / ask / trusted / off) over `PUT /api/v1/settings/tools/{tool}`, checked against the real tool
  registry. Tests: all-or-nothing batch, one posture per tool with the file left tidy, every setting has a tab and an
  explanation, 401 on the new routes. Live: tabs rendered, a tool made trusted, the sandbox pair saved in one click, a half
  pair refused. Reference doc: `docs/user/settings.md`.
<a id="p9-020"></a>

- [x] `P9-020` *(Done 2026-10-10 as `P11-001`: Settings, Tools (MCP), and `jarvis mcp add|list|test|remove`.)* An editor for `mcp-servers.toml` (add, remove and test an MCP server) in the Settings screen and as `jarvis mcp`; today it is
  hand-edited.
<a id="p9-021"></a>

- [ ] `P9-021` "Test" buttons beside each key in the console (model and voice), reusing `jarvis keys test`. *(Model and fallback: done by `P9-085`, a Test beside each model. Open: voice and search keys.)*
<a id="p9-016"></a>

- [x] `P9-016` `jarvis doctor --live` (the existing offline `doctor` and its acceptance output are unchanged): asks the model server to list its models
  with the saved key (rejected key, unreachable server, model not listed), checks the granted folders exist, Docker and the pulled
  image when the code tool is on, whether the daemon is running and has the voice a saved key implies; every non-OK finding prints
  the command that fixes it, `--json` gives the same, and a failure exits non-zero. Also: `jarvis config set code_sandbox IMAGE
  COMMAND...` / `unset code_sandbox` set or remove the pair together (one at a time the daemon's parser refuses half a pair), and
  `config get` prints a command as `node -e`. Live: found Ollama unreachable (it was updating), a deleted folder, and Docker not
  running, each with its fix. Tests: both model-list shapes read exactly, every failing status carries a fix. Not done: a hidden key
  prompt and provider presets (`P9-013`).
<a id="p9-017a"></a>

- [x] `P9-017a` Release archives: `scripts/package-release.sh` builds `jarvis-<version>-<platform>.zip|tar.gz` (both programs, a README, the
  user docs, `THIRD_PARTY.md`) with a SHA-256 file, and `.github/workflows/release.yml` builds and smoke-tests it on Linux x86_64 and
  aarch64, macOS Apple silicon and Intel (cross-built) and Windows, then uploads workflow artifacts, and on a `v*` tag also attaches the
  archives to a **draft** release (visible only to people with write access). No published release is
  created: no licence has been chosen (README, "License Status"), which blocks distribution. Live: the Windows archive was built here,
  its checksum verified, and the unpacked `jarvis` and `jarvisd` run from an empty folder (about 35 MB). The workflow YAML parses;
  it has not run on GitHub yet. `THIRD_PARTY.md` now records the one bundled asset (the face mesh, Apache-2.0).
  **CORRECTION, 2026-10-14:** the historical no-license blocker above is resolved by Apache-2.0 for original platform and website code/docs (`P9-017c`); it does not relicense the removed prototype. Current archives contain one program (`jarvis daemon` is daemon mode), not two.
<a id="p9-017b"></a>

- [ ] `P9-017b` Publishing: Apache-2.0 is selected; still generate the dependency-license inventory, sign/notarize (macOS, Windows), publish
  a verified release and provide verified-binary installers (`P9-004`). Existing website installers build from source; license choice alone does not complete this gate.
<a id="p9-017c"></a>

- [x] `P9-017c` **Open-source terms and frontend/code reconciliation** (owner-requested 2026-10-14). Original core and website code/docs use the unmodified Apache-2.0 text from the official Apache URL, with Cargo/npm metadata and NOTICE attribution; third-party materials and historical CC BY-NC 4.0 prototype terms stay separate. Archive packaging includes LICENSE/NOTICE/provenance. Website claims reflect recoverable cleanup, bounded XLSX edits, contact import/suppression, MCP credential gaps, bounded new restart continuations and experimental coding-agent review, not target architecture.
  - Verified: `cargo metadata --no-deps --format-version 1 --locked --offline` (19/19 workspace licenses); `cargo deny --offline check licenses` (passed); `bash scripts/test-package-release.sh` (Windows/Git Bash ZIP, byte-identical notices, checksum, fails without LICENSE/NOTICE; inert executable, not a real install); frontend `node --test tests/design.test.mjs tests/install-commands.test.mjs` (14 passed), `npm run check:install` and `npm run build` (passed; existing lazy-face chunk warning). Integrated Chromium: 12 cards, no runtime errors or horizontal overflow at 320/390/768/1024/1440 px; served Apache text HTTP 200. LF attributes preserve pinned license text on Windows.
  - Shared-tree limits: the attempted full frontend `npm test` hit concurrent installer-fixture `tmpdir` errors; `cargo fmt --check` hit concurrent `init_setup.rs` edits. Both were reported to the installation session, not modified here. Full Rust clippy/tests were explicitly left to the coordinating CI session rather than duplicated. No provider calls, native-device verification, publication or deployment is claimed; `P9-017b` stays open.
<a id="p9-018"></a>

- [x] `P9-018` `jarvis service install|uninstall` install and remove the per-user service: a systemd user unit (plus `loginctl
  enable-linger` for servers), a launchd agent, a Windows Run entry; the command sequences are pure data (`ServicePlan::install_steps`)
  and refuse to touch a definition JARVIS did not write. Also fixed: the launchd plist put its comment before the XML declaration
  (invalid, `launchctl` would reject it), and systemd paths with spaces are quoted. Live on Windows (install, registry entry,
  uninstall, nothing left behind); **Linux and macOS paths are unit-tested as data and type-check, but were not run on a real
  machine** (`P9-022`).
<a id="p9-018b"></a>

- [x] `P9-018b` Server readiness: a Unix daemon is started in its own process group so closing an SSH session does not stop it; `jarvis
  hud --print-url` prints the credentialed address for an SSH tunnel, and a machine with no display is told how to tunnel in instead
  of failing; `docs/user/platforms.md` states what is verified per platform. CI was red on all three platforms: three CLI init
  tests used `C:/…` paths (relative on Unix, so the daemon's own parser refused them), and one Windows gateway test had a 5 s ceiling
  on a background task on a slow runner; both fixed.
<a id="p9-022"></a>

- [ ] `P9-022` First real run on a clean Linux VPS and a Mac (and an aarch64 Linux): install, init, start, tunnel to the console, run a
  conversation, `service install`, reboot; record what breaks. Prerequisite for calling either platform supported.
  CI-found so far, fixed: a Unix socket path over 103 bytes is now refused by name (`EndpointError::SocketPathTooLong`); sandbox tests no
  longer assume a delegated cgroup; the kill test no longer reads `/proc` (absent on macOS); tests no longer use `C:/` paths on Unix; the phase 3
  gate no longer looks for its freshness witnesses with backslash paths (it reported a built `jarvisd` as missing on Linux and macOS); the
  portable-mode test uses a short directory so its socket fits.
  **First real run, Ubuntu 24.04 x86_64 VPS, 1 vCPU, 2026-10-11 (via the website installer, `frontend/` repo):** build from source 10 min 41 s;
  `install`, `init`, `start`, `service install`, the SSH tunnel and `doctor --live` all worked, and port 8765 was not reachable from outside.
  Found, **not fixed here** (core CLI, left for the agent working on it):
  (1) `jarvis install --service` before `jarvis init` starts a systemd daemon with `http_port=0`; afterwards `jarvis stop|restart` sees no
  listener on the configured port, reports "not running", and `start` then fails with "another jarvisd process already owns this profile".
  `systemctl --user restart jarvisd` is the only way out. Suggested fix: `stop`/`restart` should go through the service manager when a
  login service owns the profile (or find the owner via the singleton lock), `service install` should refuse or warn when no HTTP API is
  configured, and the error from the singleton should name the service. (2) `~/.local/bin` is on PATH only from the next login on Ubuntu;
  the post-install hint says so. (3) `jarvis service install` while a `jarvis start` daemon is running leaves the unit crash-looping every
  3 s ("another jarvisd process already owns this profile", `NRestarts=1972` after about two hours) while the manual daemon keeps serving;
  `jarvis stop` lets systemd take over. `service install` should stop the manual daemon first or refuse, and the unit needs a restart
  limit. With a local Ollama and `qwen2.5:1.5b`, `jarvis init` (no `--base-url`) then `jarvis ask` completed a real run in 26 s on one CPU.
  Still unrun: AlmaLinux, aarch64, macOS.
<a id="p9-023"></a>

- [x] `P9-023` Remembering works in conversation (ADR-0140). Found in a real chat: "remember to reply with short sentences" failed
  because the model had to supply an entity identifier it cannot know. A claim with no named subject is now about the profile's owner
  (one confirmed `You` entity, created on first use); a named entity must still exist. A model's proposal stays a `Proposed`
  `model_inference` and is never retrieved, so the console shows proposals in "Waiting for you" as "remember?" cards: Keep files the
  text as the person's own statement (which retrieval reads), Dismiss forgets it. Live: asked to remember, the card appeared, Keep
  made it `active user_statement`, and a later answer in a new run followed it. Tests: two claims share one owner; an invented id is
  still refused with a 422; a proposal with no subject is still `Proposed`; an explicit empty `entity_ids` is still refused.
  Feedback: an answer you give now lands in the conversation itself ("Approved jarvis.web.fetch. Carrying on.", "Remembered: ...",
  "Denied ...", or a failure), and the "waiting for your approval" chip turns to "approved" or "denied"; before, only a hint under the
  box changed. Checked live with a held `jarvis.web.fetch` and with Keep.
  Not done: keeping a proposal by voice ("Jarvis, keep that") or in one terminal step.
<a id="p9-025"></a>

- [x] `P9-025` The console is a face with a conversation (ADR-0141). The face fills the page and is no longer steered by the pointer: it
  has a mind of its own (expressions that change by themselves, saccades with the head following, irregular and double blinks, head
  wander, breathing, sighs, dozing off), reacts to listening, working, speaking (mood of the answer, beats of the voice), waiting and
  offline, and to events (nod and smile on approve, shake on deny or stop, concern on failure). The head rig gained eye lids, a smile
  and frown, and independent inner/outer brows. The chat is a panel on the right; the dashboard moved to an Ops page (`O`); approvals
  and "remember?" cards sit in a tray at the bottom left of the face page. Checked live in the browser (idle, amused, concerned, the
  approval tray with Approve and Deny). New chat (button, `N`, or "new chat" aloud) starts a conversation with no old context; History
  reopens earlier ones with their daemon session; conversations persist in the browser across reloads. Live: a codeword told in one chat
  was unknown in a new chat and remembered when the first was reopened. Not done: conversations are not stored on the daemon, so they
  do not follow you to another browser; a conversation cannot be renamed. Not done: the face has no ears for the room (it does not turn toward a speaker), and the
  expressions are tuned by eye on one mesh, not tested.
<a id="p9-026"></a>

- [x] `P9-026` `jarvis path install|uninstall|status`: `jarvis hud` said "command not found" because nothing puts the folder on the PATH.
  Windows: adds the folder to the user `Path` through PowerShell (the directory is quoted so it cannot leave the string; only the
  user value is read or written; idempotent, tested). Linux and macOS: links both programs into `~/.local/bin` and says if that is not
  on the PATH, never replacing a file that is not its own link. `jarvis start` prints a tip when the folder is not on the PATH.
  Live on Windows: installed, second run reported "already", the user PATH holds the folder once. Not run on Linux or macOS (type-checked
  for both). Not done: installers (`P9-017b`) should do this for you.
<a id="p9-027"></a>

- [x] `P9-027` No small limits, and no silent hangs (ADR-0142). A "create a Next.js app" request failed at its sixteenth tool call with "the
  run failed". Raised: model calls 8 to 400 and tool calls 16 to 1,200 as ceilings; a repeat guard replaces them as the loop stop; file
  write 4,000 to 60,000 characters (and the canonical-intent cap 8,192 to 1,048,576, without which no call over 8 KB worked; an edit
  stays at 2,000 and 4,000 because a held call must fit the 8,192-byte approval), context window 8,192 to 64,000 tokens, history 12 to 40 turns, model
  request timeout 2 to 15 minutes, request body 16 KB to 256 KB. Visibility: reasoning and tool-call writing are counted
  (`StreamEvent::Progress`, never kept) and shown as "thinking (~11k tokens)", the progress line shows step, file and time, failures
  say why and offer Continue, the model is asked to narrate stages (spoken when the voice is on), and a minute of quiet is spoken
  about. A model that reasons over 40,000 characters without answering is cut off and re-asked with `reasoning_effort: low`, sticky for
  the run. Live: a multi-page site ran past 50 tool calls with the limit firing and recovering. Not done: a setting for
  `reasoning_effort` (it is only sent after an overrun); a per-run "allow edits" (each edit asks, so a big coding task is many
  clicks; Settings, Permissions, can set Edit to run without asking); messages over 4,096 characters (a database check, needs a
  migration).
<a id="p9-028"></a>

- [x] `P9-028` A stray `JARVIS_` environment variable no longer stops JARVIS starting. Found live: a user-level `JARVIS_MODEL_KEY` and
  `JARVIS_VOICE_KEY` (set for a different program) made every `jarvis` command fail with "no usable configuration (unknown configuration
  environment variable: JARVIS_MODEL_KEY)", the daemon stay down and the console read "offline". Unknown `JARVIS_*` variables are now
  ignored (the prefix is not ours alone) and named, never with values, by `jarvis doctor --live` and the daemon log; a variable JARVIS
  does define with a bad value is still an error. Tests: an unknown variable is ignored, only names are reported, a bad value for a known
  one still fails. **Second half, found by the phase 1 gate on this machine two days later:** a fresh install with *no config file* still
  took the strict path (`load_with_environment` applied the environment directly), so the stray variable still stopped a first run; fixed
  with a test for the missing-file case. Also learned: the gates use a prebuilt `jarvisd`, so a gate run before `cargo build --workspace`
  tests the old daemon.
<a id="p9-029"></a>

- [x] `P9-029` CI after the limits push (run 131): Windows and Ubuntu green. Release workflow (run 7) built all five archives but every smoke test
  failed: `ls dist/*.zip` exits 2 when nothing matches and `pipefail` turned that into a failed step (now `find`); `package-release.sh` used
  `Compress-Archive` without 7z, which writes backslash paths (now Windows' bsdtar); verified locally by packaging, unzipping and running both
  programs. macOS failed the Phase 3 gate with no readable reason (job logs need a login), so `scripts/ci-run-gate.sh` now copies the panic
  message into `::error::` annotations for the three process-level gates. Still open: the macOS Phase 3 failure itself, until the next run
  shows its message.
<a id="p9-030"></a>

- [x] `P9-030` A request with a line break no longer breaks a run, and a run that cannot continue is no longer left open. Found live while
  asking JARVIS to fix a build error pasted into a request: the newline made the context's one-line source reference invalid, the run failed
  before any model call, and nothing settled it, so it sat in `context_building` for good while the console said "working". The reference
  now collapses whitespace (the objective itself is untouched), and `fail_if_unfinished` settles any run the executor gives up on as
  `executor_error` with the reason, for runs and sub-agents. Tests: a multi-line objective is answered and the model receives it intact; an
  abandoned run is failed, a finished one is not touched. **Live end to end:** the same limits work as intended: "create a Next.js app" now
  completes in about 2.5 minutes (it failed at 16 calls before), JARVIS wrote 12 files in Dutch, the site failed `next build`
  (`styled-jsx` in a server component), a pasted error got three approved edits, and the rebuilt site builds (8 static pages).
  **What this shows is missing:** JARVIS cannot run a build or a test, so it cannot catch its own mistake. That is the next thing to build
  (`P9-024`: a command-running tool, which needs a decision on where it may run, since `jarvis.code.run` needs Docker).
<a id="p9-031"></a>

- [x] `P9-031` JARVIS can run a command in a granted folder (ADR-0143): `jarvis.command.run` (`npm run build`, `cargo test`), no shell, bare
  program name on the PATH, directory confined to a granted root, allowlisted environment, whole-process-tree kill on timeout or drop, head and
  tail of the output fenced as data, asks every time unless trusted. It is host execution, not a sandbox, and the ADR says so. 15 tests run real
  commands (success, failure, timeout kill, escape attempts, secrets not inherited).
<a id="p9-032"></a>

- [x] `P9-032` A run that waits for an approval remembers its turn (ADR-0143). Found live with the command tool: after each approval the run
  resumed with no memory of its steps, approved `npm run build` about fourteen times and ended empty; and an identical approved call was refused as
  a repeat. Migration 0015 makes the repeat guard apply only to pending/denied approvals; migration 0016 stores the run's own turns while it
  waits and puts them back on resume, with "not run" results for calls after the held one. Tests: a three-call turn resumes with a result for
  every call, trimming keeps whole turns, a storage round trip. **Live:** with an injected type error the run built, fixed and rebuilt, and answered.
<a id="p9-033"></a>

- [x] `P9-033` One executable (ADR-0144). `jarvis daemon` runs the daemon (`jarvisd` is now a library plus a thin wrapper binary); `jarvis start`
  and the login service launch the `jarvis` file itself, and release archives, `jarvis path install` and the smoke test carry just `jarvis`.
  Tests: every service definition launches `<exe> daemon`; the PATH links are one file unless an old `jarvisd` is beside it. Live: see below.
<a id="p9-034"></a>

- [x] `P9-034` The daemon is ready only when it listens. CI on macOS failed intermittently (phase 3 in run 132, phase 2 in run 133) with "error sending
  request" to a freshly started daemon: the gates wait for the lifecycle row `ready`, and the daemon wrote it (and set the health state, and
  logged "daemon ready") **before** it bound the HTTP port, so a client that trusted it could connect to nothing. `declare_ready` now runs after every
  listener is bound. This is a real fault, not a test one: `jarvis status` and a service manager read the same state. All 56 test binaries pass on
  Windows; macOS is confirmed only by the next CI run.
<a id="p9-035"></a>

- [x] `P9-035` JARVIS knows the date and time. A model has no clock, so "what day is it" or "remind me tomorrow at nine" was a guess. Every run's
  system message now ends with the current weekday, date and local time with its UTC offset, plus the UTC instant (`clock.rs`). The offset is read
  once at startup, before the async runtime has threads (the `time` crate only does it soundly then), so a daemon that stays up across a daylight-saving
  change keeps the old offset until restarted; the UTC time is always right. Tests: the date follows the offset across midnight, negative and unknown
  offsets are stated honestly, a first turn carries the clock. **Live:** "what day and time is it" answered correctly (Friday 9 October 2026, 18:04, UTC+02:00).
<a id="p9-036"></a>

- [x] `P9-036` A tool's permission changes at once, and the approval card says "Always allow" (ADR-0145). Saving a tool's posture used to need a restart,
  so "stop asking me for `npm run build`" meant leaving the conversation. The pipeline now holds the policy as a replaceable snapshot, the settings
  handler rebuilds it from the saved file with the daemon's own function and swaps it (`applied: true`); a file it cannot compose leaves the running
  policy untouched. Test: on one running pipeline a call asks, then (trusted) runs, then (off) is refused, each on the very next call. **Live** through
  the API: trusted applied at once, off refused the next call, default restored.
<a id="p9-037"></a>

- [x] `P9-037` Scheduling from conversation (ADR-0145): `jarvis.schedule.add|list|remove` over the existing scheduler and its validation. Adding and removing
  ask (a schedule persists beyond the chat); listing is free; objectives read back are fenced. Tests on a real database: add/list/remove, and six bad inputs
  (both cadences, neither, past time, unparseable time, too-short interval, blank objective) each refused and creating nothing. **Live:** "remind me in
  2 hours" asked, was approved, scheduled for the right UTC time (from the clock, `P9-035`), and was listed back.
<a id="p9-038"></a>

- [x] `P9-038` Scheduled work reports back in the console. A reminder or recurring check was only readable afterwards in `jarvis runs`. The console now watches
  the schedules it already polls: when one fires and that run finishes, the answer is put in the conversation ("Scheduled: ..." then the answer) and, with
  spoken answers on, said aloud; anything already fired when the page opened is not announced again. **Live** (browser): a one-off task scheduled 75 seconds
  ahead appeared in the open console when it ran. (The first attempt polled a wrong URL and showed nothing; the page's own error log showed the 405, which is
  why this was verified in a real browser.) Not done: a notification when no console is open (needs the desktop shell, `P9-019`).
<a id="p9-039"></a>

- [x] `P9-039` A coding task now runs end to end, and its narration reads as paragraphs. Dogfood: "build a dependency-free Node todo CLI with `node:test` tests, run
  them and fix failures" completed in about two minutes with 10 passing tests and no help (the command tool was trusted, `P9-031`, `P9-036`). It showed one
  defect: the narrated stages ran together ("...run the test suite.App and tests are written") because each model call's text was streamed with nothing between
  them. A call after one that already spoke now starts its streamed text with a paragraph break (the settled answer is unchanged, and a resumed run counts as
  having spoken). Test: the stream of a tool turn plus an answer is `first.\n\nsecond`. **Live:** the same kind of task now prints each stage on its own line.
<a id="p9-040"></a>

- [x] `P9-040` A default reasoning effort (ADR-0146): `daemon.executor_reasoning_effort` none/low/medium/high, applied in the model adapter when a request does not choose
  its own (the loop's drop to `low` after an overrun still wins); refused without a live model or with an unknown word. Tests: the four words and both refusals;
  the adapter sends the default, and a request's own choice wins. **Live:** `jarvis config set` accepted `low`, refused `maximum`; a question answered in 1.5 s with it set.
<a id="p9-041"></a>

- [x] `P9-041` Desktop notifications for scheduled results (ADR-0146): on unless `notifications = off`; shown only when no console polled in the last 15 s; text passed
  apart from the script on every platform (tested with hostile text), Windows script single-quote-only. **Live:** a scheduled task with no console open logged and
  showed a notification; the exact toast script was run by hand and shown. macOS/Linux: command construction only.
<a id="p9-042"></a>

- [x] `P9-042` Web search (ADR-0146), **verified live with the owner's key (2026-10-09)**: `jarvis.web.search` over Ollama's documented endpoint, opt-in by `jarvis keys set search`;
  research recorded in `docs/research/integrations/ollama-web-search.md` (the local Ollama server does not serve the endpoint, 404). Tests: documented request and fenced results,
  401/429/500/unreachable, malformed reply, long results, bad arguments, key never in `Debug`, composition skips a missing or malformed key. **Live:** with an invalid key the real
  service refused it and the assistant said so and fell back to a page fetch. **Positive path (owner's key):** `jarvis keys test search` accepted; a run for the current Node.js LTS used the tool and answered; the real reply is `results[{title,url,content}]`, with very long `content` (40 KB seen) that the adapter clips.
<a id="p9-043"></a>

- [x] `P9-043` `jarvis install` / `jarvis uninstall` (ADR-0147): one command puts the single executable in a per-user folder, on the PATH, optionally starting at login; atomic replace;
  relative `--dir` refused; the profile is never touched. Tests: the folder choice, replace-and-no-leftovers (and executable bit on Unix), a missing source, a relative dir. **Live:**
  install into a scratch folder, replace, "already installed", uninstall. `--service` and macOS/Linux are not run here.
<a id="p9-044"></a>

- [x] `P9-044` `jarvis.memory.search` (ADR-0147): the model looks up what is remembered, by words that must all appear; same eligibility gate and fencing as a context; reports how much
  it looked at. Tests on a real database (match rules, ranking, fencing and introduction, the count when nothing matches, argument limits). **Live:** after a fact was stored, "look up my cat" found it.
<a id="p9-045"></a>

- [x] `P9-045` The search key can be set and removed in the console (Settings, Brain), not only with `jarvis keys`. **Live** in a browser: the row shows, with its help text.
<a id="p9-046"></a>

- [x] `P9-046` `jarvis.files.search` (ADR-0148): find lines containing some text across a granted folder (literal, case-insensitive, optional file-name glob), through the same confinement as read and list,
  bounded in files/bytes/time/output with `truncated` set when it stops. Tests: nested results in stable order, glob and sub-folder, skipped folders/binary/large files and the match limit,
  **a link out of the root is not followed** (junction on Windows), bad arguments, glob rules. **Live:** found the right files and lines in a project with a decoy in `node_modules`.
<a id="p9-047"></a>

- [x] `P9-047` Search then fetch works end to end with the owner's key (live): a research question used `jarvis.web.search` then `jarvis.web.fetch` and summarised the real latest Rust release with its source.- [x] `P9-048` Sign in with Google from Settings (ADR-0149): `GoogleAccount` (PKCE via the connector crate, code exchange, refresh, revoke, private token file), routes `/api/v1/google*` and the one public
  callback, and a Settings, Google tab (client id, client secret, Sign in, Disconnect, status). Tests: the authorization request (S256, state, redirect, exactly the two read scopes), the whole sign-in
  against a local Google stand-in (the verifier proves the challenge, the secret is sent, only the refresh token is stored), **forged, repeated, cancelled, wrong-path and expired answers store nothing**, a partial
  grant is refused and revoked, a revoked sign-in is removed, refresh, disconnect; at the gateway the callback is public but inert while the rest is not. **Live:** the tab renders; Google parsed the real authorization URL and
  refused only the fake client id. **Not verified:** a real sign-in (needs your own OAuth client).
<a id="p9-049"></a>

- [x] `P9-049` `jarvis.gmail.search`, `jarvis.gmail.read`, `jarvis.calendar.events` (ADR-0149): read-only, held for the owner by default, fenced as untrusted, fixed-word failures (401/403/429/5xx, not signed in). Tests with a Google stand-in:
  fenced search results with an injection-shaped snippet, base64url plain-text decoding from a multipart message, calendar ordering and bad windows, failures never echo the provider. `docs/user/google.md` is the setup guide.
<a id="p9-050"></a>

- [x] `P9-050` Sending mail and creating events (ADR-0150), opt-in: `daemon.google_actions` adds `gmail.send` + `calendar.events` to the sign-in; `jarvis.gmail.send` (external communication, risk 3, always asks, one validated recipient,
  no header injection) and `jarvis.calendar.create` (asks, no invitees). Tests with a Google stand-in: the exact MIME message, eight injection/recipient shapes refused with nothing sent, no send without the granted scope, event times to UTC
  and no attendees, the four-scope request and refusal of a partial grant, base64 both alphabets. **Not verified live:** a real send (turn actions on, sign in again; test to your own address only).
<a id="p9-051"></a>

- [x] `P9-051` A stop or restart no longer silently interrupts working tasks (ADR-0150). Found when a restart killed five of the owner's sub-agent runs: `/shutdown` and `/restart` answer 409 with the count unless `?force=true`;
  `jarvis stop|restart` refuses and leaves the daemon running, `--wait` waits for the tasks, `--force` interrupts; the console offers "Restart anyway". Tests: counted, refused, forced, parked runs not counted, `force=true` parsing.
  Not run against the live daemon (it had work in flight).
<a id="p9-052"></a>

- [x] `P9-052` Projects (ADR-0151): the owner writes a goal, standing guidance, a working folder and a status once, and every run of the project is told it, with a journal the model keeps. Storage
  (`0017_projects.sql`, `project_repository`: case-insensitive unique names, relative-folder validation, notes, session/schedule links, cascade delete), routes (`/api/v1/projects`, `project_id` on a
  run start and a schedule), the scheduler (a paused or done project's task is skipped and counted), `ProjectContext` in the executor (brief as a policy system message, journal fenced as data,
  sub-agents get the brief only). Tests: storage CRUD/validation/links, routes (auth, bad input, a refused start leaves no run behind, no authority fields), scheduler pause, an injection-shaped note staying out
  of the policy message.
<a id="p9-053"></a>

- [x] `P9-053` `jarvis.project.note` (ADR-0151): the model's journal tool. No project argument: the project comes from the call, its run and the run's conversation, so a model cannot write elsewhere; cannot claim the
  `owner` kind; no approval (one line in the owner's own journal). A task scheduled from inside a project, and a sub-agent started from inside one, inherit it. Tests: written to the calling conversation's
  project, refused outside a project and for an unknown call, owner kind/empty/over-long refused, schedule inheritance.
<a id="p9-054"></a>

- [x] `P9-054` Projects in the CLI and console: `jarvis project add|list|show|set|pause|resume|done|note|remove`, `ask|chat --project`, `schedule add --project`; Ops-page Projects panel, project window with
  journal, and a project selector beside the conversation title. `docs/user/projects.md`. Verified live on a scratch profile (debug build, local Ollama `glm-5.3:cloud`): the console panel, project window (edit, status, journal entry) and selector, then a console message in the project.
<a id="p9-055"></a>

- [x] `P9-055` Use projects on the owner's real prospecting work. *(Done 2026-10-10: the owner's daemon was migrated, the "Jarvis B2B Netherlands Sales" project exists with its brief, journal and schedules, and its runs are in use daily.)* Original note: Done on a scratch profile with a live model: the brief reached the model (it answered in Dutch, named the goal and folder), the model wrote
  a journal entry through `jarvis.project.note` without an approval, a second conversation continued from that entry, and a paused project's schedule was shown as paused. Still to do: restart the owner's daemon on this
  build when no run is in flight (their database migrates 16 to 17 with a backup), create the project from their existing sales brief, and attach the six-hourly schedule.
<a id="p9-056"></a>

- [x] `P9-056` Live view of what JARVIS is doing (ADR-0152): tool requests name their target (page, query, task; no URL query or credentials), a finished call reports `ok` and the links it touched (safe `http(s)` only, at most
  eight), `jarvis_web::links_from_results`; `mission.js` (feed, sources, satellites/beams/effects per kind of work, the face docking right); the console follows scheduled and sub-agent runs too; finished answers list their sources;
  links in answers are clickable under the same rule. Tests: target/secret stripping, hostile links refused, result classification, executor emits both events, parser, page security tests. Verified live (scratch profile, real model:
  multi-search research, a sub-agent run started from the CLI and watched from the console).
<a id="p9-057"></a>

- [x] `P9-057` Console UX pass: Ops cards show the live action, scheduled tasks can be paused/resumed/removed and show their project, recent answers expand, tools grouped by family (Ops and Permissions), plain setting labels,
  scrolling sheets with a fixed footer, dark scrollbars and focus outlines, a tidier chat header, starting points in an empty chat. Fixed a real failure found on the way: search replies over 512 KiB (ordinary) were refused; bound now 8 MiB.
<a id="p9-058"></a>

- [x] `P9-058` An existing schedule can be filed under a project (or taken out): `POST /api/v1/schedules/{id}/project`, `jarvis schedule project ID NAME|--none`, a selector on the Ops card; `set_project_link`. The Ops page
  puts finished one-off tasks behind a toggle. The finished event is also emitted when a call that waited for an approval completes (the resume path). Tests: re-filing in storage and through the route.
<a id="p9-059"></a>

- [x] `P9-059` JARVIS can manage projects, memory and schedules itself (ADR-0153). Found in a real conversation: asked to "create this as a project" it wrote a README because the journal tool refused outside a project.
  New tools: `jarvis.project.list|use|create|update|assign_schedule` (create/update/assign ask), `jarvis.memory.correct|forget` (ask; the call quotes the claim and the tool refuses when the quotation is not in the stored claim),
  `jarvis.schedule.pause` (no ask) and `resume` (asks). A run in no project is given an index of the existing ones; `jarvis.memory.search` prints `memory_id`. Tests for each, including the false-quotation and no-move cases.
<a id="p9-060"></a>

- [x] `P9-060` Main page: the face alone (ADR-0154). Rings, ticks, name ring, brackets, canvas text and the wire skull and neck removed; the face is about half its former size with a faint glow, drifting motes and a thin voice line.
  The live view is a persistent constellation (nodes on orbits that settle outward instead of leaving after five seconds, moons for links, hover names a node, pressing opens its step), the feed keeps the whole run (40 steps, older ones
  compact), the panel folds to one line after 90 seconds. Chat suggestions no longer overflow. Verified live on a scratch profile with a real research run.
<a id="p9-061"></a>

- [x] `P9-061` Security review of the projects, tools, links and observers surfaces (this session), and fix what it finds, each with a fail-closed test.
  Done (ADR-0155). The review found one HIGH: the approval card clipped arguments, so a long harmless `goal` could hide a hostile `guidance`. `NEVER_TRUSTED_TOOLS` (project create/update/assign_schedule, memory correct/forget) ignore `trust` and the settings route refuses it with 422; the card shows every field whole with no "Always allow"; a spoken yes does not answer them; `watch` says when it cut something. Tests in jarvis-tools and the gateway.
<a id="p9-062"></a>

- [x] `P9-062` Resume interrupted runs: after a restart, a run that was working gets one continuation in its session (never a continuation of a continuation, never a cancelled run); `daemon.resume_interrupted` can turn it off.
  Done (ADR-0155). `continuable_interrupted_runs` picks runs that recovery just settled, started within 2 hours, newest in their conversation, not a continuation or sub-agent; `resume.rs` starts one ordinary run in the same session with a check-first notice (at most three). Setting `daemon.resume_interrupted` (on). Tested end to end including "never continued again".
<a id="p9-063"></a>

- [x] `P9-063` The project journal keeps important old entries: the context shows the recent notes plus older decision, result and blocker entries, so a long project does not forget what it decided.
  Done (ADR-0155). `earlier_important_notes`; the brief shows up to 30 older decision/result/blocker/owner notes (1300 chars) before the recent ones.
<a id="p9-064"></a>

- [x] `P9-064` Per-project daily run limit (`daily_run_limit`): the scheduler skips fires over it; API, CLI and console field; runs today shown.
  Done (ADR-0155). Migration 0018; scheduler skips over-cap fires (rolling 24 hours); API, CLI `--daily-limit`, console "Daily runs" field and `runs_today`; the model cannot set it. Tested including raising the cap.
<a id="p9-065"></a>

- [x] `P9-065` Digest and push: `GET /api/v1/digest`, `jarvis digest`, an Ops card (runs, outcomes, tokens, answers); opt-in ntfy push for approvals and scheduled results (research record first).
  Done (ADR-0155). `GET /api/v1/digest[/hours]`, `jarvis digest`, Ops "Last 24 hours" card; ntfy push (`daemon.push_topic`/`push_server`, content-free, off by default, desktop plus push only when no console is open, new approvals are announced), `jarvis push test`, research record `docs/research/integrations/ntfy.md`. Live push to a real topic NOT run (it would publish to a third party): the request shape is tested against a local listener. Access tokens not built.
<a id="p9-066"></a>

- [x] `P9-066` Contacts store: `jarvis.contacts.save|search|stats` tools, `jarvis contacts list|export`, so prospect data is not only a CSV file.
  Done (ADR-0155). Migration 0019; `jarvis.contacts.save|search|stats` (no approval, fenced read-back, a model cannot lift do_not_contact), `/api/v1/contacts`, `jarvis contacts list|stats|add|remove|export` (CSV with formula defusing). Not built: enforcement of do_not_contact inside the Gmail send tool, a console window for contacts.
<a id="p9-067"></a>

- [x] `P9-067` Memory search falls back to ranking by matched words when no claim contains every word.
  Done (ADR-0155). `jarvis.memory.search` ranks by matched words and says "partial" when no claim has every word.
<a id="p9-068"></a>

- [x] `P9-068` Console: follow runs waiting for an approval, show tokens and runs today, CI syntax check of the console scripts.
  Done (ADR-0155). Parked runs are followed as a held step (working first, three in all); digest card shows tokens; project sheet shows runs used; CI parses hud.js, mission.js and head.js with `node --check`.
<a id="p9-070"></a>

- [x] `P9-070` A run waits out a provider rate limit (ADR-0156): when a model call is refused with a rate limit before any stream opened, it waits 20/45/90/180 s (cancellable, visible as activity) and asks again, then fails as before. Tested: clears after two, and never clears.
<a id="p9-071"></a>

- [x] `P9-071` `jarvis.gmail.send` refuses an address marked `do_not_contact` in the contact list, and fails closed when the list cannot be read (ADR-0156). Tested including case-insensitivity and the closed-database case.
<a id="p9-072"></a>

- [x] `P9-072` Console: an Ops "Contacts" card (counts by status, the first 40, and a status selector per row, which is how the owner lifts a do-not-contact).
<a id="p9-073"></a>

- [x] `P9-073` Concurrent run starts no longer fail (ADR-0156): `start_run`, the memory purge and the summary write begin `BEGIN IMMEDIATE` (they read before they write, and a deferred transaction lost the race with `SQLITE_BUSY_SNAPSHOT`); the reason behind a refused run request is logged. Regression test: 24 simultaneous starts, which fails without the change. Found because the phase 2 gate failed once the restart continuation ran beside a new request.
<a id="p9-074"></a>

- [x] `P9-074` Gmail can carry files, answer in its thread and save what arrives (ADR-0157): `jarvis.gmail.send` takes `attachments` (granted folders only, 5 files, 5 MB each, 10 MB total, secrets refused) and `reply_to_message_id` (thread headers sanitised, `threadId` sent); `jarvis.gmail.read` lists attachments; new `jarvis.gmail.save_attachment` (asks, never replaces, refuses programs). Research record: google.md Finding 25 (Google's size limit for send not found: unresolved). Verified live by the owner (2026-10-10): attachments sent and received fine.
<a id="p9-075"></a>

- [x] `P9-075` JARVIS reads PDF, Word, spreadsheet and slide files (ADR-0158): new crate `jarvis-documents` (`lopdf`, `zip`, `roxmltree`; bounded, panic-safe, no entity expansion) and the tool `jarvis.files.read_document` (granted folders, read-only, 3,500-character slices with `next_offset`, fenced). Research record `document-reading-crates.md`. Tests build their documents in memory; verified live with a hand-made .docx through a real run (it found the codeword and the price); the owner then reported it works on real Office, LibreOffice and PDF files (2026-10-10).
<a id="p9-076"></a>

- [x] `P9-076` `jarvis.files.move` (ADR-0159): moves or renames a file or folder within one granted root, never replaces anything (hard-link then remove for files, so a name taken a moment later still refuses), never crosses roots, refuses moving onto or into itself; risk low, runs without asking; not served over the inbound MCP endpoint. Tests include escapes in both directions.
<a id="p9-077"></a>

- [x] `P9-077` Model fallback (ADR-0159): `daemon.executor_fallback_model_name` asks a second model at the same provider when a call is rate limited or overloaded before any stream opened (`FallbackGateway`, wrapped around the live adapter; one extra try, that call only). Other refusals are never worked around. Tested with a recording model; a live rate limit has not been provoked.
<a id="p9-078"></a>

- [x] `P9-078` Web pages are read in parts, and PDFs on the web are read (ADR-0160): `jarvis.web.fetch` takes `offset` and answers `total_chars`/`next_offset`; `application/pdf` is read through `jarvis-documents` (5 MB, 15 s, panic-safe, same egress guard). Tests: parts join back into the whole, a PDF is read and fenced, a broken and an oversized PDF fail in words.
<a id="p9-079"></a>

- [x] `P9-079` Word notes (ADR-0160): `.docx` reading includes headers and footers (each distinct text once), footnotes and endnotes with numbers, without the separators.
<a id="p9-080"></a>

- [x] `P9-080` A stop no longer waits for an open stream (ADR-0160): the daemon's graceful shutdown waited without limit for open connections, and a console watching a run keeps a stream open, so `jarvis stop` left a daemon alive and unreachable until killed. The wait is now 8 seconds, then the connections are closed; tested with a real endless stream.
<a id="p9-081"></a>

- [x] `P9-081` *(Live 2026-10-10: after the owner signed in again, a run created a draft to the owner's own address and a Gmail search found it with the right subject and recipient; `jarvis eval` showed the run used `gmail.draft` and not `gmail.send`. The draft is still in their Drafts.)* Gmail drafts (ADR-0161): `jarvis.gmail.draft` saves the message a send would carry (attachments, reply threading, do-not-contact check) in the owner's Drafts through `drafts.create`; needs the new `gmail.compose` scope (restricted), asked for with the other action scopes, so an existing sign-in must be repeated; runs without asking (writes only to the owner's mailbox). Research record: google.md Finding 26. Live draft and the consent flow for the restricted scope NOT run.
<a id="p9-082"></a>

- [x] `P9-082` Text out of pictures (ADR-0162): `jarvis.files.read_document` reads PNG/JPEG/BMP and JPEG-scanned PDFs (`jarvis-documents::pdf_scan`) by running the owner's `tesseract` (stdin to stdout, absolute-path lookup, cleared env, time/size/pixel limits, optional validated `language`, `read_by: "ocr"`, last result cached for paging). Research record: tesseract.md. Tested with a stand-in program only; NOT run against a real tesseract (none installed); non-JPEG PDF images and TIFF/GIF unsupported.
<a id="p9-090"></a>

- [x] `P9-090` Evaluation harness (ADR-0166, `P9-011`): `jarvis eval run SUITE.toml [--case ID] [--json]` runs each case as an ordinary run in a fresh conversation through the daemon API and scores it with checks that need no second model (`contains`, `contains_any`, `not_contains`, `tools_used`, `tools_not_used`, `max_tool_calls`, `max_input_tokens`, `max_answer_chars`, `max_seconds`, `parks_for_approval`); a case that parks for approval or times out is cancelled; each full run is saved (`<data>/evals/<suite>/<time>.json`) and compared with the previous (regressions, fixed, input-token change); `jarvis eval history`; non-zero exit on a failure; `ApiClient::read_run_events`. Suites: `evals/basics.toml` (9 read-only cases) and `evals/gates.toml` (`gmail.send` must park; recipient on `.invalid`). **Live 2026-10-10** (Ollama Cloud `glm-5.3-flash`): `basics` 9/9 in 17 s after fixing one of my own checks (a substring check failed the correct answer "nothing has been deleted"); a self-check suite failed a wrong expectation, stopped a timed-out run, and showed that `jarvis.command.run` (declared ask) **ran without asking because it is in the owner's trust list**; `gates` passed (send parked, cancelled, nothing sent); no approvals left waiting. 8 unit tests (suite validation, scoring, event reading, comparison, saved history). Docs: `docs/user/evals.md`. Not built: scoring by a second model, a console view.
<a id="p9-091"></a>

- [x] `P9-091` Eval suites for the owner's own work (ADR-0166): `project` on a suite or case (runs inside that project so its rules apply), `--repeat N` (2 to 10; a case passes only if every attempt does; pass rate `2/3`; tokens are the mean; old saved records still read), `evals/prospecting.example.toml` (committed template), `evals/local/` git-ignored for private suites. **Live 2026-10-10**: the owner's 7-case Sales suite scored 6/7; `dutch-outreach-text` passed 2 of 3 attempts (once a garbled greeting "Geheer [Naam]" and informal "je"), a flaky behaviour to fix in the project's guidance or by routing. 10 unit tests for the harness in total.
<a id="p9-089"></a>

- [x] `P9-089` What each model call costs in tool definitions is now visible: `offer_tokens` per tool on `GET /api/v1/tools` and in `jarvis tools list` (name + description + argument schema, characters / 4), with the total on top. Measured on the live daemon: 37 tools, about 7,200 estimated tokens sent on every model call (a one-word run measured 6,643 input tokens in all), heaviest `gmail.draft` 377, `gmail.send` 345, `contacts.save` 327, `files.read_document` 327, `command.run` 317. Not built: offering fewer tools per run, shorter descriptions (needs a tool-use quality check first), OpenAI-style prompt-cache accounting (`cached_input_tokens` is recorded but not shown).
<a id="p9-088"></a>

- [x] `P9-088` A finished run's answer is found however long its event stream is: `last_answer` read only the first 1,000 events and looked backwards, so a run that streamed more fragments than that lost its answer (to the parent that delegated it via `jarvis.agent.result`, to the owner's notification, and to the transcript). New `latest_run_event_payload` asks the database for the latest `output_completed` directly; regression test with 1,300 events. The sub-agent "EMPTY" reports in the Sales journal were investigated: the four runs I could still see (567 to 665 events) did return 2 to 3 KB answers after 5 to 9 minutes, so that cause is not proven for those; this bug is a real way for the symptom to happen on longer runs.
<a id="p9-087"></a>

- [x] `P9-087` Token usage is now recorded (found while building P9-086, ADR-0165): the OpenAI-compatible stream adapter stopped at the finish reason and never read the usage chunk that follows it, so no run ever recorded a `usage_updated` event; the finish is now held for the usage chunk, `[DONE]`, connection close, or 2 s. Tests: usage after finish, after a tool-call turn, no usage with `[DONE]` or close, and a server that never closes. Runs from before have no usage.
<a id="p9-086"></a>

- [x] `P9-086` Token cap per project (ADR-0165): migration 0020 `projects.daily_token_limit`; `project_tokens_since` sums the `usage_updated` events of the project's runs over a rolling 24 hours; `tokens_today` and `daily_token_limit` on the project API, `jarvis project add|set --token-limit`, **Daily tokens** in the project window; the scheduler holds a fire at or over the cap (fails closed if usage cannot be read); the project tool cannot change it. Not built: stopping a run midway, money or prices, a workspace-wide cap, caps outside projects.
<a id="p9-085"></a>

- [x] `P9-085` Brain settings (ADR-0164): provider presets (Ollama local/cloud, OpenAI, Gemini, Anthropic, other) + live model list + Test + Save on one card each for the main and fallback model; `OpenAiCompatibleProvider::list_models`; `POST /api/v1/settings/models` (daemon-side, stored key only to the address it was stored for, typed key otherwise); Save refuses a changed address without that provider's key; web-search key moved to its own tab; Google tab intro corrected. Research record: model-providers.md. Verified live against local Ollama (5 models listed, test passed in 26.7 s; other-address request refused). NOT run against OpenAI, Gemini, Anthropic or Ollama Cloud (no keys); no tool-call probe; saving still needs a restart.
<a id="p9-084"></a>

- [x] `P9-084` hud.js split, first part: the settings dialog (five tabs, permissions, Google sign-in; about 370 lines) moved to `hud/settings.js`, behind `JarvisSettings(context)` with the seven helpers it shares; `explain` and `putJson` moved up beside `api`. hud.js 2578 to 2222 lines. Checked in a real browser against the live daemon (all tabs, Ops page and project form render, no page errors). The Ops region (about 1,000 lines) was measured and is NOT split: it shares 30 names and reassigns 6 variables with the rest, so it needs an explicit state object first.
<a id="p9-083"></a>

- [x] `P9-083` Fallback at a different provider (ADR-0163): `executor_fallback_base_url` + `executor_fallback_api_key_ref` (+ `jarvis keys set|test|remove fallback`, Settings "Fallback key"); `FallbackGateway::at_another_provider` asks the fallback model there on rate limit, overload, transient failure or spent credit, never on auth/model/context/content/invalid-request refusals; unreadable key degrades to no fallback. Tested with scripted models; NOT run against a real second provider.
<a id="p9-069"></a>

- [ ] `P9-069` Not started, recorded, **split on 2026-10-10 (`ADR-0167`)**: a browser tool for JavaScript pages is `P11-003`; model routing is `P11-006`; still unnumbered: an embedding memory index (`P4-005`), a second search provider, a signed installer (`P9-017b`), splitting the rest of hud.js (the Ops region needs a shared state object first) and a browser test in CI, replay of past constellations, ntfy access tokens.
<a id="p9-024"></a>

- [ ] `P9-024` Give the model more to do. **Status 2026-10-10: delivered for built-in tools; the rest is `P11`.** The model now has 37 tools: files (read, list, search, write, edit, move, read_document with OCR), web search and fetch, shell command, memory (search, propose, correct, forget), schedules, projects, contacts, Gmail and Calendar, and sub-agents. More capability now arrives through MCP (`P11-001`), a browser (`P11-003`), skills the model proposes (`P11-004`) and coding agents (`P7`), with tool discovery (`P11-002`) keeping a large catalog affordable. Close this item when `P11-001` to `P11-003` are live. The original text follows. Today Docker-isolated snippets run code too. A fresh install offers four tools (web fetch, delegate, delegate result, propose memory);
  files need a granted folder (`P9-015b`), `run_code` needs Docker, and nothing else is model-facing yet. Next, in value order: a
  model-facing memory search (so "what do you remember about me" is answered from the store, not from what happened to be in
  context), a clock/date tool, scheduling a task from conversation (the scheduler exists, `jarvis schedule`, but only as a command),
  a notes/todo tool, then the connectors (mail, calendar: `P5`, needing your OAuth credentials) and coding-agent runtimes (`P7`).
  Also show in the console, next to "Can do", what turns each missing tool on.
<a id="p9-019"></a>

- [ ] `P9-019` Tauri shell with a tray icon over the same console (see `P9-001`). **Demoted (`ADR-0167`):** the console already runs in a browser and `jarvis service install` already starts the daemon at login. Build it only as the host of an always-on wake word (`P8-010`) or if a browser tab proves too fragile.

## P10: Server And Multi-Device

<a id="p10-001"></a>

- [ ] `P10-001` Define server deployment profile, TLS termination, trusted proxy rules, backup/restore, and operator runbooks.
<a id="p10-002"></a>

- [ ] `P10-002` Complete PostgreSQL parity and concurrency tests for every canonical repository.
<a id="p10-003"></a>

- [ ] `P10-003` Implement device enrollment, pairing proof, scoped credentials, rotation, revocation, and lost-device response.
<a id="p10-004"></a>

- [ ] `P10-004` Implement remote access with explicit enablement, secure defaults, rate limits, and exposure audit.
<a id="p10-005"></a>

- [ ] `P10-005` Add team membership and roles only after workspace isolation is proven.
<a id="p10-006"></a>

- [ ] `P10-006` Establish service-level objectives, load profiles, recovery objectives, restore drills, and capacity evidence.
<a id="p10-007"></a>

- [ ] `P10-007` Evaluate optional infrastructure only with measured bottlenecks and an ADR per adoption.

## P11: Capability And Reach (`ADR-0167`, added 2026-10-10)

What makes the assistant powerful, in the order to build it. Each item starts with a research record (`docs/research/integrations/`), is tested offline, and is **only done after a live run on the owner's own setup**. None may add a required step to a first run (`ADR-0167` §7), and none may remove a tool: capability is the goal.

<a id="p11-001"></a>

- [x] `P11-001` **MCP made real and manageable** (`ADR-0168`; closes `P9-020` and the remainder of `P3-008`). The host had existed since `P3-008j` and had **never met a real server**; connecting the reference `server-everything` and `server-memory` (npm `2026.8.31`) found six defects, each fixed with a test: (1) the client refused every server that does not speak the newest revision, and all published ones answer only `initialize`, so it now uses `ClientLifecycleMode::Auto` and records the era (`McpEra`); (2) every tool was dropped silently because the servers declare JSON Schema draft-07, now read as 2020-12 where it means the same, with `dependencies`/`additionalItems` refused by name and tuple `items`/boolean `exclusiveMinimum` failing the metaschema check; (3) **MCP output reached the model unfenced** (a prompt-injection path), now fenced as untrusted data in pieces; (4) **a server inherited the daemon's whole environment** and `get-env` returned it, now an allowlist; (5) a server that exited stayed dead until restart, now restarted before the next call with a ten-second backoff, and a call that never left is `NotSent` (safe to retry) while one in flight stays ambiguous; (6) on Windows the PATH search picked Node's extension-less `npx` script instead of `npx.cmd`. Also: servers connect at the same time; the daemon logs each unreadable server and each tool left out; `GET /api/v1/mcp`, `POST /api/v1/mcp/servers`, `DELETE /api/v1/mcp/servers/{name}`, `POST /api/v1/mcp/test`; `jarvis mcp list|add|test|remove`; a **Tools (MCP)** tab; docs `docs/user/mcp.md`; research `docs/research/integrations/mcp.md` (live findings). **Live 2026-10-10** (throwaway root, Ollama `deepseek-v4.1-flash:cloud`): both servers connected (legacy), 13 and 9 tools registered at risk high / approval ask, the model called `echo`, the call parked, was approved and returned `Echo: hello-from-mcp`; the server's process chain was killed and the next call restarted it; a variable set in the daemon did not appear in `get-env`'s output; the tab listed, tested, added and removed a server in a real browser. Tests: 4 wire (legacy fallback, modern, error classes), 2 real-child-process (restart, not-sent and backoff), draft-07 (4), fencing (hostile result, intact fence), environment allowlist, 12 admin, 4 CLI. **Observation:** with the fence in place a model once called a tool twice and said the result was empty although 4,402 characters were stored; `echo` worked every time, so wording of the fence introduction is worth measuring with `jarvis eval` (`P11-001b`).
<a id="p11-001b"></a>

- [~] `P11-001b` **What `P11-001` left open.** (a) **Built 2026-10-15 (`ADR-0216`)**: `[servers.env]` names scoped `mcp:SERVER:KEY` references stored through `jarvis keys`, never values in the file; CLI/console show set/not set. Explicit injection, real-child isolation/restart/redaction and profile-store composition are verified; **GitHub account read and approved/denied write remain unrun**. See [current evidence](todo-owner-evidence-through-2026-10-16.md#credential-backed-local-mcp-2026-10-15). (b) **Named bearer-token subset built/tested 2026-10-15 (`ADR-0219`)**: same-server references, per-request resolution, CLI/Settings/Test, profile/wire regression tests and mutation-proven cross-server refusal ([evidence](todo-owner-evidence-through-2026-10-16.md#named-remote-mcp-token-credentials-2026-10-15)). **Generic pre-registered OAuth built/tested (`ADR-0220`)**: explicit issuer/client setup, discovery/S256/resource/issuer, private grants/refresh and local-first disconnect ([evidence](todo-owner-evidence-through-2026-10-16.md#generic-pre-registered-mcp-oauth-2026-10-15)). Real provider/browser acceptance and automatic client registration remain open; (c) apply a change without a restart (rebuild the host and the registry together, or neither); (d) `tools/list_changed`; (e) a first `npx` start that downloads for longer than the twenty-second handshake (warm the cache from `jarvis mcp add`, or a longer first deadline); (f) an `jarvis eval` suite for MCP use that measures whether the fence introduction makes a model ignore real output, and compares wordings.
<a id="p11-001c"></a>

- [x] `P11-001c` **Abilities: one click, in plain words** (`ADR-0169`; the owner found "add a server" meaningless). A catalog (`mcp_abilities.rs`) of ready-made entries with a plain name, what it does, what it **asks about** and what it **sends away**, a **pinned** package and its read-only tools: **Browse the web** (`@playwright/mcp` 0.0.83, `--isolated --headless`, Edge on Windows) and **Look up developer documentation** (`@upstash/context7-mcp` 4.3.0). `POST|DELETE /api/v1/mcp/abilities/{id}`; the download is prepared in the background under the allowlisted environment so the first start does not time out; the page shows off, getting ready, ready after restart, on, problem, and **one Restart now**; Node.js missing is said next to the button. `jarvis mcp on|off NAME`. The command form moved under "Advanced"; "MCP port" is now "Let other apps use JARVIS (port)". **A server can name its read-only tools** (`[servers.posture] read_only_tools`, the owner's statement; exact names; fail closed), because standing trust is deliberately ignored for outward-reaching tools, so a browser's reading could not run freely otherwise. **Live 2026-10-10**, fresh profile: Switch on, getting ready, ready after restart, Restart now; 25 browser tools with 12 reading (no approval) and 13 acting (ask); the model read a JavaScript-only page (`quotes.toscrape.com/js/`) with no approval; a page with a hidden instruction to fetch an attacker URL and say "PWNED" was reported to the user and not obeyed (one sample, one model: evidence, not proof); no browser or node process left after stop. Tests: abilities (pinned, unique, no acting tool trusted, request shape), per-tool read-only (policy, host config over a real child), CLI rendering. Docs: `docs/user/abilities.md`.
<a id="p11-002"></a>

- [x] `P11-002` **Tool discovery, so many tools cost no tokens until needed** (`ADR-0176`). Built and measured live: a plain request fell from 8,097 to 4,282 input tokens (-47%) with the browser's 25 tools registered; groups of more than six MCP tools are held back until the model calls `jarvis.tools.search`, which makes the whole group available for the rest of the run. `evals/discovery.toml`: the model found and used the browser in 3 of 3 attempts. **Open:** meaning-based ranking, other models (the eval case is how a model that never searches is caught), and the same for inbound callers (they still see every tool). **Extended 2026-10-11 (`ADR-0197`):** the built-in `calendar` and `schedule` families are held back too (groups `jarvis-calendar`, `jarvis-schedule`), and search ranking ignores filler words (a calendar question had opened the browser group). Measured: the `basics` per-call baseline 8,214 to 6,987 input tokens (-15%), 9/9 both times; `evals/discovery.toml` now has a case per family, 4/4 on two runs with `glm-5.3-flash`. The baseline had grown from 4,282 (above) to 8,214 as native tools were added (about 9,000 estimated tokens of 44 tools offered on every call); the biggest families still always offered are files (8 tools, about 1,800), Gmail (5, 1,250), projects (6, 1,100), memory (4, 900) and contacts (5, 860). Shortening those descriptions or deferring more families is the next saving and needs the same measurement. **Refined 2026-10-11 after comparing OpenClaw and Anthropic's tool search (`ADR-0198`, `docs/research/tool-catalogs-and-prompt-cost.md`):** search ignores word endings ("scheduling" finds "schedule") and the tool asks for English queries (a Dutch case in `evals/discovery.toml` passed twice), found tools and the search tool go last so the request start is stable (the provider reports 71% of input tokens cached, the saving itself not measured), and `jarvis tools list` counts held-back tools apart instead of summing them into "each call is sent". Not adopted: OpenClaw's describe/call proxy (it hides the real tool name from policy and audit) and a prompt directory of every tool name.
<a id="p11-001d"></a>

- [x] `P11-001d` **Connect apps: other agents use JARVIS with one switch and one key** (`ADR-0170`; the owner asked for "MCP on or off so external agents can connect with an API key"). The inbound endpoint existed but admitted nobody and spoke only the newest protocol. Now: a Connect apps tab and `jarvis mcp access [on|off|key]`; a Bearer key (constant-time check, fixed caller fingerprint, immediate rotation); a dual-era server (2026-07-28 and the 2025 versions); no folder required. Verified live with the official TypeScript SDK client (401 without the key, 26 tools with one folder and 21 without, `jarvis.agent.delegate` returned the right answer, an approval-needing tool refused with nothing left waiting, rotation at once, switch off closes the port). **NOT run live:** the Cursor and VS Code snippets in those products, Claude Desktop (stdio only). Open: a key per app, remote access with its own authentication, a per-key tool list.
<a id="p11-001e1"></a>

- [x] `P11-001e1` **The browser and documentation lookup come with JARVIS** (`ADR-0171`; the owner: "abilities seem so basic they should just be baked in"). Added to every profile on first start (once, recorded in `config/abilities-seeded.txt`), downloads waited for up to 30 s, opt out with `JARVIS_NO_BUILTIN_ABILITIES=1` or Switch off. Run live on a fresh profile: 25 browser and 2 documentation tools present after the first `jarvis start` (13 s). **NOT done:** a native Rust browser (still needs Node.js).
<a id="p11-001e2"></a>

- [x] `P11-001e2` **The endpoint for other agents serves the browser and documentation tools** (`ADR-0172`). Run live: 48 tools listed, a navigation ran, an acting call was refused with nothing left waiting. **NOT run live:** against ElevenLabs or any cloud agent.
<a id="p11-003a"></a>

- [x] `P11-003a` **Keep the browser's page snapshots out of the working directory** (`ADR-0192`). The pinned Playwright MCP `0.0.83` help confirms `--output-dir`; the built-in command points it at `<profile runtime>/playwright-mcp`. Startup migrates only the exact old seeded command, leaves owner-edited `browser` entries untouched, and derives `vetted_prefixes` from the same runtime-specific catalog command. Tests prove the fresh path, exact migration, custom preservation, and old/new vetted posture. Full workspace gates passed 2026-10-11.
<a id="p11-001f1"></a>

- [x] `P11-001f1` **Reach from the internet with one switch** (`ADR-0173`, `docs/research/integrations/cloudflare-tunnel.md`). A managed Cloudflare quick tunnel: pinned and checksum-verified download, `--http-host-header` so the Host guard is unchanged, key still required, both header forms accepted, wish survives restart. Run live on Windows: 9 s cold start, 401 without the key, calls through the public address, off kills the address, `jarvis stop` leaves no process. **NOT run live:** a real ElevenLabs agent, macOS, Linux, a network loss.
<a id="p11-004a"></a>

- [x] `P11-004a` **Two live defects fixed** (`ADR-0174`): an approval whose call had already started (daemon stopped mid-call) stayed listed and every answer was `409`; scheduled results were written into the open conversation. Now: hidden from the list and payload dropped at start; results go to a "Scheduled results" conversation with an unread count. Calls left authorized/submitted are now settled at start (`P11-004e`).
<a id="p11-004b"></a>

- [x] `P11-004b` **"Run without asking" now works for third-party tools** (`ADR-0175`): the owner set the browser click to trusted and it still asked, because every outward effect ignored trust and unclassified MCP tools count as outward. Trust by name now releases a third-party tool (native mail and calendar senders still always ask); the Permissions screen says "still asks" where trust cannot apply. Run live: trusted `browser_click` ran in a real task with no approval, `browser_type` still asks.
<a id="p11-004c"></a>

- [x] `P11-004c` **The owner can trust the memory and project tools by name** (`ADR-0177`, amends `ADR-0155`): the Permissions screen refused "Run without asking" for five tools, so an owner who set everything to run freely still got approvals. Now accepted and honoured, one tool at a time; no one-click "Always allow" on the card. Superseded by `P11-004d`.
<a id="p11-004d"></a>

- [x] `P11-004d` **Every tool can be trusted by name** (`ADR-0178`): the last carve-out (native tools that reach other people) is gone. A confirmation in Permissions says why it is the sharp choice. The owner's saved `calendar.create`=trusted now takes effect; `gmail.send` stays as saved (ask).
<a id="p11-004e"></a>

- [x] `P11-004e` **A call a stopped daemon left half-way is settled honestly** (`ADR-0179`): authorized becomes failed (never sent, safe to repeat), submitted becomes unknown (may have happened, not repeated blindly); the restart continuation names the uncertain tools. Run live (row rewritten to submitted, daemon restarted, row became unknown, logged). **Open:** show an unknown call on screen in the console.
- [x] `P11-001e1` **A call from another app that needs the owner waits for the owner** (`ADR-0180`): in-memory list shown with the console's approvals, answered by click, `jarvis approvals approve|deny` or voice; refused on no, silence (2 min) or a full list (8). Run live through the real endpoint: approve released the call, deny refused it. **Open in `P11-001e`:** a key per agent (so the owner can see which agent asked, and revoke one), the durable audit trail for remote calls, an alert when a call starts waiting, and a coding agent launched by JARVIS getting its own key automatically.
<a id="p11-004f"></a>

- [x] `P11-004f` **Found by a full browser test of the shipped build** (`ADR-0181`): Connect apps took a port another daemon held and the restart left a dead console with no explanation. Now a free port is chosen and a failed start says why. Everything else tested in the browser passed: all eight settings tabs, Connect apps switch/restart/key/snippets/internet address, calls through the public address, chat with tool discovery, the developer form (test/add/remove), the Folders grant and the approvals card.
<a id="p11-001g"></a>

- [x] `P11-001g` **A stable address** (`ADR-0202`, `docs/research/integrations/cloudflare-tunnel.md`, "Named tunnel"). The owner's own Cloudflare tunnel: hostname + token saved once (HUD card, `jarvis mcp access public named HOST`, `POST/DELETE /api/v1/mcp/access/public/named`); with none saved the quick tunnel is used as before. Token checked for shape, kept in a private file, never returned or logged, passed in `TUNNEL_TOKEN`; "on" only after `Registered tunnel connection`; Cloudflare's refusal shown as a Problem; the Host is rewritten to loopback only after the key check and only for the exact saved name; a change restarts a running tunnel without a race. Tests include falsifiers for the Host match (mutation checked). Run live: the pinned `cloudflared` with a well-formed fake token accepted every flag and answered `Failed to get tunnel`. **NOT run live:** a real tunnel registering and serving the endpoint, ElevenLabs through it, macOS/Linux. Not built: creating the tunnel/route/DNS through the Cloudflare API.
<a id="p11-001f"></a>

- [ ] `P11-001f` **Telephony, the rest.** Reach exists (`P11-001f1`). Still to do: a real ElevenLabs agent connected to the endpoint (needs the owner's ElevenLabs account and key; record whether it sends the secret token with `Bearer`), a phone number through Twilio or ElevenLabs, and acting tools for the caller (`P11-001e`). Follow `docs/research/integrations/elevenlabs.md` and `twilio.md`.
<a id="p11-001e"></a>

- [ ] `P11-001e` **Agents JARVIS launches use JARVIS tools, with approvals** (`ADR-0171`, "Agents JARVIS launches"). A key made per launch and revoked at its end; a tool that asks parks a normal approval for the owner (one click) and the agent waits, with a time limit; the call belongs to a run record so it is audited; a denial or timeout never runs the tool (falsification test). Needs `ADR-0039` amended. Do together with the first coding-agent runtime (`P7`).
<a id="p11-003"></a>

- [ ] `P11-003` **A browser, through Playwright MCP** (closes the browser part of `P9-069`). **Mostly delivered by `P11-001c` (a one-click ability, reads free, acting asks, a page hostile to the model resisted once).** Still open here: origin allow and block lists, a visible (non-headless) mode, a `jarvis eval` suite with a hostile page that measures the resistance over many runs and models, and the trust-boundary ADR text below. Original scope: Playwright MCP (`npx @playwright/mcp@latest`, Node 18+, accessibility snapshots, `https://github.com/microsoft/playwright-mcp`, read 2026-10-10) installed as the first MCP server from a one-click "turn on the browser" that checks for Node and says what is missing. Trust-boundary ADR first: a throwaway profile, no access to the owner's files or sessions unless the owner signs in inside it, page text fenced as untrusted, origin allow and block lists, reading a page runs freely and submitting a form or buying something asks. Falsification: a hostile page that tells the model to read a local file or send mail must fail closed. Live: a JavaScript-rendered page read in the Sales project, and a `jarvis eval` case for it.
<a id="p11-004"></a>

- [x] `P11-004` **The model proposes a skill; the owner approves with one click** (`ADR-0183`, `docs/user/skills.md`; merges `P6-009`). `jarvis.skill.propose` (description whose first line says when to use it, up to twelve steps naming tools that exist), stored `Proposed` and authored by the model's own actor; a "learn this?" card in the console and `POST /skills/{id}/approve|dismiss` with no body (the owner is the approver, so never a self-approval); at most ten waiting, duplicates answered with the waiting one. Selection could not fire for a real request (every word of the objective had to appear in the skill), so a second rule offers a skill when the request covers 75% of its first line's distinctive words. **Live, real model:** the model proposed a five-step procedure, the card appeared, Approve activated it; the next similar request then ran the procedure's `contacts.search` and `contacts.save`, and after Dismiss a similar request did neither. Tests: 2 domain, 5 tool. **Open:** the measured fewer-steps-or-tokens comparison with `jarvis eval`; a Telegram approve button (`P11-005`); the model revising an existing skill; editing a proposal before approving; meaning-based matching.
<a id="p11-005"></a>

- [~] `P11-005` **Telegram, so JARVIS is in the owner's pocket** (first slice of `P9-010`). **Built 2026-10-11 (`ADR-0184`, `docs/research/integrations/telegram.md`, `docs/user/telegram.md`). The owner ran it on a real phone 2026-10-11: works. That run showed raw Markdown in answers, fixed by `ADR-0186` (Markdown converted to Telegram HTML, plain-text fallback); the owner's phone is the check for the new formatting.** A Telegram tab in Settings (token saved write-only, no restart), pairing by a six digit code, only the paired chat answered, text in and answers out, scheduled results to the phone, approvals as Approve/Deny buttons (one tap). Tested against a stand-in for the Bot API (9 tests) and run live with the real daemon and model: Settings to paired, a question answered, a stranger ignored, an approval tapped and the schedule created. **Open:** the owner's confirmation of the new formatting on the phone; files, photos, voice notes (`P8-010`); a skill-approval button. Original text: Research record first (the Bot API long polling, file limits, inline buttons, the exact version). Setup: create a bot with @BotFather, `jarvis keys set telegram`, send the bot a message, confirm the shown pairing code once in the console or `jarvis pair telegram CODE`. Only the paired chat is answered; every other sender is ignored without a reply. Text and files in, answers and attachments out, approvals as buttons (one tap, `ADR-0136`; the ADR for this slice decides what a paired chat may approve). Long polling, so no public address and no tunnel. Voice notes in and out follow `P8-010`. Live on the owner's phone. Other chat platforms follow as separate slices.
<a id="p11-006"></a>

- [x] `P11-006` **Model routing** (`ADR-0196`): a model per kind of run, default unchanged (one model). **Built:** `daemon.executor_subagent_model_name` and `daemon.executor_background_model_name` (Settings, Brain; same provider as the main model), the kind read from how a run began (sub-agent framing; scheduled framing; mail, file or webhook trigger framing; a resumed run keeps its kind), so no migration and a run keeps one model through an approval. Tools, approvals and caps do not change. Every `usage_updated` event now records the model, and `jarvis digest` lists calls and tokens per model (older calls shown as "model not recorded"). Tests: 2 routing (kind from framing and resume, a kind without its own model uses the main one), 1 executor (routes by kind, a bad name is refused naming the field), 1 config (names validated, need a live provider), 1 executor (the usage event names the model), 1 digest storage/route (grouping, unnamed group), 2 CLI; a mutation making sub-agents ignore their model failed the test. **Run live 2026-10-11 on the owner's daemon (`glm-5.3-flash` main, `gpt-oss:20b` routed for both kinds):** a direct question used `glm-5.3-flash`; a sub-agent it started and a webhook-triggered run used `gpt-oss:20b`, and `jarvis digest` showed `glm-5.3-flash: 3 calls` (the question plus the two calls of the run that delegated) and `gpt-oss:20b: 2 calls` (the sub-agent and the webhook run). The test routing was removed afterwards. **The harness comparison (`basics`, 9 cases, one run each, 2026-10-11):** `glm-5.3-flash` 9/9, 117,552 tokens in, 1,822 out, 28 s; `gpt-oss:20b` as the main model 9/9, 96,063 in, 1,671 out, 35 s. One run each on a read-only suite of easy cases only shows that the cheaper model is not worse on these, not that it is good enough for research or mail work. **Still open:** the same comparison on the Sales (`prospecting`) suite and on longer tasks, which is what decides a recommended background model, so none is recommended; a model per project; a model for summaries (there is no separate summary call yet); the Brain tab has the two fields but a Settings picker like the main model's was not added. (The `basics` input cost grew 23% against the 2026-10-10 run, 94,944 to 117,552: the same suite sends a larger request now. Per call the baseline is about 8.2k tokens, under the suite's own 9,500 limit; where the growth came from is not investigated.)
<a id="p11-007"></a>

- [ ] `P11-007` **The first five minutes.** Guided `P9-013` setup is built (`ADR-0214`, 2026-10-14): terminal/first launch reuse console provider presets, live model listing and bounded completion testing; pinned `rpassword` hides model/voice keys. Per the owner's explicit choice, new setup requires a working ElevenLabs TTS key and short synthesis before saving; default voice ID `liN4AsrhF3CylqMDWK6p`, not a key. Existing/headless profiles and later browser fallback are preserved. Existing home Development/Projects/code folders are offered, never auto-granted. `cargo check -p jarvis-cli` passed and `cargo test -p jarvis-cli init::` passed 20 tests on Windows, including 10 new helper/contract checks for exact default/overrides, rejected or missing keys/tests, quota/empty audio, no widened grants and preserving saved configuration. Source bootstrap preflight/docs now distinguish Windows C++/SDK, macOS developer tools, Ubuntu compiler packages and source-only Rust 1.98.1; builds use `--locked` and piped setup prints a key-safe handoff instead of prompting/running setup. Parent `node --test tests\installer.test.mjs` passed 24 stand-in tests and `npm run check:install` passed; child frontend tests passed 38 and build passed. **Still open:** exact voice on an owner account, hosted model providers, native hidden-entry and clean Windows/macOS/Ubuntu setup runs; plain-language doctor feature unlocks; verified-binary publishing/install/update gates (`P9-004` to `P9-007`). Apache-2.0 is selected (`P9-017c`); that is not a signed public release.
<a id="p11-008"></a>

- [x] `P11-008` **A spend guard inside the run** (`ADR-0191`): before a model call and after every persisted `usage_updated` event, stop before another model or tool call when the project's rolling 24-hour cap is reached. The billable call that reaches the cap stays counted; a run started after exhaustion makes no provider call; the failed run says why and a linked `blocker` is written to the project journal. Optional `daemon.workspace_daily_token_limit` (zero is unlimited, set from Settings, Brain or with `jarvis config set`) applies to every run in the workspace. Configured guards fail closed with `token_limit_unavailable` when scope or usage cannot be read. Tests: exact project cap plus journal, aggregate workspace cap across runs, exhausted preflight, config round-trip and settings validation. Full workspace gates passed 2026-10-11.
<a id="p11-009"></a>

- [ ] `P11-009` *(Parked until `P11-003` is proven.)* **Hands on the desktop**: operating system applications through an accessibility-based MCP server, behind the same approvals. High effect; started only with a concrete job the owner wants done.

The next ten items come from the capability wishlist reviewed 2026-10-11 ([docs/product/capability-wishlist.md](../product/capability-wishlist.md), 25 wishes mapped to this ledger). Each starts with its research record (`docs/development/external-research.md`) and, where a protocol exists, tries an MCP server before writing code (`ADR-0167`).

<a id="p11-010"></a>

- [~] `P11-010` **JARVIS writes documents** — **built 2026-10-11 for `.xlsx`, `.csv` and `.docx` (`ADR-0185`) and `.pdf` (`ADR-0190`: standard fonts, no new dependency, rendered by pdf.js; characters outside Western European text are counted and reported, `docs/research/integrations/office-writers.md`): `jarvis.files.write_document` creates a new file (never replaces), text only never formulas; 12 writer and 4 tool tests; a real model wrote a workbook and a report and the real Excel and Word opened both with no repair. Open: `.pptx`, charts, the sales spreadsheet moved onto it.** Original text: (wishes 6, 10, 15). Reading exists (`P9-075`); writing does not: the sales project builds its spreadsheet with a hand-written script. One tool, `jarvis.files.write_document`, inside the granted folders like the other file tools: `.xlsx` (sheets of rows, header, column widths), `.csv`, `.docx` (headings, paragraphs, tables, lists), `.pdf`, then `.pptx` and charts as images. Research first: Rust crates against an MCP office server (licence, size, whether the file opens in the real Office). Done when each format is written by a real run and **opened in the real application** (or a validator that Office itself uses), and the sales spreadsheet is produced by JARVIS rather than by a script.
<a id="p11-011"></a>

- [~] `P11-011` **Images, seen** — **screenshots built 2026-10-11 (`ADR-0188`): a picture an MCP tool returns is kept (PNG, JPEG, GIF, WebP by signature, bounded, pruned), the model is told it was shown, the console shows it in the chat (and after a reload) over an authenticated route; run live with the real Playwright server and a real model. Telegram sends the pictures of a run before its answer (`sendPhoto`, falling back to a file); open: a screenshot saved under a filename (the server does not return it), image generation (below), charts.** Original text: (wishes 8, 9). The console shows an image the model produced or a tool returned (a browser screenshot, a chart, a generated picture) inline in the chat instead of a path nobody can open; images stay in a granted folder or the run's own store, never a public address. Then image generation as a tool through a provider or MCP server the owner configures (the key in Settings, the cost visible). Done when a screenshot taken by the browser appears in the chat.
<a id="p11-012"></a>

- [ ] `P11-012` **Authenticated browsing** (wish 5). The built-in browser can keep a **logged-in profile** the owner signs into once (a visible window), so JARVIS can use sites that need a login; cookies stay in a private profile folder, are never sent to a model, and a site is only used when the owner added it. Includes calling an API with an OAuth token the owner granted. Done when a logged-in page is read on a real account the owner names, and the profile can be wiped from Settings.
<a id="p11-013"></a>

- [ ] `P11-013` **Runtime without Docker, and long-running processes** (wishes 11, 12). Today code needs a container the owner set up. Research a no-Docker route (a Python or JavaScript runtime in a WASM sandbox, or a confined child process with the operating system's own sandbox) so "run this analysis" works on a fresh install; then sandboxed long-running processes (a dev server, a watcher) with a named port reachable only from this machine, listed in the console and stopped from it, with an idle limit. Done when a Python-only library runs on a machine without Docker and a dev server is started, reached and stopped.
<a id="p11-014"></a>

- [ ] `P11-014` **Knowledge over the owner's own files** (wishes 14, 16). *First half built (`ADR-0206`, 2026-10-13): `jarvis.knowledge.search` over a full-text index (SQLite FTS5, migration 25) of the granted folders: text, code, PDF, Word, spreadsheet and slide files, re-indexed when a file changes (size and time), entries removed when the file or its folder goes, hidden names, generated folders, links, binaries and big files left out, answers with the fenced passage and where to read on. Tested over a real database and real folders, including an order hidden in a file, a link out of the folder and a folder taken back. NOT run live against the owner's own folders and a real model. Open: vectors beside the full-text table (`sqlite-vec` or pgvector) once a measurement on real questions shows words are not enough (stemming, "renewal" vs "renews"); scanned PDFs and pictures; a console line showing what is indexed; the cross-project view.* Local retrieval over granted folders: an index kept in the profile's database (full text first, vectors through `sqlite-vec` or the server profile's pgvector when measured to help; research `docs/research/integrations/embeddings.md` already exists), a tool that answers with the file and passage, re-indexing when files change, and nothing indexed that the owner did not grant. Then a cross-project view of what memory already stores (entities and relations): "everything we know about this company". Done when a question about a file the model never opened is answered with the passage and its source.
<a id="p11-015"></a>

- [ ] `P11-015` **A vault for secrets** (wish 17). Keys are private files in the configuration directory today. Move them to the operating system's credential store (Windows Credential Manager, macOS Keychain, the Secret Service on Linux) with the file as the fallback, behind the same write-only Settings and `jarvis keys`; the migration is automatic, one-way and reversible, and no secret is ever shown or logged. Research the crate first (`ADR-0020`'s rule about adapters). Done when each key kind round-trips on the three platforms in CI and an existing profile migrates on first start.
<a id="p11-016"></a>

- [~] `P11-016` **(built 2026-10-11, `ADR-0189`: `calendar.create` refuses an overlap with a busy event unless `allow_overlap`, `calendar.events` lists ids, `jarvis.calendar.delete` asks, `jarvis.contacts.duplicates` previews and `jarvis.contacts.merge` asks and returns what it removed; tested against a Google stand-in and live for contacts with a real model. Not run on the owner's real Google calendar: no sign-in here, so the owner confirms a real overlap and a real delete.)** Original text: Calendar that checks before it books, contacts that merge, and a test event can be removed** (the audit left five `JARVIS tool test` events it could not delete: `jarvis.calendar.delete`, asking like create) (wishes 18, 19). Free/busy and conflict detection before `calendar.create` (the Calendar API's own free/busy query; the tool says what it collides with, and still asks), and `jarvis.contacts.merge` with a preview of what would be combined, never silent, undoable by the existing history. Done when a booking over an existing event is flagged on the owner's calendar and two real duplicate contacts are merged after one approval.
<a id="p11-017"></a>

- [ ] `P11-017` **Inbound webhooks** (wish 2). Part of `P6-012`: an authenticated local route with a secret path, reachable from outside only through the tunnel (`ADR-0173`) the owner chose, that starts an ordinary run with the payload as untrusted data. Done when a request from a real service starts one run, a replay starts none, and a bad signature or path starts none.
<a id="p11-018"></a>

- [ ] `P11-018` **Monitoring and uptime checks with alerts** (wish 22). A schedule kind that fetches an address or runs a check, remembers the last state, and **tells the owner only when it changes** (Telegram, push, desktop), with a daily cap on alerts (`P6-007`). Built from schedules, web fetch and Telegram; no new surface beyond "watch this and tell me when it changes". Done when a real site going down and coming back produces exactly two messages.
<a id="p11-019"></a>

- [ ] `P11-019` **Messaging beyond Telegram** (wish 3). Slack first (a real bot API with a socket mode that needs no public address), then WhatsApp and SMS through Twilio (`docs/research/integrations/twilio.md`; needs a number and a business registration, a decision for the owner). Each is a gateway client on the pattern of `ADR-0184`: paired once, only the paired identity answered, approvals as one tap or one word. Done per platform when the owner's own account answers a question and approves a tool.
<a id="p11-020"></a>

- [ ] `P11-020` **Maps, routing, invoicing and payments** (wishes 23, 24). MCP servers first: a maps and routing server (geocoding, distance, directions) for "how long from here to the client", and an invoicing path for the sales project (generate an invoice as a document through `P11-010`, send it as a draft for approval). **Taking or moving money is never unattended**: any payment tool is risk 3 and asks every time, and is not offered until the owner names a provider and a job. Done when a route is answered from a real server and one invoice is drafted for the owner to send.
<a id="p11-021"></a>

- [ ] `P11-021` **Skill discovery that works** (wish 21). `npx skills find` crashes on exit when JARVIS runs it as a command; find why (the process's exit under the command tool's sandbox, a missing terminal) and give the model a reliable way to look for an installable skill or MCP server when it lacks a tool, with the install itself always asked (`P11-001`'s rule: a server is a program that runs here). Done when the model finds a server for a task it cannot do, proposes it, and it is added after one approval.
<a id="p11-022"></a>

- [ ] `P11-022` **JARVIS keeps its own list honest** (wishes 20, 25). `jarvis eval --json` writes a **series** (one row per run: date, suite, case, pass, tokens, steps) the owner can chart, and a scheduled task re-reads `docs/product/capability-wishlist.md` each month, re-researches the open candidates (new MCP servers, new crate releases, changed prices), and writes what changed to the project journal as a proposal, never as a change. Done when two eval runs add two rows to a CSV and the monthly task produces a journal entry the owner can accept or ignore.
<a id="p11-023"></a>

- [~] `P11-023` **Findings of the owner's capability audit** (`C:\Users\Gebruiker\Desktop\JARVIS-FOLDER\ai-capability-audit`, seven cycles of JARVIS testing its own tools, read 2026-10-11; `ADR-0187`). **Fixed and run live (2026-10-11):** (1) `browser_file_upload` and `browser_drop` came back `the tool reported: unknown` with no reason: the MCP adapter built the failure reason from the server's text, and the outcome recorder refuses control characters (Playwright's errors have line breaks) and counts **bytes**, so the record could not be built and the call was reported as `unknown`. Reasons are now flattened and byte-bounded, so the model reads the real error (the likely cause of the upload failure, which was never visible). (2) `jarvis.tools.search` answered "nothing matched" for "gmail calendar contacts" because it searched only held-back MCP groups; it now also names matching tools **already offered**. (3) `jarvis.agent.list` (new): the four sub-agent slots were invisible; sub-agents parked on an approval no longer hold a working slot (a hard cap of 16 unsettled remains). (4) `memory.correct` refused a long correction with "a search key must be 1 to 32 words", read as a limit on the quotation; the limit is on the new claim (32 different words), now said in the schema and in plain words before anything is asked. (5) `shutdown.exe` failed with error 203 in `command.run`: the inherited environment lacked `COMPUTERNAME` (and `USERDOMAIN`). (6) `npx skills find` "crashes on exit" (`-1073740791`): a libuv assertion inside the `skills` CLI whenever telemetry is off (`DO_NOT_TRACK`, `DISABLE_TELEMETRY`), which JARVIS sets on purpose; the privacy default stays and the command result now carries a note explaining the crash after complete output. **Open:** `calendar.delete` (added to `P11-016`); a per-project allowlist for unattended runs (the audit saw `gmail.send` and `schedule.remove` complete unattended in cycle 7: it follows the owner's trust settings and prior approvals, and a scheduled run that may send mail unasked is a decision for the owner, not a bug); a batch of parallel calls is refused after one waits for approval (`an earlier call in the same turn was waiting`: by design, to keep the approved call's arguments honest; the model should send held actions on their own); `read_document` drops spreadsheet cell coordinates; `schedule.list` keeps fired one-shots; a hint for the Windows Store `python` stub (exit 9009) pending `P11-013`.
<a id="p11-024"></a>

- [x] `P11-024` **A long run does not outgrow the model's context** (`ADR-0194`; found by review 2026-10-11, not by the audit). Every tool result (up to 12,000 characters) stayed in a live run's transcript and was resent on every call, so about forty large results overflowed a 128k window and the run failed (`model_context_overflow`) half done; only a *parked* transcript had been bounded. Now, before each model call, over a 200,000-byte soft budget the oldest large results are replaced by a one-line "left out, N characters" note down to a low-water mark (one cache break, then room for many calls). Kept whole: the six newest results, every assistant turn and tool call (pairing intact), the assembled brief/journal/memory/history, discovery results (the offered tools are derived from them), and small results; nothing of an old result is quoted. The stored events and audit are unchanged and a `context_trimmed` activity event shows it. Tests: 9 unit (within budget untouched, oldest first and newest kept, pairing, idempotent, protected prefix, discovery results and the offered tools unchanged, small results kept, **results the model has not yet answered are never left out**, an earlier turn goes once the model has moved on) and 1 end-to-end over a 30-step run asserted on what the model was actually sent. **Run live 2026-10-11 against `glm-5.3-flash`, and the first version was wrong:** thirty file reads asked for in one turn had their oldest twenty-four left out before the model read them, so it re-read and then listed values it had not seen; fixed (unseen results are protected) and re-run, see the result below. **Live results (real daemon on the rebuilt release binary, `glm-5.3-flash`):** the same request, thirty reads asked for in one turn, then returned all 30 correct codes with 30 reads and no trim (30 of 30, against about 17 of 30 before the fix); thirty reads forced one per turn trimmed twice, completed, and again returned 30 of 30. **Not measured:** how often a real model needs a result it no longer sees beyond these two runs, and the right budget per provider; there was no long-task `jarvis eval` suite then (there is one now, below). Large tool-call *arguments* are not shortened. **Closed 2026-10-11 (`ADR-0199`):** the owner's own runs showed the gap (24 hours: 272 runs, 115M input tokens, 91% cached; a research sub-agent that saves its findings made 219 calls and reached 176,535 tokens, others 100,000 to 109,000, because its results were two words and the bulk was text it had written). When results are not enough, long strings in the arguments of old, already-answered calls become `[left out of the working context: N characters, already sent]` (valid JSON kept, newest calls and unanswered calls never touched, results go first). 3 new unit tests (a mutation disabling the pass fails 2 of them). **Live:** 26 files of 120 lines each written in one run trimmed once (`arguments: 4`, 83,944 bytes), 14 calls, largest request 51,183 tokens, and all 26 files exactly right (3,120 lines compared), including the ones written after the trim. **Measured with a suite (`evals/long-task.toml`, `--repeat 3`, two cases, `glm-5.3-flash`): 6 of 6 attempts passed; every file of all six checked on disk (156 files, 18,720 lines) was exact with no placeholder text, and each run's largest request stayed at 51,297 to 51,553 tokens whether it made 7 calls or 28 (63,000 to 69,000 tokens written each).** The placeholder-copy risk is 0 of 6 on one model and provider, not excluded for others; the 200 KB budget itself is still untuned (the suite is now how to tune it, for example with `--repeat` on another model).
