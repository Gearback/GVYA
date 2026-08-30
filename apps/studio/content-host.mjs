import { createHash, randomUUID } from "node:crypto";
import { promises as fs } from "node:fs";
import path from "node:path";

export const STUDIO_CONTENT_API = "/api/gvya-content";

const MAX_FILE_COUNT = 10_000;
const MAX_FILE_BYTES = 32 * 1024 * 1024;
const MAX_TOTAL_BYTES = 128 * 1024 * 1024;
const MAX_REQUEST_BYTES = 180 * 1024 * 1024;

export function gvyaContentPlugin(contentRoot) {
  const root = path.resolve(contentRoot);
  const configure = (server) => {
    server.middlewares.use(async (request, response, next) => {
      const pathname = request.url?.split("?", 1)[0];
      if (pathname !== STUDIO_CONTENT_API) {
        next();
        return;
      }
      try {
        if (request.method === "GET") {
          sendJson(response, 200, await readContentSnapshot(root));
          return;
        }
        if (request.method === "PUT") {
          const body = await readJsonBody(request);
          const saved = await writeContentSnapshot(root, body);
          sendJson(response, 200, saved);
          return;
        }
        response.setHeader("Allow", "GET, PUT");
        sendJson(response, 405, { error: "Method not allowed." });
      } catch (error) {
        const status = error instanceof ContentConflictError ? 409 : 400;
        sendJson(response, status, { error: error instanceof Error ? error.message : String(error) });
      }
    });
  };
  return {
    name: "gvya-content-host",
    configureServer: configure,
    configurePreviewServer: configure,
  };
}

export async function readContentSnapshot(contentRoot) {
  const root = path.resolve(contentRoot);
  const entries = [];
  await walkContentRoot(root, "", entries);
  entries.sort((left, right) => Buffer.from(left.path).compare(Buffer.from(right.path)));
  enforceSnapshotLimits(entries);
  return { format: "gvya.studio.content-snapshot", version: 1, revision: revisionFor(entries), entries };
}

export async function writeContentSnapshot(contentRoot, input) {
  const root = path.resolve(contentRoot);
  const snapshot = parseSnapshot(input);
  const current = await readContentSnapshot(root);
  if (snapshot.revision !== current.revision) {
    throw new ContentConflictError("Studio content changed on disk. Reload before saving so dropped-in files are not overwritten.");
  }

  const parent = path.dirname(root);
  const basename = path.basename(root);
  const nonce = `${process.pid}-${randomUUID()}`;
  const stage = path.join(parent, `.${basename}.stage-${nonce}`);
  const previous = path.join(parent, `.${basename}.previous-${nonce}`);
  await fs.mkdir(stage, { recursive: false });
  try {
    for (const entry of snapshot.entries) {
      const target = path.join(stage, ...entry.path.split("/"));
      await fs.mkdir(path.dirname(target), { recursive: true });
      await fs.writeFile(target, Buffer.from(entry.bytes_base64, "base64"), { flag: "wx" });
    }

    let movedCurrent = false;
    try {
      await fs.rename(root, previous);
      movedCurrent = true;
    } catch (error) {
      if (!isMissing(error)) throw error;
    }
    try {
      await fs.rename(stage, root);
    } catch (error) {
      if (movedCurrent) await fs.rename(previous, root);
      throw error;
    }
    if (movedCurrent) await fs.rm(previous, { recursive: true, force: true });
    return await readContentSnapshot(root);
  } finally {
    await fs.rm(stage, { recursive: true, force: true });
  }
}

async function walkContentRoot(root, relative, entries) {
  const current = relative ? path.join(root, ...relative.split("/")) : root;
  let rows;
  try {
    rows = await fs.readdir(current, { withFileTypes: true });
  } catch (error) {
    if (isMissing(error) && relative === "") return;
    throw error;
  }
  rows.sort((left, right) => Buffer.from(left.name).compare(Buffer.from(right.name)));
  for (const row of rows) {
    if (row.isSymbolicLink()) throw new Error(`Studio content may not contain symbolic links: ${joinRelative(relative, row.name)}`);
    const child = joinRelative(relative, row.name);
    if (row.isDirectory()) {
      await walkContentRoot(root, child, entries);
      continue;
    }
    if (!row.isFile()) throw new Error(`Studio content contains an unsupported filesystem entry: ${child}`);
    const bytes = await fs.readFile(path.join(root, ...child.split("/")));
    if (bytes.byteLength > MAX_FILE_BYTES) throw new Error(`Studio content file exceeds 32 MiB: ${child}`);
    entries.push({ path: child, bytes_base64: bytes.toString("base64") });
    if (entries.length > MAX_FILE_COUNT) throw new Error("Studio content exceeds the 10,000-file limit.");
  }
}

function parseSnapshot(input) {
  const row = record(input, "Studio content snapshot");
  exactKeys(row, ["format", "version", "revision", "entries"], "Studio content snapshot");
  if (row.format !== "gvya.studio.content-snapshot" || row.version !== 1) throw new Error("Unsupported Studio content snapshot.");
  if (typeof row.revision !== "string" || !/^[a-f0-9]{64}$/.test(row.revision)) throw new Error("Studio content revision is invalid.");
  if (!Array.isArray(row.entries)) throw new Error("Studio content entries must be an array.");
  const paths = new Set();
  const entries = row.entries.map((value, index) => {
    const entry = record(value, `Studio content entry ${index}`);
    exactKeys(entry, ["path", "bytes_base64"], `Studio content entry ${index}`);
    if (typeof entry.path !== "string" || !safeContentPath(entry.path)) throw new Error(`Studio content entry ${index} has an unsafe path.`);
    if (paths.has(entry.path)) throw new Error(`Duplicate Studio content path: ${entry.path}`);
    paths.add(entry.path);
    if (typeof entry.bytes_base64 !== "string" || !strictBase64(entry.bytes_base64)) throw new Error(`Studio content entry ${entry.path} has invalid bytes.`);
    const byteLength = Buffer.byteLength(entry.bytes_base64, "base64");
    if (byteLength > MAX_FILE_BYTES) throw new Error(`Studio content file exceeds 32 MiB: ${entry.path}`);
    return { path: entry.path, bytes_base64: entry.bytes_base64 };
  });
  entries.sort((left, right) => Buffer.from(left.path).compare(Buffer.from(right.path)));
  enforceSnapshotLimits(entries);
  return { revision: row.revision, entries };
}

function enforceSnapshotLimits(entries) {
  if (entries.length > MAX_FILE_COUNT) throw new Error("Studio content exceeds the 10,000-file limit.");
  let total = 0;
  for (const entry of entries) {
    total += Buffer.byteLength(entry.bytes_base64, "base64");
    if (!Number.isSafeInteger(total) || total > MAX_TOTAL_BYTES) throw new Error("Studio content exceeds the 128 MiB limit.");
  }
}

function revisionFor(entries) {
  const hash = createHash("sha256");
  for (const entry of entries) {
    hash.update(entry.path, "utf8");
    hash.update("\0");
    hash.update(entry.bytes_base64, "base64");
    hash.update("\0");
  }
  return hash.digest("hex");
}

function safeContentPath(value) {
  if (!value || value.length > 512 || value.startsWith("/") || value.includes("\\") || value.includes("\0")) return false;
  return value.split("/").every((part) => part !== "" && part !== "." && part !== "..");
}

function strictBase64(value) {
  if (value.length % 4 !== 0 || !/^(?:[A-Za-z0-9+/]{4})*(?:[A-Za-z0-9+/]{2}==|[A-Za-z0-9+/]{3}=)?$/.test(value)) return false;
  return Buffer.from(value, "base64").toString("base64") === value;
}

async function readJsonBody(request) {
  const chunks = [];
  let total = 0;
  for await (const chunk of request) {
    total += chunk.byteLength;
    if (total > MAX_REQUEST_BYTES) throw new Error("Studio content request exceeds the 180 MiB limit.");
    chunks.push(chunk);
  }
  let parsed;
  try {
    parsed = JSON.parse(Buffer.concat(chunks).toString("utf8"));
  } catch {
    throw new Error("Studio content request is not valid JSON.");
  }
  return parsed;
}

function joinRelative(parent, child) { return parent ? `${parent}/${child}` : child; }
function isMissing(error) { return error && typeof error === "object" && "code" in error && error.code === "ENOENT"; }
function record(value, label) { if (!value || typeof value !== "object" || Array.isArray(value)) throw new Error(`${label} must be an object.`); return value; }
function exactKeys(value, keys, label) { const actual = Object.keys(value).sort(); const expected = [...keys].sort(); if (JSON.stringify(actual) !== JSON.stringify(expected)) throw new Error(`${label} has unsupported or missing fields.`); }
function sendJson(response, status, value) { response.statusCode = status; response.setHeader("Content-Type", "application/json; charset=utf-8"); response.setHeader("Cache-Control", "no-store"); response.end(JSON.stringify(value)); }

class ContentConflictError extends Error {}
