-- Triggers: an event that starts an ordinary run (`P6-012`, ADR-0193). The first kind is `mail`: poll one Gmail search on an interval.
--
-- A trigger is, like a schedule, a *request to start a run*; it carries no authority. The owner writes the query and the objective; the
-- text of a matching message is never stored here and never placed in the objective, only its Gmail message id.
--
-- # Why a second table
--
-- `trigger_events` is the dedupe record. One row per (trigger, event key) is claimed *before* a run starts, by the primary key, so a
-- replayed or re-listed message cannot start a second run, however many polls or restarts see it. `run_id` is empty for the baseline
-- (messages that already matched when the trigger was created, recorded so they do not start runs) and for a claim whose run failed to
-- start: at most once, the same direction as a schedule fire.
--
-- # Times
--
-- `next_poll_nanos` and `seen_nanos` are integers for the reason `schedules.next_run_nanos` is: "is this due?" and "how many today?" are
-- comparisons made in SQL, and the text form of a timestamp does not sort.

CREATE TABLE triggers (
    id TEXT PRIMARY KEY NOT NULL CHECK (length(id) = 36),
    workspace_id TEXT NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    kind TEXT NOT NULL CHECK (kind IN ('mail')),
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
    created_at TEXT NOT NULL CHECK (length(created_at) BETWEEN 20 AND 64)
);

CREATE INDEX triggers_due_idx ON triggers (enabled, next_poll_nanos);
CREATE INDEX triggers_workspace_idx ON triggers (workspace_id, created_at);

CREATE TABLE trigger_events (
    trigger_id TEXT NOT NULL REFERENCES triggers (id) ON DELETE CASCADE,
    event_key TEXT NOT NULL CHECK (length(event_key) BETWEEN 1 AND 128),
    run_id TEXT,
    seen_nanos INTEGER NOT NULL,
    PRIMARY KEY (trigger_id, event_key)
);

CREATE INDEX trigger_events_recent_idx ON trigger_events (trigger_id, seen_nanos);

UPDATE jarvis_storage_metadata
SET schema_version = 22
WHERE singleton = 1;

PRAGMA user_version = 22;
