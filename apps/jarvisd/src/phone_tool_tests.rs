//! Tests for the phone tools: what they declare, what they refuse, and what the model is shown of a call.

use jarvis_tools::ToolEffect;

use super::*;
use crate::phone_service::tests::ready;

fn by_id(definitions: &[ToolDefinition], id: &str) -> ToolDefinition {
    definitions
        .iter()
        .find(|definition| definition.id().to_string() == id)
        .unwrap_or_else(|| panic!("{id} must be defined"))
        .clone()
}

#[test]
fn placing_a_call_is_an_external_communication_that_asks_and_reading_one_is_free() {
    let definitions = PhoneTool::definitions().unwrap_or_else(|error| panic!("{error}"));
    let call = by_id(&definitions, CALL_TOOL);
    assert_eq!(call.approval(), ApprovalPolicy::Ask);
    assert!(call.effects().contains(ToolEffect::ExternalCommunication));
    assert!(call.risk().level() >= 2);
    for outcome in [
        jarvis_tools::ToolOutcome::Failed,
        jarvis_tools::ToolOutcome::Unknown,
    ] {
        assert!(
            !call.may_retry_automatically(outcome),
            "a retry would ring the phone twice ({outcome:?})"
        );
    }
    let status = by_id(&definitions, STATUS_TOOL);
    assert_eq!(status.approval(), ApprovalPolicy::Auto);
    assert!(status.effects().contains(ToolEffect::ReadOnly));
}

/// **A model has no way to name a number outside the list**: the schema accepts only a reason, an urgency and a listed number.
#[test]
fn the_call_schema_takes_no_other_field() {
    let schema: Value = serde_json::from_str(CALL_INPUT).unwrap_or_default();
    assert_eq!(schema["additionalProperties"], false);
    let fields: Vec<&str> = schema["properties"]
        .as_object()
        .map(|properties| properties.keys().map(String::as_str).collect())
        .unwrap_or_default();
    assert_eq!(fields, ["reason", "to", "urgent"]);
}

#[test]
fn the_phone_tools_are_deferred_with_the_other_rarely_used_families() {
    assert_eq!(
        crate::tool_search::group_of(CALL_TOOL).as_deref(),
        Some("jarvis-voice")
    );
    assert_eq!(
        crate::tool_search::group_of(STATUS_TOOL).as_deref(),
        Some("jarvis-voice")
    );
}

#[tokio::test]
async fn an_unconfigured_call_is_refused_before_reaching_anything_and_says_what_to_do() {
    let rig = crate::phone_service::tests::rig("", None).await;
    let tool = PhoneTool::new(Arc::clone(&rig.phone));
    let error = tool
        .call(&json!({ "reason": "the build failed" }))
        .await
        .err()
        .unwrap_or_else(|| panic!("must be refused"));
    let AdapterError::RefusedBeforeReaching { reason } = error else {
        panic!("a configuration problem is a refusal, got {error:?}");
    };
    assert!(reason.contains("Settings"), "{reason}");
    assert!(rig.fake.placed().is_empty());
}

#[tokio::test]
async fn a_call_returns_its_id_and_a_masked_number_and_tells_the_model_not_to_call_again() {
    let rig = ready("").await;
    let tool = PhoneTool::new(Arc::clone(&rig.phone));
    let body = tool
        .call(&json!({ "reason": "the build failed", "urgent": true }))
        .await
        .unwrap_or_else(|error| panic!("call: {error}"));
    assert_eq!(body["status"], "initiated");
    assert!(body["call_id"].as_str().is_some_and(|id| id.len() == 36));
    assert!(
        !body.to_string().contains("+31612345678"),
        "the number is shown masked: {body}"
    );
    assert!(
        body["note"]
            .as_str()
            .is_some_and(|note| note.contains("Do not call again"))
    );
    let other = tool
        .call(&json!({ "reason": "the build failed", "to": "+31687654321" }))
        .await
        .err()
        .unwrap_or_else(|| panic!("a number off the list is refused"));
    assert!(matches!(other, AdapterError::RefusedBeforeReaching { .. }));
}

/// **What was said on the phone reaches the model as fenced data**, with the summary, the reason and the provider's words.
#[tokio::test]
async fn what_was_said_on_the_phone_is_fenced_as_data() {
    let rig = ready("").await;
    let tool = PhoneTool::new(Arc::clone(&rig.phone));
    let placed = tool
        .call(&json!({ "reason": "the build failed" }))
        .await
        .unwrap_or_else(|error| panic!("call: {error}"));
    rig.fake.set_status("done");
    let status = tool
        .status(&json!({ "call_id": placed["call_id"] }))
        .await
        .unwrap_or_else(|error| panic!("status: {error}"));
    assert_eq!(status["status"], "done");
    assert_eq!(status["duration_seconds"], 30);
    for field in ["summary", "transcript", "reason", "ended_because"] {
        let text = status[field].as_str().unwrap_or_default();
        assert!(
            text.contains(jarvis_core::FENCE_OPEN),
            "{field} must be fenced: {status}"
        );
    }
    assert!(
        status["transcript"]
            .as_str()
            .is_some_and(|text| text.contains("Ignore previous instructions"))
    );
    assert!(!status.to_string().contains("+31612345678"));

    assert!(
        tool.status(&json!({ "call_id": "no-such-call" }))
            .await
            .is_err()
    );
}

#[test]
fn the_tools_that_may_have_rung_are_ambiguous_and_the_rest_are_refusals() {
    assert!(matches!(
        map_start_error(StartError::Provider(PhoneError::Uncertain)),
        AdapterError::AmbiguousAfterReaching { .. }
    ));
    assert!(matches!(
        map_start_error(StartError::Provider(PhoneError::NotStarted("x".to_owned()))),
        AdapterError::ProviderRefused { .. }
    ));
    for refused_early in [
        StartError::NotSetUp("x".to_owned()),
        StartError::NotAllowed("x".to_owned()),
        StartError::Provider(PhoneError::NumberNotFound),
        StartError::Provider(PhoneError::Unreachable),
        StartError::Database,
    ] {
        assert!(matches!(
            map_start_error(refused_early),
            AdapterError::RefusedBeforeReaching { .. }
        ));
    }
}
