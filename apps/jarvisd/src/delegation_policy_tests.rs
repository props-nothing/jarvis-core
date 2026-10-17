//! The real native/coding catalog must agree on authority, not just on labels.

use jarvis_tools::{
    ActorAuthority, ApprovalPolicy, Decision, DelegationBoundary, DenyReason, EffectSet,
    Idempotency, PolicyDecision, PolicyRequest, RetryPolicy, Risk, ScopeSet, TargetAssessment,
    ToolDefinition, ToolEffect, WorkspacePolicy, declared_for_effects, evaluate,
};

fn launches() -> Vec<ToolDefinition> {
    crate::delegate::AgentTool::definitions()
        .unwrap_or_else(|e| panic!("native catalog: {e}"))
        .into_iter()
        .chain(
            crate::code_delegate_async::AsyncCodeDelegateTool::definitions()
                .unwrap_or_else(|e| panic!("coding catalog: {e}")),
        )
        .filter(|tool| {
            matches!(
                tool.id().to_string().as_str(),
                crate::delegate::DELEGATE_TOOL
                    | crate::code_delegate::DELEGATE_TOOL
                    | crate::code_delegate_async::START
                    | crate::code_delegate_async::RESUME
            )
        })
        .collect()
}

fn decision(
    tool: &ToolDefinition,
    workspace: &WorkspacePolicy,
    scopes: ScopeSet,
) -> PolicyDecision {
    evaluate(&PolicyRequest {
        definition: tool,
        actor: ActorAuthority::active(scopes),
        workspace,
        available: true,
        target: TargetAssessment::default(),
    })
}

#[test]
fn delegation_catalog_preserves_boundaries_and_disallows_blind_retries() {
    let tools = launches();
    assert_eq!(tools.len(), 4);
    for tool in tools {
        let native = tool.id().to_string().as_str() == crate::delegate::DELEGATE_TOOL;
        let expected = if native {
            vec![ToolEffect::Write, ToolEffect::Delegation]
        } else {
            vec![
                ToolEffect::Write,
                ToolEffect::Delegation,
                ToolEffect::CodeExecution,
            ]
        };
        assert_eq!(tool.effects().to_vec(), expected, "{}", tool.id());
        assert_eq!(tool.risk().level(), if native { 1 } else { 3 });
        assert_eq!(
            tool.approval(),
            if native {
                ApprovalPolicy::Auto
            } else {
                ApprovalPolicy::Ask
            }
        );
        assert_eq!(tool.version(), "1.1.0");
        assert_eq!(tool.idempotency(), Idempotency::Unsupported);
        assert!(!tool.retry().allows_retry());
        assert!(RetryPolicy::blind(1, 1, tool.effects(), tool.idempotency()).is_err());
        for risk in 0..tool.risk().level() {
            assert!(
                declared_for_effects(risk, tool.effects()).is_err(),
                "{} risk {risk}",
                tool.id()
            );
        }
        let encoded = serde_json::to_string(tool.effects()).unwrap_or_else(|e| panic!("{e}"));
        assert!(encoded.contains("delegation"));
        assert_eq!(
            serde_json::from_str::<EffectSet>(&encoded).unwrap_or_else(|e| panic!("{e}")),
            *tool.effects()
        );
        assert_eq!(
            decision(
                &tool,
                &WorkspacePolicy::default(),
                tool.required_scopes().clone()
            )
            .decision(),
            if native {
                Decision::Allow
            } else {
                Decision::RequireApproval
            }
        );
    }
}

#[test]
fn delegation_trust_never_bypasses_grants_denial_or_risk_ceiling() {
    for tool in launches() {
        let trusted = WorkspacePolicy::default().trusting(tool.id().clone());
        assert!(decision(&tool, &trusted, tool.required_scopes().clone()).is_allowed());
        assert_eq!(
            decision(&tool, &trusted, ScopeSet::none()).reason(),
            Some(DenyReason::MissingScope)
        );
        let denied = trusted.denying(tool.id().clone());
        assert_eq!(
            decision(&tool, &denied, tool.required_scopes().clone()).reason(),
            Some(DenyReason::ToolDeniedByWorkspace)
        );
        let maximum = if tool.id().to_string() == crate::delegate::DELEGATE_TOOL {
            Risk::Minimal
        } else {
            Risk::Moderate
        };
        let ceiling = WorkspacePolicy::new(maximum, maximum, true)
            .unwrap_or_else(|e| panic!("{e}"))
            .trusting(tool.id().clone());
        assert_eq!(
            decision(&tool, &ceiling, tool.required_scopes().clone()).reason(),
            Some(DenyReason::AboveWorkspaceCeiling)
        );
    }
}

#[test]
fn native_delegation_can_be_held_and_its_trust_cannot_release_a_coding_job() {
    let tools = launches();
    let native = &tools[0];
    assert_eq!(
        native.id().to_string().as_str(),
        crate::delegate::DELEGATE_TOOL
    );
    let cautious =
        WorkspacePolicy::new(Risk::High, Risk::Low, true).unwrap_or_else(|e| panic!("{e}"));
    assert!(decision(native, &cautious, native.required_scopes().clone()).is_held());
    let trusted_native = WorkspacePolicy::default().trusting(native.id().clone());
    for coding in &tools[1..] {
        assert!(decision(coding, &trusted_native, coding.required_scopes().clone()).is_held());
    }
    for trusted_launch in &tools[1..] {
        let trusted = WorkspacePolicy::default().trusting(trusted_launch.id().clone());
        for other in &tools[1..] {
            if trusted_launch.id() != other.id() {
                assert!(decision(other, &trusted, other.required_scopes().clone()).is_held());
            }
        }
    }
    // Choosing a worktree or disabling cooperative commands cannot change the declared
    // boundary: policy is independent of model-controlled task arguments.
    assert_eq!(DelegationBoundary::HostProcess.effects().risk_floor(), 3);
}

#[test]
fn inspection_and_stop_do_not_launch_delegated_work() {
    for tool in crate::code_delegate_async::AsyncCodeDelegateTool::definitions()
        .unwrap_or_else(|e| panic!("{e}"))
    {
        if matches!(
            tool.id().to_string().as_str(),
            crate::code_delegate_async::STATUS
                | crate::code_delegate_async::WAIT
                | crate::code_delegate_async::LIST
                | crate::code_delegate_async::STOP
        ) {
            assert!(!tool.effects().contains(ToolEffect::Delegation));
            assert!(!tool.effects().contains(ToolEffect::CodeExecution));
            assert!(
                decision(
                    &tool,
                    &WorkspacePolicy::default(),
                    tool.required_scopes().clone()
                )
                .is_allowed()
            );
        }
    }
}
