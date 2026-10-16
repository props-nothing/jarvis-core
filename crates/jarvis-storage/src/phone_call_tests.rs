//! Phone call records: written before the call, never regressed, counted for the rate limit, cleared on request.

use std::{
    fs,
    path::{Path, PathBuf},
};

use super::*;
use crate::{DEFAULT_DATABASE_FILENAME, LOCAL_WORKSPACE_ID, SqliteDatabase};

struct TestDirectory(PathBuf);

impl TestDirectory {
    fn new() -> Self {
        let path =
            std::env::temp_dir().join(format!("jarvis-phone-calls-{}", jarvis_core::scratch_tag()));
        must(fs::create_dir_all(&path));
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

fn new_call(now: UtcTimestamp) -> NewPhoneCall<'static> {
    NewPhoneCall {
        workspace_id: LOCAL_WORKSPACE_ID,
        number: "+31612345678",
        reason: "the build failed",
        urgent: false,
        now,
    }
}

fn update(status: &'static str) -> PhoneCallUpdate<'static> {
    PhoneCallUpdate {
        status,
        ..PhoneCallUpdate::default()
    }
}

#[tokio::test]
async fn a_call_is_recorded_before_the_provider_is_asked_and_then_attached() {
    let (_directory, database) = database().await;
    let call = must(create_phone_call(&database, &new_call(at(0))).await);
    assert_eq!(call.status(), "starting");
    assert!(call.is_live() && !call.is_final());
    assert_eq!(call.conversation_id(), None);
    assert_eq!(call.number(), "+31612345678");

    must(
        attach_provider_call(
            &database,
            LOCAL_WORKSPACE_ID,
            call.id(),
            "conv_1",
            Some("CA1"),
            at(2),
        )
        .await,
    );
    let attached = must(find_phone_call(&database, LOCAL_WORKSPACE_ID, call.id()).await);
    assert_eq!(attached.status(), "initiated");
    assert_eq!(attached.conversation_id(), Some("conv_1"));
    assert_eq!(attached.provider_call_id(), Some("CA1"));
    assert!(matches!(
        attach_provider_call(
            &database,
            LOCAL_WORKSPACE_ID,
            "no-such-call",
            "c",
            None,
            at(3)
        )
        .await,
        Err(DatabaseError::PhoneCallNotFound)
    ));
}

/// **A finished call stays finished**: a late or repeated look at the provider cannot bring it back, and a stale look cannot move a call
/// backwards.
#[tokio::test]
async fn a_final_call_is_never_changed_and_a_call_never_moves_backwards() {
    let (_directory, database) = database().await;
    let call = must(create_phone_call(&database, &new_call(at(0))).await);
    must(
        attach_provider_call(
            &database,
            LOCAL_WORKSPACE_ID,
            call.id(),
            "conv_1",
            None,
            at(1),
        )
        .await,
    );

    assert!(must(
        update_phone_call(
            &database,
            LOCAL_WORKSPACE_ID,
            call.id(),
            &update("processing"),
            at(5)
        )
        .await
    ));
    assert!(
        !must(
            update_phone_call(
                &database,
                LOCAL_WORKSPACE_ID,
                call.id(),
                &update("in_progress"),
                at(6)
            )
            .await
        ),
        "a stale look cannot move processing back to in progress"
    );
    let done = PhoneCallUpdate {
        status: "done",
        duration_seconds: Some(42),
        ended_because: Some("user hung up"),
        summary: Some("They acknowledged."),
        error: None,
    };
    assert!(must(
        update_phone_call(&database, LOCAL_WORKSPACE_ID, call.id(), &done, at(7)).await
    ));
    let stored = must(find_phone_call(&database, LOCAL_WORKSPACE_ID, call.id()).await);
    assert_eq!(stored.status(), "done");
    assert_eq!(stored.duration_seconds(), Some(42));
    assert_eq!(stored.summary(), Some("They acknowledged."));

    for word in ["in_progress", "failed", "done"] {
        assert!(
            !must(
                update_phone_call(
                    &database,
                    LOCAL_WORKSPACE_ID,
                    call.id(),
                    &update(word),
                    at(9)
                )
                .await
            ),
            "{word} must not change a finished call"
        );
    }
    assert_eq!(
        must(find_phone_call(&database, LOCAL_WORKSPACE_ID, call.id()).await),
        stored
    );
    assert!(matches!(
        update_phone_call(
            &database,
            LOCAL_WORKSPACE_ID,
            call.id(),
            &update("ringing"),
            at(9)
        )
        .await,
        Err(DatabaseError::InvalidPhoneCall { field: "status" })
    ));
}

#[tokio::test]
async fn the_rate_window_counts_every_call_that_was_started() {
    let (_directory, database) = database().await;
    for seconds in [0, 600, 1200] {
        must(create_phone_call(&database, &new_call(at(seconds))).await);
    }
    for (since, expected) in [(0, 3), (601, 1), (1300, 0)] {
        assert_eq!(
            must(count_phone_calls_since(&database, LOCAL_WORKSPACE_ID, at(since)).await),
            expected,
            "since {since}"
        );
    }
}

#[tokio::test]
async fn only_open_calls_are_unsettled_and_the_list_is_newest_first() {
    let (_directory, database) = database().await;
    let first = must(create_phone_call(&database, &new_call(at(0))).await);
    let second = must(create_phone_call(&database, &new_call(at(10))).await);
    must(
        update_phone_call(
            &database,
            LOCAL_WORKSPACE_ID,
            first.id(),
            &update("failed"),
            at(11),
        )
        .await,
    );

    let open = must(unsettled_phone_calls(&database, LOCAL_WORKSPACE_ID).await);
    assert_eq!(
        open.iter().map(StoredPhoneCall::id).collect::<Vec<_>>(),
        [second.id()]
    );
    let all = must(list_phone_calls(&database, LOCAL_WORKSPACE_ID, 10).await);
    assert_eq!(
        all.iter().map(StoredPhoneCall::id).collect::<Vec<_>>(),
        [second.id(), first.id()]
    );
    assert_eq!(
        must(list_phone_calls(&database, LOCAL_WORKSPACE_ID, 1).await).len(),
        1
    );
}

#[tokio::test]
async fn records_can_be_pruned_by_age_and_cleared_and_are_bounded_on_the_way_in() {
    let (_directory, database) = database().await;
    must(create_phone_call(&database, &new_call(at(0))).await);
    must(create_phone_call(&database, &new_call(at(1000))).await);
    assert_eq!(must(prune_phone_calls(&database, at(500)).await), 1);
    assert_eq!(
        must(list_phone_calls(&database, LOCAL_WORKSPACE_ID, 10).await).len(),
        1
    );
    assert_eq!(
        must(delete_phone_calls(&database, LOCAL_WORKSPACE_ID).await),
        1
    );
    assert!(must(list_phone_calls(&database, LOCAL_WORKSPACE_ID, 10).await).is_empty());

    let mut blank = new_call(at(0));
    blank.reason = "   ";
    assert!(matches!(
        create_phone_call(&database, &blank).await,
        Err(DatabaseError::InvalidPhoneCall { field: "reason" })
    ));
    let long = "x".repeat(MAX_PHONE_REASON_CHARS + 1);
    let mut too_long = new_call(at(0));
    too_long.reason = &long;
    assert!(create_phone_call(&database, &too_long).await.is_err());
    let mut short_number = new_call(at(0));
    short_number.number = "+1";
    assert!(matches!(
        create_phone_call(&database, &short_number).await,
        Err(DatabaseError::InvalidPhoneCall { field: "number" })
    ));
}
