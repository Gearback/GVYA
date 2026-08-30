//! Capability source decoding.

use super::super::*;

pub(in crate::source) fn parse_capability(
    value: &JsonValue,
    path: &str,
    limits: SourceLimits,
    issues: &mut Vec<SourceIssue>,
) -> Option<CapabilityDefinition> {
    let obj = expect_object(value, path, issues).ok()?;
    reject_unknown_keys(obj, crate::source::contract::CAPABILITY_KEYS, path, issues);
    let contract_obj = expect_object(
        required_value(obj, "contract", path, issues)?,
        &format!("{path}.contract"),
        issues,
    )
    .ok()?;
    reject_unknown_keys(
        contract_obj,
        crate::source::contract::CAPABILITY_CONTRACT_KEYS,
        &format!("{path}.contract"),
        issues,
    );
    let effect_class =
        match required_string(contract_obj, "effect_class", path, limits, issues)?.as_str() {
            "pure" => EffectClass::Pure,
            "reversible" => EffectClass::Reversible,
            "irreversible" => EffectClass::Irreversible,
            "external" => EffectClass::External,
            _ => {
                issues.push(issue(
                    "source.effect_class",
                    "unknown effect class",
                    Some(path),
                ));
                return None;
            }
        };
    let confirmation_hint =
        match required_string(contract_obj, "confirmation_hint", path, limits, issues)?.as_str() {
            "never" => ConfirmationHint::Never,
            "conditional" => ConfirmationHint::Conditional,
            "always" => ConfirmationHint::Always,
            _ => {
                issues.push(issue(
                    "source.confirmation_hint",
                    "unknown confirmation hint",
                    Some(path),
                ));
                return None;
            }
        };
    let reference_kinds = string_array(contract_obj.get("reference_kinds"), path, limits, issues)
        .into_iter()
        .map(ReferenceKind::new)
        .collect();
    let input_schema_value = required_value(
        contract_obj,
        "input_schema",
        &format!("{path}.contract"),
        issues,
    )?;
    if !input_schema_value.is_object() {
        issues.push(issue(
            "source.input_schema_json",
            "input_schema must be a JSON Schema object",
            Some(path),
        ));
        return None;
    }
    let input_schema = canonical_json(input_schema_value)
        .ok()
        .and_then(|bytes| String::from_utf8(bytes).ok())
        .unwrap_or_default();
    let input_shape = compile_schema_source(
        input_schema_value,
        &format!("{path}.contract.input_schema"),
        issues,
    )?;
    let output_schema_value = contract_obj
        .get("output_schema")
        .filter(|value| !value.is_null());
    let output_shape = output_schema_value.and_then(|value| {
        compile_schema_source(value, &format!("{path}.contract.output_schema"), issues)
    });
    let output_schema = output_schema_value
        .and_then(|value| canonical_json(value).ok())
        .and_then(|bytes| String::from_utf8(bytes).ok())
        .map(SchemaDocument::new);
    let contract = CapabilityContract {
        id: CapabilityId::new(required_string(contract_obj, "id", path, limits, issues)?),
        version: CapabilityVersion::new(required_string(
            contract_obj,
            "version",
            path,
            limits,
            issues,
        )?),
        title: required_string(contract_obj, "title", path, limits, issues)?,
        description: optional_string(contract_obj, "description", "", path, limits, issues),
        input_schema: SchemaDocument::new(input_schema),
        output_schema,
        reference_kinds,
        effect_class,
        confirmation_hint,
    };
    let host_effects = parse_host_effects(obj.get("host_effects"), path, limits, issues);
    Some(CapabilityDefinition {
        contract,
        input_shape,
        output_shape,
        host_effects,
    })
}

pub(in crate::source) fn parse_host_effects(
    value: Option<&JsonValue>,
    path: &str,
    limits: SourceLimits,
    issues: &mut Vec<SourceIssue>,
) -> Vec<HostEffectDeclaration> {
    let Some(array) = optional_array(value, &format!("{path}.host_effects"), issues) else {
        return Vec::new();
    };
    array
        .iter()
        .enumerate()
        .filter_map(|(index, row)| {
            let p = format!("{path}.host_effects[{index}]");
            let obj = expect_object(row, &p, issues).ok()?;
            reject_unknown_keys(obj, crate::source::contract::HOST_EFFECT_KEYS, &p, issues);
            let kind = match required_string(obj, "kind", &p, limits, issues)?.as_str() {
                "read" => HostEffectKind::Read,
                "update" => HostEffectKind::Update,
                "create" => HostEffectKind::Create,
                "delete" => HostEffectKind::Delete,
                "external" => HostEffectKind::External,
                _ => {
                    issues.push(issue(
                        "source.host_effect_kind",
                        "unknown host effect kind",
                        Some(&p),
                    ));
                    return None;
                }
            };
            Some(HostEffectDeclaration {
                resource: required_string(obj, "resource", &p, limits, issues)?,
                kind,
                summary: optional_string(obj, "summary", "", &p, limits, issues),
            })
        })
        .collect()
}

pub(in crate::source) fn parse_capability_binding(
    value: &JsonValue,
    path: &str,
    limits: SourceLimits,
    issues: &mut Vec<SourceIssue>,
) -> Option<CapabilityBindingRule> {
    let obj = expect_object(value, path, issues).ok()?;
    reject_unknown_keys(
        obj,
        crate::source::contract::CAPABILITY_BINDING_KEYS,
        path,
        issues,
    );
    let trigger_obj = expect_object(
        required_value(obj, "trigger", path, issues)?,
        &format!("{path}.trigger"),
        issues,
    )
    .ok()?;
    reject_unknown_keys(
        trigger_obj,
        crate::source::contract::CAPABILITY_TRIGGER_KEYS,
        &format!("{path}.trigger"),
        issues,
    );
    let trigger = CapabilityTrigger {
        meaning: optional_id(trigger_obj, "meaning", MeaningId::new, path, limits, issues),
        behavior: optional_id(
            trigger_obj,
            "behavior",
            BehaviorId::new,
            path,
            limits,
            issues,
        ),
        response: optional_id(
            trigger_obj,
            "response",
            ResponseId::new,
            path,
            limits,
            issues,
        ),
    };
    let arguments = parse_argument_bindings(obj.get("arguments"), path, limits, issues);
    Some(CapabilityBindingRule {
        id: CapabilityBindingId::new(required_string(obj, "id", path, limits, issues)?),
        trigger,
        capability: CapabilityId::new(required_string(obj, "capability", path, limits, issues)?),
        arguments,
    })
}
pub(in crate::source) fn parse_argument_bindings(
    value: Option<&JsonValue>,
    path: &str,
    limits: SourceLimits,
    issues: &mut Vec<SourceIssue>,
) -> Vec<ArgumentBinding> {
    let Some(array) = optional_array(value, &format!("{path}.arguments"), issues) else {
        return Vec::new();
    };
    array
        .iter()
        .enumerate()
        .filter_map(|(index, row)| {
            let p = format!("{path}.arguments[{index}]");
            let obj = expect_object(row, &p, issues).ok()?;
            reject_unknown_keys(
                obj,
                crate::source::contract::ARGUMENT_BINDING_KEYS,
                &p,
                issues,
            );
            let target =
                ArgumentPath::from_dotted(&required_string(obj, "target", &p, limits, issues)?)
                    .or_else(|| {
                        issues.push(issue(
                            "source.argument_target",
                            "invalid dotted argument target",
                            Some(&p),
                        ));
                        None
                    })?;
            let source = parse_binding_source(
                required_value(obj, "source", &p, issues)?,
                &format!("{p}.source"),
                limits,
                issues,
            )?;
            Some(ArgumentBinding { target, source })
        })
        .collect()
}
pub(in crate::source) fn parse_binding_source(
    value: &JsonValue,
    path: &str,
    limits: SourceLimits,
    issues: &mut Vec<SourceIssue>,
) -> Option<BindingSource> {
    let obj = expect_object(value, path, issues).ok()?;
    let kind = required_string(obj, "type", path, limits, issues)?;
    match kind.as_str() {
        "meaning_slot" => {
            reject_unknown_keys(
                obj,
                crate::source::contract::BINDING_SOURCE_MEANING_SLOT_KEYS,
                path,
                issues,
            );
            Some(BindingSource::MeaningSlot(required_string(
                obj, "name", path, limits, issues,
            )?))
        }
        "meaning_reference" => {
            reject_unknown_keys(
                obj,
                crate::source::contract::BINDING_SOURCE_REFERENCE_KEYS,
                path,
                issues,
            );
            Some(BindingSource::MeaningReference {
                kind: ReferenceKind::new(required_string(obj, "kind", path, limits, issues)?),
                projection: parse_projection(obj, path, limits, issues)?,
            })
        }
        "focus_reference" => {
            reject_unknown_keys(
                obj,
                crate::source::contract::BINDING_SOURCE_REFERENCE_KEYS,
                path,
                issues,
            );
            Some(BindingSource::FocusReference {
                kind: ReferenceKind::new(required_string(obj, "kind", path, limits, issues)?),
                projection: parse_projection(obj, path, limits, issues)?,
            })
        }
        "context_path" => {
            reject_unknown_keys(
                obj,
                crate::source::contract::BINDING_SOURCE_PATH_KEYS,
                path,
                issues,
            );
            Some(BindingSource::ContextPath(required_string(
                obj, "path", path, limits, issues,
            )?))
        }
        "author_state_path" => {
            reject_unknown_keys(
                obj,
                crate::source::contract::BINDING_SOURCE_PATH_KEYS,
                path,
                issues,
            );
            Some(BindingSource::AuthorStatePath(required_string(
                obj, "path", path, limits, issues,
            )?))
        }
        "literal" => {
            reject_unknown_keys(
                obj,
                crate::source::contract::BINDING_SOURCE_LITERAL_KEYS,
                path,
                issues,
            );
            Some(BindingSource::Literal(parse_model_value(
                required_value(obj, "value", path, issues)?,
                path,
                issues,
            )?))
        }
        _ => {
            issues.push(issue(
                "source.binding_source",
                "unknown binding source",
                Some(path),
            ));
            None
        }
    }
}
pub(in crate::source) fn parse_projection(
    obj: &serde_json::Map<String, JsonValue>,
    path: &str,
    limits: SourceLimits,
    issues: &mut Vec<SourceIssue>,
) -> Option<ReferenceProjection> {
    match optional_string(obj, "projection", "id", path, limits, issues).as_str() {
        "id" => Some(ReferenceProjection::Id),
        "object" => Some(ReferenceProjection::Object),
        _ => {
            issues.push(issue(
                "source.reference_projection",
                "unknown reference projection",
                Some(path),
            ));
            None
        }
    }
}

pub(in crate::source) fn parse_capability_policy(
    value: &JsonValue,
    path: &str,
    limits: SourceLimits,
    issues: &mut Vec<SourceIssue>,
) -> Option<CapabilityPolicyRule> {
    let obj = expect_object(value, path, issues).ok()?;
    reject_unknown_keys(
        obj,
        crate::source::contract::CAPABILITY_POLICY_KEYS,
        path,
        issues,
    );
    let conditions = parse_admission_predicates(obj.get("conditions"), path, limits, issues);
    let effect_obj = expect_object(
        required_value(obj, "effect", path, issues)?,
        &format!("{path}.effect"),
        issues,
    )
    .ok()?;
    let effect = match required_string(effect_obj, "type", path, limits, issues)?.as_str() {
        "allow" => {
            reject_unknown_keys(
                effect_obj,
                crate::source::contract::POLICY_EFFECT_ALLOW_KEYS,
                &format!("{path}.effect"),
                issues,
            );
            PolicyEffect::Allow
        }
        "require_confirmation" => {
            reject_unknown_keys(
                effect_obj,
                crate::source::contract::POLICY_EFFECT_REASON_KEYS,
                &format!("{path}.effect"),
                issues,
            );
            PolicyEffect::RequireConfirmation {
                reason_code: required_string(effect_obj, "reason_code", path, limits, issues)?,
            }
        }
        "deny" => {
            reject_unknown_keys(
                effect_obj,
                crate::source::contract::POLICY_EFFECT_REASON_KEYS,
                &format!("{path}.effect"),
                issues,
            );
            PolicyEffect::Deny {
                reason_code: required_string(effect_obj, "reason_code", path, limits, issues)?,
            }
        }
        _ => {
            issues.push(issue(
                "source.policy_effect",
                "unknown policy effect",
                Some(path),
            ));
            return None;
        }
    };
    Some(CapabilityPolicyRule {
        id: PolicyId::new(required_string(obj, "id", path, limits, issues)?),
        capability: CapabilityId::new(required_string(obj, "capability", path, limits, issues)?),
        priority: optional_i32(obj, "priority", 0, path, issues),
        conditions,
        effect,
    })
}
pub(in crate::source) fn parse_admission_predicates(
    value: Option<&JsonValue>,
    path: &str,
    limits: SourceLimits,
    issues: &mut Vec<SourceIssue>,
) -> Vec<AdmissionPredicate> {
    let Some(array) = optional_array(value, &format!("{path}.conditions"), issues) else {
        return Vec::new();
    };
    array
        .iter()
        .enumerate()
        .filter_map(|(index, row)| {
            let p = format!("{path}.conditions[{index}]");
            let obj = expect_object(row, &p, issues).ok()?;
            reject_unknown_keys(
                obj,
                crate::source::contract::ADMISSION_PREDICATE_KEYS,
                &p,
                issues,
            );
            let namespace = match required_string(obj, "namespace", &p, limits, issues)?.as_str() {
                "arguments" => AdmissionNamespace::Arguments,
                "context" => AdmissionNamespace::Context,
                "author" => AdmissionNamespace::Author,
                "conversation" => AdmissionNamespace::Conversation,
                "system" => AdmissionNamespace::System,
                _ => {
                    issues.push(issue(
                        "source.policy_namespace",
                        "unknown admission namespace",
                        Some(&p),
                    ));
                    return None;
                }
            };
            let op = parse_admission_op(
                required_string(obj, "op", &p, limits, issues)?.as_str(),
                &p,
                issues,
            )?;
            Some(AdmissionPredicate {
                namespace,
                path: required_string(obj, "path", &p, limits, issues)?,
                op,
                value: obj
                    .get("value")
                    .filter(|v| !v.is_null())
                    .and_then(|v| parse_model_value(v, &p, issues)),
            })
        })
        .collect()
}

pub(in crate::source) fn parse_capability_config(
    value: &JsonValue,
    path: &str,
    _limits: SourceLimits,
    issues: &mut Vec<SourceIssue>,
) -> Option<CapabilityConfig> {
    let obj = expect_object(value, path, issues).ok()?;
    reject_unknown_keys(
        obj,
        crate::source::contract::CAPABILITY_CONFIG_KEYS,
        path,
        issues,
    );
    let defaults = CapabilityConfig::default();
    let schema_obj = match obj.get("schema_limits") {
        None => None,
        Some(value) => match value.as_object() {
            Some(map) => {
                reject_unknown_keys(
                    map,
                    crate::source::contract::SCHEMA_LIMIT_KEYS,
                    &format!("{path}.schema_limits"),
                    issues,
                );
                Some(map)
            }
            None => {
                issues.push(issue(
                    "source.expected_object",
                    "schema_limits must be an object",
                    Some(path),
                ));
                None
            }
        },
    };
    let default_schema = defaults.schema_limits;
    let config = CapabilityConfig {
        schema_limits: SchemaLimits {
            max_depth: usize_field(
                schema_obj,
                "max_depth",
                default_schema.max_depth,
                path,
                issues,
            ),
            max_array_items: usize_field(
                schema_obj,
                "max_array_items",
                default_schema.max_array_items,
                path,
                issues,
            ),
            max_object_properties: usize_field(
                schema_obj,
                "max_object_properties",
                default_schema.max_object_properties,
                path,
                issues,
            ),
            max_string_bytes: usize_field(
                schema_obj,
                "max_string_bytes",
                default_schema.max_string_bytes,
                path,
                issues,
            ),
            max_errors: usize_field(
                schema_obj,
                "max_errors",
                default_schema.max_errors,
                path,
                issues,
            ),
        },
        max_proposals_per_turn: usize_field(
            Some(obj),
            "max_proposals_per_turn",
            defaults.max_proposals_per_turn,
            path,
            issues,
        ),
        max_bindings: usize_field(
            Some(obj),
            "max_bindings",
            defaults.max_bindings,
            path,
            issues,
        ),
        max_policy_rules: usize_field(
            Some(obj),
            "max_policy_rules",
            defaults.max_policy_rules,
            path,
            issues,
        ),
    };
    for config_issue in validate_capability_config(&config) {
        issues.push(issue(
            "source.capability_config_range",
            &config_issue.message,
            Some(path),
        ));
    }
    Some(config)
}
