//! Tests for `jarvis.skill.propose` and the owner's approve and dismiss, against a real database.

use super::*;
use jarvis_core::{SkillQuery, select_skills};

struct Scratch(std::path::PathBuf);

impl Drop for Scratch {
    fn drop(&mut self) {
        jarvis_core::remove_scratch_dir(&self.0);
    }
}

async fn tool() -> (Scratch, Arc<SqliteDatabase>, SkillProposeTool) {
    let tag = jarvis_core::scratch_tag();
    let path = std::env::temp_dir().join(format!("jskill-{}", &tag[tag.len() - 12..]));
    std::fs::create_dir_all(&path).unwrap_or_else(|error| panic!("create scratch: {error}"));
    let database = Arc::new(
        SqliteDatabase::test_fixture(&path.join("jarvis.sqlite3"))
            .await
            .unwrap_or_else(|error| panic!("open the database: {error}")),
    );
    let tool = SkillProposeTool::new(Arc::clone(&database));
    (Scratch(path), database, tool)
}

fn proposal(description: &str) -> Value {
    json!({
        "description": description,
        "steps": [
            { "tool": "jarvis.web.search", "instruction": "Search for the company's website." },
            { "tool": "jarvis.files.write", "instruction": "Append a row to the prospects sheet." }
        ]
    })
}

const TRIGGER: &str =
    "Research a Dutch company and add it to the prospects sheet.\nCheck the website before adding.";

async fn propose(tool: &SkillProposeTool, arguments: &Value) -> Result<Value, AdapterError> {
    tool.propose(arguments, CorrelationId::new()).await
}

async fn usable_for(database: &Arc<SqliteDatabase>, request: &str) -> usize {
    let identity = jarvis_storage::load_local_identity(database)
        .await
        .unwrap_or_else(|error| panic!("identity: {error}"));
    let workspace = identity
        .workspace_id()
        .parse()
        .unwrap_or_else(|error| panic!("workspace: {error:?}"));
    let stored = jarvis_storage::read_usable_skill_revisions(
        database,
        identity.workspace_id(),
        50,
        &|_: &str| true,
    )
    .await
    .unwrap_or_else(|error| panic!("read: {error}"));
    select_skills(
        &stored,
        &SkillQuery::new(workspace, Sensitivity::Internal).with_text(request),
    )
    .offered
    .len()
}

#[test]
fn the_proposal_is_a_cheap_write_and_the_approval_is_the_owners() {
    let definition = SkillProposeTool::definition().unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(definition.approval(), ApprovalPolicy::Auto);
    assert!(definition.effects().contains(ToolEffect::Write));
    assert!(definition.description().contains("only a proposal"));
}

/// **A proposal waits: it is listed as proposed, authored by the model, and offered to no run until the owner approves it.**
#[tokio::test]
async fn a_proposal_is_not_used_until_the_owner_approves_it() {
    let (_scratch, database, tool) = tool().await;
    let reply = propose(&tool, &proposal(TRIGGER))
        .await
        .unwrap_or_else(|error| panic!("propose: {error}"));
    assert_eq!(reply["outcome"], "proposed");
    let id = reply["revision_id"].as_str().unwrap_or_default().to_owned();

    let service = SkillService::new(Arc::clone(&database), None);
    let listed = service
        .list(50)
        .await
        .unwrap_or_else(|error| panic!("{error}"));
    let skill = &listed.skills[0];
    assert_eq!(skill.state, "proposed");
    assert_eq!(skill.source_kind, "model_inference");
    assert_eq!(skill.tool_ids, ["jarvis.web.search", "jarvis.files.write"]);

    let request = "Please research the Dutch company Acme and add it to the prospects sheet";
    assert_eq!(
        usable_for(&database, request).await,
        0,
        "a proposal must change nothing"
    );

    let approved = service
        .approve(&id)
        .await
        .unwrap_or_else(|error| panic!("approve: {error}"));
    assert_eq!(approved.reference.state, "active");
    assert_ne!(
        approved.reference.promoted_by_actor_id.as_deref(),
        Some("agent:model"),
        "the approver is the owner, never the author"
    );
    assert_eq!(usable_for(&database, request).await, 1);
    assert_eq!(
        usable_for(&database, "send a birthday card to my aunt").await,
        0
    );
}

/// **A dismissed proposal is kept for audit, never offered, and cannot be approved afterwards.**
#[tokio::test]
async fn a_dismissed_proposal_is_never_offered_and_cannot_be_approved() {
    let (_scratch, database, tool) = tool().await;
    let reply = propose(&tool, &proposal(TRIGGER))
        .await
        .unwrap_or_else(|error| panic!("propose: {error}"));
    let id = reply["revision_id"].as_str().unwrap_or_default().to_owned();
    let service = SkillService::new(Arc::clone(&database), None);
    let dismissed = service
        .dismiss(&id)
        .await
        .unwrap_or_else(|error| panic!("dismiss: {error}"));
    assert_eq!(dismissed.reference.state, "archived");
    assert_eq!(
        usable_for(
            &database,
            "research a Dutch company and add it to the prospects sheet"
        )
        .await,
        0
    );
    assert!(
        service.approve(&id).await.is_err(),
        "an archived proposal must not become active through approve"
    );
}

/// **The model cannot fill the review list: the same procedure is answered with the waiting one, and ten is the limit.**
#[tokio::test]
async fn proposals_are_deduplicated_and_capped() {
    let (_scratch, database, tool) = tool().await;
    let first = propose(&tool, &proposal(TRIGGER))
        .await
        .unwrap_or_else(|error| panic!("{error}"));
    let again = propose(&tool, &proposal(TRIGGER))
        .await
        .unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(again["outcome"], "already_proposed");
    assert_eq!(again["revision_id"], first["revision_id"]);

    for index in 1..crate::skill_service::MAX_PENDING_SKILL_PROPOSALS {
        propose(
            &tool,
            &proposal(&format!("Do thing number {index} for the owner.")),
        )
        .await
        .unwrap_or_else(|error| panic!("{index}: {error}"));
    }
    let refused = propose(&tool, &proposal("One more procedure than the list holds.")).await;
    assert!(
        matches!(&refused, Err(AdapterError::RefusedBeforeReaching { reason }) if reason.contains("waiting")),
        "{refused:?}"
    );
    let listed = SkillService::new(database, None)
        .list(50)
        .await
        .unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(
        listed.skills.len(),
        crate::skill_service::MAX_PENDING_SKILL_PROPOSALS
    );
}

/// **Bad input is refused and stores nothing.**
#[tokio::test]
async fn bad_proposals_store_nothing() {
    let (_scratch, database, tool) = tool().await;
    let many: Vec<Value> = (0..13)
        .map(|_| json!({ "tool": "jarvis.web.search", "instruction": "x" }))
        .collect();
    for arguments in [
        json!({ "steps": [] }),
        json!({ "description": "no steps here at all", "steps": [] }),
        json!({ "description": "too many steps for one procedure", "steps": many }),
        json!({ "description": "a step without a tool name", "steps": [{ "instruction": "x" }] }),
        json!({ "description": "a tool that is not an identifier", "steps": [{ "tool": "search", "instruction": "x" }] }),
        json!({ "description": "a blank instruction goes nowhere", "steps": [{ "tool": "jarvis.web.search", "instruction": "  " }] }),
        json!({ "description": "   ", "steps": [{ "tool": "jarvis.web.search", "instruction": "x" }] }),
    ] {
        let result = propose(&tool, &arguments).await;
        assert!(
            matches!(result, Err(AdapterError::RefusedBeforeReaching { .. })),
            "{arguments}: {result:?}"
        );
    }
    let listed = SkillService::new(database, None)
        .list(50)
        .await
        .unwrap_or_else(|error| panic!("{error}"));
    assert!(listed.skills.is_empty());
}
