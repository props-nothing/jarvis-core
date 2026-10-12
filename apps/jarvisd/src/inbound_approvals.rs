//! Approvals for calls that arrive from another app (`ADR-0180`).
//!
//! A call from an outside agent (Claude Code, Cursor, a phone agent) goes through the same policy as one JARVIS makes itself. When that policy says
//! "ask the owner first", there is no run to park: the call is a live request with a caller waiting on the other end. So the call **waits here**:
//! it is listed with the owner's other approvals, one click, one command or one spoken word answers it, and the caller gets the result or a
//! refusal.
//!
//! **It fails closed.** No answer within [`WAIT`] is a refusal. A full list is a refusal. A denial is a refusal. Nothing runs until the owner
//! says yes, and the yes names this one call: the arguments the owner saw are the arguments that run, because the call holds them.
//!
//! **It is in memory, deliberately.** The caller's request does not survive a daemon restart, so an approval for it that did would be a question
//! nobody is waiting on. A restart simply drops the waiting calls, and the callers see a refusal.

use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use jarvis_core::{CorrelationId, SystemClock, UtcTimestamp};
use jarvis_protocol::PendingApprovalReply;
use serde_json::Value;
use tokio::sync::oneshot;

use crate::phone_service::PhoneApproval;

/// How long a call waits for the owner before it is refused.
pub const WAIT: Duration = Duration::from_secs(120);

/// The most calls that may wait at once. A caller that asks for more is refused rather than flooding the owner with questions.
pub const MAX_WAITING: usize = 8;

/// The most characters of the arguments shown to the owner.
const MAX_SHOWN_ARGUMENT_CHARS: usize = 4_000;

struct Waiting {
    reply: PendingApprovalReply,
    answer: oneshot::Sender<bool>,
}

/// The calls from other apps that are waiting for the owner's answer.
#[derive(Default)]
pub struct InboundApprovals {
    waiting: Mutex<Vec<Waiting>>,
    /// A second way for the owner to say yes: on a phone call JARVIS placed (`ADR-0200`). Set once at startup, and absent in a profile
    /// without phone calls.
    phone: OnceLock<Arc<dyn PhoneApproval>>,
}

/// Why a call could not be put in front of the owner.
#[derive(Debug, Eq, PartialEq)]
pub enum AskError {
    /// Too many calls are already waiting.
    Full,
}

impl InboundApprovals {
    /// Lets a yes spoken on a live phone call stand for the owner's click. Set once; a second call is ignored.
    pub fn set_phone_approval(&self, phone: Arc<dyn PhoneApproval>) {
        let _ = self.phone.set(phone);
    }

    /// The phone's say, when this profile has one.
    #[must_use]
    pub fn phone_approval(&self) -> Option<&Arc<dyn PhoneApproval>> {
        self.phone.get()
    }
    /// Puts a call in front of the owner. Returns its identifier and the receiver its answer arrives on.
    ///
    /// # Errors
    ///
    /// [`AskError::Full`] when [`MAX_WAITING`] calls are already waiting.
    pub fn ask(
        &self,
        tool: &str,
        tool_version: &str,
        risk_level: u8,
        preview: &str,
        arguments: &Value,
        wait: Duration,
    ) -> Result<(String, oneshot::Receiver<bool>), AskError> {
        let mut waiting = self
            .waiting
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if waiting.len() >= MAX_WAITING {
            return Err(AskError::Full);
        }
        let id = CorrelationId::new().to_string();
        let now = UtcTimestamp::now(&SystemClock);
        let expires = UtcTimestamp::from_unix_nanos(
            now.unix_nanos() + i128::try_from(wait.as_nanos()).unwrap_or(i128::MAX / 4),
        )
        .unwrap_or(now);
        // What the owner reads is bounded; if the arguments do not fit, none are shown and the console refuses to offer an approve
        // button for something it cannot show (the fail-closed direction `PendingApprovalReply::arguments` documents).
        let text = arguments.to_string();
        let shown = (text.chars().count() <= MAX_SHOWN_ARGUMENT_CHARS).then(|| arguments.clone());
        let (answer, receiver) = oneshot::channel();
        waiting.push(Waiting {
            reply: PendingApprovalReply {
                approval_id: id.clone(),
                state: "pending".to_owned(),
                run_id: String::new(),
                call_id: id.clone(),
                tool: tool.to_owned(),
                tool_version: tool_version.to_owned(),
                risk_level,
                preview: preview.to_owned(),
                arguments: shown,
                created_at: now,
                expires_at: expires,
            },
            answer,
        });
        Ok((id, receiver))
    }

    /// The waiting calls, newest first, in the shape the approvals list uses.
    #[must_use]
    pub fn list(&self) -> Vec<PendingApprovalReply> {
        let mut replies: Vec<PendingApprovalReply> = self
            .waiting
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .iter()
            .map(|waiting| waiting.reply.clone())
            .collect();
        replies.reverse();
        replies
    }

    /// Answers a waiting call. Returns what was answered, or `None` when no call is waiting under that identifier (it was already
    /// answered, timed out, or belongs to a run).
    pub fn decide(&self, id: &str, approve: bool) -> Option<PendingApprovalReply> {
        let mut waiting = self
            .waiting
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let at = waiting
            .iter()
            .position(|entry| entry.reply.approval_id == id)?;
        let entry = waiting.remove(at);
        // The caller may have gone away; the answer then has nowhere to land, which is fine.
        let _ = entry.answer.send(approve);
        Some(entry.reply)
    }

    /// Withdraws a call that is no longer waiting (its time ran out). Idempotent.
    pub fn forget(&self, id: &str) {
        self.waiting
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .retain(|entry| entry.reply.approval_id != id);
    }
}

/// Withdraws a waiting call when the task holding it is dropped, so a cancelled call never leaves a question nobody is waiting on.
pub struct Withdraw<'a> {
    approvals: &'a InboundApprovals,
    id: String,
}

impl<'a> Withdraw<'a> {
    /// Guards `id` until this value is dropped.
    #[must_use]
    pub fn new(approvals: &'a InboundApprovals, id: &str) -> Self {
        Self {
            approvals,
            id: id.to_owned(),
        }
    }
}

impl Drop for Withdraw<'_> {
    fn drop(&mut self) {
        self.approvals.forget(&self.id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn ask(approvals: &InboundApprovals) -> Result<(String, oneshot::Receiver<bool>), AskError> {
        approvals.ask(
            "jarvis.files.write",
            "1.0.0",
            2,
            "write a file",
            &json!({"path":"a"}),
            WAIT,
        )
    }

    #[tokio::test]
    async fn a_waiting_call_is_listed_and_the_owners_answer_reaches_it_once() {
        let approvals = InboundApprovals::default();
        let (id, receiver) = ask(&approvals).unwrap_or_else(|e| panic!("{e:?}"));
        let listed = approvals.list();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].approval_id, id);
        assert_eq!(listed[0].arguments, Some(json!({"path":"a"})));

        assert!(approvals.decide(&id, true).is_some());
        assert_eq!(receiver.await, Ok(true));
        assert!(
            approvals.list().is_empty(),
            "answered, so no longer waiting"
        );
        assert!(
            approvals.decide(&id, true).is_none(),
            "a second answer finds nothing"
        );
    }

    /// Falsified by removing the guard: a dropped call would leave its question listed until the time limit.
    #[test]
    fn a_dropped_call_withdraws_its_question() {
        let approvals = InboundApprovals::default();
        let (id, _receiver) = ask(&approvals).unwrap_or_else(|e| panic!("{e:?}"));
        {
            let _guard = Withdraw::new(&approvals, &id);
            assert_eq!(approvals.list().len(), 1);
        }
        assert!(approvals.list().is_empty());
    }

    #[tokio::test]
    async fn a_denial_reaches_the_call_as_false() {
        let approvals = InboundApprovals::default();
        let (id, receiver) = ask(&approvals).unwrap_or_else(|e| panic!("{e:?}"));
        approvals.decide(&id, false);
        assert_eq!(receiver.await, Ok(false));
    }

    /// Falsified by removing the cap: a caller could fill the owner's screen with questions.
    #[test]
    fn too_many_waiting_calls_are_refused() {
        let approvals = InboundApprovals::default();
        let mut keep = Vec::new();
        for _ in 0..MAX_WAITING {
            keep.push(ask(&approvals).unwrap_or_else(|e| panic!("{e:?}")));
        }
        assert_eq!(ask(&approvals).err(), Some(AskError::Full));
        approvals.forget(&keep[0].0);
        assert!(ask(&approvals).is_ok(), "room again once one is withdrawn");
    }

    #[test]
    fn an_identifier_that_is_not_waiting_is_not_answerable() {
        let approvals = InboundApprovals::default();
        assert!(
            approvals
                .decide("0198f000-0000-7000-8000-000000000001", true)
                .is_none()
        );
    }

    #[test]
    fn arguments_too_large_to_show_are_not_shown() {
        let approvals = InboundApprovals::default();
        let big = json!({ "text": "x".repeat(MAX_SHOWN_ARGUMENT_CHARS + 1) });
        approvals
            .ask("jarvis.files.write", "1.0.0", 2, "write", &big, WAIT)
            .unwrap_or_else(|e| panic!("{e:?}"));
        assert_eq!(approvals.list()[0].arguments, None);
    }
}
