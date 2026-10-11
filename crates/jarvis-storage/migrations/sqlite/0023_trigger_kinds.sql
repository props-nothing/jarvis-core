-- More trigger kinds: a new file in a granted folder, and a request to a secret local address (`P6-012`, ADR-0195).
--
-- `kind` widens from `mail` to `mail`, `file` and `webhook`. `query` keeps meaning "what is watched": a Gmail search, a folder inside a granted
-- folder, or (for a webhook) the fixed word `webhook`. A webhook has no interval and is never polled; it is started by a request that names
-- its secret. Only the **SHA-256 of the secret** is stored, so a copy of the database cannot start a run; the secret itself is shown once.
--
-- SQLite cannot change a CHECK in place, and dropping the parent table would cascade-delete the dedupe history (`trigger_events` holds a
-- cascading foreign key to it). So both tables are rebuilt beside the old ones, the rows copied, and the old ones dropped child first.

CREATE TABLE triggers_new (
    id TEXT PRIMARY KEY NOT NULL CHECK (length(id) = 36),
    workspace_id TEXT NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    kind TEXT NOT NULL CHECK (kind IN ('mail', 'file', 'webhook')),
    query TEXT NOT NULL CHECK (length(query) BETWEEN 1 AND 300),
    objective TEXT NOT NULL CHECK (length(objective) BETWEEN 1 AND 2000),
    interval_seconds INTEGER NOT NULL CHECK (interval_seconds BETWEEN 60 AND 86400),
    project_id TEXT REFERENCES projects (id) ON DELETE SET NULL,
    enabled INTEGER NOT NULL CHECK (enabled IN (0, 1)),
    next_poll_nanos INTEGER NOT NULL,
    baselined INTEGER NOT NULL CHECK (baselined IN (0, 1)),
    last_polled_at TEXT CHECK (last_polled_at IS NULL OR length(last_polled_at) BETWEEN 20 AND 64),
    last_error TEXT CHECK (last_error IS NULL OR length(last_error) <= 300),
    last_run_id TEXT,
    fire_count INTEGER NOT NULL CHECK (fire_count >= 0),
    created_at TEXT NOT NULL CHECK (length(created_at) BETWEEN 20 AND 64),
    secret_hash TEXT CHECK (secret_hash IS NULL OR length(secret_hash) = 64),
    -- A webhook carries its secret's hash and nothing else does.
    CHECK ((kind = 'webhook') = (secret_hash IS NOT NULL))
);

INSERT INTO triggers_new (
    id, workspace_id, kind, query, objective, interval_seconds, project_id, enabled, next_poll_nanos, baselined,
    last_polled_at, last_error, last_run_id, fire_count, created_at
)
SELECT
    id, workspace_id, kind, query, objective, interval_seconds, project_id, enabled, next_poll_nanos, baselined,
    last_polled_at, last_error, last_run_id, fire_count, created_at
FROM triggers;

CREATE TABLE trigger_events_new (
    trigger_id TEXT NOT NULL REFERENCES triggers_new (id) ON DELETE CASCADE,
    event_key TEXT NOT NULL CHECK (length(event_key) BETWEEN 1 AND 128),
    run_id TEXT,
    seen_nanos INTEGER NOT NULL,
    PRIMARY KEY (trigger_id, event_key)
);

INSERT INTO trigger_events_new (trigger_id, event_key, run_id, seen_nanos)
SELECT trigger_id, event_key, run_id, seen_nanos FROM trigger_events;

DROP TABLE trigger_events;
DROP TABLE triggers;
ALTER TABLE triggers_new RENAME TO triggers;
ALTER TABLE trigger_events_new RENAME TO trigger_events;

CREATE INDEX triggers_due_idx ON triggers (enabled, next_poll_nanos);
CREATE INDEX triggers_workspace_idx ON triggers (workspace_id, created_at);
CREATE UNIQUE INDEX triggers_secret_idx ON triggers (secret_hash) WHERE secret_hash IS NOT NULL;
CREATE INDEX trigger_events_recent_idx ON trigger_events (trigger_id, seen_nanos);

UPDATE jarvis_storage_metadata
SET schema_version = 23
WHERE singleton = 1;

PRAGMA user_version = 23;
