//! Contribution application, specialization and patch merging.

use super::*;

pub(super) fn apply_all<T: Clone>(
    namespace: &mut Namespace<T>,
    contributions: &[PackageContribution<T>],
    kind: ContributionKind,
    manifest: &PackageManifest,
    visible_dependencies: &BTreeSet<PackageId>,
    issues: &mut Vec<CompositionIssue>,
) {
    let mut sorted: Vec<&PackageContribution<T>> = contributions.iter().collect();
    sorted.sort_by(|left, right| left.id.cmp(&right.id));
    for contribution in sorted {
        apply_contribution(
            namespace,
            contribution,
            kind,
            manifest,
            visible_dependencies,
            issues,
        );
    }
}

pub(super) fn apply_contribution<T: Clone>(
    namespace: &mut Namespace<T>,
    contribution: &PackageContribution<T>,
    kind: ContributionKind,
    manifest: &PackageManifest,
    visible_dependencies: &BTreeSet<PackageId>,
    issues: &mut Vec<CompositionIssue>,
) {
    let item_id = contribution.id.trim();
    if item_id.is_empty() {
        issues.push(CompositionIssue::error(
            "contribution_id_empty",
            Some(manifest.id.clone()),
            Some(kind),
            None,
            "contribution id cannot be empty",
        ));
        return;
    }
    match &contribution.mode {
        ContributionMode::Add => {
            if let Some(existing) = namespace.items.get(item_id) {
                issues.push(CompositionIssue::error(
                    "contribution_collision", Some(manifest.id.clone()), Some(kind), Some(item_id.to_owned()),
                    format!("item already exists and is owned by {}; use explicit Replace when specialization is intended", existing.provenance.owner.as_str()),
                ));
                return;
            }
            namespace.items.insert(
                item_id.to_owned(),
                ComposedItem {
                    id: item_id.to_owned(),
                    value: contribution.value.clone(),
                    provenance: ContributionProvenance {
                        owner: manifest.id.clone(),
                        exported: contribution.exported,
                        replaced: None,
                    },
                },
            );
        }
        ContributionMode::Replace {
            target_package,
            target_id,
        } => {
            if target_id != item_id {
                issues.push(CompositionIssue::error(
                    "replacement_identity_mismatch",
                    Some(manifest.id.clone()),
                    Some(kind),
                    Some(item_id.to_owned()),
                    "replacement contribution id must equal the exact target item id",
                ));
                return;
            }
            if target_package == &manifest.id {
                issues.push(CompositionIssue::error(
                    "replacement_self_target", Some(manifest.id.clone()), Some(kind), Some(item_id.to_owned()),
                    "a package cannot replace its own contribution through dependency specialization",
                ));
                return;
            }
            if !visible_dependencies.contains(target_package) {
                issues.push(CompositionIssue::error(
                    "replacement_target_not_visible",
                    Some(manifest.id.clone()),
                    Some(kind),
                    Some(item_id.to_owned()),
                    format!(
                        "target package {} is not a direct or re-exported visible dependency",
                        target_package.as_str()
                    ),
                ));
                return;
            }
            let Some(existing) = namespace.items.get(item_id) else {
                issues.push(CompositionIssue::error(
                    "replacement_target_missing",
                    Some(manifest.id.clone()),
                    Some(kind),
                    Some(item_id.to_owned()),
                    "replacement target item does not exist",
                ));
                return;
            };
            if &existing.provenance.owner != target_package {
                issues.push(CompositionIssue::error(
                    "replacement_owner_mismatch",
                    Some(manifest.id.clone()),
                    Some(kind),
                    Some(item_id.to_owned()),
                    format!(
                        "target item is currently owned by {}, not {}",
                        existing.provenance.owner.as_str(),
                        target_package.as_str()
                    ),
                ));
                return;
            }
            if !existing.provenance.exported {
                issues.push(CompositionIssue::error(
                    "replacement_target_private",
                    Some(manifest.id.clone()),
                    Some(kind),
                    Some(item_id.to_owned()),
                    "replacement target is not exported for specialization",
                ));
                return;
            }
            let replaced = Some(existing.provenance.owner.clone());
            namespace.items.insert(
                item_id.to_owned(),
                ComposedItem {
                    id: item_id.to_owned(),
                    value: contribution.value.clone(),
                    provenance: ContributionProvenance {
                        owner: manifest.id.clone(),
                        exported: contribution.exported,
                        replaced,
                    },
                },
            );
        }
    }
}

pub(super) fn compose_style_lexicon<'a>(
    items: impl Iterator<Item = &'a ComposedItem<StyleLexiconPatch>>,
) -> StyleLexicon {
    let mut formal = BTreeSet::new();
    let mut informal = BTreeSet::new();
    for item in items {
        formal.extend(item.value.formal_terms.iter().cloned());
        informal.extend(item.value.informal_terms.iter().cloned());
    }
    StyleLexicon {
        formal_terms: formal.into_iter().collect(),
        informal_terms: informal.into_iter().collect(),
    }
}

pub(super) fn single_setting<'a, T>(
    namespace: &'a Namespace<T>,
    label: &str,
    issues: &mut Vec<CompositionIssue>,
) -> Option<&'a T> {
    if namespace.items.len() > 1 {
        issues.push(CompositionIssue::error(
            "setting_multiple",
            None,
            None,
            None,
            format!("{label} must resolve to at most one composed setting contribution"),
        ));
        return None;
    }
    namespace.items.values().next().map(|item| &item.value)
}

pub(super) fn collect_provenance<T>(
    output: &mut BTreeMap<(ContributionKind, String), ContributionProvenance>,
    kind: ContributionKind,
    namespace: &Namespace<T>,
) {
    for (id, item) in &namespace.items {
        output.insert((kind, id.clone()), item.provenance.clone());
    }
}

pub(super) fn has_errors(issues: &[CompositionIssue]) -> bool {
    issues
        .iter()
        .any(|issue| issue.severity == CompositionSeverity::Error)
}

pub(super) fn valid_digest(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}
