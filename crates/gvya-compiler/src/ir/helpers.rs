//! Canonical JSON and finite-value serialization helpers.

use super::*;

pub(super) fn model_value(value: &Value) -> Result<JsonValue, IrError> {
    Ok(match value {
        Value::Null => JsonValue::Null,
        Value::Bool(value) => JsonValue::Bool(*value),
        Value::Number(value) => finite(*value, "model.value.number")?,
        Value::String(value) => string(value),
        Value::Array(values) => {
            JsonValue::Array(values.iter().map(model_value).collect::<Result<_, _>>()?)
        }
        Value::Object(values) => JsonValue::Object(
            values
                .iter()
                .map(|(key, value)| Ok((key.clone(), model_value(value)?)))
                .collect::<Result<_, IrError>>()?,
        ),
    })
}

pub(super) fn schema_document(value: &str) -> Result<JsonValue, IrError> {
    serde_json::from_str(value).map_err(|error| IrError::InvalidSchemaJson(error.to_string()))
}

pub(super) fn validate_finite(identity: &CompileIdentity) -> Result<(), IrError> {
    for (name, value) in [
        (
            "semantic.resolution_threshold",
            identity.semantic_config.resolution_threshold,
        ),
        (
            "semantic.ambiguity_margin",
            identity.semantic_config.ambiguity_margin,
        ),
        (
            "semantic.resolver_min_confidence",
            f64::from(identity.semantic_config.resolver_min_confidence),
        ),
        (
            "conversation.repair_candidate_min_score",
            identity.conversation_config.repair_candidate_min_score,
        ),
        (
            "conversation.topic_preference_margin",
            identity.conversation_config.topic_preference_margin,
        ),
    ] {
        if !value.is_finite() {
            return Err(IrError::NonFiniteNumber(name));
        }
    }
    for row in &identity.conversation_config.author_numbers {
        for (name, value) in [
            ("conversation.author_numbers.default", row.default),
            ("conversation.author_numbers.min", row.min),
            ("conversation.author_numbers.max", row.max),
        ] {
            if !value.is_finite() {
                return Err(IrError::NonFiniteNumber(name));
            }
        }
    }
    Ok(())
}

pub(super) fn finite(value: f64, field: &'static str) -> Result<JsonValue, IrError> {
    Number::from_f64(value)
        .map(JsonValue::Number)
        .ok_or(IrError::NonFiniteNumber(field))
}
pub(super) fn finite_option(value: Option<f64>, field: &'static str) -> Result<JsonValue, IrError> {
    value.map_or(Ok(JsonValue::Null), |row| finite(row, field))
}
pub(super) fn object<const N: usize>(pairs: [(&str, JsonValue); N]) -> JsonValue {
    JsonValue::Object(
        pairs
            .into_iter()
            .map(|(key, value)| (key.to_owned(), value))
            .collect(),
    )
}
pub(super) fn string(value: &str) -> JsonValue {
    JsonValue::String(value.to_owned())
}
pub(super) fn semantic_localized_texts(
    values: &[gvya_kernel::semantic::LocalizedText],
) -> JsonValue {
    JsonValue::Array(
        values
            .iter()
            .map(|value| {
                object([
                    ("language", string(&value.language)),
                    ("text", string(&value.text)),
                ])
            })
            .collect(),
    )
}
pub(super) fn map_f64(values: &BTreeMap<String, f64>) -> JsonValue {
    JsonValue::Object(
        values
            .iter()
            .filter_map(|(key, value)| {
                Number::from_f64(*value).map(|number| (key.clone(), JsonValue::Number(number)))
            })
            .collect(),
    )
}
pub(super) fn strings(values: &[String]) -> JsonValue {
    JsonValue::Array(values.iter().map(|value| string(value)).collect())
}
pub(super) fn set_strings(values: &BTreeSet<String>) -> JsonValue {
    JsonValue::Array(values.iter().map(|value| string(value)).collect())
}
pub(super) fn map_strings(values: &BTreeMap<String, String>) -> JsonValue {
    JsonValue::Object(
        values
            .iter()
            .map(|(key, value)| (key.clone(), string(value)))
            .collect(),
    )
}
pub(super) fn map_vec_strings(values: &BTreeMap<String, Vec<String>>) -> JsonValue {
    JsonValue::Object(
        values
            .iter()
            .map(|(key, value)| (key.clone(), strings(value)))
            .collect(),
    )
}
pub(super) fn uint(value: u64) -> JsonValue {
    JsonValue::Number(Number::from(value))
}
pub(super) fn integer(value: i64) -> JsonValue {
    JsonValue::Number(Number::from(value))
}
pub(super) fn usize_json(value: usize) -> JsonValue {
    uint(u64::try_from(value).unwrap_or(u64::MAX))
}
pub(super) fn option_usize(value: Option<usize>) -> JsonValue {
    value.map_or(JsonValue::Null, usize_json)
}
pub(super) fn option_u32(value: Option<u32>) -> JsonValue {
    value.map_or(JsonValue::Null, |row| uint(u64::from(row)))
}
pub(super) fn option_string(value: Option<&str>) -> JsonValue {
    value.map_or(JsonValue::Null, string)
}
pub(super) fn conversation_predicate_label(value: ConversationPredicateOp) -> &'static str {
    match value {
        ConversationPredicateOp::Exists => "exists",
        ConversationPredicateOp::Missing => "missing",
        ConversationPredicateOp::Equal => "equal",
        ConversationPredicateOp::NotEqual => "not_equal",
        ConversationPredicateOp::Greater => "greater",
        ConversationPredicateOp::GreaterOrEqual => "greater_or_equal",
        ConversationPredicateOp::Less => "less",
        ConversationPredicateOp::LessOrEqual => "less_or_equal",
    }
}
pub(super) fn admission_predicate_label(value: AdmissionPredicateOp) -> &'static str {
    match value {
        AdmissionPredicateOp::Exists => "exists",
        AdmissionPredicateOp::Missing => "missing",
        AdmissionPredicateOp::Equal => "equal",
        AdmissionPredicateOp::NotEqual => "not_equal",
        AdmissionPredicateOp::Greater => "greater",
        AdmissionPredicateOp::GreaterOrEqual => "greater_or_equal",
        AdmissionPredicateOp::Less => "less",
        AdmissionPredicateOp::LessOrEqual => "less_or_equal",
    }
}
