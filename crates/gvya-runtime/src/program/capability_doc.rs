//! Capability/schema executable document hydration.

use super::*;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CapabilitiesDoc {
    pub(super) definitions: Vec<CapabilityDefinitionDoc>,
    pub(super) bindings: Vec<CapabilityBindingDoc>,
    pub(super) policies: Vec<CapabilityPolicyDoc>,
    config: CapabilityConfigDoc,
}
impl CapabilitiesDoc {
    pub(super) fn into_catalog(self) -> Result<CapabilityCatalog, ProgramError> {
        CapabilityCatalog::new(
            self.definitions
                .into_iter()
                .map(CapabilityDefinitionDoc::into_runtime)
                .collect::<Result<_, _>>()?,
            self.bindings
                .into_iter()
                .map(CapabilityBindingDoc::into_runtime)
                .collect::<Result<_, _>>()?,
            self.policies
                .into_iter()
                .map(CapabilityPolicyDoc::into_runtime)
                .collect::<Result<_, _>>()?,
            self.config.into_runtime(),
        )
        .map_err(|issues| ProgramError::InvalidCapabilityCatalog(format!("{issues:?}")))
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CapabilityDefinitionDoc {
    contract: CapabilityContractDoc,
    input_shape: ValueSchemaDoc,
    output_shape: Option<ValueSchemaDoc>,
    host_effects: Vec<HostEffectDoc>,
}
impl CapabilityDefinitionDoc {
    pub(super) fn into_runtime(self) -> Result<CapabilityDefinition, ProgramError> {
        Ok(CapabilityDefinition {
            contract: self.contract.into_runtime()?,
            input_shape: self.input_shape.into_runtime()?,
            output_shape: self
                .output_shape
                .map(ValueSchemaDoc::into_runtime)
                .transpose()?,
            host_effects: self
                .host_effects
                .into_iter()
                .map(HostEffectDoc::into_runtime)
                .collect::<Result<_, _>>()?,
        })
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CapabilityContractDoc {
    id: String,
    version: String,
    title: String,
    description: String,
    input_schema: JsonValue,
    output_schema: Option<JsonValue>,
    reference_kinds: Vec<String>,
    effect_class: String,
    confirmation_hint: String,
}
impl CapabilityContractDoc {
    pub(super) fn into_runtime(self) -> Result<CapabilityContract, ProgramError> {
        let effect_class = match self.effect_class.as_str() {
            "pure" => EffectClass::Pure,
            "reversible" => EffectClass::Reversible,
            "irreversible" => EffectClass::Irreversible,
            "external" => EffectClass::External,
            _ => {
                return Err(ProgramError::InvalidCapabilityCatalog(format!(
                    "unknown effect class: {}",
                    self.effect_class
                )));
            }
        };
        let confirmation_hint = match self.confirmation_hint.as_str() {
            "never" => ConfirmationHint::Never,
            "conditional" => ConfirmationHint::Conditional,
            "always" => ConfirmationHint::Always,
            _ => {
                return Err(ProgramError::InvalidCapabilityCatalog(format!(
                    "unknown confirmation hint: {}",
                    self.confirmation_hint
                )));
            }
        };
        Ok(CapabilityContract {
            id: CapabilityId::new(self.id),
            version: CapabilityVersion::new(self.version),
            title: self.title,
            description: self.description,
            input_schema: SchemaDocument::new(
                serde_json::to_string(&self.input_schema)
                    .map_err(|error| ProgramError::Json(error.to_string()))?,
            ),
            output_schema: self
                .output_schema
                .map(|value| {
                    serde_json::to_string(&value)
                        .map(SchemaDocument::new)
                        .map_err(|error| ProgramError::Json(error.to_string()))
                })
                .transpose()?,
            reference_kinds: self
                .reference_kinds
                .into_iter()
                .map(ReferenceKind::new)
                .collect(),
            effect_class,
            confirmation_hint,
        })
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct HostEffectDoc {
    resource: String,
    kind: String,
    summary: String,
}
impl HostEffectDoc {
    pub(super) fn into_runtime(self) -> Result<HostEffectDeclaration, ProgramError> {
        let kind = match self.kind.as_str() {
            "read" => HostEffectKind::Read,
            "update" => HostEffectKind::Update,
            "create" => HostEffectKind::Create,
            "delete" => HostEffectKind::Delete,
            "external" => HostEffectKind::External,
            _ => {
                return Err(ProgramError::InvalidCapabilityCatalog(format!(
                    "unknown host effect kind: {}",
                    self.kind
                )));
            }
        };
        Ok(HostEffectDeclaration {
            resource: self.resource,
            kind,
            summary: self.summary,
        })
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CapabilityBindingDoc {
    id: String,
    trigger: CapabilityTriggerDoc,
    capability: String,
    arguments: Vec<ArgumentBindingDoc>,
}
impl CapabilityBindingDoc {
    pub(super) fn into_runtime(self) -> Result<CapabilityBindingRule, ProgramError> {
        Ok(CapabilityBindingRule {
            id: CapabilityBindingId::new(self.id),
            trigger: self.trigger.into_runtime(),
            capability: CapabilityId::new(self.capability),
            arguments: self
                .arguments
                .into_iter()
                .map(ArgumentBindingDoc::into_runtime)
                .collect::<Result<_, _>>()?,
        })
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CapabilityTriggerDoc {
    meaning: Option<String>,
    behavior: Option<String>,
    response: Option<String>,
}
impl CapabilityTriggerDoc {
    pub(super) fn into_runtime(self) -> CapabilityTrigger {
        CapabilityTrigger {
            meaning: self.meaning.map(MeaningId::new),
            behavior: self.behavior.map(BehaviorId::new),
            response: self.response.map(ResponseId::new),
        }
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ArgumentBindingDoc {
    target: String,
    source: BindingSourceDoc,
}
impl ArgumentBindingDoc {
    pub(super) fn into_runtime(self) -> Result<ArgumentBinding, ProgramError> {
        let target = ArgumentPath::from_dotted(&self.target)
            .ok_or_else(|| ProgramError::InvalidArgumentPath(self.target.clone()))?;
        Ok(ArgumentBinding {
            target,
            source: self.source.into_runtime()?,
        })
    }
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum BindingSourceDoc {
    MeaningSlot { name: String },
    MeaningReference { kind: String, projection: String },
    FocusReference { kind: String, projection: String },
    ContextPath { path: String },
    AuthorStatePath { path: String },
    Literal { value: JsonValue },
}
impl BindingSourceDoc {
    pub(super) fn into_runtime(self) -> Result<BindingSource, ProgramError> {
        Ok(match self {
            Self::MeaningSlot { name } => BindingSource::MeaningSlot(name),
            Self::MeaningReference { kind, projection } => BindingSource::MeaningReference {
                kind: ReferenceKind::new(kind),
                projection: parse_projection(&projection)?,
            },
            Self::FocusReference { kind, projection } => BindingSource::FocusReference {
                kind: ReferenceKind::new(kind),
                projection: parse_projection(&projection)?,
            },
            Self::ContextPath { path } => BindingSource::ContextPath(path),
            Self::AuthorStatePath { path } => BindingSource::AuthorStatePath(path),
            Self::Literal { value } => BindingSource::Literal(model_value(&value)?),
        })
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CapabilityPolicyDoc {
    id: String,
    capability: String,
    priority: i32,
    conditions: Vec<AdmissionPredicateDoc>,
    effect: PolicyEffectDoc,
}
impl CapabilityPolicyDoc {
    pub(super) fn into_runtime(self) -> Result<CapabilityPolicyRule, ProgramError> {
        Ok(CapabilityPolicyRule {
            id: PolicyId::new(self.id),
            capability: CapabilityId::new(self.capability),
            priority: self.priority,
            conditions: self
                .conditions
                .into_iter()
                .map(AdmissionPredicateDoc::into_runtime)
                .collect::<Result<_, _>>()?,
            effect: self.effect.into_runtime()?,
        })
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct AdmissionPredicateDoc {
    namespace: String,
    path: String,
    op: String,
    value: Option<JsonValue>,
}
impl AdmissionPredicateDoc {
    pub(super) fn into_runtime(self) -> Result<AdmissionPredicate, ProgramError> {
        let namespace = match self.namespace.as_str() {
            "arguments" => AdmissionNamespace::Arguments,
            "context" => AdmissionNamespace::Context,
            "author" => AdmissionNamespace::Author,
            "conversation" => AdmissionNamespace::Conversation,
            "system" => AdmissionNamespace::System,
            _ => {
                return Err(ProgramError::InvalidCapabilityCatalog(format!(
                    "unknown admission namespace: {}",
                    self.namespace
                )));
            }
        };
        Ok(AdmissionPredicate {
            namespace,
            path: self.path,
            op: parse_admission_op(&self.op)?,
            value: self.value.as_ref().map(model_value).transpose()?,
        })
    }
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum PolicyEffectDoc {
    Allow,
    RequireConfirmation { reason_code: String },
    Deny { reason_code: String },
}
impl PolicyEffectDoc {
    pub(super) fn into_runtime(self) -> Result<PolicyEffect, ProgramError> {
        Ok(match self {
            Self::Allow => PolicyEffect::Allow,
            Self::RequireConfirmation { reason_code } => {
                PolicyEffect::RequireConfirmation { reason_code }
            }
            Self::Deny { reason_code } => PolicyEffect::Deny { reason_code },
        })
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CapabilityConfigDoc {
    schema_limits: SchemaLimitsDoc,
    max_proposals_per_turn: usize,
    max_bindings: usize,
    max_policy_rules: usize,
}
impl CapabilityConfigDoc {
    pub(super) fn into_runtime(self) -> CapabilityConfig {
        CapabilityConfig {
            schema_limits: self.schema_limits.into_runtime(),
            max_proposals_per_turn: self.max_proposals_per_turn,
            max_bindings: self.max_bindings,
            max_policy_rules: self.max_policy_rules,
        }
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct SchemaLimitsDoc {
    max_depth: usize,
    max_array_items: usize,
    max_object_properties: usize,
    max_string_bytes: usize,
    max_errors: usize,
}
impl SchemaLimitsDoc {
    pub(super) fn into_runtime(self) -> SchemaLimits {
        SchemaLimits {
            max_depth: self.max_depth,
            max_array_items: self.max_array_items,
            max_object_properties: self.max_object_properties,
            max_string_bytes: self.max_string_bytes,
            max_errors: self.max_errors,
        }
    }
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum ValueSchemaDoc {
    Null,
    Boolean,
    Number {
        minimum: Option<f64>,
        maximum: Option<f64>,
    },
    Integer {
        minimum: Option<i64>,
        maximum: Option<i64>,
    },
    String {
        min_chars: Option<usize>,
        max_chars: Option<usize>,
        allowed: BTreeSet<String>,
    },
    Array {
        items: Box<ValueSchemaDoc>,
        min_items: Option<usize>,
        max_items: Option<usize>,
    },
    Object {
        properties: BTreeMap<String, ValueSchemaDoc>,
        required: BTreeSet<String>,
        additional_properties: bool,
        min_properties: Option<usize>,
        max_properties: Option<usize>,
    },
    Enum {
        values: Vec<JsonValue>,
    },
    OneOf {
        variants: Vec<ValueSchemaDoc>,
    },
}
impl ValueSchemaDoc {
    pub(super) fn into_runtime(self) -> Result<ValueSchema, ProgramError> {
        Ok(match self {
            Self::Null => ValueSchema::Null,
            Self::Boolean => ValueSchema::Boolean,
            Self::Number { minimum, maximum } => {
                if minimum.is_some_and(|v| !v.is_finite())
                    || maximum.is_some_and(|v| !v.is_finite())
                {
                    return Err(ProgramError::InvalidValueSchema(
                        "non-finite number bound".into(),
                    ));
                }
                ValueSchema::Number { minimum, maximum }
            }
            Self::Integer { minimum, maximum } => ValueSchema::Integer { minimum, maximum },
            Self::String {
                min_chars,
                max_chars,
                allowed,
            } => ValueSchema::String {
                min_chars,
                max_chars,
                allowed,
            },
            Self::Array {
                items,
                min_items,
                max_items,
            } => ValueSchema::Array {
                items: Box::new(items.into_runtime()?),
                min_items,
                max_items,
            },
            Self::Object {
                properties,
                required,
                additional_properties,
                min_properties,
                max_properties,
            } => ValueSchema::Object(ObjectSchema {
                properties: properties
                    .into_iter()
                    .map(|(key, value)| Ok((key, value.into_runtime()?)))
                    .collect::<Result<_, ProgramError>>()?,
                required,
                additional_properties,
                min_properties,
                max_properties,
            }),
            Self::Enum { values } => {
                ValueSchema::Enum(values.iter().map(model_value).collect::<Result<_, _>>()?)
            }
            Self::OneOf { variants } => ValueSchema::OneOf(
                variants
                    .into_iter()
                    .map(ValueSchemaDoc::into_runtime)
                    .collect::<Result<_, _>>()?,
            ),
        })
    }
}
