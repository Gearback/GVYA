import sdkBackend from "../../../packages/runtime-sdk/dist/backend.js?raw";
import sdkContracts from "../../../packages/runtime-sdk/dist/contracts.js?raw";
import sdkIndex from "../../../packages/runtime-sdk/dist/index.js?raw";
import sdkRuntime from "../../../packages/runtime-sdk/dist/runtime.js?raw";
import sdkWasm from "../../../packages/runtime-sdk/dist/wasm.js?raw";
import { compilerSourceEntries, WasmCompilerBackend } from "./compiler-wasm.js";
import { loadBundledEngineAssets, STUDIO_ENGINE_VERSION } from "./engine-assets.js";
import { createTarGz } from "./tar.js";
import type { StudioAssetFile, StudioBrainWorkspace } from "./types.js";

const encoder = new TextEncoder();

export interface WebRuntimeBundle {
  filename: string;
  mediaType: string;
  bytes: Uint8Array;
}

export const WEB_BUNDLE_MEDIA_TYPE = "application/gzip";

/**
 * Builds the deployable web distribution container for the resolved Bot.
 *
 * `.gvya` remains the canonical runtime artifact and is placed into this container unchanged.
 * The container itself is an ordinary compressed `.tar.gz` that any standard tool can unpack; it
 * is a distribution format, never a second meaning of `.gvya`.
 */
export async function buildWebRuntimeBundle(workspace: StudioBrainWorkspace, assetFiles: readonly StudioAssetFile[]): Promise<WebRuntimeBundle> {
  const engine = await loadBundledEngineAssets();
  const compiler = await WasmCompilerBackend.instantiate(engine.engineModule);
  const artifact = compiler.compile(await compilerSourceEntries(workspace, assetFiles));
  const brainFile = `${safeStem(workspace.brain_id)}.gvya`;
  const bytes = await createTarGz([
    { path: brainFile, bytes: artifact },
    { path: `gvya-ffi-${STUDIO_ENGINE_VERSION}.wasm`, bytes: engine.engineWasm },
    { path: "sdk/backend.js", bytes: encoder.encode(sdkBackend) },
    { path: "sdk/contracts.js", bytes: encoder.encode(sdkContracts) },
    { path: "sdk/index.js", bytes: encoder.encode(sdkIndex) },
    { path: "sdk/runtime.js", bytes: encoder.encode(sdkRuntime) },
    { path: "sdk/wasm.js", bytes: encoder.encode(sdkWasm) },
    { path: "app.js", bytes: encoder.encode(exampleModule(brainFile)) },
    { path: "index.html", bytes: encoder.encode(exampleHtml(workspace.brain_id)) },
    { path: "README.md", bytes: encoder.encode(bundleReadme(brainFile)) },
  ]);
  return { filename: `${safeStem(workspace.brain_id)}-web-${STUDIO_ENGINE_VERSION}.tar.gz`, mediaType: WEB_BUNDLE_MEDIA_TYPE, bytes };
}

function exampleModule(brainFile: string): string { return `import { GvyaRuntime, WasmRuntimeBackend, unsignedDevelopmentOpenOptions } from "./sdk/index.js";

const [wasmBytes, artifactBytes] = await Promise.all([
  fetch("./gvya-ffi-${STUDIO_ENGINE_VERSION}.wasm").then(required).then((response) => response.arrayBuffer()),
  fetch("./${brainFile}").then(required).then((response) => response.arrayBuffer()),
]);
const backend = await WasmRuntimeBackend.instantiate(wasmBytes);
// This Studio-built artifact is unsigned. Production hosts should sign artifacts and use
// requireSignedArtifactOptions(), with cryptographic verification performed by the host.
const runtime = await GvyaRuntime.open(new Uint8Array(artifactBytes), backend, unsignedDevelopmentOpenOptions());
const info = await runtime.info();
document.querySelector("#status").textContent = \`Ready: \${info.project_id}/\${info.brain_id}\`;

function required(response) {
  if (!response.ok) throw new Error(\`HTTP \${response.status} for \${response.url}\`);
  return response;
}
`;
}

function exampleHtml(brainId: string): string { return `<!doctype html>
<html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width"><title>${escapeHtml(brainId)} · GVYA</title></head>
<body><main><h1>${escapeHtml(brainId)}</h1><p id="status">Loading canonical GVYA runtime…</p></main><script type="module" src="./app.js"></script></body></html>
`;
}

function bundleReadme(brainFile: string): string { return `# GVYA web runtime bundle

This self-contained handoff includes the compiled \`${brainFile}\`, the canonical Engine ${STUDIO_ENGINE_VERSION} WASM, the thin TypeScript SDK modules, and a minimal browser bootstrap.

Unpack the distribution container first (for example \`tar -xzf <bundle>.tar.gz\`), then serve this directory over HTTP (for example \`npx serve .\`) and open \`index.html\`. Browsers do not load WASM correctly from \`file://\` URLs.

The \`.tar.gz\` is only a distribution container. \`${brainFile}\` inside it is the canonical, unmodified GVYA runtime artifact.

The included artifact is an unsigned Studio development build. Before production deployment, sign the artifact at the host/release boundary, cryptographically verify its content root, and open it with \`requireSignedArtifactOptions()\`. Never replace the Rust runtime with JavaScript semantics.
`;
}

function safeStem(value: string): string { return value.replace(/[^a-zA-Z0-9._-]+/gu, "-").replace(/^-+|-+$/gu, "") || "gvya-brain"; }
function escapeHtml(value: string): string { return value.replaceAll("&", "&amp;").replaceAll("<", "&lt;").replaceAll(">", "&gt;").replaceAll('"', "&quot;"); }
