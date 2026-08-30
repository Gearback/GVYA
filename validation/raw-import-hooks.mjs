// Node module hooks for Vite's `?raw` text imports.
//
// Studio bundles the SDK modules into a Web Export by importing their source text with Vite's
// `?raw` suffix. Node has no such convention, so validation registers these hooks to load the same
// text. This is a test-host shim only: it never changes what Studio ships.
import { readFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";

const RAW = "?raw";

export async function resolve(specifier, context, nextResolve) {
  if (!specifier.endsWith(RAW)) return nextResolve(specifier, context);
  const resolved = await nextResolve(specifier.slice(0, -RAW.length), context);
  return { ...resolved, url: `${resolved.url}${RAW}`, format: "module", shortCircuit: true };
}

export async function load(url, context, nextLoad) {
  if (!url.endsWith(RAW)) return nextLoad(url, context);
  const text = await readFile(fileURLToPath(new URL(url.slice(0, -RAW.length))), "utf8");
  return { format: "module", shortCircuit: true, source: `export default ${JSON.stringify(text)};` };
}
