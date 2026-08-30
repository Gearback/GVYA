import {
  assetFileKey,
  botAssetOwner,
  liveAssetOwnerKeys,
  projectAssetOwner,
  sharedAssetOwner,
  validateAssetFiles,
} from "./asset-files.js";
import { botPackageClosureIdentities, effectiveBotSettings } from "./studio-model.js";
import type { BotPackageClosureIdentity } from "./studio-model.js";
import { studioWorkspaceFromJson } from "./studio-workspace-io.js";
import { assertUniqueMatcherProfiles, languageProfilePath, languageProfileSourceDocument, matcherProfilePath, matcherProfileSourceDocument, pairProfileDocuments, parseLanguageProfileDocument, parseMatcherProfileDocument, profileCatalogDocument } from "./matcher-profiles.js";
import type { JsonObject, JsonValue, MatcherProfile, StudioAssetFile, StudioPackage, StudioWorkspace } from "./types.js";
import { packageSourceFiles, stableJson } from "./workspace.js";

const CONTENT_API = "/api/gvya-content";

interface ContentEntry {
  path: string;
  bytes_base64: string;
}

interface ContentSnapshot {
  format: "gvya.studio.content-snapshot";
  version: 1;
  revision: string;
  entries: ContentEntry[];
}

export interface LoadedStudioContent {
  workspace: StudioWorkspace;
  assetFiles: StudioAssetFile[];
  revision: string;
}

export async function loadStudioContent(): Promise<LoadedStudioContent> {
  const response = await fetch(CONTENT_API, { cache: "no-store" });
  const snapshot = await readResponse(response);
  const decoded = decodeContent(snapshot.entries);
  return { ...decoded, revision: snapshot.revision };
}

export async function saveStudioContent(workspace: StudioWorkspace, assetFiles: readonly StudioAssetFile[], revision: string): Promise<string> {
  const entries = await encodeContent(workspace, assetFiles);
  const response = await fetch(CONTENT_API, {
    method: "PUT",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ format: "gvya.studio.content-snapshot", version: 1, revision, entries }),
  });
  return (await readResponse(response)).revision;
}

export async function encodeContent(workspace: StudioWorkspace, assetFiles: readonly StudioAssetFile[]): Promise<ContentEntry[]> {
  const entries: ContentEntry[] = [];
  const addJson = (path: string, value: JsonValue): void => { entries.push(textEntry(path, stableJson(value))); };
  const addPackage = (base: string, pkg: StudioPackage): void => {
    const slash = pkg.path.lastIndexOf("/");
    const packageDir = slash < 0 ? "" : pkg.path.slice(0, slash + 1);
    for (const file of packageSourceFiles(pkg)) {
      const localPath = file.path === pkg.path ? "package.json" : file.path.slice(packageDir.length);
      addJson(`${base}/${localPath}`, file.json as unknown as JsonValue);
    }
    addJson(`${base}/authoring.json`, packageAuthoringJson(pkg));
  };
  assertSafeSegment(workspace.selectedProjectId, "selected Project ID", true);

  const canonicalTarget = canonicalProjectTarget(workspace);
  if (canonicalTarget) addJson("gvya.project.json", canonicalTarget);

  addJson("studio.json", {
    format: "gvya.studio.settings",
    version: 1,
    settings: structuredClone(workspace.settings) as unknown as JsonValue,
    selection: {
      project_id: workspace.selectedProjectId,
      bot_id: workspace.selectedBotId,
      package_scope: workspace.selectedPackageScope,
      package_id: workspace.selectedPackageId,
    },
    updated_serial: workspace.updatedSerial,
  });

  const packageLocations = new Map<string, { base: string; pkg: StudioPackage }>();
  assertUniqueMatcherProfiles(workspace.shared_matcher_profiles, "Shared Matcher Profiles");
  for (const profile of workspace.shared_matcher_profiles) {
    addJson(`shared/${languageProfilePath(profile.language)}`, languageProfileSourceDocument(profile));
    addJson(`shared/${matcherProfilePath(profile.language)}`, matcherProfileSourceDocument(profile));
  }
  for (const pkg of workspace.shared_packages) {
    const base = packageBase("shared/packages", pkg);
    addPackage(base, pkg);
    packageLocations.set(sharedAssetOwner(pkg.manifest.id), { base, pkg });
  }

  for (const project of workspace.projects) {
    assertSafeSegment(project.id, "Project ID");
    const projectBase = `projects/${project.id}`;
    addJson(`${projectBase}/project.json`, {
      format: "gvya.studio.project",
      version: 1,
      id: project.id,
      title: project.title,
      description: project.description,
    });
    assertUniqueMatcherProfiles(project.matcher_profiles, `Project ${project.id} Matcher Profiles`);
    for (const profile of project.matcher_profiles) {
      addJson(`${projectBase}/${languageProfilePath(profile.language)}`, languageProfileSourceDocument(profile));
      addJson(`${projectBase}/${matcherProfilePath(profile.language)}`, matcherProfileSourceDocument(profile));
    }
    for (const pkg of project.packages) {
      const base = packageBase(`${projectBase}/packages`, pkg);
      addPackage(base, pkg);
      packageLocations.set(projectAssetOwner(project.id, pkg.manifest.id), { base, pkg });
    }
    for (const bot of project.bots) {
      assertSafeSegment(bot.id, `Bot ID in Project ${project.id}`);
      const botBase = `${projectBase}/bots/${bot.id}`;
      const botSettings = effectiveBotSettings(workspace, bot);
      addJson(`${botBase}/bot.json`, {
        format: "gvya.studio.bot",
        version: 1,
        id: bot.id,
        title: bot.title,
        description: bot.description,
        default_language: bot.default_language,
        enabled_languages: [...bot.enabled_languages],
        package_ids: [...bot.package_ids],
        fallback_package_id: bot.fallback_package_id,
        ...botSettings,
      } as unknown as JsonValue);
      addPackage(`${botBase}/package`, bot.package);
      packageLocations.set(botAssetOwner(project.id, bot.id, bot.package.manifest.id), { base: `${botBase}/package`, pkg: bot.package });
    }
  }

  const liveOwners = liveAssetOwnerKeys(workspace);
  const liveAssets = validateAssetFiles(assetFiles.filter((file) => liveOwners.has(file.owner_key)));
  const expectedKeys = new Set<string>();
  for (const [owner, location] of packageLocations) {
    for (const contribution of location.pkg.contents.assets) {
      const asset = contribution.value;
      const key = assetFileKey(owner, asset.source);
      if (expectedKeys.has(key)) throw new Error(`Package ${location.pkg.manifest.id} declares duplicate asset source ${asset.source}.`);
      expectedKeys.add(key);
      const file = liveAssets.find((candidate) => assetFileKey(candidate.owner_key, candidate.source) === key);
      if (!file) throw new Error(`Package ${location.pkg.manifest.id} asset ${asset.source} has no source bytes.`);
      if (file.package_id !== location.pkg.manifest.id || file.media_type !== asset.media_type) throw new Error(`Asset metadata does not match ${location.pkg.manifest.id}/${asset.source}.`);
      entries.push({ path: `${location.base}/${asset.source}`, bytes_base64: bytesToBase64(new Uint8Array(await file.blob.arrayBuffer())) });
    }
  }
  for (const file of liveAssets) if (!expectedKeys.has(assetFileKey(file.owner_key, file.source))) throw new Error(`Asset file ${file.owner_key}/${file.source} is not declared by its Package.`);

  entries.sort((left, right) => compareUtf8(left.path, right.path));
  const paths = new Set(entries.map((entry) => entry.path));
  if (paths.size !== entries.length) throw new Error("Studio content contains duplicate filesystem paths.");
  return entries;
}

export function decodeContent(entries: readonly ContentEntry[]): { workspace: StudioWorkspace; assetFiles: StudioAssetFile[] } {
  const files = new Map<string, Uint8Array>();
  for (const entry of entries) {
    if (!safeContentPath(entry.path)) throw new Error(`Studio content has unsafe path ${entry.path}.`);
    if (files.has(entry.path)) throw new Error(`Studio content has duplicate path ${entry.path}.`);
    files.set(entry.path, base64ToBytes(entry.bytes_base64));
  }
  const consumed = new Set<string>();
  const settings = readJsonFile(files, consumed, "studio.json");
  exactKeys(settings, ["format", "version", "settings", "selection", "updated_serial"], "studio.json");
  if (settings.format !== "gvya.studio.settings" || settings.version !== 1) throw new Error("studio.json must be gvya.studio.settings version 1.");
  const selection = record(settings.selection, "studio.json#selection");
  exactKeys(selection, ["project_id", "bot_id", "package_scope", "package_id"], "studio.json#selection");
  const settingsRow = record(settings.settings, "studio.json#settings");

  const sharedPackages: unknown[] = [];
  const sharedMatcherProfiles: MatcherProfile[] = [];
  const projects: Array<{ id: string; title: unknown; description: unknown; matcher_profiles: unknown[]; packages: unknown[]; bots: unknown[] }> = [];
  const projectManifests = [...files.keys()].filter((path) => /^projects\/[^/]+\/project\.json$/u.test(path)).sort(compareUtf8);
  const sharedManifests = [...files.keys()].filter((path) => /^shared\/packages\/(?:standard|fallback)\/[^/]+\/package\.json$/u.test(path)).sort(compareUtf8);
  const sharedMatcherPaths = [...files.keys()].filter((path) => /^shared\/matcher-profiles\/[^/]+\.json$/u.test(path)).sort(compareUtf8);
  const sharedLanguagePaths = [...files.keys()].filter((path) => /^shared\/language-profiles\/[^/]+\.json$/u.test(path)).sort(compareUtf8);

  sharedMatcherProfiles.push(...readProfilePairs(files, consumed, sharedLanguagePaths, sharedMatcherPaths, "Shared"));
  const sharedAuthoringLanguage = sharedMatcherProfiles[0]?.language ?? "und";
  for (const path of sharedManifests) sharedPackages.push(readPackage(files, consumed, path, sharedAuthoringLanguage));
  for (const manifestPath of projectManifests) {
    const projectFolder = manifestPath.split("/")[1]!;
    const manifest = readJsonFile(files, consumed, manifestPath);
    exactKeys(manifest, ["format", "version", "id", "title", "description"], manifestPath);
    if (manifest.format !== "gvya.studio.project" || manifest.version !== 1) throw new Error(`${manifestPath} must be gvya.studio.project version 1.`);
    if (manifest.id !== projectFolder) throw new Error(`${manifestPath} id must match its folder name.`);
    const packagePrefix = `projects/${projectFolder}/packages/`;
    const packagePaths = [...files.keys()].filter((path) => path.startsWith(packagePrefix) && /\/package\.json$/u.test(path) && /^projects\/[^/]+\/packages\/(?:standard|fallback)\/[^/]+\/package\.json$/u.test(path)).sort(compareUtf8);
    const matcherProfilePaths = [...files.keys()].filter((path) => new RegExp(`^projects/${escapeRegExp(projectFolder)}/matcher-profiles/[^/]+\\.json$`, "u").test(path)).sort(compareUtf8);
    const languageProfilePaths = [...files.keys()].filter((path) => new RegExp(`^projects/${escapeRegExp(projectFolder)}/language-profiles/[^/]+\\.json$`, "u").test(path)).sort(compareUtf8);
    const projectMatcherProfiles = readProfilePairs(files, consumed, languageProfilePaths, matcherProfilePaths, `Project ${projectFolder}`);
    const projectAuthoringLanguage = projectMatcherProfiles[0]?.language ?? "und";
    const botManifestPaths = [...files.keys()].filter((path) => new RegExp(`^projects/${escapeRegExp(projectFolder)}/bots/[^/]+/bot\\.json$`, "u").test(path)).sort(compareUtf8);
    const bots: unknown[] = [];
    for (const botManifestPath of botManifestPaths) {
      const botFolder = botManifestPath.split("/")[3]!;
      const botManifest = readJsonFile(files, consumed, botManifestPath);
      exactKeys(botManifest, ["format", "version", "id", "title", "description", "default_language", "enabled_languages", "package_ids", "fallback_package_id", "emit_debug_map", "semantic", "conversation"], botManifestPath);
      if (botManifest.format !== "gvya.studio.bot" || botManifest.version !== 1) throw new Error(`${botManifestPath} must be gvya.studio.bot version 1.`);
      if (botManifest.id !== botFolder) throw new Error(`${botManifestPath} id must match its folder name.`);
      bots.push({
        id: botManifest.id,
        title: botManifest.title,
        description: botManifest.description,
        default_language: botManifest.default_language,
        enabled_languages: botManifest.enabled_languages,
        package_ids: botManifest.package_ids,
        fallback_package_id: botManifest.fallback_package_id,
        settings: { emit_debug_map: botManifest.emit_debug_map, semantic: botManifest.semantic, conversation: botManifest.conversation },
        package: readPackage(files, consumed, `projects/${projectFolder}/bots/${botFolder}/package/package.json`, String(botManifest.default_language)),
      });
    }
    projects.push({
      id: projectFolder,
      title: manifest.title,
      description: manifest.description,
      matcher_profiles: projectMatcherProfiles.map(profileCatalogDocument),
      packages: packagePaths.map((path) => readPackage(files, consumed, path, projectAuthoringLanguage)),
      bots,
    });
  }

  const normalized = normalizeSelection(projects, sharedPackages, selection);
  const workspace = studioWorkspaceFromJson({
    format: "gvya.studio.workspace",
    version: 1,
    shared_matcher_profiles: sharedMatcherProfiles.map(profileCatalogDocument),
    shared_packages: sharedPackages,
    settings: settingsRow,
    projects,
    selectedProjectId: normalized.projectId,
    selectedBotId: normalized.botId,
    selectedPackageScope: normalized.packageScope,
    selectedPackageId: normalized.packageId,
    updatedSerial: settings.updated_serial,
  });

  const canonicalTarget = canonicalProjectTarget(workspace);
  const canonicalSource = readOptionalJsonFile(files, consumed, "gvya.project.json");
  if (canonicalTarget && !canonicalSource) throw new Error("Studio content is missing the canonical gvya.project.json target for its selected Bot.");
  if (!canonicalTarget && canonicalSource) throw new Error("Studio content has gvya.project.json but no selected Bot target.");
  if (canonicalTarget && canonicalSource && stableJson(canonicalSource as JsonObject) !== stableJson(canonicalTarget)) {
    throw new Error("gvya.project.json must exactly describe the selected Studio Bot and its declared source paths.");
  }

  const assets: StudioAssetFile[] = [];
  const addPackageAssets = (pkg: StudioPackage, owner: string, base: string): void => {
    for (const contribution of pkg.contents.assets) {
      const asset = contribution.value;
      const path = `${base}/${asset.source}`;
      const bytes = files.get(path);
      if (!bytes) throw new Error(`Package ${pkg.manifest.id} asset file is missing: ${path}`);
      consumed.add(path);
      assets.push({ owner_key: owner, package_id: pkg.manifest.id, source: asset.source, media_type: asset.media_type, blob: new Blob([bytes], { type: asset.media_type }) });
    }
  };
  for (const pkg of workspace.shared_packages) addPackageAssets(pkg, sharedAssetOwner(pkg.manifest.id), packageBase("shared/packages", pkg));
  for (const project of workspace.projects) {
    for (const pkg of project.packages) addPackageAssets(pkg, projectAssetOwner(project.id, pkg.manifest.id), packageBase(`projects/${project.id}/packages`, pkg));
    for (const bot of project.bots) addPackageAssets(bot.package, botAssetOwner(project.id, bot.id, bot.package.manifest.id), `projects/${project.id}/bots/${bot.id}/package`);
  }
  const unknown = [...files.keys()].filter((path) => !consumed.has(path)).sort(compareUtf8);
  if (unknown.length) throw new Error(`Studio content contains an unowned file: ${unknown[0]}`);
  return { workspace, assetFiles: validateAssetFiles(assets) };
}

function readMatcherProfile(files: Map<string, Uint8Array>, consumed: Set<string>, path: string): MatcherProfile {
  return parseMatcherProfileDocument(readJsonFile(files, consumed, path), path);
}

function readLanguageProfile(files: Map<string, Uint8Array>, consumed: Set<string>, path: string): MatcherProfile {
  return parseLanguageProfileDocument(readJsonFile(files, consumed, path), path);
}

function readProfilePairs(
  files: Map<string, Uint8Array>,
  consumed: Set<string>,
  languagePaths: readonly string[],
  matcherPaths: readonly string[],
  label: string,
): MatcherProfile[] {
  const languages = new Map(languagePaths.map((path) => {
    const profile = readLanguageProfile(files, consumed, path);
    return [profile.language.toLowerCase(), profile] as const;
  }));
  const matchers = matcherPaths.map((path) => readMatcherProfile(files, consumed, path));
  const paired = matchers.map((matcher) => {
    const language = languages.get(matcher.language.toLowerCase());
    if (!language) throw new Error(`${label} Matcher Profile ${matcher.language} has no paired Language Profile.`);
    return pairProfileDocuments(language, matcher, `${label} ${matcher.language}`);
  });
  if (languages.size !== paired.length) throw new Error(`${label} Language and Matcher Profile catalogs must contain the same languages.`);
  return paired;
}

function packageAuthoringJson(pkg: StudioPackage): JsonObject {
  return {
    format: "gvya.studio.package-authoring",
    version: 1,
    authoring_language: pkg.authoring_language,
  };
}

const PACKAGE_FRAGMENT_NAMESPACES = [
  "meanings", "behaviors", "capability_result_behaviors", "openings", "fallback_behaviors",
  "style_lexicons", "capabilities", "capability_bindings", "capability_policies",
  "capability_configs", "types", "assets", "regression_cases", "scenarios",
] as const;

function readPackage(files: Map<string, Uint8Array>, consumed: Set<string>, path: string, defaultAuthoringLanguage: string): JsonObject {
  const botMatch = path.match(/^projects\/([^/]+)\/bots\/([^/]+)\/package\/package\.json$/u);
  const scopedMatch = path.match(/^(?:shared|projects\/[^/]+)\/packages\/(standard|fallback)\/([^/]+)\/package\.json$/u);
  if (!botMatch && !scopedMatch) throw new Error(`Package path is outside the content layout: ${path}`);
  const source = readJsonFile(files, consumed, path);
  exactKeys(source, ["format", "version", "manifest", "fragments"], path);
  if (source.format !== "gvya.source.package" || source.version !== 1) throw new Error(`${path} must be canonical gvya.source.package version 1.`);
  const manifest = record(source.manifest, `${path}#manifest`);
  const fragments = record(source.fragments, `${path}#fragments`);
  const supportedNamespaces = new Set<string>(PACKAGE_FRAGMENT_NAMESPACES);
  for (const key of Object.keys(fragments)) if (!supportedNamespaces.has(key)) throw new Error(`${path}#fragments contains unsupported namespace ${key}.`);
  const folderId = scopedMatch?.[2] ?? null;
  if (folderId !== null && manifest.id !== folderId) throw new Error(`${path} Package ID must match its folder name.`);
  if (scopedMatch && manifest.kind !== scopedMatch[1]) throw new Error(`${path} Package kind must match its standard/fallback folder.`);

  const slash = path.lastIndexOf("/");
  const packageBase = slash < 0 ? "" : path.slice(0, slash);
  const contents: JsonObject = {};
  const seen = new Set<string>();
  for (const namespace of PACKAGE_FRAGMENT_NAMESPACES) {
    const value = fragments[namespace];
    if (value === undefined) {
      contents[namespace] = [];
      continue;
    }
    if (!Array.isArray(value)) throw new Error(`${path}#fragments.${namespace} must be an array.`);
    const rows: JsonValue[] = [];
    for (let index = 0; index < value.length; index += 1) {
      const relative = value[index];
      if (typeof relative !== "string" || !relative.startsWith("fragments/") || !relative.endsWith(".json") || !safeContentPath(relative)) throw new Error(`${path}#fragments.${namespace}[${index}] must be a safe package-local fragments/*.json path.`);
      const fragmentPath = packageBase ? `${packageBase}/${relative}` : relative;
      if (seen.has(fragmentPath)) throw new Error(`${path} declares fragment ${relative} more than once.`);
      seen.add(fragmentPath);
      rows.push(readJsonFile(files, consumed, fragmentPath) as JsonValue);
    }
    contents[namespace] = rows;
  }

  const authoringPath = path.replace(/package\.json$/u, "authoring.json");
  const authoring = readOptionalJsonFile(files, consumed, authoringPath);
  let authoringLanguage = defaultAuthoringLanguage;
  if (authoring) {
    exactKeys(authoring, ["format", "version", "authoring_language"], authoringPath);
    if (authoring.format !== "gvya.studio.package-authoring" || authoring.version !== 1) throw new Error(`${authoringPath} must be gvya.studio.package-authoring version 1.`);
    if (typeof authoring.authoring_language !== "string" || !authoring.authoring_language.trim()) throw new Error(`${authoringPath} authoring_language must be a non-empty string.`);
    authoringLanguage = authoring.authoring_language;
  }
  if (botMatch && authoringLanguage.toLowerCase() !== defaultAuthoringLanguage.toLowerCase()) throw new Error(`${authoringPath} authoring_language must equal its Bot default language.`);
  const persisted: JsonObject = {
    path: `packages/${String(manifest.id)}/package.json`,
    manifest: source.manifest as JsonValue,
    contents,
  };
  if (!botMatch) persisted.authoring_language = authoringLanguage;
  return persisted;
}

function normalizeSelection(projects: Array<{ id: string; bots: unknown[] }>, sharedPackages: unknown[], selection: Record<string, unknown>) {
  const requestedProject = typeof selection.project_id === "string" ? selection.project_id : "";
  const project = projects.find((row) => row.id === requestedProject) ?? projects[0] ?? null;
  const requestedBot = typeof selection.bot_id === "string" ? selection.bot_id : "";
  const bots = project?.bots as Array<Record<string, unknown>> | undefined;
  const bot = bots?.find((row) => row.id === requestedBot) ?? bots?.[0] ?? null;
  const scope = selection.package_scope === "shared" || selection.package_scope === "project" || selection.package_scope === "bot" ? selection.package_scope : "bot";
  const packageId = typeof selection.package_id === "string" ? selection.package_id : "";
  if (!project) {
    const firstShared = sharedPackages[0] as Record<string, unknown> | undefined;
    const manifest = firstShared ? record(firstShared.manifest, "Shared Package manifest") : null;
    return { projectId: "", botId: "", packageScope: "shared" as const, packageId: manifest && typeof manifest.id === "string" ? manifest.id : "" };
  }
  if (!bot) return { projectId: project.id, botId: "", packageScope: "project" as const, packageId: "" };
  return { projectId: project.id, botId: String(bot.id), packageScope: scope, packageId: scope === "bot" ? String(record(bot.package, "Bot Package").manifest && record(record(bot.package, "Bot Package").manifest, "Bot Package manifest").id) : packageId };
}

function packageBase(parent: string, pkg: StudioPackage): string {
  assertSafeSegment(pkg.manifest.id, "Package ID");
  return `${parent}/${pkg.manifest.kind}/${pkg.manifest.id}`;
}

function canonicalProjectTarget(workspace: StudioWorkspace): JsonObject | null {
  const project = workspace.projects.find((row) => row.id === workspace.selectedProjectId) ?? workspace.projects[0] ?? null;
  if (!project) return null;
  const bot = project.bots.find((row) => row.id === workspace.selectedBotId) ?? project.bots[0] ?? null;
  if (!bot) return null;

  // One authoritative closure: selected Packages + Bot Package + transitive required
  // dependencies + Fallback closure. Never the rest of the Project catalog.
  const closure = botPackageClosureIdentities(workspace, project, bot);
  const contentPath = (entry: BotPackageClosureIdentity): string => {
    if (entry.scope === "bot") return `projects/${project.id}/bots/${bot.id}/package/package.json`;
    const parent = entry.scope === "project" ? `projects/${project.id}/packages` : "shared/packages";
    assertSafeSegment(entry.id, "Package ID");
    return `${parent}/${entry.kind}/${entry.id}/package.json`;
  };
  const packagePaths = closure.filter((entry) => entry.kind === "standard").map(contentPath);
  const fallbackEntries = closure.filter((entry) => entry.kind === "fallback");
  if (fallbackEntries.length > 1) throw new Error(`Selected Bot ${bot.id} resolves more than one Fallback Package.`);
  const fallbackPackage: string | null = fallbackEntries[0] ? contentPath(fallbackEntries[0]) : null;

  const settings = effectiveBotSettings(workspace, bot);
  const result: JsonObject = {
    format: "gvya.source.project",
    version: 1,
    project_id: project.id,
    brain_id: bot.id,
    languages: project.matcher_profiles.map((profile) => profile.language),
    enabled_languages: [...bot.enabled_languages],
    default_language: bot.default_language,
    language_profiles: bot.enabled_languages.map((language) => `projects/${project.id}/${languageProfilePath(language)}`),
    matcher_profiles: bot.enabled_languages.map((language) => `projects/${project.id}/${matcherProfilePath(language)}`),
    packages: packagePaths,
    semantic: structuredClone(settings.semantic) as unknown as JsonValue,
    conversation: structuredClone(settings.conversation) as unknown as JsonValue,
    emit_debug_map: settings.emit_debug_map,
  };
  if (fallbackPackage) result.fallback_package = fallbackPackage;
  return result;
}

function readJsonFile(files: Map<string, Uint8Array>, consumed: Set<string>, path: string): Record<string, unknown> {
  const bytes = files.get(path);
  if (!bytes) throw new Error(`Required Studio content file is missing: ${path}`);
  consumed.add(path);
  let value;
  try { value = JSON.parse(new TextDecoder("utf-8", { fatal: true }).decode(bytes)); }
  catch { throw new Error(`Studio content file is not valid UTF-8 JSON: ${path}`); }
  return record(value, path);
}

function readOptionalJsonFile(files: Map<string, Uint8Array>, consumed: Set<string>, path: string): Record<string, unknown> | null {
  if (!files.has(path)) return null;
  return readJsonFile(files, consumed, path);
}

function textEntry(path: string, value: string): ContentEntry { return { path, bytes_base64: bytesToBase64(new TextEncoder().encode(value)) }; }
function compareUtf8(left: string, right: string): number { const a = new TextEncoder().encode(left); const b = new TextEncoder().encode(right); const length = Math.min(a.length, b.length); for (let i = 0; i < length; i += 1) if (a[i] !== b[i]) return a[i]! - b[i]!; return a.length - b.length; }
function assertSafeSegment(value: string, label: string, empty = false): void { if (empty && value === "") return; if (!/^[A-Za-z0-9][A-Za-z0-9._-]*$/u.test(value)) throw new Error(`${label} is not a safe portable folder name: ${value}`); }
function safeContentPath(value: string): boolean { return value.length > 0 && value.length <= 512 && !value.startsWith("/") && !value.includes("\\") && !value.includes("\0") && value.split("/").every((part) => part !== "" && part !== "." && part !== ".."); }
function record(value: unknown, label: string): Record<string, unknown> { if (!value || typeof value !== "object" || Array.isArray(value)) throw new Error(`${label} must be an object.`); return value as Record<string, unknown>; }
function exactKeys(value: Record<string, unknown>, keys: readonly string[], label: string): void { const actual = Object.keys(value).sort(); const expected = [...keys].sort(); if (JSON.stringify(actual) !== JSON.stringify(expected)) throw new Error(`${label} has unsupported or missing fields.`); }
function escapeRegExp(value: string): string { return value.replace(/[.*+?^${}()|[\]\\]/gu, "\\$&"); }
function bytesToBase64(bytes: Uint8Array): string { let binary = ""; for (let offset = 0; offset < bytes.length; offset += 0x8000) binary += String.fromCharCode(...bytes.subarray(offset, offset + 0x8000)); return btoa(binary); }
function base64ToBytes(value: string): Uint8Array { let binary: string; try { binary = atob(value); } catch { throw new Error("Studio content contains invalid base64 bytes."); } const bytes = new Uint8Array(binary.length); for (let i = 0; i < binary.length; i += 1) bytes[i] = binary.charCodeAt(i); return bytes; }

async function readResponse(response: Response): Promise<ContentSnapshot> {
  const value = await response.json() as unknown;
  if (!response.ok) {
    const error = value && typeof value === "object" && "error" in value ? String((value as { error: unknown }).error) : `HTTP ${response.status}`;
    throw new Error(error);
  }
  const row = record(value, "Studio content response");
  exactKeys(row, ["format", "version", "revision", "entries"], "Studio content response");
  if (row.format !== "gvya.studio.content-snapshot" || row.version !== 1 || typeof row.revision !== "string" || !Array.isArray(row.entries)) throw new Error("Studio content response is invalid.");
  return row as unknown as ContentSnapshot;
}
