//! Strict runtime wire DTOs and conversion.

use super::*;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CapabilityResultRequestDoc {
    pub(super) format: String,
    pub(super) version: u32,
    pub(super) proposal: InvocationProposalDoc,
    pub(super) result: CapabilityResultDoc,
    #[serde(default)]
    pub(super) context: ContextDoc,
    #[serde(default)]
    pub(super) state: Option<StateDoc>,
    #[serde(default)]
    pub(super) system: BTreeMap<String, JsonValue>,
    pub(super) seed: Option<u64>,
    #[serde(default)]
    pub(super) confirmations: Vec<ConfirmationDoc>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct InvocationProposalDoc {
    id: String,
    capability: String,
    capability_version: String,
    #[serde(default)]
    arguments: BTreeMap<String, JsonValue>,
    fingerprint: String,
    trace_id: String,
}
impl InvocationProposalDoc {
    pub(super) fn into_runtime(self) -> Result<InvocationProposal, WireError> {
        if self.id.trim().is_empty()
            || self.capability.trim().is_empty()
            || self.capability_version.trim().is_empty()
            || self.fingerprint.trim().is_empty()
            || self.trace_id.trim().is_empty()
        {
            return Err(WireError::Invalid(
                "capability result proposal contains an empty required identity field".into(),
            ));
        }
        Ok(InvocationProposal {
            id: ProposalId::new(self.id),
            capability: CapabilityId::new(self.capability),
            capability_version: CapabilityVersion::new(self.capability_version),
            arguments: map_values(self.arguments)?,
            fingerprint: self.fingerprint,
            trace_id: TraceId::new(self.trace_id),
        })
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CapabilityResultDoc {
    proposal_id: String,
    succeeded: bool,
    output: Option<JsonValue>,
    error_code: Option<String>,
}
impl CapabilityResultDoc {
    pub(super) fn into_runtime(self) -> Result<CapabilityResultInput, WireError> {
        if self.proposal_id.trim().is_empty() {
            return Err(WireError::Invalid(
                "capability result proposal_id is empty".into(),
            ));
        }
        Ok(CapabilityResultInput {
            proposal_id: ProposalId::new(self.proposal_id),
            succeeded: self.succeeded,
            output: self.output.as_ref().map(model_value).transpose()?,
            error_code: self.error_code,
        })
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct OpenRequestDoc {
    pub(super) format: String,
    pub(super) version: u32,
    #[serde(default)]
    pub(super) context: ContextDoc,
    #[serde(default)]
    pub(super) state: Option<StateDoc>,
    #[serde(default)]
    pub(super) system: BTreeMap<String, JsonValue>,
    pub(super) seed: Option<u64>,
    #[serde(default)]
    pub(super) confirmations: Vec<ConfirmationDoc>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct TurnRequestDoc {
    pub(super) format: String,
    pub(super) version: u32,
    pub(super) utterance: UtteranceDoc,
    #[serde(default)]
    pub(super) context: ContextDoc,
    #[serde(default)]
    pub(super) state: Option<StateDoc>,
    #[serde(default)]
    pub(super) reference_candidates: Vec<ReferenceCandidateDoc>,
    #[serde(default)]
    pub(super) resolver_context: BTreeMap<String, JsonValue>,
    #[serde(default)]
    pub(super) system: BTreeMap<String, JsonValue>,
    #[serde(default)]
    pub(super) hint: HintDoc,
    pub(super) seed: Option<u64>,
    #[serde(default)]
    pub(super) confirmations: Vec<ConfirmationDoc>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct UtteranceDoc {
    pub(super) text: String,
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub(super) struct ContextDoc {
    #[serde(default)]
    values: BTreeMap<String, JsonValue>,
    #[serde(default)]
    pub(super) visible_references: Vec<HostReferenceDoc>,
    #[serde(default)]
    pub(super) available_capabilities: Vec<AvailableCapabilityDoc>,
}
impl ContextDoc {
    pub(super) fn into_runtime(self) -> Result<ContextSnapshot, WireError> {
        Ok(ContextSnapshot {
            values: map_values(self.values)?,
            visible_references: self
                .visible_references
                .into_iter()
                .map(HostReferenceDoc::into_runtime)
                .collect(),
            available_capabilities: self
                .available_capabilities
                .into_iter()
                .map(|row| AvailableCapability {
                    id: CapabilityId::new(row.id),
                    version: CapabilityVersion::new(row.version),
                })
                .collect(),
        })
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct AvailableCapabilityDoc {
    id: String,
    version: String,
}
#[derive(Deserialize, Clone)]
#[serde(deny_unknown_fields)]
pub(super) struct HostReferenceDoc {
    kind: String,
    id: String,
}
impl HostReferenceDoc {
    pub(super) fn into_runtime(self) -> HostReference {
        HostReference {
            kind: ReferenceKind::new(self.kind),
            id: ReferenceId::new(self.id),
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ReferenceCandidateDoc {
    reference: HostReferenceDoc,
    label: Option<String>,
    #[serde(default)]
    aliases: Vec<String>,
}
impl ReferenceCandidateDoc {
    pub(super) fn into_runtime(self) -> Result<ResolverReferenceCandidate, WireError> {
        Ok(ResolverReferenceCandidate {
            reference: self.reference.into_runtime(),
            label: self.label,
            aliases: self.aliases,
        })
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct HintDoc {
    #[serde(rename = "type")]
    kind: String,
    level: Option<u32>,
}
impl Default for HintDoc {
    fn default() -> Self {
        Self {
            kind: "none".into(),
            level: None,
        }
    }
}
impl HintDoc {
    pub(super) fn into_runtime(self) -> Result<HintRequest, WireError> {
        match self.kind.as_str() {
            "none" => Ok(HintRequest::None),
            "first" => Ok(HintRequest::First),
            "next" => Ok(HintRequest::Next),
            "auto" => Ok(HintRequest::Auto),
            "direct" => self
                .level
                .map(HintRequest::Direct)
                .ok_or_else(|| WireError::Invalid("direct hint requires level".into())),
            _ => Err(WireError::Invalid(format!(
                "unknown hint type: {}",
                self.kind
            ))),
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ConfirmationDoc {
    id: String,
    proposal_id: String,
    fingerprint: String,
    confirmed: bool,
}
impl ConfirmationDoc {
    pub(super) fn into_runtime(self) -> ConfirmationGrant {
        ConfirmationGrant {
            id: ConfirmationId::new(self.id),
            proposal_id: ProposalId::new(self.proposal_id),
            fingerprint: self.fingerprint,
            confirmed: self.confirmed,
        }
    }
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub(super) struct StateDoc {
    #[serde(default)]
    author: BTreeMap<String, JsonValue>,
    #[serde(default)]
    pub(super) conversation: ConversationStateDoc,
}
impl StateDoc {
    pub(super) fn into_runtime(self) -> Result<GvyaState, WireError> {
        let state = GvyaState {
            author: map_values(self.author)?,
            conversation: self.conversation.into_runtime()?,
        };
        validate_state(&state)?;
        Ok(state)
    }
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub(super) struct ConversationStateDoc {
    active_topic: Option<ActiveTopicDoc>,
    active_followup: Option<ActiveFollowupDoc>,
    pub(super) active_collection: Option<ActiveCollectionDoc>,
    last_meaning: Option<String>,
    last_behavior: Option<String>,
    active_language: Option<String>,
    last_topic: Option<String>,
    #[serde(default)]
    pub(super) mentioned_topics: Vec<String>,
    #[serde(default)]
    pub(super) recent_response_ids: Vec<String>,
    #[serde(default)]
    pub(super) recent_variant_keys: Vec<String>,
    #[serde(default)]
    pub(super) recent_user_messages: Vec<String>,
    #[serde(default)]
    repeat_fallback_serial: u64,
    #[serde(default)]
    repeat_memory: RepeatMemoryDoc,
    #[serde(default)]
    repair: RepairMemoryDoc,
    #[serde(default)]
    pub(super) hint_progress: BTreeMap<String, u32>,
    #[serde(default)]
    pub(super) focus: Vec<HostReferenceDoc>,
    #[serde(default)]
    user_style: UserStyleDoc,
    #[serde(default)]
    pub(super) pending_capabilities: Vec<InvocationProposalDoc>,
    #[serde(default)]
    turn_index: u64,
}
impl ConversationStateDoc {
    pub(super) fn into_runtime(self) -> Result<gvya_model::ConversationState, WireError> {
        Ok(gvya_model::ConversationState {
            active_topic: self.active_topic.map(ActiveTopicDoc::into_runtime),
            active_followup: self.active_followup.map(ActiveFollowupDoc::into_runtime),
            active_collection: self
                .active_collection
                .map(ActiveCollectionDoc::into_runtime)
                .transpose()?,
            last_meaning: self.last_meaning.map(MeaningId::new),
            last_behavior: self.last_behavior.map(BehaviorId::new),
            active_language: self.active_language,
            last_topic: self.last_topic.map(TopicId::new),
            mentioned_topics: self
                .mentioned_topics
                .into_iter()
                .map(TopicId::new)
                .collect(),
            recent_response_ids: self
                .recent_response_ids
                .into_iter()
                .map(gvya_model::ResponseId::new)
                .collect(),
            recent_variant_keys: self.recent_variant_keys,
            recent_user_messages: self.recent_user_messages,
            repeat_fallback_serial: self.repeat_fallback_serial,
            repeat_memory: self.repeat_memory.into_runtime(),
            repair: self.repair.into_runtime(),
            hint_progress: self.hint_progress,
            focus: self
                .focus
                .into_iter()
                .map(HostReferenceDoc::into_runtime)
                .collect(),
            user_style: self.user_style.into_runtime()?,
            pending_capabilities: self
                .pending_capabilities
                .into_iter()
                .map(InvocationProposalDoc::into_runtime)
                .collect::<Result<_, _>>()?,
            turn_index: self.turn_index,
        })
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ActiveCollectionDoc {
    pub(super) meaning: MeaningStateDoc,
    pub(super) remaining: Vec<MissingRequiredValueDoc>,
    authority: CollectionAuthorityDoc,
    started_turn: u64,
}
impl ActiveCollectionDoc {
    fn into_runtime(self) -> Result<ActiveCollection, WireError> {
        Ok(ActiveCollection {
            meaning: self.meaning.into_runtime()?,
            remaining: self
                .remaining
                .into_iter()
                .map(MissingRequiredValueDoc::into_runtime)
                .collect(),
            authority: self.authority.into_runtime(),
            started_turn: self.started_turn,
        })
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct MeaningStateDoc {
    id: String,
    pub(super) slots: Vec<SlotStateDoc>,
    pub(super) references: Vec<HostReferenceDoc>,
}
impl MeaningStateDoc {
    fn into_runtime(self) -> Result<Meaning, WireError> {
        Ok(Meaning {
            id: MeaningId::new(self.id),
            slots: self
                .slots
                .into_iter()
                .map(SlotStateDoc::into_runtime)
                .collect::<Result<_, _>>()?,
            references: self
                .references
                .into_iter()
                .map(HostReferenceDoc::into_runtime)
                .collect(),
        })
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct SlotStateDoc {
    name: String,
    value: JsonValue,
    provenance: ValueProvenanceDoc,
}
impl SlotStateDoc {
    fn into_runtime(self) -> Result<SlotValue, WireError> {
        Ok(SlotValue {
            name: self.name,
            value: model_value(&self.value)?,
            provenance: self.provenance.into_runtime(),
        })
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum ValueProvenanceDoc {
    Utterance,
    Context,
    ConversationState,
    HostReferenceResolver,
    NeuralProposal,
    AuthorRule,
}
impl ValueProvenanceDoc {
    fn into_runtime(self) -> ValueProvenance {
        match self {
            Self::Utterance => ValueProvenance::Utterance,
            Self::Context => ValueProvenance::Context,
            Self::ConversationState => ValueProvenance::ConversationState,
            Self::HostReferenceResolver => ValueProvenance::HostReferenceResolver,
            Self::NeuralProposal => ValueProvenance::NeuralProposal,
            Self::AuthorRule => ValueProvenance::AuthorRule,
        }
    }
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum MissingRequiredValueDoc {
    Slot { name: String },
    Reference { kind: String },
}
impl MissingRequiredValueDoc {
    fn into_runtime(self) -> MissingRequiredValue {
        match self {
            Self::Slot { name } => MissingRequiredValue::Slot { name },
            Self::Reference { kind } => MissingRequiredValue::Reference {
                kind: ReferenceKind::new(kind),
            },
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum CollectionAuthorityDoc {
    StructuralPattern,
    Deterministic,
    ResolverProposal,
}
impl CollectionAuthorityDoc {
    fn into_runtime(self) -> CollectionAuthority {
        match self {
            Self::StructuralPattern => CollectionAuthority::StructuralPattern,
            Self::Deterministic => CollectionAuthority::Deterministic,
            Self::ResolverProposal => CollectionAuthority::ResolverProposal,
        }
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ActiveTopicDoc {
    id: String,
    ttl: u32,
    source_behavior: Option<String>,
}
impl ActiveTopicDoc {
    pub(super) fn into_runtime(self) -> ActiveTopic {
        ActiveTopic {
            id: TopicId::new(self.id),
            ttl: self.ttl,
            source_behavior: self.source_behavior.map(BehaviorId::new),
        }
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ActiveFollowupDoc {
    id: String,
    ttl: u32,
    source_behavior: Option<String>,
}
impl ActiveFollowupDoc {
    pub(super) fn into_runtime(self) -> ActiveFollowup {
        ActiveFollowup {
            id: FollowupId::new(self.id),
            ttl: self.ttl,
            source_behavior: self.source_behavior.map(BehaviorId::new),
        }
    }
}
#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub(super) struct RepeatMemoryDoc {
    #[serde(default)]
    last_user_normalized: String,
    last_meaning: Option<String>,
    #[serde(default)]
    same_input_count: u32,
    #[serde(default)]
    same_meaning_count: u32,
}
impl RepeatMemoryDoc {
    pub(super) fn into_runtime(self) -> RepeatMemory {
        RepeatMemory {
            last_user_normalized: self.last_user_normalized,
            last_meaning: self.last_meaning.map(MeaningId::new),
            same_input_count: self.same_input_count,
            same_meaning_count: self.same_meaning_count,
        }
    }
}
#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub(super) struct RepairMemoryDoc {
    #[serde(default)]
    consecutive: u32,
    last_mode: Option<String>,
    last_candidate: Option<String>,
}
impl RepairMemoryDoc {
    pub(super) fn into_runtime(self) -> RepairMemory {
        RepairMemory {
            consecutive: self.consecutive,
            last_mode: self.last_mode,
            last_candidate: self.last_candidate.map(MeaningId::new),
        }
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct UserStyleDoc {
    #[serde(default = "unknown_formality")]
    formality: String,
    #[serde(default)]
    confidence: f64,
}
pub(super) fn unknown_formality() -> String {
    "unknown".into()
}
impl Default for UserStyleDoc {
    fn default() -> Self {
        Self {
            formality: unknown_formality(),
            confidence: 0.0,
        }
    }
}
impl UserStyleDoc {
    pub(super) fn into_runtime(self) -> Result<UserStyle, WireError> {
        if !self.confidence.is_finite() {
            return Err(WireError::Invalid(
                "non-finite user_style confidence".into(),
            ));
        }
        let formality = match self.formality.as_str() {
            "unknown" => Formality::Unknown,
            "formal" => Formality::Formal,
            "informal" => Formality::Informal,
            _ => {
                return Err(WireError::Invalid(format!(
                    "unknown formality: {}",
                    self.formality
                )));
            }
        };
        Ok(UserStyle {
            formality,
            confidence: self.confidence,
        })
    }
}
