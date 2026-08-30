//! Response eligibility and conversation-side author-state effects.

use std::collections::BTreeMap;

use gvya_model::{ConversationState, Meaning, Value};

use super::catalog::{
    ConversationEffect, PredicateOp, StateNamespace, StateTarget, ValueCondition, ValuePath,
    ValueRequirement,
};
use super::state::AuthorNumberDefinition;

pub const AUTHOR_STATE_MAX_TOP_LEVEL: usize = 1024;
pub const AUTHOR_STATE_MAX_DEPTH: usize = 16;
pub const AUTHOR_STATE_MAX_NODES: usize = 8192;
pub const AUTHOR_STATE_MAX_STRING_BYTES: usize = 64 * 1024;

#[derive(Clone, Debug, PartialEq)]
pub struct ConditionContext<'a> {
    pub author: &'a BTreeMap<String, Value>,
    pub conversation: &'a ConversationState,
    pub host: &'a BTreeMap<String, Value>,
    pub meaning: Option<&'a Meaning>,
    pub system: &'a BTreeMap<String, Value>,
    pub interaction: Option<&'a BTreeMap<String, Value>>,
}

#[must_use]
pub fn conditions_match(conditions: &[ValueCondition], context: &ConditionContext<'_>) -> bool {
    conditions
        .iter()
        .all(|condition| condition_matches(condition, context))
}

#[must_use]
pub fn value_requirement_matches(
    requirement: &ValueRequirement,
    context: &ConditionContext<'_>,
) -> bool {
    resolve_value(&requirement.path, context)
        .as_ref()
        .is_some_and(|actual| scalar_equal(actual, &requirement.value))
}

#[must_use]
pub fn condition_matches(condition: &ValueCondition, context: &ConditionContext<'_>) -> bool {
    let actual = resolve_value(&condition.path, context);
    match condition.op {
        PredicateOp::Exists => actual.is_some(),
        PredicateOp::Missing => actual.is_none(),
        PredicateOp::Equal => actual
            .as_ref()
            .zip(condition.value.as_ref())
            .is_some_and(|(left, right)| scalar_equal(left, right)),
        PredicateOp::NotEqual => actual
            .as_ref()
            .zip(condition.value.as_ref())
            .is_some_and(|(left, right)| !scalar_equal(left, right)),
        PredicateOp::Greater => compare_order(actual.as_ref(), condition.value.as_ref())
            .is_some_and(|ordering| ordering > 0),
        PredicateOp::GreaterOrEqual => compare_order(actual.as_ref(), condition.value.as_ref())
            .is_some_and(|ordering| ordering >= 0),
        PredicateOp::Less => compare_order(actual.as_ref(), condition.value.as_ref())
            .is_some_and(|ordering| ordering < 0),
        PredicateOp::LessOrEqual => compare_order(actual.as_ref(), condition.value.as_ref())
            .is_some_and(|ordering| ordering <= 0),
    }
}

#[must_use]
pub fn resolve_value(path: &ValuePath, context: &ConditionContext<'_>) -> Option<Value> {
    match path.namespace {
        StateNamespace::Author => path_get(context.author, &path.path).cloned(),
        StateNamespace::Context => path_get(context.host, &path.path).cloned(),
        StateNamespace::System => path_get(context.system, &path.path).cloned(),
        StateNamespace::Interaction => context
            .interaction
            .and_then(|values| path_get(values, &path.path))
            .cloned(),
        StateNamespace::Meaning => resolve_meaning_path(context.meaning, &path.path).cloned(),
        StateNamespace::Conversation => resolve_conversation_path(context.conversation, &path.path),
    }
}

#[must_use]
pub fn path_get<'a>(root: &'a BTreeMap<String, Value>, path: &str) -> Option<&'a Value> {
    let mut parts = path.split('.').filter(|part| !part.is_empty());
    let first = parts.next()?;
    let mut node = root.get(first)?;
    for part in parts {
        let Value::Object(object) = node else {
            return None;
        };
        node = object.get(part)?;
    }
    Some(node)
}

fn resolve_meaning_path<'a>(meaning: Option<&'a Meaning>, path: &str) -> Option<&'a Value> {
    let meaning = meaning?;
    let slot_name = path.strip_prefix("slots.")?;
    meaning
        .slots
        .iter()
        .find(|slot| slot.name == slot_name)
        .map(|slot| &slot.value)
}

fn resolve_conversation_path(state: &ConversationState, path: &str) -> Option<Value> {
    match path {
        "turnIndex" => Some(Value::Number(state.turn_index as f64)),
        "activeTopic" => Some(Value::String(
            state.active_topic.as_ref()?.id.as_str().to_string(),
        )),
        "topicTtl" => Some(Value::Number(f64::from(state.active_topic.as_ref()?.ttl))),
        "activeFollowup" => Some(Value::String(
            state.active_followup.as_ref()?.id.as_str().to_string(),
        )),
        "followupTtl" => Some(Value::Number(f64::from(
            state.active_followup.as_ref()?.ttl,
        ))),
        "lastMeaning" => Some(Value::String(
            state.last_meaning.as_ref()?.as_str().to_string(),
        )),
        "lastBehavior" => Some(Value::String(
            state.last_behavior.as_ref()?.as_str().to_string(),
        )),
        "lastTopic" => Some(Value::String(
            state.last_topic.as_ref()?.as_str().to_string(),
        )),
        "userStyle.formality" => Some(Value::String(
            match state.user_style.formality {
                gvya_model::Formality::Unknown => "unknown",
                gvya_model::Formality::Formal => "formal",
                gvya_model::Formality::Informal => "informal",
            }
            .to_string(),
        )),
        "userStyle.confidence" => Some(Value::Number(state.user_style.confidence)),
        "repairCount" => Some(Value::Number(f64::from(state.repair.consecutive))),
        "sameInputCount" => Some(Value::Number(f64::from(
            state.repeat_memory.same_input_count,
        ))),
        "sameMeaningCount" => Some(Value::Number(f64::from(
            state.repeat_memory.same_meaning_count,
        ))),
        _ => None,
    }
}

#[must_use]
pub fn scalar_equal(left: &Value, right: &Value) -> bool {
    // Equality is deliberately type-strict. Authoring contracts that ask for coercion must use an
    // explicit predicate/operator instead of changing the meaning of `Equal` or value requirements.
    left == right
}

fn compare_order(left: Option<&Value>, right: Option<&Value>) -> Option<i8> {
    let (left, right) = (left?, right?);
    match (left, right) {
        (Value::Number(a), Value::Number(b)) => Some(if a < b {
            -1
        } else if a > b {
            1
        } else {
            0
        }),
        (Value::String(a), Value::String(b)) => Some(if a < b {
            -1
        } else if a > b {
            1
        } else {
            0
        }),
        _ => None,
    }
}

#[must_use]
pub fn to_number(value: &Value) -> Option<f64> {
    match value {
        Value::Number(number) => Some(*number),
        Value::Bool(boolean) => Some(if *boolean { 1.0 } else { 0.0 }),
        Value::String(text) => text.trim().parse::<f64>().ok(),
        _ => None,
    }
}

pub fn initialize_author_numbers(
    author: &mut BTreeMap<String, Value>,
    definitions: &[AuthorNumberDefinition],
) {
    for definition in definitions {
        let value = path_get(author, &definition.path)
            .and_then(to_number)
            .filter(|value| value.is_finite())
            .unwrap_or(definition.default)
            .clamp(definition.min, definition.max);
        let _ = path_set(author, &definition.path, Value::Number(value));
    }
}

pub fn apply_effects(
    author: &mut BTreeMap<String, Value>,
    effects: &[ConversationEffect],
    definitions: &[AuthorNumberDefinition],
) {
    let definition = |path: &str| definitions.iter().find(|row| row.path == path);
    for effect in effects {
        match effect {
            ConversationEffect::Assign {
                target: StateTarget::Author(path),
                value,
            } => {
                if let Some(bounds) = definition(path) {
                    if let Value::Number(number) = value {
                        if number.is_finite() {
                            let _ = path_set(
                                author,
                                path,
                                Value::Number(number.clamp(bounds.min, bounds.max)),
                            );
                        }
                    }
                } else if value_is_finite(value) {
                    let _ = path_set(author, path, value.clone());
                }
            }
            ConversationEffect::Increment {
                target: StateTarget::Author(path),
                delta,
            } => {
                if !delta.is_finite() {
                    continue;
                }
                if let Some(bounds) = definition(path) {
                    let current = path_get(author, path)
                        .and_then(to_number)
                        .filter(|v| v.is_finite())
                        .unwrap_or(bounds.default);
                    let next = current + delta;
                    if next.is_finite() {
                        let _ = path_set(
                            author,
                            path,
                            Value::Number(next.clamp(bounds.min, bounds.max)),
                        );
                    }
                } else {
                    let current = path_get(author, path).and_then(to_number).unwrap_or(0.0);
                    let next = current + delta;
                    if current.is_finite() && next.is_finite() {
                        let _ = path_set(author, path, Value::Number(next));
                    }
                }
            }
        }
    }
}

#[must_use]
pub fn value_is_finite(value: &Value) -> bool {
    match value {
        Value::Number(number) => number.is_finite(),
        Value::Array(values) => values.iter().all(value_is_finite),
        Value::Object(values) => values.values().all(value_is_finite),
        Value::Null | Value::Bool(_) | Value::String(_) => true,
    }
}

pub fn path_set(root: &mut BTreeMap<String, Value>, path: &str, value: Value) -> bool {
    let parts: Vec<_> = path.split('.').filter(|part| !part.is_empty()).collect();
    let mut value_nodes = 0usize;
    if parts.is_empty()
        || parts.len() > AUTHOR_STATE_MAX_DEPTH
        || !value_within_author_limits(&value, 1, &mut value_nodes)
    {
        return false;
    }
    let mut candidate = root.clone();
    path_set_parts(&mut candidate, &parts, value);
    if !author_state_within_limits(&candidate) {
        return false;
    }
    *root = candidate;
    true
}

#[must_use]
pub fn author_state_within_limits(root: &BTreeMap<String, Value>) -> bool {
    if root.len() > AUTHOR_STATE_MAX_TOP_LEVEL {
        return false;
    }
    let mut nodes = 0usize;
    root.iter().all(|(key, value)| {
        key.len() <= AUTHOR_STATE_MAX_STRING_BYTES
            && value_within_author_limits(value, 1, &mut nodes)
    })
}

fn value_within_author_limits(value: &Value, depth: usize, nodes: &mut usize) -> bool {
    if depth > AUTHOR_STATE_MAX_DEPTH {
        return false;
    }
    *nodes = nodes.saturating_add(1);
    if *nodes > AUTHOR_STATE_MAX_NODES {
        return false;
    }
    match value {
        Value::String(value) => value.len() <= AUTHOR_STATE_MAX_STRING_BYTES,
        Value::Array(values) => {
            values.len() <= AUTHOR_STATE_MAX_NODES
                && values
                    .iter()
                    .all(|value| value_within_author_limits(value, depth + 1, nodes))
        }
        Value::Object(values) => {
            values.len() <= AUTHOR_STATE_MAX_NODES
                && values.iter().all(|(key, value)| {
                    key.len() <= AUTHOR_STATE_MAX_STRING_BYTES
                        && value_within_author_limits(value, depth + 1, nodes)
                })
        }
        Value::Number(number) => number.is_finite(),
        Value::Null | Value::Bool(_) => true,
    }
}

fn path_set_parts(root: &mut BTreeMap<String, Value>, parts: &[&str], value: Value) {
    if parts.len() == 1 {
        root.insert(parts[0].to_string(), value);
        return;
    }
    let entry = root
        .entry(parts[0].to_string())
        .or_insert_with(|| Value::Object(BTreeMap::new()));
    if !matches!(entry, Value::Object(_)) {
        *entry = Value::Object(BTreeMap::new());
    }
    let Value::Object(child) = entry else {
        return;
    };
    path_set_parts(child, &parts[1..], value);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nested_author_assignment_is_clean_break_state_effect() {
        let mut state = BTreeMap::new();
        assert!(path_set(&mut state, "trust.level", Value::Number(3.0)));
        assert_eq!(path_get(&state, "trust.level"), Some(&Value::Number(3.0)));
    }

    #[test]
    fn oversized_author_state_write_is_rejected() {
        let mut state = BTreeMap::new();
        let too_deep = (0..=AUTHOR_STATE_MAX_DEPTH)
            .map(|index| format!("k{index}"))
            .collect::<Vec<_>>()
            .join(".");
        assert!(!path_set(&mut state, &too_deep, Value::Number(1.0)));
        assert!(state.is_empty());
        assert!(!path_set(
            &mut state,
            "large",
            Value::String("x".repeat(AUTHOR_STATE_MAX_STRING_BYTES + 1))
        ));
        assert!(state.is_empty());
    }

    #[test]
    fn non_finite_effects_never_enter_author_state() {
        let mut state = BTreeMap::from([("score".into(), Value::Number(4.0))]);
        apply_effects(
            &mut state,
            &[
                ConversationEffect::Increment {
                    target: StateTarget::Author("score".into()),
                    delta: f64::INFINITY,
                },
                ConversationEffect::Assign {
                    target: StateTarget::Author("bad".into()),
                    value: Value::Number(f64::NAN),
                },
            ],
            &[],
        );
        assert_eq!(path_get(&state, "score"), Some(&Value::Number(4.0)));
        assert_eq!(path_get(&state, "bad"), None);
    }
    #[test]
    fn declared_author_number_defaults_and_clamps_effects() {
        let definitions = vec![AuthorNumberDefinition {
            path: "trust".into(),
            default: 5.0,
            min: 0.0,
            max: 10.0,
        }];
        let mut state = BTreeMap::new();
        initialize_author_numbers(&mut state, &definitions);
        assert_eq!(path_get(&state, "trust"), Some(&Value::Number(5.0)));
        apply_effects(
            &mut state,
            &[ConversationEffect::Increment {
                target: StateTarget::Author("trust".into()),
                delta: 20.0,
            }],
            &definitions,
        );
        assert_eq!(path_get(&state, "trust"), Some(&Value::Number(10.0)));
        apply_effects(
            &mut state,
            &[ConversationEffect::Assign {
                target: StateTarget::Author("trust".into()),
                value: Value::Number(-5.0),
            }],
            &definitions,
        );
        assert_eq!(path_get(&state, "trust"), Some(&Value::Number(0.0)));
    }
}
