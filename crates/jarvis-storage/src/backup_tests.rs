use super::*;
use crate::{SqliteDatabase, portable_layout};

fn must<T, E: std::fmt::Display>(result: Result<T, E>) -> T {
    result.unwrap_or_else(|error| panic!("{error}"))
}

struct Fixture {
    root: PathBuf,
    paths: AppPaths,
}

impl Fixture {
    async fn new() -> Self {
        let root = std::env::temp_dir().join(format!("jbackup-{}", jarvis_core::scratch_tag()));
        must(fs::create_dir(&root));
        let root = must(fs::canonicalize(root));
        let paths = portable_layout(&root).unwrap_or_else(|| panic!("absolute root"));
        must(paths.prepare());
        let database = must(SqliteDatabase::test_fixture(&database_path(&paths)).await);
        must(
            sqlx::query("CREATE TABLE backup_fixture (value TEXT NOT NULL)")
                .execute(database.pool())
                .await,
        );
        must(
            sqlx::query("INSERT INTO backup_fixture VALUES ('before')")
                .execute(database.pool())
                .await,
        );
        database.close().await;
        Self { root, paths }
    }

    async fn value(&self) -> String {
        let database = must(SqliteDatabase::open_default(&self.paths).await);
        let value = must(
            sqlx::query_scalar("SELECT value FROM backup_fixture")
                .fetch_one(database.pool())
                .await,
        );
        database.close().await;
        value
    }

    async fn change(&self, value: &str) {
        let database = must(SqliteDatabase::open_default(&self.paths).await);
        must(
            sqlx::query("UPDATE backup_fixture SET value = ?")
                .bind(value)
                .execute(database.pool())
                .await,
        );
        database.close().await;
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        jarvis_core::remove_scratch_dir(&self.root);
    }
}

#[tokio::test]
async fn restore_recovers_real_values_and_retains_verified_undo_without_touching_config() {
    let fixture = Fixture::new().await;
    let config = fixture.paths.config().join("owner-setting");
    must(fs::write(&config, b"unchanged"));
    let original = must(fingerprint(&database_path(&fixture.paths)));
    let snapshot = must(create_database_backup(&fixture.paths).await);
    assert_eq!(
        must(verify_database_backup(&fixture.paths, snapshot.id).await),
        snapshot
    );
    assert_eq!(must(fingerprint(&database_path(&fixture.paths))), original);
    must(no_sidecars(&database_path(&fixture.paths)));
    fixture.change("after").await;
    let restored = must(restore_database_backup(&fixture.paths, snapshot.id).await);
    assert_eq!(restored.state, RestoreState::Applied);
    assert_eq!(fixture.value().await, "before");
    must(require_settled_database_restore(&fixture.paths));
    must(restore_database_backup(&fixture.paths, restored.previous).await);
    assert_eq!(fixture.value().await, "after");
    assert_eq!(must(fs::read(config)), b"unchanged");
}

#[tokio::test]
async fn actual_daemon_lock_excludes_backup_restore_and_recovery_until_released() {
    let fixture = Fixture::new().await;
    let snapshot = must(create_database_backup(&fixture.paths).await);
    // The daemon opens and locks exactly this file. Keep its bytes intact too.
    let lock = fixture.paths.runtime().join(DAEMON_LOCK_FILE_NAME);
    must(fs::write(&lock, b"daemon metadata"));
    let owner = must(OpenOptions::new().read(true).write(true).open(&lock));
    must(owner.try_lock());
    assert!(matches!(
        create_database_backup(&fixture.paths).await,
        Err(BackupError::InUse)
    ));
    assert!(matches!(
        restore_database_backup(&fixture.paths, snapshot.id).await,
        Err(BackupError::InUse)
    ));
    assert!(matches!(
        recover_database_restore(&fixture.paths).await,
        Err(BackupError::InUse)
    ));
    drop(owner);
    assert_eq!(must(fs::read(&lock)), b"daemon metadata");
    must(create_database_backup(&fixture.paths).await);
}

#[tokio::test]
async fn tampered_snapshot_and_malformed_manifest_cannot_replace_the_database() {
    let fixture = Fixture::new().await;
    let snapshot = must(create_database_backup(&fixture.paths).await);
    let directory = must(backup_directory(&fixture.paths, snapshot.id));
    let original = must(fingerprint(&database_path(&fixture.paths)));
    must(fs::write(
        directory.join(DEFAULT_DATABASE_FILENAME),
        b"corrupt",
    ));
    assert!(matches!(
        restore_database_backup(&fixture.paths, snapshot.id).await,
        Err(BackupError::DigestMismatch)
    ));
    must(fs::write(directory.join("manifest.json"), vec![b' '; 4097]));
    assert!(matches!(
        verify_database_backup(&fixture.paths, snapshot.id).await,
        Err(BackupError::InvalidRecord)
    ));
    assert_eq!(must(fingerprint(&database_path(&fixture.paths))), original);
    assert!(must(journal(&fixture.paths)).is_none());
}

#[tokio::test]
async fn sidecars_are_never_deleted_or_ignored_to_make_restore_pass() {
    let fixture = Fixture::new().await;
    let snapshot = must(create_database_backup(&fixture.paths).await);
    let original = must(fingerprint(&database_path(&fixture.paths)));
    for suffix in ["-wal", "-shm", "-journal"] {
        let path = fixture
            .paths
            .data()
            .join(format!("{DEFAULT_DATABASE_FILENAME}{suffix}"));
        must(fs::write(&path, b"must survive"));
        assert!(matches!(
            create_database_backup(&fixture.paths).await,
            Err(BackupError::Sidecars)
        ));
        assert!(matches!(
            restore_database_backup(&fixture.paths, snapshot.id).await,
            Err(BackupError::Sidecars)
        ));
        assert_eq!(must(fs::read(&path)), b"must survive");
        assert_eq!(must(fingerprint(&database_path(&fixture.paths))), original);
        must(fs::remove_file(path)); // Only our synthetic fixture, never production cleanup.
    }
}

#[tokio::test]
async fn corrupt_foreign_future_and_mismatched_schema_markers_fail_closed() {
    use sqlx::{ConnectOptions, Connection};
    for damage in [
        "PRAGMA application_id = 42",
        "PRAGMA user_version = 999",
        "PRAGMA user_version = 1",
    ] {
        let fixture = Fixture::new().await;
        let snapshot = must(create_database_backup(&fixture.paths).await);
        let mut connection = must(
            sqlx::sqlite::SqliteConnectOptions::new()
                .filename(database_path(&fixture.paths))
                .connect()
                .await,
        );
        must(sqlx::query(damage).execute(&mut connection).await);
        must(connection.close().await);
        let original = must(fingerprint(&database_path(&fixture.paths)));
        assert!(
            matches!(
                create_database_backup(&fixture.paths).await,
                Err(BackupError::Incompatible)
            ),
            "{damage}"
        );
        assert!(
            matches!(
                restore_database_backup(&fixture.paths, snapshot.id).await,
                Err(BackupError::Incompatible)
            ),
            "{damage}"
        );
        assert_eq!(must(fingerprint(&database_path(&fixture.paths))), original);
    }
    let fixture = Fixture::new().await;
    must(fs::write(database_path(&fixture.paths), b"not SQLite"));
    assert!(matches!(
        create_database_backup(&fixture.paths).await,
        Err(BackupError::Incompatible)
    ));
}

#[tokio::test]
async fn a_matching_digest_does_not_authorize_foreign_schema_or_broken_foreign_keys() {
    use sqlx::{ConnectOptions, Connection};
    for damage in [
        "PRAGMA application_id = 42",
        "PRAGMA user_version = 999",
        "CREATE TABLE broken_fk (id INTEGER REFERENCES absent_table(id)); INSERT INTO broken_fk VALUES (1)",
    ] {
        let fixture = Fixture::new().await;
        let mut snapshot = must(create_database_backup(&fixture.paths).await);
        let directory = must(backup_directory(&fixture.paths, snapshot.id));
        let file = directory.join(DEFAULT_DATABASE_FILENAME);
        let mut connection = must(
            sqlx::sqlite::SqliteConnectOptions::new()
                .filename(&file)
                .foreign_keys(false)
                .connect()
                .await,
        );
        must(sqlx::raw_sql(damage).execute(&mut connection).await);
        must(connection.close().await);
        (snapshot.bytes, snapshot.sha256) = must(fingerprint(&file));
        must(write_record(&directory.join("manifest.json"), &snapshot));
        let original = must(fingerprint(&database_path(&fixture.paths)));
        assert!(
            matches!(
                restore_database_backup(&fixture.paths, snapshot.id).await,
                Err(BackupError::Incompatible)
            ),
            "{damage}"
        );
        assert_eq!(must(fingerprint(&database_path(&fixture.paths))), original);
        assert!(must(journal(&fixture.paths)).is_none());
    }
}

// Compiled only into this unit-test binary. No fault-injection environment
// variables or process exits exist in the distributed storage implementation.
pub(super) fn crash_at(boundary: &str) {
    if std::env::var("JARVIS_BACKUP_TEST_CRASH").as_deref() == Ok(boundary) {
        std::process::exit(86); // Abrupt termination: no Rust destructors run.
    }
}

#[tokio::test]
async fn restore_crash_child() {
    let Some(root) = std::env::var_os("JARVIS_BACKUP_TEST_ROOT") else {
        return;
    };
    let paths = portable_layout(Path::new(&root)).unwrap_or_else(|| panic!("absolute root"));
    let selected = must(must(std::env::var("JARVIS_BACKUP_TEST_ID")).parse());
    must(restore_database_backup(&paths, selected).await);
    panic!("the requested crash boundary was not reached");
}

#[tokio::test]
async fn process_death_on_either_side_of_replacement_preserves_recoverable_evidence() {
    use std::{
        process::{Command, Stdio},
        time::{Duration, Instant},
    };
    for boundary in ["pending", "replacement"] {
        let fixture = Fixture::new().await;
        let snapshot = must(create_database_backup(&fixture.paths).await);
        fixture.change("after").await;
        let mut command = Command::new(must(std::env::current_exe()));
        command
            .args([
                "--exact",
                "backup::tests::restore_crash_child",
                "--nocapture",
            ])
            .env("JARVIS_BACKUP_TEST_ROOT", &fixture.root)
            .env("JARVIS_BACKUP_TEST_ID", snapshot.id.to_string())
            .env("JARVIS_BACKUP_TEST_CRASH", boundary)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
        }
        let mut child = must(command.spawn());
        let deadline = Instant::now() + Duration::from_secs(60);
        let status = loop {
            if let Some(status) = must(child.try_wait()) {
                break status;
            }
            if Instant::now() >= deadline {
                let _ = child.kill();
                let _ = child.wait();
                panic!("restore child did not terminate at {boundary}");
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        };
        assert_eq!(
            status.code(),
            Some(86),
            "child must die at the real restore boundary"
        );
        assert!(matches!(
            require_settled_database_restore(&fixture.paths),
            Err(BackupError::Pending)
        ));
        let original = must(fingerprint(&database_path(&fixture.paths)));
        let recovered = must(recover_database_restore(&fixture.paths).await)
            .unwrap_or_else(|| panic!("journal"));
        assert_eq!(
            recovered.state,
            if boundary == "pending" {
                RestoreState::NotApplied
            } else {
                RestoreState::Applied
            }
        );
        assert_eq!(must(fingerprint(&database_path(&fixture.paths))), original);
        assert_eq!(
            fixture.value().await,
            if boundary == "pending" {
                "after"
            } else {
                "before"
            }
        );
        must(verify_database_backup(&fixture.paths, recovered.previous).await);
    }
}

async fn pending_fixture() -> (Fixture, DatabaseRestore) {
    let fixture = Fixture::new().await;
    let selected = must(create_database_backup(&fixture.paths).await);
    fixture.change("after").await;
    let previous = must(create_database_backup(&fixture.paths).await);
    let record = DatabaseRestore {
        format: FORMAT,
        selected: selected.id,
        previous: previous.id,
        before_sha256: previous.sha256,
        after_sha256: selected.sha256,
        state: RestoreState::Pending,
    };
    must(write_record(&fixture.paths.state().join(JOURNAL), &record));
    (fixture, record)
}

#[tokio::test]
async fn pending_restore_blocks_start_and_new_operations_then_recovers_before_or_after_replacement()
{
    for installed in [false, true] {
        let (fixture, pending) = pending_fixture().await;
        if installed {
            let source = must(backup_directory(&fixture.paths, pending.selected))
                .join(DEFAULT_DATABASE_FILENAME);
            must(fs::copy(source, database_path(&fixture.paths)));
        }
        assert!(matches!(
            require_settled_database_restore(&fixture.paths),
            Err(BackupError::Pending)
        ));
        assert!(matches!(
            create_database_backup(&fixture.paths).await,
            Err(BackupError::Pending)
        ));
        assert!(matches!(
            restore_database_backup(&fixture.paths, pending.selected).await,
            Err(BackupError::Pending)
        ));
        let original = must(fingerprint(&database_path(&fixture.paths)));
        let recovered = must(recover_database_restore(&fixture.paths).await)
            .unwrap_or_else(|| panic!("record"));
        assert_eq!(
            recovered.state,
            if installed {
                RestoreState::Applied
            } else {
                RestoreState::NotApplied
            }
        );
        assert_eq!(
            must(fingerprint(&database_path(&fixture.paths))),
            original,
            "recovery only classifies; it never replaces bytes"
        );
        must(require_settled_database_restore(&fixture.paths));
        assert_eq!(
            must(recover_database_restore(&fixture.paths).await),
            Some(recovered)
        );
    }
}

#[tokio::test]
async fn recovery_does_not_guess_when_neither_image_matches_or_evidence_is_missing() {
    let (fixture, pending) = pending_fixture().await;
    fixture.change("unknown").await;
    let original = must(fingerprint(&database_path(&fixture.paths)));
    assert!(matches!(
        recover_database_restore(&fixture.paths).await,
        Err(BackupError::Ambiguous)
    ));
    assert_eq!(must(fingerprint(&database_path(&fixture.paths))), original);
    assert_eq!(must(journal(&fixture.paths)), Some(pending));
    must(fs::write(fixture.paths.state().join(JOURNAL), b"{}"));
    assert!(matches!(
        require_settled_database_restore(&fixture.paths),
        Err(BackupError::InvalidRecord)
    ));
}

#[cfg(unix)]
#[tokio::test]
async fn linked_snapshots_and_lock_files_are_refused() {
    use std::os::unix::fs::symlink;
    let fixture = Fixture::new().await;
    let snapshot = must(create_database_backup(&fixture.paths).await);
    let file = must(backup_directory(&fixture.paths, snapshot.id)).join(DEFAULT_DATABASE_FILENAME);
    must(fs::remove_file(&file));
    must(symlink(database_path(&fixture.paths), &file));
    assert!(matches!(
        verify_database_backup(&fixture.paths, snapshot.id).await,
        Err(BackupError::InvalidPath)
    ));
    let lock = fixture.paths.runtime().join(DAEMON_LOCK_FILE_NAME);
    must(fs::remove_file(&lock));
    must(symlink(fixture.root.join("absent"), lock));
    assert!(matches!(
        create_database_backup(&fixture.paths).await,
        Err(BackupError::InvalidPath)
    ));
}
