//! Composition/source-identity audit rules.

use super::*;

pub(super) fn composition_issue(issue: &CompositionIssue) -> AuditIssue {
    AuditIssue {
        code: AuditCode::new(format!("package.{}", issue.code)),
        severity: match issue.severity {
            CompositionSeverity::Error => AuditSeverity::Error,
            CompositionSeverity::Warning => AuditSeverity::Warning,
            CompositionSeverity::Info => AuditSeverity::Info,
        },
        category: "package".into(),
        summary: issue.message.clone(),
        location: AuditLocation {
            package: issue.package.clone(),
            kind: issue.kind,
            item_id: issue.item_id.clone(),
            sub_id: None,
            path: None,
        },
        related: Vec::new(),
        remediation: None,
        details: BTreeMap::new(),
    }
}

pub(super) fn audit_source_identity(
    packages: &[PackageDefinition],
    issues: &mut Vec<AuditIssue>,
    limits: AuditorLimits,
) {
    for package in packages {
        if full(issues, limits) {
            return;
        }
        let owner = Some(package.manifest.id.clone());
        for contribution in &package.contents.meanings {
            check_identity(
                issues,
                limits,
                owner.clone(),
                ContributionKind::Meaning,
                &contribution.id,
                contribution.value.id.as_str(),
            );
            let mut seen = BTreeSet::new();
            if contribution.value.samples.is_empty() && contribution.value.patterns.is_empty() {
                push(
                    issues,
                    limits,
                    issue(
                        "semantic.positive_evidence_missing",
                        AuditSeverity::Error,
                        "semantic",
                        "Meaning has no positive matching evidence",
                        loc(owner.clone(), ContributionKind::Meaning, &contribution.id),
                    ),
                );
            }
            for sample in &contribution.value.samples {
                let normalized = normalize_text(&sample.text);
                if normalized.trim().is_empty() {
                    push(
                        issues,
                        limits,
                        issue(
                            "semantic.sample_empty",
                            AuditSeverity::Error,
                            "semantic",
                            "Meaning contains an empty sample",
                            loc(owner.clone(), ContributionKind::Meaning, &contribution.id),
                        ),
                    );
                } else if !seen.insert(normalized) {
                    push(
                        issues,
                        limits,
                        issue(
                            "semantic.sample_duplicate_local",
                            AuditSeverity::Warning,
                            "semantic",
                            "Meaning repeats the same normalized sample",
                            loc(owner.clone(), ContributionKind::Meaning, &contribution.id),
                        ),
                    );
                }
            }
        }
        for contribution in &package.contents.behaviors {
            check_identity(
                issues,
                limits,
                owner.clone(),
                ContributionKind::Behavior,
                &contribution.id,
                contribution.value.id.as_str(),
            );
        }
        for contribution in &package.contents.capability_result_behaviors {
            check_identity(
                issues,
                limits,
                owner.clone(),
                ContributionKind::CapabilityResultBehavior,
                &contribution.id,
                contribution.value.id.as_str(),
            );
        }
        for contribution in &package.contents.openings {
            check_identity(
                issues,
                limits,
                owner.clone(),
                ContributionKind::Opening,
                &contribution.id,
                contribution.value.id.as_str(),
            );
        }
        for contribution in &package.contents.fallback_behaviors {
            check_identity(
                issues,
                limits,
                owner.clone(),
                ContributionKind::FallbackBehavior,
                &contribution.id,
                contribution.value.id.as_str(),
            );
        }
        for contribution in &package.contents.capabilities {
            check_identity(
                issues,
                limits,
                owner.clone(),
                ContributionKind::Capability,
                &contribution.id,
                contribution.value.contract.id.as_str(),
            );
        }
        for contribution in &package.contents.capability_bindings {
            check_identity(
                issues,
                limits,
                owner.clone(),
                ContributionKind::CapabilityBinding,
                &contribution.id,
                contribution.value.id.as_str(),
            );
        }
        for contribution in &package.contents.capability_policies {
            check_identity(
                issues,
                limits,
                owner.clone(),
                ContributionKind::CapabilityPolicy,
                &contribution.id,
                contribution.value.id.as_str(),
            );
        }
        for contribution in &package.contents.types {
            check_identity(
                issues,
                limits,
                owner.clone(),
                ContributionKind::Type,
                &contribution.id,
                contribution.value.id.as_str(),
            );
        }
        for contribution in &package.contents.assets {
            check_identity(
                issues,
                limits,
                owner.clone(),
                ContributionKind::Asset,
                &contribution.id,
                contribution.value.id.as_str(),
            );
        }
        for contribution in &package.contents.regression_cases {
            check_identity(
                issues,
                limits,
                owner.clone(),
                ContributionKind::RegressionCase,
                &contribution.id,
                contribution.value.id.as_str(),
            );
        }
        for contribution in &package.contents.scenarios {
            check_identity(
                issues,
                limits,
                owner.clone(),
                ContributionKind::Scenario,
                &contribution.id,
                contribution.value.id.as_str(),
            );
        }
    }
}
