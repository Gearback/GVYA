//! Runtime wire tests.
use super::*;

#[test]
fn wire_v1_rejects_non_v1_without_a_compatibility_reader() {
    let doc = serde_json::json!({
        "format": TURN_REQUEST_FORMAT,
        "version": 2,
        "utterance": {"text": "hello"},
        "seed": null,
    });
    let bytes = serde_json::to_vec(&doc).unwrap();
    assert!(matches!(
        parse_turn_request(&bytes),
        Err(WireError::Version(2))
    ));
}

#[test]
fn wire_v1_rejects_host_selected_conversation_language() {
    for doc in [
        serde_json::json!({
            "format": TURN_REQUEST_FORMAT,
            "version": WIRE_VERSION,
            "utterance": {"text": "hello", "language": "en"},
            "seed": null,
        }),
        serde_json::json!({
            "format": TURN_REQUEST_FORMAT,
            "version": WIRE_VERSION,
            "utterance": {"text": "hello"},
            "language_policy": {"fallbacks": ["en"]},
            "seed": null,
        }),
    ] {
        assert!(parse_turn_request(&serde_json::to_vec(&doc).unwrap()).is_err());
    }
}

#[test]
fn wire_rejects_oversized_and_deep_requests() {
    let limits = WireLimits {
        max_request_bytes: 64,
        ..WireLimits::default()
    };
    assert!(matches!(
        parse_turn_request_with_limits(&vec![b' '; 65], limits),
        Err(WireError::Invalid(_))
    ));

    let mut value = JsonValue::String("x".into());
    for _ in 0..(WireLimits::default().max_value_depth + 2) {
        value = JsonValue::Array(vec![value]);
    }
    let bytes = serde_json::to_vec(&value).unwrap();
    let result: Result<JsonValue, WireError> = parse_bounded_doc(&bytes, WireLimits::default());
    assert!(matches!(result, Err(WireError::Invalid(_))));
}

#[test]
fn runtime_state_serialization_is_accepted_as_next_turn_input() {
    let mut state = GvyaState::default();
    for index in 0..MAX_MENTIONED_TOPICS {
        state
            .conversation
            .mentioned_topics
            .push(TopicId::new(format!("topic.{index}")));
    }
    for index in 0..MAX_HINT_PROGRESS_ENTRIES {
        state
            .conversation
            .hint_progress
            .insert(format!("hint.{index}"), 1);
    }
    state.conversation.active_collection = Some(ActiveCollection {
        meaning: Meaning {
            id: MeaningId::new("order.create"),
            slots: vec![SlotValue {
                name: "count".into(),
                value: Value::Number(2.0),
                provenance: ValueProvenance::Utterance,
            }],
            references: Vec::new(),
        },
        remaining: vec![MissingRequiredValue::Slot {
            name: "confirmed".into(),
        }],
        authority: CollectionAuthority::StructuralPattern,
        started_turn: 4,
    });
    let doc = serde_json::json!({
        "format": TURN_REQUEST_FORMAT,
        "version": WIRE_VERSION,
        "utterance": {"text":"hello"},
        "seed": null,
        "state": state_json(&state),
    });
    let bytes = serde_json::to_vec(&doc).unwrap();
    let parsed = parse_turn_request(&bytes).expect("serialized state must round-trip");
    assert_eq!(
        parsed.state.conversation.active_collection,
        state.conversation.active_collection
    );
}

#[test]
fn empty_or_oversized_active_collection_state_is_rejected_before_runtime_execution() {
    let doc = serde_json::json!({
        "format": TURN_REQUEST_FORMAT,
        "version": WIRE_VERSION,
        "utterance": {"text":"hello"},
        "seed": null,
        "state": {"conversation": {"active_collection": {
            "meaning": {"id":"order.create","slots":[],"references":[]},
            "remaining": [],
            "authority": "deterministic",
            "started_turn": 1
        }}}
    });
    assert!(matches!(
        parse_turn_request(&serde_json::to_vec(&doc).unwrap()),
        Err(WireError::Invalid(_))
    ));
}

#[test]
fn partial_semantic_decision_has_a_stable_typed_wire_projection() {
    let decision = gvya_kernel::semantic::SemanticDecision::Partial {
        partial: gvya_kernel::semantic::PartialMeaning {
            meaning: Meaning {
                id: MeaningId::new("order.create"),
                slots: Vec::new(),
                references: Vec::new(),
            },
            missing_required_values: vec![MissingRequiredValue::Slot {
                name: "count".into(),
            }],
        },
        source: gvya_kernel::semantic::ResolutionSource::Deterministic,
    };
    assert_eq!(
        semantic_decision(&decision),
        serde_json::json!({
            "type":"partial",
            "meaning":{"id":"order.create","slots":[],"references":[]},
            "missing_required_values":[{"type":"slot","name":"count"}],
            "source":"deterministic"
        })
    );
}

#[test]
fn runtime_managed_state_limits_are_enforced_on_input() {
    let doc = serde_json::json!({
        "format": TURN_REQUEST_FORMAT, "version": WIRE_VERSION,
        "utterance": {"text":"hello"}, "seed": null,
        "state": {"conversation": {"mentioned_topics": (0..=MAX_MENTIONED_TOPICS).map(|i| format!("t{i}")).collect::<Vec<_>>()}}
    });
    let bytes = serde_json::to_vec(&doc).unwrap();
    assert!(matches!(
        parse_turn_request(&bytes),
        Err(WireError::Invalid(_))
    ));
}
