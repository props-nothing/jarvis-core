-- Phone calls JARVIS placed to the owner (`P8-007`, `P8-009`, ADR-0200).
--
-- A row is written **before** the provider is asked to ring, with the status `starting`, so a request that times out after the provider
-- accepted it still leaves a record of a call that may be ringing, and the rate limit counts it. The provider's identifiers are attached
-- when it answers. Nothing here is retried: a second request would ring the phone twice.
--
-- The record holds what the owner needs to see afterwards (who was called, why, how it went) and no more. The transcript and the audio
-- are not kept: the provider holds them under its own retention, and the call status tool reads the transcript from there when asked.
-- `summary` is the provider's short analysis of the call. The whole table can be cleared from Settings, and rows older than ninety days
-- are removed when the daemon starts.

CREATE TABLE phone_calls (
    id TEXT PRIMARY KEY NOT NULL CHECK (length(id) = 36),
    workspace_id TEXT NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    direction TEXT NOT NULL CHECK (direction IN ('outbound', 'inbound')),
    provider TEXT NOT NULL CHECK (provider IN ('elevenlabs')),
    number TEXT NOT NULL CHECK (length(number) BETWEEN 8 AND 20),
    reason TEXT NOT NULL CHECK (length(reason) <= 500),
    urgent INTEGER NOT NULL CHECK (urgent IN (0, 1)),
    status TEXT NOT NULL CHECK (status IN ('starting', 'initiated', 'in_progress', 'processing', 'done', 'failed')),
    conversation_id TEXT CHECK (conversation_id IS NULL OR length(conversation_id) BETWEEN 1 AND 128),
    provider_call_id TEXT CHECK (provider_call_id IS NULL OR length(provider_call_id) BETWEEN 1 AND 128),
    duration_seconds INTEGER CHECK (duration_seconds IS NULL OR duration_seconds >= 0),
    ended_because TEXT CHECK (ended_because IS NULL OR length(ended_because) <= 200),
    summary TEXT CHECK (summary IS NULL OR length(summary) <= 2000),
    error TEXT CHECK (error IS NULL OR length(error) <= 300),
    created_nanos INTEGER NOT NULL,
    created_at TEXT NOT NULL CHECK (length(created_at) BETWEEN 20 AND 64),
    updated_at TEXT NOT NULL CHECK (length(updated_at) BETWEEN 20 AND 64)
);

CREATE INDEX phone_calls_workspace_idx ON phone_calls (workspace_id, created_nanos);
CREATE INDEX phone_calls_open_idx ON phone_calls (status) WHERE status IN ('starting', 'initiated', 'in_progress', 'processing');

UPDATE jarvis_storage_metadata
SET schema_version = 24
WHERE singleton = 1;

PRAGMA user_version = 24;
