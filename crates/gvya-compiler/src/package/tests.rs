//! Package composition tests.
use super::*;
use gvya_kernel::semantic::SemanticProfile;

fn test_profiles(profile: SemanticProfile) -> SemanticProfiles {
    BTreeMap::from([
        ("und".to_owned(), profile.clone()),
        ("en".to_owned(), profile.clone()),
        ("en-us".to_owned(), profile),
    ])
}

fn compose_packages(packages: &[PackageDefinition]) -> CompositionResult {
    super::compose_packages(packages, &test_profiles(SemanticProfile::empty()))
}

fn manifest(id: &str, deps: Vec<PackageDependency>) -> PackageManifest {
    PackageManifest {
        id: PackageId::new(id),
        digest: PackageDigest::new("a".repeat(64)),
        kind: PackageKind::Standard,
        description: String::new(),
        dependencies: deps,
    }
}

fn dependency(id: &str, reexport: bool) -> PackageDependency {
    PackageDependency {
        id: PackageId::new(id),
        reexport,
    }
}

fn package(id: &str, deps: Vec<PackageDependency>, contents: PackageContents) -> PackageDefinition {
    PackageDefinition {
        manifest: manifest(id, deps),
        contents,
    }
}

fn fallback_behavior(id: &str) -> FallbackBehavior {
    FallbackBehavior {
        id: gvya_model::BehaviorId::new(id),
        trigger: gvya_kernel::conversation::FallbackTrigger::Unresolved,
        priority: 0,
        conditions: Vec::new(),
        responses: vec![gvya_kernel::conversation::ResponseDefinition::text(
            format!("{id}.response"),
            "en",
            "I did not understand that.",
        )],
    }
}

fn fallback_package(
    id: &str,
    deps: Vec<PackageDependency>,
    contents: PackageContents,
) -> PackageDefinition {
    let mut package = package(id, deps, contents);
    package.manifest.kind = PackageKind::Fallback;
    package
}

#[test]
fn valid_fallback_package_is_private_add_only_and_composes_as_single_root() {
    let mut row =
        PackageContribution::add("fallback.generic", fallback_behavior("fallback.generic"));
    row.exported = false;
    let result = compose_packages(&[fallback_package(
        "fallback",
        Vec::new(),
        PackageContents {
            fallback_behaviors: vec![row],
            ..PackageContents::default()
        },
    )]);
    assert!(
        result.project.is_some(),
        "valid fallback package should compose: {:?}",
        result.issues
    );
}

#[test]
fn fallback_package_contract_forbids_dependency_override_and_standard_content() {
    let mut exported =
        PackageContribution::add("fallback.generic", fallback_behavior("fallback.generic"));
    exported.exported = true;
    let result = compose_packages(&[
        package("base", Vec::new(), PackageContents::default()),
        fallback_package(
            "fallback",
            vec![dependency("base", false)],
            PackageContents {
                meanings: vec![PackageContribution::add(
                    "wrong",
                    MeaningPattern::new("wrong", ["wrong"]),
                )],
                fallback_behaviors: vec![exported],
                ..PackageContents::default()
            },
        ),
    ]);
    assert!(result.project.is_none());
    for code in [
        "fallback_dependencies_forbidden",
        "standard_content_in_fallback_package",
        "fallback_override_contract",
    ] {
        assert!(
            result.issues.iter().any(|issue| issue.code == code),
            "missing {code}: {:?}",
            result.issues
        );
    }
}

#[test]
fn fallback_package_cannot_enter_dependency_graph_and_only_one_root_is_allowed() {
    let fallback_a = fallback_package("fallback.a", Vec::new(), PackageContents::default());
    let fallback_b = fallback_package("fallback.b", Vec::new(), PackageContents::default());
    let app = package(
        "app",
        vec![dependency("fallback.a", false)],
        PackageContents::default(),
    );
    let result = compose_packages(&[fallback_a, fallback_b, app]);
    assert!(result.project.is_none());
    assert!(
        result
            .issues
            .iter()
            .any(|issue| issue.code == "multiple_fallback_packages")
    );
    assert!(
        result
            .issues
            .iter()
            .any(|issue| issue.code == "fallback_dependency_forbidden")
    );
}

#[test]
fn standard_package_cannot_smuggle_fallback_behavior() {
    let result = compose_packages(&[package(
        "standard",
        Vec::new(),
        PackageContents {
            fallback_behaviors: vec![PackageContribution::add(
                "fallback.bad",
                fallback_behavior("fallback.bad"),
            )],
            ..PackageContents::default()
        },
    )]);
    assert!(result.project.is_none());
    assert!(
        result
            .issues
            .iter()
            .any(|issue| issue.code == "fallback_content_in_standard_package")
    );
}

#[test]
fn duplicate_add_is_not_load_order_override() {
    let base = package(
        "base",
        Vec::new(),
        PackageContents {
            meanings: vec![PackageContribution::add(
                "hello",
                MeaningPattern::new("hello", ["hello"]),
            )],
            ..PackageContents::default()
        },
    );
    let app = package(
        "app",
        vec![dependency("base", false)],
        PackageContents {
            meanings: vec![PackageContribution::add(
                "hello",
                MeaningPattern::new("hello", ["hi"]),
            )],
            ..PackageContents::default()
        },
    );
    let result = compose_packages(&[app, base]);
    assert!(result.project.is_none());
    assert!(
        result
            .issues
            .iter()
            .any(|issue| issue.code == "contribution_collision")
    );
}

#[test]
fn explicit_whole_item_replace_is_deterministic() {
    let base = package(
        "base",
        Vec::new(),
        PackageContents {
            meanings: vec![PackageContribution::add(
                "hello",
                MeaningPattern::new("hello", ["hello"]),
            )],
            ..PackageContents::default()
        },
    );
    let app = package(
        "app",
        vec![dependency("base", false)],
        PackageContents {
            meanings: vec![PackageContribution {
                id: "hello".into(),
                exported: true,
                mode: ContributionMode::Replace {
                    target_package: PackageId::new("base"),
                    target_id: "hello".into(),
                },
                value: MeaningPattern::new("hello", ["greetings"]),
            }],
            ..PackageContents::default()
        },
    );
    let result = compose_packages(&[app, base]);
    let project = result.project.expect("explicit replacement should compose");
    assert_eq!(
        project
            .semantic_catalog
            .get(&gvya_model::MeaningId::new("hello"))
            .unwrap()
            .samples,
        vec![gvya_kernel::semantic::LocalizedSample::new(
            "und",
            "greetings"
        )]
    );
    assert_eq!(
        project
            .provenance
            .get(&(ContributionKind::Meaning, "hello".into()))
            .unwrap()
            .owner,
        PackageId::new("app")
    );
}

#[test]
fn transitive_target_requires_reexport() {
    let base = package(
        "base",
        Vec::new(),
        PackageContents {
            meanings: vec![PackageContribution::add(
                "hello",
                MeaningPattern::new("hello", ["hello"]),
            )],
            ..PackageContents::default()
        },
    );
    let middle = package(
        "middle",
        vec![dependency("base", false)],
        PackageContents::default(),
    );
    let app = package(
        "app",
        vec![dependency("middle", false)],
        PackageContents {
            meanings: vec![PackageContribution {
                id: "hello".into(),
                exported: true,
                mode: ContributionMode::Replace {
                    target_package: PackageId::new("base"),
                    target_id: "hello".into(),
                },
                value: MeaningPattern::new("hello", ["greetings"]),
            }],
            ..PackageContents::default()
        },
    );
    let result = compose_packages(&[base, middle, app]);
    assert!(result.project.is_none());
    assert!(
        result
            .issues
            .iter()
            .any(|issue| issue.code == "replacement_target_not_visible")
    );
}

#[test]
fn reexport_makes_transitive_specialization_explicitly_visible() {
    let base = package(
        "base",
        Vec::new(),
        PackageContents {
            meanings: vec![PackageContribution::add(
                "hello",
                MeaningPattern::new("hello", ["hello"]),
            )],
            ..PackageContents::default()
        },
    );
    let middle = package(
        "middle",
        vec![dependency("base", true)],
        PackageContents::default(),
    );
    let app = package(
        "app",
        vec![dependency("middle", false)],
        PackageContents {
            meanings: vec![PackageContribution {
                id: "hello".into(),
                exported: true,
                mode: ContributionMode::Replace {
                    target_package: PackageId::new("base"),
                    target_id: "hello".into(),
                },
                value: MeaningPattern::new("hello", ["greetings"]),
            }],
            ..PackageContents::default()
        },
    );
    let result = compose_packages(&[base, middle, app]);
    assert!(
        result.project.is_some(),
        "re-export should expose transitive target: {:?}",
        result.issues
    );
}

#[test]
fn private_contribution_cannot_be_specialized() {
    let mut private = PackageContribution::add("hello", MeaningPattern::new("hello", ["hello"]));
    private.exported = false;
    let base = package(
        "base",
        Vec::new(),
        PackageContents {
            meanings: vec![private],
            ..PackageContents::default()
        },
    );
    let app = package(
        "app",
        vec![dependency("base", false)],
        PackageContents {
            meanings: vec![PackageContribution {
                id: "hello".into(),
                exported: true,
                mode: ContributionMode::Replace {
                    target_package: PackageId::new("base"),
                    target_id: "hello".into(),
                },
                value: MeaningPattern::new("hello", ["greetings"]),
            }],
            ..PackageContents::default()
        },
    );
    let result = compose_packages(&[base, app]);
    assert!(result.project.is_none());
    assert!(
        result
            .issues
            .iter()
            .any(|issue| issue.code == "replacement_target_private")
    );
}

#[test]
fn composition_result_does_not_depend_on_input_package_order() {
    let base = package(
        "base",
        Vec::new(),
        PackageContents {
            meanings: vec![PackageContribution::add(
                "hello",
                MeaningPattern::new("hello", ["hello"]),
            )],
            ..PackageContents::default()
        },
    );
    let addon = package(
        "addon",
        vec![dependency("base", false)],
        PackageContents {
            meanings: vec![PackageContribution::add(
                "bye",
                MeaningPattern::new("bye", ["goodbye"]),
            )],
            ..PackageContents::default()
        },
    );
    let forward = compose_packages(&[base.clone(), addon.clone()])
        .project
        .expect("forward order should compose");
    let reverse = compose_packages(&[addon, base])
        .project
        .expect("reverse order should compose");
    let forward_ids = forward
        .semantic_catalog
        .patterns()
        .iter()
        .map(|pattern| pattern.id.clone())
        .collect::<Vec<_>>();
    let reverse_ids = reverse
        .semantic_catalog
        .patterns()
        .iter()
        .map(|pattern| pattern.id.clone())
        .collect::<Vec<_>>();
    assert_eq!(forward.package_order, reverse.package_order);
    assert_eq!(forward_ids, reverse_ids);
    assert_eq!(forward.provenance, reverse.provenance);
}

#[test]
fn semantic_profile_is_language_neutral_without_profile_data() {
    let base = package(
        "base",
        Vec::new(),
        PackageContents {
            meanings: vec![PackageContribution::add(
                "hello",
                MeaningPattern::new("hello", ["hello"]),
            )],
            ..PackageContents::default()
        },
    );
    let project = compose_packages(&[base])
        .project
        .expect("neutral profile should compose");
    assert!(
        project
            .semantic_profiles
            .get("und")
            .unwrap()
            .canonical_tokens
            .is_empty()
    );
    assert!(
        project
            .semantic_profiles
            .get("und")
            .unwrap()
            .colloquial
            .is_empty()
    );
    assert_eq!(
        project
            .semantic_profiles
            .get("und")
            .unwrap()
            .canonical_token("dogs"),
        "dogs"
    );
}

#[test]
fn semantic_profile_is_composed_only_from_explicit_data() {
    let mut colloquial = BTreeMap::new();
    colloquial.insert("hiya".to_owned(), vec!["hello".to_owned()]);
    let base = package(
        "base",
        Vec::new(),
        PackageContents {
            meanings: vec![PackageContribution::add(
                "hello",
                MeaningPattern::new("hello", ["hello"]),
            )],
            ..PackageContents::default()
        },
    );
    let mut profile = SemanticProfile::empty();
    profile
        .canonical_tokens
        .insert("dogs".to_owned(), "dog".to_owned());
    profile.colloquial = colloquial;
    let project = super::compose_packages(&[base], &test_profiles(profile))
        .project
        .expect("explicit lexical data should compose");
    assert_eq!(
        project
            .semantic_profiles
            .get("und")
            .unwrap()
            .canonical_token("dogs"),
        "dog"
    );
    assert_eq!(
        project
            .semantic_profiles
            .get("und")
            .unwrap()
            .colloquial
            .get("hiya"),
        Some(&vec!["hello".to_owned()])
    );
}

#[test]
fn dependency_cycle_fails_closed() {
    let a = package(
        "a",
        vec![dependency("b", false)],
        PackageContents::default(),
    );
    let b = package(
        "b",
        vec![dependency("a", false)],
        PackageContents::default(),
    );
    let result = compose_packages(&[a, b]);
    assert!(result.project.is_none());
    assert!(
        result
            .issues
            .iter()
            .any(|issue| issue.code == "dependency_cycle")
    );
}
