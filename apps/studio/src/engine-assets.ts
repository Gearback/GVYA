export const STUDIO_ENGINE_VERSION = "v1" as const;

interface EngineBinaryManifest {
  path: string;
  sha256: string;
  bytes: number;
  abi: number;
}

interface EngineManifest {
  format: "gvya.engine-assets";
  version: 1;
  engine: typeof STUDIO_ENGINE_VERSION;
  artifact_format: 1;
  module: EngineBinaryManifest;
}

export interface EngineAssets {
  manifest: EngineManifest;
  engineWasm: Uint8Array;
  engineModule: WebAssembly.Module;
}

const ENGINE_MAX_WASM_BYTES = 192 * 1024 * 1024;
let cached: Promise<EngineAssets> | null = null;

export function loadBundledEngineAssets(): Promise<EngineAssets> {
  cached ??= load().catch((error) => { cached = null; throw error; });
  return cached;
}

async function load(): Promise<EngineAssets> {
  const base = new URL(`./engine/${STUDIO_ENGINE_VERSION}/`, document.baseURI);
  const manifestResponse = await fetch(new URL("manifest.json", base), { cache: "no-cache" });
  if (!manifestResponse.ok) throw new Error("GVYA Studio built-in Engine is unavailable. Reinstall or replace this Studio build.");
  const manifest = validateManifest(await manifestResponse.json());
  const engineWasm = await loadBinary(base, manifest.module);
  let engineModule: WebAssembly.Module;
  try {
    engineModule = await WebAssembly.compile(engineWasm);
  } catch {
    throw new Error("GVYA Studio built-in Engine is invalid. Reinstall or replace this Studio build.");
  }
  return { manifest, engineWasm, engineModule };
}

async function loadBinary(base: URL, row: EngineBinaryManifest): Promise<Uint8Array> {
  if (row.bytes <= 0 || row.bytes > ENGINE_MAX_WASM_BYTES) throw new Error("GVYA Studio built-in Engine manifest is invalid.");
  const binaryUrl = new URL(row.path, base);
  binaryUrl.searchParams.set("sha256", row.sha256);

  const cachedBytes = await fetchBinary(binaryUrl, "force-cache");
  if (await matchesBinaryManifest(cachedBytes, row)) return cachedBytes;

  // A development server/browser may retain an older same-path Engine asset across clean-break baselines.
  // Never weaken integrity checks: bypass every cache once, then verify the fresh bytes against the manifest.
  const freshUrl = new URL(binaryUrl);
  freshUrl.searchParams.set("fresh", `${Date.now()}-${Math.random().toString(36).slice(2)}`);
  const freshBytes = await fetchBinary(freshUrl, "no-store");
  if (await matchesBinaryManifest(freshBytes, row)) return freshBytes;
  throw new Error("GVYA Studio built-in Engine asset failed integrity verification after a fresh reload.");
}

async function fetchBinary(url: URL, cache: RequestCache): Promise<Uint8Array> {
  const response = await fetch(url, { cache });
  if (!response.ok) throw new Error("GVYA Studio built-in Engine is incomplete. Reinstall or replace this Studio build.");
  return new Uint8Array(await response.arrayBuffer());
}

async function matchesBinaryManifest(bytes: Uint8Array, row: EngineBinaryManifest): Promise<boolean> {
  if (bytes.byteLength !== row.bytes) return false;
  return await sha256(bytes) === row.sha256;
}

function validateManifest(value: unknown): EngineManifest {
  if (!plain(value)) throw new Error("Bundled GVYA Engine manifest is invalid.");
  const keys = Object.keys(value).sort();
  if (JSON.stringify(keys) !== JSON.stringify(["artifact_format", "engine", "format", "module", "version"])) throw new Error("Bundled GVYA Engine manifest has unsupported fields.");
  if (value.format !== "gvya.engine-assets" || value.version !== 1 || value.engine !== STUDIO_ENGINE_VERSION || value.artifact_format !== 1) throw new Error("Bundled GVYA Engine manifest version is unsupported.");
  const module = binary(value.module);
  return { format: "gvya.engine-assets", version: 1, engine: STUDIO_ENGINE_VERSION, artifact_format: 1, module };
}

function binary(value: unknown): EngineBinaryManifest {
  if (!plain(value)) throw new Error("Bundled GVYA Engine module manifest is invalid.");
  if (JSON.stringify(Object.keys(value).sort()) !== JSON.stringify(["abi", "bytes", "path", "sha256"])) throw new Error("Bundled GVYA Engine module manifest has unsupported fields.");
  if (value.path !== "gvya-ffi.wasm") throw new Error("Bundled GVYA Engine module path is invalid.");
  if (typeof value.sha256 !== "string" || !/^[0-9a-f]{64}$/u.test(value.sha256)) throw new Error("Bundled GVYA Engine module digest is invalid.");
  if (typeof value.bytes !== "number" || !Number.isSafeInteger(value.bytes)) throw new Error("Bundled GVYA Engine module byte size is invalid.");
  if (value.abi !== 1) throw new Error("Bundled GVYA Engine ABI is unsupported.");
  return { path: value.path, sha256: value.sha256, bytes: value.bytes, abi: value.abi };
}

async function sha256(bytes: Uint8Array): Promise<string> {
  const digest = await crypto.subtle.digest("SHA-256", bytes);
  return [...new Uint8Array(digest)].map((value) => value.toString(16).padStart(2, "0")).join("");
}

function plain(value: unknown): value is Record<string, unknown> {
  return value !== null && typeof value === "object" && !Array.isArray(value) && (Object.getPrototypeOf(value) === Object.prototype || Object.getPrototypeOf(value) === null);
}
