//! Conversation executable document hydration.

use super::*;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ConversationDoc {
    config: ConversationConfigDoc,
    behaviors: Vec<BehaviorDoc>,
    capability_result_behaviors: Vec<CapabilityResultBehaviorDoc>,
    openings: Vec<OpeningDoc>,
    fallback_behaviors: Vec<FallbackBehaviorDoc>,
    style_lexicon: StyleLexiconDoc,
}
impl ConversationDoc {
    pub(super) fn into_runtime(
        self,
    ) -> Result<(ConversationCatalog, ConversationConfig), ProgramError> {
        let config = self.config.into_runtime()?;
        let catalog = ConversationCatalog::new(
            self.behaviors
                .into_iter()
                .map(BehaviorDoc::into_runtime)
                .collect::<Result<_, _>>()?,
            self.capability_result_behaviors
                .into_iter()
                .map(CapabilityResultBehaviorDoc::into_runtime)
                .collect::<Result<_, _>>()?,
            self.openings
                .into_iter()
                .map(OpeningDoc::into_runtime)
                .collect::<Result<_, _>>()?,
            self.fallback_behaviors
                .into_iter()
                .map(FallbackBehaviorDoc::into_runtime)
                .collect::<Result<_, _>>()?,
        )
        .map_err(|error| ProgramError::InvalidConversationCatalog(format!("{error:?}")))?;
        Ok((
            catalog.with_style_lexicon(StyleLexicon {
                formal_terms: self.style_lexicon.formal_terms,
                informal_terms: self.style_lexicon.informal_terms,
            }),
            config,
        ))
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ConversationConfigDoc {
    default_topic_ttl: u32,
    default_followup_ttl: u32,
    recent_response_limit: usize,
    recent_variant_limit: usize,
    recent_user_window: usize,
    repeat_detection_window: usize,
    repeat_detection_threshold: u32,
    max_messages_per_turn: usize,
    repair_candidate_min_score: f64,
    author_numbers: Vec<AuthorNumberDoc>,
    topic_preference_margin: f64,
}
impl ConversationConfigDoc {
    pub(super) fn into_runtime(self) -> Result<ConversationConfig, ProgramError> {
        if !self.repair_candidate_min_score.is_finite() {
            return Err(ProgramError::NonFiniteNumber(
                "conversation.repair_candidate_min_score",
            ));
        }
        if !self.topic_preference_margin.is_finite() {
            return Err(ProgramError::NonFiniteNumber(
                "conversation.topic_preference_margin",
            ));
        }
        let config = ConversationConfig {
            default_topic_ttl: self.default_topic_ttl,
            default_followup_ttl: self.default_followup_ttl,
            recent_response_limit: self.recent_response_limit,
            recent_variant_limit: self.recent_variant_limit,
            recent_user_window: self.recent_user_window,
            repeat_detection_window: self.repeat_detection_window,
            repeat_detection_threshold: self.repeat_detection_threshold,
            max_messages_per_turn: self.max_messages_per_turn,
            repair_candidate_min_score: self.repair_candidate_min_score,
            author_numbers: self
                .author_numbers
                .into_iter()
                .map(AuthorNumberDoc::into_runtime)
                .collect::<Result<_, _>>()?,
            topic_preference_margin: self.topic_preference_margin,
        };
        config
            .validate()
            .map_err(|error| ProgramError::InvalidConversationConfig(error.0.into()))?;
        Ok(config)
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AuthorNumberDoc {
    path: String,
    default: f64,
    min: f64,
    max: f64,
}
impl AuthorNumberDoc {
    fn into_runtime(self) -> Result<AuthorNumberDefinition, ProgramError> {
        if !self.default.is_finite() || !self.min.is_finite() || !self.max.is_finite() {
            return Err(ProgramError::NonFiniteNumber("conversation.author_numbers"));
        }
        Ok(AuthorNumberDefinition {
            path: self.path,
            default: self.default,
            min: self.min,
            max: self.max,
        })
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct BehaviorDoc {
    id: String,
    meaning: String,
    topic: Option<String>,
    topic_scoped: bool,
    activates_topic: bool,
    topic_ttl: Option<u32>,
    followup_scope: Option<String>,
    repair_continuation_candidate: bool,
    repeat_same_input_after: Option<u32>,
    repeat_same_meaning_after: Option<u32>,
    requires_values: Vec<RequirementDoc>,
    forbidden_values: Vec<RequirementDoc>,
    responses: Vec<ResponseDoc>,
}
impl BehaviorDoc {
    pub(super) fn into_runtime(self) -> Result<ConversationBehavior, ProgramError> {
        Ok(ConversationBehavior {
            id: BehaviorId::new(self.id),
            meaning: MeaningId::new(self.meaning),
            topic: self.topic.map(TopicId::new),
            topic_scoped: self.topic_scoped,
            activates_topic: self.activates_topic,
            topic_ttl: self.topic_ttl,
            followup_scope: self.followup_scope.map(FollowupId::new),
            repair_continuation_candidate: self.repair_continuation_candidate,
            repeat_same_input_after: self.repeat_same_input_after,
            repeat_same_meaning_after: self.repeat_same_meaning_after,
            requires_values: self
                .requires_values
                .into_iter()
                .map(RequirementDoc::into_runtime)
                .collect::<Result<_, _>>()?,
            forbidden_values: self
                .forbidden_values
                .into_iter()
                .map(RequirementDoc::into_runtime)
                .collect::<Result<_, _>>()?,
            responses: self
                .responses
                .into_iter()
                .map(ResponseDoc::into_runtime)
                .collect::<Result<_, _>>()?,
        })
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CapabilityResultBehaviorDoc {
    id: String,
    capability: String,
    capability_version: String,
    succeeded: Option<bool>,
    error_code: Option<String>,
    responses: Vec<ResponseDoc>,
}
impl CapabilityResultBehaviorDoc {
    pub(super) fn into_runtime(self) -> Result<CapabilityResultBehavior, ProgramError> {
        Ok(CapabilityResultBehavior {
            id: BehaviorId::new(self.id),
            capability: CapabilityId::new(self.capability),
            capability_version: CapabilityVersion::new(self.capability_version),
            succeeded: self.succeeded,
            error_code: self.error_code,
            responses: self
                .responses
                .into_iter()
                .map(ResponseDoc::into_runtime)
                .collect::<Result<_, _>>()?,
        })
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct OpeningDoc {
    id: String,
    topic: Option<String>,
    topic_ttl: Option<u32>,
    responses: Vec<ResponseDoc>,
}
impl OpeningDoc {
    pub(super) fn into_runtime(self) -> Result<OpeningDefinition, ProgramError> {
        Ok(OpeningDefinition {
            id: OpeningId::new(self.id),
            topic: self.topic.map(TopicId::new),
            topic_ttl: self.topic_ttl,
            responses: self
                .responses
                .into_iter()
                .map(ResponseDoc::into_runtime)
                .collect::<Result<_, _>>()?,
        })
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct FallbackBehaviorDoc {
    id: String,
    trigger: String,
    priority: i32,
    conditions: Vec<ConditionDoc>,
    responses: Vec<ResponseDoc>,
}
impl FallbackBehaviorDoc {
    pub(super) fn into_runtime(self) -> Result<FallbackBehavior, ProgramError> {
        let trigger = match self.trigger.as_str() {
            "unresolved" => FallbackTrigger::Unresolved,
            "repeat" => FallbackTrigger::Repeat,
            other => {
                return Err(ProgramError::InvalidConversationCatalog(format!(
                    "unknown fallback trigger: {other}"
                )));
            }
        };
        Ok(FallbackBehavior {
            id: BehaviorId::new(self.id),
            trigger,
            priority: self.priority,
            conditions: self
                .conditions
                .into_iter()
                .map(ConditionDoc::into_runtime)
                .collect::<Result<_, _>>()?,
            responses: self
                .responses
                .into_iter()
                .map(ResponseDoc::into_runtime)
                .collect::<Result<_, _>>()?,
        })
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct StyleLexiconDoc {
    formal_terms: Vec<String>,
    informal_terms: Vec<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ResponseDoc {
    id: String,
    kind: String,
    texts: Vec<LocalizedTextsDoc>,
    conditions: Vec<ConditionDoc>,
    hint_level: Option<u32>,
    repeat_stage: Option<String>,
    effects: Vec<EffectDoc>,
    opens_followup: Option<FollowupDoc>,
    extra_messages: Vec<ExtraMessageDoc>,
    assets: Vec<ResponseAssetDoc>,
    links: Vec<ResponseLinkDoc>,
}
impl ResponseDoc {
    pub(super) fn into_runtime(self) -> Result<ResponseDefinition, ProgramError> {
        let kind = match self.kind.as_str() {
            "normal" => ResponseKind::Normal,
            "hint" => ResponseKind::Hint,
            "repeat" => ResponseKind::Repeat,
            "annoyed_repeat" => ResponseKind::AnnoyedRepeat,
            "final_repeat" => ResponseKind::FinalRepeat,
            "fallback" => ResponseKind::Fallback,
            "opening" => ResponseKind::Opening,
            _ => {
                return Err(ProgramError::InvalidConversationCatalog(format!(
                    "unknown response kind: {}",
                    self.kind
                )));
            }
        };
        let repeat_stage = match self.repeat_stage.as_deref() {
            None => None,
            Some("repeat") => Some(RepeatStage::Repeat),
            Some("annoyed") => Some(RepeatStage::Annoyed),
            Some("final") => Some(RepeatStage::Final),
            Some(value) => {
                return Err(ProgramError::InvalidConversationCatalog(format!(
                    "unknown repeat stage: {value}"
                )));
            }
        };
        Ok(ResponseDefinition {
            id: ResponseId::new(self.id),
            kind,
            texts: self
                .texts
                .into_iter()
                .map(LocalizedTextsDoc::into_runtime)
                .collect(),
            conditions: self
                .conditions
                .into_iter()
                .map(ConditionDoc::into_runtime)
                .collect::<Result<_, _>>()?,
            hint_level: self.hint_level,
            repeat_stage,
            effects: self
                .effects
                .into_iter()
                .map(EffectDoc::into_runtime)
                .collect::<Result<_, _>>()?,
            opens_followup: self.opens_followup.map(FollowupDoc::into_runtime),
            extra_messages: self
                .extra_messages
                .into_iter()
                .map(ExtraMessageDoc::into_runtime)
                .collect::<Result<_, _>>()?,
            assets: self
                .assets
                .into_iter()
                .map(|row| ResponseAsset {
                    asset_id: AssetId::new(row.asset_id),
                    alt_text: row.alt_text,
                })
                .collect(),
            links: self
                .links
                .into_iter()
                .map(|row| ResponseLink {
                    label: row.label,
                    url: row.url,
                })
                .collect(),
        })
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct LocalizedTextsDoc {
    language: String,
    variants: Vec<String>,
}
impl LocalizedTextsDoc {
    pub(super) fn into_runtime(self) -> LocalizedTexts {
        LocalizedTexts {
            language: self.language,
            variants: self.variants,
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RequirementDoc {
    namespace: String,
    path: String,
    value: JsonValue,
}
impl RequirementDoc {
    pub(super) fn into_runtime(self) -> Result<ValueRequirement, ProgramError> {
        Ok(ValueRequirement {
            path: ValuePath {
                namespace: parse_conversation_namespace(&self.namespace)?,
                path: self.path,
            },
            value: model_value(&self.value)?,
        })
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ConditionDoc {
    namespace: String,
    path: String,
    op: String,
    value: Option<JsonValue>,
}
impl ConditionDoc {
    pub(super) fn into_runtime(self) -> Result<ValueCondition, ProgramError> {
        let namespace = parse_conversation_namespace(&self.namespace)?;
        Ok(ValueCondition {
            path: ValuePath {
                namespace,
                path: self.path,
            },
            op: parse_conversation_op(&self.op)?,
            value: self.value.as_ref().map(model_value).transpose()?,
        })
    }
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum EffectDoc {
    Assign {
        target: StateTargetDoc,
        value: JsonValue,
    },
    Increment {
        target: StateTargetDoc,
        delta: f64,
    },
}
impl EffectDoc {
    pub(super) fn into_runtime(self) -> Result<ConversationEffect, ProgramError> {
        Ok(match self {
            Self::Assign { target, value } => ConversationEffect::Assign {
                target: target.into_runtime()?,
                value: model_value(&value)?,
            },
            Self::Increment { target, delta } => {
                if !delta.is_finite() {
                    return Err(ProgramError::NonFiniteNumber("conversation.effect.delta"));
                }
                ConversationEffect::Increment {
                    target: target.into_runtime()?,
                    delta,
                }
            }
        })
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct StateTargetDoc {
    namespace: String,
    path: String,
}
impl StateTargetDoc {
    pub(super) fn into_runtime(self) -> Result<StateTarget, ProgramError> {
        if self.namespace != "author" {
            return Err(ProgramError::InvalidConversationCatalog(format!(
                "unsupported writable state namespace: {}",
                self.namespace
            )));
        }
        Ok(StateTarget::Author(self.path))
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct FollowupDoc {
    id: String,
    ttl: u32,
    refresh_if_same: bool,
}
impl FollowupDoc {
    pub(super) fn into_runtime(self) -> FollowupDirective {
        FollowupDirective {
            id: FollowupId::new(self.id),
            ttl: self.ttl,
            refresh_if_same: self.refresh_if_same,
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ExtraMessageDoc {
    chance: f64,
    texts: Vec<LocalizedTextsDoc>,
}
impl ExtraMessageDoc {
    pub(super) fn into_runtime(self) -> Result<ExtraMessage, ProgramError> {
        if !self.chance.is_finite() {
            return Err(ProgramError::NonFiniteNumber(
                "conversation.extra_message.chance",
            ));
        }
        Ok(ExtraMessage {
            chance: self.chance,
            texts: self
                .texts
                .into_iter()
                .map(LocalizedTextsDoc::into_runtime)
                .collect(),
        })
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ResponseAssetDoc {
    asset_id: String,
    alt_text: Option<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ResponseLinkDoc {
    label: String,
    url: String,
}
