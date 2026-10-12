//! Making the endpoint for other agents reachable from the internet, with one switch (`ADR-0173`).
//!
//! A cloud agent (a phone agent such as `ElevenLabs`, a hosted assistant) connects **from the provider's servers**, so an endpoint that listens on
//! this computer only cannot be reached by it. The easiest route that needs no account, no router setting and no certificate is a Cloudflare **quick
//! tunnel**: the free `cloudflared` program makes an outbound connection and Cloudflare gives it a public `https://â€¦trycloudflare.com` address that
//! forwards to the local port.
//!
//! **What JARVIS adds is only the plumbing, not trust.** The endpoint still requires the access key on every request (`mcp_access::require_key`) and
//! is still rate bound, so a stranger who finds the address gets a 401. The tunnel is started with `--http-host-header` set to the loopback address,
//! so the endpoint's own Host check (the DNS-rebinding guard) sees what it always saw and **nothing in it was loosened**.
//!
//! **Pinned and checked.** If `cloudflared` is not already installed, JARVIS downloads one fixed release from Cloudflare's GitHub releases and
//! refuses to run it unless its SHA-256 equals the digest pinned here (copied from the release's published digest, and re-checked against the
//! downloaded file). The program runs with the same small environment an MCP server gets, and is stopped with the daemon.
//!
//! **Honest limits.** A quick tunnel's address **changes every time it starts**, and Cloudflare documents quick tunnels as a testing service with no
//! uptime guarantee and no support for server-sent events.
//!
//! **A stable address (`ADR-0202`).** An owner with a Cloudflare account and a domain can instead give JARVIS a **named tunnel**: the tunnel token
//! (made once in the Cloudflare dashboard) and the public hostname they published for it. JARVIS then runs `cloudflared tunnel run` with the token
//! in the environment (never on the command line, where other programs can read it), and the address is the hostname, the same after every restart.
//! With no token and hostname saved nothing changes: the quick tunnel above is used. The token lives in a private file of its own and is never
//! returned by the API or written to a log. The hostname is the one name the endpoint will accept as its own `Host` once the key has been checked
//! (`McpTunnel::is_public_host`), so the rebinding guard behind it is still the loopback one.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;

use base64::Engine as _;
use serde::Serialize;
use sha2::{Digest, Sha256};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::sync::{oneshot, watch};

/// The `cloudflared` release JARVIS downloads when none is installed.
pub const CLOUDFLARED_VERSION: &str = "2026.10.0";

/// The file whose presence means the owner wants the endpoint reachable from the internet.
const MARKER_FILE: &str = "mcp-public.on";

/// The owner's named tunnel (`ADR-0202`): the token Cloudflare made, and the public hostname published for it.
const NAMED_TOKEN_FILE: &str = "cloudflare-tunnel.token";
const NAMED_HOST_FILE: &str = "cloudflare-tunnel.host";

/// The longest token or pasted install command taken in. A real token is about 180 characters.
const NAMED_TOKEN_LIMIT: usize = 2048;

/// How long to wait before starting the tunnel again after it stopped on its own.
const RETRY_AFTER: Duration = Duration::from_secs(10);

/// The most a download may take, and the most it may be.
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(600);
const DOWNLOAD_LIMIT_BYTES: usize = 100 * 1024 * 1024;

/// One pinned download.
struct Build {
    asset: &'static str,
    sha256: &'static str,
    megabytes: u64,
}

/// The release assets that are a single executable, with the digests GitHub publishes for release 2026.10.0.
const BUILDS: &[(&str, &str, Build)] = &[
    (
        "windows",
        "x86_64",
        Build {
            asset: "cloudflared-windows-amd64.exe",
            sha256: "86aee4017b26625cee8484c113558f48effa4cd47f7aa05fcf425604e5d2b23c",
            megabytes: 55,
        },
    ),
    (
        "linux",
        "x86_64",
        Build {
            asset: "cloudflared-linux-amd64",
            sha256: "d33ff2d14475178d2012c2c56beba87389ac5ded27649519f198a7d3134a99db",
            megabytes: 40,
        },
    ),
    (
        "linux",
        "aarch64",
        Build {
            asset: "cloudflared-linux-arm64",
            sha256: "e6422b9d4f72d3194bc5a38676f13667c06666523217b842a877d72a80b5ac08",
            megabytes: 38,
        },
    ),
];

fn build_for_this_machine() -> Option<&'static Build> {
    BUILDS
        .iter()
        .find(|(os, arch, _)| *os == std::env::consts::OS && *arch == std::env::consts::ARCH)
        .map(|(_, _, build)| build)
}

/// What the tunnel is doing now.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum Tunnel {
    /// Not running.
    Off,
    /// The program is being downloaded.
    Getting,
    /// The program is starting and has no address yet.
    Starting,
    /// Reachable at this address (without the endpoint path).
    On {
        /// The public `https://` address.
        url: String,
    },
    /// It could not run, with a sentence an owner can act on.
    Problem {
        /// Why.
        reason: String,
    },
}

/// The tunnel and the owner's wish for it.
pub struct McpTunnel {
    directory: PathBuf,
    state: Mutex<Tunnel>,
    stop: Mutex<Option<oneshot::Sender<()>>>,
    named: RwLock<Option<Named>>,
    /// Counts changes to the named tunnel, so a running tunnel notices and starts again with the new one without a stop-and-start race.
    changes: watch::Sender<u64>,
}

/// An owner's named tunnel. `Debug` is written by hand so the token can never reach a log through a derived one.
#[derive(Clone)]
struct Named {
    host: String,
    token: String,
}

impl std::fmt::Debug for Named {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Named")
            .field("host", &self.host)
            .finish_non_exhaustive()
    }
}

/// How a watched child ended.
#[derive(Debug, Eq, PartialEq)]
enum Ended {
    Stopped,
    /// The named tunnel was saved or forgotten, so the child was stopped to start again with the new setting.
    Changed,
    Exited(String),
}

impl McpTunnel {
    /// A tunnel that keeps its marker and its downloaded program in `directory` (the configuration directory).
    #[must_use]
    pub fn new(directory: &Path) -> Arc<Self> {
        Arc::new(Self {
            directory: directory.to_path_buf(),
            state: Mutex::new(Tunnel::Off),
            stop: Mutex::new(None),
            named: RwLock::new(load_named(directory)),
            changes: watch::channel(0).0,
        })
    }

    fn named_changed(&self) {
        self.changes.send_modify(|count| *count += 1);
    }

    /// The hostname of the owner's named tunnel, when one is saved.
    #[must_use]
    pub fn stable_host(&self) -> Option<String> {
        self.named_snapshot().map(|named| named.host)
    }

    fn named_snapshot(&self) -> Option<Named> {
        self.named
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    /// Whether a request's `Host` header names the owner's own public hostname (and nothing else).
    ///
    /// Only the endpoint's key check may act on this: it lets an authenticated request that arrived through the named tunnel be seen by the
    /// endpoint as the loopback request it really is. A port is ignored; the name must match exactly.
    #[must_use]
    pub fn is_public_host(&self, host_header: &str) -> bool {
        let name = host_header
            .rsplit_once(':')
            .map_or(host_header, |(name, port)| {
                if port.bytes().all(|byte| byte.is_ascii_digit()) {
                    name
                } else {
                    host_header
                }
            });
        self.named_snapshot()
            .is_some_and(|named| named.host.eq_ignore_ascii_case(name))
    }

    /// Saves the owner's named tunnel: the hostname they published for it and its token (or the whole install command Cloudflare shows, from
    /// which the token is taken). Returns the cleaned hostname. Takes effect the next time the tunnel starts.
    ///
    /// # Errors
    ///
    /// Returns a sentence, never repeating the token, when either is unusable or cannot be saved.
    pub fn set_named(&self, host: &str, token: &str) -> Result<String, String> {
        let host = clean_hostname(host)?;
        let token = clean_token(token)?;
        std::fs::create_dir_all(&self.directory)
            .map_err(|error| format!("that could not be saved: {error}"))?;
        let path = self.directory.join(NAMED_TOKEN_FILE);
        let temporary = path.with_extension("part");
        std::fs::write(&temporary, &token)
            .map_err(|error| format!("that could not be saved: {error}"))?;
        let secured =
            jarvis_storage::secure_private_file(jarvis_storage::PathKind::Config, &temporary)
                .map_err(|_| "the token file could not be made private".to_owned())
                .and_then(|()| {
                    std::fs::rename(&temporary, &path)
                        .map_err(|error| format!("that could not be saved: {error}"))
                });
        if let Err(message) = secured {
            let _ = std::fs::remove_file(&temporary);
            return Err(message);
        }
        std::fs::write(self.directory.join(NAMED_HOST_FILE), format!("{host}\n"))
            .map_err(|error| format!("that could not be saved: {error}"))?;
        *self
            .named
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(Named {
            host: host.clone(),
            token,
        });
        self.named_changed();
        Ok(host)
    }

    /// Forgets the named tunnel, so the quick tunnel is used again. Takes effect the next time the tunnel starts.
    ///
    /// # Errors
    ///
    /// Returns a sentence when a file cannot be removed.
    pub fn clear_named(&self) -> Result<(), String> {
        *self
            .named
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = None;
        self.named_changed();
        for name in [NAMED_TOKEN_FILE, NAMED_HOST_FILE] {
            match std::fs::remove_file(self.directory.join(name)) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(format!("that could not be removed: {error}")),
            }
        }
        Ok(())
    }

    /// Whether the owner wants the endpoint reachable from the internet.
    #[must_use]
    pub fn wanted(&self) -> bool {
        self.directory.join(MARKER_FILE).exists()
    }

    /// Records the owner's wish.
    ///
    /// # Errors
    ///
    /// Returns a sentence when the marker cannot be written or removed.
    pub fn set_wanted(&self, wanted: bool) -> Result<(), String> {
        let path = self.directory.join(MARKER_FILE);
        if wanted {
            std::fs::write(&path, b"on\n")
                .map_err(|error| format!("that could not be saved: {error}"))
        } else {
            match std::fs::remove_file(&path) {
                Ok(()) => Ok(()),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
                Err(error) => Err(format!("that could not be saved: {error}")),
            }
        }
    }

    /// What it is doing.
    #[must_use]
    pub fn status(&self) -> Tunnel {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    fn set(&self, state: Tunnel) {
        *self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = state;
    }

    /// Sets what the tunnel says it is doing, so a test can stand in for a running one.
    #[cfg(test)]
    pub(crate) fn set_for_test(&self, state: Tunnel) {
        self.set(state);
    }

    /// The size of the download this machine would need, when it would need one and can make it.
    #[must_use]
    pub fn download_megabytes(&self) -> Option<u64> {
        if self.installed_program().is_some() {
            None
        } else {
            build_for_this_machine().map(|build| build.megabytes)
        }
    }

    /// Whether this machine can run a tunnel at all (an installed program, or a build to download).
    #[must_use]
    pub fn available(&self) -> bool {
        self.installed_program().is_some() || build_for_this_machine().is_some()
    }

    fn managed_path(&self) -> PathBuf {
        let name = if cfg!(windows) {
            "cloudflared.exe"
        } else {
            "cloudflared"
        };
        self.directory.join("tools").join(name)
    }

    /// A program already on this computer: one the owner installed, or the copy JARVIS downloaded earlier.
    fn installed_program(&self) -> Option<PathBuf> {
        if let Ok(found) = crate::mcp_admin::find_program_on_path("cloudflared") {
            return Some(PathBuf::from(found));
        }
        let managed = self.managed_path();
        managed.is_file().then_some(managed)
    }

    /// Starts the tunnel to `port` in the background. Does nothing when it is already running.
    pub fn start(self: &Arc<Self>, port: u16) {
        let (stop_sender, stop_receiver) = oneshot::channel();
        {
            let mut stop = self
                .stop
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if stop.is_some() {
                return;
            }
            *stop = Some(stop_sender);
        }
        let this = Arc::clone(self);
        tokio::spawn(async move {
            this.supervise(port, stop_receiver).await;
            this.stop
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .take();
        });
    }

    /// Stops the tunnel. The public address stops working at once.
    pub fn stop(&self) {
        if let Some(sender) = self
            .stop
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take()
        {
            let _ = sender.send(());
        }
        self.set(Tunnel::Off);
    }

    async fn supervise(self: &Arc<Self>, port: u16, mut stop: oneshot::Receiver<()>) {
        let program = match self.program(&mut stop).await {
            Ok(program) => program,
            Err(reason) => {
                tracing::warn!(%reason, "the public tunnel could not be prepared");
                self.set(Tunnel::Problem { reason });
                return;
            }
        };
        let mut changes = self.changes.subscribe();
        loop {
            self.set(Tunnel::Starting);
            // Read again on every round, so a tunnel saved or forgotten while this one runs or retries is picked up. Marking the count as seen
            // before reading means a change made after the read is never missed.
            changes.borrow_and_update();
            let named = self.named_snapshot();
            let mut command = tokio::process::Command::new(&program);
            command.args(match &named {
                Some(_) => named_arguments(),
                None => tunnel_arguments(port),
            });
            let ended = self
                .watch(command, &mut stop, named.as_ref(), &mut changes)
                .await;
            match ended {
                Ended::Stopped => {
                    self.set(Tunnel::Off);
                    return;
                }
                Ended::Changed => continue,
                Ended::Exited(reason) => {
                    tracing::warn!(%reason, "the public tunnel stopped; starting it again");
                    let reason = if named.is_some() {
                        format!(
                            "{reason}; check the tunnel token, that the tunnel still exists in your Cloudflare account, and your internet connection"
                        )
                    } else {
                        reason
                    };
                    self.set(Tunnel::Problem { reason });
                }
            }
            tokio::select! {
                () = tokio::time::sleep(RETRY_AFTER) => {}
                _ = changes.changed() => {}
                _ = &mut stop => {
                    self.set(Tunnel::Off);
                    return;
                }
            }
        }
    }

    /// The program to run, downloading the pinned one when nothing is installed.
    async fn program(&self, stop: &mut oneshot::Receiver<()>) -> Result<PathBuf, String> {
        if let Some(program) = self.installed_program() {
            return Ok(program);
        }
        let Some(build) = build_for_this_machine() else {
            return Err("this computer needs Cloudflare's free `cloudflared` program; install it (macOS: `brew install cloudflared`) and switch this on again".to_owned());
        };
        self.set(Tunnel::Getting);
        let target = self.managed_path();
        tokio::select! {
            result = download(build, &target) => result.map(|()| target),
            _ = stop => Err("stopped".to_owned()),
        }
    }

    /// Runs one child, learning the public address from its log, until it exits or is told to stop.
    ///
    /// A quick tunnel prints its address; a named tunnel's address is the owner's hostname, which is reported once the program says it registered
    /// a connection with Cloudflare (so "on" means Cloudflare can reach it, not merely that the program started).
    async fn watch(
        &self,
        mut command: tokio::process::Command,
        stop: &mut oneshot::Receiver<()>,
        named: Option<&Named>,
        changes: &mut watch::Receiver<u64>,
    ) -> Ended {
        command
            .env_clear()
            .envs(jarvis_mcp_transport::server_environment())
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true);
        if let Some(named) = named {
            command.env("TUNNEL_TOKEN", &named.token);
        }
        #[cfg(windows)]
        command.creation_flags(0x0800_0000);
        let mut child = match command.spawn() {
            Ok(child) => child,
            Err(error) => return Ended::Exited(format!("it could not be started: {error}")),
        };
        let Some(stderr) = child.stderr.take() else {
            return Ended::Exited("its log could not be read".to_owned());
        };
        let mut lines = BufReader::new(stderr).lines();
        loop {
            tokio::select! {
                _ = &mut *stop => {
                    let _ = child.kill().await;
                    return Ended::Stopped;
                }
                _ = changes.changed() => {
                    let _ = child.kill().await;
                    return Ended::Changed;
                }
                line = lines.next_line() => {
                    // The log is read for the address and then dropped: it is not kept or logged.
                    if let Ok(Some(line)) = line {
                        let address = match named {
                            Some(named) => registered(&line).then(|| format!("https://{}", named.host)),
                            None => extract_url(&line),
                        };
                        if let Some(url) = address {
                            self.set(Tunnel::On { url });
                        } else if named.is_some()
                            && let Some(reason) = refusal(&line)
                            && !matches!(self.status(), Tunnel::On { .. })
                        {
                            self.set(Tunnel::Problem {
                                reason: format!(
                                    "Cloudflare did not accept the tunnel ({reason}); check the token, and that the tunnel still exists in your Cloudflare account"
                                ),
                            });
                        }
                    } else {
                        let status = child.wait().await;
                        return Ended::Exited(match status {
                            Ok(status) => format!("it stopped ({status})"),
                            Err(error) => format!("it stopped ({error})"),
                        });
                    }
                }
            }
        }
    }
}

/// The arguments of the owner's named tunnel. The token is **not** here: it goes in the environment (`TUNNEL_TOKEN`), where other programs
/// cannot read it from the process list. The origin (where the traffic goes) is set in the Cloudflare dashboard, not here.
fn named_arguments() -> Vec<String> {
    [
        "tunnel",
        "--no-autoupdate",
        "--metrics",
        "127.0.0.1:0",
        "run",
    ]
    .iter()
    .map(|argument| (*argument).to_owned())
    .collect()
}

/// Whether a log line of `cloudflared` says a connection to Cloudflare's network was registered (text from the pinned program).
fn registered(line: &str) -> bool {
    line.contains("Registered tunnel connection")
}

/// What Cloudflare said when it refused to register the tunnel (a wrong or rotated token, a deleted tunnel). The program keeps retrying and never
/// exits, so without this the owner would see "starting" forever. Only Cloudflare's own short reason is kept, never the rest of the line.
fn refusal(line: &str) -> Option<String> {
    if !line.contains("Register tunnel error from server side") {
        return None;
    }
    let reason = line
        .split_once("error=\"")
        .and_then(|(_, rest)| rest.split_once('"'))
        .map(|(reason, _)| reason.trim())
        .filter(|reason| !reason.is_empty() && reason.len() <= 120)
        .unwrap_or("it was refused");
    Some(reason.to_owned())
}

/// The hostname an owner published for the tunnel, cleaned: lowercase, no scheme, no path, a real DNS name that is not an address.
fn clean_hostname(input: &str) -> Result<String, String> {
    let lowered = input.trim().to_ascii_lowercase();
    let host = lowered
        .strip_prefix("https://")
        .unwrap_or(&lowered)
        .trim_end_matches('/');
    let labels: Vec<&str> = host.split('.').collect();
    let usable = host.len() <= 253
        && labels.len() >= 2
        && labels.iter().all(|label| {
            !label.is_empty()
                && label.len() <= 63
                && !label.starts_with('-')
                && !label.ends_with('-')
                && label
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        })
        // An address such as 127.0.0.1 is not a name: the last part of a real hostname is not all digits.
        && labels
            .last()
            .is_some_and(|last| !last.bytes().all(|byte| byte.is_ascii_digit()));
    if usable {
        Ok(host.to_owned())
    } else {
        Err("that is not a hostname: give the full name you published for the tunnel in Cloudflare, such as jarvis.example.com".to_owned())
    }
}

/// The tunnel token out of what an owner pasted: the token itself, or the install command Cloudflare shows (the token is its `eyJâ€¦` word).
/// The token is a base64 text of a small JSON object with the account (`a`), tunnel (`t`) and secret (`s`); anything else is refused so a wrong
/// paste is caught here and not as a tunnel that never connects.
fn clean_token(input: &str) -> Result<String, String> {
    let refused = || {
        "that is not a tunnel token: in Cloudflare copy the install command shown when you create the tunnel (do not run it), or just its long text that starts with eyJ".to_owned()
    };
    if input.len() > NAMED_TOKEN_LIMIT {
        return Err(refused());
    }
    let word = input
        .split_whitespace()
        .rev()
        .find(|word| word.starts_with("eyJ"))
        .ok_or_else(refused)?;
    let token = word.trim_matches(|c: char| matches!(c, '"' | '\''));
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(token)
        .or_else(|_| base64::engine::general_purpose::STANDARD_NO_PAD.decode(token))
        .map_err(|_| refused())?;
    let parts: serde_json::Value = serde_json::from_slice(&decoded).map_err(|_| refused())?;
    let has = |key: &str| {
        parts
            .get(key)
            .and_then(serde_json::Value::as_str)
            .is_some_and(|value| !value.is_empty())
    };
    if has("a") && has("t") && has("s") {
        Ok(token.to_owned())
    } else {
        Err(refused())
    }
}

/// Reads the saved named tunnel. Both files must be usable; a half-saved or damaged pair means no named tunnel (the quick tunnel is used).
fn load_named(directory: &Path) -> Option<Named> {
    let host = std::fs::read_to_string(directory.join(NAMED_HOST_FILE)).ok()?;
    let token = std::fs::read_to_string(directory.join(NAMED_TOKEN_FILE)).ok()?;
    Some(Named {
        host: clean_hostname(&host).ok()?,
        token: clean_token(&token).ok()?,
    })
}

/// The arguments of a quick tunnel to the local endpoint.
///
/// `--http-host-header` makes the endpoint see its own loopback address as the Host, so its rebinding guard is unchanged. `--no-autoupdate` keeps
/// JARVIS in charge of which version runs. `--metrics 127.0.0.1:0` picks a free local port rather than fighting another copy for the default.
fn tunnel_arguments(port: u16) -> Vec<String> {
    let local = format!("127.0.0.1:{port}");
    vec![
        "tunnel".to_owned(),
        "--url".to_owned(),
        format!("http://{local}"),
        "--http-host-header".to_owned(),
        local,
        "--no-autoupdate".to_owned(),
        "--metrics".to_owned(),
        "127.0.0.1:0".to_owned(),
    ]
}

/// Finds the `https://â€¦.trycloudflare.com` address in one line of the program's log.
fn extract_url(line: &str) -> Option<String> {
    let start = line.find("https://")?;
    let candidate: String = line[start..]
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | ':' | '/'))
        .collect();
    let host = candidate.strip_prefix("https://")?;
    let host = host.trim_end_matches('/');
    (host.ends_with(".trycloudflare.com")
        && host.len() > ".trycloudflare.com".len()
        && !host.contains('/'))
    .then(|| format!("https://{host}"))
}

/// The lowercase hexadecimal text of some bytes.
fn hex_of(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    bytes.iter().fold(String::new(), |mut text, byte| {
        let _ = write!(text, "{byte:02x}");
        text
    })
}

/// Whether the bytes are the pinned release.
fn digest_matches(bytes: &[u8], expected: &str) -> bool {
    hex_of(&Sha256::digest(bytes)) == expected
}
async fn download(build: &Build, target: &Path) -> Result<(), String> {
    let address = format!(
        "https://github.com/cloudflare/cloudflared/releases/download/{CLOUDFLARED_VERSION}/{}",
        build.asset
    );
    let client = reqwest::Client::builder()
        .timeout(DOWNLOAD_TIMEOUT)
        .build()
        .map_err(|error| format!("the download could not be set up: {error}"))?;
    let response = client
        .get(&address)
        .send()
        .await
        .and_then(reqwest::Response::error_for_status)
        .map_err(|error| {
            format!("the download failed ({error}); check your internet connection")
        })?;
    let bytes = response.bytes().await.map_err(|error| {
        format!("the download failed ({error}); check your internet connection")
    })?;
    if bytes.len() > DOWNLOAD_LIMIT_BYTES {
        return Err("the download was larger than expected, so it was thrown away".to_owned());
    }
    // Fail closed: a file that is not exactly the pinned release is never written where it could be run.
    if !digest_matches(&bytes, build.sha256) {
        return Err(
            "the download did not match the checksum JARVIS expects, so it was thrown away"
                .to_owned(),
        );
    }
    let directory = target
        .parent()
        .ok_or_else(|| "there is nowhere to put the program".to_owned())?;
    std::fs::create_dir_all(directory)
        .map_err(|error| format!("the program could not be saved: {error}"))?;
    let temporary = target.with_extension("part");
    std::fs::write(&temporary, &bytes)
        .map_err(|error| format!("the program could not be saved: {error}"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&temporary, std::fs::Permissions::from_mode(0o700))
            .map_err(|error| format!("the program could not be made runnable: {error}"))?;
    }
    std::fs::rename(&temporary, target)
        .map_err(|error| format!("the program could not be saved: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_address_is_read_from_a_log_line() {
        let line = "2026-10-10T18:59:14Z INF |  https://resulting-bonds-blogs-technology.trycloudflare.com                                  |";
        assert_eq!(
            extract_url(line).as_deref(),
            Some("https://resulting-bonds-blogs-technology.trycloudflare.com")
        );
    }

    #[test]
    fn only_a_trycloudflare_address_is_taken() {
        for line in [
            "INF Requesting new quick Tunnel on trycloudflare.com...",
            "INF see https://www.cloudflare.com/website-terms/ for the terms",
            "INF https://evil.example/.trycloudflare.com",
            "INF https://.trycloudflare.com",
            "no address here",
        ] {
            assert_eq!(extract_url(line), None, "{line}");
        }
    }

    #[test]
    fn the_endpoint_sees_its_own_loopback_address_as_the_host() {
        let arguments = tunnel_arguments(8766);
        let position = |flag: &str| {
            arguments
                .iter()
                .position(|argument| argument == flag)
                .unwrap_or_else(|| panic!("{flag} is missing"))
        };
        assert_eq!(arguments[position("--url") + 1], "http://127.0.0.1:8766");
        assert_eq!(
            arguments[position("--http-host-header") + 1],
            "127.0.0.1:8766"
        );
        assert!(
            arguments
                .iter()
                .any(|argument| argument == "--no-autoupdate")
        );
    }

    #[test]
    fn every_pinned_digest_is_a_sha256_and_every_asset_is_pinned_to_the_release() {
        for (_, _, build) in BUILDS {
            assert_eq!(build.sha256.len(), 64, "{}", build.asset);
            assert!(
                build
                    .sha256
                    .chars()
                    .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
            );
            assert!(build.asset.starts_with("cloudflared-"));
        }
    }

    /// Falsified by returning `true` unconditionally: a changed file would then be written and run.
    #[test]
    fn a_download_that_is_not_the_pinned_release_is_refused() {
        let real = b"pretend release";
        let digest = hex_of(&Sha256::digest(real));
        assert!(digest_matches(real, &digest));
        assert!(!digest_matches(b"pretend release!", &digest));
        assert!(!digest_matches(real, &"0".repeat(64)));
    }

    #[test]
    fn the_owners_wish_is_a_marker_that_can_be_set_and_cleared() {
        let directory =
            std::env::temp_dir().join(format!("jarvis-tunnel-{}", jarvis_core::scratch_tag()));
        std::fs::create_dir_all(&directory).unwrap_or_else(|e| panic!("{e}"));
        let tunnel = McpTunnel::new(&directory);
        assert!(!tunnel.wanted());
        tunnel.set_wanted(true).unwrap_or_else(|e| panic!("{e}"));
        assert!(tunnel.wanted());
        tunnel.set_wanted(false).unwrap_or_else(|e| panic!("{e}"));
        tunnel
            .set_wanted(false)
            .unwrap_or_else(|e| panic!("clearing twice is fine: {e}"));
        assert!(!tunnel.wanted());
        let _ = std::fs::remove_dir_all(&directory);
    }

    #[tokio::test]
    async fn the_address_is_learned_from_the_program_and_stopping_ends_it() {
        let directory =
            std::env::temp_dir().join(format!("jarvis-tunnel-{}", jarvis_core::scratch_tag()));
        std::fs::create_dir_all(&directory).unwrap_or_else(|e| panic!("{e}"));
        let tunnel = McpTunnel::new(&directory);

        #[cfg(windows)]
        let command = {
            let mut command = tokio::process::Command::new("cmd");
            command.args([
                "/C",
                "echo INF https://fake-test-name.trycloudflare.com 1>&2 & ping -n 4 127.0.0.1 >nul",
            ]);
            command
        };
        #[cfg(not(windows))]
        let command = {
            let mut command = tokio::process::Command::new("sh");
            command.args([
                "-c",
                "echo 'INF https://fake-test-name.trycloudflare.com' 1>&2; exec sleep 30",
            ]);
            command
        };

        let (stop_sender, mut stop_receiver) = oneshot::channel();
        let watcher = Arc::clone(&tunnel);
        let mut changes = tunnel.changes.subscribe();
        let task = tokio::spawn(async move {
            watcher
                .watch(command, &mut stop_receiver, None, &mut changes)
                .await
        });
        let mut seen = None;
        for _ in 0..100 {
            if let Tunnel::On { url } = tunnel.status() {
                seen = Some(url);
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        assert_eq!(
            seen.as_deref(),
            Some("https://fake-test-name.trycloudflare.com")
        );
        let _ = stop_sender.send(());
        let ended = tokio::time::timeout(Duration::from_secs(10), task)
            .await
            .unwrap_or_else(|_| panic!("the child was not stopped"))
            .unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(ended, Ended::Stopped);
        let _ = std::fs::remove_dir_all(&directory);
    }

    fn token_for_test() -> String {
        base64::engine::general_purpose::STANDARD
            .encode(br#"{"a":"account","t":"tunnel-id","s":"c2VjcmV0"}"#)
    }

    fn scratch_directory() -> PathBuf {
        let directory =
            std::env::temp_dir().join(format!("jarvis-tunnel-{}", jarvis_core::scratch_tag()));
        std::fs::create_dir_all(&directory).unwrap_or_else(|e| panic!("{e}"));
        directory
    }

    #[test]
    fn a_hostname_is_cleaned_and_anything_that_is_not_a_name_is_refused() {
        assert_eq!(
            clean_hostname(" HTTPS://Jarvis.Example.com/ ").as_deref(),
            Ok("jarvis.example.com")
        );
        for bad in [
            "",
            "localhost",
            "127.0.0.1",
            "1.2.3.4",
            "*.example.com",
            "jarvis.example.com/mcp",
            "jarvis.example.com:8766",
            "-a.example.com",
            "a..example.com",
            "jarvis example.com",
            "http://jarvis.example.com",
        ] {
            assert!(clean_hostname(bad).is_err(), "{bad:?} must be refused");
        }
    }

    #[test]
    fn a_token_is_taken_bare_or_from_the_pasted_install_command_and_a_wrong_paste_is_refused() {
        let token = token_for_test();
        assert_eq!(clean_token(&token).as_deref(), Ok(token.as_str()));
        assert_eq!(
            clean_token(&format!("  cloudflared.exe service install {token}\n")).as_deref(),
            Ok(token.as_str())
        );
        let missing_secret =
            base64::engine::general_purpose::STANDARD.encode(br#"{"a":"account","t":"tunnel-id"}"#);
        let not_json = base64::engine::general_purpose::STANDARD.encode(b"plain text");
        for bad in [
            String::new(),
            "not a token".to_owned(),
            "eyJhbGciOiJub25lIn0".to_owned(),
            format!("eyJ{missing_secret}"),
            not_json,
            missing_secret,
            format!("eyJ{}", "A".repeat(NAMED_TOKEN_LIMIT)),
        ] {
            let Err(message) = clean_token(&bad) else {
                panic!("a wrong paste must be refused: {bad:?}");
            };
            assert!(!message.contains(&bad) || bad.is_empty());
        }
    }

    #[test]
    fn the_token_is_never_a_command_line_argument() {
        let arguments = named_arguments();
        assert!(arguments.iter().all(|argument| !argument.contains("eyJ")));
        assert!(arguments.iter().all(|argument| argument != "--token"));
        assert_eq!(arguments.last().map(String::as_str), Some("run"));
        assert!(arguments.iter().any(|a| a == "--no-autoupdate"));
    }

    #[test]
    fn a_saved_named_tunnel_survives_a_restart_and_forgetting_it_returns_to_the_quick_one() {
        let directory = scratch_directory();
        let tunnel = McpTunnel::new(&directory);
        assert_eq!(tunnel.stable_host(), None);
        let token = token_for_test();
        let host = tunnel
            .set_named("Jarvis.Example.com", &token)
            .unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(host, "jarvis.example.com");
        assert_eq!(tunnel.stable_host().as_deref(), Some("jarvis.example.com"));

        let again = McpTunnel::new(&directory);
        assert_eq!(again.stable_host().as_deref(), Some("jarvis.example.com"));
        let host_file = std::fs::read_to_string(directory.join(NAMED_HOST_FILE))
            .unwrap_or_else(|e| panic!("{e}"));
        assert!(
            !host_file.contains(&token),
            "the token has a file of its own"
        );

        again.clear_named().unwrap_or_else(|e| panic!("{e}"));
        again
            .clear_named()
            .unwrap_or_else(|e| panic!("forgetting twice is fine: {e}"));
        assert_eq!(again.stable_host(), None);
        assert_eq!(McpTunnel::new(&directory).stable_host(), None);
        assert!(!directory.join(NAMED_TOKEN_FILE).exists());
        let _ = std::fs::remove_dir_all(&directory);
    }

    /// Falsified by loading a host without its token: the tunnel would then run with nothing, and the Host rewrite would trust a name that has no
    /// tunnel behind it.
    #[test]
    fn half_a_named_tunnel_is_no_named_tunnel() {
        let directory = scratch_directory();
        let tunnel = McpTunnel::new(&directory);
        tunnel
            .set_named("jarvis.example.com", &token_for_test())
            .unwrap_or_else(|e| panic!("{e}"));
        std::fs::remove_file(directory.join(NAMED_TOKEN_FILE)).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(McpTunnel::new(&directory).stable_host(), None);

        tunnel
            .set_named("jarvis.example.com", &token_for_test())
            .unwrap_or_else(|e| panic!("{e}"));
        std::fs::write(directory.join(NAMED_TOKEN_FILE), "damaged")
            .unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(McpTunnel::new(&directory).stable_host(), None);
        let _ = std::fs::remove_dir_all(&directory);
    }

    #[test]
    fn a_refused_save_changes_nothing() {
        let directory = scratch_directory();
        let tunnel = McpTunnel::new(&directory);
        tunnel
            .set_named("jarvis.example.com", &token_for_test())
            .unwrap_or_else(|e| panic!("{e}"));
        assert!(tunnel.set_named("127.0.0.1", &token_for_test()).is_err());
        assert!(tunnel.set_named("other.example.com", "garbage").is_err());
        assert_eq!(tunnel.stable_host().as_deref(), Some("jarvis.example.com"));
        let _ = std::fs::remove_dir_all(&directory);
    }

    /// Falsified by comparing with `contains` or `ends_with`, or by accepting any host when none is saved.
    #[test]
    fn only_the_owners_own_hostname_is_treated_as_the_endpoints_own() {
        let directory = scratch_directory();
        let tunnel = McpTunnel::new(&directory);
        assert!(!tunnel.is_public_host("jarvis.example.com"));
        assert!(!tunnel.is_public_host(""));
        tunnel
            .set_named("jarvis.example.com", &token_for_test())
            .unwrap_or_else(|e| panic!("{e}"));
        assert!(tunnel.is_public_host("jarvis.example.com"));
        assert!(tunnel.is_public_host("JARVIS.example.com:443"));
        for other in [
            "",
            "example.com",
            "evil.jarvis.example.com",
            "jarvis.example.com.evil.test",
            "xjarvis.example.com",
            "jarvis.example.com:notaport",
            "127.0.0.1:8766",
            "localhost",
        ] {
            assert!(!tunnel.is_public_host(other), "{other:?}");
        }
        let _ = std::fs::remove_dir_all(&directory);
    }

    #[cfg(windows)]
    fn registering_child() -> tokio::process::Command {
        let mut command = tokio::process::Command::new("cmd");
        command.args([
            "/C",
            "echo INF https://decoy-name.trycloudflare.com 1>&2 & if defined TUNNEL_TOKEN echo INF Registered tunnel connection connIndex=0 1>&2 & ping -n 4 127.0.0.1 >nul",
        ]);
        command
    }

    #[cfg(not(windows))]
    fn registering_child() -> tokio::process::Command {
        let mut command = tokio::process::Command::new("sh");
        command.args([
            "-c",
            "echo 'INF https://decoy-name.trycloudflare.com' 1>&2; [ -n \"$TUNNEL_TOKEN\" ] && echo 'INF Registered tunnel connection connIndex=0' 1>&2; exec sleep 30",
        ]);
        command
    }

    #[tokio::test]
    async fn a_named_tunnel_is_on_at_the_owners_hostname_once_the_program_registers_and_gets_the_token_in_its_environment()
     {
        let directory = scratch_directory();
        let tunnel = McpTunnel::new(&directory);
        tunnel
            .set_named("jarvis.example.com", &token_for_test())
            .unwrap_or_else(|e| panic!("{e}"));
        let named = tunnel.named_snapshot().unwrap_or_else(|| panic!("saved"));
        let (stop_sender, mut stop_receiver) = oneshot::channel();
        let watcher = Arc::clone(&tunnel);
        let mut changes = tunnel.changes.subscribe();
        let task = tokio::spawn(async move {
            watcher
                .watch(
                    registering_child(),
                    &mut stop_receiver,
                    Some(&named),
                    &mut changes,
                )
                .await
        });
        let mut seen = None;
        for _ in 0..100 {
            if let Tunnel::On { url } = tunnel.status() {
                seen = Some(url);
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        // The decoy quick-tunnel address in the log is not taken in named mode; the owner's hostname is.
        assert_eq!(seen.as_deref(), Some("https://jarvis.example.com"));
        let _ = stop_sender.send(());
        let ended = tokio::time::timeout(Duration::from_secs(10), task)
            .await
            .unwrap_or_else(|_| panic!("the child was not stopped"))
            .unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(ended, Ended::Stopped);
        let _ = std::fs::remove_dir_all(&directory);
    }

    #[cfg(windows)]
    fn decoy_child() -> tokio::process::Command {
        let mut command = tokio::process::Command::new("cmd");
        command.args([
            "/C",
            "echo INF https://decoy-name.trycloudflare.com 1>&2 & ping -n 4 127.0.0.1 >nul",
        ]);
        command
    }

    #[cfg(not(windows))]
    fn decoy_child() -> tokio::process::Command {
        let mut command = tokio::process::Command::new("sh");
        command.args([
            "-c",
            "echo 'INF https://decoy-name.trycloudflare.com' 1>&2; exec sleep 30",
        ]);
        command
    }

    #[test]
    fn cloudflares_refusal_is_read_from_the_log_and_nothing_else_is_kept() {
        let line = "2026-10-12T19:26:14Z ERR Register tunnel error from server side error=\"Failed to get tunnel\" connIndex=0 event=0 ip=198.41.192.167";
        assert_eq!(refusal(line).as_deref(), Some("Failed to get tunnel"));
        assert_eq!(
            refusal("ERR Register tunnel error from server side connIndex=0").as_deref(),
            Some("it was refused")
        );
        assert_eq!(
            refusal("INF Registered tunnel connection connIndex=0"),
            None
        );
        assert_eq!(refusal("INF Starting tunnel tunnelID=abc"), None);
        let long = format!(
            "ERR Register tunnel error from server side error=\"{}\"",
            "x".repeat(500)
        );
        assert_eq!(refusal(&long).as_deref(), Some("it was refused"));
    }

    #[cfg(windows)]
    fn refusing_child() -> tokio::process::Command {
        let mut command = tokio::process::Command::new("cmd");
        command.args([
            "/C",
            "echo ERR Register tunnel error from server side connIndex=0 1>&2 & ping -n 4 127.0.0.1 >nul",
        ]);
        command
    }

    #[cfg(not(windows))]
    fn refusing_child() -> tokio::process::Command {
        let mut command = tokio::process::Command::new("sh");
        command.args([
            "-c",
            "echo 'ERR Register tunnel error from server side connIndex=0' 1>&2; exec sleep 30",
        ]);
        command
    }

    /// Without this the owner of a wrong token would see "starting" for ever, because the program retries and never exits.
    #[tokio::test]
    async fn a_named_tunnel_cloudflare_refuses_is_a_problem_with_a_sentence() {
        let directory = scratch_directory();
        let tunnel = McpTunnel::new(&directory);
        tunnel
            .set_named("jarvis.example.com", &token_for_test())
            .unwrap_or_else(|e| panic!("{e}"));
        let named = tunnel.named_snapshot().unwrap_or_else(|| panic!("saved"));
        let (stop_sender, mut stop_receiver) = oneshot::channel();
        let watcher = Arc::clone(&tunnel);
        let mut changes = tunnel.changes.subscribe();
        let task = tokio::spawn(async move {
            watcher
                .watch(
                    refusing_child(),
                    &mut stop_receiver,
                    Some(&named),
                    &mut changes,
                )
                .await
        });
        let mut seen = None;
        for _ in 0..100 {
            if let Tunnel::Problem { reason } = tunnel.status() {
                seen = Some(reason);
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        let reason = seen.unwrap_or_else(|| panic!("no problem was reported"));
        assert!(reason.contains("did not accept the tunnel"), "{reason}");
        assert!(!reason.contains("eyJ"), "the token is never repeated");
        let _ = stop_sender.send(());
        let _ = tokio::time::timeout(Duration::from_secs(10), task).await;
        let _ = std::fs::remove_dir_all(&directory);
    }

    /// Falsified by reporting "on" when the program starts or prints any address: Cloudflare could not yet reach the owner's hostname.
    #[tokio::test]
    async fn a_named_tunnel_that_never_registers_is_never_reported_on() {
        let directory = scratch_directory();
        let tunnel = McpTunnel::new(&directory);
        tunnel
            .set_named("jarvis.example.com", &token_for_test())
            .unwrap_or_else(|e| panic!("{e}"));
        let named = tunnel.named_snapshot().unwrap_or_else(|| panic!("saved"));
        let (stop_sender, mut stop_receiver) = oneshot::channel();
        let watcher = Arc::clone(&tunnel);
        let mut changes = tunnel.changes.subscribe();
        let task = tokio::spawn(async move {
            watcher
                .watch(
                    decoy_child(),
                    &mut stop_receiver,
                    Some(&named),
                    &mut changes,
                )
                .await
        });
        tokio::time::sleep(Duration::from_millis(1200)).await;
        assert!(!matches!(tunnel.status(), Tunnel::On { .. }));
        let _ = stop_sender.send(());
        let _ = tokio::time::timeout(Duration::from_secs(10), task).await;
        let _ = std::fs::remove_dir_all(&directory);
    }

    /// Falsified by not selecting on the change signal: the running child would go on serving the old tunnel.
    #[tokio::test]
    async fn saving_a_new_tunnel_stops_the_running_program_so_it_can_start_with_the_new_one() {
        let directory = scratch_directory();
        let tunnel = McpTunnel::new(&directory);
        let (_stop_sender, mut stop_receiver) = oneshot::channel();
        let watcher = Arc::clone(&tunnel);
        let mut changes = tunnel.changes.subscribe();
        changes.borrow_and_update();
        let task = tokio::spawn(async move {
            watcher
                .watch(decoy_child(), &mut stop_receiver, None, &mut changes)
                .await
        });
        tokio::time::sleep(Duration::from_millis(300)).await;
        tunnel
            .set_named("jarvis.example.com", &token_for_test())
            .unwrap_or_else(|e| panic!("{e}"));
        let ended = tokio::time::timeout(Duration::from_secs(10), task)
            .await
            .unwrap_or_else(|_| panic!("the child was not stopped"))
            .unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(ended, Ended::Changed);
        let _ = std::fs::remove_dir_all(&directory);
    }
}
