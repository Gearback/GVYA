//! Canonical conversation/response turn orchestration.

mod collection;
#[cfg(test)]
mod collection_contract_tests;
mod helpers;
mod interaction;
mod render;
mod selection;
#[cfg(test)]
mod tests;

use std::collections::{BTreeMap, BTreeSet};

use gvya_model::{
    ActiveCollection, BehaviorId, CapabilityId, CapabilityVersion, CollectionAuthority,
    ContextSnapshot, GvyaState, Meaning, MeaningId, MissingRequiredValue, ProposalId, ResponseItem,
    ResponseMessage, ResponsePlan, Trace, TraceCode, TraceEvent, TraceId, TraceVisibility, Value,
};

use crate::semantic::{
    CollectionTurnDecision, ElicitationPrompt, PartialMeaning, ResolutionSource, ResolverRunError,
    SemanticAnalysis, SemanticCatalog, SemanticConfig, SemanticDecision, SemanticInput,
    SemanticKernel, SemanticKernelBuildError, SemanticProfile, SemanticProfiles, compare_scored,
    normalize_language_tag, normalize_text, ordered_tokens,
};
use crate::{ResolverReferenceCandidate, SemanticResolver, UtteranceInput};

use super::{
    catalog::{
        CapabilityResultBehavior, ConversationBehavior, ConversationCatalog, FallbackBehavior,
        FallbackTrigger, OpeningDefinition, ResponseDefinition, ResponseKind,
    },
    conditions::{
        ConditionContext, apply_effects, conditions_match, initialize_author_numbers,
        value_requirement_matches,
    },
    selection::{HintRequest, LanguagePolicy, SelectedResponse, SelectionRequest, select_response},
    state::{
        ConversationConfig, FollowupTurnSnapshot, active_followup, active_topic_id,
        commit_repeat_memory, consume_followup, detect_user_style,
        finalize_followup_after_matching, global_repeat_count, hint_progress_key,
        project_repeat_counts, push_recent_response, push_recent_user_message, push_recent_variant,
        refresh_or_activate_topic, repeat_preference_for_thresholds, set_active_followup,
        set_hint_progress, tick_topic_at_turn_start, update_focus, update_repair_state,
    },
    templates::{DeterministicRng, TemplateEnvironment, TemplateRenderer, stable_seed},
};
use helpers::*;

#[derive(Clone, Debug, PartialEq)]
pub struct ConversationTurnRequest {
    pub utterance: UtteranceInput,
    pub context: ContextSnapshot,
    pub state: GvyaState,
    pub reference_candidates: Vec<ResolverReferenceCandidate>,
    pub resolver_context: BTreeMap<String, Value>,
    /// Explicit system facts. The kernel never reads the wall clock or process environment.
    pub system: BTreeMap<String, Value>,
    /// Explicit semantic fallback languages. Response fallback remains in `language_policy`.
    pub semantic_language_fallbacks: Vec<String>,
    pub language_policy: LanguagePolicy,
    pub hint: HintRequest,
    pub seed: Option<u64>,
}

impl ConversationTurnRequest {
    #[must_use]
    pub fn utterance(text: impl Into<String>, state: GvyaState) -> Self {
        Self {
            utterance: UtteranceInput {
                text: text.into(),
                language: None,
            },
            context: ContextSnapshot {
                values: BTreeMap::new(),
                visible_references: Vec::new(),
                available_capabilities: Vec::new(),
            },
            state,
            reference_candidates: Vec::new(),
            resolver_context: BTreeMap::new(),
            system: BTreeMap::new(),
            semantic_language_fallbacks: Vec::new(),
            language_policy: LanguagePolicy::default(),
            hint: HintRequest::None,
            seed: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ConversationOpenRequest {
    pub language: Option<String>,
    pub context: ContextSnapshot,
    pub state: GvyaState,
    pub system: BTreeMap<String, Value>,
    pub language_policy: LanguagePolicy,
    pub seed: Option<u64>,
}

/// Structurally validated host capability result entering deterministic conversation continuation.
/// The capability kernel validates correlation/schema before this request is constructed.
#[derive(Clone, Debug, PartialEq)]
pub struct ConversationCapabilityResultRequest {
    pub proposal_id: ProposalId,
    pub capability: CapabilityId,
    pub capability_version: CapabilityVersion,
    pub succeeded: bool,
    pub output: Option<Value>,
    pub error_code: Option<String>,
    pub language: Option<String>,
    pub context: ContextSnapshot,
    pub state: GvyaState,
    pub system: BTreeMap<String, Value>,
    pub language_policy: LanguagePolicy,
    pub seed: Option<u64>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConversationMode {
    Opening,
    CapabilityResult,
    Answer,
    Followup,
    Continuation,
    RepairContinuation,
    Collection,
    TopicContext,
    RepeatFallback,
    Fallback,
    Silent,
}

impl ConversationMode {
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Opening => "opening",
            Self::CapabilityResult => "capability_result",
            Self::Answer => "answer",
            Self::Followup => "followup",
            Self::Continuation => "continuation",
            Self::RepairContinuation => "repair_continuation",
            Self::Collection => "collection",
            Self::TopicContext => "topic_context",
            Self::RepeatFallback => "repeat_fallback",
            Self::Fallback => "fallback",
            Self::Silent => "silent",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ConversationOutcome {
    pub mode: ConversationMode,
    pub meaning: Option<Meaning>,
    pub behavior: Option<BehaviorId>,
    pub response: ResponsePlan,
    pub state: GvyaState,
    pub semantic: Option<SemanticAnalysis>,
    pub trace: Trace,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ConversationKernelBuildError {
    Semantic(SemanticKernelBuildError),
    Conversation(super::ConversationConfigError),
}

#[derive(Clone, Debug, PartialEq)]
pub struct ConversationKernel {
    semantic: SemanticKernel,
    catalog: ConversationCatalog,
    config: ConversationConfig,
}

impl ConversationKernel {
    #[must_use]
    pub fn new(
        semantic_catalog: SemanticCatalog,
        semantic_profiles: SemanticProfiles,
        semantic_config: SemanticConfig,
        catalog: ConversationCatalog,
        config: ConversationConfig,
    ) -> Result<Self, ConversationKernelBuildError> {
        config
            .validate()
            .map_err(ConversationKernelBuildError::Conversation)?;
        let semantic = SemanticKernel::new(semantic_catalog, semantic_profiles, semantic_config)
            .map_err(ConversationKernelBuildError::Semantic)?;
        Ok(Self {
            semantic,
            catalog,
            config,
        })
    }

    /// Runtime constructor accepting a compiler-hydrated semantic kernel.
    pub fn from_semantic_kernel(
        semantic: SemanticKernel,
        catalog: ConversationCatalog,
        config: ConversationConfig,
    ) -> Result<Self, super::ConversationConfigError> {
        config.validate()?;
        Ok(Self {
            semantic,
            catalog,
            config,
        })
    }

    #[must_use]
    pub fn catalog(&self) -> &ConversationCatalog {
        &self.catalog
    }

    #[must_use]
    pub fn config(&self) -> &ConversationConfig {
        &self.config
    }

    #[must_use]
    pub fn respond(&self, request: ConversationTurnRequest) -> ConversationOutcome {
        match self.respond_internal(request, None) {
            Ok(outcome) => outcome,
            Err(_impossible) => {
                unreachable!("deterministic conversation path cannot produce resolver errors")
            }
        }
    }

    pub fn respond_with_resolver(
        &self,
        request: ConversationTurnRequest,
        resolver: &dyn SemanticResolver<Error = String>,
    ) -> Result<ConversationOutcome, ResolverRunError<String>> {
        let deterministic_request = request.clone();
        match self.respond_internal(request, Some(resolver)) {
            Ok(outcome) => Ok(outcome),
            Err(ResolverRunError::Resolver(_)) => {
                // The external resolver is optional and untrusted. Transport/decoder/provider
                // failure must never take down an otherwise-valid deterministic turn. Re-run the
                // exact request without resolver assistance and record only a stable, non-sensitive
                // diagnostic; host/model error text is deliberately not copied into traces.
                let mut outcome = self.respond(deterministic_request);
                let resolver_event = event(
                    "semantic.resolver.unavailable",
                    "semantic",
                    "Optional semantic resolver was unavailable; deterministic result preserved",
                    map([("reason", Value::String("resolver_error".to_string()))]),
                );
                outcome.trace.events.push(resolver_event.clone());
                if let Some(semantic) = outcome.semantic.as_mut() {
                    semantic.trace.events.push(resolver_event);
                }
                Ok(outcome)
            }
        }
    }

    fn respond_internal(
        &self,
        mut request: ConversationTurnRequest,
        resolver: Option<&dyn SemanticResolver<Error = String>>,
    ) -> Result<ConversationOutcome, ResolverRunError<String>> {
        let normalized_for_system = normalize_text(&request.utterance.text);
        if !request.system.contains_key("mathResult") {
            if let Some(result) = super::templates::basic_math_result(&normalized_for_system) {
                request
                    .system
                    .insert("mathResult".to_string(), Value::String(result));
            }
        }
        let mut state = request.state.clone();
        initialize_author_numbers(&mut state.author, &self.config.author_numbers);
        request.utterance.language = activate_language(
            request.utterance.language.as_deref(),
            &mut state.conversation,
        );
        let neutral_profile = SemanticProfile::empty();
        let semantic_profile = self
            .semantic
            .profile_for_language(
                request.utterance.language.as_deref(),
                &request.semantic_language_fallbacks,
            )
            .unwrap_or(&neutral_profile);
        tick_topic_at_turn_start(&mut state.conversation);
        let mut followup_snapshot = FollowupTurnSnapshot::new(&state.conversation);
        let normalized = semantic_profile.normalize_text(&request.utterance.text);
        state.conversation.user_style = detect_user_style(
            &request.utterance.text,
            &normalized,
            self.catalog.style_lexicon(),
        );
        let trace_id = TraceId::new(format!("conversation:{}", state.conversation.turn_index));
        let mut trace = Trace {
            id: trace_id,
            events: Vec::new(),
        };
        trace.events.push(event(
            "conversation.turn.start",
            "conversation",
            "Started deterministic conversation turn",
            map([
                ("normalized", Value::String(normalized.clone())),
                (
                    "turn_index",
                    Value::Number(state.conversation.turn_index as f64),
                ),
                (
                    "user_style",
                    Value::String(
                        formality_label(state.conversation.user_style.formality).to_string(),
                    ),
                ),
                (
                    "user_style_confidence",
                    Value::Number(state.conversation.user_style.confidence),
                ),
            ]),
        ));

        if state.conversation.active_collection.is_some() {
            if let Some(outcome) = self.continue_active_collection(
                &request,
                state.clone(),
                &normalized,
                &mut followup_snapshot,
                resolver,
                &mut trace,
            )? {
                return Ok(outcome);
            }
            state.conversation.active_collection = None;
        }

        let global_repeat = global_repeat_count(
            &state.conversation,
            &normalized,
            self.config.repeat_detection_window,
        );
        let repeat_bypass_threshold = state
            .conversation
            .last_behavior
            .as_ref()
            .and_then(|behavior| self.catalog.behavior(behavior))
            .and_then(|behavior| behavior.repeat_same_input_after)
            .map_or(self.config.repeat_detection_threshold, |after| {
                self.config
                    .repeat_detection_threshold
                    .max(after.saturating_add(3))
            });
        if global_repeat >= repeat_bypass_threshold {
            trace.events.push(event(
                "conversation.repeat.global",
                "conversation.repeat",
                "Global repeated utterance bypassed ordinary semantic matching",
                map([("count", Value::Number(f64::from(global_repeat)))]),
            ));
            finalize_followup_after_matching(&mut state.conversation, &mut followup_snapshot);
            let outcome = self.render_fallback_trigger(
                FallbackTrigger::Repeat,
                ConversationMode::RepeatFallback,
                &request,
                state,
                normalized,
                followup_snapshot,
                &mut trace,
                true,
            );
            return Ok(outcome);
        }

        // 1. Active follow-up is a strict conversational scope and gets the first chance.
        if let Some(active) = active_followup(&state.conversation).cloned() {
            let allowed = self.catalog.followup_meanings(&active.id);
            if !allowed.is_empty() {
                let analysis = self.analyze_allowed_optional(&request, &allowed, resolver)?;
                if let Some((meaning, behavior, analysis)) = self.eligible_resolved_behavior(
                    &analysis,
                    Some(&active.id),
                    &request,
                    &state,
                    &normalized,
                    &mut trace,
                ) {
                    consume_followup(&mut state.conversation, &mut followup_snapshot);
                    trace.events.push(event(
                        "conversation.followup.accepted",
                        "conversation.followup",
                        "Resolved utterance inside active follow-up scope",
                        map([("followup", Value::String(active.id.as_str().to_string()))]),
                    ));
                    return Ok(self.answer_behavior(
                        ConversationMode::Followup,
                        meaning,
                        behavior,
                        analysis,
                        &request,
                        state,
                        normalized,
                        followup_snapshot,
                        &mut trace,
                    ));
                }
                trace.events.push(event(
                    "conversation.followup.miss",
                    "conversation.followup",
                    "Active follow-up scope did not resolve the utterance",
                    map([("followup", Value::String(active.id.as_str().to_string()))]),
                ));
            }
        }

        // 2. Normal semantics are constrained by topic/follow-up eligibility. Active-topic
        // semantics are also evaluated separately so stickiness is explicit and inspectable.
        let allowed = self
            .catalog
            .normal_meanings(active_topic_id(&state.conversation));
        let global_analysis = self.analyze_allowed_optional(&request, &allowed, resolver)?;
        let mut selected_analysis = global_analysis.clone();
        let mut mode = ConversationMode::Answer;

        if let Some(topic) = active_topic_id(&state.conversation) {
            let topic_allowed = self.catalog.topic_meanings(topic);
            if !topic_allowed.is_empty() {
                let topic_analysis =
                    self.analyze_allowed_optional(&request, &topic_allowed, resolver)?;
                if prefer_topic_analysis(
                    &topic_analysis,
                    &global_analysis,
                    self.config.topic_preference_margin,
                ) {
                    selected_analysis = topic_analysis;
                    mode = ConversationMode::TopicContext;
                    trace.events.push(event(
                        "conversation.topic.preference",
                        "conversation.topic",
                        "Active topic supplied the preferred semantic resolution",
                        map([("topic", Value::String(topic.as_str().to_string()))]),
                    ));
                }
            }
        }

        if let SemanticDecision::Partial { partial, source } = &selected_analysis.decision {
            finalize_followup_after_matching(&mut state.conversation, &mut followup_snapshot);
            return Ok(self.begin_collection(
                partial.clone(),
                source.clone(),
                selected_analysis,
                &request,
                state,
                normalized,
                followup_snapshot,
                &mut trace,
            ));
        }

        // 3. Short referential continuation is explicit conversation logic, not a synthetic
        // semantic score. It may reuse prior context only after standalone semantics has had an
        // opportunity to expose strong independent evidence for the current utterance.
        if is_contextual_continuation(&normalized, semantic_profile)
            && !has_strong_standalone_evidence(&selected_analysis)
        {
            if let Some(last_meaning) = state.conversation.last_meaning.clone() {
                if let Some(behavior) = self.catalog.behavior_for_meaning(&last_meaning, None) {
                    let meaning = Meaning {
                        id: last_meaning.clone(),
                        slots: Vec::new(),
                        references: state.conversation.focus.clone(),
                    };
                    if self.behavior_eligible(behavior, &state, &request, &meaning) {
                        let analysis =
                            self.synthetic_continuation_analysis(&request, meaning.clone());
                        trace.events.push(event(
                            "conversation.continuation.accepted",
                            "conversation.continuation",
                            "Resolved short utterance from explicit conversation continuity after standalone semantic evidence stayed weak",
                            map([("meaning", Value::String(last_meaning.as_str().to_string()))]),
                        ));
                        finalize_followup_after_matching(
                            &mut state.conversation,
                            &mut followup_snapshot,
                        );
                        return Ok(self.answer_behavior(
                            ConversationMode::Continuation,
                            meaning,
                            behavior,
                            analysis,
                            &request,
                            state,
                            normalized,
                            followup_snapshot,
                            &mut trace,
                        ));
                    }
                }
            }
        }

        finalize_followup_after_matching(&mut state.conversation, &mut followup_snapshot);
        if let Some((meaning, behavior, selected_analysis)) = self.eligible_resolved_behavior(
            &selected_analysis,
            None,
            &request,
            &state,
            &normalized,
            &mut trace,
        ) {
            return Ok(self.answer_behavior(
                mode,
                meaning,
                behavior,
                selected_analysis,
                &request,
                state,
                normalized,
                followup_snapshot,
                &mut trace,
            ));
        }

        // Preserve the useful reference follow-up floor without letting generic phrases override
        // strong standalone semantic evidence. This is explicit conversation continuity, not a
        // synthetic lexical score.
        if is_generic_followup_phrase(&normalized, semantic_profile)
            && !has_strong_standalone_evidence(&selected_analysis)
        {
            if let Some(last_meaning) = state.conversation.last_meaning.clone() {
                if let Some(behavior) = self.catalog.behavior_for_meaning(&last_meaning, None) {
                    let meaning = Meaning {
                        id: last_meaning.clone(),
                        slots: Vec::new(),
                        references: state.conversation.focus.clone(),
                    };
                    if self.behavior_eligible(behavior, &state, &request, &meaning) {
                        if self
                            .select_behavior_response(
                                &meaning,
                                behavior,
                                &request,
                                &state,
                                &normalized,
                            )
                            .is_some()
                        {
                            let analysis =
                                self.synthetic_continuation_analysis(&request, meaning.clone());
                            trace.events.push(event(
                                "conversation.continuation.generic",
                                "conversation.continuation",
                                "Resolved generic follow-up language from explicit conversation continuity after semantic evidence stayed weak",
                                map([("meaning", Value::String(last_meaning.as_str().to_string()))]),
                            ));
                            return Ok(self.answer_behavior(
                                ConversationMode::Continuation,
                                meaning,
                                behavior,
                                analysis,
                                &request,
                                state,
                                normalized,
                                followup_snapshot,
                                &mut trace,
                            ));
                        }
                    }
                }
            }
        }

        // A prior unresolved candidate may continue only for explicit generic follow-up language,
        // and only when the candidate can safely run without reconstructing required slots.
        if is_generic_followup_phrase(&normalized, semantic_profile)
            && !has_strong_standalone_evidence(&selected_analysis)
        {
            if let Some(previous_candidate) = state.conversation.repair.last_candidate.clone() {
                let can_resume_without_rebinding = self
                    .semantic
                    .catalog()
                    .get(&previous_candidate)
                    .is_some_and(|pattern| pattern.slots.iter().all(|slot| !slot.required));
                if can_resume_without_rebinding {
                    if let Some(behavior) =
                        self.catalog.behavior_for_meaning(&previous_candidate, None)
                    {
                        let meaning = Meaning {
                            id: previous_candidate.clone(),
                            slots: Vec::new(),
                            references: state.conversation.focus.clone(),
                        };
                        if behavior.repair_continuation_candidate
                            && self.behavior_eligible(behavior, &state, &request, &meaning)
                            && self
                                .select_behavior_response(
                                    &meaning,
                                    behavior,
                                    &request,
                                    &state,
                                    &normalized,
                                )
                                .is_some()
                        {
                            let analysis =
                                self.synthetic_continuation_analysis(&request, meaning.clone());
                            trace.events.push(event(
                                "conversation.repair.previous_candidate",
                                "conversation.repair",
                                "Continued a previously recorded repair candidate after explicit generic follow-up",
                                map([("meaning", Value::String(previous_candidate.as_str().to_string()))]),
                            ));
                            return Ok(self.answer_behavior(
                                ConversationMode::RepairContinuation,
                                meaning,
                                behavior,
                                analysis,
                                &request,
                                state,
                                normalized,
                                followup_snapshot,
                                &mut trace,
                            ));
                        }
                    }
                }
            }
        }

        // Near-match repair is separate from normal matcher authority. Only explicit Behavior opt-in
        // can cross the repair floor, and ordinary structural/Behavior/response eligibility still applies.
        if matches!(
            selected_analysis.decision,
            SemanticDecision::Unresolved { .. }
        ) {
            if let Some(row) = selected_analysis.scored.first() {
                let input = SemanticInput {
                    utterance: request.utterance.clone(),
                    language_fallbacks: request.semantic_language_fallbacks.clone(),
                    reference_candidates: request.reference_candidates.clone(),
                    resolver_context: request.resolver_context.clone(),
                };
                if let Some(meaning) = self.semantic.bind_scored_repair_candidate(
                    &input,
                    row,
                    self.config.repair_candidate_min_score,
                ) {
                    if let Some(behavior) = self.catalog.behavior_for_meaning(&meaning.id, None) {
                        if behavior.repair_continuation_candidate
                            && self.behavior_eligible(behavior, &state, &request, &meaning)
                            && self
                                .select_behavior_response(
                                    &meaning,
                                    behavior,
                                    &request,
                                    &state,
                                    &normalized,
                                )
                                .is_some()
                        {
                            trace.events.push(event(
                                "conversation.repair.candidate",
                                "conversation.repair",
                                "Accepted an explicitly repair-eligible below-threshold semantic candidate",
                                map([
                                    ("meaning", Value::String(meaning.id.as_str().to_string())),
                                    ("score", Value::Number(row.score)),
                                ]),
                            ));
                            return Ok(self.answer_behavior(
                                ConversationMode::RepairContinuation,
                                meaning,
                                behavior,
                                selected_analysis.clone(),
                                &request,
                                state,
                                normalized,
                                followup_snapshot,
                                &mut trace,
                            ));
                        }
                    }
                }
            }
        }

        let candidate = semantic_candidate(&selected_analysis).or_else(|| {
            selected_analysis
                .scored
                .first()
                .and_then(|row| self.semantic.catalog().patterns().get(row.pattern_index))
                .map(|pattern| pattern.id.clone())
        });
        trace.events.push(event(
            "conversation.semantic.unanswered",
            "conversation.response",
            "No answerable conversation behavior resolved",
            map([(
                "semantic_state",
                Value::String(semantic_decision_label(&selected_analysis.decision).to_string()),
            )]),
        ));
        Ok(self.render_fallback(
            &request,
            state,
            normalized,
            followup_snapshot,
            selected_analysis,
            candidate,
            &mut trace,
        ))
    }
}
