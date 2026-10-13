//! The search index over the owner's granted folders (`ADR-0206`).
//!
//! Derived data and nothing else: each row is a copy of text that sits in a folder the owner granted, kept so a question can be
//! answered with the passage and the file it came from without reading every file. The daemon's indexer decides what goes in
//! (only granted folders, only files it may read) and removes what no longer belongs; this module stores, searches and deletes.

use jarvis_core::UtcTimestamp;
use sqlx::Row;

use crate::database::{DatabaseError, SqliteDatabase};

/// The most words of a query that are looked up; more would only slow the lookup.
const MAX_QUERY_TERMS: usize = 12;
/// The most characters of one piece of text.
pub const MAX_CHUNK_CHARS: usize = 8_000;

/// How a file's text was got, which decides how a reader goes back to it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum KnowledgeKind {
    /// A plain text file: a reader goes back to it by line.
    Text,
    /// A PDF, Word, spreadsheet or slide file: a reader goes back to it by character offset into its extracted text.
    Document,
}

impl KnowledgeKind {
    /// The word stored for the kind.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::Document => "document",
        }
    }
}

/// One piece of a file's text.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct KnowledgeChunkInput {
    /// The line the piece starts on, counting from 1, in the file's text.
    pub start_line: u32,
    /// Where the piece starts, in characters from the start of the file's text.
    pub char_offset: u32,
    /// The text, never empty and never more than 8,000 characters.
    pub text: String,
}

/// A file to put in the index, replacing whatever was stored for it.
#[derive(Clone, Debug)]
pub struct KnowledgeFileInput<'a> {
    /// The granted folder the file is under, as the owner wrote it.
    pub root: &'a str,
    /// The path inside that folder, with `/` between parts.
    pub path: &'a str,
    /// How the text was got.
    pub kind: KnowledgeKind,
    /// The file's size when it was read.
    pub size_bytes: u64,
    /// When the file last changed on disk, in nanoseconds since the epoch.
    pub modified_nanos: i64,
    /// The text, in pieces.
    pub chunks: &'a [KnowledgeChunkInput],
    /// When it was read.
    pub now: UtcTimestamp,
}

/// What the index knows about a file, enough to tell whether the file has changed since.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct KnowledgeFileState {
    /// The granted folder.
    pub root: String,
    /// The path inside it.
    pub path: String,
    /// The size when it was read.
    pub size_bytes: u64,
    /// The modification time when it was read.
    pub modified_nanos: i64,
}

/// A passage that matched a search.
#[derive(Clone, Debug, PartialEq)]
pub struct KnowledgeHit {
    /// The granted folder.
    pub root: String,
    /// The path inside it.
    pub path: String,
    /// How the text was got.
    pub kind: String,
    /// The piece's place in the file, counting from 0.
    pub ordinal: u32,
    /// The line it starts on.
    pub start_line: u32,
    /// Where it starts, in characters.
    pub char_offset: u32,
    /// The passage.
    pub text: String,
    /// The BM25 rank: lower is a better match.
    pub score: f64,
}

/// How much the index holds.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct KnowledgeStats {
    /// Files indexed.
    pub files: u64,
    /// Pieces of text indexed.
    pub chunks: u64,
}

fn sqlite(operation: &'static str) -> impl FnOnce(sqlx::Error) -> DatabaseError {
    move |source| DatabaseError::Sqlite { operation, source }
}

/// Every file in the index, for comparing with what is on disk.
///
/// # Errors
///
/// Returns [`DatabaseError::Sqlite`] when the read fails.
pub async fn knowledge_manifest(
    database: &SqliteDatabase,
) -> Result<Vec<KnowledgeFileState>, DatabaseError> {
    let rows = sqlx::query("SELECT root, path, size_bytes, modified_nanos FROM knowledge_files")
        .fetch_all(database.pool())
        .await
        .map_err(sqlite("read the knowledge index"))?;
    rows.iter()
        .map(|row| {
            Ok(KnowledgeFileState {
                root: row
                    .try_get("root")
                    .map_err(sqlite("read the knowledge index"))?,
                path: row
                    .try_get("path")
                    .map_err(sqlite("read the knowledge index"))?,
                size_bytes: u64::try_from(
                    row.try_get::<i64, _>("size_bytes")
                        .map_err(sqlite("read the knowledge index"))?,
                )
                .unwrap_or(0),
                modified_nanos: row
                    .try_get("modified_nanos")
                    .map_err(sqlite("read the knowledge index"))?,
            })
        })
        .collect()
}

/// Puts a file in the index, replacing what was there, in one transaction: a search sees the old text or the new, never half of each.
///
/// # Errors
///
/// Returns [`DatabaseError::Sqlite`] when the write fails, including a piece that is empty or over 8,000 characters.
pub async fn replace_knowledge_file(
    database: &SqliteDatabase,
    file: &KnowledgeFileInput<'_>,
) -> Result<(), DatabaseError> {
    let mut transaction = database
        .pool()
        .begin()
        .await
        .map_err(sqlite("index a file"))?;
    delete_file(&mut transaction, file.root, file.path).await?;
    let file_id: i64 = sqlx::query_scalar(
        "INSERT INTO knowledge_files (root, path, kind, size_bytes, modified_nanos, indexed_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6) RETURNING id",
    )
    .bind(file.root)
    .bind(file.path)
    .bind(file.kind.as_str())
    .bind(i64::try_from(file.size_bytes).unwrap_or(i64::MAX))
    .bind(file.modified_nanos)
    .bind(file.now.to_string())
    .fetch_one(&mut *transaction)
    .await
    .map_err(sqlite("index a file"))?;
    for (ordinal, chunk) in file.chunks.iter().enumerate() {
        let length = chunk.text.chars().count();
        if length == 0 || length > MAX_CHUNK_CHARS {
            return Err(DatabaseError::InvalidKnowledge { field: "text" });
        }
        let chunk_id: i64 = sqlx::query_scalar(
            "INSERT INTO knowledge_chunks (file_id, ordinal, start_line, char_offset) VALUES (?1, ?2, ?3, ?4) RETURNING id",
        )
        .bind(file_id)
        .bind(i64::try_from(ordinal).unwrap_or(i64::MAX))
        .bind(i64::from(chunk.start_line))
        .bind(i64::from(chunk.char_offset))
        .fetch_one(&mut *transaction)
        .await
        .map_err(sqlite("index a file"))?;
        sqlx::query("INSERT INTO knowledge_fts (rowid, text) VALUES (?1, ?2)")
            .bind(chunk_id)
            .bind(&chunk.text)
            .execute(&mut *transaction)
            .await
            .map_err(sqlite("index a file"))?;
    }
    transaction.commit().await.map_err(sqlite("index a file"))
}

async fn delete_file(
    transaction: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    root: &str,
    path: &str,
) -> Result<(), DatabaseError> {
    // The chunks go first, by hand: the full-text rows are removed by a trigger on the chunk rows, and a cascade is not something to
    // depend on for that.
    sqlx::query(
        "DELETE FROM knowledge_chunks WHERE file_id IN (SELECT id FROM knowledge_files WHERE root = ?1 AND path = ?2)",
    )
    .bind(root)
    .bind(path)
    .execute(&mut **transaction)
    .await
    .map_err(sqlite("remove a file from the knowledge index"))?;
    sqlx::query("DELETE FROM knowledge_files WHERE root = ?1 AND path = ?2")
        .bind(root)
        .bind(path)
        .execute(&mut **transaction)
        .await
        .map_err(sqlite("remove a file from the knowledge index"))?;
    Ok(())
}

/// Takes one file out of the index.
///
/// # Errors
///
/// Returns [`DatabaseError::Sqlite`] when the write fails.
pub async fn remove_knowledge_file(
    database: &SqliteDatabase,
    root: &str,
    path: &str,
) -> Result<(), DatabaseError> {
    let mut transaction = database
        .pool()
        .begin()
        .await
        .map_err(sqlite("remove a file from the knowledge index"))?;
    delete_file(&mut transaction, root, path).await?;
    transaction
        .commit()
        .await
        .map_err(sqlite("remove a file from the knowledge index"))
}

/// Empties the index. Returns how many files it held.
///
/// # Errors
///
/// Returns [`DatabaseError::Sqlite`] when the write fails.
pub async fn clear_knowledge(database: &SqliteDatabase) -> Result<u64, DatabaseError> {
    let mut transaction = database
        .pool()
        .begin()
        .await
        .map_err(sqlite("clear the knowledge index"))?;
    sqlx::query("DELETE FROM knowledge_chunks")
        .execute(&mut *transaction)
        .await
        .map_err(sqlite("clear the knowledge index"))?;
    sqlx::query("DELETE FROM knowledge_fts")
        .execute(&mut *transaction)
        .await
        .map_err(sqlite("clear the knowledge index"))?;
    let removed = sqlx::query("DELETE FROM knowledge_files")
        .execute(&mut *transaction)
        .await
        .map_err(sqlite("clear the knowledge index"))?
        .rows_affected();
    transaction
        .commit()
        .await
        .map_err(sqlite("clear the knowledge index"))?;
    Ok(removed)
}

/// How many files and pieces of text the index holds.
///
/// # Errors
///
/// Returns [`DatabaseError::Sqlite`] when the read fails.
pub async fn knowledge_stats(database: &SqliteDatabase) -> Result<KnowledgeStats, DatabaseError> {
    let files: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM knowledge_files")
        .fetch_one(database.pool())
        .await
        .map_err(sqlite("count the knowledge index"))?;
    let chunks: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM knowledge_chunks")
        .fetch_one(database.pool())
        .await
        .map_err(sqlite("count the knowledge index"))?;
    Ok(KnowledgeStats {
        files: u64::try_from(files).unwrap_or(0),
        chunks: u64::try_from(chunks).unwrap_or(0),
    })
}

/// The words of a question that can be looked up: letters and digits, lower case, each once, the first few only.
fn terms(query: &str) -> Vec<String> {
    let mut found: Vec<String> = Vec::new();
    for word in query.split(|character: char| !character.is_alphanumeric()) {
        let word = word.to_lowercase();
        if !word.is_empty() && !found.contains(&word) {
            found.push(word);
            if found.len() == MAX_QUERY_TERMS {
                break;
            }
        }
    }
    found
}

/// The best passages for a question, best first, at most `limit`.
///
/// Passages holding every word of the question come first; when none holds them all, passages holding any of the words are returned
/// instead, so a question phrased a little differently from the text still finds something.
///
/// # Errors
///
/// Returns [`DatabaseError::Sqlite`] when the read fails.
pub async fn search_knowledge(
    database: &SqliteDatabase,
    query: &str,
    limit: u32,
) -> Result<Vec<KnowledgeHit>, DatabaseError> {
    let terms = terms(query);
    if terms.is_empty() {
        return Ok(Vec::new());
    }
    // Every term is quoted, so nothing the owner or a model types can be read as search syntax.
    let quoted: Vec<String> = terms.iter().map(|term| format!("\"{term}\"")).collect();
    let all = quoted.join(" ");
    let hits = run_search(database, &all, limit).await?;
    if !hits.is_empty() || terms.len() == 1 {
        return Ok(hits);
    }
    run_search(database, &quoted.join(" OR "), limit).await
}

async fn run_search(
    database: &SqliteDatabase,
    expression: &str,
    limit: u32,
) -> Result<Vec<KnowledgeHit>, DatabaseError> {
    let rows = sqlx::query(
        "SELECT f.root AS root, f.path AS path, f.kind AS kind, c.ordinal AS ordinal, c.start_line AS start_line, \
                c.char_offset AS char_offset, knowledge_fts.text AS text, bm25(knowledge_fts) AS score \
         FROM knowledge_fts \
         JOIN knowledge_chunks c ON c.id = knowledge_fts.rowid \
         JOIN knowledge_files f ON f.id = c.file_id \
         WHERE knowledge_fts MATCH ?1 \
         ORDER BY score, f.path, c.ordinal \
         LIMIT ?2",
    )
    .bind(expression)
    .bind(i64::from(limit))
    .fetch_all(database.pool())
    .await
    .map_err(sqlite("search the knowledge index"))?;
    rows.iter()
        .map(|row| {
            let number = |name: &str| -> Result<u32, DatabaseError> {
                let value: i64 = row
                    .try_get(name)
                    .map_err(sqlite("search the knowledge index"))?;
                Ok(u32::try_from(value).unwrap_or(0))
            };
            Ok(KnowledgeHit {
                root: row
                    .try_get("root")
                    .map_err(sqlite("search the knowledge index"))?,
                path: row
                    .try_get("path")
                    .map_err(sqlite("search the knowledge index"))?,
                kind: row
                    .try_get("kind")
                    .map_err(sqlite("search the knowledge index"))?,
                ordinal: number("ordinal")?,
                start_line: number("start_line")?,
                char_offset: number("char_offset")?,
                text: row
                    .try_get("text")
                    .map_err(sqlite("search the knowledge index"))?,
                score: row
                    .try_get("score")
                    .map_err(sqlite("search the knowledge index"))?,
            })
        })
        .collect()
}

#[cfg(test)]
#[path = "knowledge_tests.rs"]
mod tests;
