//! Machine-readable canonical GVYA source authoring contract.
//!
//! This module does not replace semantic/source validation. It exposes the parser-owned authoring
//! surface so external agents can discover object shapes without scraping prose documentation.

use serde_json::{Value as JsonValue, json};

#[derive(Clone, Copy)]
pub struct SourceFieldContract {
    pub name: &'static str,
    pub required: bool,
    pub value_type: &'static str,
    pub item_kind: Option<&'static str>,
    pub enum_values: &'static [&'static str],
    pub default: Option<&'static str>,
    pub description: &'static str,
}

impl SourceFieldContract {
    pub const fn required(
        name: &'static str,
        value_type: &'static str,
        item_kind: Option<&'static str>,
        enum_values: &'static [&'static str],
        description: &'static str,
    ) -> Self {
        Self {
            name,
            required: true,
            value_type,
            item_kind,
            enum_values,
            default: None,
            description,
        }
    }

    pub const fn optional(
        name: &'static str,
        value_type: &'static str,
        item_kind: Option<&'static str>,
        enum_values: &'static [&'static str],
        default: Option<&'static str>,
        description: &'static str,
    ) -> Self {
        Self {
            name,
            required: false,
            value_type,
            item_kind,
            enum_values,
            default,
            description,
        }
    }
}

#[derive(Clone, Copy)]
pub struct SourceObjectContract {
    pub kind: &'static str,
    pub description: &'static str,
    pub identity_field: Option<&'static str>,
    pub fields: &'static [SourceFieldContract],
}

const EMPTY_ENUM: &[&str] = &[];

pub const PROJECT_KEYS: &[&str] = &[
    "format",
    "version",
    "project_id",
    "brain_id",
    "languages",
    "enabled_languages",
    "default_language",
    "language_profiles",
    "matcher_profiles",
    "packages",
    "fallback_package",
    "semantic",
    "conversation",
    "emit_debug_map",
];
pub const PACKAGE_KEYS: &[&str] = &["format", "version", "manifest", "fragments"];
pub const PACKAGE_MANIFEST_KEYS: &[&str] = &["id", "kind", "description", "dependencies"];
pub const PACKAGE_CONTENTS_KEYS: &[&str] = &[
    "meanings",
    "behaviors",
    "capability_result_behaviors",
    "openings",
    "fallback_behaviors",
    "style_lexicons",
    "capabilities",
    "capability_bindings",
    "capability_policies",
    "capability_configs",
    "types",
    "assets",
    "regression_cases",
    "scenarios",
];
pub const PACKAGE_FRAGMENTS_KEYS: &[&str] = PACKAGE_CONTENTS_KEYS;
pub const CONTRIBUTION_KEYS: &[&str] = &["id", "exported", "mode", "value"];
pub const MEANING_KEYS: &[&str] = &[
    "id",
    "class",
    "patterns",
    "samples",
    "negative_samples",
    "retrieval_terms",
    "priority",
    "positive_assumption",
    "slots",
    "references",
];
pub const BEHAVIOR_KEYS: &[&str] = &[
    "id",
    "meaning",
    "topic",
    "topic_scoped",
    "activates_topic",
    "topic_ttl",
    "followup_scope",
    "repair_continuation_candidate",
    "repeat_same_input_after",
    "repeat_same_meaning_after",
    "requires_values",
    "forbidden_values",
    "responses",
];
pub const FALLBACK_BEHAVIOR_KEYS: &[&str] =
    &["id", "trigger", "priority", "conditions", "responses"];
pub const CAPABILITY_RESULT_BEHAVIOR_KEYS: &[&str] = &[
    "id",
    "capability",
    "capability_version",
    "succeeded",
    "error_code",
    "responses",
];
pub const OPENING_KEYS: &[&str] = &["id", "topic", "topic_ttl", "responses"];
pub const STYLE_LEXICON_KEYS: &[&str] = &["formal_terms", "informal_terms"];
pub const RESPONSE_KEYS: &[&str] = &[
    "id",
    "kind",
    "texts",
    "conditions",
    "hint_level",
    "repeat_stage",
    "effects",
    "opens_followup",
    "extra_messages",
    "assets",
    "links",
];
pub const CAPABILITY_KEYS: &[&str] = &["contract", "host_effects"];
pub const CAPABILITY_CONTRACT_KEYS: &[&str] = &[
    "id",
    "version",
    "title",
    "description",
    "input_schema",
    "output_schema",
    "reference_kinds",
    "effect_class",
    "confirmation_hint",
];
pub const CAPABILITY_BINDING_KEYS: &[&str] = &["id", "trigger", "capability", "arguments"];
pub const CAPABILITY_POLICY_KEYS: &[&str] =
    &["id", "capability", "priority", "conditions", "effect"];
pub const CAPABILITY_CONFIG_KEYS: &[&str] = &[
    "schema_limits",
    "max_proposals_per_turn",
    "max_bindings",
    "max_policy_rules",
];
pub const NAMED_TYPE_KEYS: &[&str] = &["id", "schema"];
pub const ASSET_KEYS: &[&str] = &["id", "media_type", "logical_path", "source"];
pub const REGRESSION_CASE_KEYS: &[&str] = &[
    "id",
    "description",
    "input",
    "language",
    "context",
    "initial_state",
    "seed",
    "unix_time_ms",
    "expectation",
    "generated",
];
pub const SCENARIO_KEYS: &[&str] = &[
    "id",
    "description",
    "context",
    "initial_state",
    "steps",
    "generated",
];
pub const MATCHER_PROFILE_KEYS: &[&str] = &["format", "version", "language", "profile"];
pub const LANGUAGE_PROFILE_KEYS: &[&str] = &["format", "version", "language", "profile"];

// Nested source-object key registries are shared by the parser and machine schema surface.
pub const PACKAGE_DEPENDENCY_KEYS: &[&str] = &["id", "reexport"];
pub const SEMANTIC_CONFIG_KEYS: &[&str] = &[
    "candidate_limit",
    "resolution_threshold",
    "ambiguity_margin",
    "resolver_min_confidence",
    "resolver_candidate_limit",
];
pub const CONVERSATION_CONFIG_KEYS: &[&str] = &[
    "default_topic_ttl",
    "default_followup_ttl",
    "recent_response_limit",
    "recent_variant_limit",
    "recent_user_window",
    "repeat_detection_window",
    "repeat_detection_threshold",
    "max_messages_per_turn",
    "repair_candidate_min_score",
    "author_numbers",
    "topic_preference_margin",
];
pub const LOCALIZED_SAMPLE_KEYS: &[&str] = &["language", "text", "priority"];
pub const LOCALIZED_TEXTS_KEYS: &[&str] = &["language", "variants"];
pub const SLOT_SPEC_KEYS: &[&str] = &[
    "name",
    "type",
    "entity_kind",
    "reference_kind",
    "required",
    "elicitation",
];
pub const REFERENCE_SPEC_KEYS: &[&str] = &["kind", "required", "elicitation"];
pub const VALUE_REQUIREMENT_KEYS: &[&str] = &["namespace", "path", "value"];
pub const VALUE_CONDITION_KEYS: &[&str] = &["namespace", "path", "op", "value"];
pub const NAMESPACE_VALUE_KEYS: &[&str] = &["namespace", "path"];
pub const FOLLOWUP_DIRECTIVE_KEYS: &[&str] = &["id", "ttl", "refresh_if_same"];
pub const EXTRA_MESSAGE_KEYS: &[&str] = &["chance", "texts"];
pub const RESPONSE_ASSET_KEYS: &[&str] = &["asset_id", "alt_text"];
pub const RESPONSE_LINK_KEYS: &[&str] = &["label", "url"];
pub const HOST_EFFECT_KEYS: &[&str] = &["resource", "kind", "summary"];
pub const CAPABILITY_TRIGGER_KEYS: &[&str] = &["meaning", "behavior", "response"];
pub const ARGUMENT_BINDING_KEYS: &[&str] = &["target", "source"];
pub const BINDING_SOURCE_MEANING_SLOT_KEYS: &[&str] = &["type", "name"];
pub const BINDING_SOURCE_REFERENCE_KEYS: &[&str] = &["type", "kind", "projection"];
pub const BINDING_SOURCE_PATH_KEYS: &[&str] = &["type", "path"];
pub const BINDING_SOURCE_LITERAL_KEYS: &[&str] = &["type", "value"];
pub const ADMISSION_PREDICATE_KEYS: &[&str] = &["namespace", "path", "op", "value"];
pub const POLICY_EFFECT_ALLOW_KEYS: &[&str] = &["type"];
pub const POLICY_EFFECT_REASON_KEYS: &[&str] = &["type", "reason_code"];
pub const CONVERSATION_EFFECT_ASSIGN_KEYS: &[&str] = &["type", "target", "value"];
pub const CONVERSATION_EFFECT_INCREMENT_KEYS: &[&str] = &["type", "target", "delta"];
pub const SCHEMA_LIMIT_KEYS: &[&str] = &[
    "max_depth",
    "max_array_items",
    "max_object_properties",
    "max_string_bytes",
    "max_errors",
];
pub const REFERENCE_CANDIDATE_KEYS: &[&str] = &["reference", "label", "aliases"];
pub const REFERENCE_ID_KEYS: &[&str] = &["kind", "id"];
pub const TURN_EXPECTATION_KEYS: &[&str] = &[
    "meaning",
    "forbidden_meanings",
    "meaning_slots",
    "meaning_references",
    "min_semantic_score",
    "conversation_mode",
    "response_ids",
    "forbidden_response_ids",
    "response_contains",
    "response_not_contains",
    "author_values",
    "conversation_values",
    "active_topic",
    "active_followup",
    "capabilities",
    "proposal_receipts",
    "forbidden_capabilities",
    "capability_result_accepted",
    "capability_result_reason_code",
    "why_codes",
    "forbidden_why_codes",
];

const PROJECT_FIELDS: &[SourceFieldContract] = &[
    SourceFieldContract::required(
        "format",
        "string",
        None,
        &["gvya.source.project"],
        "Exact source document format.",
    ),
    SourceFieldContract::required("version", "integer", None, &["1"], "Exact source version."),
    SourceFieldContract::required(
        "project_id",
        "string",
        None,
        EMPTY_ENUM,
        "Stable project identity.",
    ),
    SourceFieldContract::required(
        "brain_id",
        "string",
        None,
        EMPTY_ENUM,
        "Stable compiled Brain identity.",
    ),
    SourceFieldContract::required(
        "languages",
        "array",
        Some("language-tag"),
        EMPTY_ENUM,
        "Ordered declared language catalog.",
    ),
    SourceFieldContract::required(
        "enabled_languages",
        "array",
        Some("language-tag"),
        EMPTY_ENUM,
        "Runtime-enabled subset of declared languages.",
    ),
    SourceFieldContract::required(
        "default_language",
        "string",
        Some("language-tag"),
        EMPTY_ENUM,
        "Default runtime language; must be enabled.",
    ),
    SourceFieldContract::optional(
        "language_profiles",
        "array",
        Some("source-path"),
        EMPTY_ENUM,
        Some("[]"),
        "Declared Language Profile documents.",
    ),
    SourceFieldContract::optional(
        "matcher_profiles",
        "array",
        Some("source-path"),
        EMPTY_ENUM,
        Some("[]"),
        "Declared Matcher Profile documents.",
    ),
    SourceFieldContract::optional(
        "packages",
        "array",
        Some("source-path"),
        EMPTY_ENUM,
        Some("[]"),
        "Declared Standard Package documents.",
    ),
    SourceFieldContract::optional(
        "fallback_package",
        "string|null",
        Some("source-path"),
        EMPTY_ENUM,
        Some("null"),
        "Optional selected Fallback Package document.",
    ),
    SourceFieldContract::optional(
        "semantic",
        "object",
        Some("semantic-config"),
        EMPTY_ENUM,
        Some("canonical defaults"),
        "Semantic matcher configuration.",
    ),
    SourceFieldContract::optional(
        "conversation",
        "object",
        Some("conversation-config"),
        EMPTY_ENUM,
        Some("canonical defaults"),
        "Conversation-kernel configuration.",
    ),
    SourceFieldContract::optional(
        "emit_debug_map",
        "boolean",
        None,
        EMPTY_ENUM,
        Some("false"),
        "Whether builds emit debug-map data.",
    ),
];

const PACKAGE_FIELDS: &[SourceFieldContract] = &[
    SourceFieldContract::required(
        "format",
        "string",
        None,
        &["gvya.source.package"],
        "Exact source document format.",
    ),
    SourceFieldContract::required(
        "version",
        "integer",
        None,
        &["1"],
        "Exact Package source version.",
    ),
    SourceFieldContract::required(
        "manifest",
        "object",
        Some("package-manifest"),
        EMPTY_ENUM,
        "Package identity/composition metadata.",
    ),
    SourceFieldContract::required(
        "fragments",
        "object",
        Some("package-fragments"),
        EMPTY_ENUM,
        "Explicit namespace-to-fragment-file index. Contribution content is never embedded in the Package root.",
    ),
];

const PACKAGE_MANIFEST_FIELDS: &[SourceFieldContract] = &[
    SourceFieldContract::required("id", "string", None, EMPTY_ENUM, "Stable Package identity."),
    SourceFieldContract::required(
        "kind",
        "string",
        None,
        &["standard", "fallback"],
        "Package composition kind.",
    ),
    SourceFieldContract::optional(
        "description",
        "string",
        None,
        EMPTY_ENUM,
        Some("\"\""),
        "Human/agent-facing description.",
    ),
    SourceFieldContract::optional(
        "dependencies",
        "array",
        Some("package-dependency"),
        EMPTY_ENUM,
        Some("[]"),
        "Explicit Package dependencies.",
    ),
];

const PACKAGE_CONTENTS_FIELDS: &[SourceFieldContract] = &[
    SourceFieldContract::optional(
        "meanings",
        "array",
        Some("source-path"),
        EMPTY_ENUM,
        Some("[]"),
        "Meaning contribution fragment files.",
    ),
    SourceFieldContract::optional(
        "behaviors",
        "array",
        Some("source-path"),
        EMPTY_ENUM,
        Some("[]"),
        "Conversation Behavior contribution fragment files.",
    ),
    SourceFieldContract::optional(
        "capability_result_behaviors",
        "array",
        Some("source-path"),
        EMPTY_ENUM,
        Some("[]"),
        "Capability-result Behavior fragment files.",
    ),
    SourceFieldContract::optional(
        "openings",
        "array",
        Some("source-path"),
        EMPTY_ENUM,
        Some("[]"),
        "Opening contribution fragment files.",
    ),
    SourceFieldContract::optional(
        "fallback_behaviors",
        "array",
        Some("source-path"),
        EMPTY_ENUM,
        Some("[]"),
        "Fallback Behavior fragment files; only valid in the selected Fallback Package.",
    ),
    SourceFieldContract::optional(
        "style_lexicons",
        "array",
        Some("source-path"),
        EMPTY_ENUM,
        Some("[]"),
        "Style lexicon fragment files.",
    ),
    SourceFieldContract::optional(
        "capabilities",
        "array",
        Some("source-path"),
        EMPTY_ENUM,
        Some("[]"),
        "Capability fragment files.",
    ),
    SourceFieldContract::optional(
        "capability_bindings",
        "array",
        Some("source-path"),
        EMPTY_ENUM,
        Some("[]"),
        "Capability binding fragment files.",
    ),
    SourceFieldContract::optional(
        "capability_policies",
        "array",
        Some("source-path"),
        EMPTY_ENUM,
        Some("[]"),
        "Capability policy fragment files.",
    ),
    SourceFieldContract::optional(
        "capability_configs",
        "array",
        Some("source-path"),
        EMPTY_ENUM,
        Some("[]"),
        "Capability configuration fragment files.",
    ),
    SourceFieldContract::optional(
        "types",
        "array",
        Some("source-path"),
        EMPTY_ENUM,
        Some("[]"),
        "Named JSON Schema type fragment files.",
    ),
    SourceFieldContract::optional(
        "assets",
        "array",
        Some("source-path"),
        EMPTY_ENUM,
        Some("[]"),
        "Asset contribution fragment files.",
    ),
    SourceFieldContract::optional(
        "regression_cases",
        "array",
        Some("source-path"),
        EMPTY_ENUM,
        Some("[]"),
        "Single-turn Regression Case fragment files.",
    ),
    SourceFieldContract::optional(
        "scenarios",
        "array",
        Some("source-path"),
        EMPTY_ENUM,
        Some("[]"),
        "Conversation Scenario fragment files.",
    ),
];

const CONTRIBUTION_FIELDS: &[SourceFieldContract] = &[
    SourceFieldContract::required(
        "id",
        "string",
        None,
        EMPTY_ENUM,
        "Contribution identity; must agree with the value identity when the value has one.",
    ),
    SourceFieldContract::optional(
        "exported",
        "boolean",
        None,
        EMPTY_ENUM,
        Some("true"),
        "Whether dependents may consume this contribution.",
    ),
    SourceFieldContract::optional(
        "mode",
        "string|object",
        Some("contribution-mode"),
        &["add", "replace"],
        Some("add"),
        "Add or explicit whole-item replace.",
    ),
    SourceFieldContract::required(
        "value",
        "object",
        Some("namespace-value"),
        EMPTY_ENUM,
        "The typed object for the owning contribution namespace.",
    ),
];

const MEANING_FIELDS: &[SourceFieldContract] = &[
    SourceFieldContract::required("id", "string", None, EMPTY_ENUM, "Meaning identity."),
    SourceFieldContract::optional(
        "class",
        "string",
        None,
        &["general", "social", "clarification"],
        Some("general"),
        "Meaning class.",
    ),
    SourceFieldContract::optional(
        "patterns",
        "array",
        Some("structural-pattern"),
        EMPTY_ENUM,
        Some("[]"),
        "Deterministic whole-utterance structural patterns.",
    ),
    SourceFieldContract::optional(
        "samples",
        "array",
        Some("localized-sample"),
        EMPTY_ENUM,
        Some("[]"),
        "Positive semantic evidence.",
    ),
    SourceFieldContract::optional(
        "negative_samples",
        "array",
        Some("localized-sample"),
        EMPTY_ENUM,
        Some("[]"),
        "Negative/confounder semantic evidence.",
    ),
    SourceFieldContract::optional(
        "retrieval_terms",
        "array",
        Some("localized-sample"),
        EMPTY_ENUM,
        Some("[]"),
        "Explicit semantic retrieval terms.",
    ),
    SourceFieldContract::optional(
        "priority",
        "integer",
        None,
        EMPTY_ENUM,
        Some("1"),
        "Meaning priority.",
    ),
    SourceFieldContract::optional(
        "positive_assumption",
        "boolean",
        None,
        EMPTY_ENUM,
        Some("false"),
        "Whether the Meaning opts into positive-assumption semantics.",
    ),
    SourceFieldContract::optional(
        "slots",
        "array",
        Some("slot-spec"),
        EMPTY_ENUM,
        Some("[]"),
        "Declared typed slots.",
    ),
    SourceFieldContract::optional(
        "references",
        "array",
        Some("reference-spec"),
        EMPTY_ENUM,
        Some("[]"),
        "Declared host-reference requirements.",
    ),
];

const BEHAVIOR_FIELDS: &[SourceFieldContract] = &[
    SourceFieldContract::required("id", "string", None, EMPTY_ENUM, "Behavior identity."),
    SourceFieldContract::required(
        "meaning",
        "string",
        Some("meaning-id"),
        EMPTY_ENUM,
        "Meaning resolved by this Behavior.",
    ),
    SourceFieldContract::optional(
        "topic",
        "string|null",
        Some("topic-id"),
        EMPTY_ENUM,
        Some("null"),
        "Optional topic identity.",
    ),
    SourceFieldContract::optional(
        "topic_scoped",
        "boolean",
        None,
        EMPTY_ENUM,
        Some("false"),
        "Require the matching active topic.",
    ),
    SourceFieldContract::optional(
        "activates_topic",
        "boolean",
        None,
        EMPTY_ENUM,
        Some("false"),
        "Activate the Behavior topic when selected.",
    ),
    SourceFieldContract::optional(
        "topic_ttl",
        "integer|null",
        None,
        EMPTY_ENUM,
        Some("null"),
        "Optional topic TTL override.",
    ),
    SourceFieldContract::optional(
        "followup_scope",
        "string|null",
        Some("followup-id"),
        EMPTY_ENUM,
        Some("null"),
        "Optional follow-up scope required by this Behavior.",
    ),
    SourceFieldContract::optional(
        "repair_continuation_candidate",
        "boolean",
        None,
        EMPTY_ENUM,
        Some("false"),
        "Allow this Behavior to participate in deterministic repair continuation.",
    ),
    SourceFieldContract::optional(
        "repeat_same_input_after",
        "integer|null",
        None,
        EMPTY_ENUM,
        Some("null"),
        "Behavior-specific repeat-input stage threshold.",
    ),
    SourceFieldContract::optional(
        "repeat_same_meaning_after",
        "integer|null",
        None,
        EMPTY_ENUM,
        Some("null"),
        "Behavior-specific repeat-Meaning stage threshold.",
    ),
    SourceFieldContract::optional(
        "requires_values",
        "array",
        Some("value-requirement"),
        EMPTY_ENUM,
        Some("[]"),
        "All exact typed value requirements must match.",
    ),
    SourceFieldContract::optional(
        "forbidden_values",
        "array",
        Some("value-requirement"),
        EMPTY_ENUM,
        Some("[]"),
        "Any matching exact typed value blocks the Behavior.",
    ),
    SourceFieldContract::required(
        "responses",
        "array",
        Some("response"),
        EMPTY_ENUM,
        "Authored response definitions.",
    ),
];

const STYLE_LEXICON_FIELDS: &[SourceFieldContract] = &[
    SourceFieldContract::optional(
        "formal_terms",
        "array",
        Some("string"),
        EMPTY_ENUM,
        Some("[]"),
        "Terms associated with formal style evidence.",
    ),
    SourceFieldContract::optional(
        "informal_terms",
        "array",
        Some("string"),
        EMPTY_ENUM,
        Some("[]"),
        "Terms associated with informal style evidence.",
    ),
];

const FALLBACK_BEHAVIOR_FIELDS: &[SourceFieldContract] = &[
    SourceFieldContract::required(
        "id",
        "string",
        None,
        EMPTY_ENUM,
        "Fallback Behavior identity.",
    ),
    SourceFieldContract::required(
        "trigger",
        "string",
        None,
        &["unresolved", "repeat"],
        "Fallback trigger.",
    ),
    SourceFieldContract::optional(
        "priority",
        "integer",
        None,
        EMPTY_ENUM,
        Some("0"),
        "Fallback selection priority.",
    ),
    SourceFieldContract::optional(
        "conditions",
        "array",
        Some("value-condition"),
        EMPTY_ENUM,
        Some("[]"),
        "Eligibility conditions.",
    ),
    SourceFieldContract::required(
        "responses",
        "array",
        Some("response"),
        EMPTY_ENUM,
        "Authored fallback responses.",
    ),
];

const CAPABILITY_RESULT_BEHAVIOR_FIELDS: &[SourceFieldContract] = &[
    SourceFieldContract::required(
        "id",
        "string",
        None,
        EMPTY_ENUM,
        "Capability-result Behavior identity.",
    ),
    SourceFieldContract::required(
        "capability",
        "string",
        Some("capability-id"),
        EMPTY_ENUM,
        "Capability whose host result is handled.",
    ),
    SourceFieldContract::required(
        "capability_version",
        "string",
        None,
        EMPTY_ENUM,
        "Exact capability version handled by this Behavior.",
    ),
    SourceFieldContract::optional(
        "succeeded",
        "boolean|null",
        None,
        EMPTY_ENUM,
        Some("null"),
        "Optional success/failure filter.",
    ),
    SourceFieldContract::optional(
        "error_code",
        "string|null",
        None,
        EMPTY_ENUM,
        Some("null"),
        "Optional host error-code filter.",
    ),
    SourceFieldContract::required(
        "responses",
        "array",
        Some("response"),
        EMPTY_ENUM,
        "Responses emitted for the matching result.",
    ),
];

const OPENING_FIELDS: &[SourceFieldContract] = &[
    SourceFieldContract::required("id", "string", None, EMPTY_ENUM, "Opening identity."),
    SourceFieldContract::optional(
        "topic",
        "string|null",
        Some("topic-id"),
        EMPTY_ENUM,
        Some("null"),
        "Optional topic activated by the opening.",
    ),
    SourceFieldContract::optional(
        "topic_ttl",
        "integer|null",
        None,
        EMPTY_ENUM,
        Some("null"),
        "Optional opening topic TTL.",
    ),
    SourceFieldContract::required(
        "responses",
        "array",
        Some("response"),
        EMPTY_ENUM,
        "Responses available for this opening.",
    ),
];

const RESPONSE_FIELDS: &[SourceFieldContract] = &[
    SourceFieldContract::required("id", "string", None, EMPTY_ENUM, "Response identity."),
    SourceFieldContract::optional(
        "kind",
        "string",
        None,
        &[
            "normal",
            "hint",
            "repeat",
            "annoyed_repeat",
            "final_repeat",
            "fallback",
            "opening",
        ],
        Some("normal"),
        "Conversation response kind.",
    ),
    SourceFieldContract::optional(
        "texts",
        "array",
        Some("localized-texts"),
        EMPTY_ENUM,
        Some("[]"),
        "Localized response variants.",
    ),
    SourceFieldContract::optional(
        "conditions",
        "array",
        Some("value-condition"),
        EMPTY_ENUM,
        Some("[]"),
        "Response-level eligibility conditions.",
    ),
    SourceFieldContract::optional(
        "hint_level",
        "integer|null",
        None,
        EMPTY_ENUM,
        Some("null"),
        "Optional hint level.",
    ),
    SourceFieldContract::optional(
        "repeat_stage",
        "string|null",
        None,
        &["repeat", "annoyed", "final"],
        Some("null"),
        "Optional repeat stage.",
    ),
    SourceFieldContract::optional(
        "effects",
        "array",
        Some("conversation-effect"),
        EMPTY_ENUM,
        Some("[]"),
        "Deterministic author-state effects.",
    ),
    SourceFieldContract::optional(
        "opens_followup",
        "object|null",
        Some("followup-directive"),
        EMPTY_ENUM,
        Some("null"),
        "Optional follow-up scope opened by this response.",
    ),
    SourceFieldContract::optional(
        "extra_messages",
        "array",
        Some("extra-message"),
        EMPTY_ENUM,
        Some("[]"),
        "Additional bounded messages.",
    ),
    SourceFieldContract::optional(
        "assets",
        "array",
        Some("response-asset"),
        EMPTY_ENUM,
        Some("[]"),
        "Referenced Package assets.",
    ),
    SourceFieldContract::optional(
        "links",
        "array",
        Some("response-link"),
        EMPTY_ENUM,
        Some("[]"),
        "Renderer-independent links.",
    ),
];

const CAPABILITY_FIELDS: &[SourceFieldContract] = &[
    SourceFieldContract::required(
        "contract",
        "object",
        Some("capability-contract"),
        EMPTY_ENUM,
        "Host capability declaration.",
    ),
    SourceFieldContract::optional(
        "host_effects",
        "array",
        Some("host-effect"),
        EMPTY_ENUM,
        Some("[]"),
        "Declared host-effect metadata.",
    ),
];

const CAPABILITY_CONTRACT_FIELDS: &[SourceFieldContract] = &[
    SourceFieldContract::required("id", "string", None, EMPTY_ENUM, "Capability identity."),
    SourceFieldContract::required(
        "version",
        "string",
        None,
        EMPTY_ENUM,
        "Exact capability contract version.",
    ),
    SourceFieldContract::required(
        "title",
        "string",
        None,
        EMPTY_ENUM,
        "Human/agent-facing title.",
    ),
    SourceFieldContract::required(
        "description",
        "string",
        None,
        EMPTY_ENUM,
        "Capability purpose and contract description.",
    ),
    SourceFieldContract::required(
        "input_schema",
        "object",
        Some("json-schema"),
        EMPTY_ENUM,
        "Canonical input JSON Schema.",
    ),
    SourceFieldContract::optional(
        "output_schema",
        "object|null",
        Some("json-schema"),
        EMPTY_ENUM,
        Some("null"),
        "Optional output JSON Schema.",
    ),
    SourceFieldContract::optional(
        "reference_kinds",
        "array",
        Some("reference-kind"),
        EMPTY_ENUM,
        Some("[]"),
        "Host reference kinds accepted by this capability.",
    ),
    SourceFieldContract::required(
        "effect_class",
        "string",
        None,
        &["pure", "reversible", "irreversible", "external"],
        "Declared effect/risk class.",
    ),
    SourceFieldContract::required(
        "confirmation_hint",
        "string",
        None,
        &["never", "conditional", "always"],
        "Author-declared confirmation expectation.",
    ),
];

const CAPABILITY_BINDING_FIELDS: &[SourceFieldContract] = &[
    SourceFieldContract::required(
        "id",
        "string",
        None,
        EMPTY_ENUM,
        "Capability binding identity.",
    ),
    SourceFieldContract::required(
        "trigger",
        "object",
        Some("capability-trigger"),
        EMPTY_ENUM,
        "Meaning/Behavior/Response trigger selector.",
    ),
    SourceFieldContract::required(
        "capability",
        "string",
        Some("capability-id"),
        EMPTY_ENUM,
        "Capability invoked by this binding.",
    ),
    SourceFieldContract::optional(
        "arguments",
        "array",
        Some("argument-binding"),
        EMPTY_ENUM,
        Some("[]"),
        "Deterministic argument bindings.",
    ),
];

const CAPABILITY_POLICY_FIELDS: &[SourceFieldContract] = &[
    SourceFieldContract::required(
        "id",
        "string",
        None,
        EMPTY_ENUM,
        "Capability policy identity.",
    ),
    SourceFieldContract::required(
        "capability",
        "string",
        Some("capability-id"),
        EMPTY_ENUM,
        "Capability governed by this policy.",
    ),
    SourceFieldContract::optional(
        "priority",
        "integer",
        None,
        EMPTY_ENUM,
        Some("0"),
        "Policy priority.",
    ),
    SourceFieldContract::optional(
        "conditions",
        "array",
        Some("admission-predicate"),
        EMPTY_ENUM,
        Some("[]"),
        "Admission predicates.",
    ),
    SourceFieldContract::required(
        "effect",
        "object",
        Some("policy-effect"),
        EMPTY_ENUM,
        "Allow/reject/confirm admission effect.",
    ),
];

const CAPABILITY_CONFIG_FIELDS: &[SourceFieldContract] = &[
    SourceFieldContract::optional(
        "schema_limits",
        "object",
        Some("schema-limits"),
        EMPTY_ENUM,
        Some("canonical defaults"),
        "Optional tighter schema evaluation limits.",
    ),
    SourceFieldContract::optional(
        "max_proposals_per_turn",
        "integer",
        None,
        EMPTY_ENUM,
        Some("canonical default"),
        "Bound on admitted proposals per turn.",
    ),
    SourceFieldContract::optional(
        "max_bindings",
        "integer",
        None,
        EMPTY_ENUM,
        Some("canonical default"),
        "Bound on capability bindings.",
    ),
    SourceFieldContract::optional(
        "max_policy_rules",
        "integer",
        None,
        EMPTY_ENUM,
        Some("canonical default"),
        "Bound on capability policy rules.",
    ),
];

const NAMED_TYPE_FIELDS: &[SourceFieldContract] = &[
    SourceFieldContract::required("id", "string", None, EMPTY_ENUM, "Named type identity."),
    SourceFieldContract::required(
        "schema",
        "object",
        Some("json-schema"),
        EMPTY_ENUM,
        "Canonical JSON Schema document.",
    ),
];

const ASSET_FIELDS: &[SourceFieldContract] = &[
    SourceFieldContract::required("id", "string", None, EMPTY_ENUM, "Asset identity."),
    SourceFieldContract::required(
        "media_type",
        "string",
        None,
        EMPTY_ENUM,
        "Asset media type.",
    ),
    SourceFieldContract::required(
        "logical_path",
        "string",
        None,
        EMPTY_ENUM,
        "Safe assets/... logical path.",
    ),
    SourceFieldContract::required(
        "source",
        "string",
        Some("package-relative-path"),
        EMPTY_ENUM,
        "Package-relative source bytes path.",
    ),
];

const REGRESSION_CASE_FIELDS: &[SourceFieldContract] = &[
    SourceFieldContract::required(
        "id",
        "string",
        None,
        EMPTY_ENUM,
        "Regression Case identity.",
    ),
    SourceFieldContract::optional(
        "description",
        "string",
        None,
        EMPTY_ENUM,
        Some("\"\""),
        "Purpose of the case.",
    ),
    SourceFieldContract::required(
        "input",
        "string",
        None,
        EMPTY_ENUM,
        "User utterance under test.",
    ),
    SourceFieldContract::optional(
        "language",
        "string|null",
        Some("language-tag"),
        EMPTY_ENUM,
        Some("null"),
        "Explicit request language.",
    ),
    SourceFieldContract::optional(
        "context",
        "object",
        Some("runtime-context"),
        EMPTY_ENUM,
        Some("{}"),
        "Host context snapshot.",
    ),
    SourceFieldContract::optional(
        "initial_state",
        "object",
        Some("gvya-state"),
        EMPTY_ENUM,
        Some("{}"),
        "Initial author/conversation state.",
    ),
    SourceFieldContract::optional(
        "seed",
        "integer|null",
        None,
        EMPTY_ENUM,
        Some("null"),
        "Explicit deterministic seed.",
    ),
    SourceFieldContract::optional(
        "unix_time_ms",
        "integer|null",
        None,
        EMPTY_ENUM,
        Some("null"),
        "Explicit host time input.",
    ),
    SourceFieldContract::required(
        "expectation",
        "object",
        Some("turn-expectation"),
        EMPTY_ENUM,
        "Expected canonical observation.",
    ),
    SourceFieldContract::optional(
        "generated",
        "boolean",
        None,
        EMPTY_ENUM,
        Some("false"),
        "Generated tests do not satisfy direct mechanic-proof obligations.",
    ),
];

const SCENARIO_FIELDS: &[SourceFieldContract] = &[
    SourceFieldContract::required(
        "id",
        "string",
        None,
        EMPTY_ENUM,
        "Conversation Scenario identity.",
    ),
    SourceFieldContract::optional(
        "description",
        "string",
        None,
        EMPTY_ENUM,
        Some("\"\""),
        "Scenario purpose.",
    ),
    SourceFieldContract::optional(
        "context",
        "object",
        Some("runtime-context"),
        EMPTY_ENUM,
        Some("{}"),
        "Default host context snapshot.",
    ),
    SourceFieldContract::optional(
        "initial_state",
        "object",
        Some("gvya-state"),
        EMPTY_ENUM,
        Some("{}"),
        "Initial author/conversation state.",
    ),
    SourceFieldContract::required(
        "steps",
        "array",
        Some("scenario-step"),
        EMPTY_ENUM,
        "Ordered open/turn/capability_result/confirm steps.",
    ),
    SourceFieldContract::optional(
        "generated",
        "boolean",
        None,
        EMPTY_ENUM,
        Some("false"),
        "Generated scenarios do not satisfy direct mechanic-proof obligations.",
    ),
];

const MATCHER_PROFILE_FIELDS: &[SourceFieldContract] = &[
    SourceFieldContract::required(
        "format",
        "string",
        None,
        &["gvya.source.matcher-profile"],
        "Exact Matcher Profile source format.",
    ),
    SourceFieldContract::required("version", "integer", None, &["1"], "Exact source version."),
    SourceFieldContract::required(
        "language",
        "string",
        Some("language-tag"),
        EMPTY_ENUM,
        "Language owned by this profile.",
    ),
    SourceFieldContract::required(
        "profile",
        "object",
        Some("matcher-profile-data"),
        EMPTY_ENUM,
        "Explicit structural pattern-set data.",
    ),
];

const LANGUAGE_PROFILE_FIELDS: &[SourceFieldContract] = &[
    SourceFieldContract::required(
        "format",
        "string",
        None,
        &["gvya.source.language-profile"],
        "Exact Language Profile source format.",
    ),
    SourceFieldContract::required("version", "integer", None, &["1"], "Exact source version."),
    SourceFieldContract::required(
        "language",
        "string",
        Some("language-tag"),
        EMPTY_ENUM,
        "Language owned by this profile.",
    ),
    SourceFieldContract::required(
        "profile",
        "object",
        Some("language-profile-data"),
        EMPTY_ENUM,
        "Explicit normalization, morphology, weighting, and lexical entity data.",
    ),
];

const OBJECTS: &[SourceObjectContract] = &[
    SourceObjectContract {
        kind: "project",
        description: "gvya.project.json root document",
        identity_field: Some("project_id"),
        fields: PROJECT_FIELDS,
    },
    SourceObjectContract {
        kind: "package",
        description: "Package root document",
        identity_field: None,
        fields: PACKAGE_FIELDS,
    },
    SourceObjectContract {
        kind: "package-manifest",
        description: "Package manifest",
        identity_field: Some("id"),
        fields: PACKAGE_MANIFEST_FIELDS,
    },
    SourceObjectContract {
        kind: "package-fragments",
        description: "Explicit Package fragment-file index",
        identity_field: None,
        fields: PACKAGE_CONTENTS_FIELDS,
    },
    SourceObjectContract {
        kind: "contribution",
        description: "Contribution envelope shared by Package namespaces",
        identity_field: Some("id"),
        fields: CONTRIBUTION_FIELDS,
    },
    SourceObjectContract {
        kind: "meaning",
        description: "Semantic Meaning source object",
        identity_field: Some("id"),
        fields: MEANING_FIELDS,
    },
    SourceObjectContract {
        kind: "behavior",
        description: "Conversation Behavior source object",
        identity_field: Some("id"),
        fields: BEHAVIOR_FIELDS,
    },
    SourceObjectContract {
        kind: "fallback-behavior",
        description: "Fallback Behavior source object",
        identity_field: Some("id"),
        fields: FALLBACK_BEHAVIOR_FIELDS,
    },
    SourceObjectContract {
        kind: "capability-result-behavior",
        description: "Capability-result continuation Behavior",
        identity_field: Some("id"),
        fields: CAPABILITY_RESULT_BEHAVIOR_FIELDS,
    },
    SourceObjectContract {
        kind: "opening",
        description: "Conversation opening source object",
        identity_field: Some("id"),
        fields: OPENING_FIELDS,
    },
    SourceObjectContract {
        kind: "response",
        description: "Response definition nested in a Behavior/Opening",
        identity_field: Some("id"),
        fields: RESPONSE_FIELDS,
    },
    SourceObjectContract {
        kind: "style-lexicon",
        description: "Formal/informal style lexicon contribution value",
        identity_field: Some("contribution.id"),
        fields: STYLE_LEXICON_FIELDS,
    },
    SourceObjectContract {
        kind: "capability",
        description: "Capability declaration contribution value",
        identity_field: Some("contract.id"),
        fields: CAPABILITY_FIELDS,
    },
    SourceObjectContract {
        kind: "capability-contract",
        description: "Host capability contract",
        identity_field: Some("id"),
        fields: CAPABILITY_CONTRACT_FIELDS,
    },
    SourceObjectContract {
        kind: "capability-binding",
        description: "Capability invocation binding",
        identity_field: Some("id"),
        fields: CAPABILITY_BINDING_FIELDS,
    },
    SourceObjectContract {
        kind: "capability-policy",
        description: "Capability admission policy",
        identity_field: Some("id"),
        fields: CAPABILITY_POLICY_FIELDS,
    },
    SourceObjectContract {
        kind: "capability-config",
        description: "Capability admission/schema configuration contribution value",
        identity_field: None,
        fields: CAPABILITY_CONFIG_FIELDS,
    },
    SourceObjectContract {
        kind: "named-type",
        description: "Named JSON Schema type",
        identity_field: Some("id"),
        fields: NAMED_TYPE_FIELDS,
    },
    SourceObjectContract {
        kind: "asset",
        description: "Package asset declaration",
        identity_field: Some("id"),
        fields: ASSET_FIELDS,
    },
    SourceObjectContract {
        kind: "regression-case",
        description: "Single-turn authored Regression Case",
        identity_field: Some("id"),
        fields: REGRESSION_CASE_FIELDS,
    },
    SourceObjectContract {
        kind: "scenario",
        description: "Multi-step authored Conversation Scenario",
        identity_field: Some("id"),
        fields: SCENARIO_FIELDS,
    },
    SourceObjectContract {
        kind: "language-profile",
        description: "Standalone language mechanics profile document",
        identity_field: Some("language"),
        fields: LANGUAGE_PROFILE_FIELDS,
    },
    SourceObjectContract {
        kind: "matcher-profile",
        description: "Standalone structural Matcher Profile document",
        identity_field: Some("language"),
        fields: MATCHER_PROFILE_FIELDS,
    },
];

fn field_json(field: &SourceFieldContract) -> JsonValue {
    json!({
        "name": field.name,
        "required": field.required,
        "type": field.value_type,
        "item_kind": field.item_kind,
        "enum": field.enum_values,
        "default": field.default,
        "description": field.description,
    })
}

fn object_json(object: &SourceObjectContract) -> JsonValue {
    json!({
        "kind": object.kind,
        "description": object.description,
        "identity_field": object.identity_field,
        "unknown_fields": "rejected",
        "fields": object.fields.iter().map(field_json).collect::<Vec<_>>(),
    })
}

const NESTED_KINDS: &[&str] = &[
    "admission-predicate",
    "argument-binding",
    "binding-source",
    "capability-id",
    "capability-trigger",
    "contribution-mode",
    "conversation-config",
    "conversation-effect",
    "extra-message",
    "followup-directive",
    "followup-id",
    "gvya-state",
    "host-effect",
    "json-schema",
    "language-tag",
    "localized-sample",
    "localized-texts",
    "language-profile-data",
    "matcher-profile-data",
    "meaning-id",
    "namespace-value",
    "package-dependency",
    "package-relative-path",
    "policy-effect",
    "reference-kind",
    "reference-spec",
    "response-asset",
    "response-link",
    "runtime-context",
    "scenario-step",
    "schema-limits",
    "semantic-config",
    "slot-spec",
    "source-path",
    "string",
    "structural-pattern",
    "topic-id",
    "turn-expectation",
    "value-condition",
    "value-requirement",
];

fn key_fields(keys: &[&str]) -> Vec<JsonValue> {
    keys.iter().map(|name| json!({"name": name})).collect()
}

fn nested_object(kind: &str, keys: &[&str], description: &str) -> JsonValue {
    json!({
        "kind": kind,
        "kind_class": "nested-object",
        "description": description,
        "unknown_fields": "rejected",
        "fields": key_fields(keys),
    })
}

fn scalar_kind(kind: &str, description: &str) -> JsonValue {
    json!({
        "kind": kind,
        "kind_class": "scalar",
        "type": "string",
        "description": description,
    })
}

fn dynamic_object_kind(kind: &str, description: &str) -> JsonValue {
    json!({
        "kind": kind,
        "kind_class": "dynamic-object",
        "type": "object",
        "description": description,
        "unknown_fields": "domain-defined",
    })
}

fn variant(kind: &str, discriminator: &str, value: &str, keys: &[&str]) -> JsonValue {
    json!({
        "kind": kind,
        "discriminator": discriminator,
        "value": value,
        "unknown_fields": "rejected",
        "fields": key_fields(keys),
    })
}

fn nested_kind_json(kind: &str) -> Option<JsonValue> {
    Some(match kind {
        "package-dependency" => nested_object(
            kind,
            PACKAGE_DEPENDENCY_KEYS,
            "Package dependency declaration",
        ),
        "semantic-config" => {
            nested_object(kind, SEMANTIC_CONFIG_KEYS, "Project semantic configuration")
        }
        "conversation-config" => nested_object(
            kind,
            CONVERSATION_CONFIG_KEYS,
            "Project conversation configuration",
        ),
        "localized-sample" => nested_object(
            kind,
            LOCALIZED_SAMPLE_KEYS,
            "Language-tagged Meaning sample",
        ),
        "localized-texts" => nested_object(
            kind,
            LOCALIZED_TEXTS_KEYS,
            "Language-tagged response variants",
        ),
        "slot-spec" => nested_object(kind, SLOT_SPEC_KEYS, "Meaning slot declaration"),
        "reference-spec" => {
            nested_object(kind, REFERENCE_SPEC_KEYS, "Meaning reference declaration")
        }
        "value-requirement" => nested_object(
            kind,
            VALUE_REQUIREMENT_KEYS,
            "Exact required/forbidden state value",
        ),
        "value-condition" => {
            nested_object(kind, VALUE_CONDITION_KEYS, "Conversation state condition")
        }
        "namespace-value" => nested_object(kind, NAMESPACE_VALUE_KEYS, "Namespaced state path"),
        "followup-directive" => {
            nested_object(kind, FOLLOWUP_DIRECTIVE_KEYS, "Follow-up opening directive")
        }
        "extra-message" => {
            nested_object(kind, EXTRA_MESSAGE_KEYS, "Optional extra response message")
        }
        "response-asset" => nested_object(kind, RESPONSE_ASSET_KEYS, "Response asset reference"),
        "response-link" => nested_object(kind, RESPONSE_LINK_KEYS, "Response link"),
        "host-effect" => {
            nested_object(kind, HOST_EFFECT_KEYS, "Declared host-side effect metadata")
        }
        "capability-trigger" => {
            nested_object(kind, CAPABILITY_TRIGGER_KEYS, "Capability binding trigger")
        }
        "argument-binding" => json!({
            "kind": kind,
            "kind_class": "nested-object",
            "description": "Capability argument binding",
            "unknown_fields": "rejected",
            "fields": [
                {"name":"target", "type":"string"},
                {"name":"source", "type":"object", "item_kind":"binding-source"}
            ]
        }),
        "binding-source" => json!({
            "kind": kind,
            "kind_class": "discriminated-union",
            "description": "Capability argument source",
            "discriminator": "type",
            "variants": [
                variant(kind, "type", "meaning_slot", BINDING_SOURCE_MEANING_SLOT_KEYS),
                variant(kind, "type", "meaning_reference", BINDING_SOURCE_REFERENCE_KEYS),
                variant(kind, "type", "focus_reference", BINDING_SOURCE_REFERENCE_KEYS),
                variant(kind, "type", "context_path", BINDING_SOURCE_PATH_KEYS),
                variant(kind, "type", "author_state_path", BINDING_SOURCE_PATH_KEYS),
                variant(kind, "type", "literal", BINDING_SOURCE_LITERAL_KEYS)
            ]
        }),
        "admission-predicate" => nested_object(
            kind,
            ADMISSION_PREDICATE_KEYS,
            "Capability admission predicate",
        ),
        "policy-effect" => json!({
            "kind": kind,
            "kind_class": "discriminated-union",
            "description": "Capability policy effect",
            "discriminator": "type",
            "variants": [
                variant(kind, "type", "allow", POLICY_EFFECT_ALLOW_KEYS),
                variant(kind, "type", "require_confirmation", POLICY_EFFECT_REASON_KEYS),
                variant(kind, "type", "deny", POLICY_EFFECT_REASON_KEYS)
            ]
        }),
        "conversation-effect" => json!({
            "kind": kind,
            "kind_class": "discriminated-union",
            "description": "Authored conversation state mutation",
            "discriminator": "type",
            "variants": [
                variant(kind, "type", "assign", CONVERSATION_EFFECT_ASSIGN_KEYS),
                variant(kind, "type", "increment", CONVERSATION_EFFECT_INCREMENT_KEYS)
            ]
        }),
        "schema-limits" => nested_object(
            kind,
            SCHEMA_LIMIT_KEYS,
            "Capability JSON Schema evaluation limits",
        ),
        "turn-expectation" => nested_object(
            kind,
            TURN_EXPECTATION_KEYS,
            "Regression/scenario turn expectations",
        ),
        "scenario-step" => json!({
            "kind": kind,
            "kind_class": "discriminated-union",
            "description": "Conversation Scenario step",
            "discriminator": "type",
            "variants": [
                variant(kind, "type", "open", &["type","language","context","seed","unix_time_ms","expectation"]),
                variant(kind, "type", "turn", &["type","say","language","context","reference_candidates","resolver_context","hint","seed","unix_time_ms","expectation"]),
                variant(kind, "type", "capability_result", &["type","proposal_from_step","proposal_capability","proposal_ordinal","succeeded","output","error_code","language","context","seed","unix_time_ms","expectation"]),
                variant(kind, "type", "confirm", &["type","proposal_from_step","proposal_capability","proposal_ordinal","confirmed","context","unix_time_ms","expectation"])
            ]
        }),
        "structural-pattern" => json!({
            "kind": kind,
            "kind_class": "scalar",
            "type": "string",
            "description": "Deterministic whole-utterance structural pattern expression"
        }),
        "contribution-mode" => json!({
            "kind": kind,
            "kind_class": "scalar",
            "type": ["string","object"],
            "description": "Contribution add/replace operation; replacement object is parser-validated"
        }),
        "json-schema" => dynamic_object_kind(
            kind,
            "JSON Schema object validated by the canonical schema compiler",
        ),
        "runtime-context" => dynamic_object_kind(kind, "Explicit runtime context key/value object"),
        "gvya-state" => dynamic_object_kind(kind, "Explicit authored GVYA state object"),
        "language-profile-data" => dynamic_object_kind(
            kind,
            "Language Profile data object validated by the language-profile parser",
        ),
        "matcher-profile-data" => dynamic_object_kind(
            kind,
            "Structural pattern-set data validated by the matcher-profile parser",
        ),
        "capability-id" => scalar_kind(kind, "Capability identifier"),
        "followup-id" => scalar_kind(kind, "Follow-up identifier"),
        "language-tag" => scalar_kind(kind, "Declared language tag"),
        "meaning-id" => scalar_kind(kind, "Meaning identifier"),
        "package-relative-path" => {
            scalar_kind(kind, "Safe path relative to the owning Package root")
        }
        "reference-kind" => scalar_kind(kind, "Reference-kind identifier"),
        "source-path" => scalar_kind(kind, "Safe canonical source-tree path"),
        "string" => scalar_kind(kind, "String value"),
        "topic-id" => scalar_kind(kind, "Conversation topic identifier"),
        _ => return None,
    })
}

fn kind_summary(kind: &str) -> Option<JsonValue> {
    if let Some(object) = OBJECTS.iter().find(|object| object.kind == kind) {
        return Some(json!({
            "kind": object.kind,
            "kind_class": "source-object",
            "description": object.description,
            "identity_field": object.identity_field,
        }));
    }
    nested_kind_json(kind).map(|value| {
        json!({
            "kind": kind,
            "kind_class": value.get("kind_class").cloned().unwrap_or(JsonValue::Null),
            "description": value.get("description").cloned().unwrap_or(JsonValue::Null),
            "identity_field": JsonValue::Null,
        })
    })
}

/// Returns the canonical machine-readable source-authoring contract index or one exact object shape.
///
/// Parser/compiler validation remains authoritative for cross-field constraints and bounded semantic
/// rules that cannot be expressed as a shallow object field inventory.
pub fn source_contract_json(kind: Option<&str>) -> Result<JsonValue, String> {
    let selected = match kind {
        None => None,
        Some(kind) => {
            if let Some(object) = OBJECTS.iter().find(|object| object.kind == kind) {
                Some(json!({
                    "kind_class": "source-object",
                    "object": object_json(object),
                }))
            } else if let Some(nested) = nested_kind_json(kind) {
                Some(json!({
                    "kind_class": nested.get("kind_class").cloned().unwrap_or(JsonValue::Null),
                    "object": nested,
                }))
            } else {
                return Err(format!("unknown source schema kind {kind:?}"));
            }
        }
    };
    let document_versions = json!({
        "project": 1,
        "package": 1,
        "matcher_profile": 1,
    });
    Ok(match selected {
        Some(selected) => json!({
            "format": "gvya.cli.source-schema",
            "version": 1,
            "document_versions": document_versions,
            "authority": "gvya-compiler source parser",
            "semantic_validation": "gvya check / gvya check-package",
            "kind_class": selected["kind_class"],
            "object": selected["object"],
        }),
        None => {
            let mut kinds = OBJECTS.iter().map(|object| object.kind).collect::<Vec<_>>();
            kinds.extend(NESTED_KINDS.iter().copied());
            kinds.sort_unstable();
            kinds.dedup();
            json!({
                "format": "gvya.cli.source-schema-index",
                "version": 1,
                "document_versions": document_versions,
                "authority": "gvya-compiler source parser",
                "semantic_validation": "gvya check / gvya check-package",
                "kinds": kinds.into_iter().filter_map(kind_summary).collect::<Vec<_>>(),
            })
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_index_exposes_major_machine_authoring_objects() {
        let index = source_contract_json(None).unwrap();
        let kinds = index["kinds"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|row| row["kind"].as_str())
            .collect::<Vec<_>>();
        for expected in [
            "project",
            "package",
            "meaning",
            "behavior",
            "response",
            "style-lexicon",
            "capability",
            "regression-case",
            "scenario",
            "language-profile",
            "matcher-profile",
        ] {
            assert!(
                kinds.contains(&expected),
                "missing source schema {expected}"
            );
        }
    }

    #[test]
    fn behavior_schema_is_strict_and_reports_required_relationships() {
        let schema = source_contract_json(Some("behavior")).unwrap();
        assert_eq!(schema["object"]["unknown_fields"], "rejected");
        let fields = schema["object"]["fields"].as_array().unwrap();
        assert!(
            fields
                .iter()
                .any(|field| field["name"] == "meaning" && field["required"] == true)
        );
        assert!(
            fields
                .iter()
                .any(|field| field["name"] == "responses" && field["required"] == true)
        );
    }
    #[test]
    fn schema_exposes_current_document_versions_and_resolves_nested_kinds() {
        let index = source_contract_json(None).unwrap();
        assert_eq!(index["version"], 1);
        assert_eq!(index["document_versions"]["project"], 1);
        assert_eq!(index["document_versions"]["package"], 1);
        assert_eq!(index["document_versions"]["matcher_profile"], 1);
        assert!(index.get("source_version").is_none());
        for kind in [
            "value-requirement",
            "capability-trigger",
            "argument-binding",
            "binding-source",
            "conversation-effect",
            "policy-effect",
            "turn-expectation",
            "scenario-step",
            "runtime-context",
        ] {
            let schema = source_contract_json(Some(kind)).unwrap();
            assert_eq!(schema["version"], 1, "{kind}");
            assert_eq!(schema["object"]["kind"], kind, "{kind}");
        }
    }

    #[test]
    fn every_exposed_item_kind_is_resolvable() {
        for object in OBJECTS {
            for field in object.fields {
                if let Some(kind) = field.item_kind {
                    assert!(
                        source_contract_json(Some(kind)).is_ok(),
                        "{}.{}, unresolved item kind {kind}",
                        object.kind,
                        field.name
                    );
                }
            }
        }
        for kind in NESTED_KINDS {
            assert!(
                source_contract_json(Some(kind)).is_ok(),
                "unresolved nested kind {kind}"
            );
        }
    }
}
