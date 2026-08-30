import type {
  AdmissionPredicate,
  AssetDefinition,
  BehaviorDefinition, CapabilityResultBehavior,
  FallbackBehaviorDefinition,
  CapabilityBinding,
  CapabilityDefinition,
  CapabilityPolicy,
  Contribution,
  ConversationConfig,
  ConversationScenario,
  JsonObject,
  JsonValue,
  LocalizedTexts,
  MeaningDefinition,
  OpeningDefinition,
  PackageContents,
  RegressionCase,
  ResponseDefinition,
  RuntimeContext,
  SemanticConfig,
  StudioPackage,
  StudioBrainWorkspace,
  TurnExpectation,
  ValueCondition,
  ValueRequirement,
} from "./types.js";
import { languageProfilePath, languageProfileSourceDocument, matcherProfilePath, matcherProfileSourceDocument } from "./matcher-profiles.js";

const DEFAULT_SEMANTIC: SemanticConfig = {
  candidate_limit: 120,
  resolution_threshold: 0.45,
  ambiguity_margin: 0.04,
  resolver_min_confidence: 0.55,
  resolver_candidate_limit: 8,
};

const DEFAULT_CONVERSATION: ConversationConfig = {
  default_topic_ttl: 3,
  default_followup_ttl: 2,
  recent_response_limit: 8,
  recent_variant_limit: 4,
  recent_user_window: 4,
  repeat_detection_window: 3,
  repeat_detection_threshold: 2,
  max_messages_per_turn: 4,
  repair_candidate_min_score: 0.40,
  author_numbers: [],
  topic_preference_margin: 0.04,
};

export function emptyContents(): PackageContents {
  return {
    meanings: [],
    behaviors: [],
    capability_result_behaviors: [],
    openings: [],
    fallback_behaviors: [],
    style_lexicons: [],
    capabilities: [],
    capability_bindings: [],
    capability_policies: [],
    capability_configs: [],
    types: [],
    assets: [],
    regression_cases: [],
    scenarios: [],
  };
}

export function createEmptyBrainWorkspace(): StudioBrainWorkspace {
  const pkg = createPackage("base", "Core authored behavior");
  return {
    format: "gvya.studio.brain-view",
    version: 1,
    project_id: "new-project",
    brain_id: "main-brain",
    languages: ["en-US"],
    enabled_languages: ["en-US"],
    default_language: "en-US",
    authoring_language: "en-US",
    emit_debug_map: true,
    semantic: { ...DEFAULT_SEMANTIC },
    conversation: { ...DEFAULT_CONVERSATION },
    matcher_profiles: [],
    packages: [pkg],
    selectedPackageId: pkg.manifest.id,
    updatedSerial: 1,
  };
}

export function createStarterBrainWorkspace(): StudioBrainWorkspace {
  const pkg = createPackage("conversation", "Starter conversational behavior");
  const hello = createMeaning("greeting.hello");
  hello.samples = ["hello", "hi there", "good morning"].map((text) => ({ language: "en-US", text }));
  hello.negative_samples = [{ language: "en-US", text: "say hello to Alex" }];
  hello.class = "social";
  hello.priority = 3;

  const response = createResponse("greeting.hello.response");
  response.texts = [{ language: "en-US", variants: ["Hello.", "Hi. How can I help?"] }];
  const behavior = createBehavior("greeting.hello.behavior", hello.id);
  behavior.responses = [response];

  const test = createRegressionCase("greeting.hello.basic");
  test.input = "hello";
  test.language = "en-US";
  test.expectation.meaning = hello.id;
  test.expectation.response_ids = [response.id];

  pkg.contents.meanings.push(contribution(hello.id, hello));
  pkg.contents.behaviors.push(contribution(behavior.id, behavior));
  pkg.contents.regression_cases.push(contribution(test.id, test));

  return {
    format: "gvya.studio.brain-view",
    version: 1,
    project_id: "starter-project",
    brain_id: "main-brain",
    languages: ["en-US"],
    enabled_languages: ["en-US"],
    default_language: "en-US",
    authoring_language: "en-US",
    emit_debug_map: true,
    semantic: { ...DEFAULT_SEMANTIC },
    conversation: { ...DEFAULT_CONVERSATION },
    matcher_profiles: [],
    packages: [pkg],
    selectedPackageId: pkg.manifest.id,
    updatedSerial: 1,
  };
}

export function createPackage(id: string, description = "", kind: "standard" | "fallback" = "standard", authoringLanguage = "en-US"): StudioPackage {
  return {
    path: `packages/${id}/package.json`,
    authoring_language: authoringLanguage,
    manifest: { id, kind, description, dependencies: [] },
    contents: emptyContents(),
  };
}

export function contribution<T>(id: string, value: T): Contribution<T> {
  return { id, exported: true, mode: "add", value };
}

export function createMeaning(id: string, language = "en-US"): MeaningDefinition {
  return {
    id,
    class: "general",
    patterns: [],
    samples: [{ language, text: "" }],
    negative_samples: [],
    retrieval_terms: [],
    priority: 1,
    positive_assumption: false,
    slots: [],
    references: [],
  };
}

export function createBehavior(id: string, meaning = "", language = "en-US"): BehaviorDefinition {
  return {
    id,
    meaning,
    topic: "",
    topic_scoped: false,
    activates_topic: false,
    topic_ttl: null,
    followup_scope: "",
    repair_continuation_candidate: false,
    repeat_same_input_after: null,
    repeat_same_meaning_after: null,
    requires_values: [],
    forbidden_values: [],
    responses: [createResponse(`${id}.response`, language)],
  };
}

export function createFallbackBehavior(id: string, language = "en-US"): FallbackBehaviorDefinition {
  return {
    id,
    trigger: "unresolved",
    priority: 0,
    conditions: [],
    responses: [createResponse(`${id}.response`, language)],
  };
}

export function fallbackContribution<T>(id: string, value: T): Contribution<T> {
  return { id, exported: false, mode: "add", value };
}

export function createResponse(id: string, language = "en-US"): ResponseDefinition {
  return {
    id,
    kind: "normal",
    texts: [{ language, variants: [""] }],
    conditions: [],
    hint_level: null,
    repeat_stage: "",
    effects: [],
    opens_followup: null,
    extra_messages: [],
    assets: [],
    links: [],
  };
}

export function createCapability(id: string): CapabilityDefinition {
  return {
    contract: {
      id,
      version: "1",
      title: humanize(id),
      description: "",
      input_schema: { type: "object", properties: {}, additionalProperties: false },
      output_schema: null,
      reference_kinds: [],
      effect_class: "pure",
      confirmation_hint: "never",
    },
    host_effects: [],
  };
}

export function createBinding(id: string, capability = ""): CapabilityBinding {
  return {
    id,
    trigger: { meaning: "", behavior: "", response: "" },
    capability,
    arguments: [],
  };
}

export function createAdmissionPredicate(): AdmissionPredicate {
  return { namespace: "context", path: "", op: "exists", value: null, hasValue: false };
}

export function createPolicy(id: string, capability = ""): CapabilityPolicy {
  return { id, capability, priority: 0, conditions: [], effect: { type: "allow", reason_code: "" } };
}

export function createValueCondition(): ValueCondition {
  return { namespace: "author", path: "", op: "exists", value: null, hasValue: false };
}

export function createExpectation(): TurnExpectation {
  return {
    meaning: "",
    forbidden_meanings: [],
    meaning_slots: {},
    meaning_references: [],
    min_semantic_score: null,
    conversation_mode: "",
    response_ids: [],
    forbidden_response_ids: [],
    response_contains: [],
    response_not_contains: [],
    author_values: {},
    conversation_values: {},
    active_topic: "",
    active_followup: "",
    capabilities: [],
    proposal_receipts: [],
    forbidden_capabilities: [],
    capability_result_accepted: null,
    capability_result_reason_code: "",
    why_codes: [],
    forbidden_why_codes: [],
  };
}

export function emptyRuntimeContext(): RuntimeContext {
  return { values: {}, visible_references: [], available_capabilities: [] };
}

export function createRegressionCase(id: string, language = "en-US"): RegressionCase {
  return {
    id,
    description: "",
    input: "",
    language,
    context: emptyRuntimeContext(),
    initial_state: {},
    seed: 1,
    unix_time_ms: null,
    expectation: createExpectation(),
    generated: false,
  };
}

export function createScenario(id: string, language = "en-US"): ConversationScenario {
  return {
    id,
    description: "",
    context: emptyRuntimeContext(),
    initial_state: {},
    steps: [{ type: "turn", say: "", language, context: null, reference_candidates: [], resolver_context: {}, hint: { type: "none" }, seed: 1, unix_time_ms: null, expectation: createExpectation() }],
    generated: false,
  };
}

export function createAsset(id: string): AssetDefinition {
  return { id, media_type: "application/octet-stream", logical_path: `assets/${id}`, source: `assets/${id}` };
}

export function selectedPackage(workspace: StudioBrainWorkspace): StudioPackage {
  return workspace.packages.find((pkg) => pkg.manifest.id === workspace.selectedPackageId) ?? workspace.packages[0] ?? createPackage("base");
}

export function touch(workspace: StudioBrainWorkspace): StudioBrainWorkspace {
  return { ...workspace, updatedSerial: workspace.updatedSerial + 1 };
}

export function cloneBrainWorkspace(workspace: StudioBrainWorkspace): StudioBrainWorkspace {
  return JSON.parse(JSON.stringify(workspace)) as StudioBrainWorkspace;
}

export function uniqueId(existing: Iterable<string>, preferred: string): string {
  const used = new Set(existing);
  if (!used.has(preferred)) return preferred;
  let i = 2;
  while (used.has(`${preferred}.${i}`)) i += 1;
  return `${preferred}.${i}`;
}

export function normalizeSample(text: string): string {
  return text.trim().toLowerCase().replace(/\s+/gu, " ");
}

export function humanize(id: string): string {
  const value = id.split(/[./_-]+/u).filter(Boolean).join(" ");
  return value.length === 0 ? "New item" : value.replace(/\b\p{L}/gu, (m) => m.toUpperCase());
}

function omitEmptyString(target: JsonObject, key: string, value: string): void {
  if (value.trim() !== "") target[key] = value;
}

function omitNullNumber(target: JsonObject, key: string, value: number | null): void {
  if (value !== null) target[key] = value;
}

function serializeCondition(row: ValueCondition | AdmissionPredicate): JsonObject {
  const out: JsonObject = { namespace: row.namespace, path: row.path, op: row.op };
  if (row.hasValue) out.value = row.value;
  return out;
}

function serializeRequirement(row: ValueRequirement): JsonObject {
  return { namespace: row.namespace, path: row.path, value: row.value };
}

function serializeTexts(rows: LocalizedTexts[]): JsonValue[] {
  return rows
    .filter((row) => row.language.trim() !== "" && row.variants.some((v) => v.trim() !== ""))
    .map((row) => ({ language: row.language, variants: row.variants.filter((v) => v.trim() !== "") }));
}

function serializeResponse(response: ResponseDefinition): JsonObject {
  const out: JsonObject = {
    id: response.id,
    kind: response.kind,
    texts: serializeTexts(response.texts),
  };
  if (response.conditions.length) out.conditions = response.conditions.map(serializeCondition);
  omitNullNumber(out, "hint_level", response.hint_level);
  omitEmptyString(out, "repeat_stage", response.repeat_stage);
  if (response.effects.length) {
    out.effects = response.effects.map((effect): JsonObject =>
      effect.type === "assign"
        ? { type: "assign", target: effect.target as unknown as JsonValue, value: effect.value }
        : { type: "increment", target: effect.target as unknown as JsonValue, delta: effect.delta },
    );
  }
  if (response.opens_followup && response.opens_followup.id.trim() !== "") {
    out.opens_followup = { id: response.opens_followup.id, ttl: response.opens_followup.ttl, refresh_if_same: response.opens_followup.refresh_if_same };
  }
  if (response.extra_messages.length) {
    out.extra_messages = response.extra_messages.map((row) => ({ chance: row.chance, texts: serializeTexts(row.texts) }));
  }
  if (response.assets.length) out.assets = response.assets.filter((row) => row.asset_id.trim() !== "").map((row) => {
    const asset: JsonObject = { asset_id: row.asset_id };
    omitEmptyString(asset, "alt_text", row.alt_text);
    return asset;
  });
  if (response.links.length) out.links = response.links.filter((row) => row.url.trim() !== "").map((row) => ({ label: row.label, url: row.url }));
  return out;
}

function serializeMeaning(value: MeaningDefinition): JsonObject {
  const out: JsonObject = {
    id: value.id,
    class: value.class,
    patterns: value.patterns
      .filter((row) => row.language.trim() !== "" && row.text.trim() !== "")
      .map((row) => ({ language: row.language, text: row.text, priority: row.priority })),
    samples: value.samples
      .filter((row) => row.language.trim() !== "" && row.text.trim() !== "")
      .map((row) => ({ language: row.language, text: row.text })),
    negative_samples: value.negative_samples.filter((row) => row.language.trim() !== "" && row.text.trim() !== "").map((row) => ({ language: row.language, text: row.text })),
    retrieval_terms: value.retrieval_terms.filter((row) => row.language.trim() !== "" && row.text.trim() !== "").map((row) => ({ language: row.language, text: row.text })),
    priority: value.priority,
    positive_assumption: value.positive_assumption,
  };
  if (value.slots.length) {
    out.slots = value.slots.filter((row) => row.name.trim() !== "").map((row) => {
      const slot: JsonObject = { name: row.name, type: row.type, required: row.required };
      if (row.type === "entity") slot.entity_kind = row.entity_kind;
      if (row.type === "reference") slot.reference_kind = row.reference_kind;
      if (row.elicitation.length) {
        slot.elicitation = row.elicitation
          .filter((prompt) => prompt.language.trim() !== "" && prompt.text.trim() !== "")
          .map((prompt) => ({ language: prompt.language, text: prompt.text }));
      }
      return slot;
    });
  }
  if (value.references.length) {
    out.references = value.references.filter((row) => row.kind.trim() !== "").map((row) => {
      const reference: JsonObject = { kind: row.kind, required: row.required };
      if (row.elicitation.length) {
        reference.elicitation = row.elicitation
          .filter((prompt) => prompt.language.trim() !== "" && prompt.text.trim() !== "")
          .map((prompt) => ({ language: prompt.language, text: prompt.text }));
      }
      return reference;
    });
  }
  return out;
}

function serializeBehavior(value: BehaviorDefinition): JsonObject {
  const out: JsonObject = {
    id: value.id,
    meaning: value.meaning,
    topic_scoped: value.topic_scoped,
    activates_topic: value.activates_topic,
    responses: value.responses.map(serializeResponse),
  };
  omitEmptyString(out, "topic", value.topic);
  omitNullNumber(out, "topic_ttl", value.topic_ttl);
  omitEmptyString(out, "followup_scope", value.followup_scope);
  if (value.repair_continuation_candidate) out.repair_continuation_candidate = true;
  omitNullNumber(out, "repeat_same_input_after", value.repeat_same_input_after);
  omitNullNumber(out, "repeat_same_meaning_after", value.repeat_same_meaning_after);
  if (value.requires_values.length) out.requires_values = value.requires_values.map(serializeRequirement);
  if (value.forbidden_values.length) out.forbidden_values = value.forbidden_values.map(serializeRequirement);
  return out;
}

function serializeFallbackBehavior(value: FallbackBehaviorDefinition): JsonObject {
  const out: JsonObject = {
    id: value.id,
    trigger: value.trigger,
    priority: value.priority,
    responses: value.responses.map(serializeResponse),
  };
  if (value.conditions.length) out.conditions = value.conditions.map(serializeCondition);
  return out;
}

function serializeCapabilityResultBehavior(value: CapabilityResultBehavior): JsonObject {
  const out: JsonObject = {
    id: value.id,
    capability: value.capability,
    capability_version: value.capability_version,
    responses: value.responses.map(serializeResponse),
  };
  if (value.succeeded !== null) out.succeeded = value.succeeded;
  omitEmptyString(out, "error_code", value.error_code);
  return out;
}

function serializeOpening(value: OpeningDefinition): JsonObject {
  const out: JsonObject = {
    id: value.id,
    responses: value.responses.map(serializeResponse),
  };
  omitEmptyString(out, "topic", value.topic);
  omitNullNumber(out, "topic_ttl", value.topic_ttl);
  return out;
}

function serializeCapability(value: CapabilityDefinition): JsonObject {
  const contract: JsonObject = {
    id: value.contract.id,
    version: value.contract.version,
    title: value.contract.title,
    description: value.contract.description,
    input_schema: value.contract.input_schema,
    reference_kinds: value.contract.reference_kinds.filter(Boolean),
    effect_class: value.contract.effect_class,
    confirmation_hint: value.contract.confirmation_hint,
  };
  if (value.contract.output_schema !== null) contract.output_schema = value.contract.output_schema;
  return {
    contract,
    host_effects: value.host_effects.filter((row) => row.resource.trim() !== "").map((row) => ({ resource: row.resource, kind: row.kind, summary: row.summary })),
  };
}

function serializeBinding(value: CapabilityBinding): JsonObject {
  const trigger: JsonObject = {};
  omitEmptyString(trigger, "meaning", value.trigger.meaning);
  omitEmptyString(trigger, "behavior", value.trigger.behavior);
  omitEmptyString(trigger, "response", value.trigger.response);
  const argumentsRows = value.arguments.filter((row) => row.target.trim() !== "").map((row) => {
    const source: JsonObject = { type: row.source.type };
    if (row.source.type === "meaning_slot") source.name = row.source.name;
    if (row.source.type === "meaning_reference" || row.source.type === "focus_reference") {
      source.kind = row.source.kind;
      source.projection = row.source.projection;
    }
    if (row.source.type === "context_path" || row.source.type === "author_state_path") source.path = row.source.path;
    if (row.source.type === "literal") source.value = row.source.value;
    return { target: row.target, source };
  });
  return { id: value.id, trigger, capability: value.capability, arguments: argumentsRows };
}

function serializePolicy(value: CapabilityPolicy): JsonObject {
  const effect: JsonObject = { type: value.effect.type };
  if (value.effect.type !== "allow") effect.reason_code = value.effect.reason_code;
  return {
    id: value.id,
    capability: value.capability,
    priority: value.priority,
    conditions: value.conditions.map(serializeCondition),
    effect,
  };
}

function serializeExpectation(expectation: TurnExpectation): JsonObject {
  const out: JsonObject = {};
  omitEmptyString(out, "meaning", expectation.meaning);
  if (expectation.forbidden_meanings.length) out.forbidden_meanings = expectation.forbidden_meanings.filter(Boolean);
  if (Object.keys(expectation.meaning_slots).length) out.meaning_slots = expectation.meaning_slots;
  if (expectation.meaning_references.length) out.meaning_references = expectation.meaning_references;
  omitNullNumber(out, "min_semantic_score", expectation.min_semantic_score);
  omitEmptyString(out, "conversation_mode", expectation.conversation_mode);
  if (expectation.response_ids.length) out.response_ids = expectation.response_ids.filter(Boolean);
  if (expectation.forbidden_response_ids.length) out.forbidden_response_ids = expectation.forbidden_response_ids.filter(Boolean);
  if (expectation.response_contains.length) out.response_contains = expectation.response_contains.filter(Boolean);
  if (expectation.response_not_contains.length) out.response_not_contains = expectation.response_not_contains.filter(Boolean);
  if (Object.keys(expectation.author_values).length) out.author_values = expectation.author_values;
  if (Object.keys(expectation.conversation_values).length) out.conversation_values = expectation.conversation_values;
  omitEmptyString(out, "active_topic", expectation.active_topic);
  omitEmptyString(out, "active_followup", expectation.active_followup);
  if (expectation.capabilities.length) {
    out.capabilities = expectation.capabilities.filter((row) => row.id.trim() !== "").map((row) => {
      const cap: JsonObject = { id: row.id };
      omitEmptyString(cap, "version", row.version);
      if (row.arguments !== null) cap.arguments = row.arguments;
      return cap;
    });
  }
  if (expectation.proposal_receipts.length) {
    out.proposal_receipts = expectation.proposal_receipts.filter((row) => row.id.trim() !== "").map((row) => {
      const receipt: JsonObject = { id: row.id, outcome: row.outcome };
      omitEmptyString(receipt, "version", row.version);
      if (row.arguments !== null) receipt.arguments = row.arguments;
      omitEmptyString(receipt, "reason_code", row.reason_code);
      return receipt;
    });
  }
  if (expectation.forbidden_capabilities.length) out.forbidden_capabilities = expectation.forbidden_capabilities.filter(Boolean);
  if (expectation.capability_result_accepted !== null) out.capability_result_accepted = expectation.capability_result_accepted;
  omitEmptyString(out, "capability_result_reason_code", expectation.capability_result_reason_code);
  if (expectation.why_codes.length) out.why_codes = expectation.why_codes.filter(Boolean);
  if (expectation.forbidden_why_codes.length) out.forbidden_why_codes = expectation.forbidden_why_codes.filter(Boolean);
  return out;
}

function serializeRuntimeContext(context: RegressionCase["context"]): JsonObject {
  const out: JsonObject = {};
  if (Object.keys(context.values).length) out.values = context.values;
  if (context.visible_references.length) out.visible_references = context.visible_references;
  if (context.available_capabilities.length) out.available_capabilities = context.available_capabilities;
  return out;
}

function serializeRegression(value: RegressionCase): JsonObject {
  const out: JsonObject = {
    id: value.id,
    description: value.description,
    input: value.input,
    context: serializeRuntimeContext(value.context),
    initial_state: value.initial_state,
    expectation: serializeExpectation(value.expectation),
    generated: value.generated,
  };
  omitEmptyString(out, "language", value.language);
  omitNullNumber(out, "seed", value.seed);
  omitNullNumber(out, "unix_time_ms", value.unix_time_ms);
  return out;
}

function serializeScenario(value: ConversationScenario): JsonObject {
  return {
    id: value.id,
    description: value.description,
    context: serializeRuntimeContext(value.context),
    initial_state: value.initial_state,
    steps: value.steps.map(serializeScenarioStep),
    generated: value.generated,
  };
}

function serializeScenarioStep(step: ConversationScenario["steps"][number]): JsonObject {
  const out: JsonObject = { type: step.type, expectation: serializeExpectation(step.expectation) };
  if (step.context !== null) out.context = serializeRuntimeContext(step.context);
  omitNullNumber(out, "unix_time_ms", step.unix_time_ms);
  switch (step.type) {
    case "open":
      omitEmptyString(out, "language", step.language);
      omitNullNumber(out, "seed", step.seed);
      return out;
    case "turn":
      out.say = step.say;
      omitEmptyString(out, "language", step.language);
      if (step.reference_candidates.length) out.reference_candidates = step.reference_candidates.map((candidate) => {
        const row: JsonObject = { reference: candidate.reference };
        omitEmptyString(row, "label", candidate.label);
        if (candidate.aliases.length) row.aliases = candidate.aliases.filter(Boolean);
        return row;
      });
      if (Object.keys(step.resolver_context).length) out.resolver_context = step.resolver_context;
      if (step.hint.type !== "none") out.hint = step.hint.type === "direct" ? { type: "direct", level: step.hint.level } : { type: step.hint.type };
      omitNullNumber(out, "seed", step.seed);
      return out;
    case "capability_result":
      out.proposal_from_step = step.proposal_from_step;
      omitEmptyString(out, "proposal_capability", step.proposal_capability);
      omitNullNumber(out, "proposal_ordinal", step.proposal_ordinal);
      out.succeeded = step.succeeded;
      if (step.output !== undefined) out.output = step.output;
      omitEmptyString(out, "error_code", step.error_code);
      omitEmptyString(out, "language", step.language);
      omitNullNumber(out, "seed", step.seed);
      return out;
    case "confirm":
      out.proposal_from_step = step.proposal_from_step;
      omitEmptyString(out, "proposal_capability", step.proposal_capability);
      omitNullNumber(out, "proposal_ordinal", step.proposal_ordinal);
      out.confirmed = step.confirmed;
      return out;
  }
}

function serializeContribution<T>(row: Contribution<T>, value: JsonValue): JsonObject {
  return { id: row.id, exported: row.exported, mode: row.mode as unknown as JsonValue, value };
}

function rawContributions<T extends JsonValue>(rows: Contribution<T>[]): JsonValue[] {
  return rows.map((row) => serializeContribution(row, row.value));
}

function packageContentsDocument(pkg: StudioPackage): JsonObject {
  return {
    meanings: pkg.contents.meanings.map((row) => serializeContribution(row, serializeMeaning(row.value))),
    behaviors: pkg.contents.behaviors.map((row) => serializeContribution(row, serializeBehavior(row.value))),
    capability_result_behaviors: pkg.contents.capability_result_behaviors.map((row) => serializeContribution(row, serializeCapabilityResultBehavior(row.value))),
    openings: pkg.contents.openings.map((row) => serializeContribution(row, serializeOpening(row.value))),
    fallback_behaviors: pkg.contents.fallback_behaviors.map((row) => serializeContribution(row, serializeFallbackBehavior(row.value))),
    style_lexicons: rawContributions(pkg.contents.style_lexicons),
    capabilities: pkg.contents.capabilities.map((row) => serializeContribution(row, serializeCapability(row.value))),
    capability_bindings: pkg.contents.capability_bindings.map((row) => serializeContribution(row, serializeBinding(row.value))),
    capability_policies: pkg.contents.capability_policies.map((row) => serializeContribution(row, serializePolicy(row.value))),
    capability_configs: rawContributions(pkg.contents.capability_configs),
    types: rawContributions(pkg.contents.types),
    assets: pkg.contents.assets.map((row) => serializeContribution(row, row.value as unknown as JsonValue)),
    regression_cases: pkg.contents.regression_cases.map((row) => serializeContribution(row, serializeRegression(row.value))),
    scenarios: pkg.contents.scenarios.map((row) => serializeContribution(row, serializeScenario(row.value))),
  };
}

/** Internal Studio persistence snapshot. This is deliberately not a GVYA source document. */
export function packageSnapshotDocument(pkg: StudioPackage): JsonObject {
  return {
    manifest: {
      id: pkg.manifest.id,
      kind: pkg.manifest.kind,
      description: pkg.manifest.description,
      dependencies: pkg.manifest.dependencies.map((row) => ({ id: row.id, reexport: row.reexport })),
    },
    contents: packageContentsDocument(pkg),
  };
}

const PACKAGE_FRAGMENT_NAMESPACES = [
  "meanings", "behaviors", "capability_result_behaviors", "openings", "fallback_behaviors",
  "style_lexicons", "capabilities", "capability_bindings", "capability_policies",
  "capability_configs", "types", "assets", "regression_cases", "scenarios",
] as const;

function packageFragmentFileName(index: number, id: string): string {
  const slug = id.toLowerCase().replace(/[^a-z0-9._-]+/g, "-").replace(/-+/g, "-").replace(/^[.-]+|[.-]+$/g, "").slice(0, 72) || "item";
  return `${String(index + 1).padStart(4, "0")}-${slug}.json`;
}

function packageDirectory(path: string): string {
  const slash = path.lastIndexOf("/");
  return slash < 0 ? "" : path.slice(0, slash + 1);
}

export function packageSourceFiles(pkg: StudioPackage): Array<{ path: string; json: JsonObject }> {
  const contents = packageContentsDocument(pkg);
  const fragments: JsonObject = {};
  const files: Array<{ path: string; json: JsonObject }> = [];
  const dir = packageDirectory(pkg.path);
  for (const namespace of PACKAGE_FRAGMENT_NAMESPACES) {
    const rows = contents[namespace];
    if (!Array.isArray(rows) || rows.length === 0) continue;
    const relativePaths: JsonValue[] = [];
    rows.forEach((row, index) => {
      if (row === null || Array.isArray(row) || typeof row !== "object") throw new Error(`Package ${pkg.manifest.id} ${namespace}[${index}] is not an object contribution.`);
      const id = typeof row.id === "string" ? row.id : `item-${index + 1}`;
      const relative = `fragments/${namespace}/${packageFragmentFileName(index, id)}`;
      relativePaths.push(relative);
      files.push({ path: `${dir}${relative}`, json: row as JsonObject });
    });
    fragments[namespace] = relativePaths;
  }
  const root: JsonObject = {
    format: "gvya.source.package",
    version: 1,
    manifest: packageSnapshotDocument(pkg).manifest as JsonValue,
    fragments,
  };
  return [{ path: pkg.path, json: root }, ...files];
}

export function projectSourceDocument(workspace: StudioBrainWorkspace): JsonObject {
  const standardPackages = workspace.packages.filter((pkg) => pkg.manifest.kind === "standard");
  const fallbackPackages = workspace.packages.filter((pkg) => pkg.manifest.kind === "fallback");
  if (fallbackPackages.length > 1) throw new Error("A Brain may select at most one Fallback Package.");
  return {
    format: "gvya.source.project",
    version: 1,
    project_id: workspace.project_id,
    brain_id: workspace.brain_id,
    languages: [...workspace.languages],
    enabled_languages: [...workspace.enabled_languages],
    default_language: workspace.default_language,
    language_profiles: workspace.matcher_profiles.map((profile) => languageProfilePath(profile.language)),
    matcher_profiles: workspace.matcher_profiles.map((profile) => matcherProfilePath(profile.language)),
    packages: standardPackages.map((pkg) => pkg.path),
    fallback_package: fallbackPackages[0]?.path ?? null,
    semantic: workspace.semantic as unknown as JsonValue,
    conversation: workspace.conversation as unknown as JsonValue,
    emit_debug_map: workspace.emit_debug_map,
  };
}

export function sourceFiles(workspace: StudioBrainWorkspace): Array<{ path: string; json: JsonObject }> {
  return [
    { path: "gvya.project.json", json: projectSourceDocument(workspace) },
    ...workspace.matcher_profiles.map((profile) => ({ path: languageProfilePath(profile.language), json: languageProfileSourceDocument(profile) })),
    ...workspace.matcher_profiles.map((profile) => ({ path: matcherProfilePath(profile.language), json: matcherProfileSourceDocument(profile) })),
    ...workspace.packages.flatMap((pkg) => packageSourceFiles(pkg)),
  ];
}

export function stableJson(value: JsonValue): string {
  return `${JSON.stringify(sortJson(value), null, 2)}\n`;
}

function sortJson(value: JsonValue): JsonValue {
  if (Array.isArray(value)) return value.map(sortJson);
  if (value !== null && typeof value === "object") {
    const out: JsonObject = {};
    for (const key of Object.keys(value).sort()) out[key] = sortJson(value[key] as JsonValue);
    return out;
  }
  return value;
}
