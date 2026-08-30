//! First-class conversation and response kernel.
//!
//! This layer owns conversation continuity, runtime-managed conversational state, deterministic
//! response eligibility/selection, templates and response planning. It does not own capability
//! admission or host execution.

mod catalog;
mod conditions;
mod engine;
mod selection;
mod state;
mod templates;

pub use catalog::{
    CapabilityResultBehavior, ConversationBehavior, ConversationCatalog, ConversationCatalogError,
    ConversationEffect, ExtraMessage, FallbackBehavior, FallbackTrigger, FollowupDirective,
    LocalizedTexts, OpeningDefinition, PredicateOp, RepeatStage, ResponseAsset, ResponseDefinition,
    ResponseKind, ResponseLink, StateNamespace, StateTarget, StyleLexicon, ValueCondition,
    ValuePath, ValueRequirement, safe_http_url,
};
pub use conditions::{
    AUTHOR_STATE_MAX_DEPTH, AUTHOR_STATE_MAX_NODES, AUTHOR_STATE_MAX_STRING_BYTES,
    AUTHOR_STATE_MAX_TOP_LEVEL, ConditionContext, apply_effects, author_state_within_limits,
    condition_matches, conditions_match, initialize_author_numbers, path_get, path_set,
    value_requirement_matches,
};
pub use engine::{
    ConversationCapabilityResultRequest, ConversationKernel, ConversationKernelBuildError,
    ConversationMode, ConversationOpenRequest, ConversationOutcome, ConversationTurnRequest,
};
pub use selection::{
    HintRequest, LanguagePolicy, SelectedResponse, language_tag_is_well_formed, normalize_locale,
    resolve_hint_pick_level, resolve_language,
};
pub use state::{
    AuthorNumberDefinition, ConversationConfig, ConversationConfigError, FollowupTurnSnapshot,
    MAX_AUTHOR_NUMBER_DEFINITIONS, MAX_FOCUS_REFERENCES, MAX_HINT_PROGRESS_ENTRIES,
    MAX_MENTIONED_TOPICS, MAX_RECENT_RESPONSE_IDS, MAX_RECENT_USER_MESSAGES,
    MAX_RECENT_VARIANT_KEYS, active_followup, active_topic, active_topic_id, commit_repeat_memory,
    consume_followup, finalize_followup_after_matching, global_repeat_count, hint_progress_key,
    project_repeat_counts, push_recent_response, push_recent_user_message, push_recent_variant,
    refresh_or_activate_topic, repair_stage, repeat_preference, repeat_preference_for_thresholds,
    set_active_followup, set_active_topic, set_hint_progress, tick_topic_at_turn_start,
    update_focus, update_repair_state,
};
pub use templates::{
    DeterministicRng, RenderedTemplate, TEMPLATE_MAX_EFFECTS, TEMPLATE_MAX_EXPRESSION_DEPTH,
    TEMPLATE_MAX_FUNCTION_ARGS, TEMPLATE_MAX_OUTPUT_BYTES, TemplateEffect, TemplateEnvironment,
    TemplateRenderer, basic_math_result, format_scalar, stable_seed, truthy,
};
