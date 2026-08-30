//! Conversation authoring/runtime catalog after compilation.
//!
//! These types are executable runtime structures, not a source project format.

use std::collections::{BTreeMap, BTreeSet};

use gvya_model::{
    AssetId, BehaviorId, CapabilityId, CapabilityVersion, FollowupId, MeaningId, OpeningId,
    ResponseId, TopicId, Value,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StateNamespace {
    Author,
    Conversation,
    Context,
    Meaning,
    System,
    Interaction,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValuePath {
    pub namespace: StateNamespace,
    pub path: String,
}

impl ValuePath {
    #[must_use]
    pub fn author(path: impl Into<String>) -> Self {
        Self {
            namespace: StateNamespace::Author,
            path: path.into(),
        }
    }

    #[must_use]
    pub fn context(path: impl Into<String>) -> Self {
        Self {
            namespace: StateNamespace::Context,
            path: path.into(),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PredicateOp {
    Exists,
    Missing,
    Equal,
    NotEqual,
    Greater,
    GreaterOrEqual,
    Less,
    LessOrEqual,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ValueCondition {
    pub path: ValuePath,
    pub op: PredicateOp,
    pub value: Option<Value>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ValueRequirement {
    pub path: ValuePath,
    pub value: Value,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StateTarget {
    Author(String),
}

#[derive(Clone, Debug, PartialEq)]
pub enum ConversationEffect {
    Assign { target: StateTarget, value: Value },
    Increment { target: StateTarget, delta: f64 },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RepeatStage {
    Repeat,
    Annoyed,
    Final,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ResponseKind {
    Normal,
    Hint,
    Repeat,
    AnnoyedRepeat,
    FinalRepeat,
    Fallback,
    Opening,
}

impl ResponseKind {
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::Hint => "hint",
            Self::Repeat => "repeat",
            Self::AnnoyedRepeat => "annoyed_repeat",
            Self::FinalRepeat => "final_repeat",
            Self::Fallback => "fallback",
            Self::Opening => "opening",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct LocalizedTexts {
    pub language: String,
    pub variants: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResponseAsset {
    pub asset_id: AssetId,
    pub alt_text: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResponseLink {
    pub label: String,
    pub url: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FollowupDirective {
    pub id: FollowupId,
    pub ttl: u32,
    /// Default false preserves the proven behavior that opening the same still-active follow-up
    /// does not silently refresh its TTL. Authors must opt into refresh explicitly.
    pub refresh_if_same: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ExtraMessage {
    pub chance: f64,
    pub texts: Vec<LocalizedTexts>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ResponseDefinition {
    pub id: ResponseId,
    pub kind: ResponseKind,
    pub texts: Vec<LocalizedTexts>,
    pub conditions: Vec<ValueCondition>,
    pub hint_level: Option<u32>,
    pub repeat_stage: Option<RepeatStage>,
    pub effects: Vec<ConversationEffect>,
    pub opens_followup: Option<FollowupDirective>,
    pub extra_messages: Vec<ExtraMessage>,
    pub assets: Vec<ResponseAsset>,
    pub links: Vec<ResponseLink>,
}

impl ResponseDefinition {
    #[must_use]
    pub fn text(
        id: impl Into<String>,
        language: impl Into<String>,
        text: impl Into<String>,
    ) -> Self {
        Self {
            id: ResponseId::new(id.into()),
            kind: ResponseKind::Normal,
            texts: vec![LocalizedTexts {
                language: language.into(),
                variants: vec![text.into()],
            }],
            conditions: Vec::new(),
            hint_level: None,
            repeat_stage: None,
            effects: Vec::new(),
            opens_followup: None,
            extra_messages: Vec::new(),
            assets: Vec::new(),
            links: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ConversationBehavior {
    pub id: BehaviorId,
    pub meaning: MeaningId,
    pub topic: Option<TopicId>,
    /// When true, this behavior is ineligible unless `topic` is currently active.
    pub topic_scoped: bool,
    /// When true, accepting this behavior activates/refreshes its `topic`.
    pub activates_topic: bool,
    pub topic_ttl: Option<u32>,
    /// When set, this behavior is eligible only while this follow-up scope is active.
    pub followup_scope: Option<FollowupId>,
    /// Explicit opt-in: a below-threshold semantic candidate may answer only through deterministic repair.
    pub repair_continuation_candidate: bool,
    /// Optional Behavior-local repeat ladder start. Supported range is 2..=20.
    pub repeat_same_input_after: Option<u32>,
    /// Optional Behavior-local same-Meaning repeat ladder start. Supported range is 2..=20.
    pub repeat_same_meaning_after: Option<u32>,
    /// Exact behavior-level eligibility gates. Every required value must match.
    pub requires_values: Vec<ValueRequirement>,
    /// Exact behavior-level exclusion gates. Any matching forbidden value makes this behavior ineligible.
    pub forbidden_values: Vec<ValueRequirement>,
    pub responses: Vec<ResponseDefinition>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CapabilityResultBehavior {
    /// Author-facing deterministic handler identity. It participates in `behavior` capability
    /// triggers after a host result is accepted, allowing result-driven capability chains without
    /// granting the runtime host execution authority.
    pub id: BehaviorId,
    pub capability: CapabilityId,
    pub capability_version: CapabilityVersion,
    /// `None` matches both success and failure. `Some(true/false)` narrows the handler.
    pub succeeded: Option<bool>,
    /// Optional exact host error code. A declared error code is meaningful only for failures.
    pub error_code: Option<String>,
    pub responses: Vec<ResponseDefinition>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum FallbackTrigger {
    Unresolved,
    Repeat,
}

impl FallbackTrigger {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Unresolved => "unresolved",
            Self::Repeat => "repeat",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct FallbackBehavior {
    pub id: BehaviorId,
    pub trigger: FallbackTrigger,
    pub priority: i32,
    pub conditions: Vec<ValueCondition>,
    pub responses: Vec<ResponseDefinition>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct OpeningDefinition {
    pub id: OpeningId,
    pub topic: Option<TopicId>,
    pub topic_ttl: Option<u32>,
    pub responses: Vec<ResponseDefinition>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct StyleLexicon {
    pub formal_terms: Vec<String>,
    pub informal_terms: Vec<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ConversationCatalog {
    behaviors: Vec<ConversationBehavior>,
    capability_result_behaviors: Vec<CapabilityResultBehavior>,
    openings: Vec<OpeningDefinition>,
    fallback_behaviors: Vec<FallbackBehavior>,
    style_lexicon: StyleLexicon,
    behavior_by_id: BTreeMap<BehaviorId, usize>,
    default_behavior_by_meaning: BTreeMap<MeaningId, usize>,
    followup_behavior_by_meaning: BTreeMap<(MeaningId, FollowupId), usize>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ConversationCatalogError {
    EmptyBehaviorId,
    EmptyResponseId,
    DuplicateBehaviorId(String),
    DuplicateBehaviorScope {
        meaning: String,
        followup: Option<String>,
    },
    DuplicateResponseId(String),
    MissingTopicForScopedBehavior(String),
    InvalidRepeatThreshold(String),
    EmptyLocalizedLanguage(String),
    EmptyLocalizedVariants(String),
    InvalidChance(String),
    InvalidFollowupTtl(String),
    InvalidCapabilityResultHandler(String),
    UnsafeLink(String),
    LinkLabelTooLong(String),
}

impl ConversationCatalog {
    pub fn new(
        behaviors: Vec<ConversationBehavior>,
        capability_result_behaviors: Vec<CapabilityResultBehavior>,
        openings: Vec<OpeningDefinition>,
        fallback_behaviors: Vec<FallbackBehavior>,
    ) -> Result<Self, ConversationCatalogError> {
        let mut behavior_ids = BTreeSet::new();
        let mut response_ids = BTreeSet::new();
        let mut behavior_by_id = BTreeMap::new();
        let mut default_behavior_by_meaning = BTreeMap::new();
        let mut followup_behavior_by_meaning = BTreeMap::new();

        for (index, behavior) in behaviors.iter().enumerate() {
            if behavior.id.as_str().trim().is_empty() {
                return Err(ConversationCatalogError::EmptyBehaviorId);
            }
            if !behavior_ids.insert(behavior.id.as_str().to_string()) {
                return Err(ConversationCatalogError::DuplicateBehaviorId(
                    behavior.id.as_str().to_string(),
                ));
            }
            let duplicate_scope = if let Some(followup) = &behavior.followup_scope {
                followup_behavior_by_meaning
                    .insert((behavior.meaning.clone(), followup.clone()), index)
                    .is_some()
            } else {
                default_behavior_by_meaning
                    .insert(behavior.meaning.clone(), index)
                    .is_some()
            };
            if duplicate_scope {
                return Err(ConversationCatalogError::DuplicateBehaviorScope {
                    meaning: behavior.meaning.as_str().to_string(),
                    followup: behavior
                        .followup_scope
                        .as_ref()
                        .map(|scope| scope.as_str().to_string()),
                });
            }
            if behavior.topic_scoped && behavior.topic.is_none() {
                return Err(ConversationCatalogError::MissingTopicForScopedBehavior(
                    behavior.id.as_str().to_string(),
                ));
            }
            if behavior
                .repeat_same_input_after
                .is_some_and(|value| !(2..=20).contains(&value))
                || behavior
                    .repeat_same_meaning_after
                    .is_some_and(|value| !(2..=20).contains(&value))
            {
                return Err(ConversationCatalogError::InvalidRepeatThreshold(
                    behavior.id.as_str().to_string(),
                ));
            }
            Self::validate_responses(&behavior.responses, &mut response_ids)?;
            behavior_by_id.insert(behavior.id.clone(), index);
        }
        for handler in &capability_result_behaviors {
            if handler.id.as_str().trim().is_empty() {
                return Err(ConversationCatalogError::EmptyBehaviorId);
            }
            if !behavior_ids.insert(handler.id.as_str().to_string()) {
                return Err(ConversationCatalogError::DuplicateBehaviorId(
                    handler.id.as_str().to_string(),
                ));
            }
            if handler.capability.as_str().trim().is_empty()
                || handler.capability_version.as_str().trim().is_empty()
            {
                return Err(ConversationCatalogError::InvalidCapabilityResultHandler(
                    handler.id.as_str().to_string(),
                ));
            }
            if handler
                .error_code
                .as_ref()
                .is_some_and(|code| code.trim().is_empty())
                || (handler.succeeded == Some(true) && handler.error_code.is_some())
            {
                return Err(ConversationCatalogError::InvalidCapabilityResultHandler(
                    handler.id.as_str().to_string(),
                ));
            }
            Self::validate_responses(&handler.responses, &mut response_ids)?;
        }
        for opening in &openings {
            Self::validate_responses(&opening.responses, &mut response_ids)?;
        }
        for fallback in &fallback_behaviors {
            if fallback.id.as_str().trim().is_empty() {
                return Err(ConversationCatalogError::EmptyBehaviorId);
            }
            if !behavior_ids.insert(fallback.id.as_str().to_string()) {
                return Err(ConversationCatalogError::DuplicateBehaviorId(
                    fallback.id.as_str().to_string(),
                ));
            }
            Self::validate_responses(&fallback.responses, &mut response_ids)?;
        }

        Ok(Self {
            behaviors,
            capability_result_behaviors,
            openings,
            fallback_behaviors,
            style_lexicon: StyleLexicon::default(),
            behavior_by_id,
            default_behavior_by_meaning,
            followup_behavior_by_meaning,
        })
    }

    fn validate_responses(
        responses: &[ResponseDefinition],
        ids: &mut BTreeSet<String>,
    ) -> Result<(), ConversationCatalogError> {
        for response in responses {
            let id = response.id.as_str().trim();
            if id.is_empty() {
                return Err(ConversationCatalogError::EmptyResponseId);
            }
            if !ids.insert(id.to_string()) {
                return Err(ConversationCatalogError::DuplicateResponseId(
                    id.to_string(),
                ));
            }
            Self::validate_localized(id, &response.texts)?;
            for extra in &response.extra_messages {
                if !extra.chance.is_finite() || !(0.0..=1.0).contains(&extra.chance) {
                    return Err(ConversationCatalogError::InvalidChance(id.to_string()));
                }
                Self::validate_localized(id, &extra.texts)?;
            }
            if response
                .opens_followup
                .as_ref()
                .is_some_and(|directive| directive.ttl == 0)
            {
                return Err(ConversationCatalogError::InvalidFollowupTtl(id.to_string()));
            }
            if response.links.iter().any(|link| !safe_http_url(&link.url)) {
                return Err(ConversationCatalogError::UnsafeLink(id.to_string()));
            }
            if response
                .links
                .iter()
                .any(|link| link.label.chars().count() > 120)
            {
                return Err(ConversationCatalogError::LinkLabelTooLong(id.to_string()));
            }
        }
        Ok(())
    }

    fn validate_localized(
        id: &str,
        texts: &[LocalizedTexts],
    ) -> Result<(), ConversationCatalogError> {
        for localized in texts {
            if localized.language.trim().is_empty() {
                return Err(ConversationCatalogError::EmptyLocalizedLanguage(
                    id.to_string(),
                ));
            }
            if localized.variants.is_empty()
                || localized.variants.iter().all(|text| text.trim().is_empty())
            {
                return Err(ConversationCatalogError::EmptyLocalizedVariants(
                    id.to_string(),
                ));
            }
        }
        Ok(())
    }

    #[must_use]
    pub fn with_style_lexicon(mut self, style_lexicon: StyleLexicon) -> Self {
        self.style_lexicon = style_lexicon;
        self
    }

    #[must_use]
    pub fn style_lexicon(&self) -> &StyleLexicon {
        &self.style_lexicon
    }

    #[must_use]
    pub fn behaviors(&self) -> &[ConversationBehavior] {
        &self.behaviors
    }

    #[must_use]
    pub fn capability_result_behaviors(&self) -> &[CapabilityResultBehavior] {
        &self.capability_result_behaviors
    }

    #[must_use]
    pub fn openings(&self) -> &[OpeningDefinition] {
        &self.openings
    }

    #[must_use]
    pub fn fallback_behaviors(&self) -> &[FallbackBehavior] {
        &self.fallback_behaviors
    }

    #[must_use]
    pub fn behavior(&self, behavior: &BehaviorId) -> Option<&ConversationBehavior> {
        self.behavior_by_id
            .get(behavior)
            .and_then(|index| self.behaviors.get(*index))
    }

    #[must_use]
    pub fn behavior_for_meaning(
        &self,
        meaning: &MeaningId,
        followup: Option<&FollowupId>,
    ) -> Option<&ConversationBehavior> {
        let index = if let Some(followup) = followup {
            self.followup_behavior_by_meaning
                .get(&(meaning.clone(), followup.clone()))
        } else {
            self.default_behavior_by_meaning.get(meaning)
        }?;
        self.behaviors.get(*index)
    }

    #[must_use]
    pub fn followup_meanings(&self, followup: &FollowupId) -> BTreeSet<MeaningId> {
        self.behaviors
            .iter()
            .filter(|behavior| behavior.followup_scope.as_ref() == Some(followup))
            .map(|behavior| behavior.meaning.clone())
            .collect()
    }

    #[must_use]
    pub fn normal_meanings(&self, active_topic: Option<&TopicId>) -> BTreeSet<MeaningId> {
        self.behaviors
            .iter()
            .filter(|behavior| behavior.followup_scope.is_none())
            .filter(|behavior| !behavior.topic_scoped || behavior.topic.as_ref() == active_topic)
            .map(|behavior| behavior.meaning.clone())
            .collect()
    }

    #[must_use]
    pub fn topic_meanings(&self, topic: &TopicId) -> BTreeSet<MeaningId> {
        self.behaviors
            .iter()
            .filter(|behavior| behavior.followup_scope.is_none())
            .filter(|behavior| behavior.topic.as_ref() == Some(topic))
            .map(|behavior| behavior.meaning.clone())
            .collect()
    }
}

#[must_use]
pub fn safe_http_url(url: &str) -> bool {
    let trimmed = url.trim();
    if trimmed.is_empty() || trimmed.chars().any(char::is_whitespace) {
        return false;
    }
    let folded = trimmed.to_ascii_lowercase();
    let scheme_len = if folded.starts_with("https://") {
        8
    } else if folded.starts_with("http://") {
        7
    } else {
        return false;
    };
    let rest = &trimmed[scheme_len..];
    let authority = rest
        .split(|ch| matches!(ch, '/' | '?' | '#'))
        .next()
        .unwrap_or_default();
    !authority.is_empty() && authority != "@" && !authority.starts_with(':')
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_behavior(id: &str, meaning: &str, followup: Option<&str>) -> ConversationBehavior {
        ConversationBehavior {
            id: BehaviorId::new(id),
            meaning: MeaningId::new(meaning),
            topic: None,
            topic_scoped: false,
            activates_topic: false,
            topic_ttl: None,
            followup_scope: followup.map(FollowupId::new),
            repair_continuation_candidate: false,
            repeat_same_input_after: None,
            repeat_same_meaning_after: None,
            requires_values: Vec::new(),
            forbidden_values: Vec::new(),
            responses: vec![ResponseDefinition::text(
                format!("{id}.response"),
                "en",
                "ok",
            )],
        }
    }

    #[test]
    fn permits_one_default_and_one_handler_per_meaning_followup_scope() {
        let default = test_behavior("yes.default", "yes", None);
        let confirm = test_behavior("yes.confirm", "yes", Some("confirm"));
        let catalog =
            ConversationCatalog::new(vec![default, confirm], Vec::new(), Vec::new(), Vec::new())
                .expect("disjoint scopes must be deterministic");
        assert_eq!(
            catalog
                .behavior_for_meaning(&MeaningId::new("yes"), None)
                .map(|behavior| behavior.id.as_str()),
            Some("yes.default")
        );
        assert_eq!(
            catalog
                .behavior_for_meaning(&MeaningId::new("yes"), Some(&FollowupId::new("confirm")),)
                .map(|behavior| behavior.id.as_str()),
            Some("yes.confirm")
        );
    }

    #[test]
    fn rejects_two_handlers_for_the_same_meaning_scope() {
        let left = test_behavior("yes.confirm.a", "yes", Some("confirm"));
        let right = test_behavior("yes.confirm.b", "yes", Some("confirm"));
        let error = ConversationCatalog::new(vec![left, right], Vec::new(), Vec::new(), Vec::new())
            .expect_err("overlapping handlers must fail");
        assert_eq!(
            error,
            ConversationCatalogError::DuplicateBehaviorScope {
                meaning: "yes".into(),
                followup: Some("confirm".into()),
            }
        );
    }

    #[test]
    fn rejects_behavior_repeat_threshold_outside_authoring_range() {
        let mut behavior = ConversationBehavior {
            id: BehaviorId::new("repeat.test"),
            meaning: MeaningId::new("repeat.meaning"),
            topic: None,
            topic_scoped: false,
            activates_topic: false,
            topic_ttl: None,
            followup_scope: None,
            repair_continuation_candidate: false,
            repeat_same_input_after: Some(1),
            repeat_same_meaning_after: None,
            requires_values: Vec::new(),
            forbidden_values: Vec::new(),
            responses: vec![ResponseDefinition::text("repeat.response", "en", "ok")],
        };
        let error =
            ConversationCatalog::new(vec![behavior.clone()], Vec::new(), Vec::new(), Vec::new())
                .expect_err("threshold below 2 must fail");
        assert_eq!(
            error,
            ConversationCatalogError::InvalidRepeatThreshold("repeat.test".into())
        );
        behavior.repeat_same_input_after = Some(21);
        assert!(
            ConversationCatalog::new(vec![behavior], Vec::new(), Vec::new(), Vec::new()).is_err()
        );
    }

    #[test]
    fn rejects_non_http_links() {
        let mut response = ResponseDefinition::text("r", "en", "x");
        response.links.push(ResponseLink {
            label: "bad".into(),
            url: "javascript:alert(1)".into(),
        });
        let error = ConversationCatalog::new(
            Vec::new(),
            Vec::new(),
            Vec::new(),
            vec![FallbackBehavior {
                id: BehaviorId::new("fallback.test"),
                trigger: FallbackTrigger::Unresolved,
                priority: 0,
                conditions: Vec::new(),
                responses: vec![response],
            }],
        )
        .expect_err("unsafe link must fail");
        assert_eq!(error, ConversationCatalogError::UnsafeLink("r".to_string()));
    }

    #[test]
    fn rejects_link_labels_over_proven_120_character_limit() {
        let mut response = ResponseDefinition::text("r", "en", "x");
        response.links.push(ResponseLink {
            label: "x".repeat(121),
            url: "https://example.com".into(),
        });
        let error = ConversationCatalog::new(
            Vec::new(),
            Vec::new(),
            Vec::new(),
            vec![FallbackBehavior {
                id: BehaviorId::new("fallback.test"),
                trigger: FallbackTrigger::Unresolved,
                priority: 0,
                conditions: Vec::new(),
                responses: vec![response],
            }],
        )
        .expect_err("oversized label must fail");
        assert_eq!(
            error,
            ConversationCatalogError::LinkLabelTooLong("r".to_string())
        );
    }
}
