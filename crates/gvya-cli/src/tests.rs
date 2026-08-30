//! CLI boundary tests.

use super::*;

fn fixture_project() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../validation/fixtures/source-minimal/gvya.project.json")
}

#[test]
fn source_schema_argument_contract_is_machine_json_only() {
    assert_eq!(parse_schema_args(&["--json".into()]).unwrap(), None);
    assert_eq!(
        parse_schema_args(&["--kind".into(), "behavior".into(), "--json".into()]).unwrap(),
        Some("behavior".into())
    );
    assert!(parse_schema_args(&[]).is_err());
    assert!(parse_schema_args(&["--kind".into(), "behavior".into()]).is_err());
}

#[test]
fn source_schema_exposes_parser_owned_behavior_shape() {
    let report = source_contract_json(Some("behavior")).unwrap();
    assert_eq!(report["format"], "gvya.cli.source-schema");
    assert_eq!(report["version"], 1);
    let fields = report["object"]["fields"].as_array().unwrap();
    assert!(
        fields
            .iter()
            .any(|row| row["name"] == "meaning" && row["required"] == true)
    );
    assert!(
        fields
            .iter()
            .any(|row| row["name"] == "responses" && row["required"] == true)
    );
}

#[test]
fn inspect_argument_contract_supports_exact_authored_source_lookup() {
    let (project, kind, id) = parse_inspect_args(&[
        "demo".into(),
        "--kind".into(),
        "behavior".into(),
        "--id".into(),
        "hello".into(),
        "--json".into(),
    ])
    .unwrap();
    assert_eq!(project, PathBuf::from("demo"));
    assert_eq!(kind.as_deref(), Some("behavior"));
    assert_eq!(id.as_deref(), Some("hello"));
    assert!(parse_inspect_args(&["--id".into(), "hello".into(), "--json".into()]).is_err());
    assert!(parse_inspect_args(&["--kind".into(), "behavior".into()]).is_err());
}

#[test]
fn inspect_authored_source_returns_raw_behavior_and_nested_response_provenance() {
    let tree = load_source_tree(&fixture_project(), SourceLimits::default()).unwrap();
    let behavior =
        source_object_inventory_json(&tree, "behavior", Some("hello"), SourceLimits::default())
            .unwrap();
    assert_eq!(behavior["count"], 1);
    assert_eq!(behavior["items"][0]["location"]["namespace"], "behaviors");
    assert_eq!(behavior["items"][0]["value"]["meaning"], "hello");

    let response = source_object_inventory_json(
        &tree,
        "response",
        Some("hello.answer"),
        SourceLimits::default(),
    )
    .unwrap();
    assert_eq!(response["count"], 1);
    assert_eq!(response["items"][0]["location"]["owner_kind"], "behavior");
    assert_eq!(response["items"][0]["location"]["owner_id"], "hello");
}

#[test]
fn canonical_cli_loader_builds_the_source_fixture() {
    let tree = load_source_tree(&fixture_project(), SourceLimits::default()).unwrap();
    let result = build_source_project(&tree, BuildOptions::default(), None).unwrap();
    assert!(!result.artifact.is_empty());
    assert_eq!(
        result
            .manifest
            .get("project_id")
            .and_then(serde_json::Value::as_str),
        Some("fixture")
    );
}

#[test]
fn canonical_cli_test_command_runs_compiled_runtime_path() {
    command_test(&[fixture_project().to_string_lossy().into_owned()]).unwrap();
}

#[test]
fn build_requires_the_portable_artifact_extension() {
    assert!(parse_build_args(&["--output".into(), "brain.bin".into()]).is_err());
    assert!(parse_build_args(&["--output".into(), "brain.gvya".into()]).is_ok());
}

#[test]
fn check_accepts_the_default_source_fixture() {
    let (report, accepted) =
        check_report(&fixture_project(), &AuthoringAcceptancePolicy::default()).unwrap();
    assert!(accepted);
    assert_eq!(
        report.get("format").and_then(serde_json::Value::as_str),
        Some("gvya.cli.check")
    );
    assert_eq!(
        report
            .get("obligations")
            .and_then(serde_json::Value::as_array)
            .map(Vec::len),
        Some(0)
    );
}

#[test]
fn check_can_require_authored_tests() {
    let policy = AuthoringAcceptancePolicy {
        require_tests: true,
        ..AuthoringAcceptancePolicy::default()
    };
    let (report, accepted) = check_report(&fixture_project(), &policy).unwrap();
    assert!(!accepted);
    assert!(
        report["obligations"]
            .as_array()
            .unwrap()
            .iter()
            .any(|row| row["code"] == "authoring.tests_required")
    );
}

#[test]
fn check_arguments_are_machine_json_by_default() {
    assert_eq!(
        parse_check_args(&[
            "project.json".into(),
            "--policy".into(),
            "acceptance.json".into(),
        ])
        .unwrap(),
        (
            PathBuf::from("project.json"),
            Some(PathBuf::from("acceptance.json"))
        )
    );
    assert!(parse_check_args(&["--json".into()]).is_err());
}

#[test]
fn check_reports_an_unreadable_source_as_json() {
    let missing = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("__missing_authoring_fixture__")
        .join("gvya.project.json");
    let (report, accepted) = check_report(&missing, &AuthoringAcceptancePolicy::default()).unwrap();
    assert!(!accepted);
    assert_eq!(
        report["obligations"][0]["code"],
        "authoring.source_unreadable"
    );
    assert_eq!(report["reports"]["source"]["stage"], "load");
}

#[test]
fn init_bot_creates_a_complete_checkable_canonical_source_root() {
    let output = unique_test_path("init-bot");
    command_init(&[
        "bot".into(),
        output.to_string_lossy().into_owned(),
        "--project-id".into(),
        "test-project".into(),
        "--bot-id".into(),
        "test-bot".into(),
        "--languages".into(),
        "en-US,fa-IR".into(),
        "--enabled-languages".into(),
        "en-US".into(),
    ])
    .unwrap();
    let project: serde_json::Value =
        serde_json::from_slice(&fs::read(output.join("gvya.project.json")).unwrap()).unwrap();
    assert_eq!(project["project_id"], "test-project");
    assert_eq!(project["brain_id"], "test-bot");
    assert_eq!(project["languages"], serde_json::json!(["en-US", "fa-IR"]));
    assert_eq!(project["enabled_languages"], serde_json::json!(["en-US"]));
    let (report, accepted) = check_report(&output, &AuthoringAcceptancePolicy::default()).unwrap();
    assert!(accepted, "{report}");
    fs::remove_dir_all(output).unwrap();
}

#[test]
fn cli_loader_rejects_physical_undeclared_package_fragment() {
    let output = unique_test_path("undeclared-fragment");
    command_init(&[
        "bot".into(),
        output.to_string_lossy().into_owned(),
        "--project-id".into(),
        "test-project".into(),
        "--bot-id".into(),
        "test-bot".into(),
    ])
    .unwrap();
    let orphan_dir = output
        .join("packages")
        .join("standard")
        .join("test-bot.core")
        .join("fragments")
        .join("behaviors");
    fs::create_dir_all(&orphan_dir).unwrap();
    fs::write(
        orphan_dir.join("orphan.json"),
        br#"{"id":"orphan","value":{"id":"orphan","meaning":"none","responses":[]}}"#,
    )
    .unwrap();
    let diagnostic = load_source_tree_diagnostic(&output, SourceLimits::default()).unwrap_err();
    assert_eq!(diagnostic.code, "source.fragment_undeclared");
    assert!(diagnostic.message.contains("undeclared Package fragment"));
    assert!(
        diagnostic
            .path
            .as_deref()
            .is_some_and(|path| path.ends_with("fragments/behaviors/orphan.json"))
    );
    fs::remove_dir_all(output).unwrap();
}

#[test]
fn init_package_is_non_overwriting_and_standalone_checkable() {
    let output = unique_test_path("init-package");
    let args = [
        "package".into(),
        output.to_string_lossy().into_owned(),
        "--kind".into(),
        "fallback".into(),
        "--authoring-language".into(),
        "fa-IR".into(),
    ];
    command_init(&args).unwrap();
    assert!(output.join("package.json").is_file());
    assert!(output.join("authoring.json").is_file());
    assert!(command_init(&args).is_err());
    let tree = package_check_tree_for_test(&output).unwrap();
    let (report, accepted) =
        check_tree_report(&tree, &AuthoringAcceptancePolicy::default()).unwrap();
    assert!(accepted, "{report}");
    fs::remove_dir_all(output).unwrap();
}

#[test]
fn standalone_package_check_exposes_exact_duplicate_sample_owners() {
    let output = unique_test_path("duplicate-package");
    fs::create_dir_all(output.join("fragments/meanings")).unwrap();
    let package = serde_json::json!({
        "format": "gvya.source.package",
        "version": 1,
        "manifest": {"id": "duplicate", "kind": "standard", "dependencies": []},
        "fragments": {
            "meanings": [
                "fragments/meanings/0001-first.json",
                "fragments/meanings/0002-second.json"
            ]
        }
    });
    fs::write(
        output.join("package.json"),
        serde_json::to_vec_pretty(&package).unwrap(),
    )
    .unwrap();
    fs::write(
        output.join("fragments/meanings/0001-first.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "id": "first",
            "value": {"id": "first", "samples": [{"language": "en-US", "text": "Hello!"}]}
        }))
        .unwrap(),
    )
    .unwrap();
    fs::write(
        output.join("fragments/meanings/0002-second.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "id": "second",
            "value": {"id": "second", "samples": [{"language": "en-US", "text": "hello"}]}
        }))
        .unwrap(),
    )
    .unwrap();
    let tree = package_check_tree_for_test(&output).unwrap();
    let (report, accepted) =
        check_tree_report(&tree, &AuthoringAcceptancePolicy::default()).unwrap();
    assert!(!accepted);
    let issue = report["reports"]["audit"]["issues"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["code"] == "semantic.sample_duplicate_cross_meaning")
        .unwrap();
    assert_eq!(issue["details"]["sample"], "hello");
    assert_eq!(issue["details"]["meanings"], "first, second");
    fs::remove_dir_all(output).unwrap();
}

fn unique_test_path(label: &str) -> PathBuf {
    let serial = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!("gvya-cli-{label}-{}-{serial}", std::process::id()))
}

#[test]
fn check_change_accepts_identical_fixture_without_running_unnecessary_tests() {
    let fixture = fixture_project().to_string_lossy().into_owned();
    command_check_change(&[fixture.clone(), fixture, "--json".into()]).unwrap();
}

#[test]
fn check_change_argument_contract_requires_exact_base_and_candidate() {
    let (base, candidate, json) =
        parse_check_change_args(&["base".into(), "candidate".into(), "--json".into()]).unwrap();
    assert_eq!(base, PathBuf::from("base"));
    assert_eq!(candidate, PathBuf::from("candidate"));
    assert!(json);
    assert!(parse_check_change_args(&["only-one".into()]).is_err());
    assert!(parse_check_change_args(&["a".into(), "b".into(), "c".into()]).is_err());
}

#[test]
fn author_step_argument_contract_is_machine_json_only() {
    assert_eq!(
        parse_author_step_args(&["base".into(), "candidate".into(), "--json".into()]).unwrap(),
        (PathBuf::from("base"), PathBuf::from("candidate"))
    );
    assert!(parse_author_step_args(&["base".into(), "candidate".into()]).is_err());
    assert!(parse_author_step_args(&["base".into(), "candidate".into(), "extra".into()]).is_err());
}

#[test]
fn author_step_serializes_ready_to_promote_without_recomputing_gate_policy() {
    let decision = AuthoringLoopDecision {
        state: gvya_compiler::authoring::AuthoringLoopState::ReadyToPromote,
        accepted: true,
        promotion_allowed: true,
        actions: vec![AuthoringAction::PromoteCandidate],
    };
    let report = author_step_report_json(
        &decision,
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
        serde_json::json!({"format":"gvya.cli.check-change","version":1,"accepted":true}),
    );
    assert_eq!(report["format"], "gvya.cli.author-step");
    assert_eq!(report["version"], 1);
    assert_eq!(report["state"], "ready_to_promote");
    assert_eq!(report["primary_action"], "promote_candidate");
    assert_eq!(report["gate"]["format"], "gvya.cli.check-change");
    assert_eq!(report["gate"]["version"], 1);
    assert_eq!(
        report["source_identity"]["base"],
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
    );
    assert_eq!(
        report["source_identity"]["candidate"],
        "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
    );
    assert_eq!(report["promotion"]["allowed"], true);
    assert_eq!(report["promotion"]["identity"].as_str().unwrap().len(), 64);
}

#[test]
fn promotion_identity_is_stable_and_binds_ordered_source_pair() {
    let left = promotion_identity("a", "b");
    assert_eq!(left, promotion_identity("a", "b"));
    assert_ne!(left, promotion_identity("a", "c"));
    assert_ne!(left, promotion_identity("b", "a"));
    assert_eq!(left.len(), 64);
}

#[test]
fn author_step_candidate_source_failure_is_repairable_but_base_failure_is_blocking() {
    let candidate_diagnostics = vec![generic_diagnostic(
        "candidate",
        "source_resolution",
        "source.invalid",
        "invalid candidate source",
        "repair candidate source",
    )];
    let candidate = author_step_preflight_report_json(
        "candidate",
        "source_resolution",
        None,
        None,
        &candidate_diagnostics,
    );
    assert_eq!(candidate["state"], "repair_required");
    assert_eq!(candidate["candidate_policy"], "repair_candidate_only");
    assert_eq!(
        candidate["primary_action"],
        "resolve_candidate_source_failure"
    );
    assert_eq!(candidate["gate"], serde_json::Value::Null);

    let base_diagnostics = vec![generic_diagnostic(
        "base",
        "source_load",
        "source.load_failed",
        "missing baseline",
        "restore accepted baseline",
    )];
    let base =
        author_step_preflight_report_json("base", "source_load", None, None, &base_diagnostics);
    assert_eq!(base["state"], "blocked");
    assert_eq!(base["candidate_policy"], "do_not_mutate_candidate");
    assert_eq!(base["primary_action"], "restore_valid_accepted_baseline");
    assert_eq!(base["next_actions"].as_array().map(Vec::len), Some(1));
}

fn authoring_fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../validation/authoring-e2e/fixtures")
        .join(name)
}

fn has_authoring_action(decision: &AuthoringLoopDecision, kind: &str) -> bool {
    decision.actions.iter().any(|action| action.kind() == kind)
}

#[test]
fn real_source_authoring_meaning_behavior_slice_requires_then_satisfies_direct_proof() {
    let base = authoring_fixture("00-base");
    let missing = evaluate_change_gate(
        &base,
        &authoring_fixture("01-meaning-behavior-missing-proof"),
    )
    .unwrap();
    assert!(!missing.decision.accepted);
    assert!(missing.plan.mechanic_proof_missing);
    assert!(has_authoring_action(
        &missing.decision,
        "add_direct_mechanic_proof"
    ));

    let accepted =
        evaluate_change_gate(&base, &authoring_fixture("01-meaning-behavior-accepted")).unwrap();
    assert!(accepted.decision.accepted);
    assert!(accepted.decision.promotion_allowed);
    assert!(has_authoring_action(
        &accepted.decision,
        "promote_candidate"
    ));
}

#[test]
fn real_source_authoring_stateful_and_capability_slices_reach_promotion_only_with_proof() {
    for (base, missing, accepted) in [
        (
            "01-meaning-behavior-accepted",
            "02-stateful-missing-proof",
            "02-stateful-accepted",
        ),
        (
            "02-stateful-accepted",
            "03-capability-missing-proof",
            "03-capability-accepted",
        ),
    ] {
        let missing =
            evaluate_change_gate(&authoring_fixture(base), &authoring_fixture(missing)).unwrap();
        assert!(!missing.decision.accepted);
        assert!(has_authoring_action(
            &missing.decision,
            "add_direct_mechanic_proof"
        ));

        let accepted =
            evaluate_change_gate(&authoring_fixture(base), &authoring_fixture(accepted)).unwrap();
        assert!(accepted.decision.accepted);
        assert!(has_authoring_action(
            &accepted.decision,
            "promote_candidate"
        ));
    }
}

#[test]
fn real_source_authoring_mechanic_removal_requires_repaired_manual_scenario() {
    let base = authoring_fixture("03-capability-accepted");
    let missing =
        evaluate_change_gate(&base, &authoring_fixture("04-removal-missing-proof")).unwrap();
    assert!(!missing.decision.accepted);

    let accepted = evaluate_change_gate(&base, &authoring_fixture("04-removal-accepted")).unwrap();
    assert!(accepted.decision.accepted);
    assert!(has_authoring_action(
        &accepted.decision,
        "promote_candidate"
    ));
}

#[test]
fn real_source_authoring_malformed_candidate_is_preflight_repairable_then_accepts_repaired_slice() {
    let base = authoring_fixture("04-removal-accepted");
    let error = match evaluate_change_gate(&base, &authoring_fixture("05-malformed")) {
        Ok(_) => panic!("malformed candidate unexpectedly composed"),
        Err(error) => error,
    };
    assert_eq!(error.side, ChangeGateInputSide::Candidate);
    // Declared asset discovery parses every package during tree load, so malformed
    // package JSON is rejected at load time and never reaches source resolution.
    assert_eq!(error.stage, "source_load");
    assert_eq!(error.base_fingerprint.as_deref().map(str::len), Some(64));
    assert!(error.candidate_fingerprint.is_none());
    assert!(error.diagnostics.iter().any(|diagnostic| {
        diagnostic.code == "source.invalid_json"
            && diagnostic
                .path
                .as_deref()
                .is_some_and(|path| path.contains("packages/base/package.json"))
    }));
    assert!(error.diagnostics.iter().any(|diagnostic| {
        diagnostic.message.contains("packages/base/package.json")
            || diagnostic
                .path
                .as_deref()
                .is_some_and(|path| path.contains("packages/base/package.json"))
    }));
    let report = author_step_preflight_report_json(
        error.side.label(),
        error.stage,
        error.base_fingerprint.as_deref(),
        error.candidate_fingerprint.as_deref(),
        &error.diagnostics,
    );
    assert_eq!(report["state"], "repair_required");
    assert_eq!(report["primary_action"], "resolve_candidate_source_failure");
    assert_eq!(
        report["source_identity"]["base"].as_str().map(str::len),
        Some(64)
    );
    assert!(report["source_identity"]["candidate"].is_null());

    let repaired = evaluate_change_gate(&base, &authoring_fixture("05-repaired")).unwrap();
    assert!(repaired.decision.accepted);
    assert!(has_authoring_action(
        &repaired.decision,
        "promote_candidate"
    ));
}

#[test]
fn real_source_authoring_failing_selected_regression_targets_only_failed_test_then_repairs() {
    let base = authoring_fixture("05-repaired");
    let failing = evaluate_change_gate(&base, &authoring_fixture("06-failing-regression")).unwrap();
    assert!(!failing.decision.accepted);
    assert!(failing.decision.actions.iter().any(|action| matches!(
        action,
        AuthoringAction::ResolveRegressionFailure { test_id, .. }
            if test_id == "weather.ask.proof"
    )));

    let fixed = evaluate_change_gate(&base, &authoring_fixture("06-fixed-regression")).unwrap();
    assert!(fixed.decision.accepted);
    assert!(has_authoring_action(&fixed.decision, "promote_candidate"));
}

#[test]
fn real_source_authoring_global_change_escalates_to_full_suite_and_can_promote() {
    let evaluation = evaluate_change_gate(
        &authoring_fixture("06-fixed-regression"),
        &authoring_fixture("07-global-change"),
    )
    .unwrap();
    assert!(evaluation.plan.change_set.semantic_config_changed);
    assert!(evaluation.plan.full_suite_required);
    assert!(
        evaluation
            .plan
            .full_suite_reasons
            .contains(&"semantic_config_changed".to_string())
    );
    assert!(evaluation.decision.accepted);
}

#[test]
fn real_source_authoring_sequential_slices_are_independently_acceptable() {
    let first = evaluate_change_gate(
        &authoring_fixture("06-fixed-regression"),
        &authoring_fixture("08-sequential-a"),
    )
    .unwrap();
    assert!(first.decision.accepted);

    let second = evaluate_change_gate(
        &authoring_fixture("08-sequential-a"),
        &authoring_fixture("08-sequential-b"),
    )
    .unwrap();
    assert!(second.decision.accepted);
}
