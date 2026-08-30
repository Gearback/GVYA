//! Capability/schema executable IR serialization.

use super::helpers::*;
use super::*;

pub(super) fn capabilities(project: &ComposedProject) -> Result<JsonValue, IrError> {
    let catalog = &project.capability_catalog;
    let definitions = catalog
        .capability_ids()
        .filter_map(|id| catalog.definition(id))
        .map(capability_definition)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(object([
        ("definitions", JsonValue::Array(definitions)),
        (
            "bindings",
            JsonValue::Array(
                catalog
                    .bindings()
                    .iter()
                    .map(capability_binding)
                    .collect::<Result<_, _>>()?,
            ),
        ),
        (
            "policies",
            JsonValue::Array(
                catalog
                    .policies()
                    .iter()
                    .map(capability_policy)
                    .collect::<Result<_, _>>()?,
            ),
        ),
        ("config", capability_config(catalog.config())),
    ]))
}

pub(super) fn capability_definition(value: &CapabilityDefinition) -> Result<JsonValue, IrError> {
    Ok(object([
        (
            "contract",
            object([
                ("id", string(value.contract.id.as_str())),
                ("version", string(value.contract.version.as_str())),
                ("title", string(&value.contract.title)),
                ("description", string(&value.contract.description)),
                (
                    "input_schema",
                    schema_document(value.contract.input_schema.as_str())?,
                ),
                (
                    "output_schema",
                    match &value.contract.output_schema {
                        Some(row) => schema_document(row.as_str())?,
                        None => JsonValue::Null,
                    },
                ),
                (
                    "reference_kinds",
                    JsonValue::Array(
                        value
                            .contract
                            .reference_kinds
                            .iter()
                            .map(|row| string(row.as_str()))
                            .collect(),
                    ),
                ),
                (
                    "effect_class",
                    string(match value.contract.effect_class {
                        EffectClass::Pure => "pure",
                        EffectClass::Reversible => "reversible",
                        EffectClass::Irreversible => "irreversible",
                        EffectClass::External => "external",
                    }),
                ),
                (
                    "confirmation_hint",
                    string(match value.contract.confirmation_hint {
                        ConfirmationHint::Never => "never",
                        ConfirmationHint::Conditional => "conditional",
                        ConfirmationHint::Always => "always",
                    }),
                ),
            ]),
        ),
        ("input_shape", value_schema(&value.input_shape)?),
        (
            "output_shape",
            match &value.output_shape {
                Some(shape) => value_schema(shape)?,
                None => JsonValue::Null,
            },
        ),
        (
            "host_effects",
            JsonValue::Array(
                value
                    .host_effects
                    .iter()
                    .map(|effect| {
                        object([
                            ("resource", string(&effect.resource)),
                            (
                                "kind",
                                string(match effect.kind {
                                    HostEffectKind::Read => "read",
                                    HostEffectKind::Update => "update",
                                    HostEffectKind::Create => "create",
                                    HostEffectKind::Delete => "delete",
                                    HostEffectKind::External => "external",
                                }),
                            ),
                            ("summary", string(&effect.summary)),
                        ])
                    })
                    .collect(),
            ),
        ),
    ]))
}

pub(super) fn capability_binding(
    value: &gvya_kernel::capability::CapabilityBindingRule,
) -> Result<JsonValue, IrError> {
    Ok(object([
        ("id", string(value.id.as_str())),
        (
            "trigger",
            object([
                (
                    "meaning",
                    option_string(value.trigger.meaning.as_ref().map(|row| row.as_str())),
                ),
                (
                    "behavior",
                    option_string(value.trigger.behavior.as_ref().map(|row| row.as_str())),
                ),
                (
                    "response",
                    option_string(value.trigger.response.as_ref().map(|row| row.as_str())),
                ),
            ]),
        ),
        ("capability", string(value.capability.as_str())),
        (
            "arguments",
            JsonValue::Array(
                value
                    .arguments
                    .iter()
                    .map(argument_binding)
                    .collect::<Result<_, _>>()?,
            ),
        ),
    ]))
}

pub(super) fn argument_binding(value: &ArgumentBinding) -> Result<JsonValue, IrError> {
    Ok(object([
        ("target", string(&value.target.display())),
        ("source", binding_source(&value.source)?),
    ]))
}

pub(super) fn binding_source(value: &BindingSource) -> Result<JsonValue, IrError> {
    Ok(match value {
        BindingSource::MeaningSlot(name) => {
            object([("type", string("meaning_slot")), ("name", string(name))])
        }
        BindingSource::MeaningReference { kind, projection } => object([
            ("type", string("meaning_reference")),
            ("kind", string(kind.as_str())),
            ("projection", string(reference_projection(*projection))),
        ]),
        BindingSource::FocusReference { kind, projection } => object([
            ("type", string("focus_reference")),
            ("kind", string(kind.as_str())),
            ("projection", string(reference_projection(*projection))),
        ]),
        BindingSource::ContextPath(path) => {
            object([("type", string("context_path")), ("path", string(path))])
        }
        BindingSource::AuthorStatePath(path) => object([
            ("type", string("author_state_path")),
            ("path", string(path)),
        ]),
        BindingSource::Literal(value) => {
            object([("type", string("literal")), ("value", model_value(value)?)])
        }
    })
}
pub(super) fn reference_projection(value: ReferenceProjection) -> &'static str {
    match value {
        ReferenceProjection::Id => "id",
        ReferenceProjection::Object => "object",
    }
}

pub(super) fn capability_policy(value: &CapabilityPolicyRule) -> Result<JsonValue, IrError> {
    Ok(object([
        ("id", string(value.id.as_str())),
        ("capability", string(value.capability.as_str())),
        ("priority", integer(i64::from(value.priority))),
        (
            "conditions",
            JsonValue::Array(
                value
                    .conditions
                    .iter()
                    .map(admission_predicate)
                    .collect::<Result<_, _>>()?,
            ),
        ),
        (
            "effect",
            match &value.effect {
                PolicyEffect::Allow => object([("type", string("allow"))]),
                PolicyEffect::RequireConfirmation { reason_code } => object([
                    ("type", string("require_confirmation")),
                    ("reason_code", string(reason_code)),
                ]),
                PolicyEffect::Deny { reason_code } => object([
                    ("type", string("deny")),
                    ("reason_code", string(reason_code)),
                ]),
            },
        ),
    ]))
}

pub(super) fn admission_predicate(value: &AdmissionPredicate) -> Result<JsonValue, IrError> {
    Ok(object([
        (
            "namespace",
            string(match value.namespace {
                AdmissionNamespace::Arguments => "arguments",
                AdmissionNamespace::Context => "context",
                AdmissionNamespace::Author => "author",
                AdmissionNamespace::Conversation => "conversation",
                AdmissionNamespace::System => "system",
            }),
        ),
        ("path", string(&value.path)),
        ("op", string(admission_predicate_label(value.op))),
        (
            "value",
            match &value.value {
                Some(row) => model_value(row)?,
                None => JsonValue::Null,
            },
        ),
    ]))
}

pub(super) fn capability_config(value: &CapabilityConfig) -> JsonValue {
    object([
        ("schema_limits", schema_limits(value.schema_limits)),
        (
            "max_proposals_per_turn",
            usize_json(value.max_proposals_per_turn),
        ),
        ("max_bindings", usize_json(value.max_bindings)),
        ("max_policy_rules", usize_json(value.max_policy_rules)),
    ])
}
pub(super) fn schema_limits(value: SchemaLimits) -> JsonValue {
    object([
        ("max_depth", usize_json(value.max_depth)),
        ("max_array_items", usize_json(value.max_array_items)),
        (
            "max_object_properties",
            usize_json(value.max_object_properties),
        ),
        ("max_string_bytes", usize_json(value.max_string_bytes)),
        ("max_errors", usize_json(value.max_errors)),
    ])
}

pub(super) fn value_schema(value: &ValueSchema) -> Result<JsonValue, IrError> {
    Ok(match value {
        ValueSchema::Null => object([("type", string("null"))]),
        ValueSchema::Boolean => object([("type", string("boolean"))]),
        ValueSchema::Number { minimum, maximum } => object([
            ("type", string("number")),
            ("minimum", finite_option(*minimum, "schema.number.minimum")?),
            ("maximum", finite_option(*maximum, "schema.number.maximum")?),
        ]),
        ValueSchema::Integer { minimum, maximum } => object([
            ("type", string("integer")),
            ("minimum", minimum.map_or(JsonValue::Null, integer)),
            ("maximum", maximum.map_or(JsonValue::Null, integer)),
        ]),
        ValueSchema::String {
            min_chars,
            max_chars,
            allowed,
        } => object([
            ("type", string("string")),
            ("min_chars", option_usize(*min_chars)),
            ("max_chars", option_usize(*max_chars)),
            ("allowed", set_strings(allowed)),
        ]),
        ValueSchema::Array {
            items,
            min_items,
            max_items,
        } => object([
            ("type", string("array")),
            ("items", value_schema(items)?),
            ("min_items", option_usize(*min_items)),
            ("max_items", option_usize(*max_items)),
        ]),
        ValueSchema::Object(object_value) => capability_object_schema(object_value)?,
        ValueSchema::Enum(values) => object([
            ("type", string("enum")),
            (
                "values",
                JsonValue::Array(values.iter().map(model_value).collect::<Result<_, _>>()?),
            ),
        ]),
        ValueSchema::OneOf(values) => object([
            ("type", string("one_of")),
            (
                "variants",
                JsonValue::Array(values.iter().map(value_schema).collect::<Result<_, _>>()?),
            ),
        ]),
    })
}

pub(super) fn capability_object_schema(
    value: &CapabilityObjectSchema,
) -> Result<JsonValue, IrError> {
    let mut properties = Map::new();
    for (key, schema) in &value.properties {
        properties.insert(key.clone(), value_schema(schema)?);
    }
    Ok(object([
        ("type", string("object")),
        ("properties", JsonValue::Object(properties)),
        ("required", set_strings(&value.required)),
        (
            "additional_properties",
            JsonValue::Bool(value.additional_properties),
        ),
        ("min_properties", option_usize(value.min_properties)),
        ("max_properties", option_usize(value.max_properties)),
    ]))
}
