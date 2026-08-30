//! Small in-memory Bot coverage for the conversation collection lifecycle.

use super::*;
use crate::conversation::{
    ConversationBehavior, ConversationCatalog, ConversationConfig, ResponseDefinition,
};
use crate::semantic::{
    ElicitationPrompt, MeaningPattern, ReferenceSpec, SemanticCatalog, SemanticConfig,
    SemanticProfile, SemanticProfiles, SlotKind, SlotSpec,
};
use gvya_model::{
    BehaviorId, GvyaState, HostReference, MeaningId, ReferenceId, ReferenceKind, ResponseItem,
    Value,
};

fn required(name: &str, kind: SlotKind, question: &str) -> SlotSpec {
    SlotSpec {
        name: name.into(),
        kind,
        required: true,
        elicitation: vec![ElicitationPrompt::new("en", question)],
    }
}

fn behavior(meaning: &str, text: &str) -> ConversationBehavior {
    ConversationBehavior {
        id: BehaviorId::new(format!("{meaning}.behavior")),
        meaning: MeaningId::new(meaning),
        topic: None,
        topic_scoped: false,
        activates_topic: false,
        topic_ttl: None,
        followup_scope: None,
        repair_continuation_candidate: false,
        repeat_same_input_after: None,
        repeat_same_meaning_after: None,
        requires_values: Vec::new(),
        forbidden_values: Vec::new(),
        responses: vec![ResponseDefinition::text(
            format!("{meaning}.response"),
            "en",
            text,
        )],
    }
}

fn kernel(mut patterns: Vec<MeaningPattern>) -> ConversationKernel {
    for pattern in &mut patterns {
        for sample in &mut pattern.samples {
            if sample.language == "und" {
                sample.language = "en".into();
            }
        }
    }
    let behaviors = patterns
        .iter()
        .map(|pattern| behavior(pattern.id.as_str(), "completed"))
        .collect();
    let mut profile = SemanticProfile::empty();
    profile.boolean_values.insert("yes".into(), true);
    profile.boolean_values.insert("no".into(), false);
    let profiles: SemanticProfiles = BTreeMap::from([
        ("en".into(), profile.clone()),
        ("fa".into(), profile.clone()),
        ("und".into(), profile),
    ]);
    ConversationKernel::new(
        SemanticCatalog::new(patterns).expect("semantic catalog"),
        profiles,
        SemanticConfig::default(),
        ConversationCatalog::new(behaviors, Vec::new(), Vec::new(), Vec::new())
            .expect("conversation catalog"),
        ConversationConfig::default(),
    )
    .expect("conversation kernel")
}

fn turn(text: &str, state: GvyaState) -> ConversationTurnRequest {
    let mut request = ConversationTurnRequest::utterance(text, state);
    request.utterance.language = Some("en".into());
    request
}

fn text(outcome: &ConversationOutcome) -> Option<&str> {
    outcome
        .response
        .messages
        .first()?
        .items
        .iter()
        .find_map(|item| {
            if let ResponseItem::Text { text, .. } = item {
                Some(text.as_str())
            } else {
                None
            }
        })
}

#[test]
fn collection_prompts_then_returns_to_the_normal_behavior_pipeline() {
    let mut pattern = MeaningPattern::new("order.create", ["create order"]);
    pattern
        .slots
        .push(required("count", SlotKind::Number, "How many?"));
    let kernel = kernel(vec![pattern]);

    let first = kernel.respond(turn("create order", GvyaState::default()));
    assert_eq!(first.mode, ConversationMode::Collection);
    assert_eq!(text(&first), Some("How many?"));
    assert!(first.meaning.is_none());
    assert!(first.state.conversation.active_collection.is_some());

    let second = kernel.respond(turn("3", first.state));
    assert_eq!(second.mode, ConversationMode::Answer);
    assert_eq!(text(&second), Some("completed"));
    assert_eq!(
        second.meaning.as_ref().unwrap().slots[0].value,
        Value::Number(3.0)
    );
    assert!(second.state.conversation.active_collection.is_none());
    let semantic_trace = &second.semantic.as_ref().unwrap().trace.events;
    assert!(
        semantic_trace
            .iter()
            .any(|entry| entry.code.as_str() == "semantic.decision.collection_resolved")
    );
    assert!(
        !semantic_trace
            .iter()
            .any(|entry| entry.code.as_str() == "semantic.decision.unresolved")
    );
}

#[test]
fn collection_prompt_uses_the_requested_authored_localization() {
    let mut pattern = MeaningPattern::new("order.create", ["create order"]);
    pattern.samples[0].language = "fa".into();
    pattern.slots.push(SlotSpec {
        name: "count".into(),
        kind: SlotKind::Number,
        required: true,
        elicitation: vec![
            ElicitationPrompt::new("en", "How many?"),
            ElicitationPrompt::new("fa", "چند تا؟"),
        ],
    });
    let kernel = kernel(vec![pattern]);
    let mut request = turn("create order", GvyaState::default());
    request.utterance.language = Some("fa".into());

    let outcome = kernel.respond(request);
    assert_eq!(outcome.mode, ConversationMode::Collection);
    assert_eq!(text(&outcome), Some("چند تا؟"));
    assert!(matches!(
        &outcome.response.messages[0].items[0],
        ResponseItem::Text { language, .. } if language.as_deref() == Some("fa")
    ));
}

#[test]
fn collection_preserves_progress_and_retries_invalid_values_without_corruption() {
    let mut pattern = MeaningPattern::new("order.create", ["create order"]);
    pattern
        .slots
        .push(required("count", SlotKind::Number, "How many?"));
    pattern
        .slots
        .push(required("confirmed", SlotKind::Boolean, "Confirm?"));
    let kernel = kernel(vec![pattern]);

    let first = kernel.respond(turn("create order", GvyaState::default()));
    let invalid = kernel.respond(turn("many", first.state));
    assert_eq!(invalid.mode, ConversationMode::Collection);
    assert_eq!(text(&invalid), Some("How many?"));
    assert!(
        invalid
            .state
            .conversation
            .active_collection
            .as_ref()
            .unwrap()
            .meaning
            .slots
            .is_empty()
    );

    let count = kernel.respond(turn("3", invalid.state));
    assert_eq!(count.mode, ConversationMode::Collection);
    assert_eq!(text(&count), Some("Confirm?"));
    assert_eq!(
        count
            .state
            .conversation
            .active_collection
            .as_ref()
            .unwrap()
            .meaning
            .slots[0]
            .value,
        Value::Number(3.0)
    );
    assert!(
        count
            .semantic
            .as_ref()
            .unwrap()
            .trace
            .events
            .iter()
            .any(|entry| { entry.code.as_str() == "semantic.decision.collection_partial" })
    );

    let complete = kernel.respond(turn("yes", count.state));
    assert_eq!(complete.mode, ConversationMode::Answer);
    assert_eq!(complete.meaning.as_ref().unwrap().slots.len(), 2);
}

#[test]
fn a_clear_independent_meaning_cancels_collection_and_switches_topic() {
    let mut order = MeaningPattern::new("order.create", ["create order"]);
    order
        .slots
        .push(required("count", SlotKind::Number, "How many?"));
    let cancel = MeaningPattern::new("cancel", ["never mind"]);
    let kernel = kernel(vec![order, cancel]);

    let collecting = kernel.respond(turn("create order", GvyaState::default()));
    let cancelled = kernel.respond(turn("never mind", collecting.state));
    assert_eq!(cancelled.mode, ConversationMode::Answer);
    assert_eq!(cancelled.meaning.as_ref().unwrap().id.as_str(), "cancel");
    assert!(cancelled.state.conversation.active_collection.is_none());
}

#[test]
fn host_reference_collection_binds_only_visible_canonical_identity() {
    let mut pattern = MeaningPattern::new("door.inspect", ["inspect door"]);
    pattern.references.push(ReferenceSpec {
        kind: ReferenceKind::new("door"),
        required: true,
        elicitation: vec![ElicitationPrompt::new("en", "Which door?")],
    });
    let kernel = kernel(vec![pattern]);
    let collecting = kernel.respond(turn("inspect door", GvyaState::default()));
    let mut continuation = turn("front door", collecting.state);
    continuation
        .reference_candidates
        .push(crate::ResolverReferenceCandidate {
            reference: HostReference {
                kind: ReferenceKind::new("door"),
                id: ReferenceId::new("door-17"),
            },
            label: Some("Front Door".into()),
            aliases: vec!["front door".into()],
        });
    let completed = kernel.respond(continuation);
    assert_eq!(completed.mode, ConversationMode::Answer);
    assert_eq!(
        completed.meaning.as_ref().unwrap().references[0]
            .id
            .as_str(),
        "door-17"
    );
}

#[test]
fn a_language_without_an_authored_elicitation_fails_closed_instead_of_collecting_mutely() {
    let mut pattern = MeaningPattern::new("order.create", ["create order"]);
    pattern.slots.push(SlotSpec {
        name: "count".into(),
        kind: SlotKind::Number,
        required: true,
        elicitation: vec![ElicitationPrompt::new("fa", "چند تا؟")],
    });
    let kernel = kernel(vec![pattern]);

    let outcome = kernel.respond(turn("create order", GvyaState::default()));
    assert_ne!(outcome.mode, ConversationMode::Collection);
    assert!(outcome.state.conversation.active_collection.is_none());
    assert!(
        outcome
            .trace
            .events
            .iter()
            .any(|entry| entry.code.as_str() == "conversation.collection.unelicitable")
    );
}

#[test]
fn structurally_invalid_persisted_collection_state_is_dropped_instead_of_looping() {
    let mut pattern = MeaningPattern::new("order.create", ["create order"]);
    pattern
        .slots
        .push(required("count", SlotKind::Number, "How many?"));
    let kernel = kernel(vec![pattern]);

    let collecting = kernel.respond(turn("create order", GvyaState::default()));
    let mut state = collecting.state;
    // A bound value whose type contradicts the compiled declaration can never be repaired by
    // another user answer.
    state
        .conversation
        .active_collection
        .as_mut()
        .expect("active collection")
        .meaning
        .slots
        .push(gvya_model::SlotValue {
            name: "count".into(),
            value: Value::String("three".into()),
            provenance: gvya_model::ValueProvenance::Utterance,
        });

    let outcome = kernel.respond(turn("3", state));
    assert_ne!(outcome.mode, ConversationMode::Collection);
    assert!(outcome.state.conversation.active_collection.is_none());
    assert!(
        outcome
            .trace
            .events
            .iter()
            .any(|entry| entry.code.as_str() == "conversation.collection.state_rejected")
    );
}
