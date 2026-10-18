//! `jarvis init` and `jarvis start`: from a fresh install to a conversation in three commands.
//!
//! # Why these exist
//!
//! Everything else in this product assumed a configured daemon, and configuring one meant hand-writing a TOML document
//! with four provider settings and a key *file*, or setting five environment variables. A user who has installed
//! JARVIS and has not read the architecture documents could not ask it a question. `init` is the answer:
//!
//! ```text
//! jarvis init      # finds your local Ollama, asks which model and which folder, writes the configuration
//! jarvis start     # starts the daemon in the background and waits until it answers
//! jarvis chat
//! ```
//!
//! # What `init` decides for you, and what it refuses to
//!
//! - It **tests before it writes**: the selected model must answer, and the required `ElevenLabs` voice must produce
//!   audio, before configuration or new key files are saved. Provider listing is a hint; a model can be typed when
//!   listing is unavailable.
//! - It **never overwrites** an existing configuration without `--force`, because that file may hold decisions
//!   (policy overrides, a code sandbox) it knows nothing about.
//! - It grants **no folder unless you name one.** The filesystem tools exist only for folders an operator chose
//!   (`ADR-0020`); a default would grant a directory nobody picked.
//! - Keys are **file references**, never values in the document (`ADR-0121`). Local servers use a placeholder unless
//!   a key file is supplied. Hosted keys come from explicit files, environment variables or hidden terminal entry
//!   (`ADR-0214`); failed secure entry never falls back to echoing the key.
//! - The document is validated by the same parser the daemon uses before it is saved, so a configuration `init`
//!   writes is one the daemon will start with.

use std::io::{IsTerminal, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use jarvis_storage::{AppPaths, Config, ConfigStore, PathKind, secure_private_file};

use crate::output::ExitStatus;

/// Where a local Ollama listens, which is also what makes "find the user's models" possible without a question.
const OLLAMA_ORIGIN: &str = "http://127.0.0.1:11434";

/// The base URL the OpenAI-compatible adapter is pointed at for Ollama.
const OLLAMA_BASE_URL: &str = "http://localhost:11434/v1";

/// The key file's name, inside the profile's configuration directory.
const KEY_FILE_NAME: &str = "model.key";

/// The placeholder Ollama requires but ignores.
const OLLAMA_PLACEHOLDER_KEY: &str = "ollama";

/// Windows permission checks and optional provider discovery can exceed one minute.
/// Waiting is bounded, including when another invocation is already starting the profile.
const START_WAIT: Duration = Duration::from_secs(180);

/// What `init` was asked for.
#[derive(Debug, Default, Eq, PartialEq)]
struct InitRequest {
    model: Option<String>,
    base_url: Option<String>,
    api_key_file: Option<PathBuf>,
    workspaces: Vec<PathBuf>,
    code_image: Option<String>,
    code_interpreter: Option<String>,
    trust_code: bool,
    /// A file holding the speech provider's key, used in place.
    voice_key_file: Option<PathBuf>,
    /// Compatibility flag: setup now always requires a speech key, read from a file, environment or hidden prompt.
    elevenlabs: bool,
    voice_id: Option<String>,
    voice_model: Option<String>,
    force: bool,
}

pub(crate) fn flag_value(arguments: &[String], flag: &str) -> Option<String> {
    arguments
        .windows(2)
        .find(|pair| pair[0] == flag)
        .map(|pair| pair[1].clone())
}

fn flag_values(arguments: &[String], flag: &str) -> Vec<String> {
    arguments
        .windows(2)
        .filter(|pair| pair[0] == flag)
        .map(|pair| pair[1].clone())
        .collect()
}

fn parse_init(arguments: &[String]) -> InitRequest {
    InitRequest {
        model: flag_value(arguments, "--model"),
        base_url: flag_value(arguments, "--base-url"),
        api_key_file: flag_value(arguments, "--api-key-file").map(PathBuf::from),
        workspaces: flag_values(arguments, "--workspace")
            .into_iter()
            .map(PathBuf::from)
            .collect(),
        code_image: flag_value(arguments, "--code-image"),
        code_interpreter: flag_value(arguments, "--code-interpreter"),
        trust_code: arguments.iter().any(|argument| argument == "--trust-code"),
        voice_key_file: flag_value(arguments, "--voice-key-file").map(PathBuf::from),
        elevenlabs: arguments.iter().any(|argument| argument == "--elevenlabs"),
        voice_id: flag_value(arguments, "--voice-id"),
        voice_model: flag_value(arguments, "--voice-model"),
        force: arguments.iter().any(|argument| argument == "--force"),
    }
}

/// The models a local Ollama lists, or `None` when nothing answers there.
async fn ollama_models() -> Option<Vec<String>> {
    let client = reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(2))
        .build()
        .ok()?;
    let response = client
        .get(format!("{OLLAMA_ORIGIN}/api/tags"))
        .send()
        .await
        .ok()?;
    if !response.status().is_success() {
        return None;
    }
    let body: serde_json::Value = response.json().await.ok()?;
    let names: Vec<String> = body
        .get("models")?
        .as_array()?
        .iter()
        .filter_map(|model| model.get("name")?.as_str().map(str::to_owned))
        .collect();
    Some(names)
}

/// Asks a question on standard error and returns the trimmed answer, or `None` without a terminal.
fn ask(question: &str) -> Option<String> {
    if !std::io::stdin().is_terminal() {
        return None;
    }
    eprint!("{question}");
    let _ = std::io::stderr().flush();
    let mut answer = String::new();
    std::io::stdin().read_line(&mut answer).ok()?;
    Some(answer.trim().to_owned())
}

/// Chooses a model from a list: the named one, or the person's pick, or the first.
///
/// A named model must be **in the list**: the point of looking at the server is to refuse a typo now.
fn choose_model(requested: Option<&str>, available: &[String]) -> Result<String, String> {
    if let Some(requested) = requested {
        return if available.iter().any(|name| name == requested) {
            Ok(requested.to_owned())
        } else {
            Err(format!(
                "the server does not list a model called {requested:?}; it has: {}",
                available.join(", ")
            ))
        };
    }
    match available {
        [] => Err(
            "the server lists no models; pull one first (for example `ollama pull llama3.2`)"
                .to_owned(),
        ),
        [only] => Ok(only.clone()),
        many => {
            eprintln!("Models available:");
            for (index, name) in many.iter().enumerate() {
                eprintln!("  {}. {name}", index + 1);
            }
            let picked = ask(&format!("Which one? [1-{}, default 1] ", many.len()));
            match picked.as_deref() {
                None | Some("") => Ok(many[0].clone()),
                Some(text) => text
                    .parse::<usize>()
                    .ok()
                    .and_then(|number| number.checked_sub(1))
                    .and_then(|index| many.get(index))
                    .cloned()
                    .ok_or_else(|| format!("{text:?} is not a number from 1 to {}", many.len())),
            }
        }
    }
}

/// Renders a TOML literal string, which has no escapes — so a Windows path is written as it is.
///
/// A value containing a single quote or a line break cannot be a literal string, and is refused rather than
/// escaped: nothing `init` writes legitimately contains either.
fn literal(value: &str) -> Result<String, String> {
    if value.contains('\'') || value.contains('\n') || value.contains('\r') {
        return Err(format!(
            "{value:?} contains a quote or a line break, which a configuration value cannot"
        ));
    }
    Ok(format!("'{value}'"))
}

/// The optional code tool: a container image and the interpreter it runs a snippet with.
#[derive(Debug, Eq, PartialEq)]
struct CodeSetup {
    image: String,
    interpreter: Vec<String>,
    /// The owner's standing decision that a snippet in the throwaway container may run without asking.
    trusted: bool,
}

/// Reads the code-tool flags. `--trust-code` without an image is refused, because there would be nothing to trust.
fn code_setup(request: &InitRequest) -> Result<Option<CodeSetup>, String> {
    let Some(image) = &request.code_image else {
        if request.code_interpreter.is_some() || request.trust_code {
            return Err("--code-interpreter and --trust-code need --code-image".to_owned());
        }
        return Ok(None);
    };
    let interpreter = request
        .code_interpreter
        .as_deref()
        .unwrap_or("sh -c")
        .split_whitespace()
        .map(str::to_owned)
        .collect::<Vec<_>>();
    if interpreter.is_empty() {
        return Err("--code-interpreter must name a program".to_owned());
    }
    Ok(Some(CodeSetup {
        image: image.clone(),
        interpreter,
        trusted: request.trust_code,
    }))
}

/// The optional spoken voice: where the key is, and which voice and model to use.
#[derive(Debug, Eq, PartialEq)]
struct VoiceSetup {
    key_file: PathBuf,
    voice_id: Option<String>,
    model: Option<String>,
}

/// The private speech key file for environment or hidden-prompt entry, beside the model key.
const SPEECH_KEY_FILE_NAME: &str = "speech.key";

#[path = "init_setup.rs"]
mod setup;

/// Builds the configuration document `init` saves.
fn render_config(
    model: &str,
    base_url: &str,
    key_file: &Path,
    workspaces: &[PathBuf],
    code: Option<&CodeSetup>,
    voice: Option<&VoiceSetup>,
) -> Result<String, String> {
    let mut lines = vec![
        "schema_version = 1".to_owned(),
        String::new(),
        "[profile]".to_owned(),
        "name = \"default\"".to_owned(),
        String::new(),
        "[logging]".to_owned(),
        "level = \"info\"".to_owned(),
        String::new(),
        "[daemon]".to_owned(),
        "shutdown_timeout_seconds = 30".to_owned(),
        "http_enabled = true".to_owned(),
        "executor_model = \"openai-compatible\"".to_owned(),
        format!("executor_model_name = {}", literal(model)?),
        format!("executor_base_url = {}", literal(base_url)?),
        format!(
            "executor_api_key_ref = {}",
            literal(&key_file.display().to_string())?
        ),
    ];
    if !workspaces.is_empty() {
        let roots = workspaces
            .iter()
            .map(|root| literal(&root.display().to_string()))
            .collect::<Result<Vec<_>, _>>()?;
        lines.push(format!("tool_workspace_roots = [{}]", roots.join(", ")));
    }
    if let Some(voice) = voice {
        lines.push(format!(
            "speech_api_key_ref = {}",
            literal(&voice.key_file.display().to_string())?
        ));
        if let Some(id) = &voice.voice_id {
            lines.push(format!("speech_voice_id = {}", literal(id)?));
        }
        if let Some(model) = &voice.model {
            lines.push(format!("speech_model = {}", literal(model)?));
        }
    }
    if let Some(code) = code {
        lines.push(format!("code_sandbox_image = {}", literal(&code.image)?));
        let words = code
            .interpreter
            .iter()
            .map(|word| literal(word))
            .collect::<Result<Vec<_>, _>>()?;
        lines.push(format!("code_sandbox_interpreter = [{}]", words.join(", ")));
        if code.trusted {
            // After the `[daemon]` keys, because a table header ends them.
            lines.push(String::new());
            lines.push("[policy]".to_owned());
            lines.push("trust = [\"jarvis.code.run\"]".to_owned());
        }
    }
    let document = lines.join("\n") + "\n";
    Ok(document)
}

fn suggested_folders(home: &Path) -> Vec<PathBuf> {
    ["Development", "Projects", "code"]
        .iter()
        .map(|name| home.join(name))
        .filter(|folder| folder.is_dir())
        .collect()
}

/// Suggestions are not grants: only a path or number explicitly entered is saved.
fn resolve_folders(named: &[PathBuf]) -> Result<Vec<PathBuf>, String> {
    let mut folders = named.to_vec();
    if folders.is_empty() && can_ask() {
        let home_var = if cfg!(windows) { "USERPROFILE" } else { "HOME" };
        let suggestions = std::env::var_os(home_var)
            .map(PathBuf::from)
            .map(|home| suggested_folders(&home))
            .unwrap_or_default();
        eprintln!(
            "Choose only project folders JARVIS may read and write. Changes still ask for approval. No folder is granted by default."
        );
        for (index, folder) in suggestions.iter().enumerate() {
            eprintln!("  {}. {}", index + 1, folder.display());
        }
        while let Some(answer) =
            ask("Folder path or suggested number (blank to finish, none is fine): ")
        {
            if answer.is_empty() {
                break;
            }
            let folder = if let Ok(number) = answer.parse::<usize>() {
                number
                    .checked_sub(1)
                    .and_then(|index| suggestions.get(index))
                    .cloned()
                    .ok_or_else(|| {
                        "choose a listed folder number or enter a full path".to_owned()
                    })?
            } else {
                PathBuf::from(answer)
            };
            folders.push(folder);
        }
    }
    let mut roots = Vec::new();
    for folder in &folders {
        match std::fs::canonicalize(folder) {
            Ok(resolved) if resolved.is_dir() => {
                let resolved = strip_verbatim(resolved);
                if !roots.contains(&resolved) {
                    roots.push(resolved);
                }
            }
            _ => return Err(format!("{} is not a folder that exists", folder.display())),
        }
    }
    Ok(roots)
}
/// What was written and what to do next.
fn report(
    path: &Path,
    model: &str,
    base_url: &str,
    roots: &[PathBuf],
    code: Option<&CodeSetup>,
    voice: Option<&VoiceSetup>,
) {
    println!("configured {}", path.display());
    println!("  model   {model}");
    println!("  server  {base_url}");
    if roots.is_empty() {
        println!("  folders none (no file access; add folders later in Settings, Folders & code)");
    } else {
        for root in roots {
            println!("  folder  {}", root.display());
        }
    }
    if let Some(code) = code {
        println!(
            "  code    {} ({}){}",
            code.image,
            code.interpreter.join(" "),
            if code.trusted {
                ", runs without asking"
            } else {
                ", asks before each run"
            }
        );
    }
    match voice {
        Some(voice) => println!(
            "  voice   ElevenLabs ({})",
            voice.voice_id.as_deref().unwrap_or("default voice")
        ),
        None => {
            println!("  voice   the browser's own (add a key later in Settings, Voice & search)");
        }
    }
    println!();
    println!("next:  jarvis start     # start the assistant in the background");
    println!("       jarvis chat      # talk to it");
}

/// `jarvis init`
pub async fn init(paths: &AppPaths, arguments: &[String]) -> ExitStatus {
    let request = parse_init(arguments);
    let store = ConfigStore::from_paths(paths);
    if store.path().exists() && !request.force {
        eprintln!(
            "jarvis: {} already exists, so nothing was changed. Use Settings or `jarvis config` to change one setting; `jarvis init --force` replaces the whole configuration.",
            store.path().display()
        );
        return ExitStatus::Rejected;
    }

    match setup::configure(paths, &request).await {
        Ok(()) => ExitStatus::Ok,
        Err(message) => {
            eprintln!("jarvis: {message}");
            ExitStatus::Rejected
        }
    }
}

/// Windows canonical paths carry a `\\?\` prefix that a person never typed and a TOML reader should not see.
pub(crate) fn strip_verbatim(path: PathBuf) -> PathBuf {
    let text = path.display().to_string();
    text.strip_prefix(r"\\?\").map_or(path, PathBuf::from)
}

/// Writes a key to a file only its owner can read.
pub(crate) fn write_secret(path: &Path, contents: &str) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, contents)?;
    secure_private_file(PathKind::Config, path)
        .map_err(|error| std::io::Error::other(error.to_string()))
}

/// A daemon process this command started, which can be asked whether it has already exited.
enum Daemon {
    #[cfg(not(windows))]
    Child(std::process::Child),
    /// Started through PowerShell, so only its process id is known.
    #[cfg(windows)]
    Pid(u32),
}

impl Daemon {
    fn has_exited(&mut self) -> bool {
        match self {
            #[cfg(not(windows))]
            Self::Child(child) => matches!(child.try_wait(), Ok(Some(_))),
            #[cfg(windows)]
            Self::Pid(pid) => !windows_process_alive(*pid),
        }
    }
}

/// Starts the daemon in the background, with no handle shared with this process.
///
/// On Windows a child started directly inherits the pipe this command's output goes through, so a script that captures
/// `jarvis start` would wait for the daemon to exit. The workspace forbids `unsafe`, which is what clearing that
/// inheritance needs, so the daemon is started through PowerShell's `Start-Process` instead, which shares nothing.
fn spawn_daemon(binary: &Path, root: Option<&str>) -> Result<Daemon, String> {
    #[cfg(windows)]
    {
        let quote = |text: &str| format!("'{}'", text.replace('\'', "''"));
        let arguments = root.map_or_else(
            || " -ArgumentList @('daemon')".to_owned(),
            |root| format!(" -ArgumentList @('daemon','--root',{})", quote(root)),
        );
        let script = format!(
            "(Start-Process -FilePath {} -WindowStyle Hidden -PassThru{arguments}).Id",
            quote(&binary.display().to_string())
        );
        let output = std::process::Command::new("powershell.exe")
            .args(["-NoProfile", "-NonInteractive", "-Command", &script])
            .stdin(std::process::Stdio::null())
            .output()
            .map_err(|error| error.to_string())?;
        let pid = String::from_utf8_lossy(&output.stdout)
            .trim()
            .parse::<u32>()
            .map_err(|_| "PowerShell did not report a process id".to_owned())?;
        Ok(Daemon::Pid(pid))
    }
    #[cfg(not(windows))]
    {
        let mut command = std::process::Command::new(binary);
        command.arg("daemon");
        if let Some(root) = root {
            command.args(["--root", root]);
        }
        // Its own process group, so closing the terminal (an SSH session ending) does not send it the hangup that
        // would stop it: `jarvis start` over SSH must still be running after you log out.
        std::os::unix::process::CommandExt::process_group(&mut command, 0);
        command
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
        command
            .spawn()
            .map(Daemon::Child)
            .map_err(|error| error.to_string())
    }
}

/// Whether a process id is still running, asked of `tasklist` (no `unsafe` is needed).
#[cfg(windows)]
fn windows_process_alive(pid: u32) -> bool {
    std::process::Command::new("tasklist")
        .args(["/FI", &format!("PID eq {pid}"), "/NH", "/FO", "CSV"])
        .stdin(std::process::Stdio::null())
        .output()
        .is_ok_and(|output| String::from_utf8_lossy(&output.stdout).contains(&format!("\"{pid}\"")))
}
/// Whether a configuration already exists for this profile.
pub fn is_configured(paths: &AppPaths) -> bool {
    ConfigStore::from_paths(paths).path().exists()
}

/// Whether there is a terminal to ask questions on.
pub fn can_ask() -> bool {
    std::io::stdin().is_terminal()
}

/// `jarvis start`: starts the daemon in the background and waits until it is listening.
pub fn start(paths: &AppPaths, arguments: &[String]) -> ExitStatus {
    let store = ConfigStore::from_paths(paths);
    let loaded = match store.load() {
        Ok(loaded) => loaded,
        Err(error) => {
            eprintln!("jarvis: no usable configuration ({error}); run `jarvis init` first");
            return ExitStatus::Unavailable;
        }
    };
    let daemon = loaded.config().daemon();
    if !daemon.http_enabled() {
        eprintln!(
            "jarvis: the HTTP API is off in this configuration, so there is nothing to talk to; run `jarvis init --force`"
        );
        return ExitStatus::Rejected;
    }
    let address = SocketAddr::from(([127, 0, 0, 1], daemon.http_port()));
    if TcpStream::connect_timeout(&address, Duration::from_millis(300)).is_ok() {
        println!("already running on {address}");
        return ExitStatus::Ok;
    }

    // The daemon is this same executable run as `jarvis daemon` (`ADR-0144`), so there is nothing to find and its
    // version is always this client's.
    let Ok(binary) = std::env::current_exe() else {
        eprintln!("jarvis: could not locate this program to start the daemon with");
        return ExitStatus::Unavailable;
    };
    // A reason left by an earlier failed start is stale now.
    let startup_error = paths.runtime().join(jarvisd::STARTUP_ERROR_FILE);
    let _ = std::fs::remove_file(&startup_error);
    let mut daemon = match spawn_daemon(&binary, flag_value(arguments, "--root").as_deref()) {
        Ok(daemon) => daemon,
        Err(message) => {
            eprintln!("jarvis: the daemon could not be started: {message}");
            return ExitStatus::Unavailable;
        }
    };
    let deadline = Instant::now() + START_WAIT;
    let mut waiting_for_owner = false;
    while Instant::now() < deadline {
        if TcpStream::connect_timeout(&address, Duration::from_millis(300)).is_ok() {
            println!("started; listening on {address}");
            println!(
                "next:  jarvis hud    # the console (opened for you unless you passed --no-open)"
            );
            println!("       jarvis chat   # or talk in the terminal");
            return ExitStatus::Ok;
        }
        // The daemon exiting is the answer, and a far better one than waiting out the clock: it refuses to start
        // for a reason (a bad folder, a missing key) that `jarvis daemon` itself would have printed.
        if waiting_for_owner
            && let Ok(reason) = std::fs::read_to_string(&startup_error)
            && !reason.trim().is_empty()
            && !startup_owner_is_initializing(&reason)
        {
            eprintln!("jarvis: the daemon could not start: {}", reason.trim());
            return ExitStatus::Unavailable;
        }
        if !waiting_for_owner && daemon.has_exited() {
            match std::fs::read_to_string(&startup_error) {
                Ok(reason) if startup_owner_is_initializing(&reason) => {
                    waiting_for_owner = true;
                    eprintln!("jarvis: this profile is already starting; waiting for it to listen");
                    continue;
                }
                Ok(reason) if !reason.trim().is_empty() => {
                    eprintln!("jarvis: the daemon could not start: {}", reason.trim());
                }
                _ => eprintln!(
                    "jarvis: the daemon exited at startup. Run `jarvis daemon` in a terminal to see why."
                ),
            }
            return ExitStatus::Unavailable;
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    eprintln!(
        "jarvis: the daemon did not start listening within {} seconds",
        START_WAIT.as_secs()
    );
    ExitStatus::Unavailable
}

fn startup_owner_is_initializing(reason: &str) -> bool {
    jarvisd::startup_error_is_existing_owner(reason)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An absolute path on the platform running the test: `C:/..` is relative on Unix, and the daemon's own parser
    /// (which these tests drive) refuses a relative key or folder.
    fn abs(path: &str) -> PathBuf {
        if cfg!(windows) {
            PathBuf::from(format!("C:/{path}"))
        } else {
            PathBuf::from(format!("/{path}"))
        }
    }

    fn words(line: &str) -> Vec<String> {
        line.split_whitespace().map(str::to_owned).collect()
    }

    #[test]
    fn flags_are_read_from_anywhere_and_workspaces_repeat() {
        let request = parse_init(&words(
            "init --model glm --workspace C:/a --base-url http://x/v1 --workspace C:/b --force",
        ));
        assert_eq!(request.model.as_deref(), Some("glm"));
        assert_eq!(request.base_url.as_deref(), Some("http://x/v1"));
        assert_eq!(
            request.workspaces,
            vec![PathBuf::from("C:/a"), PathBuf::from("C:/b")]
        );
        assert!(request.force);
    }

    #[test]
    fn a_model_the_server_does_not_list_is_refused_with_what_it_has() {
        let available = vec!["a".to_owned(), "b".to_owned()];
        assert_eq!(choose_model(Some("b"), &available), Ok("b".to_owned()));
        let error = choose_model(Some("c"), &available)
            .err()
            .unwrap_or_default();
        assert!(error.contains("\"c\"") && error.contains("a, b"), "{error}");
    }

    #[test]
    fn a_single_model_is_chosen_and_an_empty_server_is_explained() {
        assert_eq!(
            choose_model(None, &["only".to_owned()]),
            Ok("only".to_owned())
        );
        let error = choose_model(None, &[]).err().unwrap_or_default();
        assert!(error.contains("ollama pull"), "{error}");
    }

    #[test]
    fn the_rendered_document_is_one_the_daemon_accepts() {
        let document = render_config(
            "glm-5.3:cloud",
            "http://localhost:11434/v1",
            abs("me/jarvis/model.key").as_path(),
            &[abs("me/notes")],
            None,
            None,
        )
        .unwrap_or_else(|error| panic!("{error}"));
        let loaded = Config::parse_with_environment(&document, std::iter::empty::<(&str, &str)>())
            .unwrap_or_else(|error| panic!("{error}\n{document}"));
        let daemon = loaded.config().daemon();
        assert!(daemon.http_enabled());
        assert_eq!(daemon.executor_model_name(), Some("glm-5.3:cloud"));
        assert_eq!(
            daemon.executor_base_url(),
            Some("http://localhost:11434/v1")
        );
        assert_eq!(daemon.tool_workspace_roots().len(), 1);
        assert!(daemon.has_complete_provider());
    }

    #[test]
    fn a_code_tool_and_its_trust_are_written_and_the_daemon_accepts_them() {
        let mut request = parse_init(&words("init --code-image node:22-alpine --trust-code"));
        request.code_interpreter = Some("node -e".to_owned());
        let code = code_setup(&request)
            .unwrap_or_else(|error| panic!("{error}"))
            .unwrap_or_else(|| panic!("a code setup"));
        assert!(code.trusted);
        let document = render_config(
            "m",
            "http://h/v1",
            abs("k").as_path(),
            &[],
            Some(&code),
            None,
        )
        .unwrap_or_else(|error| panic!("{error}"));
        let loaded = Config::parse_with_environment(&document, std::iter::empty::<(&str, &str)>())
            .unwrap_or_else(|error| panic!("{error}\n{document}"));
        assert_eq!(
            loaded.config().daemon().code_sandbox_image(),
            Some("node:22-alpine")
        );
        assert_eq!(loaded.config().policy().trust(), ["jarvis.code.run"]);

        // Without `--trust-code` no trust is written: the default stays a question.
        let plain = parse_init(&words("init --code-image alpine:3"));
        let code = code_setup(&plain)
            .unwrap_or_else(|error| panic!("{error}"))
            .unwrap_or_else(|| panic!("a code setup"));
        let document = render_config(
            "m",
            "http://h/v1",
            abs("k").as_path(),
            &[],
            Some(&code),
            None,
        )
        .unwrap_or_else(|error| panic!("{error}"));
        assert!(!document.contains("trust"), "{document}");
        assert_eq!(code.interpreter, ["sh", "-c"]);
    }

    #[test]
    fn trust_without_an_image_is_refused() {
        assert!(code_setup(&parse_init(&words("init --trust-code"))).is_err());
        assert!(code_setup(&parse_init(&words("init --code-interpreter node"))).is_err());
        assert_eq!(code_setup(&parse_init(&words("init"))), Ok(None));
    }

    #[test]
    fn a_windows_path_is_written_verbatim_and_a_quote_is_refused() {
        let document = render_config(
            "m",
            "http://h/v1",
            Path::new(r"C:\Users\me\model.key"),
            &[],
            None,
            None,
        )
        .unwrap_or_else(|error| panic!("{error}"));
        assert!(document.contains(r"'C:\Users\me\model.key'"), "{document}");
        assert!(render_config("it's", "http://h/v1", Path::new("/k"), &[], None, None).is_err());
    }

    #[test]
    fn the_verbatim_prefix_is_removed_and_only_that() {
        assert_eq!(
            strip_verbatim(PathBuf::from(r"\\?\C:\a\b")),
            PathBuf::from(r"C:\a\b")
        );
        assert_eq!(strip_verbatim(PathBuf::from("/a/b")), PathBuf::from("/a/b"));
    }

    #[test]
    fn a_voice_is_written_into_the_daemon_table_before_the_policy_table() {
        let voice = VoiceSetup {
            key_file: abs("keys/speech.key"),
            voice_id: Some("abc123".to_owned()),
            model: None,
        };
        let code = CodeSetup {
            image: "node:22-alpine".to_owned(),
            interpreter: vec!["node".to_owned(), "-e".to_owned()],
            trusted: true,
        };
        let document = render_config(
            "m",
            "http://h/v1",
            abs("k").as_path(),
            &[],
            Some(&code),
            Some(&voice),
        )
        .unwrap_or_else(|error| panic!("{error}"));
        assert!(document.contains(&format!(
            "speech_api_key_ref = '{}'",
            abs("keys/speech.key").display()
        )));
        assert!(document.contains("speech_voice_id = 'abc123'"));
        assert!(!document.contains("speech_model"));
        let voice_at = document.find("speech_api_key_ref").unwrap_or(usize::MAX);
        let policy_at = document.find("[policy]").unwrap_or(0);
        assert!(
            voice_at < policy_at,
            "a key after [policy] would land in the wrong table"
        );
        assert!(
            Config::parse_with_environment(&document, std::iter::empty::<(&str, &str)>()).is_ok(),
            "the daemon's own parser must accept it"
        );
    }

    #[test]
    fn voice_flags_take_references_and_identifiers_never_a_raw_key() {
        let request = parse_init(&words(
            "--voice-key-file speech.key --voice-id abc --elevenlabs",
        ));
        assert_eq!(request.voice_key_file, Some(PathBuf::from("speech.key")));
        assert_eq!(request.voice_id.as_deref(), Some("abc"));
        assert!(request.elevenlabs);
        assert!(!parse_init(&words("--elevenlabs-key synthetic-key")).elevenlabs);
    }

    #[test]
    fn competing_start_waits_but_real_startup_failures_are_not_hidden() {
        let owned = "another jarvisd process already owns this profile";
        assert!(startup_owner_is_initializing(&format!("{owned}\n")));
        for reason in [
            "",
            "the speech settings are not usable",
            "failed to bind the loopback HTTP transport",
            "profile lock could not be read",
        ] {
            assert!(!startup_owner_is_initializing(reason));
        }
    }
}
