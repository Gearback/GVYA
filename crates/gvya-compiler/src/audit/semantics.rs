//! Semantic catalog audit rules.

use super::*;

pub(super) fn audit_semantics(
    project: &ComposedProject,
    issues: &mut Vec<AuditIssue>,
    limits: AuditorLimits,
) {
    let patterns = project.semantic_catalog.patterns();
    let mut exact_patterns: BTreeMap<(String, String, i32, i32), BTreeSet<&str>> = BTreeMap::new();
    for pattern in patterns {
        for rule in &pattern.patterns {
            let language = normalize_language_tag(&rule.language);
            let Some(profile) =
                profile_for_authored_language(&project.semantic_profiles, &rule.language)
            else {
                continue;
            };
            let normalized = profile.normalize_text(&rule.text);
            if !normalized.is_empty() {
                exact_patterns
                    .entry((language, normalized, rule.priority, pattern.priority))
                    .or_default()
                    .insert(pattern.id.as_str());
            }
        }
    }
    for ((language, rule, priority, meaning_priority), owners) in
        exact_patterns.iter().filter(|(_, owners)| owners.len() > 1)
    {
        if full(issues, limits) {
            return;
        }
        let mut row = issue(
            "semantic.structural_pattern_duplicate_cross_meaning",
            AuditSeverity::Error,
            "semantic",
            "The same structural pattern belongs to more than one Meaning at equal priority",
            AuditLocation::project(),
        );
        row.details.insert("language".into(), language.clone());
        row.details.insert("pattern".into(), rule.clone());
        row.details
            .insert("pattern_priority".into(), priority.to_string());
        row.details
            .insert("meaning_priority".into(), meaning_priority.to_string());
        row.details.insert(
            "meanings".into(),
            owners.iter().copied().collect::<Vec<_>>().join(", "),
        );
        push(issues, limits, row);
    }

    for pattern in patterns {
        for rule in &pattern.patterns {
            let Ok(set_names) = gvya_kernel::semantic::structural_pattern_set_names(&rule.text)
            else {
                continue;
            };
            for set_name in set_names {
                if profile_for_authored_language(&project.semantic_profiles, &rule.language)
                    .is_some_and(|profile| {
                        profile.pattern_sets.contains_key(&set_name)
                            || set_name
                                .strip_prefix("entity.")
                                .is_some_and(|kind| profile.custom_entities.contains_key(kind))
                    })
                {
                    continue;
                }
                if full(issues, limits) {
                    return;
                }
                let mut row = issue(
                    "semantic.structural_pattern_set_missing",
                    AuditSeverity::Error,
                    "semantic",
                    "Structural pattern references an unknown Matcher Profile set",
                    AuditLocation::project(),
                );
                row.details
                    .insert("meaning".into(), pattern.id.as_str().to_string());
                row.details.insert("pattern".into(), rule.text.clone());
                row.details.insert("set".into(), set_name);
                push(issues, limits, row);
            }
        }
    }

    if let Err(error) =
        gvya_kernel::semantic::validate_structural_matcher(patterns, &project.semantic_profiles)
    {
        if !matches!(
            error,
            gvya_kernel::semantic::StructuralMatcherBuildError::UnknownSet { .. }
        ) && !full(issues, limits)
        {
            let mut row = issue(
                "semantic.structural_matcher_invalid",
                AuditSeverity::Error,
                "semantic",
                "Structural matcher cannot be built from the composed Meaning catalog and Matcher Profile",
                AuditLocation::project(),
            );
            row.details.insert("reason".into(), format!("{error:?}"));
            push(issues, limits, row);
        }
    }

    let mut exact: BTreeMap<(String, String), BTreeSet<&str>> = BTreeMap::new();
    for pattern in patterns {
        for sample in &pattern.samples {
            let language = normalize_language_tag(&sample.language);
            let Some(profile) =
                profile_for_authored_language(&project.semantic_profiles, &sample.language)
            else {
                continue;
            };
            let normalized = profile.normalize_text(&sample.text);
            if !normalized.is_empty() {
                exact
                    .entry((language, normalized))
                    .or_default()
                    .insert(pattern.id.as_str());
            }
        }
    }
    for ((language, sample), owners) in exact.iter().filter(|(_, owners)| owners.len() > 1) {
        if full(issues, limits) {
            return;
        }
        let mut row = issue(
            "semantic.sample_duplicate_cross_meaning",
            AuditSeverity::Error,
            "semantic",
            "The same normalized positive sample belongs to more than one meaning",
            AuditLocation::project(),
        );
        row.details.insert("language".into(), language.clone());
        row.details.insert("sample".into(), sample.clone());
        row.details.insert(
            "meanings".into(),
            owners.iter().copied().collect::<Vec<_>>().join(", "),
        );
        push(issues, limits, row);
    }

    let threshold = f64::from(limits.near_duplicate_threshold_milli) / 1000.0;
    let mut compared = 0usize;
    'outer: for left_index in 0..patterns.len() {
        for right_index in (left_index + 1)..patterns.len() {
            for left_sample in &patterns[left_index].samples {
                for right_sample in &patterns[right_index].samples {
                    if normalize_language_tag(&left_sample.language)
                        != normalize_language_tag(&right_sample.language)
                    {
                        continue;
                    }
                    if compared >= limits.max_overlap_pairs {
                        break 'outer;
                    }
                    let Some(profile) = profile_for_authored_language(
                        &project.semantic_profiles,
                        &left_sample.language,
                    ) else {
                        continue;
                    };
                    compared += 1;
                    let score = jaccard(&left_sample.text, &right_sample.text, profile);
                    if score >= threshold && score < 1.0 {
                        let mut row = issue(
                            "semantic.sample_near_overlap",
                            AuditSeverity::Warning,
                            "semantic",
                            "Two meanings have strongly overlapping positive samples",
                            AuditLocation::project(),
                        );
                        row.related.push(related(
                            "left meaning",
                            ContributionKind::Meaning,
                            patterns[left_index].id.as_str(),
                        ));
                        row.related.push(related(
                            "right meaning",
                            ContributionKind::Meaning,
                            patterns[right_index].id.as_str(),
                        ));
                        row.details
                            .insert("similarity".into(), format!("{score:.3}"));
                        row.details.insert("left".into(), left_sample.text.clone());
                        row.details
                            .insert("right".into(), right_sample.text.clone());
                        push(issues, limits, row);
                    }
                }
            }
        }
    }
    if compared >= limits.max_overlap_pairs {
        push(
            issues,
            limits,
            issue(
                "semantic.overlap_analysis_bounded",
                AuditSeverity::Info,
                "semantic",
                "Near-overlap analysis reached its configured pair limit",
                AuditLocation::project(),
            ),
        );
    }
}
