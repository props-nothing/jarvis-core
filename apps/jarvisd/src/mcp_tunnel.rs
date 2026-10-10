//! Making the endpoint for other agents reachable from the internet, with one switch (`ADR-0173`).
//!
//! A cloud agent (a phone agent such as `ElevenLabs`, a hosted assistant) connects **from the provider's servers**, so an endpoint that listens on
//! this computer only cannot be reached by it. The easiest route that needs no account, no router setting and no certificate is a Cloudflare **quick
//! tunnel**: the free `cloudflared` program makes an outbound connection and Cloudflare gives it a public `https://…trycloudflare.com` address that
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
//! uptime guarantee and no support for server-sent events. A stable address needs a named tunnel on the owner's own Cloudflare account, which is not
//! built (`P11-001g`).

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::Serialize;
use sha2::{Digest, Sha256};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::sync::oneshot;

/// The `cloudflared` release JARVIS downloads when none is installed.
pub const CLOUDFLARED_VERSION: &str = "2026.10.0";

/// The file whose presence means the owner wants the endpoint reachable from the internet.
const MARKER_FILE: &str = "mcp-public.on";

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
}

/// How a watched child ended.
#[derive(Debug, Eq, PartialEq)]
enum Ended {
    Stopped,
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
        })
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
        loop {
            self.set(Tunnel::Starting);
            let mut command = tokio::process::Command::new(&program);
            command.args(tunnel_arguments(port));
            let ended = self.watch(command, &mut stop).await;
            match ended {
                Ended::Stopped => {
                    self.set(Tunnel::Off);
                    return;
                }
                Ended::Exited(reason) => {
                    tracing::warn!(%reason, "the public tunnel stopped; starting it again");
                    self.set(Tunnel::Problem { reason });
                }
            }
            tokio::select! {
                () = tokio::time::sleep(RETRY_AFTER) => {}
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
    async fn watch(
        &self,
        mut command: tokio::process::Command,
        stop: &mut oneshot::Receiver<()>,
    ) -> Ended {
        command
            .env_clear()
            .envs(jarvis_mcp_transport::server_environment())
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true);
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
                line = lines.next_line() => {
                    // The log is read for the address and then dropped: it is not kept or logged.
                    if let Ok(Some(line)) = line {
                        if let Some(url) = extract_url(&line) {
                            self.set(Tunnel::On { url });
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

/// Finds the `https://….trycloudflare.com` address in one line of the program's log.
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
        let task = tokio::spawn(async move { watcher.watch(command, &mut stop_receiver).await });
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
}
