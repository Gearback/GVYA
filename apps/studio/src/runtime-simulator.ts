import { GvyaRuntime } from "../../../packages/runtime-sdk/dist/runtime.js";
import { WasmRuntimeBackend } from "../../../packages/runtime-sdk/dist/wasm.js";
import { unsignedDevelopmentOpenOptions } from "../../../packages/runtime-sdk/dist/contracts.js";
import type { GvyaCapabilityResultResult, GvyaRuntimeInfo, GvyaTurnRequest, GvyaTurnResult, InvocationProposal, JsonValue } from "../../../packages/runtime-sdk/dist/contracts.js";

export interface SimulationContext {
  values: Record<string, JsonValue>;
  availableCapabilities: Array<{ id: string; version: string }>;
  visibleReferences: Array<{ kind: string; id: string }>;
}

const ENGLISH_PARTS_OF_DAY = ["night", "morning", "afternoon", "evening"] as const;
const PERSIAN_PARTS_OF_DAY = ["شب", "صبح", "بعدازظهر", "عصر"] as const;
const ENGLISH_SEASONS = ["winter", "spring", "summer", "autumn"] as const;
const PERSIAN_SEASONS = ["زمستان", "بهار", "تابستان", "پاییز"] as const;

/** Explicit local host facts for Studio preview turns; the canonical runtime never reads ambient time. */
export function studioLocalSystemValues(at: Date, language: string): Record<string, JsonValue> {
  if (!Number.isFinite(at.getTime())) throw new Error("Studio simulation requires a valid local date and time.");
  const locale = supportedLocale(language);
  const persian = locale.toLowerCase().split("-")[0] === "fa";
  const calendarParts = new Intl.DateTimeFormat(locale, { year: "numeric", month: "numeric" }).formatToParts(at);
  const timeParts = new Intl.DateTimeFormat(locale, { hour: "2-digit", minute: "2-digit", hourCycle: "h23" }).formatToParts(at);
  const hour = at.getHours();
  const partOfDayIndex = hour < 5 || hour >= 21 ? 0 : hour < 12 ? 1 : hour < 17 ? 2 : 3;
  const month = at.getMonth();
  const seasonIndex = month < 2 || month === 11 ? 0 : month < 5 ? 1 : month < 8 ? 2 : 3;
  return {
    unix_time_ms: at.getTime(),
    time: new Intl.DateTimeFormat(locale, { hour: "2-digit", minute: "2-digit", hourCycle: "h23" }).format(at),
    time12: new Intl.DateTimeFormat(locale, { hour: "numeric", minute: "2-digit", hourCycle: "h12" }).format(at),
    dayOfWeek: new Intl.DateTimeFormat(locale, { weekday: "long" }).format(at),
    dateLong: new Intl.DateTimeFormat(locale, { dateStyle: "long" }).format(at),
    month: dateTimePart(calendarParts, "month", new Intl.NumberFormat(locale, { useGrouping: false }).format(month + 1)),
    monthName: new Intl.DateTimeFormat(locale, { month: "long" }).format(at),
    year: dateTimePart(calendarParts, "year", new Intl.NumberFormat(locale, { useGrouping: false }).format(at.getFullYear())),
    partOfDay: (persian ? PERSIAN_PARTS_OF_DAY : ENGLISH_PARTS_OF_DAY)[partOfDayIndex],
    season: (persian ? PERSIAN_SEASONS : ENGLISH_SEASONS)[seasonIndex],
    hour: dateTimePart(timeParts, "hour", new Intl.NumberFormat(locale, { minimumIntegerDigits: 2, useGrouping: false }).format(hour)),
    minute: dateTimePart(timeParts, "minute", new Intl.NumberFormat(locale, { minimumIntegerDigits: 2, useGrouping: false }).format(at.getMinutes())),
  };
}

function supportedLocale(language: string): string {
  const requested = language.trim() || "en-US";
  try { return Intl.DateTimeFormat.supportedLocalesOf([requested])[0] ?? "en-US"; }
  catch { return "en-US"; }
}

function dateTimePart(parts: Intl.DateTimeFormatPart[], type: Intl.DateTimeFormatPartTypes, fallback: string): string {
  return parts.find((part) => part.type === type)?.value ?? fallback;
}

function jsonRecord(value: JsonValue | undefined): Record<string, JsonValue> | null {
  return value !== null && typeof value === "object" && !Array.isArray(value) ? value as Record<string, JsonValue> : null;
}

export class StudioRuntimeSession {
  #runtime: GvyaRuntime | null = null;
  #state: JsonValue | undefined;
  #lastTurnRequest: GvyaTurnRequest | null = null;
  #info: GvyaRuntimeInfo | null = null;

  get info(): GvyaRuntimeInfo | null { return this.#info; }

  async open(wasmBytes: BufferSource | WebAssembly.Module, artifactBytes: Uint8Array): Promise<GvyaRuntimeInfo> {
    await this.close();
    const backend = await WasmRuntimeBackend.instantiate(wasmBytes);
    this.#runtime = await GvyaRuntime.open(artifactBytes, backend, unsignedDevelopmentOpenOptions());
    this.#info = await this.#runtime.info();
    this.#state = undefined;
    this.#lastTurnRequest = null;
    return this.#info;
  }

  async turn(text: string, context: SimulationContext, seed: number | null): Promise<GvyaTurnResult> {
    if (this.#runtime === null) throw new Error("GVYA Studio simulation is not ready.");
    const request = {
      format: "gvya.runtime.turn" as const,
      version: 1 as const,
      utterance: { text },
      context: {
        values: context.values,
        available_capabilities: context.availableCapabilities,
        visible_references: context.visibleReferences,
      },
      system: studioLocalSystemValues(new Date(), this.#activeLanguage()),
      ...(this.#state === undefined ? {} : { state: this.#state }),
      seed,
    };
    this.#lastTurnRequest = request;
    const result = await this.#runtime.turn(request);
    this.#state = result.state;
    return result;
  }

  async confirmProposal(proposal: InvocationProposal, confirmed: boolean, context: SimulationContext): Promise<GvyaTurnResult> {
    if (this.#runtime === null || this.#lastTurnRequest === null) throw new Error("Run the turn that produced this confirmation proposal first.");
    const request: GvyaTurnRequest = {
      ...this.#lastTurnRequest,
      context: { values: context.values, available_capabilities: context.availableCapabilities, visible_references: context.visibleReferences },
    };
    const result = await this.#runtime.confirmTurn(request, proposal, confirmed, `studio-confirm-${proposal.id}`);
    this.#state = result.state;
    this.#lastTurnRequest = request;
    return result;
  }

  async capabilityResult(
    proposal: InvocationProposal,
    succeeded: boolean,
    output: JsonValue | undefined,
    errorCode: string | null,
    context: SimulationContext,
    seed: number | null,
  ): Promise<GvyaCapabilityResultResult> {
    if (this.#runtime === null) throw new Error("GVYA Studio simulation is not ready.");
    const result = await this.#runtime.capabilityResult({
      format: "gvya.runtime.capability-result",
      version: 1,
      proposal,
      result: {
        proposal_id: proposal.id,
        succeeded,
        ...(output === undefined ? {} : { output }),
        ...(errorCode ? { error_code: errorCode } : {}),
      },
      context: { values: context.values, available_capabilities: context.availableCapabilities, visible_references: context.visibleReferences },
      system: studioLocalSystemValues(new Date(), this.#activeLanguage()),
      ...(this.#state === undefined ? {} : { state: this.#state }),
      seed,
    });
    if (result.interaction) this.#state = result.interaction.state;
    return result;
  }

  resetState(): void { this.#state = undefined; this.#lastTurnRequest = null; }

  #activeLanguage(): string {
    const state = jsonRecord(this.#state);
    const conversation = jsonRecord(state?.conversation);
    const activeLanguage = conversation?.active_language;
    if (typeof activeLanguage === "string" && activeLanguage.trim()) return activeLanguage;
    return this.#info?.default_language ?? "en-US";
  }

  async close(): Promise<void> {
    if (this.#runtime !== null) await this.#runtime.close();
    this.#runtime = null;
    this.#info = null;
    this.#state = undefined;
    this.#lastTurnRequest = null;
  }
}

export const STUDIO_SIMULATOR_JSON_MAX_BYTES = 1024 * 1024;

export function parseBoundedSimulatorJson(text: string, label: string): JsonValue {
  if (new TextEncoder().encode(text).byteLength > STUDIO_SIMULATOR_JSON_MAX_BYTES) throw new Error(`${label} exceeds the ${STUDIO_SIMULATOR_JSON_MAX_BYTES} byte simulator JSON limit.`);
  return JSON.parse(text) as JsonValue;
}
