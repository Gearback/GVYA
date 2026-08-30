//! Audit report construction, bounded collection and graph helpers.

use super::*;

pub(super) fn finalize_report(mut issues: Vec<AuditIssue>, truncated: bool) -> AuditReport {
    issues.sort_by(|left, right| {
        severity_rank(left.severity)
            .cmp(&severity_rank(right.severity))
            .then_with(|| left.category.cmp(&right.category))
            .then_with(|| left.code.as_str().cmp(right.code.as_str()))
            .then_with(|| location_key(&left.location).cmp(&location_key(&right.location)))
    });
    let mut summary = AuditSummary::default();
    let mut grouped: BTreeMap<(u8, String), Vec<usize>> = BTreeMap::new();
    for (index, row) in issues.iter().enumerate() {
        match row.severity {
            AuditSeverity::Error => summary.errors += 1,
            AuditSeverity::Warning => summary.warnings += 1,
            AuditSeverity::Info => summary.info += 1,
        }
        grouped
            .entry((severity_rank(row.severity), row.category.clone()))
            .or_default()
            .push(index);
    }
    let groups = grouped
        .into_iter()
        .map(|((rank, category), issue_indexes)| AuditGroup {
            severity: rank_severity(rank),
            category,
            issue_indexes,
        })
        .collect();
    AuditReport {
        summary,
        groups,
        issues,
        truncated,
    }
}

pub(super) fn issue(
    code: &str,
    severity: AuditSeverity,
    category: &str,
    summary: &str,
    location: AuditLocation,
) -> AuditIssue {
    AuditIssue {
        code: AuditCode::new(code),
        severity,
        category: category.into(),
        summary: summary.into(),
        location,
        related: Vec::new(),
        remediation: None,
        details: BTreeMap::new(),
    }
}

pub(super) fn push(issues: &mut Vec<AuditIssue>, limits: AuditorLimits, row: AuditIssue) {
    if issues.len() < limits.max_issues {
        issues.push(row);
    }
}

pub(super) fn full(issues: &[AuditIssue], limits: AuditorLimits) -> bool {
    issues.len() >= limits.max_issues
}

pub(super) fn loc(
    package: Option<PackageId>,
    kind: ContributionKind,
    item_id: &str,
) -> AuditLocation {
    AuditLocation {
        package,
        kind: Some(kind),
        item_id: Some(item_id.to_owned()),
        sub_id: None,
        path: None,
    }
}

pub(super) fn related(label: &str, kind: ContributionKind, item_id: &str) -> AuditRelated {
    AuditRelated {
        label: label.into(),
        package: None,
        kind: Some(kind),
        item_id: Some(item_id.to_owned()),
    }
}

pub(super) fn check_identity(
    issues: &mut Vec<AuditIssue>,
    limits: AuditorLimits,
    package: Option<PackageId>,
    kind: ContributionKind,
    contribution_id: &str,
    value_id: &str,
) {
    if contribution_id != value_id {
        let mut row = issue(
            "package.contribution_identity_mismatch",
            AuditSeverity::Error,
            "package",
            "Contribution id differs from the logical id embedded in its value",
            loc(package, kind, contribution_id),
        );
        row.details.insert("value_id".into(), value_id.to_owned());
        push(issues, limits, row);
    }
}

pub(super) fn jaccard(
    left: &str,
    right: &str,
    _profile: &gvya_kernel::semantic::SemanticProfile,
) -> f64 {
    let left_normalized = normalize_text(left);
    let right_normalized = normalize_text(right);
    let left_set: BTreeSet<String> = ordered_tokens(&left_normalized).into_iter().collect();
    let right_set: BTreeSet<String> = ordered_tokens(&right_normalized).into_iter().collect();
    if left_set.is_empty() || right_set.is_empty() {
        return 0.0;
    }
    let intersection = left_set.intersection(&right_set).count();
    let union = left_set.union(&right_set).count();
    if union == 0 {
        0.0
    } else {
        intersection as f64 / union as f64
    }
}

pub(super) fn graph_cycles(graph: &BTreeMap<String, BTreeSet<String>>) -> Vec<Vec<String>> {
    fn walk(
        node: &str,
        graph: &BTreeMap<String, BTreeSet<String>>,
        stack: &mut Vec<String>,
        on_stack: &mut BTreeSet<String>,
        seen_cycles: &mut BTreeSet<String>,
        output: &mut Vec<Vec<String>>,
    ) {
        if on_stack.contains(node) {
            if let Some(position) = stack.iter().position(|value| value == node) {
                let mut cycle = stack[position..].to_vec();
                cycle.push(node.to_owned());
                let signature = canonical_cycle(&cycle);
                if seen_cycles.insert(signature) {
                    output.push(cycle);
                }
            }
            return;
        }
        if stack.len() > 64 {
            return;
        }
        stack.push(node.to_owned());
        on_stack.insert(node.to_owned());
        if let Some(next) = graph.get(node) {
            for child in next {
                walk(child, graph, stack, on_stack, seen_cycles, output);
            }
        }
        on_stack.remove(node);
        stack.pop();
    }

    let mut output = Vec::new();
    let mut seen_cycles = BTreeSet::new();
    for node in graph.keys() {
        walk(
            node,
            graph,
            &mut Vec::new(),
            &mut BTreeSet::new(),
            &mut seen_cycles,
            &mut output,
        );
    }
    output
}

pub(super) fn canonical_cycle(cycle: &[String]) -> String {
    if cycle.len() <= 1 {
        return cycle.join("->");
    }
    let nodes = &cycle[..cycle.len() - 1];
    let mut rotations = Vec::new();
    for start in 0..nodes.len() {
        let mut row = Vec::new();
        for offset in 0..nodes.len() {
            row.push(nodes[(start + offset) % nodes.len()].clone());
        }
        rotations.push(row.join("->"));
    }
    rotations.into_iter().min().unwrap_or_default()
}

pub(super) fn severity_rank(severity: AuditSeverity) -> u8 {
    match severity {
        AuditSeverity::Error => 0,
        AuditSeverity::Warning => 1,
        AuditSeverity::Info => 2,
    }
}

pub(super) fn rank_severity(rank: u8) -> AuditSeverity {
    match rank {
        0 => AuditSeverity::Error,
        1 => AuditSeverity::Warning,
        _ => AuditSeverity::Info,
    }
}

pub(super) fn location_key(location: &AuditLocation) -> String {
    format!(
        "{}:{}:{}:{}:{}",
        location.package.as_ref().map_or("", |value| value.as_str()),
        location.kind.map_or("", |value| value.label()),
        location.item_id.as_deref().unwrap_or(""),
        location.sub_id.as_deref().unwrap_or(""),
        location.path.as_deref().unwrap_or(""),
    )
}
