import { compareUtf8 } from "./canonical-order.js";
import type {
  ConversationConfig,
  SemanticConfig,
  StudioBot,
  StudioBotConversationSettings,
  StudioBrainWorkspace,
  StudioConversationDefaults,
  StudioPackage,
  StudioProject,
  StudioWorkspace,
} from "./types.js";
import { languageKey } from "./languages.js";
import {
  matcherProfileLanguages,
  matcherProfilesForLanguages,
  missingMatcherProfileLanguages,
  packageAuthoredLanguages,
  packageLanguages,
  packageMatcherEvidenceLanguages,
} from "./matcher-profiles.js";
import { createPackage, createStarterBrainWorkspace } from "./workspace.js";

type PackageNamespace = keyof StudioPackage["contents"];
type ContributionLike = { id: string; exported: boolean; mode: "add" | { type: "replace"; target_package: string; target_id: string }; value: unknown };

export interface OverrideableContribution { namespace: PackageNamespace; id: string; source_package: string; }
export interface ProjectPackageRemovalImpact { package_ids: string[]; bot_ids: string[]; }
export type BotPackageScope = "shared" | "project" | "bot";
export interface BotPackageClosureEntry { scope: BotPackageScope; package: StudioPackage; }
export interface BotPackageEligibility { package: StudioPackage; eligible: boolean; missing_languages: string[]; }
export interface BotPackageClosureIdentity { scope: BotPackageScope; id: string; kind: "standard" | "fallback"; }

export function createStarterStudioWorkspace(): StudioWorkspace {
  const starter = createStarterBrainWorkspace();
  const projectId = "starter-project";
  const botId = "main-bot";
  const bot: StudioBot = {
    id: botId,
    title: "Main bot",
    description: "Primary compiled Brain",
    default_language: "en-US",
    enabled_languages: ["en-US"],
    package_ids: [],
    fallback_package_id: null,
    package: createPackage(`${projectId}.${botId}.bot`, "Package owned by Main bot", "standard", "en-US"),
    settings: { emit_debug_map: starter.emit_debug_map, semantic: {}, conversation: { author_numbers: [] } },
  };
  const project: StudioProject = {
    id: projectId,
    title: "Starter project",
    description: "GVYA project",
    matcher_profiles: [],
    packages: [],
    bots: [bot],
  };
  return {
    format: "gvya.studio.workspace",
    version: 1,
    shared_matcher_profiles: [],
    shared_packages: [],
    settings: { semantic: structuredClone(starter.semantic), conversation: conversationDefaults(starter.conversation) },
    projects: [project],
    selectedProjectId: project.id,
    selectedBotId: bot.id,
    selectedPackageScope: "bot",
    selectedPackageId: bot.package.manifest.id,
    updatedSerial: 1,
  };
}

export function cloneStudioWorkspace(workspace: StudioWorkspace): StudioWorkspace { return structuredClone(workspace); }
export function touchStudioWorkspace(workspace: StudioWorkspace): StudioWorkspace { return { ...workspace, updatedSerial: workspace.updatedSerial + 1 }; }

export function selectedProject(workspace: StudioWorkspace): StudioProject {
  const project = workspace.projects.find((row) => row.id === workspace.selectedProjectId) ?? workspace.projects[0];
  if (!project) throw new Error("Studio workspace has no Project.");
  return project;
}

export function selectedBot(workspace: StudioWorkspace, project = selectedProject(workspace)): StudioBot {
  const bot = project.bots.find((row) => row.id === workspace.selectedBotId) ?? project.bots[0];
  if (!bot) throw new Error(`Project ${project.id} has no Bot.`);
  return bot;
}

export function sharedAvailableLanguages(workspace: StudioWorkspace): string[] {
  return matcherProfileLanguages(workspace.shared_matcher_profiles);
}

export function projectAvailableLanguages(project: StudioProject): string[] {
  return matcherProfileLanguages(project.matcher_profiles);
}

function sharedPackageById(workspace: StudioWorkspace, id: string): StudioPackage | null {
  return workspace.shared_packages.find((pkg) => pkg.manifest.id === id) ?? null;
}

/** Live Shared+Project catalog references for one Package kind. Callers that hand Packages to an
 * editor or to a materialized Brain must clone; identity-only callers must not pay that cost. */
function botPackageCatalogRefs(workspace: StudioWorkspace, project: StudioProject, kind: "standard" | "fallback"): StudioPackage[] {
  return dedupePackages([
    ...workspace.shared_packages.filter((pkg) => pkg.manifest.kind === kind),
    ...project.packages.filter((pkg) => pkg.manifest.kind === kind),
  ]);
}

function botPackageCatalog(workspace: StudioWorkspace, project: StudioProject, kind: "standard" | "fallback"): StudioPackage[] {
  return botPackageCatalogRefs(workspace, project, kind).map((pkg) => structuredClone(pkg));
}

/** Shared and Project-owned Standard Packages visible to a Bot. Shared source remains owned by Shared scope. */
export function botAttachablePackages(workspace: StudioWorkspace, project = selectedProject(workspace)): StudioPackage[] {
  return botPackageCatalog(workspace, project, "standard");
}

export function botSelectablePackages(workspace: StudioWorkspace, project = selectedProject(workspace), bot = selectedBot(workspace, project)): StudioPackage[] {
  return botPackageEligibility(workspace, project, bot, "standard")
    .filter((row) => row.eligible)
    .map((row) => row.package);
}

export function botSelectableFallbackPackages(workspace: StudioWorkspace, project = selectedProject(workspace), bot = selectedBot(workspace, project)): StudioPackage[] {
  return botPackageEligibility(workspace, project, bot, "fallback")
    .filter((row) => row.eligible)
    .map((row) => row.package);
}

/**
 * Every attachable Package of one kind with the exact reason it can or cannot join this Bot.
 *
 * Selection surfaces render the whole catalog from this so an ineligible Package stays visible with
 * its missing languages instead of silently disappearing; mutation guards use the same rows.
 */
export function botPackageEligibility(
  workspace: StudioWorkspace,
  project = selectedProject(workspace),
  bot = selectedBot(workspace, project),
  kind: "standard" | "fallback" = "standard",
): BotPackageEligibility[] {
  return botPackageCatalog(workspace, project, kind).map((pkg) => {
    const missing_languages = packageLanguageGapsForBot(workspace, project, bot, pkg);
    return { package: pkg, eligible: missing_languages.length === 0, missing_languages };
  });
}

/** Packages owned by a Project. Shared Packages are global and are never attached or overridden at Project scope. */
export function projectVisiblePackages(_workspace: StudioWorkspace, project = selectedProject(_workspace)): StudioPackage[] {
  return project.packages.map((pkg) => structuredClone(pkg));
}

/**
 * The one authoritative answer to "which Packages belong to this Bot".
 *
 * Membership is exactly: the Packages the Bot selects, the Bot Package, every transitively
 * required dependency of those, the selected Fallback Package, and that Fallback Package's own
 * required closure. A Package that merely exists in the Project or Shared catalog is never a
 * member. Ordering is dependency-first depth-first over the Bot's selection order, then the Bot
 * Package, then the Fallback closure, so every consumer emits the same canonical order.
 *
 * Every consumer -- simulation, build, `.gvya` compilation, Web Export, source export and the
 * canonical Studio content target -- resolves through this function. Do not reconstruct a Bot's
 * package list anywhere else.
 */
export function resolveBotPackageClosure(
  workspace: StudioWorkspace,
  project = selectedProject(workspace),
  bot = selectedBot(workspace, project),
): BotPackageClosureEntry[] {
  return botPackageClosureRefs(workspace, project, bot)
    .map((entry) => ({ scope: entry.scope, package: structuredClone(entry.package) }));
}

function botPackageClosureRefs(workspace: StudioWorkspace, project: StudioProject, bot: StudioBot): BotPackageClosureEntry[] {
  const attachable = botPackageCatalogRefs(workspace, project, "standard");
  const attachableById = new Map(attachable.map((pkg) => [pkg.manifest.id, pkg]));
  const roots: StudioPackage[] = [];

  for (const id of bot.package_ids) {
    const pkg = attachableById.get(id);
    if (!pkg) throw new Error(`Bot ${bot.id} references unavailable package ${id}.`);
    roots.push(pkg);
  }
  roots.push(bot.package);

  const resolved = resolvePackageGraphRefs([...attachable, bot.package], roots);
  if (bot.fallback_package_id !== null) {
    const fallbackCatalog = botPackageCatalogRefs(workspace, project, "fallback");
    const fallback = fallbackCatalog.find((pkg) => pkg.manifest.id === bot.fallback_package_id);
    if (!fallback) throw new Error(`Bot ${bot.id} references unavailable Fallback Package ${bot.fallback_package_id}.`);
    const included = new Set(resolved.map((pkg) => pkg.manifest.id));
    for (const pkg of resolvePackageGraphRefs([...attachable, ...fallbackCatalog], [fallback])) {
      if (!included.has(pkg.manifest.id)) { included.add(pkg.manifest.id); resolved.push(pkg); }
    }
  }

  const projectIds = new Set(project.packages.map((pkg) => pkg.manifest.id));
  return resolved.map((pkg) => ({
    scope: pkg.manifest.id === bot.package.manifest.id ? "bot" : projectIds.has(pkg.manifest.id) ? "project" : "shared",
    package: pkg,
  }));
}

/**
 * The same closure as [`resolveBotPackageClosure`], reduced to the identity a path/list consumer
 * needs. Deep-cloning whole Packages is only for callers that hand them to an editor or a
 * materialized Brain; the canonical source target and every count must not pay for it.
 */
export function botPackageClosureIdentities(
  workspace: StudioWorkspace,
  project = selectedProject(workspace),
  bot = selectedBot(workspace, project),
): BotPackageClosureIdentity[] {
  return botPackageClosureRefs(workspace, project, bot)
    .map((entry) => ({ scope: entry.scope, id: entry.package.manifest.id, kind: entry.package.manifest.kind }));
}

/** Closure Package IDs for a Bot, or `null` when that Bot selection cannot currently be resolved. */
export function botPackageClosureIds(workspace: StudioWorkspace, project: StudioProject, bot: StudioBot): string[] | null {
  try { return botPackageClosureIdentities(workspace, project, bot).map((entry) => entry.id); } catch { return null; }
}

/** Closure Package IDs for every Bot in a Project, resolved once for list/count rendering. */
export function projectBotPackageClosureIds(workspace: StudioWorkspace, project = selectedProject(workspace)): Map<string, string[] | null> {
  return new Map(project.bots.map((bot) => [bot.id, botPackageClosureIds(workspace, project, bot)]));
}

function botVisiblePackages(workspace: StudioWorkspace, project = selectedProject(workspace), bot = selectedBot(workspace, project)): StudioPackage[] {
  return resolveBotPackageClosure(workspace, project, bot).map((entry) => entry.package);
}

export function resolveSelectedBrain(workspace: StudioWorkspace): StudioBrainWorkspace {
  const project = selectedProject(workspace); const bot = selectedBot(workspace, project);
  const packages = botVisiblePackages(workspace, project, bot);
  const selected = packages.find((pkg) => pkg.manifest.id === workspace.selectedPackageId)?.manifest.id ?? bot.package.manifest.id;
  const selectedPackage = packages.find((pkg) => pkg.manifest.id === selected);
  return {
    format: "gvya.studio.brain-view", version: 1, project_id: project.id, brain_id: bot.id,
    languages: projectAvailableLanguages(project),
    enabled_languages: structuredClone(bot.enabled_languages),
    default_language: bot.default_language,
    authoring_language: selected === bot.package.manifest.id ? bot.default_language : selectedPackage?.authoring_language ?? bot.default_language,
    emit_debug_map: bot.settings.emit_debug_map,
    semantic: mergeConfig(workspace.settings.semantic, bot.settings.semantic),
    conversation: effectiveBotConversation(workspace.settings.conversation, bot.settings.conversation),
    matcher_profiles: matcherProfilesForLanguages(bot.enabled_languages, project.matcher_profiles),
    packages, selectedPackageId: selected, updatedSerial: workspace.updatedSerial,
  };
}

/** Builds a transient, Package-rooted compile view with no Bot composition or Bot setting overrides. */
export function resolvePackagePreview(workspace: StudioWorkspace): StudioBrainWorkspace {
  let target: StudioPackage;
  let available: StudioPackage[];
  let projectId = "package-preview";
  let languages = sharedAvailableLanguages(workspace);
  let defaultLanguage = languages[0] ?? "und";
  let enabledLanguages = structuredClone(languages);
  let authoringLanguage = defaultLanguage;
  let matcherProfileCatalog = workspace.shared_matcher_profiles;

  if (workspace.selectedPackageScope === "shared") {
    const selected = sharedPackageById(workspace, workspace.selectedPackageId);
    if (!selected) throw new Error(`Shared Package ${workspace.selectedPackageId} does not exist.`);
    target = selected;
    authoringLanguage = target.authoring_language;
    const standardPackages = workspace.shared_packages.filter((pkg) => pkg.manifest.kind === "standard");
    available = target.manifest.kind === "fallback" ? [...standardPackages, target] : standardPackages;
  } else {
    const project = selectedProject(workspace);
    matcherProfileCatalog = project.matcher_profiles;
    projectId = project.id;
    languages = projectAvailableLanguages(project);
    defaultLanguage = languages[0] ?? "und";
    enabledLanguages = structuredClone(languages);
    const standardPackages = botAttachablePackages(workspace, project);
    if (workspace.selectedPackageScope === "project") {
      const selected = project.packages.find((pkg) => pkg.manifest.id === workspace.selectedPackageId);
      if (!selected) throw new Error(`Project Package ${workspace.selectedPackageId} does not exist.`);
      target = selected;
      authoringLanguage = target.authoring_language;
      available = target.manifest.kind === "fallback" ? [...standardPackages, target] : standardPackages;
    } else {
      const bot = selectedBot(workspace, project);
      defaultLanguage = bot.default_language;
      enabledLanguages = structuredClone(bot.enabled_languages);
      authoringLanguage = bot.default_language;
      if (bot.package.manifest.id !== workspace.selectedPackageId) throw new Error(`Bot Package ${workspace.selectedPackageId} does not exist.`);
      target = bot.package;
      available = [...standardPackages, bot.package];
    }
  }

  return {
    format: "gvya.studio.brain-view",
    version: 1,
    project_id: projectId,
    brain_id: "package-preview",
    languages,
    enabled_languages: enabledLanguages,
    default_language: defaultLanguage,
    authoring_language: authoringLanguage,
    emit_debug_map: false,
    semantic: structuredClone(workspace.settings.semantic),
    conversation: previewConversation(workspace.settings.conversation),
    matcher_profiles: matcherProfilesForLanguages(enabledLanguages, matcherProfileCatalog),
    packages: resolvePackageGraph(available, [target]),
    selectedPackageId: target.manifest.id,
    updatedSerial: workspace.updatedSerial,
  };
}

function missingPackageMatcherLanguages(packages: readonly StudioPackage[], profiles: StudioProject["matcher_profiles"]): string[] {
  return missingMatcherProfileLanguages(packages.flatMap(packageLanguages), profiles);
}

/**
 * The one language rule for compiling Packages as a given Bot. It mirrors the canonical compiler:
 *
 * 1. Matcher evidence (Meaning structural patterns, samples, negative samples, retrieval terms)
 *    needs a compiled Semantic Profile, and a compiled program's profile map is keyed exactly by
 *    the Bot's enabled languages. Evidence in any other language fails the build with
 *    `MissingLanguageProfile`.
 * 2. Every other authored language — response texts, Regression Case and Scenario languages — only
 *    has to be named by the Project language catalog, which `validate_project_language_usage`
 *    enforces as `source.language_not_allowed`.
 *
 * Both checks run against the Bot's own state, never a Project-level approximation of it, because
 * `enabled_languages` is what execution uses. Returned tags are the exact authored spellings, in
 * stable first-use order, so a diagnostic can name what an author must enable or author.
 */
function botCompileLanguageGaps(
  project: StudioProject,
  bot: StudioBot,
  packages: readonly StudioPackage[],
): string[] {
  const evidence = packages.flatMap(packageMatcherEvidenceLanguages);
  const authored = packages.flatMap(packageAuthoredLanguages);
  return dedupeLanguages([
    ...missingMatcherProfileLanguages(evidence, matcherProfilesForLanguages(bot.enabled_languages, project.matcher_profiles)),
    ...missingMatcherProfileLanguages(authored, project.matcher_profiles),
    ...missingMatcherProfileLanguages(bot.enabled_languages, project.matcher_profiles),
  ]);
}

function dedupeLanguages(languages: readonly string[]): string[] {
  const seen = new Set<string>();
  return languages.filter((language) => {
    const key = languageKey(language);
    if (seen.has(key)) return false;
    seen.add(key);
    return true;
  });
}

/**
 * Languages that block one candidate Package — and everything it requires — from joining this Bot.
 * An unresolvable dependency graph is reported as a hard block rather than swallowed.
 */
function packageLanguageGapsForBot(
  workspace: StudioWorkspace,
  project: StudioProject,
  bot: StudioBot,
  root: StudioPackage,
): string[] {
  const availableById = new Map<string, StudioPackage>();
  for (const pkg of workspace.shared_packages) availableById.set(pkg.manifest.id, pkg);
  for (const pkg of project.packages) availableById.set(pkg.manifest.id, pkg);
  availableById.set(root.manifest.id, root);
  let installed: StudioPackage[];
  try {
    installed = resolvePackageGraphRefs([...availableById.values()], [root]);
  } catch {
    return [UNRESOLVABLE_PACKAGE_GRAPH];
  }
  return botCompileLanguageGaps(project, bot, installed);
}

/** Sentinel gap for a Package whose dependency graph cannot be resolved at all. */
export const UNRESOLVABLE_PACKAGE_GRAPH = "(unresolvable package graph)";

/** Missing Language/Matcher Profile pairs that make this Bot non-compilable in Studio. */
export function botMissingMatcherLanguages(
  workspace: StudioWorkspace,
  project = selectedProject(workspace),
  bot = selectedBot(workspace, project),
): string[] {
  let closure: StudioPackage[];
  try {
    closure = botPackageClosureRefs(workspace, project, bot).map((entry) => entry.package);
  } catch {
    return [UNRESOLVABLE_PACKAGE_GRAPH];
  }
  return botCompileLanguageGaps(project, bot, closure);
}

/** Missing Language/Matcher Profile pairs for the currently selected Package and its preview dependency graph. */
export function selectedPackageMissingMatcherLanguages(workspace: StudioWorkspace): string[] {
  const preview = resolvePackagePreview(workspace);
  const profiles = workspace.selectedPackageScope === "shared"
    ? workspace.shared_matcher_profiles
    : selectedProject(workspace).matcher_profiles;
  return missingPackageMatcherLanguages(preview.packages, profiles);
}

/** Applies a Bot materialized view only to its structural Bot Package/settings. Attached Packages remain read-only. */
export function applyResolvedBrain(workspace: StudioWorkspace, before: StudioBrainWorkspace, after: StudioBrainWorkspace): StudioWorkspace {
  const next = cloneStudioWorkspace(workspace); const project = selectedProject(next); const bot = selectedBot(next, project);
  if (after.project_id !== before.project_id || after.brain_id !== before.brain_id) throw new Error("Project/Bot identity is managed by the Projects view.");
  if (JSON.stringify(after.languages) !== JSON.stringify(before.languages)) throw new Error("Project languages are managed by the Projects view.");
  if (JSON.stringify(after.enabled_languages) !== JSON.stringify(before.enabled_languages)) throw new Error("Enabled languages are managed by Bot create/edit.");
  if (after.default_language !== before.default_language) throw new Error("Bot default language is managed by Bot create/edit.");
  if (after.authoring_language !== before.authoring_language) throw new Error("Package authoring language is managed by Package metadata or derived from the Bot default.");
  if (JSON.stringify(after.matcher_profiles) !== JSON.stringify(before.matcher_profiles)) throw new Error("Matcher Profiles are JSON source data managed outside Package editors.");
  bot.settings.emit_debug_map = after.emit_debug_map;
  bot.settings.semantic = diffConfig(workspace.settings.semantic, after.semantic);
  bot.settings.conversation = botConversationSettings(workspace.settings.conversation, after.conversation);

  const beforeById = new Map(before.packages.map((pkg) => [pkg.manifest.id, pkg]));
  const afterById = new Map(after.packages.map((pkg) => [pkg.manifest.id, pkg]));
  const ownedId = bot.package.manifest.id;
  for (const id of new Set([...beforeById.keys(), ...afterById.keys()])) {
    if (id !== ownedId && packageChanged(beforeById.get(id), afterById.get(id))) throw new Error(`Attached package ${id} is read-only in Bot scope. Use the Bot Package to override it.`);
  }
  const changed = afterById.get(ownedId);
  if (!changed) throw new Error("A Bot must always keep its owned Package.");
  bot.package = structuredClone(changed);
  next.selectedPackageId = after.selectedPackageId; next.selectedPackageScope = "bot";
  return touchStudioWorkspace(next);
}

function uniquePackageIdForProject(workspace: StudioWorkspace, project: StudioProject, preferred: string): string {
  const ids = [
    ...workspace.shared_packages.map((pkg) => pkg.manifest.id),
    ...project.packages.map((pkg) => pkg.manifest.id),
    ...project.bots.map((bot) => bot.package.manifest.id),
  ];
  return uniqueScopedId(ids, preferred);
}

function createBotRecord(workspace: StudioWorkspace, project: StudioProject, id: string, defaultLanguage: string, enabledLanguages: readonly string[]): StudioBot {
  const languages = projectAvailableLanguages(project);
  if (!languages.some((language) => languageKey(language) === languageKey(defaultLanguage))) throw new Error(`Bot default language ${defaultLanguage} is not selected by Project ${project.id}.`);
  const enabledKeys = new Set(enabledLanguages.map(languageKey));
  const enabled = languages.filter((language) => enabledKeys.has(languageKey(language)));
  if (enabled.length === 0 || enabled.length !== enabledKeys.size || !enabled.some((language) => languageKey(language) === languageKey(defaultLanguage))) throw new Error("Bot enabled languages must be a non-empty Project-language subset containing its default language.");
  const packageId = uniquePackageIdForProject(workspace, project, `${project.id}.${id}.bot`);
  return {
    id,
    title: humanTitle(id),
    description: "",
    default_language: defaultLanguage,
    enabled_languages: enabled,
    package_ids: [],
    fallback_package_id: null,
    package: createPackage(packageId, `Package owned by ${humanTitle(id)}`, "standard", defaultLanguage),
    settings: { emit_debug_map: true, semantic: {}, conversation: { author_numbers: [] } },
  };
}

export function addProject(workspace: StudioWorkspace, preferredId: string, languageIds: readonly string[], mainBotDefaultLanguage: string): StudioWorkspace {
  const next = cloneStudioWorkspace(workspace); const id = uniqueScopedId(next.projects.map((row) => row.id), preferredId);
  const requestedKeys = new Set(languageIds.map(languageKey));
  const languages = sharedAvailableLanguages(next).filter((language) => requestedKeys.has(languageKey(language)));
  if (languages.length === 0 || languages.length !== requestedKeys.size) throw new Error("Project languages must be a non-empty subset of Shared Matcher Profiles.");
  const project: StudioProject = {
    id,
    title: humanTitle(id),
    description: "",
    matcher_profiles: matcherProfilesForLanguages(languages, next.shared_matcher_profiles),
    packages: [],
    bots: [],
  };
  const bot = createBotRecord(next, project, "main-bot", mainBotDefaultLanguage, languages); project.bots.push(bot); next.projects.push(project);
  next.selectedProjectId = id; next.selectedBotId = bot.id; next.selectedPackageScope = "bot"; next.selectedPackageId = bot.package.manifest.id;
  return touchStudioWorkspace(next);
}

export function removeProject(workspace: StudioWorkspace, projectId: string): StudioWorkspace {
  const next = cloneStudioWorkspace(workspace); next.projects = next.projects.filter((row) => row.id !== projectId);
  if (next.selectedProjectId === projectId) {
    const project = next.projects[0] ?? null; const bot = project?.bots[0] ?? null;
    next.selectedProjectId = project?.id ?? ""; next.selectedBotId = bot?.id ?? "";
    next.selectedPackageScope = bot ? "bot" : "project"; next.selectedPackageId = bot?.package.manifest.id ?? project?.packages[0]?.manifest.id ?? "";
  }
  return touchStudioWorkspace(next);
}

/**
 * A new Bot enables the whole Project language catalog unless the caller narrows it, matching the
 * Bot that `addProject` creates. A Project's Packages are authored for the Project's languages, so
 * a narrower default would make them ineligible for every freshly created Bot.
 */
export function addBot(workspace: StudioWorkspace, preferredId: string, defaultLanguage: string, enabledLanguages: readonly string[] = projectAvailableLanguages(selectedProject(workspace))): StudioWorkspace {
  const next = cloneStudioWorkspace(workspace); const project = selectedProject(next); const id = uniqueScopedId(project.bots.map((row) => row.id), preferredId);
  const bot = createBotRecord(next, project, id, defaultLanguage, enabledLanguages); project.bots.push(bot); next.selectedBotId = id; next.selectedPackageScope = "bot"; next.selectedPackageId = bot.package.manifest.id;
  return touchStudioWorkspace(next);
}

export function removeBot(workspace: StudioWorkspace, botId: string): StudioWorkspace {
  const next = cloneStudioWorkspace(workspace); const project = selectedProject(next); project.bots = project.bots.filter((row) => row.id !== botId);
  if (next.selectedBotId === botId) {
    const bot = project.bots[0] ?? null; next.selectedBotId = bot?.id ?? ""; next.selectedPackageScope = bot ? "bot" : "project"; next.selectedPackageId = bot?.package.manifest.id ?? project.packages[0]?.manifest.id ?? "";
  }
  return touchStudioWorkspace(next);
}

export function addSharedPackage(workspace: StudioWorkspace, preferredId = "shared.package", authoringLanguage = sharedAvailableLanguages(workspace)[0] ?? "und"): StudioWorkspace {
  const next = cloneStudioWorkspace(workspace);
  const occupied = new Set([
    ...next.shared_packages.map((pkg) => pkg.manifest.id),
    ...next.projects.flatMap((project) => [...project.packages, ...project.bots.map((bot) => bot.package)].map((pkg) => pkg.manifest.id)),
  ]);
  if (occupied.has(preferredId)) throw new Error(`Package ID ${preferredId} already exists in a Shared/Project/Bot namespace.`);
  if (!sharedAvailableLanguages(workspace).some((language) => languageKey(language) === languageKey(authoringLanguage))) throw new Error("Shared Package authoring language must come from Shared Matcher Profiles.");
  const pkg = createPackage(preferredId, "Reusable shared package", "standard", authoringLanguage); next.shared_packages.push(pkg); next.selectedPackageScope = "shared"; next.selectedPackageId = preferredId;
  return touchStudioWorkspace(next);
}

export function addSharedFallbackPackage(workspace: StudioWorkspace, preferredId = "fallback.package", authoringLanguage = sharedAvailableLanguages(workspace)[0] ?? "und"): StudioWorkspace {
  const next = cloneStudioWorkspace(workspace);
  const occupied = new Set([
    ...next.shared_packages.map((pkg) => pkg.manifest.id),
    ...next.projects.flatMap((project) => [...project.packages, ...project.bots.map((bot) => bot.package)].map((pkg) => pkg.manifest.id)),
  ]);
  if (occupied.has(preferredId)) throw new Error(`Package ID ${preferredId} already exists in a Shared/Project/Bot namespace.`);
  if (!sharedAvailableLanguages(workspace).some((language) => languageKey(language) === languageKey(authoringLanguage))) throw new Error("Shared Package authoring language must come from Shared Matcher Profiles.");
  const pkg = createPackage(preferredId, "Reusable fallback package", "fallback", authoringLanguage);
  next.shared_packages.push(pkg);
  next.selectedPackageScope = "shared";
  next.selectedPackageId = preferredId;
  return touchStudioWorkspace(next);
}

export function setBotFallbackPackage(workspace: StudioWorkspace, packageId: string | null): StudioWorkspace {
  const next = cloneStudioWorkspace(workspace);
  const project = selectedProject(next);
  const bot = selectedBot(next, project);
  if (packageId !== null) {
    if (!botSelectableFallbackPackages(next, project, bot).some((pkg) => pkg.manifest.id === packageId)) throw new Error(`Fallback Package ${packageId} is not available to this Bot.`);
  }
  bot.fallback_package_id = packageId;
  return touchStudioWorkspace(next);
}

export function addPackageToBot(workspace: StudioWorkspace, packageId: string): StudioWorkspace {
  const next = cloneStudioWorkspace(workspace); const project = selectedProject(next); const bot = selectedBot(next, project);
  if (!botSelectablePackages(next, project, bot).some((pkg) => pkg.manifest.id === packageId)) throw new Error(`Package ${packageId} is not available to this Bot.`);
  if (!bot.package_ids.includes(packageId)) bot.package_ids.push(packageId);
  return touchStudioWorkspace(next);
}

export function removePackageFromBot(workspace: StudioWorkspace, packageId: string): StudioWorkspace {
  const next = cloneStudioWorkspace(workspace); const project = selectedProject(next); const bot = selectedBot(next, project);
  if (!bot.package_ids.includes(packageId)) return workspace;
  bot.package_ids = bot.package_ids.filter((id) => id !== packageId);
  pruneBotOverridesToVisiblePackages(next, project, bot);
  return touchStudioWorkspace(next);
}

export function setBotPackages(workspace: StudioWorkspace, packageIds: string[]): StudioWorkspace {
  const next = cloneStudioWorkspace(workspace); const project = selectedProject(next); const bot = selectedBot(next, project);
  const requested = new Set(packageIds);
  if (requested.size !== packageIds.length) throw new Error("Bot Package selection contains duplicate IDs.");
  const selectableIds = new Set(botSelectablePackages(next, project, bot).map((pkg) => pkg.manifest.id));
  for (const id of requested) if (!selectableIds.has(id)) throw new Error(`Package ${id} is not available to this Bot.`);
  const available = botAttachablePackages(next, project);
  bot.package_ids = available.filter((pkg) => requested.has(pkg.manifest.id)).map((pkg) => pkg.manifest.id);
  pruneBotOverridesToVisiblePackages(next, project, bot);
  return touchStudioWorkspace(next);
}

function visibleOverrideSources(workspace: StudioWorkspace, project: StudioProject, baseId: string): Array<OverrideableContribution & { row: ContributionLike }> {
  const all = botAttachablePackages(workspace, project);
  const byId = new Map(all.map((pkg) => [pkg.manifest.id, pkg]));
  const base = byId.get(baseId);
  if (!base) throw new Error(`Package ${baseId} is unavailable for Bot override.`);
  if (base.manifest.kind !== "standard") throw new Error("Fallback Packages cannot be overridden.");

  const ordered: StudioPackage[] = [];
  const seen = new Set<string>();
  const visit = (pkg: StudioPackage) => {
    if (seen.has(pkg.manifest.id)) return;
    seen.add(pkg.manifest.id);
    const dependencies = [...pkg.manifest.dependencies].sort((left, right) => compareUtf8(left.id, right.id));
    for (const dependency of dependencies) {
      if (!dependency.reexport) continue;
      const child = byId.get(dependency.id);
      if (!child) throw new Error(`Package ${pkg.manifest.id} reexports unavailable package ${dependency.id}.`);
      visit(child);
    }
    ordered.push(pkg);
  };
  visit(base);

  const effective = new Map<string, OverrideableContribution & { row: ContributionLike }>();
  for (const pkg of ordered) {
    for (const namespace of Object.keys(pkg.contents) as PackageNamespace[]) {
      for (const row of contributionRows(pkg, namespace)) {
        // Source validation/compiler composition has already established that Add/Replace is valid.
        // Whichever contribution this visible package authors becomes the effective owner seen by
        // its dependent; this also covers a package replacing a non-reexported private dependency.
        effective.set(`${namespace}:${row.id}`, { namespace, id: row.id, source_package: pkg.manifest.id, row });
      }
    }
  }
  return [...effective.values()].filter((entry) => entry.row.exported);
}

function copyContributionIntoBotOverride(workspace: StudioWorkspace, baseId: string, namespace: PackageNamespace, contributionId: string): StudioWorkspace {
  const next = cloneStudioWorkspace(workspace); const project = selectedProject(next); const bot = selectedBot(next, project);
  if (!bot.package_ids.includes(baseId)) throw new Error(`Bot ${bot.id} must add ${baseId} before overriding it.`);
  const source = visibleOverrideSources(next, project, baseId).find((row) => row.namespace === namespace && row.id === contributionId);
  if (!source) throw new Error(`Exported contribution ${contributionId} is not visible through ${baseId}.`);
  const targetRows = contributionRows(bot.package, namespace);
  if (targetRows.some((row) => row.id === contributionId)) return workspace;
  targetRows.push({ id: source.row.id, exported: source.row.exported, mode: { type: "replace", target_package: source.source_package, target_id: source.row.id }, value: structuredClone(source.row.value) });
  ensureDependency(bot.package, baseId);
  next.selectedPackageScope = "bot"; next.selectedPackageId = bot.package.manifest.id; return touchStudioWorkspace(next);
}

export function overrideableContributions(workspace: StudioWorkspace, baseId: string): OverrideableContribution[] {
  const project = selectedProject(workspace); const bot = selectedBot(workspace, project);
  if (!bot.package_ids.includes(baseId)) throw new Error(`Bot must add Package ${baseId} before overriding it.`);
  const localIds = new Set<string>();
  for (const namespace of Object.keys(bot.package.contents) as PackageNamespace[]) for (const row of contributionRows(bot.package, namespace)) localIds.add(`${namespace}:${row.id}`);
  return visibleOverrideSources(workspace, project, baseId)
    .filter((row) => !localIds.has(`${row.namespace}:${row.id}`))
    .map(({ row: _row, ...entry }) => entry)
    .sort((a, b) => compareUtf8(`${a.namespace}:${a.id}`, `${b.namespace}:${b.id}`));
}

export function overrideContribution(workspace: StudioWorkspace, baseId: string, namespace: PackageNamespace, contributionId: string): StudioWorkspace {
  return copyContributionIntoBotOverride(workspace, baseId, namespace, contributionId);
}

export function effectiveBotSettings(workspace: StudioWorkspace, bot = selectedBot(workspace)): { emit_debug_map: boolean; semantic: SemanticConfig; conversation: ConversationConfig } {
  return { emit_debug_map: bot.settings.emit_debug_map, semantic: mergeConfig(workspace.settings.semantic, bot.settings.semantic), conversation: effectiveBotConversation(workspace.settings.conversation, bot.settings.conversation) };
}

function dependentPackageIds(packages: StudioPackage[], seedIds: Iterable<string>): Set<string> {
  const removed = new Set(seedIds); let changed = true;
  while (changed) {
    changed = false;
    for (const pkg of packages) {
      if (removed.has(pkg.manifest.id)) continue;
      if (pkg.manifest.dependencies.some((dep) => removed.has(dep.id))) { removed.add(pkg.manifest.id); changed = true; }
    }
  }
  return removed;
}

/** Removes one live Shared source and any Project-owned packages that depend on it. */
export function removeSharedPackage(workspace: StudioWorkspace, packageId: string): StudioWorkspace {
  const next = cloneStudioWorkspace(workspace);
  if (!next.shared_packages.some((pkg) => pkg.manifest.id === packageId)) throw new Error(`Shared package ${packageId} does not exist.`);
  const sharedDependents = dependentPackageIds(next.shared_packages, [packageId]);
  if (sharedDependents.size > 1) {
    const dependents = [...sharedDependents].filter((id) => id !== packageId).sort(compareUtf8);
    throw new Error(`Shared package ${packageId} is required by ${dependents.join(", ")}. Remove the dependent Shared Packages first.`);
  }
  next.shared_packages = next.shared_packages.filter((pkg) => pkg.manifest.id !== packageId);
  for (const project of next.projects) {
    const removedProjectIds = dependentPackageIds(project.packages, [packageId]);
    project.packages = project.packages.filter((pkg) => !removedProjectIds.has(pkg.manifest.id));
    for (const bot of project.bots) {
      bot.package_ids = bot.package_ids.filter((id) => id !== packageId && !removedProjectIds.has(id));
      if (bot.fallback_package_id === packageId || (bot.fallback_package_id !== null && removedProjectIds.has(bot.fallback_package_id))) bot.fallback_package_id = null;
      pruneBotOverridesToVisiblePackages(next, project, bot);
    }
    if (next.selectedPackageScope === "project" && next.selectedProjectId === project.id && removedProjectIds.has(next.selectedPackageId)) {
      const nextProjectPackage = project.packages[0] ?? null;
      const nextBot = selectedBotMaybe(project);
      next.selectedPackageScope = nextProjectPackage ? "project" : nextBot ? "bot" : "project";
      next.selectedPackageId = nextProjectPackage?.manifest.id ?? nextBot?.package.manifest.id ?? "";
    }
  }
  if (next.selectedPackageScope === "shared" && next.selectedPackageId === packageId) {
    const project = next.projects.find((row) => row.id === next.selectedProjectId) ?? next.projects[0]; const bot = project?.bots.find((row) => row.id === next.selectedBotId) ?? project?.bots[0];
    next.selectedPackageScope = bot ? "bot" : "shared"; next.selectedPackageId = bot?.package.manifest.id ?? next.shared_packages[0]?.manifest.id ?? "";
  }
  return touchStudioWorkspace(next);
}

export function addProjectPackage(workspace: StudioWorkspace, preferredId = "project.package", kind: "standard" | "fallback" = "standard", authoringLanguage?: string): StudioWorkspace {
  const next = cloneStudioWorkspace(workspace); const project = selectedProject(next);
  const languages = projectAvailableLanguages(project);
  const selectedAuthoringLanguage = authoringLanguage ?? languages[0] ?? "und";
  if (!languages.some((language) => languageKey(language) === languageKey(selectedAuthoringLanguage))) throw new Error("Project Package authoring language must come from the Project language selection.");
  const occupied = new Set([
    ...next.shared_packages.map((pkg) => pkg.manifest.id),
    ...project.packages.map((pkg) => pkg.manifest.id),
    ...project.bots.map((bot) => bot.package.manifest.id),
  ]);
  if (occupied.has(preferredId)) throw new Error(`Package ID ${preferredId} already exists in this Project namespace.`);
  const description = kind === "fallback" ? "Project-local fallback package" : "Project-local package";
  const pkg = createPackage(preferredId, description, kind, selectedAuthoringLanguage); project.packages.push(pkg); next.selectedPackageScope = "project"; next.selectedPackageId = preferredId; return touchStudioWorkspace(next);
}

export function projectPackageRemovalImpact(workspace: StudioWorkspace, packageId: string): ProjectPackageRemovalImpact {
  const project = selectedProject(workspace);
  if (!project.packages.some((pkg) => pkg.manifest.id === packageId)) throw new Error(`Project Package ${packageId} does not exist.`);
  const removedIds = dependentPackageIds(project.packages, [packageId]);
  const botIds = project.bots.filter((bot) => bot.package_ids.some((id) => removedIds.has(id)) || (bot.fallback_package_id !== null && removedIds.has(bot.fallback_package_id))).map((bot) => bot.id);
  return { package_ids: [...removedIds], bot_ids: botIds };
}

export function removeProjectPackage(workspace: StudioWorkspace, packageId: string): StudioWorkspace {
  const next = cloneStudioWorkspace(workspace); const project = selectedProject(next);
  if (!project.packages.some((pkg) => pkg.manifest.id === packageId)) throw new Error(`Project Package ${packageId} does not exist.`);
  const removedIds = dependentPackageIds(project.packages, [packageId]);
  project.packages = project.packages.filter((pkg) => !removedIds.has(pkg.manifest.id));
  for (const bot of project.bots) {
    bot.package_ids = bot.package_ids.filter((id) => !removedIds.has(id));
    if (bot.fallback_package_id !== null && removedIds.has(bot.fallback_package_id)) bot.fallback_package_id = null;
    pruneBotOverridesToVisiblePackages(next, project, bot);
  }
  if (removedIds.has(next.selectedPackageId)) {
    const nextProjectPackage = project.packages[0] ?? null;
    const nextBot = selectedBotMaybe(project);
    next.selectedPackageScope = nextProjectPackage ? "project" : nextBot ? "bot" : "project";
    next.selectedPackageId = nextProjectPackage?.manifest.id ?? nextBot?.package.manifest.id ?? "";
  }
  return touchStudioWorkspace(next);
}

export function resolveEditingBrain(workspace: StudioWorkspace): StudioBrainWorkspace {
  if (workspace.selectedPackageScope === "bot") return resolveSelectedBrain(workspace);
  if (workspace.selectedPackageScope === "project") {
    const project = selectedProject(workspace);
    const selected = project.packages.find((pkg) => pkg.manifest.id === workspace.selectedPackageId) ?? project.packages[0];
    if (!selected) throw new Error(`Project ${project.id} has no package to edit.`);
    const packages = selected.manifest.kind === "fallback"
      ? [structuredClone(selected)]
      : resolvePackageGraph(botAttachablePackages(workspace, project), [selected]);
    const languages = projectAvailableLanguages(project);
    return { format: "gvya.studio.brain-view", version: 1, project_id: project.id, brain_id: "project-package-editor", languages, enabled_languages: structuredClone(languages), default_language: languages[0] ?? "und", authoring_language: selected.authoring_language, emit_debug_map: false, semantic: structuredClone(workspace.settings.semantic), conversation: previewConversation(workspace.settings.conversation), matcher_profiles: matcherProfilesForLanguages(languages, project.matcher_profiles), packages, selectedPackageId: selected.manifest.id, updatedSerial: workspace.updatedSerial };
  }
  const pkg = sharedPackageById(workspace, workspace.selectedPackageId) ?? workspace.shared_packages[0]; if (!pkg) throw new Error("Shared Package library is empty.");
  const languages = sharedAvailableLanguages(workspace);
  return { format: "gvya.studio.brain-view", version: 1, project_id: "shared-library", brain_id: `package:${pkg.manifest.id}`, languages, enabled_languages: structuredClone(languages), default_language: languages[0] ?? "und", authoring_language: pkg.authoring_language, emit_debug_map: false, semantic: structuredClone(workspace.settings.semantic), conversation: previewConversation(workspace.settings.conversation), matcher_profiles: matcherProfilesForLanguages(languages, workspace.shared_matcher_profiles), packages: [structuredClone(pkg)], selectedPackageId: pkg.manifest.id, updatedSerial: workspace.updatedSerial };
}

export function applyEditingBrain(workspace: StudioWorkspace, before: StudioBrainWorkspace, after: StudioBrainWorkspace): StudioWorkspace {
  if (JSON.stringify(after.languages) !== JSON.stringify(before.languages)) throw new Error("Languages are derived from Matcher Profiles, not Package editors.");
  if (JSON.stringify(after.enabled_languages) !== JSON.stringify(before.enabled_languages)) throw new Error("Enabled languages are managed by Bot create/edit.");
  if (after.default_language !== before.default_language) throw new Error("Default language is managed by Bot create/edit, not Package editors.");
  if (after.authoring_language !== before.authoring_language) throw new Error("Authoring language is managed by Package metadata, not content editors.");
  if (JSON.stringify(after.matcher_profiles) !== JSON.stringify(before.matcher_profiles)) throw new Error("Matcher Profiles are JSON source data managed outside Package editors.");
  if (workspace.selectedPackageScope === "bot") return applyResolvedBrain(workspace, before, after);
  if (workspace.selectedPackageScope === "project") {
    const next = cloneStudioWorkspace(workspace); const project = selectedProject(next); if (after.project_id !== project.id) throw new Error("Project identity is managed by the Projects view.");
    const editableIds = new Set(before.packages.map((pkg) => pkg.manifest.id));
    const afterById = new Map(after.packages.map((pkg) => [pkg.manifest.id, pkg]));
    project.packages = project.packages.map((pkg) => { if (!editableIds.has(pkg.manifest.id)) return pkg; const changed = afterById.get(pkg.manifest.id); if (!changed) throw new Error(`Project Package ${pkg.manifest.id} cannot be removed through its editor.`); return structuredClone(changed); });
    next.selectedPackageScope = "project"; next.selectedPackageId = after.selectedPackageId; return touchStudioWorkspace(next);
  }
  const next = cloneStudioWorkspace(workspace); const original = before.packages[0]; const changed = after.packages[0]; if (!original || !changed) throw new Error("Shared Package editor requires one package.");
  if (changed.manifest.id !== original.manifest.id) throw new Error("Shared Package identity changes require an explicit package operation.");
  const index = next.shared_packages.findIndex((pkg) => pkg.manifest.id === original.manifest.id); if (index < 0) throw new Error("Shared Package no longer exists.");
  next.shared_packages[index] = structuredClone(changed); next.selectedPackageId = changed.manifest.id; return touchStudioWorkspace(next);
}

export function recordPackageMetadataEdit(workspace: StudioWorkspace, scope: "shared" | "project" | "bot", packageId: string, description: string, authoringLanguage?: string): StudioWorkspace {
  const next = cloneStudioWorkspace(workspace); const pkg = ownedPackage(next, scope, packageId); if (!pkg) throw new Error(`Package ${packageId} is not owned by ${scope} scope.`);
  if (scope === "bot" && authoringLanguage !== undefined) throw new Error("Bot Package authoring language is derived from the Bot default language.");
  if (authoringLanguage !== undefined) {
    const project = scope === "project" ? selectedProject(next) : null;
    const allowed = scope === "shared" ? sharedAvailableLanguages(next) : project ? projectAvailableLanguages(project) : [];
    if (!allowed.some((language) => languageKey(language) === languageKey(authoringLanguage))) throw new Error("Package authoring language must have an owning-scope Matcher Profile.");
  }
  const authoringChanged = authoringLanguage !== undefined && languageKey(pkg.authoring_language) !== languageKey(authoringLanguage);
  if (pkg.manifest.description === description && !authoringChanged) return workspace;
  pkg.manifest.description = description;
  if (authoringLanguage !== undefined) pkg.authoring_language = authoringLanguage;
  return touchStudioWorkspace(next);
}

function ownedPackage(workspace: StudioWorkspace, scope: "shared" | "project" | "bot", packageId: string): StudioPackage | null {
  if (scope === "shared") return workspace.shared_packages.find((pkg) => pkg.manifest.id === packageId) ?? null;
  const project = selectedProject(workspace);
  if (scope === "project") return project.packages.find((pkg) => pkg.manifest.id === packageId) ?? null;
  const bot = selectedBot(workspace, project); return bot.package.manifest.id === packageId ? bot.package : null;
}

function contributionRows(pkg: StudioPackage, namespace: PackageNamespace): ContributionLike[] { return pkg.contents[namespace] as unknown as ContributionLike[]; }
function ensureDependency(pkg: StudioPackage, id: string): void { if (!pkg.manifest.dependencies.some((dep) => dep.id === id)) pkg.manifest.dependencies.push({ id, reexport: false }); }
function compilerVisiblePackageIds(workspace: StudioWorkspace, project: StudioProject, bot: StudioBot): Set<string> {
  const byId = new Map(botAttachablePackages(workspace, project).map((pkg) => [pkg.manifest.id, pkg]));
  const visible = new Set<string>();
  const visit = (id: string, stack: string[]) => {
    if (visible.has(id)) return;
    if (stack.includes(id)) throw new Error(`Package re-export cycle: ${[...stack, id].join(" -> ")}`);
    const pkg = byId.get(id);
    if (!pkg) throw new Error(`Bot ${bot.id} references unavailable Package ${id}.`);
    visible.add(id);
    for (const dependency of [...pkg.manifest.dependencies].sort((left, right) => compareUtf8(left.id, right.id))) {
      if (dependency.reexport) visit(dependency.id, [...stack, id]);
    }
  };
  for (const id of bot.package_ids) visit(id, []);
  return visible;
}

function pruneBotOverridesToVisiblePackages(workspace: StudioWorkspace, project: StudioProject, bot: StudioBot): void {
  const visible = compilerVisiblePackageIds(workspace, project, bot);
  const direct = new Set(bot.package_ids);
  for (const rows of Object.values(bot.package.contents) as unknown as ContributionLike[][]) {
    const filtered = rows.filter((row) => row.mode === "add" || visible.has(row.mode.target_package));
    if (filtered.length !== rows.length) {
      rows.splice(0, rows.length, ...filtered);
    }
  }
  const dependencies = bot.package.manifest.dependencies.filter((dependency) => direct.has(dependency.id));
  if (dependencies.length !== bot.package.manifest.dependencies.length) {
    bot.package.manifest.dependencies = dependencies;
  }
}

function selectedBotMaybe(project: StudioProject): StudioBot | null { return project.bots[0] ?? null; }
function packageChanged(left: StudioPackage | undefined, right: StudioPackage | undefined): boolean { return JSON.stringify(left && { manifest: left.manifest, contents: left.contents }) !== JSON.stringify(right && { manifest: right.manifest, contents: right.contents }); }
function resolvePackageGraphRefs(availablePackages: StudioPackage[], roots: StudioPackage[]): StudioPackage[] {
  const available = dedupePackages(availablePackages);
  const byId = new Map(available.map((pkg) => [pkg.manifest.id, pkg]));
  const included = new Map<string, StudioPackage>();
  const visit = (pkg: StudioPackage, stack: string[]) => {
    if (included.has(pkg.manifest.id)) return;
    if (stack.includes(pkg.manifest.id)) throw new Error(`Package dependency cycle: ${[...stack, pkg.manifest.id].join(" -> ")}`);
    for (const dep of pkg.manifest.dependencies) {
      const dependency = byId.get(dep.id);
      if (!dependency) throw new Error(`Package ${pkg.manifest.id} requires unavailable package ${dep.id}.`);
      visit(dependency, [...stack, pkg.manifest.id]);
    }
    included.set(pkg.manifest.id, pkg);
  };
  for (const root of roots) visit(root, []);
  return [...included.values()];
}
function resolvePackageGraph(availablePackages: StudioPackage[], roots: StudioPackage[]): StudioPackage[] {
  return resolvePackageGraphRefs(availablePackages, roots).map((pkg) => structuredClone(pkg));
}
function dedupePackages(packages: StudioPackage[]): StudioPackage[] { const out = new Map<string, StudioPackage>(); for (const pkg of packages) { if (out.has(pkg.manifest.id)) throw new Error(`Package ID ${pkg.manifest.id} is present more than once in the same Project/Bot graph.`); out.set(pkg.manifest.id, pkg); } return [...out.values()]; }
function mergeConfig<T extends object>(base: T, override: Partial<T>): T { return { ...structuredClone(base), ...structuredClone(override) } as T; }
function diffConfig<T extends object>(base: T, resolved: T): Partial<T> { const out: Partial<T> = {}; for (const key of Object.keys(resolved) as Array<keyof T>) if (JSON.stringify(resolved[key]) !== JSON.stringify(base[key])) out[key] = resolved[key]; return out; }
function conversationDefaults(conversation: ConversationConfig): StudioConversationDefaults { const defaults: Partial<ConversationConfig> = structuredClone(conversation); delete defaults.author_numbers; return defaults as StudioConversationDefaults; }
function previewConversation(defaults: StudioConversationDefaults): ConversationConfig { return { ...structuredClone(defaults), author_numbers: [] }; }
function effectiveBotConversation(defaults: StudioConversationDefaults, settings: StudioBotConversationSettings): ConversationConfig { return { ...structuredClone(defaults), ...structuredClone(settings) }; }
function botConversationSettings(defaults: StudioConversationDefaults, resolved: ConversationConfig): StudioBotConversationSettings { const scalars: Partial<ConversationConfig> = structuredClone(resolved); const authorNumbers = scalars.author_numbers ?? []; delete scalars.author_numbers; return { ...diffConfig(defaults, scalars as StudioConversationDefaults), author_numbers: authorNumbers }; }
function uniqueScopedId(existing: Iterable<string>, preferred: string): string { const used = new Set(existing); if (!used.has(preferred)) return preferred; let n = 2; while (used.has(`${preferred}.${n}`)) n += 1; return `${preferred}.${n}`; }
function humanTitle(value: string): string { return value.split(/[._-]+/u).filter(Boolean).map((part) => part[0]?.toUpperCase() + part.slice(1)).join(" ") || "Untitled"; }

