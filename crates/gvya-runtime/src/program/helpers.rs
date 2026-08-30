//! Shared strict executable-document conversion helpers.

use super::*;

pub(super) fn parse_projection(value: &str) -> Result<ReferenceProjection, ProgramError> {
    match value {
        "id" => Ok(ReferenceProjection::Id),
        "object" => Ok(ReferenceProjection::Object),
        _ => Err(ProgramError::InvalidCapabilityCatalog(format!(
            "unknown reference projection: {value}"
        ))),
    }
}
pub(super) fn parse_conversation_namespace(value: &str) -> Result<StateNamespace, ProgramError> {
    match value {
        "author" => Ok(StateNamespace::Author),
        "conversation" => Ok(StateNamespace::Conversation),
        "context" => Ok(StateNamespace::Context),
        "meaning" => Ok(StateNamespace::Meaning),
        "system" => Ok(StateNamespace::System),
        "interaction" => Ok(StateNamespace::Interaction),
        _ => Err(ProgramError::InvalidConversationCatalog(format!(
            "unknown condition namespace: {value}"
        ))),
    }
}
pub(super) fn parse_conversation_op(value: &str) -> Result<ConversationPredicateOp, ProgramError> {
    match value {
        "exists" => Ok(ConversationPredicateOp::Exists),
        "missing" => Ok(ConversationPredicateOp::Missing),
        "equal" => Ok(ConversationPredicateOp::Equal),
        "not_equal" => Ok(ConversationPredicateOp::NotEqual),
        "greater" => Ok(ConversationPredicateOp::Greater),
        "greater_or_equal" => Ok(ConversationPredicateOp::GreaterOrEqual),
        "less" => Ok(ConversationPredicateOp::Less),
        "less_or_equal" => Ok(ConversationPredicateOp::LessOrEqual),
        _ => Err(ProgramError::InvalidConversationCatalog(format!(
            "unknown condition op: {value}"
        ))),
    }
}
pub(super) fn parse_admission_op(value: &str) -> Result<AdmissionPredicateOp, ProgramError> {
    match value {
        "exists" => Ok(AdmissionPredicateOp::Exists),
        "missing" => Ok(AdmissionPredicateOp::Missing),
        "equal" => Ok(AdmissionPredicateOp::Equal),
        "not_equal" => Ok(AdmissionPredicateOp::NotEqual),
        "greater" => Ok(AdmissionPredicateOp::Greater),
        "greater_or_equal" => Ok(AdmissionPredicateOp::GreaterOrEqual),
        "less" => Ok(AdmissionPredicateOp::Less),
        "less_or_equal" => Ok(AdmissionPredicateOp::LessOrEqual),
        _ => Err(ProgramError::InvalidCapabilityCatalog(format!(
            "unknown admission op: {value}"
        ))),
    }
}

pub(super) fn model_value(value: &JsonValue) -> Result<Value, ProgramError> {
    Ok(match value {
        JsonValue::Null => Value::Null,
        JsonValue::Bool(value) => Value::Bool(*value),
        JsonValue::Number(value) => {
            let value = value.as_f64().ok_or_else(|| {
                ProgramError::Json("number cannot be represented as finite f64".into())
            })?;
            if !value.is_finite() {
                return Err(ProgramError::Json("non-finite number".into()));
            }
            Value::Number(value)
        }
        JsonValue::String(value) => Value::String(value.clone()),
        JsonValue::Array(values) => {
            Value::Array(values.iter().map(model_value).collect::<Result<_, _>>()?)
        }
        JsonValue::Object(values) => Value::Object(
            values
                .iter()
                .map(|(key, value)| Ok((key.clone(), model_value(value)?)))
                .collect::<Result<_, ProgramError>>()?,
        ),
    })
}

pub(super) fn safe_asset_path(path: &str) -> bool {
    path.starts_with("assets/")
        && !path.ends_with('/')
        && !path.contains('\\')
        && !path.contains('\0')
        && !path
            .split('/')
            .any(|segment| segment.is_empty() || segment == "." || segment == "..")
}
