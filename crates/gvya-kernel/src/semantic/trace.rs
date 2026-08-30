//! Semantic trace construction.

use super::*;

pub(super) fn build_structural_trace(
    views: &SemanticViews,
    structural: &StructuralMatchSummary,
    decision: &SemanticDecision,
) -> Trace {
    build_structural_trace_optional(views, Some(structural), decision)
}

pub(super) fn build_structural_trace_optional(
    views: &SemanticViews,
    structural: Option<&StructuralMatchSummary>,
    decision: &SemanticDecision,
) -> Trace {
    let trace_id = TraceId::new(format!(
        "semantic-{:016x}",
        fnv1a64(views.normalized.as_bytes())
    ));
    let mut events = Vec::new();
    let mut normalize_details = BTreeMap::new();
    normalize_details.insert("normalized".into(), Value::String(views.normalized.clone()));
    events.push(trace_event(
        "semantic.input.normalized",
        "semantic",
        "Built deterministic semantic views",
        normalize_details,
    ));

    if let Some(structural) = structural {
        let mut details = BTreeMap::new();
        details.insert(
            "meaning".into(),
            Value::String(structural.meaning.as_str().to_string()),
        );
        details.insert("pattern".into(), Value::String(structural.pattern.clone()));
        details.insert(
            "language".into(),
            Value::String(structural.language.clone()),
        );
        details.insert(
            "priority".into(),
            Value::Number(f64::from(structural.priority)),
        );
        details.insert(
            "captures".into(),
            Value::Object(
                structural
                    .captures
                    .iter()
                    .map(|(name, value)| (name.clone(), Value::String(value.clone())))
                    .collect(),
            ),
        );
        events.push(trace_event(
            "semantic.structural.matched",
            "semantic.structural",
            "Matched an explicit structural pattern",
            details,
        ));
    }

    let (code, summary) = match decision {
        SemanticDecision::Resolved { .. } => (
            "semantic.decision.structural",
            "Resolved explicit structural pattern",
        ),
        SemanticDecision::Partial { .. } => (
            "semantic.decision.structural_partial",
            "Preserved resolved values for a partial structural Meaning",
        ),
        SemanticDecision::Ambiguous { .. } => (
            "semantic.decision.structural_ambiguous",
            "Preserved structural pattern ambiguity",
        ),
        SemanticDecision::Unresolved { .. } => (
            "semantic.decision.structural_unresolved",
            "Structural pattern matched but required binding did not resolve",
        ),
    };
    events.push(trace_event(code, "semantic", summary, BTreeMap::new()));
    Trace {
        id: trace_id,
        events,
    }
}

pub(super) fn build_trace(
    views: &SemanticViews,
    pruning: &CandidateDecision,
    bounded_scope_scan: bool,
    exhaustive_sample_scan: bool,
    exhaustive_sample_rescue: bool,
    scored: &[ScoredMeaning],
    decision: &SemanticDecision,
) -> Trace {
    let trace_id = TraceId::new(format!(
        "semantic-{:016x}",
        fnv1a64(views.normalized.as_bytes())
    ));
    let mut events = Vec::new();
    let mut normalize_details = BTreeMap::new();
    normalize_details.insert("normalized".into(), Value::String(views.normalized.clone()));
    normalize_details.insert("view_count".into(), Value::Number(views.views.len() as f64));
    events.push(trace_event(
        "semantic.input.normalized",
        "semantic",
        "Built deterministic semantic views",
        normalize_details,
    ));

    let mut entity_details = BTreeMap::new();
    entity_details.insert("count".into(), Value::Number(views.entities.len() as f64));
    entity_details.insert(
        "kinds".into(),
        Value::Array(
            views
                .entities
                .iter()
                .map(|entity| Value::String(entity.kind.as_str().to_string()))
                .collect(),
        ),
    );
    events.push(trace_event(
        "semantic.entities.extracted",
        "semantic",
        "Extracted typed semantic entities",
        entity_details,
    ));

    let mut prune_details = BTreeMap::new();
    prune_details.insert("used".into(), Value::Bool(!pruning.use_full_scan));
    prune_details.insert("reason".into(), Value::String(pruning.reason.to_string()));
    prune_details.insert(
        "candidate_count".into(),
        Value::Number(pruning.rows.len() as f64),
    );
    prune_details.insert(
        "total_patterns".into(),
        Value::Number(pruning.total_patterns as f64),
    );
    prune_details.insert(
        "posting_visits".into(),
        Value::Number(pruning.posting_visits as f64),
    );
    prune_details.insert(
        "posting_saturated".into(),
        Value::Bool(pruning.posting_saturated),
    );
    prune_details.insert(
        "saturated_keys".into(),
        Value::Number(pruning.saturated_keys as f64),
    );
    prune_details.insert("bounded_scope_scan".into(), Value::Bool(bounded_scope_scan));
    prune_details.insert(
        "exhaustive_sample_scan".into(),
        Value::Bool(exhaustive_sample_scan),
    );
    prune_details.insert(
        "exhaustive_sample_rescue".into(),
        Value::Bool(exhaustive_sample_rescue),
    );
    if let Some(typo) = &pruning.typo_lite {
        prune_details.insert(
            "typo_corrections".into(),
            Value::Array(
                typo.corrections
                    .iter()
                    .map(|row| Value::String(format!("{}→{}", row.from, row.to)))
                    .collect(),
            ),
        );
    }
    events.push(trace_event(
        "semantic.candidates.retrieved",
        "semantic",
        "Retrieved bounded semantic candidates",
        prune_details,
    ));

    let mut score_details = BTreeMap::new();
    score_details.insert(
        "top_scores".into(),
        Value::Array(
            scored
                .iter()
                .take(5)
                .map(|row| {
                    let mut item = BTreeMap::new();
                    item.insert("score".into(), Value::Number(row.score));
                    item.insert(
                        "evidence_tier".into(),
                        Value::Number(f64::from(row.breakdown.evidence_tier)),
                    );
                    item.insert(
                        "match".into(),
                        Value::String(row.breakdown.match_kind.code().to_string()),
                    );
                    Value::Object(item)
                })
                .collect(),
        ),
    );
    events.push(trace_event(
        "semantic.evidence.ranked",
        "semantic",
        "Ranked semantic evidence deterministically",
        score_details,
    ));

    let (code, summary) = match decision {
        SemanticDecision::Resolved {
            source: ResolutionSource::StructuralPattern,
            ..
        } => (
            "semantic.decision.structural",
            "Resolved explicit structural pattern",
        ),
        SemanticDecision::Partial {
            source: ResolutionSource::StructuralPattern,
            ..
        } => (
            "semantic.decision.structural_partial",
            "Selected a partial Meaning from an explicit structural pattern",
        ),
        SemanticDecision::Partial {
            source: ResolutionSource::Deterministic,
            ..
        } => (
            "semantic.decision.partial",
            "Selected a partial Meaning and preserved resolved values",
        ),
        SemanticDecision::Partial {
            source: ResolutionSource::ResolverProposal,
            ..
        } => (
            "semantic.decision.resolver_partial",
            "Selected a partial Meaning from a validated resolver proposal",
        ),
        SemanticDecision::Resolved {
            source: ResolutionSource::Deterministic,
            ..
        } => (
            "semantic.decision.resolved",
            "Resolved meaning deterministically",
        ),
        SemanticDecision::Resolved {
            source: ResolutionSource::ResolverProposal,
            ..
        } => (
            "semantic.decision.resolver",
            "Resolved validated resolver proposal",
        ),
        SemanticDecision::Ambiguous { .. } => (
            "semantic.decision.ambiguous",
            "Preserved semantic ambiguity",
        ),
        SemanticDecision::Unresolved { .. } => (
            "semantic.decision.unresolved",
            "No semantic meaning met resolution requirements",
        ),
    };
    events.push(trace_event(code, "semantic", summary, BTreeMap::new()));
    Trace {
        id: trace_id,
        events,
    }
}

pub(super) fn trace_event(
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

fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf29ce484222325_u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}
