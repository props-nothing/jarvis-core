//! Keeps a long run's working context inside a bound (`ADR-0194`).
//!
//! # The problem
//!
//! A run is a loop: the model asks for a tool, the result is appended to the transcript, the whole transcript is sent again. Each result may
//! be up to 12,000 characters and **every one stays**, so a task of thirty or forty reads (the kind an autonomous run is for) outgrows even
//! a large context window and the provider answers `model_context_overflow`. The run then fails with the work half done. Only a *parked*
//! transcript was ever bounded (`fit_transcript`); a live one was not.
//!
//! # What this does
//!
//! Before each model call, if the transcript is over a soft budget, the **oldest large tool results** are replaced by one line that says
//! they were left out and how big they were, until the transcript is down to a low-water mark. It is deterministic and costs no model call.
//! This is *observation masking*: the model keeps its own reasoning and every call it made, loses only the bulk of old observations, and can
//! run the tool again if it still needs one.
//!
//! # What it never touches
//!
//! * the **six most recent** tool results, which the model is working from;
//! * **every result the model has not yet answered**: those after its newest turn, however many it asked for at once. It has never read
//!   them, and leaving one out would hand it a placeholder where it asked for data (found live, see below);
//! * the assistant turns and their tool calls, so every result still answers a call that is still there (a provider rejects it otherwise);
//! * the messages that were assembled before the run's own turns (brief, journal, memory, history);
//! * a **discovery result** (`jarvis.tools.search`), because the offered tools are re-derived from it on every call;
//! * small results, which a placeholder would not shorten.
//!
//! The elision is not a loss of record: the full result is in the run's stored events and the call's audit entry, as before.
//!
//! # Why down to a low-water mark
//!
//! A provider caches the front of a prompt. Changing an early message breaks that cache for everything after it, so masking one result per
//! call would break it on every call. Masking in a batch, down to three quarters of the budget, breaks it once and then leaves room for
//! many calls.

use jarvis_models::{ChatMessage, Role};

/// The size of a transcript, in bytes, above which old results are left out.
///
/// Bytes over-estimate tokens for English text (a token is about four bytes), the same direction the context assembler errs
/// (`estimate_tokens`). 200 KB is about 50,000 tokens: inside a 64,000-token window with the assembled context, and well inside the 128,000
/// that most hosted models offer.
pub(crate) const SOFT_BUDGET_BYTES: usize = 200_000;

/// What the transcript is brought down to once it is over the budget.
const LOW_WATER_BYTES: usize = SOFT_BUDGET_BYTES / 4 * 3;

/// How many of the newest tool results are always kept whole.
pub(crate) const KEEP_RECENT_RESULTS: usize = 6;

/// A result shorter than this is kept: the placeholder would not make it meaningfully smaller.
const MIN_LEFT_OUT_BYTES: usize = 600;

/// What every left-out result starts with, so a second pass does not mask it again.
const LEFT_OUT: &str = "[earlier tool result left out to keep the working context small";

/// What a trim did.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Trimmed {
    /// How many results were left out.
    pub results: usize,
    /// How many bytes that saved.
    pub bytes: usize,
}

/// The bytes a message adds to a request: its text, and the name and arguments of every call it carries.
fn size(message: &ChatMessage) -> usize {
    message.text().len()
        + message
            .tool_calls()
            .iter()
            .map(|call| call.name().len() + call.arguments().len())
            .sum::<usize>()
}

/// Leaves out old large tool results when the transcript is over the soft budget.
///
/// `own_turns_from` is where the run's own turns begin (everything before it was assembled for the run and is never touched). Returns
/// what was done, or `None` when the transcript was within the budget or nothing could be shortened.
pub(crate) fn trim_old_results(
    messages: &mut [ChatMessage],
    own_turns_from: usize,
) -> Option<Trimmed> {
    trim_to(messages, own_turns_from, SOFT_BUDGET_BYTES, LOW_WATER_BYTES)
}

fn trim_to(
    messages: &mut [ChatMessage],
    own_turns_from: usize,
    budget: usize,
    target: usize,
) -> Option<Trimmed> {
    let mut total: usize = messages.iter().map(size).sum();
    if total <= budget {
        return None;
    }
    let results: Vec<usize> = messages
        .iter()
        .enumerate()
        .skip(own_turns_from)
        .filter(|(_, message)| message.role() == Role::Tool)
        .map(|(index, _)| index)
        .collect();
    // The model has not answered the results that follow its newest turn, so it has never seen them: they are never left out, however
    // many it asked for at once. Only what it has already read (and moved on from) can go.
    let unseen_from = messages
        .iter()
        .rposition(|message| message.role() == Role::Assistant)
        .map_or(0, |index| index + 1);
    let older = results.len().saturating_sub(KEEP_RECENT_RESULTS);
    let mut done = Trimmed {
        results: 0,
        bytes: 0,
    };
    for &index in &results[..older] {
        if total <= target {
            break;
        }
        if index >= unseen_from {
            break;
        }
        let text = messages[index].text();
        if text.len() < MIN_LEFT_OUT_BYTES
            || text.starts_with(LEFT_OUT)
            || crate::tool_search::is_discovery_result(&text)
        {
            continue;
        }
        let Some(call_id) = messages[index].tool_call_id().map(str::to_owned) else {
            continue;
        };
        let placeholder = format!(
            "{LEFT_OUT}: {} characters. Run the tool again if it is still needed.]",
            text.chars().count()
        );
        let saved = text.len().saturating_sub(placeholder.len());
        messages[index] = ChatMessage::tool(call_id, placeholder);
        total = total.saturating_sub(saved);
        done.results += 1;
        done.bytes += saved;
    }
    (done.results > 0).then_some(done)
}

#[cfg(test)]
mod tests {
    use jarvis_models::ToolCall;

    use super::*;

    /// An assistant call and its result, `chars` long.
    fn exchange(messages: &mut Vec<ChatMessage>, step: usize, tool: &str, chars: usize) {
        let id = format!("call_{step}");
        messages.push(ChatMessage::assistant_tool_calls(
            String::new(),
            vec![ToolCall::new(&id, tool, r#"{"path":"a.txt"}"#)],
        ));
        messages.push(ChatMessage::tool(&id, "x".repeat(chars)));
    }

    fn transcript(steps: usize, chars: usize) -> Vec<ChatMessage> {
        let mut messages = vec![
            ChatMessage::system("policy"),
            ChatMessage::user("the objective"),
        ];
        for step in 0..steps {
            exchange(&mut messages, step, "jarvis.files.read", chars);
        }
        messages
    }

    fn total(messages: &[ChatMessage]) -> usize {
        messages.iter().map(size).sum()
    }

    fn results(messages: &[ChatMessage]) -> Vec<&ChatMessage> {
        messages
            .iter()
            .filter(|message| message.role() == Role::Tool)
            .collect()
    }

    /// **A transcript within the budget is left exactly as it was.**
    #[test]
    fn a_transcript_within_the_budget_is_untouched() {
        let mut messages = transcript(10, 1000);
        let before = messages.clone();
        assert_eq!(trim_old_results(&mut messages, 2), None);
        assert_eq!(messages, before);
    }

    /// **Over the budget, the oldest large results go first, down to the low-water mark, and the newest are kept whole.**
    #[test]
    fn the_oldest_large_results_are_left_out_and_the_newest_kept() {
        let mut messages = transcript(30, 11_000);
        assert!(total(&messages) > SOFT_BUDGET_BYTES);
        let done = trim_old_results(&mut messages, 2)
            .unwrap_or_else(|| panic!("an over-budget transcript is trimmed"));
        assert!(done.results > 0 && done.bytes > 0);
        assert!(
            total(&messages) <= LOW_WATER_BYTES,
            "{} bytes",
            total(&messages)
        );

        let kept = results(&messages);
        let (old, recent) = kept.split_at(kept.len() - KEEP_RECENT_RESULTS);
        assert!(
            recent.iter().all(|message| message.text().len() == 11_000),
            "the newest results are whole"
        );
        assert!(
            old.first()
                .is_some_and(|message| message.text().starts_with(LEFT_OUT)),
            "the oldest is left out"
        );
        assert!(
            old.first()
                .is_some_and(|message| message.text().contains("11000 characters")),
            "and says how big it was"
        );
    }

    /// **Every result still answers a call that is still there**, which a provider requires.
    #[test]
    fn calls_and_their_results_keep_their_pairing() {
        let mut messages = transcript(30, 11_000);
        let calls_before: Vec<String> = messages
            .iter()
            .flat_map(|message| message.tool_calls().iter().map(|call| call.id().to_owned()))
            .collect();
        trim_old_results(&mut messages, 2);
        let calls_after: Vec<String> = messages
            .iter()
            .flat_map(|message| message.tool_calls().iter().map(|call| call.id().to_owned()))
            .collect();
        assert_eq!(calls_before, calls_after, "no call is touched");
        for pair in messages[2..].chunks(2) {
            assert_eq!(pair[0].role(), Role::Assistant);
            assert_eq!(pair[1].role(), Role::Tool);
            assert_eq!(
                pair[1].tool_call_id(),
                pair[0].tool_calls().first().map(ToolCall::id)
            );
            assert!(pair.iter().all(ChatMessage::is_consistent));
        }
    }

    /// **A second look at an already trimmed transcript does nothing**, so the prompt cache is broken once and not on every call.
    #[test]
    fn trimming_twice_does_nothing_the_second_time() {
        let mut messages = transcript(30, 11_000);
        assert!(trim_old_results(&mut messages, 2).is_some());
        let after_first = messages.clone();
        assert_eq!(trim_old_results(&mut messages, 2), None);
        assert_eq!(messages, after_first);
    }

    /// **What was assembled before the run's own turns is never touched.**
    #[test]
    fn the_assembled_context_before_the_runs_own_turns_is_protected() {
        let mut messages = transcript(30, 11_000);
        // Pretend the first twelve messages were assembled for the run (a replayed earlier turn, say).
        let guarded: Vec<ChatMessage> = messages[..12].to_vec();
        trim_old_results(&mut messages, 12);
        assert_eq!(&messages[..12], guarded.as_slice());
    }

    /// **A discovery result is never shortened**, because the tools offered are re-derived from it on every call.
    #[test]
    fn a_tool_discovery_result_is_never_left_out() {
        let found = format!(
            r#"{{"kind":"tool_discovery","found_groups":["browser"],"note":"{}"}}"#,
            "n".repeat(11_000)
        );
        let mut messages = vec![ChatMessage::user("go")];
        messages.push(ChatMessage::assistant_tool_calls(
            String::new(),
            vec![ToolCall::new("call_s", "jarvis.tools.search", "{}")],
        ));
        messages.push(ChatMessage::tool("call_s", found.clone()));
        for step in 0..30 {
            exchange(&mut messages, step, "jarvis.files.read", 11_000);
        }
        let groups_before = crate::tool_search::found_groups(&messages);
        assert!(groups_before.contains("browser"));
        trim_old_results(&mut messages, 1);
        assert_eq!(messages[2].text(), found, "the discovery result is whole");
        assert_eq!(
            crate::tool_search::found_groups(&messages),
            groups_before,
            "the offered tools do not change"
        );
    }

    /// Small results are kept: a placeholder would not shorten them.
    #[test]
    fn small_results_are_kept() {
        let mut messages = transcript(2000, 300);
        assert!(total(&messages) > SOFT_BUDGET_BYTES);
        assert_eq!(trim_old_results(&mut messages, 2), None);
    }

    /// One turn's worth of calls, `count` of them at once, answered by `count` results of `chars` each.
    fn parallel_turn(messages: &mut Vec<ChatMessage>, count: usize, chars: usize) {
        let calls: Vec<ToolCall> = (0..count)
            .map(|index| ToolCall::new(format!("p_{index}"), "jarvis.files.read", "{}"))
            .collect();
        messages.push(ChatMessage::assistant_tool_calls(String::new(), calls));
        for index in 0..count {
            messages.push(ChatMessage::tool(format!("p_{index}"), "y".repeat(chars)));
        }
    }

    /// **Results the model has not yet answered are never left out, however many it asked for at once.**
    ///
    /// Found live: a model asked for thirty file reads in one turn, and the trim, running before its next call, left out the oldest
    /// twenty-four of them before it had ever read them. It said so, re-read in batches, and then listed values it had not seen.
    #[test]
    fn results_the_model_has_not_yet_seen_are_never_left_out() {
        let mut messages = vec![ChatMessage::user("go")];
        parallel_turn(&mut messages, 30, 11_000);
        assert!(total(&messages) > SOFT_BUDGET_BYTES);
        let before = messages.clone();
        assert_eq!(trim_old_results(&mut messages, 1), None);
        assert_eq!(messages, before, "the model sees all thirty");
    }

    /// Once the model has moved on, an earlier turn's results can go, and the newest turn's stay whole.
    #[test]
    fn an_earlier_turn_can_be_left_out_once_the_model_has_moved_on() {
        let mut messages = vec![ChatMessage::user("go")];
        parallel_turn(&mut messages, 30, 11_000);
        // The model answered those thirty by asking for one more read.
        exchange(&mut messages, 99, "jarvis.files.read", 11_000);
        let done = trim_old_results(&mut messages, 1)
            .unwrap_or_else(|| panic!("what the model has read can be left out"));
        assert!(done.results > 0);
        let kept = results(&messages);
        assert!(
            kept.last()
                .is_some_and(|message| message.text().len() == 11_000),
            "the newest result is whole"
        );
        assert!(
            kept[0].text().starts_with(LEFT_OUT),
            "an old one is left out"
        );
    }
}
