// The canonical portable source archive is a compressed, deterministic, round-trippable container.
//
// Proves: the export is a real gzip member with the canonical name and media type, standard tooling
// unpacks it, the decoded source tree is byte-identical to the compiler source entries it was built
// from, re-importing it reproduces the same Brain, and decompression is bounded.
import assert from "node:assert/strict";
import { gunzipSync, gzipSync } from "node:zlib";
import { readFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { readContentSnapshot } from "../apps/studio/content-host.mjs";
import { decodeContent, encodeContent } from "../apps/studio/dist/studio-content.js";
import { buildSelectedBotFolderZip, buildSelectedPackageFolderZip } from "../apps/studio/dist/folder-export.js";
import { resolveSelectedBrain } from "../apps/studio/dist/studio-model.js";
import { compilerSourceEntries } from "../apps/studio/dist/compiler-wasm.js";
import { createTar, gunzip, readTar, readTarGz } from "../apps/studio/dist/tar.js";
import {
  buildSourceArchive,
  loadSourceArchive,
  SOURCE_ARCHIVE_EXTENSION,
  SOURCE_ARCHIVE_MEDIA_TYPE,
} from "../apps/studio/dist/source-io.js";

globalThis.document = { baseURI: new URL("../apps/studio/public/", import.meta.url).href };
globalThis.fetch = async (input) => {
  const url = input instanceof URL ? new URL(input.href) : new URL(typeof input === "string" ? input : input.url);
  url.search = "";
  return new Response(await readFile(url), { status: 200 });
};

const snapshot = await readContentSnapshot(fileURLToPath(new URL("../content", import.meta.url)));
const content = decodeContent(snapshot.entries);
const brain = resolveSelectedBrain(content.workspace);
const sourceEntries = await compilerSourceEntries(brain, content.assetFiles);

const archiveDate = new Date(2026, 7, 28, 12, 0, 0);
const project = content.workspace.projects.find((row) => row.id === content.workspace.selectedProjectId);
const bot = project.bots.find((row) => row.id === content.workspace.selectedBotId);
content.workspace.selectedPackageScope = "bot";
content.workspace.selectedPackageId = bot.package.manifest.id;
const physicalEntries = await encodeContent(content.workspace, content.assetFiles);
const packageFolder = `projects/${project.id}/bots/${bot.id}/package`;
const packageZip = await buildSelectedPackageFolderZip(content.workspace, content.assetFiles, archiveDate);
assert.equal(packageZip.filename, `${bot.package.manifest.id}-2026-08-28.zip`);
assert.equal(packageZip.mediaType, "application/zip");
assertFolderZip(packageZip.bytes, physicalEntries, packageFolder, bot.package.manifest.id);
const botFolder = `projects/${project.id}/bots/${bot.id}`;
const botZip = await buildSelectedBotFolderZip(content.workspace, content.assetFiles, archiveDate);
assert.equal(botZip.filename, `${bot.id}-2026-08-28.zip`);
assert.equal(botZip.mediaType, "application/zip");
assertFolderZip(botZip.bytes, physicalEntries, botFolder, bot.id);
console.log(`PASS explicit dated Package/Bot folder ZIP exports: ${packageZip.filename}, ${botZip.filename}`);

const archive = await buildSourceArchive(brain, content.assetFiles);

// Canonical name and media type describe a compressed container.
assert.equal(SOURCE_ARCHIVE_EXTENSION, ".gvya-source.tar.gz");
assert.equal(SOURCE_ARCHIVE_MEDIA_TYPE, "application/gzip");
assert.equal(archive.filename, `${brain.brain_id}${SOURCE_ARCHIVE_EXTENSION}`);
assert.equal(archive.mediaType, SOURCE_ARCHIVE_MEDIA_TYPE);
assert.ok(!archive.filename.endsWith(".tar"), "the raw-TAR source export name is retired");

// Real gzip framing with the deterministic zero MTIME; standard tooling unpacks it.
assert.equal(archive.bytes[0], 0x1f);
assert.equal(archive.bytes[1], 0x8b);
assert.equal(archive.bytes[2], 0x08, "source archive must use the DEFLATE compression method");
assert.deepEqual([...archive.bytes.subarray(4, 8)], [0, 0, 0, 0], "gzip MTIME must be zero for reproducible output");
const unpacked = gunzipSync(Buffer.from(archive.bytes));
const limits = { maxEntries: 50_000, maxEntryBytes: 32 * 1024 * 1024, maxTotalBytes: 160 * 1024 * 1024 };
const entries = readTar(new Uint8Array(unpacked), limits);

// The decoded source is byte-identical to the compiler source it was built from.
const expected = new Map([...sourceEntries].map((entry) => [entry.path, entry.bytes]));
assert.equal(entries.length, expected.size, "the archive carries exactly the compiler source entries");
for (const entry of entries) {
  const want = expected.get(entry.path);
  assert.ok(want, `unexpected archive path ${entry.path}`);
  assert.deepEqual([...entry.bytes], [...want], `${entry.path} must decode byte-identically`);
}
assert.ok(entries.some((entry) => entry.path === "gvya.project.json"), "the archive is rooted at a canonical compiler source tree");

// The Studio reader agrees with standard tooling, and re-import reproduces the same Brain.
const viaStudio = await readTarGz(archive.bytes, limits);
assert.deepEqual(viaStudio.map((entry) => entry.path), entries.map((entry) => entry.path));
const reimported = await loadSourceArchive(new File([archive.bytes], archive.filename, { type: archive.mediaType }));
const reimportedEntries = await compilerSourceEntries(reimported.workspace, reimported.assetFiles);
assert.deepEqual(
  reimportedEntries.map((entry) => entry.path),
  sourceEntries.map((entry) => entry.path),
  "a re-imported archive rebuilds the same canonical source tree",
);
for (const [index, entry] of reimportedEntries.entries()) {
  assert.deepEqual([...entry.bytes], [...sourceEntries[index].bytes], `${entry.path} must survive an export/import round trip unchanged`);
}

// Deterministic: the same Brain produces the same container bytes.
const again = await buildSourceArchive(brain, content.assetFiles);
assert.deepEqual([...again.bytes], [...archive.bytes], "the source archive must be byte-reproducible");

// Compression is real, not a renamed TAR.
const raw = createTar(entries.map((entry) => ({ path: entry.path, bytes: entry.bytes })));
assert.ok(archive.bytes.byteLength < raw.byteLength, "the source archive must actually compress");
const ratio = archive.bytes.byteLength / raw.byteLength;
assert.ok(ratio < 0.5, `expected meaningful compression, got ratio ${ratio.toFixed(3)}`);

// Decompression is bounded: an archive that expands past the budget is refused, not materialized.
await assert.rejects(
  () => gunzip(new Uint8Array(gzipSync(Buffer.alloc(4 * 1024 * 1024))), 1024),
  /expands beyond the supported byte limit/u,
  "a compressed archive must not be allowed to expand without limit",
);
await assert.rejects(() => gunzip(archive.bytes, 0), /positive integer/u, "the decompression budget is mandatory");

console.log(`PASS source export container: ${archive.filename} ${archive.bytes.byteLength.toLocaleString()} bytes (raw tar ${raw.byteLength.toLocaleString()}, ratio ${ratio.toFixed(3)}, ${entries.length} files)`);

function assertFolderZip(bytes, physicalEntries, sourcePrefix, archiveRoot) {
  const actual = readStoredZip(bytes);
  const prefix = `${sourcePrefix}/`;
  const expected = new Map(physicalEntries.filter((entry) => entry.path.startsWith(prefix)).map((entry) => [`${archiveRoot}/${entry.path.slice(prefix.length)}`, new Uint8Array(Buffer.from(entry.bytes_base64, "base64"))]));
  assert.equal(actual.length, expected.size, `ZIP for ${sourcePrefix} must contain the complete folder and nothing else`);
  for (const entry of actual) {
    const wanted = expected.get(entry.path);
    assert.ok(wanted, `unexpected folder ZIP path ${entry.path}`);
    assert.deepEqual([...entry.bytes], [...wanted], `${entry.path} must preserve exact folder bytes`);
  }
  assert.ok(actual.some((entry) => entry.path.endsWith("/authoring.json")), `${archiveRoot} ZIP must carry Package authoring metadata`);
  assert.ok(actual.some((entry) => entry.path === `${archiveRoot}/package.json` || entry.path === `${archiveRoot}/bot.json`), `${archiveRoot} ZIP must carry its root identity file`);
}

function readStoredZip(bytes) {
  const rows = [];
  const decoder = new TextDecoder();
  let offset = 0;
  while (offset + 4 <= bytes.byteLength) {
    const view = new DataView(bytes.buffer, bytes.byteOffset + offset, bytes.byteLength - offset);
    const signature = view.getUint32(0, true);
    if (signature === 0x02014b50 || signature === 0x06054b50) break;
    assert.equal(signature, 0x04034b50, `invalid ZIP local header at ${offset}`);
    assert.equal(view.getUint16(8, true), 0, "folder ZIP entries must use the byte-preserving STORE method");
    const size = view.getUint32(18, true);
    assert.equal(size, view.getUint32(22, true));
    const nameLength = view.getUint16(26, true);
    const extraLength = view.getUint16(28, true);
    const nameStart = offset + 30;
    const dataStart = nameStart + nameLength + extraLength;
    const dataEnd = dataStart + size;
    assert.ok(dataEnd <= bytes.byteLength, "ZIP entry exceeds archive bounds");
    rows.push({ path: decoder.decode(bytes.subarray(nameStart, nameStart + nameLength)), bytes: bytes.slice(dataStart, dataEnd) });
    offset = dataEnd;
  }
  assert.equal(new DataView(bytes.buffer, bytes.byteOffset + offset, bytes.byteLength - offset).getUint32(0, true), 0x02014b50, "ZIP central directory is missing");
  assert.equal(new DataView(bytes.buffer, bytes.byteOffset + bytes.byteLength - 22, 22).getUint32(0, true), 0x06054b50, "ZIP end record is missing");
  return rows;
}
