//! Integrated canonical runtime execution over one hydrated GVYA program.

use std::collections::BTreeMap;

use gvya_kernel::{
    CapabilityResultInput, ResolverReferenceCandidate, SemanticResolver, UtteranceInput,
    capability::{
        CAPABILITY_PENDING_PROPOSALS_MAX, CapabilityDefinition, CapabilityEvaluation,
        CapabilityEvaluationRequest, CapabilityKernel, CapabilityResultValidation,
    },
    conversation::{
        ConversationCapabilityResultRequest, ConversationKernel, ConversationMode,
        ConversationOpenRequest, ConversationOutcome, ConversationTurnRequest, HintRequest,
        LanguagePolicy, MAX_FOCUS_REFERENCES, MAX_HINT_PROGRESS_ENTRIES, MAX_MENTIONED_TOPICS,
        MAX_RECENT_RESPONSE_IDS, MAX_RECENT_USER_MESSAGES, MAX_RECENT_VARIANT_KEYS,
        author_state_within_limits,
    },
    semantic::ResolverRunError,
    why::build_why_report,
};
use gvya_model::{
    AdmissionOutcome, CapabilityId, ConfirmationGrant, ContextSnapshot, GvyaState,
    InvocationProposal, MissingRequiredValue, TraceCode, TraceEvent, TraceVisibility, Value,
    WhyReport,
};

use crate::loader::{
    LoadError, LoadPolicy, LoadedArtifact, RuntimeAsset, SignatureVerifier, load_artifact,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RuntimeLimits {
    pub max_request_bytes: usize,
    pub max_response_bytes: usize,
    pub max_string_bytes: usize,
    pub max_collection_entries: usize,
    pub max_value_depth: usize,
    pub max_value_nodes: usize,
    pub max_visible_references: usize,
    pub max_available_capabilities: usize,
    pub max_reference_candidates: usize,
    pub max_confirmations: usize,
}

impl Default for RuntimeLimits {
    fn default() -> Self {
        Self {
            max_request_bytes: 1024 * 1024,
            max_response_bytes: 2 * 1024 * 1024,
            max_string_bytes: 64 * 1024,
            max_collection_entries: 4096,
            max_value_depth: 32,
            max_value_nodes: 20_000,
            max_visible_references: 2048,
            max_available_capabilities: 2048,
            max_reference_candidates: 2048,
            max_confirmations: 1024,
        }
    }
}

impl RuntimeLimits {
    fn validate(self) -> Result<(), &'static str> {
        let ceiling = Self::default();
        let positive = self.max_request_bytes > 0
            && self.max_response_bytes > 0
            && self.max_string_bytes > 0
            && self.max_collection_entries > 0
            && self.max_value_depth > 0
            && self.max_value_nodes > 0
            && self.max_visible_references > 0
            && self.max_available_capabilities > 0
            && self.max_reference_candidates > 0
            && self.max_confirmations > 0;
        let bounded = self.max_request_bytes <= ceiling.max_request_bytes
            && self.max_response_bytes <= ceiling.max_response_bytes
            && self.max_string_bytes <= ceiling.max_string_bytes
            && self.max_collection_entries <= ceiling.max_collection_entries
            && self.max_value_depth <= ceiling.max_value_depth
            && self.max_value_nodes <= ceiling.max_value_nodes
            && self.max_visible_references <= ceiling.max_visible_references
            && self.max_available_capabilities <= ceiling.max_available_capabilities
            && self.max_reference_candidates <= ceiling.max_reference_candidates
            && self.max_confirmations <= ceiling.max_confirmations;
        if !positive {
            Err("runtime limits must be positive")
        } else if !bounded {
            Err("runtime limits may tighten but not relax canonical ceilings")
        } else {
            Ok(())
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RuntimeRequestError {
    Limit(&'static str),
    Invalid(&'static str),
}

#[derive(Debug)]
pub enum RuntimeResolverError<E> {
    Request(RuntimeRequestError),
    Resolver(ResolverRunError<E>),
}

#[derive(Clone, Debug, PartialEq)]
pub struct RuntimeUtteranceInput {
    pub text: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RuntimeTurnRequest {
    pub utterance: RuntimeUtteranceInput,
    pub context: ContextSnapshot,
    pub state: GvyaState,
    pub reference_candidates: Vec<ResolverReferenceCandidate>,
    pub resolver_context: BTreeMap<String, Value>,
    pub system: BTreeMap<String, Value>,
    pub hint: HintRequest,
    pub seed: Option<u64>,
    pub confirmations: Vec<ConfirmationGrant>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RuntimeOpenRequest {
    pub context: ContextSnapshot,
    pub state: GvyaState,
    pub system: BTreeMap<String, Value>,
    pub seed: Option<u64>,
    pub confirmations: Vec<ConfirmationGrant>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RuntimeCapabilityResultRequest {
    pub proposal: InvocationProposal,
    pub result: CapabilityResultInput,
    pub context: ContextSnapshot,
    pub state: GvyaState,
    pub system: BTreeMap<String, Value>,
    pub seed: Option<u64>,
    pub confirmations: Vec<ConfirmationGrant>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RuntimeCapabilityResultOutput {
    pub validation: CapabilityResultValidation,
    /// Present only after a structurally accepted host result. Invalid/stale results never mutate
    /// conversation state or produce follow-on capability proposals.
    pub interaction: Option<RuntimeInteractionOutput>,
    pub why: WhyReport,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RuntimeInteractionOutput {
    pub conversation: ConversationOutcome,
    pub capabilities: CapabilityEvaluation,
    pub why: WhyReport,
}

#[derive(Clone, Debug)]
pub struct Runtime {
    artifact: LoadedArtifact,
    conversation: ConversationKernel,
    capability: CapabilityKernel,
    limits: RuntimeLimits,
}

impl Runtime {
    pub fn load(
        bytes: Vec<u8>,
        policy: LoadPolicy,
        verifier: Option<&dyn SignatureVerifier>,
    ) -> Result<Self, LoadError> {
        Self::load_with_runtime_limits(bytes, policy, verifier, RuntimeLimits::default())
    }

    pub fn load_with_runtime_limits(
        bytes: Vec<u8>,
        policy: LoadPolicy,
        verifier: Option<&dyn SignatureVerifier>,
        limits: RuntimeLimits,
    ) -> Result<Self, LoadError> {
        limits
            .validate()
            .map_err(|message| LoadError::RuntimeLimits(message.into()))?;
        let artifact = load_artifact(bytes, policy, verifier)?;
        let semantic = artifact.program.semantic.clone();
        let conversation = ConversationKernel::from_semantic_kernel(
            semantic,
            artifact.program.conversation_catalog.clone(),
            artifact.program.conversation_config.clone(),
        )
        .map_err(|error| {
            LoadError::Program(crate::program::ProgramError::InvalidConversationConfig(
                error.0.into(),
            ))
        })?;
        let capability = CapabilityKernel::new(artifact.program.capability_catalog.clone());
        Ok(Self {
            artifact,
            conversation,
            capability,
            limits,
        })
    }

    #[must_use]
    pub fn limits(&self) -> RuntimeLimits {
        self.limits
    }

    #[must_use]
    pub fn project_id(&self) -> &str {
        &self.artifact.project_id
    }
    #[must_use]
    pub fn brain_id(&self) -> &str {
        &self.artifact.brain_id
    }
    #[must_use]
    pub fn enabled_languages(&self) -> &[String] {
        &self.artifact.program.enabled_languages
    }
    #[must_use]
    pub fn default_language(&self) -> &str {
        &self.artifact.program.default_language
    }
    #[must_use]
    pub fn artifact_digest(&self) -> &str {
        &self.artifact.artifact_digest
    }
    #[must_use]
    pub fn content_root(&self) -> [u8; 32] {
        self.artifact.content_root
    }
    #[must_use]
    pub fn trust(&self) -> &crate::loader::TrustStatus {
        &self.artifact.trust
    }

    /// Declared capability contracts embedded in this artifact. This is introspection only: a
    /// declaration never makes a capability available for an interaction and grants no host authority.
    pub fn capability_ids(&self) -> impl Iterator<Item = &CapabilityId> {
        self.capability.catalog().capability_ids()
    }

    /// Return one exact embedded capability contract without changing availability or policy.
    #[must_use]
    pub fn capability_definition(&self, id: &CapabilityId) -> Option<&CapabilityDefinition> {
        self.capability.catalog().definition(id)
    }

    #[must_use]
    pub fn asset(&self, id: &gvya_model::AssetId) -> Option<RuntimeAsset<'_>> {
        self.artifact.asset(id)
    }
    #[must_use]
    pub fn asset_by_logical_path(&self, path: &str) -> Option<RuntimeAsset<'_>> {
        self.artifact.asset_by_logical_path(path)
    }

    pub fn turn(
        &self,
        request: RuntimeTurnRequest,
    ) -> Result<RuntimeInteractionOutput, RuntimeRequestError> {
        validate_turn_request(&request, self.limits)?;
        let language = runtime_active_language(
            &request.state,
            self.default_language(),
            self.enabled_languages(),
        );
        let context = request.context.clone();
        let system = request.system.clone();
        let confirmations = request.confirmations.clone();
        let conversation = self.conversation.respond(ConversationTurnRequest {
            utterance: UtteranceInput {
                text: request.utterance.text,
                language: Some(language),
            },
            context: request.context,
            state: request.state,
            reference_candidates: request.reference_candidates,
            resolver_context: request.resolver_context,
            system: request.system,
            semantic_language_fallbacks: Vec::new(),
            language_policy: LanguagePolicy::default(),
            hint: request.hint,
            seed: request.seed,
        });
        self.finish_interaction(conversation, &context, &system, &confirmations)
    }

    pub fn turn_with_resolver(
        &self,
        request: RuntimeTurnRequest,
        resolver: &dyn SemanticResolver<Error = String>,
    ) -> Result<RuntimeInteractionOutput, RuntimeResolverError<String>> {
        validate_turn_request(&request, self.limits).map_err(RuntimeResolverError::Request)?;
        let language = runtime_active_language(
            &request.state,
            self.default_language(),
            self.enabled_languages(),
        );
        let context = request.context.clone();
        let system = request.system.clone();
        let confirmations = request.confirmations.clone();
        let conversation = self
            .conversation
            .respond_with_resolver(
                ConversationTurnRequest {
                    utterance: UtteranceInput {
                        text: request.utterance.text,
                        language: Some(language),
                    },
                    context: request.context,
                    state: request.state,
                    reference_candidates: request.reference_candidates,
                    resolver_context: request.resolver_context,
                    system: request.system,
                    semantic_language_fallbacks: Vec::new(),
                    language_policy: LanguagePolicy::default(),
                    hint: request.hint,
                    seed: request.seed,
                },
                resolver,
            )
            .map_err(RuntimeResolverError::Resolver)?;
        self.finish_interaction(conversation, &context, &system, &confirmations)
            .map_err(RuntimeResolverError::Request)
    }

    pub fn open(
        &self,
        request: RuntimeOpenRequest,
    ) -> Result<RuntimeInteractionOutput, RuntimeRequestError> {
        validate_open_request(&request, self.limits)?;
        let language = runtime_active_language(
            &request.state,
            self.default_language(),
            self.enabled_languages(),
        );
        let context = request.context.clone();
        let system = request.system.clone();
        let confirmations = request.confirmations.clone();
        let conversation = self.conversation.open(ConversationOpenRequest {
            language: Some(language),
            context: request.context,
            state: request.state,
            system: request.system,
            language_policy: LanguagePolicy::default(),
            seed: request.seed,
        });
        self.finish_interaction(conversation, &context, &system, &confirmations)
    }

    pub fn capability_result(
        &self,
        request: RuntimeCapabilityResultRequest,
    ) -> Result<RuntimeCapabilityResultOutput, RuntimeRequestError> {
        validate_capability_result_interaction_request(&request, self.limits)?;
        let language = runtime_active_language(
            &request.state,
            self.default_language(),
            self.enabled_languages(),
        );
        let mut validation = self
            .capability
            .validate_result(&request.proposal, &request.result);
        let pending_index = if validation.accepted {
            match request
                .state
                .conversation
                .pending_capabilities
                .iter()
                .position(|row| row.id == request.proposal.id)
            {
                Some(index)
                    if request.state.conversation.pending_capabilities[index]
                        == request.proposal =>
                {
                    Some(index)
                }
                Some(_) => {
                    reject_result_lifecycle(
                        &mut validation,
                        "proposal_receipt_mismatch",
                        "capability result proposal differs from the pending runtime receipt",
                    );
                    None
                }
                None => {
                    reject_result_lifecycle(
                        &mut validation,
                        "proposal_not_pending",
                        "capability result proposal is stale, replayed, or was never admitted by this runtime state",
                    );
                    None
                }
            }
        } else {
            None
        };
        if !validation.accepted {
            let why = build_why_report(&[&validation.trace]);
            let output = RuntimeCapabilityResultOutput {
                validation,
                interaction: None,
                why,
            };
            crate::wire::validate_capability_result_result_with_limits(&output, self.limits)
                .map_err(|_| RuntimeRequestError::Limit("response_bytes"))?;
            return Ok(output);
        }

        let context = request.context.clone();
        let system = request.system.clone();
        let confirmations = request.confirmations.clone();
        let mut state = request.state;
        state.conversation.pending_capabilities.remove(
            pending_index.expect("accepted capability result must have a pending proposal"),
        );
        let mut conversation =
            self.conversation
                .capability_result(ConversationCapabilityResultRequest {
                    proposal_id: request.proposal.id.clone(),
                    capability: request.proposal.capability.clone(),
                    capability_version: request.proposal.capability_version.clone(),
                    succeeded: request.result.succeeded,
                    output: request.result.output.clone(),
                    error_code: request.result.error_code.clone(),
                    language: Some(language),
                    context: request.context,
                    state,
                    system: request.system,
                    language_policy: LanguagePolicy::default(),
                    seed: request.seed,
                });
        let capabilities = self.capability.evaluate(CapabilityEvaluationRequest {
            conversation: &conversation,
            context: &context,
            system: &system,
            confirmations: &confirmations,
        });
        persist_admitted_proposals(&mut conversation.state, &capabilities)?;
        let mut traces = vec![&validation.trace, &conversation.trace, &capabilities.trace];
        if let Some(semantic) = &conversation.semantic {
            traces.insert(1, &semantic.trace);
        }
        let why = build_why_report(&traces);
        let interaction = RuntimeInteractionOutput {
            conversation,
            capabilities,
            why: why.clone(),
        };
        let output = RuntimeCapabilityResultOutput {
            validation,
            interaction: Some(interaction),
            why,
        };
        crate::wire::validate_capability_result_result_with_limits(&output, self.limits)
            .map_err(|_| RuntimeRequestError::Limit("response_bytes"))?;
        Ok(output)
    }

    fn finish_interaction(
        &self,
        mut conversation: ConversationOutcome,
        context: &ContextSnapshot,
        system: &BTreeMap<String, Value>,
        confirmations: &[ConfirmationGrant],
    ) -> Result<RuntimeInteractionOutput, RuntimeRequestError> {
        let capabilities = self.capability.evaluate(CapabilityEvaluationRequest {
            conversation: &conversation,
            context,
            system,
            confirmations,
        });
        persist_admitted_proposals(&mut conversation.state, &capabilities)?;
        let mut traces = Vec::new();
        if let Some(semantic) = &conversation.semantic {
            traces.push(&semantic.trace);
        }
        traces.push(&conversation.trace);
        traces.push(&capabilities.trace);
        let why = build_why_report(&traces);
        let output = RuntimeInteractionOutput {
            conversation,
            capabilities,
            why,
        };
        crate::wire::validate_turn_result_with_limits(&output, self.limits)
            .map_err(|_| RuntimeRequestError::Limit("response_bytes"))?;
        Ok(output)
    }
}

fn persist_admitted_proposals(
    state: &mut GvyaState,
    capabilities: &CapabilityEvaluation,
) -> Result<(), RuntimeRequestError> {
    for decision in &capabilities.decisions {
        if !matches!(&decision.outcome, AdmissionOutcome::Admitted) {
            continue;
        }
        let Some(proposal) = &decision.proposal else {
            continue;
        };
        if let Some(existing) = state
            .conversation
            .pending_capabilities
            .iter()
            .find(|row| row.id == proposal.id)
        {
            if existing != proposal {
                return Err(RuntimeRequestError::Invalid("proposal_id_collision"));
            }
            continue;
        }
        if state.conversation.pending_capabilities.len() >= CAPABILITY_PENDING_PROPOSALS_MAX {
            return Err(RuntimeRequestError::Limit("pending_capabilities"));
        }
        state
            .conversation
            .pending_capabilities
            .push(proposal.clone());
    }
    Ok(())
}

fn reject_result_lifecycle(validation: &mut CapabilityResultValidation, code: &str, summary: &str) {
    validation.accepted = false;
    validation.reason_code = Some(code.into());
    validation.trace.events.push(TraceEvent {
        code: TraceCode::new("capability.result_lifecycle_rejected"),
        phase: "capability.result".into(),
        summary: summary.into(),
        visibility: TraceVisibility::Author,
        details: BTreeMap::from([("reason".into(), Value::String(code.into()))]),
    });
}

fn runtime_active_language(
    state: &GvyaState,
    default_language: &str,
    enabled_languages: &[String],
) -> String {
    let enabled = |raw: &str| canonical_enabled_language(raw, enabled_languages);
    state
        .conversation
        .active_language
        .as_deref()
        .and_then(enabled)
        .unwrap_or_else(|| default_language.to_owned())
}

fn canonical_enabled_language(raw: &str, enabled_languages: &[String]) -> Option<String> {
    let normalized = gvya_kernel::conversation::normalize_locale(raw);
    if normalized.is_empty() {
        return None;
    }
    enabled_languages
        .iter()
        .find(|language| gvya_kernel::conversation::normalize_locale(language) == normalized)
        .or_else(|| {
            let base = normalized
                .split_once('-')
                .map_or(normalized.as_str(), |row| row.0);
            enabled_languages.iter().find(|language| {
                let enabled = gvya_kernel::conversation::normalize_locale(language);
                enabled
                    .split_once('-')
                    .map_or(enabled.as_str(), |row| row.0)
                    == base
            })
        })
        .cloned()
}

fn validate_turn_request(
    request: &RuntimeTurnRequest,
    limits: RuntimeLimits,
) -> Result<(), RuntimeRequestError> {
    if turn_request_text_bytes(request) > limits.max_request_bytes {
        return Err(RuntimeRequestError::Limit("request_bytes"));
    }
    let mut nodes = 0usize;
    validate_string(&request.utterance.text, limits)?;
    validate_context(&request.context, limits, &mut nodes)?;
    validate_state(&request.state, limits, &mut nodes)?;
    if request.reference_candidates.len() > limits.max_reference_candidates {
        return Err(RuntimeRequestError::Limit("reference_candidates"));
    }
    for row in &request.reference_candidates {
        validate_reference(&row.reference, limits)?;
        if let Some(label) = &row.label {
            validate_string(label, limits)?;
        }
        if row.aliases.len() > limits.max_collection_entries {
            return Err(RuntimeRequestError::Limit("reference_aliases"));
        }
        for alias in &row.aliases {
            validate_string(alias, limits)?;
        }
    }
    validate_value_map(&request.resolver_context, limits, &mut nodes)?;
    validate_value_map(&request.system, limits, &mut nodes)?;
    validate_confirmations(&request.confirmations, limits)?;
    Ok(())
}

fn validate_open_request(
    request: &RuntimeOpenRequest,
    limits: RuntimeLimits,
) -> Result<(), RuntimeRequestError> {
    if open_request_text_bytes(request) > limits.max_request_bytes {
        return Err(RuntimeRequestError::Limit("request_bytes"));
    }
    let mut nodes = 0usize;
    validate_context(&request.context, limits, &mut nodes)?;
    validate_state(&request.state, limits, &mut nodes)?;
    validate_value_map(&request.system, limits, &mut nodes)?;
    validate_confirmations(&request.confirmations, limits)?;
    Ok(())
}

fn validate_capability_result_interaction_request(
    request: &RuntimeCapabilityResultRequest,
    limits: RuntimeLimits,
) -> Result<(), RuntimeRequestError> {
    if capability_result_interaction_text_bytes(request) > limits.max_request_bytes {
        return Err(RuntimeRequestError::Limit("request_bytes"));
    }
    validate_capability_result_request(&request.proposal, &request.result, limits)?;
    let mut nodes = 0usize;
    validate_context(&request.context, limits, &mut nodes)?;
    validate_state(&request.state, limits, &mut nodes)?;
    validate_value_map(&request.system, limits, &mut nodes)?;
    validate_confirmations(&request.confirmations, limits)?;
    Ok(())
}

fn validate_capability_result_request(
    proposal: &InvocationProposal,
    result: &CapabilityResultInput,
    limits: RuntimeLimits,
) -> Result<(), RuntimeRequestError> {
    if capability_result_text_bytes(proposal, result) > limits.max_request_bytes {
        return Err(RuntimeRequestError::Limit("request_bytes"));
    }
    let mut nodes = 0usize;
    for value in [
        proposal.id.as_str(),
        proposal.capability.as_str(),
        proposal.capability_version.as_str(),
        proposal.fingerprint.as_str(),
        proposal.trace_id.as_str(),
        result.proposal_id.as_str(),
    ] {
        validate_string(value, limits)?;
    }
    validate_value_map(&proposal.arguments, limits, &mut nodes)?;
    if let Some(output) = &result.output {
        validate_value(output, 1, &mut nodes, limits)?;
    }
    if let Some(code) = &result.error_code {
        validate_string(code, limits)?;
    }
    Ok(())
}

fn validate_context(
    context: &ContextSnapshot,
    limits: RuntimeLimits,
    nodes: &mut usize,
) -> Result<(), RuntimeRequestError> {
    validate_value_map(&context.values, limits, nodes)?;
    if context.visible_references.len() > limits.max_visible_references {
        return Err(RuntimeRequestError::Limit("visible_references"));
    }
    for row in &context.visible_references {
        validate_reference(row, limits)?;
    }
    if context.available_capabilities.len() > limits.max_available_capabilities {
        return Err(RuntimeRequestError::Limit("available_capabilities"));
    }
    for row in &context.available_capabilities {
        validate_string(row.id.as_str(), limits)?;
        validate_string(row.version.as_str(), limits)?;
    }
    Ok(())
}

fn validate_state(
    state: &GvyaState,
    limits: RuntimeLimits,
    nodes: &mut usize,
) -> Result<(), RuntimeRequestError> {
    validate_value_map(&state.author, limits, nodes)?;
    if !author_state_within_limits(&state.author) {
        return Err(RuntimeRequestError::Limit("author_state"));
    }
    let c = &state.conversation;
    if c.mentioned_topics.len() > MAX_MENTIONED_TOPICS {
        return Err(RuntimeRequestError::Limit("mentioned_topics"));
    }
    if c.hint_progress.len() > MAX_HINT_PROGRESS_ENTRIES {
        return Err(RuntimeRequestError::Limit("hint_progress"));
    }
    if c.focus.len() > MAX_FOCUS_REFERENCES {
        return Err(RuntimeRequestError::Limit("focus"));
    }
    if c.recent_response_ids.len() > MAX_RECENT_RESPONSE_IDS {
        return Err(RuntimeRequestError::Limit("recent_response_ids"));
    }
    if c.recent_variant_keys.len() > MAX_RECENT_VARIANT_KEYS {
        return Err(RuntimeRequestError::Limit("recent_variant_keys"));
    }
    if c.recent_user_messages.len() > MAX_RECENT_USER_MESSAGES {
        return Err(RuntimeRequestError::Limit("recent_user_messages"));
    }
    if c.pending_capabilities.len() > CAPABILITY_PENDING_PROPOSALS_MAX {
        return Err(RuntimeRequestError::Limit("pending_capabilities"));
    }
    if let Some(collection) = &c.active_collection {
        if collection.remaining.is_empty()
            || collection.remaining.len() > gvya_kernel::semantic::MAX_ACTIVE_COLLECTION_VALUES
            || collection.meaning.slots.len() > gvya_kernel::semantic::MAX_ACTIVE_COLLECTION_VALUES
            || collection.meaning.references.len()
                > gvya_kernel::semantic::MAX_ACTIVE_COLLECTION_VALUES
        {
            return Err(RuntimeRequestError::Limit("active_collection"));
        }
        validate_string(collection.meaning.id.as_str(), limits)?;
        for slot in &collection.meaning.slots {
            validate_string(&slot.name, limits)?;
            validate_value(&slot.value, 1, nodes, limits)?;
        }
        for reference in &collection.meaning.references {
            validate_reference(reference, limits)?;
        }
        for target in &collection.remaining {
            match target {
                MissingRequiredValue::Slot { name } => validate_string(name, limits)?,
                MissingRequiredValue::Reference { kind } => validate_string(kind.as_str(), limits)?,
            }
        }
    }
    if let Some(topic) = &c.active_topic {
        validate_string(topic.id.as_str(), limits)?;
        if let Some(id) = &topic.source_behavior {
            validate_string(id.as_str(), limits)?;
        }
    }
    if let Some(followup) = &c.active_followup {
        validate_string(followup.id.as_str(), limits)?;
        if let Some(id) = &followup.source_behavior {
            validate_string(id.as_str(), limits)?;
        }
    }
    if let Some(id) = &c.last_meaning {
        validate_string(id.as_str(), limits)?;
    }
    if let Some(id) = &c.last_behavior {
        validate_string(id.as_str(), limits)?;
    }
    if let Some(language) = &c.active_language {
        validate_string(language, limits)?;
    }
    if let Some(id) = &c.last_topic {
        validate_string(id.as_str(), limits)?;
    }
    if let Some(id) = &c.repeat_memory.last_meaning {
        validate_string(id.as_str(), limits)?;
    }
    if let Some(id) = &c.repair.last_candidate {
        validate_string(id.as_str(), limits)?;
    }
    for id in &c.mentioned_topics {
        validate_string(id.as_str(), limits)?;
    }
    for id in &c.recent_response_ids {
        validate_string(id.as_str(), limits)?;
    }
    for value in &c.recent_variant_keys {
        validate_string(value, limits)?;
    }
    for value in &c.recent_user_messages {
        validate_string(value, limits)?;
    }
    for (key, _) in &c.hint_progress {
        validate_string(key, limits)?;
    }
    for row in &c.focus {
        validate_reference(row, limits)?;
    }
    for proposal in &c.pending_capabilities {
        for value in [
            proposal.id.as_str(),
            proposal.capability.as_str(),
            proposal.capability_version.as_str(),
            proposal.fingerprint.as_str(),
            proposal.trace_id.as_str(),
        ] {
            validate_string(value, limits)?;
        }
        validate_value_map(&proposal.arguments, limits, nodes)?;
    }
    validate_string(&c.repeat_memory.last_user_normalized, limits)?;
    if let Some(value) = &c.repair.last_mode {
        validate_string(value, limits)?;
    }
    if !c.user_style.confidence.is_finite() || !(0.0..=1.0).contains(&c.user_style.confidence) {
        return Err(RuntimeRequestError::Invalid("user_style.confidence"));
    }
    Ok(())
}

fn validate_value_map(
    map: &BTreeMap<String, Value>,
    limits: RuntimeLimits,
    nodes: &mut usize,
) -> Result<(), RuntimeRequestError> {
    if map.len() > limits.max_collection_entries {
        return Err(RuntimeRequestError::Limit("value_map_entries"));
    }
    for (key, value) in map {
        validate_string(key, limits)?;
        validate_value(value, 1, nodes, limits)?;
    }
    Ok(())
}

fn validate_value(
    value: &Value,
    depth: usize,
    nodes: &mut usize,
    limits: RuntimeLimits,
) -> Result<(), RuntimeRequestError> {
    if depth > limits.max_value_depth {
        return Err(RuntimeRequestError::Limit("value_depth"));
    }
    *nodes = nodes.saturating_add(1);
    if *nodes > limits.max_value_nodes {
        return Err(RuntimeRequestError::Limit("value_nodes"));
    }
    match value {
        Value::Null | Value::Bool(_) => Ok(()),
        Value::Number(number) if number.is_finite() => Ok(()),
        Value::Number(_) => Err(RuntimeRequestError::Invalid("non_finite_number")),
        Value::String(value) => validate_string(value, limits),
        Value::Array(values) => {
            if values.len() > limits.max_collection_entries {
                return Err(RuntimeRequestError::Limit("value_array_entries"));
            }
            for value in values {
                validate_value(value, depth + 1, nodes, limits)?;
            }
            Ok(())
        }
        Value::Object(values) => {
            if values.len() > limits.max_collection_entries {
                return Err(RuntimeRequestError::Limit("value_object_entries"));
            }
            for (key, value) in values {
                validate_string(key, limits)?;
                validate_value(value, depth + 1, nodes, limits)?;
            }
            Ok(())
        }
    }
}

fn validate_reference(
    reference: &gvya_model::HostReference,
    limits: RuntimeLimits,
) -> Result<(), RuntimeRequestError> {
    validate_string(reference.kind.as_str(), limits)?;
    validate_string(reference.id.as_str(), limits)
}

fn validate_confirmations(
    confirmations: &[ConfirmationGrant],
    limits: RuntimeLimits,
) -> Result<(), RuntimeRequestError> {
    if confirmations.len() > limits.max_confirmations {
        return Err(RuntimeRequestError::Limit("confirmations"));
    }
    for row in confirmations {
        validate_string(row.id.as_str(), limits)?;
        validate_string(row.proposal_id.as_str(), limits)?;
        validate_string(&row.fingerprint, limits)?;
    }
    Ok(())
}

fn validate_string(value: &str, limits: RuntimeLimits) -> Result<(), RuntimeRequestError> {
    if value.len() > limits.max_string_bytes {
        Err(RuntimeRequestError::Limit("string_bytes"))
    } else {
        Ok(())
    }
}

fn add_bytes(total: &mut usize, amount: usize) {
    *total = total.saturating_add(amount);
}

fn value_text_bytes(value: &Value, total: &mut usize) {
    match value {
        Value::Null | Value::Bool(_) | Value::Number(_) => {}
        Value::String(value) => add_bytes(total, value.len()),
        Value::Array(values) => {
            for value in values {
                value_text_bytes(value, total);
            }
        }
        Value::Object(values) => {
            for (key, value) in values {
                add_bytes(total, key.len());
                value_text_bytes(value, total);
            }
        }
    }
}

fn value_map_text_bytes(values: &BTreeMap<String, Value>, total: &mut usize) {
    for (key, value) in values {
        add_bytes(total, key.len());
        value_text_bytes(value, total);
    }
}

fn reference_text_bytes(reference: &gvya_model::HostReference, total: &mut usize) {
    add_bytes(total, reference.kind.as_str().len());
    add_bytes(total, reference.id.as_str().len());
}

fn context_text_bytes(context: &ContextSnapshot, total: &mut usize) {
    value_map_text_bytes(&context.values, total);
    for reference in &context.visible_references {
        reference_text_bytes(reference, total);
    }
    for capability in &context.available_capabilities {
        add_bytes(total, capability.id.as_str().len());
        add_bytes(total, capability.version.as_str().len());
    }
}

fn state_text_bytes(state: &GvyaState, total: &mut usize) {
    value_map_text_bytes(&state.author, total);
    let c = &state.conversation;
    if let Some(topic) = &c.active_topic {
        add_bytes(total, topic.id.as_str().len());
        if let Some(id) = &topic.source_behavior {
            add_bytes(total, id.as_str().len());
        }
    }
    if let Some(followup) = &c.active_followup {
        add_bytes(total, followup.id.as_str().len());
        if let Some(id) = &followup.source_behavior {
            add_bytes(total, id.as_str().len());
        }
    }
    if let Some(collection) = &c.active_collection {
        add_bytes(total, collection.meaning.id.as_str().len());
        for slot in &collection.meaning.slots {
            add_bytes(total, slot.name.len());
            value_text_bytes(&slot.value, total);
        }
        for reference in &collection.meaning.references {
            reference_text_bytes(reference, total);
        }
        for target in &collection.remaining {
            match target {
                MissingRequiredValue::Slot { name } => add_bytes(total, name.len()),
                MissingRequiredValue::Reference { kind } => add_bytes(total, kind.as_str().len()),
            }
        }
    }
    if let Some(id) = &c.last_meaning {
        add_bytes(total, id.as_str().len());
    }
    if let Some(id) = &c.last_behavior {
        add_bytes(total, id.as_str().len());
    }
    if let Some(language) = &c.active_language {
        add_bytes(total, language.len());
    }
    if let Some(id) = &c.last_topic {
        add_bytes(total, id.as_str().len());
    }
    if let Some(id) = &c.repeat_memory.last_meaning {
        add_bytes(total, id.as_str().len());
    }
    if let Some(id) = &c.repair.last_candidate {
        add_bytes(total, id.as_str().len());
    }
    for id in &c.mentioned_topics {
        add_bytes(total, id.as_str().len());
    }
    for id in &c.recent_response_ids {
        add_bytes(total, id.as_str().len());
    }
    for value in &c.recent_variant_keys {
        add_bytes(total, value.len());
    }
    for value in &c.recent_user_messages {
        add_bytes(total, value.len());
    }
    for key in c.hint_progress.keys() {
        add_bytes(total, key.len());
    }
    for reference in &c.focus {
        reference_text_bytes(reference, total);
    }
    for proposal in &c.pending_capabilities {
        add_bytes(total, proposal.id.as_str().len());
        add_bytes(total, proposal.capability.as_str().len());
        add_bytes(total, proposal.capability_version.as_str().len());
        value_map_text_bytes(&proposal.arguments, total);
        add_bytes(total, proposal.fingerprint.len());
        add_bytes(total, proposal.trace_id.as_str().len());
    }
    add_bytes(total, c.repeat_memory.last_user_normalized.len());
    if let Some(value) = &c.repair.last_mode {
        add_bytes(total, value.len());
    }
}

fn confirmations_text_bytes(confirmations: &[ConfirmationGrant], total: &mut usize) {
    for row in confirmations {
        add_bytes(total, row.id.as_str().len());
        add_bytes(total, row.proposal_id.as_str().len());
        add_bytes(total, row.fingerprint.len());
    }
}

fn turn_request_text_bytes(request: &RuntimeTurnRequest) -> usize {
    let mut total = request.utterance.text.len();
    context_text_bytes(&request.context, &mut total);
    state_text_bytes(&request.state, &mut total);
    for row in &request.reference_candidates {
        reference_text_bytes(&row.reference, &mut total);
        if let Some(label) = &row.label {
            add_bytes(&mut total, label.len());
        }
        for alias in &row.aliases {
            add_bytes(&mut total, alias.len());
        }
    }
    value_map_text_bytes(&request.resolver_context, &mut total);
    value_map_text_bytes(&request.system, &mut total);
    confirmations_text_bytes(&request.confirmations, &mut total);
    total
}

fn open_request_text_bytes(request: &RuntimeOpenRequest) -> usize {
    let mut total = 0usize;
    context_text_bytes(&request.context, &mut total);
    state_text_bytes(&request.state, &mut total);
    value_map_text_bytes(&request.system, &mut total);
    confirmations_text_bytes(&request.confirmations, &mut total);
    total
}

fn capability_result_interaction_text_bytes(request: &RuntimeCapabilityResultRequest) -> usize {
    let mut total = capability_result_text_bytes(&request.proposal, &request.result);
    context_text_bytes(&request.context, &mut total);
    state_text_bytes(&request.state, &mut total);
    value_map_text_bytes(&request.system, &mut total);
    confirmations_text_bytes(&request.confirmations, &mut total);
    total
}

fn capability_result_text_bytes(
    proposal: &InvocationProposal,
    result: &CapabilityResultInput,
) -> usize {
    let mut total = 0usize;
    for value in [
        proposal.id.as_str(),
        proposal.capability.as_str(),
        proposal.capability_version.as_str(),
        proposal.fingerprint.as_str(),
        proposal.trace_id.as_str(),
        result.proposal_id.as_str(),
    ] {
        add_bytes(&mut total, value.len());
    }
    value_map_text_bytes(&proposal.arguments, &mut total);
    if let Some(output) = &result.output {
        value_text_bytes(output, &mut total);
    }
    if let Some(code) = &result.error_code {
        add_bytes(&mut total, code.len());
    }
    total
}

#[must_use]
pub fn is_conversational_output(output: &RuntimeInteractionOutput) -> bool {
    !matches!(output.conversation.mode, ConversationMode::Silent)
}

#[cfg(test)]
mod language_contract_tests {
    use super::*;

    #[test]
    fn disabled_state_language_collapses_to_bot_default() {
        let mut state = GvyaState::default();
        state.conversation.active_language = Some("fa".to_string());
        assert_eq!(
            runtime_active_language(&state, "en", &["en".to_string()],),
            "en"
        );
    }

    #[test]
    fn enabled_state_language_remains_active() {
        let mut state = GvyaState::default();
        state.conversation.active_language = Some("fa".to_string());
        assert_eq!(
            runtime_active_language(&state, "en", &["en".to_string(), "fa".to_string()],),
            "fa"
        );
    }
}
