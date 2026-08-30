import { isWellFormedLanguageTag, languageKey } from "./languages.js";
import type { JsonObject, JsonValue, MatcherProfile, ResponseDefinition, StudioPackage } from "./types.js";

export const MATCHER_PROFILE_FORMAT = "gvya.source.matcher-profile";
export const LANGUAGE_PROFILE_FORMAT = "gvya.source.language-profile";

export function languageProfilePath(language: string): string {
  if (!isWellFormedLanguageTag(language)) throw new Error(`Language Profile language is invalid: ${language}.`);
  return `language-profiles/${languageKey(language)}.json`;
}

export function matcherProfilePath(language: string): string {
  if (!isWellFormedLanguageTag(language)) throw new Error(`Matcher Profile language is invalid: ${language}.`);
  return `matcher-profiles/${languageKey(language)}.json`;
}

export function matcherProfileSourceDocument(profile: MatcherProfile): JsonObject {
  matcherProfilePath(profile.language);
  return {
    format: MATCHER_PROFILE_FORMAT,
    version: 1,
    language: profile.language,
    profile: structuredClone(profile.profile) as JsonValue,
  };
}

export function languageProfileSourceDocument(profile: MatcherProfile): JsonObject {
  languageProfilePath(profile.language);
  return {
    format: LANGUAGE_PROFILE_FORMAT,
    version: 1,
    language: profile.language,
    profile: structuredClone(profile.language_profile) as JsonValue,
  };
}

export function profileCatalogDocument(profile: MatcherProfile): JsonObject {
  matcherProfilePath(profile.language);
  return {
    language: profile.language,
    language_profile: structuredClone(profile.language_profile) as JsonValue,
    profile: structuredClone(profile.profile) as JsonValue,
  };
}

export function parseMatcherProfileDocument(value: unknown, path: string): MatcherProfile {
  if (!value || typeof value !== "object" || Array.isArray(value)) throw new Error(`${path} must be an object.`);
  const row = value as Record<string, unknown>;
  const allowed = new Set(["format", "version", "language", "profile"]);
  for (const key of Object.keys(row)) if (!allowed.has(key)) throw new Error(`${path} contains unsupported key ${key}.`);
  for (const key of allowed) if (!(key in row)) throw new Error(`${path} is missing ${key}.`);
  if (row.format !== MATCHER_PROFILE_FORMAT || row.version !== 1) throw new Error(`${path} must be ${MATCHER_PROFILE_FORMAT} version 1.`);
  if (typeof row.language !== "string" || !isWellFormedLanguageTag(row.language)) throw new Error(`${path} has an invalid BCP 47 language.`);
  if (!row.profile || typeof row.profile !== "object" || Array.isArray(row.profile)) throw new Error(`${path}#profile must be an object.`);
  if (path.endsWith(".json") && !path.endsWith(`/${languageKey(row.language)}.json`)) throw new Error(`${path} must use the normalized language as its filename.`);
  return { language: row.language, language_profile: {}, profile: structuredClone(row.profile) as JsonObject };
}

export function parseLanguageProfileDocument(value: unknown, path: string): MatcherProfile {
  if (!value || typeof value !== "object" || Array.isArray(value)) throw new Error(`${path} must be an object.`);
  const row = value as Record<string, unknown>;
  const allowed = new Set(["format", "version", "language", "profile"]);
  for (const key of Object.keys(row)) if (!allowed.has(key)) throw new Error(`${path} contains unsupported key ${key}.`);
  for (const key of allowed) if (!(key in row)) throw new Error(`${path} is missing ${key}.`);
  if (row.format !== LANGUAGE_PROFILE_FORMAT || row.version !== 1) throw new Error(`${path} must be ${LANGUAGE_PROFILE_FORMAT} version 1.`);
  if (typeof row.language !== "string" || !isWellFormedLanguageTag(row.language)) throw new Error(`${path} has an invalid BCP 47 language.`);
  if (!row.profile || typeof row.profile !== "object" || Array.isArray(row.profile)) throw new Error(`${path}#profile must be an object.`);
  if (path.endsWith(".json") && !path.endsWith(`/${languageKey(row.language)}.json`)) throw new Error(`${path} must use the normalized language as its filename.`);
  return { language: row.language, language_profile: structuredClone(row.profile) as JsonObject, profile: {} };
}

export function parseProfileCatalogDocument(value: unknown, path: string): MatcherProfile {
  if (!value || typeof value !== "object" || Array.isArray(value)) throw new Error(`${path} must be an object.`);
  const row = value as Record<string, unknown>;
  const allowed = new Set(["language", "language_profile", "profile"]);
  for (const key of Object.keys(row)) if (!allowed.has(key)) throw new Error(`${path} contains unsupported key ${key}.`);
  for (const key of allowed) if (!(key in row)) throw new Error(`${path} is missing ${key}.`);
  if (typeof row.language !== "string" || !isWellFormedLanguageTag(row.language)) throw new Error(`${path} has an invalid BCP 47 language.`);
  if (!row.language_profile || typeof row.language_profile !== "object" || Array.isArray(row.language_profile)) throw new Error(`${path}.language_profile must be an object.`);
  if (!row.profile || typeof row.profile !== "object" || Array.isArray(row.profile)) throw new Error(`${path}.profile must be an object.`);
  return { language: row.language, language_profile: structuredClone(row.language_profile) as JsonObject, profile: structuredClone(row.profile) as JsonObject };
}

export function pairProfileDocuments(language: MatcherProfile, matcher: MatcherProfile, path: string): MatcherProfile {
  if (languageKey(language.language) !== languageKey(matcher.language)) throw new Error(`${path} Language and Matcher Profile languages must match.`);
  return { language: matcher.language, language_profile: structuredClone(language.language_profile), profile: structuredClone(matcher.profile) };
}

export function assertUniqueMatcherProfiles(profiles: readonly MatcherProfile[], label: string): void {
  const seen = new Set<string>();
  for (const profile of profiles) {
    matcherProfileSourceDocument(profile);
    languageProfileSourceDocument(profile);
    const key = languageKey(profile.language);
    if (seen.has(key)) throw new Error(`${label} contains duplicate Matcher Profile ${profile.language}.`);
    seen.add(key);
  }
}

export function matcherProfileLanguages(profiles: readonly MatcherProfile[]): string[] {
  assertUniqueMatcherProfiles(profiles, "Matcher Profile catalog");
  return profiles.map((profile) => profile.language);
}

export function missingMatcherProfileLanguages(
  languages: readonly string[],
  profiles: readonly MatcherProfile[],
): string[] {
  const available = new Set(matcherProfileLanguages(profiles).map(languageKey));
  const seen = new Set<string>();
  return languages.filter((language) => {
    const key = languageKey(language);
    if (!isWellFormedLanguageTag(language) || seen.has(key) || available.has(key)) return false;
    seen.add(key);
    return true;
  });
}

function languageCollector(): { add: (language: string) => void; rows: string[] } {
  const rows: string[] = [];
  const seen = new Set<string>();
  return {
    rows,
    add(language: string): void {
      if (!isWellFormedLanguageTag(language)) return;
      const key = languageKey(language);
      if (!seen.has(key)) { seen.add(key); rows.push(language); }
    },
  };
}

/**
 * Languages that need a compiled Semantic Profile: exactly the matcher evidence the kernel indexes.
 *
 * This mirrors `validate_catalog_languages` / `SemanticIndex::build` in the Rust kernel, which look
 * a profile up by exact normalized tag for every Meaning structural pattern, sample, negative
 * sample and retrieval term. A compiled program's profile map is keyed exactly by the Bot's enabled
 * languages, so any evidence language outside that set fails the build.
 */
export function packageMatcherEvidenceLanguages(pkg: StudioPackage): string[] {
  const { add, rows } = languageCollector();
  for (const meaning of pkg.contents.meanings) {
    for (const sample of [...meaning.value.patterns, ...meaning.value.samples, ...meaning.value.negative_samples, ...meaning.value.retrieval_terms]) add(sample.language);
  }
  return rows;
}

/**
 * Every language a Package writes into canonical source, in stable first-use order.
 *
 * This mirrors `validate_project_language_usage` in the compiler, which requires each of these to
 * be named by the Project language catalog. Response, Regression Case and Scenario languages need
 * no Semantic Profile — only matcher evidence does.
 */
export function packageAuthoredLanguages(pkg: StudioPackage): string[] {
  const { add, rows } = languageCollector();
  const addResponse = (response: ResponseDefinition): void => {
    for (const text of response.texts) add(text.language);
    for (const extra of response.extra_messages) for (const text of extra.texts) add(text.language);
  };
  for (const language of packageMatcherEvidenceLanguages(pkg)) add(language);
  for (const owner of [
    ...pkg.contents.behaviors,
    ...pkg.contents.capability_result_behaviors,
    ...pkg.contents.openings,
    ...pkg.contents.fallback_behaviors,
  ]) for (const response of owner.value.responses) addResponse(response);
  for (const test of pkg.contents.regression_cases) add(test.value.language);
  for (const scenario of pkg.contents.scenarios) for (const step of scenario.value.steps) if (step.type !== "confirm") add(step.language);
  return rows;
}

/** All explicit language contracts owned by a Package, including its Studio-only authoring preference. */
export function packageLanguages(pkg: StudioPackage): string[] {
  const { add, rows } = languageCollector();
  add(pkg.authoring_language);
  for (const language of packageAuthoredLanguages(pkg)) add(language);
  return rows;
}

/** Select one profile per requested language. Earlier catalogs own precedence. */
export function matcherProfilesForLanguages(
  languages: readonly string[],
  ...catalogs: readonly (readonly MatcherProfile[])[]
): MatcherProfile[] {
  const requested = new Set(languages.map(languageKey));
  const selected = new Map<string, MatcherProfile>();
  for (const catalog of catalogs) {
    assertUniqueMatcherProfiles(catalog, "Matcher Profile catalog");
    for (const profile of catalog) {
      const key = languageKey(profile.language);
      if (requested.has(key) && !selected.has(key)) selected.set(key, structuredClone(profile));
    }
  }
  return languages.flatMap((language) => {
    const profile = selected.get(languageKey(language));
    return profile ? [profile] : [];
  });
}
