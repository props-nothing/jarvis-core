//! Tests for the schedule tools, against a real database.

use super::*;

/// A scratch directory holding the fixture database, removed when the test ends.
struct Scratch(std::path::PathBuf);

impl Drop for Scratch {
    fn drop(&mut self) {
        jarvis_core::remove_scratch_dir(&self.0);
    }
}

async fn tool() -> (Scratch, ScheduleTool) {
    let tag = jarvis_core::scratch_tag();
    let path = std::env::temp_dir().join(format!("jst-{}", &tag[tag.len() - 12..]));
    std::fs::create_dir_all(&path).unwrap_or_else(|error| panic!("create scratch: {error}"));
    let database = SqliteDatabase::open(&path.join("jarvis.sqlite3"))
        .await
        .unwrap_or_else(|error| panic!("open the database: {error}"));
    (Scratch(path), ScheduleTool::new(Arc::new(database)))
}

fn in_one_hour(now: UtcTimestamp) -> String {
    UtcTimestamp::from_unix_nanos(now.unix_nanos() + 3_600_000_000_000)
        .unwrap_or_else(|error| panic!("{error}"))
        .to_string()
}

#[test]
fn the_contract_asks_before_it_persists_and_lists_freely() {
    let definitions = ScheduleTool::definitions().unwrap_or_else(|error| panic!("{error}"));
    let by_id = |id: &str| {
        definitions
            .iter()
            .find(|definition| definition.id().to_string() == id)
            .unwrap_or_else(|| panic!("{id} must be defined"))
    };
    // A schedule outlives the conversation, so a page a model read could leave a standing instruction: asking is the guard.
    assert_eq!(by_id(ADD_TOOL).approval(), ApprovalPolicy::Ask);
    assert_eq!(by_id(REMOVE_TOOL).approval(), ApprovalPolicy::Ask);
    assert_eq!(by_id(LIST_TOOL).approval(), ApprovalPolicy::Auto);
    assert!(by_id(LIST_TOOL).effects().contains(ToolEffect::ReadOnly));
    assert!(
        by_id(ADD_TOOL)
            .description()
            .contains("user is asked first")
    );
}

/// **A schedule made in conversation is listed, fenced as data, and can be removed again.**
#[tokio::test]
async fn a_task_can_be_scheduled_listed_and_removed() {
    let (_scratch, tool) = tool().await;
    let now = UtcTimestamp::now(&SystemClock);

    let added = tool
        .add(
            "no-call",
            &json!({ "objective": "Tell me to stretch", "at": in_one_hour(now) }),
            now,
        )
        .await
        .unwrap_or_else(|error| panic!("add: {error}"));
    let id = added["scheduled"]["schedule_id"]
        .as_str()
        .unwrap_or_else(|| panic!("an id is returned: {added}"))
        .to_owned();
    assert_eq!(added["scheduled"]["cadence"], "once");

    let repeated = tool
        .add(
            "no-call",
            &json!({ "objective": "Check the build", "every": "6h" }),
            now,
        )
        .await
        .unwrap_or_else(|error| panic!("add every: {error}"));
    assert_eq!(repeated["scheduled"]["interval_seconds"], 21_600);

    let listed = tool
        .list()
        .await
        .unwrap_or_else(|error| panic!("list: {error}"));
    let schedules = listed["schedules"]
        .as_array()
        .unwrap_or_else(|| panic!("{listed}"));
    assert_eq!(schedules.len(), 2);
    let objective = schedules[0]["objective"].as_str().unwrap_or_default();
    assert!(
        objective.contains(jarvis_core::FENCE_OPEN) && objective.contains("Tell me to stretch"),
        "a stored objective is data to the model that reads it back: {objective}"
    );

    tool.remove(&json!({ "schedule_id": id }))
        .await
        .unwrap_or_else(|error| panic!("remove: {error}"));
    let after = tool
        .list()
        .await
        .unwrap_or_else(|error| panic!("list: {error}"));
    assert_eq!(after["schedules"].as_array().map(Vec::len), Some(1));
}

/// **A wall-clock task is created from a time, days and a zone, and listed with all three.**
#[tokio::test]
async fn a_wall_clock_task_is_scheduled_in_its_zone() {
    let (_scratch, tool) = tool().await;
    let now = UtcTimestamp::now(&SystemClock);
    let added = tool
        .add(
            "no-call",
            &json!({
                "objective": "Brief me",
                "at_time": "08:00",
                "days": "mon-fri",
                "time_zone": "Europe/Amsterdam",
            }),
            now,
        )
        .await
        .unwrap_or_else(|error| panic!("add daily: {error}"));
    let scheduled = &added["scheduled"];
    assert_eq!(scheduled["cadence"], "daily");
    assert_eq!(scheduled["at_time"], "08:00");
    assert_eq!(scheduled["days"], "weekdays");
    assert_eq!(scheduled["time_zone"], "Europe/Amsterdam");
    assert!(scheduled["next_run_at"].is_string(), "{scheduled}");

    // The same time with no zone takes this machine's, or is refused when the machine does not say: never a guess.
    let without_zone = tool
        .add(
            "no-call",
            &json!({ "objective": "x", "at_time": "09:30" }),
            now,
        )
        .await;
    match jarvis_core::system_zone() {
        Some(zone) => {
            let added = without_zone.unwrap_or_else(|error| panic!("{error}"));
            assert_eq!(added["scheduled"]["time_zone"], zone);
        }
        None => assert!(matches!(
            without_zone,
            Err(AdapterError::RefusedBeforeReaching { .. })
        )),
    }
}

/// **A wall-clock time that is not a time, a zone that is not a place, and days without a time are each refused.**
#[tokio::test]
async fn a_wall_clock_task_with_bad_parts_creates_nothing() {
    let (_scratch, tool) = tool().await;
    let now = UtcTimestamp::now(&SystemClock);
    for arguments in [
        json!({ "objective": "x", "at_time": "25:00", "time_zone": "UTC" }),
        json!({ "objective": "x", "at_time": "08:00", "time_zone": "Mars/Olympus" }),
        json!({ "objective": "x", "at_time": "08:00", "time_zone": "+01:00" }),
        json!({ "objective": "x", "at_time": "08:00", "days": "funday", "time_zone": "UTC" }),
        json!({ "objective": "x", "days": "mon-fri" }),
        json!({ "objective": "x", "every": "1h", "time_zone": "UTC" }),
        json!({ "objective": "x", "every": "1h", "at_time": "08:00", "time_zone": "UTC" }),
    ] {
        let result = tool.add("no-call", &arguments, now).await;
        assert!(
            matches!(result, Err(AdapterError::RefusedBeforeReaching { .. })),
            "{arguments}: {result:?}"
        );
    }
    let listed = tool
        .list()
        .await
        .unwrap_or_else(|error| panic!("list: {error}"));
    assert_eq!(listed["schedules"].as_array().map(Vec::len), Some(0));
}

/// **Bad input is refused in the scheduler's own words and creates nothing.**
///
/// Both cadences, neither, a past time, an unparseable time, a too-short interval, a blank objective and an unknown id: each must
/// leave the table as it was, because a half-made schedule would fire for real later.
#[tokio::test]
async fn bad_input_is_refused_and_creates_nothing() {
    let (_scratch, tool) = tool().await;
    let now = UtcTimestamp::now(&SystemClock);
    let past = UtcTimestamp::from_unix_nanos(now.unix_nanos() - 60_000_000_000)
        .unwrap_or_else(|error| panic!("{error}"))
        .to_string();
    let future = in_one_hour(now);
    for arguments in [
        json!({ "objective": "x", "every": "1h", "at": future }),
        json!({ "objective": "x" }),
        json!({ "objective": "x", "at": past }),
        json!({ "objective": "x", "at": "tomorrow at nine" }),
        json!({ "objective": "x", "every": "10s" }),
        json!({ "objective": "   ", "every": "1h" }),
    ] {
        let result = tool.add("no-call", &arguments, now).await;
        assert!(
            matches!(result, Err(AdapterError::RefusedBeforeReaching { .. })),
            "{arguments}: {result:?}"
        );
    }
    let listed = tool
        .list()
        .await
        .unwrap_or_else(|error| panic!("list: {error}"));
    assert_eq!(listed["schedules"].as_array().map(Vec::len), Some(0));
    assert!(matches!(
        tool.remove(&json!({ "schedule_id": "no-such-id" })).await,
        Err(AdapterError::RefusedBeforeReaching { .. })
    ));
}

/// **A model can pause a task without asking and resume it only with the owner's yes; a finished one-off cannot be resumed.**
#[tokio::test]
async fn a_task_can_be_paused_and_resumed_and_resuming_asks() {
    let definitions = ScheduleTool::definitions().unwrap_or_else(|error| panic!("{error}"));
    let approval = |id: &str| {
        definitions
            .iter()
            .find(|definition| definition.id().to_string() == id)
            .map(jarvis_tools::ToolDefinition::approval)
    };
    assert_eq!(approval(PAUSE_TOOL), Some(ApprovalPolicy::Auto));
    assert_eq!(approval(RESUME_TOOL), Some(ApprovalPolicy::Ask));

    let (_scratch, tool) = tool().await;
    let now = UtcTimestamp::now(&SystemClock);
    let added = tool
        .add(
            "no-call",
            &json!({ "objective": "Check the build", "every": "6h" }),
            now,
        )
        .await
        .unwrap_or_else(|error| panic!("{error:?}"));
    let id = added["scheduled"]["schedule_id"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    let paused = tool
        .set_enabled(&json!({ "schedule_id": id }), false, now)
        .await
        .unwrap_or_else(|error| panic!("{error:?}"));
    assert_eq!(paused["schedule"]["enabled"], false);
    let resumed = tool
        .set_enabled(&json!({ "schedule_id": id }), true, now)
        .await
        .unwrap_or_else(|error| panic!("{error:?}"));
    assert_eq!(resumed["schedule"]["enabled"], true);
    assert!(
        tool.set_enabled(&json!({ "schedule_id": "nope" }), false, now)
            .await
            .is_err()
    );
}
