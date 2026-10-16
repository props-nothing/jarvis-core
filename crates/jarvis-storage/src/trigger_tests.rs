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
        must(SqliteDatabase::test_fixture(&directory.path().join(DEFAULT_DATABASE_FILENAME)).await);
    (directory, database)
}

fn new(query: &str) -> NewTrigger {
    NewTrigger {
        kind: TriggerKind::Mail,
        query: query.to_owned(),
        objective: "Note the subject in the project journal.".to_owned(),
        interval_seconds: 300,
        project_id: None,
        secret_hash: None,
    }
}

const HASH: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

fn webhook(hash: &str) -> NewTrigger {
    NewTrigger {
        kind: TriggerKind::Webhook,
        query: "ignored".to_owned(),
        interval_seconds: 5,
        secret_hash: Some(hash.to_owned()),
        ..new("")
    }
}

fn file(folder: &str) -> NewTrigger {
    NewTrigger {
        kind: TriggerKind::File,
        ..new(folder)
    }
}

/// **A webhook is made from a secret's hash and is never polled**; a file trigger needs a plain relative folder; nothing else carries a hash.
#[tokio::test]
async fn the_kinds_are_validated_and_a_webhook_is_never_polled() {
    let (_directory, database) = database().await;
    let hook = must(create_trigger(&database, LOCAL_WORKSPACE_ID, &webhook(HASH), at(0)).await);
    assert_eq!(hook.kind, TriggerKind::Webhook);
    assert_eq!(hook.query, "webhook", "what the caller sent is not stored");
    assert_eq!(hook.interval_seconds, MAX_TRIGGER_INTERVAL_SECONDS);
    assert!(hook.baselined, "there is nothing to baseline");
    // Never due, however late.
    assert!(must(claim_due_triggers(&database, at(10_000_000), 10).await).is_empty());
    // Found by the hash of its secret, and by nothing else.
    assert_eq!(
        must(find_webhook_by_secret_hash(&database, HASH).await).id,
        hook.id
    );
    assert!(matches!(
        find_webhook_by_secret_hash(&database, &"f".repeat(64)).await,
        Err(DatabaseError::TriggerNotFound)
    ));
    // The hash is unique, so two webhooks cannot share an address.
    assert!(
        create_trigger(&database, LOCAL_WORKSPACE_ID, &webhook(HASH), at(1))
            .await
            .is_err()
    );
    // A webhook without a good hash, and a mail or file trigger with one, are refused.
    for bad in ["", "abc", &"G".repeat(64), &"A".repeat(64)] {
        assert!(matches!(
            create_trigger(&database, LOCAL_WORKSPACE_ID, &webhook(bad), at(0)).await,
            Err(DatabaseError::InvalidTrigger { field: "secret" })
        ));
    }
    let with_hash = NewTrigger {
        secret_hash: Some(HASH.to_owned()),
        ..new("is:unread")
    };
    assert!(matches!(
        create_trigger(&database, LOCAL_WORKSPACE_ID, &with_hash, at(0)).await,
        Err(DatabaseError::InvalidTrigger { field: "secret" })
    ));

    let folder =
        must(create_trigger(&database, LOCAL_WORKSPACE_ID, &file("inbox/new"), at(0)).await);
    assert_eq!(folder.kind, TriggerKind::File);
    assert!(!folder.baselined && folder.enabled);
    for bad in [
        "", "/etc", "../up", "a/../b", "a//b", "C:\\x", "a\\b", "x\ny", "name;rm",
    ] {
        assert!(
            matches!(
                create_trigger(&database, LOCAL_WORKSPACE_ID, &file(bad), at(0)).await,
                Err(DatabaseError::InvalidTrigger { field: "query" })
            ),
            "{bad:?} must be refused"
        );
    }
    assert!(valid_trigger_folder("Scans (2026)/to do"));
}

/// Resuming a paused webhook leaves it baselined (it has no baseline), and the dedupe table still enforces its foreign key after the
/// migration rebuilt it.
#[tokio::test]
async fn a_resumed_webhook_stays_ready_and_events_still_need_a_trigger() {
    let (_directory, database) = database().await;
    let hook = must(create_trigger(&database, LOCAL_WORKSPACE_ID, &webhook(HASH), at(0)).await);
    must(set_trigger_enabled(&database, LOCAL_WORKSPACE_ID, &hook.id, false, at(5)).await);
    let resumed =
        must(set_trigger_enabled(&database, LOCAL_WORKSPACE_ID, &hook.id, true, at(10)).await);
    assert!(resumed.enabled && resumed.baselined);
    assert!(must(
        claim_trigger_event(&database, &hook.id, "delivery-1", at(11)).await
    ));
    assert!(!must(
        claim_trigger_event(&database, &hook.id, "delivery-1", at(12)).await
    ));
    assert!(
        claim_trigger_event(
            &database,
            "00000000-0000-0000-0000-000000000000",
            "x",
            at(12)
        )
        .await
        .is_err(),
        "an event needs a trigger"
    );
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
    assert_eq!(made.kind, TriggerKind::Mail);
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

async fn webhook_event_count(database: &SqliteDatabase, trigger: &str) -> i64 {
    must(
        sqlx::query_scalar("SELECT COUNT(*) FROM trigger_events WHERE trigger_id = ?1")
            .bind(trigger)
            .fetch_one(database.pool())
            .await,
    )
}

/// Both budgets count pending reservations, not only rows linked to a run. Twenty-four callers cannot share the last slot.
#[tokio::test]
async fn concurrent_webhook_admissions_reserve_only_the_remaining_minute_or_day_slot() {
    for (existing, seeded_at) in [(2, at(1000)), (49, at(800))] {
        let (_directory, database) = database().await;
        let made = must(create_trigger(&database, LOCAL_WORKSPACE_ID, &webhook(HASH), at(0)).await);
        for index in 0..existing {
            must(
                claim_trigger_event(&database, &made.id, &format!("old-{index}"), seeded_at).await,
            );
        }
        let database = std::sync::Arc::new(database);
        let barrier = std::sync::Arc::new(tokio::sync::Barrier::new(24));
        let mut tasks = Vec::new();
        for index in 0..24 {
            let database = std::sync::Arc::clone(&database);
            let barrier = std::sync::Arc::clone(&barrier);
            let trigger = made.id.clone();
            tasks.push(tokio::spawn(async move {
                barrier.wait().await;
                claim_webhook_event(
                    &database,
                    &trigger,
                    &format!("new-{index}"),
                    at(1000),
                    3,
                    50,
                )
                .await
            }));
        }
        let mut admitted = 0;
        let mut limited = 0;
        for task in tasks {
            match must(must(task.await)) {
                WebhookEventAdmission::Admitted => admitted += 1,
                WebhookEventAdmission::RateLimited => limited += 1,
                WebhookEventAdmission::Duplicate => panic!("the keys are distinct"),
            }
        }
        assert_eq!((admitted, limited), (1, 23), "existing claims: {existing}");
        assert_eq!(webhook_event_count(&database, &made.id).await, existing + 1);
        assert_eq!(
            must(count_trigger_runs_since(&database, &made.id, at(0)).await),
            0,
            "none of the claims needs a run link to reserve its budget"
        );
        database.close().await;
    }
}

/// Concurrent retries share one reservation, and remain duplicates when that reservation fills the budget.
#[tokio::test]
async fn concurrent_webhook_duplicates_consume_one_slot_even_at_capacity() {
    let (_directory, database) = database().await;
    let made = must(create_trigger(&database, LOCAL_WORKSPACE_ID, &webhook(HASH), at(0)).await);
    let database = std::sync::Arc::new(database);
    let barrier = std::sync::Arc::new(tokio::sync::Barrier::new(12));
    let mut tasks = Vec::new();
    for _ in 0..12 {
        let database = std::sync::Arc::clone(&database);
        let barrier = std::sync::Arc::clone(&barrier);
        let trigger = made.id.clone();
        tasks.push(tokio::spawn(async move {
            barrier.wait().await;
            claim_webhook_event(&database, &trigger, "same-delivery", at(10), 1, 1).await
        }));
    }
    let mut admitted = 0;
    let mut duplicates = 0;
    for task in tasks {
        match must(must(task.await)) {
            WebhookEventAdmission::Admitted => admitted += 1,
            WebhookEventAdmission::Duplicate => duplicates += 1,
            WebhookEventAdmission::RateLimited => panic!("a duplicate does not spend capacity"),
        }
    }
    assert_eq!((admitted, duplicates), (1, 11));
    assert_eq!(webhook_event_count(&database, &made.id).await, 1);
    assert_eq!(
        must(claim_webhook_event(&database, &made.id, "different", at(10), 1, 1).await),
        WebhookEventAdmission::RateLimited
    );
    database.close().await;
}

/// A crash or failed start does not refund a committed reservation; only elapsed budget windows restore capacity.
#[tokio::test]
async fn webhook_reservations_survive_restart_without_a_run_and_expire_by_their_windows() {
    let (directory, database) = database().await;
    let made = must(create_trigger(&database, LOCAL_WORKSPACE_ID, &webhook(HASH), at(0)).await);
    assert_eq!(
        must(claim_webhook_event(&database, &made.id, "first", at(10), 1, 2).await),
        WebhookEventAdmission::Admitted
    );
    database.close().await;
    let reopened =
        must(SqliteDatabase::open(&directory.path().join(DEFAULT_DATABASE_FILENAME)).await);
    assert_eq!(
        must(claim_webhook_event(&reopened, &made.id, "first", at(11), 1, 2).await),
        WebhookEventAdmission::Duplicate
    );
    assert_eq!(
        must(claim_webhook_event(&reopened, &made.id, "second", at(11), 1, 2).await),
        WebhookEventAdmission::RateLimited
    );
    assert_eq!(
        must(claim_webhook_event(&reopened, &made.id, "second", at(71), 1, 2).await),
        WebhookEventAdmission::Admitted
    );
    assert_eq!(
        must(claim_webhook_event(&reopened, &made.id, "third", at(132), 1, 2).await),
        WebhookEventAdmission::RateLimited
    );
    assert_eq!(
        must(claim_webhook_event(&reopened, &made.id, "third", at(86_411), 1, 2).await),
        WebhookEventAdmission::Admitted
    );
    assert_eq!(
        must(claim_webhook_event(&reopened, &made.id, "first", at(86_412), 1, 2).await),
        WebhookEventAdmission::Duplicate,
        "expiration frees budget, not delivery identity"
    );
    reopened.close().await;
}

/// Storage failure never yields admission, and an aborted insert leaves the key available for a successful retry.
#[tokio::test]
async fn webhook_reservation_fails_closed_on_database_errors() {
    let (_directory, database) = database().await;
    let made = must(create_trigger(&database, LOCAL_WORKSPACE_ID, &webhook(HASH), at(0)).await);
    must(sqlx::query("CREATE TRIGGER reject_webhook_claim BEFORE INSERT ON trigger_events BEGIN SELECT RAISE(ABORT, 'synthetic write failure'); END")
        .execute(database.pool()).await);
    assert!(matches!(
        claim_webhook_event(&database, &made.id, "delivery", at(1), 3, 50).await,
        Err(DatabaseError::Sqlite { .. })
    ));
    assert_eq!(webhook_event_count(&database, &made.id).await, 0);
    must(
        sqlx::query("DROP TRIGGER reject_webhook_claim")
            .execute(database.pool())
            .await,
    );
    assert_eq!(
        must(claim_webhook_event(&database, &made.id, "delivery", at(2), 3, 50).await),
        WebhookEventAdmission::Admitted
    );
    database.close().await;
    assert!(matches!(
        claim_webhook_event(&database, &made.id, "another", at(3), 3, 50).await,
        Err(DatabaseError::Sqlite { .. })
    ));
}

#[tokio::test]
async fn webhook_reservation_requires_a_currently_enabled_webhook_and_valid_key() {
    let (_directory, database) = database().await;
    let mail = must(create_trigger(&database, LOCAL_WORKSPACE_ID, &new("is:unread"), at(0)).await);
    let hook = must(create_trigger(&database, LOCAL_WORKSPACE_ID, &webhook(HASH), at(0)).await);
    for bad in [String::new(), "k".repeat(129)] {
        assert!(matches!(
            claim_webhook_event(&database, &hook.id, &bad, at(1), 3, 50).await,
            Err(DatabaseError::InvalidTrigger { .. })
        ));
    }
    must(set_trigger_enabled(&database, LOCAL_WORKSPACE_ID, &hook.id, false, at(1)).await);
    for id in [&mail.id, &hook.id, "missing"] {
        assert!(matches!(
            claim_webhook_event(&database, id, "delivery", at(2), 3, 50).await,
            Err(DatabaseError::TriggerNotFound)
        ));
    }
    assert_eq!(webhook_event_count(&database, &hook.id).await, 0);
    database.close().await;
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
