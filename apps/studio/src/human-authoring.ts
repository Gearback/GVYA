import { compareUtf8 } from "./canonical-order.js";
import type { Contribution, StudioPackage, StudioBrainWorkspace } from "./types.js";
import { cloneBrainWorkspace } from "./workspace.js";

export type BehaviorDeleteImpactKind = "binding" | "test" | "scenario" | "specialization" | "shared_meaning" | "minimum_response";
interface DeleteImpact {
  packageId: string;
  objectId: string;
  detail: string;
}
export interface BehaviorDeleteImpact extends DeleteImpact { kind: BehaviorDeleteImpactKind; }
export type CapabilityDeleteImpactKind = "binding" | "policy" | "result_handler" | "test" | "scenario" | "specialization";
export interface CapabilityDeleteImpact extends DeleteImpact { kind: CapabilityDeleteImpactKind; }

const AUTHOR_ID_MAX = 256;

export function validateAuthorId(value: string): string | null {
  const id = value.trim();
  if (!id) return "ID cannot be empty.";
  if (id.length > AUTHOR_ID_MAX) return `ID must be at most ${AUTHOR_ID_MAX} characters.`;
  if (/\s/u.test(id)) return "ID cannot contain whitespace.";
  if (/[\u0000-\u001f\u007f]/u.test(id)) return "ID cannot contain control characters.";
  return null;
}

export function validatePackageId(value: string): string | null {
  const invalid = validateAuthorId(value);
  if (invalid) return invalid;
  const id = value.trim();
  if (id === "." || id === ".." || /[\\/]/u.test(id)) return "Package ID cannot contain path separators or be a relative path segment.";
  return null;
}

export function validateAuthorVersion(value: string): string | null {
  const version = value.trim();
  if (!version) return "Version cannot be empty.";
  if (version.length > AUTHOR_ID_MAX) return `Version must be at most ${AUTHOR_ID_MAX} characters.`;
  if (/\s/u.test(version)) return "Version cannot contain whitespace.";
  if (/[\u0000-\u001f\u007f]/u.test(version)) return "Version cannot contain control characters.";
  return null;
}

export function renamePackageAtomic(workspace: StudioBrainWorkspace, previousId: string, nextRaw: string): StudioBrainWorkspace {
  const nextId = nextRaw.trim();
  const invalid = validatePackageId(nextId);
  if (invalid) throw new Error(invalid);
  const draft = cloneBrainWorkspace(workspace);
  const pkg = requirePackage(draft, previousId);
  if (nextId === previousId) return draft;
  if (draft.packages.some((candidate) => candidate !== pkg && candidate.manifest.id === nextId)) throw new Error(`Package ID ${nextId} already exists.`);
  pkg.manifest.id = nextId;
  pkg.path = `packages/${nextId}/package.json`;
  if (draft.selectedPackageId === previousId) draft.selectedPackageId = nextId;
  for (const candidate of draft.packages) {
    for (const dependency of candidate.manifest.dependencies) if (dependency.id === previousId) dependency.id = nextId;
    for (const rows of Object.values(candidate.contents) as Array<Contribution<unknown>[]>) {
      for (const row of rows) if (row.mode !== "add" && row.mode.target_package === previousId) row.mode.target_package = nextId;
    }
  }
  return draft;
}

export function renameCapabilityIdentityAtomic(
  workspace: StudioBrainWorkspace,
  packageId: string,
  previousId: string,
  previousVersion: string,
  nextIdRaw: string,
  nextVersionRaw: string,
): StudioBrainWorkspace {
  const nextId = nextIdRaw.trim();
  const nextVersion = nextVersionRaw.trim();
  const invalidId = validateAuthorId(nextId);
  if (invalidId) throw new Error(invalidId);
  const invalidVersion = validateAuthorVersion(nextVersion);
  if (invalidVersion) throw new Error(invalidVersion);
  const draft = cloneBrainWorkspace(workspace);
  const pkg = requirePackage(draft, packageId);
  const row = pkg.contents.capabilities.find((item) => item.value.contract.id === previousId && item.value.contract.version === previousVersion);
  if (!row) throw new Error(`Capability ${previousId}@${previousVersion} no longer exists.`);
  if (nextId === previousId && nextVersion === previousVersion) return draft;
  if (nextId !== previousId && draft.packages.some((candidate) => candidate.contents.capabilities.some((item) => item !== row && item.value.contract.id === nextId))) {
    throw new Error(`Capability ID ${nextId} already exists in this project.`);
  }

  const previousContributionId = row.id;
  const contributionTracksCapabilityId = previousContributionId === previousId;
  if (contributionTracksCapabilityId && nextId !== previousId) {
    if (pkg.contents.capabilities.some((item) => item !== row && item.id === nextId)) throw new Error(`Capability contribution ID ${nextId} already exists in package ${packageId}.`);
    row.id = nextId;
  }
  row.value.contract.id = nextId;
  row.value.contract.version = nextVersion;

  for (const packageRow of draft.packages) {
    if (contributionTracksCapabilityId && nextId !== previousId) rewriteReplacementTargets(packageRow, packageId, previousContributionId, nextId, "capabilities");
    for (const binding of packageRow.contents.capability_bindings) if (binding.value.capability === previousId) binding.value.capability = nextId;
    for (const policy of packageRow.contents.capability_policies) if (policy.value.capability === previousId) policy.value.capability = nextId;
    for (const handler of packageRow.contents.capability_result_behaviors) {
      if (handler.value.capability !== previousId) continue;
      handler.value.capability = nextId;
      if (handler.value.capability_version === previousVersion) handler.value.capability_version = nextVersion;
    }
    for (const test of packageRow.contents.regression_cases) {
      rewriteCapabilityContext(test.value.context, previousId, previousVersion, nextId, nextVersion);
      rewriteCapabilityExpectation(test.value.expectation, previousId, previousVersion, nextId, nextVersion);
    }
    for (const scenario of packageRow.contents.scenarios) {
      rewriteCapabilityContext(scenario.value.context, previousId, previousVersion, nextId, nextVersion);
      for (const step of scenario.value.steps) {
        if (step.context) rewriteCapabilityContext(step.context, previousId, previousVersion, nextId, nextVersion);
        rewriteCapabilityExpectation(step.expectation, previousId, previousVersion, nextId, nextVersion);
        if ((step.type === "capability_result" || step.type === "confirm") && step.proposal_capability === previousId) step.proposal_capability = nextId;
      }
    }
  }
  return draft;
}

export function capabilityDeleteImpacts(workspace: StudioBrainWorkspace, packageId: string, capabilityId: string): CapabilityDeleteImpact[] {
  const pkg = workspace.packages.find((row) => row.manifest.id === packageId);
  if (!pkg?.contents.capabilities.some((row) => row.value.contract.id === capabilityId)) return [];
  const impacts: CapabilityDeleteImpact[] = [];
  for (const packageRow of workspace.packages) {
    if (packageRow.manifest.id !== packageId) {
      for (const binding of packageRow.contents.capability_bindings) if (binding.value.capability === capabilityId) impacts.push({ kind: "binding", packageId: packageRow.manifest.id, objectId: binding.value.id, detail: `Capability binding ${binding.value.id} references ${capabilityId}.` });
      for (const policy of packageRow.contents.capability_policies) if (policy.value.capability === capabilityId) impacts.push({ kind: "policy", packageId: packageRow.manifest.id, objectId: policy.value.id, detail: `Capability policy ${policy.value.id} references ${capabilityId}.` });
    }
    for (const handler of packageRow.contents.capability_result_behaviors) if (handler.value.capability === capabilityId) impacts.push({ kind: "result_handler", packageId: packageRow.manifest.id, objectId: handler.value.id, detail: `Capability-result behavior ${handler.value.id} handles ${capabilityId}.` });
    for (const test of packageRow.contents.regression_cases) {
      if (capabilityContextReferences(test.value.context, capabilityId) || capabilityExpectationReferences(test.value.expectation, capabilityId)) impacts.push({ kind: "test", packageId: packageRow.manifest.id, objectId: test.value.id, detail: `Regression case ${test.value.id} references ${capabilityId}.` });
    }
    for (const scenario of packageRow.contents.scenarios) {
      const referenced = capabilityContextReferences(scenario.value.context, capabilityId) || scenario.value.steps.some((step) => (step.context !== null && capabilityContextReferences(step.context, capabilityId)) || capabilityExpectationReferences(step.expectation, capabilityId) || ((step.type === "capability_result" || step.type === "confirm") && step.proposal_capability === capabilityId));
      if (referenced) impacts.push({ kind: "scenario", packageId: packageRow.manifest.id, objectId: scenario.value.id, detail: `Scenario ${scenario.value.id} references ${capabilityId}.` });
    }
    for (const row of packageRow.contents.capabilities) if (row.mode !== "add" && row.mode.target_package === packageId && row.mode.target_id === capabilityId) impacts.push({ kind: "specialization", packageId: packageRow.manifest.id, objectId: row.id, detail: `Capability contribution ${row.id} replaces ${capabilityId}.` });
  }
  return dedupeImpacts(impacts);
}

export function deleteCapabilityAtomic(workspace: StudioBrainWorkspace, packageId: string, capabilityId: string): StudioBrainWorkspace {
  const impacts = capabilityDeleteImpacts(workspace, packageId, capabilityId);
  if (impacts.length) throw new Error(`Cannot delete capability while ${impacts.length} dependent reference${impacts.length === 1 ? "" : "s"} remain.`);
  const draft = cloneBrainWorkspace(workspace);
  const pkg = requirePackage(draft, packageId);
  const index = pkg.contents.capabilities.findIndex((row) => row.value.contract.id === capabilityId);
  if (index < 0) throw new Error(`Capability ${capabilityId} no longer exists.`);
  pkg.contents.capabilities.splice(index, 1);
  pkg.contents.capability_bindings = pkg.contents.capability_bindings.filter((row) => row.value.capability !== capabilityId);
  pkg.contents.capability_policies = pkg.contents.capability_policies.filter((row) => row.value.capability !== capabilityId);
  return draft;
}

export function renameBehaviorAtomic(workspace: StudioBrainWorkspace, packageId: string, previousId: string, nextRaw: string): StudioBrainWorkspace {
  const nextId = nextRaw.trim();
  const invalid = validateAuthorId(nextId);
  if (invalid) throw new Error(invalid);
  const draft = cloneBrainWorkspace(workspace);
  const pkg = requirePackage(draft, packageId);
  const row = pkg.contents.behaviors.find((item) => item.value.id === previousId);
  if (!row) throw new Error(`Behavior ${previousId} no longer exists.`);
  if (nextId === previousId) return draft;
  if (pkg.contents.behaviors.some((item) => item !== row && (item.id === nextId || item.value.id === nextId))) throw new Error(`Behavior ID ${nextId} already exists in package ${packageId}.`);
  row.id = nextId;
  row.value.id = nextId;
  for (const packageRow of draft.packages) {
    if (packageCanSee(draft, packageRow.manifest.id, packageId)) for (const binding of packageRow.contents.capability_bindings) if (binding.value.trigger.behavior === previousId) binding.value.trigger.behavior = nextId;
    rewriteReplacementTargets(packageRow, packageId, previousId, nextId, "behaviors");
  }
  return draft;
}

export function renameMeaningAtomic(workspace: StudioBrainWorkspace, packageId: string, previousId: string, nextRaw: string): StudioBrainWorkspace {
  const nextId = nextRaw.trim();
  const invalid = validateAuthorId(nextId);
  if (invalid) throw new Error(invalid);
  const draft = cloneBrainWorkspace(workspace);
  const pkg = requirePackage(draft, packageId);
  const row = pkg.contents.meanings.find((item) => item.value.id === previousId);
  if (!row) throw new Error(`Meaning ${previousId} no longer exists.`);
  if (nextId === previousId) return draft;
  if (pkg.contents.meanings.some((item) => item !== row && (item.id === nextId || item.value.id === nextId))) throw new Error(`Meaning ID ${nextId} already exists in package ${packageId}.`);
  row.id = nextId;
  row.value.id = nextId;
  for (const packageRow of draft.packages) {
    if (packageCanSee(draft, packageRow.manifest.id, packageId)) {
      for (const behavior of packageRow.contents.behaviors) if (behavior.value.meaning === previousId) behavior.value.meaning = nextId;
      for (const binding of packageRow.contents.capability_bindings) if (binding.value.trigger.meaning === previousId) binding.value.trigger.meaning = nextId;
      for (const test of packageRow.contents.regression_cases) rewriteExpectationMeaning(test.value.expectation, previousId, nextId);
      for (const scenario of packageRow.contents.scenarios) for (const step of scenario.value.steps) rewriteExpectationMeaning(step.expectation, previousId, nextId);
    }
    rewriteReplacementTargets(packageRow, packageId, previousId, nextId, "meanings");
  }
  return draft;
}

export function renameResponseAtomic(workspace: StudioBrainWorkspace, packageId: string, behaviorId: string, previousId: string, nextRaw: string): StudioBrainWorkspace {
  const nextId = nextRaw.trim();
  const invalid = validateAuthorId(nextId);
  if (invalid) throw new Error(invalid);
  const draft = cloneBrainWorkspace(workspace);
  const pkg = requirePackage(draft, packageId);
  const behavior = pkg.contents.behaviors.find((row) => row.value.id === behaviorId)?.value;
  if (!behavior) throw new Error(`Behavior ${behaviorId} no longer exists.`);
  const response = behavior.responses.find((row) => row.id === previousId);
  if (!response) throw new Error(`Response ${previousId} no longer exists.`);
  if (nextId === previousId) return draft;
  if (pkg.contents.behaviors.some((row) => row.value.responses.some((candidate) => candidate !== response && candidate.id === nextId))) throw new Error(`Response ID ${nextId} already exists in package ${packageId}.`);
  response.id = nextId;
  for (const packageRow of draft.packages) {
    if (!packageCanSee(draft, packageRow.manifest.id, packageId)) continue;
    for (const binding of packageRow.contents.capability_bindings) if (binding.value.trigger.response === previousId) binding.value.trigger.response = nextId;
    for (const test of packageRow.contents.regression_cases) rewriteExpectationResponse(test.value.expectation, previousId, nextId);
    for (const scenario of packageRow.contents.scenarios) for (const step of scenario.value.steps) rewriteExpectationResponse(step.expectation, previousId, nextId);
  }
  return draft;
}

export function behaviorDeleteImpacts(workspace: StudioBrainWorkspace, packageId: string, behaviorId: string): BehaviorDeleteImpact[] {
  const pkg = workspace.packages.find((row) => row.manifest.id === packageId);
  const behavior = pkg?.contents.behaviors.find((row) => row.value.id === behaviorId)?.value;
  if (!pkg || !behavior) return [];
  const responseIds = new Set(behavior.responses.map((row) => row.id));
  const impacts: BehaviorDeleteImpact[] = [];
  for (const packageRow of workspace.packages) {
    const canSee = packageCanSee(workspace, packageRow.manifest.id, packageId);
    if (canSee) for (const row of packageRow.contents.behaviors) if (!(packageRow.manifest.id === packageId && row.value.id === behaviorId) && row.value.meaning === behavior.meaning) impacts.push({ kind: "shared_meaning", packageId: packageRow.manifest.id, objectId: row.value.id, detail: `Meaning ${behavior.meaning} is also used by behavior ${row.value.id}.` });
    if (canSee) for (const binding of packageRow.contents.capability_bindings) {
      const trigger = binding.value.trigger;
      if (trigger.behavior === behaviorId || trigger.meaning === behavior.meaning || responseIds.has(trigger.response)) impacts.push({ kind: "binding", packageId: packageRow.manifest.id, objectId: binding.value.id, detail: `Capability binding ${binding.value.id} references this behavior, meaning, or one of its responses.` });
    }
    if (canSee) for (const test of packageRow.contents.regression_cases) {
      if (expectationReferences(test.value.expectation, behavior.meaning, responseIds)) impacts.push({ kind: "test", packageId: packageRow.manifest.id, objectId: test.value.id, detail: `Regression case ${test.value.id} expects this meaning or one of its responses.` });
    }
    if (canSee) for (const scenario of packageRow.contents.scenarios) {
      if (scenario.value.steps.some((step) => expectationReferences(step.expectation, behavior.meaning, responseIds))) impacts.push({ kind: "scenario", packageId: packageRow.manifest.id, objectId: scenario.value.id, detail: `Scenario ${scenario.value.id} expects this meaning or one of its responses.` });
    }
    collectReplacementImpacts(packageRow, packageId, behaviorId, "behaviors", impacts);
    collectReplacementImpacts(packageRow, packageId, behavior.meaning, "meanings", impacts);
  }
  return dedupeImpacts(impacts);
}

export function deleteBehaviorPair(workspace: StudioBrainWorkspace, packageId: string, behaviorId: string): StudioBrainWorkspace {
  const impacts = behaviorDeleteImpacts(workspace, packageId, behaviorId);
  if (impacts.length) throw new Error(`Cannot delete behavior while ${impacts.length} dependent reference${impacts.length === 1 ? "" : "s"} remain.`);
  const draft = cloneBrainWorkspace(workspace);
  const pkg = requirePackage(draft, packageId);
  const index = pkg.contents.behaviors.findIndex((row) => row.value.id === behaviorId);
  if (index < 0) throw new Error(`Behavior ${behaviorId} no longer exists.`);
  const meaningId = pkg.contents.behaviors[index]!.value.meaning;
  pkg.contents.behaviors.splice(index, 1);
  const stillUsed = pkg.contents.behaviors.some((row) => row.value.meaning === meaningId);
  if (!stillUsed) {
    const meaningIndex = pkg.contents.meanings.findIndex((row) => row.value.id === meaningId);
    if (meaningIndex >= 0) pkg.contents.meanings.splice(meaningIndex, 1);
  }
  return draft;
}

export function responseDeleteImpacts(workspace: StudioBrainWorkspace, packageId: string, behaviorId: string, responseId: string): BehaviorDeleteImpact[] {
  const pkg = workspace.packages.find((row) => row.manifest.id === packageId);
  const behavior = pkg?.contents.behaviors.find((row) => row.value.id === behaviorId)?.value;
  if (!pkg || !behavior) return [];
  const impacts: BehaviorDeleteImpact[] = [];
  if (behavior.responses.length <= 1) impacts.push({ kind: "minimum_response", packageId, objectId: behaviorId, detail: "A behavior must keep at least one authored response in the human editor." });
  for (const packageRow of workspace.packages) {
    if (!packageCanSee(workspace, packageRow.manifest.id, packageId)) continue;
    for (const binding of packageRow.contents.capability_bindings) if (binding.value.trigger.response === responseId) impacts.push({ kind: "binding", packageId: packageRow.manifest.id, objectId: binding.value.id, detail: `Capability binding ${binding.value.id} references response ${responseId}.` });
    for (const test of packageRow.contents.regression_cases) if (test.value.expectation.response_ids.includes(responseId) || test.value.expectation.forbidden_response_ids.includes(responseId)) impacts.push({ kind: "test", packageId: packageRow.manifest.id, objectId: test.value.id, detail: `Regression case ${test.value.id} references response ${responseId}.` });
    for (const scenario of packageRow.contents.scenarios) if (scenario.value.steps.some((step) => step.expectation.response_ids.includes(responseId) || step.expectation.forbidden_response_ids.includes(responseId))) impacts.push({ kind: "scenario", packageId: packageRow.manifest.id, objectId: scenario.value.id, detail: `Scenario ${scenario.value.id} references response ${responseId}.` });
  }
  return dedupeImpacts(impacts);
}

export function deleteResponseAtomic(workspace: StudioBrainWorkspace, packageId: string, behaviorId: string, responseId: string): StudioBrainWorkspace {
  const impacts = responseDeleteImpacts(workspace, packageId, behaviorId, responseId);
  if (impacts.length) throw new Error(`Cannot delete response while ${impacts.length} blocker${impacts.length === 1 ? "" : "s"} remain.`);
  const draft = cloneBrainWorkspace(workspace);
  const pkg = requirePackage(draft, packageId);
  const behavior = pkg.contents.behaviors.find((row) => row.value.id === behaviorId)?.value;
  if (!behavior) throw new Error(`Behavior ${behaviorId} no longer exists.`);
  const index = behavior.responses.findIndex((row) => row.id === responseId);
  if (index < 0) throw new Error(`Response ${responseId} no longer exists.`);
  behavior.responses.splice(index, 1);
  return draft;
}

export type BehaviorGroupMode = "flat" | "topic" | "followup";
export interface BehaviorGroup<T> { key: string; label: string; rows: T[]; }
export function groupBehaviorRows<T extends { value: { topic: string; followup_scope: string; responses: Array<{ opens_followup: { id: string } | null }> } }>(rows: T[], mode: BehaviorGroupMode): BehaviorGroup<T>[] {
  if (mode === "flat") return [{ key: "all", label: "All behaviors", rows }];
  const groups = new Map<string, T[]>();
  for (const row of rows) {
    const raw = mode === "topic"
      ? row.value.topic.trim() || "No topic"
      : row.value.followup_scope.trim() || row.value.responses.find((response) => response.opens_followup?.id)?.opens_followup?.id || "No follow-up";
    const bucket = groups.get(raw) ?? [];
    bucket.push(row);
    groups.set(raw, bucket);
  }
  return [...groups.entries()].sort(([a], [b]) => compareUtf8(a, b)).map(([label, groupRows]) => ({ key: label, label, rows: groupRows }));
}

function requirePackage(workspace: StudioBrainWorkspace, packageId: string): StudioPackage {
  const pkg = workspace.packages.find((row) => row.manifest.id === packageId);
  if (!pkg) throw new Error(`Package ${packageId} no longer exists.`);
  return pkg;
}

function rewriteExpectationMeaning(expectation: { meaning: string; forbidden_meanings: string[] }, previousId: string, nextId: string): void {
  if (expectation.meaning === previousId) expectation.meaning = nextId;
  expectation.forbidden_meanings = expectation.forbidden_meanings.map((id) => id === previousId ? nextId : id);
}

function rewriteExpectationResponse(expectation: { response_ids: string[]; forbidden_response_ids: string[] }, previousId: string, nextId: string): void {
  expectation.response_ids = expectation.response_ids.map((id) => id === previousId ? nextId : id);
  expectation.forbidden_response_ids = expectation.forbidden_response_ids.map((id) => id === previousId ? nextId : id);
}

function rewriteCapabilityExpectation(
  expectation: { capabilities: Array<{ id: string; version: string }>; proposal_receipts: Array<{ id: string; version: string }>; forbidden_capabilities: string[] },
  previousId: string,
  previousVersion: string,
  nextId: string,
  nextVersion: string,
): void {
  for (const capability of expectation.capabilities) {
    if (capability.id !== previousId) continue;
    capability.id = nextId;
    if (capability.version === previousVersion) capability.version = nextVersion;
  }
  for (const receipt of expectation.proposal_receipts) {
    if (receipt.id !== previousId) continue;
    receipt.id = nextId;
    if (receipt.version === previousVersion) receipt.version = nextVersion;
  }
  expectation.forbidden_capabilities = expectation.forbidden_capabilities.map((id) => id === previousId ? nextId : id);
}

function rewriteCapabilityContext(
  context: { available_capabilities: Array<{ id: string; version: string }> },
  previousId: string,
  previousVersion: string,
  nextId: string,
  nextVersion: string,
): void {
  for (const capability of context.available_capabilities) {
    if (capability.id !== previousId) continue;
    capability.id = nextId;
    if (capability.version === previousVersion) capability.version = nextVersion;
  }
}

function capabilityExpectationReferences(expectation: { capabilities: Array<{ id: string }>; proposal_receipts: Array<{ id: string }>; forbidden_capabilities: string[] }, capabilityId: string): boolean {
  return expectation.capabilities.some((row) => row.id === capabilityId)
    || expectation.proposal_receipts.some((row) => row.id === capabilityId)
    || expectation.forbidden_capabilities.includes(capabilityId);
}

function capabilityContextReferences(context: { available_capabilities: Array<{ id: string }> }, capabilityId: string): boolean {
  return context.available_capabilities.some((row) => row.id === capabilityId);
}

function packageCanSee(workspace: StudioBrainWorkspace, candidateId: string, targetId: string): boolean {
  if (candidateId === targetId) return true;
  const visited = new Set<string>();
  const visit = (id: string): boolean => {
    if (id === targetId) return true;
    if (visited.has(id)) return false;
    visited.add(id);
    const pkg = workspace.packages.find((row) => row.manifest.id === id);
    return Boolean(pkg?.manifest.dependencies.some((dependency) => visit(dependency.id)));
  };
  return visit(candidateId);
}

function expectationReferences(expectation: { meaning: string; forbidden_meanings: string[]; response_ids: string[]; forbidden_response_ids: string[] }, meaningId: string, responseIds: Set<string>): boolean {
  return expectation.meaning === meaningId || expectation.forbidden_meanings.includes(meaningId)
    || expectation.response_ids.some((id) => responseIds.has(id)) || expectation.forbidden_response_ids.some((id) => responseIds.has(id));
}

function rewriteReplacementTargets(pkg: StudioPackage, targetPackageId: string, previousId: string, nextId: string, namespace: keyof StudioPackage["contents"]): void {
  const rows = pkg.contents[namespace] as unknown as Contribution<unknown>[];
  for (const row of rows) if (row.mode !== "add" && row.mode.target_package === targetPackageId && row.mode.target_id === previousId) row.mode.target_id = nextId;
}

function collectReplacementImpacts(pkg: StudioPackage, targetPackageId: string, targetId: string, namespace: keyof StudioPackage["contents"], impacts: BehaviorDeleteImpact[]): void {
  const rows = pkg.contents[namespace] as unknown as Contribution<unknown>[];
  for (const row of rows) if (row.mode !== "add" && row.mode.target_package === targetPackageId && row.mode.target_id === targetId) impacts.push({ kind: "specialization", packageId: pkg.manifest.id, objectId: row.id, detail: `${String(namespace)} contribution ${row.id} replaces this object.` });
}

function dedupeImpacts<T extends DeleteImpact & { kind: string }>(rows: T[]): T[] {
  const seen = new Set<string>();
  return rows.filter((row) => { const key = `${row.kind}\u0000${row.packageId}\u0000${row.objectId}\u0000${row.detail}`; if (seen.has(key)) return false; seen.add(key); return true; });
}
