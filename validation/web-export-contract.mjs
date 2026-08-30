// Web Export is a distribution container, not a second meaning of `.gvya`.
//
// Proves: the exported bundle is a real gzip member, unpacks with standard tooling, carries the
// exact deployable file set, embeds the canonical runtime artifact unmodified, leaks no
// source/project/package content, and is materially smaller than the raw archive it replaced.
import assert from "node:assert/strict";
import { gunzipSync } from "node:zlib";
import { readFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { readContentSnapshot } from "../apps/studio/content-host.mjs";
import { decodeContent } from "../apps/studio/dist/studio-content.js";
import { resolveSelectedBrain } from "../apps/studio/dist/studio-model.js";
import { createTar, readTar } from "../apps/studio/dist/tar.js";
import { STUDIO_ENGINE_VERSION } from "../apps/studio/dist/engine-assets.js";
import { register } from "node:module";

// Studio inlines the SDK sources with Vite's `?raw` imports; teach this host to load them.
register("./raw-import-hooks.mjs", import.meta.url);

globalThis.document = { baseURI: new URL("../apps/studio/public/", import.meta.url).href };
globalThis.fetch = async (input) => {
  const url = input instanceof URL ? new URL(input.href) : new URL(typeof input === "string" ? input : input.url);
  url.search = "";
  return new Response(await readFile(url), { status: 200 });
};

const { buildWebRuntimeBundle, WEB_BUNDLE_MEDIA_TYPE } = await import("../apps/studio/dist/web-export.js");

const snapshot = await readContentSnapshot(fileURLToPath(new URL("../content", import.meta.url)));
const content = decodeContent(snapshot.entries);
const brain = resolveSelectedBrain(content.workspace);
const assetFiles = content.assetFiles;

const bundle = await buildWebRuntimeBundle(brain, assetFiles);

// Name and media type describe a compressed distribution container.
assert.equal(bundle.filename, `${brain.brain_id}-web-${STUDIO_ENGINE_VERSION}.tar.gz`);
assert.equal(bundle.mediaType, WEB_BUNDLE_MEDIA_TYPE);
assert.equal(WEB_BUNDLE_MEDIA_TYPE, "application/gzip");

// Real gzip framing with the deterministic zero MTIME, unpackable by standard tooling.
assert.equal(bundle.bytes[0], 0x1f);
assert.equal(bundle.bytes[1], 0x8b);
assert.equal(bundle.bytes[2], 0x08, "bundle must use the DEFLATE compression method");
assert.deepEqual([...bundle.bytes.subarray(4, 8)], [0, 0, 0, 0], "gzip MTIME must be zero for reproducible output");
const unpacked = gunzipSync(Buffer.from(bundle.bytes));
const entries = readTar(new Uint8Array(unpacked), { maxEntries: 64, maxEntryBytes: 64 * 1024 * 1024, maxTotalBytes: 256 * 1024 * 1024 });

// Every deployable file is present, and only those.
const brainFile = `${brain.brain_id}.gvya`;
assert.deepEqual(entries.map((entry) => entry.path).sort(), [
  "README.md",
  "app.js",
  `gvya-ffi-${STUDIO_ENGINE_VERSION}.wasm`,
  "index.html",
  "sdk/backend.js",
  "sdk/contracts.js",
  "sdk/index.js",
  "sdk/runtime.js",
  "sdk/wasm.js",
  brainFile,
].sort());

// `.gvya` stays the canonical runtime artifact: the container never rewrites it.
const artifact = entries.find((entry) => entry.path === brainFile).bytes;
assert.deepEqual([...artifact.subarray(0, 8)], [...Buffer.from("GVYA\r\n\x1a\n", "binary")], "the bundled Bot must be a canonical GVYA container");
const engineWasm = entries.find((entry) => entry.path === `gvya-ffi-${STUDIO_ENGINE_VERSION}.wasm`).bytes;
assert.deepEqual([...engineWasm.subarray(0, 4)], [0x00, 0x61, 0x73, 0x6d], "the bundled Engine must be a WebAssembly module");

// No source/project/package content leaks into a runtime distribution.
for (const forbidden of ["gvya.project.json", "package.json", "fragments/", "matcher-profiles/", "language-profiles/", "studio.json", "authoring.json"]) {
  assert.ok(!entries.some((entry) => entry.path.includes(forbidden)), `web bundle must not carry ${forbidden}`);
}
const textEntries = entries.filter((entry) => entry.path.endsWith(".md") || entry.path.endsWith(".js") || entry.path.endsWith(".html"));
const decoder = new TextDecoder();
for (const entry of textEntries) {
  assert.ok(!decoder.decode(entry.bytes).includes("gvya.source.project"), `${entry.path} must not embed canonical source`);
}

// Compression is real, not a renamed TAR.
const raw = createTar(entries.map((entry) => ({ path: entry.path, bytes: entry.bytes })));
assert.ok(bundle.bytes.byteLength < raw.byteLength, "the distribution container must actually compress");
const ratio = bundle.bytes.byteLength / raw.byteLength;
assert.ok(ratio < 0.75, `expected meaningful compression, got ratio ${ratio.toFixed(3)}`);

// Reproducible: the same input produces the same container bytes.
const again = await buildWebRuntimeBundle(brain, assetFiles);
assert.deepEqual([...again.bytes], [...bundle.bytes], "the distribution container must be byte-reproducible");

console.log(`PASS web export container: ${bundle.filename} ${bundle.bytes.byteLength.toLocaleString()} bytes (raw tar ${raw.byteLength.toLocaleString()}, ratio ${ratio.toFixed(3)})`);
