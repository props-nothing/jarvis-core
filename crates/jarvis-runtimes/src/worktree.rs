//! A git worktree on a new branch for a coding agent to work in, and what it left behind.
//!
//! A worktree is the unit of containment for delegated coding (`ADR-0207`): the agent starts in a folder that is a separate checkout of
//! the repository on its own branch, so what it changes can be read as a diff and thrown away, and the owner's own checkout is not the
//! place it works in. It is **not** an operating-system sandbox: an agent's own tools can still reach other folders, which is why the
//! caller takes a [`snapshot`] of the original repository before and after and reports a difference.
//!
//! Nothing here merges, pushes or deletes a branch. Git is run as `git -C <folder>` with arguments as an argument vector (never a
//! shell), with hooks and `core.fsmonitor` switched off for every call, so a repository's own configuration cannot make these commands
//! run its programs, and with terminal prompts disabled.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use thiserror::Error;
use tokio::io::AsyncReadExt;
use tokio::process::Command;

/// The longest patch kept, in bytes.
pub const MAX_PATCH_BYTES: usize = 24 * 1024;
/// The longest `--stat` kept, in bytes.
const MAX_STAT_BYTES: usize = 4 * 1024;
/// How long one git command may run.
const GIT_SECONDS: u64 = 60;

/// Why a worktree could not be made or read.
#[derive(Debug, Error)]
pub enum WorktreeError {
    /// `git` could not be started.
    #[error("git is not available: install git and make sure it is on the search path")]
    GitMissing,
    /// The folder is not the top level of a git repository.
    #[error("{0}")]
    NotARepository(String),
    /// The repository has no commit to branch from.
    #[error("the repository has no commits yet, so there is nothing to branch from")]
    NoCommits,
    /// A git command failed.
    #[error("git {what} failed: {detail}")]
    Git {
        /// What was being done.
        what: &'static str,
        /// The first line git wrote, bounded.
        detail: String,
    },
}

/// A repository, checked to be a top-level checkout.
#[derive(Clone, Debug)]
pub struct Repository {
    top: PathBuf,
    head: String,
    uncommitted: bool,
}

impl Repository {
    /// The repository's top-level folder.
    #[must_use]
    pub fn top(&self) -> &Path {
        &self.top
    }

    /// The commit the new branch starts from.
    #[must_use]
    pub fn head(&self) -> &str {
        &self.head
    }

    /// Whether the checkout has changes that are not committed. They are **not** in the worktree, which starts from the commit.
    #[must_use]
    pub const fn has_uncommitted_changes(&self) -> bool {
        self.uncommitted
    }
}

/// A worktree made for one delegated task.
#[derive(Clone, Debug)]
pub struct Worktree {
    /// The folder the agent works in.
    pub path: PathBuf,
    /// The branch it is on.
    pub branch: String,
    /// The commit the branch started from.
    pub base: String,
}

/// What the agent left in a worktree.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Outcome {
    /// The commit made on the branch for the agent's changes, or `None` when it changed nothing.
    pub commit: Option<String>,
    /// How many files changed.
    pub files_changed: u32,
    /// `git diff --stat` against the starting commit, bounded.
    pub stat: String,
    /// The patch against the starting commit, bounded to [`MAX_PATCH_BYTES`].
    pub patch: String,
    /// Whether the patch was cut.
    pub patch_cut: bool,
}

/// The state of a checkout, to compare before and after.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Snapshot(String);

fn null_device() -> &'static str {
    if cfg!(windows) { "NUL" } else { "/dev/null" }
}

/// Runs git in `folder`, reading at most `limit` bytes of its output. Returns the output and whether it was cut.
async fn git(
    folder: &Path,
    arguments: &[&str],
    limit: usize,
    what: &'static str,
) -> Result<(String, bool), WorktreeError> {
    let hooks = format!("core.hooksPath={}", null_device());
    let mut command = Command::new("git");
    command
        .arg("-C")
        .arg(folder)
        .args(["-c", "core.fsmonitor=false", "-c", &hooks])
        .args(arguments)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_OPTIONAL_LOCKS", "0")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    #[cfg(windows)]
    command.creation_flags(0x0800_0000);
    let mut child = command.spawn().map_err(|_| WorktreeError::GitMissing)?;
    let mut stdout = child.stdout.take();
    let mut stderr = child.stderr.take();
    let work = async {
        let mut output = Vec::new();
        let mut cut = false;
        if let Some(stream) = stdout.as_mut() {
            stream
                .take(limit as u64 + 1)
                .read_to_end(&mut output)
                .await
                .ok();
            if output.len() > limit {
                output.truncate(limit);
                cut = true;
            }
        }
        let mut errors = Vec::new();
        if let Some(stream) = stderr.as_mut() {
            stream.take(2048).read_to_end(&mut errors).await.ok();
        }
        if cut {
            let _ = child.start_kill();
        }
        let status = child.wait().await.ok();
        (output, errors, cut, status)
    };
    let Ok((output, errors, cut, status)) =
        tokio::time::timeout(Duration::from_secs(GIT_SECONDS), work).await
    else {
        return Err(WorktreeError::Git {
            what,
            detail: "it took too long".to_owned(),
        });
    };
    if !cut && !status.is_some_and(|status| status.success()) {
        let text = String::from_utf8_lossy(&errors);
        // The reason is the line git marks as fatal or an error; the progress line before it ("Preparing worktree") is not.
        let detail = text
            .lines()
            .find(|line| line.starts_with("fatal") || line.starts_with("error"))
            .or_else(|| text.lines().last())
            .unwrap_or("it reported no reason")
            .chars()
            .take(300)
            .collect();
        return Err(WorktreeError::Git { what, detail });
    }
    Ok((String::from_utf8_lossy(&output).into_owned(), cut))
}

async fn git_line(
    folder: &Path,
    arguments: &[&str],
    what: &'static str,
) -> Result<String, WorktreeError> {
    let (output, _) = git(folder, arguments, 4096, what).await?;
    Ok(output.trim().to_owned())
}

/// Checks that `folder` is the top level of a git repository with at least one commit.
///
/// # Errors
///
/// Returns [`WorktreeError`] when git is missing, the folder is not a repository's top level, or there is no commit.
pub async fn open_repository(folder: &Path) -> Result<Repository, WorktreeError> {
    let top = git_line(folder, &["rev-parse", "--show-toplevel"], "rev-parse")
        .await
        .map_err(|error| match error {
            WorktreeError::Git { .. } => {
                WorktreeError::NotARepository("that folder is not a git repository".to_owned())
            }
            other => other,
        })?;
    // Compared on canonical paths: git writes forward slashes on Windows and the folder may be spelled differently.
    let same = match (std::fs::canonicalize(&top), std::fs::canonicalize(folder)) {
        (Ok(top), Ok(folder)) => top == folder,
        _ => false,
    };
    if !same {
        return Err(WorktreeError::NotARepository(
            "that folder is inside a git repository but is not its top level; name the repository's own folder".to_owned(),
        ));
    }
    let head = git_line(folder, &["rev-parse", "--verify", "HEAD"], "rev-parse")
        .await
        .map_err(|_| WorktreeError::NoCommits)?;
    let status = git_line(folder, &["status", "--porcelain"], "status").await?;
    Ok(Repository {
        top: PathBuf::from(folder),
        head,
        uncommitted: !status.is_empty(),
    })
}

/// A short, safe name for a branch and a folder: lower-case letters, digits and hyphens.
#[must_use]
pub fn slug(text: &str) -> String {
    let mut slug = String::new();
    for character in text.chars() {
        if character.is_ascii_alphanumeric() {
            slug.push(character.to_ascii_lowercase());
        } else if !slug.ends_with('-') && !slug.is_empty() {
            slug.push('-');
        }
        if slug.len() >= 30 {
            break;
        }
    }
    let slug = slug.trim_matches('-').to_owned();
    if slug.is_empty() {
        "task".to_owned()
    } else {
        slug
    }
}

/// Strips the `\\?\` prefix Windows puts on a canonical path, which git does not understand.
fn plain(path: &Path) -> PathBuf {
    let text = path.to_string_lossy();
    match text.strip_prefix(r"\\?\") {
        Some(rest) if !rest.starts_with("UNC\\") => PathBuf::from(rest),
        _ => path.to_path_buf(),
    }
}

/// Makes a worktree for `repository` under `parent`, on a new branch `jarvis/<slug>-<id>` from the repository's current commit.
///
/// # Errors
///
/// Returns [`WorktreeError`] when git fails (for example the branch exists).
pub async fn create(
    repository: &Repository,
    parent: &Path,
    task: &str,
    id: &str,
) -> Result<Worktree, WorktreeError> {
    let name = format!("{}-{}", slug(task), id);
    let branch = format!("jarvis/{name}");
    let parent = plain(parent);
    std::fs::create_dir_all(&parent).map_err(|error| WorktreeError::Git {
        what: "worktree add",
        detail: format!("the worktree folder could not be made: {}", error.kind()),
    })?;
    let path = parent.join(&name);
    let location = path.to_string_lossy().into_owned();
    git(
        &repository.top,
        &[
            "worktree",
            "add",
            "-b",
            &branch,
            &location,
            &repository.head,
        ],
        4096,
        "worktree add",
    )
    .await?;
    Ok(Worktree {
        path,
        branch,
        base: repository.head.clone(),
    })
}

/// Removes a worktree and its branch, for a task that never started. A worktree with an agent's work in it is never removed here.
///
/// # Errors
///
/// Returns [`WorktreeError`] when git fails.
pub async fn discard(repository: &Repository, worktree: &Worktree) -> Result<(), WorktreeError> {
    let location = worktree.path.to_string_lossy().into_owned();
    git(
        &repository.top,
        &["worktree", "remove", "--force", &location],
        4096,
        "worktree remove",
    )
    .await?;
    git(
        &repository.top,
        &["branch", "-D", &worktree.branch],
        4096,
        "branch delete",
    )
    .await?;
    Ok(())
}

/// The state of a checkout: its commit and what `git status` says. A change between two snapshots means something wrote there.
///
/// # Errors
///
/// Returns [`WorktreeError`] when git fails.
pub async fn snapshot(folder: &Path) -> Result<Snapshot, WorktreeError> {
    let head = git_line(folder, &["rev-parse", "HEAD"], "rev-parse").await?;
    let (status, _) = git(folder, &["status", "--porcelain"], 64 * 1024, "status").await?;
    Ok(Snapshot(format!("{head}\n{status}")))
}

/// Commits what the agent changed in its worktree (on its own branch, with hooks off) and reads the result against the starting commit.
///
/// # Errors
///
/// Returns [`WorktreeError`] when git fails.
pub async fn finish(worktree: &Worktree, message: &str) -> Result<Outcome, WorktreeError> {
    let folder = &worktree.path;
    git(folder, &["add", "-A"], 4096, "add").await?;
    let pending = git_line(folder, &["status", "--porcelain"], "status").await?;
    let commit = if pending.is_empty() {
        None
    } else {
        let message: String = message
            .lines()
            .next()
            .unwrap_or("JARVIS")
            .chars()
            .take(200)
            .collect();
        git(
            folder,
            &[
                "-c",
                "user.name=JARVIS",
                "-c",
                "user.email=jarvis@localhost",
                "-c",
                "commit.gpgsign=false",
                "commit",
                "--no-verify",
                "-q",
                "-m",
                &format!("JARVIS: {message}"),
            ],
            4096,
            "commit",
        )
        .await?;
        Some(git_line(folder, &["rev-parse", "HEAD"], "rev-parse").await?)
    };
    let range = format!("{}..HEAD", worktree.base);
    let (stat, _) = git(folder, &["diff", "--stat", &range], MAX_STAT_BYTES, "diff").await?;
    let (numstat, _) = git(folder, &["diff", "--numstat", &range], 64 * 1024, "diff").await?;
    let (patch, patch_cut) = git(
        folder,
        &["diff", "--no-color", &range],
        MAX_PATCH_BYTES,
        "diff",
    )
    .await?;
    Ok(Outcome {
        commit,
        files_changed: u32::try_from(numstat.lines().count()).unwrap_or(u32::MAX),
        stat: stat.trim().to_owned(),
        patch,
        patch_cut,
    })
}

/// Reads an isolated checkout without staging or committing the owner's files.
///
/// # Errors
///
/// Returns an error when Git cannot inspect the checkout.
pub async fn review(worktree: &Worktree) -> Result<Outcome, WorktreeError> {
    let (stat, _) = git(
        &worktree.path,
        &["diff", "--stat", &worktree.base],
        MAX_STAT_BYTES,
        "diff",
    )
    .await?;
    let (patch, patch_cut) = git(
        &worktree.path,
        &["diff", "--no-color", &worktree.base],
        MAX_PATCH_BYTES,
        "diff",
    )
    .await?;
    let (status, _) = git(
        &worktree.path,
        &["status", "--porcelain"],
        64 * 1024,
        "status",
    )
    .await?;
    Ok(Outcome {
        commit: None,
        files_changed: u32::try_from(status.lines().count()).unwrap_or(u32::MAX),
        stat: format!("{stat}\n{status}"),
        patch,
        patch_cut,
    })
}

#[cfg(test)]
#[path = "worktree_tests.rs"]
mod tests;
