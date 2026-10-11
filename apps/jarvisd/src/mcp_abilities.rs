//! The abilities a person can switch on with one click (`ADR-0169`).
//!
//! "Add an MCP server" means nothing to most people. An **ability** is a ready-made one: a plain name, a plain sentence about what
//! it does and when it asks, a pinned program, and the tools that only read. Turning one on writes the same `mcp-servers.toml`
//! entry `jarvis mcp add` writes (so there is one mechanism, and one parser that checks it), prepares the program in the
//! background so the first start does not time out on a download, and then asks for one restart.
//!
//! **Every program here is pinned to a version that was run live** (`docs/research/integrations/mcp.md`), not `latest`: switching
//! on an ability runs a package from the internet as the owner, and a version that changed under them would be a program nobody
//! looked at. **The read-only tools are the owner-facing statement of what runs without asking**, so each list was checked
//! against the server's real tool list.

use std::collections::BTreeMap;
use std::sync::Mutex;

use serde::Serialize;

use crate::mcp_admin::ServerBody;

/// One ability.
pub struct Ability {
    /// The server name in `mcp-servers.toml` and the id in the API.
    pub id: &'static str,
    /// The name a person sees.
    pub title: &'static str,
    /// What it does, in a sentence.
    pub blurb: &'static str,
    /// What it asks about, in a sentence.
    pub asks: &'static str,
    /// What it sends away, in a sentence, or empty.
    pub shares: &'static str,
    /// The program that starts it, resolved on this computer.
    pub program: &'static str,
    /// The package and its fixed arguments, after the program.
    pub arguments: fn(&std::path::Path) -> Vec<String>,
    /// The tools (by the server's own names) that only read.
    pub read_only_tools: &'static [&'static str],
}

fn browser_arguments(runtime_directory: &std::path::Path) -> Vec<String> {
    // Edge is on every Windows computer, so it is what the browser drives there; elsewhere Chrome.
    let browser = if cfg!(windows) { "msedge" } else { "chrome" };
    [
        "-y",
        "@playwright/mcp@0.0.83",
        "--isolated",
        "--headless",
        "--browser",
        browser,
        "--output-dir",
        &runtime_directory
            .join("playwright-mcp")
            .display()
            .to_string(),
    ]
    .iter()
    .map(|part| (*part).to_owned())
    .collect()
}

fn docs_arguments(_runtime_directory: &std::path::Path) -> Vec<String> {
    ["-y", "@upstash/context7-mcp@4.3.0"]
        .iter()
        .map(|part| (*part).to_owned())
        .collect()
}

/// The abilities, in the order they are shown.
pub const ABILITIES: &[Ability] = &[
    Ability {
        id: "browser",
        title: "Browse the web",
        blurb: "Opens pages and reads them, including pages that need JavaScript, and can click and fill in forms. It uses a private window that is thrown away afterwards.",
        asks: "Opening and reading a page runs on its own. Clicking, typing, uploading and running scripts ask you first.",
        shares: "",
        program: "npx",
        arguments: browser_arguments,
        read_only_tools: &[
            "browser_navigate",
            "browser_navigate_back",
            "browser_snapshot",
            "browser_find",
            "browser_take_screenshot",
            "browser_tabs",
            "browser_wait_for",
            "browser_console_messages",
            "browser_network_requests",
            "browser_network_request",
            "browser_resize",
            "browser_close",
        ],
    },
    Ability {
        id: "docs",
        title: "Look up developer documentation",
        blurb: "Finds the current documentation and code examples for a programming library, so answers do not rely on what the model remembers.",
        asks: "Looking things up runs on its own.",
        shares: "The library name and your question are sent to context7.com.",
        program: "npx",
        arguments: docs_arguments,
        read_only_tools: &["resolve-library-id", "query-docs"],
    },
];

/// Finds an ability by id.
#[must_use]
pub fn find(id: &str) -> Option<&'static Ability> {
    ABILITIES.iter().find(|ability| ability.id == id)
}

impl Ability {
    /// The request that adds this ability to the document.
    #[must_use]
    pub fn body(&self, runtime_directory: &std::path::Path) -> ServerBody {
        let mut command = vec![self.program.to_owned()];
        command.extend((self.arguments)(runtime_directory));
        ServerBody {
            name: self.id.to_owned(),
            command,
            endpoint: None,
            read_only: false,
            read_only_tools: self
                .read_only_tools
                .iter()
                .map(|tool| (*tool).to_owned())
                .collect(),
        }
    }
}

/// The tool-name prefixes (`mcp.browser.`) of the configured servers that are exactly a catalog entry.
///
/// These are the only third-party tools the daemon serves to another app (`ADR-0172`): the program and its arguments are the pinned
/// ones, and the read-only tools were listed by this project. A server of the same name with any other command is **not** vetted, so
/// an owner who wrote their own `browser` entry does not get it re-served.
#[must_use]
pub fn vetted_prefixes(document: Option<&str>, runtime_directory: &std::path::Path) -> Vec<String> {
    crate::mcp_admin::configured_servers(document)
        .into_iter()
        .filter_map(|server| {
            let ability = find(&server.name)?;
            (server.kind == "stdio"
                && server.command.get(1..) == Some(&(ability.arguments)(runtime_directory)[..]))
            .then(|| format!("mcp.{}.", ability.id))
        })
        .collect()
}

/// How the preparation of one ability is going.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Preparation {
    /// The program is being downloaded.
    Running,
    /// It finished.
    Done,
    /// It did not finish (no network, or the package is gone).
    Failed,
}

static PREPARATION: Mutex<BTreeMap<String, Preparation>> = Mutex::new(BTreeMap::new());

/// What is known about an ability's preparation in this daemon's life.
#[must_use]
pub fn preparation(id: &str) -> Option<Preparation> {
    PREPARATION
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .get(id)
        .copied()
}

fn set_preparation(id: &str, state: Preparation) {
    PREPARATION
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .insert(id.to_owned(), state);
}

/// Downloads the program in the background, so the first start finds it ready.
///
/// Runs the package with `--help` and no input, under the **same allowlisted environment** a server gets. Returns at once; the
/// state is read with [`preparation`]. A failure only means the first start may be slow: it is recorded, not fatal.
pub fn prepare(
    ability: &'static Ability,
    program_path: String,
    runtime_directory: &std::path::Path,
) {
    set_preparation(ability.id, Preparation::Running);
    let mut arguments = (ability.arguments)(runtime_directory);
    // Everything up to the first flag is the package; the server's own flags are dropped and replaced by `--help`.
    if let Some(flag) = arguments
        .iter()
        .position(|argument| argument.starts_with("--"))
    {
        arguments.truncate(flag);
    }
    arguments.push("--help".to_owned());
    let id = ability.id;
    std::thread::spawn(move || {
        let mut command = std::process::Command::new(program_path);
        command
            .args(arguments)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .env_clear()
            .envs(jarvis_mcp_transport::server_environment());
        let outcome = command.status();
        set_preparation(
            id,
            if outcome.is_ok_and(|status| status.success()) {
                Preparation::Done
            } else {
                Preparation::Failed
            },
        );
    });
}

/// Set to anything to stop the built-in abilities from being added on start (tests, and owners who never want them).
pub const OPT_OUT_ENV: &str = "JARVIS_NO_BUILTIN_ABILITIES";

/// The abilities that have already been offered to this profile, one id per line.
///
/// Offered **once**: if the owner later switches one off, it stays off. Without this record a switched-off ability would come back
/// on the next start, which would make "off" mean nothing.
const SEEDED_FILE: &str = "abilities-seeded.txt";

/// How long a first start waits for the downloads before leaving the new abilities for the next start.
const PREPARE_WAIT: std::time::Duration = std::time::Duration::from_secs(30);

fn seeded_ids(directory: &std::path::Path) -> Vec<String> {
    std::fs::read_to_string(directory.join(SEEDED_FILE))
        .unwrap_or_default()
        .lines()
        .map(|line| line.trim().to_owned())
        .filter(|line| !line.is_empty())
        .collect()
}

fn old_browser_arguments(runtime_directory: &std::path::Path) -> Vec<String> {
    let mut arguments = browser_arguments(runtime_directory);
    arguments.truncate(arguments.len().saturating_sub(2));
    arguments
}

/// Replaces only the exact built-in browser command from before `P11-003a`.
///
/// A same-named command the owner edited is deliberately not touched: matching the complete argument
/// list is what distinguishes JARVIS's seeded entry from the owner's server.
fn migrate_browser_entry(
    document: Option<&str>,
    runtime_directory: &std::path::Path,
    resolve: &dyn Fn(&str) -> Result<String, String>,
) -> Result<Option<String>, String> {
    let Some(current) = document else {
        return Ok(None);
    };
    let old = old_browser_arguments(runtime_directory);
    let should_migrate = crate::mcp_admin::configured_servers(Some(current))
        .iter()
        .any(|server| {
            server.name == "browser"
                && server.kind == "stdio"
                && server.command.get(1..) == Some(&old[..])
        });
    if !should_migrate {
        return Ok(Some(current.to_owned()));
    }
    let without = crate::mcp_admin::remove_from_document(Some(current), "browser")?;
    let browser = find("browser").ok_or_else(|| "the browser ability is missing".to_owned())?;
    crate::mcp_admin::add_to_document(Some(&without), &browser.body(runtime_directory), resolve)
        .map(Some)
}

/// Adds each built-in ability this profile has not been offered yet, and returns the ones that were added with their program.
///
/// An ability whose program (Node.js) is missing is skipped **and not recorded**, so installing Node later makes it appear. One
/// the owner already configured is recorded and left alone. A document this cannot extend is left untouched.
pub fn seed_defaults(
    directory: &std::path::Path,
    runtime_directory: &std::path::Path,
    document: Option<&str>,
    resolve: &dyn Fn(&str) -> Result<String, String>,
) -> Vec<(&'static Ability, String)> {
    let mut seeded = seeded_ids(directory);
    let mut current = document.map(str::to_owned);
    let mut added = Vec::new();
    for ability in ABILITIES {
        if seeded.iter().any(|id| id == ability.id) {
            continue;
        }
        let configured = crate::mcp_admin::configured_servers(current.as_deref())
            .iter()
            .any(|server| server.name == ability.id);
        if configured {
            seeded.push(ability.id.to_owned());
            continue;
        }
        let Ok(program) = resolve(ability.program) else {
            continue;
        };
        match crate::mcp_admin::add_to_document(
            current.as_deref(),
            &ability.body(runtime_directory),
            resolve,
        ) {
            Ok(text) => {
                current = Some(text);
                seeded.push(ability.id.to_owned());
                added.push((ability, program));
            }
            Err(message) => {
                tracing::warn!(ability = ability.id, %message, "a built-in ability could not be added");
            }
        }
    }
    if !added.is_empty()
        && let Some(text) = &current
        && let Err(message) = crate::mcp_admin::write_document(directory, text)
    {
        tracing::warn!(%message, "the built-in abilities could not be written");
        return Vec::new();
    }
    let record = seeded.join("\n");
    if seeded.len() != seeded_ids(directory).len()
        && let Err(error) = std::fs::write(directory.join(SEEDED_FILE), record)
    {
        tracing::warn!(%error, "the record of offered abilities could not be written");
    }
    added
}

/// The document the daemon should start its servers from, after offering the built-in abilities.
///
/// The first start downloads each program once. It waits for that (bounded) so the abilities are there from the first minute; if
/// the download is not finished in time, or fails, the daemon starts from the document **as it was** and the new entries connect
/// on the next start, where the Abilities tab shows them as ready after a restart.
///
/// # Errors
///
/// Returns the sentence of an unreadable document, as [`crate::mcp_host::load_document`] does.
pub async fn startup_document(paths: &jarvis_storage::AppPaths) -> Result<Option<String>, String> {
    let directory = paths.config();
    let runtime_directory = paths.runtime();
    let before = crate::mcp_host::load_document(directory).map_err(|error| error.to_string())?;
    let migrated = migrate_browser_entry(
        before.as_deref(),
        runtime_directory,
        &crate::mcp_admin::find_program_on_path,
    )?;
    if migrated != before
        && let Some(document) = migrated.as_deref()
    {
        crate::mcp_admin::write_document(directory, document)?;
    }
    if std::env::var_os(OPT_OUT_ENV).is_some() {
        return Ok(migrated);
    }
    let added = seed_defaults(
        directory,
        runtime_directory,
        migrated.as_deref(),
        &crate::mcp_admin::find_program_on_path,
    );
    if added.is_empty() {
        return Ok(migrated);
    }
    for (ability, program) in &added {
        prepare(ability, program.clone(), runtime_directory);
    }
    let deadline = tokio::time::Instant::now() + PREPARE_WAIT;
    while added
        .iter()
        .any(|(ability, _)| preparation(ability.id) == Some(Preparation::Running))
        && tokio::time::Instant::now() < deadline
    {
        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
    }
    if added
        .iter()
        .all(|(ability, _)| preparation(ability.id) == Some(Preparation::Done))
    {
        crate::mcp_host::load_document(directory).map_err(|error| error.to_string())
    } else {
        Ok(migrated)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_ability_has_a_pinned_package_and_a_plain_description() {
        for ability in ABILITIES {
            let arguments = (ability.arguments)(std::path::Path::new("runtime"));
            let package = arguments
                .iter()
                .find(|argument| argument.contains('@') && !argument.starts_with('-'))
                .unwrap_or_else(|| panic!("{} names no package", ability.id));
            assert!(
                package.rsplit('@').next().is_some_and(|version| version
                    .chars()
                    .next()
                    .is_some_and(|c| c.is_ascii_digit())),
                "{} must pin a version, not float on latest: {package}",
                ability.id
            );
            assert!(!ability.blurb.is_empty() && !ability.asks.is_empty());
            assert!(!ability.read_only_tools.is_empty());
        }
    }

    #[test]
    fn ids_are_unique_and_valid_server_names() {
        let mut seen = std::collections::BTreeSet::new();
        for ability in ABILITIES {
            assert!(seen.insert(ability.id), "{} appears twice", ability.id);
            assert!(
                ability
                    .id
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c == '-'),
                "{}",
                ability.id
            );
        }
    }

    /// The tools that run without asking are limited to reads: nothing that clicks, types, uploads or runs code.
    #[test]
    fn the_browser_never_runs_an_acting_tool_without_asking() {
        let browser = find("browser").unwrap_or_else(|| panic!("the browser ability exists"));
        for acting in [
            "browser_click",
            "browser_type",
            "browser_fill_form",
            "browser_press_key",
            "browser_select_option",
            "browser_drag",
            "browser_drop",
            "browser_hover",
            "browser_file_upload",
            "browser_evaluate",
            "browser_run_code_unsafe",
            "browser_handle_dialog",
        ] {
            assert!(
                !browser.read_only_tools.contains(&acting),
                "{acting} acts on a page and must ask"
            );
        }
    }

    #[test]
    fn the_request_carries_the_program_the_tools_and_no_blanket_read_only() {
        let body = find("docs")
            .unwrap_or_else(|| panic!("exists"))
            .body(std::path::Path::new("runtime"));
        assert_eq!(body.name, "docs");
        assert_eq!(body.command[0], "npx");
        assert!(
            !body.read_only,
            "the ability names its read-only tools one by one"
        );
        assert_eq!(body.read_only_tools, ["resolve-library-id", "query-docs"]);
    }

    fn scratch(name: &str) -> std::path::PathBuf {
        let directory = std::env::temp_dir().join(format!(
            "jarvis-seed-{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |elapsed| elapsed.as_nanos())
        ));
        std::fs::create_dir_all(&directory).unwrap_or_else(|_| panic!("scratch directory"));
        directory
    }

    #[allow(
        clippy::unnecessary_wraps,
        reason = "the resolver the module takes returns a Result"
    )]
    fn found(program: &str) -> Result<String, String> {
        Ok(format!("/usr/bin/{program}"))
    }

    fn missing(program: &str) -> Result<String, String> {
        Err(format!("{program} is not installed"))
    }

    fn names(directory: &std::path::Path) -> Vec<String> {
        let document = crate::mcp_host::load_document(directory).ok().flatten();
        crate::mcp_admin::configured_servers(document.as_deref())
            .into_iter()
            .map(|server| server.name)
            .collect()
    }

    #[test]
    fn only_a_server_that_is_exactly_the_catalog_entry_is_vetted() {
        let directory = scratch("vetted");
        let runtime = directory.join("runtime");
        seed_defaults(&directory, &runtime, None, &found);
        let document = crate::mcp_host::load_document(&directory).ok().flatten();
        assert_eq!(
            vetted_prefixes(document.as_deref(), &runtime),
            ["mcp.browser.", "mcp.docs."]
        );

        // An owner's own server under a catalog name, running something else, is not vetted.
        let mine = "[[servers]]\nname = \"browser\"\n[servers.transport]\nkind = \"stdio\"\nprogram = \"/usr/bin/npx\"\nargs = [\"-y\", \"something-else@1.0.0\"]\n";
        assert!(vetted_prefixes(Some(mine), &runtime).is_empty());
        let _ = std::fs::remove_dir_all(&directory);
    }

    #[test]
    fn the_browser_uses_the_profile_runtime_directory_for_automatic_output() {
        let directory = scratch("output");
        let runtime = directory.join("runtime");
        let body = find("browser")
            .unwrap_or_else(|| panic!("the browser ability exists"))
            .body(&runtime);
        let position = body
            .command
            .iter()
            .position(|part| part == "--output-dir")
            .unwrap_or_else(|| panic!("the browser command must name its output directory"));
        assert_eq!(
            body.command.get(position + 1),
            Some(&runtime.join("playwright-mcp").display().to_string())
        );
        let _ = std::fs::remove_dir_all(&directory);
    }

    #[test]
    fn the_exact_old_browser_entry_migrates_but_an_owner_edited_one_does_not() {
        let directory = scratch("migration");
        let runtime = directory.join("runtime");
        let browser = find("browser").unwrap_or_else(|| panic!("the browser ability exists"));
        let mut old = browser.body(&runtime);
        old.command.truncate(old.command.len().saturating_sub(2));
        let old_document = crate::mcp_admin::add_to_document(None, &old, &found)
            .unwrap_or_else(|error| panic!("old document: {error}"));
        assert!(
            vetted_prefixes(Some(&old_document), &runtime).is_empty(),
            "the old command is no longer the vetted catalog entry"
        );

        let migrated = migrate_browser_entry(Some(&old_document), &runtime, &found)
            .unwrap_or_else(|error| panic!("migration: {error}"))
            .unwrap_or_else(|| panic!("the document remains"));
        assert_eq!(vetted_prefixes(Some(&migrated), &runtime), ["mcp.browser."]);
        assert!(migrated.contains("--output-dir"));
        assert!(migrated.contains(&runtime.join("playwright-mcp").display().to_string()));

        let edited = old_document.replace("args = [", "args = [\"--owner-edited\", ");
        let untouched = migrate_browser_entry(Some(&edited), &runtime, &found)
            .unwrap_or_else(|error| panic!("custom document: {error}"));
        assert_eq!(untouched.as_deref(), Some(edited.as_str()));
        let _ = std::fs::remove_dir_all(&directory);
    }

    #[test]
    fn a_new_profile_is_offered_every_ability_once() {
        let directory = scratch("once");
        let runtime = directory.join("runtime");
        let added = seed_defaults(&directory, &runtime, None, &found);
        assert_eq!(added.len(), ABILITIES.len());
        assert_eq!(names(&directory), ["browser", "docs"]);

        let again = seed_defaults(
            &directory,
            &runtime,
            crate::mcp_host::load_document(&directory)
                .ok()
                .flatten()
                .as_deref(),
            &found,
        );
        assert!(again.is_empty(), "nothing is added twice");
        let _ = std::fs::remove_dir_all(&directory);
    }

    #[test]
    fn an_ability_the_owner_switched_off_does_not_come_back() {
        let directory = scratch("off");
        let runtime = directory.join("runtime");
        seed_defaults(&directory, &runtime, None, &found);
        let document = crate::mcp_host::load_document(&directory).ok().flatten();
        let without = crate::mcp_admin::remove_from_document(document.as_deref(), "browser")
            .unwrap_or_else(|_| panic!("removed"));
        crate::mcp_admin::write_document(&directory, &without)
            .unwrap_or_else(|_| panic!("written"));

        let added = seed_defaults(&directory, &runtime, Some(&without), &found);
        assert!(added.is_empty());
        assert_eq!(names(&directory), ["docs"]);
        let _ = std::fs::remove_dir_all(&directory);
    }

    #[test]
    fn a_missing_program_adds_nothing_and_is_retried_after_it_is_installed() {
        let directory = scratch("missing");
        let runtime = directory.join("runtime");
        assert!(seed_defaults(&directory, &runtime, None, &missing).is_empty());
        assert!(names(&directory).is_empty());

        let added = seed_defaults(&directory, &runtime, None, &found);
        assert_eq!(
            added.len(),
            ABILITIES.len(),
            "installing Node later is enough"
        );
        let _ = std::fs::remove_dir_all(&directory);
    }

    #[test]
    fn what_the_owner_already_configured_is_left_alone() {
        let directory = scratch("owned");
        let runtime = directory.join("runtime");
        let mine = "[[servers]]\nname = \"browser\"\n[servers.transport]\nkind = \"stdio\"\nprogram = \"/usr/bin/npx\"\nargs = [\"-y\", \"something-else@1.0.0\"]\n";
        crate::mcp_admin::write_document(&directory, mine).unwrap_or_else(|_| panic!("written"));

        let added = seed_defaults(&directory, &runtime, Some(mine), &found);
        assert_eq!(added.len(), 1, "only the one the owner lacks");
        assert_eq!(added[0].0.id, "docs");
        let document = crate::mcp_host::load_document(&directory)
            .ok()
            .flatten()
            .unwrap_or_default();
        assert!(document.contains("something-else@1.0.0"));
        assert!(!document.contains("@playwright/mcp"));
        let _ = std::fs::remove_dir_all(&directory);
    }
}
