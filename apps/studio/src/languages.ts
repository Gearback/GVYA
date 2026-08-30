import { normalizeLanguageTag } from "./canonical-order.js";
const LANGUAGE_TAG = /^[A-Za-z]{1,8}(?:-[A-Za-z0-9]{1,8})*$/u;
export const STUDIO_LANGUAGE_LIMIT = 32;

export function languageKey(value: string): string {
  return normalizeLanguageTag(value);
}

export function isWellFormedLanguageTag(value: string): boolean {
  return value.length <= 63 && LANGUAGE_TAG.test(value);
}

export function assertLanguageCatalog(languages: readonly string[], label: string): void {
  if (languages.length === 0 || languages.length > STUDIO_LANGUAGE_LIMIT) throw new Error(`${label} must contain 1..=${STUDIO_LANGUAGE_LIMIT} languages.`);
  const seen = new Set<string>();
  for (const language of languages) {
    if (!isWellFormedLanguageTag(language)) throw new Error(`${label} contains invalid BCP 47 tag ${language || "(empty)"}.`);
    const key = languageKey(language);
    if (seen.has(key)) throw new Error(`${label} contains duplicate language ${language}.`);
    seen.add(key);
  }
}

export function defaultAuthoringLanguage(languages: readonly string[]): string {
  const language = languages[0];
  if (!language) throw new Error("At least one Project language is required for language-bearing authoring.");
  return language;
}
