//! Runtime wire request decoding.

use super::*;

pub fn parse_turn_request(bytes: &[u8]) -> Result<RuntimeTurnRequest, WireError> {
    parse_turn_request_with_limits(bytes, WireLimits::default())
}

pub fn parse_turn_request_with_limits(
    bytes: &[u8],
    limits: WireLimits,
) -> Result<RuntimeTurnRequest, WireError> {
    let doc: TurnRequestDoc = parse_bounded_doc(bytes, limits)?;
    validate_turn_doc_limits(&doc, limits)?;
    if doc.format != TURN_REQUEST_FORMAT {
        return Err(WireError::Format);
    }
    if doc.version != WIRE_VERSION {
        return Err(WireError::Version(doc.version));
    }
    Ok(RuntimeTurnRequest {
        utterance: RuntimeUtteranceInput {
            text: doc.utterance.text,
        },
        context: doc.context.into_runtime()?,
        state: doc
            .state
            .map_or_else(|| Ok(GvyaState::default()), StateDoc::into_runtime)?,
        reference_candidates: doc
            .reference_candidates
            .into_iter()
            .map(ReferenceCandidateDoc::into_runtime)
            .collect::<Result<_, _>>()?,
        resolver_context: map_values(doc.resolver_context)?,
        system: map_values(doc.system)?,
        hint: doc.hint.into_runtime()?,
        seed: doc.seed,
        confirmations: doc
            .confirmations
            .into_iter()
            .map(ConfirmationDoc::into_runtime)
            .collect(),
    })
}

pub fn parse_open_request(bytes: &[u8]) -> Result<RuntimeOpenRequest, WireError> {
    parse_open_request_with_limits(bytes, WireLimits::default())
}

pub fn parse_open_request_with_limits(
    bytes: &[u8],
    limits: WireLimits,
) -> Result<RuntimeOpenRequest, WireError> {
    let doc: OpenRequestDoc = parse_bounded_doc(bytes, limits)?;
    validate_open_doc_limits(&doc, limits)?;
    if doc.format != OPEN_REQUEST_FORMAT {
        return Err(WireError::Format);
    }
    if doc.version != WIRE_VERSION {
        return Err(WireError::Version(doc.version));
    }
    Ok(RuntimeOpenRequest {
        context: doc.context.into_runtime()?,
        state: doc
            .state
            .map_or_else(|| Ok(GvyaState::default()), StateDoc::into_runtime)?,
        system: map_values(doc.system)?,
        seed: doc.seed,
        confirmations: doc
            .confirmations
            .into_iter()
            .map(ConfirmationDoc::into_runtime)
            .collect(),
    })
}

pub fn parse_capability_result_request(
    bytes: &[u8],
) -> Result<RuntimeCapabilityResultRequest, WireError> {
    parse_capability_result_request_with_limits(bytes, WireLimits::default())
}

pub fn parse_capability_result_request_with_limits(
    bytes: &[u8],
    limits: WireLimits,
) -> Result<RuntimeCapabilityResultRequest, WireError> {
    let doc: CapabilityResultRequestDoc = parse_bounded_doc(bytes, limits)?;
    if doc.format != CAPABILITY_RESULT_REQUEST_FORMAT {
        return Err(WireError::Format);
    }
    if doc.version != WIRE_VERSION {
        return Err(WireError::Version(doc.version));
    }
    Ok(RuntimeCapabilityResultRequest {
        proposal: doc.proposal.into_runtime()?,
        result: doc.result.into_runtime()?,
        context: doc.context.into_runtime()?,
        state: doc
            .state
            .map_or_else(|| Ok(GvyaState::default()), StateDoc::into_runtime)?,
        system: map_values(doc.system)?,
        seed: doc.seed,
        confirmations: doc
            .confirmations
            .into_iter()
            .map(ConfirmationDoc::into_runtime)
            .collect(),
    })
}
