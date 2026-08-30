//! Incremental authoring change analysis and conservative blast-radius test selection.
//!
//! This compares two *composed* GVYA projects. Filesystem layout is not semantic authority.

use std::collections::{BTreeMap, BTreeSet};

use gvya_kernel::{
    capability::{AdmissionNamespace, BindingSource, PolicyEffect},
    conversation::{ConversationBehavior, ConversationConfig, ResponseKind, StateNamespace},
    semantic::{
        SemanticConfig, SemanticProfile, normalize_language_tag, ordered_tokens,
        profile_for_authored_language,
    },
};

use crate::{
    package::ComposedProject,
    testing::{
        ConversationScenario, ExpectedProposalOutcome, RegressionCase, ScenarioStep, TestSuite,
        TurnExpectation,
    },
};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum ChangeStatus {
    Added,
    Modified,
    Removed,
}
impl ChangeStatus {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Added => "added",
            Self::Modified => "modified",
            Self::Removed => "removed",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum ChangeKind {
    Meaning,
    Behavior,
    CapabilityResultBehavior,
    Opening,
    FallbackBehavior,
    StyleLexicon,
    Capability,
    CapabilityBinding,
    CapabilityPolicy,
    CapabilityConfig,
    Type,
    Asset,
    RegressionCase,
    Scenario,
}
impl ChangeKind {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Meaning => "meaning",
            Self::Behavior => "behavior",
            Self::CapabilityResultBehavior => "capability_result_behavior",
            Self::Opening => "opening",
            Self::FallbackBehavior => "fallback_behavior",
            Self::StyleLexicon => "style_lexicon",
            Self::Capability => "capability",
            Self::CapabilityBinding => "capability_binding",
            Self::CapabilityPolicy => "capability_policy",
            Self::CapabilityConfig => "capability_config",
            Self::Type => "type",
            Self::Asset => "asset",
            Self::RegressionCase => "regression_case",
            Self::Scenario => "scenario",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct ProjectChange {
    pub kind: ChangeKind,
    pub id: String,
    pub status: ChangeStatus,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ProjectSourceSurface {
    pub project_id: String,
    pub brain_id: String,
    pub languages: Vec<String>,
    pub enabled_languages: Vec<String>,
    pub default_language: String,
    pub semantic_config: SemanticConfig,
    pub conversation_config: ConversationConfig,
    pub emit_debug_map: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ProjectChangeSet {
    pub package_order_changed: bool,
    pub semantic_profile_changed: bool,
    pub project_identity_changed: bool,
    pub language_config_changed: bool,
    pub semantic_config_changed: bool,
    pub conversation_config_changed: bool,
    pub debug_map_changed: bool,
    pub changes: Vec<ProjectChange>,
}
impl ProjectChangeSet {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        !self.package_order_changed
            && !self.semantic_profile_changed
            && !self.project_identity_changed
            && !self.language_config_changed
            && !self.semantic_config_changed
            && !self.conversation_config_changed
            && !self.debug_map_changed
            && self.changes.is_empty()
    }
    #[must_use]
    pub fn runtime_changes(&self) -> usize {
        self.changes
            .iter()
            .filter(|c| !matches!(c.kind, ChangeKind::RegressionCase | ChangeKind::Scenario))
            .count()
            + usize::from(self.package_order_changed)
            + usize::from(self.semantic_profile_changed)
            + usize::from(self.project_identity_changed)
            + usize::from(self.language_config_changed)
            + usize::from(self.semantic_config_changed)
            + usize::from(self.conversation_config_changed)
            + usize::from(self.debug_map_changed)
    }

    #[must_use]
    pub fn mechanic_proof_required(&self) -> bool {
        self.changes
            .iter()
            .any(|c| !matches!(c.kind, ChangeKind::RegressionCase | ChangeKind::Scenario))
            || self.package_order_changed
            || self.semantic_profile_changed
            || self.language_config_changed
            || self.semantic_config_changed
            || self.conversation_config_changed
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum ChangeMechanic {
    SemanticResolution,
    BehaviorResponse,
    TopicScope,
    TopicActivation,
    FollowupScope,
    FollowupOpen,
    RepeatLadder,
    RepairContinuation,
    StateEligibility,
    StateResponse,
    StateEffect,
    Opening,
    FallbackRecovery,
    CapabilityProposal,
    CapabilityPolicy,
    CapabilityConfirmation,
    CapabilityResult,
    GlobalRuntimeContract,
}
impl ChangeMechanic {
    pub const fn label(self) -> &'static str {
        match self {
            Self::SemanticResolution => "semantic_resolution",
            Self::BehaviorResponse => "behavior_response",
            Self::TopicScope => "topic_scope",
            Self::TopicActivation => "topic_activation",
            Self::FollowupScope => "followup_scope",
            Self::FollowupOpen => "followup_open",
            Self::RepeatLadder => "repeat_ladder",
            Self::RepairContinuation => "repair_continuation",
            Self::StateEligibility => "state_eligibility",
            Self::StateResponse => "state_response",
            Self::StateEffect => "state_effect",
            Self::Opening => "opening",
            Self::FallbackRecovery => "fallback_recovery",
            Self::CapabilityProposal => "capability_proposal",
            Self::CapabilityPolicy => "capability_policy",
            Self::CapabilityConfirmation => "capability_confirmation",
            Self::CapabilityResult => "capability_result",
            Self::GlobalRuntimeContract => "global_runtime_contract",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MechanicProofRequirement {
    pub mechanic: ChangeMechanic,
    pub source_kind: Option<ChangeKind>,
    pub subject: String,
    pub covered_by: Vec<SelectedTest>,
}
impl MechanicProofRequirement {
    #[must_use]
    pub fn covered(&self) -> bool {
        !self.covered_by.is_empty()
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum SelectedTest {
    Regression(String),
    Scenario(String),
}
impl SelectedTest {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Regression(_) => "regression",
            Self::Scenario(_) => "scenario",
        }
    }
    pub fn id(&self) -> &str {
        match self {
            Self::Regression(v) | Self::Scenario(v) => v,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SelectionReason {
    pub code: String,
    pub subject: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ChangeTestPlan {
    pub change_set: ProjectChangeSet,
    pub full_suite_required: bool,
    pub full_suite_reasons: Vec<String>,
    pub semantic_neighbor_pairs_compared: usize,
    pub semantic_neighbor_truncated: bool,
    /// Added/modified candidate tests, including generated coverage rows.
    pub changed_test_count: usize,
    /// Added/modified *manual* tests that directly prove at least one changed mechanic.
    pub proof_test_count: usize,
    /// Every changed mechanic that must be proven before this slice can be accepted.
    pub mechanic_requirements: Vec<MechanicProofRequirement>,
    /// Runtime-affecting changes are not mergeable while any required mechanic lacks direct proof.
    pub mechanic_proof_missing: bool,
    pub selected: BTreeMap<SelectedTest, Vec<SelectionReason>>,
    pub neighbor_meanings: BTreeMap<String, Vec<String>>,
}
impl ChangeTestPlan {
    #[must_use]
    pub fn selected_suite(&self, candidate: &ComposedProject) -> TestSuite {
        let regression: BTreeSet<_> = self
            .selected
            .keys()
            .filter_map(|t| {
                if let SelectedTest::Regression(id) = t {
                    Some(id.as_str())
                } else {
                    None
                }
            })
            .collect();
        let scenarios: BTreeSet<_> = self
            .selected
            .keys()
            .filter_map(|t| {
                if let SelectedTest::Scenario(id) = t {
                    Some(id.as_str())
                } else {
                    None
                }
            })
            .collect();
        TestSuite {
            regression_cases: candidate
                .tests
                .regression_cases
                .iter()
                .filter(|r| regression.contains(r.id.as_str()))
                .cloned()
                .collect(),
            scenarios: candidate
                .tests
                .scenarios
                .iter()
                .filter(|r| scenarios.contains(r.id.as_str()))
                .cloned()
                .collect(),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ChangePlanLimits {
    pub max_neighbor_sample_pairs: usize,
    pub max_neighbors_per_meaning: usize,
    pub max_sentinels: usize,
}
impl Default for ChangePlanLimits {
    fn default() -> Self {
        Self {
            max_neighbor_sample_pairs: 50_000,
            max_neighbors_per_meaning: 16,
            max_sentinels: 32,
        }
    }
}

#[must_use]
pub fn diff_projects(
    base: &ComposedProject,
    candidate: &ComposedProject,
    base_source: &ProjectSourceSurface,
    candidate_source: &ProjectSourceSurface,
) -> ProjectChangeSet {
    let mut changes = Vec::new();
    diff_rows(
        base.semantic_catalog.patterns(),
        candidate.semantic_catalog.patterns(),
        |x| x.id.as_str(),
        ChangeKind::Meaning,
        &mut changes,
    );
    diff_rows(
        base.conversation_catalog.behaviors(),
        candidate.conversation_catalog.behaviors(),
        |x| x.id.as_str(),
        ChangeKind::Behavior,
        &mut changes,
    );
    diff_rows(
        base.conversation_catalog.capability_result_behaviors(),
        candidate.conversation_catalog.capability_result_behaviors(),
        |x| x.id.as_str(),
        ChangeKind::CapabilityResultBehavior,
        &mut changes,
    );
    diff_rows(
        base.conversation_catalog.openings(),
        candidate.conversation_catalog.openings(),
        |x| x.id.as_str(),
        ChangeKind::Opening,
        &mut changes,
    );
    diff_rows(
        base.conversation_catalog.fallback_behaviors(),
        candidate.conversation_catalog.fallback_behaviors(),
        |x| x.id.as_str(),
        ChangeKind::FallbackBehavior,
        &mut changes,
    );
    if base.conversation_catalog.style_lexicon() != candidate.conversation_catalog.style_lexicon() {
        changes.push(ProjectChange {
            kind: ChangeKind::StyleLexicon,
            id: "catalog".into(),
            status: ChangeStatus::Modified,
        });
    }
    let base_caps: Vec<_> = base
        .capability_catalog
        .capability_ids()
        .filter_map(|id| base.capability_catalog.definition(id).cloned())
        .collect();
    let candidate_caps: Vec<_> = candidate
        .capability_catalog
        .capability_ids()
        .filter_map(|id| candidate.capability_catalog.definition(id).cloned())
        .collect();
    diff_rows(
        &base_caps,
        &candidate_caps,
        |x| x.contract.id.as_str(),
        ChangeKind::Capability,
        &mut changes,
    );
    diff_rows(
        base.capability_catalog.bindings(),
        candidate.capability_catalog.bindings(),
        |x| x.id.as_str(),
        ChangeKind::CapabilityBinding,
        &mut changes,
    );
    diff_rows(
        base.capability_catalog.policies(),
        candidate.capability_catalog.policies(),
        |x| x.id.as_str(),
        ChangeKind::CapabilityPolicy,
        &mut changes,
    );
    if base.capability_catalog.config() != candidate.capability_catalog.config() {
        changes.push(ProjectChange {
            kind: ChangeKind::CapabilityConfig,
            id: "catalog".into(),
            status: ChangeStatus::Modified,
        });
    }
    diff_map(
        &base.types,
        &candidate.types,
        |x| x.as_str(),
        ChangeKind::Type,
        &mut changes,
    );
    diff_map(
        &base.assets,
        &candidate.assets,
        |x| x.as_str(),
        ChangeKind::Asset,
        &mut changes,
    );
    diff_rows(
        &base.tests.regression_cases,
        &candidate.tests.regression_cases,
        |x| x.id.as_str(),
        ChangeKind::RegressionCase,
        &mut changes,
    );
    diff_rows(
        &base.tests.scenarios,
        &candidate.tests.scenarios,
        |x| x.id.as_str(),
        ChangeKind::Scenario,
        &mut changes,
    );
    changes.sort();
    ProjectChangeSet {
        package_order_changed: base.package_order != candidate.package_order,
        semantic_profile_changed: base.semantic_profiles != candidate.semantic_profiles,
        project_identity_changed: base_source.project_id != candidate_source.project_id
            || base_source.brain_id != candidate_source.brain_id,
        language_config_changed: base_source.languages != candidate_source.languages
            || base_source.enabled_languages != candidate_source.enabled_languages
            || base_source.default_language != candidate_source.default_language,
        semantic_config_changed: base_source.semantic_config != candidate_source.semantic_config,
        conversation_config_changed: base_source.conversation_config
            != candidate_source.conversation_config,
        debug_map_changed: base_source.emit_debug_map != candidate_source.emit_debug_map,
        changes,
    }
}

fn diff_rows<T: PartialEq, F: Fn(&T) -> &str>(
    base: &[T],
    candidate: &[T],
    id: F,
    kind: ChangeKind,
    out: &mut Vec<ProjectChange>,
) {
    let bm: BTreeMap<_, _> = base.iter().map(|x| (id(x).to_owned(), x)).collect();
    let cm: BTreeMap<_, _> = candidate.iter().map(|x| (id(x).to_owned(), x)).collect();
    let ids: BTreeSet<_> = bm.keys().chain(cm.keys()).cloned().collect();
    for key in ids {
        match (bm.get(&key), cm.get(&key)) {
            (None, Some(_)) => out.push(ProjectChange {
                kind,
                id: key,
                status: ChangeStatus::Added,
            }),
            (Some(_), None) => out.push(ProjectChange {
                kind,
                id: key,
                status: ChangeStatus::Removed,
            }),
            (Some(a), Some(b)) if *a != *b => out.push(ProjectChange {
                kind,
                id: key,
                status: ChangeStatus::Modified,
            }),
            _ => {}
        }
    }
}
fn diff_map<K: Ord, V: PartialEq, F: Fn(&K) -> &str>(
    base: &BTreeMap<K, V>,
    candidate: &BTreeMap<K, V>,
    id: F,
    kind: ChangeKind,
    out: &mut Vec<ProjectChange>,
) {
    let keys: BTreeSet<_> = base.keys().chain(candidate.keys()).collect();
    for key in keys {
        let id = id(key).to_owned();
        match (base.get(key), candidate.get(key)) {
            (None, Some(_)) => out.push(ProjectChange {
                kind,
                id,
                status: ChangeStatus::Added,
            }),
            (Some(_), None) => out.push(ProjectChange {
                kind,
                id,
                status: ChangeStatus::Removed,
            }),
            (Some(a), Some(b)) if a != b => out.push(ProjectChange {
                kind,
                id,
                status: ChangeStatus::Modified,
            }),
            _ => {}
        }
    }
}

#[derive(Clone, Default)]
struct TestDeps {
    meanings: BTreeSet<String>,
    responses: BTreeSet<String>,
    capabilities: BTreeSet<String>,
    topics: BTreeSet<String>,
    followups: BTreeSet<String>,
    author_paths: BTreeSet<String>,
    conversation_paths: BTreeSet<String>,
    context_paths: BTreeSet<String>,
    has_open: bool,
    fallback_sensitive: bool,
    repeat_sensitive: bool,
}
fn expectation_deps(e: &TurnExpectation, d: &mut TestDeps) {
    if let Some(v) = &e.meaning {
        d.meanings.insert(v.as_str().into());
    }
    for v in &e.forbidden_meanings {
        d.meanings.insert(v.as_str().into());
    }
    for v in &e.response_ids {
        d.responses.insert(v.as_str().into());
    }
    for v in &e.forbidden_response_ids {
        d.responses.insert(v.as_str().into());
    }
    for v in &e.capabilities {
        d.capabilities.insert(v.id.as_str().into());
    }
    for v in &e.proposal_receipts {
        d.capabilities.insert(v.id.as_str().into());
    }
    for v in &e.forbidden_capabilities {
        d.capabilities.insert(v.as_str().into());
    }
    if let Some(v) = &e.active_topic {
        d.topics.insert(v.as_str().into());
    }
    if let Some(v) = &e.active_followup {
        d.followups.insert(v.as_str().into());
    }
    d.author_paths.extend(e.author_values.keys().cloned());
    d.conversation_paths
        .extend(e.conversation_values.keys().cloned());
    d.fallback_sensitive |= e.meaning.is_none()
        || e.conversation_mode
            .as_deref()
            .is_some_and(|x| x.contains("fallback") || x.contains("repair"));
    d.repeat_sensitive |= e
        .conversation_mode
        .as_deref()
        .is_some_and(|x| x.contains("repeat"));
}
fn state_deps(state: &gvya_model::GvyaState, d: &mut TestDeps) {
    d.author_paths.extend(state.author.keys().cloned());
    let conversation = &state.conversation;
    if let Some(topic) = &conversation.active_topic {
        d.topics.insert(topic.id.as_str().into());
        d.conversation_paths.insert("active_topic.id".into());
    }
    if let Some(followup) = &conversation.active_followup {
        d.followups.insert(followup.id.as_str().into());
        d.conversation_paths.insert("active_followup.id".into());
    }
    if conversation.last_meaning.is_some() {
        d.conversation_paths.insert("last_meaning".into());
    }
    if conversation.last_behavior.is_some() {
        d.conversation_paths.insert("last_behavior".into());
    }
    if conversation.last_topic.is_some() {
        d.conversation_paths.insert("last_topic".into());
    }
    if conversation.repeat_memory.same_input_count > 0 {
        d.conversation_paths
            .insert("repeat.same_input_count".into());
        d.repeat_sensitive = true;
    }
    if conversation.repeat_memory.same_meaning_count > 0 {
        d.conversation_paths
            .insert("repeat.same_meaning_count".into());
        d.repeat_sensitive = true;
    }
    if conversation.repair.consecutive > 0 {
        d.conversation_paths.insert("repair.consecutive".into());
        d.fallback_sensitive = true;
    }
    if !conversation.focus.is_empty() {
        d.conversation_paths.insert("focus.count".into());
    }
    if conversation.turn_index > 0 {
        d.conversation_paths.insert("turn_index".into());
    }
}
fn regression_deps(r: &RegressionCase) -> TestDeps {
    let mut d = TestDeps::default();
    expectation_deps(&r.expectation, &mut d);
    state_deps(&r.initial_state, &mut d);
    d.context_paths.extend(r.context.values.keys().cloned());
    d
}
fn scenario_deps(s: &ConversationScenario) -> TestDeps {
    let mut d = TestDeps::default();
    state_deps(&s.initial_state, &mut d);
    d.context_paths.extend(s.context.values.keys().cloned());
    for step in &s.steps {
        expectation_deps(step.expectation(), &mut d);
        match step {
            ScenarioStep::Open(v) => {
                d.has_open = true;
                if let Some(context) = &v.context {
                    d.context_paths.extend(context.values.keys().cloned());
                }
            }
            ScenarioStep::CapabilityResult(v) => {
                if let Some(capability) = &v.proposal_capability {
                    d.capabilities.insert(capability.as_str().into());
                }
                if let Some(context) = &v.context {
                    d.context_paths.extend(context.values.keys().cloned());
                }
            }
            ScenarioStep::Confirm(v) => {
                if let Some(capability) = &v.proposal_capability {
                    d.capabilities.insert(capability.as_str().into());
                }
                if let Some(context) = &v.context {
                    d.context_paths.extend(context.values.keys().cloned());
                }
            }
            ScenarioStep::Turn(v) => {
                if let Some(context) = &v.context {
                    d.context_paths.extend(context.values.keys().cloned());
                }
            }
        }
    }
    d
}

fn add_value_path_dep(d: &mut TestDeps, namespace: StateNamespace, path: &str) {
    match namespace {
        StateNamespace::Author => {
            d.author_paths.insert(path.to_owned());
        }
        StateNamespace::Conversation => {
            d.conversation_paths.insert(path.to_owned());
        }
        StateNamespace::Context => {
            d.context_paths.insert(path.to_owned());
        }
        StateNamespace::Meaning | StateNamespace::System | StateNamespace::Interaction => {}
    }
}

fn add_response_impact(
    d: &mut TestDeps,
    responses: &[gvya_kernel::conversation::ResponseDefinition],
) {
    for r in responses {
        d.responses.insert(r.id.as_str().into());
        if let Some(v) = &r.opens_followup {
            d.followups.insert(v.id.as_str().into());
        }
        for c in &r.conditions {
            add_value_path_dep(d, c.path.namespace, &c.path.path);
        }
        for effect in &r.effects {
            match effect {
                gvya_kernel::conversation::ConversationEffect::Assign { target, .. }
                | gvya_kernel::conversation::ConversationEffect::Increment { target, .. } => {
                    let gvya_kernel::conversation::StateTarget::Author(path) = target;
                    d.author_paths.insert(path.clone());
                }
            }
        }
    }
}

fn behavior_impact(b: &ConversationBehavior) -> TestDeps {
    let mut d = TestDeps::default();
    d.meanings.insert(b.meaning.as_str().into());
    if let Some(v) = &b.topic {
        d.topics.insert(v.as_str().into());
    }
    if let Some(v) = &b.followup_scope {
        d.followups.insert(v.as_str().into());
    }
    for req in b.requires_values.iter().chain(b.forbidden_values.iter()) {
        add_value_path_dep(&mut d, req.path.namespace, &req.path.path);
    }
    add_response_impact(&mut d, &b.responses);
    d
}
fn deps_intersect(a: &TestDeps, b: &TestDeps) -> bool {
    !a.meanings.is_disjoint(&b.meanings)
        || !a.responses.is_disjoint(&b.responses)
        || !a.capabilities.is_disjoint(&b.capabilities)
        || !a.topics.is_disjoint(&b.topics)
        || !a.followups.is_disjoint(&b.followups)
        || !a.author_paths.is_disjoint(&b.author_paths)
        || !a.conversation_paths.is_disjoint(&b.conversation_paths)
        || !a.context_paths.is_disjoint(&b.context_paths)
        || (a.has_open && b.has_open)
        || (a.fallback_sensitive && b.fallback_sensitive)
        || (a.repeat_sensitive && b.repeat_sensitive)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ProofShape {
    Direct,
    BehaviorResponse,
    TopicScope,
    TopicActivation,
    FollowupScope,
    FollowupOpen,
    Repeat,
    Repair,
    StateEligibility,
    StateResponse,
    StateEffect,
    Opening,
    Fallback { present_in_candidate: bool },
    CapabilityProposal,
    CapabilityPolicy,
    CapabilityConfirmation { required_in_candidate: bool },
    CapabilityResult,
    Global,
}

#[derive(Clone)]
struct MechanicRequirementSpec {
    mechanic: ChangeMechanic,
    source_kind: Option<ChangeKind>,
    subject: String,
    impact: TestDeps,
    shape: ProofShape,
    covered_by: BTreeSet<SelectedTest>,
}

type MechanicRequirementKey = (ChangeMechanic, Option<ChangeKind>, String);

#[derive(Clone, Default)]
struct TestEvidence {
    deps: TestDeps,
    expected_author_paths: BTreeSet<String>,
    expected_topics: BTreeSet<String>,
    expected_followups: BTreeSet<String>,
    proposal_receipt_capabilities: BTreeSet<String>,
    needs_confirmation_capabilities: BTreeSet<String>,
    non_confirmation_receipt_capabilities: BTreeSet<String>,
    has_open_step: bool,
    has_confirm_step: bool,
    has_capability_result_step: bool,
    fallback_asserted: bool,
    repair_asserted: bool,
    repeat_asserted: bool,
    response_asserted: bool,
}

fn observe_expectation(e: &TurnExpectation, evidence: &mut TestEvidence) {
    evidence
        .expected_author_paths
        .extend(e.author_values.keys().cloned());
    if let Some(topic) = &e.active_topic {
        evidence.expected_topics.insert(topic.as_str().to_owned());
    }
    if let Some(followup) = &e.active_followup {
        evidence
            .expected_followups
            .insert(followup.as_str().to_owned());
    }
    evidence.response_asserted |= !e.response_ids.is_empty()
        || !e.forbidden_response_ids.is_empty()
        || !e.response_contains.is_empty()
        || !e.response_not_contains.is_empty();
    if let Some(mode) = e.conversation_mode.as_deref() {
        evidence.fallback_asserted |= mode.contains("fallback");
        evidence.repair_asserted |= mode.contains("repair");
        evidence.repeat_asserted |= mode.contains("repeat");
    }
    for receipt in &e.proposal_receipts {
        let id = receipt.id.as_str().to_owned();
        evidence.proposal_receipt_capabilities.insert(id.clone());
        match receipt.outcome {
            ExpectedProposalOutcome::NeedsConfirmation => {
                evidence.needs_confirmation_capabilities.insert(id);
            }
            ExpectedProposalOutcome::Admitted | ExpectedProposalOutcome::Rejected => {
                evidence.non_confirmation_receipt_capabilities.insert(id);
            }
        }
    }
}

fn regression_evidence(r: &RegressionCase) -> TestEvidence {
    let mut evidence = TestEvidence {
        deps: regression_deps(r),
        ..TestEvidence::default()
    };
    observe_expectation(&r.expectation, &mut evidence);
    evidence
}

fn scenario_evidence(s: &ConversationScenario) -> TestEvidence {
    let mut evidence = TestEvidence {
        deps: scenario_deps(s),
        ..TestEvidence::default()
    };
    for step in &s.steps {
        observe_expectation(step.expectation(), &mut evidence);
        match step {
            ScenarioStep::Open(_) => evidence.has_open_step = true,
            ScenarioStep::Confirm(_) => evidence.has_confirm_step = true,
            ScenarioStep::CapabilityResult(_) => evidence.has_capability_result_step = true,
            ScenarioStep::Turn(_) => {}
        }
    }
    evidence
}

fn add_requirement(
    requirements: &mut BTreeMap<MechanicRequirementKey, MechanicRequirementSpec>,
    mechanic: ChangeMechanic,
    source_kind: Option<ChangeKind>,
    subject: impl Into<String>,
    impact: TestDeps,
    shape: ProofShape,
) {
    let subject = subject.into();
    let key = (mechanic, source_kind, subject.clone());
    if let Some(existing) = requirements.get_mut(&key) {
        merge_deps(&mut existing.impact, &impact);
        if let (
            ProofShape::CapabilityConfirmation {
                required_in_candidate: left,
            },
            ProofShape::CapabilityConfirmation {
                required_in_candidate: right,
            },
        ) = (&mut existing.shape, shape)
        {
            *left |= right;
        }
        return;
    }
    requirements.insert(
        key,
        MechanicRequirementSpec {
            mechanic,
            source_kind,
            subject,
            impact,
            shape,
            covered_by: BTreeSet::new(),
        },
    );
}

fn response_identity_impact(
    responses: &[gvya_kernel::conversation::ResponseDefinition],
) -> TestDeps {
    let mut impact = TestDeps::default();
    for response in responses {
        impact.responses.insert(response.id.as_str().into());
    }
    impact
}

fn behavior_response_impact(b: &ConversationBehavior) -> TestDeps {
    let mut impact = response_identity_impact(&b.responses);
    impact.meanings.insert(b.meaning.as_str().into());
    impact
}

fn topic_impact(b: &ConversationBehavior) -> TestDeps {
    let mut impact = TestDeps::default();
    if let Some(topic) = &b.topic {
        impact.topics.insert(topic.as_str().into());
    }
    impact
}

fn followup_scope_impact(b: &ConversationBehavior) -> TestDeps {
    let mut impact = TestDeps::default();
    if let Some(followup) = &b.followup_scope {
        impact.followups.insert(followup.as_str().into());
    }
    impact.meanings.insert(b.meaning.as_str().into());
    impact
}

fn response_followup_impact(
    responses: &[gvya_kernel::conversation::ResponseDefinition],
) -> TestDeps {
    let mut impact = TestDeps::default();
    for response in responses {
        if let Some(followup) = &response.opens_followup {
            impact.followups.insert(followup.id.as_str().into());
            impact.responses.insert(response.id.as_str().into());
        }
    }
    impact
}

fn response_repeat_impact(responses: &[gvya_kernel::conversation::ResponseDefinition]) -> TestDeps {
    let mut impact = TestDeps::default();
    for response in responses {
        if matches!(
            response.kind,
            ResponseKind::Repeat | ResponseKind::AnnoyedRepeat | ResponseKind::FinalRepeat
        ) || response.repeat_stage.is_some()
        {
            impact.responses.insert(response.id.as_str().into());
            impact.repeat_sensitive = true;
        }
    }
    impact
}

fn response_condition_impact(
    responses: &[gvya_kernel::conversation::ResponseDefinition],
) -> TestDeps {
    let mut impact = TestDeps::default();
    for response in responses {
        for condition in &response.conditions {
            add_value_path_dep(&mut impact, condition.path.namespace, &condition.path.path);
        }
        if !response.conditions.is_empty() {
            impact.responses.insert(response.id.as_str().into());
        }
    }
    impact
}

fn response_effect_impact(responses: &[gvya_kernel::conversation::ResponseDefinition]) -> TestDeps {
    let mut impact = TestDeps::default();
    for response in responses {
        for effect in &response.effects {
            match effect {
                gvya_kernel::conversation::ConversationEffect::Assign { target, .. }
                | gvya_kernel::conversation::ConversationEffect::Increment { target, .. } => {
                    let gvya_kernel::conversation::StateTarget::Author(path) = target;
                    impact.author_paths.insert(path.clone());
                }
            }
        }
        if !response.effects.is_empty() {
            impact.responses.insert(response.id.as_str().into());
        }
    }
    impact
}

fn behavior_state_eligibility_impact(b: &ConversationBehavior) -> TestDeps {
    let mut impact = TestDeps::default();
    impact.meanings.insert(b.meaning.as_str().into());
    for requirement in b.requires_values.iter().chain(b.forbidden_values.iter()) {
        add_value_path_dep(
            &mut impact,
            requirement.path.namespace,
            &requirement.path.path,
        );
    }
    impact
}

fn behavior_has_repeat_mechanic(behavior: &ConversationBehavior) -> bool {
    behavior.repeat_same_input_after.is_some()
        || behavior.repeat_same_meaning_after.is_some()
        || behavior.responses.iter().any(|response| {
            matches!(
                response.kind,
                ResponseKind::Repeat | ResponseKind::AnnoyedRepeat | ResponseKind::FinalRepeat
            ) || response.repeat_stage.is_some()
        })
}

fn repeat_signature(
    behavior: &ConversationBehavior,
) -> (
    Option<u32>,
    Option<u32>,
    Vec<(
        String,
        ResponseKind,
        Option<gvya_kernel::conversation::RepeatStage>,
    )>,
) {
    (
        behavior.repeat_same_input_after,
        behavior.repeat_same_meaning_after,
        behavior
            .responses
            .iter()
            .map(|response| {
                (
                    response.id.as_str().to_owned(),
                    response.kind,
                    response.repeat_stage,
                )
            })
            .collect(),
    )
}

fn followup_open_signature(
    responses: &[gvya_kernel::conversation::ResponseDefinition],
) -> Vec<(String, Option<(String, u32, bool)>)> {
    responses
        .iter()
        .map(|response| {
            (
                response.id.as_str().to_owned(),
                response.opens_followup.as_ref().map(|followup| {
                    (
                        followup.id.as_str().to_owned(),
                        followup.ttl,
                        followup.refresh_if_same,
                    )
                }),
            )
        })
        .collect()
}

fn response_condition_signature(
    responses: &[gvya_kernel::conversation::ResponseDefinition],
) -> Vec<(String, Vec<gvya_kernel::conversation::ValueCondition>)> {
    responses
        .iter()
        .map(|response| (response.id.as_str().to_owned(), response.conditions.clone()))
        .collect()
}

fn response_effect_signature(
    responses: &[gvya_kernel::conversation::ResponseDefinition],
) -> Vec<(String, Vec<gvya_kernel::conversation::ConversationEffect>)> {
    responses
        .iter()
        .map(|response| (response.id.as_str().to_owned(), response.effects.clone()))
        .collect()
}

fn add_policy_condition_impact(
    impact: &mut TestDeps,
    policy: &gvya_kernel::capability::CapabilityPolicyRule,
) {
    impact
        .capabilities
        .insert(policy.capability.as_str().into());
    for condition in &policy.conditions {
        match condition.namespace {
            AdmissionNamespace::Context => {
                impact.context_paths.insert(condition.path.clone());
            }
            AdmissionNamespace::Author => {
                impact.author_paths.insert(condition.path.clone());
            }
            AdmissionNamespace::Conversation => {
                impact.conversation_paths.insert(condition.path.clone());
            }
            AdmissionNamespace::Arguments | AdmissionNamespace::System => {}
        }
    }
}

fn add_binding_impact(
    project: &ComposedProject,
    binding: &gvya_kernel::capability::CapabilityBindingRule,
    impact: &mut TestDeps,
) {
    impact
        .capabilities
        .insert(binding.capability.as_str().into());
    if let Some(meaning) = &binding.trigger.meaning {
        impact.meanings.insert(meaning.as_str().into());
    }
    if let Some(response) = &binding.trigger.response {
        impact.responses.insert(response.as_str().into());
    }
    if let Some(behavior_id) = &binding.trigger.behavior {
        if let Some(behavior) = project
            .conversation_catalog
            .behaviors()
            .iter()
            .find(|behavior| &behavior.id == behavior_id)
        {
            impact.meanings.insert(behavior.meaning.as_str().into());
            impact.responses.extend(
                behavior
                    .responses
                    .iter()
                    .map(|response| response.id.as_str().to_owned()),
            );
        }
    }
    for argument in &binding.arguments {
        match &argument.source {
            BindingSource::ContextPath(path) => {
                impact.context_paths.insert(path.clone());
            }
            BindingSource::AuthorStatePath(path) => {
                impact.author_paths.insert(path.clone());
            }
            BindingSource::FocusReference { .. } => {
                impact.conversation_paths.insert("focus.count".into());
            }
            BindingSource::MeaningSlot(_)
            | BindingSource::MeaningReference { .. }
            | BindingSource::Literal(_) => {}
        }
    }
}

fn mechanic_requirements(
    base: &ComposedProject,
    candidate: &ComposedProject,
    change_set: &ProjectChangeSet,
    global_proof_scope: bool,
) -> BTreeMap<MechanicRequirementKey, MechanicRequirementSpec> {
    let mut requirements = BTreeMap::new();
    for change in &change_set.changes {
        match change.kind {
            ChangeKind::Meaning => {
                let mut impact = TestDeps::default();
                impact.meanings.insert(change.id.clone());
                add_requirement(
                    &mut requirements,
                    ChangeMechanic::SemanticResolution,
                    Some(change.kind),
                    change.id.clone(),
                    impact,
                    ProofShape::Direct,
                );
            }
            ChangeKind::Behavior => {
                let old = base
                    .conversation_catalog
                    .behaviors()
                    .iter()
                    .find(|row| row.id.as_str() == change.id);
                let new = candidate
                    .conversation_catalog
                    .behaviors()
                    .iter()
                    .find(|row| row.id.as_str() == change.id);
                let mut response_impact = TestDeps::default();
                if let Some(value) = old {
                    merge_deps(&mut response_impact, &behavior_response_impact(value));
                }
                if let Some(value) = new {
                    merge_deps(&mut response_impact, &behavior_response_impact(value));
                }
                add_requirement(
                    &mut requirements,
                    ChangeMechanic::BehaviorResponse,
                    Some(change.kind),
                    change.id.clone(),
                    response_impact,
                    ProofShape::BehaviorResponse,
                );

                if old.map(|value| (&value.topic, value.topic_scoped))
                    != new.map(|value| (&value.topic, value.topic_scoped))
                    && new.is_some_and(|value| value.topic_scoped && value.topic.is_some())
                {
                    let mut impact = TestDeps::default();
                    if let Some(value) = old {
                        merge_deps(&mut impact, &topic_impact(value));
                    }
                    if let Some(value) = new {
                        merge_deps(&mut impact, &topic_impact(value));
                    }
                    if !impact.topics.is_empty() {
                        add_requirement(
                            &mut requirements,
                            ChangeMechanic::TopicScope,
                            Some(change.kind),
                            change.id.clone(),
                            impact,
                            ProofShape::TopicScope,
                        );
                    }
                }
                if old.map(|value| (&value.topic, value.activates_topic, value.topic_ttl))
                    != new.map(|value| (&value.topic, value.activates_topic, value.topic_ttl))
                    && new.is_some_and(|value| value.activates_topic && value.topic.is_some())
                {
                    let mut impact = TestDeps::default();
                    if let Some(value) = old {
                        merge_deps(&mut impact, &topic_impact(value));
                    }
                    if let Some(value) = new {
                        merge_deps(&mut impact, &topic_impact(value));
                    }
                    if !impact.topics.is_empty() {
                        add_requirement(
                            &mut requirements,
                            ChangeMechanic::TopicActivation,
                            Some(change.kind),
                            change.id.clone(),
                            impact,
                            ProofShape::TopicActivation,
                        );
                    }
                }
                if old.map(|value| &value.followup_scope) != new.map(|value| &value.followup_scope)
                    && new.is_some_and(|value| value.followup_scope.is_some())
                {
                    let mut impact = TestDeps::default();
                    if let Some(value) = old {
                        merge_deps(&mut impact, &followup_scope_impact(value));
                    }
                    if let Some(value) = new {
                        merge_deps(&mut impact, &followup_scope_impact(value));
                    }
                    if !impact.followups.is_empty() {
                        add_requirement(
                            &mut requirements,
                            ChangeMechanic::FollowupScope,
                            Some(change.kind),
                            change.id.clone(),
                            impact,
                            ProofShape::FollowupScope,
                        );
                    }
                }
                if old.map(|value| followup_open_signature(&value.responses))
                    != new.map(|value| followup_open_signature(&value.responses))
                    && new.is_some_and(|value| {
                        value
                            .responses
                            .iter()
                            .any(|response| response.opens_followup.is_some())
                    })
                {
                    let mut impact = TestDeps::default();
                    if let Some(value) = old {
                        merge_deps(&mut impact, &response_followup_impact(&value.responses));
                    }
                    if let Some(value) = new {
                        merge_deps(&mut impact, &response_followup_impact(&value.responses));
                    }
                    if !impact.followups.is_empty() {
                        add_requirement(
                            &mut requirements,
                            ChangeMechanic::FollowupOpen,
                            Some(change.kind),
                            change.id.clone(),
                            impact,
                            ProofShape::FollowupOpen,
                        );
                    }
                }
                if old.map(|value| value.repair_continuation_candidate)
                    != new.map(|value| value.repair_continuation_candidate)
                    && new.is_some_and(|value| value.repair_continuation_candidate)
                {
                    let mut impact = TestDeps::default();
                    impact.fallback_sensitive = true;
                    if let Some(value) = new {
                        impact.meanings.insert(value.meaning.as_str().into());
                        merge_deps(&mut impact, &response_identity_impact(&value.responses));
                    }
                    add_requirement(
                        &mut requirements,
                        ChangeMechanic::RepairContinuation,
                        Some(change.kind),
                        change.id.clone(),
                        impact,
                        ProofShape::Repair,
                    );
                }
                if old.map(repeat_signature) != new.map(repeat_signature)
                    && new.is_some_and(behavior_has_repeat_mechanic)
                {
                    let mut impact = TestDeps::default();
                    impact.repeat_sensitive = true;
                    if let Some(value) = old {
                        merge_deps(&mut impact, &response_repeat_impact(&value.responses));
                        impact.meanings.insert(value.meaning.as_str().into());
                    }
                    if let Some(value) = new {
                        merge_deps(&mut impact, &response_repeat_impact(&value.responses));
                        impact.meanings.insert(value.meaning.as_str().into());
                    }
                    add_requirement(
                        &mut requirements,
                        ChangeMechanic::RepeatLadder,
                        Some(change.kind),
                        change.id.clone(),
                        impact,
                        ProofShape::Repeat,
                    );
                }
                if old.map(|value| (&value.requires_values, &value.forbidden_values))
                    != new.map(|value| (&value.requires_values, &value.forbidden_values))
                    && new.is_some_and(|value| {
                        !value.requires_values.is_empty() || !value.forbidden_values.is_empty()
                    })
                {
                    let mut impact = TestDeps::default();
                    if let Some(value) = old {
                        merge_deps(&mut impact, &behavior_state_eligibility_impact(value));
                    }
                    if let Some(value) = new {
                        merge_deps(&mut impact, &behavior_state_eligibility_impact(value));
                    }
                    if !impact.author_paths.is_empty()
                        || !impact.conversation_paths.is_empty()
                        || !impact.context_paths.is_empty()
                    {
                        add_requirement(
                            &mut requirements,
                            ChangeMechanic::StateEligibility,
                            Some(change.kind),
                            change.id.clone(),
                            impact,
                            ProofShape::StateEligibility,
                        );
                    }
                }
                if old.map(|value| response_condition_signature(&value.responses))
                    != new.map(|value| response_condition_signature(&value.responses))
                    && new.is_some_and(|value| {
                        value
                            .responses
                            .iter()
                            .any(|response| !response.conditions.is_empty())
                    })
                {
                    let mut impact = TestDeps::default();
                    if let Some(value) = old {
                        merge_deps(&mut impact, &response_condition_impact(&value.responses));
                    }
                    if let Some(value) = new {
                        merge_deps(&mut impact, &response_condition_impact(&value.responses));
                    }
                    if !impact.author_paths.is_empty()
                        || !impact.conversation_paths.is_empty()
                        || !impact.context_paths.is_empty()
                    {
                        add_requirement(
                            &mut requirements,
                            ChangeMechanic::StateResponse,
                            Some(change.kind),
                            change.id.clone(),
                            impact,
                            ProofShape::StateResponse,
                        );
                    }
                }
                if old.map(|value| response_effect_signature(&value.responses))
                    != new.map(|value| response_effect_signature(&value.responses))
                    && new.is_some_and(|value| {
                        value
                            .responses
                            .iter()
                            .any(|response| !response.effects.is_empty())
                    })
                {
                    let mut impact = TestDeps::default();
                    if let Some(value) = old {
                        merge_deps(&mut impact, &response_effect_impact(&value.responses));
                    }
                    if let Some(value) = new {
                        merge_deps(&mut impact, &response_effect_impact(&value.responses));
                    }
                    if !impact.author_paths.is_empty() {
                        add_requirement(
                            &mut requirements,
                            ChangeMechanic::StateEffect,
                            Some(change.kind),
                            change.id.clone(),
                            impact,
                            ProofShape::StateEffect,
                        );
                    }
                }
            }
            ChangeKind::CapabilityResultBehavior => {
                let mut impact = TestDeps::default();
                for behavior in base
                    .conversation_catalog
                    .capability_result_behaviors()
                    .iter()
                    .filter(|row| row.id.as_str() == change.id)
                    .chain(
                        candidate
                            .conversation_catalog
                            .capability_result_behaviors()
                            .iter()
                            .filter(|row| row.id.as_str() == change.id),
                    )
                {
                    impact
                        .capabilities
                        .insert(behavior.capability.as_str().into());
                    merge_deps(&mut impact, &response_identity_impact(&behavior.responses));
                }
                add_requirement(
                    &mut requirements,
                    ChangeMechanic::CapabilityResult,
                    Some(change.kind),
                    change.id.clone(),
                    impact,
                    ProofShape::CapabilityResult,
                );
            }
            ChangeKind::Opening => {
                let old = base
                    .conversation_catalog
                    .openings()
                    .iter()
                    .find(|row| row.id.as_str() == change.id);
                let new = candidate
                    .conversation_catalog
                    .openings()
                    .iter()
                    .find(|row| row.id.as_str() == change.id);
                let mut impact = TestDeps::default();
                impact.has_open = true;
                for opening in old.into_iter().chain(new) {
                    merge_deps(&mut impact, &response_identity_impact(&opening.responses));
                    if let Some(topic) = &opening.topic {
                        impact.topics.insert(topic.as_str().into());
                    }
                }
                add_requirement(
                    &mut requirements,
                    ChangeMechanic::Opening,
                    Some(change.kind),
                    change.id.clone(),
                    impact.clone(),
                    ProofShape::Opening,
                );
                if old.map(|value| (&value.topic, value.topic_ttl))
                    != new.map(|value| (&value.topic, value.topic_ttl))
                    && new.is_some_and(|value| value.topic.is_some())
                    && !impact.topics.is_empty()
                {
                    add_requirement(
                        &mut requirements,
                        ChangeMechanic::TopicActivation,
                        Some(change.kind),
                        change.id.clone(),
                        impact,
                        ProofShape::TopicActivation,
                    );
                }
            }
            ChangeKind::FallbackBehavior => {
                let old = base
                    .conversation_catalog
                    .fallback_behaviors()
                    .iter()
                    .find(|row| row.id.as_str() == change.id);
                let new = candidate
                    .conversation_catalog
                    .fallback_behaviors()
                    .iter()
                    .find(|row| row.id.as_str() == change.id);
                let mut impact = TestDeps::default();
                impact.fallback_sensitive = true;
                for fallback in old.into_iter().chain(new) {
                    merge_deps(&mut impact, &response_identity_impact(&fallback.responses));
                    for condition in &fallback.conditions {
                        add_value_path_dep(
                            &mut impact,
                            condition.path.namespace,
                            &condition.path.path,
                        );
                    }
                }
                add_requirement(
                    &mut requirements,
                    ChangeMechanic::FallbackRecovery,
                    Some(change.kind),
                    change.id.clone(),
                    impact.clone(),
                    ProofShape::Fallback {
                        present_in_candidate: new.is_some(),
                    },
                );
                if old.map(|value| value.trigger) != new.map(|value| value.trigger)
                    && new.is_some_and(|value| {
                        value.trigger == gvya_kernel::conversation::FallbackTrigger::Repeat
                    })
                {
                    impact.repeat_sensitive = true;
                    add_requirement(
                        &mut requirements,
                        ChangeMechanic::RepeatLadder,
                        Some(change.kind),
                        change.id.clone(),
                        impact,
                        ProofShape::Repeat,
                    );
                }
            }
            ChangeKind::Capability => {
                let id = gvya_model::CapabilityId::new(change.id.clone());
                let old = base.capability_catalog.definition(&id);
                let new = candidate.capability_catalog.definition(&id);
                let mut impact = TestDeps::default();
                impact.capabilities.insert(change.id.clone());
                add_requirement(
                    &mut requirements,
                    ChangeMechanic::CapabilityProposal,
                    Some(change.kind),
                    change.id.clone(),
                    impact.clone(),
                    ProofShape::CapabilityProposal,
                );
                let old_hint = old.map(|value| value.contract.confirmation_hint);
                let new_hint = new.map(|value| value.contract.confirmation_hint);
                if old_hint != new_hint
                    && (old_hint.is_some_and(|value| value != gvya_model::ConfirmationHint::Never)
                        || new_hint
                            .is_some_and(|value| value != gvya_model::ConfirmationHint::Never))
                {
                    add_requirement(
                        &mut requirements,
                        ChangeMechanic::CapabilityConfirmation,
                        Some(change.kind),
                        change.id.clone(),
                        impact,
                        ProofShape::CapabilityConfirmation {
                            required_in_candidate: new_hint
                                .is_some_and(|value| value != gvya_model::ConfirmationHint::Never),
                        },
                    );
                }
            }
            ChangeKind::CapabilityBinding => {
                let mut impact = TestDeps::default();
                for (project, binding) in base
                    .capability_catalog
                    .bindings()
                    .iter()
                    .filter(|row| row.id.as_str() == change.id)
                    .map(|binding| (base, binding))
                    .chain(
                        candidate
                            .capability_catalog
                            .bindings()
                            .iter()
                            .filter(|row| row.id.as_str() == change.id)
                            .map(|binding| (candidate, binding)),
                    )
                {
                    add_binding_impact(project, binding, &mut impact);
                }
                add_requirement(
                    &mut requirements,
                    ChangeMechanic::CapabilityProposal,
                    Some(change.kind),
                    change.id.clone(),
                    impact,
                    ProofShape::CapabilityProposal,
                );
            }
            ChangeKind::CapabilityPolicy => {
                let old = base
                    .capability_catalog
                    .policies()
                    .iter()
                    .find(|row| row.id.as_str() == change.id);
                let new = candidate
                    .capability_catalog
                    .policies()
                    .iter()
                    .find(|row| row.id.as_str() == change.id);
                let mut impact = TestDeps::default();
                for policy in old.into_iter().chain(new) {
                    add_policy_condition_impact(&mut impact, policy);
                }
                add_requirement(
                    &mut requirements,
                    ChangeMechanic::CapabilityPolicy,
                    Some(change.kind),
                    change.id.clone(),
                    impact.clone(),
                    ProofShape::CapabilityPolicy,
                );
                let old_confirmation = old.is_some_and(|policy| {
                    matches!(policy.effect, PolicyEffect::RequireConfirmation { .. })
                });
                let new_confirmation = new.is_some_and(|policy| {
                    matches!(policy.effect, PolicyEffect::RequireConfirmation { .. })
                });
                if old_confirmation || new_confirmation {
                    add_requirement(
                        &mut requirements,
                        ChangeMechanic::CapabilityConfirmation,
                        Some(change.kind),
                        change.id.clone(),
                        impact,
                        ProofShape::CapabilityConfirmation {
                            required_in_candidate: new_confirmation,
                        },
                    );
                }
            }
            ChangeKind::StyleLexicon
            | ChangeKind::CapabilityConfig
            | ChangeKind::Type
            | ChangeKind::Asset
            | ChangeKind::RegressionCase
            | ChangeKind::Scenario => {}
        }
    }
    if global_proof_scope {
        add_requirement(
            &mut requirements,
            ChangeMechanic::GlobalRuntimeContract,
            None,
            "project",
            TestDeps::default(),
            ProofShape::Global,
        );
    }
    requirements
}

fn non_empty_path_intersection(left: &TestDeps, right: &TestDeps) -> bool {
    !left.author_paths.is_disjoint(&right.author_paths)
        || !left
            .conversation_paths
            .is_disjoint(&right.conversation_paths)
        || !left.context_paths.is_disjoint(&right.context_paths)
}

fn capability_intersects(impact: &TestDeps, capabilities: &BTreeSet<String>) -> bool {
    !impact.capabilities.is_disjoint(capabilities)
}

fn test_proves(requirement: &MechanicRequirementSpec, evidence: &TestEvidence) -> bool {
    match requirement.shape {
        ProofShape::Direct => deps_intersect(&requirement.impact, &evidence.deps),
        ProofShape::BehaviorResponse => {
            evidence.response_asserted && deps_intersect(&requirement.impact, &evidence.deps)
        }
        ProofShape::TopicScope => {
            evidence.response_asserted
                && !requirement.impact.topics.is_disjoint(&evidence.deps.topics)
        }
        ProofShape::TopicActivation => !requirement
            .impact
            .topics
            .is_disjoint(&evidence.expected_topics),
        ProofShape::FollowupScope => {
            evidence.response_asserted
                && !requirement
                    .impact
                    .followups
                    .is_disjoint(&evidence.deps.followups)
        }
        ProofShape::FollowupOpen => !requirement
            .impact
            .followups
            .is_disjoint(&evidence.expected_followups),
        ProofShape::Repeat => {
            evidence.repeat_asserted && deps_intersect(&requirement.impact, &evidence.deps)
        }
        ProofShape::Repair => {
            evidence.repair_asserted && deps_intersect(&requirement.impact, &evidence.deps)
        }
        ProofShape::StateEligibility => {
            evidence.response_asserted
                && non_empty_path_intersection(&requirement.impact, &evidence.deps)
                && deps_intersect(&requirement.impact, &evidence.deps)
        }
        ProofShape::StateResponse => {
            evidence.response_asserted
                && non_empty_path_intersection(&requirement.impact, &evidence.deps)
                && deps_intersect(&requirement.impact, &evidence.deps)
        }
        ProofShape::StateEffect => !requirement
            .impact
            .author_paths
            .is_disjoint(&evidence.expected_author_paths),
        ProofShape::Opening => {
            evidence.has_open_step
                && (evidence.response_asserted
                    || !requirement
                        .impact
                        .topics
                        .is_disjoint(&evidence.expected_topics))
        }
        ProofShape::Fallback {
            present_in_candidate,
        } => {
            evidence.response_asserted
                && deps_intersect(&requirement.impact, &evidence.deps)
                && (!present_in_candidate || evidence.fallback_asserted)
        }
        ProofShape::CapabilityProposal => {
            capability_intersects(&requirement.impact, &evidence.deps.capabilities)
                && (requirement.impact.meanings.is_empty()
                    && requirement.impact.responses.is_empty()
                    || !requirement
                        .impact
                        .meanings
                        .is_disjoint(&evidence.deps.meanings)
                    || !requirement
                        .impact
                        .responses
                        .is_disjoint(&evidence.deps.responses))
                && (requirement.impact.author_paths.is_empty()
                    && requirement.impact.conversation_paths.is_empty()
                    && requirement.impact.context_paths.is_empty()
                    || non_empty_path_intersection(&requirement.impact, &evidence.deps))
        }
        ProofShape::CapabilityPolicy => {
            capability_intersects(&requirement.impact, &evidence.proposal_receipt_capabilities)
                && (requirement.impact.author_paths.is_empty()
                    && requirement.impact.conversation_paths.is_empty()
                    && requirement.impact.context_paths.is_empty()
                    || non_empty_path_intersection(&requirement.impact, &evidence.deps))
        }
        ProofShape::CapabilityConfirmation {
            required_in_candidate,
        } => {
            if required_in_candidate {
                evidence.has_confirm_step
                    && capability_intersects(
                        &requirement.impact,
                        &evidence.needs_confirmation_capabilities,
                    )
            } else {
                capability_intersects(
                    &requirement.impact,
                    &evidence.non_confirmation_receipt_capabilities,
                )
            }
        }
        ProofShape::CapabilityResult => {
            evidence.has_capability_result_step
                && evidence.response_asserted
                && capability_intersects(&requirement.impact, &evidence.deps.capabilities)
        }
        ProofShape::Global => true,
    }
}

#[must_use]
pub fn plan_change_tests(
    base: &ComposedProject,
    candidate: &ComposedProject,
    base_source: &ProjectSourceSurface,
    candidate_source: &ProjectSourceSurface,
    limits: ChangePlanLimits,
) -> ChangeTestPlan {
    let change_set = diff_projects(base, candidate, base_source, candidate_source);
    let mut selected: BTreeMap<SelectedTest, Vec<SelectionReason>> = BTreeMap::new();
    let mut full = Vec::new();
    if change_set.package_order_changed {
        full.push("package_order_changed".into());
    }
    if change_set.semantic_profile_changed {
        full.push("semantic_profile_changed".into());
    }
    let structural_pattern_changed = change_set
        .changes
        .iter()
        .filter(|change| change.kind == ChangeKind::Meaning)
        .any(|change| {
            let base_patterns = base
                .semantic_catalog
                .patterns()
                .iter()
                .find(|meaning| meaning.id.as_str() == change.id)
                .map(|meaning| meaning.patterns.as_slice())
                .unwrap_or(&[]);
            let candidate_patterns = candidate
                .semantic_catalog
                .patterns()
                .iter()
                .find(|meaning| meaning.id.as_str() == change.id)
                .map(|meaning| meaning.patterns.as_slice())
                .unwrap_or(&[]);
            base_patterns != candidate_patterns
        });
    if structural_pattern_changed {
        full.push("structural_pattern_changed".into());
    }
    if change_set.project_identity_changed {
        full.push("project_identity_changed".into());
    }
    if change_set.language_config_changed {
        full.push("language_config_changed".into());
    }
    if change_set.semantic_config_changed {
        full.push("semantic_config_changed".into());
    }
    if change_set.conversation_config_changed {
        full.push("conversation_config_changed".into());
    }
    if change_set.changes.iter().any(|c| {
        matches!(
            c.kind,
            ChangeKind::StyleLexicon
                | ChangeKind::CapabilityConfig
                | ChangeKind::Type
                | ChangeKind::Asset
        )
    }) {
        full.push("global_runtime_contract_changed".into());
    }

    let changed_meaning_ids: BTreeSet<String> = change_set
        .changes
        .iter()
        .filter(|c| c.kind == ChangeKind::Meaning)
        .map(|c| c.id.clone())
        .collect();
    let (neighbors, pairs, truncated) =
        semantic_neighbors(base, candidate, &changed_meaning_ids, limits);
    if truncated {
        full.push("semantic_neighbor_budget_exceeded".into());
    }
    let mut direct_impact = TestDeps::default();
    direct_impact.meanings.extend(changed_meaning_ids);
    for change in &change_set.changes {
        match change.kind {
            ChangeKind::Behavior => {
                for b in base
                    .conversation_catalog
                    .behaviors()
                    .iter()
                    .filter(|x| x.id.as_str() == change.id)
                    .chain(
                        candidate
                            .conversation_catalog
                            .behaviors()
                            .iter()
                            .filter(|x| x.id.as_str() == change.id),
                    )
                {
                    merge_deps(&mut direct_impact, &behavior_impact(b));
                }
            }
            ChangeKind::CapabilityResultBehavior => {
                for b in base
                    .conversation_catalog
                    .capability_result_behaviors()
                    .iter()
                    .filter(|x| x.id.as_str() == change.id)
                    .chain(
                        candidate
                            .conversation_catalog
                            .capability_result_behaviors()
                            .iter()
                            .filter(|x| x.id.as_str() == change.id),
                    )
                {
                    direct_impact
                        .capabilities
                        .insert(b.capability.as_str().into());
                    add_response_impact(&mut direct_impact, &b.responses);
                }
            }
            ChangeKind::Opening => {
                direct_impact.has_open = true;
                for opening in base
                    .conversation_catalog
                    .openings()
                    .iter()
                    .filter(|x| x.id.as_str() == change.id)
                    .chain(
                        candidate
                            .conversation_catalog
                            .openings()
                            .iter()
                            .filter(|x| x.id.as_str() == change.id),
                    )
                {
                    if let Some(topic) = &opening.topic {
                        direct_impact.topics.insert(topic.as_str().into());
                    }
                    add_response_impact(&mut direct_impact, &opening.responses);
                }
            }
            ChangeKind::FallbackBehavior => {
                direct_impact.fallback_sensitive = true;
                for fallback in base
                    .conversation_catalog
                    .fallback_behaviors()
                    .iter()
                    .filter(|x| x.id.as_str() == change.id)
                    .chain(
                        candidate
                            .conversation_catalog
                            .fallback_behaviors()
                            .iter()
                            .filter(|x| x.id.as_str() == change.id),
                    )
                {
                    for condition in &fallback.conditions {
                        add_value_path_dep(
                            &mut direct_impact,
                            condition.path.namespace,
                            &condition.path.path,
                        );
                    }
                    add_response_impact(&mut direct_impact, &fallback.responses);
                }
            }
            ChangeKind::Capability => {
                direct_impact.capabilities.insert(change.id.clone());
            }
            ChangeKind::CapabilityBinding => {
                for (project, binding) in base
                    .capability_catalog
                    .bindings()
                    .iter()
                    .filter(|x| x.id.as_str() == change.id)
                    .map(|b| (base, b))
                    .chain(
                        candidate
                            .capability_catalog
                            .bindings()
                            .iter()
                            .filter(|x| x.id.as_str() == change.id)
                            .map(|b| (candidate, b)),
                    )
                {
                    direct_impact
                        .capabilities
                        .insert(binding.capability.as_str().into());
                    if let Some(v) = &binding.trigger.meaning {
                        direct_impact.meanings.insert(v.as_str().into());
                    }
                    if let Some(v) = &binding.trigger.response {
                        direct_impact.responses.insert(v.as_str().into());
                    }
                    if let Some(v) = &binding.trigger.behavior {
                        if let Some(behavior) = project
                            .conversation_catalog
                            .behaviors()
                            .iter()
                            .find(|row| &row.id == v)
                        {
                            direct_impact
                                .meanings
                                .insert(behavior.meaning.as_str().into());
                        }
                    }
                }
            }
            ChangeKind::CapabilityPolicy => {
                for policy in base
                    .capability_catalog
                    .policies()
                    .iter()
                    .filter(|x| x.id.as_str() == change.id)
                    .chain(
                        candidate
                            .capability_catalog
                            .policies()
                            .iter()
                            .filter(|x| x.id.as_str() == change.id),
                    )
                {
                    direct_impact
                        .capabilities
                        .insert(policy.capability.as_str().into());
                }
            }
            _ => {}
        }
    }

    let mut impact = direct_impact.clone();
    for rows in neighbors.values() {
        impact.meanings.extend(rows.iter().cloned());
    }
    let global_proof_scope = change_set.package_order_changed
        || change_set.semantic_profile_changed
        || change_set.language_config_changed
        || change_set.semantic_config_changed
        || change_set.conversation_config_changed
        || change_set.changes.iter().any(|c| {
            matches!(
                c.kind,
                ChangeKind::StyleLexicon
                    | ChangeKind::CapabilityConfig
                    | ChangeKind::Type
                    | ChangeKind::Asset
            )
        });

    let mut requirements = mechanic_requirements(base, candidate, &change_set, global_proof_scope);
    let mut changed_test_count = 0usize;
    let mut proof_tests = BTreeSet::new();

    for r in &candidate.tests.regression_cases {
        let changed = change_set.changes.iter().any(|c| {
            c.kind == ChangeKind::RegressionCase
                && c.id == r.id.as_str()
                && c.status != ChangeStatus::Removed
        });
        let evidence = regression_evidence(r);
        let intersects = deps_intersect(&evidence.deps, &impact);
        let test = SelectedTest::Regression(r.id.as_str().into());
        if changed {
            changed_test_count += 1;
            add_reason(&mut selected, test.clone(), "changed_test", None);
            if !r.generated {
                for requirement in requirements.values_mut() {
                    if test_proves(requirement, &evidence) {
                        requirement.covered_by.insert(test.clone());
                        proof_tests.insert(test.clone());
                        add_reason(
                            &mut selected,
                            test.clone(),
                            "mechanic_proof",
                            Some(format!(
                                "{}:{}",
                                requirement.mechanic.label(),
                                requirement.subject
                            )),
                        );
                    }
                }
            }
        }
        if intersects {
            add_reason(&mut selected, test, "blast_radius", None);
        }
    }
    for s in &candidate.tests.scenarios {
        let changed = change_set.changes.iter().any(|c| {
            c.kind == ChangeKind::Scenario
                && c.id == s.id.as_str()
                && c.status != ChangeStatus::Removed
        });
        let evidence = scenario_evidence(s);
        let intersects = deps_intersect(&evidence.deps, &impact);
        let test = SelectedTest::Scenario(s.id.as_str().into());
        if changed {
            changed_test_count += 1;
            add_reason(&mut selected, test.clone(), "changed_test", None);
            if !s.generated {
                for requirement in requirements.values_mut() {
                    if test_proves(requirement, &evidence) {
                        requirement.covered_by.insert(test.clone());
                        proof_tests.insert(test.clone());
                        add_reason(
                            &mut selected,
                            test.clone(),
                            "mechanic_proof",
                            Some(format!(
                                "{}:{}",
                                requirement.mechanic.label(),
                                requirement.subject
                            )),
                        );
                    }
                }
            }
        }
        if intersects {
            add_reason(&mut selected, test, "blast_radius", None);
        }
    }
    if !change_set.is_empty() {
        select_sentinels(candidate, limits.max_sentinels, &mut selected);
    }
    if !full.is_empty() {
        for r in &candidate.tests.regression_cases {
            add_reason(
                &mut selected,
                SelectedTest::Regression(r.id.as_str().into()),
                "full_suite",
                None,
            );
        }
        for s in &candidate.tests.scenarios {
            add_reason(
                &mut selected,
                SelectedTest::Scenario(s.id.as_str().into()),
                "full_suite",
                None,
            );
        }
    }
    let mechanic_requirements = requirements
        .into_values()
        .map(|requirement| MechanicProofRequirement {
            mechanic: requirement.mechanic,
            source_kind: requirement.source_kind,
            subject: requirement.subject,
            covered_by: requirement.covered_by.into_iter().collect(),
        })
        .collect::<Vec<_>>();
    let mechanic_proof_missing = change_set.mechanic_proof_required()
        && (mechanic_requirements.is_empty()
            || mechanic_requirements
                .iter()
                .any(|requirement| !requirement.covered()));
    ChangeTestPlan {
        change_set,
        full_suite_required: !full.is_empty(),
        full_suite_reasons: full,
        semantic_neighbor_pairs_compared: pairs,
        semantic_neighbor_truncated: truncated,
        changed_test_count,
        proof_test_count: proof_tests.len(),
        mechanic_requirements,
        mechanic_proof_missing,
        selected,
        neighbor_meanings: neighbors,
    }
}
fn merge_deps(a: &mut TestDeps, b: &TestDeps) {
    a.meanings.extend(b.meanings.iter().cloned());
    a.responses.extend(b.responses.iter().cloned());
    a.capabilities.extend(b.capabilities.iter().cloned());
    a.topics.extend(b.topics.iter().cloned());
    a.followups.extend(b.followups.iter().cloned());
    a.author_paths.extend(b.author_paths.iter().cloned());
    a.conversation_paths
        .extend(b.conversation_paths.iter().cloned());
    a.context_paths.extend(b.context_paths.iter().cloned());
    a.has_open |= b.has_open;
    a.fallback_sensitive |= b.fallback_sensitive;
    a.repeat_sensitive |= b.repeat_sensitive;
}
fn add_reason(
    m: &mut BTreeMap<SelectedTest, Vec<SelectionReason>>,
    t: SelectedTest,
    code: &str,
    subject: Option<String>,
) {
    let v = m.entry(t).or_default();
    let r = SelectionReason {
        code: code.into(),
        subject,
    };
    if !v.contains(&r) {
        v.push(r);
    }
}
fn select_sentinels(
    project: &ComposedProject,
    max: usize,
    out: &mut BTreeMap<SelectedTest, Vec<SelectionReason>>,
) {
    if max == 0 {
        return;
    }
    let mut scenarios = project
        .tests
        .scenarios
        .iter()
        .filter(|row| !row.generated)
        .map(|row| SelectedTest::Scenario(row.id.as_str().into()))
        .collect::<Vec<_>>();
    let mut regression = project
        .tests
        .regression_cases
        .iter()
        .filter(|row| !row.generated)
        .map(|row| SelectedTest::Regression(row.id.as_str().into()))
        .collect::<Vec<_>>();
    scenarios.sort();
    regression.sort();
    let scenario_budget = scenarios.len().min(max.min((max / 4).max(1)));
    let regression_budget = max.saturating_sub(scenario_budget).min(regression.len());
    for test in sample_evenly(&scenarios, scenario_budget)
        .into_iter()
        .chain(sample_evenly(&regression, regression_budget))
    {
        add_reason(out, test, "sentinel", None);
    }
}

fn sample_evenly(rows: &[SelectedTest], count: usize) -> Vec<SelectedTest> {
    if count == 0 || rows.is_empty() {
        return Vec::new();
    }
    if rows.len() <= count {
        return rows.to_vec();
    }
    (0..count)
        .map(|index| rows[index * rows.len() / count].clone())
        .collect()
}

fn semantic_neighbors(
    base: &ComposedProject,
    candidate: &ComposedProject,
    ids: &BTreeSet<String>,
    limits: ChangePlanLimits,
) -> (BTreeMap<String, Vec<String>>, usize, bool) {
    let mut out = BTreeMap::new();
    let mut compared = 0usize;
    let mut truncated = false;
    for id in ids {
        let seeds = base
            .semantic_catalog
            .patterns()
            .iter()
            .filter(|p| p.id.as_str() == id)
            .chain(
                candidate
                    .semantic_catalog
                    .patterns()
                    .iter()
                    .filter(|p| p.id.as_str() == id),
            )
            .collect::<Vec<_>>();
        if seeds.is_empty() {
            continue;
        }
        let mut scores = Vec::new();
        for other in candidate.semantic_catalog.patterns() {
            if other.id.as_str() == id {
                continue;
            }
            let mut best = 0.0f64;
            'samples: for seed in &seeds {
                for a in &seed.samples {
                    for b in &other.samples {
                        if normalize_language_tag(&a.language)
                            != normalize_language_tag(&b.language)
                        {
                            continue;
                        }
                        if compared >= limits.max_neighbor_sample_pairs {
                            truncated = true;
                            break 'samples;
                        }
                        let Some(profile) = profile_for_authored_language(
                            &candidate.semantic_profiles,
                            &a.language,
                        ) else {
                            continue;
                        };
                        compared += 1;
                        best = best.max(sample_similarity(profile, &a.text, &b.text));
                    }
                }
            }
            if best >= 0.30 {
                scores.push((best, other.id.as_str().to_owned()));
            }
            if truncated {
                break;
            }
        }
        scores.sort_by(|a, b| b.0.total_cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
        scores.truncate(limits.max_neighbors_per_meaning);
        out.insert(id.clone(), scores.into_iter().map(|x| x.1).collect());
        if truncated {
            break;
        }
    }
    (out, compared, truncated)
}
fn sample_similarity(profile: &SemanticProfile, a: &str, b: &str) -> f64 {
    let ta = semantic_tokens(profile, a);
    let tb = semantic_tokens(profile, b);
    if ta.is_empty() || tb.is_empty() {
        return 0.0;
    }
    let i = ta.intersection(&tb).count();
    let u = ta.union(&tb).count();
    if u == 0 { 0.0 } else { i as f64 / u as f64 }
}
fn semantic_tokens(profile: &SemanticProfile, text: &str) -> BTreeSet<String> {
    let normalized = profile.normalize_text(text);
    let raw = ordered_tokens(&normalized);
    let colloquial = profile.normalize_colloquial_tokens(&raw);
    colloquial
        .into_iter()
        .map(|t| profile.canonical_token(&t))
        .filter(|t| !profile.is_pure_glue(t))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::package::{
        PackageContents, PackageContribution, PackageDefinition, PackageKind, PackageManifest,
        compose_packages,
    };
    use crate::testing::{RegressionCase, TurnExpectation};
    use gvya_kernel::conversation::{
        ConversationBehavior, FollowupDirective, ResponseDefinition, StateNamespace, ValuePath,
        ValueRequirement,
    };
    use gvya_kernel::semantic::{LocalizedStructuralPattern, MeaningPattern};
    use gvya_model::{
        BehaviorId, ContextSnapshot, FollowupId, GvyaState, MeaningId, PackageDigest, PackageId,
        ResponseId, TestCaseId, Value,
    };

    fn test_profiles(profile: SemanticProfile) -> gvya_kernel::semantic::SemanticProfiles {
        BTreeMap::from([
            ("und".to_owned(), profile.clone()),
            ("en".to_owned(), profile.clone()),
            ("en-us".to_owned(), profile),
        ])
    }

    fn behavior(id: &str, meaning: &str) -> ConversationBehavior {
        ConversationBehavior {
            id: BehaviorId::new(id),
            meaning: MeaningId::new(meaning),
            topic: None,
            topic_scoped: false,
            activates_topic: false,
            topic_ttl: None,
            followup_scope: None,
            repair_continuation_candidate: false,
            repeat_same_input_after: None,
            repeat_same_meaning_after: None,
            requires_values: vec![],
            forbidden_values: vec![],
            responses: vec![ResponseDefinition::text(format!("{id}.response"), "en", id)],
        }
    }
    fn case(id: &str, input: &str, meaning: &str, generated: bool) -> RegressionCase {
        RegressionCase {
            id: TestCaseId::new(id),
            description: String::new(),
            input: input.into(),
            language: Some("en".into()),
            context: ContextSnapshot {
                values: BTreeMap::new(),
                visible_references: vec![],
                available_capabilities: vec![],
            },
            initial_state: GvyaState::default(),
            seed: Some(1),
            unix_time_ms: None,
            expectation: TurnExpectation {
                meaning: Some(MeaningId::new(meaning)),
                ..TurnExpectation::default()
            },
            generated,
        }
    }
    fn source_surface() -> ProjectSourceSurface {
        ProjectSourceSurface {
            project_id: "fixture".into(),
            brain_id: "fixture".into(),
            languages: vec!["en-US".into()],
            enabled_languages: vec!["en-US".into()],
            default_language: "en-US".into(),
            semantic_config: SemanticConfig::default(),
            conversation_config: ConversationConfig::default(),
            emit_debug_map: false,
        }
    }

    fn plan(
        base: &ComposedProject,
        candidate: &ComposedProject,
        limits: ChangePlanLimits,
    ) -> ChangeTestPlan {
        let base_source = source_surface();
        let candidate_source = source_surface();
        plan_change_tests(base, candidate, &base_source, &candidate_source, limits)
    }

    fn project_with_behaviors(
        patterns: Vec<MeaningPattern>,
        behaviors: Vec<ConversationBehavior>,
        cases: Vec<RegressionCase>,
    ) -> ComposedProject {
        let pkg = PackageDefinition {
            manifest: PackageManifest {
                id: PackageId::new("p"),
                digest: PackageDigest::new("a".repeat(64)),
                kind: PackageKind::Standard,
                description: String::new(),
                dependencies: vec![],
            },
            contents: PackageContents {
                meanings: patterns
                    .into_iter()
                    .map(|p| {
                        let id = p.id.as_str().to_owned();
                        PackageContribution::add(id, p)
                    })
                    .collect(),
                behaviors: behaviors
                    .into_iter()
                    .map(|b| {
                        let id = b.id.as_str().to_owned();
                        PackageContribution::add(id, b)
                    })
                    .collect(),
                regression_cases: cases
                    .into_iter()
                    .map(|r| {
                        let id = r.id.as_str().to_owned();
                        PackageContribution::add(id, r)
                    })
                    .collect(),
                ..PackageContents::default()
            },
        };
        compose_packages(&[pkg], &test_profiles(SemanticProfile::empty()))
            .project
            .unwrap()
    }

    fn project(
        patterns: Vec<MeaningPattern>,
        cases: Vec<RegressionCase>,
        profile: SemanticProfile,
    ) -> ComposedProject {
        let behaviors = patterns
            .iter()
            .map(|p| {
                PackageContribution::add(
                    format!("{}.behavior", p.id.as_str()),
                    behavior(&format!("{}.behavior", p.id.as_str()), p.id.as_str()),
                )
            })
            .collect();
        let pkg = PackageDefinition {
            manifest: PackageManifest {
                id: PackageId::new("p"),
                digest: PackageDigest::new("a".repeat(64)),
                kind: PackageKind::Standard,
                description: String::new(),
                dependencies: vec![],
            },
            contents: PackageContents {
                meanings: patterns
                    .into_iter()
                    .map(|p| {
                        let id = p.id.as_str().to_owned();
                        PackageContribution::add(id, p)
                    })
                    .collect(),
                behaviors,
                regression_cases: cases
                    .into_iter()
                    .map(|r| {
                        let id = r.id.as_str().to_owned();
                        PackageContribution::add(id, r)
                    })
                    .collect(),
                ..PackageContents::default()
            },
        };
        compose_packages(&[pkg], &test_profiles(profile))
            .project
            .unwrap()
    }

    #[test]
    fn conversation_state_path_connects_behavior_change_to_stateful_test() {
        let mut changed = behavior("a.behavior", "a");
        changed.requires_values.push(ValueRequirement {
            path: ValuePath {
                namespace: StateNamespace::Conversation,
                path: "repair.consecutive".into(),
            },
            value: Value::Number(2.0),
        });
        let impact = behavior_impact(&changed);
        let mut test = case("stateful", "hello", "a", false);
        test.expectation
            .conversation_values
            .insert("repair.consecutive".into(), Value::Number(2.0));
        assert!(deps_intersect(&impact, &regression_deps(&test)));
    }

    #[test]
    fn context_path_connects_behavior_change_to_contextual_test() {
        let mut changed = behavior("a.behavior", "a");
        changed.requires_values.push(ValueRequirement {
            path: ValuePath {
                namespace: StateNamespace::Context,
                path: "channel".into(),
            },
            value: Value::String("game".into()),
        });
        let impact = behavior_impact(&changed);
        let mut test = case("contextual", "hello", "a", false);
        test.context
            .values
            .insert("channel".into(), Value::String("game".into()));
        assert!(deps_intersect(&impact, &regression_deps(&test)));
    }

    #[test]
    fn status_labels_are_stable() {
        assert_eq!(ChangeStatus::Modified.label(), "modified");
        assert_eq!(ChangeKind::Meaning.label(), "meaning");
    }

    #[test]
    fn modified_meaning_selects_direct_neighbor_and_sentinel_but_not_unrelated_generated() {
        let base = project(
            vec![
                MeaningPattern::new("door.open", ["open the door"]),
                MeaningPattern::new("door.unlock", ["unlock the door"]),
                MeaningPattern::new("weather", ["weather tomorrow"]),
            ],
            vec![
                case("open", "open the door", "door.open", true),
                case("unlock", "unlock the door", "door.unlock", true),
                case("weather", "weather tomorrow", "weather", true),
                case("sentinel", "hello", "weather", false),
            ],
            SemanticProfile::empty(),
        );
        let candidate = project(
            vec![
                MeaningPattern::new("door.open", ["open door", "unlock door"]),
                MeaningPattern::new("door.unlock", ["unlock the door"]),
                MeaningPattern::new("weather", ["weather tomorrow"]),
            ],
            vec![
                case("open", "open door", "door.open", true),
                case("unlock", "unlock the door", "door.unlock", true),
                case("weather", "weather tomorrow", "weather", true),
                case("sentinel", "hello", "weather", false),
            ],
            SemanticProfile::empty(),
        );
        let plan = plan(
            &base,
            &candidate,
            ChangePlanLimits {
                max_sentinels: 1,
                ..ChangePlanLimits::default()
            },
        );
        assert!(!plan.full_suite_required);
        assert!(
            plan.selected
                .contains_key(&SelectedTest::Regression("open".into()))
        );
        assert!(
            plan.selected
                .contains_key(&SelectedTest::Regression("unlock".into()))
        );
        assert!(
            plan.selected
                .contains_key(&SelectedTest::Regression("sentinel".into()))
        );
        assert!(
            !plan
                .selected
                .contains_key(&SelectedTest::Regression("weather".into()))
        );
        assert!(plan.neighbor_meanings["door.open"].contains(&"door.unlock".into()));
    }

    #[test]
    fn matcher_profile_change_forces_full_suite() {
        let base = project(
            vec![MeaningPattern::new("a", ["hello"])],
            vec![case("a", "hello", "a", true)],
            SemanticProfile::empty(),
        );
        let mut profile = SemanticProfile::empty();
        profile.pure_glue.insert("please".into());
        let candidate = project(
            vec![MeaningPattern::new("a", ["hello"])],
            vec![case("a", "hello", "a", true)],
            profile,
        );
        let plan = plan(&base, &candidate, ChangePlanLimits::default());
        assert!(plan.full_suite_required);
        assert!(
            plan.full_suite_reasons
                .contains(&"semantic_profile_changed".into())
        );
        assert_eq!(plan.selected.len(), 1);
    }

    #[test]
    fn structural_pattern_change_forces_full_suite() {
        let base = project(
            vec![MeaningPattern::new("a", ["hello"])],
            vec![case("a", "hello", "a", true)],
            SemanticProfile::empty(),
        );
        let mut changed = MeaningPattern::new("a", ["hello"]);
        changed
            .patterns
            .push(LocalizedStructuralPattern::new("en", "^ hello ^"));
        let candidate = project(
            vec![changed],
            vec![case("a", "hello", "a", true)],
            SemanticProfile::empty(),
        );
        let plan = plan(&base, &candidate, ChangePlanLimits::default());
        assert!(plan.full_suite_required);
        assert!(
            plan.full_suite_reasons
                .contains(&"structural_pattern_changed".into())
        );
    }

    #[test]
    fn changed_test_is_always_selected_even_without_runtime_change() {
        let base = project(
            vec![MeaningPattern::new("a", ["hello"])],
            vec![case("a", "hello", "a", true)],
            SemanticProfile::empty(),
        );
        let candidate = project(
            vec![MeaningPattern::new("a", ["hello"])],
            vec![case("a", "hello there", "a", true)],
            SemanticProfile::empty(),
        );
        let plan = plan(
            &base,
            &candidate,
            ChangePlanLimits {
                max_sentinels: 0,
                ..ChangePlanLimits::default()
            },
        );
        assert_eq!(plan.change_set.runtime_changes(), 0);
        assert!(
            plan.selected
                .contains_key(&SelectedTest::Regression("a".into()))
        );
    }

    #[test]
    fn exhausted_neighbor_budget_forces_full_suite_instead_of_under_testing() {
        let base = project(
            vec![
                MeaningPattern::new("a", ["one two"]),
                MeaningPattern::new("b", ["one three"]),
            ],
            vec![
                case("a", "one two", "a", true),
                case("b", "one three", "b", true),
            ],
            SemanticProfile::empty(),
        );
        let candidate = project(
            vec![
                MeaningPattern::new("a", ["one two", "one four"]),
                MeaningPattern::new("b", ["one three"]),
            ],
            vec![
                case("a", "one two", "a", true),
                case("b", "one three", "b", true),
            ],
            SemanticProfile::empty(),
        );
        let plan = plan(
            &base,
            &candidate,
            ChangePlanLimits {
                max_neighbor_sample_pairs: 1,
                max_sentinels: 0,
                ..ChangePlanLimits::default()
            },
        );
        assert!(plan.semantic_neighbor_truncated);
        assert!(plan.full_suite_required);
        assert_eq!(plan.selected.len(), 2);
    }
    #[test]
    fn runtime_patch_without_changed_test_is_explicitly_rejected_by_plan_policy() {
        let base = project(
            vec![MeaningPattern::new("a", ["hello"])],
            vec![case("a", "hello", "a", true)],
            SemanticProfile::empty(),
        );
        let candidate = project(
            vec![MeaningPattern::new("a", ["hello", "hi"])],
            vec![case("a", "hello", "a", true)],
            SemanticProfile::empty(),
        );
        let plan = plan(
            &base,
            &candidate,
            ChangePlanLimits {
                max_sentinels: 0,
                ..ChangePlanLimits::default()
            },
        );
        assert_eq!(plan.changed_test_count, 0);
        assert_eq!(plan.proof_test_count, 0);
        assert!(plan.mechanic_proof_missing);
    }

    #[test]
    fn unrelated_changed_test_does_not_satisfy_mechanic_proof() {
        let base = project(
            vec![
                MeaningPattern::new("a", ["hello"]),
                MeaningPattern::new("b", ["weather"]),
            ],
            vec![
                case("a", "hello", "a", false),
                case("b", "weather", "b", false),
            ],
            SemanticProfile::empty(),
        );
        let candidate = project(
            vec![
                MeaningPattern::new("a", ["hello", "hi"]),
                MeaningPattern::new("b", ["weather"]),
            ],
            vec![
                case("a", "hello", "a", false),
                case("b", "weather today", "b", false),
            ],
            SemanticProfile::empty(),
        );
        let plan = plan(
            &base,
            &candidate,
            ChangePlanLimits {
                max_sentinels: 0,
                ..ChangePlanLimits::default()
            },
        );
        assert_eq!(plan.changed_test_count, 1);
        assert_eq!(plan.proof_test_count, 0);
        assert!(plan.mechanic_proof_missing);
    }

    #[test]
    fn relevant_manual_changed_test_satisfies_mechanic_proof() {
        let base = project(
            vec![MeaningPattern::new("a", ["hello"])],
            vec![case("a", "hello", "a", false)],
            SemanticProfile::empty(),
        );
        let candidate = project(
            vec![MeaningPattern::new("a", ["hello", "hi"])],
            vec![case("a", "hi", "a", false)],
            SemanticProfile::empty(),
        );
        let plan = plan(
            &base,
            &candidate,
            ChangePlanLimits {
                max_sentinels: 0,
                ..ChangePlanLimits::default()
            },
        );
        assert_eq!(plan.changed_test_count, 1);
        assert_eq!(plan.proof_test_count, 1);
        assert!(!plan.mechanic_proof_missing);
    }

    #[test]
    fn generated_changed_test_is_not_manual_mechanic_proof() {
        let base = project(
            vec![MeaningPattern::new("a", ["hello"])],
            vec![case("a", "hello", "a", true)],
            SemanticProfile::empty(),
        );
        let candidate = project(
            vec![MeaningPattern::new("a", ["hello", "hi"])],
            vec![case("a", "hi", "a", true)],
            SemanticProfile::empty(),
        );
        let plan = plan(
            &base,
            &candidate,
            ChangePlanLimits {
                max_sentinels: 0,
                ..ChangePlanLimits::default()
            },
        );
        assert_eq!(plan.changed_test_count, 1);
        assert_eq!(plan.proof_test_count, 0);
        assert!(plan.mechanic_proof_missing);
    }

    #[test]
    fn every_changed_meaning_requires_its_own_direct_mechanic_proof() {
        let base = project(
            vec![
                MeaningPattern::new("a", ["alpha", "alpha alt"]),
                MeaningPattern::new("b", ["beta", "beta alt"]),
            ],
            vec![
                case("a-test", "alpha", "a", false),
                case("b-test", "beta", "b", false),
            ],
            SemanticProfile::empty(),
        );
        let candidate = project(
            vec![
                MeaningPattern::new("a", ["alpha", "alpha alt", "alpha new"]),
                MeaningPattern::new("b", ["beta", "beta alt", "beta new"]),
            ],
            vec![
                case("a-test", "alpha new", "a", false),
                case("b-test", "beta", "b", false),
            ],
            SemanticProfile::empty(),
        );
        let plan = plan(
            &base,
            &candidate,
            ChangePlanLimits {
                max_sentinels: 0,
                ..ChangePlanLimits::default()
            },
        );
        let semantic = plan
            .mechanic_requirements
            .iter()
            .filter(|requirement| requirement.mechanic == ChangeMechanic::SemanticResolution)
            .collect::<Vec<_>>();
        assert_eq!(semantic.len(), 2);
        assert!(semantic.iter().any(|requirement| {
            requirement.subject == "a"
                && requirement.covered_by == vec![SelectedTest::Regression("a-test".into())]
        }));
        assert!(
            semantic
                .iter()
                .any(|requirement| requirement.subject == "b" && !requirement.covered())
        );
        assert_eq!(plan.proof_test_count, 1);
        assert!(plan.mechanic_proof_missing);
    }

    #[test]
    fn all_changed_meanings_can_be_proven_in_the_same_slice() {
        let base = project(
            vec![
                MeaningPattern::new("a", ["alpha", "alpha alt"]),
                MeaningPattern::new("b", ["beta", "beta alt"]),
            ],
            vec![
                case("a-test", "alpha", "a", false),
                case("b-test", "beta", "b", false),
            ],
            SemanticProfile::empty(),
        );
        let candidate = project(
            vec![
                MeaningPattern::new("a", ["alpha", "alpha alt", "alpha new"]),
                MeaningPattern::new("b", ["beta", "beta alt", "beta new"]),
            ],
            vec![
                case("a-test", "alpha new", "a", false),
                case("b-test", "beta new", "b", false),
            ],
            SemanticProfile::empty(),
        );
        let plan = plan(
            &base,
            &candidate,
            ChangePlanLimits {
                max_sentinels: 0,
                ..ChangePlanLimits::default()
            },
        );
        assert_eq!(
            plan.mechanic_requirements
                .iter()
                .filter(|requirement| requirement.mechanic == ChangeMechanic::SemanticResolution)
                .count(),
            2
        );
        assert!(
            plan.mechanic_requirements
                .iter()
                .all(MechanicProofRequirement::covered)
        );
        assert_eq!(plan.proof_test_count, 2);
        assert!(!plan.mechanic_proof_missing);
    }

    #[test]
    fn behavior_change_requires_observable_response_evidence() {
        let patterns = vec![MeaningPattern::new("a", ["hello", "hi"])];
        let base = project_with_behaviors(
            patterns.clone(),
            vec![behavior("a.behavior", "a")],
            vec![case("a-test", "hello", "a", false)],
        );
        let mut changed = behavior("a.behavior", "a");
        changed.responses[0] = ResponseDefinition::text("a.behavior.response", "en", "changed");

        let candidate_without_response_proof = project_with_behaviors(
            patterns.clone(),
            vec![changed.clone()],
            vec![case("a-test", "hi", "a", false)],
        );
        let incomplete = plan(
            &base,
            &candidate_without_response_proof,
            ChangePlanLimits {
                max_sentinels: 0,
                ..ChangePlanLimits::default()
            },
        );
        assert!(incomplete.mechanic_requirements.iter().any(|requirement| {
            requirement.mechanic == ChangeMechanic::BehaviorResponse && !requirement.covered()
        }));
        assert!(incomplete.mechanic_proof_missing);

        let mut proof = case("a-test", "hi", "a", false);
        proof
            .expectation
            .response_ids
            .push(ResponseId::new("a.behavior.response"));
        let candidate = project_with_behaviors(patterns, vec![changed], vec![proof]);
        let complete = plan(
            &base,
            &candidate,
            ChangePlanLimits {
                max_sentinels: 0,
                ..ChangePlanLimits::default()
            },
        );
        assert!(complete.mechanic_requirements.iter().any(|requirement| {
            requirement.mechanic == ChangeMechanic::BehaviorResponse && requirement.covered()
        }));
        assert!(!complete.mechanic_proof_missing);
    }

    #[test]
    fn repair_continuation_requires_repair_mode_evidence() {
        let patterns = vec![MeaningPattern::new("a", ["hello", "hi"])];
        let base = project_with_behaviors(
            patterns.clone(),
            vec![behavior("a.behavior", "a")],
            vec![case("a-test", "hello", "a", false)],
        );
        let mut changed = behavior("a.behavior", "a");
        changed.repair_continuation_candidate = true;
        let candidate_without_repair_evidence = project_with_behaviors(
            patterns.clone(),
            vec![changed.clone()],
            vec![case("a-test", "hi", "a", false)],
        );
        let incomplete = plan(
            &base,
            &candidate_without_repair_evidence,
            ChangePlanLimits {
                max_sentinels: 0,
                ..ChangePlanLimits::default()
            },
        );
        assert!(incomplete.mechanic_requirements.iter().any(|requirement| {
            requirement.mechanic == ChangeMechanic::RepairContinuation && !requirement.covered()
        }));
        assert!(incomplete.mechanic_proof_missing);

        let mut proof = case("a-test", "hi", "a", false);
        proof.expectation.conversation_mode = Some("repair_continuation".into());
        proof
            .expectation
            .response_ids
            .push(ResponseId::new("a.behavior.response"));
        let candidate = project_with_behaviors(patterns, vec![changed], vec![proof]);
        let complete = plan(
            &base,
            &candidate,
            ChangePlanLimits {
                max_sentinels: 0,
                ..ChangePlanLimits::default()
            },
        );
        assert!(complete.mechanic_requirements.iter().any(|requirement| {
            requirement.mechanic == ChangeMechanic::RepairContinuation && requirement.covered()
        }));
        assert!(!complete.mechanic_proof_missing);
    }

    #[test]
    fn followup_change_needs_followup_evidence_not_only_behavior_evidence() {
        let patterns = vec![MeaningPattern::new("a", ["hello", "hi"])];
        let base = project_with_behaviors(
            patterns.clone(),
            vec![behavior("a.behavior", "a")],
            vec![case("a-test", "hello", "a", false)],
        );
        let mut changed = behavior("a.behavior", "a");
        changed.responses[0].opens_followup = Some(FollowupDirective {
            id: FollowupId::new("a.next"),
            ttl: 2,
            refresh_if_same: false,
        });
        let candidate = project_with_behaviors(
            patterns,
            vec![changed],
            vec![case("a-test", "hi", "a", false)],
        );
        let plan = plan(
            &base,
            &candidate,
            ChangePlanLimits {
                max_sentinels: 0,
                ..ChangePlanLimits::default()
            },
        );
        assert!(plan.mechanic_requirements.iter().any(|requirement| {
            requirement.mechanic == ChangeMechanic::BehaviorResponse && !requirement.covered()
        }));
        assert!(plan.mechanic_requirements.iter().any(|requirement| {
            requirement.mechanic == ChangeMechanic::FollowupOpen && !requirement.covered()
        }));
        assert!(plan.mechanic_proof_missing);
    }

    #[test]
    fn removing_followup_mechanic_uses_behavior_response_proof_without_impossible_positive_state() {
        let patterns = vec![MeaningPattern::new("a", ["hello", "hi"])];
        let mut original = behavior("a.behavior", "a");
        original.responses[0].opens_followup = Some(FollowupDirective {
            id: FollowupId::new("a.next"),
            ttl: 2,
            refresh_if_same: false,
        });
        let base = project_with_behaviors(
            patterns.clone(),
            vec![original],
            vec![case("a-test", "hello", "a", false)],
        );

        let changed = behavior("a.behavior", "a");
        let mut proof = case("a-test", "hi", "a", false);
        proof
            .expectation
            .response_ids
            .push(ResponseId::new("a.behavior.response"));
        let candidate = project_with_behaviors(patterns, vec![changed], vec![proof]);
        let plan = plan(
            &base,
            &candidate,
            ChangePlanLimits {
                max_sentinels: 0,
                ..ChangePlanLimits::default()
            },
        );

        assert!(plan.mechanic_requirements.iter().any(|requirement| {
            requirement.mechanic == ChangeMechanic::BehaviorResponse && requirement.covered()
        }));
        assert!(
            !plan
                .mechanic_requirements
                .iter()
                .any(|requirement| requirement.mechanic == ChangeMechanic::FollowupOpen)
        );
        assert!(!plan.mechanic_proof_missing);
    }

    #[test]
    fn followup_change_is_complete_when_changed_test_asserts_followup() {
        let patterns = vec![MeaningPattern::new("a", ["hello", "hi"])];
        let base = project_with_behaviors(
            patterns.clone(),
            vec![behavior("a.behavior", "a")],
            vec![case("a-test", "hello", "a", false)],
        );
        let mut changed = behavior("a.behavior", "a");
        changed.responses[0].opens_followup = Some(FollowupDirective {
            id: FollowupId::new("a.next"),
            ttl: 2,
            refresh_if_same: false,
        });
        let mut proof = case("a-test", "hi", "a", false);
        proof.expectation.active_followup = Some(FollowupId::new("a.next"));
        proof
            .expectation
            .response_ids
            .push(ResponseId::new("a.behavior.response"));
        let candidate = project_with_behaviors(patterns, vec![changed], vec![proof]);
        let plan = plan(
            &base,
            &candidate,
            ChangePlanLimits {
                max_sentinels: 0,
                ..ChangePlanLimits::default()
            },
        );
        assert!(
            plan.mechanic_requirements
                .iter()
                .all(MechanicProofRequirement::covered)
        );
        assert!(!plan.mechanic_proof_missing);
    }

    #[test]
    fn no_change_has_no_sentinel_work() {
        let base = project(
            vec![MeaningPattern::new("a", ["hello"])],
            vec![case("sentinel", "hello", "a", false)],
            SemanticProfile::empty(),
        );
        let plan = plan(&base, &base, ChangePlanLimits::default());
        assert!(plan.change_set.is_empty());
        assert!(plan.selected.is_empty());
        assert!(!plan.mechanic_proof_missing);
    }

    #[test]
    fn modified_meaning_keeps_old_neighbors_in_blast_radius() {
        let base = project(
            vec![
                MeaningPattern::new("a", ["unlock door"]),
                MeaningPattern::new("b", ["unlock gate"]),
            ],
            vec![
                case("a-test", "unlock door", "a", false),
                case("b-test", "unlock gate", "b", false),
            ],
            SemanticProfile::empty(),
        );
        let candidate = project(
            vec![
                MeaningPattern::new("a", ["sing a song"]),
                MeaningPattern::new("b", ["unlock gate"]),
            ],
            vec![
                case("a-test", "sing a song", "a", false),
                case("b-test", "unlock gate", "b", false),
            ],
            SemanticProfile::empty(),
        );
        let plan = plan(
            &base,
            &candidate,
            ChangePlanLimits {
                max_sentinels: 0,
                ..ChangePlanLimits::default()
            },
        );
        assert!(plan.neighbor_meanings["a"].contains(&"b".into()));
        assert!(
            plan.selected
                .contains_key(&SelectedTest::Regression("b-test".into()))
        );
    }

    #[test]
    fn modified_behavior_unions_old_and_new_meaning_dependencies() {
        let patterns = vec![
            MeaningPattern::new("a", ["alpha"]),
            MeaningPattern::new("b", ["beta"]),
        ];
        let base = project_with_behaviors(
            patterns.clone(),
            vec![behavior("shared.behavior", "a")],
            vec![
                case("a-test", "alpha", "a", false),
                case("b-test", "beta", "b", false),
            ],
        );
        let candidate = project_with_behaviors(
            patterns,
            vec![behavior("shared.behavior", "b")],
            vec![
                case("a-test", "alpha", "a", false),
                case("b-test", "beta", "b", false),
            ],
        );
        let plan = plan(
            &base,
            &candidate,
            ChangePlanLimits {
                max_sentinels: 0,
                ..ChangePlanLimits::default()
            },
        );
        assert!(
            plan.selected
                .contains_key(&SelectedTest::Regression("a-test".into()))
        );
        assert!(
            plan.selected
                .contains_key(&SelectedTest::Regression("b-test".into()))
        );
    }

    #[test]
    fn conversation_config_change_is_global_and_requires_mechanic_proof() {
        let project = project(
            vec![MeaningPattern::new("a", ["hello"])],
            vec![case("a", "hello", "a", false)],
            SemanticProfile::empty(),
        );
        let base_source = source_surface();
        let mut candidate_source = source_surface();
        candidate_source
            .conversation_config
            .repair_candidate_min_score = 0.55;
        let plan = plan_change_tests(
            &project,
            &project,
            &base_source,
            &candidate_source,
            ChangePlanLimits::default(),
        );
        assert!(plan.change_set.conversation_config_changed);
        assert!(plan.full_suite_required);
        assert!(
            plan.full_suite_reasons
                .contains(&"conversation_config_changed".into())
        );
        assert!(plan.mechanic_proof_missing);
        assert_eq!(plan.selected.len(), 1);
    }

    #[test]
    fn debug_map_only_change_is_visible_but_does_not_require_conversation_proof() {
        let project = project(
            vec![MeaningPattern::new("a", ["hello"])],
            vec![case("a", "hello", "a", false)],
            SemanticProfile::empty(),
        );
        let base_source = source_surface();
        let mut candidate_source = source_surface();
        candidate_source.emit_debug_map = true;
        let plan = plan_change_tests(
            &project,
            &project,
            &base_source,
            &candidate_source,
            ChangePlanLimits {
                max_sentinels: 0,
                ..ChangePlanLimits::default()
            },
        );
        assert!(plan.change_set.debug_map_changed);
        assert!(!plan.change_set.is_empty());
        assert!(!plan.full_suite_required);
        assert!(!plan.mechanic_proof_missing);
    }

    #[test]
    fn neighbor_only_changed_test_does_not_satisfy_direct_mechanic_proof() {
        let base = project(
            vec![
                MeaningPattern::new("a", ["unlock door"]),
                MeaningPattern::new("b", ["unlock gate"]),
            ],
            vec![
                case("a-test", "unlock door", "a", false),
                case("b-test", "unlock gate", "b", false),
            ],
            SemanticProfile::empty(),
        );
        let candidate = project(
            vec![
                MeaningPattern::new("a", ["unlock door", "open door"]),
                MeaningPattern::new("b", ["unlock gate"]),
            ],
            vec![
                case("a-test", "unlock door", "a", false),
                case("b-test", "please unlock gate", "b", false),
            ],
            SemanticProfile::empty(),
        );
        let plan = plan(
            &base,
            &candidate,
            ChangePlanLimits {
                max_sentinels: 0,
                ..ChangePlanLimits::default()
            },
        );
        assert!(plan.neighbor_meanings["a"].contains(&"b".into()));
        assert_eq!(plan.changed_test_count, 1);
        assert_eq!(plan.proof_test_count, 0);
        assert!(plan.mechanic_proof_missing);
        assert!(
            plan.selected
                .contains_key(&SelectedTest::Regression("b-test".into()))
        );
    }
}
