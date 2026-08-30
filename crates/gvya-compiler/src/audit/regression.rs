//! Regression/scenario audit rules.

use super::*;

pub(super) fn audit_tests(
    project: &ComposedProject,
    issues: &mut Vec<AuditIssue>,
    limits: AuditorLimits,
) {
    let meaning_ids: BTreeSet<_> = project
        .semantic_catalog
        .patterns()
        .iter()
        .map(|pattern| pattern.id.clone())
        .collect();
    let capability_ids: BTreeSet<_> = project
        .capability_catalog
        .capability_ids()
        .cloned()
        .collect();
    for case in &project.tests.regression_cases {
        if case.input.trim().is_empty() {
            let mut row = issue(
                "test.input_empty",
                AuditSeverity::Error,
                "test",
                "Regression case input is empty",
                AuditLocation::project(),
            );
            row.related.push(related(
                "regression",
                ContributionKind::RegressionCase,
                case.id.as_str(),
            ));
            push(issues, limits, row);
        }
        audit_expectation(
            &case.expectation,
            &meaning_ids,
            &capability_ids,
            issues,
            limits,
            ContributionKind::RegressionCase,
            case.id.as_str(),
        );
    }
    for scenario in &project.tests.scenarios {
        if scenario.steps.is_empty() {
            let mut row = issue(
                "test.scenario_empty",
                AuditSeverity::Error,
                "test",
                "Conversation scenario has no interaction steps",
                AuditLocation::project(),
            );
            row.related.push(related(
                "scenario",
                ContributionKind::Scenario,
                scenario.id.as_str(),
            ));
            push(issues, limits, row);
        }
        if scenario.steps.len() > 256 {
            let mut row = issue(
                "test.scenario_large",
                AuditSeverity::Warning,
                "test",
                "Scenario exceeds the default 256-step execution bound",
                AuditLocation::project(),
            );
            row.related.push(related(
                "scenario",
                ContributionKind::Scenario,
                scenario.id.as_str(),
            ));
            push(issues, limits, row);
        }
        for (index, step) in scenario.steps.iter().enumerate() {
            if let crate::testing::ScenarioStep::Turn(turn) = step {
                if turn.say.trim().is_empty() {
                    let mut row = issue(
                        "test.scenario_turn_empty",
                        AuditSeverity::Error,
                        "test",
                        "Scenario contains an empty user turn",
                        AuditLocation::project(),
                    );
                    row.related.push(related(
                        "scenario",
                        ContributionKind::Scenario,
                        scenario.id.as_str(),
                    ));
                    push(issues, limits, row);
                }
            }
            let (proposal_from_step, proposal_capability) = match step {
                crate::testing::ScenarioStep::CapabilityResult(step) => (
                    Some(step.proposal_from_step),
                    step.proposal_capability.as_ref(),
                ),
                crate::testing::ScenarioStep::Confirm(step) => (
                    Some(step.proposal_from_step),
                    step.proposal_capability.as_ref(),
                ),
                _ => (None, None),
            };
            if proposal_from_step.is_some_and(|from| from == 0 || from > index) {
                let mut row = issue(
                    "test.scenario_step_reference_invalid",
                    AuditSeverity::Error,
                    "test",
                    "Scenario step must reference an earlier one-based proposal step",
                    AuditLocation::project(),
                );
                row.related.push(related(
                    "scenario",
                    ContributionKind::Scenario,
                    scenario.id.as_str(),
                ));
                push(issues, limits, row);
            }
            if let Some(capability) = proposal_capability {
                if !capability_ids.contains(capability) {
                    let mut row = issue(
                        "test.scenario_proposal_capability_missing",
                        AuditSeverity::Error,
                        "test",
                        "Scenario proposal selector references a capability absent from the composed catalog",
                        AuditLocation::project(),
                    );
                    row.related.push(related(
                        "scenario",
                        ContributionKind::Scenario,
                        scenario.id.as_str(),
                    ));
                    push(issues, limits, row);
                }
            }
            audit_expectation(
                step.expectation(),
                &meaning_ids,
                &capability_ids,
                issues,
                limits,
                ContributionKind::Scenario,
                scenario.id.as_str(),
            );
        }
    }
}

pub(super) fn audit_expectation(
    expectation: &TurnExpectation,
    meanings: &BTreeSet<gvya_model::MeaningId>,
    capabilities: &BTreeSet<gvya_model::CapabilityId>,
    issues: &mut Vec<AuditIssue>,
    limits: AuditorLimits,
    kind: ContributionKind,
    id: &str,
) {
    if let Some(meaning) = &expectation.meaning {
        if !meanings.contains(meaning) {
            let mut row = issue(
                "test.expected_meaning_missing",
                AuditSeverity::Error,
                "test",
                "Test expects a meaning absent from the composed catalog",
                AuditLocation::project(),
            );
            row.related.push(related("test", kind, id));
            row.related.push(related(
                "meaning",
                ContributionKind::Meaning,
                meaning.as_str(),
            ));
            push(issues, limits, row);
        }
    }
    for expected in &expectation.capabilities {
        if !capabilities.contains(&expected.id) {
            let mut row = issue(
                "test.expected_capability_missing",
                AuditSeverity::Error,
                "test",
                "Test expects a capability absent from the composed catalog",
                AuditLocation::project(),
            );
            row.related.push(related("test", kind, id));
            row.related.push(related(
                "capability",
                ContributionKind::Capability,
                expected.id.as_str(),
            ));
            push(issues, limits, row);
        }
    }
    if expectation
        .min_semantic_score
        .is_some_and(|score| !score.is_finite() || !(0.0..=1.0).contains(&score))
    {
        let mut row = issue(
            "test.semantic_score_invalid",
            AuditSeverity::Error,
            "test",
            "Minimum semantic score must be finite and within 0..=1",
            AuditLocation::project(),
        );
        row.related.push(related("test", kind, id));
        push(issues, limits, row);
    }
}
