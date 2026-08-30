import godotRuntime from "../../../adapters/godot/GVYARuntime.gd?raw";
import godotWebBridge from "../../../adapters/godot/web/gvya-godot-web.js?raw";
import { compilerSourceEntries, WasmCompilerBackend } from "./compiler-wasm.js";
import { loadBundledEngineAssets, STUDIO_ENGINE_VERSION } from "./engine-assets.js";
import { createTarGz } from "./tar.js";
import type { RuntimeExportBundle } from "./runtime-exporters.js";
import type { StudioAssetFile, StudioBrainWorkspace } from "./types.js";

const encoder = new TextEncoder();

export const GODOT_BUNDLE_MEDIA_TYPE = "application/gzip";

/** Builds a Godot Web integration handoff over the canonical Engine and supported thin adapter. */
export async function buildGodotRuntimeBundle(workspace: StudioBrainWorkspace, assetFiles: readonly StudioAssetFile[]): Promise<RuntimeExportBundle> {
  const engine = await loadBundledEngineAssets();
  const compiler = await WasmCompilerBackend.instantiate(engine.engineModule);
  const artifact = compiler.compile(await compilerSourceEntries(workspace, assetFiles));
  const brainFile = `${safeStem(workspace.brain_id)}.gvya`;
  const bytes = await createTarGz([
    { path: brainFile, bytes: artifact },
    { path: "gvya-ffi.wasm", bytes: engine.engineWasm },
    { path: "GVYARuntime.gd", bytes: encoder.encode(godotRuntime) },
    { path: "gvya-godot-web.js", bytes: encoder.encode(godotWebBridge) },
    { path: "README.md", bytes: encoder.encode(bundleReadme(brainFile)) },
  ]);
  return {
    filename: `${safeStem(workspace.brain_id)}-godot-${STUDIO_ENGINE_VERSION}.tar.gz`,
    mediaType: GODOT_BUNDLE_MEDIA_TYPE,
    bytes,
  };
}

function bundleReadme(brainFile: string): string {
  return `# GVYA Godot runtime bundle

This handoff contains the compiled \`${brainFile}\`, canonical GVYA Engine ${STUDIO_ENGINE_VERSION} WASM, and the supported thin Godot Web adapter. It contains no alternative JavaScript runtime.

1. Copy \`GVYARuntime.gd\`, \`gvya-ffi.wasm\`, and \`${brainFile}\` into the Godot project. The adapter defaults to \`res://gvya-ffi.wasm\`.
2. Copy \`gvya-godot-web.js\` beside the Godot Web export and load it before the game starts:

   \`<script src="gvya-godot-web.js"></script>\`

3. Open the Bot from GDScript with an explicit trust policy:

   \`gvya.open_file("res://${brainFile}", GVYARuntime.unsigned_development_open_options())\`

The included artifact is an unsigned Studio development build. Production hosts must verify and require a signed artifact at their release boundary.
`;
}

function safeStem(value: string): string {
  return value.replace(/[^a-zA-Z0-9._-]+/gu, "-").replace(/^-+|-+$/gu, "") || "gvya-brain";
}
