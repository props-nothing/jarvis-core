//! Isolated, on-disk fixtures for repository and daemon behavior tests.
//! Migration/permission/startup tests must keep using `SqliteDatabase::open`.

use std::{fs, io::Write, path::Path};

use tokio::sync::OnceCell;

use crate::{DatabaseError, PathError, PathKind, SqliteDatabase};

// Cache bytes only: a pool cannot be shared across independently owned test runtimes.
static MIGRATED_IMAGE: OnceCell<Vec<u8>> = OnceCell::const_new();

impl SqliteDatabase {
    /// Creates an independent current-schema test database at a new path.
    ///
    /// Copies a fully closed database produced by the real migrations once per
    /// test process, then reopens through production validation, permissions and
    /// connection settings. No mutable database or connection is shared.
    /// Production binaries do not enable the `test-support` feature.
    ///
    /// # Errors
    ///
    /// Fails if the path exists, template creation fails, or production opening
    /// refuses the copy. Use [`Self::open`] for migration and startup tests.
    pub async fn test_fixture(path: &Path) -> Result<Self, DatabaseError> {
        if !path.is_absolute() {
            return Err(DatabaseError::RelativePath);
        }
        let bytes = MIGRATED_IMAGE.get_or_try_init(migrated_image).await?;
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .map_err(fixture_io)?;
        file.write_all(bytes).map_err(fixture_io)?;
        drop(file);
        Self::open(path).await
    }
}

async fn migrated_image() -> Result<Vec<u8>, DatabaseError> {
    struct TemplateDirectory(std::path::PathBuf);
    impl Drop for TemplateDirectory {
        fn drop(&mut self) {
            jarvis_core::remove_scratch_dir(&self.0);
        }
    }

    let directory = TemplateDirectory(
        std::env::temp_dir().join(format!("jarvis-test-image-{}", jarvis_core::scratch_tag())),
    );
    fs::create_dir(&directory.0).map_err(fixture_io)?;
    let path = directory.0.join(crate::DEFAULT_DATABASE_FILENAME);
    let database = SqliteDatabase::open(&path).await?;
    // Closing every connection checkpoints WAL before reading the main file.
    database.close().await;
    fs::read(path).map_err(fixture_io)
}

fn fixture_io(source: std::io::Error) -> DatabaseError {
    PathError::Io {
        kind: PathKind::Data,
        operation: "create test database image",
        source,
    }
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn must<T, E: std::fmt::Debug>(value: Result<T, E>) -> T {
        value.unwrap_or_else(|error| panic!("fixture: {error:?}"))
    }

    #[tokio::test]
    async fn copies_are_isolated_persistent_and_use_production_settings() {
        let root =
            std::env::temp_dir().join(format!("jarvis-image-check-{}", jarvis_core::scratch_tag()));
        must(fs::create_dir(&root));
        let first_path = root.join("first.sqlite3");
        let second_path = root.join("second.sqlite3");
        let first = must(SqliteDatabase::test_fixture(&first_path).await);
        let second = must(SqliteDatabase::test_fixture(&second_path).await);
        for db in [&first, &second] {
            assert!(db.pre_migration_backup().is_none());
            must(crate::load_local_identity(db).await);
            for (pragma, expected) in [
                ("PRAGMA user_version", crate::CURRENT_SCHEMA_VERSION),
                ("PRAGMA synchronous", 2),
                ("PRAGMA foreign_keys", 1),
                ("PRAGMA trusted_schema", 0),
                ("PRAGMA recursive_triggers", 1),
            ] {
                let actual: i64 = must(sqlx::query_scalar(pragma).fetch_one(db.pool()).await);
                assert_eq!(actual, expected, "{pragma}");
            }
            let journal: String = must(
                sqlx::query_scalar("PRAGMA journal_mode")
                    .fetch_one(db.pool())
                    .await,
            );
            assert_eq!(journal, "wal");
        }
        must(
            sqlx::query("CREATE TABLE fixture_isolation (value TEXT)")
                .execute(first.pool())
                .await,
        );
        must(
            sqlx::query("INSERT INTO fixture_isolation VALUES ('first only')")
                .execute(first.pool())
                .await,
        );
        assert!(
            sqlx::query("SELECT * FROM fixture_isolation")
                .fetch_all(second.pool())
                .await
                .is_err()
        );
        first.close().await;
        let reopened = must(SqliteDatabase::open(&first_path).await);
        let value: String = must(
            sqlx::query_scalar("SELECT value FROM fixture_isolation")
                .fetch_one(reopened.pool())
                .await,
        );
        assert_eq!(value, "first only");
        reopened.close().await;
        second.close().await;
        jarvis_core::remove_scratch_dir(&root);
    }

    #[tokio::test]
    async fn an_existing_file_is_never_replaced_with_a_fixture() {
        let root = std::env::temp_dir().join(format!(
            "jarvis-image-refusal-{}",
            jarvis_core::scratch_tag()
        ));
        must(fs::create_dir(&root));
        let path = root.join("existing.sqlite3");
        must(fs::write(&path, b"must survive"));
        assert!(SqliteDatabase::test_fixture(&path).await.is_err());
        assert_eq!(must(fs::read(&path)), b"must survive");
        assert!(
            SqliteDatabase::test_fixture(Path::new("relative.sqlite3"))
                .await
                .is_err()
        );
        jarvis_core::remove_scratch_dir(&root);
    }
}
