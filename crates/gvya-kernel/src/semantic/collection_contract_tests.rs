//! Focused contract coverage for partial Meanings and typed multi-turn collection.

use super::*;
use crate::{ResolverProposal, ResolverReferenceCandidate, ResolverRequest, SemanticResolver};
use gvya_model::{
    ActiveCollection, CollectionAuthority, HostReference, ReferenceId, SlotValue, Value,
    ValueProvenance,
};

fn prompt() -> Vec<ElicitationPrompt> {
    vec![ElicitationPrompt::new("en", "Please provide the value.")]
}

fn required_slot(name: &str, kind: SlotKind) -> SlotSpec {
    SlotSpec {
        name: name.into(),
        kind,
        required: true,
        elicitation: prompt(),
    }
}

fn input(text: &str) -> SemanticInput {
    let mut input = SemanticInput::utterance(text);
    input.utterance.language = Some("en".into());
    input
}

fn profiles(mut profile: SemanticProfile) -> SemanticProfiles {
    profile.boolean_values.insert("yes".into(), true);
    profile.boolean_values.insert("no".into(), false);
    BTreeMap::from([("en".into(), profile.clone()), ("und".into(), profile)])
}

fn kernel(mut patterns: Vec<MeaningPattern>, profile: SemanticProfile) -> SemanticKernel {
    for pattern in &mut patterns {
        for sample in &mut pattern.samples {
            sample.language = "en".into();
        }
    }
    SemanticKernel::new(
        SemanticCatalog::new(patterns).expect("catalog"),
        profiles(profile),
        SemanticConfig::default(),
    )
    .expect("kernel")
}

#[test]
fn missing_required_values_are_partial_in_authored_declaration_order() {
    let mut pattern = MeaningPattern::new("meeting.schedule", ["schedule meeting"]);
    pattern.slots.push(required_slot("title", SlotKind::String));
    pattern
        .slots
        .push(required_slot("attendees", SlotKind::Number));
    pattern.slots.push(SlotSpec {
        name: "note".into(),
        kind: SlotKind::String,
        required: false,
        elicitation: Vec::new(),
    });
    pattern.references.push(ReferenceSpec {
        kind: ReferenceKind::new("calendar"),
        required: true,
        elicitation: prompt(),
    });

    let analysis =
        kernel(vec![pattern], SemanticProfile::empty()).analyze(&input("schedule meeting"));
    let SemanticDecision::Partial { partial, source } = analysis.decision else {
        panic!("expected partial Meaning");
    };
    assert_eq!(source, ResolutionSource::Deterministic);
    assert!(partial.meaning.slots.is_empty());
    assert_eq!(
        partial.missing_required_values,
        vec![
            MissingRequiredValue::Slot {
                name: "title".into()
            },
            MissingRequiredValue::Slot {
                name: "attendees".into()
            },
            MissingRequiredValue::Reference {
                kind: ReferenceKind::new("calendar")
            },
        ]
    );
}

#[test]
fn complete_and_optional_binding_behavior_is_unchanged() {
    let mut pattern = MeaningPattern::new("temperature.set", ["set temperature 22"]);
    pattern
        .slots
        .push(required_slot("temperature", SlotKind::Number));
    pattern.slots.push(SlotSpec {
        name: "note".into(),
        kind: SlotKind::String,
        required: false,
        elicitation: Vec::new(),
    });
    let analysis =
        kernel(vec![pattern], SemanticProfile::empty()).analyze(&input("set temperature 22"));
    let SemanticDecision::Resolved { meaning, .. } = analysis.decision else {
        panic!("expected complete Meaning");
    };
    assert_eq!(meaning.slots.len(), 1);
    assert_eq!(meaning.slots[0].value, Value::Number(22.0));
}

#[test]
fn structural_and_semantic_authority_both_preserve_partial_values() {
    let mut structural = MeaningPattern::new("message.send", ["send message"]);
    structural.patterns = vec![LocalizedStructuralPattern::new("en", "send *{body}")];
    structural
        .slots
        .push(required_slot("body", SlotKind::String));
    structural
        .slots
        .push(required_slot("count", SlotKind::Number));
    let analysis = kernel(vec![structural], SemanticProfile::empty()).analyze(&input("send hello"));
    let SemanticDecision::Partial { partial, source } = analysis.decision else {
        panic!("expected structural partial");
    };
    assert_eq!(source, ResolutionSource::StructuralPattern);
    assert_eq!(
        partial.meaning.slots[0].value,
        Value::String("hello".into())
    );
    assert_eq!(
        partial.missing_required_values,
        vec![MissingRequiredValue::Slot {
            name: "count".into()
        }]
    );
}

#[test]
fn deterministic_collection_preserves_values_and_can_fill_distinct_types_together() {
    let mut pattern = MeaningPattern::new("order.create", ["create order"]);
    pattern.slots.push(required_slot("count", SlotKind::Number));
    pattern
        .slots
        .push(required_slot("confirmed", SlotKind::Boolean));
    let kernel = kernel(vec![pattern], SemanticProfile::empty());
    let analysis = kernel.analyze(&input("create order"));
    let SemanticDecision::Partial { partial, .. } = analysis.decision else {
        panic!("expected partial");
    };
    let active = ActiveCollection {
        meaning: partial.meaning,
        remaining: partial.missing_required_values,
        authority: CollectionAuthority::Deterministic,
        started_turn: 1,
    };
    let decision = kernel.continue_collection(&input("22 yes"), &active);
    let CollectionTurnDecision::Completed(meaning) = decision else {
        panic!("expected completed collection: {decision:?}");
    };
    assert!(
        meaning
            .slots
            .iter()
            .any(|slot| { slot.name == "count" && slot.value == Value::Number(22.0) })
    );
    assert!(
        meaning
            .slots
            .iter()
            .any(|slot| { slot.name == "confirmed" && slot.value == Value::Bool(true) })
    );
}

#[test]
fn a_bare_continuation_fills_one_missing_string_slot() {
    let mut pattern = MeaningPattern::new("message.send", ["send message"]);
    pattern.slots.push(required_slot("body", SlotKind::String));
    let kernel = kernel(vec![pattern], SemanticProfile::empty());
    let SemanticDecision::Partial { partial, .. } = kernel.analyze(&input("send message")).decision
    else {
        panic!("expected partial");
    };
    let active = ActiveCollection {
        meaning: partial.meaning,
        remaining: partial.missing_required_values,
        authority: CollectionAuthority::Deterministic,
        started_turn: 1,
    };
    let CollectionTurnDecision::Completed(meaning) =
        kernel.continue_collection(&input("hello there"), &active)
    else {
        panic!("expected completed collection");
    };
    assert_eq!(meaning.slots[0].name, "body");
    assert_eq!(meaning.slots[0].value, Value::String("hello there".into()));
}

#[test]
fn invalid_or_ambiguous_collection_does_not_mutate_active_state() {
    let mut pattern = MeaningPattern::new("door.inspect", ["inspect door"]);
    pattern.references.push(ReferenceSpec {
        kind: ReferenceKind::new("door"),
        required: true,
        elicitation: prompt(),
    });
    let kernel = kernel(vec![pattern], SemanticProfile::empty());
    let SemanticDecision::Partial { partial, .. } = kernel.analyze(&input("inspect door")).decision
    else {
        panic!("expected partial");
    };
    let active = ActiveCollection {
        meaning: partial.meaning,
        remaining: partial.missing_required_values,
        authority: CollectionAuthority::Deterministic,
        started_turn: 1,
    };
    let mut turn = input("front door");
    for id in ["door-1", "door-2"] {
        turn.reference_candidates.push(ResolverReferenceCandidate {
            reference: HostReference {
                kind: ReferenceKind::new("door"),
                id: ReferenceId::new(id),
            },
            label: Some("Front Door".into()),
            aliases: vec!["front door".into()],
        });
    }
    assert!(matches!(
        kernel.continue_collection(&turn, &active),
        CollectionTurnDecision::Ambiguous { .. }
    ));
    assert!(active.meaning.references.is_empty());
    assert_eq!(active.remaining.len(), 1);
}

#[test]
fn custom_entity_catalog_is_shared_by_semantic_and_structural_binding() {
    let mut profile = SemanticProfile::empty();
    profile.custom_entities.insert(
        "game.item".into(),
        BTreeMap::from([("health_potion".into(), vec!["healing flask".into()])]),
    );
    let mut pattern = MeaningPattern::new("item.use", ["use healing flask"]);
    pattern.patterns = vec![LocalizedStructuralPattern::new(
        "en",
        "use <set:entity.game.item>{item}",
    )];
    pattern.slots.push(required_slot(
        "item",
        SlotKind::Entity(EntityKind::new("game.item")),
    ));
    let analysis = kernel(vec![pattern], profile).analyze(&input("use healing flask"));
    let SemanticDecision::Resolved { meaning, source } = analysis.decision else {
        panic!("expected custom entity resolution");
    };
    assert_eq!(source, ResolutionSource::StructuralPattern);
    assert_eq!(
        meaning.slots[0].value,
        Value::String("health_potion".into())
    );
}

#[test]
fn normalized_custom_entity_alias_collisions_fail_kernel_construction() {
    let mut profile = SemanticProfile::empty();
    profile.custom_entities.insert(
        "game.item".into(),
        BTreeMap::from([
            ("health_potion".into(), vec!["potion".into()]),
            ("mana_potion".into(), vec!["POTION".into()]),
        ]),
    );
    let result = SemanticKernel::new(
        SemanticCatalog::new(vec![MeaningPattern::new("item.use", ["use item"])]).unwrap(),
        profiles(profile),
        SemanticConfig::default(),
    );
    assert!(matches!(
        result,
        Err(SemanticKernelBuildError::InvalidCustomEntities(_))
    ));
}

struct PartialResolver;
impl SemanticResolver for PartialResolver {
    type Error = ();

    fn propose(&self, request: &ResolverRequest) -> Result<ResolverProposal, Self::Error> {
        Ok(ResolverProposal {
            meaning: request.candidates.first().map(|row| row.meaning.clone()),
            slots: vec![SlotValue {
                name: "count".into(),
                value: Value::Number(3.0),
                provenance: ValueProvenance::NeuralProposal,
            }],
            references: Vec::new(),
            confidence: Some(0.95),
            evidence: vec!["explicit count".into()],
        })
    }
}

#[test]
fn validated_resolver_values_can_form_a_partial_but_not_escape_the_meaning_boundary() {
    let mut pattern = MeaningPattern::new("order.create", ["create order"]);
    pattern.slots.push(required_slot("count", SlotKind::Number));
    pattern.slots.push(required_slot("note", SlotKind::String));
    let kernel = kernel(vec![pattern], SemanticProfile::empty());
    let analysis = kernel
        .analyze_with_resolver(&input("order maybe"), &PartialResolver)
        .expect("resolver run");
    let SemanticDecision::Partial { partial, source } = analysis.decision else {
        panic!("expected resolver partial");
    };
    assert_eq!(source, ResolutionSource::ResolverProposal);
    assert_eq!(partial.meaning.slots[0].value, Value::Number(3.0));
    assert_eq!(
        partial.missing_required_values,
        vec![MissingRequiredValue::Slot {
            name: "note".into()
        }]
    );
}

struct DuplicateTargetResolver;
impl SemanticResolver for DuplicateTargetResolver {
    type Error = ();

    fn propose(&self, request: &ResolverRequest) -> Result<ResolverProposal, Self::Error> {
        let value = SlotValue {
            name: "count".into(),
            value: Value::Number(3.0),
            provenance: ValueProvenance::NeuralProposal,
        };
        Ok(ResolverProposal {
            meaning: request.candidates.first().map(|row| row.meaning.clone()),
            slots: vec![value.clone(), value],
            references: Vec::new(),
            confidence: Some(0.95),
            evidence: Vec::new(),
        })
    }
}

#[test]
fn duplicate_resolver_targets_fail_closed_during_collection() {
    let mut pattern = MeaningPattern::new("order.create", ["create order"]);
    pattern.slots.push(required_slot("count", SlotKind::Number));
    let kernel = kernel(vec![pattern], SemanticProfile::empty());
    let SemanticDecision::Partial { partial, .. } = kernel.analyze(&input("create order")).decision
    else {
        panic!("expected partial");
    };
    let active = ActiveCollection {
        meaning: partial.meaning,
        remaining: partial.missing_required_values,
        authority: CollectionAuthority::Deterministic,
        started_turn: 1,
    };
    let decision = kernel
        .continue_collection_with_resolver(&input("many"), &active, &DuplicateTargetResolver)
        .expect("resolver run");
    assert!(matches!(
        decision,
        CollectionTurnDecision::Invalid { reason_code }
            if reason_code == "resolver_collection_duplicate_target"
    ));
    assert!(active.meaning.slots.is_empty());
}

struct OutOfRangeConfidenceResolver;
impl SemanticResolver for OutOfRangeConfidenceResolver {
    type Error = ();

    fn propose(&self, request: &ResolverRequest) -> Result<ResolverProposal, Self::Error> {
        Ok(ResolverProposal {
            meaning: request.candidates.first().map(|row| row.meaning.clone()),
            slots: vec![SlotValue {
                name: "count".into(),
                value: Value::Number(3.0),
                provenance: ValueProvenance::NeuralProposal,
            }],
            references: Vec::new(),
            confidence: Some(5.0),
            evidence: Vec::new(),
        })
    }
}

/// Confidence is bounded evidence, not a magnitude. A value above the canonical range must fail
/// closed rather than trivially clearing the configured minimum.
#[test]
fn collection_confidence_outside_the_canonical_range_fails_closed() {
    let mut pattern = MeaningPattern::new("order.create", ["create order"]);
    pattern.slots.push(required_slot("count", SlotKind::Number));
    let kernel = kernel(vec![pattern], SemanticProfile::empty());
    let SemanticDecision::Partial { partial, .. } = kernel.analyze(&input("create order")).decision
    else {
        panic!("expected partial");
    };
    let active = ActiveCollection {
        meaning: partial.meaning,
        remaining: partial.missing_required_values,
        authority: CollectionAuthority::Deterministic,
        started_turn: 1,
    };
    let decision = kernel
        .continue_collection_with_resolver(&input("many"), &active, &OutOfRangeConfidenceResolver)
        .expect("resolver run");
    assert!(matches!(
        decision,
        CollectionTurnDecision::Invalid { reason_code }
            if reason_code == "resolver_invalid_confidence"
    ));
    assert!(active.meaning.slots.is_empty());
}

struct UnknownCustomEntityResolver;
impl SemanticResolver for UnknownCustomEntityResolver {
    type Error = ();

    fn propose(&self, request: &ResolverRequest) -> Result<ResolverProposal, Self::Error> {
        Ok(ResolverProposal {
            meaning: request.candidates.first().map(|row| row.meaning.clone()),
            slots: vec![SlotValue {
                name: "item".into(),
                value: Value::String("invented_item".into()),
                provenance: ValueProvenance::NeuralProposal,
            }],
            references: Vec::new(),
            confidence: Some(0.95),
            evidence: Vec::new(),
        })
    }
}

#[test]
fn resolver_cannot_invent_a_custom_entity_canonical_value() {
    let mut profile = SemanticProfile::empty();
    profile.custom_entities.insert(
        "game.item".into(),
        BTreeMap::from([("health_potion".into(), vec!["potion".into()])]),
    );
    let mut pattern = MeaningPattern::new("item.use", ["use item"]);
    pattern.slots.push(required_slot(
        "item",
        SlotKind::Entity(EntityKind::new("game.item")),
    ));
    let kernel = kernel(vec![pattern], profile);
    let SemanticDecision::Partial { partial, .. } = kernel.analyze(&input("use item")).decision
    else {
        panic!("expected partial");
    };
    let active = ActiveCollection {
        meaning: partial.meaning,
        remaining: partial.missing_required_values,
        authority: CollectionAuthority::Deterministic,
        started_turn: 1,
    };
    let decision = kernel
        .continue_collection_with_resolver(&input("mystery"), &active, &UnknownCustomEntityResolver)
        .expect("resolver run");
    assert!(matches!(
        decision,
        CollectionTurnDecision::Invalid { reason_code }
            if reason_code == "resolver_collection_slot_rejected"
    ));
    assert!(active.meaning.slots.is_empty());
}
