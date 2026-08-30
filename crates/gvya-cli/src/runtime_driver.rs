//! Canonical runtime adapter used by compiler test execution.

use super::*;

pub(super) struct CanonicalRuntimeDriver {
    pub(super) runtime: Runtime,
}

impl SimulationDriver for CanonicalRuntimeDriver {
    type Error = String;

    fn run_interaction(
        &mut self,
        input: SimulationInteractionInput,
    ) -> Result<SimulationObservation, Self::Error> {
        match input {
            SimulationInteractionInput::Open(input) => {
                let output = self
                    .runtime
                    .open(RuntimeOpenRequest {
                        context: input.context,
                        state: input.state,
                        system: system_values(input.unix_time_ms),
                        seed: input.seed,
                        confirmations: input.confirmations,
                    })
                    .map_err(|error| format!("runtime open rejected: {error:?}"))?;
                Ok(interaction_observation(output))
            }
            SimulationInteractionInput::Turn(input) => {
                let output = self
                    .runtime
                    .turn(RuntimeTurnRequest {
                        utterance: RuntimeUtteranceInput {
                            text: input.utterance,
                        },
                        context: input.context,
                        state: input.state,
                        reference_candidates: input.reference_candidates,
                        resolver_context: input.resolver_context,
                        system: system_values(input.unix_time_ms),
                        hint: input.hint,
                        seed: input.seed,
                        confirmations: input.confirmations,
                    })
                    .map_err(|error| format!("runtime turn rejected: {error:?}"))?;
                Ok(interaction_observation(output))
            }
            SimulationInteractionInput::CapabilityResult(input) => {
                let unchanged_state = input.state.clone();
                let output = self
                    .runtime
                    .capability_result(RuntimeCapabilityResultRequest {
                        proposal: input.proposal,
                        result: input.result,
                        context: input.context,
                        state: input.state,
                        system: system_values(input.unix_time_ms),
                        seed: input.seed,
                        confirmations: input.confirmations,
                    })
                    .map_err(|error| format!("runtime capability-result rejected: {error:?}"))?;
                let accepted = output.validation.accepted;
                let reason_code = output.validation.reason_code.clone();
                let why = output.why.clone();
                if let Some(interaction) = output.interaction {
                    let mut observation = interaction_observation(interaction);
                    observation.capability_result_accepted = Some(accepted);
                    observation.capability_result_reason_code = reason_code;
                    observation.why = why;
                    Ok(observation)
                } else {
                    Ok(SimulationObservation {
                        meaning: None,
                        semantic_score: None,
                        conversation_mode: None,
                        response_ids: Vec::new(),
                        response_texts: Vec::new(),
                        state: unchanged_state,
                        proposals: Vec::new(),
                        proposal_receipts: Vec::new(),
                        capability_result_accepted: Some(accepted),
                        capability_result_reason_code: reason_code,
                        why,
                    })
                }
            }
        }
    }
}

fn system_values(unix_time_ms: Option<i64>) -> BTreeMap<String, Value> {
    let mut system = BTreeMap::new();
    if let Some(unix_time_ms) = unix_time_ms {
        system.insert("unix_time_ms".into(), Value::Number(unix_time_ms as f64));
    }
    system
}

fn interaction_observation(
    output: gvya_runtime::RuntimeInteractionOutput,
) -> SimulationObservation {
    let semantic_score = output
        .conversation
        .semantic
        .as_ref()
        .and_then(|analysis| analysis.scored.first().map(|row| row.score));
    let response_ids = output
        .conversation
        .response
        .messages
        .iter()
        .filter_map(|message| message.source_response.clone())
        .collect::<Vec<_>>();
    let response_texts = output
        .conversation
        .response
        .messages
        .iter()
        .flat_map(|message| message.items.iter())
        .filter_map(|item| match item {
            ResponseItem::Text { text, .. } => Some(text.clone()),
            _ => None,
        })
        .collect::<Vec<_>>();
    let proposal_receipts = output
        .capabilities
        .decisions
        .iter()
        .filter_map(|decision| {
            decision
                .proposal
                .clone()
                .map(|proposal| SimulationProposalReceipt {
                    proposal,
                    outcome: decision.outcome.clone(),
                })
        })
        .collect::<Vec<_>>();
    let proposals = proposal_receipts
        .iter()
        .filter(|receipt| matches!(receipt.outcome, AdmissionOutcome::Admitted))
        .map(|receipt| receipt.proposal.clone())
        .collect::<Vec<_>>();
    SimulationObservation {
        meaning: output.conversation.meaning,
        semantic_score,
        conversation_mode: Some(output.conversation.mode.label().to_owned()),
        response_ids,
        response_texts,
        state: output.conversation.state,
        proposals,
        proposal_receipts,
        capability_result_accepted: None,
        capability_result_reason_code: None,
        why: output.why,
    }
}
