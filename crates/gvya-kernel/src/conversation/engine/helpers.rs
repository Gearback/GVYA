//! Shared deterministic conversation helper functions.

use super::*;

pub(super) fn activate_language(
    requested: Option<&str>,
    state: &mut gvya_model::ConversationState,
) -> Option<String> {
    let requested = requested
        .map(normalize_language_tag)
        .filter(|language| !language.is_empty());
    if let Some(language) = requested {
        state.active_language = Some(language.clone());
        Some(language)
    } else {
        state.active_language.clone()
    }
}

pub(super) fn prefer_topic_analysis(
    topic: &SemanticAnalysis,
    global: &SemanticAnalysis,
    margin: f64,
) -> bool {
    if !matches!(
        topic.decision,
        SemanticDecision::Resolved { .. } | SemanticDecision::Partial { .. }
    ) {
        return false;
    }
    if !matches!(
        global.decision,
        SemanticDecision::Resolved { .. } | SemanticDecision::Partial { .. }
    ) {
        return true;
    }
    let topic_structural = matches!(
        topic.decision,
        SemanticDecision::Resolved {
            source: ResolutionSource::StructuralPattern,
            ..
        } | SemanticDecision::Partial {
            source: ResolutionSource::StructuralPattern,
            ..
        }
    );
    let global_structural = matches!(
        global.decision,
        SemanticDecision::Resolved {
            source: ResolutionSource::StructuralPattern,
            ..
        } | SemanticDecision::Partial {
            source: ResolutionSource::StructuralPattern,
            ..
        }
    );
    if topic_structural != global_structural {
        return topic_structural;
    }
    let topic_score = topic.scored.first().map_or(1.0, |row| row.score);
    let global_score = global.scored.first().map_or(1.0, |row| row.score);
    topic_score + margin >= global_score
}

pub(super) fn semantic_candidate(analysis: &SemanticAnalysis) -> Option<MeaningId> {
    match &analysis.decision {
        SemanticDecision::Resolved { meaning, .. } => Some(meaning.id.clone()),
        SemanticDecision::Partial { partial, .. } => Some(partial.meaning.id.clone()),
        SemanticDecision::Ambiguous { candidates, .. } => candidates.first().cloned(),
        SemanticDecision::Unresolved { .. } => None,
    }
}

pub(super) fn semantic_decision_label(decision: &SemanticDecision) -> &'static str {
    match decision {
        SemanticDecision::Resolved { .. } => "resolved",
        SemanticDecision::Partial { .. } => "partial",
        SemanticDecision::Ambiguous { .. } => "ambiguous",
        SemanticDecision::Unresolved { .. } => "unresolved",
    }
}

pub(super) fn is_contextual_continuation(normalized: &str, profile: &SemanticProfile) -> bool {
    let ordered = ordered_tokens(normalized);
    let lexical_has_pronoun = profile.has_pronoun(&ordered);
    let tokens = profile.normalize_colloquial_tokens(&ordered);
    if tokens.is_empty() {
        return false;
    }
    let joined = tokens.join(" ");
    if profile.continuation_exact_phrases.contains(&joined) {
        return true;
    }
    if tokens.len() > 10 || tokens.iter().any(|token| profile.is_task_cue(token)) {
        return false;
    }
    let has_explicit_reference = ordered
        .iter()
        .any(|token| profile.continuation_references.contains(token));
    let has_reference = lexical_has_pronoun || has_explicit_reference;
    // Longer continuations must be explicitly referential in two independent lexical views: the
    // language profile must recognize both a pronoun and a continuation reference. Standalone
    // semantic evidence is still checked by the caller before context can win.
    if tokens.len() > 5 && !(lexical_has_pronoun && has_explicit_reference) {
        return false;
    }
    if tokens
        .first()
        .is_some_and(|token| profile.continuation_question_starters.contains(token))
        && !has_reference
    {
        return false;
    }
    has_reference
}

pub(super) fn is_generic_followup_phrase(normalized: &str, profile: &SemanticProfile) -> bool {
    profile.generic_followup_phrases.contains(normalized)
}

pub(super) fn has_strong_standalone_evidence(analysis: &SemanticAnalysis) -> bool {
    // Structural patterns are authoritative before semantic scoring and intentionally return no
    // scored rows. Treat a structural resolution as standalone authority so conversation
    // continuation cannot steal an explicit author rule merely because `scored` is empty.
    if matches!(
        analysis.decision,
        SemanticDecision::Resolved {
            source: ResolutionSource::StructuralPattern,
            ..
        } | SemanticDecision::Partial {
            source: ResolutionSource::StructuralPattern,
            ..
        }
    ) {
        return true;
    }

    analysis
        .scored
        .first()
        .is_some_and(|row| row.breakdown.evidence_tier <= 3)
}

pub(super) fn formality_label(formality: gvya_model::Formality) -> &'static str {
    match formality {
        gvya_model::Formality::Unknown => "unknown",
        gvya_model::Formality::Formal => "formal",
        gvya_model::Formality::Informal => "informal",
    }
}

pub(super) fn capability_result_interaction_map(
    request: &ConversationCapabilityResultRequest,
) -> BTreeMap<String, Value> {
    let mut result = BTreeMap::from([
        (
            "kind".to_string(),
            Value::String("capability_result".to_string()),
        ),
        (
            "proposalId".to_string(),
            Value::String(request.proposal_id.as_str().to_string()),
        ),
        (
            "capability".to_string(),
            Value::String(request.capability.as_str().to_string()),
        ),
        (
            "capabilityVersion".to_string(),
            Value::String(request.capability_version.as_str().to_string()),
        ),
        ("succeeded".to_string(), Value::Bool(request.succeeded)),
    ]);
    if let Some(output) = &request.output {
        result.insert("output".to_string(), output.clone());
    }
    if let Some(error_code) = &request.error_code {
        result.insert("errorCode".to_string(), Value::String(error_code.clone()));
    }
    result
}

pub(super) fn event(
    code: &str,
    phase: &str,
    summary: &str,
    details: BTreeMap<String, Value>,
) -> TraceEvent {
    TraceEvent {
        code: TraceCode::new(code),
        phase: phase.to_string(),
        summary: summary.to_string(),
        visibility: TraceVisibility::Author,
        details,
    }
}

pub(super) fn map<const N: usize>(pairs: [(&str, Value); N]) -> BTreeMap<String, Value> {
    pairs
        .into_iter()
        .map(|(key, value)| (key.to_string(), value))
        .collect()
}
