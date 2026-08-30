//! Program hydration tests.
use super::*;

/// The canonical `program.json` payload of the independent runtime-action fixture, as bytes the
/// loader would actually receive. Boundary tests mutate a parsed copy of it.
fn fixture_program() -> JsonValue {
    let artifact = include_bytes!("../../../../validation/fixtures/runtime-action.gvya");
    let parsed =
        gvya_artifact::parse_artifact(artifact, gvya_artifact::ArtifactLimits::default()).unwrap();
    serde_json::from_slice(parsed.entry("program.json").unwrap()).unwrap()
}

fn hydrate(document: &JsonValue) -> Result<HydratedProgram, ProgramError> {
    hydrate_program(&serde_json::to_vec(document).unwrap())
}

#[test]
fn runtime_builds_the_matcher_index_from_the_canonical_semantics_the_artifact_ships() {
    let program = hydrate(&fixture_program()).unwrap();
    let mut input = gvya_kernel::semantic::SemanticInput::utterance("hello");
    input.utterance.language = Some(program.default_language.clone());
    let analysis = program.semantic.analyze(&input);
    assert!(
        analysis
            .scored
            .iter()
            .any(|row| row.meaning.as_str() == "hello"),
        "a program carrying only patterns/profiles/config must still retrieve its Meanings"
    );
}

#[test]
fn runtime_rejects_a_program_that_still_ships_a_derived_semantic_index() {
    let mut document = fixture_program();
    document["semantic"]["index"] = serde_json::json!({
        "pattern_count": 1,
        "exact_sample": {"2:en:hello": ["hello"]},
        "token": {"2:en:hello": ["hello"]},
        "content_token": {"2:en:hello": ["hello"]},
        "bigram": {}, "content_bigram": {}, "meta_token": {}, "meta_bigram": {},
        "sample_start_bigram": {}, "exact_content": {"2:en:hello": ["hello"]},
        "known_typo_tokens": ["hello"], "typo_by_length": {"5": ["hello"]},
    });
    assert!(
        matches!(hydrate(&document), Err(ProgramError::Json(_))),
        "there is no legacy reader for a shipped matcher index"
    );
}

#[test]
fn runtime_rejects_authoring_and_debug_sections_in_the_executable_program() {
    for (field, payload) in [
        ("provenance", serde_json::json!([])),
        ("types", serde_json::json!([])),
        (
            "tests",
            serde_json::json!({"regression_case_ids": [], "scenario_ids": []}),
        ),
    ] {
        let mut document = fixture_program();
        document
            .as_object_mut()
            .unwrap()
            .insert(field.to_owned(), payload);
        assert!(
            matches!(hydrate(&document), Err(ProgramError::Json(_))),
            "{field} must not be accepted by the executable program"
        );
    }
}

#[test]
fn every_hydrated_program_field_reaches_runtime_execution_or_load_validation() {
    // If a section were deserialized and then discarded, deleting it would still hydrate.
    // Each required section must therefore be load-fatal when removed.
    for field in [
        "format",
        "version",
        "project_id",
        "brain_id",
        "enabled_languages",
        "default_language",
        "source_packages",
        "packages",
        "semantic",
        "conversation",
        "capabilities",
        "assets",
    ] {
        let mut document = fixture_program();
        document.as_object_mut().unwrap().remove(field);
        assert!(
            hydrate(&document).is_err(),
            "{field} is declared by the executable program but removing it is not load-fatal"
        );
    }
}

#[test]
fn runtime_rejects_a_non_v1_program() {
    let mut document = fixture_program();
    document["version"] = serde_json::json!(2);
    assert!(matches!(
        hydrate(&document),
        Err(ProgramError::UnsupportedVersion(2))
    ));
}

#[test]
fn runtime_rejects_required_collection_metadata_without_elicitation() {
    let mut document = fixture_program();
    document["semantic"]["patterns"][0]["slots"] = serde_json::json!([{
        "name": "count",
        "kind": {"type": "number"},
        "required": true,
        "elicitation": []
    }]);
    assert!(matches!(
        hydrate(&document),
        Err(ProgramError::InvalidSemanticCatalog(_))
    ));
}

#[test]
fn runtime_fails_closed_when_shipped_semantics_cannot_build_an_index() {
    let mut document = fixture_program();
    document["semantic"]["patterns"][0]["samples"] =
        serde_json::json!([{"language": "fa", "text": "salaam"}]);
    assert!(matches!(
        hydrate(&document),
        Err(ProgramError::InvalidSemanticIndex(_))
    ));
}

#[test]
fn semantic_config_hydration_rejects_out_of_range_values() {
    let invalid = SemanticConfigDoc {
        candidate_limit: 1,
        resolution_threshold: 0.45,
        ambiguity_margin: 0.04,
        resolver_min_confidence: 0.55,
        resolver_candidate_limit: 8,
    };
    assert!(matches!(
        invalid.into_runtime(),
        Err(ProgramError::InvalidSemanticConfig(_))
    ));

    let invalid = SemanticConfigDoc {
        candidate_limit: SemanticConfig::default().candidate_limit,
        resolution_threshold: -0.01,
        ambiguity_margin: 0.04,
        resolver_min_confidence: 0.55,
        resolver_candidate_limit: 8,
    };
    assert!(matches!(
        invalid.into_runtime(),
        Err(ProgramError::InvalidSemanticConfig(_))
    ));

    let invalid = SemanticConfigDoc {
        candidate_limit: SemanticConfig::default().candidate_limit,
        resolution_threshold: 0.45,
        ambiguity_margin: 0.04,
        resolver_min_confidence: 1.01,
        resolver_candidate_limit: 8,
    };
    assert!(matches!(
        invalid.into_runtime(),
        Err(ProgramError::InvalidSemanticConfig(_))
    ));
}

#[test]
fn semantic_profiles_must_exactly_cover_enabled_languages() {
    let profile = SemanticProfile::empty();
    let enabled = BTreeSet::from(["en-us".to_owned(), "fa-ir".to_owned()]);

    let complete = BTreeMap::from([
        ("en-us".to_owned(), profile.clone()),
        ("fa-ir".to_owned(), profile.clone()),
    ]);
    assert!(validate_semantic_profile_coverage(&complete, &enabled).is_ok());

    let missing = BTreeMap::from([("en-us".to_owned(), profile.clone())]);
    assert!(matches!(
        validate_semantic_profile_coverage(&missing, &enabled),
        Err(ProgramError::InvalidLanguageContract(_))
    ));

    let extra = BTreeMap::from([
        ("en-us".to_owned(), profile.clone()),
        ("fa-ir".to_owned(), profile.clone()),
        ("fa-latn".to_owned(), profile),
    ]);
    assert!(matches!(
        validate_semantic_profile_coverage(&extra, &enabled),
        Err(ProgramError::InvalidLanguageContract(_))
    ));
}
