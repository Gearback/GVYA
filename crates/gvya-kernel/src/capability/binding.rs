//! Deterministic binding from conversation meaning/response state into typed capability arguments.

use std::collections::BTreeMap;

use gvya_model::{
    BehaviorId, CapabilityBindingId, CapabilityId, ContextSnapshot, GvyaState, HostReference,
    Meaning, ReferenceKind, ResponseId, Value,
};

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CapabilityTrigger {
    pub meaning: Option<gvya_model::MeaningId>,
    pub behavior: Option<BehaviorId>,
    pub response: Option<ResponseId>,
}

impl CapabilityTrigger {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.meaning.is_none() && self.behavior.is_none() && self.response.is_none()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReferenceProjection {
    Id,
    Object,
}

#[derive(Clone, Debug, PartialEq)]
pub enum BindingSource {
    MeaningSlot(String),
    MeaningReference {
        kind: ReferenceKind,
        projection: ReferenceProjection,
    },
    FocusReference {
        kind: ReferenceKind,
        projection: ReferenceProjection,
    },
    ContextPath(String),
    AuthorStatePath(String),
    Literal(Value),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArgumentPath(Vec<String>);

impl ArgumentPath {
    #[must_use]
    pub fn new(parts: impl IntoIterator<Item = impl Into<String>>) -> Self {
        Self(parts.into_iter().map(Into::into).collect())
    }

    #[must_use]
    pub fn from_dotted(path: &str) -> Option<Self> {
        let parts: Vec<String> = path
            .split('.')
            .map(str::trim)
            .filter(|part| !part.is_empty())
            .map(ToOwned::to_owned)
            .collect();
        if parts.is_empty() || parts.iter().any(|part| !valid_segment(part)) {
            return None;
        }
        Some(Self(parts))
    }

    #[must_use]
    pub fn parts(&self) -> &[String] {
        &self.0
    }

    #[must_use]
    pub fn display(&self) -> String {
        self.0.join(".")
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ArgumentBinding {
    pub target: ArgumentPath,
    pub source: BindingSource,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CapabilityBindingRule {
    pub id: CapabilityBindingId,
    pub trigger: CapabilityTrigger,
    pub capability: CapabilityId,
    pub arguments: Vec<ArgumentBinding>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BindingIssue {
    pub code: String,
    pub target: Option<String>,
    pub message: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct BindingOutput {
    pub arguments: BTreeMap<String, Value>,
    pub issues: Vec<BindingIssue>,
}

#[derive(Clone, Debug)]
pub struct BindingContext<'a> {
    pub meaning: Option<&'a Meaning>,
    pub context: &'a ContextSnapshot,
    pub state: &'a GvyaState,
}

#[must_use]
pub fn bind_arguments(rule: &CapabilityBindingRule, ctx: &BindingContext<'_>) -> BindingOutput {
    let mut root = Value::Object(BTreeMap::new());
    let mut issues = Vec::new();

    for binding in &rule.arguments {
        let target = binding.target.display();
        match resolve_source(&binding.source, ctx) {
            Ok(Some(value)) => {
                if let Err(code) = set_nested(&mut root, binding.target.parts(), value) {
                    issues.push(BindingIssue {
                        code: code.to_owned(),
                        target: Some(target),
                        message: "binding target conflicts with another bound value".into(),
                    });
                }
            }
            Ok(None) => {
                // Missing optional values are omitted. Final schema validation decides whether the
                // property was required by the capability contract.
            }
            Err((code, message)) => issues.push(BindingIssue {
                code: code.to_owned(),
                target: Some(target),
                message,
            }),
        }
    }

    let arguments = match root {
        Value::Object(map) => map,
        _ => BTreeMap::new(),
    };
    BindingOutput { arguments, issues }
}

#[must_use]
pub fn trigger_matches(
    trigger: &CapabilityTrigger,
    meaning: Option<&Meaning>,
    behavior: Option<&BehaviorId>,
    response: Option<&ResponseId>,
) -> bool {
    if let Some(expected) = &trigger.meaning {
        if meaning.map(|value| &value.id) != Some(expected) {
            return false;
        }
    }
    if let Some(expected) = &trigger.behavior {
        if behavior != Some(expected) {
            return false;
        }
    }
    if let Some(expected) = &trigger.response {
        if response != Some(expected) {
            return false;
        }
    }
    !trigger.is_empty()
}

fn resolve_source(
    source: &BindingSource,
    ctx: &BindingContext<'_>,
) -> Result<Option<Value>, (&'static str, String)> {
    match source {
        BindingSource::MeaningSlot(name) => {
            let Some(meaning) = ctx.meaning else {
                return Ok(None);
            };
            let matches: Vec<&Value> = meaning
                .slots
                .iter()
                .filter(|slot| slot.name == *name)
                .map(|slot| &slot.value)
                .collect();
            match matches.as_slice() {
                [] => Ok(None),
                [value] => Ok(Some((*value).clone())),
                _ => Err((
                    "slot_ambiguous",
                    format!("meaning contains multiple values for slot {name}"),
                )),
            }
        }
        BindingSource::MeaningReference { kind, projection } => {
            let Some(meaning) = ctx.meaning else {
                return Ok(None);
            };
            unique_visible_reference(
                meaning
                    .references
                    .iter()
                    .filter(|reference| reference.kind == *kind),
                &ctx.context.visible_references,
                projection,
                "meaning_reference_ambiguous",
            )
        }
        BindingSource::FocusReference { kind, projection } => unique_visible_reference(
            ctx.state
                .conversation
                .focus
                .iter()
                .filter(|reference| reference.kind == *kind),
            &ctx.context.visible_references,
            projection,
            "focus_reference_ambiguous",
        ),
        BindingSource::ContextPath(path) => Ok(path_get(&ctx.context.values, path).cloned()),
        BindingSource::AuthorStatePath(path) => Ok(path_get(&ctx.state.author, path).cloned()),
        BindingSource::Literal(value) => Ok(Some(value.clone())),
    }
}

fn unique_visible_reference<'a>(
    references: impl Iterator<Item = &'a HostReference>,
    visible: &[HostReference],
    projection: &ReferenceProjection,
    ambiguity_code: &'static str,
) -> Result<Option<Value>, (&'static str, String)> {
    let refs: Vec<&HostReference> = references.collect();
    match refs.as_slice() {
        [] => Ok(None),
        [reference] => {
            if !visible.iter().any(|candidate| candidate == *reference) {
                return Err((
                    "reference_not_visible",
                    "host reference is not visible in the current context snapshot".into(),
                ));
            }
            Ok(Some(reference_value(reference, *projection)))
        }
        _ => Err((
            ambiguity_code,
            "more than one eligible host reference is in scope".into(),
        )),
    }
}

fn reference_value(reference: &HostReference, projection: ReferenceProjection) -> Value {
    match projection {
        ReferenceProjection::Id => Value::String(reference.id.as_str().to_owned()),
        ReferenceProjection::Object => Value::Object(BTreeMap::from([
            (
                "kind".into(),
                Value::String(reference.kind.as_str().to_owned()),
            ),
            ("id".into(), Value::String(reference.id.as_str().to_owned())),
        ])),
    }
}

fn path_get<'a>(root: &'a BTreeMap<String, Value>, path: &str) -> Option<&'a Value> {
    let mut parts = path.split('.').filter(|part| !part.is_empty());
    let first = parts.next()?;
    let mut value = root.get(first)?;
    for part in parts {
        match value {
            Value::Object(map) => value = map.get(part)?,
            _ => return None,
        }
    }
    Some(value)
}

fn set_nested(root: &mut Value, parts: &[String], value: Value) -> Result<(), &'static str> {
    if parts.is_empty() {
        return Err("binding_target_empty");
    }
    let mut node = root;
    for part in &parts[..parts.len() - 1] {
        let Value::Object(map) = node else {
            return Err("binding_target_conflict");
        };
        node = map
            .entry(part.clone())
            .or_insert_with(|| Value::Object(BTreeMap::new()));
        if !matches!(node, Value::Object(_)) {
            return Err("binding_target_conflict");
        }
    }
    let Value::Object(map) = node else {
        return Err("binding_target_conflict");
    };
    let last = &parts[parts.len() - 1];
    if map.contains_key(last) {
        return Err("binding_target_duplicate");
    }
    map.insert(last.clone(), value);
    Ok(())
}

fn valid_segment(segment: &str) -> bool {
    let mut chars = segment.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    (first.is_ascii_alphabetic() || first == '_')
        && chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '-')
}

#[cfg(test)]
mod tests {
    use super::*;
    use gvya_model::{
        ContextSnapshot, GvyaState, HostReference, Meaning, MeaningId, ReferenceId, SlotValue,
        ValueProvenance,
    };

    fn empty_context() -> ContextSnapshot {
        ContextSnapshot {
            values: BTreeMap::new(),
            visible_references: Vec::new(),
            available_capabilities: Vec::new(),
        }
    }

    #[test]
    fn meaning_slot_binds_nested_argument() {
        let rule = CapabilityBindingRule {
            id: CapabilityBindingId::new("thermostat"),
            trigger: CapabilityTrigger {
                meaning: Some(MeaningId::new("temperature.set")),
                ..CapabilityTrigger::default()
            },
            capability: CapabilityId::new("thermostat.set"),
            arguments: vec![ArgumentBinding {
                target: ArgumentPath::from_dotted("settings.temperature").unwrap(),
                source: BindingSource::MeaningSlot("temperature".into()),
            }],
        };
        let meaning = Meaning {
            id: MeaningId::new("temperature.set"),
            slots: vec![SlotValue {
                name: "temperature".into(),
                value: Value::Number(22.0),
                provenance: ValueProvenance::Utterance,
            }],
            references: Vec::new(),
        };
        let state = GvyaState::default();
        let context = empty_context();
        let output = bind_arguments(
            &rule,
            &BindingContext {
                meaning: Some(&meaning),
                context: &context,
                state: &state,
            },
        );
        assert!(output.issues.is_empty());
        assert!(matches!(
            output.arguments.get("settings"),
            Some(Value::Object(_))
        ));
    }

    #[test]
    fn duplicate_reference_kind_is_ambiguous() {
        let kind = ReferenceKind::new("room");
        let meaning = Meaning {
            id: MeaningId::new("light.off"),
            slots: Vec::new(),
            references: vec![
                HostReference {
                    kind: kind.clone(),
                    id: ReferenceId::new("a"),
                },
                HostReference {
                    kind: kind.clone(),
                    id: ReferenceId::new("b"),
                },
            ],
        };
        let rule = CapabilityBindingRule {
            id: CapabilityBindingId::new("lights"),
            trigger: CapabilityTrigger {
                meaning: Some(MeaningId::new("light.off")),
                ..CapabilityTrigger::default()
            },
            capability: CapabilityId::new("light.off"),
            arguments: vec![ArgumentBinding {
                target: ArgumentPath::from_dotted("room").unwrap(),
                source: BindingSource::MeaningReference {
                    kind,
                    projection: ReferenceProjection::Id,
                },
            }],
        };
        let state = GvyaState::default();
        let context = empty_context();
        let output = bind_arguments(
            &rule,
            &BindingContext {
                meaning: Some(&meaning),
                context: &context,
                state: &state,
            },
        );
        assert_eq!(output.issues[0].code, "meaning_reference_ambiguous");
    }

    #[test]
    fn stale_focus_reference_is_rejected_when_host_no_longer_exposes_it() {
        let kind = ReferenceKind::new("door");
        let stale = HostReference {
            kind: kind.clone(),
            id: ReferenceId::new("maintenance"),
        };
        let rule = CapabilityBindingRule {
            id: CapabilityBindingId::new("door.open"),
            trigger: CapabilityTrigger {
                meaning: Some(MeaningId::new("door.open")),
                ..CapabilityTrigger::default()
            },
            capability: CapabilityId::new("door.open"),
            arguments: vec![ArgumentBinding {
                target: ArgumentPath::from_dotted("door").unwrap(),
                source: BindingSource::FocusReference {
                    kind,
                    projection: ReferenceProjection::Id,
                },
            }],
        };
        let meaning = Meaning {
            id: MeaningId::new("door.open"),
            slots: Vec::new(),
            references: Vec::new(),
        };
        let mut state = GvyaState::default();
        state.conversation.focus.push(stale);
        let context = empty_context();
        let output = bind_arguments(
            &rule,
            &BindingContext {
                meaning: Some(&meaning),
                context: &context,
                state: &state,
            },
        );
        assert_eq!(output.issues.len(), 1);
        assert_eq!(output.issues[0].code, "reference_not_visible");
    }
}
