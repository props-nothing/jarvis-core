//! Request and reply bodies for triggers: an event that starts an ordinary run.
//!
//! # A trigger asks for a run, it does not carry authority
//!
//! Like a schedule, a trigger names **what to wait for** and **what to do**, and nothing else: no tool, no scope, no approval. The run it
//! starts is an ordinary run, so everything it needs permission for is still asked of a person. `deny_unknown_fields` turns an attempt to
//! send such a field into a `422` rather than a silently ignored value.

use jarvis_core::UtcTimestamp;
use serde::{Deserialize, Serialize};

/// Request body for `POST /api/v1/triggers`.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CreateTriggerRequest {
    /// What to wait for: `mail` (the default), `file` or `webhook`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    /// What to watch. For `mail`, the Gmail search as typed in Gmail's search box, such as `from:acme.example is:unread`. For `file`, a
    /// folder inside a folder you granted, such as `scans/new`. Not used by `webhook`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub query: Option<String>,
    /// What each run is asked to do about one event.
    pub objective: String,
    /// How often to look, such as `5m` or `1h`. Between one minute and one day; the default is five minutes. Not used by `webhook`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub every: Option<String>,
    /// The project every run belongs to, by identifier or name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_id: Option<String>,
}

/// One trigger.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct TriggerReply {
    /// The trigger's identifier.
    pub trigger_id: String,
    /// What it waits for: `mail`.
    pub kind: String,
    /// The Gmail search it watches.
    pub query: String,
    /// What each run is asked to do.
    pub objective: String,
    /// Seconds between looks.
    pub interval_seconds: u64,
    /// Whether it is looking. A paused trigger is not.
    pub enabled: bool,
    /// Whether the messages that already matched were recorded, so only later ones start runs.
    pub baselined: bool,
    /// The name of the project it belongs to, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,
    /// When it looks next, present only while it is active.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_poll_at: Option<UtcTimestamp>,
    /// When it last looked.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_polled_at: Option<UtcTimestamp>,
    /// Why the last look failed, in a fixed sentence; absent after one that worked.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_error: Option<String>,
    /// The most recent run it started.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_run_id: Option<String>,
    /// How many runs it has started.
    pub fire_count: u32,
    /// When it was created.
    pub created_at: UtcTimestamp,
    /// For a webhook, its address (`/hooks/<secret>`), **shown only in the answer that creates it**: only a hash of the secret is kept,
    /// so it cannot be shown again.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub webhook_path: Option<String>,
}

/// Response body for `GET /api/v1/triggers`.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct TriggerListReply {
    /// How many triggers there are.
    pub total: usize,
    /// The triggers, oldest first.
    pub triggers: Vec<TriggerReply>,
}
