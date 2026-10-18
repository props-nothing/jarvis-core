# Settings Reference

**Choose what JARVIS knows, reaches and may do.**

[Documentation](../README.md) / [Quick start](quick-start.md) / [Google](google.md) / [Telegram](telegram.md) / [Phone](phone.md) / [External access](connect-apps.md)

Everything you can change after setup, where it lives in the console (**Settings** in the header), and **why each one is off or
unset by default**. A setting that is unset is never "missing" or "not built yet": each one below is either safe to leave off,
needs something only you can supply (a key, Docker, a folder), or is deliberately opt-in so nothing is reachable by accident.

Change them in the console, or with `jarvis config set KEY VALUE...` / `jarvis keys set model|voice`. Settings that require a restart apply after
`jarvis restart` (the console's **Restart to apply** does it); live settings, including permissions and phone settings, apply immediately as marked below. Every change is checked by the same rules the daemon starts with and
refused, leaving the file untouched, if it would not start. Keys are stored in a private file and are never shown again.

## Finding settings

The main HUD gives the face most of the desktop, with a compact conversation panel alongside it. On phones, a compact face sits above the conversation; short viewports reserve the space for messages and controls. Approvals stay visible in the main view. **History** reveals saved conversations, retention information and **Clear history**. Storage failures remain visible even with History closed. Older unscoped browser history stays in storage without being assigned to the current profile or showing a migration banner.

Type into the single-line **Message** input and press Enter to send. **Voice options**, beside New chat in the conversation header, contains **Speak answers** and **Wake word “Jarvis”**. Escape closes the options and returns focus to their control; clicking outside also closes them. Voice errors remain visible beneath the input. Idle readiness messages and the large activity footer have been removed.

**Settings** opens on **Overview**. **Apps & tools** lists services JARVIS can use: GitHub, Google, voice/search, files/coding, built-in tools and custom MCP servers. Each row opens its setup and shows saved configuration status, not a live health check. GitHub and Google have their own details; custom servers live under **Custom tools (MCP)**. **Channels** contains Telegram and Phone. **External access** is the reverse direction: other apps calling JARVIS. Ports and uncommon system settings remain in Advanced. Close settings with the upper-right close button or Escape.

Search matches section descriptions, setting names/help and integration terms such as MCP. On phones, choose a section from the native picker; searching also reveals matching navigation buttons. Desktop tabs support arrows and Home/End. Escape closes the dialog and returns focus to its opener; in a model picker it first closes the list.

Settings use the same field, label, help and action styles in every section. Labels sit beside fields on desktop and above them on phones. Fixed choices use native dropdowns. Model actions sit below the provider fields, and providers that need no key do not show an unused key field. Read-only connection snippets keep monospace text for copying.

Unsaved ordinary fields and model drafts stay in memory across section changes and unrelated saves. **Discard drafts** resets them to saved values. Reloading discards unsaved drafts. Appearance preferences apply immediately and do not count as unsaved drafts. Password fields are cleared on submission, section changes, close and redraw; they are never stored in browser preferences.

The daemon compares saved settings and credential fingerprints with its startup snapshot. The restart footer therefore survives page reloads and sees CLI changes. Reverting a setting removes its pending change. If the comparison fails, the footer reports unknown status. The comparison describes saved changes, not provider health; existing environment overrides and live-service semantics still apply. A restart request is not proof that the new process started; reopen settings after reconnection to verify the footer.

Tab dots describe saved configuration, **not a live connection test**. Their accessible labels and tooltips distinguish configured, setup needed and unknown. Each setting says what it does in one line; the rest is under **More**. On/off settings save when flipped, and the footer lists changes waiting for **Restart to apply**. Live policy changes and the phone policy switches are excluded from the startup comparison; other phone setup can require service initialization. Voice and web-search keys have a **Test** button that checks the saved key with its provider. A failing internet address, phone door or Telegram shows a banner under the console header leading to its settings.

## Console appearance

**Settings → Console** controls the look of this browser only:

- **Accent:** Mint matches the public site; Ice and Amethyst offer cooler and violet palettes. Approval, listening and offline colors remain meaningful and do not change with the accent.
- **Atmosphere:** Cinematic adds intersecting orbital rings, segmented arcs, markers, particles and soft illumination. Quiet keeps a minimal presence without those decorative effects.
- **Density:** Comfortable gives controls more room; Compact fits more messages, cards and settings on screen.
- **Motion:** Use system preference respects the device's reduced-motion setting; Reduce motion keeps the orbit stationary and disables interface transitions and preview animation.

The live preview illustrates your choices without contacting any provider. **Reset appearance** restores only the four local appearance preferences; it does not remove conversations, keys or daemon settings. Existing saved density/motion preferences are kept when the new choices are first introduced.

Changes apply immediately and are saved in this browser's local storage, with no daemon restart. Console preferences remain available while the daemon is offline. If storage is blocked, the change applies only to the current page and the console says so. These controls do not change models, permissions or provider settings.

Deep blue surfaces and mint accents frame the face. Amber still means a decision needs you; offline, working, listening and speaking remain distinct, named states. The header shows run/decision counters only when there is activity; clicking either opens **Operations → Activity**. When disconnected, any retained counters report unavailable status rather than presenting stale counts as live. **Talk to JARVIS** sits beneath the desktop face, and the microphone beside Message is available on every layout. **Restart to apply** is unavailable unless there are pending changes and saved configuration is available. Starting-point suggestions prepare an editable message rather than starting a run. Sending is unavailable while offline or while the current answer is running, but a draft can still be edited and is not cleared by an unavailable submission.

## Connections

**Settings → Apps & tools → GitHub** uses persistent browser sign-in rather than a PAT or server command. Expand **One-time JARVIS app setup**, register the OAuth app with the callback URL shown, save its client ID/write-only secret, then press **Connect GitHub**. Private repository access is opt-in; the initial remote tools remain read-only. **Test GitHub tools** is a separate live connection check and **Disconnect GitHub** removes local authorization even if provider revocation fails. The app settings stay for the next sign-in. A first tool registration still needs the explicit restart footer; connecting never silently restarts the daemon. See [setup, privacy and verification limits](connections.md).

For a compatible remote service with browser sign-in, **Apps & tools → Custom tools (MCP) → Remote authorization → Browser OAuth** offers one-time app setup and Connect/Check sign-in/Test tools/Disconnect. Register the provider app with the displayed callback, enter its expected issuer, client ID/optional write-only secret and minimal scopes, then connect. A saved grant is not a verified account name or live health check; automatic registration is not included. See [generic OAuth setup and limits](mcp.md#a-remote-server-with-browser-sign-in).

For another remote MCP server that accepts a bearer token, **Apps & tools → Custom tools (MCP)** has an optional **Bearer key reference** field for HTTP(S). Set the value privately with `jarvis keys set mcp:NAME:KEY`, then enter only that same-server reference in Settings. Test lists tools; status reports set/not set, not a token or provider health. Rotation/removal applies to the next outbound request; registration changes still need restart. See [remote token setup and limits](mcp.md#a-remote-server-that-needs-a-token).

## Brain

You can always type a model name. **Load models** requests suggestions from the selected provider and enables the list arrow. Arrow Down selects the first suggestion; Arrow Up selects the last. Enter accepts the highlighted model, while Escape closes the list without changing the name. Tab or clicking outside dismisses it. Selecting a suggestion keeps it as an unsaved draft across section changes; it does not save or test the provider.

**Settings, Brain** has one card for the **main model** and one for the **fallback**. Pick a provider (Ollama on this computer, Ollama Cloud, OpenAI, Google Gemini, Anthropic, or any other OpenAI-compatible server), paste its key, press **Load models** to choose from what that provider actually offers (you can still type a name), press **Test** to see that the model answers, then **Save** and restart. JARVIS asks the provider itself, so a key you saved is never sent to the page, and a saved key is only ever used with the address it was saved for: changing the address means pasting that provider's key. Test checks that the model answers, not that it uses tools well. Anthropic and Gemini are reached through their OpenAI-compatible endpoints; Anthropic describes its as a way to try Claude rather than a production route (no prompt caching, thinking not returned). Under the cards, **Thinking effort** is a list, and a cheaper model for sub-agents and scheduled runs can be set. The web-search key is in **Voice & search**, beside the ElevenLabs key (which the Phone tab also uses). The table below is the same settings by name, for `jarvis config`.

| Setting | Default | What unset means |
| --- | --- | --- |
| `executor_reasoning_effort` | unset | The model decides how hard to think. `none`, `low`, `medium` or `high` makes a reasoning model faster and cheaper (`low`) or more thorough (`high`). A run that thinks past its budget is still dropped to `low` for the rest of that run. Needs the live model. |
| `executor_fallback_model_name` | unset | A second model to ask, at the same provider, when the main one is rate limited or overloaded (for example `jarvis config set executor_fallback_model_name glm-5.3:cloud`). It is used for that one call only; the next call tries the main model again. Unset, a rate-limited call waits (up to about five minutes) and then fails. Other refusals (a bad key, a model that does not exist, a context that is too large) are never worked around. |
| `executor_fallback_base_url` | unset | A **different provider** for the fallback model: its address with its `/v1` (for example `https://api.openai.com/v1`). Set the model name first, then this, then its key with `jarvis keys set fallback` (or the "Fallback key" box in Settings); check it with `jarvis keys test fallback`. With the address and key, the fallback model is asked there when the main provider is rate limited, overloaded, down, or out of credit, so a limit or outage at the main provider does not stop a run. A bad key, a missing model, a too-large context or a content refusal still stay as errors and send nothing elsewhere. **What the run is saying is sent to that provider**, so choose one you are happy with. Without the key, the address is ignored and the fallback model is asked at the main provider. |
| `executor_fallback_api_key_ref` | unset | The private file holding that provider's key; written by `jarvis keys set fallback`, never typed here. |
| `executor_subagent_model_name` | unset | The model **sub-agents** use, at the main provider (for example a cheaper one: `jarvis config set executor_subagent_model_name glm-5.3-flash`). Unset, they use the main model. Tools, approvals and caps are the same; only the model differs. Applies after a restart. `jarvis digest` shows tokens per model. |
| `executor_background_model_name` | unset | The model **scheduled tasks and runs started by a mail, file or webhook trigger** use, at the main provider. What you ask directly keeps the main model. Unset, they use the main model. Applies after a restart. |
| `workspace_daily_token_limit` | `0` (unlimited) | The most input plus output tokens all runs may use in a rolling 24 hours. After the billable model call that reaches the cap, the run stops before another model or tool call. Set **Daily token limit** in Settings, Brain, or use `jarvis config set workspace_daily_token_limit 250000`, then restart. Project-specific caps remain available in each project. |
| `telegram_token_ref` (the **Telegram** tab) | none | **Off by design.** A bot token from @BotFather; saved write-only, no restart, then a code pairs your phone (`docs/user/telegram.md`). |
| `phone_agent_id`, `phone_number_id`, `phone_owner_numbers` (the **Phone** tab) | none | **Off by design.** An ElevenLabs agent, the phone number it answers, and the numbers (international format, comma separated) JARVIS may ring. All three, and the voice key, make JARVIS able to call you; no restart (`docs/user/phone.md`, `ADR-0200`). |
| `phone_approves` | `off` | Off: a tool the phone agent asks for still waits for your click, even if the agent asked you on the phone. On: your yes on a call JARVIS placed to you counts, for tools of risk 2 or less, while the call is answered. |
| `phone_opening_line` | `off` | Off: the agent opens with its own first message. On: JARVIS writes it ("Hello, this is JARVIS. I am calling because: ..."), unless the assistant gave the exact words to say; needs the agent's *First message* override enabled at ElevenLabs. Calls to other people always open by saying an AI is calling, whatever this says. |
| `phone_call_anyone` | `off` | Off: JARVIS rings only your listed numbers. On: it also gets `jarvis.voice.call_number` for any other number, asked of you every time, never in quiet hours, always opening as an AI calling for you, at most 5 a day (2 per number), no premium-rate numbers (`ADR-0201`). |
| `phone_screen_callers` | `off` | Off: anyone can ring the number and talk to the agent (JARVIS still refuses every tool while a stranger is on the line). On: ElevenLabs asks JARVIS about each inbound call first and a call from a number that is not listed never starts. JARVIS registers this by itself and follows its internet address (needs *Reachable from the internet*). While JARVIS is off nobody gets an answer. |
| `phone_calls_per_hour`, `phone_calls_per_day` | `3`, `10` | The most calls JARVIS places an hour and a day (1 to 20 and 1 to 100; the hourly limit cannot pass the daily one). A guard against a mistaken or manipulated model ringing a phone repeatedly; applies at once. Calls to other people keep their own lower limits (`ADR-0205`). |
| `google_client_id` + client secret | none | **Off by design.** Needs an OAuth client you create in your own Google Cloud project (`docs/user/google.md`); without it there is no Google sign-in and no mail or calendar tools. Nothing is shared with Google until you press Sign in (Settings, Google). |
| `search_api_key_ref` + search key | none | **Off by design.** Without an Ollama API key (`jarvis keys set search`, from a free ollama.com account) there is no web search tool; JARVIS can still fetch a page you name. `jarvis keys test search` checks the key against the real service (`ADR-0146`). |
| `executor_model_name`, `executor_base_url`, model key | set by `jarvis init` | No model, so no answers. Setup always sets these. |

## Voice

| Setting | Default | What unset means |
| --- | --- | --- |
| Voice key (ElevenLabs) | required by new CLI setup | New setup tests a key with TTS access before saving. Existing/headless profiles can omit it, and removing it later restores browser voice. What JARVIS says is sent to ElevenLabs to be spoken (`ADR-0138`, `ADR-0214`). |
| `speech_voice_id` | `liN4AsrhF3CylqMDWK6p` | Owner-selected default voice ID, **not an API key**. The account must have access to it; choose another ID if its setup test fails. Explicit saved IDs are preserved. |
| `speech_model` | `eleven_v4_turbo` | The built-in model. Only used once a voice key is set. `eleven_v4_turbo` and `eleven_flash_v2_5` are real-time models; `eleven_v3` sounds richer but is about 2.4 times slower, so JARVIS starts speaking noticeably later with it. |

## Folders & code

| Setting | Default | What unset means |
| --- | --- | --- |
| `tool_workspace_roots` | none | **Off by design.** JARVIS's own file tools require a granted folder and confine their file access to those roots (`ADR-0020`). Creating a file runs under default policy; changing one asks by default (`ADR-0137`). Approved command tools, ACP agents and managed processes run as your user on the host: selecting a working directory does not confine their filesystem or network access. Commands show their arguments and ask by default; explicit per-tool standing trust can change the question (`ADR-0143`, `ADR-0222`). |
| `code_sandbox_image` + `code_sandbox_interpreter` | none | **Off by design.** Running code needs Docker and an image you pulled yourself (it is never pulled for you, `ADR-0128`, `ADR-0131`). They are one setting: the console saves the pair together. |
| `code_agent`, `code_agent_env`, `code_agent_auth` | none | The coding agent `jarvis.code.delegate` hands tasks to (`ADR-0230`, `ADR-0208`): a command (an argument vector, no shell; unset means an installed `copilot` or `gemini` is used if there is one), optional inherited environment-variable **names** for advanced CLI configuration, and the sign-in method JARVIS uses if the agent asks. The **Coding agent** card offers agent selection, Test and normal sign-in, with no environment-variable editor. Existing `code_agent_env` configurations remain supported. Changes apply at once, with no restart. See [code-agents.md](code-agents.md). |

## Permissions

| Setting | Default | What unset means |
| --- | --- | --- |
| Per tool: default / always ask / run without asking / off | default | Applies to the very next call, no restart (`ADR-0145`); where offered, the approval card's **Always allow** saves the same standing trust. Outward communication asks by default, but explicit **Run without asking** can waive that question for the named tool (`ADR-0178`). Choosing this for an outward tool in Permissions asks for confirmation once. Denial, required scopes, the workspace risk ceiling and an explicit **Always ask** still apply. Stored as `policy.trust`, `policy.approval` and `policy.deny`. |

## Advanced

| Setting | Default | What unset means |
| --- | --- | --- |
| `notifications` | on | When a scheduled or triggered task finishes, or a new approval is waiting, and no console is open, it is shown as a desktop notification (Windows toast, macOS notification, `notify-send` on Linux). While the console is open it shows the result itself, so nothing pops up. `jarvis config set notifications off` turns it off; it applies at once. |
| `quiet_hours` | none | A daily window, such as `22:00-07:00` (24-hour, this computer's time zone, so it follows daylight saving; it may cross midnight), in which JARVIS **holds its notices**: a finished task, a waiting approval, and the Telegram cards for it. Work goes on and nothing is dropped; what was held is announced when the window ends. `jarvis config set quiet_hours 22:00-07:00`; `jarvis config unset quiet_hours` removes it. The running daemon reads the file for every notice, so it applies at once even though `config set` prints its general "apply it with: jarvis restart" hint. Unset, you are told right away. |
| `resume_interrupted` | on | After a restart or crash, a run that was working in the last two hours gets **one** continuation in its conversation, told to check what was already done before repeating it (`ADR-0155`). A continuation is never continued, and nothing is resumed that you cancelled. `off` leaves interrupted runs as failed. |
| `push_topic` | none | Set a long, unguessable [ntfy](https://ntfy.sh) topic and JARVIS also pushes to your phone when no console is open: that an approval is waiting (and for which tool), or that a scheduled task finished. **It never sends an answer, a question or an argument.** The topic name is the only secret, so make it random. Try it with `jarvis push test`. |
| `push_server` | `https://ntfy.sh` | The ntfy server. Use your own (`https://` only) to keep even the metadata off a third party. |
| `http_port` | 8765 | The default port. |
| `mcp_serve_port` | none | **Off by design.** Switch it on from the **External access** tab (or `jarvis mcp access on`); it then listens on 127.0.0.1 only, and only a program holding your key can use it (`ADR-0170`). |

## Not settings yet (and why)

These are the honest gaps, in roadmap order. None is hidden behind an unset field: there is simply nothing to configure until the
feature exists.

- **Connectors (Gmail, Calendar)**: the contract and a 38k-line library exist, but nothing in the daemon calls them yet
  (`ROADMAP.md`, order of work item 1). Their sign-in and scopes will get a tab when they do.
- **The memory and project tools** (`project.create`, `project.update`, `project.assign_schedule`, `memory.correct`, `memory.forget`) ask by default and **can** be set to "Run without asking" here, one at a time (`ADR-0177`); the approval card never offers a one-click "Always allow" for them.
- **"Run without asking" is honoured for tools from programs you added** (the browser, documentation lookup, your own), one tool at a time. It is honoured for **every** tool, including the ones that send mail or add calendar events (`ADR-0178`): choosing it for a tool that reaches other people asks you to confirm once, and any tool can be set back to "Always ask first".
- **Browsing and documentation lookup** come with JARVIS and are listed under **Permissions** like every other tool ([abilities.md](abilities.md)). Your own tool programs (for developers) are under Advanced: [mcp.md](mcp.md). Local programs can receive explicitly mapped named credentials provisioned with `jarvis keys set mcp:SERVER:KEY`; their cards show reference and set/not set, never the value ([setup and limits](mcp.md#a-local-server-that-needs-a-key)).
- **Let other apps use JARVIS (port)** in Advanced is the reverse: it lets *another* program use JARVIS's tools. It is now the **External access** tab; it is off until you switch it on ([connect-apps.md](connect-apps.md)).
- **Voice settings**: Voice & search has a voice library and exact draft preview. **Create a voice** generates a local voice from a written description, with progress, errors and playback beside Generate. **Save and use in console** saves and selects it. **Default voice and backups**, **Provider and model settings** and **Manage this voice** hold the less frequent controls. **More ways to add a voice → Mix presets** separately offers Heart/George percentages, pronunciation and speed. Both designs can be exported and deleted. Missing local setup disables Generate with repair instructions. Normal builds include embedded Kokoro. Telegram audio, a provider-neutral phone voice, local recognition and an always-on wake word remain separate Phase 8 work. See [speech settings](speech.md#pick-edit-and-preview-voices) and [local design](speech.md#design-a-local-voice).
- **Coding-agent runtimes, and routing sub-agents to a cheaper model**: Phase 7 and the order-of-work list.
- **Scheduling, memory and skills** have their own commands and panels, not settings: `jarvis schedule`, `jarvis memory`,
  `jarvis skills`.


## Provider saves and activity

**Brain → Save** commits the model, address and replacement key together. Rotated model keys use new private files; old files remain for existing readers. A changed address cannot silently reuse a key belonging to the previous provider. **Save & test** commits first and then performs the explicit provider check; a failed check is reported separately from a successful save. No provider test occurs just by opening Settings. Load models remains optional; a key submitted only for that probe is cleared and must be entered again to save.

Operations has **Activity** (approvals, runs, projects, coding tasks and recent work), **Automations** (schedules and triggers), **Knowledge** (contacts and the memory inspector), and **Tools & diagnostics**. Coding results have moved from agent configuration to Activity; Folders & code links there. On phones, the conversation and composer occupy the screen, with pending decisions above them. Desktop conversation gets most of the available width. Hidden/reduced-motion presence rendering is throttled; polling is bounded, backs off on failures and reports stale optional panels without disabling conversation.

## Browser history

History is scoped to the daemon profile, with up to 30 chats and the latest 60 messages per chat. It is a browser cache, not the complete durable daemon transcript. **Clear history** clears this profile's browser cache and leaves daemon records intact. Storage failures are visible. If profile identity cannot be loaded, the page runs without persistent history until reloaded after reconnection. Older unscoped browser history is retained in storage but is not automatically attached to an arbitrary profile.

## UI implementation

The integration catalog uses pinned React and TypeScript, built with Vite and embedded in the Rust binary. Installed users need no Node runtime. Legacy voice, approval, settings and canvas adapters are retained for incremental migration. See [the frontend developer guide](../../apps/jarvisd/ui/README.md) and [ADR-0225](../adr/0225-hud-controls-use-explicit-state-and-task-navigation.md).
