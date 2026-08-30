//! Contract coverage for the provider-neutral JSON resolver bridge.
//!
//! Every malformed, oversized or out-of-boundary external output must fail closed with an error
//! and never panic. Encoding must be canonical, bounded and deterministic.

use super::*;
use gvya_kernel::{
    ResolverEntitySchema, ResolverReferenceCandidate, ResolverSemanticHint, UtteranceInput,
};

fn utterance(text: &str) -> UtteranceInput {
    UtteranceInput {
        text: text.into(),
        language: Some("en".into()),
    }
}

fn hint(text: &str) -> ResolverSemanticHint {
    ResolverSemanticHint {
        kind: ResolverHintKind::Sample,
        language: "en".into(),
        text: text.into(),
    }
}

fn candidate(meaning: &str) -> ResolverMeaningCandidate {
    ResolverMeaningCandidate {
        meaning: MeaningId::new(meaning),
        origin: ResolverCandidateOrigin::DeterministicMatch,
        evidence: Some(ResolverCandidateEvidence {
            semantic: ResolverEvidenceStrength::Strong,
            retrieval: ResolverEvidenceStrength::Moderate,
            matched_terms: vec!["book".into()],
        }),
        hints: vec![hint("book a hotel")],
        slots: vec![ResolverSlotSchema {
            name: "item".into(),
            required: true,
            kind: ResolverValueKind::Entity(ResolverEntitySchema {
                kind: "game.item".into(),
                canonical_values: vec!["health_potion".into(), "sword".into()],
                values_are_exhaustive: true,
            }),
        }],
        references: vec![ResolverReferenceSchema {
            kind: ReferenceKind::new("hotel"),
            required: false,
        }],
    }
}

fn request() -> ResolverRequest {
    ResolverRequest {
        task: ResolverTask::ResolveMeaning,
        utterance: utterance("book it"),
        language_fallbacks: vec!["en".into()],
        candidates: vec![candidate("booking.create")],
        collection: None,
        reference_candidates: vec![ResolverReferenceCandidate {
            reference: HostReference {
                kind: ReferenceKind::new("hotel"),
                id: ReferenceId::new("h-1"),
            },
            label: Some("Hotel One".into()),
            aliases: vec!["the hotel".into()],
        }],
        exposed_context: BTreeMap::new(),
    }
}

fn collection_request() -> ResolverRequest {
    let mut request = request();
    request.task = ResolverTask::FillCollection;
    request.candidates[0].origin = ResolverCandidateOrigin::ActiveCollection;
    request.candidates[0].evidence = None;
    request.collection = Some(ResolverCollectionContext {
        meaning: MeaningId::new("booking.create"),
        bound_slots: vec![SlotValue {
            name: "item".into(),
            value: Value::String("health_potion".into()),
            provenance: ValueProvenance::Utterance,
        }],
        bound_references: vec![HostReference {
            kind: ReferenceKind::new("hotel"),
            id: ReferenceId::new("h-1"),
        }],
        collectable: vec![ResolverCollectionTarget::Slot(ResolverSlotSchema {
            name: "count".into(),
            required: true,
            kind: ResolverValueKind::Number,
        })],
    });
    request
}

fn encoded(request: &ResolverRequest) -> String {
    encode_request(request, SemanticResolverLimits::default()).expect("encodes")
}

#[test]
fn request_encoding_is_canonical_and_deterministic() {
    let request = request();
    let first = encoded(&request);
    let second = encoded(&request.clone());
    assert_eq!(first, second);
    assert!(first.contains(r#""format":"gvya.semantic.resolver.request""#));
    assert!(first.contains(r#""version":1"#));
    assert!(first.contains(r#""task":"resolve_meaning""#));
    assert!(first.contains(r#""origin":"deterministic_match""#));
    assert!(first.contains(r#""semantic":"strong""#));
    assert!(first.contains(r#""type":"entity""#));
    assert!(first.contains(r#""values_are_exhaustive":true"#));
    assert!(first.contains(r#""deterministic_review_required":true"#));
}

#[test]
fn collection_request_projects_task_bound_values_and_targets() {
    let encoded = encoded(&collection_request());
    assert!(encoded.contains(r#""task":"fill_collection""#));
    assert!(encoded.contains(r#""origin":"active_collection""#));
    assert!(encoded.contains(r#""bound_slots":[{"name":"item","value":"health_potion"}]"#));
    assert!(encoded.contains(r#""target":"slot","name":"count""#));
    // Internal deterministic bookkeeping never crosses the boundary.
    assert!(!encoded.contains("provenance"));
}

#[test]
fn task_and_collection_context_must_agree_structurally() {
    let mut orphan = collection_request();
    orphan.collection = None;
    assert!(encode_request(&orphan, SemanticResolverLimits::default()).is_err());

    let mut stray = request();
    stray.collection = collection_request().collection;
    assert!(encode_request(&stray, SemanticResolverLimits::default()).is_err());
}

#[test]
fn no_capability_surface_is_serialized_anywhere() {
    for request in [request(), collection_request()] {
        assert!(!encoded(&request).to_lowercase().contains("capabilit"));
    }
}

#[test]
fn candidate_count_boundary_is_exact() {
    let limits = SemanticResolverLimits::default();
    let mut at_limit = request();
    at_limit.candidates = (0..RESOLVER_REQUEST_CANDIDATES_MAX)
        .map(|index| candidate(&format!("meaning.{index}")))
        .collect();
    assert!(encode_request(&at_limit, limits).is_ok());

    let mut over_limit = at_limit.clone();
    over_limit.candidates.push(candidate("meaning.overflow"));
    assert!(encode_request(&over_limit, limits).is_err());
}

#[test]
fn per_candidate_projection_boundaries_are_exact() {
    let limits = SemanticResolverLimits::default();

    let mut hints = request();
    hints.candidates[0].hints = (0..RESOLVER_HINTS_PER_CANDIDATE_MAX)
        .map(|index| hint(&format!("sample {index}")))
        .collect();
    assert!(encode_request(&hints, limits).is_ok());
    hints.candidates[0].hints.push(hint("one too many"));
    assert!(encode_request(&hints, limits).is_err());

    let mut long_hint = request();
    long_hint.candidates[0].hints = vec![hint(&"x".repeat(RESOLVER_HINT_MAX_BYTES))];
    assert!(encode_request(&long_hint, limits).is_ok());
    long_hint.candidates[0].hints = vec![hint(&"x".repeat(RESOLVER_HINT_MAX_BYTES + 1))];
    assert!(encode_request(&long_hint, limits).is_err());

    let slot = |index: usize| ResolverSlotSchema {
        name: format!("slot{index}"),
        required: false,
        kind: ResolverValueKind::String,
    };
    let mut slots = request();
    slots.candidates[0].slots = (0..RESOLVER_SLOTS_PER_CANDIDATE_MAX).map(slot).collect();
    assert!(encode_request(&slots, limits).is_ok());
    slots.candidates[0].slots.push(slot(999));
    assert!(encode_request(&slots, limits).is_err());

    let reference = |index: usize| ResolverReferenceSchema {
        kind: ReferenceKind::new(format!("kind{index}")),
        required: false,
    };
    let mut references = request();
    references.candidates[0].references = (0..RESOLVER_REFERENCES_PER_CANDIDATE_MAX)
        .map(reference)
        .collect();
    assert!(encode_request(&references, limits).is_ok());
    references.candidates[0].references.push(reference(999));
    assert!(encode_request(&references, limits).is_err());

    let mut terms = request();
    let evidence = |count: usize| {
        Some(ResolverCandidateEvidence {
            semantic: ResolverEvidenceStrength::Weak,
            retrieval: ResolverEvidenceStrength::Weak,
            matched_terms: (0..count).map(|index| format!("t{index}")).collect(),
        })
    };
    terms.candidates[0].evidence = evidence(RESOLVER_MATCHED_TERMS_MAX);
    assert!(encode_request(&terms, limits).is_ok());
    terms.candidates[0].evidence = evidence(RESOLVER_MATCHED_TERMS_MAX + 1);
    assert!(encode_request(&terms, limits).is_err());

    let entity = |count: usize| ResolverSlotSchema {
        name: "item".into(),
        required: true,
        kind: ResolverValueKind::Entity(ResolverEntitySchema {
            kind: "game.item".into(),
            canonical_values: (0..count).map(|index| format!("v{index}")).collect(),
            values_are_exhaustive: false,
        }),
    };
    let mut values = request();
    values.candidates[0].slots = vec![entity(RESOLVER_ENTITY_VALUES_PER_SLOT_MAX)];
    assert!(encode_request(&values, limits).is_ok());
    values.candidates[0].slots = vec![entity(RESOLVER_ENTITY_VALUES_PER_SLOT_MAX + 1)];
    assert!(encode_request(&values, limits).is_err());
}

#[test]
fn reference_candidate_and_context_boundaries_are_exact() {
    let limits = SemanticResolverLimits::default();
    let row = |index: usize| ResolverReferenceCandidate {
        reference: HostReference {
            kind: ReferenceKind::new("hotel"),
            id: ReferenceId::new(format!("h-{index}")),
        },
        label: None,
        aliases: vec![],
    };
    let mut references = request();
    references.reference_candidates = (0..RESOLVER_REFERENCE_CANDIDATES_MAX).map(row).collect();
    assert!(encode_request(&references, limits).is_ok());
    references.reference_candidates.push(row(9_999));
    assert!(encode_request(&references, limits).is_err());

    let mut context = request();
    context.exposed_context = (0..limits.max_context_entries)
        .map(|index| (format!("k{index}"), Value::Bool(true)))
        .collect();
    assert!(encode_request(&context, limits).is_ok());
    context
        .exposed_context
        .insert("overflow".into(), Value::Bool(true));
    assert!(encode_request(&context, limits).is_err());
}

#[test]
fn collection_projection_boundaries_are_exact() {
    let limits = SemanticResolverLimits::default();
    let target = |index: usize| {
        ResolverCollectionTarget::Slot(ResolverSlotSchema {
            name: format!("slot{index}"),
            required: true,
            kind: ResolverValueKind::Number,
        })
    };
    let mut request = collection_request();
    let collection = request.collection.as_mut().expect("collection");
    collection.collectable = (0..RESOLVER_COLLECTION_TARGETS_MAX).map(target).collect();
    assert!(encode_request(&request, limits).is_ok());
    request
        .collection
        .as_mut()
        .expect("collection")
        .collectable
        .push(target(9_999));
    assert!(encode_request(&request, limits).is_err());

    let bound = |index: usize| SlotValue {
        name: format!("bound{index}"),
        value: Value::Bool(true),
        provenance: ValueProvenance::Utterance,
    };
    let mut bounds = collection_request();
    bounds.collection.as_mut().expect("collection").bound_slots = (0
        ..RESOLVER_COLLECTION_BOUND_VALUES_MAX)
        .map(bound)
        .collect();
    assert!(encode_request(&bounds, limits).is_ok());
    bounds
        .collection
        .as_mut()
        .expect("collection")
        .bound_slots
        .push(bound(9_999));
    assert!(encode_request(&bounds, limits).is_err());
}

#[test]
fn request_byte_ceiling_fails_closed() {
    let limits = SemanticResolverLimits {
        max_request_bytes: 64,
        ..SemanticResolverLimits::default()
    };
    assert!(encode_request(&request(), limits).is_err());
}

fn proposal(raw: &str) -> Result<ResolverProposal, String> {
    decode_proposal(raw, SemanticResolverLimits::default())
}

#[test]
fn malformed_external_output_always_fails_closed_without_panicking() {
    let cases = [
        // invalid JSON
        "",
        "{",
        "not json at all",
        "[]",
        // unknown field
        r#"{"meaning":"booking.create","surprise":true}"#,
        r#"{"meaning":"booking.create","capability":"thermostat.set"}"#,
        // malformed slots
        r#"{"slots":[{"name":"count"}]}"#,
        r#"{"slots":[{"name":"count","value":1,"extra":true}]}"#,
        // malformed references
        r#"{"references":[{"kind":"person"}]}"#,
        r#"{"references":[{"kind":"person","id":"p-1","label":"Ali"}]}"#,
        // invalid confidence
        r#"{"meaning":"booking.create","confidence":"high"}"#,
        r#"{"meaning":"booking.create","confidence":-0.1}"#,
        r#"{"meaning":"booking.create","confidence":1.1}"#,
        // non-finite numbers are not representable in strict JSON at all
        r#"{"meaning":"booking.create","confidence":NaN}"#,
        r#"{"slots":[{"name":"count","value":Infinity}]}"#,
    ];
    for raw in cases {
        assert!(proposal(raw).is_err(), "{raw} must fail closed");
    }
}

#[test]
fn the_removed_capability_field_is_now_a_hard_decode_failure() {
    let raw = r#"{"meaning":"greeting.hello","capability":"thermostat.set","confidence":0.9}"#;
    let error = proposal(raw).expect_err("capability is not part of the contract");
    assert!(error.contains("strict proposal JSON"), "{error}");
}

#[test]
fn oversized_external_output_fails_closed_before_parsing() {
    let limits = SemanticResolverLimits {
        max_response_bytes: 32,
        ..SemanticResolverLimits::default()
    };
    let raw = format!(r#"{{"meaning":"{}"}}"#, "x".repeat(512));
    assert!(decode_proposal(&raw, limits).is_err());

    let too_many_slots = SemanticResolverLimits {
        max_slots: 1,
        ..SemanticResolverLimits::default()
    };
    let raw = r#"{"slots":[{"name":"a","value":1},{"name":"b","value":2}]}"#;
    assert!(decode_proposal(raw, too_many_slots).is_err());
}

#[test]
fn valid_external_output_round_trips_into_the_typed_proposal() {
    let raw = r#"{"meaning":"booking.create","slots":[{"name":"count","value":3}],"references":[{"kind":"hotel","id":"h-1"}],"confidence":0.87,"evidence":["explicit count"]}"#;
    let decoded = proposal(raw).expect("valid proposal");
    assert_eq!(
        decoded.meaning.as_ref().map(MeaningId::as_str),
        Some("booking.create")
    );
    assert_eq!(decoded.slots[0].name, "count");
    assert_eq!(decoded.slots[0].value, Value::Number(3.0));
    assert_eq!(decoded.slots[0].provenance, ValueProvenance::NeuralProposal);
    assert_eq!(decoded.references[0].id.as_str(), "h-1");
    assert_eq!(decoded.confidence, Some(0.87));
    assert_eq!(decoded.evidence, vec!["explicit count".to_string()]);

    // An empty proposal is well-formed and simply carries no Meaning for the firewall to accept.
    let empty = proposal("{}").expect("empty proposal");
    assert!(empty.meaning.is_none());
    assert!(empty.slots.is_empty());
    assert!(empty.confidence.is_none());
}

#[test]
fn duplicate_json_slot_keys_do_not_smuggle_a_second_value() {
    // serde_json keeps the last occurrence; the firewall still sees exactly one typed slot and
    // rejects genuine duplicate targets, so this can never become two assignments.
    let raw = r#"{"slots":[{"name":"count","value":1},{"name":"count","value":2}]}"#;
    let decoded = proposal(raw).expect("parses");
    assert_eq!(decoded.slots.len(), 2);
    assert_eq!(decoded.slots[0].name, decoded.slots[1].name);
}

#[test]
fn resolver_callback_errors_propagate_without_panicking() {
    let resolver = JsonSemanticResolver::new(|_request: &str| -> Result<String, String> {
        Err("model unavailable".into())
    });
    assert_eq!(
        resolver.propose(&request()).unwrap_err(),
        "model unavailable"
    );
}

#[test]
fn the_bridge_hands_the_adapter_the_full_typed_projection() {
    let resolver = JsonSemanticResolver::new(|request: &str| -> Result<String, String> {
        let doc: JsonValue = serde_json::from_str(request).expect("valid request JSON");
        assert_eq!(doc["format"], "gvya.semantic.resolver.request");
        assert_eq!(doc["task"], "resolve_meaning");
        let candidate = &doc["candidates"][0];
        assert_eq!(candidate["meaning"], "booking.create");
        assert_eq!(candidate["hints"][0]["text"], "book a hotel");
        assert_eq!(candidate["slots"][0]["kind"]["entity_kind"], "game.item");
        assert_eq!(
            candidate["slots"][0]["kind"]["canonical_values"][0],
            "health_potion"
        );
        assert_eq!(doc["reference_candidates"][0]["id"], "h-1");
        Ok(r#"{"meaning":"booking.create","confidence":0.9}"#.to_string())
    });
    let proposal = resolver.propose(&request()).expect("proposal");
    assert_eq!(
        proposal.meaning.as_ref().map(MeaningId::as_str),
        Some("booking.create")
    );
}
