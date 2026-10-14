CREATE UNIQUE INDEX delegated_tasks_id_workspace ON delegated_tasks (id, workspace_id);

CREATE TABLE delegated_task_activity (
    task_id TEXT NOT NULL,
    workspace_id TEXT NOT NULL,
    sequence INTEGER NOT NULL CHECK (sequence > 0),
    summary TEXT NOT NULL CHECK (length(summary) BETWEEN 1 AND 1600),
    created_at TEXT NOT NULL CHECK (length(created_at) BETWEEN 20 AND 64),
    PRIMARY KEY (task_id, sequence),
    FOREIGN KEY (task_id, workspace_id) REFERENCES delegated_tasks (id, workspace_id) ON DELETE CASCADE
);

UPDATE jarvis_storage_metadata
SET schema_version = 27
WHERE singleton = 1;

PRAGMA user_version = 27;