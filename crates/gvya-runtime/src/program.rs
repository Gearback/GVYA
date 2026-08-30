//! Strict hydration of the clean-break `gvya.program/1` runtime IR.

use std::collections::{BTreeMap, BTreeSet};

use gvya_kernel::{
    capability::{
        AdmissionNamespace, AdmissionPredicate, ArgumentBinding, ArgumentPath, BindingSource,
        CapabilityBindingRule, CapabilityCatalog, CapabilityConfig, CapabilityDefinition,
        CapabilityPolicyRule, CapabilityTrigger, HostEffectDeclaration, HostEffectKind,
        ObjectSchema, PolicyEffect, PredicateOp as AdmissionPredicateOp, ReferenceProjection,
        SchemaLimits, ValueSchema,
    },
    conversation::{
        AuthorNumberDefinition, CapabilityResultBehavior, ConversationBehavior,
        ConversationCatalog, ConversationConfig, ConversationEffect, ExtraMessage,
        FallbackBehavior, FallbackTrigger, FollowupDirective, LocalizedTexts, OpeningDefinition,
        PredicateOp as ConversationPredicateOp, RepeatStage, ResponseAsset, ResponseDefinition,
        ResponseKind, ResponseLink, StateNamespace, StateTarget, StyleLexicon, ValueCondition,
        ValuePath, ValueRequirement, language_tag_is_well_formed, normalize_locale,
    },
    semantic::{
        ElicitationPrompt, EntityKind, LocalizedSample, MeaningClass, MeaningPattern,
        ReferenceSpec, SEMANTIC_AMBIGUITY_MARGIN_MAX, SEMANTIC_AMBIGUITY_MARGIN_MIN,
        SEMANTIC_CANDIDATE_LIMIT_MAX, SEMANTIC_CANDIDATE_LIMIT_MIN,
        SEMANTIC_RESOLUTION_THRESHOLD_MAX, SEMANTIC_RESOLUTION_THRESHOLD_MIN,
        SEMANTIC_RESOLVER_CANDIDATE_LIMIT_MAX, SEMANTIC_RESOLVER_CANDIDATE_LIMIT_MIN,
        SEMANTIC_RESOLVER_CONFIDENCE_MAX, SEMANTIC_RESOLVER_CONFIDENCE_MIN, SemanticCatalog,
        SemanticConfig, SemanticKernel, SemanticProfile, SemanticProfiles, SlotKind, SlotSpec,
    },
};
use gvya_model::{
    AssetId, BehaviorId, CapabilityBindingId, CapabilityContract, CapabilityId, CapabilityVersion,
    ConfirmationHint, EffectClass, FollowupId, GVYA_PROGRAM_MAX_BYTES,
    GVYA_PROGRAM_MAX_COLLECTION_ENTRIES, GVYA_PROGRAM_MAX_DEPTH, GVYA_PROGRAM_MAX_NODES,
    GVYA_PROGRAM_MAX_PACKAGES, GVYA_PROGRAM_MAX_STRING_BYTES, MeaningId, OpeningId, PolicyId,
    ReferenceKind, ResponseId, SchemaDocument, TopicId, Value,
};
use serde::Deserialize;
use serde_json::Value as JsonValue;

mod asset_doc;
mod capability_doc;
mod conversation_doc;
mod helpers;
mod semantic_doc;
#[cfg(test)]
mod tests;

use asset_doc::*;
use capability_doc::*;
use conversation_doc::*;
use helpers::*;
use semantic_doc::*;

pub const PROGRAM_FORMAT: &str = "gvya.program";
pub const PROGRAM_VERSION: u32 = 1;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProgramLimits {
    pub max_program_bytes: usize,
    pub max_depth: usize,
    pub max_nodes: usize,
    pub max_collection_entries: usize,
    pub max_string_bytes: usize,
    pub max_packages: usize,
}

impl Default for ProgramLimits {
    fn default() -> Self {
        Self {
            max_program_bytes: GVYA_PROGRAM_MAX_BYTES,
            max_depth: GVYA_PROGRAM_MAX_DEPTH,
            max_nodes: GVYA_PROGRAM_MAX_NODES,
            max_collection_entries: GVYA_PROGRAM_MAX_COLLECTION_ENTRIES,
            max_string_bytes: GVYA_PROGRAM_MAX_STRING_BYTES,
            max_packages: GVYA_PROGRAM_MAX_PACKAGES,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct RuntimeAssetDefinition {
    pub id: AssetId,
    pub media_type: String,
    pub logical_path: String,
    pub digest: String,
}

#[derive(Clone, Debug)]
pub struct HydratedProgram {
    pub project_id: String,
    pub brain_id: String,
    pub enabled_languages: Vec<String>,
    pub default_language: String,
    /// Compiler provenance only; never runtime execution authority.
    pub source_packages: BTreeMap<String, String>,
    /// Canonical composed package order for diagnostics/provenance only.
    pub package_order: Vec<String>,
    pub semantic: SemanticKernel,
    pub conversation_catalog: ConversationCatalog,
    pub conversation_config: ConversationConfig,
    pub capability_catalog: CapabilityCatalog,
    pub assets: BTreeMap<AssetId, RuntimeAssetDefinition>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProgramError {
    Json(String),
    Limit(String),
    UnsupportedFormat,
    UnsupportedVersion(u32),
    NonFiniteNumber(&'static str),
    InvalidSemanticCatalog(String),
    InvalidSemanticConfig(String),
    InvalidSemanticIndex(String),
    InvalidConversationCatalog(String),
    InvalidConversationConfig(String),
    InvalidCapabilityCatalog(String),
    InvalidArgumentPath(String),
    InvalidValueSchema(String),
    DuplicateAsset(String),
    InvalidAssetPath(String),
    InvalidAssetDigest(String),
    InvalidLanguageContract(String),
}

pub fn hydrate_program(bytes: &[u8]) -> Result<HydratedProgram, ProgramError> {
    hydrate_program_with_limits(bytes, ProgramLimits::default())
}

fn validate_semantic_profile_coverage(
    profiles: &SemanticProfiles,
    enabled_languages: &BTreeSet<String>,
) -> Result<(), ProgramError> {
    if profiles.len() != enabled_languages.len()
        || !enabled_languages
            .iter()
            .all(|language| profiles.contains_key(language))
    {
        return Err(ProgramError::InvalidLanguageContract(
            "semantic profiles must exactly cover enabled_languages".into(),
        ));
    }
    Ok(())
}

pub fn hydrate_program_with_limits(
    bytes: &[u8],
    limits: ProgramLimits,
) -> Result<HydratedProgram, ProgramError> {
    if limits.max_program_bytes == 0
        || limits.max_depth == 0
        || limits.max_nodes == 0
        || limits.max_collection_entries == 0
        || limits.max_string_bytes == 0
        || limits.max_packages == 0
    {
        return Err(ProgramError::Limit(
            "program limits must be positive".into(),
        ));
    }
    if limits.max_program_bytes > GVYA_PROGRAM_MAX_BYTES
        || limits.max_depth > GVYA_PROGRAM_MAX_DEPTH
        || limits.max_nodes > GVYA_PROGRAM_MAX_NODES
        || limits.max_collection_entries > GVYA_PROGRAM_MAX_COLLECTION_ENTRIES
        || limits.max_string_bytes > GVYA_PROGRAM_MAX_STRING_BYTES
        || limits.max_packages > GVYA_PROGRAM_MAX_PACKAGES
    {
        return Err(ProgramError::Limit(
            "program limits may tighten but not relax canonical executable ceilings".into(),
        ));
    }
    if bytes.len() > limits.max_program_bytes {
        return Err(ProgramError::Limit(
            "program.json exceeds executable byte limit".into(),
        ));
    }
    let json: JsonValue =
        serde_json::from_slice(bytes).map_err(|error| ProgramError::Json(error.to_string()))?;
    let mut nodes = 0usize;
    validate_program_json_shape(&json, 0, &mut nodes, limits)?;
    let doc: ProgramDoc =
        serde_json::from_value(json).map_err(|error| ProgramError::Json(error.to_string()))?;
    if doc.format != PROGRAM_FORMAT {
        return Err(ProgramError::UnsupportedFormat);
    }
    if doc.version != PROGRAM_VERSION {
        return Err(ProgramError::UnsupportedVersion(doc.version));
    }
    if doc.source_packages.len() > limits.max_packages || doc.packages.len() > limits.max_packages {
        return Err(ProgramError::Limit(
            "program package count exceeds executable limit".into(),
        ));
    }
    if doc.enabled_languages.is_empty()
        || doc.enabled_languages.len() > 32
        || doc
            .enabled_languages
            .iter()
            .any(|row| !language_tag_is_well_formed(row))
    {
        return Err(ProgramError::InvalidLanguageContract(
            "program enabled_languages must contain 1..=32 well-formed tags".into(),
        ));
    }
    let normalized_languages: BTreeSet<String> = doc
        .enabled_languages
        .iter()
        .map(|row| normalize_locale(row))
        .collect();
    if normalized_languages.len() != doc.enabled_languages.len()
        || !normalized_languages.contains(&normalize_locale(&doc.default_language))
    {
        return Err(ProgramError::InvalidLanguageContract(
            "program default_language must name one unique enabled language".into(),
        ));
    }
    if doc.semantic.patterns.len() > limits.max_collection_entries
        || doc.capabilities.definitions.len() > limits.max_collection_entries
        || doc.capabilities.bindings.len() > limits.max_collection_entries
        || doc.capabilities.policies.len() > limits.max_collection_entries
        || doc.assets.len() > limits.max_collection_entries
    {
        return Err(ProgramError::Limit(
            "program contribution count exceeds executable limit".into(),
        ));
    }

    let source_packages = doc.source_packages.clone();
    let package_order = doc.packages.clone();

    let semantic_catalog = SemanticCatalog::new(
        doc.semantic
            .patterns
            .into_iter()
            .map(MeaningPatternDoc::into_runtime)
            .collect::<Result<_, _>>()?,
    )
    .map_err(|error| ProgramError::InvalidSemanticCatalog(format!("{error:?}")))?;
    let semantic_profiles: SemanticProfiles = doc
        .semantic
        .profiles
        .into_iter()
        .map(|(language, profile)| (normalize_locale(&language), profile.into_runtime()))
        .collect();
    validate_semantic_profile_coverage(&semantic_profiles, &normalized_languages)?;
    let semantic_config = doc.semantic.config.into_runtime()?;
    // The matcher index is derived, not shipped: it is built here from the canonical patterns,
    // profiles and config the artifact actually carries, so it cannot drift from them.
    let semantic = SemanticKernel::new(semantic_catalog, semantic_profiles, semantic_config)
        .map_err(|error| ProgramError::InvalidSemanticIndex(format!("{error:?}")))?;

    let (conversation_catalog, conversation_config) = doc.conversation.into_runtime()?;
    let capability_catalog = doc.capabilities.into_catalog()?;

    let mut assets = BTreeMap::new();
    for asset in doc.assets {
        let source_id = asset.id.clone();
        let runtime = asset.into_runtime()?;
        if assets.insert(runtime.id.clone(), runtime).is_some() {
            return Err(ProgramError::DuplicateAsset(source_id));
        }
    }

    Ok(HydratedProgram {
        project_id: doc.project_id,
        brain_id: doc.brain_id,
        enabled_languages: doc.enabled_languages,
        default_language: doc.default_language,
        source_packages,
        package_order,
        semantic,
        conversation_catalog,
        conversation_config,
        capability_catalog,
        assets,
    })
}

fn validate_program_json_shape(
    value: &JsonValue,
    depth: usize,
    nodes: &mut usize,
    limits: ProgramLimits,
) -> Result<(), ProgramError> {
    if depth > limits.max_depth {
        return Err(ProgramError::Limit(
            "program JSON depth exceeds executable limit".into(),
        ));
    }
    *nodes = nodes.saturating_add(1);
    if *nodes > limits.max_nodes {
        return Err(ProgramError::Limit(
            "program JSON node count exceeds executable limit".into(),
        ));
    }
    match value {
        JsonValue::Null | JsonValue::Bool(_) => Ok(()),
        JsonValue::Number(number) => {
            if number.as_f64().is_some_and(f64::is_finite) {
                Ok(())
            } else {
                Err(ProgramError::Limit(
                    "program contains a non-finite/out-of-range number".into(),
                ))
            }
        }
        JsonValue::String(value) => {
            if value.len() <= limits.max_string_bytes {
                Ok(())
            } else {
                Err(ProgramError::Limit(
                    "program string exceeds executable limit".into(),
                ))
            }
        }
        JsonValue::Array(values) => {
            if values.len() > limits.max_collection_entries {
                return Err(ProgramError::Limit(
                    "program array exceeds executable entry limit".into(),
                ));
            }
            for row in values {
                validate_program_json_shape(row, depth + 1, nodes, limits)?;
            }
            Ok(())
        }
        JsonValue::Object(values) => {
            if values.len() > limits.max_collection_entries {
                return Err(ProgramError::Limit(
                    "program object exceeds executable entry limit".into(),
                ));
            }
            for (key, row) in values {
                if key.len() > limits.max_string_bytes {
                    return Err(ProgramError::Limit(
                        "program object key exceeds executable string limit".into(),
                    ));
                }
                validate_program_json_shape(row, depth + 1, nodes, limits)?;
            }
            Ok(())
        }
    }
}

/// Every field here is either executed by the runtime or cross-validated by runtime load against
/// the manifest/integrity rows. Nothing is deserialized and discarded: authoring-only named types
/// and composition provenance are not part of the executable program.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ProgramDoc {
    format: String,
    version: u32,
    project_id: String,
    brain_id: String,
    enabled_languages: Vec<String>,
    default_language: String,
    source_packages: BTreeMap<String, String>,
    packages: Vec<String>,
    semantic: SemanticDoc,
    conversation: ConversationDoc,
    capabilities: CapabilitiesDoc,
    assets: Vec<AssetDoc>,
}
