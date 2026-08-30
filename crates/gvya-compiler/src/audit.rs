//! Human-first, machine-stable authoring audit model.
//!
//! The report is summary-first and grouped; raw issue detail remains available for AI/CI drill-in.

mod capabilities;
mod content;
mod conversation;
mod helpers;
mod identity;
mod regression;
mod semantics;
#[cfg(test)]
mod tests;

use capabilities::*;
use content::*;
use conversation::*;
use helpers::*;
use identity::*;
use regression::*;
use semantics::*;

use std::collections::{BTreeMap, BTreeSet};

use gvya_kernel::{
    capability::{IssueSeverity, SchemaLimits, validate_schema_definition},
    conversation::{ConversationEffect, ResponseDefinition, StateTarget},
    semantic::{
        SemanticProfile, SemanticProfiles, normalize_language_tag, normalize_text, ordered_tokens,
        profile_for_authored_language,
    },
};
use gvya_model::{AuditCode, PackageId};

use crate::{
    package::{
        ComposedProject, CompositionIssue, CompositionSeverity, ContributionKind,
        PackageDefinition, compose_packages,
    },
    testing::TurnExpectation,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum AuditSeverity {
    Error,
    Warning,
    Info,
}

impl AuditSeverity {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Error => "error",
            Self::Warning => "warning",
            Self::Info => "info",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AuditLocation {
    pub package: Option<PackageId>,
    pub kind: Option<ContributionKind>,
    pub item_id: Option<String>,
    pub sub_id: Option<String>,
    pub path: Option<String>,
}

impl AuditLocation {
    #[must_use]
    pub fn project() -> Self {
        Self {
            package: None,
            kind: None,
            item_id: None,
            sub_id: None,
            path: None,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AuditRelated {
    pub label: String,
    pub package: Option<PackageId>,
    pub kind: Option<ContributionKind>,
    pub item_id: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AuditIssue {
    pub code: AuditCode,
    pub severity: AuditSeverity,
    pub category: String,
    pub summary: String,
    pub location: AuditLocation,
    pub related: Vec<AuditRelated>,
    pub remediation: Option<String>,
    pub details: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct AuditSummary {
    pub errors: usize,
    pub warnings: usize,
    pub info: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AuditGroup {
    pub severity: AuditSeverity,
    pub category: String,
    pub issue_indexes: Vec<usize>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AuditReport {
    pub summary: AuditSummary,
    /// Human-facing order: errors, warnings, info; category order is deterministic.
    pub groups: Vec<AuditGroup>,
    pub issues: Vec<AuditIssue>,
    pub truncated: bool,
}

impl AuditReport {
    #[must_use]
    pub fn is_clean(&self) -> bool {
        self.summary.errors == 0 && !self.truncated
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AuditorLimits {
    pub max_issues: usize,
    pub max_overlap_pairs: usize,
    pub long_response_chars: usize,
    pub near_duplicate_threshold_milli: u16,
}

impl Default for AuditorLimits {
    fn default() -> Self {
        Self {
            max_issues: 4_000,
            max_overlap_pairs: 100_000,
            long_response_chars: 240,
            near_duplicate_threshold_milli: 800,
        }
    }
}

pub struct AuditContext<'a> {
    pub packages: &'a [PackageDefinition],
    pub project: &'a ComposedProject,
}

pub trait AuditRule {
    fn audit(
        &self,
        context: &AuditContext<'_>,
        issues: &mut Vec<AuditIssue>,
        limits: AuditorLimits,
    );
}

pub struct Auditor {
    limits: AuditorLimits,
    custom_rules: Vec<Box<dyn AuditRule>>,
}

impl Default for Auditor {
    fn default() -> Self {
        Self::new(AuditorLimits::default())
    }
}

impl Auditor {
    #[must_use]
    pub fn new(limits: AuditorLimits) -> Self {
        Self {
            limits,
            custom_rules: Vec::new(),
        }
    }

    #[must_use]
    pub fn with_rule(mut self, rule: impl AuditRule + 'static) -> Self {
        self.custom_rules.push(Box::new(rule));
        self
    }

    #[must_use]
    pub fn audit(
        &self,
        packages: &[PackageDefinition],
        semantic_profiles: &SemanticProfiles,
    ) -> AuditReport {
        if self.limits.max_issues == 0 {
            return finalize_report(
                vec![issue(
                    "audit.issue_budget_invalid",
                    AuditSeverity::Error,
                    "audit",
                    "Auditor max_issues must be positive",
                    AuditLocation::project(),
                )],
                true,
            );
        }

        // Audit with one sentinel slot beyond the report budget. Any pass that attempts to emit
        // one more issue proves the report is incomplete. We then truncate only for presentation.
        let report_limit = self.limits.max_issues;
        let working_limits = AuditorLimits {
            max_issues: report_limit.saturating_add(1),
            ..self.limits
        };
        let composition = compose_packages(packages, semantic_profiles);
        let mut issues = composition
            .issues
            .iter()
            .map(composition_issue)
            .collect::<Vec<_>>();
        let mut truncated = issues.len() > report_limit;
        if issues.len() > working_limits.max_issues {
            issues.truncate(working_limits.max_issues);
        }

        audit_source_identity(packages, &mut issues, working_limits);
        if let Some(project) = composition.project.as_ref() {
            let context = AuditContext { packages, project };
            audit_semantics(project, &mut issues, working_limits);
            audit_conversation(project, &mut issues, working_limits);
            audit_capabilities(project, &mut issues, working_limits);
            audit_types_and_localization(project, &mut issues, working_limits);
            audit_assets(project, &mut issues, working_limits);
            audit_tests(project, &mut issues, working_limits);
            for rule in &self.custom_rules {
                if issues.len() >= working_limits.max_issues {
                    truncated = true;
                    break;
                }
                rule.audit(&context, &mut issues, working_limits);
            }
        }
        if issues.len() > report_limit {
            issues.truncate(report_limit);
            truncated = true;
        }
        finalize_report(issues, truncated)
    }
}
