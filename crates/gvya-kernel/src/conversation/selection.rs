//! Deterministic response eligibility, hint ladders, repeat stages and localized text selection.

use gvya_model::ResponseId;

use super::{
    catalog::{LocalizedTexts, RepeatStage, ResponseDefinition, ResponseKind},
    conditions::{ConditionContext, conditions_match},
    templates::{DeterministicRng, stable_seed},
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum HintRequest {
    None,
    First,
    Next,
    Direct(u32),
    Auto,
}

impl Default for HintRequest {
    fn default() -> Self {
        Self::None
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct LanguagePolicy {
    pub base_fallback: Option<String>,
    pub fallback_order: Vec<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SelectionRequest<'r, 'c> {
    pub responses: &'r [ResponseDefinition],
    pub language: Option<&'c str>,
    pub language_policy: &'c LanguagePolicy,
    pub conditions: ConditionContext<'c>,
    pub recent_response_ids: &'c [ResponseId],
    pub recent_variant_keys: &'c [String],
    pub repeat_preference: Option<RepeatStage>,
    pub hint: HintRequest,
    pub hint_progress: u32,
    pub seed: Option<u64>,
    pub seed_parts: &'c [&'c str],
}

#[derive(Clone, Debug, PartialEq)]
pub struct SelectedResponse<'a> {
    pub response: &'a ResponseDefinition,
    pub language: String,
    pub raw_text: String,
    pub variant_key: String,
    pub selected_hint_level: Option<u32>,
}

#[must_use]
pub fn select_response<'r>(request: SelectionRequest<'r, '_>) -> Option<SelectedResponse<'r>> {
    let mut eligible: Vec<_> = request
        .responses
        .iter()
        .filter(|response| conditions_match(&response.conditions, &request.conditions))
        .filter(|response| has_any_text(response))
        .collect();
    if eligible.is_empty() {
        return None;
    }

    if !matches!(request.hint, HintRequest::None) {
        let level = resolve_hint_pick_level(&eligible, &request.hint, request.hint_progress)?;
        eligible.retain(|response| response.hint_level == Some(level));
    } else if let Some(preferred) = request.repeat_preference {
        eligible = select_repeat_stage(eligible, preferred, request.recent_response_ids);
    } else {
        // Normal turns prefer normal rows. If no normal rows exist, use all eligible rows rather
        // than silently making the behavior unanswerable.
        let normal: Vec<_> = eligible
            .iter()
            .copied()
            .filter(|response| {
                response.kind == ResponseKind::Normal
                    && response.hint_level.unwrap_or(0) == 0
                    && response.repeat_stage.is_none()
            })
            .collect();
        if !normal.is_empty() {
            eligible = normal;
        }
    }
    if eligible.is_empty() {
        return None;
    }

    let non_recent: Vec<_> = eligible
        .iter()
        .copied()
        .filter(|response| !request.recent_response_ids.contains(&response.id))
        .collect();
    let pool = if non_recent.is_empty() {
        eligible
    } else {
        non_recent
    };
    let seed = stable_seed(request.seed_parts, request.seed);
    let mut rng = DeterministicRng::new(seed);
    let response = pool.get(rng.index(pool.len())?)?;
    let language = resolve_language(&response.texts, request.language, request.language_policy)?;
    let variants = texts_for_language(&response.texts, &language);
    let raw_text = choose_text_variant(
        variants,
        response.id.as_str(),
        &language,
        request.recent_variant_keys,
        &mut rng,
    )?;
    let variant_key = variant_key(response.id.as_str(), &language, &raw_text);
    Some(SelectedResponse {
        response,
        language,
        raw_text,
        variant_key,
        selected_hint_level: response.hint_level,
    })
}

#[must_use]
pub fn resolve_hint_pick_level(
    eligible: &[&ResponseDefinition],
    request: &HintRequest,
    progress: u32,
) -> Option<u32> {
    let mut levels: Vec<u32> = eligible
        .iter()
        .filter_map(|response| response.hint_level)
        .filter(|level| *level > 0)
        .collect();
    levels.sort_unstable();
    levels.dedup();
    if levels.is_empty() {
        return None;
    }
    let requested = match request {
        HintRequest::None => return None,
        HintRequest::First => 1,
        HintRequest::Next => progress.saturating_add(1).max(1),
        HintRequest::Direct(level) => (*level).max(1),
        HintRequest::Auto => progress.saturating_add(1).max(1),
    };
    if levels.contains(&requested) {
        return Some(requested);
    }
    levels
        .iter()
        .copied()
        .filter(|level| *level <= requested)
        .max()
        .or_else(|| {
            levels
                .iter()
                .copied()
                .filter(|level| *level > requested)
                .min()
        })
        .or_else(|| levels.last().copied())
}

fn select_repeat_stage<'a>(
    candidates: Vec<&'a ResponseDefinition>,
    preferred: RepeatStage,
    recent: &[ResponseId],
) -> Vec<&'a ResponseDefinition> {
    for stage in repeat_stage_fallback_order(preferred) {
        let of_stage: Vec<_> = candidates
            .iter()
            .copied()
            .filter(|response| response_matches_stage(response, *stage))
            .collect();
        if of_stage.is_empty() {
            continue;
        }
        let non_recent: Vec<_> = of_stage
            .iter()
            .copied()
            .filter(|response| !recent.contains(&response.id))
            .collect();
        if !non_recent.is_empty() {
            return non_recent;
        }
        return of_stage;
    }
    candidates
}

fn repeat_stage_fallback_order(preferred: RepeatStage) -> &'static [Option<RepeatStage>] {
    match preferred {
        RepeatStage::Final => &[
            Some(RepeatStage::Final),
            Some(RepeatStage::Annoyed),
            Some(RepeatStage::Repeat),
            None,
        ],
        RepeatStage::Annoyed => &[Some(RepeatStage::Annoyed), Some(RepeatStage::Repeat), None],
        RepeatStage::Repeat => &[Some(RepeatStage::Repeat), None],
    }
}

fn response_matches_stage(response: &ResponseDefinition, stage: Option<RepeatStage>) -> bool {
    match stage {
        Some(stage) => response.repeat_stage == Some(stage),
        None => response.repeat_stage.is_none() && response.hint_level.unwrap_or(0) == 0,
    }
}

#[must_use]
pub fn resolve_language(
    texts: &[LocalizedTexts],
    requested: Option<&str>,
    policy: &LanguagePolicy,
) -> Option<String> {
    if texts.is_empty() {
        return None;
    }
    let available: Vec<String> = texts
        .iter()
        .filter(|row| row.variants.iter().any(|text| !text.trim().is_empty()))
        .map(|row| normalize_locale(&row.language))
        .collect();
    if available.is_empty() {
        return None;
    }

    let mut candidates = Vec::new();
    if let Some(requested) = requested {
        push_locale_candidates(&mut candidates, requested);
    }
    if let Some(base) = &policy.base_fallback {
        push_locale_candidates(&mut candidates, base);
    }
    for fallback in &policy.fallback_order {
        push_locale_candidates(&mut candidates, fallback);
    }
    for candidate in candidates {
        if available.iter().any(|language| *language == candidate) {
            return Some(candidate);
        }
    }
    None
}

fn push_locale_candidates(out: &mut Vec<String>, raw: &str) {
    let normalized = normalize_locale(raw);
    if normalized.is_empty() {
        return;
    }
    if !out.contains(&normalized) {
        out.push(normalized.clone());
    }
    if let Some((base, _)) = normalized.split_once('-') {
        let base = base.to_string();
        if !out.contains(&base) {
            out.push(base);
        }
    }
}

#[must_use]
pub fn normalize_locale(raw: &str) -> String {
    raw.trim().replace('_', "-").to_ascii_lowercase()
}

/// Validates the bounded BCP 47 shape used at the canonical source/artifact boundary. Runtime
/// host hints remain normalizable, but authored language catalogs must use hyphenated tags.
#[must_use]
pub fn language_tag_is_well_formed(raw: &str) -> bool {
    if raw.is_empty() || raw.len() > 63 || raw.contains('_') {
        return false;
    }
    let mut parts = raw.split('-');
    let Some(primary) = parts.next() else {
        return false;
    };
    if !(1..=8).contains(&primary.len()) || !primary.bytes().all(|byte| byte.is_ascii_alphabetic())
    {
        return false;
    }
    parts.all(|part| {
        (1..=8).contains(&part.len()) && part.bytes().all(|byte| byte.is_ascii_alphanumeric())
    })
}

fn texts_for_language<'a>(rows: &'a [LocalizedTexts], language: &str) -> &'a [String] {
    rows.iter()
        .find(|row| normalize_locale(&row.language) == language)
        .map_or(&[], |row| row.variants.as_slice())
}

fn choose_text_variant(
    variants: &[String],
    response_id: &str,
    language: &str,
    recent_variant_keys: &[String],
    rng: &mut DeterministicRng,
) -> Option<String> {
    let valid: Vec<_> = variants
        .iter()
        .filter(|text| !text.trim().is_empty())
        .collect();
    if valid.is_empty() {
        return None;
    }
    let non_recent: Vec<_> = valid
        .iter()
        .copied()
        .filter(|text| {
            let key = variant_key(response_id, language, text);
            !recent_variant_keys.contains(&key)
        })
        .collect();
    let pool = if non_recent.is_empty() {
        valid
    } else {
        non_recent
    };
    pool.get(rng.index(pool.len())?).map(|text| (*text).clone())
}

fn variant_key(response_id: &str, language: &str, text: &str) -> String {
    format!("{response_id}|{language}|{}", stable_seed(&[text], None))
}

fn has_any_text(response: &ResponseDefinition) -> bool {
    response
        .texts
        .iter()
        .any(|row| row.variants.iter().any(|text| !text.trim().is_empty()))
}

#[must_use]
pub fn response_kind_for_repeat(stage: RepeatStage) -> ResponseKind {
    match stage {
        RepeatStage::Repeat => ResponseKind::Repeat,
        RepeatStage::Annoyed => ResponseKind::AnnoyedRepeat,
        RepeatStage::Final => ResponseKind::FinalRepeat,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use gvya_model::ConversationState;

    use super::*;
    use crate::conversation::conditions::ConditionContext;

    fn conditions<'a>(
        author: &'a BTreeMap<String, gvya_model::Value>,
        state: &'a ConversationState,
        empty: &'a BTreeMap<String, gvya_model::Value>,
    ) -> ConditionContext<'a> {
        ConditionContext {
            author,
            conversation: state,
            host: empty,
            meaning: None,
            system: empty,
            interaction: None,
        }
    }

    #[test]
    fn hint_level_clamps_to_available_floor() {
        let mut a = ResponseDefinition::text("a", "en", "one");
        a.hint_level = Some(1);
        let mut c = ResponseDefinition::text("c", "en", "three");
        c.hint_level = Some(3);
        assert_eq!(
            resolve_hint_pick_level(&[&a, &c], &HintRequest::Direct(2), 0),
            Some(1)
        );
    }

    #[test]
    fn locale_falls_back_from_region_to_base_language() {
        let texts = vec![LocalizedTexts {
            language: "en".to_string(),
            variants: vec!["x".to_string()],
        }];
        assert_eq!(
            resolve_language(&texts, Some("en-US"), &LanguagePolicy::default()),
            Some("en".to_string())
        );
    }

    #[test]
    fn locale_does_not_infer_preference_from_authored_row_order() {
        let texts = vec![
            LocalizedTexts {
                language: "fa".to_owned(),
                variants: vec!["الف".to_owned()],
            },
            LocalizedTexts {
                language: "en".to_owned(),
                variants: vec!["a".to_owned()],
            },
        ];
        assert_eq!(
            resolve_language(&texts, None, &LanguagePolicy::default()),
            None
        );
    }

    #[test]
    fn locale_uses_language_neutral_content_only_when_policy_requests_it() {
        let texts = vec![LocalizedTexts {
            language: "und".to_owned(),
            variants: vec!["neutral".to_owned()],
        }];
        assert_eq!(
            resolve_language(
                &texts,
                Some("fa"),
                &LanguagePolicy {
                    base_fallback: None,
                    fallback_order: vec!["und".to_owned()],
                },
            ),
            Some("und".to_owned())
        );
    }

    #[test]
    fn normal_selection_avoids_recent_response_when_possible() {
        let author = BTreeMap::new();
        let state = ConversationState::default();
        let empty = BTreeMap::new();
        let a = ResponseDefinition::text("a", "en", "a");
        let b = ResponseDefinition::text("b", "en", "b");
        let recent = vec![ResponseId::new("a")];
        let responses = [a, b];
        let language_policy = LanguagePolicy::default();
        let selected = select_response(SelectionRequest {
            responses: &responses,
            language: Some("en"),
            language_policy: &language_policy,
            conditions: conditions(&author, &state, &empty),
            recent_response_ids: &recent,
            recent_variant_keys: &[],
            repeat_preference: None,
            hint: HintRequest::None,
            hint_progress: 0,
            seed: Some(1),
            seed_parts: &["x"],
        })
        .expect("selection");
        assert_eq!(selected.response.id.as_str(), "b");
    }
}
