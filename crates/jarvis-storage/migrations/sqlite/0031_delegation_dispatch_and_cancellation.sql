-- Old tasks keep unknown provenance; never infer ownership from their text.
ALTER TABLE agent_runs ADD COLUMN origin_call_id TEXT REFERENCES tool_calls(id)
    CHECK (origin_call_id IS NULL OR delegation_kind = 'native');
CREATE UNIQUE INDEX agent_runs_origin_call_idx ON agent_runs(origin_call_id) WHERE origin_call_id IS NOT NULL;
CREATE TRIGGER agent_runs_dispatch_immutable BEFORE UPDATE OF origin_call_id ON agent_runs
WHEN NEW.origin_call_id IS NOT OLD.origin_call_id
BEGIN SELECT RAISE(ABORT, 'run dispatch is immutable'); END;

ALTER TABLE delegated_tasks ADD COLUMN parent_run_id TEXT REFERENCES agent_runs(id);
ALTER TABLE delegated_tasks ADD COLUMN origin_call_id TEXT REFERENCES tool_calls(id)
    CHECK ((origin_call_id IS NULL) = (parent_run_id IS NULL));
ALTER TABLE delegated_tasks ADD COLUMN cancellation_requested_at TEXT;
CREATE UNIQUE INDEX delegated_tasks_origin_call_idx ON delegated_tasks(origin_call_id) WHERE origin_call_id IS NOT NULL;
CREATE INDEX delegated_tasks_parent_idx ON delegated_tasks(parent_run_id) WHERE parent_run_id IS NOT NULL;
CREATE TRIGGER delegated_tasks_lineage_immutable BEFORE UPDATE OF parent_run_id, origin_call_id ON delegated_tasks
WHEN NEW.parent_run_id IS NOT OLD.parent_run_id OR NEW.origin_call_id IS NOT OLD.origin_call_id
BEGIN SELECT RAISE(ABORT, 'coding task lineage is immutable'); END;

-- Separate intent preserves a completed parent's immutable result and version.
CREATE TABLE run_family_stops (
    run_id TEXT PRIMARY KEY NOT NULL REFERENCES agent_runs(id),
    requested_at TEXT NOT NULL
) STRICT;
CREATE TRIGGER run_family_stops_immutable BEFORE UPDATE ON run_family_stops
BEGIN SELECT RAISE(ABORT, 'family stop intent is immutable'); END;

UPDATE jarvis_storage_metadata SET schema_version = 31 WHERE singleton = 1;
PRAGMA user_version = 31;
