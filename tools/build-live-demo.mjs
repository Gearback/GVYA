import { cp, mkdir, readFile, rm, writeFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { readContentSnapshot } from "../apps/studio/content-host.mjs";
import { decodeContent } from "../apps/studio/dist/studio-content.js";
import { resolveSelectedBrain } from "../apps/studio/dist/studio-model.js";
import { compilerSourceEntries, WasmCompilerBackend } from "../apps/studio/dist/compiler-wasm.js";

const out = new URL("../.pages-dist/", import.meta.url);
await rm(out, { recursive: true, force: true });
await mkdir(out, { recursive: true });

const snapshot = await readContentSnapshot(fileURLToPath(new URL("../content/", import.meta.url)));
const content = decodeContent(snapshot.entries);
const brain = resolveSelectedBrain(content.workspace);
const entries = await compilerSourceEntries(brain, content.assetFiles);
const wasm = await readFile(new URL("../apps/studio/public/engine/v1/gvya-ffi.wasm", import.meta.url));
const compiler = await WasmCompilerBackend.instantiate(wasm);
compiler.validate(entries);
const artifact = compiler.compile(entries);

await cp(new URL("../web/live-demo/", import.meta.url), out, { recursive: true });
await cp(new URL("../packages/runtime-sdk/dist/", import.meta.url), new URL("sdk/", out), { recursive: true });

const exactIconBase64 = await readFile(new URL("../web/live-demo/exact-icon.png.b64", import.meta.url), "utf8");
const exactFaviconBase64 = await readFile(new URL("../web/live-demo/exact-favicon.ico.b64", import.meta.url), "utf8");
await writeFile(new URL("gvya-icon.png", out), Buffer.from(exactIconBase64.trim(), "base64"));
await writeFile(new URL("favicon.ico", out), Buffer.from(exactFaviconBase64.trim(), "base64"));
await rm(new URL("exact-icon.png.b64", out), { force: true });
await rm(new URL("exact-favicon.ico.b64", out), { force: true });

await writeFile(new URL("gvya-ffi-v1.wasm", out), wasm);
await writeFile(new URL("gvya-bot.gvya", out), artifact);
await writeFile(new URL(".nojekyll", out), "");

console.log(`Built live demo: ${artifact.byteLength} byte GVYA brain`);
