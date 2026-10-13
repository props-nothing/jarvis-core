# Settings Reference

**Choose what JARVIS knows, reaches and may do.**

[Documentation](../README.md) / [Quick start](quick-start.md) / [Google](google.md) / [Telegram](telegram.md) / [Phone](phone.md) / [Connect apps](connect-apps.md)

Everything you can change after setup, where it lives in the console (**Settings** in the header), and **why each one is off or
unset by default**. A setting that is unset is never "missing" or "not built yet": each one below is either safe to leave off,
needs something only you can supply (a key, Docker, a folder), or is deliberately opt-in so nothing is reachable by accident.

Change them in the console, or with `jarvis config set KEY VALUE...` / `jarvis keys set model|voice`. Settings that require a restart apply after
`jarvis restart` (the console's **Restart to apply** does it); live settings, including permissions and phone settings, apply immediately as marked below. Every change is checked by the same rules the daemon starts with and
refused, leaving the file untouched, if it would not start. Keys are stored in a private file and are never shown again.

## Brain

In the console, **Settings** is a side list of tabs in four groups: **JARVIS** (Brain, Voice & search, Folders & code, Permissions), **Accounts & channels** (Google, Telegram, Phone), **Connections** (Connect apps) and **System** (Alerts, Advanced). A dot beside a tab shows whether it is set up (green) or still needs setting up (amber). Each setting says what it does in one line; the rest is under **More**. On/off settings are switches that save when flipped, and the footer lists any change that still waits for **Restart to apply**. Settings that take effect at once (the phone settings, permissions) never ask for a restart. The voice and web-search keys have a **Test** button that checks the saved key with its provider straight away. If the internet address, the phone door or Telegram stops working, a banner under the console header says so and leads to the tab that fixes it.

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
| Voice key (ElevenLabs) | none | **Opt-in.** The console speaks with your browser's own voice. A key gives a natural voice; what JARVIS says is then sent to ElevenLabs to be spoken (`ADR-0138`). |
| `speech_voice_id` | George (`JBFqnCBsd6RMkjVDRZzb`) | The built-in voice. Only used once a voice key is set. |
| `speech_model` | `eleven_v4_turbo` | The built-in model. Only used once a voice key is set. `eleven_v4_turbo` and `eleven_flash_v2_5` are real-time models; `eleven_v3` sounds richer but is about 2.4 times slower, so JARVIS starts speaking noticeably later with it. |

## Folders & code

| Setting | Default | What unset means |
| --- | --- | --- |
| `tool_workspace_roots` | none | **Off by design.** With no folder granted JARVIS has no file tools at all; nothing outside a folder you chose is reachable (`ADR-0020`). Creating a file is automatic inside a granted folder, changing one asks first (`ADR-0137`). With a folder granted JARVIS can also **run a command** there (`npm run build`, `cargo test`): it asks every time and shows the full command line, runs without a shell on your machine with your privileges (not isolated), and can be set to run without asking in Permissions (`ADR-0143`). |
| `code_sandbox_image` + `code_sandbox_interpreter` | none | **Off by design.** Running code needs Docker and an image you pulled yourself (it is never pulled for you, `ADR-0128`, `ADR-0131`). They are one setting: the console saves the pair together. |
| `code_agent`, `code_agent_env`, `code_agent_auth` | none | The coding agent `jarvis.code.delegate` hands tasks to (`ADR-0207`, `ADR-0208`): a command (an argument vector, no shell; unset means an installed `copilot` or `gemini` is used if there is one), the **names** of environment variables it may inherit, and the sign-in method JARVIS uses if the agent asks. Chosen, tested and signed in on the **Coding agent** card; they apply at once, with no restart. See [code-agents.md](code-agents.md). |

## Permissions

| Setting | Default | What unset means |
| --- | --- | --- |
| Per tool: default / always ask / run without asking / off | default | Applies to the very next call, no restart (`ADR-0145`); the approval card's **Always allow** sets the same thing in one click. The tool's own rule and the risk threshold decide. Nothing is trusted unless you say so (`ADR-0133`). "Run without asking" never waives the rule that anything talking to other people asks, the workspace ceiling, or an "off". Stored as `policy.trust`, `policy.approval` and `policy.deny`. |

## Advanced

| Setting | Default | What unset means |
| --- | --- | --- |
| `notifications` | on | When a scheduled or triggered task finishes, or a new approval is waiting, and no console is open, it is shown as a desktop notification (Windows toast, macOS notification, `notify-send` on Linux). While the console is open it shows the result itself, so nothing pops up. `jarvis config set notifications off` turns it off; it applies at once. |
| `quiet_hours` | none | A daily window, such as `22:00-07:00` (24-hour, this computer's time zone, so it follows daylight saving; it may cross midnight), in which JARVIS **holds its notices**: a finished task, a waiting approval, and the Telegram cards for it. Work goes on and nothing is dropped; what was held is announced when the window ends. `jarvis config set quiet_hours 22:00-07:00`; `jarvis config unset quiet_hours` removes it. The running daemon reads the file for every notice, so it applies at once even though `config set` prints its general "apply it with: jarvis restart" hint. Unset, you are told right away. |
| `resume_interrupted` | on | After a restart or crash, a run that was working in the last two hours gets **one** continuation in its conversation, told to check what was already done before repeating it (`ADR-0155`). A continuation is never continued, and nothing is resumed that you cancelled. `off` leaves interrupted runs as failed. |
| `push_topic` | none | Set a long, unguessable [ntfy](https://ntfy.sh) topic and JARVIS also pushes to your phone when no console is open: that an approval is waiting (and for which tool), or that a scheduled task finished. **It never sends an answer, a question or an argument.** The topic name is the only secret, so make it random. Try it with `jarvis push test`. |
| `push_server` | `https://ntfy.sh` | The ntfy server. Use your own (`https://` only) to keep even the metadata off a third party. |
| `http_port` | 8765 | The default port. |
| `mcp_serve_port` | none | **Off by design.** Switch it on from the **Connect apps** tab (or `jarvis mcp access on`); it then listens on 127.0.0.1 only, and only a program holding your key can use it (`ADR-0170`). |

## Not settings yet (and why)

These are the honest gaps, in roadmap order. None is hidden behind an unset field: there is simply nothing to configure until the
feature exists.

- **Connectors (Gmail, Calendar)**: the contract and a 38k-line library exist, but nothing in the daemon calls them yet
  (`ROADMAP.md`, order of work item 1). Their sign-in and scopes will get a tab when they do.
- **The memory and project tools** (`project.create`, `project.update`, `project.assign_schedule`, `memory.correct`, `memory.forget`) ask by default and **can** be set to "Run without asking" here, one at a time (`ADR-0177`); the approval card never offers a one-click "Always allow" for them.
- **"Run without asking" is honoured for tools from programs you added** (the browser, documentation lookup, your own), one tool at a time. It is honoured for **every** tool, including the ones that send mail or add calendar events (`ADR-0178`): choosing it for a tool that reaches other people asks you to confirm once, and any tool can be set back to "Always ask first".
- **Browsing and documentation lookup** come with JARVIS and are listed under **Permissions** like every other tool ([abilities.md](abilities.md)). Your own tool programs (for developers) are under Advanced: [mcp.md](mcp.md).
- **Let other apps use JARVIS (port)** in Advanced is the reverse: it lets *another* program use JARVIS's tools. It is now the **Connect apps** tab; it is off until you switch it on ([connect-apps.md](connect-apps.md)).
- **Local (non-cloud) speech recognition and synthesis, telephony, an always-on wake word**: Phase 8.
- **Coding-agent runtimes, and routing sub-agents to a cheaper model**: Phase 7 and the order-of-work list.
- **Scheduling, memory and skills** have their own commands and panels, not settings: `jarvis schedule`, `jarvis memory`,
  `jarvis skills`.