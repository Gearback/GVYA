//! Renderer/runtime-independent regression and bounded conversation-scenario model.
//!
//! The runner consumes a `SimulationDriver`; package/audit/test layer does not invent a second runtime facade.

use std::collections::BTreeMap;

use gvya_kernel::{
    CapabilityResultInput, ResolverReferenceCandidate, capability::conversation_scalar,
    conversation::HintRequest,
};
use gvya_model::{
    AdmissionOutcome, CapabilityId, CapabilityVersion, ConfirmationGrant, ConfirmationId,
    ContextSnapshot, FollowupId, GvyaState, HostReference, InvocationProposal, Meaning, MeaningId,
    ResponseId, ScenarioId, TestCaseId, TopicId, TraceCode, Value, WhyReport,
};

#[derive(Clone, Debug, PartialEq)]
pub struct SimulationOpenInput {
    pub language: Option<String>,
    pub context: ContextSnapshot,
    pub state: GvyaState,
    pub seed: Option<u64>,
    pub unix_time_ms: Option<i64>,
    pub confirmations: Vec<ConfirmationGrant>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SimulationTurnInput {
    pub utterance: String,
    pub language: Option<String>,
    pub context: ContextSnapshot,
    pub state: GvyaState,
    pub reference_candidates: Vec<ResolverReferenceCandidate>,
    pub resolver_context: BTreeMap<String, Value>,
    pub hint: HintRequest,
    pub seed: Option<u64>,
    pub unix_time_ms: Option<i64>,
    pub confirmations: Vec<ConfirmationGrant>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SimulationCapabilityResultInput {
    pub proposal: InvocationProposal,
    pub result: CapabilityResultInput,
    pub language: Option<String>,
    pub context: ContextSnapshot,
    pub state: GvyaState,
    pub seed: Option<u64>,
    pub unix_time_ms: Option<i64>,
    pub confirmations: Vec<ConfirmationGrant>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum SimulationInteractionInput {
    Open(SimulationOpenInput),
    Turn(SimulationTurnInput),
    CapabilityResult(SimulationCapabilityResultInput),
}

#[derive(Clone, Debug, PartialEq)]
pub struct SimulationProposalReceipt {
    pub proposal: InvocationProposal,
    pub outcome: AdmissionOutcome,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SimulationObservation {
    pub meaning: Option<Meaning>,
    pub semantic_score: Option<f64>,
    pub conversation_mode: Option<String>,
    pub response_ids: Vec<ResponseId>,
    pub response_texts: Vec<String>,
    pub state: GvyaState,
    /// Admitted proposals only; retained for the existing capability expectation contract.
    pub proposals: Vec<InvocationProposal>,
    /// Every host-visible proposal receipt, including `needs_confirmation` proposals.
    pub proposal_receipts: Vec<SimulationProposalReceipt>,
    /// Populated only for a capability-result interaction.
    pub capability_result_accepted: Option<bool>,
    /// Populated only when capability-result validation reports a stable reason code.
    pub capability_result_reason_code: Option<String>,
    pub why: WhyReport,
}

pub trait SimulationDriver {
    type Error: std::fmt::Display;

    fn run_interaction(
        &mut self,
        input: SimulationInteractionInput,
    ) -> Result<SimulationObservation, Self::Error>;
}

#[derive(Clone, Debug, PartialEq)]
pub struct ExpectedCapability {
    pub id: CapabilityId,
    pub version: Option<CapabilityVersion>,
    /// `None` accepts any admitted argument object; `Some` requires exact proposal arguments.
    pub arguments: Option<BTreeMap<String, Value>>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ExpectedProposalOutcome {
    Admitted,
    NeedsConfirmation,
    Rejected,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ExpectedProposalReceipt {
    pub id: CapabilityId,
    pub version: Option<CapabilityVersion>,
    pub arguments: Option<BTreeMap<String, Value>>,
    pub outcome: ExpectedProposalOutcome,
    pub reason_code: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct TurnExpectation {
    pub meaning: Option<MeaningId>,
    pub forbidden_meanings: Vec<MeaningId>,
    pub meaning_slots: BTreeMap<String, Value>,
    pub meaning_references: Vec<HostReference>,
    pub min_semantic_score: Option<f64>,
    pub conversation_mode: Option<String>,
    pub response_ids: Vec<ResponseId>,
    pub forbidden_response_ids: Vec<ResponseId>,
    pub response_contains: Vec<String>,
    pub response_not_contains: Vec<String>,
    pub author_values: BTreeMap<String, Value>,
    pub conversation_values: BTreeMap<String, Value>,
    pub active_topic: Option<TopicId>,
    pub active_followup: Option<FollowupId>,
    pub capabilities: Vec<ExpectedCapability>,
    /// Admission receipts, including confirmation-required and rejected proposals.
    pub proposal_receipts: Vec<ExpectedProposalReceipt>,
    pub forbidden_capabilities: Vec<CapabilityId>,
    pub capability_result_accepted: Option<bool>,
    pub capability_result_reason_code: Option<String>,
    pub why_codes: Vec<TraceCode>,
    pub forbidden_why_codes: Vec<TraceCode>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RegressionCase {
    pub id: TestCaseId,
    pub description: String,
    pub input: String,
    pub language: Option<String>,
    pub context: ContextSnapshot,
    pub initial_state: GvyaState,
    pub seed: Option<u64>,
    pub unix_time_ms: Option<i64>,
    pub expectation: TurnExpectation,
    pub generated: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ScenarioOpenStep {
    pub language: Option<String>,
    /// When absent, the scenario-level context is reused for this interaction.
    pub context: Option<ContextSnapshot>,
    pub seed: Option<u64>,
    pub unix_time_ms: Option<i64>,
    pub expectation: TurnExpectation,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ScenarioTurnStep {
    pub say: String,
    pub language: Option<String>,
    /// When absent, the scenario-level context is reused for this interaction.
    pub context: Option<ContextSnapshot>,
    pub reference_candidates: Vec<ResolverReferenceCandidate>,
    pub resolver_context: BTreeMap<String, Value>,
    pub hint: HintRequest,
    pub seed: Option<u64>,
    pub unix_time_ms: Option<i64>,
    pub expectation: TurnExpectation,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ScenarioCapabilityResultStep {
    /// One-based earlier scenario step that emitted the proposal receipt.
    pub proposal_from_step: usize,
    /// Optional selector when that step emitted multiple proposal receipts.
    pub proposal_capability: Option<CapabilityId>,
    /// Optional one-based ordinal among receipts after the capability filter.
    pub proposal_ordinal: Option<usize>,
    pub succeeded: bool,
    pub output: Option<Value>,
    pub error_code: Option<String>,
    pub language: Option<String>,
    /// When absent, the scenario-level context is reused for this interaction.
    pub context: Option<ContextSnapshot>,
    pub seed: Option<u64>,
    pub unix_time_ms: Option<i64>,
    pub expectation: TurnExpectation,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ScenarioConfirmStep {
    /// One-based earlier interaction step to replay with an exact confirmation grant.
    pub proposal_from_step: usize,
    /// Optional selector when that step emitted multiple proposal receipts.
    pub proposal_capability: Option<CapabilityId>,
    /// Optional one-based ordinal among receipts after the capability filter.
    pub proposal_ordinal: Option<usize>,
    pub confirmed: bool,
    /// Optional refreshed host context for the canonical retry.
    pub context: Option<ContextSnapshot>,
    /// Optional refreshed explicit wall-clock fact for the canonical retry.
    pub unix_time_ms: Option<i64>,
    pub expectation: TurnExpectation,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ScenarioStep {
    Open(ScenarioOpenStep),
    Turn(ScenarioTurnStep),
    CapabilityResult(ScenarioCapabilityResultStep),
    Confirm(ScenarioConfirmStep),
}

impl ScenarioStep {
    #[must_use]
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Open(_) => "open",
            Self::Turn(_) => "turn",
            Self::CapabilityResult(_) => "capability_result",
            Self::Confirm(_) => "confirm",
        }
    }

    #[must_use]
    pub fn expectation(&self) -> &TurnExpectation {
        match self {
            Self::Open(step) => &step.expectation,
            Self::Turn(step) => &step.expectation,
            Self::CapabilityResult(step) => &step.expectation,
            Self::Confirm(step) => &step.expectation,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ConversationScenario {
    pub id: ScenarioId,
    pub description: String,
    pub context: ContextSnapshot,
    pub initial_state: GvyaState,
    pub steps: Vec<ScenarioStep>,
    pub generated: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TestRunLimits {
    pub max_regression_cases: usize,
    pub max_scenarios: usize,
    pub max_steps_per_scenario: usize,
    pub max_failures: usize,
}

impl Default for TestRunLimits {
    fn default() -> Self {
        Self {
            max_regression_cases: 10_000,
            max_scenarios: 2_000,
            max_steps_per_scenario: 256,
            max_failures: 2_000,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExpectationFailure {
    pub code: String,
    pub detail: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct StepRunResult {
    pub step: usize,
    pub kind: String,
    pub input: String,
    pub ok: bool,
    pub failures: Vec<ExpectationFailure>,
    pub observation: Option<SimulationObservation>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CaseRunResult {
    pub id: TestCaseId,
    pub ok: bool,
    pub failures: Vec<ExpectationFailure>,
    pub observation: Option<SimulationObservation>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ScenarioRunResult {
    pub id: ScenarioId,
    pub ok: bool,
    pub steps: Vec<StepRunResult>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TestRunReport {
    /// Authored totals, not merely the executed prefix.
    pub regression_total: usize,
    pub regression_executed: usize,
    pub regression_passed: usize,
    pub scenario_total: usize,
    pub scenario_executed: usize,
    pub scenario_passed: usize,
    /// True whenever any authored test/step was not executed because a safety budget was reached.
    pub stopped_early: bool,
    pub cases: Vec<CaseRunResult>,
    pub scenarios: Vec<ScenarioRunResult>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TestSuite {
    pub regression_cases: Vec<RegressionCase>,
    pub scenarios: Vec<ConversationScenario>,
}

impl TestSuite {
    #[must_use]
    pub fn empty() -> Self {
        Self {
            regression_cases: Vec::new(),
            scenarios: Vec::new(),
        }
    }
}

#[derive(Clone, Debug)]
struct ScenarioHistoryEntry {
    input: SimulationInteractionInput,
    observation: SimulationObservation,
}

pub fn run_test_suite<D: SimulationDriver>(
    driver: &mut D,
    suite: &TestSuite,
    limits: TestRunLimits,
) -> TestRunReport {
    let regression_count = suite
        .regression_cases
        .len()
        .min(limits.max_regression_cases);
    let scenario_count = suite.scenarios.len().min(limits.max_scenarios);
    let mut failures_seen = 0usize;
    let mut cases = Vec::new();
    let mut scenarios = Vec::new();
    let mut stopped_early = suite.regression_cases.len() > limits.max_regression_cases
        || suite.scenarios.len() > limits.max_scenarios;

    for case in suite.regression_cases.iter().take(regression_count) {
        if failures_seen >= limits.max_failures {
            stopped_early = true;
            break;
        }
        let input = SimulationInteractionInput::Turn(SimulationTurnInput {
            utterance: case.input.clone(),
            language: case.language.clone(),
            context: case.context.clone(),
            state: case.initial_state.clone(),
            reference_candidates: Vec::new(),
            resolver_context: BTreeMap::new(),
            hint: HintRequest::None,
            seed: case.seed,
            unix_time_ms: case.unix_time_ms,
            confirmations: Vec::new(),
        });
        match driver.run_interaction(input) {
            Ok(observation) => {
                let failures = check_expectation(&case.expectation, &observation);
                if !failures.is_empty() {
                    failures_seen += 1;
                }
                cases.push(CaseRunResult {
                    id: case.id.clone(),
                    ok: failures.is_empty(),
                    failures,
                    observation: Some(observation),
                });
            }
            Err(error) => {
                failures_seen += 1;
                cases.push(CaseRunResult {
                    id: case.id.clone(),
                    ok: false,
                    failures: vec![ExpectationFailure {
                        code: "driver_error".to_owned(),
                        detail: error.to_string(),
                    }],
                    observation: None,
                });
            }
        }
    }

    for scenario in suite.scenarios.iter().take(scenario_count) {
        if failures_seen >= limits.max_failures {
            stopped_early = true;
            break;
        }
        let mut state = scenario.initial_state.clone();
        let mut history = Vec::<ScenarioHistoryEntry>::new();
        let mut step_results = Vec::new();
        let mut scenario_ok = true;
        for (index, step) in scenario
            .steps
            .iter()
            .take(limits.max_steps_per_scenario)
            .enumerate()
        {
            let step_number = index + 1;
            let built = build_scenario_input(scenario, step, step_number, &state, &history);
            let input = match built {
                Ok(input) => input,
                Err(detail) => {
                    failures_seen += 1;
                    scenario_ok = false;
                    step_results.push(StepRunResult {
                        step: step_number,
                        kind: step.kind().to_owned(),
                        input: scenario_step_input_label(step),
                        ok: false,
                        failures: vec![ExpectationFailure {
                            code: "scenario_input_invalid".to_owned(),
                            detail,
                        }],
                        observation: None,
                    });
                    break;
                }
            };
            let input_label = simulation_input_label(&input);
            match driver.run_interaction(input.clone()) {
                Ok(observation) => {
                    let failures = check_expectation(step.expectation(), &observation);
                    let ok = failures.is_empty();
                    state = observation.state.clone();
                    history.push(ScenarioHistoryEntry {
                        input,
                        observation: observation.clone(),
                    });
                    step_results.push(StepRunResult {
                        step: step_number,
                        kind: step.kind().to_owned(),
                        input: input_label,
                        ok,
                        failures,
                        observation: Some(observation),
                    });
                    if !ok {
                        failures_seen += 1;
                        scenario_ok = false;
                        break;
                    }
                }
                Err(error) => {
                    failures_seen += 1;
                    scenario_ok = false;
                    step_results.push(StepRunResult {
                        step: step_number,
                        kind: step.kind().to_owned(),
                        input: input_label,
                        ok: false,
                        failures: vec![ExpectationFailure {
                            code: "driver_error".to_owned(),
                            detail: error.to_string(),
                        }],
                        observation: None,
                    });
                    break;
                }
            }
        }
        if scenario.steps.len() > limits.max_steps_per_scenario {
            stopped_early = true;
            scenario_ok = false;
            step_results.push(StepRunResult {
                step: limits.max_steps_per_scenario + 1,
                kind: "limit".to_owned(),
                input: String::new(),
                ok: false,
                failures: vec![ExpectationFailure {
                    code: "scenario_step_limit_exceeded".to_owned(),
                    detail: format!(
                        "scenario has {} steps; limit is {}",
                        scenario.steps.len(),
                        limits.max_steps_per_scenario
                    ),
                }],
                observation: None,
            });
            failures_seen += 1;
        }
        scenarios.push(ScenarioRunResult {
            id: scenario.id.clone(),
            ok: scenario_ok,
            steps: step_results,
        });
    }

    let regression_passed = cases.iter().filter(|row| row.ok).count();
    let scenario_passed = scenarios.iter().filter(|row| row.ok).count();
    TestRunReport {
        regression_total: suite.regression_cases.len(),
        regression_executed: cases.len(),
        regression_passed,
        scenario_total: suite.scenarios.len(),
        scenario_executed: scenarios.len(),
        scenario_passed,
        stopped_early,
        cases,
        scenarios,
    }
}

fn build_scenario_input(
    scenario: &ConversationScenario,
    step: &ScenarioStep,
    step_number: usize,
    state: &GvyaState,
    history: &[ScenarioHistoryEntry],
) -> Result<SimulationInteractionInput, String> {
    let context_or_default = |context: &Option<ContextSnapshot>| {
        context.clone().unwrap_or_else(|| scenario.context.clone())
    };
    match step {
        ScenarioStep::Open(step) => Ok(SimulationInteractionInput::Open(SimulationOpenInput {
            language: step.language.clone(),
            context: context_or_default(&step.context),
            state: state.clone(),
            seed: step.seed,
            unix_time_ms: step.unix_time_ms,
            confirmations: Vec::new(),
        })),
        ScenarioStep::Turn(step) => Ok(SimulationInteractionInput::Turn(SimulationTurnInput {
            utterance: step.say.clone(),
            language: step.language.clone(),
            context: context_or_default(&step.context),
            state: state.clone(),
            reference_candidates: step.reference_candidates.clone(),
            resolver_context: step.resolver_context.clone(),
            hint: step.hint.clone(),
            seed: step.seed,
            unix_time_ms: step.unix_time_ms,
            confirmations: Vec::new(),
        })),
        ScenarioStep::CapabilityResult(step) => {
            let receipt = resolve_proposal_receipt(
                history,
                step.proposal_from_step,
                step.proposal_capability.as_ref(),
                step.proposal_ordinal,
                step_number,
            )?;
            if !matches!(receipt.outcome, AdmissionOutcome::Admitted) {
                return Err(format!(
                    "capability_result step {step_number} requires an admitted proposal from step {}",
                    step.proposal_from_step
                ));
            }
            let proposal = receipt.proposal;
            Ok(SimulationInteractionInput::CapabilityResult(
                SimulationCapabilityResultInput {
                    result: CapabilityResultInput {
                        proposal_id: proposal.id.clone(),
                        succeeded: step.succeeded,
                        output: step.output.clone(),
                        error_code: step.error_code.clone(),
                    },
                    proposal,
                    language: step.language.clone(),
                    context: context_or_default(&step.context),
                    state: state.clone(),
                    seed: step.seed,
                    unix_time_ms: step.unix_time_ms,
                    confirmations: Vec::new(),
                },
            ))
        }
        ScenarioStep::Confirm(step) => {
            if step.proposal_from_step == 0 || step.proposal_from_step >= step_number {
                return Err(format!(
                    "confirm step {step_number} must reference an earlier one-based proposal step"
                ));
            }
            let receipt = resolve_proposal_receipt(
                history,
                step.proposal_from_step,
                step.proposal_capability.as_ref(),
                step.proposal_ordinal,
                step_number,
            )?;
            if !matches!(receipt.outcome, AdmissionOutcome::NeedsConfirmation { .. }) {
                return Err(format!(
                    "confirm step {step_number} requires a needs_confirmation proposal from step {}",
                    step.proposal_from_step
                ));
            }
            let proposal = receipt.proposal;
            let source = history.get(step.proposal_from_step - 1).ok_or_else(|| {
                format!("proposal step {} was not executed", step.proposal_from_step)
            })?;
            let mut retry = source.input.clone();
            let grant = ConfirmationGrant {
                id: ConfirmationId::new(format!(
                    "scenario-confirm-{}-{step_number}",
                    scenario.id.as_str()
                )),
                proposal_id: proposal.id.clone(),
                fingerprint: proposal.fingerprint.clone(),
                confirmed: step.confirmed,
            };
            refresh_retry_input(&mut retry, step.context.as_ref(), step.unix_time_ms, grant);
            Ok(retry)
        }
    }
}

fn resolve_proposal_receipt(
    history: &[ScenarioHistoryEntry],
    from_step: usize,
    capability: Option<&CapabilityId>,
    ordinal: Option<usize>,
    current_step: usize,
) -> Result<SimulationProposalReceipt, String> {
    if from_step == 0 || from_step >= current_step {
        return Err(format!(
            "step {current_step} must reference an earlier one-based proposal step"
        ));
    }
    let entry = history
        .get(from_step - 1)
        .ok_or_else(|| format!("proposal step {from_step} was not executed"))?;
    let matches = entry
        .observation
        .proposal_receipts
        .iter()
        .filter(|receipt| {
            capability
                .map(|expected| &receipt.proposal.capability == expected)
                .unwrap_or(true)
        })
        .collect::<Vec<_>>();
    if let Some(ordinal) = ordinal {
        if ordinal == 0 {
            return Err("proposal_ordinal must be one-based".to_string());
        }
        return matches
            .get(ordinal - 1)
            .map(|receipt| (*receipt).clone())
            .ok_or_else(|| match capability {
                Some(capability) => format!(
                    "step {from_step} emitted only {} proposal(s) for capability {}; proposal_ordinal {ordinal} is out of range",
                    matches.len(), capability.as_str()
                ),
                None => format!(
                    "step {from_step} emitted only {} proposal receipt(s); proposal_ordinal {ordinal} is out of range",
                    matches.len()
                ),
            });
    }
    match matches.as_slice() {
        [receipt] => Ok((*receipt).clone()),
        [] => Err(match capability {
            Some(capability) => format!(
                "step {from_step} emitted no proposal for capability {}",
                capability.as_str()
            ),
            None => format!("step {from_step} emitted no proposal receipt"),
        }),
        _ => Err(match capability {
            Some(capability) => format!(
                "step {from_step} emitted multiple proposals for capability {}; set proposal_ordinal",
                capability.as_str()
            ),
            None => format!(
                "step {from_step} emitted multiple proposal receipts; set proposal_capability and/or proposal_ordinal"
            ),
        }),
    }
}

fn refresh_retry_input(
    input: &mut SimulationInteractionInput,
    context: Option<&ContextSnapshot>,
    unix_time_ms: Option<i64>,
    grant: ConfirmationGrant,
) {
    match input {
        SimulationInteractionInput::Open(input) => {
            if let Some(context) = context {
                input.context = context.clone();
            }
            if unix_time_ms.is_some() {
                input.unix_time_ms = unix_time_ms;
            }
            input.confirmations.push(grant);
        }
        SimulationInteractionInput::Turn(input) => {
            if let Some(context) = context {
                input.context = context.clone();
            }
            if unix_time_ms.is_some() {
                input.unix_time_ms = unix_time_ms;
            }
            input.confirmations.push(grant);
        }
        SimulationInteractionInput::CapabilityResult(input) => {
            if let Some(context) = context {
                input.context = context.clone();
            }
            if unix_time_ms.is_some() {
                input.unix_time_ms = unix_time_ms;
            }
            input.confirmations.push(grant);
        }
    }
}

fn scenario_step_input_label(step: &ScenarioStep) -> String {
    match step {
        ScenarioStep::Open(_) => "open".to_owned(),
        ScenarioStep::Turn(step) => step.say.clone(),
        ScenarioStep::CapabilityResult(step) => {
            format!("capability_result from step {}", step.proposal_from_step)
        }
        ScenarioStep::Confirm(step) => format!("confirm step {}", step.proposal_from_step),
    }
}

fn simulation_input_label(input: &SimulationInteractionInput) -> String {
    match input {
        SimulationInteractionInput::Open(_) => "open".to_owned(),
        SimulationInteractionInput::Turn(input) => input.utterance.clone(),
        SimulationInteractionInput::CapabilityResult(input) => {
            format!("capability_result {}", input.proposal.capability.as_str())
        }
    }
}

#[must_use]
pub fn check_expectation(
    expectation: &TurnExpectation,
    observation: &SimulationObservation,
) -> Vec<ExpectationFailure> {
    let mut failures = Vec::new();
    if let Some(expected) = &expectation.meaning {
        if observation.meaning.as_ref().map(|value| &value.id) != Some(expected) {
            failures.push(failure(
                "meaning_mismatch",
                format!(
                    "expected meaning {}, got {}",
                    expected.as_str(),
                    display_meaning(observation.meaning.as_ref())
                ),
            ));
        }
    }
    if let Some(actual) = &observation.meaning {
        if expectation
            .forbidden_meanings
            .iter()
            .any(|forbidden| forbidden == &actual.id)
        {
            failures.push(failure(
                "forbidden_meaning",
                format!("forbidden meaning selected: {}", actual.id.as_str()),
            ));
        }
        for (name, expected) in &expectation.meaning_slots {
            let matches: Vec<_> = actual
                .slots
                .iter()
                .filter(|slot| slot.name.as_str() == name.as_str())
                .collect();
            if matches.len() != 1 || &matches[0].value != expected {
                failures.push(failure(
                    "meaning_slot_mismatch",
                    format!("meaning slot {name} does not match expectation"),
                ));
            }
        }
        for expected in &expectation.meaning_references {
            if !actual
                .references
                .iter()
                .any(|reference| reference == expected)
            {
                failures.push(failure(
                    "meaning_reference_missing",
                    format!(
                        "meaning reference {}:{} is missing",
                        expected.kind.as_str(),
                        expected.id.as_str()
                    ),
                ));
            }
        }
    } else if !expectation.meaning_slots.is_empty() || !expectation.meaning_references.is_empty() {
        failures.push(failure(
            "meaning_structure_missing",
            "structured meaning is absent".to_owned(),
        ));
    }
    if let Some(minimum) = expectation.min_semantic_score {
        let actual = observation.semantic_score.unwrap_or(0.0);
        if actual < minimum {
            failures.push(failure(
                "semantic_score_below_minimum",
                format!("semantic score {actual:.4} < {minimum:.4}"),
            ));
        }
    }
    if let Some(expected) = &expectation.conversation_mode {
        if observation.conversation_mode.as_deref() != Some(expected.as_str()) {
            failures.push(failure(
                "conversation_mode_mismatch",
                format!(
                    "expected mode {expected}, got {}",
                    observation.conversation_mode.as_deref().unwrap_or("none")
                ),
            ));
        }
    }
    for expected in &expectation.response_ids {
        if !observation
            .response_ids
            .iter()
            .any(|actual| actual == expected)
        {
            failures.push(failure(
                "response_id_missing",
                format!("expected response id {}", expected.as_str()),
            ));
        }
    }
    for forbidden in &expectation.forbidden_response_ids {
        if observation
            .response_ids
            .iter()
            .any(|actual| actual == forbidden)
        {
            failures.push(failure(
                "forbidden_response_id",
                format!("forbidden response id selected: {}", forbidden.as_str()),
            ));
        }
    }
    let joined = observation.response_texts.join("\n");
    for needle in &expectation.response_contains {
        if !joined.contains(needle) {
            failures.push(failure(
                "response_missing_text",
                format!("response does not contain {needle:?}"),
            ));
        }
    }
    for needle in &expectation.response_not_contains {
        if joined.contains(needle) {
            failures.push(failure(
                "response_forbidden_text",
                format!("response contains forbidden text {needle:?}"),
            ));
        }
    }
    for (path, expected) in &expectation.author_values {
        let actual = value_at_path(&observation.state.author, path);
        if actual != Some(expected) {
            failures.push(failure(
                "author_state_mismatch",
                format!("author state path {path} does not match expectation"),
            ));
        }
    }
    for (path, expected) in &expectation.conversation_values {
        let actual = conversation_scalar(&observation.state, path);
        if actual.as_ref() != Some(expected) {
            failures.push(failure(
                "conversation_state_mismatch",
                format!("conversation state path {path} does not match expectation"),
            ));
        }
    }
    if let Some(topic) = &expectation.active_topic {
        let actual = observation
            .state
            .conversation
            .active_topic
            .as_ref()
            .map(|row| &row.id);
        if actual != Some(topic) {
            failures.push(failure(
                "active_topic_mismatch",
                format!("expected active topic {}", topic.as_str()),
            ));
        }
    }
    if let Some(followup) = &expectation.active_followup {
        let actual = observation
            .state
            .conversation
            .active_followup
            .as_ref()
            .map(|row| &row.id);
        if actual != Some(followup) {
            failures.push(failure(
                "active_followup_mismatch",
                format!("expected active followup {}", followup.as_str()),
            ));
        }
    }
    for expected in &expectation.capabilities {
        if !observation.proposals.iter().any(|proposal| {
            proposal.capability == expected.id
                && expected
                    .version
                    .as_ref()
                    .map_or(true, |version| proposal.capability_version == *version)
                && expected
                    .arguments
                    .as_ref()
                    .map_or(true, |arguments| &proposal.arguments == arguments)
        }) {
            failures.push(failure(
                "capability_missing",
                format!("expected capability {}", expected.id.as_str()),
            ));
        }
    }
    for expected in &expectation.proposal_receipts {
        let found = observation.proposal_receipts.iter().any(|receipt| {
            if receipt.proposal.capability != expected.id {
                return false;
            }
            if expected
                .version
                .as_ref()
                .is_some_and(|version| receipt.proposal.capability_version != *version)
            {
                return false;
            }
            if expected
                .arguments
                .as_ref()
                .is_some_and(|arguments| &receipt.proposal.arguments != arguments)
            {
                return false;
            }
            let (outcome_matches, actual_reason) = match (&expected.outcome, &receipt.outcome) {
                (ExpectedProposalOutcome::Admitted, AdmissionOutcome::Admitted) => (true, None),
                (
                    ExpectedProposalOutcome::NeedsConfirmation,
                    AdmissionOutcome::NeedsConfirmation { reason_code },
                ) => (true, Some(reason_code.as_str())),
                (ExpectedProposalOutcome::Rejected, AdmissionOutcome::Rejected { reason_code }) => {
                    (true, Some(reason_code.as_str()))
                }
                _ => (false, None),
            };
            outcome_matches
                && expected
                    .reason_code
                    .as_deref()
                    .map_or(true, |reason| actual_reason == Some(reason))
        });
        if !found {
            failures.push(failure(
                "proposal_receipt_missing",
                format!(
                    "expected {:?} proposal receipt for capability {}{}",
                    expected.outcome,
                    expected.id.as_str(),
                    expected
                        .reason_code
                        .as_deref()
                        .map_or(String::new(), |reason| format!(" with reason {reason}"))
                ),
            ));
        }
    }
    for forbidden in &expectation.forbidden_capabilities {
        if observation
            .proposals
            .iter()
            .any(|proposal| proposal.capability == *forbidden)
        {
            failures.push(failure(
                "forbidden_capability",
                format!("forbidden capability proposed: {}", forbidden.as_str()),
            ));
        }
    }
    if let Some(expected) = expectation.capability_result_accepted {
        if observation.capability_result_accepted != Some(expected) {
            failures.push(failure(
                "capability_result_acceptance_mismatch",
                format!(
                    "expected capability-result accepted={expected}, got {}",
                    observation
                        .capability_result_accepted
                        .map_or("not-a-capability-result".to_owned(), |value| value
                            .to_string())
                ),
            ));
        }
    }
    if let Some(expected) = &expectation.capability_result_reason_code {
        if observation.capability_result_reason_code.as_deref() != Some(expected.as_str()) {
            failures.push(failure(
                "capability_result_reason_mismatch",
                format!(
                    "expected capability-result reason {expected}, got {}",
                    observation
                        .capability_result_reason_code
                        .as_deref()
                        .unwrap_or("none")
                ),
            ));
        }
    }
    let why_codes = why_codes(&observation.why);
    for expected in &expectation.why_codes {
        if !why_codes.iter().any(|code| *code == expected.as_str()) {
            failures.push(failure(
                "why_code_missing",
                format!("Why code missing: {}", expected.as_str()),
            ));
        }
    }
    for forbidden in &expectation.forbidden_why_codes {
        if why_codes.iter().any(|code| *code == forbidden.as_str()) {
            failures.push(failure(
                "forbidden_why_code",
                format!("forbidden Why code present: {}", forbidden.as_str()),
            ));
        }
    }
    failures
}

fn why_codes(report: &WhyReport) -> Vec<&str> {
    report
        .sections
        .iter()
        .flat_map(|section| section.entries.iter())
        .map(|entry| entry.code.as_str())
        .collect()
}

fn value_at_path<'a>(root: &'a BTreeMap<String, Value>, path: &str) -> Option<&'a Value> {
    let mut parts = path.split('.').filter(|part| !part.is_empty());
    let first = parts.next()?;
    let mut current = root.get(first)?;
    for part in parts {
        current = match current {
            Value::Object(map) => map.get(part)?,
            _ => return None,
        };
    }
    Some(current)
}

fn display_meaning(value: Option<&Meaning>) -> &str {
    value.map_or("none", |meaning| meaning.id.as_str())
}

fn failure(code: &str, detail: String) -> ExpectationFailure {
    ExpectationFailure {
        code: code.to_owned(),
        detail,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gvya_model::{ProposalId, TraceId, WhySection};
    use gvya_model::{ReferenceId, ReferenceKind};

    fn empty_context() -> ContextSnapshot {
        ContextSnapshot {
            values: BTreeMap::new(),
            visible_references: Vec::new(),
            available_capabilities: Vec::new(),
        }
    }

    fn fake_proposal() -> InvocationProposal {
        fake_proposal_with_id("p-test")
    }

    fn fake_proposal_with_id(id: &str) -> InvocationProposal {
        InvocationProposal {
            id: ProposalId::new(id),
            capability: CapabilityId::new("host.echo"),
            capability_version: CapabilityVersion::new("1"),
            arguments: BTreeMap::new(),
            fingerprint: "fingerprint".to_owned(),
            trace_id: TraceId::new("cap-test"),
        }
    }

    fn observation() -> SimulationObservation {
        SimulationObservation {
            meaning: Some(Meaning {
                id: MeaningId::new("greet"),
                slots: Vec::new(),
                references: Vec::new(),
            }),
            semantic_score: Some(0.9),
            conversation_mode: Some("intent".to_owned()),
            response_ids: vec![ResponseId::new("hello")],
            response_texts: vec!["Hello there".to_owned()],
            state: GvyaState::default(),
            proposals: Vec::new(),
            proposal_receipts: Vec::new(),
            capability_result_accepted: None,
            capability_result_reason_code: None,
            why: WhyReport {
                headline: "ok".to_owned(),
                sections: Vec::<WhySection>::new(),
                trace_ids: vec![TraceId::new("t")],
                rejected_count: 0,
            },
        }
    }

    #[test]
    fn checks_meaning_and_response_text() {
        let expectation = TurnExpectation {
            meaning: Some(MeaningId::new("greet")),
            response_contains: vec!["Hello".to_owned()],
            ..TurnExpectation::default()
        };
        assert!(check_expectation(&expectation, &observation()).is_empty());
    }

    #[derive(Default)]
    struct EchoDriver {
        inputs: Vec<SimulationInteractionInput>,
    }
    impl SimulationDriver for EchoDriver {
        type Error = String;
        fn run_interaction(
            &mut self,
            input: SimulationInteractionInput,
        ) -> Result<SimulationObservation, Self::Error> {
            self.inputs.push(input.clone());
            let mut out = observation();
            match input {
                SimulationInteractionInput::Turn(input)
                    if input.utterance == "do it" || input.utterance == "do admitted" =>
                {
                    let proposal = fake_proposal();
                    let confirmed =
                        input.utterance == "do admitted" || !input.confirmations.is_empty();
                    out.meaning = None;
                    out.response_ids.clear();
                    out.response_texts.clear();
                    out.proposal_receipts.push(SimulationProposalReceipt {
                        proposal: proposal.clone(),
                        outcome: if confirmed {
                            AdmissionOutcome::Admitted
                        } else {
                            AdmissionOutcome::NeedsConfirmation {
                                reason_code: "confirmation_required".to_owned(),
                            }
                        },
                    });
                    if confirmed {
                        out.proposals.push(proposal);
                        out.state.conversation.turn_index = 1;
                    } else {
                        // Deliberately different from the originating request state. The confirm
                        // step must replay the saved request rather than this returned state.
                        out.state.conversation.turn_index = 99;
                    }
                }
                SimulationInteractionInput::Turn(input) if input.utterance == "do rejected" => {
                    out.meaning = None;
                    out.response_ids.clear();
                    out.response_texts.clear();
                    out.proposal_receipts.push(SimulationProposalReceipt {
                        proposal: fake_proposal(),
                        outcome: AdmissionOutcome::Rejected {
                            reason_code: "policy_denied".to_owned(),
                        },
                    });
                }
                SimulationInteractionInput::Turn(input) if input.utterance == "do two admitted" => {
                    out.meaning = None;
                    out.response_ids.clear();
                    out.response_texts.clear();
                    for id in ["p-first", "p-second"] {
                        let proposal = fake_proposal_with_id(id);
                        out.proposal_receipts.push(SimulationProposalReceipt {
                            proposal: proposal.clone(),
                            outcome: AdmissionOutcome::Admitted,
                        });
                        out.proposals.push(proposal);
                    }
                }
                SimulationInteractionInput::CapabilityResult(input) => {
                    out.meaning = None;
                    out.conversation_mode = Some("capability_result".to_owned());
                    out.response_ids.clear();
                    out.response_texts.clear();
                    out.state = input.state;
                    out.capability_result_accepted = Some(true);
                }
                SimulationInteractionInput::Open(input) => {
                    out.meaning = None;
                    out.conversation_mode = Some("opening".to_owned());
                    out.state = input.state;
                }
                SimulationInteractionInput::Turn(_) => {}
            }
            Ok(out)
        }
    }

    #[test]
    fn scenario_open_step_is_first_class_and_carries_state() {
        let mut initial_state = GvyaState::default();
        initial_state.conversation.turn_index = 4;
        let scenario = ConversationScenario {
            id: ScenarioId::new("open"),
            description: String::new(),
            context: empty_context(),
            initial_state,
            steps: vec![ScenarioStep::Open(ScenarioOpenStep {
                language: Some("en-US".to_owned()),
                context: None,
                seed: Some(3),
                unix_time_ms: Some(123),
                expectation: TurnExpectation {
                    conversation_mode: Some("opening".to_owned()),
                    ..TurnExpectation::default()
                },
            })],
            generated: false,
        };
        let mut driver = EchoDriver::default();
        let report = run_test_suite(
            &mut driver,
            &TestSuite {
                regression_cases: Vec::new(),
                scenarios: vec![scenario],
            },
            TestRunLimits::default(),
        );
        assert_eq!(report.scenario_passed, 1, "{report:?}");
        assert_eq!(report.scenarios[0].steps[0].kind, "open");
        let SimulationInteractionInput::Open(open) = &driver.inputs[0] else {
            panic!("expected open input");
        };
        assert_eq!(open.state.conversation.turn_index, 4);
        assert_eq!(open.seed, Some(3));
        assert_eq!(open.unix_time_ms, Some(123));
    }

    #[test]
    fn scenario_turn_preserves_hint_reference_candidates_and_resolver_context() {
        let candidate = ResolverReferenceCandidate {
            reference: HostReference {
                kind: ReferenceKind::new("entity"),
                id: ReferenceId::new("42"),
            },
            label: Some("Answer".to_owned()),
            aliases: vec!["the answer".to_owned()],
        };
        let mut resolver_context = BTreeMap::new();
        resolver_context.insert("scope".to_owned(), Value::String("demo".to_owned()));
        let scenario = ConversationScenario {
            id: ScenarioId::new("resolver-input"),
            description: String::new(),
            context: empty_context(),
            initial_state: GvyaState::default(),
            steps: vec![ScenarioStep::Turn(ScenarioTurnStep {
                say: "use that".to_owned(),
                language: Some("en-US".to_owned()),
                context: None,
                reference_candidates: vec![candidate.clone()],
                resolver_context: resolver_context.clone(),
                hint: HintRequest::Direct(2),
                seed: Some(9),
                unix_time_ms: None,
                expectation: TurnExpectation::default(),
            })],
            generated: false,
        };
        let mut driver = EchoDriver::default();
        let report = run_test_suite(
            &mut driver,
            &TestSuite {
                regression_cases: Vec::new(),
                scenarios: vec![scenario],
            },
            TestRunLimits::default(),
        );
        assert_eq!(report.scenario_passed, 1, "{report:?}");
        let SimulationInteractionInput::Turn(turn) = &driver.inputs[0] else {
            panic!("expected turn input");
        };
        assert_eq!(turn.reference_candidates, vec![candidate]);
        assert_eq!(turn.resolver_context, resolver_context);
        assert_eq!(turn.hint, HintRequest::Direct(2));
    }

    #[test]
    fn scenario_capability_result_requires_an_admitted_proposal() {
        let scenario = ConversationScenario {
            id: ScenarioId::new("result-before-confirmation"),
            description: String::new(),
            context: empty_context(),
            initial_state: GvyaState::default(),
            steps: vec![
                ScenarioStep::Turn(ScenarioTurnStep {
                    say: "do it".to_owned(),
                    language: Some("en-US".to_owned()),
                    context: None,
                    reference_candidates: Vec::new(),
                    resolver_context: BTreeMap::new(),
                    hint: HintRequest::None,
                    seed: Some(1),
                    unix_time_ms: None,
                    expectation: TurnExpectation::default(),
                }),
                ScenarioStep::CapabilityResult(ScenarioCapabilityResultStep {
                    proposal_from_step: 1,
                    proposal_capability: Some(CapabilityId::new("host.echo")),
                    proposal_ordinal: None,
                    succeeded: true,
                    output: None,
                    error_code: None,
                    language: Some("en-US".to_owned()),
                    context: None,
                    seed: Some(2),
                    unix_time_ms: None,
                    expectation: TurnExpectation::default(),
                }),
            ],
            generated: false,
        };
        let mut driver = EchoDriver::default();
        let report = run_test_suite(
            &mut driver,
            &TestSuite {
                regression_cases: Vec::new(),
                scenarios: vec![scenario],
            },
            TestRunLimits::default(),
        );
        assert_eq!(report.scenario_passed, 0);
        assert_eq!(report.scenarios[0].steps.len(), 2);
        assert_eq!(
            report.scenarios[0].steps[1].failures[0].code,
            "scenario_input_invalid"
        );
        assert!(
            report.scenarios[0].steps[1].failures[0]
                .detail
                .contains("requires an admitted proposal")
        );
    }

    #[test]
    fn scenario_confirmation_requires_a_needs_confirmation_proposal() {
        let scenario = ConversationScenario {
            id: ScenarioId::new("confirm-admitted"),
            description: String::new(),
            context: empty_context(),
            initial_state: GvyaState::default(),
            steps: vec![
                ScenarioStep::Turn(ScenarioTurnStep {
                    say: "do admitted".to_owned(),
                    language: Some("en-US".to_owned()),
                    context: None,
                    reference_candidates: Vec::new(),
                    resolver_context: BTreeMap::new(),
                    hint: HintRequest::None,
                    seed: Some(1),
                    unix_time_ms: None,
                    expectation: TurnExpectation::default(),
                }),
                ScenarioStep::Confirm(ScenarioConfirmStep {
                    proposal_from_step: 1,
                    proposal_capability: Some(CapabilityId::new("host.echo")),
                    proposal_ordinal: None,
                    confirmed: true,
                    context: None,
                    unix_time_ms: None,
                    expectation: TurnExpectation::default(),
                }),
            ],
            generated: false,
        };
        let mut driver = EchoDriver::default();
        let report = run_test_suite(
            &mut driver,
            &TestSuite {
                regression_cases: Vec::new(),
                scenarios: vec![scenario],
            },
            TestRunLimits::default(),
        );
        assert_eq!(report.scenario_passed, 0);
        assert_eq!(
            report.scenarios[0].steps[1].failures[0].code,
            "scenario_input_invalid"
        );
        assert!(
            report.scenarios[0].steps[1].failures[0]
                .detail
                .contains("requires a needs_confirmation proposal")
        );
    }

    #[test]
    fn expectation_can_assert_rejected_proposal_receipt_and_reason() {
        let scenario = ConversationScenario {
            id: ScenarioId::new("proposal-rejected"),
            description: String::new(),
            context: empty_context(),
            initial_state: GvyaState::default(),
            steps: vec![ScenarioStep::Turn(ScenarioTurnStep {
                say: "do rejected".to_owned(),
                language: Some("en-US".to_owned()),
                context: None,
                reference_candidates: Vec::new(),
                resolver_context: BTreeMap::new(),
                hint: HintRequest::None,
                seed: Some(1),
                unix_time_ms: None,
                expectation: TurnExpectation {
                    proposal_receipts: vec![ExpectedProposalReceipt {
                        id: CapabilityId::new("host.echo"),
                        version: Some(CapabilityVersion::new("1")),
                        arguments: None,
                        outcome: ExpectedProposalOutcome::Rejected,
                        reason_code: Some("policy_denied".to_owned()),
                    }],
                    ..TurnExpectation::default()
                },
            })],
            generated: false,
        };
        let mut driver = EchoDriver::default();
        let report = run_test_suite(
            &mut driver,
            &TestSuite {
                regression_cases: Vec::new(),
                scenarios: vec![scenario],
            },
            TestRunLimits::default(),
        );
        assert_eq!(report.scenario_passed, 1, "{report:?}");
    }

    #[test]
    fn scenario_proposal_ordinal_selects_among_same_capability_receipts() {
        let scenario = ConversationScenario {
            id: ScenarioId::new("proposal-ordinal"),
            description: String::new(),
            context: empty_context(),
            initial_state: GvyaState::default(),
            steps: vec![
                ScenarioStep::Turn(ScenarioTurnStep {
                    say: "do two admitted".to_owned(),
                    language: Some("en-US".to_owned()),
                    context: None,
                    reference_candidates: Vec::new(),
                    resolver_context: BTreeMap::new(),
                    hint: HintRequest::None,
                    seed: Some(1),
                    unix_time_ms: None,
                    expectation: TurnExpectation::default(),
                }),
                ScenarioStep::CapabilityResult(ScenarioCapabilityResultStep {
                    proposal_from_step: 1,
                    proposal_capability: Some(CapabilityId::new("host.echo")),
                    proposal_ordinal: Some(2),
                    succeeded: true,
                    output: None,
                    error_code: None,
                    language: Some("en-US".to_owned()),
                    context: None,
                    seed: Some(2),
                    unix_time_ms: None,
                    expectation: TurnExpectation {
                        capability_result_accepted: Some(true),
                        ..TurnExpectation::default()
                    },
                }),
            ],
            generated: false,
        };
        let mut driver = EchoDriver::default();
        let report = run_test_suite(
            &mut driver,
            &TestSuite {
                regression_cases: Vec::new(),
                scenarios: vec![scenario],
            },
            TestRunLimits::default(),
        );
        assert_eq!(report.scenario_passed, 1, "{report:?}");
        let SimulationInteractionInput::CapabilityResult(result) = &driver.inputs[1] else {
            panic!("expected capability result input");
        };
        assert_eq!(result.result.proposal_id.as_str(), "p-second");
    }

    #[test]
    fn scenario_multiple_same_capability_receipts_require_ordinal() {
        let scenario = ConversationScenario {
            id: ScenarioId::new("proposal-ambiguous"),
            description: String::new(),
            context: empty_context(),
            initial_state: GvyaState::default(),
            steps: vec![
                ScenarioStep::Turn(ScenarioTurnStep {
                    say: "do two admitted".to_owned(),
                    language: Some("en-US".to_owned()),
                    context: None,
                    reference_candidates: Vec::new(),
                    resolver_context: BTreeMap::new(),
                    hint: HintRequest::None,
                    seed: Some(1),
                    unix_time_ms: None,
                    expectation: TurnExpectation::default(),
                }),
                ScenarioStep::CapabilityResult(ScenarioCapabilityResultStep {
                    proposal_from_step: 1,
                    proposal_capability: Some(CapabilityId::new("host.echo")),
                    proposal_ordinal: None,
                    succeeded: true,
                    output: None,
                    error_code: None,
                    language: Some("en-US".to_owned()),
                    context: None,
                    seed: Some(2),
                    unix_time_ms: None,
                    expectation: TurnExpectation::default(),
                }),
            ],
            generated: false,
        };
        let mut driver = EchoDriver::default();
        let report = run_test_suite(
            &mut driver,
            &TestSuite {
                regression_cases: Vec::new(),
                scenarios: vec![scenario],
            },
            TestRunLimits::default(),
        );
        assert_eq!(report.scenario_passed, 0);
        assert_eq!(
            report.scenarios[0].steps[1].failures[0].code,
            "scenario_input_invalid"
        );
        assert!(
            report.scenarios[0].steps[1].failures[0]
                .detail
                .contains("proposal_ordinal")
        );
    }

    #[test]
    fn scenario_carries_driver_state_surface_without_hidden_session_storage() {
        let scenario = ConversationScenario {
            id: ScenarioId::new("hello"),
            description: String::new(),
            context: empty_context(),
            initial_state: GvyaState::default(),
            steps: vec![ScenarioStep::Turn(ScenarioTurnStep {
                say: "hi".to_owned(),
                language: None,
                context: None,
                reference_candidates: Vec::new(),
                resolver_context: BTreeMap::new(),
                hint: HintRequest::None,
                seed: Some(1),
                unix_time_ms: None,
                expectation: TurnExpectation {
                    meaning: Some(MeaningId::new("greet")),
                    ..TurnExpectation::default()
                },
            })],
            generated: false,
        };
        let suite = TestSuite {
            regression_cases: Vec::new(),
            scenarios: vec![scenario],
        };
        let mut driver = EchoDriver::default();
        let report = run_test_suite(&mut driver, &suite, TestRunLimits::default());
        assert_eq!(report.scenario_passed, 1);
        assert_eq!(report.scenarios[0].steps[0].kind, "turn");
    }

    #[test]
    fn scenario_confirmation_replays_origin_and_capability_result_consumes_real_proposal() {
        let scenario = ConversationScenario {
            id: ScenarioId::new("capability-roundtrip"),
            description: String::new(),
            context: empty_context(),
            initial_state: GvyaState::default(),
            steps: vec![
                ScenarioStep::Turn(ScenarioTurnStep {
                    say: "do it".to_owned(),
                    language: Some("en-US".to_owned()),
                    context: None,
                    reference_candidates: Vec::new(),
                    resolver_context: BTreeMap::new(),
                    hint: HintRequest::None,
                    seed: Some(7),
                    unix_time_ms: None,
                    expectation: TurnExpectation::default(),
                }),
                ScenarioStep::Confirm(ScenarioConfirmStep {
                    proposal_from_step: 1,
                    proposal_capability: Some(CapabilityId::new("host.echo")),
                    proposal_ordinal: None,
                    confirmed: true,
                    context: None,
                    unix_time_ms: None,
                    expectation: TurnExpectation {
                        capabilities: vec![ExpectedCapability {
                            id: CapabilityId::new("host.echo"),
                            version: Some(CapabilityVersion::new("1")),
                            arguments: None,
                        }],
                        ..TurnExpectation::default()
                    },
                }),
                ScenarioStep::CapabilityResult(ScenarioCapabilityResultStep {
                    proposal_from_step: 2,
                    proposal_capability: Some(CapabilityId::new("host.echo")),
                    proposal_ordinal: None,
                    succeeded: true,
                    output: Some(Value::String("done".to_owned())),
                    error_code: None,
                    language: Some("en-US".to_owned()),
                    context: None,
                    seed: Some(8),
                    unix_time_ms: None,
                    expectation: TurnExpectation {
                        capability_result_accepted: Some(true),
                        conversation_mode: Some("capability_result".to_owned()),
                        ..TurnExpectation::default()
                    },
                }),
            ],
            generated: false,
        };
        let suite = TestSuite {
            regression_cases: Vec::new(),
            scenarios: vec![scenario],
        };
        let mut driver = EchoDriver::default();
        let report = run_test_suite(&mut driver, &suite, TestRunLimits::default());
        assert_eq!(report.scenario_passed, 1, "{report:?}");
        assert_eq!(driver.inputs.len(), 3);
        let SimulationInteractionInput::Turn(confirm_retry) = &driver.inputs[1] else {
            panic!("confirm must replay the originating turn");
        };
        assert_eq!(confirm_retry.utterance, "do it");
        assert_eq!(confirm_retry.seed, Some(7));
        assert_eq!(confirm_retry.state.conversation.turn_index, 0);
        assert_eq!(confirm_retry.confirmations.len(), 1);
        let SimulationInteractionInput::CapabilityResult(result_input) = &driver.inputs[2] else {
            panic!("third step must be capability-result");
        };
        assert_eq!(result_input.proposal.id.as_str(), "p-test");
        assert_eq!(result_input.state.conversation.turn_index, 1);
    }

    fn regression(id: &str) -> RegressionCase {
        RegressionCase {
            id: TestCaseId::new(id),
            description: String::new(),
            input: "hi".into(),
            language: None,
            context: empty_context(),
            initial_state: GvyaState::default(),
            seed: None,
            unix_time_ms: None,
            expectation: TurnExpectation {
                meaning: Some(MeaningId::new("greet")),
                ..TurnExpectation::default()
            },
            generated: false,
        }
    }

    #[test]
    fn suite_limit_plus_one_is_visible_and_never_passes_as_complete() {
        let suite = TestSuite {
            regression_cases: vec![regression("a"), regression("b")],
            scenarios: Vec::new(),
        };
        let report = run_test_suite(
            &mut EchoDriver::default(),
            &suite,
            TestRunLimits {
                max_regression_cases: 1,
                ..TestRunLimits::default()
            },
        );
        assert_eq!(report.regression_total, 2);
        assert_eq!(report.regression_executed, 1);
        assert_eq!(report.regression_passed, 1);
        assert!(report.stopped_early);
    }

    #[test]
    fn suite_exact_limit_is_complete_when_all_tests_pass() {
        let suite = TestSuite {
            regression_cases: vec![regression("a")],
            scenarios: Vec::new(),
        };
        let report = run_test_suite(
            &mut EchoDriver::default(),
            &suite,
            TestRunLimits {
                max_regression_cases: 1,
                ..TestRunLimits::default()
            },
        );
        assert_eq!(report.regression_total, 1);
        assert_eq!(report.regression_executed, 1);
        assert!(!report.stopped_early);
    }
}
