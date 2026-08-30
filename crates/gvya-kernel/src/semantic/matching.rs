//! Deterministic lexical/sample matching primitives.

use super::profile::SemanticProfile;

const NUMERIC_WINDOW_MAX: f64 = 0.86;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MatchKind {
    None,
    Exact,
    PhraseStart,
    PhraseSpan,
    PhraseEndShort,
    RelaxedSubsequence,
    ContentCoverage,
    NumericWindow,
    PhraseTypo,
    EmbeddedSocialPenalized,
    ReportedSpeechPenalized,
}

impl MatchKind {
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Exact => "exact",
            Self::PhraseStart => "phrase_start",
            Self::PhraseSpan => "phrase_span",
            Self::PhraseEndShort => "phrase_end_short",
            Self::RelaxedSubsequence => "relaxed_subsequence",
            Self::ContentCoverage => "content_coverage",
            Self::NumericWindow => "numeric_window",
            Self::PhraseTypo => "phrase_typo",
            Self::EmbeddedSocialPenalized => "embedded_social_penalized",
            Self::ReportedSpeechPenalized => "reported_speech_penalized",
        }
    }

    #[must_use]
    pub const fn is_strong_negative_scan_kind(self) -> bool {
        matches!(
            self,
            Self::Exact
                | Self::PhraseStart
                | Self::PhraseSpan
                | Self::PhraseEndShort
                | Self::ContentCoverage
                | Self::RelaxedSubsequence
        )
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct MatchedPair {
    pub sample_index: usize,
    pub input_index: usize,
    pub similarity: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SampleMatch {
    pub quality: f64,
    pub kind: MatchKind,
    pub span_start: Option<usize>,
    pub coverage: f64,
    pub starts_at_beginning: bool,
    pub embedded_social_penalty: f64,
    pub numeric_score: f64,
    pub numeric_window_text: String,
    pub numeric_sample: String,
    pub token_score: f64,
    pub char_score: f64,
    pub coverage_score: f64,
    pub order_score: f64,
    pub length_penalty: f64,
    pub matched_pairs: Vec<MatchedPair>,
}

impl Default for SampleMatch {
    fn default() -> Self {
        Self {
            quality: 0.0,
            kind: MatchKind::None,
            span_start: None,
            coverage: 0.0,
            starts_at_beginning: false,
            embedded_social_penalty: 0.0,
            numeric_score: 0.0,
            numeric_window_text: String::new(),
            numeric_sample: String::new(),
            token_score: 0.0,
            char_score: 0.0,
            coverage_score: 0.0,
            order_score: 0.0,
            length_penalty: 0.0,
            matched_pairs: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct RelaxedMatch {
    pub score: f64,
    pub recall: f64,
    pub order_score: f64,
    pub complete: bool,
    pub starts_at_beginning: bool,
    pub first_match: Option<usize>,
    pub last_match: Option<usize>,
    pub matched_pairs: Vec<MatchedPair>,
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct GapTolerantOrderedMatch {
    pub(super) recall: f64,
    pub(super) matched_pairs: Vec<MatchedPair>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TokenF1 {
    pub score: f64,
    pub recall: f64,
    pub precision: f64,
    pub order_score: f64,
    pub matched_pairs: Vec<MatchedPair>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct NumericSimilarity {
    pub score: f64,
    pub token_score: f64,
    pub char_score: f64,
    pub coverage_score: f64,
    pub order_score: f64,
    pub length_penalty: f64,
    pub matched_pairs: Vec<MatchedPair>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ContentCoverage {
    pub coverage: f64,
    pub matched: usize,
    pub total: usize,
    pub complete: bool,
    pub span_start: Option<usize>,
    pub span_end: Option<usize>,
    pub window_text: String,
}

#[must_use]
pub fn find_token_span(input: &[String], sample: &[String]) -> Option<usize> {
    if sample.is_empty() || sample.len() > input.len() {
        return None;
    }
    (0..=input.len() - sample.len()).find(|start| input[*start..*start + sample.len()] == *sample)
}

#[must_use]
pub fn edit_similarity(left: &str, right: &str) -> f64 {
    if left == right {
        return 1.0;
    }
    let a: Vec<char> = left.chars().collect();
    let b: Vec<char> = right.chars().collect();
    let max_len = a.len().max(b.len());
    if max_len == 0 {
        return 1.0;
    }
    if a.len().min(b.len()) < 3 {
        return 0.0;
    }
    let diff = a.len().abs_diff(b.len());
    if diff > 3 {
        return (1.0 - diff as f64 / max_len as f64).max(0.0);
    }
    let mut previous: Vec<usize> = (0..=b.len()).collect();
    let mut current = vec![0; b.len() + 1];
    for (i, ca) in a.iter().enumerate() {
        current[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let cost = usize::from(ca != cb);
            current[j + 1] = (previous[j + 1] + 1)
                .min(current[j] + 1)
                .min(previous[j] + cost);
        }
        std::mem::swap(&mut previous, &mut current);
    }
    1.0 - previous[b.len()] as f64 / max_len as f64
}

#[must_use]
fn damerau_edit_similarity(left: &str, right: &str) -> f64 {
    if left == right {
        return 1.0;
    }
    let a: Vec<char> = left.chars().collect();
    let b: Vec<char> = right.chars().collect();
    let max_len = a.len().max(b.len());
    if max_len == 0 {
        return 1.0;
    }
    let mut previous_previous: Option<Vec<usize>> = None;
    let mut previous: Vec<usize> = (0..=b.len()).collect();
    for i in 1..=a.len() {
        let mut current = vec![0; b.len() + 1];
        current[0] = i;
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
        }
        previous_previous = Some(previous);
        previous = current;
    }
    1.0 - previous[b.len()] as f64 / max_len as f64
}

#[must_use]
pub fn likely_stem_match(left: &str, right: &str) -> bool {
    if left.chars().count() < 5 || right.chars().count() < 5 {
        return false;
    }
    let (short, long) = if left.chars().count() <= right.chars().count() {
        (left, right)
    } else {
        (right, left)
    };
    long.starts_with(short)
}

#[must_use]
pub fn token_similarity(profile: &SemanticProfile, left: &str, right: &str) -> f64 {
    if profile.strict_token_match(left, right) {
        return 1.0;
    }
    let edit = edit_similarity(left, right);
    if left.chars().count() >= 4 && right.chars().count() >= 4 && edit >= 0.80 {
        return 0.85;
    }
    if likely_stem_match(left, right) {
        let short = left.chars().count().min(right.chars().count()) as f64;
        let long = left.chars().count().max(right.chars().count()) as f64;
        return if short / long >= 0.65 { 0.82 } else { 0.75 };
    }
    0.0
}

#[must_use]
pub fn content_token_coverage(
    profile: &SemanticProfile,
    sample_tokens: &[String],
    input_tokens: &[String],
) -> Option<ContentCoverage> {
    let sample_full = profile.normalize_colloquial_tokens(sample_tokens);
    let sample_content = profile.content_tokens(&sample_full);
    let input_full = profile.normalize_colloquial_tokens(input_tokens);
    let mut input_content = Vec::new();
    let mut content_to_full = Vec::new();
    for (full_index, token) in input_full.iter().enumerate() {
        if profile.is_pure_glue(token) {
            continue;
        }
        content_to_full.push(full_index);
        input_content.push(token.clone());
    }
    if sample_content.is_empty() || input_content.is_empty() {
        return None;
    }

    let mut used = vec![false; input_content.len()];
    let mut matched = 0usize;
    let mut matched_full = Vec::new();
    for sample in &sample_content {
        let found = input_content
            .iter()
            .enumerate()
            .find(|(index, input)| !used[*index] && profile.strict_token_match(sample, input))
            .map(|(index, _)| index);
        let Some(found) = found else {
            return Some(ContentCoverage {
                coverage: matched as f64 / sample_content.len() as f64,
                matched,
                total: sample_content.len(),
                complete: false,
                span_start: None,
                span_end: None,
                window_text: String::new(),
            });
        };
        used[found] = true;
        matched += 1;
        matched_full.push(content_to_full[found]);
    }

    matched_full.sort_unstable();
    let span_start = matched_full.first().copied();
    let span_end = matched_full.last().copied();
    let window_text = match (span_start, span_end) {
        (Some(start), Some(end)) if end >= start => input_full[start..=end].join(" "),
        _ => String::new(),
    };

    // Preserve the generic-content guard: multi-token samples must not collapse to a single
    // content token and become authoritative through glue removal alone.
    let complete = !(sample_content.len() == 1 && sample_full.len() >= 2);
    Some(ContentCoverage {
        coverage: 1.0,
        matched,
        total: sample_content.len(),
        complete,
        span_start: complete.then_some(span_start).flatten(),
        span_end: complete.then_some(span_end).flatten(),
        window_text: if complete { window_text } else { String::new() },
    })
}

#[must_use]
pub fn relaxed_ordered_subsequence_match(
    profile: &SemanticProfile,
    sample: &[String],
    input: &[String],
) -> RelaxedMatch {
    if sample.is_empty() || input.is_empty() {
        return RelaxedMatch {
            score: 0.0,
            recall: 0.0,
            order_score: 0.0,
            complete: false,
            starts_at_beginning: false,
            first_match: None,
            last_match: None,
            matched_pairs: Vec::new(),
        };
    }
    let mut input_index = 0;
    let mut matched_pairs = Vec::new();
    let mut matched_weight = 0.0;
    let total_weight: f64 = sample.iter().map(|token| profile.token_weight(token)).sum();
    for (sample_index, sample_token) in sample.iter().enumerate() {
        let mut found = None;
        while input_index < input.len() {
            let similarity = token_similarity(profile, sample_token, &input[input_index]);
            if similarity >= 0.75 {
                found = Some((input_index, similarity));
                input_index += 1;
                break;
            }
            input_index += 1;
        }
        if let Some((matched_index, similarity)) = found {
            matched_weight += profile.token_weight(sample_token) * similarity;
            matched_pairs.push(MatchedPair {
                sample_index,
                input_index: matched_index,
                similarity,
            });
        }
    }
    let recall = if total_weight > 0.0 {
        matched_weight / total_weight
    } else {
        0.0
    };
    let complete = matched_pairs.len() == sample.len();
    let first_match = matched_pairs.first().map(|pair| pair.input_index);
    let last_match = matched_pairs.last().map(|pair| pair.input_index);
    // The ordered scan makes every accepted pair monotonic. Order score is therefore the
    // completion fraction when incomplete and 1.0 when complete.
    let order_score = if complete {
        1.0
    } else {
        matched_pairs.len() as f64 / sample.len() as f64
    };
    let starts_at_beginning = first_match.is_some_and(|first| {
        input[..first]
            .iter()
            .all(|token| profile.token_weight(token) <= 0.35)
    });
    let extra_important = last_match.map_or(0, |last| {
        input
            .iter()
            .skip(last + 1)
            .filter(|token| profile.token_weight(token) > 0.35)
            .count()
    });
    let extra_penalty = (extra_important as f64 * 0.025).min(0.12);
    let incomplete_penalty = if complete { 0.0 } else { (1.0 - recall) * 0.25 };
    let score = (recall * 0.82 + if complete { 0.12 } else { 0.0 } + order_score * 0.06
        - extra_penalty
        - incomplete_penalty)
        .clamp(0.0, 1.0);
    RelaxedMatch {
        score,
        recall,
        order_score,
        complete,
        starts_at_beginning,
        first_match,
        last_match,
        matched_pairs,
    }
}

#[must_use]
pub(super) fn gap_tolerant_ordered_subsequence_match(
    profile: &SemanticProfile,
    sample: &[String],
    input: &[String],
) -> GapTolerantOrderedMatch {
    if sample.is_empty() || input.is_empty() {
        return GapTolerantOrderedMatch {
            recall: 0.0,
            matched_pairs: Vec::new(),
        };
    }

    let mut input_index = 0;
    let mut matched_pairs = Vec::new();
    let mut matched_weight = 0.0;
    let total_weight: f64 = sample.iter().map(|token| profile.token_weight(token)).sum();

    for (sample_index, sample_token) in sample.iter().enumerate() {
        let mut cursor = input_index;
        let mut found = None;
        while cursor < input.len() {
            let similarity = token_similarity(profile, sample_token, &input[cursor]);
            if similarity >= 0.75 {
                found = Some((cursor, similarity));
                break;
            }
            cursor += 1;
        }

        if let Some((matched_index, similarity)) = found {
            input_index = matched_index + 1;
            matched_weight += profile.token_weight(sample_token) * similarity;
            matched_pairs.push(MatchedPair {
                sample_index,
                input_index: matched_index,
                similarity,
            });
        }
    }

    let recall = if total_weight > 0.0 {
        matched_weight / total_weight
    } else {
        0.0
    };

    GapTolerantOrderedMatch {
        recall,
        matched_pairs,
    }
}

#[must_use]
pub fn weighted_token_f1(
    profile: &SemanticProfile,
    sample: &[String],
    window: &[String],
) -> TokenF1 {
    if sample.is_empty() || window.is_empty() {
        return TokenF1 {
            score: 0.0,
            recall: 0.0,
            precision: 0.0,
            order_score: 0.0,
            matched_pairs: Vec::new(),
        };
    }
    let mut used = vec![false; window.len()];
    let mut pairs = Vec::new();
    let mut matched_sample_weight = 0.0;
    let mut matched_window_weight = 0.0;
    let sample_total: f64 = sample
        .iter()
        .map(|t| profile.token_weight(t).max(0.2))
        .sum();
    let window_total: f64 = window
        .iter()
        .map(|t| profile.token_weight(t).max(0.2))
        .sum();
    for (sample_index, sample_token) in sample.iter().enumerate() {
        let mut best: Option<(usize, f64)> = None;
        for (window_index, window_token) in window.iter().enumerate() {
            if used[window_index] {
                continue;
            }
            let similarity = token_similarity(profile, sample_token, window_token);
            if similarity <= 0.0 {
                continue;
            }
            if best.map_or(true, |(_, current)| similarity > current) {
                best = Some((window_index, similarity));
            }
        }
        if let Some((window_index, similarity)) = best {
            used[window_index] = true;
            matched_sample_weight += profile.token_weight(sample_token) * similarity;
            matched_window_weight += profile.token_weight(&window[window_index]);
            pairs.push(MatchedPair {
                sample_index,
                input_index: window_index,
                similarity,
            });
        }
    }
    let recall = if sample_total > 0.0 {
        matched_sample_weight / sample_total
    } else {
        0.0
    };
    let precision = if window_total > 0.0 {
        matched_window_weight / window_total
    } else {
        0.0
    };
    let score = if recall + precision > 0.0 {
        2.0 * recall * precision / (recall + precision)
    } else {
        0.0
    };
    let order_score = if pairs.is_empty() {
        0.0
    } else {
        let ordered_after_first = pairs
            .windows(2)
            .filter(|pair| pair[0].input_index < pair[1].input_index)
            .count();
        (1 + ordered_after_first) as f64 / pairs.len() as f64
    };
    TokenF1 {
        score,
        recall,
        precision,
        order_score,
        matched_pairs: pairs,
    }
}

#[must_use]
pub fn numeric_window_similarity(
    profile: &SemanticProfile,
    sample: &[String],
    window: &[String],
) -> NumericSimilarity {
    let token = weighted_token_f1(profile, sample, window);
    let sample_text = sample.join(" ");
    let window_text = window.join(" ");
    let char_score = edit_similarity(&sample_text, &window_text).clamp(0.0, 1.0);
    let coverage_score = token.recall;
    let length_penalty =
        sample.len().abs_diff(window.len()) as f64 / sample.len().max(window.len()).max(1) as f64;
    let score =
        (token.score * 0.55 + char_score * 0.20 + coverage_score * 0.15 + token.order_score * 0.10
            - length_penalty * 0.20)
            .clamp(0.0, 1.0);
    NumericSimilarity {
        score,
        token_score: token.score,
        char_score,
        coverage_score,
        order_score: token.order_score,
        length_penalty,
        matched_pairs: token.matched_pairs,
    }
}

#[must_use]
pub fn sample_match_quality(
    profile: &SemanticProfile,
    input: &[String],
    sample: &[String],
    social: bool,
    reporting: bool,
) -> SampleMatch {
    sample_match_quality_for_view(profile, input, sample, false, social, reporting)
}

/// Generic authored-sample prior. For otherwise comparable non-exact evidence, shorter
/// authored samples receive more weight than longer samples. This never depends on MeaningClass.
/// Exact full-utterance matches remain 1.0.
fn authored_sample_length_bonus(sample_len: usize) -> f64 {
    (0.24 / sample_len.max(1) as f64).min(0.18)
}

fn is_direct_embedded_singleton_cue(profile: &SemanticProfile, sample: &[String]) -> bool {
    sample.len() == 1 && profile.social_vocabulary.contains(&sample[0])
}

fn ambiguous_singleton_in_larger_utterance(
    profile: &SemanticProfile,
    input: &[String],
    sample: &[String],
) -> bool {
    if sample.len() != 1
        || !profile.is_generic_singleton(&sample[0])
        || is_direct_embedded_singleton_cue(profile, sample)
    {
        return false;
    }
    profile
        .content_tokens(input)
        .into_iter()
        .any(|token| token.as_str() != sample[0].as_str())
}

fn weak_short_content_prefix(
    profile: &SemanticProfile,
    input: &[String],
    sample: &[String],
    content_view: bool,
) -> bool {
    content_view
        && sample.len() <= 2
        && input.len() >= sample.len().saturating_add(3)
        && !sample.iter().any(|token| profile.is_task_cue(token))
}

fn weak_glue_heavy_prefix(profile: &SemanticProfile, input: &[String], sample: &[String]) -> bool {
    if sample.len() > 4 || sample.iter().any(|token| profile.is_task_cue(token)) {
        return false;
    }
    let sample_content = profile.content_tokens(sample);
    let input_content = profile.content_tokens(input);
    sample_content.len() <= 1 && input_content.len() >= sample_content.len().saturating_add(2)
}

fn weak_short_generic_subsequence(
    profile: &SemanticProfile,
    input: &[String],
    sample: &[String],
) -> bool {
    let sample_content = profile.content_tokens(sample);
    let input_content = profile.content_tokens(input);
    // Genericness is authored profile taxonomy, never matcher vocabulary. A short sample only
    // loses ordered-subsequence authority when every content token it carries is an authored
    // generic singleton, which is what makes it an umbrella heading rather than a specialised
    // pair such as an authored `env network` boundary.
    sample_content.len() <= 2
        && input_content.len() >= sample_content.len().saturating_add(3)
        && sample_content
            .iter()
            .all(|token| profile.is_generic_singleton(token))
        && !sample_content
            .iter()
            .any(|token| profile.is_task_cue(token))
}

pub(crate) fn sample_match_quality_for_view(
    profile: &SemanticProfile,
    input: &[String],
    sample: &[String],
    content_view: bool,
    social: bool,
    reporting: bool,
) -> SampleMatch {
    if input.is_empty() || sample.is_empty() {
        return SampleMatch::default();
    }
    let input_col = if content_view {
        input.to_vec()
    } else {
        profile.normalize_colloquial_tokens(input)
    };
    let original_sample = profile.normalize_colloquial_tokens(sample);
    let sample_col = if content_view {
        profile.content_tokens(&original_sample)
    } else {
        original_sample.clone()
    };
    if sample_col.is_empty() {
        return SampleMatch::default();
    }
    let coverage = (sample_col.len() as f64 / input_col.len().max(1) as f64).min(1.0);
    let discriminative_content = profile
        .content_tokens(&sample_col)
        .iter()
        .filter(|token| profile.token_weight(token) > 0.35)
        .count()
        >= 2;
    let length_bonus = authored_sample_length_bonus(sample_col.len());
    let input_joined = input_col.join(" ");
    let sample_joined = sample_col.join(" ");
    let mut matched = if input_joined == sample_joined {
        SampleMatch {
            quality: 1.0,
            kind: MatchKind::Exact,
            span_start: Some(0),
            coverage,
            starts_at_beginning: true,
            numeric_score: 1.0,
            numeric_window_text: input_joined.clone(),
            numeric_sample: sample_joined.clone(),
            token_score: 1.0,
            char_score: 1.0,
            coverage_score: 1.0,
            order_score: 1.0,
            ..SampleMatch::default()
        }
    } else if let Some(span) =
        if content_view && sample_col.len() == 1 && original_sample.len() >= 2 {
            None
        } else {
            find_token_span(&input_col, &sample_col)
        }
    {
        let at_start = span == 0;
        let direct_singleton_cue = is_direct_embedded_singleton_cue(profile, &sample_col);
        let mut quality = if at_start {
            // Sample length is generic authored evidence: a four-word prefix outranks an
            // otherwise comparable six-word prefix without any MeaningClass exception.
            (0.90 + length_bonus + coverage * 0.02).min(0.98)
        } else if direct_singleton_cue {
            // Direct singleton cues come from the language profile, not MeaningClass. The same
            // authored token receives the same lexical treatment regardless of its owner.
            (0.72 + length_bonus + coverage * 0.02).min(0.94)
        } else if discriminative_content {
            (0.76 + length_bonus + coverage * 0.04).min(0.90)
        } else {
            (0.35 + length_bonus + coverage * 0.25).min(0.78)
        };
        let mut kind = if at_start {
            MatchKind::PhraseStart
        } else {
            MatchKind::PhraseSpan
        };
        if at_start
            && (weak_short_content_prefix(profile, &input_col, &sample_col, content_view)
                || weak_glue_heavy_prefix(profile, &input_col, &sample_col))
        {
            // A short prefix extracted only after glue removal is weak evidence when most of the
            // user's meaningful utterance remains. Keep exact/normalized prefix semantics intact
            // and preserve explicit task cues, but do not let a generic two-token heading own a
            // longer subtopic merely because it happens to start the content view.
            quality = (0.62 + coverage * 0.20).min(0.74);
            kind = MatchKind::PhraseSpan;
        }
        if ambiguous_singleton_in_larger_utterance(profile, &input_col, &sample_col) {
            // Ambiguous one-word headings stay visible as evidence but cannot own unrelated
            // continuation text by themselves. This safety rule is lexical/profile-driven and
            // independent of MeaningClass; direct singleton cues remain eligible.
            quality = quality.min(0.74);
            kind = MatchKind::PhraseSpan;
        }
        let sample_content_count = profile.content_tokens(&sample_col).len();
        if !at_start
            && span + sample_col.len() == input_col.len()
            && input_col.len() <= 6
            && sample_content_count >= 2
        {
            // Phrase-end authority is for compact topical phrases, not generic singleton tails.
            // A trailing `what`, `time`, or equivalent one-content-token sample inside a larger
            // utterance stays ordinary span evidence; exact short utterances are unchanged.
            quality = quality.max(0.72);
            kind = MatchKind::PhraseEndShort;
        }
        SampleMatch {
            quality,
            kind,
            span_start: Some(span),
            coverage,
            starts_at_beginning: at_start,
            numeric_score: quality,
            numeric_window_text: input_col[span..span + sample_col.len()].join(" "),
            numeric_sample: sample_joined.clone(),
            token_score: 1.0,
            char_score: 1.0,
            coverage_score: 1.0,
            order_score: 1.0,
            ..SampleMatch::default()
        }
    } else {
        let relaxed = relaxed_ordered_subsequence_match(profile, &sample_col, &input_col);
        let coverage_guard = content_token_coverage(profile, &original_sample, &input_col);
        let original_content_count = profile.content_tokens(&original_sample).len();
        let input_content_count = profile.content_tokens(&input_col).len();
        let relaxed_allowed = coverage_guard
            .as_ref()
            .is_some_and(|coverage| coverage.complete)
            || !(original_content_count <= 1
                && original_sample.len() >= 2
                && input_content_count > 2);
        if relaxed.complete && relaxed.recall >= 0.78 && relaxed.score >= 0.60 && relaxed_allowed {
            let start = relaxed.first_match.unwrap_or(0);
            let at_start = relaxed.starts_at_beginning;
            let weak_prefix = at_start
                && (weak_short_content_prefix(profile, &input_col, &sample_col, content_view)
                    || weak_glue_heavy_prefix(profile, &input_col, &sample_col));
            let weak_scattered =
                !at_start && weak_short_generic_subsequence(profile, &input_col, &sample_col);
            let ambiguous_singleton =
                ambiguous_singleton_in_larger_utterance(profile, &input_col, &sample_col);
            let quality = if weak_prefix || ambiguous_singleton {
                relaxed.score.min(0.74)
            } else if at_start {
                (0.90 + authored_sample_length_bonus(sample_col.len()) + relaxed.recall * 0.02)
                    .min(0.98)
            } else if weak_scattered {
                // A short generic sample scattered through a much longer utterance still leaves
                // most of the message unexplained; length priority does not override that guard.
                relaxed.score.min(0.74)
            } else {
                (relaxed.score + authored_sample_length_bonus(sample_col.len())).min(0.88)
            };
            let end = relaxed.last_match.unwrap_or(start);
            SampleMatch {
                quality,
                kind: if weak_prefix || weak_scattered || ambiguous_singleton {
                    MatchKind::PhraseSpan
                } else if at_start {
                    MatchKind::PhraseStart
                } else {
                    MatchKind::RelaxedSubsequence
                },
                span_start: relaxed.first_match,
                coverage,
                starts_at_beginning: at_start,
                numeric_score: relaxed.score,
                numeric_window_text: if end >= start && end < input_col.len() {
                    input_col[start..=end].join(" ")
                } else {
                    input_joined.clone()
                },
                numeric_sample: sample_joined.clone(),
                token_score: relaxed.recall,
                char_score: relaxed.score,
                coverage_score: relaxed.recall,
                order_score: relaxed.order_score,
                matched_pairs: relaxed.matched_pairs,
                ..SampleMatch::default()
            }
        } else if let Some(content) = coverage_guard.filter(|coverage| coverage.complete) {
            if content.total < 2
                || (content.total <= 2 && original_sample.len() >= content.total + 3)
            {
                numeric_window_sample_match(profile, &input_col, &original_sample)
            } else {
                let at_start = content.span_start == Some(0);
                let mut quality: f64 = if content.total <= 3 { 0.92 } else { 0.86 };
                if at_start && content.total <= 2 {
                    quality = (quality + 0.04).min(0.98);
                }
                SampleMatch {
                    quality,
                    kind: MatchKind::ContentCoverage,
                    span_start: content.span_start,
                    coverage,
                    starts_at_beginning: at_start,
                    numeric_score: quality,
                    numeric_window_text: content.window_text,
                    numeric_sample: original_sample.join(" "),
                    token_score: 1.0,
                    char_score: quality,
                    coverage_score: 1.0,
                    order_score: 1.0,
                    ..SampleMatch::default()
                }
            }
        } else {
            let numeric = numeric_window_sample_match(profile, &input_col, &original_sample);
            if content_view {
                numeric
            } else {
                let phrase_typo = phrase_typo_sample_match(profile, &input_col, &original_sample);
                if phrase_typo.quality > numeric.quality {
                    phrase_typo
                } else {
                    numeric
                }
            }
        }
    };
    let reporting = reporting
        || matched
            .span_start
            .is_some_and(|span| profile.has_reporting_context_before_span(&input_col, span));
    finalize_social_and_reporting(social, reporting, &mut matched);
    matched
}

fn phrase_typo_sample_match(
    profile: &SemanticProfile,
    input: &[String],
    sample: &[String],
) -> SampleMatch {
    if input.len() != sample.len() || !(2..=8).contains(&input.len()) {
        return SampleMatch::default();
    }
    let input_joined = input.join(" ");
    let sample_joined = sample.join(" ");
    let input_chars = input_joined.chars().count();
    let sample_chars = sample_joined.chars().count();
    let max_chars = input_chars.max(sample_chars);
    let min_chars = input_chars.min(sample_chars);
    if !(5..=96).contains(&max_chars) || min_chars * 4 < max_chars * 3 {
        return SampleMatch::default();
    }

    let mut corrections = 0usize;
    let mut pairs = Vec::with_capacity(sample.len());
    let mut token_similarity_total = 0.0;
    for (index, (input_token, sample_token)) in input.iter().zip(sample).enumerate() {
        let similarity = if profile.strict_token_match(input_token, sample_token) {
            1.0
        } else {
            corrections += 1;
            if corrections > 2 {
                return SampleMatch::default();
            }
            let input_len = input_token.chars().count();
            let sample_len = sample_token.chars().count();
            if input_len.min(sample_len) < 3 || input_len.max(sample_len) > 32 {
                return SampleMatch::default();
            }
            if input_len == 3
                && sample_len == 3
                && !is_adjacent_transposition(input_token, sample_token)
            {
                // Three-character substitutions are far too semantically dense to rescue
                // safely (`boy`/`bot`, `can`/`car`, ...). Keep the useful short-token case that
                // motivated phrase rescue -- an obvious adjacent swap such as `jbo` -> `job` --
                // while refusing equal-length substitutions at this shortest supported length.
                return SampleMatch::default();
            }
            if input_token.chars().next() != sample_token.chars().next() {
                return SampleMatch::default();
            }
            let similarity = damerau_edit_similarity(input_token, sample_token);
            if similarity < 0.60 {
                return SampleMatch::default();
            }
            similarity
        };
        token_similarity_total += similarity;
        pairs.push(MatchedPair {
            sample_index: index,
            input_index: index,
            similarity,
        });
    }
    if corrections == 0 {
        return SampleMatch::default();
    }

    let char_similarity = damerau_edit_similarity(&input_joined, &sample_joined);
    if char_similarity < 0.72 {
        return SampleMatch::default();
    }
    let token_score = token_similarity_total / sample.len() as f64;
    let length_ratio = min_chars as f64 / max_chars as f64;
    let quality = (char_similarity * 0.78 + token_score * 0.22).min(0.88);
    SampleMatch {
        quality,
        kind: MatchKind::PhraseTypo,
        span_start: Some(0),
        coverage: length_ratio,
        starts_at_beginning: true,
        numeric_score: char_similarity,
        numeric_window_text: input_joined,
        numeric_sample: sample_joined,
        token_score,
        char_score: char_similarity,
        coverage_score: length_ratio,
        order_score: 1.0,
        length_penalty: 1.0 - length_ratio,
        matched_pairs: pairs,
        ..SampleMatch::default()
    }
}

fn is_adjacent_transposition(left: &str, right: &str) -> bool {
    let left: Vec<char> = left.chars().collect();
    let right: Vec<char> = right.chars().collect();
    if left.len() != right.len() || left.len() < 2 || left == right {
        return false;
    }
    let mismatches: Vec<usize> = left
        .iter()
        .zip(&right)
        .enumerate()
        .filter_map(|(index, (left, right))| (left != right).then_some(index))
        .collect();
    matches!(mismatches.as_slice(), [first, second]
        if *second == *first + 1
            && left[*first] == right[*second]
            && left[*second] == right[*first])
}

fn numeric_window_sample_match(
    profile: &SemanticProfile,
    input: &[String],
    sample: &[String],
) -> SampleMatch {
    if sample.len() == 1 {
        if let Some(start) = input
            .iter()
            .position(|token| token_similarity(profile, token, &sample[0]) >= 1.0)
        {
            return SampleMatch {
                quality: 1.0,
                kind: if start == 0 {
                    MatchKind::Exact
                } else {
                    MatchKind::PhraseSpan
                },
                span_start: Some(start),
                coverage: 1.0 / input.len().max(1) as f64,
                starts_at_beginning: start == 0,
                numeric_score: 1.0,
                numeric_window_text: input[start].clone(),
                numeric_sample: sample[0].clone(),
                token_score: 1.0,
                char_score: 1.0,
                coverage_score: 1.0,
                order_score: 1.0,
                matched_pairs: vec![MatchedPair {
                    sample_index: 0,
                    input_index: 0,
                    similarity: 1.0,
                }],
                ..SampleMatch::default()
            };
        }
        return SampleMatch::default();
    }

    // Preserve the proven RC2 window ordering and bounds. Build at most 30 raw windows in
    // insertion order (-1, 0, +1, then -2/+2 for samples >= 4), then retain at most 20
    // windows that contain lexical signal. Do not let filtering change the 30-window frontier.
    let message_len = input.len();
    if message_len == 0 {
        return SampleMatch::default();
    }
    let mut sizes = Vec::new();
    for delta in [-1isize, 0, 1] {
        let requested = (sample.len() as isize + delta).max(1) as usize;
        let size = requested.min(message_len);
        if !sizes.contains(&size) {
            sizes.push(size);
        }
    }
    if sample.len() >= 4 {
        for delta in [-2isize, 2] {
            let requested = (sample.len() as isize + delta).max(1) as usize;
            let size = requested.min(message_len);
            if !sizes.contains(&size) {
                sizes.push(size);
            }
        }
    }

    let has_token_signal = sample.iter().any(|sample_token| {
        input
            .iter()
            .any(|input_token| token_similarity(profile, sample_token, input_token) >= 0.75)
    });
    if !has_token_signal {
        return SampleMatch::default();
    }

    let mut raw_windows = Vec::new();
    'sizes: for size in sizes {
        for start in 0..=message_len - size {
            raw_windows.push((start, size));
            if raw_windows.len() >= 30 {
                break 'sizes;
            }
        }
    }

    let mut signal_windows = Vec::new();
    for (start, size) in raw_windows {
        let window = &input[start..start + size];
        let keep = window.iter().any(|window_token| {
            sample
                .iter()
                .any(|sample_token| token_similarity(profile, sample_token, window_token) >= 0.75)
        });
        if keep {
            signal_windows.push((start, size));
            if signal_windows.len() >= 20 {
                break;
            }
        }
    }

    let mut best: Option<(usize, usize, NumericSimilarity)> = None;
    for (start, size) in signal_windows {
        let similarity = numeric_window_similarity(profile, sample, &input[start..start + size]);
        if similarity.score <= 0.0 {
            continue;
        }
        if best
            .as_ref()
            .is_none_or(|(_, _, current)| similarity.score > current.score)
        {
            best = Some((start, size, similarity));
        }
    }

    // The old numeric helper could return a stronger relaxed ordered-subsequence match after
    // window scoring. The main path already attempts relaxed matching first, but preserving the
    // fallback here keeps this primitive faithful when called independently or after guards.
    let relaxed = relaxed_ordered_subsequence_match(profile, sample, input);
    let best_score = best
        .as_ref()
        .map_or(0.0, |(_, _, similarity)| similarity.score);
    if relaxed.complete
        && relaxed.recall >= 0.78
        && relaxed.score >= 0.60
        && (relaxed.score > best_score || best_score <= 0.0)
    {
        let start = relaxed.first_match.unwrap_or(0);
        let end = relaxed.last_match.unwrap_or(start);
        let starts_at_beginning = relaxed.starts_at_beginning;
        let quality = if starts_at_beginning {
            (0.90 + authored_sample_length_bonus(sample.len()) + relaxed.recall * 0.02).min(0.98)
        } else {
            (relaxed.score + authored_sample_length_bonus(sample.len())).min(0.88)
        };
        return SampleMatch {
            quality,
            kind: if starts_at_beginning {
                MatchKind::PhraseStart
            } else {
                MatchKind::RelaxedSubsequence
            },
            span_start: relaxed.first_match,
            coverage: (sample.len() as f64 / input.len().max(1) as f64).min(1.0),
            starts_at_beginning,
            numeric_score: relaxed.score,
            numeric_window_text: if end >= start && end < input.len() {
                input[start..=end].join(" ")
            } else {
                input.join(" ")
            },
            numeric_sample: sample.join(" "),
            token_score: relaxed.recall,
            char_score: relaxed.score,
            coverage_score: relaxed.recall,
            order_score: relaxed.order_score,
            matched_pairs: relaxed.matched_pairs,
            ..SampleMatch::default()
        };
    }

    let Some((start, size, similarity)) = best else {
        return SampleMatch::default();
    };
    SampleMatch {
        quality: (similarity.score + authored_sample_length_bonus(sample.len()))
            .min(NUMERIC_WINDOW_MAX),
        kind: MatchKind::NumericWindow,
        span_start: Some(start),
        coverage: (sample.len() as f64 / input.len().max(1) as f64).min(1.0),
        starts_at_beginning: start == 0,
        numeric_score: similarity.score,
        numeric_window_text: input[start..start + size].join(" "),
        numeric_sample: sample.join(" "),
        token_score: similarity.token_score,
        char_score: similarity.char_score,
        coverage_score: similarity.coverage_score,
        order_score: similarity.order_score,
        length_penalty: similarity.length_penalty,
        matched_pairs: similarity.matched_pairs,
        ..SampleMatch::default()
    }
}

fn finalize_social_and_reporting(social: bool, reporting: bool, matched: &mut SampleMatch) {
    if matched.kind == MatchKind::None {
        return;
    }
    if social {
        if reporting {
            matched.quality = matched.quality.min(0.20);
            matched.kind = MatchKind::ReportedSpeechPenalized;
        }
    } else if reporting
        && matches!(
            matched.kind,
            MatchKind::NumericWindow | MatchKind::PhraseSpan | MatchKind::PhraseTypo
        )
    {
        matched.quality = (matched.quality - 0.20).max(0.0);
        matched.kind = MatchKind::ReportedSpeechPenalized;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strings(values: &[&str]) -> Vec<String> {
        values.iter().map(|s| (*s).to_string()).collect()
    }

    fn matching_test_profile() -> SemanticProfile {
        let mut profile = SemanticProfile::empty();
        profile.canonical_tokens.extend([
            ("children".to_owned(), "child".to_owned()),
            ("boxes".to_owned(), "box".to_owned()),
        ]);
        profile.pure_glue.extend(
            ["please", "the", "what", "is", "tell", "me"]
                .into_iter()
                .map(str::to_owned),
        );
        profile.very_low_weight.insert("the".to_owned());
        profile.low_weight.insert("please".to_owned());
        profile.generic_singletons.insert("what".to_owned());
        profile.reporting_verbs.insert("said".to_owned());
        profile
    }

    #[test]
    fn whole_phrase_typo_rescues_two_short_token_errors() {
        let profile = SemanticProfile::empty();
        let matched = sample_match_quality(
            &profile,
            &strings(&["nic", "jbo"]),
            &strings(&["nice", "job"]),
            false,
            false,
        );
        assert_eq!(matched.kind, MatchKind::PhraseTypo);
        assert!(matched.quality >= 0.72, "{}", matched.quality);
    }

    #[test]
    fn whole_phrase_typo_rejects_semantic_word_substitution() {
        let profile = SemanticProfile::empty();
        let matched = sample_match_quality(
            &profile,
            &strings(&["nice", "car"]),
            &strings(&["nice", "job"]),
            false,
            false,
        );
        assert_ne!(matched.kind, MatchKind::PhraseTypo);
    }

    #[test]
    fn whole_phrase_typo_rejects_three_letter_same_prefix_substitution() {
        let profile = SemanticProfile::empty();
        let matched = sample_match_quality(
            &profile,
            &strings(&["good", "boy"]),
            &strings(&["good", "bot"]),
            false,
            false,
        );
        assert_ne!(matched.kind, MatchKind::PhraseTypo);
    }

    #[test]
    fn whole_phrase_typo_keeps_three_letter_adjacent_transposition() {
        let profile = SemanticProfile::empty();
        let matched = sample_match_quality(
            &profile,
            &strings(&["nice", "jbo"]),
            &strings(&["nice", "job"]),
            false,
            false,
        );
        assert_eq!(matched.kind, MatchKind::PhraseTypo);
    }

    #[test]
    fn whole_phrase_typo_rejects_different_token_shapes() {
        let profile = SemanticProfile::empty();
        let matched = sample_match_quality(
            &profile,
            &strings(&["how", "do", "you", "work"]),
            &strings(&["how", "are", "you"]),
            false,
            false,
        );
        assert_ne!(matched.kind, MatchKind::PhraseTypo);
    }

    #[test]
    fn inflection_and_edit_similarity_are_bounded() {
        let profile = matching_test_profile();
        assert_eq!(token_similarity(&profile, "children", "child"), 1.0);
        assert_eq!(token_similarity(&profile, "boxes", "box"), 1.0);
        assert!(token_similarity(&profile, "manager", "manger") >= 0.85);
        assert!(likely_stem_match("manage", "manager"));
        assert!(!likely_stem_match("managex", "manager"));
        assert_eq!(token_similarity(&profile, "cat", "cut"), 0.0);
    }

    #[test]
    fn preserves_exact_phrase_content_and_relaxed_shapes() {
        let profile = matching_test_profile();
        assert_eq!(
            sample_match_quality(
                &profile,
                &strings(&["open", "door"]),
                &strings(&["open", "door"]),
                false,
                false
            )
            .kind,
            MatchKind::Exact
        );
        assert_eq!(
            sample_match_quality(
                &profile,
                &strings(&["open", "door", "please"]),
                &strings(&["open", "door"]),
                false,
                false
            )
            .kind,
            MatchKind::PhraseStart
        );
        let relaxed = sample_match_quality(
            &profile,
            &strings(&["please", "open", "the", "door"]),
            &strings(&["open", "door"]),
            false,
            false,
        );
        assert_eq!(relaxed.kind, MatchKind::PhraseStart);
        assert!(relaxed.starts_at_beginning);
    }

    #[test]
    fn short_content_prefix_does_not_claim_longer_subtopic() {
        let profile = matching_test_profile();
        let matched = sample_match_quality_for_view(
            &profile,
            &strings(&["gvya", "package", "dependency", "compose", "bot"]),
            &strings(&["gvya", "package"]),
            true,
            false,
            false,
        );
        assert_eq!(matched.kind, MatchKind::PhraseSpan);
        assert!(matched.quality <= 0.74);
    }

    #[test]
    fn short_generic_sample_does_not_win_scattered_across_long_utterance() {
        let mut profile = matching_test_profile();
        profile.generic_singletons.insert("gvya".to_owned());
        profile.generic_singletons.insert("package".to_owned());
        let matched = sample_match_quality_for_view(
            &profile,
            &strings(&[
                "how",
                "does",
                "gvya",
                "decide",
                "which",
                "package",
                "dependencies",
                "composed",
                "bot",
            ]),
            &strings(&["gvya", "package"]),
            false,
            false,
            false,
        );
        assert_eq!(matched.kind, MatchKind::PhraseSpan);
        assert!(matched.quality <= 0.74);
    }

    #[test]
    fn direct_singleton_cue_strength_is_lexical_not_meaning_class_specific() {
        let mut profile = matching_test_profile();
        profile.generic_singletons.insert("hi".to_owned());
        profile.social_vocabulary.insert("hi".to_owned());
        let input = strings(&[
            "i", "am", "here", "to", "say", "hi", "to", "you", "and", "i", "know", "you", "were",
            "waiting", "for", "me",
        ]);
        let sample = strings(&["hi"]);

        let social = sample_match_quality_for_view(&profile, &input, &sample, false, true, false);
        let general = sample_match_quality_for_view(&profile, &input, &sample, false, false, false);
        assert_eq!(social.kind, MatchKind::PhraseSpan);
        assert_eq!(general.kind, MatchKind::PhraseSpan);
        assert!(social.quality >= 0.90);
        assert_eq!(social.quality, general.quality);
        assert!(social.coverage < 1.0);
    }

    #[test]
    fn shorter_authored_sample_has_higher_generic_non_exact_score() {
        let profile = matching_test_profile();
        let input = strings(&[
            "please", "alpha", "beta", "gamma", "delta", "epsilon", "zeta", "for", "me", "now",
        ]);
        let four = sample_match_quality_for_view(
            &profile,
            &input,
            &strings(&["alpha", "beta", "gamma", "delta"]),
            false,
            false,
            false,
        );
        let six = sample_match_quality_for_view(
            &profile,
            &input,
            &strings(&["alpha", "beta", "gamma", "delta", "epsilon", "zeta"]),
            false,
            false,
            false,
        );
        assert_eq!(four.kind, MatchKind::PhraseSpan);
        assert_eq!(six.kind, MatchKind::PhraseSpan);
        assert!(four.quality > six.quality, "four={:?} six={:?}", four, six);
    }

    #[test]
    fn ambiguous_singleton_is_bounded_even_when_owner_is_social() {
        let mut profile = matching_test_profile();
        profile.generic_singletons.insert("right".to_owned());
        let matched = sample_match_quality_for_view(
            &profile,
            &strings(&["right", "triangle", "formula"]),
            &strings(&["right"]),
            false,
            true,
            false,
        );
        assert_eq!(matched.kind, MatchKind::PhraseSpan);
        assert!(matched.quality <= 0.74);
    }

    #[test]
    fn scattered_bound_leaves_longer_samples_alone() {
        let profile = matching_test_profile();
        let matched = sample_match_quality_for_view(
            &profile,
            &strings(&[
                "how", "does", "the", "package", "graph", "reach", "the", "runtime",
            ]),
            &strings(&["package", "graph", "runtime"]),
            false,
            false,
            false,
        );
        assert_ne!(matched.kind, MatchKind::PhraseSpan);
    }

    #[test]
    fn scattered_bound_leaves_short_utterances_alone() {
        let mut profile = matching_test_profile();
        profile.generic_singletons.insert("gvya".to_owned());
        profile.generic_singletons.insert("package".to_owned());
        let matched = sample_match_quality_for_view(
            &profile,
            &strings(&["the", "gvya", "package"]),
            &strings(&["gvya", "package"]),
            false,
            false,
            false,
        );
        assert_ne!(matched.kind, MatchKind::PhraseSpan);
    }

    #[test]
    fn scattered_bound_spares_specialised_short_samples() {
        let profile = matching_test_profile();
        let matched = sample_match_quality_for_view(
            &profile,
            &strings(&[
                "authored", "content", "reads", "env", "or", "network", "requests",
            ]),
            &strings(&["env", "network"]),
            false,
            false,
            false,
        );
        assert_ne!(
            matched.kind,
            MatchKind::PhraseSpan,
            "a short sample of non-generic tokens keeps ordered-subsequence authority"
        );
    }

    #[test]
    fn scattered_bound_protects_task_cues() {
        let mut profile = matching_test_profile();
        profile.task_cues.insert("open".to_owned());
        profile.generic_singletons.insert("open".to_owned());
        profile.generic_singletons.insert("door".to_owned());
        let matched = sample_match_quality_for_view(
            &profile,
            &strings(&[
                "please", "open", "the", "door", "quickly", "inside", "kitchen",
            ]),
            &strings(&["open", "door"]),
            false,
            false,
            false,
        );
        assert_ne!(matched.kind, MatchKind::PhraseSpan);
    }

    #[test]
    fn short_content_prefix_keeps_normal_one_token_continuation() {
        let profile = matching_test_profile();
        let matched = sample_match_quality_for_view(
            &profile,
            &strings(&["open", "door", "now"]),
            &strings(&["open", "door"]),
            true,
            false,
            false,
        );
        assert_eq!(matched.kind, MatchKind::PhraseStart);
    }

    #[test]
    fn task_cue_prefix_keeps_strong_authority_in_long_content_view() {
        let mut profile = matching_test_profile();
        profile.task_cues.insert("open".to_owned());
        let matched = sample_match_quality_for_view(
            &profile,
            &strings(&["open", "door", "quickly", "inside", "kitchen"]),
            &strings(&["open", "door"]),
            true,
            false,
            false,
        );
        assert_eq!(matched.kind, MatchKind::PhraseStart);
    }

    #[test]
    fn glue_heavy_prefix_yields_to_the_longer_subtopic() {
        let mut profile = matching_test_profile();
        profile
            .pure_glue
            .extend(["please".to_owned(), "tell".to_owned()]);
        let matched = sample_match_quality_for_view(
            &profile,
            &strings(&["please", "tell", "matcher", "semantic", "scoring"]),
            &strings(&["please", "tell"]),
            false,
            false,
            false,
        );

        assert_eq!(matched.kind, MatchKind::PhraseSpan);
        assert!(matched.quality <= 0.74);
    }

    #[test]
    fn embedded_multi_content_phrase_strength_is_meaning_class_agnostic() {
        let profile = matching_test_profile();
        let input = strings(&["please", "say", "alpha", "beta", "now"]);
        let sample = strings(&["alpha", "beta"]);
        let general = sample_match_quality(&profile, &input, &sample, false, false);
        let social = sample_match_quality(&profile, &input, &sample, true, false);
        assert_eq!(general.kind, MatchKind::PhraseSpan);
        assert_eq!(social.kind, MatchKind::PhraseSpan);
        assert_eq!(general.quality, social.quality);
        assert!(general.quality >= 0.85);
    }

    #[test]
    fn incomplete_relaxed_order_score_is_completion_fraction_not_automatic_one() {
        let profile = matching_test_profile();
        let relaxed = relaxed_ordered_subsequence_match(
            &profile,
            &strings(&["open", "maintenance", "door"]),
            &strings(&["open", "door"]),
        );
        assert!(!relaxed.complete);
        assert!(relaxed.order_score < 1.0);
    }

    #[test]
    fn gap_tolerant_ordered_match_skips_missing_sample_tokens_without_consuming_input() {
        let profile = matching_test_profile();
        let sample = strings(&["meaning", "class", "social", "general", "classification"]);
        let input = strings(&[
            "why", "is", "meaning", "marked", "social", "versus", "general", "instead",
        ]);

        let legacy = relaxed_ordered_subsequence_match(&profile, &sample, &input);
        assert_eq!(legacy.matched_pairs.len(), 1);

        let bounded = gap_tolerant_ordered_subsequence_match(&profile, &sample, &input);
        assert_eq!(
            bounded
                .matched_pairs
                .iter()
                .map(|pair| pair.sample_index)
                .collect::<Vec<_>>(),
            vec![0, 2, 3]
        );
        assert!(bounded.recall >= 0.50);
    }

    #[test]
    fn gap_tolerant_ordered_match_never_backtracks_before_the_last_success() {
        let profile = matching_test_profile();
        let bounded = gap_tolerant_ordered_subsequence_match(
            &profile,
            &strings(&["alpha", "missing", "beta"]),
            &strings(&["beta", "alpha", "beta"]),
        );

        assert_eq!(bounded.matched_pairs.len(), 2);
        assert_eq!(bounded.matched_pairs[0].input_index, 1);
        assert_eq!(bounded.matched_pairs[1].input_index, 2);
    }

    #[test]
    fn weighted_f1_order_score_counts_first_match_like_the_proven_algorithm() {
        let profile = matching_test_profile();
        let reversed = weighted_token_f1(
            &profile,
            &strings(&["open", "door"]),
            &strings(&["door", "open"]),
        );
        assert_eq!(reversed.order_score, 0.5);
    }

    #[test]
    fn generic_singleton_at_phrase_end_does_not_gain_short_phrase_authority() {
        let profile = matching_test_profile();
        let matched = sample_match_quality(
            &profile,
            &strings(&["then", "what"]),
            &strings(&["what"]),
            false,
            false,
        );
        assert_ne!(matched.kind, MatchKind::PhraseEndShort);
        assert!(matched.quality <= 0.74);
    }

    #[test]
    fn multi_content_phrase_at_short_end_keeps_phrase_end_authority() {
        let profile = matching_test_profile();
        let matched = sample_match_quality(
            &profile,
            &strings(&["please", "open", "door"]),
            &strings(&["open", "door"]),
            false,
            false,
        );
        assert_eq!(matched.kind, MatchKind::PhraseEndShort);
    }

    #[test]
    fn generic_singleton_remains_visible_but_bounded_in_long_task() {
        let profile = matching_test_profile();
        let m = sample_match_quality(
            &profile,
            &strings(&["what", "is", "door", "status"]),
            &strings(&["what"]),
            false,
            false,
        );
        assert_eq!(m.kind, MatchKind::PhraseSpan);
        assert!(m.quality <= 0.74);
    }

    #[test]
    fn multi_token_sample_with_one_content_word_has_no_content_coverage_authority() {
        let profile = matching_test_profile();
        let coverage = content_token_coverage(
            &profile,
            &strings(&["what", "addons"]),
            &strings(&["tell", "me", "addons"]),
        )
        .expect("coverage diagnostic");
        assert!(!coverage.complete);
    }

    #[test]
    fn numeric_helper_preserves_exact_single_token_shape() {
        let profile = matching_test_profile();
        let at_start = numeric_window_sample_match(
            &profile,
            &strings(&["door", "status"]),
            &strings(&["door"]),
        );
        assert_eq!(at_start.kind, MatchKind::Exact);
        assert_eq!(at_start.quality, 1.0);
        assert_eq!(at_start.coverage, 0.5);

        let embedded = numeric_window_sample_match(
            &profile,
            &strings(&["check", "door"]),
            &strings(&["door"]),
        );
        assert_eq!(embedded.kind, MatchKind::PhraseSpan);
        assert_eq!(embedded.quality, 1.0);
        assert_eq!(embedded.coverage, 0.5);
    }

    #[test]
    fn numeric_windows_keep_bounded_frontier_and_signal_cap() {
        let profile = matching_test_profile();
        let input = strings(&[
            "alpha", "beta", "gamma", "delta", "epsilon", "zeta", "eta", "theta", "iota", "kappa",
        ]);
        let sample = strings(&["beta", "gamma", "delta", "epsilon"]);
        let matched = numeric_window_sample_match(&profile, &input, &sample);
        assert_ne!(matched.kind, MatchKind::None);
        assert!(matched.numeric_score > 0.0);
    }

    #[test]
    fn reported_social_phrase_is_suppressed() {
        let profile = matching_test_profile();
        let input = strings(&["he", "said", "thank", "you"]);
        let m = sample_match_quality(
            &profile,
            &input,
            &strings(&["thank", "you"]),
            true,
            profile.has_reporting_context(&input),
        );
        assert_eq!(m.kind, MatchKind::ReportedSpeechPenalized);
        assert!(m.quality <= 0.20);
    }
}
