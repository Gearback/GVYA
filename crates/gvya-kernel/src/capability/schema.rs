//! Bounded compiled structural schema validation for capability values.
//!
//! Source contracts use a constrained JSON Schema 2020-12 profile. capability kernel executes a compiled,
//! dependency-free schema IR; parsing/canonicalizing source JSON Schema belongs to the compiler.

use std::collections::{BTreeMap, BTreeSet};

use gvya_model::Value;

#[derive(Clone, Debug, PartialEq)]
pub enum ValueSchema {
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
        items: Box<ValueSchema>,
        min_items: Option<usize>,
        max_items: Option<usize>,
    },
    Object(ObjectSchema),
    Enum(Vec<Value>),
    OneOf(Vec<ValueSchema>),
}

impl ValueSchema {
    #[must_use]
    pub fn object(properties: BTreeMap<String, ValueSchema>, required: BTreeSet<String>) -> Self {
        Self::Object(ObjectSchema {
            properties,
            required,
            additional_properties: false,
            min_properties: None,
            max_properties: None,
        })
    }

    #[must_use]
    pub fn is_object(&self) -> bool {
        matches!(self, Self::Object(_))
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ObjectSchema {
    pub properties: BTreeMap<String, ValueSchema>,
    pub required: BTreeSet<String>,
    pub additional_properties: bool,
    pub min_properties: Option<usize>,
    pub max_properties: Option<usize>,
}

impl Default for ObjectSchema {
    fn default() -> Self {
        Self {
            properties: BTreeMap::new(),
            required: BTreeSet::new(),
            additional_properties: false,
            min_properties: None,
            max_properties: None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SchemaLimits {
    pub max_depth: usize,
    pub max_array_items: usize,
    pub max_object_properties: usize,
    pub max_string_bytes: usize,
    pub max_errors: usize,
}

impl Default for SchemaLimits {
    fn default() -> Self {
        Self {
            max_depth: 16,
            max_array_items: 256,
            max_object_properties: 128,
            max_string_bytes: 8 * 1024,
            max_errors: 16,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SchemaIssue {
    pub path: String,
    pub code: String,
    pub message: String,
}

#[must_use]
pub fn validate_schema_definition(schema: &ValueSchema, limits: SchemaLimits) -> Vec<SchemaIssue> {
    if limits.max_errors == 0 {
        return vec![SchemaIssue {
            path: "$schema".into(),
            code: "schema_error_limit_invalid".into(),
            message: "max_errors must be positive".into(),
        }];
    }
    let mut issues = Vec::new();
    validate_schema_inner(schema, "$schema", 0, limits, &mut issues);
    issues
}

fn validate_schema_inner(
    schema: &ValueSchema,
    path: &str,
    depth: usize,
    limits: SchemaLimits,
    issues: &mut Vec<SchemaIssue>,
) {
    if issues.len() >= limits.max_errors {
        return;
    }
    if depth > limits.max_depth {
        push_issue(
            issues,
            limits,
            path,
            "schema_definition_depth_exceeded",
            "compiled schema exceeds configured depth",
        );
        return;
    }
    match schema {
        ValueSchema::Number { minimum, maximum } => {
            if minimum.is_some_and(|value| !value.is_finite())
                || maximum.is_some_and(|value| !value.is_finite())
            {
                push_issue(
                    issues,
                    limits,
                    path,
                    "schema_non_finite_bound",
                    "number bounds must be finite",
                );
            }
            if (*minimum).zip(*maximum).is_some_and(|(min, max)| min > max) {
                push_issue(
                    issues,
                    limits,
                    path,
                    "schema_invalid_bounds",
                    "minimum exceeds maximum",
                );
            }
        }
        ValueSchema::Integer { minimum, maximum } => {
            if (*minimum).zip(*maximum).is_some_and(|(min, max)| min > max) {
                push_issue(
                    issues,
                    limits,
                    path,
                    "schema_invalid_bounds",
                    "minimum exceeds maximum",
                );
            }
        }
        ValueSchema::String {
            min_chars,
            max_chars,
            allowed,
        } => {
            if (*min_chars)
                .zip(*max_chars)
                .is_some_and(|(min, max)| min > max)
            {
                push_issue(
                    issues,
                    limits,
                    path,
                    "schema_invalid_string_bounds",
                    "minLength exceeds maxLength",
                );
            }
            if allowed
                .iter()
                .any(|value| value.len() > limits.max_string_bytes)
            {
                push_issue(
                    issues,
                    limits,
                    path,
                    "schema_enum_string_limit_exceeded",
                    "enum string exceeds runtime limit",
                );
            }
        }
        ValueSchema::Array {
            items,
            min_items,
            max_items,
        } => {
            if (*min_items)
                .zip(*max_items)
                .is_some_and(|(min, max)| min > max)
            {
                push_issue(
                    issues,
                    limits,
                    path,
                    "schema_invalid_array_bounds",
                    "minItems exceeds maxItems",
                );
            }
            if max_items.is_some_and(|max| max > limits.max_array_items) {
                push_issue(
                    issues,
                    limits,
                    path,
                    "schema_array_limit_exceeded",
                    "schema maxItems exceeds runtime limit",
                );
            }
            validate_schema_inner(items, &format!("{path}.items"), depth + 1, limits, issues);
        }
        ValueSchema::Object(object) => {
            if object.properties.len() > limits.max_object_properties {
                push_issue(
                    issues,
                    limits,
                    path,
                    "schema_object_limit_exceeded",
                    "schema declares too many properties",
                );
            }
            if object
                .min_properties
                .zip(object.max_properties)
                .is_some_and(|(min, max)| min > max)
            {
                push_issue(
                    issues,
                    limits,
                    path,
                    "schema_invalid_object_bounds",
                    "minProperties exceeds maxProperties",
                );
            }
            if object
                .max_properties
                .is_some_and(|max| max > limits.max_object_properties)
            {
                push_issue(
                    issues,
                    limits,
                    path,
                    "schema_object_limit_exceeded",
                    "schema maxProperties exceeds runtime limit",
                );
            }
            for required in &object.required {
                if !object.properties.contains_key(required) && !object.additional_properties {
                    push_issue(
                        issues,
                        limits,
                        &format!("{path}.required"),
                        "schema_required_property_undeclared",
                        "required property is not declared",
                    );
                }
            }
            for (name, child) in &object.properties {
                validate_schema_inner(
                    child,
                    &format!("{path}.properties.{name}"),
                    depth + 1,
                    limits,
                    issues,
                );
                if issues.len() >= limits.max_errors {
                    break;
                }
            }
        }
        ValueSchema::OneOf(branches) => {
            if branches.is_empty() {
                push_issue(
                    issues,
                    limits,
                    path,
                    "schema_one_of_empty",
                    "oneOf must contain at least one branch",
                );
            }
            for (index, branch) in branches.iter().enumerate() {
                validate_schema_inner(
                    branch,
                    &format!("{path}.oneOf[{index}]"),
                    depth + 1,
                    limits,
                    issues,
                );
                if issues.len() >= limits.max_errors {
                    break;
                }
            }
        }
        ValueSchema::Enum(values) => {
            if values.is_empty() {
                push_issue(
                    issues,
                    limits,
                    path,
                    "schema_enum_empty",
                    "enum must contain at least one value",
                );
            }
        }
        ValueSchema::Null | ValueSchema::Boolean => {}
    }
}

#[must_use]
pub fn validate_value(
    value: &Value,
    schema: &ValueSchema,
    limits: SchemaLimits,
) -> Vec<SchemaIssue> {
    if limits.max_errors == 0 {
        return vec![SchemaIssue {
            path: "$".into(),
            code: "schema_error_limit_invalid".into(),
            message: "max_errors must be positive".into(),
        }];
    }
    let mut issues = Vec::new();
    validate_inner(value, schema, "$", 0, limits, &mut issues);
    issues
}

fn validate_inner(
    value: &Value,
    schema: &ValueSchema,
    path: &str,
    depth: usize,
    limits: SchemaLimits,
    issues: &mut Vec<SchemaIssue>,
) {
    if issues.len() >= limits.max_errors {
        return;
    }
    if depth > limits.max_depth {
        push_issue(
            issues,
            limits,
            path,
            "schema_depth_exceeded",
            "value exceeds configured schema depth",
        );
        return;
    }

    match schema {
        ValueSchema::Null => {
            if !matches!(value, Value::Null) {
                type_issue(issues, limits, path, "null");
            }
        }
        ValueSchema::Boolean => {
            if !matches!(value, Value::Bool(_)) {
                type_issue(issues, limits, path, "boolean");
            }
        }
        ValueSchema::Number { minimum, maximum } => match value {
            Value::Number(number) if number.is_finite() => {
                if minimum.is_some_and(|min| *number < min) {
                    push_issue(
                        issues,
                        limits,
                        path,
                        "number_below_minimum",
                        "number is below minimum",
                    );
                }
                if maximum.is_some_and(|max| *number > max) {
                    push_issue(
                        issues,
                        limits,
                        path,
                        "number_above_maximum",
                        "number is above maximum",
                    );
                }
            }
            Value::Number(_) => push_issue(
                issues,
                limits,
                path,
                "number_not_finite",
                "number must be finite",
            ),
            _ => type_issue(issues, limits, path, "number"),
        },
        ValueSchema::Integer { minimum, maximum } => match value {
            Value::Number(number) if number.is_finite() && number.fract() == 0.0 => {
                if *number < i64::MIN as f64 || *number > i64::MAX as f64 {
                    push_issue(
                        issues,
                        limits,
                        path,
                        "integer_out_of_range",
                        "integer is outside i64 range",
                    );
                    return;
                }
                let integer = *number as i64;
                if minimum.is_some_and(|min| integer < min) {
                    push_issue(
                        issues,
                        limits,
                        path,
                        "integer_below_minimum",
                        "integer is below minimum",
                    );
                }
                if maximum.is_some_and(|max| integer > max) {
                    push_issue(
                        issues,
                        limits,
                        path,
                        "integer_above_maximum",
                        "integer is above maximum",
                    );
                }
            }
            Value::Number(_) => push_issue(
                issues,
                limits,
                path,
                "integer_expected",
                "number must be a finite integer",
            ),
            _ => type_issue(issues, limits, path, "integer"),
        },
        ValueSchema::String {
            min_chars,
            max_chars,
            allowed,
        } => match value {
            Value::String(text) => {
                let byte_len = text.len();
                let char_len = text.chars().count();
                if byte_len > limits.max_string_bytes {
                    push_issue(
                        issues,
                        limits,
                        path,
                        "string_limit_exceeded",
                        "string exceeds runtime byte limit",
                    );
                }
                if min_chars.is_some_and(|min| char_len < min) {
                    push_issue(
                        issues,
                        limits,
                        path,
                        "string_too_short",
                        "string is shorter than minimum",
                    );
                }
                if max_chars.is_some_and(|max| char_len > max) {
                    push_issue(
                        issues,
                        limits,
                        path,
                        "string_too_long",
                        "string is longer than maximum",
                    );
                }
                if !allowed.is_empty() && !allowed.contains(text) {
                    push_issue(
                        issues,
                        limits,
                        path,
                        "string_not_allowed",
                        "string is not in the declared enum",
                    );
                }
            }
            _ => type_issue(issues, limits, path, "string"),
        },
        ValueSchema::Array {
            items,
            min_items,
            max_items,
        } => match value {
            Value::Array(values) => {
                if values.len() > limits.max_array_items {
                    push_issue(
                        issues,
                        limits,
                        path,
                        "array_limit_exceeded",
                        "array exceeds runtime item limit",
                    );
                    return;
                }
                if min_items.is_some_and(|min| values.len() < min) {
                    push_issue(
                        issues,
                        limits,
                        path,
                        "array_too_short",
                        "array has fewer than minItems",
                    );
                }
                if max_items.is_some_and(|max| values.len() > max) {
                    push_issue(
                        issues,
                        limits,
                        path,
                        "array_too_long",
                        "array has more than maxItems",
                    );
                }
                for (index, item) in values.iter().enumerate() {
                    validate_inner(
                        item,
                        items,
                        &format!("{path}[{index}]"),
                        depth + 1,
                        limits,
                        issues,
                    );
                    if issues.len() >= limits.max_errors {
                        break;
                    }
                }
            }
            _ => type_issue(issues, limits, path, "array"),
        },
        ValueSchema::Object(object) => match value {
            Value::Object(values) => {
                if values.len() > limits.max_object_properties {
                    push_issue(
                        issues,
                        limits,
                        path,
                        "object_limit_exceeded",
                        "object exceeds runtime property limit",
                    );
                    return;
                }
                if object.min_properties.is_some_and(|min| values.len() < min) {
                    push_issue(
                        issues,
                        limits,
                        path,
                        "object_too_small",
                        "object has fewer than minProperties",
                    );
                }
                if object.max_properties.is_some_and(|max| values.len() > max) {
                    push_issue(
                        issues,
                        limits,
                        path,
                        "object_too_large",
                        "object has more than maxProperties",
                    );
                }
                for required in &object.required {
                    if !values.contains_key(required) {
                        push_issue(
                            issues,
                            limits,
                            &property_path(path, required),
                            "required_property_missing",
                            "required property is missing",
                        );
                    }
                }
                for (name, item) in values {
                    if let Some(property_schema) = object.properties.get(name) {
                        validate_inner(
                            item,
                            property_schema,
                            &property_path(path, name),
                            depth + 1,
                            limits,
                            issues,
                        );
                    } else if !object.additional_properties {
                        push_issue(
                            issues,
                            limits,
                            &property_path(path, name),
                            "additional_property_forbidden",
                            "property is not declared by the capability contract",
                        );
                    }
                    if issues.len() >= limits.max_errors {
                        break;
                    }
                }
            }
            _ => type_issue(issues, limits, path, "object"),
        },
        ValueSchema::Enum(allowed) => {
            if !allowed
                .iter()
                .any(|candidate| value_equal(candidate, value))
            {
                push_issue(
                    issues,
                    limits,
                    path,
                    "enum_mismatch",
                    "value is not one of the declared enum values",
                );
            }
        }
        ValueSchema::OneOf(branches) => {
            if branches.is_empty() {
                push_issue(
                    issues,
                    limits,
                    path,
                    "one_of_empty",
                    "compiled oneOf has no branches",
                );
                return;
            }
            let mut matches = 0usize;
            for branch in branches {
                let mut branch_issues = Vec::new();
                validate_inner(value, branch, path, depth + 1, limits, &mut branch_issues);
                if branch_issues.is_empty() {
                    matches += 1;
                }
            }
            if matches != 1 {
                push_issue(
                    issues,
                    limits,
                    path,
                    "one_of_mismatch",
                    "value must match exactly one oneOf branch",
                );
            }
        }
    }
}

fn property_path(parent: &str, property: &str) -> String {
    if property
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
    {
        format!("{parent}.{property}")
    } else {
        format!("{parent}[{property:?}]")
    }
}

fn type_issue(issues: &mut Vec<SchemaIssue>, limits: SchemaLimits, path: &str, expected: &str) {
    push_issue(
        issues,
        limits,
        path,
        "type_mismatch",
        &format!("expected {expected}"),
    );
}

fn push_issue(
    issues: &mut Vec<SchemaIssue>,
    limits: SchemaLimits,
    path: &str,
    code: &str,
    message: &str,
) {
    if issues.len() >= limits.max_errors {
        return;
    }
    issues.push(SchemaIssue {
        path: path.to_owned(),
        code: code.to_owned(),
        message: message.to_owned(),
    });
}

fn value_equal(left: &Value, right: &Value) -> bool {
    match (left, right) {
        (Value::Null, Value::Null) => true,
        (Value::Bool(a), Value::Bool(b)) => a == b,
        (Value::Number(a), Value::Number(b)) => a.is_finite() && b.is_finite() && a == b,
        (Value::String(a), Value::String(b)) => a == b,
        (Value::Array(a), Value::Array(b)) => {
            a.len() == b.len() && a.iter().zip(b).all(|(x, y)| value_equal(x, y))
        }
        (Value::Object(a), Value::Object(b)) => {
            a.len() == b.len()
                && a.iter()
                    .all(|(key, value)| b.get(key).is_some_and(|other| value_equal(value, other)))
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn object_validation_rejects_missing_and_extra_properties() {
        let mut properties = BTreeMap::new();
        properties.insert(
            "temperature".into(),
            ValueSchema::Integer {
                minimum: Some(16),
                maximum: Some(30),
            },
        );
        let schema = ValueSchema::object(properties, BTreeSet::from(["temperature".into()]));
        let value = Value::Object(BTreeMap::from([(
            "room".into(),
            Value::String("kitchen".into()),
        )]));
        let issues = validate_value(&value, &schema, SchemaLimits::default());
        assert!(
            issues
                .iter()
                .any(|issue| issue.code == "required_property_missing")
        );
        assert!(
            issues
                .iter()
                .any(|issue| issue.code == "additional_property_forbidden")
        );
    }

    #[test]
    fn integer_bounds_are_strict() {
        let schema = ValueSchema::Integer {
            minimum: Some(0),
            maximum: Some(10),
        };
        assert!(validate_value(&Value::Number(10.0), &schema, SchemaLimits::default()).is_empty());
        assert!(!validate_value(&Value::Number(10.5), &schema, SchemaLimits::default()).is_empty());
        assert!(!validate_value(&Value::Number(11.0), &schema, SchemaLimits::default()).is_empty());
    }

    #[test]
    fn one_of_requires_exactly_one_branch() {
        let schema = ValueSchema::OneOf(vec![
            ValueSchema::Boolean,
            ValueSchema::String {
                min_chars: None,
                max_chars: None,
                allowed: BTreeSet::new(),
            },
        ]);
        assert!(validate_value(&Value::Bool(true), &schema, SchemaLimits::default()).is_empty());
        assert!(!validate_value(&Value::Number(1.0), &schema, SchemaLimits::default()).is_empty());
    }

    #[test]
    fn zero_error_budget_fails_closed() {
        let schema = ValueSchema::String {
            min_chars: None,
            max_chars: None,
            allowed: BTreeSet::new(),
        };
        let limits = SchemaLimits {
            max_errors: 0,
            ..SchemaLimits::default()
        };
        let definition = validate_schema_definition(&schema, limits);
        assert_eq!(definition[0].code, "schema_error_limit_invalid");
        let value = validate_value(&Value::Number(7.0), &schema, limits);
        assert_eq!(value[0].code, "schema_error_limit_invalid");
    }
}
