//! Machine-stable diagnostics for external authoring agents.

use gvya_compiler::{
    audit::AuditReport, package::CompositionIssue, pipeline::BuildError, source::SourceIssue,
};
use gvya_runtime::LoadError;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct AuthoringDiagnostic {
    pub side: &'static str,
    pub stage: &'static str,
    pub code: String,
    pub path: Option<String>,
    pub package_id: Option<String>,
    pub object_kind: Option<String>,
    pub object_id: Option<String>,
    pub message: String,
    pub expected: Option<String>,
    pub actual: Option<String>,
    pub remediation: Option<String>,
}

impl AuthoringDiagnostic {
    pub(super) fn json(&self) -> serde_json::Value {
        serde_json::json!({
            "side": self.side,
            "stage": self.stage,
            "code": self.code,
            "path": self.path,
            "package_id": self.package_id,
            "object_kind": self.object_kind,
            "object_id": self.object_id,
            "message": self.message,
            "expected": self.expected,
            "actual": self.actual,
            "remediation": self.remediation,
        })
    }
}

pub(super) fn generic_diagnostic(
    side: &'static str,
    stage: &'static str,
    code: impl Into<String>,
    message: impl Into<String>,
    remediation: impl Into<String>,
) -> AuthoringDiagnostic {
    AuthoringDiagnostic {
        side,
        stage,
        code: code.into(),
        path: None,
        package_id: None,
        object_kind: None,
        object_id: None,
        message: message.into(),
        expected: None,
        actual: None,
        remediation: Some(remediation.into()),
    }
}

pub(super) fn source_load_diagnostic(
    side: &'static str,
    issue: &crate::SourceLoadDiagnostic,
    remediation: &str,
) -> AuthoringDiagnostic {
    AuthoringDiagnostic {
        side,
        stage: "source_load",
        code: issue.code.to_owned(),
        path: issue.path.clone(),
        package_id: None,
        object_kind: None,
        object_id: None,
        message: issue.message.clone(),
        expected: None,
        actual: None,
        remediation: Some(remediation.to_owned()),
    }
}

pub(super) fn source_diagnostics(
    side: &'static str,
    stage: &'static str,
    issues: &[SourceIssue],
) -> Vec<AuthoringDiagnostic> {
    issues
        .iter()
        .map(|issue| AuthoringDiagnostic {
            side,
            stage,
            code: issue.code.clone(),
            path: issue.path.clone(),
            package_id: None,
            object_kind: None,
            object_id: None,
            message: issue.message.clone(),
            expected: None,
            actual: None,
            remediation: Some(
                "repair the identified source field/file and rerun author-step".into(),
            ),
        })
        .collect()
}

pub(super) fn composition_diagnostics(
    side: &'static str,
    issues: &[CompositionIssue],
) -> Vec<AuthoringDiagnostic> {
    issues
        .iter()
        .map(|issue| AuthoringDiagnostic {
            side,
            stage: "composition",
            code: issue.code.clone(),
            path: None,
            package_id: issue.package.as_ref().map(|id| id.as_str().to_owned()),
            object_kind: issue.kind.map(|kind| kind.label().to_owned()),
            object_id: issue.item_id.clone(),
            message: issue.message.clone(),
            expected: None,
            actual: None,
            remediation: Some("repair the conflicting authored contribution/package relationship and rerun author-step".into()),
        })
        .collect()
}

pub(super) fn diagnostics_for_build(
    side: &'static str,
    error: &BuildError,
) -> Vec<AuthoringDiagnostic> {
    match error {
        BuildError::Source(issues) => source_diagnostics(side, "build", issues),
        BuildError::Composition(issues) => composition_diagnostics(side, issues),
        BuildError::Audit(report) => audit_diagnostics(side, report),
        BuildError::ProgramTooLarge { actual, limit } => vec![AuthoringDiagnostic {
            side,
            stage: "build",
            code: "build.program_too_large".into(),
            path: None,
            package_id: None,
            object_kind: None,
            object_id: None,
            message: "compiled program exceeds the supported size limit".into(),
            expected: Some(format!("<= {limit}")),
            actual: Some(actual.to_string()),
            remediation: Some(
                "reduce authored runtime content before rerunning author-step".into(),
            ),
        }],
        BuildError::AssetBytesMissing { id, digest } => vec![AuthoringDiagnostic {
            side,
            stage: "build",
            code: "build.asset_bytes_missing".into(),
            path: None,
            package_id: None,
            object_kind: Some("asset".into()),
            object_id: Some(id.clone()),
            message: "declared asset bytes are missing".into(),
            expected: Some(digest.clone()),
            actual: None,
            remediation: Some("restore the declared asset file and rerun author-step".into()),
        }],
        BuildError::AssetDigestMismatch {
            id,
            expected,
            actual,
        } => vec![AuthoringDiagnostic {
            side,
            stage: "build",
            code: "build.asset_digest_mismatch".into(),
            path: None,
            package_id: None,
            object_kind: Some("asset".into()),
            object_id: Some(id.clone()),
            message: "asset digest does not match the authored declaration".into(),
            expected: Some(expected.clone()),
            actual: Some(actual.clone()),
            remediation: Some("update the asset declaration or restore the intended bytes".into()),
        }],
        other => vec![generic_diagnostic(
            side,
            "build",
            build_error_code(other),
            format!("{other:?}"),
            "repair the candidate build failure and rerun author-step",
        )],
    }
}

fn build_error_code(error: &BuildError) -> &'static str {
    match error {
        BuildError::Ir(_) => "build.ir",
        BuildError::ProgramLimits(_) => "build.program_limits",
        BuildError::AssetPathCollision(_) => "build.asset_path_collision",
        BuildError::InvalidAssetPath(_) => "build.invalid_asset_path",
        BuildError::Canonical(_) => "build.canonicalization",
        BuildError::Artifact(_) => "build.artifact",
        BuildError::Signing(_) => "build.signing",
        BuildError::InvalidSignatureEnvelope => "build.invalid_signature_envelope",
        BuildError::ArtifactAlreadySigned => "build.artifact_already_signed",
        BuildError::SignatureContentRootMismatch { .. } => "build.signature_content_root_mismatch",
        BuildError::InternalArtifactValidation(_) => "build.internal_artifact_validation",
        BuildError::Source(_)
        | BuildError::Audit(_)
        | BuildError::Composition(_)
        | BuildError::ProgramTooLarge { .. }
        | BuildError::AssetBytesMissing { .. }
        | BuildError::AssetDigestMismatch { .. } => "build.failure",
    }
}

fn audit_diagnostics(side: &'static str, report: &AuditReport) -> Vec<AuthoringDiagnostic> {
    report
        .issues
        .iter()
        .map(|issue| AuthoringDiagnostic {
            side,
            stage: "build_audit",
            code: issue.code.as_str().to_owned(),
            path: issue.location.path.clone(),
            package_id: issue
                .location
                .package
                .as_ref()
                .map(|id| id.as_str().to_owned()),
            object_kind: issue.location.kind.map(|kind| kind.label().to_owned()),
            object_id: issue.location.item_id.clone(),
            message: issue.summary.clone(),
            expected: None,
            actual: None,
            remediation: issue
                .remediation
                .clone()
                .or_else(|| Some("repair the authored audit error and rerun author-step".into())),
        })
        .collect()
}

pub(super) fn diagnostics_for_runtime(
    side: &'static str,
    error: &LoadError,
) -> Vec<AuthoringDiagnostic> {
    vec![generic_diagnostic(
        side,
        "runtime_load",
        match error {
            LoadError::Artifact(_) => "runtime.artifact",
            LoadError::Manifest(_) => "runtime.manifest",
            LoadError::Integrity(_) => "runtime.integrity",
            LoadError::Program(_) => "runtime.program",
            LoadError::DigestMismatch(_) => "runtime.digest_mismatch",
            LoadError::SizeMismatch(_) => "runtime.size_mismatch",
            LoadError::AssetMismatch(_) => "runtime.asset_mismatch",
            LoadError::Signature(_) => "runtime.signature",
            LoadError::SignatureRequired => "runtime.signature_required",
            LoadError::RuntimeLimits(_) => "runtime.limits",
        },
        format!("{error:?}"),
        "repair the candidate artifact/runtime-load failure and rerun author-step",
    )]
}
