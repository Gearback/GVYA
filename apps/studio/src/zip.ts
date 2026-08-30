import { compareUtf8 } from "./canonical-order.js";

export interface ZipEntry {
  path: string;
  bytes: Uint8Array;
}

const ZIP_UTF8_FLAG = 0x0800;
const ZIP_STORE_METHOD = 0;
const MAX_U16 = 0xffff;
const MAX_U32 = 0xffff_ffff;

interface CentralEntry {
  name: Uint8Array;
  bytes: Uint8Array;
  crc: number;
  offset: number;
}

/** Creates a dependency-free ZIP archive using the byte-preserving STORE method. */
export function createZip(entries: readonly ZipEntry[], date = new Date()): Uint8Array {
  if (entries.length > MAX_U16) throw new Error(`ZIP entry count exceeds ${MAX_U16}.`);
  const ordered = [...entries].sort((left, right) => compareUtf8(left.path, right.path));
  const seen = new Set<string>();
  const localParts: Uint8Array[] = [];
  const centralEntries: CentralEntry[] = [];
  const { dosDate, dosTime } = zipDateTime(date);
  let offset = 0;

  for (const entry of ordered) {
    validateZipPath(entry.path);
    if (seen.has(entry.path)) throw new Error(`ZIP contains duplicate path ${entry.path}.`);
    seen.add(entry.path);
    if (entry.bytes.byteLength > MAX_U32) throw new Error(`ZIP entry ${entry.path} exceeds the classic ZIP size limit.`);
    const name = new TextEncoder().encode(entry.path);
    if (name.byteLength > MAX_U16) throw new Error(`ZIP path ${entry.path} is too long.`);
    const crc = crc32(entry.bytes);
    const header = new Uint8Array(30 + name.byteLength);
    const view = new DataView(header.buffer);
    writeU32(view, 0, 0x04034b50);
    writeU16(view, 4, 20);
    writeU16(view, 6, ZIP_UTF8_FLAG);
    writeU16(view, 8, ZIP_STORE_METHOD);
    writeU16(view, 10, dosTime);
    writeU16(view, 12, dosDate);
    writeU32(view, 14, crc);
    writeU32(view, 18, entry.bytes.byteLength);
    writeU32(view, 22, entry.bytes.byteLength);
    writeU16(view, 26, name.byteLength);
    writeU16(view, 28, 0);
    header.set(name, 30);
    centralEntries.push({ name, bytes: entry.bytes, crc, offset });
    localParts.push(header, entry.bytes);
    offset += header.byteLength + entry.bytes.byteLength;
    if (offset > MAX_U32) throw new Error("ZIP local entries exceed the classic ZIP size limit.");
  }

  const centralOffset = offset;
  const centralParts: Uint8Array[] = [];
  for (const entry of centralEntries) {
    const header = new Uint8Array(46 + entry.name.byteLength);
    const view = new DataView(header.buffer);
    writeU32(view, 0, 0x02014b50);
    writeU16(view, 4, 20);
    writeU16(view, 6, 20);
    writeU16(view, 8, ZIP_UTF8_FLAG);
    writeU16(view, 10, ZIP_STORE_METHOD);
    writeU16(view, 12, dosTime);
    writeU16(view, 14, dosDate);
    writeU32(view, 16, entry.crc);
    writeU32(view, 20, entry.bytes.byteLength);
    writeU32(view, 24, entry.bytes.byteLength);
    writeU16(view, 28, entry.name.byteLength);
    writeU16(view, 30, 0);
    writeU16(view, 32, 0);
    writeU16(view, 34, 0);
    writeU16(view, 36, 0);
    writeU32(view, 38, 0);
    writeU32(view, 42, entry.offset);
    header.set(entry.name, 46);
    centralParts.push(header);
    offset += header.byteLength;
    if (offset > MAX_U32) throw new Error("ZIP central directory exceeds the classic ZIP size limit.");
  }

  const centralSize = offset - centralOffset;
  const end = new Uint8Array(22);
  const endView = new DataView(end.buffer);
  writeU32(endView, 0, 0x06054b50);
  writeU16(endView, 4, 0);
  writeU16(endView, 6, 0);
  writeU16(endView, 8, centralEntries.length);
  writeU16(endView, 10, centralEntries.length);
  writeU32(endView, 12, centralSize);
  writeU32(endView, 16, centralOffset);
  writeU16(endView, 20, 0);
  return concatBytes([...localParts, ...centralParts, end], offset + end.byteLength);
}

function zipDateTime(date: Date): { dosDate: number; dosTime: number } {
  if (!Number.isFinite(date.getTime())) throw new Error("ZIP date is invalid.");
  const year = Math.min(2107, Math.max(1980, date.getFullYear()));
  const month = date.getMonth() + 1;
  const day = date.getDate();
  return { dosDate: ((year - 1980) << 9) | (month << 5) | day, dosTime: 0 };
}

function validateZipPath(path: string): void {
  if (!path || path.length > 512 || path.startsWith("/") || path.includes("\\") || path.includes("\0") || path.split("/").some((part) => !part || part === "." || part === "..")) throw new Error(`ZIP path is unsafe: ${path}`);
}

function concatBytes(parts: readonly Uint8Array[], size: number): Uint8Array {
  const output = new Uint8Array(size);
  let offset = 0;
  for (const part of parts) { output.set(part, offset); offset += part.byteLength; }
  return output;
}

function writeU16(view: DataView, offset: number, value: number): void { view.setUint16(offset, value, true); }
function writeU32(view: DataView, offset: number, value: number): void { view.setUint32(offset, value >>> 0, true); }

const CRC32_TABLE = new Uint32Array(256).map((_, index) => {
  let value = index;
  for (let bit = 0; bit < 8; bit += 1) value = (value & 1) !== 0 ? 0xedb88320 ^ (value >>> 1) : value >>> 1;
  return value >>> 0;
});

function crc32(bytes: Uint8Array): number {
  let value = 0xffff_ffff;
  for (const byte of bytes) value = CRC32_TABLE[(value ^ byte) & 0xff]! ^ (value >>> 8);
  return (value ^ 0xffff_ffff) >>> 0;
}
