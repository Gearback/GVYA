import { compareUtf8 } from "./canonical-order.js";
import type { AuditIssue, CoverageSummary, ResponseDefinition, StudioPackage, StudioBrainWorkspace } from "./types.js";
import { isWellFormedLanguageTag, languageKey } from "./languages.js";
import { normalizeSample } from "./workspace.js";

function issue(
  severity: AuditIssue["severity"],
  code: string,
  title: string,
  detail: string,
  packageId: string,
  objectType: AuditIssue["objectType"],
  objectId: string,
): AuditIssue {
  return {
    id: `${code}:${packageId}:${objectType}:${objectId}`,
    severity,
    code,
    title,
    detail,
    packageId,
    objectType,
    objectId,
  };
}

interface WorkspaceAuditIndex {
  meanings: Set<string>;
  behaviors: Set<string>;
  responses: Set<string>;
  capabilities: Map<string, Set<string>>;
  assets: Set<string>;
}

function buildWorkspaceAuditIndex(workspace: StudioBrainWorkspace): WorkspaceAuditIndex {
  const meanings = new Set<string>();
  const behaviors = new Set<string>();
  const responses = new Set<string>();
  const capabilities = new Map<string, Set<string>>();
  const assets = new Set<string>();
  for (const pkg of workspace.packages) {
    for (const row of pkg.contents.meanings) meanings.add(row.value.id);
    for (const row of pkg.contents.behaviors) {
      behaviors.add(row.value.id);
      for (const response of row.value.responses) responses.add(response.id);
    }
    for (const row of pkg.contents.capability_result_behaviors) {
      behaviors.add(row.value.id);
      for (const response of row.value.responses) responses.add(response.id);
    }
    for (const row of pkg.contents.openings) for (const response of row.value.responses) responses.add(response.id);
    for (const row of pkg.contents.fallback_behaviors) {
      for (const response of row.value.responses) responses.add(response.id);
    }
    for (const row of pkg.contents.capabilities) {
      const versions = capabilities.get(row.value.contract.id) ?? new Set<string>();
      versions.add(row.value.contract.version);
      capabilities.set(row.value.contract.id, versions);
    }
    for (const row of pkg.contents.assets) assets.add(row.value.id);
  }
  return { meanings, behaviors, responses, capabilities, assets };
}

export function auditWorkspace(workspace: StudioBrainWorkspace): AuditIssue[] {
  const issues: AuditIssue[] = [];
  const packageIds = new Set<string>();
  if (!workspace.project_id.trim()) issues.push(issue("error", "studio.project_id", "Project ID is required", "Set a stable project ID before exporting source.", "", "project", "project"));
  if (!workspace.brain_id.trim()) issues.push(issue("error", "studio.brain_id", "Brain ID is required", "Set a stable brain ID before exporting source.", "", "project", "brain"));
  if (workspace.languages.length === 0) issues.push(issue("error", "studio.languages_empty", "Project has no languages", "Select at least one language in the Project.", "", "project", workspace.project_id));
  const projectLanguages = new Set(workspace.languages.map(languageKey));
  const enabledLanguages = new Set(workspace.enabled_languages.map(languageKey));
  if (workspace.enabled_languages.length === 0 || workspace.enabled_languages.some((language) => !isWellFormedLanguageTag(language) || !projectLanguages.has(languageKey(language))) || enabledLanguages.size !== workspace.enabled_languages.length) {
    issues.push(issue("error", "studio.enabled_languages", "Bot enabled languages are invalid", "Enabled languages must be a unique, non-empty subset of this Project's languages.", "", "project", workspace.brain_id));
  }
  if (!isWellFormedLanguageTag(workspace.default_language) || !enabledLanguages.has(languageKey(workspace.default_language))) {
    issues.push(issue("error", "studio.default_language", "Bot default language is not enabled", `${workspace.default_language || "(empty)"} must name one of this Bot's enabled languages.`, "", "project", workspace.brain_id));
  }
  if (!isWellFormedLanguageTag(workspace.authoring_language) || !projectLanguages.has(languageKey(workspace.authoring_language))) {
    issues.push(issue("error", "studio.authoring_language", "Package authoring language is unavailable", `${workspace.authoring_language || "(empty)"} must name one of the available authoring languages.`, workspace.selectedPackageId, "package", workspace.selectedPackageId));
  }

  for (const pkg of workspace.packages) {
    if (packageIds.has(pkg.manifest.id)) {
      issues.push(issue("error", "studio.package_duplicate", "Duplicate package ID", `Package ${pkg.manifest.id} is declared more than once.`, pkg.manifest.id, "package", pkg.manifest.id));
    }
    packageIds.add(pkg.manifest.id);
  }
  const fallbackPackages = workspace.packages.filter((pkg) => pkg.manifest.kind === "fallback");
  if (fallbackPackages.length > 1) issues.push(issue("error", "studio.multiple_fallback_packages", "More than one Fallback Package is selected", "A Brain may select at most one Fallback Package root.", "", "project", workspace.brain_id));

  auditDependencyCycles(workspace, issues);
  const projectIndex = buildWorkspaceAuditIndex(workspace);
  for (const pkg of workspace.packages) auditPackage(workspace, pkg, issues, packageIds, projectIndex);
  for (const row of languageCoverage(workspace)) {
    if (row.samples === 0) issues.push(issue("warning", "studio.language_no_matching_evidence", "Language has no positive matching evidence", `${row.language} is selected by the Project but has no structural patterns or Meaning samples.`, "", "project", row.language));
    if (row.responseVariants === 0) issues.push(issue("warning", "studio.language_no_responses", "Language has no response text", `${row.language} is selected by the Project but has no authored response variants.`, "", "project", row.language));
  }
  return issues.sort((a, b) => severityRank(a.severity) - severityRank(b.severity) || compareUtf8(a.code, b.code) || compareUtf8(a.objectId, b.objectId));
}

function auditDependencyCycles(workspace: StudioBrainWorkspace, issues: AuditIssue[]): void {
  const packages = new Map(workspace.packages.map((pkg) => [pkg.manifest.id, pkg]));
  const visiting = new Set<string>();
  const visited = new Set<string>();
  const reported = new Set<string>();
  const stack: string[] = [];
  const visit = (id: string): void => {
    if (visited.has(id)) return;
    if (visiting.has(id)) {
      const start = stack.indexOf(id);
      const cycle = (start >= 0 ? stack.slice(start) : [id]).concat(id);
      const key = cycle.slice(0, -1).sort().join("\u0000");
      if (!reported.has(key)) {
        reported.add(key);
        issues.push(issue("error", "studio.package_dependency_cycle", "Package dependency cycle", cycle.join(" -> "), id, "package", id));
      }
      return;
    }
    const pkg = packages.get(id);
    if (!pkg) return;
    visiting.add(id); stack.push(id);
    for (const dep of pkg.manifest.dependencies) if (packages.has(dep.id)) visit(dep.id);
    stack.pop(); visiting.delete(id); visited.add(id);
  };
  for (const id of packages.keys()) visit(id);
}

function auditPackage(workspace: StudioBrainWorkspace, pkg: StudioPackage, issues: AuditIssue[], packageIds: Set<string>, projectIndex: WorkspaceAuditIndex): void {
  const packageId = pkg.manifest.id;
  if (!packageId.trim()) issues.push(issue("error", "studio.package_id", "Package ID is required", "Each package needs a stable ID.", packageId, "package", packageId));
  if (pkg.manifest.kind === "standard" && pkg.contents.fallback_behaviors.length) issues.push(issue("error", "studio.standard_has_fallback", "Standard Package contains fallback behavior", "Move fallback behaviors into a Shared or Project Fallback Package.", packageId, "package", packageId));
  if (pkg.manifest.kind === "fallback") {
    if (pkg.manifest.dependencies.length) issues.push(issue("error", "studio.fallback_dependency", "Fallback Package has dependencies", "Fallback Packages are self-contained and cannot declare dependencies.", packageId, "package", packageId));
    const forbidden = [pkg.contents.meanings, pkg.contents.behaviors, pkg.contents.capability_result_behaviors, pkg.contents.openings, pkg.contents.style_lexicons, pkg.contents.capabilities, pkg.contents.capability_bindings, pkg.contents.capability_policies, pkg.contents.capability_configs, pkg.contents.types];
    if (forbidden.some((rows) => rows.length)) issues.push(issue("error", "studio.fallback_namespace", "Fallback Package contains Standard Package content", "Fallback Packages may contain fallback behaviors, assets, regression cases, and scenarios only.", packageId, "package", packageId));
    for (const rows of [pkg.contents.fallback_behaviors, pkg.contents.assets, pkg.contents.regression_cases, pkg.contents.scenarios]) for (const row of rows as Array<{ exported: boolean; mode: unknown }>) if (row.exported || row.mode !== "add") issues.push(issue("error", "studio.fallback_override_contract", "Fallback content is overrideable", "Fallback Package contributions must be private add-only.", packageId, "package", packageId));
  }

  const dependencyIds = new Set<string>();
  for (const dep of pkg.manifest.dependencies) {
    if (dependencyIds.has(dep.id)) issues.push(issue("error", "studio.package_dependency_duplicate", "Dependency is declared more than once", `${packageId} declares ${dep.id} more than once.`, packageId, "package", packageId));
    dependencyIds.add(dep.id);
    if (dep.id === packageId) issues.push(issue("error", "studio.package_self_dependency", "Package depends on itself", `${packageId} cannot depend on itself.`, packageId, "package", packageId));
    if (!packageIds.has(dep.id)) issues.push(issue("error", "studio.package_dependency_missing", "Dependency is not in this project", `${dep.id} is referenced but not enumerated by this project.`, packageId, "package", packageId));
    const target = workspace.packages.find((candidate) => candidate.manifest.id === dep.id);
    if (target?.manifest.kind === "fallback") issues.push(issue("error", "studio.fallback_dependency_forbidden", "Fallback Package used as dependency", "Fallback Packages are selected directly by the Brain and cannot participate in dependency or override graphs.", packageId, "package", packageId));
  }

  const responses = new Set<string>();
  const regressionMeanings = new Set(pkg.contents.regression_cases.map((row) => row.value.expectation.meaning).filter(Boolean));

  duplicateContributionIds(pkg, issues);

  const exactSamples = new Map<string, string[]>();
  for (const row of pkg.contents.meanings) {
    const meaning = row.value;
    if (!meaning.id.trim()) issues.push(issue("error", "studio.meaning_id", "Meaning ID is required", "A Meaning cannot be exported without an ID.", packageId, "meaning", row.id));
    const usable = meaning.samples.map((sample) => normalizeSample(sample.text)).filter(Boolean);
    const usablePatterns = meaning.patterns.filter((pattern) => pattern.text.trim() !== "");
    if (usable.length === 0 && usablePatterns.length === 0) issues.push(issue("error", "studio.meaning_no_positive_evidence", "Meaning has no positive matching evidence", "Add at least one structural pattern or representative semantic sample before saving this Meaning.", packageId, "meaning", meaning.id));
    for (const sample of usable) {
      const ids = exactSamples.get(sample) ?? [];
      if (!ids.includes(meaning.id)) ids.push(meaning.id);
      exactSamples.set(sample, ids);
    }
    for (const pattern of meaning.patterns) auditLanguage(workspace, pattern.language, issues, packageId, "meaning", meaning.id, "Meaning structural pattern");
    for (const sample of meaning.samples) auditLanguage(workspace, sample.language, issues, packageId, "meaning", meaning.id, "Meaning sample");
    for (const slot of meaning.slots) {
      if (slot.required && !slot.elicitation.some((prompt) => prompt.language.trim() && prompt.text.trim())) issues.push(issue("error", "studio.required_slot_elicitation_missing", "Required slot needs a collection prompt", `Add at least one localized prompt for ${slot.name || "this slot"}.`, packageId, "meaning", meaning.id));
      for (const prompt of slot.elicitation) auditLanguage(workspace, prompt.language, issues, packageId, "meaning", meaning.id, "Slot collection prompt");
    }
    for (const reference of meaning.references) {
      if (reference.required && !reference.elicitation.some((prompt) => prompt.language.trim() && prompt.text.trim())) issues.push(issue("error", "studio.required_reference_elicitation_missing", "Required reference needs a collection prompt", `Add at least one localized prompt for ${reference.kind || "this reference"}.`, packageId, "meaning", meaning.id));
      for (const prompt of reference.elicitation) auditLanguage(workspace, prompt.language, issues, packageId, "meaning", meaning.id, "Reference collection prompt");
    }
    if (!regressionMeanings.has(meaning.id)) issues.push(issue("info", "studio.meaning_untested", "Meaning has no direct regression expectation", "Add a regression case that explicitly expects this Meaning.", packageId, "meaning", meaning.id));
  }

  for (const [sample, ids] of exactSamples) {
    if (ids.length > 1) {
      for (const id of ids) issues.push(issue("warning", "studio.exact_sample_collision", "Exact sample is shared by multiple Meanings", `“${sample}” also appears in: ${ids.filter((other) => other !== id).join(", ")}. This is an authoring collision check only; runtime scoring remains canonical Rust.`, packageId, "meaning", id));
    }
  }

  for (const row of pkg.contents.behaviors) {
    const behavior = row.value;
    if (!projectIndex.meanings.has(behavior.meaning)) issues.push(issue("error", "studio.behavior_meaning_missing", "Behavior references an unknown Meaning", `${behavior.id} points to ${behavior.meaning || "(empty)"}.`, packageId, "behavior", behavior.id));
    if (behavior.responses.length === 0) issues.push(issue("error", "studio.behavior_no_responses", "Behavior has no responses", "Add at least one response before saving this Behavior.", packageId, "behavior", behavior.id));
    for (const requirement of [...behavior.requires_values, ...behavior.forbidden_values]) if (!requirement.path.trim()) issues.push(issue("error", "studio.behavior_requirement_path_empty", "Behavior value requirement has an empty path", "Choose a value path or remove the requirement.", packageId, "behavior", behavior.id));
    for (const required of behavior.requires_values) if (behavior.forbidden_values.some((forbidden) => forbidden.namespace === required.namespace && forbidden.path === required.path && JSON.stringify(forbidden.value) === JSON.stringify(required.value))) issues.push(issue("error", "studio.behavior_requirement_conflict", "Behavior requires and forbids the same value", `${required.namespace}.${required.path} cannot be both required and forbidden.`, packageId, "behavior", behavior.id));
    const localResponseIds = new Set<string>();
    for (const response of behavior.responses) {
      auditResponseLanguages(workspace, response, issues, packageId);
      if (localResponseIds.has(response.id)) issues.push(issue("error", "studio.response_duplicate", "Duplicate response ID inside behavior", `${response.id} is repeated in ${behavior.id}.`, packageId, "response", response.id));
      localResponseIds.add(response.id);
      responses.add(response.id);
      if (!hasVisibleResponseContent(response)) issues.push(issue("error", "studio.response_empty", "Response has no visible content", "Add text, an asset, or a link before saving this Behavior.", packageId, "response", response.id));
      for (const asset of response.assets) if (!projectIndex.assets.has(asset.asset_id)) issues.push(issue("error", "studio.response_asset_missing", "Response references an unknown asset", `${response.id} points to ${asset.asset_id || "(empty)"}.`, packageId, "response", response.id));
      if (response.kind === "hint" && response.hint_level === null) issues.push(issue("warning", "studio.hint_level_missing", "Hint response has no hint level", "Hint responses should declare the requested progression level.", packageId, "response", response.id));
      if ((response.kind === "repeat" || response.kind === "annoyed_repeat" || response.kind === "final_repeat") && response.repeat_stage === "") issues.push(issue("warning", "studio.repeat_stage_missing", "Repeat response has no repeat stage", "Declare repeat, annoyed, or final progression explicitly.", packageId, "response", response.id));
    }
  }

  for (const row of pkg.contents.fallback_behaviors) {
    const fallback = row.value;
    if (!fallback.id.trim()) issues.push(issue("error", "studio.fallback_id", "Fallback Behavior ID is required", "Each Fallback Behavior needs a stable ID.", packageId, "behavior", row.id));
    if (fallback.responses.length === 0) issues.push(issue("error", "studio.fallback_no_responses", "Fallback Behavior has no responses", "Add at least one response before saving this Fallback Behavior.", packageId, "behavior", fallback.id));
    for (const condition of fallback.conditions) {
      if (!condition.path.trim()) issues.push(issue("error", "studio.fallback_condition_path_empty", "Fallback condition has an empty path", "Choose a state path or remove the condition.", packageId, "behavior", fallback.id));
      if (condition.namespace === "meaning" || condition.namespace === "interaction") issues.push(issue("error", "studio.fallback_condition_namespace", "Fallback condition uses unavailable matching state", "Fallback selection may use author, conversation, context, or system state only.", packageId, "behavior", fallback.id));
    }
    const localResponseIds = new Set<string>();
    for (const response of fallback.responses) {
      auditResponseLanguages(workspace, response, issues, packageId);
      if (localResponseIds.has(response.id)) issues.push(issue("error", "studio.response_duplicate", "Duplicate response ID inside Fallback Behavior", `${response.id} is repeated in ${fallback.id}.`, packageId, "response", response.id));
      localResponseIds.add(response.id); responses.add(response.id);
      if (!hasVisibleResponseContent(response)) issues.push(issue("error", "studio.response_empty", "Response has no visible content", "Add text, an asset, or a link before saving this Fallback Behavior.", packageId, "response", response.id));
      for (const asset of response.assets) if (!projectIndex.assets.has(asset.asset_id)) issues.push(issue("error", "studio.response_asset_missing", "Response references an unknown asset", `${response.id} points to ${asset.asset_id || "(empty)"}.`, packageId, "response", response.id));
    }
  }

  for (const row of pkg.contents.capability_result_behaviors) {
    const handler = row.value;
    const versions = projectIndex.capabilities.get(handler.capability);
    if (!versions) issues.push(issue("error", "studio.result_handler_capability_missing", "Result handler references an unknown capability", `${handler.id} points to ${handler.capability || "(empty)"}.`, packageId, "behavior", handler.id));
    else if (!versions.has(handler.capability_version)) issues.push(issue("error", "studio.result_handler_version_mismatch", "Result handler capability version is stale", `${handler.id} expects ${handler.capability}@${handler.capability_version}, which is absent from the project capability catalog.`, packageId, "behavior", handler.id));
    if (handler.succeeded === true && handler.error_code.trim()) issues.push(issue("error", "studio.result_handler_success_error", "Successful result handler cannot require an error code", "Error-code matching is valid only for failure results.", packageId, "behavior", handler.id));
    if (handler.responses.length === 0) issues.push(issue("warning", "studio.result_handler_no_responses", "Capability-result handler has no responses", "The host result will be accepted but this handler cannot produce an authored continuation.", packageId, "behavior", handler.id));
    for (const response of handler.responses) {
      auditResponseLanguages(workspace, response, issues, packageId);
      responses.add(response.id);
      if (!hasVisibleResponseContent(response)) issues.push(issue("warning", "studio.response_empty", "Response has no visible content", "Add text, an asset, or a link so the response has authored output.", packageId, "response", response.id));
      for (const asset of response.assets) if (!projectIndex.assets.has(asset.asset_id)) issues.push(issue("error", "studio.response_asset_missing", "Response references an unknown asset", `${response.id} points to ${asset.asset_id || "(empty)"}.`, packageId, "response", response.id));
    }
  }

  for (const row of pkg.contents.openings) {
    const opening = row.value;
    if (!opening.id.trim()) issues.push(issue("error", "studio.opening_id", "Opening ID is required", "Each Opening needs a stable ID.", packageId, "behavior", row.id));
    if (opening.responses.length === 0) issues.push(issue("warning", "studio.opening_no_responses", "Opening has no responses", "Add at least one localized opening response.", packageId, "behavior", opening.id));
    const localResponseIds = new Set<string>();
    for (const response of opening.responses) {
      auditResponseLanguages(workspace, response, issues, packageId);
      if (localResponseIds.has(response.id)) issues.push(issue("error", "studio.response_duplicate", "Duplicate response ID inside Opening", `${response.id} is repeated in ${opening.id}.`, packageId, "response", response.id));
      localResponseIds.add(response.id);
      responses.add(response.id);
    }
  }

  for (const row of pkg.contents.capabilities) {
    const cap = row.value.contract;
    if (!cap.id.trim()) issues.push(issue("error", "studio.capability_id", "Capability ID is required", "Capability contracts require stable IDs.", packageId, "capability", row.id));
    if (!cap.version.trim()) issues.push(issue("error", "studio.capability_version", "Capability version is required", "Capability contracts require a non-empty version.", packageId, "capability", cap.id || row.id));
    if (cap.effect_class === "irreversible" && cap.confirmation_hint === "never") issues.push(issue("warning", "studio.irreversible_no_confirmation_hint", "Irreversible capability says confirmation is never hinted", "Review the contract and policy rules. Admission remains runtime-owned; this warning prevents an easy authoring oversight.", packageId, "capability", cap.id));
    if (cap.input_schema.type !== "object") issues.push(issue("warning", "studio.capability_input_schema", "Capability input schema is not object-shaped", "GVYA supports a bounded JSON Schema profile; object-shaped inputs are the normal authoring pattern for named argument bindings.", packageId, "capability", cap.id));
  }

  for (const row of pkg.contents.capability_bindings) {
    const binding = row.value;
    if (!projectIndex.capabilities.has(binding.capability)) issues.push(issue("error", "studio.binding_capability_missing", "Binding references an unknown capability", `${binding.id} points to ${binding.capability || "(empty)"}.`, packageId, "binding", binding.id));
    const triggerCount = Number(Boolean(binding.trigger.meaning)) + Number(Boolean(binding.trigger.behavior)) + Number(Boolean(binding.trigger.response));
    if (triggerCount === 0) issues.push(issue("error", "studio.binding_trigger_missing", "Binding has no trigger", "Select at least one Meaning, behavior, or response trigger.", packageId, "binding", binding.id));
    if (binding.trigger.meaning && !projectIndex.meanings.has(binding.trigger.meaning)) issues.push(issue("error", "studio.binding_meaning_unknown", "Binding trigger Meaning is absent from the project", `${binding.trigger.meaning} is not present in the composed project source.`, packageId, "binding", binding.id));
    if (binding.trigger.behavior && !projectIndex.behaviors.has(binding.trigger.behavior)) issues.push(issue("error", "studio.binding_behavior_unknown", "Binding trigger behavior is absent from the project", `${binding.trigger.behavior} is not present in the composed project source.`, packageId, "binding", binding.id));
    if (binding.trigger.response && !projectIndex.responses.has(binding.trigger.response)) issues.push(issue("error", "studio.binding_response_unknown", "Binding trigger response is absent from the project", `${binding.trigger.response} is not present in the composed project source.`, packageId, "binding", binding.id));
  }

  for (const row of pkg.contents.capability_policies) {
    const policy = row.value;
    if (!projectIndex.capabilities.has(policy.capability)) issues.push(issue("error", "studio.policy_capability_unknown", "Policy capability is absent from the project", `${policy.capability} is not present in the project capability catalog.`, packageId, "policy", policy.id));
    if (policy.effect.type !== "allow" && !policy.effect.reason_code.trim()) issues.push(issue("error", "studio.policy_reason_missing", "Policy reason code is required", "Confirmation and deny effects must carry a stable reason code for inspectable runtime decisions.", packageId, "policy", policy.id));
  }

  for (const row of pkg.contents.regression_cases) {
    const test = row.value;
    if (test.language) auditLanguage(workspace, test.language, issues, packageId, "test", test.id, "Regression language");
    if (!test.input.trim()) issues.push(issue("warning", "studio.regression_input_empty", "Regression case has no input", "Add the user utterance this case is meant to protect.", packageId, "test", test.id));
    if (!test.expectation.meaning && test.expectation.response_ids.length === 0 && test.expectation.capabilities.length === 0 && test.expectation.proposal_receipts.length === 0 && test.expectation.why_codes.length === 0) {
      issues.push(issue("info", "studio.regression_expectation_empty", "Regression case has no focused expectation", "A case without assertions can still exercise the runtime, but it protects less behavior.", packageId, "test", test.id));
    }
    if (test.expectation.meaning && !projectIndex.meanings.has(test.expectation.meaning)) issues.push(issue("error", "studio.regression_meaning_missing", "Regression case expects an unknown Meaning", `${test.id} expects ${test.expectation.meaning}.`, packageId, "test", test.id));
    for (const capability of test.expectation.capabilities) if (!projectIndex.capabilities.has(capability.id)) issues.push(issue("error", "studio.regression_capability_missing", "Regression case expects an unknown capability", `${test.id} expects ${capability.id}.`, packageId, "test", test.id));
    for (const receipt of test.expectation.proposal_receipts) if (!projectIndex.capabilities.has(receipt.id)) issues.push(issue("error", "studio.regression_proposal_receipt_capability_missing", "Regression case expects a proposal receipt for an unknown capability", `${test.id} expects a proposal receipt for ${receipt.id}.`, packageId, "test", test.id));
  }

  for (const row of pkg.contents.scenarios) {
    const scenario = row.value;
    for (const [stepIndex, step] of scenario.steps.entries()) {
      if (step.type !== "confirm" && step.language) auditLanguage(workspace, step.language, issues, packageId, "test", scenario.id, `Scenario step ${stepIndex + 1} language`);
      if (step.expectation.meaning && !projectIndex.meanings.has(step.expectation.meaning)) issues.push(issue("error", "studio.scenario_meaning_missing", "Scenario expects an unknown Meaning", `${scenario.id} step ${stepIndex + 1} expects ${step.expectation.meaning}.`, packageId, "test", scenario.id));
      for (const capability of step.expectation.capabilities) if (!projectIndex.capabilities.has(capability.id)) issues.push(issue("error", "studio.scenario_capability_missing", "Scenario expects an unknown capability", `${scenario.id} step ${stepIndex + 1} expects ${capability.id}.`, packageId, "test", scenario.id));
      for (const receipt of step.expectation.proposal_receipts) if (!projectIndex.capabilities.has(receipt.id)) issues.push(issue("error", "studio.scenario_proposal_receipt_capability_missing", "Scenario expects a proposal receipt for an unknown capability", `${scenario.id} step ${stepIndex + 1} expects a proposal receipt for ${receipt.id}.`, packageId, "test", scenario.id));
      if ((step.type === "capability_result" || step.type === "confirm") && step.proposal_capability && !projectIndex.capabilities.has(step.proposal_capability)) issues.push(issue("error", "studio.scenario_proposal_capability_missing", "Scenario references an unknown proposal capability", `${scenario.id} step ${stepIndex + 1} references ${step.proposal_capability}.`, packageId, "test", scenario.id));
      if ((step.type === "capability_result" || step.type === "confirm") && step.proposal_ordinal !== null && (!Number.isInteger(step.proposal_ordinal) || step.proposal_ordinal <= 0)) issues.push(issue("error", "studio.scenario_proposal_ordinal_invalid", "Scenario proposal ordinal must be one-based", `${scenario.id} step ${stepIndex + 1} has invalid proposal_ordinal.`, packageId, "test", scenario.id));
      if ((step.type === "capability_result" || step.type === "confirm") && (step.proposal_from_step <= 0 || step.proposal_from_step > stepIndex)) issues.push(issue("error", "studio.scenario_step_reference_invalid", "Scenario step must reference an earlier proposal step", `${scenario.id} step ${stepIndex + 1} references step ${step.proposal_from_step}.`, packageId, "test", scenario.id));
    }
  }
}

function duplicateContributionIds(pkg: StudioPackage, issues: AuditIssue[]): void {
  const groups: Array<[string, Array<{ id: string }>]> = [
    ["meaning", pkg.contents.meanings],
    ["behavior", pkg.contents.behaviors],
    ["behavior", pkg.contents.capability_result_behaviors],
    ["behavior", pkg.contents.fallback_behaviors],
    ["capability", pkg.contents.capabilities],
    ["binding", pkg.contents.capability_bindings],
    ["policy", pkg.contents.capability_policies],
    ["test", pkg.contents.regression_cases],
    ["test", pkg.contents.scenarios],
  ];
  for (const [kind, rows] of groups) {
    const seen = new Set<string>();
    for (const row of rows) {
      if (seen.has(row.id)) issues.push(issue("error", "studio.contribution_duplicate", `Duplicate ${kind} contribution ID`, `${row.id} is repeated in package ${pkg.manifest.id}.`, pkg.manifest.id, kind as AuditIssue["objectType"], row.id));
      seen.add(row.id);
    }
  }
}

export function coverageSummary(workspace: StudioBrainWorkspace): CoverageSummary {
  let meanings = 0;
  let behaviors = 0;
  let responses = 0;
  let capabilities = 0;
  let regressionCases = 0;
  let scenarios = 0;
  const expectedMeanings = new Set<string>();
  const exact = new Map<string, Set<string>>();

  for (const pkg of workspace.packages) {
    meanings += pkg.contents.meanings.length;
    behaviors += pkg.contents.behaviors.length;
    capabilities += pkg.contents.capabilities.length;
    regressionCases += pkg.contents.regression_cases.length;
    scenarios += pkg.contents.scenarios.length;
    for (const row of pkg.contents.behaviors) responses += row.value.responses.length;
    for (const row of pkg.contents.capability_result_behaviors) responses += row.value.responses.length;
    for (const row of pkg.contents.regression_cases) if (row.value.expectation.meaning) expectedMeanings.add(row.value.expectation.meaning);
    for (const row of pkg.contents.meanings) {
      for (const sample of row.value.samples.map((value) => normalizeSample(value.text)).filter(Boolean)) {
        const ids = exact.get(sample) ?? new Set<string>();
        ids.add(row.value.id);
        exact.set(sample, ids);
      }
    }
  }

  const exactSampleCollisions = [...exact.values()].filter((ids) => ids.size > 1).length;
  return { meanings, behaviors, responses, capabilities, regressionCases, scenarios, meaningsWithRegression: expectedMeanings.size, exactSampleCollisions };
}

export interface LanguageCoverageRow {
  language: string;
  samples: number;
  responseVariants: number;
  regressionTurns: number;
}

export function languageCoverage(workspace: StudioBrainWorkspace): LanguageCoverageRow[] {
  const rows = new Map(workspace.languages.map((language) => [languageKey(language), { language, samples: 0, responseVariants: 0, regressionTurns: 0 }]));
  const countTexts = (texts: Array<{language:string;variants:string[]}>) => { for (const entry of texts) { const row=rows.get(languageKey(entry.language)); if(row) row.responseVariants+=entry.variants.filter((variant)=>variant.trim()!=="").length; } };
  const countResponse = (response: ResponseDefinition) => { countTexts(response.texts); for(const extra of response.extra_messages) countTexts(extra.texts); };
  for(const pkg of workspace.packages) {
    for(const meaning of pkg.contents.meanings) for(const sample of [...meaning.value.patterns, ...meaning.value.samples]) { const row=rows.get(languageKey(sample.language)); if(row&&sample.text.trim()) row.samples+=1; }
    for(const behavior of pkg.contents.behaviors) for(const response of behavior.value.responses) countResponse(response);
    for(const behavior of pkg.contents.fallback_behaviors) for(const response of behavior.value.responses) countResponse(response);
    for(const behavior of pkg.contents.capability_result_behaviors) for(const response of behavior.value.responses) countResponse(response);
    for(const opening of pkg.contents.openings) for(const response of opening.value.responses) countResponse(response);
    for(const test of pkg.contents.regression_cases) { const row=rows.get(languageKey(test.value.language)); if(row&&test.value.input.trim()) row.regressionTurns+=1; }
    for(const scenario of pkg.contents.scenarios) for(const step of scenario.value.steps) if(step.type === "turn") { const row=rows.get(languageKey(step.language)); if(row&&step.say.trim()) row.regressionTurns+=1; }
  }
  return [...rows.values()];
}

function auditResponseLanguages(workspace:StudioBrainWorkspace,response:ResponseDefinition,issues:AuditIssue[],packageId:string):void {
  for(const texts of response.texts) auditLanguage(workspace,texts.language,issues,packageId,"response",response.id,"Response text");
  for(const extra of response.extra_messages) for(const texts of extra.texts) auditLanguage(workspace,texts.language,issues,packageId,"response",response.id,"Extra message");
}

function hasVisibleResponseContent(response:ResponseDefinition):boolean {
  return response.texts.some((texts)=>texts.variants.some((variant)=>variant.trim()!==""))
    || response.assets.some((asset)=>asset.asset_id.trim()!=="")
    || response.links.some((link)=>link.url.trim()!=="");
}

function auditLanguage(workspace:StudioBrainWorkspace,language:string,issues:AuditIssue[],packageId:string,objectType:AuditIssue["objectType"],objectId:string,label:string):void {
  const allowed=new Set(workspace.languages.map(languageKey));
  if(!isWellFormedLanguageTag(language)||!allowed.has(languageKey(language))) issues.push(issue("error","studio.language_not_selected",`${label} language is not selected`,`${language||"(empty)"} is not one of this Project's languages: ${workspace.languages.join(", ")}.`,packageId,objectType,objectId));
}

function severityRank(severity: AuditIssue["severity"]): number {
  return severity === "error" ? 0 : severity === "warning" ? 1 : 2;
}
