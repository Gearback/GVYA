//! Capability audit rules.

use super::*;

pub(super) fn audit_capabilities(
    project: &ComposedProject,
    issues: &mut Vec<AuditIssue>,
    limits: AuditorLimits,
) {
    for catalog_issue in project.capability_catalog.validate() {
        let severity = match catalog_issue.severity {
            IssueSeverity::Error => AuditSeverity::Error,
            IssueSeverity::Warning => AuditSeverity::Warning,
        };
        let mut row = issue(
            &format!("capability.{}", catalog_issue.code),
            severity,
            "capability",
            &catalog_issue.message,
            AuditLocation::project(),
        );
        row.related.push(related(
            "capability subject",
            ContributionKind::Capability,
            &catalog_issue.subject,
        ));
        push(issues, limits, row);
    }
    for handler in project.conversation_catalog.capability_result_behaviors() {
        let Some(definition) = project.capability_catalog.definition(&handler.capability) else {
            let mut row = issue(
                "capability.result_handler_undeclared",
                AuditSeverity::Error,
                "capability",
                "Capability-result handler references a capability absent from the composed catalog",
                AuditLocation::project(),
            );
            row.related.push(related(
                "result handler",
                ContributionKind::CapabilityResultBehavior,
                handler.id.as_str(),
            ));
            row.related.push(related(
                "missing capability",
                ContributionKind::Capability,
                handler.capability.as_str(),
            ));
            push(issues, limits, row);
            continue;
        };
        if definition.contract.version != handler.capability_version {
            let mut row = issue(
                "capability.result_handler_version_mismatch",
                AuditSeverity::Error,
                "capability",
                "Capability-result handler version does not match the composed capability contract",
                AuditLocation::project(),
            );
            row.related.push(related(
                "result handler",
                ContributionKind::CapabilityResultBehavior,
                handler.id.as_str(),
            ));
            row.related.push(related(
                "capability",
                ContributionKind::Capability,
                handler.capability.as_str(),
            ));
            row.details.insert(
                "handler_version".into(),
                handler.capability_version.as_str().to_string(),
            );
            row.details.insert(
                "contract_version".into(),
                definition.contract.version.as_str().to_string(),
            );
            push(issues, limits, row);
        }
    }
}
