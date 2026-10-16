CREATE TEMP TABLE coding_activity_backup AS SELECT * FROM delegated_task_activity;
DROP TABLE delegated_task_activity;
ALTER TABLE delegated_tasks RENAME TO delegated_tasks_old;

CREATE TABLE delegated_tasks (
    id TEXT PRIMARY KEY NOT NULL CHECK (length(id) = 36),
    workspace_id TEXT NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    repository TEXT NOT NULL CHECK (length(repository) BETWEEN 1 AND 4096),
    task TEXT NOT NULL CHECK (length(task) BETWEEN 1 AND 50000),
    allow_commands INTEGER NOT NULL CHECK (allow_commands IN (0, 1)),
    minutes INTEGER NOT NULL CHECK (minutes >= 0),
    status TEXT NOT NULL CHECK (status IN ('queued', 'running', 'completed', 'failed', 'cancelled', 'interrupted')),
    branch TEXT CHECK (branch IS NULL OR length(branch) BETWEEN 1 AND 300),
    worktree TEXT CHECK (worktree IS NULL OR length(worktree) BETWEEN 1 AND 4096),
    request_json TEXT CHECK (request_json IS NULL OR length(request_json) <= 1048576),
    summary TEXT CHECK (summary IS NULL OR length(summary) <= 12000),
    result_json TEXT CHECK (result_json IS NULL OR length(result_json) <= 262144),
    error TEXT CHECK (error IS NULL OR length(error) <= 1000),
    created_nanos INTEGER NOT NULL,
    created_at TEXT NOT NULL CHECK (length(created_at) BETWEEN 20 AND 64),
    updated_at TEXT NOT NULL CHECK (length(updated_at) BETWEEN 20 AND 64),
    finished_at TEXT CHECK (finished_at IS NULL OR length(finished_at) BETWEEN 20 AND 64)
);

INSERT INTO delegated_tasks (id,workspace_id,repository,task,allow_commands,minutes,status,branch,worktree,summary,result_json,error,created_nanos,created_at,updated_at,finished_at) SELECT id,workspace_id,repository,task,allow_commands,minutes,status,branch,worktree,summary,result_json,error,created_nanos,created_at,updated_at,finished_at FROM delegated_tasks_old;
DROP TABLE delegated_tasks_old;
CREATE INDEX delegated_tasks_workspace_idx ON delegated_tasks (workspace_id, created_nanos DESC);
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

INSERT INTO delegated_task_activity SELECT * FROM coding_activity_backup;
DROP TABLE coding_activity_backup;
ALTER TABLE approvals RENAME COLUMN arguments_json TO arguments_json_legacy;
ALTER TABLE approvals ADD COLUMN arguments_json TEXT CHECK (
    arguments_json IS NULL OR length(CAST(arguments_json AS BLOB)) <= 1048576
);
UPDATE approvals SET arguments_json = arguments_json_legacy;
ALTER TABLE approvals DROP COLUMN arguments_json_legacy;

UPDATE jarvis_storage_metadata SET schema_version = 29 WHERE singleton = 1;
PRAGMA user_version = 29;
