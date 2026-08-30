//! Resolver proposal validation and deterministic binding.

use super::entities::builtin_entity_value_is_canonical;
use super::*;

const RESOLVER_PROPOSAL_MAX_SLOTS: usize = 64;
const RESOLVER_PROPOSAL_MAX_REFERENCES: usize = 64;
pub(super) const RESOLVER_PROPOSAL_MAX_EVIDENCE: usize = 64;
const RESOLVER_PROPOSAL_MAX_STRING_BYTES: usize = 8 * 1024;
const RESOLVER_PROPOSAL_MAX_TOTAL_TEXT_BYTES: usize = 256 * 1024;
const RESOLVER_PROPOSAL_MAX_VALUE_DEPTH: usize = 16;
const RESOLVER_PROPOSAL_MAX_VALUE_NODES: usize = 2_048;

pub(super) fn resolver_proposal_within_limits(proposal: &ResolverProposal) -> bool {
    if proposal.slots.len() > RESOLVER_PROPOSAL_MAX_SLOTS
        || proposal.references.len() > RESOLVER_PROPOSAL_MAX_REFERENCES
        || proposal.evidence.len() > RESOLVER_PROPOSAL_MAX_EVIDENCE
    {
        return false;
    }
    let mut nodes = 0usize;
    let mut bytes = 0usize;
    for slot in &proposal.slots {
        bytes = bytes.saturating_add(slot.name.len());
        if slot.name.len() > RESOLVER_PROPOSAL_MAX_STRING_BYTES
            || !resolver_value_within_limits(&slot.value, 1, &mut nodes, &mut bytes)
        {
            return false;
        }
    }
    for reference in &proposal.references {
        bytes = bytes
            .saturating_add(reference.kind.as_str().len())
            .saturating_add(reference.id.as_str().len());
        if reference.kind.as_str().len() > RESOLVER_PROPOSAL_MAX_STRING_BYTES
            || reference.id.as_str().len() > RESOLVER_PROPOSAL_MAX_STRING_BYTES
        {
            return false;
        }
    }
    for row in &proposal.evidence {
        bytes = bytes.saturating_add(row.len());
        if row.len() > RESOLVER_PROPOSAL_MAX_STRING_BYTES {
            return false;
        }
    }
    bytes <= RESOLVER_PROPOSAL_MAX_TOTAL_TEXT_BYTES
}

pub(super) fn resolver_proposal_targets_are_unique(proposal: &ResolverProposal) -> bool {
    let mut slots = BTreeSet::new();
    let mut references = BTreeSet::new();
    proposal
        .slots
        .iter()
        .all(|slot| slots.insert(slot.name.as_str()))
        && proposal
            .references
            .iter()
            .all(|reference| references.insert(reference.kind.as_str()))
}

fn resolver_value_within_limits(
    value: &Value,
    depth: usize,
    nodes: &mut usize,
    bytes: &mut usize,
) -> bool {
    if depth > RESOLVER_PROPOSAL_MAX_VALUE_DEPTH {
        return false;
    }
    *nodes = nodes.saturating_add(1);
    if *nodes > RESOLVER_PROPOSAL_MAX_VALUE_NODES {
        return false;
    }
    match value {
        Value::Null | Value::Bool(_) => true,
        Value::Number(number) => number.is_finite(),
        Value::String(value) => {
            *bytes = bytes.saturating_add(value.len());
            value.len() <= RESOLVER_PROPOSAL_MAX_STRING_BYTES
                && *bytes <= RESOLVER_PROPOSAL_MAX_TOTAL_TEXT_BYTES
        }
        Value::Array(values) => {
            values.len() <= RESOLVER_PROPOSAL_MAX_VALUE_NODES
                && values
                    .iter()
                    .all(|row| resolver_value_within_limits(row, depth + 1, nodes, bytes))
        }
        Value::Object(values) => {
            values.len() <= RESOLVER_PROPOSAL_MAX_VALUE_NODES
                && values.iter().all(|(key, row)| {
                    *bytes = bytes.saturating_add(key.len());
                    key.len() <= RESOLVER_PROPOSAL_MAX_STRING_BYTES
                        && *bytes <= RESOLVER_PROPOSAL_MAX_TOTAL_TEXT_BYTES
                        && resolver_value_within_limits(row, depth + 1, nodes, bytes)
                })
        }
    }
}

#[derive(Debug)]
pub enum ResolverRunError<E> {
    Resolver(E),
}

/// Deterministic verdict on one untrusted resolver proposal.
///
/// A rejected review leaves the deterministic semantic decision untouched, so a weak or hostile
/// resolver can only ever fail to help; it can never degrade the deterministic outcome.
#[derive(Clone, Debug, PartialEq)]
pub struct ResolverReview {
    pub accepted: bool,
    pub reason_code: String,
    pub meaning: Option<Meaning>,
    pub partial: Option<PartialMeaning>,
}

impl ResolverReview {
    pub(super) fn rejected(reason: &str) -> Self {
        Self {
            accepted: false,
            reason_code: reason.to_string(),
            meaning: None,
            partial: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(super) enum BindOutcome {
    Resolved(Meaning),
    Partial(PartialMeaning),
    Ambiguous(String),
    Invalid(String),
}

pub(super) fn bind_meaning(
    pattern: &MeaningPattern,
    entities: &[SemanticEntity],
    normalized: &str,
    references: &[ResolverReferenceCandidate],
    profile: &SemanticProfile,
) -> BindOutcome {
    bind_meaning_with_slots(
        pattern,
        entities,
        normalized,
        references,
        profile,
        Vec::new(),
    )
}

pub(super) fn bind_meaning_with_slots(
    pattern: &MeaningPattern,
    entities: &[SemanticEntity],
    normalized: &str,
    references: &[ResolverReferenceCandidate],
    profile: &SemanticProfile,
    prebound_slots: Vec<SlotValue>,
) -> BindOutcome {
    let mut prebound = BTreeMap::new();
    for slot in prebound_slots {
        if prebound.insert(slot.name.clone(), slot).is_some() {
            return BindOutcome::Ambiguous("duplicate_prebound_slot".to_string());
        }
    }
    let mut slots = Vec::new();
    let mut missing_required_values = Vec::new();
    for spec in &pattern.slots {
        if let Some(slot) = prebound.remove(&spec.name) {
            if !slot_value_matches_kind(&slot.value, &spec.kind) {
                return BindOutcome::Invalid(format!("invalid_prebound_slot:{}", spec.name));
            }
            slots.push(slot);
            continue;
        }
        match &spec.kind {
            SlotKind::Entity(kind) => {
                let matching: Vec<_> = entities
                    .iter()
                    .filter(|entity| entity.kind == *kind)
                    .collect();
                if matching.len() > 1 {
                    let unique: BTreeSet<_> = matching
                        .iter()
                        .map(|entity| format!("{:?}", entity.value))
                        .collect();
                    if unique.len() > 1 {
                        return BindOutcome::Ambiguous(format!(
                            "multiple_values_for_slot:{}",
                            spec.name
                        ));
                    }
                }
                if let Some(entity) = matching.first() {
                    slots.push(SlotValue {
                        name: spec.name.clone(),
                        value: entity.value.clone(),
                        provenance: ValueProvenance::Utterance,
                    });
                } else if spec.required {
                    missing_required_values.push(MissingRequiredValue::Slot {
                        name: spec.name.clone(),
                    });
                }
            }
            SlotKind::Number => {
                let matching: Vec<_> = entities
                    .iter()
                    .filter(|entity| entity.kind.as_str() == "number")
                    .collect();
                let unique: BTreeSet<_> = matching
                    .iter()
                    .map(|entity| format!("{:?}", entity.value))
                    .collect();
                if unique.len() > 1 {
                    return BindOutcome::Ambiguous(format!(
                        "multiple_values_for_slot:{}",
                        spec.name
                    ));
                }
                if let Some(entity) = matching.first() {
                    slots.push(SlotValue {
                        name: spec.name.clone(),
                        value: entity.value.clone(),
                        provenance: ValueProvenance::Utterance,
                    });
                } else if spec.required {
                    missing_required_values.push(MissingRequiredValue::Slot {
                        name: spec.name.clone(),
                    });
                }
            }
            SlotKind::String | SlotKind::Boolean => {
                if spec.required {
                    missing_required_values.push(MissingRequiredValue::Slot {
                        name: spec.name.clone(),
                    });
                }
            }
            SlotKind::Reference(kind) => {
                let matched = match_references(normalized, references, kind, profile);
                if matched.len() > 1 {
                    return BindOutcome::Ambiguous(format!(
                        "multiple_references_for_slot:{}",
                        spec.name
                    ));
                }
                if let Some(reference) = matched.first() {
                    slots.push(SlotValue {
                        name: spec.name.clone(),
                        value: Value::String(reference.id.as_str().to_string()),
                        provenance: ValueProvenance::HostReferenceResolver,
                    });
                } else if spec.required {
                    missing_required_values.push(MissingRequiredValue::Slot {
                        name: spec.name.clone(),
                    });
                }
            }
        }
    }

    if let Some((name, _)) = prebound.into_iter().next() {
        return BindOutcome::Invalid(format!("unknown_prebound_slot:{name}"));
    }

    let mut resolved_references = Vec::new();
    for spec in &pattern.references {
        let matched = match_references(normalized, references, &spec.kind, profile);
        if matched.len() > 1 {
            return BindOutcome::Ambiguous(format!(
                "multiple_references_of_kind:{}",
                spec.kind.as_str()
            ));
        }
        if let Some(reference) = matched.first() {
            resolved_references.push((*reference).clone());
        } else if spec.required {
            missing_required_values.push(MissingRequiredValue::Reference {
                kind: spec.kind.clone(),
            });
        }
    }
    resolved_references.sort_by(|a, b| a.kind.cmp(&b.kind).then_with(|| a.id.cmp(&b.id)));
    resolved_references.dedup();
    let meaning = Meaning {
        id: pattern.id.clone(),
        slots,
        references: resolved_references,
    };
    if missing_required_values.is_empty() {
        BindOutcome::Resolved(meaning)
    } else {
        BindOutcome::Partial(PartialMeaning {
            meaning,
            missing_required_values,
        })
    }
}

pub(super) fn match_references<'a>(
    normalized: &str,
    candidates: &'a [ResolverReferenceCandidate],
    kind: &ReferenceKind,
    profile: &SemanticProfile,
) -> Vec<&'a HostReference> {
    let input_tokens = profile.normalize_colloquial_tokens(&ordered_tokens(normalized));
    let mut matched = Vec::new();
    for candidate in candidates {
        if candidate.reference.kind != *kind {
            continue;
        }
        let mut aliases = candidate.aliases.clone();
        if let Some(label) = &candidate.label {
            aliases.push(label.clone());
        }
        let is_match = aliases.iter().any(|alias| {
            let alias_tokens =
                profile.normalize_colloquial_tokens(&ordered_tokens(&normalize_text(alias)));
            !alias_tokens.is_empty()
                && matching::find_token_span(&input_tokens, &alias_tokens).is_some()
        });
        if is_match {
            matched.push(&candidate.reference);
        }
    }
    matched
}

pub(super) fn slot_value_matches_kind(value: &Value, kind: &SlotKind) -> bool {
    match kind {
        SlotKind::String | SlotKind::Reference(_) => matches!(value, Value::String(_)),
        SlotKind::Number => matches!(value, Value::Number(_)),
        SlotKind::Boolean => matches!(value, Value::Bool(_)),
        SlotKind::Entity(entity_kind) => entity_value_matches_kind(value, entity_kind),
    }
}

/// Type authority for one untrusted proposed value.
///
/// Every value passes the canonical GVYA rules that ordinary deterministic binding uses. There is
/// no resolver-only coercion: a built-in entity value must match the extractor's own canonical
/// shape, and any other entity kind must name a canonical value in the authored custom entity
/// catalog selected for this turn's language profile. An entity kind with no authority at all in
/// the active profile fails closed rather than accepting arbitrary content.
pub(super) fn resolver_slot_value_matches_kind(
    value: &Value,
    kind: &SlotKind,
    profile: &SemanticProfile,
) -> bool {
    match kind {
        SlotKind::String | SlotKind::Reference(_) => matches!(value, Value::String(_)),
        SlotKind::Number => matches!(value, Value::Number(number) if number.is_finite()),
        SlotKind::Boolean => matches!(value, Value::Bool(_)),
        SlotKind::Entity(entity_kind) => {
            resolver_entity_value_is_canonical(value, entity_kind, profile)
        }
    }
}

fn resolver_entity_value_is_canonical(
    value: &Value,
    kind: &EntityKind,
    profile: &SemanticProfile,
) -> bool {
    if let Some(canonical) = builtin_entity_value_is_canonical(kind.as_str(), value, profile) {
        return canonical;
    }
    let Some(catalog) = profile.custom_entities.get(kind.as_str()) else {
        return false;
    };
    matches!(value, Value::String(canonical) if catalog.contains_key(canonical))
}

fn entity_value_matches_kind(value: &Value, kind: &EntityKind) -> bool {
    match kind.as_str() {
        "number" => matches!(value, Value::Number(_)),
        "quantity" => matches!(value, Value::Object(_)),
        "date" | "time" | "color" | "unit" | "email" | "phone" | "url" | "origin" => {
            matches!(value, Value::String(_))
        }
        // Authored custom entities have canonical string values. Resolver review additionally
        // checks membership in the selected Language Profile catalog.
        _ => !matches!(value, Value::Null),
    }
}

/// Canonical ordering for an accepted Meaning.
///
/// Deterministic binding and collection continuation both emit slots/references in a stable
/// canonical order. Resolver acceptance uses the same ordering so an identical proposal cannot
/// produce a different semantic state merely because the untrusted source listed values in a
/// different order.
pub(super) fn canonicalize_meaning(meaning: &mut Meaning) {
    meaning
        .slots
        .sort_by(|left, right| left.name.cmp(&right.name));
    meaning.references.sort_by(|left, right| {
        left.kind
            .cmp(&right.kind)
            .then_with(|| left.id.cmp(&right.id))
    });
    meaning.references.dedup();
}
