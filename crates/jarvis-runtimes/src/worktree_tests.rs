//! The worktree helpers against a real throwaway repository.

use std::process::Command as Std;

use super::*;

struct Scratch(PathBuf);

impl Drop for Scratch {
    fn drop(&mut self) {
        // A worktree's files can be read-only on Windows; clearing the flag first lets the folder go.
        fn clear(path: &Path) {
            if let Ok(entries) = std::fs::read_dir(path) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if let Ok(metadata) = entry.metadata() {
                        let mut permissions = metadata.permissions();
                        #[allow(clippy::permissions_set_readonly_false)]
                        permissions.set_readonly(false);
                        let _ = std::fs::set_permissions(&path, permissions);
                        if metadata.is_dir() {
                            clear(&path);
                        }
                    }
                }
            }
        }
        clear(&self.0);
        jarvis_core::remove_scratch_dir(&self.0);
    }
}

fn scratch(name: &str) -> Scratch {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| since.as_nanos());
    let path = std::env::temp_dir().join(format!("jrt-{name}-{unique}"));
    std::fs::create_dir_all(&path).unwrap_or_else(|error| panic!("{error}"));
    Scratch(path.canonicalize().unwrap_or(path))
}

fn run(folder: &Path, arguments: &[&str]) {
    let status = Std::new("git")
        .arg("-C")
        .arg(folder)
        .args([
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@t",
            "-c",
            "commit.gpgsign=false",
        ])
        .args(arguments)
        .output()
        .unwrap_or_else(|error| panic!("git must be installed to test worktrees: {error}"));
    assert!(
        status.status.success(),
        "git {arguments:?}: {}",
        String::from_utf8_lossy(&status.stderr)
    );
}

fn repository(scratch: &Scratch) -> PathBuf {
    let repo = scratch.0.join("repo");
    std::fs::create_dir_all(&repo).unwrap_or_else(|error| panic!("{error}"));
    run(&repo, &["init", "-q", "-b", "main"]);
    std::fs::write(repo.join("a.txt"), "one\n").unwrap_or_else(|error| panic!("{error}"));
    run(&repo, &["add", "-A"]);
    run(&repo, &["commit", "-q", "-m", "first"]);
    repo
}

#[test]
fn a_task_becomes_a_short_safe_name() {
    assert_eq!(slug("Fix the login bug!"), "fix-the-login-bug");
    assert_eq!(slug("  ../../etc/passwd  "), "etc-passwd");
    assert_eq!(slug("!!!"), "task");
    assert!(slug(&"a".repeat(200)).len() <= 30);
}

#[tokio::test]
async fn a_worktree_is_a_separate_checkout_on_its_own_branch_and_its_changes_are_read_back() {
    let scratch = scratch("work");
    let repo = repository(&scratch);
    let repository = open_repository(&repo)
        .await
        .unwrap_or_else(|error| panic!("{error}"));
    assert!(!repository.has_uncommitted_changes());

    let before = snapshot(&repo)
        .await
        .unwrap_or_else(|error| panic!("{error}"));
    let worktree = create(&repository, &scratch.0.join("trees"), "Add two", "ab12cd34")
        .await
        .unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(worktree.branch, "jarvis/add-two-ab12cd34");
    assert!(worktree.path.join("a.txt").is_file());

    // What the agent does: change one file and add another, inside its worktree.
    std::fs::write(worktree.path.join("a.txt"), "one\ntwo\n")
        .unwrap_or_else(|error| panic!("{error}"));
    std::fs::write(worktree.path.join("b.txt"), "new\n").unwrap_or_else(|error| panic!("{error}"));
    let outcome = finish(&worktree, "add two\nsecond line")
        .await
        .unwrap_or_else(|error| panic!("{error}"));
    assert!(outcome.commit.is_some());
    assert_eq!(outcome.files_changed, 2);
    assert!(
        outcome.stat.contains("a.txt") && outcome.stat.contains("b.txt"),
        "{}",
        outcome.stat
    );
    assert!(outcome.patch.contains("+two"));
    assert!(!outcome.patch_cut);

    // Nothing reached the owner's own checkout, and the snapshot says so.
    let after = snapshot(&repo)
        .await
        .unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(before, after);
    assert!(!repo.join("b.txt").exists());
    let message = Std::new("git")
        .arg("-C")
        .arg(&repo)
        .args(["log", "-1", "--format=%s", &worktree.branch])
        .output()
        .unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(
        String::from_utf8_lossy(&message.stdout).trim(),
        "JARVIS: add two"
    );
}

#[tokio::test]
async fn a_task_that_changes_nothing_makes_no_commit() {
    let scratch = scratch("none");
    let repo = repository(&scratch);
    let repository = open_repository(&repo)
        .await
        .unwrap_or_else(|error| panic!("{error}"));
    let worktree = create(&repository, &scratch.0.join("trees"), "nothing", "00000001")
        .await
        .unwrap_or_else(|error| panic!("{error}"));
    let outcome = finish(&worktree, "nothing")
        .await
        .unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(outcome.commit, None);
    assert_eq!(outcome.files_changed, 0);
    assert!(outcome.patch.is_empty());
}

/// A write by the agent outside its worktree, into the owner's checkout, changes the snapshot: that is how it is noticed.
#[tokio::test]
async fn a_write_into_the_original_checkout_is_noticed() {
    let scratch = scratch("escape");
    let repo = repository(&scratch);
    let before = snapshot(&repo)
        .await
        .unwrap_or_else(|error| panic!("{error}"));
    std::fs::write(repo.join("escaped.txt"), "outside the worktree")
        .unwrap_or_else(|error| panic!("{error}"));
    let after = snapshot(&repo)
        .await
        .unwrap_or_else(|error| panic!("{error}"));
    assert_ne!(before, after);
}

#[tokio::test]
async fn only_a_repositorys_own_folder_with_a_commit_is_accepted() {
    let scratch = scratch("shape");
    let repo = repository(&scratch);
    std::fs::create_dir_all(repo.join("sub")).unwrap_or_else(|error| panic!("{error}"));
    assert!(matches!(
        open_repository(&repo.join("sub")).await,
        Err(WorktreeError::NotARepository(_))
    ));
    let plain = scratch.0.join("plain");
    std::fs::create_dir_all(&plain).unwrap_or_else(|error| panic!("{error}"));
    assert!(matches!(
        open_repository(&plain).await,
        Err(WorktreeError::NotARepository(_))
    ));
    let empty = scratch.0.join("empty");
    std::fs::create_dir_all(&empty).unwrap_or_else(|error| panic!("{error}"));
    run(&empty, &["init", "-q", "-b", "main"]);
    assert!(matches!(
        open_repository(&empty).await,
        Err(WorktreeError::NoCommits)
    ));
}

#[tokio::test]
async fn a_task_that_never_started_can_be_taken_back_out() {
    let scratch = scratch("discard");
    let repo = repository(&scratch);
    let repository = open_repository(&repo)
        .await
        .unwrap_or_else(|error| panic!("{error}"));
    let worktree = create(&repository, &scratch.0.join("trees"), "undo", "00000003")
        .await
        .unwrap_or_else(|error| panic!("{error}"));
    assert!(worktree.path.is_dir());
    discard(&repository, &worktree)
        .await
        .unwrap_or_else(|error| panic!("{error}"));
    assert!(!worktree.path.exists());
    let branches = Std::new("git")
        .arg("-C")
        .arg(&repo)
        .args(["branch", "--list", "jarvis/*"])
        .output()
        .unwrap_or_else(|error| panic!("{error}"));
    assert!(String::from_utf8_lossy(&branches.stdout).trim().is_empty());
}

#[tokio::test]
async fn uncommitted_changes_are_said_to_be_left_behind() {
    let scratch = scratch("dirty");
    let repo = repository(&scratch);
    std::fs::write(repo.join("a.txt"), "edited but not committed\n")
        .unwrap_or_else(|error| panic!("{error}"));
    let repository = open_repository(&repo)
        .await
        .unwrap_or_else(|error| panic!("{error}"));
    assert!(repository.has_uncommitted_changes());
}

/// A repository's own hook cannot run through these commands: the agent's commit must not run the repository's `pre-commit`.
#[tokio::test]
async fn a_repositorys_hooks_do_not_run() {
    let scratch = scratch("hooks");
    let repo = repository(&scratch);
    let marker = scratch.0.join("hook-ran");
    let hooks = repo.join(".git").join("hooks");
    std::fs::create_dir_all(&hooks).unwrap_or_else(|error| panic!("{error}"));
    std::fs::write(
        hooks.join("pre-commit"),
        format!(
            "#!/bin/sh\ntouch \"{}\"\nexit 1\n",
            marker.to_string_lossy().replace('\\', "/")
        ),
    )
    .unwrap_or_else(|error| panic!("{error}"));
    let repository = open_repository(&repo)
        .await
        .unwrap_or_else(|error| panic!("{error}"));
    let worktree = create(&repository, &scratch.0.join("trees"), "hooks", "00000002")
        .await
        .unwrap_or_else(|error| panic!("{error}"));
    std::fs::write(worktree.path.join("c.txt"), "c\n").unwrap_or_else(|error| panic!("{error}"));
    let outcome = finish(&worktree, "hooks")
        .await
        .unwrap_or_else(|error| panic!("{error}"));
    assert!(outcome.commit.is_some(), "the commit went through");
    assert!(!marker.exists(), "the hook did not run");
}
