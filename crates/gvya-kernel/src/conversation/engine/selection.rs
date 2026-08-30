//! Response, language, behavior eligibility and semantic re-analysis selection.

use super::helpers::*;
use super::*;
use gvya_model::FollowupId;

impl ConversationKernel {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn select_from_opening<'a>(
        &self,
        opening: &'a OpeningDefinition,
        language: Option<&str>,
        context: &ContextSnapshot,
        system: &BTreeMap<String, Value>,
        language_policy: &LanguagePolicy,
        seed: Option<u64>,
        state: &GvyaState,
    ) -> Option<SelectedResponse<'a>> {
        let conditions = ConditionContext {
            author: &state.author,
            conversation: &state.conversation,
            host: &context.values,
            meaning: None,
            system,
            interaction: None,
        };
        let turn = state.conversation.turn_index.to_string();
        let parts = ["opening", opening.id.as_str(), turn.as_str()];
        select_response(SelectionRequest {
            responses: &opening.responses,
            language,
            language_policy,
            conditions,
            recent_response_ids: &state.conversation.recent_response_ids,
            recent_variant_keys: &state.conversation.recent_variant_keys,
            repeat_preference: None,
            hint: HintRequest::None,
            hint_progress: 0,
            seed,
            seed_parts: &parts,
        })
    }

    pub(super) fn select_behavior_response<'a>(
        &self,
        meaning: &Meaning,
        behavior: &'a ConversationBehavior,
        request: &ConversationTurnRequest,
        state: &GvyaState,
        normalized: &str,
    ) -> Option<SelectedResponse<'a>> {
        let language = request
            .utterance
            .language
            .as_deref()
            .or(state.conversation.active_language.as_deref());
        self.select_behavior_response_for_language(
            meaning, behavior, request, state, normalized, language,
        )
    }

    pub(super) fn select_behavior_response_for_language<'a>(
        &self,
        meaning: &Meaning,
        behavior: &'a ConversationBehavior,
        request: &ConversationTurnRequest,
        state: &GvyaState,
        normalized: &str,
        language: Option<&str>,
    ) -> Option<SelectedResponse<'a>> {
        let (same_input, same_meaning) =
            project_repeat_counts(&state.conversation, normalized, Some(&meaning.id));
        let repeat = repeat_preference_for_thresholds(
            same_input,
            same_meaning,
            behavior.repeat_same_input_after,
            behavior.repeat_same_meaning_after,
        );
        let hint_key = hint_progress_key(&behavior.id, &meaning.references);
        let hint_progress = state
            .conversation
            .hint_progress
            .get(&hint_key)
            .copied()
            .unwrap_or(0);
        let conditions = ConditionContext {
            author: &state.author,
            conversation: &state.conversation,
            host: &request.context.values,
            meaning: Some(meaning),
            system: &request.system,
            interaction: None,
        };
        let seed_turn = state.conversation.turn_index.to_string();
        let parts = [normalized, behavior.id.as_str(), seed_turn.as_str()];
        select_response(SelectionRequest {
            responses: &behavior.responses,
            language,
            language_policy: &request.language_policy,
            conditions,
            recent_response_ids: &state.conversation.recent_response_ids,
            recent_variant_keys: &state.conversation.recent_variant_keys,
            repeat_preference: repeat,
            hint: request.hint.clone(),
            hint_progress,
            seed: request.seed,
            seed_parts: &parts,
        })
    }

    pub(super) fn matched_sample_language<'a>(
        &self,
        analysis: &'a SemanticAnalysis,
        meaning_id: &MeaningId,
    ) -> Option<&'a str> {
        if let Some(structural) = analysis
            .structural_match
            .as_ref()
            .filter(|row| &row.meaning == meaning_id)
        {
            return Some(structural.language.as_str());
        }
        analysis.scored.iter().find_map(|row| {
            self.semantic
                .catalog()
                .patterns()
                .get(row.pattern_index)
                .filter(|pattern| &pattern.id == meaning_id)
                .and_then(|_| row.breakdown.matched_sample_language.as_deref())
        })
    }

    pub(super) fn eligible_resolved_behavior<'a>(
        &'a self,
        analysis: &SemanticAnalysis,
        followup_scope: Option<&FollowupId>,
        request: &ConversationTurnRequest,
        state: &GvyaState,
        normalized: &str,
        trace: &mut Trace,
    ) -> Option<(Meaning, &'a ConversationBehavior, SemanticAnalysis)> {
        let SemanticDecision::Resolved {
            meaning: resolved,
            source,
        } = &analysis.decision
        else {
            return None;
        };
        let input = self.semantic_input_for_language(request, analysis.language.as_deref());

        // Resolver proposals already went through their own bounded deterministic review. Response
        // availability must not silently replace a resolver-selected Meaning with another semantic
        // candidate. Deterministic reference-style reranking applies only to deterministic results.
        if matches!(
            source,
            ResolutionSource::ResolverProposal | ResolutionSource::StructuralPattern
        ) {
            let behavior = self
                .catalog
                .behavior_for_meaning(&resolved.id, followup_scope)?;
            if !self.behavior_eligible(behavior, state, request, resolved) {
                trace.events.push(event(
                    "conversation.behavior.ineligible",
                    "conversation.behavior",
                    "Resolved Meaning was blocked by Behavior-level eligibility",
                    map([
                        ("behavior", Value::String(behavior.id.as_str().to_string())),
                        (
                            "required_values",
                            Value::Number(behavior.requires_values.len() as f64),
                        ),
                        (
                            "forbidden_values",
                            Value::Number(behavior.forbidden_values.len() as f64),
                        ),
                    ]),
                ));
                return None;
            }
            return self
                .select_behavior_response_for_language(
                    resolved,
                    behavior,
                    request,
                    state,
                    normalized,
                    analysis
                        .language
                        .as_deref()
                        .or(state.conversation.active_language.as_deref()),
                )
                .is_some()
                .then(|| (resolved.clone(), behavior, analysis.clone()));
        }

        let mut seen = BTreeSet::new();
        let mut candidates: Vec<(Meaning, f64)> = Vec::new();
        seen.insert(resolved.id.clone());
        let resolved_score = analysis
            .scored
            .iter()
            .find_map(|row| {
                self.semantic
                    .catalog()
                    .patterns()
                    .get(row.pattern_index)
                    .filter(|pattern| pattern.id == resolved.id)
                    .map(|_| row.score)
            })
            .unwrap_or(1.0);
        candidates.push((resolved.clone(), resolved_score));
        for row in &analysis.scored {
            let Some(meaning) = self.semantic.bind_scored_candidate(&input, row) else {
                continue;
            };
            if seen.insert(meaning.id.clone()) {
                candidates.push((meaning, row.score));
            }
        }

        for index in 0..candidates.len() {
            let (meaning, score) = &candidates[index];
            let Some(behavior) = self
                .catalog
                .behavior_for_meaning(&meaning.id, followup_scope)
            else {
                continue;
            };
            if !self.behavior_eligible(behavior, state, request, meaning) {
                trace.events.push(event(
                    "conversation.behavior.ineligible",
                    "conversation.behavior",
                    "Skipped semantic candidate because Behavior-level eligibility did not match",
                    map([
                        ("behavior", Value::String(behavior.id.as_str().to_string())),
                        ("meaning", Value::String(meaning.id.as_str().to_string())),
                    ]),
                ));
                continue;
            }
            if self
                .select_behavior_response_for_language(
                    meaning,
                    behavior,
                    request,
                    state,
                    normalized,
                    analysis
                        .language
                        .as_deref()
                        .or(state.conversation.active_language.as_deref()),
                )
                .is_some()
            {
                let mut selected_analysis = analysis.clone();
                if meaning.id != resolved.id {
                    // The original SemanticDecision was resolved because the original winner was
                    // clear. Once that winner is response-ineligible, the lower-ranked semantic
                    // frontier must be rechecked for ambiguity. Response availability must never
                    // manufacture semantic certainty between two near-equal runner-ups.
                    let margin = self.semantic.config().ambiguity_margin;
                    let previous_close = index > 1
                        && candidates
                            .get(index - 1)
                            .is_some_and(|(_, previous_score)| *previous_score - *score <= margin);
                    let next_close = candidates
                        .get(index + 1)
                        .is_some_and(|(_, next_score)| *score - *next_score <= margin);
                    if previous_close || next_close {
                        trace.events.push(event(
                            "conversation.response.rerank_ambiguous",
                            "conversation.response",
                            "Did not rerank because response eligibility would hide ambiguity on the remaining semantic frontier",
                            map([
                                ("meaning", Value::String(meaning.id.as_str().to_string())),
                                ("score", Value::Number(*score)),
                                ("previous_close", Value::Bool(previous_close)),
                                ("next_close", Value::Bool(next_close)),
                            ]),
                        ));
                        return None;
                    }
                    selected_analysis.decision = SemanticDecision::Resolved {
                        meaning: meaning.clone(),
                        source: ResolutionSource::Deterministic,
                    };
                    trace.events.push(event(
                        "conversation.response.reranked",
                        "conversation.response",
                        "Selected a lower-ranked semantic candidate because higher-ranked responses were ineligible and the remaining semantic frontier was unambiguous",
                        map([
                            ("meaning", Value::String(meaning.id.as_str().to_string())),
                            ("rank", Value::Number((index + 1) as f64)),
                        ]),
                    ));
                }
                return Some((meaning.clone(), behavior, selected_analysis));
            }
            trace.events.push(event(
                "conversation.response.candidate_ineligible",
                "conversation.response",
                "Skipped semantic candidate because its behavior had no eligible response",
                map([
                    ("meaning", Value::String(meaning.id.as_str().to_string())),
                    ("rank", Value::Number((index + 1) as f64)),
                ]),
            ));
        }
        None
    }

    pub(super) fn analyze_allowed(
        &self,
        request: &ConversationTurnRequest,
        allowed: &BTreeSet<MeaningId>,
    ) -> SemanticAnalysis {
        self.analyze_allowed_across_languages(request, allowed)
    }

    pub(super) fn analyze_allowed_optional(
        &self,
        request: &ConversationTurnRequest,
        allowed: &BTreeSet<MeaningId>,
        resolver: Option<&dyn SemanticResolver<Error = String>>,
    ) -> Result<SemanticAnalysis, ResolverRunError<String>> {
        let analysis = self.analyze_allowed_across_languages(request, allowed);
        let input = self.semantic_input_for_language(request, analysis.language.as_deref());
        match resolver {
            Some(resolver) => self
                .semantic
                .analyze_allowed_with_resolver(&input, allowed, resolver),
            None => Ok(analysis),
        }
    }

    fn analyze_allowed_across_languages(
        &self,
        request: &ConversationTurnRequest,
        allowed: &BTreeSet<MeaningId>,
    ) -> SemanticAnalysis {
        let languages: Vec<String> = self.semantic.profiles().keys().cloned().collect();
        let analyses = if languages.is_empty() {
            vec![self.semantic.analyze_allowed(
                &self.semantic_input_for_language(request, request.utterance.language.as_deref()),
                allowed,
            )]
        } else {
            languages
                .iter()
                .map(|language| {
                    self.semantic.analyze_allowed(
                        &self.semantic_input_for_language(request, Some(language)),
                        allowed,
                    )
                })
                .collect()
        };
        select_joint_language_analysis(
            analyses,
            request.utterance.language.as_deref(),
            self.semantic.config().ambiguity_margin,
        )
    }

    fn semantic_input_for_language(
        &self,
        request: &ConversationTurnRequest,
        language: Option<&str>,
    ) -> SemanticInput {
        let mut utterance = request.utterance.clone();
        utterance.language = language.map(str::to_owned);
        SemanticInput {
            utterance,
            language_fallbacks: Vec::new(),
            reference_candidates: request.reference_candidates.clone(),
            resolver_context: request.resolver_context.clone(),
        }
    }

    pub(super) fn behavior_eligible(
        &self,
        behavior: &ConversationBehavior,
        state: &GvyaState,
        request: &ConversationTurnRequest,
        meaning: &Meaning,
    ) -> bool {
        if !self.behavior_scope_eligible(behavior, state) {
            return false;
        }
        let context = ConditionContext {
            author: &state.author,
            conversation: &state.conversation,
            host: &request.context.values,
            meaning: Some(meaning),
            system: &request.system,
            interaction: None,
        };
        behavior
            .requires_values
            .iter()
            .all(|requirement| value_requirement_matches(requirement, &context))
            && behavior
                .forbidden_values
                .iter()
                .all(|requirement| !value_requirement_matches(requirement, &context))
    }

    pub(super) fn behavior_scope_eligible(
        &self,
        behavior: &ConversationBehavior,
        state: &GvyaState,
    ) -> bool {
        if let Some(scope) = &behavior.followup_scope {
            if active_followup(&state.conversation).map(|followup| &followup.id) != Some(scope) {
                return false;
            }
        }
        if behavior.topic_scoped {
            if behavior.topic.as_ref() != active_topic_id(&state.conversation) {
                return false;
            }
        }
        true
    }

    pub(super) fn synthetic_continuation_analysis(
        &self,
        request: &ConversationTurnRequest,
        meaning: Meaning,
    ) -> SemanticAnalysis {
        let allowed = BTreeSet::from([meaning.id.clone()]);
        let mut analysis = self.analyze_allowed(request, &allowed);
        analysis.language = request.utterance.language.clone();
        analysis.decision = SemanticDecision::Resolved {
            meaning,
            source: ResolutionSource::Deterministic,
        };
        analysis
    }
}

fn select_joint_language_analysis(
    mut analyses: Vec<SemanticAnalysis>,
    active_language: Option<&str>,
    ambiguity_margin: f64,
) -> SemanticAnalysis {
    analyses.sort_by(|left, right| compare_language_analyses(left, right, active_language));
    let mut selected = analyses.remove(0);

    if selected.structural_match.is_some() {
        if let Some(selected_meaning) = decisive_meaning(&selected) {
            if let Some(contender) = analyses
                .iter()
                .filter(|analysis| analysis.structural_match.is_some())
                .find_map(|analysis| different_decision_meaning(analysis, &selected_meaning))
            {
                selected.language = None;
                selected.decision = SemanticDecision::Ambiguous {
                    candidates: vec![selected_meaning, contender],
                    reason_code: "cross_language_structural_patterns_tied".to_string(),
                };
            }
        }
        return selected;
    }

    let selected_meaning = decisive_meaning(&selected);
    if let (Some(best), Some(selected_meaning)) = (selected.scored.first(), selected_meaning) {
        if let Some(contender) = analyses.iter().find_map(|analysis| {
            let row = analysis.scored.first()?;
            let contender = different_decision_meaning(analysis, &selected_meaning)?;
            (best.breakdown.evidence_tier == row.breakdown.evidence_tier
                && best.score - row.score <= ambiguity_margin)
                .then_some(contender)
        }) {
            selected.language = None;
            selected.decision = SemanticDecision::Ambiguous {
                candidates: vec![selected_meaning, contender],
                reason_code: "cross_language_candidates_too_close".to_string(),
            };
        }
    }
    selected
}

fn compare_language_analyses(
    left: &SemanticAnalysis,
    right: &SemanticAnalysis,
    active_language: Option<&str>,
) -> std::cmp::Ordering {
    right
        .structural_match
        .is_some()
        .cmp(&left.structural_match.is_some())
        .then_with(|| match (left.scored.first(), right.scored.first()) {
            (Some(left_row), Some(right_row)) => compare_scored(
                left_row,
                right_row,
                left_row.meaning.as_str(),
                right_row.meaning.as_str(),
            )
            .then_with(|| language_preference(left, right, active_language)),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => decision_rank(right)
                .cmp(&decision_rank(left))
                .then_with(|| language_preference(left, right, active_language)),
        })
}

fn language_preference(
    left: &SemanticAnalysis,
    right: &SemanticAnalysis,
    active_language: Option<&str>,
) -> std::cmp::Ordering {
    let active = active_language.map(normalize_language_tag);
    let left_active = left.language.as_deref().map(normalize_language_tag) == active;
    let right_active = right.language.as_deref().map(normalize_language_tag) == active;
    right_active
        .cmp(&left_active)
        .then_with(|| left.language.cmp(&right.language))
}

fn decision_rank(analysis: &SemanticAnalysis) -> u8 {
    match analysis.decision {
        SemanticDecision::Resolved { .. } => 4,
        SemanticDecision::Partial { .. } => 3,
        SemanticDecision::Ambiguous { .. } => 2,
        SemanticDecision::Unresolved { .. } => 1,
    }
}

fn decisive_meaning(analysis: &SemanticAnalysis) -> Option<MeaningId> {
    match &analysis.decision {
        SemanticDecision::Resolved { meaning, .. } => Some(meaning.id.clone()),
        SemanticDecision::Partial { partial, .. } => Some(partial.meaning.id.clone()),
        SemanticDecision::Ambiguous { .. } | SemanticDecision::Unresolved { .. } => None,
    }
}

fn different_decision_meaning(
    analysis: &SemanticAnalysis,
    selected_meaning: &MeaningId,
) -> Option<MeaningId> {
    match &analysis.decision {
        SemanticDecision::Resolved { meaning, .. } if &meaning.id != selected_meaning => {
            Some(meaning.id.clone())
        }
        SemanticDecision::Partial { partial, .. } if &partial.meaning.id != selected_meaning => {
            Some(partial.meaning.id.clone())
        }
        SemanticDecision::Ambiguous { candidates, .. } => candidates
            .iter()
            .find(|meaning| *meaning != selected_meaning)
            .cloned(),
        SemanticDecision::Resolved { .. }
        | SemanticDecision::Partial { .. }
        | SemanticDecision::Unresolved { .. } => None,
    }
}
