//! Transparent JSON source-project format and deterministic source resolution.
//!
//! `.gvya` is never a source file. The compiler reads `gvya.project.json` plus explicit package
//! documents, resolves asset bytes from the supplied `SourceTree`, derives package digests itself,
//! and produces package-layer `PackageDefinition` values before composition/audit.

mod content;
pub mod contract;
mod decode;
pub mod inventory;
mod testing;

use content::*;
use decode::*;
use testing::*;

use std::collections::{BTreeMap, BTreeSet};

use gvya_kernel::{
    ResolverReferenceCandidate,
    capability::{
        AdmissionNamespace, AdmissionPredicate, ArgumentBinding, ArgumentPath, BindingSource,
        CapabilityBindingRule, CapabilityConfig, CapabilityDefinition, CapabilityPolicyRule,
        CapabilityTrigger, HostEffectDeclaration, HostEffectKind, PolicyEffect,
        PredicateOp as AdmissionPredicateOp, ReferenceProjection, SchemaLimits,
        validate_capability_config,
    },
    conversation::{
        AuthorNumberDefinition, CapabilityResultBehavior, ConversationBehavior, ConversationConfig,
        ConversationEffect, ExtraMessage, FallbackBehavior, FallbackTrigger, FollowupDirective,
        HintRequest, LocalizedTexts, OpeningDefinition, PredicateOp as ConversationPredicateOp,
        ResponseAsset, ResponseDefinition, ResponseKind, ResponseLink, StateNamespace, StateTarget,
        TEMPLATE_MAX_OUTPUT_BYTES, ValueCondition, ValuePath, ValueRequirement,
        language_tag_is_well_formed, normalize_locale,
    },
    semantic::{
        ElicitationPrompt, EntityKind, LocalizedSample, LocalizedStructuralPattern, MeaningClass,
        MeaningPattern, ReferenceSpec, SEMANTIC_NEGATIVE_SAMPLES_PER_MEANING_MAX,
        SEMANTIC_PATTERNS_PER_MEANING_MAX, SEMANTIC_RETRIEVAL_TERMS_PER_MEANING_MAX,
        SEMANTIC_SAMPLES_PER_MEANING_MAX, SEMANTIC_TEXT_ITEM_MAX_BYTES,
        SEMANTIC_TEXT_PER_MEANING_MAX_BYTES, SemanticConfig, SemanticProfile, SemanticProfiles,
        SlotKind, SlotSpec,
    },
};
use gvya_model::{
    ActiveFollowup, ActiveTopic, AssetId, AvailableCapability, BehaviorId, CapabilityBindingId,
    CapabilityContract, CapabilityId, CapabilityVersion, ConfirmationHint, ContextSnapshot,
    EffectClass, FollowupId, Formality, GvyaState, HostReference, MeaningId, PackageDigest,
    PackageId, PolicyId, ReferenceId, ReferenceKind, RepairMemory, RepeatMemory, ResponseId,
    ScenarioId, SchemaDocument, TestCaseId, TopicId, TraceCode, TypeId, UserStyle, Value,
};
use serde_json::Value as JsonValue;

use crate::{
    canonical::{canonical_json, sha256_hex},
    package::{
        ContributionMode, NamedTypeDefinition, PackageAsset, PackageContents, PackageContribution,
        PackageDefinition, PackageDependency, PackageKind, PackageManifest, StyleLexiconPatch,
    },
    schema_compile::compile_json_schema,
    testing::{
        ConversationScenario, ExpectedCapability, ExpectedProposalOutcome, ExpectedProposalReceipt,
        RegressionCase, ScenarioCapabilityResultStep, ScenarioConfirmStep, ScenarioOpenStep,
        ScenarioStep, ScenarioTurnStep, TurnExpectation,
    },
};

pub const PROJECT_SOURCE_FORMAT: &str = "gvya.source.project";
pub const PACKAGE_SOURCE_FORMAT: &str = "gvya.source.package";
pub const PACKAGE_SOURCE_VERSION: u32 = 1;
pub const LANGUAGE_PROFILE_SOURCE_FORMAT: &str = "gvya.source.language-profile";
pub const MATCHER_PROFILE_SOURCE_FORMAT: &str = "gvya.source.matcher-profile";
pub const SOURCE_VERSION: u32 = 1;
pub const SOURCE_LANGUAGES_MAX: usize = 32;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SourceLimits {
    pub max_files: usize,
    pub max_file_bytes: usize,
    pub max_asset_bytes: usize,
    pub max_total_bytes: usize,
    pub max_packages: usize,
    pub max_contributions_per_kind: usize,
    pub max_string_bytes: usize,
}

impl Default for SourceLimits {
    fn default() -> Self {
        Self {
            max_files: 50_000,
            max_file_bytes: 8 * 1024 * 1024,
            max_asset_bytes: 64 * 1024 * 1024,
            max_total_bytes: 512 * 1024 * 1024,
            max_packages: 2_048,
            max_contributions_per_kind: 50_000,
            max_string_bytes: 256 * 1024,
        }
    }
}

impl SourceLimits {
    /// Source limits are caller-selectable only downward. No API caller may relax the canonical
    /// compiler ceilings and thereby create a different resource-safety contract.
    pub fn validate_canonical(self) -> Result<(), SourceIssue> {
        let ceiling = Self::default();
        let valid = self.max_files > 0
            && self.max_files <= ceiling.max_files
            && self.max_file_bytes > 0
            && self.max_file_bytes <= ceiling.max_file_bytes
            && self.max_asset_bytes > 0
            && self.max_asset_bytes <= ceiling.max_asset_bytes
            && self.max_total_bytes > 0
            && self.max_total_bytes <= ceiling.max_total_bytes
            && self.max_packages > 0
            && self.max_packages <= ceiling.max_packages
            && self.max_contributions_per_kind > 0
            && self.max_contributions_per_kind <= ceiling.max_contributions_per_kind
            && self.max_string_bytes > 0
            && self.max_string_bytes <= ceiling.max_string_bytes;
        if valid {
            Ok(())
        } else {
            Err(issue(
                "source.limit_contract",
                "SourceLimits may tighten but never relax the canonical compiler ceilings",
                None,
            ))
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct SourceTree {
    files: BTreeMap<String, Vec<u8>>,
}

impl SourceTree {
    pub fn new(
        files: BTreeMap<String, Vec<u8>>,
        limits: SourceLimits,
    ) -> Result<Self, Vec<SourceIssue>> {
        let mut issues = Vec::new();
        if let Err(limit_issue) = limits.validate_canonical() {
            issues.push(limit_issue);
            return Err(issues);
        }
        if files.len() > limits.max_files {
            issues.push(issue(
                "source.file_limit",
                "source tree contains too many files",
                None,
            ));
        }
        let mut total = 0_usize;
        for (path, bytes) in &files {
            if !safe_source_path(path) {
                issues.push(issue(
                    "source.invalid_path",
                    "source file path is not safe/canonical",
                    Some(path),
                ));
            }
            if bytes.len() > limits.max_asset_bytes {
                issues.push(issue(
                    "source.file_too_large",
                    "source file exceeds absolute file limit",
                    Some(path),
                ));
            }
            total = total.saturating_add(bytes.len());
        }
        if total > limits.max_total_bytes {
            issues.push(issue(
                "source.total_size",
                "source tree exceeds total byte limit",
                None,
            ));
        }
        if issues.is_empty() {
            Ok(Self { files })
        } else {
            Err(issues)
        }
    }

    #[must_use]
    pub fn get(&self, path: &str) -> Option<&[u8]> {
        self.files.get(path).map(Vec::as_slice)
    }
    #[must_use]
    pub fn contains(&self, path: &str) -> bool {
        self.files.contains_key(path)
    }

    /// Stable identity of the exact declared source snapshot. The checkout/root path is excluded;
    /// canonical relative source paths and bytes are included with length framing.
    #[must_use]
    pub fn fingerprint_sha256(&self) -> String {
        let mut framed = Vec::with_capacity(
            self.files
                .iter()
                .map(|(path, bytes)| path.len().saturating_add(bytes.len()).saturating_add(16))
                .sum::<usize>()
                .saturating_add(24),
        );
        framed.extend_from_slice(b"gvya.source-tree/1\0");
        for (path, bytes) in &self.files {
            framed.extend_from_slice(&(path.len() as u64).to_be_bytes());
            framed.extend_from_slice(path.as_bytes());
            framed.extend_from_slice(&(bytes.len() as u64).to_be_bytes());
            framed.extend_from_slice(bytes);
        }
        sha256_hex(&framed)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceIssue {
    pub code: String,
    pub path: Option<String>,
    pub message: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SourceProject {
    pub project_id: String,
    pub brain_id: String,
    pub languages: Vec<String>,
    pub enabled_languages: Vec<String>,
    pub default_language: String,
    pub language_profile_files: Vec<String>,
    pub matcher_profile_files: Vec<String>,
    pub package_files: Vec<String>,
    pub fallback_package_file: Option<String>,
    pub semantic_config: SemanticConfig,
    pub conversation_config: ConversationConfig,
    pub emit_debug_map: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ResolvedSourceProject {
    pub project: SourceProject,
    pub packages: Vec<PackageDefinition>,
    pub semantic_profiles: SemanticProfiles,
    pub source_language_profile_digests: BTreeMap<String, String>,
    pub source_matcher_profile_digests: BTreeMap<String, String>,
    pub asset_bytes_by_digest: BTreeMap<String, Vec<u8>>,
    pub source_package_digests: BTreeMap<String, String>,
}

pub fn resolve_source_project(
    tree: &SourceTree,
    limits: SourceLimits,
) -> Result<ResolvedSourceProject, Vec<SourceIssue>> {
    let project_value = parse_json_file(tree, "gvya.project.json", limits)?;
    let project = match parse_project(&project_value, limits) {
        Ok(value) => value,
        Err(issues) => return Err(issues),
    };
    if project.package_files.len() + usize::from(project.fallback_package_file.is_some())
        > limits.max_packages
    {
        return Err(vec![issue(
            "source.package_limit",
            "project declares too many packages",
            Some("gvya.project.json"),
        )]);
    }
    let mut packages = Vec::new();
    let mut semantic_profiles: SemanticProfiles = project
        .enabled_languages
        .iter()
        .map(|language| (normalize_locale(language), SemanticProfile::empty()))
        .collect();
    let mut source_language_profile_digests = BTreeMap::new();
    let mut source_matcher_profile_digests = BTreeMap::new();
    let mut asset_bytes_by_digest = BTreeMap::new();
    let mut source_package_digests = BTreeMap::new();
    let mut issues = Vec::new();
    let mut package_ids = BTreeSet::new();
    let mut declared_packages: Vec<(&String, PackageKind)> = project
        .package_files
        .iter()
        .map(|path| (path, PackageKind::Standard))
        .collect();
    if let Some(path) = &project.fallback_package_file {
        declared_packages.push((path, PackageKind::Fallback));
    }
    for (package_file, expected_kind) in declared_packages {
        if !safe_source_path(package_file) {
            issues.push(issue(
                "source.package_path",
                "package file path is not canonical",
                Some(package_file),
            ));
            continue;
        }
        let value = match parse_json_file(tree, package_file, limits) {
            Ok(value) => value,
            Err(mut rows) => {
                issues.append(&mut rows);
                continue;
            }
        };
        match parse_package(tree, package_file, &value, limits) {
            Ok((package, assets, digest)) => {
                if package.manifest.kind != expected_kind {
                    issues.push(issue(
                        "source.package_kind_slot",
                        &format!(
                            "project slot requires a {} package but {} declares kind {}",
                            expected_kind.label(),
                            package.manifest.id.as_str(),
                            package.manifest.kind.label()
                        ),
                        Some(package_file),
                    ));
                    continue;
                }
                if !package_ids.insert(package.manifest.id.as_str().to_owned()) {
                    issues.push(issue(
                        "source.duplicate_package_id",
                        "project contains duplicate package id",
                        Some(package_file),
                    ));
                }
                for (_id, bytes) in assets {
                    asset_bytes_by_digest
                        .entry(sha256_hex(&bytes))
                        .or_insert(bytes);
                }
                source_package_digests.insert(package.manifest.id.as_str().to_owned(), digest);
                packages.push(package);
            }
            Err(mut rows) => issues.append(&mut rows),
        }
    }
    let enabled_languages: BTreeSet<String> = project
        .enabled_languages
        .iter()
        .map(|language| normalize_locale(language))
        .collect();
    for language_profile_file in &project.language_profile_files {
        if !safe_source_path(language_profile_file) {
            issues.push(issue(
                "source.language_profile_path",
                "language-profile file path is not canonical",
                Some(language_profile_file),
            ));
            continue;
        }
        let value = match parse_json_file(tree, language_profile_file, limits) {
            Ok(value) => value,
            Err(mut rows) => {
                issues.append(&mut rows);
                continue;
            }
        };
        match parse_language_profile(language_profile_file, &value, limits) {
            Ok((language, profile, digest)) => {
                let normalized = normalize_locale(&language);
                let expected_suffix = format!("language-profiles/{normalized}.json");
                if language_profile_file != &expected_suffix
                    && !language_profile_file.ends_with(&format!("/{expected_suffix}"))
                {
                    issues.push(issue(
                        "source.language_profile_path",
                        &format!(
                            "language-profile language {language} path must end with {expected_suffix}"
                        ),
                        Some(language_profile_file),
                    ));
                    continue;
                }
                if !enabled_languages.contains(&normalized) {
                    issues.push(issue(
                        "source.language_profile_not_enabled",
                        &format!(
                            "language-profile language {language} is not enabled for this Brain"
                        ),
                        Some(language_profile_file),
                    ));
                    continue;
                }
                if source_language_profile_digests
                    .insert(normalized.clone(), digest)
                    .is_some()
                {
                    issues.push(issue(
                        "source.language_profile_duplicate",
                        "only one language-profile may be selected for each normalized language",
                        Some(language_profile_file),
                    ));
                    continue;
                }
                semantic_profiles.insert(normalized, profile);
            }
            Err(mut rows) => issues.append(&mut rows),
        }
    }
    for matcher_profile_file in &project.matcher_profile_files {
        if !safe_source_path(matcher_profile_file) {
            issues.push(issue(
                "source.matcher_profile_path",
                "matcher-profile file path is not canonical",
                Some(matcher_profile_file),
            ));
            continue;
        }
        let value = match parse_json_file(tree, matcher_profile_file, limits) {
            Ok(value) => value,
            Err(mut rows) => {
                issues.append(&mut rows);
                continue;
            }
        };
        match parse_matcher_profile(matcher_profile_file, &value, limits) {
            Ok((language, profile, digest)) => {
                let normalized = normalize_locale(&language);
                let expected_suffix = format!("matcher-profiles/{normalized}.json");
                if matcher_profile_file != &expected_suffix
                    && !matcher_profile_file.ends_with(&format!("/{expected_suffix}"))
                {
                    issues.push(issue(
                        "source.matcher_profile_path",
                        &format!(
                            "matcher-profile language {language} path must end with {expected_suffix}"
                        ),
                        Some(matcher_profile_file),
                    ));
                    continue;
                }
                if !enabled_languages.contains(&normalized) {
                    issues.push(issue(
                        "source.matcher_profile_not_enabled",
                        &format!(
                            "matcher-profile language {language} is not enabled for this Brain"
                        ),
                        Some(matcher_profile_file),
                    ));
                    continue;
                }
                if source_matcher_profile_digests
                    .insert(normalized.clone(), digest)
                    .is_some()
                {
                    issues.push(issue(
                        "source.matcher_profile_duplicate",
                        "only one matcher-profile may be selected for each normalized language",
                        Some(matcher_profile_file),
                    ));
                    continue;
                }
                let Some(language_profile) = semantic_profiles.get_mut(&normalized) else {
                    issues.push(issue(
                        "source.profile_pair_mismatch",
                        "matcher-profile has no paired language-profile",
                        Some(matcher_profile_file),
                    ));
                    continue;
                };
                merge_semantic_profile(
                    language_profile,
                    &profile,
                    matcher_profile_file,
                    &mut issues,
                );
            }
            Err(mut rows) => issues.append(&mut rows),
        }
    }
    if source_language_profile_digests
        .keys()
        .ne(source_matcher_profile_digests.keys())
    {
        issues.push(issue(
            "source.profile_pair_mismatch",
            "language_profiles and matcher_profiles must select the same normalized languages",
            Some("gvya.project.json"),
        ));
    }
    validate_project_language_usage(&project.languages, &packages, &mut issues);
    if issues.is_empty() {
        packages.sort_by(|left, right| left.manifest.id.as_str().cmp(right.manifest.id.as_str()));
        Ok(ResolvedSourceProject {
            project,
            packages,
            semantic_profiles,
            source_language_profile_digests,
            source_matcher_profile_digests,
            asset_bytes_by_digest,
            source_package_digests,
        })
    } else {
        Err(issues)
    }
}

fn parse_language_profile(
    language_profile_file: &str,
    root: &JsonValue,
    limits: SourceLimits,
) -> Result<(String, SemanticProfile, String), Vec<SourceIssue>> {
    let mut issues = Vec::new();
    let Ok(obj) = expect_object(root, language_profile_file, &mut issues) else {
        return Err(issues);
    };
    reject_unknown_keys(
        obj,
        contract::LANGUAGE_PROFILE_KEYS,
        language_profile_file,
        &mut issues,
    );
    require_exact_format(
        obj,
        LANGUAGE_PROFILE_SOURCE_FORMAT,
        language_profile_file,
        &mut issues,
    );
    let language = required_string(obj, "language", language_profile_file, limits, &mut issues)
        .unwrap_or_default();
    if !language_tag_is_well_formed(&language) {
        issues.push(issue(
            "source.language_profile_language",
            "language-profile language must be a well-formed BCP 47 tag",
            Some(language_profile_file),
        ));
    }
    let profile = obj
        .get("profile")
        .and_then(|value| {
            parse_language_profile_data(
                value,
                &format!("{language_profile_file}#profile"),
                limits,
                &mut issues,
            )
        })
        .unwrap_or_else(SemanticProfile::empty);
    if obj.get("profile").is_none() {
        issues.push(issue(
            "source.language_profile_missing",
            "language-profile document requires a profile object",
            Some(language_profile_file),
        ));
    }
    let digest = canonical_json(root)
        .map(|bytes| sha256_hex(&bytes))
        .map_err(|error| {
            vec![issue(
                "source.canonical_json",
                &format!("cannot canonicalize language-profile source: {error:?}"),
                Some(language_profile_file),
            )]
        })?;
    if issues.is_empty() {
        Ok((language, profile, digest))
    } else {
        Err(issues)
    }
}

fn parse_matcher_profile(
    matcher_profile_file: &str,
    root: &JsonValue,
    limits: SourceLimits,
) -> Result<(String, SemanticProfile, String), Vec<SourceIssue>> {
    let mut issues = Vec::new();
    let Ok(obj) = expect_object(root, matcher_profile_file, &mut issues) else {
        return Err(issues);
    };
    reject_unknown_keys(
        obj,
        contract::MATCHER_PROFILE_KEYS,
        matcher_profile_file,
        &mut issues,
    );
    require_exact_format(
        obj,
        MATCHER_PROFILE_SOURCE_FORMAT,
        matcher_profile_file,
        &mut issues,
    );
    let language = required_string(obj, "language", matcher_profile_file, limits, &mut issues)
        .unwrap_or_default();
    if !language_tag_is_well_formed(&language) {
        issues.push(issue(
            "source.matcher_profile_language",
            "matcher-profile language must be a well-formed BCP 47 tag",
            Some(matcher_profile_file),
        ));
    }
    let profile = obj
        .get("profile")
        .and_then(|value| {
            parse_matcher_profile_data(
                value,
                &format!("{matcher_profile_file}#profile"),
                limits,
                &mut issues,
            )
        })
        .unwrap_or_else(SemanticProfile::empty);
    if obj.get("profile").is_none() {
        issues.push(issue(
            "source.matcher_profile_missing",
            "matcher-profile document requires a profile object",
            Some(matcher_profile_file),
        ));
    }
    let digest = canonical_json(root)
        .map(|bytes| sha256_hex(&bytes))
        .map_err(|error| {
            vec![issue(
                "source.canonical_json",
                &format!("cannot canonicalize matcher-profile source: {error:?}"),
                Some(matcher_profile_file),
            )]
        })?;
    if issues.is_empty() {
        Ok((language, profile, digest))
    } else {
        Err(issues)
    }
}

fn merge_profile_map<T: Clone + PartialEq>(
    target: &mut BTreeMap<String, T>,
    incoming: &BTreeMap<String, T>,
    profile_file: &str,
    field: &str,
    issues: &mut Vec<SourceIssue>,
) {
    for (key, value) in incoming {
        match target.get(key) {
            Some(existing) if existing != value => issues.push(issue(
                "source.profile_conflict",
                &format!("conflicting profile mapping for {field}.{key}"),
                Some(profile_file),
            )),
            Some(_) => {}
            None => {
                target.insert(key.clone(), value.clone());
            }
        }
    }
}

fn merge_profile_pattern_sets(
    target: &mut BTreeMap<String, BTreeMap<String, String>>,
    incoming: &BTreeMap<String, BTreeMap<String, String>>,
    profile_file: &str,
    issues: &mut Vec<SourceIssue>,
) {
    for (set_name, aliases) in incoming {
        let target_aliases = target.entry(set_name.clone()).or_default();
        for (alias, canonical) in aliases {
            match target_aliases.get(alias) {
                Some(existing) if existing != canonical => issues.push(issue(
                    "source.profile_conflict",
                    &format!("conflicting profile mapping for pattern_sets.{set_name}.{alias}"),
                    Some(profile_file),
                )),
                Some(_) => {}
                None => {
                    target_aliases.insert(alias.clone(), canonical.clone());
                }
            }
        }
    }
}

fn merge_semantic_profile(
    target: &mut SemanticProfile,
    incoming: &SemanticProfile,
    profile_file: &str,
    issues: &mut Vec<SourceIssue>,
) {
    merge_profile_map(
        &mut target.canonical_tokens,
        &incoming.canonical_tokens,
        profile_file,
        "canonical_tokens",
        issues,
    );
    merge_profile_map(
        &mut target.canonical_suffixes,
        &incoming.canonical_suffixes,
        profile_file,
        "canonical_suffixes",
        issues,
    );
    merge_profile_map(
        &mut target.normalization_rewrites,
        &incoming.normalization_rewrites,
        profile_file,
        "normalization_rewrites",
        issues,
    );
    merge_profile_map(
        &mut target.colloquial,
        &incoming.colloquial,
        profile_file,
        "colloquial",
        issues,
    );
    merge_profile_map(
        &mut target.number_words,
        &incoming.number_words,
        profile_file,
        "number_words",
        issues,
    );
    merge_profile_map(
        &mut target.relative_dates,
        &incoming.relative_dates,
        profile_file,
        "relative_dates",
        issues,
    );
    merge_profile_map(
        &mut target.colors,
        &incoming.colors,
        profile_file,
        "colors",
        issues,
    );
    merge_profile_map(
        &mut target.units,
        &incoming.units,
        profile_file,
        "units",
        issues,
    );
    merge_profile_map(
        &mut target.origin_prefixes,
        &incoming.origin_prefixes,
        profile_file,
        "origin_prefixes",
        issues,
    );
    merge_profile_pattern_sets(
        &mut target.pattern_sets,
        &incoming.pattern_sets,
        profile_file,
        issues,
    );
    target
        .normalization_remove_chars
        .extend(incoming.normalization_remove_chars.iter().cloned());
    target
        .canonical_suffix_exceptions
        .extend(incoming.canonical_suffix_exceptions.iter().cloned());
    target
        .detached_suffixes
        .extend(incoming.detached_suffixes.iter().cloned());
    target.pure_glue.extend(incoming.pure_glue.iter().cloned());
    target
        .very_low_weight
        .extend(incoming.very_low_weight.iter().cloned());
    target
        .low_weight
        .extend(incoming.low_weight.iter().cloned());
    target
        .context_low_weight
        .extend(incoming.context_low_weight.iter().cloned());
    target
        .generic_singletons
        .extend(incoming.generic_singletons.iter().cloned());
    target
        .reporting_verbs
        .extend(incoming.reporting_verbs.iter().cloned());
    target
        .reporting_nouns
        .extend(incoming.reporting_nouns.iter().cloned());
    target.pronouns.extend(incoming.pronouns.iter().cloned());
    target.negations.extend(incoming.negations.iter().cloned());
    target
        .social_vocabulary
        .extend(incoming.social_vocabulary.iter().cloned());
    target.task_cues.extend(incoming.task_cues.iter().cloned());
    target
        .weak_numeric_ignore
        .extend(incoming.weak_numeric_ignore.iter().cloned());
    target
        .continuation_exact_phrases
        .extend(incoming.continuation_exact_phrases.iter().cloned());
    target
        .continuation_question_starters
        .extend(incoming.continuation_question_starters.iter().cloned());
    target
        .continuation_references
        .extend(incoming.continuation_references.iter().cloned());
    target
        .generic_followup_phrases
        .extend(incoming.generic_followup_phrases.iter().cloned());
    target
        .time_markers
        .extend(incoming.time_markers.iter().cloned());
}

fn validate_project_language_usage(
    languages: &[String],
    packages: &[PackageDefinition],
    issues: &mut Vec<SourceIssue>,
) {
    let allowed: BTreeSet<String> = languages
        .iter()
        .map(|value| normalize_locale(value))
        .collect();
    let mut check = |language: &str, owner: &str| {
        if !language_tag_is_well_formed(language) {
            issues.push(issue(
                "source.language_tag",
                &format!("{owner} uses a malformed language tag {language}"),
                Some("gvya.project.json#languages"),
            ));
        } else if !allowed.contains(&normalize_locale(language)) {
            issues.push(issue(
                "source.language_not_allowed",
                &format!("{owner} uses {language}, which is not selected by this Project"),
                Some("gvya.project.json#languages"),
            ));
        }
    };
    for package in packages {
        let package_id = package.manifest.id.as_str();
        for contribution in &package.contents.meanings {
            for rule in &contribution.value.patterns {
                check(
                    &rule.language,
                    &format!(
                        "Meaning {} structural pattern in Package {package_id}",
                        contribution.id
                    ),
                );
            }
            for sample in contribution
                .value
                .samples
                .iter()
                .chain(contribution.value.negative_samples.iter())
                .chain(contribution.value.retrieval_terms.iter())
            {
                check(
                    &sample.language,
                    &format!("Meaning {} in Package {package_id}", contribution.id),
                );
            }
        }
        let mut check_responses = |owner: &str, responses: &[ResponseDefinition]| {
            for response in responses {
                for texts in &response.texts {
                    check(
                        &texts.language,
                        &format!(
                            "Response {} on {owner} in Package {package_id}",
                            response.id.as_str()
                        ),
                    );
                }
                for extra in &response.extra_messages {
                    for texts in &extra.texts {
                        check(
                            &texts.language,
                            &format!(
                                "Extra message on Response {} in Package {package_id}",
                                response.id.as_str()
                            ),
                        );
                    }
                }
            }
        };
        for contribution in &package.contents.behaviors {
            check_responses(&contribution.id, &contribution.value.responses);
        }
        for contribution in &package.contents.capability_result_behaviors {
            check_responses(&contribution.id, &contribution.value.responses);
        }
        for contribution in &package.contents.openings {
            check_responses(&contribution.id, &contribution.value.responses);
        }
        for contribution in &package.contents.fallback_behaviors {
            check_responses(&contribution.id, &contribution.value.responses);
        }
        for contribution in &package.contents.regression_cases {
            if let Some(language) = &contribution.value.language {
                check(
                    language,
                    &format!(
                        "Regression Case {} in Package {package_id}",
                        contribution.id
                    ),
                );
            }
        }
        for contribution in &package.contents.scenarios {
            for (index, step) in contribution.value.steps.iter().enumerate() {
                let language = match step {
                    ScenarioStep::Open(step) => step.language.as_ref(),
                    ScenarioStep::Turn(step) => step.language.as_ref(),
                    ScenarioStep::CapabilityResult(step) => step.language.as_ref(),
                    ScenarioStep::Confirm(_) => None,
                };
                if let Some(language) = language {
                    check(
                        language,
                        &format!(
                            "Scenario {} step {} in Package {package_id}",
                            contribution.id,
                            index + 1
                        ),
                    );
                }
            }
        }
    }
}

fn parse_project(
    root: &JsonValue,
    limits: SourceLimits,
) -> Result<SourceProject, Vec<SourceIssue>> {
    let mut issues = Vec::new();
    let Ok(obj) = expect_object(root, "gvya.project.json", &mut issues) else {
        return Err(issues);
    };
    reject_unknown_keys(
        obj,
        contract::PROJECT_KEYS,
        "gvya.project.json",
        &mut issues,
    );
    require_exact_format(obj, PROJECT_SOURCE_FORMAT, "gvya.project.json", &mut issues);
    let project_id = required_string(obj, "project_id", "gvya.project.json", limits, &mut issues)
        .unwrap_or_default();
    let brain_id = required_string(obj, "brain_id", "gvya.project.json", limits, &mut issues)
        .unwrap_or_default();
    let languages = string_array(
        obj.get("languages"),
        "gvya.project.json#languages",
        limits,
        &mut issues,
    );
    if languages.is_empty() || languages.len() > SOURCE_LANGUAGES_MAX {
        issues.push(issue(
            "source.languages",
            "project languages must contain 1..=32 ordered BCP 47 tags",
            Some("gvya.project.json#languages"),
        ));
    }
    let mut normalized_languages = BTreeSet::new();
    for language in &languages {
        if !language_tag_is_well_formed(language) {
            issues.push(issue(
                "source.language_tag",
                "project language must be a well-formed hyphenated BCP 47 tag",
                Some("gvya.project.json#languages"),
            ));
        } else if !normalized_languages.insert(normalize_locale(language)) {
            issues.push(issue(
                "source.language_duplicate",
                "project languages must be unique after locale normalization",
                Some("gvya.project.json#languages"),
            ));
        }
    }
    let enabled_languages = string_array(
        obj.get("enabled_languages"),
        "gvya.project.json#enabled_languages",
        limits,
        &mut issues,
    );
    if enabled_languages.is_empty() || enabled_languages.len() > SOURCE_LANGUAGES_MAX {
        issues.push(issue(
            "source.enabled_languages",
            "enabled_languages must contain 1..=32 ordered BCP 47 tags",
            Some("gvya.project.json#enabled_languages"),
        ));
    }
    let mut normalized_enabled_languages = BTreeSet::new();
    for language in &enabled_languages {
        let normalized = normalize_locale(language);
        if !language_tag_is_well_formed(language) {
            issues.push(issue(
                "source.enabled_language_tag",
                "enabled language must be a well-formed hyphenated BCP 47 tag",
                Some("gvya.project.json#enabled_languages"),
            ));
        } else if !normalized_languages.contains(&normalized) {
            issues.push(issue(
                "source.enabled_language_not_declared",
                "enabled language must be one of the declared project languages",
                Some("gvya.project.json#enabled_languages"),
            ));
        } else if !normalized_enabled_languages.insert(normalized) {
            issues.push(issue(
                "source.enabled_language_duplicate",
                "enabled_languages must be unique after locale normalization",
                Some("gvya.project.json#enabled_languages"),
            ));
        }
    }
    let default_language = required_string(
        obj,
        "default_language",
        "gvya.project.json",
        limits,
        &mut issues,
    )
    .unwrap_or_default();
    if !language_tag_is_well_formed(&default_language)
        || !normalized_enabled_languages.contains(&normalize_locale(&default_language))
    {
        issues.push(issue(
            "source.default_language",
            "default_language must be one of the enabled languages",
            Some("gvya.project.json#default_language"),
        ));
    }
    let language_profile_files = string_array(
        obj.get("language_profiles"),
        "gvya.project.json#language_profiles",
        limits,
        &mut issues,
    );
    if language_profile_files.len() > SOURCE_LANGUAGES_MAX {
        issues.push(issue(
            "source.language_profile_limit",
            "language_profiles may contain at most one file per supported language",
            Some("gvya.project.json#language_profiles"),
        ));
    }
    let matcher_profile_files = string_array(
        obj.get("matcher_profiles"),
        "gvya.project.json#matcher_profiles",
        limits,
        &mut issues,
    );
    if matcher_profile_files.len() > SOURCE_LANGUAGES_MAX {
        issues.push(issue(
            "source.matcher_profile_limit",
            "matcher_profiles may contain at most one file per supported language",
            Some("gvya.project.json#matcher_profiles"),
        ));
    }
    let package_files = string_array(
        obj.get("packages"),
        "gvya.project.json#packages",
        limits,
        &mut issues,
    );
    let fallback_package_file = optional_source_string(
        obj,
        "fallback_package",
        "gvya.project.json",
        limits,
        &mut issues,
    );
    let semantic_config = parse_semantic_config(
        obj.get("semantic"),
        "gvya.project.json#semantic",
        &mut issues,
    );
    let conversation_config = parse_conversation_config(
        obj.get("conversation"),
        "gvya.project.json#conversation",
        &mut issues,
    );
    let emit_debug_map = optional_bool(
        obj,
        "emit_debug_map",
        false,
        "gvya.project.json",
        &mut issues,
    );
    if project_id.trim().is_empty() {
        issues.push(issue(
            "source.project_id",
            "project_id must not be empty",
            Some("gvya.project.json"),
        ));
    }
    if brain_id.trim().is_empty() {
        issues.push(issue(
            "source.brain_id",
            "brain_id must not be empty",
            Some("gvya.project.json"),
        ));
    }
    if package_files.is_empty() && fallback_package_file.is_none() {
        issues.push(issue(
            "source.packages_empty",
            "project must declare at least one Standard or Fallback Package",
            Some("gvya.project.json"),
        ));
    }
    if issues.is_empty() {
        Ok(SourceProject {
            project_id,
            brain_id,
            languages,
            enabled_languages,
            default_language,
            language_profile_files,
            matcher_profile_files,
            package_files,
            fallback_package_file,
            semantic_config,
            conversation_config,
            emit_debug_map,
        })
    } else {
        Err(issues)
    }
}

#[derive(Clone, Debug)]
struct LoadedPackageFragment {
    file: String,
    value: JsonValue,
    canonical: Vec<u8>,
}

type LoadedPackageFragments = BTreeMap<String, Vec<LoadedPackageFragment>>;

fn parse_package(
    tree: &SourceTree,
    package_file: &str,
    root: &JsonValue,
    limits: SourceLimits,
) -> Result<(PackageDefinition, BTreeMap<AssetId, Vec<u8>>, String), Vec<SourceIssue>> {
    let mut issues = Vec::new();
    let Ok(obj) = expect_object(root, package_file, &mut issues) else {
        return Err(issues);
    };
    reject_unknown_keys(obj, contract::PACKAGE_KEYS, package_file, &mut issues);
    require_exact_format_version(
        obj,
        PACKAGE_SOURCE_FORMAT,
        PACKAGE_SOURCE_VERSION,
        package_file,
        &mut issues,
    );
    let manifest_value = obj.get("manifest").unwrap_or(&JsonValue::Null);
    let Ok(manifest_obj) = expect_object(
        manifest_value,
        &format!("{package_file}#manifest"),
        &mut issues,
    ) else {
        return Err(issues);
    };
    reject_unknown_keys(
        manifest_obj,
        contract::PACKAGE_MANIFEST_KEYS,
        &format!("{package_file}#manifest"),
        &mut issues,
    );
    let id =
        required_string(manifest_obj, "id", package_file, limits, &mut issues).unwrap_or_default();
    let kind = match required_string(manifest_obj, "kind", package_file, limits, &mut issues)
        .unwrap_or_default()
        .as_str()
    {
        "standard" => PackageKind::Standard,
        "fallback" => PackageKind::Fallback,
        _ => {
            issues.push(issue(
                "source.package_kind",
                "package manifest kind must be standard or fallback",
                Some(package_file),
            ));
            PackageKind::Standard
        }
    };
    let description = optional_string(
        manifest_obj,
        "description",
        "",
        package_file,
        limits,
        &mut issues,
    );
    let dependencies = parse_dependencies(
        manifest_obj.get("dependencies"),
        package_file,
        limits,
        &mut issues,
    );

    let fragments_obj = match obj.get("fragments") {
        Some(value) => expect_object(value, &format!("{package_file}#fragments"), &mut issues).ok(),
        None => {
            issues.push(issue(
                "source.fragments_missing",
                "package fragments object is required",
                Some(package_file),
            ));
            None
        }
    };
    let fragments = fragments_obj.map_or_else(LoadedPackageFragments::new, |fragment_index| {
        load_package_fragments(tree, package_file, fragment_index, limits, &mut issues)
    });

    let mut assets = BTreeMap::new();
    let contents = parse_fragmented_contents(
        tree,
        package_file,
        &fragments,
        limits,
        &mut assets,
        &mut issues,
    );

    let canonical = canonical_json(root).map_err(|error| {
        vec![issue(
            "source.canonical_json",
            &format!("cannot canonicalize package source: {error:?}"),
            Some(package_file),
        )]
    })?;
    let mut digest_input = b"gvya.source.package/2\0".to_vec();
    append_digest_row(&mut digest_input, package_file.as_bytes(), &canonical);
    for namespace in contract::PACKAGE_CONTENTS_KEYS {
        if let Some(rows) = fragments.get(*namespace) {
            for fragment in rows {
                append_digest_row(
                    &mut digest_input,
                    fragment.file.as_bytes(),
                    &fragment.canonical,
                );
            }
        }
    }
    let mut asset_rows: Vec<_> = assets.iter().collect();
    asset_rows.sort_by(|(left, _), (right, _)| left.as_str().cmp(right.as_str()));
    for (asset_id, bytes) in asset_rows {
        append_digest_row(
            &mut digest_input,
            asset_id.as_str().as_bytes(),
            sha256_hex(bytes).as_bytes(),
        );
    }
    let digest = sha256_hex(&digest_input);
    if id.trim().is_empty() {
        issues.push(issue(
            "source.package_id",
            "package id must not be empty",
            Some(package_file),
        ));
    }
    if !issues.is_empty() {
        return Err(issues);
    }
    let manifest = PackageManifest {
        id: PackageId::new(id),
        digest: PackageDigest::new(digest.clone()),
        kind,
        description,
        dependencies,
    };
    Ok((PackageDefinition { manifest, contents }, assets, digest))
}

fn append_digest_row(out: &mut Vec<u8>, label: &[u8], bytes: &[u8]) {
    out.extend_from_slice(&(u32::try_from(label.len()).unwrap_or(u32::MAX)).to_le_bytes());
    out.extend_from_slice(label);
    out.extend_from_slice(&(u64::try_from(bytes.len()).unwrap_or(u64::MAX)).to_le_bytes());
    out.extend_from_slice(bytes);
}

fn load_package_fragments(
    tree: &SourceTree,
    package_file: &str,
    obj: &serde_json::Map<String, JsonValue>,
    limits: SourceLimits,
    issues: &mut Vec<SourceIssue>,
) -> LoadedPackageFragments {
    reject_unknown_keys(
        obj,
        contract::PACKAGE_FRAGMENTS_KEYS,
        &format!("{package_file}#fragments"),
        issues,
    );
    let mut seen = BTreeSet::new();
    let mut loaded = LoadedPackageFragments::new();
    for namespace in contract::PACKAGE_CONTENTS_KEYS {
        let Some(value) = obj.get(*namespace) else {
            continue;
        };
        let Some(paths) = value.as_array() else {
            issues.push(issue(
                "source.expected_array",
                "fragment namespace must be an array of package-relative source paths",
                Some(&format!("{package_file}#fragments.{namespace}")),
            ));
            continue;
        };
        if paths.len() > limits.max_contributions_per_kind {
            issues.push(issue(
                "source.contribution_limit",
                "fragment namespace exceeds configured contribution limit",
                Some(&format!("{package_file}#fragments.{namespace}")),
            ));
            continue;
        }
        let mut rows = Vec::new();
        for (index, value) in paths.iter().enumerate() {
            let index_path = format!("{package_file}#fragments.{namespace}[{index}]");
            let Some(relative) = value.as_str() else {
                issues.push(issue(
                    "source.fragment_path",
                    "fragment entry must be a string path",
                    Some(&index_path),
                ));
                continue;
            };
            if !relative.starts_with("fragments/") || !relative.ends_with(".json") {
                issues.push(issue(
                    "source.fragment_path",
                    "fragment path must be package-local under fragments/ and end in .json",
                    Some(&index_path),
                ));
                continue;
            }
            let Some(fragment_file) = join_relative(package_file, relative) else {
                issues.push(issue(
                    "source.fragment_path",
                    "fragment path is unsafe",
                    Some(&index_path),
                ));
                continue;
            };
            if !seen.insert(fragment_file.clone()) {
                issues.push(issue(
                    "source.fragment_duplicate_path",
                    "the same fragment file may be declared only once in a Package",
                    Some(&index_path),
                ));
                continue;
            }
            let value = match parse_json_file(tree, &fragment_file, limits) {
                Ok(value) => value,
                Err(mut rows) => {
                    issues.append(&mut rows);
                    continue;
                }
            };
            let canonical = match canonical_json(&value) {
                Ok(bytes) => bytes,
                Err(error) => {
                    issues.push(issue(
                        "source.canonical_json",
                        &format!("cannot canonicalize package fragment: {error:?}"),
                        Some(&fragment_file),
                    ));
                    continue;
                }
            };
            rows.push(LoadedPackageFragment {
                file: fragment_file,
                value,
                canonical,
            });
        }
        loaded.insert((*namespace).to_owned(), rows);
    }

    let package_dir = package_file.rsplit_once('/').map_or("", |(dir, _)| dir);
    let fragment_prefix = if package_dir.is_empty() {
        "fragments/".to_owned()
    } else {
        format!("{package_dir}/fragments/")
    };
    for path in tree.files.keys() {
        if path.starts_with(&fragment_prefix) && path.ends_with(".json") && !seen.contains(path) {
            issues.push(issue(
                "source.fragment_undeclared",
                "package fragment JSON exists under fragments/ but is not declared in package.json",
                Some(path),
            ));
        }
    }
    loaded
}

fn parse_fragmented_contents(
    tree: &SourceTree,
    package_file: &str,
    fragments: &LoadedPackageFragments,
    limits: SourceLimits,
    assets: &mut BTreeMap<AssetId, Vec<u8>>,
    issues: &mut Vec<SourceIssue>,
) -> PackageContents {
    PackageContents {
        meanings: decode_fragments(fragments, "meanings", limits, issues, parse_meaning),
        behaviors: decode_fragments(fragments, "behaviors", limits, issues, parse_behavior),
        capability_result_behaviors: decode_fragments(
            fragments,
            "capability_result_behaviors",
            limits,
            issues,
            parse_capability_result_behavior,
        ),
        openings: decode_fragments(fragments, "openings", limits, issues, parse_opening),
        fallback_behaviors: decode_fragments(
            fragments,
            "fallback_behaviors",
            limits,
            issues,
            parse_fallback_behavior,
        ),
        style_lexicons: decode_fragments(
            fragments,
            "style_lexicons",
            limits,
            issues,
            parse_style_patch,
        ),
        capabilities: decode_fragments(fragments, "capabilities", limits, issues, parse_capability),
        capability_bindings: decode_fragments(
            fragments,
            "capability_bindings",
            limits,
            issues,
            parse_capability_binding,
        ),
        capability_policies: decode_fragments(
            fragments,
            "capability_policies",
            limits,
            issues,
            parse_capability_policy,
        ),
        capability_configs: decode_fragments(
            fragments,
            "capability_configs",
            limits,
            issues,
            parse_capability_config,
        ),
        types: decode_fragments(fragments, "types", limits, issues, parse_named_type),
        assets: fragments
            .get("assets")
            .into_iter()
            .flatten()
            .filter_map(|fragment| {
                asset_contribution_from_fragment(
                    tree,
                    package_file,
                    &fragment.file,
                    &fragment.value,
                    limits,
                    assets,
                    issues,
                )
            })
            .collect(),
        regression_cases: decode_fragments(
            fragments,
            "regression_cases",
            limits,
            issues,
            parse_regression_case,
        ),
        scenarios: decode_fragments(fragments, "scenarios", limits, issues, parse_scenario),
    }
}

fn decode_fragments<T>(
    fragments: &LoadedPackageFragments,
    namespace: &str,
    limits: SourceLimits,
    issues: &mut Vec<SourceIssue>,
    decoder: Decoder<T>,
) -> Vec<PackageContribution<T>> {
    fragments
        .get(namespace)
        .into_iter()
        .flatten()
        .filter_map(|fragment| {
            contribution_from_fragment(&fragment.value, &fragment.file, limits, issues, decoder)
        })
        .collect()
}

pub fn safe_source_path(path: &str) -> bool {
    !path.is_empty()
        && !path.starts_with('/')
        && !path.ends_with('/')
        && !path.contains('\\')
        && !path.contains('\0')
        && !path
            .split('/')
            .any(|s| s.is_empty() || s == "." || s == "..")
        && !path.bytes().any(|b| b < 0x20 || b == 0x7f)
}
#[must_use]
pub fn safe_asset_logical_path(path: &str) -> bool {
    safe_source_path(path) && path.starts_with("assets/")
}
fn reject_unknown_keys(
    obj: &serde_json::Map<String, JsonValue>,
    allowed: &[&str],
    path: &str,
    issues: &mut Vec<SourceIssue>,
) {
    for key in obj.keys() {
        if !allowed.contains(&key.as_str()) {
            issues.push(issue(
                "source.unknown_key",
                &format!("unknown source key: {key}"),
                Some(&format!("{path}.{key}")),
            ));
        }
    }
}
fn issue(code: &str, message: &str, path: Option<&str>) -> SourceIssue {
    SourceIssue {
        code: code.to_owned(),
        path: path.map(str::to_owned),
        message: message.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn expand_test_package_specs(
        mut files: BTreeMap<String, Vec<u8>>,
    ) -> BTreeMap<String, Vec<u8>> {
        let package_paths: Vec<String> = files
            .keys()
            .filter(|path| path.ends_with("package.json"))
            .cloned()
            .collect();
        let mut additions = Vec::new();
        for package_path in package_paths {
            let Some(bytes) = files.get(&package_path).cloned() else {
                continue;
            };
            let Ok(mut root) = serde_json::from_slice::<JsonValue>(&bytes) else {
                continue;
            };
            if root.get("format").and_then(JsonValue::as_str) != Some("gvya.test.package-spec") {
                continue;
            }
            let Some(contents) = root
                .get_mut("contents")
                .and_then(JsonValue::as_object_mut)
                .map(std::mem::take)
            else {
                continue;
            };
            let mut fragment_index = serde_json::Map::new();
            let dir = package_path.rsplit_once('/').map_or("", |(dir, _)| dir);
            for (namespace, value) in contents {
                if let Some(rows) = value.as_array() {
                    let mut paths = Vec::new();
                    for (index, row) in rows.iter().enumerate() {
                        let relative = format!("fragments/{namespace}/{:04}.json", index + 1);
                        let full = if dir.is_empty() {
                            relative.clone()
                        } else {
                            format!("{dir}/{relative}")
                        };
                        additions.push((full, serde_json::to_vec(row).unwrap()));
                        paths.push(JsonValue::String(relative));
                    }
                    fragment_index.insert(namespace, JsonValue::Array(paths));
                } else {
                    fragment_index.insert(namespace, value);
                }
            }
            let obj = root.as_object_mut().unwrap();
            obj.remove("contents");
            obj.insert(
                "format".into(),
                JsonValue::String(PACKAGE_SOURCE_FORMAT.into()),
            );
            obj.insert("version".into(), JsonValue::from(PACKAGE_SOURCE_VERSION));
            obj.insert("fragments".into(), JsonValue::Object(fragment_index));
            files.insert(package_path, serde_json::to_vec(&root).unwrap());
        }
        files.extend(additions);
        files
    }

    fn test_source_tree(
        files: BTreeMap<String, Vec<u8>>,
        limits: SourceLimits,
    ) -> Result<SourceTree, Vec<SourceIssue>> {
        SourceTree::new(expand_test_package_specs(files), limits)
    }

    fn direct_fragment_tree(namespace: &str, fragment_name: &str, fragment: &[u8]) -> SourceTree {
        let relative = format!("fragments/{namespace}/{fragment_name}.json");
        let full = format!("packages/base/{relative}");
        let package = serde_json::json!({
            "format": PACKAGE_SOURCE_FORMAT,
            "version": PACKAGE_SOURCE_VERSION,
            "manifest": {"id":"base","kind":"standard","dependencies":[]},
            "fragments": {namespace: [relative]},
        });
        SourceTree::new(
            BTreeMap::from([
                (
                    "gvya.project.json".to_owned(),
                    br#"{"format":"gvya.source.project","version":1,"project_id":"demo","brain_id":"assistant","languages":["en"],"enabled_languages":["en"],"default_language":"en","packages":["packages/base/package.json"]}"#.to_vec(),
                ),
                ("packages/base/package.json".to_owned(), serde_json::to_vec(&package).unwrap()),
                (full, fragment.to_vec()),
            ]),
            SourceLimits::default(),
        )
        .unwrap()
    }

    #[test]
    fn source_tree_fingerprint_is_order_independent_but_binds_path_and_bytes() {
        let limits = SourceLimits::default();
        let mut left = BTreeMap::new();
        left.insert("b.json".to_owned(), b"two".to_vec());
        left.insert("a.json".to_owned(), b"one".to_vec());
        let mut same = BTreeMap::new();
        same.insert("a.json".to_owned(), b"one".to_vec());
        same.insert("b.json".to_owned(), b"two".to_vec());
        let mut changed_bytes = same.clone();
        changed_bytes.insert("b.json".to_owned(), b"TWO".to_vec());
        let mut changed_path = BTreeMap::new();
        changed_path.insert("a.json".to_owned(), b"one".to_vec());
        changed_path.insert("c.json".to_owned(), b"two".to_vec());

        let left = SourceTree::new(left, limits).unwrap();
        let same = SourceTree::new(same, limits).unwrap();
        let changed_bytes = SourceTree::new(changed_bytes, limits).unwrap();
        let changed_path = SourceTree::new(changed_path, limits).unwrap();
        assert_eq!(left.fingerprint_sha256(), same.fingerprint_sha256());
        assert_ne!(
            left.fingerprint_sha256(),
            changed_bytes.fingerprint_sha256()
        );
        assert_ne!(left.fingerprint_sha256(), changed_path.fingerprint_sha256());
        assert_eq!(left.fingerprint_sha256().len(), 64);
    }

    #[test]
    fn source_paths_reject_traversal_and_backslash() {
        assert!(safe_source_path("packages/base/package.json"));
        assert!(!safe_source_path("../package.json"));
        assert!(!safe_source_path("packages\\base.json"));
    }

    #[test]
    fn obsolete_inline_package_shape_is_rejected_cleanly() {
        let tree = SourceTree::new(
            BTreeMap::from([
                (
                    "gvya.project.json".to_owned(),
                    br#"{"format":"gvya.source.project","version":1,"project_id":"demo","brain_id":"assistant","languages":["en"],"enabled_languages":["en"],"default_language":"en","packages":["packages/base/package.json"]}"#.to_vec(),
                ),
                (
                    "packages/base/package.json".to_owned(),
                    br#"{"format":"gvya.source.package","version":1,"manifest":{"id":"base","kind":"standard","dependencies":[]},"contents":{}}"#.to_vec(),
                ),
            ]),
            SourceLimits::default(),
        )
        .unwrap();
        let issues = resolve_source_project(&tree, SourceLimits::default()).unwrap_err();
        assert!(issues.iter().any(|row| row.code == "source.unknown_key"));
    }

    #[test]
    fn minimal_source_resolves_and_derives_package_digest() {
        let project = br#"{"format":"gvya.source.project","version":1,"project_id":"demo","brain_id":"assistant","languages":["en-US"],"enabled_languages":["en-US"],"default_language":"en-US","packages":["packages/base/package.json"]}"#.to_vec();
        let package = br#"{"format":"gvya.test.package-spec","manifest":{"id":"base","kind":"standard","dependencies":[]},"contents":{"meanings":[{"id":"hello","value":{"id":"hello","samples":[{"language":"en-US","text":"hello"}]}}]}}"#.to_vec();
        let tree = test_source_tree(
            BTreeMap::from([
                ("gvya.project.json".into(), project),
                ("packages/base/package.json".into(), package),
            ]),
            SourceLimits::default(),
        )
        .unwrap();
        let resolved = resolve_source_project(&tree, SourceLimits::default()).unwrap();
        assert_eq!(resolved.packages.len(), 1);
        assert_eq!(resolved.packages[0].manifest.digest.as_str().len(), 64);
    }

    #[test]
    fn project_requires_an_explicit_default_language_from_enabled_languages() {
        let missing = br#"{"format":"gvya.source.project","version":1,"project_id":"demo","brain_id":"assistant","languages":["en"],"enabled_languages":["en"],"packages":[]}"#.to_vec();
        let missing_tree = test_source_tree(
            BTreeMap::from([("gvya.project.json".into(), missing)]),
            SourceLimits::default(),
        )
        .unwrap();
        let missing_issues =
            resolve_source_project(&missing_tree, SourceLimits::default()).unwrap_err();
        assert!(
            missing_issues
                .iter()
                .any(|row| row.code == "source.string_required")
        );

        let outside_catalog = br#"{"format":"gvya.source.project","version":1,"project_id":"demo","brain_id":"assistant","languages":["en","fa"],"enabled_languages":["en"],"default_language":"fa","packages":[]}"#.to_vec();
        let outside_catalog_tree = test_source_tree(
            BTreeMap::from([("gvya.project.json".into(), outside_catalog)]),
            SourceLimits::default(),
        )
        .unwrap();
        let outside_catalog_issues =
            resolve_source_project(&outside_catalog_tree, SourceLimits::default()).unwrap_err();
        assert!(
            outside_catalog_issues
                .iter()
                .any(|row| row.code == "source.default_language")
        );
    }

    #[test]
    fn enabled_languages_are_a_non_empty_project_language_subset() {
        for (project, expected_code) in [
            (
                br#"{"format":"gvya.source.project","version":1,"project_id":"demo","brain_id":"assistant","languages":["en"],"enabled_languages":[],"default_language":"en","packages":[]}"#.as_slice(),
                "source.enabled_languages",
            ),
            (
                br#"{"format":"gvya.source.project","version":1,"project_id":"demo","brain_id":"assistant","languages":["en"],"enabled_languages":["fa"],"default_language":"fa","packages":[]}"#.as_slice(),
                "source.enabled_language_not_declared",
            ),
        ] {
            let tree = test_source_tree(
                BTreeMap::from([("gvya.project.json".into(), project.to_vec())]),
                SourceLimits::default(),
            )
            .unwrap();
            let issues = resolve_source_project(&tree, SourceLimits::default()).unwrap_err();
            assert!(issues.iter().any(|row| row.code == expected_code));
        }
    }

    #[test]
    fn fallback_package_is_a_separate_source_slot_with_first_class_conditions() {
        let project = br#"{"format":"gvya.source.project","version":1,"project_id":"demo","brain_id":"assistant","languages":["en-US"],"enabled_languages":["en-US"],"default_language":"en-US","packages":[],"fallback_package":"packages/fallback/package.json"}"#.to_vec();
        let package = br#"{"format":"gvya.test.package-spec","manifest":{"id":"fallback","kind":"fallback","dependencies":[]},"contents":{"fallback_behaviors":[{"id":"angry.unresolved","exported":false,"mode":"add","value":{"id":"angry.unresolved","trigger":"unresolved","priority":100,"conditions":[{"namespace":"author","path":"mood.anger","op":"greater_or_equal","value":70}],"responses":[]}}]}}"#.to_vec();
        let tree = test_source_tree(
            BTreeMap::from([
                ("gvya.project.json".into(), project),
                ("packages/fallback/package.json".into(), package),
            ]),
            SourceLimits::default(),
        )
        .unwrap();
        let resolved = resolve_source_project(&tree, SourceLimits::default()).unwrap();
        assert_eq!(resolved.packages.len(), 1);
        assert_eq!(resolved.packages[0].manifest.kind, PackageKind::Fallback);
        let fallback = &resolved.packages[0].contents.fallback_behaviors[0].value;
        assert_eq!(fallback.priority, 100);
        assert_eq!(fallback.trigger, FallbackTrigger::Unresolved);
        assert_eq!(
            fallback.conditions[0].path.namespace,
            StateNamespace::Author
        );
        assert_eq!(fallback.conditions[0].path.path, "mood.anger");
    }

    #[test]
    fn project_slots_reject_package_kind_mismatch() {
        let project = br#"{"format":"gvya.source.project","version":1,"project_id":"demo","brain_id":"assistant","languages":["en-US"],"enabled_languages":["en-US"],"default_language":"en-US","packages":["packages/wrong/package.json"]}"#.to_vec();
        let package = br#"{"format":"gvya.test.package-spec","manifest":{"id":"wrong","kind":"fallback","dependencies":[]},"contents":{}}"#.to_vec();
        let tree = test_source_tree(
            BTreeMap::from([
                ("gvya.project.json".into(), project),
                ("packages/wrong/package.json".into(), package),
            ]),
            SourceLimits::default(),
        )
        .unwrap();
        let issues = resolve_source_project(&tree, SourceLimits::default()).unwrap_err();
        assert!(
            issues
                .iter()
                .any(|row| row.code == "source.package_kind_slot")
        );
    }

    #[test]
    fn behavior_value_requirements_are_first_class_source_fields() {
        let project = br#"{"format":"gvya.source.project","version":1,"project_id":"demo","brain_id":"assistant","languages":["en-US"],"enabled_languages":["en-US"],"default_language":"en-US","packages":["packages/base/package.json"]}"#.to_vec();
        let package = br#"{"format":"gvya.test.package-spec","manifest":{"id":"base","kind":"standard","dependencies":[]},"contents":{"meanings":[{"id":"hello","value":{"id":"hello","samples":[{"language":"en-US","text":"hello"}]}}],"behaviors":[{"id":"hello.behavior","value":{"id":"hello.behavior","meaning":"hello","requires_values":[{"namespace":"context","path":"mode","value":"ready"}],"forbidden_values":[{"namespace":"author","path":"blocked","value":true}],"responses":[]}}]}}"#.to_vec();
        let tree = test_source_tree(
            BTreeMap::from([
                ("gvya.project.json".into(), project),
                ("packages/base/package.json".into(), package),
            ]),
            SourceLimits::default(),
        )
        .unwrap();
        let resolved = resolve_source_project(&tree, SourceLimits::default()).unwrap();
        let behavior = &resolved.packages[0].contents.behaviors[0].value;
        assert_eq!(behavior.requires_values.len(), 1);
        assert_eq!(behavior.forbidden_values.len(), 1);
        assert_eq!(behavior.requires_values[0].path.path, "mode");
        assert_eq!(behavior.forbidden_values[0].path.path, "blocked");
    }

    #[test]
    fn unknown_compiler_owned_source_key_fails_closed() {
        let project = br#"{"format":"gvya.source.project","version":1,"project_id":"demo","brain_id":"assistant","languages":["en-US"],"enabled_languages":["en-US"],"default_language":"en-US","packages":["packages/base/package.json"]}"#.to_vec();
        let package = br#"{"format":"gvya.test.package-spec","manifest":{"id":"base","kind":"standard","dependencies":[]},"contents":{"meanings":[{"id":"hello","value":{"id":"hello","samples":[{"language":"en-US","text":"hello"}],"retrival_terms":["typo must not disappear"]}}]}}"#.to_vec();
        let tree = test_source_tree(
            BTreeMap::from([
                ("gvya.project.json".into(), project),
                ("packages/base/package.json".into(), package),
            ]),
            SourceLimits::default(),
        )
        .unwrap();
        let issues = resolve_source_project(&tree, SourceLimits::default()).unwrap_err();
        assert!(issues.iter().any(|row| row.code == "source.unknown_key"));
    }

    #[test]
    fn missing_required_capability_contract_does_not_drop_silently() {
        let project = br#"{"format":"gvya.source.project","version":1,"project_id":"demo","brain_id":"assistant","languages":["en-US"],"enabled_languages":["en-US"],"default_language":"en-US","packages":["packages/base/package.json"]}"#.to_vec();
        let package = br#"{"format":"gvya.test.package-spec","manifest":{"id":"base","kind":"standard","dependencies":[]},"contents":{"capabilities":[{"id":"door.open","value":{"host_effects":[]}}]}}"#.to_vec();
        let tree = test_source_tree(
            BTreeMap::from([
                ("gvya.project.json".into(), project),
                ("packages/base/package.json".into(), package),
            ]),
            SourceLimits::default(),
        )
        .unwrap();
        let issues = resolve_source_project(&tree, SourceLimits::default()).unwrap_err();
        assert!(issues.iter().any(|row| row.code == "source.field_required"));
    }

    #[test]
    fn malformed_regression_expectation_cannot_weaken_test_silently() {
        let project = br#"{"format":"gvya.source.project","version":1,"project_id":"demo","brain_id":"assistant","languages":["en-US"],"enabled_languages":["en-US"],"default_language":"en-US","packages":["packages/base/package.json"]}"#.to_vec();
        let package = br#"{"format":"gvya.test.package-spec","manifest":{"id":"base","kind":"standard","dependencies":[]},"contents":{"regression_cases":[{"id":"case.one","value":{"id":"case.one","input":"open it","expectation":{"capabilities":[{"version":"1"}]}}}]}}"#.to_vec();
        let tree = test_source_tree(
            BTreeMap::from([
                ("gvya.project.json".into(), project),
                ("packages/base/package.json".into(), package),
            ]),
            SourceLimits::default(),
        )
        .unwrap();
        let issues = resolve_source_project(&tree, SourceLimits::default()).unwrap_err();
        assert!(issues.iter().any(|row| row.code == "source.field_required"));
    }

    #[test]
    fn language_and_matcher_profiles_compose_distinct_drop_in_data() {
        let project = br#"{"format":"gvya.source.project","version":1,"project_id":"demo","brain_id":"assistant","languages":["en-US"],"enabled_languages":["en-US"],"default_language":"en-US","language_profiles":["language-profiles/en-us.json"],"matcher_profiles":["matcher-profiles/en-us.json"],"packages":["packages/base/package.json"]}"#.to_vec();
        let package = br#"{"format":"gvya.test.package-spec","manifest":{"id":"base","kind":"standard","dependencies":[]},"contents":{}}"#.to_vec();
        let language_profile = br#"{"format":"gvya.source.language-profile","version":1,"language":"en-US","profile":{"canonical_tokens":{"dogs":"dog"},"canonical_suffixes":{"ies":"y"},"detached_suffixes":[],"colloquial":{"would you please":["please"]}}}"#.to_vec();
        let matcher_profile = br#"{"format":"gvya.source.matcher-profile","version":1,"language":"en-US","profile":{"pattern_sets":{}}}"#.to_vec();
        let tree = test_source_tree(
            BTreeMap::from([
                ("gvya.project.json".into(), project),
                ("packages/base/package.json".into(), package),
                ("language-profiles/en-us.json".into(), language_profile),
                ("matcher-profiles/en-us.json".into(), matcher_profile),
            ]),
            SourceLimits::default(),
        )
        .unwrap();
        let resolved = resolve_source_project(&tree, SourceLimits::default()).unwrap();
        assert_eq!(
            resolved
                .semantic_profiles
                .get("en-us")
                .unwrap()
                .canonical_tokens
                .get("dogs"),
            Some(&"dog".to_owned())
        );
        assert_eq!(
            resolved
                .semantic_profiles
                .get("en-us")
                .unwrap()
                .colloquial
                .get("would you please"),
            Some(&vec!["please".to_owned()])
        );
        assert_eq!(resolved.source_language_profile_digests.len(), 1);
        assert_eq!(resolved.source_matcher_profile_digests.len(), 1);
    }

    #[test]
    fn matcher_profile_rejects_language_mechanics() {
        let project = br#"{"format":"gvya.source.project","version":1,"project_id":"demo","brain_id":"assistant","languages":["en-US"],"enabled_languages":["en-US"],"default_language":"en-US","language_profiles":["language-profiles/en-us.json"],"matcher_profiles":["matcher-profiles/en-us.json"],"packages":["packages/base/package.json"]}"#.to_vec();
        let package = br#"{"format":"gvya.test.package-spec","manifest":{"id":"base","kind":"standard","dependencies":[]},"contents":{}}"#.to_vec();
        let language_profile = br#"{"format":"gvya.source.language-profile","version":1,"language":"en-US","profile":{}}"#.to_vec();
        let matcher_profile = br#"{"format":"gvya.source.matcher-profile","version":1,"language":"en-US","profile":{"canonical_tokens":{"dogs":"dog"}}}"#.to_vec();
        let tree = test_source_tree(
            BTreeMap::from([
                ("gvya.project.json".into(), project),
                ("packages/base/package.json".into(), package),
                ("language-profiles/en-us.json".into(), language_profile),
                ("matcher-profiles/en-us.json".into(), matcher_profile),
            ]),
            SourceLimits::default(),
        )
        .unwrap();
        let issues = resolve_source_project(&tree, SourceLimits::default()).unwrap_err();
        assert!(issues.iter().any(|row| row.code == "source.unknown_key"));
    }

    #[test]
    fn matcher_profile_may_live_in_a_declared_nested_project_directory() {
        let project = br#"{"format":"gvya.source.project","version":1,"project_id":"demo","brain_id":"assistant","languages":["en-US"],"enabled_languages":["en-US"],"default_language":"en-US","language_profiles":["projects/demo/language-profiles/en-us.json"],"matcher_profiles":["projects/demo/matcher-profiles/en-us.json"],"packages":["packages/base/package.json"]}"#.to_vec();
        let package = br#"{"format":"gvya.test.package-spec","manifest":{"id":"base","kind":"standard","dependencies":[]},"contents":{}}"#.to_vec();
        let language_profile = br#"{"format":"gvya.source.language-profile","version":1,"language":"en-US","profile":{}}"#.to_vec();
        let matcher_profile = br#"{"format":"gvya.source.matcher-profile","version":1,"language":"en-US","profile":{}}"#.to_vec();
        let tree = test_source_tree(
            BTreeMap::from([
                ("gvya.project.json".into(), project),
                ("packages/base/package.json".into(), package),
                (
                    "projects/demo/language-profiles/en-us.json".into(),
                    language_profile,
                ),
                (
                    "projects/demo/matcher-profiles/en-us.json".into(),
                    matcher_profile,
                ),
            ]),
            SourceLimits::default(),
        )
        .unwrap();
        let resolved = resolve_source_project(&tree, SourceLimits::default()).unwrap();
        assert_eq!(resolved.source_language_profile_digests.len(), 1);
        assert_eq!(resolved.source_matcher_profile_digests.len(), 1);
    }

    #[test]
    fn source_resolution_keeps_pattern_sets_isolated_by_language() {
        let project = br#"{"format":"gvya.source.project","version":1,"project_id":"demo","brain_id":"assistant","languages":["en-US","fa-IR"],"enabled_languages":["en-US","fa-IR"],"default_language":"en-US","language_profiles":["language-profiles/en-us.json","language-profiles/fa-ir.json"],"matcher_profiles":["matcher-profiles/en-us.json","matcher-profiles/fa-ir.json"],"packages":["packages/base/package.json"]}"#.to_vec();
        let package = br#"{"format":"gvya.test.package-spec","manifest":{"id":"base","kind":"standard","dependencies":[]},"contents":{}}"#.to_vec();
        let en_language = br#"{"format":"gvya.source.language-profile","version":1,"language":"en-US","profile":{}}"#.to_vec();
        let fa_language = r#"{"format":"gvya.source.language-profile","version":1,"language":"fa-IR","profile":{}}"#.as_bytes().to_vec();
        let en = br#"{"format":"gvya.source.matcher-profile","version":1,"language":"en-US","profile":{"pattern_sets":{"devices":{"bedroom light":"device.bedroom"}}}}"#.to_vec();
        let fa = r#"{"format":"gvya.source.matcher-profile","version":1,"language":"fa-IR","profile":{"pattern_sets":{"devices":{"چراغ اتاق":"device.bedroom"}}}}"#
            .as_bytes()
            .to_vec();
        let tree = test_source_tree(
            BTreeMap::from([
                ("gvya.project.json".into(), project),
                ("packages/base/package.json".into(), package),
                ("language-profiles/en-us.json".into(), en_language),
                ("language-profiles/fa-ir.json".into(), fa_language),
                ("matcher-profiles/en-us.json".into(), en),
                ("matcher-profiles/fa-ir.json".into(), fa),
            ]),
            SourceLimits::default(),
        )
        .unwrap();
        let resolved = resolve_source_project(&tree, SourceLimits::default()).unwrap();
        assert_eq!(
            resolved
                .semantic_profiles
                .get("en-us")
                .unwrap()
                .pattern_sets
                .get("devices"),
            Some(&BTreeMap::from([(
                "bedroom light".into(),
                "device.bedroom".into(),
            )]))
        );
        assert_eq!(
            resolved
                .semantic_profiles
                .get("fa-ir")
                .unwrap()
                .pattern_sets
                .get("devices"),
            Some(&BTreeMap::from([(
                "چراغ اتاق".into(),
                "device.bedroom".into(),
            )]))
        );
    }

    #[test]
    fn matcher_profile_pattern_sets_merge_into_same_language_pair() {
        let mut target = SemanticProfile::empty();
        target.pattern_sets.insert(
            "devices".into(),
            BTreeMap::from([("light".into(), "device.light".into())]),
        );
        let mut incoming = SemanticProfile::empty();
        incoming.pattern_sets.insert(
            "devices".into(),
            BTreeMap::from([("lamp".into(), "device.light".into())]),
        );
        let mut issues = Vec::new();
        merge_semantic_profile(
            &mut target,
            &incoming,
            "matcher-profiles/en-us.json",
            &mut issues,
        );
        assert!(issues.is_empty(), "unexpected issues: {issues:?}");
        assert_eq!(
            target.pattern_sets.get("devices"),
            Some(&BTreeMap::from([
                ("light".into(), "device.light".into()),
                ("lamp".into(), "device.light".into()),
            ]))
        );
    }

    #[test]
    fn matcher_profile_pattern_set_alias_conflict_fails_closed() {
        let mut target = SemanticProfile::empty();
        target.pattern_sets.insert(
            "devices".into(),
            BTreeMap::from([("light".into(), "device.light".into())]),
        );
        let mut incoming = SemanticProfile::empty();
        incoming.pattern_sets.insert(
            "devices".into(),
            BTreeMap::from([("light".into(), "device.other".into())]),
        );
        let mut issues = Vec::new();
        merge_semantic_profile(
            &mut target,
            &incoming,
            "matcher-profiles/en-gb.json",
            &mut issues,
        );
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, "source.profile_conflict");
        assert_eq!(target.pattern_sets["devices"]["light"], "device.light");
    }

    #[test]
    fn matcher_profiles_are_selected_only_by_enabled_bot_languages() {
        let project = br#"{"format":"gvya.source.project","version":1,"project_id":"demo","brain_id":"assistant","languages":["en-US","fa-IR"],"enabled_languages":["en-US"],"default_language":"en-US","language_profiles":["language-profiles/fa-ir.json"],"matcher_profiles":["matcher-profiles/fa-ir.json"],"packages":["packages/base/package.json"]}"#.to_vec();
        let package = br#"{"format":"gvya.test.package-spec","manifest":{"id":"base","kind":"standard","dependencies":[]},"contents":{}}"#.to_vec();
        let language_profile = r#"{"format":"gvya.source.language-profile","version":1,"language":"fa-IR","profile":{"canonical_tokens":{"سگ‌ها":"سگ"}}}"#
            .as_bytes()
            .to_vec();
        let matcher_profile = r#"{"format":"gvya.source.matcher-profile","version":1,"language":"fa-IR","profile":{}}"#
            .as_bytes()
            .to_vec();
        let tree = test_source_tree(
            BTreeMap::from([
                ("gvya.project.json".into(), project),
                ("packages/base/package.json".into(), package),
                ("language-profiles/fa-ir.json".into(), language_profile),
                ("matcher-profiles/fa-ir.json".into(), matcher_profile),
            ]),
            SourceLimits::default(),
        )
        .unwrap();
        let issues = resolve_source_project(&tree, SourceLimits::default()).unwrap_err();
        assert!(
            issues
                .iter()
                .any(|row| row.code == "source.language_profile_not_enabled"),
            "unexpected issues: {issues:?}"
        );
    }

    #[test]
    fn package_cannot_hide_a_legacy_semantic_profile_namespace() {
        let project = br#"{"format":"gvya.source.project","version":1,"project_id":"demo","brain_id":"assistant","languages":["en-US"],"enabled_languages":["en-US"],"default_language":"en-US","packages":["packages/base/package.json"]}"#.to_vec();
        let package = br#"{"format":"gvya.test.package-spec","manifest":{"id":"base","kind":"standard","dependencies":[]},"contents":{"semantic_profiles":[]}}"#.to_vec();
        let tree = test_source_tree(
            BTreeMap::from([
                ("gvya.project.json".into(), project),
                ("packages/base/package.json".into(), package),
            ]),
            SourceLimits::default(),
        )
        .unwrap();
        let issues = resolve_source_project(&tree, SourceLimits::default()).unwrap_err();
        assert!(issues.iter().any(|row| {
            row.code == "source.unknown_key"
                && row.path.as_deref()
                    == Some("packages/base/package.json#fragments.semantic_profiles")
        }));
    }

    #[test]
    fn semantic_config_rejects_out_of_range_values() {
        let project = br#"{"format":"gvya.source.project","version":1,"project_id":"demo","brain_id":"assistant","languages":["en-US"],"enabled_languages":["en-US"],"default_language":"en-US","packages":["packages/base/package.json"],"semantic":{"candidate_limit":1,"resolution_threshold":-0.1,"ambiguity_margin":1.1,"resolver_min_confidence":2.0,"resolver_candidate_limit":65}}"#.to_vec();
        let package = br#"{"format":"gvya.test.package-spec","manifest":{"id":"base","kind":"standard","dependencies":[]},"contents":{}}"#.to_vec();
        let tree = test_source_tree(
            BTreeMap::from([
                ("gvya.project.json".into(), project),
                ("packages/base/package.json".into(), package),
            ]),
            SourceLimits::default(),
        )
        .unwrap();
        let issues = resolve_source_project(&tree, SourceLimits::default()).unwrap_err();
        assert!(
            issues
                .iter()
                .filter(|row| row.code == "source.range")
                .count()
                >= 5
        );
    }

    #[test]
    fn source_rejects_per_meaning_matcher_work_budget_overflow() {
        let project = br#"{"format":"gvya.source.project","version":1,"project_id":"demo","brain_id":"assistant","languages":["en-US"],"enabled_languages":["en-US"],"default_language":"en-US","packages":["packages/base/package.json"]}"#.to_vec();
        let samples = (0..=SEMANTIC_SAMPLES_PER_MEANING_MAX)
            .map(|index| serde_json::json!({"language":"en-US","text":format!("sample {index}")}))
            .collect::<Vec<_>>();
        let package = serde_json::json!({
            "format":"gvya.test.package-spec",
            "manifest":{"id":"base","kind":"standard","dependencies":[]},
            "contents":{"meanings":[{"id":"overflow","value":{"id":"overflow","samples":samples}}]}
        })
        .to_string()
        .into_bytes();
        let tree = test_source_tree(
            BTreeMap::from([
                ("gvya.project.json".into(), project),
                ("packages/base/package.json".into(), package),
            ]),
            SourceLimits::default(),
        )
        .unwrap();
        let issues = resolve_source_project(&tree, SourceLimits::default()).unwrap_err();
        assert!(
            issues
                .iter()
                .any(|row| row.code == "source.semantic_work_budget")
        );
    }

    #[test]
    fn capability_config_zero_schema_error_budget_is_rejected_at_source_boundary() {
        let project = br#"{"format":"gvya.source.project","version":1,"project_id":"demo","brain_id":"assistant","languages":["en-US"],"enabled_languages":["en-US"],"default_language":"en-US","packages":["packages/base/package.json"]}"#.to_vec();
        let package = br#"{"format":"gvya.test.package-spec","manifest":{"id":"base","kind":"standard","dependencies":[]},"contents":{"capability_configs":[{"id":"default","value":{"schema_limits":{"max_errors":0}}}]}}"#.to_vec();
        let tree = test_source_tree(
            BTreeMap::from([
                ("gvya.project.json".into(), project),
                ("packages/base/package.json".into(), package),
            ]),
            SourceLimits::default(),
        )
        .unwrap();
        let issues = resolve_source_project(&tree, SourceLimits::default()).unwrap_err();
        assert!(
            issues
                .iter()
                .any(|row| row.code == "source.capability_config_range")
        );
    }

    #[test]
    fn malformed_scenario_step_key_fails_closed() {
        let project = br#"{"format":"gvya.source.project","version":1,"project_id":"demo","brain_id":"assistant","languages":["en-US"],"enabled_languages":["en-US"],"default_language":"en-US","packages":["packages/base/package.json"]}"#.to_vec();
        let package = br#"{"format":"gvya.test.package-spec","manifest":{"id":"base","kind":"standard","dependencies":[]},"contents":{"scenarios":[{"id":"scenario.one","value":{"id":"scenario.one","steps":[{"type":"turn","sya":"hello"}]}}]}}"#.to_vec();
        let tree = test_source_tree(
            BTreeMap::from([
                ("gvya.project.json".into(), project),
                ("packages/base/package.json".into(), package),
            ]),
            SourceLimits::default(),
        )
        .unwrap();
        let issues = resolve_source_project(&tree, SourceLimits::default()).unwrap_err();
        assert!(issues.iter().any(|row| row.code == "source.unknown_key"));
        assert!(
            issues
                .iter()
                .any(|row| row.code == "source.string_required")
        );
    }

    #[test]
    fn language_bearing_content_must_use_the_project_catalog() {
        let project = br#"{"format":"gvya.source.project","version":1,"project_id":"demo","brain_id":"assistant","languages":["en-US"],"enabled_languages":["en-US"],"default_language":"en-US","packages":["packages/base/package.json"]}"#.to_vec();
        let package = r#"{"format":"gvya.test.package-spec","manifest":{"id":"base","kind":"standard","dependencies":[]},"contents":{"meanings":[{"id":"hello","value":{"id":"hello","samples":[{"language":"fa","text":"سلام"}]}}]}}"#.as_bytes().to_vec();
        let tree = test_source_tree(
            BTreeMap::from([
                ("gvya.project.json".into(), project),
                ("packages/base/package.json".into(), package),
            ]),
            SourceLimits::default(),
        )
        .unwrap();
        let issues = resolve_source_project(&tree, SourceLimits::default()).unwrap_err();
        assert!(
            issues
                .iter()
                .any(|row| row.code == "source.language_not_allowed")
        );
    }

    #[test]
    fn package_rejects_undeclared_fragment_json() {
        let tree = SourceTree::new(
            BTreeMap::from([
                (
                    "gvya.project.json".to_owned(),
                    br#"{"format":"gvya.source.project","version":1,"project_id":"demo","brain_id":"assistant","languages":["en"],"enabled_languages":["en"],"default_language":"en","packages":["packages/base/package.json"]}"#.to_vec(),
                ),
                (
                    "packages/base/package.json".to_owned(),
                    br#"{"format":"gvya.source.package","version":1,"manifest":{"id":"base","kind":"standard","dependencies":[]},"fragments":{}}"#.to_vec(),
                ),
                (
                    "packages/base/fragments/meanings/forgotten.json".to_owned(),
                    br#"{"id":"forgotten","value":{"id":"forgotten","samples":[{"language":"en","text":"forgotten"}]}}"#.to_vec(),
                ),
            ]),
            SourceLimits::default(),
        )
        .unwrap();
        let issues = resolve_source_project(&tree, SourceLimits::default()).unwrap_err();
        assert!(
            issues
                .iter()
                .any(|issue| issue.code == "source.fragment_undeclared")
        );
    }

    #[test]
    fn binding_source_rejects_fields_from_another_variant() {
        let tree = direct_fragment_tree(
            "capability_bindings",
            "binding",
            br#"{"id":"binding","value":{"id":"binding","trigger":{},"capability":"host.echo","arguments":[{"target":"message","source":{"type":"meaning_slot","name":"message","value":"must-not-be-ignored"}}]}}"#,
        );
        let issues = resolve_source_project(&tree, SourceLimits::default()).unwrap_err();
        assert!(issues.iter().any(|issue| {
            issue.code == "source.unknown_key"
                && issue
                    .path
                    .as_deref()
                    .is_some_and(|path| path.ends_with(".source.value"))
        }));
    }

    #[test]
    fn conversation_effect_rejects_fields_from_another_variant() {
        let tree = direct_fragment_tree(
            "behaviors",
            "behavior",
            br#"{"id":"hello.behavior","value":{"id":"hello.behavior","meaning":"hello","responses":[{"id":"hello.response","texts":[{"language":"en","variants":["hi"]}],"effects":[{"type":"assign","target":{"namespace":"author","path":"x"},"value":1,"delta":2}]}]}}"#,
        );
        let issues = resolve_source_project(&tree, SourceLimits::default()).unwrap_err();
        assert!(issues.iter().any(|issue| {
            issue.code == "source.unknown_key"
                && issue
                    .path
                    .as_deref()
                    .is_some_and(|path| path.ends_with(".delta"))
        }));
    }

    #[test]
    fn policy_allow_rejects_reason_code_from_other_variants() {
        let tree = direct_fragment_tree(
            "capability_policies",
            "policy",
            br#"{"id":"policy","value":{"id":"policy","capability":"host.echo","conditions":[],"effect":{"type":"allow","reason_code":"must-not-be-ignored"}}}"#,
        );
        let issues = resolve_source_project(&tree, SourceLimits::default()).unwrap_err();
        assert!(issues.iter().any(|issue| {
            issue.code == "source.unknown_key"
                && issue
                    .path
                    .as_deref()
                    .is_some_and(|path| path.ends_with(".effect.reason_code"))
        }));
    }
}
