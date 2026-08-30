//! Inverted semantic index, IDF-lite candidate retrieval and bounded typo-lite correction.

use std::collections::{BTreeMap, BTreeSet};

const RETRIEVAL_POSTING_VISIT_MULTIPLIER: usize = 32;
const RETRIEVAL_POSTINGS_PER_KEY_MULTIPLIER: usize = 4;
const RETRIEVAL_POSTING_VISIT_MAX: usize = 8_192;
const RETRIEVAL_POSTINGS_PER_KEY_MAX: usize = 1_024;
const RETRIEVAL_FEATURES_PER_CLASS_MAX: usize = 64;
const TYPO_SIGNATURE_BUCKET_MAX: usize = 128;
const TYPO_QUERY_TOKENS_MAX: usize = 32;
const FUZZY_SIGNATURE_BUCKET_MAX: usize = 128;
const FUZZY_QUERY_TOKENS_MAX: usize = 32;
const FUZZY_CANDIDATE_TOKENS_MAX: usize = 16;
const EMBEDDED_EXACT_SAMPLE_TOKENS_MAX: usize = 8;
const EMBEDDED_EXACT_SAMPLE_QUERY_TOKENS_MAX: usize = 24;
const EMBEDDED_EXACT_SAMPLE_WINDOWS_MAX: usize = 160;
const PHRASE_TYPO_SIGNATURE_BUCKET_MAX: usize = 128;
const PHRASE_TYPO_QUERY_SIGNATURES_MAX: usize = 24;
const PHRASE_TYPO_CANDIDATES_MAX: usize = 32;
const PHRASE_TYPO_POSTING_VISITS_MAX: usize = 2_048;
pub const SEMANTIC_EXACT_FANOUT_MAX: usize = 256;
use super::{
    catalog::SemanticCatalog,
    normalization::{language_fallbacks, normalize_language_tag, ordered_tokens},
    profile::{SemanticProfile, SemanticProfiles, profile_for_authored_language},
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TypoCorrection {
    pub from: String,
    pub to: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TypoLiteResult {
    pub tokens: Vec<String>,
    pub content_tokens: Vec<String>,
    pub corrections: Vec<TypoCorrection>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CandidateRow {
    pub pattern_index: usize,
    /// Deterministic integer retrieval rank in canonical milli-units.
    pub rank_milli: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CandidateDecision {
    pub use_full_scan: bool,
    pub reason: &'static str,
    pub rows: Vec<CandidateRow>,
    pub total_patterns: usize,
    pub posting_visits: usize,
    pub posting_saturated: bool,
    pub saturated_keys: usize,
    pub typo_lite: Option<TypoLiteResult>,
}

#[derive(Clone, Debug, Default, PartialEq)]
struct TypoSignatureBucket {
    values: Vec<String>,
    overflow: bool,
}

#[derive(Clone, Debug, Default, PartialEq)]
struct PatternSignatureBucket {
    rows: Vec<usize>,
    overflow: bool,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct SemanticIndex {
    exact_sample: BTreeMap<String, BTreeSet<usize>>,
    token: BTreeMap<String, BTreeSet<usize>>,
    content_token: BTreeMap<String, BTreeSet<usize>>,
    bigram: BTreeMap<String, BTreeSet<usize>>,
    content_bigram: BTreeMap<String, BTreeSet<usize>>,
    meta_token: BTreeMap<String, BTreeSet<usize>>,
    meta_bigram: BTreeMap<String, BTreeSet<usize>>,
    sample_start_bigram: BTreeMap<String, BTreeSet<usize>>,
    exact_content: BTreeMap<String, BTreeSet<usize>>,
    known_typo_tokens: BTreeSet<String>,
    typo_by_length: BTreeMap<usize, Vec<String>>,
    typo_signatures: BTreeMap<String, TypoSignatureBucket>,
    fuzzy_signatures: BTreeMap<String, TypoSignatureBucket>,
    phrase_typo_signatures: BTreeMap<String, PatternSignatureBucket>,
    pattern_count: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SemanticIndexBuildError {
    MissingLanguageProfile(String),
    ExactFanoutExceeded {
        table: &'static str,
        key: String,
        count: usize,
    },
}

impl SemanticIndex {
    pub fn build(
        catalog: &SemanticCatalog,
        profiles: &SemanticProfiles,
    ) -> Result<Self, SemanticIndexBuildError> {
        let mut index = Self {
            pattern_count: catalog.patterns().len(),
            ..Self::default()
        };
        for (pattern_index, pattern) in catalog.patterns().iter().enumerate() {
            for term in &pattern.retrieval_terms {
                let language = normalize_language_tag(&term.language);
                let profile =
                    profile_for_authored_language(profiles, &term.language).ok_or_else(|| {
                        SemanticIndexBuildError::MissingLanguageProfile(language.clone())
                    })?;
                let metadata = ordered_tokens(
                    &profile.normalize_text(&term.text.replace(['_', '/', '.', '-'], " ")),
                );
                add_index_tokens(
                    &mut index.meta_token,
                    &mut index.meta_bigram,
                    pattern_index,
                    &language,
                    &metadata,
                    profile,
                );
            }
            for sample in &pattern.samples {
                let language = normalize_language_tag(&sample.language);
                let profile = profile_for_authored_language(profiles, &sample.language)
                    .ok_or_else(|| {
                        SemanticIndexBuildError::MissingLanguageProfile(language.clone())
                    })?;
                let normalized = profile.normalize_text(&sample.text);
                if normalized.is_empty() {
                    continue;
                }
                add_localized(
                    &mut index.exact_sample,
                    &language,
                    &normalized,
                    pattern_index,
                );
                let tokens = profile.normalize_colloquial_tokens(&ordered_tokens(&normalized));
                let content = profile.content_tokens(&tokens);
                for token in &tokens {
                    if profile.is_pure_glue(token) {
                        continue;
                    }
                    for key in index_token_keys(profile, token) {
                        add_localized(&mut index.token, &language, &key, pattern_index);
                    }
                }
                for token in &content {
                    for key in index_token_keys(profile, token) {
                        add_localized(&mut index.content_token, &language, &key, pattern_index);
                    }
                }
                for bigram in adjacent_bigrams(&tokens)
                    .into_iter()
                    .chain(adjacent_bigrams(&canonicalize(profile, &tokens)))
                {
                    add_localized(&mut index.bigram, &language, &bigram, pattern_index);
                }
                for bigram in adjacent_bigrams(&content)
                    .into_iter()
                    .chain(adjacent_bigrams(&canonicalize(profile, &content)))
                {
                    add_localized(&mut index.content_bigram, &language, &bigram, pattern_index);
                }
                if !content.is_empty() {
                    let canonical_content = canonicalize(profile, &content);
                    if content.len() > 1 {
                        let raw_start = format!("{} {}", content[0], content[1]);
                        add_localized(
                            &mut index.sample_start_bigram,
                            &language,
                            &raw_start,
                            pattern_index,
                        );
                        if canonical_content.len() > 1 {
                            let canonical_start =
                                format!("{} {}", canonical_content[0], canonical_content[1]);
                            if canonical_start != raw_start {
                                add_localized(
                                    &mut index.sample_start_bigram,
                                    &language,
                                    &canonical_start,
                                    pattern_index,
                                );
                            }
                        }
                    }
                    let raw_content = content.join(" ");
                    add_localized(
                        &mut index.exact_content,
                        &language,
                        &raw_content,
                        pattern_index,
                    );
                    let canonical_content = canonical_content.join(" ");
                    if canonical_content != raw_content {
                        add_localized(
                            &mut index.exact_content,
                            &language,
                            &canonical_content,
                            pattern_index,
                        );
                    }
                }
            }
        }
        for (table, map) in [
            ("exact_sample", &index.exact_sample),
            ("exact_content", &index.exact_content),
        ] {
            if let Some((key, rows)) = map
                .iter()
                .find(|(_, rows)| rows.len() > SEMANTIC_EXACT_FANOUT_MAX)
            {
                return Err(SemanticIndexBuildError::ExactFanoutExceeded {
                    table,
                    key: key.clone(),
                    count: rows.len(),
                });
            }
        }
        index.rebuild_typo_dictionary();
        index.rebuild_fuzzy_dictionary();
        index.rebuild_phrase_typo_index();
        Ok(index)
    }

    #[must_use]
    pub fn candidate_decision(
        &self,
        normalized: &str,
        language: Option<&str>,
        language_fallbacks: &[String],
        profile: &SemanticProfile,
        limit: usize,
    ) -> CandidateDecision {
        self.candidate_decision_internal(
            normalized,
            language,
            language_fallbacks,
            profile,
            limit,
            false,
        )
    }

    /// Bounded retrieval for an optional semantic resolver. Unlike deterministic resolution, a
    /// resolver may inspect weak short-query candidates, but this still uses the same posting and
    /// candidate limits and never scans the full catalog.
    #[must_use]
    pub fn resolver_candidate_decision(
        &self,
        normalized: &str,
        language: Option<&str>,
        language_fallbacks: &[String],
        profile: &SemanticProfile,
        limit: usize,
    ) -> CandidateDecision {
        self.candidate_decision_internal(
            normalized,
            language,
            language_fallbacks,
            profile,
            limit,
            true,
        )
    }

    fn candidate_decision_internal(
        &self,
        normalized: &str,
        language: Option<&str>,
        language_fallbacks: &[String],
        profile: &SemanticProfile,
        limit: usize,
        allow_short_weak: bool,
    ) -> CandidateDecision {
        let ordered = profile.normalize_colloquial_tokens(&ordered_tokens(normalized));
        let typo_lite = self.typo_lite(&ordered, language, language_fallbacks, profile);
        let lookup = typo_lite
            .as_ref()
            .map_or_else(|| ordered.clone(), |result| result.tokens.clone());
        let lookup_normalized = lookup.join(" ");
        let input_content = profile.content_tokens(&lookup);
        let content_norm = input_content.join(" ");
        let canonical_content = canonicalize(profile, &input_content).join(" ");
        let total = self.pattern_count;
        if total == 0 {
            return CandidateDecision {
                use_full_scan: false,
                reason: "empty_catalog",
                rows: Vec::new(),
                total_patterns: 0,
                posting_visits: 0,
                posting_saturated: false,
                saturated_keys: 0,
                typo_lite,
            };
        }
        let candidate_limit = limit.max(super::SEMANTIC_CANDIDATE_LIMIT_MIN);
        let languages = retrieval_languages(language, language_fallbacks);
        let mut specificity_scores: BTreeMap<usize, u64> = BTreeMap::new();
        let mut specificity_visits = 0usize;
        let mut specificity_saturated_keys = 0usize;
        let mut add_specificity =
            |map: &BTreeMap<String, BTreeSet<usize>>, key: &str, base: u64| {
                if key.is_empty() {
                    return;
                }
                for (language, language_weight) in &languages {
                    let localized = localized_key(language, key);
                    let Some(rows) = map.get(&localized) else {
                        continue;
                    };
                    if rows.len() > candidate_limit {
                        specificity_saturated_keys = specificity_saturated_keys.saturating_add(1);
                    }
                    let weight = scale_milli(base, &[*language_weight]);
                    for row in rows.iter().take(candidate_limit) {
                        let entry = specificity_scores.entry(*row).or_insert(0);
                        *entry = entry.saturating_add(weight);
                        specificity_visits = specificity_visits.saturating_add(1);
                    }
                }
            };
        add_specificity(&self.exact_sample, normalized, 32_000);
        if lookup_normalized != normalized {
            add_specificity(&self.exact_sample, &lookup_normalized, 31_000);
        }
        add_specificity(&self.exact_content, &content_norm, 18_000);
        if canonical_content != content_norm {
            add_specificity(&self.exact_content, &canonical_content, 18_000);
        }
        // Candidate pruning must not erase an exact authored sample merely because the sample is
        // embedded in a longer utterance. Query bounded contiguous n-grams, not only singletons.
        // Retrieval uses the same monotonic authoring rule as scoring: shorter exact samples carry
        // more specificity than longer ones. This changes reachability/rank only; scorer policy,
        // reported speech, authored negatives and wrapper competition remain authoritative.
        let mut embedded_keys = BTreeSet::new();
        let mut embedded_windows = 0usize;
        for source in [&ordered, &lookup] {
            let bounded_len = source.len().min(EMBEDDED_EXACT_SAMPLE_QUERY_TOKENS_MAX);
            for size in 1..=bounded_len.min(EMBEDDED_EXACT_SAMPLE_TOKENS_MAX) {
                if embedded_windows >= EMBEDDED_EXACT_SAMPLE_WINDOWS_MAX {
                    break;
                }
                let base = 24_000u64
                    .saturating_sub((size.saturating_sub(1) as u64).saturating_mul(2_000))
                    .max(10_000);
                for start in 0..=bounded_len - size {
                    if embedded_windows >= EMBEDDED_EXACT_SAMPLE_WINDOWS_MAX {
                        break;
                    }
                    let window = &source[start..start + size];
                    embedded_windows = embedded_windows.saturating_add(1);
                    if window.iter().all(|token| profile.is_pure_glue(token)) {
                        continue;
                    }
                    let key = window.join(" ");
                    if embedded_keys.insert(key.clone()) {
                        add_specificity(&self.exact_sample, &key, base);
                    }
                }
            }
        }
        if input_content.is_empty() {
            let mut rows = integer_candidate_rows(specificity_scores);
            rows.truncate(candidate_limit);
            let reason = if rows.is_empty() {
                "no_content_tokens_bounded"
            } else {
                "specificity_match_bounded"
            };
            return CandidateDecision {
                use_full_scan: false,
                reason,
                rows,
                total_patterns: total,
                posting_visits: specificity_visits,
                posting_saturated: specificity_saturated_keys > 0,
                saturated_keys: specificity_saturated_keys,
                typo_lite,
            };
        }

        // Generic fuzzy retrieval is deliberately separate from typo correction. It never
        // rewrites the utterance and never scans the catalog: bounded Unicode n-gram buckets only
        // make scorer-supported edit/stem matches reachable. Exact visible tokens do not fan out
        // into fuzzy alternatives.
        let (fuzzy_tokens, fuzzy_saturated) =
            self.fuzzy_token_candidates(&input_content, language, language_fallbacks, profile);
        for (candidate, similarity_milli) in fuzzy_tokens {
            add_specificity(
                &self.content_token,
                &candidate,
                scale_milli(900, &[similarity_milli]),
            );
        }
        if fuzzy_saturated {
            specificity_saturated_keys = specificity_saturated_keys.saturating_add(1);
        }

        // Whole-phrase typo rescue is a reachability safety net, not ordinary retrieval. Only
        // consult it when exact/token/fuzzy specificity found nothing. The scorer still owns
        // semantic authority and independently verifies the phrase as a conservative typo match.
        // This closes the short-token hole where inputs such as `nic jbo` cannot enter token-level
        // fuzzy retrieval even though the authored sample `nice job` is visibly nearby.
        if specificity_scores.is_empty() {
            let (phrase_rows, phrase_saturated, phrase_visits) = self.phrase_typo_candidates(
                &lookup_normalized,
                language,
                language_fallbacks,
                candidate_limit.min(PHRASE_TYPO_CANDIDATES_MAX),
            );
            for (pattern_index, phrase_rank) in phrase_rows {
                specificity_scores
                    .entry(pattern_index)
                    .and_modify(|current| *current = (*current).max(phrase_rank))
                    .or_insert(phrase_rank);
            }
            specificity_visits = specificity_visits.saturating_add(phrase_visits);
            if phrase_saturated {
                specificity_saturated_keys = specificity_saturated_keys.saturating_add(1);
            }
        }
        let has_specificity = !specificity_scores.is_empty();

        let short_referential = lookup.len() <= 3 && profile.has_pronoun(&lookup);
        if ((short_referential && input_content.len() < 2)
            || (input_content.len() < 2 && lookup.len() < 4))
            && !has_specificity
            && !allow_short_weak
        {
            return CandidateDecision {
                use_full_scan: false,
                reason: "short_ambiguous_query_bounded",
                rows: Vec::new(),
                total_patterns: total,
                posting_visits: specificity_visits,
                posting_saturated: false,
                saturated_keys: 0,
                typo_lite,
            };
        }

        let mut scores = specificity_scores;
        let unique_content = unique(
            input_content
                .iter()
                .cloned()
                .chain(canonicalize(profile, &input_content))
                .collect(),
        );
        let unique_input = unique(
            lookup
                .iter()
                .cloned()
                .chain(canonicalize(profile, &lookup))
                .collect(),
        );
        let content_bigrams = unique(
            adjacent_bigrams(&input_content)
                .into_iter()
                .chain(adjacent_bigrams(&canonicalize(profile, &input_content)))
                .collect(),
        );
        let input_bigrams = unique(
            adjacent_bigrams(&lookup)
                .into_iter()
                .chain(adjacent_bigrams(&canonicalize(profile, &lookup)))
                .collect(),
        );
        let per_key_limit = candidate_limit
            .saturating_mul(RETRIEVAL_POSTINGS_PER_KEY_MULTIPLIER)
            .min(RETRIEVAL_POSTINGS_PER_KEY_MAX)
            .max(1);
        let visit_budget = candidate_limit
            .saturating_mul(RETRIEVAL_POSTING_VISIT_MULTIPLIER)
            .min(RETRIEVAL_POSTING_VISIT_MAX)
            .max(1);
        let mut remaining_visits =
            visit_budget.saturating_sub(specificity_visits.min(visit_budget));
        let mut saturated_keys = specificity_saturated_keys;

        if input_content.len() > 1 {
            let first = format!("{} {}", input_content[0], input_content[1]);
            saturated_keys += self.add_feature_scores(
                &mut scores,
                &self.sample_start_bigram,
                &first,
                5_500,
                1_000,
                &languages,
                per_key_limit,
                &mut remaining_visits,
            ) as usize;
            let canonical = canonicalize(profile, &input_content);
            if canonical.len() > 1 {
                let first_canonical = format!("{} {}", canonical[0], canonical[1]);
                if first_canonical != first {
                    saturated_keys += self.add_feature_scores(
                        &mut scores,
                        &self.sample_start_bigram,
                        &first_canonical,
                        5_500,
                        1_000,
                        &languages,
                        per_key_limit,
                        &mut remaining_visits,
                    ) as usize;
                }
            }
        }
        for bigram in content_bigrams
            .iter()
            .take(RETRIEVAL_FEATURES_PER_CLASS_MAX)
        {
            saturated_keys += self.add_feature_scores(
                &mut scores,
                &self.content_bigram,
                bigram,
                8_000,
                1_000,
                &languages,
                per_key_limit,
                &mut remaining_visits,
            ) as usize;
            saturated_keys += self.add_feature_scores(
                &mut scores,
                &self.meta_bigram,
                bigram,
                1_250,
                1_000,
                &languages,
                per_key_limit,
                &mut remaining_visits,
            ) as usize;
        }
        for bigram in input_bigrams.iter().take(RETRIEVAL_FEATURES_PER_CLASS_MAX) {
            saturated_keys += self.add_feature_scores(
                &mut scores,
                &self.bigram,
                bigram,
                4_000,
                1_000,
                &languages,
                per_key_limit,
                &mut remaining_visits,
            ) as usize;
        }
        for token in unique_content.iter().take(RETRIEVAL_FEATURES_PER_CLASS_MAX) {
            let token_weight = profile.index_token_weight_milli(token);
            saturated_keys += self.add_feature_scores(
                &mut scores,
                &self.content_token,
                token,
                3_200,
                token_weight,
                &languages,
                per_key_limit,
                &mut remaining_visits,
            ) as usize;
            saturated_keys += self.add_feature_scores(
                &mut scores,
                &self.meta_token,
                token,
                750,
                1_000,
                &languages,
                per_key_limit,
                &mut remaining_visits,
            ) as usize;
        }
        for token in unique_input.iter().take(RETRIEVAL_FEATURES_PER_CLASS_MAX) {
            if profile.is_pure_glue(token) {
                continue;
            }
            saturated_keys += self.add_feature_scores(
                &mut scores,
                &self.token,
                token,
                1_100,
                profile.index_token_weight_milli(token),
                &languages,
                per_key_limit,
                &mut remaining_visits,
            ) as usize;
        }

        let mut rows = integer_candidate_rows(scores);
        if rows.is_empty() {
            return CandidateDecision {
                use_full_scan: false,
                reason: "no_index_candidates_bounded",
                rows: Vec::new(),
                total_patterns: total,
                posting_visits: visit_budget.saturating_sub(remaining_visits),
                posting_saturated: saturated_keys > 0,
                saturated_keys,
                typo_lite,
            };
        }
        rows.truncate(candidate_limit.min(rows.len()));
        CandidateDecision {
            use_full_scan: false,
            reason: if has_specificity {
                "specificity_plus_index_bounded"
            } else {
                "inverted_index_integer_idf"
            },
            rows,
            total_patterns: total,
            posting_visits: visit_budget.saturating_sub(remaining_visits),
            posting_saturated: saturated_keys > 0,
            saturated_keys,
            typo_lite,
        }
    }

    fn rebuild_typo_dictionary(&mut self) {
        let mut known = BTreeSet::new();
        for map in [&self.content_token, &self.token, &self.meta_token] {
            for key in map.keys() {
                let Some((_, token)) = split_localized_key(key) else {
                    continue;
                };
                if is_typo_lite_token(token) {
                    known.insert(token.to_string());
                }
            }
        }
        let mut by_length: BTreeMap<usize, Vec<String>> = BTreeMap::new();
        for token in &known {
            by_length
                .entry(token.len())
                .or_default()
                .push(token.clone());
        }
        for bucket in by_length.values_mut() {
            bucket.sort();
        }
        self.typo_signatures = build_typo_signature_index(&known);
        self.known_typo_tokens = known;
        self.typo_by_length = by_length;
    }

    fn rebuild_fuzzy_dictionary(&mut self) {
        let mut known = BTreeSet::new();
        for map in [&self.content_token, &self.token, &self.meta_token] {
            for key in map.keys() {
                let Some((_, token)) = split_localized_key(key) else {
                    continue;
                };
                if is_fuzzy_retrieval_token(token) {
                    known.insert(token.to_string());
                }
            }
        }
        self.fuzzy_signatures = build_fuzzy_signature_index(&known);
    }

    fn rebuild_phrase_typo_index(&mut self) {
        let mut out: BTreeMap<String, PatternSignatureBucket> = BTreeMap::new();
        for (localized, rows) in &self.exact_sample {
            let Some((language, phrase)) = split_localized_key(localized) else {
                continue;
            };
            if !is_phrase_typo_retrieval_phrase(phrase) {
                continue;
            }
            for signature in phrase_typo_signature_keys(phrase) {
                let key = localized_key(language, &signature);
                let bucket = out.entry(key).or_default();
                for row in rows {
                    if bucket.rows.contains(row) {
                        continue;
                    }
                    if bucket.rows.len() < PHRASE_TYPO_SIGNATURE_BUCKET_MAX {
                        bucket.rows.push(*row);
                    } else {
                        bucket.overflow = true;
                        break;
                    }
                }
            }
        }
        self.phrase_typo_signatures = out;
    }

    fn phrase_typo_candidates(
        &self,
        phrase: &str,
        language: Option<&str>,
        language_fallbacks: &[String],
        limit: usize,
    ) -> (Vec<(usize, u64)>, bool, usize) {
        if !is_phrase_typo_retrieval_phrase(phrase) || limit == 0 {
            return (Vec::new(), false, 0);
        }
        let languages = retrieval_languages(language, language_fallbacks);
        if languages.is_empty() {
            return (Vec::new(), false, 0);
        }
        let mut votes: BTreeMap<usize, u64> = BTreeMap::new();
        let mut saturated = false;
        let mut visits = 0usize;
        for signature in phrase_typo_signature_keys(phrase)
            .into_iter()
            .take(PHRASE_TYPO_QUERY_SIGNATURES_MAX)
        {
            for (language, language_weight) in &languages {
                let key = localized_key(language, &signature);
                let Some(bucket) = self.phrase_typo_signatures.get(&key) else {
                    continue;
                };
                if bucket.overflow {
                    // A truncated signature would bias rescue toward catalog order. Treat that
                    // signature as unusable instead of manufacturing incomplete authority.
                    saturated = true;
                    continue;
                }
                for row in &bucket.rows {
                    if visits >= PHRASE_TYPO_POSTING_VISITS_MAX {
                        saturated = true;
                        break;
                    }
                    *votes.entry(*row).or_insert(0) += *language_weight;
                    visits += 1;
                }
                if visits >= PHRASE_TYPO_POSTING_VISITS_MAX {
                    break;
                }
            }
            if visits >= PHRASE_TYPO_POSTING_VISITS_MAX {
                break;
            }
        }
        let mut ranked: Vec<(usize, u64)> = votes
            .into_iter()
            // Require at least two exact phrase-character signatures in the weakest fallback
            // language. One common bigram is never enough to create a rescue candidate.
            .filter(|(_, vote)| *vote >= 2_000)
            .collect();
        ranked.sort_by(|(left_index, left_vote), (right_index, right_vote)| {
            right_vote
                .cmp(left_vote)
                .then_with(|| left_index.cmp(right_index))
        });
        ranked.truncate(limit);
        for (_, vote) in &mut ranked {
            // Keep rescue rank competitive enough to survive candidate pruning, while exact and
            // ordinary indexed evidence retain their larger specificity scales.
            *vote = 12_000 + (*vote).min(24_000);
        }
        (ranked, saturated, visits)
    }

    fn fuzzy_token_candidates(
        &self,
        tokens: &[String],
        language: Option<&str>,
        language_fallbacks: &[String],
        profile: &SemanticProfile,
    ) -> (Vec<(String, u64)>, bool) {
        let mut selected: BTreeMap<String, u64> = BTreeMap::new();
        let mut saturated = false;
        for token in tokens.iter().take(FUZZY_QUERY_TOKENS_MAX) {
            if !is_fuzzy_retrieval_token(token)
                || self.token_visible_for_language(token, language, language_fallbacks)
            {
                continue;
            }
            let mut votes: BTreeMap<String, usize> = BTreeMap::new();
            for key in fuzzy_signature_keys(token) {
                let Some(bucket) = self.fuzzy_signatures.get(&key) else {
                    continue;
                };
                saturated |= bucket.overflow;
                for candidate in &bucket.values {
                    if candidate != token {
                        *votes.entry(candidate.clone()).or_insert(0) += 1;
                    }
                }
            }
            let mut ranked: Vec<(String, usize)> = votes.into_iter().collect();
            ranked.sort_by(|(left_token, left_votes), (right_token, right_votes)| {
                right_votes
                    .cmp(left_votes)
                    .then_with(|| left_token.cmp(right_token))
            });
            for (candidate, _) in ranked
                .into_iter()
                .filter(|(candidate, _)| {
                    self.token_visible_for_language(candidate, language, language_fallbacks)
                })
                .take(FUZZY_CANDIDATE_TOKENS_MAX)
            {
                let similarity = super::matching::token_similarity(profile, token, &candidate);
                if similarity < 0.75 {
                    continue;
                }
                let similarity_milli = (similarity * 1_000.0).round() as u64;
                selected
                    .entry(candidate)
                    .and_modify(|current| *current = (*current).max(similarity_milli))
                    .or_insert(similarity_milli);
            }
        }
        (selected.into_iter().collect(), saturated)
    }

    fn typo_lite(
        &self,
        tokens: &[String],
        language: Option<&str>,
        language_fallbacks: &[String],
        profile: &SemanticProfile,
    ) -> Option<TypoLiteResult> {
        let mut corrected = Vec::with_capacity(tokens.len());
        let mut corrections = Vec::new();
        let mut typo_attempts = 0usize;
        for token in tokens {
            let replacement = if corrections.len() < 2 && typo_attempts < TYPO_QUERY_TOKENS_MAX {
                typo_attempts += 1;
                self.find_typo_correction(token, language, language_fallbacks)
            } else {
                None
            };
            if let Some(to) = replacement {
                corrections.push(TypoCorrection {
                    from: token.clone(),
                    to: to.clone(),
                });
                corrected.push(to);
            } else {
                corrected.push(token.clone());
            }
        }
        if corrections.is_empty() {
            return None;
        }
        Some(TypoLiteResult {
            content_tokens: profile.content_tokens(&corrected),
            tokens: corrected,
            corrections,
        })
    }

    fn find_typo_correction(
        &self,
        token: &str,
        language: Option<&str>,
        language_fallbacks: &[String],
    ) -> Option<String> {
        if !is_typo_lite_token(token) || self.known_typo_tokens.contains(token) {
            return None;
        }
        let mut candidates = BTreeSet::new();
        let mut incomplete = false;
        for key in typo_signature_keys(token) {
            let Some(bucket) = self.typo_signatures.get(&key) else {
                continue;
            };
            if bucket.overflow {
                incomplete = true;
            }
            for candidate in &bucket.values {
                if candidate == token || candidate.as_bytes().first() != token.as_bytes().first() {
                    continue;
                }
                if limited_damerau_distance(token, candidate, 1) == 1 {
                    candidates.insert(candidate.clone());
                    if candidates.len() > 1 {
                        return None;
                    }
                }
            }
        }
        if incomplete {
            return None;
        }
        let candidate = candidates.into_iter().next()?;
        self.token_visible_for_language(&candidate, language, language_fallbacks)
            .then_some(candidate)
    }

    fn token_visible_for_language(
        &self,
        token: &str,
        language: Option<&str>,
        language_fallbacks: &[String],
    ) -> bool {
        retrieval_languages(language, language_fallbacks)
            .iter()
            .any(|(language, _)| {
                let key = localized_key(language, token);
                self.content_token.contains_key(&key)
                    || self.token.contains_key(&key)
                    || self.meta_token.contains_key(&key)
            })
    }

    fn idf_milli(&self, map: &BTreeMap<String, BTreeSet<usize>>, key: &str) -> u64 {
        let total = self.pattern_count.max(1) as u64;
        let df = map
            .get(key)
            .map_or(0_u64, |rows| rows.len() as u64)
            .min(total);
        // Deterministic monotonic IDF-lite in [1.000, 5.000], using integer rational arithmetic.
        1_000 + total.saturating_sub(df).saturating_mul(4_000) / total
    }

    #[allow(clippy::too_many_arguments)]
    fn add_feature_scores(
        &self,
        scores: &mut BTreeMap<usize, u64>,
        map: &BTreeMap<String, BTreeSet<usize>>,
        raw_key: &str,
        base_weight: u64,
        token_weight: u64,
        languages: &[(String, u64)],
        per_key_limit: usize,
        remaining_visits: &mut usize,
    ) -> bool {
        let mut saturated = false;
        for (language, language_weight) in languages {
            let key = localized_key(language, raw_key);
            let Some(rows) = map.get(&key) else {
                continue;
            };
            if *remaining_visits == 0 {
                return saturated || !rows.is_empty();
            }
            let take = per_key_limit.min(*remaining_visits).min(rows.len());
            let idf = self.idf_milli(map, &key);
            let weight = scale_milli(base_weight, &[token_weight, *language_weight, idf]);
            for row in rows.iter().take(take) {
                let entry = scores.entry(*row).or_insert(0);
                *entry = entry.saturating_add(weight);
                *remaining_visits -= 1;
            }
            saturated |= rows.len() > take;
        }
        saturated
    }
}

fn add_index_tokens(
    token_map: &mut BTreeMap<String, BTreeSet<usize>>,
    bigram_map: &mut BTreeMap<String, BTreeSet<usize>>,
    pattern_index: usize,
    language: &str,
    tokens: &[String],
    profile: &SemanticProfile,
) {
    let tokens = unique(profile.content_tokens(tokens));
    for token in &tokens {
        if profile.is_pure_glue(token) {
            continue;
        }
        for key in index_token_keys(profile, token) {
            add_localized(token_map, language, &key, pattern_index);
        }
    }
    for bigram in adjacent_bigrams(&tokens)
        .into_iter()
        .chain(adjacent_bigrams(&canonicalize(profile, &tokens)))
    {
        add_localized(bigram_map, language, &bigram, pattern_index);
    }
}

fn localized_key(language: &str, key: &str) -> String {
    format!("{}:{language}:{key}", language.len())
}

fn split_localized_key(value: &str) -> Option<(&str, &str)> {
    let (len, rest) = value.split_once(':')?;
    let len = len.parse::<usize>().ok()?;
    let language = rest.get(..len)?;
    let key = rest.get(len..)?.strip_prefix(':')?;
    Some((language, key))
}

fn add_localized(
    map: &mut BTreeMap<String, BTreeSet<usize>>,
    language: &str,
    key: &str,
    index: usize,
) {
    add(map, &localized_key(language, key), index);
}

/// Exact language receives the strongest retrieval preference, followed by explicit fallbacks.
fn retrieval_languages(
    requested: Option<&str>,
    explicit_fallbacks: &[String],
) -> Vec<(String, u64)> {
    language_fallbacks(requested, explicit_fallbacks)
        .into_iter()
        .enumerate()
        .map(|(index, language)| {
            (
                language,
                match index {
                    0 => 3_000,
                    1 => 2_000,
                    _ => 1_000,
                },
            )
        })
        .collect()
}

fn scale_milli(base: u64, factors: &[u64]) -> u64 {
    let mut value = u128::from(base);
    for factor in factors {
        value = value.saturating_mul(u128::from(*factor)) / 1_000;
    }
    u64::try_from(value).unwrap_or(u64::MAX)
}

fn integer_candidate_rows(scores: BTreeMap<usize, u64>) -> Vec<CandidateRow> {
    let mut rows: Vec<(usize, u64)> = scores.into_iter().filter(|(_, score)| *score > 0).collect();
    rows.sort_by(|(left_index, left_score), (right_index, right_score)| {
        right_score
            .cmp(left_score)
            .then_with(|| left_index.cmp(right_index))
    });
    rows.into_iter()
        .map(|(pattern_index, rank_milli)| CandidateRow {
            pattern_index,
            rank_milli,
        })
        .collect()
}

fn add(map: &mut BTreeMap<String, BTreeSet<usize>>, key: &str, index: usize) {
    if !key.is_empty() {
        map.entry(key.to_string()).or_default().insert(index);
    }
}

fn index_token_keys(profile: &SemanticProfile, token: &str) -> Vec<String> {
    let canonical = profile.canonical_token(token);
    if canonical == token {
        vec![token.to_string()]
    } else {
        vec![token.to_string(), canonical]
    }
}

fn canonicalize(profile: &SemanticProfile, tokens: &[String]) -> Vec<String> {
    tokens
        .iter()
        .map(|token| profile.canonical_token(token))
        .collect()
}

fn adjacent_bigrams(tokens: &[String]) -> Vec<String> {
    tokens
        .windows(2)
        .map(|pair| format!("{} {}", pair[0], pair[1]))
        .collect()
}

fn unique(values: Vec<String>) -> Vec<String> {
    let mut seen = BTreeSet::new();
    values
        .into_iter()
        .filter(|value| seen.insert(value.clone()))
        .collect()
}

fn build_typo_signature_index(known: &BTreeSet<String>) -> BTreeMap<String, TypoSignatureBucket> {
    let mut out: BTreeMap<String, TypoSignatureBucket> = BTreeMap::new();
    for token in known {
        for key in typo_signature_keys(token) {
            let bucket = out.entry(key).or_default();
            if bucket.values.iter().any(|value| value == token) {
                continue;
            }
            if bucket.values.len() < TYPO_SIGNATURE_BUCKET_MAX {
                bucket.values.push(token.clone());
            } else {
                bucket.overflow = true;
            }
        }
    }
    out
}

fn build_fuzzy_signature_index(known: &BTreeSet<String>) -> BTreeMap<String, TypoSignatureBucket> {
    let mut out: BTreeMap<String, TypoSignatureBucket> = BTreeMap::new();
    for token in known {
        for key in fuzzy_signature_keys(token) {
            let bucket = out.entry(key).or_default();
            if bucket.values.iter().any(|value| value == token) {
                continue;
            }
            if bucket.values.len() < FUZZY_SIGNATURE_BUCKET_MAX {
                bucket.values.push(token.clone());
            } else {
                bucket.overflow = true;
            }
        }
    }
    out
}

fn fuzzy_signature_keys(token: &str) -> Vec<String> {
    let chars: Vec<char> = token.chars().collect();
    let width = if chars.len() >= 5 { 3 } else { 2 };
    if chars.len() < width {
        return Vec::new();
    }
    let mut keys: Vec<String> = chars
        .windows(width)
        .map(|window| format!("{width}:{}", window.iter().collect::<String>()))
        .collect();
    keys.sort();
    keys.dedup();
    keys
}

fn is_fuzzy_retrieval_token(token: &str) -> bool {
    let length = token.chars().count();
    (4..=48).contains(&length) && !token.bytes().all(|byte| byte.is_ascii_digit())
}

fn is_phrase_typo_retrieval_phrase(phrase: &str) -> bool {
    let token_count = phrase.split_whitespace().count();
    let char_count = phrase.chars().count();
    (2..=8).contains(&token_count) && (5..=96).contains(&char_count)
}

fn phrase_typo_signature_keys(phrase: &str) -> Vec<String> {
    if !is_phrase_typo_retrieval_phrase(phrase) {
        return Vec::new();
    }
    let token_count = phrase.split_whitespace().count();
    let chars: Vec<char> = phrase.chars().collect();
    let mut keys: Vec<String> = chars
        .windows(2)
        .map(|window| format!("p{token_count}:{}", window.iter().collect::<String>()))
        .collect();
    keys.sort();
    keys.dedup();
    keys.truncate(PHRASE_TYPO_QUERY_SIGNATURES_MAX);
    keys
}

fn typo_signature_keys(token: &str) -> Vec<String> {
    // Typo-lite tokens are ASCII lowercase, so byte slicing is safe here. Deletion signatures
    // cover insertion/deletion/substitution; adjacent-swap signatures preserve the reference
    // Damerau transposition rescue without scanning a whole length bucket. The first-character
    // prefix intentionally enforces the conservative first-character floor.
    let Some(first) = token.as_bytes().first().copied() else {
        return Vec::new();
    };
    let mut raw = Vec::with_capacity(token.len().saturating_mul(2).saturating_add(1));
    raw.push(token.to_string());
    for index in 0..token.len() {
        let mut value = String::with_capacity(token.len().saturating_sub(1));
        value.push_str(&token[..index]);
        value.push_str(&token[index + 1..]);
        raw.push(value);
    }
    let bytes = token.as_bytes();
    for index in 0..bytes.len().saturating_sub(1) {
        let mut swapped = bytes.to_vec();
        swapped.swap(index, index + 1);
        // `is_typo_lite_token` guarantees ASCII lowercase input, so this cannot fail.
        if let Ok(value) = String::from_utf8(swapped) {
            raw.push(value);
        }
    }
    raw.sort();
    raw.dedup();
    raw.into_iter()
        .map(|value| format!("{}:{value}", char::from(first)))
        .collect()
}

fn is_typo_lite_token(token: &str) -> bool {
    (5..=28).contains(&token.len()) && token.bytes().all(|byte| byte.is_ascii_lowercase())
}

#[must_use]
pub fn limited_damerau_distance(left: &str, right: &str, max_distance: usize) -> usize {
    if left == right {
        return 0;
    }
    if left.len().abs_diff(right.len()) > max_distance {
        return max_distance + 1;
    }
    let a = left.as_bytes();
    let b = right.as_bytes();
    let mut previous_previous: Option<Vec<usize>> = None;
    let mut previous: Vec<usize> = (0..=b.len()).collect();
    for i in 1..=a.len() {
        let mut current = vec![0; b.len() + 1];
        current[0] = i;
        let mut row_min = i;
        for j in 1..=b.len() {
            let cost = usize::from(a[i - 1] != b[j - 1]);
            let mut value = (previous[j] + 1)
                .min(current[j - 1] + 1)
                .min(previous[j - 1] + cost);
            if i > 1 && j > 1 && a[i - 1] == b[j - 2] && a[i - 2] == b[j - 1] {
                let transposed = previous_previous.as_ref().map_or(j - 2, |row| row[j - 2]) + 1;
                value = value.min(transposed);
            }
            current[j] = value;
            row_min = row_min.min(value);
        }
        if row_min > max_distance {
            return max_distance + 1;
        }
        previous_previous = Some(previous);
        previous = current;
    }
    previous[b.len()].min(max_distance + 1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::semantic::catalog::MeaningPattern;

    fn test_profiles(profile: SemanticProfile) -> SemanticProfiles {
        BTreeMap::from([("und".to_owned(), profile)])
    }

    #[test]
    fn damerau_detects_single_transposition() {
        assert_eq!(limited_damerau_distance("warehosue", "warehouse", 1), 1);
    }

    #[test]
    fn typo_lite_only_applies_unique_authored_correction() {
        let profile = SemanticProfile::empty();
        let catalog = SemanticCatalog::new(vec![MeaningPattern::new(
            "hours",
            ["warehouse manager hours"],
        )])
        .unwrap();
        let index = SemanticIndex::build(&catalog, &test_profiles(profile.clone())).unwrap();
        let decision =
            index.candidate_decision("warehosue manager hours", Some("und"), &[], &profile, 80);
        let typo = decision.typo_lite.expect("correction");
        assert!(
            typo.corrections
                .iter()
                .any(|row| row.from == "warehosue" && row.to == "warehouse")
        );
    }

    #[test]
    fn generic_fuzzy_index_retrieves_productive_word_variants_without_full_scan() {
        let profile = SemanticProfile::empty();
        let catalog = SemanticCatalog::new(vec![MeaningPattern::new(
            "install.manager",
            ["manager install"],
        )])
        .unwrap();
        let index = SemanticIndex::build(&catalog, &test_profiles(profile.clone())).unwrap();
        let decision =
            index.candidate_decision("managerial installation", Some("und"), &[], &profile, 32);
        assert!(!decision.use_full_scan);
        assert_eq!(decision.rows.len(), 1);
        assert_eq!(
            catalog.patterns()[decision.rows[0].pattern_index]
                .id
                .as_str(),
            "install.manager"
        );
    }

    #[test]
    fn generic_fuzzy_index_is_unicode_and_language_partition_aware() {
        let profile = SemanticProfile::empty();
        let mut pattern = MeaningPattern::new("health", ["placeholder"]);
        pattern.samples = vec![super::super::LocalizedSample::new("fa-IR", "سلامت")];
        let catalog = SemanticCatalog::new(vec![pattern]).unwrap();
        let profiles = BTreeMap::from([("fa-ir".to_owned(), profile.clone())]);
        let index = SemanticIndex::build(&catalog, &profiles).unwrap();
        let visible = index.candidate_decision("سلانت امروز", Some("fa-IR"), &[], &profile, 32);
        assert_eq!(visible.rows.len(), 1);
        let hidden = index.candidate_decision("سلانت امروز", Some("en-US"), &[], &profile, 32);
        assert!(hidden.rows.is_empty());
    }

    #[test]
    fn authored_character_rewrites_are_applied_before_exact_index_lookup() {
        let mut profile = SemanticProfile::empty();
        profile
            .normalization_rewrites
            .insert("۲".to_owned(), "2".to_owned());
        let mut pattern = MeaningPattern::new("version.two", ["placeholder"]);
        pattern.samples = vec![super::super::LocalizedSample::new("fa-IR", "نسخه 2")];
        let catalog = SemanticCatalog::new(vec![pattern]).unwrap();
        let profiles = BTreeMap::from([("fa-ir".to_owned(), profile.clone())]);
        let index = SemanticIndex::build(&catalog, &profiles).unwrap();
        let normalized = profile.normalize_text("نسخه ۲");
        assert_eq!(normalized, "نسخه 2");
        let decision = index.candidate_decision(&normalized, Some("fa-IR"), &[], &profile, 32);
        assert_eq!(decision.rows.len(), 1);
        assert_eq!(
            catalog.patterns()[decision.rows[0].pattern_index]
                .id
                .as_str(),
            "version.two"
        );
        assert!(decision.rows[0].rank_milli >= 32_000);
    }

    #[test]
    fn whole_phrase_typo_index_reaches_short_multi_token_typos() {
        let profile = SemanticProfile::empty();
        let catalog =
            SemanticCatalog::new(vec![MeaningPattern::new("praise", ["nice job"])]).unwrap();
        let index = SemanticIndex::build(&catalog, &test_profiles(profile.clone())).unwrap();
        let decision = index.candidate_decision("nic jbo", Some("und"), &[], &profile, 32);
        assert_eq!(decision.rows.len(), 1);
        assert_eq!(
            catalog.patterns()[decision.rows[0].pattern_index]
                .id
                .as_str(),
            "praise"
        );
        assert_eq!(decision.reason, "specificity_plus_index_bounded");
    }

    #[test]
    fn whole_phrase_typo_index_does_not_reach_different_token_shapes() {
        let profile = SemanticProfile::empty();
        let catalog =
            SemanticCatalog::new(vec![MeaningPattern::new("status", ["how are you"])]).unwrap();
        let index = SemanticIndex::build(&catalog, &test_profiles(profile)).unwrap();
        let (rows, _, _) = index.phrase_typo_candidates("how do you work", Some("und"), &[], 32);
        assert!(rows.is_empty());
    }

    #[test]
    fn common_token_posting_work_is_hard_bounded() {
        let profile = SemanticProfile::empty();
        let patterns = (0..5_000)
            .map(|index| {
                MeaningPattern::new(
                    format!("meaning.{index}"),
                    [format!("shared catalog token {index}")],
                )
            })
            .collect::<Vec<_>>();
        let catalog = SemanticCatalog::new(patterns).unwrap();
        let index = SemanticIndex::build(&catalog, &test_profiles(profile.clone())).unwrap();
        let decision =
            index.candidate_decision("shared catalog token", Some("und"), &[], &profile, 32);
        assert!(!decision.use_full_scan);
        assert!(decision.rows.len() <= 32);
        assert!(decision.posting_visits <= 32 * RETRIEVAL_POSTING_VISIT_MULTIPLIER);
        assert!(decision.posting_saturated);
        assert!(decision.saturated_keys > 0);
    }

    #[test]
    fn no_hit_work_stays_empty_even_for_large_catalog() {
        let profile = SemanticProfile::empty();
        let patterns = (0..5_000)
            .map(|index| {
                MeaningPattern::new(
                    format!("meaning.{index}"),
                    [format!("catalog phrase {index} token")],
                )
            })
            .collect::<Vec<_>>();
        let catalog = SemanticCatalog::new(patterns).unwrap();
        let index = SemanticIndex::build(&catalog, &test_profiles(profile.clone())).unwrap();
        let decision = index.candidate_decision(
            "completely unrelated utterance",
            Some("und"),
            &[],
            &profile,
            32,
        );
        assert!(!decision.use_full_scan);
        assert!(decision.rows.len() <= 32);
        assert_eq!(decision.total_patterns, 5_000);
    }

    #[test]
    fn embedded_exact_multiword_sample_gets_bounded_specificity() {
        let profile = SemanticProfile::empty();
        let catalog = SemanticCatalog::new(vec![
            MeaningPattern::new("short", ["alpha beta gamma delta"]),
            MeaningPattern::new("noise.one", ["alpha unrelated"]),
            MeaningPattern::new("noise.two", ["beta unrelated"]),
        ])
        .unwrap();
        let index = SemanticIndex::build(&catalog, &test_profiles(profile.clone())).unwrap();
        let decision = index.candidate_decision(
            "please alpha beta gamma delta continue",
            Some("und"),
            &[],
            &profile,
            2,
        );
        assert!(!decision.use_full_scan);
        assert!(decision.rows.iter().any(|row| {
            catalog.patterns()[row.pattern_index].id.as_str() == "short" && row.rank_milli >= 18_000
        }));
    }

    #[test]
    fn exact_one_word_sample_survives_large_catalog_without_full_scan() {
        let profile = SemanticProfile::empty();
        let mut patterns = (0..5_000)
            .map(|index| {
                MeaningPattern::new(
                    format!("meaning.{index}"),
                    [format!("catalog phrase {index} token")],
                )
            })
            .collect::<Vec<_>>();
        patterns.push(MeaningPattern::new("media", ["media"]));
        let catalog = SemanticCatalog::new(patterns).unwrap();
        let index = SemanticIndex::build(&catalog, &test_profiles(profile.clone())).unwrap();
        let decision = index.candidate_decision("media", Some("und"), &[], &profile, 32);
        assert_eq!(decision.reason, "specificity_plus_index_bounded");
        assert!(!decision.use_full_scan);
        assert_eq!(decision.rows.len(), 1);
        assert_eq!(
            catalog.patterns()[decision.rows[0].pattern_index]
                .id
                .as_str(),
            "media"
        );
    }

    #[test]
    fn exact_sample_collisions_remain_visible_to_semantic_ranking() {
        let profile = SemanticProfile::empty();
        let catalog = SemanticCatalog::new(vec![
            MeaningPattern::new("alpha", ["same exact sample"]),
            MeaningPattern::new("beta", ["same exact sample"]),
        ])
        .unwrap();
        let index = SemanticIndex::build(&catalog, &test_profiles(profile.clone())).unwrap();
        let decision = index.candidate_decision("same exact sample", Some("und"), &[], &profile, 8);
        assert_eq!(decision.reason, "specificity_plus_index_bounded");
        assert_eq!(decision.rows.len(), 2);
    }

    #[test]
    fn direct_index_limit_one_cannot_hide_exact_ambiguity() {
        let profile = SemanticProfile::empty();
        let catalog = SemanticCatalog::new(vec![
            MeaningPattern::new("alpha", ["same exact sample"]),
            MeaningPattern::new("beta", ["same exact sample"]),
        ])
        .unwrap();
        let index = SemanticIndex::build(&catalog, &test_profiles(profile.clone())).unwrap();
        let decision = index.candidate_decision("same exact sample", Some("und"), &[], &profile, 1);
        assert_eq!(decision.rows.len(), 2);
    }

    #[test]
    fn canonical_specificity_keys_are_built() {
        let mut profile = SemanticProfile::empty();
        profile
            .canonical_tokens
            .insert("boxes".to_owned(), "box".to_owned());
        let catalog =
            SemanticCatalog::new(vec![MeaningPattern::new("boxes", ["boxes status"])]).unwrap();
        let index = SemanticIndex::build(&catalog, &test_profiles(profile.clone())).unwrap();
        assert!(index.exact_content.contains_key("3:und:boxes status"));
        assert!(index.exact_content.contains_key("3:und:box status"));
        assert!(index.sample_start_bigram.contains_key("3:und:boxes status"));
        assert!(index.sample_start_bigram.contains_key("3:und:box status"));
    }

    #[test]
    fn typo_rescue_survives_large_same_length_dictionary() {
        let profile = SemanticProfile::empty();
        let mut patterns = Vec::new();
        for index in 0..2_100usize {
            let mut n = index;
            let mut suffix = ['a'; 8];
            for slot in suffix.iter_mut().rev() {
                *slot = char::from(b'a' + u8::try_from(n % 26).unwrap());
                n /= 26;
            }
            let word = format!("a{}", suffix.iter().collect::<String>());
            patterns.push(MeaningPattern::new(format!("bulk.{index}"), [word]));
        }
        patterns.push(MeaningPattern::new("warehouse", ["warehouse"]));
        let catalog = SemanticCatalog::new(patterns).unwrap();
        let index = SemanticIndex::build(&catalog, &test_profiles(profile.clone())).unwrap();
        assert!(
            index
                .typo_by_length
                .get(&9)
                .is_some_and(|rows| rows.len() > 2_048)
        );
        assert_eq!(
            index
                .find_typo_correction("warehosue", Some("und"), &[])
                .as_deref(),
            Some("warehouse")
        );
    }

    #[test]
    fn typo_signature_preserves_adjacent_transposition_rescue() {
        let profile = SemanticProfile::empty();
        let catalog = SemanticCatalog::new(vec![MeaningPattern::new(
            "warehouse",
            ["warehouse manager hours"],
        )])
        .unwrap();
        let index = SemanticIndex::build(&catalog, &test_profiles(profile.clone())).unwrap();
        let decision =
            index.candidate_decision("warehosue manager hours", Some("und"), &[], &profile, 32);
        let typo = decision
            .typo_lite
            .expect("adjacent transposition should be rescued");
        assert!(
            typo.corrections
                .iter()
                .any(|row| row.from == "warehosue" && row.to == "warehouse")
        );
    }

    #[test]
    fn single_word_typo_uses_corrected_specificity_before_short_query_guard() {
        let profile = SemanticProfile::empty();
        let catalog =
            SemanticCatalog::new(vec![MeaningPattern::new("warehouse", ["warehouse"])]).unwrap();
        let index = SemanticIndex::build(&catalog, &test_profiles(profile.clone())).unwrap();
        let decision = index.candidate_decision("warehosue", Some("und"), &[], &profile, 32);
        assert_eq!(decision.rows.len(), 1);
        assert_eq!(
            catalog.patterns()[decision.rows[0].pattern_index]
                .id
                .as_str(),
            "warehouse"
        );
        assert!(decision.typo_lite.as_ref().is_some_and(|typo| {
            typo.corrections
                .iter()
                .any(|row| row.from == "warehosue" && row.to == "warehouse")
        }));
    }

    #[test]
    fn single_word_canonical_inflection_and_glue_specificity_bypass_short_guard() {
        let mut profile = SemanticProfile::empty();
        profile
            .canonical_tokens
            .insert("boxes".to_owned(), "box".to_owned());
        profile.pure_glue.insert("the".to_owned());
        let catalog = SemanticCatalog::new(vec![
            MeaningPattern::new("box", ["box"]),
            MeaningPattern::new("warehouse", ["warehouse"]),
        ])
        .unwrap();
        let index = SemanticIndex::build(&catalog, &test_profiles(profile.clone())).unwrap();
        let boxes = index.candidate_decision("boxes", Some("und"), &[], &profile, 32);
        assert!(
            boxes
                .rows
                .iter()
                .any(|row| catalog.patterns()[row.pattern_index].id.as_str() == "box")
        );
        let glue = index.candidate_decision("the warehouse", Some("und"), &[], &profile, 32);
        assert!(
            glue.rows
                .iter()
                .any(|row| catalog.patterns()[row.pattern_index].id.as_str() == "warehouse")
        );
    }

    #[test]
    fn common_posting_saturation_is_explicit_in_candidate_trace_data() {
        let profile = SemanticProfile::empty();
        let patterns = (0..2_000)
            .map(|index| {
                MeaningPattern::new(
                    format!("shared.{index}"),
                    [format!("shared keyword phrase {index}")],
                )
            })
            .collect::<Vec<_>>();
        let catalog = SemanticCatalog::new(patterns).unwrap();
        let index = SemanticIndex::build(&catalog, &test_profiles(profile.clone())).unwrap();
        let decision =
            index.candidate_decision("shared keyword request", Some("und"), &[], &profile, 32);
        assert!(decision.posting_saturated);
        assert!(decision.saturated_keys > 0);
        assert!(decision.rows.len() <= 32);
    }

    #[test]
    fn dangerous_exact_fanout_is_rejected_instead_of_becoming_catalog_order_authority() {
        let profile = SemanticProfile::empty();
        let patterns = (0..=SEMANTIC_EXACT_FANOUT_MAX)
            .map(|index| MeaningPattern::new(format!("collision.{index}"), ["same exact phrase"]))
            .collect::<Vec<_>>();
        let catalog = SemanticCatalog::new(patterns).unwrap();
        assert!(matches!(
            SemanticIndex::build(&catalog, &test_profiles(profile.clone())),
            Err(SemanticIndexBuildError::ExactFanoutExceeded { .. })
        ));
    }

    #[test]
    fn short_ambiguous_input_does_not_scan_catalog() {
        let profile = SemanticProfile::empty();
        let catalog =
            SemanticCatalog::new(vec![MeaningPattern::new("media", ["media manager"])]).unwrap();
        let index = SemanticIndex::build(&catalog, &test_profiles(profile.clone())).unwrap();
        let decision = index.candidate_decision("media", Some("und"), &[], &profile, 80);
        assert!(!decision.use_full_scan);
        assert!(decision.rows.is_empty());
        assert_eq!(decision.reason, "short_ambiguous_query_bounded");
    }
}
