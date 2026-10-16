//! The settings a person may change after setup, and the keys behind them: one code path for `jarvis config`,
//! `jarvis keys` and the console's Settings screen (`P9-014`, `P9-015`).
//!
//! Every change is made on the stored document, checked by the **same parser the daemon starts with**, and saved the
//! way `init` saves it, so a setting cannot be valid in one surface and refused at startup. A key is written to a
//! private file and the configuration holds only its path; no function here returns, logs or errors with a key's value.

use std::path::{Path, PathBuf};

use toml::{Table, Value};

use crate::{AppPaths, Config, ConfigStore, PathKind, secure_private_file};

/// How a setting's value is read.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Kind {
    Text,
    File,
    Folders,
    Words,
    Port,
    Count,
    ToolIds,
    Flag,
    Numbers,
}

impl Kind {
    const fn name(self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::File => "file",
            Self::Folders => "folders",
            Self::Words => "words",
            Self::Port => "port",
            Self::Count => "count",
            Self::ToolIds => "tools",
            Self::Flag => "flag",
            Self::Numbers => "numbers",
        }
    }
}

struct Setting {
    table: &'static str,
    field: &'static str,
    kind: Kind,
    help: &'static str,
    /// Which tab of the settings screen it lives on.
    group: &'static str,
    /// The value in force when it is not set, when there is one.
    default: Option<&'static str>,
    /// What it means that it is not set: why it is off by default.
    unset_means: &'static str,
    /// A sample value, shown as a hint.
    example: &'static str,
}

const SETTINGS: &[Setting] = &[
    Setting {
        table: "daemon",
        field: "executor_model_name",
        kind: Kind::Text,
        help: "the model to talk to",
        group: "brain",
        default: None,
        unset_means: "No model, so the assistant cannot answer. Setup always sets one.",
        example: "glm-5.3:cloud",
    },
    Setting {
        table: "daemon",
        field: "executor_base_url",
        kind: Kind::Text,
        help: "the model server address, with its /v1",
        group: "brain",
        default: None,
        unset_means: "No model server. Setup always sets one.",
        example: "http://localhost:11434/v1",
    },
    Setting {
        table: "daemon",
        field: "executor_api_key_ref",
        kind: Kind::File,
        help: "file holding the model key (set the key itself with `jarvis keys set model`)",
        group: "brain",
        default: None,
        unset_means: "No key file. Setup writes one (a placeholder for local Ollama).",
        example: "",
    },
    Setting {
        table: "daemon",
        field: "executor_fallback_model_name",
        kind: Kind::Text,
        help: "a second model to ask when the main one is rate limited or overloaded",
        group: "brain",
        default: None,
        unset_means: "No fallback: a run waits out a rate limit (up to about five minutes) and then fails. With one, the same provider is asked for this model at once (or the provider below, when one is set), for that call only; the next call tries the main model again.",
        example: "glm-5.3:cloud",
    },
    Setting {
        table: "daemon",
        field: "executor_subagent_model_name",
        kind: Kind::Text,
        help: "the model sub-agents use, at the main provider (for example a cheaper one)",
        group: "brain",
        default: None,
        unset_means: "Sub-agents use the main model. With one set, a run started by jarvis.agent.delegate is asked this model, and its token use is recorded under it.",
        example: "glm-5.3-flash",
    },
    Setting {
        table: "daemon",
        field: "executor_background_model_name",
        kind: Kind::Text,
        help: "the model scheduled and trigger-started runs use, at the main provider",
        group: "brain",
        default: None,
        unset_means: "Scheduled tasks and runs started by a mail, file or webhook trigger use the main model. With one set, they are asked this model instead; what you ask directly still uses the main one.",
        example: "glm-5.3-flash",
    },
    Setting {
        table: "daemon",
        field: "executor_fallback_base_url",
        kind: Kind::Text,
        help: "a different provider for the fallback model: its address, with its /v1 (then set its key below)",
        group: "brain",
        default: None,
        unset_means: "The fallback model is asked at the main provider. With an address and a key, it is asked there instead, so a limit or an outage at the main provider does not stop a run. What the run is saying is then sent to that provider.",
        example: "https://api.openai.com/v1",
    },
    Setting {
        table: "daemon",
        field: "executor_fallback_api_key_ref",
        kind: Kind::File,
        help: "file holding the fallback provider key (set the key itself with `jarvis keys set fallback`)",
        group: "brain",
        default: None,
        unset_means: "No key for a different fallback provider.",
        example: "",
    },
    Setting {
        table: "daemon",
        field: "executor_reasoning_effort",
        kind: Kind::Text,
        help: "how hard a reasoning model thinks: none, low, medium or high",
        group: "brain",
        default: None,
        unset_means: "The model decides how long to think. Lower is faster and cheaper; a run that thinks past its budget is already asked to think less.",
        example: "low",
    },
    Setting {
        table: "daemon",
        field: "quiet_hours",
        kind: Kind::Text,
        help: "a daily window, such as 22:00-07:00, in which JARVIS holds its notices (desktop, push, Telegram) until it ends",
        group: "advanced",
        default: None,
        unset_means: "No quiet hours: a finished task or a waiting approval is announced at once. With a window, work still goes on; only the telling waits, and everything held is announced when the window ends. Read in this machine's time zone.",
        example: "22:00-07:00",
    },
    Setting {
        table: "daemon",
        field: "workspace_daily_token_limit",
        kind: Kind::Count,
        help: "the most input plus output tokens all runs may use in a rolling 24 hours; 0 means unlimited",
        group: "brain",
        default: Some("0"),
        unset_means: "Unlimited. Set a positive count to stop runs after the billable call that reaches the cap.",
        example: "250000",
    },
    Setting {
        table: "daemon",
        field: "google_client_id",
        kind: Kind::Text,
        help: "the Google OAuth client id (a Desktop app client you create in Google Cloud Console; see docs/user/google.md)",
        group: "google",
        default: None,
        unset_means: "Opt-in: without your own Google OAuth client there is no Google sign-in, so no mail or calendar tools. Nothing is shared with Google until you sign in.",
        example: "1234-abc.apps.googleusercontent.com",
    },
    Setting {
        table: "daemon",
        field: "google_actions",
        kind: Kind::Flag,
        help: "allow email sends, drafts, moving mail to Trash and calendar changes: on or off (sign in again for mailbox-change permission; deletion asks by default unless you explicitly trust the tool)",
        group: "google",
        default: Some("off"),
        unset_means: "Off: signing in asks only for read permissions, so JARVIS can read mail and calendar but never send or change anything.",
        example: "on",
    },
    Setting {
        table: "daemon",
        field: "google_client_secret_ref",
        kind: Kind::File,
        help: "file holding the Google client secret (set the secret itself with `jarvis keys set google`)",
        group: "google",
        default: None,
        unset_means: "Needed with the client id: Google issues a secret to Desktop app clients and exchanging the sign-in code requires it.",
        example: "",
    },
    Setting {
        table: "daemon",
        field: "search_api_key_ref",
        kind: Kind::File,
        help: "file holding the web search key (set the key itself with `jarvis keys set search`)",
        group: "brain",
        default: None,
        unset_means: "Opt-in: without an Ollama API key (a free ollama.com account makes one) the assistant has no web search tool; it can still fetch a page you give it.",
        example: "",
    },
    Setting {
        table: "daemon",
        field: "telegram_token_ref",
        kind: Kind::File,
        help: "file holding the Telegram bot token (set the token itself with `jarvis keys set telegram`)",
        group: "telegram",
        default: None,
        unset_means: "Off: JARVIS has no Telegram chat. Create a bot with @BotFather, paste its token here, then send the bot a message and enter the code it replies with.",
        example: "",
    },
    Setting {
        table: "daemon",
        field: "tool_workspace_roots",
        kind: Kind::Folders,
        help: "folders the assistant may read and write (replaces the list)",
        group: "files",
        default: None,
        unset_means: "Off by design: with no folder granted the assistant has no file tools at all, so nothing outside a folder you chose is ever reachable.",
        example: "C:/Users/me/notes; C:/Users/me/projects",
    },
    Setting {
        table: "daemon",
        field: "code_sandbox_image",
        kind: Kind::Text,
        help: "container image the code tool runs in",
        group: "files",
        default: None,
        unset_means: "Off by design: running code needs Docker and an image you pulled yourself (it is never pulled for you), so there is no code tool until you name one.",
        example: "node:22-alpine",
    },
    Setting {
        table: "daemon",
        field: "code_sandbox_interpreter",
        kind: Kind::Words,
        help: "command inside it, for example: node -e",
        group: "files",
        default: None,
        unset_means: "Needed together with the image above; off while there is no image.",
        example: "node -e",
    },
    Setting {
        table: "daemon",
        field: "code_agent",
        kind: Kind::Words,
        help: "command that starts the coding agent for `jarvis.code.delegate` (an agent that speaks ACP)",
        group: "files",
        default: None,
        unset_means: "JARVIS looks for an installed `copilot` or `gemini` command. Needs a folder granted, git, and the agent signed in once; the owner is asked before every run.",
        example: "npx -y @github/copilot --acp",
    },
    Setting {
        table: "daemon",
        field: "code_agent_env",
        kind: Kind::Words,
        help: "names of environment variables the coding agent may inherit (names only, never values)",
        group: "files",
        default: None,
        unset_means: "The agent gets only the small safe set of variables (path, home, temp), so it relies on its own sign-in; add a name such as OPENAI_API_KEY or GH_TOKEN only when it needs one.",
        example: "OPENAI_API_KEY",
    },
    Setting {
        table: "daemon",
        field: "code_agent_auth",
        kind: Kind::Text,
        help: "the agent's own sign-in method JARVIS uses when the agent asks for a sign-in",
        group: "files",
        default: None,
        unset_means: "JARVIS never signs in for the agent. Sign in once with the Sign in button on the coding agent card (or in a terminal), and the agent keeps its own credentials.",
        example: "chatgpt",
    },
    Setting {
        table: "daemon",
        field: "speech_api_key_ref",
        kind: Kind::File,
        help: "file holding the voice key (set the key itself with `jarvis keys set voice`)",
        group: "voice",
        default: None,
        unset_means: "Opt-in: without an ElevenLabs key the console speaks with your browser's own voice.",
        example: "",
    },
    Setting {
        table: "daemon",
        field: "speech_voice_id",
        kind: Kind::Text,
        help: "ElevenLabs voice id",
        group: "voice",
        default: Some("liN4AsrhF3CylqMDWK6p"),
        unset_means: "Uses the owner's preferred default voice. Only used once a voice key is set; your ElevenLabs account must have access to this voice.",
        example: "liN4AsrhF3CylqMDWK6p",
    },
    Setting {
        table: "daemon",
        field: "speech_model",
        kind: Kind::Text,
        help: "ElevenLabs model id",
        group: "voice",
        default: Some("eleven_v4_turbo"),
        unset_means: "Uses the built-in default model. Only used once a voice key is set.",
        example: "eleven_flash_v2_5",
    },
    Setting {
        table: "daemon",
        field: "phone_agent_id",
        kind: Kind::Text,
        help: "the ElevenLabs agent that speaks on the phone (its id, from the agent's page at elevenlabs.io)",
        group: "phone",
        default: None,
        unset_means: "No phone calls. The agent is the voice and the ears of a call; it uses JARVIS's tools through Connect apps, so what it may do is decided here.",
        example: "agent_0123456789abcdef",
    },
    Setting {
        table: "daemon",
        field: "phone_number_id",
        kind: Kind::Text,
        help: "the ElevenLabs phone number calls are placed from (its id, shown below once the voice key is set)",
        group: "phone",
        default: None,
        unset_means: "No phone calls. Import a Twilio number or a SIP trunk into ElevenLabs first.",
        example: "phnum_0123456789abcdef",
    },
    Setting {
        table: "daemon",
        field: "phone_owner_numbers",
        kind: Kind::Numbers,
        help: "the numbers JARVIS may call, in international format, separated by commas",
        group: "phone",
        default: None,
        unset_means: "No phone calls: JARVIS rings only a number you listed here, and a model can never name another.",
        example: "+31612345678",
    },
    Setting {
        table: "daemon",
        field: "phone_approves",
        kind: Kind::Flag,
        help: "while a call JARVIS placed to you is open, count your yes on the phone as the approval of a low-risk tool: on or off",
        group: "phone",
        default: Some("off"),
        unset_means: "Off: a tool the phone agent asks for still waits for your click in the console (or Telegram), even if the agent already asked you on the phone. On: only while a call JARVIS placed to your number is open, only for tools of risk 2 or less, and only if the agent is set to ask you first for each acting tool.",
        example: "on",
    },
    Setting {
        table: "daemon",
        field: "phone_opening_line",
        kind: Kind::Flag,
        help: "let JARVIS write the agent's first sentence for a call to you (who is calling and why) when the assistant gave no words of its own: on or off",
        group: "phone",
        default: Some("off"),
        unset_means: "Off: the agent opens with its own first message and gets the reason as the variable jarvis_reason. On: JARVIS opens with \"Hello, this is JARVIS. I am calling because: ...\", unless the assistant gave the exact words to say. Needs the agent's \"first message\" override switched on in its Security tab at ElevenLabs, or the provider refuses the call. Calls to other people always open by saying that an AI is calling, whatever this says.",
        example: "on",
    },
    Setting {
        table: "daemon",
        field: "phone_call_anyone",
        kind: Kind::Flag,
        help: "let JARVIS ring numbers that are not yours (a restaurant, a plumber), each call asked of you first: on or off",
        group: "phone",
        default: Some("off"),
        unset_means: "Off: JARVIS rings only the numbers listed above. On: it gets a second tool, jarvis.voice.call_number, for any number. Each call asks you (trust it by name only if you accept that a hostile page could steer it), is never placed in quiet hours, always opens by saying it is an AI calling for you and why, is capped at 5 a day and 2 a day per number, and never reaches premium-rate numbers. While such a call is open, other apps cannot use JARVIS's tools.",
        example: "on",
    },
    Setting {
        table: "daemon",
        field: "phone_screen_callers",
        kind: Kind::Flag,
        help: "answer calls to your phone number only from your own numbers; everyone else gets no answer: on or off",
        group: "phone",
        default: Some("off"),
        unset_means: "Off: anyone can ring the number and talk to the agent (JARVIS still refuses every tool while a stranger is on the line). On: ElevenLabs asks JARVIS about each inbound call before the agent answers, and JARVIS says yes only to the numbers listed above, so a stranger's call never starts. Needs 'Reachable from the internet' in Connect apps: JARVIS keeps ElevenLabs pointed at its current address by itself. While JARVIS is off or the address is down, nobody gets an answer. Caller ID can be forged.",
        example: "on",
    },
    Setting {
        table: "daemon",
        field: "phone_calls_per_hour",
        kind: Kind::Count,
        help: "the most calls JARVIS may place in one hour, from 1 to 20",
        group: "phone",
        default: Some("3"),
        unset_means: "Uses the default of 3 calls an hour. The limit stops a mistaken or manipulated model from ringing a phone over and over; raise it only if you really want more. It cannot be higher than the daily limit.",
        example: "5",
    },
    Setting {
        table: "daemon",
        field: "phone_calls_per_day",
        kind: Kind::Count,
        help: "the most calls JARVIS may place in one day, from 1 to 100",
        group: "phone",
        default: Some("10"),
        unset_means: "Uses the default of 10 calls a day. Calls to other people have their own, lower limits that are not settable.",
        example: "20",
    },
    Setting {
        table: "policy",
        field: "trust",
        kind: Kind::ToolIds,
        help: "tools allowed to run without asking",
        group: "permissions",
        default: None,
        unset_means: "Nothing is trusted by default: a tool that the policy holds always asks you first.",
        example: "jarvis.files.edit",
    },
    Setting {
        table: "daemon",
        field: "notifications",
        kind: Kind::Flag,
        help: "show a desktop notification when a scheduled task finishes and no console is open: on or off",
        group: "advanced",
        default: Some("on"),
        unset_means: "On: a reminder you asked for reaches you even with the console closed. Nothing is shown while the console is open (it shows the result itself).",
        example: "off",
    },
    Setting {
        table: "daemon",
        field: "resume_interrupted",
        kind: Kind::Flag,
        help: "continue a task that a restart cut off, once: on or off",
        group: "advanced",
        default: Some("on"),
        unset_means: "On: after a restart, a recent run that was interrupted gets one continuation in the same conversation, told to check what was already done before repeating it. Approvals and permissions apply as always.",
        example: "off",
    },
    Setting {
        table: "daemon",
        field: "push_topic",
        kind: Kind::Text,
        help: "an ntfy topic to push approvals and scheduled results to when no console is open",
        group: "advanced",
        default: None,
        unset_means: "No push. When set, JARVIS sends a short message (never the answer or the question) to that ntfy topic when it needs your approval or a scheduled task finishes while the console is closed. The topic name is the secret: make it long and unguessable.",
        example: "jarvis-7f3a9c2e51b84d",
    },
    Setting {
        table: "daemon",
        field: "push_server",
        kind: Kind::Text,
        help: "the ntfy server for push (https only)",
        group: "advanced",
        default: Some("https://ntfy.sh"),
        unset_means: "Uses the public ntfy.sh server. Point it at your own ntfy server to keep the messages off third-party servers.",
        example: "https://ntfy.example.com",
    },
    Setting {
        table: "daemon",
        field: "http_port",
        kind: Kind::Port,
        help: "the local port the console and CLI use",
        group: "advanced",
        default: Some("8765"),
        unset_means: "Uses the default port.",
        example: "8765",
    },
    Setting {
        table: "daemon",
        field: "mcp_serve_port",
        kind: Kind::Port,
        help: "let other programs (such as another AI app) use JARVIS's tools, on this port. Not needed for JARVIS's own built-in tools",
        group: "advanced",
        default: None,
        unset_means: "Off by design: no other program can use JARVIS unless you choose a port here.",
        example: "8766",
    },
];
/// One setting as shown to a person.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SettingView {
    /// `table.field`, the name `jarvis config` uses.
    pub key: String,
    /// What shape of value it takes: `text`, `file`, `folders`, `words`, `port` or `tools`.
    pub kind: &'static str,
    /// A one-line description.
    pub help: &'static str,
    /// The current value (a path for a key file, never a key), if set.
    pub value: Option<String>,
    /// For a file setting: `present` or `missing`.
    pub file_state: Option<&'static str>,
    /// The tab it lives on: `brain`, `voice`, `files`, `permissions`, `google` or `advanced`.
    pub group: &'static str,
    /// The value in force when it is not set, if there is one.
    pub default: Option<&'static str>,
    /// What it means that it is not set.
    pub unset_means: &'static str,
    /// A sample value.
    pub example: &'static str,
}

fn find(key: &str) -> Option<&'static Setting> {
    SETTINGS.iter().find(|setting| {
        format!("{}.{}", setting.table, setting.field) == key || setting.field == key
    })
}

/// The supported settings, one per line, for an error message.
#[must_use]
pub fn supported() -> String {
    SETTINGS
        .iter()
        .map(|setting| format!("  {}.{:<26} {}", setting.table, setting.field, setting.help))
        .collect::<Vec<_>>()
        .join("\n")
}

fn unknown(key: &str) -> String {
    format!(
        "{key:?} is not a setting that can be changed here. The settings are:\n{}",
        supported()
    )
}

fn strip_verbatim(path: PathBuf) -> PathBuf {
    let text = path.display().to_string();
    text.strip_prefix(r"\\?\").map_or(path, PathBuf::from)
}

fn convert(kind: Kind, values: &[String]) -> Result<Value, String> {
    let one = || match values {
        [only] if !only.trim().is_empty() => Ok(only.trim().to_owned()),
        _ => Err("this setting takes exactly one value".to_owned()),
    };
    match kind {
        Kind::Text => Ok(Value::String(one()?)),
        Kind::File => {
            let path = PathBuf::from(one()?);
            if !path.is_absolute() || !path.is_file() {
                return Err(format!(
                    "{} is not the absolute path of an existing file",
                    path.display()
                ));
            }
            Ok(Value::String(path.display().to_string()))
        }
        Kind::Folders => {
            let mut roots = Vec::new();
            for value in values {
                match std::fs::canonicalize(value) {
                    Ok(resolved) if resolved.is_dir() => {
                        roots.push(Value::String(
                            strip_verbatim(resolved).display().to_string(),
                        ));
                    }
                    _ => return Err(format!("{value} is not a folder that exists")),
                }
            }
            Ok(Value::Array(roots))
        }
        Kind::Words => {
            let words: Vec<String> = if values.len() == 1 {
                values[0].split_whitespace().map(str::to_owned).collect()
            } else {
                values.to_vec()
            };
            if words.is_empty() {
                return Err("give the command to run, for example: node -e".to_owned());
            }
            Ok(Value::Array(words.into_iter().map(Value::String).collect()))
        }
        Kind::Flag => match one()?.to_ascii_lowercase().as_str() {
            "on" | "true" | "yes" => Ok(Value::Boolean(true)),
            "off" | "false" | "no" => Ok(Value::Boolean(false)),
            _ => Err("say on or off".to_owned()),
        },
        Kind::Port => one()?
            .parse::<u16>()
            .ok()
            .filter(|port| *port > 0)
            .map(|port| Value::Integer(i64::from(port)))
            .ok_or_else(|| "a port is a number from 1 to 65535".to_owned()),
        Kind::Count => one()?
            .parse::<u64>()
            .ok()
            .and_then(|count| i64::try_from(count).ok())
            .map(Value::Integer)
            .ok_or_else(|| "a count is a whole number from 0 to 9223372036854775807".to_owned()),
        Kind::Numbers => {
            let mut numbers: Vec<Value> = Vec::new();
            for part in values
                .iter()
                .flat_map(|value| value.split([',', ';', '\n']))
                .filter(|part| !part.trim().is_empty())
            {
                let canonical = jarvis_core::normalize_phone_number(part).map_err(|reason| {
                    format!("{} is not a usable number: {reason}", part.trim())
                })?;
                let canonical = Value::String(canonical);
                if !numbers.contains(&canonical) {
                    numbers.push(canonical);
                }
            }
            if numbers.is_empty() {
                return Err("give at least one number, such as +31612345678".to_owned());
            }
            Ok(Value::Array(numbers))
        }
        Kind::ToolIds => {
            if values.iter().any(|value| value.trim().is_empty()) {
                return Err("tool ids must not be blank".to_owned());
            }
            Ok(Value::Array(
                values.iter().cloned().map(Value::String).collect(),
            ))
        }
    }
}

fn read_table(store: &ConfigStore) -> Result<Table, String> {
    let text = std::fs::read_to_string(store.path())
        .map_err(|_| "there is no configuration yet; run `jarvis init`".to_owned())?;
    toml::from_str(&text).map_err(|_| "the stored configuration is not valid TOML".to_owned())
}

/// Reads the stored document, lets `change` edit it, checks the result with the daemon's own parser and saves it. A
/// refused change leaves the stored document untouched.
fn edit(
    paths: &AppPaths,
    change: impl FnOnce(&mut Table) -> Result<(), String>,
) -> Result<(), String> {
    let store = ConfigStore::from_paths(paths);
    let mut table = read_table(&store)?;
    change(&mut table)?;
    let document = toml::to_string(&table).map_err(|error| error.to_string())?;
    let config: Config =
        Config::parse_with_environment(&document, std::iter::empty::<(&str, &str)>())
            .map_err(|error| format!("that would make the configuration invalid: {error}"))?
            .into_config();
    store
        .save(&config)
        .map_err(|error| format!("the configuration could not be saved: {error}"))
}

fn table_mut<'a>(table: &'a mut Table, name: &str) -> Result<&'a mut Table, String> {
    table
        .entry(name.to_owned())
        .or_insert_with(|| Value::Table(Table::new()))
        .as_table_mut()
        .ok_or_else(|| format!("[{name}] in the configuration is not a table"))
}

fn lookup<'a>(table: &'a Table, setting: &Setting) -> Option<&'a Value> {
    table.get(setting.table)?.as_table()?.get(setting.field)
}

fn show_value(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Array(items) => items.iter().map(show_value).collect::<Vec<_>>().join(", "),
        other => other.to_string(),
    }
}

/// Every supported setting with its current value.
///
/// # Errors
///
/// Returns a message when there is no configuration yet or it cannot be read.
pub fn list(paths: &AppPaths) -> Result<Vec<SettingView>, String> {
    let table = read_table(&ConfigStore::from_paths(paths))?;
    Ok(SETTINGS
        .iter()
        .map(|setting| {
            let value = lookup(&table, setting).map(show_value);
            let file_state = (setting.kind == Kind::File)
                .then(|| {
                    value.as_deref().map(|path| {
                        if Path::new(path).is_file() {
                            "present"
                        } else {
                            "missing"
                        }
                    })
                })
                .flatten();
            SettingView {
                key: format!("{}.{}", setting.table, setting.field),
                kind: setting.kind.name(),
                help: setting.help,
                value,
                file_state,
                group: setting.group,
                default: setting.default,
                unset_means: setting.unset_means,
                example: setting.example,
            }
        })
        .collect())
}

/// One setting's value.
///
/// # Errors
///
/// Returns a message for an unknown or unset setting.
pub fn get(paths: &AppPaths, key: &str) -> Result<String, String> {
    let setting = find(key).ok_or_else(|| unknown(key))?;
    let table = read_table(&ConfigStore::from_paths(paths))?;
    lookup(&table, setting)
        .map(|value| match (setting.kind, value) {
            // A command reads as a command (`node -e`), not as a list.
            (Kind::Words, Value::Array(words)) => {
                words.iter().map(show_value).collect::<Vec<_>>().join(" ")
            }
            _ => show_value(value),
        })
        .ok_or_else(|| format!("{key} is not set"))
}

/// Sets one setting. The value is converted for its kind, the whole document is re-validated by the daemon's parser, and
/// nothing is written if that fails.
///
/// # Errors
///
/// Returns a message saying why the change was refused.
pub fn set(paths: &AppPaths, key: &str, values: &[String]) -> Result<(), String> {
    let setting = find(key).ok_or_else(|| unknown(key))?;
    let value = convert(setting.kind, values)?;
    edit(paths, |table| {
        table_mut(table, setting.table)?.insert(setting.field.to_owned(), value);
        Ok(())
    })
}

/// Removes one setting.
///
/// # Errors
///
/// Returns a message for an unknown setting or when removing it would leave the configuration invalid.
pub fn unset(paths: &AppPaths, key: &str) -> Result<(), String> {
    let setting = find(key).ok_or_else(|| unknown(key))?;
    edit(paths, |table| {
        table_mut(table, setting.table)?.remove(setting.field);
        Ok(())
    })
}

/// One change in a batch: set a setting to these values, or (`None`) remove it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Change {
    /// The setting, as `table.field` or its field name.
    pub key: String,
    /// The new values, or `None` to remove the setting.
    pub values: Option<Vec<String>>,
}

/// Applies several changes **together**: every value is converted, all are applied to the stored document, and the result is
/// checked once by the daemon's parser, so settings that only make sense as a pair (a code image and its interpreter, a
/// voice key and its voice) can be changed in one step. If any part is refused nothing is written.
///
/// # Errors
///
/// Returns a message saying which change was refused or why the result would be invalid.
pub fn apply(paths: &AppPaths, changes: &[Change]) -> Result<(), String> {
    let mut prepared = Vec::new();
    for change in changes {
        let setting = find(&change.key).ok_or_else(|| unknown(&change.key))?;
        let value = match &change.values {
            Some(values) => Some(convert(setting.kind, values)?),
            None => None,
        };
        prepared.push((setting, value));
    }
    edit(paths, |table| {
        for (setting, value) in prepared {
            let section = table_mut(table, setting.table)?;
            match value {
                Some(value) => {
                    section.insert(setting.field.to_owned(), value);
                }
                None => {
                    section.remove(setting.field);
                }
            }
        }
        Ok(())
    })
}

/// What the owner has decided about one tool, beyond what the tool declares for itself.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Posture {
    /// No decision: the tool's own declaration and the risk threshold apply.
    Default,
    /// Always ask before this tool runs (`policy.approval`).
    Ask,
    /// Run without asking (`policy.trust`); never waives the external-communication rule, the ceiling or a denial.
    Trusted,
    /// Never run (`policy.deny`).
    Off,
}

impl Posture {
    /// Parses `default`, `ask`, `trusted` or `off`.
    ///
    /// # Errors
    ///
    /// Returns a message for anything else.
    pub fn parse(word: &str) -> Result<Self, String> {
        match word {
            "default" => Ok(Self::Default),
            "ask" => Ok(Self::Ask),
            "trusted" => Ok(Self::Trusted),
            "off" => Ok(Self::Off),
            _ => Err("a posture is default, ask, trusted or off".to_owned()),
        }
    }

    /// The word for this posture.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::Ask => "ask",
            Self::Trusted => "trusted",
            Self::Off => "off",
        }
    }
}

fn valid_tool_id(tool: &str) -> bool {
    !tool.is_empty()
        && tool.len() <= 128
        && tool
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
}

/// The tools the owner has given a posture, as `tool -> ask | trusted | off`. A tool with no entry has the default.
///
/// # Errors
///
/// Returns a message when there is no configuration yet.
pub fn tool_postures(
    paths: &AppPaths,
) -> Result<std::collections::BTreeMap<String, &'static str>, String> {
    let table = read_table(&ConfigStore::from_paths(paths))?;
    let mut out = std::collections::BTreeMap::new();
    let policy = table.get("policy").and_then(Value::as_table);
    for (field, name) in [("trust", "trusted"), ("deny", "off")] {
        for item in policy
            .and_then(|policy| policy.get(field))
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            if let Some(tool) = item.as_str() {
                out.insert(tool.to_owned(), name);
            }
        }
    }
    for tool in policy
        .and_then(|policy| policy.get("approval"))
        .and_then(Value::as_table)
        .into_iter()
        .flat_map(|map| map.keys())
    {
        out.insert(tool.clone(), "ask");
    }
    Ok(out)
}

/// Sets what the owner has decided about one tool. A tool has exactly one posture, so choosing one removes the others.
///
/// # Errors
///
/// Returns a message for an unusable tool id or when the result would be an invalid configuration.
pub fn set_tool_posture(paths: &AppPaths, tool: &str, posture: Posture) -> Result<(), String> {
    if !valid_tool_id(tool) {
        return Err("a tool id is letters, digits, dots, dashes and underscores".to_owned());
    }
    edit(paths, |table| {
        let policy = table_mut(table, "policy")?;
        for field in ["trust", "deny"] {
            if let Some(items) = policy.get_mut(field).and_then(Value::as_array_mut) {
                items.retain(|item| item.as_str() != Some(tool));
            }
        }
        if let Some(map) = policy.get_mut("approval").and_then(Value::as_table_mut) {
            map.remove(tool);
        }
        match posture {
            Posture::Default => {}
            Posture::Ask => {
                table_mut(policy, "approval")?
                    .insert(tool.to_owned(), Value::String("ask".to_owned()));
            }
            Posture::Trusted | Posture::Off => {
                let field = if posture == Posture::Trusted {
                    "trust"
                } else {
                    "deny"
                };
                let list = policy
                    .entry(field.to_owned())
                    .or_insert_with(|| Value::Array(Vec::new()));
                list.as_array_mut()
                    .ok_or_else(|| format!("policy.{field} in the configuration is not a list"))?
                    .push(Value::String(tool.to_owned()));
            }
        }
        // Empty lists and tables are dropped so the file stays as small as it was before the change.
        policy.retain(|_, value| match value {
            Value::Array(items) => !items.is_empty(),
            Value::Table(map) => !map.is_empty(),
            _ => true,
        });
        Ok(())
    })
}

/// Whether a tool is listed as trusted in a stored document (used by tests).
#[cfg(test)]
fn is_trusted(paths: &AppPaths, tool: &str) -> bool {
    tool_postures(paths).is_ok_and(|postures| postures.get(tool) == Some(&"trusted"))
}

/// Which key a keys operation means.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SecretKind {
    /// The model provider's key.
    Model,
    /// The speech provider's key.
    Voice,
    /// The web search provider's key.
    Search,
    /// The Google OAuth client secret.
    Google,
    /// The key of the different provider the fallback model is asked at.
    Fallback,
    /// The Telegram bot token.
    Telegram,
}

impl SecretKind {
    /// Parses `model` or `voice`.
    ///
    /// # Errors
    ///
    /// Returns a message for anything else.
    pub fn parse(word: &str) -> Result<Self, String> {
        match word {
            "model" => Ok(Self::Model),
            "voice" => Ok(Self::Voice),
            "search" => Ok(Self::Search),
            "google" => Ok(Self::Google),
            "fallback" => Ok(Self::Fallback),
            "telegram" => Ok(Self::Telegram),
            _ => Err("name the key: model, voice, search, google, fallback or telegram".to_owned()),
        }
    }

    const fn reference(self) -> &'static str {
        match self {
            Self::Model => "executor_api_key_ref",
            Self::Voice => "speech_api_key_ref",
            Self::Search => "search_api_key_ref",
            Self::Google => "google_client_secret_ref",
            Self::Fallback => "executor_fallback_api_key_ref",
            Self::Telegram => "telegram_token_ref",
        }
    }

    const fn file_name(self) -> &'static str {
        match self {
            Self::Model => "model.key",
            Self::Voice => "speech.key",
            Self::Search => "search.key",
            Self::Google => "google.key",
            Self::Fallback => "fallback.key",
            Self::Telegram => "telegram.key",
        }
    }

    /// The environment variable the CLI suggests for this key.
    #[must_use]
    pub const fn default_env(self) -> &'static str {
        match self {
            Self::Model => "JARVIS_MODEL_API_KEY",
            Self::Voice => "ELEVENLABS_API_KEY",
            Self::Search => "OLLAMA_API_KEY",
            Self::Google => "GOOGLE_CLIENT_SECRET",
            Self::Fallback => "JARVIS_FALLBACK_API_KEY",
            Self::Telegram => "JARVIS_TELEGRAM_TOKEN",
        }
    }
}

/// Whether a key is configured, without reading it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct KeyState {
    /// `not set`, `set`, `EMPTY FILE` or `FILE MISSING`.
    pub state: &'static str,
    /// The key file's path, if configured.
    pub path: Option<String>,
}

/// The state of one key. Only the file's existence and size are looked at, never its contents.
///
/// # Errors
///
/// Returns a message when there is no configuration yet.
pub fn key_state(paths: &AppPaths, kind: SecretKind) -> Result<KeyState, String> {
    let table = read_table(&ConfigStore::from_paths(paths))?;
    let path = SETTINGS
        .iter()
        .find(|setting| setting.field == kind.reference())
        .and_then(|setting| lookup(&table, setting))
        .map(show_value);
    let state = match &path {
        None => "not set",
        Some(path) => match std::fs::metadata(path) {
            Ok(meta) if meta.len() > 0 => "set",
            Ok(_) => "EMPTY FILE",
            Err(_) => "FILE MISSING",
        },
    };
    Ok(KeyState { state, path })
}

/// Checks that text is usable as a key and returns it trimmed. The refusal never repeats the text.
///
/// # Errors
///
/// Returns a message for an empty, oversized or space-containing key.
pub fn validate_secret(raw: &str) -> Result<String, String> {
    let key = raw.trim().to_owned();
    if key.is_empty() || key.len() > 256 || !key.bytes().all(|byte| byte.is_ascii_graphic()) {
        return Err("that is not a usable key (empty, too long, or containing spaces)".to_owned());
    }
    Ok(key)
}

/// Writes a key to a private file in the configuration directory and points the configuration at it.
///
/// # Errors
///
/// Returns a message when the key is unusable, the file cannot be written, or the configuration would become invalid.
pub fn set_secret(paths: &AppPaths, kind: SecretKind, raw: &str) -> Result<PathBuf, String> {
    let key = validate_secret(raw)?;
    let path = paths.config().join(kind.file_name());
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("the key file could not be written: {error}"))?;
    }
    std::fs::write(&path, key)
        .map_err(|error| format!("the key file could not be written: {error}"))?;
    secure_private_file(PathKind::Config, &path)
        .map_err(|error| format!("the key file could not be made private: {error}"))?;
    let reference = path.display().to_string();
    set(paths, kind.reference(), &[reference])?;
    Ok(path)
}

/// Saves a model provider as one configuration commit. A replacement key is staged in a new private file;
/// failed validation cannot overwrite the previously referenced credential. Old files remain for running readers.
///
/// # Errors
/// Refuses invalid settings, unreadable credentials, or reuse of an existing credential at a changed address.
pub fn set_provider(
    paths: &AppPaths,
    fallback: bool,
    model: &str,
    address: Option<&str>,
    key: Option<&str>,
) -> Result<(), String> {
    let prefix = if fallback {
        "executor_fallback"
    } else {
        "executor"
    };
    let url_key = format!("daemon.{prefix}_base_url");
    let ref_key = format!("daemon.{prefix}_api_key_ref");
    let model_key = format!("daemon.{prefix}_model_name");
    if model.trim().is_empty()
        || (!fallback && address.is_none())
        || (address.is_none() && key.is_some())
    {
        return Err(
            "choose a model and provider address; same-provider fallback uses the main key"
                .to_owned(),
        );
    }
    let old_address = get(paths, &url_key).unwrap_or_default();
    let secret = key.map(validate_secret).transpose()?;
    if let Some(address) = address
        && secret.is_none()
        && (address.trim_end_matches('/') != old_address.trim_end_matches('/')
            || key_state(
                paths,
                if fallback {
                    SecretKind::Fallback
                } else {
                    SecretKind::Model
                },
            )?
            .state
                != "set")
    {
        return Err("provide a key for the selected provider; an existing key cannot be reused at a different address".to_owned());
    }
    let mut changes = vec![
        Change {
            key: model_key,
            values: Some(vec![model.trim().to_owned()]),
        },
        Change {
            key: url_key,
            values: address.map(|value| vec![value.to_owned()]),
        },
    ];
    let staged = if let Some(secret) = secret {
        use std::io::Write as _;
        let path = paths
            .config()
            .join(format!("{prefix}-{}.key", jarvis_core::scratch_tag()));
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt as _;
            options.mode(0o600);
        }
        let mut file = options
            .open(&path)
            .map_err(|_| "could not stage the credential".to_owned())?;
        let result = (|| {
            secure_private_file(PathKind::Config, &path)
                .map_err(|_| "could not protect the credential")?;
            file.write_all(secret.as_bytes())
                .and_then(|()| file.sync_all())
                .map_err(|_| "could not write the credential")
        })();
        if let Err(error) = result {
            let _ = std::fs::remove_file(&path);
            return Err(error.to_owned());
        }
        changes.push(Change {
            key: ref_key,
            values: Some(vec![path.display().to_string()]),
        });
        Some(path)
    } else {
        if address.is_none() {
            changes.push(Change {
                key: ref_key,
                values: None,
            });
        }
        None
    };
    let result = apply(paths, &changes);
    if result.is_err()
        && let Some(path) = staged
    {
        let _ = std::fs::remove_file(path);
    }
    result
}

/// Removes the voice key and the settings that depend on it. The model key cannot be removed, only replaced.
///
/// # Errors
///
/// Returns a message for the model key, or when the configuration would become invalid.
pub fn remove_secret(paths: &AppPaths, kind: SecretKind) -> Result<(), String> {
    if kind == SecretKind::Model {
        return Err(
            "the model key cannot be removed, only replaced (the assistant needs a model)"
                .to_owned(),
        );
    }
    edit(paths, |table| {
        let daemon = table_mut(table, "daemon")?;
        let dependent: &[&str] = match kind {
            SecretKind::Search => &["search_api_key_ref"],
            SecretKind::Google => &["google_client_secret_ref"],
            SecretKind::Fallback => &["executor_fallback_api_key_ref"],
            SecretKind::Telegram => &["telegram_token_ref"],
            _ => &["speech_api_key_ref", "speech_voice_id", "speech_model"],
        };
        for field in dependent {
            daemon.remove(*field);
        }
        Ok(())
    })?;
    let _ = std::fs::remove_file(paths.config().join(kind.file_name()));
    Ok(())
}

#[cfg(test)]
mod tests;
