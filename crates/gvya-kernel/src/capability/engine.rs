//! Capability binding, validation, admission, confirmation and host-result validation.

use std::collections::BTreeMap;

use gvya_model::{
    AdmissionOutcome, ConfirmationGrant, ContextSnapshot, InvocationProposal, ProposalId,
    ResponseId, Trace, TraceCode, TraceEvent, TraceId, TraceVisibility, Value,
};

use crate::{CapabilityResultInput, conversation::ConversationOutcome};

use super::{
    binding::{BindingContext, CapabilityBindingRule, bind_arguments, trigger_matches},
    catalog::{CapabilityCatalog, CapabilityDefinition},
    policy::{PolicyContext, PolicyDecision, evaluate_policy},
    schema::validate_value,
};

#[derive(Clone, Debug)]
pub struct CapabilityEvaluationRequest<'a> {
    pub conversation: &'a ConversationOutcome,
    pub context: &'a ContextSnapshot,
    /// Explicit system facts. Capability admission never reads ambient environment/time.
    pub system: &'a BTreeMap<String, Value>,
    pub confirmations: &'a [ConfirmationGrant],
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InvocationSource {
    pub binding_id: gvya_model::CapabilityBindingId,
    pub response_id: Option<ResponseId>,
    pub message_index: Option<usize>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CapabilityDecision {
    pub source: InvocationSource,
    pub capability: gvya_model::CapabilityId,
    pub outcome: AdmissionOutcome,
    pub proposal: Option<InvocationProposal>,
    pub reason_details: Vec<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CapabilityEvaluation {
    pub decisions: Vec<CapabilityDecision>,
    pub trace: Trace,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CapabilityResultValidation {
    pub accepted: bool,
    pub reason_code: Option<String>,
    pub trace: Trace,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CapabilityKernel {
    catalog: CapabilityCatalog,
}

impl CapabilityKernel {
    #[must_use]
    pub fn new(catalog: CapabilityCatalog) -> Self {
        Self { catalog }
    }

    #[must_use]
    pub fn catalog(&self) -> &CapabilityCatalog {
        &self.catalog
    }

    #[must_use]
    pub fn evaluate(&self, request: CapabilityEvaluationRequest<'_>) -> CapabilityEvaluation {
        let trace_id = TraceId::new(format!(
            "cap-{:016x}",
            stable_hash(&format!(
                "{}|{}",
                request.conversation.trace.id.as_str(),
                request.conversation.state.conversation.turn_index
            ))
        ));
        let mut trace = Trace {
            id: trace_id.clone(),
            events: Vec::new(),
        };
        let mut decisions = Vec::new();
        let occurrences = self.matching_occurrences(request.conversation);

        trace.events.push(event(
            "capability.trigger_scan",
            "capability.trigger",
            format!(
                "{} capability binding occurrence(s) matched",
                occurrences.len()
            ),
            BTreeMap::from([("matches".into(), Value::Number(occurrences.len() as f64))]),
        ));

        for (index, occurrence) in occurrences.into_iter().enumerate() {
            if decisions.len() >= self.catalog.config().max_proposals_per_turn {
                decisions.push(CapabilityDecision {
                    source: occurrence.source(),
                    capability: occurrence.rule.capability.clone(),
                    outcome: AdmissionOutcome::Rejected {
                        reason_code: "proposal_limit_exceeded".into(),
                    },
                    proposal: None,
                    reason_details: vec![
                        "turn produced more capability candidates than configured limit".into(),
                    ],
                });
                trace.events.push(event(
                    "capability.proposal_limit",
                    "capability.admission",
                    "additional capability candidate rejected by turn proposal limit",
                    BTreeMap::new(),
                ));
                break;
            }
            decisions
                .push(self.evaluate_occurrence(occurrence, index, &request, &trace_id, &mut trace));
        }

        CapabilityEvaluation { decisions, trace }
    }

    #[must_use]
    pub fn validate_result(
        &self,
        proposal: &InvocationProposal,
        result: &CapabilityResultInput,
    ) -> CapabilityResultValidation {
        let trace_id = TraceId::new(format!(
            "cap-result-{:016x}",
            stable_hash(&format!(
                "{}|{}",
                proposal.id.as_str(),
                proposal.fingerprint
            ))
        ));
        let mut trace = Trace {
            id: trace_id,
            events: Vec::new(),
        };

        if result.proposal_id != proposal.id {
            return result_reject(
                trace,
                "proposal_id_mismatch",
                "capability result does not belong to this proposal",
            );
        }
        let Some(definition) = self.catalog.definition(&proposal.capability) else {
            return result_reject(
                trace,
                "capability_undeclared",
                "proposal capability is not in the compiled catalog",
            );
        };
        if definition.contract.version != proposal.capability_version {
            return result_reject(
                trace,
                "capability_version_mismatch",
                "proposal contract version differs from compiled catalog",
            );
        }
        let argument_issues = validate_value(
            &Value::Object(proposal.arguments.clone()),
            &definition.input_shape,
            self.catalog.config().schema_limits,
        );
        if !argument_issues.is_empty() {
            return result_reject(
                trace,
                "proposal_arguments_invalid",
                "proposal arguments no longer satisfy the compiled input contract",
            );
        }
        if proposal_fingerprint(definition, &proposal.arguments) != proposal.fingerprint {
            return result_reject(
                trace,
                "proposal_fingerprint_mismatch",
                "proposal fingerprint does not match its capability version and arguments",
            );
        }

        if result.succeeded {
            if result
                .error_code
                .as_ref()
                .is_some_and(|code| !code.trim().is_empty())
            {
                return result_reject(
                    trace,
                    "success_with_error_code",
                    "successful capability result cannot carry an error code",
                );
            }
            match (&definition.output_shape, &result.output) {
                (Some(schema), Some(output)) => {
                    let issues =
                        validate_value(output, schema, self.catalog.config().schema_limits);
                    if !issues.is_empty() {
                        trace.events.push(event(
                            "capability.result_schema_rejected",
                            "capability.result",
                            "host capability output failed schema validation",
                            BTreeMap::from([(
                                "issue_count".into(),
                                Value::Number(issues.len() as f64),
                            )]),
                        ));
                        return CapabilityResultValidation {
                            accepted: false,
                            reason_code: Some("output_schema_invalid".into()),
                            trace,
                        };
                    }
                }
                (Some(_), None) => {
                    return result_reject(
                        trace,
                        "output_required",
                        "successful capability result omitted declared output",
                    );
                }
                (None, Some(_)) => {
                    return result_reject(
                        trace,
                        "unexpected_output",
                        "capability contract does not declare an output",
                    );
                }
                (None, None) => {}
            }
        } else {
            if result
                .error_code
                .as_ref()
                .map_or(true, |code| code.trim().is_empty())
            {
                return result_reject(
                    trace,
                    "failure_error_code_required",
                    "failed capability result requires a typed error code",
                );
            }
            if let Some(output) = &result.output {
                let Some(schema) = &definition.output_shape else {
                    return result_reject(
                        trace,
                        "unexpected_failure_output",
                        "failed result returned output without an output contract",
                    );
                };
                if !validate_value(output, schema, self.catalog.config().schema_limits).is_empty() {
                    return result_reject(
                        trace,
                        "failure_output_schema_invalid",
                        "failed result output does not satisfy declared schema",
                    );
                }
            }
        }

        trace.events.push(event(
            "capability.result_accepted",
            "capability.result",
            "host capability result accepted as structurally valid input",
            BTreeMap::from([("succeeded".into(), Value::Bool(result.succeeded))]),
        ));
        CapabilityResultValidation {
            accepted: true,
            reason_code: None,
            trace,
        }
    }

    fn matching_occurrences<'a>(
        &'a self,
        conversation: &'a ConversationOutcome,
    ) -> Vec<BindingOccurrence<'a>> {
        let mut out = Vec::new();
        for rule in self.catalog.bindings() {
            if rule.trigger.response.is_some() {
                for (message_index, message) in conversation.response.messages.iter().enumerate() {
                    let response = message.source_response.as_ref();
                    if trigger_matches(
                        &rule.trigger,
                        conversation.meaning.as_ref(),
                        conversation.behavior.as_ref(),
                        response,
                    ) {
                        out.push(BindingOccurrence {
                            rule,
                            response_id: response.cloned(),
                            message_index: Some(message_index),
                        });
                    }
                }
            } else if trigger_matches(
                &rule.trigger,
                conversation.meaning.as_ref(),
                conversation.behavior.as_ref(),
                None,
            ) {
                out.push(BindingOccurrence {
                    rule,
                    response_id: None,
                    message_index: None,
                });
            }
        }
        out
    }

    fn evaluate_occurrence(
        &self,
        occurrence: BindingOccurrence<'_>,
        ordinal: usize,
        request: &CapabilityEvaluationRequest<'_>,
        trace_id: &TraceId,
        trace: &mut Trace,
    ) -> CapabilityDecision {
        let source = occurrence.source();
        let capability = occurrence.rule.capability.clone();
        let Some(definition) = self.catalog.definition(&capability) else {
            return reject(
                source,
                capability,
                "capability_undeclared",
                "binding references undeclared capability",
            );
        };

        let binding = bind_arguments(
            occurrence.rule,
            &BindingContext {
                meaning: request.conversation.meaning.as_ref(),
                context: request.context,
                state: &request.conversation.state,
            },
        );
        if !binding.issues.is_empty() {
            let details = binding
                .issues
                .iter()
                .map(|issue| format!("{}: {}", issue.code, issue.message))
                .collect();
            trace.events.push(event(
                "capability.binding_rejected",
                "capability.binding",
                format!(
                    "binding {} could not establish unambiguous arguments",
                    occurrence.rule.id.as_str()
                ),
                BTreeMap::from([(
                    "issue_count".into(),
                    Value::Number(binding.issues.len() as f64),
                )]),
            ));
            return CapabilityDecision {
                source,
                capability,
                outcome: AdmissionOutcome::Rejected {
                    reason_code: "argument_binding_failed".into(),
                },
                proposal: None,
                reason_details: details,
            };
        }

        let argument_value = Value::Object(binding.arguments.clone());
        let schema_issues = validate_value(
            &argument_value,
            &definition.input_shape,
            self.catalog.config().schema_limits,
        );
        if !schema_issues.is_empty() {
            let details = schema_issues
                .iter()
                .map(|issue| format!("{} {}: {}", issue.path, issue.code, issue.message))
                .collect();
            trace.events.push(event(
                "capability.schema_rejected",
                "capability.validation",
                format!(
                    "arguments for {} failed contract validation",
                    capability.as_str()
                ),
                BTreeMap::from([(
                    "issue_count".into(),
                    Value::Number(schema_issues.len() as f64),
                )]),
            ));
            return CapabilityDecision {
                source,
                capability,
                outcome: AdmissionOutcome::Rejected {
                    reason_code: "input_schema_invalid".into(),
                },
                proposal: None,
                reason_details: details,
            };
        }

        if !capability_available(request.context, definition) {
            let same_id = request
                .context
                .available_capabilities
                .iter()
                .any(|available| available.id == definition.contract.id);
            let code = if same_id {
                "capability_version_unavailable"
            } else {
                "capability_unavailable"
            };
            trace.events.push(event(
                "capability.unavailable",
                "capability.availability",
                format!(
                    "host does not expose {}@{}",
                    capability.as_str(),
                    definition.contract.version.as_str()
                ),
                BTreeMap::new(),
            ));
            return reject(
                source,
                capability,
                code,
                "host availability snapshot does not expose the exact capability contract version",
            );
        }

        let fingerprint = proposal_fingerprint(definition, &binding.arguments);
        let proposal_id = ProposalId::new(format!(
            "p-{:016x}",
            stable_hash(&format!(
                "{}|{}|{}|{}|{}",
                trace_id.as_str(),
                occurrence.rule.id.as_str(),
                ordinal,
                definition.contract.id.as_str(),
                fingerprint
            ))
        ));
        let proposal = InvocationProposal {
            id: proposal_id,
            capability: capability.clone(),
            capability_version: definition.contract.version.clone(),
            arguments: binding.arguments,
            fingerprint,
            trace_id: trace_id.clone(),
        };

        trace.events.push(event(
            "capability.bound",
            "capability.binding",
            format!(
                "bound {} using {}",
                capability.as_str(),
                occurrence.rule.id.as_str()
            ),
            BTreeMap::from([
                (
                    "capability".into(),
                    Value::String(capability.as_str().to_owned()),
                ),
                (
                    "version".into(),
                    Value::String(definition.contract.version.as_str().to_owned()),
                ),
                (
                    "proposal".into(),
                    Value::String(proposal.id.as_str().to_owned()),
                ),
            ]),
        ));

        let policy = evaluate_policy(
            &capability,
            definition.contract.confirmation_hint,
            self.catalog.policies(),
            &PolicyContext {
                arguments: &proposal.arguments,
                context: request.context,
                state: &request.conversation.state,
                system: request.system,
            },
        );

        let (outcome, details) =
            self.resolve_policy_and_confirmation(&proposal, policy, request.confirmations, trace);
        CapabilityDecision {
            source,
            capability,
            outcome,
            proposal: Some(proposal),
            reason_details: details,
        }
    }

    fn resolve_policy_and_confirmation(
        &self,
        proposal: &InvocationProposal,
        policy: PolicyDecision,
        grants: &[ConfirmationGrant],
        trace: &mut Trace,
    ) -> (AdmissionOutcome, Vec<String>) {
        match policy {
            PolicyDecision::Allow { policy } => {
                trace.events.push(event(
                    "capability.admitted",
                    "capability.admission",
                    "capability proposal admitted",
                    policy_detail(policy),
                ));
                (AdmissionOutcome::Admitted, Vec::new())
            }
            PolicyDecision::Reject {
                reason_code,
                policy,
            } => {
                trace.events.push(event(
                    "capability.policy_rejected",
                    "capability.admission",
                    format!("capability proposal rejected: {reason_code}"),
                    policy_detail(policy),
                ));
                (
                    AdmissionOutcome::Rejected {
                        reason_code: reason_code.clone(),
                    },
                    vec![reason_code],
                )
            }
            PolicyDecision::NeedsConfirmation {
                reason_code,
                policy,
            } => {
                let matching: Vec<&ConfirmationGrant> = grants
                    .iter()
                    .filter(|grant| grant.proposal_id == proposal.id)
                    .collect();
                if matching.len() > 1 {
                    trace.events.push(event(
                        "capability.confirmation_ambiguous",
                        "capability.confirmation",
                        "multiple confirmation grants target the same proposal",
                        BTreeMap::new(),
                    ));
                    return (
                        AdmissionOutcome::Rejected {
                            reason_code: "confirmation_ambiguous".into(),
                        },
                        vec!["multiple confirmation grants target the proposal".into()],
                    );
                }
                let Some(grant) = matching.first() else {
                    trace.events.push(event(
                        "capability.confirmation_required",
                        "capability.confirmation",
                        format!("confirmation required: {reason_code}"),
                        policy_detail(policy),
                    ));
                    return (
                        AdmissionOutcome::NeedsConfirmation {
                            reason_code: reason_code.clone(),
                        },
                        vec![reason_code],
                    );
                };
                if grant.fingerprint != proposal.fingerprint {
                    trace.events.push(event(
                        "capability.confirmation_stale",
                        "capability.confirmation",
                        "confirmation grant fingerprint does not match current proposal",
                        BTreeMap::new(),
                    ));
                    return (
                        AdmissionOutcome::Rejected {
                            reason_code: "confirmation_stale".into(),
                        },
                        vec!["confirmed proposal changed after confirmation was issued".into()],
                    );
                }
                if !grant.confirmed {
                    trace.events.push(event(
                        "capability.confirmation_declined",
                        "capability.confirmation",
                        "user declined the exact capability proposal",
                        BTreeMap::new(),
                    ));
                    return (
                        AdmissionOutcome::Rejected {
                            reason_code: "confirmation_declined".into(),
                        },
                        vec!["user declined confirmation".into()],
                    );
                }
                trace.events.push(event(
                    "capability.confirmation_accepted",
                    "capability.confirmation",
                    "exact proposal confirmation accepted",
                    BTreeMap::from([(
                        "confirmation_id".into(),
                        Value::String(grant.id.as_str().to_owned()),
                    )]),
                ));
                (AdmissionOutcome::Admitted, Vec::new())
            }
        }
    }
}

struct BindingOccurrence<'a> {
    rule: &'a CapabilityBindingRule,
    response_id: Option<ResponseId>,
    message_index: Option<usize>,
}

impl BindingOccurrence<'_> {
    fn source(&self) -> InvocationSource {
        InvocationSource {
            binding_id: self.rule.id.clone(),
            response_id: self.response_id.clone(),
            message_index: self.message_index,
        }
    }
}

fn capability_available(context: &ContextSnapshot, definition: &CapabilityDefinition) -> bool {
    context.available_capabilities.iter().any(|available| {
        available.id == definition.contract.id && available.version == definition.contract.version
    })
}

fn reject(
    source: InvocationSource,
    capability: gvya_model::CapabilityId,
    code: &str,
    message: &str,
) -> CapabilityDecision {
    CapabilityDecision {
        source,
        capability,
        outcome: AdmissionOutcome::Rejected {
            reason_code: code.into(),
        },
        proposal: None,
        reason_details: vec![message.into()],
    }
}

fn result_reject(mut trace: Trace, code: &str, message: &str) -> CapabilityResultValidation {
    trace.events.push(event(
        "capability.result_rejected",
        "capability.result",
        message,
        BTreeMap::from([("reason".into(), Value::String(code.into()))]),
    ));
    CapabilityResultValidation {
        accepted: false,
        reason_code: Some(code.into()),
        trace,
    }
}

fn policy_detail(policy: Option<gvya_model::PolicyId>) -> BTreeMap<String, Value> {
    policy.map_or_else(BTreeMap::new, |id| {
        BTreeMap::from([("policy".into(), Value::String(id.as_str().to_owned()))])
    })
}

fn event(
    code: &str,
    phase: &str,
    summary: impl Into<String>,
    details: BTreeMap<String, Value>,
) -> TraceEvent {
    TraceEvent {
        code: TraceCode::new(code),
        phase: phase.into(),
        summary: summary.into(),
        visibility: TraceVisibility::Author,
        details,
    }
}

fn proposal_fingerprint(
    definition: &CapabilityDefinition,
    arguments: &BTreeMap<String, Value>,
) -> String {
    let canonical = canonical_value(&Value::Object(arguments.clone()));
    format!(
        "{:016x}",
        stable_hash(&format!(
            "{}@{}|{}",
            definition.contract.id.as_str(),
            definition.contract.version.as_str(),
            canonical
        ))
    )
}

fn canonical_value(value: &Value) -> String {
    match value {
        Value::Null => "n".into(),
        Value::Bool(value) => {
            if *value {
                "b1".into()
            } else {
                "b0".into()
            }
        }
        Value::Number(value) => format!("d{:016x}", value.to_bits()),
        Value::String(value) => format!("s{}:{value}", value.len()),
        Value::Array(values) => {
            let body = values
                .iter()
                .map(canonical_value)
                .collect::<Vec<_>>()
                .join(",");
            format!("a{}[{body}]", values.len())
        }
        Value::Object(values) => {
            let body = values
                .iter()
                .map(|(key, value)| format!("{}:{}={}", key.len(), key, canonical_value(value)))
                .collect::<Vec<_>>()
                .join(",");
            format!("o{}{{{body}}}", values.len())
        }
    }
}

fn stable_hash(text: &str) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for byte in text.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};

    use gvya_model::{
        AvailableCapability, BehaviorId, CapabilityBindingId, CapabilityContract, CapabilityId,
        CapabilityVersion, ConfirmationGrant, ConfirmationHint, ConfirmationId, ContextSnapshot,
        EffectClass, Meaning, MeaningId, ResponseId, ResponseMessage, ResponsePlan, SchemaDocument,
        SlotValue, Trace, ValueProvenance,
    };

    use crate::conversation::{ConversationMode, ConversationOutcome};

    use super::super::{
        binding::{
            ArgumentBinding, ArgumentPath, BindingSource, CapabilityBindingRule, CapabilityTrigger,
        },
        catalog::{CapabilityConfig, CapabilityDefinition},
        schema::ValueSchema,
    };
    use super::*;

    fn capability() -> CapabilityDefinition {
        CapabilityDefinition {
            contract: CapabilityContract {
                id: CapabilityId::new("thermostat.set"),
                version: CapabilityVersion::new("1"),
                title: "Set thermostat".into(),
                description: String::new(),
                input_schema: SchemaDocument::new("{\"type\":\"object\"}"),
                output_schema: None,
                reference_kinds: Vec::new(),
                effect_class: EffectClass::Reversible,
                confirmation_hint: ConfirmationHint::Never,
            },
            input_shape: ValueSchema::object(
                BTreeMap::from([(
                    "temperature".into(),
                    ValueSchema::Integer {
                        minimum: Some(16),
                        maximum: Some(30),
                    },
                )]),
                BTreeSet::from(["temperature".into()]),
            ),
            output_shape: None,
            host_effects: Vec::new(),
        }
    }

    fn outcome() -> ConversationOutcome {
        ConversationOutcome {
            mode: ConversationMode::Answer,
            meaning: Some(Meaning {
                id: MeaningId::new("temperature.set"),
                slots: vec![SlotValue {
                    name: "temperature".into(),
                    value: Value::Number(22.0),
                    provenance: ValueProvenance::Utterance,
                }],
                references: Vec::new(),
            }),
            behavior: Some(BehaviorId::new("temperature")),
            response: ResponsePlan {
                messages: vec![ResponseMessage {
                    source_response: Some(ResponseId::new("done")),
                    kind: "normal".into(),
                    items: Vec::new(),
                }],
            },
            state: gvya_model::GvyaState::default(),
            semantic: None,
            trace: Trace {
                id: TraceId::new("turn"),
                events: Vec::new(),
            },
        }
    }

    fn kernel(hint: ConfirmationHint) -> CapabilityKernel {
        let mut definition = capability();
        definition.contract.confirmation_hint = hint;
        let binding = CapabilityBindingRule {
            id: CapabilityBindingId::new("temperature.set"),
            trigger: CapabilityTrigger {
                meaning: Some(MeaningId::new("temperature.set")),
                behavior: None,
                response: None,
            },
            capability: CapabilityId::new("thermostat.set"),
            arguments: vec![ArgumentBinding {
                target: ArgumentPath::from_dotted("temperature").unwrap(),
                source: BindingSource::MeaningSlot("temperature".into()),
            }],
        };
        CapabilityKernel::new(
            CapabilityCatalog::new(
                vec![definition],
                vec![binding],
                Vec::new(),
                CapabilityConfig::default(),
            )
            .unwrap(),
        )
    }

    fn context() -> ContextSnapshot {
        ContextSnapshot {
            values: BTreeMap::new(),
            visible_references: Vec::new(),
            available_capabilities: vec![AvailableCapability {
                id: CapabilityId::new("thermostat.set"),
                version: CapabilityVersion::new("1"),
            }],
        }
    }

    #[test]
    fn admitted_proposal_is_typed_and_versioned() {
        let outcome = outcome();
        let context = context();
        let system = BTreeMap::new();
        let evaluation = kernel(ConfirmationHint::Never).evaluate(CapabilityEvaluationRequest {
            conversation: &outcome,
            context: &context,
            system: &system,
            confirmations: &[],
        });
        assert_eq!(evaluation.decisions.len(), 1);
        assert_eq!(evaluation.decisions[0].outcome, AdmissionOutcome::Admitted);
        let proposal = evaluation.decisions[0].proposal.as_ref().unwrap();
        assert_eq!(proposal.capability_version.as_str(), "1");
        assert_eq!(
            proposal.arguments.get("temperature"),
            Some(&Value::Number(22.0))
        );
    }

    #[test]
    fn wrong_host_version_is_rejected() {
        let outcome = outcome();
        let mut context = context();
        context.available_capabilities[0].version = CapabilityVersion::new("2");
        let system = BTreeMap::new();
        let evaluation = kernel(ConfirmationHint::Never).evaluate(CapabilityEvaluationRequest {
            conversation: &outcome,
            context: &context,
            system: &system,
            confirmations: &[],
        });
        assert!(
            matches!(&evaluation.decisions[0].outcome, AdmissionOutcome::Rejected { reason_code } if reason_code == "capability_version_unavailable")
        );
    }

    #[test]
    fn required_slot_missing_is_rejected_by_capability_schema() {
        let mut conversation = outcome();
        conversation.meaning.as_mut().unwrap().slots.clear();
        let context = context();
        let system = BTreeMap::new();
        let evaluation = kernel(ConfirmationHint::Never).evaluate(CapabilityEvaluationRequest {
            conversation: &conversation,
            context: &context,
            system: &system,
            confirmations: &[],
        });
        assert!(matches!(
            &evaluation.decisions[0].outcome,
            AdmissionOutcome::Rejected { reason_code } if reason_code == "input_schema_invalid"
        ));
        assert!(evaluation.decisions[0].proposal.is_none());
    }

    #[test]
    fn confirmation_round_trip_preserves_proposal_identity() {
        let conversation = outcome();
        let context = context();
        let system = BTreeMap::new();
        let kernel = kernel(ConfirmationHint::Always);
        let first = kernel.evaluate(CapabilityEvaluationRequest {
            conversation: &conversation,
            context: &context,
            system: &system,
            confirmations: &[],
        });
        assert!(matches!(
            first.decisions[0].outcome,
            AdmissionOutcome::NeedsConfirmation { .. }
        ));
        let first_proposal = first.decisions[0].proposal.as_ref().unwrap().clone();
        let grant = ConfirmationGrant {
            id: ConfirmationId::new("confirm-1"),
            proposal_id: first_proposal.id.clone(),
            fingerprint: first_proposal.fingerprint.clone(),
            confirmed: true,
        };
        let second = kernel.evaluate(CapabilityEvaluationRequest {
            conversation: &conversation,
            context: &context,
            system: &system,
            confirmations: &[grant],
        });
        assert_eq!(second.decisions[0].outcome, AdmissionOutcome::Admitted);
        assert_eq!(
            second.decisions[0].proposal.as_ref().unwrap().id,
            first_proposal.id
        );
        assert_eq!(
            second.decisions[0].proposal.as_ref().unwrap().fingerprint,
            first_proposal.fingerprint
        );
    }

    #[test]
    fn stale_confirmation_fingerprint_is_rejected() {
        let conversation = outcome();
        let context = context();
        let system = BTreeMap::new();
        let kernel = kernel(ConfirmationHint::Always);
        let first = kernel.evaluate(CapabilityEvaluationRequest {
            conversation: &conversation,
            context: &context,
            system: &system,
            confirmations: &[],
        });
        let proposal = first.decisions[0].proposal.as_ref().unwrap();
        let grant = ConfirmationGrant {
            id: ConfirmationId::new("confirm-stale"),
            proposal_id: proposal.id.clone(),
            fingerprint: "stale".into(),
            confirmed: true,
        };
        let second = kernel.evaluate(CapabilityEvaluationRequest {
            conversation: &conversation,
            context: &context,
            system: &system,
            confirmations: &[grant],
        });
        assert!(matches!(
            &second.decisions[0].outcome,
            AdmissionOutcome::Rejected { reason_code } if reason_code == "confirmation_stale"
        ));
    }

    #[test]
    fn response_specific_binding_only_fires_for_selected_response() {
        let definition = capability();
        let binding = CapabilityBindingRule {
            id: CapabilityBindingId::new("temperature.response.done"),
            trigger: CapabilityTrigger {
                meaning: None,
                behavior: None,
                response: Some(ResponseId::new("done")),
            },
            capability: CapabilityId::new("thermostat.set"),
            arguments: vec![ArgumentBinding {
                target: ArgumentPath::from_dotted("temperature").unwrap(),
                source: BindingSource::MeaningSlot("temperature".into()),
            }],
        };
        let kernel = CapabilityKernel::new(
            CapabilityCatalog::new(
                vec![definition],
                vec![binding],
                Vec::new(),
                CapabilityConfig::default(),
            )
            .unwrap(),
        );
        let mut conversation = outcome();
        conversation.response.messages[0].source_response = Some(ResponseId::new("other"));
        let context = context();
        let system = BTreeMap::new();
        let none = kernel.evaluate(CapabilityEvaluationRequest {
            conversation: &conversation,
            context: &context,
            system: &system,
            confirmations: &[],
        });
        assert!(none.decisions.is_empty());
        conversation.response.messages[0].source_response = Some(ResponseId::new("done"));
        let one = kernel.evaluate(CapabilityEvaluationRequest {
            conversation: &conversation,
            context: &context,
            system: &system,
            confirmations: &[],
        });
        assert_eq!(one.decisions.len(), 1);
        assert_eq!(one.decisions[0].outcome, AdmissionOutcome::Admitted);
    }

    #[test]
    fn successful_result_cannot_return_undeclared_output() {
        let conversation = outcome();
        let context = context();
        let system = BTreeMap::new();
        let kernel = kernel(ConfirmationHint::Never);
        let evaluation = kernel.evaluate(CapabilityEvaluationRequest {
            conversation: &conversation,
            context: &context,
            system: &system,
            confirmations: &[],
        });
        let proposal = evaluation.decisions[0].proposal.as_ref().unwrap();
        let validation = kernel.validate_result(
            proposal,
            &CapabilityResultInput {
                proposal_id: proposal.id.clone(),
                succeeded: true,
                output: Some(Value::Null),
                error_code: None,
            },
        );
        assert!(!validation.accepted);
        assert_eq!(validation.reason_code.as_deref(), Some("unexpected_output"));
    }
}
