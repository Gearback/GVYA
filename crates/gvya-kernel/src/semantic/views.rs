//! Deterministic semantic-view construction.

use super::*;

#[must_use]
pub fn build_semantic_views(
    raw: &str,
    profile: &SemanticProfile,
    extra: Option<Vec<SemanticView>>,
) -> SemanticViews {
    let normalized = profile.normalize_text(raw);
    let ordered = ordered_tokens(&normalized);
    let colloquial_tokens = profile.normalize_colloquial_tokens(&ordered);
    let colloquial_text = colloquial_tokens.join(" ");
    let extraction = extract_entities(raw, &normalized, profile);
    let content_tokens = profile.content_tokens(&colloquial_tokens);
    let content_text = content_tokens.join(" ");
    let mut views = vec![SemanticView {
        name: "normalized".into(),
        text: normalized.clone(),
        tokens: ordered,
    }];
    if colloquial_text != normalized && !colloquial_text.is_empty() {
        push_unique_view(
            &mut views,
            "colloquial",
            colloquial_text.clone(),
            colloquial_tokens,
        );
    }
    if extraction.entity_text != normalized && !extraction.entity_text.is_empty() {
        push_unique_view(
            &mut views,
            "entity_text",
            extraction.entity_text.clone(),
            ordered_tokens(&extraction.entity_text),
        );
    }
    if extraction.clean_text != normalized && !extraction.clean_text.is_empty() {
        push_unique_view(
            &mut views,
            "clean_text",
            extraction.clean_text.clone(),
            ordered_tokens(&extraction.clean_text),
        );
    }
    if !content_text.is_empty() && content_text != normalized {
        push_unique_view(&mut views, "content_text", content_text, content_tokens);
    }
    for view in extra.into_iter().flatten() {
        push_unique_view(&mut views, &view.name, view.text, view.tokens);
    }
    SemanticViews {
        normalized,
        entities: extraction.entities,
        views,
    }
}

/// Views whose tokens are glue-stripped content tokens.
///
/// Content-view matching rules must key on this predicate rather than on one literal view
/// name: typo repair produces a second content-derived view, and a rule bounded to content
/// views silently stops applying whenever that repaired view becomes the winning one.
#[must_use]
pub(super) fn is_content_view(name: &str) -> bool {
    matches!(name, "content_text" | "typo_content")
}

pub(super) fn push_unique_view(
    views: &mut Vec<SemanticView>,
    name: &str,
    text: String,
    tokens: Vec<String>,
) {
    if text.trim().is_empty() || views.iter().any(|view| view.text == text) {
        return;
    }
    views.push(SemanticView {
        name: name.to_string(),
        text,
        tokens,
    });
}

#[cfg(test)]
mod tests {
    use super::is_content_view;

    #[test]
    fn every_content_derived_view_is_a_content_view() {
        assert!(is_content_view("content_text"));
        assert!(is_content_view("typo_content"));
    }

    #[test]
    fn views_that_keep_glue_are_not_content_views() {
        for name in [
            "normalized",
            "colloquial",
            "entity_text",
            "clean_text",
            "typo_text",
        ] {
            assert!(
                !is_content_view(name),
                "{name} must keep raw-view semantics"
            );
        }
    }
}
