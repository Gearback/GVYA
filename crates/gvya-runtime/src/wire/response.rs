//! Runtime wire response/info serialization.

use super::*;

pub fn serialize_turn_result(output: &RuntimeInteractionOutput) -> Result<Vec<u8>, WireError> {
    serialize_turn_result_with_limits(output, WireLimits::default())
}

pub fn serialize_turn_result_with_limits(
    output: &RuntimeInteractionOutput,
    limits: WireLimits,
) -> Result<Vec<u8>, WireError> {
    validate_interaction_output(output)?;
    serialize_bounded(&turn_result_json(output), limits)
}

pub(crate) fn validate_turn_result_with_limits(
    output: &RuntimeInteractionOutput,
    limits: WireLimits,
) -> Result<(), WireError> {
    serialize_turn_result_with_limits(output, limits).map(|_| ())
}

pub(super) fn turn_result_json(output: &RuntimeInteractionOutput) -> JsonValue {
    let semantic = output.conversation.semantic.as_ref().map(|analysis| {
        json!({
            "candidate_pruning_reason": analysis.candidate_pruning_reason,
            "candidate_pruning_used": analysis.candidate_pruning_used,
            "decision": semantic_decision(&analysis.decision),
            "structural_match": analysis.structural_match.as_ref().map(|row| json!({
                "meaning": row.meaning.as_str(),
                "pattern": row.pattern,
                "language": row.language,
                "priority": row.priority,
                "captures": row.captures,
            })),
            "scores": analysis.scored.iter().take(32).map(|row| json!({
                "meaning": row.meaning.as_str(),
                "pattern_index": row.pattern_index,
                "score": row.score,
                "priority": row.priority,
                "retrieval_rank_milli": row.retrieval_rank_milli,
                "evidence_tier": row.breakdown.evidence_tier,
                "evidence_strength": row.breakdown.evidence_strength,
                "match_kind": format!("{:?}", row.breakdown.match_kind).to_lowercase(),
                "match_view": row.breakdown.match_view,
                "rejected_reason": row.breakdown.rejected_reason,
            })).collect::<Vec<_>>(),
        })
    });
    let mut traces = Vec::new();
    if let Some(analysis) = &output.conversation.semantic {
        traces.push(trace_json(&analysis.trace));
    }
    traces.push(trace_json(&output.conversation.trace));
    traces.push(trace_json(&output.capabilities.trace));
    json!({
        "format": TURN_RESPONSE_FORMAT,
        "version": WIRE_VERSION,
        "mode": output.conversation.mode.label(),
        "meaning": output.conversation.meaning.as_ref().map(meaning_json),
        "behavior": output.conversation.behavior.as_ref().map(|value| value.as_str()),
        "response": response_json(&output.conversation.response),
        "state": state_json(&output.conversation.state),
        "capabilities": capability_evaluation_json(&output.capabilities),
        "why": why_json(&output.why),
        "semantic": semantic,
        "traces": traces,
    })
}

pub fn serialize_runtime_info(runtime: &Runtime) -> Result<Vec<u8>, WireError> {
    let trust = match runtime.trust() {
        TrustStatus::Unsigned => json!({"status":"unsigned"}),
        TrustStatus::PresentUnverified { key_id, algorithm } => {
            json!({"status":"present_unverified","key_id":key_id,"algorithm":algorithm})
        }
        TrustStatus::Verified { key_id, algorithm } => {
            json!({"status":"verified","key_id":key_id,"algorithm":algorithm})
        }
    };
    serialize_bounded(
        &json!({
            "format": RUNTIME_INFO_FORMAT,
            "version": WIRE_VERSION,
            "project_id": runtime.project_id(),
            "brain_id": runtime.brain_id(),
            "enabled_languages": runtime.enabled_languages(),
            "default_language": runtime.default_language(),
            "artifact_sha256": runtime.artifact_digest(),
            "content_root": runtime.content_root().iter().map(|byte| format!("{byte:02x}")).collect::<Vec<_>>().join(""),
            "trust": trust,
        }),
        WireLimits::default(),
    )
}

/// Serialize the capability contracts embedded in the loaded artifact. Declared contracts are
/// discoverable, but this endpoint does not mark any capability as host-available.
pub fn serialize_capabilities(runtime: &Runtime) -> Result<Vec<u8>, WireError> {
    // Discovery stays intentionally lightweight: full schemas/descriptions/host-effect detail are
    // fetched one capability at a time through `serialize_capability_info`. This prevents catalog
    // discovery from duplicating the entire embedded schema corpus before the response byte budget
    // can reject an oversized payload.
    let capabilities = runtime
        .capability_ids()
        .filter_map(|id| runtime.capability_definition(id))
        .map(capability_summary_json)
        .collect::<Vec<_>>();
    serialize_bounded(
        &json!({
            "format": CAPABILITIES_INFO_FORMAT,
            "version": WIRE_VERSION,
            "capabilities": capabilities,
        }),
        runtime.limits(),
    )
}

pub(super) fn capability_summary_json(definition: &CapabilityDefinition) -> JsonValue {
    let contract = &definition.contract;
    json!({
        "id": contract.id.as_str(),
        "version": contract.version.as_str(),
        "title": contract.title,
        "effect_class": effect_class_label(contract.effect_class),
        "confirmation_hint": confirmation_hint_label(contract.confirmation_hint),
    })
}

pub fn serialize_capability_info(
    runtime: &Runtime,
    id: &CapabilityId,
) -> Result<Option<Vec<u8>>, WireError> {
    let Some(definition) = runtime.capability_definition(id) else {
        return Ok(None);
    };
    serialize_bounded(
        &json!({
            "format": CAPABILITY_INFO_FORMAT,
            "version": WIRE_VERSION,
            "capability": capability_definition_json(definition),
        }),
        runtime.limits(),
    )
    .map(Some)
}

pub(super) fn capability_definition_json(definition: &CapabilityDefinition) -> JsonValue {
    let contract = &definition.contract;
    json!({
        "id": contract.id.as_str(),
        "version": contract.version.as_str(),
        "title": contract.title,
        "description": contract.description,
        "input_schema": parse_schema_document(contract.input_schema.as_str()),
        "output_schema": contract.output_schema.as_ref().map(|schema| parse_schema_document(schema.as_str())),
        "reference_kinds": contract.reference_kinds.iter().map(|kind| kind.as_str()).collect::<Vec<_>>(),
        "effect_class": effect_class_label(contract.effect_class),
        "confirmation_hint": confirmation_hint_label(contract.confirmation_hint),
        "host_effects": definition.host_effects.iter().map(|effect| json!({
            "resource": effect.resource,
            "kind": host_effect_kind_label(effect.kind),
            "summary": effect.summary,
        })).collect::<Vec<_>>(),
    })
}

pub(super) fn parse_schema_document(source: &str) -> JsonValue {
    // Compiler hydration already validated the canonical schema document. Keep introspection
    // structurally typed for hosts; a defensive fallback avoids turning read-only inspection into
    // a runtime panic if an impossible internal invariant is violated.
    serde_json::from_str(source).unwrap_or_else(|_| JsonValue::String(source.to_owned()))
}

pub(super) fn effect_class_label(value: EffectClass) -> &'static str {
    match value {
        EffectClass::Pure => "pure",
        EffectClass::Reversible => "reversible",
        EffectClass::Irreversible => "irreversible",
        EffectClass::External => "external",
    }
}

pub(super) fn confirmation_hint_label(value: ConfirmationHint) -> &'static str {
    match value {
        ConfirmationHint::Never => "never",
        ConfirmationHint::Conditional => "conditional",
        ConfirmationHint::Always => "always",
    }
}

pub(super) fn host_effect_kind_label(value: HostEffectKind) -> &'static str {
    match value {
        HostEffectKind::Read => "read",
        HostEffectKind::Update => "update",
        HostEffectKind::Create => "create",
        HostEffectKind::Delete => "delete",
        HostEffectKind::External => "external",
    }
}

pub fn serialize_asset_info(asset: &RuntimeAsset<'_>) -> Result<Vec<u8>, WireError> {
    serialize_bounded(
        &json!({
            "format": ASSET_INFO_FORMAT,
            "version": WIRE_VERSION,
            "id": asset.id.as_str(),
            "media_type": asset.media_type,
            "logical_path": asset.logical_path,
            "sha256": asset.digest,
            "size": asset.bytes.len(),
        }),
        WireLimits::default(),
    )
}

pub fn serialize_capability_result_result(
    value: &RuntimeCapabilityResultOutput,
) -> Result<Vec<u8>, WireError> {
    serialize_capability_result_result_with_limits(value, WireLimits::default())
}

pub(crate) fn validate_capability_result_result_with_limits(
    value: &RuntimeCapabilityResultOutput,
    limits: WireLimits,
) -> Result<(), WireError> {
    serialize_capability_result_result_with_limits(value, limits).map(|_| ())
}

pub fn serialize_capability_result_result_with_limits(
    value: &RuntimeCapabilityResultOutput,
    limits: WireLimits,
) -> Result<Vec<u8>, WireError> {
    validate_trace(&value.validation.trace)?;
    if let Some(interaction) = &value.interaction {
        validate_interaction_output(interaction)?;
    }
    serialize_bounded(
        &json!({
            "format": CAPABILITY_RESULT_RESPONSE_FORMAT,
            "version": WIRE_VERSION,
            "validation": {
                "accepted": value.validation.accepted,
                "reason_code": value.validation.reason_code,
                "trace": trace_json(&value.validation.trace),
            },
            "interaction": value.interaction.as_ref().map(turn_result_json),
            "why": why_json(&value.why),
        }),
        limits,
    )
}
