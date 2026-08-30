//! Conversation state lifecycle and bounded-history semantics.

use gvya_model::{
    ActiveFollowup, ActiveTopic, BehaviorId, ConversationState, FollowupId, Formality,
    HostReference, MeaningId, ResponseId, TopicId, UserStyle,
};

use super::catalog::{RepeatStage, StyleLexicon};

pub const MAX_MENTIONED_TOPICS: usize = 256;
pub const MAX_HINT_PROGRESS_ENTRIES: usize = 512;
pub const MAX_FOCUS_REFERENCES: usize = 64;
pub const MAX_RECENT_RESPONSE_IDS: usize = 64;
pub const MAX_RECENT_VARIANT_KEYS: usize = 64;
pub const MAX_RECENT_USER_MESSAGES: usize = 50;
pub const MAX_AUTHOR_NUMBER_DEFINITIONS: usize = 256;

#[derive(Clone, Debug, PartialEq)]
pub struct AuthorNumberDefinition {
    pub path: String,
    pub default: f64,
    pub min: f64,
    pub max: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ConversationConfig {
    pub default_topic_ttl: u32,
    pub default_followup_ttl: u32,
    pub recent_response_limit: usize,
    pub recent_variant_limit: usize,
    pub recent_user_window: usize,
    pub repeat_detection_window: usize,
    pub repeat_detection_threshold: u32,
    pub max_messages_per_turn: usize,
    /// Separate deterministic floor for explicitly repair-eligible Behaviors. It never changes normal matching authority.
    pub repair_candidate_min_score: f64,
    /// Bounded numeric author-state definitions with deterministic initialization and clamping.
    pub author_numbers: Vec<AuthorNumberDefinition>,
    /// If the topic-only semantic score is within this margin of the global score, the active
    /// topic may resolve the turn. This makes contextual stickiness explicit rather than hidden.
    pub topic_preference_margin: f64,
}

impl Default for ConversationConfig {
    fn default() -> Self {
        Self {
            default_topic_ttl: 3,
            default_followup_ttl: 2,
            recent_response_limit: 8,
            recent_variant_limit: 8,
            recent_user_window: 8,
            repeat_detection_window: 8,
            repeat_detection_threshold: 4,
            max_messages_per_turn: 6,
            repair_candidate_min_score: 0.40,
            author_numbers: Vec::new(),
            topic_preference_margin: 0.08,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConversationConfigError(pub &'static str);

impl ConversationConfig {
    pub fn validate(&self) -> Result<(), ConversationConfigError> {
        if self.default_topic_ttl == 0 {
            return Err(ConversationConfigError(
                "default_topic_ttl must be positive",
            ));
        }
        if self.default_followup_ttl == 0 {
            return Err(ConversationConfigError(
                "default_followup_ttl must be positive",
            ));
        }
        if !(1..=MAX_RECENT_RESPONSE_IDS).contains(&self.recent_response_limit) {
            return Err(ConversationConfigError(
                "recent_response_limit outside 1..=64",
            ));
        }
        if !(1..=MAX_RECENT_VARIANT_KEYS).contains(&self.recent_variant_limit) {
            return Err(ConversationConfigError(
                "recent_variant_limit outside 1..=64",
            ));
        }
        if !(1..=MAX_RECENT_USER_MESSAGES).contains(&self.recent_user_window) {
            return Err(ConversationConfigError("recent_user_window outside 1..=50"));
        }
        if !(1..=MAX_RECENT_USER_MESSAGES).contains(&self.repeat_detection_window) {
            return Err(ConversationConfigError(
                "repeat_detection_window outside 1..=50",
            ));
        }
        if !(2..=20).contains(&self.repeat_detection_threshold) {
            return Err(ConversationConfigError(
                "repeat_detection_threshold outside 2..=20",
            ));
        }
        if !(1..=6).contains(&self.max_messages_per_turn) {
            return Err(ConversationConfigError(
                "max_messages_per_turn outside 1..=6",
            ));
        }
        if !self.repair_candidate_min_score.is_finite()
            || !(0.0..=1.0).contains(&self.repair_candidate_min_score)
        {
            return Err(ConversationConfigError(
                "repair_candidate_min_score outside 0..=1",
            ));
        }
        if self.author_numbers.len() > MAX_AUTHOR_NUMBER_DEFINITIONS {
            return Err(ConversationConfigError(
                "author_numbers exceeds 256 definitions",
            ));
        }
        let mut paths = std::collections::BTreeSet::new();
        for row in &self.author_numbers {
            let path = row.path.trim();
            let parts = path.split('.').collect::<Vec<_>>();
            if path.is_empty()
                || path.len() > 512
                || parts.len() > 16
                || parts.iter().any(|part| part.is_empty())
            {
                return Err(ConversationConfigError("author_numbers path is invalid"));
            }
            if !paths.insert(path.to_string()) {
                return Err(ConversationConfigError(
                    "author_numbers paths must be unique",
                ));
            }
            if !row.default.is_finite()
                || !row.min.is_finite()
                || !row.max.is_finite()
                || row.min > row.default
                || row.default > row.max
            {
                return Err(ConversationConfigError(
                    "author_numbers bounds/default are invalid",
                ));
            }
        }
        for left in &paths {
            for right in &paths {
                if left != right
                    && (left.starts_with(&format!("{right}."))
                        || right.starts_with(&format!("{left}.")))
                {
                    return Err(ConversationConfigError(
                        "author_numbers paths must not overlap as parent and child",
                    ));
                }
            }
        }
        if !self.topic_preference_margin.is_finite()
            || !(0.0..=0.25).contains(&self.topic_preference_margin)
        {
            return Err(ConversationConfigError(
                "topic_preference_margin outside 0..=0.25",
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct FollowupTurnSnapshot {
    pub at_start: Option<ActiveFollowup>,
    pub consumed: bool,
    pub set_this_turn: bool,
    pub finalized: bool,
    pub expired_id: Option<FollowupId>,
}

impl FollowupTurnSnapshot {
    #[must_use]
    pub fn new(state: &ConversationState) -> Self {
        Self {
            at_start: active_followup(state).cloned(),
            consumed: false,
            set_this_turn: false,
            finalized: false,
            expired_id: None,
        }
    }
}

/// Topic TTL is decremented at the start of each user turn. A TTL of one is therefore no longer
/// active for that turn unless the turn itself refreshes it later.
pub fn tick_topic_at_turn_start(state: &mut ConversationState) {
    let Some(topic) = state.active_topic.as_mut() else {
        return;
    };
    topic.ttl = topic.ttl.saturating_sub(1);
    if topic.ttl == 0 {
        state.active_topic = None;
    }
}

/// Follow-up TTL is finalized after matching. A follow-up with TTL=1 is still eligible on the
/// current turn; if missed, it expires only after that turn.
pub fn finalize_followup_after_matching(
    state: &mut ConversationState,
    snapshot: &mut FollowupTurnSnapshot,
) {
    if snapshot.finalized {
        return;
    }
    snapshot.finalized = true;
    if snapshot.set_this_turn || snapshot.consumed {
        return;
    }
    let Some(start) = &snapshot.at_start else {
        return;
    };
    let Some(active) = state.active_followup.as_mut() else {
        return;
    };
    if active.id != start.id {
        return;
    }
    active.ttl = active.ttl.saturating_sub(1);
    if active.ttl == 0 {
        snapshot.expired_id = Some(active.id.clone());
        state.active_followup = None;
    }
}

pub fn set_active_topic(
    state: &mut ConversationState,
    topic: TopicId,
    source_behavior: Option<BehaviorId>,
    ttl: u32,
) {
    if ttl == 0 {
        return;
    }
    state.active_topic = Some(ActiveTopic {
        id: topic.clone(),
        ttl,
        source_behavior,
    });
    state.last_topic = Some(topic.clone());
    if !state.mentioned_topics.contains(&topic) {
        state.mentioned_topics.push(topic);
        if state.mentioned_topics.len() > MAX_MENTIONED_TOPICS {
            let overflow = state.mentioned_topics.len() - MAX_MENTIONED_TOPICS;
            state.mentioned_topics.drain(0..overflow);
        }
    }
}

pub fn refresh_or_activate_topic(
    state: &mut ConversationState,
    topic: &TopicId,
    source_behavior: Option<BehaviorId>,
    ttl: u32,
) {
    if ttl == 0 {
        return;
    }
    set_active_topic(state, topic.clone(), source_behavior, ttl);
}

pub fn set_active_followup(
    state: &mut ConversationState,
    snapshot: &mut FollowupTurnSnapshot,
    id: FollowupId,
    source_behavior: Option<BehaviorId>,
    ttl: u32,
    refresh_if_same: bool,
) -> bool {
    if ttl == 0 {
        return false;
    }
    if snapshot.expired_id.as_ref() == Some(&id) {
        // Proven floor: a follow-up that expired on this turn is not silently reopened by the
        // same response during that turn.
        return false;
    }
    if let Some(active) = &state.active_followup {
        if active.id == id && !refresh_if_same {
            return false;
        }
    }
    state.active_followup = Some(ActiveFollowup {
        id,
        ttl,
        source_behavior,
    });
    snapshot.set_this_turn = true;
    true
}

pub fn consume_followup(state: &mut ConversationState, snapshot: &mut FollowupTurnSnapshot) {
    if snapshot.at_start.is_some() {
        snapshot.consumed = true;
    }
    state.active_followup = None;
}

#[must_use]
pub fn active_topic(state: &ConversationState) -> Option<&ActiveTopic> {
    state.active_topic.as_ref().filter(|topic| topic.ttl > 0)
}

#[must_use]
pub fn active_followup(state: &ConversationState) -> Option<&ActiveFollowup> {
    state
        .active_followup
        .as_ref()
        .filter(|followup| followup.ttl > 0)
}

pub fn push_recent_response(state: &mut ConversationState, id: ResponseId, limit: usize) {
    state.recent_response_ids.retain(|existing| existing != &id);
    state.recent_response_ids.insert(0, id);
    state.recent_response_ids.truncate(limit.max(1));
}

pub fn push_recent_variant(state: &mut ConversationState, key: String, limit: usize) {
    state
        .recent_variant_keys
        .retain(|existing| existing != &key);
    state.recent_variant_keys.insert(0, key);
    state.recent_variant_keys.truncate(limit.max(1));
}

pub fn push_recent_user_message(state: &mut ConversationState, normalized: String, limit: usize) {
    if normalized.is_empty() {
        return;
    }
    state.recent_user_messages.insert(0, normalized);
    state.recent_user_messages.truncate(limit.max(1));
}

#[must_use]
pub fn global_repeat_count(state: &ConversationState, normalized: &str, window: usize) -> u32 {
    if normalized.is_empty() {
        return 0;
    }
    let prior = state
        .recent_user_messages
        .iter()
        .take(window.saturating_sub(1))
        .filter(|message| message.as_str() == normalized)
        .count();
    u32::try_from(prior.saturating_add(1)).unwrap_or(u32::MAX)
}

#[must_use]
pub fn project_repeat_counts(
    state: &ConversationState,
    normalized: &str,
    meaning: Option<&MeaningId>,
) -> (u32, u32) {
    let same_input =
        if !normalized.is_empty() && state.repeat_memory.last_user_normalized == normalized {
            state.repeat_memory.same_input_count.saturating_add(1)
        } else {
            1
        };
    let same_meaning = if meaning.is_some() && state.repeat_memory.last_meaning.as_ref() == meaning
    {
        state.repeat_memory.same_meaning_count.saturating_add(1)
    } else if meaning.is_some() {
        1
    } else {
        0
    };
    (same_input, same_meaning)
}

#[must_use]
pub fn repeat_preference(same_input: u32, same_meaning: u32) -> Option<RepeatStage> {
    repeat_preference_for_thresholds(same_input, same_meaning, None, None)
}

#[must_use]
pub fn repeat_preference_for_thresholds(
    same_input: u32,
    same_meaning: u32,
    same_input_after: Option<u32>,
    same_meaning_after: Option<u32>,
) -> Option<RepeatStage> {
    fn stage(count: u32, after: u32) -> Option<RepeatStage> {
        if count >= after.saturating_add(2) {
            Some(RepeatStage::Final)
        } else if count >= after.saturating_add(1) {
            Some(RepeatStage::Annoyed)
        } else if count >= after {
            Some(RepeatStage::Repeat)
        } else {
            None
        }
    }
    fn rank(stage: Option<RepeatStage>) -> u8 {
        match stage {
            None => 0,
            Some(RepeatStage::Repeat) => 1,
            Some(RepeatStage::Annoyed) => 2,
            Some(RepeatStage::Final) => 3,
        }
    }
    let input = stage(same_input, same_input_after.unwrap_or(2));
    let meaning = stage(same_meaning, same_meaning_after.unwrap_or(2));
    if rank(input) >= rank(meaning) {
        input
    } else {
        meaning
    }
}

pub fn commit_repeat_memory(
    state: &mut ConversationState,
    normalized: String,
    meaning: Option<MeaningId>,
    same_input_count: u32,
    same_meaning_count: u32,
) {
    state.repeat_memory.last_user_normalized = normalized;
    state.repeat_memory.last_meaning = meaning;
    state.repeat_memory.same_input_count = same_input_count;
    state.repeat_memory.same_meaning_count = same_meaning_count;
}

pub fn update_repair_state(
    state: &mut ConversationState,
    is_repair: bool,
    mode: &str,
    candidate: Option<MeaningId>,
) {
    if is_repair {
        state.repair.consecutive = state.repair.consecutive.saturating_add(1);
        state.repair.last_mode = Some(mode.to_string());
        state.repair.last_candidate = candidate;
    } else {
        state.repair.consecutive = 0;
        state.repair.last_mode = if mode.is_empty() {
            None
        } else {
            Some(mode.to_string())
        };
        state.repair.last_candidate = None;
    }
}

#[must_use]
pub fn repair_stage(state: &ConversationState) -> u32 {
    match state.repair.consecutive {
        0 => 1,
        1 => 2,
        _ => 3,
    }
}

pub fn update_focus(state: &mut ConversationState, references: &[HostReference]) {
    if references.is_empty() {
        return;
    }
    state.focus = references
        .iter()
        .take(MAX_FOCUS_REFERENCES)
        .cloned()
        .collect();
}

#[must_use]
pub fn hint_progress_key(behavior: &BehaviorId, focus: &[HostReference]) -> String {
    let mut key = behavior.as_str().to_string();
    if let Some(reference) = focus.first() {
        key.push('|');
        key.push_str(reference.kind.as_str());
        key.push(':');
        key.push_str(reference.id.as_str());
    }
    key
}

pub fn set_hint_progress(state: &mut ConversationState, key: String, level: u32) {
    if !state.hint_progress.contains_key(&key)
        && state.hint_progress.len() >= MAX_HINT_PROGRESS_ENTRIES
    {
        if let Some(oldest_canonical_key) = state.hint_progress.keys().next().cloned() {
            state.hint_progress.remove(&oldest_canonical_key);
        }
    }
    state.hint_progress.insert(key, level);
}

#[must_use]
pub fn detect_user_style(raw: &str, normalized: &str, lexicon: &StyleLexicon) -> UserStyle {
    let raw_folded = raw.to_lowercase();
    let normalized_folded = normalized.to_lowercase();
    let count_hits = |terms: &[String]| -> u32 {
        terms
            .iter()
            .filter(|term| {
                let term = term.trim().to_lowercase();
                !term.is_empty()
                    && (raw_folded.contains(&term) || normalized_folded.contains(&term))
            })
            .count()
            .try_into()
            .unwrap_or(u32::MAX)
    };
    let formal_hits = count_hits(&lexicon.formal_terms);
    let informal_hits = count_hits(&lexicon.informal_terms);
    if formal_hits > informal_hits && formal_hits > 0 {
        return UserStyle {
            formality: Formality::Formal,
            confidence: (0.5 + f64::from(formal_hits) * 0.15).min(1.0),
        };
    }
    if informal_hits > formal_hits && informal_hits > 0 {
        return UserStyle {
            formality: Formality::Informal,
            confidence: (0.5 + f64::from(informal_hits) * 0.15).min(1.0),
        };
    }
    UserStyle::default()
}

#[must_use]
pub fn active_topic_id(state: &ConversationState) -> Option<&TopicId> {
    active_topic(state).map(|topic| &topic.id)
}

#[cfg(test)]
mod audit_repair_tests {
    use super::*;
    use gvya_model::{ReferenceId, ReferenceKind};

    #[test]
    fn runtime_managed_state_collections_are_bounded() {
        let mut state = ConversationState::default();
        for index in 0..(MAX_MENTIONED_TOPICS + 32) {
            set_active_topic(&mut state, TopicId::new(format!("topic-{index}")), None, 1);
        }
        assert_eq!(state.mentioned_topics.len(), MAX_MENTIONED_TOPICS);
        for index in 0..(MAX_HINT_PROGRESS_ENTRIES + 32) {
            set_hint_progress(&mut state, format!("behavior-{index}"), 1);
        }
        assert_eq!(state.hint_progress.len(), MAX_HINT_PROGRESS_ENTRIES);
        let refs = (0..(MAX_FOCUS_REFERENCES + 32))
            .map(|index| HostReference {
                kind: ReferenceKind::new("item"),
                id: ReferenceId::new(format!("item-{index}")),
            })
            .collect::<Vec<_>>();
        update_focus(&mut state, &refs);
        assert_eq!(state.focus.len(), MAX_FOCUS_REFERENCES);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn topic_ttl_ticks_before_turn_and_expires_at_zero() {
        let mut state = ConversationState::default();
        set_active_topic(&mut state, TopicId::new("door"), None, 2);
        tick_topic_at_turn_start(&mut state);
        assert_eq!(active_topic(&state).map(|topic| topic.ttl), Some(1));
        tick_topic_at_turn_start(&mut state);
        assert!(active_topic(&state).is_none());
    }

    #[test]
    fn followup_ttl_one_remains_eligible_until_after_missed_turn() {
        let mut state = ConversationState::default();
        state.active_followup = Some(ActiveFollowup {
            id: FollowupId::new("yes_no"),
            ttl: 1,
            source_behavior: None,
        });
        let mut snapshot = FollowupTurnSnapshot::new(&state);
        assert_eq!(
            active_followup(&state).map(|followup| followup.ttl),
            Some(1)
        );
        finalize_followup_after_matching(&mut state, &mut snapshot);
        assert!(active_followup(&state).is_none());
        assert_eq!(
            snapshot.expired_id.as_ref().map(FollowupId::as_str),
            Some("yes_no")
        );
    }

    #[test]
    fn same_active_followup_does_not_refresh_by_default() {
        let mut state = ConversationState::default();
        state.active_followup = Some(ActiveFollowup {
            id: FollowupId::new("scope"),
            ttl: 1,
            source_behavior: None,
        });
        let mut snapshot = FollowupTurnSnapshot::new(&state);
        let opened = set_active_followup(
            &mut state,
            &mut snapshot,
            FollowupId::new("scope"),
            None,
            5,
            false,
        );
        assert!(!opened);
        assert_eq!(
            active_followup(&state).map(|followup| followup.ttl),
            Some(1)
        );
    }

    #[test]
    fn global_repeat_count_includes_current_message_and_respects_window() {
        let mut state = ConversationState::default();
        state.recent_user_messages = vec!["hi".into(), "x".into(), "hi".into(), "hi".into()];
        assert_eq!(global_repeat_count(&state, "hi", 4), 3);
    }

    #[test]
    fn repeat_stage_uses_max_of_same_input_and_same_meaning() {
        assert_eq!(repeat_preference(2, 1), Some(RepeatStage::Repeat));
        assert_eq!(repeat_preference(1, 3), Some(RepeatStage::Annoyed));
        assert_eq!(repeat_preference(4, 1), Some(RepeatStage::Final));
    }

    #[test]
    fn author_number_paths_cannot_overlap_as_parent_and_child() {
        let mut config = ConversationConfig::default();
        config.author_numbers = vec![
            AuthorNumberDefinition {
                path: "score".into(),
                default: 0.0,
                min: 0.0,
                max: 10.0,
            },
            AuthorNumberDefinition {
                path: "score.value".into(),
                default: 0.0,
                min: 0.0,
                max: 10.0,
            },
        ];
        assert_eq!(
            config.validate(),
            Err(ConversationConfigError(
                "author_numbers paths must not overlap as parent and child"
            ))
        );
    }

    #[test]
    fn style_detection_preserves_proven_confidence_shape_without_hardcoded_language() {
        let lexicon = StyleLexicon {
            formal_terms: vec!["please".into(), "thank you".into()],
            informal_terms: vec!["dude".into()],
        };
        let style = detect_user_style("Please, thank you", "please thank you", &lexicon);
        assert_eq!(style.formality, Formality::Formal);
        assert!((style.confidence - 0.8).abs() < f64::EPSILON);
    }
}
