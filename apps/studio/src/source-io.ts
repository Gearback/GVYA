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
  MatcherProfile,
  OpeningDefinition,
  PackageContents,
  PackageDependency,
  RegressionCase,
  ResponseDefinition,
  ScenarioHint,
  StudioAssetFile,
  StudioPackage,
  StudioBrainWorkspace,
  TurnExpectation,
  ValueCondition,
  ValueRequirement,
} from "./types.js";
import {
  cloneBrainWorkspace,
  createExpectation,
  emptyContents,
  emptyRuntimeContext,
} from "./workspace.js";
import { packageAssetPath, validateAssetFiles } from "./asset-files.js";
import { compilerSourceEntries, WasmCompilerBackend, type CompilerSourceEntry } from "./compiler-wasm.js";
import { loadBundledEngineAssets } from "./engine-assets.js";
import { createTarGz, normalizeSafePath, readTarGz } from "./tar.js";
import { assertLanguageCatalog, isWellFormedLanguageTag, languageKey } from "./languages.js";
import { assertUniqueMatcherProfiles, pairProfileDocuments, parseLanguageProfileDocument, parseMatcherProfileDocument, parseProfileCatalogDocument } from "./matcher-profiles.js";

const BRAIN_VIEW_MAX_BYTES = 4 * 1024 * 1024;
const BRAIN_VIEW_MAX_NODES = 100_000;
const BRAIN_VIEW_MAX_DEPTH = 64;
const BRAIN_VIEW_MAX_STRING_BYTES = 256 * 1024;
const BRAIN_VIEW_MAX_PACKAGES = 2_048;
const SOURCE_TREE_MAX_FILES = 10_000;
const SOURCE_JSON_MAX_BYTES = 8 * 1024 * 1024;
const SOURCE_TREE_MAX_BYTES = 128 * 1024 * 1024;
const SOURCE_ARCHIVE_MAX_BYTES = 160 * 1024 * 1024;

export function brainWorkspaceFromText(text: string): StudioBrainWorkspace {
  if (new TextEncoder().encode(text).byteLength > BRAIN_VIEW_MAX_BYTES) throw new Error("Internal GVYA Studio Brain view exceeds the 4 MiB limit");
  return brainWorkspaceFromJson(JSON.parse(text) as unknown);
}

export function brainWorkspaceFromJson(value: unknown): StudioBrainWorkspace {
  validateWorkspaceShapeBudget(value);
  const row = asObject(value, "workspace");
  assertKeys(row, ["format", "version", "project_id", "brain_id", "languages", "enabled_languages", "default_language", "authoring_language", "emit_debug_map", "semantic", "conversation", "matcher_profiles", "packages", "selectedPackageId", "updatedSerial"], "workspace");
  if (row.format !== "gvya.studio.brain-view" || row.version !== 1) throw new Error("Unsupported internal GVYA Studio Brain view");

  const semantic = asObject(row.semantic, "workspace.semantic");
  assertKeys(semantic, ["candidate_limit", "resolution_threshold", "ambiguity_margin", "resolver_min_confidence", "resolver_candidate_limit"], "workspace.semantic");
  const semanticConfig = {
    candidate_limit: boundedInteger(semantic.candidate_limit, 2, 256, "workspace.semantic.candidate_limit"),
    resolution_threshold: boundedNumber(semantic.resolution_threshold, 0, 1, "workspace.semantic.resolution_threshold"),
    ambiguity_margin: boundedNumber(semantic.ambiguity_margin, 0, 1, "workspace.semantic.ambiguity_margin"),
    resolver_min_confidence: boundedNumber(semantic.resolver_min_confidence, 0, 1, "workspace.semantic.resolver_min_confidence"),
    resolver_candidate_limit: boundedInteger(semantic.resolver_candidate_limit, 1, 64, "workspace.semantic.resolver_candidate_limit"),
  };

  const conversation = asObject(row.conversation, "workspace.conversation");
  assertKeys(conversation, ["default_topic_ttl", "default_followup_ttl", "recent_response_limit", "recent_variant_limit", "recent_user_window", "repeat_detection_window", "repeat_detection_threshold", "max_messages_per_turn", "repair_candidate_min_score", "author_numbers", "topic_preference_margin"], "workspace.conversation");
  const conversationConfig = {
    default_topic_ttl: boundedInteger(conversation.default_topic_ttl, 1, 0xffff_ffff, "workspace.conversation.default_topic_ttl"),
    default_followup_ttl: boundedInteger(conversation.default_followup_ttl, 1, 0xffff_ffff, "workspace.conversation.default_followup_ttl"),
    recent_response_limit: boundedInteger(conversation.recent_response_limit, 1, 64, "workspace.conversation.recent_response_limit"),
    recent_variant_limit: boundedInteger(conversation.recent_variant_limit, 1, 64, "workspace.conversation.recent_variant_limit"),
    recent_user_window: boundedInteger(conversation.recent_user_window, 1, 50, "workspace.conversation.recent_user_window"),
    repeat_detection_window: boundedInteger(conversation.repeat_detection_window, 1, 50, "workspace.conversation.repeat_detection_window"),
    repeat_detection_threshold: boundedInteger(conversation.repeat_detection_threshold, 2, 20, "workspace.conversation.repeat_detection_threshold"),
    max_messages_per_turn: boundedInteger(conversation.max_messages_per_turn, 1, 6, "workspace.conversation.max_messages_per_turn"),
    repair_candidate_min_score: boundedNumber(conversation.repair_candidate_min_score, 0, 1, "workspace.conversation.repair_candidate_min_score"),
    author_numbers: parseAuthorNumbers(conversation.author_numbers, "workspace.conversation.author_numbers"),
    topic_preference_margin: boundedNumber(conversation.topic_preference_margin, 0, 0.25, "workspace.conversation.topic_preference_margin"),
  };

  const languages = stringArray(row.languages, "workspace.languages");
  assertLanguageCatalog(languages, "workspace.languages");
  const enabledLanguages = stringArray(row.enabled_languages, "workspace.enabled_languages");
  assertLanguageCatalog(enabledLanguages, "workspace.enabled_languages");
  if (enabledLanguages.some((language) => !languages.some((declared) => languageKey(declared) === languageKey(language)))) throw new Error("workspace.enabled_languages must be a subset of workspace.languages");
  const defaultLanguage = requiredString(row.default_language, "workspace.default_language");
  if (!enabledLanguages.some((language) => languageKey(language) === languageKey(defaultLanguage))) throw new Error("workspace.default_language must name one enabled language");
  const authoringLanguage = requiredString(row.authoring_language, "workspace.authoring_language");
  if (!languages.some((language) => languageKey(language) === languageKey(authoringLanguage))) throw new Error("workspace.authoring_language must name one declared language");
  const matcherProfiles = asArray(row.matcher_profiles, "workspace.matcher_profiles").map((raw, index) => {
    const profile = asObject(raw, `workspace.matcher_profiles[${index}]`);
    assertKeys(profile, ["language", "language_profile", "profile"], `workspace.matcher_profiles[${index}]`);
    return parseProfileCatalogDocument(profile, `workspace.matcher_profiles[${index}]`);
  });
  assertUniqueMatcherProfiles(matcherProfiles, "workspace.matcher_profiles");
  if (matcherProfiles.some((profile) => !enabledLanguages.some((language) => languageKey(language) === languageKey(profile.language)))) throw new Error("workspace.matcher_profiles must contain only enabled languages");

  const rawPackages = asArray(row.packages, "workspace.packages");
  if (rawPackages.length === 0 || rawPackages.length > BRAIN_VIEW_MAX_PACKAGES) throw new Error("Internal Brain-view package count is outside the supported range");
  const packages = rawPackages.map((raw, index) => {
    const packageRow = asObject(raw, `workspace.packages[${index}]`);
    assertKeys(packageRow, ["path", "authoring_language", "manifest", "contents"], `workspace.packages[${index}]`);
    const path = requiredString(packageRow.path, `workspace.packages[${index}].path`);
    if (!safeWorkspacePath(path)) throw new Error(`workspace.packages[${index}].path is not a safe relative source path`);
    return parsePackageSnapshot(path, { manifest: packageRow.manifest, contents: packageRow.contents }, requiredString(packageRow.authoring_language, `workspace.packages[${index}].authoring_language`));
  });
  const ids = new Set(packages.map((pkg) => pkg.manifest.id));
  const paths = new Set(packages.map((pkg) => pkg.path));
  if (ids.size !== packages.length || paths.size !== packages.length) throw new Error("Workspace contains duplicate package IDs or paths");
  const selectedPackageId = requiredString(row.selectedPackageId, "workspace.selectedPackageId");
  if (!ids.has(selectedPackageId)) throw new Error("Workspace selectedPackageId does not name an existing package");
  if (languageKey(packages.find((pkg) => pkg.manifest.id === selectedPackageId)!.authoring_language) !== languageKey(authoringLanguage)) throw new Error("workspace.authoring_language must match the selected Package authoring preference");
  const updatedSerial = boundedInteger(row.updatedSerial, 0, Number.MAX_SAFE_INTEGER, "workspace.updatedSerial");
  if (typeof row.emit_debug_map !== "boolean") throw new Error("workspace.emit_debug_map must be boolean");
  return cloneBrainWorkspace({
    format: "gvya.studio.brain-view", version: 1,
    project_id: requiredString(row.project_id, "workspace.project_id"),
    brain_id: requiredString(row.brain_id, "workspace.brain_id"),
    languages,
    enabled_languages: enabledLanguages,
    default_language: defaultLanguage,
    authoring_language: authoringLanguage,
    emit_debug_map: row.emit_debug_map, semantic: semanticConfig, conversation: conversationConfig,
    matcher_profiles: matcherProfiles,
    packages, selectedPackageId, updatedSerial,
  });
}


export interface SourceImport {
  workspace: StudioBrainWorkspace;
  assetFiles: StudioAssetFile[];
}

export const SOURCE_ARCHIVE_EXTENSION = ".gvya-source.tar.gz";
export const SOURCE_ARCHIVE_MEDIA_TYPE = "application/gzip";

const SOURCE_ARCHIVE_LIMITS = {
  maxEntries: SOURCE_TREE_MAX_FILES,
  maxEntryBytes: 32 * 1024 * 1024,
  maxTotalBytes: SOURCE_ARCHIVE_MAX_BYTES,
};

/**
 * Builds the canonical portable source archive: the exact compiler source tree in a deterministic
 * path-sorted ustar stream, wrapped in one gzip member. Identical input produces identical bytes.
 */
export async function buildSourceArchive(workspace: StudioBrainWorkspace, assetFiles: readonly StudioAssetFile[]): Promise<{ filename: string; mediaType: string; bytes: Uint8Array }> {
  const bytes = await createTarGz(await compilerSourceEntries(workspace, assetFiles));
  return { filename: `${safeDownloadStem(workspace.brain_id)}${SOURCE_ARCHIVE_EXTENSION}`, mediaType: SOURCE_ARCHIVE_MEDIA_TYPE, bytes };
}

export async function exportSourceArchive(workspace: StudioBrainWorkspace, assetFiles: readonly StudioAssetFile[]): Promise<void> {
  const archive = await buildSourceArchive(workspace, assetFiles);
  downloadBytes(archive.filename, archive.bytes, archive.mediaType);
}

export async function loadSourceArchive(file: File): Promise<SourceImport> {
  if (file.size > SOURCE_ARCHIVE_MAX_BYTES) throw new Error("GVYA source archive exceeds the 160 MiB Studio limit.");
  const entries = await readTarGz(new Uint8Array(await file.arrayBuffer()), SOURCE_ARCHIVE_LIMITS);
  return loadMappedSource(new Map(entries.map((entry) => [entry.path, new Blob([entry.bytes])])));
}

export async function loadSourceTree(files: FileList | File[]): Promise<SourceImport> {
  const rows = Array.from(files);
  if (rows.length === 0 || rows.length > SOURCE_TREE_MAX_FILES) throw new Error("GVYA source tree file count is outside the supported range");
  let totalBytes = 0;
  const mapped = new Map<string, Blob>();
  for (const file of rows) {
    totalBytes += file.size;
    if (!Number.isSafeInteger(totalBytes) || totalBytes > SOURCE_TREE_MAX_BYTES) throw new Error("GVYA source tree exceeds the 128 MiB Studio limit");
    const rel = normalizePath((file as File & { webkitRelativePath?: string }).webkitRelativePath || file.name);
    if (!safeWorkspacePath(rel)) throw new Error(`GVYA source tree contains an unsafe path: ${rel}`);
    if (mapped.has(rel)) throw new Error(`GVYA source tree contains a duplicate normalized path: ${rel}`);
    mapped.set(rel, file);
  }
  return loadMappedSource(mapped);
}

async function loadMappedSource(mapped: Map<string, Blob>): Promise<SourceImport> {
  const projectEntries = [...mapped.entries()].filter(([path]) => path.endsWith("/gvya.project.json") || path === "gvya.project.json");
  if (projectEntries.length === 0) throw new Error("Selected files do not contain gvya.project.json");
  if (projectEntries.length !== 1) throw new Error("Selected files contain multiple gvya.project.json roots; select exactly one source project.");
  const [projectPath, projectFile] = projectEntries[0]!;
  const prefix = projectPath === "gvya.project.json" ? "" : projectPath.slice(0, -"gvya.project.json".length);
  const canonicalEntries: CompilerSourceEntry[] = [];
  for (const [path, blob] of mapped) {
    if (!path.startsWith(prefix)) continue;
    const relative = path.slice(prefix.length);
    if (!relative) continue;
    if (relative.endsWith(".json") && blob.size > SOURCE_JSON_MAX_BYTES) {
      throw new Error(`${relative} exceeds the 8 MiB source JSON limit`);
    }
    canonicalEntries.push({ path: relative, bytes: new Uint8Array(await blob.arrayBuffer()) });
  }
  const engine = await loadBundledEngineAssets();
  const compiler = await WasmCompilerBackend.instantiate(engine.engineModule);
  compiler.validate(canonicalEntries);
  const project = asObject(await boundedSourceJson(projectFile, "gvya.project.json"), "gvya.project.json");
  assertKeys(project, ["format", "version", "project_id", "brain_id", "languages", "enabled_languages", "default_language", "language_profiles", "matcher_profiles", "packages", "fallback_package", "semantic", "conversation", "emit_debug_map"], "gvya.project.json");
  if (project.format !== "gvya.source.project" || project.version !== 1) throw new Error("Unsupported GVYA source project format");
  const packagePaths = stringArray(project.packages, "gvya.project.json#packages");
  const languages = stringArray(project.languages, "gvya.project.json#languages");
  assertLanguageCatalog(languages, "gvya.project.json#languages");
  const enabledLanguages = stringArray(project.enabled_languages, "gvya.project.json#enabled_languages");
  assertLanguageCatalog(enabledLanguages, "gvya.project.json#enabled_languages");
  if (enabledLanguages.some((language) => !languages.some((declared) => languageKey(declared) === languageKey(language)))) throw new Error("gvya.project.json#enabled_languages must be a subset of languages");
  const defaultLanguage = requiredString(project.default_language, "gvya.project.json#default_language");
  if (!enabledLanguages.some((language) => languageKey(language) === languageKey(defaultLanguage))) throw new Error("gvya.project.json#default_language must name one enabled language");
  const fallbackPackagePath = project.fallback_package === null ? null : requiredString(project.fallback_package, "gvya.project.json#fallback_package");
  const languageProfilePaths = stringArray(project.language_profiles, "gvya.project.json#language_profiles");
  const matcherProfilePaths = stringArray(project.matcher_profiles, "gvya.project.json#matcher_profiles");
  const languageProfiles = new Map<string, MatcherProfile>();
  for (const languageProfilePath of languageProfilePaths) {
    const file = mapped.get(normalizePath(prefix + languageProfilePath));
    if (!file) throw new Error(`Missing Language Profile source: ${languageProfilePath}`);
    const profile = parseLanguageProfileDocument(await boundedSourceJson(file, languageProfilePath), languageProfilePath);
    const key = languageKey(profile.language);
    if (languageProfiles.has(key)) throw new Error(`gvya.project.json#language_profiles contains duplicate language ${profile.language}`);
    languageProfiles.set(key, profile);
  }
  const matcherProfiles: MatcherProfile[] = [];
  for (const matcherProfilePath of matcherProfilePaths) {
    const file = mapped.get(normalizePath(prefix + matcherProfilePath));
    if (!file) throw new Error(`Missing Matcher Profile source: ${matcherProfilePath}`);
    const matcher = parseMatcherProfileDocument(await boundedSourceJson(file, matcherProfilePath), matcherProfilePath);
    const language = languageProfiles.get(languageKey(matcher.language));
    if (!language) throw new Error(`Matcher Profile ${matcherProfilePath} has no paired Language Profile.`);
    matcherProfiles.push(pairProfileDocuments(language, matcher, matcherProfilePath));
  }
  if (languageProfiles.size !== matcherProfiles.length) throw new Error("Language and Matcher Profile catalogs must contain the same languages.");
  assertUniqueMatcherProfiles(matcherProfiles, "gvya.project.json#matcher_profiles");
  if (matcherProfiles.some((profile) => !enabledLanguages.some((language) => languageKey(language) === languageKey(profile.language)))) throw new Error("Matcher Profiles must belong to enabled languages");
  if (packagePaths.length === 0 && fallbackPackagePath === null) throw new Error("GVYA source project enumerates no packages");
  const packages: StudioPackage[] = [];
  for (const packagePath of packagePaths) {
    const parsed = await loadPackageSource(mapped, prefix, packagePath, defaultLanguage);
    if (parsed.manifest.kind !== "standard") throw new Error(`Project packages entry ${packagePath} must reference a Standard Package.`);
    packages.push(parsed);
  }
  if (fallbackPackagePath !== null) {
    const parsed = await loadPackageSource(mapped, prefix, fallbackPackagePath, defaultLanguage);
    if (parsed.manifest.kind !== "fallback") throw new Error(`fallback_package ${fallbackPackagePath} must reference a Fallback Package.`);
    packages.push(parsed);
  }
  const semantic = asOptionalObject(project.semantic, "semantic");
  const conversation = asOptionalObject(project.conversation, "conversation");
  const workspace: StudioBrainWorkspace = {
    format: "gvya.studio.brain-view",
    version: 1,
    project_id: requiredString(project.project_id, "project_id"),
    brain_id: requiredString(project.brain_id, "brain_id"),
    languages,
    enabled_languages: enabledLanguages,
    default_language: defaultLanguage,
    authoring_language: defaultLanguage,
    emit_debug_map: typeof project.emit_debug_map === "boolean" ? project.emit_debug_map : false,
    semantic: {
      candidate_limit: boundedInteger(numberOr(semantic.candidate_limit, 120), 2, 256, "semantic.candidate_limit"),
      resolution_threshold: boundedNumber(numberOr(semantic.resolution_threshold, 0.45), 0, 1, "semantic.resolution_threshold"),
      ambiguity_margin: boundedNumber(numberOr(semantic.ambiguity_margin, 0.04), 0, 1, "semantic.ambiguity_margin"),
      resolver_min_confidence: boundedNumber(numberOr(semantic.resolver_min_confidence, 0.55), 0, 1, "semantic.resolver_min_confidence"),
      resolver_candidate_limit: boundedInteger(numberOr(semantic.resolver_candidate_limit, 8), 1, 64, "semantic.resolver_candidate_limit"),
    },
    conversation: {
      default_topic_ttl: boundedInteger(numberOr(conversation.default_topic_ttl, 3), 1, 0xffff_ffff, "conversation.default_topic_ttl"),
      default_followup_ttl: boundedInteger(numberOr(conversation.default_followup_ttl, 2), 1, 0xffff_ffff, "conversation.default_followup_ttl"),
      recent_response_limit: boundedInteger(numberOr(conversation.recent_response_limit, 8), 1, 64, "conversation.recent_response_limit"),
      recent_variant_limit: boundedInteger(numberOr(conversation.recent_variant_limit, 4), 1, 64, "conversation.recent_variant_limit"),
      recent_user_window: boundedInteger(numberOr(conversation.recent_user_window, 4), 1, 50, "conversation.recent_user_window"),
      repeat_detection_window: boundedInteger(numberOr(conversation.repeat_detection_window, 3), 1, 50, "conversation.repeat_detection_window"),
      repeat_detection_threshold: boundedInteger(numberOr(conversation.repeat_detection_threshold, 2), 2, 20, "conversation.repeat_detection_threshold"),
      max_messages_per_turn: boundedInteger(numberOr(conversation.max_messages_per_turn, 4), 1, 6, "conversation.max_messages_per_turn"),
      repair_candidate_min_score: boundedNumber(numberOr(conversation.repair_candidate_min_score, 0.40), 0, 1, "conversation.repair_candidate_min_score"),
      author_numbers: parseAuthorNumbers(conversation.author_numbers ?? [], "conversation.author_numbers"),
      topic_preference_margin: boundedNumber(numberOr(conversation.topic_preference_margin, 0.04), 0, 0.25, "conversation.topic_preference_margin"),
    },
    matcher_profiles: matcherProfiles,
    packages,
    selectedPackageId: packages[0]?.manifest.id ?? "",
    updatedSerial: 1,
  };
  const assetFiles: StudioAssetFile[] = [];
  const assetKeys = new Set<string>();
  for (const pkg of packages) {
    for (const contribution of pkg.contents.assets) {
      const asset = contribution.value;
      const path = normalizeSafePath(prefix + packageAssetPath(pkg, asset.source));
      const blob = mapped.get(path);
      if (!blob) throw new Error(`Missing asset source: ${packageAssetPath(pkg, asset.source)}`);
      const key = `${pkg.manifest.id}\0${asset.source}`;
      if (assetKeys.has(key)) continue;
      assetKeys.add(key);
      assetFiles.push({ owner_key: `source:${pkg.manifest.id}`, package_id: pkg.manifest.id, source: asset.source, media_type: asset.media_type, blob });
    }
  }
  return { workspace: cloneBrainWorkspace(workspace), assetFiles: validateAssetFiles(assetFiles) };
}

async function boundedSourceJson(file: Blob, label: string): Promise<unknown> {
  if (file.size > SOURCE_JSON_MAX_BYTES) throw new Error(`${label} exceeds the 8 MiB source JSON limit`);
  const text = await file.text();
  if (new TextEncoder().encode(text).byteLength > SOURCE_JSON_MAX_BYTES) throw new Error(`${label} exceeds the 8 MiB source JSON limit`);
  const value = JSON.parse(text) as unknown;
  validateWorkspaceShapeBudget(value);
  return value;
}

function validateWorkspaceShapeBudget(value: unknown): void {
  let nodes = 0;
  const walk = (node: unknown, depth: number): void => {
    nodes += 1;
    if (nodes > BRAIN_VIEW_MAX_NODES) throw new Error("Workspace exceeds the JSON node budget");
    if (depth > BRAIN_VIEW_MAX_DEPTH) throw new Error("Workspace exceeds the JSON depth budget");
    if (typeof node === "string" && new TextEncoder().encode(node).byteLength > BRAIN_VIEW_MAX_STRING_BYTES) throw new Error("Workspace contains an oversized string");
    if (Array.isArray(node)) {
      if (node.length > BRAIN_VIEW_MAX_NODES) throw new Error("Workspace contains an oversized array");
      node.forEach((child) => walk(child, depth + 1));
    } else if (node !== null && typeof node === "object") {
      const entries = Object.entries(node as Record<string, unknown>);
      if (entries.length > BRAIN_VIEW_MAX_NODES) throw new Error("Workspace contains an oversized object");
      for (const [key, child] of entries) {
        if (new TextEncoder().encode(key).byteLength > BRAIN_VIEW_MAX_STRING_BYTES) throw new Error("Workspace contains an oversized object key");
        walk(child, depth + 1);
      }
    }
  };
  walk(value, 0);
}

function boundedInteger(value: unknown, min: number, max: number, path: string): number {
  if (typeof value !== "number" || !Number.isSafeInteger(value) || value < min || value > max) throw new Error(`${path} is outside ${min}..=${max}`);
  return value;
}

function boundedNumber(value: unknown, min: number, max: number, path: string): number {
  if (typeof value !== "number" || !Number.isFinite(value) || value < min || value > max) throw new Error(`${path} is outside ${min}..=${max}`);
  return value;
}

function safeWorkspacePath(path: string): boolean {
  return path.length > 0 && !path.startsWith("/") && !path.endsWith("/") && !path.includes("\\")
    && path.split("/").every((part) => part.length > 0 && part !== "." && part !== "..");
}

const PACKAGE_FRAGMENT_NAMESPACES = [
  "meanings", "behaviors", "capability_result_behaviors", "openings", "fallback_behaviors",
  "style_lexicons", "capabilities", "capability_bindings", "capability_policies",
  "capability_configs", "types", "assets", "regression_cases", "scenarios",
] as const;

async function loadPackageSource(mapped: Map<string, Blob>, prefix: string, path: string, authoringLanguage: string): Promise<StudioPackage> {
  const rootFile = mapped.get(normalizePath(prefix + path));
  if (!rootFile) throw new Error(`Missing package source: ${path}`);
  const root = asObject(await boundedSourceJson(rootFile, path), path);
  assertKeys(root, ["format", "version", "manifest", "fragments"], path);
  if (root.format !== "gvya.source.package" || root.version !== 1) throw new Error(`Unsupported Package source format: ${path}; version 1 is required.`);
  const fragments = asObject(root.fragments, `${path}#fragments`);
  assertKeys(fragments, [...PACKAGE_FRAGMENT_NAMESPACES], `${path}#fragments`);
  const contents: Record<string, unknown[]> = Object.fromEntries(PACKAGE_FRAGMENT_NAMESPACES.map((namespace) => [namespace, []]));
  const seen = new Set<string>();
  const slash = path.lastIndexOf("/");
  const dir = slash < 0 ? "" : path.slice(0, slash + 1);
  for (const namespace of PACKAGE_FRAGMENT_NAMESPACES) {
    const rawPaths = fragments[namespace] == null ? [] : asArray(fragments[namespace], `${path}#fragments.${namespace}`);
    for (let index = 0; index < rawPaths.length; index += 1) {
      const relative = requiredString(rawPaths[index], `${path}#fragments.${namespace}[${index}]`);
      if (!relative.startsWith("fragments/") || !relative.endsWith(".json") || !safeWorkspacePath(relative)) throw new Error(`${path}#fragments.${namespace}[${index}] must be a safe package-local fragments/*.json path.`);
      const fragmentPath = normalizePath(`${dir}${relative}`);
      if (!safeWorkspacePath(fragmentPath)) throw new Error(`Unsafe Package fragment path: ${fragmentPath}`);
      if (seen.has(fragmentPath)) throw new Error(`Package ${path} declares fragment ${relative} more than once.`);
      seen.add(fragmentPath);
      const blob = mapped.get(normalizePath(prefix + fragmentPath));
      if (!blob) throw new Error(`Missing Package fragment: ${fragmentPath}`);
      contents[namespace]!.push(await boundedSourceJson(blob, fragmentPath));
    }
  }
  return parsePackageSnapshot(path, { manifest: root.manifest, contents }, authoringLanguage);
}

export function parsePackageSnapshot(path: string, raw: unknown, authoringLanguage = "und"): StudioPackage {
  const doc = asObject(raw, path);
  assertKeys(doc, ["manifest", "contents"], path);
  const manifest = asObject(doc.manifest, `${path}#manifest`);
  assertKeys(manifest, ["id", "kind", "description", "dependencies"], `${path}#manifest`);
  const dependencies: PackageDependency[] = asArray(manifest.dependencies, `${path}#dependencies`).map((value, index) => {
    const row = asObject(value, `${path}#dependencies[${index}]`);
    assertKeys(row, ["id", "reexport"], `${path}#dependencies[${index}]`);
    return { id: requiredString(row.id, "dependency.id"), reexport: row.reexport === true };
  });
  const contentsObject = asObject(doc.contents, `${path}#contents`);
  const supported = Object.keys(emptyContents());
  assertKeys(contentsObject, supported, `${path}#contents`);
  const contents: PackageContents = emptyContents();
  contents.meanings = parseContributions(contentsObject.meanings, `${path}#meanings`, parseMeaning);
  contents.behaviors = parseContributions(contentsObject.behaviors, `${path}#behaviors`, parseBehavior);
  contents.capability_result_behaviors = parseContributions(contentsObject.capability_result_behaviors, `${path}#capability_result_behaviors`, parseCapabilityResultBehavior);
  contents.openings = parseContributions(contentsObject.openings, `${path}#openings`, parseOpening);
  contents.fallback_behaviors = parseContributions(contentsObject.fallback_behaviors, `${path}#fallback_behaviors`, parseFallbackBehavior);
  contents.style_lexicons = parseRawObjectContributions(contentsObject.style_lexicons, `${path}#style_lexicons`);
  contents.capabilities = parseContributions(contentsObject.capabilities, `${path}#capabilities`, parseCapability);
  contents.capability_bindings = parseContributions(contentsObject.capability_bindings, `${path}#capability_bindings`, parseBinding);
  contents.capability_policies = parseContributions(contentsObject.capability_policies, `${path}#capability_policies`, parsePolicy);
  contents.capability_configs = parseRawObjectContributions(contentsObject.capability_configs, `${path}#capability_configs`);
  contents.types = parseRawObjectContributions(contentsObject.types, `${path}#types`);
  contents.assets = parseContributions(contentsObject.assets, `${path}#assets`, parseAsset);
  contents.regression_cases = parseContributions(contentsObject.regression_cases, `${path}#regression_cases`, parseRegression);
  contents.scenarios = parseContributions(contentsObject.scenarios, `${path}#scenarios`, parseScenario);
  const packageKind = manifest.kind === "fallback" ? "fallback" : manifest.kind === "standard" ? "standard" : (() => { throw new Error(`${path}#manifest.kind must be standard or fallback`); })();
  validatePackageKindContract(packageKind, dependencies, contents, path);
  return {
    path,
    authoring_language: authoringLanguage,
    manifest: {
      id: requiredString(manifest.id, "manifest.id"),
      kind: packageKind,
      description: typeof manifest.description === "string" ? manifest.description : "",
      dependencies,
    },
    contents,
  };
}

function validatePackageKindContract(kind: "standard" | "fallback", dependencies: PackageDependency[], contents: PackageContents, path: string): void {
  if (kind === "standard") {
    if (contents.fallback_behaviors.length !== 0) throw new Error(`${path}: Standard Packages cannot contain fallback_behaviors`);
    return;
  }
  if (dependencies.length !== 0) throw new Error(`${path}: Fallback Packages cannot declare dependencies`);
  const forbidden: Array<keyof PackageContents> = [
    "meanings", "behaviors", "capability_result_behaviors", "openings", "style_lexicons",
    "capabilities", "capability_bindings", "capability_policies", "capability_configs", "types",
  ];
  for (const namespace of forbidden) if (contents[namespace].length !== 0) throw new Error(`${path}: Fallback Packages cannot contain ${namespace}`);
  for (const namespace of ["fallback_behaviors", "assets", "regression_cases", "scenarios"] as const) {
    for (const row of contents[namespace] as Array<{ exported: boolean; mode: unknown }>) {
      if (row.exported || row.mode !== "add") throw new Error(`${path}: Fallback Package contributions are private add-only and cannot be overridden`);
    }
  }
}

function parseContributions<T>(value: unknown, path: string, parser: (value: unknown, path: string) => T): Contribution<T>[] {
  return asArray(value ?? [], path).map((raw, index) => {
    const rowPath = `${path}[${index}]`;
    const row = asObject(raw, rowPath);
    assertKeys(row, ["id", "exported", "mode", "value"], rowPath);
    return {
      id: requiredString(row.id, `${rowPath}.id`),
      exported: row.exported !== false,
      mode: parseMode(row.mode, `${rowPath}.mode`),
      value: parser(row.value, `${rowPath}.value`),
    };
  });
}


function parseRawObjectContributions(value: unknown, path: string): Contribution<JsonObject>[] {
  return parseContributions(value, path, (raw, rowPath) => asObject(raw, rowPath) as JsonObject);
}

function parseMode(value: unknown, path: string): Contribution<JsonValue>["mode"] {
  if (value === undefined || value === "add") return "add";
  const row = asObject(value, path);
  assertKeys(row, ["type", "target_package", "target_id"], path);
  if (row.type !== "replace") throw new Error(`${path}: mode must be add or replace`);
  return { type: "replace", target_package: requiredString(row.target_package, `${path}.target_package`), target_id: requiredString(row.target_id, `${path}.target_id`) };
}

function parseMeaning(value: unknown, path: string): MeaningDefinition {
  const row = asObject(value, path);
  assertKeys(row, ["id", "class", "patterns", "samples", "negative_samples", "retrieval_terms", "priority", "positive_assumption", "slots", "references"], path);
  return {
    id: requiredString(row.id, `${path}.id`),
    class: row.class === "social" || row.class === "clarification" ? row.class : "general",
    patterns: parseStructuralPatterns(row.patterns ?? [], `${path}.patterns`),
    samples: parseLocalizedSamples(row.samples ?? [], `${path}.samples`),
    negative_samples: parseLocalizedSamples(row.negative_samples ?? [], `${path}.negative_samples`),
    retrieval_terms: parseLocalizedSamples(row.retrieval_terms ?? [], `${path}.retrieval_terms`),
    priority: numberOr(row.priority, 1),
    positive_assumption: row.positive_assumption === true,
    slots: asArray(row.slots ?? [], `${path}.slots`).map((raw, index) => {
      const slot = asObject(raw, `${path}.slots[${index}]`);
      assertKeys(slot, ["name", "type", "entity_kind", "reference_kind", "required", "elicitation"], `${path}.slots[${index}]`);
      const type = ["string", "number", "boolean", "entity", "reference"].includes(String(slot.type)) ? String(slot.type) as MeaningDefinition["slots"][number]["type"] : "string";
      return { name: requiredString(slot.name, "slot.name"), type, entity_kind: stringOr(slot.entity_kind), reference_kind: stringOr(slot.reference_kind), required: slot.required === true, elicitation: parseLocalizedSamples(slot.elicitation ?? [], `${path}.slots[${index}].elicitation`) };
    }),
    references: asArray(row.references ?? [], `${path}.references`).map((raw, index) => {
      const ref = asObject(raw, `${path}.references[${index}]`);
      assertKeys(ref, ["kind", "required", "elicitation"], `${path}.references[${index}]`);
      return { kind: requiredString(ref.kind, "reference.kind"), required: ref.required === true, elicitation: parseLocalizedSamples(ref.elicitation ?? [], `${path}.references[${index}].elicitation`) };
    }),
  };
}

function parseStructuralPatterns(value: unknown, path: string): MeaningDefinition["patterns"] {
  return asArray(value, path).map((raw, index) => {
    const patternPath = `${path}[${index}]`;
    const row = asObject(raw, patternPath);
    assertKeys(row, ["language", "text", "priority"], patternPath);
    const language = requiredString(row.language, `${patternPath}.language`);
    if (!isWellFormedLanguageTag(language)) throw new Error(`${patternPath}.language is not a valid BCP 47 tag`);
    return { language, text: requiredString(row.text, `${patternPath}.text`), priority: numberOr(row.priority, 0) };
  });
}

function parseLocalizedSamples(value: unknown, path: string): MeaningDefinition["samples"] {
  return asArray(value, path).map((raw, index) => {
    const samplePath = `${path}[${index}]`;
    const sample = asObject(raw, samplePath);
    assertKeys(sample, ["language", "text"], samplePath);
    const language = requiredString(sample.language, `${samplePath}.language`);
    if (!isWellFormedLanguageTag(language)) throw new Error(`${samplePath}.language is not a valid BCP 47 tag`);
    return { language, text: requiredString(sample.text, `${samplePath}.text`) };
  });
}

function parseRequirement(value: unknown, path: string): ValueRequirement {
  const row = asObject(value, path);
  assertKeys(row, ["namespace", "path", "value"], path);
  const namespace = String(row.namespace ?? "") as ValueRequirement["namespace"];
  if (!["author", "conversation", "context", "meaning", "system"].includes(namespace)) throw new Error(`Invalid requirement namespace: ${path}`);
  if (!("value" in row) || row.value === null || row.value === undefined) throw new Error(`Value requirement must contain a non-null value: ${path}`);
  return { namespace, path: requiredString(row.path, `${path}.path`), value: row.value as JsonValue };
}

function parseAuthorNumbers(value: unknown, path: string): ConversationConfig["author_numbers"] {
  const rows = asArray(value ?? [], path);
  if (rows.length > 256) throw new Error(`${path} exceeds 256 definitions.`);
  const out = rows.map((item, index) => {
    const row = asObject(item, `${path}[${index}]`);
    assertKeys(row, ["path", "default", "min", "max"], `${path}[${index}]`);
    const statePath = requiredString(row.path, `${path}[${index}].path`);
    if (statePath.length > 512 || statePath.split(".").length > 16 || statePath.split(".").some((part) => !part)) throw new Error(`${path}[${index}].path is invalid.`);
    const min = boundedNumber(row.min, -Number.MAX_VALUE, Number.MAX_VALUE, `${path}[${index}].min`);
    const max = boundedNumber(row.max, -Number.MAX_VALUE, Number.MAX_VALUE, `${path}[${index}].max`);
    const def = boundedNumber(row.default, -Number.MAX_VALUE, Number.MAX_VALUE, `${path}[${index}].default`);
    if (min > def || def > max) throw new Error(`${path}[${index}] must satisfy min <= default <= max.`);
    return { path: statePath, default: def, min, max };
  });
  const paths = out.map((row) => row.path);
  if (new Set(paths).size !== paths.length) throw new Error(`${path} paths must be unique.`);
  for (let i=0;i<paths.length;i++) for (let j=i+1;j<paths.length;j++) if (paths[i]!.startsWith(`${paths[j]}.`) || paths[j]!.startsWith(`${paths[i]}.`)) throw new Error(`${path} paths cannot overlap as parent and child.`);
  return out;
}

function parseBehavior(value: unknown, path: string): BehaviorDefinition {
  const row = asObject(value, path);
  assertKeys(row, ["id", "meaning", "topic", "topic_scoped", "activates_topic", "topic_ttl", "followup_scope", "repair_continuation_candidate", "repeat_same_input_after", "repeat_same_meaning_after", "requires_values", "forbidden_values", "responses"], path);
  return {
    id: requiredString(row.id, `${path}.id`),
    meaning: requiredString(row.meaning, `${path}.meaning`),
    topic: stringOr(row.topic),
    topic_scoped: row.topic_scoped === true,
    activates_topic: row.activates_topic === true,
    topic_ttl: nullableNumber(row.topic_ttl),
    followup_scope: stringOr(row.followup_scope),
    repair_continuation_candidate: row.repair_continuation_candidate === true,
    repeat_same_input_after: row.repeat_same_input_after == null ? null : boundedInteger(row.repeat_same_input_after, 2, 20, `${path}.repeat_same_input_after`),
    repeat_same_meaning_after: row.repeat_same_meaning_after == null ? null : boundedInteger(row.repeat_same_meaning_after, 2, 20, `${path}.repeat_same_meaning_after`),
    requires_values: asArray(row.requires_values ?? [], `${path}.requires_values`).map((item, index) => parseRequirement(item, `${path}.requires_values[${index}]`)),
    forbidden_values: asArray(row.forbidden_values ?? [], `${path}.forbidden_values`).map((item, index) => parseRequirement(item, `${path}.forbidden_values[${index}]`)),
    responses: asArray(row.responses ?? [], `${path}.responses`).map((response, index) => parseResponse(response, `${path}.responses[${index}]`)),
  };
}

function parseFallbackBehavior(value: unknown, path: string): FallbackBehaviorDefinition {
  const row = asObject(value, path);
  assertKeys(row, ["id", "trigger", "priority", "conditions", "responses"], path);
  if (!(row.trigger === "unresolved" || row.trigger === "repeat")) throw new Error(`${path}.trigger must be unresolved or repeat`);
  const conditions = asArray(row.conditions ?? [], `${path}.conditions`).map((item, index) => parseCondition(item, `${path}.conditions[${index}]`));
  if (conditions.some((condition) => condition.namespace === "meaning" || condition.namespace === "interaction")) {
    throw new Error(`${path}.conditions cannot depend on meaning or interaction state during fallback selection`);
  }
  const priority = numberOr(row.priority, 0);
  if (!Number.isSafeInteger(priority) || priority < -2147483648 || priority > 2147483647) throw new Error(`${path}.priority must be a signed 32-bit integer`);
  return {
    id: requiredString(row.id, `${path}.id`),
    trigger: row.trigger,
    priority,
    conditions,
    responses: asArray(row.responses ?? [], `${path}.responses`).map((response, index) => parseResponse(response, `${path}.responses[${index}]`)),
  };
}

function parseCapabilityResultBehavior(value: unknown, path: string): CapabilityResultBehavior {
  const row = asObject(value, path);
  assertKeys(row, ["id", "capability", "capability_version", "succeeded", "error_code", "responses"], path);
  if (row.succeeded !== undefined && row.succeeded !== null && typeof row.succeeded !== "boolean") throw new Error(`Invalid succeeded flag: ${path}`);
  return {
    id: requiredString(row.id, `${path}.id`),
    capability: requiredString(row.capability, `${path}.capability`),
    capability_version: requiredString(row.capability_version, `${path}.capability_version`),
    succeeded: row.succeeded == null ? null : row.succeeded as boolean,
    error_code: stringOr(row.error_code),
    responses: asArray(row.responses ?? [], `${path}.responses`).map((response, index) => parseResponse(response, `${path}.responses[${index}]`)),
  };
}

function parseOpening(value: unknown, path: string): OpeningDefinition {
  const row = asObject(value, path);
  assertKeys(row, ["id", "topic", "topic_ttl", "responses"], path);
  return {
    id: requiredString(row.id, `${path}.id`),
    topic: stringOr(row.topic),
    topic_ttl: nullableNumber(row.topic_ttl),
    responses: asArray(row.responses ?? [], `${path}.responses`).map((response, index) => parseResponse(response, `${path}.responses[${index}]`)),
  };
}

function parseResponse(value: unknown, path: string): ResponseDefinition {
  const row = asObject(value, path);
  assertKeys(row, ["id", "kind", "texts", "conditions", "hint_level", "repeat_stage", "effects", "opens_followup", "extra_messages", "assets", "links"], path);
  const kindValues = ["normal", "hint", "repeat", "annoyed_repeat", "final_repeat", "fallback", "opening"];
  const kind = kindValues.includes(String(row.kind)) ? String(row.kind) as ResponseDefinition["kind"] : "normal";
  const repeatStage = ["repeat", "annoyed", "final"].includes(String(row.repeat_stage)) ? String(row.repeat_stage) as ResponseDefinition["repeat_stage"] : "";
  return {
    id: requiredString(row.id, `${path}.id`),
    kind,
    texts: parseLocalizedTexts(row.texts ?? [], `${path}.texts`),
    conditions: asArray(row.conditions ?? [], `${path}.conditions`).map((item, index) => parseCondition(item, `${path}.conditions[${index}]`)),
    hint_level: nullableNumber(row.hint_level),
    repeat_stage: repeatStage,
    effects: asArray(row.effects ?? [], `${path}.effects`).map((raw, index) => {
      const effect = asObject(raw, `${path}.effects[${index}]`);
      const target = asObject(effect.target, `${path}.effects[${index}].target`);
      const type = effect.type === "increment" ? "increment" : "assign";
      return { type, target: { namespace: "author" as const, path: requiredString(target.path, "effect.target.path") }, value: asJsonValue(effect.value ?? null), delta: numberOr(effect.delta, 0) };
    }),
    opens_followup: row.opens_followup == null ? null : (() => {
      const followup = asObject(row.opens_followup, `${path}.opens_followup`);
      assertKeys(followup, ["id", "ttl", "refresh_if_same"], `${path}.opens_followup`);
      return { id: requiredString(followup.id, "opens_followup.id"), ttl: numberOr(followup.ttl, 1), refresh_if_same: followup.refresh_if_same === true };
    })(),
    extra_messages: asArray(row.extra_messages ?? [], `${path}.extra_messages`).map((raw, index) => {
      const msg = asObject(raw, `${path}.extra_messages[${index}]`);
      return { chance: numberOr(msg.chance, 1), texts: parseLocalizedTexts(msg.texts ?? [], `${path}.extra_messages[${index}].texts`) };
    }),
    assets: asArray(row.assets ?? [], `${path}.assets`).map((raw, index) => {
      const asset = asObject(raw, `${path}.assets[${index}]`);
      assertKeys(asset, ["asset_id", "alt_text"], `${path}.assets[${index}]`);
      return { asset_id: requiredString(asset.asset_id, "asset.asset_id"), alt_text: stringOr(asset.alt_text) };
    }),
    links: asArray(row.links ?? [], `${path}.links`).map((raw) => {
      const link = asObject(raw, "link");
      return { label: stringOr(link.label), url: requiredString(link.url, "link.url") };
    }),
  };
}

function parseLocalizedTexts(value: unknown, path: string): LocalizedTexts[] {
  return asArray(value, path).map((raw, index) => {
    const row = asObject(raw, `${path}[${index}]`);
    assertKeys(row, ["language", "variants"], `${path}[${index}]`);
    return { language: requiredString(row.language, "texts.language"), variants: stringArray(row.variants ?? [], "texts.variants") };
  });
}

function parseCondition(value: unknown, path: string): ValueCondition {
  const row = asObject(value, path);
  assertKeys(row, ["namespace", "path", "op", "value"], path);
  return { namespace: String(row.namespace) as ValueCondition["namespace"], path: requiredString(row.path, `${path}.path`), op: String(row.op) as ValueCondition["op"], value: asJsonValue(row.value ?? null), hasValue: Object.prototype.hasOwnProperty.call(row, "value") };
}

function parseCapability(value: unknown, path: string): CapabilityDefinition {
  const row = asObject(value, path);
  assertKeys(row, ["contract", "host_effects"], path);
  const contract = asObject(row.contract, `${path}.contract`);
  assertKeys(contract, ["id", "version", "title", "description", "input_schema", "output_schema", "reference_kinds", "effect_class", "confirmation_hint"], `${path}.contract`);
  return {
    contract: {
      id: requiredString(contract.id, "capability.id"),
      version: requiredString(contract.version, "capability.version"),
      title: requiredString(contract.title, "capability.title"),
      description: stringOr(contract.description),
      input_schema: asObject(contract.input_schema, "capability.input_schema") as JsonObject,
      output_schema: contract.output_schema == null ? null : asObject(contract.output_schema, "capability.output_schema") as JsonObject,
      reference_kinds: stringArray(contract.reference_kinds ?? [], "capability.reference_kinds"),
      effect_class: String(contract.effect_class) as CapabilityDefinition["contract"]["effect_class"],
      confirmation_hint: String(contract.confirmation_hint) as CapabilityDefinition["contract"]["confirmation_hint"],
    },
    host_effects: asArray(row.host_effects ?? [], `${path}.host_effects`).map((raw) => {
      const effect = asObject(raw, "host_effect");
      return { resource: requiredString(effect.resource, "host_effect.resource"), kind: String(effect.kind) as CapabilityDefinition["host_effects"][number]["kind"], summary: stringOr(effect.summary) };
    }),
  };
}

function parseBinding(value: unknown, path: string): CapabilityBinding {
  const row = asObject(value, path);
  assertKeys(row, ["id", "trigger", "capability", "arguments"], path);
  const trigger = asObject(row.trigger, `${path}.trigger`);
  return {
    id: requiredString(row.id, `${path}.id`),
    trigger: { meaning: stringOr(trigger.meaning), behavior: stringOr(trigger.behavior), response: stringOr(trigger.response) },
    capability: requiredString(row.capability, `${path}.capability`),
    arguments: asArray(row.arguments ?? [], `${path}.arguments`).map((raw, index) => {
      const arg = asObject(raw, `${path}.arguments[${index}]`);
      const source = asObject(arg.source, `${path}.arguments[${index}].source`);
      return {
        target: requiredString(arg.target, "binding.target"),
        source: {
          type: String(source.type) as CapabilityBinding["arguments"][number]["source"]["type"],
          name: stringOr(source.name),
          kind: stringOr(source.kind),
          projection: source.projection === "object" ? "object" : "id",
          path: stringOr(source.path),
          value: asJsonValue(source.value ?? null),
        },
      };
    }),
  };
}

function parsePolicy(value: unknown, path: string): CapabilityPolicy {
  const row = asObject(value, path);
  const effect = asObject(row.effect, `${path}.effect`);
  const effectType = effect.type === "deny" ? "deny" : effect.type === "require_confirmation" ? "require_confirmation" : "allow";
  return {
    id: requiredString(row.id, `${path}.id`), capability: requiredString(row.capability, `${path}.capability`), priority: numberOr(row.priority, 0),
    conditions: asArray(row.conditions ?? [], `${path}.conditions`).map((raw, index) => parseAdmission(raw, `${path}.conditions[${index}]`)),
    effect: effectType === "allow" ? { type: "allow", reason_code: "" } : { type: effectType, reason_code: requiredString(effect.reason_code, `${path}.effect.reason_code`) },
  };
}

function parseAdmission(value: unknown, path: string): AdmissionPredicate {
  const row = asObject(value, path);
  return { namespace: String(row.namespace) as AdmissionPredicate["namespace"], path: requiredString(row.path, `${path}.path`), op: String(row.op) as AdmissionPredicate["op"], value: asJsonValue(row.value ?? null), hasValue: Object.prototype.hasOwnProperty.call(row, "value") };
}

function parseAsset(value: unknown, path: string): AssetDefinition {
  const row = asObject(value, path);
  assertKeys(row, ["id", "media_type", "logical_path", "source"], path);
  return { id: requiredString(row.id, `${path}.id`), media_type: requiredString(row.media_type, `${path}.media_type`), logical_path: requiredString(row.logical_path, `${path}.logical_path`), source: requiredString(row.source, `${path}.source`) };
}

function parseRegression(value: unknown, path: string): RegressionCase {
  const row = asObject(value, path);
  return {
    id: requiredString(row.id, `${path}.id`), description: stringOr(row.description), input: requiredString(row.input, `${path}.input`), language: stringOr(row.language),
    context: parseContext(row.context), initial_state: asObject(row.initial_state ?? {}, `${path}.initial_state`) as JsonObject,
    seed: nullableNumber(row.seed), unix_time_ms: nullableNumber(row.unix_time_ms), expectation: parseExpectation(row.expectation), generated: row.generated === true,
  };
}

function parseScenario(value: unknown, path: string): ConversationScenario {
  const row = asObject(value, path);
  assertKeys(row, ["id", "description", "context", "initial_state", "steps", "generated"], path);
  return {
    id: requiredString(row.id, `${path}.id`),
    description: stringOr(row.description),
    context: parseContext(row.context),
    initial_state: asObject(row.initial_state ?? {}, `${path}.initial_state`) as JsonObject,
    steps: asArray(row.steps ?? [], `${path}.steps`).map((raw, index) => parseScenarioStep(raw, `${path}.steps[${index}]`)),
    generated: row.generated === true,
  };
}

function parseScenarioStep(value: unknown, path: string): ConversationScenario["steps"][number] {
  const row = asObject(value, path);
  const type = requiredString(row.type, `${path}.type`);
  const context = row.context == null ? null : parseContext(row.context);
  const expectation = parseExpectation(row.expectation);
  switch (type) {
    case "open":
      assertKeys(row, ["type", "language", "context", "seed", "unix_time_ms", "expectation"], path);
      return {
        type,
        language: stringOr(row.language),
        context,
        seed: nullableNumber(row.seed),
        unix_time_ms: nullableNumber(row.unix_time_ms),
        expectation,
      };
    case "turn":
      assertKeys(row, ["type", "say", "language", "context", "reference_candidates", "resolver_context", "hint", "seed", "unix_time_ms", "expectation"], path);
      return {
        type,
        say: requiredString(row.say, `${path}.say`),
        language: stringOr(row.language),
        context,
        reference_candidates: asArray(row.reference_candidates ?? [], `${path}.reference_candidates`).map((rawCandidate, candidateIndex) => {
          const candidate = asObject(rawCandidate, `${path}.reference_candidates[${candidateIndex}]`);
          assertKeys(candidate, ["reference", "label", "aliases"], `${path}.reference_candidates[${candidateIndex}]`);
          const reference = asObject(candidate.reference, `${path}.reference_candidates[${candidateIndex}].reference`);
          assertKeys(reference, ["kind", "id"], `${path}.reference_candidates[${candidateIndex}].reference`);
          return {
            reference: { kind: requiredString(reference.kind, "reference.kind"), id: requiredString(reference.id, "reference.id") },
            label: stringOr(candidate.label),
            aliases: stringArray(candidate.aliases ?? [], "reference candidate aliases"),
          };
        }),
        resolver_context: asObject(row.resolver_context ?? {}, `${path}.resolver_context`) as Record<string, JsonValue>,
        hint: parseScenarioHint(row.hint, `${path}.hint`),
        seed: nullableNumber(row.seed),
        unix_time_ms: nullableNumber(row.unix_time_ms),
        expectation,
      };
    case "capability_result":
      assertKeys(row, ["type", "proposal_from_step", "proposal_capability", "proposal_ordinal", "succeeded", "output", "error_code", "language", "context", "seed", "unix_time_ms", "expectation"], path);
      if (typeof row.succeeded !== "boolean") throw new Error(`${path}.succeeded: expected boolean`);
      return {
        type,
        proposal_from_step: positiveInteger(row.proposal_from_step, `${path}.proposal_from_step`),
        proposal_capability: stringOr(row.proposal_capability),
        proposal_ordinal: nullablePositiveInteger(row.proposal_ordinal, `${path}.proposal_ordinal`),
        succeeded: row.succeeded,
        output: Object.prototype.hasOwnProperty.call(row, "output") ? asJsonValue(row.output) : undefined,
        error_code: stringOr(row.error_code),
        language: stringOr(row.language),
        context,
        seed: nullableNumber(row.seed),
        unix_time_ms: nullableNumber(row.unix_time_ms),
        expectation,
      };
    case "confirm":
      assertKeys(row, ["type", "proposal_from_step", "proposal_capability", "proposal_ordinal", "confirmed", "context", "unix_time_ms", "expectation"], path);
      if (typeof row.confirmed !== "boolean") throw new Error(`${path}.confirmed: expected boolean`);
      return {
        type,
        proposal_from_step: positiveInteger(row.proposal_from_step, `${path}.proposal_from_step`),
        proposal_capability: stringOr(row.proposal_capability),
        proposal_ordinal: nullablePositiveInteger(row.proposal_ordinal, `${path}.proposal_ordinal`),
        confirmed: row.confirmed,
        context,
        unix_time_ms: nullableNumber(row.unix_time_ms),
        expectation,
      };
    default:
      throw new Error(`${path}.type: unsupported scenario step type ${type}`);
  }
}

function parseScenarioHint(value: unknown, path: string): ScenarioHint {
  if (value == null) return { type: "none" };
  const row = asObject(value, path);
  assertKeys(row, ["type", "level"], path);
  const type = requiredString(row.type, `${path}.type`);
  if (type === "direct") return { type, level: positiveInteger(row.level, `${path}.level`) };
  if (type === "none" || type === "first" || type === "next" || type === "auto") return { type };
  throw new Error(`${path}.type: unsupported hint type ${type}`);
}

function parseExpectation(value: unknown): TurnExpectation {
  if (value == null) return createExpectation();
  const row = asObject(value, "expectation");
  const out = createExpectation();
  out.meaning = stringOr(row.meaning);
  out.forbidden_meanings = stringArray(row.forbidden_meanings ?? [], "expectation.forbidden_meanings");
  out.meaning_slots = asObject(row.meaning_slots ?? {}, "expectation.meaning_slots") as Record<string, JsonValue>;
  out.meaning_references = asArray(row.meaning_references ?? [], "expectation.meaning_references").map((raw) => ({ kind: requiredString(asObject(raw, "reference").kind, "reference.kind"), id: requiredString(asObject(raw, "reference").id, "reference.id") }));
  out.min_semantic_score = nullableNumber(row.min_semantic_score);
  out.conversation_mode = stringOr(row.conversation_mode);
  out.response_ids = stringArray(row.response_ids ?? [], "expectation.response_ids");
  out.forbidden_response_ids = stringArray(row.forbidden_response_ids ?? [], "expectation.forbidden_response_ids");
  out.response_contains = stringArray(row.response_contains ?? [], "expectation.response_contains");
  out.response_not_contains = stringArray(row.response_not_contains ?? [], "expectation.response_not_contains");
  out.author_values = asObject(row.author_values ?? {}, "expectation.author_values") as Record<string, JsonValue>;
  out.conversation_values = asObject(row.conversation_values ?? {}, "expectation.conversation_values") as Record<string, JsonValue>;
  out.active_topic = stringOr(row.active_topic); out.active_followup = stringOr(row.active_followup);
  out.capabilities = asArray(row.capabilities ?? [], "expectation.capabilities").map((raw) => { const cap = asObject(raw, "expected capability"); return { id: requiredString(cap.id, "capability.id"), version: stringOr(cap.version), arguments: cap.arguments == null ? null : asObject(cap.arguments, "capability.arguments") as Record<string, JsonValue> }; });
  out.proposal_receipts = asArray(row.proposal_receipts ?? [], "expectation.proposal_receipts").map((raw, index) => {
    const receipt = asObject(raw, `expectation.proposal_receipts[${index}]`);
    const outcome = requiredString(receipt.outcome, `expectation.proposal_receipts[${index}].outcome`);
    if (outcome !== "admitted" && outcome !== "needs_confirmation" && outcome !== "rejected") throw new Error(`expectation.proposal_receipts[${index}].outcome: unsupported outcome ${outcome}`);
    const reason_code = stringOr(receipt.reason_code);
    if (outcome === "admitted" && reason_code) throw new Error(`expectation.proposal_receipts[${index}].reason_code: admitted receipt cannot have a reason code`);
    return { id: requiredString(receipt.id, `expectation.proposal_receipts[${index}].id`), version: stringOr(receipt.version), arguments: receipt.arguments == null ? null : asObject(receipt.arguments, `expectation.proposal_receipts[${index}].arguments`) as Record<string, JsonValue>, outcome, reason_code };
  });
  out.forbidden_capabilities = stringArray(row.forbidden_capabilities ?? [], "expectation.forbidden_capabilities");
  out.capability_result_accepted = row.capability_result_accepted == null ? null : booleanValue(row.capability_result_accepted, "expectation.capability_result_accepted");
  out.capability_result_reason_code = stringOr(row.capability_result_reason_code);
  out.why_codes = stringArray(row.why_codes ?? [], "expectation.why_codes");
  out.forbidden_why_codes = stringArray(row.forbidden_why_codes ?? [], "expectation.forbidden_why_codes");
  return out;
}

function parseContext(value: unknown): ReturnType<typeof emptyRuntimeContext> {
  if (value == null) return emptyRuntimeContext();
  const row = asObject(value, "context");
  return {
    values: asObject(row.values ?? {}, "context.values") as Record<string, JsonValue>,
    visible_references: asArray(row.visible_references ?? [], "context.visible_references").map((raw) => { const ref = asObject(raw, "visible reference"); return { kind: requiredString(ref.kind, "reference.kind"), id: requiredString(ref.id, "reference.id") }; }),
    available_capabilities: asArray(row.available_capabilities ?? [], "context.available_capabilities").map((raw) => { const cap = asObject(raw, "available capability"); return { id: requiredString(cap.id, "capability.id"), version: requiredString(cap.version, "capability.version") }; }),
  };
}

function downloadBytes(filename: string, bytes: Uint8Array, type: string): void {
  const blob = new Blob([bytes], { type });
  const url = URL.createObjectURL(blob);
  const anchor = document.createElement("a");
  anchor.href = url;
  anchor.download = filename;
  anchor.click();
  setTimeout(() => URL.revokeObjectURL(url), 1000);
}

function safeDownloadStem(value: string): string { return value.replace(/[^a-zA-Z0-9._-]+/gu, "-").replace(/^-+|-+$/gu, "") || "gvya-project"; }

function normalizePath(value: string): string { return value.replaceAll("\\", "/").replace(/^\.\//u, ""); }
function asArray(value: unknown, path: string): unknown[] { if (!Array.isArray(value)) throw new Error(`${path}: expected array`); return value; }
function asObject(value: unknown, path: string): Record<string, unknown> { if (typeof value !== "object" || value === null || Array.isArray(value)) throw new Error(`${path}: expected object`); return value as Record<string, unknown>; }
function asOptionalObject(value: unknown, path: string): Record<string, unknown> { return value == null ? {} : asObject(value, path); }
function requiredString(value: unknown, path: string): string { if (typeof value !== "string" || value.trim() === "") throw new Error(`${path}: expected non-empty string`); return value; }
function stringOr(value: unknown, fallback = ""): string { return typeof value === "string" ? value : fallback; }
function numberOr(value: unknown, fallback: number): number { return typeof value === "number" && Number.isFinite(value) ? value : fallback; }
function nullableNumber(value: unknown): number | null { return typeof value === "number" && Number.isFinite(value) ? value : null; }
function positiveInteger(value: unknown, path: string): number { if (typeof value !== "number" || !Number.isInteger(value) || value <= 0) throw new Error(`${path}: expected positive integer`); return value; }
function nullablePositiveInteger(value: unknown, path: string): number | null { if (value == null) return null; return positiveInteger(value, path); }
function booleanValue(value: unknown, path: string): boolean { if (typeof value !== "boolean") throw new Error(`${path}: expected boolean`); return value; }
function stringArray(value: unknown, path: string): string[] { return asArray(value, path).map((row, index) => { if (typeof row !== "string") throw new Error(`${path}[${index}]: expected string`); return row; }); }
function assertKeys(value: Record<string, unknown>, allowed: string[], path: string): void { const extras = Object.keys(value).filter((key) => !allowed.includes(key)); if (extras.length) throw new Error(`${path}: unsupported keys: ${extras.join(", ")}`); }
function asJsonValue(value: unknown): JsonValue {
  if (value === null || typeof value === "string" || typeof value === "boolean") return value;
  if (typeof value === "number") { if (!Number.isFinite(value)) throw new Error("JSON number must be finite"); return value; }
  if (Array.isArray(value)) return value.map(asJsonValue);
  if (typeof value === "object") { const out: JsonObject = {}; for (const [key, row] of Object.entries(value as Record<string, unknown>)) out[key] = asJsonValue(row); return out; }
  throw new Error("Unsupported JSON value");
}
