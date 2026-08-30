//! Meaning scoring, evidence classification and deterministic tie ordering.

use super::{
    SemanticView,
    catalog::{MeaningClass, MeaningPattern},
    matching::{
        MatchKind, MatchedPair, SampleMatch, content_token_coverage,
        gap_tolerant_ordered_subsequence_match, relaxed_ordered_subsequence_match,
        sample_match_quality, sample_match_quality_for_view,
    },
    normalization::{language_is_compatible, ordered_tokens},
    profile::SemanticProfile,
    views::is_content_view,
};
use gvya_model::MeaningId;

#[cfg(test)]
use super::catalog::LocalizedSample;

const SAMPLE_WEIGHT_CAP: f64 = 0.55;

#[derive(Clone, Debug, PartialEq)]
pub struct ScoreBreakdown {
    pub sample_similarity: f64,
    pub best_sample_quality: f64,
    pub match_kind: MatchKind,
    pub match_view: String,
    pub matched_sample_language: Option<String>,
    pub match_span_start: Option<usize>,
    pub match_coverage: f64,
    pub numeric_score: f64,
    pub token_score: f64,
    pub char_score: f64,
    pub coverage_score: f64,
    pub order_score: f64,
    pub length_penalty: f64,
    pub numeric_window_text: String,
    pub matched_pairs: Vec<MatchedPair>,
    pub retrieval_rescue: f64,
    pub negative_penalty: f64,
    pub negative_matched: Option<String>,
    pub negative_hard_block: bool,
    pub negative_hard_block_sample: Option<String>,
    pub reported_speech_suppressed: bool,
    pub social_task_penalty: f64,
    pub negation_penalty: f64,
    pub exact_bonus: f64,
    pub evidence_tier: u8,
    pub evidence_strength: f64,
    pub no_evidence: bool,
    pub rejected_reason: Option<&'static str>,
}

impl Default for ScoreBreakdown {
    fn default() -> Self {
        Self {
            sample_similarity: 0.0,
            best_sample_quality: 0.0,
            match_kind: MatchKind::None,
            match_view: "normalized".to_string(),
            matched_sample_language: None,
            match_span_start: None,
            match_coverage: 0.0,
            numeric_score: 0.0,
            token_score: 0.0,
            char_score: 0.0,
            coverage_score: 0.0,
            order_score: 0.0,
            length_penalty: 0.0,
            numeric_window_text: String::new(),
            matched_pairs: Vec::new(),
            retrieval_rescue: 0.0,
            negative_penalty: 0.0,
            negative_matched: None,
            negative_hard_block: false,
            negative_hard_block_sample: None,
            reported_speech_suppressed: false,
            social_task_penalty: 0.0,
            negation_penalty: 0.0,
            exact_bonus: 0.0,
            evidence_tier: 5,
            evidence_strength: 0.0,
            no_evidence: true,
            rejected_reason: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ScoredMeaning {
    pub meaning: MeaningId,
    pub pattern_index: usize,
    pub score: f64,
    pub priority: i32,
    pub retrieval_rank_milli: u64,
    pub breakdown: ScoreBreakdown,
}

#[must_use]
pub fn score_pattern(
    profile: &SemanticProfile,
    views: &[SemanticView],
    pattern: &MeaningPattern,
    pattern_index: usize,
    retrieval_rank_milli: u64,
    requested_language: Option<&str>,
    language_fallbacks: &[String],
) -> ScoredMeaning {
    let normalized_view = views
        .iter()
        .find(|view| view.name == "normalized")
        .or_else(|| views.first());
    let input_tokens = normalized_view.map_or_else(Vec::new, |view| view.tokens.clone());
    let reporting =
        profile.has_reporting_context(&profile.normalize_colloquial_tokens(&input_tokens));
    let social = pattern.class == MeaningClass::Social;
    let mut breakdown = ScoreBreakdown::default();

    let mut best: Option<(SampleMatch, String, String)> = None;
    for view in views {
        for sample in &pattern.samples {
            if !language_is_compatible(requested_language, language_fallbacks, &sample.language) {
                continue;
            }
            let sample_norm = profile.normalize_text(&sample.text);
            let sample_tokens = ordered_tokens(&sample_norm);
            let normalized_sample_tokens = profile.normalize_colloquial_tokens(&sample_tokens);
            let lexical_social = normalized_sample_tokens
                .iter()
                .any(|token| profile.social_vocabulary.contains(token))
                && normalized_sample_tokens.iter().all(|token| {
                    profile.social_vocabulary.contains(token) || profile.is_pure_glue(token)
                });
            let candidate = sample_match_quality_for_view(
                profile,
                &view.tokens,
                &sample_tokens,
                is_content_view(&view.name),
                social || lexical_social,
                reporting,
            );
            if best
                .as_ref()
                .is_none_or(|(current, _, _)| candidate.quality > current.quality)
            {
                best = Some((candidate, view.name.clone(), sample.language.clone()));
            }
        }
    }
    if let Some((best, view_name, sample_language)) = best {
        breakdown.best_sample_quality = round4(best.quality);
        breakdown.sample_similarity =
            round4((best.quality * SAMPLE_WEIGHT_CAP).min(SAMPLE_WEIGHT_CAP));
        breakdown.match_kind = best.kind;
        breakdown.match_view = view_name;
        breakdown.matched_sample_language = Some(sample_language);
        breakdown.match_span_start = best.span_start;
        breakdown.match_coverage = round4(best.coverage);
        breakdown.numeric_score = round4(best.numeric_score);
        breakdown.token_score = round4(best.token_score);
        breakdown.char_score = round4(best.char_score);
        breakdown.coverage_score = round4(best.coverage_score);
        breakdown.order_score = round4(best.order_score);
        breakdown.length_penalty = round4(best.length_penalty);
        breakdown.numeric_window_text = best.numeric_window_text.clone();
        breakdown.matched_pairs = best.matched_pairs.clone();
        breakdown.social_task_penalty = round4(best.embedded_social_penalty);
        if best.kind == MatchKind::ReportedSpeechPenalized && social {
            breakdown.reported_speech_suppressed = true;
        }
    }

    // Domain/retrieval rescue now uses explicit authored metadata, never semantic meaning IDs.
    breakdown.retrieval_rescue = round4(retrieval_rescue(
        profile,
        &input_tokens,
        pattern,
        breakdown.match_kind,
        requested_language,
        language_fallbacks,
    ));

    // Authored negative samples are a bounded veto set and must be checked independently of
    // positive evidence. Otherwise an exact negative can be skipped precisely because the
    // positive samples are unrelated to it.
    if !pattern.negative_samples.is_empty() {
        let negative = negative_evidence(
            profile,
            views,
            pattern,
            reporting,
            requested_language,
            language_fallbacks,
        );
        // An authored exact positive sample is the clearest positive boundary the author can
        // provide. A merely partial/fuzzy negative must not demote that exact evidence; only an
        // exact/strong negative hard block may veto it. This keeps negatives useful for close
        // confounders without making a longer negative phrase accidentally cancel its own
        // shorter exact positive prefix (for example `what is a behavior` versus
        // `what is a fallback behavior`).
        let suppress_soft_negative =
            breakdown.match_kind == MatchKind::Exact && !negative.hard_block;
        breakdown.negative_penalty = if suppress_soft_negative {
            0.0
        } else {
            negative.penalty
        };
        breakdown.negative_matched = if suppress_soft_negative {
            None
        } else {
            negative.soft_sample
        };
        breakdown.negative_hard_block = negative.hard_block;
        breakdown.negative_hard_block_sample = negative.hard_sample;
    }

    if pattern.positive_assumption
        && profile.has_negation(&profile.normalize_colloquial_tokens(&input_tokens))
    {
        breakdown.negation_penalty = 0.25;
    }
    breakdown.exact_bonus = match breakdown.match_kind {
        MatchKind::Exact => 0.05,
        MatchKind::PhraseStart => 0.03,
        _ => 0.0,
    };

    if is_weak_numeric_only(&breakdown, profile, &input_tokens) {
        breakdown.sample_similarity = 0.0;
        breakdown.exact_bonus = 0.0;
    }

    breakdown.evidence_tier = classify_evidence_tier(&breakdown);
    breakdown.evidence_strength = round4(evidence_strength(&breakdown));
    breakdown.no_evidence = breakdown.evidence_tier >= 5
        && breakdown.retrieval_rescue <= 0.0
        && breakdown.evidence_strength <= 0.0;

    if breakdown.negative_hard_block {
        breakdown.rejected_reason = Some("negative_hard_block");
    } else if breakdown.reported_speech_suppressed {
        // The old engine capped social reported-speech score and blocked conversational answers
        // downstream. GVYA makes the semantic rejection explicit at the owning layer.
        breakdown.rejected_reason = Some("reported_speech_social_suppressed");
    }

    let score = if breakdown.rejected_reason.is_some() || breakdown.no_evidence {
        0.0
    } else {
        compose_score(&breakdown)
    };

    ScoredMeaning {
        meaning: pattern.id.clone(),
        pattern_index,
        score: round4(score),
        priority: pattern.priority.max(1),
        retrieval_rank_milli,
        breakdown,
    }
}

#[derive(Default)]
struct NegativeEvidence {
    penalty: f64,
    soft_sample: Option<String>,
    hard_block: bool,
    hard_sample: Option<String>,
}

fn negative_evidence(
    profile: &SemanticProfile,
    views: &[SemanticView],
    pattern: &MeaningPattern,
    user_reporting: bool,
    requested_language: Option<&str>,
    language_fallbacks: &[String],
) -> NegativeEvidence {
    let mut result = NegativeEvidence::default();
    let normalized = views
        .iter()
        .find(|view| view.name == "normalized")
        .or_else(|| views.first());
    let user_tokens = normalized.map_or_else(Vec::new, |view| {
        profile.normalize_colloquial_tokens(&view.tokens)
    });
    for negative in &pattern.negative_samples {
        if !language_is_compatible(requested_language, language_fallbacks, &negative.language) {
            continue;
        }
        let negative_norm = profile.normalize_text(&negative.text);
        let negative_tokens = profile.normalize_colloquial_tokens(&ordered_tokens(&negative_norm));
        if negative_tokens.is_empty() {
            continue;
        }
        let negative_reporting = profile.has_reporting_context(&negative_tokens);
        if negative_reporting && !user_reporting {
            continue;
        }

        let relaxed = relaxed_ordered_subsequence_match(profile, &negative_tokens, &user_tokens);

        // Negative samples author semantic boundaries, not only near-verbatim veto strings.
        // A natural paraphrase may omit glue or one explanatory noun while still carrying the
        // same boundary concept. Allow a *soft* penalty from partial content evidence only when
        // at least three meaningful negative tokens survive in order and account for at least
        // half of the weighted negative content. This path can never hard-block by itself.
        let negative_content = profile.content_tokens(&negative_tokens);
        let user_content = profile.content_tokens(&user_tokens);
        let content_relaxed =
            gap_tolerant_ordered_subsequence_match(profile, &negative_content, &user_content);
        let meaningful_matched_indices = content_relaxed
            .matched_pairs
            .iter()
            .filter(|pair| {
                pair.similarity >= 0.75
                    && negative_content
                        .get(pair.sample_index)
                        .is_some_and(|token| profile.token_weight(token) > 0.35)
            })
            .map(|pair| pair.sample_index)
            .collect::<Vec<_>>();
        let matched_content = meaningful_matched_indices.len();
        // Keep partial negative boundaries local: between two meaningful anchors, tolerate at
        // most one omitted negative-content token. This accepts a dropped explanatory noun
        // (`meaning [class] social general`) without treating a distant shared frame
        // (`split [text string] into fragments`) as the same authored boundary.
        let bounded_internal_gap = meaningful_matched_indices
            .windows(2)
            .all(|pair| pair[1].saturating_sub(pair[0]) <= 2);
        let bounded_partial_content = negative_content.len() >= 3
            && matched_content >= 3
            && content_relaxed.recall >= 0.50
            && bounded_internal_gap;

        if relaxed.recall >= 0.82 || bounded_partial_content {
            let mut best_quality: f64 = 0.0;
            let mut best_kind = MatchKind::None;
            for view in views {
                let m = sample_match_quality(
                    profile,
                    &view.tokens,
                    &negative_tokens,
                    pattern.class == MeaningClass::Social,
                    user_reporting,
                );
                if m.quality > best_quality {
                    best_quality = m.quality;
                    best_kind = m.kind;
                }
            }
            if best_quality >= 0.25 || bounded_partial_content {
                let penalty: f64 = if best_quality >= 0.75 || best_kind == MatchKind::Exact {
                    0.60
                } else {
                    0.35
                };
                if penalty > result.penalty {
                    result.penalty = penalty;
                    result.soft_sample = Some(negative.text.clone());
                }
            }
        }

        for view in views {
            let m = sample_match_quality(
                profile,
                &view.tokens,
                &negative_tokens,
                pattern.class == MeaningClass::Social,
                user_reporting,
            );
            if usable_negative_hard_block(profile, &m, &view.tokens, &negative_tokens) {
                result.hard_block = true;
                if result.hard_sample.is_none() {
                    result.hard_sample = Some(negative.text.clone());
                }
                break;
            }
        }
    }
    result
}

fn usable_negative_hard_block(
    profile: &SemanticProfile,
    matched: &SampleMatch,
    input: &[String],
    negative: &[String],
) -> bool {
    if matched.kind == MatchKind::Exact {
        return matched.quality >= 0.98;
    }
    if !matches!(
        matched.kind,
        MatchKind::PhraseStart
            | MatchKind::PhraseSpan
            | MatchKind::PhraseEndShort
            | MatchKind::ContentCoverage
    ) || matched.quality < 0.94
    {
        return false;
    }
    let negative_content = profile.content_tokens(&profile.normalize_colloquial_tokens(negative));
    if negative_content.len() < 2 {
        return false;
    }
    content_token_coverage(profile, &negative_content, input)
        .is_some_and(|coverage| coverage.complete)
}

fn retrieval_metadata_token_match(profile: &SemanticProfile, query: &str, candidate: &str) -> bool {
    if profile.strict_token_match(query, candidate) {
        return true;
    }
    let similarity = super::matching::token_similarity(profile, query, candidate);
    if similarity >= 0.86 {
        return true;
    }
    // Retrieval metadata may use a productive one-character suffix variant (for example
    // `type`/`typed` or Persian `تایپ`/`تایپی`). Keep this strictly prefix-bounded so unrelated
    // same-length edit neighbors such as `correction`/`correlation` never become metadata hits.
    let query_len = query.chars().count();
    let candidate_len = candidate.chars().count();
    query_len.min(candidate_len) >= 4
        && query_len.abs_diff(candidate_len) == 1
        && (query.starts_with(candidate) || candidate.starts_with(query))
}

fn retrieval_rescue(
    profile: &SemanticProfile,
    input: &[String],
    pattern: &MeaningPattern,
    match_kind: MatchKind,
    requested_language: Option<&str>,
    language_fallbacks: &[String],
) -> f64 {
    if pattern.class == MeaningClass::Social || pattern.retrieval_terms.is_empty() {
        return 0.0;
    }
    let input_content = profile.content_tokens(&profile.normalize_colloquial_tokens(input));
    if input_content.is_empty() {
        return 0.0;
    }
    let has_task_cue = input.iter().any(|token| profile.is_task_cue(token));
    let mut metadata = Vec::new();
    for term in &pattern.retrieval_terms {
        if !language_is_compatible(requested_language, language_fallbacks, &term.language) {
            continue;
        }
        metadata.extend(
            profile.content_tokens(
                &profile.normalize_colloquial_tokens(&ordered_tokens(
                    &profile.normalize_text(&term.text),
                )),
            ),
        );
    }
    metadata.sort();
    metadata.dedup();
    if metadata.is_empty() {
        return 0.0;
    }
    // Authored retrieval terms are topical evidence. Authored question starters are pure
    // interrogative scaffolding and carry no subject matter, so they must not manufacture that
    // evidence: otherwise any unrelated question reaching an authored question word already
    // owns one of the two hits the rescue tier bar needs. Task cues stay counted -- in this
    // domain an authored task verb such as `build` or `save` is frequently the topic itself.
    let mut unique_input: Vec<String> = input_content
        .into_iter()
        .filter(|token| !profile.is_question_starter(token))
        .collect();
    unique_input.sort();
    unique_input.dedup();
    if unique_input.is_empty() {
        return 0.0;
    }
    let mut hits = 0usize;
    let mut strong_hits = 0usize;
    for query in &unique_input {
        if metadata
            .iter()
            .any(|candidate| retrieval_metadata_token_match(profile, query, candidate))
        {
            hits += 1;
            if query.chars().count() >= 5 {
                strong_hits += 1;
            }
        }
    }
    if hits == 0 {
        return 0.0;
    }
    if hits == 1 && !has_task_cue {
        return 0.0;
    }
    if match_kind == MatchKind::None {
        // Retrieval metadata can rescue a paraphrase, but it must not manufacture an answer from
        // two generic topical words buried in an otherwise unrelated utterance. With no positive
        // sample evidence at all, require the authored metadata to explain most of the user's
        // meaningful tokens. Strong sample/numeric evidence keeps the existing rescue behavior.
        let topical_density = hits as f64 / unique_input.len().max(1) as f64;
        if topical_density <= 0.50 {
            return 0.0;
        }
    }
    let mut score = if hits == 1 {
        if strong_hits > 0 { 0.36 } else { 0.28 }
    } else {
        // Retrieval terms are explicit authored evidence. Once two distinct metadata tokens
        // occur, unrelated natural-language filler must not dilute that evidence merely because
        // the user asked a complete sentence. Specificity and ambiguity competition still keep
        // this below complete positive-sample evidence.
        let extra_hits = hits.saturating_sub(2).min(3) as f64;
        let specificity = strong_hits as f64 / hits as f64;
        0.52 + extra_hits * 0.08 + specificity * 0.08 + if has_task_cue { 0.04 } else { 0.0 }
    };
    if matches!(
        match_kind,
        MatchKind::Exact
            | MatchKind::PhraseStart
            | MatchKind::ContentCoverage
            | MatchKind::RelaxedSubsequence
            | MatchKind::PhraseTypo
    ) {
        score *= 0.35;
    }
    score.min(0.82)
}

fn classify_evidence_tier(breakdown: &ScoreBreakdown) -> u8 {
    let sample_tier: u8 = match breakdown.match_kind {
        MatchKind::Exact => 1,
        MatchKind::PhraseStart
        | MatchKind::PhraseEndShort
        | MatchKind::RelaxedSubsequence
        | MatchKind::ContentCoverage => 2,
        MatchKind::PhraseSpan
            if breakdown.numeric_score >= 0.78 && breakdown.coverage_score >= 1.0 =>
        {
            3
        }
        MatchKind::PhraseTypo
            if breakdown.char_score >= 0.72 && breakdown.coverage_score >= 0.75 =>
        {
            3
        }
        MatchKind::NumericWindow
            if breakdown.numeric_score >= 0.72 && breakdown.coverage_score >= 0.85 =>
        {
            3
        }
        MatchKind::PhraseSpan
        | MatchKind::PhraseTypo
        | MatchKind::EmbeddedSocialPenalized
        | MatchKind::ReportedSpeechPenalized
        | MatchKind::NumericWindow => 4,
        MatchKind::None => 5,
    };
    let tier = if breakdown.retrieval_rescue >= 0.58 {
        sample_tier.min(3)
    } else {
        sample_tier
    };
    // A soft authored negative is explicit counterevidence. It already lowers the numeric score;
    // it must also surrender one level of lexical authority, otherwise tier-first ordering can
    // leave a heavily penalized umbrella Meaning above a clean, more specific candidate.
    if breakdown.negative_penalty > 0.0 {
        tier.saturating_add(1).min(5)
    } else {
        tier
    }
}

fn evidence_strength(breakdown: &ScoreBreakdown) -> f64 {
    match breakdown.evidence_tier {
        1 => (breakdown.sample_similarity + breakdown.exact_bonus).min(1.0),
        2 => (breakdown.sample_similarity + breakdown.numeric_score * 0.55).min(1.0),
        3 => (breakdown.sample_similarity + breakdown.numeric_score * 0.55)
            .max(breakdown.retrieval_rescue * 0.82)
            .min(1.0),
        4 => (breakdown.sample_similarity * 0.85 + breakdown.numeric_score * 0.15).min(1.0),
        _ => 0.0,
    }
}

fn compose_score(breakdown: &ScoreBreakdown) -> f64 {
    let tier_base = f64::from(6 - breakdown.evidence_tier.min(5)) / 5.0;
    let rescue_scale = match breakdown.evidence_tier {
        5 => 1.0,
        4 => 0.25,
        _ => 0.10,
    };
    let raw = tier_base * 0.30
        + breakdown.evidence_strength * 0.55
        + breakdown.retrieval_rescue * rescue_scale
        - breakdown.negation_penalty
        - breakdown.negative_penalty
        - breakdown.social_task_penalty;
    raw.clamp(0.0, 1.0)
}

fn is_weak_numeric_only(
    breakdown: &ScoreBreakdown,
    profile: &SemanticProfile,
    input: &[String],
) -> bool {
    if breakdown.match_kind != MatchKind::NumericWindow {
        return false;
    }
    // Weak-numeric lexical exclusions are authored profile data, never a private language list.
    let normalized = profile.normalize_colloquial_tokens(input);
    let content: Vec<&str> = normalized
        .iter()
        .map(String::as_str)
        .filter(|token| !profile.weak_numeric_ignore.contains(*token))
        .collect();
    if content.is_empty() {
        return breakdown.numeric_score < 0.62;
    }

    let window_tokens: Vec<&str> = breakdown.numeric_window_text.split_whitespace().collect();
    let mut matched = Vec::new();
    for pair in &breakdown.matched_pairs {
        if pair.similarity < 0.75 {
            continue;
        }
        if let Some(token) = window_tokens.get(pair.input_index) {
            if !matched.contains(token) {
                matched.push(*token);
            }
        }
    }
    !content.iter().any(|token| matched.contains(token))
}

/// Strict deterministic semantic order. Near-tie priority grouping is applied by the kernel only
/// after this total order establishes fixed group anchors; pairwise epsilon comparison is not a
/// valid sort comparator because it can be non-transitive.
#[must_use]
pub fn compare_scored(
    left: &ScoredMeaning,
    right: &ScoredMeaning,
    left_id: &str,
    right_id: &str,
) -> std::cmp::Ordering {
    left.breakdown
        .rejected_reason
        .is_some()
        .cmp(&right.breakdown.rejected_reason.is_some())
        .then_with(|| {
            left.breakdown
                .evidence_tier
                .cmp(&right.breakdown.evidence_tier)
        })
        .then_with(|| right.score.total_cmp(&left.score))
        .then_with(|| {
            right
                .breakdown
                .evidence_strength
                .total_cmp(&left.breakdown.evidence_strength)
        })
        .then_with(|| right.priority.cmp(&left.priority))
        .then_with(|| right.retrieval_rank_milli.cmp(&left.retrieval_rank_milli))
        .then_with(|| left_id.cmp(right_id))
}

fn round4(value: f64) -> f64 {
    (value * 10_000.0).round() / 10_000.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::semantic::{build_semantic_views, catalog::MeaningPattern};

    fn scoring_test_profile() -> SemanticProfile {
        let mut profile = SemanticProfile::empty();
        profile
            .pure_glue
            .extend(["the", "a", "what", "i"].into_iter().map(str::to_owned));
        profile.reporting_verbs.extend(
            ["said", "shouted", "printed"]
                .into_iter()
                .map(str::to_owned),
        );
        profile.reporting_nouns.insert("sign".to_owned());
        profile
            .social_vocabulary
            .extend(["hello", "thank", "you"].into_iter().map(str::to_owned));
        profile
            .negations
            .extend(["not", "never", "nah"].into_iter().map(str::to_owned));
        profile
    }

    fn view(name: &str, tokens: &[&str]) -> SemanticView {
        SemanticView {
            name: name.to_owned(),
            text: tokens.join(" "),
            tokens: tokens.iter().map(|token| (*token).to_owned()).collect(),
        }
    }

    // The utterance itself shares no vocabulary with the sample, so the injected view is the
    // only one that can match. These three cases differ only in view name / length, which is
    // exactly the surface the content-view predicate controls.
    const UNRELATED_UTTERANCE: &str = "zzz qqq www";

    fn score_injected_view(name: &str, tokens: &[&str]) -> ScoreBreakdown {
        let profile = scoring_test_profile();
        let mut pattern = MeaningPattern::new("packages", ["gvya packages"]);
        pattern.samples = vec![LocalizedSample::new("en-US", "gvya packages")];
        let views = build_semantic_views(
            UNRELATED_UTTERANCE,
            &profile,
            Some(vec![view(name, tokens)]),
        );
        score_pattern(&profile, &views.views, &pattern, 0, 0, Some("en-US"), &[]).breakdown
    }

    #[test]
    fn short_generic_prefix_is_downgraded_in_the_typo_content_view() {
        let breakdown = score_injected_view(
            "typo_content",
            &["gvya", "packages", "dependencies", "composed", "bot"],
        );
        assert_eq!(breakdown.match_view, "typo_content");
        assert_eq!(breakdown.match_kind, MatchKind::PhraseSpan);
        assert!(
            breakdown.best_sample_quality <= 0.74,
            "typo-repaired content view must share the short-prefix bound, got {}",
            breakdown.best_sample_quality
        );
    }

    #[test]
    fn typo_text_view_keeps_full_prefix_authority() {
        let breakdown = score_injected_view(
            "typo_text",
            &["gvya", "packages", "dependencies", "composed", "bot"],
        );
        assert_eq!(breakdown.match_view, "typo_text");
        assert_eq!(
            breakdown.match_kind,
            MatchKind::PhraseStart,
            "glue-preserving views must keep unchanged prefix semantics"
        );
    }

    #[test]
    fn short_generic_sample_still_owns_the_short_utterance_in_content_views() {
        let breakdown = score_injected_view("typo_content", &["gvya", "packages"]);
        assert_eq!(breakdown.match_view, "typo_content");
        assert_eq!(
            breakdown.match_kind,
            MatchKind::Exact,
            "a short utterance must still be owned by the short generic sample"
        );
    }

    #[test]
    fn soft_negative_demotes_lexical_authority_before_tier_first_sorting() {
        let clean = ScoreBreakdown {
            match_kind: MatchKind::PhraseStart,
            ..ScoreBreakdown::default()
        };
        let penalized = ScoreBreakdown {
            match_kind: MatchKind::PhraseStart,
            negative_penalty: 0.35,
            ..ScoreBreakdown::default()
        };

        assert_eq!(classify_evidence_tier(&clean), 2);
        assert_eq!(classify_evidence_tier(&penalized), 3);

        let generic = ScoredMeaning {
            meaning: MeaningId::new("generic"),
            pattern_index: 0,
            score: 0.2042,
            priority: 0,
            retrieval_rank_milli: 273_174,
            breakdown: ScoreBreakdown {
                evidence_tier: classify_evidence_tier(&penalized),
                evidence_strength: 0.9876,
                negative_penalty: 0.35,
                ..ScoreBreakdown::default()
            },
        };
        let specific = ScoredMeaning {
            meaning: MeaningId::new("specific"),
            pattern_index: 1,
            score: 0.6318,
            priority: 0,
            retrieval_rank_milli: 540_449,
            breakdown: ScoreBreakdown {
                evidence_tier: 3,
                evidence_strength: 0.6724,
                ..ScoreBreakdown::default()
            },
        };
        let mut rows = vec![generic, specific];
        rows.sort_by(|left, right| {
            compare_scored(left, right, left.meaning.as_str(), right.meaning.as_str())
        });
        assert_eq!(rows[0].meaning.as_str(), "specific");
    }

    #[test]
    fn semantic_order_is_transitive_across_near_tie_boundaries() {
        fn row(score: f64, priority: i32) -> ScoredMeaning {
            ScoredMeaning {
                meaning: MeaningId::new("test"),
                pattern_index: 0,
                score,
                priority,
                retrieval_rank_milli: 0,
                breakdown: ScoreBreakdown {
                    evidence_tier: 2,
                    evidence_strength: score,
                    ..ScoreBreakdown::default()
                },
            }
        }

        // These values formed a comparator cycle when pairwise epsilon checks allowed priority
        // to override A/B and B/C, but score to override A/C.
        let a = row(0.95, 1);
        let b = row(0.93, 2);
        let c = row(0.91, 3);
        assert!(compare_scored(&a, &b, "a", "b").is_lt());
        assert!(compare_scored(&b, &c, "b", "c").is_lt());
        assert!(compare_scored(&a, &c, "a", "c").is_lt());

        let mut rows = [(c, "c"), (a, "a"), (b, "b")];
        rows.sort_by(|(left, left_id), (right, right_id)| {
            compare_scored(left, right, left_id, right_id)
        });
        assert_eq!(rows.map(|(_, id)| id), ["a", "b", "c"]);
    }

    #[test]
    fn discriminative_embedded_phrase_is_tier_three() {
        let strong = ScoreBreakdown {
            match_kind: MatchKind::PhraseSpan,
            numeric_score: 0.80,
            coverage_score: 1.0,
            ..ScoreBreakdown::default()
        };
        let weak = ScoreBreakdown {
            match_kind: MatchKind::PhraseSpan,
            numeric_score: 0.70,
            coverage_score: 1.0,
            ..ScoreBreakdown::default()
        };
        assert_eq!(classify_evidence_tier(&strong), 3);
        assert_eq!(classify_evidence_tier(&weak), 4);
    }

    #[test]
    fn authored_retrieval_accepts_bounded_productive_suffix_variant() {
        let profile = SemanticProfile::empty();
        let mut pattern = MeaningPattern::new("typo", ["placeholder sample"]);
        pattern.retrieval_terms = vec![LocalizedSample::new("fa-IR", "غلط تایپی")];
        let input = ordered_tokens("غلط تایپ");
        let rescue = retrieval_rescue(
            &profile,
            &input,
            &pattern,
            MatchKind::NumericWindow,
            Some("fa-IR"),
            &[],
        );
        assert!(
            rescue >= 0.50,
            "expected two-token authored retrieval evidence, got {rescue}"
        );
        assert!(!retrieval_metadata_token_match(
            &profile,
            "correction",
            "correlation"
        ));
    }

    fn offdomain_probe_profile() -> SemanticProfile {
        let mut profile = scoring_test_profile();
        for token in ["how", "what", "why", "when", "which"] {
            profile
                .continuation_question_starters
                .insert(token.to_owned());
        }
        for token in ["change", "build", "install", "find", "save"] {
            profile.task_cues.insert(token.to_owned());
        }
        profile
    }

    fn rescue_for(input: &str, terms: &[&str]) -> f64 {
        let profile = offdomain_probe_profile();
        let mut pattern = MeaningPattern::new("probe", ["placeholder sample"]);
        pattern.retrieval_terms = terms
            .iter()
            .map(|text| LocalizedSample::new("en-US", *text))
            .collect();
        retrieval_rescue(
            &profile,
            &ordered_tokens(&profile.normalize_text(input)),
            &pattern,
            MatchKind::NumericWindow,
            Some("en-US"),
            &[],
        )
    }

    #[test]
    fn retrieval_only_evidence_requires_topical_density() {
        let profile = offdomain_probe_profile();
        let mut pattern = MeaningPattern::new("cli", ["command line tool"]);
        pattern.retrieval_terms = vec![LocalizedSample::new(
            "en-US",
            "inspect behavior schema authored object",
        )];
        let input = ordered_tokens(&profile.normalize_text("inspect css behavior browser"));
        let rescue = retrieval_rescue(
            &profile,
            &input,
            &pattern,
            MatchKind::None,
            Some("en-US"),
            &[],
        );
        assert_eq!(rescue, 0.0);

        let focused = ordered_tokens(&profile.normalize_text("inspect behavior schema"));
        let focused_rescue = retrieval_rescue(
            &profile,
            &focused,
            &pattern,
            MatchKind::None,
            Some("en-US"),
            &[],
        );
        assert!(focused_rescue >= 0.58);
    }

    #[test]
    fn question_scaffolding_alone_cannot_manufacture_retrieval_evidence() {
        // Independent off-domain utterances that each collide with an authored retrieval term
        // only through a question starter plus a bare task verb.
        for (utterance, terms) in [
            (
                "why do leaves change color in autumn",
                &["why trace", "proof promoting authored change"][..],
            ),
            (
                "when should i change the oil in my car",
                &["when to change a response"][..],
            ),
            (
                "which knife do i use to build a sandwich",
                &["which candidate", "build order"][..],
            ),
        ] {
            let rescue = rescue_for(utterance, terms);
            assert!(
                rescue < 0.58,
                "scaffolding must not carry {utterance:?} to the rescue tier bar, got {rescue}"
            );
        }
    }

    #[test]
    fn topical_retrieval_evidence_survives_the_scaffolding_filter() {
        // Close boundary: the same shape of utterance, but the hits are real subject-matter
        // tokens rather than scaffolding.
        let rescue = rescue_for(
            "why does the compiler emit a portable artifact",
            &["compiler artifact", "portable emit"],
        );
        assert!(
            rescue >= 0.58,
            "topical authored retrieval must still rescue, got {rescue}"
        );
    }

    #[test]
    fn authored_task_verbs_remain_topical_retrieval_evidence() {
        // Close boundary the other way: `build` and `save` are authored task cues, but in this
        // domain they are the subject of the question and must keep their retrieval evidence.
        for (utterance, terms) in [
            (
                "what does gvya build when bot content changes",
                &["build artifact"][..],
            ),
            (
                "what must my host save after every turn",
                &["host save turn"][..],
            ),
        ] {
            let rescue = rescue_for(utterance, terms);
            assert!(
                rescue > 0.0,
                "authored task verbs must stay topical for {utterance:?}, got {rescue}"
            );
        }
    }

    #[test]
    fn single_topical_hit_with_task_cue_is_unchanged_by_the_filter() {
        let rescue = rescue_for("how do i install a package", &["package audit"]);
        assert!(
            rescue > 0.0 && rescue < 0.58,
            "one topical hit stays bounded below the rescue tier bar, got {rescue}"
        );
    }

    #[test]
    fn distinct_authored_retrieval_hits_survive_natural_question_filler() {
        let mut profile = scoring_test_profile();
        profile.task_cues.insert("rebuild".to_owned());
        let mut pattern = MeaningPattern::new("engine-build", ["compile portable artifact"]);
        pattern.retrieval_terms = vec![
            LocalizedSample::new("en-US", "engine wasm"),
            LocalizedSample::new("en-US", "rebuild response"),
        ];
        let views = build_semantic_views(
            "after editing a response should i rebuild the wasm engine",
            &profile,
            None,
        );
        let scored = score_pattern(&profile, &views.views, &pattern, 0, 0, Some("en-US"), &[]);
        assert_eq!(scored.breakdown.evidence_tier, 3);
        assert!(scored.breakdown.retrieval_rescue >= 0.58);
        assert!(scored.score >= 0.45);
    }

    #[test]
    fn rejected_high_tier_candidate_sorts_after_viable_general_evidence() {
        let rejected = ScoredMeaning {
            meaning: MeaningId::new("social"),
            pattern_index: 0,
            score: 0.0,
            priority: 1,
            retrieval_rank_milli: 10,
            breakdown: ScoreBreakdown {
                evidence_tier: 1,
                rejected_reason: Some("social_wrapper_competed_by_general_meaning"),
                ..ScoreBreakdown::default()
            },
        };
        let viable = ScoredMeaning {
            meaning: MeaningId::new("general"),
            pattern_index: 1,
            score: 0.52,
            priority: 1,
            retrieval_rank_milli: 1,
            breakdown: ScoreBreakdown {
                evidence_tier: 3,
                evidence_strength: 0.55,
                ..ScoreBreakdown::default()
            },
        };
        assert!(compare_scored(&viable, &rejected, "general", "social").is_lt());
        assert!(compare_scored(&rejected, &viable, "social", "general").is_gt());
    }

    #[test]
    fn one_retrieval_token_cannot_resolve_a_general_meaning() {
        let mut profile = scoring_test_profile();
        profile.task_cues.insert("explain".to_owned());
        let mut pattern = MeaningPattern::new("bot-help", ["configure portable dialogue"]);
        pattern.retrieval_terms = vec![LocalizedSample::new("en-US", "bot")];
        let views = build_semantic_views("explain this bot", &profile, None);
        let scored = score_pattern(&profile, &views.views, &pattern, 0, 0, Some("en-US"), &[]);
        assert_eq!(scored.breakdown.evidence_tier, 5);
        assert!(scored.score < 0.45);
    }

    #[test]
    fn authored_negative_boundary_accepts_bounded_three_token_paraphrase() {
        let profile = scoring_test_profile();
        let mut pattern = MeaningPattern::new("meaning", ["what is a meaning"]);
        pattern.negative_samples = vec![LocalizedSample::new(
            "en-US",
            "meaning class social general classification",
        )];
        let views = build_semantic_views(
            "why is a meaning marked social versus general instead of inferring it from the id",
            &profile,
            None,
        );
        let scored = score_pattern(&profile, &views.views, &pattern, 0, 0, Some("en-US"), &[]);
        assert_eq!(scored.breakdown.negative_penalty, 0.35);
        assert!(!scored.breakdown.negative_hard_block);
    }

    #[test]
    fn partial_negative_boundary_requires_three_meaningful_tokens() {
        let profile = scoring_test_profile();
        let mut pattern = MeaningPattern::new("packages", ["what is a package"]);
        pattern.negative_samples = vec![LocalizedSample::new(
            "en-US",
            "package dependency graph composition order",
        )];
        let views = build_semantic_views("what is a package dependency", &profile, None);
        let scored = score_pattern(&profile, &views.views, &pattern, 0, 0, Some("en-US"), &[]);
        assert_eq!(scored.breakdown.negative_penalty, 0.0);
        assert!(!scored.breakdown.negative_hard_block);
    }

    #[test]
    fn partial_negative_boundary_rejects_two_missing_content_tokens_between_anchors() {
        let profile = scoring_test_profile();
        let mut pattern = MeaningPattern::new("fragment-index", ["package source fragments"]);
        pattern.negative_samples = vec![LocalizedSample::new(
            "en-US",
            "split a text string into fragments",
        )];
        let views = build_semantic_views(
            "why are package sources split into fragments",
            &profile,
            None,
        );
        let scored = score_pattern(&profile, &views.views, &pattern, 0, 0, Some("en-US"), &[]);
        assert_eq!(scored.breakdown.negative_penalty, 0.0);
        assert!(!scored.breakdown.negative_hard_block);
    }

    #[test]
    fn exact_positive_is_not_demoted_by_partial_soft_negative() {
        let profile = scoring_test_profile();
        let mut pattern = MeaningPattern::new("behavior", ["what is a behavior"]);
        pattern.samples = vec![LocalizedSample::new("en-US", "what is a behavior")];
        pattern.negative_samples = vec![
            LocalizedSample::new("en-US", "what is a fallback behavior"),
            LocalizedSample::new("en-US", "what is behavior eligibility"),
        ];
        let views = build_semantic_views("what is a behavior", &profile, None);
        let scored = score_pattern(&profile, &views.views, &pattern, 0, 0, Some("en-US"), &[]);
        assert_eq!(scored.breakdown.match_kind, MatchKind::Exact);
        assert_eq!(scored.breakdown.negative_penalty, 0.0);
        assert!(!scored.breakdown.negative_hard_block);
        assert_eq!(scored.breakdown.evidence_tier, 1);
        assert!(
            scored.score >= 0.45,
            "exact authored sample must resolve, got {}",
            scored.score
        );
    }

    #[test]
    fn negative_exact_is_hard_blocked() {
        let profile = scoring_test_profile();
        let mut pattern = MeaningPattern::new("allow", ["tell me what to do"]);
        pattern.negative_samples = vec![LocalizedSample::new("und", "what can you do")];
        let views = build_semantic_views("what can you do", &profile, None);
        let scored = score_pattern(&profile, &views.views, &pattern, 0, 0, Some("und"), &[]);
        assert!(scored.breakdown.negative_hard_block);
        assert_eq!(scored.score, 0.0);
    }

    #[test]
    fn fuzzy_single_token_negative_is_not_a_hard_veto() {
        let profile = scoring_test_profile();
        let mut pattern = MeaningPattern::new("install", ["install addon"]);
        pattern.negative_samples = vec![LocalizedSample::new("und", "uninstall")];
        let views = build_semantic_views("install addon", &profile, None);
        let scored = score_pattern(&profile, &views.views, &pattern, 0, 0, Some("und"), &[]);
        assert!(!scored.breakdown.negative_hard_block);
        assert!(scored.score > 0.0);
    }

    #[test]
    fn evidence_strength_is_capped_like_the_proven_floor() {
        let mut breakdown = ScoreBreakdown::default();
        breakdown.evidence_tier = 2;
        breakdown.sample_similarity = 0.55;
        breakdown.numeric_score = 1.0;
        assert_eq!(evidence_strength(&breakdown), 1.0);
    }

    #[test]
    fn numeric_window_with_real_content_match_is_not_weak() {
        let profile = scoring_test_profile();
        let mut breakdown = ScoreBreakdown::default();
        breakdown.match_kind = MatchKind::NumericWindow;
        breakdown.numeric_score = 0.75;
        breakdown.numeric_window_text = "install addon".into();
        breakdown.matched_pairs = vec![MatchedPair {
            sample_index: 0,
            input_index: 1,
            similarity: 1.0,
        }];
        assert!(!is_weak_numeric_only(
            &breakdown,
            &profile,
            &["what".into(), "addon".into()]
        ));
    }

    #[test]
    fn reported_speech_social_candidate_is_explicitly_rejected() {
        let profile = scoring_test_profile();
        let mut pattern = MeaningPattern::new("gratitude", ["thank you"]);
        pattern.class = MeaningClass::Social;
        let views = build_semantic_views("he said thank you", &profile, None);
        let scored = score_pattern(&profile, &views.views, &pattern, 0, 0, Some("und"), &[]);
        assert_eq!(
            scored.breakdown.rejected_reason,
            Some("reported_speech_social_suppressed")
        );
        assert_eq!(scored.score, 0.0);
    }
    #[test]
    fn span_aware_reported_speech_uses_authored_markers() {
        let profile = scoring_test_profile();
        let mut pattern = MeaningPattern::new("hello", ["hello"]);
        pattern.class = MeaningClass::Social;
        for utterance in ["he shouted hello", "the sign printed hello"] {
            pattern.samples = vec![LocalizedSample::new("und", "hello")];
            let views = build_semantic_views(utterance, &profile, None);
            let scored = score_pattern(&profile, &views.views, &pattern, 0, 0, Some("und"), &[]);
            assert_eq!(
                scored.breakdown.rejected_reason,
                Some("reported_speech_social_suppressed"),
                "{utterance}"
            );
            assert_eq!(scored.score, 0.0, "{utterance}");
        }
    }

    #[test]
    fn authored_negation_markers_penalize_positive_assumptions() {
        let profile = scoring_test_profile();
        let mut pattern = MeaningPattern::new("positive", ["i want access"]);
        pattern.positive_assumption = true;
        for utterance in ["nah i want access", "never i want access"] {
            let views = build_semantic_views(utterance, &profile, None);
            let scored = score_pattern(&profile, &views.views, &pattern, 0, 0, Some("und"), &[]);
            assert_eq!(scored.breakdown.negation_penalty, 0.25, "{utterance}");
        }
    }
}
