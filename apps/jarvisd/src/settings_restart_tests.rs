#![allow(clippy::unwrap_used)]

use std::sync::Arc;

use super::*;

struct Scratch(PathBuf);
impl Drop for Scratch {
    fn drop(&mut self) {
        jarvis_core::remove_scratch_dir(&self.0);
    }
}

async fn fixture() -> (Scratch, GatewayState) {
    let root = std::env::temp_dir().join(format!("jrestart-{}", jarvis_core::scratch_tag()));
    std::fs::create_dir_all(&root).unwrap();
    let root = std::fs::canonicalize(root).unwrap();
    let paths = jarvis_storage::portable_layout(&root).unwrap();
    std::fs::create_dir_all(paths.config()).unwrap();
    let database = Arc::new(
        jarvis_storage::SqliteDatabase::test_fixture(&root.join("jarvis.sqlite3"))
            .await
            .unwrap(),
    );
    let state = GatewayState::new(database, jarvis_core::ClientCredential::generate().unwrap())
        .with_settings(SettingsContext::new(paths, Some(root.clone())));
    (Scratch(root), state)
}

#[tokio::test]
async fn queued_coding_prevents_restart_launch_and_force_preserves_root_and_flag() {
    let (scratch, state) = fixture().await;
    jarvis_storage::create_delegated_task(
        state.database(),
        &jarvis_storage::NewDelegatedTask {
            workspace_id: jarvis_storage::LOCAL_WORKSPACE_ID,
            repository: "fixture",
            task: "queued coding",
            allow_commands: false,
            minutes: 0,
            now: jarvis_core::UtcTimestamp::now(&jarvis_core::SystemClock),
        },
    )
    .await
    .unwrap();
    let refused = restart_if_ready(&state, false, |_, _| {
        panic!("a busy restart must not launch")
    })
    .await;
    assert_eq!(refused.status(), StatusCode::CONFLICT);
    let calls = std::cell::Cell::new(0);
    let forced = restart_if_ready(&state, true, |root, force| {
        calls.set(calls.get() + 1);
        assert_eq!(root, Some(scratch.0.as_path()));
        assert!(
            force,
            "the detached CLI must receive the explicit force flag too"
        );
        StatusCode::ACCEPTED.into_response()
    })
    .await;
    assert_eq!(forced.status(), StatusCode::ACCEPTED);
    assert_eq!(calls.get(), 1);
    state.database().close().await;
}

#[tokio::test]
async fn unreadable_state_does_not_launch_restart_and_only_explicit_force_bypasses_it() {
    let (_scratch, state) = fixture().await;
    state.database().close().await;
    let refused = restart_if_ready(&state, false, |_, _| {
        panic!("unreadable work must not launch restart")
    })
    .await;
    assert_eq!(refused.status(), StatusCode::SERVICE_UNAVAILABLE);
    let calls = std::cell::Cell::new(0);
    let forced = restart_if_ready(&state, true, |_, force| {
        calls.set(calls.get() + 1);
        assert!(force);
        StatusCode::ACCEPTED.into_response()
    })
    .await;
    assert_eq!(forced.status(), StatusCode::ACCEPTED);
    assert_eq!(calls.get(), 1);
}
