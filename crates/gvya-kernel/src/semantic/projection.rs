//! The one resolver-safe candidate projection.
//!
//! Everything an external resolver is allowed to see about authored semantics is derived here, so
//! there is exactly one place that decides what leaves the deterministic kernel and exactly one
//! set of bounds. Nothing in this module grants authority: the projection is read-only evidence
//! that the semantic firewall independently re-validates after the resolver answers.

use gvya_model::{MeaningId, ReferenceKind};

use super::catalog::{MeaningPattern, SlotKind, SlotSpec};
use super::normalization::{language_is_compatible, ordered_tokens};
use super::profile::SemanticProfile;
use super::scoring::ScoredMeaning;
use crate::{
    RESOLVER_ENTITY_VALUE_MAX_BYTES, RESOLVER_ENTITY_VALUES_PER_SLOT_MAX, RESOLVER_HINT_MAX_BYTES,
    RESOLVER_HINTS_PER_CANDIDATE_MAX, RESOLVER_MATCHED_TERM_MAX_BYTES, RESOLVER_MATCHED_TERMS_MAX,
    RESOLVER_REFERENCES_PER_CANDIDATE_MAX, RESOLVER_SLOTS_PER_CANDIDATE_MAX,
    ResolverCandidateEvidence, ResolverCandidateOrigin, ResolverEntitySchema,
    ResolverEvidenceStrength, ResolverHintKind, ResolverMeaningCandidate, ResolverReferenceSchema,
    ResolverSemanticHint, ResolverSlotSchema, ResolverValueKind,
};

/// Authored samples are the most natural evidence, but retrieval metadata is the most
/// discriminative, so a fixed reserve keeps both visible for a Meaning with many samples.
const HINT_FIRST_SAMPLES: usize = 3;
const HINT_RETRIEVAL_TERMS: usize = 2;
const HINT_STRUCTURAL_PATTERNS: usize = 1;

/// Coarse retrieval bands in the index's canonical milli-units.
const RETRIEVAL_MODERATE_MILLI: u64 = 150_000;
const RETRIEVAL_STRONG_MILLI: u64 = 500_000;

/// Build the resolver-safe projection of one candidate Meaning.
pub(super) fn meaning_candidate(
    pattern: &MeaningPattern,
    origin: ResolverCandidateOrigin,
    evidence: Option<ResolverCandidateEvidence>,
    profile: &SemanticProfile,
    requested_language: Option<&str>,
    language_fallbacks: &[String],
) -> ResolverMeaningCandidate {
    let hints = semantic_hints(pattern, requested_language, language_fallbacks);
    ResolverMeaningCandidate {
        meaning: pattern.id.clone(),
        origin,
        evidence,
        hints,
        slots: pattern
            .slots
            .iter()
            .take(RESOLVER_SLOTS_PER_CANDIDATE_MAX)
            .map(|spec| slot_schema(spec, profile))
            .collect(),
        references: pattern
            .references
            .iter()
            .take(RESOLVER_REFERENCES_PER_CANDIDATE_MAX)
            .map(|spec| ResolverReferenceSchema {
                kind: spec.kind.clone(),
                required: spec.required,
            })
            .collect(),
    }
}

/// Deterministic matcher evidence for one scored row, normalized into stable coarse bands.
pub(super) fn candidate_evidence(
    row: &ScoredMeaning,
    hints: &[ResolverSemanticHint],
    normalized_input: &str,
    profile: &SemanticProfile,
    resolution_threshold: f64,
) -> ResolverCandidateEvidence {
    let rejected = row.breakdown.rejected_reason.is_some() || row.breakdown.no_evidence;
    let semantic = if rejected || row.score <= 0.0 {
        ResolverEvidenceStrength::None
    } else if row.score >= resolution_threshold {
        ResolverEvidenceStrength::Strong
    } else if row.score * 2.0 >= resolution_threshold {
        ResolverEvidenceStrength::Moderate
    } else {
        ResolverEvidenceStrength::Weak
    };
    let retrieval = if rejected {
        ResolverEvidenceStrength::None
    } else {
        retrieval_band(row.retrieval_rank_milli)
    };
    ResolverCandidateEvidence {
        semantic,
        retrieval,
        matched_terms: matched_terms(hints, normalized_input, profile),
    }
}

/// Evidence for a candidate that only the broader high-recall resolver stage produced.
///
/// Such a row was never semantically scored, so its semantic band is `None` by construction. This
/// is exactly the distinction a small model needs between a strong deterministic candidate and a
/// broad recall candidate, and it grants no authority either way.
pub(super) fn recall_evidence(
    rank_milli: u64,
    hints: &[ResolverSemanticHint],
    normalized_input: &str,
    profile: &SemanticProfile,
) -> ResolverCandidateEvidence {
    ResolverCandidateEvidence {
        semantic: ResolverEvidenceStrength::None,
        retrieval: retrieval_band(rank_milli),
        matched_terms: matched_terms(hints, normalized_input, profile),
    }
}

fn retrieval_band(rank_milli: u64) -> ResolverEvidenceStrength {
    if rank_milli == 0 {
        ResolverEvidenceStrength::None
    } else if rank_milli >= RETRIEVAL_STRONG_MILLI {
        ResolverEvidenceStrength::Strong
    } else if rank_milli >= RETRIEVAL_MODERATE_MILLI {
        ResolverEvidenceStrength::Moderate
    } else {
        ResolverEvidenceStrength::Weak
    }
}

/// Bounded natural-language projection of authored semantic evidence for the turn's language.
///
/// Only authored positive evidence crosses the boundary. Negative samples, responses, tests,
/// scenarios, provenance and package content never do.
fn semantic_hints(
    pattern: &MeaningPattern,
    requested_language: Option<&str>,
    language_fallbacks: &[String],
) -> Vec<ResolverSemanticHint> {
    let compatible =
        |language: &str| language_is_compatible(requested_language, language_fallbacks, language);
    let mut hints: Vec<ResolverSemanticHint> = Vec::new();
    let push = |hints: &mut Vec<ResolverSemanticHint>,
                kind: ResolverHintKind,
                language: &str,
                text: &str| {
        let text = truncate_bytes(text.trim(), RESOLVER_HINT_MAX_BYTES);
        if text.is_empty() || hints.len() >= RESOLVER_HINTS_PER_CANDIDATE_MAX {
            return;
        }
        if hints.iter().any(|row| row.text == text) {
            return;
        }
        hints.push(ResolverSemanticHint {
            kind,
            language: language.to_string(),
            text,
        });
    };

    let samples: Vec<_> = pattern
        .samples
        .iter()
        .filter(|row| compatible(&row.language))
        .collect();
    for row in samples.iter().take(HINT_FIRST_SAMPLES) {
        push(
            &mut hints,
            ResolverHintKind::Sample,
            &row.language,
            &row.text,
        );
    }
    for row in pattern
        .retrieval_terms
        .iter()
        .filter(|row| compatible(&row.language))
        .take(HINT_RETRIEVAL_TERMS)
    {
        push(
            &mut hints,
            ResolverHintKind::RetrievalTerm,
            &row.language,
            &row.text,
        );
    }
    for row in pattern
        .patterns
        .iter()
        .filter(|row| compatible(&row.language))
        .take(HINT_STRUCTURAL_PATTERNS)
    {
        push(
            &mut hints,
            ResolverHintKind::StructuralPattern,
            &row.language,
            &row.text,
        );
    }
    for row in samples.iter().skip(HINT_FIRST_SAMPLES) {
        push(
            &mut hints,
            ResolverHintKind::Sample,
            &row.language,
            &row.text,
        );
    }
    hints
}

/// Normalized input terms that actually appear in the projected hints.
///
/// This is derived only from evidence the resolver can already see, so it explains the match
/// without exposing index internals or authored content that was withheld.
fn matched_terms(
    hints: &[ResolverSemanticHint],
    normalized_input: &str,
    profile: &SemanticProfile,
) -> Vec<String> {
    let input = profile.normalize_colloquial_tokens(&ordered_tokens(normalized_input));
    let content = profile.content_tokens(&input);
    let mut hint_tokens = Vec::new();
    for hint in hints {
        let tokens = profile
            .normalize_colloquial_tokens(&ordered_tokens(&profile.normalize_text(&hint.text)));
        for token in tokens {
            let canonical = profile.canonical_token(&token);
            if !hint_tokens.contains(&canonical) {
                hint_tokens.push(canonical);
            }
        }
    }
    let mut out: Vec<String> = Vec::new();
    for token in content {
        if out.len() >= RESOLVER_MATCHED_TERMS_MAX {
            break;
        }
        let canonical = profile.canonical_token(&token);
        if hint_tokens.contains(&canonical) && !out.contains(&token) {
            out.push(truncate_bytes(&token, RESOLVER_MATCHED_TERM_MAX_BYTES));
        }
    }
    out.retain(|row| !row.is_empty());
    out
}

fn slot_schema(spec: &SlotSpec, profile: &SemanticProfile) -> ResolverSlotSchema {
    ResolverSlotSchema {
        name: spec.name.clone(),
        kind: value_kind(&spec.kind, profile),
        required: spec.required,
    }
}

fn value_kind(kind: &SlotKind, profile: &SemanticProfile) -> ResolverValueKind {
    match kind {
        SlotKind::String => ResolverValueKind::String,
        SlotKind::Number => ResolverValueKind::Number,
        SlotKind::Boolean => ResolverValueKind::Boolean,
        SlotKind::Reference(kind) => ResolverValueKind::Reference { kind: kind.clone() },
        SlotKind::Entity(kind) => ResolverValueKind::Entity(entity_schema(kind.as_str(), profile)),
    }
}

/// Canonical value authority for one entity kind, mirroring exactly what the firewall accepts.
fn entity_schema(kind: &str, profile: &SemanticProfile) -> ResolverEntitySchema {
    let bounded = |mut values: Vec<String>| {
        values.sort();
        values.dedup();
        let exhaustive = values.len() <= RESOLVER_ENTITY_VALUES_PER_SLOT_MAX;
        values.truncate(RESOLVER_ENTITY_VALUES_PER_SLOT_MAX);
        values.retain(|row| row.len() <= RESOLVER_ENTITY_VALUE_MAX_BYTES);
        (values, exhaustive)
    };
    let (canonical_values, values_are_exhaustive) = match kind {
        // Open canonical forms: the value space is generated from the utterance, not enumerable.
        "number" | "time" | "email" | "phone" | "url" | "origin" | "quantity" => {
            (Vec::new(), false)
        }
        // Partly closed: symbolic relative dates are enumerable, ISO calendar dates are not.
        "date" => {
            let (values, _) = bounded(
                profile
                    .relative_dates
                    .values()
                    .map(|value| format!("relative:{value}"))
                    .collect(),
            );
            (values, false)
        }
        "color" => bounded(profile.colors.values().cloned().collect()),
        "unit" => bounded(profile.units.values().cloned().collect()),
        _ => match profile.custom_entities.get(kind) {
            Some(catalog) => bounded(catalog.keys().cloned().collect()),
            // No authority at all in this profile: the accepted set is genuinely empty.
            None => (Vec::new(), true),
        },
    };
    ResolverEntitySchema {
        kind: kind.to_string(),
        canonical_values,
        values_are_exhaustive,
    }
}

/// Truncate on a character boundary so a bounded projection can never emit invalid UTF-8.
pub(super) fn truncate_bytes(text: &str, max_bytes: usize) -> String {
    if text.len() <= max_bytes {
        return text.to_string();
    }
    let mut end = max_bytes;
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    text[..end].to_string()
}

/// Reference declarations projected for one Meaning, used by collection targets.
pub(super) fn reference_schema(kind: &ReferenceKind, required: bool) -> ResolverReferenceSchema {
    ResolverReferenceSchema {
        kind: kind.clone(),
        required,
    }
}

/// Slot schema for one named declaration of a Meaning, used by collection targets.
pub(super) fn slot_schema_for(
    pattern: &MeaningPattern,
    name: &str,
    profile: &SemanticProfile,
) -> Option<ResolverSlotSchema> {
    pattern
        .slots
        .iter()
        .find(|spec| spec.name == name)
        .map(|spec| slot_schema(spec, profile))
}

/// Meaning identity helper kept next to the projection so candidate assembly has one owner.
pub(super) fn candidate_contains(candidates: &[ResolverMeaningCandidate], id: &MeaningId) -> bool {
    candidates.iter().any(|row| &row.meaning == id)
}
