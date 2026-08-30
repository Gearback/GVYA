//! Canonical GVYA runtime.
//!
//! A runtime loads one strict compiled `.gvya` artifact and executes the same Rust semantic,
//! conversation and capability kernels for native, WASM, C-ABI and SDK adapters. Hosts execute
//! capability proposals; this crate never performs device/game/application side effects itself.

#![forbid(unsafe_code)]

pub mod engine;
pub mod loader;
pub mod program;
pub mod resolver_bridge;
pub mod wire;

pub use engine::{
    Runtime, RuntimeCapabilityResultOutput, RuntimeCapabilityResultRequest,
    RuntimeInteractionOutput, RuntimeLimits, RuntimeOpenRequest, RuntimeRequestError,
    RuntimeResolverError, RuntimeTurnRequest, RuntimeUtteranceInput, is_conversational_output,
};
pub use loader::{
    LoadError, LoadPolicy, RuntimeAsset, SignatureEnvelope, SignatureVerifier, TrustStatus,
    load_artifact,
};
pub use program::{
    HydratedProgram, ProgramError, ProgramLimits, RuntimeAssetDefinition, hydrate_program,
    hydrate_program_with_limits,
};
pub use resolver_bridge::{JsonSemanticResolver, SemanticResolverLimits};

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use gvya_kernel::CapabilityResultInput;
    use gvya_model::{
        AdmissionOutcome, AvailableCapability, CapabilityId, CapabilityVersion, ContextSnapshot,
        GvyaState, ProposalId, ResponseItem, Value,
    };

    #[test]
    fn runtime_loads_strict_minimal_fixture_without_rebuilding_semantics() {
        let bytes = include_bytes!("../../../validation/fixtures/runtime-minimal.gvya").to_vec();
        let runtime = Runtime::load(bytes, LoadPolicy::default(), None).unwrap();
        assert_eq!(runtime.project_id(), "runtime-minimal");
        assert_eq!(runtime.enabled_languages(), &["en"]);
        assert_eq!(runtime.default_language(), "en");
        let output = runtime
            .turn(RuntimeTurnRequest {
                utterance: RuntimeUtteranceInput {
                    text: "hello".into(),
                },
                context: ContextSnapshot {
                    values: BTreeMap::new(),
                    visible_references: vec![],
                    available_capabilities: vec![],
                },
                state: GvyaState::default(),
                reference_candidates: vec![],
                resolver_context: BTreeMap::new(),
                system: BTreeMap::new(),
                hint: gvya_kernel::conversation::HintRequest::None,
                seed: Some(1),
                confirmations: vec![],
            })
            .unwrap();
        assert_eq!(
            output.conversation.mode,
            gvya_kernel::conversation::ConversationMode::Silent
        );
        assert!(output.conversation.response.messages.is_empty());
        assert_eq!(
            output
                .conversation
                .state
                .conversation
                .active_language
                .as_deref(),
            Some("en")
        );
        assert!(output.capabilities.decisions.is_empty());
    }

    #[test]
    fn runtime_action_fixture_executes_one_canonical_turn_and_capability_round_trip() {
        let bytes = include_bytes!("../../../validation/fixtures/runtime-action.gvya").to_vec();
        let runtime = Runtime::load(bytes, LoadPolicy::default(), None).unwrap();
        let output = runtime
            .turn(RuntimeTurnRequest {
                utterance: RuntimeUtteranceInput {
                    text: "hello".into(),
                },
                context: ContextSnapshot {
                    values: BTreeMap::new(),
                    visible_references: vec![],
                    available_capabilities: vec![AvailableCapability {
                        id: CapabilityId::new("demo.wave"),
                        version: CapabilityVersion::new("1"),
                    }],
                },
                state: GvyaState::default(),
                reference_candidates: vec![],
                resolver_context: BTreeMap::new(),
                system: BTreeMap::new(),
                hint: gvya_kernel::conversation::HintRequest::None,
                seed: Some(7),
                confirmations: vec![],
            })
            .unwrap();
        assert_eq!(
            output.conversation.mode,
            gvya_kernel::conversation::ConversationMode::Answer
        );
        assert_eq!(
            output.conversation.meaning.as_ref().unwrap().id.as_str(),
            "hello"
        );
        assert!(output.conversation.response.messages.iter().flat_map(|message| &message.items).any(|item| matches!(item, ResponseItem::Text { text, .. } if text == "Hello from GVYA.")));
        assert!(output.conversation.response.messages.iter().flat_map(|message| &message.items).any(|item| matches!(item, ResponseItem::Asset { asset_id, .. } if asset_id.as_str() == "tone")));
        let decision = &output.capabilities.decisions[0];
        assert!(matches!(&decision.outcome, AdmissionOutcome::Admitted));
        let proposal = decision.proposal.as_ref().unwrap();
        assert_eq!(proposal.capability.as_str(), "demo.wave");
        assert_eq!(
            output.conversation.state.conversation.pending_capabilities,
            vec![proposal.clone()]
        );
        let result = runtime
            .capability_result(RuntimeCapabilityResultRequest {
                proposal: proposal.clone(),
                result: CapabilityResultInput {
                    proposal_id: ProposalId::new(proposal.id.as_str()),
                    succeeded: true,
                    output: None,
                    error_code: None,
                },
                context: ContextSnapshot {
                    values: BTreeMap::new(),
                    visible_references: vec![],
                    available_capabilities: vec![AvailableCapability {
                        id: CapabilityId::new("demo.wave"),
                        version: CapabilityVersion::new("1"),
                    }],
                },
                state: output.conversation.state.clone(),
                system: BTreeMap::new(),
                seed: Some(8),
                confirmations: vec![],
            })
            .unwrap();
        assert!(result.validation.accepted);
        let continuation = result
            .interaction
            .as_ref()
            .expect("accepted result must continue");
        assert!(continuation.conversation.response.messages.iter().flat_map(|message| &message.items).any(|item| matches!(item, ResponseItem::Text { text, .. } if text == "Wave completed.")), "Bot default language must remain the response fallback after an explicit unsupported response locale");
        assert_eq!(
            continuation
                .conversation
                .state
                .conversation
                .active_language
                .as_deref(),
            Some("en")
        );
        assert!(
            continuation
                .conversation
                .state
                .conversation
                .pending_capabilities
                .is_empty()
        );

        let replay = runtime
            .capability_result(RuntimeCapabilityResultRequest {
                proposal: proposal.clone(),
                result: CapabilityResultInput {
                    proposal_id: proposal.id.clone(),
                    succeeded: true,
                    output: None,
                    error_code: None,
                },
                context: ContextSnapshot {
                    values: BTreeMap::new(),
                    visible_references: vec![],
                    available_capabilities: vec![AvailableCapability {
                        id: CapabilityId::new("demo.wave"),
                        version: CapabilityVersion::new("1"),
                    }],
                },
                state: continuation.conversation.state.clone(),
                system: BTreeMap::new(),
                seed: Some(9),
                confirmations: vec![],
            })
            .unwrap();
        assert!(!replay.validation.accepted);
        assert_eq!(
            replay.validation.reason_code.as_deref(),
            Some("proposal_not_pending")
        );
        assert!(replay.interaction.is_none());

        let asset = runtime.asset(&gvya_model::AssetId::new("tone")).unwrap();
        assert_eq!(asset.bytes, b"GVYA runtime fixture asset\n");
    }

    #[test]
    fn capability_result_requires_an_exact_pending_runtime_receipt() {
        let bytes = include_bytes!("../../../validation/fixtures/runtime-action.gvya").to_vec();
        let runtime = Runtime::load(bytes, LoadPolicy::default(), None).unwrap();
        let turn = runtime
            .turn(RuntimeTurnRequest {
                utterance: RuntimeUtteranceInput {
                    text: "hello".into(),
                },
                context: ContextSnapshot {
                    values: BTreeMap::new(),
                    visible_references: vec![],
                    available_capabilities: vec![AvailableCapability {
                        id: CapabilityId::new("demo.wave"),
                        version: CapabilityVersion::new("1"),
                    }],
                },
                state: GvyaState::default(),
                reference_candidates: vec![],
                resolver_context: BTreeMap::new(),
                system: BTreeMap::new(),
                hint: gvya_kernel::conversation::HintRequest::None,
                seed: Some(7),
                confirmations: vec![],
            })
            .unwrap();
        let proposal = turn.capabilities.decisions[0]
            .proposal
            .as_ref()
            .unwrap()
            .clone();
        let fabricated = runtime
            .capability_result(RuntimeCapabilityResultRequest {
                proposal: proposal.clone(),
                result: CapabilityResultInput {
                    proposal_id: proposal.id.clone(),
                    succeeded: true,
                    output: None,
                    error_code: None,
                },
                context: ContextSnapshot {
                    values: BTreeMap::new(),
                    visible_references: vec![],
                    available_capabilities: vec![],
                },
                state: GvyaState::default(),
                system: BTreeMap::new(),
                seed: None,
                confirmations: vec![],
            })
            .unwrap();
        assert!(!fabricated.validation.accepted);
        assert_eq!(
            fabricated.validation.reason_code.as_deref(),
            Some("proposal_not_pending")
        );
        assert!(fabricated.interaction.is_none());

        let mut mismatched_state = turn.conversation.state;
        mismatched_state.conversation.pending_capabilities[0]
            .fingerprint
            .push('0');
        let mismatch = runtime
            .capability_result(RuntimeCapabilityResultRequest {
                proposal: proposal.clone(),
                result: CapabilityResultInput {
                    proposal_id: proposal.id.clone(),
                    succeeded: true,
                    output: None,
                    error_code: None,
                },
                context: ContextSnapshot {
                    values: BTreeMap::new(),
                    visible_references: vec![],
                    available_capabilities: vec![],
                },
                state: mismatched_state,
                system: BTreeMap::new(),
                seed: None,
                confirmations: vec![],
            })
            .unwrap();
        assert!(!mismatch.validation.accepted);
        assert_eq!(
            mismatch.validation.reason_code.as_deref(),
            Some("proposal_receipt_mismatch")
        );
    }

    fn resolver_turn_request() -> RuntimeTurnRequest {
        RuntimeTurnRequest {
            utterance: RuntimeUtteranceInput {
                text: "unfamiliar greeting words".into(),
            },
            context: ContextSnapshot {
                values: BTreeMap::new(),
                visible_references: vec![],
                available_capabilities: vec![AvailableCapability {
                    id: CapabilityId::new("demo.wave"),
                    version: CapabilityVersion::new("1"),
                }],
            },
            state: GvyaState::default(),
            reference_candidates: vec![],
            resolver_context: BTreeMap::new(),
            system: BTreeMap::new(),
            hint: gvya_kernel::conversation::HintRequest::None,
            seed: Some(7),
            confirmations: vec![],
        }
    }

    #[test]
    fn optional_external_resolver_is_reviewed_before_authored_capability_admission() {
        let bytes = include_bytes!("../../../validation/fixtures/runtime-action.gvya").to_vec();
        let runtime = Runtime::load(bytes, LoadPolicy::default(), None).unwrap();
        let resolver = JsonSemanticResolver::new(|request: &str| -> Result<String, String> {
            assert!(request.contains("gvya.semantic.resolver.request"));
            assert!(request.contains(r#""task":"resolve_meaning""#));
            // The request never advertises the host Capability catalog.
            assert!(!request.to_lowercase().contains("capabilit"), "{request}");
            assert!(!request.contains("demo.wave"), "{request}");
            Ok(r#"{"meaning":"hello","confidence":0.95,"evidence":["fixture"]}"#.to_string())
        });
        let output = runtime
            .turn_with_resolver(resolver_turn_request(), &resolver)
            .unwrap();
        assert_eq!(
            output.conversation.meaning.as_ref().unwrap().id.as_str(),
            "hello"
        );
        let review = output
            .conversation
            .semantic
            .as_ref()
            .unwrap()
            .trace
            .events
            .iter()
            .find(|event| event.code.as_str() == "semantic.resolver.review")
            .unwrap();
        assert_eq!(review.details.get("accepted"), Some(&Value::Bool(true)));
        assert!(!review.details.contains_key("capability_ignored"));
        // Capability identity comes only from the authored binding downstream of validation.
        let proposal = output.capabilities.decisions[0].proposal.as_ref().unwrap();
        assert_eq!(proposal.capability.as_str(), "demo.wave");
    }

    /// Capability identity is absent from the resolver contract. If a provider returns a stale or
    /// malicious `capability` field, strict JSON decoding fails and the optional resolver degrades
    /// to the exact deterministic turn rather than breaking runtime availability.
    #[test]
    fn resolver_capability_injection_is_rejected_and_degrades_to_deterministic_behavior() {
        let bytes = include_bytes!("../../../validation/fixtures/runtime-action.gvya").to_vec();
        let runtime = Runtime::load(bytes, LoadPolicy::default(), None).unwrap();
        let deterministic = runtime.turn(resolver_turn_request()).unwrap();
        let resolver = JsonSemanticResolver::new(|_request: &str| -> Result<String, String> {
            Ok(r#"{"meaning":"hello","capability":"host.secret","confidence":0.95}"#.to_string())
        });
        let output = runtime
            .turn_with_resolver(resolver_turn_request(), &resolver)
            .unwrap();
        assert_eq!(
            output.conversation.meaning,
            deterministic.conversation.meaning
        );
        assert_eq!(output.conversation.mode, deterministic.conversation.mode);
        assert!(
            output
                .conversation
                .trace
                .events
                .iter()
                .any(|event| { event.code.as_str() == "semantic.resolver.unavailable" })
        );
    }

    /// With no resolver configured the deterministic runtime is unchanged, and a resolver that
    /// fails, times out, returns malformed output, or answers with nothing degrades to exactly that
    /// deterministic outcome.
    #[test]
    fn resolver_absence_failure_malformed_and_empty_output_all_degrade_to_deterministic_behavior() {
        let bytes = include_bytes!("../../../validation/fixtures/runtime-action.gvya").to_vec();
        let runtime = Runtime::load(bytes, LoadPolicy::default(), None).unwrap();
        let deterministic = runtime.turn(resolver_turn_request()).unwrap();

        let empty = JsonSemanticResolver::new(|_request: &str| -> Result<String, String> {
            Ok("{}".to_string())
        });
        let empty = runtime
            .turn_with_resolver(resolver_turn_request(), &empty)
            .unwrap();
        assert_eq!(
            empty.conversation.meaning,
            deterministic.conversation.meaning
        );
        assert_eq!(empty.conversation.mode, deterministic.conversation.mode);

        let outside = JsonSemanticResolver::new(|_request: &str| -> Result<String, String> {
            Ok(r#"{"meaning":"climate.set_temperature","confidence":1.0}"#.to_string())
        });
        let outside = runtime
            .turn_with_resolver(resolver_turn_request(), &outside)
            .unwrap();
        assert_eq!(
            outside.conversation.meaning,
            deterministic.conversation.meaning
        );

        let malformed = JsonSemanticResolver::new(|_request: &str| -> Result<String, String> {
            Ok("{not json".to_string())
        });
        let malformed = runtime
            .turn_with_resolver(resolver_turn_request(), &malformed)
            .unwrap();
        assert_eq!(
            malformed.conversation.meaning,
            deterministic.conversation.meaning
        );
        assert_eq!(malformed.conversation.mode, deterministic.conversation.mode);
        assert!(
            malformed
                .conversation
                .trace
                .events
                .iter()
                .any(|event| { event.code.as_str() == "semantic.resolver.unavailable" })
        );

        let broken = JsonSemanticResolver::new(|_request: &str| -> Result<String, String> {
            Err("resolver timed out".into())
        });
        let broken = runtime
            .turn_with_resolver(resolver_turn_request(), &broken)
            .unwrap();
        assert_eq!(
            broken.conversation.meaning,
            deterministic.conversation.meaning
        );
        assert_eq!(broken.conversation.mode, deterministic.conversation.mode);
        assert!(
            broken
                .conversation
                .trace
                .events
                .iter()
                .any(|event| { event.code.as_str() == "semantic.resolver.unavailable" })
        );
    }

    #[test]
    fn wire_serialization_rejects_non_finite_typed_host_state_instead_of_coercing_to_null() {
        let bytes = include_bytes!("../../../validation/fixtures/runtime-minimal.gvya").to_vec();
        let runtime = Runtime::load(bytes, LoadPolicy::default(), None).unwrap();
        let mut state = GvyaState::default();
        state.author.insert("bad".into(), Value::Number(f64::NAN));
        let output = runtime
            .turn(RuntimeTurnRequest {
                utterance: RuntimeUtteranceInput {
                    text: "hello".into(),
                },
                context: ContextSnapshot {
                    values: BTreeMap::new(),
                    visible_references: vec![],
                    available_capabilities: vec![],
                },
                state,
                reference_candidates: vec![],
                resolver_context: BTreeMap::new(),
                system: BTreeMap::new(),
                hint: gvya_kernel::conversation::HintRequest::None,
                seed: Some(1),
                confirmations: vec![],
            })
            .unwrap_err();
        assert!(matches!(
            output,
            RuntimeRequestError::Invalid("non_finite_number")
        ));
    }

    #[test]
    fn runtime_rejects_structurally_valid_artifact_whose_semantics_cannot_build_an_index() {
        let bytes =
            include_bytes!("../../../validation/fixtures/runtime-bad-semantics.gvya").to_vec();
        assert!(Runtime::load(bytes, LoadPolicy::default(), None).is_err());
    }

    struct FixtureVerifier;
    impl SignatureVerifier for FixtureVerifier {
        fn verify(
            &self,
            _content_root: [u8; 32],
            envelope: &SignatureEnvelope,
        ) -> Result<(), String> {
            if envelope.algorithm == "fixture-v1"
                && envelope.key_id == "fixture-key"
                && envelope.signature == "fixture-signature"
            {
                Ok(())
            } else {
                Err("unexpected fixture signature envelope".into())
            }
        }
    }

    #[test]
    fn signature_trust_is_host_owned_and_cannot_replace_structural_validation() {
        let bytes = include_bytes!("../../../validation/fixtures/runtime-signed.gvya").to_vec();
        let unverified = Runtime::load(bytes.clone(), LoadPolicy::default(), None).unwrap();
        assert!(matches!(
            unverified.trust(),
            TrustStatus::PresentUnverified { .. }
        ));

        let policy = LoadPolicy {
            require_signature: true,
            ..LoadPolicy::default()
        };
        assert!(Runtime::load(bytes.clone(), policy, None).is_err());
        let verified = Runtime::load(bytes, policy, Some(&FixtureVerifier)).unwrap();
        assert!(matches!(verified.trust(), TrustStatus::Verified { .. }));

        let malformed =
            include_bytes!("../../../validation/fixtures/runtime-bad-semantics.gvya").to_vec();
        assert!(Runtime::load(malformed, LoadPolicy::default(), Some(&FixtureVerifier)).is_err());
    }

    #[test]
    fn container_only_golden_is_not_mistaken_for_runtime_program() {
        let bytes = include_bytes!("../../../validation/fixtures/golden.gvya").to_vec();
        assert!(Runtime::load(bytes, LoadPolicy::default(), None).is_err());
    }

    #[test]
    fn direct_rust_runtime_api_enforces_the_same_string_budget() {
        let bytes = include_bytes!("../../../validation/fixtures/runtime-minimal.gvya").to_vec();
        let limits = RuntimeLimits {
            max_string_bytes: 4,
            ..RuntimeLimits::default()
        };
        let runtime =
            Runtime::load_with_runtime_limits(bytes, LoadPolicy::default(), None, limits).unwrap();
        let result = runtime.turn(RuntimeTurnRequest {
            utterance: RuntimeUtteranceInput {
                text: "hello".into(),
            },
            context: ContextSnapshot {
                values: BTreeMap::new(),
                visible_references: vec![],
                available_capabilities: vec![],
            },
            state: GvyaState::default(),
            reference_candidates: vec![],
            resolver_context: BTreeMap::new(),
            system: BTreeMap::new(),
            hint: gvya_kernel::conversation::HintRequest::None,
            seed: None,
            confirmations: vec![],
        });
        assert!(matches!(
            result,
            Err(RuntimeRequestError::Limit("string_bytes"))
        ));
    }

    #[test]
    fn direct_rust_runtime_api_enforces_aggregate_request_text_budget() {
        let bytes = include_bytes!("../../../validation/fixtures/runtime-minimal.gvya").to_vec();
        let limits = RuntimeLimits {
            max_request_bytes: 8,
            ..RuntimeLimits::default()
        };
        let runtime =
            Runtime::load_with_runtime_limits(bytes, LoadPolicy::default(), None, limits).unwrap();
        let mut values = BTreeMap::new();
        values.insert("ab".into(), Value::String("cd".into()));
        let result = runtime.turn(RuntimeTurnRequest {
            utterance: RuntimeUtteranceInput {
                text: "hello".into(),
            },
            context: ContextSnapshot {
                values,
                visible_references: vec![],
                available_capabilities: vec![],
            },
            state: GvyaState::default(),
            reference_candidates: vec![],
            resolver_context: BTreeMap::new(),
            system: BTreeMap::new(),
            hint: gvya_kernel::conversation::HintRequest::None,
            seed: None,
            confirmations: vec![],
        });
        assert!(matches!(
            result,
            Err(RuntimeRequestError::Limit("request_bytes"))
        ));
    }

    #[test]
    fn direct_rust_runtime_api_enforces_response_byte_budget_before_return() {
        let bytes = include_bytes!("../../../validation/fixtures/runtime-minimal.gvya").to_vec();
        let limits = RuntimeLimits {
            max_response_bytes: 64,
            ..RuntimeLimits::default()
        };
        let runtime =
            Runtime::load_with_runtime_limits(bytes, LoadPolicy::default(), None, limits).unwrap();
        let result = runtime.turn(RuntimeTurnRequest {
            utterance: RuntimeUtteranceInput {
                text: "hello".into(),
            },
            context: ContextSnapshot {
                values: BTreeMap::new(),
                visible_references: vec![],
                available_capabilities: vec![],
            },
            state: GvyaState::default(),
            reference_candidates: vec![],
            resolver_context: BTreeMap::new(),
            system: BTreeMap::new(),
            hint: gvya_kernel::conversation::HintRequest::None,
            seed: None,
            confirmations: vec![],
        });
        assert!(matches!(
            result,
            Err(RuntimeRequestError::Limit("response_bytes"))
        ));
    }

    #[test]
    fn direct_capability_result_api_enforces_response_byte_budget_before_return() {
        let bytes = include_bytes!("../../../validation/fixtures/runtime-action.gvya").to_vec();
        let normal = Runtime::load(bytes.clone(), LoadPolicy::default(), None).unwrap();
        let turn = normal
            .turn(RuntimeTurnRequest {
                utterance: RuntimeUtteranceInput {
                    text: "hello".into(),
                },
                context: ContextSnapshot {
                    values: BTreeMap::new(),
                    visible_references: vec![],
                    available_capabilities: vec![AvailableCapability {
                        id: CapabilityId::new("demo.wave"),
                        version: CapabilityVersion::new("1"),
                    }],
                },
                state: GvyaState::default(),
                reference_candidates: vec![],
                resolver_context: BTreeMap::new(),
                system: BTreeMap::new(),
                hint: gvya_kernel::conversation::HintRequest::None,
                seed: Some(7),
                confirmations: vec![],
            })
            .unwrap();
        let proposal = turn.capabilities.decisions[0]
            .proposal
            .as_ref()
            .unwrap()
            .clone();

        let limited = Runtime::load_with_runtime_limits(
            bytes,
            LoadPolicy::default(),
            None,
            RuntimeLimits {
                max_response_bytes: 64,
                ..RuntimeLimits::default()
            },
        )
        .unwrap();
        let result = limited.capability_result(RuntimeCapabilityResultRequest {
            proposal: proposal.clone(),
            result: CapabilityResultInput {
                proposal_id: proposal.id.clone(),
                succeeded: true,
                output: None,
                error_code: None,
            },
            context: ContextSnapshot {
                values: BTreeMap::new(),
                visible_references: vec![],
                available_capabilities: vec![],
            },
            state: turn.conversation.state,
            system: BTreeMap::new(),
            seed: Some(8),
            confirmations: vec![],
        });
        assert!(matches!(
            result,
            Err(RuntimeRequestError::Limit("response_bytes"))
        ));
    }

    #[test]
    fn direct_rust_runtime_limits_cannot_relax_canonical_ceiling() {
        let bytes = include_bytes!("../../../validation/fixtures/runtime-minimal.gvya").to_vec();
        let limits = RuntimeLimits {
            max_string_bytes: RuntimeLimits::default().max_string_bytes + 1,
            ..RuntimeLimits::default()
        };
        let error = Runtime::load_with_runtime_limits(bytes, LoadPolicy::default(), None, limits)
            .unwrap_err();
        assert!(matches!(error, LoadError::RuntimeLimits(_)));
    }

    #[test]
    fn program_limits_can_tighten_but_cannot_relax_canonical_ceiling() {
        let bytes = include_bytes!("../../../validation/fixtures/runtime-minimal.gvya").to_vec();
        let relaxed = ProgramLimits {
            max_nodes: ProgramLimits::default().max_nodes + 1,
            ..ProgramLimits::default()
        };
        let error = Runtime::load(
            bytes.clone(),
            LoadPolicy {
                program_limits: relaxed,
                ..LoadPolicy::default()
            },
            None,
        )
        .unwrap_err();
        assert!(matches!(error, LoadError::Program(ProgramError::Limit(_))));

        let tightened = ProgramLimits {
            max_program_bytes: 1,
            ..ProgramLimits::default()
        };
        let error = Runtime::load(
            bytes,
            LoadPolicy {
                program_limits: tightened,
                ..LoadPolicy::default()
            },
            None,
        )
        .unwrap_err();
        assert!(matches!(error, LoadError::Program(ProgramError::Limit(_))));
    }
}
