//! Exact authored-source inventory for machine authoring.
//!
//! This surface deliberately reports raw source values plus physical provenance. Composition and
//! runtime inspection answer different questions; an authoring agent needs to know which declared
//! source object and contribution envelope it must edit.

use serde_json::{Value as JsonValue, json};

use super::{SourceLimits, SourceTree, parse_json_file, resolve_source_project};

const INSPECTABLE_KINDS: &[&str] = &[
    "project",
    "package",
    "package-manifest",
    "package-fragments",
    "contribution",
    "meaning",
    "behavior",
    "fallback-behavior",
    "capability-result-behavior",
    "opening",
    "response",
    "style-lexicon",
    "capability",
    "capability-contract",
    "capability-binding",
    "capability-policy",
    "capability-config",
    "named-type",
    "asset",
    "regression-case",
    "scenario",
    "language-profile",
    "matcher-profile",
];

const NAMESPACE_KINDS: &[(&str, &str)] = &[
    ("meanings", "meaning"),
    ("behaviors", "behavior"),
    ("capability_result_behaviors", "capability-result-behavior"),
    ("openings", "opening"),
    ("fallback_behaviors", "fallback-behavior"),
    ("style_lexicons", "style-lexicon"),
    ("capabilities", "capability"),
    ("capability_bindings", "capability-binding"),
    ("capability_policies", "capability-policy"),
    ("capability_configs", "capability-config"),
    ("types", "named-type"),
    ("assets", "asset"),
    ("regression_cases", "regression-case"),
    ("scenarios", "scenario"),
];

#[must_use]
pub fn source_inspection_kinds() -> &'static [&'static str] {
    INSPECTABLE_KINDS
}

fn logical_value_id<'a>(
    value_kind: &str,
    contribution_id: &'a str,
    value: &'a JsonValue,
) -> Option<&'a str> {
    match value_kind {
        "capability" => value.pointer("/contract/id").and_then(JsonValue::as_str),
        "style-lexicon" | "capability-config" => Some(contribution_id),
        _ => value
            .get("id")
            .and_then(JsonValue::as_str)
            .or(Some(contribution_id)),
    }
}

fn record(
    kind: &str,
    id: Option<&str>,
    file: &str,
    pointer: &str,
    package_id: Option<&str>,
    namespace: Option<&str>,
    contribution_id: Option<&str>,
    owner_kind: Option<&str>,
    owner_id: Option<&str>,
    value: JsonValue,
) -> JsonValue {
    json!({
        "kind": kind,
        "id": id,
        "location": {
            "file": file,
            "pointer": pointer,
            "package_id": package_id,
            "namespace": namespace,
            "contribution_id": contribution_id,
            "owner_kind": owner_kind,
            "owner_id": owner_id,
        },
        "value": value,
    })
}

fn response_records(
    records: &mut Vec<JsonValue>,
    fragment_file: &str,
    package_id: &str,
    namespace: &str,
    contribution_id: &str,
    owner_kind: &str,
    value: &JsonValue,
) {
    let owner_id = logical_value_id(owner_kind, contribution_id, value).unwrap_or(contribution_id);
    let Some(responses) = value.get("responses").and_then(JsonValue::as_array) else {
        return;
    };
    for (response_index, response) in responses.iter().enumerate() {
        let response_id = response.get("id").and_then(JsonValue::as_str);
        records.push(record(
            "response",
            response_id,
            fragment_file,
            &format!("/value/responses/{response_index}"),
            Some(package_id),
            Some(namespace),
            Some(contribution_id),
            Some(owner_kind),
            Some(owner_id),
            response.clone(),
        ));
    }
}

/// Returns raw, declared authoring objects with exact file/pointer provenance.
///
/// The source first passes the canonical resolver so inspection never invents a parallel relaxed
/// parsing contract. `id` is an exact authored identity filter; when supplied, absence is an error.
pub fn source_object_inventory_json(
    tree: &SourceTree,
    kind: &str,
    id: Option<&str>,
    limits: SourceLimits,
) -> Result<JsonValue, String> {
    if !INSPECTABLE_KINDS.contains(&kind) {
        return Err(format!(
            "unsupported inspect kind {kind:?}; use `gvya schema --json` to discover source kinds"
        ));
    }

    let resolved = resolve_source_project(tree, limits)
        .map_err(|issues| format!("source resolution failed: {issues:?}"))?;
    let mut records = Vec::new();

    let project = parse_json_file(tree, "gvya.project.json", limits)
        .map_err(|issues| format!("project source parse failed: {issues:?}"))?;
    if kind == "project" {
        records.push(record(
            "project",
            project.get("project_id").and_then(JsonValue::as_str),
            "gvya.project.json",
            "",
            None,
            None,
            None,
            None,
            None,
            project.clone(),
        ));
    }

    if kind == "language-profile" {
        for language_file in &resolved.project.language_profile_files {
            let value = parse_json_file(tree, language_file, limits)
                .map_err(|issues| format!("language-profile source parse failed: {issues:?}"))?;
            let language = value
                .get("language")
                .and_then(JsonValue::as_str)
                .map(str::to_owned);
            records.push(record(
                "language-profile",
                language.as_deref(),
                language_file,
                "",
                None,
                None,
                None,
                None,
                None,
                value,
            ));
        }
    }

    if kind == "matcher-profile" {
        for matcher_file in &resolved.project.matcher_profile_files {
            let value = parse_json_file(tree, matcher_file, limits)
                .map_err(|issues| format!("matcher-profile source parse failed: {issues:?}"))?;
            let language = value
                .get("language")
                .and_then(JsonValue::as_str)
                .map(str::to_owned);
            records.push(record(
                "matcher-profile",
                language.as_deref(),
                matcher_file,
                "",
                None,
                None,
                None,
                None,
                None,
                value,
            ));
        }
    }

    let package_files = resolved
        .project
        .package_files
        .iter()
        .chain(resolved.project.fallback_package_file.iter());
    for package_file in package_files {
        let root = parse_json_file(tree, package_file, limits).map_err(|issues| {
            format!("package source parse failed for {package_file}: {issues:?}")
        })?;
        let package_id = root
            .pointer("/manifest/id")
            .and_then(JsonValue::as_str)
            .unwrap_or_default();

        if kind == "package" {
            records.push(record(
                "package",
                Some(package_id),
                package_file,
                "",
                Some(package_id),
                None,
                None,
                None,
                None,
                root.clone(),
            ));
        }
        if kind == "package-manifest" {
            if let Some(manifest) = root.get("manifest") {
                records.push(record(
                    "package-manifest",
                    Some(package_id),
                    package_file,
                    "/manifest",
                    Some(package_id),
                    None,
                    None,
                    None,
                    None,
                    manifest.clone(),
                ));
            }
        }
        if kind == "package-fragments" {
            if let Some(fragments) = root.get("fragments") {
                records.push(record(
                    "package-fragments",
                    Some(package_id),
                    package_file,
                    "/fragments",
                    Some(package_id),
                    None,
                    None,
                    None,
                    None,
                    fragments.clone(),
                ));
            }
        }

        let Some(fragments) = root.get("fragments").and_then(JsonValue::as_object) else {
            continue;
        };
        for (namespace, value_kind) in NAMESPACE_KINDS {
            let Some(fragment_paths) = fragments.get(*namespace).and_then(JsonValue::as_array)
            else {
                continue;
            };
            for relative in fragment_paths {
                let Some(relative) = relative.as_str() else {
                    continue;
                };
                let Some(fragment_file) = super::join_relative(package_file, relative) else {
                    continue;
                };
                let contribution =
                    parse_json_file(tree, &fragment_file, limits).map_err(|issues| {
                        format!("package fragment parse failed for {fragment_file}: {issues:?}")
                    })?;
                let contribution_id = contribution
                    .get("id")
                    .and_then(JsonValue::as_str)
                    .unwrap_or_default();
                if kind == "contribution" {
                    records.push(record(
                        "contribution",
                        Some(contribution_id),
                        &fragment_file,
                        "",
                        Some(package_id),
                        Some(namespace),
                        Some(contribution_id),
                        None,
                        None,
                        contribution.clone(),
                    ));
                }
                let Some(value) = contribution.get("value") else {
                    continue;
                };
                if kind == *value_kind {
                    records.push(record(
                        value_kind,
                        logical_value_id(value_kind, contribution_id, value),
                        &fragment_file,
                        "/value",
                        Some(package_id),
                        Some(namespace),
                        Some(contribution_id),
                        None,
                        None,
                        value.clone(),
                    ));
                }
                if kind == "capability-contract" && *value_kind == "capability" {
                    if let Some(contract) = value.get("contract") {
                        let contract_id = contract.get("id").and_then(JsonValue::as_str);
                        records.push(record(
                            "capability-contract",
                            contract_id,
                            &fragment_file,
                            "/value/contract",
                            Some(package_id),
                            Some(namespace),
                            Some(contribution_id),
                            Some("capability"),
                            logical_value_id("capability", contribution_id, value),
                            contract.clone(),
                        ));
                    }
                }
                if kind == "response"
                    && matches!(
                        *value_kind,
                        "behavior" | "capability-result-behavior" | "opening" | "fallback-behavior"
                    )
                {
                    response_records(
                        &mut records,
                        &fragment_file,
                        package_id,
                        namespace,
                        contribution_id,
                        value_kind,
                        value,
                    );
                }
            }
        }
    }

    if let Some(expected_id) = id {
        records.retain(|row| row.get("id").and_then(JsonValue::as_str) == Some(expected_id));
        if records.is_empty() {
            return Err(format!("{kind} not found: {expected_id}"));
        }
    }

    Ok(json!({
        "format": "gvya.cli.source-inspect",
        "version": 1,
        "view": "authored-source",
        "kind": kind,
        "id": id,
        "count": records.len(),
        "items": records,
    }))
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;

    fn sample_tree() -> SourceTree {
        SourceTree::new(
            BTreeMap::from([
                (
                    "gvya.project.json".into(),
                    br#"{"format":"gvya.source.project","version":1,"project_id":"demo","brain_id":"assistant","languages":["en-US"],"enabled_languages":["en-US"],"default_language":"en-US","packages":["packages/base/package.json"]}"#.to_vec(),
                ),
                (
                    "packages/base/package.json".into(),
                    br#"{"format":"gvya.source.package","version":1,"manifest":{"id":"base","kind":"standard","dependencies":[]},"fragments":{"meanings":["fragments/meanings/hello.json"],"behaviors":["fragments/behaviors/hello.behavior.json"]}}"#.to_vec(),
                ),
                (
                    "packages/base/fragments/meanings/hello.json".into(),
                    br#"{"id":"hello","value":{"id":"hello","samples":[{"language":"en-US","text":"hello"}]}}"#.to_vec(),
                ),
                (
                    "packages/base/fragments/behaviors/hello.behavior.json".into(),
                    br#"{"id":"hello.behavior","value":{"id":"hello.behavior","meaning":"hello","responses":[{"id":"hello.response","texts":[{"language":"en-US","variants":["Hello"]}]}]}}"#.to_vec(),
                ),
            ]),
            SourceLimits::default(),
        )
        .unwrap()
    }

    #[test]
    fn inventory_returns_exact_raw_object_with_edit_provenance() {
        let report = source_object_inventory_json(
            &sample_tree(),
            "behavior",
            Some("hello.behavior"),
            SourceLimits::default(),
        )
        .unwrap();
        assert_eq!(report["count"], 1);
        assert_eq!(
            report["items"][0]["location"]["file"],
            "packages/base/fragments/behaviors/hello.behavior.json"
        );
        assert_eq!(report["items"][0]["location"]["namespace"], "behaviors");
        assert_eq!(report["items"][0]["value"]["meaning"], "hello");
    }

    #[test]
    fn inventory_filters_by_logical_value_id_while_preserving_contribution_id() {
        let tree = SourceTree::new(
            BTreeMap::from([
                (
                    "gvya.project.json".into(),
                    br#"{"format":"gvya.source.project","version":1,"project_id":"demo","brain_id":"assistant","languages":["en-US"],"enabled_languages":["en-US"],"default_language":"en-US","packages":["packages/base/package.json"]}"#.to_vec(),
                ),
                (
                    "packages/base/package.json".into(),
                    br#"{"format":"gvya.source.package","version":1,"manifest":{"id":"base","kind":"standard","dependencies":[]},"fragments":{"meanings":["fragments/meanings/hello.json"],"behaviors":["fragments/behaviors/logical.json"]}}"#.to_vec(),
                ),
                (
                    "packages/base/fragments/meanings/hello.json".into(),
                    br#"{"id":"hello","value":{"id":"hello","samples":[{"language":"en-US","text":"hello"}]}}"#.to_vec(),
                ),
                (
                    "packages/base/fragments/behaviors/logical.json".into(),
                    br#"{"id":"physical.behavior","value":{"id":"logical.behavior","meaning":"hello","responses":[{"id":"logical.response","texts":[{"language":"en-US","variants":["Hello"]}]}]}}"#.to_vec(),
                ),
            ]),
            SourceLimits::default(),
        )
        .unwrap();
        let behavior = source_object_inventory_json(
            &tree,
            "behavior",
            Some("logical.behavior"),
            SourceLimits::default(),
        )
        .unwrap();
        assert_eq!(behavior["count"], 1);
        assert_eq!(behavior["items"][0]["id"], "logical.behavior");
        assert_eq!(
            behavior["items"][0]["location"]["contribution_id"],
            "physical.behavior"
        );

        let response = source_object_inventory_json(
            &tree,
            "response",
            Some("logical.response"),
            SourceLimits::default(),
        )
        .unwrap();
        assert_eq!(
            response["items"][0]["location"]["owner_id"],
            "logical.behavior"
        );
        assert_eq!(
            response["items"][0]["location"]["contribution_id"],
            "physical.behavior"
        );
    }

    #[test]
    fn inventory_exposes_nested_responses_as_first_class_authored_objects() {
        let report = source_object_inventory_json(
            &sample_tree(),
            "response",
            Some("hello.response"),
            SourceLimits::default(),
        )
        .unwrap();
        assert_eq!(report["count"], 1);
        assert_eq!(report["items"][0]["location"]["owner_kind"], "behavior");
        assert_eq!(report["items"][0]["location"]["owner_id"], "hello.behavior");
    }
}
