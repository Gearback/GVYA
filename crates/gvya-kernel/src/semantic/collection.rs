//! Context-constrained interpretation for one canonical active collection.

use std::collections::BTreeSet;

use gvya_model::{
    ActiveCollection, HostReference, Meaning, MissingRequiredValue, SlotValue, Value,
    ValueProvenance,
};

use super::{
    PartialMeaning, ResolverRunError, SemanticInput, SemanticKernel, SlotKind,
    build_semantic_views, canonicalize_meaning, resolver_proposal_targets_are_unique,
    resolver_proposal_within_limits, resolver_slot_value_matches_kind, slot_value_matches_kind,
};
use crate::{
    RESOLVER_COLLECTION_BOUND_VALUES_MAX, RESOLVER_COLLECTION_TARGETS_MAX, ResolverCandidateOrigin,
    ResolverCollectionContext, ResolverCollectionTarget, ResolverProposal, ResolverRequest,
    ResolverTask, SemanticResolver,
};

pub const MAX_ACTIVE_COLLECTION_VALUES: usize = 64;

#[derive(Clone, Debug, PartialEq)]
pub enum CollectionTurnDecision {
    Completed(Meaning),
    Progressed {
        partial: PartialMeaning,
        reason_code: String,
    },
    Ambiguous {
        reason_code: String,
    },
    Invalid {
        reason_code: String,
    },
}

enum TargetResolution {
    Slot(SlotValue),
    Reference(HostReference),
    Missing(String),
    Ambiguous(String),
}

impl SemanticKernel {
    #[must_use]
    pub fn continue_collection(
        &self,
        input: &SemanticInput,
        active: &ActiveCollection,
    ) -> CollectionTurnDecision {
        let Some(pattern) = self.catalog().get(&active.meaning.id) else {
            return invalid_state("collection_unknown_meaning");
        };
        let Some(profile) = self.profile_for_language(
            input.utterance.language.as_deref(),
            &input.language_fallbacks,
        ) else {
            return invalid_state("collection_missing_language_profile");
        };
        if let Err(reason_code) = validate_active_collection(pattern, active, input) {
            return invalid_state(&reason_code);
        }
        let views = build_semantic_views(&input.utterance.text, profile, None);
        let mut meaning = active.meaning.clone();
        let mut remaining = active.remaining.clone();
        let mut collected = 0usize;
        let mut used_classes = BTreeSet::new();
        let mut stop_reason = "collection_value_missing".to_string();

        while let Some(target) = remaining.first().cloned() {
            let class = target_class(pattern, &target).unwrap_or_default();
            if collected > 0 && !used_classes.insert(class.clone()) {
                break;
            }
            used_classes.insert(class);
            match resolve_target(
                pattern,
                &target,
                &input.utterance.text,
                &views.normalized,
                &views.entities,
                &input.reference_candidates,
                profile,
                collected == 0,
            ) {
                TargetResolution::Slot(slot) => meaning.slots.push(slot),
                TargetResolution::Reference(reference) => meaning.references.push(reference),
                TargetResolution::Missing(reason) => {
                    stop_reason = reason;
                    break;
                }
                TargetResolution::Ambiguous(reason_code) if collected == 0 => {
                    return CollectionTurnDecision::Ambiguous { reason_code };
                }
                TargetResolution::Ambiguous(reason) => {
                    stop_reason = reason;
                    break;
                }
            }
            remaining.remove(0);
            collected = collected.saturating_add(1);
            if matches!(target, MissingRequiredValue::Slot { ref name }
                if pattern.slots.iter().any(|slot| slot.name == *name && matches!(slot.kind, SlotKind::String)))
            {
                break;
            }
        }

        canonicalize_meaning(&mut meaning);
        if remaining.is_empty() {
            CollectionTurnDecision::Completed(meaning)
        } else if collected > 0 {
            CollectionTurnDecision::Progressed {
                partial: PartialMeaning {
                    meaning,
                    missing_required_values: remaining,
                },
                reason_code: stop_reason,
            }
        } else {
            CollectionTurnDecision::Invalid {
                reason_code: stop_reason,
            }
        }
    }

    pub fn continue_collection_with_resolver<R: SemanticResolver + ?Sized>(
        &self,
        input: &SemanticInput,
        active: &ActiveCollection,
        resolver: &R,
    ) -> Result<CollectionTurnDecision, ResolverRunError<R::Error>> {
        let deterministic = self.continue_collection(input, active);
        let CollectionTurnDecision::Invalid { reason_code } = &deterministic else {
            return Ok(deterministic);
        };
        if reason_code.starts_with("collection_state_") {
            return Ok(deterministic);
        }
        let Some(request) = self.collection_resolver_request(input, active) else {
            return Ok(deterministic);
        };
        let proposal = resolver
            .propose(&request)
            .map_err(ResolverRunError::Resolver)?;
        Ok(self.review_collection_proposal(input, active, &request, proposal))
    }

    /// Build the bounded resolver-safe projection for a `FillCollection` turn.
    ///
    /// The Meaning is already deterministic authority, so the candidate set is exactly that one
    /// Meaning. Already-bound values are read-only interpretation context, and the collectable
    /// declarations are derived from the same `ActiveCollection::remaining` state the firewall
    /// validates against, so there is no second collection schema anywhere.
    #[must_use]
    pub fn collection_resolver_request(
        &self,
        input: &SemanticInput,
        active: &ActiveCollection,
    ) -> Option<ResolverRequest> {
        let pattern = self.catalog().get(&active.meaning.id)?;
        let profile = self.profile_for_language(
            input.utterance.language.as_deref(),
            &input.language_fallbacks,
        )?;
        let candidate = super::projection::meaning_candidate(
            pattern,
            ResolverCandidateOrigin::ActiveCollection,
            None,
            profile,
            input.utterance.language.as_deref(),
            &input.language_fallbacks,
        );
        let mut collectable = Vec::new();
        for target in active
            .remaining
            .iter()
            .take(RESOLVER_COLLECTION_TARGETS_MAX)
        {
            match target {
                MissingRequiredValue::Slot { name } => {
                    if let Some(schema) = super::projection::slot_schema_for(pattern, name, profile)
                    {
                        collectable.push(ResolverCollectionTarget::Slot(schema));
                    }
                }
                MissingRequiredValue::Reference { kind } => {
                    let required = pattern
                        .references
                        .iter()
                        .find(|spec| spec.kind == *kind)
                        .is_some_and(|spec| spec.required);
                    collectable.push(ResolverCollectionTarget::Reference(
                        super::projection::reference_schema(kind, required),
                    ));
                }
            }
        }
        Some(ResolverRequest {
            task: ResolverTask::FillCollection,
            utterance: input.utterance.clone(),
            language_fallbacks: super::language_fallbacks(
                input.utterance.language.as_deref(),
                &input.language_fallbacks,
            ),
            candidates: vec![candidate],
            collection: Some(ResolverCollectionContext {
                meaning: active.meaning.id.clone(),
                bound_slots: active
                    .meaning
                    .slots
                    .iter()
                    .take(RESOLVER_COLLECTION_BOUND_VALUES_MAX)
                    .cloned()
                    .collect(),
                bound_references: active
                    .meaning
                    .references
                    .iter()
                    .take(RESOLVER_COLLECTION_BOUND_VALUES_MAX)
                    .cloned()
                    .collect(),
                collectable,
            }),
            reference_candidates: self.exposed_reference_candidates(input),
            exposed_context: super::exposed_resolver_context(&input.resolver_context),
        })
    }

    fn review_collection_proposal(
        &self,
        input: &SemanticInput,
        active: &ActiveCollection,
        request: &ResolverRequest,
        proposal: ResolverProposal,
    ) -> CollectionTurnDecision {
        if !resolver_proposal_within_limits(&proposal)
            || proposal.meaning.as_ref() != Some(&active.meaning.id)
            || !proposal
                .meaning
                .as_ref()
                .is_some_and(|meaning| request.permits_meaning(meaning))
        {
            return CollectionTurnDecision::Invalid {
                reason_code: "resolver_collection_boundary_rejected".into(),
            };
        }
        if !resolver_proposal_targets_are_unique(&proposal) {
            return CollectionTurnDecision::Invalid {
                reason_code: "resolver_collection_duplicate_target".into(),
            };
        }
        let Some(confidence) = proposal.confidence else {
            return CollectionTurnDecision::Invalid {
                reason_code: "resolver_missing_confidence".into(),
            };
        };
        if !confidence.is_finite()
            || !(super::SEMANTIC_RESOLVER_CONFIDENCE_MIN..=super::SEMANTIC_RESOLVER_CONFIDENCE_MAX)
                .contains(&confidence)
        {
            return CollectionTurnDecision::Invalid {
                reason_code: "resolver_invalid_confidence".into(),
            };
        }
        if confidence < self.config().resolver_min_confidence {
            return CollectionTurnDecision::Invalid {
                reason_code: "resolver_collection_confidence_rejected".into(),
            };
        }
        let Some(pattern) = self.catalog().get(&active.meaning.id) else {
            return invalid_state("collection_unknown_meaning");
        };
        let Some(profile) = self.profile_for_language(
            input.utterance.language.as_deref(),
            &input.language_fallbacks,
        ) else {
            return invalid_state("collection_missing_language_profile");
        };
        let remaining_slots = active
            .remaining
            .iter()
            .filter_map(|target| match target {
                MissingRequiredValue::Slot { name } => Some(name.as_str()),
                MissingRequiredValue::Reference { .. } => None,
            })
            .collect::<BTreeSet<_>>();
        let remaining_references = active
            .remaining
            .iter()
            .filter_map(|target| match target {
                MissingRequiredValue::Reference { kind } => Some(kind),
                MissingRequiredValue::Slot { .. } => None,
            })
            .collect::<BTreeSet<_>>();
        if proposal
            .slots
            .iter()
            .any(|slot| !remaining_slots.contains(slot.name.as_str()))
            || proposal
                .references
                .iter()
                .any(|reference| !remaining_references.contains(&reference.kind))
        {
            return CollectionTurnDecision::Invalid {
                reason_code: "resolver_collection_undeclared_target".into(),
            };
        }
        let mut meaning = active.meaning.clone();
        let mut remaining = active.remaining.clone();
        let mut collected = 0usize;
        for target in &active.remaining {
            match target {
                MissingRequiredValue::Slot { name } => {
                    let Some(proposed) = proposal.slots.iter().find(|slot| slot.name == *name)
                    else {
                        break;
                    };
                    let Some(spec) = pattern.slots.iter().find(|slot| slot.name == *name) else {
                        return invalid_state("collection_unknown_slot");
                    };
                    if !resolver_slot_value_matches_kind(&proposed.value, &spec.kind, profile)
                        || matches!(&spec.kind, SlotKind::Reference(kind) if !reference_slot_is_exposed(&proposed.value, kind, request))
                    {
                        return CollectionTurnDecision::Invalid {
                            reason_code: "resolver_collection_slot_rejected".into(),
                        };
                    }
                    meaning.slots.push(SlotValue {
                        name: name.clone(),
                        value: proposed.value.clone(),
                        provenance: ValueProvenance::NeuralProposal,
                    });
                }
                MissingRequiredValue::Reference { kind } => {
                    let Some(reference) = proposal.references.iter().find(|row| row.kind == *kind)
                    else {
                        break;
                    };
                    if !request
                        .reference_candidates
                        .iter()
                        .any(|candidate| candidate.reference == *reference)
                    {
                        return CollectionTurnDecision::Invalid {
                            reason_code: "resolver_collection_reference_rejected".into(),
                        };
                    }
                    meaning.references.push(reference.clone());
                }
            }
            remaining.remove(0);
            collected = collected.saturating_add(1);
        }
        canonicalize_meaning(&mut meaning);
        if remaining.is_empty() {
            CollectionTurnDecision::Completed(meaning)
        } else if collected > 0 {
            CollectionTurnDecision::Progressed {
                partial: PartialMeaning {
                    meaning,
                    missing_required_values: remaining,
                },
                reason_code: "resolver_collection_values_validated".into(),
            }
        } else {
            CollectionTurnDecision::Invalid {
                reason_code: "resolver_collection_no_value".into(),
            }
        }
    }
}

fn validate_active_collection(
    pattern: &super::MeaningPattern,
    active: &ActiveCollection,
    input: &SemanticInput,
) -> Result<(), String> {
    if active.remaining.is_empty() || active.remaining.len() > MAX_ACTIVE_COLLECTION_VALUES {
        return Err("collection_remaining_outside_bounds".into());
    }
    let mut slot_names = BTreeSet::new();
    for slot in &active.meaning.slots {
        let Some(spec) = pattern.slots.iter().find(|spec| spec.name == slot.name) else {
            return Err("collection_unknown_bound_slot".into());
        };
        if !slot_names.insert(slot.name.as_str())
            || !slot_value_matches_kind(&slot.value, &spec.kind)
        {
            return Err("collection_invalid_bound_slot".into());
        }
        if matches!(&spec.kind, SlotKind::Reference(kind) if !bound_reference_slot_is_visible(&slot.value, kind, input))
        {
            return Err("collection_stale_reference_slot".into());
        }
    }
    let mut references = BTreeSet::new();
    for reference in &active.meaning.references {
        if !pattern
            .references
            .iter()
            .any(|spec| spec.kind == reference.kind)
            || !references.insert((&reference.kind, &reference.id))
            || !input
                .reference_candidates
                .iter()
                .any(|candidate| candidate.reference == *reference)
        {
            return Err("collection_invalid_bound_reference".into());
        }
    }
    let expected = pattern
        .slots
        .iter()
        .filter(|spec| spec.required && !slot_names.contains(spec.name.as_str()))
        .map(|spec| MissingRequiredValue::Slot {
            name: spec.name.clone(),
        })
        .chain(
            pattern
                .references
                .iter()
                .filter(|spec| {
                    spec.required
                        && !active
                            .meaning
                            .references
                            .iter()
                            .any(|reference| reference.kind == spec.kind)
                })
                .map(|spec| MissingRequiredValue::Reference {
                    kind: spec.kind.clone(),
                }),
        )
        .collect::<Vec<_>>();
    if expected != active.remaining {
        return Err("collection_remaining_mismatch".into());
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn resolve_target(
    pattern: &super::MeaningPattern,
    target: &MissingRequiredValue,
    raw: &str,
    normalized: &str,
    entities: &[super::SemanticEntity],
    references: &[crate::ResolverReferenceCandidate],
    profile: &super::SemanticProfile,
    first: bool,
) -> TargetResolution {
    match target {
        MissingRequiredValue::Slot { name } => {
            let Some(spec) = pattern.slots.iter().find(|slot| slot.name == *name) else {
                return TargetResolution::Missing("collection_unknown_slot".into());
            };
            match &spec.kind {
                SlotKind::String if first => {
                    let value = raw.trim();
                    if value.is_empty() {
                        return TargetResolution::Missing("collection_empty_string".into());
                    }
                    TargetResolution::Slot(SlotValue {
                        name: name.clone(),
                        value: Value::String(value.to_string()),
                        provenance: ValueProvenance::Utterance,
                    })
                }
                SlotKind::String => {
                    TargetResolution::Missing("collection_string_not_isolated".into())
                }
                SlotKind::Boolean => match boolean_target(normalized, profile) {
                    Ok(Some(value)) => TargetResolution::Slot(SlotValue {
                        name: name.clone(),
                        value: Value::Bool(value),
                        provenance: ValueProvenance::Utterance,
                    }),
                    Ok(None) => TargetResolution::Missing("collection_boolean_not_resolved".into()),
                    Err(()) => TargetResolution::Ambiguous("collection_boolean_ambiguous".into()),
                },
                SlotKind::Number => entity_target(name, "number", entities),
                SlotKind::Entity(kind) => entity_target(name, kind.as_str(), entities),
                SlotKind::Reference(kind) => {
                    reference_slot_target(name, kind, normalized, references, profile)
                }
            }
        }
        MissingRequiredValue::Reference { kind } => {
            let matched = super::resolver::match_references(normalized, references, kind, profile);
            match matched.as_slice() {
                [] => TargetResolution::Missing("collection_reference_not_resolved".into()),
                [reference] => TargetResolution::Reference((*reference).clone()),
                _ => TargetResolution::Ambiguous("collection_reference_ambiguous".into()),
            }
        }
    }
}

fn boolean_target(normalized: &str, profile: &super::SemanticProfile) -> Result<Option<bool>, ()> {
    let input_tokens = super::normalization::ordered_tokens(normalized);
    let mut values = BTreeSet::new();
    for (phrase, value) in &profile.boolean_values {
        let phrase_tokens = profile.normalize_colloquial_tokens(
            &super::normalization::ordered_tokens(&profile.normalize_text(phrase)),
        );
        if !phrase_tokens.is_empty()
            && input_tokens
                .windows(phrase_tokens.len())
                .any(|window| window == phrase_tokens.as_slice())
        {
            values.insert(*value);
        }
    }
    match values.len() {
        0 => Ok(None),
        1 => Ok(values.into_iter().next()),
        _ => Err(()),
    }
}

fn entity_target(name: &str, kind: &str, entities: &[super::SemanticEntity]) -> TargetResolution {
    let matching = entities
        .iter()
        .filter(|entity| entity.kind.as_str() == kind)
        .collect::<Vec<_>>();
    let unique = matching
        .iter()
        .map(|entity| format!("{:?}", entity.value))
        .collect::<BTreeSet<_>>();
    if unique.len() > 1 {
        return TargetResolution::Ambiguous(format!("collection_slot_ambiguous:{name}"));
    }
    match matching.first() {
        Some(entity) => TargetResolution::Slot(SlotValue {
            name: name.to_string(),
            value: entity.value.clone(),
            provenance: ValueProvenance::Utterance,
        }),
        None => TargetResolution::Missing(format!("collection_slot_not_resolved:{name}")),
    }
}

fn reference_slot_target(
    name: &str,
    kind: &gvya_model::ReferenceKind,
    normalized: &str,
    references: &[crate::ResolverReferenceCandidate],
    profile: &super::SemanticProfile,
) -> TargetResolution {
    let matched = super::resolver::match_references(normalized, references, kind, profile);
    match matched.as_slice() {
        [] => TargetResolution::Missing(format!("collection_slot_not_resolved:{name}")),
        [reference] => TargetResolution::Slot(SlotValue {
            name: name.to_string(),
            value: Value::String(reference.id.as_str().to_string()),
            provenance: ValueProvenance::HostReferenceResolver,
        }),
        _ => TargetResolution::Ambiguous(format!("collection_slot_ambiguous:{name}")),
    }
}

/// Host reference authority during collection is exactly what this ResolverRequest exposed.
fn reference_slot_is_exposed(
    value: &Value,
    kind: &gvya_model::ReferenceKind,
    request: &ResolverRequest,
) -> bool {
    let Value::String(id) = value else {
        return false;
    };
    request.exposes_reference(kind, id)
}

/// Deterministic staleness check for an already-bound reference slot. This is ordinary state
/// validation against everything the host made visible, not the narrower resolver projection.
fn bound_reference_slot_is_visible(
    value: &Value,
    kind: &gvya_model::ReferenceKind,
    input: &SemanticInput,
) -> bool {
    let Value::String(id) = value else {
        return false;
    };
    input
        .reference_candidates
        .iter()
        .any(|candidate| candidate.reference.kind == *kind && candidate.reference.id.as_str() == id)
}

fn target_class(pattern: &super::MeaningPattern, target: &MissingRequiredValue) -> Option<String> {
    match target {
        MissingRequiredValue::Slot { name } => pattern
            .slots
            .iter()
            .find(|slot| slot.name == *name)
            .map(|slot| format!("slot:{:?}", slot.kind)),
        MissingRequiredValue::Reference { kind } => Some(format!("reference:{}", kind.as_str())),
    }
}

fn invalid_state(reason: &str) -> CollectionTurnDecision {
    CollectionTurnDecision::Invalid {
        reason_code: format!("collection_state_{reason}"),
    }
}
