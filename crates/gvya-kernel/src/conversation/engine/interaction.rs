//! Conversation opening and host capability-result interactions.

use super::helpers::*;
use super::*;

impl ConversationKernel {
    #[must_use]
    pub fn open(&self, request: ConversationOpenRequest) -> ConversationOutcome {
        let ConversationOpenRequest {
            language,
            context,
            mut state,
            system,
            language_policy,
            seed,
        } = request;
        initialize_author_numbers(&mut state.author, &self.config.author_numbers);
        let language = activate_language(language.as_deref(), &mut state.conversation);
        // Opening is still a conversation interaction. The proven lifecycle ticks topic memory at
        // interaction start while leaving an unrelated active follow-up untouched.
        tick_topic_at_turn_start(&mut state.conversation);
        let trace_id = TraceId::new(format!(
            "conversation:open:{}",
            state.conversation.turn_index
        ));
        let mut trace = Trace {
            id: trace_id,
            events: vec![event(
                "conversation.open.start",
                "conversation.opening",
                "Started deterministic opening selection",
                BTreeMap::new(),
            )],
        };
        let mut candidates = Vec::new();
        for opening in self.catalog.openings() {
            if let Some(selected) = self.select_from_opening(
                opening,
                language.as_deref(),
                &context,
                &system,
                &language_policy,
                seed,
                &state,
            ) {
                candidates.push((opening, selected));
            }
        }
        if candidates.is_empty() {
            return ConversationOutcome {
                mode: ConversationMode::Silent,
                meaning: None,
                behavior: None,
                response: ResponsePlan::default(),
                state,
                semantic: None,
                trace,
            };
        }
        let seed_value = stable_seed(
            &["opening", &state.conversation.turn_index.to_string()],
            seed,
        );
        let mut rng = DeterministicRng::new(seed_value);
        let index = rng.index(candidates.len()).unwrap_or(0);
        let (opening, selected) = candidates.swap_remove(index);
        let mut snapshot = FollowupTurnSnapshot::new(&state.conversation);
        if let Some(topic) = &opening.topic {
            refresh_or_activate_topic(
                &mut state.conversation,
                topic,
                None,
                opening.topic_ttl.unwrap_or(self.config.default_topic_ttl),
            );
        }
        let response = self.apply_and_render_selected(
            selected,
            None,
            &context,
            &system,
            None,
            &language_policy,
            seed,
            &mut state,
            &mut snapshot,
            &mut trace,
            ResponseKind::Opening,
        );
        state.conversation.turn_index = state.conversation.turn_index.saturating_add(1);
        ConversationOutcome {
            mode: ConversationMode::Opening,
            meaning: None,
            behavior: None,
            response,
            state,
            semantic: None,
            trace,
        }
    }

    #[must_use]
    pub fn capability_result(
        &self,
        request: ConversationCapabilityResultRequest,
    ) -> ConversationOutcome {
        let interaction = capability_result_interaction_map(&request);
        let mut state = request.state.clone();
        initialize_author_numbers(&mut state.author, &self.config.author_numbers);
        let language = activate_language(request.language.as_deref(), &mut state.conversation);
        tick_topic_at_turn_start(&mut state.conversation);
        let trace_id = TraceId::new(format!(
            "conversation:capability-result:{}:{}",
            request.proposal_id.as_str(),
            state.conversation.turn_index,
        ));
        let mut trace = Trace {
            id: trace_id,
            events: vec![event(
                "conversation.capability_result.start",
                "conversation.capability_result",
                "Accepted host capability result entered deterministic conversation continuation",
                map([
                    (
                        "capability",
                        Value::String(request.capability.as_str().to_string()),
                    ),
                    ("succeeded", Value::Bool(request.succeeded)),
                ]),
            )],
        };

        let mut candidates: Vec<(&CapabilityResultBehavior, SelectedResponse<'_>, u8)> = Vec::new();
        for handler in self.catalog.capability_result_behaviors() {
            if handler.capability != request.capability
                || handler.capability_version != request.capability_version
            {
                continue;
            }
            if handler
                .succeeded
                .is_some_and(|expected| expected != request.succeeded)
            {
                continue;
            }
            if handler
                .error_code
                .as_ref()
                .is_some_and(|expected| request.error_code.as_deref() != Some(expected.as_str()))
            {
                continue;
            }
            let conditions = ConditionContext {
                author: &state.author,
                conversation: &state.conversation,
                host: &request.context.values,
                meaning: None,
                system: &request.system,
                interaction: Some(&interaction),
            };
            let turn = state.conversation.turn_index.to_string();
            let parts = [
                "capability_result",
                handler.id.as_str(),
                request.proposal_id.as_str(),
                turn.as_str(),
            ];
            if let Some(selected) = select_response(SelectionRequest {
                responses: &handler.responses,
                language: language.as_deref(),
                language_policy: &request.language_policy,
                conditions,
                recent_response_ids: &state.conversation.recent_response_ids,
                recent_variant_keys: &state.conversation.recent_variant_keys,
                repeat_preference: None,
                hint: HintRequest::None,
                hint_progress: 0,
                seed: request.seed,
                seed_parts: &parts,
            }) {
                let specificity =
                    u8::from(handler.succeeded.is_some()) + u8::from(handler.error_code.is_some());
                candidates.push((handler, selected, specificity));
            }
        }
        candidates.sort_by(|left, right| {
            right
                .2
                .cmp(&left.2)
                .then_with(|| left.0.id.as_str().cmp(right.0.id.as_str()))
        });
        let Some((handler, selected, _)) = candidates.into_iter().next() else {
            trace.events.push(event(
                "conversation.capability_result.unhandled",
                "conversation.capability_result",
                "No authored capability-result handler matched; result was accepted without synthetic conversation output",
                BTreeMap::new(),
            ));
            state.conversation.turn_index = state.conversation.turn_index.saturating_add(1);
            return ConversationOutcome {
                mode: ConversationMode::CapabilityResult,
                meaning: None,
                behavior: None,
                response: ResponsePlan::default(),
                state,
                semantic: None,
                trace,
            };
        };

        state.conversation.last_behavior = Some(handler.id.clone());
        let mut snapshot = FollowupTurnSnapshot::new(&state.conversation);
        let effective_kind = selected.response.kind;
        let response = self.apply_and_render_selected(
            selected,
            None,
            &request.context,
            &request.system,
            Some(&interaction),
            &request.language_policy,
            request.seed,
            &mut state,
            &mut snapshot,
            &mut trace,
            effective_kind,
        );
        trace.events.push(event(
            "conversation.capability_result.handled",
            "conversation.capability_result",
            "Selected authored continuation for accepted capability result",
            map([("handler", Value::String(handler.id.as_str().to_string()))]),
        ));
        state.conversation.turn_index = state.conversation.turn_index.saturating_add(1);
        ConversationOutcome {
            mode: ConversationMode::CapabilityResult,
            meaning: None,
            behavior: Some(handler.id.clone()),
            response,
            state,
            semantic: None,
            trace,
        }
    }
}
