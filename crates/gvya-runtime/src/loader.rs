//! Strict `.gvya` load/integrity/signature boundary.

use std::collections::{BTreeMap, BTreeSet};

use gvya_artifact::{ArtifactEntryInfo, ArtifactLimits, EntryKind, parse_artifact};
use gvya_model::AssetId;
use serde::Deserialize;
use sha2::{Digest, Sha256};

use crate::program::{
    HydratedProgram, PROGRAM_VERSION, ProgramError, ProgramLimits, RuntimeAssetDefinition,
    hydrate_program_with_limits,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LoadPolicy {
    pub artifact_limits: ArtifactLimits,
    pub program_limits: ProgramLimits,
    pub require_signature: bool,
}
impl Default for LoadPolicy {
    fn default() -> Self {
        Self {
            artifact_limits: ArtifactLimits::default(),
            program_limits: ProgramLimits::default(),
            require_signature: false,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TrustStatus {
    Unsigned,
    PresentUnverified { key_id: String, algorithm: String },
    Verified { key_id: String, algorithm: String },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SignatureEnvelope {
    pub algorithm: String,
    pub key_id: String,
    pub signature: String,
}

/// Host-owned signature trust. Structural/digest validation always runs before this hook.
pub trait SignatureVerifier {
    fn verify(&self, content_root: [u8; 32], envelope: &SignatureEnvelope) -> Result<(), String>;
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LoadError {
    Artifact(String),
    Manifest(String),
    Integrity(String),
    Program(ProgramError),
    DigestMismatch(String),
    SizeMismatch(String),
    AssetMismatch(String),
    Signature(String),
    SignatureRequired,
    RuntimeLimits(String),
}

#[derive(Clone, Debug)]
pub struct LoadedArtifact {
    bytes: Vec<u8>,
    entries: Vec<ArtifactEntryInfo>,
    pub content_root: [u8; 32],
    pub artifact_digest: String,
    pub project_id: String,
    pub brain_id: String,
    pub program: HydratedProgram,
    pub trust: TrustStatus,
}

impl LoadedArtifact {
    #[must_use]
    pub fn asset(&self, id: &AssetId) -> Option<RuntimeAsset<'_>> {
        let definition = self.program.assets.get(id)?;
        self.asset_definition(definition)
    }

    #[must_use]
    pub fn asset_by_logical_path(&self, logical_path: &str) -> Option<RuntimeAsset<'_>> {
        let definition = self
            .program
            .assets
            .values()
            .find(|row| row.logical_path == logical_path)?;
        self.asset_definition(definition)
    }

    fn asset_definition<'a>(
        &'a self,
        definition: &'a RuntimeAssetDefinition,
    ) -> Option<RuntimeAsset<'a>> {
        let entry = self
            .entries
            .iter()
            .find(|row| row.path == definition.logical_path && row.kind == EntryKind::Asset)?;
        let start = usize::try_from(entry.offset).ok()?;
        let length = usize::try_from(entry.length).ok()?;
        let bytes = self.bytes.get(start..start.checked_add(length)?)?;
        Some(RuntimeAsset {
            id: &definition.id,
            media_type: &definition.media_type,
            logical_path: &definition.logical_path,
            digest: &definition.digest,
            bytes,
        })
    }
}

#[derive(Clone, Debug)]
pub struct RuntimeAsset<'a> {
    pub id: &'a AssetId,
    pub media_type: &'a str,
    pub logical_path: &'a str,
    pub digest: &'a str,
    pub bytes: &'a [u8],
}

pub fn load_artifact(
    bytes: Vec<u8>,
    policy: LoadPolicy,
    verifier: Option<&dyn SignatureVerifier>,
) -> Result<LoadedArtifact, LoadError> {
    let parsed = parse_artifact(&bytes, policy.artifact_limits)
        .map_err(|error| LoadError::Artifact(format!("{error:?}")))?;
    let entries = parsed.entries().to_vec();
    let content_root = parsed.content_root();
    let manifest_bytes = parsed
        .entry("manifest.json")
        .ok_or_else(|| LoadError::Artifact("manifest missing after container validation".into()))?;
    let integrity_bytes = parsed.entry("integrity.json").ok_or_else(|| {
        LoadError::Artifact("integrity missing after container validation".into())
    })?;
    let program_bytes = parsed
        .entry("program.json")
        .ok_or_else(|| LoadError::Artifact("program missing after container validation".into()))?;

    validate_metadata_json_shape("manifest", manifest_bytes)?;
    validate_metadata_json_shape("integrity", integrity_bytes)?;
    let manifest: ManifestDoc = serde_json::from_slice(manifest_bytes)
        .map_err(|error| LoadError::Manifest(error.to_string()))?;
    validate_manifest_header(&manifest)?;
    let integrity: IntegrityDoc = serde_json::from_slice(integrity_bytes)
        .map_err(|error| LoadError::Integrity(error.to_string()))?;
    validate_integrity_header(&integrity)?;

    verify_digest_size(
        "manifest.integrity",
        integrity_bytes,
        &manifest.integrity.sha256,
        None,
    )?;
    verify_digest_size(
        "manifest.program",
        program_bytes,
        &manifest.program.sha256,
        Some(manifest.program.size),
    )?;
    verify_digest_size(
        "integrity.program",
        program_bytes,
        &integrity.program.sha256,
        Some(integrity.program.size),
    )?;
    if manifest.program.sha256 != integrity.program.sha256
        || manifest.program.size != integrity.program.size
    {
        return Err(LoadError::Integrity(
            "manifest/integrity program rows disagree".into(),
        ));
    }

    // Trust is checked before executable IR hydration so a host that requires a signature does not
    // pay the full decode/validation cost for an unauthenticated program. Container/digest checks
    // above still run first so the verifier is always bound to the canonical content root.
    let trust = validate_signature(&parsed, content_root, policy.require_signature, verifier)?;

    let program = hydrate_program_with_limits(program_bytes, policy.program_limits)
        .map_err(LoadError::Program)?;
    if program.project_id != manifest.project_id || program.brain_id != manifest.brain_id {
        return Err(LoadError::Manifest(
            "program identity does not match manifest".into(),
        ));
    }
    validate_packages(&manifest, &integrity, &program)?;
    validate_debug_map(&parsed, &manifest)?;
    validate_assets(&parsed, &manifest, &integrity, &program.assets)?;

    Ok(LoadedArtifact {
        artifact_digest: sha256_hex(&bytes),
        bytes,
        entries,
        content_root,
        project_id: manifest.project_id,
        brain_id: manifest.brain_id,
        program,
        trust,
    })
}

fn validate_packages(
    manifest: &ManifestDoc,
    integrity: &IntegrityDoc,
    program: &HydratedProgram,
) -> Result<(), LoadError> {
    let mut manifest_map = BTreeMap::new();
    for row in &manifest.packages {
        if row.id.trim().is_empty() || !is_sha256_hex(&row.source_sha256) {
            return Err(LoadError::Manifest("invalid package provenance row".into()));
        }
        if manifest_map
            .insert(row.id.as_str(), row.source_sha256.as_str())
            .is_some()
        {
            return Err(LoadError::Manifest(format!(
                "duplicate manifest package id: {}",
                row.id
            )));
        }
    }
    let mut integrity_map = BTreeMap::new();
    for row in &integrity.source_packages {
        if row.id.trim().is_empty() || !is_sha256_hex(&row.sha256) {
            return Err(LoadError::Integrity(
                "invalid source package provenance row".into(),
            ));
        }
        if integrity_map
            .insert(row.id.as_str(), row.sha256.as_str())
            .is_some()
        {
            return Err(LoadError::Integrity(format!(
                "duplicate integrity package id: {}",
                row.id
            )));
        }
    }
    if manifest_map != integrity_map {
        return Err(LoadError::Integrity(
            "manifest/integrity source package sets disagree".into(),
        ));
    }
    if program.source_packages.len() != manifest_map.len() {
        return Err(LoadError::Integrity(
            "program source package provenance count disagrees".into(),
        ));
    }
    for (id, digest) in &program.source_packages {
        if !is_sha256_hex(digest) || manifest_map.get(id.as_str()).copied() != Some(digest.as_str())
        {
            return Err(LoadError::Integrity(format!(
                "program package provenance mismatch for {id}"
            )));
        }
    }
    let order: BTreeSet<&str> = program.package_order.iter().map(String::as_str).collect();
    let manifest_ids: BTreeSet<&str> = manifest_map.keys().copied().collect();
    if order.len() != program.package_order.len() || order != manifest_ids {
        return Err(LoadError::Integrity(
            "program package order is not a canonical permutation of source packages".into(),
        ));
    }
    Ok(())
}

fn validate_debug_map(
    parsed: &gvya_artifact::ParsedArtifact<'_>,
    manifest: &ManifestDoc,
) -> Result<(), LoadError> {
    let present = parsed
        .entries()
        .iter()
        .any(|row| row.kind == EntryKind::DebugMap && row.path == "debug/source-map.json");
    match manifest.debug_map.as_deref() {
        Some("debug/source-map.json") if present => Ok(()),
        None if !present => Ok(()),
        Some(_) => Err(LoadError::Manifest("unsupported debug map path".into())),
        None => Err(LoadError::Manifest(
            "debug map entry exists but manifest does not declare it".into(),
        )),
    }
}

fn validate_assets(
    parsed: &gvya_artifact::ParsedArtifact<'_>,
    manifest: &ManifestDoc,
    integrity: &IntegrityDoc,
    runtime_assets: &BTreeMap<AssetId, RuntimeAssetDefinition>,
) -> Result<(), LoadError> {
    let manifest_ids: BTreeSet<&str> = manifest.assets.iter().map(|row| row.id.as_str()).collect();
    if manifest_ids.len() != manifest.assets.len() {
        return Err(LoadError::AssetMismatch(
            "duplicate manifest asset id".into(),
        ));
    }
    let manifest_paths: BTreeSet<&str> = manifest
        .assets
        .iter()
        .map(|row| row.path.as_str())
        .collect();
    if manifest_paths.len() != manifest.assets.len() {
        return Err(LoadError::AssetMismatch(
            "duplicate manifest asset path".into(),
        ));
    }
    let integrity_paths: BTreeSet<&str> = integrity
        .assets
        .iter()
        .map(|row| row.path.as_str())
        .collect();
    if integrity_paths.len() != integrity.assets.len() {
        return Err(LoadError::AssetMismatch(
            "duplicate integrity asset path".into(),
        ));
    }
    let artifact_paths: BTreeSet<&str> = parsed
        .entries()
        .iter()
        .filter(|row| row.kind == EntryKind::Asset)
        .map(|row| row.path.as_str())
        .collect();
    let runtime_paths: BTreeSet<&str> = runtime_assets
        .values()
        .map(|row| row.logical_path.as_str())
        .collect();
    if manifest_paths != integrity_paths
        || manifest_paths != artifact_paths
        || manifest_paths != runtime_paths
    {
        return Err(LoadError::AssetMismatch(
            "asset path sets disagree across manifest/integrity/program/container".into(),
        ));
    }
    if runtime_assets.len() != manifest.assets.len() {
        return Err(LoadError::AssetMismatch("asset id counts disagree".into()));
    }
    for row in &manifest.assets {
        let definition = runtime_assets
            .get(&AssetId::new(row.id.clone()))
            .ok_or_else(|| {
                LoadError::AssetMismatch(format!("manifest asset {} absent from program", row.id))
            })?;
        if definition.logical_path != row.path
            || definition.media_type != row.media_type
            || definition.digest != row.sha256
        {
            return Err(LoadError::AssetMismatch(format!(
                "program/manifest metadata mismatch for {}",
                row.id
            )));
        }
        let integrity_row = integrity
            .assets
            .iter()
            .find(|value| value.path == row.path)
            .ok_or_else(|| {
                LoadError::AssetMismatch(format!("integrity row missing for {}", row.path))
            })?;
        if integrity_row.sha256 != row.sha256 || integrity_row.size != row.size {
            return Err(LoadError::AssetMismatch(format!(
                "manifest/integrity mismatch for {}",
                row.path
            )));
        }
        let payload = parsed.entry(&row.path).ok_or_else(|| {
            LoadError::AssetMismatch(format!("asset payload missing: {}", row.path))
        })?;
        verify_digest_size(&row.path, payload, &row.sha256, Some(row.size))?;
    }
    Ok(())
}

fn validate_signature(
    parsed: &gvya_artifact::ParsedArtifact<'_>,
    content_root: [u8; 32],
    required: bool,
    verifier: Option<&dyn SignatureVerifier>,
) -> Result<TrustStatus, LoadError> {
    let Some(bytes) = parsed.entry("signature.json") else {
        if required {
            return Err(LoadError::SignatureRequired);
        }
        return Ok(TrustStatus::Unsigned);
    };
    validate_metadata_json_shape("signature", bytes)?;
    let doc: SignatureDoc =
        serde_json::from_slice(bytes).map_err(|error| LoadError::Signature(error.to_string()))?;
    if doc.format != "gvya.signature" || doc.version != 1 {
        return Err(LoadError::Signature(
            "unsupported signature envelope".into(),
        ));
    }
    if doc.content_root != hex(&content_root) {
        return Err(LoadError::Signature(
            "signature content_root does not match validated artifact".into(),
        ));
    }
    if doc.algorithm.trim().is_empty()
        || doc.key_id.trim().is_empty()
        || doc.signature.trim().is_empty()
    {
        return Err(LoadError::Signature(
            "signature envelope contains empty required field".into(),
        ));
    }
    let envelope = SignatureEnvelope {
        algorithm: doc.algorithm,
        key_id: doc.key_id,
        signature: doc.signature,
    };
    if let Some(verifier) = verifier {
        verifier
            .verify(content_root, &envelope)
            .map_err(LoadError::Signature)?;
        Ok(TrustStatus::Verified {
            key_id: envelope.key_id,
            algorithm: envelope.algorithm,
        })
    } else if required {
        Err(LoadError::Signature(
            "signature is present but no host trust verifier was supplied".into(),
        ))
    } else {
        Ok(TrustStatus::PresentUnverified {
            key_id: envelope.key_id,
            algorithm: envelope.algorithm,
        })
    }
}

const METADATA_JSON_MAX_DEPTH: usize = 64;
const METADATA_JSON_MAX_STRUCTURAL_TOKENS: usize = 100_000;
const METADATA_JSON_MAX_STRING_BYTES: usize = 256 * 1024;

/// Allocation-free preflight for manifest/integrity/signature JSON. Container entry byte limits
/// bound raw input size; this additional scan bounds nesting, structural fan-out and individual
/// string tokens before serde allocates typed metadata collections.
fn validate_metadata_json_shape(label: &str, bytes: &[u8]) -> Result<(), LoadError> {
    let mut depth = 0_usize;
    let mut structural = 0_usize;
    let mut in_string = false;
    let mut escaped = false;
    let mut string_bytes = 0_usize;

    for &byte in bytes {
        if in_string {
            if escaped {
                escaped = false;
                string_bytes = string_bytes.saturating_add(1);
                continue;
            }
            match byte {
                b'\\' => {
                    escaped = true;
                    string_bytes = string_bytes.saturating_add(1);
                }
                b'"' => {
                    in_string = false;
                    string_bytes = 0;
                }
                _ => {
                    string_bytes = string_bytes.saturating_add(1);
                    if string_bytes > METADATA_JSON_MAX_STRING_BYTES {
                        return Err(metadata_limit_error(
                            label,
                            "string token exceeds canonical byte limit",
                        ));
                    }
                }
            }
            continue;
        }

        match byte {
            b'"' => {
                in_string = true;
                string_bytes = 0;
            }
            b'{' | b'[' => {
                depth = depth.saturating_add(1);
                structural = structural.saturating_add(1);
                if depth > METADATA_JSON_MAX_DEPTH {
                    return Err(metadata_limit_error(
                        label,
                        "nesting exceeds canonical depth limit",
                    ));
                }
            }
            b'}' | b']' => {
                depth = depth.saturating_sub(1);
            }
            b',' | b':' => {
                structural = structural.saturating_add(1);
            }
            _ => {}
        }
        if structural > METADATA_JSON_MAX_STRUCTURAL_TOKENS {
            return Err(metadata_limit_error(
                label,
                "structural complexity exceeds canonical token budget",
            ));
        }
    }
    Ok(())
}

fn metadata_limit_error(label: &str, message: &str) -> LoadError {
    let text = format!("{label} metadata {message}");
    match label {
        "manifest" => LoadError::Manifest(text),
        "integrity" => LoadError::Integrity(text),
        "signature" => LoadError::Signature(text),
        _ => LoadError::Artifact(text),
    }
}

fn verify_digest_size(
    label: &str,
    bytes: &[u8],
    expected: &str,
    expected_size: Option<usize>,
) -> Result<(), LoadError> {
    if expected_size.is_some_and(|size| size != bytes.len()) {
        return Err(LoadError::SizeMismatch(label.into()));
    }
    if sha256_hex(bytes) != expected {
        return Err(LoadError::DigestMismatch(label.into()));
    }
    Ok(())
}

fn validate_manifest_header(doc: &ManifestDoc) -> Result<(), LoadError> {
    if doc.format != "gvya.artifact" || doc.version != 1 || doc.container_version != 1 {
        return Err(LoadError::Manifest(
            "unsupported artifact manifest version".into(),
        ));
    }
    if doc.program.path != "program.json"
        || doc.program.format != "gvya.program"
        || doc.program.version != PROGRAM_VERSION
    {
        return Err(LoadError::Manifest("unsupported program row".into()));
    }
    if doc.integrity.path != "integrity.json" {
        return Err(LoadError::Manifest("invalid integrity path".into()));
    }
    if doc.signing.content_root_algorithm != "sha256-essential-entry-set-v1"
        || doc.signing.envelope_path != "signature.json"
    {
        return Err(LoadError::Manifest("unsupported signing boundary".into()));
    }
    Ok(())
}
fn validate_integrity_header(doc: &IntegrityDoc) -> Result<(), LoadError> {
    if doc.format != "gvya.integrity" || doc.version != 1 || doc.program.path != "program.json" {
        return Err(LoadError::Integrity(
            "unsupported integrity document version".into(),
        ));
    }
    Ok(())
}

fn is_sha256_hex(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}

fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|value| format!("{value:02x}"))
        .collect()
}
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|value| format!("{value:02x}")).collect()
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ManifestDoc {
    format: String,
    version: u32,
    container_version: u16,
    project_id: String,
    brain_id: String,
    program: ManifestProgram,
    integrity: ManifestIntegrity,
    packages: Vec<ManifestPackage>,
    assets: Vec<ManifestAsset>,
    debug_map: Option<String>,
    signing: ManifestSigning,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ManifestProgram {
    path: String,
    format: String,
    version: u32,
    sha256: String,
    size: usize,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ManifestIntegrity {
    path: String,
    sha256: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ManifestPackage {
    id: String,
    source_sha256: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ManifestAsset {
    id: String,
    path: String,
    media_type: String,
    sha256: String,
    size: usize,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ManifestSigning {
    content_root_algorithm: String,
    envelope_path: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct IntegrityDoc {
    format: String,
    version: u32,
    program: IntegrityProgram,
    assets: Vec<IntegrityAsset>,
    source_packages: Vec<IntegrityPackage>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct IntegrityProgram {
    path: String,
    sha256: String,
    size: usize,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct IntegrityAsset {
    path: String,
    sha256: String,
    size: usize,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct IntegrityPackage {
    id: String,
    sha256: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SignatureDoc {
    format: String,
    version: u32,
    content_root: String,
    algorithm: String,
    key_id: String,
    signature: String,
}

#[cfg(test)]
mod metadata_limit_tests {
    use super::*;

    #[test]
    fn metadata_preflight_rejects_excessive_nesting_before_typed_decode() {
        let mut bytes = vec![b'['; METADATA_JSON_MAX_DEPTH + 1];
        bytes.extend(std::iter::repeat_n(b']', METADATA_JSON_MAX_DEPTH + 1));
        assert!(matches!(
            validate_metadata_json_shape("manifest", &bytes),
            Err(LoadError::Manifest(message)) if message.contains("depth limit")
        ));
    }

    #[test]
    fn metadata_preflight_rejects_oversized_string_token_before_typed_decode() {
        let mut bytes = Vec::with_capacity(METADATA_JSON_MAX_STRING_BYTES + 3);
        bytes.push(b'\"');
        bytes.extend(std::iter::repeat_n(
            b'x',
            METADATA_JSON_MAX_STRING_BYTES + 1,
        ));
        bytes.push(b'\"');
        assert!(matches!(
            validate_metadata_json_shape("integrity", &bytes),
            Err(LoadError::Integrity(message)) if message.contains("string token")
        ));
    }
}
