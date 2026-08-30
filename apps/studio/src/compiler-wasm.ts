import { compareUtf8 } from "./canonical-order.js";
import { packageAssetPath, validateAssetFiles } from "./asset-files.js";
import type { StudioAssetFile, StudioBrainWorkspace } from "./types.js";
import { sourceFiles, stableJson } from "./workspace.js";

export interface CompilerSourceEntry { path: string; bytes: Uint8Array; }

type CompilerExports = WebAssembly.Exports & {
  readonly memory: WebAssembly.Memory;
  readonly gvya_abi_version: () => number;
  readonly gvya_pointer_width: () => number;
  readonly gvya_buffer_struct_size: () => number;
  readonly gvya_alloc: (length: number) => number;
  readonly gvya_dealloc: (ptr: number, length: number) => void;
  readonly gvya_compiler_validate_source_tree: (archivePtr: number, archiveLen: number, outPtr: number) => number;
  readonly gvya_compiler_build_source_tree: (archivePtr: number, archiveLen: number, outPtr: number) => number;
  readonly gvya_buffer_free: (outPtr: number) => void;
};

const encoder = new TextEncoder();
const decoder = new TextDecoder();
const MAGIC = encoder.encode("GVYASRC1");
const MAX_ARCHIVE_BYTES = 160 * 1024 * 1024;

export class WasmCompilerBackend {
  #exports: CompilerExports;

  private constructor(exports: CompilerExports) {
    this.#exports = exports;
    if (exports.gvya_abi_version() !== 1) throw new Error("Unsupported GVYA Engine ABI version.");
    if (exports.gvya_pointer_width() !== 32 || exports.gvya_buffer_struct_size() !== 12) {
      throw new Error("GVYA compiler WASM requires the wasm32 ABI layout.");
    }
  }

  static async instantiate(wasm: BufferSource | WebAssembly.Module): Promise<WasmCompilerBackend> {
    const instance = wasm instanceof WebAssembly.Module
      ? await WebAssembly.instantiate(wasm, {})
      : (await WebAssembly.instantiate(wasm, {})).instance;
    return new WasmCompilerBackend(instance.exports as CompilerExports);
  }

  validate(entries: readonly CompilerSourceEntry[]): void {
    const archive = packSourceArchive(entries);
    const archivePtr = this.#copyIn(archive);
    const outPtr = this.#alloc(12);
    try {
      const code = this.#exports.gvya_compiler_validate_source_tree(archivePtr, archive.byteLength, outPtr);
      const output = this.#takeBuffer(outPtr);
      if (code !== 0) throw compilerError(code, output);
    } finally {
      this.#exports.gvya_dealloc(archivePtr, archive.byteLength);
      this.#exports.gvya_dealloc(outPtr, 12);
    }
  }

  compile(entries: readonly CompilerSourceEntry[]): Uint8Array {
    const archive = packSourceArchive(entries);
    const archivePtr = this.#copyIn(archive);
    const outPtr = this.#alloc(12);
    try {
      const code = this.#exports.gvya_compiler_build_source_tree(archivePtr, archive.byteLength, outPtr);
      const output = this.#takeBuffer(outPtr);
      if (code !== 0) throw compilerError(code, output);
      return output;
    } finally {
      this.#exports.gvya_dealloc(archivePtr, archive.byteLength);
      this.#exports.gvya_dealloc(outPtr, 12);
    }
  }

  #alloc(length: number): number {
    const ptr = this.#exports.gvya_alloc(length);
    if (length > 0 && ptr === 0) throw new Error("GVYA compiler WASM allocation failed.");
    return ptr;
  }

  #copyIn(bytes: Uint8Array): number {
    const ptr = this.#alloc(bytes.byteLength);
    if (bytes.byteLength > 0) new Uint8Array(this.#exports.memory.buffer, ptr, bytes.byteLength).set(bytes);
    return ptr;
  }

  #takeBuffer(outPtr: number): Uint8Array {
    const view = new DataView(this.#exports.memory.buffer);
    const ptr = view.getUint32(outPtr, true);
    const len = view.getUint32(outPtr + 4, true);
    const capacity = view.getUint32(outPtr + 8, true);
    if (len > capacity) {
      this.#exports.gvya_buffer_free(outPtr);
      throw new Error("GVYA compiler returned a corrupt output buffer.");
    }
    const copy = ptr === 0 || len === 0 ? new Uint8Array() : new Uint8Array(new Uint8Array(this.#exports.memory.buffer, ptr, len));
    this.#exports.gvya_buffer_free(outPtr);
    return copy;
  }
}

export async function compilerSourceEntries(workspace: StudioBrainWorkspace, assetFiles: readonly StudioAssetFile[]): Promise<CompilerSourceEntry[]> {
  const entries = sourceFiles(workspace).map((entry) => ({ path: entry.path, bytes: encoder.encode(stableJson(entry.json)) }));
  const filesByPackageSource = new Map<string, StudioAssetFile>();
  for (const file of validateAssetFiles(assetFiles)) {
    const key = `${file.package_id}\0${file.source}`;
    if (filesByPackageSource.has(key)) throw new Error(`Materialized Brain has multiple owners for ${file.package_id}/${file.source}.`);
    filesByPackageSource.set(key, file);
  }
  const emittedPaths = new Set(entries.map((entry) => entry.path));
  for (const pkg of workspace.packages) {
    for (const contribution of pkg.contents.assets) {
      const asset = contribution.value;
      const file = filesByPackageSource.get(`${pkg.manifest.id}\0${asset.source}`);
      if (!file) throw new Error(`Asset ${asset.id} is missing source bytes for ${pkg.manifest.id}/${asset.source}.`);
      const path = packageAssetPath(pkg, asset.source);
      if (emittedPaths.has(path)) continue;
      emittedPaths.add(path);
      entries.push({ path, bytes: new Uint8Array(await file.blob.arrayBuffer()) });
    }
  }
  return entries;
}

export function packSourceArchive(entries: readonly CompilerSourceEntry[]): Uint8Array {
  if (entries.length === 0 || entries.length > 50_000) throw new Error("Compiler source-tree file count is outside the supported range.");
  const sorted = [...entries].sort((a, b) => compareUtf8(a.path, b.path));
  let length = 12;
  const paths = new Set<string>();
  const prepared = sorted.map((entry) => {
    const pathBytes = encoder.encode(entry.path);
    if (pathBytes.byteLength === 0 || pathBytes.byteLength > 4096) throw new Error(`Compiler source path is invalid: ${entry.path}`);
    if (paths.has(entry.path)) throw new Error(`Compiler source path is duplicated: ${entry.path}`);
    paths.add(entry.path);
    if (entry.bytes.byteLength > 64 * 1024 * 1024) throw new Error(`Compiler source file is too large: ${entry.path}`);
    length += 8 + pathBytes.byteLength + entry.bytes.byteLength;
    if (!Number.isSafeInteger(length) || length > MAX_ARCHIVE_BYTES) throw new Error("Compiler source archive exceeds the Studio transport limit.");
    return { ...entry, pathBytes };
  });
  const out = new Uint8Array(length);
  out.set(MAGIC, 0);
  const view = new DataView(out.buffer);
  let offset = 8;
  view.setUint32(offset, prepared.length, true); offset += 4;
  for (const entry of prepared) {
    view.setUint32(offset, entry.pathBytes.byteLength, true); offset += 4;
    view.setUint32(offset, entry.bytes.byteLength, true); offset += 4;
    out.set(entry.pathBytes, offset); offset += entry.pathBytes.byteLength;
    out.set(entry.bytes, offset); offset += entry.bytes.byteLength;
  }
  return out;
}

export async function sourceFingerprint(entries: readonly CompilerSourceEntry[]): Promise<string> {
  const digest = await crypto.subtle.digest("SHA-256", packSourceArchive(entries));
  return [...new Uint8Array(digest)].map((value) => value.toString(16).padStart(2, "0")).join("");
}

function compilerError(code: number, bytes: Uint8Array): Error {
  if (bytes.byteLength === 0) return new Error(`GVYA compiler failed with code ${code}.`);
  try {
    const value = JSON.parse(decoder.decode(bytes)) as { kind?: unknown; message?: unknown; details?: unknown };
    const kind = typeof value.kind === "string" ? value.kind : "build_failed";
    const message = typeof value.message === "string" ? value.message : "Canonical compilation failed.";
    const detail = diagnosticDetail(value.details);
    return new Error(detail ? `${message} ${detail}` : `${message} (${kind})`);
  } catch {
    return new Error(`GVYA compiler ${code}: ${decoder.decode(bytes)}`);
  }
}

const MAX_REPORTED_DIAGNOSTICS = 5;

function diagnosticDetail(details: unknown): string {
  if (!Array.isArray(details)) return "";
  const rows = details.filter((row): row is Record<string, unknown> => !!row && typeof row === "object").map(diagnosticLine).filter(Boolean);
  if (rows.length === 0) return "";
  const shown = rows.slice(0, MAX_REPORTED_DIAGNOSTICS);
  const remaining = rows.length - shown.length;
  const suffix = remaining > 0 ? ` (+${remaining} more)` : "";
  return `${shown.join(" | ")}${suffix}`;
}

function diagnosticLine(row: Record<string, unknown>): string {
  const code = typeof row.code === "string" ? row.code : "";
  const message = typeof row.message === "string" ? row.message : "";
  const path = typeof row.path === "string" ? row.path : "";
  const remediation = typeof row.remediation === "string" ? row.remediation : "";
  const head = [path, message].filter(Boolean).join(": ");
  const body = head || code;
  if (!body) return "";
  const codeSuffix = code && head ? ` [${code}]` : "";
  return `${body}${codeSuffix}${remediation ? ` — ${remediation}` : ""}`;
}
