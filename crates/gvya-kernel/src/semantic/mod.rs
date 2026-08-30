//! Canonical deterministic semantic kernel.
//!
//! This module owns lexical interpretation only: normalization, semantic views, entities,
//! candidate retrieval, matching/ranking, ambiguity and optional resolver proposals.
//! Conversation state/response behavior and capability admission intentionally live elsewhere.

mod catalog;
mod collection;
mod entities;
mod index;
mod matching;
mod normalization;
mod profile;
mod projection;
mod resolver;
mod scoring;
mod structural;
mod trace;
mod unicode_nfc;
mod views;

use std::collections::{BTreeMap, BTreeSet};

use gvya_model::{
    HostReference, Meaning, MeaningId, MissingRequiredValue, ReferenceKind, SlotValue, Trace,
    TraceCode, TraceEvent, TraceId, TraceVisibility, Value, ValueProvenance,
};

use crate::{
    RESOLVER_REFERENCE_CANDIDATES_MAX, ResolverCandidateOrigin, ResolverMeaningCandidate,
    ResolverProposal, ResolverReferenceCandidate, ResolverRequest, ResolverTask, SemanticResolver,
    UtteranceInput,
};

pub use catalog::{
    CatalogError, ElicitationPrompt, LocalizedSample, LocalizedText, MeaningClass, MeaningPattern,
    ReferenceSpec, SEMANTIC_NEGATIVE_SAMPLES_PER_MEANING_MAX,
    SEMANTIC_RETRIEVAL_TERMS_PER_MEANING_MAX, SEMANTIC_SAMPLES_PER_MEANING_MAX,
    SEMANTIC_TEXT_ITEM_MAX_BYTES, SEMANTIC_TEXT_PER_MEANING_MAX_BYTES, SemanticCatalog, SlotKind,
    SlotSpec,
};
pub use collection::{CollectionTurnDecision, MAX_ACTIVE_COLLECTION_VALUES};
pub use entities::{EntityExtraction, EntityKind, EntityStatus, SemanticEntity, extract_entities};
pub use index::{
    CandidateDecision, CandidateRow, SEMANTIC_EXACT_FANOUT_MAX, SemanticIndex,
    SemanticIndexBuildError, TypoCorrection, TypoLiteResult, limited_damerau_distance,
};
pub use matching::{
    ContentCoverage, MatchKind, MatchedPair, NumericSimilarity, RelaxedMatch, SampleMatch, TokenF1,
    content_token_coverage, edit_similarity, likely_stem_match, numeric_window_similarity,
    relaxed_ordered_subsequence_match, sample_match_quality, token_similarity, weighted_token_f1,
};
pub use normalization::{
    language_fallbacks, language_is_compatible, normalize_language_tag, normalize_meta_text,
    normalize_text, ordered_tokens,
};
pub use profile::{
    SemanticProfile, SemanticProfiles, profile_for_authored_language, profile_for_language,
};
#[cfg(test)]
use resolver::RESOLVER_PROPOSAL_MAX_EVIDENCE;
use resolver::{
    BindOutcome, bind_meaning, canonicalize_meaning, resolver_proposal_targets_are_unique,
    resolver_proposal_within_limits, resolver_slot_value_matches_kind, slot_value_matches_kind,
};
pub use resolver::{ResolverReview, ResolverRunError};
pub use scoring::{ScoreBreakdown, ScoredMeaning, compare_scored, score_pattern};
pub use structural::{
    LocalizedStructuralPattern, SEMANTIC_PATTERN_ATOMS_MAX, SEMANTIC_PATTERN_CAPTURES_MAX,
    SEMANTIC_PATTERNS_PER_MEANING_MAX, SEMANTIC_STRUCTURAL_RULES_MAX, SEMANTIC_STRUCTURAL_WORK_MAX,
    StructuralMatchSummary, StructuralMatcherBuildError, StructuralPatternError,
    structural_pattern_set_names, validate_structural_matcher,
};
use trace::{build_structural_trace, build_structural_trace_optional, build_trace, trace_event};
pub use views::build_semantic_views;
use views::push_unique_view;

#[derive(Clone, Debug, PartialEq)]
pub struct SemanticView {
    pub name: String,
    pub text: String,
    pub tokens: Vec<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SemanticViews {
    pub normalized: String,
    pub entities: Vec<SemanticEntity>,
    pub views: Vec<SemanticView>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SemanticInput {
    pub utterance: UtteranceInput,
    /// Explicit semantic fallback languages in priority order. The kernel never assumes `und`.
    pub language_fallbacks: Vec<String>,
    pub reference_candidates: Vec<ResolverReferenceCandidate>,
    /// Explicitly selected context safe to expose to a resolver. The deterministic matcher does
    /// not treat arbitrary host context as hidden semantic evidence.
    pub resolver_context: BTreeMap<String, Value>,
}

impl SemanticInput {
    #[must_use]
    pub fn utterance(text: impl Into<String>) -> Self {
        Self {
            utterance: UtteranceInput {
                text: text.into(),
                language: Some("und".to_string()),
            },
            language_fallbacks: Vec::new(),
            reference_candidates: Vec::new(),
            resolver_context: BTreeMap::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum ResolutionSource {
    StructuralPattern,
    Deterministic,
    ResolverProposal,
}

/// A confidently selected Meaning whose valid values are preserved while required declarations
/// remain unsatisfied.
#[derive(Clone, Debug, PartialEq)]
pub struct PartialMeaning {
    pub meaning: Meaning,
    pub missing_required_values: Vec<MissingRequiredValue>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum SemanticDecision {
    Resolved {
        meaning: Meaning,
        source: ResolutionSource,
    },
    Partial {
        partial: PartialMeaning,
        source: ResolutionSource,
    },
    Ambiguous {
        candidates: Vec<MeaningId>,
        reason_code: String,
    },
    Unresolved {
        reason_code: String,
        best_score: f64,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct SemanticAnalysis {
    /// Language Profile whose authored evidence produced this deterministic frontier.
    /// Conversation commits it only after a resolved or partial Meaning.
    pub language: Option<String>,
    pub views: SemanticViews,
    pub structural_match: Option<StructuralMatchSummary>,
    pub typo_lite: Option<TypoLiteResult>,
    pub candidate_pruning_reason: String,
    pub candidate_pruning_used: bool,
    pub scored: Vec<ScoredMeaning>,
    pub decision: SemanticDecision,
    pub trace: Trace,
}

pub const SEMANTIC_CANDIDATE_LIMIT_MIN: usize = 2;
pub const SEMANTIC_CANDIDATE_LIMIT_MAX: usize = 256;
pub const SEMANTIC_RESOLUTION_THRESHOLD_MIN: f64 = 0.0;
pub const SEMANTIC_RESOLUTION_THRESHOLD_MAX: f64 = 1.0;
pub const SEMANTIC_AMBIGUITY_MARGIN_MIN: f64 = 0.0;
pub const SEMANTIC_AMBIGUITY_MARGIN_MAX: f64 = 1.0;
pub const SEMANTIC_RESOLVER_CONFIDENCE_MIN: f32 = 0.0;
pub const SEMANTIC_RESOLVER_CONFIDENCE_MAX: f32 = 1.0;
pub const SEMANTIC_RESOLVER_CANDIDATE_LIMIT_MIN: usize = 1;
pub const SEMANTIC_RESOLVER_CANDIDATE_LIMIT_MAX: usize = 64;
const SEMANTIC_EXHAUSTIVE_RESCUE_PATTERNS_MAX: usize = 1_024;
const SEMANTIC_EXHAUSTIVE_RESCUE_EVIDENCE_MAX: usize = 16_384;
const SEMANTIC_EXHAUSTIVE_RESCUE_WORK_MAX: usize = 65_536;

#[derive(Clone, Debug, PartialEq)]
pub struct SemanticConfig {
    pub candidate_limit: usize,
    pub resolution_threshold: f64,
    pub ambiguity_margin: f64,
    pub resolver_min_confidence: f32,
    pub resolver_candidate_limit: usize,
}

impl Default for SemanticConfig {
    fn default() -> Self {
        Self {
            candidate_limit: 120,
            resolution_threshold: 0.45,
            ambiguity_margin: 0.04,
            resolver_min_confidence: 0.55,
            resolver_candidate_limit: 8,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SemanticConfigError(pub &'static str);

impl SemanticConfig {
    pub fn validate(&self) -> Result<(), SemanticConfigError> {
        if !(SEMANTIC_CANDIDATE_LIMIT_MIN..=SEMANTIC_CANDIDATE_LIMIT_MAX)
            .contains(&self.candidate_limit)
        {
            return Err(SemanticConfigError(
                "candidate_limit outside canonical range",
            ));
        }
        if !self.resolution_threshold.is_finite()
            || !(SEMANTIC_RESOLUTION_THRESHOLD_MIN..=SEMANTIC_RESOLUTION_THRESHOLD_MAX)
                .contains(&self.resolution_threshold)
        {
            return Err(SemanticConfigError(
                "resolution_threshold outside canonical range",
            ));
        }
        if !self.ambiguity_margin.is_finite()
            || !(SEMANTIC_AMBIGUITY_MARGIN_MIN..=SEMANTIC_AMBIGUITY_MARGIN_MAX)
                .contains(&self.ambiguity_margin)
        {
            return Err(SemanticConfigError(
                "ambiguity_margin outside canonical range",
            ));
        }
        if !self.resolver_min_confidence.is_finite()
            || !(SEMANTIC_RESOLVER_CONFIDENCE_MIN..=SEMANTIC_RESOLVER_CONFIDENCE_MAX)
                .contains(&self.resolver_min_confidence)
        {
            return Err(SemanticConfigError(
                "resolver_min_confidence outside canonical range",
            ));
        }
        if !(SEMANTIC_RESOLVER_CANDIDATE_LIMIT_MIN..=SEMANTIC_RESOLVER_CANDIDATE_LIMIT_MAX)
            .contains(&self.resolver_candidate_limit)
        {
            return Err(SemanticConfigError(
                "resolver_candidate_limit outside canonical range",
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SemanticKernelBuildError {
    Config(SemanticConfigError),
    MissingLanguageProfile(String),
    Structural(StructuralMatcherBuildError),
    Index(SemanticIndexBuildError),
    InvalidCustomEntities(String),
}

#[derive(Clone, Debug, PartialEq)]
pub struct SemanticKernel {
    catalog: SemanticCatalog,
    profiles: SemanticProfiles,
    config: SemanticConfig,
    structural: structural::StructuralMatcher,
    index: SemanticIndex,
}

fn validate_catalog_languages(
    patterns: &[MeaningPattern],
    profiles: &SemanticProfiles,
) -> Result<(), String> {
    for pattern in patterns {
        for language in pattern
            .patterns
            .iter()
            .map(|row| row.language.as_str())
            .chain(pattern.samples.iter().map(|row| row.language.as_str()))
            .chain(
                pattern
                    .negative_samples
                    .iter()
                    .map(|row| row.language.as_str()),
            )
            .chain(
                pattern
                    .retrieval_terms
                    .iter()
                    .map(|row| row.language.as_str()),
            )
            .chain(
                pattern
                    .slots
                    .iter()
                    .flat_map(|slot| slot.elicitation.iter())
                    .map(|row| row.language.as_str()),
            )
            .chain(
                pattern
                    .references
                    .iter()
                    .flat_map(|reference| reference.elicitation.iter())
                    .map(|row| row.language.as_str()),
            )
        {
            let normalized = normalize_language_tag(language);
            if !profiles.contains_key(&normalized) {
                return Err(normalized);
            }
        }
    }
    Ok(())
}

impl SemanticKernel {
    /// The one semantic kernel constructor. Compiler and runtime both build the matcher index
    /// deterministically from the canonical catalog/profiles/config, so the index is never a
    /// serialized artifact section and can never drift from the executable patterns it indexes.
    pub fn new(
        catalog: SemanticCatalog,
        profiles: SemanticProfiles,
        config: SemanticConfig,
    ) -> Result<Self, SemanticKernelBuildError> {
        config
            .validate()
            .map_err(SemanticKernelBuildError::Config)?;
        validate_catalog_languages(catalog.patterns(), &profiles)
            .map_err(SemanticKernelBuildError::MissingLanguageProfile)?;
        validate_custom_entity_profiles(&profiles)?;
        let structural = structural::StructuralMatcher::build(catalog.patterns(), &profiles)
            .map_err(SemanticKernelBuildError::Structural)?;
        let index =
            SemanticIndex::build(&catalog, &profiles).map_err(SemanticKernelBuildError::Index)?;
        Ok(Self {
            catalog,
            profiles,
            config,
            structural,
            index,
        })
    }

    #[must_use]
    pub fn catalog(&self) -> &SemanticCatalog {
        &self.catalog
    }

    #[must_use]
    pub fn profiles(&self) -> &SemanticProfiles {
        &self.profiles
    }

    #[must_use]
    pub fn profile_for_language(
        &self,
        requested: Option<&str>,
        explicit_fallbacks: &[String],
    ) -> Option<&SemanticProfile> {
        profile_for_language(&self.profiles, requested, explicit_fallbacks)
    }

    #[must_use]
    pub fn config(&self) -> &SemanticConfig {
        &self.config
    }

    #[must_use]
    pub fn analyze(&self, input: &SemanticInput) -> SemanticAnalysis {
        self.analyze_filtered(input, None)
    }

    /// Analyze using the same compiler-hydrated full index while restricting resolution to an
    /// explicit set of meanings. No subset catalog or runtime index is constructed.
    #[must_use]
    pub fn analyze_allowed(
        &self,
        input: &SemanticInput,
        allowed: &BTreeSet<MeaningId>,
    ) -> SemanticAnalysis {
        self.analyze_filtered(input, Some(allowed))
    }

    fn analyze_filtered(
        &self,
        input: &SemanticInput,
        allowed: Option<&BTreeSet<MeaningId>>,
    ) -> SemanticAnalysis {
        let neutral_profile = SemanticProfile::empty();
        let profile = self
            .profile_for_language(
                input.utterance.language.as_deref(),
                &input.language_fallbacks,
            )
            .unwrap_or(&neutral_profile);
        let mut views = build_semantic_views(&input.utterance.text, profile, None);
        let permitted =
            |pattern: &MeaningPattern| allowed.is_none_or(|set| set.contains(&pattern.id));
        match self.structural.resolve(
            self.catalog.patterns(),
            profile,
            &views.normalized,
            &views.entities,
            &input.reference_candidates,
            input.utterance.language.as_deref(),
            &input.language_fallbacks,
            permitted,
        ) {
            structural::StructuralDecision::Resolved { meaning, summary } => {
                let decision = SemanticDecision::Resolved {
                    meaning,
                    source: ResolutionSource::StructuralPattern,
                };
                let trace = build_structural_trace(&views, &summary, &decision);
                return SemanticAnalysis {
                    language: input.utterance.language.clone(),
                    views,
                    structural_match: Some(summary),
                    typo_lite: None,
                    candidate_pruning_reason: "structural_pattern_authority".to_string(),
                    candidate_pruning_used: false,
                    scored: Vec::new(),
                    decision,
                    trace,
                };
            }
            structural::StructuralDecision::Ambiguous {
                candidates,
                reason_code,
                summary,
            } => {
                let decision = SemanticDecision::Ambiguous {
                    candidates,
                    reason_code,
                };
                let trace = build_structural_trace_optional(&views, summary.as_ref(), &decision);
                return SemanticAnalysis {
                    language: input.utterance.language.clone(),
                    views,
                    structural_match: summary,
                    typo_lite: None,
                    candidate_pruning_reason: "structural_pattern_authority".to_string(),
                    candidate_pruning_used: false,
                    scored: Vec::new(),
                    decision,
                    trace,
                };
            }
            structural::StructuralDecision::Partial { partial, summary } => {
                let decision = SemanticDecision::Partial {
                    partial,
                    source: ResolutionSource::StructuralPattern,
                };
                let trace = build_structural_trace(&views, &summary, &decision);
                return SemanticAnalysis {
                    language: input.utterance.language.clone(),
                    views,
                    structural_match: Some(summary),
                    typo_lite: None,
                    candidate_pruning_reason: "structural_pattern_authority".to_string(),
                    candidate_pruning_used: false,
                    scored: Vec::new(),
                    decision,
                    trace,
                };
            }
            structural::StructuralDecision::BudgetExceeded => {
                let decision = SemanticDecision::Unresolved {
                    reason_code: "structural_match_budget_exceeded".to_string(),
                    best_score: 0.0,
                };
                let trace = build_structural_trace_optional(&views, None, &decision);
                return SemanticAnalysis {
                    language: input.utterance.language.clone(),
                    views,
                    structural_match: None,
                    typo_lite: None,
                    candidate_pruning_reason: "structural_match_budget_exceeded".to_string(),
                    candidate_pruning_used: false,
                    scored: Vec::new(),
                    decision,
                    trace,
                };
            }
            structural::StructuralDecision::Invalid {
                reason_code,
                summary,
            } => {
                let decision = SemanticDecision::Unresolved {
                    reason_code,
                    best_score: 0.0,
                };
                let trace = build_structural_trace(&views, &summary, &decision);
                return SemanticAnalysis {
                    language: input.utterance.language.clone(),
                    views,
                    structural_match: Some(summary),
                    typo_lite: None,
                    candidate_pruning_reason: "structural_pattern_invalid_binding".to_string(),
                    candidate_pruning_used: false,
                    scored: Vec::new(),
                    decision,
                    trace,
                };
            }
            structural::StructuralDecision::NoMatch => {}
        }
        let candidate_decision = self.index.candidate_decision(
            &views.normalized,
            input.utterance.language.as_deref(),
            &input.language_fallbacks,
            profile,
            self.config.candidate_limit,
        );
        if let Some(typo) = &candidate_decision.typo_lite {
            let typo_text = typo.tokens.join(" ");
            push_unique_view(
                &mut views.views,
                "typo_text",
                typo_text.clone(),
                typo.tokens.clone(),
            );
            let typo_content = typo.content_tokens.join(" ");
            if !typo_content.is_empty() {
                push_unique_view(
                    &mut views.views,
                    "typo_content",
                    typo_content,
                    typo.content_tokens.clone(),
                );
            }
        }

        let mut scored = Vec::new();
        for row in &candidate_decision.rows {
            if let Some(pattern) = self
                .catalog
                .patterns()
                .get(row.pattern_index)
                .filter(|pattern| permitted(pattern))
            {
                scored.push(score_pattern(
                    profile,
                    &views.views,
                    pattern,
                    row.pattern_index,
                    row.rank_milli,
                    input.utterance.language.as_deref(),
                    &input.language_fallbacks,
                ));
            }
        }
        // Ordinary global semantic work is bounded by candidate_limit. When conversation scope
        // has already reduced authority to a small explicit allow-list, complete that bounded
        // scope even when global retrieval returned only some of its rows. A separate bounded
        // exhaustive *strong-sample* rescue below may inspect a small/medium catalog, but it never
        // appends weak or retrieval-only rows.
        let mut scoped_fallback = false;
        if let Some(allowed) = allowed.filter(|set| set.len() <= self.config.candidate_limit) {
            let mut already_scored: BTreeSet<usize> =
                scored.iter().map(|row| row.pattern_index).collect();
            for meaning_id in allowed {
                if let Some((pattern_index, pattern)) = self.catalog.get_with_index(meaning_id) {
                    if already_scored.insert(pattern_index) {
                        scored.push(score_pattern(
                            profile,
                            &views.views,
                            pattern,
                            pattern_index,
                            0,
                            input.utterance.language.as_deref(),
                            &input.language_fallbacks,
                        ));
                        scoped_fallback = true;
                    }
                }
            }
        }

        // Retrieval is an optimization, never final authority over authored positive samples.
        // If bounded retrieval did not surface any decision-grade sample evidence, a small or
        // medium catalog gets one complete rescue pass through the *same* canonical scorer. This
        // specifically protects a good sample from being hidden behind candidate_limit without
        // reintroducing AIMLBot-style unbounded brute force. Weak/retrieval-only matches discovered
        // by the scan are discarded and therefore cannot manufacture a new false-positive frontier.
        let already_scored: BTreeSet<usize> = scored.iter().map(|row| row.pattern_index).collect();
        let has_unscored_permitted = self
            .catalog
            .patterns()
            .iter()
            .enumerate()
            .any(|(index, pattern)| permitted(pattern) && !already_scored.contains(&index));
        let mut exhaustive_sample_scan = false;
        let mut exhaustive_sample_rescue = false;
        if has_unscored_permitted
            && !scored.iter().any(Self::has_decision_grade_sample_evidence)
            && self.exhaustive_sample_rescue_within_budget(&views, &permitted)
        {
            exhaustive_sample_scan = true;
            for (pattern_index, pattern) in self.catalog.patterns().iter().enumerate() {
                if already_scored.contains(&pattern_index) || !permitted(pattern) {
                    continue;
                }
                let candidate = score_pattern(
                    profile,
                    &views.views,
                    pattern,
                    pattern_index,
                    0,
                    input.utterance.language.as_deref(),
                    &input.language_fallbacks,
                );
                if Self::has_decision_grade_sample_evidence(&candidate) {
                    scored.push(candidate);
                    exhaustive_sample_rescue = true;
                }
            }
        }

        self.apply_wrapper_competition(&mut scored);
        self.sort_scores(&mut scored);

        let decision = self.deterministic_decision(&scored, &views, input);
        let trace = build_trace(
            &views,
            &candidate_decision,
            scoped_fallback,
            exhaustive_sample_scan,
            exhaustive_sample_rescue,
            &scored,
            &decision,
        );
        SemanticAnalysis {
            language: input.utterance.language.clone(),
            views,
            structural_match: None,
            typo_lite: candidate_decision.typo_lite.clone(),
            candidate_pruning_reason: if exhaustive_sample_rescue {
                "exhaustive_sample_rescue".to_string()
            } else if scoped_fallback {
                "bounded_allowed_scope_complete".to_string()
            } else {
                candidate_decision.reason.to_string()
            },
            candidate_pruning_used: true,
            scored,
            decision,
            trace,
        }
    }

    pub fn analyze_with_resolver<R: SemanticResolver + ?Sized>(
        &self,
        input: &SemanticInput,
        resolver: &R,
    ) -> Result<SemanticAnalysis, ResolverRunError<R::Error>> {
        let analysis = self.analyze(input);
        self.apply_resolver(input, analysis, resolver)
    }

    pub fn analyze_allowed_with_resolver<R: SemanticResolver + ?Sized>(
        &self,
        input: &SemanticInput,
        allowed: &BTreeSet<MeaningId>,
        resolver: &R,
    ) -> Result<SemanticAnalysis, ResolverRunError<R::Error>> {
        let analysis = self.analyze_filtered(input, Some(allowed));
        self.apply_resolver(input, analysis, resolver)
    }

    /// Re-bind a previously scored deterministic candidate without rerunning retrieval/scoring.
    /// This is used by the Conversation Kernel to preserve the reference behavior of trying a
    /// lower-ranked semantically valid candidate when a higher-ranked behavior has no eligible
    /// response. It never promotes rows below the semantic resolution floor.
    #[must_use]
    pub fn bind_scored_candidate(
        &self,
        input: &SemanticInput,
        row: &ScoredMeaning,
    ) -> Option<Meaning> {
        self.bind_scored_candidate_at_floor(input, row, self.config.resolution_threshold)
    }

    /// Re-bind a deterministic scored row for Conversation repair using an explicit lower floor.
    /// Repair may relax score authority, never the Meaning's structural validity.
    #[must_use]
    pub(crate) fn bind_scored_repair_candidate(
        &self,
        input: &SemanticInput,
        row: &ScoredMeaning,
        repair_floor: f64,
    ) -> Option<Meaning> {
        if !repair_floor.is_finite() || !(0.0..=1.0).contains(&repair_floor) {
            return None;
        }
        self.bind_scored_candidate_at_floor(input, row, repair_floor)
    }

    fn bind_scored_candidate_at_floor(
        &self,
        input: &SemanticInput,
        row: &ScoredMeaning,
        floor: f64,
    ) -> Option<Meaning> {
        if row.score < floor {
            return None;
        }
        let profile = self.profile_for_language(
            input.utterance.language.as_deref(),
            &input.language_fallbacks,
        )?;
        let pattern = self.catalog.patterns().get(row.pattern_index)?;
        let views = build_semantic_views(&input.utterance.text, profile, None);
        match bind_meaning(
            pattern,
            &views.entities,
            &views.normalized,
            &input.reference_candidates,
            profile,
        ) {
            BindOutcome::Resolved(meaning) => Some(meaning),
            BindOutcome::Partial(_) | BindOutcome::Ambiguous(_) | BindOutcome::Invalid(_) => None,
        }
    }

    fn apply_resolver<R: SemanticResolver + ?Sized>(
        &self,
        input: &SemanticInput,
        mut analysis: SemanticAnalysis,
        resolver: &R,
    ) -> Result<SemanticAnalysis, ResolverRunError<R::Error>> {
        if matches!(
            analysis.decision,
            SemanticDecision::Resolved { .. } | SemanticDecision::Partial { .. }
        ) || analysis.structural_match.is_some()
        {
            return Ok(analysis);
        }
        let request = self.resolver_request(input, &analysis);
        let proposal = resolver
            .propose(&request)
            .map_err(ResolverRunError::Resolver)?;
        let review = self.review_resolver_proposal(input, &analysis, &request, proposal);
        let mut details = BTreeMap::new();
        details.insert("accepted".to_string(), Value::Bool(review.accepted));
        details.insert(
            "reason".to_string(),
            Value::String(review.reason_code.clone()),
        );
        analysis.trace.events.push(trace_event(
            "semantic.resolver.review",
            "semantic",
            "Reviewed untrusted semantic resolver proposal",
            details,
        ));
        if let Some(meaning) = review.meaning {
            analysis.decision = SemanticDecision::Resolved {
                meaning,
                source: ResolutionSource::ResolverProposal,
            };
        } else if let Some(partial) = review.partial {
            analysis.decision = SemanticDecision::Partial {
                partial,
                source: ResolutionSource::ResolverProposal,
            };
        }
        Ok(analysis)
    }

    /// Build the bounded resolver-safe projection for an ordinary `ResolveMeaning` turn.
    ///
    /// Candidates come from the deterministic scored rows first, then from the separate broader
    /// high-recall resolver retrieval stage. Both are bounded by `resolver_candidate_limit`, and
    /// the resulting candidate list is the complete boundary the firewall later enforces.
    #[must_use]
    pub fn resolver_request(
        &self,
        input: &SemanticInput,
        analysis: &SemanticAnalysis,
    ) -> ResolverRequest {
        let neutral_profile = SemanticProfile::empty();
        let selected = self.profile_for_language(
            input.utterance.language.as_deref(),
            &input.language_fallbacks,
        );
        let profile = selected.unwrap_or(&neutral_profile);
        let limit = self
            .config
            .resolver_candidate_limit
            .min(crate::RESOLVER_REQUEST_CANDIDATES_MAX);
        let mut candidates: Vec<ResolverMeaningCandidate> = Vec::new();
        for row in &analysis.scored {
            if candidates.len() >= limit {
                break;
            }
            let Some(pattern) = self.catalog.patterns().get(row.pattern_index) else {
                continue;
            };
            if projection::candidate_contains(&candidates, &pattern.id) {
                continue;
            }
            let mut candidate = projection::meaning_candidate(
                pattern,
                ResolverCandidateOrigin::DeterministicMatch,
                None,
                profile,
                input.utterance.language.as_deref(),
                &input.language_fallbacks,
            );
            candidate.evidence = Some(projection::candidate_evidence(
                row,
                &candidate.hints,
                &analysis.views.normalized,
                profile,
                self.config.resolution_threshold,
            ));
            candidates.push(candidate);
        }
        if candidates.len() < limit {
            if let Some(selected) = selected {
                let weak = self.index.resolver_candidate_decision(
                    &analysis.views.normalized,
                    input.utterance.language.as_deref(),
                    &input.language_fallbacks,
                    selected,
                    limit,
                );
                for row in weak.rows {
                    if candidates.len() >= limit {
                        break;
                    }
                    let Some(pattern) = self.catalog.patterns().get(row.pattern_index) else {
                        continue;
                    };
                    if projection::candidate_contains(&candidates, &pattern.id) {
                        continue;
                    }
                    let mut candidate = projection::meaning_candidate(
                        pattern,
                        ResolverCandidateOrigin::ResolverRecall,
                        None,
                        profile,
                        input.utterance.language.as_deref(),
                        &input.language_fallbacks,
                    );
                    candidate.evidence = Some(projection::recall_evidence(
                        row.rank_milli,
                        &candidate.hints,
                        &analysis.views.normalized,
                        profile,
                    ));
                    candidates.push(candidate);
                }
            }
        }
        ResolverRequest {
            task: ResolverTask::ResolveMeaning,
            utterance: input.utterance.clone(),
            language_fallbacks: language_fallbacks(
                input.utterance.language.as_deref(),
                &input.language_fallbacks,
            ),
            candidates,
            collection: None,
            reference_candidates: self.exposed_reference_candidates(input),
            exposed_context: exposed_resolver_context(&input.resolver_context),
        }
    }

    /// The concrete host reference boundary for one resolver turn.
    ///
    /// A host may legitimately attach far more visible references than a small structured resolver
    /// can reason about, so the projection is bounded. Anything not projected here is not exposed
    /// and can never become authority.
    pub(super) fn exposed_reference_candidates(
        &self,
        input: &SemanticInput,
    ) -> Vec<ResolverReferenceCandidate> {
        input
            .reference_candidates
            .iter()
            .take(RESOLVER_REFERENCE_CANDIDATES_MAX)
            .cloned()
            .collect()
    }

    #[must_use]
    pub fn review_resolver_proposal(
        &self,
        input: &SemanticInput,
        _analysis: &SemanticAnalysis,
        request: &ResolverRequest,
        proposal: ResolverProposal,
    ) -> ResolverReview {
        if !resolver_proposal_within_limits(&proposal) {
            return ResolverReview::rejected("resolver_proposal_limit_exceeded");
        }
        if !resolver_proposal_targets_are_unique(&proposal) {
            return ResolverReview::rejected("resolver_duplicate_target");
        }
        let Some(meaning_id) = proposal.meaning.clone() else {
            return ResolverReview::rejected("resolver_missing_meaning");
        };
        if !request.permits_meaning(&meaning_id) {
            return ResolverReview::rejected("resolver_meaning_outside_candidate_boundary");
        }
        let Some(pattern) = self.catalog.get(&meaning_id) else {
            return ResolverReview::rejected("resolver_unknown_meaning");
        };
        let Some(profile) = self.profile_for_language(
            input.utterance.language.as_deref(),
            &input.language_fallbacks,
        ) else {
            return ResolverReview::rejected("resolver_missing_language_profile");
        };
        let Some(confidence) = proposal.confidence else {
            return ResolverReview::rejected("resolver_missing_confidence");
        };
        if !confidence.is_finite()
            || !(SEMANTIC_RESOLVER_CONFIDENCE_MIN..=SEMANTIC_RESOLVER_CONFIDENCE_MAX)
                .contains(&confidence)
        {
            return ResolverReview::rejected("resolver_invalid_confidence");
        }
        if confidence < self.config.resolver_min_confidence {
            return ResolverReview::rejected("resolver_low_confidence");
        }
        // Reference authority is exactly what this ResolverRequest exposed. The raw semantic
        // input may legitimately carry more host references than the bounded projection showed,
        // and a value the resolver never saw must never become authority.
        let allowed_refs: Vec<_> = request
            .reference_candidates
            .iter()
            .map(|candidate| candidate.reference.clone())
            .collect();
        if proposal
            .references
            .iter()
            .any(|reference| !allowed_refs.contains(reference))
        {
            return ResolverReview::rejected("resolver_unknown_reference");
        }
        if proposal.references.iter().any(|reference| {
            !pattern
                .references
                .iter()
                .any(|spec| spec.kind == reference.kind)
        }) {
            return ResolverReview::rejected("resolver_undeclared_reference");
        }
        let mut slots = Vec::new();
        for slot in proposal.slots {
            let Some(spec) = pattern.slots.iter().find(|spec| spec.name == slot.name) else {
                return ResolverReview::rejected("resolver_unknown_slot");
            };
            if !resolver_slot_value_matches_kind(&slot.value, &spec.kind, profile) {
                return ResolverReview::rejected("resolver_slot_type_mismatch");
            }
            if let SlotKind::Reference(reference_kind) = &spec.kind {
                let Value::String(reference_id) = &slot.value else {
                    return ResolverReview::rejected("resolver_slot_type_mismatch");
                };
                let exposed = request.reference_candidates.iter().any(|candidate| {
                    candidate.reference.kind == *reference_kind
                        && candidate.reference.id.as_str() == reference_id
                });
                if !exposed {
                    return ResolverReview::rejected("resolver_unknown_reference_slot");
                }
            }
            slots.push(SlotValue {
                name: slot.name,
                value: slot.value,
                provenance: ValueProvenance::NeuralProposal,
            });
        }
        let references = proposal.references;
        let mut missing_required_values = pattern
            .slots
            .iter()
            .filter(|spec| spec.required && !slots.iter().any(|slot| slot.name == spec.name))
            .map(|spec| MissingRequiredValue::Slot {
                name: spec.name.clone(),
            })
            .collect::<Vec<_>>();
        missing_required_values.extend(
            pattern
                .references
                .iter()
                .filter(|spec| {
                    spec.required
                        && !references
                            .iter()
                            .any(|reference| reference.kind == spec.kind)
                })
                .map(|spec| MissingRequiredValue::Reference {
                    kind: spec.kind.clone(),
                }),
        );
        let mut meaning = Meaning {
            id: meaning_id,
            slots,
            references,
        };
        canonicalize_meaning(&mut meaning);
        if !missing_required_values.is_empty() {
            return ResolverReview {
                accepted: true,
                reason_code: "resolver_partial_semantic_proposal_validated".to_string(),
                meaning: None,
                partial: Some(PartialMeaning {
                    meaning,
                    missing_required_values,
                }),
            };
        }
        ResolverReview {
            accepted: true,
            reason_code: "resolver_semantic_proposal_validated".to_string(),
            meaning: Some(meaning),
            partial: None,
        }
    }

    fn deterministic_decision(
        &self,
        scored: &[ScoredMeaning],
        views: &SemanticViews,
        input: &SemanticInput,
    ) -> SemanticDecision {
        let Some(best) = scored.first() else {
            return SemanticDecision::Unresolved {
                reason_code: "no_candidates".into(),
                best_score: 0.0,
            };
        };
        let retrieval_rescued = best.score < self.config.resolution_threshold
            && self.retrieval_dominance_rescue(scored, self.config.resolution_threshold);
        if best.score < self.config.resolution_threshold && !retrieval_rescued {
            return SemanticDecision::Unresolved {
                reason_code: "below_resolution_threshold".into(),
                best_score: best.score,
            };
        }
        if let Some(second) = scored.get(1) {
            let same_tier = best.breakdown.evidence_tier == second.breakdown.evidence_tier;
            if !retrieval_rescued
                && same_tier
                && (best.score - second.score).abs() <= self.config.ambiguity_margin
                && !Self::evidence_consensus_breaks_ambiguity(best, second)
                && !Self::exceptional_retrieval_breaks_ambiguity(scored)
                && !Self::retrieval_authority_breaks_near_tie(scored)
            {
                return SemanticDecision::Ambiguous {
                    candidates: [best, second]
                        .into_iter()
                        .filter_map(|row| {
                            self.catalog
                                .patterns()
                                .get(row.pattern_index)
                                .map(|pattern| pattern.id.clone())
                        })
                        .collect(),
                    reason_code: "top_candidates_too_close".into(),
                };
            }
        }
        let Some(pattern) = self.catalog.patterns().get(best.pattern_index) else {
            return SemanticDecision::Unresolved {
                reason_code: "internal_candidate_missing".into(),
                best_score: best.score,
            };
        };
        match bind_meaning(
            pattern,
            &views.entities,
            &views.normalized,
            &input.reference_candidates,
            self.profile_for_language(
                input.utterance.language.as_deref(),
                &input.language_fallbacks,
            )
            .expect("analysis selected an executable language profile"),
        ) {
            BindOutcome::Resolved(meaning) => SemanticDecision::Resolved {
                meaning,
                source: ResolutionSource::Deterministic,
            },
            BindOutcome::Partial(partial) => SemanticDecision::Partial {
                partial,
                source: ResolutionSource::Deterministic,
            },
            BindOutcome::Ambiguous(reason) => SemanticDecision::Ambiguous {
                candidates: vec![pattern.id.clone()],
                reason_code: reason,
            },
            BindOutcome::Invalid(reason) => SemanticDecision::Unresolved {
                reason_code: reason,
                best_score: best.score,
            },
        }
    }

    /// A close raw score is not an ambiguity when every independent authored-evidence axis
    /// agrees on the same winner. This is deliberately stricter than ordinary score ordering:
    /// score, evidence strength, and retrieval authority must all lead by non-trivial margins.
    fn evidence_consensus_breaks_ambiguity(best: &ScoredMeaning, second: &ScoredMeaning) -> bool {
        if best.breakdown.rejected_reason.is_some() || best.score <= second.score {
            return false;
        }
        let score_leads = best.score - second.score >= 0.015;
        let strength_leads =
            best.breakdown.evidence_strength - second.breakdown.evidence_strength >= 0.015;
        let rank_leads = best.retrieval_rank_milli >= second.retrieval_rank_milli + 20_000
            && best.retrieval_rank_milli.saturating_mul(100)
                >= second.retrieval_rank_milli.saturating_mul(105);
        score_leads && strength_leads && rank_leads
    }

    /// Breaks a same-tier ambiguity only when authored retrieval authority is exceptional and
    /// independently dominant over both the semantic runner-up and every other retrieved row.
    /// This covers saturated evidence-strength ties without turning retrieval into a general
    /// score override: weak, negative, rejected, or merely close retrieval evidence cannot use it.
    fn exceptional_retrieval_breaks_ambiguity(scored: &[ScoredMeaning]) -> bool {
        let Some(best) = scored.first() else {
            return false;
        };
        let Some(second) = scored.get(1) else {
            return false;
        };
        if best.breakdown.evidence_tier != second.breakdown.evidence_tier
            || best.breakdown.evidence_tier > 3
            || best.breakdown.rejected_reason.is_some()
            || best.breakdown.negative_hard_block
            || best.breakdown.negative_penalty > 0.0
            || best.score <= second.score
            || best.breakdown.evidence_strength < 0.65
            || best.retrieval_rank_milli < 600_000
        {
            return false;
        }

        let best_rank = best.retrieval_rank_milli;
        let second_rank = second.retrieval_rank_milli;
        if best_rank < second_rank.saturating_mul(3) {
            return false;
        }

        let strongest_competitor = scored
            .iter()
            .skip(1)
            .map(|row| row.retrieval_rank_milli)
            .max()
            .unwrap_or(0);
        best_rank >= strongest_competitor.saturating_add(150_000)
            && best_rank.saturating_mul(10) >= strongest_competitor.saturating_mul(13)
    }

    /// Confirms a same-tier near-tie that the near-tie regrouping already decided on authored
    /// retrieval authority.
    ///
    /// `sort_scores` promotes a row inside a fixed near-tie band when authored retrieval leads.
    /// The ambiguity check then ignores that decision and refuses purely because the raw scores
    /// sit inside the margin, so a Meaning whose authored retrieval metadata clearly and
    /// materially singles it out can never resolve. This restores that ordering decision without
    /// widening the ambiguity margin or lowering any threshold: it stays inside one evidence
    /// tier, demands explicit authored retrieval metadata rather than a rare index posting, and
    /// refuses whenever retrieval is weak, contested by any other retrieved row, or contradicted
    /// by authored negative evidence.
    fn retrieval_authority_breaks_near_tie(scored: &[ScoredMeaning]) -> bool {
        /// Below this the leader is not materially retrieved at all.
        const MIN_LEADER_RANK: u64 = 280_000;
        /// A lead smaller than this is a close retrieval competitor, not discrimination.
        const MIN_RANK_GAP: u64 = 75_000;
        /// Explicit authored retrieval metadata, not a rare positive-sample token.
        const MIN_RESCUE: f64 = 0.50;
        /// The leader may trail on raw score only inside the same near-tie band.
        const MAX_SCORE_DEFICIT: f64 = 0.025;

        let Some(best) = scored.first() else {
            return false;
        };
        let Some(second) = scored.get(1) else {
            return false;
        };
        if best.breakdown.evidence_tier != second.breakdown.evidence_tier
            || best.breakdown.evidence_tier > 3
            || best.breakdown.rejected_reason.is_some()
            || second.breakdown.rejected_reason.is_some()
            || best.breakdown.negative_hard_block
            || best.breakdown.negative_penalty > 0.0
        {
            return false;
        }
        if best.breakdown.retrieval_rescue < MIN_RESCUE
            || second.score - best.score > MAX_SCORE_DEFICIT
        {
            return false;
        }
        let best_rank = best.retrieval_rank_milli;
        if best_rank < MIN_LEADER_RANK {
            return false;
        }
        let strongest_other = scored
            .iter()
            .skip(1)
            .map(|row| row.retrieval_rank_milli)
            .max()
            .unwrap_or(0);
        best_rank >= strongest_other.saturating_add(MIN_RANK_GAP)
            && best_rank.saturating_mul(10) >= strongest_other.saturating_mul(13)
    }

    /// Allows a narrowly-bounded lexical rescue below the ordinary semantic threshold.
    ///
    /// Candidate retrieval is authored evidence too: exact/content/meta postings are weighted by
    /// deterministic IDF before the more permissive sample scorer runs. When that evidence clearly
    /// singles out the same top row, requiring the ordinary score floor can turn a correct lexical
    /// winner into fallback merely because the user's sentence contains natural filler. Rescue is
    /// deliberately unavailable for weak evidence, close retrieval competitors, vetoed rows, or
    /// candidates far below the configured floor.
    fn retrieval_dominance_rescue(
        &self,
        scored: &[ScoredMeaning],
        resolution_threshold: f64,
    ) -> bool {
        let Some(best) = scored.first() else {
            return false;
        };
        let Some(second) = scored.get(1) else {
            return false;
        };
        let Some(pattern) = self.catalog.patterns().get(best.pattern_index) else {
            return false;
        };
        if pattern.class == MeaningClass::Social
            || best.breakdown.rejected_reason.is_some()
            || best.breakdown.negative_hard_block
            || best.breakdown.negative_penalty > 0.0
            || best.breakdown.evidence_tier > 4
        {
            return false;
        }
        let best_rank = best.retrieval_rank_milli;
        if best_rank < 300_000 {
            return false;
        }
        let second_rank = second.retrieval_rank_milli;
        if best_rank <= second_rank {
            return false;
        }
        let gap = best_rank - second_rank;
        let ordinary_dominance =
            gap >= 50_000 && best_rank.saturating_mul(10) >= second_rank.saturating_mul(11);
        if !ordinary_dominance {
            return false;
        }
        let exceptional_dominance =
            best_rank >= 400_000 && gap >= 150_000 && best_rank >= second_rank.saturating_mul(2);
        // The index rank may be high because one generic positive-sample token is rare in the
        // catalog. That is useful for bounded candidate retrieval, but it is not enough authority
        // to bypass the resolution floor. Only explicit authored retrieval metadata may do that.
        let evidence_is_sufficient = if exceptional_dominance {
            best.breakdown.retrieval_rescue >= 0.36
        } else {
            best.breakdown.retrieval_rescue >= 0.50
        };
        if !evidence_is_sufficient {
            return false;
        }
        // The exceptional path is intentionally only ~2.5 points deeper than the ordinary rescue
        // and still requires a >2x retrieval winner. This reaches strongly indexed, filler-heavy
        // paraphrases without turning high-fanout generic postings into threshold bypasses.
        let allowed_below_floor = if exceptional_dominance { 0.145 } else { 0.12 };
        resolution_threshold - best.score <= allowed_below_floor + f64::EPSILON * 8.0
    }

    fn promote_pareto_evidence(scored: &mut [ScoredMeaning]) {
        let mut start = 0usize;
        while start < scored.len() {
            let anchor = &scored[start];
            let anchor_tier = anchor.breakdown.evidence_tier;
            let anchor_score = anchor.score;
            let anchor_strength = anchor.breakdown.evidence_strength;
            let anchor_rank = anchor.retrieval_rank_milli;
            let mut promote = None;
            let mut index = start + 1;
            while index < scored.len()
                && scored[index].breakdown.evidence_tier == anchor_tier
                && anchor_score - scored[index].score <= 0.03
            {
                let candidate = &scored[index];
                let strength_dominates =
                    candidate.breakdown.evidence_strength >= anchor_strength + 0.12;
                let rank_dominates = candidate.retrieval_rank_milli >= anchor_rank + 150_000
                    && candidate.retrieval_rank_milli.saturating_mul(2)
                        >= anchor_rank.saturating_mul(3);
                if candidate.breakdown.rejected_reason.is_none()
                    && strength_dominates
                    && rank_dominates
                {
                    promote = Some(index);
                    break;
                }
                index += 1;
            }
            if let Some(index) = promote {
                scored.swap(start, index);
            }
            start += 1;
        }
    }

    fn sort_scores(&self, scored: &mut [ScoredMeaning]) {
        scored.sort_by(|left, right| {
            let left_id = self
                .catalog
                .patterns()
                .get(left.pattern_index)
                .map_or("", |pattern| pattern.id.as_str());
            let right_id = self
                .catalog
                .patterns()
                .get(right.pattern_index)
                .map_or("", |pattern| pattern.id.as_str());
            compare_scored(left, right, left_id, right_id)
        });

        // A lower raw score may still carry decisively stronger independent evidence. Promote only
        // inside a narrow fixed score band and only when both evidence strength and retrieval
        // authority dominate by large margins. This is intentionally stricter than ordinary
        // near-tie priority ordering and never lowers the resolution threshold.
        Self::promote_pareto_evidence(scored);

        // Apply authored priority only inside fixed near-tie groups. Group membership is measured
        // against the strongest semantic row before mutation, so it cannot form epsilon chains or
        // violate the total-order contract required by Rust sorting.
        let mut start = 0usize;
        while start < scored.len() {
            let anchor_tier = scored[start].breakdown.evidence_tier;
            let anchor_score = scored[start].score;
            let anchor_strength = scored[start].breakdown.evidence_strength;
            let mut end = start + 1;
            while end < scored.len()
                && scored[end].breakdown.evidence_tier == anchor_tier
                && (anchor_score - scored[end].score).abs() <= 0.03
                && (anchor_strength - scored[end].breakdown.evidence_strength).abs() <= 0.08
            {
                end += 1;
            }
            scored[start..end].sort_by(|left, right| {
                let left_id = self
                    .catalog
                    .patterns()
                    .get(left.pattern_index)
                    .map_or("", |pattern| pattern.id.as_str());
                let right_id = self
                    .catalog
                    .patterns()
                    .get(right.pattern_index)
                    .map_or("", |pattern| pattern.id.as_str());
                right
                    .priority
                    .cmp(&left.priority)
                    .then_with(|| right.retrieval_rank_milli.cmp(&left.retrieval_rank_milli))
                    .then_with(|| {
                        right
                            .breakdown
                            .evidence_strength
                            .total_cmp(&left.breakdown.evidence_strength)
                    })
                    .then_with(|| right.score.total_cmp(&left.score))
                    .then_with(|| left_id.cmp(right_id))
            });
            start = end;
        }
    }

    /// Social and clarification phrases are conversational wrappers, not hidden task detectors.
    /// Preserve them when they are the best evidence on the turn, but reject a non-exact wrapper
    /// when a General Meaning independently reaches the ordinary resolution floor. This is
    /// language-neutral and relies only on authored classes plus competing semantic evidence.
    fn apply_wrapper_competition(&self, scored: &mut [ScoredMeaning]) {
        let has_general = scored.iter().any(|row| {
            row.score >= self.config.resolution_threshold
                && row.breakdown.evidence_tier <= 3
                && row.breakdown.rejected_reason.is_none()
                && self
                    .catalog
                    .patterns()
                    .get(row.pattern_index)
                    .is_some_and(|pattern| pattern.class == MeaningClass::General)
        });
        if !has_general {
            return;
        }
        for row in scored {
            let Some(pattern) = self.catalog.patterns().get(row.pattern_index) else {
                continue;
            };
            if !matches!(
                pattern.class,
                MeaningClass::Social | MeaningClass::Clarification
            ) || row.breakdown.match_kind == MatchKind::Exact
                || row.breakdown.match_coverage >= 1.0
                || row.breakdown.rejected_reason.is_some()
            {
                continue;
            }
            row.score = 0.0;
            row.breakdown.rejected_reason = Some(match pattern.class {
                MeaningClass::Social => "social_wrapper_competed_by_general_meaning",
                MeaningClass::Clarification => "clarification_wrapper_competed_by_general_meaning",
                MeaningClass::General => unreachable!(),
            });
        }
    }

    fn has_decision_grade_sample_evidence(row: &ScoredMeaning) -> bool {
        if row.score <= 0.0
            || row.breakdown.rejected_reason.is_some()
            || row.breakdown.negative_penalty > 0.0
        {
            return false;
        }
        match row.breakdown.match_kind {
            MatchKind::Exact
            | MatchKind::PhraseStart
            | MatchKind::PhraseEndShort
            | MatchKind::RelaxedSubsequence
            | MatchKind::ContentCoverage => true,
            MatchKind::PhraseSpan => {
                row.breakdown.numeric_score >= 0.78 && row.breakdown.coverage_score >= 1.0
            }
            MatchKind::PhraseTypo => {
                row.breakdown.char_score >= 0.72 && row.breakdown.coverage_score >= 0.75
            }
            MatchKind::NumericWindow => {
                row.breakdown.numeric_score >= 0.72 && row.breakdown.coverage_score >= 0.85
            }
            MatchKind::None
            | MatchKind::EmbeddedSocialPenalized
            | MatchKind::ReportedSpeechPenalized => false,
        }
    }

    fn exhaustive_sample_rescue_within_budget(
        &self,
        views: &SemanticViews,
        permitted: &impl Fn(&MeaningPattern) -> bool,
    ) -> bool {
        let mut patterns = 0usize;
        let mut evidence = 0usize;
        for pattern in self
            .catalog
            .patterns()
            .iter()
            .filter(|pattern| permitted(pattern))
        {
            patterns = patterns.saturating_add(1);
            if patterns > SEMANTIC_EXHAUSTIVE_RESCUE_PATTERNS_MAX {
                return false;
            }
            evidence = evidence
                .saturating_add(pattern.samples.len())
                .saturating_add(pattern.negative_samples.len());
            if evidence > SEMANTIC_EXHAUSTIVE_RESCUE_EVIDENCE_MAX {
                return false;
            }
        }
        let work = evidence
            .saturating_mul(views.views.len().max(1))
            .saturating_add(patterns);
        work <= SEMANTIC_EXHAUSTIVE_RESCUE_WORK_MAX
    }
}

/// Explicitly selected host context, bounded before it can reach an external resolver.
fn exposed_resolver_context(context: &BTreeMap<String, Value>) -> BTreeMap<String, Value> {
    context
        .iter()
        .take(crate::RESOLVER_CONTEXT_ENTRIES_MAX)
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect()
}

fn validate_custom_entity_profiles(
    profiles: &SemanticProfiles,
) -> Result<(), SemanticKernelBuildError> {
    for (language, profile) in profiles {
        if profile.custom_entities.len() > 256 {
            return Err(SemanticKernelBuildError::InvalidCustomEntities(format!(
                "{language}: too many custom entity kinds"
            )));
        }
        let mut total_values = 0usize;
        let mut total_aliases = 0usize;
        for (kind, values) in &profile.custom_entities {
            if kind.trim().is_empty() || values.is_empty() {
                return Err(SemanticKernelBuildError::InvalidCustomEntities(format!(
                    "{language}: empty custom entity kind/catalog"
                )));
            }
            total_values = total_values.saturating_add(values.len());
            let mut normalized_aliases = BTreeMap::<Vec<String>, String>::new();
            for (canonical, aliases) in values {
                if canonical.trim().is_empty() {
                    return Err(SemanticKernelBuildError::InvalidCustomEntities(format!(
                        "{language}:{kind}: empty canonical value"
                    )));
                }
                for alias in std::iter::once(canonical).chain(aliases.iter()) {
                    let normalized = profile.normalize_colloquial_tokens(&ordered_tokens(
                        &profile.normalize_text(alias),
                    ));
                    if normalized.is_empty() {
                        return Err(SemanticKernelBuildError::InvalidCustomEntities(format!(
                            "{language}:{kind}: empty normalized alias"
                        )));
                    }
                    total_aliases = total_aliases.saturating_add(1);
                    if normalized_aliases
                        .insert(normalized, canonical.clone())
                        .is_some_and(|existing| existing != *canonical)
                    {
                        return Err(SemanticKernelBuildError::InvalidCustomEntities(format!(
                            "{language}:{kind}: normalized alias collision"
                        )));
                    }
                }
            }
        }
        if total_values > 4_096 || total_aliases > 16_384 {
            return Err(SemanticKernelBuildError::InvalidCustomEntities(format!(
                "{language}: custom entity catalog exceeds canonical work budget"
            )));
        }
        let mut normalized_booleans = BTreeSet::new();
        for phrase in profile.boolean_values.keys() {
            let normalized = profile.normalize_text(phrase);
            if normalized.is_empty() || !normalized_booleans.insert(normalized) {
                return Err(SemanticKernelBuildError::InvalidCustomEntities(format!(
                    "{language}: invalid boolean vocabulary"
                )));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod collection_contract_tests;
#[cfg(test)]
mod firewall_tests;
#[cfg(test)]
mod runtime_index_tests;
#[cfg(test)]
mod tests;
