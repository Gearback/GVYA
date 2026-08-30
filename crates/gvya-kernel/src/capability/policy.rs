//! Deterministic capability policy and confirmation admission.

use std::collections::BTreeMap;

use gvya_model::{
    CapabilityId, ConfirmationHint, ContextSnapshot, Formality, GvyaState, PolicyId, Value,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AdmissionNamespace {
    Arguments,
    Context,
    Author,
    Conversation,
    System,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PredicateOp {
    Exists,
    Missing,
    Equal,
    NotEqual,
    Greater,
    GreaterOrEqual,
    Less,
    LessOrEqual,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AdmissionPredicate {
    pub namespace: AdmissionNamespace,
    pub path: String,
    pub op: PredicateOp,
    pub value: Option<Value>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PolicyEffect {
    Allow,
    RequireConfirmation { reason_code: String },
    Deny { reason_code: String },
}

impl PolicyEffect {
    fn severity(&self) -> u8 {
        match self {
            Self::Allow => 0,
            Self::RequireConfirmation { .. } => 1,
            Self::Deny { .. } => 2,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct CapabilityPolicyRule {
    pub id: PolicyId,
    pub capability: CapabilityId,
    pub priority: i32,
    pub conditions: Vec<AdmissionPredicate>,
    pub effect: PolicyEffect,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PolicyDecision {
    Allow {
        policy: Option<PolicyId>,
    },
    NeedsConfirmation {
        reason_code: String,
        policy: Option<PolicyId>,
    },
    Reject {
        reason_code: String,
        policy: Option<PolicyId>,
    },
}

pub struct PolicyContext<'a> {
    pub arguments: &'a BTreeMap<String, Value>,
    pub context: &'a ContextSnapshot,
    pub state: &'a GvyaState,
    pub system: &'a BTreeMap<String, Value>,
}

#[must_use]
pub fn evaluate_policy(
    capability: &CapabilityId,
    confirmation_hint: ConfirmationHint,
    rules: &[CapabilityPolicyRule],
    ctx: &PolicyContext<'_>,
) -> PolicyDecision {
    let mut matching: Vec<&CapabilityPolicyRule> = rules
        .iter()
        .filter(|rule| {
            rule.capability == *capability
                && predicates_match_with_conversation(&rule.conditions, ctx)
        })
        .collect();
    matching.sort_by(|left, right| {
        right
            .priority
            .cmp(&left.priority)
            .then_with(|| right.effect.severity().cmp(&left.effect.severity()))
            .then_with(|| left.id.as_str().cmp(right.id.as_str()))
    });

    if let Some(rule) = matching.first() {
        return match &rule.effect {
            PolicyEffect::Allow => PolicyDecision::Allow {
                policy: Some(rule.id.clone()),
            },
            PolicyEffect::RequireConfirmation { reason_code } => {
                PolicyDecision::NeedsConfirmation {
                    reason_code: reason_code.clone(),
                    policy: Some(rule.id.clone()),
                }
            }
            PolicyEffect::Deny { reason_code } => PolicyDecision::Reject {
                reason_code: reason_code.clone(),
                policy: Some(rule.id.clone()),
            },
        };
    }

    match confirmation_hint {
        ConfirmationHint::Never => PolicyDecision::Allow { policy: None },
        ConfirmationHint::Always => PolicyDecision::NeedsConfirmation {
            reason_code: "confirmation_required_by_contract".into(),
            policy: None,
        },
        ConfirmationHint::Conditional => PolicyDecision::Reject {
            reason_code: "conditional_policy_unresolved".into(),
            policy: None,
        },
    }
}

#[must_use]
pub fn predicate_matches(predicate: &AdmissionPredicate, ctx: &PolicyContext<'_>) -> bool {
    let actual = resolve_path(predicate.namespace, &predicate.path, ctx);
    match predicate.op {
        PredicateOp::Exists => actual.is_some(),
        PredicateOp::Missing => actual.is_none(),
        PredicateOp::Equal => actual
            .zip(predicate.value.as_ref())
            .is_some_and(|(actual, expected)| scalar_equal(actual, expected)),
        PredicateOp::NotEqual => match actual.zip(predicate.value.as_ref()) {
            Some((actual, expected)) => !scalar_equal(actual, expected),
            None => true,
        },
        PredicateOp::Greater => compare_numbers(actual, predicate.value.as_ref(), |a, b| a > b),
        PredicateOp::GreaterOrEqual => {
            compare_numbers(actual, predicate.value.as_ref(), |a, b| a >= b)
        }
        PredicateOp::Less => compare_numbers(actual, predicate.value.as_ref(), |a, b| a < b),
        PredicateOp::LessOrEqual => {
            compare_numbers(actual, predicate.value.as_ref(), |a, b| a <= b)
        }
    }
}

fn resolve_path<'a>(
    namespace: AdmissionNamespace,
    path: &str,
    ctx: &'a PolicyContext<'_>,
) -> Option<&'a Value> {
    match namespace {
        AdmissionNamespace::Arguments => map_path_get(ctx.arguments, path),
        AdmissionNamespace::Context => map_path_get(&ctx.context.values, path),
        AdmissionNamespace::Author => map_path_get(&ctx.state.author, path),
        AdmissionNamespace::System => map_path_get(ctx.system, path),
        AdmissionNamespace::Conversation => None,
    }
}

/// Conversation predicates are resolved as scalars without converting the runtime state into an
/// untyped bag. This keeps writable conversation authority separate from policy inspection.
#[must_use]
pub fn conversation_scalar(state: &GvyaState, path: &str) -> Option<Value> {
    match path {
        "active_topic.id" => state
            .conversation
            .active_topic
            .as_ref()
            .map(|value| Value::String(value.id.as_str().to_owned())),
        "active_followup.id" => state
            .conversation
            .active_followup
            .as_ref()
            .map(|value| Value::String(value.id.as_str().to_owned())),
        "last_meaning" => state
            .conversation
            .last_meaning
            .as_ref()
            .map(|value| Value::String(value.as_str().to_owned())),
        "last_behavior" => state
            .conversation
            .last_behavior
            .as_ref()
            .map(|value| Value::String(value.as_str().to_owned())),
        "last_topic" => state
            .conversation
            .last_topic
            .as_ref()
            .map(|value| Value::String(value.as_str().to_owned())),
        "repeat.same_input_count" => Some(Value::Number(f64::from(
            state.conversation.repeat_memory.same_input_count,
        ))),
        "repeat.same_meaning_count" => Some(Value::Number(f64::from(
            state.conversation.repeat_memory.same_meaning_count,
        ))),
        "repair.consecutive" => Some(Value::Number(f64::from(
            state.conversation.repair.consecutive,
        ))),
        "turn_index" => Some(Value::Number(state.conversation.turn_index as f64)),
        "focus.count" => Some(Value::Number(state.conversation.focus.len() as f64)),
        "user_style.formality" => Some(Value::String(
            match state.conversation.user_style.formality {
                Formality::Unknown => "unknown",
                Formality::Formal => "formal",
                Formality::Informal => "informal",
            }
            .into(),
        )),
        _ => None,
    }
}

#[must_use]
pub fn predicate_matches_with_conversation(
    predicate: &AdmissionPredicate,
    ctx: &PolicyContext<'_>,
) -> bool {
    if predicate.namespace != AdmissionNamespace::Conversation {
        return predicate_matches(predicate, ctx);
    }
    let owned = conversation_scalar(ctx.state, &predicate.path);
    match predicate.op {
        PredicateOp::Exists => owned.is_some(),
        PredicateOp::Missing => owned.is_none(),
        PredicateOp::Equal => owned
            .as_ref()
            .zip(predicate.value.as_ref())
            .is_some_and(|(actual, expected)| scalar_equal(actual, expected)),
        PredicateOp::NotEqual => match owned.as_ref().zip(predicate.value.as_ref()) {
            Some((actual, expected)) => !scalar_equal(actual, expected),
            None => true,
        },
        PredicateOp::Greater => {
            compare_numbers(owned.as_ref(), predicate.value.as_ref(), |a, b| a > b)
        }
        PredicateOp::GreaterOrEqual => {
            compare_numbers(owned.as_ref(), predicate.value.as_ref(), |a, b| a >= b)
        }
        PredicateOp::Less => {
            compare_numbers(owned.as_ref(), predicate.value.as_ref(), |a, b| a < b)
        }
        PredicateOp::LessOrEqual => {
            compare_numbers(owned.as_ref(), predicate.value.as_ref(), |a, b| a <= b)
        }
    }
}

#[must_use]
pub fn predicates_match_with_conversation(
    predicates: &[AdmissionPredicate],
    ctx: &PolicyContext<'_>,
) -> bool {
    predicates
        .iter()
        .all(|predicate| predicate_matches_with_conversation(predicate, ctx))
}

fn map_path_get<'a>(root: &'a BTreeMap<String, Value>, path: &str) -> Option<&'a Value> {
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

fn compare_numbers(
    actual: Option<&Value>,
    expected: Option<&Value>,
    compare: impl Fn(f64, f64) -> bool,
) -> bool {
    match (actual, expected) {
        (Some(Value::Number(left)), Some(Value::Number(right)))
            if left.is_finite() && right.is_finite() =>
        {
            compare(*left, *right)
        }
        _ => false,
    }
}

fn scalar_equal(left: &Value, right: &Value) -> bool {
    match (left, right) {
        (Value::Null, Value::Null) => true,
        (Value::Bool(a), Value::Bool(b)) => a == b,
        (Value::Number(a), Value::Number(b)) => a.is_finite() && b.is_finite() && a == b,
        (Value::String(a), Value::String(b)) => a == b,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gvya_model::{CapabilityId, ConfirmationHint, ContextSnapshot, PolicyId};

    fn context<'a>(
        arguments: &'a BTreeMap<String, Value>,
        state: &'a GvyaState,
        snapshot: &'a ContextSnapshot,
    ) -> PolicyContext<'a> {
        static EMPTY: std::sync::OnceLock<BTreeMap<String, Value>> = std::sync::OnceLock::new();
        PolicyContext {
            arguments,
            context: snapshot,
            state,
            system: EMPTY.get_or_init(BTreeMap::new),
        }
    }

    #[test]
    fn deny_wins_at_equal_priority() {
        let capability = CapabilityId::new("door.open");
        let rules = vec![
            CapabilityPolicyRule {
                id: PolicyId::new("allow"),
                capability: capability.clone(),
                priority: 10,
                conditions: Vec::new(),
                effect: PolicyEffect::Allow,
            },
            CapabilityPolicyRule {
                id: PolicyId::new("deny"),
                capability: capability.clone(),
                priority: 10,
                conditions: Vec::new(),
                effect: PolicyEffect::Deny {
                    reason_code: "locked".into(),
                },
            },
        ];
        let args = BTreeMap::new();
        let state = GvyaState::default();
        let snapshot = ContextSnapshot {
            values: BTreeMap::new(),
            visible_references: Vec::new(),
            available_capabilities: Vec::new(),
        };
        let decision = evaluate_policy(
            &capability,
            ConfirmationHint::Never,
            &rules,
            &context(&args, &state, &snapshot),
        );
        assert!(matches!(decision, PolicyDecision::Reject { .. }));
    }

    #[test]
    fn conditional_without_matching_rule_fails_closed() {
        let capability = CapabilityId::new("money.send");
        let args = BTreeMap::new();
        let state = GvyaState::default();
        let snapshot = ContextSnapshot {
            values: BTreeMap::new(),
            visible_references: Vec::new(),
            available_capabilities: Vec::new(),
        };
        assert!(matches!(
            evaluate_policy(&capability, ConfirmationHint::Conditional, &[], &context(&args, &state, &snapshot)),
            PolicyDecision::Reject { reason_code, .. } if reason_code == "conditional_policy_unresolved"
        ));
    }
}
