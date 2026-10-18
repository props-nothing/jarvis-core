use super::*;
use sqlx::Connection;
use std::{fs::File, io::Write};

async fn database_connection(paths: &AppPaths) -> sqlx::SqliteConnection {
    must(
        sqlx::SqliteConnection::connect_with(
            &sqlx::sqlite::SqliteConnectOptions::new()
                .filename(paths.data().join(jarvis_storage::DEFAULT_DATABASE_FILENAME)),
        )
        .await,
    )
}

fn must<T, E: std::fmt::Display>(value: std::result::Result<T, E>) -> T {
    value.unwrap_or_else(|error| panic!("{error}"))
}

struct Fixture {
    root: PathBuf,
    install: PathBuf,
    paths: AppPaths,
    id: UpdateId,
}
impl Fixture {
    async fn new() -> Self {
        let root = std::env::temp_dir().join(format!("jupdate-{}", jarvis_core::scratch_tag()));
        must(fs::create_dir(&root));
        let root = must(fs::canonicalize(root));
        let install = root.join("install");
        must(fs::create_dir(&install));
        let paths = must(AppPaths::from_root(&root.join("profile")));
        must(paths.prepare());
        let db = must(
            jarvis_storage::SqliteDatabase::test_fixture(
                &paths.data().join(jarvis_storage::DEFAULT_DATABASE_FILENAME),
            )
            .await,
        );
        db.close().await;
        let id = UpdateId::new();
        drop(must(InstallationGuard::acquire(&install, true)));
        must(fs::create_dir(
            install.join(UPDATE_DIRECTORY).join(id.to_string()),
        ));
        Self {
            root,
            install,
            paths,
            id,
        }
    }
    fn stage(&self) -> PathBuf {
        must(stage_dir(&self.install, self.id))
    }
    fn target(&self) -> PathBuf {
        self.install.join(program_name())
    }
    fn programs(&self) {
        let (old, new) = programs();
        must(files::copy_new(old, &self.target()));
        must(files::copy_new(new, &self.stage().join(program_name())));
    }
    async fn activate(&self) -> Result<BinaryUpdate> {
        let guard = InstallationGuard::acquire(&self.install, true)?;
        let offline = OfflineProfile::acquire(&self.paths)?;
        let candidate = image(&self.stage().join(program_name())).await?;
        activate_prepared(
            &self.install,
            &self.paths,
            self.id,
            &guard,
            &offline,
            candidate,
        )
        .await
    }
    async fn settle(&self, rollback: bool) -> Result<BinaryUpdate> {
        let guard = InstallationGuard::acquire(&self.install, true)?;
        let offline = OfflineProfile::acquire(&self.paths)?;
        settle_owned(&self.install, &self.paths, &guard, &offline, rollback).await
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        jarvis_core::remove_scratch_dir(&self.root);
    }
}

// Real OS executables isolate lifecycle failure tests from the cryptographic
// acceptance gate. They are fixtures, not a claim of live signed activation.
fn programs() -> &'static (PathBuf, PathBuf) {
    static PROGRAMS: std::sync::OnceLock<(PathBuf, PathBuf)> = std::sync::OnceLock::new();
    PROGRAMS.get_or_init(|| {
        let directory = std::env::temp_dir().join(format!("jupdate-fixture-{}", jarvis_core::scratch_tag()));
        must(fs::create_dir(&directory));
        let mut result = Vec::new();
        for version in ["0.1.0", "0.2.0"] {
            let mut contract = probe(); contract.version = version.to_owned();
            let json = must(serde_json::to_string(&contract));
            let source = format!("fn main() {{ let a: Vec<_> = std::env::args().collect(); if a.get(1).map(String::as_str) == Some(\"version\") {{ println!(\"jarvis-cli {version}\"); }} else if a.get(1).map(String::as_str) == Some(\"update\") && a.get(2).map(String::as_str) == Some(\"probe\") {{ println!(\"{{}}\", {json:?}); }} else {{ std::process::exit(2); }} }}");
            let input = directory.join("fixture.rs"); must(fs::write(&input, source));
            let output = directory.join(format!("fixture-{}{}", version.replace('.', "_"), std::env::consts::EXE_SUFFIX));
            let mut command = std::process::Command::new("rustc");
            command.arg(&input).arg("-o").arg(&output);
            #[cfg(windows)] { use std::os::windows::process::CommandExt; command.creation_flags(0x0800_0000); }
            assert!(must(command.status()).success());
            result.push(output);
        }
        (result.remove(0), result.remove(0))
    })
}

fn manifest_text() -> String {
    let mut lines = vec![
        "JARVIS-RELEASE-1".to_owned(),
        "repository=props-nothing/jarvis-core".to_owned(),
        format!("tag=ci-{}", "a".repeat(40)),
        format!("commit={}", "a".repeat(40)),
        "version=0.1.0".to_owned(),
    ];
    for platform in manifest::PLATFORMS {
        let ext = if platform.starts_with("windows") {
            "zip"
        } else {
            "tar.gz"
        };
        lines.push(format!(
            "{platform}\tjarvis-0.1.0-{platform}.{ext}\t12\t{}",
            "b".repeat(64)
        ));
    }
    lines.join("\n") + "\n"
}

#[test]
fn manifest_binds_every_platform_release_commit_and_version() {
    let text = manifest_text();
    let tag = format!("ci-{}", "a".repeat(40));
    assert!(manifest::parse(&text, &tag, "0.1.0", "windows-x86_64").is_ok());
    for changed in [
        text.replace("repository=props-nothing", "repository=attacker"),
        text.replace("commit=aaaa", "commit=bbbb"),
        text.replace("version=0.1.0", "version=0.2.0"),
        text.replace("linux-aarch64", "linux-x86_64"),
        text.replace("\t12\t", "\t0\t"),
        text.replace("\t12\t", "\t268435457\t"),
        text.clone() + "extra\n",
    ] {
        assert!(manifest::parse(&changed, &tag, "0.1.0", "windows-x86_64").is_err());
    }
    for tag in ["latest", "../escape", "v0.2.0", "ci-aaaa"] {
        assert!(manifest::selector(tag, "0.1.0").is_err());
    }
}

#[tokio::test]
async fn activation_retains_previous_and_database_snapshot_and_blocks_other_owners() {
    let f = Fixture::new().await;
    f.programs();
    let shared = must(InstallationGuard::acquire(&f.install, false));
    assert!(f.activate().await.is_err());
    drop(shared);
    let busy = must(OfflineProfile::acquire(&f.paths));
    assert!(f.activate().await.is_err());
    drop(busy);
    let before = must(files::fingerprint(
        &f.paths
            .data()
            .join(jarvis_storage::DEFAULT_DATABASE_FILENAME),
    ));
    let record = must(f.activate().await);
    assert_eq!(record.state, BinaryUpdateState::Applied);
    assert_eq!(must(image(&f.target()).await), record.candidate);
    assert_eq!(
        must(files::fingerprint(&f.stage().join("previous"))),
        identity(&record.previous)
    );
    assert_eq!(
        must(files::fingerprint(
            &f.paths
                .data()
                .join(jarvis_storage::DEFAULT_DATABASE_FILENAME)
        )),
        before
    );
    must(jarvis_storage::verify_database_backup(&f.paths, record.backup).await);
    must(jarvis_storage::require_settled_binary_update(&f.install));
}

#[tokio::test]
async fn candidate_change_before_replace_retains_old_executable_and_fails_closed() {
    let f = Fixture::new().await;
    f.programs();
    let guard = must(InstallationGuard::acquire(&f.install, true));
    let offline = must(OfflineProfile::acquire(&f.paths));
    let candidate = must(image(&f.stage().join(program_name())).await);
    must(fs::write(
        f.stage().join(program_name()),
        b"corrupt candidate",
    ));
    assert!(
        activate_prepared(&f.install, &f.paths, f.id, &guard, &offline, candidate)
            .await
            .is_err()
    );
    let record =
        must(jarvis_storage::binary_update_status(&f.install)).unwrap_or_else(|| panic!("journal"));
    assert_eq!(record.state, BinaryUpdateState::RolledBack);
    assert_eq!(must(image(&f.target()).await), record.previous);
}

#[tokio::test]
async fn unknown_installed_bytes_and_malformed_journal_block_startup() {
    let f = Fixture::new().await;
    f.programs();
    must(f.activate().await);
    must(fs::write(f.target(), b"foreign binary"));
    assert!(InstallationGuard::acquire(&f.install, false).is_err());
    must(fs::write(
        f.install.join(UPDATE_DIRECTORY).join("journal.json"),
        b"{}",
    ));
    assert!(InstallationGuard::acquire(&f.install, false).is_err());
}

#[tokio::test]
async fn installation_shared_locks_coexist_but_cannot_mutate() {
    let f = Fixture::new().await;
    let first = must(InstallationGuard::acquire(&f.install, false));
    let second = must(InstallationGuard::acquire(&f.install, false));
    assert!(InstallationGuard::acquire(&f.install, true).is_err());
    drop(first);
    drop(second);
    drop(must(InstallationGuard::acquire(&f.install, true)));
}

#[test]
fn compatibility_fails_closed_on_any_changed_contract() {
    let contract = probe().compatibility;
    assert!(contract.permits(&contract));
    let mut changed = contract.clone();
    changed.database += 1;
    assert!(!contract.permits(&changed));
    changed = contract.clone();
    changed.configuration += 1;
    assert!(!contract.permits(&changed));
    changed = contract.clone();
    changed.protocol += 1;
    assert!(!contract.permits(&changed));
    changed = contract.clone();
    changed.maintenance = 0;
    assert!(!contract.permits(&changed));
}

#[tokio::test]
async fn bad_verifier_pin_refuses_before_process_execution() {
    let f = Fixture::new().await;
    must(fs::write(
        f.stage().join(if cfg!(windows) {
            "cosign.exe"
        } else {
            "cosign"
        }),
        b"not the pinned verifier",
    ));
    let result = release::verify(&f.stage(), &format!("ci-{}", "a".repeat(40)), "0.1.0").await;
    assert!(matches!(
        result,
        Err(Error::Refused("signature verifier checksum mismatch"))
    ));
}

#[tokio::test]
async fn actual_archive_extraction_checks_digest_and_regular_member() {
    let f = Fixture::new().await;
    let platform = must(release::platform());
    let member = format!("jarvis-0.1.0-{platform}/{}", program_name());
    let archive = f.stage().join("archive");
    let suffix;
    if cfg!(windows) {
        suffix = "zip";
        let mut zip = zip::ZipWriter::new(must(File::create(&archive)));
        must(zip.start_file(&member, zip::write::SimpleFileOptions::default()));
        must(zip.write_all(b"valid payload"));
        must(zip.finish());
    } else {
        suffix = "tar.gz";
        let gzip = flate2::write::GzEncoder::new(
            must(File::create(&archive)),
            flate2::Compression::default(),
        );
        let mut tar = tar::Builder::new(gzip);
        let mut header = tar::Header::new_gnu();
        header.set_size(13);
        header.set_mode(0o755);
        header.set_cksum();
        must(tar.append_data(&mut header, &member, &b"valid payload"[..]));
        must(must(tar.into_inner()).finish());
    }
    let (bytes, sha256) = must(files::fingerprint(&archive));
    let mut selected = manifest::Archive {
        name: format!("test.{suffix}"),
        bytes,
        sha256,
    };
    let output = f.stage().join("out");
    must(release::extract(&f.stage(), "0.1.0", &selected, &output));
    assert_eq!(must(fs::read(output)), b"valid payload");
    selected.sha256 = "0".repeat(64);
    assert!(release::extract(&f.stage(), "0.1.0", &selected, &f.stage().join("bad")).is_err());
}

#[tokio::test]
async fn rollback_preserves_data_written_after_activation_and_is_repeatable() {
    let f = Fixture::new().await;
    f.programs();
    let mut db = database_connection(&f.paths).await;
    must(
        sqlx::query("CREATE TABLE update_fixture (value TEXT NOT NULL)")
            .execute(&mut db)
            .await,
    );
    must(
        sqlx::query("INSERT INTO update_fixture VALUES ('before')")
            .execute(&mut db)
            .await,
    );
    must(db.close().await);
    let record = must(f.activate().await);
    let mut db = database_connection(&f.paths).await;
    must(
        sqlx::query("UPDATE update_fixture SET value = 'after'")
            .execute(&mut db)
            .await,
    );
    must(db.close().await);
    for _ in 0..2 {
        assert_eq!(
            must(f.settle(true).await).state,
            BinaryUpdateState::RolledBack
        );
        assert_eq!(must(image(&f.target()).await), record.previous);
        let mut db = database_connection(&f.paths).await;
        let value: String = must(
            sqlx::query_scalar("SELECT value FROM update_fixture")
                .fetch_one(&mut db)
                .await,
        );
        assert_eq!(value, "after");
        must(db.close().await);
    }
}

#[tokio::test]
async fn incompatible_candidate_is_refused_before_snapshot_or_journal() {
    let f = Fixture::new().await;
    f.programs();
    let original = must(files::fingerprint(&f.target()));
    let guard = must(InstallationGuard::acquire(&f.install, true));
    let offline = must(OfflineProfile::acquire(&f.paths));
    let mut candidate = must(image(&f.stage().join(program_name())).await);
    candidate.compatibility.database += 1;
    assert!(
        activate_prepared(&f.install, &f.paths, f.id, &guard, &offline, candidate)
            .await
            .is_err()
    );
    assert_eq!(must(files::fingerprint(&f.target())), original);
    assert!(must(jarvis_storage::binary_update_status(&f.install)).is_none());
    assert!(!f.stage().join("previous").exists());
}

#[tokio::test]
async fn recovery_refuses_changed_database_wrong_profile_and_missing_previous() {
    let f = Fixture::new().await;
    f.programs();
    let mut record = must(f.activate().await);
    let guard = must(InstallationGuard::acquire(&f.install, true));
    record.state = BinaryUpdateState::Activating;
    must(guard.write_update(&record));
    drop(guard);
    let mut db = database_connection(&f.paths).await;
    must(
        sqlx::query("CREATE TABLE changed_during_update (value TEXT)")
            .execute(&mut db)
            .await,
    );
    must(db.close().await);
    assert!(f.settle(false).await.is_err());
    assert!(
        must(jarvis_storage::binary_update_status(&f.install))
            .is_some_and(|r| r.state.is_pending())
    );
    let other = Fixture::new().await;
    assert!(recover(&f.install, &other.paths).await.is_err());
    let g = Fixture::new().await;
    g.programs();
    must(g.activate().await);
    must(fs::write(g.stage().join("previous"), b"tampered previous"));
    assert!(g.settle(true).await.is_err());
}

#[test]
fn crash_transition_child() {
    let Ok(root) = std::env::var("JARVIS_BINARY_TEST_CRASH_ROOT") else {
        return;
    };
    let install = PathBuf::from(root).join("install");
    let guard = must(InstallationGuard::acquire(&install, true));
    let mut record =
        must(jarvis_storage::binary_update_status(&install)).unwrap_or_else(|| panic!("record"));
    let stage = must(stage_dir(&install, record.id));
    record.state = BinaryUpdateState::Activating;
    must(guard.write_update(&record));
    if std::env::var("JARVIS_BINARY_TEST_CRASH_AFTER").as_deref() == Ok("1") {
        must(files::replace(
            &stage.join(program_name()),
            &install.join(program_name()),
            &identity(&record.candidate),
        ));
    }
    std::process::exit(86); // Abrupt termination without Rust destructors.
}

#[tokio::test]
async fn actual_process_death_before_and_after_replacement_is_recoverable() {
    for after in [false, true] {
        let f = Fixture::new().await;
        f.programs();
        let record = must(f.activate().await);
        must(files::replace(
            &f.stage().join("previous"),
            &f.target(),
            &identity(&record.previous),
        ));
        let mut command = std::process::Command::new(must(std::env::current_exe()));
        command
            .args(["--exact", "tests::crash_transition_child", "--nocapture"])
            .env("JARVIS_BINARY_TEST_CRASH_ROOT", &f.root)
            .env(
                "JARVIS_BINARY_TEST_CRASH_AFTER",
                if after { "1" } else { "0" },
            );
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x0800_0000);
        }
        assert_eq!(must(command.status()).code(), Some(86));
        assert!(InstallationGuard::acquire(&f.install, false).is_err());
        let settled = must(f.settle(false).await);
        assert_eq!(
            settled.state,
            if after {
                BinaryUpdateState::Applied
            } else {
                BinaryUpdateState::RolledBack
            }
        );
        drop(must(InstallationGuard::acquire(&f.install, false)));
    }
}

#[cfg(unix)]
#[tokio::test]
async fn linked_installation_and_candidate_are_refused() {
    let f = Fixture::new().await;
    f.programs();
    let link = f.root.join("linked");
    must(std::os::unix::fs::symlink(&f.install, &link));
    assert!(directory(&link).is_err());
    let candidate = f.stage().join(program_name());
    must(fs::remove_file(&candidate));
    must(std::os::unix::fs::symlink(f.target(), &candidate));
    assert!(image(&candidate).await.is_err());
}

#[tokio::test]
async fn service_files_require_exact_ownership_and_path() {
    use jarvis_diagnostics::{ServiceKind, ServicePlan};
    let f = Fixture::new().await;
    let plan = ServicePlan {
        kind: ServiceKind::native(),
        definition_path: f.root.join("service"),
        binary_path: f.target(),
        root: None,
        root_argument: None,
    };
    must(service_plan_preflight(&plan, PathMode::Portable));
    must(fs::write(&plan.definition_path, "foreign service"));
    assert!(service_plan_preflight(&plan, PathMode::Native).is_err());
    must(fs::write(&plan.definition_path, plan.render()));
    must(service_plan_preflight(&plan, PathMode::Native));
    assert!(service_plan_preflight(&plan, PathMode::Portable).is_err());
    let mut other = plan.clone();
    other.binary_path = f.root.join("other-jarvis");
    must(fs::write(&plan.definition_path, other.render()));
    assert!(service_plan_preflight(&plan, PathMode::Native).is_err());
}

#[tokio::test]
async fn source_digest_refusal_preserves_target_bytes() {
    let f = Fixture::new().await;
    let source = f.root.join("source");
    let target = f.root.join("target");
    must(fs::write(&source, b"new"));
    must(fs::write(&target, b"old"));
    assert!(files::replace(&source, &target, &(3, "0".repeat(64))).is_err());
    assert_eq!(must(fs::read(&target)), b"old");
}

#[test]
fn slow_output_child() {
    if std::env::var("JARVIS_BINARY_TEST_SLOW").as_deref() == Ok("1") {
        std::thread::sleep(std::time::Duration::from_secs(30));
    }
}

#[tokio::test]
async fn process_deadline_terminates_a_stuck_health_check() {
    let mut command = Command::new(must(std::env::current_exe()));
    command
        .args(["--exact", "tests::slow_output_child"])
        .env("JARVIS_BINARY_TEST_SLOW", "1");
    assert!(matches!(
        release::output(&mut command, 1).await,
        Err(Error::Refused("verification/executable check timed out"))
    ));
}

#[tokio::test]
async fn linked_and_duplicate_tar_executables_are_never_accepted() {
    for linked in [true, false] {
        let f = Fixture::new().await;
        let member = format!(
            "jarvis-0.1.0-{}/{}",
            must(release::platform()),
            program_name()
        );
        let archive = f.stage().join("archive");
        let gzip = flate2::write::GzEncoder::new(
            must(File::create(&archive)),
            flate2::Compression::default(),
        );
        let mut tar = tar::Builder::new(gzip);
        if linked {
            let mut header = tar::Header::new_gnu();
            header.set_size(0);
            header.set_entry_type(tar::EntryType::Symlink);
            must(header.set_link_name("../../outside"));
            header.set_cksum();
            must(tar.append_data(&mut header, &member, std::io::empty()));
        } else {
            for _ in 0..2 {
                let mut header = tar::Header::new_gnu();
                header.set_size(3);
                header.set_mode(0o755);
                header.set_cksum();
                must(tar.append_data(&mut header, &member, &b"exe"[..]));
            }
        }
        must(must(tar.into_inner()).finish());
        let (bytes, sha256) = must(files::fingerprint(&archive));
        let selected = manifest::Archive {
            name: "fixture.tar.gz".into(),
            bytes,
            sha256,
        };
        assert!(release::extract(&f.stage(), "0.1.0", &selected, &f.stage().join("out")).is_err());
        assert!(!f.root.join("outside").exists());
    }
}
