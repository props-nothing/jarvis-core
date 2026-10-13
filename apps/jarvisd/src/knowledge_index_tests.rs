//! The knowledge index: what it takes in, what it must leave out, and what it removes. Over a real database and real folders.

use std::io::Write;
use std::path::{Path, PathBuf};

use jarvis_storage::{KnowledgeStats, knowledge_manifest, knowledge_stats, search_knowledge};

use super::*;

struct Scratch(PathBuf);

impl Drop for Scratch {
    fn drop(&mut self) {
        jarvis_core::remove_scratch_dir(&self.0);
    }
}

impl Scratch {
    fn new(prefix: &str) -> Self {
        let tag = jarvis_core::scratch_tag();
        let path = std::env::temp_dir().join(format!("{prefix}-{}", &tag[tag.len() - 12..]));
        std::fs::create_dir_all(&path).unwrap_or_else(|error| panic!("create scratch: {error}"));
        Self(path.canonicalize().unwrap_or(path))
    }

    fn write(&self, relative: &str, contents: &[u8]) {
        let path = self.0.join(relative);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap_or_else(|error| panic!("{error}"));
        }
        std::fs::write(path, contents).unwrap_or_else(|error| panic!("{error}"));
    }
}

async fn database(scratch: &Scratch) -> Arc<SqliteDatabase> {
    Arc::new(
        SqliteDatabase::open(&scratch.0.join("jarvis.sqlite3"))
            .await
            .unwrap_or_else(|error| panic!("open the database: {error}")),
    )
}

fn index_over(database: &Arc<SqliteDatabase>, folders: &[&Path]) -> KnowledgeIndex {
    let roots = WorkspaceRoots::new(folders.iter().copied())
        .unwrap_or_else(|error| panic!("roots: {error}"));
    KnowledgeIndex::new(Arc::clone(database), Arc::new(roots))
}

async fn pass(index: &KnowledgeIndex) -> PassSummary {
    index
        .pass(Duration::from_secs(30))
        .await
        .unwrap_or_else(|error| panic!("pass: {error}"))
}

async fn paths(database: &SqliteDatabase) -> Vec<String> {
    let mut paths: Vec<String> = knowledge_manifest(database)
        .await
        .unwrap_or_else(|error| panic!("{error}"))
        .into_iter()
        .map(|file| file.path)
        .collect();
    paths.sort();
    paths
}

fn docx(paragraphs: &[&str]) -> Vec<u8> {
    let body = paragraphs.iter().fold(String::new(), |mut body, text| {
        body.push_str("<w:p><w:r><w:t>");
        body.push_str(text);
        body.push_str("</w:t></w:r></w:p>");
        body
    });
    let xml = format!(
        r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>{body}</w:body></w:document>"#
    );
    let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    writer
        .start_file(
            "word/document.xml",
            zip::write::SimpleFileOptions::default(),
        )
        .unwrap_or_else(|error| panic!("{error}"));
    writer
        .write_all(xml.as_bytes())
        .unwrap_or_else(|error| panic!("{error}"));
    writer
        .finish()
        .unwrap_or_else(|error| panic!("{error}"))
        .into_inner()
}

#[test]
fn text_is_cut_into_pieces_that_say_where_they_start() {
    let text = "# Title\n\nFirst paragraph about apples.\n\n".to_owned()
        + &"A line of padding that goes on.\n".repeat(60)
        + "\nThe last paragraph mentions zebras.\n";
    let chunks = chunk_text(&text);
    assert!(chunks.len() >= 3, "{}", chunks.len());
    let letters: Vec<char> = text.chars().collect();
    for chunk in &chunks {
        assert!(chunk.text.chars().count() <= CHUNK_HARD_CHARS + CHUNK_TARGET_CHARS);
        // The offset points at the piece's own first words, so a reader can go straight back to them.
        let from: String = letters[chunk.char_offset as usize..].iter().collect();
        let head: String = chunk.text.chars().take(12).collect();
        assert!(from.trim_start().starts_with(&head), "{from:?} / {head:?}");
        let line_text = text
            .lines()
            .nth(chunk.start_line as usize - 1)
            .unwrap_or("");
        assert!(
            chunk.text.contains(line_text.trim()),
            "the piece starts on the line it says: {line_text:?}"
        );
    }
    assert_eq!(chunks[0].start_line, 1);
    assert!(
        chunks
            .last()
            .is_some_and(|chunk| chunk.text.contains("zebras"))
    );
}

#[test]
fn a_very_long_line_and_a_very_long_file_are_bounded() {
    let one_line = "word ".repeat(5_000);
    let chunks = chunk_text(&one_line);
    assert!(chunks.len() > 10);
    assert!(
        chunks
            .iter()
            .all(|chunk| chunk.text.chars().count() <= CHUNK_TARGET_CHARS)
    );

    let huge = "a sentence of some length here\n".repeat(100_000);
    assert_eq!(chunk_text(&huge).len(), MAX_CHUNKS_PER_FILE);
    assert!(chunk_text("   \n\n  \n").is_empty());
}

#[tokio::test]
async fn text_and_documents_become_searchable_with_their_place() {
    let scratch = Scratch::new("jki-take");
    let folder = Scratch::new("jki-take-files");
    folder.write(
        "notes/trip.md",
        b"Flight to Lisbon leaves Friday.\nHotel is near the harbour.\n",
    );
    folder.write(
        "offers/acme.docx",
        &docx(&["Offer for Acme", "The price is 4,200 euro excluding VAT."]),
    );
    let database = database(&scratch).await;
    let index = index_over(&database, &[&folder.0]);

    let summary = pass(&index).await;
    assert_eq!(summary.indexed, 2);
    assert!(!summary.incomplete);
    assert!(index.freshness().is_some());

    let hits = search_knowledge(&database, "lisbon flight", 5)
        .await
        .unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].path, "notes/trip.md");
    assert_eq!(hits[0].kind, "text");
    assert_eq!(hits[0].start_line, 1);

    let hits = search_knowledge(&database, "price euro", 5)
        .await
        .unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(hits.len(), 1, "{hits:?}");
    assert_eq!(hits[0].path, "offers/acme.docx");
    assert_eq!(hits[0].kind, "document");
    // The offset is into the text `read_document` returns, so reading from it lands on the passage.
    let bytes = std::fs::read(folder.0.join("offers/acme.docx")).unwrap_or_default();
    let extracted = extract("acme.docx", &bytes).unwrap_or_else(|error| panic!("{error}"));
    let from: String = extracted
        .text
        .chars()
        .skip(hits[0].char_offset as usize)
        .collect();
    assert!(
        from.trim_start()
            .starts_with(hits[0].text.chars().take(10).collect::<String>().as_str())
    );
}

/// **What must stay out stays out:** hidden files and folders, generated folders, links out of the folder, binary files, other file
/// types and files over the limit are never read into the index, so a search cannot return them.
#[tokio::test]
async fn what_is_hidden_generated_binary_or_too_big_is_never_indexed() {
    let scratch = Scratch::new("jki-leave");
    let folder = Scratch::new("jki-leave-files");
    let outside = Scratch::new("jki-leave-outside");
    folder.write("ok.txt", b"findable needle text");
    folder.write(".env", b"needle API_KEY=hunter2");
    folder.write(".ssh/notes.txt", b"needle in a hidden folder");
    folder.write("node_modules/dep/readme.md", b"needle in dependencies");
    folder.write("target/out.txt", b"needle in build output");
    folder.write("binary.txt", b"needle\0\0\0 binary");
    folder.write("photo.png", b"needle but not a text type");
    folder.write(
        "latin1.txt",
        &[0x6e, 0x65, 0x65, 0x64, 0x6c, 0x65, 0xe9, 0xff],
    );
    folder.write("huge.txt", "needle ".repeat(300_000).as_bytes());
    outside.write("secret.txt", b"needle outside the grant");
    #[cfg(windows)]
    let linked = std::process::Command::new("cmd")
        .arg("/C")
        .arg("mklink")
        .arg("/J")
        .arg(folder.0.join("escape"))
        .arg(&outside.0)
        .output()
        .is_ok_and(|output| output.status.success());
    #[cfg(unix)]
    let linked = std::os::unix::fs::symlink(&outside.0, folder.0.join("escape")).is_ok();
    assert!(
        linked,
        "the link is a fixture: without it this test proves nothing"
    );

    let database = database(&scratch).await;
    let index = index_over(&database, &[&folder.0]);
    let _ = pass(&index).await;

    // Files that are not text are recorded without any text, so they are not read again until they change; nothing else is recorded.
    assert_eq!(
        paths(&database).await,
        vec![
            "binary.txt".to_owned(),
            "latin1.txt".to_owned(),
            "ok.txt".to_owned()
        ]
    );
    assert_eq!(
        knowledge_stats(&database).await.unwrap_or_default().chunks,
        1,
        "only ok.txt holds any text"
    );
    let hits = search_knowledge(&database, "needle", 20)
        .await
        .unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(hits.len(), 1, "{hits:?}");
    assert_eq!(hits[0].path, "ok.txt");
}

#[tokio::test]
async fn a_changed_file_is_read_again_a_gone_one_is_removed_and_an_unchanged_one_is_left() {
    let scratch = Scratch::new("jki-change");
    let folder = Scratch::new("jki-change-files");
    folder.write("a.txt", b"the first wording");
    folder.write("b.txt", b"stays the same");
    let database = database(&scratch).await;
    let index = index_over(&database, &[&folder.0]);
    assert_eq!(pass(&index).await.indexed, 2);

    let quiet = pass(&index).await;
    assert_eq!((quiet.indexed, quiet.removed), (0, 0), "nothing changed");

    // A different size makes the change certain whatever the clock's resolution.
    folder.write("a.txt", b"the second and longer wording");
    let changed = pass(&index).await;
    assert_eq!((changed.indexed, changed.removed), (1, 0));
    assert!(
        search_knowledge(&database, "first", 5)
            .await
            .unwrap_or_default()
            .is_empty()
    );
    assert_eq!(
        search_knowledge(&database, "second", 5)
            .await
            .unwrap_or_default()
            .len(),
        1
    );

    std::fs::remove_file(folder.0.join("b.txt")).unwrap_or_else(|error| panic!("{error}"));
    let gone = pass(&index).await;
    assert_eq!(gone.removed, 1);
    assert_eq!(paths(&database).await, vec!["a.txt".to_owned()]);
    assert!(
        search_knowledge(&database, "stays", 5)
            .await
            .unwrap_or_default()
            .is_empty(),
        "text of a deleted file cannot be found"
    );
}

/// **A folder the owner took back leaves nothing behind:** the next pass over a different grant removes what the old one put in.
#[tokio::test]
async fn text_from_a_folder_that_is_no_longer_granted_is_removed() {
    let scratch = Scratch::new("jki-revoke");
    let old = Scratch::new("jki-revoke-old");
    let new = Scratch::new("jki-revoke-new");
    old.write("private.txt", b"confidential figures");
    new.write("public.txt", b"open figures");
    let database = database(&scratch).await;

    let before = index_over(&database, &[&old.0]);
    pass(&before).await;
    assert_eq!(paths(&database).await, vec!["private.txt".to_owned()]);

    let after = index_over(&database, &[&new.0]);
    let summary = pass(&after).await;
    assert_eq!(summary.removed, 1);
    assert_eq!(paths(&database).await, vec!["public.txt".to_owned()]);
    assert!(
        search_knowledge(&database, "confidential", 5)
            .await
            .unwrap_or_default()
            .is_empty()
    );

    // With no folder granted at all the whole index is emptied.
    assert_eq!(purge_when_ungranted(&database).await.unwrap_or(0), 1);
    assert_eq!(
        knowledge_stats(&database).await.unwrap_or_default(),
        KnowledgeStats::default()
    );
}

#[tokio::test]
async fn every_granted_folder_is_covered_and_an_earlier_folder_owns_a_shared_path() {
    let scratch = Scratch::new("jki-roots");
    let first = Scratch::new("jki-roots-one");
    let second = Scratch::new("jki-roots-two");
    first.write("shared.txt", b"from the first folder");
    second.write("shared.txt", b"from the second folder");
    second.write("only-second.txt", b"unique to the second folder");
    let database = database(&scratch).await;
    let index = index_over(&database, &[&first.0, &second.0]);
    pass(&index).await;

    assert_eq!(
        paths(&database).await,
        vec!["only-second.txt".to_owned(), "shared.txt".to_owned()]
    );
    let shared = search_knowledge(&database, "shared folder wording from", 5)
        .await
        .unwrap_or_default();
    assert!(
        shared
            .iter()
            .all(|hit| !hit.text.contains("second") || hit.path != "shared.txt"),
        "the file tools read shared.txt from the first folder, so the index must too: {shared:?}"
    );
    assert_eq!(index.root_count(), 2);
}

#[tokio::test]
async fn a_pass_that_runs_out_of_time_says_so_and_the_next_one_carries_on() {
    let scratch = Scratch::new("jki-budget");
    let folder = Scratch::new("jki-budget-files");
    for number in 0..20 {
        folder.write(
            &format!("f{number}.txt"),
            format!("note number {number}").as_bytes(),
        );
    }
    let database = database(&scratch).await;
    let index = index_over(&database, &[&folder.0]);

    let cut = index
        .pass(Duration::ZERO)
        .await
        .unwrap_or_else(|error| panic!("{error}"));
    assert!(cut.incomplete);
    assert_eq!(cut.indexed, 0);

    let rest = pass(&index).await;
    assert_eq!(rest.indexed, 20);
    assert!(!rest.incomplete);
}
