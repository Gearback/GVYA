//! Fail-safe canonical source scaffolding for machine authors.

use super::*;

pub(super) fn command_init(args: &[String]) -> Result<(), String> {
    let Some(kind) = args.first().map(String::as_str) else {
        return Err(init_usage());
    };
    match kind {
        "bot" => command_init_bot(&args[1..]),
        "package" => command_init_package(&args[1..]),
        _ => Err(format!(
            "init kind must be bot or package\n\n{}",
            init_usage()
        )),
    }
}

fn command_init_package(args: &[String]) -> Result<(), String> {
    let Some(output) = args.first().filter(|value| !value.starts_with('-')) else {
        return Err(format!(
            "init package requires OUTPUT_DIR\n\n{}",
            init_usage()
        ));
    };
    let output = PathBuf::from(output);
    let mut id = None;
    let mut kind = "standard".to_owned();
    let mut description = String::new();
    let mut authoring_language = None;
    let mut index = 1usize;
    while index < args.len() {
        let (flag, value) = option_value(args, index, "init package")?;
        match flag {
            "--id" => id = Some(value.to_owned()),
            "--kind" => kind = value.to_owned(),
            "--description" => description = value.to_owned(),
            "--authoring-language" => authoring_language = Some(value.to_owned()),
            _ => return Err(format!("unknown init package argument {flag:?}")),
        }
        index += 2;
    }
    let id = id.unwrap_or_else(|| inferred_id(&output).unwrap_or_default());
    validate_scaffold_id(&id, "Package ID")?;
    if !matches!(kind.as_str(), "standard" | "fallback") {
        return Err("--kind must be standard or fallback".into());
    }
    if let Some(language) = &authoring_language {
        validate_language(language, "--authoring-language")?;
    }
    create_new_directory(&output)?;
    let result = (|| {
        let package_path = output.join("package.json");
        write_json_new(
            &package_path,
            &empty_package_document(&id, &kind, &description),
        )?;
        let mut created = vec![package_path.display().to_string()];
        if let Some(language) = &authoring_language {
            let sidecar_path = output.join("authoring.json");
            write_json_new(
                &sidecar_path,
                &serde_json::json!({
                    "format": "gvya.studio.package-authoring",
                    "version": 1,
                    "authoring_language": language,
                    "history": [],
                }),
            )?;
            created.push(sidecar_path.display().to_string());
        }
        print_json(&serde_json::json!({
            "format": "gvya.cli.init",
            "version": 1,
            "created": created,
            "target": {"kind": "package", "id": id, "package_kind": kind},
            "next": {"command": "check-package", "args": [package_path.display().to_string()]},
        }))
    })();
    if result.is_err() {
        let _cleanup = fs::remove_dir_all(&output);
    }
    result
}

fn command_init_bot(args: &[String]) -> Result<(), String> {
    let Some(output) = args.first().filter(|value| !value.starts_with('-')) else {
        return Err(format!("init bot requires OUTPUT_DIR\n\n{}", init_usage()));
    };
    let output = PathBuf::from(output);
    let inferred = inferred_id(&output).unwrap_or_default();
    let mut project_id = None;
    let mut bot_id = None;
    let mut package_id = None;
    let mut languages = vec!["en-US".to_owned()];
    let mut enabled_languages = None;
    let mut default_language = None;
    let mut description = String::new();
    let mut index = 1usize;
    while index < args.len() {
        let (flag, value) = option_value(args, index, "init bot")?;
        match flag {
            "--project-id" => project_id = Some(value.to_owned()),
            "--bot-id" => bot_id = Some(value.to_owned()),
            "--package-id" => package_id = Some(value.to_owned()),
            "--languages" => languages = parse_languages(value, "--languages")?,
            "--enabled-languages" => {
                enabled_languages = Some(parse_languages(value, "--enabled-languages")?);
            }
            "--default-language" => default_language = Some(value.to_owned()),
            "--description" => description = value.to_owned(),
            _ => return Err(format!("unknown init bot argument {flag:?}")),
        }
        index += 2;
    }
    let project_id = project_id.unwrap_or_else(|| inferred.clone());
    let bot_id = bot_id.unwrap_or_else(|| inferred.clone());
    let package_id = package_id.unwrap_or_else(|| format!("{bot_id}.core"));
    validate_scaffold_id(&project_id, "Project ID")?;
    validate_scaffold_id(&bot_id, "Bot ID")?;
    validate_scaffold_id(&package_id, "Package ID")?;
    let enabled_languages = enabled_languages.unwrap_or_else(|| languages.clone());
    let language_keys = languages
        .iter()
        .map(|language| normalize_locale(language))
        .collect::<std::collections::BTreeSet<_>>();
    if enabled_languages
        .iter()
        .any(|language| !language_keys.contains(&normalize_locale(language)))
    {
        return Err("--enabled-languages must be a subset of --languages".into());
    }
    let default_language = default_language.unwrap_or_else(|| enabled_languages[0].clone());
    if !enabled_languages
        .iter()
        .any(|language| normalize_locale(language) == normalize_locale(&default_language))
    {
        return Err("--default-language must be enabled".into());
    }
    let package_logical = format!("packages/standard/{package_id}/package.json");
    create_new_directory(&output)?;
    let result = (|| {
        let package_path = output
            .join("packages")
            .join("standard")
            .join(&package_id)
            .join("package.json");
        let package_parent = package_path
            .parent()
            .ok_or_else(|| "cannot derive initial Package directory".to_string())?;
        fs::create_dir_all(package_parent).map_err(|error| {
            format!(
                "cannot create initial Package directory {}: {error}",
                package_parent.display()
            )
        })?;
        let core_description = if description.is_empty() {
            format!("Core authored behavior for {bot_id}")
        } else {
            description.clone()
        };
        write_json_new(
            &package_path,
            &empty_package_document(&package_id, "standard", &core_description),
        )?;
        let project_path = output.join("gvya.project.json");
        write_json_new(
            &project_path,
            &serde_json::json!({
                "format": "gvya.source.project",
                "version": 1,
                "project_id": project_id,
                "brain_id": bot_id,
                "languages": languages,
                "enabled_languages": enabled_languages,
                "default_language": default_language,
                "matcher_profiles": [],
                "packages": [package_logical],
                "fallback_package": null,
                "emit_debug_map": true,
                "semantic": {
                    "candidate_limit": 120,
                    "resolution_threshold": 0.45,
                    "ambiguity_margin": 0.04,
                    "resolver_min_confidence": 0.55,
                    "resolver_candidate_limit": 8,
                },
                "conversation": {
                    "default_topic_ttl": 3,
                    "default_followup_ttl": 2,
                    "recent_response_limit": 8,
                    "recent_variant_limit": 4,
                    "recent_user_window": 4,
                    "repeat_detection_window": 3,
                    "repeat_detection_threshold": 2,
                    "max_messages_per_turn": 4,
                    "topic_preference_margin": 0.04,
                },
            }),
        )?;
        print_json(&serde_json::json!({
            "format": "gvya.cli.init",
            "version": 1,
            "created": [project_path.display().to_string(), package_path.display().to_string()],
            "target": {"kind": "bot", "project_id": project_id, "bot_id": bot_id, "package_id": package_id},
            "next": {"command": "check", "args": [output.display().to_string()]},
            "notes": ["One canonical source root is one resolved Bot compile target."],
        }))
    })();
    if result.is_err() {
        let _cleanup = fs::remove_dir_all(&output);
    }
    result
}

fn empty_package_document(id: &str, kind: &str, description: &str) -> serde_json::Value {
    serde_json::json!({
        "format": "gvya.source.package",
        "version": 1,
        "manifest": {
            "id": id,
            "kind": kind,
            "description": description,
            "dependencies": [],
        },
        "fragments": {},
    })
}

fn inferred_id(output: &Path) -> Option<String> {
    output
        .file_name()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

fn validate_scaffold_id(value: &str, label: &str) -> Result<(), String> {
    let mut bytes = value.bytes();
    let Some(first) = bytes.next() else {
        return Err(format!(
            "{label} is required; pass it explicitly when OUTPUT_DIR has no usable folder name"
        ));
    };
    if !first.is_ascii_alphanumeric()
        || !bytes.all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
    {
        return Err(format!(
            "{label} must start with an ASCII letter or number and use only letters, numbers, dot, underscore, or hyphen"
        ));
    }
    Ok(())
}

fn parse_languages(value: &str, flag: &str) -> Result<Vec<String>, String> {
    let languages = value
        .split(',')
        .map(str::trim)
        .map(str::to_owned)
        .collect::<Vec<_>>();
    if languages.is_empty() || languages.len() > 32 || languages.iter().any(String::is_empty) {
        return Err(format!(
            "{flag} requires 1..=32 comma-separated BCP 47 tags"
        ));
    }
    let mut normalized = std::collections::BTreeSet::new();
    for language in &languages {
        validate_language(language, flag)?;
        if !normalized.insert(normalize_locale(language)) {
            return Err(format!("{flag} contains a duplicate language: {language}"));
        }
    }
    Ok(languages)
}

fn validate_language(value: &str, flag: &str) -> Result<(), String> {
    if language_tag_is_well_formed(value) {
        Ok(())
    } else {
        Err(format!("{flag} contains a malformed BCP 47 tag: {value}"))
    }
}

fn option_value<'a>(
    args: &'a [String],
    index: usize,
    command: &str,
) -> Result<(&'a str, &'a str), String> {
    let flag = args[index].as_str();
    if !flag.starts_with("--") {
        return Err(format!(
            "{command} accepts exactly one positional OUTPUT_DIR"
        ));
    }
    let Some(value) = args.get(index + 1) else {
        return Err(format!("{flag} requires a value"));
    };
    Ok((flag, value))
}

fn create_new_directory(path: &Path) -> Result<(), String> {
    if path.exists() {
        return Err(format!(
            "refusing to overwrite existing output directory: {}",
            path.display()
        ));
    }
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent).map_err(|error| {
            format!("cannot create output parent {}: {error}", parent.display())
        })?;
    }
    fs::create_dir(path)
        .map_err(|error| format!("cannot create output directory {}: {error}", path.display()))
}

fn write_json_new(path: &Path, value: &serde_json::Value) -> Result<(), String> {
    let mut bytes = serde_json::to_vec_pretty(value)
        .map_err(|error| format!("cannot serialize scaffold JSON: {error}"))?;
    bytes.push(b'\n');
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| format!("cannot create {}: {error}", path.display()))?;
    file.write_all(&bytes)
        .map_err(|error| format!("cannot write {}: {error}", path.display()))
}

fn init_usage() -> String {
    "gvya init bot OUTPUT_DIR [--project-id ID] [--bot-id ID] [--package-id ID] [--languages en-US,fa-IR] [--enabled-languages en-US] [--default-language en-US] [--description TEXT]\n  gvya init package OUTPUT_DIR [--id ID] [--kind standard|fallback] [--description TEXT] [--authoring-language LANGUAGE]".into()
}
