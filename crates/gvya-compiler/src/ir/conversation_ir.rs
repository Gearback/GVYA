//! Conversation executable IR serialization.

use super::helpers::*;
use super::*;

pub(super) fn conversation(
    project: &ComposedProject,
    config: &ConversationConfig,
) -> Result<JsonValue, IrError> {
    Ok(object([
        (
            "config",
            object([
                (
                    "default_topic_ttl",
                    uint(u64::from(config.default_topic_ttl)),
                ),
                (
                    "default_followup_ttl",
                    uint(u64::from(config.default_followup_ttl)),
                ),
                (
                    "recent_response_limit",
                    usize_json(config.recent_response_limit),
                ),
                (
                    "recent_variant_limit",
                    usize_json(config.recent_variant_limit),
                ),
                ("recent_user_window", usize_json(config.recent_user_window)),
                (
                    "repeat_detection_window",
                    usize_json(config.repeat_detection_window),
                ),
                (
                    "repeat_detection_threshold",
                    uint(u64::from(config.repeat_detection_threshold)),
                ),
                (
                    "max_messages_per_turn",
                    usize_json(config.max_messages_per_turn),
                ),
                (
                    "repair_candidate_min_score",
                    finite(
                        config.repair_candidate_min_score,
                        "conversation.repair_candidate_min_score",
                    )?,
                ),
                (
                    "author_numbers",
                    JsonValue::Array(
                        config
                            .author_numbers
                            .iter()
                            .map(author_number)
                            .collect::<Result<_, _>>()?,
                    ),
                ),
                (
                    "topic_preference_margin",
                    finite(
                        config.topic_preference_margin,
                        "conversation.topic_preference_margin",
                    )?,
                ),
            ]),
        ),
        (
            "behaviors",
            JsonValue::Array(
                project
                    .conversation_catalog
                    .behaviors()
                    .iter()
                    .map(behavior)
                    .collect::<Result<_, _>>()?,
            ),
        ),
        (
            "capability_result_behaviors",
            JsonValue::Array(
                project
                    .conversation_catalog
                    .capability_result_behaviors()
                    .iter()
                    .map(capability_result_behavior)
                    .collect::<Result<_, _>>()?,
            ),
        ),
        (
            "openings",
            JsonValue::Array(
                project
                    .conversation_catalog
                    .openings()
                    .iter()
                    .map(opening)
                    .collect::<Result<_, _>>()?,
            ),
        ),
        (
            "fallback_behaviors",
            JsonValue::Array(
                project
                    .conversation_catalog
                    .fallback_behaviors()
                    .iter()
                    .map(fallback_behavior)
                    .collect::<Result<_, _>>()?,
            ),
        ),
        (
            "style_lexicon",
            object([
                (
                    "formal_terms",
                    strings(&project.conversation_catalog.style_lexicon().formal_terms),
                ),
                (
                    "informal_terms",
                    strings(&project.conversation_catalog.style_lexicon().informal_terms),
                ),
            ]),
        ),
    ]))
}

pub(super) fn behavior(value: &ConversationBehavior) -> Result<JsonValue, IrError> {
    Ok(object([
        ("id", string(value.id.as_str())),
        ("meaning", string(value.meaning.as_str())),
        (
            "topic",
            option_string(value.topic.as_ref().map(|row| row.as_str())),
        ),
        ("topic_scoped", JsonValue::Bool(value.topic_scoped)),
        ("activates_topic", JsonValue::Bool(value.activates_topic)),
        ("topic_ttl", option_u32(value.topic_ttl)),
        (
            "followup_scope",
            option_string(value.followup_scope.as_ref().map(|row| row.as_str())),
        ),
        (
            "repair_continuation_candidate",
            JsonValue::Bool(value.repair_continuation_candidate),
        ),
        (
            "repeat_same_input_after",
            option_u32(value.repeat_same_input_after),
        ),
        (
            "repeat_same_meaning_after",
            option_u32(value.repeat_same_meaning_after),
        ),
        (
            "requires_values",
            JsonValue::Array(
                value
                    .requires_values
                    .iter()
                    .map(requirement)
                    .collect::<Result<_, _>>()?,
            ),
        ),
        (
            "forbidden_values",
            JsonValue::Array(
                value
                    .forbidden_values
                    .iter()
                    .map(requirement)
                    .collect::<Result<_, _>>()?,
            ),
        ),
        (
            "responses",
            JsonValue::Array(
                value
                    .responses
                    .iter()
                    .map(response)
                    .collect::<Result<_, _>>()?,
            ),
        ),
    ]))
}

pub(super) fn capability_result_behavior(
    value: &CapabilityResultBehavior,
) -> Result<JsonValue, IrError> {
    Ok(object([
        ("id", string(value.id.as_str())),
        ("capability", string(value.capability.as_str())),
        (
            "capability_version",
            string(value.capability_version.as_str()),
        ),
        (
            "succeeded",
            value.succeeded.map_or(JsonValue::Null, JsonValue::Bool),
        ),
        ("error_code", option_string(value.error_code.as_deref())),
        (
            "responses",
            JsonValue::Array(
                value
                    .responses
                    .iter()
                    .map(response)
                    .collect::<Result<_, _>>()?,
            ),
        ),
    ]))
}

pub(super) fn opening(value: &OpeningDefinition) -> Result<JsonValue, IrError> {
    Ok(object([
        ("id", string(value.id.as_str())),
        (
            "topic",
            option_string(value.topic.as_ref().map(|row| row.as_str())),
        ),
        ("topic_ttl", option_u32(value.topic_ttl)),
        (
            "responses",
            JsonValue::Array(
                value
                    .responses
                    .iter()
                    .map(response)
                    .collect::<Result<_, _>>()?,
            ),
        ),
    ]))
}

pub(super) fn fallback_behavior(value: &FallbackBehavior) -> Result<JsonValue, IrError> {
    Ok(object([
        ("id", string(value.id.as_str())),
        ("trigger", string(value.trigger.label())),
        ("priority", JsonValue::Number(Number::from(value.priority))),
        (
            "conditions",
            JsonValue::Array(
                value
                    .conditions
                    .iter()
                    .map(condition)
                    .collect::<Result<_, _>>()?,
            ),
        ),
        (
            "responses",
            JsonValue::Array(
                value
                    .responses
                    .iter()
                    .map(response)
                    .collect::<Result<_, _>>()?,
            ),
        ),
    ]))
}

pub(super) fn response(value: &ResponseDefinition) -> Result<JsonValue, IrError> {
    Ok(object([
        ("id", string(value.id.as_str())),
        (
            "kind",
            string(match value.kind {
                ResponseKind::Normal => "normal",
                ResponseKind::Hint => "hint",
                ResponseKind::Repeat => "repeat",
                ResponseKind::AnnoyedRepeat => "annoyed_repeat",
                ResponseKind::FinalRepeat => "final_repeat",
                ResponseKind::Fallback => "fallback",
                ResponseKind::Opening => "opening",
            }),
        ),
        ("texts", localized_texts(&value.texts)),
        (
            "conditions",
            JsonValue::Array(
                value
                    .conditions
                    .iter()
                    .map(condition)
                    .collect::<Result<_, _>>()?,
            ),
        ),
        ("hint_level", option_u32(value.hint_level)),
        (
            "repeat_stage",
            option_string(value.repeat_stage.map(|stage| match stage {
                RepeatStage::Repeat => "repeat",
                RepeatStage::Annoyed => "annoyed",
                RepeatStage::Final => "final",
            })),
        ),
        (
            "effects",
            JsonValue::Array(value.effects.iter().map(effect).collect::<Result<_, _>>()?),
        ),
        (
            "opens_followup",
            value
                .opens_followup
                .as_ref()
                .map_or(JsonValue::Null, followup),
        ),
        (
            "extra_messages",
            JsonValue::Array(
                value
                    .extra_messages
                    .iter()
                    .map(extra_message)
                    .collect::<Result<_, _>>()?,
            ),
        ),
        (
            "assets",
            JsonValue::Array(
                value
                    .assets
                    .iter()
                    .map(|asset| {
                        object([
                            ("asset_id", string(asset.asset_id.as_str())),
                            ("alt_text", option_string(asset.alt_text.as_deref())),
                        ])
                    })
                    .collect(),
            ),
        ),
        (
            "links",
            JsonValue::Array(
                value
                    .links
                    .iter()
                    .map(|link| {
                        object([("label", string(&link.label)), ("url", string(&link.url))])
                    })
                    .collect(),
            ),
        ),
    ]))
}

pub(super) fn localized_texts(values: &[LocalizedTexts]) -> JsonValue {
    JsonValue::Array(
        values
            .iter()
            .map(|value| {
                object([
                    ("language", string(&value.language)),
                    ("variants", strings(&value.variants)),
                ])
            })
            .collect(),
    )
}

pub(super) fn requirement(value: &ValueRequirement) -> Result<JsonValue, IrError> {
    Ok(object([
        (
            "namespace",
            string(match value.path.namespace {
                StateNamespace::Author => "author",
                StateNamespace::Conversation => "conversation",
                StateNamespace::Context => "context",
                StateNamespace::Meaning => "meaning",
                StateNamespace::System => "system",
                StateNamespace::Interaction => "interaction",
            }),
        ),
        ("path", string(&value.path.path)),
        ("value", model_value(&value.value)?),
    ]))
}

pub(super) fn condition(value: &ValueCondition) -> Result<JsonValue, IrError> {
    Ok(object([
        (
            "namespace",
            string(match value.path.namespace {
                StateNamespace::Author => "author",
                StateNamespace::Conversation => "conversation",
                StateNamespace::Context => "context",
                StateNamespace::Meaning => "meaning",
                StateNamespace::System => "system",
                StateNamespace::Interaction => "interaction",
            }),
        ),
        ("path", string(&value.path.path)),
        ("op", string(conversation_predicate_label(value.op))),
        (
            "value",
            match &value.value {
                Some(row) => model_value(row)?,
                None => JsonValue::Null,
            },
        ),
    ]))
}

pub(super) fn effect(value: &ConversationEffect) -> Result<JsonValue, IrError> {
    match value {
        ConversationEffect::Assign { target, value } => Ok(object([
            ("type", string("assign")),
            ("target", state_target(target)),
            ("value", model_value(value)?),
        ])),
        ConversationEffect::Increment { target, delta } => Ok(object([
            ("type", string("increment")),
            ("target", state_target(target)),
            ("delta", finite(*delta, "conversation.effect.delta")?),
        ])),
    }
}

pub(super) fn state_target(value: &StateTarget) -> JsonValue {
    match value {
        StateTarget::Author(path) => {
            object([("namespace", string("author")), ("path", string(path))])
        }
    }
}

pub(super) fn followup(value: &FollowupDirective) -> JsonValue {
    object([
        ("id", string(value.id.as_str())),
        ("ttl", uint(u64::from(value.ttl))),
        ("refresh_if_same", JsonValue::Bool(value.refresh_if_same)),
    ])
}

pub(super) fn extra_message(value: &ExtraMessage) -> Result<JsonValue, IrError> {
    Ok(object([
        (
            "chance",
            finite(value.chance, "conversation.extra_message.chance")?,
        ),
        ("texts", localized_texts(&value.texts)),
    ]))
}

fn author_number(value: &AuthorNumberDefinition) -> Result<JsonValue, IrError> {
    Ok(object([
        ("path", string(&value.path)),
        (
            "default",
            finite(value.default, "conversation.author_numbers.default")?,
        ),
        ("min", finite(value.min, "conversation.author_numbers.min")?),
        ("max", finite(value.max, "conversation.author_numbers.max")?),
    ]))
}
