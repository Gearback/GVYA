import type { RuntimeBackend } from "./backend.js";
import type {
  GvyaAssetInfo,
  GvyaCapabilitiesInfo,
  GvyaCapabilityInfo,
  GvyaCapabilityResultRequest,
  GvyaCapabilityResultResult,
  GvyaOpenRequest,
  GvyaRuntimeInfo,
  GvyaRuntimeOpenOptions,
  GvyaTurnRequest,
  GvyaTurnResult,
} from "./contracts.js";

type WasmExports = WebAssembly.Exports & {
  readonly memory: WebAssembly.Memory;
  readonly gvya_abi_version: () => number;
  readonly gvya_pointer_width: () => number;
  readonly gvya_buffer_struct_size: () => number;
  readonly gvya_alloc: (length: number) => number;
  readonly gvya_dealloc: (ptr: number, length: number) => void;
  readonly gvya_runtime_open_with_options_json: (artifactPtr: number, artifactLen: number, optionsPtr: number, optionsLen: number, handlePtr: number, outPtr: number) => number;
  readonly gvya_runtime_close: (handle: bigint) => number;
  readonly gvya_runtime_info_json: (handle: bigint, outPtr: number) => number;
  readonly gvya_runtime_capabilities_json: (handle: bigint, outPtr: number) => number;
  readonly gvya_runtime_capability_info_json: (handle: bigint, idPtr: number, idLen: number, outPtr: number) => number;
  readonly gvya_runtime_turn_json: (handle: bigint, requestPtr: number, requestLen: number, outPtr: number) => number;
  readonly gvya_runtime_open_conversation_json: (handle: bigint, requestPtr: number, requestLen: number, outPtr: number) => number;
  readonly gvya_runtime_capability_result_json: (handle: bigint, requestPtr: number, requestLen: number, outPtr: number) => number;
  readonly gvya_runtime_asset_by_path: (handle: bigint, pathPtr: number, pathLen: number, outPtr: number) => number;
  readonly gvya_runtime_asset_by_id: (handle: bigint, idPtr: number, idLen: number, outPtr: number) => number;
  readonly gvya_runtime_asset_info_by_id_json: (handle: bigint, idPtr: number, idLen: number, outPtr: number) => number;
  readonly gvya_buffer_free: (outPtr: number) => void;
};

const encoder = new TextEncoder();
const decoder = new TextDecoder();
const DEFAULT_MAX_ARTIFACT_BYTES = 512 * 1024 * 1024;
const CANONICAL_ARTIFACT_LIMITS = {
  max_entries: 16_384,
  max_path_bytes: 512,
  max_entry_bytes: 64 * 1024 * 1024,
  max_total_bytes: DEFAULT_MAX_ARTIFACT_BYTES,
  max_manifest_bytes: 8 * 1024 * 1024,
  max_integrity_bytes: 8 * 1024 * 1024,
  max_signature_bytes: 256 * 1024,
  max_debug_map_bytes: 16 * 1024 * 1024,
} as const;
const MAX_OPEN_OPTIONS_BYTES = 64 * 1024;
const MAX_RUNTIME_REQUEST_BYTES = 1024 * 1024;

export class WasmRuntimeBackend implements RuntimeBackend {
  #exports: WasmExports;
  #handle: bigint | null = null;

  private constructor(exports: WasmExports) {
    this.#exports = exports;
    if (exports.gvya_abi_version() !== 1) throw new Error("Unsupported GVYA ABI version");
    if (exports.gvya_pointer_width() !== 32 || exports.gvya_buffer_struct_size() !== 12) {
      throw new Error("GVYA WASM adapter requires the wasm32 ABI layout");
    }
  }

  static async instantiate(wasm: BufferSource | WebAssembly.Module): Promise<WasmRuntimeBackend> {
    const instance = wasm instanceof WebAssembly.Module
      ? await WebAssembly.instantiate(wasm, {})
      : (await WebAssembly.instantiate(wasm, {})).instance;
    return new WasmRuntimeBackend(instance.exports as WasmExports);
  }

  async open(artifact: Uint8Array, options: GvyaRuntimeOpenOptions): Promise<void> {
    if (this.#handle !== null) throw new Error("GVYA runtime is already open");
    validateArtifactLimits(options.artifact_limits);
    const maxArtifactBytes = options.artifact_limits?.max_total_bytes ?? DEFAULT_MAX_ARTIFACT_BYTES;
    if (artifact.byteLength > maxArtifactBytes) throw new Error("GVYA artifact exceeds configured total byte limit before WASM copy");
    const optionsBytes = encoder.encode(JSON.stringify(options));
    if (optionsBytes.byteLength > MAX_OPEN_OPTIONS_BYTES) throw new Error("GVYA runtime open options exceed byte limit");
    const artifactPtr = this.#copyIn(artifact);
    const optionsPtr = this.#copyIn(optionsBytes);
    const handlePtr = this.#alloc(8);
    const outPtr = this.#alloc(12);
    try {
      const code = this.#exports.gvya_runtime_open_with_options_json(artifactPtr, artifact.length, optionsPtr, optionsBytes.length, handlePtr, outPtr);
      const message = this.#takeBuffer(outPtr);
      if (code !== 0) throw new Error(this.#errorMessage(code, message));
      this.#handle = new DataView(this.#exports.memory.buffer).getBigUint64(handlePtr, true);
      if (this.#handle === 0n) throw new Error("GVYA runtime returned an invalid handle");
    } finally {
      this.#exports.gvya_dealloc(artifactPtr, artifact.length);
      this.#exports.gvya_dealloc(optionsPtr, optionsBytes.length);
      this.#exports.gvya_dealloc(handlePtr, 8);
      this.#exports.gvya_dealloc(outPtr, 12);
    }
  }

  async info(): Promise<GvyaRuntimeInfo> {
    return this.#jsonNoInput("runtime info", "gvya.runtime.info", this.#exports.gvya_runtime_info_json);
  }

  async capabilities(): Promise<GvyaCapabilitiesInfo> {
    return this.#jsonNoInput("capabilities", "gvya.runtime.capabilities", this.#exports.gvya_runtime_capabilities_json);
  }

  async capabilityInfo(id: string): Promise<GvyaCapabilityInfo> {
    const response = await this.#callInput(this.#exports.gvya_runtime_capability_info_json, encoder.encode(id));
    const parsed = this.#parseJson(response);
    assertFormat(parsed, "gvya.runtime.capability-info", "capability info");
    return parsed as unknown as GvyaCapabilityInfo;
  }

  async turn(request: GvyaTurnRequest): Promise<GvyaTurnResult> {
    return this.#jsonInput("turn", "gvya.runtime.turn-result", this.#exports.gvya_runtime_turn_json, request);
  }

  async openConversation(request: GvyaOpenRequest): Promise<GvyaTurnResult> {
    return this.#jsonInput("conversation open", "gvya.runtime.turn-result", this.#exports.gvya_runtime_open_conversation_json, request);
  }

  async capabilityResult(request: GvyaCapabilityResultRequest): Promise<GvyaCapabilityResultResult> {
    return this.#jsonInput("capability result", "gvya.runtime.capability-result-result", this.#exports.gvya_runtime_capability_result_json, request);
  }

  async assetByPath(path: string): Promise<Uint8Array> {
    return this.#bytesInput("asset path", this.#exports.gvya_runtime_asset_by_path, encoder.encode(path));
  }

  async assetById(id: string): Promise<Uint8Array> {
    return this.#bytesInput("asset id", this.#exports.gvya_runtime_asset_by_id, encoder.encode(id));
  }

  async assetInfoById(id: string): Promise<GvyaAssetInfo> {
    const bytes = encoder.encode(id);
    const response = await this.#callInput(this.#exports.gvya_runtime_asset_info_by_id_json, bytes);
    const parsed = this.#parseJson(response);
    assertFormat(parsed, "gvya.runtime.asset-info", "asset info");
    return parsed as unknown as GvyaAssetInfo;
  }

  async close(): Promise<void> {
    if (this.#handle === null) return;
    const handle = this.#handle;
    this.#handle = null;
    const code = this.#exports.gvya_runtime_close(handle);
    if (code !== 0) throw new Error(`GVYA close failed with code ${code}`);
  }

  async #jsonNoInput<T>(label: string, format: string, call: (handle: bigint, outPtr: number) => number): Promise<T> {
    const handle = this.#requiredHandle();
    const outPtr = this.#alloc(12);
    try {
      const code = call(handle, outPtr);
      const response = this.#takeBuffer(outPtr);
      if (code !== 0) throw new Error(this.#errorMessage(code, response));
      const parsed = this.#parseJson(response);
      assertFormat(parsed, format, label);
      return parsed as T;
    } finally {
      this.#exports.gvya_dealloc(outPtr, 12);
    }
  }

  async #jsonInput<T>(label: string, format: string, call: (handle: bigint, ptr: number, len: number, outPtr: number) => number, value: unknown): Promise<T> {
    const response = await this.#callInput(call, encoder.encode(JSON.stringify(value)));
    const parsed = this.#parseJson(response);
    assertFormat(parsed, format, label);
    return parsed as T;
  }

  async #bytesInput(label: string, call: (handle: bigint, ptr: number, len: number, outPtr: number) => number, bytes: Uint8Array): Promise<Uint8Array> {
    try { return await this.#callInput(call, bytes); }
    catch (error) { throw new Error(`GVYA ${label} failed: ${String(error)}`); }
  }

  async #callInput(call: (handle: bigint, ptr: number, len: number, outPtr: number) => number, bytes: Uint8Array): Promise<Uint8Array> {
    const handle = this.#requiredHandle();
    if (bytes.byteLength > MAX_RUNTIME_REQUEST_BYTES) throw new Error("GVYA runtime input exceeds canonical request byte limit before WASM copy");
    const ptr = this.#copyIn(bytes);
    const outPtr = this.#alloc(12);
    try {
      const code = call(handle, ptr, bytes.length, outPtr);
      const response = this.#takeBuffer(outPtr);
      if (code !== 0) throw new Error(this.#errorMessage(code, response));
      return response;
    } finally {
      this.#exports.gvya_dealloc(ptr, bytes.length);
      this.#exports.gvya_dealloc(outPtr, 12);
    }
  }

  #requiredHandle(): bigint {
    if (this.#handle === null) throw new Error("GVYA runtime is not open");
    return this.#handle;
  }

  #alloc(length: number): number {
    const ptr = this.#exports.gvya_alloc(length);
    if (length > 0 && ptr === 0) throw new Error("GVYA WASM allocation failed");
    return ptr;
  }

  #copyIn(bytes: Uint8Array): number {
    const ptr = this.#alloc(bytes.length);
    if (bytes.length > 0) new Uint8Array(this.#exports.memory.buffer, ptr, bytes.length).set(bytes);
    return ptr;
  }

  #takeBuffer(outPtr: number): Uint8Array {
    const view = new DataView(this.#exports.memory.buffer);
    const ptr = view.getUint32(outPtr, true);
    const len = view.getUint32(outPtr + 4, true);
    const capacity = view.getUint32(outPtr + 8, true);
    if (len > capacity) {
      // Always ask the ABI to reset the output descriptor even when the module returned corrupt
      // metadata. The ABI itself refuses to free an impossible len/capacity pair.
      this.#exports.gvya_buffer_free(outPtr);
      throw new Error("GVYA returned a corrupt output buffer");
    }
    const copy = ptr === 0 || len === 0 ? new Uint8Array() : new Uint8Array(new Uint8Array(this.#exports.memory.buffer, ptr, len));
    this.#exports.gvya_buffer_free(outPtr);
    return copy;
  }

  #parseJson(bytes: Uint8Array): unknown { return JSON.parse(decoder.decode(bytes)); }

  #errorMessage(code: number, bytes: Uint8Array): string {
    if (bytes.length === 0) return `GVYA call failed with code ${code}`;
    try {
      const value = JSON.parse(decoder.decode(bytes)) as { error?: unknown };
      if (typeof value.error === "string") return `GVYA ${code}: ${value.error}`;
    } catch { /* raw fallback below */ }
    return `GVYA ${code}: ${decoder.decode(bytes)}`;
  }
}

function validateArtifactLimits(limits: GvyaRuntimeOpenOptions["artifact_limits"]): void {
  if (limits === undefined) return;
  for (const [name, ceiling] of Object.entries(CANONICAL_ARTIFACT_LIMITS)) {
    const value = limits[name as keyof typeof CANONICAL_ARTIFACT_LIMITS];
    if (value === undefined) continue;
    if (!Number.isSafeInteger(value) || value <= 0) throw new Error(`GVYA ${name} must be a positive safe integer`);
    if (value > ceiling) throw new Error(`GVYA ${name} cannot exceed the canonical ceiling`);
  }
}

function assertFormat(value: unknown, format: string, label: string): asserts value is Record<string, unknown> {
  if (typeof value !== "object" || value === null) throw new Error(`GVYA returned a non-object ${label}`);
  const row = value as Record<string, unknown>;
  if (row.format !== format || row.version !== 1) throw new Error(`GVYA returned an unsupported ${label}`);
}
