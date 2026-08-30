//! Compiler IR tests.
use super::*;
use crate::package::{
    NamedTypeDefinition, PackageContents, PackageContribution, PackageDefinition, PackageKind,
    PackageManifest,
};
use gvya_model::{PackageDigest, PackageId, TypeId};

fn test_profiles(profile: SemanticProfile) -> SemanticProfiles {
    BTreeMap::from([
        ("und".to_owned(), profile.clone()),
        ("en".to_owned(), profile.clone()),
        ("en-us".to_owned(), profile),
    ])
}

fn compose_packages(packages: &[PackageDefinition]) -> crate::package::CompositionResult {
    crate::package::compose_packages(packages, &test_profiles(SemanticProfile::empty()))
}

#[test]
fn empty_composed_project_ir_is_reproducible() {
    let package = PackageDefinition {
        manifest: PackageManifest {
            id: PackageId::new("base"),
            digest: PackageDigest::new("a".repeat(64)),
            kind: PackageKind::Standard,
            description: String::new(),
            dependencies: vec![],
        },
        contents: PackageContents::default(),
    };
    let project = compose_packages(&[package]).project.unwrap();
    let identity = CompileIdentity {
        project_id: "p".into(),
        brain_id: "b".into(),
        enabled_languages: vec!["en".into()],
        default_language: "en".into(),
        semantic_config: SemanticConfig::default(),
        conversation_config: ConversationConfig::default(),
        source_packages: BTreeMap::new(),
    };
    let a = compile_ir(&project, &identity).unwrap();
    let b = compile_ir(&project, &identity).unwrap();
    assert_eq!(a.bytes, b.bytes);
    assert_eq!(a.digest_hex, b.digest_hex);
}

#[test]
fn semantic_ir_carries_canonical_matcher_inputs_and_never_a_derived_index() {
    let mut contents = PackageContents::default();
    contents.meanings.push(PackageContribution::add(
        "lights.set",
        MeaningPattern {
            id: gvya_model::MeaningId::new("lights.set"),
            class: MeaningClass::General,
            patterns: vec![],
            samples: vec![gvya_kernel::semantic::LocalizedSample::new(
                "en-US",
                "turn kitchen lights off",
            )],
            negative_samples: vec![],
            retrieval_terms: vec![gvya_kernel::semantic::LocalizedSample::new(
                "en-US",
                "smart home lighting",
            )],
            priority: 1,
            positive_assumption: false,
            slots: vec![],
            references: vec![],
        },
    ));
    let package = PackageDefinition {
        manifest: PackageManifest {
            id: PackageId::new("base"),
            digest: PackageDigest::new("a".repeat(64)),
            kind: PackageKind::Standard,
            description: String::new(),
            dependencies: vec![],
        },
        contents,
    };
    let project = compose_packages(&[package]).project.unwrap();
    let identity = CompileIdentity {
        project_id: "p".into(),
        brain_id: "b".into(),
        enabled_languages: vec!["en-US".into()],
        default_language: "en-US".into(),
        semantic_config: SemanticConfig::default(),
        conversation_config: ConversationConfig::default(),
        source_packages: BTreeMap::new(),
    };
    let ir = compile_ir(&project, &identity).unwrap();

    // The executable semantic section is canonical matcher input only.
    let semantic = ir.document["semantic"].as_object().unwrap();
    assert_eq!(
        semantic.keys().map(String::as_str).collect::<Vec<_>>(),
        vec!["config", "patterns", "profiles"]
    );
    assert!(
        !ir.document.to_string().contains("\"index\""),
        "no derived matcher index may be serialized into the runtime program"
    );

    // Everything the index is derived from is present: samples, retrieval terms and profiles.
    let pattern = &ir.document["semantic"]["patterns"][0];
    assert_eq!(pattern["id"], "lights.set");
    assert_eq!(pattern["samples"][0]["text"], "turn kitchen lights off");
    assert_eq!(pattern["retrieval_terms"][0]["text"], "smart home lighting");
    assert!(ir.document["semantic"]["profiles"]["en-us"].is_object());

    // And the runtime kernel built from exactly that data retrieves through both surfaces.
    let kernel = gvya_kernel::semantic::SemanticKernel::new(
        project.semantic_catalog.clone(),
        project.semantic_profiles.clone(),
        SemanticConfig::default(),
    )
    .unwrap();
    for utterance in ["turn kitchen lights off", "smart home lighting"] {
        let mut input = gvya_kernel::semantic::SemanticInput::utterance(utterance);
        input.utterance.language = Some("en-US".to_owned());
        let analysis = kernel.analyze(&input);
        assert!(
            analysis
                .scored
                .iter()
                .any(|row| row.meaning.as_str() == "lights.set"),
            "runtime-built index must retrieve lights.set for {utterance:?}"
        );
    }
}

#[test]
fn semantic_ir_carries_the_profile_that_produces_canonical_specificity_keys() {
    let mut contents = PackageContents::default();
    contents.meanings.push(PackageContribution::add(
        "boxes.status",
        MeaningPattern::new("boxes.status", ["boxes status"]),
    ));
    let package = PackageDefinition {
        manifest: PackageManifest {
            id: PackageId::new("base"),
            digest: PackageDigest::new("a".repeat(64)),
            kind: PackageKind::Standard,
            description: String::new(),
            dependencies: vec![],
        },
        contents,
    };
    let mut profile = SemanticProfile::empty();
    profile
        .canonical_tokens
        .insert("boxes".to_owned(), "box".to_owned());
    let project = crate::package::compose_packages(&[package], &test_profiles(profile))
        .project
        .unwrap();
    let identity = CompileIdentity {
        project_id: "p".into(),
        brain_id: "b".into(),
        enabled_languages: vec!["und".into()],
        default_language: "und".into(),
        semantic_config: SemanticConfig::default(),
        conversation_config: ConversationConfig::default(),
        source_packages: BTreeMap::new(),
    };
    let ir = compile_ir(&project, &identity).unwrap();
    assert_eq!(
        ir.document["semantic"]["profiles"]["und"]["canonical_tokens"]["boxes"],
        "box"
    );

    // Both the authored and the canonicalized surface form must still resolve from the
    // runtime-built index, which is what the specificity keys existed to guarantee.
    let kernel = gvya_kernel::semantic::SemanticKernel::new(
        project.semantic_catalog.clone(),
        project.semantic_profiles.clone(),
        SemanticConfig::default(),
    )
    .unwrap();
    for utterance in ["boxes status", "box status"] {
        let analysis = kernel.analyze(&gvya_kernel::semantic::SemanticInput::utterance(utterance));
        assert!(
            analysis
                .scored
                .iter()
                .any(|row| row.meaning.as_str() == "boxes.status"),
            "runtime-built index must retrieve boxes.status for {utterance:?}"
        );
    }
}

#[test]
fn compiler_rejects_dangerous_exact_semantic_fanout() {
    let mut contents = PackageContents::default();
    for index in 0..=gvya_kernel::semantic::SEMANTIC_EXACT_FANOUT_MAX {
        contents.meanings.push(PackageContribution::add(
            format!("collision.{index}"),
            MeaningPattern::new(format!("collision.{index}"), ["same exact phrase"]),
        ));
    }
    let package = PackageDefinition {
        manifest: PackageManifest {
            id: PackageId::new("base"),
            digest: PackageDigest::new("a".repeat(64)),
            kind: PackageKind::Standard,
            description: String::new(),
            dependencies: vec![],
        },
        contents,
    };
    let project = compose_packages(&[package]).project.unwrap();
    let identity = CompileIdentity {
        project_id: "p".into(),
        brain_id: "b".into(),
        enabled_languages: vec!["und".into()],
        default_language: "und".into(),
        semantic_config: SemanticConfig::default(),
        conversation_config: ConversationConfig::default(),
        source_packages: BTreeMap::new(),
    };
    assert!(matches!(
        compile_ir(&project, &identity),
        Err(IrError::InvalidSemanticIndex(_))
    ));
}

#[test]
fn non_finite_model_value_fails_closed_instead_of_becoming_null() {
    let mut contents = PackageContents::default();
    contents.meanings.push(PackageContribution::add(
        "hello",
        MeaningPattern::new("hello", ["hello"]),
    ));
    contents.behaviors.push(PackageContribution::add(
        "hello.behavior",
        ConversationBehavior {
            id: gvya_model::BehaviorId::new("hello.behavior"),
            meaning: gvya_model::MeaningId::new("hello"),
            topic: None,
            topic_scoped: false,
            activates_topic: false,
            topic_ttl: None,
            followup_scope: None,
            repair_continuation_candidate: false,
            repeat_same_input_after: None,
            repeat_same_meaning_after: None,
            requires_values: vec![ValueRequirement {
                path: gvya_kernel::conversation::ValuePath {
                    namespace: StateNamespace::Context,
                    path: "device.level".into(),
                },
                value: Value::Number(f64::NAN),
            }],
            forbidden_values: Vec::new(),
            responses: vec![ResponseDefinition::text("hello.response", "und", "Hello")],
        },
    ));
    let package = PackageDefinition {
        manifest: PackageManifest {
            id: PackageId::new("base"),
            digest: PackageDigest::new("a".repeat(64)),
            kind: PackageKind::Standard,
            description: String::new(),
            dependencies: vec![],
        },
        contents,
    };
    let project = compose_packages(&[package]).project.unwrap();
    let identity = CompileIdentity {
        project_id: "p".into(),
        brain_id: "b".into(),
        enabled_languages: vec!["und".into()],
        default_language: "und".into(),
        semantic_config: SemanticConfig::default(),
        conversation_config: ConversationConfig::default(),
        source_packages: BTreeMap::new(),
    };
    assert!(matches!(
        compile_ir(&project, &identity),
        Err(IrError::NonFiniteNumber("model.value.number"))
    ));
}

#[test]
fn runtime_program_carries_no_authoring_or_debug_only_sections() {
    let mut contents = PackageContents::default();
    contents.meanings.push(PackageContribution::add(
        "hello",
        MeaningPattern::new("hello", ["hello"]),
    ));
    contents.types.push(PackageContribution::add(
        "level",
        NamedTypeDefinition {
            id: TypeId::new("level"),
            schema: ValueSchema::Integer {
                minimum: Some(0),
                maximum: Some(10),
            },
        },
    ));
    contents.regression_cases.push(PackageContribution::add(
        "hello.case",
        crate::testing::RegressionCase {
            id: gvya_model::TestCaseId::new("hello.case"),
            description: String::new(),
            input: "hello".into(),
            language: Some("und".into()),
            context: gvya_model::ContextSnapshot {
                values: BTreeMap::new(),
                visible_references: Vec::new(),
                available_capabilities: Vec::new(),
            },
            initial_state: gvya_model::GvyaState::default(),
            seed: None,
            unix_time_ms: None,
            expectation: crate::testing::TurnExpectation {
                meaning: Some(gvya_model::MeaningId::new("hello")),
                ..crate::testing::TurnExpectation::default()
            },
            generated: false,
        },
    ));
    let package = PackageDefinition {
        manifest: PackageManifest {
            id: PackageId::new("base"),
            digest: PackageDigest::new("a".repeat(64)),
            kind: PackageKind::Standard,
            description: String::new(),
            dependencies: vec![],
        },
        contents,
    };
    let project = compose_packages(&[package]).project.unwrap();
    let identity = CompileIdentity {
        project_id: "p".into(),
        brain_id: "b".into(),
        enabled_languages: vec!["und".into()],
        default_language: "und".into(),
        semantic_config: SemanticConfig::default(),
        conversation_config: ConversationConfig::default(),
        source_packages: BTreeMap::new(),
    };

    // The composed project still carries authoring data for audit, tests and the debug map.
    assert_eq!(project.types.len(), 1);
    assert_eq!(project.tests.regression_cases.len(), 1);
    assert!(!project.provenance.is_empty());

    // The executable program does not.
    let ir = compile_ir(&project, &identity).unwrap();
    assert_eq!(
        ir.document
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        vec![
            "assets",
            "brain_id",
            "capabilities",
            "conversation",
            "default_language",
            "enabled_languages",
            "format",
            "packages",
            "project_id",
            "semantic",
            "source_packages",
            "version",
        ]
    );
    let serialized = String::from_utf8(ir.bytes.clone()).unwrap();
    for absent in [
        "provenance",
        "regression_case",
        "scenario",
        "hello.case",
        "level",
    ] {
        assert!(
            !serialized.contains(absent),
            "runtime program must not carry {absent:?}"
        );
    }
}
