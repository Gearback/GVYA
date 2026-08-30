//! Runtime semantic-index construction tests.
//!
//! The matcher index is derived data. Compiler and runtime build it from the same canonical
//! catalog/profiles/config, so these tests pin the construction contract itself rather than a
//! serialized index snapshot.

use super::*;

fn test_profiles(profile: SemanticProfile) -> SemanticProfiles {
    BTreeMap::from([("und".to_owned(), profile)])
}

#[test]
fn runtime_built_index_serves_matching_and_allowed_filter_without_a_subset_index() {
    let catalog = SemanticCatalog::new(vec![
        MeaningPattern::new("hello", ["hello"]),
        MeaningPattern::new("door.open", ["open door"]),
    ])
    .unwrap();
    let kernel = SemanticKernel::new(
        catalog,
        test_profiles(SemanticProfile::empty()),
        SemanticConfig::default(),
    )
    .unwrap();
    let allowed = BTreeSet::from([MeaningId::new("hello")]);
    let analysis = kernel.analyze_allowed(&SemanticInput::utterance("open door"), &allowed);
    assert!(analysis.scored.iter().all(|row| row.pattern_index == 0));

    let full = kernel.analyze(&SemanticInput::utterance("open door"));
    assert!(
        matches!(full.decision, SemanticDecision::Resolved { ref meaning, .. } if meaning.id.as_str() == "door.open")
    );
}

#[test]
fn identical_semantic_inputs_build_an_identical_index() {
    let patterns = || {
        vec![
            MeaningPattern::new("hello", ["hello", "hi there"]),
            MeaningPattern::new("door.open", ["open door", "please open the door"]),
        ]
    };
    let left = SemanticIndex::build(
        &SemanticCatalog::new(patterns()).unwrap(),
        &test_profiles(SemanticProfile::empty()),
    )
    .unwrap();
    let right = SemanticIndex::build(
        &SemanticCatalog::new(patterns()).unwrap(),
        &test_profiles(SemanticProfile::empty()),
    )
    .unwrap();
    assert_eq!(left, right);
}

#[test]
fn index_construction_fails_closed_when_a_sample_language_has_no_profile() {
    let catalog = SemanticCatalog::new(vec![MeaningPattern {
        id: MeaningId::new("hello"),
        class: MeaningClass::General,
        patterns: vec![],
        priority: 1,
        samples: vec![LocalizedSample::new("fa", "سلام")],
        negative_samples: Vec::new(),
        retrieval_terms: Vec::new(),
        slots: Vec::new(),
        references: Vec::new(),
        positive_assumption: false,
    }])
    .unwrap();
    assert!(matches!(
        SemanticKernel::new(
            catalog,
            test_profiles(SemanticProfile::empty()),
            SemanticConfig::default(),
        ),
        Err(SemanticKernelBuildError::MissingLanguageProfile(_))
    ));
}

#[test]
fn index_construction_fails_closed_on_pathological_exact_fanout() {
    let mut patterns = Vec::new();
    for index in 0..=SEMANTIC_EXACT_FANOUT_MAX {
        patterns.push(MeaningPattern::new(format!("fanout.{index:04}"), ["media"]));
    }
    let catalog = SemanticCatalog::new(patterns).unwrap();
    assert!(matches!(
        SemanticIndex::build(&catalog, &test_profiles(SemanticProfile::empty())),
        Err(SemanticIndexBuildError::ExactFanoutExceeded { .. })
    ));
}

#[test]
fn bounded_allowed_scope_completes_exact_candidate_hidden_by_global_collision_budget() {
    let profile = SemanticProfile::empty();
    let mut patterns = Vec::new();
    for index in 0..200 {
        patterns.push(MeaningPattern {
            id: MeaningId::new(format!("collision.{index:03}")),
            class: MeaningClass::General,
            patterns: vec![],
            priority: 1,
            samples: vec![LocalizedSample::new("und", "media")],
            negative_samples: Vec::new(),
            retrieval_terms: Vec::new(),
            slots: Vec::new(),
            references: Vec::new(),
            positive_assumption: false,
        });
    }
    let target = MeaningId::new("collision.199");
    let kernel = SemanticKernel::new(
        SemanticCatalog::new(patterns).unwrap(),
        test_profiles(profile),
        SemanticConfig {
            candidate_limit: 32,
            ..SemanticConfig::default()
        },
    )
    .unwrap();
    let allowed = BTreeSet::from([target.clone()]);
    let analysis = kernel.analyze_allowed(&SemanticInput::utterance("media"), &allowed);
    assert!(
        matches!(analysis.decision, SemanticDecision::Resolved { ref meaning, .. } if meaning.id == target)
    );
    assert_eq!(
        analysis.candidate_pruning_reason,
        "bounded_allowed_scope_complete"
    );
}
