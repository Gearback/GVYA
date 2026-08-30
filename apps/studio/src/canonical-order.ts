const encoder = new TextEncoder();

/** Rust-compatible UTF-8 byte ordering for deterministic source/archive identities. */
export function compareUtf8(left: string, right: string): number {
  if (left === right) return 0;
  const a = encoder.encode(left);
  const b = encoder.encode(right);
  const length = Math.min(a.byteLength, b.byteLength);
  for (let index = 0; index < length; index += 1) {
    const delta = (a[index] ?? 0) - (b[index] ?? 0);
    if (delta !== 0) return delta;
  }
  return a.byteLength - b.byteLength;
}

/** BCP-47 language tags are ASCII by contract; never use the ambient browser locale. */
export function normalizeLanguageTag(value: string): string {
  return value.trim().replaceAll("_", "-").replace(/[A-Z]/gu, (char) => char.toLowerCase());
}
