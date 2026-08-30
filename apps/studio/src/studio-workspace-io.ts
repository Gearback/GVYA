import type { ConversationConfig, JsonObject, JsonValue, MatcherProfile, SemanticConfig, StudioBot, StudioBotConversationSettings, StudioConversationDefaults, StudioPackage, StudioProject, StudioWorkspace } from "./types.js";
import { languageKey } from "./languages.js";
import { assertUniqueMatcherProfiles, parseProfileCatalogDocument, profileCatalogDocument } from "./matcher-profiles.js";
import { parsePackageSnapshot } from "./source-io.js";
import { cloneStudioWorkspace, resolveSelectedBrain } from "./studio-model.js";
import { packageSnapshotDocument, stableJson } from "./workspace.js";

const MAX_BYTES = 32 * 1024 * 1024;
const MAX_PACKAGES = 4096;
const MAX_PROJECTS = 512;
const MAX_BOTS = 2048;

export function studioWorkspaceToText(workspace: StudioWorkspace): string {
  return stableJson(studioWorkspaceToJson(workspace) as JsonValue);
}

export function studioWorkspaceToJson(workspace: StudioWorkspace): JsonObject {
  return {
    format: "gvya.studio.workspace",
    version: 1,
    shared_matcher_profiles: workspace.shared_matcher_profiles.map(profileCatalogDocument),
    shared_packages: workspace.shared_packages.map((pkg) => persistPackage(pkg)),
    settings: structuredClone(workspace.settings) as unknown as JsonValue,
    projects: workspace.projects.map((project) => ({
      id: project.id,
      title: project.title,
      description: project.description,
      matcher_profiles: project.matcher_profiles.map(profileCatalogDocument),
      packages: project.packages.map((pkg) => persistPackage(pkg)),
      bots: project.bots.map((bot) => ({
        id: bot.id,
        title: bot.title,
        description: bot.description,
        default_language: bot.default_language,
        enabled_languages: [...bot.enabled_languages],
        package_ids: [...bot.package_ids],
        fallback_package_id: bot.fallback_package_id,
        package: persistPackage(bot.package, false),
        settings: structuredClone(bot.settings) as unknown as JsonValue,
      })),
    })),
    selectedProjectId: workspace.selectedProjectId,
    selectedBotId: workspace.selectedBotId,
    selectedPackageScope: workspace.selectedPackageScope,
    selectedPackageId: workspace.selectedPackageId,
    updatedSerial: workspace.updatedSerial,
  };
}

function persistPackage(pkg: StudioPackage, includeAuthoringLanguage = true): JsonObject {
  const source = packageSnapshotDocument(pkg);
  const persisted: JsonObject = {
    path: pkg.path,
    manifest: source.manifest as JsonValue,
    contents: source.contents as JsonValue,
  };
  if (includeAuthoringLanguage) persisted.authoring_language = pkg.authoring_language;
  return persisted;
}

export function studioWorkspaceFromText(text: string): StudioWorkspace {
  if (new TextEncoder().encode(text).byteLength > MAX_BYTES) throw new Error("GVYA Studio workspace exceeds the 32 MiB persistence limit.");
  return studioWorkspaceFromJson(JSON.parse(text) as unknown);
}

export function studioWorkspaceFromJson(value: unknown): StudioWorkspace {
  const root = object(value, "workspace");
  exactKeys(root, ["format", "version", "shared_matcher_profiles", "shared_packages", "settings", "projects", "selectedProjectId", "selectedBotId", "selectedPackageScope", "selectedPackageId", "updatedSerial"], "workspace");
  if (root.format !== "gvya.studio.workspace" || root.version !== 1) throw new Error("Unsupported GVYA Studio workspace. Version 1 is required; no migration or compatibility reader exists.");
  const sharedMatcherProfiles = array(root.shared_matcher_profiles, "workspace.shared_matcher_profiles").map((row, i) => parseProfileCatalogDocument(row, `shared_matcher_profiles[${i}]`));
  assertUniqueMatcherProfiles(sharedMatcherProfiles, "Shared Matcher Profiles");
  const shared = array(root.shared_packages, "workspace.shared_packages").map((row, i) => parseStudioPackage(row, `shared_packages[${i}]`));
  if (shared.length > MAX_PACKAGES) throw new Error("Shared package library exceeds the supported package count.");
  assertUniquePackageIds(shared, "shared package library");
  const settingsRow = object(root.settings, "workspace.settings"); exactKeys(settingsRow, ["semantic", "conversation"], "workspace.settings");
  const settings = { semantic: parseSemantic(settingsRow.semantic, "workspace.settings.semantic"), conversation: parseConversationDefaults(settingsRow.conversation, "workspace.settings.conversation") };
  const projects = array(root.projects, "workspace.projects").map((row, i) => parseProject(row, `projects[${i}]`));
  if (projects.length > MAX_PROJECTS) throw new Error("Studio workspace Project count is outside the supported range."); uniqueStrings(projects.map((row) => row.id), "Project IDs");
  for (const projectRow of projects) {
    assertUniqueMatcherProfiles(projectRow.matcher_profiles, `Project ${projectRow.id} Matcher Profiles`);
    for (const bot of projectRow.bots) {
      const enabledKeys = new Set(bot.enabled_languages.map(languageKey));
      if (enabledKeys.size === 0) throw new Error(`Bot ${bot.id} enabled_languages must be non-empty.`);
      if (!enabledKeys.has(languageKey(bot.default_language))) throw new Error(`Bot ${bot.id} default_language is not enabled.`);
      if (languageKey(bot.package.authoring_language) !== languageKey(bot.default_language)) throw new Error(`Bot ${bot.id} Package authoring language must derive from its default language.`);
    }
  }
  const selectedProjectId = plainString(root.selectedProjectId, "workspace.selectedProjectId"); const project = projects.find((row) => row.id === selectedProjectId) ?? null;
  if (projects.length === 0) { if (selectedProjectId !== "") throw new Error("selectedProjectId must be empty when the workspace has no Projects."); } else if (!project) throw new Error("selectedProjectId does not name a Project.");
  const selectedBotId = plainString(root.selectedBotId, "workspace.selectedBotId");
  if (!project) { if (selectedBotId !== "") throw new Error("selectedBotId must be empty when there is no selected Project."); }
  else if (project.bots.length === 0) { if (selectedBotId !== "") throw new Error("selectedBotId must be empty when the selected Project has no Bots."); }
  else if (!project.bots.some((row) => row.id === selectedBotId)) throw new Error("selectedBotId does not name a Bot in the selected Project.");
  const selectedPackageScope = root.selectedPackageScope; if (!( ["shared", "project", "bot"] as unknown[]).includes(selectedPackageScope)) throw new Error("selectedPackageScope is invalid.");
  const selectedPackageId = typeof root.selectedPackageId === "string" ? root.selectedPackageId : "";
  const updatedSerial = integer(root.updatedSerial, 0, Number.MAX_SAFE_INTEGER, "workspace.updatedSerial");
  const candidate = cloneStudioWorkspace({ format: "gvya.studio.workspace", version: 1, shared_matcher_profiles: sharedMatcherProfiles, shared_packages: shared, settings, projects, selectedProjectId, selectedBotId, selectedPackageScope: selectedPackageScope as StudioWorkspace["selectedPackageScope"], selectedPackageId, updatedSerial });
  for (const projectRow of projects) {
    const graphIds = [...shared, ...projectRow.packages, ...projectRow.bots.map((bot) => bot.package)].map((pkg) => pkg.manifest.id);
    uniqueStrings(graphIds, `Project ${projectRow.id} Shared/Project/Bot Package IDs`);
    for (const botRow of projectRow.bots) { const probe = cloneStudioWorkspace(candidate); probe.selectedProjectId = projectRow.id; probe.selectedBotId = botRow.id; resolveSelectedBrain(probe); }
  }
  return candidate;
}

function parseProject(value: unknown, path: string): StudioProject {
  const row = object(value, path); exactKeys(row, ["id", "title", "description", "matcher_profiles", "packages", "bots"], path);
  const packages = array(row.packages, `${path}.packages`).map((pkg, i) => parseStudioPackage(pkg, `${path}.packages[${i}]`)); assertUniquePackageIds(packages, `${path}.packages`);
  const bots = array(row.bots, `${path}.bots`).map((bot, i) => parseBot(bot, `${path}.bots[${i}]`)); if (bots.length > MAX_BOTS) throw new Error(`${path}.bots count is outside the supported range.`); uniqueStrings(bots.map((row) => row.id), `${path} Bot IDs`);
  uniqueStrings([...packages, ...bots.map((bot) => bot.package)].map((pkg) => pkg.manifest.id), `${path} owned Package IDs`);
  const matcher_profiles: MatcherProfile[] = array(row.matcher_profiles, `${path}.matcher_profiles`).map((profile, index) => parseProfileCatalogDocument(profile, `${path}.matcher_profiles[${index}]`));
  return { id: string(row.id, `${path}.id`), title: plainString(row.title, `${path}.title`), description: plainString(row.description, `${path}.description`), matcher_profiles, packages, bots };
}
function parseBot(value: unknown, path: string): StudioBot {
  const row = object(value, path); exactKeys(row, ["id", "title", "description", "default_language", "enabled_languages", "package_ids", "fallback_package_id", "package", "settings"], path);
  const defaultLanguage = string(row.default_language, `${path}.default_language`);
  const packageRow = parseStudioPackage(row.package, `${path}.package`, defaultLanguage);
  if (packageRow.manifest.kind !== "standard") throw new Error(`${path}.package must be a Standard Package.`);
  const fallbackPackageId = row.fallback_package_id === null ? null : string(row.fallback_package_id, `${path}.fallback_package_id`);
  const settings = object(row.settings, `${path}.settings`); exactKeys(settings, ["emit_debug_map", "semantic", "conversation"], `${path}.settings`);
  return { id: string(row.id, `${path}.id`), title: plainString(row.title, `${path}.title`), description: plainString(row.description, `${path}.description`), default_language: defaultLanguage, enabled_languages: languageIds(row.enabled_languages, `${path}.enabled_languages`), package_ids: ids(row.package_ids, `${path}.package_ids`), fallback_package_id: fallbackPackageId, package: packageRow, settings: { emit_debug_map: bool(settings.emit_debug_map, `${path}.settings.emit_debug_map`), semantic: parsePartialSemantic(settings.semantic, `${path}.settings.semantic`), conversation: parseBotConversation(settings.conversation, `${path}.settings.conversation`) } };
}
function parseStudioPackage(value: unknown, path: string, derivedAuthoringLanguage?: string): StudioPackage {
  const row = object(value, path); exactKeys(row, derivedAuthoringLanguage === undefined ? ["path", "authoring_language", "manifest", "contents"] : ["path", "manifest", "contents"], path); const packagePath = string(row.path, `${path}.path`);
  const authoringLanguage = derivedAuthoringLanguage ?? string(row.authoring_language, `${path}.authoring_language`);
  return parsePackageSnapshot(packagePath, { manifest: row.manifest, contents: row.contents }, authoringLanguage);
}
function parseSemantic(value: unknown, path: string): SemanticConfig { const row = object(value, path); return { candidate_limit: integer(row.candidate_limit, 2, 256, `${path}.candidate_limit`), resolution_threshold: number(row.resolution_threshold, 0, 1, `${path}.resolution_threshold`), ambiguity_margin: number(row.ambiguity_margin, 0, 1, `${path}.ambiguity_margin`), resolver_min_confidence: number(row.resolver_min_confidence, 0, 1, `${path}.resolver_min_confidence`), resolver_candidate_limit: integer(row.resolver_candidate_limit, 1, 64, `${path}.resolver_candidate_limit`) }; }
function parseConversationDefaults(value: unknown, path: string): StudioConversationDefaults { const row = object(value, path); exactKeys(row, ["default_topic_ttl","default_followup_ttl","recent_response_limit","recent_variant_limit","recent_user_window","repeat_detection_window","repeat_detection_threshold","max_messages_per_turn","repair_candidate_min_score","topic_preference_margin"], path); return { default_topic_ttl: integer(row.default_topic_ttl, 1, 0xffff_ffff, `${path}.default_topic_ttl`), default_followup_ttl: integer(row.default_followup_ttl, 1, 0xffff_ffff, `${path}.default_followup_ttl`), recent_response_limit: integer(row.recent_response_limit, 1, 64, `${path}.recent_response_limit`), recent_variant_limit: integer(row.recent_variant_limit, 1, 64, `${path}.recent_variant_limit`), recent_user_window: integer(row.recent_user_window, 1, 50, `${path}.recent_user_window`), repeat_detection_window: integer(row.repeat_detection_window, 1, 50, `${path}.repeat_detection_window`), repeat_detection_threshold: integer(row.repeat_detection_threshold, 2, 20, `${path}.repeat_detection_threshold`), max_messages_per_turn: integer(row.max_messages_per_turn, 1, 6, `${path}.max_messages_per_turn`), repair_candidate_min_score: number(row.repair_candidate_min_score, 0, 1, `${path}.repair_candidate_min_score`), topic_preference_margin: number(row.topic_preference_margin, 0, .25, `${path}.topic_preference_margin`) }; }
function parsePartialSemantic(value: unknown, path: string): Partial<SemanticConfig> { const row = object(value, path); const allowed = new Set(["candidate_limit","resolution_threshold","ambiguity_margin","resolver_min_confidence","resolver_candidate_limit"]); for (const key of Object.keys(row)) if (!allowed.has(key)) throw new Error(`${path} contains unsupported key ${key}.`); const out: Partial<SemanticConfig> = {}; if ("candidate_limit" in row) out.candidate_limit=integer(row.candidate_limit,2,256,`${path}.candidate_limit`); if("resolution_threshold" in row) out.resolution_threshold=number(row.resolution_threshold,0,1,`${path}.resolution_threshold`); if("ambiguity_margin" in row) out.ambiguity_margin=number(row.ambiguity_margin,0,1,`${path}.ambiguity_margin`); if("resolver_min_confidence" in row) out.resolver_min_confidence=number(row.resolver_min_confidence,0,1,`${path}.resolver_min_confidence`); if("resolver_candidate_limit" in row) out.resolver_candidate_limit=integer(row.resolver_candidate_limit,1,64,`${path}.resolver_candidate_limit`); return out; }
function parseBotConversation(value: unknown, path: string): StudioBotConversationSettings {
  const row = object(value, path);
  const allowed = new Set<keyof ConversationConfig>(["default_topic_ttl", "default_followup_ttl", "recent_response_limit", "recent_variant_limit", "recent_user_window", "repeat_detection_window", "repeat_detection_threshold", "max_messages_per_turn", "repair_candidate_min_score", "author_numbers", "topic_preference_margin"]);
  for (const key of Object.keys(row)) if (!allowed.has(key as keyof ConversationConfig)) throw new Error(`${path} contains unsupported key ${key}.`);
  const out: Partial<ConversationConfig> = {};
  const ints = ["default_topic_ttl","default_followup_ttl","recent_response_limit","recent_variant_limit","recent_user_window","repeat_detection_window","repeat_detection_threshold","max_messages_per_turn"] as const;
  for (const key of ints) if (key in row) { const min=key==="repeat_detection_threshold"?2:1; const max=key==="max_messages_per_turn"?6:key==="repeat_detection_threshold"?20:key.includes("window")?50:key.includes("limit")?64:0xffff_ffff; out[key]=integer(row[key],min,max,`${path}.${key}`); }
  if ("repair_candidate_min_score" in row) out.repair_candidate_min_score=number(row.repair_candidate_min_score,0,1,`${path}.repair_candidate_min_score`);
  if (!("author_numbers" in row)) throw new Error(`${path} is missing Bot-owned author_numbers.`);
  out.author_numbers=parseAuthorNumbers(row.author_numbers,`${path}.author_numbers`);
  if ("topic_preference_margin" in row) out.topic_preference_margin=number(row.topic_preference_margin,0,.25,`${path}.topic_preference_margin`);
  return out as StudioBotConversationSettings;
}

function parseAuthorNumbers(value: unknown, path: string): ConversationConfig["author_numbers"] { const rows=array(value,path); if(rows.length>256)throw new Error(`${path} exceeds 256 definitions.`); const out=rows.map((item,i)=>{const row=object(item,`${path}[${i}]`); exactKeys(row,["path","default","min","max"],`${path}[${i}]`); const p=string(row.path,`${path}[${i}].path`); if(p.length>512||p.split(".").length>16||p.split(".").some((x)=>!x))throw new Error(`${path}[${i}].path is invalid.`); const min=number(row.min,-Number.MAX_VALUE,Number.MAX_VALUE,`${path}[${i}].min`), max=number(row.max,-Number.MAX_VALUE,Number.MAX_VALUE,`${path}[${i}].max`), def=number(row.default,-Number.MAX_VALUE,Number.MAX_VALUE,`${path}[${i}].default`); if(min>def||def>max)throw new Error(`${path}[${i}] bounds are invalid.`); return {path:p,default:def,min,max};}); const paths=out.map(x=>x.path); uniqueStrings(paths,`${path} paths`); for(let i=0;i<paths.length;i++)for(let j=i+1;j<paths.length;j++)if(paths[i]!.startsWith(`${paths[j]}.`)||paths[j]!.startsWith(`${paths[i]}.`))throw new Error(`${path} paths cannot overlap as parent and child.`); return out; }

function ids(value: unknown, path: string): string[] { const out=array(value,path).map((item,i)=>string(item,`${path}[${i}]`)); uniqueStrings(out,`${path} package IDs`); return out; }
function languageIds(value: unknown, path: string): string[] { const out=array(value,path).map((item,i)=>string(item,`${path}[${i}]`)); const keys=out.map(languageKey); uniqueStrings(keys,`${path} language IDs`); return out; }
function assertUniquePackageIds(rows: StudioPackage[], label: string): void { uniqueStrings(rows.map((pkg) => pkg.manifest.id), `${label} package IDs`); }
function uniqueStrings(rows: string[], label: string): void { if (new Set(rows).size !== rows.length) throw new Error(`${label} must be unique.`); }
function object(value: unknown, path: string): Record<string, unknown> { if (!value || typeof value !== "object" || Array.isArray(value)) throw new Error(`${path} must be an object.`); return value as Record<string, unknown>; }
function array(value: unknown, path: string): unknown[] { if (!Array.isArray(value)) throw new Error(`${path} must be an array.`); return value; }
function string(value: unknown, path: string): string { if (typeof value !== "string" || !value.trim()) throw new Error(`${path} must be a non-empty string.`); return value; }
function plainString(value: unknown, path: string): string { if (typeof value !== "string") throw new Error(`${path} must be a string.`); return value; }
function bool(value: unknown, path: string): boolean { if (typeof value !== "boolean") throw new Error(`${path} must be boolean.`); return value; }
function integer(value: unknown, min: number, max: number, path: string): number { if (!Number.isSafeInteger(value) || (value as number) < min || (value as number) > max) throw new Error(`${path} is outside its supported range.`); return value as number; }
function number(value: unknown, min: number, max: number, path: string): number { if (typeof value !== "number" || !Number.isFinite(value) || value < min || value > max) throw new Error(`${path} is outside its supported range.`); return value; }
function exactKeys(value: Record<string, unknown>, allowed: string[], path: string): void { const set=new Set(allowed); for(const key of Object.keys(value)) if(!set.has(key)) throw new Error(`${path} contains unsupported key ${key}.`); for(const key of allowed) if(!(key in value)) throw new Error(`${path} is missing ${key}.`); }
