//! Stable JSON wire contract shared by C/WASM/JavaScript adapters.

use std::{
    collections::BTreeMap,
    io::{self, Write},
};

use gvya_kernel::{
    CapabilityResultInput, ResolverReferenceCandidate,
    capability::{
        CAPABILITY_PENDING_PROPOSALS_MAX, CapabilityDecision, CapabilityDefinition,
        CapabilityEvaluation, HostEffectKind,
    },
    conversation::{
        HintRequest, MAX_FOCUS_REFERENCES, MAX_HINT_PROGRESS_ENTRIES, MAX_MENTIONED_TOPICS,
        MAX_RECENT_RESPONSE_IDS, MAX_RECENT_USER_MESSAGES, MAX_RECENT_VARIANT_KEYS,
        author_state_within_limits,
    },
    semantic::MAX_ACTIVE_COLLECTION_VALUES,
};
use gvya_model::{
    ActiveCollection, ActiveFollowup, ActiveTopic, AdmissionOutcome, AvailableCapability,
    BehaviorId, CapabilityId, CapabilityVersion, CollectionAuthority, ConfirmationGrant,
    ConfirmationHint, ConfirmationId, ContextSnapshot, EffectClass, FollowupId, Formality,
    GvyaState, HostReference, InvocationProposal, Meaning, MeaningId, MissingRequiredValue,
    ProposalId, ReferenceId, ReferenceKind, RepairMemory, RepeatMemory, ResponseItem, ResponsePlan,
    SlotValue, TopicId, Trace, TraceId, TraceVisibility, UserStyle, Value, ValueProvenance,
    WhyEntryStatus, WhyReport, WhySectionKind,
};
use serde::{Deserialize, de::DeserializeOwned};
use serde_json::{Map, Value as JsonValue, json};

use crate::{
    engine::{
        Runtime, RuntimeCapabilityResultOutput, RuntimeCapabilityResultRequest,
        RuntimeInteractionOutput, RuntimeLimits, RuntimeOpenRequest, RuntimeTurnRequest,
        RuntimeUtteranceInput,
    },
    loader::{RuntimeAsset, TrustStatus},
};

mod doc;
mod json;
mod limits;
mod request;
mod response;
#[cfg(test)]
mod tests;

use doc::*;
use json::*;
use limits::*;
pub use request::{
    parse_capability_result_request, parse_capability_result_request_with_limits,
    parse_open_request, parse_open_request_with_limits, parse_turn_request,
    parse_turn_request_with_limits,
};
pub use response::{
    serialize_asset_info, serialize_capabilities, serialize_capability_info,
    serialize_capability_result_result, serialize_capability_result_result_with_limits,
    serialize_runtime_info, serialize_turn_result, serialize_turn_result_with_limits,
};
pub(crate) use response::{
    validate_capability_result_result_with_limits, validate_turn_result_with_limits,
};

pub const TURN_REQUEST_FORMAT: &str = "gvya.runtime.turn";
pub const TURN_RESPONSE_FORMAT: &str = "gvya.runtime.turn-result";
pub const OPEN_REQUEST_FORMAT: &str = "gvya.runtime.open";
pub const CAPABILITY_RESULT_REQUEST_FORMAT: &str = "gvya.runtime.capability-result";
pub const CAPABILITY_RESULT_RESPONSE_FORMAT: &str = "gvya.runtime.capability-result-result";
pub const RUNTIME_INFO_FORMAT: &str = "gvya.runtime.info";
pub const CAPABILITIES_INFO_FORMAT: &str = "gvya.runtime.capabilities";
pub const CAPABILITY_INFO_FORMAT: &str = "gvya.runtime.capability-info";
pub const ASSET_INFO_FORMAT: &str = "gvya.runtime.asset-info";
pub const WIRE_VERSION: u32 = 1;

pub type WireLimits = RuntimeLimits;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum WireError {
    Json(String),
    Format,
    Version(u32),
    Invalid(String),
}
