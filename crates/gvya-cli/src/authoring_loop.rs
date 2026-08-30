//! Machine serialization for compiler-owned authoring-loop decisions.

use super::*;

pub(super) fn author_step_preflight_report_json(
    side: &str,
    stage: &str,
    base_fingerprint: Option<&str>,
    candidate_fingerprint: Option<&str>,
    diagnostics: &[AuthoringDiagnostic],
) -> serde_json::Value {
    let candidate_failure = side == "candidate";
    let state = if candidate_failure {
        "repair_required"
    } else {
        "blocked"
    };
    let primary_action = if candidate_failure {
        "resolve_candidate_source_failure"
    } else {
        "restore_valid_accepted_baseline"
    };
    let mut actions = vec![serde_json::json!({
        "kind": primary_action,
        "side": side,
        "stage": stage,
        "diagnostic_codes": diagnostics.iter().map(|diagnostic| diagnostic.code.as_str()).collect::<Vec<_>>()
    })];
    if candidate_failure {
        actions.push(serde_json::json!({
            "kind": "rerun_author_step",
            "after": "candidate_only_repairs"
        }));
    }
    serde_json::json!({
        "format": "gvya.cli.author-step",
        "version": 1,
        "state": state,
        "accepted": false,
        "promotion_allowed": false,
        "base_policy": if candidate_failure { "immutable_until_candidate_promotion" } else { "valid_accepted_baseline_required" },
        "candidate_policy": if candidate_failure { "repair_candidate_only" } else { "do_not_mutate_candidate" },
        "primary_action": primary_action,
        "source_identity": {
            "algorithm": "sha256",
            "contract": "gvya.source-tree/1",
            "base": base_fingerprint,
            "candidate": candidate_fingerprint
        },
        "promotion": { "contract": "gvya.promotion/1", "allowed": false, "identity": null },
        "diagnostics": diagnostics.iter().map(AuthoringDiagnostic::json).collect::<Vec<_>>(),
        "next_actions": actions,
        "gate": serde_json::Value::Null
    })
}

pub(super) fn author_step_report_json(
    decision: &AuthoringLoopDecision,
    base_fingerprint: &str,
    candidate_fingerprint: &str,
    gate_report: serde_json::Value,
) -> serde_json::Value {
    let actions = decision.actions.iter().map(action_json).collect::<Vec<_>>();
    let primary_action = decision.actions.first().map(AuthoringAction::kind);
    let promotion_identity = decision
        .promotion_allowed
        .then(|| promotion_identity(base_fingerprint, candidate_fingerprint));

    serde_json::json!({
        "format": "gvya.cli.author-step",
        "version": 1,
        "state": decision.state.label(),
        "accepted": decision.accepted,
        "promotion_allowed": decision.promotion_allowed,
        "base_policy": "immutable_until_candidate_promotion",
        "candidate_policy": if decision.accepted { "do_not_repair" } else { "repair_candidate_only" },
        "primary_action": primary_action,
        "source_identity": {
            "algorithm": "sha256",
            "contract": "gvya.source-tree/1",
            "base": base_fingerprint,
            "candidate": candidate_fingerprint
        },
        "promotion": {
            "contract": "gvya.promotion/1",
            "allowed": decision.promotion_allowed,
            "identity": promotion_identity
        },
        "diagnostics": [],
        "next_actions": actions,
        "gate": gate_report
    })
}

pub(super) fn promotion_identity(base_fingerprint: &str, candidate_fingerprint: &str) -> String {
    let mut framed = Vec::with_capacity(base_fingerprint.len() + candidate_fingerprint.len() + 64);
    framed.extend_from_slice(b"gvya.promotion/1\0");
    framed.extend_from_slice(&(base_fingerprint.len() as u64).to_be_bytes());
    framed.extend_from_slice(base_fingerprint.as_bytes());
    framed.extend_from_slice(&(candidate_fingerprint.len() as u64).to_be_bytes());
    framed.extend_from_slice(candidate_fingerprint.as_bytes());
    gvya_compiler::canonical::sha256_hex(&framed)
}

fn action_json(action: &AuthoringAction) -> serde_json::Value {
    match action {
        AuthoringAction::ResolveBuildFailure => serde_json::json!({
            "kind": action.kind(), "stage": "build"
        }),
        AuthoringAction::ResolveRuntimeLoadFailure => serde_json::json!({
            "kind": action.kind(), "stage": "runtime_load"
        }),
        AuthoringAction::AddDirectMechanicProof {
            mechanic,
            source_kind,
            subject,
        } => serde_json::json!({
            "kind": action.kind(),
            "mechanic": mechanic.label(),
            "source_kind": source_kind.map(|kind| kind.label()),
            "subject": subject,
            "constraint": "manual_changed_non_generated_test"
        }),
        AuthoringAction::ResolveRegressionFailure {
            test_id,
            failure_codes,
        } => serde_json::json!({
            "kind": action.kind(),
            "test_kind": "regression",
            "test_id": test_id,
            "failure_codes": failure_codes
        }),
        AuthoringAction::ResolveScenarioFailure {
            test_id,
            failed_steps,
        } => serde_json::json!({
            "kind": action.kind(),
            "test_kind": "scenario",
            "test_id": test_id,
            "failed_steps": failed_steps.iter().map(|step| serde_json::json!({
                "step": step.step,
                "failure_codes": step.failure_codes
            })).collect::<Vec<_>>()
        }),
        AuthoringAction::ResolveIncompleteTestExecution => serde_json::json!({
            "kind": action.kind(),
            "reason": "selected test execution stopped before the selected suite completed"
        }),
        AuthoringAction::InspectGateRejection => serde_json::json!({
            "kind": action.kind(),
            "reason": "canonical gate rejected the candidate without a more specific author-step action"
        }),
        AuthoringAction::RerunAuthorStep => serde_json::json!({
            "kind": action.kind(), "after": "candidate_only_repairs"
        }),
        AuthoringAction::PromoteCandidate => serde_json::json!({
            "kind": action.kind(),
            "reason": "candidate passed the canonical incremental change gate"
        }),
        AuthoringAction::KeepBaseline => serde_json::json!({
            "kind": action.kind(),
            "reason": "candidate has no semantic source changes"
        }),
    }
}
