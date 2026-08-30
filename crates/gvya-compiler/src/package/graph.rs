//! Package-kind, dependency-graph and visibility validation.

use super::*;

pub(super) fn validate_package_kind_contract(
    package: &PackageDefinition,
    issues: &mut Vec<CompositionIssue>,
) {
    let manifest = &package.manifest;
    let contents = &package.contents;
    match manifest.kind {
        PackageKind::Standard => {
            if !contents.fallback_behaviors.is_empty() {
                issues.push(CompositionIssue::error(
                    "fallback_content_in_standard_package",
                    Some(manifest.id.clone()),
                    Some(ContributionKind::FallbackBehavior),
                    None,
                    "fallback behaviors may exist only in a fallback package",
                ));
            }
        }
        PackageKind::Fallback => {
            if !manifest.dependencies.is_empty() {
                issues.push(CompositionIssue::error(
                    "fallback_dependencies_forbidden",
                    Some(manifest.id.clone()),
                    Some(ContributionKind::FallbackBehavior),
                    None,
                    "fallback packages are self-contained and cannot declare dependencies",
                ));
            }
            let forbidden = [
                ("meanings", !contents.meanings.is_empty()),
                ("behaviors", !contents.behaviors.is_empty()),
                (
                    "capability_result_behaviors",
                    !contents.capability_result_behaviors.is_empty(),
                ),
                ("openings", !contents.openings.is_empty()),
                ("style_lexicons", !contents.style_lexicons.is_empty()),
                ("capabilities", !contents.capabilities.is_empty()),
                (
                    "capability_bindings",
                    !contents.capability_bindings.is_empty(),
                ),
                (
                    "capability_policies",
                    !contents.capability_policies.is_empty(),
                ),
                (
                    "capability_configs",
                    !contents.capability_configs.is_empty(),
                ),
                ("types", !contents.types.is_empty()),
            ];
            for (namespace, present) in forbidden {
                if present {
                    issues.push(CompositionIssue::error(
                        "standard_content_in_fallback_package",
                        Some(manifest.id.clone()),
                        Some(ContributionKind::FallbackBehavior),
                        Some(namespace.to_string()),
                        format!("fallback package cannot contain standard namespace {namespace}"),
                    ));
                }
            }
            validate_fallback_add_only(
                &contents.fallback_behaviors,
                manifest,
                "fallback_behaviors",
                issues,
            );
            validate_fallback_add_only(&contents.assets, manifest, "assets", issues);
            validate_fallback_add_only(
                &contents.regression_cases,
                manifest,
                "regression_cases",
                issues,
            );
            validate_fallback_add_only(&contents.scenarios, manifest, "scenarios", issues);
        }
    }
}

pub(super) fn validate_fallback_add_only<T>(
    rows: &[PackageContribution<T>],
    manifest: &PackageManifest,
    namespace: &str,
    issues: &mut Vec<CompositionIssue>,
) {
    for row in rows {
        if row.exported || !matches!(row.mode, ContributionMode::Add) {
            issues.push(CompositionIssue::error(
                "fallback_override_contract",
                Some(manifest.id.clone()),
                Some(ContributionKind::FallbackBehavior),
                Some(row.id.clone()),
                format!("fallback package {namespace} contributions must be private add-only content; fallback packages cannot be overridden"),
            ));
        }
    }
}

pub(super) fn validate_graph<'a>(
    packages: &'a [PackageDefinition],
) -> (
    BTreeMap<PackageId, GraphPackage<'a>>,
    Vec<PackageId>,
    Vec<CompositionIssue>,
) {
    let mut definitions = BTreeMap::new();
    let mut issues = Vec::new();
    for package in packages {
        let id = package.manifest.id.clone();
        if id.as_str().trim().is_empty() {
            issues.push(CompositionIssue::error(
                "package_id_empty",
                None,
                None,
                None,
                "package id cannot be empty",
            ));
            continue;
        }
        if !valid_digest(package.manifest.digest.as_str()) {
            issues.push(CompositionIssue::error(
                "package_digest_invalid",
                Some(id.clone()),
                None,
                None,
                "package digest must be exactly 64 hexadecimal characters",
            ));
        }
        if definitions.insert(id.clone(), package).is_some() {
            issues.push(CompositionIssue::error(
                "duplicate_package",
                Some(id),
                None,
                None,
                "package id is declared more than once",
            ));
        }
    }

    let fallback_count = definitions
        .values()
        .filter(|package| package.manifest.kind == PackageKind::Fallback)
        .count();
    if fallback_count > 1 {
        issues.push(CompositionIssue::error(
            "multiple_fallback_packages",
            None,
            Some(ContributionKind::FallbackBehavior),
            None,
            "a compiled Brain may contain at most one selected fallback package",
        ));
    }
    for package in definitions.values() {
        validate_package_kind_contract(package, &mut issues);
    }

    for package in definitions.values() {
        let mut dependency_ids = BTreeSet::new();
        for dependency in &package.manifest.dependencies {
            if !dependency_ids.insert(dependency.id.clone()) {
                issues.push(CompositionIssue::error(
                    "duplicate_dependency",
                    Some(package.manifest.id.clone()),
                    None,
                    Some(dependency.id.as_str().to_owned()),
                    "dependency is declared more than once",
                ));
                continue;
            }
            match definitions.get(&dependency.id) {
                None => issues.push(CompositionIssue::error(
                    "dependency_missing",
                    Some(package.manifest.id.clone()),
                    None,
                    Some(dependency.id.as_str().to_owned()),
                    "declared dependency is not present in the resolved package graph",
                )),
                Some(target) => {
                    if target.manifest.kind == PackageKind::Fallback {
                        issues.push(CompositionIssue::error(
                            "fallback_dependency_forbidden",
                            Some(package.manifest.id.clone()),
                            None,
                            Some(dependency.id.as_str().to_owned()),
                            "fallback packages are selected directly by the Brain and cannot participate in dependency or override graphs",
                        ));
                    }
                }
            }
        }
    }

    let order = topo_order(&definitions, &mut issues);
    let mut graph = BTreeMap::new();
    if !has_errors(&issues) {
        for (id, definition) in &definitions {
            graph.insert(
                id.clone(),
                GraphPackage {
                    definition,
                    visible_dependencies: visible_dependencies(definition, &definitions),
                },
            );
        }
    }
    (graph, order, issues)
}

pub(super) fn topo_order(
    definitions: &BTreeMap<PackageId, &PackageDefinition>,
    issues: &mut Vec<CompositionIssue>,
) -> Vec<PackageId> {
    #[derive(Clone, Copy, Eq, PartialEq)]
    enum Mark {
        Visiting,
        Done,
    }

    fn visit(
        id: &PackageId,
        definitions: &BTreeMap<PackageId, &PackageDefinition>,
        marks: &mut BTreeMap<PackageId, Mark>,
        stack: &mut Vec<PackageId>,
        order: &mut Vec<PackageId>,
        issues: &mut Vec<CompositionIssue>,
    ) {
        match marks.get(id).copied() {
            Some(Mark::Done) => return,
            Some(Mark::Visiting) => {
                let cycle = stack
                    .iter()
                    .map(|value| value.as_str())
                    .collect::<Vec<_>>()
                    .join(" -> ");
                issues.push(CompositionIssue::error(
                    "dependency_cycle",
                    Some(id.clone()),
                    None,
                    None,
                    format!(
                        "package dependency cycle detected: {cycle} -> {}",
                        id.as_str()
                    ),
                ));
                return;
            }
            None => {}
        }
        marks.insert(id.clone(), Mark::Visiting);
        stack.push(id.clone());
        if let Some(package) = definitions.get(id) {
            let mut dependencies = package.manifest.dependencies.clone();
            dependencies.sort_by(|left, right| left.id.cmp(&right.id));
            for dependency in dependencies {
                if definitions.contains_key(&dependency.id) {
                    visit(&dependency.id, definitions, marks, stack, order, issues);
                }
            }
        }
        stack.pop();
        marks.insert(id.clone(), Mark::Done);
        if !order.contains(id) {
            order.push(id.clone());
        }
    }

    let mut marks = BTreeMap::new();
    let mut stack = Vec::new();
    let mut order = Vec::new();
    for id in definitions.keys() {
        visit(id, definitions, &mut marks, &mut stack, &mut order, issues);
    }
    order
}

pub(super) fn visible_dependencies(
    package: &PackageDefinition,
    definitions: &BTreeMap<PackageId, &PackageDefinition>,
) -> BTreeSet<PackageId> {
    fn include_reexports(
        id: &PackageId,
        definitions: &BTreeMap<PackageId, &PackageDefinition>,
        visible: &mut BTreeSet<PackageId>,
    ) {
        let Some(package) = definitions.get(id) else {
            return;
        };
        let mut deps = package.manifest.dependencies.clone();
        deps.sort_by(|left, right| left.id.cmp(&right.id));
        for dependency in deps.into_iter().filter(|dependency| dependency.reexport) {
            if visible.insert(dependency.id.clone()) {
                include_reexports(&dependency.id, definitions, visible);
            }
        }
    }

    let mut visible = BTreeSet::new();
    let mut direct = package.manifest.dependencies.clone();
    direct.sort_by(|left, right| left.id.cmp(&right.id));
    for dependency in direct {
        if visible.insert(dependency.id.clone()) {
            include_reexports(&dependency.id, definitions, &mut visible);
        }
    }
    visible
}
