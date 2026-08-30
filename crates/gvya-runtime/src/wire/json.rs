//! Runtime/model JSON projection and output validation.

use super::*;

pub(super) fn semantic_decision(value: &gvya_kernel::semantic::SemanticDecision) -> JsonValue {
    match value {
        gvya_kernel::semantic::SemanticDecision::Resolved { meaning, source } => {
            json!({"type":"resolved","meaning":meaning_json(meaning),"source":match source { gvya_kernel::semantic::ResolutionSource::StructuralPattern=>"structural_pattern", gvya_kernel::semantic::ResolutionSource::Deterministic=>"deterministic", gvya_kernel::semantic::ResolutionSource::ResolverProposal=>"resolver_proposal" }})
        }
        gvya_kernel::semantic::SemanticDecision::Partial { partial, source } => {
            json!({"type":"partial","meaning":meaning_json(&partial.meaning),"missing_required_values":partial.missing_required_values.iter().map(missing_required_value_json).collect::<Vec<_>>(),"source":match source { gvya_kernel::semantic::ResolutionSource::StructuralPattern=>"structural_pattern", gvya_kernel::semantic::ResolutionSource::Deterministic=>"deterministic", gvya_kernel::semantic::ResolutionSource::ResolverProposal=>"resolver_proposal" }})
        }
        gvya_kernel::semantic::SemanticDecision::Ambiguous {
            candidates,
            reason_code,
        } => {
            json!({"type":"ambiguous","candidates":candidates.iter().map(|id|id.as_str()).collect::<Vec<_>>(),"reason_code":reason_code})
        }
        gvya_kernel::semantic::SemanticDecision::Unresolved {
            reason_code,
            best_score,
        } => json!({"type":"unresolved","reason_code":reason_code,"best_score":best_score}),
    }
}
fn missing_required_value_json(value: &MissingRequiredValue) -> JsonValue {
    match value {
        MissingRequiredValue::Slot { name } => json!({"type":"slot","name":name}),
        MissingRequiredValue::Reference { kind } => {
            json!({"type":"reference","kind":kind.as_str()})
        }
    }
}
fn active_collection_json(value: &ActiveCollection) -> JsonValue {
    json!({"meaning":meaning_json(&value.meaning),"remaining":value.remaining.iter().map(missing_required_value_json).collect::<Vec<_>>(),"authority":match value.authority {CollectionAuthority::StructuralPattern=>"structural_pattern",CollectionAuthority::Deterministic=>"deterministic",CollectionAuthority::ResolverProposal=>"resolver_proposal"},"started_turn":value.started_turn})
}
pub(super) fn meaning_json(value: &Meaning) -> JsonValue {
    json!({"id":value.id.as_str(),"slots":value.slots.iter().map(|slot|json!({"name":slot.name,"value":json_value(&slot.value),"provenance":provenance_label(&slot.provenance)})).collect::<Vec<_>>(),"references":value.references.iter().map(reference_json).collect::<Vec<_>>()})
}
pub(super) fn reference_json(value: &HostReference) -> JsonValue {
    json!({"kind":value.kind.as_str(),"id":value.id.as_str()})
}
pub(super) fn response_json(value: &ResponsePlan) -> JsonValue {
    json!({"messages":value.messages.iter().map(|message|json!({"source_response":message.source_response.as_ref().map(|id|id.as_str()),"kind":message.kind,"items":message.items.iter().map(|item|match item {ResponseItem::Text{text,language}=>json!({"type":"text","text":text,"language":language}),ResponseItem::Asset{asset_id,alt_text}=>json!({"type":"asset","asset_id":asset_id.as_str(),"alt_text":alt_text}),ResponseItem::Link{label,url}=>json!({"type":"link","label":label,"url":url})}).collect::<Vec<_>>() })).collect::<Vec<_>>()})
}
pub(super) fn state_json(value: &GvyaState) -> JsonValue {
    json!({"author":map_json(&value.author),"conversation":{"active_topic":value.conversation.active_topic.as_ref().map(|row|json!({"id":row.id.as_str(),"ttl":row.ttl,"source_behavior":row.source_behavior.as_ref().map(|id|id.as_str())})),"active_followup":value.conversation.active_followup.as_ref().map(|row|json!({"id":row.id.as_str(),"ttl":row.ttl,"source_behavior":row.source_behavior.as_ref().map(|id|id.as_str())})),"active_collection":value.conversation.active_collection.as_ref().map(active_collection_json),"last_meaning":value.conversation.last_meaning.as_ref().map(|id|id.as_str()),"last_behavior":value.conversation.last_behavior.as_ref().map(|id|id.as_str()),"active_language":value.conversation.active_language,"last_topic":value.conversation.last_topic.as_ref().map(|id|id.as_str()),"mentioned_topics":value.conversation.mentioned_topics.iter().map(|id|id.as_str()).collect::<Vec<_>>(),"recent_response_ids":value.conversation.recent_response_ids.iter().map(|id|id.as_str()).collect::<Vec<_>>(),"recent_variant_keys":value.conversation.recent_variant_keys,"recent_user_messages":value.conversation.recent_user_messages,"repeat_fallback_serial":value.conversation.repeat_fallback_serial,"repeat_memory":{"last_user_normalized":value.conversation.repeat_memory.last_user_normalized,"last_meaning":value.conversation.repeat_memory.last_meaning.as_ref().map(|id|id.as_str()),"same_input_count":value.conversation.repeat_memory.same_input_count,"same_meaning_count":value.conversation.repeat_memory.same_meaning_count},"repair":{"consecutive":value.conversation.repair.consecutive,"last_mode":value.conversation.repair.last_mode,"last_candidate":value.conversation.repair.last_candidate.as_ref().map(|id|id.as_str())},"hint_progress":value.conversation.hint_progress,"focus":value.conversation.focus.iter().map(reference_json).collect::<Vec<_>>(),"user_style":{"formality":match value.conversation.user_style.formality{Formality::Unknown=>"unknown",Formality::Formal=>"formal",Formality::Informal=>"informal"},"confidence":value.conversation.user_style.confidence},"pending_capabilities":value.conversation.pending_capabilities.iter().map(invocation_proposal_json).collect::<Vec<_>>(),"turn_index":value.conversation.turn_index}})
}
pub(super) fn invocation_proposal_json(row: &InvocationProposal) -> JsonValue {
    json!({"id":row.id.as_str(),"capability":row.capability.as_str(),"capability_version":row.capability_version.as_str(),"arguments":map_json(&row.arguments),"fingerprint":row.fingerprint,"trace_id":row.trace_id.as_str()})
}
pub(super) fn capability_evaluation_json(value: &CapabilityEvaluation) -> JsonValue {
    json!({"decisions":value.decisions.iter().map(capability_decision_json).collect::<Vec<_>>(),"trace_id":value.trace.id.as_str()})
}
pub(super) fn capability_decision_json(value: &CapabilityDecision) -> JsonValue {
    json!({"source":{"binding_id":value.source.binding_id.as_str(),"response_id":value.source.response_id.as_ref().map(|id|id.as_str()),"message_index":value.source.message_index},"capability":value.capability.as_str(),"outcome":match &value.outcome { AdmissionOutcome::Admitted=>json!({"type":"admitted"}), AdmissionOutcome::NeedsConfirmation{reason_code}=>json!({"type":"needs_confirmation","reason_code":reason_code}), AdmissionOutcome::Rejected{reason_code}=>json!({"type":"rejected","reason_code":reason_code}) },"proposal":value.proposal.as_ref().map(invocation_proposal_json),"reason_details":value.reason_details})
}
pub(super) fn why_json(value: &WhyReport) -> JsonValue {
    json!({"headline":value.headline,"rejected_count":value.rejected_count,"trace_ids":value.trace_ids.iter().map(|id|id.as_str()).collect::<Vec<_>>(),"sections":value.sections.iter().map(|section|json!({"id":section.id.as_str(),"kind":why_section_label(section.kind),"title":section.title,"entries":section.entries.iter().map(|entry|json!({"code":entry.code.as_str(),"status":why_status_label(entry.status),"summary":entry.summary,"visibility":visibility_label(entry.visibility),"details":map_json(&entry.details)})).collect::<Vec<_>>() })).collect::<Vec<_>>()})
}
pub(super) fn trace_json(value: &Trace) -> JsonValue {
    json!({"id":value.id.as_str(),"events":value.events.iter().map(|event|json!({"code":event.code.as_str(),"phase":event.phase,"summary":event.summary,"visibility":visibility_label(event.visibility),"details":map_json(&event.details)})).collect::<Vec<_>>()})
}
pub(super) fn provenance_label(value: &ValueProvenance) -> &'static str {
    match value {
        ValueProvenance::Utterance => "utterance",
        ValueProvenance::Context => "context",
        ValueProvenance::ConversationState => "conversation_state",
        ValueProvenance::HostReferenceResolver => "host_reference_resolver",
        ValueProvenance::NeuralProposal => "neural_proposal",
        ValueProvenance::AuthorRule => "author_rule",
    }
}
pub(super) fn visibility_label(value: TraceVisibility) -> &'static str {
    match value {
        TraceVisibility::Public => "public",
        TraceVisibility::Author => "author",
        TraceVisibility::Sensitive => "sensitive",
    }
}
pub(super) fn why_section_label(value: WhySectionKind) -> &'static str {
    match value {
        WhySectionKind::Summary => "summary",
        WhySectionKind::Rejections => "rejections",
        WhySectionKind::Understanding => "understanding",
        WhySectionKind::Conversation => "conversation",
        WhySectionKind::Capability => "capability",
        WhySectionKind::Context => "context",
        WhySectionKind::Selected => "selected",
        WhySectionKind::Other => "other",
    }
}
pub(super) fn why_status_label(value: WhyEntryStatus) -> &'static str {
    match value {
        WhyEntryStatus::Information => "information",
        WhyEntryStatus::Accepted => "accepted",
        WhyEntryStatus::Selected => "selected",
        WhyEntryStatus::Required => "required",
        WhyEntryStatus::Rejected => "rejected",
    }
}
pub(super) fn map_values(
    values: BTreeMap<String, JsonValue>,
) -> Result<BTreeMap<String, Value>, WireError> {
    values
        .into_iter()
        .map(|(key, value)| Ok((key, model_value(&value)?)))
        .collect()
}
pub(super) fn model_value(value: &JsonValue) -> Result<Value, WireError> {
    Ok(match value {
        JsonValue::Null => Value::Null,
        JsonValue::Bool(v) => Value::Bool(*v),
        JsonValue::Number(v) => {
            let n = v
                .as_f64()
                .ok_or_else(|| WireError::Invalid("number cannot be represented as f64".into()))?;
            if !n.is_finite() {
                return Err(WireError::Invalid("non-finite number".into()));
            }
            Value::Number(n)
        }
        JsonValue::String(v) => Value::String(v.clone()),
        JsonValue::Array(v) => Value::Array(v.iter().map(model_value).collect::<Result<_, _>>()?),
        JsonValue::Object(v) => Value::Object(
            v.iter()
                .map(|(k, v)| Ok((k.clone(), model_value(v)?)))
                .collect::<Result<_, WireError>>()?,
        ),
    })
}
pub(super) fn validate_interaction_output(
    output: &RuntimeInteractionOutput,
) -> Result<(), WireError> {
    if let Some(meaning) = &output.conversation.meaning {
        validate_meaning(meaning)?;
    }
    validate_state(&output.conversation.state)?;
    if let Some(analysis) = &output.conversation.semantic {
        validate_trace(&analysis.trace)?;
        if let gvya_kernel::semantic::SemanticDecision::Unresolved { best_score, .. } =
            &analysis.decision
        {
            if !best_score.is_finite() {
                return Err(WireError::Invalid(
                    "runtime produced a non-finite unresolved best score".into(),
                ));
            }
        }
        for row in &analysis.scored {
            for value in [row.score, row.breakdown.evidence_strength] {
                if !value.is_finite() {
                    return Err(WireError::Invalid(
                        "runtime produced a non-finite semantic score".into(),
                    ));
                }
            }
        }
    }
    validate_trace(&output.conversation.trace)?;
    validate_trace(&output.capabilities.trace)?;
    for decision in &output.capabilities.decisions {
        if let Some(proposal) = &decision.proposal {
            validate_map(&proposal.arguments)?;
        }
    }
    for section in &output.why.sections {
        for entry in &section.entries {
            validate_map(&entry.details)?;
        }
    }
    Ok(())
}

pub(super) fn validate_meaning(meaning: &Meaning) -> Result<(), WireError> {
    for slot in &meaning.slots {
        validate_value(&slot.value)?;
    }
    Ok(())
}
pub(super) fn validate_state(state: &GvyaState) -> Result<(), WireError> {
    validate_map(&state.author)?;
    if !author_state_within_limits(&state.author) {
        return Err(WireError::Invalid(
            "runtime produced author state outside canonical state budget".into(),
        ));
    }
    if state.conversation.mentioned_topics.len() > MAX_MENTIONED_TOPICS {
        return Err(WireError::Invalid(
            "runtime produced too many mentioned topics".into(),
        ));
    }
    if state.conversation.hint_progress.len() > MAX_HINT_PROGRESS_ENTRIES {
        return Err(WireError::Invalid(
            "runtime produced too many hint progress entries".into(),
        ));
    }
    if state.conversation.focus.len() > MAX_FOCUS_REFERENCES {
        return Err(WireError::Invalid(
            "runtime produced too many focus references".into(),
        ));
    }
    if state.conversation.recent_response_ids.len() > MAX_RECENT_RESPONSE_IDS {
        return Err(WireError::Invalid(
            "runtime produced too many recent response ids".into(),
        ));
    }
    if state.conversation.recent_variant_keys.len() > MAX_RECENT_VARIANT_KEYS {
        return Err(WireError::Invalid(
            "runtime produced too many recent variant keys".into(),
        ));
    }
    if state.conversation.recent_user_messages.len() > MAX_RECENT_USER_MESSAGES {
        return Err(WireError::Invalid(
            "runtime produced too many recent user messages".into(),
        ));
    }
    if state.conversation.pending_capabilities.len() > CAPABILITY_PENDING_PROPOSALS_MAX {
        return Err(WireError::Invalid(
            "runtime produced too many pending capability proposals".into(),
        ));
    }
    if let Some(collection) = &state.conversation.active_collection {
        if collection.remaining.is_empty()
            || collection.remaining.len() > MAX_ACTIVE_COLLECTION_VALUES
            || collection.meaning.slots.len() > MAX_ACTIVE_COLLECTION_VALUES
            || collection.meaning.references.len() > MAX_ACTIVE_COLLECTION_VALUES
        {
            return Err(WireError::Invalid(
                "runtime produced invalid active collection bounds".into(),
            ));
        }
        validate_meaning(&collection.meaning)?;
    }
    for proposal in &state.conversation.pending_capabilities {
        validate_map(&proposal.arguments)?;
    }
    if !state.conversation.user_style.confidence.is_finite() {
        return Err(WireError::Invalid(
            "runtime produced non-finite user style confidence".into(),
        ));
    }
    Ok(())
}
pub(super) fn validate_trace(trace: &Trace) -> Result<(), WireError> {
    for event in &trace.events {
        validate_map(&event.details)?;
    }
    Ok(())
}
pub(super) fn validate_map(values: &BTreeMap<String, Value>) -> Result<(), WireError> {
    for value in values.values() {
        validate_value(value)?;
    }
    Ok(())
}
pub(super) fn validate_value(value: &Value) -> Result<(), WireError> {
    match value {
        Value::Number(number) if !number.is_finite() => Err(WireError::Invalid(
            "runtime wire output contains a non-finite number".into(),
        )),
        Value::Array(values) => {
            for value in values {
                validate_value(value)?;
            }
            Ok(())
        }
        Value::Object(values) => validate_map(values),
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => Ok(()),
    }
}

pub(super) fn json_value(value: &Value) -> JsonValue {
    match value {
        Value::Null => JsonValue::Null,
        Value::Bool(v) => JsonValue::Bool(*v),
        Value::Number(v) => JsonValue::Number(
            serde_json::Number::from_f64(*v).expect("GVYA wire output was finite-validated"),
        ),
        Value::String(v) => JsonValue::String(v.clone()),
        Value::Array(v) => JsonValue::Array(v.iter().map(json_value).collect()),
        Value::Object(v) => JsonValue::Object(
            v.iter()
                .map(|(k, v)| (k.clone(), json_value(v)))
                .collect::<Map<_, _>>(),
        ),
    }
}
pub(super) fn map_json(values: &BTreeMap<String, Value>) -> JsonValue {
    JsonValue::Object(
        values
            .iter()
            .map(|(k, v)| (k.clone(), json_value(v)))
            .collect(),
    )
}
