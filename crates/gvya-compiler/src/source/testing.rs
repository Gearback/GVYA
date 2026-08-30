//! Canonical regression/scenario/context/state source decoding.

use super::*;

pub(super) fn parse_regression_case(
    value: &JsonValue,
    path: &str,
    limits: SourceLimits,
    issues: &mut Vec<SourceIssue>,
) -> Option<RegressionCase> {
    let obj = expect_object(value, path, issues).ok()?;
    reject_unknown_keys(
        obj,
        crate::source::contract::REGRESSION_CASE_KEYS,
        path,
        issues,
    );
    Some(RegressionCase {
        id: TestCaseId::new(required_string(obj, "id", path, limits, issues)?),
        description: optional_string(obj, "description", "", path, limits, issues),
        input: required_string(obj, "input", path, limits, issues)?,
        language: optional_source_string(obj, "language", path, limits, issues),
        context: parse_context(obj.get("context"), &format!("{path}.context"), issues),
        initial_state: parse_state(
            obj.get("initial_state"),
            &format!("{path}.initial_state"),
            issues,
        ),
        seed: optional_u64_strict(obj, "seed", path, issues),
        unix_time_ms: optional_i64_strict(obj, "unix_time_ms", path, issues),
        expectation: parse_expectation(
            obj.get("expectation"),
            &format!("{path}.expectation"),
            limits,
            issues,
        ),
        generated: optional_bool(obj, "generated", false, path, issues),
    })
}

pub(super) fn parse_scenario(
    value: &JsonValue,
    path: &str,
    limits: SourceLimits,
    issues: &mut Vec<SourceIssue>,
) -> Option<ConversationScenario> {
    let obj = expect_object(value, path, issues).ok()?;
    reject_unknown_keys(obj, crate::source::contract::SCENARIO_KEYS, path, issues);
    let steps = match obj.get("steps") {
        None => {
            issues.push(issue(
                "source.field_required",
                "steps is required",
                Some(path),
            ));
            Vec::new()
        }
        Some(value) => optional_array(Some(value), &format!("{path}.steps"), issues).map_or_else(
            Vec::new,
            |rows| {
                rows.iter()
                    .enumerate()
                    .filter_map(|(index, row)| {
                        parse_scenario_step(row, &format!("{path}.steps[{index}]"), limits, issues)
                    })
                    .collect()
            },
        ),
    };
    Some(ConversationScenario {
        id: ScenarioId::new(required_string(obj, "id", path, limits, issues)?),
        description: optional_string(obj, "description", "", path, limits, issues),
        context: parse_context(obj.get("context"), &format!("{path}.context"), issues),
        initial_state: parse_state(
            obj.get("initial_state"),
            &format!("{path}.initial_state"),
            issues,
        ),
        steps,
        generated: optional_bool(obj, "generated", false, path, issues),
    })
}

fn parse_scenario_step(
    value: &JsonValue,
    path: &str,
    limits: SourceLimits,
    issues: &mut Vec<SourceIssue>,
) -> Option<ScenarioStep> {
    let obj = expect_object(value, path, issues).ok()?;
    let kind = required_string(obj, "type", path, limits, issues)?;
    let context =
        |obj: &serde_json::Map<String, JsonValue>, issues: &mut Vec<SourceIssue>| match obj
            .get("context")
        {
            None | Some(JsonValue::Null) => None,
            Some(value) => Some(parse_context(
                Some(value),
                &format!("{path}.context"),
                issues,
            )),
        };
    let expectation = |obj: &serde_json::Map<String, JsonValue>, issues: &mut Vec<SourceIssue>| {
        parse_expectation(
            obj.get("expectation"),
            &format!("{path}.expectation"),
            limits,
            issues,
        )
    };
    match kind.as_str() {
        "open" => {
            reject_unknown_keys(
                obj,
                &[
                    "type",
                    "language",
                    "context",
                    "seed",
                    "unix_time_ms",
                    "expectation",
                ],
                path,
                issues,
            );
            Some(ScenarioStep::Open(ScenarioOpenStep {
                language: optional_source_string(obj, "language", path, limits, issues),
                context: context(obj, issues),
                seed: optional_u64_strict(obj, "seed", path, issues),
                unix_time_ms: optional_i64_strict(obj, "unix_time_ms", path, issues),
                expectation: expectation(obj, issues),
            }))
        }
        "turn" => {
            reject_unknown_keys(
                obj,
                &[
                    "type",
                    "say",
                    "language",
                    "context",
                    "reference_candidates",
                    "resolver_context",
                    "hint",
                    "seed",
                    "unix_time_ms",
                    "expectation",
                ],
                path,
                issues,
            );
            Some(ScenarioStep::Turn(ScenarioTurnStep {
                say: required_string(obj, "say", path, limits, issues)?,
                language: optional_source_string(obj, "language", path, limits, issues),
                context: context(obj, issues),
                reference_candidates: parse_reference_candidates(
                    obj.get("reference_candidates"),
                    &format!("{path}.reference_candidates"),
                    limits,
                    issues,
                ),
                resolver_context: parse_value_map(
                    obj.get("resolver_context"),
                    &format!("{path}.resolver_context"),
                    issues,
                ),
                hint: parse_hint(obj.get("hint"), &format!("{path}.hint"), issues),
                seed: optional_u64_strict(obj, "seed", path, issues),
                unix_time_ms: optional_i64_strict(obj, "unix_time_ms", path, issues),
                expectation: expectation(obj, issues),
            }))
        }
        "capability_result" => {
            reject_unknown_keys(
                obj,
                &[
                    "type",
                    "proposal_from_step",
                    "proposal_capability",
                    "proposal_ordinal",
                    "succeeded",
                    "output",
                    "error_code",
                    "language",
                    "context",
                    "seed",
                    "unix_time_ms",
                    "expectation",
                ],
                path,
                issues,
            );
            Some(ScenarioStep::CapabilityResult(
                ScenarioCapabilityResultStep {
                    proposal_from_step: required_step_ref(obj, "proposal_from_step", path, issues)?,
                    proposal_capability: optional_source_id(
                        obj,
                        "proposal_capability",
                        CapabilityId::new,
                        path,
                        limits,
                        issues,
                    ),
                    proposal_ordinal: optional_step_ordinal(obj, "proposal_ordinal", path, issues),
                    succeeded: required_bool(obj, "succeeded", path, issues)?,
                    output: match obj.get("output") {
                        None => None,
                        Some(value) => parse_model_value(value, &format!("{path}.output"), issues),
                    },
                    error_code: optional_source_string(obj, "error_code", path, limits, issues),
                    language: optional_source_string(obj, "language", path, limits, issues),
                    context: context(obj, issues),
                    seed: optional_u64_strict(obj, "seed", path, issues),
                    unix_time_ms: optional_i64_strict(obj, "unix_time_ms", path, issues),
                    expectation: expectation(obj, issues),
                },
            ))
        }
        "confirm" => {
            reject_unknown_keys(
                obj,
                &[
                    "type",
                    "proposal_from_step",
                    "proposal_capability",
                    "proposal_ordinal",
                    "confirmed",
                    "context",
                    "unix_time_ms",
                    "expectation",
                ],
                path,
                issues,
            );
            Some(ScenarioStep::Confirm(ScenarioConfirmStep {
                proposal_from_step: required_step_ref(obj, "proposal_from_step", path, issues)?,
                proposal_capability: optional_source_id(
                    obj,
                    "proposal_capability",
                    CapabilityId::new,
                    path,
                    limits,
                    issues,
                ),
                proposal_ordinal: optional_step_ordinal(obj, "proposal_ordinal", path, issues),
                confirmed: required_bool(obj, "confirmed", path, issues)?,
                context: context(obj, issues),
                unix_time_ms: optional_i64_strict(obj, "unix_time_ms", path, issues),
                expectation: expectation(obj, issues),
            }))
        }
        other => {
            issues.push(issue(
                "source.scenario_step_type",
                &format!("unsupported scenario step type {other:?}"),
                Some(path),
            ));
            None
        }
    }
}

fn parse_hint(value: Option<&JsonValue>, path: &str, issues: &mut Vec<SourceIssue>) -> HintRequest {
    let Some(value) = value else {
        return HintRequest::None;
    };
    if value.is_null() {
        return HintRequest::None;
    }
    let Ok(obj) = expect_object(value, path, issues) else {
        return HintRequest::None;
    };
    reject_unknown_keys(obj, &["type", "level"], path, issues);
    let kind = required_raw_string(obj, "type", path, issues).unwrap_or_else(|| "none".to_owned());
    match kind.as_str() {
        "none" => HintRequest::None,
        "first" => HintRequest::First,
        "next" => HintRequest::Next,
        "auto" => HintRequest::Auto,
        "direct" => required_u32_strict(obj, "level", path, issues)
            .map(HintRequest::Direct)
            .unwrap_or(HintRequest::None),
        other => {
            issues.push(issue(
                "source.hint_type",
                &format!("unsupported hint type {other:?}"),
                Some(path),
            ));
            HintRequest::None
        }
    }
}

fn parse_reference_candidates(
    value: Option<&JsonValue>,
    path: &str,
    limits: SourceLimits,
    issues: &mut Vec<SourceIssue>,
) -> Vec<ResolverReferenceCandidate> {
    optional_array(value, path, issues).map_or_else(Vec::new, |rows| {
        rows.iter()
            .enumerate()
            .filter_map(|(index, row)| {
                let row_path = format!("{path}[{index}]");
                let obj = expect_object(row, &row_path, issues).ok()?;
                reject_unknown_keys(obj, &["reference", "label", "aliases"], &row_path, issues);
                let reference_value = obj.get("reference")?;
                let reference_obj =
                    expect_object(reference_value, &format!("{row_path}.reference"), issues)
                        .ok()?;
                reject_unknown_keys(
                    reference_obj,
                    &["kind", "id"],
                    &format!("{row_path}.reference"),
                    issues,
                );
                Some(ResolverReferenceCandidate {
                    reference: HostReference {
                        kind: ReferenceKind::new(required_string(
                            reference_obj,
                            "kind",
                            &format!("{row_path}.reference"),
                            limits,
                            issues,
                        )?),
                        id: ReferenceId::new(required_string(
                            reference_obj,
                            "id",
                            &format!("{row_path}.reference"),
                            limits,
                            issues,
                        )?),
                    },
                    label: optional_source_string(obj, "label", &row_path, limits, issues),
                    aliases: string_array(
                        obj.get("aliases"),
                        &format!("{row_path}.aliases"),
                        limits,
                        issues,
                    ),
                })
            })
            .collect()
    })
}

fn required_step_ref(
    obj: &serde_json::Map<String, JsonValue>,
    key: &str,
    path: &str,
    issues: &mut Vec<SourceIssue>,
) -> Option<usize> {
    required_u32_strict(obj, key, path, issues).and_then(|value| {
        if value == 0 {
            issues.push(issue(
                "source.scenario_step_ref",
                &format!("{key} must be a one-based positive step number"),
                Some(path),
            ));
            None
        } else {
            Some(value as usize)
        }
    })
}

fn optional_step_ordinal(
    obj: &serde_json::Map<String, JsonValue>,
    key: &str,
    path: &str,
    issues: &mut Vec<SourceIssue>,
) -> Option<usize> {
    let value = optional_u64_strict(obj, key, path, issues)?;
    if value == 0 {
        issues.push(issue(
            "source.scenario_proposal_ordinal",
            &format!("{key} must be a one-based positive ordinal"),
            Some(path),
        ));
        None
    } else if value > usize::MAX as u64 {
        issues.push(issue(
            "source.scenario_proposal_ordinal",
            &format!("{key} exceeds the supported platform range"),
            Some(path),
        ));
        None
    } else {
        Some(value as usize)
    }
}

fn required_bool(
    obj: &serde_json::Map<String, JsonValue>,
    key: &str,
    path: &str,
    issues: &mut Vec<SourceIssue>,
) -> Option<bool> {
    match obj.get(key) {
        Some(JsonValue::Bool(value)) => Some(*value),
        None => {
            issues.push(issue(
                "source.field_required",
                &format!("{key} is required"),
                Some(path),
            ));
            None
        }
        Some(_) => {
            issues.push(issue(
                "source.bool_required",
                &format!("{key} must be boolean"),
                Some(path),
            ));
            None
        }
    }
}

pub(super) fn parse_expectation(
    value: Option<&JsonValue>,
    path: &str,
    limits: SourceLimits,
    issues: &mut Vec<SourceIssue>,
) -> TurnExpectation {
    let Some(value) = value else {
        return TurnExpectation::default();
    };
    if value.is_null() {
        return TurnExpectation::default();
    }
    let Ok(obj) = expect_object(value, path, issues) else {
        return TurnExpectation::default();
    };
    reject_unknown_keys(
        obj,
        &[
            "meaning",
            "forbidden_meanings",
            "meaning_slots",
            "meaning_references",
            "min_semantic_score",
            "conversation_mode",
            "response_ids",
            "forbidden_response_ids",
            "response_contains",
            "response_not_contains",
            "author_values",
            "conversation_values",
            "active_topic",
            "active_followup",
            "capabilities",
            "proposal_receipts",
            "forbidden_capabilities",
            "capability_result_accepted",
            "capability_result_reason_code",
            "why_codes",
            "forbidden_why_codes",
        ],
        path,
        issues,
    );
    TurnExpectation {
        meaning: optional_source_id(obj, "meaning", MeaningId::new, path, limits, issues),
        forbidden_meanings: string_array(
            obj.get("forbidden_meanings"),
            &format!("{path}.forbidden_meanings"),
            limits,
            issues,
        )
        .into_iter()
        .map(MeaningId::new)
        .collect(),
        meaning_slots: parse_value_map(
            obj.get("meaning_slots"),
            &format!("{path}.meaning_slots"),
            issues,
        ),
        meaning_references: parse_host_references(
            obj.get("meaning_references"),
            &format!("{path}.meaning_references"),
            issues,
        ),
        min_semantic_score: optional_f64_strict(obj, "min_semantic_score", path, issues),
        conversation_mode: optional_source_string(obj, "conversation_mode", path, limits, issues),
        response_ids: string_array(
            obj.get("response_ids"),
            &format!("{path}.response_ids"),
            limits,
            issues,
        )
        .into_iter()
        .map(ResponseId::new)
        .collect(),
        forbidden_response_ids: string_array(
            obj.get("forbidden_response_ids"),
            &format!("{path}.forbidden_response_ids"),
            limits,
            issues,
        )
        .into_iter()
        .map(ResponseId::new)
        .collect(),
        response_contains: string_array(
            obj.get("response_contains"),
            &format!("{path}.response_contains"),
            limits,
            issues,
        ),
        response_not_contains: string_array(
            obj.get("response_not_contains"),
            &format!("{path}.response_not_contains"),
            limits,
            issues,
        ),
        author_values: parse_value_map(
            obj.get("author_values"),
            &format!("{path}.author_values"),
            issues,
        ),
        conversation_values: parse_value_map(
            obj.get("conversation_values"),
            &format!("{path}.conversation_values"),
            issues,
        ),
        active_topic: optional_source_id(obj, "active_topic", TopicId::new, path, limits, issues),
        active_followup: optional_source_id(
            obj,
            "active_followup",
            FollowupId::new,
            path,
            limits,
            issues,
        ),
        capabilities: parse_expected_capabilities(
            obj.get("capabilities"),
            &format!("{path}.capabilities"),
            issues,
        ),
        proposal_receipts: parse_expected_proposal_receipts(
            obj.get("proposal_receipts"),
            &format!("{path}.proposal_receipts"),
            issues,
        ),
        forbidden_capabilities: string_array(
            obj.get("forbidden_capabilities"),
            &format!("{path}.forbidden_capabilities"),
            limits,
            issues,
        )
        .into_iter()
        .map(CapabilityId::new)
        .collect(),
        capability_result_accepted: match obj.get("capability_result_accepted") {
            None | Some(JsonValue::Null) => None,
            Some(JsonValue::Bool(value)) => Some(*value),
            Some(_) => {
                issues.push(issue(
                    "source.bool",
                    "capability_result_accepted must be boolean or null",
                    Some(&format!("{path}.capability_result_accepted")),
                ));
                None
            }
        },
        capability_result_reason_code: optional_source_string(
            obj,
            "capability_result_reason_code",
            path,
            limits,
            issues,
        ),
        why_codes: string_array(
            obj.get("why_codes"),
            &format!("{path}.why_codes"),
            limits,
            issues,
        )
        .into_iter()
        .map(TraceCode::new)
        .collect(),
        forbidden_why_codes: string_array(
            obj.get("forbidden_why_codes"),
            &format!("{path}.forbidden_why_codes"),
            limits,
            issues,
        )
        .into_iter()
        .map(TraceCode::new)
        .collect(),
    }
}

pub(super) fn parse_expected_proposal_receipts(
    value: Option<&JsonValue>,
    path: &str,
    issues: &mut Vec<SourceIssue>,
) -> Vec<ExpectedProposalReceipt> {
    let Some(rows) = optional_array(value, path, issues) else {
        return Vec::new();
    };
    rows.iter()
        .enumerate()
        .filter_map(|(index, row)| {
            let item_path = format!("{path}[{index}]");
            let obj = expect_object(row, &item_path, issues).ok()?;
            reject_unknown_keys(
                obj,
                &["id", "version", "arguments", "outcome", "reason_code"],
                &item_path,
                issues,
            );
            let outcome = match required_raw_string(obj, "outcome", &item_path, issues)?.as_str() {
                "admitted" => ExpectedProposalOutcome::Admitted,
                "needs_confirmation" => ExpectedProposalOutcome::NeedsConfirmation,
                "rejected" => ExpectedProposalOutcome::Rejected,
                other => {
                    issues.push(issue(
                        "source.proposal_outcome",
                        &format!("unsupported proposal outcome {other:?}"),
                        Some(&format!("{item_path}.outcome")),
                    ));
                    return None;
                }
            };
            let reason_code = optional_raw_source_string(obj, "reason_code", &item_path, issues);
            if matches!(outcome, ExpectedProposalOutcome::Admitted) && reason_code.is_some() {
                issues.push(issue(
                    "source.proposal_reason",
                    "admitted proposal receipt cannot declare reason_code",
                    Some(&format!("{item_path}.reason_code")),
                ));
                return None;
            }
            let arguments = match obj.get("arguments") {
                None | Some(JsonValue::Null) => None,
                Some(value) => Some(parse_value_map(
                    Some(value),
                    &format!("{item_path}.arguments"),
                    issues,
                )),
            };
            Some(ExpectedProposalReceipt {
                id: CapabilityId::new(required_raw_string(obj, "id", &item_path, issues)?),
                version: optional_raw_source_string(obj, "version", &item_path, issues)
                    .map(CapabilityVersion::new),
                arguments,
                outcome,
                reason_code,
            })
        })
        .collect()
}

pub(super) fn parse_expected_capabilities(
    value: Option<&JsonValue>,
    path: &str,
    issues: &mut Vec<SourceIssue>,
) -> Vec<ExpectedCapability> {
    let Some(rows) = optional_array(value, path, issues) else {
        return Vec::new();
    };
    rows.iter()
        .enumerate()
        .filter_map(|(index, row)| {
            let item_path = format!("{path}[{index}]");
            let obj = expect_object(row, &item_path, issues).ok()?;
            reject_unknown_keys(obj, &["id", "version", "arguments"], &item_path, issues);
            let arguments = match obj.get("arguments") {
                None | Some(JsonValue::Null) => None,
                Some(value) => Some(parse_value_map(
                    Some(value),
                    &format!("{item_path}.arguments"),
                    issues,
                )),
            };
            Some(ExpectedCapability {
                id: CapabilityId::new(required_raw_string(obj, "id", &item_path, issues)?),
                version: optional_raw_source_string(obj, "version", &item_path, issues)
                    .map(CapabilityVersion::new),
                arguments,
            })
        })
        .collect()
}

pub(super) fn parse_context(
    value: Option<&JsonValue>,
    path: &str,
    issues: &mut Vec<SourceIssue>,
) -> ContextSnapshot {
    let Some(value) = value else {
        return empty_context();
    };
    if value.is_null() {
        return empty_context();
    }
    let Ok(obj) = expect_object(value, path, issues) else {
        return empty_context();
    };
    reject_unknown_keys(
        obj,
        &["values", "visible_references", "available_capabilities"],
        path,
        issues,
    );
    let values = parse_value_map(obj.get("values"), &format!("{path}.values"), issues);
    let visible_references = parse_host_references(
        obj.get("visible_references"),
        &format!("{path}.visible_references"),
        issues,
    );
    let available_capabilities = optional_array(
        obj.get("available_capabilities"),
        &format!("{path}.available_capabilities"),
        issues,
    )
    .map_or_else(Vec::new, |rows| {
        rows.iter()
            .enumerate()
            .filter_map(|(index, row)| {
                let item_path = format!("{path}.available_capabilities[{index}]");
                let cap = expect_object(row, &item_path, issues).ok()?;
                reject_unknown_keys(cap, &["id", "version"], &item_path, issues);
                Some(AvailableCapability {
                    id: CapabilityId::new(required_raw_string(cap, "id", &item_path, issues)?),
                    version: CapabilityVersion::new(required_raw_string(
                        cap, "version", &item_path, issues,
                    )?),
                })
            })
            .collect()
    });
    ContextSnapshot {
        values,
        visible_references,
        available_capabilities,
    }
}

pub(super) fn empty_context() -> ContextSnapshot {
    ContextSnapshot {
        values: BTreeMap::new(),
        visible_references: Vec::new(),
        available_capabilities: Vec::new(),
    }
}

pub(super) fn parse_host_references(
    value: Option<&JsonValue>,
    path: &str,
    issues: &mut Vec<SourceIssue>,
) -> Vec<HostReference> {
    let Some(rows) = optional_array(value, path, issues) else {
        return Vec::new();
    };
    rows.iter()
        .enumerate()
        .filter_map(|(index, row)| {
            let item_path = format!("{path}[{index}]");
            let obj = expect_object(row, &item_path, issues).ok()?;
            reject_unknown_keys(obj, &["kind", "id"], &item_path, issues);
            Some(HostReference {
                kind: ReferenceKind::new(required_raw_string(obj, "kind", &item_path, issues)?),
                id: ReferenceId::new(required_raw_string(obj, "id", &item_path, issues)?),
            })
        })
        .collect()
}

pub(super) fn parse_state(
    value: Option<&JsonValue>,
    path: &str,
    issues: &mut Vec<SourceIssue>,
) -> GvyaState {
    let Some(value) = value else {
        return GvyaState::default();
    };
    if value.is_null() {
        return GvyaState::default();
    }
    let Ok(obj) = expect_object(value, path, issues) else {
        return GvyaState::default();
    };
    reject_unknown_keys(obj, &["author", "conversation"], path, issues);
    let author = parse_value_map(obj.get("author"), &format!("{path}.author"), issues);
    let mut conversation = gvya_model::ConversationState::default();
    let conversation_path = format!("{path}.conversation");
    if let Some(value) = obj.get("conversation") {
        if !value.is_null() {
            if let Ok(c) = expect_object(value, &conversation_path, issues) {
                reject_unknown_keys(
                    c,
                    &[
                        "active_topic",
                        "active_followup",
                        "last_meaning",
                        "last_behavior",
                        "last_topic",
                        "mentioned_topics",
                        "recent_response_ids",
                        "recent_variant_keys",
                        "recent_user_messages",
                        "repeat_fallback_serial",
                        "repeat_memory",
                        "repair",
                        "hint_progress",
                        "focus",
                        "user_style",
                        "turn_index",
                    ],
                    &conversation_path,
                    issues,
                );
                conversation.active_topic = parse_active_topic(
                    c.get("active_topic"),
                    &format!("{conversation_path}.active_topic"),
                    issues,
                );
                conversation.active_followup = parse_active_followup(
                    c.get("active_followup"),
                    &format!("{conversation_path}.active_followup"),
                    issues,
                );
                conversation.last_meaning =
                    optional_raw_source_string(c, "last_meaning", &conversation_path, issues)
                        .map(MeaningId::new);
                conversation.last_behavior =
                    optional_raw_source_string(c, "last_behavior", &conversation_path, issues)
                        .map(BehaviorId::new);
                conversation.last_topic =
                    optional_raw_source_string(c, "last_topic", &conversation_path, issues)
                        .map(TopicId::new);
                conversation.mentioned_topics = strict_raw_string_array(
                    c.get("mentioned_topics"),
                    &format!("{conversation_path}.mentioned_topics"),
                    issues,
                )
                .into_iter()
                .map(TopicId::new)
                .collect();
                conversation.recent_response_ids = strict_raw_string_array(
                    c.get("recent_response_ids"),
                    &format!("{conversation_path}.recent_response_ids"),
                    issues,
                )
                .into_iter()
                .map(ResponseId::new)
                .collect();
                conversation.recent_variant_keys = strict_raw_string_array(
                    c.get("recent_variant_keys"),
                    &format!("{conversation_path}.recent_variant_keys"),
                    issues,
                );
                conversation.recent_user_messages = strict_raw_string_array(
                    c.get("recent_user_messages"),
                    &format!("{conversation_path}.recent_user_messages"),
                    issues,
                );
                conversation.repeat_fallback_serial =
                    optional_u64_strict(c, "repeat_fallback_serial", &conversation_path, issues)
                        .unwrap_or(0);
                conversation.repeat_memory = parse_repeat_memory(
                    c.get("repeat_memory"),
                    &format!("{conversation_path}.repeat_memory"),
                    issues,
                );
                conversation.repair = parse_repair_memory(
                    c.get("repair"),
                    &format!("{conversation_path}.repair"),
                    issues,
                );
                conversation.hint_progress = parse_u32_map(
                    c.get("hint_progress"),
                    &format!("{conversation_path}.hint_progress"),
                    issues,
                );
                conversation.focus = parse_host_references(
                    c.get("focus"),
                    &format!("{conversation_path}.focus"),
                    issues,
                );
                conversation.user_style = parse_user_style(
                    c.get("user_style"),
                    &format!("{conversation_path}.user_style"),
                    issues,
                );
                conversation.turn_index =
                    optional_u64_strict(c, "turn_index", &conversation_path, issues).unwrap_or(0);
            }
        }
    }
    GvyaState {
        author,
        conversation,
    }
}

pub(super) fn parse_repeat_memory(
    value: Option<&JsonValue>,
    path: &str,
    issues: &mut Vec<SourceIssue>,
) -> RepeatMemory {
    let Some(value) = value else {
        return RepeatMemory::default();
    };
    if value.is_null() {
        return RepeatMemory::default();
    }
    let Ok(obj) = expect_object(value, path, issues) else {
        return RepeatMemory::default();
    };
    reject_unknown_keys(
        obj,
        &[
            "last_user_normalized",
            "last_meaning",
            "same_input_count",
            "same_meaning_count",
        ],
        path,
        issues,
    );
    RepeatMemory {
        last_user_normalized: optional_raw_source_string(obj, "last_user_normalized", path, issues)
            .unwrap_or_default(),
        last_meaning: optional_raw_source_string(obj, "last_meaning", path, issues)
            .map(MeaningId::new),
        same_input_count: optional_u32_strict(obj, "same_input_count", path, issues).unwrap_or(0),
        same_meaning_count: optional_u32_strict(obj, "same_meaning_count", path, issues)
            .unwrap_or(0),
    }
}

pub(super) fn parse_repair_memory(
    value: Option<&JsonValue>,
    path: &str,
    issues: &mut Vec<SourceIssue>,
) -> RepairMemory {
    let Some(value) = value else {
        return RepairMemory::default();
    };
    if value.is_null() {
        return RepairMemory::default();
    }
    let Ok(obj) = expect_object(value, path, issues) else {
        return RepairMemory::default();
    };
    reject_unknown_keys(
        obj,
        &["consecutive", "last_mode", "last_candidate"],
        path,
        issues,
    );
    RepairMemory {
        consecutive: optional_u32_strict(obj, "consecutive", path, issues).unwrap_or(0),
        last_mode: optional_raw_source_string(obj, "last_mode", path, issues),
        last_candidate: optional_raw_source_string(obj, "last_candidate", path, issues)
            .map(MeaningId::new),
    }
}

pub(super) fn parse_user_style(
    value: Option<&JsonValue>,
    path: &str,
    issues: &mut Vec<SourceIssue>,
) -> UserStyle {
    let Some(value) = value else {
        return UserStyle::default();
    };
    if value.is_null() {
        return UserStyle::default();
    }
    let Ok(obj) = expect_object(value, path, issues) else {
        return UserStyle::default();
    };
    reject_unknown_keys(obj, &["formality", "confidence"], path, issues);
    let formality = match optional_raw_source_string(obj, "formality", path, issues)
        .as_deref()
        .unwrap_or("unknown")
    {
        "unknown" => Formality::Unknown,
        "formal" => Formality::Formal,
        "informal" => Formality::Informal,
        _ => {
            issues.push(issue(
                "source.formality",
                "formality must be unknown, formal, or informal",
                Some(path),
            ));
            Formality::Unknown
        }
    };
    let confidence = optional_f64_strict(obj, "confidence", path, issues).unwrap_or(0.0);
    UserStyle {
        formality,
        confidence,
    }
}

pub(super) fn parse_active_topic(
    value: Option<&JsonValue>,
    path: &str,
    issues: &mut Vec<SourceIssue>,
) -> Option<ActiveTopic> {
    let value = value?;
    if value.is_null() {
        return None;
    }
    let obj = expect_object(value, path, issues).ok()?;
    reject_unknown_keys(obj, &["id", "ttl", "source_behavior"], path, issues);
    Some(ActiveTopic {
        id: TopicId::new(required_raw_string(obj, "id", path, issues)?),
        ttl: required_u32_strict(obj, "ttl", path, issues)?,
        source_behavior: optional_raw_source_string(obj, "source_behavior", path, issues)
            .map(BehaviorId::new),
    })
}

pub(super) fn parse_active_followup(
    value: Option<&JsonValue>,
    path: &str,
    issues: &mut Vec<SourceIssue>,
) -> Option<ActiveFollowup> {
    let value = value?;
    if value.is_null() {
        return None;
    }
    let obj = expect_object(value, path, issues).ok()?;
    reject_unknown_keys(obj, &["id", "ttl", "source_behavior"], path, issues);
    Some(ActiveFollowup {
        id: FollowupId::new(required_raw_string(obj, "id", path, issues)?),
        ttl: required_u32_strict(obj, "ttl", path, issues)?,
        source_behavior: optional_raw_source_string(obj, "source_behavior", path, issues)
            .map(BehaviorId::new),
    })
}

pub(super) fn parse_value_map(
    value: Option<&JsonValue>,
    path: &str,
    issues: &mut Vec<SourceIssue>,
) -> BTreeMap<String, Value> {
    let Some(value) = value else {
        return BTreeMap::new();
    };
    if value.is_null() {
        return BTreeMap::new();
    }
    let Some(map) = value.as_object() else {
        issues.push(issue(
            "source.expected_object",
            "expected JSON object",
            Some(path),
        ));
        return BTreeMap::new();
    };
    map.iter()
        .filter_map(|(key, value)| {
            parse_model_value(value, &format!("{path}.{key}"), issues)
                .map(|value| (key.clone(), value))
        })
        .collect()
}

pub(super) fn parse_u32_map(
    value: Option<&JsonValue>,
    path: &str,
    issues: &mut Vec<SourceIssue>,
) -> BTreeMap<String, u32> {
    let Some(value) = value else {
        return BTreeMap::new();
    };
    if value.is_null() {
        return BTreeMap::new();
    }
    let Some(map) = value.as_object() else {
        issues.push(issue(
            "source.expected_object",
            "expected JSON object",
            Some(path),
        ));
        return BTreeMap::new();
    };
    map.iter()
        .filter_map(|(key, value)| {
            match value.as_u64().and_then(|value| u32::try_from(value).ok()) {
                Some(value) => Some((key.clone(), value)),
                None => {
                    issues.push(issue(
                        "source.u32",
                        "map value must be an unsigned 32-bit integer",
                        Some(&format!("{path}.{key}")),
                    ));
                    None
                }
            }
        })
        .collect()
}
