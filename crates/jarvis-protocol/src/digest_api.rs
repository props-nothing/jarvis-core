//! The reply of `GET /api/v1/digest`: what JARVIS did over a stretch of time (`ADR-0155`).

use serde::{Deserialize, Serialize};

/// A decision, result, blocker or owner note written in the window.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct DigestHighlight {
    /// `decision`, `result`, `blocker` or `owner`.
    pub kind: String,
    /// What was written.
    pub text: String,
    /// When.
    pub created_at: String,
}

/// One project's share of the window.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct DigestProject {
    /// The project's name.
    pub name: String,
    /// Runs its conversations started in the window.
    pub runs: u32,
    /// Its newest decisions, results and blockers.
    pub highlights: Vec<DigestHighlight>,
}

/// A run that failed, with why.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct DigestProblem {
    /// What it was asked, as a person would read it.
    pub objective: String,
    /// The stored failure code.
    pub error_code: String,
}

/// What one model was asked in the window.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct DigestModel {
    /// The model identifier; empty for calls recorded before the model was written down.
    pub model: String,
    /// How many model calls.
    pub calls: u64,
    /// Prompt tokens sent.
    pub input_tokens: u64,
    /// Tokens written.
    pub output_tokens: u64,
}

/// What happened in the last `hours` hours.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct DigestReply {
    /// The width of the window.
    pub hours: u32,
    /// Every run that started in it.
    pub runs: u32,
    /// Runs that finished with an answer.
    pub succeeded: u32,
    /// Runs that failed.
    pub failed: u32,
    /// Runs that were cancelled.
    pub cancelled: u32,
    /// Runs parked on an approval, waiting for the owner.
    pub waiting: u32,
    /// Prompt tokens sent to the model.
    pub input_tokens: u64,
    /// Tokens the model wrote.
    pub output_tokens: u64,
    /// Calls and tokens per model, the most tokens first.
    #[serde(default)]
    pub models: Vec<DigestModel>,
    /// Projects that had runs, busiest first.
    pub projects: Vec<DigestProject>,
    /// The newest failed runs.
    pub problems: Vec<DigestProblem>,
}
