-- Durable state for coding tasks handed to an external ACP agent (`P7-004b`, ADR-0209).
-- The process is not durable; the intent, status and result are. A daemon start marks a task that was queued or running as interrupted,
-- because its child process died with the prior daemon and silently restarting a coding task could repeat effects.

CREATE TABLE delegated_tasks (
    id TEXT PRIMARY KEY NOT NULL CHECK (length(id) = 36),
    workspace_id TEXT NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    repository TEXT NOT NULL CHECK (length(repository) BETWEEN 1 AND 260),
    task TEXT NOT NULL CHECK (length(task) BETWEEN 1 AND 4000),
    allow_commands INTEGER NOT NULL CHECK (allow_commands IN (0, 1)),
    minutes INTEGER NOT NULL CHECK (minutes BETWEEN 1 AND 60),
    status TEXT NOT NULL CHECK (status IN ('queued', 'running', 'completed', 'failed', 'cancelled', 'interrupted')),
    branch TEXT CHECK (branch IS NULL OR length(branch) BETWEEN 1 AND 300),
    worktree TEXT CHECK (worktree IS NULL OR length(worktree) BETWEEN 1 AND 4096),
    summary TEXT CHECK (summary IS NULL OR length(summary) <= 12000),
    result_json TEXT CHECK (result_json IS NULL OR length(result_json) <= 32768),
    error TEXT CHECK (error IS NULL OR length(error) <= 1000),
    created_nanos INTEGER NOT NULL,
    created_at TEXT NOT NULL CHECK (length(created_at) BETWEEN 20 AND 64),
    updated_at TEXT NOT NULL CHECK (length(updated_at) BETWEEN 20 AND 64),
    finished_at TEXT CHECK (finished_at IS NULL OR length(finished_at) BETWEEN 20 AND 64)
);

CREATE INDEX delegated_tasks_workspace_idx ON delegated_tasks (workspace_id, created_nanos DESC);
CREATE UNIQUE INDEX delegated_tasks_active_idx ON delegated_tasks (workspace_id)
WHERE status IN ('queued', 'running');

UPDATE jarvis_storage_metadata SET schema_version = 26 WHERE singleton = 1;
PRAGMA user_version = 26;
