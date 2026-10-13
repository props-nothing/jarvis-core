//! The indexer behind `jarvis.knowledge.search` (`ADR-0206`).
//!
//! It keeps a full-text index of the text in the folders the owner granted, so a question about a file nobody opened can be answered
//! with the passage and the file it came from. The index is derived data and the granted folders stay the source of truth, which is why
//! every rule here fails toward holding less:
//!
//! - **Only granted folders, only through their handles.** The walk and every read go through [`WorkspaceRoots`], the same confinement
//!   the file tools use, so a link out of a folder is never followed. Links, names that start with a dot (`.env`, `.ssh`), generated
//!   folders and files that are not text or a document are left out.
//! - **What is no longer granted is removed.** An entry whose folder is not granted any more is deleted at the next pass, and so is an
//!   entry whose file is gone; with no folder granted at all the whole index is emptied when the daemon starts ([`purge_when_ungranted`]).
//! - **A changed file is read again, an unchanged one is not.** A file is compared by size and modification time with what was stored.
//! - **Everything is bounded:** files, bytes per file, pieces per file and time per pass. A pass that runs out of time stops and the
//!   next one carries on, so a huge tree is indexed over several passes and never blocks anything.
//!
//! Text from documents is parsed by `jarvis-documents`, which treats every document as hostile.

use std::collections::{HashMap, HashSet};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use jarvis_core::{SystemClock, UtcTimestamp};
use jarvis_documents::{MAX_INPUT_BYTES, extract};
use jarvis_storage::{
    KnowledgeChunkInput, KnowledgeFileInput, KnowledgeKind, MAX_CHUNK_CHARS, SqliteDatabase,
    knowledge_manifest, remove_knowledge_file, replace_knowledge_file,
};
use jarvis_tools::WorkspaceRoots;
use thiserror::Error;

/// How often the background pass runs.
pub const PASS_INTERVAL: Duration = Duration::from_secs(300);
/// How long a pass may run before it stops and leaves the rest to the next one.
pub const PASS_BUDGET: Duration = Duration::from_secs(60);

/// The most files the index holds. A tree beyond this is indexed in part, and the answer says so.
const MAX_FILES: usize = 20_000;
/// The deepest folder a walk enters.
const MAX_DEPTH: usize = 12;
/// The largest text file read, in bytes.
const MAX_TEXT_BYTES: u64 = 1024 * 1024;
/// The most pieces one file contributes, so one huge file cannot crowd out the rest.
const MAX_CHUNKS_PER_FILE: usize = 400;
/// A piece is cut at a line end once it holds this many characters.
const CHUNK_TARGET_CHARS: usize = 1_200;
/// A line longer than this is cut into pieces of this size.
const CHUNK_HARD_CHARS: usize = 1_800;
/// A paragraph break ends a piece early once it holds this many characters.
const CHUNK_SOFT_CHARS: usize = 500;
/// How long parsing a document may take before it is given up on. The parsers are not interruptible, so one that hangs is abandoned on
/// its own thread.
const PARSE_SECONDS: u64 = 25;

/// Folders a walk never enters: version control, dependencies and build output.
const SKIPPED_DIRECTORIES: [&str; 9] = [
    "node_modules",
    "target",
    "dist",
    "build",
    "__pycache__",
    "venv",
    "vendor",
    "site-packages",
    "$recycle.bin",
];

const TEXT_EXTENSIONS: [&str; 34] = [
    "txt", "md", "markdown", "rst", "org", "csv", "tsv", "json", "jsonl", "toml", "yaml", "yml",
    "ini", "cfg", "conf", "log", "xml", "html", "htm", "rs", "py", "js", "ts", "tsx", "jsx", "go",
    "java", "c", "h", "cpp", "cs", "sh", "ps1", "sql",
];
const DOCUMENT_EXTENSIONS: [&str; 4] = ["pdf", "docx", "xlsx", "pptx"];

/// Why a pass could not run.
#[derive(Debug, Error)]
pub enum KnowledgeError {
    /// The index could not be read or written.
    #[error("the knowledge index is not available")]
    Storage(#[from] jarvis_storage::DatabaseError),
    /// The walk of the granted folders could not run.
    #[error("the granted folders could not be walked")]
    Walk,
}

/// What a pass did.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PassSummary {
    /// Files the walk found that may be indexed.
    pub found: usize,
    /// Files read and put in the index.
    pub indexed: usize,
    /// Entries taken out because the file is gone, is shadowed, or its folder is not granted any more.
    pub removed: usize,
    /// Whether the pass stopped early, for time or because the tree holds more files than the index may.
    pub incomplete: bool,
}

/// Where the last pass stands, for the tool to say.
#[derive(Clone, Copy, Debug)]
pub struct Freshness {
    /// When the last pass finished.
    pub finished_at: Instant,
    /// What it did.
    pub summary: PassSummary,
}

/// The index over the granted folders.
pub struct KnowledgeIndex {
    database: Arc<SqliteDatabase>,
    roots: Arc<WorkspaceRoots>,
    /// Held for the length of a pass, so two never run at once.
    running: tokio::sync::Mutex<()>,
    last: Mutex<Option<Freshness>>,
}

struct Candidate {
    root_index: usize,
    root: String,
    path: String,
    kind: KnowledgeKind,
    size: u64,
    modified_nanos: i64,
}

struct Walked {
    files: Vec<Candidate>,
    /// Whether the walk covered everything (not cut short by the file limit).
    complete: bool,
}

impl KnowledgeIndex {
    /// Builds the index over the daemon's database and the granted folders.
    #[must_use]
    pub fn new(database: Arc<SqliteDatabase>, roots: Arc<WorkspaceRoots>) -> Self {
        Self {
            database,
            roots,
            running: tokio::sync::Mutex::new(()),
            last: Mutex::new(None),
        }
    }

    /// The database, for the tool's searches.
    #[must_use]
    pub fn database(&self) -> &SqliteDatabase {
        &self.database
    }

    /// How many folders are granted.
    #[must_use]
    pub fn root_count(&self) -> usize {
        self.roots.len()
    }

    /// Whether a pass is running now.
    #[must_use]
    pub fn is_running(&self) -> bool {
        self.running.try_lock().is_err()
    }

    /// How the last pass ended, or `None` before the first one has finished.
    #[must_use]
    pub fn freshness(&self) -> Option<Freshness> {
        self.last.lock().ok().and_then(|held| *held)
    }

    /// Runs one pass: walks the granted folders, reads what is new or changed, and removes what no longer belongs.
    ///
    /// Waits if another pass is running, then runs its own.
    ///
    /// # Errors
    ///
    /// Returns [`KnowledgeError`] when the index cannot be written or the walk cannot run.
    pub async fn pass(&self, budget: Duration) -> Result<PassSummary, KnowledgeError> {
        let _running = self.running.lock().await;
        let started = Instant::now();
        let roots = Arc::clone(&self.roots);
        let walked = tokio::task::spawn_blocking(move || walk(&roots, started + budget))
            .await
            .map_err(|_| KnowledgeError::Walk)?;
        let manifest = knowledge_manifest(&self.database).await?;
        let known: HashMap<(&str, &str), (u64, i64)> = manifest
            .iter()
            .map(|file| {
                (
                    (file.root.as_str(), file.path.as_str()),
                    (file.size_bytes, file.modified_nanos),
                )
            })
            .collect();
        let seen: HashSet<(&str, &str)> = walked
            .files
            .iter()
            .map(|file| (file.root.as_str(), file.path.as_str()))
            .collect();
        let granted: HashSet<String> = (0..self.roots.len())
            .filter_map(|index| self.roots.root_label(index))
            .map(|label| label.display().to_string())
            .collect();

        let mut summary = PassSummary {
            found: walked.files.len(),
            incomplete: !walked.complete,
            ..PassSummary::default()
        };
        for file in &manifest {
            let ungranted = !granted.contains(&file.root);
            let gone = walked.complete && !seen.contains(&(file.root.as_str(), file.path.as_str()));
            if ungranted || gone {
                remove_knowledge_file(&self.database, &file.root, &file.path).await?;
                summary.removed += 1;
            }
        }
        for candidate in &walked.files {
            if known.get(&(candidate.root.as_str(), candidate.path.as_str()))
                == Some(&(candidate.size, candidate.modified_nanos))
            {
                continue;
            }
            if started.elapsed() >= budget {
                summary.incomplete = true;
                break;
            }
            self.index_file(candidate).await?;
            summary.indexed += 1;
        }
        if let Ok(mut held) = self.last.lock() {
            *held = Some(Freshness {
                finished_at: Instant::now(),
                summary,
            });
        }
        Ok(summary)
    }

    /// Reads one file and replaces its entry. A file that cannot be read or holds no text is still recorded, with no pieces, so it is
    /// not read again until it changes.
    async fn index_file(&self, candidate: &Candidate) -> Result<(), KnowledgeError> {
        let roots = Arc::clone(&self.roots);
        let (root_index, path, kind, size) = (
            candidate.root_index,
            PathBuf::from(&candidate.path),
            candidate.kind,
            candidate.size,
        );
        let chunks = match read_text(&roots, root_index, &path, kind, size).await {
            Some(text) => chunk_text(&text),
            None => Vec::new(),
        };
        replace_knowledge_file(
            &self.database,
            &KnowledgeFileInput {
                root: &candidate.root,
                path: &candidate.path,
                kind: candidate.kind,
                size_bytes: candidate.size,
                modified_nanos: candidate.modified_nanos,
                chunks: &chunks,
                now: UtcTimestamp::now(&SystemClock),
            },
        )
        .await?;
        Ok(())
    }
}

/// Empties the index when no folder is granted: the text in it came from folders the owner has since taken back.
///
/// # Errors
///
/// Returns [`KnowledgeError::Storage`] when the index cannot be written.
pub async fn purge_when_ungranted(database: &SqliteDatabase) -> Result<u64, KnowledgeError> {
    Ok(jarvis_storage::clear_knowledge(database).await?)
}

/// Runs a pass at the start and then at every interval, for as long as the index is alive. The task holds only a weak reference, so it
/// ends when the daemon lets go of the index.
pub fn spawn_passes(index: &Arc<KnowledgeIndex>) {
    let weak = Arc::downgrade(index);
    tokio::spawn(async move {
        loop {
            let Some(index) = weak.upgrade() else {
                return;
            };
            if let Err(error) = index.pass(PASS_BUDGET).await {
                tracing::warn!(%error, "the knowledge index could not be brought up to date");
            }
            drop(index);
            tokio::time::sleep(PASS_INTERVAL).await;
        }
    });
}

/// Whether a name is one a walk leaves out: hidden (a leading dot) or a folder of generated files.
fn skipped(name: &str, is_directory: bool) -> bool {
    name.starts_with('.')
        || (is_directory
            && SKIPPED_DIRECTORIES
                .iter()
                .any(|skipped| skipped.eq_ignore_ascii_case(name)))
}

fn kind_of(name: &str) -> Option<KnowledgeKind> {
    let extension = name.rsplit_once('.')?.1.to_ascii_lowercase();
    if TEXT_EXTENSIONS.contains(&extension.as_str()) {
        Some(KnowledgeKind::Text)
    } else if DOCUMENT_EXTENSIONS.contains(&extension.as_str()) {
        Some(KnowledgeKind::Document)
    } else {
        None
    }
}

/// Lists the files that may be indexed in every granted folder, in grant order. A path an earlier folder already has is left to it,
/// because that is the one the file tools would read.
fn walk(roots: &WorkspaceRoots, deadline: Instant) -> Walked {
    let mut files: Vec<Candidate> = Vec::new();
    let mut claimed: HashSet<String> = HashSet::new();
    let mut complete = true;
    'roots: for root_index in 0..roots.len() {
        let Some(label) = roots.root_label(root_index) else {
            continue;
        };
        let root = label.display().to_string();
        let mut pending: Vec<(PathBuf, usize)> = vec![(PathBuf::from("."), 0)];
        while let Some((folder, depth)) = pending.pop() {
            if Instant::now() >= deadline {
                complete = false;
                break 'roots;
            }
            let Ok(entries) = roots.list_in_root(root_index, &folder) else {
                continue;
            };
            for entry in entries {
                let Some(name) = entry.name else { continue };
                if entry.is_link || skipped(&name, entry.is_directory) {
                    continue;
                }
                let relative = if folder == Path::new(".") {
                    PathBuf::from(&name)
                } else {
                    folder.join(&name)
                };
                if entry.is_directory {
                    if depth < MAX_DEPTH {
                        pending.push((relative, depth + 1));
                    }
                    continue;
                }
                let Some(kind) = kind_of(&name) else { continue };
                let limit = match kind {
                    KnowledgeKind::Text => MAX_TEXT_BYTES,
                    KnowledgeKind::Document => MAX_INPUT_BYTES as u64,
                };
                if entry.len > limit {
                    continue;
                }
                let path = relative
                    .components()
                    .map(|part| part.as_os_str().to_string_lossy())
                    .collect::<Vec<_>>()
                    .join("/");
                if !claimed.insert(path.clone()) {
                    continue;
                }
                if files.len() >= MAX_FILES {
                    complete = false;
                    break 'roots;
                }
                files.push(Candidate {
                    root_index,
                    root: root.clone(),
                    path,
                    kind,
                    size: entry.len,
                    modified_nanos: entry.modified_nanos.unwrap_or(0),
                });
            }
        }
    }
    Walked { files, complete }
}

/// The text of a file: read as UTF-8 text, or parsed as a document. `None` when it cannot be read or holds no text.
async fn read_text(
    roots: &Arc<WorkspaceRoots>,
    root_index: usize,
    path: &Path,
    kind: KnowledgeKind,
    size: u64,
) -> Option<String> {
    let roots = Arc::clone(roots);
    let owned = path.to_path_buf();
    let limit = match kind {
        KnowledgeKind::Text => MAX_TEXT_BYTES,
        KnowledgeKind::Document => MAX_INPUT_BYTES as u64,
    };
    let bytes = tokio::task::spawn_blocking(move || {
        let file = roots.open_file_in_root(root_index, &owned).ok()?;
        let mut bytes = Vec::with_capacity(usize::try_from(size.min(limit)).unwrap_or(0));
        file.take(limit + 1).read_to_end(&mut bytes).ok()?;
        (bytes.len() as u64 <= limit).then_some(bytes)
    })
    .await
    .ok()??;
    match kind {
        KnowledgeKind::Text => {
            // A NUL early on is the mark of a binary file, and what is not UTF-8 is not text to search.
            if bytes.iter().take(8192).any(|byte| *byte == 0) {
                return None;
            }
            String::from_utf8(bytes).ok()
        }
        KnowledgeKind::Document => {
            let name = path.to_string_lossy().into_owned();
            let parsed = tokio::time::timeout(
                Duration::from_secs(PARSE_SECONDS),
                tokio::task::spawn_blocking(move || extract(&name, &bytes)),
            )
            .await
            .ok()?
            .ok()?;
            parsed.ok().map(|extracted| extracted.text)
        }
    }
}

/// Cuts text into pieces a few paragraphs long, noting where each starts (its line and its character offset).
///
/// Cuts fall at line ends, preferably at a blank line; a line too long to be one piece is cut by characters. At most
/// [`MAX_CHUNKS_PER_FILE`] pieces are returned.
#[must_use]
pub fn chunk_text(text: &str) -> Vec<KnowledgeChunkInput> {
    let mut chunker = Chunker::default();
    for line in text.split_inclusive('\n') {
        chunker.line(line);
        if chunker.chunks.len() >= MAX_CHUNKS_PER_FILE {
            break;
        }
    }
    chunker.finish()
}

/// Builds pieces line by line, remembering where the open piece began.
#[derive(Default)]
struct Chunker {
    chunks: Vec<KnowledgeChunkInput>,
    piece: String,
    piece_chars: usize,
    piece_line: u32,
    piece_offset: usize,
    /// Characters before the next line.
    offset: usize,
    /// Lines seen so far.
    lines: u32,
}

impl Chunker {
    fn line(&mut self, line: &str) {
        self.lines = self.lines.saturating_add(1);
        let line_chars = line.chars().count();
        if line_chars > CHUNK_HARD_CHARS {
            // A very long line (minified text, a one-line export) is cut by characters.
            self.flush();
            let letters: Vec<char> = line.chars().collect();
            for (index, part) in letters.chunks(CHUNK_TARGET_CHARS).enumerate() {
                self.piece_line = self.lines;
                self.piece_offset = self.offset + index * CHUNK_TARGET_CHARS;
                self.piece = part.iter().collect();
                self.flush();
            }
            self.offset += line_chars;
            return;
        }
        if self.piece_chars == 0 {
            self.piece_line = self.lines;
            self.piece_offset = self.offset;
        }
        self.piece.push_str(line);
        self.piece_chars += line_chars;
        self.offset += line_chars;
        let blank = line.trim().is_empty();
        if self.piece_chars >= CHUNK_TARGET_CHARS || (blank && self.piece_chars >= CHUNK_SOFT_CHARS)
        {
            self.flush();
        }
    }

    fn flush(&mut self) {
        let trimmed = self.piece.trim();
        if !trimmed.is_empty() {
            self.chunks.push(KnowledgeChunkInput {
                start_line: self.piece_line.max(1),
                char_offset: u32::try_from(self.piece_offset).unwrap_or(u32::MAX),
                text: trimmed.to_owned(),
            });
        }
        self.piece.clear();
        self.piece_chars = 0;
    }

    fn finish(mut self) -> Vec<KnowledgeChunkInput> {
        self.flush();
        self.chunks.truncate(MAX_CHUNKS_PER_FILE);
        debug_assert!(
            self.chunks
                .iter()
                .all(|chunk| chunk.text.chars().count() <= MAX_CHUNK_CHARS)
        );
        self.chunks
    }
}

#[cfg(test)]
#[path = "knowledge_index_tests.rs"]
mod tests;
