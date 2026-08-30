//! Type, localization and asset audit rules.

use super::*;

pub(super) fn audit_types_and_localization(
    project: &ComposedProject,
    issues: &mut Vec<AuditIssue>,
    limits: AuditorLimits,
) {
    for definition in project.types.values() {
        for schema_issue in validate_schema_definition(&definition.schema, SchemaLimits::default())
        {
            let mut row = issue(
                &format!("type.{}", schema_issue.code),
                AuditSeverity::Error,
                "type",
                &schema_issue.message,
                AuditLocation::project(),
            );
            row.related.push(related(
                "type",
                ContributionKind::Type,
                definition.id.as_str(),
            ));
            row.location.path = Some(schema_issue.path);
            push(issues, limits, row);
        }
    }
}

pub(super) fn audit_assets(
    project: &ComposedProject,
    issues: &mut Vec<AuditIssue>,
    limits: AuditorLimits,
) {
    let declared: BTreeSet<_> = project.assets.keys().cloned().collect();
    let mut inspect = |response: &ResponseDefinition| {
        for asset in &response.assets {
            if !declared.contains(&asset.asset_id) {
                let mut row = issue(
                    "asset.reference_undeclared",
                    AuditSeverity::Error,
                    "asset",
                    "Response references an asset that is not declared by the composed package graph",
                    AuditLocation::project(),
                );
                row.location.sub_id = Some(response.id.as_str().to_owned());
                row.related.push(related(
                    "missing asset",
                    ContributionKind::Asset,
                    asset.asset_id.as_str(),
                ));
                push(issues, limits, row);
            }
        }
    };
    for behavior in project.conversation_catalog.behaviors() {
        for response in &behavior.responses {
            inspect(response);
        }
    }
    for handler in project.conversation_catalog.capability_result_behaviors() {
        for response in &handler.responses {
            inspect(response);
        }
    }
    for opening in project.conversation_catalog.openings() {
        for response in &opening.responses {
            inspect(response);
        }
    }
    for fallback in project.conversation_catalog.fallback_behaviors() {
        for response in &fallback.responses {
            inspect(response);
        }
    }

    drop(inspect);
    for asset in project.assets.values() {
        if asset.media_type.trim().is_empty() || asset.logical_path.trim().is_empty() {
            let mut row = issue(
                "asset.metadata_incomplete",
                AuditSeverity::Error,
                "asset",
                "Asset requires non-empty media type and logical path",
                AuditLocation::project(),
            );
            row.related
                .push(related("asset", ContributionKind::Asset, asset.id.as_str()));
            push(issues, limits, row);
        }
        if asset.digest.as_str().len() != 64
            || !asset
                .digest
                .as_str()
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit())
        {
            let mut row = issue(
                "asset.digest_invalid",
                AuditSeverity::Error,
                "asset",
                "Asset digest must be exactly 64 hexadecimal characters",
                AuditLocation::project(),
            );
            row.related
                .push(related("asset", ContributionKind::Asset, asset.id.as_str()));
            push(issues, limits, row);
        }
        if asset.logical_path.starts_with('/')
            || asset.logical_path.split('/').any(|part| part == "..")
        {
            let mut row = issue(
                "asset.path_unsafe",
                AuditSeverity::Error,
                "asset",
                "Asset logical path must be relative and cannot traverse parent directories",
                AuditLocation::project(),
            );
            row.related
                .push(related("asset", ContributionKind::Asset, asset.id.as_str()));
            push(issues, limits, row);
        }
    }
}
