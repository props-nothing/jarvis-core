-- A search index over the folders the owner granted (`P11-014`, ADR-0206).
--
-- It is derived data: every row can be rebuilt from the granted folders, and the daemon removes rows whose file is gone, changed, or no
-- longer under a granted folder, and all of them when no folder is granted. `knowledge_files` says what was read and when it last
-- changed on disk (so an unchanged file is not read again); `knowledge_chunks` says where each piece of text sits in its file;
-- `knowledge_fts` holds the pieces themselves and is the full-text index over them (SQLite FTS5, ranked by BM25). A piece's row in
-- `knowledge_fts` has the same rowid as its row in `knowledge_chunks`, and a trigger removes it when the chunk goes.

CREATE TABLE knowledge_files (
    id INTEGER PRIMARY KEY NOT NULL,
    root TEXT NOT NULL CHECK (length(root) BETWEEN 1 AND 4096),
    path TEXT NOT NULL CHECK (length(path) BETWEEN 1 AND 4096),
    kind TEXT NOT NULL CHECK (kind IN ('text', 'document')),
    size_bytes INTEGER NOT NULL CHECK (size_bytes >= 0),
    modified_nanos INTEGER NOT NULL,
    indexed_at TEXT NOT NULL CHECK (length(indexed_at) BETWEEN 20 AND 64),
    UNIQUE (root, path)
);

CREATE TABLE knowledge_chunks (
    id INTEGER PRIMARY KEY NOT NULL,
    file_id INTEGER NOT NULL REFERENCES knowledge_files (id) ON DELETE CASCADE,
    ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
    start_line INTEGER NOT NULL CHECK (start_line >= 1),
    char_offset INTEGER NOT NULL CHECK (char_offset >= 0),
    UNIQUE (file_id, ordinal)
);

CREATE VIRTUAL TABLE knowledge_fts USING fts5 (
    text,
    tokenize = 'unicode61 remove_diacritics 2'
);

CREATE TRIGGER knowledge_chunks_after_delete AFTER DELETE ON knowledge_chunks BEGIN
    DELETE FROM knowledge_fts WHERE rowid = old.id;
END;

UPDATE jarvis_storage_metadata
SET schema_version = 25
WHERE singleton = 1;

PRAGMA user_version = 25;
