import { rm } from "node:fs/promises";
import { resolve } from "node:path";

const root = resolve(import.meta.dirname, "..");
const targets = [
  "apps/studio/dist",
  "apps/studio/web-dist",
  "packages/runtime-sdk/dist",
  "target",
  "node_modules",
  // npm workspaces and Vite both create nested caches that are just as much build debris as the
  // root ones, and `validate-source.py` fails on any file under a generated directory.
  "apps/studio/node_modules",
  "packages/runtime-sdk/node_modules",
  "tools/__pycache__",
];

for (const relative of targets) {
  await rm(resolve(root, relative), {
    recursive: true,
    force: true,
    maxRetries: 3,
    retryDelay: 100,
  });
}
