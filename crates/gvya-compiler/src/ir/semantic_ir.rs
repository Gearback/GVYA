//! Semantic executable IR serialization.

use super::helpers::*;
use super::*;

pub(super) fn semantic(
    project: &ComposedProject,
    config: &SemanticConfig,
) -> Result<JsonValue, IrError> {
    // The matcher index is derived data: the runtime builds it from exactly these canonical
    // patterns/profiles. Build it here only to prove the composed semantics can produce a valid
    // bounded index at compile time, then discard it instead of serializing megabytes of it.
    validate_semantic_index_is_constructible(
        &project.semantic_catalog,
        &project.semantic_profiles,
    )?;
    let patterns = project.semantic_catalog.patterns();
    Ok(object([
        (
            "config",
            object([
                ("candidate_limit", usize_json(config.candidate_limit)),
                (
                    "resolution_threshold",
                    finite(config.resolution_threshold, "semantic.resolution_threshold")?,
                ),
                (
                    "ambiguity_margin",
                    finite(config.ambiguity_margin, "semantic.ambiguity_margin")?,
                ),
                (
                    "resolver_min_confidence",
                    finite(
                        f64::from(config.resolver_min_confidence),
                        "semantic.resolver_min_confidence",
                    )?,
                ),
                (
                    "resolver_candidate_limit",
                    usize_json(config.resolver_candidate_limit),
                ),
            ]),
        ),
        (
            "profiles",
            JsonValue::Object(
                project
                    .semantic_profiles
                    .iter()
                    .map(|(language, profile)| (language.clone(), semantic_profile(profile)))
                    .collect(),
            ),
        ),
        (
            "patterns",
            JsonValue::Array(
                patterns
                    .iter()
                    .map(meaning_pattern)
                    .collect::<Result<_, _>>()?,
            ),
        ),
    ]))
}

/// Compile-time proof that the composed semantic catalog/profile pair yields a valid bounded
/// matcher index. The index itself is never serialized; only its constructibility is a build gate.
fn validate_semantic_index_is_constructible(
    catalog: &gvya_kernel::semantic::SemanticCatalog,
    profiles: &SemanticProfiles,
) -> Result<(), IrError> {
    SemanticIndex::build(catalog, profiles)
        .map(|_| ())
        .map_err(|error| IrError::InvalidSemanticIndex(format!("{error:?}")))
}

pub(super) fn semantic_profile(profile: &SemanticProfile) -> JsonValue {
    object([
        ("canonical_tokens", map_strings(&profile.canonical_tokens)),
        (
            "canonical_suffixes",
            map_strings(&profile.canonical_suffixes),
        ),
        (
            "canonical_suffix_exceptions",
            set_strings(&profile.canonical_suffix_exceptions),
        ),
        ("detached_suffixes", set_strings(&profile.detached_suffixes)),
        (
            "normalization_rewrites",
            map_strings(&profile.normalization_rewrites),
        ),
        (
            "normalization_remove_chars",
            set_strings(&profile.normalization_remove_chars),
        ),
        ("colloquial", map_vec_strings(&profile.colloquial)),
        ("pure_glue", set_strings(&profile.pure_glue)),
        ("very_low_weight", set_strings(&profile.very_low_weight)),
        ("low_weight", set_strings(&profile.low_weight)),
        (
            "context_low_weight",
            set_strings(&profile.context_low_weight),
        ),
        (
            "generic_singletons",
            set_strings(&profile.generic_singletons),
        ),
        ("reporting_verbs", set_strings(&profile.reporting_verbs)),
        ("reporting_nouns", set_strings(&profile.reporting_nouns)),
        ("pronouns", set_strings(&profile.pronouns)),
        ("negations", set_strings(&profile.negations)),
        ("social_vocabulary", set_strings(&profile.social_vocabulary)),
        ("task_cues", set_strings(&profile.task_cues)),
        (
            "weak_numeric_ignore",
            set_strings(&profile.weak_numeric_ignore),
        ),
        (
            "continuation_exact_phrases",
            set_strings(&profile.continuation_exact_phrases),
        ),
        (
            "continuation_question_starters",
            set_strings(&profile.continuation_question_starters),
        ),
        (
            "continuation_references",
            set_strings(&profile.continuation_references),
        ),
        (
            "generic_followup_phrases",
            set_strings(&profile.generic_followup_phrases),
        ),
        (
            "boolean_values",
            JsonValue::Object(
                profile
                    .boolean_values
                    .iter()
                    .map(|(text, value)| (text.clone(), JsonValue::Bool(*value)))
                    .collect(),
            ),
        ),
        (
            "custom_entities",
            JsonValue::Object(
                profile
                    .custom_entities
                    .iter()
                    .map(|(kind, values)| (kind.clone(), map_vec_strings(values)))
                    .collect(),
            ),
        ),
        (
            "pattern_sets",
            JsonValue::Object(
                profile
                    .pattern_sets
                    .iter()
                    .map(|(name, aliases)| (name.clone(), map_strings(aliases)))
                    .collect(),
            ),
        ),
        ("number_words", map_f64(&profile.number_words)),
        ("relative_dates", map_strings(&profile.relative_dates)),
        ("colors", map_strings(&profile.colors)),
        ("units", map_strings(&profile.units)),
        ("time_markers", set_strings(&profile.time_markers)),
        ("origin_prefixes", map_vec_strings(&profile.origin_prefixes)),
    ])
}

pub(super) fn meaning_pattern(pattern: &MeaningPattern) -> Result<JsonValue, IrError> {
    Ok(object([
        ("id", string(pattern.id.as_str())),
        (
            "class",
            string(match pattern.class {
                MeaningClass::General => "general",
                MeaningClass::Social => "social",
                MeaningClass::Clarification => "clarification",
            }),
        ),
        (
            "patterns",
            JsonValue::Array(
                pattern
                    .patterns
                    .iter()
                    .map(|rule| {
                        object([
                            ("language", string(&rule.language)),
                            ("text", string(&rule.text)),
                            ("priority", integer(i64::from(rule.priority))),
                        ])
                    })
                    .collect(),
            ),
        ),
        (
            "samples",
            JsonValue::Array(
                pattern
                    .samples
                    .iter()
                    .map(|sample| {
                        object([
                            ("language", string(&sample.language)),
                            ("text", string(&sample.text)),
                        ])
                    })
                    .collect(),
            ),
        ),
        (
            "negative_samples",
            semantic_localized_texts(&pattern.negative_samples),
        ),
        (
            "retrieval_terms",
            semantic_localized_texts(&pattern.retrieval_terms),
        ),
        ("priority", integer(i64::from(pattern.priority))),
        (
            "positive_assumption",
            JsonValue::Bool(pattern.positive_assumption),
        ),
        (
            "slots",
            JsonValue::Array(
                pattern
                    .slots
                    .iter()
                    .map(|slot| {
                        object([
                            ("name", string(&slot.name)),
                            (
                                "kind",
                                match &slot.kind {
                                    SlotKind::String => object([("type", string("string"))]),
                                    SlotKind::Number => object([("type", string("number"))]),
                                    SlotKind::Boolean => object([("type", string("boolean"))]),
                                    SlotKind::Entity(kind) => object([
                                        ("type", string("entity")),
                                        ("entity_kind", string(kind.as_str())),
                                    ]),
                                    SlotKind::Reference(kind) => object([
                                        ("type", string("reference")),
                                        ("reference_kind", string(kind.as_str())),
                                    ]),
                                },
                            ),
                            ("required", JsonValue::Bool(slot.required)),
                            (
                                "elicitation",
                                JsonValue::Array(
                                    slot.elicitation
                                        .iter()
                                        .map(|prompt| {
                                            object([
                                                ("language", string(&prompt.language)),
                                                ("text", string(&prompt.text)),
                                            ])
                                        })
                                        .collect(),
                                ),
                            ),
                        ])
                    })
                    .collect(),
            ),
        ),
        (
            "references",
            JsonValue::Array(
                pattern
                    .references
                    .iter()
                    .map(|reference| {
                        object([
                            ("kind", string(reference.kind.as_str())),
                            ("required", JsonValue::Bool(reference.required)),
                            (
                                "elicitation",
                                JsonValue::Array(
                                    reference
                                        .elicitation
                                        .iter()
                                        .map(|prompt| {
                                            object([
                                                ("language", string(&prompt.language)),
                                                ("text", string(&prompt.text)),
                                            ])
                                        })
                                        .collect(),
                                ),
                            ),
                        ])
                    })
                    .collect(),
            ),
        ),
    ]))
}
