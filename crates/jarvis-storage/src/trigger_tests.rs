//! Trigger storage: bounds, the poll claim, and the event claim that makes a replay start nothing.

use std::path::{Path, PathBuf};

use super::*;
use crate::{DEFAULT_DATABASE_FILENAME, LOCAL_WORKSPACE_ID, SqliteDatabase};

struct TestDirectory(PathBuf);

impl TestDirectory {
    fn new() -> Self {
        let path =
            std::env::temp_dir().join(format!("jarvis-triggers-{}", jarvis_core::scratch_tag()));
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

fn at(seconds: i128) -> UtcTimestamp {
    must(UtcTimestamp::from_unix_nanos(
        1_774_000_000_000_000_000 + seconds * 1_000_000_000,
    ))
}

async fn database() -> (TestDirectory, SqliteDatabase) {
    let directory = TestDirectory::new();
    let database =
        must(SqliteDatabase::open(&directory.path().join(DEFAULT_DATABASE_FILENAME)).await);
    (directory, database)
}

fn new(query: &str) -> NewTrigger {
    NewTrigger {
        query: query.to_owned(),
        objective: "Note the subject in the project journal.".to_owned(),
        interval_seconds: 300,
        project_id: None,
    }
}

#[tokio::test]
async fn a_trigger_is_created_polls_one_interval_later_and_is_found_in_its_workspace_only() {
    let (_directory, database) = database().await;
    let made = must(
        create_trigger(
            &database,
            LOCAL_WORKSPACE_ID,
            &new("from:boss is:unread"),
            at(0),
        )
        .await,
    );
    assert_eq!(made.kind, "mail");
    assert_eq!(made.query, "from:boss is:unread");
    assert!(made.enabled && !made.baselined);
    assert_eq!(made.next_poll, at(300));
    assert_eq!(
        must(find_trigger(&database, LOCAL_WORKSPACE_ID, &made.id).await),
        made
    );
    assert!(matches!(
        find_trigger(&database, "another-workspace", &made.id).await,
        Err(DatabaseError::TriggerNotFound)
    ));
    assert_eq!(
        must(list_triggers(&database, LOCAL_WORKSPACE_ID).await).len(),
        1
    );
}

/// **Anything outside the bounds is refused**, so a stored row never contradicts the table's own checks.
#[tokio::test]
async fn unacceptable_triggers_are_refused() {
    let (_directory, database) = database().await;
    for query in [
        String::new(),
        "   ".to_owned(),
        "x".repeat(MAX_TRIGGER_QUERY_CHARS + 1),
        "a\nb".to_owned(),
    ] {
        assert!(matches!(
            create_trigger(&database, LOCAL_WORKSPACE_ID, &new(&query), at(0)).await,
            Err(DatabaseError::InvalidTrigger { field: "query" })
        ));
    }
    for interval in [
        0,
        MIN_TRIGGER_INTERVAL_SECONDS - 1,
        MAX_TRIGGER_INTERVAL_SECONDS + 1,
    ] {
        let trigger = NewTrigger {
            interval_seconds: interval,
            ..new("is:unread")
        };
        assert!(matches!(
            create_trigger(&database, LOCAL_WORKSPACE_ID, &trigger, at(0)).await,
            Err(DatabaseError::InvalidTrigger { field: "interval" })
        ));
    }
    let long = NewTrigger {
        objective: "o".repeat(MAX_TRIGGER_OBJECTIVE_CHARS + 1),
        ..new("is:unread")
    };
    assert!(matches!(
        create_trigger(&database, LOCAL_WORKSPACE_ID, &long, at(0)).await,
        Err(DatabaseError::InvalidTrigger { field: "objective" })
    ));
    for _ in 0..MAX_TRIGGERS_PER_WORKSPACE {
        must(create_trigger(&database, LOCAL_WORKSPACE_ID, &new("is:unread"), at(0)).await);
    }
    assert!(matches!(
        create_trigger(&database, LOCAL_WORKSPACE_ID, &new("is:unread"), at(0)).await,
        Err(DatabaseError::TriggerLimit)
    ));
}

/// **A poll is claimed once.** Two passes over the same due trigger poll it once between them, and the next poll moves a full interval on.
#[tokio::test]
async fn a_due_poll_is_claimed_by_exactly_one_pass_and_moves_on_by_an_interval() {
    let (_directory, database) = database().await;
    let made = must(create_trigger(&database, LOCAL_WORKSPACE_ID, &new("is:unread"), at(0)).await);

    assert!(must(claim_due_triggers(&database, at(299), 10).await).is_empty());
    let (first, second) = tokio::join!(
        claim_due_triggers(&database, at(300), 10),
        claim_due_triggers(&database, at(300), 10)
    );
    let claimed = must(first).len() + must(second).len();
    assert_eq!(claimed, 1, "one pass owns the poll");
    let after = must(find_trigger(&database, LOCAL_WORKSPACE_ID, &made.id).await);
    assert_eq!(after.next_poll, at(600));
}

/// **The event claim is the dedupe.** The first claim of a key wins; the same key again, from any pass, wins nothing.
#[tokio::test]
async fn an_event_is_claimed_once_and_a_replay_claims_nothing() {
    let (_directory, database) = database().await;
    let made = must(create_trigger(&database, LOCAL_WORKSPACE_ID, &new("is:unread"), at(0)).await);
    assert!(must(
        claim_trigger_event(&database, &made.id, "m1", at(10)).await
    ));
    assert!(!must(
        claim_trigger_event(&database, &made.id, "m1", at(20)).await
    ));
    assert!(must(
        claim_trigger_event(&database, &made.id, "m2", at(20)).await
    ));
    assert!(matches!(
        claim_trigger_event(&database, &made.id, "", at(20)).await,
        Err(DatabaseError::InvalidTrigger { .. })
    ));
    assert!(matches!(
        claim_trigger_event(&database, &made.id, &"k".repeat(129), at(20)).await,
        Err(DatabaseError::InvalidTrigger { .. })
    ));
}

/// Only an event that started a run counts toward the daily cap; a baseline entry does not, and the window is respected.
#[tokio::test]
async fn only_events_that_started_a_run_inside_the_window_count() {
    let (_directory, database) = database().await;
    let made = must(create_trigger(&database, LOCAL_WORKSPACE_ID, &new("is:unread"), at(0)).await);
    must(claim_trigger_event(&database, &made.id, "baseline", at(10)).await);
    must(claim_trigger_event(&database, &made.id, "old", at(20)).await);
    must(record_trigger_run(&database, &made.id, "old", "run-old").await);
    must(claim_trigger_event(&database, &made.id, "new", at(1000)).await);
    must(record_trigger_run(&database, &made.id, "new", "run-new").await);

    assert_eq!(
        must(count_trigger_runs_since(&database, &made.id, at(0)).await),
        2
    );
    assert_eq!(
        must(count_trigger_runs_since(&database, &made.id, at(500)).await),
        1
    );
    let after = must(find_trigger(&database, LOCAL_WORKSPACE_ID, &made.id).await);
    assert_eq!(after.fire_count, 2);
    assert_eq!(after.last_run_id.as_deref(), Some("run-new"));
}

/// Pausing stops the polls; resuming starts a fresh baseline, so what arrived while it was paused starts no burst of runs.
#[tokio::test]
async fn resuming_a_paused_trigger_baselines_again_and_deleting_removes_its_history() {
    let (_directory, database) = database().await;
    let made = must(create_trigger(&database, LOCAL_WORKSPACE_ID, &new("is:unread"), at(0)).await);
    must(mark_trigger_baselined(&database, &made.id).await);
    must(claim_trigger_event(&database, &made.id, "m1", at(1)).await);

    let paused =
        must(set_trigger_enabled(&database, LOCAL_WORKSPACE_ID, &made.id, false, at(10)).await);
    assert!(!paused.enabled);
    assert!(must(claim_due_triggers(&database, at(100_000), 10).await).is_empty());

    let resumed =
        must(set_trigger_enabled(&database, LOCAL_WORKSPACE_ID, &made.id, true, at(1000)).await);
    assert!(resumed.enabled && !resumed.baselined);
    assert_eq!(resumed.next_poll, at(1300));

    must(delete_trigger(&database, LOCAL_WORKSPACE_ID, &made.id).await);
    assert!(matches!(
        find_trigger(&database, LOCAL_WORKSPACE_ID, &made.id).await,
        Err(DatabaseError::TriggerNotFound)
    ));
    // A deleted trigger cannot be claimed against: its dedupe record went with it and the key constraint refuses a new one.
    assert!(
        claim_trigger_event(&database, &made.id, "m1", at(2000))
            .await
            .is_err()
    );
}

#[tokio::test]
async fn a_poll_records_when_it_happened_and_why_it_failed() {
    let (_directory, database) = database().await;
    let made = must(create_trigger(&database, LOCAL_WORKSPACE_ID, &new("is:unread"), at(0)).await);
    must(record_trigger_poll(&database, &made.id, at(5), Some("Google is not connected")).await);
    let failed = must(find_trigger(&database, LOCAL_WORKSPACE_ID, &made.id).await);
    assert_eq!(
        failed.last_error.as_deref(),
        Some("Google is not connected")
    );
    assert_eq!(failed.last_polled_at, Some(at(5)));
    must(record_trigger_poll(&database, &made.id, at(6), None).await);
    let worked = must(find_trigger(&database, LOCAL_WORKSPACE_ID, &made.id).await);
    assert_eq!(worked.last_error, None);
}
