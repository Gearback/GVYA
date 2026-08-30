//! Provider-neutral JSON bridge to an optional external structured semantic resolver.
//!
//! The host owns transport, credentials, model selection, timeout and availability policy. This
//! module only serializes the deliberately bounded `ResolverRequest` and parses an untrusted
//! proposal back. It is not specific to any provider: a small local structured model, another
//! local classifier/extractor, a deterministic experimental resolver and a plain test double all
//! satisfy the same contract.
//!
//! Nothing here is semantic authority. The semantic kernel independently re-validates candidate
//! Meaning, declaration ownership, canonical value types, custom entity membership, host reference
//! exposure, collection boundaries, required-value completeness and confidence. Capability binding
//! is not part of this contract at all.

use std::collections::BTreeMap;

use gvya_kernel::{
    RESOLVER_COLLECTION_BOUND_VALUES_MAX, RESOLVER_COLLECTION_TARGETS_MAX,
    RESOLVER_ENTITY_VALUE_MAX_BYTES, RESOLVER_ENTITY_VALUES_PER_SLOT_MAX, RESOLVER_HINT_MAX_BYTES,
    RESOLVER_HINTS_PER_CANDIDATE_MAX, RESOLVER_MATCHED_TERM_MAX_BYTES, RESOLVER_MATCHED_TERMS_MAX,
    RESOLVER_REFERENCE_CANDIDATES_MAX, RESOLVER_REFERENCES_PER_CANDIDATE_MAX,
    RESOLVER_REQUEST_CANDIDATES_MAX, RESOLVER_SLOTS_PER_CANDIDATE_MAX, ResolverCandidateEvidence,
    ResolverCandidateOrigin, ResolverCollectionContext, ResolverCollectionTarget,
    ResolverEvidenceStrength, ResolverHintKind, ResolverMeaningCandidate, ResolverProposal,
    ResolverReferenceSchema, ResolverRequest, ResolverSlotSchema, ResolverTask, ResolverValueKind,
    SemanticResolver,
};
use gvya_model::{
    HostReference, MeaningId, ReferenceId, ReferenceKind, SlotValue, Value, ValueProvenance,
};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

/// Byte and collection ceilings applied at the JSON boundary itself.
///
/// Per-candidate projection ceilings are not repeated here: they are the canonical kernel
/// constants, and this bridge enforces exactly those so there is one source of truth.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SemanticResolverLimits {
    pub max_request_bytes: usize,
    pub max_response_bytes: usize,
    pub max_candidates: usize,
    pub max_context_entries: usize,
    pub max_slots: usize,
    pub max_references: usize,
    pub max_evidence: usize,
    pub max_string_bytes: usize,
}

impl Default for SemanticResolverLimits {
    fn default() -> Self {
        Self {
            max_request_bytes: 256 * 1024,
            max_response_bytes: 256 * 1024,
            max_candidates: RESOLVER_REQUEST_CANDIDATES_MAX,
            max_context_entries: 64,
            max_slots: 64,
            max_references: 64,
            max_evidence: 32,
            max_string_bytes: 16 * 1024,
        }
    }
}

/// A `SemanticResolver` implemented by a host-owned `String -> String` callback.
pub struct JsonSemanticResolver<F> {
    invoke: F,
    limits: SemanticResolverLimits,
}

impl<F> JsonSemanticResolver<F> {
    pub fn new(invoke: F) -> Self {
        Self {
            invoke,
            limits: SemanticResolverLimits::default(),
        }
    }

    pub fn with_limits(invoke: F, limits: SemanticResolverLimits) -> Self {
        Self { invoke, limits }
    }
}

impl<F> SemanticResolver for JsonSemanticResolver<F>
where
    F: Fn(&str) -> Result<String, String>,
{
    type Error = String;

    fn propose(&self, request: &ResolverRequest) -> Result<ResolverProposal, Self::Error> {
        let encoded = encode_request(request, self.limits)?;
        let raw = (self.invoke)(&encoded)?;
        decode_proposal(&raw, self.limits)
    }
}

// -------------------------------------------------------------------------------------------
// Request document
// -------------------------------------------------------------------------------------------

#[derive(Serialize)]
struct RequestDoc {
    format: &'static str,
    version: u32,
    task: &'static str,
    utterance: UtteranceDoc,
    language_fallbacks: Vec<String>,
    candidates: Vec<CandidateDoc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    collection: Option<CollectionDoc>,
    reference_candidates: Vec<ReferenceCandidateDoc>,
    exposed_context: BTreeMap<String, JsonValue>,
    authority: AuthorityDoc,
}

#[derive(Serialize)]
struct UtteranceDoc {
    text: String,
    language: Option<String>,
}

#[derive(Serialize)]
struct CandidateDoc {
    meaning: String,
    origin: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    evidence: Option<EvidenceDoc>,
    hints: Vec<HintDoc>,
    slots: Vec<SlotSchemaDoc>,
    references: Vec<ReferenceSchemaDoc>,
}

#[derive(Serialize)]
struct EvidenceDoc {
    semantic: &'static str,
    retrieval: &'static str,
    matched_terms: Vec<String>,
}

#[derive(Serialize)]
struct HintDoc {
    kind: &'static str,
    language: String,
    text: String,
}

#[derive(Serialize)]
struct SlotSchemaDoc {
    name: String,
    required: bool,
    kind: ValueKindDoc,
}

#[derive(Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ValueKindDoc {
    String,
    Number,
    Boolean,
    Entity {
        entity_kind: String,
        canonical_values: Vec<String>,
        values_are_exhaustive: bool,
    },
    Reference {
        reference_kind: String,
    },
}

#[derive(Serialize)]
struct ReferenceSchemaDoc {
    kind: String,
    required: bool,
}

#[derive(Serialize)]
struct CollectionDoc {
    meaning: String,
    bound_slots: Vec<BoundSlotDoc>,
    bound_references: Vec<ReferenceDoc>,
    collectable: Vec<CollectionTargetDoc>,
}

/// Already-bound values are read-only interpretation context. Provenance is deliberately not
/// projected: it is internal deterministic bookkeeping, not something a resolver may act on.
#[derive(Serialize)]
struct BoundSlotDoc {
    name: String,
    value: JsonValue,
}

#[derive(Serialize)]
#[serde(tag = "target", rename_all = "snake_case")]
enum CollectionTargetDoc {
    Slot {
        name: String,
        required: bool,
        kind: ValueKindDoc,
    },
    Reference {
        kind: String,
        required: bool,
    },
}

#[derive(Serialize)]
struct ReferenceCandidateDoc {
    kind: String,
    id: String,
    label: Option<String>,
    aliases: Vec<String>,
}

/// The machine-readable authority declaration.
///
/// These are constants because the typed request already makes the corresponding violations
/// impossible to express; they exist so an adapter can generate a correctly constrained schema
/// without re-deriving GVYA policy. There is intentionally no Capability statement: Capability is
/// absent from the contract entirely rather than present and disclaimed.
#[derive(Serialize)]
struct AuthorityDoc {
    candidate_meanings_are_exhaustive: bool,
    declared_values_only: bool,
    exposed_references_are_exhaustive: bool,
    deterministic_review_required: bool,
}

// -------------------------------------------------------------------------------------------
// Proposal document
// -------------------------------------------------------------------------------------------

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ProposalDoc {
    #[serde(default)]
    meaning: Option<String>,
    #[serde(default)]
    slots: Vec<SlotDoc>,
    #[serde(default)]
    references: Vec<ReferenceDoc>,
    #[serde(default)]
    confidence: Option<f32>,
    #[serde(default)]
    evidence: Vec<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SlotDoc {
    name: String,
    value: JsonValue,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ReferenceDoc {
    kind: String,
    id: String,
}

// -------------------------------------------------------------------------------------------
// Encoding
// -------------------------------------------------------------------------------------------

fn task_name(task: ResolverTask) -> &'static str {
    match task {
        ResolverTask::ResolveMeaning => "resolve_meaning",
        ResolverTask::FillCollection => "fill_collection",
    }
}

fn origin_name(origin: ResolverCandidateOrigin) -> &'static str {
    match origin {
        ResolverCandidateOrigin::DeterministicMatch => "deterministic_match",
        ResolverCandidateOrigin::ResolverRecall => "resolver_recall",
        ResolverCandidateOrigin::ActiveCollection => "active_collection",
    }
}

fn strength_name(strength: ResolverEvidenceStrength) -> &'static str {
    match strength {
        ResolverEvidenceStrength::None => "none",
        ResolverEvidenceStrength::Weak => "weak",
        ResolverEvidenceStrength::Moderate => "moderate",
        ResolverEvidenceStrength::Strong => "strong",
    }
}

fn hint_kind_name(kind: ResolverHintKind) -> &'static str {
    match kind {
        ResolverHintKind::Sample => "sample",
        ResolverHintKind::RetrievalTerm => "retrieval_term",
        ResolverHintKind::StructuralPattern => "structural_pattern",
    }
}

fn encode_request(
    request: &ResolverRequest,
    limits: SemanticResolverLimits,
) -> Result<String, String> {
    validate_text(&request.utterance.text, limits, "utterance")?;
    if let Some(language) = &request.utterance.language {
        validate_text(language, limits, "language")?;
    }
    // Task and collection context must agree structurally, so an adapter can never be handed a
    // collection fill with no collection state or an ordinary turn carrying one.
    match (request.task, request.collection.is_some()) {
        (ResolverTask::FillCollection, false) => {
            return Err("fill_collection request carries no collection context".into());
        }
        (ResolverTask::ResolveMeaning, true) => {
            return Err("resolve_meaning request must not carry collection context".into());
        }
        _ => {}
    }
    if request.candidates.len() > RESOLVER_REQUEST_CANDIDATES_MAX
        || request.candidates.len() > limits.max_candidates
    {
        return Err("resolver request contains too many candidate meanings".into());
    }
    if request.reference_candidates.len() > RESOLVER_REFERENCE_CANDIDATES_MAX
        || request.reference_candidates.len() > limits.max_references
    {
        return Err("resolver request contains too many reference candidates".into());
    }
    if request.exposed_context.len() > limits.max_context_entries {
        return Err("resolver request contains too many exposed context entries".into());
    }

    let candidates = request
        .candidates
        .iter()
        .map(|row| candidate_doc(row, limits))
        .collect::<Result<Vec<_>, String>>()?;
    let collection = request
        .collection
        .as_ref()
        .map(|row| collection_doc(row, limits))
        .transpose()?;
    let mut context = BTreeMap::new();
    for (key, value) in &request.exposed_context {
        validate_text(key, limits, "context key")?;
        context.insert(key.clone(), value_to_json(value, limits)?);
    }
    let references = request
        .reference_candidates
        .iter()
        .map(|row| {
            validate_text(row.reference.kind.as_str(), limits, "reference kind")?;
            validate_text(row.reference.id.as_str(), limits, "reference id")?;
            if let Some(label) = &row.label {
                validate_text(label, limits, "reference label")?;
            }
            for alias in &row.aliases {
                validate_text(alias, limits, "reference alias")?;
            }
            Ok(ReferenceCandidateDoc {
                kind: row.reference.kind.as_str().to_string(),
                id: row.reference.id.as_str().to_string(),
                label: row.label.clone(),
                aliases: row.aliases.clone(),
            })
        })
        .collect::<Result<Vec<_>, String>>()?;

    let doc = RequestDoc {
        format: "gvya.semantic.resolver.request",
        version: 1,
        task: task_name(request.task),
        utterance: UtteranceDoc {
            text: request.utterance.text.clone(),
            language: request.utterance.language.clone(),
        },
        language_fallbacks: request.language_fallbacks.clone(),
        candidates,
        collection,
        reference_candidates: references,
        exposed_context: context,
        authority: AuthorityDoc {
            candidate_meanings_are_exhaustive: true,
            declared_values_only: true,
            exposed_references_are_exhaustive: true,
            deterministic_review_required: true,
        },
    };
    let encoded = serde_json::to_string(&doc)
        .map_err(|error| format!("resolver request serialization failed: {error}"))?;
    if encoded.len() > limits.max_request_bytes {
        return Err("resolver request exceeds configured byte limit".into());
    }
    Ok(encoded)
}

fn candidate_doc(
    candidate: &ResolverMeaningCandidate,
    limits: SemanticResolverLimits,
) -> Result<CandidateDoc, String> {
    validate_text(candidate.meaning.as_str(), limits, "candidate meaning")?;
    if candidate.hints.len() > RESOLVER_HINTS_PER_CANDIDATE_MAX {
        return Err("resolver candidate contains too many hints".into());
    }
    if candidate.slots.len() > RESOLVER_SLOTS_PER_CANDIDATE_MAX {
        return Err("resolver candidate contains too many slot declarations".into());
    }
    if candidate.references.len() > RESOLVER_REFERENCES_PER_CANDIDATE_MAX {
        return Err("resolver candidate contains too many reference declarations".into());
    }
    let mut hints = Vec::new();
    for hint in &candidate.hints {
        if hint.text.len() > RESOLVER_HINT_MAX_BYTES {
            return Err("resolver candidate hint exceeds the projection byte limit".into());
        }
        validate_text(&hint.language, limits, "hint language")?;
        validate_text(&hint.text, limits, "hint text")?;
        hints.push(HintDoc {
            kind: hint_kind_name(hint.kind),
            language: hint.language.clone(),
            text: hint.text.clone(),
        });
    }
    Ok(CandidateDoc {
        meaning: candidate.meaning.as_str().to_string(),
        origin: origin_name(candidate.origin),
        evidence: candidate
            .evidence
            .as_ref()
            .map(|row| evidence_doc(row, limits))
            .transpose()?,
        hints,
        slots: candidate
            .slots
            .iter()
            .map(|spec| slot_schema_doc(spec, limits))
            .collect::<Result<Vec<_>, String>>()?,
        references: candidate
            .references
            .iter()
            .map(|spec| reference_schema_doc(spec, limits))
            .collect::<Result<Vec<_>, String>>()?,
    })
}

fn evidence_doc(
    evidence: &ResolverCandidateEvidence,
    limits: SemanticResolverLimits,
) -> Result<EvidenceDoc, String> {
    if evidence.matched_terms.len() > RESOLVER_MATCHED_TERMS_MAX {
        return Err("resolver candidate exposes too many matched terms".into());
    }
    for term in &evidence.matched_terms {
        if term.len() > RESOLVER_MATCHED_TERM_MAX_BYTES {
            return Err("resolver matched term exceeds the projection byte limit".into());
        }
        validate_text(term, limits, "matched term")?;
    }
    Ok(EvidenceDoc {
        semantic: strength_name(evidence.semantic),
        retrieval: strength_name(evidence.retrieval),
        matched_terms: evidence.matched_terms.clone(),
    })
}

fn slot_schema_doc(
    spec: &ResolverSlotSchema,
    limits: SemanticResolverLimits,
) -> Result<SlotSchemaDoc, String> {
    validate_text(&spec.name, limits, "slot name")?;
    Ok(SlotSchemaDoc {
        name: spec.name.clone(),
        required: spec.required,
        kind: value_kind_doc(&spec.kind, limits)?,
    })
}

fn reference_schema_doc(
    spec: &ResolverReferenceSchema,
    limits: SemanticResolverLimits,
) -> Result<ReferenceSchemaDoc, String> {
    validate_text(spec.kind.as_str(), limits, "reference declaration kind")?;
    Ok(ReferenceSchemaDoc {
        kind: spec.kind.as_str().to_string(),
        required: spec.required,
    })
}

fn value_kind_doc(
    kind: &ResolverValueKind,
    limits: SemanticResolverLimits,
) -> Result<ValueKindDoc, String> {
    Ok(match kind {
        ResolverValueKind::String => ValueKindDoc::String,
        ResolverValueKind::Number => ValueKindDoc::Number,
        ResolverValueKind::Boolean => ValueKindDoc::Boolean,
        ResolverValueKind::Reference { kind } => {
            validate_text(kind.as_str(), limits, "reference kind")?;
            ValueKindDoc::Reference {
                reference_kind: kind.as_str().to_string(),
            }
        }
        ResolverValueKind::Entity(schema) => {
            validate_text(&schema.kind, limits, "entity kind")?;
            if schema.canonical_values.len() > RESOLVER_ENTITY_VALUES_PER_SLOT_MAX {
                return Err("resolver entity schema exposes too many canonical values".into());
            }
            for value in &schema.canonical_values {
                if value.len() > RESOLVER_ENTITY_VALUE_MAX_BYTES {
                    return Err("resolver canonical entity value exceeds the byte limit".into());
                }
                validate_text(value, limits, "canonical entity value")?;
            }
            ValueKindDoc::Entity {
                entity_kind: schema.kind.clone(),
                canonical_values: schema.canonical_values.clone(),
                values_are_exhaustive: schema.values_are_exhaustive,
            }
        }
    })
}

fn collection_doc(
    collection: &ResolverCollectionContext,
    limits: SemanticResolverLimits,
) -> Result<CollectionDoc, String> {
    validate_text(collection.meaning.as_str(), limits, "collection meaning")?;
    if collection.bound_slots.len() > RESOLVER_COLLECTION_BOUND_VALUES_MAX
        || collection.bound_references.len() > RESOLVER_COLLECTION_BOUND_VALUES_MAX
    {
        return Err("resolver collection context exposes too many bound values".into());
    }
    if collection.collectable.len() > RESOLVER_COLLECTION_TARGETS_MAX {
        return Err("resolver collection context exposes too many collectable targets".into());
    }
    let mut bound_slots = Vec::new();
    for slot in &collection.bound_slots {
        validate_text(&slot.name, limits, "bound slot name")?;
        bound_slots.push(BoundSlotDoc {
            name: slot.name.clone(),
            value: value_to_json(&slot.value, limits)?,
        });
    }
    let mut bound_references = Vec::new();
    for reference in &collection.bound_references {
        validate_text(reference.kind.as_str(), limits, "bound reference kind")?;
        validate_text(reference.id.as_str(), limits, "bound reference id")?;
        bound_references.push(ReferenceDoc {
            kind: reference.kind.as_str().to_string(),
            id: reference.id.as_str().to_string(),
        });
    }
    let mut collectable = Vec::new();
    for target in &collection.collectable {
        collectable.push(match target {
            ResolverCollectionTarget::Slot(spec) => {
                validate_text(&spec.name, limits, "collectable slot name")?;
                CollectionTargetDoc::Slot {
                    name: spec.name.clone(),
                    required: spec.required,
                    kind: value_kind_doc(&spec.kind, limits)?,
                }
            }
            ResolverCollectionTarget::Reference(spec) => {
                validate_text(spec.kind.as_str(), limits, "collectable reference kind")?;
                CollectionTargetDoc::Reference {
                    kind: spec.kind.as_str().to_string(),
                    required: spec.required,
                }
            }
        });
    }
    Ok(CollectionDoc {
        meaning: collection.meaning.as_str().to_string(),
        bound_slots,
        bound_references,
        collectable,
    })
}

// -------------------------------------------------------------------------------------------
// Decoding
// -------------------------------------------------------------------------------------------

fn decode_proposal(raw: &str, limits: SemanticResolverLimits) -> Result<ResolverProposal, String> {
    if raw.len() > limits.max_response_bytes {
        return Err("resolver response exceeds configured byte limit".into());
    }
    // A proposal is a JSON object. Serde would otherwise happily build the struct from a
    // positional sequence, which is not part of this contract.
    let value: JsonValue = serde_json::from_str(raw)
        .map_err(|error| format!("resolver response is not strict proposal JSON: {error}"))?;
    if !value.is_object() {
        return Err("resolver response is not strict proposal JSON: expected an object".into());
    }
    let doc: ProposalDoc = serde_json::from_value(value)
        .map_err(|error| format!("resolver response is not strict proposal JSON: {error}"))?;
    if doc.slots.len() > limits.max_slots {
        return Err("resolver response contains too many slots".into());
    }
    if doc.references.len() > limits.max_references {
        return Err("resolver response contains too many references".into());
    }
    if doc.evidence.len() > limits.max_evidence {
        return Err("resolver response contains too many evidence rows".into());
    }
    if let Some(confidence) = doc.confidence {
        if !confidence.is_finite() || !(0.0..=1.0).contains(&confidence) {
            return Err("resolver confidence must be finite and inside 0..=1".into());
        }
    }
    if let Some(value) = &doc.meaning {
        validate_text(value, limits, "meaning")?;
    }
    for value in &doc.evidence {
        validate_text(value, limits, "evidence")?;
    }
    let slots = doc
        .slots
        .into_iter()
        .map(|row| {
            validate_text(&row.name, limits, "slot name")?;
            Ok(SlotValue {
                name: row.name,
                value: json_to_value(row.value, limits)?,
                provenance: ValueProvenance::NeuralProposal,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    let references = doc
        .references
        .into_iter()
        .map(|row| {
            validate_text(&row.kind, limits, "reference kind")?;
            validate_text(&row.id, limits, "reference id")?;
            Ok(HostReference {
                kind: ReferenceKind::new(row.kind),
                id: ReferenceId::new(row.id),
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok(ResolverProposal {
        meaning: doc.meaning.map(MeaningId::new),
        slots,
        references,
        confidence: doc.confidence,
        evidence: doc.evidence,
    })
}

fn validate_text(value: &str, limits: SemanticResolverLimits, label: &str) -> Result<(), String> {
    if value.len() > limits.max_string_bytes {
        return Err(format!("{label} exceeds configured string limit"));
    }
    if value.contains('\0') {
        return Err(format!("{label} contains NUL"));
    }
    Ok(())
}

fn value_to_json(value: &Value, limits: SemanticResolverLimits) -> Result<JsonValue, String> {
    match value {
        Value::Null => Ok(JsonValue::Null),
        Value::Bool(value) => Ok(JsonValue::Bool(*value)),
        Value::Number(value) => serde_json::Number::from_f64(*value)
            .map(JsonValue::Number)
            .ok_or_else(|| "non-finite value cannot cross the resolver boundary".into()),
        Value::String(value) => {
            validate_text(value, limits, "context string")?;
            Ok(JsonValue::String(value.clone()))
        }
        Value::Array(values) => values
            .iter()
            .map(|value| value_to_json(value, limits))
            .collect::<Result<Vec<_>, _>>()
            .map(JsonValue::Array),
        Value::Object(values) => values
            .iter()
            .map(|(key, value)| {
                validate_text(key, limits, "context object key")?;
                Ok((key.clone(), value_to_json(value, limits)?))
            })
            .collect::<Result<serde_json::Map<_, _>, String>>()
            .map(JsonValue::Object),
    }
}

fn json_to_value(value: JsonValue, limits: SemanticResolverLimits) -> Result<Value, String> {
    match value {
        JsonValue::Null => Ok(Value::Null),
        JsonValue::Bool(value) => Ok(Value::Bool(value)),
        JsonValue::Number(value) => value
            .as_f64()
            .filter(|number| number.is_finite())
            .map(Value::Number)
            .ok_or_else(|| "resolver number must be finite".into()),
        JsonValue::String(value) => {
            validate_text(&value, limits, "resolver value string")?;
            Ok(Value::String(value))
        }
        JsonValue::Array(values) => values
            .into_iter()
            .map(|value| json_to_value(value, limits))
            .collect::<Result<Vec<_>, _>>()
            .map(Value::Array),
        JsonValue::Object(values) => values
            .into_iter()
            .map(|(key, value)| {
                validate_text(&key, limits, "resolver value object key")?;
                Ok((key, json_to_value(value, limits)?))
            })
            .collect::<Result<BTreeMap<_, _>, String>>()
            .map(Value::Object),
    }
}

#[cfg(test)]
mod tests;
