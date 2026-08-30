//! Adversarial coverage for the deterministic semantic firewall and the resolver information
//! model.
//!
//! These tests are load-bearing. The governing rule is that an external resolver may interpret,
//! but GVYA decides what is admissible: a weak, buggy or hostile resolver must never be able to
//! create semantic authority merely by returning well-formed output.

use super::*;
use crate::{
    RESOLVER_HINTS_PER_CANDIDATE_MAX, ResolverCandidateOrigin, ResolverCollectionTarget,
    ResolverEvidenceStrength, ResolverHintKind, ResolverProposal, ResolverReferenceCandidate,
    ResolverRequest, ResolverTask, ResolverValueKind,
};
use gvya_model::{
    ActiveCollection, CollectionAuthority, HostReference, MeaningId, ReferenceId, ReferenceKind,
    SlotValue, Value, ValueProvenance,
};

// -------------------------------------------------------------------------------------------
// Fixtures
// -------------------------------------------------------------------------------------------

fn languages(profile: SemanticProfile) -> SemanticProfiles {
    BTreeMap::from([
        ("und".to_owned(), profile.clone()),
        ("en".to_owned(), profile.clone()),
        ("en-us".to_owned(), profile.clone()),
        ("fa".to_owned(), profile.clone()),
        ("fa-ir".to_owned(), profile),
    ])
}

fn build(patterns: Vec<MeaningPattern>, profile: SemanticProfile) -> SemanticKernel {
    SemanticKernel::new(
        SemanticCatalog::new(patterns).expect("catalog"),
        languages(profile),
        SemanticConfig::default(),
    )
    .expect("kernel")
}

fn english(text: &str) -> SemanticInput {
    let mut input = SemanticInput::utterance(text);
    input.utterance.language = Some("en".into());
    input
}

fn prompt() -> Vec<ElicitationPrompt> {
    vec![ElicitationPrompt::new("en", "Please provide the value.")]
}

fn slot(name: &str, kind: SlotKind, required: bool) -> SlotSpec {
    SlotSpec {
        name: name.into(),
        kind,
        required,
        elicitation: prompt(),
    }
}

fn english_pattern(id: &str, samples: &[&str]) -> MeaningPattern {
    let mut pattern = MeaningPattern::new(id, samples.iter().copied());
    for sample in &mut pattern.samples {
        sample.language = "en".into();
    }
    pattern
}

fn proposal(meaning: &str, slots: Vec<SlotValue>) -> ResolverProposal {
    ResolverProposal {
        meaning: Some(MeaningId::new(meaning)),
        slots,
        references: vec![],
        confidence: Some(0.99),
        evidence: vec!["adversarial".into()],
    }
}

fn number(name: &str, value: f64) -> SlotValue {
    SlotValue {
        name: name.into(),
        value: Value::Number(value),
        provenance: ValueProvenance::NeuralProposal,
    }
}

fn text(name: &str, value: &str) -> SlotValue {
    SlotValue {
        name: name.into(),
        value: Value::String(value.into()),
        provenance: ValueProvenance::NeuralProposal,
    }
}

/// The canonical greeting-versus-action fixture: a harmless social turn and a real device action
/// that must never be reachable from it.
fn greeting_kernel() -> SemanticKernel {
    let mut greeting = english_pattern("greeting.hello", &["hello there", "hi there"]);
    greeting.class = MeaningClass::Social;
    let smalltalk = english_pattern("smalltalk.general", &["how are you", "nice weather"]);
    let mut climate = english_pattern(
        "climate.set_temperature",
        &["set the temperature to 20 degrees", "make it warmer inside"],
    );
    climate
        .slots
        .push(slot("temperature", SlotKind::Number, true));
    build(vec![greeting, smalltalk, climate], SemanticProfile::empty())
}

fn game_profile() -> SemanticProfile {
    let mut profile = SemanticProfile::empty();
    profile.custom_entities.insert(
        "game.item".into(),
        BTreeMap::from([
            ("health_potion".into(), vec!["potion".into()]),
            ("sword".into(), vec!["blade".into()]),
        ]),
    );
    profile
}

/// `order.create` with one entity slot and one number slot: the canonical collection fixture.
fn order_kernel() -> SemanticKernel {
    let mut order = english_pattern("order.create", &["create order", "place an order"]);
    order.slots.push(slot(
        "item",
        SlotKind::Entity(EntityKind::new("game.item")),
        true,
    ));
    order.slots.push(slot("count", SlotKind::Number, true));
    build(vec![order], game_profile())
}

fn order_collection(bound_item: &str) -> ActiveCollection {
    ActiveCollection {
        meaning: Meaning {
            id: MeaningId::new("order.create"),
            slots: vec![SlotValue {
                name: "item".into(),
                value: Value::String(bound_item.into()),
                provenance: ValueProvenance::Utterance,
            }],
            references: vec![],
        },
        remaining: vec![MissingRequiredValue::Slot {
            name: "count".into(),
        }],
        authority: CollectionAuthority::Deterministic,
        started_turn: 1,
    }
}

fn review(
    kernel: &SemanticKernel,
    input: &SemanticInput,
    proposal: ResolverProposal,
) -> (ResolverRequest, ResolverReview) {
    let analysis = kernel.analyze(input);
    let request = kernel.resolver_request(input, &analysis);
    let review = kernel.review_resolver_proposal(input, &analysis, &request, proposal);
    (request, review)
}

// -------------------------------------------------------------------------------------------
// A1 — Meaning candidate authority
// -------------------------------------------------------------------------------------------

/// The headline invariant: "hi" can never become a thermostat command, no matter how confident,
/// well-formed or well-evidenced the resolver output is.
#[test]
fn greeting_cannot_become_a_device_action_through_the_resolver() {
    let kernel = greeting_kernel();
    let input = english("hi there");
    let (request, review) = review(
        &kernel,
        &input,
        proposal("climate.set_temperature", vec![number("temperature", 20.0)]),
    );
    assert!(
        !request.permits_meaning(&MeaningId::new("climate.set_temperature")),
        "fixture must not expose the action Meaning: {:?}",
        request
            .candidates
            .iter()
            .map(|row| row.meaning.as_str())
            .collect::<Vec<_>>()
    );
    assert!(!review.accepted);
    assert_eq!(
        review.reason_code,
        "resolver_meaning_outside_candidate_boundary"
    );
    assert!(review.meaning.is_none() && review.partial.is_none());
}

/// There is no catalog fallback, no fuzzy identifier recovery and no empty-Meaning acceptance.
#[test]
fn no_catalog_fallback_or_close_enough_identifier_recovery_exists() {
    let kernel = greeting_kernel();
    let input = english("hi there");
    for meaning in [
        "greeting.hell",   // one character away from a real exposed candidate
        "GREETING.HELLO",  // case variant
        "greeting.hello ", // whitespace variant
        "",
    ] {
        let (_, review) = review(&kernel, &input, proposal(meaning, vec![]));
        assert!(!review.accepted, "{meaning} must not resolve");
    }
    let (_, missing) = review(
        &kernel,
        &input,
        ResolverProposal {
            meaning: None,
            slots: vec![],
            references: vec![],
            confidence: Some(1.0),
            evidence: vec![],
        },
    );
    assert!(!missing.accepted);
    assert_eq!(missing.reason_code, "resolver_missing_meaning");
}

// -------------------------------------------------------------------------------------------
// A2 — Capability authority
// -------------------------------------------------------------------------------------------

/// Capability selection is structurally impossible: neither side of the contract has a field for
/// it, and a validated resolver Meaning carries only declared semantic values.
#[test]
fn the_resolver_contract_has_no_capability_surface_on_either_side() {
    let kernel = greeting_kernel();
    let input = english("hi there");
    let (request, review) = review(&kernel, &input, proposal("greeting.hello", vec![]));
    assert!(review.accepted);
    let exposed = format!("{request:?}").to_lowercase();
    assert!(!exposed.contains("capabilit"), "{exposed}");
    let proposal_shape = format!("{:?}", proposal("greeting.hello", vec![])).to_lowercase();
    assert!(!proposal_shape.contains("capabilit"), "{proposal_shape}");
}

// -------------------------------------------------------------------------------------------
// A3 — Slot declaration authority
// -------------------------------------------------------------------------------------------

/// A valid candidate does not license invented values. `greeting.hello` declares no slots, so a
/// temperature is a hard rejection rather than a silently discarded extra.
#[test]
fn a_valid_candidate_cannot_carry_an_undeclared_value() {
    let kernel = greeting_kernel();
    let input = english("hi there");
    let (_, review) = review(
        &kernel,
        &input,
        proposal("greeting.hello", vec![number("temperature", 20.0)]),
    );
    assert!(!review.accepted);
    assert_eq!(review.reason_code, "resolver_unknown_slot");
    assert!(review.meaning.is_none() && review.partial.is_none());
}

/// A slot owned by a different Meaning is just as undeclared.
#[test]
fn a_slot_owned_by_another_meaning_is_rejected() {
    let kernel = order_kernel();
    let input = english("create order");
    let (_, review) = review(
        &kernel,
        &input,
        proposal(
            "order.create",
            vec![text("item", "sword"), number("temperature", 20.0)],
        ),
    );
    assert!(!review.accepted);
    assert_eq!(review.reason_code, "resolver_unknown_slot");
}

#[test]
fn duplicate_slot_assignment_fails_closed() {
    let kernel = order_kernel();
    let input = english("create order");
    let (_, review) = review(
        &kernel,
        &input,
        proposal(
            "order.create",
            vec![number("count", 3.0), number("count", 4.0)],
        ),
    );
    assert!(!review.accepted);
    assert_eq!(review.reason_code, "resolver_duplicate_target");
}

// -------------------------------------------------------------------------------------------
// A4/A5 — Type and custom entity authority
// -------------------------------------------------------------------------------------------

#[test]
fn proposed_values_pass_the_same_canonical_type_rules_as_deterministic_extraction() {
    let kernel = order_kernel();
    let input = english("create order");
    for value in [
        Value::String("abc".into()),
        Value::Bool(true),
        Value::Null,
        Value::Array(vec![Value::Number(1.0)]),
        Value::Object(BTreeMap::new()),
    ] {
        let (_, review) = review(
            &kernel,
            &input,
            proposal(
                "order.create",
                vec![SlotValue {
                    name: "count".into(),
                    value: value.clone(),
                    provenance: ValueProvenance::NeuralProposal,
                }],
            ),
        );
        assert!(!review.accepted, "Number slot must reject {value:?}");
        assert_eq!(review.reason_code, "resolver_slot_type_mismatch");
    }
}

/// The resolver may only name canonical values from the authored entity catalog for this
/// Meaning/language/profile. Aliases are deterministic extraction hints, not proposal values.
#[test]
fn custom_entity_values_must_be_authored_canonical_values() {
    let kernel = order_kernel();
    let input = english("create order");
    let cases = [
        ("health_potion", true),
        ("sword", true),
        ("laser_cannon", false), // invented
        ("potion", false),       // alias, not a canonical value
        ("blade", false),        // alias, not a canonical value
        ("HEALTH_POTION", false),
        ("", false),
    ];
    for (value, accepted) in cases {
        let (_, review) = review(
            &kernel,
            &input,
            proposal(
                "order.create",
                vec![text("item", value), number("count", 1.0)],
            ),
        );
        assert_eq!(review.accepted, accepted, "item={value}");
        if !accepted {
            assert_eq!(review.reason_code, "resolver_slot_type_mismatch");
        }
    }
}

/// The catalog is scoped to the turn's language profile: an entity kind with no authority in the
/// active profile cannot be filled at all.
#[test]
fn a_custom_entity_kind_absent_from_the_active_profile_cannot_be_filled() {
    let mut order = english_pattern("order.create", &["create order"]);
    order.slots.push(slot(
        "item",
        SlotKind::Entity(EntityKind::new("game.item")),
        true,
    ));
    let kernel = build(vec![order], SemanticProfile::empty());
    let input = english("create order");
    let (_, review) = review(
        &kernel,
        &input,
        proposal("order.create", vec![text("item", "health_potion")]),
    );
    assert!(!review.accepted);
    assert_eq!(review.reason_code, "resolver_slot_type_mismatch");
}

/// An authored catalog whose normalized aliases collide is rejected at kernel construction, so an
/// ambiguous value can never reach the firewall in the first place.
#[test]
fn ambiguous_custom_entity_catalogs_fail_before_any_resolver_runs() {
    let mut profile = SemanticProfile::empty();
    profile.custom_entities.insert(
        "game.item".into(),
        BTreeMap::from([
            ("health_potion".into(), vec!["potion".into()]),
            ("mana_potion".into(), vec!["Potion".into()]),
        ]),
    );
    let built = SemanticKernel::new(
        SemanticCatalog::new(vec![english_pattern("order.create", &["create order"])]).unwrap(),
        languages(profile),
        SemanticConfig::default(),
    );
    assert!(matches!(
        built,
        Err(SemanticKernelBuildError::InvalidCustomEntities(_))
    ));
}

// -------------------------------------------------------------------------------------------
// A6 — Host reference authority
// -------------------------------------------------------------------------------------------

fn message_kernel() -> SemanticKernel {
    let mut pattern = english_pattern("message.send", &["send a message"]);
    pattern.references.push(ReferenceSpec {
        kind: ReferenceKind::new("person"),
        required: true,
        elicitation: prompt(),
    });
    build(vec![pattern], SemanticProfile::empty())
}

fn person_input(text: &str) -> SemanticInput {
    let mut input = english(text);
    input.reference_candidates.push(ResolverReferenceCandidate {
        reference: HostReference {
            kind: ReferenceKind::new("person"),
            id: ReferenceId::new("person-1"),
        },
        label: Some("Ali".into()),
        aliases: vec!["ali".into()],
    });
    input
}

/// The host-owned stable ID is authority. A label the resolver happens to know does not create a
/// reference, and neither does a plausible-looking identifier.
#[test]
fn an_unexposed_reference_id_is_rejected_even_with_a_correct_label() {
    let kernel = message_kernel();
    let input = person_input("send a message");
    let invented = HostReference {
        kind: ReferenceKind::new("person"),
        id: ReferenceId::new("person-999"),
    };
    let (_, review) = review(
        &kernel,
        &input,
        ResolverProposal {
            meaning: Some(MeaningId::new("message.send")),
            slots: vec![],
            references: vec![invented],
            confidence: Some(1.0),
            evidence: vec!["the user said Ali".into()],
        },
    );
    assert!(!review.accepted);
    assert_eq!(review.reason_code, "resolver_unknown_reference");
}

#[test]
fn a_reference_of_the_wrong_kind_is_rejected_even_when_the_id_exists() {
    let kernel = message_kernel();
    let input = person_input("send a message");
    let wrong_kind = HostReference {
        kind: ReferenceKind::new("door"),
        id: ReferenceId::new("person-1"),
    };
    let (_, review) = review(
        &kernel,
        &input,
        ResolverProposal {
            meaning: Some(MeaningId::new("message.send")),
            slots: vec![],
            references: vec![wrong_kind],
            confidence: Some(1.0),
            evidence: vec![],
        },
    );
    assert!(!review.accepted);
    assert_eq!(review.reason_code, "resolver_unknown_reference");
}

#[test]
fn an_exposed_reference_of_an_undeclared_kind_is_still_rejected() {
    let kernel = greeting_kernel();
    let mut input = english("hi there");
    input.reference_candidates.push(ResolverReferenceCandidate {
        reference: HostReference {
            kind: ReferenceKind::new("person"),
            id: ReferenceId::new("person-1"),
        },
        label: Some("Ali".into()),
        aliases: vec![],
    });
    let (_, review) = review(
        &kernel,
        &input,
        ResolverProposal {
            meaning: Some(MeaningId::new("greeting.hello")),
            slots: vec![],
            references: vec![HostReference {
                kind: ReferenceKind::new("person"),
                id: ReferenceId::new("person-1"),
            }],
            confidence: Some(1.0),
            evidence: vec![],
        },
    );
    assert!(!review.accepted);
    assert_eq!(review.reason_code, "resolver_undeclared_reference");
}

#[test]
fn a_correctly_exposed_and_declared_reference_is_accepted() {
    let kernel = message_kernel();
    let input = person_input("send a message");
    let (_, review) = review(
        &kernel,
        &input,
        ResolverProposal {
            meaning: Some(MeaningId::new("message.send")),
            slots: vec![],
            references: vec![HostReference {
                kind: ReferenceKind::new("person"),
                id: ReferenceId::new("person-1"),
            }],
            confidence: Some(0.9),
            evidence: vec![],
        },
    );
    assert!(review.accepted, "{}", review.reason_code);
    let meaning = review.meaning.expect("complete meaning");
    assert_eq!(meaning.references[0].id.as_str(), "person-1");
}

// -------------------------------------------------------------------------------------------
// A7 — Required-value completeness
// -------------------------------------------------------------------------------------------

/// Missing required declarations produce a Partial Meaning under exactly the deterministic rules.
/// No placeholder, default or null is ever invented, and confidence does not change the outcome.
#[test]
fn missing_required_values_produce_a_partial_never_an_invented_value() {
    let kernel = order_kernel();
    let input = english("create order");
    let (_, review) = review(
        &kernel,
        &input,
        proposal("order.create", vec![text("item", "sword")]),
    );
    assert!(review.accepted);
    assert!(review.meaning.is_none());
    let partial = review.partial.expect("partial meaning");
    assert_eq!(
        partial.missing_required_values,
        vec![MissingRequiredValue::Slot {
            name: "count".into()
        }]
    );
    assert_eq!(partial.meaning.slots.len(), 1);
    assert_eq!(
        partial.meaning.slots[0].value,
        Value::String("sword".into())
    );
}

// -------------------------------------------------------------------------------------------
// A8 — Collection firewall
// -------------------------------------------------------------------------------------------

struct Scripted(ResolverProposal);
impl SemanticResolver for Scripted {
    type Error = ();
    fn propose(&self, _request: &ResolverRequest) -> Result<ResolverProposal, Self::Error> {
        Ok(self.0.clone())
    }
}

fn collection_outcome(proposal: ResolverProposal) -> CollectionTurnDecision {
    let kernel = order_kernel();
    let active = order_collection("sword");
    kernel
        .continue_collection_with_resolver(&english("many"), &active, &Scripted(proposal))
        .expect("resolver run")
}

/// During collection the resolver may fill only what is still collectable. Overwriting an
/// already-bound value is rejected outright rather than partially applied.
#[test]
fn the_resolver_cannot_mutate_an_already_bound_collection_value() {
    let decision = collection_outcome(proposal(
        "order.create",
        vec![text("item", "health_potion"), number("count", 3.0)],
    ));
    assert!(matches!(
        decision,
        CollectionTurnDecision::Invalid { ref reason_code }
            if reason_code == "resolver_collection_undeclared_target"
    ));
}

#[test]
fn the_resolver_cannot_add_an_unrelated_slot_during_collection() {
    let decision = collection_outcome(proposal(
        "order.create",
        vec![number("count", 3.0), number("temperature", 20.0)],
    ));
    assert!(matches!(
        decision,
        CollectionTurnDecision::Invalid { ref reason_code }
            if reason_code == "resolver_collection_undeclared_target"
    ));
}

#[test]
fn the_resolver_cannot_complete_a_collection_through_a_different_meaning() {
    let decision = collection_outcome(proposal("greeting.hello", vec![number("count", 3.0)]));
    assert!(matches!(
        decision,
        CollectionTurnDecision::Invalid { ref reason_code }
            if reason_code == "resolver_collection_boundary_rejected"
    ));
}

#[test]
fn the_resolver_cannot_attach_an_unexposed_reference_during_collection() {
    let decision = collection_outcome(ResolverProposal {
        meaning: Some(MeaningId::new("order.create")),
        slots: vec![number("count", 3.0)],
        references: vec![HostReference {
            kind: ReferenceKind::new("person"),
            id: ReferenceId::new("person-999"),
        }],
        confidence: Some(0.99),
        evidence: vec![],
    });
    assert!(matches!(
        decision,
        CollectionTurnDecision::Invalid { ref reason_code }
            if reason_code == "resolver_collection_undeclared_target"
    ));
}

/// The permitted case: fill exactly the remaining declaration and the collection completes with
/// the already-bound value preserved untouched.
#[test]
fn filling_only_the_remaining_declaration_completes_the_collection() {
    let decision = collection_outcome(proposal("order.create", vec![number("count", 3.0)]));
    let CollectionTurnDecision::Completed(meaning) = decision else {
        panic!("expected completion, got {decision:?}");
    };
    assert_eq!(meaning.id.as_str(), "order.create");
    assert_eq!(meaning.slots.len(), 2);
    assert_eq!(meaning.slots[0].name, "count");
    assert_eq!(meaning.slots[0].value, Value::Number(3.0));
    assert_eq!(meaning.slots[1].name, "item");
    assert_eq!(meaning.slots[1].value, Value::String("sword".into()));
    // The preserved value keeps its original deterministic provenance.
    assert_eq!(meaning.slots[1].provenance, ValueProvenance::Utterance);
}

// -------------------------------------------------------------------------------------------
// A9 — Confidence is evidence, not authority
// -------------------------------------------------------------------------------------------

/// Perfect confidence cannot buy a single deterministic exemption.
#[test]
fn maximum_confidence_bypasses_nothing() {
    let kernel = order_kernel();
    let input = english("create order");
    let violations = [
        (
            "outside candidate boundary",
            proposal("climate.set_temperature", vec![]),
        ),
        (
            "undeclared slot",
            proposal("order.create", vec![number("temperature", 20.0)]),
        ),
        (
            "type mismatch",
            proposal("order.create", vec![text("count", "three")]),
        ),
        (
            "invented entity",
            proposal("order.create", vec![text("item", "laser_cannon")]),
        ),
    ];
    for (label, mut row) in violations {
        row.confidence = Some(1.0);
        let (_, review) = review(&kernel, &input, row);
        assert!(!review.accepted, "{label} must still be rejected");
    }
}

#[test]
fn malformed_confidence_values_fail_closed() {
    let kernel = order_kernel();
    let input = english("create order");
    for confidence in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, -0.1, 1.1] {
        let mut row = proposal("order.create", vec![text("item", "sword")]);
        row.confidence = Some(confidence);
        let (_, review) = review(&kernel, &input, row);
        assert!(!review.accepted, "confidence={confidence}");
        assert_eq!(review.reason_code, "resolver_invalid_confidence");
    }
}

// -------------------------------------------------------------------------------------------
// A10 — Deterministic acceptance
// -------------------------------------------------------------------------------------------

/// The same request and the same proposal always produce the same verdict and the same semantic
/// state. There is no tie-breaking randomness, wall-clock dependency or hidden mutable state.
#[test]
fn acceptance_is_deterministic_under_replay() {
    let kernel = order_kernel();
    let input = english("create order");
    let analysis = kernel.analyze(&input);
    let request = kernel.resolver_request(&input, &analysis);
    let candidate = proposal(
        "order.create",
        vec![text("item", "sword"), number("count", 2.0)],
    );
    let first = kernel.review_resolver_proposal(&input, &analysis, &request, candidate.clone());
    for _ in 0..16 {
        let replay =
            kernel.review_resolver_proposal(&input, &analysis, &request, candidate.clone());
        assert_eq!(replay, first);
    }
    // Rebuilding the request from scratch is stable too.
    let rebuilt = kernel.resolver_request(&input, &kernel.analyze(&input));
    assert_eq!(rebuilt, request);
    assert_eq!(
        kernel.review_resolver_proposal(&input, &analysis, &rebuilt, candidate),
        first
    );
}

#[test]
fn collection_acceptance_is_deterministic_under_replay() {
    let first = collection_outcome(proposal("order.create", vec![number("count", 3.0)]));
    for _ in 0..8 {
        assert_eq!(
            collection_outcome(proposal("order.create", vec![number("count", 3.0)])),
            first
        );
    }
}

// -------------------------------------------------------------------------------------------
// PART F — the resolver information model
// -------------------------------------------------------------------------------------------

fn documented_kernel() -> SemanticKernel {
    let mut order = english_pattern(
        "order.create",
        &["create order", "place an order", "start a new order"],
    );
    order
        .retrieval_terms
        .push(LocalizedText::new("en", "purchase requisition"));
    order
        .negative_samples
        .push(LocalizedText::new("en", "cancel my order"));
    order.slots.push(slot(
        "item",
        SlotKind::Entity(EntityKind::new("game.item")),
        true,
    ));
    order.slots.push(slot("count", SlotKind::Number, true));
    order.slots.push(slot("note", SlotKind::String, false));
    order.references.push(ReferenceSpec {
        kind: ReferenceKind::new("person"),
        required: true,
        elicitation: prompt(),
    });
    let unrelated = english_pattern("weather.ask", &["what is the weather"]);
    build(vec![order, unrelated], game_profile())
}

#[test]
fn a_candidate_exposes_everything_needed_to_interpret_the_turn() {
    let kernel = documented_kernel();
    let mut input = english("create order");
    input.reference_candidates.push(ResolverReferenceCandidate {
        reference: HostReference {
            kind: ReferenceKind::new("person"),
            id: ReferenceId::new("person-1"),
        },
        label: Some("Ali".into()),
        aliases: vec!["ali".into()],
    });
    let analysis = kernel.analyze(&input);
    let request = kernel.resolver_request(&input, &analysis);

    assert_eq!(request.task, ResolverTask::ResolveMeaning);
    assert!(request.collection.is_none());
    assert_eq!(request.language_fallbacks, vec!["en".to_string()]);

    let candidate = request
        .candidate(&MeaningId::new("order.create"))
        .expect("order.create is a candidate");
    assert_eq!(
        candidate.origin,
        ResolverCandidateOrigin::DeterministicMatch
    );

    // Bounded natural-language meaning.
    assert!(!candidate.hints.is_empty());
    assert!(candidate.hints.len() <= RESOLVER_HINTS_PER_CANDIDATE_MAX);
    assert!(
        candidate
            .hints
            .iter()
            .any(|hint| hint.text == "create order" && hint.kind == ResolverHintKind::Sample)
    );
    assert!(candidate.hints.iter().any(|hint| {
        hint.text == "purchase requisition" && hint.kind == ResolverHintKind::RetrievalTerm
    }));

    // Typed semantic slot schema, including canonical entity values.
    let item = candidate
        .slots
        .iter()
        .find(|slot| slot.name == "item")
        .expect("item slot");
    assert!(item.required);
    let ResolverValueKind::Entity(schema) = &item.kind else {
        panic!("item must be an entity declaration");
    };
    assert_eq!(schema.kind, "game.item");
    assert_eq!(
        schema.canonical_values,
        vec!["health_potion".to_string(), "sword".to_string()]
    );
    assert!(schema.values_are_exhaustive);
    let count = candidate
        .slots
        .iter()
        .find(|slot| slot.name == "count")
        .expect("count slot");
    assert_eq!(count.kind, ResolverValueKind::Number);
    assert!(count.required);
    let note = candidate
        .slots
        .iter()
        .find(|slot| slot.name == "note")
        .expect("note slot");
    assert_eq!(note.kind, ResolverValueKind::String);
    assert!(!note.required);

    // Reference declarations are separate from concrete legal references.
    assert_eq!(candidate.references.len(), 1);
    assert_eq!(candidate.references[0].kind.as_str(), "person");
    assert!(candidate.references[0].required);
    assert_eq!(request.reference_candidates.len(), 1);
    assert_eq!(
        request.reference_candidates[0].reference.id.as_str(),
        "person-1"
    );

    // Stable deterministic evidence.
    let evidence = candidate.evidence.as_ref().expect("match evidence");
    assert_eq!(evidence.semantic, ResolverEvidenceStrength::Strong);
    assert!(evidence.matched_terms.contains(&"order".to_string()));
}

/// A strong deterministic candidate and a broad recall candidate are distinguishable without
/// either of them gaining authority from the distinction.
#[test]
fn candidate_origin_and_evidence_separate_strong_matches_from_recall() {
    let kernel = documented_kernel();
    let input = english("create order");
    let analysis = kernel.analyze(&input);
    let request = kernel.resolver_request(&input, &analysis);
    let order = request
        .candidate(&MeaningId::new("order.create"))
        .expect("order candidate");
    assert_eq!(order.origin, ResolverCandidateOrigin::DeterministicMatch);
    assert_eq!(
        order.evidence.as_ref().map(|row| row.semantic),
        Some(ResolverEvidenceStrength::Strong)
    );
    if let Some(weather) = request.candidate(&MeaningId::new("weather.ask")) {
        assert!(
            weather.evidence.as_ref().map(|row| row.semantic)
                != Some(ResolverEvidenceStrength::Strong),
            "an unrelated candidate must not read as strong evidence"
        );
    }
}

/// The projection is a bounded window onto authored semantics, not a source dump.
#[test]
fn withheld_authoring_content_never_reaches_the_resolver() {
    let kernel = documented_kernel();
    let input = english("create order");
    let analysis = kernel.analyze(&input);
    let request = kernel.resolver_request(&input, &analysis);
    let exposed = format!("{request:?}");

    // Authored negative evidence stays inside the deterministic matcher.
    assert!(!exposed.contains("cancel my order"), "{exposed}");
    // Matcher internals, scores and provenance are not part of the contract.
    for leaked in [
        "ScoreBreakdown",
        "retrieval_rank_milli",
        "pattern_index",
        "evidence_tier",
        "negative_",
        "Behavior",
        "Response",
        "Scenario",
        "provenance",
    ] {
        assert!(!exposed.contains(leaked), "{leaked} leaked: {exposed}");
    }
    for candidate in &request.candidates {
        assert!(candidate.hints.len() <= RESOLVER_HINTS_PER_CANDIDATE_MAX);
        assert!(
            candidate
                .hints
                .iter()
                .all(|hint| hint.text.len() <= crate::RESOLVER_HINT_MAX_BYTES)
        );
    }
}

#[test]
fn the_candidate_set_is_bounded_by_the_configured_resolver_limit() {
    let patterns: Vec<_> = (0..64)
        .map(|index| english_pattern(&format!("topic.{index}"), &["shared common phrase"]))
        .collect();
    let kernel = build(patterns, SemanticProfile::empty());
    let input = english("shared common phrase");
    let analysis = kernel.analyze(&input);
    let request = kernel.resolver_request(&input, &analysis);
    assert!(request.candidates.len() <= kernel.config().resolver_candidate_limit);
    assert!(request.candidates.len() <= crate::RESOLVER_REQUEST_CANDIDATES_MAX);
}

#[test]
fn exposed_host_reference_candidates_are_bounded() {
    let kernel = message_kernel();
    let mut input = english("send a message");
    input.reference_candidates = (0..(crate::RESOLVER_REFERENCE_CANDIDATES_MAX + 40))
        .map(|index| ResolverReferenceCandidate {
            reference: HostReference {
                kind: ReferenceKind::new("person"),
                id: ReferenceId::new(format!("person-{index}")),
            },
            label: None,
            aliases: vec![],
        })
        .collect();
    let analysis = kernel.analyze(&input);
    let request = kernel.resolver_request(&input, &analysis);
    assert_eq!(
        request.reference_candidates.len(),
        crate::RESOLVER_REFERENCE_CANDIDATES_MAX
    );
    // A reference the projection did not expose cannot become authority.
    let beyond = HostReference {
        kind: ReferenceKind::new("person"),
        id: ReferenceId::new(format!(
            "person-{}",
            crate::RESOLVER_REFERENCE_CANDIDATES_MAX
        )),
    };
    let review = kernel.review_resolver_proposal(
        &input,
        &analysis,
        &request,
        ResolverProposal {
            meaning: Some(MeaningId::new("message.send")),
            slots: vec![],
            references: vec![beyond],
            confidence: Some(1.0),
            evidence: vec![],
        },
    );
    assert!(!review.accepted);
    assert_eq!(review.reason_code, "resolver_unknown_reference");
}

// -------------------------------------------------------------------------------------------
// B3 — Language-aware projection
// -------------------------------------------------------------------------------------------

fn bilingual_kernel() -> SemanticKernel {
    let mut pattern = MeaningPattern::new("order.create", ["create order"]);
    pattern.samples = vec![
        LocalizedSample::new("en", "create order"),
        LocalizedSample::new("fa", "سفارش بساز"),
    ];
    pattern.retrieval_terms = vec![
        LocalizedText::new("en", "purchase requisition"),
        LocalizedText::new("fa", "خرید"),
    ];
    build(vec![pattern], SemanticProfile::empty())
}

#[test]
fn hints_follow_the_turn_language_and_do_not_leak_other_profiles() {
    let kernel = bilingual_kernel();

    let english_input = english("create order");
    let english_analysis = kernel.analyze(&english_input);
    let english_request = kernel.resolver_request(&english_input, &english_analysis);
    let english_candidate = english_request
        .candidate(&MeaningId::new("order.create"))
        .expect("candidate");
    assert!(
        english_candidate
            .hints
            .iter()
            .all(|hint| hint.language == "en"),
        "{:?}",
        english_candidate.hints
    );
    assert!(
        english_candidate
            .hints
            .iter()
            .any(|hint| hint.text == "create order")
    );

    let mut persian_input = SemanticInput::utterance("سفارش بساز");
    persian_input.utterance.language = Some("fa".into());
    let persian_analysis = kernel.analyze(&persian_input);
    let persian_request = kernel.resolver_request(&persian_input, &persian_analysis);
    let persian_candidate = persian_request
        .candidate(&MeaningId::new("order.create"))
        .expect("candidate");
    assert!(
        persian_candidate
            .hints
            .iter()
            .all(|hint| hint.language == "fa"),
        "{:?}",
        persian_candidate.hints
    );
    assert!(
        persian_candidate
            .hints
            .iter()
            .any(|hint| hint.text == "سفارش بساز")
    );
    assert_eq!(persian_request.language_fallbacks, vec!["fa".to_string()]);
}

// -------------------------------------------------------------------------------------------
// B8/B9 — Collection target information
// -------------------------------------------------------------------------------------------

#[test]
fn a_collection_turn_exposes_the_active_meaning_bound_values_and_remaining_targets() {
    let kernel = order_kernel();
    let active = order_collection("sword");
    let request = kernel
        .collection_resolver_request(&english("many"), &active)
        .expect("collection request");

    assert_eq!(request.task, ResolverTask::FillCollection);
    assert_eq!(request.candidates.len(), 1);
    assert_eq!(
        request.candidates[0].origin,
        ResolverCandidateOrigin::ActiveCollection
    );
    // The Meaning is already authority, so there is no match evidence to report.
    assert!(request.candidates[0].evidence.is_none());

    let collection = request.collection.as_ref().expect("collection context");
    assert_eq!(collection.meaning.as_str(), "order.create");
    assert_eq!(collection.bound_slots.len(), 1);
    assert_eq!(collection.bound_slots[0].name, "item");
    assert_eq!(
        collection.bound_slots[0].value,
        Value::String("sword".into())
    );
    assert!(collection.bound_references.is_empty());

    assert_eq!(collection.collectable.len(), 1);
    let ResolverCollectionTarget::Slot(target) = &collection.collectable[0] else {
        panic!("count is a slot target");
    };
    assert_eq!(target.name, "count");
    assert_eq!(target.kind, ResolverValueKind::Number);
    assert!(target.required);

    // The collectable set is exactly the state the firewall validates against: the already-bound
    // declaration is context, never a proposal target.
    assert!(!collection.collectable.iter().any(
        |target| matches!(target, ResolverCollectionTarget::Slot(slot) if slot.name == "item")
    ));
}

#[test]
fn the_collection_request_is_derived_from_the_same_active_state_the_firewall_validates() {
    let kernel = order_kernel();
    let active = order_collection("health_potion");
    let request = kernel
        .collection_resolver_request(&english("many"), &active)
        .expect("collection request");
    let collection = request.collection.as_ref().expect("collection context");
    assert_eq!(collection.meaning, active.meaning.id);
    assert_eq!(collection.bound_slots, active.meaning.slots);
    assert_eq!(collection.collectable.len(), active.remaining.len());
}
