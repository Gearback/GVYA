// Godot Export must be a deterministic integration handoff over the canonical Engine and adapter.
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { register } from "node:module";
import { gunzipSync } from "node:zlib";
import { fileURLToPath } from "node:url";
import { readContentSnapshot } from "../apps/studio/content-host.mjs";
import { decodeContent } from "../apps/studio/dist/studio-content.js";
import { STUDIO_ENGINE_VERSION } from "../apps/studio/dist/engine-assets.js";
import { resolveSelectedBrain } from "../apps/studio/dist/studio-model.js";
import { readTar } from "../apps/studio/dist/tar.js";

register("./raw-import-hooks.mjs", import.meta.url);
const { buildGodotRuntimeBundle, GODOT_BUNDLE_MEDIA_TYPE } = await import("../apps/studio/dist/godot-export.js");

globalThis.document = { baseURI: new URL("../apps/studio/public/", import.meta.url).href };
globalThis.fetch = async (input) => {
  const url = input instanceof URL ? new URL(input.href) : new URL(typeof input === "string" ? input : input.url);
  url.search = "";
  return new Response(await readFile(url), { status: 200 });
};

const snapshot = await readContentSnapshot(fileURLToPath(new URL("../content", import.meta.url)));
const content = decodeContent(snapshot.entries);
const brain = resolveSelectedBrain(content.workspace);
const bundle = await buildGodotRuntimeBundle(brain, content.assetFiles);

assert.equal(bundle.filename, `${brain.brain_id}-godot-${STUDIO_ENGINE_VERSION}.tar.gz`);
assert.equal(bundle.mediaType, GODOT_BUNDLE_MEDIA_TYPE);
assert.equal(bundle.bytes[0], 0x1f);
assert.equal(bundle.bytes[1], 0x8b);

const entries = readTar(new Uint8Array(gunzipSync(Buffer.from(bundle.bytes))), {
  maxEntries: 16,
  maxEntryBytes: 64 * 1024 * 1024,
  maxTotalBytes: 256 * 1024 * 1024,
});
const brainFile = `${brain.brain_id}.gvya`;
assert.deepEqual(entries.map((entry) => entry.path).sort(), [
  "GVYARuntime.gd",
  "README.md",
  brainFile,
  "gvya-ffi.wasm",
  "gvya-godot-web.js",
].sort());

const byPath = new Map(entries.map((entry) => [entry.path, entry.bytes]));
assert.deepEqual([...byPath.get(brainFile).subarray(0, 8)], [...Buffer.from("GVYA\r\n\x1a\n", "binary")]);
assert.deepEqual([...byPath.get("gvya-ffi.wasm").subarray(0, 4)], [0x00, 0x61, 0x73, 0x6d]);
assert.deepEqual(byPath.get("GVYARuntime.gd"), new Uint8Array(await readFile(new URL("../adapters/godot/GVYARuntime.gd", import.meta.url))));
assert.deepEqual(byPath.get("gvya-godot-web.js"), new Uint8Array(await readFile(new URL("../adapters/godot/web/gvya-godot-web.js", import.meta.url))));

const readme = new TextDecoder().decode(byPath.get("README.md"));
assert.match(readme, /unsigned Studio development build/u);
assert.match(readme, /gvya-godot-web\.js/u);
for (const forbidden of ["gvya.project.json", "package.json", "fragments/", "matcher-profiles/", "language-profiles/"]) {
  assert.ok(!entries.some((entry) => entry.path.includes(forbidden)), `Godot bundle must not carry ${forbidden}`);
}

const again = await buildGodotRuntimeBundle(brain, content.assetFiles);
assert.deepEqual(again.bytes, bundle.bytes, "Godot runtime bundle must be byte-reproducible");

console.log(`PASS Godot runtime export: ${bundle.filename} ${bundle.bytes.byteLength.toLocaleString()} bytes`);
