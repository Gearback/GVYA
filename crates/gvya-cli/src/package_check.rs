//! Standalone Package validation through the canonical Rust source/compiler/runtime path.

use super::*;

pub(super) fn command_check_package(args: &[String]) -> Result<(), String> {
    let (package_path, policy_path) = parse_check_package_args(args)?;
    let policy = load_acceptance_policy(policy_path.as_deref())?;
    let (report, accepted) = match load_package_check_tree(&package_path, SourceLimits::default()) {
        Ok(tree) => check_tree_report(&tree, &policy),
        Err(error) => Ok(check_source_failure_report(
            &policy,
            "authoring.package_unreadable",
            "repair_package_source",
            "The standalone Package could not be loaded.",
            serde_json::json!({"stage": "package_load", "error": error}),
        )),
    }?;
    print_json(&report)?;
    if accepted {
        Ok(())
    } else {
        Err("package check rejected the source".into())
    }
}

fn parse_check_package_args(args: &[String]) -> Result<(PathBuf, Option<PathBuf>), String> {
    let Some(first) = args.first().filter(|value| !value.starts_with('-')) else {
        return Err("check-package requires PACKAGE.json or its containing directory".into());
    };
    let package = PathBuf::from(first);
    let mut policy = None;
    let mut index = 1usize;
    while index < args.len() {
        match args[index].as_str() {
            "--policy" => {
                let Some(value) = args.get(index + 1) else {
                    return Err("--policy requires POLICY.json".into());
                };
                if policy.is_some() {
                    return Err("check-package accepts one --policy".into());
                }
                policy = Some(PathBuf::from(value));
                index += 2;
            }
            value => return Err(format!("unknown check-package argument {value:?}")),
        }
    }
    Ok((package, policy))
}

fn load_package_check_tree(path: &Path, limits: SourceLimits) -> Result<SourceTree, String> {
    let package_path = if path.is_dir() {
        path.join("package.json")
    } else {
        path.to_path_buf()
    };
    let metadata = fs::symlink_metadata(&package_path)
        .map_err(|error| format!("cannot inspect Package {}: {error}", package_path.display()))?;
    if metadata.file_type().is_symlink() {
        return Err("Package path may not be a symlink".into());
    }
    let package_path = package_path
        .canonicalize()
        .map_err(|error| format!("cannot resolve Package {}: {error}", package_path.display()))?;
    let root = package_path
        .parent()
        .ok_or_else(|| "Package has no containing directory".to_string())?;
    let package_bytes = read_bounded_file(&package_path, limits.max_file_bytes, "Package")?;
    let package_json: serde_json::Value = serde_json::from_slice(&package_bytes)
        .map_err(|error| format!("invalid Package JSON: {error}"))?;
    let package_kind = package_json
        .get("manifest")
        .and_then(|value| value.get("kind"))
        .and_then(serde_json::Value::as_str)
        .unwrap_or("standard");

    let mut files = BTreeMap::new();
    files.insert("package.json".to_owned(), package_bytes);
    let mut languages = Vec::new();
    let mut language_keys = std::collections::BTreeSet::new();
    let mut declared_assets = std::collections::BTreeSet::new();
    for fragment in declared_package_fragments("package.json", &package_json)? {
        let fragment_bytes =
            read_declared_source_file(root, &fragment.path, limits.max_file_bytes)?;
        let fragment_json: serde_json::Value = serde_json::from_slice(&fragment_bytes)
            .map_err(|error| format!("invalid Package fragment JSON {}: {error}", fragment.path))?;
        collect_languages(&fragment_json, &mut languages, &mut language_keys);
        if fragment.namespace == "assets" {
            declared_assets.insert(declared_asset_source_from_fragment(
                "package.json",
                &fragment.path,
                &fragment_json,
            )?);
        }
        files.insert(fragment.path, fragment_bytes);
    }
    if languages.is_empty() {
        languages.push("und".to_owned());
    }
    for asset_path in declared_assets {
        let bytes = read_declared_source_file(root, &asset_path, limits.max_asset_bytes)?;
        files.insert(asset_path, bytes);
    }

    let fallback = package_kind == "fallback";
    let project_json = serde_json::json!({
        "format": "gvya.source.project",
        "version": 1,
        "project_id": "package-check",
        "brain_id": "package-check",
        "languages": languages,
        "enabled_languages": languages,
        "default_language": languages[0],
        "packages": if fallback { Vec::<String>::new() } else { vec!["package.json".to_owned()] },
        "fallback_package": if fallback { Some("package.json") } else { None },
    });
    files.insert(
        "gvya.project.json".to_owned(),
        serde_json::to_vec(&project_json)
            .map_err(|error| format!("cannot serialize Package check wrapper: {error}"))?,
    );
    SourceTree::new(files, limits)
        .map_err(|issues| format!("Package source tree rejected: {issues:?}"))
}

fn collect_languages(
    value: &serde_json::Value,
    languages: &mut Vec<String>,
    language_keys: &mut std::collections::BTreeSet<String>,
) {
    match value {
        serde_json::Value::Array(rows) => {
            for row in rows {
                collect_languages(row, languages, language_keys);
            }
        }
        serde_json::Value::Object(object) => {
            if let Some(language) = object.get("language").and_then(serde_json::Value::as_str) {
                if language_tag_is_well_formed(language)
                    && language_keys.insert(normalize_locale(language))
                {
                    languages.push(language.to_owned());
                }
            }
            for row in object.values() {
                collect_languages(row, languages, language_keys);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
pub(super) fn package_check_tree_for_test(path: &Path) -> Result<SourceTree, String> {
    load_package_check_tree(path, SourceLimits::default())
}
