//! Canonical conversation-owned partial-Meaning collection lifecycle.

use super::helpers::*;
use super::*;

impl ConversationKernel {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn continue_active_collection(
        &self,
        request: &ConversationTurnRequest,
        mut state: GvyaState,
        normalized: &str,
        followup_snapshot: &mut FollowupTurnSnapshot,
        resolver: Option<&dyn SemanticResolver<Error = String>>,
        trace: &mut Trace,
    ) -> Result<Option<ConversationOutcome>, ResolverRunError<String>> {
        let Some(active) = state.conversation.active_collection.clone() else {
            return Ok(None);
        };

        // A clearly independent deterministic Meaning is an explicit topic switch. Resolver output
        // is not used for this preview, so an external provider is invoked at most once per turn.
        let allowed = self
            .catalog
            .normal_meanings(active_topic_id(&state.conversation));
        let preview = self.analyze_allowed(request, &allowed);
        if decision_meaning_id(&preview.decision).is_some_and(|id| id != &active.meaning.id)
            && has_strong_standalone_evidence(&preview)
        {
            trace.events.push(event(
                "conversation.collection.topic_switch",
                "conversation.collection",
                "A clear independent Meaning superseded active value collection",
                map([(
                    "previous_meaning",
                    Value::String(active.meaning.id.as_str().to_string()),
                )]),
            ));
            state.conversation.active_collection = None;
            return Ok(None);
        }

        if let SemanticDecision::Resolved { meaning, .. } = &preview.decision {
            if meaning.id == active.meaning.id {
                state.conversation.active_collection = None;
                if let Some(behavior) = self.catalog.behavior_for_meaning(&meaning.id, None) {
                    if self.behavior_eligible(behavior, &state, request, meaning) {
                        return Ok(Some(self.answer_behavior(
                            ConversationMode::Answer,
                            meaning.clone(),
                            behavior,
                            preview,
                            request,
                            state,
                            normalized.to_string(),
                            followup_snapshot.clone(),
                            trace,
                        )));
                    }
                }
            }
        }

        let input = SemanticInput {
            utterance: request.utterance.clone(),
            language_fallbacks: request.semantic_language_fallbacks.clone(),
            reference_candidates: request.reference_candidates.clone(),
            resolver_context: request.resolver_context.clone(),
        };
        let decision = match resolver {
            Some(resolver) => self
                .semantic
                .continue_collection_with_resolver(&input, &active, resolver)?,
            None => self.semantic.continue_collection(&input, &active),
        };
        match decision {
            CollectionTurnDecision::Completed(meaning) => {
                state.conversation.active_collection = None;
                trace.events.push(event(
                    "conversation.collection.completed",
                    "conversation.collection",
                    "Completed a partial Meaning and returned to the normal Behavior pipeline",
                    map([("meaning", Value::String(meaning.id.as_str().to_string()))]),
                ));
                let analysis = self.collection_analysis(request, meaning.clone(), &active);
                if let Some(behavior) = self.catalog.behavior_for_meaning(&meaning.id, None) {
                    if self.behavior_eligible(behavior, &state, request, &meaning)
                        && self
                            .select_behavior_response(
                                &meaning, behavior, request, &state, normalized,
                            )
                            .is_some()
                    {
                        return Ok(Some(self.answer_behavior(
                            ConversationMode::Answer,
                            meaning,
                            behavior,
                            analysis,
                            request,
                            state,
                            normalized.to_string(),
                            followup_snapshot.clone(),
                            trace,
                        )));
                    }
                }
                let candidate = Some(meaning.id.clone());
                return Ok(Some(self.render_fallback(
                    request,
                    state,
                    normalized.to_string(),
                    followup_snapshot.clone(),
                    analysis,
                    candidate,
                    trace,
                )));
            }
            CollectionTurnDecision::Progressed {
                partial,
                reason_code,
            } => {
                trace.events.push(event(
                    "conversation.collection.progressed",
                    "conversation.collection",
                    "Preserved newly collected values and requested the next declaration",
                    map([("reason", Value::String(reason_code))]),
                ));
                state.conversation.active_collection = Some(ActiveCollection {
                    meaning: partial.meaning.clone(),
                    remaining: partial.missing_required_values.clone(),
                    authority: active.authority,
                    started_turn: active.started_turn,
                });
                let analysis = self.partial_collection_analysis(request, partial.clone(), &active);
                Ok(Some(self.render_collection_prompt(
                    partial,
                    analysis,
                    request,
                    state,
                    normalized,
                    followup_snapshot.clone(),
                    trace,
                )))
            }
            // Structurally invalid persisted/compiled collection state can never be repaired by
            // another user answer, so it must not hold the turn in an unbreakable prompt loop.
            // Fail closed: drop the collection and let the ordinary turn pipeline handle the input.
            CollectionTurnDecision::Invalid { reason_code }
                if reason_code.starts_with("collection_state_") =>
            {
                trace.events.push(event(
                    "conversation.collection.state_rejected",
                    "conversation.collection",
                    "Dropped an invalid persisted collection state instead of re-prompting from it",
                    map([("reason", Value::String(reason_code))]),
                ));
                state.conversation.active_collection = None;
                Ok(None)
            }
            CollectionTurnDecision::Ambiguous { reason_code }
            | CollectionTurnDecision::Invalid { reason_code } => {
                trace.events.push(event(
                    "conversation.collection.retry",
                    "conversation.collection",
                    "Collection remained active because the continuation was invalid or ambiguous",
                    map([("reason", Value::String(reason_code))]),
                ));
                let partial = PartialMeaning {
                    meaning: active.meaning.clone(),
                    missing_required_values: active.remaining.clone(),
                };
                let analysis = self.partial_collection_analysis(request, partial.clone(), &active);
                Ok(Some(self.render_collection_prompt(
                    partial,
                    analysis,
                    request,
                    state,
                    normalized,
                    followup_snapshot.clone(),
                    trace,
                )))
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn begin_collection(
        &self,
        partial: PartialMeaning,
        source: ResolutionSource,
        analysis: SemanticAnalysis,
        request: &ConversationTurnRequest,
        mut state: GvyaState,
        normalized: String,
        followup_snapshot: FollowupTurnSnapshot,
        trace: &mut Trace,
    ) -> ConversationOutcome {
        let authority = authority_from_source(&source);
        state.conversation.active_collection = Some(ActiveCollection {
            meaning: partial.meaning.clone(),
            remaining: partial.missing_required_values.clone(),
            authority,
            started_turn: state.conversation.turn_index,
        });
        trace.events.push(event(
            "conversation.collection.started",
            "conversation.collection",
            "Started canonical collection for a partial Meaning",
            map([
                (
                    "meaning",
                    Value::String(partial.meaning.id.as_str().to_string()),
                ),
                (
                    "remaining",
                    Value::Number(partial.missing_required_values.len() as f64),
                ),
            ]),
        ));
        self.render_collection_prompt(
            partial,
            analysis,
            request,
            state,
            &normalized,
            followup_snapshot,
            trace,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn render_collection_prompt(
        &self,
        partial: PartialMeaning,
        analysis: SemanticAnalysis,
        request: &ConversationTurnRequest,
        mut state: GvyaState,
        normalized: &str,
        followup_snapshot: FollowupTurnSnapshot,
        trace: &mut Trace,
    ) -> ConversationOutcome {
        // Collection is only honest while an authored localized prompt can actually be spoken.
        // A required declaration may carry prompts for a subset of the enabled languages, so a
        // turn in an uncovered language must fail closed into the canonical fallback pipeline
        // instead of holding the conversation in a mute collection.
        let Some(prompt) = self.collection_prompt(&partial, request, &state) else {
            trace.events.push(event(
                "conversation.collection.unelicitable",
                "conversation.collection",
                "No authored elicitation prompt was renderable for this turn language",
                map([(
                    "meaning",
                    Value::String(partial.meaning.id.as_str().to_string()),
                )]),
            ));
            state.conversation.active_collection = None;
            let candidate = Some(partial.meaning.id.clone());
            return self.render_fallback(
                request,
                state,
                normalized.to_string(),
                followup_snapshot,
                analysis,
                candidate,
                trace,
            );
        };
        let prompt = ResponsePlan {
            messages: vec![ResponseMessage {
                source_response: None,
                kind: "elicitation".into(),
                items: vec![ResponseItem::Text {
                    text: prompt.text.clone(),
                    language: Some(prompt.language.clone()),
                }],
            }],
        };
        push_recent_user_message(
            &mut state.conversation,
            normalized.to_string(),
            self.config.recent_user_window,
        );
        state.conversation.turn_index = state.conversation.turn_index.saturating_add(1);
        trace.events.push(event(
            "conversation.collection.elicited",
            "conversation.collection",
            "Rendered an authored localized elicitation prompt",
            BTreeMap::new(),
        ));
        ConversationOutcome {
            mode: ConversationMode::Collection,
            meaning: None,
            behavior: None,
            response: prompt,
            state,
            semantic: Some(analysis),
            trace: trace.clone(),
        }
    }

    fn collection_prompt<'a>(
        &'a self,
        partial: &PartialMeaning,
        request: &ConversationTurnRequest,
        state: &GvyaState,
    ) -> Option<&'a ElicitationPrompt> {
        let pattern = self.semantic.catalog().get(&partial.meaning.id)?;
        let prompts = match partial.missing_required_values.first()? {
            MissingRequiredValue::Slot { name } => pattern
                .slots
                .iter()
                .find(|slot| slot.name == *name)
                .map(|slot| slot.elicitation.as_slice())?,
            MissingRequiredValue::Reference { kind } => pattern
                .references
                .iter()
                .find(|reference| reference.kind == *kind)
                .map(|reference| reference.elicitation.as_slice())?,
        };
        let localized: Vec<crate::conversation::LocalizedTexts> = prompts
            .iter()
            .map(|prompt| crate::conversation::LocalizedTexts {
                language: prompt.language.clone(),
                variants: vec![prompt.text.clone()],
            })
            .collect();
        let requested = request
            .utterance
            .language
            .as_deref()
            .or(state.conversation.active_language.as_deref());
        let language =
            crate::conversation::resolve_language(&localized, requested, &request.language_policy)?;
        prompts.iter().find(|prompt| {
            crate::conversation::selection::normalize_locale(&prompt.language) == language
        })
    }

    fn collection_analysis(
        &self,
        request: &ConversationTurnRequest,
        meaning: Meaning,
        active: &ActiveCollection,
    ) -> SemanticAnalysis {
        let allowed = BTreeSet::from([meaning.id.clone()]);
        let mut analysis = self.analyze_allowed(request, &allowed);
        analysis.decision = SemanticDecision::Resolved {
            meaning,
            source: source_from_authority(active.authority),
        };
        replace_collection_decision_trace(
            &mut analysis,
            "semantic.decision.collection_resolved",
            "Active value collection completed the selected Meaning",
        );
        analysis
    }

    fn partial_collection_analysis(
        &self,
        request: &ConversationTurnRequest,
        partial: PartialMeaning,
        active: &ActiveCollection,
    ) -> SemanticAnalysis {
        let allowed = BTreeSet::from([partial.meaning.id.clone()]);
        let mut analysis = self.analyze_allowed(request, &allowed);
        analysis.decision = SemanticDecision::Partial {
            partial,
            source: source_from_authority(active.authority),
        };
        replace_collection_decision_trace(
            &mut analysis,
            "semantic.decision.collection_partial",
            "Active value collection preserved a still-partial Meaning",
        );
        analysis
    }
}

fn replace_collection_decision_trace(analysis: &mut SemanticAnalysis, code: &str, summary: &str) {
    analysis
        .trace
        .events
        .retain(|entry| !entry.code.as_str().starts_with("semantic.decision."));
    analysis
        .trace
        .events
        .push(event(code, "semantic.decision", summary, BTreeMap::new()));
}

fn decision_meaning_id(decision: &SemanticDecision) -> Option<&MeaningId> {
    match decision {
        SemanticDecision::Resolved { meaning, .. } => Some(&meaning.id),
        SemanticDecision::Partial { partial, .. } => Some(&partial.meaning.id),
        SemanticDecision::Ambiguous { .. } | SemanticDecision::Unresolved { .. } => None,
    }
}

fn authority_from_source(source: &ResolutionSource) -> CollectionAuthority {
    match source {
        ResolutionSource::StructuralPattern => CollectionAuthority::StructuralPattern,
        ResolutionSource::Deterministic => CollectionAuthority::Deterministic,
        ResolutionSource::ResolverProposal => CollectionAuthority::ResolverProposal,
    }
}

fn source_from_authority(authority: CollectionAuthority) -> ResolutionSource {
    match authority {
        CollectionAuthority::StructuralPattern => ResolutionSource::StructuralPattern,
        CollectionAuthority::Deterministic => ResolutionSource::Deterministic,
        CollectionAuthority::ResolverProposal => ResolutionSource::ResolverProposal,
    }
}
