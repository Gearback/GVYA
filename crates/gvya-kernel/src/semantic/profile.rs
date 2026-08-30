//! Explicit authored lexical/language policy for the deterministic semantic kernel.

use std::collections::{BTreeMap, BTreeSet};

use super::normalization::{language_fallbacks, normalize_language_tag, normalize_text};

/// Executable language policy keyed by canonical BCP47 language tag.
///
/// Language Profiles and their paired Matcher Profiles remain isolated all the way into the
/// runtime. A turn selects one explicit profile from this map; lexical policy from another
/// language can never leak into normalization, morphology, weighting or structural sets.
pub type SemanticProfiles = BTreeMap<String, SemanticProfile>;

/// Select the first explicitly available profile in the request/fallback language chain.
/// No natural language and no `und` profile is injected by ambient policy.
#[must_use]
pub fn profile_for_language<'a>(
    profiles: &'a SemanticProfiles,
    requested: Option<&str>,
    explicit_fallbacks: &[String],
) -> Option<&'a SemanticProfile> {
    language_fallbacks(requested, explicit_fallbacks)
        .into_iter()
        .find_map(|language| profiles.get(&language))
}

/// Resolve an authored localized row to its exact language profile.
#[must_use]
pub fn profile_for_authored_language<'a>(
    profiles: &'a SemanticProfiles,
    language: &str,
) -> Option<&'a SemanticProfile> {
    profiles.get(&normalize_language_tag(language))
}

/// Author-extendable lexical profile used by the deterministic semantic kernel.
///
/// The kernel contains no built-in language-specific vocabulary, morphology table or script
/// folding. Generic bounded token similarity remains part of the language-neutral matcher.
/// Every language-specific behavior comes from an explicit standalone Language Profile; structural pattern sets come from its paired Matcher Profile.
#[derive(Clone, Debug, PartialEq)]
pub struct SemanticProfile {
    /// Explicit token canonicalization (for example authored singular/plural aliases).
    pub canonical_tokens: BTreeMap<String, String>,
    /// Explicit suffix rewrites applied to tokens that have no exact canonical-token mapping.
    /// The longest matching suffix wins and at least three stem characters must remain.
    pub canonical_suffixes: BTreeMap<String, String>,
    /// Exact normalized tokens that must never be rewritten by `canonical_suffixes`.
    /// This keeps productive authored suffix rules bounded around known lexical confounders.
    pub canonical_suffix_exceptions: BTreeSet<String>,
    /// Standalone suffix tokens removed after authored phrase normalization when they follow a stem.
    pub detached_suffixes: BTreeSet<String>,
    /// Explicit single-character rewrites applied after Unicode NFC/lowercase normalization.
    pub normalization_rewrites: BTreeMap<String, String>,
    /// Explicit characters to remove after generic normalization (for authored diacritic policy).
    pub normalization_remove_chars: BTreeSet<String>,
    pub colloquial: BTreeMap<String, Vec<String>>,
    pub pure_glue: BTreeSet<String>,
    pub very_low_weight: BTreeSet<String>,
    pub low_weight: BTreeSet<String>,
    pub context_low_weight: BTreeSet<String>,
    pub generic_singletons: BTreeSet<String>,
    pub reporting_verbs: BTreeSet<String>,
    pub reporting_nouns: BTreeSet<String>,
    pub pronouns: BTreeSet<String>,
    pub negations: BTreeSet<String>,
    pub social_vocabulary: BTreeSet<String>,
    pub task_cues: BTreeSet<String>,
    /// Tokens that must not by themselves make an otherwise numeric-only match strong.
    pub weak_numeric_ignore: BTreeSet<String>,
    /// Exact short phrases allowed to continue current context.
    pub continuation_exact_phrases: BTreeSet<String>,
    /// Question starters that prevent implicit contextual continuation.
    pub continuation_question_starters: BTreeSet<String>,
    /// Referential tokens accepted as contextual continuation cues.
    pub continuation_references: BTreeSet<String>,
    /// Weak phrases that may continue the previous meaning only after standalone semantics fail.
    pub generic_followup_phrases: BTreeSet<String>,
    /// Exact authored phrase -> boolean value vocabulary used by typed Boolean collection.
    pub boolean_values: BTreeMap<String, bool>,
    /// Bounded authored entity catalogs: entity kind -> canonical value -> surface aliases.
    pub custom_entities: BTreeMap<String, BTreeMap<String, Vec<String>>>,
    /// Explicit named sets used by structural patterns. Alias text maps to the canonical captured value.
    pub pattern_sets: BTreeMap<String, BTreeMap<String, String>>,
    /// Explicit word-number vocabulary.
    pub number_words: BTreeMap<String, f64>,
    /// Explicit relative-date token -> canonical symbolic value.
    pub relative_dates: BTreeMap<String, String>,
    /// Explicit color token -> canonical color value.
    pub colors: BTreeMap<String, String>,
    /// Explicit quantity-unit token -> canonical unit value.
    pub units: BTreeMap<String, String>,
    /// Explicit natural-language time markers (for example an authored equivalent of `at`).
    pub time_markers: BTreeSet<String>,
    /// Normalized origin-introduction phrase -> stop-token list.
    pub origin_prefixes: BTreeMap<String, Vec<String>>,
}

impl Default for SemanticProfile {
    fn default() -> Self {
        Self::empty()
    }
}

impl SemanticProfile {
    /// Fully language-neutral profile. This is the canonical default; lexical behavior must be
    /// selected explicitly by source language-profile composition.
    #[must_use]
    pub fn empty() -> Self {
        Self {
            canonical_tokens: BTreeMap::new(),
            canonical_suffixes: BTreeMap::new(),
            canonical_suffix_exceptions: BTreeSet::new(),
            detached_suffixes: BTreeSet::new(),
            normalization_rewrites: BTreeMap::new(),
            normalization_remove_chars: BTreeSet::new(),
            colloquial: BTreeMap::new(),
            pure_glue: BTreeSet::new(),
            very_low_weight: BTreeSet::new(),
            low_weight: BTreeSet::new(),
            context_low_weight: BTreeSet::new(),
            generic_singletons: BTreeSet::new(),
            reporting_verbs: BTreeSet::new(),
            reporting_nouns: BTreeSet::new(),
            pronouns: BTreeSet::new(),
            negations: BTreeSet::new(),
            social_vocabulary: BTreeSet::new(),
            task_cues: BTreeSet::new(),
            weak_numeric_ignore: BTreeSet::new(),
            continuation_exact_phrases: BTreeSet::new(),
            continuation_question_starters: BTreeSet::new(),
            continuation_references: BTreeSet::new(),
            generic_followup_phrases: BTreeSet::new(),
            boolean_values: BTreeMap::new(),
            custom_entities: BTreeMap::new(),
            pattern_sets: BTreeMap::new(),
            number_words: BTreeMap::new(),
            relative_dates: BTreeMap::new(),
            colors: BTreeMap::new(),
            units: BTreeMap::new(),
            time_markers: BTreeSet::new(),
            origin_prefixes: BTreeMap::new(),
        }
    }

    #[must_use]
    pub fn rewrite_characters(&self, input: &str) -> String {
        if self.normalization_rewrites.is_empty() && self.normalization_remove_chars.is_empty() {
            return input.to_string();
        }
        let mut rewritten = String::with_capacity(input.len());
        for ch in input.chars() {
            let key = ch.to_string();
            if self.normalization_remove_chars.contains(&key) {
                continue;
            }
            if let Some(value) = self.normalization_rewrites.get(&key) {
                rewritten.push_str(value);
            } else {
                rewritten.push(ch);
            }
        }
        rewritten
    }

    #[must_use]
    pub fn normalize_text(&self, input: &str) -> String {
        normalize_text(&self.rewrite_characters(input))
    }

    #[must_use]
    pub fn normalize_colloquial_tokens(&self, tokens: &[String]) -> Vec<String> {
        let mut out = Vec::new();
        let mut index = 0;
        while index < tokens.len() {
            let longest = self
                .colloquial
                .iter()
                .filter_map(|(phrase, replacement)| {
                    let phrase_tokens: Vec<&str> = phrase.split_whitespace().collect();
                    let length = phrase_tokens.len();
                    if length == 0
                        || index + length > tokens.len()
                        || !phrase_tokens
                            .iter()
                            .zip(&tokens[index..index + length])
                            .all(|(expected, actual)| *expected == actual)
                    {
                        return None;
                    }
                    Some((length, replacement))
                })
                .max_by_key(|(length, _)| *length);
            if let Some((length, replacement)) = longest {
                out.extend(replacement.iter().cloned());
                index += length;
            } else {
                out.push(tokens[index].clone());
                index += 1;
            }
        }
        out.into_iter().fold(Vec::new(), |mut normalized, token| {
            if !normalized.is_empty() && self.detached_suffixes.contains(&token) {
                return normalized;
            }
            normalized.push(token);
            normalized
        })
    }

    #[must_use]
    pub fn canonical_token(&self, token: &str) -> String {
        let normalized = self.normalize_text(token);
        if let Some(canonical) = self.canonical_tokens.get(&normalized) {
            return canonical.clone();
        }
        if self.canonical_suffix_exceptions.contains(&normalized) {
            return normalized;
        }
        let suffix_rewrite = self
            .canonical_suffixes
            .iter()
            .filter_map(|(suffix, replacement)| {
                let stem = normalized.strip_suffix(suffix)?;
                (stem.chars().count() >= 3).then_some((suffix.chars().count(), stem, replacement))
            })
            .max_by_key(|(suffix_length, _, _)| *suffix_length);
        if let Some((_, stem, replacement)) = suffix_rewrite {
            format!("{stem}{replacement}")
        } else {
            normalized
        }
    }

    #[must_use]
    pub fn strict_token_match(&self, left: &str, right: &str) -> bool {
        left == right || self.canonical_token(left) == self.canonical_token(right)
    }

    #[must_use]
    pub fn token_weight(&self, token: &str) -> f64 {
        if self.very_low_weight.contains(token) {
            0.20
        } else if self.context_low_weight.contains(token) {
            0.40
        } else if self.low_weight.contains(token) {
            0.35
        } else {
            1.0
        }
    }

    /// Integer retrieval-only weight. Candidate ranking must not depend on floating-point math.
    #[must_use]
    pub fn index_token_weight_milli(&self, token: &str) -> u64 {
        let len = token.chars().count();
        if len >= 8 {
            1_350
        } else if len >= 5 {
            1_150
        } else if len <= 2 {
            650
        } else {
            1_000
        }
    }

    #[must_use]
    pub fn is_pure_glue(&self, token: &str) -> bool {
        self.pure_glue.contains(token)
    }

    #[must_use]
    pub fn content_tokens(&self, tokens: &[String]) -> Vec<String> {
        tokens
            .iter()
            .filter(|token| !self.is_pure_glue(token))
            .cloned()
            .collect()
    }

    #[must_use]
    pub fn is_generic_singleton(&self, token: &str) -> bool {
        self.generic_singletons.contains(token)
    }

    #[must_use]
    pub fn has_negation(&self, tokens: &[String]) -> bool {
        tokens.iter().any(|token| self.negations.contains(token))
    }

    #[must_use]
    pub fn has_pronoun(&self, tokens: &[String]) -> bool {
        tokens.iter().any(|token| self.pronouns.contains(token))
    }

    #[must_use]
    pub fn is_question_starter(&self, token: &str) -> bool {
        self.continuation_question_starters.contains(token)
    }

    #[must_use]
    pub fn is_task_cue(&self, token: &str) -> bool {
        self.task_cues.contains(token)
    }

    #[must_use]
    pub fn has_non_social_content(&self, tokens: &[String]) -> bool {
        self.content_tokens(&self.normalize_colloquial_tokens(tokens))
            .iter()
            .any(|token| !self.social_vocabulary.contains(token))
    }

    #[must_use]
    pub fn has_reporting_context(&self, tokens: &[String]) -> bool {
        if tokens.len() <= 2 {
            return false;
        }
        let has_verb = tokens
            .iter()
            .any(|token| self.reporting_verbs.contains(token));
        if !has_verb {
            return false;
        }
        let has_noun = tokens
            .iter()
            .any(|token| self.reporting_nouns.contains(token));
        has_noun || tokens.len() >= 4
    }

    #[must_use]
    pub fn has_reporting_context_before_span(&self, tokens: &[String], span_start: usize) -> bool {
        if span_start == 0 || span_start > tokens.len() {
            return false;
        }
        tokens[..span_start]
            .iter()
            .any(|token| self.reporting_verbs.contains(token))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_string()).collect()
    }

    #[test]
    fn default_profile_is_language_neutral() {
        let profile = SemanticProfile::default();
        assert!(profile.colloquial.is_empty());
        assert!(profile.canonical_tokens.is_empty());
        assert!(profile.canonical_suffixes.is_empty());
        assert!(profile.canonical_suffix_exceptions.is_empty());
        assert!(profile.detached_suffixes.is_empty());
        assert_eq!(
            profile.normalize_colloquial_tokens(&words(&["u", "dogs"])),
            words(&["u", "dogs"])
        );
        assert_eq!(profile.canonical_token("dogs"), "dogs");
    }

    #[test]
    fn token_and_script_rewrites_are_authored_data_only() {
        let mut profile = SemanticProfile::empty();
        profile.canonical_tokens.insert("dogs".into(), "dog".into());
        profile
            .normalization_rewrites
            .insert("ي".into(), "ی".into());
        assert_eq!(profile.canonical_token("dogs"), "dog");
        assert_eq!(profile.normalize_text("ي"), "ی");
    }

    #[test]
    fn authored_suffixes_and_longest_phrase_rewrites_are_deterministic() {
        let mut profile = SemanticProfile::empty();
        profile
            .canonical_suffixes
            .extend([("ies".into(), "y".into()), ("s".into(), String::new())]);
        profile
            .canonical_tokens
            .insert("this".into(), "this".into());
        profile.canonical_suffix_exceptions.extend(
            [
                "news",
                "focus",
                "status",
                "plus",
                "less",
                "perhaps",
                "ambiguous",
            ]
            .into_iter()
            .map(str::to_string),
        );
        profile.detached_suffixes.insert("ها".into());
        profile
            .colloquial
            .insert("would you please".into(), vec!["please".into()]);
        profile
            .colloquial
            .insert("would you".into(), vec!["please".into()]);

        assert_eq!(profile.canonical_token("stories"), "story");
        assert_eq!(profile.canonical_token("packages"), "package");
        assert_eq!(profile.canonical_token("this"), "this");
        assert_eq!(profile.canonical_token("is"), "is");
        for confounder in [
            "news",
            "focus",
            "status",
            "plus",
            "less",
            "perhaps",
            "ambiguous",
        ] {
            assert_eq!(profile.canonical_token(confounder), confounder);
        }
        assert_eq!(
            profile.normalize_colloquial_tokens(&words(&["would", "you", "please", "help"])),
            words(&["please", "help"])
        );
        assert_eq!(
            profile.normalize_colloquial_tokens(&words(&["پکیج", "ها"])),
            words(&["پکیج"])
        );
        assert_eq!(
            profile.normalize_colloquial_tokens(&words(&["ها"])),
            words(&["ها"])
        );
    }
}
