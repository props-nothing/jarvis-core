ALTER TABLE agent_runs ADD COLUMN full_objective TEXT
    CHECK (full_objective IS NULL OR length(CAST(full_objective AS BLOB)) BETWEEN 1 AND 800000);

UPDATE jarvis_storage_metadata SET schema_version = 28 WHERE singleton = 1;
PRAGMA user_version = 28;