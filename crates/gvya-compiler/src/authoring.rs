//! Deterministic authoring-loop state derived from the canonical incremental change gate.
//!
//! This module does not plan conversation semantics or host a model. It translates the existing
//! `ChangeTestPlan` plus canonical build/runtime/test execution results into bounded next actions for
//! an external authoring agent.

use crate::{
    change::{ChangeKind, ChangeMechanic, ChangeTestPlan},
    testing::TestRunReport,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AuthoringLoopState {
    NoChange,
    RepairRequired,
    ReadyToPromote,
}

impl AuthoringLoopState {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::NoChange => "no_change",
            Self::RepairRequired => "repair_required",
            Self::ReadyToPromote => "ready_to_promote",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FailedScenarioStep {
    pub step: usize,
    pub failure_codes: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AuthoringAction {
    ResolveBuildFailure,
    ResolveRuntimeLoadFailure,
    AddDirectMechanicProof {
        mechanic: ChangeMechanic,
        source_kind: Option<ChangeKind>,
        subject: String,
    },
    ResolveRegressionFailure {
        test_id: String,
        failure_codes: Vec<String>,
    },
    ResolveScenarioFailure {
        test_id: String,
        failed_steps: Vec<FailedScenarioStep>,
    },
    ResolveIncompleteTestExecution,
    InspectGateRejection,
    RerunAuthorStep,
    PromoteCandidate,
    KeepBaseline,
}

impl AuthoringAction {
    #[must_use]
    pub const fn kind(&self) -> &'static str {
        match self {
            Self::ResolveBuildFailure => "resolve_build_failure",
            Self::ResolveRuntimeLoadFailure => "resolve_runtime_load_failure",
            Self::AddDirectMechanicProof { .. } => "add_direct_mechanic_proof",
            Self::ResolveRegressionFailure { .. } => "resolve_regression_failure",
            Self::ResolveScenarioFailure { .. } => "resolve_scenario_failure",
            Self::ResolveIncompleteTestExecution => "resolve_incomplete_test_execution",
            Self::InspectGateRejection => "inspect_gate_rejection",
            Self::RerunAuthorStep => "rerun_author_step",
            Self::PromoteCandidate => "promote_candidate",
            Self::KeepBaseline => "keep_baseline",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AuthoringLoopDecision {
    pub state: AuthoringLoopState,
    pub accepted: bool,
    pub promotion_allowed: bool,
    pub actions: Vec<AuthoringAction>,
}

#[must_use]
pub fn plan_authoring_step(
    plan: &ChangeTestPlan,
    build_failed: bool,
    runtime_load_failed: bool,
    tests: Option<&TestRunReport>,
) -> AuthoringLoopDecision {
    let tests_passed = tests.is_none_or(|report| {
        report.regression_executed == report.regression_total
            && report.regression_passed == report.regression_total
            && report.scenario_executed == report.scenario_total
            && report.scenario_passed == report.scenario_total
            && !report.stopped_early
    });
    let accepted =
        !build_failed && !runtime_load_failed && tests_passed && !plan.mechanic_proof_missing;
    let has_changes = !plan.change_set.is_empty();

    if accepted {
        let (state, promotion_allowed, action) = if has_changes {
            (
                AuthoringLoopState::ReadyToPromote,
                true,
                AuthoringAction::PromoteCandidate,
            )
        } else {
            (
                AuthoringLoopState::NoChange,
                false,
                AuthoringAction::KeepBaseline,
            )
        };
        return AuthoringLoopDecision {
            state,
            accepted,
            promotion_allowed,
            actions: vec![action],
        };
    }

    let mut actions = Vec::new();
    if build_failed {
        actions.push(AuthoringAction::ResolveBuildFailure);
    }
    if runtime_load_failed {
        actions.push(AuthoringAction::ResolveRuntimeLoadFailure);
    }
    actions.extend(
        plan.mechanic_requirements
            .iter()
            .filter(|requirement| !requirement.covered())
            .map(|requirement| AuthoringAction::AddDirectMechanicProof {
                mechanic: requirement.mechanic,
                source_kind: requirement.source_kind,
                subject: requirement.subject.clone(),
            }),
    );
    if let Some(report) = tests {
        actions.extend(report.cases.iter().filter(|case| !case.ok).map(|case| {
            AuthoringAction::ResolveRegressionFailure {
                test_id: case.id.as_str().to_owned(),
                failure_codes: case
                    .failures
                    .iter()
                    .map(|failure| failure.code.clone())
                    .collect(),
            }
        }));
        actions.extend(
            report
                .scenarios
                .iter()
                .filter(|scenario| !scenario.ok)
                .map(|scenario| AuthoringAction::ResolveScenarioFailure {
                    test_id: scenario.id.as_str().to_owned(),
                    failed_steps: scenario
                        .steps
                        .iter()
                        .filter(|step| !step.ok)
                        .map(|step| FailedScenarioStep {
                            step: step.step,
                            failure_codes: step
                                .failures
                                .iter()
                                .map(|failure| failure.code.clone())
                                .collect(),
                        })
                        .collect(),
                }),
        );
        if report.stopped_early
            || report.regression_executed != report.regression_total
            || report.scenario_executed != report.scenario_total
        {
            actions.push(AuthoringAction::ResolveIncompleteTestExecution);
        }
    }
    if actions.is_empty() {
        actions.push(AuthoringAction::InspectGateRejection);
    }
    actions.push(AuthoringAction::RerunAuthorStep);

    AuthoringLoopDecision {
        state: AuthoringLoopState::RepairRequired,
        accepted,
        promotion_allowed: false,
        actions,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use gvya_model::{ScenarioId, TestCaseId};

    use super::*;
    use crate::{
        change::{
            ChangeStatus, MechanicProofRequirement, ProjectChange, ProjectChangeSet, SelectedTest,
        },
        testing::{
            CaseRunResult, ExpectationFailure, ScenarioRunResult, StepRunResult, TestRunReport,
        },
    };

    fn plan(
        changes: Vec<ProjectChange>,
        requirements: Vec<MechanicProofRequirement>,
    ) -> ChangeTestPlan {
        ChangeTestPlan {
            change_set: ProjectChangeSet {
                package_order_changed: false,
                semantic_profile_changed: false,
                project_identity_changed: false,
                language_config_changed: false,
                semantic_config_changed: false,
                conversation_config_changed: false,
                debug_map_changed: false,
                changes,
            },
            full_suite_required: false,
            full_suite_reasons: Vec::new(),
            semantic_neighbor_pairs_compared: 0,
            semantic_neighbor_truncated: false,
            changed_test_count: 0,
            proof_test_count: 0,
            mechanic_proof_missing: requirements
                .iter()
                .any(|requirement| !requirement.covered()),
            mechanic_requirements: requirements,
            selected: BTreeMap::new(),
            neighbor_meanings: BTreeMap::new(),
        }
    }

    fn passing_report() -> TestRunReport {
        TestRunReport {
            regression_total: 0,
            regression_executed: 0,
            regression_passed: 0,
            scenario_total: 0,
            scenario_executed: 0,
            scenario_passed: 0,
            stopped_early: false,
            cases: Vec::new(),
            scenarios: Vec::new(),
        }
    }

    #[test]
    fn no_change_keeps_the_accepted_baseline() {
        let decision = plan_authoring_step(&plan(Vec::new(), Vec::new()), false, false, None);
        assert_eq!(decision.state, AuthoringLoopState::NoChange);
        assert!(decision.accepted);
        assert!(!decision.promotion_allowed);
        assert_eq!(decision.actions, vec![AuthoringAction::KeepBaseline]);
    }

    #[test]
    fn accepted_changed_candidate_is_ready_to_promote() {
        let decision = plan_authoring_step(
            &plan(
                vec![ProjectChange {
                    kind: ChangeKind::Behavior,
                    id: "hello".into(),
                    status: ChangeStatus::Modified,
                }],
                vec![MechanicProofRequirement {
                    mechanic: ChangeMechanic::BehaviorResponse,
                    source_kind: Some(ChangeKind::Behavior),
                    subject: "hello".into(),
                    covered_by: vec![SelectedTest::Regression("hello.proof".into())],
                }],
            ),
            false,
            false,
            Some(&passing_report()),
        );
        assert_eq!(decision.state, AuthoringLoopState::ReadyToPromote);
        assert!(decision.accepted);
        assert!(decision.promotion_allowed);
        assert_eq!(decision.actions, vec![AuthoringAction::PromoteCandidate]);
    }

    #[test]
    fn missing_mechanic_proof_becomes_a_targeted_repair_action() {
        let decision = plan_authoring_step(
            &plan(
                vec![ProjectChange {
                    kind: ChangeKind::Meaning,
                    id: "hello".into(),
                    status: ChangeStatus::Modified,
                }],
                vec![MechanicProofRequirement {
                    mechanic: ChangeMechanic::SemanticResolution,
                    source_kind: Some(ChangeKind::Meaning),
                    subject: "hello".into(),
                    covered_by: Vec::new(),
                }],
            ),
            false,
            false,
            Some(&passing_report()),
        );
        assert_eq!(decision.state, AuthoringLoopState::RepairRequired);
        assert!(!decision.accepted);
        assert_eq!(
            decision.actions,
            vec![
                AuthoringAction::AddDirectMechanicProof {
                    mechanic: ChangeMechanic::SemanticResolution,
                    source_kind: Some(ChangeKind::Meaning),
                    subject: "hello".into(),
                },
                AuthoringAction::RerunAuthorStep,
            ]
        );
    }

    #[test]
    fn build_failure_precedes_candidate_rerun() {
        let decision = plan_authoring_step(&plan(Vec::new(), Vec::new()), true, false, None);
        assert_eq!(
            decision.actions,
            vec![
                AuthoringAction::ResolveBuildFailure,
                AuthoringAction::RerunAuthorStep
            ]
        );
    }

    #[test]
    fn failed_selected_tests_are_machine_actionable() {
        let report = TestRunReport {
            regression_total: 1,
            regression_executed: 1,
            regression_passed: 0,
            scenario_total: 1,
            scenario_executed: 1,
            scenario_passed: 0,
            stopped_early: false,
            cases: vec![CaseRunResult {
                id: TestCaseId::new("reg.fail"),
                ok: false,
                failures: vec![ExpectationFailure {
                    code: "meaning_mismatch".into(),
                    detail: "mismatch".into(),
                }],
                observation: None,
            }],
            scenarios: vec![ScenarioRunResult {
                id: ScenarioId::new("scenario.fail"),
                ok: false,
                steps: vec![StepRunResult {
                    step: 2,
                    kind: "turn".into(),
                    input: "hello".into(),
                    ok: false,
                    failures: vec![ExpectationFailure {
                        code: "response_mismatch".into(),
                        detail: "mismatch".into(),
                    }],
                    observation: None,
                }],
            }],
        };
        let decision =
            plan_authoring_step(&plan(Vec::new(), Vec::new()), false, false, Some(&report));
        assert_eq!(decision.state, AuthoringLoopState::RepairRequired);
        assert!(decision.actions.iter().any(|action| matches!(
            action,
            AuthoringAction::ResolveRegressionFailure { test_id, failure_codes }
                if test_id == "reg.fail" && failure_codes == &["meaning_mismatch"]
        )));
        assert!(decision.actions.iter().any(|action| matches!(
            action,
            AuthoringAction::ResolveScenarioFailure { test_id, failed_steps }
                if test_id == "scenario.fail"
                    && failed_steps == &[FailedScenarioStep {
                        step: 2,
                        failure_codes: vec!["response_mismatch".into()],
                    }]
        )));
    }

    #[test]
    fn incomplete_selected_suite_is_never_promotion_ready() {
        let mut report = passing_report();
        report.regression_total = 2;
        report.regression_executed = 1;
        report.regression_passed = 1;
        report.stopped_early = true;
        let decision =
            plan_authoring_step(&plan(Vec::new(), Vec::new()), false, false, Some(&report));
        assert!(!decision.accepted);
        assert!(
            decision
                .actions
                .contains(&AuthoringAction::ResolveIncompleteTestExecution)
        );
    }
}
