//! End-to-end deterministic source -> package -> audit -> IR -> `.gvya` build pipeline.

use std::collections::{BTreeMap, BTreeSet};

use gvya_model::{
    GVYA_PROGRAM_MAX_BYTES, GVYA_PROGRAM_MAX_COLLECTION_ENTRIES, GVYA_PROGRAM_MAX_DEPTH,
    GVYA_PROGRAM_MAX_NODES, GVYA_PROGRAM_MAX_STRING_BYTES,
};
use serde_json::Value as JsonValue;

use crate::{
    artifact::{
        ArtifactEntry, ArtifactError, ArtifactLimits, EntryKind, build_artifact, parse_artifact,
    },
    audit::{AuditReport, Auditor, AuditorLimits},
    canonical::{canonical_json, sha256_hex},
    ir::{CompileIdentity, CompiledIr, IrError, compile_ir},
    package::{CompositionIssue, compose_packages},
    source::{
        ResolvedSourceProject, SourceIssue, SourceLimits, SourceTree, resolve_source_project,
        safe_asset_logical_path,
    },
};

pub const ARTIFACT_FORMAT: &str = "gvya.artifact";
pub const ARTIFACT_VERSION: u32 = 1;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BuildOptions {
    pub source_limits: SourceLimits,
    pub artifact_limits: ArtifactLimits,
    pub auditor_limits: AuditorLimits,
}

impl Default for BuildOptions {
    fn default() -> Self {
        Self {
            source_limits: SourceLimits::default(),
            artifact_limits: ArtifactLimits::default(),
            auditor_limits: AuditorLimits::default(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SignatureEnvelope {
    pub algorithm: String,
    pub key_id: String,
    /// Opaque transport-safe signature representation selected by the signer, commonly base64url.
    pub signature: String,
}

pub trait ArtifactSigner {
    type Error: std::fmt::Display;
    fn sign_content_root(&self, content_root: [u8; 32]) -> Result<SignatureEnvelope, Self::Error>;
}

#[derive(Clone, Debug, PartialEq)]
pub enum BuildError {
    Source(Vec<SourceIssue>),
    Audit(AuditReport),
    Composition(Vec<CompositionIssue>),
    Ir(IrError),
    ProgramTooLarge {
        actual: usize,
        limit: usize,
    },
    ProgramLimits(String),
    AssetBytesMissing {
        id: String,
        digest: String,
    },
    AssetDigestMismatch {
        id: String,
        expected: String,
        actual: String,
    },
    AssetPathCollision(String),
    InvalidAssetPath(String),
    Canonical(String),
    Artifact(ArtifactError),
    Signing(String),
    InvalidSignatureEnvelope,
    ArtifactAlreadySigned,
    SignatureContentRootMismatch {
        expected: String,
        actual: String,
    },
    InternalArtifactValidation(ArtifactError),
}

#[derive(Clone, Debug, PartialEq)]
pub struct BuildResult {
    pub artifact: Vec<u8>,
    pub artifact_digest: String,
    pub content_root: String,
    pub program_digest: String,
    pub manifest: JsonValue,
    pub integrity: JsonValue,
    pub audit: AuditReport,
}

pub fn build_source_project(
    tree: &SourceTree,
    options: BuildOptions,
    signer: Option<&dyn ArtifactSigner<Error = String>>,
) -> Result<BuildResult, BuildError> {
    let resolved =
        resolve_source_project(tree, options.source_limits).map_err(BuildError::Source)?;
    build_resolved_project(resolved, options, signer)
}

pub(crate) fn build_resolved_project(
    resolved: ResolvedSourceProject,
    options: BuildOptions,
    signer: Option<&dyn ArtifactSigner<Error = String>>,
) -> Result<BuildResult, BuildError> {
    let auditor = Auditor::new(options.auditor_limits);
    let audit = auditor.audit(&resolved.packages, &resolved.semantic_profiles);
    if !audit.is_clean() {
        return Err(BuildError::Audit(audit));
    }
    let composition = compose_packages(&resolved.packages, &resolved.semantic_profiles);
    let Some(project) = composition.project else {
        return Err(BuildError::Composition(composition.issues));
    };

    let identity = CompileIdentity {
        project_id: resolved.project.project_id.clone(),
        brain_id: resolved.project.brain_id.clone(),
        enabled_languages: resolved.project.enabled_languages.clone(),
        default_language: resolved.project.default_language.clone(),
        semantic_config: resolved.project.semantic_config.clone(),
        conversation_config: resolved.project.conversation_config.clone(),
        source_packages: resolved.source_package_digests.clone(),
    };
    let ir = compile_ir(&project, &identity).map_err(BuildError::Ir)?;
    if ir.bytes.len() > GVYA_PROGRAM_MAX_BYTES {
        return Err(BuildError::ProgramTooLarge {
            actual: ir.bytes.len(),
            limit: GVYA_PROGRAM_MAX_BYTES,
        });
    }
    validate_compiled_program_shape(&ir.bytes)?;
    let asset_entries = resolve_asset_entries(&project, &resolved)?;
    let integrity = build_integrity(&ir, &asset_entries, &resolved)?;
    let integrity_bytes = canonical_json(&integrity)
        .map_err(|error| BuildError::Canonical(format!("integrity: {error:?}")))?;
    let manifest = build_manifest(&ir, &integrity_bytes, &resolved, &project, &asset_entries);
    let manifest_bytes = canonical_json(&manifest)
        .map_err(|error| BuildError::Canonical(format!("manifest: {error:?}")))?;

    let mut entries = vec![
        ArtifactEntry {
            kind: EntryKind::Manifest,
            path: "manifest.json".into(),
            essential: true,
            bytes: manifest_bytes,
        },
        ArtifactEntry {
            kind: EntryKind::Program,
            path: "program.json".into(),
            essential: true,
            bytes: ir.bytes.clone(),
        },
        ArtifactEntry {
            kind: EntryKind::Integrity,
            path: "integrity.json".into(),
            essential: true,
            bytes: integrity_bytes,
        },
    ];
    entries.extend(asset_entries.iter().cloned());
    if resolved.project.emit_debug_map {
        entries.push(ArtifactEntry {
            kind: EntryKind::DebugMap,
            path: "debug/source-map.json".into(),
            essential: false,
            bytes: debug_map(&project, &resolved)?,
        });
    }

    let unsigned =
        build_artifact(entries.clone(), options.artifact_limits).map_err(BuildError::Artifact)?;
    let parsed_unsigned = parse_artifact(&unsigned, options.artifact_limits)
        .map_err(BuildError::InternalArtifactValidation)?;
    let root = parsed_unsigned.content_root();
    if let Some(signer) = signer {
        let envelope = signer
            .sign_content_root(root)
            .map_err(|error| BuildError::Signing(error.to_string()))?;
        validate_signature_envelope(&envelope)?;
        let signature = serde_json::json!({
            "format": "gvya.signature",
            "version": 1,
            "content_root": crate::canonical::hex(&root),
            "algorithm": envelope.algorithm,
            "key_id": envelope.key_id,
            "signature": envelope.signature,
        });
        entries.push(ArtifactEntry {
            kind: EntryKind::Signature,
            path: "signature.json".into(),
            essential: false,
            bytes: canonical_json(&signature)
                .map_err(|error| BuildError::Canonical(format!("signature: {error:?}")))?,
        });
    }
    let artifact =
        build_artifact(entries, options.artifact_limits).map_err(BuildError::Artifact)?;
    let parsed = parse_artifact(&artifact, options.artifact_limits)
        .map_err(BuildError::InternalArtifactValidation)?;
    if parsed.content_root() != root {
        return Err(BuildError::InternalArtifactValidation(
            ArtifactError::TableNotCanonical,
        ));
    }
    Ok(BuildResult {
        artifact_digest: sha256_hex(&artifact),
        artifact,
        content_root: crate::canonical::hex(&root),
        program_digest: ir.digest_hex,
        manifest,
        integrity,
        audit,
    })
}

/// Returns the deterministic content root that an external signing authority must sign.
pub fn artifact_signing_content_root(
    bytes: &[u8],
    limits: ArtifactLimits,
) -> Result<String, BuildError> {
    let parsed = parse_artifact(bytes, limits).map_err(BuildError::Artifact)?;
    Ok(crate::canonical::hex(&parsed.content_root()))
}

/// Attaches a host-produced signature envelope to an existing unsigned `.gvya` artifact.
///
/// The external signer must sign the exact content root returned by
/// [`artifact_signing_content_root`]. This function never handles private keys and refuses to
/// replace an existing signature envelope.
pub fn attach_signature_envelope(
    unsigned_artifact: &[u8],
    expected_content_root: &str,
    envelope: &SignatureEnvelope,
    limits: ArtifactLimits,
) -> Result<Vec<u8>, BuildError> {
    validate_signature_envelope(envelope)?;
    let parsed = parse_artifact(unsigned_artifact, limits).map_err(BuildError::Artifact)?;
    if parsed
        .entries()
        .iter()
        .any(|entry| entry.kind == EntryKind::Signature)
    {
        return Err(BuildError::ArtifactAlreadySigned);
    }
    let root = parsed.content_root();
    let actual = crate::canonical::hex(&root);
    if expected_content_root != actual {
        return Err(BuildError::SignatureContentRootMismatch {
            expected: expected_content_root.to_owned(),
            actual,
        });
    }

    let mut entries = Vec::with_capacity(parsed.entries().len() + 1);
    for info in parsed.entries() {
        let bytes = parsed.entry(&info.path).ok_or_else(|| {
            BuildError::InternalArtifactValidation(ArtifactError::EntryOutOfBounds(
                info.path.clone(),
            ))
        })?;
        entries.push(ArtifactEntry {
            kind: info.kind,
            path: info.path.clone(),
            essential: info.essential,
            bytes: bytes.to_vec(),
        });
    }
    let signature = serde_json::json!({
        "format": "gvya.signature",
        "version": 1,
        "content_root": actual,
        "algorithm": envelope.algorithm.as_str(),
        "key_id": envelope.key_id.as_str(),
        "signature": envelope.signature.as_str(),
    });
    entries.push(ArtifactEntry {
        kind: EntryKind::Signature,
        path: "signature.json".into(),
        essential: false,
        bytes: canonical_json(&signature)
            .map_err(|error| BuildError::Canonical(format!("signature: {error:?}")))?,
    });
    let artifact = build_artifact(entries, limits).map_err(BuildError::Artifact)?;
    let reparsed =
        parse_artifact(&artifact, limits).map_err(BuildError::InternalArtifactValidation)?;
    if reparsed.content_root() != root {
        return Err(BuildError::InternalArtifactValidation(
            ArtifactError::TableNotCanonical,
        ));
    }
    Ok(artifact)
}

fn validate_compiled_program_shape(bytes: &[u8]) -> Result<(), BuildError> {
    let value: JsonValue = serde_json::from_slice(bytes)
        .map_err(|error| BuildError::ProgramLimits(format!("compiled program JSON: {error}")))?;
    let mut nodes = 0usize;
    fn walk(value: &JsonValue, depth: usize, nodes: &mut usize) -> Result<(), String> {
        if depth > GVYA_PROGRAM_MAX_DEPTH {
            return Err("compiled program exceeds canonical depth limit".into());
        }
        *nodes = nodes.saturating_add(1);
        if *nodes > GVYA_PROGRAM_MAX_NODES {
            return Err("compiled program exceeds canonical node limit".into());
        }
        match value {
            JsonValue::Null | JsonValue::Bool(_) | JsonValue::Number(_) => Ok(()),
            JsonValue::String(value) => {
                if value.len() <= GVYA_PROGRAM_MAX_STRING_BYTES {
                    Ok(())
                } else {
                    Err("compiled program string exceeds canonical limit".into())
                }
            }
            JsonValue::Array(values) => {
                if values.len() > GVYA_PROGRAM_MAX_COLLECTION_ENTRIES {
                    return Err("compiled program array exceeds canonical entry limit".into());
                }
                for value in values {
                    walk(value, depth + 1, nodes)?;
                }
                Ok(())
            }
            JsonValue::Object(values) => {
                if values.len() > GVYA_PROGRAM_MAX_COLLECTION_ENTRIES {
                    return Err("compiled program object exceeds canonical entry limit".into());
                }
                for (key, value) in values {
                    if key.len() > GVYA_PROGRAM_MAX_STRING_BYTES {
                        return Err(
                            "compiled program object key exceeds canonical string limit".into()
                        );
                    }
                    walk(value, depth + 1, nodes)?;
                }
                Ok(())
            }
        }
    }
    walk(&value, 0, &mut nodes).map_err(BuildError::ProgramLimits)
}

fn resolve_asset_entries(
    project: &crate::package::ComposedProject,
    resolved: &ResolvedSourceProject,
) -> Result<Vec<ArtifactEntry>, BuildError> {
    let mut paths = BTreeSet::new();
    let mut entries = Vec::new();
    for asset in project.assets.values() {
        if !safe_asset_logical_path(&asset.logical_path) {
            return Err(BuildError::InvalidAssetPath(asset.logical_path.clone()));
        }
        if !paths.insert(asset.logical_path.clone()) {
            return Err(BuildError::AssetPathCollision(asset.logical_path.clone()));
        }
        let expected = asset.digest.as_str();
        let Some(bytes) = resolved.asset_bytes_by_digest.get(expected) else {
            return Err(BuildError::AssetBytesMissing {
                id: asset.id.as_str().to_owned(),
                digest: expected.to_owned(),
            });
        };
        let actual = sha256_hex(bytes);
        if actual != expected {
            return Err(BuildError::AssetDigestMismatch {
                id: asset.id.as_str().to_owned(),
                expected: expected.to_owned(),
                actual,
            });
        }
        entries.push(ArtifactEntry {
            kind: EntryKind::Asset,
            path: asset.logical_path.clone(),
            essential: true,
            bytes: bytes.clone(),
        });
    }
    entries.sort_by(|left, right| left.path.cmp(&right.path));
    Ok(entries)
}

fn build_integrity(
    ir: &CompiledIr,
    assets: &[ArtifactEntry],
    resolved: &ResolvedSourceProject,
) -> Result<JsonValue, BuildError> {
    let asset_rows = assets
        .iter()
        .map(|entry| {
            serde_json::json!({
                "path": entry.path,
                "sha256": sha256_hex(&entry.bytes),
                "size": entry.bytes.len(),
            })
        })
        .collect::<Vec<_>>();
    let packages = resolved
        .source_package_digests
        .iter()
        .map(|(id, digest)| serde_json::json!({"id": id, "sha256": digest}))
        .collect::<Vec<_>>();
    Ok(serde_json::json!({
        "format": "gvya.integrity",
        "version": 1,
        "program": {"path": "program.json", "sha256": ir.digest_hex, "size": ir.bytes.len()},
        "assets": asset_rows,
        "source_packages": packages,
    }))
}

fn build_manifest(
    ir: &CompiledIr,
    integrity_bytes: &[u8],
    resolved: &ResolvedSourceProject,
    project: &crate::package::ComposedProject,
    assets: &[ArtifactEntry],
) -> JsonValue {
    let packages = resolved.packages.iter().map(|package| serde_json::json!({
        "id": package.manifest.id.as_str(), "source_sha256": package.manifest.digest.as_str()
    })).collect::<Vec<_>>();
    let asset_rows = project_asset_manifest_rows(project, assets);
    serde_json::json!({
        "format": ARTIFACT_FORMAT,
        "version": ARTIFACT_VERSION,
        "container_version": crate::artifact::CONTAINER_VERSION,
        "project_id": resolved.project.project_id,
        "brain_id": resolved.project.brain_id,
        "program": {"path": "program.json", "format": crate::ir::PROGRAM_FORMAT, "version": crate::ir::PROGRAM_VERSION, "sha256": ir.digest_hex, "size": ir.bytes.len()},
        "integrity": {"path": "integrity.json", "sha256": sha256_hex(integrity_bytes)},
        "packages": packages,
        "assets": asset_rows,
        "debug_map": if resolved.project.emit_debug_map { JsonValue::String("debug/source-map.json".into()) } else { JsonValue::Null },
        "signing": {"content_root_algorithm": "sha256-essential-entry-set-v1", "envelope_path": "signature.json"},
    })
}

fn project_asset_manifest_rows(
    project: &crate::package::ComposedProject,
    assets: &[ArtifactEntry],
) -> Vec<JsonValue> {
    let sizes: BTreeMap<&str, usize> = assets
        .iter()
        .map(|entry| (entry.path.as_str(), entry.bytes.len()))
        .collect();
    project.assets.values().map(|asset| serde_json::json!({
        "id": asset.id.as_str(), "path": asset.logical_path, "media_type": asset.media_type,
        "sha256": asset.digest.as_str(), "size": sizes.get(asset.logical_path.as_str()).copied().unwrap_or(0)
    })).collect()
}

fn debug_map(
    project: &crate::package::ComposedProject,
    resolved: &ResolvedSourceProject,
) -> Result<Vec<u8>, BuildError> {
    // The debug map is the only place composition provenance is described. It is a non-essential
    // tooling entry, emitted only when the Bot asks for a debug build.
    let provenance = project.provenance.iter().map(|((kind, id), row)| serde_json::json!({
        "kind": kind.label(), "id": id, "package": row.owner.as_str(), "exported": row.exported,
        "replaced": row.replaced.as_ref().map(|id| serde_json::json!({"package":id.as_str()})),
    })).collect::<Vec<_>>();
    let tests = serde_json::json!({
        "regression_case_ids": project.tests.regression_cases.iter().map(|row| row.id.as_str()).collect::<Vec<_>>(),
        "scenario_ids": project.tests.scenarios.iter().map(|row| row.id.as_str()).collect::<Vec<_>>(),
    });
    let doc = serde_json::json!({
        "format":"gvya.debug-map", "version":1,
        "project_id":resolved.project.project_id, "brain_id":resolved.project.brain_id,
        "source_packages":resolved.source_package_digests, "provenance":provenance, "tests":tests,
    });
    canonical_json(&doc).map_err(|error| BuildError::Canonical(format!("debug map: {error:?}")))
}

fn validate_signature_envelope(value: &SignatureEnvelope) -> Result<(), BuildError> {
    let valid = !value.algorithm.trim().is_empty()
        && value.algorithm.len() <= 128
        && !value.key_id.trim().is_empty()
        && value.key_id.len() <= 512
        && !value.signature.trim().is_empty()
        && value.signature.len() <= 16 * 1024
        && value.algorithm.bytes().all(|byte| byte.is_ascii_graphic())
        && value.key_id.chars().all(|ch| !ch.is_control())
        && value.signature.chars().all(|ch| !ch.is_control());
    if valid {
        Ok(())
    } else {
        Err(BuildError::InvalidSignatureEnvelope)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::{PACKAGE_SOURCE_FORMAT, PACKAGE_SOURCE_VERSION, SourceIssue, SourceTree};

    fn expand_test_package_specs(
        mut files: BTreeMap<String, Vec<u8>>,
    ) -> BTreeMap<String, Vec<u8>> {
        let package_paths: Vec<String> = files
            .keys()
            .filter(|path| path.ends_with("package.json"))
            .cloned()
            .collect();
        let mut additions = Vec::new();
        for package_path in package_paths {
            let Some(bytes) = files.get(&package_path).cloned() else {
                continue;
            };
            let Ok(mut root) = serde_json::from_slice::<serde_json::Value>(&bytes) else {
                continue;
            };
            if root.get("format").and_then(serde_json::Value::as_str)
                != Some("gvya.test.package-spec")
            {
                continue;
            }
            let Some(contents) = root
                .get_mut("contents")
                .and_then(serde_json::Value::as_object_mut)
                .map(std::mem::take)
            else {
                continue;
            };
            let dir = package_path.rsplit_once('/').map_or("", |(dir, _)| dir);
            let mut fragments = serde_json::Map::new();
            for (namespace, value) in contents {
                if let Some(rows) = value.as_array() {
                    let mut paths = Vec::new();
                    for (index, row) in rows.iter().enumerate() {
                        let relative = format!("fragments/{namespace}/{:04}.json", index + 1);
                        let full = if dir.is_empty() {
                            relative.clone()
                        } else {
                            format!("{dir}/{relative}")
                        };
                        additions.push((full, serde_json::to_vec(row).unwrap()));
                        paths.push(serde_json::Value::String(relative));
                    }
                    fragments.insert(namespace, serde_json::Value::Array(paths));
                } else {
                    fragments.insert(namespace, value);
                }
            }
            let obj = root.as_object_mut().unwrap();
            obj.remove("contents");
            obj.insert(
                "format".into(),
                serde_json::Value::String(PACKAGE_SOURCE_FORMAT.into()),
            );
            obj.insert(
                "version".into(),
                serde_json::Value::from(PACKAGE_SOURCE_VERSION),
            );
            obj.insert("fragments".into(), serde_json::Value::Object(fragments));
            files.insert(package_path, serde_json::to_vec(&root).unwrap());
        }
        files.extend(additions);
        files
    }

    fn test_source_tree(
        files: BTreeMap<String, Vec<u8>>,
        limits: SourceLimits,
    ) -> Result<SourceTree, Vec<SourceIssue>> {
        SourceTree::new(expand_test_package_specs(files), limits)
    }

    fn minimal_tree() -> SourceTree {
        let project = br#"{"format":"gvya.source.project","version":1,"project_id":"demo","brain_id":"assistant","languages":["en"],"enabled_languages":["en"],"default_language":"en","packages":["packages/base/package.json"]}"#.to_vec();
        let package = br#"{"format":"gvya.test.package-spec","manifest":{"id":"base","kind":"standard","dependencies":[]},"contents":{"meanings":[{"id":"hello","value":{"id":"hello","samples":[{"language":"en","text":"hello"}]}}],"behaviors":[{"id":"hello","value":{"id":"hello","meaning":"hello","responses":[{"id":"hello.answer","texts":[{"language":"en","variants":["Hello"]}]}]}}]}}"#.to_vec();
        test_source_tree(
            BTreeMap::from([
                ("gvya.project.json".into(), project),
                ("packages/base/package.json".into(), package),
            ]),
            SourceLimits::default(),
        )
        .unwrap()
    }

    #[test]
    fn build_is_byte_reproducible() {
        let tree = minimal_tree();
        let a = build_source_project(&tree, BuildOptions::default(), None).unwrap();
        let b = build_source_project(&tree, BuildOptions::default(), None).unwrap();
        assert_eq!(a.artifact, b.artifact);
        assert_eq!(a.artifact_digest, b.artifact_digest);
    }

    #[test]
    fn package_file_enumeration_order_is_not_artifact_authority() {
        let base = br#"{"format":"gvya.test.package-spec","manifest":{"id":"base","kind":"standard","dependencies":[]},"contents":{"meanings":[{"id":"hello","value":{"id":"hello","samples":[{"language":"en","text":"hello"}]}}],"behaviors":[{"id":"hello","value":{"id":"hello","meaning":"hello","responses":[{"id":"hello.answer","texts":[{"language":"en","variants":["Hello"]}]}]}}]}}"#.to_vec();
        let extra = br#"{"format":"gvya.test.package-spec","manifest":{"id":"extra","kind":"standard","dependencies":[]},"contents":{}}"#.to_vec();
        let project_a = br#"{"format":"gvya.source.project","version":1,"project_id":"demo","brain_id":"assistant","languages":["en"],"enabled_languages":["en"],"default_language":"en","packages":["packages/base/package.json","packages/extra/package.json"]}"#.to_vec();
        let project_b = br#"{"format":"gvya.source.project","version":1,"project_id":"demo","brain_id":"assistant","languages":["en"],"enabled_languages":["en"],"default_language":"en","packages":["packages/extra/package.json","packages/base/package.json"]}"#.to_vec();
        let files = |project| {
            BTreeMap::from([
                ("gvya.project.json".into(), project),
                ("packages/base/package.json".into(), base.clone()),
                ("packages/extra/package.json".into(), extra.clone()),
            ])
        };
        let a = build_source_project(
            &test_source_tree(files(project_a), SourceLimits::default()).unwrap(),
            BuildOptions::default(),
            None,
        )
        .unwrap();
        let b = build_source_project(
            &test_source_tree(files(project_b), SourceLimits::default()).unwrap(),
            BuildOptions::default(),
            None,
        )
        .unwrap();
        assert_eq!(a.artifact, b.artifact);
        assert_eq!(a.artifact_digest, b.artifact_digest);
    }

    fn package_spec(id: &str, dependencies: &str) -> Vec<u8> {
        format!(
            r#"{{"format":"gvya.test.package-spec","manifest":{{"id":"{id}","kind":"standard","dependencies":[{dependencies}]}},"contents":{{"meanings":[{{"id":"{id}.hello","value":{{"id":"{id}.hello","samples":[{{"language":"en","text":"hello from {id}"}}]}}}}],"behaviors":[{{"id":"{id}.hello","value":{{"id":"{id}.hello","meaning":"{id}.hello","responses":[{{"id":"{id}.hello.answer","texts":[{{"language":"en","variants":["Hello"]}}]}}]}}}}]}}}}"#
        )
        .into_bytes()
    }

    fn project_with_packages(paths: &[&str]) -> Vec<u8> {
        let list = paths
            .iter()
            .map(|path| format!("\"{path}\""))
            .collect::<Vec<_>>()
            .join(",");
        format!(
            r#"{{"format":"gvya.source.project","version":1,"project_id":"demo","brain_id":"assistant","languages":["en"],"enabled_languages":["en"],"default_language":"en","packages":[{list}]}}"#
        )
        .into_bytes()
    }

    fn manifest_package_ids(result: &BuildResult) -> Vec<String> {
        result.manifest["packages"]
            .as_array()
            .expect("manifest packages array")
            .iter()
            .map(|row| row["id"].as_str().expect("package id").to_owned())
            .collect()
    }

    #[test]
    fn package_present_in_the_source_tree_but_unselected_never_enters_the_build() {
        let files = BTreeMap::from([
            (
                "gvya.project.json".to_owned(),
                project_with_packages(&["packages/selected/package.json"]),
            ),
            (
                "packages/selected/package.json".to_owned(),
                package_spec("selected", ""),
            ),
            (
                "packages/unselected/package.json".to_owned(),
                package_spec("unselected", ""),
            ),
        ]);
        let built = build_source_project(
            &test_source_tree(files, SourceLimits::default()).unwrap(),
            BuildOptions::default(),
            None,
        )
        .unwrap();
        assert_eq!(manifest_package_ids(&built), vec!["selected".to_owned()]);
        let program = String::from_utf8(
            parse_artifact(&built.artifact, ArtifactLimits::default())
                .unwrap()
                .entry("program.json")
                .unwrap()
                .to_vec(),
        )
        .unwrap();
        assert!(!program.contains("unselected"));
    }

    #[test]
    fn transitive_dependency_left_out_of_the_project_list_fails_closed() {
        let files = BTreeMap::from([
            (
                "gvya.project.json".to_owned(),
                project_with_packages(&["packages/app/package.json"]),
            ),
            (
                "packages/app/package.json".to_owned(),
                package_spec("app", r#"{"id":"base","reexport":false}"#),
            ),
            (
                "packages/base/package.json".to_owned(),
                package_spec("base", ""),
            ),
        ]);
        let error = build_source_project(
            &test_source_tree(files, SourceLimits::default()).unwrap(),
            BuildOptions::default(),
            None,
        )
        .unwrap_err();
        let BuildError::Audit(report) = error else {
            panic!("omitting a required dependency must fail the audit gate");
        };
        assert!(
            report
                .issues
                .iter()
                .any(|row| row.code.as_str() == "package.dependency_missing")
        );
    }

    #[test]
    fn declared_transitive_dependency_is_composed_into_the_brain() {
        let files = BTreeMap::from([
            (
                "gvya.project.json".to_owned(),
                project_with_packages(&["packages/app/package.json", "packages/base/package.json"]),
            ),
            (
                "packages/app/package.json".to_owned(),
                package_spec("app", r#"{"id":"base","reexport":false}"#),
            ),
            (
                "packages/base/package.json".to_owned(),
                package_spec("base", ""),
            ),
        ]);
        let built = build_source_project(
            &test_source_tree(files, SourceLimits::default()).unwrap(),
            BuildOptions::default(),
            None,
        )
        .unwrap();
        assert_eq!(
            manifest_package_ids(&built),
            vec!["app".to_owned(), "base".to_owned()]
        );
    }

    #[test]
    fn debug_map_is_additive_and_never_changes_the_executable_program() {
        let package = package_spec("base", "");
        let project = |debug: bool| {
            format!(
                r#"{{"format":"gvya.source.project","version":1,"project_id":"demo","brain_id":"assistant","languages":["en"],"enabled_languages":["en"],"default_language":"en","packages":["packages/base/package.json"],"emit_debug_map":{debug}}}"#
            )
            .into_bytes()
        };
        let build = |debug: bool| {
            build_source_project(
                &test_source_tree(
                    BTreeMap::from([
                        ("gvya.project.json".to_owned(), project(debug)),
                        ("packages/base/package.json".to_owned(), package.clone()),
                    ]),
                    SourceLimits::default(),
                )
                .unwrap(),
                BuildOptions::default(),
                None,
            )
            .unwrap()
        };
        let release = build(false);
        let debug = build(true);

        // Byte-identical executable program: the debug map cannot define runtime semantics.
        assert_eq!(release.program_digest, debug.program_digest);
        let release_entries = parse_artifact(&release.artifact, ArtifactLimits::default()).unwrap();
        let debug_entries = parse_artifact(&debug.artifact, ArtifactLimits::default()).unwrap();
        assert_eq!(
            release_entries.entry("program.json"),
            debug_entries.entry("program.json")
        );

        // The debug map exists only in the debug build, is non-essential, and is declared.
        assert!(release_entries.entry("debug/source-map.json").is_none());
        assert_eq!(release.manifest["debug_map"], JsonValue::Null);
        let map = debug_entries.entry("debug/source-map.json").unwrap();
        assert_eq!(debug.manifest["debug_map"], "debug/source-map.json");
        assert!(
            debug_entries
                .entries()
                .iter()
                .any(|row| row.path == "debug/source-map.json" && !row.essential)
        );

        // Composition provenance lives there, and only there.
        let map: JsonValue = serde_json::from_slice(map).unwrap();
        assert!(
            map["provenance"].as_array().is_some_and(|rows| rows
                .iter()
                .any(|row| row["kind"] == "meaning"
                    && row["package"] == "base"
                    && row["exported"] == true))
        );
        assert!(map["tests"]["regression_case_ids"].is_array());
    }

    struct FakeSigner;
    impl ArtifactSigner for FakeSigner {
        type Error = String;
        fn sign_content_root(&self, root: [u8; 32]) -> Result<SignatureEnvelope, Self::Error> {
            Ok(SignatureEnvelope {
                algorithm: "test-only".into(),
                key_id: "key".into(),
                signature: crate::canonical::hex(&root),
            })
        }
    }

    #[test]
    fn signature_does_not_change_content_root() {
        let tree = minimal_tree();
        let unsigned = build_source_project(&tree, BuildOptions::default(), None).unwrap();
        let signed =
            build_source_project(&tree, BuildOptions::default(), Some(&FakeSigner)).unwrap();
        assert_eq!(unsigned.content_root, signed.content_root);
        assert_ne!(unsigned.artifact_digest, signed.artifact_digest);
    }

    #[test]
    fn external_signature_attachment_preserves_content_root() {
        let tree = minimal_tree();
        let unsigned = build_source_project(&tree, BuildOptions::default(), None).unwrap();
        let envelope = SignatureEnvelope {
            algorithm: "test-only".into(),
            key_id: "external-key".into(),
            signature: "opaque-signature".into(),
        };
        let signed = attach_signature_envelope(
            &unsigned.artifact,
            &unsigned.content_root,
            &envelope,
            ArtifactLimits::default(),
        )
        .unwrap();
        let parsed = parse_artifact(&signed, ArtifactLimits::default()).unwrap();
        assert_eq!(
            crate::canonical::hex(&parsed.content_root()),
            unsigned.content_root
        );
        assert!(parsed.entry("signature.json").is_some());
        assert!(matches!(
            attach_signature_envelope(
                &signed,
                &unsigned.content_root,
                &envelope,
                ArtifactLimits::default()
            ),
            Err(BuildError::ArtifactAlreadySigned)
        ));
    }

    #[test]
    fn external_signature_attachment_binds_exact_content_root() {
        let tree = minimal_tree();
        let unsigned = build_source_project(&tree, BuildOptions::default(), None).unwrap();
        let envelope = SignatureEnvelope {
            algorithm: "test-only".into(),
            key_id: "external-key".into(),
            signature: "opaque-signature".into(),
        };
        assert!(matches!(
            attach_signature_envelope(
                &unsigned.artifact,
                &"00".repeat(32),
                &envelope,
                ArtifactLimits::default(),
            ),
            Err(BuildError::SignatureContentRootMismatch { .. })
        ));
    }
}
