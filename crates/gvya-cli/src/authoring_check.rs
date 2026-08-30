use gvya_compiler::{
    analysis::ProjectAnalysis,
    audit::AuditReport,
    testing::{SimulationObservation, TestRunReport},
};
use serde_json::{Map, Value, json};

#[derive(Clone, Debug, PartialEq)]
pub struct AuthoringAcceptancePolicy {
    pub max_audit_warnings: Option<usize>,
    pub require_tests: bool,
    pub min_meaning_expectation_coverage: Option<f64>,
    pub min_behavior_expectation_coverage: Option<f64>,
    pub min_capability_expectation_coverage: Option<f64>,
    pub max_ambiguity_pairs: Option<usize>,
    pub max_fallback_observation_ratio: Option<f64>,
}

impl Default for AuthoringAcceptancePolicy {
    fn default() -> Self {
        Self {
            max_audit_warnings: None,
            require_tests: false,
            min_meaning_expectation_coverage: None,
            min_behavior_expectation_coverage: None,
            min_capability_expectation_coverage: None,
            max_ambiguity_pairs: None,
            max_fallback_observation_ratio: None,
        }
    }
}

impl AuthoringAcceptancePolicy {
    pub fn parse(value: &Value) -> Result<Self, String> {
        let object = value
            .as_object()
            .ok_or_else(|| "authoring acceptance policy must be a JSON object".to_owned())?;
        let allowed = [
            "format",
            "version",
            "max_audit_warnings",
            "require_tests",
            "min_meaning_expectation_coverage",
            "min_behavior_expectation_coverage",
            "min_capability_expectation_coverage",
            "max_ambiguity_pairs",
            "max_fallback_observation_ratio",
        ];
        let extras = object
            .keys()
            .filter(|key| !allowed.contains(&key.as_str()))
            .cloned()
            .collect::<Vec<_>>();
        if !extras.is_empty() {
            return Err(format!(
                "authoring acceptance policy contains unsupported fields: {}",
                extras.join(", ")
            ));
        }
        if object.get("format").and_then(Value::as_str) != Some("gvya.authoring.acceptance")
            || object.get("version").and_then(Value::as_u64) != Some(1)
        {
            return Err(
                "authoring acceptance policy requires format gvya.authoring.acceptance version 1"
                    .into(),
            );
        }
        Ok(Self {
            max_audit_warnings: optional_usize(object, "max_audit_warnings")?,
            require_tests: optional_bool(object, "require_tests")?.unwrap_or(false),
            min_meaning_expectation_coverage: optional_ratio(
                object,
                "min_meaning_expectation_coverage",
            )?,
            min_behavior_expectation_coverage: optional_ratio(
                object,
                "min_behavior_expectation_coverage",
            )?,
            min_capability_expectation_coverage: optional_ratio(
                object,
                "min_capability_expectation_coverage",
            )?,
            max_ambiguity_pairs: optional_usize(object, "max_ambiguity_pairs")?,
            max_fallback_observation_ratio: optional_ratio(
                object,
                "max_fallback_observation_ratio",
            )?,
        })
    }

    pub fn to_json(&self) -> Value {
        json!({
            "format": "gvya.authoring.acceptance",
            "version": 1,
            "max_audit_warnings": self.max_audit_warnings,
            "require_tests": self.require_tests,
            "min_meaning_expectation_coverage": self.min_meaning_expectation_coverage,
            "min_behavior_expectation_coverage": self.min_behavior_expectation_coverage,
            "min_capability_expectation_coverage": self.min_capability_expectation_coverage,
            "max_ambiguity_pairs": self.max_ambiguity_pairs,
            "max_fallback_observation_ratio": self.max_fallback_observation_ratio,
        })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct FallbackObservationMeasurement {
    pub observed_turns: usize,
    pub fallback_turns: usize,
    pub ratio: Option<f64>,
}

impl FallbackObservationMeasurement {
    pub fn to_json(&self) -> Value {
        json!({
            "observed_turns": self.observed_turns,
            "fallback_turns": self.fallback_turns,
            "ratio": self.ratio,
        })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct AuthoringGateResult {
    pub accepted: bool,
    pub obligations: Vec<Value>,
    pub fallback: FallbackObservationMeasurement,
}

pub fn evaluate(
    policy: &AuthoringAcceptancePolicy,
    audit: &AuditReport,
    analysis: &ProjectAnalysis,
    tests: Option<&TestRunReport>,
    build_error: Option<&str>,
    runtime_error: Option<&str>,
) -> AuthoringGateResult {
    let mut obligations = Vec::new();
    if let Some(error) = build_error {
        obligations.push(obligation(
            "authoring.build_failed",
            "repair_source",
            "Canonical build failed; repair source before acceptance.",
            json!({"error": error}),
        ));
    }
    if let Some(error) = runtime_error {
        obligations.push(obligation(
            "authoring.runtime_verification_failed",
            "repair_source",
            "The built artifact could not enter canonical runtime verification.",
            json!({"error": error}),
        ));
    }
    if audit.summary.errors > 0 {
        let codes = audit
            .issues
            .iter()
            .filter(|issue| issue.severity.label() == "error")
            .map(|issue| issue.code.as_str())
            .collect::<Vec<_>>();
        obligations.push(obligation(
            "authoring.audit_errors",
            "resolve_audit_errors",
            "Resolve every compiler-owned audit error.",
            json!({"count": audit.summary.errors, "codes": codes}),
        ));
    }
    if let Some(maximum) = policy.max_audit_warnings {
        if audit.summary.warnings > maximum {
            let codes = audit
                .issues
                .iter()
                .filter(|issue| issue.severity.label() == "warning")
                .map(|issue| issue.code.as_str())
                .collect::<Vec<_>>();
            obligations.push(obligation(
                "authoring.audit_warning_budget",
                "resolve_audit_warnings",
                "Reduce audit warnings to the configured acceptance budget.",
                json!({"actual": audit.summary.warnings, "maximum": maximum, "codes": codes}),
            ));
        }
    }

    let fallback = fallback_measurement(tests);
    match tests {
        Some(report) => {
            let total = report.regression_total + report.scenario_total;
            if policy.require_tests && total == 0 {
                obligations.push(obligation(
                    "authoring.tests_required",
                    "add_tests",
                    "The acceptance policy requires at least one Regression Case or Conversation Scenario.",
                    json!({}),
                ));
            }
            if !tests_passed(report) {
                let failed_regressions = report
                    .cases
                    .iter()
                    .filter(|row| !row.ok)
                    .map(|row| row.id.as_str())
                    .collect::<Vec<_>>();
                let failed_scenarios = report
                    .scenarios
                    .iter()
                    .filter(|row| !row.ok)
                    .map(|row| row.id.as_str())
                    .collect::<Vec<_>>();
                obligations.push(obligation(
                    "authoring.tests_failed",
                    "repair_tests_or_source",
                    "All authored Regression Cases and Conversation Scenarios must execute and pass.",
                    json!({
                        "failed_regressions": failed_regressions,
                        "failed_scenarios": failed_scenarios,
                        "stopped_early": report.stopped_early,
                    }),
                ));
            }
        }
        None => obligations.push(obligation(
            "authoring.tests_not_run",
            "repair_source",
            "Tests could not run because build/runtime verification did not complete.",
            json!({}),
        )),
    }

    expectation_coverage_gate(
        &mut obligations,
        "meanings",
        policy.min_meaning_expectation_coverage,
        analysis.coverage.meanings.ratio(),
        analysis
            .coverage
            .meanings
            .uncovered
            .iter()
            .map(|id| id.as_str())
            .collect(),
    );
    expectation_coverage_gate(
        &mut obligations,
        "behaviors",
        policy.min_behavior_expectation_coverage,
        analysis.coverage.behaviors.ratio(),
        analysis
            .coverage
            .behaviors
            .uncovered
            .iter()
            .map(|id| id.as_str())
            .collect(),
    );
    expectation_coverage_gate(
        &mut obligations,
        "capabilities",
        policy.min_capability_expectation_coverage,
        analysis.coverage.capabilities.ratio(),
        analysis
            .coverage
            .capabilities
            .uncovered
            .iter()
            .map(|id| id.as_str())
            .collect(),
    );

    if let Some(maximum) = policy.max_ambiguity_pairs {
        if analysis.ambiguity.truncated_pairs || analysis.ambiguity.truncated_results {
            obligations.push(obligation(
                "authoring.ambiguity_measurement_truncated",
                "narrow_or_restructure_semantics",
                "Ambiguity analysis was truncated, so the configured ambiguity gate cannot be proven.",
                json!({
                    "returned_pairs": analysis.ambiguity.pairs.len(),
                    "maximum": maximum,
                    "truncated_pairs": analysis.ambiguity.truncated_pairs,
                    "truncated_results": analysis.ambiguity.truncated_results,
                }),
            ));
        } else if analysis.ambiguity.pairs.len() > maximum {
            obligations.push(obligation(
                "authoring.ambiguity_budget",
                "resolve_semantic_ambiguity",
                "Reduce semantic overlap pairs to the configured acceptance budget.",
                json!({"actual": analysis.ambiguity.pairs.len(), "maximum": maximum}),
            ));
        }
    }

    if let Some(maximum) = policy.max_fallback_observation_ratio {
        match fallback.ratio {
            Some(actual) if actual > maximum => obligations.push(obligation(
                "authoring.fallback_dependence",
                "reduce_fallback_dependence",
                "Reduce observed fallback use to the configured acceptance ratio.",
                json!({"actual": actual, "maximum": maximum, "fallback_turns": fallback.fallback_turns, "observed_turns": fallback.observed_turns}),
            )),
            None => obligations.push(obligation(
                "authoring.fallback_measurement_unavailable",
                "add_tests",
                "Fallback dependence cannot be measured without executed test observations.",
                json!({"maximum": maximum}),
            )),
            Some(_) => {}
        }
    }

    AuthoringGateResult {
        accepted: obligations.is_empty(),
        obligations,
        fallback,
    }
}

fn tests_passed(report: &TestRunReport) -> bool {
    report.regression_executed == report.regression_total
        && report.regression_passed == report.regression_total
        && report.scenario_executed == report.scenario_total
        && report.scenario_passed == report.scenario_total
        && !report.stopped_early
}

fn fallback_measurement(tests: Option<&TestRunReport>) -> FallbackObservationMeasurement {
    let mut observed_turns = 0usize;
    let mut fallback_turns = 0usize;
    let mut observe = |observation: &SimulationObservation| {
        observed_turns += 1;
        if matches!(
            observation.conversation_mode.as_deref(),
            Some("fallback" | "repeat_fallback")
        ) {
            fallback_turns += 1;
        }
    };
    if let Some(report) = tests {
        for case in &report.cases {
            if let Some(observation) = &case.observation {
                observe(observation);
            }
        }
        for scenario in &report.scenarios {
            for step in &scenario.steps {
                if let Some(observation) = &step.observation {
                    observe(observation);
                }
            }
        }
    }
    FallbackObservationMeasurement {
        observed_turns,
        fallback_turns,
        ratio: (observed_turns > 0).then(|| fallback_turns as f64 / observed_turns as f64),
    }
}

fn expectation_coverage_gate(
    obligations: &mut Vec<Value>,
    kind: &str,
    minimum: Option<f64>,
    actual: f64,
    uncovered: Vec<&str>,
) {
    if let Some(minimum) = minimum {
        if actual < minimum {
            obligations.push(obligation(
                &format!("authoring.{kind}_expectation_coverage"),
                "add_or_strengthen_tests",
                &format!(
                    "Increase {kind} expectation coverage to the configured acceptance ratio."
                ),
                json!({"actual": actual, "minimum": minimum, "uncovered": uncovered}),
            ));
        }
    }
}

fn obligation(code: &str, action: &str, summary: &str, details: Value) -> Value {
    json!({"code": code, "action": action, "summary": summary, "details": details})
}

fn optional_usize(object: &Map<String, Value>, key: &str) -> Result<Option<usize>, String> {
    match object.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(value) => value
            .as_u64()
            .and_then(|number| usize::try_from(number).ok())
            .map(Some)
            .ok_or_else(|| format!("{key} must be a non-negative integer or null")),
    }
}

fn optional_bool(object: &Map<String, Value>, key: &str) -> Result<Option<bool>, String> {
    match object.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(value) => value
            .as_bool()
            .map(Some)
            .ok_or_else(|| format!("{key} must be a boolean or null")),
    }
}

fn optional_ratio(object: &Map<String, Value>, key: &str) -> Result<Option<f64>, String> {
    match object.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(value) => {
            let number = value
                .as_f64()
                .ok_or_else(|| format!("{key} must be a number from 0 through 1 or null"))?;
            if number.is_finite() && (0.0..=1.0).contains(&number) {
                Ok(Some(number))
            } else {
                Err(format!("{key} must be a number from 0 through 1 or null"))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn policy_parser_accepts_the_versioned_contract() {
        let policy = AuthoringAcceptancePolicy::parse(&json!({
            "format": "gvya.authoring.acceptance",
            "version": 1,
            "max_audit_warnings": 2,
            "require_tests": true,
            "min_meaning_expectation_coverage": 0.8,
            "min_behavior_expectation_coverage": 0.7,
            "min_capability_expectation_coverage": 1.0,
            "max_ambiguity_pairs": 3,
            "max_fallback_observation_ratio": 0.15,
        }))
        .unwrap();
        assert_eq!(policy.max_audit_warnings, Some(2));
        assert!(policy.require_tests);
        assert_eq!(policy.max_fallback_observation_ratio, Some(0.15));
    }

    #[test]
    fn policy_parser_rejects_unknown_fields_and_invalid_ratios() {
        assert!(
            AuthoringAcceptancePolicy::parse(&json!({
                "format": "gvya.authoring.acceptance",
                "version": 1,
                "score": 90,
            }))
            .is_err()
        );
        assert!(
            AuthoringAcceptancePolicy::parse(&json!({
                "format": "gvya.authoring.acceptance",
                "version": 1,
                "min_meaning_expectation_coverage": 1.01,
            }))
            .is_err()
        );
    }
}
