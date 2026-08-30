//! Compiled capability catalog and validation.

use std::collections::{BTreeMap, BTreeSet};

use gvya_model::{CapabilityContract, CapabilityId, ConfirmationHint, EffectClass};

use super::{
    binding::{BindingSource, CapabilityBindingRule},
    policy::{CapabilityPolicyRule, PolicyEffect},
    schema::{SchemaLimits, ValueSchema, validate_schema_definition},
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HostEffectKind {
    Read,
    Update,
    Create,
    Delete,
    External,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HostEffectDeclaration {
    /// Host-owned resource class, for example `door`, `inventory`, `payment` or `game.character`.
    pub resource: String,
    pub kind: HostEffectKind,
    /// Authoring-client summary only; it is not executable authority.
    pub summary: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CapabilityDefinition {
    pub contract: CapabilityContract,
    /// Compiler-produced runtime shape corresponding to `contract.input_schema`.
    pub input_shape: ValueSchema,
    /// Compiler-produced runtime shape corresponding to `contract.output_schema`.
    pub output_shape: Option<ValueSchema>,
    pub host_effects: Vec<HostEffectDeclaration>,
}

pub const CAPABILITY_SCHEMA_DEPTH_MAX: usize = 64;
pub const CAPABILITY_SCHEMA_ARRAY_ITEMS_MAX: usize = 16_384;
pub const CAPABILITY_SCHEMA_OBJECT_PROPERTIES_MAX: usize = 4_096;
pub const CAPABILITY_SCHEMA_STRING_BYTES_MAX: usize = 256 * 1024;
pub const CAPABILITY_SCHEMA_ERRORS_MAX: usize = 256;
pub const CAPABILITY_PROPOSALS_PER_TURN_MAX: usize = 256;
/// Canonical bound for admitted host proposals retained until a result consumes them.
pub const CAPABILITY_PENDING_PROPOSALS_MAX: usize = 1024;
pub const CAPABILITY_BINDINGS_MAX: usize = 50_000;
pub const CAPABILITY_POLICY_RULES_MAX: usize = 50_000;

#[derive(Clone, Debug, PartialEq)]
pub struct CapabilityConfig {
    pub schema_limits: SchemaLimits,
    pub max_proposals_per_turn: usize,
    pub max_bindings: usize,
    pub max_policy_rules: usize,
}

impl Default for CapabilityConfig {
    fn default() -> Self {
        Self {
            schema_limits: SchemaLimits::default(),
            max_proposals_per_turn: 8,
            max_bindings: 2048,
            max_policy_rules: 4096,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct CapabilityCatalog {
    capabilities: BTreeMap<CapabilityId, CapabilityDefinition>,
    bindings: Vec<CapabilityBindingRule>,
    policies: Vec<CapabilityPolicyRule>,
    config: CapabilityConfig,
}

pub fn validate_capability_config(config: &CapabilityConfig) -> Vec<CatalogIssue> {
    let mut issues = Vec::new();
    let schema = config.schema_limits;
    if schema.max_depth == 0 || schema.max_depth > CAPABILITY_SCHEMA_DEPTH_MAX {
        issues.push(CatalogIssue::error(
            "schema_depth_limit_invalid",
            "catalog",
            "schema max_depth must be within 1..=64",
        ));
    }
    if schema.max_array_items == 0 || schema.max_array_items > CAPABILITY_SCHEMA_ARRAY_ITEMS_MAX {
        issues.push(CatalogIssue::error(
            "schema_array_limit_invalid",
            "catalog",
            "schema max_array_items must be within 1..=16384",
        ));
    }
    if schema.max_object_properties == 0
        || schema.max_object_properties > CAPABILITY_SCHEMA_OBJECT_PROPERTIES_MAX
    {
        issues.push(CatalogIssue::error(
            "schema_object_limit_invalid",
            "catalog",
            "schema max_object_properties must be within 1..=4096",
        ));
    }
    if schema.max_string_bytes == 0 || schema.max_string_bytes > CAPABILITY_SCHEMA_STRING_BYTES_MAX
    {
        issues.push(CatalogIssue::error(
            "schema_string_limit_invalid",
            "catalog",
            "schema max_string_bytes must be within 1..=262144",
        ));
    }
    if schema.max_errors == 0 || schema.max_errors > CAPABILITY_SCHEMA_ERRORS_MAX {
        issues.push(CatalogIssue::error(
            "schema_error_limit_invalid",
            "catalog",
            "schema max_errors must be within 1..=256",
        ));
    }
    if config.max_proposals_per_turn == 0
        || config.max_proposals_per_turn > CAPABILITY_PROPOSALS_PER_TURN_MAX
    {
        issues.push(CatalogIssue::error(
            "proposal_limit_invalid",
            "catalog",
            "max proposals per turn must be within 1..=256",
        ));
    }
    if config.max_bindings > CAPABILITY_BINDINGS_MAX {
        issues.push(CatalogIssue::error(
            "binding_limit_invalid",
            "catalog",
            "max bindings must be <= 50000",
        ));
    }
    if config.max_policy_rules > CAPABILITY_POLICY_RULES_MAX {
        issues.push(CatalogIssue::error(
            "policy_limit_invalid",
            "catalog",
            "max policy rules must be <= 50000",
        ));
    }
    issues
}

impl CapabilityCatalog {
    pub fn new(
        definitions: Vec<CapabilityDefinition>,
        bindings: Vec<CapabilityBindingRule>,
        policies: Vec<CapabilityPolicyRule>,
        config: CapabilityConfig,
    ) -> Result<Self, Vec<CatalogIssue>> {
        let mut capabilities = BTreeMap::new();
        let mut issues = Vec::new();
        for definition in definitions {
            let id = definition.contract.id.clone();
            if capabilities.insert(id.clone(), definition).is_some() {
                issues.push(CatalogIssue::error(
                    "duplicate_capability",
                    id.as_str(),
                    "capability id is declared more than once",
                ));
            }
        }
        let mut bindings = bindings;
        bindings.sort_by(|left, right| left.id.as_str().cmp(right.id.as_str()));
        let mut policies = policies;
        policies.sort_by(|left, right| {
            left.capability
                .as_str()
                .cmp(right.capability.as_str())
                .then_with(|| left.id.as_str().cmp(right.id.as_str()))
        });
        let catalog = Self {
            capabilities,
            bindings,
            policies,
            config,
        };
        issues.extend(catalog.validate());
        if issues
            .iter()
            .any(|issue| issue.severity == IssueSeverity::Error)
        {
            Err(issues)
        } else {
            Ok(catalog)
        }
    }

    #[must_use]
    pub fn definition(&self, capability: &CapabilityId) -> Option<&CapabilityDefinition> {
        self.capabilities.get(capability)
    }

    #[must_use]
    pub fn bindings(&self) -> &[CapabilityBindingRule] {
        &self.bindings
    }

    #[must_use]
    pub fn policies(&self) -> &[CapabilityPolicyRule] {
        &self.policies
    }

    #[must_use]
    pub fn config(&self) -> &CapabilityConfig {
        &self.config
    }

    #[must_use]
    pub fn capability_ids(&self) -> impl Iterator<Item = &CapabilityId> {
        self.capabilities.keys()
    }

    #[must_use]
    pub fn validate(&self) -> Vec<CatalogIssue> {
        let mut issues = Vec::new();
        issues.extend(validate_capability_config(&self.config));
        if self.bindings.len() > self.config.max_bindings {
            issues.push(CatalogIssue::error(
                "binding_limit_exceeded",
                "catalog",
                "binding count exceeds configured limit",
            ));
        }
        if self.policies.len() > self.config.max_policy_rules {
            issues.push(CatalogIssue::error(
                "policy_limit_exceeded",
                "catalog",
                "policy count exceeds configured limit",
            ));
        }

        for definition in self.capabilities.values() {
            validate_definition(definition, self.config.schema_limits, &mut issues);
        }

        let mut binding_ids = BTreeSet::new();
        for binding in &self.bindings {
            if !binding_ids.insert(binding.id.as_str().to_owned()) {
                issues.push(CatalogIssue::error(
                    "duplicate_binding",
                    binding.id.as_str(),
                    "binding id is declared more than once",
                ));
            }
            if binding.trigger.is_empty() {
                issues.push(CatalogIssue::error(
                    "binding_trigger_empty",
                    binding.id.as_str(),
                    "binding must declare meaning, behavior or response trigger",
                ));
            }
            if !self.capabilities.contains_key(&binding.capability) {
                issues.push(CatalogIssue::error(
                    "binding_capability_undeclared",
                    binding.id.as_str(),
                    "binding references an undeclared capability",
                ));
            }
            let mut targets = BTreeSet::new();
            for argument in &binding.arguments {
                if argument.target.parts().is_empty() {
                    issues.push(CatalogIssue::error(
                        "binding_target_empty",
                        binding.id.as_str(),
                        "argument target is empty",
                    ));
                }
                if !targets.insert(argument.target.display()) {
                    issues.push(CatalogIssue::error(
                        "binding_target_duplicate",
                        binding.id.as_str(),
                        "two bindings write the same argument path",
                    ));
                }
                match &argument.source {
                    BindingSource::ContextPath(path)
                    | BindingSource::AuthorStatePath(path)
                    | BindingSource::MeaningSlot(path)
                        if path.trim().is_empty() =>
                    {
                        issues.push(CatalogIssue::error(
                            "binding_source_empty",
                            binding.id.as_str(),
                            "binding source path/name is empty",
                        ));
                    }
                    BindingSource::MeaningReference { kind, .. }
                    | BindingSource::FocusReference { kind, .. } => {
                        if let Some(definition) = self.capabilities.get(&binding.capability) {
                            if !definition.contract.reference_kinds.contains(kind) {
                                issues.push(CatalogIssue::error(
                                    "binding_reference_kind_undeclared",
                                    binding.id.as_str(),
                                    "reference binding kind is not declared by the capability contract",
                                ));
                            }
                        }
                    }
                    _ => {}
                }
            }
        }

        let mut policy_ids = BTreeSet::new();
        for policy in &self.policies {
            if !policy_ids.insert(policy.id.as_str().to_owned()) {
                issues.push(CatalogIssue::error(
                    "duplicate_policy",
                    policy.id.as_str(),
                    "policy id is declared more than once",
                ));
            }
            if !self.capabilities.contains_key(&policy.capability) {
                issues.push(CatalogIssue::error(
                    "policy_capability_undeclared",
                    policy.id.as_str(),
                    "policy references an undeclared capability",
                ));
            }
            match &policy.effect {
                PolicyEffect::RequireConfirmation { reason_code }
                | PolicyEffect::Deny { reason_code }
                    if reason_code.trim().is_empty() =>
                {
                    issues.push(CatalogIssue::error(
                        "policy_reason_empty",
                        policy.id.as_str(),
                        "deny/confirmation reason code cannot be empty",
                    ));
                }
                _ => {}
            }
        }

        for definition in self.capabilities.values() {
            if definition.contract.confirmation_hint == ConfirmationHint::Conditional
                && !self
                    .policies
                    .iter()
                    .any(|policy| policy.capability == definition.contract.id)
            {
                issues.push(CatalogIssue::error(
                    "conditional_capability_without_policy",
                    definition.contract.id.as_str(),
                    "conditional confirmation requires at least one explicit policy rule",
                ));
            }
        }

        issues
    }
}

fn validate_definition(
    definition: &CapabilityDefinition,
    limits: SchemaLimits,
    issues: &mut Vec<CatalogIssue>,
) {
    let contract = &definition.contract;
    if contract.id.as_str().trim().is_empty() {
        issues.push(CatalogIssue::error(
            "capability_id_empty",
            "capability",
            "capability id cannot be empty",
        ));
    }
    if contract.version.as_str().trim().is_empty() {
        issues.push(CatalogIssue::error(
            "capability_version_empty",
            contract.id.as_str(),
            "capability version cannot be empty",
        ));
    }
    let mut reference_kinds = BTreeSet::new();
    for kind in &contract.reference_kinds {
        if kind.as_str().trim().is_empty() {
            issues.push(CatalogIssue::error(
                "capability_reference_kind_empty",
                contract.id.as_str(),
                "reference kind cannot be empty",
            ));
        } else if !reference_kinds.insert(kind.as_str().to_owned()) {
            issues.push(CatalogIssue::error(
                "capability_reference_kind_duplicate",
                contract.id.as_str(),
                "reference kind is declared more than once",
            ));
        }
    }
    if !definition.input_shape.is_object() {
        issues.push(CatalogIssue::error(
            "capability_input_not_object",
            contract.id.as_str(),
            "capability input root must be object-shaped",
        ));
    }
    for issue in validate_schema_definition(&definition.input_shape, limits) {
        issues.push(CatalogIssue::error(
            "compiled_input_schema_invalid",
            contract.id.as_str(),
            &format!("{} {}: {}", issue.path, issue.code, issue.message),
        ));
    }
    if let Some(output_shape) = &definition.output_shape {
        for issue in validate_schema_definition(output_shape, limits) {
            issues.push(CatalogIssue::error(
                "compiled_output_schema_invalid",
                contract.id.as_str(),
                &format!("{} {}: {}", issue.path, issue.code, issue.message),
            ));
        }
    }
    if contract.output_schema.is_some() != definition.output_shape.is_some() {
        issues.push(CatalogIssue::error(
            "capability_output_shape_mismatch",
            contract.id.as_str(),
            "compiled output shape must match output-schema presence",
        ));
    }
    if contract.input_schema.as_str().trim().is_empty() {
        issues.push(CatalogIssue::error(
            "capability_input_schema_empty",
            contract.id.as_str(),
            "source input schema document is empty",
        ));
    }
    if contract.effect_class == EffectClass::Pure
        && definition
            .host_effects
            .iter()
            .any(|effect| effect.kind != HostEffectKind::Read)
    {
        issues.push(CatalogIssue::error(
            "pure_capability_declares_mutation",
            contract.id.as_str(),
            "pure capability may declare read effects only",
        ));
    }
    let mut effect_keys = BTreeSet::new();
    for effect in &definition.host_effects {
        if effect.resource.trim().is_empty() {
            issues.push(CatalogIssue::error(
                "host_effect_resource_empty",
                contract.id.as_str(),
                "host effect resource cannot be empty",
            ));
        }
        let key = format!("{}::{:?}", effect.resource, effect.kind);
        if !effect_keys.insert(key) {
            issues.push(CatalogIssue::warning(
                "duplicate_host_effect",
                contract.id.as_str(),
                "duplicate host effect declaration",
            ));
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IssueSeverity {
    Error,
    Warning,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CatalogIssue {
    pub severity: IssueSeverity,
    pub code: String,
    pub subject: String,
    pub message: String,
}

impl CatalogIssue {
    fn error(code: &str, subject: &str, message: &str) -> Self {
        Self {
            severity: IssueSeverity::Error,
            code: code.into(),
            subject: subject.into(),
            message: message.into(),
        }
    }

    fn warning(code: &str, subject: &str, message: &str) -> Self {
        Self {
            severity: IssueSeverity::Warning,
            code: code.into(),
            subject: subject.into(),
            message: message.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gvya_model::{CapabilityVersion, SchemaDocument};

    fn definition(id: &str) -> CapabilityDefinition {
        CapabilityDefinition {
            contract: CapabilityContract {
                id: CapabilityId::new(id),
                version: CapabilityVersion::new("1"),
                title: id.into(),
                description: String::new(),
                input_schema: SchemaDocument::new("{}"),
                output_schema: None,
                reference_kinds: Vec::new(),
                effect_class: EffectClass::Reversible,
                confirmation_hint: ConfirmationHint::Never,
            },
            input_shape: ValueSchema::object(BTreeMap::new(), BTreeSet::new()),
            output_shape: None,
            host_effects: Vec::new(),
        }
    }

    #[test]
    fn binding_to_undeclared_capability_is_rejected() {
        let binding = CapabilityBindingRule {
            id: gvya_model::CapabilityBindingId::new("b"),
            trigger: super::super::binding::CapabilityTrigger {
                meaning: Some(gvya_model::MeaningId::new("x")),
                behavior: None,
                response: None,
            },
            capability: CapabilityId::new("missing"),
            arguments: Vec::new(),
        };
        let result = CapabilityCatalog::new(
            vec![definition("known")],
            vec![binding],
            Vec::new(),
            CapabilityConfig::default(),
        );
        assert!(result.is_err());
    }

    #[test]
    fn catalog_sorts_bindings_by_stable_id() {
        let make_binding = |id: &str| CapabilityBindingRule {
            id: gvya_model::CapabilityBindingId::new(id),
            trigger: super::super::binding::CapabilityTrigger {
                meaning: Some(gvya_model::MeaningId::new("x")),
                behavior: None,
                response: None,
            },
            capability: CapabilityId::new("known"),
            arguments: Vec::new(),
        };
        let catalog = CapabilityCatalog::new(
            vec![definition("known")],
            vec![make_binding("z-last"), make_binding("a-first")],
            Vec::new(),
            CapabilityConfig::default(),
        )
        .unwrap();
        assert_eq!(catalog.bindings()[0].id.as_str(), "a-first");
        assert_eq!(catalog.bindings()[1].id.as_str(), "z-last");
    }

    #[test]
    fn duplicate_reference_kind_is_rejected_at_catalog_boundary() {
        let mut definition = definition("door.open");
        definition.contract.reference_kinds = vec![
            gvya_model::ReferenceKind::new("door"),
            gvya_model::ReferenceKind::new("door"),
        ];
        let issues = CapabilityCatalog::new(
            vec![definition],
            Vec::new(),
            Vec::new(),
            CapabilityConfig::default(),
        )
        .unwrap_err();
        assert!(
            issues
                .iter()
                .any(|issue| issue.code == "capability_reference_kind_duplicate")
        );
    }
}
