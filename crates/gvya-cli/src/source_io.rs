//! Bounded canonical source loading and argument parsing.

use super::*;

pub(super) fn parse_attach_signature_args(
    args: &[String],
) -> Result<(PathBuf, PathBuf, PathBuf), String> {
    let Some(first) = args.first().filter(|value| !value.starts_with('-')) else {
        return Err(format!(
            "attach-signature requires ARTIFACT.gvya --envelope FILE.json --output SIGNED.gvya\n\n{USAGE}"
        ));
    };
    let artifact = PathBuf::from(first);
    if artifact.extension().and_then(|value| value.to_str()) != Some("gvya") {
        return Err("attach-signature input must use the .gvya extension".into());
    }
    let mut envelope = None;
    let mut output = None;
    let mut index = 1;
    while index < args.len() {
        match args[index].as_str() {
            "--envelope" => {
                let Some(value) = args.get(index + 1) else {
                    return Err("--envelope requires a path".into());
                };
                envelope = Some(PathBuf::from(value));
                index += 2;
            }
            "--output" | "-o" => {
                let Some(value) = args.get(index + 1) else {
                    return Err("--output requires a path".into());
                };
                output = Some(PathBuf::from(value));
                index += 2;
            }
            other => return Err(format!("unknown attach-signature argument {other:?}")),
        }
    }
    let envelope =
        envelope.ok_or_else(|| "attach-signature requires --envelope FILE.json".to_string())?;
    let output =
        output.ok_or_else(|| "attach-signature requires --output SIGNED.gvya".to_string())?;
    if output.extension().and_then(|value| value.to_str()) != Some("gvya") {
        return Err("attach-signature output must use the .gvya extension".into());
    }
    Ok((artifact, envelope, output))
}

fn parse_base_candidate_args(
    args: &[String],
    command: &str,
) -> Result<(PathBuf, PathBuf, bool), String> {
    let mut paths = Vec::new();
    let mut json_output = false;
    for arg in args {
        match arg.as_str() {
            "--json" => json_output = true,
            value if value.starts_with('-') => {
                return Err(format!("unknown {command} argument {value:?}"));
            }
            value => paths.push(PathBuf::from(value)),
        }
    }
    if paths.len() != 2 {
        return Err(format!(
            "{command} requires BASE_PROJECT CANDIDATE_PROJECT{}",
            if command == "author-step" {
                " --json"
            } else {
                " [--json]"
            }
        ));
    }
    Ok((paths.remove(0), paths.remove(0), json_output))
}

pub(super) fn parse_check_change_args(args: &[String]) -> Result<(PathBuf, PathBuf, bool), String> {
    parse_base_candidate_args(args, "check-change")
}

pub(super) fn parse_author_step_args(args: &[String]) -> Result<(PathBuf, PathBuf), String> {
    let (base, candidate, json_output) = parse_base_candidate_args(args, "author-step")?;
    if !json_output {
        return Err("author-step is machine-oriented and requires --json".into());
    }
    Ok((base, candidate))
}

pub(super) fn parse_build_args(args: &[String]) -> Result<(PathBuf, PathBuf), String> {
    let mut project = PathBuf::from("gvya.project.json");
    let mut output: Option<PathBuf> = None;
    let mut index = 0;
    if let Some(first) = args.first().filter(|value| !value.starts_with('-')) {
        project = PathBuf::from(first);
        index = 1;
    }
    while index < args.len() {
        match args[index].as_str() {
            "--output" | "-o" => {
                let Some(value) = args.get(index + 1) else {
                    return Err("--output requires a path".into());
                };
                output = Some(PathBuf::from(value));
                index += 2;
            }
            other => return Err(format!("unknown build argument {other:?}")),
        }
    }
    let output = output.ok_or_else(|| "build requires --output FILE.gvya".to_string())?;
    if output.extension().and_then(|value| value.to_str()) != Some("gvya") {
        return Err("build output must use the .gvya extension".into());
    }
    Ok((project, output))
}

pub(super) fn project_root(project: &Path) -> Result<PathBuf, String> {
    if project.is_dir() {
        return Ok(project.to_path_buf());
    }
    let name = project
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or_default();
    if name != "gvya.project.json" {
        return Err("PROJECT must be gvya.project.json or its containing directory".into());
    }
    Ok(project
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .to_path_buf())
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct SourceLoadDiagnostic {
    pub(super) code: &'static str,
    pub(super) path: Option<String>,
    pub(super) message: String,
}

fn between<'a>(text: &'a str, prefix: &str, suffix: char) -> Option<&'a str> {
    text.strip_prefix(prefix)?
        .split_once(suffix)
        .map(|(head, _)| head)
}

fn classify_source_load_error(message: String) -> SourceLoadDiagnostic {
    if message.starts_with("invalid gvya.project.json:") {
        return SourceLoadDiagnostic {
            code: "source.invalid_json",
            path: Some("gvya.project.json".into()),
            message,
        };
    }
    if let Some(path) = between(&message, "invalid package JSON ", ':') {
        return SourceLoadDiagnostic {
            code: "source.invalid_json",
            path: Some(path.to_owned()),
            message,
        };
    }
    if let Some(path) = between(&message, "invalid Package fragment JSON ", ':') {
        return SourceLoadDiagnostic {
            code: "source.invalid_json",
            path: Some(path.to_owned()),
            message,
        };
    }
    if let Some(path) = between(&message, "cannot inspect declared source ", ':')
        .or_else(|| between(&message, "cannot resolve declared source ", ':'))
        .or_else(|| between(&message, "cannot stat declared source ", ':'))
    {
        return SourceLoadDiagnostic {
            code: "source.file_unreadable",
            path: Some(path.to_owned()),
            message,
        };
    }
    if let Some(path) = message
        .strip_prefix("unsafe declared source path: ")
        .or_else(|| message.strip_prefix("declared source path may not traverse a symlink: "))
        .or_else(|| message.strip_prefix("declared source escaped project root: "))
    {
        return SourceLoadDiagnostic {
            code: "source.invalid_path",
            path: Some(path.to_owned()),
            message,
        };
    }
    if let Some(path) = message.strip_prefix("undeclared Package fragment ") {
        return SourceLoadDiagnostic {
            code: "source.fragment_undeclared",
            path: Some(path.to_owned()),
            message,
        };
    }
    if message.starts_with("gvya.project.json ") {
        return SourceLoadDiagnostic {
            code: "source.project_shape",
            path: Some("gvya.project.json".into()),
            message,
        };
    }
    if message.starts_with("package ") || message.starts_with("Package asset fragment ") {
        return SourceLoadDiagnostic {
            code: "source.package_shape",
            path: None,
            message,
        };
    }
    if message.starts_with("source tree rejected:") {
        return SourceLoadDiagnostic {
            code: "source.tree_rejected",
            path: None,
            message,
        };
    }
    SourceLoadDiagnostic {
        code: "source.load_failed",
        path: None,
        message,
    }
}

pub(super) fn load_source_tree_diagnostic(
    project: &Path,
    limits: SourceLimits,
) -> Result<SourceTree, SourceLoadDiagnostic> {
    load_source_tree(project, limits).map_err(classify_source_load_error)
}

pub(super) fn load_source_tree(project: &Path, limits: SourceLimits) -> Result<SourceTree, String> {
    let root = project_root(project)?;
    let root = root
        .canonicalize()
        .map_err(|error| format!("cannot resolve project root {}: {error}", root.display()))?;
    let mut files = BTreeMap::new();

    let project_bytes =
        read_declared_source_file(&root, "gvya.project.json", limits.max_file_bytes)?;
    let project_json: serde_json::Value = serde_json::from_slice(&project_bytes)
        .map_err(|error| format!("invalid gvya.project.json: {error}"))?;
    let package_paths = project_json
        .get("packages")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| "gvya.project.json packages must be an array".to_string())?;
    let matcher_profile_paths = project_json
        .get("matcher_profiles")
        .map(|value| {
            value
                .as_array()
                .ok_or_else(|| "gvya.project.json matcher_profiles must be an array".to_string())
        })
        .transpose()?
        .cloned()
        .unwrap_or_default();
    let language_profile_paths = project_json
        .get("language_profiles")
        .map(|value| {
            value
                .as_array()
                .ok_or_else(|| "gvya.project.json language_profiles must be an array".to_string())
        })
        .transpose()?
        .cloned()
        .unwrap_or_default();
    let fallback_package_path = project_json
        .get("fallback_package")
        .filter(|value| !value.is_null())
        .map(|value| {
            value.as_str().ok_or_else(|| {
                "gvya.project.json fallback_package must be a string or null".to_string()
            })
        })
        .transpose()?;
    let declared_package_count = package_paths.len() + usize::from(fallback_package_path.is_some());
    if declared_package_count == 0 || declared_package_count > limits.max_packages {
        return Err("gvya.project.json package count is outside the supported range".into());
    }
    files.insert("gvya.project.json".into(), project_bytes);

    for language_profile_value in language_profile_paths {
        let language_profile_path = language_profile_value.as_str().ok_or_else(|| {
            "gvya.project.json language_profiles must contain strings".to_string()
        })?;
        if !safe_source_path(language_profile_path) {
            return Err(format!(
                "unsafe declared language-profile path: {language_profile_path}"
            ));
        }
        let language_profile_bytes =
            read_declared_source_file(&root, language_profile_path, limits.max_file_bytes)?;
        files.insert(language_profile_path.to_owned(), language_profile_bytes);
    }

    for matcher_profile_value in matcher_profile_paths {
        let matcher_profile_path = matcher_profile_value
            .as_str()
            .ok_or_else(|| "gvya.project.json matcher_profiles must contain strings".to_string())?;
        if !safe_source_path(matcher_profile_path) {
            return Err(format!(
                "unsafe declared matcher-profile path: {matcher_profile_path}"
            ));
        }
        let matcher_profile_bytes =
            read_declared_source_file(&root, matcher_profile_path, limits.max_file_bytes)?;
        files.insert(matcher_profile_path.to_owned(), matcher_profile_bytes);
    }

    let mut declared_assets = std::collections::BTreeSet::new();
    let mut declared_packages = Vec::with_capacity(declared_package_count);
    for package_value in package_paths {
        declared_packages.push(
            package_value
                .as_str()
                .ok_or_else(|| "gvya.project.json packages must contain strings".to_string())?,
        );
    }
    if let Some(fallback_package_path) = fallback_package_path {
        declared_packages.push(fallback_package_path);
    }

    for package_path in declared_packages {
        if !safe_source_path(package_path) {
            return Err(format!("unsafe declared package path: {package_path}"));
        }
        let package_bytes = read_declared_source_file(&root, package_path, limits.max_file_bytes)?;
        let package_json: serde_json::Value = serde_json::from_slice(&package_bytes)
            .map_err(|error| format!("invalid package JSON {package_path}: {error}"))?;
        let declared_fragments = declared_package_fragments(package_path, &package_json)?;
        let declared_fragment_paths = declared_fragments
            .iter()
            .map(|fragment| fragment.path.as_str())
            .collect::<std::collections::BTreeSet<_>>();
        for physical in physical_package_fragment_json_files(&root, package_path, limits)? {
            if !declared_fragment_paths.contains(physical.as_str()) {
                return Err(format!("undeclared Package fragment {physical}"));
            }
        }
        for fragment in declared_fragments {
            let fragment_bytes =
                read_declared_source_file(&root, &fragment.path, limits.max_file_bytes)?;
            if fragment.namespace == "assets" {
                let fragment_json: serde_json::Value = serde_json::from_slice(&fragment_bytes)
                    .map_err(|error| {
                        format!("invalid Package fragment JSON {}: {error}", fragment.path)
                    })?;
                declared_assets.insert(declared_asset_source_from_fragment(
                    package_path,
                    &fragment.path,
                    &fragment_json,
                )?);
            }
            files.insert(fragment.path, fragment_bytes);
        }
        files.insert(package_path.to_owned(), package_bytes);
    }

    for asset_path in declared_assets {
        let bytes = read_declared_source_file(&root, &asset_path, limits.max_asset_bytes)?;
        files.insert(asset_path, bytes);
    }
    SourceTree::new(files, limits).map_err(|issues| format!("source tree rejected: {issues:?}"))
}

pub(super) const PACKAGE_FRAGMENT_NAMESPACES: &[&str] = &[
    "meanings",
    "behaviors",
    "capability_result_behaviors",
    "openings",
    "fallback_behaviors",
    "style_lexicons",
    "capabilities",
    "capability_bindings",
    "capability_policies",
    "capability_configs",
    "types",
    "assets",
    "regression_cases",
    "scenarios",
];

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct DeclaredPackageFragment {
    pub(super) namespace: String,
    pub(super) path: String,
}

pub(super) fn declared_package_fragments(
    package_path: &str,
    package: &serde_json::Value,
) -> Result<Vec<DeclaredPackageFragment>, String> {
    if package.get("format").and_then(serde_json::Value::as_str) != Some("gvya.source.package")
        || package.get("version").and_then(serde_json::Value::as_u64) != Some(1)
    {
        return Err(format!(
            "package {package_path} must use gvya.source.package version 1"
        ));
    }
    let fragments = package
        .get("fragments")
        .and_then(serde_json::Value::as_object)
        .ok_or_else(|| format!("package {package_path} fragments must be an object"))?;
    for key in fragments.keys() {
        if !PACKAGE_FRAGMENT_NAMESPACES.contains(&key.as_str()) {
            return Err(format!(
                "package {package_path} declares unknown fragment namespace {key:?}"
            ));
        }
    }
    let mut declared = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    for namespace in PACKAGE_FRAGMENT_NAMESPACES {
        let Some(rows) = fragments.get(*namespace) else {
            continue;
        };
        let rows = rows.as_array().ok_or_else(|| {
            format!("package {package_path} fragment namespace {namespace} must be an array")
        })?;
        for (index, row) in rows.iter().enumerate() {
            let relative = row.as_str().ok_or_else(|| {
                format!(
                    "package {package_path} fragment namespace {namespace}[{index}] must be a string"
                )
            })?;
            if !relative.starts_with("fragments/") || !relative.ends_with(".json") {
                return Err(format!(
                    "package {package_path} fragment {relative:?} must be a fragments/*.json path"
                ));
            }
            let path = join_declared_relative(package_path, relative).ok_or_else(|| {
                format!("package {package_path} has unsafe fragment path {relative:?}")
            })?;
            if !seen.insert(path.clone()) {
                return Err(format!(
                    "package {package_path} declares duplicate fragment path {relative:?}"
                ));
            }
            declared.push(DeclaredPackageFragment {
                namespace: (*namespace).to_owned(),
                path,
            });
        }
    }
    Ok(declared)
}

fn physical_package_fragment_json_files(
    root: &Path,
    package_path: &str,
    limits: SourceLimits,
) -> Result<std::collections::BTreeSet<String>, String> {
    let package_dir = package_path.rsplit_once('/').map_or("", |(dir, _)| dir);
    let fragment_root_logical = if package_dir.is_empty() {
        "fragments".to_owned()
    } else {
        format!("{package_dir}/fragments")
    };
    let fragment_root = root.join(&fragment_root_logical);
    let metadata = match fs::symlink_metadata(&fragment_root) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Default::default()),
        Err(error) => {
            return Err(format!(
                "cannot inspect Package fragment root {fragment_root_logical}: {error}"
            ));
        }
    };
    if metadata.file_type().is_symlink() {
        return Err(format!(
            "Package fragment root may not be a symlink: {fragment_root_logical}"
        ));
    }
    if !metadata.is_dir() {
        return Err(format!(
            "Package fragment root is not a directory: {fragment_root_logical}"
        ));
    }

    let max_fragment_files = limits
        .max_contributions_per_kind
        .saturating_mul(PACKAGE_FRAGMENT_NAMESPACES.len());
    let mut output = std::collections::BTreeSet::new();
    let mut stack = vec![(fragment_root, fragment_root_logical.clone())];
    while let Some((directory, logical_directory)) = stack.pop() {
        let mut entries = fs::read_dir(&directory)
            .map_err(|error| {
                format!("cannot read Package fragment directory {logical_directory}: {error}")
            })?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| {
                format!("cannot enumerate Package fragment directory {logical_directory}: {error}")
            })?;
        entries.sort_by_key(std::fs::DirEntry::file_name);
        for entry in entries {
            let file_name = entry.file_name().to_string_lossy().into_owned();
            let logical = format!("{logical_directory}/{file_name}");
            if !safe_source_path(&logical) {
                return Err(format!(
                    "unsafe Package fragment filesystem path: {logical}"
                ));
            }
            let metadata = fs::symlink_metadata(entry.path())
                .map_err(|error| format!("cannot inspect Package fragment {logical}: {error}"))?;
            if metadata.file_type().is_symlink() {
                return Err(format!(
                    "Package fragment path may not traverse a symlink: {logical}"
                ));
            }
            if metadata.is_dir() {
                stack.push((entry.path(), logical));
            } else if metadata.is_file() && logical.ends_with(".json") {
                output.insert(logical);
                if output.len() > max_fragment_files {
                    return Err(format!(
                        "Package fragment file count exceeds supported limit under {fragment_root_logical}"
                    ));
                }
            }
        }
    }
    Ok(output)
}

pub(super) fn declared_asset_source_from_fragment(
    package_path: &str,
    fragment_path: &str,
    fragment: &serde_json::Value,
) -> Result<String, String> {
    let source = fragment
        .get("value")
        .and_then(|value| value.get("source"))
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| {
            format!("Package asset fragment {fragment_path} must declare value.source")
        })?;
    join_declared_relative(package_path, source).ok_or_else(|| {
        format!("Package asset fragment {fragment_path} has unsafe value.source path")
    })
}

pub(super) fn join_declared_relative(package_file: &str, relative: &str) -> Option<String> {
    if relative.is_empty()
        || relative.starts_with('/')
        || relative.contains('\\')
        || relative
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..")
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

pub(super) fn read_declared_source_file(
    root: &Path,
    logical: &str,
    max_bytes: usize,
) -> Result<Vec<u8>, String> {
    if !safe_source_path(logical) {
        return Err(format!("unsafe declared source path: {logical}"));
    }
    let mut current = root.to_path_buf();
    for component in logical.split('/') {
        current.push(component);
        let metadata = fs::symlink_metadata(&current)
            .map_err(|error| format!("cannot inspect declared source {logical}: {error}"))?;
        if metadata.file_type().is_symlink() {
            return Err(format!(
                "declared source path may not traverse a symlink: {logical}"
            ));
        }
    }
    let canonical = current
        .canonicalize()
        .map_err(|error| format!("cannot resolve declared source {logical}: {error}"))?;
    if !canonical.starts_with(root) {
        return Err(format!("declared source escaped project root: {logical}"));
    }
    let metadata = fs::metadata(&canonical)
        .map_err(|error| format!("cannot stat declared source {logical}: {error}"))?;
    if !metadata.is_file() {
        return Err(format!("declared source is not a regular file: {logical}"));
    }
    if metadata.len() > u64::try_from(max_bytes).unwrap_or(u64::MAX) {
        return Err(format!("declared source exceeds byte limit: {logical}"));
    }
    read_bounded_file(&canonical, max_bytes, &format!("declared source {logical}"))
}

pub(super) fn parse_inspect_args(
    args: &[String],
) -> Result<(PathBuf, Option<String>, Option<String>), String> {
    let mut project = PathBuf::from("gvya.project.json");
    let mut project_seen = false;
    let mut kind = None;
    let mut id = None;
    let mut json_output = false;
    let mut index = 0usize;
    while index < args.len() {
        match args[index].as_str() {
            "--json" => {
                json_output = true;
                index += 1;
            }
            "--kind" => {
                let Some(value) = args.get(index + 1) else {
                    return Err("--kind requires KIND".into());
                };
                if value.trim().is_empty() || kind.is_some() {
                    return Err("inspect accepts one non-empty --kind KIND".into());
                }
                kind = Some(value.clone());
                index += 2;
            }
            "--id" => {
                let Some(value) = args.get(index + 1) else {
                    return Err("--id requires ID".into());
                };
                if value.trim().is_empty() || id.is_some() {
                    return Err("inspect accepts one non-empty --id ID".into());
                }
                id = Some(value.clone());
                index += 2;
            }
            value if value.starts_with('-') => {
                return Err(format!("unknown inspect argument {value:?}"));
            }
            value => {
                if project_seen {
                    return Err("inspect accepts at most one PROJECT".into());
                }
                project = PathBuf::from(value);
                project_seen = true;
                index += 1;
            }
        }
    }
    if !json_output {
        return Err("inspect is machine-oriented and requires --json".into());
    }
    if id.is_some() && kind.is_none() {
        return Err("inspect --id requires --kind KIND".into());
    }
    Ok((project, kind, id))
}

pub(super) fn parse_project_machine_args(
    args: &[String],
    command: &str,
) -> Result<(PathBuf, bool), String> {
    let mut project = PathBuf::from("gvya.project.json");
    let mut project_seen = false;
    let mut json_output = false;
    for arg in args {
        if arg == "--json" {
            json_output = true;
            continue;
        }
        if arg.starts_with('-') {
            return Err(format!("unknown {command} argument {arg:?}"));
        }
        if project_seen {
            return Err(format!("{command} accepts at most one PROJECT"));
        }
        project = PathBuf::from(arg);
        project_seen = true;
    }
    if matches!(command, "inspect" | "capabilities" | "analysis") && !json_output {
        return Err(format!("{command} is machine-oriented and requires --json"));
    }
    Ok((project, json_output))
}

pub(super) fn parse_check_args(args: &[String]) -> Result<(PathBuf, Option<PathBuf>), String> {
    let mut project = PathBuf::from("gvya.project.json");
    let mut project_seen = false;
    let mut policy = None;
    let mut index = 0usize;
    while index < args.len() {
        match args[index].as_str() {
            "--policy" => {
                let Some(value) = args.get(index + 1) else {
                    return Err("--policy requires POLICY.json".into());
                };
                if policy.is_some() {
                    return Err("check accepts one --policy".into());
                }
                policy = Some(PathBuf::from(value));
                index += 2;
            }
            value if value.starts_with('-') => {
                return Err(format!("unknown check argument {value:?}"));
            }
            value => {
                if project_seen {
                    return Err("check accepts at most one PROJECT".into());
                }
                project = PathBuf::from(value);
                project_seen = true;
                index += 1;
            }
        }
    }
    Ok((project, policy))
}

pub(super) fn parse_capability_args(args: &[String]) -> Result<(PathBuf, String), String> {
    let mut project = PathBuf::from("gvya.project.json");
    let mut project_seen = false;
    let mut id = None;
    let mut json_output = false;
    let mut index = 0usize;
    while index < args.len() {
        match args[index].as_str() {
            "--id" => {
                let Some(value) = args.get(index + 1) else {
                    return Err("--id requires CAPABILITY".into());
                };
                if value.trim().is_empty() {
                    return Err("--id requires a non-empty CAPABILITY".into());
                }
                id = Some(value.clone());
                index += 2;
            }
            "--json" => {
                json_output = true;
                index += 1;
            }
            value if value.starts_with('-') => {
                return Err(format!("unknown capability argument {value:?}"));
            }
            value => {
                if project_seen {
                    return Err("capability accepts at most one PROJECT".into());
                }
                project = PathBuf::from(value);
                project_seen = true;
                index += 1;
            }
        }
    }
    if !json_output {
        return Err("capability is machine-oriented and requires --json".into());
    }
    Ok((
        project,
        id.ok_or_else(|| "capability requires --id CAPABILITY".to_string())?,
    ))
}

pub(super) fn parse_schema_args(args: &[String]) -> Result<Option<String>, String> {
    let mut kind = None;
    let mut json_output = false;
    let mut index = 0usize;
    while index < args.len() {
        match args[index].as_str() {
            "--kind" => {
                let Some(value) = args.get(index + 1) else {
                    return Err("--kind requires KIND".into());
                };
                if value.trim().is_empty() {
                    return Err("--kind requires a non-empty KIND".into());
                }
                if kind.is_some() {
                    return Err("schema accepts one --kind".into());
                }
                kind = Some(value.clone());
                index += 2;
            }
            "--json" => {
                json_output = true;
                index += 1;
            }
            value => return Err(format!("unknown schema argument {value:?}")),
        }
    }
    if !json_output {
        return Err("schema is machine-oriented and requires --json".into());
    }
    Ok(kind)
}

pub(super) fn parse_runtime_request_args(
    args: &[String],
    command: &str,
) -> Result<(PathBuf, PathBuf), String> {
    let mut project = PathBuf::from("gvya.project.json");
    let mut project_seen = false;
    let mut request = None;
    let mut index = 0usize;
    while index < args.len() {
        match args[index].as_str() {
            "--request" => {
                let Some(value) = args.get(index + 1) else {
                    return Err("--request requires FILE.json".into());
                };
                request = Some(PathBuf::from(value));
                index += 2;
            }
            value if value.starts_with('-') => {
                return Err(format!("unknown {command} argument {value:?}"));
            }
            value => {
                if project_seen {
                    return Err(format!("{command} accepts at most one PROJECT"));
                }
                project = PathBuf::from(value);
                project_seen = true;
                index += 1;
            }
        }
    }
    Ok((
        project,
        request.ok_or_else(|| format!("{command} requires --request REQUEST.json"))?,
    ))
}

pub(super) fn resolve_composed(
    project_path: &Path,
) -> Result<
    (
        gvya_compiler::source::ResolvedSourceProject,
        ComposedProject,
    ),
    String,
> {
    let limits = SourceLimits::default();
    let tree = load_source_tree(project_path, limits)?;
    let resolved = resolve_source_project(&tree, limits)
        .map_err(|issues| format!("source resolution failed: {issues:?}"))?;
    let composition = compose_packages(&resolved.packages, &resolved.semantic_profiles);
    let project = composition
        .project
        .ok_or_else(|| format!("composition failed: {:?}", composition.issues))?;
    Ok((resolved, project))
}
