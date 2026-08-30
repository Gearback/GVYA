//! Typed, deterministic capability authority.
//!
//! This layer binds admitted conversation outcomes to declared host capabilities, validates typed
//! arguments, applies availability/policy/confirmation gates, and emits invocation proposals.
//! It never executes host code or mutates host state.

mod binding;
mod catalog;
mod engine;
mod policy;
mod schema;

pub use binding::{
    ArgumentBinding, ArgumentPath, BindingIssue, BindingOutput, BindingSource,
    CapabilityBindingRule, CapabilityTrigger, ReferenceProjection, bind_arguments, trigger_matches,
};
pub use catalog::{
    CAPABILITY_BINDINGS_MAX, CAPABILITY_PENDING_PROPOSALS_MAX, CAPABILITY_POLICY_RULES_MAX,
    CAPABILITY_PROPOSALS_PER_TURN_MAX, CAPABILITY_SCHEMA_ARRAY_ITEMS_MAX,
    CAPABILITY_SCHEMA_DEPTH_MAX, CAPABILITY_SCHEMA_ERRORS_MAX,
    CAPABILITY_SCHEMA_OBJECT_PROPERTIES_MAX, CAPABILITY_SCHEMA_STRING_BYTES_MAX, CapabilityCatalog,
    CapabilityConfig, CapabilityDefinition, CatalogIssue, HostEffectDeclaration, HostEffectKind,
    IssueSeverity, validate_capability_config,
};
pub use engine::{
    CapabilityDecision, CapabilityEvaluation, CapabilityEvaluationRequest, CapabilityKernel,
    CapabilityResultValidation, InvocationSource,
};
pub use policy::{
    AdmissionNamespace, AdmissionPredicate, CapabilityPolicyRule, PolicyContext, PolicyDecision,
    PolicyEffect, PredicateOp, conversation_scalar, evaluate_policy,
    predicate_matches_with_conversation, predicates_match_with_conversation,
};
pub use schema::{
    ObjectSchema, SchemaIssue, SchemaLimits, ValueSchema, validate_schema_definition,
    validate_value,
};
