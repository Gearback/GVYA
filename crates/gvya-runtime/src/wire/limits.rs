//! Bounded JSON parsing, shape and serialization limits.

use super::*;

pub(super) fn parse_bounded_doc<T: DeserializeOwned>(
    bytes: &[u8],
    limits: WireLimits,
) -> Result<T, WireError> {
    if bytes.len() > limits.max_request_bytes {
        return Err(WireError::Invalid(
            "runtime request exceeds configured byte limit".into(),
        ));
    }
    let value: JsonValue =
        serde_json::from_slice(bytes).map_err(|error| WireError::Json(error.to_string()))?;
    let mut nodes = 0usize;
    validate_json_shape(&value, 0, &mut nodes, limits)?;
    serde_json::from_value(value).map_err(|error| WireError::Json(error.to_string()))
}

pub(super) fn validate_json_shape(
    value: &JsonValue,
    depth: usize,
    nodes: &mut usize,
    limits: WireLimits,
) -> Result<(), WireError> {
    if depth > limits.max_value_depth {
        return Err(WireError::Invalid(
            "runtime JSON exceeds configured depth limit".into(),
        ));
    }
    *nodes = nodes.saturating_add(1);
    if *nodes > limits.max_value_nodes {
        return Err(WireError::Invalid(
            "runtime JSON exceeds configured node limit".into(),
        ));
    }
    match value {
        JsonValue::String(value) => {
            if value.len() > limits.max_string_bytes {
                return Err(WireError::Invalid(
                    "runtime JSON string exceeds configured byte limit".into(),
                ));
            }
        }
        JsonValue::Array(values) => {
            if values.len() > limits.max_collection_entries {
                return Err(WireError::Invalid(
                    "runtime JSON array exceeds configured entry limit".into(),
                ));
            }
            for value in values {
                validate_json_shape(value, depth + 1, nodes, limits)?;
            }
        }
        JsonValue::Object(values) => {
            if values.len() > limits.max_collection_entries {
                return Err(WireError::Invalid(
                    "runtime JSON object exceeds configured entry limit".into(),
                ));
            }
            for (key, value) in values {
                if key.len() > limits.max_string_bytes {
                    return Err(WireError::Invalid(
                        "runtime JSON key exceeds configured byte limit".into(),
                    ));
                }
                validate_json_shape(value, depth + 1, nodes, limits)?;
            }
        }
        JsonValue::Null | JsonValue::Bool(_) | JsonValue::Number(_) => {}
    }
    Ok(())
}

pub(super) fn validate_turn_doc_limits(
    doc: &TurnRequestDoc,
    limits: WireLimits,
) -> Result<(), WireError> {
    validate_context_doc_limits(&doc.context, limits)?;
    if doc.reference_candidates.len() > limits.max_reference_candidates {
        return Err(WireError::Invalid(
            "too many resolver reference candidates".into(),
        ));
    }
    if doc.confirmations.len() > limits.max_confirmations {
        return Err(WireError::Invalid("too many confirmation grants".into()));
    }
    if let Some(state) = &doc.state {
        validate_state_doc_limits(state)?;
    }
    Ok(())
}

pub(super) fn validate_open_doc_limits(
    doc: &OpenRequestDoc,
    limits: WireLimits,
) -> Result<(), WireError> {
    validate_context_doc_limits(&doc.context, limits)?;
    if doc.confirmations.len() > limits.max_confirmations {
        return Err(WireError::Invalid("too many confirmation grants".into()));
    }
    if let Some(state) = &doc.state {
        validate_state_doc_limits(state)?;
    }
    Ok(())
}

pub(super) fn validate_context_doc_limits(
    doc: &ContextDoc,
    limits: WireLimits,
) -> Result<(), WireError> {
    if doc.visible_references.len() > limits.max_visible_references {
        return Err(WireError::Invalid("too many visible references".into()));
    }
    if doc.available_capabilities.len() > limits.max_available_capabilities {
        return Err(WireError::Invalid("too many available capabilities".into()));
    }
    Ok(())
}

pub(super) fn validate_state_doc_limits(doc: &StateDoc) -> Result<(), WireError> {
    if let Some(collection) = &doc.conversation.active_collection {
        if collection.remaining.is_empty()
            || collection.remaining.len() > MAX_ACTIVE_COLLECTION_VALUES
            || collection.meaning.slots.len() > MAX_ACTIVE_COLLECTION_VALUES
            || collection.meaning.references.len() > MAX_ACTIVE_COLLECTION_VALUES
        {
            return Err(WireError::Invalid(
                "conversation active_collection exceeds runtime state limit".into(),
            ));
        }
    }
    if doc.conversation.mentioned_topics.len() > MAX_MENTIONED_TOPICS {
        return Err(WireError::Invalid(
            "conversation mentioned_topics exceeds runtime state limit".into(),
        ));
    }
    if doc.conversation.hint_progress.len() > MAX_HINT_PROGRESS_ENTRIES {
        return Err(WireError::Invalid(
            "conversation hint_progress exceeds runtime state limit".into(),
        ));
    }
    if doc.conversation.focus.len() > MAX_FOCUS_REFERENCES {
        return Err(WireError::Invalid(
            "conversation focus exceeds runtime state limit".into(),
        ));
    }
    if doc.conversation.recent_response_ids.len() > MAX_RECENT_RESPONSE_IDS {
        return Err(WireError::Invalid(
            "conversation recent_response_ids exceeds runtime state limit".into(),
        ));
    }
    if doc.conversation.recent_variant_keys.len() > MAX_RECENT_VARIANT_KEYS {
        return Err(WireError::Invalid(
            "conversation recent_variant_keys exceeds runtime state limit".into(),
        ));
    }
    if doc.conversation.recent_user_messages.len() > MAX_RECENT_USER_MESSAGES {
        return Err(WireError::Invalid(
            "conversation recent_user_messages exceeds runtime state limit".into(),
        ));
    }
    if doc.conversation.pending_capabilities.len() > CAPABILITY_PENDING_PROPOSALS_MAX {
        return Err(WireError::Invalid(
            "conversation pending_capabilities exceeds runtime state limit".into(),
        ));
    }
    Ok(())
}

pub(super) fn serialize_bounded(
    value: &JsonValue,
    limits: WireLimits,
) -> Result<Vec<u8>, WireError> {
    let mut writer = LimitedVecWriter::new(limits.max_response_bytes);
    match serde_json::to_writer(&mut writer, value) {
        Ok(()) => Ok(writer.into_inner()),
        Err(_) if writer.exceeded() => Err(WireError::Invalid(
            "runtime response exceeds configured byte limit".into(),
        )),
        Err(error) => Err(WireError::Json(error.to_string())),
    }
}

/// JSON output sink that never allocates past the configured response ceiling. This keeps the
/// response budget authoritative even when Runtime is used directly rather than through an FFI
/// adapter, and avoids the previous serialize-then-reject allocation spike.
pub(super) struct LimitedVecWriter {
    bytes: Vec<u8>,
    limit: usize,
    exceeded: bool,
}

impl LimitedVecWriter {
    pub(super) fn new(limit: usize) -> Self {
        Self {
            bytes: Vec::with_capacity(limit.min(8 * 1024)),
            limit,
            exceeded: false,
        }
    }

    pub(super) fn exceeded(&self) -> bool {
        self.exceeded
    }

    pub(super) fn into_inner(self) -> Vec<u8> {
        self.bytes
    }
}

impl Write for LimitedVecWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let Some(next) = self.bytes.len().checked_add(buf.len()) else {
            self.exceeded = true;
            return Err(io::Error::other("GVYA response byte limit exceeded"));
        };
        if next > self.limit {
            self.exceeded = true;
            return Err(io::Error::other("GVYA response byte limit exceeded"));
        }
        self.bytes.extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
