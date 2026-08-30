//! Conversation/response audit rules.

use super::*;

pub(super) fn audit_conversation(
    project: &ComposedProject,
    issues: &mut Vec<AuditIssue>,
    limits: AuditorLimits,
) {
    let known_meanings: BTreeSet<_> = project
        .semantic_catalog
        .patterns()
        .iter()
        .map(|pattern| pattern.id.clone())
        .collect();
    let mut openers: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut consumers: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut followup_edges: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut response_texts: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut response_variants: Vec<(String, String)> = Vec::new();

    for behavior in project.conversation_catalog.behaviors() {
        if !known_meanings.contains(&behavior.meaning) {
            let mut row = issue(
                "conversation.behavior_meaning_missing",
                AuditSeverity::Error,
                "conversation",
                "Behavior references a meaning that is not in the composed semantic catalog",
                AuditLocation::project(),
            );
            row.related.push(related(
                "behavior",
                ContributionKind::Behavior,
                behavior.id.as_str(),
            ));
            row.related.push(related(
                "missing meaning",
                ContributionKind::Meaning,
                behavior.meaning.as_str(),
            ));
            push(issues, limits, row);
        }
        for requirement in behavior
            .requires_values
            .iter()
            .chain(&behavior.forbidden_values)
        {
            if requirement.path.path.trim().is_empty() {
                let mut row = issue(
                    "conversation.behavior_requirement_path_empty",
                    AuditSeverity::Error,
                    "conversation",
                    "Behavior value requirement has an empty path",
                    AuditLocation::project(),
                );
                row.location.sub_id = Some(behavior.id.as_str().to_owned());
                push(issues, limits, row);
            }
        }
        for required in &behavior.requires_values {
            if behavior.forbidden_values.iter().any(|forbidden| {
                forbidden.path == required.path && forbidden.value == required.value
            }) {
                let mut row = issue(
                    "conversation.behavior_requirement_conflict",
                    AuditSeverity::Error,
                    "conversation",
                    "Behavior requires and forbids the same value",
                    AuditLocation::project(),
                );
                row.location.sub_id = Some(behavior.id.as_str().to_owned());
                push(issues, limits, row);
            }
        }
        if let Some(scope) = &behavior.followup_scope {
            consumers
                .entry(scope.as_str().to_owned())
                .or_default()
                .push(behavior.id.as_str().to_owned());
        }
        audit_response_set(
            &behavior.responses,
            issues,
            limits,
            &mut openers,
            &mut response_texts,
            &mut response_variants,
        );
        if let Some(from) = &behavior.followup_scope {
            for response in &behavior.responses {
                if let Some(to) = &response.opens_followup {
                    followup_edges
                        .entry(from.as_str().to_owned())
                        .or_default()
                        .insert(to.id.as_str().to_owned());
                }
            }
        }
    }
    for opening in project.conversation_catalog.openings() {
        audit_response_set(
            &opening.responses,
            issues,
            limits,
            &mut openers,
            &mut response_texts,
            &mut response_variants,
        );
    }
    for fallback in project.conversation_catalog.fallback_behaviors() {
        audit_response_set(
            &fallback.responses,
            issues,
            limits,
            &mut openers,
            &mut response_texts,
            &mut response_variants,
        );
    }

    for (followup, opener_ids) in &openers {
        if !consumers.contains_key(followup) {
            let mut row = issue(
                "conversation.followup_without_consumer",
                AuditSeverity::Warning,
                "conversation",
                "A response opens a follow-up scope that no behavior consumes",
                AuditLocation::project(),
            );
            row.details.insert("followup".into(), followup.clone());
            row.details.insert("openers".into(), opener_ids.join(", "));
            push(issues, limits, row);
        }
    }
    for (followup, consumer_ids) in &consumers {
        if !openers.contains_key(followup) {
            let mut row = issue(
                "conversation.followup_without_opener",
                AuditSeverity::Warning,
                "conversation",
                "A behavior requires a follow-up scope that no response opens",
                AuditLocation::project(),
            );
            row.details.insert("followup".into(), followup.clone());
            row.details
                .insert("consumers".into(), consumer_ids.join(", "));
            push(issues, limits, row);
        }
    }
    for cycle in graph_cycles(&followup_edges) {
        let mut row = issue(
            "conversation.followup_cycle",
            AuditSeverity::Warning,
            "conversation",
            "Follow-up transitions contain a cycle; verify that the loop is intentional and bounded by TTL",
            AuditLocation::project(),
        );
        row.details.insert("cycle".into(), cycle.join(" -> "));
        push(issues, limits, row);
    }

    for (normalized, ids) in response_texts.iter().filter(|(_, ids)| ids.len() > 1) {
        let mut row = issue(
            "conversation.response_text_duplicate",
            AuditSeverity::Info,
            "conversation",
            "The same normalized response text appears in multiple response definitions",
            AuditLocation::project(),
        );
        row.details.insert("text".into(), normalized.clone());
        row.details.insert("responses".into(), ids.join(", "));
        push(issues, limits, row);
    }
    audit_response_near_duplicates(
        &response_variants,
        &SemanticProfile::empty(),
        issues,
        limits,
    );
}

pub(super) fn audit_response_set(
    responses: &[ResponseDefinition],
    issues: &mut Vec<AuditIssue>,
    limits: AuditorLimits,
    openers: &mut BTreeMap<String, Vec<String>>,
    response_texts: &mut BTreeMap<String, Vec<String>>,
    response_variants: &mut Vec<(String, String)>,
) {
    for response in responses {
        if full(issues, limits) {
            return;
        }
        let texts = response
            .texts
            .iter()
            .flat_map(|localized| localized.variants.iter());
        let mut readable = false;
        let mut local_seen = BTreeSet::new();
        for text in texts {
            let trimmed = text.trim();
            if !trimmed.is_empty() {
                readable = true;
            }
            if trimmed.chars().count() > limits.long_response_chars {
                let mut row = issue(
                    "conversation.response_long",
                    AuditSeverity::Warning,
                    "conversation",
                    "Response text is long enough to deserve a deliberate human review",
                    AuditLocation::project(),
                );
                row.location.sub_id = Some(response.id.as_str().to_owned());
                row.details
                    .insert("characters".into(), trimmed.chars().count().to_string());
                push(issues, limits, row);
            }
            let normalized = normalize_text(trimmed);
            if !normalized.is_empty() {
                response_texts
                    .entry(normalized.clone())
                    .or_default()
                    .push(response.id.as_str().to_owned());
                response_variants.push((response.id.as_str().to_owned(), normalized.clone()));
                if !local_seen.insert(normalized) {
                    let mut row = issue(
                        "conversation.response_variant_duplicate",
                        AuditSeverity::Warning,
                        "conversation",
                        "Response repeats the same normalized text variant",
                        AuditLocation::project(),
                    );
                    row.location.sub_id = Some(response.id.as_str().to_owned());
                    push(issues, limits, row);
                }
            }
        }
        if !readable && response.assets.is_empty() && response.links.is_empty() {
            let mut row = issue(
                "conversation.response_no_presentable_content",
                AuditSeverity::Warning,
                "conversation",
                "Response has no readable text, asset or link; verify that a capability-only turn is intentional",
                AuditLocation::project(),
            );
            row.location.sub_id = Some(response.id.as_str().to_owned());
            push(issues, limits, row);
        }
        if let Some(followup) = &response.opens_followup {
            openers
                .entry(followup.id.as_str().to_owned())
                .or_default()
                .push(response.id.as_str().to_owned());
        }
        for condition in &response.conditions {
            if condition.path.path.trim().is_empty() {
                let mut row = issue(
                    "conversation.condition_path_empty",
                    AuditSeverity::Error,
                    "conversation",
                    "Response condition has an empty value path",
                    AuditLocation::project(),
                );
                row.location.sub_id = Some(response.id.as_str().to_owned());
                push(issues, limits, row);
            }
        }
        for effect in &response.effects {
            match effect {
                ConversationEffect::Assign {
                    target: StateTarget::Author(path),
                    ..
                } if path.trim().is_empty() => {
                    let mut row = issue(
                        "conversation.effect_path_empty",
                        AuditSeverity::Error,
                        "conversation",
                        "Author-state assignment has an empty target path",
                        AuditLocation::project(),
                    );
                    row.location.sub_id = Some(response.id.as_str().to_owned());
                    push(issues, limits, row);
                }
                ConversationEffect::Increment {
                    target: StateTarget::Author(path),
                    delta,
                } => {
                    if path.trim().is_empty() || !delta.is_finite() {
                        let mut row = issue(
                            "conversation.effect_increment_invalid",
                            AuditSeverity::Error,
                            "conversation",
                            "Author-state increment requires a non-empty path and finite delta",
                            AuditLocation::project(),
                        );
                        row.location.sub_id = Some(response.id.as_str().to_owned());
                        push(issues, limits, row);
                    }
                }
                _ => {}
            }
        }
    }
}

pub(super) fn audit_response_near_duplicates(
    variants: &[(String, String)],
    profile: &gvya_kernel::semantic::SemanticProfile,
    issues: &mut Vec<AuditIssue>,
    limits: AuditorLimits,
) {
    let threshold = f64::from(limits.near_duplicate_threshold_milli) / 1000.0;
    let mut compared = 0usize;
    let mut bounded = false;
    'pairs: for left_index in 0..variants.len() {
        for right_index in (left_index + 1)..variants.len() {
            if compared >= limits.max_overlap_pairs {
                bounded = true;
                break 'pairs;
            }
            let (left_id, left_text) = &variants[left_index];
            let (right_id, right_text) = &variants[right_index];
            if left_id == right_id || left_text == right_text {
                continue;
            }
            compared += 1;
            let score = jaccard(left_text, right_text, profile);
            if score >= threshold && score < 1.0 {
                let mut row = issue(
                    "conversation.response_text_near_duplicate",
                    AuditSeverity::Info,
                    "conversation",
                    "Two response definitions have strongly overlapping text; verify that the repetition is deliberate",
                    AuditLocation::project(),
                );
                row.details.insert("left_response".into(), left_id.clone());
                row.details
                    .insert("right_response".into(), right_id.clone());
                row.details
                    .insert("similarity".into(), format!("{score:.3}"));
                row.details.insert("left".into(), left_text.clone());
                row.details.insert("right".into(), right_text.clone());
                push(issues, limits, row);
                if full(issues, limits) {
                    return;
                }
            }
        }
    }
    if bounded {
        push(
            issues,
            limits,
            issue(
                "conversation.response_overlap_analysis_bounded",
                AuditSeverity::Info,
                "conversation",
                "Near-duplicate response analysis reached its configured pair limit",
                AuditLocation::project(),
            ),
        );
    }
}
