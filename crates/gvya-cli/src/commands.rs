//! CLI command adapters.

use super::*;

pub(super) fn command_schema(args: &[String]) -> Result<(), String> {
    let kind = parse_schema_args(args)?;
    let report = source_contract_json(kind.as_deref())?;
    print_json(&report)
}

pub(super) fn command_build(args: &[String]) -> Result<(), String> {
    let (project, output) = parse_build_args(args)?;
    let tree = load_source_tree(&project, SourceLimits::default())?;
    let result = build_source_project(&tree, BuildOptions::default(), None)
        .map_err(|error| format!("build failed: {error:?}"))?;
    if let Some(parent) = output.parent().filter(|path| !path.as_os_str().is_empty()) {
        fs::create_dir_all(parent)
            .map_err(|error| format!("cannot create output directory: {error}"))?;
    }
    fs::write(&output, &result.artifact)
        .map_err(|error| format!("cannot write {}: {error}", output.display()))?;
    println!("built {}", output.display());
    println!("artifact_sha256={}", result.artifact_digest);
    println!("content_root={}", result.content_root);
    Ok(())
}

pub(super) fn command_audit(args: &[String]) -> Result<(), String> {
    let (project, json_output) = parse_project_machine_args(args, "audit")?;
    let tree = load_source_tree(&project, SourceLimits::default())?;
    let resolved = resolve_source_project(&tree, SourceLimits::default())
        .map_err(|issues| format!("source resolution failed: {issues:?}"))?;
    let report = Auditor::new(AuditorLimits::default())
        .audit(&resolved.packages, &resolved.semantic_profiles);
    if json_output {
        print_json(&audit_report_json(&report))?;
    } else {
        for issue in &report.issues {
            println!(
                "{} {}: {}",
                issue.severity.label(),
                issue.code.as_str(),
                issue.summary
            );
        }
        println!(
            "errors={} warnings={} info={} truncated={}",
            report.summary.errors, report.summary.warnings, report.summary.info, report.truncated
        );
    }
    if report.is_clean() {
        Ok(())
    } else {
        Err("audit contains errors".into())
    }
}

pub(super) fn command_test(args: &[String]) -> Result<(), String> {
    let (project_path, json_output) = parse_project_machine_args(args, "test")?;
    let limits = SourceLimits::default();
    let tree = load_source_tree(&project_path, limits)?;
    let resolved = resolve_source_project(&tree, limits)
        .map_err(|issues| format!("source resolution failed: {issues:?}"))?;
    let composition = compose_packages(&resolved.packages, &resolved.semantic_profiles);
    let project = composition
        .project
        .ok_or_else(|| format!("composition failed: {:?}", composition.issues))?;
    let built = build_source_project(&tree, BuildOptions::default(), None)
        .map_err(|error| format!("build before test failed: {error:?}"))?;
    let runtime = Runtime::load(built.artifact, LoadPolicy::default(), None)
        .map_err(|error| format!("compiled artifact cannot load: {error:?}"))?;
    let mut driver = CanonicalRuntimeDriver { runtime };
    let report = run_test_suite(&mut driver, &project.tests, TestRunLimits::default());
    if json_output {
        print_json(&test_report_json(&report))?;
    } else {
        for case in &report.cases {
            if !case.ok {
                eprintln!("FAIL regression {}: {:?}", case.id.as_str(), case.failures);
            }
        }
        for scenario in &report.scenarios {
            if !scenario.ok {
                eprintln!("FAIL scenario {}", scenario.id.as_str());
            }
        }
        println!(
            "regression_passed={}/{} executed={} scenarios_passed={}/{} executed={} stopped_early={}",
            report.regression_passed,
            report.regression_total,
            report.regression_executed,
            report.scenario_passed,
            report.scenario_total,
            report.scenario_executed,
            report.stopped_early,
        );
    }
    if report.regression_executed == report.regression_total
        && report.regression_passed == report.regression_total
        && report.scenario_executed == report.scenario_total
        && report.scenario_passed == report.scenario_total
        && !report.stopped_early
    {
        Ok(())
    } else {
        Err("authored regression/scenario suite failed".into())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ChangeGateInputSide {
    Base,
    Candidate,
}

impl ChangeGateInputSide {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Base => "base",
            Self::Candidate => "candidate",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct ChangeGateSetupError {
    pub side: ChangeGateInputSide,
    pub stage: &'static str,
    pub base_fingerprint: Option<String>,
    pub candidate_fingerprint: Option<String>,
    pub diagnostics: Vec<AuthoringDiagnostic>,
}

impl std::fmt::Display for ChangeGateSetupError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let detail = self
            .diagnostics
            .first()
            .map_or("unknown authoring preflight failure", |diagnostic| {
                diagnostic.message.as_str()
            });
        write!(
            formatter,
            "{} {} failed: {detail}",
            self.side.label(),
            self.stage
        )
    }
}

pub(super) struct ChangeGateEvaluation {
    pub plan: ChangeTestPlan,
    pub base_fingerprint: String,
    pub candidate_fingerprint: String,
    pub build_diagnostics: Vec<AuthoringDiagnostic>,
    pub runtime_diagnostics: Vec<AuthoringDiagnostic>,
    pub test_report: Option<gvya_compiler::testing::TestRunReport>,
    pub decision: AuthoringLoopDecision,
}

pub(super) fn evaluate_change_gate(
    base_path: &Path,
    candidate_path: &Path,
) -> Result<ChangeGateEvaluation, ChangeGateSetupError> {
    let limits = SourceLimits::default();
    let base_tree =
        load_source_tree_diagnostic(base_path, limits).map_err(|issue| ChangeGateSetupError {
            side: ChangeGateInputSide::Base,
            stage: "source_load",
            base_fingerprint: None,
            candidate_fingerprint: None,
            diagnostics: vec![source_load_diagnostic(
                "base",
                &issue,
                "restore a readable accepted BASE source tree before authoring continues",
            )],
        })?;
    let base_fingerprint = base_tree.fingerprint_sha256();
    let base_resolved =
        resolve_source_project(&base_tree, limits).map_err(|issues| ChangeGateSetupError {
            side: ChangeGateInputSide::Base,
            stage: "source_resolution",
            base_fingerprint: Some(base_fingerprint.clone()),
            candidate_fingerprint: None,
            diagnostics: source_diagnostics("base", "source_resolution", &issues),
        })?;
    let base_composition =
        compose_packages(&base_resolved.packages, &base_resolved.semantic_profiles);
    let base = base_composition
        .project
        .ok_or_else(|| ChangeGateSetupError {
            side: ChangeGateInputSide::Base,
            stage: "composition",
            base_fingerprint: Some(base_fingerprint.clone()),
            candidate_fingerprint: None,
            diagnostics: composition_diagnostics("base", &base_composition.issues),
        })?;

    let candidate_tree = load_source_tree_diagnostic(candidate_path, limits).map_err(|issue| {
        ChangeGateSetupError {
            side: ChangeGateInputSide::Candidate,
            stage: "source_load",
            base_fingerprint: Some(base_fingerprint.clone()),
            candidate_fingerprint: None,
            diagnostics: vec![source_load_diagnostic(
                "candidate",
                &issue,
                "repair the CANDIDATE source tree and rerun author-step",
            )],
        }
    })?;
    let candidate_fingerprint = candidate_tree.fingerprint_sha256();

    let candidate_resolved =
        resolve_source_project(&candidate_tree, limits).map_err(|issues| ChangeGateSetupError {
            side: ChangeGateInputSide::Candidate,
            stage: "source_resolution",
            base_fingerprint: Some(base_fingerprint.clone()),
            candidate_fingerprint: Some(candidate_fingerprint.clone()),
            diagnostics: source_diagnostics("candidate", "source_resolution", &issues),
        })?;
    let candidate_composition = compose_packages(
        &candidate_resolved.packages,
        &candidate_resolved.semantic_profiles,
    );
    let candidate = candidate_composition
        .project
        .ok_or_else(|| ChangeGateSetupError {
            side: ChangeGateInputSide::Candidate,
            stage: "composition",
            base_fingerprint: Some(base_fingerprint.clone()),
            candidate_fingerprint: Some(candidate_fingerprint.clone()),
            diagnostics: composition_diagnostics("candidate", &candidate_composition.issues),
        })?;

    let base_surface = ProjectSourceSurface {
        project_id: base_resolved.project.project_id.clone(),
        brain_id: base_resolved.project.brain_id.clone(),
        languages: base_resolved.project.languages.clone(),
        enabled_languages: base_resolved.project.enabled_languages.clone(),
        default_language: base_resolved.project.default_language.clone(),
        semantic_config: base_resolved.project.semantic_config.clone(),
        conversation_config: base_resolved.project.conversation_config.clone(),
        emit_debug_map: base_resolved.project.emit_debug_map,
    };
    let candidate_surface = ProjectSourceSurface {
        project_id: candidate_resolved.project.project_id.clone(),
        brain_id: candidate_resolved.project.brain_id.clone(),
        languages: candidate_resolved.project.languages.clone(),
        enabled_languages: candidate_resolved.project.enabled_languages.clone(),
        default_language: candidate_resolved.project.default_language.clone(),
        semantic_config: candidate_resolved.project.semantic_config.clone(),
        conversation_config: candidate_resolved.project.conversation_config.clone(),
        emit_debug_map: candidate_resolved.project.emit_debug_map,
    };
    let plan = plan_change_tests(
        &base,
        &candidate,
        &base_surface,
        &candidate_surface,
        ChangePlanLimits::default(),
    );
    let selected_suite = plan.selected_suite(&candidate);

    let mut build_diagnostics = Vec::new();
    let mut runtime_diagnostics = Vec::new();
    let mut test_report = None;
    if !plan.change_set.is_empty() {
        match build_source_project(&candidate_tree, BuildOptions::default(), None) {
            Ok(built) => match Runtime::load(built.artifact, LoadPolicy::default(), None) {
                Ok(runtime) => {
                    let mut driver = CanonicalRuntimeDriver { runtime };
                    test_report = Some(run_test_suite(
                        &mut driver,
                        &selected_suite,
                        TestRunLimits::default(),
                    ));
                }
                Err(error) => runtime_diagnostics = diagnostics_for_runtime("candidate", &error),
            },
            Err(error) => build_diagnostics = diagnostics_for_build("candidate", &error),
        }
    }
    let decision = plan_authoring_step(
        &plan,
        !build_diagnostics.is_empty(),
        !runtime_diagnostics.is_empty(),
        test_report.as_ref(),
    );

    Ok(ChangeGateEvaluation {
        plan,
        base_fingerprint,
        candidate_fingerprint,
        build_diagnostics,
        runtime_diagnostics,
        test_report,
        decision,
    })
}

pub(super) fn command_check_change(args: &[String]) -> Result<(), String> {
    let (base_path, candidate_path, json_output) = parse_check_change_args(args)?;
    let evaluation =
        evaluate_change_gate(&base_path, &candidate_path).map_err(|error| error.to_string())?;
    let report = check_change_report_json(
        &evaluation.plan,
        &evaluation.base_fingerprint,
        &evaluation.candidate_fingerprint,
        &evaluation.build_diagnostics,
        &evaluation.runtime_diagnostics,
        evaluation.test_report.as_ref(),
        evaluation.decision.accepted,
    );
    if json_output {
        print_json(&report)?;
    } else {
        println!(
            "accepted={} changes={} runtime_changes={} changed_tests={} mechanic_proof_tests={} mechanics={}/{} selected_tests={} full_suite_required={}",
            evaluation.decision.accepted,
            evaluation.plan.change_set.changes.len(),
            evaluation.plan.change_set.runtime_changes(),
            evaluation.plan.changed_test_count,
            evaluation.plan.proof_test_count,
            evaluation
                .plan
                .mechanic_requirements
                .iter()
                .filter(|requirement| requirement.covered())
                .count(),
            evaluation.plan.mechanic_requirements.len(),
            evaluation.plan.selected.len(),
            evaluation.plan.full_suite_required
        );
        if evaluation.plan.mechanic_proof_missing {
            for requirement in evaluation
                .plan
                .mechanic_requirements
                .iter()
                .filter(|requirement| !requirement.covered())
            {
                eprintln!(
                    "FAIL missing mechanic proof: {} {}:{}",
                    requirement.mechanic.label(),
                    requirement
                        .source_kind
                        .map_or("project", |kind| kind.label()),
                    requirement.subject
                );
            }
        }
        for diagnostic in &evaluation.build_diagnostics {
            eprintln!("FAIL build {}: {}", diagnostic.code, diagnostic.message);
        }
        for diagnostic in &evaluation.runtime_diagnostics {
            eprintln!("FAIL runtime {}: {}", diagnostic.code, diagnostic.message);
        }
        if let Some(tests) = evaluation.test_report.as_ref() {
            println!(
                "selected_regression_passed={}/{} selected_scenarios_passed={}/{}",
                tests.regression_passed,
                tests.regression_total,
                tests.scenario_passed,
                tests.scenario_total
            );
        }
    }
    if evaluation.decision.accepted {
        Ok(())
    } else {
        Err("incremental change gate rejected the candidate".into())
    }
}

pub(super) fn command_author_step(args: &[String]) -> Result<(), String> {
    let (base_path, candidate_path) = parse_author_step_args(args)?;
    let evaluation = match evaluate_change_gate(&base_path, &candidate_path) {
        Ok(evaluation) => evaluation,
        Err(error) => {
            let report = author_step_preflight_report_json(
                error.side.label(),
                error.stage,
                error.base_fingerprint.as_deref(),
                error.candidate_fingerprint.as_deref(),
                &error.diagnostics,
            );
            print_json(&report)?;
            return Err(if error.side == ChangeGateInputSide::Candidate {
                "authoring step requires candidate source repair".into()
            } else {
                "authoring step is blocked by an invalid accepted baseline".into()
            });
        }
    };
    let gate_report = check_change_report_json(
        &evaluation.plan,
        &evaluation.base_fingerprint,
        &evaluation.candidate_fingerprint,
        &evaluation.build_diagnostics,
        &evaluation.runtime_diagnostics,
        evaluation.test_report.as_ref(),
        evaluation.decision.accepted,
    );
    let report = author_step_report_json(
        &evaluation.decision,
        &evaluation.base_fingerprint,
        &evaluation.candidate_fingerprint,
        gate_report,
    );
    print_json(&report)?;
    if evaluation.decision.accepted {
        Ok(())
    } else {
        Err("authoring step requires candidate repair".into())
    }
}

fn check_change_report_json(
    plan: &ChangeTestPlan,
    base_fingerprint: &str,
    candidate_fingerprint: &str,
    build_diagnostics: &[AuthoringDiagnostic],
    runtime_diagnostics: &[AuthoringDiagnostic],
    tests: Option<&gvya_compiler::testing::TestRunReport>,
    accepted: bool,
) -> serde_json::Value {
    let selected = plan.selected.iter().map(|(test,reasons)| serde_json::json!({
        "kind": test.kind(), "id": test.id(),
        "reasons": reasons.iter().map(|reason| serde_json::json!({"code":reason.code,"subject":reason.subject})).collect::<Vec<_>>()
    })).collect::<Vec<_>>();
    let mechanic_requirements = plan
        .mechanic_requirements
        .iter()
        .map(|requirement| {
            serde_json::json!({
                "mechanic": requirement.mechanic.label(),
                "source_kind": requirement.source_kind.map(|kind| kind.label()),
                "subject": requirement.subject,
                "covered": requirement.covered(),
                "covered_by": requirement.covered_by.iter().map(|test| serde_json::json!({
                    "kind": test.kind(), "id": test.id()
                })).collect::<Vec<_>>()
            })
        })
        .collect::<Vec<_>>();
    let mechanics_covered = plan
        .mechanic_requirements
        .iter()
        .filter(|requirement| requirement.covered())
        .count();
    serde_json::json!({
        "format":"gvya.cli.check-change", "version":1, "accepted":accepted,
        "source_identity": {
            "algorithm": "sha256",
            "contract": "gvya.source-tree/1",
            "base": base_fingerprint,
            "candidate": candidate_fingerprint
        },
        "change_set":{
            "package_order_changed":plan.change_set.package_order_changed,
            "semantic_profile_changed":plan.change_set.semantic_profile_changed,
            "project_identity_changed":plan.change_set.project_identity_changed,
            "language_config_changed":plan.change_set.language_config_changed,
            "semantic_config_changed":plan.change_set.semantic_config_changed,
            "conversation_config_changed":plan.change_set.conversation_config_changed,
            "debug_map_changed":plan.change_set.debug_map_changed,
            "runtime_changes":plan.change_set.runtime_changes(),
            "changes":plan.change_set.changes.iter().map(|change|serde_json::json!({"kind":change.kind.label(),"id":change.id,"status":change.status.label()})).collect::<Vec<_>>()
        },
        "impact":{
            "full_suite_required":plan.full_suite_required,
            "full_suite_reasons":plan.full_suite_reasons,
            "semantic_neighbor_pairs_compared":plan.semantic_neighbor_pairs_compared,
            "semantic_neighbor_truncated":plan.semantic_neighbor_truncated,
            "neighbor_meanings":plan.neighbor_meanings,
            "selected_tests":selected,
            "selected_count":plan.selected.len(),
            "changed_test_count":plan.changed_test_count,
            "mechanic_proof_test_count":plan.proof_test_count,
            "mechanic_coverage": {
                "required": plan.mechanic_requirements.len(),
                "covered": mechanics_covered,
                "missing": plan.mechanic_requirements.len().saturating_sub(mechanics_covered),
                "requirements": mechanic_requirements
            },
            "mechanic_proof_missing":plan.mechanic_proof_missing
        },
        "execution":{
            "build_diagnostics": build_diagnostics.iter().map(AuthoringDiagnostic::json).collect::<Vec<_>>(),
            "runtime_diagnostics": runtime_diagnostics.iter().map(AuthoringDiagnostic::json).collect::<Vec<_>>(),
            "tests":tests.map(test_report_json)
        }
    })
}

pub(super) fn command_inspect(args: &[String]) -> Result<(), String> {
    let (project_path, kind, id) = parse_inspect_args(args)?;
    if let Some(kind) = kind {
        let tree = load_source_tree(&project_path, SourceLimits::default())?;
        let report =
            source_object_inventory_json(&tree, &kind, id.as_deref(), SourceLimits::default())?;
        return print_json(&report);
    }
    let (resolved, project) = resolve_composed(&project_path)?;
    let audit = Auditor::new(AuditorLimits::default())
        .audit(&resolved.packages, &resolved.semantic_profiles);
    let analysis = analyze_project(
        &project,
        &resolved.project.semantic_config,
        &resolved.project.conversation_config,
        &resolved.project.project_id,
        &resolved.project.brain_id,
        AnalysisLimits::default(),
    );
    print_json(&serde_json::json!({
        "format": "gvya.cli.inspect", "version": 1,
        "package_order": project.package_order.iter().map(|id| id.as_str()).collect::<Vec<_>>(),
        "counts": {
            "meanings": project.semantic_catalog.patterns().len(),
            "behaviors": project.conversation_catalog.behaviors().len(),
            "capability_result_behaviors": project.conversation_catalog.capability_result_behaviors().len(),
            "capabilities": project.capability_catalog.capability_ids().count(),
            "bindings": project.capability_catalog.bindings().len(),
            "policies": project.capability_catalog.policies().len(),
            "regression_cases": project.tests.regression_cases.len(),
            "scenarios": project.tests.scenarios.len(),
        },
        "meanings": project.semantic_catalog.patterns().iter().map(|row| serde_json::json!({"id": row.id.as_str(), "samples": row.samples.len()})).collect::<Vec<_>>(),
        "behaviors": project.conversation_catalog.behaviors().iter().map(|row| serde_json::json!({"id": row.id.as_str(), "meaning": row.meaning.as_str(), "responses": row.responses.len()})).collect::<Vec<_>>(),
        "capability_result_behaviors": project.conversation_catalog.capability_result_behaviors().iter().map(|row| serde_json::json!({"id": row.id.as_str(), "capability": row.capability.as_str(), "version": row.capability_version.as_str(), "succeeded": row.succeeded, "error_code": row.error_code, "responses": row.responses.len()})).collect::<Vec<_>>(),
        "capabilities": capabilities_json(&project),
        "audit": audit_report_json(&audit),
        "analysis": analysis_json(&analysis),
    }))
}

pub(super) fn command_capabilities(args: &[String]) -> Result<(), String> {
    let (project_path, _) = parse_project_machine_args(args, "capabilities")?;
    let (_, project) = resolve_composed(&project_path)?;
    print_json(
        &serde_json::json!({"format": "gvya.cli.capabilities", "version": 1, "capabilities": capabilities_json(&project)}),
    )
}

pub(super) fn command_capability(args: &[String]) -> Result<(), String> {
    let (project_path, id) = parse_capability_args(args)?;
    let (_, project) = resolve_composed(&project_path)?;
    let definition = project
        .capability_catalog
        .definition(&CapabilityId::new(id.clone()))
        .ok_or_else(|| format!("capability not found: {id}"))?;
    print_json(
        &serde_json::json!({"format": "gvya.cli.capability", "version": 1, "capability": capability_detail_json(definition)}),
    )
}

pub(super) fn command_analysis(args: &[String]) -> Result<(), String> {
    let (project_path, _) = parse_project_machine_args(args, "analysis")?;
    let (resolved, project) = resolve_composed(&project_path)?;
    print_json(&analysis_json(&analyze_project(
        &project,
        &resolved.project.semantic_config,
        &resolved.project.conversation_config,
        &resolved.project.project_id,
        &resolved.project.brain_id,
        AnalysisLimits {
            max_discoverability_probes: 128,
            discoverability_probes_per_meaning: 1,
            ..AnalysisLimits::default()
        },
    )))
}

pub(super) fn command_check(args: &[String]) -> Result<(), String> {
    let (project_path, policy_path) = parse_check_args(args)?;
    let policy = load_acceptance_policy(policy_path.as_deref())?;
    let (report, accepted) = check_report(&project_path, &policy)?;
    print_json(&report)?;
    if accepted {
        Ok(())
    } else {
        Err("authoring acceptance gate rejected the source".into())
    }
}

pub(super) fn load_acceptance_policy(
    policy_path: Option<&Path>,
) -> Result<AuthoringAcceptancePolicy, String> {
    if let Some(path) = policy_path {
        let bytes = read_bounded_file(path, AUTHORING_POLICY_MAX_BYTES, "authoring policy")?;
        let value = serde_json::from_slice::<serde_json::Value>(&bytes)
            .map_err(|error| format!("invalid authoring policy JSON: {error}"))?;
        Ok(AuthoringAcceptancePolicy::parse(&value)?)
    } else {
        Ok(AuthoringAcceptancePolicy::default())
    }
}

pub(super) fn check_report(
    project_path: &Path,
    policy: &AuthoringAcceptancePolicy,
) -> Result<(serde_json::Value, bool), String> {
    let limits = SourceLimits::default();
    let tree = match load_source_tree(project_path, limits) {
        Ok(tree) => tree,
        Err(error) => {
            return Ok(check_source_failure_report(
                policy,
                "authoring.source_unreadable",
                "repair_source_tree",
                "The GVYA source tree could not be loaded.",
                serde_json::json!({"stage": "load", "error": error}),
            ));
        }
    };
    check_tree_report(&tree, policy)
}

pub(super) fn check_tree_report(
    tree: &SourceTree,
    policy: &AuthoringAcceptancePolicy,
) -> Result<(serde_json::Value, bool), String> {
    let limits = SourceLimits::default();
    let resolved = match resolve_source_project(tree, limits) {
        Ok(resolved) => resolved,
        Err(issues) => {
            let issues = issues
                .iter()
                .map(|issue| {
                    serde_json::json!({
                        "code": issue.code,
                        "path": issue.path,
                        "message": issue.message,
                    })
                })
                .collect::<Vec<_>>();
            return Ok(check_source_failure_report(
                policy,
                "authoring.source_invalid",
                "repair_source",
                "GVYA source validation failed before composition.",
                serde_json::json!({"stage": "resolve", "issues": issues}),
            ));
        }
    };
    let composition = compose_packages(&resolved.packages, &resolved.semantic_profiles);
    let Some(project) = composition.project else {
        let issues = composition
            .issues
            .iter()
            .map(|issue| {
                serde_json::json!({
                    "severity": format!("{:?}", issue.severity).to_ascii_lowercase(),
                    "code": issue.code,
                    "package": issue.package.as_ref().map(|id| id.as_str()),
                    "kind": issue.kind.map(|kind| kind.label()),
                    "item_id": issue.item_id,
                    "message": issue.message,
                })
            })
            .collect::<Vec<_>>();
        return Ok(check_source_failure_report(
            policy,
            "authoring.composition_failed",
            "repair_package_composition",
            "Package composition failed before canonical measurements could run.",
            serde_json::json!({"stage": "composition", "issues": issues}),
        ));
    };
    let audit = Auditor::new(AuditorLimits::default())
        .audit(&resolved.packages, &resolved.semantic_profiles);
    let analysis = analyze_project(
        &project,
        &resolved.project.semantic_config,
        &resolved.project.conversation_config,
        &resolved.project.project_id,
        &resolved.project.brain_id,
        AnalysisLimits::default(),
    );

    let mut artifact_digest = None;
    let mut content_root = None;
    let mut build_error = None;
    let mut runtime_error = None;
    let mut tests = None;
    match build_source_project(tree, BuildOptions::default(), None) {
        Ok(built) => {
            artifact_digest = Some(built.artifact_digest.to_string());
            content_root = Some(built.content_root.to_string());
            match Runtime::load(built.artifact, LoadPolicy::default(), None) {
                Ok(runtime) => {
                    let mut driver = CanonicalRuntimeDriver { runtime };
                    tests = Some(run_test_suite(
                        &mut driver,
                        &project.tests,
                        TestRunLimits::default(),
                    ));
                }
                Err(error) => runtime_error = Some(format!("{error:?}")),
            }
        }
        Err(error) => build_error = Some(format!("{error:?}")),
    }

    let gate = evaluate(
        policy,
        &audit,
        &analysis,
        tests.as_ref(),
        build_error.as_deref(),
        runtime_error.as_deref(),
    );
    let accepted = gate.accepted;
    let tests_passed = tests.as_ref().is_some_and(|report| {
        report.regression_executed == report.regression_total
            && report.regression_passed == report.regression_total
            && report.scenario_executed == report.scenario_total
            && report.scenario_passed == report.scenario_total
            && !report.stopped_early
    });
    let report = serde_json::json!({
        "format": "gvya.cli.check",
        "version": 1,
        "accepted": accepted,
        "policy": policy.to_json(),
        "quality_vector": {
            "correctness": {
                "canonical_build": build_error.is_none(),
                "runtime_verification": runtime_error.is_none() && build_error.is_none(),
                "audit_errors": audit.summary.errors,
                "audit_warnings": audit.summary.warnings,
                "tests_passed": tests_passed,
            },
            "semantic_clarity": {
                "ambiguity_pairs": analysis.ambiguity.pairs.len(),
                "truncated_pairs": analysis.ambiguity.truncated_pairs,
                "truncated_results": analysis.ambiguity.truncated_results,
            },
            "expectation_coverage": {
                "meanings": analysis.coverage.meanings.ratio(),
                "behaviors": analysis.coverage.behaviors.ratio(),
                "capabilities": analysis.coverage.capabilities.ratio(),
            },
            "fallback_dependence": gate.fallback.to_json(),
            "complexity": {
                "packages": project.package_order.len(),
                "meanings": project.semantic_catalog.patterns().len(),
                "behaviors": project.conversation_catalog.behaviors().len(),
                "capabilities": project.capability_catalog.capability_ids().count(),
                "regression_cases": project.tests.regression_cases.len(),
                "scenarios": project.tests.scenarios.len(),
            },
            "determinism": {
                "artifact_sha256": artifact_digest,
                "content_root": content_root,
            },
        },
        "obligations": gate.obligations,
        "reports": {
            "source": null,
            "audit": audit_report_json(&audit),
            "analysis": analysis_json(&analysis),
            "tests": tests.as_ref().map(test_report_json),
            "build_error": build_error,
            "runtime_error": runtime_error,
        },
    });
    Ok((report, accepted))
}

pub(super) fn check_source_failure_report(
    policy: &AuthoringAcceptancePolicy,
    code: &str,
    action: &str,
    summary: &str,
    details: serde_json::Value,
) -> (serde_json::Value, bool) {
    let report = serde_json::json!({
        "format": "gvya.cli.check",
        "version": 1,
        "accepted": false,
        "policy": policy.to_json(),
        "quality_vector": {
            "correctness": {
                "canonical_build": false,
                "runtime_verification": false,
                "audit_errors": null,
                "audit_warnings": null,
                "tests_passed": false,
            },
            "semantic_clarity": {
                "ambiguity_pairs": null,
                "truncated_pairs": null,
                "truncated_results": null,
            },
            "expectation_coverage": {
                "meanings": null,
                "behaviors": null,
                "capabilities": null,
            },
            "fallback_dependence": {
                "observed_turns": 0,
                "fallback_turns": 0,
                "ratio": null,
            },
            "complexity": {
                "packages": null,
                "meanings": null,
                "behaviors": null,
                "capabilities": null,
                "regression_cases": null,
                "scenarios": null,
            },
            "determinism": {
                "artifact_sha256": null,
                "content_root": null,
            },
        },
        "obligations": [{
            "code": code,
            "action": action,
            "summary": summary,
            "details": details.clone(),
        }],
        "reports": {
            "source": details,
            "audit": null,
            "analysis": null,
            "tests": null,
            "build_error": null,
            "runtime_error": null,
        },
    });
    (report, false)
}

pub(super) fn command_turn(args: &[String]) -> Result<(), String> {
    let (project_path, request_path) = parse_runtime_request_args(args, "turn")?;
    let tree = load_source_tree(&project_path, SourceLimits::default())?;
    let built = build_source_project(&tree, BuildOptions::default(), None)
        .map_err(|error| format!("build before turn failed: {error:?}"))?;
    let runtime = Runtime::load(built.artifact, LoadPolicy::default(), None)
        .map_err(|error| format!("compiled artifact cannot load: {error:?}"))?;
    let request_bytes = read_bounded_file(
        &request_path,
        RuntimeLimits::default().max_request_bytes,
        "turn request",
    )?;
    let request = parse_turn_request(&request_bytes)
        .map_err(|error| format!("invalid canonical turn request: {error:?}"))?;
    let output = runtime
        .turn(request)
        .map_err(|error| format!("runtime turn rejected: {error:?}"))?;
    let bytes = serialize_turn_result(&output)
        .map_err(|error| format!("cannot serialize turn result: {error:?}"))?;
    let text = String::from_utf8(bytes)
        .map_err(|_| "canonical turn response was not UTF-8".to_string())?;
    println!("{text}");
    Ok(())
}

pub(super) fn command_capability_result(args: &[String]) -> Result<(), String> {
    let (project_path, request_path) = parse_runtime_request_args(args, "capability-result")?;
    let tree = load_source_tree(&project_path, SourceLimits::default())?;
    let built = build_source_project(&tree, BuildOptions::default(), None)
        .map_err(|error| format!("build before capability-result failed: {error:?}"))?;
    let runtime = Runtime::load(built.artifact, LoadPolicy::default(), None)
        .map_err(|error| format!("compiled artifact cannot load: {error:?}"))?;
    let request_bytes = read_bounded_file(
        &request_path,
        RuntimeLimits::default().max_request_bytes,
        "capability-result request",
    )?;
    let request = parse_capability_result_request(&request_bytes)
        .map_err(|error| format!("invalid canonical capability-result request: {error:?}"))?;
    let output = runtime
        .capability_result(request)
        .map_err(|error| format!("runtime capability-result rejected: {error:?}"))?;
    let bytes = serialize_capability_result_result(&output)
        .map_err(|error| format!("cannot serialize capability-result: {error:?}"))?;
    let text = String::from_utf8(bytes)
        .map_err(|_| "canonical capability-result response was not UTF-8".to_string())?;
    println!("{text}");
    Ok(())
}

pub(super) fn command_signing_root(args: &[String]) -> Result<(), String> {
    if args.len() != 1 {
        return Err(format!(
            "signing-root requires exactly one ARTIFACT.gvya\n\n{USAGE}"
        ));
    }
    let path = PathBuf::from(&args[0]);
    if path.extension().and_then(|value| value.to_str()) != Some("gvya") {
        return Err("signing-root input must use the .gvya extension".into());
    }
    let artifact_limits = gvya_compiler::artifact::ArtifactLimits::default();
    let bytes = read_bounded_file(&path, artifact_limits.max_total_bytes, "artifact")?;
    let root = artifact_signing_content_root(&bytes, artifact_limits)
        .map_err(|error| format!("cannot derive signing root: {error:?}"))?;
    println!("{root}");
    Ok(())
}

pub(super) fn command_attach_signature(args: &[String]) -> Result<(), String> {
    let (artifact_path, envelope_path, output_path) = parse_attach_signature_args(args)?;
    let artifact_limits = gvya_compiler::artifact::ArtifactLimits::default();
    let artifact = read_bounded_file(&artifact_path, artifact_limits.max_total_bytes, "artifact")?;
    let envelope_bytes = read_bounded_file(
        &envelope_path,
        SIGNATURE_ENVELOPE_MAX_BYTES,
        "signature envelope input",
    )?;
    let value: serde_json::Value = serde_json::from_slice(&envelope_bytes)
        .map_err(|error| format!("invalid signature envelope JSON: {error}"))?;
    let object = value
        .as_object()
        .ok_or_else(|| "signature envelope must be an object".to_string())?;
    let allowed = [
        "format",
        "version",
        "content_root",
        "algorithm",
        "key_id",
        "signature",
    ];
    if object.keys().any(|key| !allowed.contains(&key.as_str())) {
        return Err("signature envelope contains an unknown field".into());
    }
    if object.get("format").and_then(serde_json::Value::as_str) != Some("gvya.signature.input")
        || object.get("version").and_then(serde_json::Value::as_u64) != Some(1)
    {
        return Err("signature envelope must use gvya.signature.input/1".into());
    }
    let content_root = object
        .get("content_root")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| "signature envelope content_root must be a string".to_string())?;
    if content_root.len() != 64
        || !content_root
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        return Err("signature envelope content_root must be 64 lowercase hex characters".into());
    }
    let field = |name: &str| {
        object
            .get(name)
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned)
            .ok_or_else(|| format!("signature envelope {name} must be a string"))
    };
    let envelope = SignatureEnvelope {
        algorithm: field("algorithm")?,
        key_id: field("key_id")?,
        signature: field("signature")?,
    };
    let limits = gvya_compiler::artifact::ArtifactLimits::default();
    let signed = attach_signature_envelope(&artifact, content_root, &envelope, limits)
        .map_err(|error| format!("cannot attach signature: {error:?}"))?;
    if let Some(parent) = output_path
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
    {
        fs::create_dir_all(parent)
            .map_err(|error| format!("cannot create output directory: {error}"))?;
    }
    fs::write(&output_path, signed)
        .map_err(|error| format!("cannot write {}: {error}", output_path.display()))?;
    println!("signed {}", output_path.display());
    println!("content_root={content_root}");
    Ok(())
}
