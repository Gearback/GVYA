//! Compiler audit tests.
use super::*;
use crate::package::{PackageContents, PackageKind, PackageManifest};
use gvya_kernel::semantic::{
    LocalizedSample, LocalizedStructuralPattern, MeaningPattern, SemanticProfile, SemanticProfiles,
};
use gvya_model::PackageDigest;

fn test_profiles(profile: SemanticProfile) -> SemanticProfiles {
    BTreeMap::from([
        ("und".to_owned(), profile.clone()),
        ("en".to_owned(), profile.clone()),
        ("en-us".to_owned(), profile),
    ])
}

fn package(patterns: Vec<(&str, &str)>) -> PackageDefinition {
    PackageDefinition {
        manifest: PackageManifest {
            id: PackageId::new("base"),
            digest: PackageDigest::new("a".repeat(64)),
            kind: PackageKind::Standard,
            description: String::new(),
            dependencies: Vec::new(),
        },
        contents: PackageContents {
            meanings: patterns
                .into_iter()
                .map(|(id, sample)| {
                    crate::package::PackageContribution::add(id, MeaningPattern::new(id, [sample]))
                })
                .collect(),
            ..PackageContents::default()
        },
    }
}

#[test]
fn exact_cross_meaning_sample_overlap_is_error() {
    let report = Auditor::default().audit(
        &[package(vec![("a", "open door"), ("b", "open door")])],
        &test_profiles(SemanticProfile::empty()),
    );
    assert!(report.issues.iter().any(|row| row.code.as_str()
        == "semantic.sample_duplicate_cross_meaning"
        && row.severity == AuditSeverity::Error));
}

#[test]
fn repeated_sample_inside_one_meaning_is_only_a_local_warning() {
    let mut source = package(vec![("a", "open door")]);
    source.contents.meanings[0]
        .value
        .samples
        .push(LocalizedSample::new("und", "open door"));
    let report = Auditor::default().audit(&[source], &test_profiles(SemanticProfile::empty()));
    assert!(
        !report
            .issues
            .iter()
            .any(|row| row.code.as_str() == "semantic.sample_duplicate_cross_meaning")
    );
    assert!(report.issues.iter().any(|row| {
        row.code.as_str() == "semantic.sample_duplicate_local"
            && row.severity == AuditSeverity::Warning
    }));
}

#[test]
fn report_is_summary_first_and_severity_grouped() {
    let report = Auditor::default().audit(
        &[package(vec![
            ("a", "open the maintenance door"),
            ("b", "open maintenance door now"),
        ])],
        &test_profiles(SemanticProfile::empty()),
    );
    assert_eq!(report.summary.errors, 0);
    assert!(
        report
            .groups
            .windows(2)
            .all(|pair| severity_rank(pair[0].severity) <= severity_rank(pair[1].severity))
    );
}

#[test]
fn near_duplicate_response_text_is_reported_without_treating_exact_duplicates_as_near() {
    let profile = gvya_kernel::semantic::SemanticProfile::empty();
    let mut exact_issues = Vec::new();
    audit_response_near_duplicates(
        &[
            ("r1".to_owned(), "open the maintenance door now".to_owned()),
            ("r2".to_owned(), "open the maintenance door now".to_owned()),
        ],
        &profile,
        &mut exact_issues,
        AuditorLimits::default(),
    );
    assert!(exact_issues.is_empty());

    let mut near_issues = Vec::new();
    audit_response_near_duplicates(
        &[
            ("r1".to_owned(), "open the maintenance door now".to_owned()),
            ("r2".to_owned(), "open maintenance door now".to_owned()),
        ],
        &profile,
        &mut near_issues,
        AuditorLimits::default(),
    );
    assert_eq!(
        near_issues
            .iter()
            .filter(|row| row.code.as_str() == "conversation.response_text_near_duplicate")
            .count(),
        1
    );
    assert!(near_issues[0].details.contains_key("left_response"));
    assert!(near_issues[0].details.contains_key("right_response"));
}

#[test]
fn composition_collision_is_exposed_as_machine_stable_package_issue() {
    let first = package(vec![("a", "one")]);
    let mut second = package(vec![("a", "two")]);
    second.manifest.id = PackageId::new("other");
    let report =
        Auditor::default().audit(&[first, second], &test_profiles(SemanticProfile::empty()));
    assert!(
        report
            .issues
            .iter()
            .any(|row| row.code.as_str() == "package.contribution_collision")
    );
}

#[test]
fn zero_issue_budget_is_never_clean() {
    let report = Auditor::new(AuditorLimits {
        max_issues: 0,
        ..AuditorLimits::default()
    })
    .audit(&[], &test_profiles(SemanticProfile::empty()));
    assert!(report.truncated);
    assert!(!report.is_clean());
    assert!(
        report
            .issues
            .iter()
            .any(|row| row.code.as_str() == "audit.issue_budget_invalid")
    );
}

#[test]
fn saturated_diagnostic_budget_fails_closed_even_when_visible_rows_are_warnings() {
    let package = package(vec![("a", "open door"), ("b", "close door")]);
    let mut package = package;
    package.contents.meanings[0]
        .value
        .samples
        .push(gvya_kernel::semantic::LocalizedSample::new(
            "und",
            "open door",
        ));
    package.contents.meanings[1]
        .value
        .samples
        .push(gvya_kernel::semantic::LocalizedSample::new(
            "und",
            "close door",
        ));
    let report = Auditor::new(AuditorLimits {
        max_issues: 1,
        ..AuditorLimits::default()
    })
    .audit(&[package], &test_profiles(SemanticProfile::empty()));
    assert!(report.truncated);
    assert!(!report.is_clean());
}

#[test]
fn invalid_structural_matcher_profile_data_is_a_compiler_audit_error() {
    let mut source = package(vec![("device.on", "turn on device")]);
    source.contents.meanings[0]
        .value
        .patterns
        .push(LocalizedStructuralPattern::new(
            "en-US",
            "turn on <set:devices>",
        ));
    let mut profile = SemanticProfile::empty();
    profile.pattern_sets.insert(
        "devices".into(),
        BTreeMap::from([
            ("bedroom-light".into(), "device.one".into()),
            ("bedroom light".into(), "device.two".into()),
        ]),
    );
    let report = Auditor::default().audit(&[source], &test_profiles(profile));
    assert!(report.issues.iter().any(|row| {
        row.code.as_str() == "semantic.structural_matcher_invalid"
            && row.severity == AuditSeverity::Error
    }));
}
