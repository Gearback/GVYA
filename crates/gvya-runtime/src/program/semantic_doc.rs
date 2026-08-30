//! Semantic executable document hydration.

use super::*;
use gvya_kernel::semantic::LocalizedStructuralPattern;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct SemanticDoc {
    pub(super) config: SemanticConfigDoc,
    pub(super) profiles: BTreeMap<String, SemanticProfileDoc>,
    pub(super) patterns: Vec<MeaningPatternDoc>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct SemanticConfigDoc {
    pub(super) candidate_limit: usize,
    pub(super) resolution_threshold: f64,
    pub(super) ambiguity_margin: f64,
    pub(super) resolver_min_confidence: f32,
    pub(super) resolver_candidate_limit: usize,
}
impl SemanticConfigDoc {
    pub(super) fn into_runtime(self) -> Result<SemanticConfig, ProgramError> {
        for (name, value) in [
            ("semantic.resolution_threshold", self.resolution_threshold),
            ("semantic.ambiguity_margin", self.ambiguity_margin),
            (
                "semantic.resolver_min_confidence",
                f64::from(self.resolver_min_confidence),
            ),
        ] {
            if !value.is_finite() {
                return Err(ProgramError::NonFiniteNumber(name));
            }
        }
        if !(SEMANTIC_CANDIDATE_LIMIT_MIN..=SEMANTIC_CANDIDATE_LIMIT_MAX)
            .contains(&self.candidate_limit)
        {
            return Err(ProgramError::InvalidSemanticConfig(
                "candidate_limit outside canonical range".into(),
            ));
        }
        if !(SEMANTIC_RESOLUTION_THRESHOLD_MIN..=SEMANTIC_RESOLUTION_THRESHOLD_MAX)
            .contains(&self.resolution_threshold)
        {
            return Err(ProgramError::InvalidSemanticConfig(
                "resolution_threshold outside canonical range".into(),
            ));
        }
        if !(SEMANTIC_AMBIGUITY_MARGIN_MIN..=SEMANTIC_AMBIGUITY_MARGIN_MAX)
            .contains(&self.ambiguity_margin)
        {
            return Err(ProgramError::InvalidSemanticConfig(
                "ambiguity_margin outside canonical range".into(),
            ));
        }
        if !(SEMANTIC_RESOLVER_CONFIDENCE_MIN..=SEMANTIC_RESOLVER_CONFIDENCE_MAX)
            .contains(&self.resolver_min_confidence)
        {
            return Err(ProgramError::InvalidSemanticConfig(
                "resolver_min_confidence outside canonical range".into(),
            ));
        }
        if !(SEMANTIC_RESOLVER_CANDIDATE_LIMIT_MIN..=SEMANTIC_RESOLVER_CANDIDATE_LIMIT_MAX)
            .contains(&self.resolver_candidate_limit)
        {
            return Err(ProgramError::InvalidSemanticConfig(
                "resolver_candidate_limit outside canonical range".into(),
            ));
        }
        Ok(SemanticConfig {
            candidate_limit: self.candidate_limit,
            resolution_threshold: self.resolution_threshold,
            ambiguity_margin: self.ambiguity_margin,
            resolver_min_confidence: self.resolver_min_confidence,
            resolver_candidate_limit: self.resolver_candidate_limit,
        })
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct SemanticProfileDoc {
    canonical_tokens: BTreeMap<String, String>,
    canonical_suffixes: BTreeMap<String, String>,
    canonical_suffix_exceptions: BTreeSet<String>,
    detached_suffixes: BTreeSet<String>,
    normalization_rewrites: BTreeMap<String, String>,
    normalization_remove_chars: BTreeSet<String>,
    colloquial: BTreeMap<String, Vec<String>>,
    pure_glue: BTreeSet<String>,
    very_low_weight: BTreeSet<String>,
    low_weight: BTreeSet<String>,
    context_low_weight: BTreeSet<String>,
    generic_singletons: BTreeSet<String>,
    reporting_verbs: BTreeSet<String>,
    reporting_nouns: BTreeSet<String>,
    pronouns: BTreeSet<String>,
    negations: BTreeSet<String>,
    social_vocabulary: BTreeSet<String>,
    task_cues: BTreeSet<String>,
    weak_numeric_ignore: BTreeSet<String>,
    continuation_exact_phrases: BTreeSet<String>,
    continuation_question_starters: BTreeSet<String>,
    continuation_references: BTreeSet<String>,
    generic_followup_phrases: BTreeSet<String>,
    boolean_values: BTreeMap<String, bool>,
    custom_entities: BTreeMap<String, BTreeMap<String, Vec<String>>>,
    pattern_sets: BTreeMap<String, BTreeMap<String, String>>,
    number_words: BTreeMap<String, f64>,
    relative_dates: BTreeMap<String, String>,
    colors: BTreeMap<String, String>,
    units: BTreeMap<String, String>,
    time_markers: BTreeSet<String>,
    origin_prefixes: BTreeMap<String, Vec<String>>,
}
impl SemanticProfileDoc {
    pub(super) fn into_runtime(self) -> SemanticProfile {
        SemanticProfile {
            canonical_tokens: self.canonical_tokens,
            canonical_suffixes: self.canonical_suffixes,
            canonical_suffix_exceptions: self.canonical_suffix_exceptions,
            detached_suffixes: self.detached_suffixes,
            normalization_rewrites: self.normalization_rewrites,
            normalization_remove_chars: self.normalization_remove_chars,
            colloquial: self.colloquial,
            pure_glue: self.pure_glue,
            very_low_weight: self.very_low_weight,
            low_weight: self.low_weight,
            context_low_weight: self.context_low_weight,
            generic_singletons: self.generic_singletons,
            reporting_verbs: self.reporting_verbs,
            reporting_nouns: self.reporting_nouns,
            pronouns: self.pronouns,
            negations: self.negations,
            social_vocabulary: self.social_vocabulary,
            task_cues: self.task_cues,
            weak_numeric_ignore: self.weak_numeric_ignore,
            continuation_exact_phrases: self.continuation_exact_phrases,
            continuation_question_starters: self.continuation_question_starters,
            continuation_references: self.continuation_references,
            generic_followup_phrases: self.generic_followup_phrases,
            boolean_values: self.boolean_values,
            custom_entities: self.custom_entities,
            pattern_sets: self.pattern_sets,
            number_words: self.number_words,
            relative_dates: self.relative_dates,
            colors: self.colors,
            units: self.units,
            time_markers: self.time_markers,
            origin_prefixes: self.origin_prefixes,
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct MeaningPatternDoc {
    id: String,
    class: String,
    patterns: Vec<StructuralPatternDoc>,
    samples: Vec<LocalizedSampleDoc>,
    negative_samples: Vec<LocalizedSampleDoc>,
    retrieval_terms: Vec<LocalizedSampleDoc>,
    priority: i32,
    positive_assumption: bool,
    slots: Vec<SlotDoc>,
    references: Vec<ReferenceSpecDoc>,
}
impl MeaningPatternDoc {
    pub(super) fn into_runtime(self) -> Result<MeaningPattern, ProgramError> {
        let class = match self.class.as_str() {
            "general" => MeaningClass::General,
            "social" => MeaningClass::Social,
            "clarification" => MeaningClass::Clarification,
            _ => {
                return Err(ProgramError::InvalidSemanticCatalog(format!(
                    "unknown meaning class: {}",
                    self.class
                )));
            }
        };
        Ok(MeaningPattern {
            id: MeaningId::new(self.id),
            class,
            patterns: self
                .patterns
                .into_iter()
                .map(|rule| LocalizedStructuralPattern {
                    language: rule.language,
                    text: rule.text,
                    priority: rule.priority,
                })
                .collect(),
            samples: self
                .samples
                .into_iter()
                .map(|sample| LocalizedSample::new(sample.language, sample.text))
                .collect(),
            negative_samples: self
                .negative_samples
                .into_iter()
                .map(|sample| LocalizedSample::new(sample.language, sample.text))
                .collect(),
            retrieval_terms: self
                .retrieval_terms
                .into_iter()
                .map(|sample| LocalizedSample::new(sample.language, sample.text))
                .collect(),
            priority: self.priority,
            positive_assumption: self.positive_assumption,
            slots: self
                .slots
                .into_iter()
                .map(SlotDoc::into_runtime)
                .collect::<Result<_, _>>()?,
            references: self
                .references
                .into_iter()
                .map(|row| ReferenceSpec {
                    kind: ReferenceKind::new(row.kind),
                    required: row.required,
                    elicitation: row
                        .elicitation
                        .into_iter()
                        .map(|prompt| ElicitationPrompt::new(prompt.language, prompt.text))
                        .collect(),
                })
                .collect(),
        })
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct StructuralPatternDoc {
    language: String,
    text: String,
    priority: i32,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct LocalizedSampleDoc {
    language: String,
    text: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct SlotDoc {
    name: String,
    kind: SlotKindDoc,
    required: bool,
    elicitation: Vec<LocalizedSampleDoc>,
}
impl SlotDoc {
    pub(super) fn into_runtime(self) -> Result<SlotSpec, ProgramError> {
        Ok(SlotSpec {
            name: self.name,
            kind: self.kind.into_runtime()?,
            required: self.required,
            elicitation: self
                .elicitation
                .into_iter()
                .map(|prompt| ElicitationPrompt::new(prompt.language, prompt.text))
                .collect(),
        })
    }
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum SlotKindDoc {
    String,
    Number,
    Boolean,
    Entity { entity_kind: String },
    Reference { reference_kind: String },
}
impl SlotKindDoc {
    pub(super) fn into_runtime(self) -> Result<SlotKind, ProgramError> {
        Ok(match self {
            Self::String => SlotKind::String,
            Self::Number => SlotKind::Number,
            Self::Boolean => SlotKind::Boolean,
            Self::Entity { entity_kind } => SlotKind::Entity(EntityKind::new(entity_kind)),
            Self::Reference { reference_kind } => {
                SlotKind::Reference(ReferenceKind::new(reference_kind))
            }
        })
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ReferenceSpecDoc {
    kind: String,
    required: bool,
    elicitation: Vec<LocalizedSampleDoc>,
}
