//! Semantic source decoding.

use super::super::*;

pub(in crate::source) fn parse_meaning(
    value: &JsonValue,
    path: &str,
    limits: SourceLimits,
    issues: &mut Vec<SourceIssue>,
) -> Option<MeaningPattern> {
    let obj = expect_object(value, path, issues).ok()?;
    reject_unknown_keys(obj, crate::source::contract::MEANING_KEYS, path, issues);
    let id = required_string(obj, "id", path, limits, issues)?;
    let class = match optional_string(obj, "class", "general", path, limits, issues).as_str() {
        "general" => MeaningClass::General,
        "social" => MeaningClass::Social,
        "clarification" => MeaningClass::Clarification,
        _ => {
            issues.push(issue(
                "source.meaning_class",
                "unknown meaning class",
                Some(path),
            ));
            MeaningClass::General
        }
    };
    let patterns = parse_structural_patterns(
        obj.get("patterns"),
        &format!("{path}.patterns"),
        limits,
        issues,
    );
    let samples = parse_localized_samples(
        obj.get("samples"),
        &format!("{path}.samples"),
        limits,
        issues,
    );
    let negative_samples = parse_localized_samples(
        obj.get("negative_samples"),
        &format!("{path}.negative_samples"),
        limits,
        issues,
    );
    let retrieval_terms = parse_localized_samples(
        obj.get("retrieval_terms"),
        &format!("{path}.retrieval_terms"),
        limits,
        issues,
    );
    for (label, length, max) in [
        (
            "patterns",
            patterns.len(),
            SEMANTIC_PATTERNS_PER_MEANING_MAX,
        ),
        ("samples", samples.len(), SEMANTIC_SAMPLES_PER_MEANING_MAX),
        (
            "negative_samples",
            negative_samples.len(),
            SEMANTIC_NEGATIVE_SAMPLES_PER_MEANING_MAX,
        ),
        (
            "retrieval_terms",
            retrieval_terms.len(),
            SEMANTIC_RETRIEVAL_TERMS_PER_MEANING_MAX,
        ),
    ] {
        if length > max {
            issues.push(issue(
                "source.semantic_work_budget",
                &format!("{label} exceeds the canonical per-Meaning matcher budget"),
                Some(path),
            ));
        }
    }
    let mut semantic_text_bytes = 0usize;
    for text in patterns
        .iter()
        .map(|rule| &rule.text)
        .chain(samples.iter().map(|sample| &sample.text))
        .chain(negative_samples.iter().map(|sample| &sample.text))
        .chain(retrieval_terms.iter().map(|sample| &sample.text))
    {
        if text.len() > SEMANTIC_TEXT_ITEM_MAX_BYTES {
            issues.push(issue(
                "source.semantic_text_item",
                "semantic sample/retrieval text exceeds the canonical matcher item budget",
                Some(path),
            ));
        }
        semantic_text_bytes = semantic_text_bytes.saturating_add(text.len());
    }
    if semantic_text_bytes > SEMANTIC_TEXT_PER_MEANING_MAX_BYTES {
        issues.push(issue(
            "source.semantic_text_budget",
            "Meaning semantic text exceeds the canonical matcher byte budget",
            Some(path),
        ));
    }
    let priority = optional_i32(obj, "priority", 1, path, issues);
    let positive_assumption = optional_bool(obj, "positive_assumption", false, path, issues);
    let slots = parse_slots(obj.get("slots"), path, limits, issues);
    let references = parse_reference_specs(obj.get("references"), path, limits, issues);
    Some(MeaningPattern {
        id: MeaningId::new(id),
        class,
        patterns,
        samples,
        negative_samples,
        retrieval_terms,
        priority,
        positive_assumption,
        slots,
        references,
    })
}

pub(in crate::source) fn parse_structural_patterns(
    value: Option<&JsonValue>,
    path: &str,
    limits: SourceLimits,
    issues: &mut Vec<SourceIssue>,
) -> Vec<LocalizedStructuralPattern> {
    let Some(rows) = optional_array(value, path, issues) else {
        return Vec::new();
    };
    rows.iter()
        .enumerate()
        .filter_map(|(index, value)| {
            let item_path = format!("{path}[{index}]");
            let obj = expect_object(value, &item_path, issues).ok()?;
            reject_unknown_keys(
                obj,
                crate::source::contract::LOCALIZED_SAMPLE_KEYS,
                &item_path,
                issues,
            );
            let language = required_string(obj, "language", &item_path, limits, issues)?;
            let text = required_string(obj, "text", &item_path, limits, issues)?;
            if !language_tag_is_well_formed(&language) {
                issues.push(issue(
                    "source.language_tag",
                    "pattern language must be a well-formed hyphenated BCP 47 tag",
                    Some(&item_path),
                ));
            }
            Some(LocalizedStructuralPattern {
                language,
                text,
                priority: optional_i32(obj, "priority", 0, &item_path, issues),
            })
        })
        .collect()
}

pub(in crate::source) fn parse_localized_samples(
    value: Option<&JsonValue>,
    path: &str,
    limits: SourceLimits,
    issues: &mut Vec<SourceIssue>,
) -> Vec<LocalizedSample> {
    let Some(rows) = optional_array(value, path, issues) else {
        return Vec::new();
    };
    rows.iter()
        .enumerate()
        .filter_map(|(index, value)| {
            let item_path = format!("{path}[{index}]");
            let obj = expect_object(value, &item_path, issues).ok()?;
            reject_unknown_keys(obj, &["language", "text"], &item_path, issues);
            let language = required_string(obj, "language", &item_path, limits, issues)?;
            let text = required_string(obj, "text", &item_path, limits, issues)?;
            if !language_tag_is_well_formed(&language) {
                issues.push(issue(
                    "source.language_tag",
                    "sample language must be a well-formed hyphenated BCP 47 tag",
                    Some(&item_path),
                ));
            }
            Some(LocalizedSample::new(language, text))
        })
        .collect()
}

pub(in crate::source) fn parse_slots(
    value: Option<&JsonValue>,
    path: &str,
    limits: SourceLimits,
    issues: &mut Vec<SourceIssue>,
) -> Vec<SlotSpec> {
    let Some(array) = optional_array(value, path, issues) else {
        return Vec::new();
    };
    array
        .iter()
        .enumerate()
        .filter_map(|(index, row)| {
            let row_path = format!("{path}.slots[{index}]");
            let obj = expect_object(row, &row_path, issues).ok()?;
            reject_unknown_keys(
                obj,
                crate::source::contract::SLOT_SPEC_KEYS,
                &row_path,
                issues,
            );
            let name = required_string(obj, "name", &row_path, limits, issues)?;
            let kind_name = required_string(obj, "type", &row_path, limits, issues)?;
            let kind = match kind_name.as_str() {
                "string" => SlotKind::String,
                "number" => SlotKind::Number,
                "boolean" => SlotKind::Boolean,
                "entity" => SlotKind::Entity(EntityKind::new(required_string(
                    obj,
                    "entity_kind",
                    &row_path,
                    limits,
                    issues,
                )?)),
                "reference" => SlotKind::Reference(ReferenceKind::new(required_string(
                    obj,
                    "reference_kind",
                    &row_path,
                    limits,
                    issues,
                )?)),
                _ => {
                    issues.push(issue(
                        "source.slot_type",
                        "unknown slot type",
                        Some(&row_path),
                    ));
                    return None;
                }
            };
            Some(SlotSpec {
                name,
                kind,
                required: optional_bool(obj, "required", false, &row_path, issues),
                elicitation: parse_localized_samples(
                    obj.get("elicitation"),
                    &format!("{row_path}.elicitation"),
                    limits,
                    issues,
                )
                .into_iter()
                .map(|row| ElicitationPrompt::new(row.language, row.text))
                .collect(),
            })
        })
        .collect()
}

pub(in crate::source) fn parse_reference_specs(
    value: Option<&JsonValue>,
    path: &str,
    limits: SourceLimits,
    issues: &mut Vec<SourceIssue>,
) -> Vec<ReferenceSpec> {
    let Some(array) = optional_array(value, path, issues) else {
        return Vec::new();
    };
    array
        .iter()
        .enumerate()
        .filter_map(|(index, row)| {
            let row_path = format!("{path}.references[{index}]");
            let obj = expect_object(row, &row_path, issues).ok()?;
            reject_unknown_keys(
                obj,
                crate::source::contract::REFERENCE_SPEC_KEYS,
                &row_path,
                issues,
            );
            Some(ReferenceSpec {
                kind: ReferenceKind::new(required_string(obj, "kind", &row_path, limits, issues)?),
                required: optional_bool(obj, "required", false, &row_path, issues),
                elicitation: parse_localized_samples(
                    obj.get("elicitation"),
                    &format!("{row_path}.elicitation"),
                    limits,
                    issues,
                )
                .into_iter()
                .map(|row| ElicitationPrompt::new(row.language, row.text))
                .collect(),
            })
        })
        .collect()
}

fn parse_pattern_sets(
    value: Option<&JsonValue>,
    path: &str,
    limits: SourceLimits,
    issues: &mut Vec<SourceIssue>,
) -> BTreeMap<String, BTreeMap<String, String>> {
    let Some(value) = value else {
        return BTreeMap::new();
    };
    let Ok(obj) = expect_object(value, path, issues) else {
        return BTreeMap::new();
    };
    obj.iter()
        .filter_map(|(name, aliases)| {
            if name.is_empty() || name.len() > limits.max_string_bytes {
                issues.push(issue(
                    "source.pattern_set_name",
                    "pattern set name is empty or too large",
                    Some(path),
                ));
                return None;
            }
            Some((
                name.clone(),
                parse_string_map(Some(aliases), &format!("{path}.{name}"), limits, issues),
            ))
        })
        .collect()
}

fn parse_bool_map(
    value: Option<&JsonValue>,
    path: &str,
    limits: SourceLimits,
    issues: &mut Vec<SourceIssue>,
) -> BTreeMap<String, bool> {
    let Some(value) = value else {
        return BTreeMap::new();
    };
    let Ok(obj) = expect_object(value, path, issues) else {
        return BTreeMap::new();
    };
    obj.iter()
        .filter_map(|(key, value)| {
            if key.is_empty() || key.len() > limits.max_string_bytes {
                issues.push(issue(
                    "source.boolean_vocabulary_key",
                    "boolean vocabulary key is empty or too large",
                    Some(path),
                ));
                return None;
            }
            match value.as_bool() {
                Some(value) => Some((key.clone(), value)),
                None => {
                    issues.push(issue(
                        "source.boolean_vocabulary_value",
                        "boolean vocabulary values must be booleans",
                        Some(&format!("{path}.{key}")),
                    ));
                    None
                }
            }
        })
        .collect()
}

fn parse_custom_entities(
    value: Option<&JsonValue>,
    path: &str,
    limits: SourceLimits,
    issues: &mut Vec<SourceIssue>,
) -> BTreeMap<String, BTreeMap<String, Vec<String>>> {
    let Some(value) = value else {
        return BTreeMap::new();
    };
    let Ok(obj) = expect_object(value, path, issues) else {
        return BTreeMap::new();
    };
    obj.iter()
        .filter_map(|(kind, values)| {
            if kind.is_empty() || kind.len() > limits.max_string_bytes {
                issues.push(issue(
                    "source.custom_entity_kind",
                    "custom entity kind is empty or too large",
                    Some(path),
                ));
                return None;
            }
            Some((
                kind.clone(),
                parse_string_vec_map(Some(values), &format!("{path}.{kind}"), limits, issues),
            ))
        })
        .collect()
}

pub(in crate::source) fn parse_language_profile_data(
    value: &JsonValue,
    path: &str,
    limits: SourceLimits,
    issues: &mut Vec<SourceIssue>,
) -> Option<SemanticProfile> {
    let obj = expect_object(value, path, issues).ok()?;
    reject_unknown_keys(
        obj,
        &[
            "canonical_tokens",
            "canonical_suffixes",
            "canonical_suffix_exceptions",
            "detached_suffixes",
            "normalization_rewrites",
            "normalization_remove_chars",
            "colloquial",
            "pure_glue",
            "very_low_weight",
            "low_weight",
            "context_low_weight",
            "generic_singletons",
            "reporting_verbs",
            "reporting_nouns",
            "pronouns",
            "negations",
            "social_vocabulary",
            "task_cues",
            "weak_numeric_ignore",
            "continuation_exact_phrases",
            "continuation_question_starters",
            "continuation_references",
            "generic_followup_phrases",
            "boolean_values",
            "custom_entities",
            "number_words",
            "relative_dates",
            "colors",
            "units",
            "time_markers",
            "origin_prefixes",
        ],
        path,
        issues,
    );
    Some(SemanticProfile {
        canonical_tokens: parse_string_map(
            obj.get("canonical_tokens"),
            &format!("{path}.canonical_tokens"),
            limits,
            issues,
        ),
        canonical_suffixes: parse_string_map(
            obj.get("canonical_suffixes"),
            &format!("{path}.canonical_suffixes"),
            limits,
            issues,
        ),
        canonical_suffix_exceptions: string_set(
            obj.get("canonical_suffix_exceptions"),
            &format!("{path}.canonical_suffix_exceptions"),
            limits,
            issues,
        ),
        detached_suffixes: string_set(
            obj.get("detached_suffixes"),
            &format!("{path}.detached_suffixes"),
            limits,
            issues,
        ),
        normalization_rewrites: parse_string_map(
            obj.get("normalization_rewrites"),
            &format!("{path}.normalization_rewrites"),
            limits,
            issues,
        ),
        normalization_remove_chars: string_set(
            obj.get("normalization_remove_chars"),
            &format!("{path}.normalization_remove_chars"),
            limits,
            issues,
        ),
        colloquial: parse_string_vec_map(
            obj.get("colloquial"),
            &format!("{path}.colloquial"),
            limits,
            issues,
        ),
        pure_glue: string_set(
            obj.get("pure_glue"),
            &format!("{path}.pure_glue"),
            limits,
            issues,
        ),
        very_low_weight: string_set(
            obj.get("very_low_weight"),
            &format!("{path}.very_low_weight"),
            limits,
            issues,
        ),
        low_weight: string_set(
            obj.get("low_weight"),
            &format!("{path}.low_weight"),
            limits,
            issues,
        ),
        context_low_weight: string_set(
            obj.get("context_low_weight"),
            &format!("{path}.context_low_weight"),
            limits,
            issues,
        ),
        generic_singletons: string_set(
            obj.get("generic_singletons"),
            &format!("{path}.generic_singletons"),
            limits,
            issues,
        ),
        reporting_verbs: string_set(
            obj.get("reporting_verbs"),
            &format!("{path}.reporting_verbs"),
            limits,
            issues,
        ),
        reporting_nouns: string_set(
            obj.get("reporting_nouns"),
            &format!("{path}.reporting_nouns"),
            limits,
            issues,
        ),
        pronouns: string_set(
            obj.get("pronouns"),
            &format!("{path}.pronouns"),
            limits,
            issues,
        ),
        negations: string_set(
            obj.get("negations"),
            &format!("{path}.negations"),
            limits,
            issues,
        ),
        social_vocabulary: string_set(
            obj.get("social_vocabulary"),
            &format!("{path}.social_vocabulary"),
            limits,
            issues,
        ),
        task_cues: string_set(
            obj.get("task_cues"),
            &format!("{path}.task_cues"),
            limits,
            issues,
        ),
        weak_numeric_ignore: string_set(
            obj.get("weak_numeric_ignore"),
            &format!("{path}.weak_numeric_ignore"),
            limits,
            issues,
        ),
        continuation_exact_phrases: string_set(
            obj.get("continuation_exact_phrases"),
            &format!("{path}.continuation_exact_phrases"),
            limits,
            issues,
        ),
        continuation_question_starters: string_set(
            obj.get("continuation_question_starters"),
            &format!("{path}.continuation_question_starters"),
            limits,
            issues,
        ),
        continuation_references: string_set(
            obj.get("continuation_references"),
            &format!("{path}.continuation_references"),
            limits,
            issues,
        ),
        generic_followup_phrases: string_set(
            obj.get("generic_followup_phrases"),
            &format!("{path}.generic_followup_phrases"),
            limits,
            issues,
        ),
        boolean_values: parse_bool_map(
            obj.get("boolean_values"),
            &format!("{path}.boolean_values"),
            limits,
            issues,
        ),
        custom_entities: parse_custom_entities(
            obj.get("custom_entities"),
            &format!("{path}.custom_entities"),
            limits,
            issues,
        ),
        pattern_sets: BTreeMap::new(),
        number_words: parse_f64_map(
            obj.get("number_words"),
            &format!("{path}.number_words"),
            limits,
            issues,
        ),
        relative_dates: parse_string_map(
            obj.get("relative_dates"),
            &format!("{path}.relative_dates"),
            limits,
            issues,
        ),
        colors: parse_string_map(obj.get("colors"), &format!("{path}.colors"), limits, issues),
        units: parse_string_map(obj.get("units"), &format!("{path}.units"), limits, issues),
        time_markers: string_set(
            obj.get("time_markers"),
            &format!("{path}.time_markers"),
            limits,
            issues,
        ),
        origin_prefixes: parse_string_vec_map(
            obj.get("origin_prefixes"),
            &format!("{path}.origin_prefixes"),
            limits,
            issues,
        ),
    })
}

pub(in crate::source) fn parse_matcher_profile_data(
    value: &JsonValue,
    path: &str,
    limits: SourceLimits,
    issues: &mut Vec<SourceIssue>,
) -> Option<SemanticProfile> {
    let obj = expect_object(value, path, issues).ok()?;
    reject_unknown_keys(obj, &["pattern_sets"], path, issues);
    let mut profile = SemanticProfile::empty();
    profile.pattern_sets = parse_pattern_sets(
        obj.get("pattern_sets"),
        &format!("{path}.pattern_sets"),
        limits,
        issues,
    );
    Some(profile)
}
