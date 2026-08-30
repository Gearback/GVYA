import { compareUtf8 } from "./canonical-order.js";
const BLOCK_BYTES = 512;
const encoder = new TextEncoder();
const decoder = new TextDecoder("utf-8", { fatal: true });

export interface TarEntry {
  path: string;
  bytes: Uint8Array;
}

export interface TarLimits {
  maxEntries: number;
  maxEntryBytes: number;
  maxTotalBytes: number;
}

export function createTar(entries: readonly TarEntry[]): Uint8Array {
  const normalized = normalizeEntries(entries);
  const total = normalized.reduce((sum, entry) => sum + BLOCK_BYTES + padded(entry.bytes.byteLength), BLOCK_BYTES * 2);
  const output = new Uint8Array(total);
  let offset = 0;
  for (const entry of normalized) {
    const header = output.subarray(offset, offset + BLOCK_BYTES);
    const { name, prefix } = splitUstarPath(entry.path);
    writeText(header, 0, 100, name);
    writeOctal(header, 100, 8, 0o644);
    writeOctal(header, 108, 8, 0);
    writeOctal(header, 116, 8, 0);
    writeOctal(header, 124, 12, entry.bytes.byteLength);
    writeOctal(header, 136, 12, 0);
    header.fill(0x20, 148, 156);
    header[156] = 0x30;
    writeText(header, 257, 6, "ustar\0");
    writeText(header, 263, 2, "00");
    writeText(header, 345, 155, prefix);
    const checksum = header.reduce((sum, byte) => sum + byte, 0);
    writeChecksum(header, checksum);
    offset += BLOCK_BYTES;
    output.set(entry.bytes, offset);
    offset += padded(entry.bytes.byteLength);
  }
  return output;
}

/**
 * Deterministic ustar stream wrapped in one gzip member.
 *
 * The archived bytes are fully reproducible: entries are path-sorted and every ustar header field
 * (mode, uid, gid, mtime) is fixed. The gzip framing comes from the platform `CompressionStream`,
 * which writes a zero MTIME, so a given engine reproduces identical bytes for identical input.
 */
export async function createTarGz(entries: readonly TarEntry[]): Promise<Uint8Array> {
  return gzip(createTar(entries));
}

export async function gzip(bytes: Uint8Array): Promise<Uint8Array> {
  return collect(new Blob([bytes as BlobPart]).stream().pipeThrough(new CompressionStream("gzip")), Number.POSITIVE_INFINITY);
}

/**
 * Bounded gzip decompression. The budget is mandatory: a small archive can expand without limit,
 * so the stream is abandoned as soon as it exceeds what the caller is prepared to hold.
 */
export async function gunzip(bytes: Uint8Array, maxBytes: number): Promise<Uint8Array> {
  if (!Number.isSafeInteger(maxBytes) || maxBytes <= 0) throw new Error("Decompression budget must be a positive integer byte count.");
  return collect(new Blob([bytes as BlobPart]).stream().pipeThrough(new DecompressionStream("gzip")), maxBytes);
}

/** Reads one gzip-compressed deterministic ustar stream under the same bounds as the raw reader. */
export async function readTarGz(bytes: Uint8Array, limits: TarLimits): Promise<TarEntry[]> {
  return readTar(await gunzip(bytes, limits.maxTotalBytes), limits);
}

async function collect(stream: ReadableStream<Uint8Array>, maxBytes: number): Promise<Uint8Array> {
  const chunks: Uint8Array[] = [];
  let total = 0;
  const reader = stream.getReader();
  for (;;) {
    const { done, value } = await reader.read();
    if (done) break;
    total += value.byteLength;
    if (total > maxBytes) {
      await reader.cancel();
      throw new Error("Compressed archive expands beyond the supported byte limit.");
    }
    chunks.push(value);
  }
  const out = new Uint8Array(total);
  let offset = 0;
  for (const chunk of chunks) { out.set(chunk, offset); offset += chunk.byteLength; }
  return out;
}

export function readTar(bytes: Uint8Array, limits: TarLimits): TarEntry[] {
  if (bytes.byteLength > limits.maxTotalBytes) throw new Error("Archive exceeds the supported byte limit.");
  if (bytes.byteLength < BLOCK_BYTES * 2 || bytes.byteLength % BLOCK_BYTES !== 0) throw new Error("Archive is not a complete ustar stream.");
  const entries: TarEntry[] = [];
  const paths = new Set<string>();
  let offset = 0;
  let payloadBytes = 0;
  while (offset + BLOCK_BYTES <= bytes.byteLength) {
    const header = bytes.subarray(offset, offset + BLOCK_BYTES);
    if (header.every((byte) => byte === 0)) break;
    if (readText(header, 257, 6) !== "ustar") throw new Error("Archive entry is not ustar.");
    verifyChecksum(header);
    const type = header[156];
    if (!(type === 0 || type === 0x30)) throw new Error("Archive contains a non-file entry.");
    const name = readText(header, 0, 100);
    const prefix = readText(header, 345, 155);
    const path = normalizeSafePath(prefix ? `${prefix}/${name}` : name);
    if (paths.has(path)) throw new Error(`Archive contains duplicate path ${path}.`);
    paths.add(path);
    const size = readOctal(header, 124, 12);
    if (size > limits.maxEntryBytes) throw new Error(`Archive entry ${path} exceeds its byte limit.`);
    payloadBytes += size;
    if (!Number.isSafeInteger(payloadBytes) || payloadBytes > limits.maxTotalBytes) throw new Error("Archive payload exceeds the supported byte limit.");
    const start = offset + BLOCK_BYTES;
    const end = start + size;
    if (end > bytes.byteLength) throw new Error(`Archive entry ${path} is truncated.`);
    entries.push({ path, bytes: bytes.slice(start, end) });
    if (entries.length > limits.maxEntries) throw new Error("Archive contains too many files.");
    offset = start + padded(size);
  }
  if (entries.length === 0) throw new Error("Archive contains no files.");
  return entries;
}

export function normalizeSafePath(path: string): string {
  const normalized = path.replaceAll("\\", "/").replace(/^\.\//u, "");
  if (!normalized || normalized.startsWith("/") || normalized.endsWith("/") || normalized.includes("\0")) throw new Error(`Unsafe archive path: ${path}`);
  const parts = normalized.split("/");
  if (parts.some((part) => !part || part === "." || part === "..")) throw new Error(`Unsafe archive path: ${path}`);
  if (encoder.encode(normalized).byteLength > 255) throw new Error(`Archive path is too long: ${path}`);
  return normalized;
}

function normalizeEntries(entries: readonly TarEntry[]): TarEntry[] {
  const paths = new Set<string>();
  return entries.map((entry) => {
    const path = normalizeSafePath(entry.path);
    if (paths.has(path)) throw new Error(`Archive contains duplicate path ${path}.`);
    paths.add(path);
    return { path, bytes: entry.bytes };
  }).sort((left, right) => compareUtf8(left.path, right.path));
}

function splitUstarPath(path: string): { name: string; prefix: string } {
  if (encoder.encode(path).byteLength <= 100) return { name: path, prefix: "" };
  for (let index = path.lastIndexOf("/"); index > 0; index = path.lastIndexOf("/", index - 1)) {
    const prefix = path.slice(0, index);
    const name = path.slice(index + 1);
    if (encoder.encode(prefix).byteLength <= 155 && encoder.encode(name).byteLength <= 100) return { name, prefix };
  }
  throw new Error(`Archive path cannot be represented by ustar: ${path}`);
}

function padded(size: number): number { return Math.ceil(size / BLOCK_BYTES) * BLOCK_BYTES; }

function writeText(target: Uint8Array, offset: number, width: number, value: string): void {
  const bytes = encoder.encode(value);
  if (bytes.byteLength > width) throw new Error("ustar text field overflow.");
  target.set(bytes, offset);
}

function readText(source: Uint8Array, offset: number, width: number): string {
  const field = source.subarray(offset, offset + width);
  const end = field.indexOf(0);
  return decoder.decode(end < 0 ? field : field.subarray(0, end));
}

function writeOctal(target: Uint8Array, offset: number, width: number, value: number): void {
  const text = value.toString(8).padStart(width - 1, "0");
  if (text.length >= width) throw new Error("ustar numeric field overflow.");
  writeText(target, offset, width, `${text}\0`);
}

function readOctal(source: Uint8Array, offset: number, width: number): number {
  const text = readText(source, offset, width).trim();
  if (!/^[0-7]+$/u.test(text)) throw new Error("Archive contains an invalid ustar numeric field.");
  const value = Number.parseInt(text, 8);
  if (!Number.isSafeInteger(value) || value < 0) throw new Error("Archive contains an unsupported ustar numeric value.");
  return value;
}

function writeChecksum(target: Uint8Array, checksum: number): void {
  const text = checksum.toString(8).padStart(6, "0");
  writeText(target, 148, 8, `${text}\0 `);
}

function verifyChecksum(header: Uint8Array): void {
  const expected = readOctal(header, 148, 8);
  let actual = 0;
  for (let index = 0; index < header.length; index += 1) actual += index >= 148 && index < 156 ? 0x20 : header[index]!;
  if (actual !== expected) throw new Error("Archive entry checksum is invalid.");
}
