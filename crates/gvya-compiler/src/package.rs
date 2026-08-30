//! Deterministic package graph, explicit specialization and runtime-catalog composition.
//!
//! Packages are compiler inputs, not runtime plug-ins. Filesystem/load order carries no authority.
//! A contribution either adds a new logical item or explicitly replaces one exported contribution
//! owned by a visible dependency. Replacement is whole-item only; there is no hidden partial merge.

mod graph;
mod merge;
#[cfg(test)]
mod tests;

use graph::*;
use merge::*;

use std::collections::{BTreeMap, BTreeSet};

use gvya_kernel::{
    capability::{
        CapabilityBindingRule, CapabilityCatalog, CapabilityConfig, CapabilityDefinition,
        CapabilityPolicyRule, IssueSeverity,
    },
    conversation::{
        CapabilityResultBehavior, ConversationBehavior, ConversationCatalog, FallbackBehavior,
        OpeningDefinition, StyleLexicon,
    },
    semantic::{MeaningPattern, SemanticCatalog, SemanticProfiles},
};
use gvya_model::{AssetId, PackageDigest, PackageId, TypeId};

use crate::testing::{ConversationScenario, RegressionCase, TestSuite};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PackageDependency {
    pub id: PackageId,
    /// Makes this dependency visible to packages that depend on this package.
    pub reexport: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PackageKind {
    Standard,
    Fallback,
}

impl PackageKind {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Standard => "standard",
            Self::Fallback => "fallback",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PackageManifest {
    pub id: PackageId,
    /// SHA-256 of the canonical source package. compiler/artifact layer owns byte-level canonicalization/hashing.
    pub digest: PackageDigest,
    pub kind: PackageKind,
    pub description: String,
    pub dependencies: Vec<PackageDependency>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ContributionMode {
    Add,
    Replace {
        target_package: PackageId,
        target_id: String,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct PackageContribution<T> {
    /// Logical identity inside this contribution namespace.
    pub id: String,
    /// Whether a visible dependent package may explicitly replace this item.
    pub exported: bool,
    pub mode: ContributionMode,
    pub value: T,
}

impl<T> PackageContribution<T> {
    #[must_use]
    pub fn add(id: impl Into<String>, value: T) -> Self {
        Self {
            id: id.into(),
            exported: true,
            mode: ContributionMode::Add,
            value,
        }
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct StyleLexiconPatch {
    pub formal_terms: BTreeSet<String>,
    pub informal_terms: BTreeSet<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PackageAsset {
    pub id: AssetId,
    pub media_type: String,
    pub logical_path: String,
    pub digest: PackageDigest,
}

#[derive(Clone, Debug, PartialEq)]
pub struct NamedTypeDefinition {
    pub id: TypeId,
    pub schema: gvya_kernel::capability::ValueSchema,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct PackageContents {
    pub meanings: Vec<PackageContribution<MeaningPattern>>,
    pub behaviors: Vec<PackageContribution<ConversationBehavior>>,
    pub capability_result_behaviors: Vec<PackageContribution<CapabilityResultBehavior>>,
    pub openings: Vec<PackageContribution<OpeningDefinition>>,
    pub fallback_behaviors: Vec<PackageContribution<FallbackBehavior>>,
    pub style_lexicons: Vec<PackageContribution<StyleLexiconPatch>>,
    pub capabilities: Vec<PackageContribution<CapabilityDefinition>>,
    pub capability_bindings: Vec<PackageContribution<CapabilityBindingRule>>,
    pub capability_policies: Vec<PackageContribution<CapabilityPolicyRule>>,
    pub capability_configs: Vec<PackageContribution<CapabilityConfig>>,
    pub types: Vec<PackageContribution<NamedTypeDefinition>>,
    pub assets: Vec<PackageContribution<PackageAsset>>,
    pub regression_cases: Vec<PackageContribution<RegressionCase>>,
    pub scenarios: Vec<PackageContribution<ConversationScenario>>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PackageDefinition {
    pub manifest: PackageManifest,
    pub contents: PackageContents,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum ContributionKind {
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

impl ContributionKind {
    #[must_use]
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

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContributionProvenance {
    pub owner: PackageId,
    pub exported: bool,
    pub replaced: Option<PackageId>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum CompositionSeverity {
    Error,
    Warning,
    Info,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompositionIssue {
    pub severity: CompositionSeverity,
    pub code: String,
    pub package: Option<PackageId>,
    pub kind: Option<ContributionKind>,
    pub item_id: Option<String>,
    pub message: String,
}

impl CompositionIssue {
    fn error(
        code: impl Into<String>,
        package: Option<PackageId>,
        kind: Option<ContributionKind>,
        item_id: Option<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            severity: CompositionSeverity::Error,
            code: code.into(),
            package,
            kind,
            item_id,
            message: message.into(),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ComposedItem<T> {
    pub id: String,
    pub value: T,
    pub provenance: ContributionProvenance,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ComposedProject {
    pub package_order: Vec<PackageId>,
    pub semantic_catalog: SemanticCatalog,
    pub semantic_profiles: SemanticProfiles,
    pub conversation_catalog: ConversationCatalog,
    pub capability_catalog: CapabilityCatalog,
    pub types: BTreeMap<TypeId, NamedTypeDefinition>,
    pub assets: BTreeMap<AssetId, PackageAsset>,
    pub tests: TestSuite,
    pub provenance: BTreeMap<(ContributionKind, String), ContributionProvenance>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CompositionResult {
    pub project: Option<ComposedProject>,
    pub issues: Vec<CompositionIssue>,
}

#[derive(Clone, Debug)]
struct GraphPackage<'a> {
    definition: &'a PackageDefinition,
    visible_dependencies: BTreeSet<PackageId>,
}

#[derive(Clone, Debug)]
struct Namespace<T> {
    items: BTreeMap<String, ComposedItem<T>>,
}

impl<T> Default for Namespace<T> {
    fn default() -> Self {
        Self {
            items: BTreeMap::new(),
        }
    }
}

pub fn compose_packages(
    packages: &[PackageDefinition],
    semantic_profiles: &SemanticProfiles,
) -> CompositionResult {
    let (graph, order, mut issues) = validate_graph(packages);
    if has_errors(&issues) {
        return CompositionResult {
            project: None,
            issues,
        };
    }

    let mut meanings = Namespace::default();
    let mut behaviors = Namespace::default();
    let mut capability_result_behaviors = Namespace::default();
    let mut openings = Namespace::default();
    let mut fallback_behaviors = Namespace::default();
    let mut style_lexicons = Namespace::default();
    let mut capabilities = Namespace::default();
    let mut capability_bindings = Namespace::default();
    let mut capability_policies = Namespace::default();
    let mut capability_configs = Namespace::default();
    let mut types = Namespace::default();
    let mut assets = Namespace::default();
    let mut regression_cases = Namespace::default();
    let mut scenarios = Namespace::default();

    for package_id in &order {
        let Some(node) = graph.get(package_id) else {
            continue;
        };
        let manifest = &node.definition.manifest;
        let visible = &node.visible_dependencies;
        let contents = &node.definition.contents;

        apply_all(
            &mut meanings,
            &contents.meanings,
            ContributionKind::Meaning,
            manifest,
            visible,
            &mut issues,
        );
        apply_all(
            &mut behaviors,
            &contents.behaviors,
            ContributionKind::Behavior,
            manifest,
            visible,
            &mut issues,
        );
        apply_all(
            &mut capability_result_behaviors,
            &contents.capability_result_behaviors,
            ContributionKind::CapabilityResultBehavior,
            manifest,
            visible,
            &mut issues,
        );
        apply_all(
            &mut openings,
            &contents.openings,
            ContributionKind::Opening,
            manifest,
            visible,
            &mut issues,
        );
        apply_all(
            &mut fallback_behaviors,
            &contents.fallback_behaviors,
            ContributionKind::FallbackBehavior,
            manifest,
            visible,
            &mut issues,
        );
        apply_all(
            &mut style_lexicons,
            &contents.style_lexicons,
            ContributionKind::StyleLexicon,
            manifest,
            visible,
            &mut issues,
        );
        apply_all(
            &mut capabilities,
            &contents.capabilities,
            ContributionKind::Capability,
            manifest,
            visible,
            &mut issues,
        );
        apply_all(
            &mut capability_bindings,
            &contents.capability_bindings,
            ContributionKind::CapabilityBinding,
            manifest,
            visible,
            &mut issues,
        );
        apply_all(
            &mut capability_policies,
            &contents.capability_policies,
            ContributionKind::CapabilityPolicy,
            manifest,
            visible,
            &mut issues,
        );
        apply_all(
            &mut capability_configs,
            &contents.capability_configs,
            ContributionKind::CapabilityConfig,
            manifest,
            visible,
            &mut issues,
        );
        apply_all(
            &mut types,
            &contents.types,
            ContributionKind::Type,
            manifest,
            visible,
            &mut issues,
        );
        apply_all(
            &mut assets,
            &contents.assets,
            ContributionKind::Asset,
            manifest,
            visible,
            &mut issues,
        );
        apply_all(
            &mut regression_cases,
            &contents.regression_cases,
            ContributionKind::RegressionCase,
            manifest,
            visible,
            &mut issues,
        );
        apply_all(
            &mut scenarios,
            &contents.scenarios,
            ContributionKind::Scenario,
            manifest,
            visible,
            &mut issues,
        );
    }

    if has_errors(&issues) {
        return CompositionResult {
            project: None,
            issues,
        };
    }

    let style_lexicon = compose_style_lexicon(style_lexicons.items.values());
    let capability_config =
        match single_setting(&capability_configs, "capability_config", &mut issues) {
            Some(value) => value.clone(),
            None => CapabilityConfig::default(),
        };

    let semantic_catalog = match SemanticCatalog::new(
        meanings
            .items
            .values()
            .map(|item| item.value.clone())
            .collect(),
    ) {
        Ok(catalog) => catalog,
        Err(error) => {
            issues.push(CompositionIssue::error(
                "semantic_catalog_invalid",
                None,
                Some(ContributionKind::Meaning),
                None,
                format!("semantic catalog validation failed: {error:?}"),
            ));
            return CompositionResult {
                project: None,
                issues,
            };
        }
    };

    let conversation_catalog = match ConversationCatalog::new(
        behaviors
            .items
            .values()
            .map(|item| item.value.clone())
            .collect(),
        capability_result_behaviors
            .items
            .values()
            .map(|item| item.value.clone())
            .collect(),
        openings
            .items
            .values()
            .map(|item| item.value.clone())
            .collect(),
        fallback_behaviors
            .items
            .values()
            .map(|item| item.value.clone())
            .collect(),
    ) {
        Ok(catalog) => catalog.with_style_lexicon(style_lexicon),
        Err(error) => {
            issues.push(CompositionIssue::error(
                "conversation_catalog_invalid",
                None,
                Some(ContributionKind::Behavior),
                None,
                format!("conversation catalog validation failed: {error:?}"),
            ));
            return CompositionResult {
                project: None,
                issues,
            };
        }
    };

    let capability_catalog = match CapabilityCatalog::new(
        capabilities
            .items
            .values()
            .map(|item| item.value.clone())
            .collect(),
        capability_bindings
            .items
            .values()
            .map(|item| item.value.clone())
            .collect(),
        capability_policies
            .items
            .values()
            .map(|item| item.value.clone())
            .collect(),
        capability_config,
    ) {
        Ok(catalog) => catalog,
        Err(catalog_issues) => {
            for issue in catalog_issues {
                issues.push(CompositionIssue {
                    severity: match issue.severity {
                        IssueSeverity::Error => CompositionSeverity::Error,
                        IssueSeverity::Warning => CompositionSeverity::Warning,
                    },
                    code: format!("capability.{}", issue.code),
                    package: None,
                    kind: Some(ContributionKind::Capability),
                    item_id: Some(issue.subject),
                    message: issue.message,
                });
            }
            return CompositionResult {
                project: None,
                issues,
            };
        }
    };

    let mut provenance = BTreeMap::new();
    collect_provenance(&mut provenance, ContributionKind::Meaning, &meanings);
    collect_provenance(&mut provenance, ContributionKind::Behavior, &behaviors);
    collect_provenance(
        &mut provenance,
        ContributionKind::CapabilityResultBehavior,
        &capability_result_behaviors,
    );
    collect_provenance(&mut provenance, ContributionKind::Opening, &openings);
    collect_provenance(
        &mut provenance,
        ContributionKind::FallbackBehavior,
        &fallback_behaviors,
    );
    collect_provenance(
        &mut provenance,
        ContributionKind::StyleLexicon,
        &style_lexicons,
    );
    collect_provenance(&mut provenance, ContributionKind::Capability, &capabilities);
    collect_provenance(
        &mut provenance,
        ContributionKind::CapabilityBinding,
        &capability_bindings,
    );
    collect_provenance(
        &mut provenance,
        ContributionKind::CapabilityPolicy,
        &capability_policies,
    );
    collect_provenance(
        &mut provenance,
        ContributionKind::CapabilityConfig,
        &capability_configs,
    );
    collect_provenance(&mut provenance, ContributionKind::Type, &types);
    collect_provenance(&mut provenance, ContributionKind::Asset, &assets);
    collect_provenance(
        &mut provenance,
        ContributionKind::RegressionCase,
        &regression_cases,
    );
    collect_provenance(&mut provenance, ContributionKind::Scenario, &scenarios);

    let typed = types
        .items
        .values()
        .map(|item| (item.value.id.clone(), item.value.clone()))
        .collect();
    let asset_map = assets
        .items
        .values()
        .map(|item| (item.value.id.clone(), item.value.clone()))
        .collect();
    let tests = TestSuite {
        regression_cases: regression_cases
            .items
            .values()
            .map(|item| item.value.clone())
            .collect(),
        scenarios: scenarios
            .items
            .values()
            .map(|item| item.value.clone())
            .collect(),
    };

    if has_errors(&issues) {
        return CompositionResult {
            project: None,
            issues,
        };
    }

    CompositionResult {
        project: Some(ComposedProject {
            package_order: order,
            semantic_catalog,
            semantic_profiles: semantic_profiles.clone(),
            conversation_catalog,
            capability_catalog,
            types: typed,
            assets: asset_map,
            tests,
            provenance,
        }),
        issues,
    }
}
