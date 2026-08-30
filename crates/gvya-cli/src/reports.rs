//! Machine-readable CLI report serialization.

use super::*;

pub(super) fn audit_report_json(report: &AuditReport) -> serde_json::Value {
    serde_json::json!({
        "format": "gvya.cli.audit", "version": 1,
        "summary": {"errors": report.summary.errors, "warnings": report.summary.warnings, "info": report.summary.info},
        "truncated": report.truncated,
        "issues": report.issues.iter().map(|issue| serde_json::json!({
            "severity": issue.severity.label(), "code": issue.code.as_str(), "category": issue.category, "summary": issue.summary,
            "location": {
                "package": issue.location.package.as_ref().map(|id| id.as_str()),
                "kind": issue.location.kind.map(|kind| kind.label()),
                "item_id": issue.location.item_id,
                "sub_id": issue.location.sub_id,
                "path": issue.location.path,
            },
            "related": issue.related.iter().map(|row| serde_json::json!({
                "label": row.label, "package": row.package.as_ref().map(|id| id.as_str()), "kind": row.kind.map(|kind| kind.label()), "item_id": row.item_id,
            })).collect::<Vec<_>>(),
            "remediation": issue.remediation, "details": issue.details,
        })).collect::<Vec<_>>(),
    })
}

pub(super) fn analysis_json(analysis: &ProjectAnalysis) -> serde_json::Value {
    serde_json::json!({
        "format": "gvya.cli.analysis", "version": 1,
        "expectation_coverage": {
            "meanings": {"total": analysis.coverage.meanings.total, "covered": analysis.coverage.meanings.covered, "ratio": analysis.coverage.meanings.ratio(), "uncovered": analysis.coverage.meanings.uncovered.iter().map(|id| id.as_str()).collect::<Vec<_>>()},
            "behaviors": {"total": analysis.coverage.behaviors.total, "covered": analysis.coverage.behaviors.covered, "ratio": analysis.coverage.behaviors.ratio(), "uncovered": analysis.coverage.behaviors.uncovered.iter().map(|id| id.as_str()).collect::<Vec<_>>()},
            "capabilities": {"total": analysis.coverage.capabilities.total, "covered": analysis.coverage.capabilities.covered, "ratio": analysis.coverage.capabilities.ratio(), "uncovered": analysis.coverage.capabilities.uncovered.iter().map(|id| id.as_str()).collect::<Vec<_>>()},
            "test_origins": {
                "manual_regression": analysis.coverage.test_origins.manual_regression, "generated_regression": analysis.coverage.test_origins.generated_regression,
                "manual_scenarios": analysis.coverage.test_origins.manual_scenarios, "generated_scenarios": analysis.coverage.test_origins.generated_scenarios,
            },
        },
        "discoverability": {
            "identity_terms": analysis.discoverability.identity_terms,
            "total_probes": analysis.discoverability.total_probes,
            "resolved_to_expected": analysis.discoverability.resolved_to_expected,
            "ratio": analysis.discoverability.ratio(),
            "truncated": analysis.discoverability.truncated,
            "kernel_error": analysis.discoverability.kernel_error,
            "diagnostic_only": true,
            "meanings_requiring_review": analysis.discoverability.meanings_requiring_review.iter().map(|id| id.as_str()).collect::<Vec<_>>(),
            "meanings": analysis.discoverability.meanings.iter().map(|row| serde_json::json!({
                "meaning": row.meaning.as_str(),
                "identity_bearing_samples": row.identity_bearing_samples,
                "has_identity_free_sample": row.has_identity_free_sample,
                "probes": row.probes,
                "resolved_to_expected": row.resolved_to_expected,
            })).collect::<Vec<_>>(),
            "probes": analysis.discoverability.probes.iter().map(|row| serde_json::json!({
                "meaning": row.meaning.as_str(),
                "language": row.language,
                "source_sample": row.source_sample,
                "input": row.input,
                "decision": row.decision,
                "resolved_meaning": row.resolved_meaning.as_ref().map(|id| id.as_str()),
                "expected_score": row.expected_score,
            })).collect::<Vec<_>>(),
        },
        "repair_boundaries": {
            "repair_floor": analysis.repair_boundaries.repair_floor,
            "resolution_threshold": analysis.repair_boundaries.resolution_threshold,
            "warning_margin": analysis.repair_boundaries.warning_margin,
            "fragile_count": analysis.repair_boundaries.fragile_count,
            "kernel_error": analysis.repair_boundaries.kernel_error,
            "probes": analysis.repair_boundaries.probes.iter().map(|row| serde_json::json!({
                "test_id": row.test_id,
                "expected_meaning": row.expected_meaning.as_str(),
                "candidate_meaning": row.candidate_meaning.as_ref().map(|id| id.as_str()),
                "score": row.score,
                "inside_repair_band": row.inside_repair_band,
                "distance_to_floor": row.distance_to_floor,
                "distance_to_resolution": row.distance_to_resolution,
                "fragile": row.fragile,
            })).collect::<Vec<_>>(),
        },
        "ambiguity": {
            "compared_pairs": analysis.ambiguity.compared_pairs, "truncated_pairs": analysis.ambiguity.truncated_pairs, "truncated_results": analysis.ambiguity.truncated_results,
            "pairs": analysis.ambiguity.pairs.iter().map(|row| serde_json::json!({"left_meaning": row.left_meaning.as_str(), "left_sample": row.left_sample, "right_meaning": row.right_meaning.as_str(), "right_sample": row.right_sample, "similarity": row.similarity, "exact": row.exact})).collect::<Vec<_>>(),
        },
    })
}

pub(super) fn capabilities_json(project: &ComposedProject) -> serde_json::Value {
    serde_json::Value::Array(
        project
            .capability_catalog
            .capability_ids()
            .filter_map(|id| project.capability_catalog.definition(id))
            .map(capability_summary_json)
            .collect::<Vec<_>>(),
    )
}

pub(super) fn capability_summary_json(
    definition: &gvya_kernel::capability::CapabilityDefinition,
) -> serde_json::Value {
    serde_json::json!({
        "id": definition.contract.id.as_str(), "version": definition.contract.version.as_str(),
        "title": definition.contract.title, "effect_class": effect_class_label(definition.contract.effect_class),
        "confirmation_hint": confirmation_hint_label(definition.contract.confirmation_hint),
    })
}

pub(super) fn capability_detail_json(
    definition: &gvya_kernel::capability::CapabilityDefinition,
) -> serde_json::Value {
    let input_schema =
        serde_json::from_str::<serde_json::Value>(definition.contract.input_schema.as_str())
            .unwrap_or_else(|_| {
                serde_json::Value::String(definition.contract.input_schema.as_str().to_owned())
            });
    let output_schema = definition.contract.output_schema.as_ref().map(|schema| {
        serde_json::from_str::<serde_json::Value>(schema.as_str())
            .unwrap_or_else(|_| serde_json::Value::String(schema.as_str().to_owned()))
    });
    serde_json::json!({
        "id": definition.contract.id.as_str(), "version": definition.contract.version.as_str(), "title": definition.contract.title, "description": definition.contract.description,
        "input_schema": input_schema, "output_schema": output_schema,
        "reference_kinds": definition.contract.reference_kinds.iter().map(|kind| kind.as_str()).collect::<Vec<_>>(),
        "effect_class": effect_class_label(definition.contract.effect_class), "confirmation_hint": confirmation_hint_label(definition.contract.confirmation_hint),
        "host_effects": definition.host_effects.iter().map(|effect| serde_json::json!({"resource": effect.resource, "kind": format!("{:?}", effect.kind).to_ascii_lowercase(), "summary": effect.summary})).collect::<Vec<_>>(),
    })
}

const fn effect_class_label(value: EffectClass) -> &'static str {
    match value {
        EffectClass::Pure => "pure",
        EffectClass::Reversible => "reversible",
        EffectClass::Irreversible => "irreversible",
        EffectClass::External => "external",
    }
}

const fn confirmation_hint_label(value: ConfirmationHint) -> &'static str {
    match value {
        ConfirmationHint::Never => "never",
        ConfirmationHint::Conditional => "conditional",
        ConfirmationHint::Always => "always",
    }
}

pub(super) fn test_report_json(
    report: &gvya_compiler::testing::TestRunReport,
) -> serde_json::Value {
    serde_json::json!({
        "format": "gvya.cli.test", "version": 1,
        "summary": {
            "regression_total": report.regression_total, "regression_executed": report.regression_executed, "regression_passed": report.regression_passed,
            "scenario_total": report.scenario_total, "scenario_executed": report.scenario_executed, "scenario_passed": report.scenario_passed, "stopped_early": report.stopped_early,
        },
        "regression_cases": report.cases.iter().map(|row| serde_json::json!({
            "id": row.id.as_str(), "ok": row.ok,
            "failures": row.failures.iter().map(|failure| serde_json::json!({"code": failure.code, "detail": failure.detail})).collect::<Vec<_>>(),
            "observation": row.observation.as_ref().map(observation_json),
        })).collect::<Vec<_>>(),
        "scenarios": report.scenarios.iter().map(|scenario| serde_json::json!({
            "id": scenario.id.as_str(), "ok": scenario.ok,
            "steps": scenario.steps.iter().map(|step| serde_json::json!({"step": step.step, "type": step.kind, "input": step.input, "ok": step.ok, "failures": step.failures.iter().map(|failure| serde_json::json!({"code": failure.code, "detail": failure.detail})).collect::<Vec<_>>(), "observation": step.observation.as_ref().map(observation_json)})).collect::<Vec<_>>(),
        })).collect::<Vec<_>>(),
    })
}

pub(super) fn observation_json(observation: &SimulationObservation) -> serde_json::Value {
    let why_codes = observation
        .why
        .sections
        .iter()
        .flat_map(|section| section.entries.iter())
        .map(|entry| entry.code.as_str())
        .collect::<Vec<_>>();
    serde_json::json!({
        "meaning": observation.meaning.as_ref().map(|meaning| meaning.id.as_str()), "semantic_score": observation.semantic_score,
        "conversation_mode": observation.conversation_mode, "response_ids": observation.response_ids.iter().map(|id| id.as_str()).collect::<Vec<_>>(),
        "response_texts": observation.response_texts,
        "capability_proposals": observation.proposals.iter().map(|proposal| serde_json::json!({"id": proposal.id.as_str(), "capability": proposal.capability.as_str(), "version": proposal.capability_version.as_str(), "fingerprint": proposal.fingerprint})).collect::<Vec<_>>(),
        "proposal_receipts": observation.proposal_receipts.iter().map(|receipt| {
            let (outcome, reason_code) = match &receipt.outcome {
                gvya_model::AdmissionOutcome::Admitted => ("admitted", None),
                gvya_model::AdmissionOutcome::NeedsConfirmation { reason_code } => ("needs_confirmation", Some(reason_code.as_str())),
                gvya_model::AdmissionOutcome::Rejected { reason_code } => ("rejected", Some(reason_code.as_str())),
            };
            serde_json::json!({"id": receipt.proposal.id.as_str(), "capability": receipt.proposal.capability.as_str(), "version": receipt.proposal.capability_version.as_str(), "outcome": outcome, "reason_code": reason_code})
        }).collect::<Vec<_>>(),
        "capability_result_accepted": observation.capability_result_accepted,
        "capability_result_reason_code": observation.capability_result_reason_code,
        "why_codes": why_codes,
    })
}
