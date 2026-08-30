//! Strict primitive/configuration decoders shared by source domains.

use super::*;

pub(super) fn strict_raw_string_array(
    value: Option<&JsonValue>,
    path: &str,
    issues: &mut Vec<SourceIssue>,
) -> Vec<String> {
    let Some(rows) = optional_array(value, path, issues) else {
        return Vec::new();
    };
    rows.iter()
        .enumerate()
        .filter_map(|(index, value)| match value.as_str() {
            Some(value) => Some(value.to_owned()),
            None => {
                issues.push(issue(
                    "source.string_array",
                    "array item must be a string",
                    Some(&format!("{path}[{index}]")),
                ));
                None
            }
        })
        .collect()
}

pub(super) fn optional_source_string(
    obj: &serde_json::Map<String, JsonValue>,
    key: &str,
    path: &str,
    limits: SourceLimits,
    issues: &mut Vec<SourceIssue>,
) -> Option<String> {
    match obj.get(key) {
        None | Some(JsonValue::Null) => None,
        Some(_) => required_string(obj, key, path, limits, issues),
    }
}

pub(super) fn optional_raw_source_string(
    obj: &serde_json::Map<String, JsonValue>,
    key: &str,
    path: &str,
    issues: &mut Vec<SourceIssue>,
) -> Option<String> {
    match obj.get(key) {
        None | Some(JsonValue::Null) => None,
        Some(_) => required_raw_string(obj, key, path, issues),
    }
}

pub(super) fn optional_source_id<T, F>(
    obj: &serde_json::Map<String, JsonValue>,
    key: &str,
    constructor: F,
    path: &str,
    limits: SourceLimits,
    issues: &mut Vec<SourceIssue>,
) -> Option<T>
where
    F: Fn(String) -> T,
{
    optional_source_string(obj, key, path, limits, issues).map(constructor)
}

pub(super) fn optional_u64_strict(
    obj: &serde_json::Map<String, JsonValue>,
    key: &str,
    path: &str,
    issues: &mut Vec<SourceIssue>,
) -> Option<u64> {
    match obj.get(key) {
        None | Some(JsonValue::Null) => None,
        Some(value) => match value.as_u64() {
            Some(value) => Some(value),
            None => {
                issues.push(issue(
                    "source.u64",
                    &format!("{key} must be an unsigned integer"),
                    Some(path),
                ));
                None
            }
        },
    }
}

pub(super) fn optional_u32_strict(
    obj: &serde_json::Map<String, JsonValue>,
    key: &str,
    path: &str,
    issues: &mut Vec<SourceIssue>,
) -> Option<u32> {
    optional_u64_strict(obj, key, path, issues).and_then(|value| match u32::try_from(value) {
        Ok(value) => Some(value),
        Err(_) => {
            issues.push(issue(
                "source.u32",
                &format!("{key} is out of range"),
                Some(path),
            ));
            None
        }
    })
}

pub(super) fn required_u32_strict(
    obj: &serde_json::Map<String, JsonValue>,
    key: &str,
    path: &str,
    issues: &mut Vec<SourceIssue>,
) -> Option<u32> {
    if !obj.contains_key(key) {
        issues.push(issue(
            "source.field_required",
            &format!("{key} is required"),
            Some(path),
        ));
        return None;
    }
    optional_u32_strict(obj, key, path, issues).or_else(|| {
        if obj.get(key).is_some_and(JsonValue::is_null) {
            issues.push(issue(
                "source.u32_required",
                &format!("{key} must be an unsigned integer"),
                Some(path),
            ));
        }
        None
    })
}

pub(super) fn optional_i64_strict(
    obj: &serde_json::Map<String, JsonValue>,
    key: &str,
    path: &str,
    issues: &mut Vec<SourceIssue>,
) -> Option<i64> {
    match obj.get(key) {
        None | Some(JsonValue::Null) => None,
        Some(value) => match value.as_i64() {
            Some(value) => Some(value),
            None => {
                issues.push(issue(
                    "source.i64",
                    &format!("{key} must be a signed integer"),
                    Some(path),
                ));
                None
            }
        },
    }
}

pub(super) fn optional_f64_strict(
    obj: &serde_json::Map<String, JsonValue>,
    key: &str,
    path: &str,
    issues: &mut Vec<SourceIssue>,
) -> Option<f64> {
    match obj.get(key) {
        None | Some(JsonValue::Null) => None,
        Some(value) => match value.as_f64().filter(|value| value.is_finite()) {
            Some(value) => Some(value),
            None => {
                issues.push(issue(
                    "source.number",
                    &format!("{key} must be a finite number"),
                    Some(path),
                ));
                None
            }
        },
    }
}

pub(super) fn parse_model_value(
    value: &JsonValue,
    path: &str,
    issues: &mut Vec<SourceIssue>,
) -> Option<Value> {
    match value {
        JsonValue::Null => Some(Value::Null),
        JsonValue::Bool(v) => Some(Value::Bool(*v)),
        JsonValue::Number(v) => v
            .as_f64()
            .filter(|v| v.is_finite())
            .map(Value::Number)
            .or_else(|| {
                issues.push(issue("source.number", "number must be finite", Some(path)));
                None
            }),
        JsonValue::String(v) => Some(Value::String(v.clone())),
        JsonValue::Array(rows) => Some(Value::Array(
            rows.iter()
                .filter_map(|v| parse_model_value(v, path, issues))
                .collect(),
        )),
        JsonValue::Object(map) => Some(Value::Object(
            map.iter()
                .filter_map(|(k, v)| parse_model_value(v, path, issues).map(|v| (k.clone(), v)))
                .collect(),
        )),
    }
}

pub(super) fn parse_dependencies(
    value: Option<&JsonValue>,
    path: &str,
    limits: SourceLimits,
    issues: &mut Vec<SourceIssue>,
) -> Vec<PackageDependency> {
    let Some(array) = optional_array(value, &format!("{path}#dependencies"), issues) else {
        return Vec::new();
    };
    array
        .iter()
        .filter_map(|row| {
            let obj = expect_object(row, path, issues).ok()?;
            reject_unknown_keys(
                obj,
                crate::source::contract::PACKAGE_DEPENDENCY_KEYS,
                path,
                issues,
            );
            Some(PackageDependency {
                id: PackageId::new(required_string(obj, "id", path, limits, issues)?),
                reexport: optional_bool(obj, "reexport", false, path, issues),
            })
        })
        .collect()
}

pub(super) fn parse_semantic_config(
    value: Option<&JsonValue>,
    path: &str,
    issues: &mut Vec<SourceIssue>,
) -> SemanticConfig {
    use gvya_kernel::semantic::{
        SEMANTIC_AMBIGUITY_MARGIN_MAX, SEMANTIC_AMBIGUITY_MARGIN_MIN, SEMANTIC_CANDIDATE_LIMIT_MAX,
        SEMANTIC_CANDIDATE_LIMIT_MIN, SEMANTIC_RESOLUTION_THRESHOLD_MAX,
        SEMANTIC_RESOLUTION_THRESHOLD_MIN, SEMANTIC_RESOLVER_CANDIDATE_LIMIT_MAX,
        SEMANTIC_RESOLVER_CANDIDATE_LIMIT_MIN, SEMANTIC_RESOLVER_CONFIDENCE_MAX,
        SEMANTIC_RESOLVER_CONFIDENCE_MIN,
    };
    let d = SemanticConfig::default();
    let Some(value) = value else {
        return d;
    };
    let Some(obj) = value.as_object() else {
        issues.push(issue(
            "source.expected_object",
            "semantic config must be an object",
            Some(path),
        ));
        return d;
    };
    reject_unknown_keys(
        obj,
        crate::source::contract::SEMANTIC_CONFIG_KEYS,
        path,
        issues,
    );
    let candidate_limit = usize_range_field(
        obj,
        "candidate_limit",
        d.candidate_limit,
        SEMANTIC_CANDIDATE_LIMIT_MIN,
        SEMANTIC_CANDIDATE_LIMIT_MAX,
        path,
        issues,
    );
    let resolution_threshold = f64_range_field(
        obj,
        "resolution_threshold",
        d.resolution_threshold,
        SEMANTIC_RESOLUTION_THRESHOLD_MIN,
        SEMANTIC_RESOLUTION_THRESHOLD_MAX,
        path,
        issues,
    );
    let ambiguity_margin = f64_range_field(
        obj,
        "ambiguity_margin",
        d.ambiguity_margin,
        SEMANTIC_AMBIGUITY_MARGIN_MIN,
        SEMANTIC_AMBIGUITY_MARGIN_MAX,
        path,
        issues,
    );
    let resolver_min_confidence = f64_range_field(
        obj,
        "resolver_min_confidence",
        f64::from(d.resolver_min_confidence),
        f64::from(SEMANTIC_RESOLVER_CONFIDENCE_MIN),
        f64::from(SEMANTIC_RESOLVER_CONFIDENCE_MAX),
        path,
        issues,
    ) as f32;
    let resolver_candidate_limit = usize_range_field(
        obj,
        "resolver_candidate_limit",
        d.resolver_candidate_limit,
        SEMANTIC_RESOLVER_CANDIDATE_LIMIT_MIN,
        SEMANTIC_RESOLVER_CANDIDATE_LIMIT_MAX,
        path,
        issues,
    );
    SemanticConfig {
        candidate_limit,
        resolution_threshold,
        ambiguity_margin,
        resolver_min_confidence,
        resolver_candidate_limit,
    }
}
pub(super) fn parse_conversation_config(
    value: Option<&JsonValue>,
    path: &str,
    issues: &mut Vec<SourceIssue>,
) -> ConversationConfig {
    let defaults = ConversationConfig::default();
    let Some(value) = value else {
        return defaults;
    };
    let Some(obj) = value.as_object() else {
        issues.push(issue(
            "source.expected_object",
            "conversation config must be an object",
            Some(path),
        ));
        return defaults;
    };
    reject_unknown_keys(
        obj,
        crate::source::contract::CONVERSATION_CONFIG_KEYS,
        path,
        issues,
    );
    let config = ConversationConfig {
        default_topic_ttl: u32_field(
            obj,
            "default_topic_ttl",
            defaults.default_topic_ttl,
            path,
            issues,
        ),
        default_followup_ttl: u32_field(
            obj,
            "default_followup_ttl",
            defaults.default_followup_ttl,
            path,
            issues,
        ),
        recent_response_limit: usize_field(
            Some(obj),
            "recent_response_limit",
            defaults.recent_response_limit,
            path,
            issues,
        ),
        recent_variant_limit: usize_field(
            Some(obj),
            "recent_variant_limit",
            defaults.recent_variant_limit,
            path,
            issues,
        ),
        recent_user_window: usize_field(
            Some(obj),
            "recent_user_window",
            defaults.recent_user_window,
            path,
            issues,
        ),
        repeat_detection_window: usize_field(
            Some(obj),
            "repeat_detection_window",
            defaults.repeat_detection_window,
            path,
            issues,
        ),
        repeat_detection_threshold: u32_field(
            obj,
            "repeat_detection_threshold",
            defaults.repeat_detection_threshold,
            path,
            issues,
        ),
        max_messages_per_turn: usize_field(
            Some(obj),
            "max_messages_per_turn",
            defaults.max_messages_per_turn,
            path,
            issues,
        ),
        repair_candidate_min_score: finite_field(
            obj,
            "repair_candidate_min_score",
            defaults.repair_candidate_min_score,
            path,
            issues,
        ),
        author_numbers: parse_author_numbers(obj.get("author_numbers"), path, issues),
        topic_preference_margin: finite_field(
            obj,
            "topic_preference_margin",
            defaults.topic_preference_margin,
            path,
            issues,
        ),
    };
    if let Err(error) = config.validate() {
        issues.push(issue(
            "source.conversation_config_range",
            error.0,
            Some(path),
        ));
    }
    config
}

fn parse_author_numbers(
    value: Option<&JsonValue>,
    path: &str,
    issues: &mut Vec<SourceIssue>,
) -> Vec<AuthorNumberDefinition> {
    let Some(rows) = optional_array(value, path, issues) else {
        return Vec::new();
    };
    rows.iter()
        .enumerate()
        .filter_map(|(index, row)| {
            let item_path = format!("{path}.author_numbers[{index}]");
            let obj = expect_object(row, &item_path, issues).ok()?;
            reject_unknown_keys(obj, &["path", "default", "min", "max"], &item_path, issues);
            let path_value = obj
                .get("path")
                .and_then(JsonValue::as_str)
                .map(str::to_owned);
            let number = |key: &str| {
                obj.get(key)
                    .and_then(JsonValue::as_f64)
                    .filter(|v| v.is_finite())
            };
            let Some(path_value) = path_value else {
                issues.push(issue(
                    "source.author_number",
                    "author number path must be a string",
                    Some(&item_path),
                ));
                return None;
            };
            let (Some(default), Some(min), Some(max)) =
                (number("default"), number("min"), number("max"))
            else {
                issues.push(issue(
                    "source.author_number",
                    "author number default/min/max must be finite numbers",
                    Some(&item_path),
                ));
                return None;
            };
            Some(AuthorNumberDefinition {
                path: path_value,
                default,
                min,
                max,
            })
        })
        .collect()
}

pub(super) fn parse_conversation_op(
    value: &str,
    path: &str,
    issues: &mut Vec<SourceIssue>,
) -> Option<ConversationPredicateOp> {
    Some(match value {
        "exists" => ConversationPredicateOp::Exists,
        "missing" => ConversationPredicateOp::Missing,
        "equal" => ConversationPredicateOp::Equal,
        "not_equal" => ConversationPredicateOp::NotEqual,
        "greater" => ConversationPredicateOp::Greater,
        "greater_or_equal" => ConversationPredicateOp::GreaterOrEqual,
        "less" => ConversationPredicateOp::Less,
        "less_or_equal" => ConversationPredicateOp::LessOrEqual,
        _ => {
            issues.push(issue(
                "source.predicate_op",
                "unknown predicate op",
                Some(path),
            ));
            return None;
        }
    })
}
pub(super) fn parse_admission_op(
    value: &str,
    path: &str,
    issues: &mut Vec<SourceIssue>,
) -> Option<AdmissionPredicateOp> {
    Some(match value {
        "exists" => AdmissionPredicateOp::Exists,
        "missing" => AdmissionPredicateOp::Missing,
        "equal" => AdmissionPredicateOp::Equal,
        "not_equal" => AdmissionPredicateOp::NotEqual,
        "greater" => AdmissionPredicateOp::Greater,
        "greater_or_equal" => AdmissionPredicateOp::GreaterOrEqual,
        "less" => AdmissionPredicateOp::Less,
        "less_or_equal" => AdmissionPredicateOp::LessOrEqual,
        _ => {
            issues.push(issue(
                "source.predicate_op",
                "unknown predicate op",
                Some(path),
            ));
            return None;
        }
    })
}

pub(super) fn parse_json_file(
    tree: &SourceTree,
    path: &str,
    limits: SourceLimits,
) -> Result<JsonValue, Vec<SourceIssue>> {
    let Some(bytes) = tree.get(path) else {
        return Err(vec![issue(
            "source.missing_file",
            "required source file is missing",
            Some(path),
        )]);
    };
    if bytes.len() > limits.max_file_bytes {
        return Err(vec![issue(
            "source.json_too_large",
            "JSON source file exceeds configured byte limit",
            Some(path),
        )]);
    }
    serde_json::from_slice(bytes).map_err(|error| {
        vec![issue(
            "source.invalid_json",
            &format!("invalid JSON: {error}"),
            Some(path),
        )]
    })
}
pub(super) fn require_exact_format(
    obj: &serde_json::Map<String, JsonValue>,
    expected: &str,
    path: &str,
    issues: &mut Vec<SourceIssue>,
) {
    require_exact_format_version(obj, expected, SOURCE_VERSION, path, issues);
}

pub(super) fn require_exact_format_version(
    obj: &serde_json::Map<String, JsonValue>,
    expected: &str,
    expected_version: u32,
    path: &str,
    issues: &mut Vec<SourceIssue>,
) {
    if obj.get("format").and_then(JsonValue::as_str) != Some(expected) {
        issues.push(issue(
            "source.format",
            "source document format identifier does not match",
            Some(path),
        ));
    }
    if obj.get("version").and_then(JsonValue::as_u64) != Some(u64::from(expected_version)) {
        issues.push(issue(
            "source.version",
            "unsupported source document version",
            Some(path),
        ));
    }
}
pub(super) fn expect_object<'a>(
    value: &'a JsonValue,
    path: &str,
    issues: &mut Vec<SourceIssue>,
) -> Result<&'a serde_json::Map<String, JsonValue>, ()> {
    value.as_object().ok_or_else(|| {
        issues.push(issue(
            "source.expected_object",
            "expected JSON object",
            Some(path),
        ));
    })
}
pub(super) fn required_value<'a>(
    obj: &'a serde_json::Map<String, JsonValue>,
    key: &str,
    path: &str,
    issues: &mut Vec<SourceIssue>,
) -> Option<&'a JsonValue> {
    match obj.get(key) {
        Some(value) => Some(value),
        None => {
            issues.push(issue(
                "source.field_required",
                &format!("{key} is required"),
                Some(path),
            ));
            None
        }
    }
}
pub(super) fn required_f64(
    obj: &serde_json::Map<String, JsonValue>,
    key: &str,
    path: &str,
    issues: &mut Vec<SourceIssue>,
) -> Option<f64> {
    match required_value(obj, key, path, issues)?
        .as_f64()
        .filter(|value| value.is_finite())
    {
        Some(value) => Some(value),
        None => {
            issues.push(issue(
                "source.number",
                &format!("{key} must be a finite number"),
                Some(path),
            ));
            None
        }
    }
}
pub(super) fn required_raw_string(
    obj: &serde_json::Map<String, JsonValue>,
    key: &str,
    path: &str,
    issues: &mut Vec<SourceIssue>,
) -> Option<String> {
    match required_value(obj, key, path, issues)?.as_str() {
        Some(value) => Some(value.to_owned()),
        None => {
            issues.push(issue(
                "source.string_required",
                &format!("{key} must be a string"),
                Some(path),
            ));
            None
        }
    }
}
pub(super) fn required_string(
    obj: &serde_json::Map<String, JsonValue>,
    key: &str,
    path: &str,
    limits: SourceLimits,
    issues: &mut Vec<SourceIssue>,
) -> Option<String> {
    let value = obj.get(key).and_then(JsonValue::as_str);
    match value {
        Some(value) if value.len() <= limits.max_string_bytes => Some(value.to_owned()),
        Some(_) => {
            issues.push(issue(
                "source.string_too_large",
                "string exceeds configured byte limit",
                Some(path),
            ));
            None
        }
        None => {
            issues.push(issue(
                "source.string_required",
                &format!("{key} must be a string"),
                Some(path),
            ));
            None
        }
    }
}
pub(super) fn optional_string(
    obj: &serde_json::Map<String, JsonValue>,
    key: &str,
    default: &str,
    path: &str,
    limits: SourceLimits,
    issues: &mut Vec<SourceIssue>,
) -> String {
    obj.get(key).map_or_else(
        || default.to_owned(),
        |_| required_string(obj, key, path, limits, issues).unwrap_or_else(|| default.to_owned()),
    )
}
pub(super) fn string_array(
    value: Option<&JsonValue>,
    path: &str,
    limits: SourceLimits,
    issues: &mut Vec<SourceIssue>,
) -> Vec<String> {
    let Some(array) = optional_array(value, path, issues) else {
        return Vec::new();
    };
    array
        .iter()
        .filter_map(|v| match v.as_str() {
            Some(s) if s.len() <= limits.max_string_bytes => Some(s.to_owned()),
            _ => {
                issues.push(issue(
                    "source.string_array",
                    "array contains non-string or oversized string",
                    Some(path),
                ));
                None
            }
        })
        .collect()
}
pub(super) fn parse_string_map(
    value: Option<&JsonValue>,
    path: &str,
    limits: SourceLimits,
    issues: &mut Vec<SourceIssue>,
) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    let Some(value) = value else {
        return out;
    };
    let Some(map) = value.as_object() else {
        issues.push(issue(
            "source.expected_object",
            "expected JSON object",
            Some(path),
        ));
        return out;
    };
    for (key, value) in map {
        if key.len() > limits.max_string_bytes {
            issues.push(issue(
                "source.string",
                "object key exceeds source string limit",
                Some(path),
            ));
            continue;
        }
        match value.as_str() {
            Some(text) if text.len() <= limits.max_string_bytes => {
                out.insert(key.clone(), text.to_owned());
            }
            _ => issues.push(issue(
                "source.string_map",
                "map values must be bounded strings",
                Some(path),
            )),
        }
    }
    out
}

pub(super) fn parse_string_vec_map(
    value: Option<&JsonValue>,
    path: &str,
    limits: SourceLimits,
    issues: &mut Vec<SourceIssue>,
) -> BTreeMap<String, Vec<String>> {
    let mut out = BTreeMap::new();
    let Some(value) = value else {
        return out;
    };
    let Some(map) = value.as_object() else {
        issues.push(issue(
            "source.expected_object",
            "expected JSON object",
            Some(path),
        ));
        return out;
    };
    for (key, value) in map {
        if key.len() > limits.max_string_bytes {
            issues.push(issue(
                "source.string",
                "object key exceeds source string limit",
                Some(path),
            ));
            continue;
        }
        out.insert(
            key.clone(),
            string_array(Some(value), &format!("{path}.{key}"), limits, issues),
        );
    }
    out
}

pub(super) fn parse_f64_map(
    value: Option<&JsonValue>,
    path: &str,
    limits: SourceLimits,
    issues: &mut Vec<SourceIssue>,
) -> BTreeMap<String, f64> {
    let mut out = BTreeMap::new();
    let Some(value) = value else {
        return out;
    };
    let Some(map) = value.as_object() else {
        issues.push(issue(
            "source.expected_object",
            "expected JSON object",
            Some(path),
        ));
        return out;
    };
    for (key, value) in map {
        if key.len() > limits.max_string_bytes {
            issues.push(issue(
                "source.string",
                "object key exceeds source string limit",
                Some(path),
            ));
            continue;
        }
        match value.as_f64() {
            Some(number) if number.is_finite() => {
                out.insert(key.clone(), number);
            }
            _ => issues.push(issue(
                "source.number_map",
                "map values must be finite numbers",
                Some(path),
            )),
        }
    }
    out
}

pub(super) fn string_set(
    value: Option<&JsonValue>,
    path: &str,
    limits: SourceLimits,
    issues: &mut Vec<SourceIssue>,
) -> BTreeSet<String> {
    string_array(value, path, limits, issues)
        .into_iter()
        .collect()
}
pub(super) fn optional_array<'a>(
    value: Option<&'a JsonValue>,
    path: &str,
    issues: &mut Vec<SourceIssue>,
) -> Option<&'a Vec<JsonValue>> {
    match value {
        None => None,
        Some(JsonValue::Array(rows)) => Some(rows),
        Some(_) => {
            issues.push(issue(
                "source.expected_array",
                "expected JSON array",
                Some(path),
            ));
            None
        }
    }
}
pub(super) fn optional_bool(
    obj: &serde_json::Map<String, JsonValue>,
    key: &str,
    default: bool,
    path: &str,
    issues: &mut Vec<SourceIssue>,
) -> bool {
    match obj.get(key) {
        None => default,
        Some(v) => v.as_bool().unwrap_or_else(|| {
            issues.push(issue(
                "source.bool",
                &format!("{key} must be boolean"),
                Some(path),
            ));
            default
        }),
    }
}
pub(super) fn optional_i64(
    obj: &serde_json::Map<String, JsonValue>,
    key: &str,
    default: i64,
    path: &str,
    issues: &mut Vec<SourceIssue>,
) -> i64 {
    obj.get(key).map_or(default, |v| {
        v.as_i64().unwrap_or_else(|| {
            issues.push(issue(
                "source.integer",
                &format!("{key} must be integer"),
                Some(path),
            ));
            default
        })
    })
}
pub(super) fn optional_i32(
    obj: &serde_json::Map<String, JsonValue>,
    key: &str,
    default: i32,
    path: &str,
    issues: &mut Vec<SourceIssue>,
) -> i32 {
    let value = optional_i64(obj, key, i64::from(default), path, issues);
    match i32::try_from(value) {
        Ok(value) => value,
        Err(_) => {
            issues.push(issue(
                "source.i32_range",
                &format!("{key} is outside the signed 32-bit range"),
                Some(path),
            ));
            default
        }
    }
}
pub(super) fn optional_u32(
    obj: &serde_json::Map<String, JsonValue>,
    key: &str,
    path: &str,
    issues: &mut Vec<SourceIssue>,
) -> Option<u32> {
    match obj.get(key) {
        None | Some(JsonValue::Null) => None,
        Some(value) => match value.as_u64() {
            Some(value) => u32::try_from(value).ok().or_else(|| {
                issues.push(issue(
                    "source.u32",
                    &format!("{key} is out of range"),
                    Some(path),
                ));
                None
            }),
            None => {
                issues.push(issue(
                    "source.u32",
                    &format!("{key} must be an unsigned integer"),
                    Some(path),
                ));
                None
            }
        },
    }
}
pub(super) fn required_u32(
    obj: &serde_json::Map<String, JsonValue>,
    key: &str,
    path: &str,
    issues: &mut Vec<SourceIssue>,
) -> Option<u32> {
    optional_u32(obj, key, path, issues).or_else(|| {
        issues.push(issue(
            "source.u32_required",
            &format!("{key} must be an unsigned integer"),
            Some(path),
        ));
        None
    })
}
pub(super) fn usize_field(
    obj: Option<&serde_json::Map<String, JsonValue>>,
    key: &str,
    default: usize,
    path: &str,
    issues: &mut Vec<SourceIssue>,
) -> usize {
    obj.and_then(|o| o.get(key)).map_or(default, |v| {
        v.as_u64()
            .and_then(|v| usize::try_from(v).ok())
            .unwrap_or_else(|| {
                issues.push(issue(
                    "source.usize",
                    &format!("{key} must be an unsigned integer"),
                    Some(path),
                ));
                default
            })
    })
}
pub(super) fn u32_field(
    obj: &serde_json::Map<String, JsonValue>,
    key: &str,
    default: u32,
    path: &str,
    issues: &mut Vec<SourceIssue>,
) -> u32 {
    obj.get(key).map_or(default, |v| {
        v.as_u64()
            .and_then(|v| u32::try_from(v).ok())
            .unwrap_or_else(|| {
                issues.push(issue(
                    "source.u32",
                    &format!("{key} must be unsigned integer"),
                    Some(path),
                ));
                default
            })
    })
}
pub(super) fn finite_field(
    obj: &serde_json::Map<String, JsonValue>,
    key: &str,
    default: f64,
    path: &str,
    issues: &mut Vec<SourceIssue>,
) -> f64 {
    obj.get(key).map_or(default, |v| {
        v.as_f64().filter(|v| v.is_finite()).unwrap_or_else(|| {
            issues.push(issue(
                "source.finite",
                &format!("{key} must be finite number"),
                Some(path),
            ));
            default
        })
    })
}
pub(super) fn usize_range_field(
    obj: &serde_json::Map<String, JsonValue>,
    key: &str,
    default: usize,
    min: usize,
    max: usize,
    path: &str,
    issues: &mut Vec<SourceIssue>,
) -> usize {
    let value = usize_field(Some(obj), key, default, path, issues);
    if !(min..=max).contains(&value) {
        issues.push(issue(
            "source.range",
            &format!("{key} must be inside {min}..={max}"),
            Some(path),
        ));
        default
    } else {
        value
    }
}
pub(super) fn f64_range_field(
    obj: &serde_json::Map<String, JsonValue>,
    key: &str,
    default: f64,
    min: f64,
    max: f64,
    path: &str,
    issues: &mut Vec<SourceIssue>,
) -> f64 {
    let value = finite_field(obj, key, default, path, issues);
    if !(min..=max).contains(&value) {
        issues.push(issue(
            "source.range",
            &format!("{key} must be inside {min}..={max}"),
            Some(path),
        ));
        default
    } else {
        value
    }
}
pub(super) fn optional_id<T, F>(
    obj: &serde_json::Map<String, JsonValue>,
    key: &str,
    constructor: F,
    path: &str,
    limits: SourceLimits,
    issues: &mut Vec<SourceIssue>,
) -> Option<T>
where
    F: Fn(String) -> T,
{
    obj.get(key)
        .filter(|v| !v.is_null())
        .and_then(|_| required_string(obj, key, path, limits, issues))
        .map(constructor)
}
pub(super) fn join_relative(package_file: &str, relative: &str) -> Option<String> {
    if relative.is_empty()
        || relative.starts_with('/')
        || relative.contains('\\')
        || relative
            .split('/')
            .any(|s| s.is_empty() || s == "." || s == "..")
    {
        return None;
    }
    let dir = package_file.rsplit_once('/').map_or("", |(dir, _)| dir);
    let joined = if dir.is_empty() {
        relative.to_owned()
    } else {
        format!("{dir}/{relative}")
    };
    safe_source_path(&joined).then_some(joined)
}
