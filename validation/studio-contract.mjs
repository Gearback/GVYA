import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { readContentSnapshot } from "../apps/studio/content-host.mjs";
import { decodeContent, encodeContent } from "../apps/studio/dist/studio-content.js";
import { createStarterBrainWorkspace, createPackage, contribution, createMeaning, createFallbackBehavior, fallbackContribution, createRegressionCase, createScenario, packageSnapshotDocument, packageSourceFiles, projectSourceDocument } from "../apps/studio/dist/workspace.js";
import { auditWorkspace, coverageSummary, languageCoverage } from "../apps/studio/dist/audit.js";
import { loadSourceTree, brainWorkspaceFromText } from "../apps/studio/dist/source-io.js";

const DEFAULT_FALLBACK_PACKAGE_IDS = { formal: "gvya.fallback.formal", informal: "gvya.fallback.informal" };
const DEFAULT_SMALLTALK_PACKAGE_IDS = ["core.smalltalk.formal", "core.smalltalk.informal"];
const DEFAULT_STANDARD_PACKAGE_ID = DEFAULT_SMALLTALK_PACKAGE_IDS[0];
const DEFAULT_STANDARD_BEHAVIOR_ID = `${DEFAULT_STANDARD_PACKAGE_ID}.hello.behavior`;
const DEFAULT_SHARED_LANGUAGES = ["en-US", "fa-IR"];
const defaultContent = decodeContent((await readContentSnapshot(fileURLToPath(new URL("../content", import.meta.url)))).entries);
const createDefaultStudio = () => {
  const studio = structuredClone(defaultContent.workspace);
  const project = studio.projects.find((row) => row.id === "gvya-project");
  const bot = project?.bots.find((row) => row.id === "gvya-bot");
  assert.ok(project && bot, "physical content must include the starter Project and Bot");
  studio.projects = [project];
  project.bots = [bot];
  project.packages = [];
  bot.fallback_package_id = null;
  studio.selectedProjectId = project.id;
  studio.selectedBotId = bot.id;
  studio.selectedPackageScope = "bot";
  studio.selectedPackageId = bot.package.manifest.id;
  return studioModel.setBotPackages(studio, []);
};

globalThis.document = { baseURI: new URL("../apps/studio/public/", import.meta.url).href };
globalThis.fetch = async (input) => {
  const url = input instanceof URL
    ? new URL(input.href)
    : new URL(typeof input === "string" ? input : input.url);
  url.search = "";
  return new Response(await readFile(url), { status: 200 });
};

const workspace = createStarterBrainWorkspace();
const project = projectSourceDocument(workspace);
const pkgFiles = packageSourceFiles(workspace.packages[0]);
const pkg = pkgFiles[0].json;
const pkgSnapshot = packageSnapshotDocument(workspace.packages[0]);

assert.equal(project.format, "gvya.source.project");
assert.equal(project.version, 1);
assert.deepEqual(project.languages, ["en-US"]);
assert.deepEqual(project.enabled_languages, ["en-US"]);
assert.equal(project.default_language, "en-US");
assert.deepEqual(project.language_profiles, []);
assert.deepEqual(project.matcher_profiles, []);
assert.deepEqual(project.packages, ["packages/conversation/package.json"]);
assert.equal(pkg.format, "gvya.source.package");
assert.equal(pkg.version, 1);

const canonicalNamespaces = [
  "meanings", "behaviors", "capability_result_behaviors", "openings", "fallback_behaviors",
  "style_lexicons", "capabilities", "capability_bindings", "capability_policies", "capability_configs", "types",
  "assets", "regression_cases", "scenarios",
].sort();
assert.ok(Object.keys(pkg.fragments).every((namespace) => canonicalNamespaces.includes(namespace)));
assert.ok(pkg.fragments.meanings?.length > 0 && pkg.fragments.behaviors?.length > 0);

const behavior = workspace.packages[0].contents.behaviors[0].value;
behavior.followup_scope = "confirm.help";
behavior.requires_values = [{ namespace: "context", path: "device.ready", value: true }];
behavior.forbidden_values = [{ namespace: "author", path: "device.disabled", value: true }];
const response = workspace.packages[0].contents.behaviors[0].value.responses[0];
response.opens_followup = { id: "followup.help", ttl: 3, refresh_if_same: true };
response.assets = [{ asset_id: "asset.avatar", alt_text: "Assistant avatar" }];
const roundTripShape = packageSnapshotDocument(workspace.packages[0]);
const serializedBehavior = roundTripShape.contents.behaviors[0].value;
assert.equal(serializedBehavior.followup_scope, "confirm.help");
assert.deepEqual(serializedBehavior.requires_values, [{ namespace: "context", path: "device.ready", value: true }]);
assert.deepEqual(serializedBehavior.forbidden_values, [{ namespace: "author", path: "device.disabled", value: true }]);
const serializedResponse = roundTripShape.contents.behaviors[0].value.responses[0];
assert.deepEqual(serializedResponse.opens_followup, { id: "followup.help", ttl: 3, refresh_if_same: true });
assert.deepEqual(serializedResponse.assets, [{ asset_id: "asset.avatar", alt_text: "Assistant avatar" }]);

const receiptAuditWorkspace = createStarterBrainWorkspace();
const receiptRegression = createRegressionCase("proposal.receipt.unknown");
receiptRegression.input = "try it";
receiptRegression.expectation.proposal_receipts = [{ id: "host.unknown", version: "1", arguments: null, outcome: "rejected", reason_code: "policy.denied" }];
receiptAuditWorkspace.packages[0].contents.regression_cases.push(contribution(receiptRegression.id, receiptRegression));
const receiptScenario = createScenario("proposal.receipt.scenario");
receiptScenario.steps[0].say = "try it";
receiptScenario.steps[0].expectation.proposal_receipts = [{ id: "host.unknown", version: "1", arguments: null, outcome: "needs_confirmation", reason_code: "confirm.required" }];
receiptAuditWorkspace.packages[0].contents.scenarios.push(contribution(receiptScenario.id, receiptScenario));
const receiptAuditIssues = auditWorkspace(receiptAuditWorkspace);
assert.ok(receiptAuditIssues.some((row) => row.code === "studio.regression_proposal_receipt_capability_missing"));
assert.ok(receiptAuditIssues.some((row) => row.code === "studio.scenario_proposal_receipt_capability_missing"));
const openingResponse = structuredClone(response);
openingResponse.id = "response.opening";
openingResponse.kind = "opening";
openingResponse.texts = [{ language: "en-US", variants: ["Welcome"] }];
workspace.packages[0].contents.openings.push(contribution("opening.default", { id: "opening.default", topic: "", topic_ttl: null, responses: [openingResponse] }));
const serializedOpening = packageSnapshotDocument(workspace.packages[0]).contents.openings[0].value;
assert.equal(serializedOpening.id, "opening.default");
assert.deepEqual(serializedOpening.responses[0].texts, [{ language: "en-US", variants: ["Welcome"] }]);
const fallbackPackage = createPackage("fallback.personality", "Personality-aware unresolved behavior", "fallback");
const angryFallback = createFallbackBehavior("fallback.angry");
angryFallback.priority = 100;
angryFallback.conditions = [{ namespace: "author", path: "mood.anger", op: "greater_or_equal", value: 70, hasValue: true }];
angryFallback.responses[0].kind = "fallback";
angryFallback.responses[0].texts = [{ language: "en-US", variants: ["What now?"] }];
fallbackPackage.contents.fallback_behaviors.push(fallbackContribution(angryFallback.id, angryFallback));
const fallbackDoc = packageSnapshotDocument(fallbackPackage);
assert.equal(fallbackDoc.manifest.kind, "fallback");
assert.equal(fallbackDoc.contents.fallback_behaviors.length, 1);
assert.equal(fallbackDoc.contents.fallback_behaviors[0].exported, false);
assert.equal(fallbackDoc.contents.fallback_behaviors[0].mode, "add");
assert.equal(fallbackDoc.contents.fallback_behaviors[0].value.priority, 100);

const second = createMeaning("greeting.other");
second.samples = [{ language: "en-US", text: "  HELLO  " }];
workspace.packages[0].contents.meanings.push(contribution(second.id, second));
const issues = auditWorkspace(workspace);
assert.ok(issues.some((issue) => issue.code === "studio.exact_sample_collision"));
const coverage = coverageSummary(workspace);
assert.equal(coverage.exactSampleCollisions, 1);
assert.deepEqual(languageCoverage(workspace), [{ language: "en-US", samples: 4, responseVariants: 3, regressionTurns: 1 }]);

const eligibilityConflictWorkspace = createStarterBrainWorkspace();
const eligibilityConflictBehavior = eligibilityConflictWorkspace.packages[0].contents.behaviors[0].value;
eligibilityConflictBehavior.requires_values = [{ namespace: "context", path: "device.ready", value: true }];
eligibilityConflictBehavior.forbidden_values = [{ namespace: "context", path: "device.ready", value: true }];
assert.ok(auditWorkspace(eligibilityConflictWorkspace).some((issue) => issue.code === "studio.behavior_requirement_conflict" && issue.severity === "error"));

const missingEvidenceWorkspace = createStarterBrainWorkspace();
missingEvidenceWorkspace.packages[0].contents.meanings[0].value.samples = [{ language: "en-US", text: "" }];
assert.ok(auditWorkspace(missingEvidenceWorkspace).some((issue) => issue.code === "studio.meaning_no_positive_evidence" && issue.severity === "error"));
const patternOnlyWorkspace = createStarterBrainWorkspace();
patternOnlyWorkspace.packages[0].contents.meanings[0].value.samples = [];
patternOnlyWorkspace.packages[0].contents.meanings[0].value.patterns = [{ language: "en-US", text: "HELLO ^" }];
assert.equal(auditWorkspace(patternOnlyWorkspace).some((issue) => issue.code === "studio.meaning_no_positive_evidence"), false);

const missingCollectionPromptWorkspace = createStarterBrainWorkspace();
missingCollectionPromptWorkspace.packages[0].contents.meanings[0].value.slots = [{ name: "count", type: "number", entity_kind: "", reference_kind: "", required: true, elicitation: [] }];
missingCollectionPromptWorkspace.packages[0].contents.meanings[0].value.references = [{ kind: "person", required: true, elicitation: [] }];
const missingCollectionPromptIssues = auditWorkspace(missingCollectionPromptWorkspace);
assert.ok(missingCollectionPromptIssues.some((issue) => issue.code === "studio.required_slot_elicitation_missing" && issue.severity === "error"));
assert.ok(missingCollectionPromptIssues.some((issue) => issue.code === "studio.required_reference_elicitation_missing" && issue.severity === "error"));
missingCollectionPromptWorkspace.packages[0].contents.meanings[0].value.slots[0].elicitation = [{ language: "en-US", text: "How many?" }];
missingCollectionPromptWorkspace.packages[0].contents.meanings[0].value.references[0].elicitation = [{ language: "en-US", text: "Who?" }];
const authoredCollectionPromptIssues = auditWorkspace(missingCollectionPromptWorkspace);
assert.equal(authoredCollectionPromptIssues.some((issue) => issue.code === "studio.required_slot_elicitation_missing"), false);
assert.equal(authoredCollectionPromptIssues.some((issue) => issue.code === "studio.required_reference_elicitation_missing"), false);

const missingResponseWorkspace = createStarterBrainWorkspace();
missingResponseWorkspace.packages[0].contents.behaviors[0].value.responses = [];
assert.ok(auditWorkspace(missingResponseWorkspace).some((issue) => issue.code === "studio.behavior_no_responses" && issue.severity === "error"));
const emptyResponseWorkspace = createStarterBrainWorkspace();
emptyResponseWorkspace.packages[0].contents.behaviors[0].value.responses[0].texts = [{ language: "en-US", variants: [""] }];
emptyResponseWorkspace.packages[0].contents.behaviors[0].value.responses[0].links = [{ label: "", url: "" }];
assert.ok(auditWorkspace(emptyResponseWorkspace).some((issue) => issue.code === "studio.response_empty" && issue.severity === "error"));

const persisted = brainWorkspaceFromText(JSON.stringify(createStarterBrainWorkspace()));
const persistedReceiptWorkspace = createStarterBrainWorkspace();
const persistedReceiptScenario = createScenario("persisted.proposal.receipt");
persistedReceiptScenario.steps[0].say = "try it";
persistedReceiptScenario.steps[0].expectation.proposal_receipts = [{ id: "host.echo", version: "1", arguments: { value: 1 }, outcome: "rejected", reason_code: "policy.denied" }];
persistedReceiptWorkspace.packages[0].contents.scenarios.push(contribution(persistedReceiptScenario.id, persistedReceiptScenario));
const persistedReceipt = brainWorkspaceFromText(JSON.stringify(persistedReceiptWorkspace));
assert.deepEqual(persistedReceipt.packages[0].contents.scenarios[0].value.steps[0].expectation.proposal_receipts, [{ id: "host.echo", version: "1", arguments: { value: 1 }, outcome: "rejected", reason_code: "policy.denied" }]);
assert.equal(persisted.format, "gvya.studio.brain-view");
assert.equal(persisted.packages.length, 1);
assert.equal(persisted.default_language, "en-US");
assert.deepEqual(persisted.enabled_languages, ["en-US"]);
assert.equal(persisted.authoring_language, "en-US");
const invalidDefaultWorkspace = createStarterBrainWorkspace();
invalidDefaultWorkspace.default_language = "fa";
assert.ok(auditWorkspace(invalidDefaultWorkspace).some((issue) => issue.code === "studio.default_language" && issue.severity === "error"));
const malformedWorkspace = JSON.parse(JSON.stringify(createStarterBrainWorkspace()));
malformedWorkspace.unexpected = true;
assert.throws(() => brainWorkspaceFromText(JSON.stringify(malformedWorkspace)), /unexpected|unknown/i);
const malformedConfig = JSON.parse(JSON.stringify(createStarterBrainWorkspace()));
malformedConfig.semantic.candidate_limit = 999999;
assert.throws(() => brainWorkspaceFromText(JSON.stringify(malformedConfig)), /candidate_limit|semantic/i);
const tooSmallCandidateLimit = JSON.parse(JSON.stringify(createStarterBrainWorkspace()));
tooSmallCandidateLimit.semantic.candidate_limit = 1;
assert.throws(() => brainWorkspaceFromText(JSON.stringify(tooSmallCandidateLimit)), /candidate_limit|semantic/i);
const lowerBoundaryCandidateLimit = JSON.parse(JSON.stringify(createStarterBrainWorkspace()));
lowerBoundaryCandidateLimit.semantic.candidate_limit = 2;
assert.equal(brainWorkspaceFromText(JSON.stringify(lowerBoundaryCandidateLimit)).semantic.candidate_limit, 2);
const upperBoundaryCandidateLimit = JSON.parse(JSON.stringify(createStarterBrainWorkspace()));
upperBoundaryCandidateLimit.semantic.candidate_limit = 256;
assert.equal(brainWorkspaceFromText(JSON.stringify(upperBoundaryCandidateLimit)).semantic.candidate_limit, 256);


const fakeFile = (name, text, webkitRelativePath = name, declaredSize = Buffer.byteLength(text)) => ({
  name,
  webkitRelativePath,
  size: declaredSize,
  async text() { return text; },
  async arrayBuffer() { return new TextEncoder().encode(text).buffer; },
});
const sourceWorkspace = createStarterBrainWorkspace();
const sourceBehavior = sourceWorkspace.packages[0].contents.behaviors[0].value;
sourceBehavior.followup_scope = "confirm.help";
sourceBehavior.requires_values = [{ namespace: "context", path: "device.ready", value: true }];
sourceBehavior.forbidden_values = [{ namespace: "author", path: "device.disabled", value: true }];
sourceWorkspace.packages.push(structuredClone(fallbackPackage));
const sourceProjectText = JSON.stringify(projectSourceDocument(sourceWorkspace));
const sourceFiles = [
  fakeFile("gvya.project.json", sourceProjectText),
  ...sourceWorkspace.packages.flatMap((pkg) => packageSourceFiles(pkg).map((file) =>
    fakeFile(file.path.split("/").at(-1), JSON.stringify(file.json), file.path))),
];
const importedSource = await loadSourceTree(sourceFiles);
assert.equal(importedSource.workspace.project_id, sourceWorkspace.project_id);
assert.equal(importedSource.workspace.default_language, sourceWorkspace.default_language);
assert.equal(importedSource.workspace.packages[0].manifest.id, "conversation");
assert.equal(importedSource.workspace.packages[0].contents.behaviors[0].value.followup_scope, "confirm.help");
assert.deepEqual(importedSource.workspace.packages[0].contents.behaviors[0].value.requires_values, [{ namespace: "context", path: "device.ready", value: true }]);
assert.deepEqual(importedSource.workspace.packages[0].contents.behaviors[0].value.forbidden_values, [{ namespace: "author", path: "device.disabled", value: true }]);
assert.deepEqual(importedSource.assetFiles, []);
await assert.rejects(
  () => loadSourceTree([...sourceFiles, fakeFile("duplicate-project.json", sourceProjectText, "./gvya.project.json")]),
  /duplicate normalized path/i,
);
await assert.rejects(
  () => loadSourceTree([fakeFile("gvya.project.json", sourceProjectText, "gvya.project.json", 8 * 1024 * 1024 + 1), sourceFiles[1], sourceFiles[2]]),
  /8 MiB source JSON limit/i,
);
const badSourceProject = JSON.parse(sourceProjectText);
badSourceProject.semantic.candidate_limit = 1;
await assert.rejects(
  () => loadSourceTree([fakeFile("gvya.project.json", JSON.stringify(badSourceProject)), sourceFiles[1], sourceFiles[2]]),
  /candidate_limit|semantic/i,
);


const dependencyWorkspace = createStarterBrainWorkspace();
const dependencyPeer = createPackage("peer");
dependencyWorkspace.packages.push(dependencyPeer);
dependencyWorkspace.packages[0].manifest.dependencies = [{ id: "peer", reexport: false }];
dependencyPeer.manifest.dependencies = [{ id: "conversation", reexport: false }];
assert.ok(auditWorkspace(dependencyWorkspace).some((issue) => issue.code === "studio.package_dependency_cycle" && issue.severity === "error"));
const missingDependencyWorkspace = createStarterBrainWorkspace();
missingDependencyWorkspace.packages[0].manifest.dependencies = [{ id: "missing.package", reexport: false }];
assert.ok(auditWorkspace(missingDependencyWorkspace).some((issue) => issue.code === "studio.package_dependency_missing" && issue.severity === "error"));
const fallbackGraphWorkspace = createStarterBrainWorkspace();
const fallbackGraphPackage = createPackage("fallback.graph", "", "fallback");
fallbackGraphWorkspace.packages.push(fallbackGraphPackage);
fallbackGraphWorkspace.packages[0].manifest.dependencies = [{ id: fallbackGraphPackage.manifest.id, reexport: false }];
assert.ok(auditWorkspace(fallbackGraphWorkspace).some((issue) => issue.code === "studio.fallback_dependency_forbidden" && issue.severity === "error"));
fallbackGraphWorkspace.packages.push(createPackage("fallback.second", "", "fallback"));
assert.ok(auditWorkspace(fallbackGraphWorkspace).some((issue) => issue.code === "studio.multiple_fallback_packages" && issue.severity === "error"));

console.log("PASS Studio source contract serialization");
console.log("PASS canonical package namespace coverage");
console.log("PASS response asset/follow-up source shapes");
console.log("PASS authoring audit exact-collision behavior");
console.log("PASS strict bounded workspace persistence/import decoder");
console.log("PASS bounded source-tree import, duplicate-path rejection, and canonical semantic bounds");
console.log("PASS project dependency graph errors are fail-closed in Studio audit");

// Studio v1 authoring hierarchy: Projects own Standard/Fallback Packages; Bots attach only Standard Packages and own all overrides.
const studioModel = await import("../apps/studio/dist/studio-model.js");
const studioIo = await import("../apps/studio/dist/studio-workspace-io.js");
let studio = createDefaultStudio();
assert.equal(studio.version, 1);
assert.equal(studio.projects.length, 1);
const defaultStandardPackage = studio.shared_packages.find((pkg) => pkg.manifest.id === DEFAULT_STANDARD_PACKAGE_ID);
assert.ok(defaultStandardPackage);
assert.ok(!("version" in defaultStandardPackage.manifest));
const studioProject = studioModel.selectedProject(studio);
const studioBot = studioModel.selectedBot(studio, studioProject);
assert.ok(!("languages" in studio.settings));
assert.ok(!("language_ids" in studioProject));
assert.deepEqual(studioModel.sharedAvailableLanguages(studio), DEFAULT_SHARED_LANGUAGES);
assert.deepEqual(studioModel.projectAvailableLanguages(studioProject), DEFAULT_SHARED_LANGUAGES);
assert.equal(studioBot.default_language, "en-US");
assert.deepEqual(studioBot.enabled_languages, DEFAULT_SHARED_LANGUAGES);
assert.equal(studioBot.package.authoring_language, studioBot.default_language);
assert.equal(defaultStandardPackage.authoring_language, "en-US");
assert.equal(studioBot.fallback_package_id, null, "default Fallback Packages are available but never selected implicitly");
const defaultFallbackPackages = studio.shared_packages.filter((pkg) => pkg.manifest.kind === "fallback");
assert.deepEqual(defaultFallbackPackages.map((pkg) => pkg.manifest.id), [DEFAULT_FALLBACK_PACKAGE_IDS.formal, DEFAULT_FALLBACK_PACKAGE_IDS.informal]);
for (const fallbackPackage of defaultFallbackPackages) {
  assert.equal(fallbackPackage.authoring_language, "en-US");
  assert.deepEqual(fallbackPackage.manifest.dependencies, []);
  const fallbackTriggers = fallbackPackage.contents.fallback_behaviors.map((row) => row.value.trigger);
  assert.equal(fallbackTriggers.filter((trigger) => trigger === "unresolved").length, 3, "default fallback package must keep all three unresolved repair stages");
  assert.equal(fallbackTriggers.filter((trigger) => trigger === "repeat").length, 1, "default fallback package must keep the repeat fallback");
  assert.ok(fallbackPackage.contents.fallback_behaviors.every((row) => row.exported === false && row.mode === "add"));
  for (const behavior of fallbackPackage.contents.fallback_behaviors) {
    for (const response of behavior.value.responses) {
      assert.deepEqual(response.texts.map((row) => row.language), DEFAULT_SHARED_LANGUAGES);
      assert.ok(response.texts.every((row) => row.variants.length >= 2 && row.variants.every((text) => text.trim().length > 0)));
    }
  }
}
const defaultSmalltalkPackages = studio.shared_packages.filter((pkg) => DEFAULT_SMALLTALK_PACKAGE_IDS.includes(pkg.manifest.id));
assert.deepEqual(defaultSmalltalkPackages.map((pkg) => pkg.manifest.id).sort(), [...DEFAULT_SMALLTALK_PACKAGE_IDS].sort());
for (const smalltalkPackage of defaultSmalltalkPackages) {
  assert.equal(smalltalkPackage.manifest.kind, "standard");
  assert.deepEqual(smalltalkPackage.manifest.dependencies, []);
  assert.equal(smalltalkPackage.contents.meanings.length, 116);
  assert.equal(smalltalkPackage.contents.behaviors.length, 116);
  const expectedRegressionCount = smalltalkPackage.manifest.id === "core.smalltalk.formal" ? 241 : 239;
  assert.equal(smalltalkPackage.contents.regression_cases.length, expectedRegressionCount);
  assert.match(smalltalkPackage.manifest.description, /explicit host system facts/u);
}
for (const sharedPackage of studio.shared_packages) {
  assert.equal(sharedPackage.authoring_language, "en-US");
  for (const meaning of sharedPackage.contents.meanings) {
    assert.deepEqual([...new Set(meaning.value.samples.map((row) => row.language))].sort(), [...DEFAULT_SHARED_LANGUAGES].sort(), `${sharedPackage.manifest.id}:${meaning.id} samples must be bilingual`);
  }
  const responseOwners = [
    ...sharedPackage.contents.behaviors,
    ...sharedPackage.contents.fallback_behaviors,
  ];
  for (const owner of responseOwners) {
    for (const response of owner.value.responses) {
      assert.deepEqual(response.texts.map((row) => row.language), DEFAULT_SHARED_LANGUAGES, `${sharedPackage.manifest.id}:${response.id} responses must be bilingual`);
    }
  }
}
console.log("PASS physical Shared Packages are English/fa-IR bilingual and include formal/informal pure Smalltalk defaults");
let portableSourceStudio = studioModel.addPackageToBot(createDefaultStudio(), DEFAULT_STANDARD_PACKAGE_ID);
portableSourceStudio = studioModel.setBotFallbackPackage(portableSourceStudio, DEFAULT_FALLBACK_PACKAGE_IDS.informal);
const collectionMeaning = createMeaning("collection.roundtrip");
collectionMeaning.samples = [{ language: "en-US", text: "make an order" }];
collectionMeaning.slots = [{ name: "count", type: "number", entity_kind: "", reference_kind: "", required: true, elicitation: [{ language: "en-US", text: "How many?" }] }];
collectionMeaning.references = [{ kind: "person", required: true, elicitation: [{ language: "en-US", text: "Who?" }] }];
portableSourceStudio.projects[0].bots[0].package.contents.meanings.push(contribution(collectionMeaning.id, collectionMeaning));
const portableSourceEntries = await encodeContent(portableSourceStudio, []);
const collectionRoundTrip = decodeContent(portableSourceEntries).workspace.projects[0].bots[0].package.contents.meanings.find((row) => row.id === collectionMeaning.id)?.value;
assert.deepEqual(collectionRoundTrip?.slots[0].elicitation, [{ language: "en-US", text: "How many?" }]);
assert.deepEqual(collectionRoundTrip?.references[0].elicitation, [{ language: "en-US", text: "Who?" }]);
const canonicalTargetEntry = portableSourceEntries.find((entry) => entry.path === "gvya.project.json");
assert.ok(canonicalTargetEntry, "Studio must persist one canonical CLI target for the selected Bot");
const canonicalTargetJson = JSON.parse(Buffer.from(canonicalTargetEntry.bytes_base64, "base64").toString("utf8"));
assert.equal(canonicalTargetJson.format, "gvya.source.project");
assert.equal(canonicalTargetJson.project_id, "gvya-project");
assert.equal(canonicalTargetJson.brain_id, "gvya-bot");
assert.ok(canonicalTargetJson.packages.includes(`shared/packages/standard/${DEFAULT_STANDARD_PACKAGE_ID}/package.json`));
assert.throws(() => decodeContent(portableSourceEntries.map((entry) => entry.path === "gvya.project.json"
  ? { ...entry, bytes_base64: Buffer.from(`${JSON.stringify({ ...canonicalTargetJson, brain_id: "wrong-bot" }, null, 2)}\n`).toString("base64") }
  : entry)), /must exactly describe the selected Studio Bot/u);
assert.ok(portableSourceEntries.some((entry) => entry.path === `shared/packages/standard/${DEFAULT_STANDARD_PACKAGE_ID}/package.json`));
assert.ok(portableSourceEntries.some((entry) => entry.path === "shared/packages/fallback/gvya.fallback.informal/package.json"));
assert.ok(!portableSourceEntries.some((entry) => entry.path === `projects/gvya-project/packages/standard/${DEFAULT_STANDARD_PACKAGE_ID}/package.json`));
assert.ok(!portableSourceEntries.some((entry) => entry.path === "projects/gvya-project/packages/fallback/gvya.fallback.informal/package.json"));
assert.deepEqual(portableSourceStudio.projects[0].packages, [], "selecting Shared Packages must not create Project-owned copies");
const editablePackagePath = `shared/packages/standard/${DEFAULT_STANDARD_PACKAGE_ID}/package.json`;
const editablePackageEntry = portableSourceEntries.find((entry) => entry.path === editablePackagePath);
assert.ok(editablePackageEntry);
const editablePackageJson = JSON.parse(Buffer.from(editablePackageEntry.bytes_base64, "base64").toString("utf8"));
assert.equal(editablePackageJson.format, "gvya.source.package");
assert.equal(editablePackageJson.version, 1);
assert.deepEqual(Object.keys(editablePackageJson).sort(), ["format", "fragments", "manifest", "version"]);
assert.ok(Array.isArray(editablePackageJson.fragments.behaviors));
const botDiskJson = JSON.parse(Buffer.from(portableSourceEntries.find((entry) => entry.path === "projects/gvya-project/bots/gvya-bot/bot.json").bytes_base64, "base64").toString("utf8"));
assert.ok("semantic" in botDiskJson && "conversation" in botDiskJson && "emit_debug_map" in botDiskJson);
assert.ok(!("settings" in botDiskJson));
const firstBehaviorRelative = editablePackageJson.fragments.behaviors[0];
const firstBehaviorPath = editablePackagePath.replace(/package\.json$/u, firstBehaviorRelative);
const firstBehaviorEntry = portableSourceEntries.find((entry) => entry.path === firstBehaviorPath);
assert.ok(firstBehaviorEntry, "declared behavior fragment must be persisted beside package.json");
const firstBehaviorJson = JSON.parse(Buffer.from(firstBehaviorEntry.bytes_base64, "base64").toString("utf8"));
const originalBehaviorTopicScope = firstBehaviorJson.value.topic_scoped;
firstBehaviorJson.value.topic_scoped = !originalBehaviorTopicScope;
const fragmentEditedContent = decodeContent(portableSourceEntries.map((entry) => entry.path === firstBehaviorPath
  ? { ...entry, bytes_base64: Buffer.from(`${JSON.stringify(firstBehaviorJson, null, 2)}\n`).toString("base64") }
  : entry));
assert.equal(fragmentEditedContent.workspace.shared_packages.find((pkg) => pkg.manifest.id === DEFAULT_STANDARD_PACKAGE_ID).contents.behaviors[0].value.topic_scoped, !originalBehaviorTopicScope);
editablePackageJson.manifest.description = "Edited directly by an external agent";
const agentEditedContent = decodeContent(portableSourceEntries.map((entry) => entry.path === editablePackagePath
  ? { ...entry, bytes_base64: Buffer.from(`${JSON.stringify(editablePackageJson, null, 2)}\n`).toString("base64") }
  : entry));
assert.equal(agentEditedContent.workspace.shared_packages.find((pkg) => pkg.manifest.id === DEFAULT_STANDARD_PACKAGE_ID).manifest.description, "Edited directly by an external agent");
const withoutHumanSidecars = decodeContent(portableSourceEntries.filter((entry) => !entry.path.endsWith("/authoring.json")));
assert.equal(withoutHumanSidecars.workspace.shared_packages.find((pkg) => pkg.manifest.id === DEFAULT_STANDARD_PACKAGE_ID).authoring_language, "en-US");
console.log("PASS canonical Package manifest/index and declared fragments are directly agent-editable; human authoring sidecars are optional");
const targetShell = createDefaultStudio();
targetShell.shared_packages = [];
targetShell.projects = [];
targetShell.selectedProjectId = "";
targetShell.selectedBotId = "";
targetShell.selectedPackageScope = "shared";
targetShell.selectedPackageId = "";
targetShell.settings.semantic.resolution_threshold = 0.91;
targetShell.settings.conversation.default_topic_ttl = 99;
const targetStudioEntry = (await encodeContent(targetShell, [])).find((entry) => entry.path === "studio.json");
assert.ok(targetStudioEntry);
const projectOnlyEntries = [
  targetStudioEntry,
  canonicalTargetEntry,
  ...portableSourceEntries.filter((entry) => entry.path.startsWith("projects/gvya-project/")),
];
assert.throws(() => decodeContent(projectOnlyEntries), /unavailable package|unavailable Fallback Package/u, "a Project with live Shared references requires its Shared library");
const copiedProject = decodeContent([
  targetStudioEntry,
  canonicalTargetEntry,
  ...portableSourceEntries.filter((entry) => entry.path.startsWith("shared/") || entry.path.startsWith("projects/gvya-project/")),
]).workspace;
const portableSourceBrain = studioModel.resolveSelectedBrain(portableSourceStudio);
const copiedProjectBrain = studioModel.resolveSelectedBrain(copiedProject);
assert.deepEqual(projectSourceDocument(copiedProjectBrain), projectSourceDocument(portableSourceBrain));
assert.deepEqual(copiedProjectBrain.packages.map(packageSnapshotDocument), portableSourceBrain.packages.map(packageSnapshotDocument));
console.log("PASS Project settings remain portable while live Shared references resolve from the destination library");
assert.ok(!("default_language" in studioBot.settings));
assert.deepEqual(studioProject.packages, []);
assert.ok(!("shared_package_ids" in studioProject));
assert.ok(!("shared_overrides" in studioProject));
assert.deepEqual(studioBot.package_ids, []);
assert.equal(studioBot.package.manifest.id, `${studioProject.id}.${studioBot.id}.bot`);
assert.equal(typeof studioModel.addSharedPackageToProject, "undefined");
assert.equal(typeof studioModel.createProjectOverride, "undefined");
assert.equal(typeof studioModel.removeProjectOverride, "undefined");
let resolved = studioModel.resolveSelectedBrain(studio);
assert.deepEqual(resolved.packages.map((pkg) => pkg.manifest.id), [studioBot.package.manifest.id]);
assert.ok(!("projects" in projectSourceDocument(resolved)) && !("shared_packages" in projectSourceDocument(resolved)));
const languageStudio = createDefaultStudio();
const englishProfile = languageStudio.projects[0].matcher_profiles.find((row) => row.language === "en-US");
assert.ok(englishProfile);
languageStudio.projects[0].matcher_profiles = [{ language: "fa", language_profile: {}, profile: {} }, englishProfile, { language: "es", language_profile: {}, profile: {} }];
languageStudio.projects[0].bots[0].enabled_languages = ["en-US", "fa", "es"];
assert.deepEqual(studioModel.resolveSelectedBrain(languageStudio).languages, ["fa", "en-US", "es"], "Project language order comes directly from its Matcher Profile documents");
assert.deepEqual(projectSourceDocument(studioModel.resolveSelectedBrain(languageStudio)).languages, ["fa", "en-US", "es"]);
assert.deepEqual(projectSourceDocument(studioModel.resolveSelectedBrain(languageStudio)).enabled_languages, ["en-US", "fa", "es"], "Bot enabled languages, not Project profile order, become runtime authority");
assert.deepEqual(studioModel.resolveSelectedBrain(languageStudio).matcher_profiles.map((row) => row.language), ["en-US", "fa", "es"], "active Bot languages automatically select their Project matcher profiles");
const englishOnlyLanguageStudio = structuredClone(languageStudio);
englishOnlyLanguageStudio.projects[0].bots[0].enabled_languages = ["en-US"];
assert.deepEqual(studioModel.resolveSelectedBrain(englishOnlyLanguageStudio).matcher_profiles.map((row) => row.language), ["en-US"], "inactive Bot languages cannot leak matcher profiles into runtime source");
assert.equal(studioModel.resolveSelectedBrain(languageStudio).default_language, "en-US", "Project language order must not change the Bot default");
assert.equal(projectSourceDocument(studioModel.resolveSelectedBrain(languageStudio)).default_language, "en-US");
const automaticProfileStudio = studioModel.addProject(createDefaultStudio(), "automatic-profiles", DEFAULT_SHARED_LANGUAGES, "en-US");
assert.deepEqual(studioModel.selectedProject(automaticProfileStudio).matcher_profiles.map((row) => row.language), DEFAULT_SHARED_LANGUAGES, "choosing Project languages must copy known matcher profiles without a separate authoring action");
assert.deepEqual(studioModel.resolveSelectedBrain(automaticProfileStudio).matcher_profiles.map((row) => row.language), DEFAULT_SHARED_LANGUAGES, "the new Bot must automatically compile matcher profiles for its active languages");

const botProfileDisabledStudio = createDefaultStudio();
botProfileDisabledStudio.projects[0].matcher_profiles = botProfileDisabledStudio.projects[0].matcher_profiles.filter((row) => row.language !== "fa-IR");
assert.deepEqual(studioModel.botMissingMatcherLanguages(botProfileDisabledStudio), ["fa-IR"], "a Bot remains loadable but disabled when an enabled-language Matcher Profile disappears");
assert.deepEqual(studioModel.resolveSelectedBrain(botProfileDisabledStudio).matcher_profiles.map((row) => row.language), ["en-US"]);
assert.doesNotThrow(() => studioIo.studioWorkspaceFromText(studioIo.studioWorkspaceToText(botProfileDisabledStudio)), "disabled Bots must remain persistable for repair");

let attachedPackageDisabledStudio = studioModel.addPackageToBot(createDefaultStudio(), DEFAULT_STANDARD_PACKAGE_ID);
attachedPackageDisabledStudio.projects[0].bots[0].enabled_languages = ["en-US"];
attachedPackageDisabledStudio.projects[0].matcher_profiles = attachedPackageDisabledStudio.projects[0].matcher_profiles.filter((row) => row.language !== "fa-IR");
assert.deepEqual(studioModel.botMissingMatcherLanguages(attachedPackageDisabledStudio), ["fa-IR"], "an attached Package with uncovered content disables the Bot");
assert.ok(!studioModel.botSelectablePackages(attachedPackageDisabledStudio).some((pkg) => pkg.manifest.id === DEFAULT_STANDARD_PACKAGE_ID), "a Package with a missing Project Matcher Profile must not be selectable for a Bot");
assert.ok(!studioModel.botSelectableFallbackPackages(attachedPackageDisabledStudio).some((pkg) => pkg.manifest.id === DEFAULT_FALLBACK_PACKAGE_IDS.formal), "an uncovered Fallback Package must not be selectable for a Bot");
assert.throws(() => studioModel.setBotPackages(attachedPackageDisabledStudio, [DEFAULT_STANDARD_PACKAGE_ID]), /not available to this Bot/u);
assert.throws(() => studioModel.setBotFallbackPackage(attachedPackageDisabledStudio, DEFAULT_FALLBACK_PACKAGE_IDS.formal), /not available to this Bot/u);
assert.doesNotThrow(() => studioIo.studioWorkspaceFromText(studioIo.studioWorkspaceToText(attachedPackageDisabledStudio)), "Bots disabled by attached Package content must remain persistable for repair");
attachedPackageDisabledStudio.selectedPackageScope = "shared";
attachedPackageDisabledStudio.selectedPackageId = DEFAULT_STANDARD_PACKAGE_ID;
assert.deepEqual(studioModel.selectedPackageMissingMatcherLanguages(attachedPackageDisabledStudio), [], "the live Shared source remains valid against its own Language/Matcher Profile pairs");
assert.deepEqual(studioModel.resolvePackagePreview(attachedPackageDisabledStudio).packages.map((pkg) => pkg.manifest.id), [DEFAULT_STANDARD_PACKAGE_ID]);

const sharedPackageDisabledStudio = createDefaultStudio();
sharedPackageDisabledStudio.shared_matcher_profiles = sharedPackageDisabledStudio.shared_matcher_profiles.filter((row) => row.language !== "fa-IR");
sharedPackageDisabledStudio.selectedPackageScope = "shared";
sharedPackageDisabledStudio.selectedPackageId = DEFAULT_STANDARD_PACKAGE_ID;
assert.deepEqual(studioModel.selectedPackageMissingMatcherLanguages(sharedPackageDisabledStudio), ["fa-IR"], "a Shared Package remains loadable but disabled when its Language/Matcher Profile pair disappears");
assert.doesNotThrow(() => studioIo.studioWorkspaceFromText(studioIo.studioWorkspaceToText(sharedPackageDisabledStudio)), "disabled Packages must remain persistable for repair");
console.log("PASS paired Language/Matcher Profile documents are the language catalogs; uncovered Bots/Packages are disabled and unselectable");

const authoringStudio = studioModel.addSharedPackage(createDefaultStudio(), "shared.persian-authoring", "fa-IR");
assert.equal(studioModel.resolveEditingBrain(authoringStudio).authoring_language, "fa-IR");
assert.equal(projectSourceDocument(studioModel.resolveEditingBrain(authoringStudio)).enabled_languages[0], "en-US");
assert.ok(!("authoring_language" in packageSourceFiles(authoringStudio.shared_packages.find((pkg) => pkg.manifest.id === "shared.persian-authoring"))[0].json), "Package authoring preference must not enter canonical source");

const previewStudio = createDefaultStudio();
const previewRoot = createPackage("preview.root");
previewRoot.manifest.dependencies = [{ id: DEFAULT_STANDARD_PACKAGE_ID, reexport: false }];
previewStudio.shared_packages.push(previewRoot);
previewStudio.selectedPackageScope = "shared";
previewStudio.selectedPackageId = previewRoot.manifest.id;
const sharedPreview = studioModel.resolvePackagePreview(previewStudio);
assert.equal(sharedPreview.brain_id, "package-preview");
assert.deepEqual(sharedPreview.packages.map((row) => row.manifest.id), [DEFAULT_STANDARD_PACKAGE_ID, "preview.root"]);
assert.ok(!sharedPreview.packages.some((row) => row.manifest.id === previewStudio.projects[0].bots[0].package.manifest.id), "Package preview must exclude unrelated Bot composition");
previewStudio.projects[0].bots[0].settings.semantic.resolution_threshold = 0.99;
assert.equal(studioModel.resolvePackagePreview(previewStudio).semantic.resolution_threshold, previewStudio.settings.semantic.resolution_threshold, "Package preview must ignore Bot setting overrides");
const missingPreview = structuredClone(previewStudio);
missingPreview.shared_packages.find((row) => row.manifest.id === "preview.root").manifest.dependencies = [{ id: "missing.preview", reexport: false }];
assert.throws(() => studioModel.resolvePackagePreview(missingPreview), /requires unavailable package missing\.preview/u);
console.log("PASS Package preview resolves only its rooted dependency graph without Bot composition/settings");
console.log("PASS Studio v1 ownership model + exactly one structural Bot Package + no Project-level Shared attachment/override");

// Portable Bot files materialize effective settings so Project folders never inherit another machine's Studio defaults.
assert.ok(studio.settings.semantic.resolution_threshold > 0);
assert.deepEqual(studioBot.settings.semantic, studio.settings.semantic);
const globalThreshold = studio.settings.semantic.resolution_threshold;
studioBot.settings.semantic.resolution_threshold = globalThreshold + 0.01;
assert.equal(studioModel.resolveSelectedBrain(studio).semantic.resolution_threshold, globalThreshold + 0.01);
delete studioBot.settings.semantic.resolution_threshold;
assert.equal(studioModel.resolveSelectedBrain(studio).semantic.resolution_threshold, globalThreshold);

// Shared Package editing is global and carries no automatic revision state.
const sharedEditingStudio = { ...studio, selectedPackageScope: "shared", selectedPackageId: DEFAULT_STANDARD_PACKAGE_ID };
const sharedEditing = studioModel.resolveEditingBrain(sharedEditingStudio);
const sharedChanged = structuredClone(sharedEditing);
sharedChanged.packages[0].contents.behaviors[0].value.responses[0].texts[0].variants[0] = "Changed";
studio = studioModel.applyEditingBrain(sharedEditingStudio, sharedEditing, sharedChanged);
const changedShared = studio.shared_packages.find((row) => row.manifest.id === DEFAULT_STANDARD_PACKAGE_ID);
assert.equal(changedShared.contents.behaviors[0].value.responses[0].texts[0].variants[0], "Changed");
assert.ok(!("history" in changedShared));
console.log("PASS Shared Package identity + direct editing without automatic revision state");

// Shared Standard Packages remain live sources beside Project-owned Packages.
studio = studioModel.addProjectPackage(studio, "project.dialog");
const projectDialog = studio.projects[0].packages.find((pkg) => pkg.manifest.id === "project.dialog");
assert.deepEqual(new Set(studioModel.botAttachablePackages(studio).map((pkg) => pkg.manifest.id)), new Set([...DEFAULT_SMALLTALK_PACKAGE_IDS, "iot.assistant", "project.dialog"]));
assert.deepEqual(new Set(studioModel.botSelectablePackages(studio).map((pkg) => pkg.manifest.id)), new Set([...DEFAULT_SMALLTALK_PACKAGE_IDS, "iot.assistant", "project.dialog"]));
console.log("PASS Shared Standard Packages remain live sources beside Project-owned Packages");

// Shared and Project Fallback Packages are optional per Bot, selected whole, and never enter the override graph.
studio = studioModel.addSharedFallbackPackage(studio, "fallback.personality");
const sharedFallback = studio.shared_packages.find((row) => row.manifest.id === "fallback.personality");
const studioAngryFallback = createFallbackBehavior("fallback.angry");
studioAngryFallback.priority = 100;
studioAngryFallback.conditions = [{ namespace: "author", path: "mood.anger", op: "greater_or_equal", value: 70, hasValue: true }];
studioAngryFallback.responses[0].kind = "fallback";
studioAngryFallback.responses[0].texts = [{ language: "en-US", variants: ["What now?"] }];
sharedFallback.contents.fallback_behaviors.push(fallbackContribution(studioAngryFallback.id, studioAngryFallback));
assert.equal(studioModel.selectedBot(studio).fallback_package_id, null);
assert.ok(!studioModel.botAttachablePackages(studio).some((pkg) => pkg.manifest.id === "fallback.personality"));
assert.throws(() => studioModel.addPackageToBot(studio, "fallback.personality"), /not standard|not available/i);
studio = studioModel.setBotFallbackPackage(studio, "fallback.personality");
assert.equal(studioModel.selectedBot(studio).fallback_package_id, "fallback.personality");
resolved = studioModel.resolveSelectedBrain(studio);
assert.equal(resolved.packages.filter((pkg) => pkg.manifest.kind === "fallback").length, 1);
assert.equal(projectSourceDocument(resolved).fallback_package, "packages/fallback.personality/package.json");
assert.throws(() => studioModel.setBotFallbackPackage(studio, DEFAULT_STANDARD_PACKAGE_ID), /not fallback|not available/i);
studio = studioModel.addProjectPackage(studio, "fallback.project", "fallback");
const projectFallbackEditing = studioModel.resolveEditingBrain(studio);
assert.deepEqual(projectFallbackEditing.packages.map((pkg) => pkg.manifest.id), ["fallback.project"]);
const changedProjectFallback = structuredClone(projectFallbackEditing);
const projectFallbackBehavior = createFallbackBehavior("fallback.project.unresolved");
changedProjectFallback.packages[0].contents.fallback_behaviors.push(fallbackContribution(projectFallbackBehavior.id, projectFallbackBehavior));
studio = studioModel.applyEditingBrain(studio, projectFallbackEditing, changedProjectFallback);
assert.deepEqual(
  new Set(studioModel.botSelectableFallbackPackages(studio).map((pkg) => pkg.manifest.id)),
  new Set([DEFAULT_FALLBACK_PACKAGE_IDS.formal, DEFAULT_FALLBACK_PACKAGE_IDS.informal, "fallback.personality", "fallback.project"]),
);
assert.ok(!studioModel.botAttachablePackages(studio).some((pkg) => pkg.manifest.kind === "fallback"));
studio = studioModel.setBotFallbackPackage(studio, "fallback.project");
resolved = studioModel.resolveSelectedBrain(studio);
assert.equal(resolved.packages.filter((pkg) => pkg.manifest.kind === "fallback").length, 1);
assert.equal(projectSourceDocument(resolved).fallback_package, "packages/fallback.project/package.json");
assert.deepEqual(studioModel.resolvePackagePreview(studio).packages.map((pkg) => pkg.manifest.id), ["fallback.project"]);
assert.equal(studioIo.studioWorkspaceFromText(studioIo.studioWorkspaceToText(studio)).projects[0].packages.find((pkg) => pkg.manifest.id === "fallback.project").manifest.kind, "fallback");
studio = studioModel.setBotFallbackPackage(studio, "fallback.personality");
console.log("PASS Shared/Project Fallback Packages are optional per Bot, whole-selected, and outside the override graph");

// Bot manages Shared and Project Package membership as one explicit selection; there is no Project composition layer.
studio = studioModel.setBotPackages(studio, [DEFAULT_STANDARD_PACKAGE_ID, "project.dialog"]);
assert.deepEqual(studioModel.selectedBot(studio).package_ids.sort(), [DEFAULT_STANDARD_PACKAGE_ID, "project.dialog"]);
resolved = studioModel.resolveSelectedBrain(studio);
assert.deepEqual(new Set(resolved.packages.map((pkg) => pkg.manifest.id)), new Set([DEFAULT_STANDARD_PACKAGE_ID, "project.dialog", studioModel.selectedBot(studio).package.manifest.id, "fallback.personality"]));

// Bot is the only override scope, for both Shared and Project Packages; replacements live in its one Bot Package.
assert.ok(studioModel.overrideableContributions(studio, DEFAULT_STANDARD_PACKAGE_ID).some((row) => row.namespace === "behaviors" && row.id === DEFAULT_STANDARD_BEHAVIOR_ID));
const ownedBotBehaviorCount = studioModel.selectedBot(studio).package.contents.behaviors.length;
studio = studioModel.overrideContribution(studio, DEFAULT_STANDARD_PACKAGE_ID, "behaviors", DEFAULT_STANDARD_BEHAVIOR_ID);
const botPackage = studioModel.selectedBot(studio).package;
const standardBehaviorReplacement = botPackage.contents.behaviors.find((row) => row.mode.target_package === DEFAULT_STANDARD_PACKAGE_ID);
assert.ok(standardBehaviorReplacement);
assert.ok(botPackage.manifest.dependencies.some((dep) => dep.id === DEFAULT_STANDARD_PACKAGE_ID));
assert.ok(!("packages" in studioModel.selectedBot(studio)));
console.log("PASS Bot-only override writes Shared/Project replacements into the single Bot Package");

// Updating managed Package selection never removes the structural Bot Package and cleans only replacements for unchecked Packages.
studio = studioModel.setBotPackages(studio, ["project.dialog"]);
assert.ok(!studioModel.selectedBot(studio).package_ids.includes(DEFAULT_STANDARD_PACKAGE_ID));
assert.equal(studioModel.selectedBot(studio).package.contents.behaviors.length, ownedBotBehaviorCount);
assert.ok(studioModel.selectedBot(studio).package.contents.behaviors.every((row) => row.mode.target_package !== DEFAULT_STANDARD_PACKAGE_ID));
assert.ok(studioModel.selectedBot(studio).package);
assert.throws(() => studioModel.setBotPackages(studio, ["missing.package"]), /not available/i);
console.log("PASS Bot Package is structural; managed Package selection changes composition and matching replacements atomically");

// Project Package deletion detaches Standard Packages and clears a selected Project Fallback, but never deletes the Bot Package.
studio = studioModel.setBotFallbackPackage(studio, "fallback.project");
assert.deepEqual(studioModel.projectPackageRemovalImpact(studio, "fallback.project").bot_ids, ["gvya-bot"]);
studio = studioModel.removeProjectPackage(studio, "fallback.project");
assert.equal(studioModel.selectedBot(studio).fallback_package_id, null);
studio = studioModel.removeProjectPackage(studio, "project.dialog");
assert.ok(!studio.projects[0].packages.some((pkg) => pkg.manifest.id === "project.dialog"));
assert.ok(!studioModel.selectedBot(studio).package_ids.includes("project.dialog"));
assert.ok(studioModel.selectedBot(studio).package);

// IDs identify one source owner across Shared, Project, and Bot scope within a Project graph.
assert.throws(() => studioModel.addProjectPackage(studio, DEFAULT_STANDARD_PACKAGE_ID), /already exists/i);
const sharedCollisionStudio = studioModel.addProjectPackage(studio, "project.collision");
assert.throws(() => studioModel.addSharedPackage(sharedCollisionStudio, "project.collision"), /already exists/i);
const collisionWorkspace = JSON.parse(studioIo.studioWorkspaceToText(studio));
collisionWorkspace.projects[0].bots = [];
collisionWorkspace.selectedBotId = "";
collisionWorkspace.projects[0].packages.push(structuredClone(collisionWorkspace.shared_packages[0]));
assert.throws(() => studioIo.studioWorkspaceFromText(JSON.stringify(collisionWorkspace)), /Package IDs|duplicate/i);
console.log("PASS Package IDs have one Shared/Project/Bot owner within each Project graph");

// Project Package deletion exposes its exact transitive package/Bot impact before mutation.
let impactStudio = createDefaultStudio();
impactStudio = studioModel.addProjectPackage(impactStudio, "project.base");
impactStudio = studioModel.addProjectPackage(impactStudio, "project.child");
impactStudio.projects[0].packages.find((pkg) => pkg.manifest.id === "project.child").manifest.dependencies.push({ id: "project.base", reexport: true });
impactStudio = studioModel.addPackageToBot(impactStudio, "project.child");
const removalImpact = studioModel.projectPackageRemovalImpact(impactStudio, "project.base");
assert.deepEqual(new Set(removalImpact.package_ids), new Set(["project.base", "project.child"]));
assert.deepEqual(removalImpact.bot_ids, ["gvya-bot"]);
console.log("PASS Project Package removal impact is explicit and transitive");

// One canonical Bot package closure feeds simulation, build, export and the canonical CLI target.
{
  const declareDependency = (workspace, packageId, dependencyId, reexport = false) => {
    const pkg = workspace.projects[0].packages.find((row) => row.manifest.id === packageId);
    assert.ok(pkg, `test package ${packageId} must exist`);
    pkg.manifest.dependencies.push({ id: dependencyId, reexport });
  };
  let closureStudio = createDefaultStudio();
  for (const id of ["closure.a", "closure.b", "closure.c", "closure.d", "closure.unrelated"]) {
    closureStudio = studioModel.addProjectPackage(closureStudio, id);
  }
  closureStudio = studioModel.addProjectPackage(closureStudio, "closure.recovery", "fallback");
  declareDependency(closureStudio, "closure.a", "closure.b");
  declareDependency(closureStudio, "closure.b", "closure.c");
  declareDependency(closureStudio, "closure.d", "closure.c");
  closureStudio = studioModel.setBotPackages(closureStudio, ["closure.a", "closure.d"]);
  closureStudio = studioModel.setBotFallbackPackage(closureStudio, "closure.recovery");

  const closureProject = studioModel.selectedProject(closureStudio);
  const closureBot = studioModel.selectedBot(closureStudio, closureProject);
  const botPackageId = closureBot.package.manifest.id;
  const closure = studioModel.resolveBotPackageClosure(closureStudio, closureProject, closureBot);
  const closureIds = closure.map((entry) => entry.package.manifest.id);

  // 1/2/3/4: direct selection, Bot Package, transitive and nested-transitive dependencies.
  assert.ok(closureIds.includes("closure.a"), "directly selected Package must be in the closure");
  assert.ok(closureIds.includes("closure.d"), "every directly selected Package must be in the closure");
  assert.ok(closureIds.includes(botPackageId), "the Bot Package must be in the closure");
  assert.ok(closureIds.includes("closure.b"), "a transitively required dependency must be in the closure");
  assert.ok(closureIds.includes("closure.c"), "a nested transitively required dependency must be in the closure");
  // 5: the selected Fallback Package and its own required closure.
  assert.ok(closureIds.includes("closure.recovery"), "the selected Fallback Package must be in the closure");
  // 6/7: unreachable Project and Shared catalog Packages never join a Bot.
  assert.ok(!closureIds.includes("closure.unrelated"), "an unselected Project Package must never enter a Bot");
  for (const id of DEFAULT_SMALLTALK_PACKAGE_IDS) {
    assert.ok(!closureIds.includes(id), `unselected shared content Package ${id} must never enter a Bot`);
  }
  // 8: a diamond dependency contributes exactly one copy.
  assert.equal(closureIds.filter((id) => id === "closure.c").length, 1, "a shared dependency must appear once");
  assert.equal(new Set(closureIds).size, closureIds.length, "closure must not repeat a Package");
  // 9: deterministic, dependency-before-dependent ordering.
  assert.deepEqual(closureIds, ["closure.c", "closure.b", "closure.a", "closure.d", botPackageId, "closure.recovery"]);
  assert.deepEqual(studioModel.resolveBotPackageClosure(closureStudio, closureProject, closureBot).map((entry) => entry.package.manifest.id), closureIds);
  assert.deepEqual(closure.map((entry) => entry.scope), ["project", "project", "project", "project", "bot", "project"]);

  // 10: simulation/build resolution and canonical source-target resolution agree exactly.
  assert.deepEqual(studioModel.resolveSelectedBrain(closureStudio).packages.map((pkg) => pkg.manifest.id), closureIds);
  const closureTargetEntry = (await encodeContent(closureStudio, [])).find((entry) => entry.path === "gvya.project.json");
  assert.ok(closureTargetEntry, "Studio content must persist one canonical CLI target");
  const closureTarget = JSON.parse(Buffer.from(closureTargetEntry.bytes_base64, "base64").toString("utf8"));
  const targetPackageId = (path) => {
    const bot = /^projects\/[^/]+\/bots\/([^/]+)\/package\/package\.json$/u.exec(path);
    if (bot) return botPackageId;
    const owned = /\/([^/]+)\/package\.json$/u.exec(path);
    assert.ok(owned, `canonical target path is not a package file: ${path}`);
    return owned[1];
  };
  const targetIds = [...closureTarget.packages, ...(closureTarget.fallback_package ? [closureTarget.fallback_package] : [])].map(targetPackageId);
  assert.equal(targetIds.length, closureIds.length, "canonical target must carry exactly the resolved closure");
  assert.deepEqual(new Set(targetIds), new Set(closureIds));
  assert.deepEqual(studioModel.botPackageClosureIds(closureStudio, closureProject, closureBot), closureIds);
  // The cheap identity projection the canonical target uses must equal the cloning closure exactly.
  assert.deepEqual(
    studioModel.botPackageClosureIdentities(closureStudio, closureProject, closureBot),
    closure.map((entry) => ({ scope: entry.scope, id: entry.package.manifest.id, kind: entry.package.manifest.kind })),
    "closure identities must match the resolved closure in order, scope and kind",
  );
  assert.deepEqual(studioModel.projectBotPackageClosureIds(closureStudio, closureProject).get(closureBot.id), closureIds);

  // A Bot Package dependency alone pulls its whole closure in, with nothing directly selected.
  let indirectStudio = createDefaultStudio();
  indirectStudio = studioModel.addProjectPackage(indirectStudio, "indirect.base");
  indirectStudio = studioModel.addProjectPackage(indirectStudio, "indirect.leaf");
  indirectStudio = studioModel.addProjectPackage(indirectStudio, "indirect.unused");
  declareDependency(indirectStudio, "indirect.base", "indirect.leaf");
  studioModel.selectedBot(indirectStudio).package.manifest.dependencies.push({ id: "indirect.base", reexport: false });
  const indirectIds = studioModel.resolveBotPackageClosure(indirectStudio).map((entry) => entry.package.manifest.id);
  assert.deepEqual(indirectIds, ["indirect.leaf", "indirect.base", studioModel.selectedBot(indirectStudio).package.manifest.id]);
  const indirectTargetEntry = (await encodeContent(indirectStudio, [])).find((entry) => entry.path === "gvya.project.json");
  const indirectTarget = JSON.parse(Buffer.from(indirectTargetEntry.bytes_base64, "base64").toString("utf8"));
  assert.equal(indirectTarget.packages.length, 3, "canonical target must include a dependency reached only through the Bot Package");
  assert.ok(indirectTarget.packages.some((path) => path.endsWith("/indirect.leaf/package.json")));
  assert.ok(!indirectTarget.packages.some((path) => path.includes("indirect.unused")));

  // A Fallback Package that declares a required dependency carries that dependency too. The
  // compiler separately forbids fallback dependencies, so this closure rule is fail-closed depth.
  let fallbackStudio = createDefaultStudio();
  fallbackStudio = studioModel.addProjectPackage(fallbackStudio, "fallbackdep.support");
  fallbackStudio = studioModel.addProjectPackage(fallbackStudio, "fallbackdep.recovery", "fallback");
  declareDependency(fallbackStudio, "fallbackdep.recovery", "fallbackdep.support");
  fallbackStudio = studioModel.setBotFallbackPackage(fallbackStudio, "fallbackdep.recovery");
  const fallbackIds = studioModel.resolveBotPackageClosure(fallbackStudio).map((entry) => entry.package.manifest.id);
  assert.ok(fallbackIds.includes("fallbackdep.support"), "a Fallback Package dependency closure must be resolved");
  assert.ok(fallbackIds.includes("fallbackdep.recovery"));
}
console.log("PASS one canonical Bot package closure: selection + Bot Package + transitive + fallback, nothing else");

// Debug-map inclusion is an explicit per-Bot build choice that reaches the canonical compiler target.
{
  const canonicalTargetFor = async (workspace) => {
    const entry = (await encodeContent(workspace, [])).find((row) => row.path === "gvya.project.json");
    return JSON.parse(Buffer.from(entry.bytes_base64, "base64").toString("utf8"));
  };
  let debugStudio = createDefaultStudio();
  debugStudio = studioModel.cloneStudioWorkspace(debugStudio);
  studioModel.selectedBot(debugStudio).settings.emit_debug_map = true;
  assert.equal((await canonicalTargetFor(debugStudio)).emit_debug_map, true);
  assert.equal(studioModel.resolveSelectedBrain(debugStudio).emit_debug_map, true);
  studioModel.selectedBot(debugStudio).settings.emit_debug_map = false;
  assert.equal((await canonicalTargetFor(debugStudio)).emit_debug_map, false);
  assert.equal(studioModel.resolveSelectedBrain(debugStudio).emit_debug_map, false);
  assert.equal(projectSourceDocument(studioModel.resolveSelectedBrain(debugStudio)).emit_debug_map, false);
}
console.log("PASS debug source-map inclusion is an explicit Bot build setting carried to canonical source");

// Package eligibility uses the Bot's own compile languages, never a Project-level approximation.
{
  const enable = (workspace, languages) => {
    const next = studioModel.cloneStudioWorkspace(workspace);
    const bot = studioModel.selectedBot(next);
    bot.enabled_languages = studioModel.projectAvailableLanguages(studioModel.selectedProject(next)).filter((language) => languages.some((row) => row.toLowerCase() === language.toLowerCase()));
    bot.default_language = bot.enabled_languages[0];
    return next;
  };
  const eligibility = (workspace, kind = "standard") => new Map(
    studioModel.botPackageEligibility(workspace, studioModel.selectedProject(workspace), studioModel.selectedBot(workspace), kind)
      .map((row) => [row.package.manifest.id, row]),
  );

  // The shared Smalltalk Packages carry en-US *and* fa-IR matcher evidence.
  const bilingual = createDefaultStudio();
  assert.deepEqual(studioModel.selectedBot(bilingual).enabled_languages, DEFAULT_SHARED_LANGUAGES);
  const bilingualRows = eligibility(bilingual);
  assert.equal(bilingualRows.get(DEFAULT_STANDARD_PACKAGE_ID).eligible, true, "a Bot enabling both languages may use a bilingual Package");
  assert.deepEqual(bilingualRows.get(DEFAULT_STANDARD_PACKAGE_ID).missing_languages, []);
  assert.deepEqual(studioModel.botMissingMatcherLanguages(bilingual), []);

  // EN-only Bot vs EN+FA Package: ineligible, and the reason names the exact missing language.
  const englishOnly = enable(bilingual, ["en-US"]);
  const englishRows = eligibility(englishOnly);
  assert.equal(englishRows.get(DEFAULT_STANDARD_PACKAGE_ID).eligible, false, "an EN-only Bot cannot compile fa-IR matcher evidence");
  assert.deepEqual(englishRows.get(DEFAULT_STANDARD_PACKAGE_ID).missing_languages, ["fa-IR"]);
  assert.ok(!studioModel.botSelectablePackages(englishOnly).some((pkg) => pkg.manifest.id === DEFAULT_STANDARD_PACKAGE_ID));
  assert.throws(() => studioModel.addPackageToBot(englishOnly, DEFAULT_STANDARD_PACKAGE_ID), /not available/i, "an ineligible Package must not be attachable");
  assert.throws(() => studioModel.setBotPackages(englishOnly, [DEFAULT_STANDARD_PACKAGE_ID]), /not available/i);
  // The catalog stays visible so the author can see why, and count/UI rows agree with the rule.
  assert.equal(englishRows.size, eligibility(bilingual).size, "ineligible Packages stay listed with their reason");

  // Selection never silently widens the Bot's languages.
  assert.deepEqual(studioModel.selectedBot(englishOnly).enabled_languages, ["en-US"]);
  try { studioModel.addPackageToBot(englishOnly, DEFAULT_STANDARD_PACKAGE_ID); } catch { /* expected */ }
  assert.deepEqual(studioModel.selectedBot(englishOnly).enabled_languages, ["en-US"], "a rejected selection must not enable extra Bot languages");

  // Response-only languages are a Project-catalog requirement, not a Bot Semantic Profile one.
  let responseOnly = studioModel.addProjectPackage(englishOnly, "eligible.response-only");
  {
    const pkg = studioModel.selectedProject(responseOnly).packages.find((row) => row.manifest.id === "eligible.response-only");
    const meaning = createMeaning("eligible.response-only.hello");
    meaning.samples = [{ language: "en-US", text: "hello there" }];
    pkg.contents.meanings.push(contribution(meaning.id, meaning));
    const behavior = structuredClone(workspace.packages[0].contents.behaviors[0].value);
    behavior.id = "eligible.response-only.behavior";
    behavior.meaning = meaning.id;
    behavior.responses[0].id = "eligible.response-only.response";
    behavior.responses[0].texts = [{ language: "en-US", variants: ["Hi"] }, { language: "fa-IR", variants: ["سلام"] }];
    pkg.contents.behaviors.push(contribution(behavior.id, behavior));
  }
  const responseRows = eligibility(responseOnly);
  assert.equal(responseRows.get("eligible.response-only").eligible, true, "a fa-IR response text needs no fa-IR Semantic Profile");
  responseOnly = studioModel.addPackageToBot(responseOnly, "eligible.response-only");
  assert.ok(studioModel.selectedBot(responseOnly).package_ids.includes("eligible.response-only"));

  // A dependency's languages block the dependent Package, not just the root.
  let dependency = studioModel.addProjectPackage(englishOnly, "eligible.leaf");
  dependency = studioModel.addProjectPackage(dependency, "eligible.root");
  {
    const leaf = studioModel.selectedProject(dependency).packages.find((row) => row.manifest.id === "eligible.leaf");
    const meaning = createMeaning("eligible.leaf.salaam");
    meaning.samples = [{ language: "fa-IR", text: "سلام" }];
    leaf.contents.meanings.push(contribution(meaning.id, meaning));
    studioModel.selectedProject(dependency).packages.find((row) => row.manifest.id === "eligible.root").manifest.dependencies.push({ id: "eligible.leaf", reexport: false });
  }
  const dependencyRows = eligibility(dependency);
  assert.equal(dependencyRows.get("eligible.leaf").eligible, false, "fa-IR matcher evidence in a leaf Package needs fa-IR enabled");
  assert.deepEqual(dependencyRows.get("eligible.leaf").missing_languages, ["fa-IR"]);
  assert.equal(dependencyRows.get("eligible.root").eligible, false, "a Package inherits its required dependency's language needs");
  assert.deepEqual(dependencyRows.get("eligible.root").missing_languages, ["fa-IR"]);
  assert.equal(eligibility(enable(dependency, DEFAULT_SHARED_LANGUAGES)).get("eligible.root").eligible, true, "enabling the language makes the whole graph eligible");

  // Fallback Packages obey the identical rule through the same resolver.
  let fallbackStudio = studioModel.addProjectPackage(englishOnly, "eligible.recovery", "fallback");
  {
    const pkg = studioModel.selectedProject(fallbackStudio).packages.find((row) => row.manifest.id === "eligible.recovery");
    const behavior = createFallbackBehavior("eligible.recovery.unresolved");
    behavior.responses[0].kind = "fallback";
    behavior.responses[0].texts = [{ language: "en-US", variants: ["Sorry."] }];
    pkg.contents.fallback_behaviors.push(fallbackContribution(behavior.id, behavior));
  }
  assert.equal(eligibility(fallbackStudio, "fallback").get("eligible.recovery").eligible, true);
  // The rule is precise, not blanket: a Fallback Package cannot hold Meanings, so its bilingual
  // response texts need only the Project catalog and stay eligible for an EN-only Bot.
  assert.equal(eligibility(fallbackStudio, "fallback").get(DEFAULT_FALLBACK_PACKAGE_IDS.formal).eligible, true, "fa-IR fallback response text needs no fa-IR Semantic Profile");
  assert.doesNotThrow(() => studioModel.setBotFallbackPackage(fallbackStudio, DEFAULT_FALLBACK_PACKAGE_IDS.formal));
  assert.doesNotThrow(() => studioModel.setBotFallbackPackage(fallbackStudio, "eligible.recovery"));
  // Dropping fa-IR from the Project catalog is what makes that same Fallback Package ineligible.
  const narrowProject = studioModel.cloneStudioWorkspace(fallbackStudio);
  narrowProject.projects[0].matcher_profiles = narrowProject.projects[0].matcher_profiles.filter((row) => row.language !== "fa-IR");
  assert.equal(eligibility(narrowProject, "fallback").get(DEFAULT_FALLBACK_PACKAGE_IDS.formal).eligible, false, "an authored language outside the Project catalog blocks selection");
  assert.deepEqual(eligibility(narrowProject, "fallback").get(DEFAULT_FALLBACK_PACKAGE_IDS.formal).missing_languages, ["fa-IR"]);
  assert.throws(() => studioModel.setBotFallbackPackage(narrowProject, DEFAULT_FALLBACK_PACKAGE_IDS.formal), /not available/i);

  // Every eligible Package actually compiles: the rule is proven against real source, not asserted.
  const compilable = studioModel.setBotPackages(bilingual, [DEFAULT_STANDARD_PACKAGE_ID]);
  assert.deepEqual(studioModel.botMissingMatcherLanguages(compilable), []);
  assert.deepEqual(
    projectSourceDocument(studioModel.resolveSelectedBrain(compilable)).enabled_languages,
    DEFAULT_SHARED_LANGUAGES,
    "the compiled target carries exactly the Bot languages the eligibility rule checked",
  );

  // An enabled language whose Project Matcher Profile disappears still disables the Bot.
  const brokenProfile = studioModel.cloneStudioWorkspace(compilable);
  brokenProfile.projects[0].matcher_profiles = brokenProfile.projects[0].matcher_profiles.filter((row) => row.language !== "fa-IR");
  assert.deepEqual(studioModel.botMissingMatcherLanguages(brokenProfile), ["fa-IR"]);
}
console.log("PASS package eligibility is the Bot's own compile-language rule across selection, dependencies, fallback and diagnostics");

// Workspace v1 is the only contract; alternate version tags and obsolete shapes are rejected.
const persistedStudioText = studioIo.studioWorkspaceToText(studio);
assert.ok(!("ai_providers" in JSON.parse(persistedStudioText)), "Studio persistence contains no model/provider configuration");
const persistedStudioShape = JSON.parse(persistedStudioText);
assert.ok(!("languages" in persistedStudioShape.settings));
assert.ok(!("language_ids" in persistedStudioShape.projects[0]));
assert.equal(persistedStudioShape.shared_packages[0].authoring_language, "en-US");
assert.ok(!("authoring_language" in persistedStudioShape.projects[0].bots[0].package), "Bot Package authoring language is derived, not persisted independently");
const persistedStudio = studioIo.studioWorkspaceFromText(persistedStudioText);
assert.equal(persistedStudio.version, 1);
const preLanguageBoundaryShape = JSON.parse(persistedStudioText); delete preLanguageBoundaryShape.projects[0].bots[0].enabled_languages;
assert.throws(() => studioIo.studioWorkspaceFromText(JSON.stringify(preLanguageBoundaryShape)), /enabled_languages|missing/i);
const preAuthoringPreferenceShape = JSON.parse(persistedStudioText); delete preAuthoringPreferenceShape.shared_packages[0].authoring_language;
assert.throws(() => studioIo.studioWorkspaceFromText(JSON.stringify(preAuthoringPreferenceShape)), /authoring_language|missing/i);
const oldStudioVersion = JSON.parse(studioIo.studioWorkspaceToText(studio)); oldStudioVersion.version = 999;
assert.throws(() => studioIo.studioWorkspaceFromText(JSON.stringify(oldStudioVersion)), /Version 1|Unsupported/i);
const oldBotPackagesShape = JSON.parse(studioIo.studioWorkspaceToText(studio)); oldBotPackagesShape.projects[0].bots[0].packages = [oldBotPackagesShape.projects[0].bots[0].package]; delete oldBotPackagesShape.projects[0].bots[0].package;
assert.throws(() => studioIo.studioWorkspaceFromText(JSON.stringify(oldBotPackagesShape)), /unsupported|missing|package/i);
const oldProjectShape = JSON.parse(studioIo.studioWorkspaceToText(studio)); oldProjectShape.projects[0].shared_package_ids = []; oldProjectShape.projects[0].shared_overrides = [];
assert.throws(() => studioIo.studioWorkspaceFromText(JSON.stringify(oldProjectShape)), /unsupported|property|shared_package_ids|shared_overrides/i);
const obsoleteSettingsLanguageCatalog = JSON.parse(persistedStudioText); obsoleteSettingsLanguageCatalog.settings.languages = DEFAULT_SHARED_LANGUAGES;
assert.throws(() => studioIo.studioWorkspaceFromText(JSON.stringify(obsoleteSettingsLanguageCatalog)), /unsupported|property|languages/i);
const obsoleteProjectLanguageCatalog = JSON.parse(persistedStudioText); obsoleteProjectLanguageCatalog.projects[0].language_ids = DEFAULT_SHARED_LANGUAGES;
assert.throws(() => studioIo.studioWorkspaceFromText(JSON.stringify(obsoleteProjectLanguageCatalog)), /unsupported|property|language_ids/i);
const obsoleteProviderShape = JSON.parse(studioIo.studioWorkspaceToText(studio)); obsoleteProviderShape.ai_providers = [];
assert.throws(() => studioIo.studioWorkspaceFromText(JSON.stringify(obsoleteProviderShape)), /unsupported|property|ai_providers/i);
console.log("PASS Studio v1 rejects pre-language-boundary, alternate-version, and obsolete shapes without migration");

// Starter/default resources remain removable. The Bot Package is deleted only with its Bot.
let deletionStudio = createDefaultStudio();
const ownedBotPackageId = studioModel.selectedBot(deletionStudio).package.manifest.id;
deletionStudio = studioModel.removeBot(deletionStudio, "gvya-bot");
assert.equal(deletionStudio.projects[0].bots.length, 0);
assert.ok(!JSON.stringify(deletionStudio).includes(ownedBotPackageId));
assert.doesNotThrow(() => studioIo.studioWorkspaceFromText(studioIo.studioWorkspaceToText(deletionStudio)));
deletionStudio = studioModel.removeProject(deletionStudio, "gvya-project");
assert.equal(deletionStudio.projects.length, 0);
assert.doesNotThrow(() => studioIo.studioWorkspaceFromText(studioIo.studioWorkspaceToText(deletionStudio)));
console.log("PASS Bot Package lifetime is exactly Bot lifetime");

// Shared library deletion clears live Bot references and Bot-owned replacements.
let packageDeletionStudio = createDefaultStudio();
packageDeletionStudio = studioModel.addSharedFallbackPackage(packageDeletionStudio, "fallback.delete-me");
packageDeletionStudio = studioModel.setBotFallbackPackage(packageDeletionStudio, "fallback.delete-me");
packageDeletionStudio = studioModel.removeSharedPackage(packageDeletionStudio, "fallback.delete-me");
assert.equal(studioModel.selectedBot(packageDeletionStudio).fallback_package_id, null);
assert.ok(!packageDeletionStudio.projects[0].packages.some((pkg) => pkg.manifest.id === "fallback.delete-me"));
packageDeletionStudio = studioModel.addPackageToBot(packageDeletionStudio, DEFAULT_STANDARD_PACKAGE_ID);
const packageDeletionOwnedBehaviorCount = studioModel.selectedBot(packageDeletionStudio).package.contents.behaviors.length;
packageDeletionStudio = studioModel.overrideContribution(packageDeletionStudio, DEFAULT_STANDARD_PACKAGE_ID, "behaviors", DEFAULT_STANDARD_BEHAVIOR_ID);
const structuralBotPackageId = studioModel.selectedBot(packageDeletionStudio).package.manifest.id;
packageDeletionStudio = studioModel.removeSharedPackage(packageDeletionStudio, DEFAULT_STANDARD_PACKAGE_ID);
assert.ok(!packageDeletionStudio.shared_packages.some((row) => row.manifest.id === DEFAULT_STANDARD_PACKAGE_ID));
assert.ok(!packageDeletionStudio.projects[0].bots[0].package_ids.includes(DEFAULT_STANDARD_PACKAGE_ID));
assert.equal(packageDeletionStudio.projects[0].bots[0].package.contents.behaviors.length, packageDeletionOwnedBehaviorCount);
assert.equal(packageDeletionStudio.projects[0].bots[0].package.manifest.id, structuralBotPackageId);
let sharedDependencyStudio = createDefaultStudio();
sharedDependencyStudio = studioModel.addSharedPackage(sharedDependencyStudio, "shared.dependent");
sharedDependencyStudio.shared_packages.find((pkg) => pkg.manifest.id === "shared.dependent").manifest.dependencies.push({ id: DEFAULT_STANDARD_PACKAGE_ID, reexport: false });
assert.throws(() => studioModel.removeSharedPackage(sharedDependencyStudio, DEFAULT_STANDARD_PACKAGE_ID), /required by shared\.dependent/u);
console.log("PASS Shared deletion cleans live Bot references and blocks dangling Shared dependencies");

console.log("PASS Studio hierarchy resolves away before canonical compiler source");
