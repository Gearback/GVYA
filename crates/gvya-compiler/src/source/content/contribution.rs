//! Contribution source decoding.

use super::super::*;

pub(in crate::source) type Decoder<T> =
    fn(&JsonValue, &str, SourceLimits, &mut Vec<SourceIssue>) -> Option<T>;

pub(in crate::source) fn contribution_from_fragment<T>(
    row: &JsonValue,
    fragment_file: &str,
    limits: SourceLimits,
    issues: &mut Vec<SourceIssue>,
    decoder: Decoder<T>,
) -> Option<PackageContribution<T>> {
    let path = fragment_file.to_owned();
    let Some(row_obj) = row.as_object() else {
        issues.push(issue(
            "source.expected_object",
            "contribution fragment must be an object",
            Some(&path),
        ));
        return None;
    };
    reject_unknown_keys(
        row_obj,
        crate::source::contract::CONTRIBUTION_KEYS,
        &path,
        issues,
    );
    let id = required_string(row_obj, "id", &path, limits, issues).unwrap_or_default();
    let exported = optional_bool(row_obj, "exported", true, &path, issues);
    let mode = parse_mode(row_obj.get("mode"), &path, limits, issues);
    let Some(value) = row_obj.get("value") else {
        issues.push(issue(
            "source.value_missing",
            "contribution value is required",
            Some(&path),
        ));
        return None;
    };
    decoder(value, &format!("{path}#value"), limits, issues).map(|decoded| PackageContribution {
        id,
        exported,
        mode,
        value: decoded,
    })
}

pub(in crate::source) fn asset_contribution_from_fragment(
    tree: &SourceTree,
    package_file: &str,
    fragment_file: &str,
    row: &JsonValue,
    limits: SourceLimits,
    asset_bytes: &mut BTreeMap<AssetId, Vec<u8>>,
    issues: &mut Vec<SourceIssue>,
) -> Option<PackageContribution<PackageAsset>> {
    let path = fragment_file.to_owned();
    let Some(row_obj) = row.as_object() else {
        issues.push(issue(
            "source.expected_object",
            "asset contribution fragment must be an object",
            Some(&path),
        ));
        return None;
    };
    reject_unknown_keys(
        row_obj,
        crate::source::contract::CONTRIBUTION_KEYS,
        &path,
        issues,
    );
    let id = required_string(row_obj, "id", &path, limits, issues).unwrap_or_default();
    let exported = optional_bool(row_obj, "exported", true, &path, issues);
    let mode = parse_mode(row_obj.get("mode"), &path, limits, issues);
    let Some(value_obj) = row_obj.get("value").and_then(JsonValue::as_object) else {
        issues.push(issue(
            "source.expected_object",
            "asset value must be an object",
            Some(&path),
        ));
        return None;
    };
    reject_unknown_keys(
        value_obj,
        crate::source::contract::ASSET_KEYS,
        &format!("{path}#value"),
        issues,
    );
    let asset_id = required_string(value_obj, "id", &path, limits, issues).unwrap_or_default();
    let media_type =
        required_string(value_obj, "media_type", &path, limits, issues).unwrap_or_default();
    let logical_path =
        required_string(value_obj, "logical_path", &path, limits, issues).unwrap_or_default();
    let source = required_string(value_obj, "source", &path, limits, issues).unwrap_or_default();
    if !safe_asset_logical_path(&logical_path) {
        issues.push(issue(
            "source.asset_logical_path",
            "asset logical_path is unsafe",
            Some(&path),
        ));
        return None;
    }
    let Some(source_path) = join_relative(package_file, &source) else {
        issues.push(issue(
            "source.asset_source_path",
            "asset source path is unsafe",
            Some(&path),
        ));
        return None;
    };
    let Some(bytes) = tree.get(&source_path) else {
        issues.push(issue(
            "source.asset_missing",
            "asset source file is missing",
            Some(&source_path),
        ));
        return None;
    };
    if bytes.len() > limits.max_asset_bytes {
        issues.push(issue(
            "source.asset_too_large",
            "asset exceeds configured byte limit",
            Some(&source_path),
        ));
        return None;
    }
    let asset_id_typed = AssetId::new(asset_id);
    let digest = sha256_hex(bytes);
    asset_bytes.insert(asset_id_typed.clone(), bytes.to_vec());
    Some(PackageContribution {
        id,
        exported,
        mode,
        value: PackageAsset {
            id: asset_id_typed,
            media_type,
            logical_path,
            digest: PackageDigest::new(digest),
        },
    })
}

pub(in crate::source) fn parse_mode(
    value: Option<&JsonValue>,
    path: &str,
    limits: SourceLimits,
    issues: &mut Vec<SourceIssue>,
) -> ContributionMode {
    let Some(value) = value else {
        return ContributionMode::Add;
    };
    if value == "add" {
        return ContributionMode::Add;
    }
    let Some(obj) = value.as_object() else {
        issues.push(issue(
            "source.mode",
            "mode must be \"add\" or a replace object",
            Some(path),
        ));
        return ContributionMode::Add;
    };
    reject_unknown_keys(
        obj,
        &["type", "target_package", "target_id"],
        &format!("{path}.mode"),
        issues,
    );
    if obj.get("type").and_then(JsonValue::as_str) != Some("replace") {
        issues.push(issue(
            "source.mode",
            "replace mode must declare type=replace",
            Some(path),
        ));
        return ContributionMode::Add;
    }
    let target_package =
        required_string(obj, "target_package", path, limits, issues).unwrap_or_default();
    let target_id = required_string(obj, "target_id", path, limits, issues).unwrap_or_default();
    ContributionMode::Replace {
        target_package: PackageId::new(target_package),
        target_id,
    }
}
