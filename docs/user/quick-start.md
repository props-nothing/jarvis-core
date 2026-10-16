# Your First Run

**One assistant. Your model. Your folders. Your rules.**

[Documentation](../README.md) / [Settings](settings.md) / [Platforms](platforms.md) / [Troubleshooting](#stopping-and-troubleshooting)

For someone who wants to use JARVIS, not build it. Contributors: see [development/getting-started.md](../development/getting-started.md).
The target experience, and what is still missing from it, is in [product/onboarding.md](../product/onboarding.md).

## What you need

- A model. The easiest is [Ollama](https://ollama.com) running on this machine (local models, or Ollama Cloud models through it).
  Any OpenAI-compatible server works too.
- An **ElevenLabs API key for new setup**, supplied privately; it is not the voice ID. The default voice ID is `liN4AsrhF3CylqMDWK6p`. Existing profiles keep working without repeating setup; this requirement does not change headless runtime behavior.
- Optionally Docker, if you want the assistant to run code in a throwaway container.
- One program, `jarvis` (the assistant it runs in the background is the same file, started as `jarvis daemon`). Either unpack an archive from a successful [prerelease](https://github.com/props-nothing/jarvis-core/releases)
  or CI workflow artifact (these builds are unsigned; Intel macOS is cross-built without runtime smoke testing), or build it with a Rust toolchain:
  `cargo build --release -p jarvis-cli --locked`, then take it from `target/release/`.

Only source builds need Git, Rust via rustup and native build tools: Visual Studio C++ Build Tools + a Windows SDK, Xcode command-line tools on macOS, or `build-essential` on Ubuntu. Follow the [platform-specific prerequisite steps](../development/getting-started.md#build-from-source). An installed executable needs no Rust. The website's [source install commands](../operations/install-and-release.md#source-bootstrap-available-today) build and install the program; they are not signed binary downloads and do not run setup through piped input.

Start the built program directly before it is on your PATH:

```powershell
# Windows, from the repository root
.\target\release\jarvis.exe
```

```bash
# Linux / macOS, from the repository root
./target/release/jarvis
```

After installation, the shorter `jarvis` commands below work in a new terminal.

## Installing it

`jarvis install` copies the program to your own folder (`%LOCALAPPDATA%\Programs\JARVIS` on Windows, `~/.local/share/jarvis` elsewhere), puts it on your PATH so `jarvis` works in any new terminal,
and with `--service` starts it at login. It needs no administrator rights and never touches your settings or memory; `jarvis uninstall` takes it out again.

## First run

```text
jarvis
```

The first time, guided setup chooses a model/provider, asks for your required ElevenLabs API key with hidden input, and asks which folders, if any, the assistant may read and write. `~/Development` and `~/projects` are useful suggestions, **not automatic grants**. Only after setup succeeds does it start in the background and open the signed-in browser console. Later runs skip new setup and keep the existing profile. `jarvis --no-open` starts it without opening a window.

After a piped website install, open a new terminal and run `jarvis`; the installer only installed the program. For unattended setup, provide existing **private absolute key-file paths**, replacing these example paths and choosing existing workspace folders. Never put an API key value in arguments or shell history.

```powershell
# Windows; never start after a failed init
jarvis init --base-url https://api.example.com/v1 --model NAME --api-key-file C:\Secrets\model.key --voice-key-file C:\Secrets\voice.key --workspace "$HOME\Development" --workspace "$HOME\projects"
if ($LASTEXITCODE -eq 0) {
  jarvis start
  if ($LASTEXITCODE -eq 0) { jarvis doctor --live }
}
```

```bash
# Ubuntu / macOS; && prevents startup if setup fails
jarvis init --base-url https://api.example.com/v1 --model NAME --api-key-file "$HOME/private/model.key" --voice-key-file "$HOME/private/voice.key" --workspace "$HOME/Development" --workspace "$HOME/projects" && jarvis start && jarvis doctor --live
```

Alternatively, supply `ELEVENLABS_API_KEY` through a private environment and pass `--elevenlabs` instead of `--voice-key-file`. No key value is shown here or needed by the source installer. Model and voice validation errors stop setup; do not start a new profile after a failed `init`. Omit `--workspace` to grant no folders.

`--workspace` may be repeated. JARVIS's file tools read, list, search and create files inside those granted folders, including new Excel, CSV and Word files. Changing an existing file and running a command ask by default; explicit per-tool standing trust can change that. Approved commands and coding agents run with your host privileges: the selected working directory does not confine their filesystem access. See [coding boundaries](code-agents.md#review-and-trust-boundaries).

## Keys

| Key | What it is for | How to give it today |
| --- | --- | --- |
| Model key | A hosted model provider (not needed for local Ollama) | `jarvis keys set model --from-env NAME` (or `--file PATH`, or pipe it in), or `--api-key-file PATH` at `init`. |
| Fallback key | A second provider for when the main model is limited or down | `jarvis keys set fallback --from-env NAME`, after `executor_fallback_model_name` and `executor_fallback_base_url` are set (see settings.md). |
| Voice key | Required for new setup; a natural voice (ElevenLabs) | Hidden setup prompt, absolute `--voice-key-file PATH`, or `--elevenlabs` with private `ELEVENLABS_API_KEY`. For an existing profile: `jarvis keys set voice --from-env ELEVENLABS_API_KEY`, then `jarvis restart`. Existing profiles and browser fallback remain supported. |

```text
jarvis keys status               # which keys are set (never their values)
jarvis keys set voice --from-env ELEVENLABS_API_KEY
jarvis keys test model           # asks the model server to accept the key
jarvis keys test voice           # asks the running daemon to make a sentence of audio
jarvis keys remove voice
```

Keys are written to a private file and the configuration holds only its path; they are never printed, logged or sent to the
browser, and never taken as a command-line value (it would stay in your shell history). With no flag and a terminal, `keys set`
asks for it with hidden input. Environment names and absolute file paths are safe command arguments; the secret value itself is not.

## Changing settings

```text
jarvis config set code_sandbox node:22-alpine node -e    # the code container and its command are set together
jarvis config show               # every setting you can change, and whether key files are present
jarvis config get executor_model_name
jarvis config set executor_model_name deepseek-v4-flash:cloud
jarvis config set tool_workspace_roots C:/Users/me/notes C:/Users/me/projects    # replaces the list
jarvis config set policy.trust jarvis.files.edit        # tools allowed to run without asking
jarvis config unset speech_model
jarvis restart                   # apply
```

Each change is checked by the same rules the daemon starts with and refused, leaving the file untouched, if it would not
start (for example a voice id without a voice key). Changing one setting changes only that one: your folders, trust list and
voice stay as they were. The same settings and keys are in **Settings**, which opens on **Overview**. **Apps & tools** contains integrations and files/coding, **Channels** contains Telegram and Phone, and **External access** controls other apps calling JARVIS. Search finds setting names and help; uncommon system settings remain in Advanced. **Console** adjusts appearance, density and motion for this browser, immediately and even while offline. Brain picks a provider and model, tests it, and sets the fallback. Provider keys are password fields and are never shown again;
**Restart to apply** restarts JARVIS with what you saved, and the footer says which changes are still waiting for it. [settings.md](settings.md) explains every setting and why a few are off by
default.

## Using it

- **The console** (opened for you; `jarvis hud` opens it again) is a face with a conversation beside it. The face is the page: it has a
  mind of its own (it looks about, changes expression, blinks, nods off after a long quiet) and reacts to what is happening: it
  listens, thinks, speaks with the voice, looks worried when something fails and pleased when you approve. Type or talk in the panel on
  the right; **New chat** (or `N`, or say "new chat") starts a fresh conversation with none of the old context, and **History** reopens an
  earlier one where you left off. Conversations are kept in your browser, so closing the page does not lose them. Starting-point cards fill a message you can edit before sending; offline submissions keep the draft. Anything that needs a yes or no appears in a tray at the lower right of the face, with Approve and Deny. **Operations** (or press `O`) opens
  the operations page: what is running, scheduled and recent, and which tools it can use and which it asks about first. **Stop everything**
  appears in the header whenever a task is running or waiting. **Silence voice** stops spoken output without canceling a task.
- **Voice:** the microphone button (or `M`) to talk; turn on **Speak answers**; enable the **wake word** and say "Jarvis, ...".
  "Jarvis, stop" cancels everything. Voice recognition is your browser's (Chrome or Edge).
- **The terminal:** `jarvis chat`, `jarvis ask "..."`, `jarvis watch`, `jarvis approvals`, `jarvis cancel --all`, `jarvis runs`,
  `jarvis schedule` (or just ask: "remind me tomorrow at nine" schedules it after you approve), `jarvis memory`, `jarvis tools list`. Every command has `--json` where it prints data.
- **Watching it work:** while JARVIS works, the left of the face lists each action as it happens (searching, reading a page, running a command, a sub-agent) with the pages it found as links you can open, and each action also appears as a node round
  the face and stays there while the task goes on: point at one to see what it was, press it to jump to its step. After a minute and a half of quiet the list folds to one line. Scheduled tasks and sub-agents appear there as well. Finished answers list their sources. Nothing is sent anywhere for this; it is what the run already reports.
- **Long-running work:** give it a project (`jarvis project add`, or Ops → Projects): a goal, your standing guidance and a journal that every run of it is told, so a recurring task
  continues instead of restarting. See `docs/user/projects.md`.
- **While you are away:** `jarvis digest` says what it did (runs, outcomes, what waits for you, tokens, what each project decided). A task a restart cut off is continued once, and a model call the provider rate limits is waited out (up to about five minutes) rather than failing the task. Set a `push_topic` (Settings → Advanced, then `jarvis push test`)
  to hear on your phone, with no content, when an approval is waiting or a scheduled task finished.
- **Long tasks.** While it works you see a live line (the step it is on, what it is doing, how long, and "thinking (~11k tokens)" during a
  silent think), and the model is asked to tell you what it is about to do and to say when each stage is done. If a task stops, it says why
  and offers **Continue**. A task may take hundreds of steps; **Stop everything** is always there.
- **What it can do on a fresh install:** read web pages, hand work to helper agents, and offer to remember things. Files need a folder
  you grant (Settings, Folders & code) and containerized code needs Docker. Memory search and scheduling from conversation are available;
  Gmail and Calendar need Google sign-in. The built-in browser and documentation lookup need Node.js. [Abilities](abilities.md) explains setup and limits.
- **Reading documents.** With a folder granted, `jarvis.files.read_document` reads PDF, Word (`.docx`), spreadsheet (`.xlsx`) and slide (`.pptx`) files in it, a few thousand characters at a time, and treats their text as data to read, never as instructions. It also reads a Word file's headers, footers, footnotes and endnotes. It also reads a photo or screenshot (`.png`, `.jpg`, `.bmp`) and a scanned PDF as text, if you install the free **tesseract** program (Windows installer from github.com/UB-Mannheim/tesseract/wiki; macOS `brew install tesseract`; Linux the `tesseract-ocr` package, plus the language data for anything but English); the answer is marked as read from a picture, so expect small mistakes. It cannot read password-protected files or the old `.doc`/`.xls` formats. Together with `jarvis.gmail.save_attachment`, a PDF that arrives by mail can be saved and read.
- **Handing a coding task to a coding agent.** Grant a folder and select/test your ACP agent in **Settings → Apps & tools → Files & coding**. `jarvis.code.delegate` starts a durable job in the first permitted folder by default, including a non-Git or dirty working directory. Git/worktrees are optional; no automatic commit or overall deadline is imposed. JARVIS asks under normal policy, records activity and independent checks, and supports Stop and an explicitly approved fresh continuation. Recorded Codex tasks and Windows Codex cancellation exist; fresh sign-in, other-agent/platform cancellation and complete website behavior remain open. TODO tracks the native daemon lifecycle gate separately. This is host execution, not an OS sandbox. See [coding agents](code-agents.md).
- **Searching your own files.** With a folder granted, `jarvis.knowledge.search` answers "what do my files say about …" without knowing which file to open: it returns the matching passages (from text, notes, code, PDF, Word, spreadsheet and slide files), each with the file's path and where it starts, so JARVIS can read on. JARVIS keeps a search index of the granted folders inside its own profile database, updated at start and every five minutes (ask it to refresh if you just added a file). It leaves out hidden files and folders (such as `.env`), `node_modules`, `target`, `dist` and `build`, links, binary files, text files over 1 MB and documents over 10 MB, and it matches words, not meaning (so "renewal" does not find "renews"). Removing a folder from Settings, Folders & code removes its text from the index at the next start. The index only holds a copy of text you already granted; it is rebuilt from the folders and can be deleted at any time (`ADR-0229`).
- **Seeing what the browser sees.** When the built-in browser takes a screenshot, the picture appears in the chat (click it to open it full size) and is still there if you reload the page. In the Telegram chat it arrives as a photo just before the answer. JARVIS itself only knows a picture was shown to you, so ask it to describe the page and it reads the page's text. Ask for the screenshot without a file name: a screenshot saved under a name is only a file. Pictures are kept in the profile's `images` folder (the newest 120, at most 256 MB) and are only served to the page that holds your key.
- **Writing documents.** With a folder granted, `jarvis.files.write_document` creates a new Excel workbook (`.xlsx`), CSV, Word (`.docx`) or PDF (`.pdf`) from what you ask for: a report, a quotation, a list. It never replaces a file (a name that exists is refused) and runs without asking, like creating any file. Text is always written as text, never as a spreadsheet formula. The PDF uses the standard fonts, so it covers Western European text (accents, the euro sign, typographic quotes); a character outside that, such as Chinese or an emoji, is drawn as `?` and JARVIS tells you, and a Word file is the better choice for those.
- **Long pages and PDFs on the web.** `jarvis.web.fetch` reads a long page a part at a time (it is told where the next part starts) and can read a PDF at a link, up to 5 MB. What it reads is data, never instructions, and private addresses are still refused.
- **Organising files.** `jarvis.files.move` moves or renames a file or folder inside a granted folder (missing folders are created). It never replaces anything: if the new name exists it refuses. It runs without asking, like creating a file.
- **Remembering.** Say "remember that I like short answers". JARVIS offers it in **Waiting for you** as a "remember?" card; **Keep**
  makes it shape later answers, **Dismiss** forgets it. It asks because a web page it read could otherwise tell it what to remember.
- **Approvals are a yes or no.** Anything it asks about appears with Approve and Deny; you can also say "yes" or "no", or use
  `jarvis approvals approve|deny`.

## Take It Further

| Next step | Guide |
| --- | --- |
| Mail, drafts, attachments and calendar events | [Google](google.md) |
| Chat and approvals on your phone | [Telegram](telegram.md) |
| Calls through your ElevenLabs agent | [Phone setup and trust limits](phone.md) |
| More tools and another app using JARVIS | [MCP](mcp.md) / [connect apps](connect-apps.md) |
| Work that starts when something happens | [Triggers](triggers.md) / [projects](projects.md) |
| Approved procedures and delegated code changes | [Skills](skills.md) / [coding agents](code-agents.md) |

To check that a change made JARVIS better and not worse, run a suite of prompts against it and compare with the last run: `jarvis eval run evals/basics.toml` ([evals.md](evals.md)).

## `jarvis` is not found?

A built or unpacked JARVIS is a folder, and nothing puts that folder on your PATH, so a terminal says `jarvis: command not found`
(or "not recognized"). Run it once by its path: `jarvis path install` from inside the folder (for example
`.\target\release\jarvis.exe path install`). On Windows it adds the folder to your user PATH; on Linux and macOS it links `jarvis`
into `~/.local/bin`. Open a new terminal and `jarvis` works anywhere. `jarvis path status` says whether it does, and
`jarvis path uninstall` undoes it. `jarvis start` mentions this when it notices.

## Running it all the time, and on other machines

- `jarvis service install` makes JARVIS start when you log in (a systemd user unit on Linux, a launchd agent on macOS, a login entry
  on Windows) and `jarvis service uninstall` undoes it.
- On a server with no screen, `jarvis hud --print-url` prints the console address (with your credential) to open through an SSH
  tunnel. JARVIS only listens on `127.0.0.1`.
- [platforms.md](platforms.md) says what is verified on Linux and macOS and what is not, and has the headless-server walkthrough.

## Stopping and troubleshooting

- `jarvis stop` and `jarvis restart` first check ordinary active runs, queued/running coding jobs and live managed processes, including previews. Without `--force`, any such work produces a busy refusal; `--wait` retries that busy response until the work finishes. If work state cannot be read, they fail closed with an unavailable error (HTTP 503); `--wait` does not turn unknown state into permission to stop. `--force` explicitly overrides the preflight and can interrupt that work. Native approvals already waiting for your decision remain durable. The check is a snapshot, not a barrier that prevents new work from starting between the check and shutdown. Once stopping, the command waits for the daemon to finish; restart then starts it again.
- `jarvis doctor --live` checks the things that actually go wrong, each with its fix: the model server unreachable or rejecting the key,
  a granted folder gone, Docker not running for the code tool, JARVIS not started. `jarvis doctor` alone diagnoses the installation; `jarvis logs --lines 50` shows what the daemon said; `jarvis status` says whether it
  is running.
- "No credential": open the console with `jarvis hud`, not by typing the address.
- A portable, throwaway profile: add `--root DIR` (an existing folder) to any command.
