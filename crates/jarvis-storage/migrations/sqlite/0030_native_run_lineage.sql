-- Legacy objectives establish only the old restricted role, never a guessed parent.
ALTER TABLE agent_runs ADD COLUMN parent_run_id TEXT REFERENCES agent_runs(id)
    CHECK (parent_run_id IS NULL OR (length(parent_run_id) = 36 AND parent_run_id <> id));
ALTER TABLE agent_runs ADD COLUMN delegation_kind TEXT NOT NULL DEFAULT 'none'
    CHECK (delegation_kind IN ('none', 'native', 'legacy'))
    CHECK ((delegation_kind = 'native' AND parent_run_id IS NOT NULL)
        OR (delegation_kind <> 'native' AND parent_run_id IS NULL));
UPDATE agent_runs SET delegation_kind = 'legacy'
WHERE substr(COALESCE(full_objective, objective), 1, length('You are a sub-agent working for JARVIS on one bounded task. Do only that task, use the tools you are offered, and report the result briefly and factually. You cannot delegate further, and nobody is available to answer questions, so make reasonable assumptions.' || char(10) || char(10) || 'Task: ')) = ('You are a sub-agent working for JARVIS on one bounded task. Do only that task, use the tools you are offered, and report the result briefly and factually. You cannot delegate further, and nobody is available to answer questions, so make reasonable assumptions.' || char(10) || char(10) || 'Task: ');
CREATE INDEX agent_runs_parent_idx ON agent_runs(parent_run_id) WHERE parent_run_id IS NOT NULL;
CREATE INDEX agent_runs_delegated_idx ON agent_runs(workspace_id, id) WHERE delegation_kind <> 'none';
CREATE TRIGGER agent_runs_lineage_immutable
BEFORE UPDATE OF parent_run_id, delegation_kind ON agent_runs
WHEN NEW.parent_run_id IS NOT OLD.parent_run_id OR NEW.delegation_kind <> OLD.delegation_kind
BEGIN
    SELECT RAISE(ABORT, 'run lineage is immutable');
END;
UPDATE jarvis_storage_metadata SET schema_version = 30 WHERE singleton = 1;
PRAGMA user_version = 30;
