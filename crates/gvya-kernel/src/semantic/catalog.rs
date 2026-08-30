//! Runtime semantic catalog types. Source-project/package syntax is compiler-owned later.

use gvya_model::{MeaningId, ReferenceKind};

use super::{
    entities::EntityKind,
    structural::{
        LocalizedStructuralPattern, SEMANTIC_PATTERNS_PER_MEANING_MAX,
        SEMANTIC_STRUCTURAL_RULES_MAX, validate_pattern,
    },
};

pub const SEMANTIC_SAMPLES_PER_MEANING_MAX: usize = 128;
pub const SEMANTIC_NEGATIVE_SAMPLES_PER_MEANING_MAX: usize = 128;
pub const SEMANTIC_RETRIEVAL_TERMS_PER_MEANING_MAX: usize = 128;
pub const SEMANTIC_TEXT_ITEM_MAX_BYTES: usize = 16 * 1024;
pub const SEMANTIC_TEXT_PER_MEANING_MAX_BYTES: usize = 256 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MeaningClass {
    General,
    Social,
    Clarification,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SlotKind {
    String,
    Number,
    Boolean,
    Entity(EntityKind),
    Reference(ReferenceKind),
}

/// Author-owned, localizable request used by the Conversation layer when collecting a value.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ElicitationPrompt {
    pub language: String,
    pub text: String,
}

impl ElicitationPrompt {
    #[must_use]
    pub fn new(language: impl Into<String>, text: impl Into<String>) -> Self {
        Self {
            language: language.into(),
            text: text.into(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SlotSpec {
    pub name: String,
    pub kind: SlotKind,
    pub required: bool,
    pub elicitation: Vec<ElicitationPrompt>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReferenceSpec {
    pub kind: ReferenceKind,
    pub required: bool,
    pub elicitation: Vec<ElicitationPrompt>,
}

/// Executable semantic meaning definition consumed by the canonical semantic kernel.
///
/// This is not a source authoring format; source syntax is compiler-owned and separate.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LocalizedText {
    pub language: String,
    pub text: String,
}

impl LocalizedText {
    #[must_use]
    pub fn new(language: impl Into<String>, text: impl Into<String>) -> Self {
        Self {
            language: language.into(),
            text: text.into(),
        }
    }
}

pub type LocalizedSample = LocalizedText;

#[derive(Clone, Debug, PartialEq)]
pub struct MeaningPattern {
    pub id: MeaningId,
    pub class: MeaningClass,
    /// Explicit whole-utterance structural rules. These are authoritative before semantic samples.
    pub patterns: Vec<LocalizedStructuralPattern>,
    pub samples: Vec<LocalizedSample>,
    pub negative_samples: Vec<LocalizedText>,
    /// Explicit language-tagged lexical retrieval metadata. It replaces hidden ID/topic-name semantics.
    pub retrieval_terms: Vec<LocalizedText>,
    pub priority: i32,
    pub positive_assumption: bool,
    pub slots: Vec<SlotSpec>,
    pub references: Vec<ReferenceSpec>,
}

impl MeaningPattern {
    #[must_use]
    pub fn new<I, S>(id: impl Into<String>, samples: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self {
            id: MeaningId::new(id.into()),
            class: MeaningClass::General,
            patterns: Vec::new(),
            samples: samples
                .into_iter()
                .map(|text| LocalizedSample::new("und", text))
                .collect(),
            negative_samples: Vec::new(),
            retrieval_terms: Vec::new(),
            priority: 1,
            positive_assumption: false,
            slots: Vec::new(),
            references: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CatalogError {
    EmptyMeaningId,
    DuplicateMeaningId(String),
    EmptySample(String),
    TooManyStructuralPatterns(String),
    TooManyStructuralPatternsTotal,
    InvalidStructuralPattern {
        meaning: String,
        pattern: String,
        reason: String,
    },
    DuplicateSlot {
        meaning: String,
        slot: String,
    },
    DuplicateReference {
        meaning: String,
        kind: String,
    },
    MissingRequiredElicitation {
        meaning: String,
        declaration: String,
    },
    InvalidElicitation {
        meaning: String,
        declaration: String,
    },
    TooManySamples(String),
    TooManyNegativeSamples(String),
    TooManyRetrievalTerms(String),
    SemanticTextItemTooLarge {
        meaning: String,
    },
    SemanticTextBudgetExceeded {
        meaning: String,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct SemanticCatalog {
    patterns: Vec<MeaningPattern>,
    by_id: std::collections::BTreeMap<String, usize>,
}

impl SemanticCatalog {
    /// Creates an empty, already-valid runtime catalog.
    #[must_use]
    pub fn empty() -> Self {
        Self {
            patterns: Vec::new(),
            by_id: std::collections::BTreeMap::new(),
        }
    }

    pub fn new(patterns: Vec<MeaningPattern>) -> Result<Self, CatalogError> {
        let structural_rule_count = patterns
            .iter()
            .map(|pattern| pattern.patterns.len())
            .sum::<usize>();
        if structural_rule_count > SEMANTIC_STRUCTURAL_RULES_MAX {
            return Err(CatalogError::TooManyStructuralPatternsTotal);
        }
        let mut ids = std::collections::BTreeSet::new();
        for pattern in &patterns {
            if pattern.id.as_str().trim().is_empty() {
                return Err(CatalogError::EmptyMeaningId);
            }
            if !ids.insert(pattern.id.as_str().to_string()) {
                return Err(CatalogError::DuplicateMeaningId(
                    pattern.id.as_str().to_string(),
                ));
            }
            if pattern.patterns.len() > SEMANTIC_PATTERNS_PER_MEANING_MAX {
                return Err(CatalogError::TooManyStructuralPatterns(
                    pattern.id.as_str().to_string(),
                ));
            }
            if pattern
                .samples
                .iter()
                .chain(pattern.negative_samples.iter())
                .chain(pattern.retrieval_terms.iter())
                .any(|sample| sample.language.trim().is_empty() || sample.text.trim().is_empty())
            {
                return Err(CatalogError::EmptySample(pattern.id.as_str().to_string()));
            }
            for rule in &pattern.patterns {
                if rule.language.trim().is_empty() || rule.text.trim().is_empty() {
                    return Err(CatalogError::EmptySample(pattern.id.as_str().to_string()));
                }
                if let Err(error) = validate_pattern(pattern, rule) {
                    return Err(CatalogError::InvalidStructuralPattern {
                        meaning: pattern.id.as_str().to_string(),
                        pattern: rule.text.clone(),
                        reason: format!("{error:?}"),
                    });
                }
            }
            if pattern.samples.len() > SEMANTIC_SAMPLES_PER_MEANING_MAX {
                return Err(CatalogError::TooManySamples(
                    pattern.id.as_str().to_string(),
                ));
            }
            if pattern.negative_samples.len() > SEMANTIC_NEGATIVE_SAMPLES_PER_MEANING_MAX {
                return Err(CatalogError::TooManyNegativeSamples(
                    pattern.id.as_str().to_string(),
                ));
            }
            if pattern.retrieval_terms.len() > SEMANTIC_RETRIEVAL_TERMS_PER_MEANING_MAX {
                return Err(CatalogError::TooManyRetrievalTerms(
                    pattern.id.as_str().to_string(),
                ));
            }
            let mut semantic_text_bytes = 0usize;
            for text in pattern
                .patterns
                .iter()
                .map(|rule| &rule.text)
                .chain(pattern.samples.iter().map(|sample| &sample.text))
                .chain(pattern.negative_samples.iter().map(|row| &row.text))
                .chain(pattern.retrieval_terms.iter().map(|row| &row.text))
                .chain(
                    pattern
                        .slots
                        .iter()
                        .flat_map(|slot| slot.elicitation.iter())
                        .map(|row| &row.text),
                )
                .chain(
                    pattern
                        .references
                        .iter()
                        .flat_map(|reference| reference.elicitation.iter())
                        .map(|row| &row.text),
                )
            {
                if text.len() > SEMANTIC_TEXT_ITEM_MAX_BYTES {
                    return Err(CatalogError::SemanticTextItemTooLarge {
                        meaning: pattern.id.as_str().to_string(),
                    });
                }
                semantic_text_bytes = semantic_text_bytes.saturating_add(text.len());
                if semantic_text_bytes > SEMANTIC_TEXT_PER_MEANING_MAX_BYTES {
                    return Err(CatalogError::SemanticTextBudgetExceeded {
                        meaning: pattern.id.as_str().to_string(),
                    });
                }
            }
            let mut slots = std::collections::BTreeSet::new();
            for slot in &pattern.slots {
                if !slots.insert(slot.name.clone()) {
                    return Err(CatalogError::DuplicateSlot {
                        meaning: pattern.id.as_str().to_string(),
                        slot: slot.name.clone(),
                    });
                }
                validate_elicitation(
                    pattern.id.as_str(),
                    &format!("slot:{}", slot.name),
                    slot.required,
                    &slot.elicitation,
                )?;
            }
            let mut references = std::collections::BTreeSet::new();
            for reference in &pattern.references {
                if !references.insert(reference.kind.clone()) {
                    return Err(CatalogError::DuplicateReference {
                        meaning: pattern.id.as_str().to_string(),
                        kind: reference.kind.as_str().to_string(),
                    });
                }
                validate_elicitation(
                    pattern.id.as_str(),
                    &format!("reference:{}", reference.kind.as_str()),
                    reference.required,
                    &reference.elicitation,
                )?;
            }
        }
        let by_id = patterns
            .iter()
            .enumerate()
            .map(|(index, pattern)| (pattern.id.as_str().to_string(), index))
            .collect();
        Ok(Self { patterns, by_id })
    }

    #[must_use]
    pub fn patterns(&self) -> &[MeaningPattern] {
        &self.patterns
    }

    #[must_use]
    pub fn get(&self, id: &MeaningId) -> Option<&MeaningPattern> {
        self.get_with_index(id).map(|(_, pattern)| pattern)
    }

    #[must_use]
    pub fn get_with_index(&self, id: &MeaningId) -> Option<(usize, &MeaningPattern)> {
        let index = *self.by_id.get(id.as_str())?;
        self.patterns.get(index).map(|pattern| (index, pattern))
    }
}

fn validate_elicitation(
    meaning: &str,
    declaration: &str,
    required: bool,
    prompts: &[ElicitationPrompt],
) -> Result<(), CatalogError> {
    if required && prompts.is_empty() {
        return Err(CatalogError::MissingRequiredElicitation {
            meaning: meaning.to_string(),
            declaration: declaration.to_string(),
        });
    }
    let mut languages = std::collections::BTreeSet::new();
    if prompts.iter().any(|prompt| {
        prompt.language.trim().is_empty()
            || prompt.text.trim().is_empty()
            || !languages.insert(prompt.language.trim().to_ascii_lowercase())
    }) {
        return Err(CatalogError::InvalidElicitation {
            meaning: meaning.to_string(),
            declaration: declaration.to_string(),
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matcher_work_budget_rejects_excess_samples_and_text() {
        let too_many = MeaningPattern::new(
            "too.many",
            (0..=SEMANTIC_SAMPLES_PER_MEANING_MAX).map(|index| format!("sample {index}")),
        );
        assert!(matches!(
            SemanticCatalog::new(vec![too_many]),
            Err(CatalogError::TooManySamples(_))
        ));

        let too_large =
            MeaningPattern::new("too.large", ["x".repeat(SEMANTIC_TEXT_ITEM_MAX_BYTES + 1)]);
        assert!(matches!(
            SemanticCatalog::new(vec![too_large]),
            Err(CatalogError::SemanticTextItemTooLarge { .. })
        ));
    }

    #[test]
    fn required_values_need_nonempty_unique_localized_elicitation() {
        let mut missing = MeaningPattern::new("missing", ["missing"]);
        missing.slots.push(SlotSpec {
            name: "value".into(),
            kind: SlotKind::String,
            required: true,
            elicitation: Vec::new(),
        });
        assert!(matches!(
            SemanticCatalog::new(vec![missing]),
            Err(CatalogError::MissingRequiredElicitation { .. })
        ));

        let mut duplicate = MeaningPattern::new("duplicate", ["duplicate"]);
        duplicate.references.push(ReferenceSpec {
            kind: gvya_model::ReferenceKind::new("door"),
            required: true,
            elicitation: vec![
                ElicitationPrompt::new("en", "Which door?"),
                ElicitationPrompt::new("EN", "Choose a door."),
            ],
        });
        assert!(matches!(
            SemanticCatalog::new(vec![duplicate]),
            Err(CatalogError::InvalidElicitation { .. })
        ));
    }
}
