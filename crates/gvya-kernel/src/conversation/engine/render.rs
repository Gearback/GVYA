//! Authored behavior/fallback rendering and state application.

use super::helpers::*;
use super::*;

impl ConversationKernel {
    pub(super) fn answer_behavior(
        &self,
        mode: ConversationMode,
        meaning: Meaning,
        behavior: &ConversationBehavior,
        analysis: SemanticAnalysis,
        request: &ConversationTurnRequest,
        mut state: GvyaState,
        normalized: String,
        mut followup_snapshot: FollowupTurnSnapshot,
        trace: &mut Trace,
    ) -> ConversationOutcome {
        let normalized = analysis
            .language
            .as_deref()
            .and_then(|language| self.semantic.profile_for_language(Some(language), &[]))
            .map_or(normalized, |profile| {
                profile.normalize_text(&request.utterance.text)
            });
        let (same_input, same_meaning) =
            project_repeat_counts(&state.conversation, &normalized, Some(&meaning.id));
        let hint_key = hint_progress_key(&behavior.id, &meaning.references);
        let preferred_language = analysis
            .language
            .as_deref()
            .or_else(|| self.matched_sample_language(&analysis, &meaning.id))
            .or(state.conversation.active_language.as_deref());
        let Some(selected) = self.select_behavior_response_for_language(
            &meaning,
            behavior,
            request,
            &state,
            &normalized,
            preferred_language,
        ) else {
            trace.events.push(event(
                "conversation.response.ineligible",
                "conversation.response",
                "Resolved behavior had no eligible response",
                map([("behavior", Value::String(behavior.id.as_str().to_string()))]),
            ));
            return self.render_fallback(
                request,
                state,
                normalized,
                followup_snapshot,
                analysis,
                Some(meaning.id.clone()),
                trace,
            );
        };

        // Follow-up miss finalization occurs before response-side opening/effects. This preserves
        // TTL=1 eligibility and prevents an expired scope from being reopened accidentally.
        finalize_followup_after_matching(&mut state.conversation, &mut followup_snapshot);
        state.conversation.active_collection = None;
        if let Some(language) = &analysis.language {
            state.conversation.active_language = Some(normalize_language_tag(language));
        }

        state.conversation.last_meaning = Some(meaning.id.clone());
        state.conversation.last_behavior = Some(behavior.id.clone());
        update_focus(&mut state.conversation, &meaning.references);
        if let Some(topic) = &behavior.topic {
            state.conversation.last_topic = Some(topic.clone());
            let current_same = active_topic_id(&state.conversation) == Some(topic);
            if behavior.activates_topic || current_same {
                refresh_or_activate_topic(
                    &mut state.conversation,
                    topic,
                    Some(behavior.id.clone()),
                    behavior.topic_ttl.unwrap_or(self.config.default_topic_ttl),
                );
            }
        }
        if let Some(level) = selected.selected_hint_level {
            set_hint_progress(&mut state.conversation, hint_key, level);
        }
        commit_repeat_memory(
            &mut state.conversation,
            normalized.clone(),
            Some(meaning.id.clone()),
            same_input,
            same_meaning,
        );
        update_repair_state(&mut state.conversation, false, mode.label(), None);

        let effective_kind = if selected.selected_hint_level.is_some() {
            ResponseKind::Hint
        } else if let Some(stage) = selected.response.repeat_stage {
            crate::conversation::selection::response_kind_for_repeat(stage)
        } else {
            selected.response.kind
        };
        let response = self.apply_and_render_selected(
            selected,
            Some(&meaning),
            &request.context,
            &request.system,
            None,
            &request.language_policy,
            request.seed,
            &mut state,
            &mut followup_snapshot,
            trace,
            effective_kind,
        );
        push_recent_user_message(
            &mut state.conversation,
            normalized,
            self.config.recent_user_window,
        );
        state.conversation.turn_index = state.conversation.turn_index.saturating_add(1);
        trace.events.push(event(
            "conversation.response.selected",
            "conversation.response",
            "Selected and rendered response",
            map([
                ("behavior", Value::String(behavior.id.as_str().to_string())),
                ("meaning", Value::String(meaning.id.as_str().to_string())),
            ]),
        ));
        ConversationOutcome {
            mode,
            meaning: Some(meaning),
            behavior: Some(behavior.id.clone()),
            response,
            state,
            semantic: Some(analysis),
            trace: trace.clone(),
        }
    }

    pub(super) fn fallback_behavior_selectable(
        &self,
        behavior: &FallbackBehavior,
        request: &ConversationTurnRequest,
        state: &GvyaState,
        normalized: &str,
    ) -> bool {
        let conditions = ConditionContext {
            author: &state.author,
            conversation: &state.conversation,
            host: &request.context.values,
            meaning: None,
            system: &request.system,
            interaction: None,
        };
        if !conditions_match(&behavior.conditions, &conditions) {
            return false;
        }
        let serial = state.conversation.repeat_fallback_serial.to_string();
        let parts = [
            behavior.trigger.label(),
            normalized,
            behavior.id.as_str(),
            serial.as_str(),
        ];
        select_response(SelectionRequest {
            responses: &behavior.responses,
            language: request
                .utterance
                .language
                .as_deref()
                .or(state.conversation.active_language.as_deref()),
            language_policy: &request.language_policy,
            conditions,
            recent_response_ids: &state.conversation.recent_response_ids,
            recent_variant_keys: &state.conversation.recent_variant_keys,
            repeat_preference: None,
            hint: HintRequest::None,
            hint_progress: 0,
            seed: request.seed,
            seed_parts: &parts,
        })
        .is_some()
    }

    pub(super) fn pick_fallback_behavior<'a>(
        &'a self,
        trigger: FallbackTrigger,
        request: &ConversationTurnRequest,
        state: &GvyaState,
        normalized: &str,
    ) -> Option<&'a FallbackBehavior> {
        let mut best_priority = i32::MIN;
        let mut candidates = Vec::new();
        for behavior in self
            .catalog
            .fallback_behaviors()
            .iter()
            .filter(|behavior| behavior.trigger == trigger)
        {
            if !self.fallback_behavior_selectable(behavior, request, state, normalized) {
                continue;
            }
            match behavior.priority.cmp(&best_priority) {
                std::cmp::Ordering::Greater => {
                    best_priority = behavior.priority;
                    candidates.clear();
                    candidates.push(behavior);
                }
                std::cmp::Ordering::Equal => candidates.push(behavior),
                std::cmp::Ordering::Less => {}
            }
        }
        if candidates.is_empty() {
            return None;
        }
        candidates.sort_by(|left, right| left.id.as_str().cmp(right.id.as_str()));
        let serial = state.conversation.repeat_fallback_serial.to_string();
        let priority = best_priority.to_string();
        let seed = stable_seed(
            &[
                "fallback_behavior",
                trigger.label(),
                normalized,
                serial.as_str(),
                priority.as_str(),
            ],
            request.seed,
        );
        DeterministicRng::new(seed)
            .index(candidates.len())
            .and_then(|index| candidates.get(index).copied())
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn render_standalone_pool(
        &self,
        mode: ConversationMode,
        pool: &[ResponseDefinition],
        meaning: Option<Meaning>,
        behavior: Option<BehaviorId>,
        request: &ConversationTurnRequest,
        mut state: GvyaState,
        normalized: String,
        mut followup_snapshot: FollowupTurnSnapshot,
        trace: &mut Trace,
        increment_repeat_serial: bool,
    ) -> ConversationOutcome {
        let conditions = ConditionContext {
            author: &state.author,
            conversation: &state.conversation,
            host: &request.context.values,
            meaning: meaning.as_ref(),
            system: &request.system,
            interaction: None,
        };
        let serial = state.conversation.repeat_fallback_serial.to_string();
        let parts = [mode.label(), normalized.as_str(), serial.as_str()];
        let selected = select_response(SelectionRequest {
            responses: pool,
            language: request.utterance.language.as_deref(),
            language_policy: &request.language_policy,
            conditions,
            recent_response_ids: &state.conversation.recent_response_ids,
            recent_variant_keys: &state.conversation.recent_variant_keys,
            repeat_preference: None,
            hint: HintRequest::None,
            hint_progress: 0,
            seed: request.seed,
            seed_parts: &parts,
        });
        let response = if let Some(selected) = selected {
            self.apply_and_render_selected(
                selected,
                meaning.as_ref(),
                &request.context,
                &request.system,
                None,
                &request.language_policy,
                request.seed,
                &mut state,
                &mut followup_snapshot,
                trace,
                if mode == ConversationMode::RepeatFallback {
                    ResponseKind::Repeat
                } else {
                    ResponseKind::Fallback
                },
            )
        } else {
            ResponsePlan::default()
        };
        if let Some(id) = &behavior {
            state.conversation.last_behavior = Some(id.clone());
        }
        let (same_input, _) = project_repeat_counts(&state.conversation, &normalized, None);
        commit_repeat_memory(
            &mut state.conversation,
            normalized.clone(),
            None,
            same_input,
            0,
        );
        update_repair_state(
            &mut state.conversation,
            mode == ConversationMode::Fallback,
            mode.label(),
            None,
        );
        if increment_repeat_serial {
            state.conversation.repeat_fallback_serial =
                state.conversation.repeat_fallback_serial.saturating_add(1);
        }
        push_recent_user_message(
            &mut state.conversation,
            normalized,
            self.config.recent_user_window,
        );
        state.conversation.turn_index = state.conversation.turn_index.saturating_add(1);
        ConversationOutcome {
            mode: if response.messages.is_empty() {
                ConversationMode::Silent
            } else {
                mode
            },
            meaning,
            behavior,
            response,
            state,
            semantic: None,
            trace: trace.clone(),
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn render_fallback_trigger(
        &self,
        trigger: FallbackTrigger,
        mode: ConversationMode,
        request: &ConversationTurnRequest,
        state: GvyaState,
        normalized: String,
        followup_snapshot: FollowupTurnSnapshot,
        trace: &mut Trace,
        increment_repeat_serial: bool,
    ) -> ConversationOutcome {
        if let Some(behavior) = self.pick_fallback_behavior(trigger, request, &state, &normalized) {
            trace.events.push(event(
                "conversation.fallback_behavior.selected",
                "conversation.fallback",
                "Selected the highest-priority eligible authored fallback behavior",
                map([
                    ("behavior", Value::String(behavior.id.as_str().to_string())),
                    ("trigger", Value::String(trigger.label().to_string())),
                    ("priority", Value::Number(f64::from(behavior.priority))),
                ]),
            ));
            return self.render_standalone_pool(
                mode,
                &behavior.responses,
                None,
                Some(behavior.id.clone()),
                request,
                state,
                normalized,
                followup_snapshot,
                trace,
                increment_repeat_serial,
            );
        }

        trace.events.push(event(
            "conversation.fallback.none",
            "conversation.fallback",
            "No authored fallback behavior was eligible; returned Silent",
            map([("trigger", Value::String(trigger.label().to_string()))]),
        ));
        self.render_standalone_pool(
            mode,
            &[],
            None,
            None,
            request,
            state,
            normalized,
            followup_snapshot,
            trace,
            increment_repeat_serial,
        )
    }

    pub(super) fn render_fallback(
        &self,
        request: &ConversationTurnRequest,
        state: GvyaState,
        normalized: String,
        followup_snapshot: FollowupTurnSnapshot,
        semantic: SemanticAnalysis,
        candidate: Option<MeaningId>,
        trace: &mut Trace,
    ) -> ConversationOutcome {
        let mut outcome = self.render_fallback_trigger(
            FallbackTrigger::Unresolved,
            ConversationMode::Fallback,
            request,
            state,
            normalized,
            followup_snapshot,
            trace,
            false,
        );
        outcome.semantic = Some(semantic);
        outcome.state.conversation.repair.last_candidate = candidate;
        outcome
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn apply_and_render_selected(
        &self,
        selected: SelectedResponse<'_>,
        meaning: Option<&Meaning>,
        context: &ContextSnapshot,
        system: &BTreeMap<String, Value>,
        interaction: Option<&BTreeMap<String, Value>>,
        language_policy: &LanguagePolicy,
        explicit_seed: Option<u64>,
        state: &mut GvyaState,
        followup_snapshot: &mut FollowupTurnSnapshot,
        trace: &mut Trace,
        effective_kind: ResponseKind,
    ) -> ResponsePlan {
        // Proven ordering: authored response effects are visible to the response template itself.
        apply_effects(
            &mut state.author,
            &selected.response.effects,
            &self.config.author_numbers,
        );
        push_recent_response(
            &mut state.conversation,
            selected.response.id.clone(),
            self.config.recent_response_limit,
        );
        push_recent_variant(
            &mut state.conversation,
            selected.variant_key.clone(),
            self.config.recent_variant_limit,
        );
        if let Some(directive) = &selected.response.opens_followup {
            let source_behavior = state.conversation.last_behavior.clone();
            let opened = set_active_followup(
                &mut state.conversation,
                followup_snapshot,
                directive.id.clone(),
                source_behavior,
                directive.ttl,
                directive.refresh_if_same,
            );
            trace.events.push(event(
                "conversation.followup.directive",
                "conversation.followup",
                "Applied response follow-up directive",
                map([
                    ("followup", Value::String(directive.id.as_str().to_string())),
                    ("opened", Value::Bool(opened)),
                    ("ttl", Value::Number(f64::from(directive.ttl))),
                ]),
            ));
        }

        let seed = stable_seed(
            &[
                selected.response.id.as_str(),
                selected.language.as_str(),
                &state.conversation.turn_index.to_string(),
            ],
            explicit_seed,
        );
        let env = TemplateEnvironment {
            host: context.values.clone(),
            system: system.clone(),
            interaction: interaction.cloned().unwrap_or_default(),
            meaning: meaning.cloned(),
            conversation: state.conversation.clone(),
        };
        let rendered =
            TemplateRenderer::new(&mut state.author, &env, seed).render(&selected.raw_text);
        if rendered.limit_exceeded {
            trace.events.push(event(
                "conversation.template.limit_exceeded",
                "conversation.template",
                "Template rendering exceeded canonical work/output limits and failed closed",
                BTreeMap::new(),
            ));
        }
        if !rendered.effects.is_empty() {
            trace.events.push(event(
                "conversation.template.effects",
                "conversation.template",
                "Template updated author state through allowlisted assignment",
                map([("count", Value::Number(rendered.effects.len() as f64))]),
            ));
        }

        let mut items = vec![ResponseItem::Text {
            text: rendered.text,
            language: Some(selected.language.clone()),
        }];
        for asset in &selected.response.assets {
            items.push(ResponseItem::Asset {
                asset_id: asset.asset_id.clone(),
                alt_text: asset.alt_text.clone(),
            });
        }
        let mut seen_links = BTreeSet::new();
        for link in &selected.response.links {
            let key = link.url.trim().to_ascii_lowercase();
            if !seen_links.insert(key) {
                continue;
            }
            items.push(ResponseItem::Link {
                label: link.label.clone(),
                url: link.url.clone(),
            });
        }
        let mut messages = vec![ResponseMessage {
            source_response: Some(selected.response.id.clone()),
            kind: effective_kind.label().to_string(),
            items,
        }];

        let max_extra = self.config.max_messages_per_turn.saturating_sub(1);
        for (index, extra) in selected
            .response
            .extra_messages
            .iter()
            .enumerate()
            .take(max_extra)
        {
            let extra_seed = stable_seed(
                &[
                    selected.response.id.as_str(),
                    "extra",
                    &index.to_string(),
                    &state.conversation.turn_index.to_string(),
                ],
                explicit_seed.map(|seed| seed.wrapping_add(index as u64 + 1)),
            );
            let mut rng = DeterministicRng::new(extra_seed);
            if extra.chance <= 0.0 || (extra.chance < 1.0 && rng.unit_f64() > extra.chance) {
                continue;
            }
            let Some(language) = crate::conversation::selection::resolve_language(
                &extra.texts,
                Some(&selected.language),
                language_policy,
            ) else {
                continue;
            };
            let Some(row) = extra.texts.iter().find(|row| {
                crate::conversation::selection::normalize_locale(&row.language) == language
            }) else {
                continue;
            };
            let valid: Vec<_> = row
                .variants
                .iter()
                .filter(|text| !text.trim().is_empty())
                .collect();
            let Some(raw) = rng
                .index(valid.len())
                .and_then(|pick| valid.get(pick))
                .map(|text| (*text).clone())
            else {
                continue;
            };
            let env = TemplateEnvironment {
                host: context.values.clone(),
                system: system.clone(),
                interaction: interaction.cloned().unwrap_or_default(),
                meaning: meaning.cloned(),
                conversation: state.conversation.clone(),
            };
            let rendered = TemplateRenderer::new(&mut state.author, &env, extra_seed).render(&raw);
            if rendered.limit_exceeded {
                trace.events.push(event(
                    "conversation.template.limit_exceeded",
                    "conversation.template",
                    "Extra-message template exceeded canonical work/output limits and failed closed",
                    BTreeMap::new(),
                ));
                continue;
            }
            messages.push(ResponseMessage {
                source_response: Some(selected.response.id.clone()),
                kind: "extra".to_string(),
                items: vec![ResponseItem::Text {
                    text: rendered.text,
                    language: Some(language),
                }],
            });
            if messages.len() >= self.config.max_messages_per_turn {
                break;
            }
        }
        ResponsePlan { messages }
    }
}
