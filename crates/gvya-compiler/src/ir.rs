//! Canonical compiled runtime IR.
//!
//! The IR is deliberately explicit rather than serializing Rust structs by implementation detail.
//! This keeps `.gvya` stable across internal refactors and gives runtime/SDK layer a versioned loader target.

mod capability_ir;
mod conversation_ir;
mod helpers;
mod output_ir;
mod semantic_ir;
#[cfg(test)]
mod tests;

use capability_ir::*;
use conversation_ir::*;
use helpers::*;
use output_ir::*;
use semantic_ir::*;

use std::collections::{BTreeMap, BTreeSet};

use gvya_kernel::{
    capability::{
        AdmissionNamespace, AdmissionPredicate, ArgumentBinding, BindingSource, CapabilityConfig,
        CapabilityDefinition, CapabilityPolicyRule, HostEffectKind,
        ObjectSchema as CapabilityObjectSchema, PolicyEffect, PredicateOp as AdmissionPredicateOp,
        ReferenceProjection, SchemaLimits, ValueSchema,
    },
    conversation::{
        AuthorNumberDefinition, CapabilityResultBehavior, ConversationBehavior, ConversationConfig,
        ConversationEffect, ExtraMessage, FallbackBehavior, FollowupDirective, LocalizedTexts,
        OpeningDefinition, PredicateOp as ConversationPredicateOp, RepeatStage, ResponseDefinition,
        ResponseKind, StateNamespace, StateTarget, ValueCondition, ValueRequirement,
    },
    semantic::{
        MeaningClass, MeaningPattern, SemanticConfig, SemanticIndex, SemanticProfile,
        SemanticProfiles, SlotKind,
    },
};
use gvya_model::{ConfirmationHint, EffectClass, Value};
use serde_json::{Map, Number, Value as JsonValue};

use crate::{
    canonical::{CanonicalError, canonical_json, sha256_hex},
    package::ComposedProject,
};

/// The executable runtime program. It carries only what the runtime executes and what runtime
/// load must cross-validate against the manifest/integrity rows. Authoring-only data (named types,
/// regression/scenario corpus) and debug provenance never enter it; provenance belongs to the
/// optional `debug/source-map.json` entry.
pub const PROGRAM_FORMAT: &str = "gvya.program";
pub const PROGRAM_VERSION: u32 = 1;

#[derive(Clone, Debug, PartialEq)]
pub struct CompileIdentity {
    pub project_id: String,
    pub brain_id: String,
    pub enabled_languages: Vec<String>,
    pub default_language: String,
    pub semantic_config: SemanticConfig,
    pub conversation_config: ConversationConfig,
    /// Package id -> canonical source digest. This is diagnostics/provenance, not runtime authority.
    pub source_packages: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum IrError {
    Canonical(CanonicalError),
    NonFiniteNumber(&'static str),
    InvalidSchemaJson(String),
    InvalidSemanticIndex(String),
}

impl From<CanonicalError> for IrError {
    fn from(value: CanonicalError) -> Self {
        Self::Canonical(value)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct CompiledIr {
    pub document: JsonValue,
    pub bytes: Vec<u8>,
    pub digest_hex: String,
}

pub fn compile_ir(
    project: &ComposedProject,
    identity: &CompileIdentity,
) -> Result<CompiledIr, IrError> {
    validate_finite(identity)?;
    let document = object([
        ("format", string(PROGRAM_FORMAT)),
        ("version", uint(u64::from(PROGRAM_VERSION))),
        ("project_id", string(&identity.project_id)),
        ("brain_id", string(&identity.brain_id)),
        (
            "enabled_languages",
            JsonValue::Array(
                identity
                    .enabled_languages
                    .iter()
                    .map(|row| string(row))
                    .collect(),
            ),
        ),
        ("default_language", string(&identity.default_language)),
        ("source_packages", map_strings(&identity.source_packages)),
        (
            "packages",
            JsonValue::Array(
                project
                    .package_order
                    .iter()
                    .map(|id| string(id.as_str()))
                    .collect(),
            ),
        ),
        ("semantic", semantic(project, &identity.semantic_config)?),
        (
            "conversation",
            conversation(project, &identity.conversation_config)?,
        ),
        ("capabilities", capabilities(project)?),
        ("assets", assets(project)),
    ]);
    let bytes = canonical_json(&document)?;
    let digest_hex = sha256_hex(&bytes);
    Ok(CompiledIr {
        document,
        bytes,
        digest_hex,
    })
}
