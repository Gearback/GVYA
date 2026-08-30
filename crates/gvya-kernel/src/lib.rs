//! Foundation interaction and extension boundaries.
//!
//! The canonical semantic matcher/ranker lives here. Conversation behavior and capability
//! admission are separate owning layers.

#![forbid(unsafe_code)]

pub mod capability;
pub mod conversation;
pub mod semantic;
pub mod why;

use std::collections::BTreeMap;

use gvya_model::{
    ContextSnapshot, GvyaState, HostReference, MeaningId, ProposalId, ReferenceKind, SlotValue,
    Value,
};

#[derive(Clone, Debug, PartialEq)]
pub struct UtteranceInput {
    pub text: String,
    pub language: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CapabilityResultInput {
    pub proposal_id: ProposalId,
    pub succeeded: bool,
    pub output: Option<Value>,
    pub error_code: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ConfirmationDecision {
    Confirm,
    Decline,
}

#[derive(Clone, Debug, PartialEq)]
pub enum InteractionInput {
    Utterance(UtteranceInput),
    HostSignal {
        signal: String,
        payload: Value,
    },
    CapabilityResult(CapabilityResultInput),
    ConfirmationDecision {
        proposal_id: ProposalId,
        decision: ConfirmationDecision,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct InteractionRequest {
    pub input: InteractionInput,
    pub context: ContextSnapshot,
    pub state: GvyaState,
    /// Explicit seed when a future brain opts into deterministic seeded selection.
    pub seed: Option<u64>,
    /// Explicit host-supplied timestamp; the core never reads ambient wall clock for semantics.
    pub unix_time_ms: Option<i64>,
}

// ---------------------------------------------------------------------------------------------
// Optional external semantic resolver contract.
//
// GVYA owns semantic authority. An external resolver only ever sees a bounded, typed, resolver-safe
// projection of the deterministic candidate set, and only ever returns an untrusted proposal that
// the deterministic semantic firewall must independently validate. Nothing in this contract can
// select a Capability, reach host execution, or widen the candidate boundary.
// ---------------------------------------------------------------------------------------------

/// Maximum candidate Meanings any single resolver request may expose.
pub const RESOLVER_REQUEST_CANDIDATES_MAX: usize = 64;
/// Maximum semantic hints projected per candidate Meaning.
pub const RESOLVER_HINTS_PER_CANDIDATE_MAX: usize = 6;
/// Maximum bytes of one projected semantic hint.
pub const RESOLVER_HINT_MAX_BYTES: usize = 160;
/// Maximum slot declarations projected per candidate Meaning.
pub const RESOLVER_SLOTS_PER_CANDIDATE_MAX: usize = 32;
/// Maximum reference declarations projected per candidate Meaning.
pub const RESOLVER_REFERENCES_PER_CANDIDATE_MAX: usize = 16;
/// Maximum canonical entity values projected for one entity-typed declaration.
pub const RESOLVER_ENTITY_VALUES_PER_SLOT_MAX: usize = 32;
/// Maximum bytes of one projected canonical entity value.
pub const RESOLVER_ENTITY_VALUE_MAX_BYTES: usize = 96;
/// Maximum concrete host reference candidates exposed to a resolver.
pub const RESOLVER_REFERENCE_CANDIDATES_MAX: usize = 64;
/// Maximum explicitly exposed resolver context entries.
pub const RESOLVER_CONTEXT_ENTRIES_MAX: usize = 64;
/// Maximum collectable declarations exposed for an active collection.
pub const RESOLVER_COLLECTION_TARGETS_MAX: usize = 64;
/// Maximum already-bound collection values exposed as read-only context.
pub const RESOLVER_COLLECTION_BOUND_VALUES_MAX: usize = 64;
/// Maximum deterministic matched terms projected per candidate Meaning.
pub const RESOLVER_MATCHED_TERMS_MAX: usize = 8;
/// Maximum bytes of one projected matched term.
pub const RESOLVER_MATCHED_TERM_MAX_BYTES: usize = 64;

/// A host reference made visible to an optional semantic resolver.
///
/// Display aliases are resolver hints only. `reference.id` remains authoritative.
#[derive(Clone, Debug, PartialEq)]
pub struct ResolverReferenceCandidate {
    pub reference: HostReference,
    pub label: Option<String>,
    pub aliases: Vec<String>,
}

/// What GVYA is asking the resolver to do this turn.
///
/// The two tasks carry genuinely different authority, so an adapter never has to infer intent from
/// the shape of the candidate list.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ResolverTask {
    /// Choose at most one of the exposed candidate Meanings and propose its declared values.
    ResolveMeaning,
    /// The Meaning is already deterministic authority. Propose values only for the collectable
    /// declarations named in `ResolverRequest::collection`.
    FillCollection,
}

/// Canonical value authority projected for one entity-typed declaration.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolverEntitySchema {
    pub kind: String,
    /// Bounded canonical values the active language profile accepts for this kind.
    ///
    /// Empty means either an open canonical form (dates, times, numbers, e-mail, phone, URL,
    /// origin, quantity) or a kind with no authority at all in the active profile.
    pub canonical_values: Vec<String>,
    /// True when `canonical_values` is the complete accepted set for this kind and profile, so a
    /// constrained decoder may treat it as an enumeration. An empty exhaustive set means the
    /// declaration cannot be filled at all on this turn.
    pub values_are_exhaustive: bool,
}

/// Semantic value contract for one declaration. This is deliberately not a Capability JSON Schema:
/// the resolver interprets an utterance into Meaning values, it does not build host inputs.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ResolverValueKind {
    String,
    Number,
    Boolean,
    Entity(ResolverEntitySchema),
    Reference { kind: ReferenceKind },
}

/// One declared slot of a candidate Meaning.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolverSlotSchema {
    pub name: String,
    pub kind: ResolverValueKind,
    pub required: bool,
}

/// One declared host reference requirement of a candidate Meaning. Concrete legal reference IDs
/// are exposed separately in `ResolverRequest::reference_candidates`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolverReferenceSchema {
    pub kind: ReferenceKind,
    pub required: bool,
}

/// How a candidate entered the resolver set.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ResolverCandidateOrigin {
    /// Produced and scored by ordinary deterministic candidate retrieval.
    DeterministicMatch,
    /// Added by the separate broader high-recall resolver retrieval stage.
    ResolverRecall,
    /// The already-authoritative Meaning of the active collection.
    ActiveCollection,
}

/// Coarse normalized deterministic evidence band.
///
/// Raw matcher scores are tuning-sensitive and are not stable public semantics, so the resolver
/// sees a normalized band instead. The band carries no authority: it cannot widen the candidate
/// boundary or relax any validation.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum ResolverEvidenceStrength {
    None,
    Weak,
    Moderate,
    Strong,
}

/// Why the deterministic matcher produced this candidate.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolverCandidateEvidence {
    /// `Strong` means this candidate independently reached the configured resolution floor.
    pub semantic: ResolverEvidenceStrength,
    /// Authored lexical retrieval authority for this candidate.
    pub retrieval: ResolverEvidenceStrength,
    /// Bounded normalized input terms that matched the projected hints.
    pub matched_terms: Vec<String>,
}

/// Where one projected semantic hint came from.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ResolverHintKind {
    Sample,
    RetrievalTerm,
    StructuralPattern,
}

/// One bounded natural-language hint derived from authored semantic evidence.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolverSemanticHint {
    pub kind: ResolverHintKind,
    pub language: String,
    pub text: String,
}

/// The resolver-safe projection of one candidate Meaning.
#[derive(Clone, Debug, PartialEq)]
pub struct ResolverMeaningCandidate {
    pub meaning: MeaningId,
    pub origin: ResolverCandidateOrigin,
    /// Absent for a candidate the deterministic matcher did not produce, such as the already
    /// authoritative Meaning of an active collection.
    pub evidence: Option<ResolverCandidateEvidence>,
    pub hints: Vec<ResolverSemanticHint>,
    pub slots: Vec<ResolverSlotSchema>,
    pub references: Vec<ResolverReferenceSchema>,
}

/// One declaration the active collection may still legally collect this turn.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ResolverCollectionTarget {
    Slot(ResolverSlotSchema),
    Reference(ResolverReferenceSchema),
}

/// Canonical read-only projection of the active collection.
///
/// `bound_slots`/`bound_references` are interpretation context only. The semantic firewall rejects
/// any proposal that tries to change them, and `collectable` is derived from the same
/// `ActiveCollection` state the firewall later validates against.
#[derive(Clone, Debug, PartialEq)]
pub struct ResolverCollectionContext {
    pub meaning: MeaningId,
    pub bound_slots: Vec<SlotValue>,
    pub bound_references: Vec<HostReference>,
    pub collectable: Vec<ResolverCollectionTarget>,
}

/// Explicit privacy/authority boundary supplied to an optional external semantic resolver.
///
/// The resolver does not receive the full `InteractionRequest`, arbitrary GVYA state, the host
/// Capability catalog or any host execution hook. The deterministic kernel decides what is safe
/// and relevant to expose, and every field here is bounded.
#[derive(Clone, Debug, PartialEq)]
pub struct ResolverRequest {
    pub task: ResolverTask,
    pub utterance: UtteranceInput,
    /// The semantic language fallback order actually used for this turn's matching.
    pub language_fallbacks: Vec<String>,
    /// The complete candidate boundary. A proposal may name no other Meaning.
    pub candidates: Vec<ResolverMeaningCandidate>,
    /// Present exactly when `task` is `FillCollection`.
    pub collection: Option<ResolverCollectionContext>,
    /// The complete host reference boundary. A proposal may name no other reference ID.
    pub reference_candidates: Vec<ResolverReferenceCandidate>,
    pub exposed_context: BTreeMap<String, Value>,
}

impl ResolverRequest {
    /// The candidate Meaning boundary. There is no catalog fallback, fuzzy ID recovery or
    /// evidence-driven widening anywhere downstream of this test.
    #[must_use]
    pub fn permits_meaning(&self, meaning: &MeaningId) -> bool {
        self.candidates.iter().any(|row| &row.meaning == meaning)
    }

    #[must_use]
    pub fn candidate(&self, meaning: &MeaningId) -> Option<&ResolverMeaningCandidate> {
        self.candidates.iter().find(|row| &row.meaning == meaning)
    }

    /// The concrete host reference boundary for one declared kind.
    #[must_use]
    pub fn exposes_reference(&self, kind: &ReferenceKind, id: &str) -> bool {
        self.reference_candidates
            .iter()
            .any(|row| row.reference.kind == *kind && row.reference.id.as_str() == id)
    }
}

/// Untrusted output from an optional external semantic resolver.
///
/// There is deliberately no Capability field: Capability identity comes only from authored GVYA
/// bindings applied after semantic validation, so a resolver cannot influence execution at all.
#[derive(Clone, Debug, PartialEq)]
pub struct ResolverProposal {
    pub meaning: Option<MeaningId>,
    pub slots: Vec<SlotValue>,
    pub references: Vec<HostReference>,
    /// Bounded evidence, not a calibrated probability, and never a bypass for any deterministic
    /// rule. See `SemanticConfig::resolver_min_confidence`.
    pub confidence: Option<f32>,
    pub evidence: Vec<String>,
}

/// External structured resolver adapters implement this role. Deterministic validation of every
/// proposal remains downstream and mandatory.
pub trait SemanticResolver {
    type Error;

    fn propose(&self, request: &ResolverRequest) -> Result<ResolverProposal, Self::Error>;
}
