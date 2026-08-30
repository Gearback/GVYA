//! Conversation source decoding.

use super::super::*;

pub(in crate::source) fn parse_behavior(
    value: &JsonValue,
    path: &str,
    limits: SourceLimits,
    issues: &mut Vec<SourceIssue>,
) -> Option<ConversationBehavior> {
    let obj = expect_object(value, path, issues).ok()?;
    reject_unknown_keys(obj, crate::source::contract::BEHAVIOR_KEYS, path, issues);
    Some(ConversationBehavior {
        id: BehaviorId::new(required_string(obj, "id", path, limits, issues)?),
        meaning: MeaningId::new(required_string(obj, "meaning", path, limits, issues)?),
        topic: optional_id(obj, "topic", TopicId::new, path, limits, issues),
        topic_scoped: optional_bool(obj, "topic_scoped", false, path, issues),
        activates_topic: optional_bool(obj, "activates_topic", false, path, issues),
        topic_ttl: optional_u32(obj, "topic_ttl", path, issues),
        followup_scope: optional_id(obj, "followup_scope", FollowupId::new, path, limits, issues),
        repair_continuation_candidate: optional_bool(
            obj,
            "repair_continuation_candidate",
            false,
            path,
            issues,
        ),
        repeat_same_input_after: optional_u32(obj, "repeat_same_input_after", path, issues),
        repeat_same_meaning_after: optional_u32(obj, "repeat_same_meaning_after", path, issues),
        requires_values: parse_value_requirements(
            obj.get("requires_values"),
            "requires_values",
            path,
            limits,
            issues,
        ),
        forbidden_values: parse_value_requirements(
            obj.get("forbidden_values"),
            "forbidden_values",
            path,
            limits,
            issues,
        ),
        responses: parse_response_array(obj.get("responses"), path, limits, issues),
    })
}

pub(in crate::source) fn parse_fallback_behavior(
    value: &JsonValue,
    path: &str,
    limits: SourceLimits,
    issues: &mut Vec<SourceIssue>,
) -> Option<FallbackBehavior> {
    let obj = expect_object(value, path, issues).ok()?;
    reject_unknown_keys(
        obj,
        crate::source::contract::FALLBACK_BEHAVIOR_KEYS,
        path,
        issues,
    );
    let trigger = match required_string(obj, "trigger", path, limits, issues)?.as_str() {
        "unresolved" => FallbackTrigger::Unresolved,
        "repeat" => FallbackTrigger::Repeat,
        _ => {
            issues.push(issue(
                "source.fallback_trigger",
                "fallback behavior trigger must be unresolved or repeat",
                Some(path),
            ));
            return None;
        }
    };
    let priority = match obj.get("priority") {
        Some(JsonValue::Number(value)) => value
            .as_i64()
            .and_then(|value| i32::try_from(value).ok())
            .unwrap_or_else(|| {
                issues.push(issue(
                    "source.fallback_priority",
                    "fallback behavior priority must be a signed 32-bit integer",
                    Some(path),
                ));
                0
            }),
        None => 0,
        Some(_) => {
            issues.push(issue(
                "source.fallback_priority",
                "fallback behavior priority must be an integer",
                Some(path),
            ));
            0
        }
    };
    let conditions = parse_conditions(obj.get("conditions"), path, limits, issues);
    for condition in &conditions {
        if matches!(
            condition.path.namespace,
            StateNamespace::Meaning | StateNamespace::Interaction
        ) {
            issues.push(issue(
                "source.fallback_condition_namespace",
                "fallback behavior conditions cannot use meaning or interaction namespaces because no semantic meaning/host interaction exists during fallback selection",
                Some(path),
            ));
        }
    }
    Some(FallbackBehavior {
        id: BehaviorId::new(required_string(obj, "id", path, limits, issues)?),
        trigger,
        priority,
        conditions,
        responses: parse_response_array(obj.get("responses"), path, limits, issues),
    })
}

pub(in crate::source) fn parse_capability_result_behavior(
    value: &JsonValue,
    path: &str,
    limits: SourceLimits,
    issues: &mut Vec<SourceIssue>,
) -> Option<CapabilityResultBehavior> {
    let obj = expect_object(value, path, issues).ok()?;
    reject_unknown_keys(
        obj,
        crate::source::contract::CAPABILITY_RESULT_BEHAVIOR_KEYS,
        path,
        issues,
    );
    let succeeded = match obj.get("succeeded") {
        None | Some(JsonValue::Null) => None,
        Some(JsonValue::Bool(value)) => Some(*value),
        Some(_) => {
            issues.push(issue(
                "source.capability_result_succeeded",
                "succeeded must be boolean or null",
                Some(path),
            ));
            None
        }
    };
    let error_code = optional_source_string(obj, "error_code", path, limits, issues)
        .filter(|value| !value.is_empty());
    if succeeded == Some(true) && error_code.is_some() {
        issues.push(issue(
            "source.capability_result_handler",
            "successful capability-result handler cannot require an error_code",
            Some(path),
        ));
    }
    Some(CapabilityResultBehavior {
        id: BehaviorId::new(required_string(obj, "id", path, limits, issues)?),
        capability: CapabilityId::new(required_string(obj, "capability", path, limits, issues)?),
        capability_version: CapabilityVersion::new(required_string(
            obj,
            "capability_version",
            path,
            limits,
            issues,
        )?),
        succeeded,
        error_code,
        responses: parse_response_array(obj.get("responses"), path, limits, issues),
    })
}

pub(in crate::source) fn parse_opening(
    value: &JsonValue,
    path: &str,
    limits: SourceLimits,
    issues: &mut Vec<SourceIssue>,
) -> Option<OpeningDefinition> {
    let obj = expect_object(value, path, issues).ok()?;
    reject_unknown_keys(obj, crate::source::contract::OPENING_KEYS, path, issues);
    Some(OpeningDefinition {
        id: gvya_model::OpeningId::new(required_string(obj, "id", path, limits, issues)?),
        topic: optional_id(obj, "topic", TopicId::new, path, limits, issues),
        topic_ttl: optional_u32(obj, "topic_ttl", path, issues),
        responses: parse_response_array(obj.get("responses"), path, limits, issues),
    })
}

pub(in crate::source) fn parse_response_array(
    value: Option<&JsonValue>,
    path: &str,
    limits: SourceLimits,
    issues: &mut Vec<SourceIssue>,
) -> Vec<ResponseDefinition> {
    let Some(array) = optional_array(value, path, issues) else {
        return Vec::new();
    };
    array
        .iter()
        .enumerate()
        .filter_map(|(index, row)| {
            parse_response(row, &format!("{path}.responses[{index}]"), limits, issues)
        })
        .collect()
}

pub(in crate::source) fn parse_response(
    value: &JsonValue,
    path: &str,
    limits: SourceLimits,
    issues: &mut Vec<SourceIssue>,
) -> Option<ResponseDefinition> {
    let obj = expect_object(value, path, issues).ok()?;
    reject_unknown_keys(obj, crate::source::contract::RESPONSE_KEYS, path, issues);
    let kind = match optional_string(obj, "kind", "normal", path, limits, issues).as_str() {
        "normal" => ResponseKind::Normal,
        "hint" => ResponseKind::Hint,
        "repeat" => ResponseKind::Repeat,
        "annoyed_repeat" => ResponseKind::AnnoyedRepeat,
        "final_repeat" => ResponseKind::FinalRepeat,
        "fallback" => ResponseKind::Fallback,
        "opening" => ResponseKind::Opening,
        _ => {
            issues.push(issue(
                "source.response_kind",
                "unknown response kind",
                Some(path),
            ));
            ResponseKind::Normal
        }
    };
    Some(ResponseDefinition {
        id: ResponseId::new(required_string(obj, "id", path, limits, issues)?),
        kind,
        texts: parse_localized_texts(obj.get("texts"), path, limits, issues),
        conditions: parse_conditions(obj.get("conditions"), path, limits, issues),
        hint_level: optional_u32(obj, "hint_level", path, issues),
        repeat_stage: match optional_source_string(obj, "repeat_stage", path, limits, issues)
            .as_deref()
        {
            None => None,
            Some("repeat") => Some(gvya_kernel::conversation::RepeatStage::Repeat),
            Some("annoyed") => Some(gvya_kernel::conversation::RepeatStage::Annoyed),
            Some("final") => Some(gvya_kernel::conversation::RepeatStage::Final),
            Some(_) => {
                issues.push(issue(
                    "source.repeat_stage",
                    "unknown repeat_stage",
                    Some(path),
                ));
                None
            }
        },
        effects: parse_effects(obj.get("effects"), path, limits, issues),
        opens_followup: parse_followup(obj.get("opens_followup"), path, limits, issues),
        extra_messages: parse_extra_messages(obj.get("extra_messages"), path, limits, issues),
        assets: parse_response_assets(obj.get("assets"), path, limits, issues),
        links: parse_links(obj.get("links"), path, limits, issues),
    })
}

pub(in crate::source) fn parse_localized_texts(
    value: Option<&JsonValue>,
    path: &str,
    limits: SourceLimits,
    issues: &mut Vec<SourceIssue>,
) -> Vec<LocalizedTexts> {
    let Some(array) = optional_array(value, path, issues) else {
        return Vec::new();
    };
    array
        .iter()
        .enumerate()
        .filter_map(|(index, row)| {
            let row_path = format!("{path}.texts[{index}]");
            let obj = expect_object(row, &row_path, issues).ok()?;
            reject_unknown_keys(
                obj,
                crate::source::contract::LOCALIZED_TEXTS_KEYS,
                &row_path,
                issues,
            );
            let variants = string_array(obj.get("variants"), &row_path, limits, issues)
                .into_iter()
                .filter(|variant| {
                    if variant.len() > TEMPLATE_MAX_OUTPUT_BYTES {
                        issues.push(issue(
                            "source.response_text_too_large",
                            "response variant exceeds canonical rendered-text byte limit",
                            Some(&row_path),
                        ));
                        false
                    } else {
                        true
                    }
                })
                .collect();
            Some(LocalizedTexts {
                language: required_string(obj, "language", &row_path, limits, issues)?,
                variants,
            })
        })
        .collect()
}

pub(in crate::source) fn parse_value_requirements(
    value: Option<&JsonValue>,
    key: &str,
    path: &str,
    limits: SourceLimits,
    issues: &mut Vec<SourceIssue>,
) -> Vec<ValueRequirement> {
    let Some(array) = optional_array(value, &format!("{path}.{key}"), issues) else {
        return Vec::new();
    };
    array
        .iter()
        .enumerate()
        .filter_map(|(index, row)| {
            let row_path = format!("{path}.{key}[{index}]");
            let obj = expect_object(row, &row_path, issues).ok()?;
            reject_unknown_keys(
                obj,
                crate::source::contract::VALUE_REQUIREMENT_KEYS,
                &row_path,
                issues,
            );
            let namespace =
                match required_string(obj, "namespace", &row_path, limits, issues)?.as_str() {
                    "author" => StateNamespace::Author,
                    "conversation" => StateNamespace::Conversation,
                    "context" => StateNamespace::Context,
                    "meaning" => StateNamespace::Meaning,
                    "system" => StateNamespace::System,
                    _ => {
                        issues.push(issue(
                            "source.requirement_namespace",
                            "unknown value requirement namespace",
                            Some(&row_path),
                        ));
                        return None;
                    }
                };
            let raw_value = obj
                .get("value")
                .filter(|value| !value.is_null())
                .or_else(|| {
                    issues.push(issue(
                        "source.requirement_value",
                        "value requirement must contain a non-null value",
                        Some(&row_path),
                    ));
                    None
                })?;
            Some(ValueRequirement {
                path: ValuePath {
                    namespace,
                    path: required_string(obj, "path", &row_path, limits, issues)?,
                },
                value: parse_model_value(raw_value, &row_path, issues)?,
            })
        })
        .collect()
}

pub(in crate::source) fn parse_conditions(
    value: Option<&JsonValue>,
    path: &str,
    limits: SourceLimits,
    issues: &mut Vec<SourceIssue>,
) -> Vec<ValueCondition> {
    let Some(array) = optional_array(value, path, issues) else {
        return Vec::new();
    };
    array
        .iter()
        .enumerate()
        .filter_map(|(index, row)| {
            let row_path = format!("{path}.conditions[{index}]");
            let obj = expect_object(row, &row_path, issues).ok()?;
            reject_unknown_keys(
                obj,
                crate::source::contract::VALUE_CONDITION_KEYS,
                &row_path,
                issues,
            );
            let namespace =
                match required_string(obj, "namespace", &row_path, limits, issues)?.as_str() {
                    "author" => StateNamespace::Author,
                    "conversation" => StateNamespace::Conversation,
                    "context" => StateNamespace::Context,
                    "meaning" => StateNamespace::Meaning,
                    "system" => StateNamespace::System,
                    "interaction" => StateNamespace::Interaction,
                    _ => {
                        issues.push(issue(
                            "source.condition_namespace",
                            "unknown condition namespace",
                            Some(&row_path),
                        ));
                        return None;
                    }
                };
            let op = parse_conversation_op(
                required_string(obj, "op", &row_path, limits, issues)?.as_str(),
                &row_path,
                issues,
            )?;
            Some(ValueCondition {
                path: ValuePath {
                    namespace,
                    path: required_string(obj, "path", &row_path, limits, issues)?,
                },
                op,
                value: obj
                    .get("value")
                    .filter(|value| !value.is_null())
                    .and_then(|value| parse_model_value(value, &row_path, issues)),
            })
        })
        .collect()
}

pub(in crate::source) fn parse_effects(
    value: Option<&JsonValue>,
    path: &str,
    limits: SourceLimits,
    issues: &mut Vec<SourceIssue>,
) -> Vec<ConversationEffect> {
    let Some(array) = optional_array(value, path, issues) else {
        return Vec::new();
    };
    array
        .iter()
        .enumerate()
        .filter_map(|(index, row)| {
            let row_path = format!("{path}.effects[{index}]");
            let obj = expect_object(row, &row_path, issues).ok()?;
            let target_obj = expect_object(
                required_value(obj, "target", &row_path, issues)?,
                &format!("{row_path}.target"),
                issues,
            )
            .ok()?;
            reject_unknown_keys(
                target_obj,
                crate::source::contract::NAMESPACE_VALUE_KEYS,
                &format!("{row_path}.target"),
                issues,
            );
            let target_namespace = optional_source_string(
                target_obj,
                "namespace",
                &format!("{row_path}.target"),
                limits,
                issues,
            )
            .unwrap_or_else(|| "author".to_owned());
            if target_namespace != "author" {
                issues.push(issue(
                    "source.effect_authority",
                    "conversation effects may only target author state",
                    Some(&row_path),
                ));
                return None;
            }
            let target = StateTarget::Author(required_string(
                target_obj, "path", &row_path, limits, issues,
            )?);
            match required_string(obj, "type", &row_path, limits, issues)?.as_str() {
                "assign" => {
                    reject_unknown_keys(
                        obj,
                        crate::source::contract::CONVERSATION_EFFECT_ASSIGN_KEYS,
                        &row_path,
                        issues,
                    );
                    Some(ConversationEffect::Assign {
                        target,
                        value: parse_model_value(
                            required_value(obj, "value", &row_path, issues)?,
                            &row_path,
                            issues,
                        )?,
                    })
                }
                "increment" => {
                    reject_unknown_keys(
                        obj,
                        crate::source::contract::CONVERSATION_EFFECT_INCREMENT_KEYS,
                        &row_path,
                        issues,
                    );
                    Some(ConversationEffect::Increment {
                        target,
                        delta: required_f64(obj, "delta", &row_path, issues)?,
                    })
                }
                _ => {
                    issues.push(issue(
                        "source.effect_type",
                        "unknown conversation effect type",
                        Some(&row_path),
                    ));
                    None
                }
            }
        })
        .collect()
}

pub(in crate::source) fn parse_followup(
    value: Option<&JsonValue>,
    path: &str,
    limits: SourceLimits,
    issues: &mut Vec<SourceIssue>,
) -> Option<FollowupDirective> {
    let value = value.filter(|value| !value.is_null())?;
    let obj = expect_object(value, &format!("{path}.opens_followup"), issues).ok()?;
    reject_unknown_keys(
        obj,
        crate::source::contract::FOLLOWUP_DIRECTIVE_KEYS,
        &format!("{path}.opens_followup"),
        issues,
    );
    Some(FollowupDirective {
        id: FollowupId::new(required_string(obj, "id", path, limits, issues)?),
        ttl: required_u32(obj, "ttl", path, issues)?,
        refresh_if_same: optional_bool(obj, "refresh_if_same", false, path, issues),
    })
}

pub(in crate::source) fn parse_extra_messages(
    value: Option<&JsonValue>,
    path: &str,
    limits: SourceLimits,
    issues: &mut Vec<SourceIssue>,
) -> Vec<ExtraMessage> {
    let Some(array) = optional_array(value, path, issues) else {
        return Vec::new();
    };
    array
        .iter()
        .enumerate()
        .filter_map(|(index, row)| {
            let row_path = format!("{path}.extra_messages[{index}]");
            let obj = expect_object(row, &row_path, issues).ok()?;
            reject_unknown_keys(
                obj,
                crate::source::contract::EXTRA_MESSAGE_KEYS,
                &row_path,
                issues,
            );
            let chance = required_f64(obj, "chance", &row_path, issues)?;
            Some(ExtraMessage {
                chance,
                texts: parse_localized_texts(obj.get("texts"), &row_path, limits, issues),
            })
        })
        .collect()
}

pub(in crate::source) fn parse_response_assets(
    value: Option<&JsonValue>,
    path: &str,
    limits: SourceLimits,
    issues: &mut Vec<SourceIssue>,
) -> Vec<ResponseAsset> {
    let Some(array) = optional_array(value, path, issues) else {
        return Vec::new();
    };
    array
        .iter()
        .enumerate()
        .filter_map(|(index, row)| {
            let p = format!("{path}.assets[{index}]");
            let obj = expect_object(row, &p, issues).ok()?;
            reject_unknown_keys(
                obj,
                crate::source::contract::RESPONSE_ASSET_KEYS,
                &p,
                issues,
            );
            Some(ResponseAsset {
                asset_id: AssetId::new(required_string(obj, "asset_id", &p, limits, issues)?),
                alt_text: optional_source_string(obj, "alt_text", &p, limits, issues),
            })
        })
        .collect()
}
pub(in crate::source) fn parse_links(
    value: Option<&JsonValue>,
    path: &str,
    limits: SourceLimits,
    issues: &mut Vec<SourceIssue>,
) -> Vec<ResponseLink> {
    let Some(array) = optional_array(value, path, issues) else {
        return Vec::new();
    };
    array
        .iter()
        .enumerate()
        .filter_map(|(index, row)| {
            let p = format!("{path}.links[{index}]");
            let obj = expect_object(row, &p, issues).ok()?;
            reject_unknown_keys(obj, crate::source::contract::RESPONSE_LINK_KEYS, &p, issues);
            Some(ResponseLink {
                label: required_string(obj, "label", &p, limits, issues)?,
                url: required_string(obj, "url", &p, limits, issues)?,
            })
        })
        .collect()
}

pub(in crate::source) fn parse_style_patch(
    value: &JsonValue,
    path: &str,
    limits: SourceLimits,
    issues: &mut Vec<SourceIssue>,
) -> Option<StyleLexiconPatch> {
    let obj = expect_object(value, path, issues).ok()?;
    reject_unknown_keys(
        obj,
        crate::source::contract::STYLE_LEXICON_KEYS,
        path,
        issues,
    );
    Some(StyleLexiconPatch {
        formal_terms: string_set(obj.get("formal_terms"), path, limits, issues),
        informal_terms: string_set(obj.get("informal_terms"), path, limits, issues),
    })
}
