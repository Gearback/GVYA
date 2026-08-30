//! Fail-closed compiler for the GVYA JSON Schema 2020-12 capability profile.
//!
//! Runtime validation uses the small `ValueSchema` IR. This compiler translates only semantics
//! the runtime can enforce exactly; unsupported assertion keywords are errors, never ignored.

use std::collections::{BTreeMap, BTreeSet};

use gvya_kernel::capability::{ObjectSchema, SchemaLimits, ValueSchema, validate_value};
use gvya_model::Value;
use serde_json::Value as JsonValue;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SchemaCompileIssue {
    pub path: String,
    pub code: String,
    pub message: String,
}

pub fn compile_json_schema(
    document: &JsonValue,
    limits: SchemaLimits,
) -> Result<ValueSchema, Vec<SchemaCompileIssue>> {
    let mut issues = Vec::new();
    let schema = compile_node(document, "$", 0, limits, &mut issues);
    if issues.is_empty() {
        schema.ok_or_else(|| vec![issue("$", "schema.empty", "schema did not compile")])
    } else {
        Err(issues)
    }
}

fn compile_node(
    value: &JsonValue,
    path: &str,
    depth: usize,
    limits: SchemaLimits,
    issues: &mut Vec<SchemaCompileIssue>,
) -> Option<ValueSchema> {
    if depth > limits.max_depth {
        issues.push(issue(
            path,
            "schema.depth",
            "schema exceeds configured depth",
        ));
        return None;
    }
    let Some(obj) = value.as_object() else {
        issues.push(issue(
            path,
            "schema.object_required",
            "GVYA v1 capability schemas must be JSON Schema objects",
        ));
        return None;
    };
    reject_unsupported_keywords(obj, path, issues);

    if let Some(one_of) = obj.get("oneOf") {
        let Some(rows) = one_of.as_array() else {
            issues.push(issue(path, "schema.one_of_array", "oneOf must be an array"));
            return None;
        };
        if rows.is_empty() || rows.len() > 32 {
            issues.push(issue(
                path,
                "schema.one_of_bound",
                "oneOf must contain 1..=32 branches",
            ));
            return None;
        }
        let variants = rows
            .iter()
            .enumerate()
            .filter_map(|(index, row)| {
                compile_node(
                    row,
                    &format!("{path}.oneOf[{index}]"),
                    depth + 1,
                    limits,
                    issues,
                )
            })
            .collect();
        return Some(ValueSchema::OneOf(variants));
    }

    let types = schema_types(obj.get("type"), path, issues)?;
    if types.len() > 1 {
        let mut variants = Vec::new();
        for type_name in types {
            let mut clone = obj.clone();
            clone.insert("type".into(), JsonValue::String(type_name));
            if let Some(schema) =
                compile_node(&JsonValue::Object(clone), path, depth + 1, limits, issues)
            {
                variants.push(schema);
            }
        }
        return Some(ValueSchema::OneOf(variants));
    }
    let type_name = types.into_iter().next()?;
    if obj.contains_key("enum") && matches!(type_name.as_str(), "object" | "array") {
        issues.push(issue(path, "schema.enum_combination_unsupported", "GVYA v1 does not silently weaken object/array schemas combined with enum; express alternatives with oneOf"));
        return None;
    }
    match type_name.as_str() {
        "null" => Some(ValueSchema::Null),
        "boolean" => enum_or(ValueSchema::Boolean, obj, path, limits, issues),
        "number" => {
            let minimum = finite_number(obj.get("minimum"), &format!("{path}.minimum"), issues);
            let maximum = finite_number(obj.get("maximum"), &format!("{path}.maximum"), issues);
            enum_or(
                ValueSchema::Number { minimum, maximum },
                obj,
                path,
                limits,
                issues,
            )
        }
        "integer" => {
            let minimum = integer(obj.get("minimum"), &format!("{path}.minimum"), issues);
            let maximum = integer(obj.get("maximum"), &format!("{path}.maximum"), issues);
            enum_or(
                ValueSchema::Integer { minimum, maximum },
                obj,
                path,
                limits,
                issues,
            )
        }
        "string" => {
            let min_chars =
                usize_keyword(obj.get("minLength"), &format!("{path}.minLength"), issues);
            let max_chars =
                usize_keyword(obj.get("maxLength"), &format!("{path}.maxLength"), issues);
            if max_chars.is_some_and(|max| max > limits.max_string_bytes) {
                issues.push(issue(
                    path,
                    "schema.string_runtime_bound",
                    "maxLength exceeds the runtime's absolute string-byte safety ceiling",
                ));
            }
            let allowed = string_enum(obj.get("enum"), path, limits, issues);
            Some(ValueSchema::String {
                min_chars,
                max_chars,
                allowed,
            })
        }
        "array" => {
            let Some(items) = obj.get("items") else {
                issues.push(issue(
                    path,
                    "schema.items_required",
                    "array schema requires a single items schema",
                ));
                return None;
            };
            let items = compile_node(items, &format!("{path}.items"), depth + 1, limits, issues)?;
            let min_items = usize_keyword(obj.get("minItems"), &format!("{path}.minItems"), issues);
            let max_items = usize_keyword(obj.get("maxItems"), &format!("{path}.maxItems"), issues);
            if max_items.is_some_and(|max| max > limits.max_array_items) {
                issues.push(issue(
                    path,
                    "schema.array_runtime_bound",
                    "maxItems exceeds runtime array safety limit",
                ));
            }
            Some(ValueSchema::Array {
                items: Box::new(items),
                min_items,
                max_items,
            })
        }
        "object" => {
            let mut properties = BTreeMap::new();
            if let Some(value) = obj.get("properties") {
                let Some(map) = value.as_object() else {
                    issues.push(issue(
                        path,
                        "schema.properties_object",
                        "properties must be an object",
                    ));
                    return None;
                };
                if map.len() > limits.max_object_properties {
                    issues.push(issue(
                        path,
                        "schema.property_limit",
                        "properties exceed runtime object-property limit",
                    ));
                }
                for (name, child) in map {
                    if name.is_empty() || name.len() > 256 || name.chars().any(char::is_control) {
                        issues.push(issue(
                            path,
                            "schema.property_name",
                            "property name is empty, too long, or contains control characters",
                        ));
                        continue;
                    }
                    if let Some(compiled) = compile_node(
                        child,
                        &format!("{path}.properties.{name}"),
                        depth + 1,
                        limits,
                        issues,
                    ) {
                        properties.insert(name.clone(), compiled);
                    }
                }
            }
            let required = required_set(obj.get("required"), path, issues);
            for name in &required {
                if !properties.contains_key(name) {
                    issues.push(issue(
                        path,
                        "schema.required_unknown",
                        "required names must also appear in properties",
                    ));
                }
            }
            let additional_properties = match obj.get("additionalProperties") {
                Some(JsonValue::Bool(value)) => *value,
                None => {
                    issues.push(issue(
                        path,
                        "schema.additional_properties_explicit",
                        "object schemas must explicitly declare additionalProperties in GVYA v1",
                    ));
                    false
                }
                Some(_) => {
                    issues.push(issue(
                        path,
                        "schema.additional_properties",
                        "additionalProperties must be boolean in GVYA v1",
                    ));
                    false
                }
            };
            let min_properties = usize_keyword(
                obj.get("minProperties"),
                &format!("{path}.minProperties"),
                issues,
            );
            let max_properties = usize_keyword(
                obj.get("maxProperties"),
                &format!("{path}.maxProperties"),
                issues,
            );
            if max_properties.is_some_and(|max| max > limits.max_object_properties) {
                issues.push(issue(
                    path,
                    "schema.object_runtime_bound",
                    "maxProperties exceeds runtime object-property limit",
                ));
            }
            Some(ValueSchema::Object(ObjectSchema {
                properties,
                required,
                additional_properties,
                min_properties,
                max_properties,
            }))
        }
        _ => {
            issues.push(issue(path, "schema.type", "unsupported JSON Schema type"));
            None
        }
    }
}

fn schema_types(
    value: Option<&JsonValue>,
    path: &str,
    issues: &mut Vec<SchemaCompileIssue>,
) -> Option<Vec<String>> {
    match value {
        Some(JsonValue::String(value)) => Some(vec![value.clone()]),
        Some(JsonValue::Array(values)) => {
            let mut out = Vec::new();
            for value in values {
                let Some(value) = value.as_str() else {
                    issues.push(issue(
                        path,
                        "schema.type_array",
                        "type array must contain strings",
                    ));
                    return None;
                };
                if !out.iter().any(|row| row == value) {
                    out.push(value.to_owned());
                }
            }
            if out.is_empty() {
                issues.push(issue(
                    path,
                    "schema.type_empty",
                    "type array cannot be empty",
                ));
                None
            } else {
                Some(out)
            }
        }
        _ => {
            issues.push(issue(
                path,
                "schema.type_required",
                "GVYA v1 schema requires explicit type (or oneOf branches with type)",
            ));
            None
        }
    }
}

fn enum_or(
    base: ValueSchema,
    obj: &serde_json::Map<String, JsonValue>,
    path: &str,
    limits: SchemaLimits,
    issues: &mut Vec<SchemaCompileIssue>,
) -> Option<ValueSchema> {
    let Some(value) = obj.get("enum") else {
        return Some(base);
    };
    let Some(rows) = value.as_array() else {
        issues.push(issue(path, "schema.enum_array", "enum must be an array"));
        return None;
    };
    if rows.is_empty() || rows.len() > 256 {
        issues.push(issue(
            path,
            "schema.enum_bound",
            "enum must contain 1..=256 values",
        ));
        return None;
    }
    let values: Vec<Value> = rows
        .iter()
        .filter_map(|row| json_model_value(row, path, issues))
        .collect();
    for value in &values {
        if !validate_value(value, &base, limits).is_empty() {
            issues.push(issue(
                path,
                "schema.enum_conflicts_with_type_constraints",
                "every enum value must satisfy the schema's type and bounds",
            ));
            return None;
        }
    }
    Some(ValueSchema::Enum(values))
}

fn string_enum(
    value: Option<&JsonValue>,
    path: &str,
    limits: SchemaLimits,
    issues: &mut Vec<SchemaCompileIssue>,
) -> BTreeSet<String> {
    let Some(value) = value else {
        return BTreeSet::new();
    };
    let Some(rows) = value.as_array() else {
        issues.push(issue(path, "schema.enum_array", "enum must be an array"));
        return BTreeSet::new();
    };
    if rows.is_empty() || rows.len() > 256 {
        issues.push(issue(
            path,
            "schema.enum_bound",
            "enum must contain 1..=256 values",
        ));
    }
    rows.iter()
        .filter_map(|row| match row.as_str() {
            Some(value) if value.len() <= limits.max_string_bytes => Some(value.to_owned()),
            Some(_) => {
                issues.push(issue(
                    path,
                    "schema.enum_string_bound",
                    "enum string exceeds runtime byte limit",
                ));
                None
            }
            None => {
                issues.push(issue(
                    path,
                    "schema.enum_string",
                    "string schema enum may contain only strings",
                ));
                None
            }
        })
        .collect()
}

fn required_set(
    value: Option<&JsonValue>,
    path: &str,
    issues: &mut Vec<SchemaCompileIssue>,
) -> BTreeSet<String> {
    let Some(value) = value else {
        return BTreeSet::new();
    };
    let Some(rows) = value.as_array() else {
        issues.push(issue(
            path,
            "schema.required_array",
            "required must be an array",
        ));
        return BTreeSet::new();
    };
    rows.iter()
        .filter_map(|row| {
            row.as_str().map(str::to_owned).or_else(|| {
                issues.push(issue(
                    path,
                    "schema.required_string",
                    "required values must be strings",
                ));
                None
            })
        })
        .collect()
}

fn finite_number(
    value: Option<&JsonValue>,
    path: &str,
    issues: &mut Vec<SchemaCompileIssue>,
) -> Option<f64> {
    value.and_then(|value| {
        value
            .as_f64()
            .filter(|number| number.is_finite())
            .or_else(|| {
                issues.push(issue(
                    path,
                    "schema.number",
                    "keyword must be a finite number",
                ));
                None
            })
    })
}
fn integer(
    value: Option<&JsonValue>,
    path: &str,
    issues: &mut Vec<SchemaCompileIssue>,
) -> Option<i64> {
    value.and_then(|value| {
        value.as_i64().or_else(|| {
            issues.push(issue(
                path,
                "schema.integer",
                "keyword must be an i64 integer",
            ));
            None
        })
    })
}
fn usize_keyword(
    value: Option<&JsonValue>,
    path: &str,
    issues: &mut Vec<SchemaCompileIssue>,
) -> Option<usize> {
    value.and_then(|value| {
        value
            .as_u64()
            .and_then(|value| usize::try_from(value).ok())
            .or_else(|| {
                issues.push(issue(
                    path,
                    "schema.unsigned",
                    "keyword must be a non-negative integer in range",
                ));
                None
            })
    })
}

fn json_model_value(
    value: &JsonValue,
    path: &str,
    issues: &mut Vec<SchemaCompileIssue>,
) -> Option<Value> {
    match value {
        JsonValue::Null => Some(Value::Null),
        JsonValue::Bool(value) => Some(Value::Bool(*value)),
        JsonValue::Number(value) => value
            .as_f64()
            .filter(|number| number.is_finite())
            .map(Value::Number)
            .or_else(|| {
                issues.push(issue(
                    path,
                    "schema.enum_number",
                    "enum number must be finite",
                ));
                None
            }),
        JsonValue::String(value) => Some(Value::String(value.clone())),
        JsonValue::Array(values) => Some(Value::Array(
            values
                .iter()
                .filter_map(|row| json_model_value(row, path, issues))
                .collect(),
        )),
        JsonValue::Object(values) => Some(Value::Object(
            values
                .iter()
                .filter_map(|(key, row)| {
                    json_model_value(row, path, issues).map(|row| (key.clone(), row))
                })
                .collect(),
        )),
    }
}

fn reject_unsupported_keywords(
    obj: &serde_json::Map<String, JsonValue>,
    path: &str,
    issues: &mut Vec<SchemaCompileIssue>,
) {
    const SUPPORTED: &[&str] = &[
        "$schema",
        "$id",
        "title",
        "description",
        "type",
        "enum",
        "oneOf",
        "minimum",
        "maximum",
        "minLength",
        "maxLength",
        "items",
        "minItems",
        "maxItems",
        "properties",
        "required",
        "additionalProperties",
        "minProperties",
        "maxProperties",
    ];
    for key in obj.keys() {
        if !SUPPORTED.contains(&key.as_str()) {
            issues.push(issue(
                &format!("{path}.{key}"),
                "schema.keyword_unsupported",
                "assertion/keyword is not in the GVYA v1 executable schema profile",
            ));
        }
    }
}

fn issue(path: &str, code: &str, message: &str) -> SchemaCompileIssue {
    SchemaCompileIssue {
        path: path.to_owned(),
        code: code.to_owned(),
        message: message.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compiles_typed_object_and_unicode_length_semantics() {
        let source = serde_json::json!({"type":"object","properties":{"name":{"type":"string","minLength":1,"maxLength":20}},"required":["name"],"additionalProperties":false});
        let schema = compile_json_schema(&source, SchemaLimits::default()).unwrap();
        let ValueSchema::Object(object) = schema else {
            panic!("object expected")
        };
        assert!(matches!(
            object.properties["name"],
            ValueSchema::String {
                min_chars: Some(1),
                max_chars: Some(20),
                ..
            }
        ));
    }

    #[test]
    fn unsupported_assertion_is_not_silently_ignored() {
        let source = serde_json::json!({"type":"string","pattern":"^x$"});
        assert!(compile_json_schema(&source, SchemaLimits::default()).is_err());
    }
}
