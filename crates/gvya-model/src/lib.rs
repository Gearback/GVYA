//! Foundation domain vocabulary for GVYA.
//!
//! Shared data/identity vocabulary. Executable algorithms live in their owning kernel, compiler,
//! and runtime crates rather than in this model crate.

#![forbid(unsafe_code)]

use std::collections::BTreeMap;

/// Portable executable-program envelope shared by compiler and runtime. These are hard executable
/// ceilings, not authoring recommendations. A canonical compiler must not emit IR outside them and
/// a runtime must reject hand-built artifacts that exceed them.
pub const GVYA_PROGRAM_MAX_BYTES: usize = 32 * 1024 * 1024;
pub const GVYA_PROGRAM_MAX_DEPTH: usize = 64;
pub const GVYA_PROGRAM_MAX_NODES: usize = 1_000_000;
pub const GVYA_PROGRAM_MAX_COLLECTION_ENTRIES: usize = 50_000;
pub const GVYA_PROGRAM_MAX_STRING_BYTES: usize = 256 * 1024;
pub const GVYA_PROGRAM_MAX_PACKAGES: usize = 2_048;

macro_rules! string_id {
    ($name:ident) => {
        #[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
        pub struct $name(String);

        impl $name {
            /// Creates an ID without applying domain-specific syntax validation.
            ///
            /// Syntax validation is compiler-owned and deliberately absent from this identity wrapper.
            #[must_use]
            pub fn new(value: impl Into<String>) -> Self {
                Self(value.into())
            }

            /// Returns the underlying author-facing value.
            #[must_use]
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }
    };
}

string_id!(ProjectId);
string_id!(PackageId);
string_id!(PackageDigest);
string_id!(TypeId);
string_id!(TestCaseId);
string_id!(ScenarioId);
string_id!(AuditCode);
string_id!(WhySectionId);
string_id!(BrainId);
string_id!(BehaviorId);
string_id!(MeaningId);
string_id!(ResponseId);
string_id!(OpeningId);
string_id!(TopicId);
string_id!(FollowupId);
string_id!(AssetId);
string_id!(CapabilityId);
string_id!(CapabilityVersion);
string_id!(CapabilityBindingId);
string_id!(PolicyId);
string_id!(ConfirmationId);
string_id!(ReferenceKind);
string_id!(ReferenceId);
string_id!(ProposalId);
string_id!(TraceId);
string_id!(TraceCode);

/// Language-neutral JSON-like value used at the foundation boundary.
///
/// The executable implementation may later use a dedicated serialization crate;
/// this enum keeps the model boundary dependency-free and does not define the `.gvya` encoding.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Null,
    Bool(bool),
    Number(f64),
    String(String),
    Array(Vec<Value>),
    Object(BTreeMap<String, Value>),
}

/// A JSON Schema document represented opaquely until schema parsing is introduced by its owning schema layer.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SchemaDocument {
    canonical_source: String,
}

impl SchemaDocument {
    #[must_use]
    pub fn new(canonical_source: impl Into<String>) -> Self {
        Self {
            canonical_source: canonical_source.into(),
        }
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.canonical_source
    }
}

/// A host-owned object identity. Labels are not authority.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HostReference {
    pub kind: ReferenceKind,
    pub id: ReferenceId,
}

/// Provenance for a resolved slot/reference value.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ValueProvenance {
    Utterance,
    Context,
    ConversationState,
    HostReferenceResolver,
    NeuralProposal,
    AuthorRule,
}

/// A typed semantic value attached to a Meaning.
#[derive(Clone, Debug, PartialEq)]
pub struct SlotValue {
    pub name: String,
    pub value: Value,
    pub provenance: ValueProvenance,
}

/// Structured interpretation; not synonymous with a capability call.
#[derive(Clone, Debug, PartialEq)]
pub struct Meaning {
    pub id: MeaningId,
    pub slots: Vec<SlotValue>,
    pub references: Vec<HostReference>,
}

/// One required Meaning declaration that is still unsatisfied.
///
/// Slot kinds remain semantic-catalog authority; persisted collection state keeps only the
/// declaration identity needed to look that authority up again.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MissingRequiredValue {
    Slot { name: String },
    Reference { kind: ReferenceKind },
}

/// Semantic authority that selected a partial Meaning before collection began.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CollectionAuthority {
    StructuralPattern,
    Deterministic,
    ResolverProposal,
}

/// Canonical, serializable multi-turn value-collection state owned by the conversation.
#[derive(Clone, Debug, PartialEq)]
pub struct ActiveCollection {
    pub meaning: Meaning,
    pub remaining: Vec<MissingRequiredValue>,
    pub authority: CollectionAuthority,
    /// Deterministic lifecycle metadata; no wall-clock time enters collection semantics.
    pub started_turn: u64,
}

/// Effect/risk declaration used by authoring and deterministic admission.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EffectClass {
    Pure,
    Reversible,
    Irreversible,
    External,
}

/// Whether a capability contract has an author-declared confirmation expectation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConfirmationHint {
    Never,
    Conditional,
    Always,
}

/// Declares an operation the host may implement. It contains no executable host callback.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CapabilityContract {
    pub id: CapabilityId,
    pub version: CapabilityVersion,
    pub title: String,
    pub description: String,
    pub input_schema: SchemaDocument,
    pub output_schema: Option<SchemaDocument>,
    pub reference_kinds: Vec<ReferenceKind>,
    pub effect_class: EffectClass,
    pub confirmation_hint: ConfirmationHint,
}

/// One exact capability contract version exposed by the host for this interaction.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AvailableCapability {
    pub id: CapabilityId,
    pub version: CapabilityVersion,
}

/// Immutable host-supplied facts for one interaction.
#[derive(Clone, Debug, PartialEq)]
pub struct ContextSnapshot {
    pub values: BTreeMap<String, Value>,
    pub visible_references: Vec<HostReference>,
    pub available_capabilities: Vec<AvailableCapability>,
}

/// A currently active conversation topic. TTL is measured in completed user turns.
#[derive(Clone, Debug, PartialEq)]
pub struct ActiveTopic {
    pub id: TopicId,
    pub ttl: u32,
    pub source_behavior: Option<BehaviorId>,
}

/// A short-lived follow-up scope. TTL is measured in eligible turns after the opener.
#[derive(Clone, Debug, PartialEq)]
pub struct ActiveFollowup {
    pub id: FollowupId,
    pub ttl: u32,
    pub source_behavior: Option<BehaviorId>,
}

/// Consecutive-repeat state. Counts include the most recently committed turn.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RepeatMemory {
    pub last_user_normalized: String,
    pub last_meaning: Option<MeaningId>,
    pub same_input_count: u32,
    pub same_meaning_count: u32,
}

/// Consecutive repair state used to stage clarification responses.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RepairMemory {
    pub consecutive: u32,
    pub last_mode: Option<String>,
    pub last_candidate: Option<MeaningId>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Formality {
    Unknown,
    Formal,
    Informal,
}

impl Default for Formality {
    fn default() -> Self {
        Self::Unknown
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct UserStyle {
    pub formality: Formality,
    pub confidence: f64,
}

/// Runtime-owned conversation state. Author state is deliberately separate.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ConversationState {
    pub active_topic: Option<ActiveTopic>,
    pub active_followup: Option<ActiveFollowup>,
    pub active_collection: Option<ActiveCollection>,
    pub last_meaning: Option<MeaningId>,
    pub last_behavior: Option<BehaviorId>,
    /// Explicit interaction language currently owned by this conversation session. It is updated
    /// only from resolved request/session authority, never inferred from whichever response text
    /// happened to be selected.
    pub active_language: Option<String>,
    pub last_topic: Option<TopicId>,
    pub mentioned_topics: Vec<TopicId>,
    pub recent_response_ids: Vec<ResponseId>,
    pub recent_variant_keys: Vec<String>,
    pub recent_user_messages: Vec<String>,
    pub repeat_fallback_serial: u64,
    pub repeat_memory: RepeatMemory,
    pub repair: RepairMemory,
    pub hint_progress: BTreeMap<String, u32>,
    pub focus: Vec<HostReference>,
    pub user_style: UserStyle,
    /// Admitted host invocation proposals awaiting exactly one matching host result.
    /// The runtime owns this ledger; author state cannot address or mutate it.
    pub pending_capabilities: Vec<InvocationProposal>,
    pub turn_index: u64,
}

/// Author-addressable state and runtime-managed conversation state are intentionally separated.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GvyaState {
    pub author: BTreeMap<String, Value>,
    pub conversation: ConversationState,
}

/// Ordered, renderer-independent communication plan.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ResponsePlan {
    pub messages: Vec<ResponseMessage>,
}

/// One conversational message. Extra messages remain distinct rather than being flattened.
#[derive(Clone, Debug, PartialEq)]
pub struct ResponseMessage {
    pub source_response: Option<ResponseId>,
    pub kind: String,
    pub items: Vec<ResponseItem>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ResponseItem {
    Text {
        text: String,
        language: Option<String>,
    },
    Asset {
        asset_id: AssetId,
        alt_text: Option<String>,
    },
    Link {
        label: String,
        url: String,
    },
}

/// Result of deterministic capability admission.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AdmissionOutcome {
    Admitted,
    NeedsConfirmation { reason_code: String },
    Rejected { reason_code: String },
}

/// A host-visible request to execute a capability. It is not proof of execution.
#[derive(Clone, Debug, PartialEq)]
pub struct InvocationProposal {
    pub id: ProposalId,
    pub capability: CapabilityId,
    pub capability_version: CapabilityVersion,
    pub arguments: BTreeMap<String, Value>,
    /// Stable fingerprint of capability version + bound arguments. Confirmation grants bind to it.
    pub fingerprint: String,
    pub trace_id: TraceId,
}

/// Host round-trip proof that the user explicitly confirmed or declined an exact proposal.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConfirmationGrant {
    pub id: ConfirmationId,
    pub proposal_id: ProposalId,
    pub fingerprint: String,
    pub confirmed: bool,
}

/// Sensitivity controls what a human-facing Why surface may render by default.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TraceVisibility {
    Public,
    Author,
    Sensitive,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TraceEvent {
    pub code: TraceCode,
    pub phase: String,
    pub summary: String,
    pub visibility: TraceVisibility,
    pub details: BTreeMap<String, Value>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Trace {
    pub id: TraceId,
    pub events: Vec<TraceEvent>,
}

/// Human-facing Why groups are stable presentation semantics, not raw trace phases.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum WhySectionKind {
    Summary,
    Rejections,
    Understanding,
    Conversation,
    Capability,
    Context,
    Selected,
    Other,
}

/// Compact status used by Why surfaces before a reviewer drills into details.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WhyEntryStatus {
    Information,
    Accepted,
    Selected,
    Required,
    Rejected,
}

#[derive(Clone, Debug, PartialEq)]
pub struct WhyEntry {
    pub code: TraceCode,
    pub status: WhyEntryStatus,
    pub summary: String,
    pub visibility: TraceVisibility,
    pub details: BTreeMap<String, Value>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct WhySection {
    pub id: WhySectionId,
    pub kind: WhySectionKind,
    pub title: String,
    pub entries: Vec<WhyEntry>,
}

/// Renderer-independent progressive-disclosure explanation model.
#[derive(Clone, Debug, PartialEq)]
pub struct WhyReport {
    pub headline: String,
    pub sections: Vec<WhySection>,
    pub trace_ids: Vec<TraceId>,
    pub rejected_count: usize,
}
