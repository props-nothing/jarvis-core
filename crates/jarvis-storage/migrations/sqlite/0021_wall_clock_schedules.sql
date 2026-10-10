-- Wall-clock schedules: "every weekday at 08:00 in Europe/Amsterdam" (`P6-003`, ADR-0182).
--
-- A third cadence, `daily`, joins `every` and `once`. It is the one that needs a time zone, so the zone is stored with the schedule: the
-- IANA name, the minute of the day, and the weekday set (bit 0 Monday .. bit 6 Sunday). The next fire is still stored as an integer in
-- `next_run_nanos`, so "is this due?" stays a comparison in SQL; the zone rules are applied in Rust when a fire is claimed.
--
-- SQLite cannot change a CHECK constraint in place, so the table is rebuilt. Nothing references `schedules` by foreign key (project links
-- hold the id as text), so the swap is safe inside the migration's transaction. Existing rows keep every value.

CREATE TABLE schedules_new (
    id TEXT PRIMARY KEY NOT NULL CHECK (length(id) = 36),
    workspace_id TEXT NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    objective TEXT NOT NULL CHECK (length(objective) BETWEEN 1 AND 4096),
    cadence TEXT NOT NULL CHECK (cadence IN ('every', 'once', 'daily')),
    interval_seconds INTEGER CHECK (interval_seconds IS NULL OR interval_seconds BETWEEN 60 AND 31622400),
    minute_of_day INTEGER CHECK (minute_of_day IS NULL OR minute_of_day BETWEEN 0 AND 1439),
    weekdays INTEGER CHECK (weekdays IS NULL OR weekdays BETWEEN 1 AND 127),
    time_zone TEXT CHECK (time_zone IS NULL OR length(time_zone) BETWEEN 1 AND 64),
    enabled INTEGER NOT NULL CHECK (enabled IN (0, 1)),
    next_run_nanos INTEGER NOT NULL,
    session_id TEXT REFERENCES sessions (id) ON DELETE SET NULL,
    last_run_id TEXT,
    last_fired_at TEXT CHECK (last_fired_at IS NULL OR length(last_fired_at) BETWEEN 20 AND 64),
    fire_count INTEGER NOT NULL CHECK (fire_count >= 0),
    skipped_count INTEGER NOT NULL CHECK (skipped_count >= 0),
    created_at TEXT NOT NULL CHECK (length(created_at) BETWEEN 20 AND 64),
    -- Each cadence carries exactly its own fields: the columns cannot describe half a schedule.
    CHECK ((cadence = 'every') = (interval_seconds IS NOT NULL)),
    CHECK ((cadence = 'daily') = (minute_of_day IS NOT NULL AND weekdays IS NOT NULL AND time_zone IS NOT NULL)),
    CHECK ((cadence = 'daily') OR (minute_of_day IS NULL AND weekdays IS NULL AND time_zone IS NULL))
);

INSERT INTO schedules_new (
    id, workspace_id, objective, cadence, interval_seconds, enabled, next_run_nanos, session_id,
    last_run_id, last_fired_at, fire_count, skipped_count, created_at
)
SELECT
    id, workspace_id, objective, cadence, interval_seconds, enabled, next_run_nanos, session_id,
    last_run_id, last_fired_at, fire_count, skipped_count, created_at
FROM schedules;

DROP TABLE schedules;
ALTER TABLE schedules_new RENAME TO schedules;

CREATE INDEX schedules_due_idx ON schedules (enabled, next_run_nanos);
CREATE INDEX schedules_workspace_idx ON schedules (workspace_id, created_at);

UPDATE jarvis_storage_metadata
SET schema_version = 21
WHERE singleton = 1;

PRAGMA user_version = 21;
