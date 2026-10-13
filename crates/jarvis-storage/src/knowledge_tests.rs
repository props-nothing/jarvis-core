//! The knowledge index: replaced as a whole, searched by words, emptied on request, and never run by what a query says.

use std::path::{Path, PathBuf};

use super::*;
use crate::DEFAULT_DATABASE_FILENAME;

struct TestDirectory(PathBuf);

impl TestDirectory {
    fn new() -> Self {
        let path =
            std::env::temp_dir().join(format!("jarvis-knowledge-{}", jarvis_core::scratch_tag()));
        must(std::fs::create_dir_all(&path));
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TestDirectory {
    fn drop(&mut self) {
        jarvis_core::remove_scratch_dir(&self.0);
    }
}

fn must<T, E: std::fmt::Debug>(result: Result<T, E>) -> T {
    match result {
        Ok(value) => value,
        Err(error) => panic!("expected success, got {error:?}"),
    }
}

fn now() -> UtcTimestamp {
    must(UtcTimestamp::from_unix_nanos(1_774_000_000_000_000_000))
}

async fn database() -> (TestDirectory, SqliteDatabase) {
    let directory = TestDirectory::new();
    let database =
        must(SqliteDatabase::open(&directory.path().join(DEFAULT_DATABASE_FILENAME)).await);
    (directory, database)
}

fn chunk(text: &str) -> KnowledgeChunkInput {
    KnowledgeChunkInput {
        start_line: 1,
        char_offset: 0,
        text: text.to_owned(),
    }
}

async fn put(database: &SqliteDatabase, path: &str, texts: &[&str]) {
    let chunks: Vec<KnowledgeChunkInput> = texts.iter().map(|text| chunk(text)).collect();
    must(
        replace_knowledge_file(
            database,
            &KnowledgeFileInput {
                root: "C:/notes",
                path,
                kind: KnowledgeKind::Text,
                size_bytes: 10,
                modified_nanos: 5,
                chunks: &chunks,
                now: now(),
            },
        )
        .await,
    );
}

#[tokio::test]
async fn a_question_finds_the_passage_and_the_file_it_came_from() {
    let (_directory, database) = database().await;
    put(
        &database,
        "clients/acme.md",
        &[
            "The Acme contract renews in March.",
            "Invoices go to finance.",
        ],
    )
    .await;
    put(&database, "misc.txt", &["Groceries: milk, eggs."]).await;

    let hits = must(search_knowledge(&database, "when does the acme contract renew", 5).await);
    assert_eq!(hits.len(), 1, "{hits:?}");
    assert_eq!(hits[0].path, "clients/acme.md");
    assert_eq!(hits[0].root, "C:/notes");
    assert_eq!(hits[0].ordinal, 0);
    assert!(hits[0].text.contains("renews in March"));
}

#[tokio::test]
async fn every_word_is_wanted_first_and_any_word_is_the_fallback() {
    let (_directory, database) = database().await;
    put(&database, "a.txt", &["alpha beta gamma"]).await;
    put(&database, "b.txt", &["alpha only here"]).await;

    let both = must(search_knowledge(&database, "alpha gamma", 5).await);
    assert_eq!(both.len(), 1);
    assert_eq!(both[0].path, "a.txt");

    let fallback = must(search_knowledge(&database, "alpha delta", 5).await);
    assert_eq!(
        fallback.len(),
        2,
        "no passage has both words, so either will do"
    );
}

#[tokio::test]
async fn replacing_a_file_leaves_none_of_its_old_text_searchable() {
    let (_directory, database) = database().await;
    put(&database, "a.txt", &["the old wording", "second piece"]).await;
    put(&database, "a.txt", &["the new wording"]).await;

    assert!(must(search_knowledge(&database, "old", 5).await).is_empty());
    assert!(must(search_knowledge(&database, "second", 5).await).is_empty());
    assert_eq!(must(search_knowledge(&database, "new", 5).await).len(), 1);
    assert_eq!(
        must(knowledge_stats(&database).await),
        KnowledgeStats {
            files: 1,
            chunks: 1
        }
    );
}

#[tokio::test]
async fn removing_and_clearing_take_the_text_out_of_every_search() {
    let (_directory, database) = database().await;
    put(&database, "a.txt", &["secret plan"]).await;
    put(&database, "b.txt", &["secret recipe"]).await;

    must(remove_knowledge_file(&database, "C:/notes", "a.txt").await);
    let hits = must(search_knowledge(&database, "secret", 5).await);
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].path, "b.txt");

    assert_eq!(must(clear_knowledge(&database).await), 1);
    assert!(must(search_knowledge(&database, "secret", 5).await).is_empty());
    assert_eq!(
        must(knowledge_stats(&database).await),
        KnowledgeStats::default()
    );
}

#[tokio::test]
async fn the_manifest_says_what_was_read_and_when_it_last_changed() {
    let (_directory, database) = database().await;
    put(&database, "a.txt", &["text"]).await;
    let manifest = must(knowledge_manifest(&database).await);
    assert_eq!(
        manifest,
        vec![KnowledgeFileState {
            root: "C:/notes".to_owned(),
            path: "a.txt".to_owned(),
            size_bytes: 10,
            modified_nanos: 5,
        }]
    );
}

/// Search syntax in a question is only words: a quote, a column filter or an operator cannot widen or break the lookup.
#[tokio::test]
async fn a_query_is_words_and_never_search_syntax() {
    let (_directory, database) = database().await;
    put(&database, "a.txt", &["plain text about budgets"]).await;

    for hostile in [
        "\" OR 1=1 --",
        "text:budgets NOT plain",
        "budgets*",
        "(((",
        "NEAR(budgets text)",
        "\"",
    ] {
        let result = search_knowledge(&database, hostile, 5).await;
        assert!(
            result.is_ok(),
            "{hostile:?} must not be an error: {result:?}"
        );
    }
    assert!(must(search_knowledge(&database, "   ", 5).await).is_empty());
    assert!(must(search_knowledge(&database, "!!!", 5).await).is_empty());
}

#[tokio::test]
async fn an_empty_or_oversized_piece_is_refused_and_leaves_the_old_text_in_place() {
    let (_directory, database) = database().await;
    put(&database, "a.txt", &["keep me"]).await;
    let bad = [chunk("fine"), chunk("")];
    let refused = replace_knowledge_file(
        &database,
        &KnowledgeFileInput {
            root: "C:/notes",
            path: "a.txt",
            kind: KnowledgeKind::Text,
            size_bytes: 1,
            modified_nanos: 1,
            chunks: &bad,
            now: now(),
        },
    )
    .await;
    assert!(refused.is_err());
    assert_eq!(
        must(search_knowledge(&database, "keep", 5).await).len(),
        1,
        "a failed replacement is rolled back whole"
    );
}
