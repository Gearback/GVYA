//! Bounded authoring analysis primitives for coverage and ambiguity review.

use std::collections::BTreeSet;

use gvya_kernel::{
    conversation::ConversationConfig,
    semantic::{
        MeaningPattern, SemanticConfig, SemanticDecision, SemanticInput, SemanticKernel,
        normalize_text, ordered_tokens,
    },
};
use gvya_model::{BehaviorId, CapabilityId, MeaningId};

use crate::{package::ComposedProject, testing::TurnExpectation};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AnalysisLimits {
    pub max_sample_pairs: usize,
    pub max_ambiguities: usize,
    pub ambiguity_threshold_milli: u16,
    pub max_discoverability_probes: usize,
    pub discoverability_probes_per_meaning: usize,
    pub fragile_repair_margin_milli: u16,
}

impl Default for AnalysisLimits {
    fn default() -> Self {
        Self {
            max_sample_pairs: 200_000,
            max_ambiguities: 2_000,
            ambiguity_threshold_milli: 650,
            max_discoverability_probes: 0,
            discoverability_probes_per_meaning: 0,
            fragile_repair_margin_milli: 10,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CoverageCounts<T> {
    pub total: usize,
    pub covered: usize,
    pub uncovered: Vec<T>,
}

impl<T> CoverageCounts<T> {
    #[must_use]
    pub fn ratio(&self) -> f64 {
        if self.total == 0 {
            1.0
        } else {
            self.covered as f64 / self.total as f64
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TestOriginCounts {
    pub manual_regression: usize,
    pub generated_regression: usize,
    pub manual_scenarios: usize,
    pub generated_scenarios: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CoverageReport {
    pub meanings: CoverageCounts<MeaningId>,
    pub behaviors: CoverageCounts<BehaviorId>,
    pub capabilities: CoverageCounts<CapabilityId>,
    pub test_origins: TestOriginCounts,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AmbiguityPair {
    pub left_meaning: MeaningId,
    pub left_sample: String,
    pub right_meaning: MeaningId,
    pub right_sample: String,
    pub similarity: f64,
    pub exact: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AmbiguityReport {
    pub compared_pairs: usize,
    pub truncated_pairs: bool,
    pub truncated_results: bool,
    pub pairs: Vec<AmbiguityPair>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ProjectAnalysis {
    pub coverage: CoverageReport,
    pub ambiguity: AmbiguityReport,
    pub discoverability: DiscoverabilityReport,
    pub repair_boundaries: RepairBoundaryReport,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DiscoverabilityProbe {
    pub meaning: MeaningId,
    pub language: String,
    pub source_sample: String,
    pub input: String,
    pub decision: String,
    pub resolved_meaning: Option<MeaningId>,
    pub expected_score: Option<f64>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MeaningDiscoverability {
    pub meaning: MeaningId,
    pub identity_bearing_samples: usize,
    pub has_identity_free_sample: bool,
    pub probes: usize,
    pub resolved_to_expected: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DiscoverabilityReport {
    pub identity_terms: Vec<String>,
    pub total_probes: usize,
    pub resolved_to_expected: usize,
    pub truncated: bool,
    pub kernel_error: Option<String>,
    pub meanings_requiring_review: Vec<MeaningId>,
    pub meanings: Vec<MeaningDiscoverability>,
    pub probes: Vec<DiscoverabilityProbe>,
}

impl DiscoverabilityReport {
    #[must_use]
    pub fn ratio(&self) -> f64 {
        if self.total_probes == 0 {
            1.0
        } else {
            self.resolved_to_expected as f64 / self.total_probes as f64
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct RepairBoundaryProbe {
    pub test_id: String,
    pub expected_meaning: MeaningId,
    pub candidate_meaning: Option<MeaningId>,
    pub score: Option<f64>,
    pub inside_repair_band: bool,
    pub distance_to_floor: Option<f64>,
    pub distance_to_resolution: Option<f64>,
    pub fragile: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RepairBoundaryReport {
    pub repair_floor: f64,
    pub resolution_threshold: f64,
    pub warning_margin: f64,
    pub fragile_count: usize,
    pub kernel_error: Option<String>,
    pub probes: Vec<RepairBoundaryProbe>,
}

#[must_use]
pub fn analyze_project(
    project: &ComposedProject,
    semantic_config: &SemanticConfig,
    conversation_config: &ConversationConfig,
    project_id: &str,
    brain_id: &str,
    limits: AnalysisLimits,
) -> ProjectAnalysis {
    let semantic = SemanticKernel::new(
        project.semantic_catalog.clone(),
        project.semantic_profiles.clone(),
        semantic_config.clone(),
    );
    ProjectAnalysis {
        coverage: coverage_report(project),
        ambiguity: ambiguity_report(project.semantic_catalog.patterns(), limits),
        discoverability: discoverability_report(
            project,
            project_id,
            brain_id,
            semantic.as_ref().map_err(|error| format!("{error:?}")),
            limits,
        ),
        repair_boundaries: repair_boundary_report(
            project,
            semantic_config,
            conversation_config,
            semantic.as_ref().map_err(|error| format!("{error:?}")),
            limits,
        ),
    }
}

#[must_use]
pub fn coverage_report(project: &ComposedProject) -> CoverageReport {
    let mut expected_meanings = BTreeSet::new();
    let mut expected_capabilities = BTreeSet::new();
    for case in &project.tests.regression_cases {
        collect_expectation(
            &case.expectation,
            &mut expected_meanings,
            &mut expected_capabilities,
        );
    }
    for scenario in &project.tests.scenarios {
        for step in &scenario.steps {
            collect_expectation(
                step.expectation(),
                &mut expected_meanings,
                &mut expected_capabilities,
            );
        }
    }

    let all_meanings: BTreeSet<_> = project
        .semantic_catalog
        .patterns()
        .iter()
        .map(|pattern| pattern.id.clone())
        .collect();
    let all_behaviors: BTreeSet<_> = project
        .conversation_catalog
        .behaviors()
        .iter()
        .map(|behavior| behavior.id.clone())
        .collect();
    let covered_behaviors: BTreeSet<_> = project
        .conversation_catalog
        .behaviors()
        .iter()
        .filter(|behavior| expected_meanings.contains(&behavior.meaning))
        .map(|behavior| behavior.id.clone())
        .collect();
    let all_capabilities: BTreeSet<_> = project
        .capability_catalog
        .capability_ids()
        .cloned()
        .collect();

    CoverageReport {
        meanings: counts(&all_meanings, &expected_meanings),
        behaviors: counts(&all_behaviors, &covered_behaviors),
        capabilities: counts(&all_capabilities, &expected_capabilities),
        test_origins: TestOriginCounts {
            manual_regression: project
                .tests
                .regression_cases
                .iter()
                .filter(|case| !case.generated)
                .count(),
            generated_regression: project
                .tests
                .regression_cases
                .iter()
                .filter(|case| case.generated)
                .count(),
            manual_scenarios: project
                .tests
                .scenarios
                .iter()
                .filter(|scenario| !scenario.generated)
                .count(),
            generated_scenarios: project
                .tests
                .scenarios
                .iter()
                .filter(|scenario| scenario.generated)
                .count(),
        },
    }
}

#[must_use]
pub fn ambiguity_report(patterns: &[MeaningPattern], limits: AnalysisLimits) -> AmbiguityReport {
    let threshold = f64::from(limits.ambiguity_threshold_milli) / 1000.0;
    let mut pairs = Vec::new();
    let mut compared = 0usize;
    let mut truncated_pairs = false;
    let mut truncated_results = false;

    'outer: for left_index in 0..patterns.len() {
        for right_index in (left_index + 1)..patterns.len() {
            for left_sample in &patterns[left_index].samples {
                for right_sample in &patterns[right_index].samples {
                    if compared >= limits.max_sample_pairs {
                        truncated_pairs = true;
                        break 'outer;
                    }
                    compared += 1;
                    let left_normalized = normalize_text(&left_sample.text);
                    let right_normalized = normalize_text(&right_sample.text);
                    let exact = !left_normalized.is_empty() && left_normalized == right_normalized;
                    let similarity = if exact {
                        1.0
                    } else {
                        jaccard_normalized(&left_normalized, &right_normalized)
                    };
                    if similarity >= threshold {
                        if pairs.len() >= limits.max_ambiguities {
                            truncated_results = true;
                            continue;
                        }
                        pairs.push(AmbiguityPair {
                            left_meaning: patterns[left_index].id.clone(),
                            left_sample: left_sample.text.clone(),
                            right_meaning: patterns[right_index].id.clone(),
                            right_sample: right_sample.text.clone(),
                            similarity,
                            exact,
                        });
                    }
                }
            }
        }
    }
    pairs.sort_by(|left, right| {
        right
            .similarity
            .total_cmp(&left.similarity)
            .then_with(|| left.left_meaning.cmp(&right.left_meaning))
            .then_with(|| left.right_meaning.cmp(&right.right_meaning))
            .then_with(|| left.left_sample.cmp(&right.left_sample))
            .then_with(|| left.right_sample.cmp(&right.right_sample))
    });
    AmbiguityReport {
        compared_pairs: compared,
        truncated_pairs,
        truncated_results,
        pairs,
    }
}

fn discoverability_report(
    project: &ComposedProject,
    project_id: &str,
    brain_id: &str,
    semantic: Result<&SemanticKernel, String>,
    limits: AnalysisLimits,
) -> DiscoverabilityReport {
    let identity_terms = identity_terms(project_id, brain_id);
    if limits.max_discoverability_probes == 0 || limits.discoverability_probes_per_meaning == 0 {
        return DiscoverabilityReport {
            identity_terms,
            total_probes: 0,
            resolved_to_expected: 0,
            truncated: false,
            kernel_error: None,
            meanings_requiring_review: Vec::new(),
            meanings: Vec::new(),
            probes: Vec::new(),
        };
    }
    let Ok(semantic) = semantic else {
        return DiscoverabilityReport {
            identity_terms,
            total_probes: 0,
            resolved_to_expected: 0,
            truncated: false,
            kernel_error: semantic.err(),
            meanings_requiring_review: Vec::new(),
            meanings: Vec::new(),
            probes: Vec::new(),
        };
    };
    if identity_terms.is_empty() {
        return DiscoverabilityReport {
            identity_terms,
            total_probes: 0,
            resolved_to_expected: 0,
            truncated: false,
            kernel_error: None,
            meanings_requiring_review: Vec::new(),
            meanings: Vec::new(),
            probes: Vec::new(),
        };
    }

    let identity_set: BTreeSet<_> = identity_terms.iter().cloned().collect();
    let mut meanings = Vec::new();
    let mut probes = Vec::new();
    let mut meanings_requiring_review = Vec::new();
    let mut resolved_to_expected = 0usize;
    let mut truncated = false;

    for pattern in project.semantic_catalog.patterns() {
        let has_identity_free_sample = pattern.samples.iter().any(|sample| {
            ordered_tokens(&normalize_text(&sample.text))
                .iter()
                .all(|token| !identity_set.contains(token))
        });
        let mut identity_samples = pattern
            .samples
            .iter()
            .filter_map(|sample| {
                let tokens = ordered_tokens(&normalize_text(&sample.text));
                tokens
                    .iter()
                    .any(|token| identity_set.contains(token))
                    .then_some((sample, tokens))
            })
            .collect::<Vec<_>>();
        if identity_samples.is_empty() {
            continue;
        }
        identity_samples.sort_by(|(left, left_tokens), (right, right_tokens)| {
            left_tokens
                .len()
                .cmp(&right_tokens.len())
                .then_with(|| left.language.cmp(&right.language))
                .then_with(|| left.text.cmp(&right.text))
        });
        let identity_bearing_samples = identity_samples.len();
        let mut meaning_probe_count = 0usize;
        let mut meaning_resolved = 0usize;
        let mut seen = BTreeSet::new();

        for (sample, tokens) in identity_samples {
            if meaning_probe_count >= limits.discoverability_probes_per_meaning {
                break;
            }
            if probes.len() >= limits.max_discoverability_probes {
                truncated = true;
                break;
            }
            let input = tokens
                .into_iter()
                .filter(|token| !identity_set.contains(token))
                .collect::<Vec<_>>()
                .join(" ");
            if input.is_empty() || !seen.insert((sample.language.clone(), input.clone())) {
                continue;
            }
            let mut semantic_input = SemanticInput::utterance(input.clone());
            semantic_input.utterance.language = Some(sample.language.clone());
            let analysis = semantic.analyze(&semantic_input);
            let resolved_meaning = match &analysis.decision {
                SemanticDecision::Resolved { meaning, .. } => Some(meaning.id.clone()),
                SemanticDecision::Partial { partial, .. } => Some(partial.meaning.id.clone()),
                SemanticDecision::Ambiguous { .. } | SemanticDecision::Unresolved { .. } => None,
            };
            let decision = match &analysis.decision {
                SemanticDecision::Resolved { meaning, .. } if meaning.id == pattern.id => {
                    "resolved_expected"
                }
                SemanticDecision::Partial { partial, .. } if partial.meaning.id == pattern.id => {
                    "resolved_expected"
                }
                SemanticDecision::Resolved { .. } => "resolved_other",
                SemanticDecision::Partial { .. } => "resolved_other",
                SemanticDecision::Ambiguous { .. } => "ambiguous",
                SemanticDecision::Unresolved { .. } => "unresolved",
            };
            let expected_score = analysis
                .scored
                .iter()
                .find(|row| row.meaning == pattern.id)
                .map(|row| row.score);
            if decision == "resolved_expected" {
                meaning_resolved += 1;
                resolved_to_expected += 1;
            }
            meaning_probe_count += 1;
            probes.push(DiscoverabilityProbe {
                meaning: pattern.id.clone(),
                language: sample.language.clone(),
                source_sample: sample.text.clone(),
                input,
                decision: decision.to_string(),
                resolved_meaning,
                expected_score,
            });
        }
        if !has_identity_free_sample || meaning_resolved != meaning_probe_count {
            meanings_requiring_review.push(pattern.id.clone());
        }
        meanings.push(MeaningDiscoverability {
            meaning: pattern.id.clone(),
            identity_bearing_samples,
            has_identity_free_sample,
            probes: meaning_probe_count,
            resolved_to_expected: meaning_resolved,
        });
        if truncated {
            break;
        }
    }

    DiscoverabilityReport {
        identity_terms,
        total_probes: probes.len(),
        resolved_to_expected,
        truncated,
        kernel_error: None,
        meanings_requiring_review,
        meanings,
        probes,
    }
}

fn repair_boundary_report(
    project: &ComposedProject,
    semantic_config: &SemanticConfig,
    conversation_config: &ConversationConfig,
    semantic: Result<&SemanticKernel, String>,
    limits: AnalysisLimits,
) -> RepairBoundaryReport {
    let warning_margin = f64::from(limits.fragile_repair_margin_milli) / 1000.0;
    let Ok(semantic) = semantic else {
        return RepairBoundaryReport {
            repair_floor: conversation_config.repair_candidate_min_score,
            resolution_threshold: semantic_config.resolution_threshold,
            warning_margin,
            fragile_count: 0,
            kernel_error: semantic.err(),
            probes: Vec::new(),
        };
    };
    let mut probes = Vec::new();
    for case in &project.tests.regression_cases {
        if case.generated
            || case.expectation.conversation_mode.as_deref() != Some("repair_continuation")
        {
            continue;
        }
        let Some(expected_meaning) = &case.expectation.meaning else {
            continue;
        };
        let mut input = SemanticInput::utterance(case.input.clone());
        input.utterance.language = case.language.clone();
        let analysis = semantic.analyze(&input);
        let candidate = analysis.scored.first();
        let score = candidate.map(|row| row.score);
        let inside_repair_band = score.is_some_and(|score| {
            score >= conversation_config.repair_candidate_min_score
                && score < semantic_config.resolution_threshold
        });
        let distance_to_floor =
            score.map(|score| (score - conversation_config.repair_candidate_min_score).max(0.0));
        let distance_to_resolution =
            score.map(|score| (semantic_config.resolution_threshold - score).max(0.0));
        let fragile = inside_repair_band
            && distance_to_floor
                .zip(distance_to_resolution)
                .is_some_and(|(floor, resolution)| floor.min(resolution) < warning_margin);
        probes.push(RepairBoundaryProbe {
            test_id: case.id.as_str().to_string(),
            expected_meaning: expected_meaning.clone(),
            candidate_meaning: candidate.map(|row| row.meaning.clone()),
            score,
            inside_repair_band,
            distance_to_floor,
            distance_to_resolution,
            fragile,
        });
    }
    let fragile_count = probes.iter().filter(|probe| probe.fragile).count();
    RepairBoundaryReport {
        repair_floor: conversation_config.repair_candidate_min_score,
        resolution_threshold: semantic_config.resolution_threshold,
        warning_margin,
        fragile_count,
        kernel_error: None,
        probes,
    }
}

fn identity_terms(project_id: &str, brain_id: &str) -> Vec<String> {
    let project: BTreeSet<_> = ordered_tokens(&normalize_text(project_id))
        .into_iter()
        .filter(|token| token.chars().count() >= 3)
        .collect();
    let brain: BTreeSet<_> = ordered_tokens(&normalize_text(brain_id))
        .into_iter()
        .filter(|token| token.chars().count() >= 3)
        .collect();
    project.intersection(&brain).cloned().collect()
}

fn collect_expectation(
    expectation: &TurnExpectation,
    meanings: &mut BTreeSet<MeaningId>,
    capabilities: &mut BTreeSet<CapabilityId>,
) {
    if let Some(meaning) = &expectation.meaning {
        meanings.insert(meaning.clone());
    }
    for capability in &expectation.capabilities {
        capabilities.insert(capability.id.clone());
    }
}

fn counts<T: Clone + Ord>(all: &BTreeSet<T>, covered: &BTreeSet<T>) -> CoverageCounts<T> {
    let actual: BTreeSet<_> = all.intersection(covered).cloned().collect();
    CoverageCounts {
        total: all.len(),
        covered: actual.len(),
        uncovered: all.difference(&actual).cloned().collect(),
    }
}

fn jaccard_normalized(left: &str, right: &str) -> f64 {
    let left_set: BTreeSet<_> = ordered_tokens(left).into_iter().collect();
    let right_set: BTreeSet<_> = ordered_tokens(right).into_iter().collect();
    if left_set.is_empty() || right_set.is_empty() {
        return 0.0;
    }
    let intersection = left_set.intersection(&right_set).count();
    let union = left_set.union(&right_set).count();
    if union == 0 {
        0.0
    } else {
        intersection as f64 / union as f64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_overlap_is_ranked_first() {
        let patterns = vec![
            MeaningPattern::new("a", ["open door"]),
            MeaningPattern::new("b", ["open door"]),
            MeaningPattern::new("c", ["close window"]),
        ];
        let report = ambiguity_report(&patterns, AnalysisLimits::default());
        assert_eq!(report.pairs.first().unwrap().similarity, 1.0);
        assert!(report.pairs.first().unwrap().exact);
    }

    #[test]
    fn ambiguity_work_is_bounded() {
        let patterns = vec![
            MeaningPattern::new("a", ["one", "two", "three"]),
            MeaningPattern::new("b", ["one", "two", "three"]),
        ];
        let report = ambiguity_report(
            &patterns,
            AnalysisLimits {
                max_sample_pairs: 2,
                ..AnalysisLimits::default()
            },
        );
        assert_eq!(report.compared_pairs, 2);
        assert!(report.truncated_pairs);
    }

    #[test]
    fn discoverability_identity_uses_only_shared_project_and_brain_tokens() {
        assert_eq!(
            identity_terms("gvya-project", "gvya-bot"),
            vec!["gvya".to_string()]
        );
        assert!(identity_terms("support", "assistant").is_empty());
    }
}
